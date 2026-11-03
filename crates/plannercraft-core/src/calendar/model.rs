//! Modèle stocké d'un calendrier (ADR-0008, § 6.1).
//!
//! Ce module décrit et valide la forme stockée ; il ne calcule rien. La
//! résolution d'un jour, le dépliage des récurrences, la forme compilée et les
//! conversions ouvrées vivent dans les autres modules de [`crate::calendar`].

use serde::{Deserialize, Serialize};

use crate::arena::HasUid;
use crate::calendar::recurrence::{expand_exception, runs_overlap};
use crate::duration::Minutes;
use crate::error::CalendarModelError;
use crate::ids::{CalendarUid, Guid};
use crate::model::Extras;
use crate::time::{DayRange, Month, TimeOfDay, Week, Weekday};

/// Plage horaire `[from, to)` à la minute, `from < to <= 24:00`.
/// Sérialisée en `{"from":"08:00","to":"12:00"}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "TimeRangeRepr")]
pub struct TimeRange {
    from: TimeOfDay,
    to: TimeOfDay,
}

/// Forme sérialisée de [`TimeRange`], validée à la lecture.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TimeRangeRepr {
    from: TimeOfDay,
    to: TimeOfDay,
}

impl TryFrom<TimeRangeRepr> for TimeRange {
    type Error = CalendarModelError;
    fn try_from(r: TimeRangeRepr) -> Result<Self, Self::Error> {
        Self::new(r.from, r.to)
    }
}

impl TimeRange {
    /// Plage `[from, to)`.
    ///
    /// # Errors
    ///
    /// [`CalendarModelError::EmptyRange`] si `from >= to`.
    pub fn new(from: TimeOfDay, to: TimeOfDay) -> Result<Self, CalendarModelError> {
        if from >= to {
            return Err(CalendarModelError::EmptyRange);
        }
        Ok(Self { from, to })
    }

    /// Début (inclus).
    #[must_use]
    pub const fn from(self) -> TimeOfDay {
        self.from
    }

    /// Fin (exclue).
    #[must_use]
    pub const fn to(self) -> TimeOfDay {
        self.to
    }

    /// Durée de la plage en minutes (> 0).
    #[must_use]
    pub fn len(self) -> Minutes {
        // `to > from`, les deux dans 0..=1 440 : jamais de débordement.
        Minutes::new(i64::from(self.to.minutes().saturating_sub(self.from.minutes())))
    }
}

/// Plages d'un jour ouvré : 1 à 5, triées par début, disjointes ; deux plages
/// contiguës sont admises (fusionnées à la compilation). Sérialisées en
/// tableau.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<TimeRange>", into = "Vec<TimeRange>")]
pub struct DayHours(Vec<TimeRange>);

impl DayHours {
    /// Nombre maximal de plages par jour (S1).
    pub const MAX_RANGES: usize = 5;

    /// Trie les plages et les valide.
    ///
    /// # Errors
    ///
    /// [`CalendarModelError::NoRanges`] (aucune plage),
    /// [`CalendarModelError::TooManyRanges`] (plus de 5),
    /// [`CalendarModelError::OverlappingRanges`] (chevauchement).
    pub fn new(mut ranges: Vec<TimeRange>) -> Result<Self, CalendarModelError> {
        if ranges.is_empty() {
            return Err(CalendarModelError::NoRanges);
        }
        if ranges.len() > Self::MAX_RANGES {
            return Err(CalendarModelError::TooManyRanges { count: ranges.len() });
        }
        ranges.sort_unstable();
        if ranges.windows(2).any(|w| matches!(w, [a, b] if a.to > b.from)) {
            return Err(CalendarModelError::OverlappingRanges);
        }
        Ok(Self(ranges))
    }

    /// Plages, triées.
    #[must_use]
    pub fn ranges(&self) -> &[TimeRange] {
        &self.0
    }

    /// Minutes ouvrées du jour (au plus 1 440).
    #[must_use]
    pub fn total(&self) -> Minutes {
        Minutes::new(self.0.iter().map(|r| r.len().get()).sum())
    }
}

impl TryFrom<Vec<TimeRange>> for DayHours {
    type Error = CalendarModelError;
    fn try_from(ranges: Vec<TimeRange>) -> Result<Self, Self::Error> {
        Self::new(ranges)
    }
}

