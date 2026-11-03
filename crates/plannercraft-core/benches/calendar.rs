//! Benchmark informatif des calendriers compilés (ADR-0008, § 9.4).
//!
//! Scénario : calendrier Standard avec 20 jours fériés par an sur 5 ans
//! (2026-2030), compilé, puis 10 000 appels de `add_working` et de
//! `working_between` sur des instants et des durées pseudo-aléatoires. Le
//! générateur est déterministe (congruence linéaire à graine fixe) : aucune
//! dépendance d'aléa.
//!
//! Second scénario (R-172) : `intersect_two_derived`, intersection de deux
//! dérivés de ce Standard (un congé de deux semaines chacun et un jour
//! propre), comme le calendrier effectif d'une affectation (R-171).
//!
//! Cibles informatives : compilation < 50 µs ; conversion < 300 ns en
//! moyenne. Non bloquant et non exécuté en CI (seulement compilé par
//! `cargo clippy --all-targets`) : `cargo bench -p plannercraft-core`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::arithmetic_side_effects,
    reason = "benchmark : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use plannercraft_core::calendar::{
    Calendar, CalendarException, CompiledCalendar, DayHours, DayRule, DaySetting, Recurrence, TimeRange,
    standard,
};
use plannercraft_core::{Arena, CalendarUid, Day, DayRange, Minutes, Moment, TimeOfDay, Weekday};

/// Nombre de conversions par itération.
const CALLS: usize = 10_000;

/// Congruence linéaire (constantes de Knuth, MMIX) : déterministe.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    /// Entier dans `[lo, hi]`.
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        let span = u64::try_from(hi - lo + 1).unwrap();
        lo + i64::try_from(self.next() % span).unwrap()
    }
}

/// Standard avec 20 jours fériés par an, de 2026 à 2030.
fn calendars() -> Arena<CalendarUid, Calendar> {
    let mut c = standard(CalendarUid::new(1), "Standard".to_owned());
    let mut rng = Lcg(2026);
    for year in 2026..=2030 {
        let first = format!("{year}-01-01").parse::<Day>().unwrap().index();
        let mut added = 0;
        while added < 20 {
            let d = Day::new(i32::try_from(rng.range(i64::from(first), i64::from(first) + 364)).unwrap())
                .unwrap();
            let e = CalendarException {
                name: format!("Férié {added}"),
                period: DayRange::single(d),
                setting: DaySetting::NonWorking,
                recurrence: Recurrence::Period,
                occurrences: None,
            };
            if c.add_exception(e).is_ok() {
                added += 1;
            }
        }
    }
    Arena::try_from(vec![c]).unwrap()
}

/// Instants et durées pseudo-aléatoires sur 2026-2030.
fn inputs() -> Vec<(Moment, Minutes, Moment)> {
    let lo = "2026-01-01T00:00".parse::<Moment>().unwrap().minutes();
    let hi = "2030-12-31T00:00".parse::<Moment>().unwrap().minutes();
    let mut rng = Lcg(42);
    (0..CALLS)
        .map(|_| {
            let a = Moment::from_minutes(rng.range(lo, hi)).unwrap();
            let b = Moment::from_minutes(rng.range(lo, hi)).unwrap();
            (a, Minutes::new(rng.range(-50_000, 50_000)), b)
        })
        .collect()
}

/// Les calendriers de [`calendars`] plus deux dérivés (UID 2 et 3) : congé
/// de deux semaines chacun, vendredi chômé pour l'un, samedi ouvré pour
/// l'autre (motif du lundi).
fn two_derived() -> Arena<CalendarUid, Calendar> {
    let base = calendars().get(CalendarUid::new(1)).unwrap().clone();
    let derived = |uid: u32, from: &str, to: &str, wd: Weekday, rule: DayRule| {
        let mut c = Calendar::new_derived(
            CalendarUid::new(uid),
            format!("Ressource {uid}"),
            CalendarUid::new(1),
        );
        c.set_day(wd, rule).unwrap();
        c.add_exception(CalendarException {
            name: "Congé".to_owned(),
            period: DayRange::new(from.parse().unwrap(), to.parse().unwrap()).unwrap(),
            setting: DaySetting::NonWorking,
            recurrence: Recurrence::Period,
            occurrences: None,
        })
        .unwrap();
        c
    };
    let range =
        |a: u16, b: u16| TimeRange::new(TimeOfDay::hm(a, 0).unwrap(), TimeOfDay::hm(b, 0).unwrap()).unwrap();
    let monday = DayRule::Working(DayHours::new(vec![range(8, 12), range(13, 17)]).unwrap());
    Arena::try_from(vec![
        base,
        derived(
            2,
            "2026-08-03",
            "2026-08-14",
            Weekday::Friday,
            DayRule::NonWorking,
        ),
        derived(3, "2027-07-19", "2027-07-30", Weekday::Saturday, monday),
    ])
    .unwrap()
}

fn bench(c: &mut Criterion) {
    let cals = calendars();
    let uid = CalendarUid::new(1);
    c.bench_function("compile Standard + 100 fériés", |b| {
        b.iter(|| CompiledCalendar::compile(black_box(&cals), uid).unwrap());
    });
    let compiled = CompiledCalendar::compile(&cals, uid).unwrap();
    let inputs = inputs();
    c.bench_function("add_working × 10 000", |b| {
        b.iter(|| {
            for (t, d, _) in &inputs {
                black_box(compiled.add_working(black_box(*t), black_box(*d)).ok());
            }
        });
    });
    c.bench_function("working_between × 10 000", |b| {
        b.iter(|| {
            for (a, _, bb) in &inputs {
                black_box(compiled.working_between(black_box(*a), black_box(*bb)).ok());
            }
        });
    });
    let derived = two_derived();
    let a = CompiledCalendar::compile(&derived, CalendarUid::new(2)).unwrap();
    let b = CompiledCalendar::compile(&derived, CalendarUid::new(3)).unwrap();
    c.bench_function("intersect_two_derived", |bch| {
        bch.iter(|| black_box(&a).intersect(black_box(&b)).unwrap());
    });
    c.bench_function("union_two_derived", |bch| {
        bch.iter(|| black_box(&a).union(black_box(&b)).unwrap());
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
