//! Interface egui de PlannerCraft : table des tâches, Gantt, état d'interface
//! sérialisable.
//!
//! C'est la seule crate (avec les applications) autorisée à dépendre d'egui,
//! eframe, winit ou rfd (règle 9). Elle est partagée par l'application
//! desktop (`apps/plannercraft`), la version web (`apps/plannercraft-web`) et les tests
//! kittest : tous appellent [`ui`] avec un `&mut egui::Ui` (API d'egui 0.37)
//! et une [`App`] indépendante d'eframe. Le document est une session
//! d'édition de `plannercraft_automation` : toute modification passe par un lot de
//! commandes du registre ([`Action::Exec`]) ; aucune date n'est calculée ici
//! (règle 3), tout vient de `plannercraft_core`.

pub mod action;
pub mod app;
pub mod block;
pub mod cells;
pub mod clipboard;
pub mod column;
pub mod diag_text;
pub mod dialog;
pub mod document;
pub mod edit;
pub mod error_text;
pub mod field;
pub mod format;
pub mod geom;
pub mod grid;
pub mod i18n;
pub mod indicators;
pub mod io;
pub mod keymap;
pub mod nav;
pub mod notify;
pub mod panes;
pub mod query;
pub mod quit;
pub mod relay;
pub mod resource_rows;
pub mod ribbon;
pub mod row_view;
pub mod rows;
pub mod selection;
pub mod shell;
pub mod shift;
pub mod state;
pub mod structure;
pub mod task_form;
pub mod theme;
pub mod timeline_ops;
pub mod timephased;
pub mod timescale;
pub mod view;

pub use action::Action;
pub use app::{App, DocOrigin, DocumentObserver, EditOrigin, Env, OpenDocument};
pub use i18n::Language;
pub use io::{FileService, NoFileService, RfdFileService};
pub use shell::PlannerCraftApp;
pub use state::{GanttStyle, LanguagePref, RecentFile, ThemePref, UiState, ViewKind};
pub use timescale::TimeScaleZoom;

/// Projet de démonstration embarqué (construction d'une maison, MSPDI écrit à
/// la main) : bouton « Ouvrir l'exemple » et fixture des tests.
pub const DEMO_MSPDI: &[u8] = include_bytes!("../assets/demo/house.xml");

/// Dessine une image complète de l'application dans `ui` : la même fonction
/// sert au desktop, au web et aux tests (`Harness::build_ui_state`).
pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    app.show(ui);
}
