//! Aides des tests des commandes `resource.*`, `assignment.*` et de
//! `task.update` étendu (R-195 à R-197, lot J5 L7) : projet neuf au repère
//! du cahier J5 (calendrier Standard, début lun 19/10/2026 08:00), lecture
//! des ressources, des affectations, des durées et du travail.

use plannercraft_automation::Session;
use plannercraft_core::{Assignment, Resource, ResourceUid, TaskUid};

use crate::mspdi::new_project;
use crate::session::session;
use crate::task_edit::apply;

/// Session sur un projet neuf commencé le lun 19/10/2026 08:00.
#[must_use]
pub fn fresh() -> Session {
    session(new_project("2026-10-19T08:00"))
}

/// Session neuve avec les tâches `(nom, durée en texte)`, UID 1, 2… dans
/// l'ordre, ajoutées par `task.add`.
#[must_use]
pub fn with_tasks(tasks: &[(&str, &str)]) -> Session {
    let mut s = fresh();
    let ops: Vec<serde_json::Value> = tasks
        .iter()
        .map(|(name, d)| serde_json::json!({ "op": "task.add", "name": name, "duration": { "text": d } }))
        .collect();
    apply(&mut s, serde_json::Value::Array(ops));
    s
}

/// Ressource `uid`.
///
/// # Panics
///
/// Si elle est absente.
#[must_use]
pub fn resource(s: &Session, uid: u32) -> &Resource {
    match s.project().resources.get(ResourceUid::new(uid)) {
        Some(r) => r,
        None => panic!("ressource {uid} absente"),
    }
}

/// Affectation de la ressource `resource` à la tâche `task`.
///
/// # Panics
///
/// Si elle est absente.
#[must_use]
pub fn assignment(s: &Session, task: u32, resource: u32) -> &Assignment {
    match s
        .project()
        .assignments
        .iter()
        .find(|a| a.task == TaskUid::new(task) && a.resource == Some(ResourceUid::new(resource)))
    {
        Some(a) => a,
        None => panic!("affectation {resource} → {task} absente"),
    }
}

/// `(unités, travail en minutes)` de l'affectation de `resource` à `task`.
#[must_use]
pub fn units_work(s: &Session, task: u32, resource: u32) -> (i64, i64) {
    let a = assignment(s, task, resource);
    (a.units.get(), a.work.minutes().get())
}

/// Durée enregistrée de la tâche `uid`, en minutes ouvrées.
#[must_use]
pub fn duration(s: &Session, uid: u32) -> i64 {
    crate::task_edit::get(s, uid).duration.minutes().get()
}

/// Travail enregistré de la tâche `uid`, en minutes.
#[must_use]
pub fn work(s: &Session, uid: u32) -> i64 {
    crate::task_edit::get(s, uid).work.minutes().get()
}
