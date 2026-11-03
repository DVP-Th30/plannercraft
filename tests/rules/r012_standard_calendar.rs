//! R-012 — Calendrier Standard : lun-ven 08:00-12:00 et 13:00-17:00, sam et
//! dim chômés.

use plannercraft_core::calendar::{CalendarKind, DaySetting, standard};
use plannercraft_core::{CalendarUid, Weekday};

#[test]
fn r012_standard_week() {
    let c = standard(CalendarUid::new(1), "Standard".to_owned());
    let CalendarKind::Base { week } = c.kind() else {
        panic!("Standard est un calendrier de base");
    };
    // Lundi à vendredi : 240 + 240 = 480 ; samedi et dimanche : chômés.
    for d in [
        Weekday::Monday,
        Weekday::Tuesday,
        Weekday::Wednesday,
        Weekday::Thursday,
        Weekday::Friday,
    ] {
        assert_eq!(week.get(d).total().get(), 480, "{d:?}");
    }
    assert_eq!(*week.get(Weekday::Saturday), DaySetting::NonWorking);
    assert_eq!(*week.get(Weekday::Sunday), DaySetting::NonWorking);
    // Semaine : 5 × 480 = 2 400.
    let total: i64 = Weekday::ALL.iter().map(|d| week.get(*d).total().get()).sum();
    assert_eq!(total, 2_400);
}

#[test]
fn r012_standard_conversions() {
    use plannercraft_tests::{compile_one, mins, moment, standard};
    let c = compile_one(standard());
    // Lun 19/10 08:00 → ven 23/10 17:00 : 5 × 480 = 2 400.
    assert_eq!(
        c.working_between(moment("2026-10-19T08:00"), moment("2026-10-23T17:00")),
        Ok(mins(2_400))
    );
    // Samedi 24/10 entier : 0.
    assert_eq!(
        c.working_between(moment("2026-10-24T00:00"), moment("2026-10-25T00:00")),
        Ok(mins(0))
    );
}
