//! R-040 — Début à fin (SF) : borne de fin `b = g(ES_p)`, convertie comme
//! R-039. Source : S8.

use plannercraft_core::{Driver, LinkType};
use plannercraft_tests::moment;
use plannercraft_tests::net::{Net, lag_w};

fn run(lag: i64) -> plannercraft_tests::net::Run {
    Net::new("2026-10-19T08:00")
        .task("A", 960)
        .task("B", 480)
        .link("A", "B", LinkType::StartToFinish, lag_w(lag))
        .run()
}

#[test]
fn r040_sf_with_lag() {
    // A 2 j (début lun 08:00) ; B SF+3 j 1 j : add(lun 08:00, 1 440) =
    // mer 17:00 → backward_span(mer 17:00, 480) = mer 08:00 – 17:00.
    let r = run(1_440);
    assert_eq!(r.at("B").early.start, moment("2026-10-21T08:00"));
    assert_eq!(r.at("B").early.finish, moment("2026-10-21T17:00"));
    assert_eq!(
        r.at("B").driver,
        Driver::Link {
            predecessor: r.uid("A"),
            link: LinkType::StartToFinish
        }
    );
}

#[test]
fn r040_sf_without_lag_hits_project_start() {
    // B SF+0 : borne lun 08:00 → backward_span = ven 16/10 08:00 – 17:00,
    // avant le début → plancher : B lun 08:00 – 17:00.
    let r = run(0);
    assert_eq!(r.at("B").early.start, moment("2026-10-19T08:00"));
    assert_eq!(r.at("B").early.finish, moment("2026-10-19T17:00"));
    assert_eq!(r.at("B").driver, Driver::ProjectStart);
}
