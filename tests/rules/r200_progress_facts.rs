//! R-200 — Faits d'avancement (partie faits, contrat J6 lot L0) : achevée ⇔
//! fin réelle ; commencée ⇔ début réel, durée réelle > 0 ou % > 0 ; durée
//! réelle effective = saisie si > 0, sinon `percent_of(D, %)` ; reste =
//! D − réel (≥ 0) ; % dérivé = `round_div(réel × 10 000, D)`. Calendrier
//! Standard, % en 1/10 000.

use plannercraft_core::progress::{facts, percent_of_duration, remaining_duration};
use plannercraft_core::{
    Duration, DurationUnit, Minutes, Moment, Percent, ProgressState, ProjectOptions, Task, TaskUid, TimeBasis,
};

fn task(d: i64) -> Task {
    let mut t = Task::new(TaskUid::new(1), "T".to_owned(), &ProjectOptions::default());
    t.duration = Duration::new(Minutes::new(d), DurationUnit::Day, TimeBasis::Working, false).unwrap();
    t
}

fn at(s: &str) -> Moment {
    s.parse().unwrap()
}

#[test]
fn r200_facts_states() {
    // T 5 j (2 400 min) sans saisie : non commencée, réel 0, reste D, 0 %.
    let mut t = task(2_400);
    let f = facts(&t);
    assert_eq!(f.state, ProgressState::NotStarted);
    assert_eq!(f.actual_duration, Minutes::ZERO);
    assert!(!f.implicit_start);
    assert_eq!(remaining_duration(&t), Minutes::new(2_400));
    assert_eq!(percent_of_duration(&t), Percent::ZERO);

    // Début réel seul (lun 19/10 08:00) : commencée à 0 %, début explicite.
    t.progress.actual_start = Some(at("2026-10-19T08:00"));
    let f = facts(&t);
    assert_eq!(f.state, ProgressState::Started);
    assert_eq!(f.actual_start, Some(at("2026-10-19T08:00")));
    assert!(!f.implicit_start);
    assert_eq!(percent_of_duration(&t), Percent::ZERO);

    // Durée réelle 960 sans début réel : commencée, début implicite ;
    // reste 2 400 − 960 = 1 440 ; % = 960 × 10 000 / 2 400 = 4 000.
    let mut t = task(2_400);
    t.progress.actual_duration = Minutes::new(960);
    let f = facts(&t);
    assert_eq!(f.state, ProgressState::Started);
    assert!(f.implicit_start);
    assert_eq!(f.actual_duration, Minutes::new(960));
    assert_eq!(remaining_duration(&t), Minutes::new(1_440));
    assert_eq!(percent_of_duration(&t), Percent::new(4_000).unwrap());

    // % seul (10 %) : commencée.
    let mut t = task(2_400);
    t.progress.percent_complete = Percent::new(1_000).unwrap();
    assert_eq!(facts(&t).state, ProgressState::Started);

    // Fin réelle saisie : achevée, réel = D (aucune durée réelle saisie),
    // reste 0, 100 %.
    let mut t = task(2_400);
    t.progress.actual_start = Some(at("2026-10-19T08:00"));
    t.progress.actual_finish = Some(at("2026-10-23T17:00"));
    let f = facts(&t);
    assert_eq!(f.state, ProgressState::Complete);
    assert_eq!(f.actual_finish, Some(at("2026-10-23T17:00")));
    assert_eq!(f.actual_duration, Minutes::new(2_400));
    assert_eq!(remaining_duration(&t), Minutes::ZERO);
    assert_eq!(percent_of_duration(&t), Percent::FULL);

    // Jalon (D = 0) achevé : 100 % ; commencé sans fin réelle : 0 %.
    let mut m = task(0);
    m.progress.actual_start = Some(at("2026-10-19T08:00"));
    assert_eq!(percent_of_duration(&m), Percent::ZERO);
    m.progress.actual_finish = Some(at("2026-10-19T08:00"));
    assert_eq!(percent_of_duration(&m), Percent::FULL);
}

#[test]
fn r200_facts_percent_without_actual_duration() {
    // T 3 j (1 440 min), 33,33 % sans durée réelle : réel =
    // round_div(1 440 × 3 333, 10 000) = 479,95 → 480 ; reste 960 ;
    // % dérivé = round_div(480 × 10 000, 1 440) = 3 333,3 → 3 333.
    let mut t = task(1_440);
    t.progress.percent_complete = Percent::new(3_333).unwrap();
    let f = facts(&t);
    assert_eq!(f.state, ProgressState::Started);
    assert!(f.implicit_start);
    assert_eq!(f.actual_duration, Minutes::new(480));
    assert_eq!(remaining_duration(&t), Minutes::new(960));
    assert_eq!(percent_of_duration(&t), Percent::new(3_333).unwrap());

    // 33 % (3 300) : réel = 1 440 × 0,33 = 475,2 → 475 (exemple de R-204).
    t.progress.percent_complete = Percent::new(3_300).unwrap();
    assert_eq!(facts(&t).actual_duration, Minutes::new(475));

    // La durée réelle saisie l'emporte sur le % (vérité = réel, ADR-0014
    // D4) : 960 saisies et 10 % stocké ⇒ réel 960, % dérivé 6 667.
    t.progress.actual_duration = Minutes::new(960);
    t.progress.percent_complete = Percent::new(1_000).unwrap();
    assert_eq!(facts(&t).actual_duration, Minutes::new(960));
    assert_eq!(percent_of_duration(&t), Percent::new(6_667).unwrap());
}

#[test]
fn r200_equation_round_trip() {
    // L2 : toute saisie rend un avancement cohérent ; relu par `facts`, il
    // redonne réel, reste = D − réel et % dérivé. T 5 j (2 400) lun-ven.
    use crate::r201_percent_input::{FRI, MON, calendar, ctx, write};
    use plannercraft_core::progress::edit::{Input, apply};
    let cal = calendar();
    let cx = ctx(&cal, MON, FRI);
    let inputs = [
        Input::Percent(Percent::new(4_000).unwrap()),
        Input::ActualDuration(Minutes::new(1_200)),
        Input::RemainingDuration(Minutes::new(2_400)),
        Input::ActualStart(at("2026-10-20T08:00")),
        Input::ActualFinish(at("2026-10-21T12:00")),
        Input::Percent(Percent::FULL),
    ];
    let mut tk = task(2_400);
    for input in inputs {
        let out = apply(&tk, &[], &[input], &cx).unwrap();
        write(&mut tk, &out);
        let read = facts(&tk);
        let dur = tk.duration.minutes();
        assert_eq!(read.actual_duration, out.progress.actual_duration, "{input:?}");
        assert_eq!(
            out.progress.remaining_duration.get(),
            dur.get() - read.actual_duration.get(),
            "{input:?}"
        );
        assert_eq!(
            remaining_duration(&tk),
            out.progress.remaining_duration,
            "{input:?}"
        );
        assert_eq!(
            percent_of_duration(&tk),
            out.progress.percent_complete,
            "{input:?}"
        );
        assert_eq!(
            read.state == ProgressState::Complete,
            out.progress.percent_complete == Percent::FULL
        );
    }
    // 40 % : réel 960, reste 1 440, % 4 000 (exemple de R-200).
    let out = apply(
        &task(2_400),
        &[],
        &[Input::Percent(Percent::new(4_000).unwrap())],
        &cx,
    )
    .unwrap();
    assert_eq!(out.progress.actual_duration, Minutes::new(960));
    assert_eq!(out.progress.remaining_duration, Minutes::new(1_440));
}
