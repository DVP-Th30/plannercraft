//! P12 : `round_div` est égal à l'arrondi de référence, calculé en fractions
//! `i128` sur des entrées aléatoires (ADR-0008, § 9.3).

#![allow(
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use plannercraft_core::round_div;
use proptest::prelude::*;

/// Arrondi de référence : compare `2 × |num|` aux multiples de `den`, écrit
/// autrement que l'implémentation (pas de reste, comparaison de doubles).
fn reference(num: i128, den: i128) -> i128 {
    let abs = num.abs();
    // Plus grand q tel que q × den <= |num|.
    let q = abs / den;
    // Moitié ou plus : 2 × (|num| − q × den) >= den.
    let up = 2 * (abs - q * den) >= den;
    let magnitude = if up { q + 1 } else { q };
    if num < 0 { -magnitude } else { magnitude }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    #[test]
    fn p12_round_div_matches_reference(num in -(1_i128 << 100)..(1_i128 << 100), den in 1_i128..(1_i128 << 40)) {
        let expected = reference(num, den);
        match round_div(num, den) {
            Some(v) => prop_assert_eq!(i128::from(v), expected),
            None => prop_assert!(i64::try_from(expected).is_err()),
        }
    }

    #[test]
    fn p12_round_div_small_values(num in -100_000_i128..100_000, den in 1_i128..1_000) {
        prop_assert_eq!(round_div(num, den).map(i128::from), Some(reference(num, den)));
    }

    #[test]
    fn p12_round_div_rejects_non_positive_denominator(num in any::<i64>(), den in -1_000_i128..=0) {
        prop_assert_eq!(round_div(i128::from(num), den), None);
    }
}
