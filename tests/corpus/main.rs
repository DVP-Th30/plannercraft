//! Harnais du corpus public (ADR-0004, ADR-0009 § 9).
//!
//! Le corpus est décrit par `tests/corpus/manifest.toml` (versionné, format
//! 2) ; les fichiers vivent dans `tests/corpus/files/`, ignoré par git (droits
//! d'auteur), remplis par `cargo xtask corpus fetch`. `corpus_parity` lit
//! chaque fichier présent, laisse le moteur recalculer, compare aux valeurs
//! enregistrées par MS Project, écrit le rapport (`target/tmp/corpus-report.md`
//! et `.json`) et applique le verdict contre les planchers. Les autres tests
//! vérifient le manifeste et le harnais lui-même sur des données écrites à la
//! main.
//!
//! `PLANNERCRAFT_CORPUS=required` (job CI `corpus`) : échec si tous les fichiers
//! sont absents ; sans la variable, un corpus absent n'est qu'un
//! avertissement.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use std::path::Path;

use plannercraft_core::parity::{CORPUS_FLOOR, FLOOR, ORACLE_FLOOR};
use plannercraft_tests::corpus::compare::{CaseKind, Cause, Excluded, Field};
use plannercraft_tests::corpus::manifest::{self, Entry, Producer};
use plannercraft_tests::corpus::report::{self, Floors};
use plannercraft_tests::corpus::roundtrip::{self, RoundTrip};
use plannercraft_tests::corpus::verdict::{self, AbsentReason, FileResult, FileStatus};
use plannercraft_tests::corpus::{self, FileRun};

fn load_manifest() -> manifest::Manifest {
    manifest::load(&plannercraft_tests::tests_path("corpus/manifest.toml")).expect("manifeste lisible")
}

fn floors() -> Floors {
    Floors {
        corpus: CORPUS_FLOOR,
        oracle: ORACLE_FLOOR,
        total: FLOOR,
    }
}

#[test]
fn manifest_is_well_formed() {
    let m = load_manifest();
    assert_eq!(m.format, manifest::FORMAT, "format de manifeste inconnu");
    let problems = manifest::validate(&m);
    assert!(problems.is_empty(), "manifeste invalide : {problems:#?}");
}

#[test]
fn corpus_floor_is_the_sum_of_manifest_floors() {
    let m = load_manifest();
    assert_eq!(
        u64::from(CORPUS_FLOOR),
        manifest::floor_sum(&m),
        "plannercraft_core::parity::CORPUS_FLOOR doit valoir la somme des `floor` du manifeste"
    );
    assert_eq!(
        u64::from(FLOOR),
        u64::from(CORPUS_FLOOR) + u64::from(ORACLE_FLOOR)
    );
}

/// Parité du corpus : rapport, puis verdict.
#[test]
fn corpus_parity() {
    let m = load_manifest();
    let runs = corpus::run(&m.file, &plannercraft_tests::tests_path("corpus/files"));
    let results: Vec<FileResult> = runs.iter().map(FileRun::result).collect();
    let required = std::env::var("PLANNERCRAFT_CORPUS").is_ok_and(|v| v == "required");
    let mut v = verdict::decide(&results, CORPUS_FLOOR, required);
    // P25 : aller-retour MSPDI des fichiers présents.
    let files = plannercraft_tests::tests_path("corpus/files");
    let round_trips: Vec<RoundTrip> = runs
        .iter()
        .filter_map(|run| {
            let doc = run.document.as_ref()?;
            let source = std::fs::read_to_string(files.join(&run.entry.name)).ok()?;
            Some(roundtrip::round_trip(&run.entry, doc, &source))
        })
        .collect();
    v.failures.extend(roundtrip::failures(&round_trips));
    let mut r = report::build(&runs, floors(), v.clone());
    r.round_trips = round_trips;
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"));
    report::write(dir, &r).expect("écriture du rapport");
    print!("{}", report::summary(&r));
    println!("rapport : {}", dir.join("corpus-report.md").display());
    for w in &v.warnings {
        eprintln!("avertissement : {w}");
    }
    for n in &v.raisable {
        println!("{n}");
    }
    assert!(v.passed(), "parité du corpus en échec : {:#?}", v.failures);
}

// --- Manifeste -------------------------------------------------------------

