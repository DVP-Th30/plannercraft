//! Temps du projet à la minute (R-003 à R-005) : instants, jours, heures.
//!
//! Heure locale du projet, sans fuseau ni heure d'été : chaque jour dure
//! 1 440 minutes. Domaine : du 1984-01-01 00:00 au 2150-01-01 00:00 (= fin du
//! 31/12/2149), bornes de MS Project (S11). chrono ne sert qu'aux frontières
//! (conversion, lecture et écriture de texte).

use std::fmt;
use std::str::FromStr;

use chrono::{Datelike, Days, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use serde::{Deserialize, Serialize};

use crate::duration::Minutes;
use crate::error::TimeError;

/// Minutes par jour civil (R-005).
const MINUTES_PER_DAY: i64 = 1_440;
/// Nombre de jours du domaine : 1984-01-01 à 2149-12-31 inclus.
const DAY_COUNT: i32 = 60_631;
/// Dernier jour du domaine (2149-12-31, un mercredi).
const LAST_DAY: i32 = 60_630;
/// Dernier instant du domaine : 60 631 × 1 440.
const MAX_MINUTES: i64 = 87_308_640;

/// 1984-01-01, origine du temps (un dimanche).
const EPOCH: NaiveDate = match NaiveDate::from_ymd_opt(1984, 1, 1) {
    Some(date) => date,
    None => NaiveDate::MIN,
};

/// Instant à la minute, heure locale du projet, sans fuseau ni heure d'été.
///
/// Minutes depuis 1984-01-01T00:00. Sérialisé en `"2026-10-19T08:00"` ; la
/// lecture tolère un `":00"` final (secondes nulles), rien d'autre.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Moment(i64);

impl Moment {
    /// 1984-01-01T00:00.
    pub const MIN: Self = Self(0);
    /// 2150-01-01T00:00, c'est-à-dire le 2149-12-31 à 24:00 (inclus).
    pub const MAX: Self = Self(MAX_MINUTES);

    /// Instant depuis un nombre de minutes depuis 1984-01-01T00:00.
    ///
    /// # Errors
    ///
    /// [`TimeError::OutOfDomain`] hors de `0..=87_308_640`.
    pub const fn from_minutes(m: i64) -> Result<Self, TimeError> {
        if m < 0 || m > MAX_MINUTES {
            return Err(TimeError::OutOfDomain);
        }
        Ok(Self(m))
    }

    /// Instant d'un jour à une heure ; 24:00 donne le lendemain 00:00.
    ///
    /// # Errors
    ///
    /// [`TimeError::OutOfDomain`] (impossible dans le domaine, gardé par
    /// sûreté) ou [`TimeError::Overflow`].
    pub fn at(day: Day, time: TimeOfDay) -> Result<Self, TimeError> {
        let m = i64::from(day.0)
            .checked_mul(MINUTES_PER_DAY)
            .and_then(|m| m.checked_add(i64::from(time.0)))
            .ok_or(TimeError::Overflow)?;
        Self::from_minutes(m)
    }

    /// Saisie stricte depuis chrono : des secondes non nulles sont refusées.
    ///
    /// # Errors
    ///
    /// [`TimeError::NonZeroSeconds`] ou [`TimeError::OutOfDomain`].
    pub fn from_naive(dt: NaiveDateTime) -> Result<Self, TimeError> {
        match Self::from_naive_floor(dt)? {
            (moment, false) => Ok(moment),
            (_, true) => Err(TimeError::NonZeroSeconds),
        }
    }

    /// R-004 : import tolérant, secondes tronquées (plancher). Le booléen vaut
    /// `true` si une troncature a eu lieu.
    ///
    /// # Errors
    ///
    /// [`TimeError::OutOfDomain`] hors du domaine ; [`TimeError::Overflow`]
    /// pour une date chrono extrême.
    pub fn from_naive_floor(dt: NaiveDateTime) -> Result<(Self, bool), TimeError> {
        let days = dt.date().signed_duration_since(EPOCH).num_days();
        let minutes_in_day = i64::from(dt.hour())
            .checked_mul(60)
            .and_then(|m| m.checked_add(i64::from(dt.minute())))
            .ok_or(TimeError::Overflow)?;
        let m = days
            .checked_mul(MINUTES_PER_DAY)
            .and_then(|m| m.checked_add(minutes_in_day))
            .ok_or(TimeError::Overflow)?;
        let truncated = dt.second() != 0 || dt.nanosecond() != 0;
        Ok((Self::from_minutes(m)?, truncated))
    }

    /// Date et heure chrono ; totale sur le domaine.
    #[must_use]
    pub fn to_naive(self) -> NaiveDateTime {
        let midnight = NaiveDateTime::new(EPOCH, NaiveTime::MIN);
        // Toujours défini sur le domaine (vérifié exhaustivement par les
        // tests) ; l'origine sert de repli plutôt qu'une panique.
        chrono::TimeDelta::try_minutes(self.0)
            .and_then(|d| midnight.checked_add_signed(d))
            .unwrap_or(midnight)
    }

    /// Minutes depuis 1984-01-01T00:00.
    #[must_use]
    pub const fn minutes(self) -> i64 {
        self.0
    }

    /// Jour de l'instant. [`Moment::MAX`] appartient au 2149-12-31 (à 24:00).
    #[must_use]
    pub fn day(self) -> Day {
        let index = self.0.checked_div(MINUTES_PER_DAY).unwrap_or(0);
        let index = i32::try_from(index).unwrap_or(LAST_DAY).min(LAST_DAY);
        Day(index)
    }

    /// Heure de l'instant dans son jour ; 24:00 pour [`Moment::MAX`].
    #[must_use]
    pub fn time_of_day(self) -> TimeOfDay {
        let start = i64::from(self.day().0).checked_mul(MINUTES_PER_DAY).unwrap_or(0);
        let within = self.0.checked_sub(start).unwrap_or(0);
        TimeOfDay(u16::try_from(within).unwrap_or(0))
    }

    /// Jour de semaine de l'instant (celui de [`Moment::day`]).
    #[must_use]
    pub fn weekday(self) -> Weekday {
        self.day().weekday()
    }

    /// Temps civil : instant + `d` minutes, sans calendrier (R-028).
    ///
    /// # Errors
    ///
    /// [`TimeError::OutOfDomain`] si le résultat sort du domaine,
    /// [`TimeError::Overflow`] si le calcul déborde.
    pub fn checked_add(self, d: Minutes) -> Result<Self, TimeError> {
        let m = self.0.checked_add(d.get()).ok_or(TimeError::Overflow)?;
        Self::from_minutes(m)
    }

    /// Minutes civiles de `self` à `later` (négatif si `later` est avant).
    /// Ne déborde pas : deux instants du domaine sont à moins de 2^27 minutes.
    #[must_use]
    pub fn minutes_until(self, later: Self) -> Minutes {
        Minutes::new(later.0.saturating_sub(self.0))
    }
}

impl fmt::Display for Moment {
    /// `2026-10-19T08:00`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let dt = self.to_naive();
        write!(
            f,
            "{:04}-{:02}-{:02}T{:02}:{:02}",
            dt.year(),
            dt.month(),
            dt.day(),
            dt.hour(),
            dt.minute()
        )
    }
}

