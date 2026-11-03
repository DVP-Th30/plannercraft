//! R-098 — Doit finir le (MFO) : option active, épingle sur `A = (P.add(f*,
//! −d), f*)` (R-090) ; option inactive : fenêtre (plancher de fin, plafond
//! `f*`). Sources : S27 (« EF,LF,SF=CD »), S8, S30 (exemple MFO : « Project
//! warns us of the scheduling conflict, and then overlaps the tasks »).

use plannercraft_core::{Constraint, Minutes};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

fn mfo(d: &str) -> Constraint {
    Constraint::MustFinishOn { date: moment(d) }
}

#[test]
fn r098_pinned_on_the_finish() {
    // A 1 j MFO mer 21/10 17:00 → mer 08:00 – 17:00, TF 0.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .constraint("A", mfo("2026-10-21T17:00"))
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-21T08:00"));
    assert_eq!(run.at("A").early.finish, moment("2026-10-21T17:00"));
    assert_eq!(run.at("A").total_slack, Minutes::ZERO);
}

#[test]
fn r098_finish_at_a_range_start() {
    // MFO jeu 22/10 08:00 : f* = jeu 08:00 (début d'une plage) → A mer 08:00
    // – jeu 08:00 ; B FS → jeu 08:00 – 17:00.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 480)
        .fs("A", "B")
        .constraint("A", mfo("2026-10-22T08:00"))
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-21T08:00"));
    assert_eq!(run.at("A").early.finish, moment("2026-10-22T08:00"));
    assert_eq!(run.at("B").early.start, moment("2026-10-22T08:00"));
    assert_eq!(run.at("B").early.finish, moment("2026-10-22T17:00"));
}

#[test]
fn r098_conflict_with_a_link() {
    // P 3 j FS → A 1 j MFO mar 20/10 17:00 → A mar 08:00 – 17:00 ;
    // TF(P) −960 ; un override.
    let run = Net::new("2026-10-19T08:00")
        .task("P", 1_440)
        .task("A", 480)
        .fs("P", "A")
        .constraint("A", mfo("2026-10-20T17:00"))
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-20T08:00"));
    assert_eq!(run.at("P").total_slack, Minutes::new(-960));
    assert_eq!(run.codes(), ["constraint.override"]);
    // Option inactive : fenêtre, A jeu 22/10 (le lien), `not_honored`.
    let run = Net::new("2026-10-19T08:00")
        .task("P", 1_440)
        .task("A", 480)
        .fs("P", "A")
        .constraint("A", mfo("2026-10-20T17:00"))
        .honor(false)
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-22T08:00"));
    assert_eq!(run.at("A").total_slack, Minutes::new(-960));
    assert_eq!(run.codes(), ["constraint.not_honored"]);
}
