//! R-216 — `progress.update` : saisie d'avancement équilibrée par le cœur
//! (`progress::edit::apply`) sur les dates planifiées de la session, écrite
//! dans le projet (avancement, durée, travail réel des affectations) ;
//! refus du cœur rendus en erreurs de commande ; R-161 amendée (`Stop`,
//! `Resume`, `ResumeValid` jetés). Repère : calendrier Standard, début du
//! projet lun 19/10/2026 08:00 (1 j = 480 min).

use plannercraft_automation::error::codes;
use plannercraft_automation::{ApplyMode, ApplyOptions, Session};
use plannercraft_core::{
    Diagnostic, ExtraEntry, Minutes, Percent, Subject, TaskOutcome, TaskSchedule, TaskUid, Work,
};
use plannercraft_tests::moment;
use plannercraft_tests::resource_edit::{assignment, duration, with_tasks};
use plannercraft_tests::session::edit;
use plannercraft_tests::task_edit::{apply, batch, get, try_apply};
use serde_json::json;

/// Planning de la tâche `uid`.
fn sched(s: &Session, uid: u32) -> TaskSchedule {
    match s.schedule().get(TaskUid::new(uid)) {
        Some(TaskOutcome::Scheduled(t)) => *t,
        other => panic!("tâche {uid} non planifiée : {other:?}"),
    }
}

/// A 2 j → B 3 j (FS).
fn chain() -> Session {
    let mut s = with_tasks(&[("A", "2j"), ("B", "3j")]);
    apply(&mut s, json!([{ "op": "link.add", "pred": 1, "succ": 2 }]));
    s
}

#[test]
fn r216_update_moves_successors_dry_run() {
    let mut s = chain();
    assert_eq!(sched(&s, 2).finish, moment("2026-10-23T17:00"));
    let before = s.project().clone();
    let ops = batch(json!([{ "op": "progress.update", "task": 1, "actual_finish": "2026-10-20T12:00" }]));
    let dry = s.apply(&ops, ApplyMode::DryRun).unwrap();
    assert!(!dry.committed);
    assert_eq!(s.project(), &before);
    let b = dry
        .diff
        .changed
        .iter()
        .find(|c| c.uid == TaskUid::new(2))
        .unwrap();
    assert_eq!(
        b.finish,
        (Some(moment("2026-10-23T17:00")), Some(moment("2026-10-23T12:00")))
    );
    assert_eq!(b.finish_delta_min, -300);

    let done = s.apply(&ops, ApplyMode::Commit).unwrap();
    assert_eq!(done.diff, dry.diff);
    let a = get(&s, 1);
    assert_eq!(a.progress.percent_complete, Percent::FULL);
    assert_eq!(a.progress.actual_start, Some(moment("2026-10-19T08:00")));
    assert_eq!(a.progress.actual_finish, Some(moment("2026-10-20T12:00")));
    assert_eq!(a.progress.actual_duration, Minutes::new(720));
    assert_eq!(duration(&s, 1), 720);
    let b = sched(&s, 2);
    assert_eq!(
        (b.start, b.finish),
        (moment("2026-10-20T13:00"), moment("2026-10-23T12:00"))
    );
}

#[test]
fn r216_conflict_refused() {
    let mut s = chain();
    let before = s.project().clone();
    for (op, code, field) in [
        (
            json!({ "op": "progress.update", "task": 1, "percent_complete": { "text": "40%" },
                    "actual_duration": { "text": "1j" } }),
            codes::PARAM_CONFLICT,
            "actual_duration",
        ),
        (
            json!({ "op": "progress.update", "task": 1, "percent_complete": { "value": 5_000 },
                    "actual_finish": "2026-10-20T12:00" }),
            codes::PARAM_CONFLICT,
            "actual_finish",
        ),
        (
            json!({ "op": "progress.update", "task": 1, "percent_complete": { "value": 3_000, "text": "40%" } }),
            codes::PARAM_MISMATCH,
            "percent_complete",
        ),
        (
            json!({ "op": "progress.update", "task": 1, "percent_complete": { "value": 10_001 } }),
            codes::PARAM_INVALID,
            "percent_complete",
        ),
        (
            json!({ "op": "progress.update", "task": 1, "actual_start": "2026-10-20T08:00",
                    "actual_finish": "2026-10-19T12:00" }),
            codes::PROGRESS_INVALID_ORDER,
            "actual_finish",
        ),
        (
            // Travail saisi sur une tâche sans affectation de travail.
            json!({ "op": "progress.update", "task": 1, "actual_work": { "text": "2h" } }),
            codes::PARAM_CONFLICT,
            "actual_work",
        ),
    ] {
        let err = try_apply(&mut s, json!([op.clone()])).unwrap_err();
        assert_eq!((err.code, err.field.as_deref()), (code, Some(field)), "{op}");
        assert_eq!(err.op_index, Some(0), "{op}");
        assert_eq!(s.project(), &before, "{op}");
    }
}

