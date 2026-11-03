//! R-038 — Début à début (SS) : borne de début `g(ES_p)`. Source : S8.

use plannercraft_core::LinkType;
use plannercraft_tests::moment;
use plannercraft_tests::net::{Net, lag_w};

#[test]
fn r038_ss_with_lag() {
    // A 2 j lun 08:00 ; B SS+4 h 1 j : add(lun 08:00, 240) = lun 12:00,
    // recalé à 13:00 (pause) → B lun 13:00 – mar 12:00.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 960)
        .task("B", 480)
        .link("A", "B", LinkType::StartToStart, lag_w(240))
        .run();
    assert_eq!(run.at("B").early.start, moment("2026-10-19T13:00"));
    assert_eq!(run.at("B").early.finish, moment("2026-10-20T12:00"));
}

#[test]
fn r038_ss_without_lag() {
    // B SS 1 j : même début que A, lun 08:00 – 17:00.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 960)
        .task("B", 480)
        .link("A", "B", LinkType::StartToStart, lag_w(0))
        .run();
    assert_eq!(run.at("B").early.start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("B").early.finish, moment("2026-10-19T17:00"));
}
