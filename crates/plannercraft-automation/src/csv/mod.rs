//! CSV des tâches (ADR-0003, ADR-0010 D16) : import d'un fichier exporté par
//! MS Project (Fichier › Enregistrer sous › CSV) ou d'un tableur, export
//! relisible par les deux.
//!
//! Aucune entrée/sortie : des octets en entrée, un [`Project`] en sortie (et
//! l'inverse). Lecture RFC 4180 par la crate `csv` (champs entre guillemets,
//! multilignes), décodage par `encoding_rs`. Une valeur illisible devient un
//! diagnostic par ligne et la valeur par défaut : jamais d'échec global pour
//! une ligne, jamais de panique (règle 10). Seuls la taille, l'absence
//! d'en-tête exploitable et les bornes de [`CsvLimits`] refusent le fichier.
//!
//! Règles : R-144 (encodage, séparateur, locale, en-têtes), R-145 (lignes
//! vers tâches), R-146 (export).

mod decode;
mod export;
mod header;
mod import;
mod preview;

pub(crate) use export::guard;
pub use export::{CsvExportOptions, default_columns, export};
pub use import::{MAX_DIAGNOSTICS, MAX_DIAGNOSTICS_PER_CODE, import};
pub use preview::{CsvPreview, PREVIEW_ROWS, Recognized, preview};

use plannercraft_core::{Moment, Project, ScheduleAnchor};
use serde::Serialize;

/// R-144 : sort d'un intitulé de colonne d'en-tête (casse, accents,
/// espaces et ponctuation ignorés ; français ou anglais) : colonne lue,
/// connue mais non prise en charge, ou inconnue (jamais
/// [`Recognized::Duplicate`], qui dépend des autres colonnes). Sert aussi
/// au collage d'un texte externe (R-256).
#[must_use]
pub fn recognize(header: &str) -> Recognized {
    match header::recognize(header) {
        Some((header::Field::Column(c), _)) => Recognized::Column(c),
        Some((header::Field::Unsupported, _)) => Recognized::Unsupported,
        None => Recognized::Unknown,
    }
}

/// Bornes de l'import (règle 10 : un fichier hostile ne fait pas croître la
/// mémoire sans borne).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CsvLimits {
    /// Taille maximale du fichier, en octets (64 Mio).
    pub max_bytes: u64,
    /// Nombre maximal de lignes de données (200 000).
    pub max_rows: u32,
    /// Nombre maximal de colonnes (256).
    pub max_columns: u16,
    /// Taille maximale d'un champ, en octets (1 Mio) ; au-delà, la valeur est
    /// ignorée (`csv.bad_value`).
    pub max_field_bytes: usize,
}

impl Default for CsvLimits {
    fn default() -> Self {
        Self {
            max_bytes: 67_108_864,
            max_rows: 200_000,
            max_columns: 256,
            max_field_bytes: 1_048_576,
        }
    }
}

/// Paramètres régionaux d'un CSV : langue des en-têtes et des valeurs,
/// marque décimale, ordre des dates, séparateur de liste.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CsvLocale {
    /// Français : en-têtes français, virgule décimale, JJ/MM/AAAA, `;`.
    Fr,
    /// Anglais : en-têtes anglais, point décimal, MM/JJ/AAAA, `,`.
    En,
}

/// Encodage du fichier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CsvEncoding {
    /// UTF-8 (avec ou sans BOM).
    Utf8,
    /// UTF-16 petit-boutiste.
    Utf16Le,
    /// UTF-16 gros-boutiste.
    Utf16Be,
    /// Windows-1252 (« ANSI » d'un Windows occidental).
    Windows1252,
}

/// Traitement des colonnes Début et Fin à l'import.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CsvDates {
    /// Ignorées : le moteur recalcule ; un écart est signalé
    /// (`csv.date_mismatch`, information).
    #[default]
    Ignore,
    /// Converties en contraintes, comme une date saisie à la main dans MS
    /// Project (S30) : Début → « ne pas commencer avant le », Fin → « ne pas
    /// finir avant le ».
    Constrain,
}

