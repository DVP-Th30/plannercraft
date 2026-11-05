//! Vues : fonctions de dessin qui lisent un contexte d'image ([`FrameCtx`])
//! et poussent des `Action`, sans jamais modifier l'application.
//!
//! Chaque vue se teste seule avec un `FrameCtx` fabriqué à la main. Les vues
//! n'importent pas `app` : c'est `App` qui assemble l'image.

use std::collections::BTreeSet;
use std::ops::Range;

use plannercraft_automation::gesture::{GestureKind, Preview};
use plannercraft_core::{ResourceUid, Severity, TaskUid};

use crate::action::Action;
use crate::app::OpenDocument;
use crate::column::Column;
use crate::dialog::DialogState;
use crate::edit::TableEdit;
use crate::i18n::Texts;
use crate::nav::CellCursor;
use crate::row_view::{DisplayRow, FilterMemo, RowView, VisibleRows};
use crate::rows::RowModel;
use crate::selection::Selection;
use crate::state::{TaskViewState, UiState};
use crate::theme::Palette;

pub mod approval;
pub mod chrome;
pub mod details;
pub mod diagnostics;
pub mod format_panel;
pub mod gantt;
pub mod main_view;
pub mod recovery;
pub mod resource_sheet;
pub mod ribbon;
pub mod scene_paint;
pub mod search;
pub mod side_panel;
pub mod table;
pub mod task_view;
pub mod timeline;
pub mod timescale;
pub mod usage;

pub use approval::{ApprovalBanner, ApprovalSummary};

/// Hauteur fixe d'une ligne (table et Gantt), en points logiques.
pub const ROW_H: f32 = 22.0;

/// Hauteur de l'en-tête commun (deux niveaux d'échelle de temps).
pub const HEADER_H: f32 = 44.0;

/// Lignes dessinées en plus au-dessus et au-dessous de la fenêtre.
pub const OVERSCAN: usize = 2;

/// Mesures de la vue principale, en points logiques.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// Hauteur d'une ligne.
    pub row_h: f32,
    /// Hauteur de l'en-tête.
    pub header_h: f32,
    /// Marge horizontale du texte dans une cellule.
    pub pad_x: f32,
    /// Retrait du nom par niveau hiérarchique au-delà du premier.
    pub indent: f32,
    /// Largeur du séparateur table / Gantt.
    pub splitter_w: f32,
    /// Hauteur de la barre de défilement horizontale du Gantt.
    pub hbar_h: f32,
    /// Largeur réservée à droite du corps par la barre de défilement
    /// verticale pleine (`ScrollStyle::solid`) : le Gantt s'arrête avant.
    pub vbar_w: f32,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            row_h: ROW_H,
            header_h: HEADER_H,
            pad_x: 4.0,
            indent: 14.0,
            splitter_w: 5.0,
            hbar_h: 10.0,
            vbar_w: egui::style::ScrollStyle::solid().allocated_width(),
        }
    }
}

