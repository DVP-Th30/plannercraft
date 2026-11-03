//! R-083 — Valeurs enregistrées par MS Project, marges en dixièmes de minute
//! (import MSPDI).

use plannercraft_core::{Minutes, TaskUid};
use plannercraft_mspdi::{ImportCode, MspdiDocument, ReadOptions, RecordedTask, Tenths, read};
use plannercraft_tests::{moment, tests_path};

fn fixture() -> MspdiDocument {
    let bytes = std::fs::read(tests_path("fixtures/mspdi/tasks.xml")).unwrap();
    read(&bytes, &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap()
}

fn rec(doc: &MspdiDocument, uid: u32) -> RecordedTask {
    *doc.recorded.tasks.get(&TaskUid::new(uid)).unwrap()
}

#[test]
fn r083_slack_kept_in_tenths() {
    let doc = fixture();
    // 9 600 dixièmes = 960 min ; 768 = 76,8 min, gardés sans arrondi.
    let a = rec(&doc, 2);
    assert_eq!(a.total_slack, Some(Tenths(9_600)));
    assert_eq!(a.free_slack, Some(Tenths(768)));
    // 43 200 dixièmes = 4 320 min (récapitulative du projet).
    assert_eq!(rec(&doc, 0).total_slack, Some(Tenths(43_200)));
}

#[test]
fn r083_recorded_dates_and_flags() {
    let doc = fixture();
    let a = rec(&doc, 2);
    assert_eq!(a.early_start, Some(moment("2026-10-19T08:00")));
    assert_eq!(a.early_finish, Some(moment("2026-10-23T17:00")));
    assert_eq!(a.late_start, Some(moment("2026-10-21T08:00")));
    assert_eq!(a.late_finish, Some(moment("2026-10-27T17:00")));
    assert_eq!(a.start, Some(moment("2026-10-19T08:00")));
    assert_eq!(a.critical, Some(false));
    assert_eq!(a.summary, Some(false));
    assert_eq!(a.duration, Some(Minutes::new(2_400)));
    // Absents : aucune valeur.
    assert_eq!(rec(&doc, 3).total_slack, None);
    // Une entrée par tâche lue, UID 0 compris.
    assert_eq!(doc.recorded.tasks.len(), 9);
    assert_eq!(doc.recorded.project_finish, Some(moment("2026-10-30T17:00")));
}

#[test]
fn r083_leveling_delay_goes_to_the_model() {
    // UID 4 : `LevelingDelay` 4 800 dixièmes, sans format : une entrée du
    // modèle depuis J5 (R-198), plus une valeur enregistrée ni une fonction
    // non prise en charge ; UID 2 : 0 → aucun retard.
    let doc = fixture();
    let task = |uid: u32| doc.project.tasks.get(TaskUid::new(uid)).unwrap();
    let delay = task(4).leveling_delay.unwrap();
    assert_eq!(delay.minutes, Minutes::new(480));
    assert_eq!(task(2).leveling_delay, None);
    assert!(
        !doc.diagnostics
            .iter()
            .any(|d| matches!(&d.code, ImportCode::Unsupported { what } if what == "leveling_delay"))
    );
}

#[test]
fn r083_assignments_counted() {
    // Deux affectations sur UID 2, une sur UID 3 ; TaskUID 42 inconnu.
    let doc = fixture();
    assert_eq!(rec(&doc, 2).assignments, 2);
    assert_eq!(rec(&doc, 3).assignments, 1);
    assert_eq!(rec(&doc, 4).assignments, 0);
    assert!(doc.diagnostics.iter().any(|d| d.code == ImportCode::DanglingReference
        && d.element == "Assignments/Assignment/TaskUID"));
}

#[test]
fn r083_start_and_finish_slack_are_recorded() {
    // `StartSlack` et `FinishSlack` lus en dixièmes, sans arrondi, dans le
    // planning enregistré (réécrits tels quels avec `Computed::Recorded`,
    // R-122).
    let bytes = std::fs::read(plannercraft_tests::tests_path("fixtures/mspdi/resources.xml")).unwrap();
    let doc = plannercraft_mspdi::read(
        &bytes,
        &plannercraft_mspdi::ReadOptions::new(plannercraft_tests::moment("2026-01-01T00:00")),
    )
    .unwrap();
    let rec = &doc.recorded.tasks[&plannercraft_core::TaskUid::new(1)];
    assert_eq!(rec.start_slack, Some(plannercraft_mspdi::Tenths(9_600)));
    assert_eq!(rec.finish_slack, Some(plannercraft_mspdi::Tenths(4_800)));
    assert_eq!(
        doc.recorded.tasks[&plannercraft_core::TaskUid::new(2)].start_slack,
        None
    );
}
