//! R-253 — Bascule auto ↔ manuel : `task.update { mode: "manual" }` écrit
//! `Manual { start, finish, duration }` égaux aux dates planifiées courantes
//! (O49) ; la tâche ne bouge plus quand ses prédécesseurs changent (S73) ;
//! `mode: "auto"` retire les valeurs manuelles, la tâche est recalculée,
//! contrainte et échéance gardées. Repère : projet « P » de
//! `docs/plans/j8.md` § 2 ; 6 « Plâtrerie » 4 j, lun 09/11 08:00 – jeu 12/11
//! 17:00. Source : S73.

use plannercraft_automation::error::codes;
use plannercraft_core::{Constraint, SchedulingMode};
use plannercraft_tests::daily::{dates, plan_p};
use plannercraft_tests::moment;
use plannercraft_tests::task_edit::{apply, get, try_apply};
use serde_json::json;

#[test]
fn r253_auto_to_manual_freezes_dates() {
    let mut s = plan_p();
    assert_eq!(
        dates(&s, 6),
        (moment("2026-11-09T08:00"), moment("2026-11-12T17:00"))
    );
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 6, "mode": "manual" }]),
    );
    let SchedulingMode::Manual {
        start,
        finish,
        duration,
    } = get(&s, 6).mode
    else {
        panic!("6 devrait être manuelle");
    };
    assert_eq!(start, Some(moment("2026-11-09T08:00")));
    assert_eq!(finish, Some(moment("2026-11-12T17:00")));
    assert_eq!(duration.map(|d| d.minutes().get()), Some(4 * 480));
    // Mêmes dates après le recalcul ; aucune contrainte posée.
    assert_eq!(
        dates(&s, 6),
        (moment("2026-11-09T08:00"), moment("2026-11-12T17:00"))
    );
    assert_eq!(get(&s, 6).constraint, Constraint::AsSoonAsPossible);
    assert!(
        s.schedule()
            .scheduled(plannercraft_core::TaskUid::new(6))
            .unwrap()
            .manual
    );
    // Déjà manuelle : rien ne change ; `mode: null` refusé.
    let before = s.project().clone();
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 6, "mode": "manual" }]),
    );
    assert_eq!(s.project(), &before);
    let err = try_apply(&mut s, json!([{ "op": "task.update", "uid": 6, "mode": null }])).unwrap_err();
    assert_eq!(err.code, codes::PARAM_INVALID);
    assert_eq!(err.field.as_deref(), Some("mode"));
}

#[test]
fn r253_manual_ignores_predecessor() {
    let mut s = plan_p();
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 6, "mode": "manual" }]),
    );
    // 3 allongée de 2 j (10 j → 12 j) : 6 ne bouge pas (S73).
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 3, "duration": { "text": "12j" } }]),
    );
    assert_eq!(dates(&s, 3).1, moment("2026-11-10T17:00"));
    assert_eq!(
        dates(&s, 6),
        (moment("2026-11-09T08:00"), moment("2026-11-12T17:00"))
    );
    // Retour en automatique : recalculée derrière 3.
    apply(&mut s, json!([{ "op": "task.update", "uid": 6, "mode": "auto" }]));
    assert_eq!(get(&s, 6).mode, SchedulingMode::Auto);
    assert_eq!(
        dates(&s, 6),
        (moment("2026-11-11T08:00"), moment("2026-11-16T17:00"))
    );
}

#[test]
fn r253_manual_to_auto_recalc_keeps_constraint() {
    let mut s = plan_p();
    apply(
        &mut s,
        json!([
            { "op": "task.set_constraint", "uid": 6, "kind": "snet", "date": "2026-11-10T08:00" },
            { "op": "task.set_deadline", "uid": 6, "deadline": "2026-11-20T17:00" }
        ]),
    );
    assert_eq!(
        dates(&s, 6),
        (moment("2026-11-10T08:00"), moment("2026-11-13T17:00"))
    );
    // Bascule et retour dans le même lot : figée sur les dates du clone,
    // puis libérée ; contrainte et échéance intactes.
    apply(
        &mut s,
        json!([
            { "op": "task.update", "uid": 6, "mode": "manual" },
            { "op": "task.update", "uid": 2, "duration": { "text": "1j" } }
        ]),
    );
    assert_eq!(
        dates(&s, 6),
        (moment("2026-11-10T08:00"), moment("2026-11-13T17:00"))
    );
    apply(&mut s, json!([{ "op": "task.update", "uid": 6, "mode": "auto" }]));
    let t = get(&s, 6);
    assert_eq!(t.mode, SchedulingMode::Auto);
    assert_eq!(
        t.constraint,
        Constraint::StartNoEarlierThan {
            date: moment("2026-11-10T08:00")
        }
    );
    assert_eq!(t.deadline, Some(moment("2026-11-20T17:00")));
    // 3 finit plus tôt (2 raccourcie) ; la SNET retient 6 au 10/11.
    assert_eq!(
        dates(&s, 6),
        (moment("2026-11-10T08:00"), moment("2026-11-13T17:00"))
    );
    // Une récapitulative passe aussi en manuel, à ses dates calculées.
    let (start, finish) = dates(&s, 5);
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 5, "mode": "manual" }]),
    );
    assert!(matches!(
        get(&s, 5).mode,
        SchedulingMode::Manual { start: Some(a), finish: Some(b), .. } if a == start && b == finish
    ));
}

