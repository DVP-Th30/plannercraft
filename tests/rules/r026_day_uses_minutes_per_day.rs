//! R-026 — Un jour vaut `MinutesPerDay`, quelles que soient les plages du
//! calendrier : une durée en jours est convertie en minutes par R-006, puis
//! consommée sur le calendrier.

use plannercraft_core::calendar::{Calendar, DaySetting};
use plannercraft_core::{
    CalendarUid, ConversionFactors, Decimal, DurationUnit, Minutes, Moment, TimeBasis, Week, Weekday,
};
use plannercraft_tests::{compile_one, moment, standard, working};

/// Calendrier lun-ven avec ces plages.
fn weekdays(ranges: &[(&str, &str)]) -> Calendar {
    Calendar::new_base(
        CalendarUid::new(7),
        "Test".to_owned(),
        Week::from_fn(|d| match d {
            Weekday::Saturday | Weekday::Sunday => DaySetting::NonWorking,
            _ => working(ranges),
        }),
    )
}

fn days(factors: &ConversionFactors, n: i64) -> Minutes {
    factors
        .to_minutes(Decimal::integer(n), DurationUnit::Day, TimeBasis::Working)
        .unwrap()
}

fn add(c: Calendar, from: Moment, d: Minutes) -> Moment {
    compile_one(c).add_working(from, d).unwrap()
}

#[test]
fn r026_calendar_shorter_than_a_day() {
    // Plages de 420 min/jour, 1 j = 480 : lun 19/10 08:00 + 480 → lundi 420,
    // mardi 08:00-09:00 (60) → mar 20/10 09:00.
    let f = ConversionFactors::default();
    let c = weekdays(&[("08:00", "12:00"), ("13:00", "16:00")]);
    assert_eq!(
        add(c, moment("2026-10-19T08:00"), days(&f, 1)),
        moment("2026-10-20T09:00")
    );
}

#[test]
fn r026_calendar_longer_than_a_day() {
    // Plages de 540 min/jour, 1 j = 480 : 08-12 (240) + 13-17 (240) → lun
    // 17:00, avant la fin de la plage à 18:00.
    let f = ConversionFactors::default();
    let c = weekdays(&[("08:00", "12:00"), ("13:00", "18:00")]);
    assert_eq!(
        add(c, moment("2026-10-19T08:00"), days(&f, 1)),
        moment("2026-10-19T17:00")
    );
}

#[test]
fn r026_standard_with_420_minutes_per_day() {
    // 1 j = 420 : 2 j = 840. Lun 08:00 + 840 : lundi 480, mardi 360 → 08-12
    // (240) + 13-15 (120) → mar 15:00.
    let f = ConversionFactors::new(420, 2_400, 20).unwrap();
    assert_eq!(days(&f, 2), Minutes::new(840));
    assert_eq!(
        add(standard(), moment("2026-10-19T08:00"), days(&f, 2)),
        moment("2026-10-20T15:00")
    );
}
