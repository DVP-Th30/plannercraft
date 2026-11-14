//! Jours chômés proposés à l'import (lot P1b) : jours fériés français
//! (R-274, liste de `plannercraft-core`) sur les années couvertes par le
//! planning, et périodes déduites des écarts ([`crate::deduce`]), posés
//! comme exceptions du calendrier du projet (R-275).

use chrono::Datelike as _;
use plannercraft_automation::text::Language;
use plannercraft_core::Day;
use plannercraft_core::calendar::holidays::french_public_holidays;

use crate::convert::NonWorkingDays;
use crate::deduce::ProposedPeriod;
use crate::header::HeaderLanguage;
use crate::plan::ImportedPlan;

/// Premier et dernier jour lus (débuts et fins), s'il y en a.
#[must_use]
pub fn read_span(plan: &ImportedPlan) -> Option<(Day, Day)> {
    let days = plan
        .tasks
        .iter()
        .flat_map(|t| [t.start, t.finish])
        .flatten()
        .map(plannercraft_core::Moment::day);
    let (mut lo, mut hi): (Option<Day>, Option<Day>) = (None, None);
    for d in days {
        lo = Some(lo.map_or(d, |l| l.min(d)));
        hi = Some(hi.map_or(d, |h| h.max(d)));
    }
    Some((lo?, hi?))
}

/// Les fériés français sont proposés cochés quand l'en-tête du tableau est
/// en français (langue des intitulés et des dates du PDF).
#[must_use]
pub fn suggests_french_holidays(plan: &ImportedPlan) -> bool {
    plan.language == Some(HeaderLanguage::Fr)
}

/// Les onze fêtes légales de chaque année couverte par les dates lues
/// (années entières : une tâche qui déborde d'un jour ou deux reste
/// juste), nommées dans `lang`.
#[must_use]
pub fn french_holidays(plan: &ImportedPlan, lang: Language) -> Vec<NonWorkingDays> {
    let Some((first, last)) = read_span(plan) else {
        return Vec::new();
    };
    let (y0, y1) = (first.to_naive().year(), last.to_naive().year());
    // Une année de plus : un écart de fin peut tomber après la dernière date lue.
    (y0..=y1.saturating_add(1))
        .flat_map(french_public_holidays)
        .map(|(day, h)| NonWorkingDays {
            from: day,
            to: day,
            name: match lang {
                Language::Fr => h.name_fr(),
                Language::En => h.name_en(),
            }
            .to_owned(),
        })
        .collect()
}

/// Placements par défaut de toutes les périodes proposées.
#[must_use]
pub fn default_placements(periods: &[ProposedPeriod]) -> Vec<(Day, Day)> {
    periods
        .iter()
        .filter_map(|p| p.placement(p.default_option))
        .collect()
}

/// Périodes déduites cochées (placements choisis), nommées `name`.
#[must_use]
pub fn periods(chosen: &[(Day, Day)], name: &str) -> Vec<NonWorkingDays> {
    chosen
        .iter()
        .map(|&(from, to)| NonWorkingDays {
            from,
            to,
            name: name.to_owned(),
        })
        .collect()
}