#[test]
fn r253_toggle_mode_helper() {
    // Raccourci de l'interface (H-UI-36) : une tâche automatique dans la
    // sélection ⇒ toutes en manuel ; sinon toutes en automatique ; sans
    // cascade aux descendants ; un lot.
    use plannercraft_automation::helpers;
    use plannercraft_tests::daily::uids;
    let mut s = plan_p();
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 6, "mode": "manual" }]),
    );
    let b = helpers::toggle_mode(&s, &uids(&[6, 2, 6])).unwrap().unwrap();
    assert_eq!(b.ops.len(), 1);
    s.apply(&b, plannercraft_automation::ApplyMode::Commit).unwrap();
    assert!(matches!(get(&s, 2).mode, SchedulingMode::Manual { .. }));
    assert_eq!(
        dates(&s, 2),
        (moment("2026-10-19T08:00"), moment("2026-10-23T17:00"))
    );
    let b = helpers::toggle_mode(&s, &uids(&[2, 6])).unwrap().unwrap();
    assert_eq!(b.ops.len(), 2);
    s.apply(&b, plannercraft_automation::ApplyMode::Commit).unwrap();
    assert_eq!(get(&s, 2).mode, SchedulingMode::Auto);
    assert_eq!(get(&s, 6).mode, SchedulingMode::Auto);
    // Récapitulative seule : elle bascule, pas ses enfants.
    let b = helpers::toggle_mode(&s, &uids(&[5])).unwrap().unwrap();
    assert_eq!(b.ops.len(), 1);
    assert_eq!(helpers::toggle_mode(&s, &[]), Ok(None));
}

/// Relecture finale de J8 : une suite de passages en manuel purs dans un
/// lot lit un seul planning de référence (figer une tâche à ses dates ne
/// déplace aucune autre tâche). Le résultat est celui de passages faits
/// un par un, chacun dans son lot (planning recalculé à chaque fois) :
/// récapitulatives, feuilles liées, contrainte, avancement et tâche
/// inactive compris.
#[test]
fn r253_freeze_batch_matches_one_by_one() {
    use plannercraft_automation::{ApplyMode, Batch, helpers};
    use plannercraft_tests::generate::ui_network;
    use plannercraft_tests::session::session;
    let mut p = ui_network(6, 7, 60);
    // Quelques cas de plus : avancement, inactive, contrainte.
    let mut base = session(p.clone());
    apply(
        &mut base,
        json!([
            { "op": "progress.update", "task": 3, "percent_complete": { "text": "40%" } },
            { "op": "task.update", "uid": 12, "active": false },
            { "op": "task.set_constraint", "uid": 20, "kind": "snet", "date": "2026-02-02T08:00" }
        ]),
    );
    p = base.project().clone();
    let all: Vec<_> = p
        .tasks
        .iter()
        .filter(|t| !t.is_null)
        .map(plannercraft_core::HasUid::uid)
        .collect();
    let batch = helpers::toggle_mode(&base, &all).unwrap().unwrap();
    assert!(batch.ops.len() > 40);
    let mut at_once = session(p.clone());
    at_once.apply(&batch, ApplyMode::Commit).unwrap();
    let mut one_by_one = session(p);
    for op in &batch.ops {
        let single = Batch {
            ops: vec![op.clone()],
            ..batch.clone()
        };
        one_by_one.apply(&single, ApplyMode::Commit).unwrap();
    }
    for (a, b) in at_once
        .project()
        .tasks
        .iter()
        .zip(one_by_one.project().tasks.iter())
    {
        assert_eq!(a.mode, b.mode, "tâche {} « {} »", a.uid().get(), a.name);
        assert!(matches!(a.mode, SchedulingMode::Manual { .. }) || a.is_null);
    }
    assert_eq!(at_once.schedule(), one_by_one.schedule());
}

/// Relecture finale de J8 : « Mode » sur toute la sélection d'un projet de
/// 5 000 tâches passe en un lot (un seul recalcul au lieu d'un par tâche :
/// le budget `schedule_recomputes` refusait le lot au-delà d'environ 200
/// tâches).
#[test]
fn r253_toggle_mode_5000_within_budget() {
    use plannercraft_automation::{ApplyMode, helpers};
    use plannercraft_tests::generate::large_network;
    use plannercraft_tests::session::session;
    let p = large_network(7);
    let all: Vec<_> = p
        .tasks
        .iter()
        .filter(|t| !t.is_null)
        .map(plannercraft_core::HasUid::uid)
        .collect();
    let mut s = session(p);
    let batch = helpers::toggle_mode(&s, &all).unwrap().unwrap();
    assert!(batch.ops.len() > 4_000);
    s.apply(&batch, ApplyMode::Commit).unwrap();
    assert!(
        s.project()
            .tasks
            .iter()
            .all(|t| t.is_null || matches!(t.mode, SchedulingMode::Manual { .. }))
    );
}