/// Options d'import.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CsvImportOptions {
    /// Séparateur imposé ; `None` : détecté sur la ligne d'en-tête.
    pub separator: Option<u8>,
    /// Paramètres régionaux imposés ; `None` : détectés.
    pub locale: Option<CsvLocale>,
    /// Encodage imposé ; `None` : détecté.
    pub encoding: Option<CsvEncoding>,
    /// Sens et date pilote du projet créé.
    pub anchor: ScheduleAnchor,
    /// « Maintenant », injecté (ADR-0005).
    pub current_date: Moment,
    /// Traitement des colonnes Début et Fin.
    pub dates: CsvDates,
    /// Bornes.
    pub limits: CsvLimits,
}

impl CsvImportOptions {
    /// Options par défaut pour un projet de sens et de date `anchor` : tout
    /// détecté, dates ignorées, bornes par défaut.
    #[must_use]
    pub fn new(anchor: ScheduleAnchor, current_date: Moment) -> Self {
        Self {
            separator: None,
            locale: None,
            encoding: None,
            anchor,
            current_date,
            dates: CsvDates::Ignore,
            limits: CsvLimits::default(),
        }
    }
}

/// Ce que l'import a retenu (imposé ou détecté).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Detected {
    /// Séparateur de champs.
    pub separator: u8,
    /// Paramètres régionaux.
    pub locale: CsvLocale,
    /// Encodage.
    pub encoding: CsvEncoding,
}

/// Résultat d'un import.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CsvImport {
    /// Projet créé.
    pub project: Project,
    /// Diagnostics, dans l'ordre des lignes.
    pub diagnostics: Vec<CsvDiagnostic>,
    /// Séparateur, paramètres régionaux et encodage retenus.
    pub detected: Detected,
}

/// Diagnostic d'import, rattaché à une ligne physique du fichier.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CsvDiagnostic {
    /// Ligne physique (1 = première ligne du fichier ; 0 = le fichier
    /// entier).
    pub line: u64,
    /// En-tête de la colonne en cause, tel qu'écrit dans le fichier.
    pub column: Option<String>,
    /// Nature.
    pub code: CsvCode,
    /// Valeur brute en cause (200 caractères au plus) ; pour une cellule de
    /// prédécesseurs, le premier élément fautif.
    pub value: Option<String>,
    /// Autres éléments fautifs de la même cellule de prédécesseurs, résumés
    /// par ce diagnostic (un seul par cellule) ; 0 sinon.
    #[serde(skip_serializing_if = "is_zero")]
    pub others: u32,
}

