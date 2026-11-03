//! Commandes `resource.*` (R-195, lot J5 L7) : ajout, modification,
//! suppression ; désignation des ressources dans un lot (UID ou étiquette
//! `$x` d'un `resource.add` antérieur, espace de noms distinct de celui des
//! tâches).

use std::collections::{BTreeMap, BTreeSet};

use plannercraft_core::calendar::{Calendar, CalendarKind};
use plannercraft_core::{
    Assignment, AssignmentUid, CalendarUid, HasUid, ModelError, Money, Rate, RateUnit, Resource,
    ResourceKind, ResourceUid, Subject, Units,
};

use super::assignment::{invalid, unbalance};
use super::exec::Ctx;
use super::refs::is_valid_label;
use super::task::MAX_NAME_CHARS;
use super::{
    Patch, RateJson, RateUnitJson, ResourceAdd, ResourceCalendarJson, ResourceDelete, ResourceKindJson,
    ResourceRef, ResourceUpdate, UnitsJson,
};
use crate::error::{CommandError, CommandErrorKind, codes};
use crate::outcome::SideEffect;

/// Erreur du modèle (jamais attendue : les contrôles la précèdent).
fn model(e: &ModelError) -> CommandError {
    CommandError::new(
        codes::PARAM_INVALID,
        CommandErrorKind::InvalidParam(e.to_string()),
    )
}

/// Texte de ressource (nom, initiales, groupe, étiquette d'unité) : au plus
/// [`MAX_NAME_CHARS`] caractères ; vide accepté.
fn check_text(field: &str, s: &str) -> Result<(), CommandError> {
    if s.chars().count() > MAX_NAME_CHARS {
        return Err(invalid(field, "texte de plus de 512 caractères"));
    }
    Ok(())
}

/// `resource.not_found` pour `uid`, sur `field`.
fn not_found(uid: ResourceUid, field: &str) -> CommandError {
    CommandError::new(codes::RESOURCE_NOT_FOUND, CommandErrorKind::UnknownResource(uid))
        .with_field(field)
        .with_subject(Subject::Resource(uid))
}

/// `calendar.not_found` pour `uid`, sur `field`.
fn calendar_not_found(uid: CalendarUid, field: &str) -> CommandError {
    CommandError::new(
        codes::CALENDAR_NOT_FOUND,
        CommandErrorKind::InvalidParam(format!("aucun calendrier d'UID {uid}")),
    )
    .with_field(field)
    .with_subject(Subject::Calendar(uid))
}

/// R-195 : UID désigné par `r`, qui doit exister dans `ctx.project`.
///
/// # Errors
///
/// `resource.not_found`, `batch.unknown_label` ou `param.invalid` (chaîne
/// sans `$`), sur `field`.
fn resolve_resource(
    r: &ResourceRef,
    labels: &BTreeMap<String, ResourceUid>,
    ctx: &Ctx<'_>,
    field: &str,
) -> Result<ResourceUid, CommandError> {
    let uid = match r {
        ResourceRef::Uid(u) => *u,
        ResourceRef::Label(l) => {
            if !is_valid_label(l) {
                return Err(invalid(field, "étiquette \"$x\" attendue"));
            }
            *labels.get(l).ok_or_else(|| {
                CommandError::new(
                    codes::BATCH_UNKNOWN_LABEL,
                    CommandErrorKind::UnknownLabel(l.clone()),
                )
                .with_field(field)
            })?
        }
    };
    if ctx.project.resources.contains(uid) {
        Ok(uid)
    } else {
        Err(not_found(uid, field))
    }
}

