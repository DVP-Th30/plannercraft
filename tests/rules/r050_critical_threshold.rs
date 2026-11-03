//! R-050 — Critique : `TF ≤ critical_slack_limit_days × minutes_per_day`.
//! Sources : S19, S20, S2 ; Q5 (seuil en durée : 1 j = 8 h).

use plannercraft_tests::net::Net;

fn n2(limit: u32) -> plannercraft_tests::net::Run {
    // N2 : TF(A) = TF(E) = 360, TF(B) = TF(C) = 0 (R-049).
    Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("E", 120)
        .task("B", 960)
        .task("C", 480)
        .fs("A", "E")
        .fs("E", "C")
        .fs("B", "C")
        .options(|o| o.critical_slack_limit_days = limit)
        .run()
}

fn critical(run: &plannercraft_tests::net::Run) -> Vec<&str> {
    let mut names: Vec<&str> = ["A", "E", "B", "C"]
        .into_iter()
        .filter(|n| run.at(n).critical)
        .collect();
    names.sort_unstable();
    names
}

#[test]
fn r050_default_threshold_zero() {
    let run = n2(0);
    assert_eq!(critical(&run), ["B", "C"]);
    // `Schedule::critical` les rend dans l'ordre des UID.
    let uids: Vec<_> = run.schedule.critical().map(|(u, _)| u).collect();
    assert_eq!(uids, [run.uid("B"), run.uid("C")]);
}

#[test]
fn r050_threshold_one_day() {
    // Seuil 1 j = 480 min : 360 ≤ 480, A et E deviennent critiques.
    let run = n2(1);
    assert_eq!(critical(&run), ["A", "B", "C", "E"]);
}
