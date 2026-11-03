//! R-213 — Cas limites de la saisie d'avancement : récapitulative et tâche
//! inactive refusées ; jalon (D = 0) à un % strictement entre 0 et 100 ⇒
//! 100 %, fin réelle = début réel, `MilestoneRounded`.

use crate::r201_percent_input::{MON, at, calendar, ctx, pct, task};
use plannercraft_core::progress::edit::{EditError, EditNote, Input, apply};
use plannercraft_core::{Minutes, Percent};

#[test]
fn r213_summary_refused() {
    let cal = calendar();
    let mut c = ctx(&cal, MON, "2026-10-23T17:00");
    c.summary = true;
    let e = apply(&task(2_400), &[], &[Input::Percent(pct(5_000))], &c);
    assert_eq!(e, Err(EditError::SummaryNotEditable));
    assert_eq!(
        EditError::SummaryNotEditable.code(),
        "progress.summary_not_editable"
    );
    c.summary = false;
    let mut t = task(2_400);
    t.active = false;
    assert_eq!(
        apply(&t, &[], &[Input::Percent(pct(5_000))], &c),
        Err(EditError::Inactive)
    );
    assert_eq!(EditError::Inactive.code(), "progress.inactive");
}

#[test]
fn r213_milestone_rounded() {
    // Jalon lun 08:00 à 30 % : 100 %, début et fin réels lun 08:00, D 0.
    let cal = calendar();
    let c = ctx(&cal, MON, MON);
    let e = apply(&task(0), &[], &[Input::Percent(pct(3_000))], &c).unwrap();
    assert_eq!(e.notes, [EditNote::MilestoneRounded]);
    assert_eq!(e.progress.actual_start, Some(at(MON)));
    assert_eq!(e.progress.actual_finish, Some(at(MON)));
    assert_eq!(e.progress.percent_complete, Percent::FULL);
    assert_eq!(e.duration, Minutes::ZERO);
    // 100 % : même résultat, sans remarque.
    let e = apply(&task(0), &[], &[Input::Percent(Percent::FULL)], &c).unwrap();
    assert_eq!(e.notes, []);
    assert_eq!(e.progress.actual_finish, Some(at(MON)));
}

/// R-213 (arbitrage du 2026-10-20) : `task.update duration` sous la durée
/// réelle stockée est refusée (`progress.duration_below_actual`, champ
/// `duration`, message FR/EN), projet intact ; égale à la durée réelle :
/// acceptée.
#[test]
fn r213_duration_below_actual_refused() {
    use plannercraft_automation::error::codes;
    use plannercraft_automation::text::Language;
    use plannercraft_tests::resource_edit::{duration, with_tasks};
    use plannercraft_tests::task_edit::{apply as apply_ops, try_apply};
    use serde_json::json;

    let mut s = with_tasks(&[("A", "3j")]);
    apply_ops(
        &mut s,
        json!([{ "op": "progress.update", "task": 1, "actual_duration": { "text": "2j" } }]),
    );
    let before = s.project().clone();
    let err = try_apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 1, "duration": { "text": "1j" } }]),
    )
    .unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref(), err.subject),
        (
            codes::PROGRESS_DURATION_BELOW_ACTUAL,
            Some("duration"),
            Some(plannercraft_core::Subject::Task(plannercraft_core::TaskUid::new(
                1
            )))
        )
    );
    assert_eq!(s.project(), &before);
    assert!(
        err.message(Language::Fr)
            .contains("inférieure à la durée réelle déjà saisie (960 min)"),
        "{}",
        err.message(Language::Fr)
    );
    assert!(
        err.message(Language::En)
            .contains("less than the actual duration already entered (960 min)")
    );
    apply_ops(
        &mut s,
        json!([{ "op": "task.update", "uid": 1, "duration": { "text": "2j" } }]),
    );
    assert_eq!(duration(&s, 1), 960);
}
