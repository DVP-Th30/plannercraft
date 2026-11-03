//! Lien entre tâches (ADR-0008, § 5.4), porté par le successeur.
//!
//! `Project::validate` contrôle l'unicité de la paire (prédécesseur,
//! successeur) — hypothèse : MS Project n'autoriserait qu'un lien par paire —,
//! l'absence d'auto-lien et l'existence du prédécesseur. L'index d'adjacence
//! est construit au recalcul (J2).

use serde::{Deserialize, Serialize};

use crate::duration::Lag;
use crate::ids::TaskUid;

/// Type de lien.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkType {
    /// Fin à début (FD / FS).
    #[default]
    FinishToStart,
    /// Début à début (DD / SS).
    StartToStart,
    /// Fin à fin (FF).
    FinishToFinish,
    /// Début à fin (DF / SF).
    StartToFinish,
}

/// Lien vers un prédécesseur, rangé dans le successeur.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Link {
    /// Tâche prédécesseur.
    pub predecessor: TaskUid,
    /// Type de lien.
    pub kind: LinkType,
    /// Décalage (avance si négatif).
    pub lag: Lag,
}

impl Link {
    /// Lien fin à début sans décalage.
    #[must_use]
    pub const fn finish_to_start(predecessor: TaskUid) -> Self {
        Self {
            predecessor,
            kind: LinkType::FinishToStart,
            lag: Lag::ZERO,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{Link, LinkType};
    use crate::ids::TaskUid;

    #[test]
    fn link_json() {
        let l = Link::finish_to_start(TaskUid::new(3));
        let json = serde_json::to_string(&l).unwrap();
        assert_eq!(
            json,
            r#"{"predecessor":3,"kind":"finish_to_start","lag":{"basis":"working","value":{"kind":"time","minutes":0,"unit":"day"}}}"#
        );
        assert_eq!(serde_json::from_str::<Link>(&json).unwrap(), l);
        assert_eq!(
            serde_json::to_string(&LinkType::StartToFinish).unwrap(),
            r#""start_to_finish""#
        );
    }
}
