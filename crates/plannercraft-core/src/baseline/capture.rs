//! Capture d'une ligne de base (R-220 à R-223 ; lot L4).
//!
//! [`capture`] copie les valeurs du moteur (`Schedule`) sur une portée,
//! avec cumul éventuel vers les récapitulatives ; [`apply`] écrit la
//! capture dans le modèle et jette les `Extras` des lignes remplacées.

use std::collections::{BTreeMap, BTreeSet};

use super::measure::Measures;
use crate::calendar::CalendarSet;
use crate::diagnostic::Subject;
use crate::duration::{Duration, Minutes, TimeBasis, Work};
use crate::ids::{AssignmentUid, BaselineNumber, ResourceUid, TaskUid};
use crate::model::{
    AssignmentBaseline, Extras, Project, ResourceBaseline, SchedulingMode, Task, TaskBaseline,
};
use crate::schedule::{Schedule, TaskRole, TaskSchedule};
use crate::time::Moment;

/// Portée d'une capture (R-221).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Scope {
    /// Projet entier : toutes les tâches planifiées, leurs affectations,
    /// toutes les ressources.
    #[default]
    Project,
    /// Ces tâches et leurs affectations ; UID 0 seulement s'il est cité.
    Tasks(Vec<TaskUid>),
}

/// Cumul vers les récapitulatives (R-222 ; S58 « Roll up baselines »).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rollup {
    /// Aucun cumul.
    #[default]
    None,
    /// « To all summary tasks » : chaque ancêtre d'une tâche de la portée.
    ToAllSummaries,
    /// « From subtasks into selected summary task(s) » : seules les
    /// récapitulatives de la sélection.
    FromSubtasks,
}

/// Demande de capture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaselineSpec {
    /// Numéro de la ligne (0 à 10).
    pub number: BaselineNumber,
    /// Portée.
    pub scope: Scope,
    /// Cumul.
    pub rollup: Rollup,
    /// Remplacer une ligne existante (R-223).
    pub overwrite: bool,
}

/// Lignes capturées, à écrire par [`apply`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BaselineEdit {
    /// Lignes des tâches (récapitulatives et UID 0 compris).
    pub tasks: BTreeMap<TaskUid, TaskBaseline>,
    /// Lignes des affectations.
    pub assignments: BTreeMap<AssignmentUid, AssignmentBaseline>,
    /// Lignes des ressources.
    pub resources: BTreeMap<ResourceUid, ResourceBaseline>,
    /// Tâches de la portée sautées (lignes vides, exclues du calcul ;
    /// `baseline.task_skipped`).
    pub skipped: Vec<TaskUid>,
    /// Un coût stocké a été recopié dans une ligne posée : PlannerCraft ne
    /// recalcule pas les coûts (`baseline.cost_not_computed`, arbitrage du
    /// 2026-10-20 de l'ADR-0014 : recopié, jamais effacé).
    pub cost_copied: bool,
}

/// Capture refusée.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BaselineError {
    /// Ligne déjà présente sur `count` entités de la portée, sans
    /// `overwrite` (`baseline.exists`, R-223).
    #[error("la ligne de base {} existe déjà sur {count} élément(s)", .number.get())]
    Exists {
        /// Numéro.
        number: BaselineNumber,
        /// Entités qui l'ont déjà.
        count: u32,
    },
    /// Tâche de la portée inexistante (`task.not_found`).
    #[error("aucune tâche d'UID {0}")]
    UnknownTask(TaskUid),
    /// Cumul demandé sans sélection (`baseline.rollup_needs_selection`,
    /// R-222).
    #[error("le cumul vers les récapitulatives demande une sélection de tâches")]
    RollupNeedsSelection,
}

/// Travail d'une ligne posée : absent quand il est nul, comme MS Project
/// (A7, O42 : 16 tâches et 6 affectations de travail nul sans `Work` de
/// base) ; un travail de base absent vaut 0 pour les écarts (R-225).
fn stored_work(w: Work) -> Option<Work> {
    (w != Work::ZERO).then_some(w)
}

