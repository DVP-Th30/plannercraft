//! P39 : équilibrage d'une saisie d'avancement (`progress::edit::apply`,
//! R-200 à R-202, R-211 à R-213), 1 024 suites de saisies générées sur le
//! calendrier Standard. Après chaque saisie acceptée :
//! - 0 ≤ réel ≤ D, reste = D − réel, % = % dérivé relu par `facts` ;
//! - 100 % ⇔ fin réelle, fin réelle ≥ début réel ;
//! - monter le % d'une tâche commencée ne déplace jamais son début réel ;
//! - travail réel ≤ travail sur chaque affectation, sommes écrites ;
//! - `apply` idempotente : la même saisie sur le résultat écrit le redonne.

#![allow(
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use plannercraft_core::calendar::standard;
use plannercraft_core::progress::edit::{Ctx, Edited, Input, Part, apply};
use plannercraft_core::progress::{facts, percent_of_duration, remaining_duration};
use plannercraft_core::{
    Arena, AssignmentUid, CalendarUid, CompiledCalendar, Duration, DurationUnit, Minutes, Moment, Percent,
    ProgressOptions, ProgressState, ProjectOptions, Span, Task, TaskUid, TimeBasis, TimeScale, Work,
};
use proptest::prelude::*;

const MON: &str = "2026-10-19T08:00";

fn at(s: &str) -> Moment {
    s.parse().unwrap()
}

fn percent() -> impl Strategy<Value = i64> {
    prop_oneof![1 => Just(0_i64), 1 => Just(10_000_i64), 4 => 1_i64..10_000]
}

fn minutes() -> impl Strategy<Value = i64> {
    prop_oneof![1 => Just(0_i64), 4 => 1_i64..=30_000]
}

/// Instant à `m` minutes civiles de lun 19/10 08:00 (souvent chômé).
fn instant() -> impl Strategy<Value = i64> {
    0_i64..=20_000
}

fn input() -> impl Strategy<Value = Input> {
    let base = at(MON);
    let moment = move |m: i64| base.checked_add(Minutes::new(m)).unwrap();
    prop_oneof![
        3 => percent().prop_map(|p| Input::Percent(Percent::new(p).unwrap())),
        1 => instant().prop_map(move |m| Input::ActualStart(moment(m))),
        1 => instant().prop_map(move |m| Input::ActualFinish(moment(m))),
        1 => minutes().prop_map(|m| Input::ActualDuration(Minutes::new(m))),
        1 => minutes().prop_map(|m| Input::RemainingDuration(Minutes::new(m))),
        1 => percent().prop_map(|p| Input::PercentWork(Percent::new(p).unwrap())),
        1 => (0_i64..=60_000).prop_map(|w| Input::ActualWork(Work::new(Minutes::new(w)).unwrap())),
        1 => percent().prop_map(|p| Input::Physical(Percent::new(p).unwrap())),
    ]
}

fn write(t: &mut Task, e: &Edited) {
    t.progress = e.progress;
    t.duration = Duration::new(e.duration, t.duration.unit(), t.duration.basis(), false).unwrap();
}

