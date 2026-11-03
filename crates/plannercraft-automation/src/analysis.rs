//! Analyses J6 (lot L8) : `analysis.what_if` (R-234) et
//! `analysis.baseline_variance` (R-228).
//!
//! Contrat de surface (C-MCP-01, règle 3) : toute date, durée et tout écart
//! viennent du moteur — planning simulé par [`Session::simulate`], écarts
//! de `plannercraft_core::baseline::variance` (R-224, R-225), temps ouvré compté
//! par le calendrier compilé du cœur (R-020). Ce module recopie et met en
//! forme ; il ne calcule aucune date.
//!
//! Séparé de `view.rs` (vues des tâches et du projet) pour garder chaque
//! fichier lisible ; les deux partagent [`ViewCtx`].

use std::collections::BTreeMap;

use plannercraft_core::schedule::explain::{DEFAULT_DEPTH, Side, explain};
use plannercraft_core::{
    BaselineNumber, CalendarSet, CompiledCalendar, DurationUnit, Minutes, Moment, Project, Schedule,
    TaskBaseline, TaskUid, Work,
};
use serde::Serialize;

use crate::command::params::{VarianceParams, WhatIfParams};
use crate::command::{Batch, TaskRef, resolve_task};
use crate::error::{CommandError, CommandErrorKind, codes};
use crate::explain::{ExplainView, explain_error, explain_view};
use crate::json::DurationJson;
use crate::outcome::SideEffect;
use crate::session::{ApplyOptions, Session};
use crate::text::{Language, TextContext};
use crate::view::{BaselineOut, DiagView, DiffView, ViewCtx, baseline_out, signed_minutes, work_out};
use crate::workspace::ToolError;

/// Tâches expliquées par un `what_if` au plus (R-234).
pub const MAX_WHAT_IF_EXPLAIN: usize = 5;
/// Lignes d'une page de `baseline_variance` par défaut (R-228).
pub const DEFAULT_VARIANCE_ROWS: usize = 50;
/// Lignes d'une page de `baseline_variance` au plus (C-MCP-03).
pub const MAX_VARIANCE_ROWS: usize = 500;

/// Résultat d'un `what_if` (R-234).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WhatIfView {
    /// Vrai si le lot simulé serait accepté.
    pub accepted: bool,
    /// Refus du lot, comme `apply_batch` les rendrait (absent si accepté).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ToolError>,
    /// Diff du planning, celui d'un `dry_run` du même lot (absent si
    /// refusé).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<DiffView>,
    /// Étiquettes `$x` des tâches que le lot créerait → UID simulé.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, u32>,
    /// Avertissements nouveaux du lot simulé.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<DiagView>,
    /// Effets de bord du lot simulé.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub side_effects: Vec<SideEffect>,
    /// Écart de fin du projet après le lot, à la ligne de base demandée
    /// (R-224 ; unité par défaut du projet) ; absent sans `baseline`, si
    /// aucune tâche n'a cette ligne, ou si le lot est refusé.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_finish_variance: Option<DurationJson>,
    /// Fin du projet dans cette ligne de base (ISO local).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_finish: Option<String>,
    /// Vrai si la ligne du projet est déduite des tâches de premier niveau
    /// (projet sans récapitulative stockée, R-228).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub baseline_derived: bool,
    /// Explications du début des tâches demandées, sur l'état simulé
    /// (R-230 à R-233).
    pub explanations: Vec<ExplainView>,
}

/// Page de `baseline_variance` (R-228).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct VarianceView {
    /// Numéro de la ligne de base.
    pub baseline: u8,
    /// Ligne du projet (toujours présente).
    pub project: VarianceRow,
    /// Tâches de la page : |écart de fin| décroissant, puis ID.
    pub tasks: Vec<VarianceRow>,
    /// UID à passer en `cursor` pour la page suivante ; absent à la fin.
    pub next_cursor: Option<u32>,
    /// Tâches retenues (toutes pages comprises).
    pub total: usize,
}

/// Ligne d'écarts d'une tâche ou du projet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct VarianceRow {
    /// UID (0 : projet).
    pub uid: u32,
    /// ID (ligne) ; absent pour le projet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    /// Nom.
    pub name: String,
    /// Début planifié (ISO local).
    pub start: Option<String>,
    /// Fin planifiée.
    pub finish: Option<String>,
    /// Durée (saisie d'une feuille, calculée d'une récapitulative).
    pub duration: DurationJson,
    /// Travail calculé (heures) ; absent quand nul.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work: Option<DurationJson>,
    /// Valeurs de la ligne de base ; `null` sans ligne.
    pub baseline: Option<BaselineOut>,
    /// Écarts ; absents sans ligne de base.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variance: Option<VarianceOut>,
    /// Vrai si la ligne du projet est déduite des tâches de premier niveau
    /// (projet sans récapitulative stockée).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub derived: bool,
}

