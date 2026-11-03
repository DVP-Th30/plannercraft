//! R-151 — Allocation des UID de tâche dans une session.

use plannercraft_automation::error::codes;
use plannercraft_automation::{ApplyMode, ApplyOptions};
use plannercraft_core::{Task, TaskUid};
use plannercraft_tests::mspdi::new_project;
use plannercraft_tests::session::{add_task, commit, push_tasks, rename, session};

#[test]
fn r151_uid_monotone_after_undo() {
    // Tâches {1, 2, 7} → prochain 8 ; ajout (8), undo, ajout → 9.
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1, 2, 7], 1);
    let mut s = session(p);
    let o = commit(&mut s, "a", &[add_task("A", 1)]);
    assert_eq!(o.created, vec![TaskUid::new(8)]);
    s.undo().unwrap();
    assert!(s.project().tasks.get(TaskUid::new(8)).is_none());
    let o = commit(&mut s, "b", &[add_task("B", 1)]);
    assert_eq!(o.created, vec![TaskUid::new(9)]);
}

#[test]
fn r151_redo_reuses_post_image_uid() {
    // Le rétablissement réapplique la post-image : même UID ; le compteur
    // continue après.
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1], 1);
    let mut s = session(p);
    commit(&mut s, "a", &[add_task("A", 1), add_task("B", 1)]);
    s.undo().unwrap();
    s.redo().unwrap();
    let names: Vec<_> = s
        .project()
        .tasks
        .iter()
        .map(|t| (t.uid().get(), t.name.clone()))
        .collect();
    assert_eq!(
        names,
        vec![(1, "T1".to_owned()), (2, "A".to_owned()), (3, "B".to_owned())]
    );
    let o = commit(&mut s, "c", &[add_task("C", 1)]);
    assert_eq!(o.created, vec![TaskUid::new(4)]);
}

#[test]
fn r151_uid_overflow_refused() {
    // Fichier avec l'UID 4 294 967 295 → ajout refusé, état intact.
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[u32::MAX], 1);
    let mut s = session(p);
    let before = s.project().clone();
    let err = s
        .apply_edits(
            None,
            &[rename(u32::MAX, "X"), add_task("A", 1)],
            ApplyMode::Commit,
            ApplyOptions::default(),
        )
        .unwrap_err();
    assert_eq!(err.code, codes::LIMIT_EXCEEDED);
    assert_eq!(err.op_index, Some(1));
    assert_eq!(s.project(), &before);
    assert_eq!(s.history().entries.len(), 0);
}

#[test]
fn r151_uid_never_zero() {
    // Projet vide, avec la récapitulative du projet (UID 0) : premier UID 1.
    let mut p = new_project("2026-11-02T08:00");
    p.summary_task = Some(Task::new(TaskUid::PROJECT_SUMMARY, "P".to_owned(), &p.options));
    let mut s = session(p);
    let o = commit(&mut s, "a", &[add_task("A", 1)]);
    assert_eq!(o.created, vec![TaskUid::new(1)]);
}
