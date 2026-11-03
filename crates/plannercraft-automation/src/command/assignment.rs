//! Commandes `assignment.*` (R-196, lot J5 L7) et équilibrage partagé avec
//! `task.update` (R-197) et `resource.delete` (R-195).
//!
//! Les commandes ne calculent rien elles-mêmes : elles lisent l'état d'une
//! tâche et de ses affectations de travail ([`Balance`]), demandent
//! l'équilibrage au cœur (`plannercraft_core::resource::edit::apply`, R-170,
//! R-189 à R-194), puis écrivent le résultat ([`Balance::commit`]). Le
//! recalcul du planning ne fait que placer (ADR-0013 D3).
//!
//! Hypothèses du lot (fiche R-196) :
//! - une tâche manuelle s'équilibre durée tenue (durée fixe, non pilotée) :
//!   sa durée est une saisie (ADR-0005), jamais recalculée par une
//!   affectation ;
//! - une récapitulative ne s'équilibre jamais (R-193) : ses affectations
//!   gardent le travail saisi, à défaut durée enregistrée × unités (R-179) ;
//! - ressource matérielle : unités 100 %, travail = quantité × 60 min
//!   (O30) ; ressource de coût : unités 100 %, travail nul.

use std::collections::BTreeSet;

use plannercraft_core::resource::{Edit, EditError, Part, Rebalanced, TaskSide, apply};
use plannercraft_core::{
    Assignment, AssignmentUid, Duration, HasUid, Minutes, ModelError, ResourceKind, ResourceUid,
    SchedulingMode, Subject, TaskOutcome, TaskType, TaskUid, TimeBasis, Units, Work, round_div,
};

use super::exec::Ctx;
use super::extras_invalidation::TaskChange;
use super::outline::is_summary_at;
use super::{AssignmentAdd, AssignmentKey, AssignmentUpdate, Patch, ResourceRef, TaskRef};
use crate::error::{CommandError, CommandErrorKind, codes};
use crate::json::DurationJson;
use crate::outcome::SideEffect;

/// Échelle des unités : 10 000 = 100 % (R-002).
const UNIT_SCALE: i128 = 10_000;

/// Quantité par défaut d'une affectation matérielle : 1 unité.
const DEFAULT_QUANTITY: Units = Units::FULL;

/// `param.invalid` sur `field`, avec une explication.
pub(super) fn invalid(field: &str, why: &str) -> CommandError {
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

/// `assignment.kind_mismatch` sur `field` : champ contraire au type de la
/// ressource.
fn kind_mismatch(field: &str, resource: ResourceUid) -> CommandError {
    CommandError::new(
        codes::ASSIGNMENT_KIND_MISMATCH,
        CommandErrorKind::AssignmentKindMismatch,
    )
    .with_field(field)
    .with_subject(Subject::Resource(resource))
}

/// Refus de l'équilibrage → erreur de commande sur `field`, sujet la tâche.
pub(super) fn edit_error(e: EditError, task: TaskUid, field: &str) -> CommandError {
    let base = match e {
        EditError::FixedWorkEffortDriven => CommandError::new(
            codes::TASK_FIXED_WORK_EFFORT_DRIVEN,
            CommandErrorKind::FixedWorkEffortDriven,
        ),
        EditError::ZeroUnitsFixedWork => CommandError::new(
            codes::ASSIGNMENT_ZERO_UNITS_FIXED_WORK,
            CommandErrorKind::ZeroUnitsFixedWork,
        ),
        EditError::SummaryNotEditable => {
            CommandError::new(codes::TASK_COMPUTED_FIELD, CommandErrorKind::ComputedField)
        }
        EditError::UnknownAssignment(_) | EditError::DuplicateAssignment(_) | EditError::Overflow => {
            CommandError::new(
                codes::PARAM_INVALID,
                CommandErrorKind::InvalidParam(e.to_string()),
            )
        }
    };
    base.with_field(field).with_subject(Subject::Task(task))
}

/// Comment une tâche s'équilibre.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    /// Feuille automatique : matrice S41, pilotage par l'effort (R-189 à
    /// R-194).
    Balanced,
    /// Tâche manuelle : durée tenue (équilibrée comme une durée fixe non
    /// pilotée ; hypothèse).
    Manual,
    /// Récapitulative : jamais équilibrée (R-193).
    Summary,
}

