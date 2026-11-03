//! Commandes `progress.*` (J6, ADR-0014 D3 à D5 ; R-216, R-217, R-219).
//!
//! `progress.update` appelle `plannercraft_core::progress::edit::apply` avec les
//! dates planifiées de la session (planning courant du clone du lot), puis
//! écrit l'avancement, la durée et le travail des affectations ; la durée et
//! le travail planifié passent par l'équilibrage du cœur (R-189 à R-194),
//! comme `task.update`. `progress.reschedule` (« replanifier le travail
//! restant », S33 « Update Project ») pose une reprise sur les tâches en
//! cours et une SNET sur les tâches non commencées en retard ; il ne touche
//! jamais une date réelle. Les commandes ne calculent aucune date : le
//! recalcul du lot place (ADR-0014 D3).

use std::collections::BTreeSet;

use plannercraft_core::calendar::{self, CompiledCalendar, Span};
use plannercraft_core::progress::edit::{self, EditError, EditNote, Input, Part, apply};
use plannercraft_core::progress::facts;
use plannercraft_core::resource::Edit;
use plannercraft_core::schedule::measure_calendar_in;
use plannercraft_core::{
    Arena, Constraint, ConstraintKind, Diagnostic, Duration, Minutes, Moment, Progress, ProgressState,
    Project, ResourceKind, Schedule, SchedulingMode, Subject, Task, TaskOutcome, TaskUid, TimeBasis, Work,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::assignment::{Balance, balance_of};
use super::exec::Ctx;
use super::extras_invalidation::TaskChange;
use super::outline::is_summary_at;
use super::{DurationJson, TaskRef};
use crate::error::{CommandError, CommandErrorKind, codes};
use crate::json::{MomentSchema, PercentJson};
use crate::outcome::SideEffect;

/// `progress.update` (R-216) : avancement d'une feuille. Champs absents :
/// inchangés. Conflits refusés (`param.conflict`, R-202) : `percent_complete`
/// avec `actual_duration` ; `percent_complete` < 100 avec `actual_finish` ;
/// `actual_finish` avec `remaining_duration` > 0 ; `percent_work_complete`
/// avec `actual_work`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProgressUpdate {
    /// Tâche (feuille active).
    pub task: TaskRef,
    /// % achevé (durée) (R-201).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent_complete: Option<PercentJson>,
    /// Début réel (R-202).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<MomentSchema>")]
    pub actual_start: Option<Moment>,
    /// Fin réelle (R-202 : 100 %, durée mesurée).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<MomentSchema>")]
    pub actual_finish: Option<Moment>,
    /// Durée réelle (R-202).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_duration: Option<DurationJson>,
    /// Durée restante (R-202 : durée = réel + reste).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_duration: Option<DurationJson>,
    /// % travail achevé (R-211).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent_work_complete: Option<PercentJson>,
    /// Travail réel de la tâche, réparti sur ses affectations (R-211).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_work: Option<DurationJson>,
    /// % physique (R-212, sans effet sur les dates).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physical_percent_complete: Option<PercentJson>,
}

/// `progress.reschedule` (R-217) : replanifie le travail restant après
/// `after`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProgressReschedule {
    /// Date après laquelle le reste est replanifié (défaut : date d'état
    /// effective, R-219).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<MomentSchema>")]
    pub after: Option<Moment>,
    /// Tâches concernées (défaut : toutes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tasks: Option<Vec<TaskRef>>,
}

