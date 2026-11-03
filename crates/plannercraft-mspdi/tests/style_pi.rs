//! Style du document dans le fichier MSPDI (H-R12, ADR-0016 D7, lot L6 de
//! J8) : instruction `<?plannercraft-style?>` écrite juste après la déclaration
//! XML ssi le projet a une entrée de style, lue bornée, jamais écrite comme
//! élément ; un projet sans style s'écrit comme en J7.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

use std::path::PathBuf;

use plannercraft_core::{BuiltinCalendarNames, ExtraEntry, Moment, Project, ProjectInit, ScheduleAnchor};
use plannercraft_mspdi::style_pi::{self, StyleIgnoredReason};
use plannercraft_mspdi::write::{Computed, Indent, OrderProfile, WriteCode, WriteOptions, write};
use plannercraft_mspdi::{ImportCode, ReadOptions, normalize, read};

const STYLE: &str = r#"{"base":"sobre","over":{"density":"airy"},"v":1}"#;
const DECL: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#;

fn now() -> Moment {
    "2026-10-19T08:00".parse().unwrap()
}

fn project() -> Project {
    Project::new(ProjectInit {
        name: "P".to_owned(),
        anchor: ScheduleAnchor::FromStart(now()),
        current_date: now(),
        calendar_names: BuiltinCalendarNames::default(),
    })
}

fn styled(payload: &str) -> Project {
    let mut p = project();
    style_pi::set(&mut p.extras, Some(payload.to_owned()));
    p
}

fn text(p: &Project, o: &WriteOptions) -> String {
    String::from_utf8(write(p, Computed::Omit, o).xml).unwrap()
}

fn reread(xml: &str) -> plannercraft_mspdi::MspdiDocument {
    read(xml.as_bytes(), &ReadOptions::new(now())).unwrap()
}

#[test]
fn h_r12_pi_written_after_declaration() {
    let xml = text(&styled(STYLE), &WriteOptions::default());
    let expected = format!("{DECL}\n<?plannercraft-style {STYLE}?>\n<Project xmlns=");
    assert!(
        xml.starts_with(&expected),
        "{}",
        xml.chars().take(200).collect::<String>()
    );
    assert_eq!(xml.matches("plannercraft-style").count(), 1);
    // Aucune entrée d'origine `plannercraft` écrite comme élément XML.
    assert!(!xml.contains("<style") && !xml.contains("plannercraft</"));
    // CRLF et sans indentation : l'instruction suit la fin de ligne choisie.
    let crlf = WriteOptions {
        newline: plannercraft_mspdi::write::Newline::Crlf,
        indent: Indent::None,
        ..WriteOptions::default()
    };
    let xml = text(&styled(STYLE), &crlf);
    assert!(xml.starts_with(&format!("{DECL}\r\n<?plannercraft-style {STYLE}?>\r\n<Project")));
    // Relu : entrée de style égale, aucun diagnostic de style.
    let doc = reread(&xml);
    assert_eq!(
        style_pi::entry(&doc.project.extras).map(|e| e.payload.as_str()),
        Some(STYLE)
    );
    assert!(doc.diagnostics.iter().all(|d| d.code.code() != "style.ignored"));
}

#[test]
fn h_r12_preference_off_writes_nothing() {
    let off = WriteOptions {
        style: false,
        ..WriteOptions::default()
    };
    let xml = text(&styled(STYLE), &off);
    assert_eq!(xml, text(&project(), &WriteOptions::default()));
    assert!(!xml.contains("plannercraft-style"));
    // normalize suit l'écriture : style perdu sans écriture, gardé avec.
    assert!(style_pi::entry(&normalize(&styled(STYLE), &off).extras).is_none());
    let kept = normalize(&styled(STYLE), &WriteOptions::default());
    assert_eq!(
        style_pi::entry(&kept.extras).map(|e| e.payload.as_str()),
        Some(STYLE)
    );
}

#[test]
fn h_r12_unwritable_payload_reported_not_written() {
    for bad in ["", " {}", "{\"a\":\"?>\"}"] {
        let w = write(&styled(bad), Computed::Omit, &WriteOptions::default());
        let xml = String::from_utf8(w.xml).unwrap();
        assert!(!xml.contains("plannercraft-style"), "{bad:?}");
        assert!(
            w.diagnostics
                .iter()
                .any(|d| d.code == WriteCode::ExtraInvalid && d.element.contains("plannercraft-style")),
            "{bad:?}"
        );
    }
}

