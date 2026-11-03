//! Propriété P35 des calendriers combinés (R-172, ADR-0013 décision 2).
//!
//! Générateurs : deux scénarios indépendants, chacun un calendrier de base
//! aléatoire (0 à 5 plages par jour, au pas de 1 ou 15 minutes, parfois sans
//! temps ouvré), 0 à 8 exceptions sur 2026-2027 (récurrentes ou non,
//! chevauchements compris), 0 à 2 semaines datées, et un dérivé dans 40 %
//! des cas ; instants en 2026-2027 et près des bornes du domaine.
//!
//! - P35a : `a ∩ b` et `a ∪ b` contre un oracle fondé sur `resolve_day`
//!   (API froide), jour par jour sur ±400 jours : plages du jour (croisées
//!   ici par un balayage des bornes, sans fonction de `plannercraft-core`), W
//!   cumulé, `is_working` ; hors fenêtre, inclusion-exclusion sur tout le
//!   domaine : |a ∩ b| + |a ∪ b| = |a| + |b|.
//! - P35b : commutativité (aux UID près) et `a ∩ a = a`, `a ∪ a = a`.
//! - P35c : ajouter un jour chômé à b ne raccourcit jamais une fin (passe
//!   avant) ni ne retarde un début (passe arrière) calculés sur `a ∩ b`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use plannercraft_core::calendar::{
    Calendar, CalendarException, CompiledCalendar, DayHours, DayRule, DaySetting, Recurrence, TimeRange,
    WorkWeek, resolve_day,
};
use plannercraft_core::{
    Arena, CalendarError, CalendarUid, Day, DayRange, Minutes, Moment, TimeOfDay, Week, Weekday,
};
use proptest::prelude::*;

const DAY_COUNT: i64 = 60_631;
const MAX: i64 = 87_308_640;
/// Demi-largeur de la fenêtre de l'oracle, en jours.
const WINDOW: i64 = 400;

// ---------------------------------------------------------------------------
// Générateurs
// ---------------------------------------------------------------------------

fn day_index(s: &str) -> i32 {
    s.parse::<Day>().unwrap().index()
}

fn minutes_of(s: &str) -> i64 {
    s.parse::<Moment>().unwrap().minutes()
}

/// Réglage d'un jour : 0 à 5 plages, au pas de 1 ou de 15 minutes.
fn day_setting() -> impl Strategy<Value = DaySetting> {
    (
        0_usize..=5,
        any::<bool>(),
        prop::collection::vec(0_u16..=1_440, 10),
    )
        .prop_map(|(k, coarse, mut points)| {
            if coarse {
                for p in &mut points {
                    *p = *p / 15 * 15;
                }
            }
            let mut points: Vec<u16> = points.into_iter().take(2 * k).collect();
            points.sort_unstable();
            let ranges: Vec<TimeRange> = points
                .chunks(2)
                .filter(|c| c.len() == 2 && c[0] < c[1])
                .map(|c| {
                    TimeRange::new(TimeOfDay::new(c[0]).unwrap(), TimeOfDay::new(c[1]).unwrap()).unwrap()
                })
                .collect();
            if ranges.is_empty() {
                DaySetting::NonWorking
            } else {
                DaySetting::Working(DayHours::new(ranges).unwrap())
            }
        })
}

fn week_of<T: Clone + std::fmt::Debug>(s: impl Strategy<Value = T>) -> impl Strategy<Value = Week<T>> {
    prop::collection::vec(s, 7).prop_map(|v| Week {
        sunday: v[0].clone(),
        monday: v[1].clone(),
        tuesday: v[2].clone(),
        wednesday: v[3].clone(),
        thursday: v[4].clone(),
        friday: v[5].clone(),
        saturday: v[6].clone(),
    })
}

fn day_rule() -> impl Strategy<Value = DayRule> {
    prop_oneof![
        2 => Just(DayRule::Inherit),
        3 => day_setting().prop_map(DayRule::from),
    ]
}

/// Semaine de base : parfois entièrement chômée.
fn base_week() -> impl Strategy<Value = Week<DaySetting>> {
    prop_oneof![
        9 => week_of(day_setting()),
        1 => Just(Week::from_fn(|_| DaySetting::NonWorking)),
    ]
}

/// Intervalle de jours en 2026-2027, de 1 à `max_len` jours.
fn period(max_len: i32) -> impl Strategy<Value = DayRange> {
    let lo = day_index("2026-01-01");
    let hi = day_index("2027-12-31");
    (lo..=hi, 0..max_len).prop_map(|(first, len)| {
        DayRange::new(Day::new(first).unwrap(), Day::new(first + len).unwrap()).unwrap()
    })
}

