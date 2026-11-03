//! R-041 — Calendrier du décalage. Décalage ouvré compté sur le calendrier
//! de tâche du successeur s'il en a un, sinon sur celui du projet (jamais
//! celui du prédécesseur, jamais l'échelle écoulée d'une durée écoulée) ;
//! décalage écoulé : temps civil. Source : S21.

use plannercraft_core::LinkType;
use plannercraft_tests::moment;
use plannercraft_tests::net::{Net, lag_e, lag_w};

#[test]
fn r041_lag_on_successor_task_calendar() {
    // A (Standard) lun 08:00 – 17:00 ; B (calendrier 24 heures, UID 2)
    // FS+1 j : add₂₄(lun 17:00, 480) = mar 01:00 → B mar 01:00 – 09:00.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 480)
        .calendar("B", 2)
        .link("A", "B", LinkType::FinishToStart, lag_w(480))
        .run();
    assert_eq!(run.at("B").early.start, moment("2026-10-20T01:00"));
    assert_eq!(run.at("B").early.finish, moment("2026-10-20T09:00"));
}

#[test]
fn r041_lag_on_project_calendar_not_predecessor() {
    // A (24 heures) 1 j = lun 08:00 – 16:00 ; B (sans calendrier) FS+1 j :
    // add_std(lun 16:00, 480) = mar 16:00 → B mar 16:00 – mer 16:00.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .calendar("A", 2)
        .task("B", 480)
        .link("A", "B", LinkType::FinishToStart, lag_w(480))
        .run();
    assert_eq!(run.at("A").early.finish, moment("2026-10-19T16:00"));
    assert_eq!(run.at("B").early.start, moment("2026-10-20T16:00"));
    assert_eq!(run.at("B").early.finish, moment("2026-10-21T16:00"));
}

#[test]
fn r041_elapsed_lag_is_civil_time() {
    // Début ven 23/10 08:00 ; A 1 j ; B FS+2 ej : ven 17:00 + 2 880 =
    // dim 25/10 17:00 → B lun 26/10 08:00 – 17:00.
    let run = Net::new("2026-10-23T08:00")
        .task("A", 480)
        .task("B", 480)
        .link("A", "B", LinkType::FinishToStart, lag_e(2_880))
        .run();
    assert_eq!(run.at("B").early.start, moment("2026-10-26T08:00"));
    assert_eq!(run.at("B").early.finish, moment("2026-10-26T17:00"));
}

#[test]
fn r041_working_lag_ignores_elapsed_duration_of_successor() {
    // B a une durée écoulée de 8 h ; son décalage ouvré FS+1 j reste sur le
    // calendrier du projet : add_std(lun 17:00, 480) = mar 17:00 (et non
    // mar 01:00 en temps civil) → B mar 17:00 – mer 01:00 (temps civil).
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .elapsed("B", 480)
        .link("A", "B", LinkType::FinishToStart, lag_w(480))
        .run();
    assert_eq!(run.at("B").early.start, moment("2026-10-20T17:00"));
    assert_eq!(run.at("B").early.finish, moment("2026-10-21T01:00"));
}
