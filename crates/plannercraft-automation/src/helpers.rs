//! Raccourcis d'édition de l'interface (R-252, R-253, R-255 ; ADR-0016 D4).
//!
//! Chaque raccourci rend un lot ordinaire (`progress.update`, `task.update`)
//! appliqué par le registre : une entrée d'annulation, `dry_run` gratuit ;
//! `None` quand il n'y a rien à faire. Contrat posé par le lot L0,
//! implémenté en L2. Les opérations suivent l'ordre des ID ; doublons de la
//! sélection fusionnés, lignes vides ignorées.

use std::collections::BTreeSet;

use plannercraft_core::schedule::{Exclusion, TaskOutcome};
use plannercraft_core::{Percent, Project, SchedulingMode, Task, TaskUid};

use crate::command::{
    Batch, Command, ModeJson, Patch, PercentJson, ProgressUpdate, TaskRef, TaskUpdate, is_summary_at,
};
use crate::session::Session;

/// Pas de « marquer n % » (boutons 0 à 100 %, H-UI-35).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PercentStep {
    /// 0 %.
    P0,
    /// 25 %.
    P25,
    /// 50 %.
    P50,
    /// 75 %.
    P75,
    /// 100 %.
    P100,
}

impl PercentStep {
    /// % achevé en centièmes (10 000 = 100 %).
    #[must_use]
    pub const fn hundredths(self) -> u16 {
        match self {
            Self::P0 => 0,
            Self::P25 => 2_500,
            Self::P50 => 5_000,
            Self::P75 => 7_500,
            Self::P100 => 10_000,
        }
    }
}

/// Refus d'un raccourci.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum HelperError {
    /// Tâche inconnue.
    #[error("tâche inconnue : {0:?}")]
    UnknownTask(TaskUid),
    /// Raccourci pas encore disponible (contrat L0 ; plus rendu depuis L2,
    /// gardé pour la stabilité de l'API).
    #[error("raccourci pas encore disponible")]
    NotYetAvailable,
}

/// Positions de la sélection `uids` (doublons fusionnés, lignes vides
/// ignorées) ; `cascade` : descendants des récapitulatives sélectionnées
/// compris (une passe, R-052 : lignes vides ignorées).
fn selected(project: &Project, uids: &[TaskUid], cascade: bool) -> Result<BTreeSet<usize>, HelperError> {
    let mut picked = BTreeSet::new();
    for &uid in uids {
        let pos = project.tasks.position(uid).ok_or(HelperError::UnknownTask(uid))?;
        if project.tasks.at(pos).is_some_and(|t| !t.is_null) {
            picked.insert(pos);
        }
    }
    if !cascade || picked.is_empty() {
        return Ok(picked);
    }
    let mut out = BTreeSet::new();
    // Niveau de la récapitulative sélectionnée qui couvre la position.
    let mut cover: Option<u16> = None;
    for (pos, t) in project.tasks.iter().enumerate() {
        if t.is_null {
            continue;
        }
        let level = t.outline_level.get();
        if cover.is_some_and(|c| level <= c) {
            cover = None;
        }
        if cover.is_some() {
            out.insert(pos);
        } else if picked.contains(&pos) {
            out.insert(pos);
            cover = Some(level);
        }
    }
    Ok(out)
}

/// Lot ordinaire des opérations `ops` ; `None` si vide.
fn batch_of(ops: Vec<Command>) -> Option<Batch> {
    (!ops.is_empty()).then_some(Batch {
        format_version: Batch::FORMAT_VERSION,
        lang: None,
        ops,
        label: None,
    })
}

/// `task.update` de `uid` sans champ.
const fn task_update(uid: TaskUid) -> TaskUpdate {
    TaskUpdate {
        uid: TaskRef::Uid(uid),
        name: Patch::Keep,
        duration: Patch::Keep,
        milestone: Patch::Keep,
        notes: Patch::Keep,
        start: Patch::Keep,
        finish: Patch::Keep,
        active: Patch::Keep,
        task_type: Patch::Keep,
        effort_driven: Patch::Keep,
        work: Patch::Keep,
        mode: Patch::Keep,
        custom: std::collections::BTreeMap::new(),
    }
}

/// Tâches non vides aux positions `positions`, dans l'ordre des ID.
fn tasks_at<'a>(project: &'a Project, positions: &'a BTreeSet<usize>) -> impl Iterator<Item = &'a Task> + 'a {
    positions.iter().filter_map(|&pos| project.tasks.at(pos))
}

