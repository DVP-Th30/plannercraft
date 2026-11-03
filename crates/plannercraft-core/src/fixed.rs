//! Point fixe : arrondi unique (R-001) et décimal exact saisi ou importé.
//!
//! Toute division vers un entier du moteur (minutes, 1/10 000, centièmes)
//! passe par [`round_div`] : au plus proche, la moitié loin de zéro, calcul en
//! `i128`, erreur plutôt que débordement (ADR-0005, ADR-0008).

use serde::{Deserialize, Serialize};

use crate::error::FixedError;

/// R-001 : `num / den` arrondi au plus proche, la moitié loin de zéro.
///
/// Renvoie `None` si `den <= 0` ou si le résultat ne tient pas dans un `i64`.
///
/// Exemples : `round_div(48, 10) == Some(5)` (4,8) ; `round_div(-5, 10) ==
/// Some(-1)` (−0,5) ; `round_div(-25, 10) == Some(-3)` (−2,5).
#[must_use]
pub fn round_div(num: i128, den: i128) -> Option<i64> {
    if den <= 0 {
        return None;
    }
    let quotient = num.checked_div(den)?;
    // Reste de même signe que `num` (division tronquée vers zéro).
    let remainder = num.checked_rem(den)?;
    let remainder_abs = remainder.checked_abs()?;
    // `2 × |r| >= den` écrit sans multiplication : `|r| >= den − |r|`
    // (`den − |r| > 0` car `|r| < den`).
    let rounded = if remainder_abs >= den.checked_sub(remainder_abs)? {
        if num < 0 {
            quotient.checked_sub(1)?
        } else {
            quotient.checked_add(1)?
        }
    } else {
        quotient
    };
    i64::try_from(rounded).ok()
}

/// `10^scale` en `i128` (scale <= [`Decimal::MAX_SCALE`]).
fn pow10(scale: u8) -> Option<i128> {
    10_i128.checked_pow(u32::from(scale))
}

/// Décimal exact saisi ou importé : valeur = `mantissa / 10^scale`.
///
/// L'égalité est structurelle : `1,5` s'écrit `Decimal::new(15, 1)` et
/// `Decimal::new(150, 2)` en est une autre écriture, distincte pour `==`.
/// [`Decimal::normalized`] donne l'écriture canonique (sans zéro final).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "DecimalRepr")]
pub struct Decimal {
    mantissa: i64,
    scale: u8,
}

/// Forme sérialisée de [`Decimal`], validée à la lecture.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DecimalRepr {
    mantissa: i64,
    scale: u8,
}

impl TryFrom<DecimalRepr> for Decimal {
    type Error = FixedError;
    fn try_from(repr: DecimalRepr) -> Result<Self, Self::Error> {
        Self::new(repr.mantissa, repr.scale)
    }
}

impl Decimal {
    /// Nombre maximal de décimales.
    pub const MAX_SCALE: u8 = 6;

    /// Décimal `mantissa / 10^scale`.
    ///
    /// # Errors
    ///
    /// [`FixedError::TooManyDecimals`] si `scale > 6`.
    pub const fn new(mantissa: i64, scale: u8) -> Result<Self, FixedError> {
        if scale > Self::MAX_SCALE {
            return Err(FixedError::TooManyDecimals);
        }
        Ok(Self { mantissa, scale })
    }

    /// Entier exact (`scale` = 0).
    #[must_use]
    pub const fn integer(v: i64) -> Self {
        Self {
            mantissa: v,
            scale: 0,
        }
    }

    /// Mantisse.
    #[must_use]
    pub const fn mantissa(self) -> i64 {
        self.mantissa
    }

    /// Nombre de décimales.
    #[must_use]
    pub const fn scale(self) -> u8 {
        self.scale
    }

    /// Écriture canonique : zéros finaux retirés (`1,50` → `1,5`, `2,00` → `2`).
    #[must_use]
    pub fn normalized(self) -> Self {
        let mut out = self;
        while out.scale > 0 && out.mantissa.checked_rem(10) == Some(0) {
            match (out.mantissa.checked_div(10), out.scale.checked_sub(1)) {
                (Some(mantissa), Some(scale)) => out = Self { mantissa, scale },
                _ => break,
            }
        }
        out
    }

    /// Conversion vers une unité entière : `round_div(mantissa × factor,
    /// 10^scale)` (R-001). Exemple : 1,5 j avec `factor` = 480 → 720.
    ///
    /// # Errors
    ///
    /// [`FixedError::Overflow`] si le résultat ne tient pas dans un `i64`.
    pub fn scale_to_integer(self, factor: i64) -> Result<i64, FixedError> {
        let num = i128::from(self.mantissa)
            .checked_mul(i128::from(factor))
            .ok_or(FixedError::Overflow)?;
        let den = pow10(self.scale).ok_or(FixedError::Overflow)?;
        round_div(num, den).ok_or(FixedError::Overflow)
    }
}

