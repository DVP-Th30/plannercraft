//! Commande `task.shift` (R-267, finitions avant v1.0.0, lot P1,
//! ADR-0022 D1) : décale des tâches d'une durée signée, sur le calendrier
//! de chaque tâche.
//!
//! - Tâche automatique : contrainte « Début au plus tôt le » (SNET) posée à
//!   son début planifié courant décalé de `by` ; une contrainte autre que
//!   « Dès que possible » ou SNET est remplacée (effet
//!   `shift_constraint_replaced`).
//! - Tâche manuelle : début et fin manuels décalés de `by`, durée gardée.
//! - Récapitulative sélectionnée : ses feuilles (une passe, lignes vides
//!   ignorées).
//! - Tâche inactive ou achevée : ignorée (effet `shift_skipped`).
//! - Début obtenu plus tard que la date visée (prédécesseurs) : décalage
//!   fait quand même (contrainte posée), effet `shift_limited`.
//!
//! Aucune date n'est estimée : les dates visées viennent du calendrier de
//! la tâche (`TimeScale::add`, R-021 à R-023, R-028) et du planning du
//! moteur.

use std::collections::BTreeSet;

use plannercraft_core::schedule::{Exclusion, TaskOutcome};
use plannercraft_core::{
    Constraint, ConstraintKind, LagValue, Minutes, Moment, Percent, SchedulingMode, Subject, TaskUid,
    TimeBasis,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::exec::Ctx;
use super::extras_invalidation::TaskChange;
use super::task::on_task_scale;
use super::{DurationJson, LagJson, TaskRef, is_summary_at};
use crate::error::{CommandError, CommandErrorKind, codes};
use crate::outcome::SideEffect;

/// `task.shift` (R-267) : décale `tasks` de `by`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskShift {
    /// Tâches à décaler (une récapitulative vaut ses feuilles).
    pub tasks: Vec<TaskRef>,
    /// Décalage **signé** : texte (« 1j », « -1sem », « +2d », « 3ej ») ou
    /// minutes signées (`-480`) avec unité ; jours ouvrés du calendrier de
    /// chaque tâche (temps civil si écoulé) ; pas de pourcentage ni de
    /// « ? ».
    pub by: DurationJson,
}

/// Raison d'une tâche ignorée (`shift_skipped`, code stable).
pub const SKIP_INACTIVE: &str = "inactive";
/// Tâche achevée (100 %).
pub const SKIP_COMPLETE: &str = "complete";
/// Tâche sans date (manuelle sans début ni fin, ou non planifiée).
pub const SKIP_UNDATED: &str = "undated";

/// `param.invalid` sur `field`.
fn invalid(field: &str, why: &str) -> CommandError {
    CommandError::new(
        codes::PARAM_INVALID,
        CommandErrorKind::InvalidParam(why.to_owned()),
    )
    .with_field(field)
}

/// Décalage signé de `by` : (minutes, écoulé). Forme lue comme un décalage
/// de lien (R-142 : signe facultatif), pourcentage refusé ; `estimated`
/// refusé (un décalage n'est pas estimé).
fn amount(by: &DurationJson, ctx: &Ctx<'_>) -> Result<(i64, bool), CommandError> {
    if by.estimated == Some(true) {
        return Err(invalid("by", "un décalage ne peut pas être estimé (« ? »)"));
    }
    let lag = LagJson {
        minutes: by.minutes,
        percent: None,
        unit: by.unit,
        text: by.text.clone(),
        elapsed: by.elapsed,
    };
    let lag = ctx.lag(&lag, "by")?;
    match lag.value {
        LagValue::Time { minutes, .. } => Ok((minutes.get(), lag.basis == TimeBasis::Elapsed)),
        LagValue::Percent { .. } => Err(invalid(
            "by",
            "durée attendue (« 1j », « -1sem ») ; un pourcentage n'a pas de sens ici",
        )),
    }
}

/// Positions des feuilles visées par `uids` (R-267) : une récapitulative
/// vaut ses descendants non récapitulatifs ; lignes vides ignorées ;
/// ordre des ID.
fn leaves(ctx: &Ctx<'_>, uids: &BTreeSet<TaskUid>) -> Vec<TaskUid> {
    let tasks = &ctx.project.tasks;
    let mut out = Vec::new();
    // Niveau de la récapitulative sélectionnée qui couvre la position.
    let mut cover: Option<u16> = None;
    for (pos, t) in tasks.iter().enumerate() {
        if t.is_null {
            continue;
        }
        let level = t.outline_level.get();
        if cover.is_some_and(|c| level <= c) {
            cover = None;
        }
        let picked = uids.contains(&t.uid());
        if cover.is_none() && picked && is_summary_at(tasks, pos) {
            cover = Some(level);
            continue;
        }
        if (cover.is_some() || picked) && !is_summary_at(tasks, pos) {
            out.push(t.uid());
        }
    }
    out
}

