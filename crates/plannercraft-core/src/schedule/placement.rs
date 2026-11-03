//! Placement d'une feuille affectée, affectation par affectation (ADR-0013
//! D1 ; règles R-170, R-171, R-173, R-174, R-176, R-177 ; lot L4a).
//!
//! Trois temps :
//! 1. [`resolve`], avant le réseau : pour chaque affectation, ressource de
//!    travail ou affectation neutre (R-176), calendrier effectif (R-171,
//!    intersections compilées et mises en cache par paire, R-172), durée
//!    `d_a` (R-170) ; pour une tâche dont les affectations de travail ont
//!    plusieurs calendriers effectifs, leur union (échelle représentative,
//!    R-173). Les calendriers combinés sont possédés par [`Resolved`], qui
//!    vit aussi longtemps que le réseau.
//! 2. [`attach`], pendant la construction du réseau : les feuilles
//!    automatiques actives à durée ouvrée reçoivent leurs parts (`Part`,
//!    rangées en CSR par position de tâche, triées par UID d'affectation) ;
//!    chaque calendrier effectif devient une échelle de [`TimeScale`].
//! 3. Les passes placent la feuille par un [`Placer`] : une échelle et une
//!    durée (chemin J4, une seule affectation sans délai comprise), ou ses
//!    parts. La sortie ([`Calc::place_assignments`]) range les
//!    [`AssignmentSchedule`] et la durée calculée des feuilles (mesure de
//!    l'union des intervalles travaillés, R-173).
//!
//! Placement d'une feuille à parts, depuis une borne `x` (R-173, R-177) :
//! - part `a` : `s_a = forward_span_a(add_a(x, delay_a), d_a)` sur son
//!   échelle ;
//! - début de la tâche : `T(x) = min_a forward_span_a(x, d_a).start`, le
//!   premier instant où une ressource affectée peut travailler (les délais ne
//!   décalent pas la tâche, S45) ; `T` est idempotente et les parts placées
//!   depuis `T(x)` sont celles placées depuis `x` (aucune part n'a de temps
//!   ouvré dans `[x, T(x))`) : le début planifié suffit à les retrouver ;
//! - fin : `max_a f_a`.
//!
//! Passe arrière et contraintes de fin (R-174) : `LS = min_a
//! (backward_span_a(y, d_a).start − delay_a)` sur l'échelle de chaque part,
//! dates au plus tard `F(LS)` ; dernier début plaçable sous un plafond de
//! début `y` : `max_a latest_start_a(y, d_a)` (exact pour `T`).

use std::collections::BTreeMap;

use super::constraint::Mode;
use super::network::{ELAPSED, Fail, PROJECT, Role, TaskInfo, ix, pos};
use super::{AssignmentRole, AssignmentSchedule, Calc, EffectiveCalendar};
use crate::calendar::{CalendarSet, CompiledCalendar, Span, TimeScale};
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::duration::{Duration, Minutes, TimeBasis, Work};
use crate::error::CalendarError;
use crate::fixed::round_div;
use crate::ids::{AssignmentUid, CalendarUid, ResourceUid, TaskUid};
use crate::model::{
    ConstraintKind, LevelingDelay, Project, ResourceKind, SchedulingMode, Task, TaskType, WorkContour,
};
use crate::quantity::Units;
use crate::time::Moment;

/// Code de l'échec d'un `d_a` hors du domaine des durées (R-170, R-181).
const OUT_OF_DOMAIN: &str = "assignment.duration_out_of_domain";

/// Plafond des unions mises en cache pour la mesure de la durée d'une
/// feuille (règle 10) ; au-delà, la mesure se replie sur le plus grand des
/// temps ouvrés des échelles actives.
const UNION_CACHE_LIMIT: usize = 256;

/// Unions compilées par ensemble trié d'échelles (mesure de la durée d'une
/// feuille, R-173) ; `None` : union trop morcelée.
type UnionCache = BTreeMap<Vec<u32>, Option<CompiledCalendar>>;

/// Segments combinés gardés par recalcul, par défaut (`Limits`, règle 10) :
/// environ 128 Mo de segments (32 octets chacun) ; 20 000 intersections ou
/// unions de calendriers à 200 segments (Standard à 100 fériés).
pub(super) const DEFAULT_COMBINED_SEGMENTS: usize = 4_000_000;

/// Travail des combinaisons (segments lus) permis par segment gardé.
const WORK_FACTOR: usize = 8;

/// Parts d'une feuille au plus pour la mesure exacte de l'union (R-173,
/// règle 10) ; au-delà, durée = plus grand `d_a`,
/// `assignment.too_many_parts`.
pub(super) const MAX_MEASURED_PARTS: usize = 1_000;

/// Budget des calendriers combinés d'un recalcul (R-172, règle 10) : le
/// travail (segments lus par chaque combinaison) et les segments gardés
/// sont décomptés ; une combinaison qui le dépasse n'est pas faite.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Budget {
    /// Segments gardés restants.
    kept: usize,
    /// Travail restant.
    work: usize,
    /// Plafond (segments gardés), pour le diagnostic.
    limit: usize,
}

impl Budget {
    /// Budget de `limit` segments gardés.
    const fn new(limit: usize) -> Self {
        Self {
            kept: limit,
            work: limit.saturating_mul(WORK_FACTOR),
            limit,
        }
    }

    /// Réserve le travail d'une combinaison de calendriers de `cost`
    /// segments ; faux si le budget est épuisé.
    fn admit(&mut self, cost: usize) -> bool {
        if cost > self.work {
            return false;
        }
        self.work = self.work.saturating_sub(cost);
        true
    }

    /// Garde le calendrier combiné `c` ; faux s'il dépasse le budget.
    fn keep(&mut self, c: &CompiledCalendar) -> bool {
        let n = c.segment_count();
        if n > self.kept {
            return false;
        }
        self.kept = self.kept.saturating_sub(n);
        true
    }

    /// Plafond, pour `calendar.too_many_segments`.
    fn limit_u32(&self) -> u32 {
        u32::try_from(self.limit).unwrap_or(u32::MAX)
    }
}

/// Calendrier effectif résolu, avant l'indexation des échelles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum CalRef {
    /// Calendrier compilé du modèle (`CalendarSet`).
    Set(CalendarUid),
    /// Échelle du calendrier du projet, `PROJECT` (R-175).
    Project,
    /// Temps écoulé, `ELAPSED` (tâche à durée écoulée, R-182).
    Elapsed,
    /// Calendrier combiné possédé par [`Resolved`] (intersection ou union).
    Owned(usize),
}

/// Affectation résolue.
#[derive(Clone, Copy, Debug)]
struct Resolution {
    /// UID de l'affectation.
    uid: AssignmentUid,
    /// Position de la tâche dans le modèle.
    task_pos: usize,
    /// Tâche.
    task: TaskUid,
    /// Ressource (`None` : affectation fictive −65 535).
    resource: Option<ResourceUid>,
    /// Ressource de travail connue : l'affectation peut placer la tâche
    /// (R-176 sinon).
    work_resource: bool,
    /// Calendrier effectif (R-171).
    cal: CalRef,
    /// Calendrier effectif, pour la sortie.
    calendar: EffectiveCalendar,
    /// Calendrier de la tâche seule (affectation neutre, R-176).
    task_calendar: EffectiveCalendar,
    /// `d_a` (R-170), ou le code de son échec.
    duration: Result<Minutes, &'static str>,
    /// Délai d'affectation (R-177).
    delay: Minutes,
    /// Retard de nivellement de l'affectation (R-178).
    leveling: Option<LevelingDelay>,
    /// Profil de travail (R-180).
    contour: WorkContour,
    /// Travail.
    work: Work,
    /// Unités.
    units: Units,
    /// Tâche en cours (R-209) : `(d_act, d_rem)`, durées de la partie
    /// réelle et du reste de l'affectation, ou le code de leur échec.
    progress: Option<Result<(Minutes, Minutes), &'static str>>,
}

