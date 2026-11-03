//! Affectation d'une ressource à une tâche (ADR-0008, § 5.5).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::arena::HasUid;
use crate::duration::{Minutes, Work};
use crate::ids::{AssignmentUid, BaselineNumber, Guid, ResourceUid, TaskUid};
use crate::model::Extras;
use crate::model::LevelingDelay;
use crate::model::baseline::AssignmentBaseline;
use crate::quantity::Units;

/// Profil de répartition du travail. Stocké ; seul `Flat` est calculé en V1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkContour {
    /// Uniforme.
    #[default]
    Flat,
    /// Croissant.
    BackLoaded,
    /// Décroissant.
    FrontLoaded,
    /// Deux pics.
    DoublePeak,
    /// Pic au début.
    EarlyPeak,
    /// Pic à la fin.
    LatePeak,
    /// En cloche.
    Bell,
    /// En tortue.
    Turtle,
    /// Profil modifié à la main.
    Contoured,
}

/// Affectation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    uid: AssignmentUid,
    /// GUID conservé tel quel.
    pub guid: Option<Guid>,
    /// Tâche affectée.
    pub task: TaskUid,
    /// Ressource ; `None` pour une affectation sans ressource (hypothèse, à
    /// confirmer au corpus).
    pub resource: Option<ResourceUid>,
    /// Unités (1/10 000).
    pub units: Units,
    /// Travail.
    pub work: Work,
    /// Travail réel.
    pub actual_work: Work,
    /// Travail restant.
    pub remaining_work: Work,
    /// Retard d'affectation (>= 0 attendu).
    pub delay: Minutes,
    /// Retard de nivellement enregistré (lu, jamais calculé : ADR-0013 D4,
    /// R-178) ; absent du JSON quand il n'y en a pas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leveling_delay: Option<LevelingDelay>,
    /// Profil (stocké ; `Flat` seul calculé en V1).
    pub contour: WorkContour,
    /// Lignes de base 0 à 10.
    pub baselines: BTreeMap<BaselineNumber, AssignmentBaseline>,
    /// Données hors modèle.
    pub extras: Extras,
}

impl HasUid for Assignment {
    type Uid = AssignmentUid;
    fn uid(&self) -> AssignmentUid {
        self.uid
    }
}

impl Assignment {
    /// Affectation à 100 %, sans travail saisi.
    #[must_use]
    pub fn new(uid: AssignmentUid, task: TaskUid, resource: Option<ResourceUid>) -> Self {
        Self {
            uid,
            guid: None,
            task,
            resource,
            units: Units::FULL,
            work: Work::ZERO,
            actual_work: Work::ZERO,
            remaining_work: Work::ZERO,
            delay: Minutes::ZERO,
            leveling_delay: None,
            contour: WorkContour::Flat,
            baselines: BTreeMap::new(),
            extras: Extras::new(),
        }
    }

    /// UID.
    #[must_use]
    pub const fn uid(&self) -> AssignmentUid {
        self.uid
    }
}
