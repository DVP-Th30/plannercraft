//! Lecture des fichiers binaires de Microsoft Project (`.mpp`), en lecture
//! seule (ADR-0036, lot M1).
//!
//! Un `.mpp` est un fichier composé (`[MS-CFB]`) dont les flux portent les
//! entités du projet (tâches, liens, calendriers, propriétés). Leur
//! encodage n'est décrit par aucune spécification publique : tout ce que ce
//! lecteur sait du format a été **observé** sur des fichiers enregistrés par
//! MS Project, comparés à un corrigé externe, et consigné fait par fait dans
//! `docs/formats/mpp.md` (clean-room, ADR-0036 D1). Mesure : `docs/plans/mpp-import.md`.
//!
//! Formats : MPP14 (Project 2010 à 2021 et Microsoft 365) lu ; MPP12
//! (Project 2007), MPP9 (2000-2003), MPP8 (98) reconnus et refusés.
//!
//! Sortie : le modèle du cœur ([`plannercraft_core::Project`]) avec les
//! **entrées** seules, comme le lecteur MSPDI ; les dates enregistrées par
//! MS Project vont dans [`Recorded`], jamais dans le projet (règle 3) : le
//! moteur recalcule, [`compare`] confronte.
//!
//! Aucune entrée/sortie : des octets en entrée. Compile en
//! `wasm32-unknown-unknown`.
#![forbid(unsafe_code)]

pub mod bytes;
pub mod calendar;
pub mod cfb;
pub mod compare;
pub mod detect;
pub mod diag;
pub mod keep;
pub mod layout;
pub mod project;
pub mod report;
pub mod resource;
pub mod rtf;
pub mod tables;
pub mod task;
pub mod time;

use std::collections::BTreeMap;

use plannercraft_core::{BuiltinCalendarNames, Moment, Project, TaskUid};
use serde::Serialize;

pub use compare::{Comparison, DateGap, compare};
pub use detect::{FileFormat, Layout};
pub use diag::{MppCode, MppDiagnostic};
pub use keep::{KeptDates, kept_dates};

/// Bornes de la lecture (entrée hostile, règle 10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Taille du fichier, en octets.
    pub max_bytes: usize,
    /// Taille d'un flux lu, en octets.
    pub max_stream_bytes: usize,
    /// Enregistrements lus par table (tâches, liens, calendriers).
    pub max_records: usize,
    /// Diagnostics gardés (au-delà : comptés).
    pub max_diagnostics: usize,
}

impl Default for Limits {
    /// 256 Mio de fichier, 64 Mio par flux, 1 000 000 d'enregistrements par
    /// table, 10 000 diagnostics.
    fn default() -> Self {
        Self {
            max_bytes: 256 * 1024 * 1024,
            max_stream_bytes: 64 * 1024 * 1024,
            max_records: 1_000_000,
            max_diagnostics: 10_000,
        }
    }
}

/// Options de lecture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadOptions {
    /// Nom du projet (nom du fichier, sans extension).
    pub name: String,
    /// « Maintenant », injecté (règle 5).
    pub current_date: Moment,
    /// Noms des calendriers fournis (calendrier de repli).
    pub calendar_names: BuiltinCalendarNames,
    /// Bornes.
    pub limits: Limits,
}

impl ReadOptions {
    /// Options par défaut, noms de calendriers français.
    #[must_use]
    pub fn new(name: String, current_date: Moment) -> Self {
        Self {
            name,
            current_date,
            calendar_names: BuiltinCalendarNames::default(),
            limits: Limits::default(),
        }
    }
}