/// Résultat de [`resolve`] : décisions par affectation et calendriers
/// combinés possédés.
pub(crate) struct Resolved {
    /// Intersections et unions compilées.
    owned: Vec<CompiledCalendar>,
    /// Affectations résolues, triées par (position de tâche, UID).
    items: Vec<Resolution>,
    /// Échelle représentative d'une tâche dont les affectations de travail
    /// ont plusieurs calendriers effectifs (union, R-173), par position.
    unions: BTreeMap<usize, CalRef>,
    /// Intersections par paire (tâche, ressource), indices dans `owned` ;
    /// `None` : vide, en échec ou hors budget. Reprises par `load` (aucune
    /// recompilation).
    pairs: BTreeMap<(CalendarUid, CalendarUid), Option<usize>>,
    /// Budget restant, pour la mesure des unions (R-173).
    budget: Budget,
}

impl Resolved {
    /// Intersection compilée de la paire (tâche, ressource), si elle a été
    /// gardée.
    pub(crate) fn intersection(&self, task: CalendarUid, resource: CalendarUid) -> Option<&CompiledCalendar> {
        let i = (*self.pairs.get(&(task, resource))?)?;
        self.owned.get(i)
    }
}

/// Calendrier compilé utilisable (compilé, avec du temps ouvré).
fn usable(set: &CalendarSet, uid: CalendarUid) -> bool {
    set.get(uid).is_ok_and(CompiledCalendar::has_working_time)
}

/// R-170 (moteur) : `d_a = round_div(W × 10 000, U)` pour les unités fixes
/// et le travail fixe ; durée de la tâche pour une feuille à durée fixe et
/// pour des unités nulles (R-181) ; hors du domaine des durées : échec
/// (R-069). Une récapitulative (`summary`) n'a pas de durée tenue : son
/// affectation dure W ÷ U quel que soit son type (R-179 ; toutes les
/// récapitulatives du corpus sont à durée fixe, O32, de durée enregistrée
/// nulle dans A6).
fn assignment_duration(
    task: &Task,
    work: Work,
    units: Units,
    summary: bool,
) -> Result<Minutes, &'static str> {
    if (task.task_type == TaskType::FixedDuration && !summary) || units == Units::ZERO {
        return Ok(task.duration.minutes());
    }
    i128::from(work.minutes().get())
        .checked_mul(10_000)
        .and_then(|num| round_div(num, i128::from(units.get())))
        .filter(|m| (0..=Duration::MAX_MINUTES).contains(m))
        .map(Minutes::new)
        .ok_or(OUT_OF_DOMAIN)
}

/// R-209 (tâche en cours) : durées de la partie réelle et du reste d'une
/// affectation de travail `W`, travail réel `AW` (borné à `[0, W]`),
/// unités `U` : `d_act = round_div(AW × 10 000, U)`, `d_rem =
/// round_div((W − AW) × 10 000, U)` ; unités nulles (R-181) et durée fixe
/// (R-170) : durée réelle et reste de la tâche. `None` si la tâche n'est pas
/// en cours ; hors du domaine des durées : échec (R-069).
fn progress_durations(
    task: &Task,
    work: Work,
    actual_work: Work,
    units: Units,
) -> Option<Result<(Minutes, Minutes), &'static str>> {
    let f = crate::progress::facts(task);
    if f.state != crate::progress::ProgressState::Started {
        return None;
    }
    if task.task_type == TaskType::FixedDuration || units == Units::ZERO {
        return Some(Ok((f.actual_duration, crate::progress::remaining_duration(task))));
    }
    let w = work.minutes().get().max(0);
    let aw = actual_work.minutes().get().clamp(0, w);
    let d = |m: i64| {
        i128::from(m)
            .checked_mul(10_000)
            .and_then(|num| round_div(num, i128::from(units.get())))
            .filter(|m| (0..=Duration::MAX_MINUTES).contains(m))
            .map(Minutes::new)
            .ok_or(OUT_OF_DOMAIN)
    };
    Some(d(aw).and_then(|act| Ok((act, d(w.saturating_sub(aw))?))))
}

/// Construction de [`Resolved`].
struct Resolver<'s> {
    set: &'s CalendarSet,
    /// Calendrier du projet (échelle `PROJECT`).
    project_place: &'s CompiledCalendar,
    owned: Vec<CompiledCalendar>,
    /// Intersections par paire (tâche, ressource) ; `None` : vide ou en
    /// échec.
    pairs: BTreeMap<(CalendarUid, CalendarUid), Option<usize>>,
    /// Unions par ensemble trié de calendriers effectifs.
    unions: BTreeMap<Vec<CalRef>, Option<usize>>,
    /// Budget des combinaisons.
    budget: Budget,
}