impl FromStr for Moment {
    type Err = TimeError;

    /// `AAAA-MM-JJTHH:MM`, éventuellement suivi de `:00`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || TimeError::InvalidFormat { input: s.to_owned() };
        let bytes = s.as_bytes();
        let date_part = bytes.get(..10).ok_or_else(invalid)?;
        let rest = bytes.get(10..).ok_or_else(invalid)?;
        let date = parse_date(date_part).ok_or_else(invalid)?;
        let time = match rest {
            [b'T', h1, h2, b':', m1, m2] | [b'T', h1, h2, b':', m1, m2, b':', b'0', b'0'] => {
                let hour = parse_digits(&[*h1, *h2]).ok_or_else(invalid)?;
                let minute = parse_digits(&[*m1, *m2]).ok_or_else(invalid)?;
                NaiveTime::from_hms_opt(hour, minute, 0).ok_or_else(invalid)?
            }
            _ => return Err(invalid()),
        };
        Self::from_naive(NaiveDateTime::new(date, time))
    }
}

impl TryFrom<String> for Moment {
    type Error = TimeError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<Moment> for String {
    fn from(m: Moment) -> Self {
        m.to_string()
    }
}

/// Valeur d'un chiffre ASCII.
fn digit(b: u8) -> Option<u32> {
    char::from(b).to_digit(10)
}

/// Entier écrit en chiffres ASCII uniquement (pas de signe, pas d'espace).
fn parse_digits(bytes: &[u8]) -> Option<u32> {
    if bytes.is_empty() {
        return None;
    }
    bytes
        .iter()
        .try_fold(0_u32, |acc, b| acc.checked_mul(10)?.checked_add(digit(*b)?))
}

/// `AAAA-MM-JJ` strict.
fn parse_date(bytes: &[u8]) -> Option<NaiveDate> {
    match bytes {
        [y1, y2, y3, y4, b'-', mo1, mo2, b'-', d1, d2] => {
            let year = i32::try_from(parse_digits(&[*y1, *y2, *y3, *y4])?).ok()?;
            let month = parse_digits(&[*mo1, *mo2])?;
            let day = parse_digits(&[*d1, *d2])?;
            NaiveDate::from_ymd_opt(year, month, day)
        }
        _ => None,
    }
}

/// Jour civil : nombre de jours depuis 1984-01-01, de 0 à 60 630
/// (2149-12-31). Sérialisé en `"2026-12-25"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Day(i32);

impl Day {
    /// 1984-01-01 (un dimanche).
    pub const MIN: Self = Self(0);
    /// 2149-12-31 (un mercredi).
    pub const MAX: Self = Self(LAST_DAY);

