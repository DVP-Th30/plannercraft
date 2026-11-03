//! Propriétés des calendriers compilés (ADR-0008, § 9.3, P1 à P9).
//!
//! Générateurs : calendriers de base aléatoires (0 à 5 plages par jour, au pas
//! de 1 ou 15 minutes, jours chômés compris, plages jusqu'à 24:00), 0 à 20
//! exceptions sur 2026-2027 (récurrentes ou non, chevauchements compris),
//! 0 à 3 semaines datées, 0 ou 1 calendrier dérivé, parfois un calendrier
//! sans temps ouvré ; instants en 2026-2027 et près des bornes (1984, 2149) ;
//! d dans [−50 000, 50 000].
//!
//! - P1 : `add_working` et `working_between` sont inverses.
//! - P2 : minimalité (d > 0) et maximalité (d < 0) du résultat.
//! - P3 : `next_working_moment` et `previous_working_moment` : idempotence,
//!   sens, aucun temps ouvré sauté, minute ouvrée.
//! - P4 : monotonie de `add_working`, `next`, `previous` et de W.
//! - P5 : dualités `add(add(t, d), −d) == next(t)` et
//!   `add(add(t, −d), d) == previous(t)`.
//! - P6 : Chasles, antisymétrie, `0 <= W(a, b) <= b − a`.
//! - P7 : calendrier 24 heures : `add_working(t, d) == t + d`.
//! - P8 : test différentiel contre un oracle naïf fondé sur `resolve_day`
//!   (API froide), jour par jour sur une fenêtre de ±400 jours ; au-delà, le
//!   cas n'est pas comparé. `resolve_day` partage avec la compilation le
//!   dépliage des récurrences, l'ordre des exceptions et le choix de la
//!   semaine datée : chaque jour de la fenêtre est donc aussi comparé à un
//!   second oracle, écrit ici sans aucune fonction de `plannercraft-core`
//!   (récurrences dépliées jour par jour avec chrono, précédence R-016 à
//!   R-019 réécrite).
//! - P9 : un calendrier dérivé compilé est identique au même calendrier écrit
//!   en entier comme calendrier de base.
//! - P10 : temps écoulé : `forward_span(Elapsed, t, d)` finit à t + d et
//!   `between` vaut b − a ; en temps ouvré, un placement contient exactement
//!   d minutes ouvrées, commence au plus tôt à `from` (passe avant) et finit
//!   au plus tard à `to` (passe arrière).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use std::collections::BTreeMap;

