//! R-077 — Plages horaires d'un jour (import MSPDI) : `ToTime` 00:00 lu
//! 24:00, plages triées, vides ou inversées ignorées.

use std::fmt::Write as _;

use plannercraft_mspdi::{ImportCode, MspdiDocument, ReadOptions, read};
use plannercraft_tests::{day_minutes, moment, tests_path};

fn fixture() -> MspdiDocument {
    let bytes = std::fs::read(tests_path("fixtures/mspdi/calendars.xml")).unwrap();
    read(&bytes, &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap()
}

fn count(doc: &MspdiDocument, code: &ImportCode, element: &str) -> usize {
    doc.diagnostics
        .iter()
        .filter(|d| &d.code == code && d.element == element)
        .count()
}

#[test]
fn r077_midnight_end_is_24_00() {
    // Calendrier 2 « 24 Hours » : 00:00 → 00:00 = toute la journée, 1 440.
    let doc = fixture();
    let cals = &doc.project.calendars;
    assert_eq!(day_minutes(cals, 2, "2026-10-19"), 1_440);
    assert_eq!(day_minutes(cals, 2, "2026-10-25"), 1_440);
}

#[test]
fn r077_ranges_sorted_and_contiguous_merged() {
    // Calendrier 3, lundi : 18:00-22:00 puis 10:00-18:00 → triées,
    // contiguës (fusionnées par R-011) : 720.
    let doc = fixture();
    assert_eq!(day_minutes(&doc.project.calendars, 3, "2026-10-19"), 720);
}

#[test]
fn r077_inverted_range_ignored() {
    // Calendrier 3, mardi : 08:00-12:00 gardée, 12:00 → 08:00 ignorée : 240.
    let doc = fixture();
    assert_eq!(day_minutes(&doc.project.calendars, 3, "2026-10-20"), 240);
    assert_eq!(
        count(
            &doc,
            &ImportCode::InvalidValue,
            "Calendars/Calendar/WeekDays/WeekDay/WorkingTimes/WorkingTime"
        ),
        1
    );
}

#[test]
fn r077_overlapping_ranges_merged_with_diagnostic() {
    // Calendrier 3, jeudi : 08:00-12:00 et 10:00-14:00 se chevauchent →
    // 08:00-14:00 = 360, signalé.
    let doc = fixture();
    assert_eq!(day_minutes(&doc.project.calendars, 3, "2026-10-22"), 360);
    assert_eq!(
        count(
            &doc,
            &ImportCode::Inconsistent,
            "Calendars/Calendar/WeekDays/WeekDay/WorkingTimes/WorkingTime"
        ),
        1
    );
}

#[test]
fn r077_more_than_five_ranges() {
    // Six plages disjointes d'une heure : les cinq premières gardées, signalé.
    let mut ranges = String::new();
    for h in 8..14 {
        let _ = write!(
            ranges,
            "<WorkingTime><FromTime>{h:02}:00:00</FromTime><ToTime>{h:02}:30:00</ToTime></WorkingTime>"
        );
    }
    let xml = format!(
        "<Project><StartDate>2026-10-19T08:00:00</StartDate><CalendarUID>1</CalendarUID><Calendars><Calendar>\
         <UID>1</UID><IsBaseCalendar>1</IsBaseCalendar><WeekDays>\
         <WeekDay><DayType>2</DayType><DayWorking>1</DayWorking><WorkingTimes>{ranges}</WorkingTimes></WeekDay>\
         </WeekDays></Calendar></Calendars></Project>"
    );
    let doc = read(xml.as_bytes(), &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap();
    assert_eq!(day_minutes(&doc.project.calendars, 1, "2026-10-19"), 150);
    assert_eq!(
        count(
            &doc,
            &ImportCode::InvalidValue,
            "Calendars/Calendar/WeekDays/WeekDay/WorkingTimes/WorkingTime"
        ),
        1
    );
}
