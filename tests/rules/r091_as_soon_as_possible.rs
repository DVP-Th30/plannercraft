//! R-091 — Le plus tôt possible (ASAP) : depuis le début, aucune borne ;
//! c'est le comportement de J2. Sources : S27 (« (Scheduled)ES=(Calculated)ES
//! + Delay »), S28.

use plannercraft_core::{Constraint, Driver, LinkType, Minutes};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

#[test]
fn r091_asap_changes_nothing() {
    // Réseau N1 de R-045, toutes les tâches ASAP explicites : A lun, B lun –
    // mar, C mer, D lun ; TF A 480, B 0, C 0, D 960.
    let mut net = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 960)
        .task("C", 480)
        .task("D", 480)
        .fs("A", "C")
        .fs("B", "C");
    for name in ["A", "B", "C", "D"] {
        net = net.constraint(name, Constraint::AsSoonAsPossible);
    }
    let run = net.run();
    assert_eq!(run.at("A").early.start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("B").early.finish, moment("2026-10-20T17:00"));
    assert_eq!(run.at("C").early.start, moment("2026-10-21T08:00"));
    assert_eq!(run.at("D").early.start, moment("2026-10-19T08:00"));
    for (name, tf) in [("A", 480), ("B", 0), ("C", 0), ("D", 960)] {
        assert_eq!(run.at(name).total_slack, Minutes::new(tf), "{name}");
    }
    assert_eq!(
        run.at("C").driver,
        Driver::Link {
            predecessor: run.uid("B"),
            link: LinkType::FinishToStart
        }
    );
    assert_eq!(run.at("D").driver, Driver::ProjectStart);
    assert_eq!(run.codes(), [""; 0]);
}
