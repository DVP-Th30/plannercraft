//! R-001 — Arrondi unique : au plus proche, la moitié loin de zéro, en `i128`.

use plannercraft_core::{Decimal, round_div};

fn dec(mantissa: i64, scale: u8) -> Decimal {
    Decimal::new(mantissa, scale).unwrap()
}

#[test]
fn r001_day_fractions_round_to_nearest_minute() {
    // 0,01 j × 480 = 4,8 min → 5.
    assert_eq!(dec(1, 2).scale_to_integer(480), Ok(5));
    // 0,33 j × 480 = 158,4 min → 158.
    assert_eq!(dec(33, 2).scale_to_integer(480), Ok(158));
    // 1,5 j × 480 = 720 min exactement.
    assert_eq!(dec(15, 1).scale_to_integer(480), Ok(720));
}

#[test]
fn r001_hour_fraction_rounds_up_from_three_quarters() {
    // 0,3125 h × 60 = 18,75 min → 19.
    assert_eq!(dec(3125, 4).scale_to_integer(60), Ok(19));
}

#[test]
fn r001_halves_go_away_from_zero() {
    // 0,5 min → 1 ; −0,5 min → −1 ; −2,5 min → −3.
    assert_eq!(round_div(5, 10), Some(1));
    assert_eq!(round_div(-5, 10), Some(-1));
    assert_eq!(round_div(-25, 10), Some(-3));
    assert_eq!(dec(5, 1).scale_to_integer(1), Ok(1));
    assert_eq!(dec(-5, 1).scale_to_integer(1), Ok(-1));
    assert_eq!(dec(-25, 1).scale_to_integer(1), Ok(-3));
}

#[test]
fn r001_overflow_is_an_error_not_a_wrap() {
    // i64::MAX × 2 ne tient pas dans un i64 : erreur, jamais de valeur repliée.
    assert!(Decimal::integer(i64::MAX).scale_to_integer(2).is_err());
    assert_eq!(round_div(i128::from(i64::MAX) * 4, 2), None);
    // Dénominateur nul ou négatif : refusé.
    assert_eq!(round_div(10, 0), None);
    assert_eq!(round_div(10, -1), None);
}
