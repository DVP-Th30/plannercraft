//! Exécution des commandes de tâches (R-152 à R-155, R-157, R-158, R-161 ;
//! lot L4) ; `task.update` étendu au type, au pilotage par l'effort et au
//! travail (R-197, lot J5 L7).
//!
//! Chaque commande travaille sur le clone du lot (`Ctx::project`) : un
//! refus interrompt le lot sans rien laisser (le pipeline jette le clone).
//! Les contrôles précèdent toujours les modifications de la commande. Les
//! changements d'ordre passent par `Arena::splice` (un bloc, une passe), sans
//! insertion élément par élément.

use std::collections::BTreeSet;

use plannercraft_core::calendar::{CompiledCalendar, TimeScale, builtin};
use plannercraft_core::resource::Edit;
use plannercraft_core::resource::edit::effective_effort_driven;
use plannercraft_core::schedule::{TaskOutcome, TaskRole, measure_calendar_in};
use plannercraft_core::{
    Arena, Assignment, CalendarError, Constraint, Duration, Minutes, ModelError, Moment, OutlineLevel,
    ScheduleAnchor, SchedulingMode, Subject, Task, TaskType, TaskUid, TimeBasis, Work,
};

use super::assignment::{Balance, balance_of};
use super::custom;
use super::exec::Ctx;
use super::extras_invalidation::TaskChange;
use super::outline::{
    MAX_LEVEL, block_end, level_at, level_limit, order, prev_non_null, renumbered, summaries,
};
use super::{ModeJson, Patch, Placement, TaskAdd, TaskDelete, TaskMove, TaskOne, TaskRef, TaskUpdate};
use crate::error::{CommandError, CommandErrorKind, codes};
use crate::outcome::SideEffect;

/// Longueur maximale d'un nom de tâche, en caractères (`Task/Name` MSPDI).
pub(crate) const MAX_NAME_CHARS: usize = 512;

/// `param.invalid` sur `field`, avec une explication.
fn invalid(field: &str, why: &str) -> CommandError {
    CommandError::new(
        codes::PARAM_INVALID,
        CommandErrorKind::InvalidParam(why.to_owned()),
    )
    .with_field(field)
}

/// Erreur du modèle (jamais attendue : les contrôles la précèdent).
fn model(e: &ModelError) -> CommandError {
    CommandError::new(
        codes::PARAM_INVALID,
        CommandErrorKind::InvalidParam(e.to_string()),
    )
}

/// Nom de tâche : au plus [`MAX_NAME_CHARS`] caractères ; vide accepté
/// (R-152).
fn check_name(name: &str) -> Result<(), CommandError> {
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(invalid("name", "nom de plus de 512 caractères"));
    }
    Ok(())
}

/// Niveau `level` → `OutlineLevel`.
fn outline(level: u16) -> Result<OutlineLevel, CommandError> {
    OutlineLevel::new(level).map_err(|e| model(&e))
}

/// Niveau d'un enfant de `level` (R-153 : au plus 65 535).
fn child_level(level: u16) -> Result<u16, CommandError> {
    level.checked_add(1).ok_or_else(level_limit)
}

/// Position d'une tâche existante du clone.
fn position(ctx: &Ctx<'_>, uid: TaskUid, field: &str) -> Result<usize, CommandError> {
    ctx.project.tasks.position(uid).ok_or_else(|| {
        CommandError::new(codes::TASK_NOT_FOUND, CommandErrorKind::UnknownTask(uid))
            .with_field(field)
            .with_subject(Subject::Task(uid))
    })
}

/// Marque `uid` comme touchée par l'opération courante (R-160).
fn touch(ctx: &mut Ctx<'_>, uid: TaskUid) {
    ctx.touched.insert(Subject::Task(uid), ctx.op_index);
}

/// Place d'insertion : position dans l'ordre courant et niveau.
#[derive(Clone, Copy, Debug)]
struct Slot {
    index: usize,
    level: u16,
}

/// R-152, R-154 : place désignée par `at` dans l'ordre courant, et la tâche
/// de référence du placement (aucune pour `End`).
fn slot(ctx: &Ctx<'_>, at: &Placement, field: &str) -> Result<(Slot, Option<TaskUid>), CommandError> {
    let tasks = &ctx.project.tasks;
    let target = |r: &TaskRef| -> Result<(TaskUid, usize, u16), CommandError> {
        let uid = ctx.task(r, field)?;
        let pos = position(ctx, uid, field)?;
        Ok((uid, pos, level_at(tasks, pos)))
    };
    Ok(match at {
        Placement::End => (
            Slot {
                index: tasks.len(),
                level: 1,
            },
            None,
        ),
        Placement::After(r) => {
            let (uid, pos, level) = target(r)?;
            (
                Slot {
                    index: block_end(tasks, pos),
                    level,
                },
                Some(uid),
            )
        }
        Placement::Before(r) => {
            let (uid, pos, level) = target(r)?;
            (Slot { index: pos, level }, Some(uid))
        }
        Placement::FirstChildOf(r) => {
            let (uid, pos, level) = target(r)?;
            (
                Slot {
                    index: pos.saturating_add(1),
                    level: child_level(level)?,
                },
                Some(uid),
            )
        }
        Placement::LastChildOf(r) => {
            let (uid, pos, level) = target(r)?;
            (
                Slot {
                    index: block_end(tasks, pos),
                    level: child_level(level)?,
                },
                Some(uid),
            )
        }
    })
}

