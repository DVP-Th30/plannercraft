//! R-152 — `task.add` : valeurs par défaut (« 1 jour? » estimé, ALAP à
//! rebours) et les cinq placements.

use plannercraft_automation::SideEffect;
use plannercraft_automation::error::codes;
use plannercraft_core::{
    BuiltinCalendarNames, Constraint, DurationUnit, Project, ProjectInit, ScheduleAnchor, TaskId, TaskUid,
};
use plannercraft_tests::moment;
use plannercraft_tests::mspdi::new_project;
use plannercraft_tests::session::session;
use plannercraft_tests::task_edit::{apply, get, names, push_outline, try_apply};
use serde_json::json;

/// [1 A (n1), 2 A1 (n2), 3 B (n1)].
fn abc() -> plannercraft_automation::Session {
    let mut p = new_project("2026-11-02T08:00");
    push_outline(&mut p, &[(1, 1), (2, 2), (3, 1)]);
    for (uid, name) in [(1, "A"), (2, "A1"), (3, "B")] {
        name.clone_into(&mut p.tasks.get_mut(TaskUid::new(uid)).unwrap().name);
    }
    session(p)
}

fn n(rows: &[(&str, u16)]) -> Vec<(String, u16)> {
    rows.iter().map(|&(s, l)| (s.to_owned(), l)).collect()
}

#[test]
fn r152_add_defaults_estimated_1d() {
    let mut s = session(new_project("2026-11-02T08:00"));
    let o = apply(&mut s, json!([{ "op": "task.add", "name": "A", "as": "$a" }]));
    assert_eq!(o.created, [TaskUid::new(1)]);
    assert_eq!(o.labels.get("$a"), Some(&TaskUid::new(1)));
    let t = get(&s, 1);
    assert_eq!(t.duration.minutes().get(), 480);
    assert_eq!(t.duration.unit(), DurationUnit::Day);
    assert!(t.duration.is_estimated());
    assert!(!t.milestone);
    assert_eq!(t.constraint, Constraint::AsSoonAsPossible);
    assert_eq!(t.outline_level.get(), 1);
    // Durée fournie : non estimée, sauf `estimated:true` ou « ? ».
    apply(
        &mut s,
        json!([
            { "op": "task.add", "name": "B", "duration": { "text": "5j" } },
            { "op": "task.add", "name": "C", "duration": { "minutes": 960, "estimated": true } },
        ]),
    );
    assert!(!get(&s, 2).duration.is_estimated());
    assert_eq!(get(&s, 2).duration.minutes().get(), 2400);
    assert!(get(&s, 3).duration.is_estimated());
    // Ajout en fin : aucun ID d'avant ne change, pas de renumérotation.
    let o = apply(&mut s, json!([{ "op": "task.add", "name": "D" }]));
    assert!(
        o.side_effects
            .iter()
            .all(|e| !matches!(e, SideEffect::IdsRenumbered { .. }))
    );
}

#[test]
fn r152_add_backward_alap() {
    let p = Project::new(ProjectInit {
        name: "P".to_owned(),
        anchor: ScheduleAnchor::FromFinish(moment("2026-11-30T17:00")),
        current_date: moment("2026-10-17T08:00"),
        calendar_names: BuiltinCalendarNames::default(),
    });
    let mut s = session(p);
    apply(&mut s, json!([{ "op": "task.add", "name": "A" }]));
    assert_eq!(get(&s, 1).constraint, Constraint::AsLateAsPossible);
}

#[test]
fn r152_placements_all_five() {
    let cases = [
        (json!("end"), n(&[("A", 1), ("A1", 2), ("B", 1), ("X", 1)])),
        (
            json!({ "after": 1 }),
            n(&[("A", 1), ("A1", 2), ("X", 1), ("B", 1)]),
        ),
        (
            json!({ "before": 2 }),
            n(&[("A", 1), ("X", 2), ("A1", 2), ("B", 1)]),
        ),
        (
            json!({ "first_child_of": 3 }),
            n(&[("A", 1), ("A1", 2), ("B", 1), ("X", 2)]),
        ),
        (
            json!({ "last_child_of": 1 }),
            n(&[("A", 1), ("A1", 2), ("X", 2), ("B", 1)]),
        ),
    ];
    for (at, expected) in cases {
        let mut s = abc();
        apply(&mut s, json!([{ "op": "task.add", "name": "X", "at": at }]));
        assert_eq!(names(&s), expected, "{at}");
    }
    // Exemple de la fiche : X en ID 3, IdsRenumbered { from: 3, count: 2 }.
    let mut s = abc();
    let o = apply(
        &mut s,
        json!([{ "op": "task.add", "name": "X", "at": { "after": 1 } }]),
    );
    assert_eq!(s.project().task_id(TaskUid::new(4)), Some(TaskId::new(3)));
    assert!(o.side_effects.contains(&SideEffect::IdsRenumbered {
        from: TaskId::new(3),
        count: 2
    }));
    // Une feuille qui reçoit un enfant devient récapitulative.
    let mut s = abc();
    apply(
        &mut s,
        json!([{ "op": "task.add", "name": "X", "at": { "first_child_of": 3 } }]),
    );
    let summary = s.schedule().scheduled(TaskUid::new(3)).unwrap();
    assert_eq!(summary.role, plannercraft_core::TaskRole::Summary);
    // Placement par étiquette du même lot.
    let mut s = abc();
    apply(
        &mut s,
        json!([
            { "op": "task.add", "name": "P", "as": "$p" },
            { "op": "task.add", "name": "C", "at": { "last_child_of": "$p" } },
        ]),
    );
    assert_eq!(names(&s), n(&[("A", 1), ("A1", 2), ("B", 1), ("P", 1), ("C", 2)]));
    // Cible inconnue : refus.
    let mut s = abc();
    let err = try_apply(
        &mut s,
        json!([{ "op": "task.add", "name": "X", "at": { "after": 99 } }]),
    )
    .unwrap_err();
    assert_eq!(err.code, codes::TASK_NOT_FOUND);
    assert_eq!(err.field.as_deref(), Some("at"));
}

#[test]
fn r152_name_too_long_refused() {
    let mut s = session(new_project("2026-11-02T08:00"));
    let long: String = "é".repeat(513);
    let err = try_apply(&mut s, json!([{ "op": "task.add", "name": long }])).unwrap_err();
    assert_eq!(err.code, codes::PARAM_INVALID);
    assert_eq!(err.field.as_deref(), Some("name"));
    assert_eq!(err.op_index, Some(0));
    assert!(s.project().tasks.is_empty());
    // 512 caractères et nom vide : acceptés.
    let ok: String = "é".repeat(512);
    apply(
        &mut s,
        json!([{ "op": "task.add", "name": ok }, { "op": "task.add", "name": "" }]),
    );
    assert_eq!(s.project().tasks.len(), 2);
    // `task.update` : même borne.
    let err = try_apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 1, "name": "x".repeat(513) }]),
    )
    .unwrap_err();
    assert_eq!(err.code, codes::PARAM_INVALID);
    assert_eq!(err.field.as_deref(), Some("name"));
}
