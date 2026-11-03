//! Forme compilée d'un calendrier et conversions ouvrées (ADR-0008, § 6.4 et
//! 6.5, règles R-020 à R-029).
//!
//! Définition normative : pour un calendrier compilé, **W(t)** est le nombre
//! de minutes ouvrées dans `[Moment::MIN, t)`. W est croissante au sens large,
//! de pente 1 dans les plages et 0 ailleurs ; toutes les conversions en
//! dérivent. Fusionner deux plages contiguës, y compris de part et d'autre de
//! minuit, ne change pas W.
//!
//! Structure : partition triée de `[jour 0, jour 60 631)` en segments. Un
//! segment `Weekly` répète un motif de semaine (semaine par défaut ou semaine
//! datée résolue) ; un segment `Daily` répète un motif de jour (série de jours
//! d'exception). Chaque segment porte le cumul W à son premier jour. Les motifs
//! de jour et de semaine sont internés. Aucune fenêtre, aucun cache paresseux,
//! aucune mutabilité intérieure : la structure est immuable, `Send + Sync` et
//! déterministe ; elle est recompilée à chaque recalcul.
//!
//! Coût d'une conversion, sans allocation : le segment d'un jour par un index
//! de blocs de 64 jours (calculé à la compilation, immuable : le premier
//! segment de chaque bloc) puis une dichotomie sur les seuls segments du
//! bloc ; le segment d'un cumul par une dichotomie, ou par une recherche
//! galopante depuis le segment d'un instant déjà converti (placement d'une
//! durée : la cible est presque toujours dans le même segment ou les
//! suivants) ; une dichotomie sur au plus 7 jours, une sur les plages du
//! jour (au plus 5 pour un calendrier stocké, davantage pour un calendrier
//! combiné par [`CompiledCalendar::intersect`] ou
//! [`CompiledCalendar::union`], R-172).
//!
//! Dans un [`CalendarSet`], un calendrier dérivé sans semaine propre est
//! compilé par surcharge de sa base compilée (ses seules exceptions posées
//! sur les segments de la base, J6 lot L9) : même fonction W que la
//! compilation complète, sans rejouer les exceptions de la base. La
//! compilation de l'ensemble est bornée par un budget de segments
//! ([`CalendarSet::compile_with`], R-172 amendée).

use std::collections::{BTreeMap, BTreeSet};

use crate::arena::Arena;
use crate::calendar::Direction;
use crate::calendar::combine::{Op, combine};
use crate::calendar::model::{Calendar, CalendarKind, DaySetting, TimeRange};
use crate::calendar::recurrence::{DayClaims, expand_exception};
use crate::calendar::resolve::{
    CalendarChain, chain, defined, exceptions_in_order, work_week_owner, work_weeks_in_order,
};
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::duration::Minutes;
use crate::error::CalendarError;
use crate::ids::CalendarUid;
use crate::model::Project;
use crate::time::{Day, Moment, TimeOfDay, Week, Weekday};

/// Minutes par jour civil (R-005).
const MINUTES_PER_DAY: i64 = 1_440;
/// Nombre de jours du domaine (1984-01-01 à 2149-12-31).
pub(super) const DAY_COUNT: i32 = 60_631;
/// Jours par bloc de l'index des segments (2⁶).
const BLOCK_SHIFT: i32 = 6;
/// En deçà de ce nombre de segments, une dichotomie directe est plus
/// rapide que l'index de blocs ou la recherche galopante.
const FEW_SEGMENTS: usize = 16;

/// Segments compilés gardés par [`CalendarSet::compile`], par défaut
/// (`Limits::compiled_segments`, R-172 amendée, règle 10) : environ 64 Mo
/// de segments (32 octets chacun), soit 33 calendriers au morcellement
/// maximal (un segment par jour du domaine) ou 9 000 calendriers Standard à
/// 100 fériés (environ 220 segments).
pub(crate) const DEFAULT_COMPILED_SEGMENTS: usize = 2_000_000;

/// Travail de compilation permis par segment gardé : séries d'exception
/// dépliées et points de coupure, tous calendriers confondus.
const COMPILE_WORK_FACTOR: usize = 8;

/// Place laissée à une compilation par le budget de [`CalendarSet`] :
/// segments gardés et travail (séries dépliées, points de coupure).
#[derive(Clone, Copy, Debug)]
struct Room {
    segments: usize,
    work: usize,
}

impl Room {
    /// Aucune borne ([`CompiledCalendar::compile`] seul).
    const UNBOUNDED: Self = Self {
        segments: usize::MAX,
        work: usize::MAX,
    };
}

/// Issue d'une compilation bornée.
enum Built {
    /// Calendrier compilé et travail consommé.
    Done(CompiledCalendar, usize),
    /// Budget dépassé : compilation abandonnée.
    Over,
}

/// Motif d'un jour : plages fusionnées (contiguës réunies), cumul avant
/// chaque plage, total.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DayPattern {
    pub(super) ranges: Vec<TimeRange>,
    cum: Vec<i64>,
    total: i64,
}

impl DayPattern {
    /// Motif d'un réglage de jour, plages contiguës fusionnées (R-011).
    fn new(setting: &DaySetting) -> Self {
        let mut merged: Vec<(u16, u16)> = Vec::new();
        if let DaySetting::Working(h) = setting {
            for r in h.ranges() {
                let (from, to) = (r.from().minutes(), r.to().minutes());
                match merged.last_mut() {
                    Some((_, end)) if *end >= from => *end = (*end).max(to),
                    _ => merged.push((from, to)),
                }
            }
        }
        let ranges: Vec<TimeRange> = merged
            .into_iter()
            .filter_map(|(a, b)| TimeRange::new(TimeOfDay::new(a).ok()?, TimeOfDay::new(b).ok()?).ok())
            .collect();
        Self::from_ranges(ranges)
    }

    /// Motif de plages déjà triées, disjointes et non contiguës (forme
    /// fusionnée) : cumuls et total.
    pub(super) fn from_ranges(ranges: Vec<TimeRange>) -> Self {
        let mut cum = Vec::with_capacity(ranges.len());
        let mut total: i64 = 0;
        for r in &ranges {
            cum.push(total);
            // Au plus 1 440 minutes par jour : jamais de débordement.
            total = total.saturating_add(r.len().get());
        }
        Self { ranges, cum, total }
    }

    /// Minutes ouvrées du motif avant la minute `m` du jour (`0..=1440`).
    #[inline]
    fn within(&self, m: i64) -> Option<i64> {
        let count = self.ranges.partition_point(|r| i64::from(r.from().minutes()) < m);
        let Some(j) = count.checked_sub(1) else {
            return Some(0);
        };
        let r = self.ranges.get(j)?;
        let before = *self.cum.get(j)?;
        let into = m.checked_sub(i64::from(r.from().minutes()))?;
        before.checked_add(into.min(r.len().get()))
    }

    /// Minute du jour qui termine la `w`-ième minute ouvrée du motif
    /// (`1 <= w <= total`).
    #[inline]
    fn locate(&self, w: i64) -> Option<i64> {
        let j = self.cum.partition_point(|c| *c < w).checked_sub(1)?;
        let r = self.ranges.get(j)?;
        let offset = w.checked_sub(*self.cum.get(j)?)?;
        if offset < 1 || offset > r.len().get() {
            return None;
        }
        i64::from(r.from().minutes()).checked_add(offset)
    }
}

/// Motif de semaine : motif de jour de chaque jour de semaine, cumul avant
/// chaque jour (`prefix[k]` = minutes des jours `0..k`, dimanche = 0), total.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct WeekPattern {
    pub(super) days: Week<u32>,
    prefix: [i64; 8],
    total: i64,
}

impl WeekPattern {
    fn new(days: Week<u32>, patterns: &[DayPattern]) -> Option<Self> {
        let mut prefix = [0_i64; 8];
        let mut acc: i64 = 0;
        for (k, wd) in Weekday::ALL.iter().enumerate() {
            let p = patterns.get(usize::try_from(*days.get(*wd)).ok()?)?;
            acc = acc.checked_add(p.total)?;
            *prefix.get_mut(k.checked_add(1)?)? = acc;
        }
        Some(Self {
            days,
            prefix,
            total: acc,
        })
    }

    /// Wp(d) : minutes du motif répété depuis le jour 0 jusqu'au jour `d`
    /// exclu. Le jour 0 est un dimanche.
    #[inline]
    fn cum_at(&self, day: i32) -> Option<i64> {
        let q = i64::from(day.checked_div_euclid(7)?);
        let k = usize::try_from(day.checked_rem_euclid(7)?).ok()?;
        q.checked_mul(self.total)?.checked_add(*self.prefix.get(k)?)
    }
}

/// Règle d'un segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum SegmentRule {
    /// Motif de semaine répété.
    Weekly(u32),
    /// Chaque jour identique (série de jours d'exception).
    Daily(u32),
}

/// Segment `[first_day, jour suivant le segment)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Segment {
    pub(super) first_day: i32,
    pub(super) rule: SegmentRule,
    /// W au début du segment.
    cum_start: i64,
    /// `Wp(first_day)` pour un segment `Weekly`, 0 sinon.
    wp_start: i64,
}