fn recurrence() -> impl Strategy<Value = Recurrence> {
    prop_oneof![
        6 => Just(Recurrence::Period),
        2 => (1_u16..=5).prop_map(|every| Recurrence::Daily { every }),
        2 => (1_u16..=3, week_of(any::<bool>())).prop_map(|(every, days)| Recurrence::Weekly { every, days }),
    ]
}

fn exception() -> impl Strategy<Value = CalendarException> {
    (period(60), day_setting(), recurrence()).prop_map(|(period, setting, recurrence)| CalendarException {
        name: String::new(),
        period,
        setting,
        recurrence,
        occurrences: None,
    })
}

fn work_week() -> impl Strategy<Value = WorkWeek> {
    (period(90), week_of(day_rule())).prop_map(|(period, days)| WorkWeek {
        name: String::new(),
        period,
        days,
    })
}

/// Calendrier avec exceptions et semaines datées posées par la lecture JSON,
/// qui accepte les chevauchements (fichier importé).
fn with_extras(c: &Calendar, exceptions: &[CalendarException], work_weeks: &[WorkWeek]) -> Calendar {
    let mut json = serde_json::to_value(c).unwrap();
    json["exceptions"] = serde_json::to_value(exceptions).unwrap();
    json["work_weeks"] = serde_json::to_value(work_weeks).unwrap();
    serde_json::from_value(json).unwrap()
}

/// Parties d'un calendrier généré (base ou dérivé).
#[derive(Clone, Debug)]
struct Level<W> {
    week: W,
    exceptions: Vec<CalendarException>,
    work_weeks: Vec<WorkWeek>,
}

/// Scénario : une base (UID 1), éventuellement un dérivé (UID 2) ; le
/// calendrier étudié est le dérivé s'il existe.
#[derive(Clone, Debug)]
struct Scenario {
    base: Level<Week<DaySetting>>,
    derived: Option<Level<Week<DayRule>>>,
}

fn scenario() -> impl Strategy<Value = Scenario> {
    let base = (
        base_week(),
        prop::collection::vec(exception(), 0..=8),
        prop::collection::vec(work_week(), 0..=2),
    )
        .prop_map(|(week, exceptions, work_weeks)| Level {
            week,
            exceptions,
            work_weeks,
        });
    let derived = prop_oneof![
        3 => Just(None),
        2 => (
            week_of(day_rule()),
            prop::collection::vec(exception(), 0..=4),
            prop::collection::vec(work_week(), 0..=2),
        )
            .prop_map(|(week, exceptions, work_weeks)| Some(Level { week, exceptions, work_weeks })),
    ];
    (base, derived).prop_map(|(base, derived)| Scenario { base, derived })
}

impl Scenario {
    const fn uid(&self) -> CalendarUid {
        CalendarUid::new(if self.derived.is_some() { 2 } else { 1 })
    }

    /// Calendriers, avec en tête des exceptions du calendrier étudié un jour
    /// chômé `holiday` s'il est fourni.
    fn calendars(&self, holiday: Option<Day>) -> Arena<CalendarUid, Calendar> {
        let extra = holiday.map(|d| CalendarException {
            name: String::new(),
            period: DayRange::single(d),
            setting: DaySetting::NonWorking,
            recurrence: Recurrence::Period,
            occurrences: None,
        });
        let with = |list: &[CalendarException], studied: bool| -> Vec<CalendarException> {
            let mut out: Vec<CalendarException> = if studied {
                extra.clone().into_iter().collect()
            } else {
                Vec::new()
            };
            out.extend(list.iter().cloned());
            out
        };
        let base = Calendar::new_base(CalendarUid::new(1), "Base".to_owned(), self.base.week.clone());
        let mut list = vec![with_extras(
            &base,
            &with(&self.base.exceptions, self.derived.is_none()),
            &self.base.work_weeks,
        )];
        if let Some(d) = &self.derived {
            let mut c = Calendar::new_derived(CalendarUid::new(2), "Dérivé".to_owned(), CalendarUid::new(1));
            for wd in Weekday::ALL {
                c.set_day(wd, d.week.get(wd).clone()).unwrap();
            }
            list.push(with_extras(&c, &with(&d.exceptions, true), &d.work_weeks));
        }
        Arena::try_from(list).unwrap()
    }

    fn compile(&self, holiday: Option<Day>) -> CompiledCalendar {
        CompiledCalendar::compile(&self.calendars(holiday), self.uid()).unwrap()
    }