/// Écarts signés (valeur − ligne de base), calculés par le moteur.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct VarianceOut {
    /// Écart de début (unité par défaut du projet).
    pub start: DurationJson,
    /// Écart de fin.
    pub finish: DurationJson,
    /// Écart de durée ; absent pour une ligne du projet déduite.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<DurationJson>,
    /// Écart de travail (heures) ; absent pour une ligne du projet déduite.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work: Option<DurationJson>,
}

/// Erreur `param.invalid` sur `field`.
fn invalid(field: &str, why: &str) -> CommandError {
    CommandError::new(
        codes::PARAM_INVALID,
        CommandErrorKind::InvalidParam(why.to_owned()),
    )
    .with_field(field)
}

/// Erreur `limit.exceeded` sur `field`.
fn limit(what: &'static str, max: usize, field: &str) -> CommandError {
    CommandError::new(
        codes::LIMIT_EXCEEDED,
        CommandErrorKind::Limit {
            what,
            max: u64::try_from(max).unwrap_or(u64::MAX),
        },
    )
    .with_field(field)
}

/// Numéro de ligne de base (0 à 10), sinon `param.invalid` sur `baseline`.
fn baseline_number(n: u8) -> Result<BaselineNumber, CommandError> {
    BaselineNumber::new(n).map_err(|_| invalid("baseline", "« baseline » : 0 à 10"))
}

/// Ligne de base du projet et ses écarts de dates.
struct ProjectLine {
    /// Valeurs de la ligne (stockée sur l'UID 0, ou déduite).
    baseline: TaskBaseline,
    /// Vrai si déduite des tâches de premier niveau.
    derived: bool,
    /// Écarts : ceux du moteur (ligne stockée) ; dates seules (déduite).
    variance: plannercraft_core::baseline::Variance,
}

/// Minutes signées de `a` à `b` sur le calendrier du projet (R-020), à
/// défaut minutes civiles (jamais de panique).
fn project_between(p: &Project, a: Moment, b: Moment) -> Minutes {
    CompiledCalendar::compile(&p.calendars, p.calendar)
        .ok()
        .and_then(|c| c.working_between(a, b).ok())
        .unwrap_or_else(|| a.minutes_until(b))
}

/// Ligne de base `number` du projet sur l'état (`p`, `s`) : celle de la
/// récapitulative stockée (UID 0) et ses écarts du moteur (R-224, R-225) ;
/// sans récapitulative stockée (projet créé par PlannerCraft, ADR-0014 lot L4),
/// déduite des tâches de premier niveau dotées de la ligne (début min, fin
/// max), écarts de début et de fin mesurés sur le calendrier du projet.
/// `None` si aucune ligne n'est trouvée.
fn project_line(p: &Project, s: &Schedule, set: &CalendarSet, number: BaselineNumber) -> Option<ProjectLine> {
    let v = plannercraft_core::baseline::variance(p, s, set, number);
    if let Some(baseline) = v.project.baseline {
        return Some(ProjectLine {
            baseline,
            derived: false,
            variance: v.project.variance,
        });
    }
    if p.summary_task.is_some() {
        return None;
    }
    let mut start: Option<Moment> = None;
    let mut finish: Option<Moment> = None;
    for t in p
        .tasks
        .iter()
        .filter(|t| !t.is_null && t.outline_level.get() == 1)
    {
        let Some(b) = t.baselines.get(&number) else {
            continue;
        };
        if let Some(bs) = b.start {
            start = Some(start.map_or(bs, |x| x.min(bs)));
        }
        if let Some(bf) = b.finish {
            finish = Some(finish.map_or(bf, |x| x.max(bf)));
        }
    }
    if start.is_none() && finish.is_none() {
        return None;
    }
    let (now_start, now_finish) = s
        .project_summary
        .as_ref()
        .map_or((s.project_start, s.project_finish), |ps| (ps.start, ps.finish));
    let date = |base: Option<Moment>, now: Moment| base.map_or(Minutes::ZERO, |b| project_between(p, b, now));
    Some(ProjectLine {
        baseline: TaskBaseline {
            start,
            finish,
            ..TaskBaseline::default()
        },
        derived: true,
        variance: plannercraft_core::baseline::Variance {
            start: date(start, now_start),
            finish: date(finish, now_finish),
            duration: Minutes::ZERO,
            work: Minutes::ZERO,
        },
    })
}

