//! R-227 — `baseline.set` : numéro 0 à 10, portée et cumul par
//! `baseline::capture` sur le planning de la session, refus d'une ligne
//! existante sans `overwrite` (`baseline.exists`), effet `BaselineSet`,
//! annulable ; en `dry_run`, diff vide et effet rendu. Repère : calendrier
//! Standard, début du projet lun 19/10/2026 08:00 ; A7 relu pour l'exemple
//! du cahier (`baseline.exists { number: 0, count: 26 }`).

use plannercraft_automation::error::{CommandErrorKind, codes};
use plannercraft_automation::text::Language;
use plannercraft_automation::{ApplyMode, DocOrigin, Session, SideEffect};
use plannercraft_core::{BaselineNumber, Diagnostic, Money, TaskUid};
use plannercraft_tests::moment;
use plannercraft_tests::resource_edit::with_tasks;
use plannercraft_tests::task_edit::{apply, batch, get, try_apply};
use serde_json::json;

/// A 2 j → B 3 j (FS), R affectée à A.
fn project() -> Session {
    let mut s = with_tasks(&[("A", "2j"), ("B", "3j")]);
    apply(
        &mut s,
        json!([
            { "op": "link.add", "pred": 1, "succ": 2 },
            { "op": "resource.add", "name": "R" },
            { "op": "assignment.add", "task": 1, "resource": 1 }
        ]),
    );
    s
}

#[test]
fn r227_exists_refused() {
    let mut s = project();
    let ops = batch(json!([{ "op": "baseline.set" }]));
    let dry = s.apply(&ops, ApplyMode::DryRun).unwrap();
    assert_eq!(dry.diff.changed_total, 0);
    assert_eq!(
        dry.side_effects,
        [SideEffect::BaselineSet {
            number: 0,
            tasks: 2,
            assignments: 1,
            resources: 1
        }]
    );
    assert_eq!(get(&s, 1).baselines.len(), 0);
    let out = s.apply(&ops, ApplyMode::Commit).unwrap();
    assert_eq!(out.side_effects, dry.side_effects);
    let b = &get(&s, 2).baselines[&BaselineNumber::PRIMARY];
    assert_eq!(
        (b.start, b.finish),
        (Some(moment("2026-10-21T08:00")), Some(moment("2026-10-23T17:00")))
    );
    // Seconde fois : refusée, avec le nombre d'entités qui l'ont.
    let err = s.apply(&ops, ApplyMode::Commit).unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref(), &err.kind),
        (
            codes::BASELINE_EXISTS,
            Some("overwrite"),
            &CommandErrorKind::BaselineExists { number: 0, count: 4 }
        )
    );
    // Autre numéro : accepté ; numéro hors bornes, cumul sans sélection,
    // tâche inconnue : refusés.
    apply(
        &mut s,
        json!([{ "op": "baseline.set", "number": 10, "tasks": [2] }]),
    );
    assert!(
        get(&s, 2)
            .baselines
            .contains_key(&BaselineNumber::new(10).unwrap())
    );
    assert!(
        !get(&s, 1)
            .baselines
            .contains_key(&BaselineNumber::new(10).unwrap())
    );
    for (op, code, field) in [
        (
            json!({ "op": "baseline.set", "number": 11 }),
            codes::PARAM_INVALID,
            "number",
        ),
        (
            json!({ "op": "baseline.set", "number": 1, "rollup": "to_all_summaries" }),
            codes::BASELINE_ROLLUP_NEEDS_SELECTION,
            "rollup",
        ),
        (
            json!({ "op": "baseline.set", "number": 1, "tasks": [7] }),
            codes::TASK_NOT_FOUND,
            "tasks",
        ),
    ] {
        let err = try_apply(&mut s, json!([op.clone()])).unwrap_err();
        assert_eq!((err.code, err.field.as_deref()), (code, Some(field)), "{op}");
    }
}

