//! Lignes de base à l'écriture (R-126) : par numéro croissant, valeurs
//! saisies et `Extras` de la portée `Baseline#n/`.

use std::collections::BTreeMap;

use plannercraft_core::{AssignmentBaseline, BaselineNumber, Money, ResourceBaseline, TaskBaseline, Work};

use super::emit::Emitter;
use super::{OrderProfile, Planned, entity, scoped};
use crate::codes;
use crate::scalar::{format_bool, format_datetime, format_iso_duration};
use crate::schema::Parent;

/// Travail ISO.
fn work(w: Option<Work>) -> Option<String> {
    w.map(|w| format_iso_duration(w.minutes().get()))
}

/// Coût en centièmes (entier, O18).
fn cents(m: Option<Money>) -> Option<String> {
    m.map(|m| m.get().to_string())
}

/// Lignes de base d'une tâche.
pub(crate) fn task(
    em: &mut Emitter,
    baselines: &BTreeMap<BaselineNumber, TaskBaseline>,
    planned: &[Planned<'_>],
    profile: OrderProfile,
) {
    for (n, b) in baselines {
        let extras = scoped(planned, n.get());
        entity(
            em,
            "Baseline",
            Parent::TaskBaseline,
            profile,
            &extras,
            |em, name| match name {
                "Number" => em.leaf(name, &n.get().to_string()),
                "Start" => em.opt_leaf(name, b.start.map(format_datetime)),
                "Finish" => em.opt_leaf(name, b.finish.map(format_datetime)),
                "Duration" => em.opt_leaf(name, b.duration.map(|d| format_iso_duration(d.minutes().get()))),
                "DurationFormat" => em.opt_leaf(
                    name,
                    b.duration.map(|d| {
                        codes::duration_format_code(d.unit(), d.basis(), d.is_estimated()).to_string()
                    }),
                ),
                "EstimatedDuration" => {
                    em.opt_leaf(name, b.duration.map(|d| format_bool(d.is_estimated()).to_owned()));
                }
                "Work" => em.opt_leaf(name, work(b.work)),
                "Cost" => em.opt_leaf(name, cents(b.cost)),
                "FixedCost" => em.opt_leaf(name, cents(b.fixed_cost)),
                _ => {}
            },
        );
    }
}

/// Lignes de base d'une ressource.
pub(crate) fn resource(
    em: &mut Emitter,
    baselines: &BTreeMap<BaselineNumber, ResourceBaseline>,
    planned: &[Planned<'_>],
    profile: OrderProfile,
) {
    for (n, b) in baselines {
        let extras = scoped(planned, n.get());
        entity(
            em,
            "Baseline",
            Parent::ResourceBaseline,
            profile,
            &extras,
            |em, name| match name {
                "Number" => em.leaf(name, &n.get().to_string()),
                "Work" => em.opt_leaf(name, work(b.work)),
                "Cost" => em.opt_leaf(name, cents(b.cost)),
                _ => {}
            },
        );
    }
}

/// Lignes de base d'une affectation.
pub(crate) fn assignment(
    em: &mut Emitter,
    baselines: &BTreeMap<BaselineNumber, AssignmentBaseline>,
    planned: &[Planned<'_>],
    profile: OrderProfile,
) {
    for (n, b) in baselines {
        let extras = scoped(planned, n.get());
        entity(
            em,
            "Baseline",
            Parent::AssignmentBaseline,
            profile,
            &extras,
            |em, name| match name {
                "Number" => em.leaf(name, &n.get().to_string()),
                "Start" => em.opt_leaf(name, b.start.map(format_datetime)),
                "Finish" => em.opt_leaf(name, b.finish.map(format_datetime)),
                "Work" => em.opt_leaf(name, work(b.work)),
                "Cost" => em.opt_leaf(name, cents(b.cost)),
                _ => {}
            },
        );
    }
}
