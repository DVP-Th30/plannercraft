//! Journal JSONL des appels (ADR-0012 : un journal d'enveloppes
//! `{v, seq, tool, params, result?}`, rejeu `build_30` et trace ; lot L6).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Entrée du journal : une ligne JSON.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// Version du format (1).
    pub v: u32,
    /// Numéro d'ordre.
    pub seq: u64,
    /// Id de la commande ou nom de l'outil.
    pub tool: String,
    /// Paramètres.
    pub params: Value,
    /// Résultat, s'il est connu.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
}

/// Ligne du journal illisible.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum JournalError {
    /// JSON invalide ou forme inattendue.
    #[error("ligne de journal illisible : {0}")]
    Parse(String),
}

/// Ligne JSON compacte d'une entrée, sans fin de ligne.
#[must_use]
pub fn to_line(e: &Entry) -> String {
    // Une `Value` se sérialise toujours (clés textuelles) : la chaîne vide
    // n'est qu'un repli sans panique.
    serde_json::to_string(e).unwrap_or_default()
}

/// Lit une ligne du journal.
///
/// # Errors
///
/// [`JournalError::Parse`] si la ligne n'est pas une entrée.
pub fn parse_line(s: &str) -> Result<Entry, JournalError> {
    serde_json::from_str(s).map_err(|e| JournalError::Parse(e.to_string()))
}
