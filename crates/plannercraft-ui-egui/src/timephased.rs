//! Données d'utilisation par période pour la vue « Utilisation des
//! ressources » (J7, ADR-0015 D7 ; lot L11) : valeurs du moteur
//! (`plannercraft_core::resource::usage_with`, `usage_by_assignment`, les deux
//! fonctions qu'appelle `analysis.resource_usage`) par blocs de
//! [`CHUNK_PERIODS`] périodes, pour les seules ressources visibles, gardées
//! en cache par état du document ([`MAX_CHUNKS`] blocs au plus).
//!
//! Sans egui. Aucun calcul de travail ici (règle 3) : chaque cellule est une
//! valeur du moteur, chaque période une borne du moteur. Seules les bornes
//! d'un **bloc** (début de la graduation fine qui contient le début de la
//! plage affichée, puis 64 graduations plus loin) sont posées par
//! l'interface, comme les cellules de l'en-tête de l'échelle de temps
//! (ADR-0011, décision 4) : elles ne servent qu'à choisir la requête, le
//! moteur découpe ensuite ses propres périodes. Refus du moteur
//! (`usage.limit`) gardé en cache et affiché par la vue (règle 10).

use std::collections::BTreeMap;

use chrono::Months;
use plannercraft_core::resource::usage::{Granularity, UsageError, usage_by_assignment, usage_with};
use plannercraft_core::{
    AssignmentUid, CalendarSet, Day, Moment, Project, ResourceUid, Schedule, Span, TimeOfDay, Weekday, Work,
};

use crate::timescale::{TierUnit, TimeAxis, TimeScaleZoom, ViewSpan};

/// Périodes d'un bloc.
pub const CHUNK_PERIODS: usize = 64;

/// Blocs gardés en cache au plus (les plus anciens sortent).
pub const MAX_CHUNKS: usize = 8;

/// Blocs parcourus au plus pour une image : la plage affichée (20 ans au
/// plus, `MAX_RANGE_DAYS`) tient en 115 blocs de 64 jours.
pub const MAX_CHUNK_INDEX: i64 = 128;

/// Granularité de la vue au zoom `zoom` : la graduation fine de l'en-tête
/// (H-UI-25) ; l'en-tête n'a jamais l'année en bas.
#[must_use]
pub const fn granularity(zoom: TimeScaleZoom) -> Granularity {
    match zoom.tiers().1 {
        TierUnit::Day => Granularity::Day,
        TierUnit::Week => Granularity::Week,
        TierUnit::Month => Granularity::Month,
        TierUnit::Quarter | TierUnit::Year => Granularity::Quarter,
    }
}

/// Premier jour de la graduation `by` qui contient `d` (semaine au
/// `week_start`, mois au 1er, trimestre civil) ; `d` si le début sort du
/// domaine.
fn unit_start(d: Day, by: Granularity, week_start: Weekday) -> Day {
    match by {
        Granularity::Day => d,
        Granularity::Week => {
            let mut day = d;
            for _ in 0..7 {
                if day.weekday() == week_start {
                    return day;
                }
                match day.checked_add_days(-1) {
                    Ok(prev) => day = prev,
                    Err(_) => return d,
                }
            }
            d
        }
        Granularity::Month | Granularity::Quarter => {
            use chrono::Datelike as _;
            let date = d.to_naive();
            let month = if by == Granularity::Quarter {
                (date.month().saturating_sub(1) / 3)
                    .saturating_mul(3)
                    .saturating_add(1)
            } else {
                date.month()
            };
            chrono::NaiveDate::from_ymd_opt(date.year(), month, 1)
                .and_then(|first| Day::from_naive(first).ok())
                .unwrap_or(d)
        }
    }
}

/// Jour à `n` graduations `by` de `first` (début d'une graduation).
fn units_after(first: Day, by: Granularity, n: i64) -> Option<Day> {
    let days = |per: i64| {
        n.checked_mul(per)
            .and_then(|k| i32::try_from(k).ok())
            .and_then(|k| first.checked_add_days(k).ok())
    };
    let months = |per: i64| {
        n.checked_mul(per)
            .and_then(|k| u32::try_from(k).ok())
            .and_then(|k| first.to_naive().checked_add_months(Months::new(k)))
            .and_then(|d| Day::from_naive(d).ok())
    };
    match by {
        Granularity::Day => days(1),
        Granularity::Week => days(7),
        Granularity::Month => months(1),
        Granularity::Quarter => months(3),
    }
}