impl Resolver<'_> {
    /// Calendrier compilé de `r`.
    fn compiled(&self, r: CalRef) -> Option<&CompiledCalendar> {
        match r {
            CalRef::Set(uid) => self.set.get(uid).ok(),
            CalRef::Project => Some(self.project_place),
            CalRef::Elapsed => None,
            CalRef::Owned(i) => self.owned.get(i),
        }
    }

    /// Intersection compilée de `own` (tâche) et `res` (ressource), mise en
    /// cache ; `None` si elle est vide ou trop morcelée (diagnostic).
    fn intersection(
        &mut self,
        (own, res): (CalendarUid, CalendarUid),
        assignment: AssignmentUid,
        diags: &mut Diagnostics,
    ) -> Option<usize> {
        if let Some(slot) = self.pairs.get(&(own, res)) {
            if slot.is_none() {
                Self::empty(own, res, assignment, diags);
            }
            return *slot;
        }
        let set = self.set;
        let over = CalendarError::TooManySegments {
            limit: self.budget.limit_u32(),
        };
        let combined = match (set.get(own), set.get(res)) {
            (Ok(a), Ok(b)) => {
                if self
                    .budget
                    .admit(a.segment_count().saturating_add(b.segment_count()))
                {
                    a.intersect(b)
                        .and_then(|c| if self.budget.keep(&c) { Ok(c) } else { Err(over) })
                } else {
                    Err(over)
                }
            }
            (Err(e), _) | (_, Err(e)) => Err(e),
        };
        let slot = match combined {
            Ok(c) if c.has_working_time() => {
                let i = self.owned.len();
                self.owned.push(c);
                Some(i)
            }
            Ok(_) => {
                Self::empty(own, res, assignment, diags);
                None
            }
            Err(CalendarError::TooManySegments { limit }) => {
                diags.push(Diagnostic::CalendarTooManySegments {
                    calendar: own,
                    other: res,
                    limit,
                });
                None
            }
            Err(_) => None,
        };
        self.pairs.insert((own, res), slot);
        slot
    }

    /// `assignment.calendar_empty` : repli sur le calendrier de la tâche.
    fn empty(own: CalendarUid, res: CalendarUid, assignment: AssignmentUid, diags: &mut Diagnostics) {
        diags.push(Diagnostic::AssignmentCalendarEmpty {
            assignment,
            task_calendar: own,
            resource_calendar: res,
        });
    }

    /// Union compilée des calendriers `refs` (triés, sans doublon, au moins
    /// deux), mise en cache ; `None` si elle est trop morcelée.
    fn union(&mut self, refs: &[CalRef], diags: &mut Diagnostics) -> Option<usize> {
        if let Some(slot) = self.unions.get(refs) {
            return *slot;
        }
        let mut acc: Option<CompiledCalendar> = None;
        let mut failed = false;
        let mut budget = self.budget;
        let limit = budget.limit_u32();
        for &r in refs {
            let Some(c) = self.compiled(r) else {
                failed = true;
                break;
            };
            let cost = acc
                .as_ref()
                .map_or(0, CompiledCalendar::segment_count)
                .saturating_add(c.segment_count());
            let next = match &acc {
                _ if !budget.admit(cost) => Err(CalendarError::TooManySegments { limit }),
                None => Ok(c.clone()),
                Some(a) => a.union(c),
            };
            match next {
                Ok(n) => acc = Some(n),
                Err(e) => {
                    if let CalendarError::TooManySegments { limit } = e {
                        diags.push(Diagnostic::CalendarTooManySegments {
                            calendar: acc.as_ref().map_or(c.uid(), CompiledCalendar::uid),
                            other: c.uid(),
                            limit,
                        });
                    }
                    failed = true;
                    break;
                }
            }
        }
        let slot = match acc.filter(|_| !failed) {
            Some(c) if budget.keep(&c) => {
                let i = self.owned.len();
                self.owned.push(c);
                Some(i)
            }
            Some(c) => {
                diags.push(Diagnostic::CalendarTooManySegments {
                    calendar: c.uid(),
                    other: c.uid(),
                    limit: budget.limit_u32(),
                });
                None
            }
            None => None,
        };
        self.budget = budget;
        self.unions.insert(refs.to_vec(), slot);
        slot
    }

    /// R-171 : calendrier effectif d'une affectation de la tâche `task` à
    /// une ressource de calendrier `res_cal`.
    fn effective(
        &mut self,
        task: &Task,
        res_cal: Option<CalendarUid>,
        assignment: AssignmentUid,
        diags: &mut Diagnostics,
    ) -> (CalRef, EffectiveCalendar) {
        let set = self.set;
        let own = task.calendar.filter(|&c| !task.is_null && usable(set, c));
        let project = (
            CalRef::Project,
            EffectiveCalendar::Project(self.project_place.uid()),
        );
        if task.ignore_resource_calendar {
            // S48 : « only the task calendar is considered ».
            return own.map_or(project, |o| (CalRef::Set(o), EffectiveCalendar::Task(o)));
        }
        let res = match res_cal {
            Some(c) if usable(set, c) => Some(c),
            Some(c) => {
                // Calendrier de ressource inutilisable : comme sans
                // calendrier (R-066).
                diags.push(Diagnostic::TaskCalendarFallback {
                    task: task.uid(),
                    calendar: c,
                });
                None
            }
            None => None,
        };
        match (own, res) {
            (Some(o), Some(r)) => {
                let both = EffectiveCalendar::Intersection { task: o, resource: r };
                if o == r {
                    return (CalRef::Set(o), both);
                }
                match self.intersection((o, r), assignment, diags) {
                    Some(i) => (CalRef::Owned(i), both),
                    None => (CalRef::Set(o), EffectiveCalendar::Task(o)),
                }
            }
            (Some(o), None) => (CalRef::Set(o), EffectiveCalendar::Task(o)),
            (None, Some(r)) => (CalRef::Set(r), EffectiveCalendar::Resource(r)),
            (None, None) => project,
        }
    }
}

/// Étape 1 : résout chaque affectation d'une tâche connue (R-170, R-171,
/// R-176) et l'union des calendriers effectifs des tâches qui en ont
/// plusieurs. `project_place` : calendrier du projet, de base (échelle
/// `PROJECT`, R-113, R-175) ; `combined_segments` : budget des calendriers
/// combinés (`Limits`).
pub(super) fn resolve(
    project: &Project,
    set: &CalendarSet,
    project_place: &CompiledCalendar,
    combined_segments: usize,
    diags: &mut Diagnostics,
) -> Resolved {
    let mut r = Resolver {
        set,
        project_place,
        owned: Vec::new(),
        pairs: BTreeMap::new(),
        unions: BTreeMap::new(),
        budget: Budget::new(combined_segments),
    };
    let mut items = Vec::with_capacity(project.assignments.len());
    for a in &project.assignments {
        // Tâche inconnue : signalée par la validation, ignorée ici.
        let Some(task_pos) = project.tasks.position(a.task) else {
            continue;
        };
        let Some(task) = project.tasks.at(task_pos) else {
            continue;
        };
        let resource = a.resource.and_then(|uid| project.resources.get(uid));
        let work_resource = resource.is_some_and(|res| res.kind == ResourceKind::Work);
        let task_calendar = if task.duration.basis() == TimeBasis::Elapsed {
            EffectiveCalendar::Elapsed
        } else {
            match task.calendar.filter(|&c| !task.is_null && usable(set, c)) {
                Some(o) => EffectiveCalendar::Task(o),
                None => EffectiveCalendar::Project(project_place.uid()),
            }
        };
        let (cal, calendar) = if work_resource && task_calendar == EffectiveCalendar::Elapsed {
            // R-182 : durée écoulée, calendriers ignorés.
            (CalRef::Elapsed, EffectiveCalendar::Elapsed)
        } else if work_resource {
            r.effective(task, resource.and_then(|res| res.calendar), a.uid(), diags)
        } else {
            (CalRef::Project, task_calendar)
        };
        items.push(Resolution {
            uid: a.uid(),
            task_pos,
            task: a.task,
            resource: a.resource,
            work_resource,
            cal,
            calendar,
            task_calendar,
            // Rôle de la tâche inconnu ici : une récapitulative est
            // corrigée par `attach` (R-179).
            duration: assignment_duration(task, a.work, a.units, false),
            delay: a.delay.max(Minutes::ZERO),
            leveling: a.leveling_delay.filter(|l| l.minutes > Minutes::ZERO),
            contour: a.contour,
            work: a.work,
            units: a.units,
            progress: progress_durations(task, a.work, a.actual_work, a.units),
        });
    }
    // Clés uniques (UID d'affectation) : tri instable sans effet sur l'ordre.
    items.sort_unstable_by_key(|i| (i.task_pos, i.uid));
    // Union des calendriers effectifs des affectations de travail, par
    // tâche qui en a plusieurs (échelle représentative, R-173).
    let mut unions = BTreeMap::new();
    let mut k = 0;
    // Tampon réutilisé d'une tâche à l'autre (aucune allocation par tâche).
    let mut refs: Vec<CalRef> = Vec::new();
    while let Some(first) = items.get(k) {
        let p = first.task_pos;
        refs.clear();
        while let Some(i) = items.get(k).filter(|i| i.task_pos == p) {
            if i.work_resource {
                refs.push(i.cal);
            }
            k = k.saturating_add(1);
        }
        refs.sort_unstable();
        refs.dedup();
        if refs.len() >= 2 {
            let rep = r.union(&refs, diags).map(CalRef::Owned);
            if let Some(rep) = rep {
                unions.insert(p, rep);
            }
        }
    }
    Resolved {
        owned: r.owned,
        items,
        unions,
        pairs: r.pairs,
        budget: r.budget,
    }
}

