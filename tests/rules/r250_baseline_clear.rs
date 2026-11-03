//! R-250 — Effacer une ligne de base (`baseline.clear`) : sans `tasks`,
//! toutes les tâches (UID 0 compris), leurs affectations et les
//! ressources ; avec `tasks`, ces tâches et leurs affectations (ressources
//! inchangées, hypothèse) ; récapitulatives non recalculées ; `Extras`
//! `Baseline#n/` jetés ; rien à effacer : `baseline.nothing_to_clear`
//! (information), aucun changement ; une entrée d'annulation. Ensuite,
//! écarts « sans ligne de base » (R-224, R-225). Repère : calendrier
//! Standard, début lun 19/10/2026 08:00.

use plannercraft_automation::error::codes;
use plannercraft_automation::{ApplyMode, Session, SideEffect};
use plannercraft_core::baseline;
use plannercraft_core::{
    AssignmentUid, BaselineNumber, CalendarSet, Diagnostic, ExtraEntry, Minutes, ResourceUid, TaskUid,
};
use plannercraft_mspdi::write::WriteOptions;
use plannercraft_tests::mspdi::compact;
use plannercraft_tests::resource_edit::with_tasks;
use plannercraft_tests::task_edit::{apply, batch, get, try_apply};
use serde_json::json;

/// A 2 j → B 3 j (FS), R affectée à A ; ligne 0 posée.
fn project() -> Session {
    let mut s = with_tasks(&[("A", "2j"), ("B", "3j")]);
    apply(
        &mut s,
        json!([
            { "op": "link.add", "pred": 1, "succ": 2 },
            { "op": "resource.add", "name": "R" },
            { "op": "assignment.add", "task": 1, "resource": 1 },
            { "op": "baseline.set" }
        ]),
    );
    s
}

const N0: BaselineNumber = BaselineNumber::PRIMARY;

fn has_line(s: &Session, task: u32) -> bool {
    get(s, task).baselines.contains_key(&N0)
}

fn finish_variance(s: &Session, task: u32) -> Minutes {
    let (cals, _) = CalendarSet::compile(s.project());
    let v = baseline::variance(s.project(), s.schedule(), &cals, N0);
    v.tasks
        .iter()
        .find(|t| t.uid == TaskUid::new(task))
        .unwrap()
        .variance
        .finish
}

#[test]
fn r250_clear_project() {
    // Cahier : ligne 0, B retardée d'un jour ⇒ écart de fin +480 ;
    // `clear_baseline 0` ⇒ plus de ligne, écart de fin 0, écart de
    // travail = W.
    let mut s = project();
    apply(
        &mut s,
        json!([{ "op": "task.set_constraint", "uid": 2, "kind": "snet", "date": "2026-10-22T08:00" }]),
    );
    assert_eq!(finish_variance(&s, 2), Minutes::new(480));
    // Extras de portée `Baseline#0/` relus d'un fichier : jetés.
    let mut project = s.project().clone();
    let entry = |key: &str| ExtraEntry {
        origin: plannercraft_mspdi::extras::ORIGIN.to_owned(),
        key: key.to_owned(),
        payload: String::new(),
    };
    let extras = &mut project.tasks.get_mut(TaskUid::new(1)).unwrap().extras.0;
    extras.push(entry("Baseline#0/BCWS"));
    extras.push(entry("Baseline#1/BCWS"));
    let mut s = plannercraft_tests::session::session(project);
    let dry = s
        .apply(&batch(json!([{ "op": "baseline.clear" }])), ApplyMode::DryRun)
        .unwrap();
    assert!(has_line(&s, 1));
    let out = apply(&mut s, json!([{ "op": "baseline.clear" }]));
    assert_eq!(out.side_effects, dry.side_effects);
    assert_eq!(
        out.side_effects[0],
        SideEffect::BaselineCleared {
            number: 0,
            tasks: 2,
            assignments: 1,
            resources: 1
        }
    );
    assert!(out.side_effects.contains(&SideEffect::ExtrasDropped {
        subject: plannercraft_core::Subject::Task(TaskUid::new(1)),
        keys: vec!["Baseline#0/BCWS".to_owned()],
    }));
    assert!(!has_line(&s, 1) && !has_line(&s, 2));
    let p = s.project();
    assert!(p.resources.get(ResourceUid::new(1)).unwrap().baselines.is_empty());
    assert!(
        p.assignments
            .get(AssignmentUid::new(1))
            .unwrap()
            .baselines
            .is_empty()
    );
    // Ligne 1 de portée intacte.
    assert!(get(&s, 1).extras.0.iter().any(|e| e.key == "Baseline#1/BCWS"));
    assert_eq!(finish_variance(&s, 2), Minutes::ZERO);
    let (cals, _) = CalendarSet::compile(s.project());
    let v = baseline::variance(s.project(), s.schedule(), &cals, N0);
    let a = v.tasks.iter().find(|t| t.uid == TaskUid::new(1)).unwrap();
    assert_eq!((a.baseline, a.variance.work), (None, Minutes::new(960)));
    // MSPDI : écart de fin 0, écart de travail = W (en millièmes de minute).
    let xml = String::from_utf8(s.save_bytes(&WriteOptions::default()).xml).unwrap();
    let xml = compact(&xml);
    assert!(
        !xml.contains("<Baseline><Number>0</Number>"),
        "ligne de base écrite"
    );
    assert!(xml.contains("<FinishVariance>0</FinishVariance>"));
    assert!(xml.contains("<WorkVariance>960000.00</WorkVariance>"), "{xml}");
}

