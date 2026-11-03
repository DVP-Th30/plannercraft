//! R-049 — Marge libre : `FS = min(TF, min des écarts aux successeurs)`,
//! écart = `M.between(ancre propre, h(ancre au plus tôt du successeur))` ;
//! sans successeur : FS = TF ; aucune troncature. Source : S17 ; formule :
//! hypothèse ; observé O2.

use plannercraft_core::{LinkType, Minutes};
use plannercraft_tests::net::{Net, lag_w};

#[test]
fn r049_free_slack_n1() {
    // N1 : A = between(lun 17:00, mer 08:00) = 480 ; D sans successeur :
    // FS = TF = 960 ; B et C : 0.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 960)
        .task("C", 480)
        .task("D", 480)
        .fs("A", "C")
        .fs("B", "C")
        .run();
    assert_eq!(run.at("A").free_slack, Minutes::new(480));
    assert_eq!(run.at("D").free_slack, Minutes::new(960));
    assert_eq!(run.at("B").free_slack, Minutes::ZERO);
    assert_eq!(run.at("C").free_slack, Minutes::ZERO);
}

#[test]
fn r049_free_slack_n2() {
    // N2 : A 1 j → E 2 h → C ; B 2 j → C ; C 1 j. E mar 08:00 – 10:00,
    // C mer 08:00 – 17:00. E LS mar 15:00, TF 360, FS = between(mar 10:00,
    // mer 08:00) = 120 + 240 = 360. A LS lun 15:00, TF 360, FS 0 (E commence
    // le matin ouvré qui suit A).
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("E", 120)
        .task("B", 960)
        .task("C", 480)
        .fs("A", "E")
        .fs("E", "C")
        .fs("B", "C")
        .run();
    assert_eq!(run.at("E").total_slack, Minutes::new(360));
    assert_eq!(run.at("E").free_slack, Minutes::new(360));
    assert_eq!(run.at("A").total_slack, Minutes::new(360));
    assert_eq!(run.at("A").free_slack, Minutes::ZERO);
}

#[test]
fn r049_free_slack_with_lag() {
    // A 1 j ; B FS+1 j 1 j (mer 08:00) ; D 4 j seul (fin jeu 17:00).
    // Écart de A : h(ES_B = mer 08:00) = next(add(mer 08:00, −480)) =
    // mar 08:00 ; between(lun 17:00, mar 08:00) = 0.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 480)
        .task("D", 1_920)
        .link("A", "B", LinkType::FinishToStart, lag_w(480))
        .run();
    assert_eq!(run.at("A").free_slack, Minutes::ZERO);
    // TF(A) = TF(B) = 480 (B peut finir jeu 17:00).
    assert_eq!(run.at("A").total_slack, Minutes::new(480));
    assert_eq!(run.at("B").free_slack, Minutes::new(480));
}