/// Part d'une feuille : une affectation de travail qui la place (R-173).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Part {
    /// Affectation.
    pub assignment: AssignmentUid,
    /// Échelle effective (indice dans `Network::scales`).
    pub scale: u32,
    /// Délai d'affectation, ouvré sur l'échelle (R-177).
    pub delay: Minutes,
    /// Retard de nivellement de l'affectation, ajouté au délai (R-178) :
    /// minutes, et vrai s'il est écoulé.
    pub level: (Minutes, bool),
    /// Durée `d_a` (R-170).
    pub d: Minutes,
}

impl Part {
    /// Part sans délai ni retard de nivellement.
    fn undelayed(&self) -> bool {
        self.delay == Minutes::ZERO && self.level.0 == Minutes::ZERO
    }
}

/// Parts (au moins une) sur une même échelle, sans délai, toutes de durée
/// non nulle ou toutes nulles : elles commencent au même instant et la
/// feuille se place comme sa plus longue part ; l'union de leurs
/// intervalles est cette part (mesure = plus grand `d_a`).
fn shared_scale(parts: &[Part]) -> bool {
    match parts {
        [] => false,
        [first, rest @ ..] => {
            first.undelayed()
                && rest.iter().all(|p| {
                    p.scale == first.scale
                        && p.undelayed()
                        && (p.d == Minutes::ZERO) == (first.d == Minutes::ZERO)
                })
        }
    }
}

/// Retard de nivellement d'une affectation, sous la forme d'une part.
fn level_of(l: Option<LevelingDelay>) -> (Minutes, bool) {
    l.map_or((Minutes::ZERO, false), |l| (l.minutes, l.elapsed))
}

/// Affectation à ranger dans le planning, avec la part qui la place.
#[derive(Clone, Copy, Debug)]
struct Output {
    /// Résolution.
    res: Resolution,
    /// Indice dans `Parts::items`, pour une affectation qui place sa
    /// feuille.
    part: Option<usize>,
    /// Échelle effective d'une affectation de travail d'une récapitulative
    /// (R-179) ou d'une tâche manuelle (R-183).
    scale: Option<u32>,
}

/// Parts des feuilles (CSR par position de tâche) et affectations à ranger.
#[derive(Debug, Default)]
pub(super) struct Parts {
    /// Parts de la tâche en position `p` : `parts[offsets[p]..offsets[p + 1]]`.
    offsets: Vec<u32>,
    /// Parts, triées par UID d'affectation dans chaque tâche.
    items: Vec<Part>,
    /// Échelle représentative (union des échelles effectives), par position
    /// de tâche à parts de plusieurs échelles.
    rep: BTreeMap<usize, u32>,
    /// Échec d'un `d_a` (R-069), par position de tâche.
    fail: BTreeMap<usize, &'static str>,
    /// Affectations à ranger (feuilles actives, récapitulatives, manuelles),
    /// triées par position de tâche puis UID.
    outputs: Vec<Output>,
    /// Budget des combinaisons restant après `resolve` (mesure des unions).
    budget: Budget,
    /// Feuilles en cours (R-209) : parts du reste de durée `d_rem` > 0
    /// (sans délai : le délai court depuis le début de la tâche, déjà
    /// passé ; une part sans reste ne place pas la reprise), triées par UID
    /// d'affectation dans chaque tâche.
    rem_items: Vec<Part>,
    /// Plage de `rem_items` de la tâche en position `p`.
    rem_ranges: BTreeMap<usize, (usize, usize)>,
    /// `(d_act, d_rem)` de chaque affectation de travail d'une feuille en
    /// cours.
    rem_durations: BTreeMap<AssignmentUid, (Minutes, Minutes)>,
}

impl Parts {
    /// Parts de la tâche en position `p` (vide : chemin J4).
    pub fn of(&self, p: usize) -> &[Part] {
        let range = || {
            let a = pos(*self.offsets.get(p)?)?;
            let b = pos(*self.offsets.get(p.checked_add(1)?)?)?;
            self.items.get(a..b)
        };
        range().unwrap_or_default()
    }

    /// Code d'échec du `d_a` d'une affectation de la tâche en position `p`.
    pub fn failure(&self, p: usize) -> Option<&'static str> {
        self.fail.get(&p).copied()
    }

    /// Échelle de placement d'une feuille à parts : celle de son unique
    /// part, ou l'union de leurs échelles (sinon la première) ; `None` sans
    /// part.
    pub fn place(&self, p: usize) -> Option<u32> {
        let parts = self.of(p);
        let first = parts.first()?;
        Some(self.rep.get(&p).copied().unwrap_or(first.scale))
    }

    /// Union compilée des échelles des parts de la feuille en position `p`
    /// (R-173) ; `None` sans union (une seule échelle, union trop morcelée
    /// ou hors budget).
    pub fn union_scale(&self, p: usize) -> Option<u32> {
        self.rep.get(&p).copied()
    }

    /// R-209 : part du reste de l'affectation de travail `res` d'une feuille
    /// en cours en position `p` (échelle `scale`) : gardée si `d_rem` > 0,
    /// durées `(d_act, d_rem)` notées ; échec hors du domaine (R-069).
    fn push_remaining(&mut self, res: &Resolution, scale: u32, p: usize, diags: &mut Diagnostics) {
        match res.progress {
            Some(Ok((act, rem))) => {
                if rem > Minutes::ZERO {
                    self.rem_items.push(Part {
                        assignment: res.uid,
                        scale,
                        delay: Minutes::ZERO,
                        level: (Minutes::ZERO, false),
                        d: rem,
                    });
                }
                self.rem_durations.insert(res.uid, (act, rem));
            }
            Some(Err(code)) => {
                diags.push(Diagnostic::AssignmentDurationOutOfDomain { assignment: res.uid });
                self.fail.insert(p, code);
            }
            None => {}
        }
    }

    /// Plage des parts du reste de la tâche en position `p`, ajoutées depuis
    /// `first`.
    fn close_remaining(&mut self, p: usize, first: usize) {
        if self.rem_items.len() > first {
            self.rem_ranges.insert(p, (first, self.rem_items.len()));
        }
    }

    /// Parts du reste (`d_rem` > 0) de la feuille en cours en position
    /// `p` (R-209) ; vide sans part.
    pub fn remaining_of(&self, p: usize) -> &[Part] {
        let Some(&(a, b)) = self.rem_ranges.get(&p) else {
            return &[];
        };
        self.rem_items.get(a..b).unwrap_or_default()
    }

    /// Durée de la tâche pour le réseau (lags en %, jalon, % achevé) : le
    /// plus grand des `d_a` (convention du lot L2) ; `None` sans part.
    pub fn duration(&self, p: usize) -> Option<Minutes> {
        self.of(p).iter().map(|part| part.d).max()
    }
}

/// Feuille qui se place par ses affectations : active, automatique (R-173 ;
/// durée écoulée sur le temps civil, R-182 ; manuelle : dates saisies,
/// R-183).
fn placed_by_parts(info: &TaskInfo, task: &Task) -> bool {
    info.role == Role::Leaf
        && !info.inactive()
        && !info.manual_undated
        && matches!(task.mode, SchedulingMode::Auto)
}

