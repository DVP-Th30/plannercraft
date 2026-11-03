//! R-048 — Marge totale : `min(M.between(ES, LS), M.between(EF, LF))`, M =
//! calendrier effectif, temps civil pour une durée écoulée. Sources : S16,
//! S18, Q5 ; dixièmes de minute dans MSPDI : O2.

use plannercraft_core::Minutes;
use plannercraft_tests::net::Net;

#[test]
fn r048_total_slack_n1() {
    // N1 : A 480 (8 h), B 0, C 0, D 960 (2 j).
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 960)
        .task("C", 480)
        .task("D", 480)
        .fs("A", "C")
        .fs("B", "C")
        .run();
    assert_eq!(run.at("A").total_slack, Minutes::new(480));
    assert_eq!(run.at("B").total_slack, Minutes::ZERO);
    assert_eq!(run.at("C").total_slack, Minutes::ZERO);
    assert_eq!(run.at("D").total_slack, Minutes::new(960));
}

#[test]
fn r048_total_slack_on_task_calendar() {
    // A (24 heures) 1 j = lun 08:00 – 16:00 ; B 2 j (Standard) lun – mar :
    // fin du projet mar 17:00. A LF = mar 17:00 sur 24 heures, LS =
    // mar 09:00 : marge mesurée en temps ouvré de 24 heures = 25 h = 1 500.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .calendar("A", 2)
        .task("B", 960)
        .run();
    assert_eq!(run.at("A").total_slack, Minutes::new(1_500));
}
