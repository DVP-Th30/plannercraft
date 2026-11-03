//! R-002 — Unités, pourcentages et coûts en point fixe, conversion par R-001.

use plannercraft_core::{Decimal, Money, Percent, SignedPercent, Units};

fn dec(mantissa: i64, scale: u8) -> Decimal {
    Decimal::new(mantissa, scale).unwrap()
}

#[test]
fn r002_units_from_fraction_and_percent() {
    // 1,5 × 10 000 = 15 000 ; 150 % × 100 = 15 000.
    assert_eq!(Units::from_fraction(dec(15, 1)).unwrap().get(), 15_000);
    assert_eq!(Units::from_percent(Decimal::integer(150)).unwrap().get(), 15_000);
}

#[test]
fn r002_units_upper_bound_is_sixty_million() {
    // 60 000 000 unités = 600 000 000 000 : accepté ; 60 000 001 : refusé (S11).
    assert_eq!(
        Units::from_fraction(Decimal::integer(60_000_000)).unwrap().get(),
        600_000_000_000
    );
    assert!(Units::from_fraction(Decimal::integer(60_000_001)).is_err());
    assert!(Units::new(-1).is_err());
}

#[test]
fn r002_percent_rounding_and_bounds() {
    // 33,33 % × 100 = 3 333 ; 33,335 % × 100 = 3 333,5 → 3 334 (moitié loin de zéro).
    assert_eq!(Percent::from_percent(dec(3333, 2)).unwrap().get(), 3_333);
    assert_eq!(Percent::from_percent(dec(33_335, 3)).unwrap().get(), 3_334);
    // 101 % → 10 100 > 10 000 : refusé.
    assert!(Percent::from_percent(Decimal::integer(101)).is_err());
    assert!(Percent::from_percent(Decimal::integer(-1)).is_err());
}

#[test]
fn r002_money_in_hundredths() {
    // 12,345 × 100 = 1 234,5 → 1 235 ; −0,005 × 100 = −0,5 → −1.
    assert_eq!(Money::from_decimal(dec(12_345, 3)).unwrap().get(), 1_235);
    assert_eq!(Money::from_decimal(dec(-5, 3)).unwrap().get(), -1);
    // 999 999 999 999,99 accepté ; un centième de plus refusé (S11).
    assert_eq!(
        Money::from_decimal(dec(99_999_999_999_999, 2)).unwrap().get(),
        99_999_999_999_999
    );
    assert!(Money::from_decimal(dec(100_000_000_000_000, 2)).is_err());
}

#[test]
fn r002_signed_percent_bounds() {
    // ±10 000 % = ±1 000 000 (borne : hypothèse).
    assert_eq!(
        SignedPercent::from_percent(Decimal::integer(-10_000))
            .unwrap()
            .get(),
        -1_000_000
    );
    assert!(SignedPercent::from_percent(Decimal::integer(10_001)).is_err());
    assert_eq!(
        SignedPercent::from_percent(Decimal::integer(-50)).unwrap().get(),
        -5_000
    );
}