impl From<DayHours> for Vec<TimeRange> {
    fn from(h: DayHours) -> Self {
        h.0
    }
}

/// Réglage complet d'un jour : chômé ou ouvré avec ses plages.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DaySetting {
    /// Jour chômé.
    NonWorking,
    /// Jour ouvré.
    Working(DayHours),
}

impl DaySetting {
    /// Minutes ouvrées du jour.
    #[must_use]
    pub fn total(&self) -> Minutes {
        match self {
            Self::NonWorking => Minutes::ZERO,
            Self::Working(h) => h.total(),
        }
    }
}

/// Règle d'un jour d'un calendrier dérivé ou d'une semaine datée : hérité
/// (passe au niveau suivant), chômé, ou ouvré avec ses plages.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DayRule {
    /// Pas de réglage à ce niveau : le niveau suivant décide.
    Inherit,
    /// Jour chômé.
    NonWorking,
    /// Jour ouvré.
    Working(DayHours),
}

impl From<DaySetting> for DayRule {
    fn from(s: DaySetting) -> Self {
        match s {
            DaySetting::NonWorking => Self::NonWorking,
            DaySetting::Working(h) => Self::Working(h),
        }
    }
}

/// Nature d'un calendrier : de base (semaine complète) ou dérivé (une base et
/// ses seules différences). Héritage à un niveau, comme MS Project.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum CalendarKind {
    /// Calendrier de base : semaine complète par construction.
    Base {
        /// Semaine par défaut.
        week: Week<DaySetting>,
    },
    /// Calendrier dérivé (calendrier de ressource, par exemple).
    Derived {
        /// Calendrier de base dont il hérite.
        base: CalendarUid,
        /// Semaine par défaut, jour par jour (`Inherit` = jour de la base).
        week: Week<DayRule>,
    },
}

/// Semaine de travail datée (période nommée, S13). Sur sa période, elle
/// remplace la semaine par défaut du même calendrier pour les jours qu'elle
/// définit ; `Inherit` renvoie à la semaine par défaut.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkWeek {
    /// Nom.
    pub name: String,
    /// Période, jours inclus.
    pub period: DayRange,
    /// Règle de chaque jour de semaine.
    pub days: Week<DayRule>,
}

/// Exception datée : remplace la journée entière sur les jours qu'elle couvre.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarException {
    /// Nom (« Noël »).
    pub name: String,
    /// Période, jours entiers inclus ; une récurrence y est restreinte.
    pub period: DayRange,
    /// Réglage des jours couverts.
    pub setting: DaySetting,
    /// Récurrence des jours couverts dans la période.
    pub recurrence: Recurrence,
    /// Si présent : nombre maximal d'occurrences, dans l'ordre chronologique.
    pub occurrences: Option<u32>,
}

/// Rang d'un jour dans le mois.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonthPosition {
    /// Premier.
    First,
    /// Deuxième.
    Second,
    /// Troisième.
    Third,
    /// Quatrième.
    Fourth,
    /// Dernier.
    Last,
}

/// Jour visé par une récurrence par rang.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonthItem {
    /// Jour calendaire.
    Day,
    /// Jour ouvré (lundi à vendredi).
    Weekday,
    /// Jour de week-end (samedi ou dimanche).
    WeekendDay,
    /// Jour de semaine donné.
    On(Weekday),
}

/// Récurrence d'une exception (R-018, dépliage dans `calendar/recurrence.rs`).
///
/// Sérialisée avec une étiquette `type` (`{"type":"daily","every":2}`) et
/// validée à la lecture : `every >= 1`, `day` dans 1..=31.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", try_from = "RecurrenceRepr")]
pub enum Recurrence {
    /// Chaque jour de la période.
    Period,
    /// Un jour sur `every`, depuis le premier jour de la période.
    Daily {
        /// Pas, en jours (>= 1).
        every: u16,
    },
    /// Les jours cochés, une semaine sur `every`.
    Weekly {
        /// Pas, en semaines (>= 1).
        every: u16,
        /// Jours cochés.
        days: Week<bool>,
    },
    /// Le jour `day` du mois, un mois sur `every`.
    MonthlyByDay {
        /// Pas, en mois (>= 1).
        every: u16,
        /// Jour du mois, 1 à 31.
        day: u8,
    },
    /// Le n-ième `item` du mois, un mois sur `every`.
    MonthlyByPosition {
        /// Pas, en mois (>= 1).
        every: u16,
        /// Rang.
        position: MonthPosition,
        /// Jour visé.
        item: MonthItem,
    },
    /// Le jour `day` du mois `month`, chaque année.
    YearlyByDay {
        /// Mois.
        month: Month,
        /// Jour du mois, 1 à 31.
        day: u8,
    },
    /// Le n-ième `item` du mois `month`, chaque année.
    YearlyByPosition {
        /// Mois.
        month: Month,
        /// Rang.
        position: MonthPosition,
        /// Jour visé.
        item: MonthItem,
    },
    /// Sémantique non documentée : conservée, ignorée avec un diagnostic.
    ByDayCount {
        /// Pas.
        every: u16,
    },
    /// Sémantique non documentée : conservée, ignorée avec un diagnostic.
    ByWeekdayCount {
        /// Pas.
        every: u16,
    },
}

