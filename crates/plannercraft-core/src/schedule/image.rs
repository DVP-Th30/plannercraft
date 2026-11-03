//! Durée du modèle d'une feuille placée par ses affectations (R-173 amendée
//! le 2026-10-19).
//!
//! La durée d'une telle feuille est une **sortie** du moteur : la mesure de
//! l'union des intervalles travaillés de ses affectations (S14 : « the total
//! span of active working time »), et non le plus grand des `d_a`. Les
//! commandes (session, `plannercraft-automation`) la reportent dans
//! `Task::duration` après chaque recalcul et l'écrivain MSPDI l'écrit
//! (R-199) : la session et le fichier relu portent la même durée, et les
//! éditions suivantes (R-189 à R-194) partent de la même valeur.
//!
//! Exclues (la durée du modèle y est une **entrée** du placement, la
//! reporter changerait le placement suivant) : la durée fixe (`d_a` = durée
//! de la tâche, S41, R-170) et toute feuille dont une affectation de travail
//! a des unités nulles (`d_a` = durée de la tâche, R-181 : reporter l'union
//! l'allongerait à chaque recalcul) ; une feuille achevée ou en cours (J6,
//! R-209 : sa durée est celle de l'avancement).

use std::collections::BTreeSet;

use super::{AssignmentRole, Schedule, TaskOutcome, TaskRole, TaskSchedule};
use crate::duration::Duration;
use crate::ids::TaskUid;
use crate::model::{Project, SchedulingMode, Task, TaskType};
use crate::progress::ProgressState;
use crate::quantity::Units;

/// Feuilles dont la durée du modèle suit la durée calculée : placées par au
/// moins une affectation de travail (rôle `Work`, R-173), sans affectation
/// de travail à unités nulles (R-181). Amendement J6 (R-209) : une feuille
/// achevée ou en cours en sort, sa durée est celle de l'avancement (l'union
/// de ses affectations placées depuis le début réel écraserait la mesure ;
/// arbitrage de l'ADR-0014, `sync_assigned_durations`).
#[must_use]
pub fn placed_by_assignments(project: &Project, schedule: &Schedule) -> BTreeSet<TaskUid> {
    let mut placed = BTreeSet::new();
    let mut excluded = BTreeSet::new();
    for a in &project.assignments {
        let Some(s) = schedule.assignments.get(&a.uid()) else {
            continue;
        };
        if s.role != AssignmentRole::Work {
            continue;
        }
        // R-209 : feuille achevée ou en cours, durée de l'avancement.
        let started = schedule
            .scheduled(s.task)
            .and_then(|t| t.progress)
            .is_some_and(|p| p.state != ProgressState::NotStarted);
        if started {
            excluded.insert(s.task);
            continue;
        }
        if a.units == Units::ZERO {
            excluded.insert(s.task);
        } else {
            placed.insert(s.task);
        }
    }
    placed.retain(|t| !excluded.contains(t));
    placed
}

/// Durée calculée d'une feuille automatique de `placed` (voir
/// [`placed_by_assignments`]), non dégradée, hors durée fixe : la mesure de
/// l'union (`TaskSchedule::duration`), avec l'unité, la base et
/// l'estimation de la tâche. `None` : la durée du modèle reste telle quelle.
#[must_use]
pub fn assigned_leaf_duration(task: &Task, s: &TaskSchedule, placed: &BTreeSet<TaskUid>) -> Option<Duration> {
    let eligible = s.role == TaskRole::Leaf
        && !s.manual
        && !s.degraded
        && matches!(task.mode, SchedulingMode::Auto)
        && task.task_type != TaskType::FixedDuration
        && placed.contains(&task.uid());
    if !eligible {
        return None;
    }
    let d = &task.duration;
    Duration::new(s.duration, d.unit(), d.basis(), d.is_estimated()).ok()
}

/// Reporte dans `project` la durée calculée de chaque feuille éligible
/// ([`assigned_leaf_duration`]) qui en diffère ; `schedule` doit être celui
/// de `project`. Rend les UID des tâches modifiées, triés.
pub fn sync_assigned_durations(project: &mut Project, schedule: &Schedule) -> Vec<TaskUid> {
    let placed = placed_by_assignments(project, schedule);
    let mut changed = Vec::new();
    for &uid in &placed {
        let Some(TaskOutcome::Scheduled(s)) = schedule.get(uid) else {
            continue;
        };
        let Some(task) = project.tasks.get_mut(uid) else {
            continue;
        };
        if let Some(d) = assigned_leaf_duration(task, s, &placed)
            && d != task.duration
        {
            task.duration = d;
            changed.push(uid);
        }
    }
    changed
}