/// Refus du fichier entier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MppError {
    /// Fichier plus grand que la borne.
    TooLarge,
    /// Pas un fichier composé (pas un `.mpp`).
    NotCompound,
    /// Fichier composé illisible (en-tête, chaîne de secteurs, répertoire).
    Corrupt,
    /// Fichier composé sans les dossiers d'un projet MS Project.
    NotProject,
    /// Format reconnu mais non pris en charge par ce lot.
    Unsupported(FileFormat),
    /// Version d'enregistrement absente ou illisible (`Props14`).
    UnknownLayout,
    /// Flux attendu absent ou illisible.
    MissingStream(&'static str),
    /// Enregistrements de tâche incohérents (R-292) : disposition non
    /// reconnue pour cette version d'enregistrement.
    LayoutUnrecognized {
        /// Version de l'application qui a enregistré le fichier.
        saved_by: String,
        /// Enregistrements en service.
        records: u32,
        /// Dont incohérents.
        inconsistent: u32,
    },
}

impl MppError {
    /// Code stable (`mpp.…`).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::TooLarge => "mpp.too_large",
            Self::NotCompound => "mpp.not_compound",
            Self::Corrupt => "mpp.corrupt",
            Self::NotProject => "mpp.not_project",
            Self::Unsupported(_) => "mpp.unsupported_version",
            Self::UnknownLayout => "mpp.unknown_layout",
            Self::MissingStream(_) => "mpp.missing_stream",
            Self::LayoutUnrecognized { .. } => "mpp.layout_unrecognized",
        }
    }

    /// Message pour l'utilisateur (`fr` : français, sinon anglais).
    #[must_use]
    pub fn message(&self, fr: bool) -> String {
        match (self, fr) {
            (Self::TooLarge, true) => "fichier trop volumineux (256 Mo au plus)".to_owned(),
            (Self::TooLarge, false) => "file too large (256 MB at most)".to_owned(),
            (Self::NotCompound, true) => {
                "ce n'est pas un fichier MS Project (.mpp) : conteneur de fichiers composés absent".to_owned()
            }
            (Self::NotCompound, false) => {
                "not an MS Project (.mpp) file: no compound file container".to_owned()
            }
            (Self::Corrupt, true) => "fichier .mpp endommagé : conteneur illisible".to_owned(),
            (Self::Corrupt, false) => "damaged .mpp file: unreadable container".to_owned(),
            (Self::NotProject, true) => {
                "fichier composé sans données MS Project (autre application Office ?)".to_owned()
            }
            (Self::NotProject, false) => {
                "compound file without MS Project data (another Office application?)".to_owned()
            }
            (Self::Unsupported(f), true) => format!(
                "format {} ({}) reconnu mais pas encore pris en charge : enregistrez le fichier depuis MS Project 2010 ou plus récent, ou en XML",
                f.label(),
                f.versions()
            ),
            (Self::Unsupported(f), false) => format!(
                "format {} ({}) recognised but not supported yet: save the file from MS Project 2010 or later, or as XML",
                f.label(),
                f.versions()
            ),
            (Self::UnknownLayout, true) => {
                "version d'enregistrement illisible : fichier MPP14 non pris en charge".to_owned()
            }
            (Self::UnknownLayout, false) => "unreadable save version: MPP14 file not supported".to_owned(),
            (Self::MissingStream(s), true) => {
                format!("fichier .mpp incomplet : flux {s} absent ou illisible")
            }
            (Self::MissingStream(s), false) => {
                format!("incomplete .mpp file: stream {s} missing or unreadable")
            }
            (
                Self::LayoutUnrecognized {
                    saved_by,
                    records,
                    inconsistent,
                },
                true,
            ) => format!(
                "disposition des enregistrements non reconnue pour un fichier enregistré par MS Project {saved_by} : {inconsistent} tâches incohérentes sur {records} ; rien n'est importé plutôt qu'un planning faux"
            ),
            (
                Self::LayoutUnrecognized {
                    saved_by,
                    records,
                    inconsistent,
                },
                false,
            ) => format!(
                "record layout not recognised for a file saved by MS Project {saved_by}: {inconsistent} inconsistent tasks out of {records}; nothing is imported rather than a wrong schedule"
            ),
        }
    }
}

impl std::fmt::Display for MppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message(true))
    }
}

impl std::error::Error for MppError {}