impl std::fmt::Display for Decimal {
    /// Écriture avec un point décimal (`-1.50`) : présentation technique,
    /// la mise en forme localisée appartient au MCP et à l'interface.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.scale == 0 {
            return write!(f, "{}", self.mantissa);
        }
        let den = pow10(self.scale).ok_or(std::fmt::Error)?;
        let value = i128::from(self.mantissa);
        let abs = value.checked_abs().ok_or(std::fmt::Error)?;
        let int_part = abs.checked_div(den).ok_or(std::fmt::Error)?;
        let frac_part = abs.checked_rem(den).ok_or(std::fmt::Error)?;
        let sign = if value < 0 { "-" } else { "" };
        let width = usize::from(self.scale);
        write!(f, "{sign}{int_part}.{frac_part:0width$}")
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::arithmetic_side_effects,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{Decimal, round_div};
    use crate::error::FixedError;

    #[test]
    fn round_div_halves_away_from_zero() {
        assert_eq!(round_div(5, 10), Some(1));
        assert_eq!(round_div(-5, 10), Some(-1));
        assert_eq!(round_div(15, 10), Some(2));
        assert_eq!(round_div(-15, 10), Some(-2));
        assert_eq!(round_div(25, 10), Some(3));
        assert_eq!(round_div(-25, 10), Some(-3));
        assert_eq!(round_div(14, 10), Some(1));
        assert_eq!(round_div(-14, 10), Some(-1));
        assert_eq!(round_div(16, 10), Some(2));
        assert_eq!(round_div(0, 7), Some(0));
        assert_eq!(round_div(7, 1), Some(7));
        assert_eq!(round_div(-1, 3), Some(0));
        assert_eq!(round_div(2, 3), Some(1));
    }

    #[test]
    fn round_div_rejects_bad_denominator_and_overflow() {
        assert_eq!(round_div(1, 0), None);
        assert_eq!(round_div(1, -2), None);
        assert_eq!(round_div(i128::from(i64::MAX) + 1, 1), None);
        assert_eq!(round_div(i128::from(i64::MIN) - 1, 1), None);
        assert_eq!(round_div(i128::from(i64::MIN), 1), Some(i64::MIN));
        assert_eq!(round_div(i128::MIN, i128::MAX), Some(-1));
        assert_eq!(round_div(i128::MAX, i128::MAX), Some(1));
        assert_eq!(round_div(i128::MAX, 2), None);
    }

    #[test]
    fn decimal_scale_is_bounded() {
        assert_eq!(Decimal::new(1, 7), Err(FixedError::TooManyDecimals));
        assert_eq!(Decimal::new(1, 6).unwrap().scale(), 6);
        assert_eq!(Decimal::integer(-3).mantissa(), -3);
    }

    #[test]
    fn decimal_scale_to_integer() {
        assert_eq!(Decimal::new(15, 1).unwrap().scale_to_integer(480), Ok(720));
        assert_eq!(Decimal::new(1, 2).unwrap().scale_to_integer(480), Ok(5));
        assert_eq!(Decimal::new(-5, 1).unwrap().scale_to_integer(1), Ok(-1));
        assert_eq!(
            Decimal::integer(i64::MAX).scale_to_integer(2),
            Err(FixedError::Overflow)
        );
        assert_eq!(Decimal::integer(i64::MIN).scale_to_integer(1), Ok(i64::MIN));
    }

    #[test]
    fn decimal_normalized_and_display() {
        assert_eq!(
            Decimal::new(150, 2).unwrap().normalized(),
            Decimal::new(15, 1).unwrap()
        );
        assert_eq!(Decimal::new(200, 2).unwrap().normalized(), Decimal::integer(2));
        assert_eq!(Decimal::new(0, 3).unwrap().normalized(), Decimal::integer(0));
        assert_eq!(Decimal::new(-150, 2).unwrap().to_string(), "-1.50");
        assert_eq!(Decimal::new(5, 3).unwrap().to_string(), "0.005");
        assert_eq!(Decimal::new(-5, 3).unwrap().to_string(), "-0.005");
        assert_eq!(Decimal::integer(42).to_string(), "42");
        assert_eq!(
            Decimal::new(i64::MIN, 6).unwrap().to_string(),
            "-9223372036854.775808"
        );
    }

    #[test]
    fn decimal_serde_is_validated() {
        let d: Decimal = serde_json::from_str(r#"{"mantissa":15,"scale":1}"#).unwrap();
        assert_eq!(d, Decimal::new(15, 1).unwrap());
        assert_eq!(serde_json::to_string(&d).unwrap(), r#"{"mantissa":15,"scale":1}"#);
        assert!(serde_json::from_str::<Decimal>(r#"{"mantissa":15,"scale":7}"#).is_err());
        assert!(serde_json::from_str::<Decimal>(r#"{"mantissa":15,"scale":1,"x":0}"#).is_err());
    }
}
