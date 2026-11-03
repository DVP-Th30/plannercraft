//! R-009 — Durée stockée en minutes ; les options n'agissent qu'aux
//! conversions (affichage à 2 décimales, R-001).

use plannercraft_core::{ConversionFactors, Decimal, Duration, DurationUnit, Minutes, TimeBasis};

const W: TimeBasis = TimeBasis::Working;

fn dec(mantissa: i64, scale: u8) -> Decimal {
    Decimal::new(mantissa, scale).unwrap()
}

#[test]
fn r009_changing_factors_keeps_stored_minutes() {
    // « 84 j » saisi à 480 min/jour : 84 × 480 = 40 320 min.
    let at_480 = ConversionFactors::default();
    let minutes = at_480
        .to_minutes(Decimal::integer(84), DurationUnit::Day, W)
        .unwrap();
    let d = Duration::new(minutes, DurationUnit::Day, W, false).unwrap();
    assert_eq!(d.minutes(), Minutes::new(40_320));
    // Passage à 540 min/jour : la durée stockée ne change pas ; seule la valeur
    // affichée est recalculée : 40 320 / 540 = 74,666… → 74,67 j.
    let at_540 = ConversionFactors::new(540, 2_700, 20).unwrap();
    assert_eq!(d.minutes(), Minutes::new(40_320));
    assert_eq!(
        at_540.to_unit_value(d.minutes(), DurationUnit::Day, W),
        Ok(dec(7_467, 2))
    );
    assert_eq!(
        at_480.to_unit_value(d.minutes(), DurationUnit::Day, W),
        Ok(Decimal::integer(84))
    );
}

#[test]
fn r009_display_value_two_decimals() {
    let f = ConversionFactors::default();
    // 158 / 480 = 0,329… → 0,33 j ; 720 / 480 = 1,5 j ; 2 400 / 2 400 = 1 sem.
    assert_eq!(
        f.to_unit_value(Minutes::new(158), DurationUnit::Day, W),
        Ok(dec(33, 2))
    );
    assert_eq!(
        f.to_unit_value(Minutes::new(720), DurationUnit::Day, W),
        Ok(dec(15, 1))
    );
    assert_eq!(
        f.to_unit_value(Minutes::new(2_400), DurationUnit::Week, W),
        Ok(Decimal::integer(1))
    );
}