/// Dates d'une tâche telles qu'enregistrées par MS Project (calculées par
/// lui) : jamais dans le projet, seulement comparées (règle 3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct RecordedTask {
    /// Début enregistré.
    pub start: Option<Moment>,
    /// Fin enregistrée.
    pub finish: Option<Moment>,
    /// Le début ou la fin enregistré porte une fraction de minute
    /// (dixièmes non multiples de 10), tronquée à la lecture (R-283).
    pub sub_minute: bool,
}

/// Dates d'une affectation telles qu'enregistrées par MS Project.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct RecordedAssignment {
    /// Début enregistré.
    pub start: Option<Moment>,
    /// Fin enregistrée.
    pub finish: Option<Moment>,
}

/// Valeurs enregistrées.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Recorded {
    /// Début du projet enregistré.
    pub project_start: Option<Moment>,
    /// Fin du projet enregistrée.
    pub project_finish: Option<Moment>,
    /// Par tâche (UID), lignes vides exclues.
    pub tasks: BTreeMap<TaskUid, RecordedTask>,
    /// Par affectation (UID).
    pub assignments: BTreeMap<plannercraft_core::AssignmentUid, RecordedAssignment>,
}

/// Ce que le fichier contient et qui n'est pas repris : enregistrements
/// illisibles ou rattachés à rien (ressources et affectations : lues
/// depuis le lot M2, R-294, R-295).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct NotImported {
    /// Ressources (enregistrements de `TBkndRsc`, hors réservés et
    /// supprimés).
    pub resources: u32,
    /// Affectations (`TBkndAssn`).
    pub assignments: u32,
    /// Calendriers dérivés (calendriers de ressource) dont la base manque.
    pub resource_calendars: u32,
}

/// Provenance du fichier.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SourceInfo {
    /// Format.
    pub format: FileFormat,
    /// Disposition des enregistrements.
    pub layout: Layout,
    /// Version de l'application qui a enregistré le fichier (`Props14`,
    /// « 16,0,10730,20102 »).
    pub saved_by: String,
    /// Données non reprises.
    pub not_imported: NotImported,
    /// Diagnostics supprimés au-delà de la borne.
    pub dropped_diagnostics: u64,
    /// Enregistrements de tâche en service lus (R-292).
    pub task_records: u32,
    /// Dont incohérents (R-292).
    pub inconsistent_records: u32,
}

/// Fichier lu.
#[derive(Clone, Debug)]
pub struct MppDocument {
    /// Projet (entrées seules).
    pub project: Project,
    /// Dates enregistrées.
    pub recorded: Recorded,
    /// Provenance.
    pub source: SourceInfo,
    /// Diagnostics de la lecture, puis ceux de `Project::validate`.
    pub diagnostics: Vec<MppDiagnostic>,
}

/// Lit un fichier `.mpp`.
///
/// # Errors
///
/// [`MppError`] : fichier refusé en entier (pas un `.mpp`, endommagé,
/// version non prise en charge). Tout le reste devient un diagnostic.
pub fn read(bytes: &[u8], options: &ReadOptions) -> Result<MppDocument, MppError> {
    if bytes.len() > options.limits.max_bytes {
        return Err(MppError::TooLarge);
    }
    let container = cfb::Cfb::parse(bytes).map_err(|e| match e {
        cfb::CfbError::NotCompound => MppError::NotCompound,
        _ => MppError::Corrupt,
    })?;
    let found = detect::detect(&container, options.limits.max_stream_bytes)?;
    let mut sink = diag::Sink::new(options.limits.max_diagnostics);
    let built = project::build(&container, &found, options, &mut sink)?;
    let mut diagnostics = sink.items;
    diagnostics.extend(
        built
            .project
            .validate()
            .finish()
            .into_iter()
            .map(|d| MppDiagnostic::core(&d)),
    );
    Ok(MppDocument {
        project: built.project,
        recorded: built.recorded,
        source: SourceInfo {
            format: found.format,
            layout: found.layout,
            saved_by: found.saved_by,
            not_imported: built.not_imported,
            dropped_diagnostics: sink.dropped,
            task_records: built.task_records,
            inconsistent_records: built.inconsistent_records,
        },
        diagnostics,
    })
}