/// État d'une tâche et de ses affectations de travail, en cours
/// d'équilibrage.
#[derive(Clone, Debug)]
pub(super) struct Balance {
    /// Tâche.
    pub task: TaskUid,
    /// Côté tâche : type et pilotage réels, durée et travail courants.
    pub side: TaskSide,
    /// Affectations de ressources de travail, triées par UID.
    pub parts: Vec<Part>,
    /// Mode d'équilibrage.
    pub mode: Mode,
}

impl Balance {
    /// Côté tâche passé à l'équilibrage des grandeurs : celui de la tâche,
    /// ou durée fixe non pilotée pour une tâche manuelle.
    const fn edit_side(&self) -> TaskSide {
        match self.mode {
            Mode::Manual => TaskSide {
                task_type: TaskType::FixedDuration,
                effort_driven: false,
                ..self.side
            },
            Mode::Balanced | Mode::Summary => self.side,
        }
    }

    /// Applique `edit` (durée, travail, unités, ajout, retrait) ; l'état
    /// suit le résultat. Erreurs sur `field`.
    pub fn edit(&mut self, edit: &Edit, field: &str) -> Result<(), CommandError> {
        let r: Rebalanced =
            apply(&self.edit_side(), &self.parts, edit).map_err(|e| edit_error(e, self.task, field))?;
        self.side.duration = r.duration;
        self.side.work = r.work;
        self.parts = r.parts;
        Ok(())
    }

    /// Contrôle un changement de type (R-189 : ne recalcule rien ; R-193 :
    /// unités nulles refusées en travail fixe) et l'enregistre ; le
    /// pilotage effectif suit (travail fixe toujours piloté).
    pub fn set_type(&mut self, t: TaskType, field: &str) -> Result<(), CommandError> {
        apply(&self.side, &self.parts, &Edit::Type(t)).map_err(|e| edit_error(e, self.task, field))?;
        self.side.task_type = t;
        self.side.effort_driven =
            plannercraft_core::resource::edit::effective_effort_driven(t, self.side.effort_driven);
        Ok(())
    }

    /// Contrôle un changement de pilotage (R-193) et l'enregistre.
    pub fn set_effort_driven(&mut self, e: bool, field: &str) -> Result<(), CommandError> {
        apply(&self.side, &self.parts, &Edit::EffortDriven(e))
            .map_err(|err| edit_error(err, self.task, field))?;
        self.side.effort_driven = e;
        Ok(())
    }

    /// Part de l'affectation `uid`, si c'en est une.
    pub fn part(&self, uid: AssignmentUid) -> Option<Part> {
        self.parts.iter().find(|p| p.uid == uid).copied()
    }

