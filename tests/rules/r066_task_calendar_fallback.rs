//! R-066 — Calendrier d'une tâche et replis : calendrier propre, sinon du
//! projet ; propre inutilisable → projet + `task.calendar_fallback` ; projet
//! inutilisable → Standard intégré + `project.calendar_fallback`. Source :
//! S8 ; replis : hypothèse.

use plannercraft_core::calendar::{Calendar, DaySetting};
use plannercraft_core::{CalendarUid, Diagnostic, Week};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

#[test]
fn r066_unknown_task_calendar() {
    // A, calendrier 99 inexistant, 1 j → calendrier du projet : lun 08:00 –
    // 17:00, avertissement.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .calendar("A", 99)
        .run();
    assert_eq!(run.at("A").early.finish, moment("2026-10-19T17:00"));
    assert!(
        run.schedule
            .diagnostics
            .contains(&Diagnostic::TaskCalendarFallback {
                task: run.uid("A"),
                calendar: CalendarUid::new(99),
            })
    );
}

#[test]
fn r066_project_calendar_without_working_time() {
    // Calendrier du projet (UID 9) sans temps ouvré → Standard intégré,
    // erreur ; A lun 08:00 – 17:00.
    let run = Net::new("2026-10-19T08:00")
        .project(|p| {
            let empty = Calendar::new_base(
                CalendarUid::new(9),
                "Vide".to_owned(),
                Week::from_fn(|_| DaySetting::NonWorking),
            );
            p.calendars.push(empty).unwrap();
            p.calendar = CalendarUid::new(9);
        })
        .task("A", 480)
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("A").early.finish, moment("2026-10-19T17:00"));
    assert!(
        run.schedule
            .diagnostics
            .contains(&Diagnostic::ProjectCalendarFallback {
                calendar: CalendarUid::new(9),
            })
    );
}

#[test]
fn r066_task_calendar_used() {
    // Calendrier propre utilisable (24 heures) : A 1 j = lun 08:00 – 16:00.
    let run = Net::new("2026-10-19T08:00").task("A", 480).calendar("A", 2).run();
    assert_eq!(run.at("A").early.finish, moment("2026-10-19T16:00"));
    assert!(!run.codes().contains(&"task.calendar_fallback"));
}
