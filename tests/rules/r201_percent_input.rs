//! R-201 — Saisie du % achevé (`progress::edit::apply`). Calendrier Standard
//! (lun-ven 08:00-12:00, 13:00-17:00), T 5 j (2 400 min) planifiée lun
//! 19/10/2026 08:00 – ven 23/10 17:00, % en 1/10 000. Les aides de ce
//! fichier servent aussi aux tests R-202 et R-211 à R-213.

use plannercraft_core::calendar::standard;
use plannercraft_core::progress::edit::{Ctx, Edited, Input, Part, apply};
use plannercraft_core::{
    Arena, AssignmentUid, CalendarUid, CompiledCalendar, Duration, DurationUnit, Minutes, Moment, Percent,
    ProgressOptions, ProjectOptions, Span, Task, TaskUid, TimeBasis, Work,
};

pub(crate) fn at(s: &str) -> Moment {
    s.parse().unwrap()
}

pub(crate) fn pct(v: i64) -> Percent {
    Percent::new(v).unwrap()
}

pub(crate) fn work(m: i64) -> Work {
    Work::new(Minutes::new(m)).unwrap()
}

/// Calendrier Standard compilé.
pub(crate) fn calendar() -> CompiledCalendar {
    let u = CalendarUid::new(1);
    let cals = Arena::try_from(vec![standard(u, "S".to_owned())]).unwrap();
    CompiledCalendar::compile(&cals, u).unwrap()
}

/// Contexte : dates planifiées `start` – `finish`, options par défaut.
pub(crate) fn ctx<'a>(cal: &'a CompiledCalendar, start: &str, finish: &str) -> Ctx<'a> {
    Ctx {
        scheduled: Span {
            start: at(start),
            finish: at(finish),
        },
        measure: cal,
        status_date: at(start),
        options: ProgressOptions::default(),
        summary: false,
    }
}

/// Feuille de `d` minutes ouvrées.
pub(crate) fn task(d: i64) -> Task {
    let mut t = Task::new(TaskUid::new(1), "T".to_owned(), &ProjectOptions::default());
    t.duration = Duration::new(Minutes::new(d), DurationUnit::Day, TimeBasis::Working, false).unwrap();
    t
}

/// Affectation de travail `w`, sans travail réel.
pub(crate) fn part(uid: u32, w: i64) -> Part {
    Part {
        uid: AssignmentUid::new(uid),
        work: work(w),
        actual_work: Work::ZERO,
    }
}

/// Écrit le résultat sur la tâche (comme `progress.update`).
pub(crate) fn write(t: &mut Task, e: &Edited) {
    t.progress = e.progress;
    t.duration = Duration::new(e.duration, t.duration.unit(), t.duration.basis(), false).unwrap();
}

/// T 5 j planifiée lun 08:00 – ven 17:00.
pub(crate) const MON: &str = "2026-10-19T08:00";
pub(crate) const FRI: &str = "2026-10-23T17:00";

#[test]
fn r201_percent_sets_actual_start() {
    // 40 % de 2 400 = 960 ; début réel = début planifié lun 08:00 ; reste
    // 2 400 − 960 = 1 440 ; pas de fin réelle.
    let cal = calendar();
    let c = ctx(&cal, MON, FRI);
    let e = apply(&task(2_400), &[], &[Input::Percent(pct(4_000))], &c).unwrap();
    assert_eq!(e.progress.actual_start, Some(at(MON)));
    assert_eq!(e.progress.actual_duration, Minutes::new(960));
    assert_eq!(e.progress.remaining_duration, Minutes::new(1_440));
    assert_eq!(e.progress.percent_complete, pct(4_000));
    assert_eq!(e.progress.actual_finish, None);
    assert_eq!(e.duration, Minutes::new(2_400));

    // Début réel déjà saisi (mar 08:00) : gardé, le % ne le déplace pas.
    let mut t = task(2_400);
    t.progress.actual_start = Some(at("2026-10-20T08:00"));
    let e = apply(&t, &[], &[Input::Percent(pct(4_000))], &c).unwrap();
    assert_eq!(e.progress.actual_start, Some(at("2026-10-20T08:00")));
}

#[test]
fn r201_hundred_sets_actual_finish() {
    // 40 % puis 100 % : fin réelle = fin planifiée ven 23/10 17:00, réel =
    // D = 2 400, reste 0, début réel inchangé.
    let cal = calendar();
    let c = ctx(&cal, MON, FRI);
    let mut t = task(2_400);
    let e = apply(&t, &[], &[Input::Percent(pct(4_000))], &c).unwrap();
    write(&mut t, &e);
    let e = apply(&t, &[], &[Input::Percent(Percent::FULL)], &c).unwrap();
    assert_eq!(e.progress.actual_start, Some(at(MON)));
    assert_eq!(e.progress.actual_finish, Some(at(FRI)));
    assert_eq!(e.progress.actual_duration, Minutes::new(2_400));
    assert_eq!(e.progress.remaining_duration, Minutes::ZERO);
    assert_eq!(e.progress.percent_complete, Percent::FULL);
    assert_eq!(e.duration, Minutes::new(2_400));
}

#[test]
fn r201_zero_clears() {
    // 40 % avec deux affectations puis 0 % : dates réelles effacées, réel 0,
    // travail réel 0 sur chaque affectation.
    let cal = calendar();
    let c = ctx(&cal, MON, FRI);
    let mut t = task(2_400);
    let parts = [part(1, 2_400), part(2, 1_200)];
    let e = apply(&t, &parts, &[Input::Percent(pct(4_000))], &c).unwrap();
    // AW = 40 % de W : 960 et 480.
    assert_eq!(e.parts[0].actual_work, work(960));
    assert_eq!(e.parts[1].actual_work, work(480));
    write(&mut t, &e);
    let e = apply(&t, &e.parts, &[Input::Percent(Percent::ZERO)], &c).unwrap();
    assert_eq!(e.progress.actual_start, None);
    assert_eq!(e.progress.actual_finish, None);
    assert_eq!(e.progress.actual_duration, Minutes::ZERO);
    assert_eq!(e.progress.remaining_duration, Minutes::new(2_400));
    assert_eq!(e.progress.percent_complete, Percent::ZERO);
    assert!(e.parts.iter().all(|p| p.actual_work == Work::ZERO));
    assert_eq!(e.progress.actual_work, Work::ZERO);
    assert_eq!(e.progress.remaining_work, work(3_600));
}
