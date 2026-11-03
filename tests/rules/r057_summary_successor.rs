//! R-057 — Lien sortant d'une récapitulative : FS et FF lisent la fin
//! agrégée, et au retour leur plafond de fin s'applique à toutes ses
//! feuilles ; SS et SF lisent le début agrégé, et au retour leur plafond de
//! début s'applique aux seules feuilles dont le début au plus tôt égale
//! celui de la récapitulative (ADR-0009 A11). Source : S24 ; observé O6
//! (A6 UID 11 et 25) ; restriction SS/SF : hypothèse.

use plannercraft_core::{LinkType, Minutes};
use plannercraft_tests::moment;
use plannercraft_tests::net::{Net, lag_pct, lag_w};

#[test]
fn r057_finish_to_start_from_summary() {
    // S {A 1 j, B 2 j} FS → Y 1 j : Y mer 08:00 – 17:00 ; A LF mar 17:00,
    // TF 480, FS 480 (lien hérité de S) ; B TF 0.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .task("B", 960)
        .level("B", 2)
        .task("Y", 480)
        .fs("S", "Y")
        .run();
    assert_eq!(run.at("Y").early.start, moment("2026-10-21T08:00"));
    assert_eq!(run.at("A").late.finish, moment("2026-10-20T17:00"));
    assert_eq!(run.at("A").total_slack, Minutes::new(480));
    assert_eq!(run.at("A").free_slack, Minutes::new(480));
    assert_eq!(run.at("B").total_slack, Minutes::ZERO);
    assert_eq!(run.at("B").free_slack, Minutes::ZERO);
}

#[test]
fn r057_start_link_caps_same_start_leaves_only() {
    // S {A 1 j ; B FS de A 1 j} SS+1 j → Y 1 j : Y mar 08:00 – 17:00 ;
    // plafond de début h(LS_Y) = lun 08:00 sur A seule (même début que S)
    // → TF(A) = TF(B) = TF(Y) = 0. Sur toutes les feuilles, B aurait
    // TF = −480 sans aucune contrainte.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .task("B", 480)
        .level("B", 2)
        .fs("A", "B")
        .task("Y", 480)
        .link("S", "Y", LinkType::StartToStart, lag_w(480))
        .run();
    assert_eq!(run.at("Y").early.start, moment("2026-10-20T08:00"));
    for name in ["A", "B", "Y"] {
        assert_eq!(run.at(name).total_slack, Minutes::ZERO, "{name}");
        assert!(run.at(name).free_slack >= Minutes::ZERO, "{name}");
    }
}

#[test]
fn r057_percent_lag_uses_summary_duration() {
    // S {A 1 j, B 2 j} FS+50 % → Y : 50 % de la durée de S (960, R-054) =
    // 480 → add(mar 17:00, 480) = mer 17:00 → Y jeu 22/10 08:00.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .task("B", 960)
        .level("B", 2)
        .task("Y", 480)
        .link("S", "Y", LinkType::FinishToStart, lag_pct(50))
        .run();
    assert_eq!(run.at("Y").early.start, moment("2026-10-22T08:00"));
}

#[test]
fn r057_finish_to_finish_from_summary() {
    // S {A 1 j, B 2 j} FF → Y 1 j : borne de fin mar 17:00 → Y mar 08:00 –
    // 17:00 ; au retour, plafond de fin mar 17:00 sur A et B : TF(A) = 480,
    // TF(B) = 0.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .task("B", 960)
        .level("B", 2)
        .task("Y", 480)
        .link("S", "Y", LinkType::FinishToFinish, lag_w(0))
        .run();
    assert_eq!(run.at("Y").early.start, moment("2026-10-20T08:00"));
    assert_eq!(run.at("A").late.finish, moment("2026-10-20T17:00"));
    assert_eq!(run.at("A").total_slack, Minutes::new(480));
    assert_eq!(run.at("B").total_slack, Minutes::ZERO);
}
