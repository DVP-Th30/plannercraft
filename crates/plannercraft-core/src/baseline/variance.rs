//! Écarts à une ligne de base (R-224, R-225 ; lot L4).
//!
//! Écart = valeur − ligne de base (S59). Dates : minutes ouvrées signées
//! sur l'échelle de mesure de la tâche (R-113 ; civiles pour une feuille à
//! durée écoulée) ; sans ligne ou sans date de base : 0. Durée et travail :
//! valeur − valeur de base, une valeur de base absente comptant 0 (O43).

use super::measure::Measures;
use crate::calendar::CalendarSet;
use crate::duration::{Minutes, Work};
use crate::ids::{AssignmentUid, BaselineNumber, ResourceUid, TaskUid};
use crate::model::{Project, Task, TaskBaseline};
use crate::schedule::{Schedule, TaskOutcome, TaskSchedule};
use crate::time::Moment;

/// Écarts signés d'une tâche, en minutes ouvrées de son échelle de mesure
/// (R-113) : valeur − ligne de base.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Variance {
    /// Écart de début (R-224) ; 0 sans ligne.
    pub start: Minutes,
    /// Écart de fin (R-224) ; 0 sans ligne.
    pub finish: Minutes,
    /// Écart de durée (R-225).
    pub duration: Minutes,
    /// Écart de travail (R-225) ; sans ligne : le travail (O43).
    pub work: Minutes,
}

/// Écarts d'une tâche.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskVariance {
    /// Tâche (UID 0 : projet).
    pub uid: TaskUid,
    /// Ligne de base de référence ; `None` si la tâche n'en a pas.
    pub baseline: Option<TaskBaseline>,
    /// Écarts.
    pub variance: Variance,
}

/// Écarts du projet à la ligne `number`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variances {
    /// Numéro de la ligne de référence.
    pub number: BaselineNumber,
    /// Projet (UID 0).
    pub project: TaskVariance,
    /// Tâches planifiées, dans l'ordre des UID.
    pub tasks: Vec<TaskVariance>,
    /// Écart de travail des ressources (R-225).
    pub resources: Vec<(ResourceUid, Minutes)>,
    /// Écart de travail des affectations (R-225).
    pub assignments: Vec<(AssignmentUid, Minutes)>,
}

/// `a − b` en minutes, borné (jamais de débordement : les deux valeurs
/// sont des durées ou des travaux du domaine).
fn diff(a: Minutes, b: Minutes) -> Minutes {
    Minutes::new(a.get().saturating_sub(b.get()))
}

/// Travail d'une ligne de base, 0 si absent (R-225, O43).
fn base_work(w: Option<Work>) -> Minutes {
    w.map_or(Minutes::ZERO, Work::minutes)
}

/// R-224, R-225 : écarts d'une tâche planifiée.
fn task_variance(
    measures: &Measures<'_>,
    uid: TaskUid,
    task: Option<&Task>,
    ts: &TaskSchedule,
    number: BaselineNumber,
) -> TaskVariance {
    let baseline = task.and_then(|t| t.baselines.get(&number)).copied();
    let date = |base: Option<Moment>, now: Moment| {
        base.map_or(Minutes::ZERO, |b| measures.between(task, ts.role, b, now))
    };
    let variance = match baseline {
        None => Variance {
            start: Minutes::ZERO,
            finish: Minutes::ZERO,
            duration: ts.duration,
            work: ts.work.minutes(),
        },
        Some(b) => Variance {
            start: date(b.start, ts.start),
            finish: date(b.finish, ts.finish),
            duration: diff(ts.duration, b.duration.map_or(Minutes::ZERO, |d| d.minutes())),
            work: diff(ts.work.minutes(), base_work(b.work)),
        },
    };
    TaskVariance {
        uid,
        baseline,
        variance,
    }
}

/// Écarts du planning `s` de `p` à la ligne `number` (R-224, R-225), sur
/// les calendriers compilés `set` (`CalendarSet::compile(p)`).
///
/// - Tâches : les tâches planifiées, dans l'ordre des UID ; projet : l'UID 0
///   (ligne lue sur la récapitulative du projet du modèle ; écarts nuls si
///   rien n'est planifié).
/// - Ressources : toutes celles du modèle (UID 0 exclu, comme les charges),
///   travail calculé − travail de base.
/// - Affectations : les affectations placées, travail − travail de base ;
///   0 pour une affectation fictive (sans ressource).
#[must_use]
pub fn variance(p: &Project, s: &Schedule, set: &CalendarSet, number: BaselineNumber) -> Variances {
    let measures = Measures::new(p, set);
    let root = p.summary_task.as_ref();
    let project = match &s.project_summary {
        Some(ts) => task_variance(&measures, TaskUid::PROJECT_SUMMARY, root, ts, number),
        None => TaskVariance {
            uid: TaskUid::PROJECT_SUMMARY,
            baseline: root.and_then(|t| t.baselines.get(&number)).copied(),
            variance: Variance::default(),
        },
    };
    let tasks = s
        .tasks
        .iter()
        .filter_map(|(uid, outcome)| match outcome {
            TaskOutcome::Scheduled(ts) => Some(task_variance(&measures, *uid, p.tasks.get(*uid), ts, number)),
            TaskOutcome::Excluded { .. } => None,
        })
        .collect();
    let resources = p
        .resources
        .iter()
        .filter(|r| r.uid() != ResourceUid::new(0))
        .map(|r| {
            let work = s
                .resources
                .get(&r.uid())
                .map_or(Minutes::ZERO, |l| l.work.minutes());
            let base = base_work(r.baselines.get(&number).and_then(|b| b.work));
            (r.uid(), diff(work, base))
        })
        .collect();
    // Affectation fictive (sans ressource) : écart nul, son travail ne
    // compte pas (corpus : 75 affectations fictives de travail non nul,
    // `WorkVariance` 0 partout).
    let assignments = s
        .assignments
        .iter()
        .map(|(uid, placed)| {
            if placed.resource.is_none() {
                return (*uid, Minutes::ZERO);
            }
            let base = base_work(
                p.assignments
                    .get(*uid)
                    .and_then(|a| a.baselines.get(&number))
                    .and_then(|b| b.work),
            );
            (*uid, diff(placed.work.minutes(), base))
        })
        .collect();
    Variances {
        number,
        project,
        tasks,
        resources,
        assignments,
    }
}