/// `analysis.what_if` (R-234) : simule `params.ops` sur une copie
/// (`Session::simulate`, même pipeline que `dry_run`) ; la session ne
/// change pas. Un lot refusé est rendu comme donnée (`accepted: false`),
/// message dans `lang` ; textes des durées dans la langue de la session.
///
/// # Errors
///
/// Paramètres seulement : `limit.exceeded` (plus de 5 tâches à
/// expliquer), `param.invalid` (`baseline` hors de 0 à 10),
/// `task.not_found`, `batch.unknown_label` ou `explain.not_scheduled`
/// (champ `explain`) pour une tâche à expliquer sur l'état simulé.
pub fn what_if(session: &Session, params: &WhatIfParams, lang: Language) -> Result<WhatIfView, CommandError> {
    if params.explain.len() > MAX_WHAT_IF_EXPLAIN {
        return Err(limit("explain", MAX_WHAT_IF_EXPLAIN, "explain"));
    }
    let number = params.baseline.map(baseline_number).transpose()?;
    let batch = Batch {
        format_version: Batch::FORMAT_VERSION,
        lang: None,
        ops: params.ops.clone(),
        label: None,
    };
    let options = ApplyOptions {
        detail: params.detail,
        ..ApplyOptions::default()
    };
    let sim = match session.simulate(&batch, options) {
        Ok(sim) => sim,
        Err(e) => {
            return Ok(WhatIfView {
                accepted: false,
                errors: vec![ToolError::from_command(&e, lang)],
                diff: None,
                labels: BTreeMap::new(),
                warnings: Vec::new(),
                side_effects: Vec::new(),
                project_finish_variance: None,
                baseline_finish: None,
                baseline_derived: false,
                explanations: Vec::new(),
            });
        }
    };
    let ctx = ViewCtx::new(session);
    let text = TextContext::for_project(&sim.project, session.lang());
    let line = number.and_then(|n| {
        let (set, _) = CalendarSet::compile(&sim.project);
        project_line(&sim.project, &sim.schedule, &set, n)
    });
    let unit = sim.project.options.default_duration_unit;
    let mut explanations = Vec::with_capacity(params.explain.len());
    for r in &params.explain {
        let uid = match r {
            TaskRef::Uid(u) if *u == TaskUid::PROJECT_SUMMARY => *u,
            r => resolve_task(r, &sim.outcome.labels, &sim.project, "explain")?,
        };
        let x = explain(&sim.project, &sim.schedule, uid, Side::Start, DEFAULT_DEPTH)
            .map_err(|e| explain_error(e).with_field("explain"))?;
        explanations.push(explain_view(&sim.project, &x, &text));
    }
    let o = &sim.outcome;
    Ok(WhatIfView {
        accepted: true,
        errors: Vec::new(),
        diff: Some(ctx.diff(&o.diff)),
        labels: o.labels.iter().map(|(k, v)| (k.clone(), v.get())).collect(),
        warnings: DiagView::list(&o.warnings),
        side_effects: o.side_effects.clone(),
        project_finish_variance: line
            .as_ref()
            .map(|l| signed_minutes(l.variance.finish, unit, &text)),
        baseline_finish: line
            .as_ref()
            .and_then(|l| l.baseline.finish)
            .map(|m| m.to_string()),
        baseline_derived: line.as_ref().is_some_and(|l| l.derived),
        explanations,
    })
}

/// Écarts en sortie : dates et durée dans l'unité `unit`, travail en
/// heures ; durée et travail omis si `dates_only`.
fn variance_out(
    v: &plannercraft_core::baseline::Variance,
    unit: DurationUnit,
    dates_only: bool,
    text: &TextContext,
) -> VarianceOut {
    VarianceOut {
        start: signed_minutes(v.start, unit, text),
        finish: signed_minutes(v.finish, unit, text),
        duration: (!dates_only).then(|| signed_minutes(v.duration, unit, text)),
        work: (!dates_only).then(|| signed_minutes(v.work, DurationUnit::Hour, text)),
    }
}

/// Vrai si un écart est non nul.
fn varied(v: &plannercraft_core::baseline::Variance) -> bool {
    [v.start, v.finish, v.duration, v.work]
        .iter()
        .any(|m| *m != Minutes::ZERO)
}