    /// Jour depuis son numéro.
    ///
    /// # Errors
    ///
    /// [`TimeError::OutOfDomain`] hors de `0..=60_630`.
    pub const fn new(v: i32) -> Result<Self, TimeError> {
        if v < 0 || v >= DAY_COUNT {
            return Err(TimeError::OutOfDomain);
        }
        Ok(Self(v))
    }

    /// Jour depuis une date chrono.
    ///
    /// # Errors
    ///
    /// [`TimeError::OutOfDomain`] hors du domaine.
    pub fn from_naive(d: NaiveDate) -> Result<Self, TimeError> {
        let days = d.signed_duration_since(EPOCH).num_days();
        let days = i32::try_from(days).map_err(|_| TimeError::OutOfDomain)?;
        Self::new(days)
    }

    /// Date chrono ; totale sur le domaine.
    #[must_use]
    pub fn to_naive(self) -> NaiveDate {
        // Toujours défini sur le domaine (vérifié exhaustivement par les
        // tests) ; l'origine sert de repli plutôt qu'une panique.
        u64::try_from(self.0)
            .ok()
            .and_then(|n| EPOCH.checked_add_days(Days::new(n)))
            .unwrap_or(EPOCH)
    }

    /// Jour de semaine ; le jour 0 est un dimanche.
    #[must_use]
    pub fn weekday(self) -> Weekday {
        Weekday::from_sunday_index(self.0.checked_rem_euclid(7).unwrap_or(0))
    }

    /// Jour + `n` jours.
    ///
    /// # Errors
    ///
    /// [`TimeError::OutOfDomain`] hors du domaine, [`TimeError::Overflow`].
    pub fn checked_add_days(self, n: i32) -> Result<Self, TimeError> {
        Self::new(self.0.checked_add(n).ok_or(TimeError::Overflow)?)
    }

