//! Lignes de base des tâches, ressources et affectations (R-088, R-126) :
//! numéro 0 à 10, valeurs modélisées ; le reste (`TimephasedData`,
//! `Interim`…) en `Extras` de l'entité sous la portée `Baseline#n/` (R-087).

use std::collections::BTreeMap;

use plannercraft_core::{
    AssignmentBaseline, BaselineNumber, Duration, ExtraEntry, Minutes, Money, ProjectOptions,
    ResourceBaseline, Subject, TaskBaseline, TimeBasis, Work,
};

use crate::codes::{self, DurationCode};
use crate::diag::{ImportCode, Sink};
use crate::extras;
use crate::fields::Fields;
use crate::project::code;
use crate::scalar::parse_decimal;
use crate::schema::Parent;
use crate::xml::Element;

/// Lignes de base lues et leurs `Extras` (portée `Baseline#n/`).
pub(crate) struct Read<T> {
    pub(crate) map: BTreeMap<BaselineNumber, T>,
    pub(crate) extras: Vec<ExtraEntry>,
}

/// Lit les éléments `Baseline` d'une entité ; `read_one` lit les valeurs
/// d'une ligne. Numéro absent, illisible ou hors de 0 à 10 : ligne ignorée,
/// signalée ; numéro en double : la première gagne.
fn read_all<T>(
    children: &[Element<'_>],
    path: &'static str,
    parent: Parent,
    subject: Option<Subject>,
    sink: &mut Sink<'_>,
    mut read_one: impl FnMut(&Fields<'_>, &mut Sink<'_>) -> T,
) -> Read<T> {
    let mut map = BTreeMap::new();
    let mut extras = Vec::new();
    for e in children
        .iter()
        .filter(|c| c.name == "Baseline" && c.raw.is_none())
    {
        let f = Fields::new(&e.children, path, subject, e.at);
        let Some((n, at)) = f.int("Number", sink) else {
            if f.scalar("Number", sink).is_none() {
                f.missing("Number", sink);
            }
            continue;
        };
        let Some(number) = u8::try_from(n).ok().and_then(|n| BaselineNumber::new(n).ok()) else {
            f.invalid("Number", at, &n.to_string(), sink);
            continue;
        };
        if map.contains_key(&number) {
            f.report(ImportCode::Duplicate, "Number", at, Some(&n.to_string()), sink);
            continue;
        }
        map.insert(number, read_one(&f, sink));
        extras.extend(extras::collect(&e.children, parent, Some(number.get())));
    }
    Read { map, extras }
}

/// Travail ISO ≥ 0 ; négatif ou trop grand → absent, signalé.
pub(crate) fn work(f: &Fields<'_>, name: &str, sink: &mut Sink<'_>) -> Option<Work> {
    let (d, at) = f.duration(name, sink)?;
    let w = Work::new(Minutes::new(d.minutes)).ok();
    if w.is_none() {
        f.invalid(name, at, &d.minutes.to_string(), sink);
    }
    w
}

/// Coût en centièmes (`Cost`, `FixedCost` des lignes de base : centièmes
/// d'unité monétaire, comme le `Cost` des tâches, O18) ; arrondi à
/// l'unité (R-001, `mspdi.rounded`).
pub(crate) fn cents(f: &Fields<'_>, name: &str, sink: &mut Sink<'_>) -> Option<Money> {
    let (t, at) = f.scalar(name, sink)?;
    let Some(p) = parse_decimal(t) else {
        f.invalid(name, at, t, sink);
        return None;
    };
    let m = p.value.scale_to_integer(1).ok().and_then(|v| Money::new(v).ok());
    match m {
        None => f.invalid(name, at, t, sink),
        Some(_) if p.rounded || p.value.normalized().scale() > 0 => {
            f.report(ImportCode::Rounded, name, at, Some(t), sink);
        }
        Some(_) => {}
    }
    m
}

/// Durée d'une ligne de base de tâche : `Duration` ISO, unité et base par
/// `DurationFormat` (absent : unité par défaut du projet, ouvrée),
/// estimation par `EstimatedDuration` (sinon le code « ? »).
fn baseline_duration(f: &Fields<'_>, options: &ProjectOptions, sink: &mut Sink<'_>) -> Option<Duration> {
    let (d, at) = f.duration("Duration", sink)?;
    let (unit, basis, estimated) = match code(f, "DurationFormat", codes::duration_format, sink) {
        Some(DurationCode::Time {
            unit,
            basis,
            estimated,
        }) => (unit, basis, estimated),
        Some(DurationCode::Null { estimated }) => {
            (options.default_duration_unit, TimeBasis::Working, estimated)
        }
        Some(DurationCode::Percent { .. }) | None => {
            (options.default_duration_unit, TimeBasis::Working, false)
        }
    };
    let estimated = f.bool("EstimatedDuration", sink).unwrap_or(estimated);
    let made = Duration::new(Minutes::new(d.minutes), unit, basis, estimated).ok();
    if made.is_none() {
        f.invalid("Duration", at, &d.minutes.to_string(), sink);
    }
    made
}

/// Lignes de base d'une tâche.
pub(crate) fn task(
    children: &[Element<'_>],
    options: &ProjectOptions,
    subject: Option<Subject>,
    sink: &mut Sink<'_>,
) -> Read<TaskBaseline> {
    read_all(
        children,
        "Tasks/Task/Baseline",
        Parent::TaskBaseline,
        subject,
        sink,
        |f, sink| TaskBaseline {
            start: f.datetime("Start", sink),
            finish: f.datetime("Finish", sink),
            duration: baseline_duration(f, options, sink),
            work: work(f, "Work", sink),
            cost: cents(f, "Cost", sink),
            fixed_cost: cents(f, "FixedCost", sink),
        },
    )
}

/// Lignes de base d'une ressource.
pub(crate) fn resource(
    children: &[Element<'_>],
    subject: Option<Subject>,
    sink: &mut Sink<'_>,
) -> Read<ResourceBaseline> {
    read_all(
        children,
        "Resources/Resource/Baseline",
        Parent::ResourceBaseline,
        subject,
        sink,
        |f, sink| ResourceBaseline {
            work: work(f, "Work", sink),
            cost: cents(f, "Cost", sink),
        },
    )
}

/// Lignes de base d'une affectation.
pub(crate) fn assignment(
    children: &[Element<'_>],
    subject: Option<Subject>,
    sink: &mut Sink<'_>,
) -> Read<AssignmentBaseline> {
    read_all(
        children,
        "Assignments/Assignment/Baseline",
        Parent::AssignmentBaseline,
        subject,
        sink,
        |f, sink| AssignmentBaseline {
            start: f.datetime("Start", sink),
            finish: f.datetime("Finish", sink),
            work: work(f, "Work", sink),
            cost: cents(f, "Cost", sink),
        },
    )
}
