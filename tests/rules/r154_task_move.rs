//! R-154 — `task.move` : déplacement d'un sous-arbre entier, niveaux
//! décalés, cible dans le bloc refusée.

use plannercraft_automation::SideEffect;
use plannercraft_automation::error::codes;
use plannercraft_core::TaskId;
use plannercraft_tests::mspdi::new_project;
use plannercraft_tests::session::session;
use plannercraft_tests::task_edit::{apply, push_outline, rows, try_apply};
use serde_json::json;

fn outline(spec: &[(u32, u16)]) -> plannercraft_automation::Session {
    let mut p = new_project("2026-11-02T08:00");
    push_outline(&mut p, spec);
    session(p)
}

#[test]
fn r154_move_subtree_with_levels() {
    // [1 A(1), 2 A1(2), 3 B(1), 4 B1(2)], move 3 to FirstChildOf(1) →
    // [A(1), B(2), B1(3), A1(2)].
    let mut s = outline(&[(1, 1), (2, 2), (3, 1), (4, 2)]);
    let o = apply(
        &mut s,
        json!([{ "op": "task.move", "uid": 3, "to": { "first_child_of": 1 } }]),
    );
    assert_eq!(rows(&s), [(1, 1), (3, 2), (4, 3), (2, 2)]);
    assert!(o.side_effects.contains(&SideEffect::IdsRenumbered {
        from: TaskId::new(2),
        count: 3
    }));
    // Retour en fin : niveau 1, bloc entier.
    apply(&mut s, json!([{ "op": "task.move", "uid": 3, "to": "end" }]));
    assert_eq!(rows(&s), [(1, 1), (2, 2), (3, 1), (4, 2)]);
    // Avant une tâche plus profonde : le bloc prend son niveau.
    apply(
        &mut s,
        json!([{ "op": "task.move", "uid": 3, "to": { "before": 2 } }]),
    );
    assert_eq!(rows(&s), [(1, 1), (3, 2), (4, 3), (2, 2)]);
    // Après une tâche placée plus loin : cible résolue avant le retrait.
    let mut s = outline(&[(1, 1), (2, 1), (3, 1)]);
    apply(
        &mut s,
        json!([{ "op": "task.move", "uid": 1, "to": { "after": 3 } }]),
    );
    assert_eq!(rows(&s), [(2, 1), (3, 1), (1, 1)]);
    apply(
        &mut s,
        json!([{ "op": "task.move", "uid": 1, "to": { "last_child_of": 2 } }]),
    );
    assert_eq!(rows(&s), [(2, 1), (1, 2), (3, 1)]);
}

#[test]
fn r154_move_into_own_subtree_refused() {
    let mut s = outline(&[(1, 1), (2, 2), (3, 3), (4, 1)]);
    let before = s.project().clone();
    for to in [
        json!({ "after": 2 }),
        json!({ "first_child_of": 3 }),
        json!({ "last_child_of": 1 }),
        json!({ "before": 1 }),
    ] {
        let err = try_apply(&mut s, json!([{ "op": "task.move", "uid": 1, "to": to }])).unwrap_err();
        assert_eq!(err.code, codes::TASK_MOVE_INTO_SUBTREE, "{to}");
        assert_eq!(err.field.as_deref(), Some("to"));
    }
    assert_eq!(s.project(), &before);
    // Hors du bloc : accepté.
    apply(
        &mut s,
        json!([{ "op": "task.move", "uid": 2, "to": { "after": 4 } }]),
    );
    assert_eq!(rows(&s), [(1, 1), (4, 1), (2, 1), (3, 2)]);
}
