//! R-172 — Intersection et union compilées : `a.intersect(b)` est ouvré à
//! l'instant t si et seulement si a et b le sont ; `a.union(b)` si a ou b
//! l'est. Exceptions et semaines datées déjà dépliées dans la forme
//! compilée ; résultat déterministe, UID de `a` ; `a ∩ a = a`.
//!
//! Repère : S = Standard (lun-ven 08:00-12:00, 13:00-17:00, UID 1) ;
//! L = base lun-sam 09:00-18:00 (UID 2).
//!
//! Budget de compilation (amendement J6, lot L9, règle 10) :
//! `Limits::compiled_segments` borne les segments des calendriers du modèle
//! compilés par recalcul (`CalendarSet::compile_with`) ; le calendrier du
//! projet et sa base passent d'abord ; au-delà, `calendar.too_many_segments`
//! et repli R-066.

use plannercraft_core::calendar::{
    Calendar, CalendarException, CalendarSet, CompiledCalendar, DaySetting, Recurrence, TimeRange,
};
use plannercraft_core::{CalendarError, CalendarUid, Day, DayRange, Diagnostic, Limits, Week, Weekday};
use plannercraft_tests::net::Net;
use plannercraft_tests::{calendars, compiled, day, exception, mins, moment, standard, time, working};

/// Base lun-sam 09:00-18:00, UID 2.
fn mon_sat() -> Calendar {
    Calendar::new_base(
        CalendarUid::new(2),
        "Lun-sam".to_owned(),
        Week::from_fn(|wd| match wd {
            Weekday::Sunday => DaySetting::NonWorking,
            _ => working(&[("09:00", "18:00")]),
        }),
    )
}

/// S et L compilés, S avec un férié le mer 21/10/2026 si `holiday`.
fn pair(holiday: bool) -> (CompiledCalendar, CompiledCalendar) {
    let mut s = standard();
    if holiday {
        s.add_exception(exception("2026-10-21", "2026-10-21", DaySetting::NonWorking))
            .unwrap();
    }
    let a = calendars(vec![s, mon_sat()]);
    (compiled(&a, 1), compiled(&a, 2))
}

/// Plages du jour `d`, en `(début, fin)` texte.
fn ranges(c: &CompiledCalendar, d: &str) -> Vec<(String, String)> {
    c.day_ranges(day(d))
        .unwrap()
        .iter()
        .map(|r: &TimeRange| (r.from().to_string(), r.to().to_string()))
        .collect()
}

fn r(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(a, b)| (time(a).to_string(), time(b).to_string()))
        .collect()
}

#[test]
fn r172_intersection_weekly() {
    let (s, l) = pair(false);
    let i = s.intersect(&l).unwrap();
    assert_eq!(i.uid(), CalendarUid::new(1));
    // Lun 19/10 : [08-12, 13-17] ∩ [09-18] = [09-12, 13-17].
    assert_eq!(
        ranges(&i, "2026-10-19"),
        r(&[("09:00", "12:00"), ("13:00", "17:00")])
    );
    // Sam 24/10 : S chômé ; dim 25/10 : les deux chômés.
    assert_eq!(ranges(&i, "2026-10-24"), r(&[]));
    assert_eq!(ranges(&i, "2026-10-25"), r(&[]));
    // Semaine du 19/10 : 5 × (180 + 240) = 2 100 minutes.
    assert_eq!(
        i.working_between(moment("2026-10-19T00:00"), moment("2026-10-26T00:00")),
        Ok(mins(2_100))
    );
    // 08:30 : ouvré dans S seulement ; 09:30 : dans les deux.
    assert_eq!(i.is_working(moment("2026-10-19T08:30")), Ok(false));
    assert_eq!(i.is_working(moment("2026-10-19T09:30")), Ok(true));
    // Lun 08:00 + 420 min : 09:00-12:00 (180) + 13:00-17:00 (240) → 17:00.
    assert_eq!(
        i.add_working(moment("2026-10-19T08:00"), mins(420)),
        Ok(moment("2026-10-19T17:00"))
    );
}

