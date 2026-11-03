//! Tâche (ADR-0008, § 5.3).
//!
//! Les valeurs calculées (début et fin au plus tôt et au plus tard, marges,
//! critique, récapitulative) sont hors du modèle : le moteur les produit dans
//! un `Schedule` (J2). Seules les dates des tâches en mode manuel sont des
//! entrées.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::arena::HasUid;
use crate::duration::{Duration, Minutes, TimeBasis, Work};
use crate::error::ModelError;
use crate::ids::{BaselineNumber, CalendarUid, Guid, TaskUid};
use crate::model::Extras;
use crate::model::baseline::TaskBaseline;
use crate::model::custom::{CustomFieldId, CustomValue};
use crate::model::leveling::LevelingDelay;
use crate::model::link::Link;
use crate::model::options::ProjectOptions;
use crate::quantity::Percent;
use crate::time::Moment;

/// Type de tâche : la grandeur tenue fixe quand on modifie les autres (J5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    /// Unités fixes.
    #[default]
    FixedUnits,
    /// Durée fixe.
    FixedDuration,
    /// Travail fixe.
    FixedWork,
}

/// Type de contrainte, sans sa date (S27, S34). Les codes MSPDI
/// (`ConstraintType` 0 à 7) vivent dans `plannercraft-mspdi`, jamais dans le cœur
/// (ADR-0008).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintKind {
    /// Le plus tôt possible.
    AsSoonAsPossible,
    /// Le plus tard possible.
    AsLateAsPossible,
    /// Doit commencer le.
    MustStartOn,
    /// Doit finir le.
    MustFinishOn,
    /// Ne pas commencer avant le.
    StartNoEarlierThan,
    /// Ne pas commencer après le.
    StartNoLaterThan,
    /// Ne pas finir avant le.
    FinishNoEarlierThan,
    /// Ne pas finir après le.
    FinishNoLaterThan,
}

impl ConstraintKind {
    /// Vrai pour les six types datés (S29 : « All the constraints except As
    /// Soon As Possible and As Late As Possible require a date »).
    #[must_use]
    pub const fn is_dated(self) -> bool {
        !matches!(self, Self::AsSoonAsPossible | Self::AsLateAsPossible)
    }

    /// Vrai pour les types dont la date est un début (MSO, SNET, SNLT),
    /// normalisée côté début (R-090).
    #[must_use]
    pub const fn is_start_side(self) -> bool {
        matches!(
            self,
            Self::MustStartOn | Self::StartNoEarlierThan | Self::StartNoLaterThan
        )
    }
}

/// Contrainte de date ; une contrainte datée porte sa date.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Constraint {
    /// Le plus tôt possible.
    #[default]
    AsSoonAsPossible,
    /// Le plus tard possible.
    AsLateAsPossible,
    /// Doit commencer le.
    MustStartOn {
        /// Date imposée.
        date: Moment,
    },
    /// Doit finir le.
    MustFinishOn {
        /// Date imposée.
        date: Moment,
    },
    /// Ne pas commencer avant le.
    StartNoEarlierThan {
        /// Date limite.
        date: Moment,
    },
    /// Ne pas commencer après le.
    StartNoLaterThan {
        /// Date limite.
        date: Moment,
    },
    /// Ne pas finir avant le.
    FinishNoEarlierThan {
        /// Date limite.
        date: Moment,
    },
    /// Ne pas finir après le.
    FinishNoLaterThan {
        /// Date limite.
        date: Moment,
    },
}

