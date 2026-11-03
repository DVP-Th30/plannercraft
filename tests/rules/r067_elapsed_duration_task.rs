//! R-067 — Tâche à durée écoulée : placement et marges en temps civil
//! (R-028). Source : Q5.

use plannercraft_core::Minutes;
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

fn run(a_minutes: i64, limit: u32) -> plannercraft_tests::net::Run {
    Net::new("2026-10-19T08:00")
        .elapsed("A", a_minutes)
        .task("B", 480)
        .fs("A", "B")
        .options(|o| o.critical_slack_limit_days = limit)
        .run()
}

#[test]
fn r067_elapsed_task_slack_in_civil_time() {
    // A 57 eh (3 420) lun 08:00 → mer 21/10 17:00 ; B FS 1 j → jeu 08:00 –
    // 17:00. A LF = h(jeu 08:00) = jeu 08:00 : TF(A) = 15 h = 900 en temps
    // civil (et non 0 en temps ouvré).
    let r = run(3_420, 1);
    assert_eq!(r.at("A").early.finish, moment("2026-10-21T17:00"));
    assert_eq!(r.at("B").early.start, moment("2026-10-22T08:00"));
    assert_eq!(r.at("A").late.finish, moment("2026-10-22T08:00"));
    assert_eq!(r.at("A").total_slack, Minutes::new(900));
    // Seuil 1 j (480) : non critique ; seuil 2 j (960) : critique.
    assert!(!r.at("A").critical);
    assert!(run(3_420, 2).at("A").critical);
}

#[test]
fn r067_elapsed_task_over_weekend() {
    // A de 105 eh (6 300) : lun 08:00 → ven 23/10 17:00 ; B lun 26/10 08:00 ;
    // TF(A) = 63 h = 3 780.
    let r = run(6_300, 0);
    assert_eq!(r.at("A").early.finish, moment("2026-10-23T17:00"));
    assert_eq!(r.at("B").early.start, moment("2026-10-26T08:00"));
    assert_eq!(r.at("A").total_slack, Minutes::new(3_780));
    assert_eq!(r.at("A").free_slack, Minutes::new(3_780));
}
