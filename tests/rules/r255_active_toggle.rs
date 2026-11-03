//! R-255 — Basculer actif / inactif une sélection : si au moins une tâche
//! sélectionnée est active, toutes deviennent inactives, sinon toutes
//! actives ; une récapitulative entraîne ses descendants ; un seul lot.
//! Repère : projet « P ». Source : S74 (« Inactivate a task » : la même
//! commande réactive) ; sélection mêlée et descendants : hypothèse.

use plannercraft_automation::helpers;
use plannercraft_automation::{ApplyMode, Batch, Session};
use plannercraft_tests::daily::{plan_p, uids};
use plannercraft_tests::task_edit::{apply, get};
use serde_json::json;

fn commit(s: &mut Session, b: &Batch) {
    s.apply(b, ApplyMode::Commit).unwrap();
}

/// (UID, actif) des opérations du lot.
fn targets(b: &Batch) -> Vec<(u32, bool)> {
    b.ops
        .iter()
        .map(|op| {
            let v = serde_json::to_value(op).unwrap();
            assert_eq!(v["op"], json!("task.update"));
            (
                u32::try_from(v["uid"].as_u64().unwrap()).unwrap(),
                v["active"].as_bool().unwrap(),
            )
        })
        .collect()
}

fn active(s: &Session, ns: &[u32]) -> Vec<bool> {
    ns.iter().map(|&n| get(s, n).active).collect()
}

#[test]
fn r255_any_active_makes_all_inactive() {
    // Exemple : {2 actif, 3 inactif} ⇒ 2 et 3 inactifs ; refaire ⇒ actifs.
    let mut s = plan_p();
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 3, "active": false }]),
    );
    let b = helpers::toggle_active(&s, &uids(&[2, 3])).unwrap().unwrap();
    assert_eq!(targets(&b), [(2, false)]);
    commit(&mut s, &b);
    assert_eq!(active(&s, &[2, 3]), [false, false]);
    let b = helpers::toggle_active(&s, &uids(&[3, 2, 3])).unwrap().unwrap();
    assert_eq!(targets(&b), [(2, true), (3, true)]);
    commit(&mut s, &b);
    assert_eq!(active(&s, &[2, 3]), [true, true]);
    assert_eq!(helpers::toggle_active(&s, &[]), Ok(None));
}

#[test]
fn r255_summary_cascades() {
    // Exemple : {1} ⇒ 1, 2, 3, 4 inactifs (un lot, une annulation) ;
    // refaire ⇒ tous actifs.
    let mut s = plan_p();
    let before = s.project().clone();
    let len = s.history().entries.len();
    let b = helpers::toggle_active(&s, &uids(&[1])).unwrap().unwrap();
    assert_eq!(targets(&b), [(1, false), (2, false), (3, false), (4, false)]);
    commit(&mut s, &b);
    assert_eq!(s.history().entries.len(), len + 1);
    assert_eq!(active(&s, &[1, 2, 3, 4, 5]), [false, false, false, false, true]);
    let b = helpers::toggle_active(&s, &uids(&[1])).unwrap().unwrap();
    commit(&mut s, &b);
    assert_eq!(s.project().tasks, before.tasks);
    // Descendant déjà inactif : pas d'opération pour lui.
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 7, "active": false }]),
    );
    let b = helpers::toggle_active(&s, &uids(&[5])).unwrap().unwrap();
    assert_eq!(targets(&b), [(5, false), (6, false)]);
    s.apply(&b, ApplyMode::Commit).unwrap();
    s.undo().unwrap();
    assert_eq!(active(&s, &[5, 6, 7]), [true, true, false]);
}