const ENTRY: &str = r#"format = 2
[[file]]
name = "a.xml"
source = "s"
url = "https://example.invalid/a.xml"
mirrors = []
license = "MIT OR Apache-2.0"
rights = "copie privée"
date = "2026-10-17"
size = 3
sha256 = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
producer = "msproject-probable"
save_version = 14
note = ""
floor = 2
"#;

#[test]
fn manifest_keys_are_all_required_and_unknown_keys_refused() {
    let m = manifest::parse(ENTRY).unwrap();
    assert_eq!(manifest::validate(&m), Vec::<String>::new());
    assert_eq!(m.file[0].producer, Producer::MsprojectProbable);
    assert_eq!(manifest::floor_sum(&m), 2);
    for key in [
        "mirrors",
        "license",
        "rights",
        "size",
        "producer",
        "save_version",
        "note",
        "floor",
    ] {
        let prefix = format!("{key} =");
        let text = ENTRY
            .lines()
            .filter(|l| !l.starts_with(&prefix))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(manifest::parse(&text).is_err(), "clé {key} manquante acceptée");
    }
    assert!(manifest::parse(&format!("{ENTRY}extra = 1\n")).is_err());
    assert!(manifest::parse(&ENTRY.replace("msproject-probable", "libreoffice")).is_err());
}

#[test]
fn manifest_content_rules() {
    let mut m = manifest::parse(ENTRY).unwrap();
    let mut second = m.file[0].clone();
    second.name = "../b.xml".to_owned();
    second.url = "http://example.invalid/b.xml".to_owned();
    second.mirrors = vec!["ftp://x".to_owned()];
    second.license = "GPL 3".to_owned();
    second.size = 0;
    m.file.push(second);
    m.format = 1;
    let problems = manifest::validate(&m).join("\n");
    for expected in [
        "format 1",
        "nom invalide",
        "url non https",
        "miroir non https",
        "licence",
        "taille nulle",
        "même empreinte",
    ] {
        assert!(problems.contains(expected), "{expected} absent de :\n{problems}");
    }
}