    /// Écrit l'état dans le clone du lot : durée (minutes changées, unité et
    /// base gardées), travail de la tâche (feuille), unités et travail des
    /// affectations ; jette la répartition des affectations changées
    /// (R-161) et celle de la tâche si sa durée change. Rend vrai si la
    /// durée de la tâche a changé.
    pub fn commit(&self, ctx: &mut Ctx<'_>) -> Result<bool, CommandError> {
        let mut changed_parts = Vec::new();
        for p in &self.parts {
            let Some(a) = ctx.project.assignments.get_mut(p.uid) else {
                continue;
            };
            if a.units != p.units || a.work != p.work {
                a.units = p.units;
                a.work = p.work;
                changed_parts.push(p.uid);
            }
        }
        let mut duration_changed = false;
        if self.mode != Mode::Summary {
            let Some(t) = ctx.project.tasks.get_mut(self.task) else {
                return Err(not_found_task(self.task));
            };
            if t.duration.minutes() != self.side.duration {
                let d = &t.duration;
                t.duration = Duration::new(self.side.duration, d.unit(), d.basis(), d.is_estimated())
                    .map_err(|_| invalid("duration", "durée recalculée hors des bornes du modèle"))?;
                duration_changed = true;
            }
            t.work = self.side.work;
        }
        for uid in changed_parts {
            ctx.touched.insert(Subject::Assignment(uid), ctx.op_index);
            ctx.drop_one_assignment_extras(uid);
        }
        if duration_changed {
            ctx.drop_task_extras(self.task, TaskChange::Duration);
            ctx.drop_assignment_extras(self.task);
        }
        ctx.touched.insert(Subject::Task(self.task), ctx.op_index);
        Ok(duration_changed)
    }
}

/// `task.not_found` pour `uid`.
fn not_found_task(uid: TaskUid) -> CommandError {
    CommandError::new(codes::TASK_NOT_FOUND, CommandErrorKind::UnknownTask(uid))
        .with_subject(Subject::Task(uid))
}

/// Vrai si la ressource `r` existe et est une ressource de travail.
fn is_work_resource(ctx: &Ctx<'_>, r: Option<ResourceUid>) -> bool {
    r.and_then(|r| ctx.project.resources.get(r))
        .is_some_and(|res| res.kind == ResourceKind::Work)
}

/// État d'équilibrage de la tâche `task`, sans les affectations `skip`
/// (retirées plus tôt dans la même opération).
pub(super) fn balance_of(
    ctx: &Ctx<'_>,
    task: TaskUid,
    skip: &BTreeSet<AssignmentUid>,
) -> Result<Balance, CommandError> {
    let tasks = &ctx.project.tasks;
    let pos = tasks.position(task).ok_or_else(|| not_found_task(task))?;
    let t = tasks.at(pos).ok_or_else(|| not_found_task(task))?;
    let summary = is_summary_at(tasks, pos);
    let mode = if summary {
        Mode::Summary
    } else if matches!(t.mode, SchedulingMode::Manual { .. }) {
        Mode::Manual
    } else {
        Mode::Balanced
    };
    let mut parts: Vec<Part> = ctx
        .project
        .assignments
        .iter()
        .filter(|a| a.task == task && !skip.contains(&a.uid()) && is_work_resource(ctx, a.resource))
        .map(|a| Part {
            uid: a.uid(),
            units: a.units,
            work: a.work,
        })
        .collect();
    parts.sort_by_key(|p| p.uid);
    let mut total: i64 = 0;
    for p in &parts {
        total = total
            .checked_add(p.work.minutes().get())
            .ok_or_else(|| invalid("work", "travail total hors des bornes"))?;
    }
    let work =
        Work::new(Minutes::new(total)).map_err(|_| invalid("work", "travail total hors des bornes"))?;
    Ok(Balance {
        task,
        side: TaskSide {
            task_type: t.task_type,
            effort_driven: t.effort_driven,
            duration: t.duration.minutes(),
            work,
            summary,
        },
        parts,
        mode,
    })
}

/// Travail en minutes ouvrées d'une durée JSON (`elapsed` refusé).
fn work_of(ctx: &Ctx<'_>, d: &DurationJson, field: &str) -> Result<Work, CommandError> {
    let d = ctx.duration(d, field)?;
    if d.basis() == TimeBasis::Elapsed {
        return Err(invalid(
            field,
            "travail en temps ouvré attendu (durée écoulée refusée)",
        ));
    }
    Work::new(d.minutes()).map_err(|_| invalid(field, "travail hors des bornes"))
}

