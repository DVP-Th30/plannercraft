//! R-155 — `task.delete` : récapitulative en cascade seulement ; liens et
//! affectations retirés ; ID renumérotés, UID jamais réutilisés.

use plannercraft_automation::error::codes;
use plannercraft_automation::{CommandErrorKind, SideEffect};
use plannercraft_core::{Assignment, AssignmentUid, ExtraEntry, Subject, TaskId, TaskUid};
use plannercraft_tests::mspdi::new_project;
use plannercraft_tests::session::{link, push_tasks, session};
use plannercraft_tests::task_edit::{apply, get, push_outline, rows, try_apply};
use serde_json::json;

fn uid(u: u32) -> TaskUid {
    TaskUid::new(u)
}

#[test]
fn r155_delete_summary_requires_cascade() {
    // [1 P(1), 2 a(2), 3 a1(3), 4 b(2), 5 Q(1)] : P a trois descendants.
    let mut p = new_project("2026-11-02T08:00");
    push_outline(&mut p, &[(1, 1), (2, 2), (3, 3), (4, 2), (5, 1)]);
    let mut s = session(p);
    let err = try_apply(&mut s, json!([{ "op": "task.delete", "uid": 1 }])).unwrap_err();
    assert_eq!(err.code, codes::TASK_HAS_CHILDREN);
    assert_eq!(err.kind, CommandErrorKind::HasChildren { count: 3 });
    assert_eq!(rows(&s).len(), 5);
    // Une feuille se supprime sans `cascade`.
    apply(&mut s, json!([{ "op": "task.delete", "uid": 3 }]));
    assert_eq!(rows(&s), [(1, 1), (2, 2), (4, 2), (5, 1)]);
    // Cascade : tout le sous-arbre part.
    let o = apply(
        &mut s,
        json!([{ "op": "task.delete", "uid": 1, "cascade": true }]),
    );
    assert_eq!(rows(&s), [(5, 1)]);
    let deleted: Vec<_> = o
        .side_effects
        .iter()
        .filter_map(|e| match e {
            SideEffect::TaskDeleted(u) => Some(u.get()),
            _ => None,
        })
        .collect();
    assert_eq!(deleted, [1, 2, 4]);
}

#[test]
fn r155_delete_cascade_removes_links_assignments() {
    // [1 P(1), 2 a(2), 3 b(2), 4 X(1)] ; chaîne a → X → b (feuilles) ;
    // affectations sur b et X.
    let mut p = new_project("2026-11-02T08:00");
    push_outline(&mut p, &[(1, 1), (2, 2), (3, 2), (4, 1)]);
    link(&mut p, 2, 4);
    link(&mut p, 4, 3);
    p.assignments
        .push(Assignment::new(AssignmentUid::new(1), uid(3), None))
        .unwrap();
    p.assignments
        .push(Assignment::new(AssignmentUid::new(2), uid(4), None))
        .unwrap();
    let mut s = session(p);
    let o = apply(
        &mut s,
        json!([{ "op": "task.delete", "uid": 1, "cascade": true }]),
    );
    assert_eq!(rows(&s), [(4, 1)]);
    assert_eq!(get(&s, 4).predecessors, []);
    let removed: Vec<_> = o
        .side_effects
        .iter()
        .filter_map(|e| match e {
            SideEffect::LinkRemoved { pred, succ } => Some((pred.get(), succ.get())),
            _ => None,
        })
        .collect();
    // 4 → 3 part avec 3 (successeur supprimé) ; 2 → 4 retiré de 4.
    assert_eq!(removed, [(4, 3), (2, 4)]);
    assert!(
        o.side_effects
            .contains(&SideEffect::AssignmentRemoved(AssignmentUid::new(1)))
    );
    let kept: Vec<_> = s.project().assignments.iter().map(|a| a.task.get()).collect();
    assert_eq!(kept, [4]);
}

#[test]
fn r155_ids_renumbered() {
    // 5 tâches, 2 → 4 et 4 → 5 ; delete 4 → liens retirés, 5 devient ID 4.
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1, 2, 3, 4, 5], 1);
    link(&mut p, 2, 4);
    link(&mut p, 4, 5);
    let mut s = session(p);
    let o = apply(&mut s, json!([{ "op": "task.delete", "uid": 4 }]));
    assert_eq!(s.project().task_id(uid(5)), Some(TaskId::new(4)));
    assert_eq!(get(&s, 5).predecessors, []);
    assert_eq!(
        o.side_effects,
        [
            SideEffect::TaskDeleted(uid(4)),
            SideEffect::LinkRemoved {
                pred: uid(2),
                succ: uid(4)
            },
            SideEffect::LinkRemoved {
                pred: uid(4),
                succ: uid(5)
            },
            SideEffect::IdsRenumbered {
                from: TaskId::new(4),
                count: 1
            },
        ]
    );
    // Les UID ne sont pas réutilisés (R-151) : la tâche suivante reçoit 6.
    let o = apply(&mut s, json!([{ "op": "task.add", "name": "N" }]));
    assert_eq!(o.created, [uid(6)]);
    // Supprimer la dernière : aucune renumérotation.
    let o = apply(&mut s, json!([{ "op": "task.delete", "uid": 6 }]));
    assert_eq!(o.side_effects, [SideEffect::TaskDeleted(uid(6))]);
}

#[test]
fn r155_effects_order_extras_after_assignments() {
    // 2 → 4, 4 → 5 ; 5 relue avec `PreLeveledStart` (R-161 amendée en J5 :
    // `LevelingDelay` n'est plus jeté) ; affectation sur 4.
    // `delete 4` : TaskDeleted, LinkRemoved, AssignmentRemoved, puis
    // ExtrasDropped (5), puis IdsRenumbered — l'ordre de la fiche.
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1, 2, 3, 4, 5], 1);
    link(&mut p, 2, 4);
    link(&mut p, 4, 5);
    p.tasks.get_mut(uid(5)).unwrap().extras.0.push(ExtraEntry {
        origin: "mspdi".to_owned(),
        key: "PreLeveledStart".to_owned(),
        payload: "<PreLeveledStart>2026-11-02T08:00:00</PreLeveledStart>".to_owned(),
    });
    p.assignments
        .push(Assignment::new(AssignmentUid::new(1), uid(4), None))
        .unwrap();
    let mut s = session(p);
    let o = apply(&mut s, json!([{ "op": "task.delete", "uid": 4 }]));
    assert_eq!(
        o.side_effects,
        [
            SideEffect::TaskDeleted(uid(4)),
            SideEffect::LinkRemoved {
                pred: uid(2),
                succ: uid(4)
            },
            SideEffect::LinkRemoved {
                pred: uid(4),
                succ: uid(5)
            },
            SideEffect::AssignmentRemoved(AssignmentUid::new(1)),
            SideEffect::ExtrasDropped {
                subject: Subject::Task(uid(5)),
                keys: vec!["PreLeveledStart".to_owned()],
            },
            SideEffect::IdsRenumbered {
                from: TaskId::new(4),
                count: 1
            },
        ]
    );
}