/// Miroir de [`Recurrence`] pour la lecture, avant validation.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum RecurrenceRepr {
    Period,
    Daily {
        every: u16,
    },
    Weekly {
        every: u16,
        days: Week<bool>,
    },
    MonthlyByDay {
        every: u16,
        day: u8,
    },
    MonthlyByPosition {
        every: u16,
        position: MonthPosition,
        item: MonthItem,
    },
    YearlyByDay {
        month: Month,
        day: u8,
    },
    YearlyByPosition {
        month: Month,
        position: MonthPosition,
        item: MonthItem,
    },
    ByDayCount {
        every: u16,
    },
    ByWeekdayCount {
        every: u16,
    },
}

impl TryFrom<RecurrenceRepr> for Recurrence {
    type Error = CalendarModelError;
    fn try_from(r: RecurrenceRepr) -> Result<Self, Self::Error> {
        let rec = match r {
            RecurrenceRepr::Period => Self::Period,
            RecurrenceRepr::Daily { every } => Self::Daily { every },
            RecurrenceRepr::Weekly { every, days } => Self::Weekly { every, days },
            RecurrenceRepr::MonthlyByDay { every, day } => Self::MonthlyByDay { every, day },
            RecurrenceRepr::MonthlyByPosition {
                every,
                position,
                item,
            } => Self::MonthlyByPosition {
                every,
                position,
                item,
            },
            RecurrenceRepr::YearlyByDay { month, day } => Self::YearlyByDay { month, day },
            RecurrenceRepr::YearlyByPosition {
                month,
                position,
                item,
            } => Self::YearlyByPosition {
                month,
                position,
                item,
            },
            RecurrenceRepr::ByDayCount { every } => Self::ByDayCount { every },
            RecurrenceRepr::ByWeekdayCount { every } => Self::ByWeekdayCount { every },
        };
        rec.validate()?;
        Ok(rec)
    }
}

impl Recurrence {
    /// Vérifie `every >= 1` et `day` dans 1..=31.
    ///
    /// # Errors
    ///
    /// [`CalendarModelError::InvalidRecurrence`].
    pub const fn validate(&self) -> Result<(), CalendarModelError> {
        let valid = match *self {
            Self::Period | Self::YearlyByPosition { .. } => true,
            Self::Daily { every }
            | Self::Weekly { every, .. }
            | Self::MonthlyByPosition { every, .. }
            | Self::ByDayCount { every }
            | Self::ByWeekdayCount { every } => every >= 1,
            Self::MonthlyByDay { every, day } => every >= 1 && day >= 1 && day <= 31,
            Self::YearlyByDay { day, .. } => day >= 1 && day <= 31,
        };
        if valid {
            Ok(())
        } else {
            Err(CalendarModelError::InvalidRecurrence)
        }
    }

    /// Vrai pour les types de sémantique non documentée, conservés mais
    /// ignorés au calcul (diagnostic `CalendarRecurrenceUnsupported`).
    #[must_use]
    pub const fn is_unsupported(&self) -> bool {
        matches!(self, Self::ByDayCount { .. } | Self::ByWeekdayCount { .. })
    }
}

/// Calendrier stocké.
///
/// La désérialisation accepte des exceptions ou des semaines datées qui se
/// chevauchent (fichier importé) ; `Project::validate` les signale et la
/// compilation garde la première (R-017). Les commandes (`add_*`) les
/// refusent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Calendar {
    uid: CalendarUid,
    /// GUID conservé tel quel.
    pub guid: Option<Guid>,
    /// Nom.
    pub name: String,
    kind: CalendarKind,
    work_weeks: Vec<WorkWeek>,
    exceptions: Vec<CalendarException>,
    /// Données hors modèle.
    pub extras: Extras,
}