impl Ctx<'_> {
    /// R-195 : ressource désignée par `r` dans le clone du lot.
    pub(crate) fn resource(&self, r: &ResourceRef, field: &str) -> Result<ResourceUid, CommandError> {
        resolve_resource(r, self.resource_labels, self, field)
    }

    /// Unités JSON → modèle (voir [`UnitsJson::to_units`]), sur `field`.
    pub(crate) fn units(&self, u: &UnitsJson, field: &str) -> Result<Units, CommandError> {
        u.to_units(self.text).map_err(|e| e.with_field(field))
    }

    /// Quantité JSON → modèle (voir [`UnitsJson::to_quantity`]), sur
    /// `field`.
    pub(crate) fn quantity(&self, u: &UnitsJson, field: &str) -> Result<Units, CommandError> {
        u.to_quantity(self.text).map_err(|e| e.with_field(field))
    }

    /// Enregistre l'étiquette `label` de la ressource créée `uid` (une fois
    /// par lot).
    fn define_resource_label(&mut self, label: &str, uid: ResourceUid) -> Result<(), CommandError> {
        if !is_valid_label(label) {
            return Err(invalid("as", "étiquette \"$x\" attendue"));
        }
        if self.resource_labels.contains_key(label) {
            return Err(CommandError::new(
                codes::BATCH_DUPLICATE_LABEL,
                CommandErrorKind::DuplicateLabel(label.to_owned()),
            )
            .with_field("as"));
        }
        self.resource_labels.insert(label.to_owned(), uid);
        Ok(())
    }
}

/// Type du modèle d'un type JSON, avec l'étiquette d'unité d'une
/// matérielle.
fn kind_of(k: ResourceKindJson, label: String) -> ResourceKind {
    match k {
        ResourceKindJson::Work => ResourceKind::Work,
        ResourceKindJson::Material => ResourceKind::Material { label },
        ResourceKindJson::Cost => ResourceKind::Cost,
    }
}

/// Calendrier de base dont dérive le calendrier propre d'une ressource :
/// celui du projet s'il est de base, sinon sa base (R-195).
fn own_base(ctx: &Ctx<'_>) -> CalendarUid {
    let project = ctx.project.calendar;
    match ctx.project.calendars.get(project).map(Calendar::kind) {
        Some(CalendarKind::Derived { base, .. }) => *base,
        Some(CalendarKind::Base { .. }) | None => project,
    }
}

/// R-195 : `resource.add`.
pub(super) fn add(op: &ResourceAdd, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    check_text("name", &op.name)?;
    for (field, v) in [
        ("initials", &op.initials),
        ("group", &op.group),
        ("material_label", &op.material_label),
    ] {
        if let Some(v) = v {
            check_text(field, v)?;
        }
    }
    let kind_json = op.kind.unwrap_or_default();
    let work = kind_json == ResourceKindJson::Work;
    if kind_json != ResourceKindJson::Material && op.material_label.is_some() {
        return Err(invalid(
            "material_label",
            "étiquette d'unité réservée à une ressource matérielle",
        ));
    }
    if !work && op.max_units.is_some() {
        return Err(invalid(
            "max_units",
            "capacité réservée à une ressource de travail",
        ));
    }
    if !work && op.calendar.is_some() {
        return Err(invalid(
            "calendar",
            "calendrier réservé à une ressource de travail",
        ));
    }
    let max_units = op
        .max_units
        .as_ref()
        .map(|u| ctx.units(u, "max_units"))
        .transpose()?
        .unwrap_or(Units::FULL);
    let calendar_choice = if work {
        Some(op.calendar.unwrap_or_default())
    } else {
        None
    };
    if let Some(ResourceCalendarJson::Uid(c)) = calendar_choice
        && !ctx.project.calendars.contains(c)
    {
        return Err(calendar_not_found(c, "calendar"));
    }

    let uid = ctx.uids.alloc_resource()?;
    if let Some(label) = &op.label {
        ctx.define_resource_label(label, uid)?;
    }
    let mut r = Resource::new(uid, op.name.clone());
    r.kind = kind_of(kind_json, op.material_label.clone().unwrap_or_default());
    r.max_units = max_units;
    if let Some(i) = &op.initials {
        r.initials.clone_from(i);
    }
    if let Some(g) = &op.group {
        r.group.clone_from(g);
    }
    r.calendar = match calendar_choice {
        Some(ResourceCalendarJson::Own) => {
            // Calendrier propre, nommé comme la ressource (O34).
            let cal = ctx.uids.alloc_calendar()?;
            let base = own_base(ctx);
            ctx.project
                .calendars
                .push(Calendar::new_derived(cal, op.name.clone(), base))
                .map_err(|e| model(&e))?;
            ctx.effects.push(SideEffect::CalendarCreated(cal));
            ctx.touched.insert(Subject::Calendar(cal), ctx.op_index);
            Some(cal)
        }
        Some(ResourceCalendarJson::Uid(c)) => Some(c),
        Some(ResourceCalendarJson::Project) | None => None,
    };
    ctx.project.resources.push(r).map_err(|e| model(&e))?;
    ctx.created_resources.push(uid);
    ctx.touched.insert(Subject::Resource(uid), ctx.op_index);
    Ok(())
}

