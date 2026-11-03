//! R-056 — Lien FS ou SS vers une récapitulative (porte) : la borne
//! s'applique comme plancher de début à toutes ses sous-tâches, à tout
//! niveau. Source : S24 ; observé O6 (A6 UID 9).

use plannercraft_core::{Driver, LinkType};
use plannercraft_tests::moment;
use plannercraft_tests::net::{Net, lag_w};

#[test]
fn r056_finish_to_start_gates_all_subtasks() {
    // X 2 j (lun – mar) FS → S {A 1 j, B 1 j} → A, B mer 08:00 – 17:00,
    // pilote : la porte de S.
    let run = Net::new("2026-10-19T08:00")
        .task("X", 960)
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .task("B", 480)
        .level("B", 2)
        .fs("X", "S")
        .run();
    for name in ["A", "B"] {
        assert_eq!(run.at(name).early.start, moment("2026-10-21T08:00"), "{name}");
        assert_eq!(run.at(name).early.finish, moment("2026-10-21T17:00"), "{name}");
        assert_eq!(
            run.at(name).driver,
            Driver::SummaryGate {
                summary: run.uid("S")
            }
        );
    }
    assert_eq!(run.at("S").early.start, moment("2026-10-21T08:00"));
}

#[test]
fn r056_start_to_start_gate_not_realigned() {
    // X SS+1 j → S : borne add(lun 08:00, 480) = lun 17:00 (instant non
    // recalé) → A, B mar 08:00 – 17:00.
    let run = Net::new("2026-10-19T08:00")
        .task("X", 960)
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .task("B", 480)
        .level("B", 2)
        .link("X", "S", LinkType::StartToStart, lag_w(480))
        .run();
    for name in ["A", "B"] {
        assert_eq!(run.at(name).early.start, moment("2026-10-20T08:00"), "{name}");
        assert_eq!(run.at(name).early.finish, moment("2026-10-20T17:00"), "{name}");
    }
}

#[test]
fn r056_gate_reaches_every_level() {
    // X 2 j FS → S1 {S2 {A 1 j}} → A mer 08:00 ; pilote : la porte la plus
    // proche qui porte la borne (S2 hérite de S1).
    let run = Net::new("2026-10-19T08:00")
        .task("X", 960)
        .task("S1", 0)
        .task("S2", 0)
        .level("S2", 2)
        .task("A", 480)
        .level("A", 3)
        .fs("X", "S1")
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-21T08:00"));
    assert_eq!(
        run.at("A").driver,
        Driver::SummaryGate {
            summary: run.uid("S2")
        }
    );
}

#[test]
fn r056_gate_and_own_link_larger_wins() {
    // X 1 j FS → S {A 1 j, B 1 j} ; Y 3 j FS → B : B suit Y (jeu 22/10),
    // A suit la porte (mar 20/10).
    let run = Net::new("2026-10-19T08:00")
        .task("X", 480)
        .task("Y", 1_440)
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .task("B", 480)
        .level("B", 2)
        .fs("X", "S")
        .fs("Y", "B")
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-20T08:00"));
    assert_eq!(run.at("B").early.start, moment("2026-10-22T08:00"));
    assert_eq!(
        run.at("B").driver,
        Driver::Link {
            predecessor: run.uid("Y"),
            link: LinkType::FinishToStart
        }
    );
}
