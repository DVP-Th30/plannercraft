//! R-128 — Calendriers à l'écriture.

use plannercraft_core::calendar::{
    Calendar, CalendarException, DayHours, DaySetting, Recurrence, TimeRange, WorkWeek,
};
use plannercraft_core::{CalendarSet, CalendarUid, DayRange, Month, TimeOfDay, Week, Weekday};
use plannercraft_mspdi::write::{Computed, OrderProfile, WriteOptions, write};
use plannercraft_tests::mspdi::{compact, new_project, reread, text, written};
use plannercraft_tests::{day, time};

fn standard(proj: &plannercraft_core::Project) -> &Calendar {
    proj.calendars.get(CalendarUid::new(1)).unwrap()
}

#[test]
fn r128_period_exception_is_written_daily_and_reads_the_same_days() {
    let mut proj = new_project("2026-10-19T08:00");
    let cal = proj.calendars.get_mut(CalendarUid::new(1)).unwrap();
    cal.add_exception(CalendarException {
        name: "Noël".to_owned(),
        period: DayRange::single(day("2026-12-25")),
        setting: DaySetting::NonWorking,
        recurrence: Recurrence::Period,
        occurrences: None,
    })
    .unwrap();
    cal.add_exception(CalendarException {
        name: "Fête".to_owned(),
        period: DayRange::new(day("2025-04-04"), day("2027-04-04")).unwrap(),
        setting: DaySetting::NonWorking,
        recurrence: Recurrence::YearlyByDay {
            month: Month::April,
            day: 4,
        },
        occurrences: None,
    })
    .unwrap();
    let out = written(&proj);
    let xml = compact(&text(&out));
    assert!(xml.contains(
        "<Exception><EnteredByOccurrences>0</EnteredByOccurrences><TimePeriod><FromDate>2026-12-25T00:00:00</FromDate>\
         <ToDate>2026-12-25T23:59:00</ToDate></TimePeriod><Name>Noël</Name><Type>1</Type><Period>1</Period>\
         <DayWorking>0</DayWorking></Exception>"
    ));
    // Férié annuel (`Type 2`, mois 3 = avril, jour 4) réécrit à l'identique.
    assert!(xml.contains(
        "<Name>Fête</Name><Type>2</Type><Month>3</Month><MonthDay>4</MonthDay><DayWorking>0</DayWorking>"
    ));
    let back = reread(&out.xml).project;
    let e = &standard(&back).exceptions()[0];
    assert_eq!(e.recurrence, Recurrence::Daily { every: 1 });
    assert_eq!(standard(&back).exceptions()[1], standard(&proj).exceptions()[1]);
    // Mêmes plages pour chaque jour de 2026.
    let (before, _) = CalendarSet::compile(&proj);
    let (after, _) = CalendarSet::compile(&back);
    let (b, a) = (
        before.get(CalendarUid::new(1)).unwrap(),
        after.get(CalendarUid::new(1)).unwrap(),
    );
    let mut d = day("2026-01-01");
    while d <= day("2026-12-31") {
        assert_eq!(b.day_ranges(d).unwrap(), a.day_ranges(d).unwrap(), "{d}");
        d = d.checked_add_days(1).unwrap();
    }
}

#[test]
fn r128_base_derived_ranges_and_work_weeks() {
    let mut proj = new_project("2026-10-19T08:00");
    // Dérivé du Standard : samedi 18:00-24:00 seulement.
    let mut night = Calendar::new_derived(CalendarUid::new(10), "Soir".to_owned(), CalendarUid::new(1));
    night
        .set_day(
            Weekday::Saturday,
            DaySetting::Working(
                DayHours::new(vec![
                    TimeRange::new(time("18:00"), TimeOfDay::END_OF_DAY).unwrap(),
                ])
                .unwrap(),
            )
            .into(),
        )
        .unwrap();
    night
        .add_work_week(WorkWeek {
            name: "Hiver".to_owned(),
            period: DayRange::new(day("2026-12-21"), day("2026-12-31")).unwrap(),
            days: Week::from_fn(|d| {
                if d == Weekday::Friday {
                    plannercraft_core::calendar::DayRule::NonWorking
                } else {
                    plannercraft_core::calendar::DayRule::Inherit
                }
            }),
        })
        .unwrap();
    proj.calendars.push(night).unwrap();
    let out = written(&proj);
    let xml = compact(&text(&out));
    // Base : `IsBaseCalendar 1`, `BaseCalendarUID -1`, sept jours.
    assert!(xml.contains("<UID>1</UID><Name>Standard</Name><IsBaseCalendar>1</IsBaseCalendar><BaseCalendarUID>-1</BaseCalendarUID>"));
    // Dérivé : seul le samedi ; fin de plage 24:00 écrite 00:00:00.
    assert!(xml.contains(
        "<IsBaseCalendar>0</IsBaseCalendar><BaseCalendarUID>1</BaseCalendarUID><WeekDays><WeekDay><DayType>7</DayType>\
         <DayWorking>1</DayWorking><WorkingTimes><WorkingTime><FromTime>18:00:00</FromTime><ToTime>00:00:00</ToTime>"
    ));
    assert!(xml.contains("<WorkWeek><TimePeriod><FromDate>2026-12-21T00:00:00</FromDate><ToDate>2026-12-31T23:59:00</ToDate></TimePeriod><Name>Hiver</Name><WeekDays><WeekDay><DayType>6</DayType><DayWorking>0</DayWorking></WeekDay></WeekDays></WorkWeek>"));
    // L'ancienne forme (`WeekDay` de `DayType 0`) n'est jamais écrite.
    assert!(!xml.contains("<DayType>0</DayType>"));
    let back = reread(&out.xml).project;
    assert_eq!(back.calendars, proj.calendars);
}

#[test]
fn r128_strict_work_week_form() {
    // Profil strict : forme pj15 (`WeekDay` direct, sans plages), relue
    // (amendement R-082).
    let mut proj = new_project("2026-10-19T08:00");
    let ww = WorkWeek {
        name: "Été".to_owned(),
        period: DayRange::new(day("2026-07-01"), day("2026-07-31")).unwrap(),
        days: Week::from_fn(|d| {
            if d == Weekday::Friday {
                plannercraft_core::calendar::DayRule::NonWorking
            } else {
                plannercraft_core::calendar::DayRule::Inherit
            }
        }),
    };
    proj.calendars
        .get_mut(CalendarUid::new(1))
        .unwrap()
        .add_work_week(ww.clone())
        .unwrap();
    let out = write(
        &proj,
        Computed::Omit,
        &WriteOptions {
            profile: OrderProfile::Strict,
            ..WriteOptions::default()
        },
    );
    let xml = compact(&String::from_utf8(out.xml.clone()).unwrap());
    assert!(xml.contains(
        "<Name>Été</Name><WeekDay><DayType>6</DayType><DayWorking>0</DayWorking></WeekDay></WorkWeek>"
    ));
    let back = reread(&out.xml).project;
    assert_eq!(standard(&back).work_weeks(), [ww]);
}