impl Constraint {
    /// Type de la contrainte.
    #[must_use]
    pub const fn kind(&self) -> ConstraintKind {
        match self {
            Self::AsSoonAsPossible => ConstraintKind::AsSoonAsPossible,
            Self::AsLateAsPossible => ConstraintKind::AsLateAsPossible,
            Self::MustStartOn { .. } => ConstraintKind::MustStartOn,
            Self::MustFinishOn { .. } => ConstraintKind::MustFinishOn,
            Self::StartNoEarlierThan { .. } => ConstraintKind::StartNoEarlierThan,
            Self::StartNoLaterThan { .. } => ConstraintKind::StartNoLaterThan,
            Self::FinishNoEarlierThan { .. } => ConstraintKind::FinishNoEarlierThan,
            Self::FinishNoLaterThan { .. } => ConstraintKind::FinishNoLaterThan,
        }
    }

    /// Date de la contrainte ; `None` pour ASAP et ALAP.
    #[must_use]
    pub const fn date(&self) -> Option<Moment> {
        match self {
            Self::AsSoonAsPossible | Self::AsLateAsPossible => None,
            Self::MustStartOn { date }
            | Self::MustFinishOn { date }
            | Self::StartNoEarlierThan { date }
            | Self::StartNoLaterThan { date }
            | Self::FinishNoEarlierThan { date }
            | Self::FinishNoLaterThan { date } => Some(*date),
        }
    }

    /// Contrainte à partir de son type et de sa date. `None` si la date
    /// manque pour un type daté, ou est donnée pour ASAP ou ALAP.
    #[must_use]
    pub const fn from_parts(kind: ConstraintKind, date: Option<Moment>) -> Option<Self> {
        Some(match (kind, date) {
            (ConstraintKind::AsSoonAsPossible, None) => Self::AsSoonAsPossible,
            (ConstraintKind::AsLateAsPossible, None) => Self::AsLateAsPossible,
            (ConstraintKind::MustStartOn, Some(date)) => Self::MustStartOn { date },
            (ConstraintKind::MustFinishOn, Some(date)) => Self::MustFinishOn { date },
            (ConstraintKind::StartNoEarlierThan, Some(date)) => Self::StartNoEarlierThan { date },
            (ConstraintKind::StartNoLaterThan, Some(date)) => Self::StartNoLaterThan { date },
            (ConstraintKind::FinishNoEarlierThan, Some(date)) => Self::FinishNoEarlierThan { date },
            (ConstraintKind::FinishNoLaterThan, Some(date)) => Self::FinishNoLaterThan { date },
            _ => return None,
        })
    }

    /// R-108 : contrainte par défaut d'une nouvelle tâche selon le sens de
    /// planification : « le plus tôt possible » depuis le début, « le plus
    /// tard possible » à rebours (S28, S29, S33). Pour l'ajout d'une tâche
    /// (J4) et l'import CSV ; [`Task::new`] reste ASAP.
    #[must_use]
    pub const fn default_for(anchor: &crate::model::ScheduleAnchor) -> Self {
        match anchor {
            crate::model::ScheduleAnchor::FromStart(_) => Self::AsSoonAsPossible,
            crate::model::ScheduleAnchor::FromFinish(_) => Self::AsLateAsPossible,
        }
    }
}

/// Mode de planification. Une tâche manuelle est stockée, pas recalculée
/// (ADR-0005) : ses dates sont des entrées.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SchedulingMode {
    /// Planification automatique.
    #[default]
    Auto,
    /// Planification manuelle, avec les valeurs saisies.
    Manual {
        /// Début saisi.
        start: Option<Moment>,
        /// Fin saisie.
        finish: Option<Moment>,
        /// Durée saisie.
        duration: Option<Duration>,
    },
}

/// Niveau hiérarchique (>= 1). La hiérarchie et le caractère récapitulatif se
/// déduisent de l'ordre des tâches et de leur niveau (J2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub struct OutlineLevel(u16);

impl OutlineLevel {
    /// Premier niveau.
    pub const TOP: Self = Self(1);

    /// Niveau hiérarchique.
    ///
    /// # Errors
    ///
    /// [`ModelError::InvalidOutlineLevel`] pour 0.
    pub const fn new(level: u16) -> Result<Self, ModelError> {
        if level == 0 {
            return Err(ModelError::InvalidOutlineLevel { level });
        }
        Ok(Self(level))
    }

