//! R-058 — Lien FF ou SF vers une récapitulative : déplié en borne de fin
//! sur chaque feuille active descendante, avec l'avertissement
//! `link.finish_to_summary`. Source : hypothèse (MS Project le refuserait
//! peut-être à la saisie).

use plannercraft_core::{Diagnostic, Limits, LinkType};
use plannercraft_tests::moment;
use plannercraft_tests::net::{Net, lag_w};

#[test]
fn r058_finish_to_finish_unfolded_on_leaves() {
    // X 3 j (lun – mer) FF → S {A 1 j, B 1 j} → A, B mer 08:00 – 17:00.
    let run = Net::new("2026-10-19T08:00")
        .task("X", 1_440)
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .task("B", 480)
        .level("B", 2)
        .link("X", "S", LinkType::FinishToFinish, lag_w(0))
        .run();
    for name in ["A", "B"] {
        assert_eq!(run.at(name).early.start, moment("2026-10-21T08:00"), "{name}");
        assert_eq!(run.at(name).early.finish, moment("2026-10-21T17:00"), "{name}");
    }
    assert!(
        run.schedule
            .diagnostics
            .contains(&Diagnostic::LinkFinishToSummary {
                predecessor: run.uid("X"),
                successor: run.uid("S"),
            })
    );
}

#[test]
fn r058_start_to_finish_unfolded_on_nested_leaves() {
    // X 1 j SF+2 j → S1 {S2 {A 1 j}, B 1 j} : borne de fin add(lun 08:00,
    // 960) = mar 17:00 → A et B mar 08:00 – 17:00 (feuilles à tout niveau).
    let run = Net::new("2026-10-19T08:00")
        .task("X", 480)
        .task("S1", 0)
        .task("S2", 0)
        .level("S2", 2)
        .task("A", 480)
        .level("A", 3)
        .task("B", 480)
        .level("B", 2)
        .link("X", "S1", LinkType::StartToFinish, lag_w(960))
        .run();
    for name in ["A", "B"] {
        assert_eq!(run.at(name).early.start, moment("2026-10-20T08:00"), "{name}");
    }
    assert!(run.codes().contains(&"link.finish_to_summary"));
}

#[test]
fn r058_expansion_budget_ignores_remaining_links() {
    // Budget abaissé à 3 arcs dépliés. X1 FF → S {A, B} (2 arcs) tient ;
    // X2 FF → S (2 arcs de plus) dépasse : ignoré, comme X3 FF → T {C}
    // (1 arc, qui tiendrait, mais les liens restants sont ignorés : jamais
    // de dépliage partiel ni de choix selon la taille). Un seul diagnostic,
    // avec le nombre de liens ignorés ; le calcul continue.
    let run = Net::new("2026-10-19T08:00")
        .task("X1", 480)
        .task("X2", 1_440)
        .task("X3", 1_440)
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .task("B", 480)
        .level("B", 2)
        .task("T", 0)
        .task("C", 480)
        .level("C", 2)
        .link("X1", "S", LinkType::FinishToFinish, lag_w(0))
        .link("X2", "S", LinkType::FinishToFinish, lag_w(0))
        .link("X3", "T", LinkType::FinishToFinish, lag_w(0))
        .run_with(Limits {
            expanded_arcs: 3,
            ..Limits::default()
        });
    let limits: Vec<_> = run
        .schedule
        .diagnostics
        .iter()
        .filter(|d| matches!(d, Diagnostic::LinkExpansionLimit { .. }))
        .collect();
    assert_eq!(limits, [&Diagnostic::LinkExpansionLimit { limit: 3, ignored: 2 }]);
    // X1 (lun) FF → A, B lun ; X2 (lun – mer) et X3 ignorés.
    for name in ["A", "B", "C"] {
        assert_eq!(run.at(name).early.start, moment("2026-10-19T08:00"), "{name}");
    }
    let finish_to_summary = run
        .schedule
        .diagnostics
        .iter()
        .filter(|d| matches!(d, Diagnostic::LinkFinishToSummary { .. }))
        .count();
    assert_eq!(finish_to_summary, 1);
}

#[test]
fn r058_expansion_budget_default_keeps_large_unfold() {
    // Budget par défaut : 50 liens FF vers une récapitulative de 50
    // feuilles (2 500 arcs) restent dépliés, sans `link.expansion_limit`.
    let mut net = Net::new("2026-10-19T08:00");
    for i in 0..50 {
        net = net.task(&format!("X{i}"), 480);
    }
    net = net.task("S", 0);
    for i in 0..50 {
        let leaf = format!("L{i}");
        net = net.task(&leaf, 480).level(&leaf, 2);
    }
    for i in 0..50 {
        net = net.link(&format!("X{i}"), "S", LinkType::FinishToFinish, lag_w(0));
    }
    let run = net.run();
    assert!(!run.codes().contains(&"link.expansion_limit"));
    assert_eq!(run.at("L49").early.start, moment("2026-10-19T08:00"));
}