/// Calendrier compilé : W(t) en segments, conversions ouvrées.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledCalendar {
    uid: CalendarUid,
    /// Motifs de jour internés ; `day_patterns[0]` est le jour chômé.
    pub(super) day_patterns: Vec<DayPattern>,
    /// Motifs de semaine internés.
    pub(super) week_patterns: Vec<WeekPattern>,
    /// Partition triée de `[0, 60 631)` ; `segments[0].first_day == 0`.
    pub(super) segments: Vec<Segment>,
    /// Index des segments par bloc de 64 jours : `blocks[b]` = indice du
    /// segment qui contient le jour `b × 64` (fonction des segments, donc
    /// sans effet sur l'égalité).
    blocks: Vec<u32>,
    /// Empreinte des segments et du nombre de motifs (fonction des
    /// segments, donc sans effet sur l'égalité), calculée à l'assemblage :
    /// voir [`CompiledCalendar::shape_fingerprint`].
    fingerprint: u64,
    /// `W(Moment::MAX)`.
    total: i64,
}

/// Mélange déterministe d'un mot dans une empreinte (multiplication par une
/// constante impaire puis rotation, même résultat sur toutes les
/// plateformes).
const fn mix(h: u64, v: u64) -> u64 {
    (h ^ v).wrapping_mul(0x9e37_79b9_7f4a_7c15).rotate_left(29)
}

/// Motifs en cours d'internement.
pub(super) struct Patterns {
    pub(super) days: Vec<DayPattern>,
    day_index: BTreeMap<Vec<TimeRange>, u32>,
    pub(super) weeks: Vec<WeekPattern>,
    week_index: BTreeMap<[u32; 7], u32>,
}

impl Patterns {
    pub(super) fn new() -> Option<Self> {
        let mut p = Self {
            days: Vec::new(),
            day_index: BTreeMap::new(),
            weeks: Vec::new(),
            week_index: BTreeMap::new(),
        };
        // Motif n° 0 : jour chômé.
        p.day(&DaySetting::NonWorking)?;
        Some(p)
    }

    /// Motifs d'un calendrier compilé, index reconstruits (surcharge d'une
    /// base, [`CompiledCalendar::overlay`]) : les indices des motifs de la
    /// base sont gardés.
    fn from_compiled(c: &CompiledCalendar) -> Option<Self> {
        let mut day_index = BTreeMap::new();
        for (i, d) in c.day_patterns.iter().enumerate() {
            day_index.insert(d.ranges.clone(), u32::try_from(i).ok()?);
        }
        let mut week_index = BTreeMap::new();
        for (i, w) in c.week_patterns.iter().enumerate() {
            week_index.insert(Weekday::ALL.map(|wd| *w.days.get(wd)), u32::try_from(i).ok()?);
        }
        Some(Self {
            days: c.day_patterns.clone(),
            day_index,
            weeks: c.week_patterns.clone(),
            week_index,
        })
    }

    fn day(&mut self, setting: &DaySetting) -> Option<u32> {
        self.intern_day(DayPattern::new(setting))
    }

    /// Motif de jour de plages déjà fusionnées (voir
    /// [`DayPattern::from_ranges`]), interné.
    pub(super) fn day_of_ranges(&mut self, ranges: Vec<TimeRange>) -> Option<u32> {
        self.intern_day(DayPattern::from_ranges(ranges))
    }

    fn intern_day(&mut self, pattern: DayPattern) -> Option<u32> {
        if let Some(i) = self.day_index.get(&pattern.ranges) {
            return Some(*i);
        }
        let i = u32::try_from(self.days.len()).ok()?;
        self.day_index.insert(pattern.ranges.clone(), i);
        self.days.push(pattern);
        Some(i)
    }

    fn week(&mut self, settings: &Week<DaySetting>) -> Option<u32> {
        let mut days: Week<u32> = Week::default();
        for wd in Weekday::ALL {
            *days.get_mut(wd) = self.day(settings.get(wd))?;
        }
        self.week_of_days(days)
    }

    /// Motif de semaine de motifs de jour déjà internés, interné.
    pub(super) fn week_of_days(&mut self, days: Week<u32>) -> Option<u32> {
        let key = Weekday::ALL.map(|wd| *days.get(wd));
        if let Some(i) = self.week_index.get(&key) {
            return Some(*i);
        }
        let i = u32::try_from(self.weeks.len()).ok()?;
        self.weeks.push(WeekPattern::new(days, &self.days)?);
        self.week_index.insert(key, i);
        Some(i)
    }
}

/// Semaine résolue hors exceptions (R-016, étapes 3 à 6) quand la semaine
/// datée du dérivé est `derived_ww` et celle de la base `base_ww`.
fn resolved_week(
    derived: Option<&Calendar>,
    derived_ww: Option<usize>,
    base: &Calendar,
    base_ww: Option<usize>,
) -> Option<Week<DaySetting>> {
    let CalendarKind::Base { week: base_week } = base.kind() else {
        return None;
    };
    let day = |wd: Weekday| -> DaySetting {
        if let Some(d) = derived {
            if let Some(s) = derived_ww
                .and_then(|i| d.work_weeks().get(i))
                .and_then(|w| defined(w.days.get(wd)))
            {
                return s;
            }
            if let CalendarKind::Derived { week, .. } = d.kind()
                && let Some(s) = defined(week.get(wd))
            {
                return s;
            }
        }
        if let Some(s) = base_ww
            .and_then(|i| base.work_weeks().get(i))
            .and_then(|w| defined(w.days.get(wd)))
        {
            return s;
        }
        base_week.get(wd).clone()
    };
    Some(Week::from_fn(day))
}

/// Résultat d'une conversion, avec son erreur propre.
pub(super) type Converted = Result<Moment, CalendarError>;

/// W(t) et le segment de `t` ([`CompiledCalendar::probe`]).
#[derive(Clone, Copy, Debug)]
pub(super) struct Probe {
    v: i64,
    seg: usize,
}

/// Index des segments par bloc de `2^BLOCK_SHIFT` jours : pour chaque bloc,
/// l'indice du segment qui contient son premier jour ; `None` si un
/// invariant est violé.
fn block_index(segments: &[Segment]) -> Option<Vec<u32>> {
    let count = usize::try_from((DAY_COUNT >> BLOCK_SHIFT).checked_add(1)?).ok()?;
    let mut blocks = Vec::with_capacity(count);
    let mut i = 0usize;
    for b in 0..count {
        let day = i32::try_from(b)
            .ok()?
            .checked_shl(u32::try_from(BLOCK_SHIFT).ok()?)?;
        while segments
            .get(i.checked_add(1)?)
            .is_some_and(|s| s.first_day <= day)
        {
            i = i.checked_add(1)?;
        }
        blocks.push(u32::try_from(i).ok()?);
    }
    Some(blocks)
}

/// Jour de semaine du jour `day` (le jour 0 est un dimanche).
fn weekday_of(day: i32) -> Option<Weekday> {
    let k = usize::try_from(day.checked_rem_euclid(7)?).ok()?;
    Weekday::ALL.get(k).copied()
}

impl CompiledCalendar {
    /// Compile le calendrier `uid` (ADR-0008, § 6.4) :
    /// 1. chaîne d'héritage ;
    /// 2. exceptions dépliées, posées niveau par niveau (dérivé d'abord), dans
    ///    l'ordre (début, indice), chacune sur les jours encore libres ;
    /// 3. points de coupure : bornes du domaine, des séries d'exception et des
    ///    semaines datées ;
    /// 4. chaque intervalle élémentaire reçoit un motif de jour (exception) ou
    ///    de semaine (précédence R-016) ;
    /// 5. motifs internés, segments adjacents identiques fusionnés, cumuls.
    ///
    /// Un calendrier sans temps ouvré se compile ; ses conversions renvoient
    /// [`CalendarError::NoWorkingTime`].
    ///
    /// # Errors
    ///
    /// Erreurs de [`chain`] ; [`CalendarError::Inconsistent`] si un invariant
    /// interne est violé (jamais attendu).
    pub fn compile(
        calendars: &Arena<CalendarUid, Calendar>,
        uid: CalendarUid,
    ) -> Result<Self, CalendarError> {
        let ch = chain(calendars, uid)?;
        match Self::build(ch, Room::UNBOUNDED) {
            Some(Built::Done(c, _)) => Ok(c),
            Some(Built::Over) | None => Err(CalendarError::Inconsistent { calendar: uid }),
        }
    }