/// Tâche du modèle et son planning (UID 0 : récapitulative du projet) ;
/// `None` pour une ligne vide, une tâche exclue du calcul ou un UID 0 sans
/// récapitulative stockée dans le modèle (rien où poser la ligne).
fn lookup<'a>(p: &'a Project, s: &'a Schedule, uid: TaskUid) -> Option<(&'a Task, &'a TaskSchedule)> {
    if uid == TaskUid::PROJECT_SUMMARY {
        return Some((p.summary_task.as_ref()?, s.project_summary.as_ref()?));
    }
    let task = p.tasks.get(uid).filter(|t| !t.is_null)?;
    Some((task, s.scheduled(uid)?))
}

/// R-220 : ligne d'une tâche planifiée — début, fin, durée calculée (unité
/// et estimation de la tâche ; base ouvrée pour une récapitulative, mesurée
/// sur son calendrier, R-054 ; tâche manuelle : durée saisie
/// `ManualDuration` quand il y en a une, O42 sur A7 UID 1), travail
/// calculé ; coût et coût fixe de la ligne remplacée recopiés tels quels
/// (R-223, arbitrage du 2026-10-20).
fn task_line(task: &Task, ts: &TaskSchedule, previous: Option<&TaskBaseline>) -> TaskBaseline {
    let duration = if let SchedulingMode::Manual {
        duration: Some(entered),
        ..
    } = task.mode
    {
        Some(entered)
    } else {
        let basis = match ts.role {
            TaskRole::Leaf => task.duration.basis(),
            TaskRole::Summary => TimeBasis::Working,
        };
        Duration::new(
            ts.duration,
            task.duration.unit(),
            basis,
            task.duration.is_estimated(),
        )
        .ok()
    };
    TaskBaseline {
        start: Some(ts.start),
        finish: Some(ts.finish),
        duration,
        work: stored_work(ts.work),
        cost: previous.and_then(|b| b.cost),
        fixed_cost: previous.and_then(|b| b.fixed_cost),
    }
}

/// Hiérarchie des tâches non vides (R-052) : parent de chaque tâche
/// (premier niveau : l'UID 0 quand `root` est vrai) et enfants dans
/// l'ordre des ID.
struct Outline {
    /// Parent de chaque tâche non vide.
    parent: BTreeMap<TaskUid, TaskUid>,
    /// Enfants de chaque récapitulative, dans l'ordre des ID.
    children: BTreeMap<TaskUid, Vec<TaskUid>>,
    /// Récapitulatives dans l'ordre inverse des ID, l'UID 0 en dernier :
    /// chaque enfant passe avant son parent.
    bottom_up: Vec<TaskUid>,
}

impl Outline {
    fn new(p: &Project, root: bool) -> Self {
        let mut parent = BTreeMap::new();
        let mut children: BTreeMap<TaskUid, Vec<TaskUid>> = BTreeMap::new();
        // Pile des tâches ouvertes : (niveau, UID).
        let mut stack: Vec<(u16, TaskUid)> = Vec::new();
        let mut order = Vec::new();
        for task in p.tasks.iter().filter(|t| !t.is_null) {
            let level = task.outline_level.get();
            while stack.last().is_some_and(|&(l, _)| l >= level) {
                stack.pop();
            }
            let up = match stack.last() {
                Some(&(_, uid)) => Some(uid),
                None if root => Some(TaskUid::PROJECT_SUMMARY),
                None => None,
            };
            if let Some(up) = up {
                parent.insert(task.uid(), up);
                children.entry(up).or_default().push(task.uid());
            }
            stack.push((level, task.uid()));
            order.push(task.uid());
        }
        let mut bottom_up: Vec<TaskUid> = order
            .into_iter()
            .rev()
            .filter(|u| children.contains_key(u))
            .collect();
        if root && children.contains_key(&TaskUid::PROJECT_SUMMARY) {
            bottom_up.push(TaskUid::PROJECT_SUMMARY);
        }
        Self {
            parent,
            children,
            bottom_up,
        }
    }
}

