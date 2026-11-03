//! R-007 — Unités écoulées : em = 1 ; eh = 60 ; ej = 1 440 ; esem = 10 080 ;
//! emois = 43 200 (30 jours), indépendants des options.

use plannercraft_core::{ConversionFactors, Decimal, DurationUnit, TimeBasis};

const E: TimeBasis = TimeBasis::Elapsed;

#[test]
fn r007_elapsed_factors() {
    let f = ConversionFactors::default();
    // 2 ej × 1 440 = 2 880 ; 5 ej × 1 440 = 7 200 ; 1 emois = 30 × 1 440 = 43 200.
    assert_eq!(
        f.to_minutes(Decimal::integer(2), DurationUnit::Day, E)
            .unwrap()
            .get(),
        2_880
    );
    assert_eq!(
        f.to_minutes(Decimal::integer(5), DurationUnit::Day, E)
            .unwrap()
            .get(),
        7_200
    );
    assert_eq!(
        f.to_minutes(Decimal::integer(1), DurationUnit::Month, E)
            .unwrap()
            .get(),
        43_200
    );
    // 1 esem = 7 × 1 440 = 10 080 ; 1 eh = 60 ; 1 em = 1.
    assert_eq!(
        f.to_minutes(Decimal::integer(1), DurationUnit::Week, E)
            .unwrap()
            .get(),
        10_080
    );
    assert_eq!(
        f.to_minutes(Decimal::integer(1), DurationUnit::Hour, E)
            .unwrap()
            .get(),
        60
    );
    assert_eq!(
        f.to_minutes(Decimal::integer(1), DurationUnit::Minute, E)
            .unwrap()
            .get(),
        1
    );
}

#[test]
fn r007_elapsed_ignores_options() {
    // Avec MinutesPerDay = 420 (et 7 j/mois), 1 ej vaut toujours 1 440 et
    // 1 emois 43 200.
    let f = ConversionFactors::new(420, 2_100, 7).unwrap();
    assert_eq!(
        f.to_minutes(Decimal::integer(1), DurationUnit::Day, E)
            .unwrap()
            .get(),
        1_440
    );
    assert_eq!(
        f.to_minutes(Decimal::integer(1), DurationUnit::Month, E)
            .unwrap()
            .get(),
        43_200
    );
}
