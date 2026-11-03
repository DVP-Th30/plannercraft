//! R-069 — Conversion impossible : en passe avant → `Excluded {
//! Unschedulable }` + `task.unschedulable { Forward }` ; ses liens sortants
//! sont ignorés et ses successeurs, planifiés sans eux, ont `degraded`
//! (transitif) ; en passe arrière → `late = early`, `degraded`. Hypothèse
//! (règle 10).

use plannercraft_core::{Diagnostic, Exclusion, Pass, TaskOutcome};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

#[test]
fn r069_out_of_domain_task_excluded_and_successors_degraded() {
    // Début mer 31/12/2149 08:00 ; A 2 j : la fin sort du domaine →
    // `OutOfDomain { After }` → non planifiable ; B FS de A 1 j → mer 08:00
    // – 17:00, dégradée ; C FS de B, dégradée aussi.
    let run = Net::new("2149-12-31T08:00")
        .task("A", 960)
        .task("B", 480)
        .task("C", 0)
        .fs("A", "B")
        .fs("B", "C")
        .run();
    assert_eq!(
        *run.outcome("A"),
        TaskOutcome::Excluded {
            reason: Exclusion::Unschedulable,
            display: None
        }
    );
    assert!(run.schedule.diagnostics.contains(&Diagnostic::TaskUnschedulable {
        task: run.uid("A"),
        pass: Pass::Forward,
        cause: "calendar.out_of_domain".to_owned(),
    }));
    assert_eq!(run.at("B").early.start, moment("2149-12-31T08:00"));
    assert_eq!(run.at("B").early.finish, moment("2149-12-31T17:00"));
    assert!(run.at("B").degraded);
    assert!(run.at("C").degraded);
    assert!(run.codes().contains(&"link.excluded_endpoint"));
    // A n'apparaît pas dans l'ordre des tâches planifiées.
    assert!(!run.schedule.order.contains(&run.uid("A")));
}

#[test]
fn r069_failed_link_into_summary_is_dropped_not_the_summary() {
    // Début lun 29/12/2149 08:00 ; X 1 j FS+4 800 → S {A 1 j} : la borne
    // add(lun 17:00, 4 800) sort du domaine → le lien est ignoré
    // (`link.unschedulable`, erreur), sans `task.unschedulable`. La porte de
    // S garde ses autres bornes : Y 1 j FS → S place A au mar 30/12 08:00.
    // S et A restent planifiées, dégradées ; X n'a aucun successeur
    // effectif : LF(X) = fin du projet.
    let run = Net::new("2149-12-29T08:00")
        .task("X", 480)
        .task("Y", 480)
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .link(
            "X",
            "S",
            plannercraft_core::LinkType::FinishToStart,
            plannercraft_tests::net::lag_w(4_800),
        )
        .fs("Y", "S")
        .run();
    assert!(run.schedule.diagnostics.contains(&Diagnostic::LinkUnschedulable {
        predecessor: run.uid("X"),
        successor: run.uid("S"),
        cause: "calendar.out_of_domain".to_owned(),
    }));
    assert!(!run.codes().contains(&"task.unschedulable"), "{:?}", run.codes());
    assert_eq!(run.at("A").early.start, moment("2149-12-30T08:00"));
    assert!(run.at("A").degraded && run.at("S").degraded);
    assert_eq!(run.at("S").early, run.at("A").early);
    assert_eq!(run.at("X").late.finish, run.schedule.project_finish);
    assert!(!run.at("X").degraded);
}