    /// Compilation d'une chaîne valide dans la place `room` : abandonnée
    /// ([`Built::Over`]) dès que le travail (séries d'exception dépliées,
    /// points de coupure) ou le nombre de segments la dépasse, avant toute
    /// allocation proportionnelle au dépassement ; `None` si un invariant
    /// est violé.
    fn build(ch: CalendarChain<'_>, room: Room) -> Option<Built> {
        let uid = ch.calendar().uid();
        let derived = match ch {
            CalendarChain::Base(_) => None,
            CalendarChain::Derived { calendar, .. } => Some(calendar),
        };
        let base = ch.base();
        let levels = || derived.into_iter().chain([base]);
        let mut patterns = Patterns::new()?;

        // 2. Exceptions : dérivé d'abord, puis base, chacune sur les jours libres.
        let mut claims: DayClaims<u32> = DayClaims::new();
        let mut work: usize = 0;
        for c in levels() {
            for (_, e) in exceptions_in_order(c) {
                // Place épuisée : abandon avant de déplier (une exception se
                // déplie en au plus une série par jour du domaine, le
                // dépassement est donc borné).
                if work >= room.work {
                    return Some(Built::Over);
                }
                let p = patterns.day(&e.setting)?;
                // Une récurrence invalide ne peut pas être stockée.
                let runs = expand_exception(e).ok()?;
                work = work.saturating_add(runs.len().max(1));
                if work > room.work {
                    return Some(Built::Over);
                }
                for r in runs {
                    claims.claim(r.first().index(), r.last().index(), p);
                }
            }
        }

        // 3. Points de coupure.
        let mut cuts: BTreeSet<i32> = BTreeSet::from([0, DAY_COUNT]);
        for (first, last, _) in claims.iter() {
            cuts.insert(first);
            cuts.insert(last.saturating_add(1));
        }
        for c in levels() {
            for w in c.work_weeks() {
                cuts.insert(w.period.first().index());
                cuts.insert(w.period.last().index().saturating_add(1));
            }
        }
        let cuts: Vec<i32> = cuts.into_iter().collect();
        work = work.saturating_add(cuts.len());
        if work > room.work {
            return Some(Built::Over);
        }

        // 4. Règle de chaque intervalle élémentaire, fusion des voisins égaux.
        let derived_ww = derived.map(work_weeks_in_order).unwrap_or_default();
        let base_ww = work_weeks_in_order(base);
        let mut week_cache: BTreeMap<(Option<usize>, Option<usize>), u32> = BTreeMap::new();
        let mut raw: Vec<(i32, SegmentRule)> = Vec::new();
        for pair in cuts.windows(2) {
            let &[first, _] = pair else {
                return None;
            };
            let rule = if let Some(p) = claims.owner(first) {
                SegmentRule::Daily(p)
            } else {
                let key = (
                    work_week_owner(&derived_ww, first),
                    work_week_owner(&base_ww, first),
                );
                let w = if let Some(w) = week_cache.get(&key) {
                    *w
                } else {
                    let w = patterns.week(&resolved_week(derived, key.0, base, key.1)?)?;
                    week_cache.insert(key, w);
                    w
                };
                SegmentRule::Weekly(w)
            };
            if raw.last().map(|(_, r)| *r) != Some(rule) {
                if raw.len() >= room.segments {
                    return Some(Built::Over);
                }
                raw.push((first, rule));
            }
        }

        Some(Built::Done(Self::assemble(uid, patterns, &raw)?, work))
    }

    /// Vrai si le calendrier dérivé `c` se compile par surcharge de sa base
    /// compilée ([`Self::overlay`]) : aucun jour de sa semaine par défaut
    /// n'est défini et il n'a aucune semaine datée ; seules ses exceptions
    /// le distinguent de sa base.
    fn overlays_base(c: &Calendar) -> bool {
        match c.kind() {
            CalendarKind::Derived { week, .. } => {
                c.work_weeks().is_empty() && Weekday::ALL.iter().all(|wd| defined(week.get(*wd)).is_none())
            }
            CalendarKind::Base { .. } => false,
        }
    }

    /// Compilation d'un dérivé sans semaine propre (voir
    /// [`Self::overlays_base`]) par surcharge de sa base compilée `base`
    /// (J6, lot L9, dette de l'ADR-0013) : ses exceptions, dépliées et posées
    /// dans l'ordre (R-016, étape 2, le dérivé d'abord), remplacent les jours
    /// de la base qu'elles couvrent ; les autres jours gardent la règle de
    /// la base (exception de la base ou semaine résolue, identiques à la
    /// précédence R-016 puisque le dérivé n'a ni semaine ni semaine datée).
    /// Même fonction W que [`Self::compile`] (motifs internés dans un autre
    /// ordre : formes compilées différentes, conversions identiques) ;
    /// coût linéaire en segments de la base plus les séries du dérivé, sans
    /// rejouer les exceptions de la base. `None` si un invariant est violé.
    fn overlay(base: &Self, derived: &Calendar, room: Room) -> Option<Built> {
        let mut patterns = Patterns::from_compiled(base)?;
        let mut claims: DayClaims<u32> = DayClaims::new();
        let mut work: usize = 0;
        for (_, e) in exceptions_in_order(derived) {
            if work >= room.work {
                return Some(Built::Over);
            }
            let p = patterns.day(&e.setting)?;
            let runs = expand_exception(e).ok()?;
            work = work.saturating_add(runs.len().max(1));
            if work > room.work {
                return Some(Built::Over);
            }
            for r in runs {
                claims.claim(r.first().index(), r.last().index(), p);
            }
        }
        work = work.saturating_add(base.segments.len());
        if work > room.work {
            return Some(Built::Over);
        }
        // Balayage : séries du dérivé, sinon segments de la base ; voisins
        // de même règle fusionnés.
        let mut raw: Vec<(i32, SegmentRule)> = Vec::with_capacity(base.segments.len());
        let push = |raw: &mut Vec<(i32, SegmentRule)>, first: i32, rule: SegmentRule| -> bool {
            if raw.last().map(|(_, r)| *r) == Some(rule) {
                return true;
            }
            if raw.len() >= room.segments {
                return false;
            }
            raw.push((first, rule));
            true
        };
        let mut runs = claims.iter().peekable();
        let mut si = 0usize;
        let mut day = 0i32;
        while day < DAY_COUNT {
            if let Some(&(first, last, p)) = runs.peek()
                && first <= day
            {
                if !push(&mut raw, day, SegmentRule::Daily(p)) {
                    return Some(Built::Over);
                }
                day = last.checked_add(1)?;
                runs.next();
                continue;
            }
            while base
                .segments
                .get(si.checked_add(1)?)
                .is_some_and(|s| s.first_day <= day)
            {
                si = si.checked_add(1)?;
            }
            let seg = base.segments.get(si)?;
            let seg_end = base
                .segments
                .get(si.checked_add(1)?)
                .map_or(DAY_COUNT, |s| s.first_day);
            let next_run = runs.peek().map_or(DAY_COUNT, |r| r.0);
            if !push(&mut raw, day, seg.rule) {
                return Some(Built::Over);
            }
            let next = seg_end.min(next_run);
            if next <= day {
                return None;
            }
            day = next;
        }
        Some(Built::Done(Self::assemble(derived.uid(), patterns, &raw)?, work))
    }

    /// Étape 5 de la compilation, partagée avec la combinaison (R-172) :
    /// cumuls d'une partition `raw` de `[0, 60 631)` (premier jour et règle
    /// de chaque segment, voisins distincts) ; `None` si un invariant est
    /// violé.
    pub(super) fn assemble(uid: CalendarUid, patterns: Patterns, raw: &[(i32, SegmentRule)]) -> Option<Self> {
        let mut segments = Vec::with_capacity(raw.len());
        let mut cum: i64 = 0;
        for (i, (first_day, rule)) in raw.iter().enumerate() {
            let end = match raw.get(i.checked_add(1)?) {
                Some((f, _)) => *f,
                None => DAY_COUNT,
            };
            let (work, wp_start) = match *rule {
                SegmentRule::Weekly(w) => {
                    let wp = patterns.weeks.get(usize::try_from(w).ok()?)?;
                    let start = wp.cum_at(*first_day)?;
                    (wp.cum_at(end)?.checked_sub(start)?, start)
                }
                SegmentRule::Daily(p) => {
                    let dp = patterns.days.get(usize::try_from(p).ok()?)?;
                    let n = i64::from(end.checked_sub(*first_day)?);
                    (n.checked_mul(dp.total)?, 0)
                }
            };
            segments.push(Segment {
                first_day: *first_day,
                rule: *rule,
                cum_start: cum,
                wp_start,
            });
            cum = cum.checked_add(work)?;
        }
        let blocks = block_index(&segments)?;
        let mut fingerprint = mix(
            u64::try_from(patterns.days.len()).ok()?,
            u64::try_from(patterns.weeks.len()).ok()?,
        );
        for seg in &segments {
            let rule = match seg.rule {
                SegmentRule::Weekly(i) => u64::from(i) << 1,
                SegmentRule::Daily(i) => (u64::from(i) << 1) | 1,
            };
            // Bits recopiés (complément à deux), déterministe.
            let first = u64::from_le_bytes(i64::from(seg.first_day).to_le_bytes());
            let cum = u64::from_le_bytes(seg.cum_start.to_le_bytes());
            fingerprint = mix(mix(mix(fingerprint, first), rule), cum);
        }
        Some(Self {
            uid,
            day_patterns: patterns.days,
            week_patterns: patterns.weeks,
            segments,
            blocks,
            fingerprint,
            total: cum,
        })
    }

    /// UID du calendrier compilé.
    #[must_use]
    pub const fn uid(&self) -> CalendarUid {
        self.uid
    }

    /// Vrai si `self` et `other` ont la même forme compilée, UID mis à part
    /// (mêmes motifs, mêmes segments) : ils ont alors la même fonction W et
    /// donnent les mêmes conversions. Condition suffisante, non nécessaire
    /// (deux formes différentes peuvent décrire le même temps ouvré).
    #[must_use]
    pub fn same_working_time(&self, other: &Self) -> bool {
        self.total == other.total
            && self.segments == other.segments
            && self.week_patterns == other.week_patterns
            && self.day_patterns == other.day_patterns
    }

    /// Nombre de segments de la forme compilée (coût d'une combinaison,
    /// R-172 : linéaire en segments des deux calendriers ; budget du
    /// recalcul, `Limits::combined_segments`).
    #[must_use]
    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    /// Empreinte de la forme compilée, UID mis à part : égale pour deux
    /// calendriers de même forme ([`Self::same_working_time`]), elle range
    /// les candidats avant la comparaison complète (placement, une échelle
    /// par forme). Déterministe, calculée à l'assemblage : (total, nombre
    /// de segments, mélange des segments et du nombre de motifs).
    #[must_use]
    pub const fn shape_fingerprint(&self) -> (i64, usize, u64) {
        (self.total, self.segments.len(), self.fingerprint)
    }

    /// Vrai si le calendrier a au moins une minute ouvrée sur le domaine.
    #[must_use]
    pub const fn has_working_time(&self) -> bool {
        self.total > 0
    }

