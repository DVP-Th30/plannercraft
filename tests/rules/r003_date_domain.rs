//! R-003 — Domaine des dates et résolution à la minute.

use plannercraft_core::{Day, Moment, TimeError, Weekday};
use plannercraft_tests::moment;

#[test]
fn r003_reference_moment() {
    // 1984-01-01 → 2026-10-19 : 42 ans dont 11 bissextiles (1984, 1988, …,
    // 2024) = 42 × 365 + 11 = 15 341 jours ; + 291 jours (01/01 → 19/10/2026,
    // 2026 non bissextile : 31+28+31+30+31+30+31+31+30 = 273, + 18) = 15 632.
    // 15 632 × 1 440 = 22 510 080 ; + 8 h × 60 = 22 510 560.
    assert_eq!(moment("2026-10-19T08:00").minutes(), 22_510_560);
}

#[test]
fn r003_domain_bounds() {
    assert_eq!("1983-12-31T23:59".parse::<Moment>(), Err(TimeError::OutOfDomain));
    assert_eq!(moment("1984-01-01T00:00"), Moment::MIN);
    // 2150-01-01T00:00 = fin du 31/12/2149 : 60 631 jours × 1 440 = 87 308 640.
    assert_eq!(moment("2150-01-01T00:00"), Moment::MAX);
    assert_eq!(Moment::MAX.minutes(), 87_308_640);
    assert_eq!("2150-01-01T00:01".parse::<Moment>(), Err(TimeError::OutOfDomain));
    assert_eq!(Moment::from_minutes(-1), Err(TimeError::OutOfDomain));
}

#[test]
fn r003_day_zero_is_a_sunday_and_last_day_a_wednesday() {
    assert_eq!(Day::MIN.weekday(), Weekday::Sunday);
    assert_eq!(Day::MIN.to_string(), "1984-01-01");
    assert_eq!(Day::MAX.to_string(), "2149-12-31");
    assert_eq!(Day::MAX.weekday(), Weekday::Wednesday);
    assert_eq!(Day::MAX.index(), 60_630);
}
