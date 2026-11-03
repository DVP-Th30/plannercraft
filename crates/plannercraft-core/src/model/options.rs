//! Options du projet (ADR-0008, § 5.2, R-032, R-033).
//!
//! Deux séries de valeurs par défaut :
//! - celles de l'application, pour un projet neuf : [`ProjectOptions::default`] ;
//! - celles du schéma MSPDI, pour un élément absent à l'import
//!   (`DefaultTaskType` = durée fixe, `NewTasksEffortDriven` = vrai, S2) :
//!   appliquées par `plannercraft-mspdi` (J3), avec leur propre règle.

use serde::{Deserialize, Serialize};

use crate::duration::{ConversionFactors, DurationUnit};
use crate::error::TimeError;
use crate::model::task::TaskType;
use crate::time::{Day, Moment, TimeOfDay, Weekday};

/// Options du projet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "options indépendantes reprises de MSPDI (S2), pas un état"
)]
pub struct ProjectOptions {
    /// Facteurs de conversion des unités (R-006).
    pub factors: ConversionFactors,
    /// Heure de début par défaut d'une date saisie sans heure (R-033).
    pub default_start_time: TimeOfDay,
    /// Heure de fin par défaut d'une date saisie sans heure (R-033).
    pub default_finish_time: TimeOfDay,
    /// Unité de saisie des durées.
    pub default_duration_unit: DurationUnit,
    /// Unité de saisie du travail.
    pub default_work_unit: DurationUnit,
    /// Type des nouvelles tâches.
    pub default_task_type: TaskType,
    /// Nouvelles tâches pilotées par l'effort.
    pub new_tasks_effort_driven: bool,
    /// Durée des nouvelles tâches estimée (« 1j? »).
    pub new_tasks_estimated: bool,
    /// Premier jour de la semaine (affichage seulement).
    pub week_start_day: Weekday,
    /// Respecter les contraintes plutôt que les liens.
    pub honor_constraints: bool,
    /// Seuil de criticité en jours : une tâche est critique si sa marge
    /// totale est inférieure ou égale à N jours (`CriticalSlackLimit`, S2,
    /// S19 ; R-050). 0 par défaut. Absent d'un JSON : 0.
    #[serde(default)]
    pub critical_slack_limit_days: u32,
    /// Calculer plusieurs chemins critiques (`MultipleCriticalPaths`, S2,
    /// S20 ; R-051). Faux par défaut. Absent d'un JSON : faux.
    #[serde(default)]
    pub multiple_critical_paths: bool,
    /// Options de l'avancement et de la date d'état (J6, R-205, R-218).
    /// Absentes du JSON quand elles valent leurs défauts : le format V1 et
    /// sa référence restent inchangés.
    #[serde(default, skip_serializing_if = "ProgressOptions::is_default")]
    pub progress: ProgressOptions,
}

/// Options de l'avancement (J6, ADR-0014 D2, D3 ; S54 ; O36).
///
/// `split_in_progress` (`SplitsInProgressTasks`) agit au recalcul (R-205) ;
/// `task_updates_resource` (`TaskUpdatesResource`) à la saisie (R-211). Les
/// quatre options `Move*` de la date d'état (S54 : elles n'agissent qu'à la
/// saisie) sont lues et réécrites ; seule la valeur `false` est calculée,
/// `true` donne `progress.option_unsupported` (R-218).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "options indépendantes reprises de MSPDI (S2, S54), pas un état"
)]
pub struct ProgressOptions {
    /// Fractionner les tâches en cours (`SplitsInProgressTasks`, vrai par
    /// défaut).
    #[serde(default = "yes")]
    pub split_in_progress: bool,
    /// Le % achevé de la tâche met à jour le travail réel des ressources
    /// (`TaskUpdatesResource`, vrai par défaut).
    #[serde(default = "yes")]
    pub task_updates_resource: bool,
    /// `MoveCompletedEndsBack` (faux par défaut).
    #[serde(default)]
    pub move_completed_ends_back: bool,
    /// `MoveRemainingStartsBack` (faux par défaut).
    #[serde(default)]
    pub move_remaining_starts_back: bool,
    /// `MoveRemainingStartsForward` (faux par défaut).
    #[serde(default)]
    pub move_remaining_starts_forward: bool,
    /// `MoveCompletedEndsForward` (faux par défaut).
    #[serde(default)]
    pub move_completed_ends_forward: bool,
}