    /// Minutes ouvrées sur tout le domaine : W(`Moment::MAX`).
    #[must_use]
    pub const fn total_working(&self) -> Minutes {
        Minutes::new(self.total)
    }

    /// R-172 : intersection, ouvrée à l'instant t si et seulement si `self`
    /// et `other` le sont (calendrier effectif d'une affectation, R-171).
    /// Le résultat porte l'UID de `self`. Coût linéaire en nombre de
    /// segments des deux calendriers (module `combine`).
    ///
    /// # Errors
    ///
    /// [`CalendarError::TooManySegments`] si le résultat dépasse le plafond
    /// de segments ou de motifs ; [`CalendarError::Inconsistent`] si un
    /// invariant interne est violé (jamais attendu).
    pub fn intersect(&self, other: &Self) -> Result<Self, CalendarError> {
        combine(self, other, Op::Intersect)
    }

    /// R-172 : union, ouvrée à l'instant t si et seulement si `self` ou
    /// `other` l'est (mesure de la durée d'une feuille affectée, R-173). Le
    /// résultat porte l'UID de `self`.
    ///
    /// # Errors
    ///
    /// Comme [`CompiledCalendar::intersect`].
    pub fn union(&self, other: &Self) -> Result<Self, CalendarError> {
        combine(self, other, Op::Union)
    }

    /// Erreur d'invariant interne.
    pub(super) const fn inconsistent(&self) -> CalendarError {
        CalendarError::Inconsistent { calendar: self.uid }
    }

    /// Indice du segment qui contient le jour `day` : bloc de `day`, puis
    /// dichotomie entre le segment du début du bloc et celui du bloc suivant
    /// (inclus).
    #[inline]
    fn segment_index(&self, day: i32) -> Option<usize> {
        if self.segments.len() <= FEW_SEGMENTS {
            return self
                .segments
                .partition_point(|s| s.first_day <= day)
                .checked_sub(1);
        }
        if day < 0 {
            return None;
        }
        let b = usize::try_from(day >> BLOCK_SHIFT).ok()?;
        let lo = usize::try_from(*self.blocks.get(b)?).ok()?;
        let hi = match self.blocks.get(b.checked_add(1)?) {
            Some(&next) => usize::try_from(next).ok()?.checked_add(1)?,
            None => self.segments.len(),
        };
        let window = self.segments.get(lo..hi.min(self.segments.len()))?;
        lo.checked_add(window.partition_point(|s| s.first_day <= day).checked_sub(1)?)
    }

    /// Segment qui contient le jour `day`.
    fn segment(&self, day: i32) -> Option<&Segment> {
        self.segments.get(self.segment_index(day)?)
    }

    fn day_pattern(&self, i: u32) -> Option<&DayPattern> {
        self.day_patterns.get(usize::try_from(i).ok()?)
    }

    fn week_pattern(&self, i: u32) -> Option<&WeekPattern> {
        self.week_patterns.get(usize::try_from(i).ok()?)
    }

    /// Motif du jour `day` (dans un segment donné).
    #[inline]
    fn pattern_of_day(&self, seg: &Segment, day: i32) -> Option<&DayPattern> {
        match seg.rule {
            SegmentRule::Daily(p) => self.day_pattern(p),
            SegmentRule::Weekly(w) => self.day_pattern(*self.week_pattern(w)?.days.get(weekday_of(day)?)),
        }
    }

    /// W(t), en minutes depuis 1984-01-01 00:00 (`0..=MAX`).
    fn cumulative(&self, t: i64) -> Option<i64> {
        self.cumulative_at(t).map(|(w, _)| w)
    }

    /// W(t) et l'indice du segment de `t` (le dernier pour `Moment::MAX`),
    /// point de départ d'une recherche galopante ([`Self::reach_from`]).
    fn cumulative_at(&self, t: i64) -> Option<(i64, usize)> {
        if t >= Moment::MAX.minutes() {
            return Some((self.total, self.segments.len().checked_sub(1)?));
        }
        let day = i32::try_from(t.checked_div_euclid(MINUTES_PER_DAY)?).ok()?;
        let minute = t.checked_rem_euclid(MINUTES_PER_DAY)?;
        let index = self.segment_index(day)?;
        let seg = self.segments.get(index)?;
        let dp = self.pattern_of_day(seg, day)?;
        let before_day = match seg.rule {
            SegmentRule::Weekly(w) => {
                let wp = self.week_pattern(w)?;
                wp.cum_at(day)?.checked_sub(seg.wp_start)?
            }
            SegmentRule::Daily(_) => i64::from(day.checked_sub(seg.first_day)?).checked_mul(dp.total)?,
        };
        let w = seg
            .cum_start
            .checked_add(before_day)?
            .checked_add(dp.within(minute)?);
        Some((w?, index))
    }

    /// Plus petit t tel que W(t) >= v (`1 <= v <= total`) : la fin de la
    /// v-ième minute ouvrée.
    fn first_reach(&self, target: i64) -> Option<i64> {
        if target < 1 || target > self.total {
            return None;
        }
        // Dernier segment dont le cumul de début est < target : il contient
        // la minute visée (les segments sans travail sont sautés).
        let index = self
            .segments
            .partition_point(|s| s.cum_start < target)
            .checked_sub(1)?;
        self.first_reach_in(target, index)
    }

    /// [`Self::first_reach`] par une recherche galopante depuis le segment
    /// `hint` (même résultat : dernier segment dont le cumul de début est
    /// < target, les cumuls étant croissants au sens large) ; rend aussi
    /// l'indice du segment trouvé.
    fn first_reach_near(&self, target: i64, hint: usize) -> Option<(i64, usize)> {
        if target < 1 || target > self.total {
            return None;
        }
        let segs = &self.segments;
        if segs.len() <= FEW_SEGMENTS {
            let index = segs.partition_point(|s| s.cum_start < target).checked_sub(1)?;
            return Some((self.first_reach_in(target, index)?, index));
        }
        let below = |i: usize| segs.get(i).is_some_and(|s| s.cum_start < target);
        let hint = hint.min(segs.len().checked_sub(1)?);
        // Fenêtre [lo, hi) : below(lo) vrai, below(hi) faux (ou hi = len).
        let (lo, hi) = if below(hint) {
            let mut lo = hint;
            let mut step = 1usize;
            loop {
                let probe = lo.saturating_add(step);
                if probe >= segs.len() || !below(probe) {
                    break (lo, probe.min(segs.len()));
                }
                lo = probe;
                step = step.saturating_mul(2);
            }
        } else {
            let mut hi = hint;
            let mut step = 1usize;
            loop {
                // `segments[0].cum_start == 0 < target` : la descente
                // s'arrête au plus tard en 0.
                let probe = hi.saturating_sub(step);
                if below(probe) {
                    break (probe, hi);
                }
                if probe == 0 {
                    return None;
                }
                hi = probe;
                step = step.saturating_mul(2);
            }
        };
        let window = segs.get(lo..hi)?;
        let index = lo.checked_add(window.partition_point(|s| s.cum_start < target).checked_sub(1)?)?;
        Some((self.first_reach_in(target, index)?, index))
    }

    /// Fin de la `target`-ième minute ouvrée, dans le segment `index` qui la
    /// contient.
    fn first_reach_in(&self, target: i64, index: usize) -> Option<i64> {
        let seg = self.segments.get(index)?;
        let rest = target.checked_sub(seg.cum_start)?;
        let (day, within, pattern) = match seg.rule {
            SegmentRule::Weekly(w) => {
                let week = self.week_pattern(w)?;
                // Rang de la minute dans le motif répété depuis le jour 0.
                let rank = seg.wp_start.checked_add(rest)?;
                let weeks = rank.checked_sub(1)?.checked_div_euclid(week.total)?;
                let in_week = rank.checked_sub(weeks.checked_mul(week.total)?)?;
                let k = week.prefix.get(1..)?.partition_point(|p| *p < in_week);
                let within = in_week.checked_sub(*week.prefix.get(k)?)?;
                let weekday = *Weekday::ALL.get(k)?;
                let day = weeks.checked_mul(7)?.checked_add(i64::try_from(k).ok()?)?;
                (day, within, self.day_pattern(*week.days.get(weekday))?)
            }
            SegmentRule::Daily(p) => {
                let pattern = self.day_pattern(p)?;
                let days = rest.checked_sub(1)?.checked_div_euclid(pattern.total)?;
                let within = rest.checked_sub(days.checked_mul(pattern.total)?)?;
                (i64::from(seg.first_day).checked_add(days)?, within, pattern)
            }
        };
        if day >= i64::from(DAY_COUNT) {
            return None;
        }
        day.checked_mul(MINUTES_PER_DAY)?
            .checked_add(pattern.locate(within)?)
    }

    /// W(t) avec l'erreur d'invariant.
    fn w(&self, t: Moment) -> Result<i64, CalendarError> {
        self.cumulative(t.minutes()).ok_or_else(|| self.inconsistent())
    }

    /// Instant `first_reach(v) + shift`.
    fn reach(&self, v: i64, shift: i64) -> Result<Moment, CalendarError> {
        self.first_reach(v)
            .and_then(|m| m.checked_add(shift))
            .and_then(|m| Moment::from_minutes(m).ok())
            .ok_or_else(|| self.inconsistent())
    }

    /// [`Self::reach`] depuis le segment `hint` ; rend aussi le segment
    /// trouvé.
    fn reach_from(&self, v: i64, shift: i64, hint: usize) -> Result<(Moment, usize), CalendarError> {
        self.first_reach_near(v, hint)
            .and_then(|(m, i)| Some((Moment::from_minutes(m.checked_add(shift)?).ok()?, i)))
            .ok_or_else(|| self.inconsistent())
    }

