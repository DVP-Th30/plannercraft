//! Journal de rejeu `--journal` : requête écrite (et synchronisée) avant
//! l'appel, résultat après (lot L8).
//!
//! Deux lignes [`Entry`] par appel, de même `seq` :
//! 1. la requête (`tool` = nom de l'outil MCP, `params`), sans `result`,
//!    écrite et synchronisée sur disque **avant** l'appel ;
//! 2. le résultat (`params` à `null`, `result` = `{"ok": valeur}` ou
//!    `{"error": erreur structurée}`), après l'appel.
//!
//! Le fichier est complété s'il existe ; `seq` repart de 1 à chaque
//! lancement du serveur. Une erreur d'écriture est signalée sur stderr et
//! l'appel continue (hypothèse : le journal ne doit pas empêcher le
//! travail).

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use plannercraft_automation::journal::{Entry, to_line};
use serde_json::Value;

/// Version du format des entrées.
const ENTRY_VERSION: u32 = 1;

/// Fichier de journal ouvert.
#[derive(Debug)]
pub(crate) struct JournalFile {
    path: PathBuf,
    file: File,
    seq: u64,
}

impl JournalFile {
    /// Ouvre (ou crée) le journal en ajout.
    ///
    /// # Errors
    ///
    /// L'erreur d'ouverture, en texte.
    pub(crate) fn open(path: &Path) -> Result<Self, String> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| format!("journal {} : {e}", path.display()))?;
        Ok(Self {
            path: path.to_path_buf(),
            file,
            seq: 0,
        })
    }

    /// Écrit la requête et la synchronise ; renvoie son numéro d'ordre.
    pub(crate) fn begin(&mut self, tool: &str, params: &Value) -> u64 {
        self.seq = self.seq.saturating_add(1);
        let entry = Entry {
            v: ENTRY_VERSION,
            seq: self.seq,
            tool: tool.to_owned(),
            params: params.clone(),
            result: None,
        };
        self.write(&entry);
        self.seq
    }

    /// Écrit le résultat de l'appel `seq`.
    pub(crate) fn finish(&mut self, seq: u64, tool: &str, result: Value) {
        let entry = Entry {
            v: ENTRY_VERSION,
            seq,
            tool: tool.to_owned(),
            params: Value::Null,
            result: Some(result),
        };
        self.write(&entry);
    }

    fn write(&mut self, entry: &Entry) {
        let mut line = to_line(entry);
        line.push('\n');
        let res = self
            .file
            .write_all(line.as_bytes())
            .and_then(|()| self.file.flush())
            .and_then(|()| match self.file.sync_data() {
                // Tube ou périphérique qui ne se synchronise pas : rien à
                // garantir de plus.
                Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => Ok(()),
                other => other,
            });
        if let Err(e) = res {
            // Diagnostic seulement : l'appel continue.
            eprintln!(
                "plannercraft mcp : écriture du journal {} impossible (seq {}) : {e}",
                self.path.display(),
                entry.seq
            );
        }
    }
}
