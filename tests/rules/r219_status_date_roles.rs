//! R-219 — Rôles de la date d'état (amende R-034) : défaut de `after` de
//! `progress.reschedule` (R-217) ; aucun effet sur le recalcul (P37), avec
//! ou sans avancement ; date future ou antérieure au début du projet
//! admise. Repère : calendrier Standard, début du projet lun 19/10/2026
//! 08:00, date courante injectée sam 17/10/2026 08:00.

use plannercraft_core::{Constraint, TaskUid};
use plannercraft_tests::moment;
use plannercraft_tests::resource_edit::with_tasks;
use plannercraft_tests::task_edit::{apply, get};
use serde_json::json;

#[test]
fn r219_status_date_no_effect() {
    // A 2 j commencée (40 %) → B 3 j non commencée.
    let mut s = with_tasks(&[("A", "2j"), ("B", "3j")]);
    apply(
        &mut s,
        json!([
            { "op": "link.add", "pred": 1, "succ": 2 },
            { "op": "progress.update", "task": 1, "percent_complete": { "text": "40%" } }
        ]),
    );
    let before = s.schedule().clone();
    for date in ["2026-10-20T12:00", "2027-03-01T08:00", "2026-01-05T08:00"] {
        let out = apply(&mut s, json!([{ "op": "project.set_status_date", "date": date }]));
        assert_eq!(out.diff.changed_total, 0, "{date}");
        assert_eq!(s.schedule().tasks, before.tasks, "{date}");
        assert_eq!(s.project().status_date, Some(moment(date)));
    }
    let out = apply(&mut s, json!([{ "op": "project.set_status_date", "date": null }]));
    assert_eq!(out.diff.changed_total, 0);
    assert_eq!(s.project().status_date, None);
}

#[test]
fn r219_default_after_is_effective_status_date() {
    // Date d'état NA : la date courante (sam 17/10) précède tout début ;
    // rien ne bouge.
    let mut s = with_tasks(&[("W", "1j")]);
    let out = apply(&mut s, json!([{ "op": "progress.reschedule" }]));
    assert_eq!(out.side_effects.len(), 0);
    assert_eq!(get(&s, 1).constraint, Constraint::AsSoonAsPossible);
    // Date d'état mer 21/10 08:00 : W (prévue lun 19/10) reçoit la SNET.
    apply(
        &mut s,
        json!([
            { "op": "project.set_status_date", "date": "2026-10-21T08:00" },
            { "op": "progress.reschedule" }
        ]),
    );
    assert_eq!(
        get(&s, 1).constraint,
        Constraint::StartNoEarlierThan {
            date: moment("2026-10-21T08:00")
        }
    );
    let Some(plannercraft_core::TaskOutcome::Scheduled(w)) = s.schedule().get(TaskUid::new(1)) else {
        panic!("W non planifiée");
    };
    assert_eq!(w.start, moment("2026-10-21T08:00"));
}
