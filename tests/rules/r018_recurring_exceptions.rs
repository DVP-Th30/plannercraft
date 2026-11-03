//! R-018 — Exceptions récurrentes : dépliées en jours explicites, restreintes
//! à la période et limitées à `occurrences` (ordre chronologique) ; jamais
//! lues comme une période continue.

use plannercraft_core::calendar::{
    Calendar, CalendarException, DaySetting, MonthItem, MonthPosition, Recurrence, expand_exception,
};
use plannercraft_core::{
    BuiltinCalendarNames, CalendarUid, Diagnostic, Month, Project, ProjectInit, ScheduleAnchor, Week, Weekday,
};
use plannercraft_tests::{calendars, day_minutes, days, moment, standard};

fn recurring(from: &str, to: &str, recurrence: Recurrence, occurrences: Option<u32>) -> CalendarException {
    CalendarException {
        name: String::new(),
        period: days(from, to),
        setting: DaySetting::NonWorking,
        recurrence,
        occurrences,
    }
}

/// Jours dépliés, un par un, en texte.
fn expanded(e: &CalendarException) -> Vec<String> {
    let mut out = Vec::new();
    for r in expand_exception(e).unwrap() {
        let mut d = r.first();
        loop {
            out.push(d.to_string());
            if d == r.last() {
                break;
            }
            d = d.checked_add_days(1).unwrap();
        }
    }
    out
}

#[test]
fn r018_daily_every_two_days() {
    // Lun 02/11/2026 au ven 06/11, un jour sur deux : lun 2, mer 4, ven 6.
    let e = recurring("2026-11-02", "2026-11-06", Recurrence::Daily { every: 2 }, None);
    assert_eq!(expanded(&e), ["2026-11-02", "2026-11-04", "2026-11-06"]);
    let mut c = standard();
    c.add_exception(e).unwrap();
    let a = calendars(vec![c]);
    // Mardi 3 et jeudi 5 restent ouvrés : la récurrence n'est pas une période
    // continue.
    assert_eq!(day_minutes(&a, 1, "2026-11-02"), 0);
    assert_eq!(day_minutes(&a, 1, "2026-11-03"), 480);
    assert_eq!(day_minutes(&a, 1, "2026-11-05"), 480);
    assert_eq!(day_minutes(&a, 1, "2026-11-06"), 0);
}

#[test]
fn r018_occurrences_limit() {
    // Tous les jours à partir du jeu 26/11/2026 (période jusqu'au 31/12),
    // 2 occurrences : 26 et 27/11 seulement.
    let e = recurring(
        "2026-11-26",
        "2026-12-31",
        Recurrence::Daily { every: 1 },
        Some(2),
    );
    assert_eq!(expanded(&e), ["2026-11-26", "2026-11-27"]);
}

#[test]
fn r018_yearly_by_day() {
    // Noël du 25/12/2026 au 25/12/2028 : 3 jours.
    let e = recurring(
        "2026-12-25",
        "2028-12-25",
        Recurrence::YearlyByDay {
            month: Month::December,
            day: 25,
        },
        None,
    );
    assert_eq!(expanded(&e), ["2026-12-25", "2027-12-25", "2028-12-25"]);
    let mut c = standard();
    c.add_exception(e).unwrap();
    let a = calendars(vec![c]);
    // Mar 15/06/2027, dans la période mais hors récurrence : 480.
    assert_eq!(day_minutes(&a, 1, "2027-06-15"), 480);
    // Lun 25/12/2028 : chômé.
    assert_eq!(day_minutes(&a, 1, "2028-12-25"), 0);
}

#[test]
fn r018_weekly() {
    // Chaque vendredi de janvier 2027 (le 1er est un vendredi).
    let e = recurring(
        "2027-01-01",
        "2027-01-31",
        Recurrence::Weekly {
            every: 1,
            days: Week::from_fn(|d| d == Weekday::Friday),
        },
        None,
    );
    assert_eq!(
        expanded(&e),
        [
            "2027-01-01",
            "2027-01-08",
            "2027-01-15",
            "2027-01-22",
            "2027-01-29"
        ]
    );
    // Un lundi sur deux du mer 04/11/2026 au 06/12/2026. Semaines (dim-sam)
    // comptées depuis celle du 04/11 (01/11-07/11) : n° 0 (lundi 02/11, hors
    // période), n° 2 (lundi 16/11), n° 4 (lundi 30/11).
    let e = recurring(
        "2026-11-04",
        "2026-12-06",
        Recurrence::Weekly {
            every: 2,
            days: Week::from_fn(|d| d == Weekday::Monday),
        },
        None,
    );
    assert_eq!(expanded(&e), ["2026-11-16", "2026-11-30"]);
}