    /// Valeur.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl Default for OutlineLevel {
    fn default() -> Self {
        Self::TOP
    }
}

impl TryFrom<u16> for OutlineLevel {
    type Error = ModelError;
    fn try_from(v: u16) -> Result<Self, Self::Error> {
        Self::new(v)
    }
}

impl From<OutlineLevel> for u16 {
    fn from(l: OutlineLevel) -> Self {
        l.0
    }
}

/// Priorité, de 0 à 1 000 ; 500 par défaut.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub struct Priority(u16);

impl Priority {
    /// Priorité par défaut.
    pub const DEFAULT: Self = Self(500);
    /// Plus haute priorité.
    pub const MAX: u16 = 1_000;

    /// Priorité.
    ///
    /// # Errors
    ///
    /// [`ModelError::InvalidPriority`] au-delà de 1 000.
    pub const fn new(priority: u16) -> Result<Self, ModelError> {
        if priority > Self::MAX {
            return Err(ModelError::InvalidPriority { priority });
        }
        Ok(Self(priority))
    }

    /// Valeur.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl Default for Priority {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl TryFrom<u16> for Priority {
    type Error = ModelError;
    fn try_from(v: u16) -> Result<Self, Self::Error> {
        Self::new(v)
    }
}

impl From<Priority> for u16 {
    fn from(p: Priority) -> Self {
        p.0
    }
}

/// Avancement saisi (ADR-0005) : trois pourcentages distincts, dates réelles,
/// durées et travail réels et restants. La durée réelle et la durée restante
/// sont >= 0 (contrôlé par `Project::validate`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Progress {
    /// % achevé (durée).
    pub percent_complete: Percent,
    /// % travail achevé.
    pub percent_work_complete: Percent,
    /// % physique achevé.
    pub physical_percent_complete: Percent,
    /// Début réel.
    pub actual_start: Option<Moment>,
    /// Fin réelle.
    pub actual_finish: Option<Moment>,
    /// Durée réelle (>= 0).
    pub actual_duration: Minutes,
    /// Durée restante (>= 0).
    pub remaining_duration: Minutes,
    /// Travail réel.
    pub actual_work: Work,
    /// Travail restant.
    pub remaining_work: Work,
    /// Reprise saisie d'une tâche en cours (`Resume` relu ou posé par
    /// `progress.reschedule`, R-204, R-214, R-217) ; `Stop` n'est jamais
    /// stocké (ADR-0014 D2). Absente du JSON quand il n'y en a pas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<Moment>,
}

/// Tâche.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "drapeaux indépendants repris de MSPDI (IsNull, EffortDriven, Milestone…), pas un état"
)]
pub struct Task {
    uid: TaskUid,
    /// GUID conservé tel quel.
    pub guid: Option<Guid>,
    /// Nom.
    pub name: String,
    /// Ligne vide MSPDI (`IsNull`) : garde les ID justes ; ignorée par le calcul.
    pub is_null: bool,
    /// Niveau hiérarchique (>= 1).
    pub outline_level: OutlineLevel,
    /// Code WBS, stocké tel quel.
    pub wbs: Option<String>,
    /// Durée.
    pub duration: Duration,
    /// Travail.
    pub work: Work,
    /// Type de tâche.
    pub task_type: TaskType,
    /// Pilotée par l'effort.
    pub effort_driven: bool,
    /// Jalon (drapeau, indépendant d'une durée nulle).
    pub milestone: bool,
    /// Contrainte de date.
    pub constraint: Constraint,
    /// Échéance.
    pub deadline: Option<Moment>,
    /// Calendrier propre.
    pub calendar: Option<CalendarUid>,
    /// Ignorer le calendrier des ressources.
    pub ignore_resource_calendar: bool,
    /// Priorité.
    pub priority: Priority,
    /// Retard de nivellement enregistré (lu, jamais calculé : ADR-0013 D4,
    /// R-178) ; absent du JSON quand il n'y en a pas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leveling_delay: Option<LevelingDelay>,
    /// Avancement saisi.
    pub progress: Progress,
    /// Liens vers les prédécesseurs, portés par le successeur, ordre MSPDI
    /// conservé.
    pub predecessors: Vec<Link>,
    /// Notes.
    pub notes: String,
    /// Mode de planification (stocké, non recalculé si manuel).
    pub mode: SchedulingMode,
    /// Active (stocké, non calculé en V1).
    pub active: bool,
    /// Récurrente (stocké, non calculé en V1).
    pub recurring: bool,
    /// Date de création.
    pub created: Option<Moment>,
    /// Lignes de base 0 à 10 (seule la n° 0 sera calculée en V1).
    pub baselines: BTreeMap<BaselineNumber, TaskBaseline>,
    /// Champs personnalisés (R-263) : absent = vide (un texte vide et
    /// `Flag(false)` ne sont pas stockés) ; absents du JSON quand il n'y en
    /// a pas (sessions et journaux antérieurs relus tels quels).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub custom: BTreeMap<CustomFieldId, CustomValue>,
    /// Données hors modèle.
    pub extras: Extras,
}

