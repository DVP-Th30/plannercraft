//! Texte des valeurs de planification, à la manière de MS Project (ADR-0008 :
//! « parseur du texte : J4 (plannercraft-automation) », avancé en J3 par
//! l'ADR-0010) : durées (« 5j », « 1,5 j », « 5ej », « 2j? »), décalages
//! (« +2j », « -50% »), prédécesseurs (« 3FD+2j;5DD », « 3FS+2d,5SS »),
//! dates (« 19/10/2026 08:00 »), en français et en anglais ; texte
//! d'affichage des colonnes de la table (`display`, J7 lot L6 : « 5 jours »,
//! « 3FD+2 jours », « 50 % », « Maçon[50 %];Électricien »), les mots étant
//! fournis par l'appelant.
//!
//! Aucune entrée/sortie : du texte en entrée, une valeur du modèle en sortie
//! (et l'inverse). Aucun flottant : les nombres sont lus en
//! [`Decimal`](plannercraft_core::Decimal) et convertis par R-001. Une entrée
//! dépasse rarement quelques caractères ; elle est bornée à
//! [`MAX_INPUT_CHARS`] caractères ([`TextError::TooLong`]).
//!
//! Règles : R-140 (durée, lecture), R-141 (durée et décalage, écriture),
//! R-142 (prédécesseurs), R-143 (dates).

mod date;
mod display;
mod duration;
mod predecessors;

pub(crate) mod lex;

pub use date::{DateRole, format_date, parse_date};
pub use display::{
    DisplayLabels, ESTIMATED_SUFFIX, PluralRule, display_duration, display_minutes_as, display_percent,
    display_predecessors, format_resource_names, parse_percent, write_decimal, write_display_duration,
    write_display_lag,
};
pub use duration::{format_duration, format_lag, parse_duration, parse_lag};
pub(crate) use predecessors::predecessor_elements;
pub use predecessors::{
    PredecessorRef, format_predecessor, format_predecessors, link_code, parse_predecessors,
};

use plannercraft_core::{ConversionFactors, DurationUnit, Project, ProjectOptions, TimeOfDay};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Longueur maximale d'une entrée, en caractères.
pub const MAX_INPUT_CHARS: usize = 1_024;

/// Langue d'écriture. La lecture accepte le français et l'anglais quelle que
/// soit la langue du contexte. Désérialisable (`"fr"`, `"en"`) : langue d'un
/// lot de commandes (ADR-0012 D6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    /// Français : « 5j », « 2s », « 3FD+2j ».
    Fr,
    /// Anglais : « 5d », « 2w », « 3FS+2d ».
    En,
}

/// Marque décimale à l'écriture. La lecture d'une durée accepte les deux.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DecimalMark {
    /// Virgule (« 1,5j »).
    Comma,
    /// Point (« 1.5d »).
    Dot,
}

/// Ordre du jour et du mois dans une date saisie.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DateOrder {
    /// JJ/MM/AAAA (français).
    DayMonthYear,
    /// MM/JJ/AAAA (anglais des États-Unis).
    MonthDayYear,
}

/// Contexte de lecture et d'écriture du texte : langue, marques, facteurs
/// de conversion et valeurs par défaut du projet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextContext {
    /// Langue d'écriture ; la lecture accepte le français et l'anglais.
    pub language: Language,
    /// Marque décimale à l'écriture.
    pub decimal: DecimalMark,
    /// Séparateur d'une liste de prédécesseurs (`;` en français, `,` en
    /// anglais : séparateur de liste des paramètres régionaux, S38) ; à la
    /// lecture, l'anglais admet aussi `;` (R-142).
    pub list_separator: char,
    /// Facteurs de conversion des unités (R-006, R-007).
    pub factors: ConversionFactors,
    /// Unité d'une durée saisie sans unité (S10).
    pub default_duration_unit: DurationUnit,
    /// Heure d'une date de début saisie sans heure (R-033).
    pub default_start_time: TimeOfDay,
    /// Heure d'une date de fin saisie sans heure (R-033).
    pub default_finish_time: TimeOfDay,
    /// Ordre du jour et du mois dans une date.
    pub date_order: DateOrder,
}