#[test]
fn r172_union_weekly() {
    let (s, l) = pair(false);
    let u = s.union(&l).unwrap();
    assert_eq!(u.uid(), CalendarUid::new(1));
    // Lun : [08-12, 13-17] ∪ [09-18] = [08-18] ; sam : [09-18] ; dim chômé.
    assert_eq!(ranges(&u, "2026-10-19"), r(&[("08:00", "18:00")]));
    assert_eq!(ranges(&u, "2026-10-24"), r(&[("09:00", "18:00")]));
    assert_eq!(ranges(&u, "2026-10-25"), r(&[]));
    // Semaine : 5 × 600 + 540 = 3 540 minutes.
    assert_eq!(
        u.working_between(moment("2026-10-19T00:00"), moment("2026-10-26T00:00")),
        Ok(mins(3_540))
    );
    // 12:30 : pause de S, ouvré dans L, donc dans l'union.
    assert_eq!(u.is_working(moment("2026-10-19T12:30")), Ok(true));
    // Ven 23/10 17:00 + 120 min : ven 17:00-18:00 (60), sam 09:00-10:00 (60).
    assert_eq!(
        u.add_working(moment("2026-10-23T17:00"), mins(120)),
        Ok(moment("2026-10-24T10:00"))
    );
}

#[test]
fn r172_exception_crossed() {
    let (s, l) = pair(true);
    let i = s.intersect(&l).unwrap();
    let u = s.union(&l).unwrap();
    // Mer 21/10, férié de S : l'intersection est chômée, l'union vaut L.
    assert_eq!(ranges(&i, "2026-10-21"), r(&[]));
    assert_eq!(ranges(&u, "2026-10-21"), r(&[("09:00", "18:00")]));
    // Les jours voisins gardent la règle de la semaine.
    assert_eq!(
        ranges(&i, "2026-10-20"),
        r(&[("09:00", "12:00"), ("13:00", "17:00")])
    );
    assert_eq!(ranges(&u, "2026-10-22"), r(&[("08:00", "18:00")]));
    // Semaine du 19/10 : 4 × 420 = 1 680 ; union 4 × 600 + 540 + 540 = 3 480.
    let (week_start, week_end) = (moment("2026-10-19T00:00"), moment("2026-10-26T00:00"));
    assert_eq!(i.working_between(week_start, week_end), Ok(mins(1_680)));
    assert_eq!(u.working_between(week_start, week_end), Ok(mins(3_480)));
    // Mar 20/10 16:00 + 120 min d'intersection : mar 16:00-17:00 (60), le
    // mercredi saute, jeu 09:00-10:00 (60).
    assert_eq!(
        i.add_working(moment("2026-10-20T16:00"), mins(120)),
        Ok(moment("2026-10-22T10:00"))
    );
    // Commutativité : mêmes plages, UID de l'appelant.
    let i2 = l.intersect(&s).unwrap();
    assert_eq!(i2.uid(), CalendarUid::new(2));
    assert_eq!(i2.total_working(), i.total_working());
    for d in ["2026-10-19", "2026-10-21", "2026-10-24"] {
        assert_eq!(ranges(&i2, d), ranges(&i, d));
        assert_eq!(ranges(&l.union(&s).unwrap(), d), ranges(&u, d));
    }
}

#[test]
fn r172_idempotent() {
    let (s, l) = pair(true);
    for c in [&s, &l] {
        // a ∩ a = a et a ∪ a = a, à la structure près.
        assert_eq!(&c.intersect(c).unwrap(), c);
        assert_eq!(&c.union(c).unwrap(), c);
    }
    // (S ∩ L) ∩ L = S ∩ L ; (S ∪ L) ∪ S = S ∪ L (mêmes plages et total).
    let i = s.intersect(&l).unwrap();
    let ii = i.intersect(&l).unwrap();
    let u = s.union(&l).unwrap();
    let uu = u.union(&s).unwrap();
    assert_eq!(ii.total_working(), i.total_working());
    assert_eq!(uu.total_working(), u.total_working());
    for d in ["2026-10-19", "2026-10-21", "2026-10-24", "2026-10-25"] {
        assert_eq!(ranges(&ii, d), ranges(&i, d));
        assert_eq!(ranges(&uu, d), ranges(&u, d));
    }
}

/// Limites par défaut, budget de compilation `segments`.
fn compile_budget(segments: usize) -> Limits {
    Limits {
        compiled_segments: segments,
        ..Limits::default()
    }
}

