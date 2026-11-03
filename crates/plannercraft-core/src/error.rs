//! Erreurs du cœur : refus d'une opération (ADR-0008, § 7.1).
//!
//! Chaque type d'erreur :
//! - a un message français (`Display`, par `thiserror`) ;
//! - donne un code stable par `code()` : préfixe du module, suffixe de la
//!   variante en `snake_case` (`fixed.too_many_decimals`) ;
//! - se sérialise avec ce code dans le champ `code` (`#[serde(tag = "code")]`,
//!   chaque variante renommée par son code). Un test vérifie que les deux
//!   coïncident pour chaque variante.
//!
//! Les erreurs ne se désérialisent pas : elles sortent du cœur, elles n'y
//! entrent jamais.

use serde::Serialize;

use crate::calendar::Direction;
use crate::ids::CalendarUid;
use crate::time::Day;

/// Erreur du point fixe (`fixed.rs`, `quantity.rs`).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize)]
#[serde(tag = "code")]
pub enum FixedError {
    /// Plus de 6 décimales.
    #[serde(rename = "fixed.too_many_decimals")]
    #[error("trop de décimales (au plus 6)")]
    TooManyDecimals,
    /// Valeur hors des bornes du type.
    #[serde(rename = "fixed.out_of_range")]
    #[error("valeur {value} hors des bornes [{min} ; {max}]")]
    OutOfRange {
        /// Borne inférieure incluse.
        min: i64,
        /// Borne supérieure incluse.
        max: i64,
        /// Valeur refusée.
        value: i128,
    },
    /// Dépassement de capacité pendant un calcul.
    #[serde(rename = "fixed.overflow")]
    #[error("dépassement de capacité dans un calcul en point fixe")]
    Overflow,
}

impl FixedError {
    /// Code stable de l'erreur.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::TooManyDecimals => "fixed.too_many_decimals",
            Self::OutOfRange { .. } => "fixed.out_of_range",
            Self::Overflow => "fixed.overflow",
        }
    }
}

/// Erreur du temps (`time.rs`).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize)]
#[serde(tag = "code")]
pub enum TimeError {
    /// Instant ou jour hors du domaine 1984-01-01 00:00 à 2150-01-01 00:00.
    #[serde(rename = "time.out_of_domain")]
    #[error("date hors du domaine (du 1er janvier 1984 au 31 décembre 2149)")]
    OutOfDomain,
    /// Heure au-delà de 24:00, ou minutes >= 60.
    #[serde(rename = "time.invalid_time_of_day")]
    #[error("heure invalide ({minutes} minutes depuis minuit, au plus 24:00)")]
    InvalidTimeOfDay {
        /// Heure refusée, en minutes depuis minuit.
        minutes: u16,
    },
    /// Secondes non nulles dans une saisie stricte.
    #[serde(rename = "time.non_zero_seconds")]
    #[error("les secondes doivent être nulles (résolution à la minute)")]
    NonZeroSeconds,
    /// Texte qui n'a pas la forme attendue.
    #[serde(rename = "time.invalid_format")]
    #[error("format de date ou d'heure invalide : « {input} »")]
    InvalidFormat {
        /// Texte refusé.
        input: String,
    },
    /// Intervalle de jours dont le début suit la fin.
    #[serde(rename = "time.invalid_day_range")]
    #[error("intervalle de jours invalide : du {first} au {last}")]
    InvalidDayRange {
        /// Premier jour.
        first: Day,
        /// Dernier jour, antérieur au premier.
        last: Day,
    },
    /// Dépassement de capacité pendant un calcul.
    #[serde(rename = "time.overflow")]
    #[error("dépassement de capacité dans un calcul de date")]
    Overflow,
}

impl TimeError {
    /// Code stable de l'erreur.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::OutOfDomain => "time.out_of_domain",
            Self::InvalidTimeOfDay { .. } => "time.invalid_time_of_day",
            Self::NonZeroSeconds => "time.non_zero_seconds",
            Self::InvalidFormat { .. } => "time.invalid_format",
            Self::InvalidDayRange { .. } => "time.invalid_day_range",
            Self::Overflow => "time.overflow",
        }
    }
}

