//! R-045 — Fin du projet : max des EF des tâches actives planifiées
//! (manuelles comprises, inactives exclues) ; sans tâche planifiée : début du
//! projet brut. Sources : S22, S16 ; projet vide : hypothèse.

use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

/// Réseau N1 : A 1 j ; B 2 j ; C 1 j FS de A et de B ; D 1 j seul.
fn n1() -> Net {
    Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 960)
        .task("C", 480)
        .task("D", 480)
        .fs("A", "C")
        .fs("B", "C")
}

#[test]
fn r045_max_early_finish() {
    // C mer 21/10 08:00 – 17:00 : fin du projet mer 17:00.
    let run = n1().run();
    assert_eq!(run.schedule.project_finish, moment("2026-10-21T17:00"));
}

#[test]
fn r045_empty_project() {
    // Aucune tâche : la fin du projet est son début, brut (sam 17/10 10:00).
    let run = Net::new("2026-10-17T10:00").run();
    assert_eq!(run.schedule.project_finish, moment("2026-10-17T10:00"));
    assert_eq!(run.schedule.tasks.len(), 0);
    assert_eq!(run.schedule.order.len(), 0);
}

#[test]
fn r045_finish_before_the_project_start() {
    // Max des fins au plus tôt, même avant le début du projet : M manuelle
    // ven 16/10 08:00 – 17:00 seule → fin du projet ven 16/10 17:00 (et non
    // le début du projet, lun 19/10 08:00).
    let run = Net::new("2026-10-19T08:00")
        .task("M", 480)
        .manual("M", Some("2026-10-16T08:00"), Some("2026-10-16T17:00"))
        .run();
    assert_eq!(run.schedule.project_finish, moment("2026-10-16T17:00"));
}
