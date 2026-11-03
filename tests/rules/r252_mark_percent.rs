//! R-252 — Marquer n % une sélection (n ∈ {0, 25, 50, 75, 100}) : chaque
//! feuille sélectionnée reçoit `progress.update { percent_complete: n × 100 }`
//! ; une récapitulative sélectionnée vaut ses feuilles descendantes ; lignes
//! vides, tâches inactives ignorées (jalons de durée nulle aussi pour 25,
//! 50 et 75 %) ; doublons fusionnés ; un seul lot (une entrée
//! d'annulation) ; aucune écriture pour une feuille déjà à n %. Repère :
//! projet « P ». Source : S76 (« Update work on a project » : boutons 0 à
//! 100 % sur une sélection) ; récapitulatives, inactives : hypothèse.

use plannercraft_automation::helpers::{self, HelperError, PercentStep};
use plannercraft_automation::{ApplyMode, Batch, Session};
use plannercraft_core::{Percent, TaskUid};
use plannercraft_tests::daily::{plan_p, uids};
use plannercraft_tests::task_edit::{apply, get};
use serde_json::json;

fn commit(s: &mut Session, b: &Batch) {
    s.apply(b, ApplyMode::Commit).unwrap();
}

/// (UID, % en 1/10 000) des opérations du lot.
fn targets(b: &Batch) -> Vec<(u32, i64)> {
    b.ops
        .iter()
        .map(|op| {
            let v = serde_json::to_value(op).unwrap();
            assert_eq!(v["op"], json!("progress.update"));
            (
                u32::try_from(v["task"].as_u64().unwrap()).unwrap(),
                v["percent_complete"]["value"].as_i64().unwrap(),
            )
        })
        .collect()
}

fn percent(s: &Session, n: u32) -> i64 {
    get(s, n).progress.percent_complete.get()
}

#[test]
fn r252_summary_means_leaves() {
    // Exemple de R-252 : {1} avec 2 à 0 % et 3 à 100 %, marquer 50 % ⇒
    // deux `progress.update` (2 et 3 à 5 000) ; le jalon 4 est ignoré.
    let mut s = plan_p();
    apply(
        &mut s,
        json!([{ "op": "progress.update", "task": 3, "percent_complete": { "text": "100%" } }]),
    );
    let b = helpers::mark_percent(&s, &uids(&[1]), PercentStep::P50)
        .unwrap()
        .unwrap();
    assert_eq!(targets(&b), [(2, 5_000), (3, 5_000)]);
    commit(&mut s, &b);
    assert_eq!((percent(&s, 2), percent(&s, 3)), (5_000, 5_000));
    // Le % de la récapitulative vient du moteur.
    let p1 = s.schedule().scheduled(TaskUid::new(1)).unwrap().percent_complete;
    assert!(p1 > Percent::ZERO && p1 < Percent::FULL, "{p1:?}");
    // 100 % sur {1} : le jalon est compris.
    let b = helpers::mark_percent(&s, &uids(&[1]), PercentStep::P100)
        .unwrap()
        .unwrap();
    assert_eq!(targets(&b), [(2, 10_000), (3, 10_000), (4, 10_000)]);
    // UID inconnu : refus, rien produit.
    assert_eq!(
        helpers::mark_percent(&s, &uids(&[99]), PercentStep::P25),
        Err(HelperError::UnknownTask(TaskUid::new(99)))
    );
}

#[test]
fn r252_inactive_skipped() {
    let mut s = plan_p();
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 7, "active": false }]),
    );
    let b = helpers::mark_percent(&s, &uids(&[5]), PercentStep::P100)
        .unwrap()
        .unwrap();
    assert_eq!(targets(&b), [(6, 10_000)]);
    assert_eq!(helpers::mark_percent(&s, &uids(&[7]), PercentStep::P50), Ok(None));
    // Sous une récapitulative inactive : ignorées aussi.
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 1, "active": false }]),
    );
    assert_eq!(
        helpers::mark_percent(&s, &uids(&[1, 2, 3]), PercentStep::P25),
        Ok(None)
    );
}

#[test]
fn r252_noop_returns_none() {
    let mut s = plan_p();
    assert_eq!(helpers::mark_percent(&s, &uids(&[2]), PercentStep::P0), Ok(None));
    assert_eq!(helpers::mark_percent(&s, &[], PercentStep::P50), Ok(None));
    let b = helpers::mark_percent(&s, &uids(&[2]), PercentStep::P50)
        .unwrap()
        .unwrap();
    commit(&mut s, &b);
    assert_eq!(helpers::mark_percent(&s, &uids(&[2]), PercentStep::P50), Ok(None));
    // Doublons et récapitulative recouvrante fusionnés : 3 seule.
    let b = helpers::mark_percent(&s, &uids(&[1, 2, 2, 3]), PercentStep::P50)
        .unwrap()
        .unwrap();
    assert_eq!(targets(&b), [(3, 5_000)]);
}

#[test]
fn r252_single_undo_entry() {
    let mut s = plan_p();
    let before = s.project().clone();
    let len = s.history().entries.len();
    let b = helpers::mark_percent(&s, &uids(&[1, 5]), PercentStep::P75)
        .unwrap()
        .unwrap();
    assert_eq!(targets(&b).len(), 4);
    commit(&mut s, &b);
    assert_eq!(s.history().entries.len(), len + 1);
    assert_eq!(percent(&s, 7), 7_500);
    s.undo().unwrap();
    assert_eq!(s.project(), &before);
}