/// État transitoire de la vue : jamais sérialisé.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewTransient {
    /// Défilement vertical à imposer une seule fois (restauration,
    /// `ScrollToRow`, tests).
    pub pending_scroll_y: Option<f32>,
    /// Défilement horizontal du Gantt, en pixels depuis le début de la plage.
    pub gantt_scroll_x: f32,
    /// Accumulateur du zoom à la molette (somme de `ln(zoom_delta)`).
    pub zoom_accum: f32,
    /// Ajuster le zoom à la prochaine image (après une ouverture).
    pub needs_fit: bool,
    /// Largeur du Gantt à l'image précédente : référence des ancres de
    /// zoom (centre du Gantt) et de la borne du défilement horizontal.
    pub last_gantt_width: f32,
    /// Abscisse écran du bord gauche du Gantt à l'image précédente (relevé
    /// pour les tests ; le réducteur ne travaille qu'en abscisses relatives
    /// au Gantt).
    pub last_gantt_left: f32,
    /// Saisie en cours dans la table (J4, lot L12).
    pub edit: TableEdit,
    /// Cellule courante de la table (J7, lot L1) : sa ligne suit
    /// `App::selection` (la ligne vide sans sélection), sa colonne est
    /// posée par un clic, Tab ou les flèches.
    pub cursor: CellCursor<TaskUid, Column>,
    /// Volet qui a le focus clavier (J7, lot L1).
    pub focus: Pane,
    /// Donner le focus clavier à la table à la prochaine image (ouverture
    /// d'un document, J7 lot L1).
    pub focus_table_pending: bool,
    /// Défilement vertical de la table et du Gantt à l'image précédente
    /// (haut de la fenêtre visible, en points depuis la ligne 0) : base du
    /// suivi du curseur (H-UI-17).
    pub last_scroll_y: f32,
    /// Hauteur visible du corps à l'image précédente : pas d'une page et
    /// suivi du curseur (H-UI-15, H-UI-17).
    pub last_body_height: f32,
    /// Hauteur du contenu défilé à l'image précédente (borne du
    /// défilement imposé, connue d'egui avec une image de retard).
    pub last_content_h: f32,
    /// Défilement horizontal de la table, en points (J7, lot L6), borné à
    /// chaque image par la vue principale.
    pub table_scroll_x: f32,
    /// Largeur visible de la table à l'image précédente : borne du
    /// défilement horizontal et suivi du curseur (H-UI-17, J7 lot L6).
    pub last_table_width: f32,
    /// Glisser d'une barre en cours (J7, lot L8).
    pub drag: Option<BarDrag>,
    /// Fenêtre ouverte (J7, lots L8, L9, L12, L13).
    pub dialog: Option<DialogState>,
    /// Feuille des ressources : curseur, saisie, lignes, confirmation (J7,
    /// lot L10).
    pub resources: resource_sheet::ResourceSheetState,
    /// Ressources dépliées en affectations dans l'utilisation des
    /// ressources (J7, lot L11 ; repliées par défaut).
    pub expanded: BTreeSet<ResourceUid>,
    /// Utilisation des ressources : curseur, lignes, blocs de périodes du
    /// moteur (J7, lot L11).
    pub usage: usage::UsageState,
    /// Filtre, tri et niveau affiché de la vue des tâches (J8, lot L5) :
    /// une vue, rien d'écrit (P50).
    pub task_view: TaskViewState,
    /// Récapitulatives repliées (J8, lot L5, H-UI-34).
    pub collapsed: BTreeSet<TaskUid>,
    /// Version du repli : change à chaque changement de `collapsed` (clé du
    /// cache de `RowView`).
    pub collapse_version: u64,
    /// Version de réapplication du filtre (J8, lot L9, H-UI-30) : change à
    /// chaque application d'un filtre (clé du cache de `RowView`).
    pub reapply_version: u64,
    /// Lignes gardées visibles malgré le filtre jusqu'à sa réapplication
    /// (J8, lot L9, H-UI-30).
    pub filter_memo: FilterMemo,
    /// Barre de recherche ouverte (J8, lot L9, H-UI-33).
    pub search: Option<search::SearchState>,
    /// Fenêtre « Trier » ouverte (J8, lot L9, H-UI-31).
    pub sort_dialog: Option<crate::dialog::sort::SortDialog>,
    /// Fenêtre « Filtre » ouverte (J8, lot L9, H-UI-30).
    pub filter_dialog: Option<crate::dialog::filter::FilterDialog>,
    /// Volet de mise en forme des barres ouvert (J8, lot L15, H-UI-59).
    pub format_panel: Option<format_panel::FormatPanelState>,
    /// Onglet du ruban ouvert en surimpression, ruban replié (ADR-0017,
    /// H-UI-72).
    pub ribbon_overlay: Option<crate::ribbon::RibbonTab>,
    /// Chronologie : disposition en cache (ADR-0017, H-UI-78, lot L4).
    pub timeline: timeline::TimelineView,
    /// Glisser de sélection en cours dans la table, de l'appui au relâcher
    /// (ADR-0018, H-UI-88 à H-UI-90 ; tenu par le lot L1).
    pub select_drag: Option<SelectDrag>,
    /// Déplacement de lignes en cours par leur ID (ADR-0018, H-UI-94 ;
    /// tenu par le lot L3).
    pub row_drag: Option<RowDrag>,
    // --- Seconde passe UX (ADR-0019, contrat L0).
    /// Glisser d'un séparateur de volets en cours (H-UI-111, lot L4).
    pub split_drag: Option<crate::panes::SplitDrag>,
    /// Glisser d'un en-tête de colonne en cours (H-UI-116, lot L6).
    pub column_drag: Option<crate::column::ColumnDrag>,
    /// Sélecteur de champ ouvert (H-UI-114, lot L6).
    pub picker: Option<crate::field::FieldPicker>,
    /// Volet de détails : brouillon du formulaire (H-UI-112, lot L5).
    pub details: Option<details::DetailsTransient>,
    /// Volets : relevé de la dernière image et passage au clavier demandé
    /// par F6 (H-UI-111, lot L4).
    pub panes: main_view::PanesView,
    // --- Finitions avant v1.0.0 (ADR-0022, lot P2).
    /// Pinceau « Reproduire la mise en forme » armé (H-UI-128).
    pub painter: Option<crate::text_style::PainterState>,
}

