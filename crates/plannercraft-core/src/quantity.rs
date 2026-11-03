//! Quantités en point fixe (ADR-0005, R-002) : unités d'affectation et
//! pourcentages en 1/10 000, coûts en centièmes.
//!
//! Chaque type borne sa valeur à la construction et à la désérialisation
//! (`#[serde(try_from = "i64")]`) : aucune valeur hors bornes ne peut exister.
//! Sérialisation en entiers JSON ; la présentation « 50% » appartient au MCP
//! et à l'interface.

use serde::{Deserialize, Serialize};

use crate::error::FixedError;
use crate::fixed::Decimal;

/// Vérifie `min <= v <= max`.
fn check_range(value: i64, min: i64, max: i64) -> Result<i64, FixedError> {
    if value < min || value > max {
        return Err(FixedError::OutOfRange {
            min,
            max,
            value: i128::from(value),
        });
    }
    Ok(value)
}

/// Implémente `TryFrom<i64>` (via `new`) et `From<T> for i64` pour un newtype.
macro_rules! int_newtype_serde {
    ($ty:ident) => {
        impl TryFrom<i64> for $ty {
            type Error = FixedError;
            fn try_from(v: i64) -> Result<Self, Self::Error> {
                Self::new(v)
            }
        }
        impl From<$ty> for i64 {
            fn from(v: $ty) -> i64 {
                v.0
            }
        }
    };
}

/// Unités d'affectation en 1/10 000 : 10 000 = 100 % = 1,0 en MSPDI.
///
/// Bornes : 0 à 600 000 000 000, soit 60 000 000 unités (S11).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "i64", into = "i64")]
pub struct Units(i64);
int_newtype_serde!(Units);

impl Units {
    /// Valeur minimale.
    pub const MIN: i64 = 0;
    /// Valeur maximale : 60 000 000 unités (S11).
    pub const MAX: i64 = 600_000_000_000;
    /// 100 %.
    pub const FULL: Self = Self(10_000);
    /// Aucune unité.
    pub const ZERO: Self = Self(0);

    /// Unités en 1/10 000.
    ///
    /// # Errors
    ///
    /// [`FixedError::OutOfRange`] hors de `0..=600_000_000_000`.
    pub fn new(v: i64) -> Result<Self, FixedError> {
        check_range(v, Self::MIN, Self::MAX).map(Self)
    }

    /// Depuis une fraction (`1,5` → 15 000), arrondie par R-001.
    ///
    /// # Errors
    ///
    /// [`FixedError::Overflow`] ou [`FixedError::OutOfRange`].
    pub fn from_fraction(v: Decimal) -> Result<Self, FixedError> {
        Self::new(v.scale_to_integer(10_000)?)
    }

    /// Depuis un pourcentage (`150` → 15 000), arrondi par R-001.
    ///
    /// # Errors
    ///
    /// [`FixedError::Overflow`] ou [`FixedError::OutOfRange`].
    pub fn from_percent(v: Decimal) -> Result<Self, FixedError> {
        Self::new(v.scale_to_integer(100)?)
    }

    /// Valeur en 1/10 000.
    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }
}

/// Pourcentage d'avancement en 1/10 000 : 0 à 10 000 (100 %).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "i64", into = "i64")]
pub struct Percent(i64);
int_newtype_serde!(Percent);

impl Percent {
    /// 0 %.
    pub const ZERO: Self = Self(0);
    /// 100 %.
    pub const FULL: Self = Self(10_000);

    /// Pourcentage en 1/10 000.
    ///
    /// # Errors
    ///
    /// [`FixedError::OutOfRange`] hors de `0..=10_000`.
    pub fn new(v: i64) -> Result<Self, FixedError> {
        check_range(v, 0, 10_000).map(Self)
    }

    /// Depuis un pourcentage décimal (`33,335` → 3 334), arrondi par R-001.
    ///
    /// # Errors
    ///
    /// [`FixedError::Overflow`] ou [`FixedError::OutOfRange`].
    pub fn from_percent(v: Decimal) -> Result<Self, FixedError> {
        Self::new(v.scale_to_integer(100)?)
    }

    /// Valeur en 1/10 000.
    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }
}

impl Default for Percent {
    fn default() -> Self {
        Self::ZERO
    }
}

/// Pourcentage signé en 1/10 000 (décalage de lien en % de la durée du
/// prédécesseur) : ±1 000 000, soit ±10 000 % (borne : hypothèse).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "i64", into = "i64")]
pub struct SignedPercent(i64);
int_newtype_serde!(SignedPercent);

impl SignedPercent {
    /// Borne absolue : 10 000 %.
    pub const LIMIT: i64 = 1_000_000;
    const LIMIT_NEG: i64 = -1_000_000;

    /// Pourcentage signé en 1/10 000.
    ///
    /// # Errors
    ///
    /// [`FixedError::OutOfRange`] hors de `-1_000_000..=1_000_000`.
    pub fn new(v: i64) -> Result<Self, FixedError> {
        check_range(v, Self::LIMIT_NEG, Self::LIMIT).map(Self)
    }

    /// Depuis un pourcentage décimal (`-50` → −5 000), arrondi par R-001.
    ///
    /// # Errors
    ///
    /// [`FixedError::Overflow`] ou [`FixedError::OutOfRange`].
    pub fn from_percent(v: Decimal) -> Result<Self, FixedError> {
        Self::new(v.scale_to_integer(100)?)
    }