/// Côté d'une date saisie (R-158).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Start,
    Finish,
}

/// R-158 : contrainte posée par une date saisie. Depuis le début : début →
/// SNET, fin → FNET ; à rebours : SNLT, FNLT. Date effacée : contrainte par
/// défaut du sens (ASAP, ALAP à rebours, R-108).
fn entered_constraint(anchor: &ScheduleAnchor, side: Side, date: Option<Moment>) -> Constraint {
    let backward = matches!(anchor, ScheduleAnchor::FromFinish(_));
    match (date, side, backward) {
        (None, _, _) => Constraint::default_for(anchor),
        (Some(date), Side::Start, false) => Constraint::StartNoEarlierThan { date },
        (Some(date), Side::Finish, false) => Constraint::FinishNoEarlierThan { date },
        (Some(date), Side::Start, true) => Constraint::StartNoLaterThan { date },
        (Some(date), Side::Finish, true) => Constraint::FinishNoLaterThan { date },
    }
}

/// R-158 : date saisie d'une opération, au plus une (début ou fin) :
/// `None` si aucune, sinon le côté et la date (`None` : effacée).
fn entered_date(
    start: &Patch<Moment>,
    finish: &Patch<Moment>,
) -> Result<Option<(Side, Option<Moment>)>, CommandError> {
    let side = |p: &Patch<Moment>, s: Side| match p {
        Patch::Keep => None,
        Patch::Set(d) => Some((s, Some(*d))),
        Patch::Clear => Some((s, None)),
    };
    match (side(start, Side::Start), side(finish, Side::Finish)) {
        (Some(_), Some(_)) => Err(invalid(
            "finish",
            "un seul de « start » ou « finish » par opération (chacun pose la contrainte de la tâche)",
        )),
        (s, f) => Ok(s.or(f)),
    }
}

/// Pose la contrainte d'une date saisie et signale l'effet (R-158) ; vrai
/// si la contrainte a changé.
fn set_entered(ctx: &mut Ctx<'_>, uid: TaskUid, side: Side, date: Option<Moment>) -> bool {
    let constraint = entered_constraint(&ctx.project.anchor, side, date);
    let Some(t) = ctx.project.tasks.get_mut(uid) else {
        return false;
    };
    let changed = t.constraint != constraint;
    t.constraint = constraint;
    ctx.effects.push(SideEffect::ConstraintSet {
        uid,
        kind: constraint.kind(),
    });
    changed
}

/// R-161 : hiérarchie changée. Jette les `Extras` de planning des tâches
/// `moved` et de celles qui deviennent récapitulatives ou cessent de
/// l'être (`before` : récapitulatives d'avant la commande) ; les marque
/// touchées (R-160).
fn hierarchy_changed(ctx: &mut Ctx<'_>, before: &BTreeSet<TaskUid>, moved: &[TaskUid]) {
    let after = summaries(&ctx.project.tasks);
    let mut hit: BTreeSet<TaskUid> = moved.iter().copied().collect();
    hit.extend(
        before
            .symmetric_difference(&after)
            .copied()
            .filter(|u| ctx.project.tasks.contains(*u)),
    );
    for uid in hit {
        touch(ctx, uid);
        ctx.drop_task_extras(uid, TaskChange::Outline);
    }
}

/// Insère `items` à la position `index` (un bloc, une passe).
fn insert_block(ctx: &mut Ctx<'_>, index: usize, items: Vec<Task>) -> Result<(), CommandError> {
    ctx.project
        .tasks
        .splice(index..index, items)
        .map(drop)
        .map_err(|e| model(&e))
}

/// R-152 : `task.add`.
pub(super) fn add(op: &TaskAdd, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    check_name(&op.name)?;
    let at = op.at.clone().unwrap_or_default();
    let (slot, _) = slot(ctx, &at, "at")?;
    let duration = op
        .duration
        .as_ref()
        .map(|d| ctx.duration(d, "duration"))
        .transpose()?;
    let entered = entered_date(
        &op.start.map_or(Patch::Keep, Patch::Set),
        &op.finish.map_or(Patch::Keep, Patch::Set),
    )?;
    let uid = ctx.uids.alloc()?;
    if let Some(label) = &op.label {
        ctx.define_label(label, uid)?;
    }
    let mut task = Task::new(uid, op.name.clone(), &ctx.project.options);
    // R-193, R-197 : une tâche créée à travail fixe est pilotée par l'effort,
    // même si l'option du projet ne l'est pas.
    task.effort_driven = effective_effort_driven(task.task_type, task.effort_driven);
    task.outline_level = outline(slot.level)?;
    task.constraint = Constraint::default_for(&ctx.project.anchor);
    if let Some(d) = duration {
        task.duration = d;
    }
    // R-157 : durée nulle ⇒ jalon ; un `milestone` explicite l'emporte.
    task.milestone = op.milestone.unwrap_or(task.duration.is_zero());
    if let Some(notes) = &op.notes {
        task.notes.clone_from(notes);
    }
    let before_order = order(&ctx.project.tasks);
    let before_summaries = summaries(&ctx.project.tasks);
    insert_block(ctx, slot.index, vec![task])?;
    ctx.created.push(uid);
    touch(ctx, uid);
    if let Some((side, date)) = entered {
        set_entered(ctx, uid, side, date);
    }
    // Le parent qui reçoit son premier enfant devient récapitulative.
    hierarchy_changed(ctx, &before_summaries, &[]);
    if let Some(e) = renumbered(&before_order, &ctx.project.tasks) {
        ctx.effects.push(e);
    }
    Ok(())
}

