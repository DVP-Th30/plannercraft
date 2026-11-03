//! Durées, travail et décalages en minutes entières (ADR-0005, ADR-0008),
//! facteurs de conversion des unités (R-006 à R-010).
//!
//! Une durée est stockée en minutes avec son unité d'affichage ; les facteurs
//! du projet n'agissent qu'aux conversions (R-009). Le texte (« 5j »,
//! « 3FS+2j ») est lu et écrit par `plannercraft-automation` (J4), pas ici.

use serde::{Deserialize, Serialize};

use crate::error::DurationError;
use crate::fixed::{Decimal, round_div};
use crate::quantity::SignedPercent;

/// Quantité de minutes, signée (durée, écart, décalage) ; entier JSON.
///
/// Pas d'opérateurs `+`/`-` : ils devraient paniquer ou saturer sans le dire
/// (règle 10). Les opérations passent par `checked_*`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Minutes(i64);

impl Minutes {
    /// Zéro minute.
    pub const ZERO: Self = Self(0);

    /// Quantité de minutes.
    #[must_use]
    pub const fn new(v: i64) -> Self {
        Self(v)
    }

    /// Valeur en minutes.
    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }

    /// Somme, `None` en cas de débordement.
    #[must_use]
    pub fn checked_add(self, o: Self) -> Option<Self> {
        self.0.checked_add(o.0).map(Self)
    }

    /// Différence, `None` en cas de débordement.
    #[must_use]
    pub fn checked_sub(self, o: Self) -> Option<Self> {
        self.0.checked_sub(o.0).map(Self)
    }

    /// Opposé, `None` pour `i64::MIN`.
    #[must_use]
    pub fn checked_neg(self) -> Option<Self> {
        self.0.checked_neg().map(Self)
    }

    /// Vrai si strictement négatif.
    #[must_use]
    pub const fn is_negative(self) -> bool {
        self.0 < 0
    }
}

/// Unité d'affichage et de saisie d'une durée.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DurationUnit {
    /// Minute.
    Minute,
    /// Heure.
    Hour,
    /// Jour.
    Day,
    /// Semaine.
    Week,
    /// Mois.
    Month,
}

/// Base de temps : ouvrée (calendrier) ou écoulée (temps civil, 24 h sur 24).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeBasis {
    /// Temps ouvré, consommé sur un calendrier.
    Working,
    /// Temps écoulé : tout le temps compte, y compris chômé (S3).
    Elapsed,
}

/// Facteurs de conversion des unités ouvrées (R-006), entiers comme MSPDI (S2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ConversionFactorsRepr")]
pub struct ConversionFactors {
    minutes_per_day: u32,
    minutes_per_week: u32,
    days_per_month: u32,
}

/// Forme sérialisée de [`ConversionFactors`], validée à la lecture.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConversionFactorsRepr {
    minutes_per_day: u32,
    minutes_per_week: u32,
    days_per_month: u32,
}

impl TryFrom<ConversionFactorsRepr> for ConversionFactors {
    type Error = DurationError;
    fn try_from(r: ConversionFactorsRepr) -> Result<Self, Self::Error> {
        Self::new(r.minutes_per_day, r.minutes_per_week, r.days_per_month)
    }
}

/// Minutes d'une unité écoulée (R-007) : ej = 1 440, esem = 10 080,
/// emois = 30 jours = 43 200 (Q1).
const ELAPSED_MINUTE: i64 = 1;
const ELAPSED_HOUR: i64 = 60;
const ELAPSED_DAY: i64 = 1_440;
const ELAPSED_WEEK: i64 = 10_080;
const ELAPSED_MONTH: i64 = 43_200;