/// Erreur des durées, du travail et des décalages (`duration.rs`).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize)]
#[serde(tag = "code")]
pub enum DurationError {
    /// Durée ou travail négatif.
    #[serde(rename = "duration.negative")]
    #[error("une durée ou un travail ne peut pas être négatif")]
    Negative,
    /// Valeur au-delà des bornes.
    #[serde(rename = "duration.out_of_range")]
    #[error("durée, travail ou décalage hors des bornes")]
    OutOfRange,
    /// Dépassement de capacité pendant un calcul.
    #[serde(rename = "duration.overflow")]
    #[error("dépassement de capacité dans un calcul de durée")]
    Overflow,
    /// Facteurs de conversion hors des bornes.
    #[serde(rename = "duration.invalid_factors")]
    #[error(
        "facteurs de conversion invalides : {minutes_per_day} min/jour (1 à 1 440), \
         {minutes_per_week} min/semaine (1 à 10 080), {days_per_month} jours/mois (1 à 31)"
    )]
    InvalidFactors {
        /// Minutes par jour.
        minutes_per_day: u32,
        /// Minutes par semaine.
        minutes_per_week: u32,
        /// Jours par mois.
        days_per_month: u32,
    },
}

impl DurationError {
    /// Code stable de l'erreur.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Negative => "duration.negative",
            Self::OutOfRange => "duration.out_of_range",
            Self::Overflow => "duration.overflow",
            Self::InvalidFactors { .. } => "duration.invalid_factors",
        }
    }
}

/// Erreur du modèle stocké d'un calendrier (`calendar/model.rs`).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize)]
#[serde(tag = "code")]
pub enum CalendarModelError {
    /// Plage horaire vide ou inversée (`from >= to`).
    #[serde(rename = "calendar_model.empty_range")]
    #[error("plage horaire vide : le début doit précéder la fin")]
    EmptyRange,
    /// Jour ouvré sans aucune plage.
    #[serde(rename = "calendar_model.no_ranges")]
    #[error("un jour ouvré doit avoir au moins une plage horaire")]
    NoRanges,
    /// Plus de 5 plages dans un jour.
    #[serde(rename = "calendar_model.too_many_ranges")]
    #[error("{count} plages horaires dans un jour : au plus 5")]
    TooManyRanges {
        /// Nombre de plages refusé.
        count: usize,
    },
    /// Plages qui se chevauchent dans un jour.
    #[serde(rename = "calendar_model.overlapping_ranges")]
    #[error("plages horaires qui se chevauchent")]
    OverlappingRanges,
    /// Jour « hérité » sur un calendrier de base, qui n'a pas de parent.
    #[serde(rename = "calendar_model.inherit_on_base")]
    #[error("un calendrier de base n'hérite d'aucun calendrier : jour « hérité » impossible")]
    InheritOnBase,
    /// Récurrence invalide (pas nul, jour du mois hors de 1 à 31).
    #[serde(rename = "calendar_model.invalid_recurrence")]
    #[error("récurrence invalide : intervalle d'au moins 1, jour du mois de 1 à 31")]
    InvalidRecurrence,
    /// Exception qui chevauche une exception existante du même calendrier.
    #[serde(rename = "calendar_model.overlapping_exception")]
    #[error("l'exception chevauche l'exception n° {existing} du calendrier")]
    OverlappingException {
        /// Indice de l'exception existante.
        existing: usize,
    },
    /// Semaine datée qui chevauche une semaine datée existante.
    #[serde(rename = "calendar_model.overlapping_work_week")]
    #[error("la semaine de travail chevauche la semaine n° {existing} du calendrier")]
    OverlappingWorkWeek {
        /// Indice de la semaine existante.
        existing: usize,
    },
    /// Indice d'exception ou de semaine inexistant.
    #[serde(rename = "calendar_model.index_out_of_range")]
    #[error("aucun élément n° {index}")]
    IndexOutOfRange {
        /// Indice refusé.
        index: usize,
    },
}

