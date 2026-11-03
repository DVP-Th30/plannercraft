//! R-061 — Jalon et durée nulle dans le réseau : placement par R-031 dans les
//! deux passes ; l'indicateur `milestone` n'influe pas (seule la durée
//! compte). Observé O1 (A2 UID 1) ; A6 UID 4 et 38.

use plannercraft_core::Minutes;
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

#[test]
fn r061_milestone_between_tasks() {
    // A 1 j ; M FS 0 → lun 17:00 (fin de A, R-031) ; N FS de M 1 j →
    // mar 08:00 – 17:00. Au plus tard : M = backward_span(mar 08:00, 0) =
    // mar 08:00 (début de plage), TF 0 ; A LF lun 17:00.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("M", 0)
        .task("N", 480)
        .fs("A", "M")
        .fs("M", "N")
        .run();
    assert_eq!(run.at("M").early.start, moment("2026-10-19T17:00"));
    assert_eq!(run.at("M").early.finish, moment("2026-10-19T17:00"));
    assert_eq!(run.at("N").early.start, moment("2026-10-20T08:00"));
    assert_eq!(run.at("M").late.start, moment("2026-10-20T08:00"));
    assert_eq!(run.at("M").late.finish, moment("2026-10-20T08:00"));
    assert_eq!(run.at("M").total_slack, Minutes::ZERO);
    assert_eq!(run.at("A").late.finish, moment("2026-10-19T17:00"));
}

#[test]
fn r061_start_milestone_late_finish_o1() {
    // O1 : projet commençant mar 20/10 08:00, jalon sans prédécesseur au
    // début (mar 08:00), successeur N 1 j (LS mar 08:00) → LF du jalon
    // mar 08:00 (et non lun 17:00), TF 0.
    let run = Net::new("2026-10-20T08:00")
        .task("M", 0)
        .task("N", 480)
        .fs("M", "N")
        .run();
    assert_eq!(run.at("M").early.start, moment("2026-10-20T08:00"));
    assert_eq!(run.at("M").late.finish, moment("2026-10-20T08:00"));
    assert_eq!(run.at("M").total_slack, Minutes::ZERO);
}

#[test]
fn r061_milestone_flag_ignored() {
    // Drapeau jalon sur une tâche d'un jour : planifiée comme un jour.
    let run = Net::new("2026-10-19T08:00")
        .task("J", 480)
        .with_task("J", |t| t.milestone = true)
        .run();
    assert_eq!(run.at("J").early.finish, moment("2026-10-19T17:00"));
}
