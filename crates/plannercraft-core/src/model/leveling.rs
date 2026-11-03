//! Retard de nivellement enregistré (ADR-0013 D4, contrat J5 lot L0).
//!
//! Le nivellement reste hors V1 : PlannerCraft ne calcule jamais de retard, il lit
//! celui d'un fichier (`LevelingDelay` et `LevelingDelayFormat` MSPDI, R-088
//! amendée en L3) et l'applique (R-178, L4b). Sur une tâche, il décale le
//! début et la fin, pas le début au plus tôt (O27) ; sur une affectation,
//! seulement l'affectation (O26).

use serde::{Deserialize, Serialize};

use crate::duration::{DurationUnit, Minutes};

/// Retard de nivellement d'une tâche ou d'une affectation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LevelingDelay {
    /// Durée du retard, en minutes (>= 0) : ouvrées, ou écoulées si
    /// `elapsed`.
    pub minutes: Minutes,
    /// Unité d'affichage (format MSPDI relu).
    pub unit: DurationUnit,
    /// Retard en temps écoulé (formats `e*` de MSPDI, O27).
    pub elapsed: bool,
}

impl LevelingDelay {
    /// Retard de `minutes` (>= 0) ; `None` pour une valeur négative.
    #[must_use]
    pub const fn new(minutes: Minutes, unit: DurationUnit, elapsed: bool) -> Option<Self> {
        if minutes.is_negative() {
            return None;
        }
        Some(Self {
            minutes,
            unit,
            elapsed,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::LevelingDelay;
    use crate::duration::{DurationUnit, Minutes};

    #[test]
    fn leveling_delay_json_and_sign() {
        let d = LevelingDelay::new(Minutes::new(4_320), DurationUnit::Day, true).unwrap();
        let json = serde_json::to_string(&d).unwrap();
        assert_eq!(json, r#"{"minutes":4320,"unit":"day","elapsed":true}"#);
        assert_eq!(serde_json::from_str::<LevelingDelay>(&json).unwrap(), d);
        assert!(LevelingDelay::new(Minutes::new(-1), DurationUnit::Day, false).is_none());
        assert!(
            serde_json::from_str::<LevelingDelay>(r#"{"minutes":0,"unit":"day","elapsed":false,"x":1}"#)
                .is_err()
        );
    }
}
