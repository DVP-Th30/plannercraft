//! R-157 — Champs calculés d'une récapitulative ; durée nulle ⇒ jalon ;
//! dates d'une tâche manuelle refusées.

use plannercraft_automation::CommandErrorKind;
use plannercraft_automation::error::codes;
use plannercraft_core::{SchedulingMode, Subject, TaskUid};
use plannercraft_tests::mspdi::new_project;
use plannercraft_tests::session::{push_tasks, session};
use plannercraft_tests::task_edit::{apply, get, push_outline, try_apply};
use serde_json::json;

#[test]
fn r157_summary_duration_refused() {
    // [1 P(1), 2 a(2)] : `update 1 duration 3j` → task.computed_field.
    let mut p = new_project("2026-11-02T08:00");
    push_outline(&mut p, &[(1, 1), (2, 2)]);
    let mut s = session(p);
    let before = s.project().clone();
    for (field, value) in [
        ("duration", json!({ "text": "3j" })),
        ("start", json!("2026-11-04T08:00")),
        ("finish", json!("2026-11-06T17:00")),
    ] {
        let mut op = json!({ "op": "task.update", "uid": 1 });
        op[field] = value;
        let err = try_apply(&mut s, json!([op])).unwrap_err();
        assert_eq!(err.code, codes::TASK_COMPUTED_FIELD, "{field}");
        assert_eq!(err.kind, CommandErrorKind::ComputedField);
        assert_eq!(err.field.as_deref(), Some(field));
        assert_eq!(err.subject, Some(Subject::Task(TaskUid::new(1))));
    }
    assert_eq!(s.project(), &before);
    // Nom et notes d'une récapitulative : modifiables.
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 1, "name": "Lot 1", "notes": "n" }]),
    );
    assert_eq!(get(&s, 1).name, "Lot 1");
}

#[test]
fn r157_zero_duration_milestone() {
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1], 3);
    let mut s = session(p);
    // Ajout de durée nulle : jalon.
    apply(
        &mut s,
        json!([{ "op": "task.add", "name": "J", "duration": { "minutes": 0 } }]),
    );
    assert!(get(&s, 2).milestone);
    // Durée passée à 0 : jalon.
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 1, "duration": { "text": "0j" } }]),
    );
    assert!(get(&s, 1).milestone);
    // `milestone:true` sur une durée > 0 : drapeau posé, durée gardée.
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 1, "duration": { "text": "2j" }, "milestone": false }]),
    );
    assert!(!get(&s, 1).milestone);
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 1, "milestone": true }]),
    );
    assert!(get(&s, 1).milestone);
    assert_eq!(get(&s, 1).duration.minutes().get(), 960);
    // `milestone:false` explicite l'emporte sur la durée nulle.
    apply(
        &mut s,
        json!([{ "op": "task.add", "name": "K", "duration": { "minutes": 0 }, "milestone": false }]),
    );
    assert!(!get(&s, 3).milestone);
}

#[test]
fn r157_manual_dates_entered_by_r254() {
    // Amendée par R-254 (J8, lot L2) : les dates d'une feuille manuelle se
    // saisissent (valeurs manuelles, sans contrainte) ; une date ne
    // s'efface pas.
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1], 1);
    p.tasks.get_mut(TaskUid::new(1)).unwrap().mode = SchedulingMode::Manual {
        start: None,
        finish: None,
        duration: None,
    };
    let mut s = session(p);
    let err = try_apply(&mut s, json!([{ "op": "task.update", "uid": 1, "finish": null }])).unwrap_err();
    assert_eq!(err.code, codes::PARAM_INVALID);
    assert_eq!(err.field.as_deref(), Some("finish"));
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 1, "start": "2026-11-04T08:00" }]),
    );
    let SchedulingMode::Manual { start, finish, .. } = get(&s, 1).mode else {
        panic!("1 devrait rester manuelle");
    };
    assert_eq!(start, Some(plannercraft_tests::moment("2026-11-04T08:00")));
    assert_eq!(finish, Some(plannercraft_tests::moment("2026-11-04T17:00")));
    assert_eq!(
        get(&s, 1).constraint,
        plannercraft_core::Constraint::AsSoonAsPossible
    );
    // Le nom reste modifiable.
    apply(&mut s, json!([{ "op": "task.update", "uid": 1, "name": "M" }]));
    assert_eq!(get(&s, 1).name, "M");
}
