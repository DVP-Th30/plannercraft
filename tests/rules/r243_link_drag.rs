//! R-243 — Tracer un lien : de l'intérieur de la barre de A à la barre de
//! B ⇒ `link.add { pred: A, succ: B, kind: FS }`, sans décalage ; A = B,
//! ligne vide ou tâche exclue ⇒ aucun lot ; refus du registre (R-156 :
//! doublon, cycle ; R-059 : vers sa descendante) ⇒ rien ne change.

use plannercraft_automation::error::codes;
use plannercraft_automation::gesture::{self, GestureError, NotDraggable};
use plannercraft_automation::{ApplyMode, Session};
use plannercraft_core::{Link, LinkType, TaskUid};
use plannercraft_tests::resource_edit::with_tasks;
use plannercraft_tests::task_edit::{apply, get};
use serde_json::json;

fn uid(u: u32) -> TaskUid {
    TaskUid::new(u)
}

fn seven() -> Session {
    let tasks: Vec<(String, &str)> = (1..=7).map(|i| (format!("T{i}"), "1j")).collect();
    let refs: Vec<(&str, &str)> = tasks.iter().map(|(n, d)| (n.as_str(), *d)).collect();
    with_tasks(&refs)
}

#[test]
fn r243_link_fs_zero() {
    // A (ID 3) vers B (ID 7) ⇒ Prédécesseurs de 7 « 3 ».
    let mut s = seven();
    let b = gesture::link_drag(&s, uid(3), uid(7)).unwrap();
    s.apply(&b, ApplyMode::Commit).unwrap();
    let links = &get(&s, 7).predecessors;
    assert_eq!(links.as_slice(), &[Link::finish_to_start(uid(3))]);
    assert_eq!(links[0].kind, LinkType::FinishToStart);
    assert_eq!(
        gesture::link_drag(&s, uid(3), uid(3)).unwrap_err(),
        GestureError::SameTask
    );
}

#[test]
fn r243_duplicate_refused() {
    let mut s = seven();
    let b = gesture::link_drag(&s, uid(3), uid(7)).unwrap();
    s.apply(&b, ApplyMode::Commit).unwrap();
    let before = s.project().clone();
    let err = s.apply(&b, ApplyMode::Commit).unwrap_err();
    assert_eq!(err.code, codes::LINK_DUPLICATE);
    assert_eq!(s.project(), &before);
}

#[test]
fn r243_cycle_refused() {
    let mut s = seven();
    apply(
        &mut s,
        json!([{ "op": "link.add", "pred": 1, "succ": 2 }, { "op": "link.add", "pred": 2, "succ": 3 }]),
    );
    let before = s.project().clone();
    let b = gesture::link_drag(&s, uid(3), uid(1)).unwrap();
    let err = s.apply(&b, ApplyMode::Commit).unwrap_err();
    assert_eq!(err.code, codes::BATCH_WOULD_CREATE_ERRORS);
    assert_eq!(s.project(), &before);
}

#[test]
fn r243_summary_descendant_and_excluded() {
    let mut s = seven();
    apply(
        &mut s,
        json!([{ "op": "task.indent", "uid": 2 }, { "op": "task.update", "uid": 5, "active": false }]),
    );
    let b = gesture::link_drag(&s, uid(1), uid(2)).unwrap();
    assert_eq!(
        s.apply(&b, ApplyMode::Commit).unwrap_err().code,
        codes::LINK_SUMMARY_DESCENDANT
    );
    assert_eq!(
        gesture::link_drag(&s, uid(4), uid(5)).unwrap_err(),
        GestureError::NotDraggable(uid(5), NotDraggable::Excluded)
    );
    assert_eq!(
        gesture::link_drag(&s, uid(4), uid(99)).unwrap_err(),
        GestureError::UnknownTask(uid(99))
    );
}

#[test]
fn r243_already_linked_gesture() {
    // R-260 c (seconde passe UX) : glisser 3 → 7 alors que 3 → 7 existe, ou
    // 7 → 3 alors que 3 → 7 existe ⇒ `AlreadyLinked`, aucun lot ; le
    // registre garde `link.duplicate` (R-156) pour un lot écrit à la main.
    let mut s = seven();
    let b = gesture::link_drag(&s, uid(3), uid(7)).unwrap();
    s.apply(&b, ApplyMode::Commit).unwrap();
    let entries = s.history().entries.len();
    for (a, z) in [(3, 7), (7, 3)] {
        let err = gesture::link_drag(&s, uid(a), uid(z)).unwrap_err();
        assert_eq!(err, GestureError::AlreadyLinked);
        assert_eq!(err.code(), codes::LINK_DUPLICATE);
    }
    assert_eq!(s.history().entries.len(), entries);
    // Une autre paire reste permise.
    assert!(gesture::link_drag(&s, uid(3), uid(6)).is_ok());
}