/// `calendar.too_many_segments` d'un calendrier du modèle hors budget.
fn over(uid: u32, limit: u32) -> Diagnostic {
    Diagnostic::CalendarTooManySegments {
        calendar: CalendarUid::new(uid),
        other: CalendarUid::new(uid),
        limit,
    }
}

#[test]
fn r172_compile_budget() {
    // Calendriers intégrés : Standard (UID 1, celui du projet) et 24 heures
    // (UID 2), un segment chacun. A (1 j) sur 24 heures.
    let net = || Net::new("2026-10-19T08:00").task("A", 480).calendar("A", 2);
    // Budget par défaut : 24 heures, lun 08:00 – 16:00.
    let run = net().run();
    assert_eq!(run.at("A").early.finish, moment("2026-10-19T16:00"));
    assert!(!run.codes().contains(&"calendar.too_many_segments"));
    // Budget d'un segment : Standard (projet) compilé, 24 heures refusé ;
    // A se replie sur le calendrier du projet (R-066) : lun 08:00 – 17:00.
    let run = net().run_with(compile_budget(1));
    assert_eq!(run.at("A").early.finish, moment("2026-10-19T17:00"));
    let diags = &run.schedule.diagnostics;
    assert!(diags.contains(&over(2, 1)), "{diags:?}");
    assert!(over(2, 1).to_string().contains("budget de 1 segments"));
    assert!(diags.contains(&Diagnostic::TaskCalendarFallback {
        task: run.uid("A"),
        calendar: CalendarUid::new(2),
    }));
    assert!(!run.codes().contains(&"project.calendar_fallback"));
    // Budget nul : même le calendrier du projet est refusé, Standard
    // intégré (R-066, erreur), jamais de panique.
    let run = net().run_with(compile_budget(0));
    assert_eq!(run.at("A").early.finish, moment("2026-10-19T17:00"));
    assert!(run.codes().contains(&"project.calendar_fallback"));
    assert!(run.schedule.diagnostics.contains(&over(1, 0)));
}

#[test]
fn r172_compile_budget_project_calendar_first() {
    // Calendrier du projet = 24 heures (UID 2), rangé après Standard : il
    // est compilé d'abord ; Standard, refusé, ne sert à personne. B sans
    // calendrier propre : lun 08:00 – 16:00.
    let run = Net::new("2026-10-19T08:00")
        .project(|p| p.calendar = CalendarUid::new(2))
        .task("B", 480)
        .run_with(compile_budget(1));
    assert_eq!(run.at("B").early.finish, moment("2026-10-19T16:00"));
    assert!(run.schedule.diagnostics.contains(&over(1, 1)));
    assert!(!run.codes().contains(&"project.calendar_fallback"));
}

#[test]
fn r172_compile_budget_bounds_work() {
    // Un jour chômé sur deux sur tout le domaine : environ 30 000 séries
    // dépliées et 60 000 segments. Budget de 1 000 segments (8 000 de
    // travail) : abandon pendant le dépliage, avant de poser les segments.
    let mut p = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .calendar("A", 7)
        .build();
    let mut c = Calendar::new_derived(CalendarUid::new(7), "Alterné".to_owned(), CalendarUid::new(1));
    c.add_exception(CalendarException {
        name: "Un jour sur deux".to_owned(),
        period: DayRange::new(Day::new(0).unwrap(), Day::new(60_630).unwrap()).unwrap(),
        setting: DaySetting::NonWorking,
        recurrence: Recurrence::Daily { every: 2 },
        occurrences: None,
    })
    .unwrap();
    p.calendars.push(c).unwrap();
    let (set, diags) = CalendarSet::compile_with(&p, 1_000);
    assert_eq!(
        set.get(CalendarUid::new(7)).unwrap_err(),
        CalendarError::TooManySegments { limit: 1_000 }
    );
    assert!(diags.finish().contains(&over(7, 1_000)));
    // Les autres calendriers restent compilés.
    assert!(set.get(CalendarUid::new(1)).unwrap().has_working_time());
    // Budget par défaut : compilé en entier (plus de 60 000 segments).
    let (set, _) = CalendarSet::compile(&p);
    assert!(set.get(CalendarUid::new(7)).unwrap().segment_count() > 60_000);
}
