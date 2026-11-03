//! R-197 — `task.update` étendu (`type`, `effort_driven`, `work`) et
//! défauts du projet (`default_task_type`, `new_tasks_effort_driven`) :
//! durée et travail ensemble refusés (`param.conflict`), récapitulative
//! refusée (`task.computed_field`), travail fixe toujours piloté, durée d'une
//! feuille affectée équilibrée (R-189 « durée »), défauts pris par les seules
//! tâches créées ensuite. Repère : calendrier Standard (1 j = 480 min).

use plannercraft_automation::error::codes;
use plannercraft_core::TaskType;
use plannercraft_tests::resource_edit::{duration, units_work, with_tasks, work};
use plannercraft_tests::task_edit::{apply, get, try_apply};
use serde_json::json;

/// Tâche A de 2 j, une ressource R affectée à 100 % (16 h).
fn assigned(task_type: &str) -> plannercraft_automation::Session {
    let mut s = with_tasks(&[("A", "2j")]);
    apply(
        &mut s,
        json!([
            { "op": "task.update", "uid": 1, "type": task_type },
            { "op": "resource.add", "name": "R" },
            { "op": "assignment.add", "task": 1, "resource": 1 }
        ]),
    );
    assert_eq!(units_work(&s, 1, 1), (10_000, 960));
    s
}

#[test]
fn r197_param_conflict() {
    let mut s = assigned("fixed_units");
    let err = try_apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 1, "duration": { "text": "3j" }, "work": { "text": "8h" } }]),
    )
    .unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref()),
        (codes::PARAM_CONFLICT, Some("work"))
    );
    // Travail saisi sans affectation de travail : refusé sur son champ.
    let mut s = with_tasks(&[("B", "1j")]);
    let err = try_apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 1, "work": { "text": "8h" } }]),
    )
    .unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref()),
        (codes::PARAM_INVALID, Some("work"))
    );
}

#[test]
fn r197_summary_refused() {
    let mut s = with_tasks(&[("Lot", "1j"), ("A", "1j")]);
    apply(&mut s, json!([{ "op": "task.indent", "uid": 2 }]));
    for (op, field) in [
        (
            json!({ "op": "task.update", "uid": 1, "type": "fixed_work" }),
            "type",
        ),
        (
            json!({ "op": "task.update", "uid": 1, "effort_driven": true }),
            "effort_driven",
        ),
        (
            json!({ "op": "task.update", "uid": 1, "work": { "text": "8h" } }),
            "work",
        ),
    ] {
        let err = try_apply(&mut s, json!([op.clone()])).unwrap_err();
        assert_eq!(
            (err.code, err.field.as_deref()),
            (codes::TASK_COMPUTED_FIELD, Some(field)),
            "{op}"
        );
    }
}

#[test]
fn r197_fixed_work_always_effort_driven() {
    // `type: fixed_work` sur une tâche non pilotée la rend pilotée ;
    // `effort_driven: false` sur une tâche à travail fixe est refusé.
    let mut s = with_tasks(&[("A", "2j")]);
    assert!(!get(&s, 1).effort_driven);
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 1, "type": "fixed_work" }]),
    );
    assert_eq!(get(&s, 1).task_type, TaskType::FixedWork);
    assert!(get(&s, 1).effort_driven);
    let err = try_apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 1, "effort_driven": false }]),
    )
    .unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref()),
        (codes::TASK_FIXED_WORK_EFFORT_DRIVEN, Some("effort_driven"))
    );
    // Changer de type ne recalcule rien (R-189).
    let mut s = assigned("fixed_units");
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 1, "type": "fixed_duration" }]),
    );
    assert_eq!((duration(&s, 1), units_work(&s, 1, 1)), (960, (10_000, 960)));
}

#[test]
fn r197_duration_on_assigned_leaf() {
    // R-189 « durée » (une affectation à 100 %, 2 j, 16 h) : unités fixes et
    // durée fixe : 3 j ⇒ 24 h ; travail fixe : 4 j ⇒ 50 %.
    for (kind, days, expected) in [
        ("fixed_units", "3j", (10_000, 1_440)),
        ("fixed_duration", "3j", (10_000, 1_440)),
        ("fixed_work", "4j", (5_000, 960)),
    ] {
        let mut s = assigned(kind);
        apply(
            &mut s,
            json!([{ "op": "task.update", "uid": 1, "duration": { "text": days } }]),
        );
        assert_eq!(units_work(&s, 1, 1), expected, "{kind}");
        assert_eq!(work(&s, 1), expected.1, "{kind}");
    }
}

#[test]
fn r197_work_on_assigned_task() {
    // R-194 par `task.update work` : unités fixes 2 j, R1 100 % 16 h, R2
    // 50 % 8 h ; travail → 48 h ⇒ 32 h et 16 h, durée 4 j. Durée fixe :
    // travail → 24 h ⇒ unités recalculées (150 %, R-189).
    let mut s = with_tasks(&[("A", "2j")]);
    apply(
        &mut s,
        json!([
            { "op": "resource.add", "name": "R1" },
            { "op": "resource.add", "name": "R2" },
            { "op": "assignment.add", "task": 1, "resource": 1 },
            { "op": "assignment.add", "task": 1, "resource": 2, "units": { "text": "50%" } }
        ]),
    );
    assert_eq!(work(&s, 1), 1_440);
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 1, "work": { "text": "48h" } }]),
    );
    assert_eq!(units_work(&s, 1, 1), (10_000, 1_920));
    assert_eq!(units_work(&s, 1, 2), (5_000, 960));
    assert_eq!((duration(&s, 1), work(&s, 1)), (1_920, 2_880));
    let mut s = assigned("fixed_duration");
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 1, "work": { "text": "24h" } }]),
    );
    assert_eq!((duration(&s, 1), units_work(&s, 1, 1)), (960, (15_000, 1_440)));
}

#[test]
fn r197_defaults_new_tasks_only() {
    // `project.set_info` : la tâche existante garde son type ; la suivante
    // prend durée fixe pilotée. Travail fixe par défaut, nouvelles tâches
    // non pilotées : la tâche créée l'est quand même (R-193).
    let mut s = with_tasks(&[("A", "1j")]);
    apply(
        &mut s,
        json!([
            { "op": "project.set_info", "default_task_type": "fixed_duration", "new_tasks_effort_driven": true },
            { "op": "task.add", "name": "B" }
        ]),
    );
    assert_eq!(
        (get(&s, 1).task_type, get(&s, 1).effort_driven),
        (TaskType::FixedUnits, false)
    );
    assert_eq!(
        (get(&s, 2).task_type, get(&s, 2).effort_driven),
        (TaskType::FixedDuration, true)
    );
    let o = s.project().options;
    assert_eq!(
        (o.default_task_type, o.new_tasks_effort_driven),
        (TaskType::FixedDuration, true)
    );
    apply(
        &mut s,
        json!([
            { "op": "project.set_info", "default_task_type": "fixed_work", "new_tasks_effort_driven": false },
            { "op": "task.add", "name": "C" }
        ]),
    );
    assert_eq!(
        (get(&s, 3).task_type, get(&s, 3).effort_driven),
        (TaskType::FixedWork, true)
    );
    let err = try_apply(
        &mut s,
        json!([{ "op": "project.set_info", "default_task_type": null }]),
    )
    .unwrap_err();
    assert_eq!(err.field.as_deref(), Some("default_task_type"));
}
