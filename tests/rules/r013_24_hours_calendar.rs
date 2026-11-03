//! R-013 — Calendrier 24 heures : tous les jours 00:00-24:00.

use plannercraft_core::calendar::{CalendarKind, twenty_four_hours};
use plannercraft_core::{CalendarUid, Weekday};

#[test]
fn r013_24_hours_week() {
    let c = twenty_four_hours(CalendarUid::new(2), "24 heures".to_owned());
    let CalendarKind::Base { week } = c.kind() else {
        panic!("24 heures est un calendrier de base");
    };
    // Chaque jour : 1 440 ; semaine : 7 × 1 440 = 10 080.
    for d in Weekday::ALL {
        assert_eq!(week.get(d).total().get(), 1_440, "{d:?}");
    }
    let total: i64 = Weekday::ALL.iter().map(|d| week.get(*d).total().get()).sum();
    assert_eq!(total, 10_080);
}

#[test]
fn r013_24_hours_conversions() {
    use plannercraft_core::{ConversionFactors, Decimal, DurationUnit, TimeBasis};
    use plannercraft_tests::{compile_one, mins, moment};
    let c = compile_one(twenty_four_hours(CalendarUid::new(2), "24 heures".to_owned()));
    // Sam 24/10 10:00 → dim 25/10 10:00 : 24 h ouvrées = 1 440.
    assert_eq!(
        c.working_between(moment("2026-10-24T10:00"), moment("2026-10-25T10:00")),
        Ok(mins(1_440))
    );
    // 1 j = 480 minutes (R-026), même sur un calendrier de 24 h : sam 10:00
    // + 480 → sam 18:00.
    let one_day = ConversionFactors::default()
        .to_minutes(Decimal::integer(1), DurationUnit::Day, TimeBasis::Working)
        .unwrap();
    assert_eq!(
        c.add_working(moment("2026-10-24T10:00"), one_day),
        Ok(moment("2026-10-24T18:00"))
    );
}