#[test]
fn format_helpers() {
    assert!(manifest::is_iso_date("2026-10-17"));
    assert!(!manifest::is_iso_date("17/10/2026"));
    assert!(manifest::is_sha256(&"a".repeat(64)));
    assert!(!manifest::is_sha256(&"A".repeat(64)));
    assert!(manifest::is_license("none"));
    assert!(manifest::is_license("Apache-2.0"));
    assert!(manifest::is_license("MIT OR Apache-2.0"));
    assert!(!manifest::is_license("MIT OR"));
    assert!(!manifest::is_license(""));
    assert_eq!(
        manifest::sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

// --- Verdict ---------------------------------------------------------------

fn compared(name: &str, floor: u32, cases: u32, identical: u32) -> FileResult {
    FileResult {
        name: name.to_owned(),
        floor,
        complete_required: true,
        status: FileStatus::Compared {
            cases,
            identical,
            incomplete: 0,
        },
    }
}

fn absent(name: &str, floor: u32) -> FileResult {
    FileResult {
        name: name.to_owned(),
        floor,
        complete_required: true,
        status: FileStatus::Absent(AbsentReason::NotDownloaded),
    }
}

#[test]
fn verdict_incomplete_msproject_file_fails() {
    let incomplete = |complete_required| FileResult {
        name: "a".to_owned(),
        floor: 0,
        complete_required,
        status: FileStatus::Compared {
            cases: 10,
            identical: 10,
            incomplete: 1,
        },
    };
    let v = verdict::decide(&[incomplete(true)], 0, false);
    assert!(
        v.failures.iter().any(|f| f.contains("comparaison incomplète")),
        "{v:?}"
    );
    assert!(verdict::decide(&[incomplete(false)], 0, false).passed());
}

#[test]
fn verdict_regression_on_a_present_file_fails() {
    let v = verdict::decide(&[compared("a", 10, 20, 9), compared("b", 5, 5, 5)], 15, false);
    assert!(!v.passed());
    assert!(v.failures[0].contains("a : régression"), "{v:?}");
    // Le global (14 < 15) échoue aussi.
    assert_eq!(v.failures.len(), 2, "{v:?}");
}

#[test]
fn verdict_absent_file_counts_neither_for_nor_against() {
    let v = verdict::decide(&[compared("a", 10, 20, 10), absent("b", 5)], 15, false);
    assert!(v.passed(), "{v:?}");
    assert_eq!(v.warnings.len(), 1);
    // Un fichier présent ne compense pas un autre en régression.
    let v = verdict::decide(&[compared("a", 10, 20, 15), compared("b", 5, 5, 0)], 15, false);
    assert!(!v.passed());
}

#[test]
fn verdict_corpus_floor_must_match_the_manifest() {
    let v = verdict::decide(&[compared("a", 10, 20, 10)], 9, false);
    assert!(v.failures.iter().any(|f| f.contains("CORPUS_FLOOR")), "{v:?}");
}

#[test]
fn verdict_read_error_fails_only_with_a_floor() {
    let err = |floor| FileResult {
        name: "a".to_owned(),
        floor,
        complete_required: true,
        status: FileStatus::ReadError("mspdi.xml".to_owned()),
    };
    assert!(verdict::decide(&[err(0)], 0, false).passed());
    assert!(!verdict::decide(&[err(1)], 1, false).passed());
}

#[test]
fn verdict_all_absent_fails_only_when_required() {
    let all = [absent("a", 3), absent("b", 0)];
    assert!(verdict::decide(&all, 3, false).passed());
    assert!(!verdict::decide(&all, 3, true).passed());
    // Manifeste vide : rien à exiger.
    assert!(verdict::decide(&[], 0, true).passed());
    // Un seul présent suffit.
    assert!(verdict::decide(&[absent("a", 3), compared("b", 0, 1, 0)], 3, true).passed());
}

#[test]
fn verdict_reports_raisable_floors() {
    let v = verdict::decide(&[compared("a", 2, 10, 7)], 2, true);
    assert!(v.passed());
    assert_eq!(v.raisable.len(), 1);
    assert!(v.raisable[0].contains("plancher relevable"));
}

// --- Comparaison et rapport, sur un MSPDI écrit à la main ------------------

const WEEK: &str = "<WeekDays>\
<WeekDay><DayType>1</DayType><DayWorking>0</DayWorking></WeekDay>\
<WeekDay><DayType>2</DayType><DayWorking>1</DayWorking><WorkingTimes><WorkingTime><FromTime>08:00:00</FromTime><ToTime>12:00:00</ToTime></WorkingTime><WorkingTime><FromTime>13:00:00</FromTime><ToTime>17:00:00</ToTime></WorkingTime></WorkingTimes></WeekDay>\
<WeekDay><DayType>3</DayType><DayWorking>1</DayWorking><WorkingTimes><WorkingTime><FromTime>08:00:00</FromTime><ToTime>12:00:00</ToTime></WorkingTime><WorkingTime><FromTime>13:00:00</FromTime><ToTime>17:00:00</ToTime></WorkingTime></WorkingTimes></WeekDay>\
<WeekDay><DayType>4</DayType><DayWorking>1</DayWorking><WorkingTimes><WorkingTime><FromTime>08:00:00</FromTime><ToTime>12:00:00</ToTime></WorkingTime><WorkingTime><FromTime>13:00:00</FromTime><ToTime>17:00:00</ToTime></WorkingTime></WorkingTimes></WeekDay>\
<WeekDay><DayType>5</DayType><DayWorking>1</DayWorking><WorkingTimes><WorkingTime><FromTime>08:00:00</FromTime><ToTime>12:00:00</ToTime></WorkingTime><WorkingTime><FromTime>13:00:00</FromTime><ToTime>17:00:00</ToTime></WorkingTime></WorkingTimes></WeekDay>\
<WeekDay><DayType>6</DayType><DayWorking>1</DayWorking><WorkingTimes><WorkingTime><FromTime>08:00:00</FromTime><ToTime>12:00:00</ToTime></WorkingTime><WorkingTime><FromTime>13:00:00</FromTime><ToTime>17:00:00</ToTime></WorkingTime></WorkingTimes></WeekDay>\
<WeekDay><DayType>7</DayType><DayWorking>0</DayWorking></WeekDay>\
</WeekDays>";

/// Tâche avec ses valeurs enregistrées « comme MS Project » (début, fin,
/// dates au plus tôt et au plus tard identiques : aucune marge).
fn task(uid: u32, name: &str, level: u32, extra: &str, start: &str, finish: &str, dur: &str) -> String {
    format!(
        "<Task><UID>{uid}</UID><ID>{uid}</ID><Name>{name}</Name><OutlineLevel>{level}</OutlineLevel>\
<Duration>{dur}</Duration><DurationFormat>7</DurationFormat>\
<Start>{start}</Start><Finish>{finish}</Finish><EarlyStart>{start}</EarlyStart><EarlyFinish>{finish}</EarlyFinish>\
<LateStart>{start}</LateStart><LateFinish>{finish}</LateFinish><TotalSlack>0</TotalSlack><FreeSlack>0</FreeSlack>\
<Critical>1</Critical>{extra}</Task>"
    )
}

/// Projet : A 1 j → B 1 j (FS), M manuelle, ligne vide ; lun 19/10/2026.
/// `b_late_finish` remplace la fin au plus tard enregistrée de B.
fn sample(b_late_finish: &str) -> String {
    let a = task(
        1,
        "Alpha confidentiel",
        1,
        "",
        "2026-10-19T08:00:00",
        "2026-10-19T17:00:00",
        "PT8H0M0S",
    );
    let b = task(
        2,
        "Bravo confidentiel",
        1,
        "<PredecessorLink><PredecessorUID>1</PredecessorUID><Type>1</Type></PredecessorLink>",
        "2026-10-20T08:00:00",
        "2026-10-20T17:00:00",
        "PT8H0M0S",
    )
    .replace(
        "<LateFinish>2026-10-20T17:00:00</LateFinish>",
        &format!("<LateFinish>{b_late_finish}</LateFinish>"),
    );
    let m = task(
        3,
        "Manuelle confidentielle",
        1,
        "<Manual>1</Manual><ManualStart>2026-10-19T08:00:00</ManualStart><ManualFinish>2026-10-19T17:00:00</ManualFinish>",
        "2026-10-19T08:00:00",
        "2026-10-19T17:00:00",
        "PT8H0M0S",
    );
    let root = task(
        0,
        "Projet confidentiel",
        0,
        "<Summary>1</Summary>",
        "2026-10-19T08:00:00",
        "2026-10-20T17:00:00",
        "PT16H0M0S",
    )
    .replace(
        "<DurationFormat>7</DurationFormat>",
        "<DurationFormat>21</DurationFormat>",
    );
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
<Project xmlns=\"http://schemas.microsoft.com/project\"><SaveVersion>14</SaveVersion><Name>Projet confidentiel</Name>\
<StartDate>2026-10-19T08:00:00</StartDate><CalendarUID>1</CalendarUID>\
<Calendars><Calendar><UID>1</UID><Name>Standard</Name><IsBaseCalendar>1</IsBaseCalendar>{WEEK}</Calendar></Calendars>\
<Tasks>{root}{a}{b}{m}<Task><UID>4</UID><ID>4</ID><IsNull>1</IsNull></Task></Tasks></Project>"
    )
}

fn entry(name: &str, bytes: &[u8], floor: u32) -> Entry {
    let mut e = manifest::parse(ENTRY).unwrap().file.remove(0);
    name.clone_into(&mut e.name);
    e.size = u64::try_from(bytes.len()).unwrap();
    e.sha256 = manifest::sha256_hex(bytes);
    e.floor = floor;
    e
}

#[test]
fn identical_project_compares_every_field_and_excludes_manual_and_null_rows() {
    let xml = sample("2026-10-20T17:00:00");
    let run = corpus::run_bytes(&entry("ok.xml", xml.as_bytes(), 3), xml.as_bytes());
    let cmp = run.comparison.as_ref().expect("comparé");
    assert_eq!(
        run.status,
        FileStatus::Compared {
            cases: 3,
            identical: 3,
            incomplete: 0
        },
        "{cmp:#?}"
    );
    let kinds: Vec<_> = cmp.cases.iter().map(|c| (c.uid, c.kind)).collect();
    assert_eq!(
        kinds,
        [
            (0, CaseKind::ProjectSummary),
            (1, CaseKind::Leaf),
            (2, CaseKind::Leaf)
        ]
    );
    // Durée comparée pour l'UID 0 seulement.
    assert!(cmp.cases[0].compared.contains(&Field::Duration));
    assert!(!cmp.cases[1].compared.contains(&Field::Duration));
    assert_eq!(cmp.cases[1].compared.len(), 9);
    assert_eq!(cmp.excluded.get(&Excluded::Manual), Some(&1));
    assert_eq!(cmp.excluded.get(&Excluded::NullRow), Some(&1));
    // Règles exercées.
    assert!(cmp.cases[2].rules.contains("R-037"));
    assert!(cmp.cases[1].rules.contains("R-036"));
    assert!(cmp.cases[0].rules.contains("R-068"));
}

#[test]
fn divergence_is_reported_with_its_delta_and_without_task_names() {
    let xml = sample("2026-10-21T17:00:00");
    let file = entry("diff.xml", xml.as_bytes(), 0);
    let run = corpus::run_bytes(&file, xml.as_bytes());
    let cmp = run.comparison.as_ref().unwrap();
    let b = cmp.cases.iter().find(|c| c.uid == 2).unwrap();
    assert!(!b.identical());
    assert_eq!(b.divergences.len(), 1);
    let d = &b.divergences[0];
    assert_eq!(d.field, Field::LateFinish);
    assert_eq!(d.recorded, "2026-10-21T17:00");
    assert_eq!(d.computed, "2026-10-20T17:00");
    assert_eq!(d.delta_minutes, Some(-1_440));
    // Divergence locale : ni cause du modèle ni prédécesseur divergent ;
    // l'UID 0, identique, n'est pas touché.
    assert!(b.causes.is_empty(), "{:?}", b.causes);
    assert_eq!(
        run.status,
        FileStatus::Compared {
            cases: 3,
            identical: 2,
            incomplete: 0
        }
    );

    let runs = [run];
    let results: Vec<_> = runs.iter().map(FileRun::result).collect();
    let v = verdict::decide(&results, 0, true);
    let r = report::build(
        &runs,
        Floors {
            corpus: 0,
            oracle: 0,
            total: 0,
        },
        v,
    );
    let md = report::markdown(&r);
    let json = serde_json::to_string(&r).unwrap();
    for text in [&md, &json] {
        assert!(
            !text.contains("confidenti"),
            "nom de tâche dans le rapport :\n{text}"
        );
        assert!(text.contains("late_finish"));
    }
    assert!(
        md.contains("| 2 | 2 | leaf | late_finish | 2026-10-21T17:00 | 2026-10-20T17:00 | -1440 |"),
        "{md}"
    );
    // Déterministe.
    assert_eq!(md, report::markdown(&r));
    let sig = r.files[0].signature.as_ref().unwrap();
    assert_eq!(
        sig.missing,
        ["ui_sections"],
        "pas de section d'interface : pas une signature MS Project"
    );
}

#[test]
fn propagated_divergence_is_labelled() {
    // A diverge (fin au plus tôt enregistrée décalée) : B, son successeur, et
    // l'UID 0, sa récapitulative, reçoivent l'étiquette de propagation dès
    // qu'ils divergent eux aussi.
    let xml = sample("2026-10-21T17:00:00").replacen(
        "<EarlyFinish>2026-10-19T17:00:00</EarlyFinish>",
        "<EarlyFinish>2026-10-19T16:00:00</EarlyFinish>",
        2,
    );
    let run = corpus::run_bytes(&entry("p.xml", xml.as_bytes(), 0), xml.as_bytes());
    let cmp = run.comparison.unwrap();
    let b = cmp.cases.iter().find(|c| c.uid == 2).unwrap();
    assert!(b.causes.contains(&Cause::DivergentPredecessor), "{:?}", b.causes);
}

#[test]
fn missing_recorded_slack_makes_the_comparison_incomplete() {
    // Relecture J2 : une régression du lecteur qui perdrait `TotalSlack`
    // laisserait le cas « identique » sur les autres champs ; pour un fichier
    // enregistré par MS Project, le verdict échoue.
    let xml = sample("2026-10-20T17:00:00").replacen("<TotalSlack>0</TotalSlack>", "", 2);
    let mut e = entry("s.xml", xml.as_bytes(), 0);
    e.producer = Producer::Msproject;
    let run = corpus::run_bytes(&e, xml.as_bytes());
    let cmp = run.comparison.as_ref().unwrap();
    // UID 0 et A sans marge totale : comparés sur les autres champs.
    assert_eq!(cmp.identical_count(), 3);
    assert_eq!(cmp.incomplete_count(), 2);
    assert_eq!(cmp.compared_by_field().get(&Field::TotalSlack), Some(&1));
    assert_eq!(cmp.cases[0].missing_fields(), [Field::TotalSlack]);
    let v = verdict::decide(&[run.result()], 0, true);
    assert!(!v.passed(), "{v:?}");
    let r = report::build(&[run], floors_zero(), v);
    assert_eq!(r.files[0].incomplete, 2);
    assert_eq!(r.files[0].compared_by_field.get("total_slack"), Some(&1));
    assert!(report::markdown(&r).contains("incomplètes : 2"));
}

fn floors_zero() -> Floors {
    Floors {
        corpus: 0,
        oracle: 0,
        total: 0,
    }
}

#[test]
fn hash_mismatch_and_missing_files_are_absent() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("corpus-harness-test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("x.xml"), b"abd").unwrap();
    let e = entry("x.xml", b"abc", 0);
    let run = corpus::run_file(&e, &dir);
    assert!(matches!(
        run.status,
        FileStatus::Absent(AbsentReason::HashMismatch { .. })
    ));
    let e = entry("absent.xml", b"abc", 0);
    assert_eq!(
        corpus::run_file(&e, &dir).status,
        FileStatus::Absent(AbsentReason::NotDownloaded)
    );
    // Contenu conforme au manifeste mais illisible : erreur de lecture.
    let e = entry("bad.xml", b"<Projet/>", 0);
    let run = corpus::run_bytes(&e, b"<Projet/>");
    assert!(matches!(run.status, FileStatus::ReadError(_)), "{:?}", run.status);
}

// --- Aller-retour MSPDI (P25) ----------------------------------------------

#[test]
fn p25_round_trip_of_a_sample_and_verdict() {
    // Document écrit à la main (ordre pj15, pas celui de MS Project) :
    // projet identique avec les valeurs relues comme avec celles du moteur ;
    // un fichier `msproject` non identique fait échouer le verdict.
    let xml = sample("2026-10-20T17:00:00");
    let mut e = entry("rt.xml", xml.as_bytes(), 0);
    let run = corpus::run_bytes(&e, xml.as_bytes());
    let doc = run.document.as_ref().unwrap();
    let rt = roundtrip::round_trip(&e, doc, &xml);
    assert!(rt.project_identical_recorded, "{rt:#?}");
    assert!(rt.project_identical_schedule, "{rt:#?}");
    let mut bad = rt;
    bad.msproject = true;
    bad.recorded_identical = false;
    assert_eq!(
        roundtrip::failures(&[bad.clone()]),
        ["rt.xml : aller-retour MSPDI non identique"]
    );
    bad.msproject = false;
    assert_eq!(roundtrip::failures(&[bad]), Vec::<String>::new());
    e.producer = Producer::Msproject;
    let md = roundtrip::markdown(&[roundtrip::round_trip(&e, doc, &xml)]);
    assert!(
        md.contains("## Aller-retour MSPDI") && md.contains("| rt.xml | oui |"),
        "{md}"
    );
}

// --- Ressources (J5, lot L9) ----------------------------------------------

/// Parité des valeurs de ressources (compteur séparé des cas de tâches,
/// hors `FLOOR`) : chaque fichier présent tient son plancher de
/// `resources::FLOORS`, et `CORPUS_RESOURCE_FLOOR` en vaut la somme.
#[test]
fn corpus_resource_floor() {
    use plannercraft_core::parity::CORPUS_RESOURCE_FLOOR;
    use plannercraft_tests::corpus::resources::FLOORS;

    let m = load_manifest();
    let floor_sum: u32 = FLOORS.iter().map(|(_, f)| f).sum();
    assert_eq!(
        CORPUS_RESOURCE_FLOOR, floor_sum,
        "CORPUS_RESOURCE_FLOOR = somme de resources::FLOORS"
    );
    for (name, _) in FLOORS {
        assert!(
            m.file.iter().any(|e| e.name == name),
            "{name} : absent du manifeste"
        );
    }
    let runs = corpus::run(&m.file, &plannercraft_tests::tests_path("corpus/files"));
    let mut failures = Vec::new();
    let mut present = 0_u32;
    for run in &runs {
        let Some(cmp) = &run.resources else { continue };
        present += 1;
        let floor = FLOORS
            .iter()
            .find(|(n, _)| *n == run.entry.name)
            .map_or(0, |(_, f)| *f);
        let identical = cmp.identical_count();
        println!(
            "ressources {} : {} / {} cas identiques (plancher {floor}), {} fictive(s) écartée(s), écartées {:?}",
            run.entry.name,
            identical,
            cmp.case_count(),
            cmp.placeholders,
            cmp.excluded
        );
        for c in cmp.cases.iter().filter(|c| !c.identical()) {
            println!("  {:?} {} : {:?}", c.kind, c.uid, c.divergences);
        }
        if identical < floor {
            failures.push(format!("{} : {identical} < plancher {floor}", run.entry.name));
        }
        if identical > floor {
            println!(
                "{} : plancher de ressources relevable ({floor} → {identical})",
                run.entry.name
            );
        }
    }
    let all: Vec<_> = runs
        .iter()
        .filter_map(|r| Some((r.entry.name.as_str(), r.resources.as_ref()?)))
        .collect();
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"));
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("corpus-resources.json"),
        serde_json::to_string_pretty(&all).unwrap(),
    )
    .unwrap();
    if present == 0 {
        let required = std::env::var("PLANNERCRAFT_CORPUS").is_ok_and(|v| v == "required");
        assert!(!required, "corpus absent sous PLANNERCRAFT_CORPUS=required");
        eprintln!("avertissement : corpus absent, parité des ressources non mesurée");
    }
    assert!(
        failures.is_empty(),
        "parité des ressources en échec : {failures:#?}"
    );
}