/// Glisser de sélection en cours (ADR-0018, contrat L0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectDrag {
    /// Cible de l'appui qui l'a commencé.
    pub target: crate::action::PressTarget,
    /// Dernière cellule envoyée (`SelectOp::DragTo` n'est poussé que quand
    /// elle change).
    pub last: (crate::nav::RowRef<TaskUid>, Column),
    /// Le pointeur s'est éloigné du point d'appui d'au moins le seuil de
    /// glisser d'egui (`max_click_dist`) : tant que faux, aucun
    /// `SelectOp::DragTo` (un tremblement qui franchit une bordure reste la
    /// cellule appuyée, H-UI-88) ni défilement automatique (H-UI-91) ; une
    /// fois vrai, il le reste jusqu'au relâcher.
    pub armed: bool,
    /// Ouvert par un Ctrl/Cmd+appui (`PressMode::Toggle`) : jamais armé, le
    /// glisser ne remplace pas la multi-sélection par une plage (relecture
    /// finale, n° 6 ; blocs disjoints reportés après v1.0.0).
    pub toggle: bool,
}

/// Déplacement de lignes en cours (ADR-0018, contrat L0, H-UI-94).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RowDrag {
    /// Tâches déplacées (sélectionnées sans ancêtre sélectionné), dans
    /// l'ordre des lignes.
    pub uids: Vec<TaskUid>,
    /// Ligne affichée devant laquelle le trait d'insertion est dessiné
    /// (`None` : après la dernière ligne).
    pub insert_before: Option<DisplayRow>,
    /// Raison du refus du lâcher à cet endroit (curseur interdit,
    /// info-bulle), `None` si le lâcher est permis.
    pub refusal: Option<crate::i18n::TextId>,
}

/// Volet de la vue principale (J7).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pane {
    /// Table des tâches.
    #[default]
    Table,
    /// Diagramme de Gantt.
    Gantt,
}

/// Glisser d'une barre en cours (J7, lot L8) : l'interface ne garde qu'un
/// écart en jours entiers et l'aperçu calculé par
/// `plannercraft_automation::gesture::preview`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarDrag {
    /// Tâche glissée.
    pub uid: TaskUid,
    /// Geste.
    pub kind: GestureKind,
    /// Abscisse du début du glisser, relative au bord gauche du Gantt.
    pub origin_x: f32,
    /// Écart courant en jours entiers (`TimeAxis::days_for_dx`).
    pub days: i64,
    /// Ligne affichée de la barre d'une autre tâche sous le pointeur
    /// (tracer un lien, R-243) ; `None` ailleurs, même sur une autre ligne.
    pub over_row: Option<DisplayRow>,
    /// Aperçu pour `days`, recalculé seulement quand `days` change.
    pub preview: Option<Preview>,
}

/// Relevé d'une image (tests de synchronisation et de performance).
///
/// Chaque volet écrit son propre champ (`table::paint_body` pour
/// `table_rows`, `gantt::paint_body` pour `gantt_rows`) : comparer les deux
/// compare deux chemins de dessin distincts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    /// Lignes dessinées dans la table (écrit par `table::paint_body`).
    pub table_rows: Range<usize>,
    /// Lignes dessinées dans le Gantt (écrit par `gantt::paint_body`).
    pub gantt_rows: Range<usize>,
    /// Liens dessinés.
    pub links_painted: usize,
    /// Formes émises par la vue principale.
    pub shapes: usize,
}

