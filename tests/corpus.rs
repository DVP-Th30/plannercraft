//! Harnais du corpus public (ADR-0004, ADR-0009 ; cible `tests/corpus`).
//!
//! Chaîne : manifeste figé ([`manifest`]) → fichier présent et empreinte
//! conforme → lecture MSPDI (`plannercraft_mspdi::read` : les valeurs calculées par
//! MS Project vont dans `RecordedSchedule`, jamais dans le `Project`, donc
//! l'effacement est structurel) → recalcul (`plannercraft_core::schedule`) →
//! comparaison champ par champ ([`compare`]) → verdict contre les planchers
//! ([`verdict`]) → rapport sans nom de tâche ([`report`]).
//!
//! Aucun téléchargement ici : `cargo xtask corpus fetch` remplit
//! `tests/corpus/files/`. Un fichier absent ne compte ni pour ni contre.

pub mod compare;
pub mod manifest;
pub mod report;
pub mod resources;
pub mod roundtrip;
pub mod verdict;

use std::path::Path;

use plannercraft_mspdi::{MspdiDocument, ReadOptions};

use crate::corpus::compare::FileComparison;
use crate::corpus::manifest::Entry;
use crate::corpus::resources::ResourceComparison;
use crate::corpus::verdict::{AbsentReason, FileResult, FileStatus};

/// « Maintenant » injecté à la lecture (règle 5), utilisé seulement si un
/// fichier n'a pas de `CurrentDate` : 2026-01-01 00:00.
pub const CURRENT_DATE: &str = "2026-01-01T00:00";

/// Résultat complet d'un fichier du manifeste.
#[derive(Debug)]
pub struct FileRun {
    /// Entrée du manifeste.
    pub entry: Entry,
    /// État pour le verdict (absent, erreur de lecture, comparé).
    pub status: FileStatus,
    /// Document lu (fichier présent et lisible).
    pub document: Option<MspdiDocument>,
    /// Codes des diagnostics du moteur (fichier comparé).
    pub engine_diagnostics: Vec<String>,
    /// Comparaison (fichier comparé).
    pub comparison: Option<FileComparison>,
    /// Comparaison des ressources (fichier comparé, J5 lot L9).
    pub resources: Option<ResourceComparison>,
}

impl FileRun {
    /// Vue réduite pour le verdict.
    #[must_use]
    pub fn result(&self) -> FileResult {
        FileResult {
            name: self.entry.name.clone(),
            floor: self.entry.floor,
            complete_required: self.entry.producer == manifest::Producer::Msproject,
            status: self.status.clone(),
        }
    }
}

/// Traite chaque entrée du manifeste, dans l'ordre, sur les fichiers de
/// `files_dir`.
#[must_use]
pub fn run(entries: &[Entry], files_dir: &Path) -> Vec<FileRun> {
    entries.iter().map(|entry| run_file(entry, files_dir)).collect()
}

fn absent(entry: &Entry, reason: AbsentReason) -> FileRun {
    FileRun {
        entry: entry.clone(),
        status: FileStatus::Absent(reason),
        document: None,
        engine_diagnostics: Vec::new(),
        comparison: None,
        resources: None,
    }
}

/// Traite un fichier : présence, empreinte, lecture, recalcul, comparaison.
#[must_use]
pub fn run_file(entry: &Entry, files_dir: &Path) -> FileRun {
    let Ok(bytes) = std::fs::read(files_dir.join(&entry.name)) else {
        return absent(entry, AbsentReason::NotDownloaded);
    };
    let actual = manifest::sha256_hex(&bytes);
    if actual != entry.sha256 {
        return absent(entry, AbsentReason::HashMismatch { actual });
    }
    run_bytes(entry, &bytes)
}

/// Lecture, recalcul et comparaison d'un contenu déjà vérifié.
#[must_use]
pub fn run_bytes(entry: &Entry, bytes: &[u8]) -> FileRun {
    let now = crate::moment(CURRENT_DATE);
    let document = match plannercraft_mspdi::read(bytes, &ReadOptions::new(now)) {
        Ok(d) => d,
        Err(e) => {
            return FileRun {
                entry: entry.clone(),
                status: FileStatus::ReadError(format!("{} : {e}", e.code())),
                document: None,
                engine_diagnostics: Vec::new(),
                comparison: None,
                resources: None,
            };
        }
    };
    let schedule = plannercraft_core::schedule(&document.project);
    let comparison = compare::compare(&document, &schedule);
    let resources = resources::compare(&document, &schedule);
    let engine_diagnostics = schedule.diagnostics.iter().map(|d| d.code().to_owned()).collect();
    FileRun {
        entry: entry.clone(),
        status: FileStatus::Compared {
            cases: comparison.case_count(),
            identical: comparison.identical_count(),
            incomplete: comparison.incomplete_count(),
        },
        document: Some(document),
        engine_diagnostics,
        comparison: Some(comparison),
        resources: Some(resources),
    }
}