#[test]
fn r018_monthly() {
    // Dernier vendredi de janvier à mars 2027 : 29/01, 26/02, 26/03.
    let e = recurring(
        "2027-01-01",
        "2027-03-31",
        Recurrence::MonthlyByPosition {
            every: 1,
            position: MonthPosition::Last,
            item: MonthItem::On(Weekday::Friday),
        },
        None,
    );
    assert_eq!(expanded(&e), ["2027-01-29", "2027-02-26", "2027-03-26"]);
    // Le 31 de janvier à avril 2027 : février et avril n'en ont pas, sautés.
    let e = recurring(
        "2027-01-01",
        "2027-04-30",
        Recurrence::MonthlyByDay { every: 1, day: 31 },
        None,
    );
    assert_eq!(expanded(&e), ["2027-01-31", "2027-03-31"]);
}

#[test]
fn r018_yearly_by_position() {
    // 4e jeudi de novembre, 2026-2027 : 26/11/2026 (jeudis 5, 12, 19, 26) et
    // 25/11/2027 (jeudis 4, 11, 18, 25).
    let e = recurring(
        "2026-01-01",
        "2027-12-31",
        Recurrence::YearlyByPosition {
            month: Month::November,
            position: MonthPosition::Fourth,
            item: MonthItem::On(Weekday::Thursday),
        },
        None,
    );
    assert_eq!(expanded(&e), ["2026-11-26", "2027-11-25"]);
}

#[test]
fn r018_unsupported_recurrence_ignored_with_warning() {
    let e = recurring(
        "2026-11-02",
        "2026-11-30",
        Recurrence::ByDayCount { every: 1 },
        None,
    );
    // Aucun jour.
    assert_eq!(expanded(&e), Vec::<String>::new());
    let mut c = standard();
    c.add_exception(e).unwrap();
    let a = calendars(vec![c.clone()]);
    assert_eq!(day_minutes(&a, 1, "2026-11-02"), 480);
    // Avertissement de `Project::validate`.
    let mut p = Project::new(ProjectInit {
        name: "P".to_owned(),
        anchor: ScheduleAnchor::FromStart(moment("2026-11-02T08:00")),
        current_date: moment("2026-11-02T08:00"),
        calendar_names: BuiltinCalendarNames::default(),
    });
    let mut list: Vec<Calendar> = p.calendars.iter().cloned().collect();
    list[0] = c;
    p.calendars = calendars(list);
    assert_eq!(
        p.validate().finish(),
        [Diagnostic::CalendarRecurrenceUnsupported {
            calendar: CalendarUid::new(1),
            exception: 0
        }]
    );
}

#[test]
fn r018_daily_every_two_days_conversion() {
    use plannercraft_tests::{compile_one, mins};
    let mut c = standard();
    c.add_exception(recurring(
        "2026-11-02",
        "2026-11-06",
        Recurrence::Daily { every: 2 },
        None,
    ))
    .unwrap();
    // Lun 02/11 00:00 → sam 07/11 00:00 : seuls mar 3 et jeu 5 sont ouvrés,
    // 2 × 480 = 960.
    assert_eq!(
        compile_one(c).working_between(moment("2026-11-02T00:00"), moment("2026-11-07T00:00")),
        Ok(mins(960))
    );
}

#[test]
fn r018_recurring_exception_overlapping_a_period() {
    use plannercraft_tests::{compile_one, mins, working};
    // Lues depuis un fichier, stockées à rebours de leurs dates de début :
    // n° 0 = du mar 03/11 au ven 06/11/2026, ouvré [09:00,11:00) ; n° 1 =
    // chaque mercredi du lun 02/11 au lun 30/11, chômé. La n° 1 commence la
    // première : elle possède le mer 04/11 (sa première occurrence) ; la n° 0
    // garde les 03, 05 et 06/11.
    let period = CalendarException {
        name: String::new(),
        period: days("2026-11-03", "2026-11-06"),
        setting: working(&[("09:00", "11:00")]),
        recurrence: Recurrence::Period,
        occurrences: None,
    };
    let wednesdays = recurring(
        "2026-11-02",
        "2026-11-30",
        Recurrence::Weekly {
            every: 1,
            days: Week::from_fn(|d| d == Weekday::Wednesday),
        },
        None,
    );
    let mut json = serde_json::to_value(standard()).unwrap();
    json["exceptions"] = serde_json::to_value(vec![period, wednesdays]).unwrap();
    let c: Calendar = serde_json::from_value(json).unwrap();
    let a = calendars(vec![c.clone()]);
    // Lun 02/11 : 480 ; mar 03/11 : 120 ; mer 04/11 : 0 ; jeu 05 et ven
    // 06/11 : 120 ; sam 07/11 : 0. Total 840.
    let minutes: Vec<i64> = [
        "2026-11-02",
        "2026-11-03",
        "2026-11-04",
        "2026-11-05",
        "2026-11-06",
        "2026-11-07",
    ]
    .iter()
    .map(|d| day_minutes(&a, 1, d))
    .collect();
    assert_eq!(minutes, [480, 120, 0, 120, 120, 0]);
    // Mer 11/11 : chômé par la récurrence, hors de la période n° 0.
    assert_eq!(day_minutes(&a, 1, "2026-11-11"), 0);
    assert_eq!(
        compile_one(c).working_between(moment("2026-11-02T00:00"), moment("2026-11-08T00:00")),
        Ok(mins(840))
    );
}