    /// Numéro du jour depuis 1984-01-01.
    #[must_use]
    pub const fn index(self) -> i32 {
        self.0
    }
}

impl fmt::Display for Day {
    /// `2026-12-25`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let d = self.to_naive();
        write!(f, "{:04}-{:02}-{:02}", d.year(), d.month(), d.day())
    }
}

impl FromStr for Day {
    type Err = TimeError;
    /// `AAAA-MM-JJ` strict.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let date =
            parse_date(s.as_bytes()).ok_or_else(|| TimeError::InvalidFormat { input: s.to_owned() })?;
        Self::from_naive(date)
    }
}

impl TryFrom<String> for Day {
    type Error = TimeError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<Day> for String {
    fn from(d: Day) -> Self {
        d.to_string()
    }
}

/// Heure dans un jour, à la minute : de 00:00 à 24:00 inclus (24:00 = fin du
/// jour, pour une plage horaire qui va jusqu'à minuit). Sérialisé en `"08:00"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TimeOfDay(u16);

impl TimeOfDay {
    /// 00:00.
    pub const MIDNIGHT: Self = Self(0);
    /// 24:00.
    pub const END_OF_DAY: Self = Self(1_440);

    /// Heure en minutes depuis minuit.
    ///
    /// # Errors
    ///
    /// [`TimeError::InvalidTimeOfDay`] au-delà de 1 440.
    pub const fn new(minutes: u16) -> Result<Self, TimeError> {
        if minutes > 1_440 {
            return Err(TimeError::InvalidTimeOfDay { minutes });
        }
        Ok(Self(minutes))
    }

    /// Heure depuis heures et minutes (`hm(8, 30)` = 08:30, `hm(24, 0)` = 24:00).
    ///
    /// # Errors
    ///
    /// [`TimeError::InvalidTimeOfDay`] si `m >= 60` ou si l'heure dépasse 24:00.
    pub fn hm(h: u16, m: u16) -> Result<Self, TimeError> {
        let total = u32::from(h).saturating_mul(60).saturating_add(u32::from(m));
        let minutes = u16::try_from(total).unwrap_or(u16::MAX);
        if m >= 60 {
            return Err(TimeError::InvalidTimeOfDay { minutes });
        }
        Self::new(minutes)
    }

    /// Minutes depuis minuit.
    #[must_use]
    pub const fn minutes(self) -> u16 {
        self.0
    }
}

impl fmt::Display for TimeOfDay {
    /// `08:00`, `24:00`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let h = self.0.checked_div(60).unwrap_or(0);
        let m = self.0.checked_rem(60).unwrap_or(0);
        write!(f, "{h:02}:{m:02}")
    }
}

impl FromStr for TimeOfDay {
    type Err = TimeError;
    /// `HH:MM` strict, de `00:00` à `24:00`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || TimeError::InvalidFormat { input: s.to_owned() };
        match s.as_bytes() {
            [h1, h2, b':', m1, m2] => {
                let h = parse_digits(&[*h1, *h2]).and_then(|v| u16::try_from(v).ok());
                let m = parse_digits(&[*m1, *m2]).and_then(|v| u16::try_from(v).ok());
                match (h, m) {
                    (Some(h), Some(m)) => Self::hm(h, m).map_err(|_| invalid()),
                    _ => Err(invalid()),
                }
            }
            _ => Err(invalid()),
        }
    }
}

impl TryFrom<String> for TimeOfDay {
    type Error = TimeError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<TimeOfDay> for String {
    fn from(t: TimeOfDay) -> Self {
        t.to_string()
    }
}

/// Jour de semaine. L'ordre (`Ord`) commence au dimanche.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Weekday {
    /// Dimanche.
    Sunday,
    /// Lundi.
    Monday,
    /// Mardi.
    Tuesday,
    /// Mercredi.
    Wednesday,
    /// Jeudi.
    Thursday,
    /// Vendredi.
    Friday,
    /// Samedi.
    Saturday,
}

