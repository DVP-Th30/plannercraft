//! R-014 — Calendrier Équipe de nuit : la plage de 23:00 appartient au jour
//! qui commence.

use plannercraft_core::calendar::{CalendarKind, DaySetting, night_shift};
use plannercraft_core::{CalendarUid, Weekday};

fn ranges(setting: &DaySetting) -> Vec<(u16, u16)> {
    match setting {
        DaySetting::NonWorking => vec![],
        DaySetting::Working(h) => h
            .ranges()
            .iter()
            .map(|r| (r.from().minutes() / 60, r.to().minutes() / 60))
            .collect(),
    }
}

#[test]
fn r014_night_shift_week() {
    let c = night_shift(CalendarUid::new(3), "Équipe de nuit".to_owned());
    let CalendarKind::Base { week } = c.kind() else {
        panic!("Équipe de nuit est un calendrier de base");
    };
    // Dimanche chômé ; lundi 23-24 (60) ; mardi à vendredi 0-3, 4-8, 23-24
    // (180 + 240 + 60 = 480) ; samedi 0-3, 4-8 (180 + 240 = 420).
    assert_eq!(ranges(week.get(Weekday::Sunday)), []);
    assert_eq!(ranges(week.get(Weekday::Monday)), [(23, 24)]);
    for d in [
        Weekday::Tuesday,
        Weekday::Wednesday,
        Weekday::Thursday,
        Weekday::Friday,
    ] {
        assert_eq!(ranges(week.get(d)), [(0, 3), (4, 8), (23, 24)], "{d:?}");
        assert_eq!(week.get(d).total().get(), 480);
    }
    assert_eq!(ranges(week.get(Weekday::Saturday)), [(0, 3), (4, 8)]);
    // Semaine : 60 + 4 × 480 + 420 = 2 400.
    let total: i64 = Weekday::ALL.iter().map(|d| week.get(*d).total().get()).sum();
    assert_eq!(total, 2_400);
}

fn night(from: &str, d: i64) -> plannercraft_core::Moment {
    plannercraft_tests::compile_one(night_shift(CalendarUid::new(3), "Équipe de nuit".to_owned()))
        .add_working(plannercraft_tests::moment(from), plannercraft_tests::mins(d))
        .unwrap()
}

#[test]
fn r014_night_shift_one_night() {
    use plannercraft_tests::moment;
    // Lun 19/10 23:00 + 480 : lun 23-24 (60), mar 00-03 (180), mar 04-08
    // (240) → mar 20/10 08:00.
    assert_eq!(night("2026-10-19T23:00", 480), moment("2026-10-20T08:00"));
    // Lun 00:00 + 480 : rien avant 23:00, même nuit → mar 08:00.
    assert_eq!(night("2026-10-19T00:00", 480), moment("2026-10-20T08:00"));
}

#[test]
fn r014_night_shift_break() {
    use plannercraft_tests::{compile_one, moment};
    let c = compile_one(night_shift(CalendarUid::new(3), "Équipe de nuit".to_owned()));
    // Mar 03:00 : pause jusqu'à 04:00.
    assert_eq!(
        c.next_working_moment(moment("2026-10-20T03:00")),
        Ok(moment("2026-10-20T04:00"))
    );
    // Mar 03:00 + 60 : 04:00-05:00 → 05:00.
    assert_eq!(night("2026-10-20T03:00", 60), moment("2026-10-20T05:00"));
}

#[test]
fn r014_night_shift_weekend() {
    use plannercraft_tests::moment;
    // Ven 23/10 23:00 + 480 : ven 23-24 (60), sam 00-03 (180), sam 04-08
    // (240) → sam 24/10 08:00.
    assert_eq!(night("2026-10-23T23:00", 480), moment("2026-10-24T08:00"));
    // + 481 : plus rien du sam 08:00 au lun 23:00 → lun 26/10 23:01.
    assert_eq!(night("2026-10-23T23:00", 481), moment("2026-10-26T23:01"));
    // Sam 24/10 08:00 + 60 : lun 26 23:00-24:00 → mar 27/10 00:00.
    assert_eq!(night("2026-10-24T08:00", 60), moment("2026-10-27T00:00"));
}
