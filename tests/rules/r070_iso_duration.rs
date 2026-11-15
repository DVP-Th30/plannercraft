//! R-070 — Durée ISO 8601 → dixièmes de minute (import MSPDI, ADR-0039).

use plannercraft_mspdi::scalar::{IsoDurationError, parse_iso_duration};

/// Minutes entières d'une durée qui en est une.
fn minutes(s: &str) -> i64 {
    parse_iso_duration(s).unwrap().minutes().whole_minutes().unwrap()
}

#[test]
fn r070_hours_minutes_days() {
    // 80 h = 4 800 min (10 jours de 480) ; 8 h 30 = 510.
    assert_eq!(minutes("PT80H0M0S"), 4_800);
    assert_eq!(minutes("PT8H30M0S"), 510);
    assert_eq!(minutes("PT0H0M0S"), 0);
    // Un jour ISO vaut 24 h, pas `MinutesPerDay` : P2D = 2 880.
    assert_eq!(minutes("P2D"), 2_880);
}

#[test]
fn r070_decimals_in_last_component() {
    // 1,5 h = 90 min ; décimale ailleurs que dans la dernière composante : refus.
    assert_eq!(minutes("PT1.5H"), 90);
    assert_eq!(parse_iso_duration("PT1.5H30M"), Err(IsoDurationError::Syntax));
}

#[test]
fn r070_seconds_to_the_tenth() {
    // ADR-0039 : au dixième de minute (6 s). 30 s = 5 dixièmes exacts (avant :
    // 1 min, arrondi signalé) ; 29 s → 5 dixièmes, arrondi signalé (avant :
    // 0 min) ; un travail se lit au millième (0,06 s).
    let d = parse_iso_duration("PT0H0M30S").unwrap();
    assert_eq!((d.tenths, d.tenth_rounded), (5, false));
    let d = parse_iso_duration("PT0H0M29S").unwrap();
    assert_eq!((d.tenths, d.tenth_rounded), (5, true));
    assert_eq!((d.milli, d.milli_rounded), (483, true));
    let d = parse_iso_duration("PT1H52M30S").unwrap();
    assert_eq!((d.tenths, d.tenth_rounded), (1_125, false));
}

#[test]
fn r070_years_and_months_refused() {
    // Longueur variable : refusés (l'import applique une durée nulle avec un
    // diagnostic `mspdi.invalid_value`).
    assert_eq!(parse_iso_duration("P1M"), Err(IsoDurationError::YearsOrMonths));
    assert_eq!(parse_iso_duration("P1Y"), Err(IsoDurationError::YearsOrMonths));
}

#[test]
fn r070_negative_is_read_signed() {
    // −8 h se lit −480 ; c'est la tâche qui refuse une durée négative.
    assert_eq!(minutes("-PT8H"), -480);
}

/// Tâches de la fixture `tasks.xml`.
fn tasks_fixture() -> plannercraft_mspdi::MspdiDocument {
    let bytes = std::fs::read(plannercraft_tests::tests_path("fixtures/mspdi/tasks.xml")).unwrap();
    plannercraft_mspdi::read(
        &bytes,
        &plannercraft_mspdi::ReadOptions::new(plannercraft_tests::moment("2026-01-01T00:00")),
    )
    .unwrap()
}

#[test]
fn r070_task_duration_refusals() {
    // UID 6 : `-PT8H0M0S` → durée 0, signalée ; UID 8 : `P1M` → 0, signalée.
    let doc = tasks_fixture();
    for uid in [6, 8] {
        let t = doc
            .project
            .tasks
            .get(plannercraft_core::TaskUid::new(uid))
            .unwrap();
        assert!(t.duration.is_zero(), "{uid}");
    }
    let invalid: Vec<Option<&str>> = doc
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == plannercraft_mspdi::ImportCode::InvalidValue && d.element == "Tasks/Task/Duration"
        })
        .map(|d| d.value.as_deref())
        .collect();
    assert_eq!(invalid, [Some("-PT8H0M0S"), Some("P1M")]);
}
