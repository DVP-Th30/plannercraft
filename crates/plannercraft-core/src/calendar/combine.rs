//! Intersection et union de calendriers compilés (R-172, ADR-0008, ADR-0013
//! décision 2).
//!
//! Les exceptions et les semaines datées sont déjà dépliées dans la forme
//! compilée : combiner deux calendriers revient à fusionner leurs deux
//! partitions de segments (deux curseurs, O(n + m) segments) et à croiser,
//! sur chaque intervalle élémentaire, les deux règles :
//! - `Daily ∩ Daily` → `Daily` (un motif de jour croisé) ;
//! - `Weekly ∩ Weekly`, `Daily ∩ Weekly` → `Weekly` (7 motifs de jour
//!   croisés, jour de semaine par jour de semaine).
//!
//! Un motif de jour croisé est l'intersection (ou l'union) des plages triées
//! des deux motifs, plages contiguës réunies. Motifs et règles croisés sont
//! internés et mis en cache par paire (`BTreeMap`, déterministe) ; segments
//! voisins identiques fusionnés ; cumuls recalculés par
//! [`CompiledCalendar::assemble`]. Aucune précédence n'est rejouée.

use std::collections::BTreeMap;

use crate::calendar::compiled::{CompiledCalendar, DAY_COUNT, Patterns, Segment, SegmentRule};
use crate::calendar::model::TimeRange;
use crate::error::CalendarError;
use crate::time::{TimeOfDay, Week, Weekday};

/// Plafond de segments et de motifs d'un calendrier combiné (2²⁰). Garde
/// défensive : le domaine compte 60 631 jours, une partition a donc au plus
/// 60 631 segments et 7 × 60 631 + 1 motifs de jour.
const LIMIT: u32 = 1 << 20;

/// Opération ensembliste sur le temps ouvré.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Op {
    /// Ouvré dans les deux.
    Intersect,
    /// Ouvré dans l'un ou l'autre.
    Union,
}

/// Combinaison de `a` et `b`, UID de `a`, plafond [`LIMIT`].
///
/// # Errors
///
/// Voir [`combine_bounded`].
pub(super) fn combine(
    a: &CompiledCalendar,
    b: &CompiledCalendar,
    op: Op,
) -> Result<CompiledCalendar, CalendarError> {
    combine_bounded(a, b, op, LIMIT)
}

/// Échec d'une combinaison, avant traduction en [`CalendarError`].
enum Failure {
    /// Plafond de segments ou de motifs dépassé.
    Limit,
    /// Invariant interne violé.
    Inconsistent,
}

/// Combinaison de `a` et `b` (UID de `a`) avec un plafond `limit` de
/// segments et de motifs.
///
/// # Errors
///
/// [`CalendarError::TooManySegments`] au-delà du plafond ;
/// [`CalendarError::Inconsistent`] (UID de `a`) si un invariant est violé.
fn combine_bounded(
    a: &CompiledCalendar,
    b: &CompiledCalendar,
    op: Op,
    limit: u32,
) -> Result<CompiledCalendar, CalendarError> {
    let run = || -> Result<CompiledCalendar, Failure> {
        let mut cross = Cross::new(a, b, op, limit)?;
        let raw = cross.partition()?;
        CompiledCalendar::assemble(a.uid(), cross.patterns, &raw).ok_or(Failure::Inconsistent)
    };
    run().map_err(|f| match f {
        Failure::Limit => CalendarError::TooManySegments { limit },
        Failure::Inconsistent => a.inconsistent(),
    })
}

/// État d'une combinaison : motifs du résultat et caches par paire.
struct Cross<'a> {
    a: &'a CompiledCalendar,
    b: &'a CompiledCalendar,
    op: Op,
    limit: usize,
    patterns: Patterns,
    /// (motif de `a`, motif de `b`) → motif du résultat.
    days: BTreeMap<(u32, u32), u32>,
    /// (règle de `a`, règle de `b`) → règle du résultat.
    rules: BTreeMap<(SegmentRule, SegmentRule), SegmentRule>,
}

impl<'a> Cross<'a> {
    fn new(a: &'a CompiledCalendar, b: &'a CompiledCalendar, op: Op, limit: u32) -> Result<Self, Failure> {
        Ok(Self {
            a,
            b,
            op,
            limit: usize::try_from(limit).map_err(|_| Failure::Inconsistent)?,
            patterns: Patterns::new().ok_or(Failure::Inconsistent)?,
            days: BTreeMap::new(),
            rules: BTreeMap::new(),
        })
    }

