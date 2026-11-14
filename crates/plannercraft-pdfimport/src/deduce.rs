//! Calendrier déduit des écarts (lot P1b, R-276, R-277). **Le moteur
//! calcule, jamais un LLM** (règle 3) : arithmétique entière sur les dates
//! lues, déterministe, bornée.
//!
//! Le PDF ne dit pas quels jours son auteur a chômés (congés d'été, de fin
//! d'année, ponts) ; il les révèle par les écarts entre fins lues et fins
//! calculées. Une feuille lue « du début `s` à la fin `f`, durée `d` jours »
//! a travaillé `d` jours ouvrés dans `[s, f]` ; le calendrier `C` du projet
//! (Standard, plus les fériés cochés) y en compte `W`. Donc (R-276) :
//! - `W = d` : tous les jours ouvrés de `C` dans `[s, f]` ont été
//!   travaillés (**preuve**) ;
//! - `W > d` : exactement `e = W − d` jours ouvrés de `C` dans `[s, f]`
//!   ont été chômés (**équation**) ;
//! - `W < d`, ou début ou fin lus un jour chômé de `C` : l'auteur a
//!   travaillé des jours que `C` chôme ; rien n'est déduit de la tâche
//!   (**contredite**).
//!
//! Jours candidats : ouvrés dans `C`, couverts par une équation et par
//! aucune preuve. Une équation dont les jours candidats sont moins
//! nombreux que `e` est **impossible** (demi-journée, décalage invisible…) :
//! elle n'explique rien et n'est pas forcée. Les candidats forment des
//! suites (jours ouvrés consécutifs de `C`).
//!
//! Dans chaque suite (R-277), on cherche les **périodes continues** qui
//! satisfont exactement le plus d'équations locales (toutes leurs
//! candidates dans la suite) sans en dépasser aucune ; il leur faut la
//! majorité stricte des équations locales de la suite (les autres la
//! contredisent : rien n'est proposé). La plus courte, si
//! elle est unique et contenue dans toutes les autres, est **déterminée**
//! par le PDF. Sinon, les plus courtes sont des **placements** équivalents
//! (le PDF dit combien de jours, dans quelle fenêtre, pas lesquels) :
//! proposés seulement à partir de 2 jours ouvrés (un jour seul se confond
//! avec une demi-journée), avec un placement par défaut (celui qui forme
//! la plus longue fermeture avec les jours chômés voisins, puis qui y
//! englobe le plus de fériés, puis le plus proche du milieu de la fenêtre,
//! puis le plus tôt) que l'utilisateur déplace. Une période d'un seul jour déterminée doit expliquer au moins
//! deux équations. Mieux vaut ne rien proposer qu'inventer des congés.

use std::collections::BTreeSet;

use plannercraft_core::calendar::{DaySetting, DaySource, resolve_day};
use plannercraft_core::duration::TimeBasis;
use plannercraft_core::{Day, Project};
use serde::Serialize;

use crate::plan::ImportedPlan;

/// Marge de part et d'autre des dates lues, en jours calendaires.
const MARGIN_DAYS: i32 = 14;

/// Les deux marges, en jours.
const MARGINS: usize = 28;

/// Bornes du calcul (règle 10 : temps borné, jamais de boucle sans borne).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeduceLimits {
    /// Jours ouvrés au plus d'une période proposée (4 semaines : au-delà,
    /// ce ne sont plus des congés plausibles).
    pub max_period_days: usize,
    /// Étendue au plus des dates lues, en jours calendaires (au-delà, rien
    /// n'est déduit).
    pub max_span_days: usize,
    /// Évaluations (période candidate × équation) au plus ; au-delà, les
    /// suites restantes ne sont pas examinées.
    pub max_evaluations: u64,
}

impl Default for DeduceLimits {
    fn default() -> Self {
        Self {
            max_period_days: 20,
            max_span_days: 7_400,
            max_evaluations: 20_000_000,
        }
    }
}