impl CalendarModelError {
    /// Code stable de l'erreur.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::EmptyRange => "calendar_model.empty_range",
            Self::NoRanges => "calendar_model.no_ranges",
            Self::TooManyRanges { .. } => "calendar_model.too_many_ranges",
            Self::OverlappingRanges => "calendar_model.overlapping_ranges",
            Self::InheritOnBase => "calendar_model.inherit_on_base",
            Self::InvalidRecurrence => "calendar_model.invalid_recurrence",
            Self::OverlappingException { .. } => "calendar_model.overlapping_exception",
            Self::OverlappingWorkWeek { .. } => "calendar_model.overlapping_work_week",
            Self::IndexOutOfRange { .. } => "calendar_model.index_out_of_range",
        }
    }
}

/// Erreur de calendrier : héritage, conversion, invariant interne.
///
/// C'est l'erreur du chemin chaud (conversions ouvrées) : la cause (aucun
/// temps ouvré, sortie du domaine et sa direction) n'est jamais perdue.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize)]
#[serde(tag = "code")]
pub enum CalendarError {
    /// Calendrier inexistant.
    #[serde(rename = "calendar.unknown_calendar")]
    #[error("calendrier {calendar} inconnu")]
    UnknownCalendar {
        /// UID cherché.
        calendar: CalendarUid,
    },
    /// Calendrier dérivé dont la base n'existe pas.
    #[serde(rename = "calendar.base_missing")]
    #[error("le calendrier {calendar} hérite du calendrier {base}, qui n'existe pas")]
    BaseMissing {
        /// Calendrier dérivé.
        calendar: CalendarUid,
        /// Base introuvable.
        base: CalendarUid,
    },
    /// Calendrier dérivé dont la base est elle-même dérivée (un seul niveau
    /// d'héritage, comme MS Project).
    #[serde(rename = "calendar.base_not_base")]
    #[error("le calendrier {calendar} hérite du calendrier {base}, qui n'est pas un calendrier de base")]
    BaseNotBase {
        /// Calendrier dérivé.
        calendar: CalendarUid,
        /// Base qui n'en est pas une.
        base: CalendarUid,
    },
    /// Boucle d'héritage ; `chain` se termine par le premier UID répété.
    #[serde(rename = "calendar.inheritance_cycle")]
    #[error("boucle d'héritage entre calendriers : {}", format_chain(chain))]
    InheritanceCycle {
        /// Chemin complet, premier UID répété en fin.
        chain: Vec<CalendarUid>,
    },
    /// Calendrier sans aucune minute ouvrée.
    #[serde(rename = "calendar.no_working_time")]
    #[error("le calendrier {calendar} n'a aucun temps ouvré")]
    NoWorkingTime {
        /// Calendrier.
        calendar: CalendarUid,
    },
    /// Résultat hors du domaine des dates (1984-2149).
    #[serde(rename = "calendar.out_of_domain")]
    #[error("calcul sur le calendrier {calendar} hors du domaine des dates ({direction})")]
    OutOfDomain {
        /// Calendrier.
        calendar: CalendarUid,
        /// Sens du dépassement.
        direction: Direction,
    },
    /// Temps écoulé (sans calendrier) dont le résultat sort du domaine des
    /// dates.
    #[serde(rename = "calendar.elapsed_out_of_domain")]
    #[error("calcul en temps écoulé hors du domaine des dates ({direction})")]
    ElapsedOutOfDomain {
        /// Sens du dépassement.
        direction: Direction,
    },
    /// Placement d'une durée négative.
    #[serde(rename = "calendar.negative_span")]
    #[error("une durée à placer ne peut pas être négative")]
    NegativeSpan,
    /// Invariant interne violé : jamais attendu.
    #[serde(rename = "calendar.inconsistent")]
    #[error("incohérence interne du calendrier {calendar} (anomalie à signaler)")]
    Inconsistent {
        /// Calendrier.
        calendar: CalendarUid,
    },
    /// Intersection ou union de deux calendriers compilés trop morcelée :
    /// plafond de segments atteint (R-172, ADR-0013 D2).
    #[serde(rename = "calendar.too_many_segments")]
    #[error("calendrier combiné trop morcelé (au plus {limit} segments)")]
    TooManySegments {
        /// Plafond de segments.
        limit: u32,
    },
    /// Erreur du modèle stocké (le code est celui de l'erreur portée ;
    /// variante non étiquetée, donc placée en dernier pour serde).
    #[serde(untagged)]
    #[error(transparent)]
    Model(CalendarModelError),
}