/// R-157 : refus d'une date ou d'une durée sur une récapitulative, dont le
/// moteur les calcule. Depuis J8 (R-254), une feuille manuelle accepte ces
/// trois champs (valeurs manuelles, [`manual_entry`]).
fn check_computed(op: &TaskUpdate, uid: TaskUid, summary: bool) -> Result<(), CommandError> {
    let fields = [
        ("duration", !op.duration.is_keep()),
        ("start", !op.start.is_keep()),
        ("finish", !op.finish.is_keep()),
    ];
    for (field, present) in fields {
        if present && summary {
            return Err(
                CommandError::new(codes::TASK_COMPUTED_FIELD, CommandErrorKind::ComputedField)
                    .with_field(field)
                    .with_subject(Subject::Task(uid)),
            );
        }
    }
    Ok(())
}

/// Durée saisie par `task.update` (R-157), contrôlée par
/// [`check_not_below_actual`].
fn entered_duration(
    op: &TaskUpdate,
    ctx: &Ctx<'_>,
    uid: TaskUid,
) -> Result<Option<plannercraft_core::Duration>, CommandError> {
    let d = match &op.duration {
        Patch::Keep => return Ok(None),
        Patch::Set(d) => ctx.duration(d, "duration")?,
        Patch::Clear => return Err(invalid("duration", "durée attendue (la durée ne s'efface pas)")),
    };
    check_not_below_actual(ctx, uid, d, "duration")?;
    Ok(Some(d))
}

/// R-213 (arbitrage du 2026-10-20) : une durée sous la durée réelle
/// stockée de `uid` laisserait un avancement incohérent (réel > D) :
/// refusée, `progress.duration_below_actual` sur `field`.
fn check_not_below_actual(
    ctx: &Ctx<'_>,
    uid: TaskUid,
    d: plannercraft_core::Duration,
    field: &str,
) -> Result<(), CommandError> {
    let actual = ctx
        .project
        .tasks
        .get(uid)
        .map_or(plannercraft_core::Minutes::ZERO, |t| t.progress.actual_duration);
    if actual > plannercraft_core::Minutes::ZERO && d.minutes() < actual {
        return Err(CommandError::new(
            codes::PROGRESS_DURATION_BELOW_ACTUAL,
            CommandErrorKind::DurationBelowActual {
                actual: actual.get(),
                entered: d.minutes().get(),
            },
        )
        .with_field(field)
        .with_subject(Subject::Task(uid)));
    }
    Ok(())
}