/// `sample` plus une ressource affectée à A (8 h, lun 19/10) et une
/// affectation fictive −65 535 sur B ; `finish` remplace la fin enregistrée
/// de l'affectation de A.
fn sample_with_resource(finish: &str) -> String {
    let xml = sample("2026-10-20T17:00:00").replacen(
        "<Name>Alpha confidentiel</Name><OutlineLevel>1</OutlineLevel><Duration>PT8H0M0S</Duration><DurationFormat>7</DurationFormat>",
        "<Name>Alpha confidentiel</Name><OutlineLevel>1</OutlineLevel><Duration>PT8H0M0S</Duration><DurationFormat>7</DurationFormat><Work>PT8H0M0S</Work>",
        1,
    );
    assert!(xml.contains("<Work>PT8H0M0S</Work>"));
    xml.replace(
        "</Tasks>",
        &format!(
            "</Tasks><Resources><Resource><UID>1</UID><ID>1</ID><Name>R</Name><Type>1</Type>\
<MaxUnits>1.00</MaxUnits><Work>PT8H0M0S</Work><OverAllocated>0</OverAllocated></Resource></Resources>\
<Assignments><Assignment><UID>1</UID><TaskUID>1</TaskUID><ResourceUID>1</ResourceUID><Units>1</Units>\
<Work>PT8H0M0S</Work><Start>2026-10-19T08:00:00</Start><Finish>{finish}</Finish></Assignment>\
<Assignment><UID>2</UID><TaskUID>2</TaskUID><ResourceUID>-65535</ResourceUID><Units>1</Units>\
<Work>PT8H0M0S</Work><Start>2026-10-20T08:00:00</Start><Finish>2026-10-20T17:00:00</Finish></Assignment></Assignments>"
        ),
    )
}

