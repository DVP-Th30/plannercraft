//! Session « headless » de PlannerCraft : projet ouvert, historique et registre de
//! commandes.
//!
//! Toute action (menu, raccourci, CLI, MCP, canal de contrôle) passe par le
//! même registre de commandes, identifiées par un id stable (`task.add`,
//! `link.add`…). Cette crate ne dépend d'aucune interface graphique
//! (règle 9, vérifiée par `cargo xtask layers`).
//!
//! J4 (ADR-0012, contrat L0) : [`session::Session`] (lot de commandes,
//! `dry_run`, annuler/rétablir), [`command`] (format versionné), [`registry`],
//! vues JSON, journal et [`workspace::Workspace`] communs à la CLI, au MCP
//! et au canal de contrôle.
//!
//! Modules livrés en J3 (ADR-0010) :
//! - [`text`] : texte des durées, des décalages, des prédécesseurs et des
//!   dates à la manière de MS Project, en français et en anglais (R-140 à
//!   R-143) ;
//! - [`csv`] : import et export des tâches en CSV, depuis et vers MS Project
//!   ou un tableur (R-144 à R-146).
//!
//! J8 (ADR-0016, contrat L0) : [`filter`] (filtre partagé, R-251),
//! [`helpers`] (raccourcis d'édition, R-252, R-253, R-255), [`clip`]
//! (presse-papiers de lignes, R-244, R-256), [`render_text`] (textes du
//! rendu, règle 3), [`command::style`] (style du document), [`pdf`] (export
//! PDF, lot L7).
//!
//! Aucune entrée/sortie hors de `files` et `control` (absents en wasm) : du
//! texte ou des octets en entrée, une valeur du modèle en sortie.

pub mod agent;
pub mod analysis;
pub mod clip;
pub mod command;
#[cfg(not(target_arch = "wasm32"))]
pub mod control;
pub mod csv;
mod delta;
mod diag_delta;
pub mod diff;
pub mod error;
pub mod explain;
pub mod field_change;
#[cfg(not(target_arch = "wasm32"))]
pub mod files;
pub mod filter;
pub mod gesture;
pub mod helpers;
pub mod history;
pub mod journal;
pub mod json;
pub mod outcome;
pub mod pdf;
/// Journal de reprise : dossiers et orphelins sur le desktop seulement ;
/// journal, rejeu et [`recovery::Recorder`] aussi sur le web (sauvegarde
/// automatique du navigateur, lot W1, ADR-0031).
pub mod recovery;
pub mod registry;
pub mod render_text;
pub mod session;
pub mod text;
pub mod timeline;
mod uid;
pub mod view;
pub mod workspace;

pub use command::{Batch, Command, Patch, Placement, TaskRef};
pub use diff::{ScheduleDiff, TaskChange};
pub use error::{CommandError, CommandErrorKind, HistoryError, OpenError};
pub use history::{EditSource, HistoryView};
pub use outcome::{ApplyOutcome, SideEffect};
pub use registry::{Availability, CommandSpec, Kind, REGISTRY};
pub use session::{
    AnchorSpec, ApplyMode, ApplyOptions, DocOrigin, NewProject, Session, SessionLimits, Simulation,
};
pub use workspace::{CallEnv, ToolError, Workspace, call_with};
