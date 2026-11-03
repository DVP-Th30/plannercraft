//! Dépliage des récurrences d'exception en séries de jours (R-018).
//!
//! Une exception est toujours dépliée en jours explicites, restreints à sa
//! période et limités à `occurrences` occurrences si ce champ est renseigné
//! (ordre chronologique). Une récurrence n'est jamais lue comme une période
//! continue. Les types de sémantique non documentée (`ByDayCount`,
//! `ByWeekdayCount`) ne donnent aucun jour (diagnostic
//! `CalendarRecurrenceUnsupported`).
//!
//! Ce module contient aussi [`DayClaims`], l'attribution « le premier
//! l'emporte » de séries de jours, partagée par la compilation (R-017) et par
//! le contrôle des chevauchements de `Project::validate`.

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};

use crate::calendar::model::{CalendarException, MonthItem, MonthPosition, Recurrence};
use crate::error::CalendarModelError;
use crate::time::{Day, DayRange, Month, Week, Weekday};

/// Déplie une exception (R-018) : séries de jours consécutifs, triées,
/// disjointes, bornées à la période (donc au domaine).
///
/// - `Period` : chaque jour de la période ;
/// - `Daily { every }` : `first + k × every` ;
/// - `Weekly { every, days }` : semaines (commençant le dimanche) comptées
///   depuis celle qui contient `period.first`, une sur `every`, jours cochés ;
/// - `MonthlyByDay` et `MonthlyByPosition` : un mois sur `every`, compté
///   depuis le mois de `period.first` ; un jour inexistant (31 avril) est
///   sauté ;
/// - `YearlyByDay` et `YearlyByPosition` : chaque année de la période ; un
///   29 février d'année non bissextile est sauté ;
/// - `ByDayCount`, `ByWeekdayCount` : aucun jour.
///
/// `occurrences = Some(0)` ne donne aucun jour (hypothèse).
///
/// # Errors
///
/// [`CalendarModelError::InvalidRecurrence`] si la récurrence est invalide
/// (`every` nul, jour du mois hors de 1 à 31).
pub fn expand_exception(e: &CalendarException) -> Result<Vec<DayRange>, CalendarModelError> {
    e.recurrence.validate()?;
    let period = e.period;
    let limit = e.occurrences.map(|n| usize::try_from(n).unwrap_or(usize::MAX));
    let days: Vec<Day> = match e.recurrence {
        // Séries contiguës : pas de dépliage jour par jour.
        Recurrence::Period | Recurrence::Daily { every: 1 } => return Ok(contiguous(period, limit)),
        Recurrence::Daily { every } => step_days(period, every),
        Recurrence::Weekly { every, days } => weekly(period, every, days),
        Recurrence::MonthlyByDay { every, day } => monthly(period, every, |y, m| date(y, m, u32::from(day))),
        Recurrence::MonthlyByPosition {
            every,
            position,
            item,
        } => monthly(period, every, |y, m| nth_in_month(y, m, position, item)),
        Recurrence::YearlyByDay { month, day } => yearly(period, month, |y, m| date(y, m, u32::from(day))),
        Recurrence::YearlyByPosition {
            month,
            position,
            item,
        } => yearly(period, month, |y, m| nth_in_month(y, m, position, item)),
        Recurrence::ByDayCount { .. } | Recurrence::ByWeekdayCount { .. } => Vec::new(),
    };
    let days = days
        .into_iter()
        .filter(|d| period.contains(*d))
        .take(limit.unwrap_or(usize::MAX));
    Ok(to_runs(days))
}

/// Période entière, tronquée à `limit` jours.
fn contiguous(period: DayRange, limit: Option<usize>) -> Vec<DayRange> {
    let last = match limit {
        None => period.last(),
        Some(0) => return Vec::new(),
        Some(n) => {
            // n − 1 jours après le premier, sans dépasser la période.
            let extra = i32::try_from(n.saturating_sub(1)).unwrap_or(i32::MAX);
            let span = period.last().index().saturating_sub(period.first().index());
            let extra = extra.min(span);
            period.first().checked_add_days(extra).unwrap_or(period.last())
        }
    };
    DayRange::new(period.first(), last).map_or_else(|_| Vec::new(), |r| vec![r])
}

/// Un jour sur `every` depuis le premier jour de la période.
fn step_days(period: DayRange, every: u16) -> Vec<Day> {
    let first = period.first().index();
    let last = period.last().index();
    (first..=last)
        .step_by(usize::from(every))
        .filter_map(|i| Day::new(i).ok())
        .collect()
}