/// Étape 2 : parts des feuilles et échelles effectives, ajoutées à `scales`.
#[allow(
    clippy::too_many_lines,
    reason = "un seul parcours des affectations résolues ; les parts du reste (J6) y sont rangées au passage"
)]
pub(super) fn attach<'a>(
    project: &Project,
    resolved: &'a Resolved,
    set: &'a CalendarSet,
    scales: &mut Vec<TimeScale<'a>>,
    tasks: &[TaskInfo],
    diags: &mut Diagnostics,
) -> Option<Parts> {
    let mut index: BTreeMap<CalRef, u32> = BTreeMap::new();
    index.insert(CalRef::Project, PROJECT);
    // Échelles déjà rangées par empreinte de forme : seules celles de même
    // empreinte sont comparées en entier (O(S log S), et non O(S²) pour S
    // calendriers distincts).
    let mut shapes: BTreeMap<(i64, usize, u64), Vec<u32>> = BTreeMap::new();
    for (i, s) in scales.iter().enumerate() {
        if let (TimeScale::Working(c), Ok(i)) = (s, u32::try_from(i)) {
            shapes.entry(c.shape_fingerprint()).or_default().push(i);
        }
    }
    // Un calendrier déjà rangé (calendrier du projet, calendrier d'une
    // tâche) garde son échelle : une affectation sur le calendrier de sa
    // tâche la place exactement comme sans affectation (P34).
    let mut scale_of = |r: CalRef, scales: &mut Vec<TimeScale<'a>>| -> Option<u32> {
        if let Some(&i) = index.get(&r) {
            return Some(i);
        }
        let c = match r {
            CalRef::Set(uid) => set.get(uid).ok()?,
            CalRef::Owned(i) => resolved.owned.get(i)?,
            CalRef::Project => return Some(PROJECT),
            CalRef::Elapsed => return Some(ELAPSED),
        };
        // Une échelle de même forme compilée (calendrier de ressource dérivé
        // sans exception propre, intersection égale au calendrier de la
        // tâche) est réutilisée : mêmes conversions, et les parts qui la
        // partagent se placent comme une durée unique (`Placer::of`).
        let key = c.shape_fingerprint();
        let known = shapes.get(&key).and_then(|same| {
            same.iter().copied().find(|&k| {
                matches!(pos(k).and_then(|k| scales.get(k)),
                    Some(TimeScale::Working(s)) if std::ptr::eq(*s, c) || s.same_working_time(c))
            })
        });
        let i = if let Some(k) = known {
            k
        } else {
            let i = ix(scales.len())?;
            scales.push(TimeScale::Working(c));
            shapes.entry(key).or_default().push(i);
            i
        };
        index.insert(r, i);
        Some(i)
    };
    let n = tasks.len();
    let mut out = Parts {
        offsets: Vec::with_capacity(n.checked_add(1)?),
        budget: resolved.budget,
        ..Parts::default()
    };
    out.offsets.push(0);
    let mut k = 0usize;
    for p in 0..n {
        let info = tasks.get(p)?;
        let task = project.tasks.at(p)?;
        let by_parts = placed_by_parts(info, task);
        let rem_first = out.rem_items.len();
        while let Some(res) = resolved.items.get(k).filter(|i| i.task_pos == p) {
            k = k.checked_add(1)?;
            // R-183 : tâche inactive, aucune affectation rangée.
            if info.role == Role::Null || info.inactive() || info.manual_undated {
                continue;
            }
            if res.work_resource {
                notices(res, diags);
            }
            let mut part = None;
            let mut scale = None;
            let mut resolution = *res;
            if by_parts && res.work_resource {
                match res.duration {
                    Ok(d) => {
                        part = Some(out.items.len());
                        let scale = scale_of(res.cal, scales)?;
                        out.items.push(Part {
                            assignment: res.uid,
                            scale,
                            delay: res.delay,
                            level: level_of(res.leveling),
                            d,
                        });
                        out.push_remaining(res, scale, p, diags); // R-209
                    }
                    Err(code) => {
                        diags.push(Diagnostic::AssignmentDurationOutOfDomain { assignment: res.uid });
                        out.fail.insert(p, code);
                    }
                }
            } else if res.work_resource {
                // R-179, R-183 : échelle effective, pour placer l'affectation
                // d'une récapitulative ou mesurer celle d'une manuelle.
                scale = Some(scale_of(res.cal, scales)?);
                let summary = info.role == Role::Summary && matches!(task.mode, SchedulingMode::Auto);
                if summary {
                    // R-179 : W ÷ U, même pour une récapitulative à durée
                    // fixe.
                    resolution.duration = assignment_duration(task, res.work, res.units, true);
                    if resolution.duration.is_err() {
                        diags.push(Diagnostic::AssignmentDurationOutOfDomain { assignment: res.uid });
                    }
                }
            }
            out.outputs.push(Output {
                res: resolution,
                part,
                scale,
            });
        }
        if let Some(&rep) = resolved.unions.get(&p).filter(|_| by_parts) {
            out.rep.insert(p, scale_of(rep, scales)?);
        }
        out.close_remaining(p, rem_first);
        out.offsets.push(ix(out.items.len())?);
    }
    Some(out)
}

/// Placement d'une feuille : une échelle et une durée (chemin J4, et une
/// seule affectation sans délai, P34), ou ses parts (R-173, R-174, R-177).
#[derive(Clone, Copy)]
pub(super) enum Placer<'p, 'a> {
    /// Une échelle, une durée.
    Single(TimeScale<'a>, Minutes),
    /// Parts d'une feuille affectée.
    Parts {
        /// Parts, triées par UID d'affectation.
        parts: &'p [Part],
        /// Échelles du réseau.
        scales: &'p [TimeScale<'a>],
        /// Échelle de placement de la feuille (union des échelles de ses
        /// parts) : instant de fin d'une contrainte de fin (R-174).
        place: TimeScale<'a>,
    },
}

