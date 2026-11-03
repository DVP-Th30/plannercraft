//! R-037 — Fin à début (FS) : borne de début `g(EF_p)`. Sources : S8, S15.

use plannercraft_core::{Driver, LinkType};
use plannercraft_tests::moment;
use plannercraft_tests::net::{Net, lag_w};

#[test]
fn r037_fs_without_lag() {
    // A 2 j (lun 08:00 – mar 17:00) ; B FS 1 j : borne mar 17:00, recalée au
    // matin suivant → mer 21/10 08:00 – 17:00.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 960)
        .task("B", 480)
        .fs("A", "B")
        .run();
    assert_eq!(run.at("A").early.finish, moment("2026-10-20T17:00"));
    assert_eq!(run.at("B").early.start, moment("2026-10-21T08:00"));
    assert_eq!(run.at("B").early.finish, moment("2026-10-21T17:00"));
    assert_eq!(
        run.at("B").driver,
        Driver::Link {
            predecessor: run.uid("A"),
            link: LinkType::FinishToStart
        }
    );
}

#[test]
fn r037_fs_with_lag() {
    // B FS+1 j : add(mar 17:00, 480) = mer 17:00 → B jeu 22/10 08:00 – 17:00.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 960)
        .task("B", 480)
        .link("A", "B", LinkType::FinishToStart, lag_w(480))
        .run();
    assert_eq!(run.at("B").early.start, moment("2026-10-22T08:00"));
    assert_eq!(run.at("B").early.finish, moment("2026-10-22T17:00"));
}