/// Bornes `[début, fin)` du bloc `k` : de la graduation `64 k` à la
/// graduation `64 (k + 1)`, comptées depuis celle qui contient le début de
/// la plage. `None` hors du domaine des dates ou pour `k` négatif.
#[must_use]
pub fn chunk_bounds(
    range: ViewSpan,
    by: Granularity,
    week_start: Weekday,
    k: i64,
) -> Option<(Moment, Moment)> {
    if k < 0 {
        return None;
    }
    let first = unit_start(range.start, by, week_start);
    let per = i64::try_from(CHUNK_PERIODS).ok()?;
    let from = units_after(first, by, k.checked_mul(per)?)?;
    let to = units_after(first, by, k.checked_add(1)?.checked_mul(per)?)?;
    let from = Moment::at(from, TimeOfDay::MIDNIGHT).ok()?;
    let to = Moment::at(to, TimeOfDay::MIDNIGHT).ok()?;
    (from < to).then_some((from, to))
}

/// Blocs qui coupent l'intervalle d'abscisses écran `clip` (gauche,
/// droite) sur l'axe `axis` : `(k, début, fin)`, dans l'ordre. Sens direct
/// seulement (date → pixel) ; s'arrête à la fin de la plage.
#[must_use]
pub fn visible_chunks(
    axis: &TimeAxis,
    range: ViewSpan,
    by: Granularity,
    week_start: Weekday,
    clip: (f32, f32),
) -> Vec<(i64, Moment, Moment)> {
    let (left, right) = clip;
    let mut out = Vec::new();
    if !left.is_finite() || !right.is_finite() || right <= left {
        return out;
    }
    let Ok(end) = Moment::at(range.end, TimeOfDay::MIDNIGHT) else {
        return out;
    };
    for k in 0..MAX_CHUNK_INDEX {
        let Some((from, to)) = chunk_bounds(range, by, week_start, k) else {
            break;
        };
        if from >= end || axis.x(from) >= right {
            break;
        }
        if axis.x(to) > left {
            out.push((k, from, to));
        }
    }
    out
}

/// Clé du cache : état du document, granularité, ressources visibles (ordre
/// d'affichage) et, parmi elles, celles dépliées en affectations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageCacheKey {
    /// (numéro d'ouverture, génération) du document.
    pub doc: (u64, u64),
    /// Granularité des périodes.
    pub by: Granularity,
    /// Ressources visibles, dans l'ordre des lignes.
    pub resources: Vec<ResourceUid>,
    /// Ressources visibles dépliées (lignes d'affectation demandées).
    pub expanded: Vec<ResourceUid>,
}

/// Un bloc de périodes calculé par le moteur.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PeriodChunk {
    /// Périodes du bloc (bornes du moteur).
    pub periods: Vec<Span>,
    /// Travail par période de chaque ressource de la clé, dans son ordre.
    pub rows: Vec<Vec<Work>>,
    /// Surutilisation par période de chaque ressource de la clé.
    pub over: Vec<Vec<bool>>,
    /// Travail par période des affectations des ressources dépliées.
    pub assignments: BTreeMap<AssignmentUid, Vec<Work>>,
}

impl PeriodChunk {
    /// Travail et surutilisation de la ressource d'indice `i` (dans la clé)
    /// pendant la période `p`.
    #[must_use]
    pub fn resource_cell(&self, i: usize, p: usize) -> Option<(Work, bool)> {
        let work = self.rows.get(i).and_then(|row| row.get(p)).copied()?;
        let over = self
            .over
            .get(i)
            .and_then(|row| row.get(p))
            .copied()
            .unwrap_or(false);
        Some((work, over))
    }

    /// Travail de l'affectation `uid` pendant la période `p`.
    #[must_use]
    pub fn assignment_cell(&self, uid: AssignmentUid, p: usize) -> Option<Work> {
        self.assignments.get(&uid).and_then(|row| row.get(p)).copied()
    }
}