/// Harnais des ressources : un projet cohérent est identique (un cas de
/// tâche à trois valeurs, un cas de ressource à deux), l'affectation fictive
/// n'est jamais comparée, et une fin d'affectation fausse est nommée.
#[test]
fn resource_comparison_counts_cases_and_reports_divergences() {
    use plannercraft_tests::corpus::resources::{ResourceCaseKind, ResourceField};

    let xml = sample_with_resource("2026-10-19T17:00:00");
    let run = corpus::run_bytes(&entry("res.xml", xml.as_bytes(), 0), xml.as_bytes());
    let cmp = run.resources.as_ref().expect("comparé");
    assert_eq!((cmp.case_count(), cmp.identical_count()), (2, 2), "{cmp:#?}");
    assert_eq!(cmp.placeholders, 1);
    let shape: Vec<_> = cmp.cases.iter().map(|c| (c.kind, c.uid, c.compared)).collect();
    assert_eq!(
        shape,
        [(ResourceCaseKind::Leaf, 1, 3), (ResourceCaseKind::Resource, 1, 2)]
    );

    let xml = sample_with_resource("2026-10-19T16:00:00");
    let run = corpus::run_bytes(&entry("res-diff.xml", xml.as_bytes(), 0), xml.as_bytes());
    let cmp = run.resources.as_ref().unwrap();
    assert_eq!(cmp.identical_count(), 1);
    let d = &cmp.cases[0].divergences;
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(
        (
            d[0].field,
            d[0].assignment,
            d[0].recorded.as_str(),
            d[0].computed.as_str()
        ),
        (
            ResourceField::AssignmentFinish,
            Some(1),
            "2026-10-19T16:00",
            "2026-10-19T17:00"
        )
    );
}