/// Période chômée proposée.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProposedPeriod {
    /// Placements équivalents (premier et dernier jour chômés), dans
    /// l'ordre des dates ; un seul : période déterminée par le PDF.
    pub options: Vec<(Day, Day)>,
    /// Placement par défaut (indice dans `options`).
    pub default_option: usize,
    /// Jours ouvrés du calendrier retirés par la période.
    pub working_days: u32,
    /// Équations satisfaites exactement (toutes périodes proposées
    /// placées par défaut) dont l'intervalle touche cette période.
    pub explained: u32,
}

impl ProposedPeriod {
    /// Période déterminée par le PDF (un seul placement).
    #[must_use]
    pub fn exact(&self) -> bool {
        self.options.len() <= 1
    }

    /// Placement n° `k` (sinon le placement par défaut).
    #[must_use]
    pub fn placement(&self, k: usize) -> Option<(Day, Day)> {
        self.options
            .get(k)
            .or_else(|| self.options.get(self.default_option))
            .copied()
    }

    /// Fenêtre : du premier jour du premier placement au dernier jour du
    /// dernier.
    #[must_use]
    pub fn window(&self) -> Option<(Day, Day)> {
        Some((self.options.first()?.0, self.options.last()?.1))
    }
}

/// Résultat de la déduction.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Deduction {
    /// Périodes proposées, dans l'ordre des dates.
    pub periods: Vec<ProposedPeriod>,
    /// Feuilles exploitables (durée en jours entiers, dates lues aux heures
    /// par défaut, début et fin sur des jours ouvrés du calendrier).
    pub usable: u32,
    /// Feuilles sans écart (preuves).
    pub proven: u32,
    /// Feuilles à écart (équations).
    pub equations: u32,
    /// Équations satisfaites par les périodes proposées toutes cochées
    /// (placements par défaut).
    pub explained: u32,
    /// Équations impossibles : moins de jours candidats que de jours à
    /// retirer (demi-journée, décalage invisible) ; laissées telles quelles.
    pub impossible: u32,
    /// Feuilles écartées : durée qui n'est pas un nombre entier de jours,
    /// ou heure lue autre que l'heure par défaut (demi-journées).
    pub partial_days: u32,
    /// Feuilles contredites : moins de jours ouvrés que leur durée, ou
    /// début ou fin lus un jour chômé du calendrier.
    pub contradicted: u32,
    /// Suites de jours candidats qui n'ont donné aucune période (un seul
    /// jour sans placement unique, ou aucune période qui satisfasse une
    /// équation sans en dépasser une autre).
    pub ambiguous: u32,
    /// Une borne a été atteinte : déduction partielle.
    pub bounded: bool,
}

/// Feuille exploitable à écart : indices de jour dans l'étendue, jours à
/// retirer.
#[derive(Clone, Copy, Debug)]
struct Leaf {
    first: usize,
    last: usize,
    excess: usize,
}

/// Période retenue dans une suite (indices de jour dans l'étendue).
#[derive(Clone, Debug)]
struct Pick {
    options: Vec<(usize, usize)>,
    default_option: usize,
}

impl Pick {
    fn chosen(&self) -> (usize, usize) {
        self.options.get(self.default_option).copied().unwrap_or((0, 0))
    }
}

/// Étendue des dates lues : jours ouvrés du calendrier et sommes préfixes.
struct Span {
    min: Day,
    working: Vec<bool>,
    /// Jour chômé par une exception du calendrier (férié, congé posé).
    exception: Vec<bool>,
    prefix_working: Vec<usize>,
}

impl Span {
    fn len(&self) -> usize {
        self.working.len()
    }

    fn is_working(&self, i: usize) -> bool {
        self.working.get(i).copied().unwrap_or(false)
    }

    fn working_days(&self, a: usize, b: usize) -> usize {
        count(&self.prefix_working, a, b)
    }