impl<'p, 'a> Placer<'p, 'a> {
    /// Placement de la feuille de parts `parts` (vide : J4) sur l'échelle de
    /// placement `place`, de durée `d`.
    pub fn of(scales: &'p [TimeScale<'a>], parts: &'p [Part], place: u32, d: Minutes) -> Result<Self, Fail> {
        // Une échelle commune, sans délai, toutes de durée > 0 (ou toutes
        // nulles) : le placement est celui de la plus longue (`d` = max
        // `d_a`), le chemin J4 suffit.
        let single = parts.is_empty() || shared_scale(parts);
        let scale = *pos(place)
            .and_then(|i| scales.get(i))
            .ok_or(Fail::Internal("échelle hors bornes"))?;
        Ok(if single {
            Self::Single(scale, d)
        } else {
            Self::Parts {
                parts,
                scales,
                place: scale,
            }
        })
    }

    /// Échelle d'une part.
    fn scale(scales: &[TimeScale<'a>], part: &Part) -> Result<TimeScale<'a>, Fail> {
        pos(part.scale)
            .and_then(|i| scales.get(i))
            .copied()
            .ok_or(Fail::Internal("échelle de part hors bornes"))
    }

    /// R-177, R-178 : dates de la part depuis la borne `from` : délai compté
    /// en temps ouvré de son échelle, puis retard de nivellement de
    /// l'affectation (ouvré sur la même échelle, ou écoulé), puis `d_a`
    /// (R-030, R-031).
    pub fn part_span(scales: &[TimeScale<'a>], part: &Part, from: Moment) -> Result<Span, Fail> {
        let scale = Self::scale(scales, part)?;
        let x = scale.add(from, part.delay)?;
        let x = Self::level_scale(scale, part).add(x, part.level.0)?;
        Ok(scale.forward_span(x, part.d)?)
    }

    /// Échelle du retard de nivellement d'une part : la sienne, ou le temps
    /// écoulé (R-178).
    fn level_scale(scale: TimeScale<'a>, part: &Part) -> TimeScale<'a> {
        if part.level.1 { TimeScale::Elapsed } else { scale }
    }

    /// Passe avant depuis la borne `from` (R-173) : début `T(from)`, fin
    /// `max f_a`.
    pub fn forward_span(&self, from: Moment) -> Result<Span, Fail> {
        let (parts, scales) = match self {
            Self::Single(scale, d) => return Ok(scale.forward_span(from, *d)?),
            Self::Parts { parts, scales, .. } => (*parts, *scales),
        };
        let mut span: Option<Span> = None;
        for part in parts {
            let own = Self::scale(scales, part)?.forward_span(from, part.d)?;
            let start = own.start;
            let finish = if part.undelayed() {
                own.finish
            } else {
                Self::part_span(scales, part, from)?.finish
            };
            span = Some(span.map_or(Span { start, finish }, |s| Span {
                start: s.start.min(start),
                finish: s.finish.max(finish),
            }));
        }
        span.ok_or(Fail::Internal("feuille sans part"))
    }

    /// Passe arrière sous le plafond `to` (R-174) : `LS = min_a
    /// (backward_span_a(to, d_a).start − delay_a)`, dates `F(LS)`.
    pub fn backward_span(&self, to: Moment) -> Result<Span, Fail> {
        let (parts, scales) = match self {
            Self::Single(scale, d) => return Ok(scale.backward_span(to, *d)?),
            Self::Parts { parts, scales, .. } => (*parts, *scales),
        };
        let mut start: Option<Moment> = None;
        for part in parts {
            let scale = Self::scale(scales, part)?;
            let b = scale.backward_span(to, part.d)?.start;
            let neg = |m: Minutes| m.checked_neg().ok_or(Fail::Internal("délai hors bornes"));
            let b = Self::level_scale(scale, part).add(b, neg(part.level.0)?)?;
            let origin = scale.add(b, neg(part.delay)?)?;
            start = Some(start.map_or(origin, |s| s.min(origin)));
        }
        let ls = start.ok_or(Fail::Internal("feuille sans part"))?;
        self.forward_span(ls)
    }

    /// Dernier instant dont le placement commence au plus tard en `y`
    /// (R-047) : `max_a latest_start_a(y, d_a)`.
    pub fn latest_start(&self, y: Moment) -> Result<Moment, Fail> {
        let (parts, scales) = match self {
            Self::Single(scale, d) => return Ok(scale.latest_start(y, *d)?),
            Self::Parts { parts, scales, .. } => (*parts, *scales),
        };
        let mut best: Option<Moment> = None;
        for part in parts {
            let x = Self::scale(scales, part)?.latest_start(y, part.d)?;
            best = Some(best.map_or(x, |b| b.max(x)));
        }
        best.ok_or(Fail::Internal("feuille sans part"))
    }

    /// R-090, R-174 : ancrage d'une contrainte datée. Type début : passe
    /// avant depuis la date. Type fin : `f*` = la date recalée sur l'échelle
    /// de placement comme pour une feuille sans affectation (R-031 en passe
    /// arrière), fin de l'ancrage = `f*` (O14), début = min des débuts qui
    /// finissent chaque part en `f*`.
    pub fn anchor(&self, kind: ConstraintKind, date: Moment) -> Result<Span, Fail> {
        match self {
            Self::Single(scale, d) => Ok(super::constraint::anchor(*scale, kind, date, *d)?),
            Self::Parts { .. } if kind.is_start_side() => self.forward_span(date),
            Self::Parts { place, .. } => {
                let finish = place.backward_span(date, Minutes::ZERO)?.finish;
                let start = self.backward_span(finish)?.start;
                Ok(Span { start, finish })
            }
        }
    }
}

impl<'n, 'a> Calc<'n, 'a> {
    /// Placement du nœud `v` : feuille à parts, sinon son échelle de
    /// placement et sa durée ; échec d'un `d_a` hors domaine (R-069).
    pub(super) fn placer(&self, v: super::network::NodeIx) -> Result<Placer<'n, 'a>, Fail> {
        let net: &'n super::network::Network<'a> = self.net;
        let node = net.node(v).ok_or(Fail::Internal("nœud hors bornes"))?;
        let parts = if node.kind == super::network::NodeKind::Leaf {
            if let Some(code) = net.parts.failure(node.task) {
                return Err(Fail::Conversion(code));
            }
            net.parts.of(node.task)
        } else {
            &[]
        };
        Placer::of(&net.scales, parts, node.place, node.duration)
    }

    /// Placement du reste de la feuille en cours `v` (R-204, R-209) : ses
    /// parts du reste (`d_rem`, sans délai), sinon son échelle de placement
    /// et sa durée restante ; feuille qui n'est pas en cours : son placement
    /// ([`Self::placer`]).
    pub(super) fn remaining_placer(&self, v: super::network::NodeIx) -> Result<Placer<'n, 'a>, Fail> {
        let net: &'n super::network::Network<'a> = self.net;
        let node = net.node(v).ok_or(Fail::Internal("nœud hors bornes"))?;
        let Some(ip) = node.in_progress() else {
            return self.placer(v);
        };
        if let Some(code) = net.parts.failure(node.task) {
            return Err(Fail::Conversion(code));
        }
        let parts = net.parts.remaining_of(node.task);
        let d = parts.iter().map(|part| part.d).max().unwrap_or(ip.remaining);
        Placer::of(&net.scales, parts, node.place, d)
    }

    /// Sortie : affectations placées (R-171, R-173, R-176, R-177) et durée
    /// calculée des feuilles à parts (mesure de l'union des intervalles
    /// travaillés, R-173), par position de tâche.
    pub(super) fn place_assignments(
        &mut self,
    ) -> (
        BTreeMap<AssignmentUid, AssignmentSchedule>,
        BTreeMap<usize, Minutes>,
    ) {
        let net = self.net;
        let mut durations = BTreeMap::new();
        let mut unions: UnionCache = BTreeMap::new();
        let mut budget = net.parts.budget;
        let mut sweep = Sweep::default();
        // Dates de chaque part depuis le début planifié, par indice de part :
        // réutilisées par `assignment_schedule` (même borne, même résultat).
        let mut placed: Vec<Option<(Moment, Span)>> = vec![None; net.parts.items.len()];
        let mut spans: Vec<Span> = Vec::new();
        for p in 0..net.tasks.len() {
            let parts = net.parts.of(p);
            if parts.len() < 2 && parts.first().is_none_or(Part::undelayed) {
                continue;
            }
            // R-209 : une feuille en cours garde sa durée (D de
            // l'avancement) ; ses parts sont placées en deux temps.
            let running = net
                .task(p)
                .and_then(|t| t.exit)
                .and_then(|v| net.node(v))
                .is_some_and(|n| n.in_progress().is_some());
            if running {
                continue;
            }
            let Some(planned) = self.planned_start(p) else {
                continue;
            };
            let first = net.parts.offsets.get(p).and_then(|&o| pos(o));
            spans.clear();
            for (k, part) in parts.iter().enumerate() {
                let Ok(span) = Placer::part_span(&net.scales, part, planned) else {
                    continue;
                };
                spans.push(span);
                if let Some(slot) = first
                    .and_then(|f| f.checked_add(k))
                    .and_then(|i| placed.get_mut(i))
                {
                    *slot = Some((planned, span));
                }
            }
            if spans.len() != parts.len() {
                continue;
            }
            let measured = if parts.len() == 1 || shared_scale(parts) {
                parts.iter().map(|part| part.d).max()
            } else if parts.len() > MAX_MEASURED_PARTS {
                // Règle 10 : mesure exacte bornée ; repli sur le plus grand
                // `d_a`, signalé.
                if let Some(task) = net.tasks.get(p).map(|t| t.uid) {
                    self.diags.push(Diagnostic::AssignmentTooManyParts {
                        task,
                        parts: u32::try_from(parts.len()).unwrap_or(u32::MAX),
                        limit: u32::try_from(MAX_MEASURED_PARTS).unwrap_or(u32::MAX),
                    });
                }
                parts.iter().map(|part| part.d).max()
            } else {
                // L'union de toutes les échelles des parts est l'échelle de
                // la feuille quand elle a été compilée (R-173) ; sinon (union
                // trop morcelée ou hors budget), chaque intervalle passe par
                // `measure_union` et son repli, jamais par l'échelle de la
                // première part seule.
                let mut all: Vec<u32> = parts.iter().map(|part| part.scale).collect();
                all.sort_unstable();
                all.dedup();
                let union = net.parts.union_scale(p).and_then(|i| net.scale(i)).copied();
                let full = union.map(|s| (all.as_slice(), s));
                union_measure(
                    &net.scales,
                    (parts, &spans),
                    (full, &mut unions, &mut budget),
                    &mut sweep,
                )
            };
            if let Some(m) = measured {
                durations.insert(p, m);
            }
        }
        // Sortie triée par UID puis construite en bloc (plus rapide que des
        // insertions une à une).
        let mut out: Vec<(AssignmentUid, AssignmentSchedule)> = net
            .parts
            .outputs
            .iter()
            .filter_map(|o| Some((o.res.uid, self.assignment_schedule(o, &placed)?)))
            .collect();
        out.sort_unstable_by_key(|(uid, _)| *uid);
        (out.into_iter().collect(), durations)
    }

    /// Début planifié de la tâche en position `p` (au plus tôt depuis le
    /// début, au plus tard à rebours, R-112) ; `None` sans dates.
    fn planned_start(&self, p: usize) -> Option<Moment> {
        let v = self.net.task(p)?.exit?;
        let s = self.state(v)?;
        match self.mode {
            Mode::FromStart => s.early.map(|e| e.start),
            Mode::FromFinish => s.late.map(|l| l.start),
        }
    }

    /// `AssignmentSchedule` d'une affectation, si sa tâche est planifiée.
    #[allow(
        clippy::too_many_lines,
        reason = "un bras par rôle d'affectation (J5) et par état d'avancement (J6), rien à factoriser"
    )]
    fn assignment_schedule(
        &self,
        o: &Output,
        placed: &[Option<(Moment, Span)>],
    ) -> Option<AssignmentSchedule> {
        let net = self.net;
        let info = net.task(o.res.task_pos)?;
        let task = self.project.tasks.at(o.res.task_pos)?;
        let v = info.exit?;
        let state = self.state(v)?;
        let early = state.early?;
        let late = state.late;
        // R-112 : l'affectation est rangée sur les dates planifiées de sa
        // tâche (au plus tôt depuis le début, au plus tard à rebours, repli
        // au plus tôt sans dates au plus tard comme `rollup`) ; ses dates au
        // plus tard restent dans `late`.
        let planned = match self.mode {
            Mode::FromStart => early,
            Mode::FromFinish => late.unwrap_or(early),
        };
        let node = net.node(v)?;
        let res = &o.res;
        // R-183 : une tâche manuelle (feuille ou récapitulative) range ses
        // affectations sur ses dates saisies.
        let role = if !matches!(task.mode, SchedulingMode::Auto) {
            AssignmentRole::Manual
        } else if info.role == Role::Summary {
            AssignmentRole::Summary
        } else if res.work_resource {
            AssignmentRole::Work
        } else {
            AssignmentRole::Neutral
        };
        let parts = net.parts.of(o.res.task_pos);
        let mut effective_units = res.units;
        let span_from = |i: usize, part: &Part, from: Moment| match placed.get(i) {
            Some(Some((at, span))) if *at == from => Some(*span),
            _ => Placer::part_span(&net.scales, part, from).ok(),
        };
        // R-209 : feuille achevée, affectations de travail placées depuis le
        // début réel sur leur échelle, travail nul daté au début réel brut ;
        // partie réelle = dates planifiées.
        let completed = node.actual().is_some();
        // R-209 : feuille en cours, affectation en deux parts.
        let running = state.run.filter(|_| node.in_progress().is_some());
        let mut split: Option<(Span, Option<Moment>)> = None;
        let (span, late, duration, calendar) = match o.part.and_then(|i| Some((i, net.parts.items.get(i)?))) {
            Some((_, part)) if running.is_some() => {
                let run = running?;
                let (whole, late_span, minutes, actual_span, rest) =
                    self.running_assignment(o.res.task_pos, part, res.work, run)?;
                split = Some((actual_span, rest));
                (whole, late_span, minutes, res.calendar)
            }
            Some((_, part)) if completed && res.work == Work::ZERO => {
                let at = Span {
                    start: planned.start,
                    finish: planned.start,
                };
                (at, Some(at), part.d, res.calendar)
            }
            Some((i, part)) if completed => {
                let at = span_from(i, part, planned.start)?;
                (at, Some(at), part.d, res.calendar)
            }
            // Une seule part sans délai : la feuille est la part (P34).
            Some((_, part)) if parts.len() == 1 && part.undelayed() => (planned, late, part.d, res.calendar),
            Some((i, part)) => (
                span_from(i, part, planned.start)?,
                late.and_then(|l| span_from(i, part, l.start)),
                part.d,
                res.calendar,
            ),
            None => match (role, o.scale, res.duration) {
                // R-179 : affectation de travail d'une récapitulative : départ
                // au début de la récapitulative, délais compris, `d_a` sur son
                // calendrier effectif, jamais tronquée.
                (AssignmentRole::Summary, Some(scale), Ok(d)) => {
                    let part = Part {
                        assignment: res.uid,
                        scale,
                        delay: res.delay,
                        level: level_of(res.leveling),
                        d,
                    };
                    (
                        Placer::part_span(&net.scales, &part, planned.start).ok()?,
                        late.and_then(|l| Placer::part_span(&net.scales, &part, l.start).ok()),
                        d,
                        res.calendar,
                    )
                }
                // R-183 : affectation de travail d'une tâche manuelle : le
                // span de la tâche, débit = travail ÷ temps ouvré du span.
                (AssignmentRole::Manual, Some(scale), _) => {
                    let measured = net
                        .scale(scale)
                        .and_then(|s| s.between(planned.start, planned.finish).ok())
                        .filter(|m| m.get() > 0);
                    if let Some(m) = measured {
                        effective_units = rate(res.work, m).unwrap_or(res.units);
                    }
                    (planned, late, measured.unwrap_or(node.duration), res.calendar)
                }
                _ => {
                    let calendar = match role {
                        AssignmentRole::Work if task.duration.basis() == TimeBasis::Elapsed => {
                            EffectiveCalendar::Elapsed
                        }
                        AssignmentRole::Work | AssignmentRole::Summary => res.calendar,
                        AssignmentRole::Neutral | AssignmentRole::Manual => res.task_calendar,
                    };
                    (planned, late, node.duration, calendar)
                }
            },
        };
        Some(AssignmentSchedule {
            task: res.task,
            resource: res.resource,
            role,
            span,
            late,
            duration,
            work: res.work,
            effective_units,
            calendar,
            actual: split.map(|(a, _)| a).or(completed.then_some(span)),
            remaining_start: split.and_then(|(_, r)| r),
        })
    }

    /// R-209 : affectation de travail de part `part` de la feuille en
    /// cours en position `p`, dates `run` : partie réelle depuis le début
    /// réel (`d_act` sur son échelle, sans délai), reste depuis la reprise
    /// (`d_rem`, sur son échelle ; nul : pas de reste, span = partie réelle) ;
    /// travail nul : instant du début réel brut. Rend (span, dates au plus
    /// tard, durée, partie réelle, début du reste).
    #[allow(
        clippy::type_complexity,
        reason = "n-uplet local, défait aussitôt par l'unique appelant"
    )]
    fn running_assignment(
        &self,
        p: usize,
        part: &Part,
        work: Work,
        run: super::Run,
    ) -> Option<(Span, Option<Span>, Minutes, Span, Option<Moment>)> {
        let net = self.net;
        let point = Span {
            start: run.actual_start,
            finish: run.actual_start,
        };
        if work == Work::ZERO {
            return Some((point, Some(point), Minutes::ZERO, point, None));
        }
        let (d_act, d_rem) = *net.parts.rem_durations.get(&part.assignment)?;
        let actual = if d_act == Minutes::ZERO {
            point
        } else {
            let act_part = Part {
                delay: Minutes::ZERO,
                level: (Minutes::ZERO, false),
                d: d_act,
                ..*part
            };
            Placer::part_span(&net.scales, &act_part, run.actual_start).ok()?
        };
        let duration = d_act.checked_add(d_rem).unwrap_or(part.d);
        let Some(rem_part) = net
            .parts
            .remaining_of(p)
            .iter()
            .find(|r| r.assignment == part.assignment)
        else {
            return Some((actual, Some(actual), duration, actual, None));
        };
        let rem = Placer::part_span(&net.scales, rem_part, run.resume).ok()?;
        let late = run
            .late_resume
            .and_then(|l| Placer::part_span(&net.scales, rem_part, l).ok())
            .map(|l| Span {
                start: actual.start,
                finish: l.finish.max(actual.start),
            });
        let span = Span {
            start: actual.start,
            finish: rem.finish.max(actual.start),
        };
        Some((span, late, duration, actual, Some(rem.start)))
    }
}