    /// Plages fusionnées du jour `day` par la résolution froide.
    fn resolved(&self, calendars: &Arena<CalendarUid, Calendar>, day: Day) -> Vec<(i64, i64)> {
        merged(&resolve_day(calendars, self.uid(), day).unwrap().hours)
    }
}

/// Instant : surtout en 2026-2027, parfois près des bornes du domaine.
fn moment() -> impl Strategy<Value = Moment> {
    let lo = minutes_of("2026-01-01T00:00");
    let hi = minutes_of("2028-01-01T00:00");
    let span = 3 * 365 * 1_440;
    prop_oneof![
        8 => lo..=hi,
        1 => 0..=span,
        1 => (MAX - span)..=MAX,
    ]
    .prop_map(|m| Moment::from_minutes(m).unwrap())
}

fn at(m: i64) -> Moment {
    Moment::from_minutes(m).unwrap()
}

// ---------------------------------------------------------------------------
// Oracle
// ---------------------------------------------------------------------------

fn merged(setting: &DaySetting) -> Vec<(i64, i64)> {
    let mut out: Vec<(i64, i64)> = Vec::new();
    if let DaySetting::Working(h) = setting {
        for r in h.ranges() {
            let (a, b) = (i64::from(r.from().minutes()), i64::from(r.to().minutes()));
            match out.last_mut() {
                Some((_, end)) if *end >= a => *end = (*end).max(b),
                _ => out.push((a, b)),
            }
        }
    }
    out
}

fn contains(ranges: &[(i64, i64)], m: i64) -> bool {
    ranges.iter().any(|&(a, b)| a <= m && m < b)
}

/// Plages d'un jour croisées par balayage : entre deux bornes consécutives
/// (de l'une ou l'autre liste), l'appartenance est constante ; elle est
/// testée sur la première minute.
fn crossed(a: &[(i64, i64)], b: &[(i64, i64)], union: bool) -> Vec<(i64, i64)> {
    let mut cuts: Vec<i64> = a.iter().chain(b).flat_map(|&(x, y)| [x, y]).collect();
    cuts.extend([0, 1_440]);
    cuts.sort_unstable();
    cuts.dedup();
    let mut out: Vec<(i64, i64)> = Vec::new();
    for w in cuts.windows(2) {
        let (p, q) = (w[0], w[1]);
        let (ia, ib) = (contains(a, p), contains(b, p));
        if if union { ia || ib } else { ia && ib } {
            match out.last_mut() {
                Some((_, end)) if *end == p => *end = q,
                _ => out.push((p, q)),
            }
        }
    }
    out
}

fn ranges_of(c: &CompiledCalendar, day: Day) -> Vec<(i64, i64)> {
    c.day_ranges(day)
        .unwrap()
        .iter()
        .map(|r| (i64::from(r.from().minutes()), i64::from(r.to().minutes())))
        .collect()
}

fn w(c: &CompiledCalendar, a: Moment, b: Moment) -> i64 {
    c.working_between(a, b).unwrap().get()
}

/// Fin (d > 0) ou début (d < 0) placé, réduit à ce que P35c compare.
fn placed(r: Result<Moment, CalendarError>) -> Option<i64> {
    r.ok().map(Moment::minutes)
}

