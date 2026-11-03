//! Retards de nivellement à l'écriture (R-198) : `LevelingDelay` en
//! dixièmes de minute (minutes × 10) et `LevelingDelayFormat` (codes S3),
//! écrits seulement s'il y a un retard (classe M?) ; MS Project lit un
//! élément absent comme un retard nul.

use plannercraft_core::{Duration, LevelingDelay};

use super::Planned;
use super::emit::Emitter;
use crate::codes;

/// Retard écrit : non nul et dans le domaine des durées (R-002), sinon
/// rien (la relecture le refuserait).
pub(crate) fn writable(delay: Option<LevelingDelay>) -> Option<LevelingDelay> {
    delay.filter(|d| d.minutes.get() > 0 && d.minutes.get() <= Duration::MAX_MINUTES)
}

/// Élément `name` (`LevelingDelay` ou `LevelingDelayFormat`) d'un retard.
pub(crate) fn write(em: &mut Emitter, name: &'static str, delay: Option<LevelingDelay>) {
    let Some(d) = writable(delay) else {
        return;
    };
    let value = if name == "LevelingDelay" {
        d.minutes.get().saturating_mul(10)
    } else {
        codes::leveling_delay_format_code(d.unit, d.elapsed)
    };
    em.leaf_int(name, value);
}

/// `Extras` `LevelingDelay`/`LevelingDelayFormat` hérités d'un projet
/// enregistré avant J5 (PlannerCraft les gardait verbatim) : réécrits tels quels
/// tant que le modèle n'a pas de retard écrit ; jetés sinon (le modèle
/// prime, pas d'élément en double).
pub(crate) fn drop_legacy(planned: &mut Vec<&Planned<'_>>, delay: Option<LevelingDelay>) {
    if writable(delay).is_some() {
        planned.retain(|p| !matches!(p.key.name.as_str(), "LevelingDelay" | "LevelingDelayFormat"));
    }
}
