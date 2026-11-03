//! Lignes de base (ADR-0008, § 5.5) : 11 lignes (0 à 10) stockées pour
//! l'aller-retour, champs optionnels ; seule la n° 0 sera calculée en V1
//! (écart au brief, ADR-0008).

use serde::{Deserialize, Serialize};

use crate::duration::{Duration, Work};
use crate::quantity::Money;
use crate::time::Moment;

/// Ligne de base d'une tâche.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskBaseline {
    /// Début.
    pub start: Option<Moment>,
    /// Fin.
    pub finish: Option<Moment>,
    /// Durée.
    pub duration: Option<Duration>,
    /// Travail.
    pub work: Option<Work>,
    /// Coût.
    pub cost: Option<Money>,
    /// Coût fixe.
    pub fixed_cost: Option<Money>,
}

/// Ligne de base d'une ressource.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceBaseline {
    /// Travail.
    pub work: Option<Work>,
    /// Coût.
    pub cost: Option<Money>,
}

/// Ligne de base d'une affectation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssignmentBaseline {
    /// Début.
    pub start: Option<Moment>,
    /// Fin.
    pub finish: Option<Moment>,
    /// Travail.
    pub work: Option<Work>,
    /// Coût.
    pub cost: Option<Money>,
}