#[test]
fn h_r12_oversized_ignored() {
    let big = format!(r#"{{"v":1,"base":"sobre","name":"{}"}}"#, "x".repeat(70_000));
    let xml = text(&project(), &WriteOptions::default()).replacen(
        "<Project",
        &format!("<?plannercraft-style {big}?>\n<Project"),
        1,
    );
    let doc = reread(&xml);
    assert!(style_pi::entry(&doc.project.extras).is_none());
    let d = doc
        .diagnostics
        .iter()
        .find(|d| d.code.code() == "style.ignored")
        .expect("diagnostic style.ignored");
    assert_eq!(
        d.code,
        ImportCode::StyleIgnored {
            reason: StyleIgnoredReason::TooLarge
        }
    );
    assert_eq!(d.severity, plannercraft_core::Severity::Warning);
}

#[test]
fn h_r12_first_before_root_wins() {
    let base = text(&project(), &WriteOptions::default());
    let xml = base
        .replacen(
            "<Project",
            &format!("<?plannercraft-style {STYLE}?>\n<?other x?>\n<?plannercraft-style {{}}?>\n<Project"),
            1,
        )
        .replacen("<Name>", "<?plannercraft-style {}?><Name>", 1);
    let doc = reread(&xml);
    assert_eq!(
        style_pi::entry(&doc.project.extras).map(|e| e.payload.as_str()),
        Some(STYLE)
    );
    let reasons: Vec<ImportCode> = doc
        .diagnostics
        .iter()
        .filter(|d| d.code.code() == "style.ignored")
        .map(|d| d.code.clone())
        .collect();
    assert_eq!(
        reasons,
        [
            ImportCode::StyleIgnored {
                reason: StyleIgnoredReason::Duplicate
            },
            ImportCode::StyleIgnored {
                reason: StyleIgnoredReason::AfterRoot
            },
        ]
    );
    // Une seule entrée, en fin de liste ; les autres instructions ignorées.
    let n = doc
        .project
        .extras
        .0
        .iter()
        .filter(|e| e.origin == style_pi::ORIGIN)
        .count();
    assert_eq!(n, 1);
}

/// P56 (partie MSPDI) : projet stylé écrit, relu, réécrit ⇒ mêmes octets,
/// dans les deux profils.
#[test]
fn p56_mspdi_styled_roundtrip_same_bytes() {
    for profile in [OrderProfile::Observed, OrderProfile::Strict] {
        let o = WriteOptions {
            profile,
            ..WriteOptions::default()
        };
        let first = text(&styled(STYLE), &o);
        let again = text(&reread(&first).project, &o);
        assert_eq!(first, again, "{profile:?}");
    }
}

/// Renommage du 2026-11-03 : un fichier écrit par une version nommée
/// `planrs` (`<?planrs-style …?>`) garde son style à la lecture ; la
/// réécriture le place sous la nouvelle cible, une seule fois.
#[test]
fn h_r12_legacy_planrs_style_pi_read() {
    let xml = text(&styled(STYLE), &WriteOptions::default());
    let legacy = xml.replace("<?plannercraft-style ", "<?planrs-style ");
    assert!(legacy.contains("<?planrs-style ") && !legacy.contains("plannercraft-style"));
    let doc = reread(&legacy);
    assert_eq!(
        style_pi::entry(&doc.project.extras).map(|e| e.payload.as_str()),
        Some(STYLE)
    );
    assert!(doc.diagnostics.iter().all(|d| d.code.code() != "style.ignored"));
    let rewritten = text(&doc.project, &WriteOptions::default());
    assert_eq!(rewritten, xml);
    // Les deux cibles : la première l'emporte, la seconde est un doublon.
    let both = xml.replace(
        "<?plannercraft-style ",
        &format!(
            "<?planrs-style {}?>\n<?plannercraft-style ",
            r#"{"base":"presentation","v":1}"#
        ),
    );
    let doc = reread(&both);
    assert_eq!(
        style_pi::entry(&doc.project.extras).map(|e| e.payload.as_str()),
        Some(r#"{"base":"presentation","v":1}"#)
    );
    assert!(doc.diagnostics.iter().any(|d| d.code
        == ImportCode::StyleIgnored {
            reason: StyleIgnoredReason::Duplicate
        }));
}

/// Entrée de style d'origine `planrs` (projet JSON écrit par une ancienne
/// version, journal de reprise) : reconnue, écrite sous la nouvelle cible,
/// remplacée par `set`.
#[test]
fn h_r12_legacy_planrs_style_origin() {
    let mut p = project();
    p.extras.0.push(ExtraEntry {
        origin: style_pi::LEGACY_ORIGIN.to_owned(),
        key: style_pi::KEY.to_owned(),
        payload: STYLE.to_owned(),
    });
    assert_eq!(
        style_pi::entry(&p.extras).map(|e| e.payload.as_str()),
        Some(STYLE)
    );
    assert_eq!(
        text(&p, &WriteOptions::default()),
        text(&styled(STYLE), &WriteOptions::default())
    );
    style_pi::set(&mut p.extras, Some("{}".to_owned()));
    assert_eq!(p.extras.0.len(), 1);
    assert_eq!(p.extras.0[0].origin, style_pi::ORIGIN);
}

/// Une entrée d'origine `plannercraft` d'une autre clé n'est ni écrite comme
/// élément ni comme instruction.
#[test]
fn h_r12_other_plannercraft_entries_not_written() {
    let mut p = project();
    p.extras.0.push(ExtraEntry {
        origin: "plannercraft".to_owned(),
        key: "other".to_owned(),
        payload: "<Other/>".to_owned(),
    });
    assert_eq!(
        text(&p, &WriteOptions::default()),
        text(&project(), &WriteOptions::default())
    );
}

/// Fichiers du corpus (`tests/corpus/files`, absents : test sauté).
fn corpus() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/files");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|d| d.filter_map(Result::ok).map(|e| e.path()).collect())
        .unwrap_or_default();
    files.retain(|p| p.extension().is_some_and(|e| e == "xml"));
    files.sort();
    files
}

/// Corpus relu et écrit : aucune instruction de style, et octets identiques
/// avec la préférence d'écriture vraie ou fausse (un fichier jamais stylé
/// garde les octets de J7 : la seule différence possible du lot L6 est
/// l'instruction).
#[test]
fn h_r12_no_style_no_pi_bytes_equal_j7() {
    let files = corpus();
    if files.is_empty() {
        eprintln!("corpus absent : test sauté (`cargo xtask corpus fetch`)");
    }
    for path in files {
        let bytes = std::fs::read(&path).unwrap();
        let doc = read(&bytes, &ReadOptions::new(now())).unwrap();
        assert!(
            style_pi::entry(&doc.project.extras).is_none(),
            "{}",
            path.display()
        );
        let rec = Computed::Recorded(&doc.recorded);
        let on = write(&doc.project, rec, &WriteOptions::default()).xml;
        let off = write(
            &doc.project,
            rec,
            &WriteOptions {
                style: false,
                ..WriteOptions::default()
            },
        )
        .xml;
        assert_eq!(on, off, "{}", path.display());
        let on = String::from_utf8(on).unwrap();
        assert!(!on.contains("plannercraft-style"), "{}", path.display());
        assert!(
            on.starts_with(&format!("{DECL}\n<Project ")),
            "{}",
            path.display()
        );
    }
}

/// H-R12 : le contenu d'une instruction n'est pas analysé (XML 1.0 § 2.6) :
/// `&`, `<`, `&amp;` y sont pris tels quels, sans résolution d'entité.
#[test]
fn h_r12_pi_content_taken_verbatim() {
    let base = text(&project(), &WriteOptions::default());
    for raw in ["a&b", "x<y", "&amp;", "&#38;", "a & b < c &lt;"] {
        let xml = base.replacen("<Project", &format!("<?plannercraft-style {raw}?>\n<Project"), 1);
        let doc =
            read(xml.as_bytes(), &ReadOptions::new(now())).unwrap_or_else(|e| panic!("{raw:?} : {e:?}"));
        assert_eq!(
            style_pi::entry(&doc.project.extras).map(|e| e.payload.as_str()),
            Some(raw),
            "{raw:?}"
        );
    }
}
