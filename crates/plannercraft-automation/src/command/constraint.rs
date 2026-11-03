//! Exécution des contraintes et échéances (R-159, R-161).
//!
//! Une contrainte en conflit avec un lien n'est jamais refusée : le moteur
//! la signale (`constraint.override`, `constraint.not_honored`), et R-160 la
//! rend en avertissement.

use plannercraft_core::{Constraint, ConstraintKind, Subject};

use super::exec::Ctx;
use super::extras_invalidation::TaskChange;
use super::link::is_summary;
use super::{Patch, SetConstraint, SetDeadline};
use crate::error::{CommandError, CommandErrorKind, codes};

/// Types acceptés sur une récapitulative (R-159, aligné sur R-103 : les
/// seuls qui agissent sur une récapitulative depuis le début).
const SUMMARY_KINDS: [ConstraintKind; 3] = [
    ConstraintKind::AsSoonAsPossible,
    ConstraintKind::StartNoEarlierThan,
    ConstraintKind::FinishNoLaterThan,
];

/// `param.invalid` sur `field`, avec l'explication `why`.
fn invalid(why: &str, field: &str) -> CommandError {
    CommandError::new(
        codes::PARAM_INVALID,
        CommandErrorKind::InvalidParam(why.to_owned()),
    )
    .with_field(field)
}

/// `task.set_constraint` (R-159).
pub(super) fn set_constraint(op: &SetConstraint, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let uid = ctx.task(&op.uid, "uid")?;
    let kind = ConstraintKind::from(op.kind);
    let constraint = Constraint::from_parts(kind, op.date).ok_or_else(|| {
        if kind.is_dated() {
            invalid("ce type de contrainte exige une date", "date")
        } else {
            invalid("« asap » et « alap » ne prennent pas de date", "date")
        }
    })?;
    if is_summary(ctx.project, uid) && !SUMMARY_KINDS.contains(&kind) {
        return Err(invalid(
            "une récapitulative n'accepte que « asap », « snet » ou « fnlt »",
            "kind",
        )
        .with_subject(Subject::Task(uid)));
    }
    ctx.touched.insert(Subject::Task(uid), ctx.op_index);
    let task = ctx.project.tasks.get_mut(uid).ok_or_else(|| {
        CommandError::new(codes::TASK_NOT_FOUND, CommandErrorKind::UnknownTask(uid)).with_field("uid")
    })?;
    if task.constraint != constraint {
        task.constraint = constraint;
        ctx.drop_task_extras(uid, TaskChange::Constraint);
    }
    Ok(())
}

/// `task.set_deadline` (R-159) : pose (`Set`), retire (`Clear`) ou garde
/// l'échéance. Aucune clé d'`Extras` n'en dépend (R-161).
pub(super) fn set_deadline(op: &SetDeadline, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let uid = ctx.task(&op.uid, "uid")?;
    ctx.touched.insert(Subject::Task(uid), ctx.op_index);
    let task = ctx.project.tasks.get_mut(uid).ok_or_else(|| {
        CommandError::new(codes::TASK_NOT_FOUND, CommandErrorKind::UnknownTask(uid)).with_field("uid")
    })?;
    match op.deadline {
        Patch::Keep => {}
        Patch::Set(d) => task.deadline = Some(d),
        Patch::Clear => task.deadline = None,
    }
    Ok(())
}
