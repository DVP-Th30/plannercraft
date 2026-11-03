//! R-019 — Semaines de travail datées : sur leur période (jours inclus),
//! elles remplacent la semaine par défaut du même calendrier pour les jours
//! qu'elles définissent ; `Inherit` renvoie à la semaine par défaut.

use plannercraft_core::calendar::{Calendar, DayRule, WorkWeek};
use plannercraft_core::{Week, Weekday};
use plannercraft_tests::{calendars, day_minutes, days, hours, standard};

/// Standard avec une semaine datée sur `[from, to]`.
fn with_work_week(from: &str, to: &str, rule: impl Fn(Weekday) -> DayRule) -> Calendar {
    let mut c = standard();
    c.add_work_week(WorkWeek {
        name: "Été".to_owned(),
        period: days(from, to),
        days: Week::from_fn(rule),
    })
    .unwrap();
    c
}

#[test]
fn r019_summer_hours() {
    // Du 01/07/2026 au 31/08/2026, lun-ven [08:00,15:00) = 420.
    let c = with_work_week("2026-07-01", "2026-08-31", |d| match d {
        Weekday::Saturday | Weekday::Sunday => DayRule::Inherit,
        _ => DayRule::Working(hours(&[("08:00", "15:00")])),
    });
    let a = calendars(vec![c]);
    // Mar 30/06 : avant la période, 480. Mer 01/07 (borne) : 420.
    assert_eq!(day_minutes(&a, 1, "2026-06-30"), 480);
    assert_eq!(day_minutes(&a, 1, "2026-07-01"), 420);
    assert_eq!(day_minutes(&a, 1, "2026-07-06"), 420);
    // Lun 31/08 (borne) : 420 ; mar 01/09 : 480.
    assert_eq!(day_minutes(&a, 1, "2026-08-31"), 420);
    assert_eq!(day_minutes(&a, 1, "2026-09-01"), 480);
    // Samedi hérité de la semaine par défaut : chômé.
    assert_eq!(day_minutes(&a, 1, "2026-07-04"), 0);
}

#[test]
fn r019_inherit_falls_back_to_default_week() {
    // Du 01/07/2027 au 31/08/2027 : lun-jeu hérités (Standard, 480),
    // vendredi [08:00,12:00) = 240.
    let c = with_work_week("2027-07-01", "2027-08-31", |d| match d {
        Weekday::Friday => DayRule::Working(hours(&[("08:00", "12:00")])),
        _ => DayRule::Inherit,
    });
    let a = calendars(vec![c]);
    // Semaine du dim 04/07/2027 au sam 10/07 : 4 × 480 + 240 = 2 160.
    let week: i64 = [
        "2027-07-04",
        "2027-07-05",
        "2027-07-06",
        "2027-07-07",
        "2027-07-08",
        "2027-07-09",
        "2027-07-10",
    ]
    .iter()
    .map(|d| day_minutes(&a, 1, d))
    .sum();
    assert_eq!(week, 2_160);
}

#[test]
fn r019_work_week_conversions() {
    use plannercraft_tests::{compile_one, mins, moment};
    // Été 2026, lun-ven [08:00,15:00) : lun 06/07 08:00 + 480 → lundi 420,
    // mar 07/07 08:00-09:00 (60) → mar 07/07 09:00.
    let c = with_work_week("2026-07-01", "2026-08-31", |d| match d {
        Weekday::Saturday | Weekday::Sunday => DayRule::Inherit,
        _ => DayRule::Working(hours(&[("08:00", "15:00")])),
    });
    assert_eq!(
        compile_one(c).add_working(moment("2026-07-06T08:00"), mins(480)),
        Ok(moment("2026-07-07T09:00"))
    );
    // Été 2027, vendredi [08:00,12:00) seul : dim 04/07 → dim 11/07/2027 =
    // 4 × 480 + 240 = 2 160.
    let c = with_work_week("2027-07-01", "2027-08-31", |d| match d {
        Weekday::Friday => DayRule::Working(hours(&[("08:00", "12:00")])),
        _ => DayRule::Inherit,
    });
    assert_eq!(
        compile_one(c).working_between(moment("2027-07-04T00:00"), moment("2027-07-11T00:00")),
        Ok(mins(2_160))
    );
}