/// Jours cochés, une semaine (dimanche à samedi) sur `every`, comptée depuis
/// celle qui contient le premier jour de la période. Le jour 0 est un
/// dimanche : la semaine d'un jour `d` est `d div 7`.
fn weekly(period: DayRange, every: u16, days: Week<bool>) -> Vec<Day> {
    let week_of = |d: i32| d.checked_div_euclid(7).unwrap_or(0);
    let start_week = week_of(period.first().index());
    let every = i32::from(every);
    (period.first().index()..=period.last().index())
        .filter_map(|i| Day::new(i).ok())
        .filter(|d| {
            let offset = week_of(d.index()).saturating_sub(start_week);
            offset.checked_rem_euclid(every) == Some(0) && *days.get(d.weekday())
        })
        .collect()
}

/// Année et mois (1 à 12) d'un jour.
fn year_month(d: Day) -> (i32, u32) {
    let n = d.to_naive();
    (n.year(), n.month())
}

/// Un mois sur `every` depuis le mois du premier jour de la période ; `pick`
/// choisit le jour dans le mois (ou aucun s'il n'existe pas).
fn monthly(period: DayRange, every: u16, pick: impl Fn(i32, u32) -> Option<Day>) -> Vec<Day> {
    let index = |(y, m): (i32, u32)| {
        y.checked_mul(12)
            .and_then(|v| v.checked_add(i32::try_from(m).ok()?))
            .and_then(|v| v.checked_sub(1))
    };
    let (Some(start), Some(end)) = (
        index(year_month(period.first())),
        index(year_month(period.last())),
    ) else {
        return Vec::new();
    };
    (start..=end)
        .step_by(usize::from(every))
        .filter_map(|abs| {
            let y = abs.checked_div_euclid(12)?;
            let m = u32::try_from(abs.checked_rem_euclid(12)?).ok()?.checked_add(1)?;
            pick(y, m)
        })
        .collect()
}

/// Chaque année de la période, au mois `month` ; `pick` choisit le jour.
fn yearly(period: DayRange, month: Month, pick: impl Fn(i32, u32) -> Option<Day>) -> Vec<Day> {
    let (first_year, _) = year_month(period.first());
    let (last_year, _) = year_month(period.last());
    (first_year..=last_year)
        .filter_map(|y| pick(y, month.number()))
        .collect()
}

/// Jour civil s'il existe et appartient au domaine.
fn date(y: i32, m: u32, d: u32) -> Option<Day> {
    NaiveDate::from_ymd_opt(y, m, d).and_then(|n| Day::from_naive(n).ok())
}

/// n-ième `item` du mois (ou le dernier).
fn nth_in_month(y: i32, m: u32, position: MonthPosition, item: MonthItem) -> Option<Day> {
    let mut candidates = (1..=31).filter_map(|d| date(y, m, d)).filter(|d| {
        let wd = d.weekday();
        let weekend = matches!(wd, Weekday::Saturday | Weekday::Sunday);
        match item {
            MonthItem::Day => true,
            MonthItem::Weekday => !weekend,
            MonthItem::WeekendDay => weekend,
            MonthItem::On(target) => wd == target,
        }
    });
    match position {
        MonthPosition::First => candidates.next(),
        MonthPosition::Second => candidates.nth(1),
        MonthPosition::Third => candidates.nth(2),
        MonthPosition::Fourth => candidates.nth(3),
        MonthPosition::Last => candidates.next_back(),
    }
}

/// Jours triés et distincts → séries de jours consécutifs.
fn to_runs(days: impl Iterator<Item = Day>) -> Vec<DayRange> {
    let mut runs: Vec<(Day, Day)> = Vec::new();
    for d in days {
        match runs.last_mut() {
            Some((_, last)) if last.index().checked_add(1) == Some(d.index()) => *last = d,
            Some((_, last)) if *last >= d => {}
            _ => runs.push((d, d)),
        }
    }
    runs.into_iter()
        .filter_map(|(a, b)| DayRange::new(a, b).ok())
        .collect()
}