/// Débit d'une affectation : `round_div(W × 10 000, m)` (R-183, R-186) ;
/// `None` hors du domaine des unités.
fn rate(work: Work, m: Minutes) -> Option<Units> {
    let num = i128::from(work.minutes().get()).checked_mul(10_000)?;
    let u = round_div(num, i128::from(m.get()))?;
    Units::new(u).ok()
}

/// Diagnostics d'une affectation de travail d'une tâche active : unités
/// nulles (R-181), profil non uniforme placé comme uniforme (R-180).
fn notices(res: &Resolution, diags: &mut Diagnostics) {
    if res.units == Units::ZERO {
        diags.push(Diagnostic::AssignmentZeroUnits { assignment: res.uid });
    }
    if res.contour != WorkContour::Flat {
        diags.push(Diagnostic::AssignmentContourUnsupported {
            assignment: res.uid,
            contour: res.contour,
        });
    }
}

/// Tampons de [`union_measure`], réutilisés d'une feuille à l'autre.
#[derive(Debug, Default)]
struct Sweep {
    /// Événements `(instant, fin ?, échelle)`.
    events: Vec<(Moment, bool, u32)>,
    /// Échelles des parts actives, triées, sans doublon.
    keys: Vec<u32>,
    /// Parts actives de chaque échelle de `keys`.
    counts: Vec<u32>,
}