/// `analysis.baseline_variance` (R-228) : ligne du projet, puis tâches
/// planifiées triées par |écart de fin| décroissant puis ID (comme R-163),
/// filtrées (`only_varied` : tâches dotées de la ligne et d'un écart non
/// nul) et paginées (curseur : UID de la dernière ligne de la page
/// précédente, dans cet ordre).
///
/// # Errors
///
/// `param.invalid` (`baseline` hors de 0 à 10, `limit` 0),
/// `limit.exceeded` au-delà de [`MAX_VARIANCE_ROWS`], `task.not_found`
/// (champ `cursor`) si le curseur ne désigne aucune ligne retenue.
pub fn baseline_variance(session: &Session, params: &VarianceParams) -> Result<VarianceView, CommandError> {
    let number = baseline_number(params.baseline.unwrap_or(0))?;
    let page = match params.limit {
        None => DEFAULT_VARIANCE_ROWS,
        Some(0) => return Err(invalid("limit", "« limit » doit valoir au moins 1")),
        Some(n) => {
            let n = usize::try_from(n).unwrap_or(usize::MAX);
            if n > MAX_VARIANCE_ROWS {
                return Err(limit("limit", MAX_VARIANCE_ROWS, "limit"));
            }
            n
        }
    };
    let p = session.project();
    let s = session.schedule();
    let ctx = ViewCtx::new(session);
    let text = ctx.text();
    let unit = p.options.default_duration_unit;
    let (set, _) = CalendarSet::compile(p);
    let all = plannercraft_core::baseline::variance(p, s, &set, number);
    let by_uid: BTreeMap<TaskUid, &plannercraft_core::baseline::TaskVariance> =
        all.tasks.iter().map(|v| (v.uid, v)).collect();
    // J6-PERF-3 : tri sur la clé seule (|écart de fin|, ID), pagination,
    // puis vues des seules lignes de la page.
    let mut keys: Vec<(
        Minutes,
        usize,
        &plannercraft_core::Task,
        &plannercraft_core::baseline::TaskVariance,
    )> = Vec::new();
    for (pos, task) in ctx.rows() {
        let Some(v) = by_uid.get(&task.uid()) else {
            continue;
        };
        if params.only_varied && (v.baseline.is_none() || !varied(&v.variance)) {
            continue;
        }
        let key = if v.baseline.is_some() {
            v.variance.finish
        } else {
            Minutes::ZERO
        };
        keys.push((key, pos, task, v));
    }
    keys.sort_by(|a, b| {
        b.0.get()
            .unsigned_abs()
            .cmp(&a.0.get().unsigned_abs())
            .then(a.1.cmp(&b.1))
    });
    let total = keys.len();
    let skip = match params.cursor {
        None => 0,
        Some(c) => keys
            .iter()
            .position(|r| r.2.uid().get() == c)
            .and_then(|i| i.checked_add(1))
            .ok_or_else(|| {
                CommandError::new(
                    codes::TASK_NOT_FOUND,
                    CommandErrorKind::UnknownTask(TaskUid::new(c)),
                )
                .with_field("cursor")
            })?,
    };
    let rest = total.saturating_sub(skip);
    let tasks: Vec<VarianceRow> = keys
        .into_iter()
        .skip(skip)
        .take(page)
        .map(|(_, pos, task, v)| {
            let view = ctx.task_brief(pos, task);
            VarianceRow {
                uid: task.uid().get(),
                id: Some(view.id),
                name: view.name,
                start: view.start,
                finish: view.finish,
                duration: view.duration,
                work: view.work,
                baseline: v.baseline.as_ref().map(|b| baseline_out(b, text)),
                variance: v
                    .baseline
                    .is_some()
                    .then(|| variance_out(&v.variance, unit, false, text)),
                derived: false,
            }
        })
        .collect();
    let next_cursor = if rest > tasks.len() {
        tasks.last().map(|r| r.uid)
    } else {
        None
    };
    Ok(VarianceView {
        baseline: number.get(),
        project: project_row(session, &set, number, unit, text),
        tasks,
        next_cursor,
        total,
    })
}

/// Ligne du projet de `baseline_variance`.
fn project_row(
    session: &Session,
    set: &CalendarSet,
    number: BaselineNumber,
    unit: DurationUnit,
    text: &TextContext,
) -> VarianceRow {
    let p = session.project();
    let s = session.schedule();
    let line = project_line(p, s, set, number);
    let ps = s.project_summary.as_ref();
    let name = if p.info.name.is_empty() {
        session.origin().name.clone()
    } else {
        p.info.name.clone()
    };
    let duration = ps.map_or(Minutes::ZERO, |ps| ps.duration);
    let work = ps.map_or(Work::ZERO, |ps| ps.work);
    VarianceRow {
        uid: TaskUid::PROJECT_SUMMARY.get(),
        id: None,
        name,
        start: Some(ps.map_or(s.project_start, |ps| ps.start).to_string()),
        finish: Some(ps.map_or(s.project_finish, |ps| ps.finish).to_string()),
        duration: signed_minutes(duration, unit, text),
        work: (work != Work::ZERO).then(|| work_out(work, text)),
        baseline: line.as_ref().map(|l| baseline_out(&l.baseline, text)),
        variance: line
            .as_ref()
            .map(|l| variance_out(&l.variance, unit, l.derived, text)),
        derived: line.as_ref().is_some_and(|l| l.derived),
    }
}
