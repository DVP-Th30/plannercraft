//! R-039 — Fin à fin (FF) : borne de fin `b = g(EF_p)`, convertie en borne de
//! début `P(s).backward_span(b, d).start`. Source : S8 ; conversion observée
//! (O7, A6 UID 38).

use plannercraft_core::{Driver, LinkType};
use plannercraft_tests::moment;
use plannercraft_tests::net::{Net, lag_w};

fn run(b_minutes: i64) -> plannercraft_tests::net::Run {
    Net::new("2026-10-19T08:00")
        .task("A", 960)
        .task("B", b_minutes)
        .link("A", "B", LinkType::FinishToFinish, lag_w(0))
        .run()
}

#[test]
fn r039_ff_converts_finish_bound() {
    // A 2 j (fin mar 17:00) ; B FF 1 j : backward_span(mar 17:00, 480) =
    // mar 08:00 – 17:00.
    let r = run(480);
    assert_eq!(r.at("B").early.start, moment("2026-10-20T08:00"));
    assert_eq!(r.at("B").early.finish, moment("2026-10-20T17:00"));
    assert_eq!(
        r.at("B").driver,
        Driver::Link {
            predecessor: r.uid("A"),
            link: LinkType::FinishToFinish
        }
    );
}

#[test]
fn r039_ff_bound_before_project_start() {
    // B FF 3 j : backward_span(mar 17:00, 1 440) commence ven 16/10 08:00,
    // avant le début du projet → plancher : B lun 08:00 – mer 21/10 17:00.
    let r = run(1_440);
    assert_eq!(r.at("B").early.start, moment("2026-10-19T08:00"));
    assert_eq!(r.at("B").early.finish, moment("2026-10-21T17:00"));
    assert_eq!(r.at("B").driver, Driver::ProjectStart);
}