/// Vrai pour 0 (sérialisation de [`CsvDiagnostic::others`]).
#[allow(clippy::trivially_copy_pass_by_ref, reason = "signature imposée par serde")]
const fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// Nature d'un diagnostic d'import (codes stables `csv.*`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "code")]
pub enum CsvCode {
    /// En-tête inconnu : colonne ignorée.
    #[serde(rename = "csv.unknown_column")]
    UnknownColumn,
    /// Colonne connue, non prise en charge en J3 (ressources, travail).
    #[serde(rename = "csv.unsupported_column")]
    UnsupportedColumn,
    /// Colonne en double : la première gagne.
    #[serde(rename = "csv.duplicate_column")]
    DuplicateColumn,
    /// Valeur illisible : valeur par défaut.
    #[serde(rename = "csv.bad_value")]
    BadValue,
    /// Saut de niveau hiérarchique de plus d'un cran : ramené au niveau
    /// précédent + 1.
    #[serde(rename = "csv.outline_jump")]
    OutlineJump,
    /// Durée d'une récapitulative ignorée (calculée, R-054) : information.
    #[serde(rename = "csv.summary_duration_ignored")]
    SummaryDurationIgnored,
    /// Prédécesseur inconnu : lien ignoré.
    #[serde(rename = "csv.unknown_predecessor")]
    UnknownPredecessor,
    /// Tâche prédécesseur d'elle-même : lien ignoré.
    #[serde(rename = "csv.self_predecessor")]
    SelfPredecessor,
    /// Prédécesseur répété sur une ligne : le premier gagne.
    #[serde(rename = "csv.duplicate_predecessor")]
    DuplicatePredecessor,
    /// Prédécesseur d'un autre projet : lien ignoré.
    #[serde(rename = "csv.external_predecessor")]
    ExternalPredecessor,
    /// Date du fichier différente de la date calculée (dates ignorées) :
    /// information.
    #[serde(rename = "csv.date_mismatch")]
    DateMismatch {
        /// Date du fichier.
        csv: Moment,
        /// Date calculée ; `None` si la tâche n'est pas planifiée.
        computed: Option<Moment>,
    },
    /// Colonne de dates lue jour/mois (ou mois/jour) selon la locale déduite
    /// des seuls en-têtes, sans qu'aucune valeur ne lève l'ambiguïté (R-144,
    /// amendée en J4) : avertissement, une fois par colonne, à la première
    /// valeur que l'autre ordre lirait autrement.
    #[serde(rename = "csv.date_ambiguous")]
    DateAmbiguous,
    /// Caractère de contrôle (ou séquence d'encodage invalide) remplacé
    /// par U+FFFD.
    #[serde(rename = "csv.invalid_char")]
    InvalidChar,
    /// Colonne « N° unique des prédécesseurs » sans colonne « N° unique » :
    /// les UID sont réattribués, les liens par UID seraient faux ; colonne
    /// ignorée (une fois, première ligne concernée).
    #[serde(rename = "csv.unique_predecessors_without_uid")]
    UniquePredecessorsWithoutUid,
    /// Diagnostics supprimés au-delà des plafonds ([`MAX_DIAGNOSTICS`],
    /// [`MAX_DIAGNOSTICS_PER_CODE`]) : un seul, en fin de liste, ligne 0.
    #[serde(rename = "csv.truncated")]
    Truncated {
        /// Nombre de diagnostics supprimés.
        dropped: u64,
    },
}

impl CsvCode {
    /// Code stable (`csv.bad_value`…), égal au champ `code` du JSON.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnknownColumn => "csv.unknown_column",
            Self::UnsupportedColumn => "csv.unsupported_column",
            Self::DuplicateColumn => "csv.duplicate_column",
            Self::BadValue => "csv.bad_value",
            Self::OutlineJump => "csv.outline_jump",
            Self::SummaryDurationIgnored => "csv.summary_duration_ignored",
            Self::UnknownPredecessor => "csv.unknown_predecessor",
            Self::SelfPredecessor => "csv.self_predecessor",
            Self::DuplicatePredecessor => "csv.duplicate_predecessor",
            Self::ExternalPredecessor => "csv.external_predecessor",
            Self::DateMismatch { .. } => "csv.date_mismatch",
            Self::DateAmbiguous => "csv.date_ambiguous",
            Self::InvalidChar => "csv.invalid_char",
            Self::UniquePredecessorsWithoutUid => "csv.unique_predecessors_without_uid",
            Self::Truncated { .. } => "csv.truncated",
        }
    }

    /// Gravité : information pour une durée de récapitulative ignorée et un
    /// écart de date ; avertissement sinon (une valeur par défaut a été
    /// appliquée ou une donnée ignorée).
    #[must_use]
    pub const fn severity(&self) -> plannercraft_core::Severity {
        match self {
            Self::SummaryDurationIgnored | Self::DateMismatch { .. } => plannercraft_core::Severity::Info,
            _ => plannercraft_core::Severity::Warning,
        }
    }
}