#[test]
fn r227_overwrite() {
    let mut s = project();
    apply(&mut s, json!([{ "op": "baseline.set" }]));
    // B retardée d'un jour, ligne 0 remplacée : nouvelles dates.
    apply(
        &mut s,
        json!([
            { "op": "task.set_constraint", "uid": 2, "kind": "snet", "date": "2026-10-22T08:00" },
            { "op": "baseline.set", "overwrite": true }
        ]),
    );
    let b = &get(&s, 2).baselines[&BaselineNumber::PRIMARY];
    assert_eq!(b.start, Some(moment("2026-10-22T08:00")));
    // Ligne de base du fichier avec un coût : recopié, information.
    let mut p = s.project().clone();
    p.tasks
        .get_mut(TaskUid::new(1))
        .unwrap()
        .baselines
        .get_mut(&BaselineNumber::PRIMARY)
        .unwrap()
        .cost = Some(Money::new(800_000).unwrap());
    let origin = DocOrigin {
        name: "P".to_owned(),
        path: None,
    };
    let mut s = Session::from_project(p, origin, moment("2026-10-17T08:00"), Language::Fr);
    let out = apply(&mut s, json!([{ "op": "baseline.set", "overwrite": true }]));
    assert!(out.warnings.contains(&Diagnostic::BaselineCostNotComputed {
        number: BaselineNumber::PRIMARY
    }));
    assert_eq!(
        get(&s, 1).baselines[&BaselineNumber::PRIMARY].cost,
        Some(Money::new(800_000).unwrap())
    );
}

#[test]
fn r227_undo_restores_previous_line() {
    let mut s = project();
    apply(&mut s, json!([{ "op": "baseline.set" }]));
    let first = s.project().clone();
    apply(
        &mut s,
        json!([
            { "op": "task.update", "uid": 2, "duration": { "text": "5j" } },
            { "op": "baseline.set", "overwrite": true }
        ]),
    );
    assert_ne!(
        get(&s, 2).baselines[&BaselineNumber::PRIMARY],
        first.tasks.get(TaskUid::new(2)).unwrap().baselines[&BaselineNumber::PRIMARY]
    );
    s.undo().unwrap();
    assert_eq!(s.project(), &first);
    s.undo().unwrap();
    assert_eq!((get(&s, 1).baselines.len(), get(&s, 2).baselines.len()), (0, 0));
    s.redo().unwrap();
    assert_eq!(s.project(), &first);
}

#[test]
fn r227_skipped_tasks_info() {
    let mut s = with_tasks(&[("A", "1j"), ("I", "1j")]);
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 2, "active": false }]),
    );
    let out = apply(&mut s, json!([{ "op": "baseline.set" }]));
    assert!(
        out.warnings
            .contains(&Diagnostic::BaselineTaskSkipped { count: 1 })
    );
    assert_eq!(get(&s, 2).baselines.len(), 0);
}

#[test]
fn r227_corpus_a7_exists() {
    // A7 (fichier du corpus, absent hors du job `corpus`), O42 : ligne 0
    // sur 26 tâches, 36 affectations, 4 ressources. `baseline.set` 0 refusé,
    // `count` = entités de la portée projet qui l'ont déjà (R-223) ; avec
    // `overwrite`, accepté, coût recopié (information).
    let Ok(bytes) = std::fs::read(plannercraft_tests::tests_path("corpus/files/projinteg.xml")) else {
        return;
    };
    let (mut s, _) =
        Session::open_bytes(&bytes, origin("A7"), moment("2026-10-17T08:00"), Language::Fr).unwrap();
    let p = s.project();
    let zero = BaselineNumber::PRIMARY;
    let has = p
        .summary_task
        .iter()
        .chain(p.tasks.iter())
        .filter(|t| t.baselines.contains_key(&zero))
        .count()
        + p.assignments
            .iter()
            .filter(|a| a.baselines.contains_key(&zero))
            .count()
        + p.resources
            .iter()
            .filter(|r| r.baselines.contains_key(&zero))
            .count();
    let err = try_apply(&mut s, json!([{ "op": "baseline.set" }])).unwrap_err();
    assert_eq!(
        (err.code, err.kind),
        (
            codes::BASELINE_EXISTS,
            CommandErrorKind::BaselineExists {
                number: 0,
                count: u32::try_from(has).unwrap()
            }
        )
    );
    let out = apply(&mut s, json!([{ "op": "baseline.set", "overwrite": true }]));
    assert!(
        out.warnings
            .contains(&Diagnostic::BaselineCostNotComputed { number: zero })
    );
}

fn origin(name: &str) -> DocOrigin {
    DocOrigin {
        name: name.to_owned(),
        path: None,
    }
}