    fn day(&self, i: usize) -> Option<Day> {
        i32::try_from(i)
            .ok()
            .and_then(|i| self.min.checked_add_days(i).ok())
    }

    /// Fermeture formée par `[a, b]` et les jours chômés qui la touchent
    /// (14 au plus de chaque côté) : jours calendaires, et jours chômés par
    /// une exception (fériés) qu'elle contient.
    fn closure(&self, a: usize, b: usize) -> (usize, usize) {
        let mut left = 0_usize;
        let mut i = a;
        while i > 0 && left < 14 {
            i = i.saturating_sub(1);
            if self.is_working(i) {
                break;
            }
            left = left.saturating_add(1);
        }
        let mut right = 0_usize;
        let mut j = b.saturating_add(1);
        while j < self.len() && right < 14 {
            if self.is_working(j) {
                break;
            }
            right = right.saturating_add(1);
            j = j.saturating_add(1);
        }
        let (from, to) = (a.saturating_sub(left), b.saturating_add(right));
        let holidays = (from..=to)
            .filter(|i| self.exception.get(*i).copied().unwrap_or(false))
            .count();
        (to.saturating_sub(from).saturating_add(1), holidays)
    }
}

/// Vrais de `pre` sur `[a, b]`.
fn count(pre: &[usize], a: usize, b: usize) -> usize {
    let hi = pre.get(b.saturating_add(1)).copied().unwrap_or(0);
    hi.saturating_sub(pre.get(a).copied().unwrap_or(0))
}

/// Feuilles lues `(début, fin, jours)` exploitables, et compteurs des
/// écartées (R-276).
fn readable_leaves(plan: &ImportedPlan, project: &Project, out: &mut Deduction) -> Vec<(Day, Day, usize)> {
    let options = &project.options;
    let per_day = i64::from(options.factors.minutes_per_day());
    let mut raw = Vec::new();
    if per_day <= 0 {
        return raw;
    }
    for t in &plan.tasks {
        if t.summary || t.milestone() {
            continue;
        }
        let (Some(start), Some(finish), Some(duration)) = (t.start, t.finish, t.duration.as_ref()) else {
            continue;
        };
        if duration.basis() != TimeBasis::Working || start.day() > finish.day() {
            continue;
        }
        let minutes = duration.minutes().get();
        let whole = minutes.checked_rem(per_day) == Some(0);
        let on_defaults = start.time_of_day() == options.default_start_time
            && finish.time_of_day() == options.default_finish_time;
        if !whole || !on_defaults {
            out.partial_days = out.partial_days.saturating_add(1);
            continue;
        }
        let days = minutes.checked_div(per_day).and_then(|d| usize::try_from(d).ok());
        if let Some(days) = days {
            raw.push((start.day(), finish.day(), days));
        }
    }
    raw
}

/// Ajoute 1 sur `[a, b]` à un tableau de différences.
fn mark(diff: &mut [i64], a: usize, b: usize) {
    if let Some(c) = diff.get_mut(a) {
        *c = c.saturating_add(1);
    }
    if let Some(c) = diff.get_mut(b.saturating_add(1)) {
        *c = c.saturating_sub(1);
    }
}

/// Jours couverts d'un tableau de différences.
fn covered(diff: &[i64], len: usize) -> Vec<bool> {
    let mut acc = 0_i64;
    diff.iter()
        .take(len)
        .map(|d| {
            acc = acc.saturating_add(*d);
            acc > 0
        })
        .collect()
}

