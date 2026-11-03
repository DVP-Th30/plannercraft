//! Cumuls du travail et surutilisation des ressources (ADR-0013 D6 ; règles
//! R-184 à R-187, lot L5).
//!
//! Passe de sortie du moteur, après le placement des affectations
//! ([`apply`]) :
//! - travail des tâches ([`super::TaskSchedule::work`]) et des ressources
//!   ([`super::ResourceLoad::work`]) : sommes des affectations de travail
//!   (R-184), ressources matérielles et de coût à part (R-185) ;
//! - débit des affectations dont `d_a` ne vient pas des unités (durée fixe,
//!   unités nulles) : travail ÷ temps ouvré du span (R-186) ;
//! - surutilisation exacte (R-186, R-187) : balayage des bornes des spans
//!   d'une ressource ; sur chaque intervalle élémentaire, débits regroupés
//!   par échelle effective. Une seule échelle : la charge vaut la somme sur
//!   ses minutes ouvrées, rien à énumérer pour le pic. Plusieurs échelles :
//!   plages ouvrées énumérées, seulement si la somme dépasse la capacité ou
//!   le pic courant. Énumération bornée à [`MAX_RANGES`] plages par calcul
//!   (règle 10), sortie à [`super::MAX_OVERLOADS`] intervalles par
//!   ressource ; au-delà, `truncated` ;
//! - diagnostics `resource.overallocated` (un par ressource) et
//!   `resource.availability_ignored`.
//!
//! L'utilisation par période (R-188) se calcule à la demande
//! ([`crate::resource::usage`]) avec les mêmes échelles ([`Scales`]).

use std::borrow::Cow;
use std::collections::BTreeMap;

use super::network::{Network, Role};
use super::placement::Resolved;
use super::{
    AssignmentRole, EffectiveCalendar, MAX_OVERLOADS, Overload, ResourceLoad, Schedule, TaskOutcome,
};
use crate::calendar::{CalendarSet, CompiledCalendar, Span, TimeScale};
use crate::diagnostic::Diagnostic;
use crate::duration::{Minutes, Work};
use crate::fixed::round_div;
use crate::ids::{CalendarUid, ResourceUid};
use crate::model::{Project, Resource, ResourceKind, TaskType};
use crate::quantity::Units;
use crate::time::{Moment, TimeOfDay};

/// Plages ouvrées énumérées au plus par calcul de surutilisation (règle 10,
/// R-186) ; au-delà, les ressources restantes sont marquées `truncated`.
pub const MAX_RANGES: u64 = 2_000_000;

/// Clé `Extras` des disponibilités datées relues (`AvailabilityPeriods`,
/// R-187).
const AVAILABILITY_KEY: &str = "AvailabilityPeriods";

/// R-113 : calendrier qui place les tâches sans affectation de travail ni
/// calendrier propre (échelle `Project` d'une affectation, R-175), comme le
/// moteur : base du calendrier du projet s'il est dérivé, Standard intégré
/// s'il est inutilisable (R-066). `None` : repli impossible.
pub(crate) fn project_place<'s>(
    project: &Project,
    set: &'s CalendarSet,
) -> Option<Cow<'s, CompiledCalendar>> {
    match set.get(project.calendar) {
        Ok(c) if c.has_working_time() => Some(Cow::Borrowed(super::derived_base(project, set).unwrap_or(c))),
        _ => super::compile_builtin_standard(project).map(Cow::Owned),
    }
}

/// Échelles effectives des affectations (R-171), par [`EffectiveCalendar`] :
/// calendriers compilés du projet, intersections reprises du placement
/// (recalcul du moteur, aucune recompilation : la mémoire n'est pas
/// doublée) ou recompilées et mises en cache par paire (utilisation par
/// période, à la demande).
pub(crate) struct Scales<'s> {
    /// Calendriers compilés du projet.
    set: &'s CalendarSet,
    /// Calendrier de placement du projet ([`project_place`]).
    place: &'s CompiledCalendar,
    /// Intersections gardées par le placement du même recalcul.
    resolved: Option<&'s Resolved>,
    /// Intersections compilées.
    owned: Vec<CompiledCalendar>,
    /// Intersection par paire (tâche, ressource) ; `None` : vide ou en échec.
    pairs: BTreeMap<(CalendarUid, CalendarUid), Option<usize>>,
}

