//! R-071 — `DurationFormat` d'une tâche : unité, base, estimation (import
//! MSPDI).

use plannercraft_core::{DurationUnit, Minutes, TaskUid, TimeBasis};
use plannercraft_mspdi::{ImportCode, MspdiDocument, ReadOptions, read};
use plannercraft_tests::{moment, tests_path};

fn fixture() -> MspdiDocument {
    let bytes = std::fs::read(tests_path("fixtures/mspdi/tasks.xml")).unwrap();
    read(&bytes, &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap()
}

fn duration(doc: &MspdiDocument, uid: u32) -> plannercraft_core::Duration {
    doc.project.tasks.get(TaskUid::new(uid)).unwrap().duration
}

#[test]
fn r071_estimated_day_code() {
    // UID 2 : `PT40H0M0S` + 39 (« j? ») → 2 400 min, jour, ouvrée, estimée.
    let d = duration(&fixture(), 2);
    assert_eq!(d.minutes(), Minutes::new(2_400));
    assert_eq!(d.unit(), DurationUnit::Day);
    assert_eq!(d.basis(), TimeBasis::Working);
    assert!(d.is_estimated());
}

#[test]
fn r071_elapsed_day_code() {
    // UID 3 : `PT120H0M0S` + 8 (« ej ») → 7 200 min (5 ej), écoulée.
    let d = duration(&fixture(), 3);
    assert_eq!(d.minutes(), Minutes::new(7_200));
    assert_eq!(d.unit(), DurationUnit::Day);
    assert_eq!(d.basis(), TimeBasis::Elapsed);
    assert!(!d.is_estimated());
}

#[test]
fn r071_estimated_element_wins_over_code() {
    // UID 4 : code 7 (« j », non estimé) mais `Estimated 1` → estimée.
    let d = duration(&fixture(), 4);
    assert_eq!(d.minutes(), Minutes::new(480));
    assert!(d.is_estimated());
}

#[test]
fn r071_null_format_uses_project_unit() {
    // UID 7 : code 21 (nul) → unité par défaut du projet (`DurationFormat` 7
    // du projet : jour), 960 min.
    let d = duration(&fixture(), 7);
    assert_eq!(d.minutes(), Minutes::new(960));
    assert_eq!(d.unit(), DurationUnit::Day);
    // Projet en heures (`DurationFormat` 5) : la tâche nulle prend l'heure.
    let xml = "<Project><StartDate>2026-10-19T08:00:00</StartDate><DurationFormat>5</DurationFormat><Tasks>\
               <Task><UID>1</UID><ID>1</ID><Duration>PT2H0M0S</Duration><DurationFormat>53</DurationFormat></Task>\
               </Tasks></Project>";
    let doc = read(xml.as_bytes(), &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap();
    let d = duration(&doc, 1);
    assert_eq!(d.unit(), DurationUnit::Hour);
    assert!(d.is_estimated());
}

#[test]
fn r071_percent_code_on_task_duration() {
    // UID 6 : code 19 (« % ») sur une durée de tâche → jour, signalé.
    let doc = fixture();
    assert_eq!(duration(&doc, 6).unit(), DurationUnit::Day);
    assert!(doc.diagnostics.iter().any(|d| d.code == ImportCode::InvalidValue
        && d.element == "Tasks/Task/DurationFormat"
        && d.value.as_deref() == Some("19")));
}

#[test]
fn r071_summary_duration_is_recorded_only() {
    // Récapitulative du projet : `PT80H0M0S` + 21 → 4 800 min enregistrées,
    // jour ; la durée d'une récapitulative automatique est calculée (R-054),
    // elle ne va pas dans le modèle.
    let doc = fixture();
    let rec = doc.recorded.tasks.get(&TaskUid::new(0)).unwrap();
    assert_eq!(rec.duration, Some(Minutes::new(4_800)));
    let summary = doc.project.summary_task.as_ref().unwrap();
    assert_eq!(summary.duration.minutes(), Minutes::ZERO);
    assert_eq!(summary.duration.unit(), DurationUnit::Day);
    // Récapitulative UID 10 (niveau 1, suivie de tâches de niveau 2) : idem.
    assert_eq!(duration(&doc, 10).minutes(), Minutes::ZERO);
    assert_eq!(
        doc.recorded.tasks.get(&TaskUid::new(10)).unwrap().duration,
        Some(Minutes::new(2_400))
    );
}
