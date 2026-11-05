//! Résultat d'un lot appliqué (contrat J4, ADR-0012) : partagé par L2
//! (pipeline), L4 et L5 (effets de bord), L6 (vues).

use std::collections::BTreeMap;

use plannercraft_core::{
    AssignmentUid, CalendarUid, ConstraintKind, Diagnostic, ResourceUid, Subject, TaskId, TaskUid,
};
use serde::Serialize;

use crate::diff::ScheduleDiff;

/// Résultat de `Session::apply`, `undo` ou `redo`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ApplyOutcome {
    /// Vrai si l'état de la session a changé (faux en `DryRun` ou lot vide).
    pub committed: bool,
    /// Étiquettes `$x` → UID créées par le lot (R-150).
    pub labels: BTreeMap<String, TaskUid>,
    /// Tâches créées, dans l'ordre.
    pub created: Vec<TaskUid>,
    /// Diff de planning (R-163).
    pub diff: ScheduleDiff,
    /// Avertissements et informations nouveaux (R-160), triés.
    pub warnings: Vec<Diagnostic>,
    /// Effets de bord, dans l'ordre.
    pub side_effects: Vec<SideEffect>,
    /// Nombre d'entrées de l'historique après l'opération.
    pub history_len: usize,
    /// Étiquettes `$x` → UID des ressources créées par le lot (J5, R-195) ;
    /// absent du JSON quand vide.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub resource_labels: BTreeMap<String, ResourceUid>,
    /// Ressources créées, dans l'ordre (J5) ; absent du JSON quand vide.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub created_resources: Vec<ResourceUid>,
}

/// Effet de bord d'une commande, signalé à l'appelant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SideEffect {
    /// Lien retiré par une suppression (R-155).
    LinkRemoved {
        /// Prédécesseur.
        pred: TaskUid,
        /// Successeur.
        succ: TaskUid,
    },
    /// Affectation retirée par une suppression (R-155).
    AssignmentRemoved(AssignmentUid),
    /// Clés d'`Extras` jetées (R-161).
    ExtrasDropped {
        /// Entité.
        subject: Subject,
        /// Clés jetées.
        keys: Vec<String>,
    },
    /// ID renumérotés (R-150).
    IdsRenumbered {
        /// Premier ID changé.
        from: TaskId,
        /// Nombre de tâches renumérotées.
        count: usize,
    },
    /// Valeurs relues du fichier jetées au premier commit (R-162).
    RecordedDiscarded,
    /// Tâche supprimée (R-155).
    TaskDeleted(TaskUid),
    /// Contrainte posée par une date saisie (R-158).
    ConstraintSet {
        /// Tâche.
        uid: TaskUid,
        /// Type posé.
        kind: ConstraintKind,
    },
    /// Calendrier propre créé pour une ressource (`resource.add`, O34 ;
    /// J5).
    CalendarCreated(CalendarUid),
    /// Affectation fictive −65 535 retirée par l'affectation d'une vraie
    /// ressource (`assignment.add`, O24 ; J5).
    PlaceholderRemoved(AssignmentUid),
    /// Ligne de base enregistrée (`baseline.set`, R-227 ; J6) : nombre de
    /// lignes posées par genre d'entité.
    BaselineSet {
        /// Numéro de la ligne (0 à 10).
        number: u8,
        /// Tâches (récapitulatives et UID 0 compris).
        tasks: u32,
        /// Affectations.
        assignments: u32,
        /// Ressources.
        resources: u32,
    },
    /// Calendrier supprimé (`calendar.delete`, R-248 ; J7).
    CalendarDeleted(CalendarUid),
    /// Ligne de base effacée (`baseline.clear`, R-250 ; J7) : nombre de
    /// lignes retirées par genre d'entité.
    BaselineCleared {
        /// Numéro de la ligne (0 à 10).
        number: u8,
        /// Tâches (UID 0 compris).
        tasks: u32,
        /// Affectations.
        assignments: u32,
        /// Ressources.
        resources: u32,
    },
    /// Tâche laissée telle quelle par `progress.reschedule` (R-217 ; J6) :
    /// `reason` en code stable (`constraint`).
    RescheduleSkipped {
        /// Tâche.
        task: TaskUid,
        /// Raison.
        reason: &'static str,
    },
    /// Tâche décalée par `task.shift` (R-267) : contrainte posée (tâche
    /// automatique, avec `constraint_set`) ou dates manuelles déplacées.
    TaskShifted {
        /// Tâche.
        task: TaskUid,
    },
    /// Tâche ignorée par `task.shift` (R-267) : `reason` en code stable
    /// (`inactive`, `complete`, `undated`).
    ShiftSkipped {
        /// Tâche.
        task: TaskUid,
        /// Raison.
        reason: &'static str,
    },
    /// Contrainte autre que « Dès que possible » ou « Début au plus tôt
    /// le » remplacée par `task.shift` (R-267).
    ShiftConstraintReplaced {
        /// Tâche.
        task: TaskUid,
        /// Type remplacé.
        previous: ConstraintKind,
    },
    /// Début obtenu après la date visée par `task.shift` (prédécesseurs) :
    /// le décalage est fait, mais limité (R-267).
    ShiftLimited {
        /// Tâche.
        task: TaskUid,
    },
}