    /// Partition du résultat : les deux partitions parcourues ensemble,
    /// règles croisées, voisins identiques fusionnés.
    fn partition(&mut self) -> Result<Vec<(i32, SegmentRule)>, Failure> {
        let (sa, sb) = (&self.a.segments, &self.b.segments);
        let next = |s: &[Segment], i: usize| -> Option<i32> {
            match s.get(i.checked_add(1)?) {
                Some(seg) => Some(seg.first_day),
                None => Some(DAY_COUNT),
            }
        };
        let mut raw: Vec<(i32, SegmentRule)> = Vec::new();
        let (mut i, mut j, mut day) = (0_usize, 0_usize, 0_i32);
        while day < DAY_COUNT {
            let ra = sa.get(i).ok_or(Failure::Inconsistent)?.rule;
            let rb = sb.get(j).ok_or(Failure::Inconsistent)?.rule;
            let rule = self.rule(ra, rb)?;
            if raw.last().map(|(_, r)| *r) != Some(rule) {
                if raw.len() >= self.limit {
                    return Err(Failure::Limit);
                }
                raw.push((day, rule));
            }
            let na = next(sa, i).ok_or(Failure::Inconsistent)?;
            let nb = next(sb, j).ok_or(Failure::Inconsistent)?;
            let end = na.min(nb);
            // Chaque tour avance au moins un curseur : les deux partitions
            // sont strictement croissantes et finissent à DAY_COUNT.
            if end <= day {
                return Err(Failure::Inconsistent);
            }
            if na == end {
                i = i.checked_add(1).ok_or(Failure::Inconsistent)?;
            }
            if nb == end {
                j = j.checked_add(1).ok_or(Failure::Inconsistent)?;
            }
            day = end;
        }
        Ok(raw)
    }

    /// Règle croisée de `ra` (de `a`) et `rb` (de `b`), mise en cache.
    fn rule(&mut self, ra: SegmentRule, rb: SegmentRule) -> Result<SegmentRule, Failure> {
        if let Some(r) = self.rules.get(&(ra, rb)) {
            return Ok(*r);
        }
        let r = if let (SegmentRule::Daily(pa), SegmentRule::Daily(pb)) = (ra, rb) {
            SegmentRule::Daily(self.day(pa, pb)?)
        } else {
            // Au moins une règle hebdomadaire : 7 motifs croisés.
            let mut days: Week<u32> = Week::default();
            for wd in Weekday::ALL {
                let pa = day_of(self.a, ra, wd)?;
                let pb = day_of(self.b, rb, wd)?;
                *days.get_mut(wd) = self.day(pa, pb)?;
            }
            let w = self.patterns.week_of_days(days).ok_or(Failure::Inconsistent)?;
            if self.patterns.weeks.len() > self.limit {
                return Err(Failure::Limit);
            }
            SegmentRule::Weekly(w)
        };
        self.rules.insert((ra, rb), r);
        Ok(r)
    }

    /// Motif de jour croisé de `pa` (de `a`) et `pb` (de `b`), mis en cache.
    fn day(&mut self, pa: u32, pb: u32) -> Result<u32, Failure> {
        if let Some(p) = self.days.get(&(pa, pb)) {
            return Ok(*p);
        }
        let ra = ranges_of(self.a, pa)?;
        let rb = ranges_of(self.b, pb)?;
        let merged = match self.op {
            Op::Intersect => intersect_ranges(ra, rb),
            Op::Union => union_ranges(ra, rb),
        };
        let ranges = to_time_ranges(&merged).ok_or(Failure::Inconsistent)?;
        let p = self.patterns.day_of_ranges(ranges).ok_or(Failure::Inconsistent)?;
        if self.patterns.days.len() > self.limit {
            return Err(Failure::Limit);
        }
        self.days.insert((pa, pb), p);
        Ok(p)
    }
}

/// Motif de jour du jour de semaine `wd` sous la règle `rule` de `c`.
fn day_of(c: &CompiledCalendar, rule: SegmentRule, wd: Weekday) -> Result<u32, Failure> {
    match rule {
        SegmentRule::Daily(p) => Ok(p),
        SegmentRule::Weekly(w) => {
            let week = usize::try_from(w)
                .ok()
                .and_then(|w| c.week_patterns.get(w))
                .ok_or(Failure::Inconsistent)?;
            Ok(*week.days.get(wd))
        }
    }
}

/// Plages du motif de jour `p` de `c`, en minutes du jour.
fn ranges_of(c: &CompiledCalendar, p: u32) -> Result<Vec<(u16, u16)>, Failure> {
    let pattern = usize::try_from(p)
        .ok()
        .and_then(|p| c.day_patterns.get(p))
        .ok_or(Failure::Inconsistent)?;
    Ok(pattern
        .ranges
        .iter()
        .map(|r| (r.from().minutes(), r.to().minutes()))
        .collect())
}

/// Intersection de deux listes de plages triées et disjointes : plages
/// triées, disjointes, contiguës réunies.
fn intersect_ranges(a: Vec<(u16, u16)>, b: Vec<(u16, u16)>) -> Vec<(u16, u16)> {
    let mut out: Vec<(u16, u16)> = Vec::new();
    let mut ia = a.into_iter().peekable();
    let mut ib = b.into_iter().peekable();
    while let (Some(&(a0, a1)), Some(&(b0, b1))) = (ia.peek(), ib.peek()) {
        let (lo, hi) = (a0.max(b0), a1.min(b1));
        if lo < hi {
            push_fused(&mut out, lo, hi);
        }
        // La plage qui finit la première ne recoupe plus rien.
        if a1 <= b1 {
            ia.next();
        } else {
            ib.next();
        }
    }
    out
}

