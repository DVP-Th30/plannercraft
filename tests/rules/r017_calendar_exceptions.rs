//! R-017 — Exceptions datées : période en jours entiers, bornes incluses ;
//! l'exception remplace la journée entière (chômée ou avec ses propres
//! plages) ; chevauchement dans un même calendrier refusé par une commande, et
//! à l'import la première (par date de début, puis ordre de stockage)
//! l'emporte avec un avertissement.

use plannercraft_core::calendar::{Calendar, DaySetting};
use plannercraft_core::{
    BuiltinCalendarNames, CalendarModelError, CalendarUid, Diagnostic, Project, ProjectInit, ScheduleAnchor,
};
use plannercraft_tests::{calendars, day_minutes, exception, moment, standard, working};

#[test]
fn r017_exception_replaces_whole_day() {
    let mut c = standard();
    // Samedi 24/10/2026 ouvré [08:00,12:00) : 240 au lieu de 0.
    c.add_exception(exception(
        "2026-10-24",
        "2026-10-24",
        working(&[("08:00", "12:00")]),
    ))
    .unwrap();
    // Jeudi 24/12/2026 ouvré [08:00,12:00) : 240 au lieu de 480 (l'après-midi
    // disparaît : la journée entière est remplacée).
    c.add_exception(exception(
        "2026-12-24",
        "2026-12-24",
        working(&[("08:00", "12:00")]),
    ))
    .unwrap();
    let a = calendars(vec![c]);
    assert_eq!(day_minutes(&a, 1, "2026-10-24"), 240);
    assert_eq!(day_minutes(&a, 1, "2026-12-24"), 240);
    // Lendemains inchangés : dim 25/10 chômé, ven 25/12 480.
    assert_eq!(day_minutes(&a, 1, "2026-10-25"), 0);
    assert_eq!(day_minutes(&a, 1, "2026-12-25"), 480);
}

#[test]
fn r017_period_bounds_are_inclusive() {
    let mut c = standard();
    c.add_exception(exception("2026-12-24", "2026-12-31", DaySetting::NonWorking))
        .unwrap();
    let a = calendars(vec![c]);
    // Mer 23/12 : avant ; jeu 24/12 et jeu 31/12 : bornes incluses ; ven
    // 01/01/2027 : après.
    assert_eq!(day_minutes(&a, 1, "2026-12-23"), 480);
    assert_eq!(day_minutes(&a, 1, "2026-12-24"), 0);
    assert_eq!(day_minutes(&a, 1, "2026-12-31"), 0);
    assert_eq!(day_minutes(&a, 1, "2027-01-01"), 480);
}

#[test]
fn r017_overlap_refused_by_command() {
    let mut c = standard();
    c.add_exception(exception("2026-11-11", "2026-11-11", DaySetting::NonWorking))
        .unwrap();
    assert_eq!(
        c.add_exception(exception(
            "2026-11-10",
            "2026-11-12",
            working(&[("08:00", "12:00")])
        )),
        Err(CalendarModelError::OverlappingException { existing: 0 })
    );
}

#[test]
fn r017_overlap_on_import_first_wins_with_warning() {
    // Deux exceptions sur le 11/11 lues depuis un fichier : chômée (n° 0)
    // puis ouvrée 8-12 (n° 1). Même début : l'ordre de stockage départage.
    let mut json = serde_json::to_value(standard()).unwrap();
    json["exceptions"] = serde_json::to_value(vec![
        exception("2026-11-11", "2026-11-11", DaySetting::NonWorking),
        exception("2026-11-11", "2026-11-11", working(&[("08:00", "12:00")])),
    ])
    .unwrap();
    let c: Calendar = serde_json::from_value(json).unwrap();
    let a = calendars(vec![c.clone()]);
    // Le 11/11 est chômé.
    assert_eq!(day_minutes(&a, 1, "2026-11-11"), 0);

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
        [Diagnostic::CalendarExceptionsOverlap {
            calendar: CalendarUid::new(1),
            kept: 0,
            ignored: 1
        }]
    );
}

