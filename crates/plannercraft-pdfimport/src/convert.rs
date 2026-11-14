//! Conversion du planning lu en projet (étape 7), par les commandes du
//! moteur (`task.add`, `link.add`, `task.set_constraint`), jamais en
//! écrivant le modèle à la main. **Règle 3 : les dates viennent du
//! moteur.** Les dates lues servent seulement (ADR-0032 D5, R-270 à
//! R-272) :
//! - à fixer la date de début du projet (plus petit début lu, R-270) ;
//! - à poser « Début au plus tôt le » (début lu) sur une feuille sans
//!   prédécesseur propre que le moteur, sans elle, placerait avant son
//!   début lu (R-271 ; sur demande, aussi sur les feuilles liées dont
//!   aucun prédécesseur n'est en avance) ;
//! - à comparer : après calcul, chaque date calculée qui diffère au jour
//!   de la date lue est listée avec l'écart (R-272).
//!
//! Lot P1b : des jours chômés choisis par l'utilisateur (jours fériés
//! français, R-274 ; périodes déduites des écarts, R-276) sont posés comme
//! exceptions du calendrier du projet (`calendar.set_exception`) avant les
//! tâches (R-275).

use std::collections::{BTreeMap, BTreeSet};

use plannercraft_automation::command::{
    CalendarException, CalendarRef, ConstraintKindJson, DurationJson, LagJson, LinkSpec, LinkTypeJson,
    SetConstraint, TaskAdd,
};
use plannercraft_automation::text::{Language, TextContext};
use plannercraft_automation::{
    AnchorSpec, ApplyMode, Batch, Command, DocOrigin, NewProject, Placement, Session, SessionLimits, TaskRef,
};
use plannercraft_core::{Day, LinkType, Moment, TaskUid};
use serde::Serialize;

use crate::plan::{ImportedPlan, ImportedTask, Warning, WarningCode};

/// Date dont le calcul diffère de la lecture.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DateGap {
    /// N° lu de la tâche.
    pub id: u32,
    /// Champ (`start` ou `finish`).
    pub field: &'static str,
    /// Date lue (`AAAA-MM-JJ`).
    pub read: String,
    /// Date calculée par le moteur (`AAAA-MM-JJ`).
    pub computed: String,
    /// Écart en jours calendaires (calculé moins lu).
    pub days: i64,
}

/// Projet construit et comparaison des dates.
#[derive(Debug)]
pub struct Converted {
    /// Session sur le projet (historique vide, document non modifié).
    pub session: Session,
    /// UID de chaque tâche du planning lu, dans l'ordre.
    pub uids: Vec<TaskUid>,
    /// Avertissements de la conversion (liens ou contraintes refusés).
    pub warnings: Vec<Warning>,
    /// Début du projet (plus petit début lu).
    pub project_start: Moment,
    /// Contraintes « Début au plus tôt le » posées.
    pub constraints: u32,
    /// Liens posés.
    pub links: u32,
    /// Dates lues comparées (début et fin).
    pub dates_compared: u32,
    /// Dates égales au jour près.
    pub dates_equal: u32,
    /// Écarts, dans l'ordre des tâches.
    pub gaps: Vec<DateGap>,
}

/// Erreur de conversion (aucune date lue, ou commande refusée par le
/// moteur sur l'ajout des tâches).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConvertError {
    /// Aucune date lue : pas de date de début du projet.
    NoStart,
    /// Le moteur a refusé une commande (code stable du moteur).
    Refused(String),
}

impl std::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoStart => f.write_str("aucune date lue : début du projet inconnu"),
            Self::Refused(code) => write!(f, "commande refusée par le moteur : {code}"),
        }
    }
}

impl std::error::Error for ConvertError {}

/// Passes de contraintes au plus (`pin_read_starts`).
const MAX_CONSTRAINT_PASSES: usize = 50;

/// Jours chômés à poser dans le calendrier du projet (R-275).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NonWorkingDays {
    /// Premier jour.
    pub from: Day,
    /// Dernier jour (inclus).
    pub to: Day,
    /// Nom de l'exception (« Noël », « Congés déduits de l'import »…).
    pub name: String,
}

/// Jours chômés posés au plus par une conversion (borne, règle 10).
pub const MAX_NON_WORKING_DAYS: usize = 20_000;

