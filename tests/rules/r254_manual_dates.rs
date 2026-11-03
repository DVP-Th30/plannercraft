//! R-254 — Sur une feuille manuelle, `task.update { start | finish |
//! duration }` écrit les valeurs `Manual` sans poser de contrainte : début
//! saisi ⇒ fin = début + durée manuelle sur le calendrier de la tâche ; fin
//! saisie ⇒ durée recalculée (fin avant début :
//! `task.manual_finish_before_start`) ; durée saisie ⇒ fin recalculée.
//! Repère : projet « P », 6 « Plâtrerie » manuelle 4 j, lun 09/11 08:00 –
//! jeu 12/11 17:00. Source : S73 (les dates ne bougent pas seules) ; calcul
//! de la fin : hypothèse.

use plannercraft_automation::Session;
use plannercraft_automation::error::codes;
use plannercraft_core::{Constraint, Moment, SchedulingMode};
use plannercraft_tests::daily::{dates, plan_p};
use plannercraft_tests::moment;
use plannercraft_tests::task_edit::{apply, get, try_apply};
use serde_json::json;

fn manual_6() -> Session {
    let mut s = plan_p();
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 6, "mode": "manual" }]),
    );
    s
}

/// Valeurs manuelles de 6 : début, fin, durée en minutes.
fn manual(s: &Session) -> (Option<Moment>, Option<Moment>, Option<i64>) {
    match get(s, 6).mode {
        SchedulingMode::Manual {
            start,
            finish,
            duration,
        } => (start, finish, duration.map(|d| d.minutes().get())),
        SchedulingMode::Auto => panic!("6 devrait être manuelle"),
    }
}

#[test]
fn r254_start_moves_finish() {
    let mut s = manual_6();
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 6, "start": "2026-11-16T08:00" }]),
    );
    assert_eq!(
        manual(&s),
        (
            Some(moment("2026-11-16T08:00")),
            Some(moment("2026-11-19T17:00")),
            Some(4 * 480)
        )
    );
    assert_eq!(get(&s, 6).constraint, Constraint::AsSoonAsPossible);
    assert_eq!(
        dates(&s, 6),
        (moment("2026-11-16T08:00"), moment("2026-11-19T17:00"))
    );
    // Passage en manuel et date saisie dans la même opération.
    let mut s = plan_p();
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 6, "mode": "manual", "start": "2026-11-16T08:00" }]),
    );
    assert_eq!(manual(&s).1, Some(moment("2026-11-19T17:00")));
    assert_eq!(get(&s, 6).constraint, Constraint::AsSoonAsPossible);
}

#[test]
fn r254_finish_before_start_refused() {
    let mut s = manual_6();
    let before = s.project().clone();
    let err = try_apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 6, "finish": "2026-11-05T17:00" }]),
    )
    .unwrap_err();
    assert_eq!(err.code, codes::TASK_MANUAL_FINISH_BEFORE_START);
    assert_eq!(err.field.as_deref(), Some("finish"));
    assert_eq!(s.project(), &before);
    // Une date ne s'efface pas sur une tâche manuelle.
    let err = try_apply(&mut s, json!([{ "op": "task.update", "uid": 6, "start": null }])).unwrap_err();
    assert_eq!(err.code, codes::PARAM_INVALID);
    // Fin saisie après le début : durée recalculée (ven 13/11 17:00 ⇒ 5 j).
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 6, "finish": "2026-11-13T17:00" }]),
    );
    assert_eq!(
        manual(&s),
        (
            Some(moment("2026-11-09T08:00")),
            Some(moment("2026-11-13T17:00")),
            Some(5 * 480)
        )
    );
    assert_eq!(get(&s, 6).duration.minutes().get(), 5 * 480);
    assert_eq!(get(&s, 6).constraint, Constraint::AsSoonAsPossible);
}

#[test]
fn r254_duration_moves_finish() {
    let mut s = manual_6();
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 6, "duration": { "text": "2j" } }]),
    );
    assert_eq!(
        manual(&s),
        (
            Some(moment("2026-11-09T08:00")),
            Some(moment("2026-11-10T17:00")),
            Some(2 * 480)
        )
    );
    assert_eq!(get(&s, 6).duration.minutes().get(), 2 * 480);
    assert_eq!(
        dates(&s, 6),
        (moment("2026-11-09T08:00"), moment("2026-11-10T17:00"))
    );
    // Durée et fin ensemble : contradictoires.
    let err = try_apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 6, "duration": { "text": "3j" }, "finish": "2026-11-20T17:00" }]),
    )
    .unwrap_err();
    assert_eq!(err.code, codes::PARAM_CONFLICT);
    // Une récapitulative reste calculée (R-157).
    let err = try_apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 5, "duration": { "text": "3j" } }]),
    )
    .unwrap_err();
    assert_eq!(err.code, codes::TASK_COMPUTED_FIELD);
}