impl<'s> Scales<'s> {
    /// Échelles vides.
    pub(crate) const fn new(set: &'s CalendarSet, place: &'s CompiledCalendar) -> Self {
        Self {
            set,
            place,
            resolved: None,
            owned: Vec::new(),
            pairs: BTreeMap::new(),
        }
    }

    /// Échelles du recalcul : intersections du placement (`resolved`).
    const fn of_resolved(set: &'s CalendarSet, place: &'s CompiledCalendar, resolved: &'s Resolved) -> Self {
        Self {
            set,
            place,
            resolved: Some(resolved),
            owned: Vec::new(),
            pairs: BTreeMap::new(),
        }
    }

    /// Compile l'intersection de `cal` si besoin (avant [`Scales::get`]).
    pub(crate) fn prepare(&mut self, cal: EffectiveCalendar) {
        let EffectiveCalendar::Intersection { task, resource } = cal else {
            return;
        };
        if task == resource || self.resolved.is_some() || self.pairs.contains_key(&(task, resource)) {
            return;
        }
        let slot = match (self.set.get(task), self.set.get(resource)) {
            (Ok(a), Ok(b)) => a.intersect(b).ok().filter(CompiledCalendar::has_working_time),
            _ => None,
        }
        .map(|c| {
            self.owned.push(c);
            self.owned.len().saturating_sub(1)
        });
        self.pairs.insert((task, resource), slot);
    }

    /// Échelle de `cal` ; `None` si son calendrier est inutilisable ou si
    /// l'intersection n'a pas été préparée.
    pub(crate) fn get(&self, cal: EffectiveCalendar) -> Option<TimeScale<'_>> {
        let usable = |uid: CalendarUid| {
            self.set
                .get(uid)
                .ok()
                .filter(|c| c.has_working_time())
                .map(TimeScale::Working)
        };
        match cal {
            EffectiveCalendar::Task(c) | EffectiveCalendar::Resource(c) => usable(c),
            EffectiveCalendar::Intersection { task, resource } if task == resource => usable(task),
            EffectiveCalendar::Intersection { task, resource } => {
                if let Some(r) = self.resolved {
                    return r.intersection(task, resource).map(TimeScale::Working);
                }
                let i = (*self.pairs.get(&(task, resource))?)?;
                self.owned.get(i).map(TimeScale::Working)
            }
            EffectiveCalendar::Project(_) => Some(TimeScale::Working(self.place)),
            EffectiveCalendar::Elapsed => Some(TimeScale::Elapsed),
        }
    }
}

/// Ressource de travail connue du projet (R-184 : seules ses affectations
/// font le travail d'une tâche).
fn work_resource(project: &Project, resource: Option<ResourceUid>) -> bool {
    resource
        .and_then(|r| project.resources.get(r))
        .is_some_and(|r| r.kind == ResourceKind::Work)
}

/// Travail borné au domaine de [`Work`] (somme saturée, règle 10).
fn clamp_work(v: i128) -> Work {
    let m = v.clamp(0, i128::from(Work::MAX_MINUTES));
    i64::try_from(m)
        .ok()
        .and_then(|m| Work::new(Minutes::new(m)).ok())
        .unwrap_or(Work::ZERO)
}

/// Unités bornées au domaine de [`Units`].
fn clamp_units(v: i128) -> Units {
    let u = v.clamp(0, i128::from(Units::MAX));
    i64::try_from(u)
        .ok()
        .and_then(|u| Units::new(u).ok())
        .unwrap_or(Units::ZERO)
}

/// Débit `round_div(W × 10 000, m)` (R-186) ; `None` pour `m` nul ou hors
/// domaine.
fn rate(work: Work, m: Minutes) -> Option<Units> {
    let num = i128::from(work.minutes().get()).checked_mul(10_000)?;
    Units::new(round_div(num, i128::from(m.get()))?).ok()
}

