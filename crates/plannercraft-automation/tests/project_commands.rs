//! Commandes du projet (lot J4 L5) : `project.set_info`,
//! `project.set_anchor`, `project.set_status_date`. Aucune n'est une règle
//! de calcul : elles posent des entrées du moteur.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use plannercraft_automation::text::Language;
use plannercraft_automation::{ApplyMode, ApplyOutcome, Batch, CommandError, DocOrigin, Session};
use plannercraft_core::{
    BuiltinCalendarNames, Constraint, Moment, Project, ProjectInit, ScheduleAnchor, Task, TaskUid,
};

fn moment(s: &str) -> Moment {
    s.parse().unwrap()
}

/// Projet depuis le lun 02/11/2026 08:00, tâche 1 « A » (1 jour estimé).
fn session() -> Session {
    let mut p = Project::new(ProjectInit {
        name: "P".to_owned(),
        anchor: ScheduleAnchor::FromStart(moment("2026-11-02T08:00")),
        current_date: moment("2026-10-17T08:00"),
        calendar_names: BuiltinCalendarNames::default(),
    });
    let t = Task::new(TaskUid::new(1), "A".to_owned(), &p.options);
    p.tasks.push(t).unwrap();
    let origin = DocOrigin {
        name: "P".to_owned(),
        path: None,
    };
    Session::from_project(p, origin, moment("2026-10-17T08:00"), Language::Fr)
}

fn run(s: &mut Session, ops: &str) -> Result<ApplyOutcome, CommandError> {
    let batch = Batch::from_json_str(&format!(r#"{{"format_version":1,"ops":[{ops}]}}"#)).unwrap();
    s.apply(&batch, ApplyMode::Commit)
}

#[test]
fn project_set_anchor_backward() {
    // À rebours depuis le ven 20/11 17:00 : la tâche finit à l'ancre, sa
    // contrainte (ASAP) n'est pas changée ; annuler rétablit le sens.
    let mut s = session();
    run(
        &mut s,
        r#"{"op":"project.set_anchor","finish":"2026-11-20T17:00"}"#,
    )
    .unwrap();
    assert_eq!(
        s.project().anchor,
        ScheduleAnchor::FromFinish(moment("2026-11-20T17:00"))
    );
    assert_eq!(
        s.project().tasks.get(TaskUid::new(1)).unwrap().constraint,
        Constraint::AsSoonAsPossible
    );
    assert_eq!(s.schedule().project_finish, moment("2026-11-20T17:00"));
    assert_eq!(s.schedule().project_start, moment("2026-11-20T08:00"));
    // Retour depuis le début.
    run(
        &mut s,
        r#"{"op":"project.set_anchor","start":"2026-11-09T08:00"}"#,
    )
    .unwrap();
    assert_eq!(s.schedule().project_finish, moment("2026-11-09T17:00"));
    s.undo().unwrap();
    assert_eq!(
        s.project().anchor,
        ScheduleAnchor::FromFinish(moment("2026-11-20T17:00"))
    );
    // Les deux ancres, ou aucune : refus à la lecture.
    for json in [
        r#"{"format_version":1,"ops":[{"op":"project.set_anchor"}]}"#,
        r#"{"format_version":1,"ops":[{"op":"project.set_anchor","start":"2026-11-09T08:00","finish":"2026-11-20T17:00"}]}"#,
    ] {
        assert_eq!(Batch::from_json_str(json).unwrap_err().code, "param.invalid");
    }
}

#[test]
fn project_status_date_patch() {
    let mut s = session();
    run(
        &mut s,
        r#"{"op":"project.set_status_date","date":"2026-11-04T17:00"}"#,
    )
    .unwrap();
    assert_eq!(s.project().status_date, Some(moment("2026-11-04T17:00")));
    // Champ absent : gardé.
    run(&mut s, r#"{"op":"project.set_status_date"}"#).unwrap();
    assert_eq!(s.project().status_date, Some(moment("2026-11-04T17:00")));
    // `null` : NA, la date du jour s'applique (R-034).
    run(&mut s, r#"{"op":"project.set_status_date","date":null}"#).unwrap();
    assert_eq!(s.project().status_date, None);
    assert_eq!(s.project().effective_status_date(), moment("2026-10-17T08:00"));
}

#[test]
fn project_set_info_patch() {
    let mut s = session();
    run(
        &mut s,
        r#"{"op":"project.set_info","name":"Chantier","author":"Ana","company":"ACME"}"#,
    )
    .unwrap();
    let info = &s.project().info;
    assert_eq!(
        (info.name.as_str(), info.author.as_str(), info.company.as_str()),
        ("Chantier", "Ana", "ACME")
    );
    run(&mut s, r#"{"op":"project.set_info","author":null,"title":"T"}"#).unwrap();
    let info = &s.project().info;
    assert_eq!(
        (info.name.as_str(), info.author.as_str(), info.title.as_str()),
        ("Chantier", "", "T")
    );
    assert!(s.is_dirty());
    s.undo().unwrap();
    assert_eq!(s.project().info.author, "Ana");
}