impl HasUid for Calendar {
    type Uid = CalendarUid;
    fn uid(&self) -> CalendarUid {
        self.uid
    }
}

impl Calendar {
    /// Calendrier de base, sans exception ni semaine datée.
    #[must_use]
    pub const fn new_base(uid: CalendarUid, name: String, week: Week<DaySetting>) -> Self {
        Self {
            uid,
            guid: None,
            name,
            kind: CalendarKind::Base { week },
            work_weeks: Vec::new(),
            exceptions: Vec::new(),
            extras: Extras::new(),
        }
    }

    /// Calendrier dérivé de `base`, tous les jours hérités.
    #[must_use]
    pub const fn new_derived(uid: CalendarUid, name: String, base: CalendarUid) -> Self {
        Self {
            uid,
            guid: None,
            name,
            kind: CalendarKind::Derived {
                base,
                week: Week {
                    sunday: DayRule::Inherit,
                    monday: DayRule::Inherit,
                    tuesday: DayRule::Inherit,
                    wednesday: DayRule::Inherit,
                    thursday: DayRule::Inherit,
                    friday: DayRule::Inherit,
                    saturday: DayRule::Inherit,
                },
            },
            work_weeks: Vec::new(),
            exceptions: Vec::new(),
            extras: Extras::new(),
        }
    }

    /// UID.
    #[must_use]
    pub const fn uid(&self) -> CalendarUid {
        self.uid
    }

    /// Nature (base ou dérivé) et semaine par défaut.
    #[must_use]
    pub const fn kind(&self) -> &CalendarKind {
        &self.kind
    }

    /// Vrai pour un calendrier de base.
    #[must_use]
    pub const fn is_base(&self) -> bool {
        matches!(self.kind, CalendarKind::Base { .. })
    }

    /// Exceptions, dans l'ordre de stockage.
    #[must_use]
    pub fn exceptions(&self) -> &[CalendarException] {
        &self.exceptions
    }

    /// Semaines datées, dans l'ordre de stockage.
    #[must_use]
    pub fn work_weeks(&self) -> &[WorkWeek] {
        &self.work_weeks
    }

    /// Règle d'un jour de la semaine par défaut.
    ///
    /// # Errors
    ///
    /// [`CalendarModelError::InheritOnBase`] : `Inherit` sur un calendrier de
    /// base.
    pub fn set_day(&mut self, d: Weekday, rule: DayRule) -> Result<(), CalendarModelError> {
        match &mut self.kind {
            CalendarKind::Base { week } => {
                *week.get_mut(d) = match rule {
                    DayRule::Inherit => return Err(CalendarModelError::InheritOnBase),
                    DayRule::NonWorking => DaySetting::NonWorking,
                    DayRule::Working(h) => DaySetting::Working(h),
                };
            }
            CalendarKind::Derived { week, .. } => *week.get_mut(d) = rule,
        }
        Ok(())
    }

    /// Retire l'exception n° `index` et la rend.
    ///
    /// # Errors
    ///
    /// [`CalendarModelError::IndexOutOfRange`].
    pub fn remove_exception(&mut self, index: usize) -> Result<CalendarException, CalendarModelError> {
        if index >= self.exceptions.len() {
            return Err(CalendarModelError::IndexOutOfRange { index });
        }
        Ok(self.exceptions.remove(index))
    }

    /// Ajoute une semaine datée et rend son indice.
    ///
    /// # Errors
    ///
    /// [`CalendarModelError::OverlappingWorkWeek`] si sa période chevauche
    /// celle d'une semaine datée existante (une semaine datée n'est pas
    /// récurrente : ses jours sont ceux de sa période).
    pub fn add_work_week(&mut self, w: WorkWeek) -> Result<usize, CalendarModelError> {
        if let Some(existing) = self.work_weeks.iter().position(|o| o.period.overlaps(w.period)) {
            return Err(CalendarModelError::OverlappingWorkWeek { existing });
        }
        self.work_weeks.push(w);
        Ok(self.work_weeks.len().saturating_sub(1))
    }