/// Options de la conversion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConvertOptions {
    /// Nom du projet (nom du fichier PDF, par exemple).
    pub name: String,
    /// « Maintenant » injecté (date courante du projet).
    pub now: Moment,
    /// Langue de la session.
    pub lang: Language,
    /// R-271 étendue : poser « Début au plus tôt le » aussi sur les
    /// feuilles qui ont des prédécesseurs, quand le moteur les place avant
    /// leur début lu (contrainte invisible dans le PDF, décalage non
    /// imprimé) ; sinon seulement sur les feuilles sans prédécesseur.
    pub pin_read_starts: bool,
    /// Jours chômés posés comme exceptions du calendrier du projet avant
    /// les tâches (R-275) ; un jour cité deux fois garde le premier nom.
    pub non_working: Vec<NonWorkingDays>,
}

/// Étiquette de lot de la tâche de rang `k`.
fn label(k: usize) -> String {
    format!("$t{k}")
}

/// R-270 : début du projet = plus petit début lu ; à défaut, plus petite
/// fin lue ramenée à l'heure de début par défaut.
fn project_start(plan: &ImportedPlan) -> Option<Moment> {
    plan.tasks.iter().filter_map(|t| t.start).min().or_else(|| {
        let day = plan.tasks.iter().filter_map(|t| t.finish).min()?.day();
        Moment::at(
            day,
            plannercraft_core::ProjectOptions::default().default_start_time,
        )
        .ok()
    })
}

const fn link_json(kind: LinkType) -> LinkTypeJson {
    match kind {
        LinkType::FinishToStart => LinkTypeJson::Fs,
        LinkType::StartToStart => LinkTypeJson::Ss,
        LinkType::FinishToFinish => LinkTypeJson::Ff,
        LinkType::StartToFinish => LinkTypeJson::Sf,
    }
}

