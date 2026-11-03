//! R-079 — Calendrier de base ou dérivé (import MSPDI), dont les
//! `BaseCalendarUID` négatifs.

use plannercraft_core::calendar::CalendarKind;
use plannercraft_core::{CalendarUid, Diagnostic};
use plannercraft_mspdi::{ImportCode, MspdiDocument, ReadOptions, read};
use plannercraft_tests::{day_minutes, moment, tests_path};

fn fixture() -> MspdiDocument {
    let bytes = std::fs::read(tests_path("fixtures/mspdi/calendars.xml")).unwrap();
    read(&bytes, &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap()
}

fn kind(doc: &MspdiDocument, uid: u32) -> CalendarKind {
    doc.project
        .calendars
        .get(CalendarUid::new(uid))
        .unwrap()
        .kind()
        .clone()
}

fn calendars(xml: &str) -> MspdiDocument {
    let xml = format!(
        "<Project><StartDate>2026-10-19T08:00:00</StartDate><CalendarUID>1</CalendarUID><Calendars>{xml}</Calendars></Project>"
    );
    read(xml.as_bytes(), &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap()
}

#[test]
fn r079_base_and_derived() {
    let doc = fixture();
    assert!(matches!(kind(&doc, 1), CalendarKind::Base { .. }));
    assert!(matches!(kind(&doc, 4), CalendarKind::Derived { base, .. } if base == CalendarUid::new(1)));
}

#[test]
fn r079_negative_base_fixture_inherits() {
    // Calendrier 5 : `IsBaseCalendar 0`, `BaseCalendarUID −65533` → dérivé
    // du calendrier de base d'UID minimal (1, ici aussi celui du projet) ;
    // il hérite de Noël.
    let doc = fixture();
    assert!(matches!(kind(&doc, 5), CalendarKind::Derived { base, .. } if base == CalendarUid::new(1)));
    let cals = &doc.project.calendars;
    assert_eq!(day_minutes(cals, 5, "2026-10-19"), 480);
    assert_eq!(day_minutes(cals, 5, "2026-12-25"), 0);
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.code == ImportCode::DanglingReference
                && d.element == "Calendars/Calendar/BaseCalendarUID"
                && d.value.as_deref() == Some("-65533"))
    );
}

#[test]
fn r079_negative_base_uses_min_uid_base() {
    // Amendement J5 (O29, A6 calendrier 11 « IDEA ») : bases 1 (lun-ven) et
    // 9 (lun-sam, calendrier du projet) ; le dérivé 11 à base −65533 hérite
    // du calendrier de base d'UID minimal (1), pas de celui du projet. Une
    // base absente garde le calendrier du projet.
    let monday_to_saturday = (2..=7)
        .map(|d| format!("<WeekDay><DayType>{d}</DayType><DayWorking>1</DayWorking></WeekDay>"))
        .collect::<Vec<_>>()
        .concat();
    let monday_to_friday = (2..=6)
        .map(|d| format!("<WeekDay><DayType>{d}</DayType><DayWorking>1</DayWorking></WeekDay>"))
        .collect::<Vec<_>>()
        .concat();
    let xml = format!(
        "<Project><StartDate>2026-10-19T08:00:00</StartDate><CalendarUID>9</CalendarUID><Calendars>\
         <Calendar><UID>1</UID><IsBaseCalendar>1</IsBaseCalendar><BaseCalendarUID>-1</BaseCalendarUID><WeekDays>{monday_to_friday}</WeekDays></Calendar>\
         <Calendar><UID>9</UID><IsBaseCalendar>1</IsBaseCalendar><BaseCalendarUID>-1</BaseCalendarUID><WeekDays>{monday_to_saturday}</WeekDays></Calendar>\
         <Calendar><UID>11</UID><IsBaseCalendar>0</IsBaseCalendar><BaseCalendarUID>-65533</BaseCalendarUID></Calendar>\
         <Calendar><UID>12</UID><IsBaseCalendar>0</IsBaseCalendar></Calendar>\
         </Calendars></Project>"
    );
    let doc = read(xml.as_bytes(), &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap();
    assert!(matches!(kind(&doc, 11), CalendarKind::Derived { base, .. } if base == CalendarUid::new(1)));
    // Samedi 24/10/2026 : chômé sur le dérivé (base 1), ouvré sur le 9.
    assert_eq!(day_minutes(&doc.project.calendars, 11, "2026-10-24"), 0);
    assert_eq!(day_minutes(&doc.project.calendars, 9, "2026-10-24"), 480);
    assert!(matches!(kind(&doc, 12), CalendarKind::Derived { base, .. } if base == CalendarUid::new(9)));
}

#[test]
fn r079_negative_base_of_project_calendar_itself_is_base() {
    // Le calendrier du projet lui-même sans base valide : calendrier de base,
    // jours manquants chômés, signalé.
    let doc = calendars(
        "<Calendar><UID>1</UID><IsBaseCalendar>0</IsBaseCalendar><BaseCalendarUID>-1</BaseCalendarUID>\
         <WeekDays><WeekDay><DayType>2</DayType><DayWorking>1</DayWorking></WeekDay></WeekDays></Calendar>",
    );
    assert!(matches!(kind(&doc, 1), CalendarKind::Base { .. }));
    assert_eq!(day_minutes(&doc.project.calendars, 1, "2026-10-19"), 480);
    assert_eq!(day_minutes(&doc.project.calendars, 1, "2026-10-20"), 0);
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.code == ImportCode::DanglingReference)
    );
}

#[test]
fn r079_missing_base_is_reported() {
    // Base 99 inexistante : dérivé gardé, référence signalée par l'import et
    // par `Project::validate`.
    let doc = calendars(
        "<Calendar><UID>1</UID><IsBaseCalendar>1</IsBaseCalendar></Calendar>\
         <Calendar><UID>2</UID><IsBaseCalendar>0</IsBaseCalendar><BaseCalendarUID>99</BaseCalendarUID></Calendar>",
    );
    assert!(matches!(kind(&doc, 2), CalendarKind::Derived { base, .. } if base == CalendarUid::new(99)));
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.code == ImportCode::DanglingReference)
    );
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| matches!(d.code, ImportCode::Core(Diagnostic::CalendarBaseMissing { .. })))
    );
}

#[test]
fn r079_duplicate_uid_keeps_first() {
    let doc = calendars(
        "<Calendar><UID>1</UID><Name>A</Name><IsBaseCalendar>1</IsBaseCalendar></Calendar>\
         <Calendar><UID>1</UID><Name>B</Name><IsBaseCalendar>1</IsBaseCalendar></Calendar>",
    );
    assert_eq!(doc.project.calendars.len(), 1);
    assert_eq!(doc.project.calendars.get(CalendarUid::new(1)).unwrap().name, "A");
    assert!(doc.diagnostics.iter().any(|d| d.code == ImportCode::Duplicate));
}
