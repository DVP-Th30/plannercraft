//! Aides des tests de la session d'édition (R-151 à R-164, lot J4 L2) :
//! opérations de test passées au pipeline par `Session::apply_edits`, en
//! attendant les commandes réelles (L4, L5).

use plannercraft_automation::error::CommandError;
use plannercraft_automation::session::{Edit, EditCtx};
use plannercraft_automation::text::Language;
use plannercraft_automation::{ApplyMode, ApplyOptions, ApplyOutcome, DocOrigin, Session};
use plannercraft_core::{Duration, DurationUnit, Link, Minutes, Project, Subject, Task, TaskUid, TimeBasis};

use crate::moment;

/// Session sur `p` (« maintenant » : 17/10/2026 08:00, français).
#[must_use]
pub fn session(p: Project) -> Session {
    let origin = DocOrigin {
        name: "P".to_owned(),
        path: None,
    };
    Session::from_project(p, origin, moment("2026-10-17T08:00"), Language::Fr)
}

/// Durée de `n` jours ouvrés (480 min), non estimée.
///
/// # Panics
///
/// Si la durée sort du domaine (jamais pour des tailles de test).
#[must_use]
pub fn days(n: i64) -> Duration {
    match Duration::new(
        Minutes::new(n.saturating_mul(480)),
        DurationUnit::Day,
        TimeBasis::Working,
        false,
    ) {
        Ok(d) => d,
        Err(e) => panic!("durée de test invalide : {e}"),
    }
}

/// Tâche `uid` nommée `T<uid>`, de `n` jours.
#[must_use]
pub fn task(p: &Project, uid: u32, n: i64) -> Task {
    let mut t = Task::new(TaskUid::new(uid), format!("T{uid}"), &p.options);
    t.duration = days(n);
    t
}

/// Ajoute à `p` les tâches `uids`, de `n` jours chacune, à la fin.
///
/// # Panics
///
/// Si un UID est en double.
pub fn push_tasks(p: &mut Project, uids: &[u32], n: i64) {
    for &u in uids {
        let t = task(p, u, n);
        if let Err(e) = p.tasks.push(t) {
            panic!("tâche de test {u} : {e}");
        }
    }
}

/// Ajoute le lien fin à début `pred → succ` au modèle (hors session).
///
/// # Panics
///
/// Si `succ` est absent.
pub fn link(p: &mut Project, pred: u32, succ: u32) {
    match p.tasks.get_mut(TaskUid::new(succ)) {
        Some(t) => t.predecessors.push(Link::finish_to_start(TaskUid::new(pred))),
        None => panic!("tâche de test {succ} absente"),
    }
}

/// Opération de test depuis une closure.
pub fn edit(f: impl Fn(&mut EditCtx<'_, '_>) -> Result<(), CommandError> + 'static) -> Edit {
    Box::new(f)
}

/// Erreur « tâche inconnue » d'une opération de test.
fn not_found(uid: u32) -> CommandError {
    CommandError::new(
        plannercraft_automation::error::codes::TASK_NOT_FOUND,
        plannercraft_automation::CommandErrorKind::UnknownTask(TaskUid::new(uid)),
    )
}

/// Ajoute une tâche de `n` jours à la fin, UID alloué par la session.
pub fn add_task(name: &str, n: i64) -> Edit {
    let name = name.to_owned();
    edit(move |c| {
        let uid = c.alloc_uid()?;
        let mut t = Task::new(uid, name.clone(), &c.project().options);
        t.duration = days(n);
        c.project().tasks.push(t).map_err(|_| not_found(uid.get()))?;
        c.touch(Subject::Task(uid));
        c.created(uid, None);
        Ok(())
    })
}

/// Renomme la tâche `uid`.
pub fn rename(uid: u32, name: &str) -> Edit {
    let name = name.to_owned();
    edit(move |c| {
        let t = c
            .project()
            .tasks
            .get_mut(TaskUid::new(uid))
            .ok_or_else(|| not_found(uid))?;
        t.name.clone_from(&name);
        c.touch(Subject::Task(TaskUid::new(uid)));
        Ok(())
    })
}

/// Fixe la durée de la tâche `uid` à `n` jours.
pub fn set_days(uid: u32, n: i64) -> Edit {
    edit(move |c| {
        let t = c
            .project()
            .tasks
            .get_mut(TaskUid::new(uid))
            .ok_or_else(|| not_found(uid))?;
        t.duration = days(n);
        c.touch(Subject::Task(TaskUid::new(uid)));
        Ok(())
    })
}

/// Ajoute le lien fin à début `pred → succ`.
pub fn add_link(pred: u32, succ: u32) -> Edit {
    edit(move |c| {
        let t = c
            .project()
            .tasks
            .get_mut(TaskUid::new(succ))
            .ok_or_else(|| not_found(succ))?;
        t.predecessors.push(Link::finish_to_start(TaskUid::new(pred)));
        c.touch(Subject::Link {
            predecessor: TaskUid::new(pred),
            successor: TaskUid::new(succ),
        });
        Ok(())
    })
}

/// Retire le lien `pred → succ`.
pub fn remove_link(pred: u32, succ: u32) -> Edit {
    edit(move |c| {
        let t = c
            .project()
            .tasks
            .get_mut(TaskUid::new(succ))
            .ok_or_else(|| not_found(succ))?;
        t.predecessors.retain(|l| l.predecessor != TaskUid::new(pred));
        c.touch(Subject::Link {
            predecessor: TaskUid::new(pred),
            successor: TaskUid::new(succ),
        });
        Ok(())
    })
}

/// Supprime la tâche `uid` (sans toucher aux liens qui la citent).
pub fn delete_task(uid: u32) -> Edit {
    edit(move |c| {
        c.project()
            .tasks
            .remove(TaskUid::new(uid))
            .ok_or_else(|| not_found(uid))?;
        c.touch(Subject::Task(TaskUid::new(uid)));
        Ok(())
    })
}

/// Opération toujours refusée (`param.invalid`).
pub fn fail() -> Edit {
    edit(|_| {
        Err(CommandError::new(
            plannercraft_automation::error::codes::PARAM_INVALID,
            plannercraft_automation::CommandErrorKind::InvalidParam("test".to_owned()),
        ))
    })
}

/// Commit d'opérations de test ; panique sur un refus.
///
/// # Panics
///
/// Si le lot est refusé.
pub fn commit(s: &mut Session, label: &str, edits: &[Edit]) -> ApplyOutcome {
    match s.apply_edits(
        Some(label.to_owned()),
        edits,
        ApplyMode::Commit,
        ApplyOptions::default(),
    ) {
        Ok(o) => o,
        Err(e) => panic!("lot de test « {label} » refusé : {e:?}"),
    }
}
