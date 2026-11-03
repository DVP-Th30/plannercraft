//! R-081 — Ancienne forme des exceptions (`WeekDay` de `DayType` 0, import
//! MSPDI).

use plannercraft_mspdi::{MspdiDocument, ReadOptions, read};
use plannercraft_tests::{day_minutes, moment, tests_path};

fn fixture() -> MspdiDocument {
    let bytes = std::fs::read(tests_path("fixtures/mspdi/calendars.xml")).unwrap();
    read(&bytes, &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap()
}

#[test]
fn r081_legacy_form_used_without_exceptions_element() {
    // Calendrier 6, sans `Exceptions` : `DayType 0` du dim 01/02/2004, ouvré
    // 08:00-12:00 et 13:00-17:00 → exception ouvrée de 480 min.
    let doc = fixture();
    let cals = &doc.project.calendars;
    assert_eq!(day_minutes(cals, 6, "2004-02-01"), 480);
    assert_eq!(day_minutes(cals, 6, "2004-02-08"), 0);
}

#[test]
fn r081_legacy_form_ignored_when_exceptions_present() {
    // Calendrier 7 : ancienne forme (mer 21/10/2026 chômé) ignorée, la forme
    // `Exceptions` (jeu 22/10/2026 chômé) fait foi.
    let doc = fixture();
    let cals = &doc.project.calendars;
    assert_eq!(day_minutes(cals, 7, "2026-10-21"), 480);
    assert_eq!(day_minutes(cals, 7, "2026-10-22"), 0);
}