/// Calcule le bloc `[from, to)` par le moteur : `usage_with` pour les
/// ressources de la clé, `usage_by_assignment` pour les dépliées (aucun
/// appel s'il n'y en a pas).
///
/// # Errors
///
/// Le refus du moteur (`usage.limit`).
pub fn fetch(
    project: &Project,
    schedule: &Schedule,
    set: &CalendarSet,
    key: &UsageCacheKey,
    (from, to): (Moment, Moment),
) -> Result<PeriodChunk, UsageError> {
    let report = usage_with(project, schedule, set, from, to, key.by, &key.resources)?;
    let assignments = if key.expanded.is_empty() {
        BTreeMap::new()
    } else {
        usage_by_assignment(project, schedule, set, from, to, key.by, &key.expanded)?
            .rows
            .into_iter()
            .map(|row| (row.assignment, row.work))
            .collect()
    };
    // Bornes du moteur : débuts de ses périodes, la première ramenée à
    // `from`, la dernière fermée par `to` (comme `AssignmentUsageReport`).
    let starts = &report.periods;
    let periods = starts
        .iter()
        .enumerate()
        .map(|(i, &start)| Span {
            start: if i == 0 { from } else { start },
            finish: i
                .checked_add(1)
                .and_then(|j| starts.get(j))
                .copied()
                .unwrap_or(to),
        })
        .collect();
    let (rows, over) = report
        .rows
        .into_iter()
        .map(|row| (row.work, row.overallocated))
        .unzip();
    Ok(PeriodChunk {
        periods,
        rows,
        over,
        assignments,
    })
}

/// Cache des blocs d'une clé : vidé quand la clé change, plafonné à
/// [`MAX_CHUNKS`] blocs (le plus ancien inséré sort).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UsageCache {
    /// Clé des blocs gardés.
    key: Option<UsageCacheKey>,
    /// Blocs par indice, avec leur rang d'insertion ; un refus est gardé
    /// comme un bloc (pas de nouvelle requête à chaque image).
    chunks: BTreeMap<i64, (u64, Result<PeriodChunk, UsageError>)>,
    /// Rang de la prochaine insertion.
    next: u64,
    /// Requêtes faites au moteur depuis la création (tests).
    queries: u64,
}

impl UsageCache {
    /// Bloc `k` de la clé `key`, calculé par `compute` s'il n'est pas en
    /// cache. Une clé différente vide le cache.
    pub fn get_or_fetch(
        &mut self,
        key: &UsageCacheKey,
        k: i64,
        compute: impl FnOnce() -> Result<PeriodChunk, UsageError>,
    ) -> &Result<PeriodChunk, UsageError> {
        if self.key.as_ref() != Some(key) {
            self.key = Some(key.clone());
            self.chunks.clear();
        }
        let rank = self.next;
        if !self.chunks.contains_key(&k) {
            while self.chunks.len() >= MAX_CHUNKS {
                let oldest = self
                    .chunks
                    .iter()
                    .min_by_key(|(_, (rank, _))| *rank)
                    .map(|(k, _)| *k);
                match oldest {
                    Some(old) => drop(self.chunks.remove(&old)),
                    None => break,
                }
            }
            self.queries = self.queries.saturating_add(1);
            self.next = self.next.saturating_add(1);
        }
        &self.chunks.entry(k).or_insert_with(|| (rank, compute())).1
    }

    /// Requêtes faites au moteur depuis la création.
    #[must_use]
    pub const fn queries(&self) -> u64 {
        self.queries
    }

    /// Blocs en cache.
    #[must_use]
    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    /// Aucun bloc en cache.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// Indices des blocs en cache.
    #[must_use]
    pub fn indices(&self) -> Vec<i64> {
        self.chunks.keys().copied().collect()
    }

    /// Clé des blocs gardés.
    #[must_use]
    pub const fn key(&self) -> Option<&UsageCacheKey> {
        self.key.as_ref()
    }