/// Union de deux listes de plages triées et disjointes : plages triées,
/// disjointes, contiguës réunies.
fn union_ranges(a: Vec<(u16, u16)>, b: Vec<(u16, u16)>) -> Vec<(u16, u16)> {
    let mut out: Vec<(u16, u16)> = Vec::new();
    let mut ia = a.into_iter().peekable();
    let mut ib = b.into_iter().peekable();
    loop {
        let take_a = match (ia.peek(), ib.peek()) {
            (Some(x), Some(y)) => x.0 <= y.0,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => break,
        };
        let next = if take_a { ia.next() } else { ib.next() };
        if let Some((lo, hi)) = next {
            push_fused(&mut out, lo, hi);
        }
    }
    out
}

/// Ajoute `[lo, hi)` à des plages triées par début, en réunissant une plage
/// qui touche ou recoupe la dernière.
fn push_fused(out: &mut Vec<(u16, u16)>, lo: u16, hi: u16) {
    match out.last_mut() {
        Some((_, end)) if *end >= lo => *end = (*end).max(hi),
        _ => out.push((lo, hi)),
    }
}

/// Plages en [`TimeRange`] ; `None` si une borne est invalide (jamais
/// attendu : bornes issues de plages valides).
fn to_time_ranges(ranges: &[(u16, u16)]) -> Option<Vec<TimeRange>> {
    ranges
        .iter()
        .map(|&(a, b)| TimeRange::new(TimeOfDay::new(a).ok()?, TimeOfDay::new(b).ok()?).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects,
        clippy::panic,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{LIMIT, Op, combine_bounded, intersect_ranges, union_ranges};
    use crate::arena::Arena;
    use crate::calendar::builtin::{standard, twenty_four_hours};
    use crate::calendar::compiled::CompiledCalendar;
    use crate::calendar::model::{Calendar, CalendarException, DaySetting, Recurrence};
    use crate::error::CalendarError;
    use crate::ids::CalendarUid;
    use crate::time::{Day, DayRange};

    fn compile(c: Calendar) -> CompiledCalendar {
        let u = c.uid();
        CompiledCalendar::compile(&Arena::try_from(vec![c]).unwrap(), u).unwrap()
    }

    /// Standard avec un férié le mer 21/10/2026 : 3 segments.
    fn standard_with_holiday() -> CompiledCalendar {
        let mut c = standard(CalendarUid::new(1), "Standard".to_owned());
        c.add_exception(CalendarException {
            name: String::new(),
            period: DayRange::single("2026-10-21".parse::<Day>().unwrap()),
            setting: DaySetting::NonWorking,
            recurrence: Recurrence::Period,
            occurrences: None,
        })
        .unwrap();
        compile(c)
    }

    #[test]
    fn r172_segment_limit_error() {
        let a = standard_with_holiday();
        let b = compile(twenty_four_hours(CalendarUid::new(2), "24 heures".to_owned()));
        assert_eq!(a.segments.len(), 3);
        // Plafond de 2 segments : les résultats en ont 3 (l'union avec
        // 24 heures n'en aurait qu'un, le férié y est ouvré).
        for (other, op) in [(&b, Op::Intersect), (&a, Op::Union)] {
            assert_eq!(
                combine_bounded(&a, other, op, 2),
                Err(CalendarError::TooManySegments { limit: 2 })
            );
        }
        assert_eq!(combine_bounded(&a, &b, Op::Union, 2).unwrap().segments.len(), 1);
        // Plafond de 1 motif : le jour chômé occupe déjà la place.
        assert_eq!(
            combine_bounded(&a, &b, Op::Intersect, 1),
            Err(CalendarError::TooManySegments { limit: 1 })
        );
        // Plafond réel (2²⁰) : accepté, 3 segments.
        let c = combine_bounded(&a, &b, Op::Intersect, LIMIT).unwrap();
        assert_eq!(c.segments.len(), 3);
        assert_eq!(c.uid(), CalendarUid::new(1));
    }

    #[test]
    fn crossed_ranges() {
        let s = vec![(480, 720), (780, 1_020)];
        let l = vec![(540, 1_080)];
        assert_eq!(intersect_ranges(s.clone(), l.clone()), [(540, 720), (780, 1_020)]);
        assert_eq!(union_ranges(s.clone(), l), [(480, 1_080)]);
        // Plages contiguës réunies par l'union.
        assert_eq!(union_ranges(vec![(0, 60)], vec![(60, 120)]), [(0, 120)]);
        // Recouvrement nul : rien en commun.
        assert_eq!(intersect_ranges(vec![(0, 60)], vec![(60, 120)]), []);
        assert_eq!(intersect_ranges(s.clone(), Vec::new()), []);
        assert_eq!(union_ranges(Vec::new(), s.clone()), s);
    }
}