/// R-173 : mesure de l'union des intervalles travaillés `[s_a, f_a)` des
/// parts, chacun sur son échelle : balayage des débuts et des fins triés,
/// compteur de parts actives par échelle (O(P log P) pour P parts, et non
/// un filtrage de toutes les parts par intervalle élémentaire) ; sur
/// chaque intervalle élémentaire, temps ouvré de l'union des échelles des
/// parts actives (union compilée mise en cache par ensemble d'échelles).
/// `None` si une conversion échoue.
fn union_measure(
    scales: &[TimeScale<'_>],
    (parts, spans): (&[Part], &[Span]),
    (full, cache, budget): (Option<(&[u32], TimeScale<'_>)>, &mut UnionCache, &mut Budget),
    sweep: &mut Sweep,
) -> Option<Minutes> {
    let Sweep { events, keys, counts } = sweep;
    events.clear();
    keys.clear();
    counts.clear();
    // À un même instant, les débuts avant les fins (un intervalle vide
    // entre puis sort).
    for (part, s) in parts.iter().zip(spans) {
        events.push((s.start, false, part.scale));
        events.push((s.finish, true, part.scale));
    }
    events.sort_unstable();
    let mut total = Minutes::ZERO;
    let mut i = 0usize;
    while let Some(&(lo, _, _)) = events.get(i) {
        // Événements de l'instant `lo` : parts actives sur `[lo, suivant)`.
        while let Some(&(_, end, scale)) = events.get(i).filter(|e| e.0 == lo) {
            match (keys.binary_search(&scale), end) {
                (Ok(k), false) => {
                    let n = counts.get_mut(k)?;
                    *n = n.saturating_add(1);
                }
                (Err(k), false) => {
                    keys.insert(k, scale);
                    counts.insert(k, 1);
                }
                (Ok(k), true) => {
                    let n = counts.get_mut(k)?;
                    *n = n.saturating_sub(1);
                    if *n == 0 {
                        keys.remove(k);
                        counts.remove(k);
                    }
                }
                (Err(_), true) => return None,
            }
            i = i.saturating_add(1);
        }
        let Some(&(hi, _, _)) = events.get(i) else {
            break;
        };
        let m = match keys.as_slice() {
            [] => Minutes::ZERO,
            [one] => scales.get(pos(*one)?)?.between(lo, hi).ok()?,
            many => match full {
                Some((all, union)) if all == many => union.between(lo, hi).ok()?,
                _ => measure_union(scales, many, (lo, hi), (cache, budget))?,
            },
        };
        total = total.checked_add(m)?;
    }
    Some(total)
}

/// Temps ouvré de `[lo, hi)` sur l'union des échelles `many` ; repli (union
/// trop morcelée, cache plein) : le plus grand des temps ouvrés.
fn measure_union(
    scales: &[TimeScale<'_>],
    many: &[u32],
    (lo, hi): (Moment, Moment),
    (cache, budget): (&mut UnionCache, &mut Budget),
) -> Option<Minutes> {
    let working = |i: u32| match scales.get(pos(i)?)? {
        TimeScale::Working(c) => Some(*c),
        TimeScale::Elapsed => None,
    };
    if !cache.contains_key(many) && cache.len() < UNION_CACHE_LIMIT {
        let mut acc: Option<CompiledCalendar> = None;
        for &i in many {
            let c = working(i);
            let cost = acc
                .as_ref()
                .map_or(0, CompiledCalendar::segment_count)
                .saturating_add(c.map_or(0, CompiledCalendar::segment_count));
            acc = match (acc, c) {
                _ if !budget.admit(cost) => None,
                (None, Some(c)) => Some(c.clone()),
                (Some(a), Some(c)) => a.union(c).ok(),
                _ => None,
            };
            if acc.is_none() {
                break;
            }
        }
        let acc = acc.filter(|u| budget.keep(u));
        cache.insert(many.to_vec(), acc);
    }
    if let Some(Some(u)) = cache.get(many) {
        return u.working_between(lo, hi).ok();
    }
    let mut best = Minutes::ZERO;
    for &i in many {
        best = best.max(scales.get(pos(i)?)?.between(lo, hi).ok()?);
    }
    Some(best)
}