/// Diagnostics du document prêts à afficher : triés par gravité et mis en
/// texte **une fois** par document et par langue (cache d'`App`), comptés
/// une fois. Aucune image ne refait ce travail proportionnel au nombre de
/// diagnostics (ADR-0011, décision 3).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiagnosticsView {
    /// Gravité et texte affiché, la plus grave d'abord (ordre d'émission à
    /// gravité égale : diagnostics d'import, puis de calcul).
    pub entries: Vec<(Severity, String)>,
    /// Nombre d'erreurs.
    pub errors: usize,
    /// Nombre d'avertissements.
    pub warnings: usize,
}

/// Rang d'affichage d'une gravité (la plus grave d'abord).
const fn severity_rank(severity: Severity) -> u8 {
    match severity {
        Severity::Error => 0,
        Severity::Warning => 1,
        Severity::Info => 2,
    }
}

/// Texte affichable d'un diagnostic : « → » (U+2192), absent des polices
/// d'egui, devient « -> » (voir `docs/ui-needs.md`, N5 : diagnostics non
/// traduits, mis en forme par le cœur).
fn displayable(text: &str) -> String {
    text.replace('\u{2192}', "->")
}

impl DiagnosticsView {
    /// Construit la vue des diagnostics d'import et de calcul d'un document
    /// (tri stable par gravité), traduits par code dans la langue de
    /// `texts` (N5, J8 lot L14).
    #[must_use]
    pub fn build(doc: &OpenDocument, texts: &crate::i18n::Texts) -> Self {
        let tr = crate::diag_text::DiagTexts::new(texts);
        let mut entries: Vec<(Severity, String)> = doc
            .import_diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.severity, displayable(&tr.import(diagnostic))))
            .chain(
                doc.schedule()
                    .diagnostics
                    .iter()
                    .map(|diagnostic| (diagnostic.severity(), displayable(&tr.core(diagnostic)))),
            )
            .collect();
        entries.sort_by_key(|(severity, _)| severity_rank(*severity));
        let count = |wanted: Severity| entries.iter().filter(|(severity, _)| *severity == wanted).count();
        let (errors, warnings) = (count(Severity::Error), count(Severity::Warning));
        Self {
            entries,
            errors,
            warnings,
        }
    }
}

/// Contexte d'une image : lectures partagées, file d'actions et relevé.
pub struct FrameCtx<'a> {
    /// Préférences d'interface.
    pub state: &'a UiState,
    /// Document ouvert, s'il y en a un.
    pub doc: Option<&'a OpenDocument>,
    /// Modèle de lignes du document (dans la langue courante, ordre des
    /// ID).
    pub rows: Option<&'a RowModel>,
    /// Vue des lignes (J8, lot L5) : lignes affichées, construite sur
    /// `rows` ; lue par [`FrameCtx::visible`].
    pub row_view: Option<&'a RowView>,
    /// Messages traduits.
    pub texts: &'a Texts,
    /// Couleurs du thème résolu.
    pub palette: &'a Palette,
    /// Mesures.
    pub metrics: &'a Metrics,
    /// Diagnostics du document, triés et mis en texte (vide sans document).
    pub diagnostics: &'a DiagnosticsView,
    /// Tâche sélectionnée.
    pub selection: Option<TaskUid>,
    /// Tâches sélectionnées (lot L14) ; `selection` en est la tâche active.
    pub selected: &'a Selection,
    /// File des actions de l'image.
    pub actions: &'a mut Vec<Action>,
    /// Relevé de l'image.
    pub stats: &'a mut FrameStats,
    /// Une fenêtre est ouverte (« Affecter des ressources », non modale,
    /// laisse la table cliquable) : aucune saisie de cellule ne s'ouvre, ses
    /// touches seraient retirées de l'image (relecture finale J7, F9).
    pub window_open: bool,
    /// Vue des tâches à cette image : filtre, tri, niveau, recherche,
    /// triangles de repli (J8, lot L9).
    pub task_view: task_view::TaskViewFrame,
}

impl<'a> FrameCtx<'a> {
    /// Lignes affichées (modèle lu à travers sa vue) ; `None` sans document.
    #[must_use]
    pub fn visible(&self) -> Option<VisibleRows<'a>> {
        self.rows
            .zip(self.row_view)
            .map(|(model, view)| VisibleRows::new(model, view))
    }
}
