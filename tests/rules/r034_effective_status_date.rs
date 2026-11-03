//! R-034 — Date d'état effective : sans date d'état (NA), la date courante
//! injectée.

use plannercraft_core::{BuiltinCalendarNames, Project, ProjectInit, ScheduleAnchor};
use plannercraft_tests::moment;

fn project() -> Project {
    Project::new(ProjectInit {
        name: "P".to_owned(),
        anchor: ScheduleAnchor::FromStart(moment("2026-10-19T08:00")),
        current_date: moment("2026-10-17T08:00"),
        calendar_names: BuiltinCalendarNames::default(),
    })
}

#[test]
fn r034_na_status_date_uses_current_date() {
    let p = project();
    assert_eq!(p.status_date, None);
    assert_eq!(p.effective_status_date(), moment("2026-10-17T08:00"));
}

#[test]
fn r034_explicit_status_date_wins() {
    let mut p = project();
    p.status_date = Some(moment("2026-10-15T17:00"));
    assert_eq!(p.effective_status_date(), moment("2026-10-15T17:00"));
}
