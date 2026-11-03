//! Robustesse du lecteur MSPDI (règle 10, ADR-0009 § 7.8) : aucune panique
//! sur une entrée tronquée, mutée ou hostile ; erreurs structurelles typées,
//! tout le reste en diagnostics.
//!
//! Entrées : fixtures écrites à la main (`tests/fixtures/mspdi/`) et, s'ils
//! sont présents, les fichiers du corpus public (`tests/corpus/files/`, non
//! versionnés, téléchargés par `cargo xtask corpus fetch`).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use plannercraft_mspdi::{ImportCode, Limits, MspdiError, ReadOptions, read};
use proptest::prelude::*;

fn opts() -> ReadOptions {
    ReadOptions::new("2026-01-01T00:00".parse().unwrap())
}

fn repo(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(relative)
}

/// Fichiers `.xml` d'un dossier, triés par nom ; dossier absent : aucun.
fn xml_files(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<(String, Vec<u8>)> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("xml"))
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            (name, std::fs::read(&p).unwrap())
        })
        .collect();
    out.sort();
    out
}

fn fixtures() -> Vec<(String, Vec<u8>)> {
    let out = xml_files(&repo("tests/fixtures/mspdi"));
    assert!(!out.is_empty(), "aucune fixture MSPDI");
    out
}

/// Petites fixtures (moins de 8 Kio) pour les mutations.
fn small_fixtures() -> Vec<Vec<u8>> {
    fixtures()
        .into_iter()
        .map(|(_, b)| b)
        .filter(|b| b.len() < 8 * 1024)
        .collect()
}

#[test]
fn fixtures_read_cleanly() {
    for (name, bytes) in fixtures() {
        let doc = read(&bytes, &opts()).unwrap_or_else(|e| panic!("{name} : {e}"));
        assert_eq!(
            doc.source.namespace.as_deref(),
            Some("http://schemas.microsoft.com/project"),
            "{name}"
        );
    }
}

#[test]
fn input_guards() {
    assert_eq!(read(b"", &opts()), Err(MspdiError::Empty));
    // BOM seul : vide une fois le BOM retiré.
    assert_eq!(read(&[0xEF, 0xBB, 0xBF], &opts()), Err(MspdiError::Empty));
    assert_eq!(
        read(&[0xFF, 0xFE, b'<', 0], &opts()),
        Err(MspdiError::Utf16NotSupported)
    );
    assert_eq!(
        read(&[0xFE, 0xFF, 0, b'<'], &opts()),
        Err(MspdiError::Utf16NotSupported)
    );
    match read(b"<Project>\n  <Name>\xC3\x28</Name></Project>", &opts()) {
        Err(MspdiError::InvalidUtf8 { at }) => {
            assert_eq!((at.byte, at.line, at.column), (18, 2, 9));
        }
        other => panic!("{other:?}"),
    }
    let limits = Limits {
        max_bytes: 10,
        ..Limits::default()
    };
    assert_eq!(
        read(b"<Project></Project>", &ReadOptions { limits, ..opts() }),
        Err(MspdiError::TooLarge { size: 19, max: 10 })
    );
    // BOM UTF-8 suivi d'un document : accepté.
    let mut with_bom = vec![0xEF, 0xBB, 0xBF];
    with_bom.extend_from_slice(b"<Project><StartDate>2026-10-19T08:00:00</StartDate></Project>");
    assert!(read(&with_bom, &opts()).is_ok());
}

#[test]
fn hostile_documents() {
    assert_eq!(
        read(b"<Projet/>", &opts()),
        Err(MspdiError::NotMspdi {
            root: "Projet".to_owned()
        })
    );
    let doctype = b"<?xml version=\"1.0\"?><!DOCTYPE Project [<!ENTITY a \"aaaaaaaaaa\"><!ENTITY b \"&a;&a;&a;\">]><Project>&b;</Project>";
    assert!(matches!(
        read(doctype, &opts()),
        Err(MspdiError::DoctypeNotAllowed { .. })
    ));
    let deep = format!("<Project>{}", "<a>".repeat(100_000));
    assert!(matches!(
        read(deep.as_bytes(), &opts()),
        Err(MspdiError::TooDeep { .. })
    ));
    let long = format!("<Project><Name>{}</Name></Project>", "x".repeat(20 * 1024 * 1024));
    assert!(matches!(
        read(long.as_bytes(), &opts()),
        Err(MspdiError::TextTooLong { .. })
    ));
}

#[test]
fn depth_limit_set_to_the_maximum_cannot_overflow_the_stack() {
    // Relecture J2 : `max_depth` réglé à u16::MAX et 65 000 éléments
    // imbriqués dans une tâche faisaient déborder la pile (libération
    // récursive de l'arbre). La profondeur est plafonnée à MAX_DEPTH, et les
    // enfants inconnus d'une tâche ne sont plus construits : lecture sur un
    // fil de 2 Mio, erreur typée.
    let handle = std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let limits = Limits {
                max_depth: u16::MAX,
                ..Limits::default()
            };
            let xml = format!(
                "<Project><Tasks><Task><UID>1</UID>{}{}</Task></Tasks></Project>",
                "<a>".repeat(65_000),
                "</a>".repeat(65_000)
            );
            read(xml.as_bytes(), &ReadOptions { limits, ..opts() })
        })
        .unwrap();
    assert!(matches!(
        handle.join().unwrap(),
        Err(MspdiError::TooDeep {
            max: plannercraft_mspdi::MAX_DEPTH,
            ..
        })
    ));
}