/// Applique un `Patch<String>` à un texte effaçable (`null` → vide).
fn patch_text(field: &mut String, patch: &Patch<String>) -> bool {
    let new = match patch {
        Patch::Keep => return false,
        Patch::Set(v) => v.as_str(),
        Patch::Clear => "",
    };
    if field == new {
        return false;
    }
    new.clone_into(field);
    true
}

/// Affectations de la ressource `uid`, par UID croissant.
fn assignments_of(ctx: &Ctx<'_>, uid: ResourceUid) -> Vec<AssignmentUid> {
    let mut v: Vec<AssignmentUid> = ctx
        .project
        .assignments
        .iter()
        .filter(|a| a.resource == Some(uid))
        .map(HasUid::uid)
        .collect();
    v.sort();
    v
}

/// `resource.has_assignments { count }` sur `field`.
fn has_assignments(uid: ResourceUid, count: usize, field: &str) -> CommandError {
    CommandError::new(
        codes::RESOURCE_HAS_ASSIGNMENTS,
        CommandErrorKind::ResourceHasAssignments { count },
    )
    .with_field(field)
    .with_subject(Subject::Resource(uid))
}

/// Même type, étiquette d'unité mise à part.
fn same_kind(a: &ResourceKind, b: &ResourceKind) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

/// Taux à poser par `resource.update` (R-195 amendée) ; `None` : gardé.
struct Rates {
    standard: Option<Rate>,
    overtime: Option<Rate>,
    per_use: Option<Money>,
}

/// Montant d'un taux en centièmes, positif ou nul, dans les bornes de
/// `Money` (sinon `param.invalid`).
fn amount(v: i64, field: &str) -> Result<Money, CommandError> {
    if v < 0 {
        return Err(invalid(field, "montant négatif"));
    }
    Money::new(v).map_err(|_| invalid(field, "montant hors bornes"))
}

/// Unité du modèle d'une unité JSON.
const fn rate_unit(u: RateUnitJson) -> RateUnit {
    match u {
        RateUnitJson::Minute => RateUnit::Minute,
        RateUnitJson::Hour => RateUnit::Hour,
        RateUnitJson::Day => RateUnit::Day,
        RateUnitJson::Week => RateUnit::Week,
        RateUnitJson::Month => RateUnit::Month,
        RateUnitJson::Year => RateUnit::Year,
        RateUnitJson::MaterialUnit => RateUnit::MaterialUnit,
    }
}