    /// W(t) et le segment de `t`, avec l'erreur d'invariant.
    fn w_at(&self, t: Moment) -> Result<(i64, usize), CalendarError> {
        self.cumulative_at(t.minutes()).ok_or_else(|| self.inconsistent())
    }

    /// R-030 (passe avant d'une durée `d > 0`, `TimeScale::forward_span`) :
    /// `start = next_working_moment(from)`, `finish = add_working(start,
    /// d)`, mêmes résultats et mêmes erreurs, en une seule évaluation de W :
    /// W(start) = W(from), aucune minute ouvrée ne les séparant.
    pub(super) fn span_after(&self, from: Moment, d: i64) -> Result<(Moment, Moment), CalendarError> {
        if self.total == 0 {
            return Err(self.no_working_time());
        }
        let (v, seg) = self.w_at(from)?;
        if v >= self.total {
            return Err(self.out_of_domain(Direction::After));
        }
        let (start, seg) = self.reach_from(v.saturating_add(1), -1, seg)?;
        let finish = match v.checked_add(d) {
            Some(target) if target <= self.total => self.reach_from(target, 0, seg)?.0,
            _ => return Err(self.out_of_domain(Direction::After)),
        };
        Ok((start, finish))
    }

    /// R-030 (passe arrière d'une durée `d > 0`,
    /// `TimeScale::backward_span`) : `finish = previous_working_moment(to)`,
    /// `start = add_working(finish, −d)`, mêmes résultats et mêmes erreurs,
    /// en une seule évaluation de W : W(finish) = W(to).
    pub(super) fn span_before(&self, to: Moment, d: i64) -> Result<(Moment, Moment), CalendarError> {
        if self.total == 0 {
            return Err(self.no_working_time());
        }
        let (v, seg) = self.w_at(to)?;
        if v == 0 {
            return Err(self.out_of_domain(Direction::Before));
        }
        let (finish, seg) = self.reach_from(v, 0, seg)?;
        if d > v {
            return Err(self.out_of_domain(Direction::Before));
        }
        let target = v
            .checked_sub(d)
            .and_then(|x| x.checked_add(1))
            .ok_or_else(|| self.inconsistent())?;
        let start = self.reach_from(target, -1, seg)?.0;
        Ok((start, finish))
    }

    /// W(t) et le segment de `t`, pour [`Self::next_of`] et
    /// [`Self::previous_of`] : instant ouvré suivant et précédent de `t`
    /// (R-024, R-025) sur une seule évaluation de W.
    pub(super) fn probe(&self, t: Moment) -> Result<Probe, CalendarError> {
        let (v, seg) = self.w_at(t)?;
        Ok(Probe { v, seg })
    }

    /// `next_working_moment(t)` depuis la sonde de `t` (mêmes erreurs).
    pub(super) fn next_of(&self, p: Probe) -> Converted {
        if self.total == 0 {
            return Err(self.no_working_time());
        }
        if p.v >= self.total {
            return Err(self.out_of_domain(Direction::After));
        }
        self.reach_from(p.v.saturating_add(1), -1, p.seg).map(|(m, _)| m)
    }

    /// `previous_working_moment(t)` depuis la sonde de `t` (mêmes erreurs).
    pub(super) fn previous_of(&self, p: Probe) -> Converted {
        if self.total == 0 {
            return Err(self.no_working_time());
        }
        if p.v == 0 {
            return Err(self.out_of_domain(Direction::Before));
        }
        self.reach_from(p.v, 0, p.seg).map(|(m, _)| m)
    }

    /// Erreur « aucun temps ouvré ».
    const fn no_working_time(&self) -> CalendarError {
        CalendarError::NoWorkingTime { calendar: self.uid }
    }

    /// Erreur « hors du domaine ».
    const fn out_of_domain(&self, direction: Direction) -> CalendarError {
        CalendarError::OutOfDomain {
            calendar: self.uid,
            direction,
        }
    }

    /// Vrai si la minute `[t, t + 1)` est ouvrée.
    ///
    /// # Errors
    ///
    /// [`CalendarError::Inconsistent`] (jamais attendu).
    pub fn is_working(&self, t: Moment) -> Result<bool, CalendarError> {
        if t >= Moment::MAX {
            return Ok(false);
        }
        let next = Moment::from_minutes(t.minutes().saturating_add(1)).map_err(|_| self.inconsistent())?;
        Ok(self.w(next)? > self.w(t)?)
    }

    /// Plages fusionnées du jour `day` (plages contiguës réunies).
    ///
    /// # Errors
    ///
    /// [`CalendarError::Inconsistent`] (jamais attendu).
    pub fn day_ranges(&self, day: Day) -> Result<&[TimeRange], CalendarError> {
        let d = day.index();
        self.segment(d)
            .and_then(|seg| self.pattern_of_day(seg, d))
            .map(|p| p.ranges.as_slice())
            .ok_or_else(|| self.inconsistent())
    }

    /// R-020 : minutes ouvrées entre `a` et `b`, W(b) − W(a) ; signé
    /// (négatif si `b` précède `a`), antisymétrique et additif.
    ///
    /// # Errors
    ///
    /// [`CalendarError::Inconsistent`] (jamais attendu).
    pub fn working_between(&self, a: Moment, b: Moment) -> Result<Minutes, CalendarError> {
        let diff = self
            .w(b)?
            .checked_sub(self.w(a)?)
            .ok_or_else(|| self.inconsistent())?;
        Ok(Minutes::new(diff))
    }

    /// R-021 à R-023 : instant à `d` minutes ouvrées de `t`, avec v = W(t) :
    /// - `d = 0` : `t`, sans recalage (R-023) ;
    /// - `d > 0` : plus petit u tel que W(u) = v + d ; il tombe sur une fin de
    ///   minute ouvrée, jamais au début de la plage suivante (R-021) ;
    /// - `d < 0` : plus grand s tel que W(s) = v − |d| ; il tombe sur un début
    ///   de minute ouvrée (R-022).
    ///
    /// # Errors
    ///
    /// [`CalendarError::NoWorkingTime`] si le calendrier n'a aucune minute
    /// ouvrée (et `d ≠ 0`) ; [`CalendarError::OutOfDomain`] si la cible sort
    /// du domaine (`After` : v + d > total ; `Before` : v < |d|).
    pub fn add_working(&self, t: Moment, d: Minutes) -> Result<Moment, CalendarError> {
        let d = d.get();
        if d == 0 {
            return Ok(t);
        }
        if self.total == 0 {
            return Err(self.no_working_time());
        }
        let v = self.w(t)?;
        if d > 0 {
            match v.checked_add(d) {
                Some(target) if target <= self.total => self.reach(target, 0),
                _ => Err(self.out_of_domain(Direction::After)),
            }
        } else {
            match d.checked_neg() {
                Some(n) if n <= v => {
                    let target = v
                        .checked_sub(n)
                        .and_then(|x| x.checked_add(1))
                        .ok_or_else(|| self.inconsistent())?;
                    self.reach(target, -1)
                }
                _ => Err(self.out_of_domain(Direction::Before)),
            }
        }
    }

    /// R-024 : plus petit u >= t dont la minute `[u, u + 1)` est ouvrée.
    /// Idempotente.
    ///
    /// # Errors
    ///
    /// [`CalendarError::NoWorkingTime`] ; [`CalendarError::OutOfDomain`]
    /// (`After`) s'il n'y a plus de minute ouvrée après `t`.
    pub fn next_working_moment(&self, t: Moment) -> Result<Moment, CalendarError> {
        if self.total == 0 {
            return Err(self.no_working_time());
        }
        let v = self.w(t)?;
        if v >= self.total {
            return Err(self.out_of_domain(Direction::After));
        }
        self.reach(v.saturating_add(1), -1)
    }

    /// R-025 : plus grand u <= t tel que la minute `[u − 1, u)` est ouvrée.
    /// Idempotente.
    ///
    /// # Errors
    ///
    /// [`CalendarError::NoWorkingTime`] ; [`CalendarError::OutOfDomain`]
    /// (`Before`) s'il n'y a aucune minute ouvrée avant `t`.
    pub fn previous_working_moment(&self, t: Moment) -> Result<Moment, CalendarError> {
        if self.total == 0 {
            return Err(self.no_working_time());
        }
        let v = self.w(t)?;
        if v == 0 {
            return Err(self.out_of_domain(Direction::Before));
        }
        self.reach(v, 0)
    }
}

/// Diagnostic de la compilation du calendrier `uid` : aucun temps ouvré, ou
/// échec. Toute erreur de compilation donne un diagnostic : celles que
/// [`Diagnostic::from_calendar_error`] ne couvre pas deviennent
/// [`Diagnostic::CalendarInternal`] (§ 6.5 : un diagnostic par calendrier en
/// échec).
fn compile_diagnostic(
    uid: CalendarUid,
    result: &Result<CompiledCalendar, CalendarError>,
) -> Option<Diagnostic> {
    match result {
        Ok(cc) if !cc.has_working_time() => Some(Diagnostic::CalendarNoWorkingTime { calendar: uid }),
        Ok(_) => None,
        Err(CalendarError::TooManySegments { limit }) => Some(Diagnostic::CalendarTooManySegments {
            calendar: uid,
            other: uid,
            limit: *limit,
        }),
        Err(e) => {
            Some(Diagnostic::from_calendar_error(e).unwrap_or(Diagnostic::CalendarInternal { calendar: uid }))
        }
    }
}