use plannercraft_core::calendar::{
    Calendar, CalendarException, CompiledCalendar, DayHours, DayRule, DaySetting, MonthItem, MonthPosition,
    Recurrence, TimeRange, TimeScale, WorkWeek, expand_exception, resolve_day, twenty_four_hours,
};
use plannercraft_core::{
    Arena, CalendarError, CalendarUid, Day, DayRange, Direction, Minutes, Moment, Month, TimeOfDay, Week,
    Weekday,
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

fn day_rule() -> impl Strategy<Value = DayRule> {
    prop_oneof![
        2 => Just(DayRule::Inherit),
        3 => day_setting().prop_map(DayRule::from),
    ]
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

/// Semaine de base : parfois entièrement chômée.
fn base_week() -> impl Strategy<Value = Week<DaySetting>> {
    prop_oneof![
        9 => week_of(day_setting()),
        1 => Just(Week::from_fn(|_| DaySetting::NonWorking)),
    ]
}

fn weekday() -> impl Strategy<Value = Weekday> {
    prop::sample::select(Weekday::ALL.to_vec())
}

fn month_item() -> impl Strategy<Value = MonthItem> {
    prop_oneof![
        Just(MonthItem::Day),
        Just(MonthItem::Weekday),
        Just(MonthItem::WeekendDay),
        weekday().prop_map(MonthItem::On),
    ]
}

fn position() -> impl Strategy<Value = MonthPosition> {
    prop::sample::select(vec![
        MonthPosition::First,
        MonthPosition::Second,
        MonthPosition::Third,
        MonthPosition::Fourth,
        MonthPosition::Last,
    ])
}

fn recurrence() -> impl Strategy<Value = Recurrence> {
    let month = prop::sample::select(Month::ALL.to_vec());
    prop_oneof![
        6 => Just(Recurrence::Period),
        2 => (1_u16..=5).prop_map(|every| Recurrence::Daily { every }),
        2 => (1_u16..=3, week_of(any::<bool>())).prop_map(|(every, days)| Recurrence::Weekly { every, days }),
        1 => (1_u16..=3, 1_u8..=31).prop_map(|(every, day)| Recurrence::MonthlyByDay { every, day }),
        1 => (1_u16..=3, position(), month_item())
            .prop_map(|(every, position, item)| Recurrence::MonthlyByPosition { every, position, item }),
        1 => (month.clone(), 1_u8..=31).prop_map(|(month, day)| Recurrence::YearlyByDay { month, day }),
        1 => (month, position(), month_item())
            .prop_map(|(month, position, item)| Recurrence::YearlyByPosition { month, position, item }),
        1 => (1_u16..=3).prop_map(|every| Recurrence::ByDayCount { every }),
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

fn exception() -> impl Strategy<Value = CalendarException> {
    (
        period(60),
        day_setting(),
        recurrence(),
        prop_oneof![3 => Just(None), 1 => (0_u32..=10).prop_map(Some)],
    )
        .prop_map(|(period, setting, recurrence, occurrences)| CalendarException {
            name: String::new(),
            period,
            setting,
            recurrence,
            occurrences,
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

/// Scénario : un calendrier de base (UID 1), éventuellement un dérivé (UID 2),
/// et le calendrier étudié.
#[derive(Clone, Debug)]
struct Scenario {
    calendars: Arena<CalendarUid, Calendar>,
    uid: CalendarUid,
}

fn scenario() -> impl Strategy<Value = Scenario> {
    let base = (
        base_week(),
        prop::collection::vec(exception(), 0..=20),
        prop::collection::vec(work_week(), 0..=3),
    );
    // Un dérivé dans 40 % des cas (poids entiers : aucun flottant).
    let derived = prop_oneof![
        3 => Just(None),
        2 => (
            week_of(day_rule()),
            prop::collection::vec(exception(), 0..=10),
            prop::collection::vec(work_week(), 0..=3),
        )
            .prop_map(Some),
    ];
    (base, derived).prop_map(|((week, exceptions, work_weeks), derived)| {
        let base = Calendar::new_base(CalendarUid::new(1), "Base".to_owned(), week);
        let mut list = vec![with_extras(&base, &exceptions, &work_weeks)];
        let mut uid = CalendarUid::new(1);
        if let Some((rules, exceptions, work_weeks)) = derived {
            let mut d = Calendar::new_derived(CalendarUid::new(2), "Dérivé".to_owned(), CalendarUid::new(1));
            for wd in Weekday::ALL {
                d.set_day(wd, rules.get(wd).clone()).unwrap();
            }
            list.push(with_extras(&d, &exceptions, &work_weeks));
            uid = CalendarUid::new(2);
        }
        Scenario {
            calendars: Arena::try_from(list).unwrap(),
            uid,
        }
    })
}

impl Scenario {
    fn compile(&self) -> CompiledCalendar {
        CompiledCalendar::compile(&self.calendars, self.uid).unwrap()
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

fn delta() -> impl Strategy<Value = i64> {
    prop_oneof![-50_000_i64..=50_000, -3_000_i64..=3_000]
}

fn at(m: i64) -> Moment {
    Moment::from_minutes(m).unwrap()
}

fn w(c: &CompiledCalendar, a: Moment, b: Moment) -> i64 {
    c.working_between(a, b).unwrap().get()
}

// ---------------------------------------------------------------------------
// Oracle naïf (P8)
// ---------------------------------------------------------------------------

/// Résultat de l'oracle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Naive {
    At(i64),
    After,
    Before,
    /// Au-delà de la fenêtre : non comparé.
    Unknown,
}

/// Plages fusionnées de chaque jour d'une fenêtre, par `resolve_day`.
struct Oracle {
    first_day: i64,
    /// Jours `first_day..first_day + days.len()`.
    days: Vec<Vec<(i64, i64)>>,
}

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

impl Oracle {
    fn new(s: &Scenario, center: Moment) -> Self {
        let c = center.minutes() / 1_440;
        let first_day = (c - WINDOW).max(0);
        let end_day = (c + WINDOW + 1).min(DAY_COUNT);
        let days = (first_day..end_day)
            .map(|d| {
                let day = Day::new(i32::try_from(d).unwrap()).unwrap();
                merged(&resolve_day(&s.calendars, s.uid, day).unwrap().hours)
            })
            .collect();
        Self { first_day, days }
    }

    fn end_day(&self) -> i64 {
        self.first_day + i64::try_from(self.days.len()).unwrap()
    }

    fn contains(&self, t: i64) -> bool {
        t >= self.first_day * 1_440 && t <= self.end_day() * 1_440
    }

    /// Plages absolues (minutes) du jour `d`, `None` hors fenêtre.
    fn ranges(&self, d: i64) -> Option<Vec<(i64, i64)>> {
        let i = usize::try_from(d - self.first_day).ok()?;
        self.days
            .get(i)
            .map(|r| r.iter().map(|(a, b)| (d * 1_440 + a, d * 1_440 + b)).collect())
    }

    /// Minutes ouvrées dans [a, b), a <= b, dans la fenêtre.
    fn between(&self, a: i64, b: i64) -> i64 {
        let mut total = 0;
        let mut d = a / 1_440;
        while d * 1_440 < b {
            if let Some(ranges) = self.ranges(d) {
                for (x, y) in ranges {
                    total += (y.min(b) - x.max(a)).max(0);
                }
            }
            d += 1;
        }
        total
    }

    /// Plages absolues à partir du jour de `t`, dans l'ordre.
    fn forward(&self, t: i64) -> (Vec<(i64, i64)>, bool) {
        let mut out = Vec::new();
        let mut d = (t / 1_440).min(DAY_COUNT - 1);
        while let Some(ranges) = self.ranges(d) {
            out.extend(ranges);
            d += 1;
        }
        // Vrai si la fenêtre va jusqu'à la fin du domaine.
        (out, self.end_day() == DAY_COUNT)
    }

    fn backward(&self, t: i64) -> (Vec<(i64, i64)>, bool) {
        let mut out = Vec::new();
        let mut d = (t / 1_440).min(DAY_COUNT - 1);
        while let Some(ranges) = self.ranges(d) {
            out.extend(ranges.into_iter().rev());
            d -= 1;
        }
        (out, self.first_day == 0)
    }

    fn add(&self, t: i64, d: i64) -> Naive {
        if d == 0 {
            return Naive::At(t);
        }
        if d > 0 {
            let (ranges, to_end) = self.forward(t);
            let mut left = d;
            for (a, b) in ranges {
                let from = a.max(t);
                if from >= b {
                    continue;
                }
                if b - from >= left {
                    return Naive::At(from + left);
                }
                left -= b - from;
            }
            if to_end { Naive::After } else { Naive::Unknown }
        } else {
            let (ranges, to_start) = self.backward(t);
            let mut left = -d;
            for (a, b) in ranges {
                let to = b.min(t);
                if to <= a {
                    continue;
                }
                if to - a >= left {
                    return Naive::At(to - left);
                }
                left -= to - a;
            }
            if to_start { Naive::Before } else { Naive::Unknown }
        }
    }

    fn next(&self, t: i64) -> Naive {
        let (ranges, to_end) = self.forward(t);
        for (a, b) in ranges {
            if b > t {
                return Naive::At(a.max(t));
            }
        }
        if to_end { Naive::After } else { Naive::Unknown }
    }

    fn previous(&self, t: i64) -> Naive {
        let (ranges, to_start) = self.backward(t);
        for (a, b) in ranges {
            if a < t {
                return Naive::At(b.min(t));
            }
        }
        if to_start { Naive::Before } else { Naive::Unknown }
    }
}

// ---------------------------------------------------------------------------
// Oracle indépendant de la résolution d'un jour (P8)
// ---------------------------------------------------------------------------

/// Résolution d'un jour écrite d'après R-016 à R-019, sans `resolve_day`,
/// `expand_exception` ni aucun tri de `plannercraft-core`.
mod independent {
    use chrono::{Datelike, NaiveDate};
    use plannercraft_core::calendar::{
        Calendar, CalendarException, CalendarKind, DayRule, DaySetting, MonthItem, MonthPosition, Recurrence,
        WorkWeek,
    };
    use plannercraft_core::{Arena, CalendarUid, Day, Weekday};

    fn date(d: Day) -> NaiveDate {
        NaiveDate::from_ymd_opt(1984, 1, 1).unwrap() + chrono::Days::new(u64::try_from(d.index()).unwrap())
    }

    fn weekday(d: NaiveDate) -> Weekday {
        Weekday::ALL[usize::try_from(d.weekday().num_days_from_sunday()).unwrap()]
    }

    fn item_matches(d: NaiveDate, item: MonthItem) -> bool {
        let wd = weekday(d);
        let weekend = matches!(wd, Weekday::Saturday | Weekday::Sunday);
        match item {
            MonthItem::Day => true,
            MonthItem::Weekday => !weekend,
            MonthItem::WeekendDay => weekend,
            MonthItem::On(w) => wd == w,
        }
    }

    /// `d` est le n-ième (ou le dernier) jour `item` de son mois.
    fn is_nth(d: NaiveDate, position: MonthPosition, item: MonthItem) -> bool {
        let candidates: Vec<NaiveDate> = (1..=31)
            .filter_map(|k| NaiveDate::from_ymd_opt(d.year(), d.month(), k))
            .filter(|x| item_matches(*x, item))
            .collect();
        let chosen = match position {
            MonthPosition::First => candidates.first(),
            MonthPosition::Second => candidates.get(1),
            MonthPosition::Third => candidates.get(2),
            MonthPosition::Fourth => candidates.get(3),
            MonthPosition::Last => candidates.last(),
        };
        chosen == Some(&d)
    }

    fn months_between(first: NaiveDate, d: NaiveDate) -> i64 {
        (i64::from(d.year()) * 12 + i64::from(d.month0()))
            - (i64::from(first.year()) * 12 + i64::from(first.month0()))
    }

    fn sunday_of(d: NaiveDate) -> NaiveDate {
        d - chrono::Days::new(u64::from(d.weekday().num_days_from_sunday()))
    }

    /// Le jour `d` de la période répond-il à la récurrence (avant la limite
    /// d'occurrences) ?
    fn matches(r: Recurrence, first: NaiveDate, d: NaiveDate) -> bool {
        match r {
            Recurrence::Period => true,
            Recurrence::Daily { every } => (d - first).num_days() % i64::from(every) == 0,
            Recurrence::Weekly { every, days } => {
                let weeks = (sunday_of(d) - sunday_of(first)).num_days() / 7;
                weeks % i64::from(every) == 0 && *days.get(weekday(d))
            }
            Recurrence::MonthlyByDay { every, day } => {
                months_between(first, d) % i64::from(every) == 0 && d.day() == u32::from(day)
            }
            Recurrence::MonthlyByPosition {
                every,
                position,
                item,
            } => months_between(first, d) % i64::from(every) == 0 && is_nth(d, position, item),
            Recurrence::YearlyByDay { month, day } => {
                d.month() == month.number() && d.day() == u32::from(day)
            }
            Recurrence::YearlyByPosition {
                month,
                position,
                item,
            } => d.month() == month.number() && is_nth(d, position, item),
            Recurrence::ByDayCount { .. } | Recurrence::ByWeekdayCount { .. } => false,
        }
    }

    /// Jours (indices) d'une exception, dépliés un à un dans la période.
    fn expand(e: &CalendarException) -> Vec<i32> {
        let first = date(e.period.first());
        let mut out: Vec<i32> = (e.period.first().index()..=e.period.last().index())
            .filter(|&k| matches(e.recurrence, first, date(Day::new(k).unwrap())))
            .collect();
        if let Some(n) = e.occurrences {
            out.truncate(usize::try_from(n).unwrap());
        }
        out
    }

    /// Indices d'une liste triés par (premier jour, indice), à la main.
    fn order_by_start(starts: &[i32]) -> Vec<usize> {
        let mut idx: Vec<usize> = (0..starts.len()).collect();
        // Tri par insertion, stable : à début égal, l'ordre de stockage.
        for i in 1..idx.len() {
            let mut j = i;
            while j > 0 && starts[idx[j - 1]] > starts[idx[j]] {
                idx.swap(j - 1, j);
                j -= 1;
            }
        }
        idx
    }

    fn exception_setting(c: &Calendar, day: i32) -> Option<DaySetting> {
        let ex = c.exceptions();
        let starts: Vec<i32> = ex.iter().map(|e| e.period.first().index()).collect();
        order_by_start(&starts)
            .into_iter()
            .find(|&i| {
                ex[i].period.first().index() <= day
                    && day <= ex[i].period.last().index()
                    && expand(&ex[i]).contains(&day)
            })
            .map(|i| ex[i].setting.clone())
    }

    fn rule_setting(rule: &DayRule) -> Option<DaySetting> {
        match rule {
            DayRule::Inherit => None,
            DayRule::NonWorking => Some(DaySetting::NonWorking),
            DayRule::Working(h) => Some(DaySetting::Working(h.clone())),
        }
    }

    /// Semaines d'un niveau (R-019) : la première semaine datée qui couvre le
    /// jour le possède ; un jour `Inherit` y renvoie à la semaine par défaut
    /// du même calendrier.
    fn week_setting(c: &Calendar, day: i32, wd: Weekday) -> Option<DaySetting> {
        let ww: &[WorkWeek] = c.work_weeks();
        let starts: Vec<i32> = ww.iter().map(|w| w.period.first().index()).collect();
        let owner = order_by_start(&starts)
            .into_iter()
            .find(|&i| ww[i].period.first().index() <= day && day <= ww[i].period.last().index());
        if let Some(s) = owner.and_then(|i| rule_setting(ww[i].days.get(wd))) {
            return Some(s);
        }
        match c.kind() {
            CalendarKind::Base { week } => Some(week.get(wd).clone()),
            CalendarKind::Derived { week, .. } => rule_setting(week.get(wd)),
        }
    }

    /// Réglage du jour (R-016) : exception du dérivé, de la base, semaines du
    /// dérivé, semaines de la base.
    pub fn resolve(calendars: &Arena<CalendarUid, Calendar>, uid: CalendarUid, day: Day) -> DaySetting {
        let c = calendars.get(uid).unwrap();
        let levels: Vec<&Calendar> = match c.kind() {
            CalendarKind::Base { .. } => vec![c],
            CalendarKind::Derived { base, .. } => vec![c, calendars.get(*base).unwrap()],
        };
        let k = day.index();
        let wd = weekday(date(day));
        levels
            .iter()
            .find_map(|l| exception_setting(l, k))
            .or_else(|| levels.iter().find_map(|l| week_setting(l, k, wd)))
            .unwrap()
    }
}

/// Résultat compilé dans les termes de l'oracle.
fn as_naive(r: Result<Moment, CalendarError>) -> Naive {
    match r {
        Ok(m) => Naive::At(m.minutes()),
        Err(CalendarError::OutOfDomain {
            direction: Direction::After,
            ..
        }) => Naive::After,
        Err(CalendarError::OutOfDomain {
            direction: Direction::Before,
            ..
        }) => Naive::Before,
        Err(e) => panic!("erreur inattendue : {e}"),
    }
}

/// Compare si l'oracle sait répondre.
fn agree(compiled: Naive, naive: Naive) -> Result<(), TestCaseError> {
    if naive != Naive::Unknown {
        prop_assert_eq!(compiled, naive);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// P9 : calendrier dérivé mis à plat
// ---------------------------------------------------------------------------

/// Écrit le dérivé `derived` (de base `base`, sans semaine datée) comme un
/// calendrier de base : semaine par défaut résolue, mêmes semaines datées
/// (`Inherit` y renvoie à la semaine par défaut mise à plat), exceptions des
/// deux niveaux réécrites en séries disjointes, le dérivé d'abord.
fn flatten(base: &Calendar, derived: &Calendar) -> Calendar {
    let plannercraft_core::calendar::CalendarKind::Base { week: base_week } = base.kind() else {
        panic!("base attendue");
    };
    let plannercraft_core::calendar::CalendarKind::Derived { week: rules, .. } = derived.kind() else {
        panic!("dérivé attendu");
    };
    let week = Week::from_fn(|wd| match rules.get(wd) {
        DayRule::Inherit => base_week.get(wd).clone(),
        DayRule::NonWorking => DaySetting::NonWorking,
        DayRule::Working(h) => DaySetting::Working(h.clone()),
    });
    // Jour → réglage : dérivé d'abord, chaque niveau dans l'ordre (début,
    // indice), le premier l'emporte.
    let mut days: BTreeMap<i32, DaySetting> = BTreeMap::new();
    for c in [derived, base] {
        let mut order: Vec<(usize, &CalendarException)> = c.exceptions().iter().enumerate().collect();
        order.sort_by_key(|(i, e)| (e.period.first(), *i));
        for (_, e) in order {
            for r in expand_exception(e).unwrap() {
                for d in r.first().index()..=r.last().index() {
                    days.entry(d).or_insert_with(|| e.setting.clone());
                }
            }
        }
    }
    let mut exceptions: Vec<CalendarException> = Vec::new();
    let mut run: Option<(i32, i32, DaySetting)> = None;
    for (d, s) in days {
        match &mut run {
            Some((_, last, setting)) if *last + 1 == d && *setting == s => *last = d,
            _ => {
                if let Some((first, last, setting)) = run.take() {
                    exceptions.push(period_exception(first, last, setting));
                }
                run = Some((d, d, s));
            }
        }
    }
    if let Some((first, last, setting)) = run {
        exceptions.push(period_exception(first, last, setting));
    }
    let flat = Calendar::new_base(CalendarUid::new(3), "À plat".to_owned(), week);
    with_extras(&flat, &exceptions, derived.work_weeks())
}

fn period_exception(first: i32, last: i32, setting: DaySetting) -> CalendarException {
    CalendarException {
        name: String::new(),
        period: DayRange::new(Day::new(first).unwrap(), Day::new(last).unwrap()).unwrap(),
        setting,
        recurrence: Recurrence::Period,
        occurrences: None,
    }
}

// ---------------------------------------------------------------------------
// Propriétés
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    /// P1 : `add_working` et `working_between` sont inverses.
    #[test]
    fn p1_add_and_between_are_inverse(s in scenario(), t in moment(), d in delta()) {
        let c = s.compile();
        if let Ok(u) = c.add_working(t, Minutes::new(d)) {
            if d >= 0 {
                prop_assert_eq!(w(&c, t, u), d);
            } else {
                prop_assert_eq!(w(&c, u, t), -d);
            }
        }
    }

    /// P2 : minimalité pour d > 0, maximalité pour d < 0.
    #[test]
    fn p2_result_is_tight(s in scenario(), t in moment(), d in delta()) {
        let c = s.compile();
        if let Ok(u) = c.add_working(t, Minutes::new(d)) {
            if d > 0 && u > t {
                prop_assert!(w(&c, t, at(u.minutes() - 1)) < d);
            }
            if d < 0 && u < t {
                prop_assert!(w(&c, at(u.minutes() + 1), t) < -d);
            }
        }
    }

    /// P3 : instants ouvrés suivant et précédent.
    #[test]
    fn p3_next_and_previous(s in scenario(), t in moment()) {
        let c = s.compile();
        if let Ok(n) = c.next_working_moment(t) {
            prop_assert!(n >= t);
            prop_assert_eq!(c.next_working_moment(n), Ok(n));
            prop_assert_eq!(w(&c, t, n), 0);
            prop_assert_eq!(c.is_working(n), Ok(true));
        }
        if let Ok(p) = c.previous_working_moment(t) {
            prop_assert!(p <= t);
            prop_assert_eq!(c.previous_working_moment(p), Ok(p));
            prop_assert_eq!(w(&c, p, t), 0);
            prop_assert_eq!(c.is_working(at(p.minutes() - 1)), Ok(true));
        }
        if !c.has_working_time() {
            let none = Err(CalendarError::NoWorkingTime { calendar: s.uid });
            prop_assert_eq!(c.next_working_moment(t), none.clone());
            prop_assert_eq!(c.previous_working_moment(t), none.clone());
            prop_assert_eq!(c.add_working(t, Minutes::new(1)), none);
        }
    }

    /// P4 : monotonie.
    #[test]
    fn p4_monotonic(
        s in scenario(),
        t1 in moment(),
        gap in 0_i64..=20_000,
        d1 in delta(),
        d2 in delta(),
    ) {
        let c = s.compile();
        let t2 = at((t1.minutes() + gap).min(MAX));
        prop_assert!(w(&c, t1, t2) >= 0);
        if let (Ok(a), Ok(b)) = (c.add_working(t1, Minutes::new(d1)), c.add_working(t2, Minutes::new(d1))) {
            prop_assert!(a <= b);
        }
        let (lo, hi) = (d1.min(d2), d1.max(d2));
        if let (Ok(a), Ok(b)) = (c.add_working(t1, Minutes::new(lo)), c.add_working(t1, Minutes::new(hi))) {
            prop_assert!(a <= b);
        }
        if let (Ok(a), Ok(b)) = (c.next_working_moment(t1), c.next_working_moment(t2)) {
            prop_assert!(a <= b);
        }
        if let (Ok(a), Ok(b)) = (c.previous_working_moment(t1), c.previous_working_moment(t2)) {
            prop_assert!(a <= b);
        }
    }

    /// P5 : dualités.
    #[test]
    fn p5_dualities(s in scenario(), t in moment(), d in 1_i64..=50_000) {
        let c = s.compile();
        let d = Minutes::new(d);
        let back = Minutes::new(-d.get());
        if let Ok(u) = c.add_working(t, d) {
            prop_assert_eq!(c.add_working(u, back), c.next_working_moment(t));
        }
        if let Ok(u) = c.add_working(t, back) {
            prop_assert_eq!(c.add_working(u, d), c.previous_working_moment(t));
        }
    }

    /// P6 : Chasles, antisymétrie, bornes.
    #[test]
    fn p6_chasles(s in scenario(), a in moment(), b in moment(), x in moment()) {
        let c = s.compile();
        prop_assert_eq!(w(&c, a, b) + w(&c, b, x), w(&c, a, x));
        prop_assert_eq!(w(&c, a, b), -w(&c, b, a));
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        let v = w(&c, lo, hi);
        prop_assert!(v >= 0 && v <= hi.minutes() - lo.minutes());
    }

    /// P7 : calendrier 24 heures.
    #[test]
    fn p7_24_hours_is_civil_time(t in moment(), d in delta()) {
        let a = Arena::try_from(vec![twenty_four_hours(CalendarUid::new(2), "24 heures".to_owned())]).unwrap();
        let c = CompiledCalendar::compile(&a, CalendarUid::new(2)).unwrap();
        let civil = t.minutes() + d;
        match c.add_working(t, Minutes::new(d)) {
            Ok(u) => prop_assert_eq!(u.minutes(), civil),
            Err(_) => prop_assert!(!(0..=MAX).contains(&civil)),
        }
        prop_assert_eq!(w(&c, t, at(civil.clamp(0, MAX))), civil.clamp(0, MAX) - t.minutes());
    }

    /// P8 : forme compilée = oracle naïf fondé sur `resolve_day`.
    #[test]
    fn p8_matches_naive_oracle(
        s in scenario(),
        t in moment(),
        other in -WINDOW * 1_440..=WINDOW * 1_440,
        d in delta(),
    ) {
        let c = s.compile();
        let o = Oracle::new(&s, t);
        // Plages de chaque jour de la fenêtre.
        for (i, ranges) in o.days.iter().enumerate() {
            let day = Day::new(i32::try_from(o.first_day + i64::try_from(i).unwrap()).unwrap()).unwrap();
            // `resolve_day` contre l'oracle indépendant.
            let independent = merged(&independent::resolve(&s.calendars, s.uid, day));
            prop_assert_eq!(&independent, ranges, "jour {} (oracle indépendant)", day);
            let got: Vec<(i64, i64)> = c
                .day_ranges(day)
                .unwrap()
                .iter()
                .map(|r| (i64::from(r.from().minutes()), i64::from(r.to().minutes())))
                .collect();
            prop_assert_eq!(&got, ranges, "jour {}", day);
        }
        // W entre deux instants de la fenêtre.
        let b = (t.minutes() + other).clamp(0, MAX);
        if o.contains(b) {
            let (lo, hi) = (t.minutes().min(b), t.minutes().max(b));
            prop_assert_eq!(w(&c, at(lo), at(hi)), o.between(lo, hi));
        }
        if !c.has_working_time() {
            prop_assert!(o.days.iter().all(Vec::is_empty));
            prop_assert_eq!(
                c.add_working(t, Minutes::new(d)),
                if d == 0 { Ok(t) } else { Err(CalendarError::NoWorkingTime { calendar: s.uid }) }
            );
            return Ok(());
        }
        agree(as_naive(c.add_working(t, Minutes::new(d))), o.add(t.minutes(), d))?;
        agree(as_naive(c.next_working_moment(t)), o.next(t.minutes()))?;
        agree(as_naive(c.previous_working_moment(t)), o.previous(t.minutes()))?;
    }

    /// P9 : dérivé compilé = le même calendrier écrit en entier comme base.
    #[test]
    fn p9_derived_equals_flattened_base(
        week in base_week(),
        base_exceptions in prop::collection::vec(exception(), 0..=10),
        rules in week_of(day_rule()),
        derived_exceptions in prop::collection::vec(exception(), 0..=10),
        derived_work_weeks in prop::collection::vec(work_week(), 0..=3),
        t in moment(),
        d in delta(),
    ) {
        let base = with_extras(
            &Calendar::new_base(CalendarUid::new(1), "Base".to_owned(), week),
            &base_exceptions,
            &[],
        );
        let mut derived = Calendar::new_derived(CalendarUid::new(2), "Dérivé".to_owned(), CalendarUid::new(1));
        for wd in Weekday::ALL {
            derived.set_day(wd, rules.get(wd).clone()).unwrap();
        }
        let derived = with_extras(&derived, &derived_exceptions, &derived_work_weeks);
        let flat = flatten(&base, &derived);
        let a = Arena::try_from(vec![base, derived, flat]).unwrap();
        let cd = CompiledCalendar::compile(&a, CalendarUid::new(2)).unwrap();
        let cf = CompiledCalendar::compile(&a, CalendarUid::new(3)).unwrap();
        prop_assert_eq!(cd.total_working(), cf.total_working());
        for d in day_index("2025-12-01")..=day_index("2028-04-01") {
            let day = Day::new(d).unwrap();
            prop_assert_eq!(cd.day_ranges(day).unwrap(), cf.day_ranges(day).unwrap(), "jour {}", day);
        }
        let d = Minutes::new(d);
        let as_flat = |r: Result<Moment, CalendarError>| match r {
            Err(CalendarError::NoWorkingTime { .. }) => Err("aucun temps ouvré"),
            Err(CalendarError::OutOfDomain { direction, .. }) => Err(match direction {
                Direction::After => "après",
                Direction::Before => "avant",
            }),
            Err(_) => Err("autre"),
            Ok(m) => Ok(m),
        };
        prop_assert_eq!(as_flat(cd.add_working(t, d)), as_flat(cf.add_working(t, d)));
        prop_assert_eq!(as_flat(cd.next_working_moment(t)), as_flat(cf.next_working_moment(t)));
        prop_assert_eq!(as_flat(cd.previous_working_moment(t)), as_flat(cf.previous_working_moment(t)));
    }

    /// P10 : placement en temps écoulé et en temps ouvré.
    #[test]
    fn p10_spans(s in scenario(), t in moment(), b in moment(), d in 0_i64..=50_000) {
        let e = TimeScale::Elapsed;
        prop_assert_eq!(e.between(t, b), Ok(Minutes::new(b.minutes() - t.minutes())));
        match e.forward_span(t, Minutes::new(d)) {
            Ok(span) => {
                prop_assert_eq!(span.start, t);
                prop_assert_eq!(span.finish.minutes(), t.minutes() + d);
            }
            Err(_) => prop_assert!(t.minutes() + d > MAX),
        }
        match e.backward_span(t, Minutes::new(d)) {
            Ok(span) => {
                prop_assert_eq!(span.finish, t);
                prop_assert_eq!(span.start.minutes(), t.minutes() - d);
            }
            Err(_) => prop_assert!(t.minutes() - d < 0),
        }
        let c = s.compile();
        let w_scale = TimeScale::Working(&c);
        if let Ok(span) = w_scale.forward_span(t, Minutes::new(d)) {
            prop_assert!(span.start >= t && span.finish >= span.start);
            prop_assert_eq!(w(&c, span.start, span.finish), d);
            prop_assert_eq!(w(&c, t, span.start), 0);
        }
        if let Ok(span) = w_scale.backward_span(t, Minutes::new(d)) {
            prop_assert!(span.finish <= t && span.finish >= span.start);
            prop_assert_eq!(w(&c, span.start, span.finish), d);
            prop_assert_eq!(w(&c, span.finish, t), 0);
        }
    }
}
