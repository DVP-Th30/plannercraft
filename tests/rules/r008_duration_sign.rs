//! R-008 — Signe des durées : durée et travail >= 0, décalage signé.

use plannercraft_core::{
    ConversionFactors, Decimal, Duration, DurationError, DurationUnit, Lag, LagValue, Minutes, TimeBasis,
    Work,
};

const W: TimeBasis = TimeBasis::Working;

#[test]
fn r008_negative_duration_refused() {
    assert_eq!(
        Duration::new(Minutes::new(-480), DurationUnit::Day, W, false),
        Err(DurationError::Negative)
    );
    assert_eq!(Work::new(Minutes::new(-1)), Err(DurationError::Negative));
}

#[test]
fn r008_negative_lag_accepted() {
    // Une avance de 1 j = décalage de −480 min (S15).
    let lag = Lag::new(
        W,
        LagValue::Time {
            minutes: Minutes::new(-480),
            unit: DurationUnit::Day,
        },
    );
    assert!(lag.is_ok());
    // La conversion d'une saisie négative est admise (c'est Duration::new qui refuse).
    let f = ConversionFactors::default();
    assert_eq!(
        f.to_minutes(Decimal::integer(-1), DurationUnit::Day, W),
        Ok(Minutes::new(-480))
    );
}

#[test]
fn r008_bounds() {
    // Travail ≤ 1 666 666 666 h × 60 = 99 999 999 960 min (S11).
    assert!(Work::new(Minutes::new(99_999_999_960)).is_ok());
    assert_eq!(
        Work::new(Minutes::new(99_999_999_961)),
        Err(DurationError::OutOfRange)
    );
    // |durée| et |décalage| ≤ 87 308 640 min (étendue du domaine des dates).
    assert!(Duration::new(Minutes::new(87_308_640), DurationUnit::Day, W, false).is_ok());
    assert_eq!(
        Duration::new(Minutes::new(87_308_641), DurationUnit::Day, W, false),
        Err(DurationError::OutOfRange)
    );
    assert_eq!(
        Lag::new(
            W,
            LagValue::Time {
                minutes: Minutes::new(-87_308_641),
                unit: DurationUnit::Day
            }
        ),
        Err(DurationError::OutOfRange)
    );
}