impl Weekday {
    /// Les sept jours, dimanche d'abord.
    pub const ALL: [Self; 7] = [
        Self::Sunday,
        Self::Monday,
        Self::Tuesday,
        Self::Wednesday,
        Self::Thursday,
        Self::Friday,
        Self::Saturday,
    ];

    /// Jour depuis un rang 0..=6 compté depuis le dimanche (rang réduit
    /// modulo 7 par l'appelant ; toute autre valeur donne samedi).
    const fn from_sunday_index(i: i32) -> Self {
        match i {
            0 => Self::Sunday,
            1 => Self::Monday,
            2 => Self::Tuesday,
            3 => Self::Wednesday,
            4 => Self::Thursday,
            5 => Self::Friday,
            _ => Self::Saturday,
        }
    }
}

/// Une valeur par jour de semaine, en champs nommés : ni indexation, ni
/// confusion dimanche = 0 / lundi = 0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Week<T> {
    /// Dimanche.
    pub sunday: T,
    /// Lundi.
    pub monday: T,
    /// Mardi.
    pub tuesday: T,
    /// Mercredi.
    pub wednesday: T,
    /// Jeudi.
    pub thursday: T,
    /// Vendredi.
    pub friday: T,
    /// Samedi.
    pub saturday: T,
}

impl<T> Week<T> {
    /// Valeur du jour `d`.
    pub const fn get(&self, d: Weekday) -> &T {
        match d {
            Weekday::Sunday => &self.sunday,
            Weekday::Monday => &self.monday,
            Weekday::Tuesday => &self.tuesday,
            Weekday::Wednesday => &self.wednesday,
            Weekday::Thursday => &self.thursday,
            Weekday::Friday => &self.friday,
            Weekday::Saturday => &self.saturday,
        }
    }

    /// Valeur modifiable du jour `d`.
    pub const fn get_mut(&mut self, d: Weekday) -> &mut T {
        match d {
            Weekday::Sunday => &mut self.sunday,
            Weekday::Monday => &mut self.monday,
            Weekday::Tuesday => &mut self.tuesday,
            Weekday::Wednesday => &mut self.wednesday,
            Weekday::Thursday => &mut self.thursday,
            Weekday::Friday => &mut self.friday,
            Weekday::Saturday => &mut self.saturday,
        }
    }

    /// Semaine construite jour par jour, dimanche d'abord.
    pub fn from_fn(mut f: impl FnMut(Weekday) -> T) -> Self {
        Self {
            sunday: f(Weekday::Sunday),
            monday: f(Weekday::Monday),
            tuesday: f(Weekday::Tuesday),
            wednesday: f(Weekday::Wednesday),
            thursday: f(Weekday::Thursday),
            friday: f(Weekday::Friday),
            saturday: f(Weekday::Saturday),
        }
    }

    /// Applique `f` à chaque jour, dimanche d'abord.
    pub fn map<U>(&self, mut f: impl FnMut(&T) -> U) -> Week<U> {
        Week {
            sunday: f(&self.sunday),
            monday: f(&self.monday),
            tuesday: f(&self.tuesday),
            wednesday: f(&self.wednesday),
            thursday: f(&self.thursday),
            friday: f(&self.friday),
            saturday: f(&self.saturday),
        }
    }
}

/// Intervalle de jours, bornes incluses, `first <= last`.
/// Sérialisé en `{"from":"2026-12-24","to":"2026-12-31"}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "DayRangeRepr", into = "DayRangeRepr")]
pub struct DayRange {
    first: Day,
    last: Day,
}

/// Forme sérialisée de [`DayRange`].
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DayRangeRepr {
    from: Day,
    to: Day,
}

impl TryFrom<DayRangeRepr> for DayRange {
    type Error = TimeError;
    fn try_from(r: DayRangeRepr) -> Result<Self, Self::Error> {
        Self::new(r.from, r.to)
    }
}

impl From<DayRange> for DayRangeRepr {
    fn from(r: DayRange) -> Self {
        Self {
            from: r.first,
            to: r.last,
        }
    }
}