/// Déduit les périodes chômées qui expliquent les écarts (R-276, R-277),
/// sur le calendrier du projet `project` (Standard, plus les jours fériés
/// déjà posés).
#[must_use]
#[allow(clippy::too_many_lines, reason = "cinq étapes linéaires commentées")]
pub fn deduce(plan: &ImportedPlan, project: &Project, limits: &DeduceLimits) -> Deduction {
    let mut out = Deduction::default();
    let raw = readable_leaves(plan, project, &mut out);
    let (Some(first), Some(last)) = (raw.iter().map(|r| r.0).min(), raw.iter().map(|r| r.1).max()) else {
        return out;
    };
    // Étendue des dates lues, élargie de deux semaines de chaque côté (la
    // fermeture d'une période voit les week-ends et fériés voisins).
    let min = first.checked_add_days(-MARGIN_DAYS).unwrap_or(Day::MIN);
    let max = last.checked_add_days(MARGIN_DAYS).unwrap_or(Day::MAX);
    let len = usize::try_from(max.index().saturating_sub(min.index()))
        .unwrap_or(usize::MAX)
        .saturating_add(1);
    if len > limits.max_span_days.saturating_add(MARGINS) {
        out.bounded = true;
        return out;
    }
    // 1. Jours ouvrés du calendrier du projet sur l'étendue.
    let mut working = Vec::with_capacity(len);
    let mut exception = Vec::with_capacity(len);
    let mut day = min;
    for _ in 0..len {
        let resolved = resolve_day(&project.calendars, project.calendar, day).ok();
        let open = resolved
            .as_ref()
            .is_some_and(|r| matches!(r.hours, DaySetting::Working(_)));
        working.push(open);
        exception.push(!open && resolved.is_some_and(|r| matches!(r.source, DaySource::Exception { .. })));
        day = day.checked_add_days(1).unwrap_or(day);
    }
    let span = Span {
        min,
        prefix_working: prefix(&working),
        working,
        exception,
    };
    let index = |d: Day| usize::try_from(d.index().saturating_sub(min.index())).unwrap_or(0);

    // 2. Preuves et équations (R-276).
    let mut leaves: Vec<Leaf> = Vec::new();
    let mut proven_diff = vec![0_i64; len.saturating_add(1)];
    let mut equation_diff = vec![0_i64; len.saturating_add(1)];
    for (s, f, days) in raw {
        let (first, last) = (index(s), index(f));
        let w = span.working_days(first, last);
        if !span.is_working(first) || !span.is_working(last) || w < days {
            out.contradicted = out.contradicted.saturating_add(1);
            continue;
        }
        out.usable = out.usable.saturating_add(1);
        if w == days {
            out.proven = out.proven.saturating_add(1);
            mark(&mut proven_diff, first, last);
        } else {
            out.equations = out.equations.saturating_add(1);
            mark(&mut equation_diff, first, last);
            leaves.push(Leaf {
                first,
                last,
                excess: w.saturating_sub(days),
            });
        }
    }
    let (proven, in_equation) = (covered(&proven_diff, len), covered(&equation_diff, len));
    let candidate: Vec<bool> = (0..len)
        .map(|i| {
            span.is_working(i)
                && in_equation.get(i).copied().unwrap_or(false)
                && !proven.get(i).copied().unwrap_or(false)
        })
        .collect();
    let prefix_candidate = prefix(&candidate);
    let leaves: Vec<Leaf> = leaves
        .into_iter()
        .filter(|l| {
            let ok = count(&prefix_candidate, l.first, l.last) >= l.excess;
            if !ok {
                out.impossible = out.impossible.saturating_add(1);
            }
            ok
        })
        .collect();

    // 3. Suites de candidats : jours ouvrés consécutifs du calendrier (un
    // jour chômé ne coupe pas la suite, un jour ouvré non candidat oui).
    let mut runs: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    for i in 0..len {
        if !span.is_working(i) {
            continue;
        }
        if candidate.get(i).copied().unwrap_or(false) {
            current.push(i);
        } else if !current.is_empty() {
            runs.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        runs.push(current);
    }

    // 4. Périodes par suite (R-277).
    let mut evaluations = 0_u64;
    let mut picks: Vec<Pick> = Vec::new();
    for run in &runs {
        match pick_in_run(run, &leaves, &prefix_candidate, &span, limits, &mut evaluations) {
            RunOutcome::Pick(p) => picks.push(p),
            RunOutcome::Nothing => out.ambiguous = out.ambiguous.saturating_add(1),
            RunOutcome::Bounded => {
                out.bounded = true;
                break;
            }
        }
    }

    // 5. Périodes ensemble : aucune équation dépassée (sinon la période qui
    // explique le moins est écartée), puis équations expliquées.
    let removed = |picks: &[Pick], l: &Leaf| -> usize {
        picks
            .iter()
            .map(|p| {
                let (a, b) = p.chosen();
                let (lo, hi) = (a.max(l.first), b.min(l.last));
                if hi >= lo { span.working_days(lo, hi) } else { 0 }
            })
            .sum()
    };
    for _ in 0..picks.len() {
        if !leaves.iter().any(|l| removed(&picks, l) > l.excess) {
            break;
        }
        let weakest = picks
            .iter()
            .enumerate()
            .min_by_key(|(_, p)| {
                leaves
                    .iter()
                    .filter(|l| removed(std::slice::from_ref(*p), l) > 0 && removed(&picks, l) == l.excess)
                    .count()
            })
            .map(|(k, _)| k);
        if let Some(k) = weakest {
            picks.remove(k);
        }
    }
    let explained_by = |picks: &[Pick]| -> BTreeSet<usize> {
        leaves
            .iter()
            .enumerate()
            .filter(|(_, l)| removed(picks, l) == l.excess)
            .map(|(k, _)| k)
            .collect()
    };
    // Un jour déterminé seul doit expliquer au moins deux équations (sinon :
    // demi-journée possible) ; une période qui n'explique rien est écartée.
    let explained = explained_by(&picks);
    let touching = |p: &Pick, explained: &BTreeSet<usize>| -> usize {
        let (a, b) = p.chosen();
        explained
            .iter()
            .filter_map(|k| leaves.get(*k))
            .filter(|l| l.first <= b && l.last >= a)
            .count()
    };
    let (kept, dropped): (Vec<Pick>, Vec<Pick>) = picks.into_iter().partition(|p| {
        let (a, b) = p.chosen();
        let n = touching(p, &explained);
        n > 0 && (span.working_days(a, b) > 1 || n >= 2)
    });
    out.ambiguous = out
        .ambiguous
        .saturating_add(u32::try_from(dropped.len()).unwrap_or(u32::MAX));
    let explained = explained_by(&kept);
    out.explained = u32::try_from(explained.len()).unwrap_or(u32::MAX);
    for p in &kept {
        let (a, b) = p.chosen();
        let options: Vec<(Day, Day)> = p
            .options
            .iter()
            .filter_map(|&(a, b)| Some((span.day(a)?, span.day(b)?)))
            .collect();
        if options.len() != p.options.len() {
            continue;
        }
        out.periods.push(ProposedPeriod {
            options,
            default_option: p.default_option,
            working_days: u32::try_from(span.working_days(a, b)).unwrap_or(u32::MAX),
            explained: u32::try_from(touching(p, &explained)).unwrap_or(u32::MAX),
        });
    }
    out
}

/// Réponse de l'examen d'une suite.
enum RunOutcome {
    Pick(Pick),
    Nothing,
    Bounded,
}

/// Meilleures périodes continues d'une suite (R-277).
#[allow(clippy::too_many_lines, reason = "évaluation, puis choix du placement")]
fn pick_in_run(
    run: &[usize],
    leaves: &[Leaf],
    prefix_candidate: &[usize],
    span: &Span,
    limits: &DeduceLimits,
    evaluations: &mut u64,
) -> RunOutcome {
    let (Some(&run_first), Some(&run_last)) = (run.first(), run.last()) else {
        return RunOutcome::Nothing;
    };
    // Équations qui touchent la suite : plage d'indices de la suite
    // qu'elles couvrent, jours à retirer, locales (toutes leurs candidates
    // dans la suite) ou non.
    let mut touching: Vec<(usize, usize, usize, bool)> = Vec::new();
    for l in leaves {
        if l.last < run_first || l.first > run_last {
            continue;
        }
        let a = run.partition_point(|d| *d < l.first);
        let b = run.partition_point(|d| *d <= l.last);
        if b <= a {
            continue;
        }
        let local = count(prefix_candidate, l.first, l.last) == b.saturating_sub(a);
        touching.push((a, b.saturating_sub(1), l.excess, local));
    }
    let mut best = 0_u32;
    let mut best_intervals: Vec<(usize, usize)> = Vec::new();
    let n = run.len();
    for i in 0..n {
        let end = n.min(i.saturating_add(limits.max_period_days));
        for j in i..end {
            *evaluations = evaluations.saturating_add(u64::try_from(touching.len()).unwrap_or(u64::MAX));
            if *evaluations > limits.max_evaluations {
                return RunOutcome::Bounded;
            }
            let mut score = 0_u32;
            let mut valid = true;
            for &(a, b, excess, local) in &touching {
                let (lo, hi) = (a.max(i), b.min(j));
                let overlap = if hi >= lo {
                    hi.saturating_sub(lo).saturating_add(1)
                } else {
                    0
                };
                if overlap > excess {
                    valid = false;
                    break;
                }
                if local && overlap == excess {
                    score = score.saturating_add(1);
                }
            }
            if !valid || score == 0 || score < best {
                continue;
            }
            if score > best {
                best = score;
                best_intervals.clear();
            }
            best_intervals.push((i, j));
        }
    }
    // Une seule période par suite : les équations locales qu'elle ne
    // satisfait pas la contredisent ; il lui faut la majorité stricte.
    let locals = touching.iter().filter(|t| t.3).count();
    let unsatisfied = u32::try_from(locals).unwrap_or(u32::MAX).saturating_sub(best);
    if best <= unsatisfied {
        return RunOutcome::Nothing;
    }
    let Some(shortest) = best_intervals.iter().map(|p| p.1.saturating_sub(p.0)).min() else {
        return RunOutcome::Nothing;
    };
    let smallest: Vec<(usize, usize)> = best_intervals
        .iter()
        .filter(|p| p.1.saturating_sub(p.0) == shortest)
        .copied()
        .collect();
    let to_days = |p: &(usize, usize)| {
        (
            run.get(p.0).copied().unwrap_or(0),
            run.get(p.1).copied().unwrap_or(0),
        )
    };
    if let [only] = smallest.as_slice()
        && best_intervals.iter().all(|p| p.0 <= only.0 && p.1 >= only.1)
    {
        return RunOutcome::Pick(Pick {
            options: vec![to_days(only)],
            default_option: 0,
        });
    }
    // Placements équivalents : à partir de 2 jours ouvrés seulement.
    if shortest == 0 {
        return RunOutcome::Nothing;
    }
    let options: Vec<(usize, usize)> = smallest.iter().map(to_days).collect();
    let (Some(first), Some(last)) = (options.first(), options.last()) else {
        return RunOutcome::Nothing;
    };
    let middle = first.0.saturating_add(last.1);
    let default_option = options
        .iter()
        .enumerate()
        .max_by_key(|&(k, &(a, b))| {
            let centre = a.saturating_add(b).abs_diff(middle);
            let (length, holidays) = span.closure(a, b);
            (length, holidays, std::cmp::Reverse(centre), std::cmp::Reverse(k))
        })
        .map_or(0, |(k, _)| k);
    RunOutcome::Pick(Pick {
        options,
        default_option,
    })
}

/// Sommes préfixes d'une suite de booléens (`pre[i]` = vrais avant `i`).
fn prefix(flags: &[bool]) -> Vec<usize> {
    let mut pre = Vec::with_capacity(flags.len().saturating_add(1));
    let mut acc = 0_usize;
    pre.push(0);
    for f in flags {
        acc = acc.saturating_add(usize::from(*f));
        pre.push(acc);
    }
    pre
}