/// Saisie de `op`, dans l'ordre du contrat (l'ordre d'application est
/// celui de R-202, fixé par le cœur).
fn inputs_of(op: &ProgressUpdate, ctx: &Ctx<'_>) -> Result<Vec<Input>, CommandError> {
    let percent = |p: &PercentJson, field: &str| p.to_percent(ctx.text).map_err(|e| e.with_field(field));
    let mut inputs = Vec::new();
    if let Some(p) = &op.percent_complete {
        inputs.push(Input::Percent(percent(p, PERCENT)?));
    }
    if let Some(t) = op.actual_start {
        inputs.push(Input::ActualStart(t));
    }
    if let Some(t) = op.actual_finish {
        inputs.push(Input::ActualFinish(t));
    }
    if let Some(d) = &op.actual_duration {
        inputs.push(Input::ActualDuration(ctx.duration(d, ACTUAL_DURATION)?.minutes()));
    }
    if let Some(d) = &op.remaining_duration {
        inputs.push(Input::RemainingDuration(
            ctx.duration(d, REMAINING_DURATION)?.minutes(),
        ));
    }
    if let Some(p) = &op.percent_work_complete {
        inputs.push(Input::PercentWork(percent(p, PERCENT_WORK)?));
    }
    if let Some(d) = &op.actual_work {
        let d = ctx.duration(d, ACTUAL_WORK)?;
        if d.basis() == TimeBasis::Elapsed {
            return Err(invalid(
                ACTUAL_WORK,
                "travail en temps ouvré attendu (durée écoulée refusée)",
            ));
        }
        let w = Work::new(d.minutes()).map_err(|_| invalid(ACTUAL_WORK, "travail hors des bornes"))?;
        inputs.push(Input::ActualWork(w));
    }
    if let Some(p) = &op.physical_percent_complete {
        inputs.push(Input::Physical(percent(p, PHYSICAL)?));
    }
    Ok(inputs)
}

/// Champs de `progress.update`, comme les nomme le cœur (R-202).
const PERCENT: &str = "percent_complete";
const ACTUAL_FINISH: &str = "actual_finish";
const ACTUAL_DURATION: &str = "actual_duration";
const REMAINING_DURATION: &str = "remaining_duration";
const PERCENT_WORK: &str = "percent_work_complete";
const ACTUAL_WORK: &str = "actual_work";
const PHYSICAL: &str = "physical_percent_complete";

/// Refus de la saisie → erreur de commande (R-216), sujet la tâche.
fn edit_error(e: EditError, task: TaskUid) -> CommandError {
    let base = match e {
        EditError::SummaryNotEditable => CommandError::new(
            codes::PROGRESS_SUMMARY_NOT_EDITABLE,
            CommandErrorKind::ProgressSummaryNotEditable,
        )
        .with_field("task"),
        EditError::Inactive => {
            CommandError::new(codes::PROGRESS_INACTIVE, CommandErrorKind::ProgressInactive).with_field("task")
        }
        EditError::InvalidOrder => CommandError::new(
            codes::PROGRESS_INVALID_ORDER,
            CommandErrorKind::ProgressInvalidOrder,
        )
        .with_field(ACTUAL_FINISH),
        EditError::Conflict(a, b) => {
            // Le second nom est un champ saisi, sauf « assignments » (travail
            // saisi sur une tâche sans affectation de travail).
            let field = if b == "assignments" { a } else { b };
            CommandError::new(codes::PARAM_CONFLICT, CommandErrorKind::ParamConflict(a, b)).with_field(field)
        }
        EditError::Overflow => CommandError::new(
            codes::LIMIT_EXCEEDED,
            CommandErrorKind::Limit {
                what: "duration",
                max: u64::try_from(Duration::MAX_MINUTES).unwrap_or(u64::MAX),
            },
        ),
    };
    base.with_subject(Subject::Task(task))
}

/// Calendrier compilé qui mesure `task` (R-113, celui du moteur) ; repli
/// sur le Standard intégré quand le calendrier du projet est inutilisable
/// (R-066). Calendriers compilés une fois par lot tant qu'ils ne changent
/// pas (J6-PERF-2).
fn measure_of(ctx: &mut Ctx<'_>, task: &Task) -> Result<CompiledCalendar, CommandError> {
    let found = {
        let (project, set) = ctx.calendar_set();
        measure_calendar_in(project, set, task)
            .and_then(|uid| set.get(uid).ok())
            .cloned()
    };
    if let Some(cal) = found {
        return Ok(cal);
    }
    let project = &*ctx.project;
    Arena::try_from(vec![calendar::standard(project.calendar, "Standard".to_owned())])
        .ok()
        .and_then(|arena| CompiledCalendar::compile(&arena, project.calendar).ok())
        .ok_or_else(|| invalid("task", "aucun calendrier utilisable pour mesurer la tâche"))
}

