//! R-160 — Refus d'un lot sur nouvelle erreur (delta de diagnostics).

use plannercraft_automation::error::codes;
use plannercraft_automation::{ApplyMode, ApplyOptions, CommandErrorKind};
use plannercraft_core::{Diagnostic, Link, Subject, TaskUid};
use plannercraft_tests::moment;
use plannercraft_tests::mspdi::new_project;
use plannercraft_tests::session::{add_link, commit, edit, link, push_tasks, remove_link, rename, session};

fn cycles(s: &plannercraft_automation::Session) -> Vec<Vec<u32>> {
    s.diagnostics()
        .iter()
        .filter_map(|d| match d {
            Diagnostic::LinkCycle { tasks } => Some(tasks.iter().map(|t| t.get()).collect()),
            _ => None,
        })
        .collect()
}

#[test]
fn r160_existing_error_not_blocking() {
    // Fichier importé avec `link.unknown_task` sur la tâche 12 ; renommage de
    // la tâche 4 → accepté, l'erreur héritée reste.
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &(1..=12).collect::<Vec<_>>(), 1);
    p.tasks
        .get_mut(TaskUid::new(12))
        .unwrap()
        .predecessors
        .push(Link::finish_to_start(TaskUid::new(99)));
    let mut s = session(p);
    assert!(s.diagnostics().iter().any(|d| d.code() == "link.unknown_task"));
    let o = commit(&mut s, "r", &[rename(4, "Quatre")]);
    assert!(o.committed);
    assert_eq!(s.project().tasks.get(TaskUid::new(4)).unwrap().name, "Quatre");
    assert!(s.diagnostics().iter().any(|d| d.code() == "link.unknown_task"));
}

#[test]
fn r160_new_cycle_refused_with_op_index() {
    // Projet sans cycle, 1 → 3 existe ; lot [renommer 2, lien 3 → 1] →
    // refus `batch.would_create_errors [link.cycle {1, 3}]`, op_index 1.
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1, 2, 3], 1);
    link(&mut p, 1, 3);
    let mut s = session(p);
    let before = s.project().clone();
    let generation = s.generation();
    let err = s
        .apply_edits(
            None,
            &[rename(2, "X"), add_link(3, 1)],
            ApplyMode::Commit,
            ApplyOptions::default(),
        )
        .unwrap_err();
    assert_eq!(err.code, codes::BATCH_WOULD_CREATE_ERRORS);
    assert_eq!(err.op_index, Some(1));
    let CommandErrorKind::WouldCreateErrors(errors) = &err.kind else {
        panic!("{err:?}");
    };
    assert!(errors.contains(&Diagnostic::LinkCycle {
        tasks: vec![TaskUid::new(1), TaskUid::new(3)]
    }));
    assert_eq!(s.project(), &before);
    assert_eq!(s.generation(), generation);
    assert_eq!(s.history().entries.len(), 0);
}

#[test]
fn r160_unblamed_error_points_to_last_op() {
    // Une erreur dont le sujet n'a été touché par aucune opération est
    // imputée à la dernière opération.
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1, 2, 3], 1);
    let mut s = session(p);
    let sneaky = edit(|c| {
        c.project()
            .tasks
            .get_mut(TaskUid::new(2))
            .unwrap()
            .predecessors
            .push(Link::finish_to_start(TaskUid::new(77)));
        Ok(())
    });
    let err = s
        .apply_edits(
            None,
            &[sneaky, rename(1, "A"), rename(3, "C")],
            ApplyMode::Commit,
            ApplyOptions::default(),
        )
        .unwrap_err();
    assert_eq!(err.code, codes::BATCH_WOULD_CREATE_ERRORS);
    assert_eq!(err.op_index, Some(2));
}

#[test]
fn r160_cycle_shrink_accepted() {
    // Cycle {5, 7, 9} existant (5→7, 7→9, 9→5, 9→7) ; retirer 9→5 laisse
    // {7, 9} : accepté (réparation par étapes).
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[5, 7, 9], 1);
    link(&mut p, 5, 7);
    link(&mut p, 7, 9);
    link(&mut p, 9, 5);
    link(&mut p, 9, 7);
    let mut s = session(p);
    assert_eq!(cycles(&s), vec![vec![5, 7, 9]]);
    let o = commit(&mut s, "r", &[remove_link(9, 5)]);
    assert!(o.committed);
    assert_eq!(cycles(&s), vec![vec![7, 9]]);
    // Agrandir le cycle de nouveau est refusé.
    let err = s
        .apply_edits(
            None,
            &[add_link(9, 5)],
            ApplyMode::Commit,
            ApplyOptions::default(),
        )
        .unwrap_err();
    assert_eq!(err.code, codes::BATCH_WOULD_CREATE_ERRORS);
    assert_eq!(err.subject, Some(Subject::Task(TaskUid::new(5))));
}

#[test]
fn r160_new_warning_returned() {
    // Échéance dépassée posée par le lot : avertissement rendu, lot accepté ;
    // un second lot ne le rend plus (il n'est plus nouveau).
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1], 3);
    let mut s = session(p);
    let deadline = edit(|c| {
        c.project().tasks.get_mut(TaskUid::new(1)).unwrap().deadline = Some(moment("2026-11-03T17:00"));
        c.touch(Subject::Task(TaskUid::new(1)));
        Ok(())
    });
    let o = commit(&mut s, "d", &[deadline]);
    assert!(o.committed);
    assert!(
        o.warnings.iter().any(|d| d.code() == "task.deadline_missed"),
        "{:?}",
        o.warnings
    );
    let o = commit(&mut s, "r", &[rename(1, "A")]);
    assert_eq!(o.warnings.len(), 0, "{:?}", o.warnings);
}