/// Défaut `true` d'un champ serde.
const fn yes() -> bool {
    true
}

impl Default for ProgressOptions {
    /// Valeurs de MS Project pour un projet neuf, relevées dans les cinq
    /// fichiers du corpus (O36) : fractionnement et mise à jour des
    /// ressources actifs, options `Move*` inactives.
    fn default() -> Self {
        Self {
            split_in_progress: true,
            task_updates_resource: true,
            move_completed_ends_back: false,
            move_remaining_starts_back: false,
            move_remaining_starts_forward: false,
            move_completed_ends_forward: false,
        }
    }
}

impl ProgressOptions {
    /// Vrai si toutes les options valent leur défaut (champ omis du JSON).
    #[must_use]
    #[allow(
        clippy::trivially_copy_pass_by_ref,
        reason = "signature imposée par `skip_serializing_if`"
    )]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Options posées à une valeur que le calcul ne sait pas suivre
    /// (R-218) : `Move*` à vrai, `task_updates_resource` à faux ; dans
    /// l'ordre de [`ProgressOption`].
    #[must_use]
    pub fn unsupported(&self) -> Vec<ProgressOption> {
        [
            (ProgressOption::TaskUpdatesResource, !self.task_updates_resource),
            (
                ProgressOption::MoveCompletedEndsBack,
                self.move_completed_ends_back,
            ),
            (
                ProgressOption::MoveRemainingStartsBack,
                self.move_remaining_starts_back,
            ),
            (
                ProgressOption::MoveRemainingStartsForward,
                self.move_remaining_starts_forward,
            ),
            (
                ProgressOption::MoveCompletedEndsForward,
                self.move_completed_ends_forward,
            ),
        ]
        .into_iter()
        .filter_map(|(o, set)| set.then_some(o))
        .collect()
    }
}

/// Option d'avancement que le calcul ne suit pas quand elle quitte son
/// défaut (R-218, `progress.option_unsupported`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressOption {
    /// `TaskUpdatesResource` à faux.
    TaskUpdatesResource,
    /// `MoveCompletedEndsBack` à vrai.
    MoveCompletedEndsBack,
    /// `MoveRemainingStartsBack` à vrai.
    MoveRemainingStartsBack,
    /// `MoveRemainingStartsForward` à vrai.
    MoveRemainingStartsForward,
    /// `MoveCompletedEndsForward` à vrai.
    MoveCompletedEndsForward,
}

impl ProgressOption {
    /// Nom de l'élément MSPDI (S2, S54).
    #[must_use]
    pub const fn mspdi_name(self) -> &'static str {
        match self {
            Self::TaskUpdatesResource => "TaskUpdatesResource",
            Self::MoveCompletedEndsBack => "MoveCompletedEndsBack",
            Self::MoveRemainingStartsBack => "MoveRemainingStartsBack",
            Self::MoveRemainingStartsForward => "MoveRemainingStartsForward",
            Self::MoveCompletedEndsForward => "MoveCompletedEndsForward",
        }
    }
}

impl Default for ProjectOptions {
    /// Valeurs de l'application pour un projet neuf (R-032) : 480 / 2 400 /
    /// 20 (S8) ; 08:00 et 17:00 (S8, S9) ; durées en jours (S10), travail en
    /// heures ; unités fixes, sans pilotage par l'effort (hypothèse) ;
    /// estimées (S14) ; semaine commençant le lundi (hypothèse, version
    /// française) ; contraintes respectées (S2) ; seuil de criticité 0 et un
    /// seul chemin critique (S19, S20).
    fn default() -> Self {
        Self {
            factors: ConversionFactors::default(),
            default_start_time: TimeOfDay::hm(8, 0).unwrap_or(TimeOfDay::MIDNIGHT),
            default_finish_time: TimeOfDay::hm(17, 0).unwrap_or(TimeOfDay::END_OF_DAY),
            default_duration_unit: DurationUnit::Day,
            default_work_unit: DurationUnit::Hour,
            default_task_type: TaskType::FixedUnits,
            new_tasks_effort_driven: false,
            new_tasks_estimated: true,
            week_start_day: Weekday::Monday,
            honor_constraints: true,
            critical_slack_limit_days: 0,
            multiple_critical_paths: false,
            progress: ProgressOptions::default(),
        }
    }
}