impl ConversionFactors {
    /// Facteurs du projet : minutes par jour (1..=1 440), minutes par semaine
    /// (1..=10 080), jours par mois (1..=31). Bornes : hypothèse. Aucune
    /// cohérence n'est imposée entre jour et semaine (R-006).
    ///
    /// # Errors
    ///
    /// [`DurationError::InvalidFactors`] hors des bornes.
    pub const fn new(
        minutes_per_day: u32,
        minutes_per_week: u32,
        days_per_month: u32,
    ) -> Result<Self, DurationError> {
        let valid = minutes_per_day >= 1
            && minutes_per_day <= 1_440
            && minutes_per_week >= 1
            && minutes_per_week <= 10_080
            && days_per_month >= 1
            && days_per_month <= 31;
        if !valid {
            return Err(DurationError::InvalidFactors {
                minutes_per_day,
                minutes_per_week,
                days_per_month,
            });
        }
        Ok(Self {
            minutes_per_day,
            minutes_per_week,
            days_per_month,
        })
    }

    /// Minutes par jour (`MinutesPerDay`).
    #[must_use]
    pub const fn minutes_per_day(&self) -> u32 {
        self.minutes_per_day
    }

    /// Minutes par semaine (`MinutesPerWeek`).
    #[must_use]
    pub const fn minutes_per_week(&self) -> u32 {
        self.minutes_per_week
    }

    /// Jours par mois (`DaysPerMonth`).
    #[must_use]
    pub const fn days_per_month(&self) -> u32 {
        self.days_per_month
    }

    /// Minutes d'une unité (R-006 en ouvré, R-007 en écoulé). En ouvré :
    /// m = 1, h = 60, j = minutes par jour, sem = minutes par semaine,
    /// mois = jours par mois × minutes par jour.
    #[must_use]
    pub fn minutes_per_unit(&self, unit: DurationUnit, basis: TimeBasis) -> Minutes {
        let m = match basis {
            TimeBasis::Working => match unit {
                DurationUnit::Minute => 1,
                DurationUnit::Hour => 60,
                DurationUnit::Day => i64::from(self.minutes_per_day),
                DurationUnit::Week => i64::from(self.minutes_per_week),
                // Au plus 31 × 1 440 = 44 640 : ne sature jamais.
                DurationUnit::Month => {
                    i64::from(self.days_per_month).saturating_mul(i64::from(self.minutes_per_day))
                }
            },
            TimeBasis::Elapsed => match unit {
                DurationUnit::Minute => ELAPSED_MINUTE,
                DurationUnit::Hour => ELAPSED_HOUR,
                DurationUnit::Day => ELAPSED_DAY,
                DurationUnit::Week => ELAPSED_WEEK,
                DurationUnit::Month => ELAPSED_MONTH,
            },
        };
        Minutes(m)
    }

    /// Saisie décimale → minutes, arrondie par R-001. Un négatif est admis
    /// ici (décalages) ; c'est [`Duration::new`] qui le refuse.
    ///
    /// # Errors
    ///
    /// [`DurationError::Overflow`] si le résultat ne tient pas dans un `i64`.
    pub fn to_minutes(
        &self,
        value: Decimal,
        unit: DurationUnit,
        basis: TimeBasis,
    ) -> Result<Minutes, DurationError> {
        let factor = self.minutes_per_unit(unit, basis).get();
        value
            .scale_to_integer(factor)
            .map(Minutes)
            .map_err(|_| DurationError::Overflow)
    }

    /// Minutes → valeur dans l'unité, à 2 décimales (R-001, R-009), en
    /// écriture canonique (`1,50` rendu `1,5`). Sert à l'affichage et à
    /// l'export.
    ///
    /// # Errors
    ///
    /// [`DurationError::Overflow`] si le calcul déborde.
    pub fn to_unit_value(
        &self,
        m: Minutes,
        unit: DurationUnit,
        basis: TimeBasis,
    ) -> Result<Decimal, DurationError> {
        let factor = self.minutes_per_unit(unit, basis).get();
        let num = i128::from(m.0).checked_mul(100).ok_or(DurationError::Overflow)?;
        let hundredths = round_div(num, i128::from(factor)).ok_or(DurationError::Overflow)?;
        Decimal::new(hundredths, 2)
            .map(Decimal::normalized)
            .map_err(|_| DurationError::Overflow)
    }
}