#[test]
fn r216_summary_refused() {
    let mut s = with_tasks(&[("Lot", "1j"), ("A", "1j"), ("B", "1j")]);
    apply(
        &mut s,
        json!([
            { "op": "task.indent", "uid": 2 },
            { "op": "task.update", "uid": 3, "active": false }
        ]),
    );
    let err = try_apply(
        &mut s,
        json!([{ "op": "progress.update", "task": 1, "percent_complete": { "text": "50%" } }]),
    )
    .unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref(), err.subject),
        (
            codes::PROGRESS_SUMMARY_NOT_EDITABLE,
            Some("task"),
            Some(Subject::Task(TaskUid::new(1)))
        )
    );
    let err = try_apply(
        &mut s,
        json!([{ "op": "progress.update", "task": 3, "percent_complete": { "text": "50%" } }]),
    )
    .unwrap_err();
    assert_eq!(err.code, codes::PROGRESS_INACTIVE);
    let err = try_apply(
        &mut s,
        json!([{ "op": "progress.update", "task": 9, "percent_complete": { "text": "50%" } }]),
    )
    .unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref()),
        (codes::TASK_NOT_FOUND, Some("task"))
    );
}

#[test]
fn r216_drops_stop_resume_extras() {
    let mut s = with_tasks(&[("A", "2j")]);
    let mspdi = |key: &str| ExtraEntry {
        origin: "mspdi".to_owned(),
        key: key.to_owned(),
        payload: format!("<{key}>x</{key}>"),
    };
    let plant = edit(move |c| {
        let t = c.project().tasks.get_mut(TaskUid::new(1)).unwrap();
        for key in ["Stop", "Resume", "ResumeValid", "PreLeveledStart", "Hyperlink"] {
            t.extras.0.push(mspdi(key));
        }
        Ok(())
    });
    s.apply_edits(None, &[plant], ApplyMode::Commit, ApplyOptions::default())
        .unwrap();
    let out = apply(
        &mut s,
        json!([{ "op": "progress.update", "task": 1, "percent_complete": { "text": "50%" } }]),
    );
    assert!(
        out.side_effects
            .contains(&plannercraft_automation::SideEffect::ExtrasDropped {
                subject: Subject::Task(TaskUid::new(1)),
                keys: vec!["Resume".to_owned(), "ResumeValid".to_owned(), "Stop".to_owned()],
            })
    );
    let keys: Vec<&str> = get(&s, 1).extras.0.iter().map(|e| e.key.as_str()).collect();
    // Durée inchangée : les autres clés de planning restent.
    assert_eq!(keys, ["PreLeveledStart", "Hyperlink"]);
    // Même saisie : avancement inchangé, rien à jeter.
    let out = apply(
        &mut s,
        json!([{ "op": "progress.update", "task": 1, "percent_complete": { "text": "50%" } }]),
    );
    assert_eq!(out.side_effects.len(), 0);
}

