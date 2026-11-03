//! R-068 — Récapitulative du projet (UID 0) : agrégat des tâches actives
//! planifiées de premier niveau par R-053 à R-055 et R-060, sur le
//! calendrier du projet ; présente si une tâche est planifiée. Source : S8,
//! S23 ; observé A1 à A7 (UID 0).

use plannercraft_core::{Driver, Minutes, Percent, TaskRole, TaskUid};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

#[test]
fn r068_project_summary_of_n1() {
    // N1 : A 1 j ; B 2 j ; C 1 j FS de A et de B ; D 1 j seul →
    // lun 19/10 08:00 – mer 21/10 17:00, durée 1 440, TF 0, critique.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 960)
        .task("C", 480)
        .task("D", 480)
        .fs("A", "C")
        .fs("B", "C")
        .run();
    let ps = run.schedule.project_summary.unwrap();
    assert_eq!(run.schedule.scheduled(TaskUid::PROJECT_SUMMARY), Some(&ps));
    assert_eq!(ps.role, TaskRole::Summary);
    assert_eq!(ps.driver, Driver::Rollup);
    assert_eq!(ps.early.start, moment("2026-10-19T08:00"));
    assert_eq!(ps.early.finish, moment("2026-10-21T17:00"));
    assert_eq!(ps.late.finish, moment("2026-10-21T17:00"));
    assert_eq!(ps.duration, Minutes::new(1_440));
    assert_eq!(ps.total_slack, Minutes::ZERO);
    assert_eq!(ps.free_slack, Minutes::ZERO);
    assert!(ps.critical);
    // Pas dans la liste des tâches critiques (UID 0 hors de `tasks`).
    assert!(
        run.schedule
            .critical()
            .all(|(uid, _)| uid != TaskUid::PROJECT_SUMMARY)
    );
}

#[test]
fn r068_top_level_only_and_percent_over_leaves() {
    // S {A 960 à 50 %, B 480} ; C 240 à 100 % ; I inactive 5 j : l'agrégat
    // vient de S et C (I exclue) ; % = (480 + 240) / 1 680 → 4 286.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .task("A", 960)
        .level("A", 2)
        .with_task("A", |t| {
            t.progress.percent_complete = Percent::new(5_000).unwrap();
        })
        .task("B", 480)
        .level("B", 2)
        .task("C", 240)
        .with_task("C", |t| t.progress.percent_complete = Percent::FULL)
        .task("I", 2_400)
        .inactive("I")
        .run();
    let ps = run.schedule.project_summary.unwrap();
    assert_eq!(ps.early.finish, moment("2026-10-20T17:00"));
    assert_eq!(ps.percent_complete, Percent::new(4_286).unwrap());
}

#[test]
fn r068_absent_without_scheduled_task() {
    // Projet vide, ou seulement une tâche inactive : aucune récapitulative.
    assert!(
        Net::new("2026-10-19T08:00")
            .run()
            .schedule
            .project_summary
            .is_none()
    );
    let run = Net::new("2026-10-19T08:00").task("I", 480).inactive("I").run();
    assert!(run.schedule.project_summary.is_none());
    assert!(run.schedule.scheduled(TaskUid::PROJECT_SUMMARY).is_none());
}