/// Ce que le décalage fait d'une feuille.
enum Plan {
    /// Ignorée (raison stable).
    Skip(&'static str),
    /// Automatique : début planifié courant.
    Auto(Moment),
    /// Manuelle : début et fin manuels.
    Manual(Option<Moment>, Option<Moment>),
}

/// Plan d'une feuille, lu sur le planning courant du clone.
fn plan_of(ctx: &Ctx<'_>, uid: TaskUid) -> Plan {
    let Some(t) = ctx.project.tasks.get(uid) else {
        return Plan::Skip(SKIP_UNDATED);
    };
    let outcome = ctx.schedule().get(uid);
    let inactive = !t.active
        || matches!(
            outcome,
            Some(TaskOutcome::Excluded {
                reason: Exclusion::Inactive | Exclusion::InactiveAncestor,
                ..
            })
        );
    if inactive {
        return Plan::Skip(SKIP_INACTIVE);
    }
    if t.progress.percent_complete >= Percent::FULL {
        return Plan::Skip(SKIP_COMPLETE);
    }
    match t.mode {
        SchedulingMode::Manual { start, finish, .. } => {
            if start.is_none() && finish.is_none() {
                Plan::Skip(SKIP_UNDATED)
            } else {
                Plan::Manual(start, finish)
            }
        }
        SchedulingMode::Auto => match ctx.schedule().scheduled(uid) {
            Some(s) => Plan::Auto(s.start),
            None => Plan::Skip(SKIP_UNDATED),
        },
    }
}

/// Début `start` décalé de `by` sur l'échelle de `uid` ; une tâche de
/// durée non nulle commence au premier instant ouvré qui suit (R-024 : 1 j
/// après lun 08:00 = mar 08:00, et non lun 17:00) ; un jalon garde
/// l'instant obtenu (fin d'une minute ouvrée, R-031).
fn shifted_start(
    ctx: &mut Ctx<'_>,
    uid: TaskUid,
    elapsed: bool,
    start: Moment,
    by: Minutes,
) -> Result<Moment, CommandError> {
    let milestone = ctx.project.tasks.get(uid).is_none_or(|t| t.duration.is_zero());
    on_task_scale(ctx, uid, elapsed, "by", |sc| {
        let t = sc.add(start, by)?;
        if milestone {
            Ok(t)
        } else {
            sc.forward_span(t, Minutes::new(1)).map(|span| span.start)
        }
    })
}

/// `task.shift` (R-267).
///
/// # Errors
///
/// `param.invalid` (`tasks` vide, `by` illisible, en pourcentage, estimé,
/// ou date décalée hors du domaine), `task.not_found`, `limit.exceeded`
/// (budget de recalcul du lot).
pub(super) fn shift(op: &TaskShift, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    if op.tasks.is_empty() {
        return Err(invalid("tasks", "au moins une tâche attendue"));
    }
    let (minutes, elapsed) = amount(&op.by, ctx)?;
    let mut picked = BTreeSet::new();
    for r in &op.tasks {
        picked.insert(ctx.task(r, "tasks")?);
    }
    if minutes == 0 {
        return Ok(());
    }
    let targets = leaves(ctx, &picked);
    ctx.ensure_schedule()?;
    let plans: Vec<(TaskUid, Plan)> = targets.into_iter().map(|uid| (uid, plan_of(ctx, uid))).collect();
    let by = Minutes::new(minutes);
    let mut automatic = Vec::new();
    for (uid, plan) in plans {
        match plan {
            Plan::Skip(reason) => ctx.effects.push(SideEffect::ShiftSkipped { task: uid, reason }),
            Plan::Auto(start) => {
                let target = shifted_start(ctx, uid, elapsed, start, by)?;
                let constraint = Constraint::from_parts(ConstraintKind::StartNoEarlierThan, Some(target))
                    .ok_or_else(|| invalid("by", "date décalée hors du domaine"))?;
                let task = ctx.project.tasks.get_mut(uid).ok_or_else(|| {
                    CommandError::new(codes::TASK_NOT_FOUND, CommandErrorKind::UnknownTask(uid))
                })?;
                let previous = task.constraint.kind();
                task.constraint = constraint;
                if !matches!(
                    previous,
                    ConstraintKind::AsSoonAsPossible | ConstraintKind::StartNoEarlierThan
                ) {
                    ctx.effects
                        .push(SideEffect::ShiftConstraintReplaced { task: uid, previous });
                }
                ctx.effects.push(SideEffect::ConstraintSet {
                    uid,
                    kind: ConstraintKind::StartNoEarlierThan,
                });
                ctx.effects.push(SideEffect::TaskShifted { task: uid });
                ctx.drop_task_extras(uid, TaskChange::Constraint);
                ctx.touched.insert(Subject::Task(uid), ctx.op_index);
                automatic.push((uid, target));
            }
            Plan::Manual(start, finish) => {
                let start = match start {
                    Some(s) => Some(shifted_start(ctx, uid, elapsed, s, by)?),
                    None => None,
                };
                let finish = match finish {
                    Some(f) => Some(on_task_scale(ctx, uid, elapsed, "by", |sc| sc.add(f, by))?),
                    None => None,
                };
                let task = ctx.project.tasks.get_mut(uid).ok_or_else(|| {
                    CommandError::new(codes::TASK_NOT_FOUND, CommandErrorKind::UnknownTask(uid))
                })?;
                if let SchedulingMode::Manual { duration, .. } = task.mode {
                    task.mode = SchedulingMode::Manual {
                        start,
                        finish,
                        duration,
                    };
                }
                ctx.effects.push(SideEffect::TaskShifted { task: uid });
                ctx.drop_task_extras(uid, TaskChange::Mode);
                ctx.touched.insert(Subject::Task(uid), ctx.op_index);
            }
        }
    }
    if automatic.is_empty() {
        return Ok(());
    }
    // Début obtenu après la date visée : limité par les prédécesseurs (ou
    // par une autre tâche du lot) ; le décalage reste fait.
    ctx.ensure_schedule()?;
    for (uid, target) in automatic {
        let start = ctx.schedule().scheduled(uid).map(|s| s.start);
        if start.is_some_and(|s| s > target) {
            ctx.effects.push(SideEffect::ShiftLimited { task: uid });
        }
    }
    Ok(())
}