#[test]
fn r250_clear_selection() {
    let mut s = project();
    apply(&mut s, json!([{ "op": "baseline.set", "number": 2 }]));
    let o = apply(&mut s, json!([{ "op": "baseline.clear", "tasks": [1] }]));
    assert_eq!(
        o.side_effects,
        [SideEffect::BaselineCleared {
            number: 0,
            tasks: 1,
            assignments: 1,
            resources: 0
        }]
    );
    assert!(!has_line(&s, 1) && has_line(&s, 2));
    let p = s.project();
    // Ressource inchangée (hypothèse) ; ligne 2 intacte.
    assert!(
        p.resources
            .get(ResourceUid::new(1))
            .unwrap()
            .baselines
            .contains_key(&N0)
    );
    assert!(
        get(&s, 1)
            .baselines
            .contains_key(&BaselineNumber::new(2).unwrap())
    );
    // Numéro hors bornes, tâche inconnue : refusés.
    for (op, code, field) in [
        (
            json!({ "op": "baseline.clear", "number": 11 }),
            codes::PARAM_INVALID,
            "number",
        ),
        (
            json!({ "op": "baseline.clear", "tasks": [9] }),
            codes::TASK_NOT_FOUND,
            "tasks",
        ),
    ] {
        let err = try_apply(&mut s, json!([op.clone()])).unwrap_err();
        assert_eq!((err.code, err.field.as_deref()), (code, Some(field)), "{op}");
    }
}

#[test]
fn r250_nothing_to_clear() {
    let mut s = project();
    let before = s.project().clone();
    let o = apply(&mut s, json!([{ "op": "baseline.clear", "number": 3 }]));
    assert_eq!(
        o.warnings,
        [Diagnostic::BaselineNothingToClear {
            number: BaselineNumber::new(3).unwrap()
        }]
    );
    assert_eq!(o.side_effects, []);
    assert_eq!(s.project(), &before);
    assert_eq!(o.diff.changed_total, 0);
}

#[test]
fn r250_undo_restores() {
    let mut s = project();
    let before = s.project().clone();
    let len = s.history().entries.len();
    apply(&mut s, json!([{ "op": "baseline.clear" }]));
    assert_eq!(s.history().entries.len(), len + 1);
    assert!(!has_line(&s, 1));
    s.undo().unwrap();
    assert_eq!(s.project(), &before);
    s.redo().unwrap();
    assert!(!has_line(&s, 2));
}