/// R-222 : ligne `n` d'une récapitulative recalculée sur les lignes `n`
/// de ses enfants planifiés (capturées ou stockées) : début min, fin max,
/// travail Σ, durée = temps ouvré entre les deux sur son échelle de mesure.
/// `None` si aucun enfant n'a de ligne `n` (enfants sans ligne ignorés).
fn rolled_line(
    project: &Project,
    schedule: &Schedule,
    number: BaselineNumber,
    target: (&Task, &TaskSchedule),
    children: &[TaskUid],
    captured: &BTreeMap<TaskUid, TaskBaseline>,
    measures: &Measures<'_>,
) -> Option<TaskBaseline> {
    let (task, _) = target;
    let mut any = false;
    let mut start: Option<Moment> = None;
    let mut finish: Option<Moment> = None;
    let mut work: Option<Minutes> = None;
    for &child in children {
        let Some((child_task, _)) = lookup(project, schedule, child) else {
            continue;
        };
        let Some(line) = captured.get(&child).or_else(|| child_task.baselines.get(&number)) else {
            continue;
        };
        any = true;
        start = [start, line.start].into_iter().flatten().min();
        finish = [finish, line.finish].into_iter().flatten().max();
        if let Some(w) = line.work {
            let sum = work.unwrap_or(Minutes::ZERO).checked_add(w.minutes());
            // Débordement (jamais attendu) : plafond du travail.
            work = Some(sum.unwrap_or(Minutes::new(Work::MAX_MINUTES)));
        }
    }
    if !any {
        return None;
    }
    let previous = task.baselines.get(&number).or_else(|| captured.get(&task.uid()));
    let duration = match (start, finish) {
        (Some(from), Some(to)) => {
            let m = measures.between(Some(task), TaskRole::Summary, from, to);
            Duration::new(
                Minutes::new(m.get().max(0)),
                task.duration.unit(),
                TimeBasis::Working,
                task.duration.is_estimated(),
            )
            .ok()
        }
        _ => None,
    };
    Some(TaskBaseline {
        start,
        finish,
        duration,
        work: work
            .and_then(|m| Work::new(Minutes::new(m.get().min(Work::MAX_MINUTES))).ok())
            .and_then(stored_work),
        cost: previous.and_then(|b| b.cost),
        fixed_cost: previous.and_then(|b| b.fixed_cost),
    })
}

