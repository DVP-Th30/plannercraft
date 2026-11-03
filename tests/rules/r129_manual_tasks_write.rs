//! R-129 — Tâches manuelles à l'écriture.

use plannercraft_core::{Duration, DurationUnit, Minutes, SchedulingMode, Task, TaskUid, TimeBasis};
use plannercraft_tests::moment;
use plannercraft_tests::mspdi::{compact, new_project, reread, text, written};

#[test]
fn r129_manual_fields_always_written() {
    let mut proj = new_project("2024-10-04T09:00");
    let opts = proj.options;
    let mut m = Task::new(TaskUid::new(7), "M".to_owned(), &opts);
    m.mode = SchedulingMode::Manual {
        start: Some(moment("2024-10-12T09:00")),
        finish: Some(moment("2024-10-18T18:00")),
        duration: None,
    };
    proj.tasks.push(m).unwrap();
    let mut n = Task::new(TaskUid::new(8), "N".to_owned(), &opts);
    n.mode = SchedulingMode::Manual {
        start: None,
        finish: None,
        duration: Some(
            Duration::new(Minutes::new(960), DurationUnit::Day, TimeBasis::Working, true).unwrap(),
        ),
    };
    proj.tasks.push(n).unwrap();
    let out = written(&proj);
    let xml = compact(&text(&out));
    assert!(xml.contains(
        "<ManualStart>2024-10-12T09:00:00</ManualStart><ManualFinish>2024-10-18T18:00:00</ManualFinish><ManualDuration/>"
    ));
    assert!(xml.contains("<ManualStart/><ManualFinish/><ManualDuration>PT16H0M0S</ManualDuration>"));
    assert!(xml.contains("<Manual>1</Manual>"));
    // Relu : vide = non saisi, sans repli (amendement R-085).
    let back = reread(&out.xml).project;
    assert_eq!(
        back.tasks.get(TaskUid::new(7)).unwrap().mode,
        proj.tasks.get(TaskUid::new(7)).unwrap().mode
    );
    assert_eq!(
        back.tasks.get(TaskUid::new(8)).unwrap().mode,
        proj.tasks.get(TaskUid::new(8)).unwrap().mode
    );
}

#[test]
fn r129_automatic_task_has_no_manual_fields() {
    let mut proj = new_project("2026-10-19T08:00");
    let opts = proj.options;
    proj.tasks
        .push(Task::new(TaskUid::new(1), "A".to_owned(), &opts))
        .unwrap();
    let xml = text(&written(&proj));
    assert!(xml.contains("<Manual>0</Manual>"));
    assert!(!xml.contains("<ManualStart"));
}