/// Calendriers d'un projet, compilés une fois par recalcul (aucun cache ni
/// invalidation).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarSet {
    compiled: BTreeMap<CalendarUid, Result<CompiledCalendar, CalendarError>>,
}

/// Budget de compilation de [`CalendarSet::compile_with`] (R-172 amendée,
/// règle 10) : segments gardés et travail restants, tous calendriers
/// confondus.
#[derive(Clone, Copy, Debug)]
struct CompileBudget {
    kept: usize,
    work: usize,
    limit: usize,
}

impl CompileBudget {
    const fn new(limit: usize) -> Self {
        Self {
            kept: limit,
            work: limit.saturating_mul(COMPILE_WORK_FACTOR),
            limit,
        }
    }

    /// Compile `uid` dans la place restante et la décompte ;
    /// [`CalendarError::TooManySegments`] (plafond = le budget) au-delà.
    fn compile(
        &mut self,
        calendars: &Arena<CalendarUid, Calendar>,
        uid: CalendarUid,
        base: Option<&CompiledCalendar>,
    ) -> Result<CompiledCalendar, CalendarError> {
        let ch = chain(calendars, uid)?;
        let room = Room {
            segments: self.kept,
            work: self.work,
        };
        let built = match (ch, base) {
            (CalendarChain::Derived { calendar, .. }, Some(b))
                if CompiledCalendar::overlays_base(calendar) =>
            {
                CompiledCalendar::overlay(b, calendar, room)
            }
            _ => CompiledCalendar::build(ch, room),
        };
        match built {
            Some(Built::Done(c, work)) => {
                self.kept = self.kept.saturating_sub(c.segment_count());
                self.work = self.work.saturating_sub(work);
                Ok(c)
            }
            Some(Built::Over) => {
                // Le premier refus ferme le budget : les calendriers suivants
                // sont refusés sans être dépliés (un fichier hostile ne fait
                // pas recommencer un calendrier presque entier à chaque
                // refus ; un projet réel n'approche pas du budget).
                self.kept = 0;
                self.work = 0;
                Err(CalendarError::TooManySegments {
                    limit: u32::try_from(self.limit).unwrap_or(u32::MAX),
                })
            }
            None => Err(CalendarError::Inconsistent { calendar: uid }),
        }
    }
}

impl CalendarSet {
    /// Compile chaque calendrier du projet. Rend un diagnostic par calendrier
    /// en échec (héritage), sans temps ouvré ou hors budget ; n'échoue
    /// jamais. Budget par défaut ([`DEFAULT_COMPILED_SEGMENTS`], celui de
    /// `Limits::default`).
    #[must_use]
    pub fn compile(project: &Project) -> (Self, Diagnostics) {
        Self::compile_with(project, DEFAULT_COMPILED_SEGMENTS)
    }

    /// [`CalendarSet::compile`] avec un budget de `limit` segments gardés
    /// (et 8 fois autant de travail : séries d'exception dépliées, points de
    /// coupure), tous calendriers confondus (R-172 amendée, règle 10). Le
    /// calendrier du projet, puis sa base, sont compilés d'abord ; les autres
    /// dans l'ordre du modèle. Le premier calendrier qui ne tient plus dans
    /// le budget est abandonné et ferme le budget : lui et les suivants ne
    /// sont pas compilés, `get` rend [`CalendarError::TooManySegments`], un
    /// diagnostic `calendar.too_many_segments` (`calendar` = `other`) est
    /// émis pour chacun et leurs tâches et ressources se replient comme pour
    /// un calendrier en échec (R-066, R-171). Travail total borné par le
    /// budget plus un dépliage d'exception (au plus 60 631 séries).
    #[must_use]
    pub fn compile_with(project: &Project, limit: usize) -> (Self, Diagnostics) {
        let mut compiled = BTreeMap::new();
        let mut budget = CompileBudget::new(limit);
        let first = project.calendar;
        let first_base = match project.calendars.get(first).map(Calendar::kind) {
            Some(CalendarKind::Derived { base, .. }) => Some(*base),
            _ => None,
        };
        let priority = [Some(first), first_base]
            .into_iter()
            .flatten()
            .filter(|uid| project.calendars.get(*uid).is_some());
        for uid in priority.chain(project.calendars.iter().map(Calendar::uid)) {
            if compiled.contains_key(&uid) {
                continue;
            }
            // Dérivé : sa base d'abord, puis surcharge de la base compilée
            // quand il n'a pas de semaine propre (lot L9).
            let base = match project.calendars.get(uid).map(Calendar::kind) {
                Some(CalendarKind::Derived { base, .. }) => Some(*base),
                _ => None,
            };
            if let Some(b) = base.filter(|b| project.calendars.get(*b).is_some())
                && !compiled.contains_key(&b)
            {
                let r = budget.compile(&project.calendars, b, None);
                compiled.insert(b, r);
            }
            let base_compiled = base.and_then(|b| compiled.get(&b)).and_then(|r| r.as_ref().ok());
            let r = budget.compile(&project.calendars, uid, base_compiled);
            compiled.insert(uid, r);
        }
        // Diagnostics dans l'ordre du modèle.
        let mut out = Diagnostics::new();
        for c in &project.calendars {
            if let Some(d) = compiled
                .get(&c.uid())
                .and_then(|r| compile_diagnostic(c.uid(), r))
            {
                out.push(d);
            }
        }
        (Self { compiled }, out)
    }

    /// Calendrier compilé `uid`.
    ///
    /// # Errors
    ///
    /// L'erreur de compilation du calendrier, recopiée ; ou
    /// [`CalendarError::UnknownCalendar`].
    pub fn get(&self, uid: CalendarUid) -> Result<&CompiledCalendar, CalendarError> {
        match self.compiled.get(&uid) {
            Some(Ok(c)) => Ok(c),
            Some(Err(e)) => Err(e.clone()),
            None => Err(CalendarError::UnknownCalendar { calendar: uid }),
        }
    }
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

    use super::{CalendarSet, CompiledCalendar, Segment, SegmentRule, compile_diagnostic};
    use crate::arena::Arena;
    use crate::calendar::Direction;
    use crate::calendar::builtin::{BuiltinCalendarNames, standard, twenty_four_hours};
    use crate::calendar::model::{Calendar, CalendarException, DayHours, DaySetting, Recurrence, TimeRange};
    use crate::diagnostic::Diagnostic;
    use crate::duration::Minutes;
    use crate::error::CalendarError;
    use crate::ids::CalendarUid;
    use crate::model::{Project, ProjectInit, ScheduleAnchor};
    use crate::time::{Day, DayRange, Moment, TimeOfDay, Week};

    fn uid(v: u32) -> CalendarUid {
        CalendarUid::new(v)
    }

    fn m(s: &str) -> Moment {
        s.parse().unwrap()
    }

    fn day(s: &str) -> Day {
        s.parse().unwrap()
    }

    fn holiday(from: &str, to: &str) -> CalendarException {
        CalendarException {
            name: String::new(),
            period: DayRange::new(day(from), day(to)).unwrap(),
            setting: DaySetting::NonWorking,
            recurrence: Recurrence::Period,
            occurrences: None,
        }
    }

    fn compile(c: Calendar) -> CompiledCalendar {
        let u = c.uid();
        CompiledCalendar::compile(&Arena::try_from(vec![c]).unwrap(), u).unwrap()
    }

    #[test]
    fn standard_with_two_holidays_segments() {
        let mut c = standard(uid(1), "Standard".to_owned());
        c.add_exception(holiday("2026-11-11", "2026-11-11")).unwrap();
        c.add_exception(holiday("2026-12-24", "2026-12-25")).unwrap();
        let cc = compile(c);
        // Motifs : chômé (0), jour Standard (1) ; une semaine.
        assert_eq!(cc.day_patterns.len(), 2);
        assert_eq!(cc.week_patterns.len(), 1);
        assert_eq!(cc.week_patterns[0].total, 2_400);
        let d1 = day("2026-11-11").index();
        let d2 = day("2026-12-24").index();
        // Semaine, férié, semaine, deux fériés, semaine.
        let wp = |d: i32| cc.week_patterns[0].cum_at(d).unwrap();
        let expected = [
            Segment {
                first_day: 0,
                rule: SegmentRule::Weekly(0),
                cum_start: 0,
                wp_start: 0,
            },
            Segment {
                first_day: d1,
                rule: SegmentRule::Daily(0),
                cum_start: wp(d1),
                wp_start: 0,
            },
            Segment {
                first_day: d1 + 1,
                rule: SegmentRule::Weekly(0),
                cum_start: wp(d1),
                wp_start: wp(d1 + 1),
            },
            Segment {
                first_day: d2,
                rule: SegmentRule::Daily(0),
                // Le mercredi 11/11 (480) n'a pas été travaillé.
                cum_start: wp(d2) - 480,
                wp_start: 0,
            },
            Segment {
                first_day: d2 + 2,
                rule: SegmentRule::Weekly(0),
                cum_start: wp(d2) - 480,
                wp_start: wp(d2 + 2),
            },
        ];
        assert_eq!(cc.segments, expected);
        // Total : la semaine répétée sur tout le domaine, moins les 3 jours
        // fériés ouvrés (mer 11/11, jeu 24/12 et ven 25/12).
        assert_eq!(cc.total, wp(60_631) - 3 * 480);
    }

