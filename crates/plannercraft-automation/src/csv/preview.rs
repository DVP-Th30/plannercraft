//! Aperçu d'un CSV avant import (H-UI-40, ADR-0016 ; lot L3).
//!
//! Lit les [`PREVIEW_ROWS`] premières lignes de données avec les options de
//! l'import (séparateur, encodage, langue modifiables) et dit, colonne par
//! colonne, ce que l'import en fera. Les diagnostics sont ceux de l'import
//! du seul début du fichier (en-tête et lignes montrées) : même lecture,
//! mêmes codes, mêmes numéros de ligne ; le fichier entier n'est jamais
//! importé pour un aperçu.

use std::collections::BTreeSet;

use ::csv::{ReaderBuilder, StringRecord};

use super::decode::{decode, detect_separator, preamble};
use super::{
    CsvCode, CsvColumn, CsvDiagnostic, CsvEncoding, CsvError, CsvImportOptions, Detected, import, recognize,
};

/// Lignes de données au plus dans l'aperçu.
pub const PREVIEW_ROWS: usize = 20;

/// Sort d'une colonne d'en-tête.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recognized {
    /// Colonne lue.
    Column(CsvColumn),
    /// Colonne connue, non prise en charge.
    Unsupported,
    /// En-tête inconnu : ignorée.
    Unknown,
    /// Colonne en double : ignorée (la première gagne).
    Duplicate,
}

/// Aperçu d'un CSV.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CsvPreview {
    /// Séparateur, langue et encodage retenus.
    pub detected: Detected,
    /// En-têtes tels qu'écrits et leur sort.
    pub columns: Vec<(String, Recognized)>,
    /// Au plus [`PREVIEW_ROWS`] lignes de données, cellules brutes.
    pub rows: Vec<Vec<String>>,
    /// Diagnostics des lignes montrées.
    pub diagnostics: Vec<CsvDiagnostic>,
}

/// Vrai si toutes les cellules sont vides (espaces).
fn blank(record: &StringRecord) -> bool {
    record.iter().all(|c| c.trim().is_empty())
}

/// Sort de chaque en-tête : colonne lue, non prise en charge, inconnue, ou
/// en double (la première gagne, comme l'import).
fn columns_of(headers: &[String]) -> Vec<(String, Recognized)> {
    let mut seen = BTreeSet::new();
    headers
        .iter()
        .map(|h| {
            let sort = match recognize(h) {
                Recognized::Column(c) if !seen.insert(c) => Recognized::Duplicate,
                r => r,
            };
            (h.clone(), sort)
        })
        .collect()
}