impl Default for ConversionFactors {
    /// 480 min/jour, 2 400 min/semaine, 20 jours/mois (S8).
    fn default() -> Self {
        Self {
            minutes_per_day: 480,
            minutes_per_week: 2_400,
            days_per_month: 20,
        }
    }
}

/// Durée de tâche : minutes >= 0, unité d'affichage, base (ouvrée ou
/// écoulée), estimation (« ? »). Sérialisée en
/// `{"minutes":2400,"unit":"day","basis":"working","estimated":false}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "DurationRepr")]
pub struct Duration {
    minutes: Minutes,
    unit: DurationUnit,
    basis: TimeBasis,
    estimated: bool,
}

/// Forme sérialisée de [`Duration`], validée à la lecture.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DurationRepr {
    minutes: Minutes,
    unit: DurationUnit,
    basis: TimeBasis,
    estimated: bool,
}

impl TryFrom<DurationRepr> for Duration {
    type Error = DurationError;
    fn try_from(r: DurationRepr) -> Result<Self, Self::Error> {
        Self::new(r.minutes, r.unit, r.basis, r.estimated)
    }
}

impl Duration {
    /// Durée maximale : l'étendue du domaine des dates (87 308 640 min).
    pub const MAX_MINUTES: i64 = 87_308_640;

    /// Durée de tâche (R-008).
    ///
    /// # Errors
    ///
    /// [`DurationError::Negative`] si `minutes < 0`,
    /// [`DurationError::OutOfRange`] au-delà de [`Duration::MAX_MINUTES`].
    pub const fn new(
        minutes: Minutes,
        unit: DurationUnit,
        basis: TimeBasis,
        estimated: bool,
    ) -> Result<Self, DurationError> {
        if minutes.0 < 0 {
            return Err(DurationError::Negative);
        }
        if minutes.0 > Self::MAX_MINUTES {
            return Err(DurationError::OutOfRange);
        }
        Ok(Self {
            minutes,
            unit,
            basis,
            estimated,
        })
    }

    /// Durée nulle ouvrée, non estimée.
    #[must_use]
    pub const fn zero(unit: DurationUnit) -> Self {
        Self {
            minutes: Minutes::ZERO,
            unit,
            basis: TimeBasis::Working,
            estimated: false,
        }
    }

    /// Minutes.
    #[must_use]
    pub const fn minutes(&self) -> Minutes {
        self.minutes
    }

    /// Unité d'affichage.
    #[must_use]
    pub const fn unit(&self) -> DurationUnit {
        self.unit
    }

    /// Base de temps.
    #[must_use]
    pub const fn basis(&self) -> TimeBasis {
        self.basis
    }

    /// Vrai si la durée est estimée (« ? »).
    #[must_use]
    pub const fn is_estimated(&self) -> bool {
        self.estimated
    }

    /// Vrai si la durée est nulle.
    #[must_use]
    pub const fn is_zero(&self) -> bool {
        self.minutes.0 == 0
    }
}

/// Travail : minutes >= 0, au plus 1 666 666 666 h = 99 999 999 960 min
/// (S11). Distinct de [`Duration`] ; entier JSON.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "i64", into = "i64")]
pub struct Work(Minutes);

impl TryFrom<i64> for Work {
    type Error = DurationError;
    fn try_from(v: i64) -> Result<Self, Self::Error> {
        Self::new(Minutes(v))
    }
}

impl From<Work> for i64 {
    fn from(w: Work) -> Self {
        w.0.0
    }
}

impl Work {
    /// Aucun travail.
    pub const ZERO: Self = Self(Minutes::ZERO);
    /// Travail maximal en minutes (S11).
    pub const MAX_MINUTES: i64 = 99_999_999_960;

    /// Travail en minutes (R-008).
    ///
    /// # Errors
    ///
    /// [`DurationError::Negative`] ou [`DurationError::OutOfRange`].
    pub const fn new(m: Minutes) -> Result<Self, DurationError> {
        if m.0 < 0 {
            return Err(DurationError::Negative);
        }
        if m.0 > Self::MAX_MINUTES {
            return Err(DurationError::OutOfRange);
        }
        Ok(Self(m))
    }

