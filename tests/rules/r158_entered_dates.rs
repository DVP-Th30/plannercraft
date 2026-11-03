//! R-158 — Début ou fin saisis → contrainte (SNET/FNET depuis le début,
//! SNLT/FNLT à rebours ; effacés → ASAP/ALAP).

use plannercraft_automation::SideEffect;
use plannercraft_automation::error::codes;
use plannercraft_core::{
    BuiltinCalendarNames, Constraint, ConstraintKind, Project, ProjectInit, ScheduleAnchor, TaskUid,
};
use plannercraft_tests::moment;
use plannercraft_tests::mspdi::new_project;
use plannercraft_tests::session::{link, push_tasks, session};
use plannercraft_tests::task_edit::{apply, get, try_apply};
use serde_json::json;

fn uid(u: u32) -> TaskUid {
    TaskUid::new(u)
}

fn backward() -> Project {
    let mut p = Project::new(ProjectInit {
        name: "P".to_owned(),
        anchor: ScheduleAnchor::FromFinish(moment("2026-11-30T17:00")),
        current_date: moment("2026-10-17T08:00"),
        calendar_names: BuiltinCalendarNames::default(),
    });
    push_tasks(&mut p, &[1], 1);
    p.tasks.get_mut(uid(1)).unwrap().constraint = Constraint::AsLateAsPossible;
    p
}

#[test]
fn r158_start_snet() {
    // Projet du lun 02/11/2026 ; `update 3 start 2026-11-04T08:00` → SNET,
    // début planifié mer 04/11 08:00 (aucun lien ne pousse plus tard).
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1, 2, 3], 1);
    let mut s = session(p);
    let o = apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 3, "start": "2026-11-04T08:00" }]),
    );
    assert_eq!(
        get(&s, 3).constraint,
        Constraint::StartNoEarlierThan {
            date: moment("2026-11-04T08:00")
        }
    );
    assert!(o.side_effects.contains(&SideEffect::ConstraintSet {
        uid: uid(3),
        kind: ConstraintKind::StartNoEarlierThan
    }));
    assert_eq!(
        s.schedule().scheduled(uid(3)).unwrap().start,
        moment("2026-11-04T08:00")
    );
    // Un lien qui pousse plus tard l'emporte : 1 et 2 de 3 j, 1 → 2 ; SNET
    // mar 03/11 sur 2.
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1, 2], 3);
    link(&mut p, 1, 2);
    let mut s = session(p);
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 2, "start": "2026-11-03T08:00" }]),
    );
    // 1 finit le mer 04/11 17:00 ; 2 commence le jeu 05/11 08:00.
    assert_eq!(
        s.schedule().scheduled(uid(2)).unwrap().start,
        moment("2026-11-05T08:00")
    );
    // À l'ajout aussi.
    let o = apply(
        &mut s,
        json!([{ "op": "task.add", "name": "N", "start": "2026-11-10T08:00" }]),
    );
    let n = o.created[0];
    assert_eq!(
        s.project().tasks.get(n).unwrap().constraint,
        Constraint::StartNoEarlierThan {
            date: moment("2026-11-10T08:00")
        }
    );
}

#[test]
fn r158_finish_fnet() {
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1], 1);
    p.tasks.get_mut(uid(1)).unwrap().constraint = Constraint::MustStartOn {
        date: moment("2026-11-03T08:00"),
    };
    let mut s = session(p);
    // Une contrainte de date déjà présente est remplacée.
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 1, "finish": "2026-11-06T17:00" }]),
    );
    assert_eq!(
        get(&s, 1).constraint,
        Constraint::FinishNoEarlierThan {
            date: moment("2026-11-06T17:00")
        }
    );
    assert_eq!(
        s.schedule().scheduled(uid(1)).unwrap().finish,
        moment("2026-11-06T17:00")
    );
    // Début et fin dans la même opération : refus (une seule contrainte).
    let err = try_apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 1, "start": "2026-11-03T08:00", "finish": "2026-11-06T17:00" }]),
    )
    .unwrap_err();
    assert_eq!(err.code, codes::PARAM_INVALID);
}

#[test]
fn r158_backward_snlt_fnlt() {
    let mut s = session(backward());
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 1, "start": "2026-11-10T08:00" }]),
    );
    assert_eq!(
        get(&s, 1).constraint,
        Constraint::StartNoLaterThan {
            date: moment("2026-11-10T08:00")
        }
    );
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 1, "finish": "2026-11-20T17:00" }]),
    );
    assert_eq!(
        get(&s, 1).constraint,
        Constraint::FinishNoLaterThan {
            date: moment("2026-11-20T17:00")
        }
    );
}

#[test]
fn r158_clear_asap() {
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1], 1);
    let mut s = session(p);
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 1, "start": "2026-11-04T08:00" }]),
    );
    let o = apply(&mut s, json!([{ "op": "task.update", "uid": 1, "start": null }]));
    assert_eq!(get(&s, 1).constraint, Constraint::AsSoonAsPossible);
    assert!(o.side_effects.contains(&SideEffect::ConstraintSet {
        uid: uid(1),
        kind: ConstraintKind::AsSoonAsPossible
    }));
    // À rebours : ALAP.
    let mut s = session(backward());
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 1, "finish": "2026-11-20T17:00" }]),
    );
    apply(&mut s, json!([{ "op": "task.update", "uid": 1, "finish": null }]));
    assert_eq!(get(&s, 1).constraint, Constraint::AsLateAsPossible);
}
