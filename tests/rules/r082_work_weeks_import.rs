//! R-082 — Semaines de travail datées (`WorkWeeks`, import MSPDI).

use plannercraft_mspdi::{MspdiDocument, ReadOptions, read};
use plannercraft_tests::{day_minutes, moment, tests_path};

fn fixture() -> MspdiDocument {
    let bytes = std::fs::read(tests_path("fixtures/mspdi/calendars.xml")).unwrap();
    read(&bytes, &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap()
}

#[test]
fn r082_work_week_with_inherited_day() {
    // Calendrier 8 : « Winter Break » du mar 22/12 au sam 26/12/2026 23:59,
    // dimanche à vendredi chômés, samedi absent → hérité de la semaine par
    // défaut du calendrier (samedi 08:00-12:00).
    let doc = fixture();
    let cals = &doc.project.calendars;
    for d in ["2026-12-22", "2026-12-23", "2026-12-24", "2026-12-25"] {
        assert_eq!(day_minutes(cals, 8, d), 0, "{d}");
    }
    assert_eq!(day_minutes(cals, 8, "2026-12-26"), 240);
    // Avant et après la période : semaine par défaut.
    assert_eq!(day_minutes(cals, 8, "2026-12-21"), 480);
    assert_eq!(day_minutes(cals, 8, "2026-12-28"), 480);
    let calendar = cals.get(plannercraft_core::CalendarUid::new(8)).unwrap();
    assert_eq!(calendar.work_weeks().len(), 1);
    assert_eq!(calendar.work_weeks()[0].name, "Winter Break");
}

#[test]
fn r082_pj15_work_week_form() {
    // Amendement J3 (R-128) : forme du schéma pj15, `WeekDay` direct sous
    // `WorkWeek` (`DayType`, `DayWorking`, sans plages) ; un jour ouvré sans
    // plage reçoit 08:00-12:00 et 13:00-17:00 (R-078).
    let xml = "<Project xmlns=\"http://schemas.microsoft.com/project\"><StartDate>2026-10-19T08:00:00</StartDate>\
               <CalendarUID>1</CalendarUID><Calendars><Calendar><UID>1</UID><IsBaseCalendar>1</IsBaseCalendar>\
               <WeekDays><WeekDay><DayType>1</DayType><DayWorking>0</DayWorking></WeekDay></WeekDays>\
               <WorkWeeks><WorkWeek><TimePeriod><FromDate>2026-12-21T00:00:00</FromDate><ToDate>2026-12-26T23:59:00</ToDate>\
               </TimePeriod><Name>Hiver</Name><WeekDay><DayType>6</DayType><DayWorking>0</DayWorking></WeekDay>\
               <WeekDay><DayType>1</DayType><DayWorking>1</DayWorking></WeekDay></WorkWeek></WorkWeeks></Calendar>\
               </Calendars></Project>";
    let doc = plannercraft_mspdi::read(
        xml.as_bytes(),
        &plannercraft_mspdi::ReadOptions::new(plannercraft_tests::moment("2026-01-01T00:00")),
    )
    .unwrap();
    let cal = doc.project.calendars.at(0).unwrap();
    let ww = &cal.work_weeks()[0];
    assert_eq!(ww.name, "Hiver");
    assert_eq!(
        *ww.days.get(plannercraft_core::Weekday::Friday),
        plannercraft_core::calendar::DayRule::NonWorking
    );
    match ww.days.get(plannercraft_core::Weekday::Sunday) {
        plannercraft_core::calendar::DayRule::Working(h) => {
            assert_eq!(h.total(), plannercraft_core::Minutes::new(480));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        *ww.days.get(plannercraft_core::Weekday::Monday),
        plannercraft_core::calendar::DayRule::Inherit
    );
}