/// Capture la ligne `spec.number` sur la portée de `spec`, à partir du
/// planning `s` de `p` (R-220 à R-223).
///
/// - Portée (R-221) : `Project` = toutes les tâches planifiées (UID 0
///   compris quand le modèle a une récapitulative du projet), leurs
///   affectations placées, toutes les ressources (UID 0 exclu, comme les
///   charges) ; `Tasks` = ces tâches et leurs affectations placées, aucune
///   ressource. Lignes vides et tâches exclues du calcul : `skipped`.
/// - Cumul (R-222) : `ToAllSummaries`, chaque ancêtre (UID 0 compris) d'une
///   tâche capturée ; `FromSubtasks`, chaque récapitulative de la sélection ;
///   ligne recalculée sur les lignes de ses enfants, des plus profondes aux
///   plus hautes.
/// - Remplacement (R-223) : une entité de la portée (hors ancêtres cumulés)
///   déjà dotée de la ligne ⇒ [`BaselineError::Exists`] sans `overwrite` ;
///   coût et coût fixe de la ligne remplacée recopiés tels quels
///   (`cost_copied`).
///
/// # Errors
///
/// [`BaselineError::UnknownTask`] (tâche de la sélection inexistante),
/// [`BaselineError::RollupNeedsSelection`] (cumul sur le projet entier ou
/// une sélection vide), [`BaselineError::Exists`].
pub fn capture(p: &Project, s: &Schedule, spec: &BaselineSpec) -> Result<BaselineEdit, BaselineError> {
    let n = spec.number;
    let selection: Option<BTreeSet<TaskUid>> = match &spec.scope {
        Scope::Project if spec.rollup != Rollup::None => return Err(BaselineError::RollupNeedsSelection),
        Scope::Project => None,
        Scope::Tasks(uids) => {
            if let Some(uid) = uids
                .iter()
                .find(|u| **u != TaskUid::PROJECT_SUMMARY && p.tasks.get(**u).is_none())
            {
                return Err(BaselineError::UnknownTask(*uid));
            }
            if uids.is_empty() && spec.rollup != Rollup::None {
                return Err(BaselineError::RollupNeedsSelection);
            }
            Some(uids.iter().copied().collect())
        }
    };

    // Tâches de la portée, dans l'ordre des UID.
    let candidates: Vec<TaskUid> = if let Some(set) = &selection {
        set.iter().copied().collect()
    } else {
        let mut all: BTreeSet<TaskUid> = p.tasks.uids().collect();
        if p.summary_task.is_some() {
            all.insert(TaskUid::PROJECT_SUMMARY);
        }
        all.into_iter().collect()
    };
    let mut edit = BaselineEdit::default();
    let mut existing: u32 = 0;
    for uid in candidates {
        let Some((task, ts)) = lookup(p, s, uid) else {
            edit.skipped.push(uid);
            continue;
        };
        let previous = task.baselines.get(&n);
        if previous.is_some() {
            existing = existing.saturating_add(1);
        }
        edit.tasks.insert(uid, task_line(task, ts, previous));
    }
    for a in &p.assignments {
        if !edit.tasks.contains_key(&a.task) {
            continue;
        }
        let Some(placed) = s.assignments.get(&a.uid()) else {
            continue;
        };
        let previous = a.baselines.get(&n);
        if previous.is_some() {
            existing = existing.saturating_add(1);
        }
        // Affectation fictive (sans ressource, −65 535 relue) : pas de
        // travail de base, comme MS Project (A7, O42).
        let line = AssignmentBaseline {
            start: Some(placed.span.start),
            finish: Some(placed.span.finish),
            work: a.resource.and_then(|_| stored_work(placed.work)),
            cost: previous.and_then(|b| b.cost),
        };
        edit.assignments.insert(a.uid(), line);
    }
    if selection.is_none() {
        for r in p.resources.iter().filter(|r| r.uid() != ResourceUid::new(0)) {
            let previous = r.baselines.get(&n);
            if previous.is_some() {
                existing = existing.saturating_add(1);
            }
            let work = s.resources.get(&r.uid()).map_or(Work::ZERO, |l| l.work);
            let line = ResourceBaseline {
                work: stored_work(work),
                cost: previous.and_then(|b| b.cost),
            };
            edit.resources.insert(r.uid(), line);
        }
    }
    if existing > 0 && !spec.overwrite {
        return Err(BaselineError::Exists {
            number: n,
            count: existing,
        });
    }

    if spec.rollup != Rollup::None {
        rollup(p, s, spec, &mut edit);
    }
    edit.cost_copied = edit
        .tasks
        .values()
        .any(|b| b.cost.is_some() || b.fixed_cost.is_some())
        || edit.assignments.values().any(|b| b.cost.is_some())
        || edit.resources.values().any(|b| b.cost.is_some());
    Ok(edit)
}

