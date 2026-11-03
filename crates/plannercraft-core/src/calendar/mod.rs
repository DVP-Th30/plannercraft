//! Calendriers (ADR-0008, § 6).
//!
//! État :
//! - [`model`] : modèle stocké (plages, jours, base ou dérivé, exceptions,
//!   récurrences, semaines datées), validé ;
//! - [`builtin`] : Standard, 24 heures, Équipe de nuit ;
//! - [`recurrence`] : dépliage des récurrences (R-018) ;
//! - [`resolve`] : chaîne d'héritage ([`resolve::chain`], R-015) et
//!   résolution d'un jour ([`resolve::resolve_day`], R-016, R-017, R-019),
//!   chemin froid ;
//! - [`compiled`] : forme compilée (fonction cumulée W), conversions ouvrées
//!   (R-020 à R-029) et [`CalendarSet`] ;
//! - `combine` : intersection et union de calendriers compilés
//!   ([`CompiledCalendar::intersect`], [`CompiledCalendar::union`], R-172) ;
//! - [`scale`] : échelle de temps ([`TimeScale`]) et placement d'une durée
//!   ou d'un jalon (R-030, R-031), seul point d'entrée du CPM.

pub mod builtin;
mod combine;
pub mod compiled;
pub mod model;
pub mod recurrence;
pub mod resolve;
pub mod scale;

use serde::{Deserialize, Serialize};

/// Sens d'un dépassement du domaine des dates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Avant le 1er janvier 1984.
    Before,
    /// Après le 31 décembre 2149.
    After,
}

impl std::fmt::Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Before => "avant le 1er janvier 1984",
            Self::After => "après le 31 décembre 2149",
        })
    }
}

pub use builtin::{BuiltinCalendarNames, night_shift, standard, twenty_four_hours};
pub use compiled::{CalendarSet, CompiledCalendar};
pub use model::{
    Calendar, CalendarException, CalendarKind, DayHours, DayRule, DaySetting, MonthItem, MonthPosition,
    Recurrence, TimeRange, WorkWeek,
};
pub use recurrence::expand_exception;
pub use resolve::{CalendarChain, DaySource, ResolvedDay, chain, resolve_day};
pub use scale::{Span, TimeScale};