/// Vrai si deux listes de séries triées et disjointes ont un jour commun.
#[must_use]
pub(crate) fn runs_overlap(a: &[DayRange], b: &[DayRange]) -> bool {
    let (mut i, mut j) = (a.iter().peekable(), b.iter().peekable());
    while let (Some(x), Some(y)) = (i.peek(), j.peek()) {
        if x.overlaps(**y) {
            return true;
        }
        if x.last() < y.last() {
            i.next();
        } else {
            j.next();
        }
    }
    false
}

/// Attribution de jours « le premier l'emporte » : séries disjointes de jours
/// (`[first, last]`, numéros de jour) et leur propriétaire.
///
/// Les séries sont posées dans l'ordre de précédence ; une série ne prend que
/// les jours encore libres (R-017).
#[derive(Clone, Debug, Default)]
pub(crate) struct DayClaims<T> {
    runs: BTreeMap<i32, (i32, T)>,
    /// Tampon des jours libres d'une attribution, réutilisé (aucune
    /// allocation par série une fois la capacité atteinte).
    gaps: Vec<(i32, i32)>,
}

impl<T: Copy + PartialEq> DayClaims<T> {
    /// Attribution vide.
    pub(crate) const fn new() -> Self {
        Self {
            runs: BTreeMap::new(),
            gaps: Vec::new(),
        }
    }

    /// Attribue à `owner` les jours libres de `[first, last]` et rend les
    /// propriétaires (distincts, par jour croissant) des jours déjà pris.
    pub(crate) fn claim(&mut self, first: i32, last: i32, owner: T) -> Vec<T> {
        // Série déjà posée qui contient `first`, s'il y en a une.
        let start = self
            .runs
            .range(..=first)
            .next_back()
            .filter(|(_, (l, _))| *l >= first)
            .map_or(first, |(f, _)| *f);
        let mut conflicts: Vec<T> = Vec::new();
        let gaps = &mut self.gaps;
        gaps.clear();
        let mut cursor = first;
        for (&f, &(l, o)) in self.runs.range(start..=last) {
            if f > cursor {
                gaps.push((cursor, f.saturating_sub(1)));
            }
            if !conflicts.contains(&o) {
                conflicts.push(o);
            }
            cursor = cursor.max(l.saturating_add(1));
        }
        if cursor <= last {
            gaps.push((cursor, last));
        }
        for &(a, b) in gaps.iter() {
            self.runs.insert(a, (b, owner));
        }
        conflicts
    }

    /// Propriétaire du jour `day`, s'il est pris.
    pub(crate) fn owner(&self, day: i32) -> Option<T> {
        self.runs
            .range(..=day)
            .next_back()
            .filter(|(_, (l, _))| *l >= day)
            .map(|(_, (_, o))| *o)
    }

    /// Séries posées, triées : `(first, last, owner)`.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (i32, i32, T)> + '_ {
        self.runs.iter().map(|(f, (l, o))| (*f, *l, *o))
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{DayClaims, expand_exception, runs_overlap};
    use crate::calendar::model::{CalendarException, DaySetting, MonthItem, MonthPosition, Recurrence};
    use crate::error::CalendarModelError;
    use crate::time::{Day, DayRange, Month, Week, Weekday};

    fn day(s: &str) -> Day {
        s.parse().unwrap()
    }

    fn ex(from: &str, to: &str, recurrence: Recurrence, occurrences: Option<u32>) -> CalendarException {
        CalendarException {
            name: String::new(),
            period: DayRange::new(day(from), day(to)).unwrap(),
            setting: DaySetting::NonWorking,
            recurrence,
            occurrences,
        }
    }

    /// Séries dépliées, en texte.
    fn runs(e: &CalendarException) -> Vec<(String, String)> {
        expand_exception(e)
            .unwrap()
            .into_iter()
            .map(|r| (r.first().to_string(), r.last().to_string()))
            .collect()
    }

    fn single(days: &[&str]) -> Vec<(String, String)> {
        days.iter().map(|d| ((*d).to_owned(), (*d).to_owned())).collect()
    }

    fn pair(a: &str, b: &str) -> (String, String) {
        (a.to_owned(), b.to_owned())
    }