    /// Retire la semaine datée n° `index` et la rend.
    ///
    /// # Errors
    ///
    /// [`CalendarModelError::IndexOutOfRange`].
    pub fn remove_work_week(&mut self, index: usize) -> Result<WorkWeek, CalendarModelError> {
        if index >= self.work_weeks.len() {
            return Err(CalendarModelError::IndexOutOfRange { index });
        }
        Ok(self.work_weeks.remove(index))
    }

    /// Ajoute une exception et rend son indice.
    ///
    /// Le chevauchement se juge sur les jours dépliés des récurrences
    /// (R-017, R-018), pas sur les périodes brutes : deux exceptions
    /// hebdomadaires sur la même période, l'une le lundi et l'autre le
    /// vendredi, coexistent.
    ///
    /// # Errors
    ///
    /// [`CalendarModelError::InvalidRecurrence`] (récurrence invalide) ;
    /// [`CalendarModelError::OverlappingException`] si un jour déplié est déjà
    /// couvert par une exception existante.
    pub fn add_exception(&mut self, e: CalendarException) -> Result<usize, CalendarModelError> {
        let runs = expand_exception(&e)?;
        for (existing, other) in self.exceptions.iter().enumerate() {
            if other.period.overlaps(e.period) && runs_overlap(&runs, &expand_exception(other)?) {
                return Err(CalendarModelError::OverlappingException { existing });
            }
        }
        self.exceptions.push(e);
        Ok(self.exceptions.len().saturating_sub(1))
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::panic,
        clippy::indexing_slicing,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{
        Calendar, CalendarException, CalendarKind, DayHours, DayRule, DaySetting, MonthItem, MonthPosition,
        Recurrence, TimeRange, WorkWeek,
    };
    use crate::calendar::builtin::standard;
    use crate::duration::Minutes;
    use crate::error::CalendarModelError;
    use crate::ids::CalendarUid;
    use crate::time::{Day, DayRange, Month, TimeOfDay, Week, Weekday};

    fn t(h: u16, m: u16) -> TimeOfDay {
        TimeOfDay::hm(h, m).unwrap()
    }

    fn r(a: u16, b: u16) -> TimeRange {
        TimeRange::new(t(a, 0), t(b, 0)).unwrap()
    }

    fn day(s: &str) -> Day {
        s.parse().unwrap()
    }

    #[test]
    fn time_range() {
        assert_eq!(
            TimeRange::new(t(12, 0), t(8, 0)),
            Err(CalendarModelError::EmptyRange)
        );
        assert_eq!(
            TimeRange::new(t(8, 0), t(8, 0)),
            Err(CalendarModelError::EmptyRange)
        );
        let full = TimeRange::new(TimeOfDay::MIDNIGHT, TimeOfDay::END_OF_DAY).unwrap();
        assert_eq!(full.len(), Minutes::new(1_440));
        assert_eq!((r(8, 12).from(), r(8, 12).to()), (t(8, 0), t(12, 0)));
        let json = serde_json::to_string(&r(8, 12)).unwrap();
        assert_eq!(json, r#"{"from":"08:00","to":"12:00"}"#);
        assert!(serde_json::from_str::<TimeRange>(r#"{"from":"12:00","to":"08:00"}"#).is_err());
    }

    #[test]
    fn day_hours() {
        let h = DayHours::new(vec![r(13, 17), r(8, 12)]).unwrap();
        assert_eq!(h.ranges(), [r(8, 12), r(13, 17)]);
        assert_eq!(h.total(), Minutes::new(480));
        assert_eq!(
            DayHours::new(vec![r(8, 12), r(12, 17)]).unwrap().total(),
            Minutes::new(540)
        );
        assert_eq!(DayHours::new(vec![]), Err(CalendarModelError::NoRanges));
        assert_eq!(
            DayHours::new(vec![r(8, 12), TimeRange::new(t(11, 0), t(13, 0)).unwrap()]),
            Err(CalendarModelError::OverlappingRanges)
        );
        assert_eq!(
            DayHours::new(vec![r(0, 1), r(2, 3), r(4, 5), r(6, 7), r(8, 9), r(10, 11)]),
            Err(CalendarModelError::TooManyRanges { count: 6 })
        );
        assert!(DayHours::new(vec![r(0, 1), r(2, 3), r(4, 5), r(6, 7), r(8, 9)]).is_ok());
        assert!(serde_json::from_str::<DayHours>("[]").is_err());
        let json = serde_json::to_string(&DaySetting::Working(h)).unwrap();
        assert_eq!(
            json,
            r#"{"working":[{"from":"08:00","to":"12:00"},{"from":"13:00","to":"17:00"}]}"#
        );
        assert_eq!(serde_json::to_string(&DayRule::Inherit).unwrap(), r#""inherit""#);
        assert_eq!(DaySetting::NonWorking.total(), Minutes::ZERO);
    }

    #[test]
    fn recurrence_validation_and_json() {
        assert_eq!(
            Recurrence::Daily { every: 0 }.validate(),
            Err(CalendarModelError::InvalidRecurrence)
        );
        assert_eq!(
            Recurrence::MonthlyByDay { every: 1, day: 32 }.validate(),
            Err(CalendarModelError::InvalidRecurrence)
        );
        assert_eq!(
            Recurrence::YearlyByDay {
                month: Month::May,
                day: 0
            }
            .validate(),
            Err(CalendarModelError::InvalidRecurrence)
        );
        assert!(
            Recurrence::YearlyByDay {
                month: Month::February,
                day: 30
            }
            .validate()
            .is_ok()
        );
        assert!(Recurrence::ByDayCount { every: 2 }.is_unsupported());
        assert!(!Recurrence::Period.is_unsupported());
        let rec = Recurrence::MonthlyByPosition {
            every: 1,
            position: MonthPosition::Last,
            item: MonthItem::On(Weekday::Friday),
        };
        let json = serde_json::to_string(&rec).unwrap();
        assert_eq!(
            json,
            r#"{"type":"monthly_by_position","every":1,"position":"last","item":{"on":"friday"}}"#
        );
        assert_eq!(serde_json::from_str::<Recurrence>(&json).unwrap(), rec);
        assert_eq!(
            serde_json::to_string(&Recurrence::Period).unwrap(),
            r#"{"type":"period"}"#
        );
        assert!(serde_json::from_str::<Recurrence>(r#"{"type":"daily","every":0}"#).is_err());
        assert!(
            serde_json::from_str::<Recurrence>(r#"{"type":"monthly_by_day","every":1,"day":31}"#).is_ok()
        );
        assert!(
            serde_json::from_str::<Recurrence>(r#"{"type":"monthly_by_day","every":1,"day":32}"#).is_err()
        );
    }

    #[test]
    fn set_day_on_base_and_derived() {
        let week = Week::from_fn(|_| DaySetting::NonWorking);
        let mut base = Calendar::new_base(CalendarUid::new(1), "B".to_owned(), week);
        assert!(base.is_base());
        assert_eq!(
            base.set_day(Weekday::Monday, DayRule::Inherit),
            Err(CalendarModelError::InheritOnBase)
        );
        let h = DayHours::new(vec![r(8, 12)]).unwrap();
        base.set_day(Weekday::Monday, DayRule::Working(h.clone()))
            .unwrap();
        match base.kind() {
            CalendarKind::Base { week } => assert_eq!(week.monday, DaySetting::Working(h.clone())),
            CalendarKind::Derived { .. } => panic!("base attendu"),
        }
        let mut derived = Calendar::new_derived(CalendarUid::new(2), "D".to_owned(), CalendarUid::new(1));
        assert!(!derived.is_base());
        derived.set_day(Weekday::Friday, DayRule::NonWorking).unwrap();
        match derived.kind() {
            CalendarKind::Derived { base, week } => {
                assert_eq!(*base, CalendarUid::new(1));
                assert_eq!(week.friday, DayRule::NonWorking);
                assert_eq!(week.monday, DayRule::Inherit);
            }
            CalendarKind::Base { .. } => panic!("dérivé attendu"),
        }
    }

    #[test]
    fn work_weeks_and_exceptions() {
        let mut c = Calendar::new_derived(CalendarUid::new(2), "D".to_owned(), CalendarUid::new(1));
        let ww = |a: &str, b: &str| WorkWeek {
            name: "été".to_owned(),
            period: DayRange::new(day(a), day(b)).unwrap(),
            days: Week::from_fn(|_| DayRule::Inherit),
        };
        assert_eq!(c.add_work_week(ww("2027-07-01", "2027-08-31")), Ok(0));
        assert_eq!(
            c.add_work_week(ww("2027-08-31", "2027-09-30")),
            Err(CalendarModelError::OverlappingWorkWeek { existing: 0 })
        );
        assert_eq!(c.add_work_week(ww("2027-09-01", "2027-09-30")), Ok(1));
        assert_eq!(c.work_weeks().len(), 2);
        assert_eq!(
            c.remove_work_week(2),
            Err(CalendarModelError::IndexOutOfRange { index: 2 })
        );
        assert_eq!(c.remove_work_week(0).unwrap().period.first(), day("2027-07-01"));
        assert_eq!(
            c.remove_exception(0),
            Err(CalendarModelError::IndexOutOfRange { index: 0 })
        );

        // Commandes : chevauchement jugé sur les jours dépliés.
        let weekly = |d: Weekday| CalendarException {
            name: String::new(),
            period: DayRange::new(day("2026-11-01"), day("2026-11-30")).unwrap(),
            setting: DaySetting::NonWorking,
            recurrence: Recurrence::Weekly {
                every: 1,
                days: Week::from_fn(|x| x == d),
            },
            occurrences: None,
        };
        assert_eq!(c.add_exception(weekly(Weekday::Monday)), Ok(0));
        assert_eq!(c.add_exception(weekly(Weekday::Friday)), Ok(1));
        let mut armistice = weekly(Weekday::Monday);
        armistice.period = DayRange::single(day("2026-11-11"));
        armistice.recurrence = Recurrence::Period;
        assert_eq!(c.add_exception(armistice.clone()), Ok(2));
        // Lundi 16/11 : déjà couvert par l'exception n° 0.
        armistice.period = DayRange::single(day("2026-11-16"));
        assert_eq!(
            c.add_exception(armistice.clone()),
            Err(CalendarModelError::OverlappingException { existing: 0 })
        );
        armistice.recurrence = Recurrence::Daily { every: 0 };
        assert_eq!(
            c.add_exception(armistice),
            Err(CalendarModelError::InvalidRecurrence)
        );
        assert_eq!(c.remove_exception(1).unwrap(), weekly(Weekday::Friday));
        assert_eq!(c.exceptions().len(), 2);
        while !c.exceptions().is_empty() {
            c.remove_exception(0).unwrap();
        }

        // Des exceptions qui se chevauchent se lisent (fichier importé).
        let ex = CalendarException {
            name: "Armistice".to_owned(),
            period: DayRange::single(day("2026-11-11")),
            setting: DaySetting::NonWorking,
            recurrence: Recurrence::Period,
            occurrences: None,
        };
        let mut json = serde_json::to_value(&c).unwrap();
        json["exceptions"] = serde_json::to_value(vec![ex.clone(), ex.clone()]).unwrap();
        let mut read: Calendar = serde_json::from_value(json).unwrap();
        assert_eq!(read.exceptions().len(), 2);
        assert_eq!(read.remove_exception(1).unwrap(), ex);
        assert_eq!(read.exceptions().len(), 1);
    }

    #[test]
    fn calendar_golden_json() {
        let c = Calendar::new_derived(CalendarUid::new(4), "Paul".to_owned(), CalendarUid::new(1));
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(
            json,
            concat!(
                r#"{"uid":4,"guid":null,"name":"Paul","kind":{"derived":{"base":1,"week":{"#,
                r#""sunday":"inherit","monday":"inherit","tuesday":"inherit","wednesday":"inherit","#,
                r#""thursday":"inherit","friday":"inherit","saturday":"inherit"}}},"#,
                r#""work_weeks":[],"exceptions":[],"extras":[]}"#
            )
        );
        assert_eq!(serde_json::from_str::<Calendar>(&json).unwrap(), c);
        let mut bad = serde_json::to_value(&c).unwrap();
        bad["unknown"] = serde_json::Value::Bool(true);
        assert!(serde_json::from_value::<Calendar>(bad).is_err());
        // Champ inconnu dans une variante de `CalendarKind` : refusé aussi.
        let mut bad = serde_json::to_value(&c).unwrap();
        bad["kind"]["derived"]["bogus"] = serde_json::Value::from(1);
        assert!(serde_json::from_value::<Calendar>(bad).is_err());
        let base = standard(CalendarUid::new(1), "Standard".to_owned());
        let mut bad = serde_json::to_value(&base).unwrap();
        bad["kind"]["base"]["bogus"] = serde_json::Value::from(1);
        assert!(serde_json::from_value::<Calendar>(bad).is_err());
    }
}