/// Passe de sortie : travail des tâches et des ressources (R-184, R-185),
/// débits (R-186), charges et surutilisation (R-186, R-187). `place` :
/// calendrier de placement du projet (échelle `Project`).
pub(super) fn apply(
    project: &Project,
    net: &Network<'_>,
    (set, place, resolved): (&CalendarSet, &CompiledCalendar, &Resolved),
    out: &mut Schedule,
) {
    // Une intersection d'une affectation rangée a été gardée par le
    // placement (sinon son calendrier effectif est celui de la tâche,
    // R-171) : rien à recompiler.
    let scales = Scales::of_resolved(set, place, resolved);
    effective_rates(project, &scales, out);
    let (own, by_resource) = model_work(project, net);
    task_work(net, own, out);
    let mut diags = Vec::new();
    out.resources = resource_loads(project, &by_resource, &scales, out, &mut diags);
    if !diags.is_empty() {
        out.diagnostics.extend(diags);
        out.diagnostics.sort();
        out.diagnostics.dedup();
    }
}

/// R-186 : débit des affectations de travail dont `d_a` ne vient pas des
/// unités (tâche à durée fixe, unités nulles, R-170, R-181) : travail ÷
/// temps ouvré du span sur l'échelle effective ; unités gardées si ce temps
/// est nul. Les tâches manuelles l'ont déjà (R-183).
fn effective_rates(project: &Project, scales: &Scales<'_>, out: &mut Schedule) {
    let any_fixed_duration = project
        .tasks
        .iter()
        .any(|t| t.task_type == TaskType::FixedDuration);
    for a in out.assignments.values_mut() {
        if !matches!(a.role, AssignmentRole::Work | AssignmentRole::Summary)
            || (!any_fixed_duration && a.effective_units != Units::ZERO)
            || !work_resource(project, a.resource)
        {
            continue;
        }
        let fixed_duration = any_fixed_duration
            && project
                .tasks
                .get(a.task)
                .is_some_and(|t| t.task_type == TaskType::FixedDuration);
        if !fixed_duration && a.effective_units != Units::ZERO {
            continue;
        }
        let measured = scales
            .get(a.calendar)
            .and_then(|s| s.between(a.span.start, a.span.finish).ok())
            .filter(|m| m.get() > 0);
        if let Some(u) = measured.and_then(|m| rate(a.work, m)) {
            a.effective_units = u;
        }
    }
}

/// Travail propre de chaque tâche active (par position : Σ W de ses
/// affectations à des ressources de travail) et travail de chaque
/// ressource (Σ W de ses affectations des tâches actives), lus sur les
/// affectations du modèle : une tâche planifiée dégradée garde son travail.
fn model_work(project: &Project, net: &Network<'_>) -> (Vec<i128>, BTreeMap<ResourceUid, i128>) {
    let mut own: Vec<i128> = vec![0; net.tasks.len()];
    let mut by_resource: BTreeMap<ResourceUid, i128> = BTreeMap::new();
    // Jointure par UID de tâche (deux listes triées) plutôt qu'une recherche
    // de position par affectation.
    let mut positions: Vec<(crate::ids::TaskUid, usize)> =
        net.tasks.iter().enumerate().map(|(p, i)| (i.uid, p)).collect();
    positions.sort_unstable();
    let mut items: Vec<(crate::ids::TaskUid, ResourceUid, i128)> = project
        .assignments
        .iter()
        .filter_map(|a| Some((a.task, a.resource?, i128::from(a.work.minutes().get()))))
        .collect();
    items.sort_unstable_by_key(|e| e.0);
    let mut pos = positions.into_iter().peekable();
    let mut kinds: BTreeMap<ResourceUid, bool> = BTreeMap::new();
    for (task, r, w) in items {
        while pos.next_if(|e| e.0 < task).is_some() {}
        let Some(&(uid, p)) = pos.peek() else { break };
        if uid != task
            || !net
                .tasks
                .get(p)
                .is_some_and(|i| i.role != Role::Null && !i.inactive())
        {
            continue;
        }
        let slot = by_resource.entry(r).or_insert(0);
        *slot = slot.saturating_add(w);
        let is_work = *kinds.entry(r).or_insert_with(|| work_resource(project, Some(r)));
        if is_work && let Some(t) = own.get_mut(p) {
            *t = t.saturating_add(w);
        }
    }
    (own, by_resource)
}

