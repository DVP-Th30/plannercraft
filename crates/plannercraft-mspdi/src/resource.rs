//! Import des ressources (R-088, R-124) : type, capacité, taux, coût par
//! utilisation, calendrier, drapeaux, lignes de base ; travail et
//! surutilisation enregistrés dans `RecordedSchedule` (R-198) ; le reste en
//! `Extras` (R-087).

use std::collections::{BTreeMap, BTreeSet};

use plannercraft_core::{
    Arena, CalendarUid, ConversionFactors, Decimal, Extras, Money, Rate, RateUnit, Resource, ResourceKind,
    ResourceUid, Subject, Units, round_div,
};

use crate::baseline;
use crate::codes::{self, ResourceTypeCode};
use crate::diag::{ImportCode, Sink};
use crate::extras;
use crate::fields::Fields;
use crate::project::{code, guid};
use crate::recorded::RecordedResource;
use crate::scalar::parse_decimal;
use crate::schema::Parent;
use crate::xml::Element;

/// Chemin d'une ressource.
const RESOURCE: &str = "Resources/Resource";

/// Importe les ressources dans l'ordre du document ; valeurs enregistrées
/// dans `recorded` (R-198).
pub(crate) fn import(
    elements: &[Element<'_>],
    factors: &ConversionFactors,
    recorded: &mut BTreeMap<ResourceUid, RecordedResource>,
    sink: &mut Sink<'_>,
) -> Arena<ResourceUid, Resource> {
    let mut out = Arena::new();
    let mut seen: BTreeSet<ResourceUid> = BTreeSet::new();
    for element in elements {
        let mut f = Fields::new(&element.children, RESOURCE, None, element.at);
        let Some(uid) = resource_uid(&f, sink) else {
            continue;
        };
        if !seen.insert(uid) {
            f.report(
                ImportCode::Duplicate,
                "UID",
                element.at,
                Some(&uid.get().to_string()),
                sink,
            );
            continue;
        }
        f.set_subject(Some(Subject::Resource(uid)));
        let mut r = resource(uid, &f, factors, sink);
        recorded.insert(
            uid,
            RecordedResource {
                work: baseline::work(&f, "Work", sink),
                overallocated: f.bool("OverAllocated", sink),
                work_variance: crate::task::variance_decimal(&f, "WorkVariance", sink),
            },
        );
        let baselines = baseline::resource(&element.children, f.subject(), sink);
        r.baselines = baselines.map;
        let mut kept = extras::collect(&element.children, Parent::Resource, None);
        kept.extend(baselines.extras);
        r.extras = Extras(kept);
        extras::canonical_order(&mut r.extras, Parent::Resource);
        // UID uniques (filtrés plus haut) : l'ajout ne peut pas échouer.
        let _ = out.push(r);
    }
    out
}

/// UID d'une ressource ; absent, négatif ou illisible : ressource ignorée.
fn resource_uid(f: &Fields<'_>, sink: &mut Sink<'_>) -> Option<ResourceUid> {
    let Some((v, at)) = f.int("UID", sink) else {
        if f.scalar("UID", sink).is_none() {
            f.missing("UID", sink);
        }
        return None;
    };
    let uid = u32::try_from(v).ok().map(ResourceUid::new);
    if uid.is_none() {
        f.invalid("UID", at, &v.to_string(), sink);
    }
    uid
}

/// Construit une ressource (sans lignes de base ni `Extras`).
fn resource(uid: ResourceUid, f: &Fields<'_>, factors: &ConversionFactors, sink: &mut Sink<'_>) -> Resource {
    let text = |name: &str, sink: &mut Sink<'_>| f.raw(name, sink).unwrap_or_default().to_owned();
    let mut r = Resource::new(uid, text("Name", sink));
    r.guid = guid(f, "GUID", sink);
    r.initials = text("Initials", sink);
    r.group = text("Group", sink);
    r.notes = text("Notes", sink);
    r.kind = kind(f, sink);
    r.max_units = units(f, "MaxUnits", sink).unwrap_or(Units::FULL);
    r.calendar = calendar(f, sink);
    r.standard_rate = rate(f, "StandardRate", "StandardRateFormat", factors, sink);
    r.overtime_rate = rate(f, "OvertimeRate", "OvertimeRateFormat", factors, sink);
    r.cost_per_use = money(f, "CostPerUse", sink).unwrap_or(Money::ZERO);
    r.is_generic = f.bool("IsGeneric", sink).unwrap_or(false);
    r.is_inactive = f.bool("IsInactive", sink).unwrap_or(false);
    r
}

/// R-124 : `IsCostResource 1` ou `Type 2` → coût ; `Type 0` → matérielle
/// (étiquette `MaterialLabel`) ; `Type 1` ou absent → travail.
fn kind(f: &Fields<'_>, sink: &mut Sink<'_>) -> ResourceKind {
    let declared = code(f, "Type", codes::resource_type, sink);
    if f.bool("IsCostResource", sink) == Some(true) || declared == Some(ResourceTypeCode::Cost) {
        return ResourceKind::Cost;
    }
    match declared {
        Some(ResourceTypeCode::Material) => ResourceKind::Material {
            label: f.raw("MaterialLabel", sink).unwrap_or_default().to_owned(),
        },
        _ => ResourceKind::Work,
    }
}

/// Unités décimales (`MaxUnits`, `Units` : 1 = 100 %) → 1/10 000 (R-001).
pub(crate) fn units(f: &Fields<'_>, name: &str, sink: &mut Sink<'_>) -> Option<Units> {
    let (t, at) = f.scalar(name, sink)?;
    let u = parse_decimal(t).and_then(|p| {
        if p.rounded {
            f.report(ImportCode::Rounded, name, at, Some(t), sink);
        }
        Units::from_fraction(p.value).ok()
    });
    if u.is_none() {
        f.invalid(name, at, t, sink);
    }
    u
}

/// Montant décimal en unité monétaire (`CostPerUse`) → centièmes (R-001).
fn money(f: &Fields<'_>, name: &str, sink: &mut Sink<'_>) -> Option<Money> {
    let (t, at) = f.scalar(name, sink)?;
    let m = parse_decimal(t).and_then(|p| Money::from_decimal(p.value).ok());
    if m.is_none() {
        f.invalid(name, at, t, sink);
    }
    m
}

/// Calendrier propre (`CalendarUID`, négatif = aucun).
fn calendar(f: &Fields<'_>, sink: &mut Sink<'_>) -> Option<CalendarUid> {
    let (v, at) = f.int("CalendarUID", sink)?;
    if v < 0 {
        return None;
    }
    let uid = u32::try_from(v).ok().map(CalendarUid::new);
    if uid.is_none() {
        f.invalid("CalendarUID", at, &v.to_string(), sink);
    }
    uid
}

/// R-124 : facteur `(num, den)` qui convertit un taux horaire en taux par
/// unité d'affichage : minute 1/60, heure 1, jour `MinutesPerDay`/60,
/// semaine `MinutesPerWeek`/60, mois `DaysPerMonth` × `MinutesPerDay`/60,
/// an 12 mois (hypothèse), unité matérielle 1.
#[must_use]
pub(crate) fn rate_factor(unit: RateUnit, factors: &ConversionFactors) -> (i128, i128) {
    let per_day = i128::from(factors.minutes_per_day());
    let month = i128::from(factors.days_per_month()).saturating_mul(per_day);
    match unit {
        RateUnit::Minute => (1, 60),
        RateUnit::Hour | RateUnit::MaterialUnit => (1, 1),
        RateUnit::Day => (per_day, 60),
        RateUnit::Week => (i128::from(factors.minutes_per_week()), 60),
        RateUnit::Month => (month, 60),
        RateUnit::Year => (month.saturating_mul(12), 60),
    }
}

/// R-124 : montant en centièmes par unité d'affichage =
/// `round_div(taux/h × 100 × num, den)`. Exemple : 4,15875/h, jour de
/// 480 min → 3 327.
#[must_use]
pub(crate) fn rate_amount(per_hour: Decimal, unit: RateUnit, factors: &ConversionFactors) -> Option<Money> {
    let (num, den) = rate_factor(unit, factors);
    let scale = 10_i128.checked_pow(u32::from(per_hour.scale()))?;
    let n = i128::from(per_hour.mantissa())
        .checked_mul(100)?
        .checked_mul(num)?;
    let d = scale.checked_mul(den)?;
    Money::new(round_div(n, d)?).ok()
}

/// Taux (`StandardRate` + `StandardRateFormat`…) : taux horaire du fichier
/// converti dans l'unité d'affichage ; format absent : heure ; illisible ou
/// hors bornes : 0 par heure, signalé.
fn rate(f: &Fields<'_>, name: &str, format: &str, factors: &ConversionFactors, sink: &mut Sink<'_>) -> Rate {
    let per = code(f, format, codes::rate_format, sink).unwrap_or(RateUnit::Hour);
    let Some((t, at)) = f.scalar(name, sink) else {
        return Rate {
            amount: Money::ZERO,
            per,
        };
    };
    let amount = parse_decimal(t).and_then(|p| rate_amount(p.value, per, factors));
    if amount.is_none() {
        f.invalid(name, at, t, sink);
    }
    Rate {
        amount: amount.unwrap_or(Money::ZERO),
        per,
    }
}