/// Calcule sur l'échelle de mesure de `uid` (R-113 : calendrier propre
/// utilisable, sinon celui du projet ou sa base, sinon le Standard intégré
/// de R-066 ; temps civil pour une durée écoulée, R-067) ; `elapsed` : la
/// durée en jeu est écoulée.
pub(super) fn on_task_scale<R>(
    ctx: &mut Ctx<'_>,
    uid: TaskUid,
    elapsed: bool,
    field: &str,
    f: impl FnOnce(TimeScale<'_>) -> Result<R, CalendarError>,
) -> Result<R, CommandError> {
    let out_of_domain = |_: CalendarError| invalid(field, "date hors du domaine du calendrier de la tâche");
    if elapsed {
        return f(TimeScale::Elapsed).map_err(out_of_domain);
    }
    let (project, set) = ctx.calendar_set();
    let own = project
        .tasks
        .get(uid)
        .and_then(|t| measure_calendar_in(project, set, t))
        .and_then(|c| set.get(c).ok());
    if let Some(c) = own {
        return f(TimeScale::Working(c)).map_err(out_of_domain);
    }
    let standard = builtin::standard(project.calendar, "Standard".to_owned());
    let arena = Arena::try_from(vec![standard]).map_err(|e| model(&e))?;
    let c = CompiledCalendar::compile(&arena, project.calendar).map_err(out_of_domain)?;
    f(TimeScale::Working(&c)).map_err(out_of_domain)
}

/// Vrai si `op` ne fait que passer la tâche en manuel (R-253) : aucune
/// autre valeur. Une suite de tels passages lit un seul planning
/// (`Ctx::freeze_schedule`) : figer une tâche à ses dates courantes ne
/// déplace aucune autre tâche (relecture finale de J8 : « Mode » sur
/// toute la sélection d'un grand projet ne recalcule plus une fois par
/// tâche).
pub(super) fn is_pure_freeze(op: &TaskUpdate) -> bool {
    op.mode == Patch::Set(ModeJson::Manual)
        && op.name.is_keep()
        && op.duration.is_keep()
        && op.milestone.is_keep()
        && op.notes.is_keep()
        && op.start.is_keep()
        && op.finish.is_keep()
        && op.active.is_keep()
        && op.task_type.is_keep()
        && op.effort_driven.is_keep()
        && op.work.is_keep()
        && op.custom.is_empty()
}

/// R-253 : valeurs manuelles d'une tâche qui passe en manuel : ses dates
/// planifiées courantes (planning du clone, O49 : `ManualStart` = `Start`,
/// `ManualFinish` = `Finish`) et sa durée ; tâche exclue : son placement
/// d'affichage (R-062) s'il existe, sinon aucune date (le moteur la signale
/// `task.manual_undated`). `pure` : passage en manuel seul
/// ([`is_pure_freeze`]) : planning de référence de la suite s'il existe,
/// sinon le planning courant, gardé comme référence.
fn frozen_manual(ctx: &mut Ctx<'_>, uid: TaskUid, pure: bool) -> Result<SchedulingMode, CommandError> {
    let Some(d) = ctx.project.tasks.get(uid).map(|t| t.duration) else {
        return Err(CommandError::new(
            codes::TASK_NOT_FOUND,
            CommandErrorKind::UnknownTask(uid),
        ));
    };
    if pure && let Some(s) = ctx.freeze_schedule() {
        return Ok(freeze_values(d, s.get(uid)));
    }
    ctx.ensure_schedule()?;
    if pure {
        ctx.keep_freeze_schedule();
    }
    Ok(freeze_values(d, ctx.schedule().get(uid)))
}

/// Valeurs manuelles figées d'une tâche de durée `d` selon son résultat
/// du planning (R-253).
fn freeze_values(d: Duration, outcome: Option<&TaskOutcome>) -> SchedulingMode {
    match outcome {
        Some(TaskOutcome::Scheduled(s)) => {
            // Récapitulative : durée calculée, ouvrée (R-054).
            let duration = if s.role == TaskRole::Summary {
                Duration::new(s.duration, d.unit(), TimeBasis::Working, false).unwrap_or(d)
            } else {
                d
            };
            SchedulingMode::Manual {
                start: Some(s.start),
                finish: Some(s.finish),
                duration: Some(duration),
            }
        }
        Some(TaskOutcome::Excluded {
            display: Some(span), ..
        }) => SchedulingMode::Manual {
            start: Some(span.start),
            finish: Some(span.finish),
            duration: Some(d),
        },
        _ => SchedulingMode::Manual {
            start: None,
            finish: None,
            duration: Some(d),
        },
    }
}

/// Début planifié de `uid` dans le planning du clone, s'il est planifié.
fn planned_start(ctx: &mut Ctx<'_>, uid: TaskUid) -> Result<Option<Moment>, CommandError> {
    ctx.ensure_schedule()?;
    Ok(ctx.schedule().scheduled(uid).map(|s| s.start))
}

/// R-254 : début, fin ou durée saisis sur une feuille manuelle (`base` :
/// ses valeurs manuelles, figées par R-253 si elle vient d'y passer) ;
/// valeurs manuelles nouvelles et durée qui en résulte, aucune contrainte
/// posée. Début ⇒ fin = début + durée sur l'échelle de la tâche ; fin ⇒
/// durée = temps du début à la fin (fin avant début :
/// `task.manual_finish_before_start`) ; durée ⇒ fin = début + durée. Sans
/// début saisi, le début planifié sert ; sans aucun début connu, seules la
/// durée et la fin saisies sont écrites. `None` si aucun des trois champs.
fn manual_entry(
    op: &TaskUpdate,
    ctx: &mut Ctx<'_>,
    uid: TaskUid,
    base: SchedulingMode,
) -> Result<Option<(SchedulingMode, Duration)>, CommandError> {
    let entered = entered_date(&op.start, &op.finish)?;
    if entered.is_none() && op.duration.is_keep() {
        return Ok(None);
    }
    if !op.duration.is_keep() && matches!(entered, Some((Side::Finish, _))) {
        return Err(conflict("duration", "finish"));
    }
    let Some(task) = ctx.project.tasks.get(uid) else {
        return Err(CommandError::new(
            codes::TASK_NOT_FOUND,
            CommandErrorKind::UnknownTask(uid),
        ));
    };
    let task_duration = task.duration;
    let SchedulingMode::Manual {
        start: base_start,
        finish: base_finish,
        duration: base_duration,
    } = base
    else {
        return Ok(None);
    };
    let duration = match entered_duration(op, ctx, uid)? {
        Some(d) => d,
        None => base_duration.unwrap_or(task_duration),
    };
    let elapsed = duration.basis() == TimeBasis::Elapsed;
    let manual = |start, finish, duration| SchedulingMode::Manual {
        start,
        finish,
        duration: Some(duration),
    };
    match entered {
        Some((side, None)) => {
            let field = if side == Side::Start { "start" } else { "finish" };
            Err(invalid(
                field,
                "date attendue : une tâche manuelle garde ses dates (repasser en automatique pour les libérer)",
            ))
        }
        Some((Side::Start, Some(start))) => {
            let finish = on_task_scale(ctx, uid, elapsed, "start", |sc| sc.add(start, duration.minutes()))?;
            Ok(Some((manual(Some(start), Some(finish), duration), duration)))
        }
        Some((Side::Finish, Some(finish))) => {
            let Some(start) = base_start.map_or_else(|| planned_start(ctx, uid), |s| Ok(Some(s)))? else {
                return Ok(Some((manual(None, Some(finish), duration), duration)));
            };
            if finish < start {
                return Err(CommandError::new(
                    codes::TASK_MANUAL_FINISH_BEFORE_START,
                    CommandErrorKind::ManualFinishBeforeStart { start, finish },
                )
                .with_field("finish")
                .with_subject(Subject::Task(uid)));
            }
            let minutes = on_task_scale(ctx, uid, elapsed, "finish", |sc| sc.between(start, finish))?;
            let measured = Duration::new(
                minutes.max(Minutes::ZERO),
                duration.unit(),
                duration.basis(),
                false,
            )
            .map_err(|_| invalid("finish", "durée hors des bornes"))?;
            check_not_below_actual(ctx, uid, measured, "finish")?;
            Ok(Some((manual(Some(start), Some(finish), measured), measured)))
        }
        None => {
            let Some(start) = base_start.map_or_else(|| planned_start(ctx, uid), |s| Ok(Some(s)))? else {
                return Ok(Some((manual(None, base_finish, duration), duration)));
            };
            let finish = on_task_scale(ctx, uid, elapsed, "duration", |sc| {
                sc.add(start, duration.minutes())
            })?;
            Ok(Some((manual(Some(start), Some(finish), duration), duration)))
        }
    }
}

/// Contrôles du nom et du drapeau actif d'un `task.update`.
fn check_name_active(op: &TaskUpdate) -> Result<(), CommandError> {
    match &op.name {
        Patch::Clear => return Err(invalid("name", "le nom ne s'efface pas (chaîne vide acceptée)")),
        Patch::Set(n) => check_name(n)?,
        Patch::Keep => {}
    }
    if op.active == Patch::Clear {
        return Err(invalid("active", "booléen attendu"));
    }
    Ok(())
}

/// Mode, durée et date saisie d'un `task.update` (R-157, R-158, R-253,
/// R-254), contrôlés avant toute modification.
struct Planned {
    /// Mode nouveau (R-253) ou valeurs manuelles nouvelles (R-254).
    mode: Option<SchedulingMode>,
    /// Durée à poser sur la tâche.
    duration: Option<Duration>,
    /// Date saisie d'une tâche automatique (contrainte, R-158).
    entered: Option<(Side, Option<Moment>)>,
}

/// Prépare un `task.update` : mode (R-253), puis dates et durée, valeurs
/// manuelles d'une feuille manuelle (R-254) ou contrainte d'une tâche
/// automatique (R-158) ; `manual` : la tâche est manuelle avant
/// l'opération.
fn plan_dates(
    op: &TaskUpdate,
    ctx: &mut Ctx<'_>,
    uid: TaskUid,
    summary: bool,
    manual: bool,
) -> Result<Planned, CommandError> {
    let target = match op.mode {
        Patch::Keep => None,
        Patch::Set(m) => Some(m),
        Patch::Clear => {
            return Err(invalid(
                "mode",
                "mode attendu (« auto » ou « manual » ; il ne s'efface pas)",
            ));
        }
    };
    check_computed(op, uid, summary)?;
    // R-253 : passage en manuel ⇒ dates planifiées courantes figées ;
    // retour en automatique ⇒ valeurs manuelles retirées (contrainte et
    // échéance gardées, recalcul par le moteur).
    let new_mode = match target {
        // Feuille dont l'opération donne le début (collage R-244) : les
        // dates figées seraient aussitôt remplacées (R-254 part du début
        // saisi et de la durée de la tâche) ; pas de recalcul du planning
        // (relecture finale de J8).
        Some(ModeJson::Manual) if !manual && !summary && matches!(op.start, Patch::Set(_)) => {
            let d = ctx.project.tasks.get(uid).map(|t| t.duration);
            Some(SchedulingMode::Manual {
                start: None,
                finish: None,
                duration: d,
            })
        }
        Some(ModeJson::Manual) if !manual => Some(frozen_manual(ctx, uid, is_pure_freeze(op))?),
        Some(ModeJson::Auto) if manual => Some(SchedulingMode::Auto),
        _ => None,
    };
    let manual_after = target.map_or(manual, |m| m == ModeJson::Manual);
    // R-254 : dates et durée d'une feuille manuelle, sans contrainte.
    let current_mode = ctx.project.tasks.get(uid).map(|t| t.mode);
    let manual_values = if manual_after && !summary {
        let base = new_mode.or(current_mode).unwrap_or_default();
        manual_entry(op, ctx, uid, base)?
    } else {
        None
    };
    let (duration, entered) = match &manual_values {
        Some((_, d)) => {
            let current = ctx.project.tasks.get(uid).map(|t| t.duration);
            (Some(*d).filter(|d| Some(*d) != current), None)
        }
        None if manual_after && !summary => (None, None),
        None => (
            entered_duration(op, ctx, uid)?,
            entered_date(&op.start, &op.finish)?,
        ),
    };
    let mode = manual_values.as_ref().map(|(m, _)| *m).or(new_mode);
    Ok(Planned {
        mode,
        duration,
        entered,
    })
}

/// R-157, R-158, R-161, R-253, R-254 : `task.update`.
pub(super) fn update(op: &TaskUpdate, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let uid = ctx.task(&op.uid, "uid")?;
    let pos = position(ctx, uid, "uid")?;
    let summary = block_end(&ctx.project.tasks, pos) > pos.saturating_add(1);
    let manual = ctx
        .project
        .tasks
        .get(uid)
        .is_some_and(|t| matches!(t.mode, SchedulingMode::Manual { .. }));
    // Contrôles, avant toute modification.
    check_name_active(op)?;
    let Planned {
        mode: new_mode,
        duration,
        entered,
    } = plan_dates(op, ctx, uid, summary, manual)?;
    let balance = j5_balance(
        op,
        ctx,
        uid,
        summary,
        duration.as_ref().map(plannercraft_core::Duration::minutes),
    )?;
    // R-264 : champs personnalisés, contrôlés avant toute modification.
    let custom_edits = custom::plan_values(&op.custom, ctx, uid)?;

    let Some(t) = ctx.project.tasks.get_mut(uid) else {
        return Err(CommandError::new(
            codes::TASK_NOT_FOUND,
            CommandErrorKind::UnknownTask(uid),
        ));
    };
    let mut changes = Vec::new();
    if let Patch::Set(n) = &op.name
        && t.name != *n
    {
        t.name.clone_from(n);
        changes.push(TaskChange::Name);
    }
    if let Some(d) = duration {
        if t.duration != d {
            t.duration = d;
            changes.push(TaskChange::Duration);
        }
        // R-157 : durée nulle ⇒ jalon (sauf `milestone` explicite).
        if d.is_zero() && op.milestone.is_keep() && !t.milestone {
            t.milestone = true;
            changes.push(TaskChange::Milestone);
        }
    }
    let milestone = match op.milestone {
        Patch::Keep => None,
        Patch::Set(b) => Some(b),
        Patch::Clear => Some(false),
    };
    if let Some(m) = milestone
        && t.milestone != m
    {
        t.milestone = m;
        changes.push(TaskChange::Milestone);
    }
    let notes = match &op.notes {
        Patch::Keep => None,
        Patch::Set(n) => Some(n.as_str()),
        Patch::Clear => Some(""),
    };
    if let Some(n) = notes
        && t.notes != n
    {
        n.clone_into(&mut t.notes);
        changes.push(TaskChange::Notes);
    }
    if let Patch::Set(a) = op.active
        && t.active != a
    {
        t.active = a;
        changes.push(TaskChange::Active);
    }
    if let Some(m) = new_mode
        && t.mode != m
    {
        t.mode = m;
        changes.push(TaskChange::Mode);
    }
    if let Some(b) = &balance {
        set_scheduling(t, b, !op.work.is_keep(), &mut changes);
    }
    // R-264 : aucune valeur calculée n'en dépend, aucun `Extras` de
    // planning jeté (R-161).
    custom::apply_values(t, &custom_edits);
    if let Some((side, date)) = entered
        && set_entered(ctx, uid, side, date)
    {
        changes.push(TaskChange::Constraint);
    }
    if let Some(b) = &balance {
        // Durée, travail et affectations équilibrés (R-189 à R-194).
        b.commit(ctx)?;
    }
    touch(ctx, uid);
    for c in &changes {
        ctx.drop_task_extras(uid, *c);
    }
    if changes.contains(&TaskChange::Duration) {
        ctx.drop_assignment_extras(uid);
    }
    Ok(())
}

/// R-197 : type et pilotage équilibrés posés sur la tâche ; changements
/// notés (le travail l'est par l'équilibrage, s'il a été saisi).
fn set_scheduling(t: &mut Task, b: &Balance, work_entered: bool, changes: &mut Vec<TaskChange>) {
    if t.task_type != b.side.task_type {
        t.task_type = b.side.task_type;
        changes.push(TaskChange::Type);
    }
    if t.effort_driven != b.side.effort_driven {
        t.effort_driven = b.side.effort_driven;
        changes.push(TaskChange::EffortDriven);
    }
    if work_entered && t.work != b.side.work {
        changes.push(TaskChange::Work);
    }
}

/// `param.conflict` entre `a` et `b`, sur `b`.
fn conflict(a: &'static str, b: &'static str) -> CommandError {
    CommandError::new(codes::PARAM_CONFLICT, CommandErrorKind::ParamConflict(a, b)).with_field(b)
}

/// R-197 : contrôles et équilibrage des champs J5 de `task.update` (`type`,
/// `effort_driven`, `work`) et de la durée d'une feuille affectée (R-189
/// « durée »), avant toute modification. `None` si rien n'est à équilibrer
/// (aucun champ J5, durée sur une tâche sans affectation de travail).
fn j5_balance(
    op: &TaskUpdate,
    ctx: &Ctx<'_>,
    uid: TaskUid,
    summary: bool,
    duration: Option<plannercraft_core::Minutes>,
) -> Result<Option<Balance>, CommandError> {
    if !op.duration.is_keep() && !op.work.is_keep() {
        return Err(conflict("duration", "work"));
    }
    let fields = [
        ("type", !op.task_type.is_keep()),
        ("effort_driven", !op.effort_driven.is_keep()),
        ("work", !op.work.is_keep()),
    ];
    let j5 = fields.iter().any(|(_, set)| *set);
    if summary && let Some((field, _)) = fields.iter().find(|(_, set)| *set) {
        // R-197 : type, pilotage et travail d'une récapitulative sont
        // calculés (jamais pilotée, R-193).
        return Err(
            CommandError::new(codes::TASK_COMPUTED_FIELD, CommandErrorKind::ComputedField)
                .with_field(field)
                .with_subject(Subject::Task(uid)),
        );
    }
    let task_type = match &op.task_type {
        Patch::Keep => None,
        Patch::Set(t) => Some(TaskType::from(*t)),
        Patch::Clear => return Err(invalid("type", "type attendu (il ne s'efface pas)")),
    };
    let effort = match op.effort_driven {
        Patch::Keep => None,
        Patch::Set(e) => Some(e),
        Patch::Clear => return Err(invalid("effort_driven", "booléen attendu")),
    };
    let work = match &op.work {
        Patch::Keep => None,
        Patch::Set(w) => {
            let d = ctx.duration(w, "work")?;
            if d.basis() == TimeBasis::Elapsed {
                return Err(invalid(
                    "work",
                    "travail en temps ouvré attendu (durée écoulée refusée)",
                ));
            }
            Some(Work::new(d.minutes()).map_err(|_| invalid("work", "travail hors des bornes"))?)
        }
        Patch::Clear => return Err(invalid("work", "travail attendu (il ne s'efface pas)")),
    };
    if !j5 && (duration.is_none() || summary) {
        return Ok(None);
    }
    let mut b = balance_of(ctx, uid, &BTreeSet::new())?;
    if !j5 && b.parts.is_empty() {
        return Ok(None);
    }
    if let Some(t) = task_type {
        b.set_type(t, "type")?;
    }
    if let Some(e) = effort {
        b.set_effort_driven(e, "effort_driven")?;
    }
    if let Some(d) = duration
        && !b.parts.is_empty()
    {
        b.edit(&Edit::Duration(d), "duration")?;
    }
    if let Some(w) = work {
        if b.parts.is_empty() {
            return Err(invalid(
                "work",
                "travail saisi sur une tâche sans affectation de ressource de travail",
            ));
        }
        b.edit(&Edit::Work(w), "work")?;
    }
    Ok(Some(b))
}

/// R-155 : `task.delete`.
pub(super) fn delete(op: &TaskDelete, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let uid = ctx.task(&op.uid, "uid")?;
    let pos = position(ctx, uid, "uid")?;
    let end = block_end(&ctx.project.tasks, pos);
    let descendants = end.saturating_sub(pos).saturating_sub(1);
    if descendants > 0 && !op.cascade {
        return Err(CommandError::new(
            codes::TASK_HAS_CHILDREN,
            CommandErrorKind::HasChildren { count: descendants },
        )
        .with_field("cascade")
        .with_subject(Subject::Task(uid)));
    }
    let before_order = order(&ctx.project.tasks);
    let before_summaries = summaries(&ctx.project.tasks);
    let removed = ctx
        .project
        .tasks
        .splice(pos..end, Vec::new())
        .map_err(|e| model(&e))?;
    let deleted: BTreeSet<TaskUid> = removed.iter().map(Task::uid).collect();
    for t in &removed {
        let u = t.uid();
        touch(ctx, u);
        ctx.effects.push(SideEffect::TaskDeleted(u));
    }
    // Liens dont une tâche supprimée est successeur : partis avec elle.
    for t in &removed {
        for l in &t.predecessors {
            ctx.effects.push(SideEffect::LinkRemoved {
                pred: l.predecessor,
                succ: t.uid(),
            });
        }
    }
    // Liens dont elle est prédécesseur : une passe sur les tâches gardées.
    let mut relinked = Vec::new();
    for t in &ctx.project.tasks {
        if t.predecessors.iter().any(|l| deleted.contains(&l.predecessor)) {
            relinked.push(t.uid());
        }
    }
    // Successeurs dont les liens changent : Extras jetés après les
    // affectations, pour l'ordre des effets de R-155.
    let mut dropped_links = Vec::with_capacity(relinked.len());
    for succ in relinked {
        let Some(t) = ctx.project.tasks.get_mut(succ) else {
            continue;
        };
        let mut gone = Vec::new();
        t.predecessors.retain(|l| {
            let hit = deleted.contains(&l.predecessor);
            if hit {
                gone.push(l.predecessor);
            }
            !hit
        });
        for pred in gone {
            ctx.effects.push(SideEffect::LinkRemoved { pred, succ });
            ctx.touched.insert(
                Subject::Link {
                    predecessor: pred,
                    successor: succ,
                },
                ctx.op_index,
            );
        }
        touch(ctx, succ);
        dropped_links.push(succ);
    }
    // Affectations des tâches supprimées : une reconstruction de l'arène.
    if ctx.project.assignments.iter().any(|a| deleted.contains(&a.task)) {
        let all: Vec<Assignment> = std::mem::take(&mut ctx.project.assignments).into();
        let (gone, kept): (Vec<Assignment>, Vec<Assignment>) =
            all.into_iter().partition(|a| deleted.contains(&a.task));
        ctx.project.assignments = kept.try_into().map_err(|e: ModelError| model(&e))?;
        for a in gone {
            ctx.effects.push(SideEffect::AssignmentRemoved(a.uid()));
        }
    }
    for succ in dropped_links {
        ctx.drop_task_extras(succ, TaskChange::Links);
    }
    hierarchy_changed(ctx, &before_summaries, &[]);
    if let Some(e) = renumbered(&before_order, &ctx.project.tasks) {
        ctx.effects.push(e);
    }
    Ok(())
}

/// Décale de `delta` le niveau des tâches `block` (R-153, R-154).
fn shift_levels(block: &mut [Task], delta: i32) -> Result<(), CommandError> {
    // Contrôle complet d'abord : un refus ne laisse rien à moitié.
    let mut levels = Vec::with_capacity(block.len());
    for t in block.iter() {
        let level = i32::from(t.outline_level.get())
            .checked_add(delta)
            .and_then(|l| u16::try_from(l).ok())
            .filter(|&l| l >= 1)
            .ok_or_else(level_limit)?;
        levels.push(outline(level)?);
    }
    for (t, l) in block.iter_mut().zip(levels) {
        t.outline_level = l;
    }
    Ok(())
}

/// R-154 : `task.move`.
pub(super) fn move_to(op: &TaskMove, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let uid = ctx.task(&op.uid, "uid")?;
    let pos = position(ctx, uid, "uid")?;
    let end = block_end(&ctx.project.tasks, pos);
    // Cible résolue avant le retrait du bloc.
    let (slot, target) = slot(ctx, &op.to, "to")?;
    let into_block = target
        .and_then(|t| ctx.project.tasks.position(t))
        .is_some_and(|p| (pos..end).contains(&p))
        || (pos < slot.index && slot.index < end);
    if into_block {
        return Err(CommandError::new(
            codes::TASK_MOVE_INTO_SUBTREE,
            CommandErrorKind::MoveIntoOwnSubtree,
        )
        .with_field("to")
        .with_subject(Subject::Task(uid)));
    }
    let delta = i32::from(slot.level).saturating_sub(i32::from(level_at(&ctx.project.tasks, pos)));
    let size = end.saturating_sub(pos);
    let index = if slot.index >= end {
        slot.index.saturating_sub(size)
    } else {
        slot.index
    };
    // Bloc remis à sa place, au même niveau : rien ne change, aucun Extras
    // n'est jeté (R-161 : un champ n'est modifié que si sa valeur change).
    if index == pos && delta == 0 {
        touch(ctx, uid);
        return Ok(());
    }
    let before_order = order(&ctx.project.tasks);
    let before_summaries = summaries(&ctx.project.tasks);
    // Contrôle des niveaux sur une copie du bloc, avant tout retrait.
    let mut block: Vec<Task> = ctx.project.tasks.iter().skip(pos).take(size).cloned().collect();
    shift_levels(&mut block, delta)?;
    let moved: Vec<TaskUid> = block.iter().map(Task::uid).collect();
    ctx.project
        .tasks
        .splice(pos..end, Vec::new())
        .map_err(|e| model(&e))?;
    insert_block(ctx, index, block)?;
    hierarchy_changed(ctx, &before_summaries, &moved);
    if let Some(e) = renumbered(&before_order, &ctx.project.tasks) {
        ctx.effects.push(e);
    }
    Ok(())
}

/// `task.outline_refused` sur la tâche `uid`.
fn outline_refused(uid: TaskUid) -> CommandError {
    CommandError::new(codes::TASK_OUTLINE_REFUSED, CommandErrorKind::OutlineRefused)
        .with_field("uid")
        .with_subject(Subject::Task(uid))
}

/// Décale de `delta` le bloc de la tâche à la position `pos` (en place).
fn shift_block(ctx: &mut Ctx<'_>, pos: usize, delta: i32) -> Result<(), CommandError> {
    let end = block_end(&ctx.project.tasks, pos);
    let size = end.saturating_sub(pos);
    let mut block: Vec<Task> = ctx.project.tasks.iter().skip(pos).take(size).cloned().collect();
    shift_levels(&mut block, delta)?;
    let moved: Vec<TaskUid> = block.iter().map(Task::uid).collect();
    let before_summaries = summaries(&ctx.project.tasks);
    ctx.project.tasks.splice(pos..end, block).map_err(|e| model(&e))?;
    hierarchy_changed(ctx, &before_summaries, &moved);
    Ok(())
}

/// R-153 : `task.indent`.
pub(super) fn indent(op: &TaskOne, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let uid = ctx.task(&op.uid, "uid")?;
    let pos = position(ctx, uid, "uid")?;
    let tasks = &ctx.project.tasks;
    let Some(prev) = prev_non_null(tasks, pos) else {
        return Err(outline_refused(uid));
    };
    if level_at(tasks, pos) > level_at(tasks, prev) {
        return Err(outline_refused(uid));
    }
    if level_at(tasks, pos) == MAX_LEVEL {
        return Err(level_limit().with_field("uid"));
    }
    shift_block(ctx, pos, 1)
}

/// R-153 : `task.outdent`.
pub(super) fn outdent(op: &TaskOne, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let uid = ctx.task(&op.uid, "uid")?;
    let pos = position(ctx, uid, "uid")?;
    if level_at(&ctx.project.tasks, pos) <= 1 {
        return Err(outline_refused(uid));
    }
    shift_block(ctx, pos, -1)
}