impl HasUid for Task {
    type Uid = TaskUid;
    fn uid(&self) -> TaskUid {
        self.uid
    }
}

impl Task {
    /// Nouvelle tâche automatique (R-032) : durée « 1j? » (une journée de
    /// `minutes_per_day`, affichée dans l'unité par défaut, estimée si
    /// l'option le dit, S14), type et pilotage par l'effort pris dans les
    /// options, contrainte « le plus tôt possible », priorité 500, active.
    #[must_use]
    pub fn new(uid: TaskUid, name: String, options: &ProjectOptions) -> Self {
        let one_day = Minutes::new(i64::from(options.factors.minutes_per_day()));
        // Toujours valide : 1 à 1 440 minutes.
        let duration = Duration::new(
            one_day,
            options.default_duration_unit,
            TimeBasis::Working,
            options.new_tasks_estimated,
        )
        .unwrap_or_else(|_| Duration::zero(options.default_duration_unit));
        Self {
            uid,
            guid: None,
            name,
            is_null: false,
            outline_level: OutlineLevel::TOP,
            wbs: None,
            duration,
            work: Work::ZERO,
            task_type: options.default_task_type,
            effort_driven: options.new_tasks_effort_driven,
            milestone: false,
            constraint: Constraint::AsSoonAsPossible,
            deadline: None,
            calendar: None,
            ignore_resource_calendar: false,
            priority: Priority::DEFAULT,
            leveling_delay: None,
            progress: Progress::default(),
            predecessors: Vec::new(),
            notes: String::new(),
            mode: SchedulingMode::Auto,
            active: true,
            recurring: false,
            created: None,
            baselines: BTreeMap::new(),
            custom: BTreeMap::new(),
            extras: Extras::new(),
        }
    }