/// Refus du fichier entier.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize)]
#[serde(tag = "code")]
pub enum CsvError {
    /// Fichier plus grand que [`CsvLimits::max_bytes`].
    #[serde(rename = "csv.error.too_large")]
    #[error("fichier CSV trop grand")]
    TooLarge,
    /// Fichier vide (ou fait d'espaces).
    #[serde(rename = "csv.error.empty")]
    #[error("fichier CSV vide")]
    Empty,
    /// Aucune ligne d'en-tête (lignes vides seulement).
    #[serde(rename = "csv.error.no_header")]
    #[error("aucune ligne d'en-tête")]
    NoHeader,
    /// Ni colonne Nom ni colonne Durée.
    #[serde(rename = "csv.error.no_known_column")]
    #[error("aucune colonne Nom ni Durée dans l'en-tête")]
    NoKnownColumn,
    /// Plus de lignes que [`CsvLimits::max_rows`].
    #[serde(rename = "csv.error.too_many_rows")]
    #[error("trop de lignes")]
    TooManyRows,
    /// Plus de colonnes que [`CsvLimits::max_columns`].
    #[serde(rename = "csv.error.too_many_columns")]
    #[error("trop de colonnes")]
    TooManyColumns,
    /// Octets invalides dans l'encodage imposé.
    #[serde(rename = "csv.error.encoding")]
    #[error("le fichier n'est pas dans l'encodage indiqué")]
    Encoding,
    /// Fonction posée par le contrat J8 (L0), livrée par un lot ultérieur
    /// (aperçu, L3).
    #[serde(rename = "csv.error.not_yet_available")]
    #[error("fonction pas encore disponible")]
    NotYetAvailable,
}

impl CsvError {
    /// Code stable de l'erreur, égal au champ `code` du JSON.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::TooLarge => "csv.error.too_large",
            Self::Empty => "csv.error.empty",
            Self::NoHeader => "csv.error.no_header",
            Self::NoKnownColumn => "csv.error.no_known_column",
            Self::TooManyRows => "csv.error.too_many_rows",
            Self::TooManyColumns => "csv.error.too_many_columns",
            Self::NotYetAvailable => "csv.error.not_yet_available",
            Self::Encoding => "csv.error.encoding",
        }
    }
}

/// Colonne d'un CSV des tâches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CsvColumn {
    /// N° (ID, position).
    Id,
    /// N° unique (UID).
    UniqueId,
    /// Nom.
    Name,
    /// Durée.
    Duration,
    /// Début.
    Start,
    /// Fin.
    Finish,
    /// Prédécesseurs (par ID).
    Predecessors,
    /// N° unique des prédécesseurs (par UID).
    UniquePredecessors,
    /// Niveau hiérarchique.
    OutlineLevel,
    /// Jalon.
    Milestone,
    /// Notes.
    Notes,
    /// Code WBS.
    Wbs,
    /// % achevé.
    PercentComplete,
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{CsvCode, CsvError};
    use plannercraft_core::Moment;

    #[test]
    fn codes_match_serialization() {
        for c in [
            CsvCode::UnknownColumn,
            CsvCode::UnsupportedColumn,
            CsvCode::DuplicateColumn,
            CsvCode::BadValue,
            CsvCode::OutlineJump,
            CsvCode::SummaryDurationIgnored,
            CsvCode::UnknownPredecessor,
            CsvCode::SelfPredecessor,
            CsvCode::DuplicatePredecessor,
            CsvCode::ExternalPredecessor,
            CsvCode::DateMismatch {
                csv: Moment::MIN,
                computed: None,
            },
            CsvCode::DateAmbiguous,
            CsvCode::InvalidChar,
            CsvCode::UniquePredecessorsWithoutUid,
            CsvCode::Truncated { dropped: 3 },
        ] {
            assert_eq!(serde_json::to_value(&c).unwrap()["code"], c.code());
        }
        for e in [
            CsvError::TooLarge,
            CsvError::Empty,
            CsvError::NoHeader,
            CsvError::NoKnownColumn,
            CsvError::TooManyRows,
            CsvError::TooManyColumns,
            CsvError::Encoding,
            CsvError::NotYetAvailable,
        ] {
            assert_eq!(serde_json::to_value(&e).unwrap()["code"], e.code());
            assert_ne!(e.to_string(), "");
        }
    }
}
