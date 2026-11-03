//! Aides des tests de l'usage quotidien (J8, lot L2 : R-251 à R-255) :
//! projet « P » du repère de `docs/plans/j8.md` § 2 (calendrier Standard,
//! début lun 19/10/2026 08:00).
//!
//! UID = ID : 1 « Gros œuvre » (récapitulative) ⊃ 2 « Fondations » 5 j,
//! 3 « Murs » 10 j (2→3 FS), 4 « Livraison » jalon (3→4 FS) ; 5 « Second
//! œuvre » ⊃ 6 « Plâtrerie » 4 j (3→6 FS), 7 « Peinture » 3 j (6→7 FS) ;
//! chemin critique 2, 3, 6, 7.

use plannercraft_automation::Session;
use plannercraft_core::{Moment, TaskUid};
use serde_json::json;

use crate::resource_edit::fresh;
use crate::task_edit::apply;

/// Session sur le projet « P ».
#[must_use]
pub fn plan_p() -> Session {
    let mut s = fresh();
    apply(
        &mut s,
        json!([
            { "op": "task.add", "name": "Gros œuvre", "as": "$go" },
            { "op": "task.add", "name": "Fondations", "duration": { "text": "5j" }, "at": { "last_child_of": "$go" }, "as": "$f" },
            { "op": "task.add", "name": "Murs", "duration": { "text": "10j" }, "at": { "last_child_of": "$go" }, "as": "$m" },
            { "op": "task.add", "name": "Livraison", "duration": { "text": "0j" }, "at": { "last_child_of": "$go" }, "as": "$l" },
            { "op": "task.add", "name": "Second œuvre", "as": "$so" },
            { "op": "task.add", "name": "Plâtrerie", "duration": { "text": "4j" }, "at": { "last_child_of": "$so" }, "as": "$p" },
            { "op": "task.add", "name": "Peinture", "duration": { "text": "3j" }, "at": { "last_child_of": "$so" }, "as": "$pe" },
            { "op": "link.add", "pred": "$f", "succ": "$m" },
            { "op": "link.add", "pred": "$m", "succ": "$l" },
            { "op": "link.add", "pred": "$m", "succ": "$p" },
            { "op": "link.add", "pred": "$p", "succ": "$pe" }
        ]),
    );
    s
}

/// UID `n`.
#[must_use]
pub const fn uid(n: u32) -> TaskUid {
    TaskUid::new(n)
}

/// UID des tâches `ns`.
#[must_use]
pub fn uids(ns: &[u32]) -> Vec<TaskUid> {
    ns.iter().copied().map(TaskUid::new).collect()
}

/// Début et fin planifiés de la tâche `n`.
///
/// # Panics
///
/// Si elle n'est pas planifiée.
#[must_use]
pub fn dates(s: &Session, n: u32) -> (Moment, Moment) {
    match s.schedule().scheduled(TaskUid::new(n)) {
        Some(t) => (t.start, t.finish),
        None => panic!("tâche {n} non planifiée"),
    }
}