    /// UID.
    #[must_use]
    pub const fn uid(&self) -> TaskUid {
        self.uid
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{Constraint, ConstraintKind, OutlineLevel, Priority, SchedulingMode, Task, TaskType};
    use crate::duration::{DurationUnit, Minutes};
    use crate::error::ModelError;
    use crate::ids::TaskUid;
    use crate::model::options::ProjectOptions;

    #[test]
    fn bounded_newtypes() {
        assert_eq!(
            OutlineLevel::new(0),
            Err(ModelError::InvalidOutlineLevel { level: 0 })
        );
        assert_eq!(OutlineLevel::new(3).unwrap().get(), 3);
        assert_eq!(OutlineLevel::default(), OutlineLevel::TOP);
        assert_eq!(
            Priority::new(1_001),
            Err(ModelError::InvalidPriority { priority: 1_001 })
        );
        assert_eq!(Priority::new(1_000).unwrap().get(), 1_000);
        assert_eq!(Priority::default().get(), 500);
        assert!(serde_json::from_str::<OutlineLevel>("0").is_err());
        assert!(serde_json::from_str::<Priority>("1001").is_err());
        assert_eq!(serde_json::to_string(&Priority::DEFAULT).unwrap(), "500");
    }

    #[test]
    fn task_new_follows_options() {
        let options = ProjectOptions {
            default_duration_unit: DurationUnit::Hour,
            default_task_type: TaskType::FixedWork,
            new_tasks_effort_driven: true,
            new_tasks_estimated: false,
            ..ProjectOptions::default()
        };
        let t = Task::new(TaskUid::new(7), "T".to_owned(), &options);
        assert_eq!(t.uid(), TaskUid::new(7));
        assert_eq!(t.duration.minutes(), Minutes::new(480));
        assert_eq!(t.duration.unit(), DurationUnit::Hour);
        assert!(!t.duration.is_estimated());
        assert_eq!(t.task_type, TaskType::FixedWork);
        assert!(t.effort_driven);
    }

    #[test]
    fn constraint_kind_date_and_parts() {
        let d: crate::time::Moment = "2026-10-21T13:00".parse().unwrap();
        let all = [
            ConstraintKind::AsSoonAsPossible,
            ConstraintKind::AsLateAsPossible,
            ConstraintKind::MustStartOn,
            ConstraintKind::MustFinishOn,
            ConstraintKind::StartNoEarlierThan,
            ConstraintKind::StartNoLaterThan,
            ConstraintKind::FinishNoEarlierThan,
            ConstraintKind::FinishNoLaterThan,
        ];
        for kind in all {
            let date = kind.is_dated().then_some(d);
            let c = Constraint::from_parts(kind, date).unwrap();
            assert_eq!(c.kind(), kind);
            assert_eq!(c.date(), date);
            // Date manquante ou en trop : refus.
            let wrong = if kind.is_dated() { None } else { Some(d) };
            assert_eq!(Constraint::from_parts(kind, wrong), None);
        }
        let start_side: Vec<_> = all.into_iter().filter(|k| k.is_start_side()).collect();
        assert_eq!(
            start_side,
            [
                ConstraintKind::MustStartOn,
                ConstraintKind::StartNoEarlierThan,
                ConstraintKind::StartNoLaterThan
            ]
        );
        assert_eq!(
            serde_json::to_string(&ConstraintKind::FinishNoEarlierThan).unwrap(),
            r#""finish_no_earlier_than""#
        );
    }

    #[test]
    fn constraint_and_mode_json() {
        let c = Constraint::MustStartOn {
            date: "2026-10-19T08:00".parse().unwrap(),
        };
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(json, r#"{"type":"must_start_on","date":"2026-10-19T08:00"}"#);
        assert_eq!(serde_json::from_str::<Constraint>(&json).unwrap(), c);
        assert_eq!(
            serde_json::to_string(&Constraint::AsSoonAsPossible).unwrap(),
            r#"{"type":"as_soon_as_possible"}"#
        );
        assert!(serde_json::from_str::<Constraint>(r#"{"type":"must_start_on"}"#).is_err());
        assert_eq!(serde_json::to_string(&SchedulingMode::Auto).unwrap(), r#""auto""#);
        let m = SchedulingMode::Manual {
            start: None,
            finish: None,
            duration: None,
        };
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(json, r#"{"manual":{"start":null,"finish":null,"duration":null}}"#);
        assert_eq!(serde_json::from_str::<SchedulingMode>(&json).unwrap(), m);
        // Champ inconnu dans la variante `manual` : refusé.
        assert!(
            serde_json::from_str::<SchedulingMode>(
                r#"{"manual":{"start":null,"finish":null,"duration":null,"bogus":3}}"#
            )
            .is_err()
        );
    }
}