// ---------------------------------------------------------------------------
// Propriétés
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    /// P35a : intersection et union contre l'oracle `resolve_day` croisé.
    #[test]
    fn p35_matches_crossed_resolve_day(sa in scenario(), sb in scenario(), t in moment()) {
        let (a, b) = (sa.compile(None), sb.compile(None));
        let i = a.intersect(&b).unwrap();
        let u = a.union(&b).unwrap();
        prop_assert_eq!(i.uid(), sa.uid());
        prop_assert_eq!(u.uid(), sa.uid());
        // Inclusion-exclusion sur tout le domaine.
        prop_assert_eq!(
            i.total_working().get() + u.total_working().get(),
            a.total_working().get() + b.total_working().get()
        );
        prop_assert!(i.total_working() <= a.total_working().min(b.total_working()));
        prop_assert!(u.total_working() >= a.total_working().max(b.total_working()));
        // Jour par jour sur la fenêtre, W cumulé depuis son premier jour.
        let center = t.minutes() / 1_440;
        let first = (center - WINDOW).max(0);
        let end = (center + WINDOW + 1).min(DAY_COUNT);
        let origin = at(first * 1_440);
        let (ca, cb) = (sa.calendars(None), sb.calendars(None));
        let (mut wi, mut wu) = (0_i64, 0_i64);
        for d in first..end {
            let day = Day::new(i32::try_from(d).unwrap()).unwrap();
            let (ra, rb) = (sa.resolved(&ca, day), sb.resolved(&cb, day));
            let ei = crossed(&ra, &rb, false);
            let eu = crossed(&ra, &rb, true);
            prop_assert_eq!(&ranges_of(&i, day), &ei, "intersection, jour {}", day);
            prop_assert_eq!(&ranges_of(&u, day), &eu, "union, jour {}", day);
            prop_assert_eq!(w(&i, origin, at(d * 1_440)), wi, "W ∩ au début du jour {}", day);
            prop_assert_eq!(w(&u, origin, at(d * 1_440)), wu, "W ∪ au début du jour {}", day);
            wi += ei.iter().map(|(x, y)| y - x).sum::<i64>();
            wu += eu.iter().map(|(x, y)| y - x).sum::<i64>();
        }
        // Minute t : ouvrée dans ∩ ⇔ dans a et b ; dans ∪ ⇔ dans a ou b.
        let (wa, wb) = (a.is_working(t).unwrap(), b.is_working(t).unwrap());
        prop_assert_eq!(i.is_working(t).unwrap(), wa && wb);
        prop_assert_eq!(u.is_working(t).unwrap(), wa || wb);
    }

    /// P35b : commutativité aux UID près, idempotence.
    #[test]
    fn p35_commutative_and_idempotent(sa in scenario(), sb in scenario(), t in moment()) {
        let (a, b) = (sa.compile(None), sb.compile(None));
        let (ab, ba) = (a.intersect(&b).unwrap(), b.intersect(&a).unwrap());
        let (uab, uba) = (a.union(&b).unwrap(), b.union(&a).unwrap());
        prop_assert_eq!(ab.total_working(), ba.total_working());
        prop_assert_eq!(uab.total_working(), uba.total_working());
        let (aa, ua) = (a.intersect(&a).unwrap(), a.union(&a).unwrap());
        prop_assert_eq!(aa.total_working(), a.total_working());
        prop_assert_eq!(ua.total_working(), a.total_working());
        let center = i32::try_from(t.minutes() / 1_440).unwrap();
        for d in (center - 30).max(0)..(center + 30).min(60_631) {
            let day = Day::new(d).unwrap();
            prop_assert_eq!(ab.day_ranges(day).unwrap(), ba.day_ranges(day).unwrap());
            prop_assert_eq!(uab.day_ranges(day).unwrap(), uba.day_ranges(day).unwrap());
            prop_assert_eq!(aa.day_ranges(day).unwrap(), a.day_ranges(day).unwrap());
            prop_assert_eq!(ua.day_ranges(day).unwrap(), a.day_ranges(day).unwrap());
        }
        prop_assert_eq!(w(&ab, at(0), t), w(&ba, at(0), t));
        prop_assert_eq!(w(&aa, at(0), t), w(&a, at(0), t));
    }

    /// P35c : un jour chômé ajouté à b ne raccourcit jamais une fin ni ne
    /// retarde un début calculés sur `a ∩ b`.
    #[test]
    fn p35_extra_holiday_never_shortens(
        sa in scenario(),
        sb in scenario(),
        t in moment(),
        offset in -60_i64..=60,
        d in 1_i64..=50_000,
    ) {
        let holiday = Day::new(i32::try_from((t.minutes() / 1_440 + offset).clamp(0, DAY_COUNT - 1)).unwrap()).unwrap();
        let a = sa.compile(None);
        let (b, b2) = (sb.compile(None), sb.compile(Some(holiday)));
        // Le jour ajouté peut être déjà réclamé par une exception plus
        // ancienne : seul le cas où il est bien chômé compte.
        prop_assume!(b2.day_ranges(holiday).unwrap().is_empty());
        let (i, i2) = (a.intersect(&b).unwrap(), a.intersect(&b2).unwrap());
        prop_assert!(i2.total_working() <= i.total_working());
        if let Some(f) = placed(i.add_working(t, Minutes::new(d))) {
            if let Some(f2) = placed(i2.add_working(t, Minutes::new(d))) {
                prop_assert!(f2 >= f, "fin raccourcie : {} < {}", f2, f);
            }
        } else {
            prop_assert!(placed(i2.add_working(t, Minutes::new(d))).is_none());
        }
        if let Some(s) = placed(i.add_working(t, Minutes::new(-d))) {
            if let Some(s2) = placed(i2.add_working(t, Minutes::new(-d))) {
                prop_assert!(s2 <= s, "début retardé : {} > {}", s2, s);
            }
        } else {
            prop_assert!(placed(i2.add_working(t, Minutes::new(-d))).is_none());
        }
    }
}