impl ProjectOptions {
    /// R-033 : une date de début saisie sans heure reçoit l'heure de début par
    /// défaut. L'alignement sur le calendrier se fait ensuite au placement
    /// (R-030).
    ///
    /// # Errors
    ///
    /// [`TimeError`] si l'instant sort du domaine (impossible : un jour du
    /// domaine à une heure de 00:00 à 24:00 y reste).
    pub fn start_of(&self, day: Day) -> Result<Moment, TimeError> {
        Moment::at(day, self.default_start_time)
    }

    /// R-033 : une date de fin saisie sans heure reçoit l'heure de fin par
    /// défaut.
    ///
    /// # Errors
    ///
    /// [`TimeError`] si l'instant sort du domaine (impossible, voir
    /// [`ProjectOptions::start_of`]).
    pub fn finish_of(&self, day: Day) -> Result<Moment, TimeError> {
        Moment::at(day, self.default_finish_time)
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{ProgressOption, ProgressOptions, ProjectOptions};

    #[test]
    fn cpm_options_default_when_absent() {
        // Un JSON sans les options CPM (projet écrit avant J2) se lit avec
        // leurs valeurs par défaut : `FORMAT_VERSION` reste 1.
        let mut v = serde_json::to_value(ProjectOptions::default()).unwrap();
        let o = v.as_object_mut().unwrap();
        assert!(o.remove("critical_slack_limit_days").is_some());
        assert!(o.remove("multiple_critical_paths").is_some());
        let read: ProjectOptions = serde_json::from_value(v).unwrap();
        assert_eq!(read, ProjectOptions::default());
        let custom = ProjectOptions {
            critical_slack_limit_days: 2,
            multiple_critical_paths: true,
            ..ProjectOptions::default()
        };
        let json = serde_json::to_string(&custom).unwrap();
        assert_eq!(serde_json::from_str::<ProjectOptions>(&json).unwrap(), custom);
    }

    #[test]
    fn progress_options_default_omitted_and_read() {
        // Contrat J6 : options d'avancement absentes du JSON à leurs
        // défauts (référence V1 inchangée), relues avec leurs défauts.
        let v = serde_json::to_value(ProjectOptions::default()).unwrap();
        assert!(v.get("progress").is_none());
        let read: ProjectOptions = serde_json::from_value(v).unwrap();
        assert_eq!(read.progress, ProgressOptions::default());
        assert!(read.progress.split_in_progress && read.progress.task_updates_resource);
        assert_eq!(read.progress.unsupported(), Vec::<ProgressOption>::new());
        let custom = ProjectOptions {
            progress: ProgressOptions {
                split_in_progress: false,
                task_updates_resource: false,
                move_completed_ends_forward: true,
                ..ProgressOptions::default()
            },
            ..ProjectOptions::default()
        };
        let json = serde_json::to_string(&custom).unwrap();
        assert!(
            json.contains(r#""progress":{"split_in_progress":false"#),
            "{json}"
        );
        assert_eq!(serde_json::from_str::<ProjectOptions>(&json).unwrap(), custom);
        assert_eq!(
            custom.progress.unsupported(),
            [
                ProgressOption::TaskUpdatesResource,
                ProgressOption::MoveCompletedEndsForward
            ]
        );
        assert_eq!(
            ProgressOption::MoveCompletedEndsForward.mspdi_name(),
            "MoveCompletedEndsForward"
        );
        // Champ partiel : les autres prennent leur défaut.
        let partial: ProgressOptions =
            serde_json::from_str(r#"{"move_remaining_starts_back":true}"#).unwrap();
        assert!(partial.split_in_progress && partial.move_remaining_starts_back);
    }
}
