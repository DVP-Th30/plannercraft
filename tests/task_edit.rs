//! Aides des tests des commandes de tâches (R-152 à R-158, R-161, lot J4
//! L4) : lots JSON appliqués par `Session::apply`, projets hiérarchiques,
//! lecture de l'ordre et des niveaux.

use plannercraft_automation::{ApplyMode, ApplyOutcome, Batch, CommandError, Session};
use plannercraft_core::{OutlineLevel, Project, TaskUid};

use crate::session::task;

/// Lot JSON `{"format_version":1,"ops":ops}`.
///
/// # Panics
///
/// Si le JSON n'est pas un lot valide.
#[must_use]
pub fn batch(ops: serde_json::Value) -> Batch {
    let mut v = serde_json::Map::new();
    v.insert("format_version".to_owned(), serde_json::Value::from(1));
    v.insert("ops".to_owned(), ops);
    match Batch::from_json(serde_json::Value::Object(v)) {
        Ok(b) => b,
        Err(e) => panic!("lot de test illisible : {e:?}"),
    }
}

/// Applique le lot `ops` (commit).
///
/// # Errors
///
/// Le refus du lot.
pub fn try_apply(s: &mut Session, ops: serde_json::Value) -> Result<ApplyOutcome, CommandError> {
    s.apply(&batch(ops), ApplyMode::Commit)
}

/// Applique le lot `ops` (commit) ; panique sur un refus.
///
/// # Panics
///
/// Si le lot est refusé.
pub fn apply(s: &mut Session, ops: serde_json::Value) -> ApplyOutcome {
    match try_apply(s, ops) {
        Ok(o) => o,
        Err(e) => panic!("lot refusé : {e:?}"),
    }
}

/// Ajoute à `p` les tâches `(uid, niveau)`, d'un jour chacune, dans l'ordre.
///
/// # Panics
///
/// Si un UID est en double ou un niveau vaut 0.
pub fn push_outline(p: &mut Project, spec: &[(u32, u16)]) {
    for &(uid, level) in spec {
        let mut t = task(p, uid, 1);
        t.outline_level = match OutlineLevel::new(level) {
            Ok(l) => l,
            Err(e) => panic!("niveau de test {level} : {e}"),
        };
        if let Err(e) = p.tasks.push(t) {
            panic!("tâche de test {uid} : {e}");
        }
    }
}

/// Ordre des tâches : `(UID, niveau)`.
#[must_use]
pub fn rows(s: &Session) -> Vec<(u32, u16)> {
    s.project()
        .tasks
        .iter()
        .map(|t| (t.uid().get(), t.outline_level.get()))
        .collect()
}

/// Ordre des tâches : `(nom, niveau)`.
#[must_use]
pub fn names(s: &Session) -> Vec<(String, u16)> {
    s.project()
        .tasks
        .iter()
        .map(|t| (t.name.clone(), t.outline_level.get()))
        .collect()
}

/// Tâche `uid` de la session.
///
/// # Panics
///
/// Si la tâche est absente.
#[must_use]
pub fn get(s: &Session, uid: u32) -> &plannercraft_core::Task {
    match s.project().tasks.get(TaskUid::new(uid)) {
        Some(t) => t,
        None => panic!("tâche {uid} absente"),
    }
}