    #[test]
    fn daily_segment_for_an_exception_range() {
        // Exception ouvrée [09:00,11:00) du lun 02/11 au dim 08/11/2026 : un
        // segment Daily de 7 jours, motif de 120 minutes.
        let mut c = standard(uid(1), "Standard".to_owned());
        let h = DayHours::new(vec![
            TimeRange::new(TimeOfDay::hm(9, 0).unwrap(), TimeOfDay::hm(11, 0).unwrap()).unwrap(),
        ])
        .unwrap();
        c.add_exception(CalendarException {
            setting: DaySetting::Working(h),
            ..holiday("2026-11-02", "2026-11-08")
        })
        .unwrap();
        let cc = compile(c);
        assert_eq!(cc.segments.len(), 3);
        let s = cc.segments[1];
        assert_eq!(s.first_day, day("2026-11-02").index());
        let SegmentRule::Daily(p) = s.rule else {
            panic!("segment Daily attendu");
        };
        assert_eq!(cc.day_patterns[p as usize].total, 120);
        assert_eq!(cc.segments[2].first_day, day("2026-11-09").index());
        assert_eq!(cc.segments[2].cum_start - s.cum_start, 7 * 120);
        // Semaine du 02/11 : 7 × 120 = 840.
        assert_eq!(
            cc.working_between(m("2026-11-02T00:00"), m("2026-11-09T00:00")),
            Ok(Minutes::new(840))
        );
        assert_eq!(
            cc.add_working(m("2026-11-07T10:00"), Minutes::new(120)),
            Ok(m("2026-11-08T10:00"))
        );
    }

    #[test]
    fn domain_bounds() {
        let cc = compile(twenty_four_hours(uid(2), "24 heures".to_owned()));
        assert_eq!(cc.total, 87_308_640);
        assert_eq!(cc.segments.len(), 1);
        assert_eq!(
            cc.working_between(Moment::MIN, Moment::MAX),
            Ok(Minutes::new(87_308_640))
        );
        assert_eq!(
            cc.add_working(Moment::MIN, Minutes::new(87_308_640)),
            Ok(Moment::MAX)
        );
        assert_eq!(
            cc.add_working(Moment::MIN, Minutes::new(87_308_641)),
            Err(CalendarError::OutOfDomain {
                calendar: uid(2),
                direction: Direction::After
            })
        );
        assert_eq!(
            cc.add_working(Moment::MAX, Minutes::new(-87_308_640)),
            Ok(Moment::MIN)
        );
        assert_eq!(cc.next_working_moment(Moment::MIN), Ok(Moment::MIN));
        assert_eq!(cc.previous_working_moment(Moment::MAX), Ok(Moment::MAX));
        assert_eq!(
            cc.next_working_moment(Moment::MAX),
            Err(CalendarError::OutOfDomain {
                calendar: uid(2),
                direction: Direction::After
            })
        );
        assert_eq!(
            cc.previous_working_moment(Moment::MIN),
            Err(CalendarError::OutOfDomain {
                calendar: uid(2),
                direction: Direction::Before
            })
        );
        assert_eq!(cc.is_working(Moment::MAX), Ok(false));
        assert_eq!(cc.is_working(Moment::MIN), Ok(true));
        assert_eq!(
            cc.add_working(Moment::MIN, Minutes::new(i64::MAX)),
            Err(CalendarError::OutOfDomain {
                calendar: uid(2),
                direction: Direction::After
            })
        );
        assert_eq!(
            cc.add_working(Moment::MAX, Minutes::new(i64::MIN)),
            Err(CalendarError::OutOfDomain {
                calendar: uid(2),
                direction: Direction::Before
            })
        );
    }

    #[test]
    fn contiguous_ranges_across_midnight() {
        // Lundi [20:00,24:00), mardi [00:00,04:00) : W ne voit qu'une plage.
        let range = |a: u16, b: u16| {
            DaySetting::Working(
                DayHours::new(vec![
                    TimeRange::new(TimeOfDay::hm(a, 0).unwrap(), TimeOfDay::hm(b, 0).unwrap()).unwrap(),
                ])
                .unwrap(),
            )
        };
        let week = Week::from_fn(|d| match d {
            crate::time::Weekday::Monday => range(20, 24),
            crate::time::Weekday::Tuesday => range(0, 4),
            _ => DaySetting::NonWorking,
        });
        let cc = compile(Calendar::new_base(uid(5), "Nuit".to_owned(), week));
        // Lun 19/10/2026 23:00 + 120 = mar 01:00 ; mar 00:00 − 60 = lun 23:00.
        assert_eq!(
            cc.add_working(m("2026-10-19T23:00"), Minutes::new(120)),
            Ok(m("2026-10-20T01:00"))
        );
        assert_eq!(
            cc.add_working(m("2026-10-20T00:00"), Minutes::new(-60)),
            Ok(m("2026-10-19T23:00"))
        );
        // Minuit est dans la plage : instant ouvré suivant = lui-même.
        assert_eq!(
            cc.next_working_moment(m("2026-10-20T00:00")),
            Ok(m("2026-10-20T00:00"))
        );
        assert_eq!(
            cc.day_ranges(day("2026-10-19")).unwrap(),
            [TimeRange::new(TimeOfDay::hm(20, 0).unwrap(), TimeOfDay::END_OF_DAY).unwrap()]
        );
    }

    #[test]
    fn calendar_set_diagnostics() {
        let mut p = Project::new(ProjectInit {
            name: "P".to_owned(),
            anchor: ScheduleAnchor::FromStart(m("2026-10-19T08:00")),
            current_date: m("2026-10-17T08:00"),
            calendar_names: BuiltinCalendarNames::default(),
        });
        let none = Calendar::new_base(
            uid(4),
            "Rien".to_owned(),
            Week::from_fn(|_| DaySetting::NonWorking),
        );
        p.calendars.push(none).unwrap();
        p.calendars
            .push(Calendar::new_derived(uid(5), "Orphelin".to_owned(), uid(99)))
            .unwrap();
        let (set, diags) = CalendarSet::compile(&p);
        assert_eq!(
            diags.finish(),
            [
                Diagnostic::CalendarBaseMissing {
                    calendar: uid(5),
                    base: uid(99)
                },
                Diagnostic::CalendarNoWorkingTime { calendar: uid(4) },
            ]
        );
        assert!(set.get(uid(1)).unwrap().has_working_time());
        assert!(!set.get(uid(4)).unwrap().has_working_time());
        assert_eq!(
            set.get(uid(5)).unwrap_err(),
            CalendarError::BaseMissing {
                calendar: uid(5),
                base: uid(99)
            }
        );
        assert_eq!(
            set.get(uid(7)).unwrap_err(),
            CalendarError::UnknownCalendar { calendar: uid(7) }
        );
    }

    #[test]
    fn every_compile_failure_has_a_diagnostic() {
        // Résultats fabriqués : un invariant violé (jamais produit par la
        // compilation) et des erreurs hors héritage donnent un diagnostic
        // d'erreur interne, jamais un silence.
        for e in [
            CalendarError::Inconsistent { calendar: uid(6) },
            CalendarError::UnknownCalendar { calendar: uid(6) },
            CalendarError::NegativeSpan,
        ] {
            let d = compile_diagnostic(uid(6), &Err(e)).unwrap();
            assert_eq!(d, Diagnostic::CalendarInternal { calendar: uid(6) });
            assert_eq!(d.severity(), crate::diagnostic::Severity::Error);
        }
        assert_eq!(
            compile_diagnostic(
                uid(5),
                &Err(CalendarError::BaseMissing {
                    calendar: uid(5),
                    base: uid(99)
                })
            ),
            Some(Diagnostic::CalendarBaseMissing {
                calendar: uid(5),
                base: uid(99)
            })
        );
        let a = Arena::try_from(vec![standard(uid(1), "Standard".to_owned())]).unwrap();
        assert_eq!(
            compile_diagnostic(uid(1), &CompiledCalendar::compile(&a, uid(1))),
            None
        );
    }

    /// Index de blocs et recherches galopantes (optimisation du 2026-10-19) :
    /// mêmes résultats que la dichotomie et que les conversions séparées,
    /// sur un calendrier dense en exceptions (blocs à nombreux segments),
    /// sur tout le domaine, bornes comprises.
    #[test]
    fn fast_paths_match_reference() {
        let mut cal = standard(uid(1), "Standard".to_owned());
        // Un jour chômé sur trois pendant deux ans, puis des séries isolées.
        let first = day("2026-01-01").index();
        for k in (0..730).step_by(3) {
            let d = Day::new(first + k).unwrap();
            let e = CalendarException {
                name: String::new(),
                period: DayRange::new(d, d).unwrap(),
                setting: DaySetting::NonWorking,
                recurrence: Recurrence::Period,
                occurrences: None,
            };
            cal.add_exception(e).unwrap();
        }
        cal.add_exception(holiday("1984-01-02", "1984-01-20")).unwrap();
        cal.add_exception(holiday("2149-12-01", "2149-12-31")).unwrap();
        let cc = compile(cal);
        assert!(cc.segments.len() > 400, "{}", cc.segments.len());
        // Segment d'un jour : bloc puis dichotomie = dichotomie seule.
        for d in 0..super::DAY_COUNT {
            let reference = cc.segments.partition_point(|s| s.first_day <= d) - 1;
            assert_eq!(cc.segment_index(d), Some(reference), "jour {d}");
        }
        // Recherche galopante depuis toute indication = dichotomie.
        let mut seed: u64 = 2026;
        let mut next = |hi: i64| {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            i64::try_from((seed >> 33) % u64::try_from(hi).unwrap()).unwrap()
        };
        let total = cc.total;
        let n = i64::try_from(cc.segments.len()).unwrap();
        let mut targets: Vec<i64> = (0..20_000).map(|_| next(total) + 1).collect();
        targets.extend([1, 2, total - 1, total]);
        for t in targets {
            let hint = usize::try_from(next(n)).unwrap();
            let reference = cc.first_reach(t).unwrap();
            assert_eq!(
                cc.first_reach_near(t, hint).map(|r| r.0),
                Some(reference),
                "{t} {hint}"
            );
        }
        // Conversions fusionnées = conversions séparées (mêmes erreurs).
        let max = Moment::MAX.minutes();
        let mut moments: Vec<i64> = (0..20_000).map(|_| next(max + 1)).collect();
        moments.extend([0, 1, max - 1, max]);
        for t in moments {
            let t = Moment::from_minutes(t).unwrap();
            let d = next(20_000) + 1;
            let after = cc
                .next_working_moment(t)
                .and_then(|s| Ok((s, cc.add_working(s, Minutes::new(d))?)));
            assert_eq!(cc.span_after(t, d), after, "{t} {d}");
            let before = cc
                .previous_working_moment(t)
                .and_then(|f| Ok((cc.add_working(f, Minutes::new(-d))?, f)));
            assert_eq!(cc.span_before(t, d), before, "{t} {d}");
            let probe = cc.probe(t).unwrap();
            assert_eq!(cc.next_of(probe), cc.next_working_moment(t), "{t}");
            assert_eq!(cc.previous_of(probe), cc.previous_working_moment(t), "{t}");
        }
    }