/// Délai d'affectation en minutes (`Delay`, R-177) : ouvré, ≥ 0.
fn delay_of(ctx: &Ctx<'_>, d: &DurationJson, field: &str) -> Result<Minutes, CommandError> {
    let d = ctx.duration(d, field)?;
    if d.basis() == TimeBasis::Elapsed {
        return Err(invalid(
            field,
            "délai en temps ouvré attendu (durée écoulée refusée)",
        ));
    }
    Ok(d.minutes())
}

/// Travail d'une quantité matérielle : `q × 60 min` (R-001, O30).
fn quantity_work(q: Units, field: &str) -> Result<Work, CommandError> {
    let minutes = i128::from(q.get())
        .checked_mul(60)
        .and_then(|n| round_div(n, UNIT_SCALE))
        .ok_or_else(|| invalid(field, "quantité hors des bornes"))?;
    Work::new(Minutes::new(minutes)).map_err(|_| invalid(field, "quantité hors des bornes"))
}

/// Durée calculée de la récapitulative `task` dans l'état courant du lot
/// (R-054, R-064 : temps ouvré de son span, ou durée utilisée d'une
/// récapitulative manuelle), et non sa durée du modèle : celle-ci vaut 0
/// pour une récapitulative automatique relue (valeur calculée,
/// `computed_zero`) et reste celle de la saisie pour une tâche devenue
/// récapitulative. À défaut (récapitulative non planifiée) : `stored`.
/// Recalcul complet du clone : réservé à l'affectation d'une
/// récapitulative sans travail fourni, cas rare.
fn summary_duration(ctx: &Ctx<'_>, task: TaskUid, stored: Minutes) -> Minutes {
    match plannercraft_core::schedule(ctx.project).get(task) {
        Some(TaskOutcome::Scheduled(s)) => s.duration,
        _ => stored,
    }
}

/// Travail par défaut d'une affectation de récapitulative : durée calculée
/// × unités (R-170, R-179, R-196 ; hypothèse).
fn summary_work(duration: Minutes, units: Units) -> Result<Work, CommandError> {
    let minutes = i128::from(duration.get())
        .checked_mul(i128::from(units.get()))
        .and_then(|n| round_div(n, UNIT_SCALE))
        .ok_or_else(|| invalid("units", "travail hors des bornes"))?;
    Work::new(Minutes::new(minutes)).map_err(|_| invalid("units", "travail hors des bornes"))
}

/// Affectation de `resource` à `task`, s'il y en a une.
fn find(ctx: &Ctx<'_>, task: TaskUid, resource: ResourceUid) -> Option<AssignmentUid> {
    ctx.project
        .assignments
        .iter()
        .find(|a| a.task == task && a.resource == Some(resource))
        .map(HasUid::uid)
}

/// Tâche et ressource désignées ; l'affectation qui les lie, ou
/// `assignment.not_found`.
fn resolve_key(
    ctx: &Ctx<'_>,
    task: &TaskRef,
    resource: &ResourceRef,
) -> Result<(TaskUid, ResourceUid, AssignmentUid), CommandError> {
    let t = ctx.task(task, "task")?;
    let r = ctx.resource(resource, "resource")?;
    let a = find(ctx, t, r).ok_or_else(|| {
        CommandError::new(
            codes::ASSIGNMENT_NOT_FOUND,
            CommandErrorKind::UnknownAssignment { task: t, resource: r },
        )
        .with_field("resource")
        .with_subject(Subject::Task(t))
    })?;
    Ok((t, r, a))
}

/// Type de la ressource `r` (existante).
fn kind_of(ctx: &Ctx<'_>, r: ResourceUid) -> ResourceKind {
    ctx.project
        .resources
        .get(r)
        .map_or(ResourceKind::Work, |res| res.kind.clone())
}

