//! R-078 — Jours de la semaine (import MSPDI) : `DayType` 1 = dimanche …
//! 7 = samedi, `DayWorking`, plages par défaut, jours manquants.

use plannercraft_mspdi::{ImportCode, MspdiDocument, ReadOptions, read};
use plannercraft_tests::{day_minutes, moment, tests_path};

fn fixture() -> MspdiDocument {
    let bytes = std::fs::read(tests_path("fixtures/mspdi/calendars.xml")).unwrap();
    read(&bytes, &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap()
}

#[test]
fn r078_standard_week() {
    // Calendrier 1 : lun-ven 08:00-12:00 et 13:00-17:00 → 2 400 par semaine
    // (lun 19/10 au dim 25/10/2026).
    let doc = fixture();
    let cals = &doc.project.calendars;
    let week: i64 = ["19", "20", "21", "22", "23", "24", "25"]
        .iter()
        .map(|d| day_minutes(cals, 1, &format!("2026-10-{d}")))
        .sum();
    assert_eq!(week, 2_400);
    // DayType 1 = dimanche (25/10) et 7 = samedi (24/10) : chômés.
    assert_eq!(day_minutes(cals, 1, "2026-10-25"), 0);
    assert_eq!(day_minutes(cals, 1, "2026-10-24"), 0);
}

#[test]
fn r078_six_working_days() {
    // Calendrier 8 : samedi ouvré 08:00-12:00 en plus → 2 640.
    let doc = fixture();
    let cals = &doc.project.calendars;
    let week: i64 = (19..=25)
        .map(|d| day_minutes(cals, 8, &format!("2026-10-{d}")))
        .sum();
    assert_eq!(week, 2_640);
}

#[test]
fn r078_working_day_without_ranges_gets_default_hours() {
    // Calendrier 3, mercredi : `DayWorking 1` sans plage → 08:00-12:00 et
    // 13:00-17:00 (480), signalé.
    let doc = fixture();
    assert_eq!(day_minutes(&doc.project.calendars, 3, "2026-10-21"), 480);
    assert!(doc.diagnostics.iter().any(|d| d.code == ImportCode::InvalidValue
        && d.element == "Calendars/Calendar/WeekDays/WeekDay/WorkingTimes"));
}

#[test]
fn r078_missing_day_of_base_calendar_is_non_working() {
    // Calendrier 3 : dimanche absent → chômé, signalé (valeur : DayType 1).
    let doc = fixture();
    assert_eq!(day_minutes(&doc.project.calendars, 3, "2026-10-25"), 0);
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.code == ImportCode::MissingRequired
                && d.element == "Calendars/Calendar/WeekDays/WeekDay"
                && d.value.as_deref() == Some("1"))
    );
}

#[test]
fn r078_missing_day_of_derived_calendar_is_inherited() {
    // Calendrier 4, dérivé du 1, ne définit que le samedi (08:00-12:00) :
    // lundi hérité (480), samedi 240.
    let doc = fixture();
    let cals = &doc.project.calendars;
    assert_eq!(day_minutes(cals, 4, "2026-10-19"), 480);
    assert_eq!(day_minutes(cals, 4, "2026-10-24"), 240);
}

#[test]
fn r078_unknown_or_duplicate_day_type() {
    let xml = "<Project><StartDate>2026-10-19T08:00:00</StartDate><CalendarUID>1</CalendarUID><Calendars><Calendar>\
               <UID>1</UID><IsBaseCalendar>1</IsBaseCalendar><WeekDays>\
               <WeekDay><DayType>9</DayType><DayWorking>1</DayWorking></WeekDay>\
               <WeekDay><DayType>2</DayType><DayWorking>0</DayWorking></WeekDay>\
               <WeekDay><DayType>2</DayType><DayWorking>1</DayWorking></WeekDay>\
               </WeekDays></Calendar></Calendars></Project>";
    let doc = read(xml.as_bytes(), &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap();
    // Code 9 inconnu : ignoré ; lundi en double : le premier (chômé) gagne.
    assert_eq!(day_minutes(&doc.project.calendars, 1, "2026-10-19"), 0);
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.code == ImportCode::UnknownCode { code: 9 })
    );
    assert!(doc.diagnostics.iter().any(|d| d.code == ImportCode::Duplicate));
}
