//! R-183 — Tâche manuelle ou inactive affectée. Manuelle (feuille ou
//! récapitulative) : dates saisies, chaque affectation couvre le span de la
//! tâche, `effective_units` = W ÷ temps ouvré du span (A7 : 69 h sur 60 h ⇒
//! 115 %) ; inactive : aucune `AssignmentSchedule`. Source : R-063, R-062 ;
//! O31.

use plannercraft_core::{AssignmentRole, AssignmentUid, Units};
use plannercraft_tests::net::Net;

use crate::r171_assignment_effective_calendar::{assign, placed, resource};

#[test]
fn r183_manual_effective_units() {
    // Manuelle lun 19/10 08:00 – mar 20/10 17:00 (16 h ouvrées), R1 24 h ⇒
    // span de la tâche, débit 150 %.
    let net = Net::new("2026-10-19T08:00").task("A", 960).manual(
        "A",
        Some("2026-10-19T08:00"),
        Some("2026-10-20T17:00"),
    );
    let run = assign(resource(net, 1, None), 1, "A", 1, 1_440).run();
    let a = placed(&run, 1);
    assert_eq!(a.role, AssignmentRole::Manual);
    assert_eq!(a.span, run.at("A").early);
    assert_eq!(a.effective_units, Units::new(15_000).unwrap());
}

#[test]
fn r183_manual_summary_compressed() {
    // Récapitulative manuelle lun – ven 23/10 (40 h), R1 46 h ⇒ rôle
    // manuel, débit 115 % (comme A7 R1 : 69 h sur 60 h).
    let net = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .manual("S", Some("2026-10-19T08:00"), Some("2026-10-23T17:00"));
    let run = assign(resource(net, 1, None), 1, "S", 1, 2_760).run();
    let a = placed(&run, 1);
    assert_eq!(a.role, AssignmentRole::Manual);
    assert_eq!(a.effective_units, Units::new(11_500).unwrap());
}

#[test]
fn r183_inactive_no_assignment_schedule() {
    let net = Net::new("2026-10-19T08:00").task("A", 480).inactive("A");
    let run = assign(resource(net, 1, None), 1, "A", 1, 480).run();
    assert!(!run.schedule.assignments.contains_key(&AssignmentUid::new(1)));
}
