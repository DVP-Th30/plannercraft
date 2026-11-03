//! Valeurs calculées par MS Project, telles qu'enregistrées dans le fichier
//! (R-083). Elles ne vont jamais dans le `Project` : le harnais du corpus les
//! compare au moteur, qui les recalcule.

use std::collections::BTreeMap;

use plannercraft_core::{AssignmentUid, Decimal, Minutes, Moment, ResourceUid, TaskUid, Work};

/// Dixièmes de minute, tels que MSPDI les écrit (`TotalSlack`, `FreeSlack`,
/// `StartSlack`, `FinishSlack`). Exemple : 9 600 = 960 minutes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Tenths(pub i64);

/// Planning enregistré.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecordedSchedule {
    /// `StartDate` du projet.
    pub project_start: Option<Moment>,
    /// `FinishDate` du projet.
    pub project_finish: Option<Moment>,
    /// Une entrée par tâche lue, UID 0 compris.
    pub tasks: BTreeMap<TaskUid, RecordedTask>,
    /// Une entrée par affectation lue (R-198).
    pub assignments: BTreeMap<AssignmentUid, RecordedAssignment>,
    /// Une entrée par ressource lue, UID 0 compris (R-198).
    pub resources: BTreeMap<ResourceUid, RecordedResource>,
}

/// Valeurs enregistrées d'une tâche ; chaque champ est absent s'il manque
/// ou s'il est illisible (diagnostic).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecordedTask {
    /// `Start`.
    pub start: Option<Moment>,
    /// `Finish`.
    pub finish: Option<Moment>,
    /// `EarlyStart`.
    pub early_start: Option<Moment>,
    /// `EarlyFinish`.
    pub early_finish: Option<Moment>,
    /// `LateStart`.
    pub late_start: Option<Moment>,
    /// `LateFinish`.
    pub late_finish: Option<Moment>,
    /// `TotalSlack`, en dixièmes de minute.
    pub total_slack: Option<Tenths>,
    /// `FreeSlack`, en dixièmes de minute.
    pub free_slack: Option<Tenths>,
    /// `StartSlack`, en dixièmes de minute (R-083, réécrit tel quel avec
    /// `Computed::Recorded`, R-122).
    pub start_slack: Option<Tenths>,
    /// `FinishSlack`, en dixièmes de minute.
    pub finish_slack: Option<Tenths>,
    /// `Critical`.
    pub critical: Option<bool>,
    /// `Summary`.
    pub summary: Option<bool>,
    /// `Duration` enregistrée (récapitulatives : valeur calculée).
    pub duration: Option<Minutes>,
    /// Nombre d'affectations de la tâche (section `Assignments`, `TaskUID`).
    pub assignments: u32,
    /// `Stop` : fin de la partie réelle (R-214, R-215).
    pub stop: Option<Moment>,
    /// `Resume` : reprise du reste, telle qu'écrite (R-214 ; celle d'une
    /// feuille commencée non achevée est aussi lue dans le modèle,
    /// `Progress::resume`).
    pub resume: Option<Moment>,
    /// `StartVariance` : minutes × 1000 (R-226).
    pub start_variance: Option<i64>,
    /// `FinishVariance` : minutes × 1000 (R-226).
    pub finish_variance: Option<i64>,
    /// `WorkVariance` : minutes × 1000, décimal normalisé (R-226).
    pub work_variance: Option<Decimal>,
}

/// Valeurs enregistrées d'une affectation (R-198) ; chaque champ est absent
/// s'il manque ou s'il est illisible (diagnostic).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecordedAssignment {
    /// `Start`.
    pub start: Option<Moment>,
    /// `Finish`.
    pub finish: Option<Moment>,
    /// `Work` (aussi lu dans le modèle, `Assignment::work`).
    pub work: Option<Work>,
    /// `ActualStart` (R-214, R-215).
    pub actual_start: Option<Moment>,
    /// `ActualFinish`.
    pub actual_finish: Option<Moment>,
    /// `Stop` : fin de la partie réelle.
    pub stop: Option<Moment>,
    /// `Resume` : reprise du reste.
    pub resume: Option<Moment>,
    /// `StartVariance` (unité non documentée par pj15, R-226).
    pub start_variance: Option<i64>,
    /// `FinishVariance` (unité non documentée par pj15, R-226).
    pub finish_variance: Option<i64>,
    /// `WorkVariance` : minutes × 1000, décimal normalisé (R-226).
    pub work_variance: Option<Decimal>,
}

/// Valeurs enregistrées d'une ressource (R-198) ; chaque champ est absent
/// s'il manque ou s'il est illisible (diagnostic).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecordedResource {
    /// `Work` : travail total de la ressource.
    pub work: Option<Work>,
    /// `OverAllocated` (O31 : drapeau de MS Project, pas un oracle du pic).
    pub overallocated: Option<bool>,
    /// `WorkVariance` : minutes × 1000, décimal normalisé (R-226).
    pub work_variance: Option<Decimal>,
}