/// R-195 amendée (O46, ADR-0013 D7) : taux stockés tels que saisis
/// (montant en centièmes et unité, jamais convertis ni calculés).
/// Ressource de travail : unité de temps (défaut heure) ; matérielle :
/// unité de matériau seulement (défaut), pas de taux des heures
/// supplémentaires ; coût : aucun taux. `cost_per_use` : montant seul
/// (`per` refusé). `null` remet 0 (par heure, ou par unité de matériau).
fn rates(op: &ResourceUpdate, kind: &ResourceKind) -> Result<Rates, CommandError> {
    let material = matches!(kind, ResourceKind::Material { .. });
    let cost = *kind == ResourceKind::Cost;
    let default_unit = if material {
        RateUnit::MaterialUnit
    } else {
        RateUnit::Hour
    };
    let rate = |p: &Patch<RateJson>, field: &str| -> Result<Option<Rate>, CommandError> {
        let r = match p {
            Patch::Keep => return Ok(None),
            Patch::Clear => RateJson { amount: 0, per: None },
            Patch::Set(r) => *r,
        };
        if cost {
            return Err(invalid(field, "taux sans objet pour une ressource de coût"));
        }
        let per = r.per.map_or(default_unit, rate_unit);
        if material != (per == RateUnit::MaterialUnit) {
            return Err(invalid(
                field,
                "unité de matériau pour une ressource matérielle, unité de temps sinon",
            ));
        }
        Ok(Some(Rate {
            amount: amount(r.amount, field)?,
            per,
        }))
    };
    let standard = rate(&op.standard_rate, "standard_rate")?;
    if material && !op.overtime_rate.is_keep() {
        return Err(invalid(
            "overtime_rate",
            "pas de taux des heures supplémentaires pour une ressource matérielle",
        ));
    }
    let overtime = rate(&op.overtime_rate, "overtime_rate")?;
    let per_use = match &op.cost_per_use {
        Patch::Keep => None,
        Patch::Clear => Some(Money::ZERO),
        Patch::Set(r) => {
            if r.per.is_some() {
                return Err(invalid(
                    "cost_per_use",
                    "« per » sans objet pour un coût par utilisation",
                ));
            }
            Some(amount(r.amount, "cost_per_use")?)
        }
    };
    Ok(Rates {
        standard,
        overtime,
        per_use,
    })
}