/// Dates planifiées courantes de `uid` (R-201 : début réel implicite, fin
/// réelle à 100 %) : celles du planning ; une tâche exclue du calcul garde
/// son placement d'affichage, à défaut le début du projet.
fn scheduled_span(schedule: &Schedule, uid: TaskUid) -> Span {
    match schedule.get(uid) {
        Some(TaskOutcome::Scheduled(s)) => Span {
            start: s.start,
            finish: s.finish,
        },
        Some(TaskOutcome::Excluded {
            display: Some(span), ..
        }) => *span,
        _ => Span {
            start: schedule.project_start,
            finish: schedule.project_start,
        },
    }
}

/// Affectations de travail de `uid` vues par la saisie (R-211), triées par
/// UID : travail planifié et travail réel.
fn parts_of(project: &Project, uid: TaskUid) -> Vec<Part> {
    let mut parts: Vec<Part> = project
        .assignments
        .iter()
        .filter(|a| a.task == uid)
        .filter(|a| {
            a.resource
                .and_then(|r| project.resources.get(r))
                .is_some_and(|r| r.kind == ResourceKind::Work)
        })
        .map(|a| Part {
            uid: a.uid(),
            work: a.work,
            actual_work: a.actual_work,
        })
        .collect();
    parts.sort_unstable();
    parts
}

/// Copie de `task` à la durée `minutes` (unité, base et estimation
/// gardées) et à l'avancement `progress` : point de départ d'une saisie
/// rejouée après l'équilibrage.
fn restated(task: &Task, minutes: Minutes, progress: Progress) -> Result<Task, CommandError> {
    let d = &task.duration;
    let mut t = task.clone();
    t.duration = Duration::new(minutes, d.unit(), d.basis(), d.is_estimated())
        .map_err(|_| invalid("duration", "durée recalculée hors des bornes du modèle"))?;
    t.progress = progress;
    Ok(t)
}

/// Parts de la saisie sur le travail planifié de l'équilibrage `b` et le
/// travail réel de `done`.
fn rebased(b: &Balance, done: &[Part]) -> Vec<Part> {
    let mut parts: Vec<Part> = b
        .parts
        .iter()
        .map(|p| Part {
            uid: p.uid,
            work: p.work,
            actual_work: done
                .iter()
                .find(|d| d.uid == p.uid)
                .map_or(Work::ZERO, |d| d.actual_work),
        })
        .collect();
    parts.sort_unstable();
    parts
}