#[test]
fn r019_overlap_storage_order_differs_from_start_order() {
    use plannercraft_core::calendar::Calendar;
    use plannercraft_core::{
        BuiltinCalendarNames, CalendarUid, Diagnostic, Project, ProjectInit, ScheduleAnchor,
    };
    use plannercraft_tests::{compile_one, mins, moment};
    // Lues depuis un fichier (une commande refuse le chevauchement), stockées
    // à rebours de leurs dates de début :
    // - n° 0 : du lun 09/11 au lun 30/11/2026, lundi [08:00,10:00), autres
    //   jours hérités ;
    // - n° 1 : du dim 01/11 au dim 15/11/2026, mardi chômé, autres jours
    //   hérités.
    // La n° 1 commence la première : elle possède les jours communs (09 au
    // 15/11), y compris son lundi hérité, qui renvoie à la semaine par défaut
    // (480) et non au lundi de la n° 0 (hypothèse, R-019).
    let ww = |from: &str, to: &str, wd: Weekday, rule: DayRule| WorkWeek {
        name: String::new(),
        period: days(from, to),
        days: Week::from_fn(|d| if d == wd { rule.clone() } else { DayRule::Inherit }),
    };
    let mut json = serde_json::to_value(standard()).unwrap();
    json["work_weeks"] = serde_json::to_value(vec![
        ww(
            "2026-11-09",
            "2026-11-30",
            Weekday::Monday,
            DayRule::Working(hours(&[("08:00", "10:00")])),
        ),
        ww("2026-11-01", "2026-11-15", Weekday::Tuesday, DayRule::NonWorking),
    ])
    .unwrap();
    let c: Calendar = serde_json::from_value(json).unwrap();
    let a = calendars(vec![c.clone()]);
    // Lun 02/11 : 480 (n° 1, lundi hérité) ; mar 03/11 : 0 (n° 1).
    assert_eq!(day_minutes(&a, 1, "2026-11-02"), 480);
    assert_eq!(day_minutes(&a, 1, "2026-11-03"), 0);
    // Lun 09/11 : n° 1, lundi hérité → 480 ; mar 10/11 : 0.
    assert_eq!(day_minutes(&a, 1, "2026-11-09"), 480);
    assert_eq!(day_minutes(&a, 1, "2026-11-10"), 0);
    // Lun 16/11 : seule la n° 0 couvre → 120 ; mar 17/11 hérité → 480.
    assert_eq!(day_minutes(&a, 1, "2026-11-16"), 120);
    assert_eq!(day_minutes(&a, 1, "2026-11-17"), 480);
    // Du lun 09/11 00:00 au mer 18/11 00:00 : 480 + 0 + 3 × 480 + 120 + 480
    // = 2 520.
    assert_eq!(
        compile_one(c.clone()).working_between(moment("2026-11-09T00:00"), moment("2026-11-18T00:00")),
        Ok(mins(2_520))
    );
    // Diagnostic : la n° 1 retenue, la n° 0 ignorée sur les jours communs.
    let mut p = Project::new(ProjectInit {
        name: "P".to_owned(),
        anchor: ScheduleAnchor::FromStart(moment("2026-11-09T08:00")),
        current_date: moment("2026-11-09T08:00"),
        calendar_names: BuiltinCalendarNames::default(),
    });
    let mut list: Vec<Calendar> = p.calendars.iter().cloned().collect();
    list[0] = c;
    p.calendars = calendars(list);
    assert_eq!(
        p.validate().finish(),
        [Diagnostic::CalendarWorkWeeksOverlap {
            calendar: CalendarUid::new(1),
            kept: 1,
            ignored: 0
        }]
    );
}
