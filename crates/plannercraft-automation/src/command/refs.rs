//! Désignation des tâches dans un lot (R-150) et lecture des paramètres
//! JSON communs aux commandes (durées, décalages).
//!
//! Une commande désigne une tâche par son UID (entier JSON) ou par
//! l'étiquette `"$x"` qu'un `task.add` antérieur du même lot lui a donnée
//! (`"as":"$x"`). L'ID (position + 1, R-052) n'est jamais accepté : il change
//! à chaque ajout, suppression ou déplacement.

use std::collections::BTreeMap;

use plannercraft_core::{Duration, Lag, Project, Subject, TaskUid};

use super::TaskRef;
use super::exec::Ctx;
use crate::error::{CommandError, CommandErrorKind, codes};
use crate::json::{DurationJson, LagJson};

/// Motif d'une étiquette (R-150) : `$` puis 1 à 64 caractères parmi
/// `A-Z a-z 0-9 _ -`.
pub const LABEL_PATTERN: &str = "^\\$[A-Za-z0-9_-]{1,64}$";

/// Longueur maximale d'une étiquette, `$` exclu.
const LABEL_MAX: usize = 64;

/// Vrai si `s` suit [`LABEL_PATTERN`].
#[must_use]
pub fn is_valid_label(s: &str) -> bool {
    s.strip_prefix('$').is_some_and(|rest| {
        (1..=LABEL_MAX).contains(&rest.len())
            && rest
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    })
}

/// Explication du refus d'une chaîne qui n'est pas une étiquette (lecture
/// JSON d'un [`TaskRef`]).
pub(super) fn label_refusal(s: &str) -> String {
    let shown: String = s.chars().take(LABEL_MAX).collect();
    if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
        format!("« {shown} » : un UID s'écrit en entier JSON (sans guillemets) ; l'ID n'est jamais accepté")
    } else {
        format!("« {shown} » : UID (entier) ou étiquette \"$x\" (motif {LABEL_PATTERN}) attendu")
    }
}

/// `param.invalid` pour une étiquette mal formée, sur le champ `field`.
fn bad_label(label: &str, field: &str) -> CommandError {
    CommandError::new(
        codes::PARAM_INVALID,
        CommandErrorKind::InvalidParam(label_refusal(label)),
    )
    .with_field(field)
}

/// R-150 : UID désigné par `r`, qui doit exister dans `project`.
///
/// - `Uid(u)` : `u` doit être une tâche du projet (la tâche récapitulative
///   du projet, UID 0, n'en est pas une) ;
/// - `Label(l)` : `l` suit [`LABEL_PATTERN`] et figure dans `labels` (table
///   des étiquettes du lot), et sa tâche existe encore.
///
/// # Errors
///
/// `task.not_found` (sujet : la tâche), `batch.unknown_label`, ou
/// `param.invalid` pour une chaîne sans `$` ; le champ est `field`.
pub fn resolve_task(
    r: &TaskRef,
    labels: &BTreeMap<String, TaskUid>,
    project: &Project,
    field: &str,
) -> Result<TaskUid, CommandError> {
    let uid = match r {
        TaskRef::Uid(u) => *u,
        TaskRef::Label(l) => {
            if !is_valid_label(l) {
                return Err(bad_label(l, field));
            }
            *labels.get(l).ok_or_else(|| {
                CommandError::new(
                    codes::BATCH_UNKNOWN_LABEL,
                    CommandErrorKind::UnknownLabel(l.clone()),
                )
                .with_field(field)
            })?
        }
    };
    if project.tasks.contains(uid) {
        Ok(uid)
    } else {
        Err(
            CommandError::new(codes::TASK_NOT_FOUND, CommandErrorKind::UnknownTask(uid))
                .with_field(field)
                .with_subject(Subject::Task(uid)),
        )
    }
}

/// R-150 : enregistre l'étiquette `label` (champ `as` d'un `task.add`) pour
/// la tâche `uid`. Une étiquette ne se définit qu'une fois par lot.
///
/// # Errors
///
/// `param.invalid` si `label` ne suit pas [`LABEL_PATTERN`] ;
/// `batch.duplicate_label` si elle est déjà définie (table inchangée).
pub fn define_label(
    label: &str,
    uid: TaskUid,
    labels: &mut BTreeMap<String, TaskUid>,
) -> Result<(), CommandError> {
    if !is_valid_label(label) {
        return Err(bad_label(label, "as"));
    }
    if labels.contains_key(label) {
        return Err(CommandError::new(
            codes::BATCH_DUPLICATE_LABEL,
            CommandErrorKind::DuplicateLabel(label.to_owned()),
        )
        .with_field("as"));
    }
    labels.insert(label.to_owned(), uid);
    Ok(())
}

#[allow(
    dead_code,
    reason = "contrat J4 : aides appelées par les commandes de L4 et L5"
)]
impl Ctx<'_> {
    /// R-150 : tâche désignée par `r` dans le clone du lot (voir
    /// [`resolve_task`]).
    pub(crate) fn task(&self, r: &TaskRef, field: &str) -> Result<TaskUid, CommandError> {
        resolve_task(r, self.labels, self.project, field)
    }

    /// R-150 : enregistre l'étiquette d'une tâche créée (voir
    /// [`define_label`]).
    pub(crate) fn define_label(&mut self, label: &str, uid: TaskUid) -> Result<(), CommandError> {
        define_label(label, uid, self.labels)
    }

    /// Durée JSON → modèle dans la langue du lot (voir
    /// [`DurationJson::to_duration`]), erreur sur le champ `field`.
    pub(crate) fn duration(&self, d: &DurationJson, field: &str) -> Result<Duration, CommandError> {
        d.to_duration(self.text).map_err(|e| e.with_field(field))
    }

    /// Décalage JSON → modèle (voir [`LagJson::to_lag`]), erreur sur le
    /// champ `field`.
    pub(crate) fn lag(&self, l: &LagJson, field: &str) -> Result<Lag, CommandError> {
        l.to_lag(self.text).map_err(|e| e.with_field(field))
    }
}