/// Aperçu de `bytes` : en-tête, au plus [`PREVIEW_ROWS`] lignes de données
/// non vides (cellules brutes, toutes colonnes), diagnostics de l'import de
/// ce début de fichier, séparateur, langue et encodage retenus.
///
/// # Errors
///
/// [`CsvError`] (refus du fichier entier, comme l'import) : trop grand,
/// vide, sans en-tête, sans colonne Nom ni Durée, trop de colonnes, octets
/// invalides dans l'encodage imposé. Le nombre de lignes n'est pas
/// contrôlé ici (l'import le fera).
pub fn preview(bytes: &[u8], opts: &CsvImportOptions) -> Result<CsvPreview, CsvError> {
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if size > opts.limits.max_bytes {
        return Err(CsvError::TooLarge);
    }
    let decoded = decode(bytes, opts.encoding)?;
    if decoded.text.trim().is_empty() {
        return Err(CsvError::Empty);
    }
    let pre = preamble(&decoded.text);
    let body = decoded.text.get(pre.body_start..).unwrap_or("");
    let separator = opts
        .separator
        .or(pre.separator)
        .unwrap_or_else(|| detect_separator(body));
    let mut reader = ReaderBuilder::new()
        .delimiter(separator)
        .has_headers(false)
        .flexible(true)
        .from_reader(body.as_bytes());
    let mut record = StringRecord::new();
    let headers = loop {
        match reader.read_record(&mut record) {
            Ok(true) if blank(&record) => {}
            Ok(true) => break record.iter().map(str::to_owned).collect::<Vec<_>>(),
            Ok(false) | Err(_) => return Err(CsvError::NoHeader),
        }
    };
    if headers.len() > usize::from(opts.limits.max_columns) {
        return Err(CsvError::TooManyColumns);
    }
    let columns = columns_of(&headers);
    let mut rows = Vec::new();
    while rows.len() < PREVIEW_ROWS {
        match reader.read_record(&mut record) {
            Ok(true) if blank(&record) => {}
            Ok(true) => {
                if record.len() > usize::from(opts.limits.max_columns) {
                    return Err(CsvError::TooManyColumns);
                }
                rows.push(record.iter().map(str::to_owned).collect());
            }
            // Fin du fichier, ou enregistrement illisible : l'import du
            // début en fait un diagnostic.
            Ok(false) | Err(_) => break,
        }
    }
    // Début du fichier jusqu'à la fin du dernier enregistrement lu, importé
    // tel quel (séparateur et encodage fixés : le texte est déjà décodé).
    let end = usize::try_from(reader.position().byte())
        .unwrap_or(usize::MAX)
        .saturating_add(pre.body_start);
    let head = decoded.text.get(..end).unwrap_or(&decoded.text);
    let mut head_opts = *opts;
    head_opts.separator = Some(separator);
    head_opts.encoding = Some(CsvEncoding::Utf8);
    let imported = import(head.as_bytes(), &head_opts)?;
    let mut diagnostics = imported.diagnostics;
    if decoded.malformed {
        diagnostics.insert(
            0,
            CsvDiagnostic {
                line: 0,
                column: None,
                code: CsvCode::InvalidChar,
                value: None,
                others: 0,
            },
        );
    }
    Ok(CsvPreview {
        detected: Detected {
            separator,
            locale: imported.detected.locale,
            encoding: decoded.encoding,
        },
        columns,
        rows,
        diagnostics,
    })
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use plannercraft_core::{Moment, ScheduleAnchor};

    use super::*;
    use crate::csv::CsvLocale;

    fn opts() -> CsvImportOptions {
        let now: Moment = "2026-10-19T08:00".parse().unwrap();
        CsvImportOptions::new(ScheduleAnchor::FromStart(now), now)
    }

    #[test]
    fn csv_preview_20_rows() {
        use std::fmt::Write as _;
        let mut text = String::from("Nom;Durée;Couleur;Nom\n");
        for i in 0..30 {
            write!(text, "T{i};{}j;bleu;x\n\n", i + 1).unwrap();
        }
        let p = preview(text.as_bytes(), &opts()).unwrap();
        assert_eq!(p.rows.len(), PREVIEW_ROWS);
        assert_eq!(p.rows[0], ["T0", "1j", "bleu", "x"]);
        assert_eq!(p.rows[19][0], "T19");
        assert_eq!(
            p.columns.iter().map(|(_, r)| *r).collect::<Vec<_>>(),
            [
                Recognized::Column(CsvColumn::Name),
                Recognized::Column(CsvColumn::Duration),
                Recognized::Unknown,
                Recognized::Duplicate,
            ]
        );
        // Diagnostics de l'en-tête seulement (aucune ligne au-delà de la
        // vingtième n'est lue) ; ordre de l'import (rang du premier « Nom »).
        let codes: Vec<&str> = p.diagnostics.iter().map(|d| d.code.code()).collect();
        assert_eq!(codes, ["csv.duplicate_column", "csv.unknown_column"]);
    }

    #[test]
    fn csv_preview_detects_fr_semicolon() {
        let p = preview("Nom;Durée\nCoffrage;3j\nFerraillage;2,5j\n".as_bytes(), &opts()).unwrap();
        assert_eq!(p.detected.separator, b';');
        assert_eq!(p.detected.locale, CsvLocale::Fr);
        assert_eq!(p.detected.encoding, CsvEncoding::Utf8);
        assert_eq!(p.rows.len(), 2);
        assert!(p.diagnostics.is_empty(), "{:?}", p.diagnostics);
        // Séparateur imposé : une seule colonne « Nom;Durée », reconnue
        // nulle part ⇒ refus comme l'import.
        let mut forced = opts();
        forced.separator = Some(b',');
        assert_eq!(
            preview(b"Nom;Duree\nA;1j\n", &forced),
            Err(CsvError::NoKnownColumn)
        );
        // Anglais, virgule, Windows-1252 détecté.
        let p = preview(b"Name,Duration\nCaf\xe9,2d\n", &opts()).unwrap();
        assert_eq!(
            (p.detected.separator, p.detected.locale, p.detected.encoding),
            (b',', CsvLocale::En, CsvEncoding::Windows1252)
        );
        assert_eq!(p.rows[0][0], "Café");
    }

    #[test]
    fn csv_preview_refusals_and_bad_values() {
        assert_eq!(preview(b"  \n", &opts()), Err(CsvError::Empty));
        let mut small = opts();
        small.limits.max_bytes = 4;
        assert_eq!(preview(b"Nom;Duree\n", &small), Err(CsvError::TooLarge));
        // Valeur illisible dans une ligne montrée : diagnostic de sa ligne.
        let p = preview("Nom;Durée\nA;zz\n".as_bytes(), &opts()).unwrap();
        assert_eq!(p.diagnostics.len(), 1);
        assert_eq!(p.diagnostics[0].line, 2);
        assert_eq!(p.diagnostics[0].code.code(), "csv.bad_value");
    }
}
