//! R-202 — Saisie du début réel, de la durée réelle, du reste et de la fin
//! réelle ; ordre d'application et conflits. Calendrier Standard, T 5 j
//! (2 400 min) planifiée lun 19/10/2026 08:00 – ven 23/10 17:00.

use crate::r201_percent_input::{FRI, MON, at, calendar, ctx, pct, task, write};
use plannercraft_core::progress::edit::{EditError, Input, apply};
use plannercraft_core::{Minutes, Percent};

#[test]
fn r202_actual_start_only() {
    // Début réel mar 20/10 08:00 seul : commencée à 0 %, réel 0, reste D.
    let cal = calendar();
    let c = ctx(&cal, MON, FRI);
    let e = apply(
        &task(2_400),
        &[],
        &[Input::ActualStart(at("2026-10-20T08:00"))],
        &c,
    )
    .unwrap();
    assert_eq!(e.progress.actual_start, Some(at("2026-10-20T08:00")));
    assert_eq!(e.progress.actual_finish, None);
    assert_eq!(e.progress.actual_duration, Minutes::ZERO);
    assert_eq!(e.progress.remaining_duration, Minutes::new(2_400));
    assert_eq!(e.progress.percent_complete, Percent::ZERO);
    assert_eq!(e.duration, Minutes::new(2_400));
}

#[test]
fn r202_actual_duration_beyond_duration() {
    // Durée réelle 1 200 < D : % = 1 200 × 10 000 / 2 400 = 5 000, début
    // réel implicite lun 08:00.
    let cal = calendar();
    let c = ctx(&cal, MON, FRI);
    let e = apply(
        &task(2_400),
        &[],
        &[Input::ActualDuration(Minutes::new(1_200))],
        &c,
    )
    .unwrap();
    assert_eq!(e.progress.actual_start, Some(at(MON)));
    assert_eq!(e.progress.percent_complete, pct(5_000));
    assert_eq!(e.progress.actual_finish, None);

    // Durée réelle 2 880 ≥ D : D = 2 880 (6 j), achevée ; fin réelle =
    // forward_span(lun 08:00, 2 880) = lun 26/10 17:00.
    let e = apply(
        &task(2_400),
        &[],
        &[Input::ActualDuration(Minutes::new(2_880))],
        &c,
    )
    .unwrap();
    assert_eq!(e.duration, Minutes::new(2_880));
    assert_eq!(e.progress.actual_duration, Minutes::new(2_880));
    assert_eq!(e.progress.actual_finish, Some(at("2026-10-26T17:00")));
    assert_eq!(e.progress.remaining_duration, Minutes::ZERO);
    assert_eq!(e.progress.percent_complete, Percent::FULL);
}

#[test]
fn r202_remaining_changes_duration() {
    // T 5 j, réel 1 200 puis reste 2 400 : D = 1 200 + 2 400 = 3 600, réel
    // inchangé, % = 1 200 × 10 000 / 3 600 = 3 333,3 → 3 333 (S50).
    let cal = calendar();
    let c = ctx(&cal, MON, FRI);
    let mut t = task(2_400);
    let e = apply(&t, &[], &[Input::ActualDuration(Minutes::new(1_200))], &c).unwrap();
    write(&mut t, &e);
    let e = apply(&t, &[], &[Input::RemainingDuration(Minutes::new(2_400))], &c).unwrap();
    assert_eq!(e.duration, Minutes::new(3_600));
    assert_eq!(e.progress.actual_duration, Minutes::new(1_200));
    assert_eq!(e.progress.remaining_duration, Minutes::new(2_400));
    assert_eq!(e.progress.percent_complete, pct(3_333));

    // Reste 0 : D = réel = 1 200, achevée ; fin réelle = fin de la partie
    // réelle, forward_span(lun 08:00, 1 200) = mer 21/10 12:00.
    write(&mut t, &e);
    let e = apply(&t, &[], &[Input::RemainingDuration(Minutes::ZERO)], &c).unwrap();
    assert_eq!(e.duration, Minutes::new(1_200));
    assert_eq!(e.progress.actual_finish, Some(at("2026-10-21T12:00")));
    assert_eq!(e.progress.percent_complete, Percent::FULL);
}

