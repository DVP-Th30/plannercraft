//! R-064 — Récapitulative manuelle : avec au moins une date saisie, elle
//! garde ses dates comme une feuille manuelle (R-063, durée saisie sinon
//! temps ouvré entre les dates, sur son calendrier) et ses successeurs les
//! lisent ; ses enfants sont planifiés normalement, la porte de ses liens
//! entrants s'applique à eux (S24) et ils n'héritent ni de ses liens
//! sortants ni de ceux de ses ancêtres ; ses dates au plus tard et sa marge
//! se calculent comme pour une feuille manuelle ; sans date : récapitulative
//! automatique. Source : observé O8 (A7 UID 1, 5, 6) ; reste : hypothèse.

use plannercraft_core::{Driver, Minutes, TaskRole};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

#[test]
fn r064_manual_summary_keeps_entered_dates() {
    // S manuelle mar 20/10 08:00 – ven 23/10 17:00 {A 1 j} : A lun 08:00 –
    // 17:00, S reste mar – ven ; Y FS de S 1 j → lun 26/10 08:00 – 17:00.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .manual("S", Some("2026-10-20T08:00"), Some("2026-10-23T17:00"))
        .task("A", 480)
        .level("A", 2)
        .task("Y", 480)
        .fs("S", "Y")
        .run();
    let s = run.at("S");
    assert_eq!(s.role, TaskRole::Summary);
    assert!(s.manual);
    assert_eq!(s.driver, Driver::Manual);
    assert_eq!(s.early.start, moment("2026-10-20T08:00"));
    assert_eq!(s.early.finish, moment("2026-10-23T17:00"));
    // Durée utilisée : temps ouvré entre les dates, 4 j = 1 920.
    assert_eq!(s.duration, Minutes::new(1_920));
    assert_eq!(run.at("A").early.start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("Y").early.start, moment("2026-10-26T08:00"));
    assert_eq!(run.schedule.project_finish, moment("2026-10-26T17:00"));
    // Au plus tard, comme une feuille : plafond h(LS_Y) = lun 26/10 08:00
    // → backward_span(lun 08:00, 1 920) = mar 08:00 – ven 17:00, TF 0.
    assert_eq!(s.late.start, moment("2026-10-20T08:00"));
    assert_eq!(s.total_slack, Minutes::ZERO);
    assert!(s.critical);
    // A n'hérite pas du lien S → Y (les dates de S ne dépendent pas d'elle) :
    // LF = fin du projet → TF = between(lun 19 17:00, lun 26 17:00) = 2 400.
    assert_eq!(run.at("A").late.finish, moment("2026-10-26T17:00"));
    assert_eq!(run.at("A").total_slack, Minutes::new(2_400));
    assert_eq!(run.at("A").free_slack, Minutes::new(2_400));
}

#[test]
fn r064_gate_applies_to_children() {
    // X 2 j FS → S manuelle lun 19/10 08:00 – 17:00 {A 1 j} : A suit la
    // porte (mer 21/10 08:00), S garde ses dates.
    let run = Net::new("2026-10-19T08:00")
        .task("X", 960)
        .task("S", 0)
        .manual("S", Some("2026-10-19T08:00"), Some("2026-10-19T17:00"))
        .task("A", 480)
        .level("A", 2)
        .fs("X", "S")
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-21T08:00"));
    assert_eq!(run.at("S").early.start, moment("2026-10-19T08:00"));
    // Marge libre de X : écart au début réel des sous-tâches (mer 08:00) = 0.
    assert_eq!(run.at("X").free_slack, Minutes::ZERO);
}

#[test]
fn r064_single_entered_date() {
    // S manuelle, début mer 21/10 08:00 seul, durée de la tâche 2 j → fin
    // jeu 22/10 17:00.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 960)
        .manual("S", Some("2026-10-21T08:00"), None)
        .task("A", 480)
        .level("A", 2)
        .run();
    assert_eq!(run.at("S").early.finish, moment("2026-10-22T17:00"));
    assert_eq!(run.at("S").duration, Minutes::new(960));
}

#[test]
fn r064_undated_manual_summary_is_automatic() {
    // S manuelle sans date {A 2 j} : récapitulative automatique, lun – mar.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .manual("S", None, None)
        .task("A", 960)
        .level("A", 2)
        .run();
    let s = run.at("S");
    assert!(!s.manual);
    assert_eq!(s.driver, Driver::Rollup);
    assert_eq!(s.early.finish, moment("2026-10-20T17:00"));
}

#[test]
fn r064_parent_aggregates_entered_dates() {
    // P {S manuelle mar 20/10 – ven 23/10 {A 1 j}} : P agrège les dates de S
    // (son enfant direct), pas celles de A.
    let run = Net::new("2026-10-19T08:00")
        .task("P", 0)
        .task("S", 0)
        .level("S", 2)
        .manual("S", Some("2026-10-20T08:00"), Some("2026-10-23T17:00"))
        .task("A", 480)
        .level("A", 3)
        .run();
    assert_eq!(run.at("P").early.start, moment("2026-10-20T08:00"));
    assert_eq!(run.at("P").early.finish, moment("2026-10-23T17:00"));
    assert_eq!(run.at("A").early.start, moment("2026-10-19T08:00"));
}