    #[test]
    fn period_and_daily() {
        let e = ex("2026-12-24", "2026-12-31", Recurrence::Period, None);
        assert_eq!(runs(&e), [pair("2026-12-24", "2026-12-31")]);
        let e = ex("2026-12-24", "2026-12-31", Recurrence::Period, Some(3));
        assert_eq!(runs(&e), [pair("2026-12-24", "2026-12-26")]);
        let e = ex("2026-12-24", "2026-12-31", Recurrence::Period, Some(0));
        assert_eq!(runs(&e), []);
        let e = ex("2026-12-24", "2026-12-31", Recurrence::Period, Some(100));
        assert_eq!(runs(&e), [pair("2026-12-24", "2026-12-31")]);
        let e = ex("2026-11-02", "2026-11-06", Recurrence::Daily { every: 2 }, None);
        assert_eq!(runs(&e), single(&["2026-11-02", "2026-11-04", "2026-11-06"]));
        let e = ex(
            "2026-11-26",
            "2026-12-31",
            Recurrence::Daily { every: 1 },
            Some(2),
        );
        assert_eq!(runs(&e), [pair("2026-11-26", "2026-11-27")]);
        let e = ex(
            "2026-11-02",
            "2026-11-30",
            Recurrence::Daily { every: 10 },
            Some(2),
        );
        assert_eq!(runs(&e), single(&["2026-11-02", "2026-11-12"]));
    }

    #[test]
    fn weekly() {
        let fridays = Week::from_fn(|d| d == Weekday::Friday);
        let e = ex(
            "2027-01-01",
            "2027-01-31",
            Recurrence::Weekly {
                every: 1,
                days: fridays,
            },
            None,
        );
        assert_eq!(
            runs(&e),
            single(&[
                "2027-01-01",
                "2027-01-08",
                "2027-01-15",
                "2027-01-22",
                "2027-01-29"
            ])
        );
        // Semaine du mer 04/11/2026 : dim 01/11 au sam 07/11 ; une sur deux :
        // semaines du 15/11 et du 29/11 ; lundis 16/11 et 30/11.
        let mondays = Week::from_fn(|d| d == Weekday::Monday);
        let e = ex(
            "2026-11-04",
            "2026-12-06",
            Recurrence::Weekly {
                every: 2,
                days: mondays,
            },
            None,
        );
        assert_eq!(runs(&e), single(&["2026-11-16", "2026-11-30"]));
        // Samedi et dimanche consécutifs : une seule série.
        let weekend = Week::from_fn(|d| matches!(d, Weekday::Saturday | Weekday::Sunday));
        let e = ex(
            "2026-10-24",
            "2026-11-01",
            Recurrence::Weekly {
                every: 1,
                days: weekend,
            },
            Some(3),
        );
        // Trois occurrences : sam 24, dim 25 (début de la semaine suivante),
        // sam 31 ; le 24 et le 25 forment une seule série.
        assert_eq!(
            runs(&e),
            [pair("2026-10-24", "2026-10-25"), pair("2026-10-31", "2026-10-31")]
        );
    }

    #[test]
    fn monthly() {
        let e = ex(
            "2027-01-01",
            "2027-03-31",
            Recurrence::MonthlyByPosition {
                every: 1,
                position: MonthPosition::Last,
                item: MonthItem::On(Weekday::Friday),
            },
            None,
        );
        assert_eq!(runs(&e), single(&["2027-01-29", "2027-02-26", "2027-03-26"]));
        let e = ex(
            "2027-01-01",
            "2027-04-30",
            Recurrence::MonthlyByDay { every: 1, day: 31 },
            None,
        );
        assert_eq!(runs(&e), single(&["2027-01-31", "2027-03-31"]));
        // Un mois sur deux depuis janvier : janvier, mars, mai ; le 15 janvier
        // est avant la période (qui commence le 20).
        let e = ex(
            "2027-01-20",
            "2027-05-31",
            Recurrence::MonthlyByDay { every: 2, day: 15 },
            None,
        );
        assert_eq!(runs(&e), single(&["2027-03-15", "2027-05-15"]));
        // Positions : janvier 2027 commence un vendredi.
        let pos = |position, item| {
            let e = ex(
                "2027-01-01",
                "2027-01-31",
                Recurrence::MonthlyByPosition {
                    every: 1,
                    position,
                    item,
                },
                None,
            );
            runs(&e)
        };
        assert_eq!(pos(MonthPosition::First, MonthItem::Day), single(&["2027-01-01"]));
        assert_eq!(pos(MonthPosition::Last, MonthItem::Day), single(&["2027-01-31"]));
        // Jours ouvrés : ven 1, lun 4, mar 5, mer 6.
        assert_eq!(
            pos(MonthPosition::Second, MonthItem::Weekday),
            single(&["2027-01-04"])
        );
        assert_eq!(
            pos(MonthPosition::Fourth, MonthItem::Weekday),
            single(&["2027-01-06"])
        );
        assert_eq!(
            pos(MonthPosition::Last, MonthItem::Weekday),
            single(&["2027-01-29"])
        );
        // Week-ends : sam 2, dim 3, sam 9.
        assert_eq!(
            pos(MonthPosition::Third, MonthItem::WeekendDay),
            single(&["2027-01-09"])
        );
        assert_eq!(
            pos(MonthPosition::Last, MonthItem::WeekendDay),
            single(&["2027-01-31"])
        );
        assert_eq!(
            pos(MonthPosition::First, MonthItem::On(Weekday::Monday)),
            single(&["2027-01-04"])
        );
    }