#[test]
fn r202_actual_finish_measures_duration() {
    // Début réel lun 08:00, fin réelle mer 21/10 12:00 : D = réel = temps
    // ouvré entre les deux = 480 + 480 + 240 = 1 200 ; 100 %.
    let cal = calendar();
    let c = ctx(&cal, MON, FRI);
    let inputs = [
        Input::ActualStart(at(MON)),
        Input::ActualFinish(at("2026-10-21T12:00")),
    ];
    let e = apply(&task(2_400), &[], &inputs, &c).unwrap();
    assert_eq!(e.duration, Minutes::new(1_200));
    assert_eq!(e.progress.actual_duration, Minutes::new(1_200));
    assert_eq!(e.progress.remaining_duration, Minutes::ZERO);
    assert_eq!(e.progress.percent_complete, Percent::FULL);

    // Fin réelle seule : début réel implicite = début planifié (lun 08:00).
    let e = apply(
        &task(2_400),
        &[],
        &[Input::ActualFinish(at("2026-10-21T12:00"))],
        &c,
    )
    .unwrap();
    assert_eq!(e.progress.actual_start, Some(at(MON)));
    assert_eq!(e.duration, Minutes::new(1_200));

    // L'ordre de la saisie ne compte pas : fin réelle avant début réel dans
    // la liste, même résultat (ordre de R-202).
    let reversed = [inputs[1], inputs[0]];
    assert_eq!(
        apply(&task(2_400), &[], &reversed, &c),
        apply(&task(2_400), &[], &inputs, &c)
    );
}

#[test]
fn r202_invalid_order() {
    // Fin réelle lun 19/10 12:00 avant le début réel mar 20/10 08:00.
    let cal = calendar();
    let c = ctx(&cal, MON, FRI);
    let inputs = [
        Input::ActualStart(at("2026-10-20T08:00")),
        Input::ActualFinish(at("2026-10-19T12:00")),
    ];
    assert_eq!(
        apply(&task(2_400), &[], &inputs, &c),
        Err(EditError::InvalidOrder)
    );
    assert_eq!(EditError::InvalidOrder.code(), "progress.invalid_order");
}

#[test]
fn r202_conflicts() {
    let cal = calendar();
    let c = ctx(&cal, MON, FRI);
    let t = task(2_400);
    let fin = Input::ActualFinish(at("2026-10-21T12:00"));
    let cases: [(&[Input], EditError); 6] = [
        (
            &[
                Input::Percent(pct(5_000)),
                Input::ActualDuration(Minutes::new(600)),
            ],
            EditError::Conflict("percent_complete", "actual_duration"),
        ),
        (
            &[Input::Percent(pct(5_000)), fin],
            EditError::Conflict("percent_complete", "actual_finish"),
        ),
        (
            &[fin, Input::RemainingDuration(Minutes::new(60))],
            EditError::Conflict("actual_finish", "remaining_duration"),
        ),
        (
            &[
                Input::PercentWork(pct(5_000)),
                Input::ActualWork(crate::r201_percent_input::work(60)),
            ],
            EditError::Conflict("percent_work_complete", "actual_work"),
        ),
        // Même champ saisi deux fois avec deux valeurs.
        (
            &[Input::Percent(pct(1_000)), Input::Percent(pct(2_000))],
            EditError::Conflict("percent_complete", "percent_complete"),
        ),
        // Durée réelle contredite par la fin réelle (1 200 mesurées).
        (
            &[Input::ActualDuration(Minutes::new(600)), fin],
            EditError::Conflict("actual_duration", "actual_finish"),
        ),
    ];
    for (inputs, err) in cases {
        assert_eq!(apply(&t, &[], inputs, &c), Err(err), "{inputs:?}");
    }
    // Compatibles : 100 % avec la fin réelle, reste 0 avec la fin réelle,
    // même saisie répétée.
    assert!(apply(&t, &[], &[Input::Percent(Percent::FULL), fin], &c).is_ok());
    assert!(apply(&t, &[], &[fin, Input::RemainingDuration(Minutes::ZERO)], &c).is_ok());
    assert!(
        apply(
            &t,
            &[],
            &[Input::Percent(pct(1_000)), Input::Percent(pct(1_000))],
            &c
        )
        .is_ok()
    );
}