#[test]
fn r017_working_saturday_conversions() {
    use plannercraft_tests::{compile_one, mins};
    // Samedi 24/10/2026 ouvré [08:00,12:00).
    let mut c = standard();
    c.add_exception(exception(
        "2026-10-24",
        "2026-10-24",
        working(&[("08:00", "12:00")]),
    ))
    .unwrap();
    let c = compile_one(c);
    // Ven 23/10 17:00 → lun 26/10 08:00 : le samedi matin, 240.
    assert_eq!(
        c.working_between(moment("2026-10-23T17:00"), moment("2026-10-26T08:00")),
        Ok(mins(240))
    );
    // Ven 13:00 + 480 : ven 13-17 (240), sam 08-12 (240) → sam 24/10 12:00.
    assert_eq!(
        c.add_working(moment("2026-10-23T13:00"), mins(480)),
        Ok(moment("2026-10-24T12:00"))
    );
}

#[test]
fn r017_christmas_eve_conversions() {
    use plannercraft_tests::{compile_one, mins};
    // Jeu 24/12 ouvré [08:00,12:00) : 08:00 + 480 → jeu 08-12 (240), ven
    // 25/12 08-12 (240) → ven 25/12 12:00.
    let mut c = standard();
    c.add_exception(exception(
        "2026-12-24",
        "2026-12-24",
        working(&[("08:00", "12:00")]),
    ))
    .unwrap();
    assert_eq!(
        compile_one(c.clone()).add_working(moment("2026-12-24T08:00"), mins(480)),
        Ok(moment("2026-12-25T12:00"))
    );
    // Si le 25/12 est aussi chômé : jeu 240, lun 28/12 08-12 (240) → lun
    // 28/12 12:00.
    c.add_exception(exception("2026-12-25", "2026-12-25", DaySetting::NonWorking))
        .unwrap();
    assert_eq!(
        compile_one(c).add_working(moment("2026-12-24T08:00"), mins(480)),
        Ok(moment("2026-12-28T12:00"))
    );
}

#[test]
fn r017_year_end_closure_conversions() {
    use plannercraft_tests::{compile_one, mins};
    // Fermeture du 24/12/2026 au 31/12/2026 : mer 23/12 16:00 + 480 → mer
    // 16-17 (60), fermeture puis ven 01/01/2027 08-12 (240) + 13-16 (180) →
    // ven 01/01/2027 16:00.
    let mut c = standard();
    c.add_exception(exception("2026-12-24", "2026-12-31", DaySetting::NonWorking))
        .unwrap();
    assert_eq!(
        compile_one(c.clone()).add_working(moment("2026-12-23T16:00"), mins(480)),
        Ok(moment("2027-01-01T16:00"))
    );
    // Si le 01/01/2027 est aussi chômé : week-end, puis lun 04/01 → 16:00.
    c.add_exception(exception("2027-01-01", "2027-01-01", DaySetting::NonWorking))
        .unwrap();
    assert_eq!(
        compile_one(c).add_working(moment("2026-12-23T16:00"), mins(480)),
        Ok(moment("2027-01-04T16:00"))
    );
}

#[test]
fn r017_storage_order_differs_from_start_order() {
    // Lues depuis un fichier, stockées à rebours de leurs dates de début :
    // n° 0 = mer 11/11/2026 ouvré [08:00,12:00) ; n° 1 = du lun 09/11 au ven
    // 13/11, chômé. La n° 1 commence la première : elle l'emporte le 11/11,
    // malgré son rang de stockage.
    let mut json = serde_json::to_value(standard()).unwrap();
    json["exceptions"] = serde_json::to_value(vec![
        exception("2026-11-11", "2026-11-11", working(&[("08:00", "12:00")])),
        exception("2026-11-09", "2026-11-13", DaySetting::NonWorking),
    ])
    .unwrap();
    let c: Calendar = serde_json::from_value(json).unwrap();
    let a = calendars(vec![c.clone()]);
    assert_eq!(day_minutes(&a, 1, "2026-11-11"), 0);
    assert_eq!(day_minutes(&a, 1, "2026-11-09"), 0);
    assert_eq!(day_minutes(&a, 1, "2026-11-16"), 480);
    // Forme compilée : semaine du 09/11 entièrement chômée (0, et non 240).
    assert_eq!(
        plannercraft_tests::compile_one(c.clone())
            .working_between(moment("2026-11-09T00:00"), moment("2026-11-14T00:00")),
        Ok(plannercraft_tests::mins(0))
    );
    // Diagnostic : la n° 1 retenue, la n° 0 ignorée.
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
        [Diagnostic::CalendarExceptionsOverlap {
            calendar: CalendarUid::new(1),
            kept: 1,
            ignored: 0
        }]
    );
}