    #[test]
    fn same_working_time_ignores_uid_only() {
        // Calendrier de ressource dérivé sans exception propre : même forme
        // compilée que sa base (réutilisé comme une seule échelle par le
        // placement) ; avec un congé propre, différente.
        let base = standard(uid(1), "Standard".to_owned());
        let derived = Calendar::new_derived(uid(2), "R".to_owned(), uid(1));
        let mut leave = Calendar::new_derived(uid(3), "L".to_owned(), uid(1));
        leave.add_exception(holiday("2026-11-23", "2026-11-24")).unwrap();
        let arena = Arena::try_from(vec![base, derived, leave]).unwrap();
        let b = CompiledCalendar::compile(&arena, uid(1)).unwrap();
        let d = CompiledCalendar::compile(&arena, uid(2)).unwrap();
        let l = CompiledCalendar::compile(&arena, uid(3)).unwrap();
        assert_ne!(b, d);
        assert!(b.same_working_time(&d));
        assert!(!b.same_working_time(&l));
        assert!(b.intersect(&d).unwrap().same_working_time(&b));
        // Même forme ⇒ même empreinte ; formes différentes : empreintes
        // différentes ici (condition non garantie en général).
        assert_eq!(b.shape_fingerprint(), d.shape_fingerprint());
        assert_ne!(b.shape_fingerprint(), l.shape_fingerprint());
        assert!(l.segment_count() > b.segment_count());
    }

    /// Surcharge d'une base compilée (J6, lot L9) = compilation complète,
    /// sur tout le domaine : mêmes plages chaque jour, même W à chaque
    /// minuit et en des instants tirés, mêmes conversions ; base à fériés,
    /// semaine datée et exception ouvrée, dérivés à exceptions qui
    /// chevauchent, prolongent ou jouxtent celles de la base, récurrentes,
    /// aux bornes du domaine.
    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "test : jeu de calendriers et comparaisons en un seul lieu"
    )]
    fn overlay_matches_full_compile() {
        use crate::calendar::model::WorkWeek;
        let ranges = |a: u16, b: u16| {
            DaySetting::Working(
                DayHours::new(vec![
                    TimeRange::new(TimeOfDay::hm(a, 0).unwrap(), TimeOfDay::hm(b, 0).unwrap()).unwrap(),
                ])
                .unwrap(),
            )
        };
        let mut base = standard(uid(1), "Standard".to_owned());
        let first = day("2026-01-01").index();
        for k in (0..150).step_by(5) {
            let d = Day::new(first + k).unwrap();
            base.add_exception(CalendarException {
                period: DayRange::new(d, d).unwrap(),
                ..holiday("2026-01-01", "2026-01-01")
            })
            .unwrap();
        }
        base.add_exception(CalendarException {
            setting: ranges(9, 11),
            ..holiday("2026-07-06", "2026-07-12")
        })
        .unwrap();
        base.add_work_week(WorkWeek {
            name: "Été".to_owned(),
            period: DayRange::new(day("2027-07-01"), day("2027-08-31")).unwrap(),
            days: Week::from_fn(|wd| match wd {
                crate::time::Weekday::Saturday => crate::calendar::model::DayRule::Working(
                    DayHours::new(vec![
                        TimeRange::new(TimeOfDay::hm(8, 0).unwrap(), TimeOfDay::hm(12, 0).unwrap()).unwrap(),
                    ])
                    .unwrap(),
                ),
                _ => crate::calendar::model::DayRule::Inherit,
            }),
        })
        .unwrap();
        let derived = |u: u32, f: &dyn Fn(&mut Calendar)| {
            let mut c = Calendar::new_derived(uid(u), format!("D{u}"), uid(1));
            f(&mut c);
            c
        };
        let cals = vec![
            base,
            derived(2, &|_| {}),
            derived(3, &|c| {
                // Chevauche des fériés de la base, jouxte la semaine ouvrée
                // d'exception, prolonge une série.
                c.add_exception(holiday("2026-01-04", "2026-01-12")).unwrap();
                c.add_exception(holiday("2026-07-13", "2026-07-14")).unwrap();
                c.add_exception(CalendarException {
                    setting: ranges(7, 19),
                    ..holiday("2026-07-08", "2026-07-08")
                })
                .unwrap();
                c.add_exception(CalendarException {
                    setting: ranges(9, 11),
                    ..holiday("2026-07-01", "2026-07-05")
                })
                .unwrap();
                c.add_exception(holiday("2027-07-30", "2027-08-02")).unwrap();
            }),
            derived(4, &|c| {
                // Bornes du domaine et récurrence.
                c.add_exception(holiday("1984-01-01", "1984-01-03")).unwrap();
                c.add_exception(holiday("2149-12-30", "2149-12-31")).unwrap();
                c.add_exception(CalendarException {
                    recurrence: Recurrence::Daily { every: 3 },
                    ..holiday("2026-02-01", "2026-12-31")
                })
                .unwrap();
            }),
        ];
        let arena = Arena::try_from(cals).unwrap();
        let b = CompiledCalendar::compile(&arena, uid(1)).unwrap();
        for u in 2..=4 {
            let c = arena.get(uid(u)).unwrap();
            assert!(CompiledCalendar::overlays_base(c));
            let Some(super::Built::Done(over, _)) = CompiledCalendar::overlay(&b, c, super::Room::UNBOUNDED)
            else {
                panic!("surcharge attendue");
            };
            let full = CompiledCalendar::compile(&arena, uid(u)).unwrap();
            assert_eq!(over.uid(), uid(u));
            assert_eq!(over.total, full.total, "calendrier {u}");
            assert_eq!(over.segments.len(), full.segments.len(), "calendrier {u}");
            for d in 0..super::DAY_COUNT {
                let dd = Day::new(d).unwrap();
                assert_eq!(over.day_ranges(dd), full.day_ranges(dd), "{u} jour {d}");
                let t = Moment::from_minutes(i64::from(d) * 1_440).unwrap();
                assert_eq!(over.w(t), full.w(t), "{u} jour {d}");
            }
            let mut seed: u64 = u64::from(u);
            let mut next = |hi: i64| {
                seed = seed
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                i64::try_from((seed >> 33) % u64::try_from(hi).unwrap()).unwrap()
            };
            let max = Moment::MAX.minutes();
            for _ in 0..5_000 {
                let t = Moment::from_minutes(next(max + 1)).unwrap();
                let d = Minutes::new(next(20_000) - 10_000);
                assert_eq!(over.add_working(t, d), full.add_working(t, d), "{u} {t}");
                assert_eq!(
                    over.next_working_moment(t),
                    full.next_working_moment(t),
                    "{u} {t}"
                );
                assert_eq!(
                    over.span_after(t, d.get().abs() + 1),
                    full.span_after(t, d.get().abs() + 1)
                );
            }
        }
        // Sans exception : forme identique à la base (échelle partagée par
        // le placement, `same_working_time`).
        let Some(super::Built::Done(plain, _)) =
            CompiledCalendar::overlay(&b, arena.get(uid(2)).unwrap(), super::Room::UNBOUNDED)
        else {
            panic!("surcharge attendue");
        };
        assert!(plain.same_working_time(&b));
        // Un dérivé à semaine propre n'est pas surchargé.
        let mut own = Calendar::new_derived(uid(5), "Propre".to_owned(), uid(1));
        own.set_day(
            crate::time::Weekday::Saturday,
            crate::calendar::model::DayRule::NonWorking,
        )
        .unwrap();
        assert!(!CompiledCalendar::overlays_base(&own));
        let mut dated = Calendar::new_derived(uid(6), "Datée".to_owned(), uid(1));
        dated
            .add_work_week(WorkWeek {
                name: "Hiver".to_owned(),
                period: DayRange::new(day("2027-01-04"), day("2027-01-10")).unwrap(),
                days: Week::from_fn(|_| crate::calendar::model::DayRule::Inherit),
            })
            .unwrap();
        assert!(!CompiledCalendar::overlays_base(&dated));
        assert!(!CompiledCalendar::overlays_base(&b_model()));
    }

    /// Calendrier de base (jamais surchargé).
    fn b_model() -> Calendar {
        standard(uid(1), "Standard".to_owned())
    }
}
