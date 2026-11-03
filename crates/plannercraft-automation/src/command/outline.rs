//! Calculs de blocs hiérarchiques sur l'ordre des tâches (R-052, R-152 à
//! R-155, lot L4).
//!
//! La hiérarchie se déduit de l'ordre et des niveaux, comme dans le moteur :
//! les lignes vides (`is_null`) sont ignorées pour décider qui est parent de
//! qui, mais une ligne vide située entre deux descendants d'une tâche fait
//! partie de son bloc. Le **bloc** d'une tâche est la tâche suivie de ses
//! descendants : il s'arrête après le dernier descendant non vide (les
//! lignes vides qui suivent restent en place).

use std::collections::BTreeSet;

use plannercraft_core::{Arena, Task, TaskId, TaskUid};

use crate::error::{CommandError, CommandErrorKind, codes};
use crate::outcome::SideEffect;

/// Tâches d'un projet, dans l'ordre des ID.
pub(super) type Tasks = Arena<TaskUid, Task>;

/// Niveau hiérarchique maximal (`OutlineLevel`, `u16`).
pub(super) const MAX_LEVEL: u16 = u16::MAX;

/// Niveau de la tâche à la position `pos` (0 si absente : jamais attendu).
pub(super) fn level_at(tasks: &Tasks, pos: usize) -> u16 {
    tasks.at(pos).map_or(0, |t| t.outline_level.get())
}

/// Fin (exclue) du bloc de la tâche à la position `pos` : 1 + position de
/// son dernier descendant non vide, `pos + 1` sans descendant.
pub(super) fn block_end(tasks: &Tasks, pos: usize) -> usize {
    let level = level_at(tasks, pos);
    let start = pos.saturating_add(1);
    let mut end = start;
    for (q, t) in tasks.iter().enumerate().skip(start) {
        if t.is_null {
            continue;
        }
        if t.outline_level.get() <= level {
            break;
        }
        end = q.saturating_add(1);
    }
    end
}

/// Vrai si la tâche à la position `pos` est une récapitulative (R-052) :
/// non vide, et la tâche non vide suivante est d'un niveau supérieur. Les
/// lignes vides intercalées ne comptent pas, comme dans le moteur et
/// [`summaries`]. S'arrête à la première tâche non vide qui suit.
#[must_use]
pub fn is_summary_at(tasks: &Tasks, pos: usize) -> bool {
    let Some(task) = tasks.at(pos).filter(|t| !t.is_null) else {
        return false;
    };
    let level = task.outline_level.get();
    tasks
        .iter()
        .skip(pos.saturating_add(1))
        .find(|t| !t.is_null)
        .is_some_and(|next| next.outline_level.get() > level)
}

/// Position de la tâche non vide qui précède `pos`.
pub(super) fn prev_non_null(tasks: &Tasks, pos: usize) -> Option<usize> {
    (0..pos).rev().find(|&q| tasks.at(q).is_some_and(|t| !t.is_null))
}

/// UID des récapitulatives : tâches non vides dont la tâche non vide
/// suivante est d'un niveau supérieur (R-052). Une passe.
pub(super) fn summaries(tasks: &Tasks) -> BTreeSet<TaskUid> {
    let mut out = BTreeSet::new();
    let mut prev: Option<(TaskUid, u16)> = None;
    for t in tasks.iter().filter(|t| !t.is_null) {
        let level = t.outline_level.get();
        if let Some((uid, l)) = prev
            && level > l
        {
            out.insert(uid);
        }
        prev = Some((t.uid(), level));
    }
    out
}

/// UID des tâches, dans l'ordre.
pub(super) fn order(tasks: &Tasks) -> Vec<TaskUid> {
    tasks.uids().collect()
}

/// `limit.exceeded` sur le niveau hiérarchique (R-153).
pub(super) fn level_limit() -> CommandError {
    CommandError::new(
        codes::LIMIT_EXCEEDED,
        CommandErrorKind::Limit {
            what: "outline_level",
            max: u64::from(MAX_LEVEL),
        },
    )
}

/// ID (position + 1) d'une position.
fn id_of(pos: usize) -> TaskId {
    TaskId::new(u32::try_from(pos.saturating_add(1)).unwrap_or(u32::MAX))
}