/// R-252 : marque n % les feuilles de la sélection : chaque feuille reçoit
/// `progress.update { percent_complete: n × 100 }` ; une récapitulative
/// sélectionnée vaut ses feuilles descendantes ; tâches inactives (ou sous
/// une récapitulative inactive) ignorées, jalons de durée nulle ignorés
/// pour 25, 50 et 75 % ;
/// aucune opération pour une feuille dont le % stocké après la saisie
/// (R-201) serait le % courant. Le % des récapitulatives vient du moteur.
///
/// # Errors
///
/// [`HelperError::UnknownTask`] : UID absent du projet.
pub fn mark_percent(s: &Session, uids: &[TaskUid], p: PercentStep) -> Result<Option<Batch>, HelperError> {
    let project = s.project();
    let schedule = s.schedule();
    let positions = selected(project, uids, true)?;
    let percent = Percent::new(i64::from(p.hundredths())).unwrap_or(Percent::ZERO);
    let mut ops = Vec::new();
    for (pos, t) in positions
        .iter()
        .filter_map(|&pos| project.tasks.at(pos).map(|t| (pos, t)))
    {
        if is_summary_at(&project.tasks, pos) {
            continue;
        }
        let inactive = matches!(
            schedule.get(t.uid()),
            Some(TaskOutcome::Excluded {
                reason: Exclusion::Inactive | Exclusion::InactiveAncestor,
                ..
            })
        ) || !t.active;
        // Jalon de durée nulle : seulement 0 ou 100 % (un % partiel y
        // serait arrondi, `progress.milestone_rounded`).
        let partial_on_zero = t.duration.is_zero() && !matches!(p, PercentStep::P0 | PercentStep::P100);
        let current = t.progress.percent_complete;
        if inactive
            || partial_on_zero
            || current == percent
            || crate::gesture::stored_percent(t, percent) == current
        {
            continue;
        }
        ops.push(Command::ProgressUpdate(ProgressUpdate {
            task: TaskRef::Uid(t.uid()),
            percent_complete: Some(PercentJson {
                value: Some(percent.get()),
                text: None,
            }),
            actual_start: None,
            actual_finish: None,
            actual_duration: None,
            remaining_duration: None,
            percent_work_complete: None,
            actual_work: None,
            physical_percent_complete: None,
        }));
    }
    Ok(batch_of(ops))
}

/// R-253 : bascule auto ↔ manuel de la sélection (H-UI-36) : si au moins
/// une tâche sélectionnée est automatique, toutes passent en manuel (dates
/// planifiées figées par `task.update`), sinon toutes en automatique ; sans
/// cascade aux descendants ; seules les tâches dont le mode change ont une
/// opération.
///
/// # Errors
///
/// [`HelperError::UnknownTask`] : UID absent du projet.
pub fn toggle_mode(s: &Session, uids: &[TaskUid]) -> Result<Option<Batch>, HelperError> {
    let project = s.project();
    let positions = selected(project, uids, false)?;
    let is_manual = |t: &Task| matches!(t.mode, SchedulingMode::Manual { .. });
    let to_manual = tasks_at(project, &positions).any(|t| !is_manual(t));
    let target = if to_manual {
        ModeJson::Manual
    } else {
        ModeJson::Auto
    };
    let ops = tasks_at(project, &positions)
        .filter(|t| is_manual(t) != to_manual)
        .map(|t| {
            Command::TaskUpdate(TaskUpdate {
                mode: Patch::Set(target),
                ..task_update(t.uid())
            })
        })
        .collect();
    Ok(batch_of(ops))
}

/// R-255 : bascule actif / inactif de la sélection : si au moins une tâche
/// sélectionnée est active (drapeau propre), toutes deviennent inactives,
/// sinon toutes actives ; une récapitulative entraîne ses descendants ;
/// seules les tâches dont le drapeau change ont une opération.
///
/// # Errors
///
/// [`HelperError::UnknownTask`] : UID absent du projet.
pub fn toggle_active(s: &Session, uids: &[TaskUid]) -> Result<Option<Batch>, HelperError> {
    let project = s.project();
    let picked = selected(project, uids, false)?;
    let any_active = tasks_at(project, &picked).any(|t| t.active);
    let target = !any_active;
    let positions = selected(project, uids, true)?;
    let ops = tasks_at(project, &positions)
        .filter(|t| t.active != target)
        .map(|t| {
            Command::TaskUpdate(TaskUpdate {
                active: Patch::Set(target),
                ..task_update(t.uid())
            })
        })
        .collect();
    Ok(batch_of(ops))
}
