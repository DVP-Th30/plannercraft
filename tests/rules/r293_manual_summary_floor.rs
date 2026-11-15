//! R-293 — Sous-tâches d'une récapitulative manuelle datée : leur plancher
//! de base est le début saisi de la plus proche récapitulative manuelle
//! datée qui les contient, à la place du début du projet, même avant lui ;
//! la porte de cette récapitulative est son début saisi seul (ses liens
//! entrants ne passent pas à ses enfants) ; les liens propres des enfants
//! et les portes des récapitulatives intermédiaires restent des planchers.
//! Source :
//! hypothèse fondée sur l'observation de fichiers `.mpp` réels publics (20
//! feuilles automatiques sans lien ni contrainte sous une récapitulative
//! manuelle, toutes au début saisi de celle-ci, dont 2 avant le début du
//! projet ; `docs/plans/mpp-import.md`, lot M2). Amende R-064.

use plannercraft_core::Driver;
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

#[test]
fn r293_children_start_at_the_entered_start() {
    // Début du projet lun 19/10 ; S manuelle mer 21/10 08:00 – ven 23/10
    // 17:00 {A 1 j} : A mer 08:00 – 17:00, pilotée par la porte de S.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .manual("S", Some("2026-10-21T08:00"), Some("2026-10-23T17:00"))
        .task("A", 480)
        .level("A", 2)
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-21T08:00"));
    assert_eq!(run.at("A").early.finish, moment("2026-10-21T17:00"));
    assert_eq!(run.at("S").early.start, moment("2026-10-21T08:00"));
}

#[test]
fn r293_before_the_project_start() {
    // S manuelle jeu 15/10 08:00 – ven 16/10 17:00, avant le début du
    // projet lun 19/10 : A suit S, jeu 15/10 08:00 ; B hors de S, lun 19/10.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .manual("S", Some("2026-10-15T08:00"), Some("2026-10-16T17:00"))
        .task("A", 480)
        .level("A", 2)
        .task("B", 480)
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-15T08:00"));
    assert_eq!(run.at("B").early.start, moment("2026-10-19T08:00"));
}

#[test]
fn r293_nested_summary_and_links() {
    // S manuelle mer 21/10 {T automatique {A 1 j}, C 1 j} ; X 4 j FS → C :
    // A mer 08:00 (sous T, plus proche manuelle S) ; C suit X, ven 23/10.
    let run = Net::new("2026-10-19T08:00")
        .task("X", 1_920)
        .task("S", 0)
        .manual("S", Some("2026-10-21T08:00"), Some("2026-10-23T17:00"))
        .task("T", 0)
        .level("T", 2)
        .task("A", 480)
        .level("A", 3)
        .task("C", 480)
        .level("C", 2)
        .fs("X", "C")
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-21T08:00"));
    assert_eq!(run.at("C").early.start, moment("2026-10-23T08:00"));
    assert!(matches!(run.at("C").driver, Driver::Link { .. }));
}

#[test]
fn r293_undated_manual_summary_keeps_the_project_start() {
    // S manuelle sans date : récapitulative automatique (R-064), A au début
    // du projet.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .manual("S", None, None)
        .task("A", 480)
        .level("A", 2)
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-19T08:00"));
}

#[test]
fn r293_links_into_the_manual_summary_are_not_inherited() {
    // X 4 j FS → S manuelle mar 20/10 {A 1 j} : S garde ses dates, A part
    // de mar 20/10 08:00 (et non de la fin de X, ven 23/10).
    let run = Net::new("2026-10-19T08:00")
        .task("X", 1_920)
        .task("S", 0)
        .manual("S", Some("2026-10-20T08:00"), Some("2026-10-21T17:00"))
        .task("A", 480)
        .level("A", 2)
        .fs("X", "S")
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-20T08:00"));
    assert_eq!(run.at("S").early.start, moment("2026-10-20T08:00"));
}