/// R-184 : travail des tâches. Feuille : son travail propre ;
/// récapitulative : Σ de ses enfants + ses propres affectations de travail ;
/// UID 0 : Σ des tâches de premier niveau.
fn task_work(net: &Network<'_>, mut total: Vec<i128>, out: &mut Schedule) {
    // Enfants après leur parent (ordre des ID) : remontée en sens inverse.
    let mut top: i128 = 0;
    for p in (0..net.tasks.len()).rev() {
        let Some(info) = net.tasks.get(p) else { continue };
        let own = total.get(p).copied().unwrap_or(0);
        match info.parent {
            Some(parent) => {
                if let Some(t) = total.get_mut(parent) {
                    *t = t.saturating_add(own);
                }
            }
            None if info.role != Role::Null => top = top.saturating_add(own),
            None => {}
        }
    }
    // Jointure par UID (les deux côtés triés) plutôt qu'une recherche par
    // tâche.
    let mut by_uid: Vec<(crate::ids::TaskUid, i128)> = net
        .tasks
        .iter()
        .zip(total)
        .filter(|(_, w)| *w != 0)
        .map(|(i, w)| (i.uid, w))
        .collect();
    by_uid.sort_unstable_by_key(|e| e.0);
    let mut it = by_uid.into_iter().peekable();
    for (uid, outcome) in &mut out.tasks {
        while it.next_if(|e| e.0 < *uid).is_some() {}
        if let Some((_, w)) = it.next_if(|e| e.0 == *uid)
            && let TaskOutcome::Scheduled(ts) = outcome
        {
            ts.work = clamp_work(w);
        }
    }
    if let Some(ps) = out.project_summary.as_mut() {
        ps.work = clamp_work(top);
    }
}

/// R-187 : ressource soumise au contrôle de surutilisation : de travail,
/// active, UID ≠ 0 (génériques comprises).
fn in_scope(r: &Resource) -> bool {
    r.kind == ResourceKind::Work && !r.is_inactive && r.uid() != ResourceUid::new(0)
}

/// R-184, R-185, R-186, R-187 : une charge par ressource du projet (UID 0
/// exclu). Travail : Σ W des affectations des tâches actives (matérielle :
/// quantité ; coût : nul). Surutilisation : ressources de [`in_scope`]
/// seulement.
fn resource_loads(
    project: &Project,
    work: &BTreeMap<ResourceUid, i128>,
    scales: &Scales<'_>,
    out: &Schedule,
    diags: &mut Vec<Diagnostic>,
) -> BTreeMap<ResourceUid, ResourceLoad> {
    // Affectations placées par ressource, dans l'ordre des UID.
    let mut placed: BTreeMap<ResourceUid, Vec<Item>> = BTreeMap::new();
    let mut keys: BTreeMap<EffectiveCalendar, usize> = BTreeMap::new();
    let mut list: Vec<TimeScale<'_>> = Vec::new();
    for a in out.assignments.values() {
        let Some(r) = a.resource else { continue };
        if a.span.start >= a.span.finish || a.effective_units == Units::ZERO {
            continue;
        }
        let key = if let Some(&k) = keys.get(&a.calendar) {
            k
        } else {
            let Some(s) = scales.get(a.calendar) else { continue };
            list.push(s);
            let k = list.len().saturating_sub(1);
            keys.insert(a.calendar, k);
            k
        };
        placed.entry(r).or_default().push(Item {
            span: a.span,
            rate: i128::from(a.effective_units.get()),
            scale: key,
        });
    }
    let mut budget = MAX_RANGES;
    let mut loads = BTreeMap::new();
    for r in &project.resources {
        let uid = r.uid();
        if uid == ResourceUid::new(0) {
            continue;
        }
        let w = match r.kind {
            ResourceKind::Cost => Work::ZERO,
            ResourceKind::Work | ResourceKind::Material { .. } => {
                clamp_work(work.get(&uid).copied().unwrap_or(0))
            }
        };
        let mut acc = Acc::default();
        if in_scope(r) {
            if r.extras.0.iter().any(|e| e.key == AVAILABILITY_KEY) {
                diags.push(Diagnostic::ResourceAvailabilityIgnored { resource: uid });
            }
            if let Some(items) = placed.get(&uid) {
                sweep(items, &list, i128::from(r.max_units.get()), &mut budget, &mut acc);
            }
            if acc.overallocated {
                let first = acc.overloads.first().copied().or(acc.hint);
                if let Some(o) = first {
                    diags.push(Diagnostic::ResourceOverallocated {
                        resource: uid,
                        from: o.from,
                        to: o.to,
                        peak: o.peak,
                    });
                }
            }
        }
        loads.insert(
            uid,
            ResourceLoad {
                work: w,
                peak: clamp_units(acc.peak),
                overallocated: acc.overallocated,
                overloads: acc.overloads,
                truncated: acc.truncated,
            },
        );
    }
    loads
}