/// `5 → 6 → 5`.
fn format_chain(chain: &[CalendarUid]) -> String {
    chain
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" → ")
}

impl CalendarError {
    /// Code stable de l'erreur (celui de l'erreur portée pour
    /// [`CalendarError::Model`]).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnknownCalendar { .. } => "calendar.unknown_calendar",
            Self::BaseMissing { .. } => "calendar.base_missing",
            Self::BaseNotBase { .. } => "calendar.base_not_base",
            Self::InheritanceCycle { .. } => "calendar.inheritance_cycle",
            Self::NoWorkingTime { .. } => "calendar.no_working_time",
            Self::OutOfDomain { .. } => "calendar.out_of_domain",
            Self::ElapsedOutOfDomain { .. } => "calendar.elapsed_out_of_domain",
            Self::NegativeSpan => "calendar.negative_span",
            Self::Inconsistent { .. } => "calendar.inconsistent",
            Self::TooManySegments { .. } => "calendar.too_many_segments",
            Self::Model(e) => e.code(),
        }
    }
}

impl From<CalendarModelError> for CalendarError {
    fn from(e: CalendarModelError) -> Self {
        Self::Model(e)
    }
}

/// Type d'entité identifiée par un UID (messages d'erreur).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    /// Tâche.
    Task,
    /// Ressource.
    Resource,
    /// Affectation.
    Assignment,
    /// Calendrier.
    Calendar,
}

impl std::fmt::Display for EntityKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Task => "tâche",
            Self::Resource => "ressource",
            Self::Assignment => "affectation",
            Self::Calendar => "calendrier",
        })
    }
}

/// Erreur du modèle (identifiants, collections, format).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize)]
#[serde(tag = "code")]
pub enum ModelError {
    /// Deux entités du même type portent le même UID.
    #[serde(rename = "model.duplicate_uid")]
    #[error("UID {uid} en double ({entity})")]
    DuplicateUid {
        /// Type d'entité.
        entity: EntityKind,
        /// UID en double.
        uid: u32,
    },
    /// UID réservé (UID 0 de tâche, réservé à la tâche récapitulative du projet).
    #[serde(rename = "model.reserved_uid")]
    #[error("l'UID de tâche {uid} est réservé à la tâche récapitulative du projet")]
    ReservedUid {
        /// UID refusé.
        uid: u32,
    },
    /// Position d'insertion au-delà de la fin.
    #[serde(rename = "model.position_out_of_range")]
    #[error("position {position} hors de la liste")]
    PositionOutOfRange {
        /// Position refusée.
        position: usize,
    },
    /// Version du format JSON inconnue.
    #[serde(rename = "model.unsupported_format_version")]
    #[error("version de format {version} non prise en charge (attendue : 1)")]
    UnsupportedFormatVersion {
        /// Version lue.
        version: u32,
    },
    /// GUID mal écrit.
    #[serde(rename = "model.invalid_guid")]
    #[error("GUID invalide : « {input} » (attendu : 8-4-4-4-12 chiffres hexadécimaux)")]
    InvalidGuid {
        /// Texte refusé.
        input: String,
    },
    /// Numéro de ligne de base au-delà de 10.
    #[serde(rename = "model.invalid_baseline_number")]
    #[error("ligne de base n° {number} : les numéros vont de 0 à 10")]
    InvalidBaselineNumber {
        /// Numéro refusé.
        number: u8,
    },
    /// Niveau hiérarchique nul.
    #[serde(rename = "model.invalid_outline_level")]
    #[error("niveau hiérarchique {level} : il commence à 1")]
    InvalidOutlineLevel {
        /// Niveau refusé.
        level: u16,
    },
    /// Priorité au-delà de 1 000.
    #[serde(rename = "model.invalid_priority")]
    #[error("priorité {priority} : elle va de 0 à 1 000")]
    InvalidPriority {
        /// Priorité refusée.
        priority: u16,
    },
    /// Réordonnancement d'une collection par une liste qui n'est pas une
    /// permutation exacte de ses UID (longueur différente, UID inconnu ou en
    /// double).
    #[serde(rename = "model.not_a_permutation")]
    #[error(
        "ordre refusé ({entity}) : {given} UID fournis pour {expected} présents, chacun exactement une fois{}",
        .uid.map_or_else(String::new, |u| format!(" (UID {u} inconnu ou en double)"))
    )]
    NotAPermutation {
        /// Type d'entité.
        entity: EntityKind,
        /// Nombre d'éléments présents.
        expected: usize,
        /// Nombre d'UID fournis.
        given: usize,
        /// Premier UID inconnu ou répété, s'il y en a un.
        uid: Option<u32>,
    },
    /// Tâche récapitulative du projet dont l'UID n'est pas 0.
    #[serde(rename = "model.summary_task_uid")]
    #[error("la tâche récapitulative du projet doit avoir l'UID 0 (lu : {uid})")]
    SummaryTaskUid {
        /// UID lu.
        uid: u32,
    },
}