/// R-196 : `assignment.add`.
pub(super) fn add(op: &AssignmentAdd, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let task = ctx.task(&op.task, "task")?;
    let resource = ctx.resource(&op.resource, "resource")?;
    if let Some(existing) = find(ctx, task, resource) {
        return Err(CommandError::new(
            codes::ASSIGNMENT_DUPLICATE,
            CommandErrorKind::DuplicateAssignment { task, resource },
        )
        .with_field("resource")
        .with_subject(Subject::Assignment(existing)));
    }
    let kind = kind_of(ctx, resource);
    let max_units = ctx
        .project
        .resources
        .get(resource)
        .map_or(Units::FULL, |r| r.max_units);
    // Champs selon le type de la ressource, lus avant toute modification.
    let work_kind = kind == ResourceKind::Work;
    if !work_kind {
        if op.units.is_some() {
            return Err(kind_mismatch("units", resource));
        }
        if op.work.is_some() {
            return Err(kind_mismatch("work", resource));
        }
    }
    let material = matches!(kind, ResourceKind::Material { .. });
    if !material && op.quantity.is_some() {
        return Err(kind_mismatch("quantity", resource));
    }
    let units = match &op.units {
        Some(u) => ctx.units(u, "units")?,
        None if work_kind => Units::FULL.min(max_units),
        None => Units::FULL,
    };
    let work = op.work.as_ref().map(|w| work_of(ctx, w, "work")).transpose()?;
    let quantity = op
        .quantity
        .as_ref()
        .map(|q| ctx.quantity(q, "quantity"))
        .transpose()?;
    let delay = op
        .delay
        .as_ref()
        .map(|d| delay_of(ctx, d, "delay"))
        .transpose()?
        .unwrap_or(Minutes::ZERO);

    let uid = ctx.uids.alloc_assignment()?;
    let mut a = Assignment::new(uid, task, Some(resource));
    a.units = units;
    a.delay = delay;
    let mut balance = None;
    if work_kind {
        let mut b = balance_of(ctx, task, &BTreeSet::new())?;
        if b.mode == Mode::Summary {
            a.work = match work {
                Some(w) => w,
                None => summary_work(summary_duration(ctx, task, b.side.duration), units)?,
            };
        } else {
            b.edit(&Edit::Add { uid, units }, "units")?;
            if let Some(w) = work {
                b.edit(&Edit::AssignmentWork { uid, work: w }, "work")?;
            }
            if let Some(p) = b.part(uid) {
                a.units = p.units;
                a.work = p.work;
            }
            balance = Some(b);
        }
    } else if material {
        a.work = quantity_work(quantity.unwrap_or(DEFAULT_QUANTITY), "quantity")?;
    }
    // Affectations fictives (−65 535) de la tâche, retirées (O24).
    let placeholders: Vec<AssignmentUid> = ctx
        .project
        .assignments
        .iter()
        .filter(|x| x.task == task && x.resource.is_none())
        .map(HasUid::uid)
        .collect();
    ctx.project.assignments.push(a).map_err(|e| model(&e))?;
    ctx.touched.insert(Subject::Assignment(uid), ctx.op_index);
    if let Some(b) = balance {
        b.commit(ctx)?;
    }
    for p in placeholders {
        if ctx.project.assignments.remove(p).is_some() {
            ctx.effects.push(SideEffect::PlaceholderRemoved(p));
        }
    }
    ctx.touched.insert(Subject::Task(task), ctx.op_index);
    Ok(())
}

