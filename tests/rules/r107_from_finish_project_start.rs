//! R-107 — Début du projet à rebours : `S*` = min des débuts au plus tard
//! des feuilles actives planifiées (manuelles comprises) et des
//! récapitulatives manuelles datées ; l'ancre `F` si aucune ; passe avant
//! secondaire depuis `S*` brut (dates au plus tôt, marges R-048 à R-050
//! inchangées). `Schedule::project_start` = `S*`, `project_finish` = `F`.
//! Source : miroir de R-036, R-045, R-046 (hypothèse).

use plannercraft_core::{Driver, Minutes, TaskUid};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

#[test]
fn r107_computed_start_and_secondary_forward_pass() {
    // Suite de R-106 : F ven 23/10 17:00 ; A 1 j FS → B 2 j ; C 1 j.
    // S* = mer 21/10 08:00 (LS de A) ; au plus tôt A mer, B jeu – ven, C
    // mer ; TF A 0, B 0, C 960 ; FS(C) 960 ; UID 0 mer 08:00 – ven 17:00,
    // durée 1 440.
    let run = Net::new("2026-10-19T08:00")
        .from_finish("2026-10-23T17:00")
        .task("A", 480)
        .task("B", 960)
        .task("C", 480)
        .fs("A", "B")
        .run();
    let s = &run.schedule;
    assert_eq!(s.project_start, moment("2026-10-21T08:00"));
    assert_eq!(s.project_finish, moment("2026-10-23T17:00"));
    assert_eq!(run.at("A").early.start, moment("2026-10-21T08:00"));
    assert_eq!(run.at("A").driver, Driver::ProjectStart);
    assert_eq!(run.at("B").early.start, moment("2026-10-22T08:00"));
    assert_eq!(run.at("B").early.finish, moment("2026-10-23T17:00"));
    assert_eq!(run.at("C").early.start, moment("2026-10-21T08:00"));
    for (name, tf) in [("A", 0), ("B", 0), ("C", 960)] {
        assert_eq!(run.at(name).total_slack, Minutes::new(tf), "{name}");
    }
    assert_eq!(run.at("C").free_slack, Minutes::new(960));
    assert!(!run.at("C").critical);
    let p = s.scheduled(TaskUid::PROJECT_SUMMARY).unwrap();
    assert_eq!(p.start, moment("2026-10-21T08:00"));
    assert_eq!(p.finish, moment("2026-10-23T17:00"));
    assert_eq!(p.duration, Minutes::new(1_440));
    assert_eq!(p.total_slack, Minutes::ZERO);
}

#[test]
fn r107_empty_project_starts_at_the_anchor() {
    let run = Net::new("2026-10-19T08:00").from_finish("2026-10-24T10:00").run();
    assert_eq!(run.schedule.project_start, moment("2026-10-24T10:00"));
    assert_eq!(run.schedule.project_finish, moment("2026-10-24T10:00"));
    assert!(run.schedule.project_summary.is_none());
}

#[test]
fn r107_manual_task_counts_for_the_computed_start() {
    // M manuelle lun 19/10 08:00 – 17:00 ; A 1 j ; F ven 23/10 17:00 :
    // S* = lun 08:00 (LS de M) ; A au plus tôt lun, au plus tard ven, TF
    // 1 920.
    let run = Net::new("2026-10-19T08:00")
        .from_finish("2026-10-23T17:00")
        .task("M", 480)
        .task("A", 480)
        .manual("M", Some("2026-10-19T08:00"), Some("2026-10-19T17:00"))
        .run();
    assert_eq!(run.schedule.project_start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("A").early.start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("A").start, moment("2026-10-23T08:00"));
    assert_eq!(run.at("A").total_slack, Minutes::new(1_920));
}
