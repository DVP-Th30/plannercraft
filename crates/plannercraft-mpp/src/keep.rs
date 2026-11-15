//! « Garder les dates de MS Project » (R-299, lot M3, ADR-0038) : quelles
//! tâches passer en planification manuelle, avec leur début et leur fin
//! enregistrés, pour que le planning recalculé n'ait plus d'écart réel
//! (R-298) avec le fichier.
//!
//! Rien n'est écrit ici : la fonction rend la liste, l'appelant la pose
//! par les commandes ordinaires (`task.update`, une entrée d'annulation).
//! Les dates gardées sont des **valeurs saisies** d'une tâche manuelle
//! (R-063), comme dans MS Project ; le moteur calcule tout le reste
//! (règle 3).

use std::collections::BTreeMap;

use plannercraft_core::{Moment, Project, SchedulingMode, TaskOutcome, TaskRole, TaskUid};
use serde::Serialize;

use crate::Recorded;
use crate::compare::{GapClass, compare};

/// Passes au plus : une tâche gardée à ses dates peut déplacer un
/// successeur jusque-là identique, gardé à la passe suivante.
pub const MAX_PASSES: usize = 8;

/// Dates enregistrées gardées pour une tâche.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct KeptDates {
    /// Tâche.
    pub uid: TaskUid,
    /// Début enregistré par MS Project.
    pub start: Moment,
    /// Fin enregistrée par MS Project.
    pub finish: Moment,
}

/// Tâches à passer en planification manuelle avec leurs dates enregistrées
/// (R-299), dans l'ordre du planning : les **feuilles** en écart réel
/// (R-298 ; les écarts d'arrondi restent), par passes (au plus
/// [`MAX_PASSES`]) jusqu'à ce qu'aucune feuille nouvelle ne soit en écart
/// réel. Une récapitulative n'est jamais gardée : ses dates suivent ses
/// sous-tâches (R-053, R-157). Une feuille déjà gardée qui reste en écart
/// (début réel qui remplace le début saisi, R-208) n'est pas reprise.
#[must_use]
pub fn kept_dates(project: &Project, recorded: &Recorded) -> Vec<KeptDates> {
    let mut work = project.clone();
    let mut kept: BTreeMap<TaskUid, KeptDates> = BTreeMap::new();
    for _ in 0..MAX_PASSES {
        let schedule = plannercraft_core::schedule(&work);
        let comparison = compare(&work, &schedule, recorded);
        let mut added = false;
        for gap in comparison.gaps.iter().filter(|g| g.class == GapClass::Real) {
            let uid = TaskUid::new(gap.uid);
            if kept.contains_key(&uid) {
                continue;
            }
            let leaf = matches!(
                schedule.get(uid),
                Some(TaskOutcome::Scheduled(s)) if s.role == TaskRole::Leaf
            );
            let Some((start, finish)) = recorded.tasks.get(&uid).and_then(|r| r.start.zip(r.finish)) else {
                continue;
            };
            if !leaf || finish < start {
                continue;
            }
            let Some(task) = work.tasks.get_mut(uid) else {
                continue;
            };
            task.mode = SchedulingMode::Manual {
                start: Some(start),
                finish: Some(finish),
                duration: Some(task.duration),
            };
            kept.insert(uid, KeptDates { uid, start, finish });
            added = true;
        }
        if !added {
            break;
        }
    }
    project
        .tasks
        .iter()
        .filter_map(|t| kept.get(&t.uid()).copied())
        .collect()
}
