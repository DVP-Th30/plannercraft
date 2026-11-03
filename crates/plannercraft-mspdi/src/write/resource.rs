//! Ressources à l'écriture (R-124).

use plannercraft_core::{
    Arena, ConversionFactors, Decimal, Money, Rate, Resource, ResourceKind, ResourceUid, Subject, Units,
};

use super::computed::Values;
use super::emit::Emitter;
use super::{OrderProfile, baseline, entity, max_len, plan, unscoped};
use crate::codes;
use crate::resource::rate_factor;
use crate::scalar::{format_bool, format_decimal, format_iso_duration};
use crate::schema::Parent;

/// Décimales au plus d'un taux horaire écrit.
const RATE_SCALE: u8 = 6;

/// Unités 1/10 000 → décimal (10 000 → « 1 », 5 000 → « 0.5 »).
pub(crate) fn units_text(u: Units) -> String {
    Decimal::new(u.get(), 4).map_or_else(|_| "1".to_owned(), format_decimal)
}

/// Centièmes → décimal en unité monétaire (1 250 → « 12.5 »).
fn money_text(m: Money) -> String {
    Decimal::new(m.get(), 2).map_or_else(|_| "0".to_owned(), format_decimal)
}

/// R-124 : taux horaire écrit = centièmes par unité d'affichage ÷ (100 ×
/// facteur), au plus 6 décimales (R-001) : l'erreur, au plus
/// 5·10⁻⁷ × 100 × 8 928 h < 0,45 centime pour un an, garantit la même
/// valeur à la relecture.
pub(crate) fn hourly_rate_text(rate: Rate, factors: &ConversionFactors) -> String {
    let (num, den) = rate_factor(rate.per, factors);
    let scale = 10_i128.pow(u32::from(RATE_SCALE));
    let n = i128::from(rate.amount.get())
        .checked_mul(den)
        .and_then(|v| v.checked_mul(scale));
    let d = num.checked_mul(100);
    let mantissa = n.zip(d).and_then(|(n, d)| plannercraft_core::round_div(n, d));
    mantissa
        .and_then(|m| Decimal::new(m, RATE_SCALE).ok())
        .map_or_else(|| "0".to_owned(), format_decimal)
}

/// Écrit les ressources du projet, dans l'ordre de l'arène.
pub(crate) fn write_all(
    em: &mut Emitter,
    p: &plannercraft_core::Project,
    values: Option<&Values>,
    profile: OrderProfile,
) {
    for (position, r) in p.resources.iter().enumerate() {
        write(
            em,
            r,
            (&p.resources, position),
            &p.options.factors,
            values,
            profile,
        );
    }
}

/// Écrit une ressource ; `position` : rang dans `resources` (`ID` :
/// [`plannercraft_core::resource_id`]) ;
/// `values` : travail et surutilisation à écrire (R-198, R-122).
pub(crate) fn write(
    em: &mut Emitter,
    r: &Resource,
    (resources, position): (&Arena<ResourceUid, Resource>, usize),
    factors: &ConversionFactors,
    values: Option<&Values>,
    profile: OrderProfile,
) {
    let computed = values.and_then(|v| v.resources.get(&r.uid()));
    em.subject = Some(Subject::Resource(r.uid()));
    let numbers: Vec<u8> = r.baselines.keys().map(|n| n.get()).collect();
    let planned = plan(em, "Resource", Parent::Resource, &r.extras, &numbers, profile);
    let top = unscoped(&planned);
    // ID 0 pour la ressource UID 0 cachée (O34), puis 1, 2… : les ID de
    // MS Project sont gardés.
    let id = plannercraft_core::resource_id(resources, position);
    let text = |em: &mut Emitter, name: &'static str, value: &str| {
        em.text(name, value, max_len(Parent::Resource, name));
    };
    entity(
        em,
        "Resource",
        Parent::Resource,
        profile,
        &top,
        |em, name| match name {
            "UID" => em.leaf(name, &r.uid().get().to_string()),
            "GUID" => em.opt_leaf(name, r.guid.map(|g| g.to_string())),
            "ID" => em.leaf(name, &id.to_string()),
            "Name" => text(em, name, &r.name),
            // pj15 n'énumère que 0 (matérielle) et 1 (travail) : une ressource
            // coût s'écrit `Type 1` et `IsCostResource 1`.
            "Type" => em.leaf(
                name,
                if matches!(r.kind, ResourceKind::Material { .. }) {
                    "0"
                } else {
                    "1"
                },
            ),
            "Initials" => text(em, name, &r.initials),
            "MaterialLabel" => match &r.kind {
                ResourceKind::Material { label } => text(em, name, label),
                _ => text(em, name, ""),
            },
            "Group" => text(em, name, &r.group),
            "MaxUnits" => em.leaf(name, &units_text(r.max_units)),
            "StandardRate" => em.leaf(name, &hourly_rate_text(r.standard_rate, factors)),
            "StandardRateFormat" => em.leaf(name, &codes::rate_format_code(r.standard_rate.per).to_string()),
            "OvertimeRate" => em.leaf(name, &hourly_rate_text(r.overtime_rate, factors)),
            "OvertimeRateFormat" => em.leaf(name, &codes::rate_format_code(r.overtime_rate.per).to_string()),
            "CostPerUse" => em.leaf(name, &money_text(r.cost_per_use)),
            "CalendarUID" => em.leaf(
                name,
                &r.calendar
                    .map_or_else(|| "-1".to_owned(), |c| c.get().to_string()),
            ),
            "Notes" => text(em, name, &r.notes),
            "IsGeneric" => em.leaf(name, format_bool(r.is_generic)),
            "IsInactive" => em.leaf(name, format_bool(r.is_inactive)),
            "IsCostResource" => em.leaf(name, format_bool(r.kind == ResourceKind::Cost)),
            // R-122, R-198 : valeurs calculées.
            "OverAllocated" => em.opt_leaf(
                name,
                computed
                    .and_then(|v| v.overallocated)
                    .map(|b| format_bool(b).to_owned()),
            ),
            "Work" => em.opt_leaf(
                name,
                computed
                    .and_then(|v| v.work)
                    .map(|w| format_iso_duration(w.minutes().get())),
            ),
            // R-226 : écart de travail (minutes × 1000).
            "WorkVariance" => em.opt_leaf(
                name,
                computed
                    .and_then(|v| v.work_variance)
                    .map(super::computed::variance_text),
            ),
            "Baseline" => baseline::resource(em, &r.baselines, &planned, profile),
            _ => {}
        },
    );
    em.subject = None;
}
