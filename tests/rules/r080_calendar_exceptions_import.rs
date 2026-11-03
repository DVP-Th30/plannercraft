//! R-080 — Exceptions de calendrier, forme `Exceptions/Exception` (import
//! MSPDI).

use plannercraft_mspdi::{ImportCode, MspdiDocument, ReadOptions, read};
use plannercraft_tests::{day_minutes, moment, tests_path};

fn fixture() -> MspdiDocument {
    let bytes = std::fs::read(tests_path("fixtures/mspdi/calendars.xml")).unwrap();
    read(&bytes, &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap()
}

#[test]
fn r080_single_day_until_23_59() {
    // « Noël », `Type 1`, du 25/12/2026 00:00 au 25/12 23:59 : un seul jour.
    let doc = fixture();
    let cals = &doc.project.calendars;
    assert_eq!(day_minutes(cals, 1, "2026-12-25"), 0);
    assert_eq!(day_minutes(cals, 1, "2026-12-24"), 480);
    assert_eq!(day_minutes(cals, 1, "2026-12-28"), 480);
}

#[test]
fn r080_yearly_by_day_restricted_to_period() {
    // `Type 2`, `Month 3` (avril, 0 = janvier), `MonthDay 4`, du 04/04/2025
    // au 06/04/2025 : seul le ven 04/04/2025 est chômé ; le 04/04/2026 (un
    // samedi) est hors période.
    let doc = fixture();
    let cals = &doc.project.calendars;
    assert_eq!(day_minutes(cals, 1, "2025-04-04"), 0);
    assert_eq!(day_minutes(cals, 1, "2025-04-07"), 480);
    assert_eq!(day_minutes(cals, 1, "2025-04-03"), 480);
}

#[test]
fn r080_to_date_at_midnight_ends_the_day_before() {
    // « Pont », `Type 9`, du 09/11/2026 00:00 au 11/11/2026 00:00 : lundi 9
    // et mardi 10 chômés, mercredi 11 ouvré.
    let doc = fixture();
    let cals = &doc.project.calendars;
    assert_eq!(day_minutes(cals, 1, "2026-11-09"), 0);
    assert_eq!(day_minutes(cals, 1, "2026-11-10"), 0);
    assert_eq!(day_minutes(cals, 1, "2026-11-11"), 480);
}

#[test]
fn r080_weekly_with_working_times() {
    // `Type 6`, `DaysOfWeek` 64 (samedi), juin 2026, ouvré 08:00-12:00 :
    // samedis 06, 13, 20, 27/06 = 240 ; samedi 04/07 hors période = 0.
    let doc = fixture();
    let cals = &doc.project.calendars;
    for d in ["2026-06-06", "2026-06-13", "2026-06-20", "2026-06-27"] {
        assert_eq!(day_minutes(cals, 1, d), 240, "{d}");
    }
    assert_eq!(day_minutes(cals, 1, "2026-07-04"), 0);
    assert_eq!(day_minutes(cals, 1, "2026-06-08"), 480);
}

#[test]
fn r080_occurrences_only_when_entered_by_occurrences() {
    // `Type 1`, `Period 2`, `EnteredByOccurrences 1`, `Occurrences 3`, à
    // partir du lun 02/03/2026 : 02, 04 et 06/03 chômés ; le 10/03 (5e jour
    // de la série) reste ouvré ; le 03/03 aussi (hors pas).
    let doc = fixture();
    let cals = &doc.project.calendars;
    for d in ["2026-03-02", "2026-03-04", "2026-03-06"] {
        assert_eq!(day_minutes(cals, 1, d), 0, "{d}");
    }
    assert_eq!(day_minutes(cals, 1, "2026-03-03"), 480);
    assert_eq!(day_minutes(cals, 1, "2026-03-10"), 480);
}

#[test]
fn r080_unknown_type_ignored() {
    // `Type 12` : exception ignorée, signalée ; le lun 04/05/2026 reste ouvré.
    let doc = fixture();
    assert_eq!(day_minutes(&doc.project.calendars, 1, "2026-05-04"), 480);
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.code == ImportCode::UnknownCode { code: 12 }
                && d.element == "Calendars/Calendar/Exceptions/Exception/Type")
    );
}