/// Construit le projet : tâches et hiérarchie, liens, contraintes (R-271),
/// puis compare les dates calculées aux dates lues (R-272).
///
/// # Errors
///
/// [`ConvertError`] si aucune date n'est lue ou si le moteur refuse
/// l'ajout des tâches.
#[allow(clippy::too_many_lines, reason = "quatre étapes linéaires commentées")]
pub fn convert(plan: &ImportedPlan, options: &ConvertOptions) -> Result<Converted, ConvertError> {
    let ConvertOptions { name, now, lang, .. } = options;
    let (name, now, lang) = (name.as_str(), *now, *lang);
    let start = project_start(plan).ok_or(ConvertError::NoStart)?;
    let mut session = Session::new_project(
        NewProject {
            name: name.to_owned(),
            anchor: AnchorSpec::Start(start),
            lang: Some(lang),
        },
        now,
    )
    .map_err(|e| ConvertError::Refused(e.code.to_owned()))?;
    // Un lot porte toutes les tâches (les étiquettes `$tN` ne valent que
    // dans leur lot) : borne des opérations relevée à la borne de l'import.
    let exceptions = exception_ops(&options.non_working);
    session.set_limits(SessionLimits {
        max_ops_per_batch: plan
            .tasks
            .len()
            .max(exceptions.len())
            .saturating_add(1)
            .max(SessionLimits::default().max_ops_per_batch),
        ..SessionLimits::default()
    });
    let ctx = TextContext::for_project(session.project(), lang);
    let mut warnings = Vec::new();

    // 0. R-275 : jours chômés du calendrier du projet, avant les tâches
    // (les contraintes de R-271 dépendent du calendrier).
    if !exceptions.is_empty() {
        apply(&mut session, exceptions, lang).map_err(ConvertError::Refused)?;
    }

    // 1. Tâches, hiérarchie et durées.
    let mut ops = Vec::with_capacity(plan.tasks.len());
    let mut parents: Vec<(u16, usize)> = Vec::new();
    for (k, t) in plan.tasks.iter().enumerate() {
        while parents.last().is_some_and(|(level, _)| *level >= t.level) {
            parents.pop();
        }
        let at = parents
            .last()
            .map(|(_, p)| Placement::LastChildOf(TaskRef::Label(label(*p))));
        ops.push(Command::TaskAdd(task_add(t, k, at, &ctx)));
        if t.summary {
            parents.push((t.level, k));
        }
    }
    let outcome = apply(&mut session, ops, lang).map_err(ConvertError::Refused)?;
    let uids: Vec<TaskUid> = (0..plan.tasks.len())
        .map(|k| outcome.labels.get(&label(k)).copied().unwrap_or(TaskUid::new(0)))
        .collect();

    // 2. Liens, en un lot ; s'il est refusé (boucle), un par un.
    let by_id: BTreeMap<u32, TaskUid> = plan.tasks.iter().zip(&uids).map(|(t, u)| (t.id, *u)).collect();
    let mut links: Vec<(u32, Command)> = Vec::new();
    for (t, succ) in plan.tasks.iter().zip(&uids) {
        for p in &t.predecessors {
            let Some(pred) = by_id.get(&p.id) else { continue };
            links.push((
                t.id,
                Command::LinkAdd(LinkSpec {
                    pred: TaskRef::Uid(*pred),
                    succ: TaskRef::Uid(*succ),
                    kind: Some(link_json(p.kind)),
                    lag: Some(LagJson::from_lag(&p.lag, &ctx)),
                }),
            ));
        }
    }
    let mut placed_links = 0_u32;
    let all: Vec<Command> = links.iter().map(|(_, c)| c.clone()).collect();
    if apply(&mut session, all, lang).is_ok() {
        placed_links = u32::try_from(links.len()).unwrap_or(u32::MAX);
    } else {
        for (id, c) in links {
            let value = match &c {
                Command::LinkAdd(spec) => format!("{:?} → {id}", spec.pred),
                _ => String::new(),
            };
            match apply(&mut session, vec![c], lang) {
                Ok(_) => placed_links = placed_links.saturating_add(1),
                Err(code) => warnings.push(Warning {
                    code: WarningCode::LinkRefused,
                    id: Some(id),
                    page: None,
                    value: Some(format!("{value} ({code})")),
                }),
            }
        }
    }

    // 3. R-271 : « Début au plus tôt le » (début lu) sur les feuilles que le
    // moteur place avant leur début lu : sans prédécesseur propre par
    // défaut ; avec `pin_read_starts`, aussi celles qui en ont, par passes
    // successives, en commençant par celles dont aucun prédécesseur n'est
    // lui-même en avance (la divergence est corrigée à sa source).
    let mut constrained: BTreeSet<usize> = BTreeSet::new();
    let mut placed_constraints = 0_u32;
    for _pass in 0..MAX_CONSTRAINT_PASSES {
        let early = |k: usize| -> bool {
            let (Some(t), Some(uid)) = (plan.tasks.get(k), uids.get(k)) else {
                return false;
            };
            let Some(read) = t.start else { return false };
            let computed = session.schedule().scheduled(*uid).map(|s| s.start);
            computed.is_some_and(|c| c.day() < read.day())
        };
        let finish_early = |id: u32| -> bool {
            let Some(k) = plan.tasks.iter().position(|t| t.id == id) else {
                return false;
            };
            let (Some(t), Some(uid)) = (plan.tasks.get(k), uids.get(k)) else {
                return false;
            };
            let Some(read) = t.finish else { return false };
            let computed = session.schedule().scheduled(*uid).map(|s| s.finish);
            computed.is_some_and(|c| c.day() < read.day())
        };
        let mut batch = Vec::new();
        for (k, t) in plan.tasks.iter().enumerate() {
            let Some(read) = t.start else { continue };
            if t.summary || read <= start || constrained.contains(&k) {
                continue;
            }
            let eligible = if options.pin_read_starts {
                t.predecessors.iter().all(|p| !finish_early(p.id))
            } else {
                t.predecessors.is_empty()
            };
            if eligible && early(k) {
                batch.push(k);
            }
        }
        if batch.is_empty() {
            break;
        }
        let command = |k: usize| {
            let t = plan.tasks.get(k)?;
            Some(Command::TaskSetConstraint(SetConstraint {
                uid: TaskRef::Uid(*uids.get(k)?),
                kind: ConstraintKindJson::Snet,
                date: t.start,
            }))
        };
        let all: Vec<Command> = batch.iter().filter_map(|k| command(*k)).collect();
        if apply(&mut session, all, lang).is_ok() {
            placed_constraints =
                placed_constraints.saturating_add(u32::try_from(batch.len()).unwrap_or(u32::MAX));
        } else {
            for k in &batch {
                let Some(c) = command(*k) else { continue };
                match apply(&mut session, vec![c], lang) {
                    Ok(_) => placed_constraints = placed_constraints.saturating_add(1),
                    Err(code) => warnings.push(Warning {
                        code: WarningCode::ConstraintRefused,
                        id: plan.tasks.get(*k).map(|t| t.id),
                        page: None,
                        value: Some(code),
                    }),
                }
            }
        }
        constrained.extend(batch);
        if !options.pin_read_starts {
            break;
        }
    }

    // Document neuf : historique vide, non modifié.
    let origin = DocOrigin {
        name: name.to_owned(),
        path: None,
    };
    let session = Session::from_project(session.project().clone(), origin, now, lang);

    // 4. R-272 : comparaison au jour.
    let mut gaps = Vec::new();
    let mut compared = 0_u32;
    let mut equal = 0_u32;
    for (t, uid) in plan.tasks.iter().zip(&uids) {
        let scheduled = session.schedule().scheduled(*uid);
        for (field, read, computed) in [
            ("start", t.start, scheduled.map(|s| s.start)),
            ("finish", t.finish, scheduled.map(|s| s.finish)),
        ] {
            let (Some(read), Some(computed)) = (read, computed) else {
                continue;
            };
            compared = compared.saturating_add(1);
            // Une fin calculée à minuit (00:00) appartient au jour d'avant.
            let computed_day = if field == "finish" && computed.time_of_day().minutes() == 0 {
                computed.day().checked_add_days(-1).unwrap_or(computed.day())
            } else {
                computed.day()
            };
            let days = i64::from(computed_day.index()).saturating_sub(i64::from(read.day().index()));
            if days == 0 {
                equal = equal.saturating_add(1);
            } else {
                gaps.push(DateGap {
                    id: t.id,
                    field,
                    read: read.day().to_string(),
                    computed: computed_day.to_string(),
                    days,
                });
            }
        }
    }
    Ok(Converted {
        session,
        uids,
        warnings,
        project_start: start,
        constraints: placed_constraints,
        links: placed_links,
        dates_compared: compared,
        dates_equal: equal,
        gaps,
    })
}