    /// Valeur en 1/10 000.
    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }
}

/// Montant en centièmes d'unité monétaire, signé ; |v| <= 99 999 999 999 999
/// (999 999 999 999,99, S11).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "i64", into = "i64")]
pub struct Money(i64);
int_newtype_serde!(Money);

impl Money {
    /// Borne absolue en centièmes.
    pub const LIMIT: i64 = 99_999_999_999_999;
    const LIMIT_NEG: i64 = -99_999_999_999_999;
    /// Zéro.
    pub const ZERO: Self = Self(0);

    /// Montant en centièmes.
    ///
    /// # Errors
    ///
    /// [`FixedError::OutOfRange`] si |v| > 99 999 999 999 999.
    pub fn new(v: i64) -> Result<Self, FixedError> {
        check_range(v, Self::LIMIT_NEG, Self::LIMIT).map(Self)
    }

    /// Depuis un montant décimal (`12,345` → 1 235), arrondi par R-001.
    ///
    /// # Errors
    ///
    /// [`FixedError::Overflow`] ou [`FixedError::OutOfRange`].
    pub fn from_decimal(v: Decimal) -> Result<Self, FixedError> {
        Self::new(v.scale_to_integer(100)?)
    }

    /// Valeur en centièmes.
    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }
}

impl Default for Money {
    fn default() -> Self {
        Self::ZERO
    }
}

/// Unité de temps (ou de matériau) d'un taux.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateUnit {
    /// Par minute.
    Minute,
    /// Par heure.
    Hour,
    /// Par jour.
    Day,
    /// Par semaine.
    Week,
    /// Par mois.
    Month,
    /// Par an.
    Year,
    /// Par unité de matériau (ressource matérielle).
    MaterialUnit,
}

/// Taux conservé tel que saisi : montant et unité, sans conversion en €/h.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rate {
    /// Montant par unité.
    pub amount: Money,
    /// Unité du taux.
    pub per: RateUnit,
}

impl Default for Rate {
    /// 0 par heure.
    fn default() -> Self {
        Self {
            amount: Money::ZERO,
            per: RateUnit::Hour,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{Money, Percent, Rate, RateUnit, SignedPercent, Units};
    use crate::error::FixedError;
    use crate::fixed::Decimal;

    fn dec(m: i64, s: u8) -> Decimal {
        Decimal::new(m, s).unwrap()
    }

    #[test]
    fn units_bounds() {
        assert_eq!(Units::new(0).unwrap().get(), 0);
        assert_eq!(Units::new(600_000_000_000).unwrap().get(), 600_000_000_000);
        assert_eq!(
            Units::new(-1),
            Err(FixedError::OutOfRange {
                min: 0,
                max: 600_000_000_000,
                value: -1
            })
        );
        assert!(Units::new(600_000_000_001).is_err());
        assert_eq!(Units::FULL.get(), 10_000);
        assert_eq!(Units::from_fraction(dec(15, 1)).unwrap().get(), 15_000);
        assert_eq!(Units::from_percent(Decimal::integer(150)).unwrap().get(), 15_000);
        assert_eq!(
            Units::from_fraction(Decimal::integer(i64::MAX)),
            Err(FixedError::Overflow)
        );
    }

    #[test]
    fn percent_bounds() {
        assert!(Percent::new(-1).is_err());
        assert!(Percent::new(10_001).is_err());
        assert_eq!(Percent::FULL.get(), 10_000);
        assert_eq!(Percent::default(), Percent::ZERO);
        assert_eq!(Percent::from_percent(dec(3333, 2)).unwrap().get(), 3_333);
    }

    #[test]
    fn signed_percent_bounds() {
        assert_eq!(SignedPercent::new(-1_000_000).unwrap().get(), -1_000_000);
        assert!(SignedPercent::new(1_000_001).is_err());
        assert!(SignedPercent::new(-1_000_001).is_err());
        assert_eq!(
            SignedPercent::from_percent(Decimal::integer(-50)).unwrap().get(),
            -5_000
        );
    }

    #[test]
    fn money_bounds() {
        assert_eq!(Money::new(99_999_999_999_999).unwrap().get(), 99_999_999_999_999);
        assert!(Money::new(100_000_000_000_000).is_err());
        assert!(Money::new(-100_000_000_000_000).is_err());
        assert_eq!(Money::from_decimal(dec(12_345, 3)).unwrap().get(), 1_235);
    }

    #[test]
    fn serde_integers_validated() {
        assert_eq!(serde_json::to_string(&Units::FULL).unwrap(), "10000");
        assert_eq!(serde_json::from_str::<Units>("15000").unwrap().get(), 15_000);
        assert!(serde_json::from_str::<Units>("-1").is_err());
        assert!(serde_json::from_str::<Percent>("10001").is_err());
        assert!(serde_json::from_str::<SignedPercent>("-1000001").is_err());
        assert!(serde_json::from_str::<Money>("100000000000000").is_err());
        assert!(serde_json::from_str::<Units>("1.5").is_err());
        let rate = Rate {
            amount: Money::new(5_000).unwrap(),
            per: RateUnit::Hour,
        };
        let json = serde_json::to_string(&rate).unwrap();
        assert_eq!(json, r#"{"amount":5000,"per":"hour"}"#);
        assert_eq!(serde_json::from_str::<Rate>(&json).unwrap(), rate);
        assert_eq!(Rate::default().per, RateUnit::Hour);
    }
}