impl ModelError {
    /// Code stable de l'erreur.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::DuplicateUid { .. } => "model.duplicate_uid",
            Self::ReservedUid { .. } => "model.reserved_uid",
            Self::PositionOutOfRange { .. } => "model.position_out_of_range",
            Self::UnsupportedFormatVersion { .. } => "model.unsupported_format_version",
            Self::InvalidGuid { .. } => "model.invalid_guid",
            Self::InvalidBaselineNumber { .. } => "model.invalid_baseline_number",
            Self::InvalidOutlineLevel { .. } => "model.invalid_outline_level",
            Self::InvalidPriority { .. } => "model.invalid_priority",
            Self::NotAPermutation { .. } => "model.not_a_permutation",
            Self::SummaryTaskUid { .. } => "model.summary_task_uid",
        }
    }
}

/// Toute erreur du cœur. Sérialisée comme l'erreur portée (son champ `code`).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize)]
#[serde(untagged)]
pub enum CoreError {
    /// Point fixe.
    #[error(transparent)]
    Fixed(#[from] FixedError),
    /// Temps.
    #[error(transparent)]
    Time(#[from] TimeError),
    /// Durées.
    #[error(transparent)]
    Duration(#[from] DurationError),
    /// Calendriers.
    #[error(transparent)]
    Calendar(#[from] CalendarError),
    /// Modèle.
    #[error(transparent)]
    Model(#[from] ModelError),
}

impl CoreError {
    /// Code stable de l'erreur portée.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Fixed(e) => e.code(),
            Self::Time(e) => e.code(),
            Self::Duration(e) => e.code(),
            Self::Calendar(e) => e.code(),
            Self::Model(e) => e.code(),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{
        CalendarError, CalendarModelError, CoreError, DurationError, EntityKind, FixedError, ModelError,
        TimeError,
    };
    use crate::calendar::Direction;
    use crate::ids::CalendarUid;
    use crate::time::Day;

    /// Vérifie que l'étiquette serde `code` vaut `code()`.
    pub(crate) fn assert_tag(value: &impl serde::Serialize, code: &str) {
        let json = serde_json::to_value(value).unwrap();
        assert_eq!(json["code"].as_str(), Some(code), "{json}");
    }

    #[test]
    fn fixed_error_codes_match_serde_tags() {
        for e in [
            FixedError::TooManyDecimals,
            FixedError::OutOfRange {
                min: 0,
                max: 1,
                value: 2,
            },
            FixedError::Overflow,
        ] {
            assert_tag(&e, e.code());
            assert!(e.code().starts_with("fixed."));
            assert_ne!(e.to_string(), "");
        }
    }

    #[test]
    fn time_error_codes_match_serde_tags() {
        for e in [
            TimeError::OutOfDomain,
            TimeError::InvalidTimeOfDay { minutes: 1441 },
            TimeError::NonZeroSeconds,
            TimeError::InvalidFormat {
                input: "x".to_owned(),
            },
            TimeError::InvalidDayRange {
                first: Day::MAX,
                last: Day::MIN,
            },
            TimeError::Overflow,
        ] {
            assert_tag(&e, e.code());
            assert!(e.code().starts_with("time."));
            assert_ne!(e.to_string(), "");
        }
    }

    #[test]
    fn duration_error_codes_match_serde_tags() {
        for e in [
            DurationError::Negative,
            DurationError::OutOfRange,
            DurationError::Overflow,
            DurationError::InvalidFactors {
                minutes_per_day: 0,
                minutes_per_week: 0,
                days_per_month: 0,
            },
        ] {
            assert_tag(&e, e.code());
            assert!(e.code().starts_with("duration."));
            assert_ne!(e.to_string(), "");
        }
    }

    #[test]
    fn calendar_model_error_codes_match_serde_tags() {
        for e in [
            CalendarModelError::EmptyRange,
            CalendarModelError::NoRanges,
            CalendarModelError::TooManyRanges { count: 6 },
            CalendarModelError::OverlappingRanges,
            CalendarModelError::InheritOnBase,
            CalendarModelError::InvalidRecurrence,
            CalendarModelError::OverlappingException { existing: 0 },
            CalendarModelError::OverlappingWorkWeek { existing: 0 },
            CalendarModelError::IndexOutOfRange { index: 3 },
        ] {
            assert_tag(&e, e.code());
            assert!(e.code().starts_with("calendar_model."));
            assert_ne!(e.to_string(), "");
        }
    }

    #[test]
    fn model_error_codes_match_serde_tags() {
        for e in [
            ModelError::DuplicateUid {
                entity: EntityKind::Task,
                uid: 1,
            },
            ModelError::ReservedUid { uid: 0 },
            ModelError::PositionOutOfRange { position: 9 },
            ModelError::UnsupportedFormatVersion { version: 2 },
            ModelError::InvalidGuid {
                input: "x".to_owned(),
            },
            ModelError::InvalidBaselineNumber { number: 11 },
            ModelError::InvalidOutlineLevel { level: 0 },
            ModelError::InvalidPriority { priority: 1001 },
            ModelError::NotAPermutation {
                entity: EntityKind::Task,
                expected: 3,
                given: 2,
                uid: None,
            },
            ModelError::NotAPermutation {
                entity: EntityKind::Task,
                expected: 3,
                given: 3,
                uid: Some(7),
            },
            ModelError::SummaryTaskUid { uid: 3 },
        ] {
            assert_tag(&e, e.code());
            assert!(e.code().starts_with("model."));
            assert_ne!(e.to_string(), "");
        }
        assert_eq!(EntityKind::Calendar.to_string(), "calendrier");
    }

    #[test]
    fn calendar_error_codes_match_serde_tags() {
        let c = CalendarUid::new(5);
        for e in [
            CalendarError::UnknownCalendar { calendar: c },
            CalendarError::BaseMissing { calendar: c, base: c },
            CalendarError::BaseNotBase { calendar: c, base: c },
            CalendarError::InheritanceCycle {
                chain: vec![c, CalendarUid::new(6), c],
            },
            CalendarError::NoWorkingTime { calendar: c },
            CalendarError::OutOfDomain {
                calendar: c,
                direction: Direction::After,
            },
            CalendarError::ElapsedOutOfDomain {
                direction: Direction::Before,
            },
            CalendarError::NegativeSpan,
            CalendarError::Model(CalendarModelError::TooManyRanges { count: 6 }),
            CalendarError::Inconsistent { calendar: c },
            CalendarError::TooManySegments { limit: 100_000 },
        ] {
            assert_tag(&e, e.code());
            assert_ne!(e.to_string(), "");
        }
        assert_eq!(
            CalendarError::InheritanceCycle {
                chain: vec![c, CalendarUid::new(6), c]
            }
            .to_string(),
            "boucle d'héritage entre calendriers : 5 → 6 → 5"
        );
        let model = CalendarError::from(CalendarModelError::EmptyRange);
        assert_eq!(model.code(), "calendar_model.empty_range");
        assert_eq!(
            serde_json::to_string(&model).unwrap(),
            r#"{"code":"calendar_model.empty_range"}"#
        );
    }

    #[test]
    fn core_error_delegates_code() {
        for e in [
            CoreError::from(FixedError::Overflow),
            CoreError::from(TimeError::OutOfDomain),
            CoreError::from(DurationError::Negative),
            CoreError::from(CalendarError::NegativeSpan),
            CoreError::from(ModelError::ReservedUid { uid: 0 }),
        ] {
            assert_tag(&e, e.code());
            assert_ne!(e.to_string(), "");
        }
    }
}
