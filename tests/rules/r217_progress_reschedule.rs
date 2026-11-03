//! R-217 — `progress.reschedule` (« replanifier le travail restant après »,
//! S33 « Update Project ») : reprise posée sur une tâche en cours dont la
//! partie réelle s'arrête avant `after`, SNET `after` sur une tâche non
//! commencée en retard (contrainte ASAP ou SNET plus tôt), tâche à une autre
//! contrainte sautée avec un effet ; dates réelles jamais modifiées.
//! Repère : calendrier Standard, début du projet lun 19/10/2026 08:00,
//! date d'état ven 13/11/2026 17:00.

use plannercraft_automation::{Session, SideEffect};
use plannercraft_core::{Constraint, Moment, SchedulingMode, TaskOutcome, TaskSchedule, TaskUid};
use plannercraft_tests::moment;
use plannercraft_tests::mspdi::new_project;
use plannercraft_tests::resource_edit::with_tasks;
use plannercraft_tests::session::{session, task};
use plannercraft_tests::task_edit::{apply, get, try_apply};
use serde_json::json;

/// Planning de la tâche `uid`.
fn sched(s: &Session, uid: u32) -> TaskSchedule {
    match s.schedule().get(TaskUid::new(uid)) {
        Some(TaskOutcome::Scheduled(t)) => *t,
        other => panic!("tâche {uid} non planifiée : {other:?}"),
    }
}

/// Reprise calculée de la tâche `uid` (R-204).
fn resume(s: &Session, uid: u32) -> Moment {
    sched(s, uid).progress.unwrap().resume
}

/// T 20 j commencée lun 19/10 avec 17 j réels (`Stop` mar 10/11 17:00) ;
/// U 1 j SNET jeu 12/11 08:00 ; V 1 j MFO jeu 12/11 17:00 ; W 1 j non
/// commencée (début lun 19/10) ; C 1 j achevée ; date d'état ven 13/11
/// 17:00.
fn project() -> Session {
    let mut s = with_tasks(&[("T", "20j"), ("U", "1j"), ("V", "1j"), ("W", "1j"), ("C", "1j")]);
    apply(
        &mut s,
        json!([
            { "op": "progress.update", "task": 1, "actual_start": "2026-10-19T08:00",
              "actual_duration": { "text": "17j" } },
            { "op": "task.set_constraint", "uid": 2, "kind": "snet", "date": "2026-11-12T08:00" },
            { "op": "task.set_constraint", "uid": 3, "kind": "mfo", "date": "2026-11-12T17:00" },
            { "op": "progress.update", "task": 5, "percent_complete": { "text": "100%" } },
            { "op": "project.set_status_date", "date": "2026-11-13T17:00" }
        ]),
    );
    let t = sched(&s, 1).progress.unwrap();
    assert_eq!(t.stop, moment("2026-11-10T17:00"));
    assert_eq!(t.resume, moment("2026-11-11T08:00"));
    assert_eq!(sched(&s, 2).start, moment("2026-11-12T08:00"));
    s
}

#[test]
fn r217_resume_set_in_progress() {
    let mut s = project();
    apply(&mut s, json!([{ "op": "progress.reschedule" }]));
    // Reprise saisie = date d'état ; le moteur la recale sur l'instant
    // ouvré suivant : lun 16/11 08:00 ; reste 3 j ⇒ fin mer 18/11 17:00.
    assert_eq!(get(&s, 1).progress.resume, Some(moment("2026-11-13T17:00")));
    assert_eq!(resume(&s, 1), moment("2026-11-16T08:00"));
    assert_eq!(sched(&s, 1).finish, moment("2026-11-18T17:00"));
    // Reprise saisie plus tardive : gardée.
    let mut s = project();
    apply(
        &mut s,
        json!([{ "op": "progress.reschedule", "after": "2026-11-12T17:00" },
               { "op": "progress.reschedule", "after": "2026-11-11T12:00" }]),
    );
    assert_eq!(get(&s, 1).progress.resume, Some(moment("2026-11-12T17:00")));
}

#[test]
fn r217_snet_on_unstarted() {
    let mut s = project();
    let out = apply(&mut s, json!([{ "op": "progress.reschedule" }]));
    let after = moment("2026-11-13T17:00");
    for uid in [2, 4] {
        assert_eq!(
            get(&s, uid).constraint,
            Constraint::StartNoEarlierThan { date: after },
            "{uid}"
        );
        assert_eq!(sched(&s, uid).start, moment("2026-11-16T08:00"), "{uid}");
        assert!(out.side_effects.contains(&SideEffect::ConstraintSet {
            uid: TaskUid::new(uid),
            kind: plannercraft_core::ConstraintKind::StartNoEarlierThan,
        }));
    }
    // Sélection : seule W ; U garde sa SNET plus tôt. Début déjà après
    // `after` : rien.
    let mut s = project();
    apply(&mut s, json!([{ "op": "progress.reschedule", "tasks": [4] }]));
    assert_eq!(
        get(&s, 2).constraint,
        Constraint::StartNoEarlierThan {
            date: moment("2026-11-12T08:00")
        }
    );
    assert_eq!(sched(&s, 4).start, moment("2026-11-16T08:00"));
    let out = apply(
        &mut s,
        json!([{ "op": "progress.reschedule", "after": "2026-11-13T17:00", "tasks": [4] }]),
    );
    assert_eq!(out.side_effects.len(), 0);
    let err = try_apply(&mut s, json!([{ "op": "progress.reschedule", "tasks": [42] }])).unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref()),
        (
            plannercraft_automation::error::codes::TASK_NOT_FOUND,
            Some("tasks")
        )
    );
}

#[test]
fn r217_constraint_skipped() {
    let mut s = project();
    let out = apply(&mut s, json!([{ "op": "progress.reschedule" }]));
    assert!(out.side_effects.contains(&SideEffect::RescheduleSkipped {
        task: TaskUid::new(3),
        reason: "constraint",
    }));
    assert_eq!(
        get(&s, 3).constraint,
        Constraint::MustFinishOn {
            date: moment("2026-11-12T17:00")
        }
    );
    // Tâche manuelle en retard : sautée, le moteur ne la replace pas.
    let mut p = new_project("2026-10-19T08:00");
    let mut m = task(&p, 1, 1);
    m.mode = SchedulingMode::Manual {
        start: Some(moment("2026-10-19T08:00")),
        finish: None,
        duration: None,
    };
    p.tasks.push(m).unwrap();
    let mut s = session(p);
    let out = apply(
        &mut s,
        json!([{ "op": "progress.reschedule", "after": "2026-10-21T08:00" }]),
    );
    assert_eq!(
        out.side_effects,
        [SideEffect::RescheduleSkipped {
            task: TaskUid::new(1),
            reason: "manual",
        }]
    );
}

#[test]
fn r217_actuals_untouched() {
    let mut s = project();
    let before_t = get(&s, 1).progress;
    let before_c = get(&s, 5).clone();
    let c = sched(&s, 5);
    apply(&mut s, json!([{ "op": "progress.reschedule" }]));
    let t = get(&s, 1).progress;
    assert_eq!(
        (t.actual_start, t.actual_finish, t.actual_duration),
        (
            before_t.actual_start,
            before_t.actual_finish,
            before_t.actual_duration
        )
    );
    assert_eq!(sched(&s, 1).start, moment("2026-10-19T08:00"));
    assert_eq!(get(&s, 5), &before_c);
    assert_eq!((sched(&s, 5).start, sched(&s, 5).finish), (c.start, c.finish));
}
