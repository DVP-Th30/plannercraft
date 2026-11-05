//! R-267 — Décaler des tâches (`task.shift { tasks, by }`, outil MCP
//! `shift_tasks`) : tâche automatique ⇒ contrainte « Début au plus tôt le »
//! (SNET) à son début courant décalé de `by` sur son calendrier ; tâche
//! manuelle ⇒ début et fin décalés, durée gardée ; récapitulative ⇒ ses
//! feuilles ; inactive ou achevée ⇒ ignorée (effet `shift_skipped`) ;
//! contrainte autre que ASAP / SNET remplacée (`shift_constraint_replaced`) ;
//! début retenu par les prédécesseurs ⇒ décalage fait, `shift_limited` ; une
//! entrée d'annulation par lot. Repère : projet « P » (2 « Fondations » lun
//! 19/10 – ven 23/10, 3 « Murs » lun 26/10 – ven 06/11, 6 « Plâtrerie » lun
//! 09/11 – jeu 12/11). Source : hypothèse (finitions P1, ADR-0022 D1).

use plannercraft_automation::error::codes;
use plannercraft_automation::text::Language;
use plannercraft_automation::{ApplyOutcome, SideEffect, Workspace};
use plannercraft_core::{Constraint, ConstraintKind, SchedulingMode, TaskUid};
use plannercraft_tests::daily::{dates, plan_p};
use plannercraft_tests::moment;
use plannercraft_tests::task_edit::{apply, get, try_apply};
use serde_json::json;

fn uid(n: u32) -> TaskUid {
    TaskUid::new(n)
}

/// Tâches ignorées par le lot, avec leur raison.
fn skipped(out: &ApplyOutcome) -> Vec<(u32, &'static str)> {
    out.side_effects
        .iter()
        .filter_map(|e| match e {
            SideEffect::ShiftSkipped { task, reason } => Some((task.get(), *reason)),
            _ => None,
        })
        .collect()
}

#[test]
fn r267_auto_forward_sets_snet() {
    let mut s = plan_p();
    let len = s.history().entries.len();
    let before = s.project().clone();
    let out = apply(
        &mut s,
        json!([{ "op": "task.shift", "tasks": [2], "by": { "text": "1j" } }]),
    );
    assert_eq!(
        get(&s, 2).constraint,
        Constraint::StartNoEarlierThan {
            date: moment("2026-10-20T08:00")
        }
    );
    assert_eq!(
        dates(&s, 2),
        (moment("2026-10-20T08:00"), moment("2026-10-26T17:00"))
    );
    // Le successeur suit par son lien.
    assert_eq!(dates(&s, 3).0, moment("2026-10-27T08:00"));
    assert!(out.side_effects.contains(&SideEffect::ConstraintSet {
        uid: uid(2),
        kind: ConstraintKind::StartNoEarlierThan,
    }));
    // Une entrée d'annulation ; annuler rend le projet.
    assert_eq!(s.history().entries.len(), len + 1);
    s.undo().unwrap();
    assert_eq!(s.project(), &before);
}

#[test]
fn r267_week_on_task_calendar_and_minutes_form() {
    let mut s = plan_p();
    // « 1sem » = 5 jours ouvrés du calendrier Standard : lun 26/10 ⇒ lun
    // 02/11 ; le samedi et le dimanche sont sautés.
    apply(
        &mut s,
        json!([{ "op": "task.shift", "tasks": [3], "by": { "text": "1sem" } }]),
    );
    assert_eq!(
        get(&s, 3).constraint,
        Constraint::StartNoEarlierThan {
            date: moment("2026-11-02T08:00")
        }
    );
    // Minutes signées : −480 (un jour ouvré en arrière depuis le 02/11).
    apply(
        &mut s,
        json!([{ "op": "task.shift", "tasks": [3], "by": { "minutes": -480, "unit": "day" } }]),
    );
    assert_eq!(
        get(&s, 3).constraint,
        Constraint::StartNoEarlierThan {
            date: moment("2026-10-30T08:00")
        }
    );
}

#[test]
fn r267_backward_limited_by_predecessors() {
    let mut s = plan_p();
    let out = apply(
        &mut s,
        json!([{ "op": "task.shift", "tasks": [3], "by": { "text": "-1j" } }]),
    );
    // Contrainte posée au ven 23/10, mais 2 finit ce jour-là : 3 reste au
    // lun 26/10, décalage signalé comme limité.
    assert_eq!(
        get(&s, 3).constraint,
        Constraint::StartNoEarlierThan {
            date: moment("2026-10-23T08:00")
        }
    );
    assert_eq!(dates(&s, 3).0, moment("2026-10-26T08:00"));
    assert!(
        out.side_effects
            .contains(&SideEffect::ShiftLimited { task: uid(3) })
    );
}

#[test]
fn r267_manual_moves_dates_keeps_duration() {
    let mut s = plan_p();
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 6, "mode": "manual" }]),
    );
    apply(
        &mut s,
        json!([{ "op": "task.shift", "tasks": [6], "by": { "text": "1sem" } }]),
    );
    let SchedulingMode::Manual {
        start,
        finish,
        duration,
    } = get(&s, 6).mode
    else {
        panic!("6 devrait rester manuelle");
    };
    assert_eq!(start, Some(moment("2026-11-16T08:00")));
    assert_eq!(finish, Some(moment("2026-11-19T17:00")));
    assert_eq!(duration.map(|d| d.minutes().get()), Some(4 * 480));
    // Aucune contrainte posée sur une tâche manuelle.
    assert_eq!(get(&s, 6).constraint, Constraint::AsSoonAsPossible);
    assert_eq!(
        dates(&s, 6),
        (moment("2026-11-16T08:00"), moment("2026-11-19T17:00"))
    );
}

