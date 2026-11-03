//! Ajout et retrait de tâches de la chronologie (ADR-0017 D4, D7 ;
//! H-UI-79) : un seul `style.apply { patch: { timeline } }` par geste, une
//! entrée d'annulation ; la liste vit dans le style du document
//! (`PlanStyle.timeline`).
//!
//! Bascule sur la sélection : si une tâche sélectionnée n'est pas sur la
//! frise, toutes y vont ; sinon toutes en sortent. Liste écrite triée par
//! UID, sans doublon, au plus [`MAX_TIMELINE_TASKS`] UID (au-delà : refus,
//! rien d'écrit). Un UID d'une tâche supprimée reste dans la liste pendant
//! la session (ignoré à l'affichage, H-UI-79) : la liste n'est jamais
//! nettoyée ici, un geste ne touche que les tâches qu'il désigne. Elle
//! l'est à l'ouverture du fichier (`style::check_opened`), avant qu'une
//! tâche neuve puisse reprendre l'UID.

use plannercraft_automation::command::Command;
use plannercraft_automation::command::style::{StyleApply, TemplateJson, doc_style, effective_style};
use plannercraft_automation::{Batch, Session};
use plannercraft_core::TaskUid;
use plannercraft_render::style::{MAX_TIMELINE_TASKS, TemplateId, TimelineSpec};

use crate::action::TimelineOp;
use crate::i18n::TextId;
use crate::selection::Selection;

/// Refus d'une opération de la chronologie (message au bandeau).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimelineOpError {
    /// Plus de [`MAX_TIMELINE_TASKS`] tâches : rien n'est écrit.
    TooMany,
}

impl TimelineOpError {
    /// Message du refus.
    #[must_use]
    pub const fn text_id(self) -> TextId {
        match self {
            Self::TooMany => TextId::TimelineTooMany,
        }
    }

    /// Plafond cité par le message (`$max`).
    #[must_use]
    pub const fn max() -> usize {
        MAX_TIMELINE_TASKS
    }
}

/// Nouvelle liste de la chronologie après `op` sur `current` (UID triés ou
/// non) ; `selected` : UID des tâches sélectionnées existantes. `None` :
/// rien à faire (sélection vide, liste inchangée, opération sans écriture).
///
/// # Errors
///
/// [`TimelineOpError::TooMany`] si la liste dépasserait
/// [`MAX_TIMELINE_TASKS`] UID.
pub fn next_list(
    current: &[u32],
    selected: &[u32],
    op: TimelineOp,
) -> Result<Option<Vec<u32>>, TimelineOpError> {
    let mut list = current.to_vec();
    list.sort_unstable();
    list.dedup();
    let before = list.clone();
    match op {
        TimelineOp::ToggleSelection => {
            if selected.is_empty() {
                return Ok(None);
            }
            let all_in = selected.iter().all(|uid| list.binary_search(uid).is_ok());
            if all_in {
                list.retain(|uid| !selected.contains(uid));
            } else {
                list.extend_from_slice(selected);
                list.sort_unstable();
                list.dedup();
            }
        }
        TimelineOp::Remove(uid) => list.retain(|u| *u != uid.get()),
        TimelineOp::SetVisible(_) | TimelineOp::Activate(_) => return Ok(None),
    }
    if list == before {
        return Ok(None);
    }
    if list.len() > MAX_TIMELINE_TASKS {
        return Err(TimelineOpError::TooMany);
    }
    Ok(Some(list))
}

/// Lot qui applique `op` (`ToggleSelection` ou `Remove`) à la liste de la
/// chronologie du document ; `default` : modèle des documents sans style
/// (préférence, comme la couleur de phase). `Ok(None)` : rien à faire.
///
/// # Errors
///
/// [`TimelineOpError::TooMany`] : limite dépassée, rien n'est écrit.
pub fn batch(
    session: &Session,
    selection: &Selection,
    op: TimelineOp,
    default: TemplateId,
) -> Result<Option<Batch>, TimelineOpError> {
    let project = session.project();
    let current = effective_style(project, default).timeline.tasks;
    // Tâches sélectionnées existantes (ligne vide exclue), ordre des UID.
    let selected: Vec<u32> = selection
        .uids()
        .iter()
        .copied()
        .filter(|uid| project.tasks.get(*uid).is_some_and(|task| !task.is_null))
        .map(TaskUid::get)
        .collect();
    let Some(list) = next_list(&current, &selected, op)? else {
        return Ok(None);
    };
    let spec = TimelineSpec { tasks: list };
    let Ok(spec) = serde_json::to_value(&spec) else {
        return Ok(None);
    };
    // Document sans style : modèle par défaut des préférences, comme la
    // couleur de phase (J8 L15) et les mois relatifs (H-UI-67).
    let template = doc_style(project).is_none().then(|| TemplateJson::from(default));
    Ok(Some(Batch {
        format_version: Batch::FORMAT_VERSION,
        lang: None,
        ops: vec![Command::StyleApply(StyleApply {
            template,
            patch: Some(serde_json::json!({ "timeline": spec })),
            reset: false,
        })],
        label: None,
    }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests : un échec doit s'arrêter net")]
mod tests {
    use plannercraft_core::TaskUid;

    use super::{TimelineOpError, next_list};
    use crate::action::TimelineOp;

    #[test]
    fn h_ui_79_toggle_adds_missing_then_removes_all() {
        assert_eq!(
            next_list(&[], &[4, 3], TimelineOp::ToggleSelection),
            Ok(Some(vec![3, 4]))
        );
        // Une tâche absente : toutes y vont.
        assert_eq!(
            next_list(&[3], &[3, 6], TimelineOp::ToggleSelection),
            Ok(Some(vec![3, 6]))
        );
        // Toutes présentes : toutes en sortent, les autres restent.
        assert_eq!(
            next_list(&[2, 3, 6], &[3, 6], TimelineOp::ToggleSelection),
            Ok(Some(vec![2]))
        );
        assert_eq!(next_list(&[3], &[], TimelineOp::ToggleSelection), Ok(None));
    }

    #[test]
    fn h_ui_79_remove_and_noop() {
        let three = TaskUid::new(3);
        assert_eq!(
            next_list(&[3, 4], &[], TimelineOp::Remove(three)),
            Ok(Some(vec![4]))
        );
        assert_eq!(next_list(&[4], &[], TimelineOp::Remove(three)), Ok(None));
        assert_eq!(next_list(&[4], &[4], TimelineOp::SetVisible(true)), Ok(None));
        assert_eq!(next_list(&[4], &[4], TimelineOp::Activate(three)), Ok(None));
    }

    #[test]
    fn h_ui_79_limit_and_sorted_unique() {
        let full: Vec<u32> = (1..=200).collect();
        assert_eq!(
            next_list(&full, &[500], TimelineOp::ToggleSelection),
            Err(TimelineOpError::TooMany)
        );
        // Retirer reste possible au plafond.
        assert_eq!(
            next_list(&full, &[1], TimelineOp::ToggleSelection)
                .unwrap()
                .map(|l| l.len()),
            Some(199)
        );
        // Liste relue non triée ou en double : écrite triée, sans doublon.
        assert_eq!(
            next_list(&[9, 2, 9], &[5], TimelineOp::ToggleSelection),
            Ok(Some(vec![2, 5, 9]))
        );
    }
}
