//! R-059 — Lien entre une récapitulative et sa descendante : ignoré,
//! `link.summary_descendant` (erreur), dans les deux sens. Source : Q7.

use plannercraft_core::{Diagnostic, Driver};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

#[test]
fn r059_link_from_subtask_to_its_summary_ignored() {
    // S {A 1 j}, lien A FS → S : ignoré, A lun 08:00 – 17:00.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 480)
        .task("A", 480)
        .level("A", 2)
        .fs("A", "S")
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("A").early.finish, moment("2026-10-19T17:00"));
    assert_eq!(run.at("A").driver, Driver::ProjectStart);
    assert!(
        run.schedule
            .diagnostics
            .contains(&Diagnostic::LinkSummaryDescendant {
                predecessor: run.uid("A"),
                successor: run.uid("S"),
            })
    );
}

#[test]
fn r059_link_from_summary_to_its_subtask_ignored() {
    // Lien S FS → A, dans l'autre sens : ignoré aussi.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 480)
        .task("A", 480)
        .level("A", 2)
        .fs("S", "A")
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-19T08:00"));
    assert!(run.codes().contains(&"link.summary_descendant"));
}
