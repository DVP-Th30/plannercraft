//! R-132 — Contraintes, échéance, option des contraintes et sens à
//! l'écriture.

use plannercraft_core::{Constraint, ScheduleAnchor, Task, TaskUid};
use plannercraft_tests::moment;
use plannercraft_tests::mspdi::{compact, new_project, reread, text, written};

#[test]
fn r132_constraint_type_and_date() {
    let mut proj = new_project("2026-10-19T08:00");
    let opts = proj.options;
    let mut a = Task::new(TaskUid::new(1), "A".to_owned(), &opts);
    a.constraint = Constraint::StartNoEarlierThan {
        date: moment("2026-10-21T13:00"),
    };
    a.deadline = Some(moment("2026-10-23T17:00"));
    proj.tasks.push(a).unwrap();
    let mut b = Task::new(TaskUid::new(2), "B".to_owned(), &opts);
    b.constraint = Constraint::AsLateAsPossible;
    proj.tasks.push(b).unwrap();
    proj.options.honor_constraints = false;
    let out = written(&proj);
    let xml = compact(&text(&out));
    assert!(xml.contains("<ConstraintType>4</ConstraintType>"));
    assert!(xml.contains("<ConstraintDate>2026-10-21T13:00:00</ConstraintDate>"));
    assert!(xml.contains("<Deadline>2026-10-23T17:00:00</Deadline>"));
    assert!(xml.contains("<ConstraintType>1</ConstraintType>"));
    assert_eq!(xml.matches("<ConstraintDate>").count(), 1);
    assert!(xml.contains("<HonorConstraints>0</HonorConstraints>"));
    let back = reread(&out.xml).project;
    assert_eq!(
        back.tasks.get(TaskUid::new(1)).unwrap().constraint,
        proj.tasks.get(TaskUid::new(1)).unwrap().constraint
    );
    assert_eq!(
        back.tasks.get(TaskUid::new(1)).unwrap().deadline,
        Some(moment("2026-10-23T17:00"))
    );
    assert_eq!(
        back.tasks.get(TaskUid::new(2)).unwrap().constraint,
        Constraint::AsLateAsPossible
    );
    assert!(!back.options.honor_constraints);
}

#[test]
fn r132_schedule_from_finish() {
    let mut proj = new_project("2026-10-19T08:00");
    proj.anchor = ScheduleAnchor::FromFinish(moment("2026-10-23T17:00"));
    let out = written(&proj);
    let xml = compact(&text(&out));
    assert!(xml.contains("<ScheduleFromStart>0</ScheduleFromStart>"));
    assert!(xml.contains("<FinishDate>2026-10-23T17:00:00</FinishDate>"));
    // Sans valeurs calculées, le début (calculé) n'est pas écrit.
    assert!(!xml.contains("<StartDate>"));
    assert_eq!(reread(&out.xml).project.anchor, proj.anchor);
}
