//! R-053 — Dates d'une récapitulative : début = min ES, fin = max EF des
//! enfants actifs planifiés (manuels compris) ; aucun enfant planifié →
//! `EmptySummary`. Source : S23, S8 ; observé O3 (A1 UID 108).

use plannercraft_core::{Driver, Exclusion, TaskOutcome, TaskRole};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

#[test]
fn r053_summary_spans_its_children() {
    // S {A 2 j ; B FS de A 1 j} : A lun 08:00 – mar 17:00, B mer 08:00 –
    // 17:00 → S lun 08:00 – mer 17:00.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .task("A", 960)
        .level("A", 2)
        .task("B", 480)
        .level("B", 2)
        .fs("A", "B")
        .run();
    let s = run.at("S");
    assert_eq!(s.role, TaskRole::Summary);
    assert_eq!(s.driver, Driver::Rollup);
    assert_eq!(s.early.start, moment("2026-10-19T08:00"));
    assert_eq!(s.early.finish, moment("2026-10-21T17:00"));
    // En J2, dates planifiées = dates au plus tôt.
    assert_eq!((s.start, s.finish), (s.early.start, s.early.finish));
}

#[test]
fn r053_manual_child_counts() {
    // S {A 1 j ; M manuelle jeu 22/10 08:00 – ven 23/10 17:00} : la
    // manuelle entre dans l'agrégat → S lun 08:00 – ven 17:00.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .task("M", 480)
        .level("M", 2)
        .manual("M", Some("2026-10-22T08:00"), Some("2026-10-23T17:00"))
        .run();
    assert_eq!(run.at("S").early.start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("S").early.finish, moment("2026-10-23T17:00"));
}

#[test]
fn r053_inactive_child_excluded_from_rollup() {
    // S {A 1 j, I inactive 3 j} : I hors de l'agrégat → S lun 08:00 – 17:00.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .task("I", 1_440)
        .level("I", 2)
        .inactive("I")
        .run();
    assert_eq!(run.at("S").early.finish, moment("2026-10-19T17:00"));
}

#[test]
fn r053_summary_without_scheduled_child_is_empty() {
    // S {I inactive} → EmptySummary ; un lien vers S est ignoré
    // (`link.excluded_endpoint`) et Y commence au début du projet.
    let run = Net::new("2026-10-19T08:00")
        .task("X", 480)
        .task("S", 0)
        .task("I", 480)
        .level("I", 2)
        .inactive("I")
        .task("Y", 480)
        .fs("X", "S")
        .fs("S", "Y")
        .run();
    assert_eq!(
        *run.outcome("S"),
        TaskOutcome::Excluded {
            reason: Exclusion::EmptySummary,
            display: None,
        }
    );
    assert_eq!(run.at("Y").early.start, moment("2026-10-19T08:00"));
    assert!(run.codes().contains(&"link.excluded_endpoint"));
}
