//! Exécution des commandes de liens (R-156, R-161).
//!
//! Un lien est porté par son successeur (`Task::predecessors`) et désigné par
//! la paire (prédécesseur, successeur) : une paire n'a qu'un lien (R-156,
//! comme `Project::validate`). Les refus propres à la commande (auto-lien,
//! doublon, récapitulative ↔ descendante, lien absent) sont immédiats ; un
//! cycle n'est connu qu'après le recalcul et passe par R-160.

use plannercraft_core::{Lag, Link, LinkType, Project, Subject, Task, TaskUid};

use super::exec::Ctx;
use super::extras_invalidation::TaskChange;
use super::outline;
use super::{LinkKey, LinkSpec, TaskRef};
use crate::error::{CommandError, CommandErrorKind, codes};

/// Niveau hiérarchique de la tâche à la position `pos` (R-052).
fn level_at(p: &Project, pos: usize) -> Option<u16> {
    p.tasks.at(pos).map(|t| t.outline_level.get())
}

/// Vrai si la tâche `uid` a au moins une sous-tâche : la tâche non vide
/// suivante dans l'ordre d'affichage est d'un niveau plus profond (R-052).
pub(super) fn is_summary(p: &Project, uid: TaskUid) -> bool {
    p.tasks
        .position(uid)
        .is_some_and(|pos| outline::is_summary_at(&p.tasks, pos))
}

/// Vrai si l'une des deux tâches est une descendante de l'autre (R-059) :
/// toutes les tâches non vides de la plus haute (exclue) à la plus basse
/// (incluse) sont plus profondes que la plus haute ; les lignes vides ne
/// comptent pas (R-052). Parcours borné par la distance.
fn related_by_outline(p: &Project, a: TaskUid, b: TaskUid) -> bool {
    let (Some(pa), Some(pb)) = (p.tasks.position(a), p.tasks.position(b)) else {
        return false;
    };
    let (top, bottom) = if pa < pb { (pa, pb) } else { (pb, pa) };
    let Some(top_level) = level_at(p, top) else {
        return false;
    };
    p.tasks
        .iter()
        .skip(top.saturating_add(1))
        .take(bottom.saturating_sub(top))
        .filter(|t| !t.is_null)
        .all(|t| t.outline_level.get() > top_level)
}

/// Erreur d'un lien, sujet le lien.
fn link_error(code: &'static str, kind: CommandErrorKind, pred: TaskUid, succ: TaskUid) -> CommandError {
    CommandError::new(code, kind).with_subject(Subject::Link {
        predecessor: pred,
        successor: succ,
    })
}

/// `link.not_found` : la paire n'a pas de lien.
fn not_found(pred: TaskUid, succ: TaskUid) -> CommandError {
    link_error(
        codes::LINK_NOT_FOUND,
        CommandErrorKind::InvalidParam(format!("aucun lien de la tâche {pred} vers la tâche {succ}")),
        pred,
        succ,
    )
}

/// Successeur `succ` du clone (résolu, donc présent).
fn successor(p: &mut Project, succ: TaskUid) -> Result<&mut Task, CommandError> {
    p.tasks.get_mut(succ).ok_or_else(|| {
        CommandError::new(codes::TASK_NOT_FOUND, CommandErrorKind::UnknownTask(succ))
            .with_field("succ")
            .with_subject(Subject::Task(succ))
    })
}

/// Résout la paire et marque le lien et le successeur comme touchés (R-160).
fn resolve_pair(
    ctx: &mut Ctx<'_>,
    pred: &TaskRef,
    succ: &TaskRef,
) -> Result<(TaskUid, TaskUid), CommandError> {
    let p = ctx.task(pred, "pred")?;
    let s = ctx.task(succ, "succ")?;
    ctx.touched.insert(
        Subject::Link {
            predecessor: p,
            successor: s,
        },
        ctx.op_index,
    );
    ctx.touched.insert(Subject::Task(s), ctx.op_index);
    Ok((p, s))
}

/// `link.add` (R-156) : type FS et décalage nul par défaut.
pub(super) fn add(op: &LinkSpec, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let (pred, succ) = resolve_pair(ctx, &op.pred, &op.succ)?;
    if pred == succ {
        return Err(link_error(codes::LINK_SELF, CommandErrorKind::SelfLink, pred, succ).with_field("succ"));
    }
    let kind = op.kind.map_or(LinkType::FinishToStart, LinkType::from);
    let lag = match &op.lag {
        Some(l) => ctx.lag(l, "lag")?,
        None => Lag::ZERO,
    };
    let task = successor(ctx.project, succ)?;
    if task.predecessors.iter().any(|l| l.predecessor == pred) {
        return Err(link_error(
            codes::LINK_DUPLICATE,
            CommandErrorKind::DuplicateLink,
            pred,
            succ,
        ));
    }
    if related_by_outline(ctx.project, pred, succ) {
        return Err(link_error(
            codes::LINK_SUMMARY_DESCENDANT,
            CommandErrorKind::SummaryDescendantLink,
            pred,
            succ,
        ));
    }
    successor(ctx.project, succ)?.predecessors.push(Link {
        predecessor: pred,
        kind,
        lag,
    });
    ctx.drop_task_extras(succ, TaskChange::Links);
    Ok(())
}

/// `link.update` (R-156) : remplace le type et/ou le décalage ; un champ
/// absent est gardé.
pub(super) fn update(op: &LinkSpec, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let (pred, succ) = resolve_pair(ctx, &op.pred, &op.succ)?;
    let lag = match &op.lag {
        Some(l) => Some(ctx.lag(l, "lag")?),
        None => None,
    };
    let task = successor(ctx.project, succ)?;
    let link = task
        .predecessors
        .iter_mut()
        .find(|l| l.predecessor == pred)
        .ok_or_else(|| not_found(pred, succ))?;
    let before = (link.kind, link.lag);
    if let Some(k) = op.kind {
        link.kind = k.into();
    }
    if let Some(l) = lag {
        link.lag = l;
    }
    // R-161 : un champ n'est modifié que si sa valeur change.
    if (link.kind, link.lag) != before {
        ctx.drop_task_extras(succ, TaskChange::Links);
    }
    Ok(())
}

/// `link.remove` (R-156).
pub(super) fn remove(op: &LinkKey, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let (pred, succ) = resolve_pair(ctx, &op.pred, &op.succ)?;
    let task = successor(ctx.project, succ)?;
    let before = task.predecessors.len();
    task.predecessors.retain(|l| l.predecessor != pred);
    if task.predecessors.len() == before {
        return Err(not_found(pred, succ));
    }
    ctx.drop_task_extras(succ, TaskChange::Links);
    Ok(())
}