/// `progress.update` (R-216) : saisie équilibrée par le cœur
/// (`progress::edit::apply`, R-200 à R-202, R-211 à R-213), puis écrite.
///
/// - Durée changée par la saisie (durée réelle au-delà de D, reste saisi,
///   fin réelle mesurée) sur une feuille affectée : équilibrée comme
///   `task.update duration` (R-189 « durée »), puis la saisie est rejouée
///   sur le travail équilibré (le travail réel suit le nouveau travail).
/// - Travail réel au-delà du travail d'une affectation (W = AW, R-211) :
///   équilibré comme un travail d'affectation saisi (R-189 « travail ») ; si
///   la durée en change, l'avancement est recalculé sur la nouvelle durée
///   (tâche achevée : durée réelle = nouvelle durée, fin réelle déplacée).
/// - R-161 amendée : `Stop`, `Resume`, `ResumeValid` de la tâche jetés si
///   son avancement change ; lignes `TimephasedData` 2-3 d'une affectation
///   jetées si son travail réel change.
/// - Remarques du cœur → `progress.milestone_rounded`,
///   `progress.option_unsupported`.
pub(super) fn update(op: &ProgressUpdate, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let uid = ctx.task(&op.task, "task")?;
    let inputs = inputs_of(op, ctx)?;
    let tasks = &ctx.project.tasks;
    let pos = tasks.position(uid).ok_or_else(|| not_found(uid))?;
    let summary = is_summary_at(tasks, pos);
    let task = tasks.at(pos).ok_or_else(|| not_found(uid))?.clone();
    let measure = measure_of(ctx, &task)?;
    ctx.ensure_schedule()?;
    let scheduled = scheduled_span(ctx.schedule(), uid);
    let ectx = edit::Ctx {
        scheduled,
        measure: &measure,
        status_date: ctx.project.effective_status_date(),
        options: ctx.project.options.progress,
        summary,
    };
    let fail = |e| edit_error(e, uid);
    let parts = parts_of(ctx.project, uid);
    let mut e = apply(&task, &parts, &inputs, &ectx).map_err(fail)?;
    let mut notes = e.notes.clone();
    let d0 = task.duration.minutes();
    let mut balance = None;
    if !parts.is_empty() {
        let mut b = balance_of(ctx, uid, &BTreeSet::new())?;
        if e.duration != d0 {
            b.edit(&Edit::Duration(e.duration), "duration")?;
            let t1 = restated(&task, e.duration, e.progress)?;
            e = apply(&t1, &rebased(&b, &e.parts), &inputs, &ectx).map_err(fail)?;
            notes.extend(e.notes.iter().copied());
        }
        for p in &e.parts {
            if b.part(p.uid).is_some_and(|bp| bp.work != p.work) {
                b.edit(
                    &Edit::AssignmentWork {
                        uid: p.uid,
                        work: p.work,
                    },
                    ACTUAL_WORK,
                )?;
            }
        }
        if b.side.duration != e.duration {
            // Une tâche achevée garde réel = D : sa durée réelle suit (et sa
            // fin réelle avec elle) ; en cours, seuls le reste et le % changent.
            let t2 = restated(&task, b.side.duration, e.progress)?;
            let again = if e.progress.actual_finish.is_some() {
                vec![Input::ActualDuration(b.side.duration)]
            } else {
                Vec::new()
            };
            e = apply(&t2, &rebased(&b, &e.parts), &again, &ectx).map_err(fail)?;
        }
        balance = Some(b);
    }

    // Écriture : durée et travail planifié, puis avancement.
    if let Some(b) = &balance {
        b.commit(ctx)?;
    } else if e.duration != d0 {
        let t = ctx.project.tasks.get_mut(uid).ok_or_else(|| not_found(uid))?;
        let d = &t.duration;
        t.duration = Duration::new(e.duration, d.unit(), d.basis(), d.is_estimated())
            .map_err(|_| invalid("duration", "durée recalculée hors des bornes du modèle"))?;
        ctx.drop_task_extras(uid, TaskChange::Duration);
        ctx.drop_assignment_extras(uid);
    }
    let t = ctx.project.tasks.get_mut(uid).ok_or_else(|| not_found(uid))?;
    let progress_changed = t.progress != e.progress;
    t.progress = e.progress;
    if progress_changed {
        ctx.drop_task_extras(uid, TaskChange::Progress);
    }
    for p in &e.parts {
        let Some(a) = ctx.project.assignments.get_mut(p.uid) else {
            continue;
        };
        let changed = a.actual_work != p.actual_work;
        a.actual_work = p.actual_work;
        // Reste dérivé (ADR-0014 D4) : stocké cohérent pour les vues.
        a.remaining_work = plannercraft_core::progress::remaining_work(a);
        if changed {
            ctx.touched.insert(Subject::Assignment(p.uid), ctx.op_index);
            ctx.drop_actual_timephased(p.uid);
        }
    }
    ctx.touched.insert(Subject::Task(uid), ctx.op_index);
    notes.sort_unstable();
    notes.dedup();
    for n in notes {
        ctx.notes.push(match n {
            EditNote::MilestoneRounded => Diagnostic::ProgressMilestoneRounded { task: uid },
            EditNote::OptionUnsupported(option) => Diagnostic::ProgressOptionUnsupported { option },
        });
    }
    Ok(())
}

/// `task.not_found` pour `uid`.
fn not_found(uid: TaskUid) -> CommandError {
    CommandError::new(codes::TASK_NOT_FOUND, CommandErrorKind::UnknownTask(uid))
        .with_subject(Subject::Task(uid))
}

/// `param.invalid` sur `field`.
fn invalid(field: &str, why: &str) -> CommandError {
    CommandError::new(
        codes::PARAM_INVALID,
        CommandErrorKind::InvalidParam(why.to_owned()),
    )
    .with_field(field)
}