/// Affectation placée d'une ressource, pour le balayage.
#[derive(Clone, Copy, Debug)]
struct Item {
    /// Span (non vide).
    span: Span,
    /// Débit (1/10 000, > 0).
    rate: i128,
    /// Échelle effective (indice dans la liste des échelles).
    scale: usize,
}

/// Résultat du balayage d'une ressource.
#[derive(Debug, Default)]
struct Acc {
    /// Pic de la charge (1/10 000).
    peak: i128,
    /// Une minute dépasse la capacité.
    overallocated: bool,
    /// Intervalles maximaux, dans l'ordre.
    overloads: Vec<Overload>,
    /// Sortie coupée (plafond d'intervalles ou de plages).
    truncated: bool,
    /// Budget de plages épuisé : plus aucune énumération.
    exhausted: bool,
    /// Premier intervalle surutilisé trouvé sans énumération (budget
    /// épuisé), pour le diagnostic.
    hint: Option<Overload>,
    /// Tampon des plages énumérées (réutilisé).
    ranges: Vec<(Moment, Moment)>,
    /// Tampon des variations de charge (réutilisé).
    deltas: Vec<(Moment, i128)>,
}

impl Acc {
    /// Minute(s) `[from, to)` à la charge `load` > capacité : fusion avec
    /// l'intervalle précédent s'il est contigu.
    fn push(&mut self, from: Moment, to: Moment, load: i128) {
        self.overallocated = true;
        let peak = clamp_units(load);
        if let Some(last) = self.overloads.last_mut()
            && last.to == from
        {
            last.to = to;
            last.peak = last.peak.max(peak);
            return;
        }
        if self.overloads.len() < MAX_OVERLOADS {
            self.overloads.push(Overload { from, to, peak });
        } else {
            self.truncated = true;
        }
    }

    /// Les intervalles sont encore utiles (ni coupés ni à budget épuisé).
    const fn wants_intervals(&self) -> bool {
        !self.truncated && !self.exhausted
    }
}