/// Vérifie P39 sur une saisie acceptée.
fn check(before: &Task, inputs: &[Input], out: &Edited, parts_before: &[Part]) {
    let pg = &out.progress;
    let dur = out.duration;
    assert!(
        dur >= Minutes::ZERO && dur.get() <= Duration::MAX_MINUTES,
        "{dur:?}"
    );
    assert!(
        pg.actual_duration >= Minutes::ZERO && pg.actual_duration <= dur,
        "{pg:?} {dur:?}"
    );
    assert_eq!(
        pg.remaining_duration.get(),
        dur.get() - pg.actual_duration.get(),
        "{pg:?}"
    );
    assert_eq!(
        pg.percent_complete == Percent::FULL,
        pg.actual_finish.is_some(),
        "{pg:?}"
    );
    if let Some(fin) = pg.actual_finish {
        let s = pg.actual_start.unwrap();
        assert!(fin >= s, "{pg:?}");
    }
    let started = pg.actual_start.is_some() || pg.actual_duration > Minutes::ZERO;
    assert_eq!(started, pg.actual_start.is_some(), "{pg:?}");
    // Monter le % d'une tâche commencée ne déplace pas son début réel.
    let raised = inputs
        .iter()
        .any(|i| matches!(i, Input::Percent(v) if *v > Percent::ZERO));
    let start_entered = inputs.iter().any(|i| matches!(i, Input::ActualStart(_)));
    if raised && !start_entered && before.progress.actual_start.is_some() {
        assert_eq!(pg.actual_start, before.progress.actual_start, "{inputs:?}");
    }
    // Relu par `facts`, le résultat redonne réel, reste et % (R-200).
    let mut after = before.clone();
    write(&mut after, out);
    let f = facts(&after);
    assert_eq!(f.actual_duration, pg.actual_duration, "{pg:?}");
    assert_eq!(f.state == ProgressState::Complete, pg.actual_finish.is_some());
    assert_eq!(remaining_duration(&after), pg.remaining_duration);
    assert_eq!(percent_of_duration(&after), pg.percent_complete, "{pg:?}");
    // Travail : AW ≤ W, sommes écrites, UID triés et inchangés.
    let mut uids: Vec<AssignmentUid> = parts_before.iter().map(|q| q.uid).collect();
    uids.sort_unstable();
    assert_eq!(out.parts.iter().map(|q| q.uid).collect::<Vec<_>>(), uids);
    if !out.parts.is_empty() {
        let w: i64 = out.parts.iter().map(|q| q.work.minutes().get()).sum();
        let aw: i64 = out.parts.iter().map(|q| q.actual_work.minutes().get()).sum();
        assert!(
            out.parts.iter().all(|q| q.actual_work <= q.work),
            "{:?}",
            out.parts
        );
        assert_eq!(pg.actual_work.minutes().get(), aw);
        assert_eq!(pg.remaining_work.minutes().get(), w - aw);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    #[test]
    fn p39_progress_edit(
        d in prop_oneof![1 => Just(0_i64), 6 => 1_i64..=20_000],
        elapsed in prop::bool::weighted(0.15),
        works in prop::collection::vec(prop_oneof![1 => Just(0_i64), 4 => 1_i64..=20_000], 0..=3),
        steps in prop::collection::vec(prop::collection::vec(input(), 1..=3), 1..=6),
    ) {
        let u = CalendarUid::new(1);
        let cals = Arena::try_from(vec![standard(u, "S".to_owned())]).unwrap();
        let cal = CompiledCalendar::compile(&cals, u).unwrap();
        let basis = if elapsed { TimeBasis::Elapsed } else { TimeBasis::Working };
        let scale = if elapsed { TimeScale::Elapsed } else { TimeScale::Working(&cal) };
        let scheduled: Span = scale.forward_span(at(MON), Minutes::new(d)).unwrap();
        let ctx = Ctx {
            scheduled,
            measure: &cal,
            status_date: at(MON),
            options: ProgressOptions::default(),
            summary: false,
        };
        let mut task = Task::new(TaskUid::new(1), "T".to_owned(), &ProjectOptions::default());
        task.duration = Duration::new(Minutes::new(d), DurationUnit::Day, basis, false).unwrap();
        let mut parts: Vec<Part> = works
            .iter()
            .enumerate()
            .map(|(i, w)| Part {
                uid: AssignmentUid::new(u32::try_from(works.len() - i).unwrap()),
                work: Work::new(Minutes::new(*w)).unwrap(),
                actual_work: Work::ZERO,
            })
            .collect();
        for inputs in &steps {
            let Ok(e) = apply(&task, &parts, inputs, &ctx) else { continue };
            check(&task, inputs, &e, &parts);
            let mut after = task.clone();
            write(&mut after, &e);
            // Idempotence : la même saisie sur le résultat écrit le redonne.
            let again = apply(&after, &e.parts, inputs, &ctx);
            prop_assert_eq!(again.as_ref(), Ok(&e), "{:?}", inputs);
            task = after;
            parts.clone_from(&e.parts);
        }
    }
}