    #[test]
    fn yearly() {
        let e = ex(
            "2026-12-25",
            "2028-12-25",
            Recurrence::YearlyByDay {
                month: Month::December,
                day: 25,
            },
            None,
        );
        assert_eq!(runs(&e), single(&["2026-12-25", "2027-12-25", "2028-12-25"]));
        // 29 février : seulement les années bissextiles.
        let e = ex(
            "2027-01-01",
            "2029-12-31",
            Recurrence::YearlyByDay {
                month: Month::February,
                day: 29,
            },
            None,
        );
        assert_eq!(runs(&e), single(&["2028-02-29"]));
        let e = ex(
            "2026-01-01",
            "2027-12-31",
            Recurrence::YearlyByPosition {
                month: Month::November,
                position: MonthPosition::Fourth,
                item: MonthItem::On(Weekday::Thursday),
            },
            None,
        );
        assert_eq!(runs(&e), single(&["2026-11-26", "2027-11-25"]));
    }

    #[test]
    fn domain_bounds_and_errors() {
        // Dernier jour du domaine : la récurrence annuelle s'arrête en 2149.
        let e = ex(
            "2148-01-01",
            "2149-12-31",
            Recurrence::YearlyByDay {
                month: Month::December,
                day: 31,
            },
            None,
        );
        assert_eq!(runs(&e), single(&["2148-12-31", "2149-12-31"]));
        let e = ex("1984-01-01", "1984-01-03", Recurrence::Period, None);
        assert_eq!(runs(&e), [pair("1984-01-01", "1984-01-03")]);
        let e = ex("2026-01-01", "2026-01-31", Recurrence::Daily { every: 0 }, None);
        assert_eq!(expand_exception(&e), Err(CalendarModelError::InvalidRecurrence));
        let e = ex(
            "2026-01-01",
            "2026-01-31",
            Recurrence::ByDayCount { every: 1 },
            None,
        );
        assert_eq!(runs(&e), []);
        let e = ex(
            "2026-01-01",
            "2026-01-31",
            Recurrence::ByWeekdayCount { every: 1 },
            None,
        );
        assert_eq!(runs(&e), []);
    }

    #[test]
    fn overlap_of_runs() {
        let r = |a: &str, b: &str| DayRange::new(day(a), day(b)).unwrap();
        let a = [r("2026-01-01", "2026-01-03"), r("2026-01-10", "2026-01-12")];
        assert!(runs_overlap(&a, &[r("2026-01-12", "2026-01-20")]));
        assert!(!runs_overlap(&a, &[r("2026-01-04", "2026-01-09")]));
        assert!(!runs_overlap(&a, &[]));
    }

    #[test]
    fn claims_first_wins() {
        let mut c: DayClaims<u8> = DayClaims::new();
        assert_eq!(c.claim(10, 20, 1), [] as [u8; 0]);
        // [5, 25] : prend [5, 9] et [21, 25] ; conflit avec 1.
        assert_eq!(c.claim(5, 25, 2), [1]);
        assert_eq!(c.claim(30, 30, 3), [] as [u8; 0]);
        assert_eq!(c.claim(0, 40, 4), [2, 1, 3]);
        assert_eq!(
            c.iter().collect::<Vec<_>>(),
            [
                (0, 4, 4),
                (5, 9, 2),
                (10, 20, 1),
                (21, 25, 2),
                (26, 29, 4),
                (30, 30, 3),
                (31, 40, 4)
            ]
        );
        assert_eq!(c.owner(9), Some(2));
        assert_eq!(c.owner(41), None);
        assert_eq!(c.claim(12, 12, 5), [1]);
    }
}
