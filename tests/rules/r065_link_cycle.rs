//! R-065 — Cycles : dans chaque composante fortement connexe, les liens qui
//! remontent la liste sont coupés ; un `link.cycle` par composante, un
//! `link.ignored_in_cycle` par lien coupé ; jamais de panique. Hypothèse
//! (MS Project refuse les relations circulaires à la saisie).

use plannercraft_core::{Diagnostic, TaskUid};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

#[test]
fn r065_cycle_cut_on_backward_link() {
    // UID = ID : A (1), B (2), C (3), X (4) ; liens A → B, B → A, B → C,
    // X → A (FS, 1 j chacune). B → A remonte la liste : coupé. X lun,
    // A mar, B mer, C jeu.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 480)
        .task("C", 480)
        .task("X", 480)
        .fs("A", "B")
        .fs("B", "A")
        .fs("B", "C")
        .fs("X", "A")
        .run();
    assert_eq!(run.at("X").early.start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("A").early.start, moment("2026-10-20T08:00"));
    assert_eq!(run.at("B").early.start, moment("2026-10-21T08:00"));
    assert_eq!(run.at("C").early.start, moment("2026-10-22T08:00"));
    let d = &run.schedule.diagnostics;
    assert!(d.contains(&Diagnostic::LinkCycle {
        tasks: vec![TaskUid::new(1), TaskUid::new(2)]
    }));
    assert!(d.contains(&Diagnostic::LinkIgnoredInCycle {
        predecessor: TaskUid::new(2),
        successor: TaskUid::new(1)
    }));
    assert_eq!(run.codes().iter().filter(|c| **c == "link.cycle").count(), 1);
}

#[test]
fn r065_two_components() {
    // Deux cycles indépendants : A ⇄ B et C → D → E → C. Coupés : B → A et
    // E → C ; un diagnostic `link.cycle` par composante.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 480)
        .task("C", 480)
        .task("D", 480)
        .task("E", 480)
        .fs("A", "B")
        .fs("B", "A")
        .fs("C", "D")
        .fs("D", "E")
        .fs("E", "C")
        .run();
    let cycles: Vec<&Diagnostic> = run
        .schedule
        .diagnostics
        .iter()
        .filter(|d| d.code() == "link.cycle")
        .collect();
    assert_eq!(cycles.len(), 2);
    assert!(
        run.schedule
            .diagnostics
            .contains(&Diagnostic::LinkIgnoredInCycle {
                predecessor: TaskUid::new(5),
                successor: TaskUid::new(3)
            })
    );
    // C lun, D mar, E mer.
    assert_eq!(run.at("E").early.start, moment("2026-10-21T08:00"));
}
