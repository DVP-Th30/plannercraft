//! R-043 — Décalage en pourcentage : `L = percent_of(dur(p), pct)` (R-010)
//! consommé sur `σ` (`%` ouvré, `e%` civil). Sources : S21, S15 ; base pour
//! une récapitulative : hypothèse.

use plannercraft_core::LinkType;
use plannercraft_tests::moment;
use plannercraft_tests::net::{Net, lag_epct, lag_pct};

fn run(a: i64, kind: LinkType, lag: plannercraft_core::Lag) -> plannercraft_tests::net::Run {
    Net::new("2026-10-19T08:00")
        .task("A", a)
        .task("B", 480)
        .link("A", "B", kind, lag)
        .run()
}

#[test]
fn r043_percent_of_predecessor_duration() {
    // A 2 j ; B SS+50 % : 50 % de 960 = 480 → add(lun 08:00, 480) =
    // lun 17:00 → B mar 08:00 – 17:00.
    let r = run(960, LinkType::StartToStart, lag_pct(50));
    assert_eq!(r.at("B").early.start, moment("2026-10-20T08:00"));
    assert_eq!(r.at("B").early.finish, moment("2026-10-20T17:00"));
}

#[test]
fn r043_negative_percent() {
    // B FS −50 % : add(mar 17:00, −480) = mar 08:00 → B mar 08:00 – 17:00.
    let r = run(960, LinkType::FinishToStart, lag_pct(-50));
    assert_eq!(r.at("B").early.start, moment("2026-10-20T08:00"));
}

#[test]
fn r043_elapsed_percent() {
    // B SS+50e % : lun 08:00 + 480 min civiles = lun 16:00 → B lun 16:00 –
    // mar 16:00.
    let r = run(960, LinkType::StartToStart, lag_epct(50));
    assert_eq!(r.at("B").early.start, moment("2026-10-19T16:00"));
    assert_eq!(r.at("B").early.finish, moment("2026-10-20T16:00"));
}

#[test]
fn r043_percent_rounded_to_the_minute() {
    // A 1 j, B FS+33 % : 0,33 × 480 = 158,4 → 158 (R-001) ;
    // add(lun 17:00, 158) = mar 10:38 → B mar 10:38 – mer 10:38.
    let r = run(480, LinkType::FinishToStart, lag_pct(33));
    assert_eq!(r.at("B").early.start, moment("2026-10-20T10:38"));
    assert_eq!(r.at("B").early.finish, moment("2026-10-21T10:38"));
}