#[test]
fn r267_summary_means_leaves_skips_inactive_and_complete() {
    let mut s = plan_p();
    apply(
        &mut s,
        json!([
            { "op": "progress.update", "task": 2, "percent_complete": { "value": 10000 } },
            { "op": "task.update", "uid": 6, "active": false }
        ]),
    );
    let out = apply(
        &mut s,
        json!([{ "op": "task.shift", "tasks": [1, 5], "by": { "text": "2j" } }]),
    );
    assert_eq!(skipped(&out), vec![(2, "complete"), (6, "inactive")]);
    // 3 et le jalon 4 (sous 1), 7 (sous 5) : contrainte posée ; les
    // récapitulatives restent calculées.
    for n in [3, 4, 7] {
        assert_eq!(
            get(&s, n).constraint.kind(),
            ConstraintKind::StartNoEarlierThan,
            "{n}"
        );
    }
    for n in [1, 2, 5, 6] {
        assert_eq!(get(&s, n).constraint, Constraint::AsSoonAsPossible, "{n}");
    }
}

#[test]
fn r267_other_constraint_replaced() {
    let mut s = plan_p();
    apply(
        &mut s,
        json!([{ "op": "task.set_constraint", "uid": 7, "kind": "fnlt", "date": "2026-11-30T17:00" }]),
    );
    let out = apply(
        &mut s,
        json!([{ "op": "task.shift", "tasks": [7, 6], "by": { "text": "1j" } }]),
    );
    assert!(out.side_effects.contains(&SideEffect::ShiftConstraintReplaced {
        task: uid(7),
        previous: ConstraintKind::FinishNoLaterThan,
    }));
    assert!(
        !out.side_effects
            .iter()
            .any(|e| matches!(e, SideEffect::ShiftConstraintReplaced { task, .. } if *task == uid(6)))
    );
    assert_eq!(get(&s, 7).constraint.kind(), ConstraintKind::StartNoEarlierThan);
}

#[test]
fn r267_zero_and_errors() {
    let mut s = plan_p();
    let before = s.project().clone();
    // Décalage nul : rien ne change.
    apply(
        &mut s,
        json!([{ "op": "task.shift", "tasks": [2], "by": { "minutes": 0 } }]),
    );
    assert_eq!(s.project(), &before);
    for (by, field) in [
        (json!({ "text": "50%" }), "by"),
        (json!({ "text": "2j?" }), "by"),
        (json!({ "minutes": 480, "estimated": true }), "by"),
        (json!({}), "by"),
    ] {
        let err = try_apply(&mut s, json!([{ "op": "task.shift", "tasks": [2], "by": by }])).unwrap_err();
        assert!(
            err.code == codes::PARAM_INVALID || err.code.starts_with("text."),
            "{}",
            err.code
        );
        assert_eq!(err.field.as_deref(), Some(field));
    }
    let err = try_apply(
        &mut s,
        json!([{ "op": "task.shift", "tasks": [], "by": { "text": "1j" } }]),
    )
    .unwrap_err();
    assert_eq!(err.code, codes::PARAM_INVALID);
    assert_eq!(err.field.as_deref(), Some("tasks"));
    let err = try_apply(
        &mut s,
        json!([{ "op": "task.shift", "tasks": [99], "by": { "text": "1j" } }]),
    )
    .unwrap_err();
    assert_eq!(err.code, codes::TASK_NOT_FOUND);
    assert_eq!(s.project(), &before);
}

#[test]
fn r267_mcp_shift_tasks() {
    let spec = plannercraft_automation::registry::lookup("task.shift").unwrap();
    assert_eq!(spec.mcp_name, "shift_tasks");
    assert!(spec.is_listed() && spec.dry_run && spec.undoable);
    let mut ws = Workspace::new(moment("2026-11-02T08:00"), Language::Fr);
    ws.call(
        "project.new",
        json!({ "name": "Essai", "start": "2026-11-02T08:00" }),
    )
    .unwrap();
    let out = ws
        .call(
            "batch.apply",
            json!({ "ops": [{ "op": "task.add", "name": "A", "as": "$a", "duration": { "text": "2j" } }] }),
        )
        .unwrap();
    let a = out["labels"]["$a"].as_u64().unwrap();
    ws.call(
        "task.shift",
        json!({ "tasks": [a], "by": { "text": "1d" }, "dry_run": true }),
    )
    .unwrap();
    let a_uid = TaskUid::new(u32::try_from(a).unwrap());
    let constraint = |ws: &Workspace| {
        ws.session()
            .unwrap()
            .project()
            .tasks
            .get(a_uid)
            .unwrap()
            .constraint
    };
    assert_eq!(constraint(&ws), Constraint::AsSoonAsPossible);
    let out = ws
        .call("task.shift", json!({ "tasks": [a], "by": { "text": "1d" } }))
        .unwrap();
    assert_eq!(
        out["side_effects"][0]["constraint_set"]["kind"],
        json!("start_no_earlier_than")
    );
    assert_eq!(
        constraint(&ws),
        Constraint::StartNoEarlierThan {
            date: moment("2026-11-03T08:00")
        }
    );
    assert_eq!(ws.session().unwrap().history().entries.len(), 2);
}
