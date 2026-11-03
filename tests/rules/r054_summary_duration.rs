//! R-054 — Durée d'une récapitulative : temps ouvré entre son début et sa
//! fin sur le calendrier de la récapitulative, sinon du projet ; jamais la
//! somme des durées des enfants. Source : Q4, S8.

use plannercraft_core::Minutes;
use plannercraft_tests::moment;
use plannercraft_tests::net::{Net, lag_w};

#[test]
fn r054_duration_is_working_time_between_dates() {
    // S {A 1 j ; B FS+2 j de A 1 j} : A lun, B jeu 22/10 08:00 – 17:00 →
    // S lun 08:00 – jeu 17:00 = 4 j = 1 920 (la somme des durées ferait 960).
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .task("B", 480)
        .level("B", 2)
        .link("A", "B", plannercraft_core::LinkType::FinishToStart, lag_w(960))
        .run();
    assert_eq!(run.at("B").early.start, moment("2026-10-22T08:00"));
    assert_eq!(run.at("S").early.finish, moment("2026-10-22T17:00"));
    assert_eq!(run.at("S").duration, Minutes::new(1_920));
}

#[test]
fn r054_weekend_not_counted() {
    // Début ven 23/10 08:00, S {A 1 j ; B FS de A 1 j} → ven 08:00 – lun
    // 26/10 17:00 = 960 (le week-end n'est pas ouvré).
    let run = Net::new("2026-10-23T08:00")
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .task("B", 480)
        .level("B", 2)
        .fs("A", "B")
        .run();
    assert_eq!(run.at("S").early.finish, moment("2026-10-26T17:00"));
    assert_eq!(run.at("S").duration, Minutes::new(960));
}

#[test]
fn r054_summary_calendar_used() {
    // S (calendrier 24 heures) {A 1 j Standard} : A lun 08:00 – 17:00 ;
    // durée de S sur 24 heures = 540 (9 h civiles), pas 480.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .calendar("S", 2)
        .task("A", 480)
        .level("A", 2)
        .run();
    assert_eq!(run.at("A").early.finish, moment("2026-10-19T17:00"));
    assert_eq!(run.at("S").duration, Minutes::new(540));
}