#[test]
fn r216_assignments_follow_progress() {
    // A 2 j, R 100 % (960) ; 50 % ⇒ travail réel 480, reste 480.
    let mut s = with_tasks(&[("A", "2j")]);
    apply(
        &mut s,
        json!([
            { "op": "resource.add", "name": "R" },
            { "op": "assignment.add", "task": 1, "resource": 1 },
            { "op": "progress.update", "task": 1, "percent_complete": { "text": "50%" } }
        ]),
    );
    let a = assignment(&s, 1, 1);
    assert_eq!((a.actual_work, a.remaining_work), (work(480), work(480)));
    assert_eq!(
        get(&s, 1).progress.percent_work_complete,
        Percent::new(5_000).unwrap()
    );

    // Reste saisi 2 j ⇒ D = 480 + 960 = 1 440 ; unités fixes : W 1 440,
    // travail réel 480 (le travail suit la durée, R-189 « durée »).
    apply(
        &mut s,
        json!([{ "op": "progress.update", "task": 1, "remaining_duration": { "text": "2j" } }]),
    );
    assert_eq!(duration(&s, 1), 1_440);
    let a = assignment(&s, 1, 1);
    assert_eq!((a.work, a.actual_work), (work(1_440), work(480)));
    assert_eq!(get(&s, 1).progress.percent_complete, Percent::new(3_333).unwrap());

    // Travail réel au-delà du travail (R-211 : W = AW) sur une tâche à
    // unités fixes : la durée suit le travail (R-189 « travail ») ; tâche
    // achevée, fin réelle déplacée avec elle.
    let mut s = with_tasks(&[("B", "1j")]);
    apply(
        &mut s,
        json!([
            { "op": "resource.add", "name": "R" },
            { "op": "assignment.add", "task": 1, "resource": 1 },
            { "op": "progress.update", "task": 1, "actual_work": { "text": "10h" } }
        ]),
    );
    let a = assignment(&s, 1, 1);
    assert_eq!((a.work, a.actual_work), (work(600), work(600)));
    assert_eq!(duration(&s, 1), 600);
    let b = get(&s, 1);
    assert_eq!(b.progress.actual_duration, Minutes::new(600));
    assert_eq!(b.progress.actual_finish, Some(moment("2026-10-20T10:00")));
    assert_eq!(
        (sched(&s, 1).start, sched(&s, 1).finish),
        (moment("2026-10-19T08:00"), moment("2026-10-20T10:00"))
    );
}

#[test]
fn r216_milestone_rounded_warning() {
    let mut s = with_tasks(&[("J", "0j")]);
    let out = apply(
        &mut s,
        json!([{ "op": "progress.update", "task": 1, "percent_complete": { "text": "40%" } }]),
    );
    assert!(out.warnings.contains(&Diagnostic::ProgressMilestoneRounded {
        task: TaskUid::new(1)
    }));
    assert_eq!(get(&s, 1).progress.percent_complete, Percent::FULL);
}

/// Travail de `m` minutes.
fn work(m: i64) -> Work {
    Work::new(Minutes::new(m)).unwrap()
}

/// Relecture finale J6 (J6-PERF-2) : le planning du clone est recalculé une
/// fois par état du clone (une suite d'opérations qui lisent sans écrire
/// réutilise le dernier recalcul), et le travail de recalcul d'un lot est
/// borné (`SessionLimits::max_batch_schedule_work`) : au-delà,
/// `limit.exceeded`, session intacte.
#[test]
fn r216_batch_schedule_recomputes_cached_and_bounded() {
    use plannercraft_automation::SessionLimits;
    let mut s = chain();
    // Avancement de B mis sous sa forme écrite (restes dérivés) : la même
    // saisie ne le change plus.
    apply(
        &mut s,
        json!([{ "op": "progress.update", "task": 2, "physical_percent_complete": { "value": 0 } }]),
    );
    // Budget d'un seul recalcul (2 tâches).
    s.set_limits(SessionLimits {
        max_batch_schedule_work: 2,
        ..SessionLimits::default()
    });
    // Op 0 écrit ; ops 1 et 2 lisent le même clone (% physique inchangé) :
    // un seul recalcul.
    apply(
        &mut s,
        json!([
            { "op": "task.update", "uid": 1, "name": "A'" },
            { "op": "progress.update", "task": 2, "physical_percent_complete": { "value": 0 } },
            { "op": "progress.update", "task": 2, "physical_percent_complete": { "value": 0 } }
        ]),
    );
    // Deux saisies qui écrivent après une écriture : deux recalculs, refusé.
    let before = s.project().clone();
    let err = try_apply(
        &mut s,
        json!([
            { "op": "task.update", "uid": 1, "name": "A" },
            { "op": "progress.update", "task": 1, "percent_complete": { "text": "50%" } },
            { "op": "progress.update", "task": 2, "percent_complete": { "text": "10%" } }
        ]),
    )
    .unwrap_err();
    assert_eq!(err.code, codes::LIMIT_EXCEEDED);
    assert_eq!(err.op_index, Some(2));
    assert_eq!(s.project(), &before);
}