/// R-275 : une commande `calendar.set_exception` (jours chômés, calendrier
/// du projet) par suite de jours consécutifs de même nom ; un jour cité deux
/// fois garde le premier nom (les exceptions ne se chevauchent jamais,
/// sinon le moteur refuserait le lot) ; [`MAX_NON_WORKING_DAYS`] jours au
/// plus.
#[must_use]
pub fn exception_ops(periods: &[NonWorkingDays]) -> Vec<Command> {
    let mut days: BTreeMap<Day, &str> = BTreeMap::new();
    'periods: for p in periods {
        let mut d = p.from;
        while d <= p.to {
            if days.len() >= MAX_NON_WORKING_DAYS {
                break 'periods;
            }
            days.entry(d).or_insert(p.name.as_str());
            let Ok(next) = d.checked_add_days(1) else { break };
            d = next;
        }
    }
    let mut runs: Vec<(Day, Day, &str)> = Vec::new();
    for (d, name) in days {
        match runs.last_mut() {
            Some((_, to, n)) if *n == name && to.checked_add_days(1).ok() == Some(d) => *to = d,
            _ => runs.push((d, d, name)),
        }
    }
    runs.into_iter()
        .map(|(from, to, name)| {
            Command::CalendarSetException(CalendarException {
                cal: CalendarRef::Project,
                from,
                to,
                ranges: Vec::new(),
                name: Some(name.to_owned()),
            })
        })
        .collect()
}

/// `task.add` d'une tâche lue : nom, placement, durée lue (feuille
/// seulement : la durée d'une récapitulative est calculée), jalon.
fn task_add(t: &ImportedTask, k: usize, at: Option<Placement>, ctx: &TextContext) -> TaskAdd {
    let duration = if t.summary {
        None
    } else {
        t.duration.as_ref().map(|d| DurationJson::from_duration(d, ctx))
    };
    TaskAdd {
        name: t.name.clone(),
        label: Some(label(k)),
        at,
        duration,
        milestone: t.milestone().then_some(true),
        notes: None,
        start: None,
        finish: None,
    }
}

/// Applique un lot ; rend le code du moteur en cas de refus.
fn apply(
    session: &mut Session,
    ops: Vec<Command>,
    lang: Language,
) -> Result<plannercraft_automation::ApplyOutcome, String> {
    let batch = Batch {
        format_version: Batch::FORMAT_VERSION,
        lang: Some(lang),
        ops,
        label: Some("Import PDF".to_owned()),
    };
    session
        .apply(&batch, ApplyMode::Commit)
        .map_err(|e| e.code.to_owned())
}