/// R-150 : renumérotation des ID entre deux ordres. Rien si aucune tâche
/// présente avant n'a changé d'ID (ajout en fin, par exemple) ; sinon
/// `from` = premier ID dont l'occupant a changé, `count` = nombre de
/// positions, de `from` à la dernière position changée de l'ordre d'après.
pub(super) fn renumbered(before: &[TaskUid], after: &Tasks) -> Option<SideEffect> {
    let after: Vec<TaskUid> = order(after);
    let first = after
        .iter()
        .enumerate()
        .find(|&(p, a)| before.get(p) != Some(a))
        .map(|(p, _)| p)?;
    let last = after
        .iter()
        .enumerate()
        .rev()
        .find(|&(p, a)| before.get(p) != Some(a))
        .map_or(first, |(p, _)| p);
    // Une tâche d'avant présente à une position changée a changé d'ID.
    let existed: BTreeSet<TaskUid> = before.iter().copied().collect();
    let existing_moved = after
        .iter()
        .skip(first)
        .take(last.saturating_sub(first).saturating_add(1))
        .any(|u| existed.contains(u));
    if !existing_moved {
        return None;
    }
    Some(SideEffect::IdsRenumbered {
        from: id_of(first),
        count: last.saturating_sub(first).saturating_add(1),
    })
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use plannercraft_core::{OutlineLevel, ProjectOptions, Task, TaskId, TaskUid};

    use super::{Tasks, block_end, is_summary_at, order, prev_non_null, renumbered, summaries};
    use crate::outcome::SideEffect;

    /// Tâches `(uid, niveau, vide)`.
    fn tasks(spec: &[(u32, u16, bool)]) -> Tasks {
        let o = ProjectOptions::default();
        let v: Vec<Task> = spec
            .iter()
            .map(|&(u, l, null)| {
                let mut t = Task::new(TaskUid::new(u), format!("T{u}"), &o);
                t.outline_level = OutlineLevel::new(l).unwrap();
                t.is_null = null;
                t
            })
            .collect();
        Tasks::try_from(v).unwrap()
    }

    #[test]
    fn blocks_skip_trailing_null_rows() {
        // P(1) a(2) [vide 3] b(2) [vide 2] Q(1)
        let t = tasks(&[
            (1, 1, false),
            (2, 2, false),
            (3, 3, true),
            (4, 2, false),
            (5, 2, true),
            (6, 1, false),
        ]);
        assert_eq!(block_end(&t, 0), 4);
        assert_eq!(block_end(&t, 1), 2);
        assert_eq!(block_end(&t, 5), 6);
        assert_eq!(prev_non_null(&t, 4), Some(3));
        assert_eq!(prev_non_null(&t, 0), None);
        assert_eq!(summaries(&t).into_iter().collect::<Vec<_>>(), [TaskUid::new(1)]);
        let flags: Vec<bool> = (0..6).map(|q| is_summary_at(&t, q)).collect();
        assert_eq!(flags, [true, false, false, false, false, false]);
    }

    #[test]
    fn summary_skips_null_row_of_any_level() {
        // S(1) [vide 1] A(2) : S reste récapitulative (R-052).
        let t = tasks(&[(1, 1, false), (2, 1, true), (3, 2, false)]);
        assert!(is_summary_at(&t, 0));
        assert!(!is_summary_at(&t, 1));
        assert!(!is_summary_at(&t, 2));
        assert_eq!(summaries(&t).into_iter().collect::<Vec<_>>(), [TaskUid::new(1)]);
    }

    #[test]
    fn renumbering_ranges() {
        let before = tasks(&[(1, 1, false), (2, 1, false), (3, 1, false)]);
        let b = order(&before);
        // Ajout en fin : rien.
        let end = tasks(&[(1, 1, false), (2, 1, false), (3, 1, false), (4, 1, false)]);
        assert_eq!(renumbered(&b, &end), None);
        // Ajout en tête : 1 à 4.
        let head = tasks(&[(4, 1, false), (1, 1, false), (2, 1, false), (3, 1, false)]);
        assert_eq!(
            renumbered(&b, &head),
            Some(SideEffect::IdsRenumbered {
                from: TaskId::new(1),
                count: 4
            })
        );
        // Suppression de la 2 : la 3 devient 2.
        let del = tasks(&[(1, 1, false), (3, 1, false)]);
        assert_eq!(
            renumbered(&b, &del),
            Some(SideEffect::IdsRenumbered {
                from: TaskId::new(2),
                count: 1
            })
        );
        // Suppression de la dernière : rien.
        let last = tasks(&[(1, 1, false), (2, 1, false)]);
        assert_eq!(renumbered(&b, &last), None);
    }
}
