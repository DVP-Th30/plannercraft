//! R-221 — Ligne de base : portée. `Project` : toutes les tâches planifiées
//! (UID 0 compris quand le modèle a une récapitulative du projet), leurs
//! affectations, toutes les ressources ; `Tasks` : ces tâches et leurs
//! affectations, ressources non touchées, UID 0 seulement s'il est cité.
//! Lignes vides et tâches exclues du calcul sautées (`skipped`, d'où
//! `baseline.task_skipped`). Source : S58 (« Entire Project », « Selected
//! tasks ») ; cas limites : hypothèse.

use plannercraft_core::baseline::{self, BaselineError, Rollup, Scope};
use plannercraft_core::{AssignmentUid, TaskUid};
use plannercraft_tests::net::Net;

use crate::r171_assignment_effective_calendar::{assign, resource};
use crate::r220_baseline_values::{number, plan, spec, with_root};

#[test]
fn r221_selection_scope() {
    // T et U affectées à R1 (affectations 1 et 2) : la sélection {U} ne
    // capture que U et l'affectation 2, ni ressource ni UID 0.
    let net = Net::new("2026-10-19T08:00").task("T", 480).task("U", 480);
    let net = assign(
        assign(resource(with_root(net), 1, None), 1, "T", 1, 480),
        2,
        "U",
        1,
        480,
    );
    let (t, u) = (net.uid("T"), net.uid("U"));
    let (p, s) = plan(net);
    let edit = baseline::capture(&p, &s, &spec(0, Scope::Tasks(vec![u, u]), Rollup::None, false)).unwrap();
    assert_eq!(edit.tasks.keys().copied().collect::<Vec<_>>(), vec![u]);
    assert_eq!(
        edit.assignments.keys().copied().collect::<Vec<_>>(),
        vec![AssignmentUid::new(2)]
    );
    assert!(edit.resources.is_empty());
    // UID 0 cité : capturé.
    let edit = baseline::capture(
        &p,
        &s,
        &spec(
            0,
            Scope::Tasks(vec![u, TaskUid::PROJECT_SUMMARY]),
            Rollup::None,
            false,
        ),
    )
    .unwrap();
    assert_eq!(
        edit.tasks.keys().copied().collect::<Vec<_>>(),
        vec![TaskUid::PROJECT_SUMMARY, u]
    );
    // Projet entier : les deux tâches, l'UID 0, les deux affectations, R1.
    let edit = baseline::capture(&p, &s, &spec(0, Scope::Project, Rollup::None, false)).unwrap();
    assert_eq!(
        edit.tasks.keys().copied().collect::<Vec<_>>(),
        vec![TaskUid::PROJECT_SUMMARY, t, u]
    );
    assert_eq!(edit.assignments.len(), 2);
    assert_eq!(edit.resources.len(), 1);
    // Tâche inconnue : refus.
    assert_eq!(
        baseline::capture(
            &p,
            &s,
            &spec(0, Scope::Tasks(vec![TaskUid::new(99)]), Rollup::None, false)
        ),
        Err(BaselineError::UnknownTask(TaskUid::new(99)))
    );
}

#[test]
fn r221_skipped_tasks() {
    // Ligne vide N et tâche inactive I : sautées, sans ligne ; sans
    // récapitulative du projet dans le modèle, l'UID 0 cité est sauté.
    let net = Net::new("2026-10-19T08:00")
        .task("T", 480)
        .null_row("N")
        .task("I", 480)
        .inactive("I");
    let (kept, null, inactive) = (net.uid("T"), net.uid("N"), net.uid("I"));
    let (mut p, s) = plan(net);
    let edit = baseline::capture(&p, &s, &spec(0, Scope::Project, Rollup::None, false)).unwrap();
    assert_eq!(edit.tasks.keys().copied().collect::<Vec<_>>(), vec![kept]);
    assert_eq!(edit.skipped, vec![null, inactive]);
    let edit = baseline::capture(
        &p,
        &s,
        &spec(
            0,
            Scope::Tasks(vec![TaskUid::PROJECT_SUMMARY, inactive]),
            Rollup::None,
            false,
        ),
    )
    .unwrap();
    assert!(edit.tasks.is_empty());
    assert_eq!(edit.skipped, vec![TaskUid::PROJECT_SUMMARY, inactive]);
    // L'écriture n'invente pas de récapitulative du projet.
    let _ = baseline::apply(&mut p, number(0), edit);
    assert!(p.summary_task.is_none());
    assert!(p.tasks.get(inactive).unwrap().baselines.is_empty());
}
