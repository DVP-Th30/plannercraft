//! R-025 — Instant ouvré précédent : `previous_working_moment(t)` = plus
//! grand u <= t tel que la minute [u − 1, u) est ouvrée ; s'il n'en existe
//! aucun : `OutOfDomain { Before }`. Calendrier Standard.

use plannercraft_core::{CalendarError, CalendarUid, Direction};
use plannercraft_tests::{compile_one, moment, standard};

fn previous(t: &str) -> Result<plannercraft_core::Moment, CalendarError> {
    compile_one(standard()).previous_working_moment(moment(t))
}

#[test]
fn r025_aligns_on_previous_range_end() {
    // Lun 26/10 08:00 → ven 23/10 17:00.
    assert_eq!(previous("2026-10-26T08:00"), Ok(moment("2026-10-23T17:00")));
    // Lun 13:00 → lun 12:00.
    assert_eq!(previous("2026-10-19T13:00"), Ok(moment("2026-10-19T12:00")));
    // Dim 25/10 10:00 → ven 17:00.
    assert_eq!(previous("2026-10-25T10:00"), Ok(moment("2026-10-23T17:00")));
    // Lun 09:30, dans une plage : inchangé.
    assert_eq!(previous("2026-10-19T09:30"), Ok(moment("2026-10-19T09:30")));
}

#[test]
fn r025_nothing_before_first_working_minute() {
    // Lun 1984-01-02 08:00 : première minute ouvrée du domaine (le 1er est un
    // dimanche) ; rien avant.
    assert_eq!(
        previous("1984-01-02T08:00"),
        Err(CalendarError::OutOfDomain {
            calendar: CalendarUid::new(1),
            direction: Direction::Before
        })
    );
    // Une minute plus tard : 08:01.
    assert_eq!(previous("1984-01-02T08:01"), Ok(moment("1984-01-02T08:01")));
}