#[test]
fn unknown_task_children_are_not_kept_without_extras() {
    // Relecture J2 (bloquant) : tout enfant d'une tâche était construit en
    // mémoire. Sans `Extras` (`keep_extras` faux), deux millions d'enfants
    // inconnus sont sautés et comptés ; la tâche se lit normalement.
    let n = 2_000_000;
    let xml = format!(
        "<Project><StartDate>2026-10-19T08:00:00</StartDate><Tasks><Task><UID>1</UID><Name>t</Name>\
         <Duration>PT8H0M0S</Duration>{}</Task></Tasks></Project>",
        "<X/>".repeat(n)
    );
    let options = ReadOptions {
        keep_extras: false,
        ..opts()
    };
    let doc = read(xml.as_bytes(), &options).unwrap();
    assert_eq!(doc.project.tasks.len(), 1);
    assert!(doc.project.tasks.at(0).unwrap().extras.is_empty());
    assert_eq!(doc.source.skipped_elements, u64::try_from(n).unwrap());
}

#[test]
fn r087_many_unknown_task_children_are_kept_in_linear_time() {
    // Avec `Extras` (défaut), chaque inconnu est capturé (une capture
    // compte pour un élément retenu) ; clés calculées en un parcours.
    let n = 200_000;
    let xml = format!(
        "<Project><StartDate>2026-10-19T08:00:00</StartDate><Tasks><Task><UID>1</UID><Name>t</Name>\
         <Duration>PT8H0M0S</Duration>{}</Task></Tasks></Project>",
        "<X/>".repeat(n)
    );
    let doc = read(xml.as_bytes(), &opts()).unwrap();
    let extras = &doc.project.tasks.at(0).unwrap().extras;
    assert_eq!(extras.0.len(), n);
    // `Duration` est M? (absente selon le rôle de la tâche) : l'ancre est
    // `Name`, le dernier élément toujours réécrit.
    assert!(extras.0.iter().all(|e| e.key == "Name>X" && e.payload == "<X/>"));
    assert_eq!(doc.source.skipped_elements, 0);
}

#[test]
fn import_diagnostics_are_capped() {
    // Relecture J2 : un diagnostic par anomalie, sans borne. Un million de
    // tâches à l'UID en double : 1 000 diagnostics `mspdi.duplicate` gardés,
    // les autres comptés et signalés par `mspdi.truncated`.
    let n = 1_000_000;
    let xml = format!(
        "<Project><StartDate>2026-10-19T08:00:00</StartDate><Tasks>{}</Tasks></Project>",
        "<Task><UID>1</UID></Task>".repeat(n)
    );
    let doc = read(xml.as_bytes(), &opts()).unwrap();
    let duplicates = doc
        .diagnostics
        .iter()
        .filter(|d| d.code == ImportCode::Duplicate)
        .count();
    assert_eq!(duplicates, 1_000);
    let dropped = u64::try_from(n - 1 - 1_000).unwrap();
    assert_eq!(doc.source.dropped_diagnostics, dropped);
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.code == ImportCode::Truncated { dropped })
    );
    assert!(doc.diagnostics.len() <= plannercraft_mspdi::diag::MAX_DIAGNOSTICS + 1 + 100);
}

#[test]
fn many_predecessors_read_in_linear_time() {
    // Relecture J2 : la détection des doublons était quadratique (36 s pour
    // 200 000 prédécesseurs en release). Ici 200 000 tâches, dont une liée à
    // toutes les autres, plus un doublon.
    let n: u32 = 200_000;
    let mut xml = String::from("<Project><StartDate>2026-10-19T08:00:00</StartDate><Tasks>");
    for uid in 1..n {
        let _ = write!(xml, "<Task><UID>{uid}</UID><Duration>PT8H0M0S</Duration></Task>");
    }
    let _ = write!(xml, "<Task><UID>{n}</UID><Duration>PT8H0M0S</Duration>");
    for uid in 1..n {
        let _ = write!(
            xml,
            "<PredecessorLink><PredecessorUID>{uid}</PredecessorUID></PredecessorLink>"
        );
    }
    xml.push_str(
        "<PredecessorLink><PredecessorUID>1</PredecessorUID></PredecessorLink></Task></Tasks></Project>",
    );
    let started = std::time::Instant::now();
    let doc = read(xml.as_bytes(), &opts()).unwrap();
    let last = doc.project.tasks.get(plannercraft_core::TaskUid::new(n)).unwrap();
    assert_eq!(last.predecessors.len(), usize::try_from(n - 1).unwrap());
    assert!(doc.diagnostics.iter().any(|d| d.code == ImportCode::Duplicate));
    // Garde-fou large (build de test non optimisé) ; la cible de 1 s en
    // release est mesurée par `tests/perf.rs`.
    assert!(started.elapsed().as_secs() < 60, "{:?}", started.elapsed());
}