impl TextContext {
    /// Contexte d'un projet : ses facteurs et ses valeurs par défaut, les
    /// marques de la langue (français : virgule, `;`, JJ/MM ; anglais :
    /// point, `,`, MM/JJ).
    #[must_use]
    pub fn for_project(p: &Project, language: Language) -> Self {
        Self::for_options(&p.options, language)
    }

    /// Contexte depuis les options d'un projet (même règle que
    /// [`TextContext::for_project`]).
    #[must_use]
    pub fn for_options(options: &ProjectOptions, language: Language) -> Self {
        let (decimal, list_separator, date_order) = match language {
            Language::Fr => (DecimalMark::Comma, ';', DateOrder::DayMonthYear),
            Language::En => (DecimalMark::Dot, ',', DateOrder::MonthDayYear),
        };
        Self {
            language,
            decimal,
            list_separator,
            factors: options.factors,
            default_duration_unit: options.default_duration_unit,
            default_start_time: options.default_start_time,
            default_finish_time: options.default_finish_time,
            date_order,
        }
    }
}

/// Texte refusé. Positions en caractères (pas en octets), depuis 0.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize)]
#[serde(tag = "code")]
pub enum TextError {
    /// Entrée vide (ou faite d'espaces).
    #[serde(rename = "text.empty")]
    #[error("valeur vide")]
    Empty,
    /// Nombre attendu ou illisible.
    #[serde(rename = "text.bad_number")]
    #[error("nombre attendu ou illisible au caractère {at}")]
    BadNumber {
        /// Position du premier caractère fautif.
        at: usize,
    },
    /// Unité inconnue.
    #[serde(rename = "text.unknown_unit")]
    #[error("unité inconnue : « {unit} »")]
    UnknownUnit {
        /// Texte de l'unité tel que saisi.
        unit: String,
    },
    /// Durée négative (seul un décalage peut l'être, R-008).
    #[serde(rename = "text.negative")]
    #[error("une durée ne peut pas être négative")]
    Negative,
    /// Valeur hors des bornes du modèle (R-002, R-003, R-008).
    #[serde(rename = "text.out_of_range")]
    #[error("valeur hors des bornes")]
    OutOfRange,
    /// Type de lien inconnu.
    #[serde(rename = "text.bad_link_type")]
    #[error("type de lien inconnu : « {code} »")]
    BadLinkType {
        /// Code tel que saisi (`value` en JSON : `code` y est le code de
        /// l'erreur).
        #[serde(rename = "value")]
        code: String,
    },
    /// Prédécesseur dans un autre projet (chemin de fichier, S38) : non pris
    /// en charge.
    #[serde(rename = "text.external_predecessor")]
    #[error("prédécesseur externe (autre projet) non pris en charge")]
    ExternalPredecessor,
    /// Date illisible ou inexistante.
    #[serde(rename = "text.bad_date")]
    #[error("date illisible ou inexistante")]
    BadDate,
    /// Entrée trop longue ([`MAX_INPUT_CHARS`] caractères au plus).
    #[serde(rename = "text.too_long")]
    #[error("texte trop long (au plus {} caractères)", MAX_INPUT_CHARS)]
    TooLong,
}

impl TextError {
    /// Code stable de l'erreur (`text.empty`…), égal au champ `code` du JSON.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Empty => "text.empty",
            Self::BadNumber { .. } => "text.bad_number",
            Self::UnknownUnit { .. } => "text.unknown_unit",
            Self::Negative => "text.negative",
            Self::OutOfRange => "text.out_of_range",
            Self::BadLinkType { .. } => "text.bad_link_type",
            Self::ExternalPredecessor => "text.external_predecessor",
            Self::BadDate => "text.bad_date",
            Self::TooLong => "text.too_long",
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::TextError;

    #[test]
    fn error_codes_match_serialization() {
        for e in [
            TextError::Empty,
            TextError::BadNumber { at: 3 },
            TextError::UnknownUnit { unit: "x".to_owned() },
            TextError::Negative,
            TextError::OutOfRange,
            TextError::BadLinkType {
                code: "XY".to_owned(),
            },
            TextError::ExternalPredecessor,
            TextError::BadDate,
            TextError::TooLong,
        ] {
            let v = serde_json::to_value(&e).unwrap();
            assert_eq!(v["code"], e.code());
            assert_ne!(e.to_string(), "");
        }
    }
}
