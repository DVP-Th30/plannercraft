//! R-153 — Indentation et désindentation d'un bloc (tâche + descendants).

use plannercraft_automation::CommandErrorKind;
use plannercraft_automation::error::codes;
use plannercraft_core::{Diagnostic, OutlineLevel, TaskUid};
use plannercraft_tests::mspdi::new_project;
use plannercraft_tests::session::{link, session};
use plannercraft_tests::task_edit::{apply, push_outline, rows, try_apply};
use serde_json::json;

fn outline(spec: &[(u32, u16)]) -> plannercraft_automation::Session {
    let mut p = new_project("2026-11-02T08:00");
    push_outline(&mut p, spec);
    session(p)
}

#[test]
fn r153_indent_first_refused() {
    let mut s = outline(&[(1, 1), (2, 1)]);
    let err = try_apply(&mut s, json!([{ "op": "task.indent", "uid": 1 }])).unwrap_err();
    assert_eq!(err.code, codes::TASK_OUTLINE_REFUSED);
    assert_eq!(err.kind, CommandErrorKind::OutlineRefused);
    assert_eq!(rows(&s), [(1, 1), (2, 1)]);
}

#[test]
fn r153_indent_jump_refused() {
    // [A(1), B(2)] : B au-delà de A + 1 → refus.
    let mut s = outline(&[(1, 1), (2, 2)]);
    let err = try_apply(&mut s, json!([{ "op": "task.indent", "uid": 2 }])).unwrap_err();
    assert_eq!(err.code, codes::TASK_OUTLINE_REFUSED);
    assert_eq!(rows(&s), [(1, 1), (2, 2)]);
}

#[test]
fn r153_indent_block() {
    // [A(1), B(1), B1(2), C(1)], indent B → B et B1 descendent d'un niveau.
    let mut s = outline(&[(1, 1), (2, 1), (3, 2), (4, 1)]);
    apply(&mut s, json!([{ "op": "task.indent", "uid": 2 }]));
    assert_eq!(rows(&s), [(1, 1), (2, 2), (3, 3), (4, 1)]);
    // Niveau maximal : refus `limit.exceeded`.
    let mut p = new_project("2026-11-02T08:00");
    push_outline(&mut p, &[(1, 1), (2, 1)]);
    for uid in [1, 2] {
        p.tasks.get_mut(TaskUid::new(uid)).unwrap().outline_level = OutlineLevel::new(u16::MAX).unwrap();
    }
    let mut s = session(p);
    let err = try_apply(&mut s, json!([{ "op": "task.indent", "uid": 2 }])).unwrap_err();
    assert_eq!(err.code, codes::LIMIT_EXCEEDED);
}

#[test]
fn r153_outdent_siblings_become_children() {
    // [1 P(1), 2 a(2), 3 b(2), 4 c(2)], outdent 3 → [P(1), a(2), b(1), c(2)] :
    // c devient enfant de b.
    let mut s = outline(&[(1, 1), (2, 2), (3, 2), (4, 2)]);
    apply(&mut s, json!([{ "op": "task.outdent", "uid": 3 }]));
    assert_eq!(rows(&s), [(1, 1), (2, 2), (3, 1), (4, 2)]);
    let b = s.schedule().scheduled(TaskUid::new(3)).unwrap();
    assert_eq!(b.role, plannercraft_core::TaskRole::Summary);
    // Le bloc entier remonte : [P(1), a(2), a1(3)], outdent 2 → a(1), a1(2).
    let mut s = outline(&[(1, 1), (2, 2), (3, 3)]);
    apply(&mut s, json!([{ "op": "task.outdent", "uid": 2 }]));
    assert_eq!(rows(&s), [(1, 1), (2, 1), (3, 2)]);
}

#[test]
fn r153_outdent_level1_refused() {
    let mut s = outline(&[(1, 1), (2, 2)]);
    let err = try_apply(&mut s, json!([{ "op": "task.outdent", "uid": 1 }])).unwrap_err();
    assert_eq!(err.code, codes::TASK_OUTLINE_REFUSED);
    assert_eq!(err.field.as_deref(), Some("uid"));
}

#[test]
fn r153_indent_creates_summary_descendant_link_refused() {
    // [A(1), B(1)] et A → B ; indenter B en ferait un lien récapitulative →
    // descendante (R-059) : refus par R-160, état intact.
    let mut p = new_project("2026-11-02T08:00");
    push_outline(&mut p, &[(1, 1), (2, 1)]);
    link(&mut p, 1, 2);
    let mut s = session(p);
    let before = s.project().clone();
    let err = try_apply(&mut s, json!([{ "op": "task.indent", "uid": 2 }])).unwrap_err();
    assert_eq!(err.code, codes::BATCH_WOULD_CREATE_ERRORS);
    assert_eq!(err.op_index, Some(0));
    let CommandErrorKind::WouldCreateErrors(errors) = &err.kind else {
        panic!("{err:?}");
    };
    assert!(
        errors
            .iter()
            .any(|d| matches!(d, Diagnostic::LinkSummaryDescendant { .. })),
        "{errors:?}"
    );
    assert_eq!(s.project(), &before);
}