/// Ce que `progress.reschedule` fait d'une tâche (R-217).
enum Move {
    /// Reprise posée (tâche en cours).
    Resume,
    /// SNET posée (tâche non commencée).
    Snet,
    /// Laissée telle quelle, avec un effet (`reason`).
    Skip(&'static str),
}

/// `progress.reschedule` (R-217) : replanifie le travail restant après
/// `after` (défaut : date d'état effective, R-219), sur `tasks` ou sur
/// toutes les feuilles actives automatiques.
///
/// - Feuille en cours dont la fin de la partie réelle (`Stop` du planning)
///   précède `after` : reprise = `after` (une reprise saisie plus tardive
///   est gardée) ; le moteur la recale sur l'instant ouvré suivant (R-204).
/// - Feuille non commencée dont le début planifié précède `after` : SNET
///   `after` si sa contrainte est ASAP ou une SNET plus tôt ; toute autre
///   contrainte ⇒ sautée, `RescheduleSkipped { reason: "constraint" }`.
/// - Tâche manuelle qui serait déplacée : sautée (`reason: "manual"`, le
///   moteur ne la replace pas, ADR-0005).
/// - Achevées, récapitulatives, inactives, lignes vides : jamais modifiées ;
///   les dates réelles ne changent jamais.
pub(super) fn reschedule(op: &ProgressReschedule, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let after = op.after.unwrap_or_else(|| ctx.project.effective_status_date());
    let selected = match &op.tasks {
        None => None,
        Some(refs) => {
            let mut set = BTreeSet::new();
            for r in refs {
                set.insert(ctx.task(r, "tasks")?);
            }
            Some(set)
        }
    };
    ctx.ensure_schedule()?;
    let schedule = ctx.schedule();
    let tasks = &ctx.project.tasks;
    let mut moves = Vec::new();
    for (pos, t) in tasks.iter().enumerate() {
        let uid = t.uid();
        if t.is_null
            || !t.active
            || is_summary_at(tasks, pos)
            || selected.as_ref().is_some_and(|s| !s.contains(&uid))
        {
            continue;
        }
        let Some(TaskOutcome::Scheduled(s)) = schedule.get(uid) else {
            continue;
        };
        let manual = matches!(t.mode, SchedulingMode::Manual { .. });
        let action = match facts(t).state {
            ProgressState::Complete => None,
            ProgressState::Started => {
                let stop = s.progress.as_ref().map_or(s.start, |p| p.stop);
                let kept = t.progress.resume.is_some_and(|r| r >= after);
                (stop < after && !kept).then_some(Move::Resume)
            }
            ProgressState::NotStarted => (s.start < after).then(|| match t.constraint {
                Constraint::AsSoonAsPossible => Move::Snet,
                Constraint::StartNoEarlierThan { date } if date < after => Move::Snet,
                _ => Move::Skip("constraint"),
            }),
        };
        let action = match action {
            Some(Move::Resume | Move::Snet) if manual => Some(Move::Skip("manual")),
            a => a,
        };
        if let Some(a) = action {
            moves.push((uid, a));
        }
    }
    for (uid, action) in moves {
        let t = ctx.project.tasks.get_mut(uid).ok_or_else(|| not_found(uid))?;
        match action {
            Move::Resume => {
                t.progress.resume = Some(after);
                ctx.drop_task_extras(uid, TaskChange::Progress);
            }
            Move::Snet => {
                t.constraint = Constraint::StartNoEarlierThan { date: after };
                ctx.effects.push(SideEffect::ConstraintSet {
                    uid,
                    kind: ConstraintKind::StartNoEarlierThan,
                });
                ctx.drop_task_extras(uid, TaskChange::Constraint);
            }
            Move::Skip(reason) => {
                ctx.effects
                    .push(SideEffect::RescheduleSkipped { task: uid, reason });
                continue;
            }
        }
        ctx.touched.insert(Subject::Task(uid), ctx.op_index);
    }
    ctx.touched.insert(Subject::Project, ctx.op_index);
    Ok(())
}