#[test]
fn absurd_values_become_diagnostics() {
    let xml = "<Project><StartDate>2026-10-19T08:00:00</StartDate>\
               <MinutesPerDay>99999999999999999999999</MinutesPerDay>\
               <CalendarUID>-7</CalendarUID>\
               <Calendars><Calendar><UID>-3</UID></Calendar><Calendar><UID>4294967296</UID></Calendar>\
               <Calendar><UID>1</UID><IsBaseCalendar>1</IsBaseCalendar><WeekDays><WeekDay><DayType>2</DayType>\
               <DayWorking>1</DayWorking><WorkingTimes><WorkingTime><FromTime>99:00:00</FromTime>\
               <ToTime>12:00:00</ToTime></WorkingTime></WorkingTimes></WeekDay></WeekDays>\
               <Exceptions><Exception><TimePeriod><FromDate>2400-01-01T00:00:00</FromDate></TimePeriod>\
               <Type>1</Type><Period>999999</Period></Exception></Exceptions></Calendar></Calendars></Project>";
    let doc = read(xml.as_bytes(), &opts()).unwrap();
    assert_eq!(doc.project.options.factors.minutes_per_day(), 480);
    assert_eq!(doc.project.calendars.len(), 1);
    assert!(doc.diagnostics.iter().any(|d| d.code == ImportCode::InvalidValue));
}

#[test]
fn every_prefix_is_ok_or_err() {
    for (_, bytes) in fixtures() {
        for end in 0..=bytes.len() {
            let _ = read(&bytes[..end], &opts());
        }
    }
}

/// Générateur pseudo-aléatoire xorshift64, graine fixe (déterministe).
struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
fn corpus_files_read_and_truncations_never_panic() {
    let files = xml_files(&repo("tests/corpus/files"));
    let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
    for (name, bytes) in files {
        read(&bytes, &opts()).unwrap_or_else(|e| panic!("{name} : {e}"));
        for _ in 0..200 {
            let end = usize::try_from(rng.next() % (bytes.len() as u64 + 1)).unwrap();
            let _ = read(&bytes[..end], &opts());
        }
    }
}

/// Mutation d'un document.
#[derive(Clone, Debug)]
enum Mutation {
    /// Octet remplacé.
    Replace(usize, u8),
    /// Octet inséré.
    Insert(usize, u8),
    /// Octet supprimé.
    Delete(usize),
    /// Bloc dupliqué (début, longueur).
    Duplicate(usize, usize),
    /// Balise fermante retirée (n-ième `</`).
    DropClosingTag(usize),
}

fn mutation() -> impl Strategy<Value = Mutation> {
    prop_oneof![
        (any::<usize>(), any::<u8>()).prop_map(|(p, b)| Mutation::Replace(p, b)),
        (any::<usize>(), any::<u8>()).prop_map(|(p, b)| Mutation::Insert(p, b)),
        any::<usize>().prop_map(Mutation::Delete),
        (any::<usize>(), 1_usize..400).prop_map(|(p, n)| Mutation::Duplicate(p, n)),
        any::<usize>().prop_map(Mutation::DropClosingTag),
    ]
}

fn apply(bytes: &mut Vec<u8>, m: &Mutation) {
    let len = bytes.len();
    if len == 0 {
        return;
    }
    match *m {
        Mutation::Replace(p, b) => bytes[p % len] = b,
        Mutation::Insert(p, b) => bytes.insert(p % (len + 1), b),
        Mutation::Delete(p) => {
            bytes.remove(p % len);
        }
        Mutation::Duplicate(p, n) => {
            let start = p % len;
            let end = (start + n).min(len);
            let block: Vec<u8> = bytes[start..end].to_vec();
            bytes.splice(end..end, block);
        }
        Mutation::DropClosingTag(k) => {
            let positions: Vec<usize> = bytes
                .windows(2)
                .enumerate()
                .filter(|(_, w)| w == b"</")
                .map(|(i, _)| i)
                .collect();
            if positions.is_empty() {
                return;
            }
            let start = positions[k % positions.len()];
            if let Some(off) = bytes[start..].iter().position(|b| *b == b'>') {
                bytes.drain(start..=start + off);
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    /// P18 : octet modifié, inséré, supprimé ; bloc dupliqué ; balise
    /// fermante retirée : `Ok` ou `Err`, jamais de panique.
    #[test]
    fn p18_mutations_never_panic(
        index in any::<usize>(),
        mutations in proptest::collection::vec(mutation(), 1..6)
    ) {
        let small = small_fixtures();
        let mut bytes = small[index % small.len()].clone();
        for m in &mutations {
            apply(&mut bytes, m);
        }
        let _ = read(&bytes, &opts());
    }
}