/// R-186 : balayage des affectations d'une ressource de capacité `cap`.
fn sweep(items: &[Item], scales: &[TimeScale<'_>], cap: i128, budget: &mut u64, acc: &mut Acc) {
    // Bornes : (instant, indice, début ?) ; à instant égal, l'ordre importe
    // peu (l'intervalle élémentaire suit l'application de toutes).
    let mut events: Vec<(Moment, usize, bool)> = Vec::with_capacity(items.len().saturating_mul(2));
    for (i, it) in items.iter().enumerate() {
        events.push((it.span.start, i, true));
        events.push((it.span.finish, i, false));
    }
    events.sort_unstable();
    // Débits actifs par échelle : (somme, nombre d'affectations).
    let mut active: BTreeMap<usize, (i128, u32)> = BTreeMap::new();
    let mut k = 0;
    while let Some(&(t, _, _)) = events.get(k) {
        while let Some(&(_, i, start)) = events.get(k).filter(|e| e.0 == t) {
            k = k.saturating_add(1);
            let Some(it) = items.get(i) else { continue };
            if start {
                let e = active.entry(it.scale).or_insert((0, 0));
                e.0 = e.0.saturating_add(it.rate);
                e.1 = e.1.saturating_add(1);
            } else if let Some(e) = active.get_mut(&it.scale) {
                e.0 = e.0.saturating_sub(it.rate);
                e.1 = e.1.saturating_sub(1);
                if e.1 == 0 {
                    active.remove(&it.scale);
                }
            }
        }
        let Some(&(hi, _, _)) = events.get(k) else { break };
        if active.is_empty() {
            continue;
        }
        elementary(&active, scales, (t, hi), (cap, budget), acc);
    }
}

/// Intervalle élémentaire `[lo, hi)` d'actifs constants.
fn elementary(
    active: &BTreeMap<usize, (i128, u32)>,
    scales: &[TimeScale<'_>],
    (lo, hi): (Moment, Moment),
    (cap, budget): (i128, &mut u64),
    acc: &mut Acc,
) {
    let total = active.values().fold(0i128, |s, v| s.saturating_add(v.0));
    if active.len() == 1 {
        let Some((&k, &(sum, _))) = active.iter().next() else {
            return;
        };
        let Some(scale) = scales.get(k).copied() else {
            return;
        };
        if sum > cap && acc.wants_intervals() {
            let mut ranges = std::mem::take(&mut acc.ranges);
            ranges.clear();
            let complete = working_ranges(scale, (lo, hi), budget, &mut ranges);
            if complete {
                if !ranges.is_empty() {
                    acc.peak = acc.peak.max(sum);
                }
                for &(a, b) in &ranges {
                    acc.push(a, b, sum);
                }
            }
            acc.ranges = ranges;
            if complete {
                return;
            }
            acc.exhausted = true;
            acc.truncated = true;
        }
        // Sans énumération : la charge vaut `sum` sur toute minute ouvrée.
        if (sum > acc.peak || (sum > cap && !acc.overallocated))
            && scale.between(lo, hi).is_ok_and(|m| m.get() > 0)
        {
            acc.peak = acc.peak.max(sum);
            if sum > cap && !acc.overallocated {
                acc.overallocated = true;
                acc.hint = Some(Overload {
                    from: lo,
                    to: hi,
                    peak: clamp_units(sum),
                });
            }
        }
        return;
    }
    // Plusieurs échelles : la charge varie avec leurs plages ouvrées.
    let needed = (total > cap && acc.wants_intervals()) || total > acc.peak;
    if !needed || acc.exhausted {
        return;
    }
    let mut deltas = std::mem::take(&mut acc.deltas);
    let mut ranges = std::mem::take(&mut acc.ranges);
    deltas.clear();
    let mut complete = true;
    for (&k, &(sum, _)) in active {
        let Some(scale) = scales.get(k).copied() else {
            continue;
        };
        ranges.clear();
        if !working_ranges(scale, (lo, hi), budget, &mut ranges) {
            complete = false;
            break;
        }
        for &(a, b) in &ranges {
            deltas.push((a, sum));
            deltas.push((b, sum.saturating_neg()));
        }
    }
    acc.ranges = ranges;
    if !complete {
        acc.exhausted = true;
        acc.truncated = true;
        acc.deltas = deltas;
        return;
    }
    deltas.sort_unstable();
    let mut load: i128 = 0;
    let mut j = 0;
    while let Some(&(t, _)) = deltas.get(j) {
        while let Some(&(_, d)) = deltas.get(j).filter(|e| e.0 == t) {
            load = load.saturating_add(d);
            j = j.saturating_add(1);
        }
        let Some(&(next, _)) = deltas.get(j) else { break };
        if load > 0 {
            acc.peak = acc.peak.max(load);
            if load > cap {
                if acc.wants_intervals() {
                    acc.push(t, next, load);
                } else {
                    acc.overallocated = true;
                }
            }
        }
    }
    acc.deltas = deltas;
}

/// Plages ouvrées de `scale` dans `[lo, hi)`, dans l'ordre (temps écoulé :
/// la plage entière). Faux si le budget est épuisé (plages partielles).
pub(crate) fn working_ranges(
    scale: TimeScale<'_>,
    (lo, hi): (Moment, Moment),
    budget: &mut u64,
    out: &mut Vec<(Moment, Moment)>,
) -> bool {
    let cal = match scale {
        TimeScale::Elapsed => {
            if lo < hi {
                out.push((lo, hi));
            }
            return true;
        }
        TimeScale::Working(cal) => cal,
    };
    let mut cursor = lo;
    while cursor < hi {
        let Some(left) = budget.checked_sub(1) else {
            return false;
        };
        *budget = left;
        let day = cursor.day();
        let Ok(ranges) = cal.day_ranges(day) else { break };
        let Ok(midnight) = Moment::at(day, TimeOfDay::MIDNIGHT) else {
            break;
        };
        let mut found = false;
        for range in ranges {
            let at = |t: TimeOfDay| midnight.checked_add(Minutes::new(i64::from(t.minutes())));
            let (Ok(from), Ok(to)) = (at(range.from()), at(range.to())) else {
                continue;
            };
            let (from, to) = (from.max(cursor), to.min(hi));
            if from >= to {
                continue;
            }
            let Some(left) = budget.checked_sub(1) else {
                return false;
            };
            *budget = left;
            found = true;
            out.push((from, to));
        }
        let next = day
            .checked_add_days(1)
            .ok()
            .and_then(|d| Moment::at(d, TimeOfDay::MIDNIGHT).ok());
        let next = match next {
            // Jour sans plage restante : saut à la prochaine minute ouvrée.
            Some(n) if !found => match cal.next_working_moment(n) {
                Ok(u) => u,
                Err(_) => break,
            },
            Some(n) => n,
            None => break,
        };
        if next <= cursor {
            break;
        }
        cursor = next;
    }
    true
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::*;
    use crate::calendar::builtin;
    use crate::time::Day;

    fn at(d: i32, h: u16) -> Moment {
        Moment::at(Day::new(d).unwrap(), TimeOfDay::hm(h, 0).unwrap()).unwrap()
    }

    #[test]
    fn ranges_skip_nights_and_clip() {
        let std = builtin::standard(CalendarUid::new(1), "Standard".to_owned());
        let arena = crate::Arena::try_from(vec![std]).unwrap();
        let c = CompiledCalendar::compile(&arena, CalendarUid::new(1)).unwrap();
        // 1984-01-02 est un lundi (jour 1).
        let mut out = Vec::new();
        let mut budget = 100;
        assert!(working_ranges(
            TimeScale::Working(&c),
            (at(1, 10), at(2, 14)),
            &mut budget,
            &mut out
        ));
        assert_eq!(
            out,
            vec![
                (at(1, 10), at(1, 12)),
                (at(1, 13), at(1, 17)),
                (at(2, 8), at(2, 12)),
                (at(2, 13), at(2, 14)),
            ]
        );
        let mut tiny = 2;
        assert!(!working_ranges(
            TimeScale::Working(&c),
            (at(1, 8), at(5, 17)),
            &mut tiny,
            &mut Vec::new()
        ));
    }

    #[test]
    fn acc_merges_contiguous_and_caps() {
        let mut acc = Acc::default();
        acc.push(at(1, 8), at(1, 12), 20_000);
        acc.push(at(1, 12), at(1, 13), 30_000);
        assert_eq!(acc.overloads.len(), 1);
        assert_eq!(acc.overloads[0].peak, Units::new(30_000).unwrap());
        for i in 0..2_000u16 {
            let d = i32::from(i).saturating_add(10);
            acc.push(at(d, 8), at(d, 9), 20_000);
        }
        assert_eq!(acc.overloads.len(), MAX_OVERLOADS);
        assert!(acc.truncated && acc.overallocated);
    }
}
