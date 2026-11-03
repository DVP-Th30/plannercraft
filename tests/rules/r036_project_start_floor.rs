//! R-036 — Début du projet, plancher de toute tâche automatique. Toute borne
//! de début est ≥ début du projet (brut) ; une tâche sans prédécesseur
//! commence au début du projet recalé sur son échelle ; une avance qui
//! ferait commencer avant est tronquée. Sources : S8, S22, S24 ; troncature :
//! hypothèse.

use plannercraft_core::{Driver, LinkType};
use plannercraft_tests::moment;
use plannercraft_tests::net::{Net, lag_w};

#[test]
fn r036_start_realigned_on_calendar() {
    // Début sam 17/10 00:00 (non ouvré).
    let run = Net::new("2026-10-17T00:00").task("A", 960).task("M", 0).run();
    // A 2 j : lun 19/10 08:00 → mar 20/10 17:00.
    assert_eq!(run.at("A").early.start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("A").early.finish, moment("2026-10-20T17:00"));
    assert_eq!(run.at("A").driver, Driver::ProjectStart);
    // Jalon sans lien : sam 00:00 ne termine aucune minute ouvrée → instant
    // ouvré suivant, lun 08:00 (R-031).
    assert_eq!(run.at("M").early.start, moment("2026-10-19T08:00"));
    // Le début du projet reste brut dans le planning.
    assert_eq!(run.schedule.project_start, moment("2026-10-17T00:00"));
}

#[test]
fn r036_lead_truncated_at_project_start() {
    // Début lun 08:00 ; A 1 j ; B 1 j SS −1 j de A : add(lun 08:00, −480) =
    // ven 16/10 08:00, avant le début du projet → B lun 08:00 – 17:00,
    // pilote : début du projet.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 480)
        .link("A", "B", LinkType::StartToStart, lag_w(-480))
        .run();
    assert_eq!(run.at("B").early.start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("B").early.finish, moment("2026-10-19T17:00"));
    assert_eq!(run.at("B").driver, Driver::ProjectStart);
}