    /// Minutes de travail.
    #[must_use]
    pub const fn minutes(self) -> Minutes {
        self.0
    }
}

/// Décalage de lien : une seule source de vérité par information. `Elapsed`
/// avec un pourcentage correspond au format « e% » (S3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "LagRepr")]
pub struct Lag {
    /// Base de temps du décalage.
    pub basis: TimeBasis,
    /// Valeur : durée signée ou pourcentage de la durée du prédécesseur.
    pub value: LagValue,
}

/// Forme sérialisée de [`Lag`], validée à la lecture.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LagRepr {
    basis: TimeBasis,
    value: LagValue,
}

impl TryFrom<LagRepr> for Lag {
    type Error = DurationError;
    fn try_from(r: LagRepr) -> Result<Self, Self::Error> {
        Self::new(r.basis, r.value)
    }
}

/// Valeur d'un décalage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LagValue {
    /// Durée signée (avance = négatif), |minutes| <= [`Duration::MAX_MINUTES`].
    Time {
        /// Minutes, signées.
        minutes: Minutes,
        /// Unité d'affichage.
        unit: DurationUnit,
    },
    /// Pourcentage de la durée du prédécesseur (appliqué en J2, R-010).
    Percent {
        /// Pourcentage signé en 1/10 000.
        percent: SignedPercent,
    },
}

impl Lag {
    /// Décalage nul : ouvré, 0 jour.
    pub const ZERO: Self = Self {
        basis: TimeBasis::Working,
        value: LagValue::Time {
            minutes: Minutes::ZERO,
            unit: DurationUnit::Day,
        },
    };

    /// Décalage validé (R-008).
    ///
    /// # Errors
    ///
    /// [`DurationError::OutOfRange`] si |minutes| > [`Duration::MAX_MINUTES`].
    pub const fn new(basis: TimeBasis, value: LagValue) -> Result<Self, DurationError> {
        if let LagValue::Time { minutes, .. } = value
            && (minutes.0 > Duration::MAX_MINUTES || minutes.0 < -Duration::MAX_MINUTES)
        {
            return Err(DurationError::OutOfRange);
        }
        Ok(Self { basis, value })
    }
}

impl Default for Lag {
    fn default() -> Self {
        Self::ZERO
    }
}