impl DayRange {
    /// Intervalle `[first, last]`.
    ///
    /// # Errors
    ///
    /// [`TimeError::InvalidDayRange`] si `first > last`.
    pub fn new(first: Day, last: Day) -> Result<Self, TimeError> {
        if first > last {
            return Err(TimeError::InvalidDayRange { first, last });
        }
        Ok(Self { first, last })
    }

    /// Un seul jour.
    #[must_use]
    pub const fn single(d: Day) -> Self {
        Self { first: d, last: d }
    }

    /// Premier jour (inclus).
    #[must_use]
    pub const fn first(self) -> Day {
        self.first
    }

    /// Dernier jour (inclus).
    #[must_use]
    pub const fn last(self) -> Day {
        self.last
    }

    /// Vrai si `d` est dans l'intervalle.
    #[must_use]
    pub fn contains(self, d: Day) -> bool {
        self.first <= d && d <= self.last
    }

    /// Vrai si les deux intervalles ont au moins un jour commun.
    #[must_use]
    pub fn overlaps(self, other: Self) -> bool {
        self.first <= other.last && other.first <= self.last
    }
}

/// Mois de l'année.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Month {
    /// Janvier.
    January,
    /// Février.
    February,
    /// Mars.
    March,
    /// Avril.
    April,
    /// Mai.
    May,
    /// Juin.
    June,
    /// Juillet.
    July,
    /// Août.
    August,
    /// Septembre.
    September,
    /// Octobre.
    October,
    /// Novembre.
    November,
    /// Décembre.
    December,
}

impl Month {
    /// Les douze mois, janvier d'abord.
    pub const ALL: [Self; 12] = [
        Self::January,
        Self::February,
        Self::March,
        Self::April,
        Self::May,
        Self::June,
        Self::July,
        Self::August,
        Self::September,
        Self::October,
        Self::November,
        Self::December,
    ];

