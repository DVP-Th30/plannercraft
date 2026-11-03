//! R-042 — Avance (décalage négatif) : un décalage négatif recule la borne
//! (convention de début, R-022). Source : S15 (S21 inverse le signe : erreur
//! de la page, consignée).

use plannercraft_core::LinkType;
use plannercraft_tests::moment;
use plannercraft_tests::net::{Net, lag_w};

fn run(lag: i64) -> plannercraft_tests::net::Run {
    Net::new("2026-10-19T08:00")
        .task("A", 960)
        .task("B", 480)
        .link("A", "B", LinkType::FinishToStart, lag_w(lag))
        .run()
}

#[test]
fn r042_lead_of_one_day() {
    // A 2 j (fin mar 17:00) ; B FS −1 j : add(mar 17:00, −480) = mar 08:00
    // → B mar 08:00 – 17:00.
    let r = run(-480);
    assert_eq!(r.at("B").early.start, moment("2026-10-20T08:00"));
    assert_eq!(r.at("B").early.finish, moment("2026-10-20T17:00"));
}

#[test]
fn r042_lead_of_two_hours() {
    // B FS −2 h : add(mar 17:00, −120) = mar 15:00 → B mar 15:00 – mer 15:00.
    let r = run(-120);
    assert_eq!(r.at("B").early.start, moment("2026-10-20T15:00"));
    assert_eq!(r.at("B").early.finish, moment("2026-10-21T15:00"));
}