    /// Remplace chaque bloc gardé par le refus `error` (tests du message).
    #[cfg(test)]
    pub(crate) fn poison(&mut self, error: UsageError) {
        for (_, slot) in self.chunks.values_mut() {
            *slot = Err(error);
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use plannercraft_core::resource::usage::{Granularity, UsageError};
    use plannercraft_core::{Day, Moment, Weekday};

    use super::{
        CHUNK_PERIODS, MAX_CHUNKS, PeriodChunk, UsageCache, UsageCacheKey, chunk_bounds, granularity,
    };
    use crate::timescale::{TimeScaleZoom, ViewSpan};

    fn day(s: &str) -> Day {
        Day::from_naive(s.parse().unwrap()).unwrap()
    }

    fn m(s: &str) -> Moment {
        s.parse().unwrap()
    }

    fn range() -> ViewSpan {
        ViewSpan {
            start: day("2026-10-14"),
            end: day("2028-01-01"),
        }
    }

    /// H-UI-25 : la granularité est la graduation fine de l'en-tête.
    #[test]
    fn granularity_is_bottom_tier() {
        assert_eq!(granularity(TimeScaleZoom::Day), Granularity::Day);
        assert_eq!(granularity(TimeScaleZoom::Week), Granularity::Week);
        assert_eq!(granularity(TimeScaleZoom::Month), Granularity::Month);
        assert_eq!(granularity(TimeScaleZoom::Quarter), Granularity::Quarter);
    }

    /// Blocs alignés sur la graduation qui contient le début de la plage,
    /// 64 graduations chacun, jointifs.
    #[test]
    fn chunk_bounds_aligned_and_contiguous() {
        let r = range();
        let w = Weekday::Monday;
        assert_eq!(
            chunk_bounds(r, Granularity::Day, w, 0),
            Some((m("2026-10-14T00:00"), m("2026-12-17T00:00")))
        );
        // Mercredi 14/10 : semaine du lundi 12/10, 64 semaines = 448 jours.
        assert_eq!(
            chunk_bounds(r, Granularity::Week, w, 0),
            Some((m("2026-10-12T00:00"), m("2028-01-03T00:00")))
        );
        assert_eq!(
            chunk_bounds(r, Granularity::Month, w, 1),
            Some((m("2032-02-01T00:00"), m("2037-06-01T00:00")))
        );
        assert_eq!(
            chunk_bounds(r, Granularity::Quarter, w, 0),
            Some((m("2026-10-01T00:00"), m("2042-10-01T00:00")))
        );
        for by in [Granularity::Day, Granularity::Week, Granularity::Month] {
            let (_, end0) = chunk_bounds(r, by, w, 0).unwrap();
            let (start1, _) = chunk_bounds(r, by, w, 1).unwrap();
            assert_eq!(end0, start1, "{by:?}");
        }
        assert_eq!(chunk_bounds(r, Granularity::Day, w, -1), None);
        // Hors du domaine des dates : aucun bloc, aucune panique.
        assert_eq!(chunk_bounds(r, Granularity::Quarter, w, i64::MAX), None);
        assert_eq!(chunk_bounds(r, Granularity::Day, w, 100_000), None);
    }

    fn key(n: u32) -> UsageCacheKey {
        UsageCacheKey {
            doc: (1, 0),
            by: Granularity::Week,
            resources: vec![plannercraft_core::ResourceUid::new(n)],
            expanded: Vec::new(),
        }
    }

    /// Bloc en cache : aucune requête ; clé changée : cache vidé ; au plus
    /// `MAX_CHUNKS` blocs, le plus ancien sort ; un refus est gardé.
    #[test]
    fn cache_hits_evicts_and_keeps_refusals() {
        let mut cache = UsageCache::default();
        let k1 = key(1);
        assert!(cache.get_or_fetch(&k1, 0, || Ok(PeriodChunk::default())).is_ok());
        assert!(
            cache
                .get_or_fetch(&k1, 0, || panic!("bloc en cache recalculé"))
                .is_ok()
        );
        assert_eq!(cache.queries(), 1);
        for k in 1..=i64::try_from(MAX_CHUNKS).unwrap() {
            let _ = cache.get_or_fetch(&k1, k, || Ok(PeriodChunk::default()));
        }
        assert_eq!(cache.len(), MAX_CHUNKS);
        assert!(!cache.indices().contains(&0), "{:?}", cache.indices());
        let limit = UsageError::Limit {
            periods: 1,
            resources: 1,
        };
        assert_eq!(cache.get_or_fetch(&k1, 50, || Err(limit)), &Err(limit));
        assert_eq!(
            cache.get_or_fetch(&k1, 50, || panic!("refus recalculé")),
            &Err(limit)
        );
        let before = cache.queries();
        let _ = cache.get_or_fetch(&key(2), 50, || Ok(PeriodChunk::default()));
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.queries(), before + 1);
        assert_eq!(CHUNK_PERIODS, 64);
    }
}