    /// Numéro civil du mois, 1 (janvier) à 12, convention de chrono. Ce n'est
    /// pas l'encodage MSPDI (0 = janvier), qui vit dans `plannercraft-mspdi`.
    #[must_use]
    pub const fn number(self) -> u32 {
        match self {
            Self::January => 1,
            Self::February => 2,
            Self::March => 3,
            Self::April => 4,
            Self::May => 5,
            Self::June => 6,
            Self::July => 7,
            Self::August => 8,
            Self::September => 9,
            Self::October => 10,
            Self::November => 11,
            Self::December => 12,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::arithmetic_side_effects,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime};

    use super::{Day, DayRange, EPOCH, Moment, Month, TimeOfDay, Week, Weekday};
    use crate::duration::Minutes;
    use crate::error::TimeError;

    fn chrono_weekday(w: chrono::Weekday) -> Weekday {
        match w {
            chrono::Weekday::Sun => Weekday::Sunday,
            chrono::Weekday::Mon => Weekday::Monday,
            chrono::Weekday::Tue => Weekday::Tuesday,
            chrono::Weekday::Wed => Weekday::Wednesday,
            chrono::Weekday::Thu => Weekday::Thursday,
            chrono::Weekday::Fri => Weekday::Friday,
            chrono::Weekday::Sat => Weekday::Saturday,
        }
    }

    /// Les 60 631 jours du domaine, comparés à chrono pas à pas : date,
    /// aller-retour, jour de semaine, instant de minuit et de fin de journée.
    #[test]
    fn exhaustive_days_match_chrono() {
        let mut expected = EPOCH;
        for i in 0..60_631 {
            let day = Day::new(i).unwrap();
            assert_eq!(day.to_naive(), expected);
            assert_eq!(Day::from_naive(expected).unwrap(), day);
            assert_eq!(day.weekday(), chrono_weekday(expected.weekday()));
            let midnight = Moment::at(day, TimeOfDay::MIDNIGHT).unwrap();
            assert_eq!(midnight.minutes(), i64::from(i) * 1440);
            assert_eq!(midnight.to_naive(), NaiveDateTime::new(expected, NaiveTime::MIN));
            assert_eq!(Moment::from_naive(midnight.to_naive()).unwrap(), midnight);
            assert_eq!(midnight.day(), day);
            assert_eq!(midnight.time_of_day(), TimeOfDay::MIDNIGHT);
            assert_eq!(midnight.weekday(), day.weekday());
            let last_minute = Moment::from_minutes(midnight.minutes() + 1439).unwrap();
            assert_eq!(last_minute.day(), day);
            assert_eq!(last_minute.time_of_day().minutes(), 1439);
            assert_eq!(day.to_string().parse::<Day>().unwrap(), day);
            expected = expected.succ_opt().unwrap();
        }
        assert_eq!(expected, NaiveDate::from_ymd_opt(2150, 1, 1).unwrap());
        assert_eq!(Day::new(60_631), Err(TimeError::OutOfDomain));
        assert_eq!(Day::new(-1), Err(TimeError::OutOfDomain));
    }

    #[test]
    fn moment_bounds() {
        assert_eq!(Moment::MAX.minutes(), 87_308_640);
        assert_eq!(Moment::MAX.day(), Day::MAX);
        assert_eq!(Moment::MAX.time_of_day(), TimeOfDay::END_OF_DAY);
        assert_eq!(Moment::MAX.to_string(), "2150-01-01T00:00");
        assert_eq!(Moment::at(Day::MAX, TimeOfDay::END_OF_DAY).unwrap(), Moment::MAX);
        assert_eq!(Moment::from_minutes(-1), Err(TimeError::OutOfDomain));
        assert_eq!(Moment::from_minutes(87_308_641), Err(TimeError::OutOfDomain));
        assert_eq!(
            Moment::MIN.checked_add(Minutes::new(-1)),
            Err(TimeError::OutOfDomain)
        );
        assert_eq!(
            Moment::MAX.checked_add(Minutes::new(i64::MAX)),
            Err(TimeError::Overflow)
        );
        assert_eq!(Moment::MIN.minutes_until(Moment::MAX), Minutes::new(87_308_640));
        assert_eq!(Moment::MAX.minutes_until(Moment::MIN), Minutes::new(-87_308_640));
        let far = NaiveDate::from_ymd_opt(200_000, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        assert_eq!(Moment::from_naive(far), Err(TimeError::OutOfDomain));
    }

    #[test]
    fn moment_at_end_of_day_is_next_midnight() {
        let day: Day = "2026-10-19".parse().unwrap();
        let next: Moment = "2026-10-20T00:00".parse().unwrap();
        assert_eq!(Moment::at(day, TimeOfDay::END_OF_DAY).unwrap(), next);
    }

    #[test]
    fn moment_text_format() {
        let m: Moment = "2026-10-19T08:00".parse().unwrap();
        assert_eq!(m.minutes(), 22_510_560);
        assert_eq!(m.to_string(), "2026-10-19T08:00");
        assert_eq!("2026-10-19T08:00:00".parse::<Moment>().unwrap(), m);
        for bad in [
            "2026-10-19T08:00:30",
            "2026-10-19T08:00:00.000",
            "2026-10-19 08:00",
            "2026-10-19T8:00",
            "2026-10-19T24:00",
            "2026-10-19T08:60",
            "2026-02-30T08:00",
            "+2026-10-19T08:00",
            "2026-10-19T08:00Z",
            "2026-10-19",
            "",
            "２026-10-19T08:00",
        ] {
            assert!(bad.parse::<Moment>().is_err(), "{bad}");
        }
        assert_eq!("1983-12-31T23:59".parse::<Moment>(), Err(TimeError::OutOfDomain));
        assert_eq!(
            "x".parse::<Moment>(),
            Err(TimeError::InvalidFormat {
                input: "x".to_owned()
            })
        );
    }

    #[test]
    fn time_of_day_all_values() {
        for m in 0..=1440_u16 {
            let t = TimeOfDay::new(m).unwrap();
            assert_eq!(t.minutes(), m);
            assert_eq!(t.to_string().parse::<TimeOfDay>().unwrap(), t);
            assert_eq!(TimeOfDay::hm(m / 60, m % 60).unwrap(), t);
        }
        assert_eq!(
            TimeOfDay::new(1441),
            Err(TimeError::InvalidTimeOfDay { minutes: 1441 })
        );
        assert!(TimeOfDay::hm(24, 1).is_err());
        assert!(TimeOfDay::hm(8, 60).is_err());
        assert!(TimeOfDay::hm(u16::MAX, 0).is_err());
        assert_eq!(TimeOfDay::END_OF_DAY.to_string(), "24:00");
        for bad in ["8:00", "08:0", "24:01", "25:00", "08h00", "-1:00"] {
            assert!(bad.parse::<TimeOfDay>().is_err(), "{bad}");
        }
    }

    #[test]
    fn week_get_each_day() {
        let week = Week::from_fn(|d| d);
        for d in Weekday::ALL {
            assert_eq!(*week.get(d), d);
        }
        let mut counts = week.map(|_| 0_u8);
        *counts.get_mut(Weekday::Friday) = 4;
        assert_eq!(counts.friday, 4);
        assert_eq!(counts.monday, 0);
        let json = serde_json::to_string(&counts).unwrap();
        assert!(serde_json::from_str::<Week<u8>>(&json).is_ok());
        assert!(
            serde_json::from_str::<Week<u8>>(
                r#"{"sunday":0,"monday":0,"tuesday":0,"wednesday":0,"thursday":0,"friday":0,"saturday":0,"funday":1}"#
            )
            .is_err()
        );
    }

    #[test]
    fn day_range() {
        let a: Day = "2026-12-24".parse().unwrap();
        let b: Day = "2026-12-31".parse().unwrap();
        let r = DayRange::new(a, b).unwrap();
        assert!(r.contains(a) && r.contains(b));
        assert!(!r.contains(b.checked_add_days(1).unwrap()));
        assert_eq!(
            DayRange::new(b, a),
            Err(TimeError::InvalidDayRange { first: b, last: a })
        );
        assert_eq!(DayRange::single(a).first(), DayRange::single(a).last());
        assert!(r.overlaps(DayRange::single(b)));
        assert!(!r.overlaps(DayRange::single(a.checked_add_days(-1).unwrap())));
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(json, r#"{"from":"2026-12-24","to":"2026-12-31"}"#);
        assert_eq!(serde_json::from_str::<DayRange>(&json).unwrap(), r);
        assert!(serde_json::from_str::<DayRange>(r#"{"from":"2026-12-31","to":"2026-12-24"}"#).is_err());
        assert_eq!(Day::MAX.checked_add_days(1), Err(TimeError::OutOfDomain));
        assert_eq!(Day::MAX.checked_add_days(i32::MAX), Err(TimeError::Overflow));
    }

    #[test]
    fn serde_strings() {
        let m: Moment = serde_json::from_str(r#""2026-10-19T08:00:00""#).unwrap();
        assert_eq!(serde_json::to_string(&m).unwrap(), r#""2026-10-19T08:00""#);
        assert!(serde_json::from_str::<Moment>("22510560").is_err());
        assert!(serde_json::from_str::<TimeOfDay>(r#""24:30""#).is_err());
        assert_eq!(
            serde_json::to_string(&TimeOfDay::END_OF_DAY).unwrap(),
            r#""24:00""#
        );
        assert_eq!(serde_json::to_string(&Weekday::Monday).unwrap(), r#""monday""#);
        assert_eq!(serde_json::to_string(&Month::December).unwrap(), r#""december""#);
    }

    #[test]
    fn month_numbers() {
        for (i, m) in Month::ALL.iter().enumerate() {
            assert_eq!(m.number(), u32::try_from(i).unwrap() + 1);
        }
    }
}