/// R-010 : minutes = `round_div(durée du prédécesseur × pct, 10 000)`.
///
/// # Errors
///
/// [`DurationError::Overflow`] si le résultat ne tient pas dans un `i64`.
pub fn percent_of(duration: Minutes, pct: SignedPercent) -> Result<Minutes, DurationError> {
    let num = i128::from(duration.0)
        .checked_mul(i128::from(pct.get()))
        .ok_or(DurationError::Overflow)?;
    round_div(num, 10_000).map(Minutes).ok_or(DurationError::Overflow)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{
        ConversionFactors, Duration, DurationUnit, Lag, LagValue, Minutes, TimeBasis, Work, percent_of,
    };
    use crate::error::DurationError;
    use crate::fixed::Decimal;
    use crate::quantity::SignedPercent;

    const W: TimeBasis = TimeBasis::Working;
    const E: TimeBasis = TimeBasis::Elapsed;

    #[test]
    fn factors_bounds() {
        assert!(ConversionFactors::new(1, 1, 1).is_ok());
        assert!(ConversionFactors::new(1_440, 10_080, 31).is_ok());
        for (d, w, m) in [
            (0, 2400, 20),
            (1441, 2400, 20),
            (480, 0, 20),
            (480, 10_081, 20),
            (480, 2400, 0),
            (480, 2400, 32),
        ] {
            assert_eq!(
                ConversionFactors::new(d, w, m),
                Err(DurationError::InvalidFactors {
                    minutes_per_day: d,
                    minutes_per_week: w,
                    days_per_month: m
                })
            );
        }
        let f = ConversionFactors::default();
        assert_eq!(
            (f.minutes_per_day(), f.minutes_per_week(), f.days_per_month()),
            (480, 2_400, 20)
        );
    }

    #[test]
    fn factors_per_unit() {
        let f = ConversionFactors::new(450, 2_250, 21).unwrap();
        assert_eq!(f.minutes_per_unit(DurationUnit::Minute, W).get(), 1);
        assert_eq!(f.minutes_per_unit(DurationUnit::Hour, W).get(), 60);
        assert_eq!(f.minutes_per_unit(DurationUnit::Day, W).get(), 450);
        assert_eq!(f.minutes_per_unit(DurationUnit::Week, W).get(), 2_250);
        assert_eq!(f.minutes_per_unit(DurationUnit::Month, W).get(), 9_450);
        assert_eq!(f.minutes_per_unit(DurationUnit::Day, E).get(), 1_440);
        assert_eq!(f.minutes_per_unit(DurationUnit::Week, E).get(), 10_080);
        assert_eq!(f.minutes_per_unit(DurationUnit::Month, E).get(), 43_200);
        let max = ConversionFactors::new(1_440, 10_080, 31).unwrap();
        assert_eq!(max.minutes_per_unit(DurationUnit::Month, W).get(), 44_640);
    }

    #[test]
    fn to_minutes_and_back() {
        let f = ConversionFactors::default();
        let dec = |m, s| Decimal::new(m, s).unwrap();
        assert_eq!(
            f.to_minutes(dec(15, 1), DurationUnit::Day, W),
            Ok(Minutes::new(720))
        );
        assert_eq!(
            f.to_minutes(Decimal::integer(-1), DurationUnit::Day, W),
            Ok(Minutes::new(-480))
        );
        assert_eq!(
            f.to_minutes(Decimal::integer(i64::MAX), DurationUnit::Hour, W),
            Err(DurationError::Overflow)
        );
        assert_eq!(
            f.to_unit_value(Minutes::new(720), DurationUnit::Day, W),
            Ok(dec(15, 1))
        );
        assert_eq!(
            f.to_unit_value(Minutes::new(2_400), DurationUnit::Week, W),
            Ok(Decimal::integer(1))
        );
        assert_eq!(
            f.to_unit_value(Minutes::new(-158), DurationUnit::Day, W),
            Ok(dec(-33, 2))
        );
        assert_eq!(
            f.to_unit_value(Minutes::new(i64::MAX), DurationUnit::Minute, W),
            Err(DurationError::Overflow)
        );
    }

    #[test]
    fn duration_new() {
        assert_eq!(
            Duration::new(Minutes::new(-1), DurationUnit::Day, W, false),
            Err(DurationError::Negative)
        );
        assert_eq!(
            Duration::new(Minutes::new(87_308_641), DurationUnit::Day, W, false),
            Err(DurationError::OutOfRange)
        );
        let d = Duration::new(Minutes::new(87_308_640), DurationUnit::Week, E, true).unwrap();
        assert_eq!(
            (
                d.minutes().get(),
                d.unit(),
                d.basis(),
                d.is_estimated(),
                d.is_zero()
            ),
            (87_308_640, DurationUnit::Week, E, true, false)
        );
        let z = Duration::zero(DurationUnit::Hour);
        assert!(z.is_zero() && !z.is_estimated() && z.basis() == W && z.unit() == DurationUnit::Hour);
    }

    #[test]
    fn work_new() {
        assert_eq!(Work::new(Minutes::new(-1)), Err(DurationError::Negative));
        assert_eq!(
            Work::new(Minutes::new(99_999_999_961)),
            Err(DurationError::OutOfRange)
        );
        assert_eq!(
            Work::new(Minutes::new(99_999_999_960)).unwrap().minutes().get(),
            99_999_999_960
        );
        assert_eq!(
            serde_json::to_string(&Work::new(Minutes::new(60)).unwrap()).unwrap(),
            "60"
        );
        assert!(serde_json::from_str::<Work>("-1").is_err());
    }

    #[test]
    fn lag_new_and_percent_of() {
        let t = |m| LagValue::Time {
            minutes: Minutes::new(m),
            unit: DurationUnit::Day,
        };
        assert!(Lag::new(W, t(-87_308_640)).is_ok());
        assert_eq!(Lag::new(W, t(-87_308_641)), Err(DurationError::OutOfRange));
        assert_eq!(Lag::new(E, t(87_308_641)), Err(DurationError::OutOfRange));
        assert_eq!(Lag::default(), Lag::ZERO);
        let pct = SignedPercent::new(5_000).unwrap();
        assert_eq!(percent_of(Minutes::new(1_440), pct), Ok(Minutes::new(720)));
        assert_eq!(
            percent_of(Minutes::new(i64::MAX), SignedPercent::new(1_000_000).unwrap()),
            Err(DurationError::Overflow)
        );
    }

    #[test]
    fn golden_json() {
        let d = Duration::new(Minutes::new(2_400), DurationUnit::Day, W, false).unwrap();
        let json = serde_json::to_string(&d).unwrap();
        assert_eq!(
            json,
            r#"{"minutes":2400,"unit":"day","basis":"working","estimated":false}"#
        );
        assert_eq!(serde_json::from_str::<Duration>(&json).unwrap(), d);
        assert!(
            serde_json::from_str::<Duration>(
                r#"{"minutes":-1,"unit":"day","basis":"working","estimated":false}"#
            )
            .is_err()
        );
        assert!(serde_json::from_str::<Duration>(r#"{"minutes":1,"unit":"day","basis":"working"}"#).is_err());

        let lag = Lag::new(
            W,
            LagValue::Time {
                minutes: Minutes::new(-480),
                unit: DurationUnit::Day,
            },
        )
        .unwrap();
        let json = serde_json::to_string(&lag).unwrap();
        assert_eq!(
            json,
            r#"{"basis":"working","value":{"kind":"time","minutes":-480,"unit":"day"}}"#
        );
        assert_eq!(serde_json::from_str::<Lag>(&json).unwrap(), lag);

        let lag = Lag::new(
            E,
            LagValue::Percent {
                percent: SignedPercent::new(-5_000).unwrap(),
            },
        )
        .unwrap();
        let json = serde_json::to_string(&lag).unwrap();
        assert_eq!(
            json,
            r#"{"basis":"elapsed","value":{"kind":"percent","percent":-5000}}"#
        );
        assert_eq!(serde_json::from_str::<Lag>(&json).unwrap(), lag);
        assert!(
            serde_json::from_str::<Lag>(
                r#"{"basis":"working","value":{"kind":"time","minutes":-87308641,"unit":"day"}}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<Lag>(
                r#"{"basis":"working","value":{"kind":"percent","percent":1000001}}"#
            )
            .is_err()
        );

        let f = ConversionFactors::default();
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(
            json,
            r#"{"minutes_per_day":480,"minutes_per_week":2400,"days_per_month":20}"#
        );
        assert_eq!(serde_json::from_str::<ConversionFactors>(&json).unwrap(), f);
        assert!(
            serde_json::from_str::<ConversionFactors>(
                r#"{"minutes_per_day":0,"minutes_per_week":2400,"days_per_month":20}"#
            )
            .is_err()
        );
    }
}

#[cfg(test)]
mod minutes_tests {
    use super::Minutes;

    #[test]
    fn minutes_checked_ops() {
        assert_eq!(
            Minutes::new(5).checked_add(Minutes::new(-7)),
            Some(Minutes::new(-2))
        );
        assert_eq!(Minutes::new(i64::MAX).checked_add(Minutes::new(1)), None);
        assert_eq!(Minutes::new(i64::MIN).checked_sub(Minutes::new(1)), None);
        assert_eq!(Minutes::new(i64::MIN).checked_neg(), None);
        assert_eq!(Minutes::new(3).checked_neg(), Some(Minutes::new(-3)));
        assert!(Minutes::new(-1).is_negative());
        assert!(!Minutes::ZERO.is_negative());
    }
}