/// R-195 : `resource.update`.
#[allow(
    clippy::too_many_lines,
    reason = "un contrôle par champ, puis une écriture par champ, dans l'ordre du contrat"
)]
pub(super) fn update(op: &ResourceUpdate, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let uid = ctx.resource(&op.uid, "uid")?;
    let Some(current) = ctx.project.resources.get(uid) else {
        return Err(not_found(uid, "uid"));
    };
    let current_kind = current.kind.clone();
    // Contrôles, avant toute modification.
    match &op.name {
        Patch::Clear => return Err(invalid("name", "le nom ne s'efface pas (chaîne vide acceptée)")),
        Patch::Set(n) => check_text("name", n)?,
        Patch::Keep => {}
    }
    for (field, p) in [
        ("initials", &op.initials),
        ("group", &op.group),
        ("material_label", &op.material_label),
    ] {
        if let Patch::Set(v) = p {
            check_text(field, v)?;
        }
    }
    let new_kind = match &op.kind {
        Patch::Keep => current_kind.clone(),
        Patch::Clear => return Err(invalid("kind", "type attendu (il ne s'efface pas)")),
        Patch::Set(k) => {
            let label = match &current_kind {
                ResourceKind::Material { label } => label.clone(),
                ResourceKind::Work | ResourceKind::Cost => String::new(),
            };
            let k = kind_of(*k, label);
            if !same_kind(&k, &current_kind) {
                let count = assignments_of(ctx, uid).len();
                if count > 0 {
                    return Err(has_assignments(uid, count, "kind"));
                }
            }
            k
        }
    };
    let work = new_kind == ResourceKind::Work;
    let material = matches!(new_kind, ResourceKind::Material { .. });
    if !material && !op.material_label.is_keep() {
        return Err(invalid(
            "material_label",
            "étiquette d'unité réservée à une ressource matérielle",
        ));
    }
    let max_units = match &op.max_units {
        Patch::Keep => None,
        Patch::Clear => return Err(invalid("max_units", "capacité attendue (elle ne s'efface pas)")),
        Patch::Set(_) if !work => {
            return Err(invalid(
                "max_units",
                "capacité réservée à une ressource de travail",
            ));
        }
        Patch::Set(u) => Some(ctx.units(u, "max_units")?),
    };
    let calendar = match op.calendar {
        Patch::Keep => None,
        Patch::Clear => Some(None),
        Patch::Set(_) if !work => {
            return Err(invalid(
                "calendar",
                "calendrier réservé à une ressource de travail",
            ));
        }
        Patch::Set(c) => {
            if !ctx.project.calendars.contains(c) {
                return Err(calendar_not_found(c, "calendar"));
            }
            Some(Some(c))
        }
    };
    if op.inactive == Patch::Clear {
        return Err(invalid("inactive", "booléen attendu"));
    }
    let rates = rates(op, &new_kind)?;

    let Some(r) = ctx.project.resources.get_mut(uid) else {
        return Err(not_found(uid, "uid"));
    };
    if let Patch::Set(n) = &op.name {
        r.name.clone_from(n);
    }
    let mut calendar_changed = false;
    if !same_kind(&new_kind, &r.kind) && !work && r.calendar.is_some() {
        // Matérielle ou coût : aucun calendrier (le calendrier propre reste
        // dans le projet).
        r.calendar = None;
        calendar_changed = true;
    }
    r.kind = new_kind;
    if let (ResourceKind::Material { label }, Patch::Set(_) | Patch::Clear) =
        (&mut r.kind, &op.material_label)
    {
        patch_text(label, &op.material_label);
    }
    if let Some(u) = max_units {
        r.max_units = u;
    }
    if let Some(c) = calendar
        && r.calendar != c
    {
        r.calendar = c;
        calendar_changed = true;
    }
    patch_text(&mut r.initials, &op.initials);
    patch_text(&mut r.group, &op.group);
    patch_text(&mut r.notes, &op.notes);
    if let Patch::Set(b) = op.inactive {
        r.is_inactive = b;
    }
    if let Some(rate) = rates.standard {
        r.standard_rate = rate;
    }
    if let Some(rate) = rates.overtime {
        r.overtime_rate = rate;
    }
    if let Some(cost) = rates.per_use {
        r.cost_per_use = cost;
    }
    if calendar_changed {
        // R-161 : le calendrier change le placement de ses affectations.
        for a in assignments_of(ctx, uid) {
            ctx.drop_one_assignment_extras(a);
        }
    }
    ctx.touched.insert(Subject::Resource(uid), ctx.op_index);
    Ok(())
}

/// R-195 : `resource.delete`. Avec des affectations, `cascade` est
/// obligatoire : chacune est retirée par R-191 ou R-192, dans l'ordre des
/// UID ; le calendrier créé pour la ressource est gardé.
pub(super) fn delete(op: &ResourceDelete, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let uid = ctx.resource(&op.uid, "uid")?;
    let assigned = assignments_of(ctx, uid);
    if !assigned.is_empty() && !op.cascade {
        return Err(has_assignments(uid, assigned.len(), "cascade"));
    }
    let mut skip = BTreeSet::new();
    for a in &assigned {
        unbalance(ctx, *a, &mut skip)?;
    }
    if !skip.is_empty() {
        // Une reconstruction de l'arène pour toutes les affectations.
        let all: Vec<Assignment> = std::mem::take(&mut ctx.project.assignments).into();
        let (gone, kept): (Vec<Assignment>, Vec<Assignment>) =
            all.into_iter().partition(|a| skip.contains(&a.uid()));
        ctx.project.assignments = kept.try_into().map_err(|e: ModelError| model(&e))?;
        for a in gone {
            ctx.touched.insert(Subject::Assignment(a.uid()), ctx.op_index);
            ctx.effects.push(SideEffect::AssignmentRemoved(a.uid()));
        }
    }
    ctx.project.resources.remove(uid);
    ctx.touched.insert(Subject::Resource(uid), ctx.op_index);
    Ok(())
}