/// R-222 : cumul des lignes capturées vers les récapitulatives visées.
fn rollup(p: &Project, s: &Schedule, spec: &BaselineSpec, edit: &mut BaselineEdit) {
    let root = p.summary_task.is_some() && s.project_summary.is_some();
    let outline = Outline::new(p, root);
    let mut targets: BTreeSet<TaskUid> = BTreeSet::new();
    match spec.rollup {
        Rollup::None => return,
        Rollup::ToAllSummaries => {
            for &uid in edit.tasks.keys() {
                let mut cur = uid;
                // La hiérarchie est un arbre : au plus une montée par tâche.
                for _ in 0..=p.tasks.len() {
                    let Some(&up) = outline.parent.get(&cur) else {
                        break;
                    };
                    if !targets.insert(up) {
                        break;
                    }
                    cur = up;
                }
            }
        }
        Rollup::FromSubtasks => {
            targets.extend(
                edit.tasks
                    .keys()
                    .copied()
                    .filter(|u| outline.children.contains_key(u)),
            );
        }
    }
    let (set, _) = CalendarSet::compile(p);
    let measures = Measures::new(p, &set);
    for target in outline.bottom_up.iter().filter(|u| targets.contains(u)) {
        let Some(found) = lookup(p, s, *target) else {
            continue;
        };
        let Some(children) = outline.children.get(target) else {
            continue;
        };
        if let Some(line) = rolled_line(p, s, spec.number, found, children, &edit.tasks, &measures) {
            edit.tasks.insert(*target, line);
        }
    }
}

/// Origine des `Extras` relus d'un fichier MSPDI (`plannercraft_mspdi::extras::ORIGIN`).
const MSPDI_ORIGIN: &str = "mspdi";

/// Jette les `Extras` de portée `Baseline#n/` ; rend leurs clés triées,
/// sans doublon.
fn drop_scoped(extras: &mut Extras, n: BaselineNumber) -> Vec<String> {
    let prefix = format!("Baseline#{}/", n.get());
    let mut keys = Vec::new();
    extras.0.retain(|e| {
        let scoped = e.origin == MSPDI_ORIGIN && e.key.starts_with(&prefix);
        if scoped {
            keys.push(e.key.clone());
        }
        !scoped
    });
    keys.sort();
    keys.dedup();
    keys
}

/// Écrit la capture `edit` comme ligne `number` de `p` ; rend, par entité
/// dont la ligne a été remplacée, les clés d'`Extras` de portée
/// `Baseline#n/` jetées (R-223, extension de R-161), dans l'ordre : tâches,
/// ressources, affectations (UID croissants). Une entité absente du modèle
/// est ignorée ; l'UID 0 n'est écrit que si le modèle a une récapitulative
/// du projet.
pub fn apply(p: &mut Project, number: BaselineNumber, edit: BaselineEdit) -> Vec<(Subject, Vec<String>)> {
    let mut dropped = Vec::new();
    for (uid, line) in edit.tasks {
        let task = if uid == TaskUid::PROJECT_SUMMARY {
            p.summary_task.as_mut()
        } else {
            p.tasks.get_mut(uid)
        };
        let Some(task) = task else { continue };
        if task.baselines.insert(number, line).is_some() {
            let keys = drop_scoped(&mut task.extras, number);
            if !keys.is_empty() {
                dropped.push((Subject::Task(uid), keys));
            }
        }
    }
    for (uid, line) in edit.resources {
        let Some(r) = p.resources.get_mut(uid) else {
            continue;
        };
        if r.baselines.insert(number, line).is_some() {
            let keys = drop_scoped(&mut r.extras, number);
            if !keys.is_empty() {
                dropped.push((Subject::Resource(uid), keys));
            }
        }
    }
    for (uid, line) in edit.assignments {
        let Some(a) = p.assignments.get_mut(uid) else {
            continue;
        };
        if a.baselines.insert(number, line).is_some() {
            let keys = drop_scoped(&mut a.extras, number);
            if !keys.is_empty() {
                dropped.push((Subject::Assignment(uid), keys));
            }
        }
    }
    dropped
}