/// R-196 : `assignment.update`.
pub(super) fn update(op: &AssignmentUpdate, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let (task, resource, uid) = resolve_key(ctx, &op.task, &op.resource)?;
    let kind = kind_of(ctx, resource);
    let work_kind = kind == ResourceKind::Work;
    let material = matches!(kind, ResourceKind::Material { .. });
    // Contrôles, avant toute modification.
    if !work_kind {
        if !op.units.is_keep() {
            return Err(kind_mismatch("units", resource));
        }
        if !op.work.is_keep() {
            return Err(kind_mismatch("work", resource));
        }
    }
    if !material && !op.quantity.is_keep() {
        return Err(kind_mismatch("quantity", resource));
    }
    let units = match &op.units {
        Patch::Keep => None,
        Patch::Set(u) => Some(ctx.units(u, "units")?),
        Patch::Clear => return Err(invalid("units", "unités attendues (elles ne s'effacent pas)")),
    };
    let work = match &op.work {
        Patch::Keep => None,
        Patch::Set(w) => Some(work_of(ctx, w, "work")?),
        Patch::Clear => return Err(invalid("work", "travail attendu (il ne s'efface pas)")),
    };
    let quantity = match &op.quantity {
        Patch::Keep => None,
        Patch::Set(q) => Some(quantity_work(ctx.quantity(q, "quantity")?, "quantity")?),
        Patch::Clear => return Err(invalid("quantity", "quantité attendue (elle ne s'efface pas)")),
    };
    let delay = match &op.delay {
        Patch::Keep => None,
        Patch::Set(d) => Some(delay_of(ctx, d, "delay")?),
        Patch::Clear => Some(Minutes::ZERO),
    };

    let mut changed = false;
    if work_kind && (units.is_some() || work.is_some()) {
        let mut b = balance_of(ctx, task, &BTreeSet::new())?;
        if b.mode == Mode::Summary {
            // Récapitulative : valeurs posées telles quelles (R-179, R-193).
            if let Some(a) = ctx.project.assignments.get_mut(uid) {
                let before = (a.units, a.work);
                if let Some(u) = units {
                    a.units = u;
                }
                if let Some(w) = work {
                    a.work = w;
                }
                changed = before != (a.units, a.work);
            }
        } else {
            if let Some(u) = units {
                b.edit(&Edit::Units { uid, units: u }, "units")?;
            }
            if let Some(w) = work {
                b.edit(&Edit::AssignmentWork { uid, work: w }, "work")?;
            }
            b.commit(ctx)?;
        }
    }
    if let Some(a) = ctx.project.assignments.get_mut(uid) {
        if let Some(w) = quantity
            && a.work != w
        {
            a.work = w;
            changed = true;
        }
        if let Some(d) = delay
            && a.delay != d
        {
            a.delay = d;
            changed = true;
        }
    }
    if changed {
        ctx.drop_one_assignment_extras(uid);
    }
    ctx.touched.insert(Subject::Assignment(uid), ctx.op_index);
    ctx.touched.insert(Subject::Task(task), ctx.op_index);
    Ok(())
}

/// R-191, R-192 : équilibre la tâche de l'affectation `uid` comme si elle
/// était retirée (les affectations `skip` le sont déjà), sans la retirer de
/// l'arène ; l'ajoute à `skip`. Sans effet sur une récapitulative ou pour
/// une ressource qui n'est pas de travail.
pub(super) fn unbalance(
    ctx: &mut Ctx<'_>,
    uid: AssignmentUid,
    skip: &mut BTreeSet<AssignmentUid>,
) -> Result<(), CommandError> {
    let Some(a) = ctx.project.assignments.get(uid) else {
        return Ok(());
    };
    let (task, resource) = (a.task, a.resource);
    if is_work_resource(ctx, resource) && ctx.project.tasks.contains(task) {
        let mut b = balance_of(ctx, task, skip)?;
        if b.mode != Mode::Summary {
            b.edit(&Edit::Remove(uid), "resource")?;
            b.commit(ctx)?;
        }
    }
    skip.insert(uid);
    ctx.touched.insert(Subject::Task(task), ctx.op_index);
    Ok(())
}

/// R-196 : `assignment.remove` (aucune affectation fictive recréée).
pub(super) fn remove(op: &AssignmentKey, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let (_, _, uid) = resolve_key(ctx, &op.task, &op.resource)?;
    let mut skip = BTreeSet::new();
    unbalance(ctx, uid, &mut skip)?;
    if ctx.project.assignments.remove(uid).is_some() {
        ctx.effects.push(SideEffect::AssignmentRemoved(uid));
    }
    ctx.touched.insert(Subject::Assignment(uid), ctx.op_index);
    Ok(())
}
