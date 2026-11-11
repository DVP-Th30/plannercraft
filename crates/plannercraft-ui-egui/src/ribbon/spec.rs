//! Contenu du ruban (H-UI-70, `docs/plans/ribbon.md` § 2.2, ADR-0017 D3) :
//! table [`RIBBON`] onglet → groupes → éléments, lue par le moteur de
//! dessin (`view::ribbon`, lot L2).
//!
//! Chaque élément porte une [`RibbonCmd`] : [`RibbonCmd::action`] et
//! [`RibbonCmd::enabled`] sont **les seules** sources de l'action poussée
//! et de l'activation, partagées avec l'entrée jumelle de la barre de
//! menus (libellé [`RibbonCmd::menu_label`]) ; aucune fonction n'est
//! dupliquée, chaque commande pousse une [`Action`] existante.
//!
//! Lot L0 : table, actions et prédicats posés. Lot L2 : `view::ribbon`
//! dessine la table ; les menus de `view::chrome` dessinent leurs entrées
//! jumelles avec ces mêmes fonctions ([`RibbonCmd::menu_path`] : chemin du
//! jumeau ; [`RibbonCmd::checked`] lit la tâche active).

use plannercraft_automation::helpers::PercentStep;
use plannercraft_render::style::BarRole;

use super::icon_data::Icon;
use super::{Enabled, RibbonOp, RibbonTab};
use crate::action::{
    ACTIVE_COLUMN, Action, CellOp, ColumnOp, DialogKind, FormatOp, PaneOp, SelectOp, ShiftStep, TaskCmd,
    TextStyleOp, TimelineOp,
};
use crate::clipboard::ClipboardOp;
use crate::i18n::TextId;
use crate::io::SaveKind;
use crate::keymap::{Binding, Shortcut, UiBinding, shortcut_for_os};
use crate::state::{GanttStyle, ViewKind};
use crate::structure::StructureOp;
use crate::view::FrameCtx;
use crate::view::side_panel::SideOp;
use crate::view::task_view::TaskViewOp;

/// Éléments au plus dans un groupe (règle 10 : plafond écrit).
pub const MAX_GROUP_ITEMS: usize = 64;

/// Taille et forme d'un élément (H-UI-71).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemSize {
    /// Grand bouton : icône 32 px, libellé dessous (deux lignes au plus).
    Large,
    /// Petit bouton : icône 16 px, libellé à droite, trois par colonne.
    Small,
    /// Bouton texte de la rangée 0 % … 100 % (libellé avec `$percent`).
    PercentRow,
    /// Liste déroulante (libellé + « ▾ ») : menu dessiné par le moteur.
    Dropdown,
    /// Case à cocher ou bascule (état reflété, H-UI-74).
    Check,
}

/// Élément du ruban.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemSpec {
    /// Taille et forme.
    pub size: ItemSize,
    /// Icône (aucune : rangée des pourcentages, cases).
    pub icon: Option<Icon>,
    /// Libellé (celui de l'entrée de menu quand il est identique).
    pub label: TextId,
    /// Phrase d'aide de l'info-bulle (H-UI-73).
    pub tip: TextId,
    /// Liaison de `KEYMAP` dont le raccourci est affiché (H-UI-73).
    pub binding: Option<Binding>,
    /// Commande.
    pub cmd: RibbonCmd,
}

/// Groupe nommé d'un onglet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupSpec {
    /// Nom du groupe (sous le groupe).
    pub label: TextId,
    /// Icône principale (groupe replié en bouton à menu, H-UI-75).
    pub icon: Icon,
    /// Éléments, dans l'ordre.
    pub items: &'static [ItemSpec],
    /// Rang de réduction quand la largeur manque (1 : réduit d'abord ;
    /// H-UI-75, de droite à gauche).
    pub shrink_rank: u8,
}

/// Commande d'un élément du ruban : une par élément distinct du § 2.2
/// (un même bouton présent sur deux onglets garde sa commande).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RibbonCmd {
    /// Accès rapide : Enregistrer.
    Save,
    /// Accès rapide : Annuler.
    Undo,
    /// Accès rapide : Rétablir.
    Redo,
    /// Liste des vues (Tâche → Affichage).
    ViewMenu,
    /// Diagramme de Gantt.
    GanttChart,
    /// Gantt de suivi.
    TrackingGantt,
    /// Feuille des ressources.
    ResourceSheet,
    /// Utilisation des ressources.
    ResourceUsage,
    /// Coller.
    Paste,
    /// Couper.
    Cut,
    /// Copier.
    Copy,
    /// Collage spécial…
    PasteSpecial,
    /// Mettre en forme les barres (type de la tâche active).
    FormatBars,
    /// Galerie des modèles de mise en forme.
    Gallery,
    /// Marquer n % achevé (R-252).
    MarkPercent(PercentStep),
    /// Lier les tâches.
    Link,
    /// Délier.
    Unlink,
    /// Indenter (abaisser).
    Indent,
    /// Désindenter (hausser).
    Outdent,
    /// Désactiver (bascule, R-255).
    Inactivate,
    /// Planifier manuellement (bascule, R-253).
    ManualSchedule,
    /// Insérer une tâche (Inser).
    InsertTask,
    /// Insérer une récapitulative (H-UI-83).
    InsertSummary,
    /// Insérer un jalon (H-UI-83).
    InsertMilestone,
    /// Informations sur la tâche.
    TaskInformation,
    /// Notes de la tâche (H-UI-84).
    TaskNotes,
    /// Ajouter à la chronologie (bascule, H-UI-79).
    AddToTimeline,
    /// Rechercher.
    Find,
    /// Défiler jusqu'à la tâche (H-UI-84).
    ScrollToTask,
    /// Affecter les ressources.
    AssignResources,
    /// Informations sur le projet.
    ProjectInformation,
    /// Modifier le temps de travail.
    WorkingTime,
    /// Définir la référence.
    SetBaseline,
    /// Mettre à jour le projet (« Replanifier »).
    UpdateProject,
    /// Mode impression.
    PrintMode,
    /// Mise en page…
    PageSetup,
    /// Exporter en PDF…
    ExportPdf,
    /// Liste Trier (Trier…, Par ID).
    SortMenu,
    /// Liste Mode plan (niveaux, tous, masquer, afficher).
    OutlineMenu,
    /// Liste Filtre (préréglages, personnalisé, effacer).
    FilterMenu,
    /// Liste Échelle de temps (Jour … Trimestre).
    TimescaleMenu,
    /// Zoom avant.
    ZoomIn,
    /// Zoom arrière.
    ZoomOut,
    /// Projet entier.
    EntireProject,
    /// Case Chronologie (H-UI-78).
    TimelineVisible,
    /// Case Volet Opérations IA.
    AiPanel,
    /// Case Assistant IA (ADR-0024, lot A1, H-UI-138) : volet latéral sur
    /// l'onglet Assistant.
    Assistant,
    /// Case Diagnostics.
    Diagnostics,
    /// Liste Thème.
    ThemeMenu,
    /// Liste Langue.
    LanguageMenu,
    /// Case Réduire le ruban (H-UI-72).
    CollapseRibbon,
    /// Liste Colonnes (une case par colonne).
    ColumnsMenu,
    /// Case Tâches critiques (surbrillance).
    CriticalTasks,
    /// Case Mois relatifs.
    RelativeMonths,
    /// Case Graphique seul.
    ChartOnly,
    // --- Passe UX (ADR-0018 D10, lot L4) : commandes des menus contextuels
    // et du menu Édition ; leurs éléments du ruban (groupe Édition) sont
    // posés par L5 dans [`RIBBON`].
    /// Remplir vers le bas (Ctrl/Cmd+D, R-258).
    FillDown,
    /// Effacer le contenu du bloc (Ctrl/Cmd+Suppr, R-259).
    ClearContents,
    /// Tout sélectionner (Ctrl/Cmd+A, H-UI-92).
    SelectAll,
    /// Supprimer la tâche (Suppr ; menus seulement : pas d'élément du
    /// ruban).
    DeleteTask,
    // --- Seconde passe UX (ADR-0019, contrat L0) : éléments du ruban et
    // jumeaux de menus posés par L10.
    /// Case Détails (Affichage → Fractionner, H-UI-112).
    DetailsPane,
    /// Insérer une colonne… (Format → Colonnes, H-UI-114).
    InsertColumn,
    /// Paramètres de champ… (Format → Colonnes, H-UI-115).
    FieldSettings,
    /// Champs personnalisés… (Projet → Propriétés, H-UI-118 ; fenêtre du
    /// lot L9).
    CustomFields,
    // --- Finitions avant v1.0.0 (ADR-0022, lot P1).
    /// Liste Décaler ▾ (Tâche → Planification, H-UI-121).
    ShiftMenu,
    /// Entrée de Décaler ▾ : pas prédéfini (R-267).
    Shift(ShiftStep),
    /// Entrée de Décaler ▾ : « Personnalisé… » (fenêtre de saisie).
    ShiftCustom,
    /// Inspecter : « Pourquoi cette date ? » sur la tâche active
    /// (Tâche → Tâches, H-UI-123).
    Inspect,
    /// Zoom → Tâches sélectionnées (Affichage → Zoom, H-UI-124).
    ZoomSelected,
    // --- Finitions avant v1.0.0 (ADR-0022, lot P2) : groupe Texte de
    // l'onglet Tâche (H-UI-127), jumeaux au menu Format.
    /// Gras (bascule, Ctrl/Cmd+B).
    Bold,
    /// Italique (bascule, Ctrl/Cmd+I depuis les finitions P4).
    Italic,
    /// Liste Couleur du texte ▾.
    TextColorMenu,
    /// Liste Surlignage ▾.
    HighlightMenu,
    /// Reproduire la mise en forme (pinceau, bascule, H-UI-128).
    FormatPainter,
    /// Effacer la mise en forme.
    ClearFormatting,
    // --- Finitions avant v1.0.0 (ADR-0022, lot P3).
    /// Liste Regrouper par ▾ (Affichage → Données, H-UI-129).
    GroupByMenu,
}

/// Pas de « marquer n % », dans l'ordre de la rangée (H-UI-35).
pub const PERCENT_STEPS: [(PercentStep, i64); 5] = [
    (PercentStep::P0, 0),
    (PercentStep::P25, 25),
    (PercentStep::P50, 50),
    (PercentStep::P75, 75),
    (PercentStep::P100, 100),
];

/// Valeur affichée d'un pas de pourcentage.
#[must_use]
pub const fn percent_value(step: PercentStep) -> i64 {
    match step {
        PercentStep::P0 => 0,
        PercentStep::P25 => 25,
        PercentStep::P50 => 50,
        PercentStep::P75 => 75,
        PercentStep::P100 => 100,
    }
}

impl RibbonCmd {
    /// Action poussée par un clic (celle du jumeau de menu) ; `None` pour
    /// une liste déroulante, dont le menu pousse lui-même ses actions.
    /// Les cases poussent la valeur inverse de leur état ([`Self::checked`]).
    #[must_use]
    pub fn action(self, fc: &FrameCtx<'_>) -> Option<Action> {
        Some(match self {
            Self::Save => Action::Save,
            Self::Undo => Action::Undo,
            Self::Redo => Action::Redo,
            Self::ViewMenu
            | Self::SortMenu
            | Self::OutlineMenu
            | Self::FilterMenu
            | Self::TimescaleMenu
            | Self::ThemeMenu
            | Self::LanguageMenu
            | Self::ColumnsMenu
            | Self::ShiftMenu
            | Self::TextColorMenu
            | Self::HighlightMenu
            | Self::GroupByMenu => return None,
            Self::Bold => Action::TextStyle(TextStyleOp::ToggleBold),
            Self::Italic => Action::TextStyle(TextStyleOp::ToggleItalic),
            Self::FormatPainter => Action::TextStyle(TextStyleOp::Painter),
            Self::ClearFormatting => Action::TextStyle(TextStyleOp::Clear),
            Self::Shift(step) => Action::TaskCmd(TaskCmd::Shift(step)),
            Self::ShiftCustom => Action::ShowDialog(DialogKind::Shift),
            Self::Inspect => Action::SidePanel(SideOp::Open(crate::view::side_panel::SideTab::WhyThisDate)),
            Self::ZoomSelected => Action::ZoomToSelection,
            // Fenêtre « Champs personnalisés » (lot L9, H-UI-118).
            Self::CustomFields => Action::ShowDialog(DialogKind::CustomFields),
            // Comme le menu Affichage : le style du Gantt ramène à la vue
            // Gantt ; le moteur pousse aussi `SetGanttStyle` (voir
            // [`Self::gantt_style`]).
            Self::GanttChart | Self::TrackingGantt => Action::SetView(ViewKind::Gantt),
            Self::ResourceSheet => Action::SetView(ViewKind::ResourceSheet),
            Self::ResourceUsage => Action::SetView(ViewKind::ResourceUsage),
            Self::Paste => Action::Clipboard(ClipboardOp::Paste),
            Self::Cut => Action::Clipboard(ClipboardOp::Cut),
            Self::Copy => Action::Clipboard(ClipboardOp::Copy),
            Self::PasteSpecial => Action::Clipboard(ClipboardOp::PasteSpecial),
            Self::FormatBars => format_bars_action(fc),
            Self::Gallery => Action::ShowDialog(DialogKind::Gallery),
            Self::MarkPercent(step) => Action::TaskCmd(TaskCmd::MarkPercent(step)),
            Self::Link => Action::Structure(StructureOp::Link),
            Self::Unlink => Action::Structure(StructureOp::Unlink),
            Self::Indent => Action::Structure(StructureOp::Indent),
            Self::Outdent => Action::Structure(StructureOp::Outdent),
            Self::Inactivate => Action::TaskCmd(TaskCmd::ToggleActive),
            Self::ManualSchedule => Action::TaskCmd(TaskCmd::ToggleMode),
            Self::InsertTask => Action::Structure(StructureOp::InsertBefore),
            Self::InsertSummary => Action::Structure(StructureOp::InsertSummary),
            Self::InsertMilestone => Action::Structure(StructureOp::InsertMilestone),
            Self::TaskInformation => Action::ShowTaskInformation,
            Self::TaskNotes => Action::ShowTaskNotes,
            Self::AddToTimeline => Action::Timeline(TimelineOp::ToggleSelection),
            Self::Find => Action::TaskView(TaskViewOp::OpenSearch),
            Self::ScrollToTask => Action::ScrollToTask,
            Self::AssignResources => Action::ShowDialog(DialogKind::AssignResources),
            Self::ProjectInformation => Action::ShowDialog(DialogKind::ProjectInformation),
            Self::WorkingTime => Action::ShowDialog(DialogKind::WorkingTime),
            Self::SetBaseline => Action::ShowDialog(DialogKind::Baseline),
            Self::UpdateProject => Action::ShowDialog(DialogKind::Reschedule),
            Self::PrintMode => Action::ShowDialog(DialogKind::PageSetup { print_mode: true }),
            Self::PageSetup => Action::ShowDialog(DialogKind::PageSetup { print_mode: false }),
            Self::ExportPdf => Action::Export(SaveKind::Pdf),
            Self::ZoomIn => Action::ZoomBy(1),
            Self::ZoomOut => Action::ZoomBy(-1),
            Self::EntireProject => Action::FitZoom,
            Self::TimelineVisible => Action::Timeline(TimelineOp::SetVisible(!fc.state.timeline_visible)),
            // Lot A5 (H-UI-161) : Opérations IA ouvre son onglet ou referme
            // le volet s'il y est ; Assistant IA est la bascule du volet
            // (bouton du ruban, Ctrl/Cmd+J).
            Self::AiPanel => {
                Action::SidePanel(SideOp::ToggleTab(crate::view::side_panel::SideTab::AiOperations))
            }
            Self::Assistant => Action::SidePanel(SideOp::Toggle),
            Self::Diagnostics => Action::ToggleDiagnostics,
            Self::CollapseRibbon => Action::Ribbon(RibbonOp::ToggleCollapsed),
            Self::CriticalTasks => Action::SetHighlightCritical(!fc.task_view.state.highlight_critical),
            Self::RelativeMonths => Action::Format(FormatOp::SetRelativeMonths(!relative_months(fc))),
            Self::ChartOnly => Action::SetChartOnly(!fc.state.chart_only),
            Self::FillDown => Action::Cells(CellOp::FillDown),
            Self::ClearContents => Action::Cells(CellOp::Clear),
            Self::SelectAll => Action::SelectOp(SelectOp::All),
            Self::DeleteTask => Action::Structure(StructureOp::Delete),
            Self::DetailsPane => Action::Pane(PaneOp::ShowDetails(!fc.state.details.visible)),
            Self::InsertColumn => Action::Column(ColumnOp::OpenPicker { at: ACTIVE_COLUMN }),
            Self::FieldSettings => Action::Column(ColumnOp::OpenSettings(ACTIVE_COLUMN)),
        })
    }

    /// Style du Gantt poussé avec [`Action::SetView`] par les éléments de
    /// vue Gantt (comme le menu Affichage).
    #[must_use]
    pub const fn gantt_style(self) -> Option<GanttStyle> {
        match self {
            Self::GanttChart => Some(GanttStyle::Standard),
            Self::TrackingGantt => Some(GanttStyle::Tracking),
            _ => None,
        }
    }

    /// Activation, partagée avec le jumeau de menu (H-UI-70) ; grisé :
    /// la raison affichée (H-UI-73). Prédicats des menus de J8 : document
    /// ouvert, sélection, historique, verrou du tri et du filtre (H-UI-32).
    #[must_use]
    pub fn enabled(self, fc: &FrameCtx<'_>) -> Enabled {
        let history = fc.doc.map(|doc| doc.session.history());
        match self {
            // Préférences d'affichage : actives sans document, comme leurs
            // cases du menu Affichage (J8).
            Self::ThemeMenu
            | Self::LanguageMenu
            | Self::CollapseRibbon
            | Self::AiPanel
            | Self::Assistant
            | Self::Diagnostics
            | Self::TimelineVisible
            | Self::DetailsPane
            | Self::CriticalTasks
            | Self::ChartOnly => return Enabled::YES,
            Self::Undo if history.as_ref().is_none_or(|h| h.entries.is_empty()) => {
                return Enabled::no(TextId::RibbonReasonNothingToUndo);
            }
            Self::Redo if history.as_ref().is_none_or(|h| h.redo.is_empty()) => {
                return Enabled::no(TextId::RibbonReasonNothingToRedo);
            }
            _ => {}
        }
        if fc.doc.is_none() {
            return Enabled::no(TextId::RibbonReasonNoDocument);
        }
        // Commandes des tâches hors du Gantt (vues des ressources) : le
        // réducteur les ignore ou viserait des tâches invisibles.
        if self.needs_task_view() && fc.state.view != ViewKind::Gantt {
            return Enabled::no(TextId::RibbonReasonTaskView);
        }
        // Regroupement (finitions P3, H-UI-130) : insérer et indenter
        // grisés, avec la raison.
        if fc.task_view.group_by.is_some()
            && matches!(
                self,
                Self::Indent | Self::Outdent | Self::InsertTask | Self::InsertSummary | Self::InsertMilestone
            )
        {
            return Enabled::no(TextId::ViewGroupLocked);
        }
        let locked = fc.task_view.state.reorder_locked();
        if locked
            && matches!(
                self,
                Self::Indent | Self::Outdent | Self::InsertSummary | Self::InsertMilestone
            )
        {
            return Enabled::no(TextId::ViewReorderLocked);
        }
        if self.needs_selection() && fc.selection.is_none() {
            return Enabled::no(TextId::RibbonReasonNoSelection);
        }
        Enabled::YES
    }

    /// Commande qui vise les tâches (structure, commandes de tâche,
    /// fenêtres d'une tâche, chronologie, défilement, presse-papiers) :
    /// grisée hors de la vue des tâches (Gantt), car les vues des
    /// ressources n'affichent pas la sélection des tâches.
    #[must_use]
    pub const fn needs_task_view(self) -> bool {
        matches!(
            self,
            Self::MarkPercent(_)
                | Self::Link
                | Self::Unlink
                | Self::Indent
                | Self::Outdent
                | Self::Inactivate
                | Self::ManualSchedule
                | Self::InsertTask
                | Self::InsertSummary
                | Self::InsertMilestone
                | Self::TaskInformation
                | Self::TaskNotes
                | Self::AddToTimeline
                | Self::ScrollToTask
                | Self::Cut
                | Self::Copy
                | Self::Paste
                | Self::PasteSpecial
                | Self::FillDown
                | Self::ClearContents
                | Self::SelectAll
                | Self::DeleteTask
                | Self::InsertColumn
                | Self::FieldSettings
                | Self::ShiftMenu
                | Self::Shift(_)
                | Self::ShiftCustom
                | Self::Inspect
                | Self::ZoomSelected
                | Self::Bold
                | Self::Italic
                | Self::TextColorMenu
                | Self::HighlightMenu
                | Self::FormatPainter
                | Self::ClearFormatting
                | Self::GroupByMenu
        )
    }

    /// Commande qui agit sur la sélection (grisée sans tâche active).
    #[must_use]
    pub const fn needs_selection(self) -> bool {
        matches!(
            self,
            Self::MarkPercent(_)
                | Self::Link
                | Self::Unlink
                | Self::Indent
                | Self::Outdent
                | Self::Inactivate
                | Self::ManualSchedule
                | Self::TaskInformation
                | Self::TaskNotes
                | Self::AddToTimeline
                | Self::ScrollToTask
                | Self::Cut
                | Self::Copy
                | Self::FillDown
                | Self::ClearContents
                | Self::DeleteTask
                | Self::ShiftMenu
                | Self::Shift(_)
                | Self::ShiftCustom
                | Self::Inspect
                | Self::ZoomSelected
                | Self::Bold
                | Self::Italic
                | Self::TextColorMenu
                | Self::HighlightMenu
                | Self::ClearFormatting
        )
    }

    /// État d'une case ou d'une bascule (H-UI-74) ; `None` pour un bouton.
    /// Les bascules de tâche (Désactiver, Planifier manuellement, Ajouter à
    /// la chronologie) lisent la **tâche active** (sélection mixte : son
    /// état). Volet Opérations IA : l'état du volet n'est pas dans
    /// [`FrameCtx`] ; `view::chrome::item_checked` le substitue (`false`
    /// ici).
    #[must_use]
    pub fn checked(self, fc: &FrameCtx<'_>) -> Option<bool> {
        match self {
            Self::TimelineVisible => Some(fc.state.timeline_visible),
            Self::DetailsPane => Some(fc.state.details.visible),
            Self::AiPanel | Self::Assistant => Some(false),
            Self::Inactivate => Some(active_task(fc).is_some_and(|task| !task.active)),
            Self::ManualSchedule => {
                Some(active_task(fc).is_some_and(|task| {
                    matches!(task.mode, plannercraft_core::SchedulingMode::Manual { .. })
                }))
            }
            Self::AddToTimeline => Some(on_timeline(fc)),
            Self::Diagnostics => Some(fc.state.diagnostics_open),
            Self::CollapseRibbon => Some(fc.state.ribbon.collapsed),
            Self::CriticalTasks => Some(fc.task_view.state.highlight_critical),
            Self::RelativeMonths => Some(relative_months(fc)),
            Self::ChartOnly => Some(fc.state.chart_only),
            // Finitions P2 (H-UI-127) : état de la tâche active.
            Self::Bold => Some(active_text(fc).bold),
            Self::Italic => Some(active_text(fc).italic),
            Self::FormatPainter => Some(fc.task_view.painter),
            _ => None,
        }
    }

    /// Libellé de l'entrée jumelle de la barre de menus (H-UI-70 ; jumeaux
    /// nouveaux ajoutés aux menus en L2).
    #[must_use]
    pub const fn menu_label(self) -> TextId {
        match self {
            Self::Save => TextId::MenuSave,
            Self::Undo => TextId::MenuUndo,
            Self::Redo => TextId::MenuRedo,
            Self::ViewMenu | Self::GanttChart => TextId::MenuGanttStandard,
            Self::TrackingGantt => TextId::MenuGanttTracking,
            Self::ResourceSheet => TextId::MenuViewResourceSheet,
            Self::ResourceUsage => TextId::MenuViewResourceUsage,
            Self::Paste => TextId::MenuPaste,
            Self::Cut => TextId::MenuCut,
            Self::Copy => TextId::MenuCopy,
            Self::PasteSpecial => TextId::MenuPasteSpecial,
            Self::FormatBars => TextId::MenuFormatBars,
            Self::Gallery => TextId::MenuGallery,
            Self::MarkPercent(_) => TextId::MenuMarkPercent,
            Self::Link => TextId::StructureLink,
            Self::Unlink => TextId::StructureUnlink,
            Self::Indent => TextId::StructureIndent,
            Self::Outdent => TextId::StructureOutdent,
            Self::Inactivate => TextId::MenuToggleActive,
            Self::ManualSchedule => TextId::MenuToggleMode,
            Self::InsertTask => TextId::StructureInsert,
            Self::InsertSummary => TextId::MenuInsertSummary,
            Self::InsertMilestone => TextId::MenuInsertMilestone,
            Self::TaskInformation => TextId::TaskInfoMenu,
            Self::TaskNotes => TextId::MenuTaskNotes,
            Self::AddToTimeline => TextId::TimelineAdd,
            Self::Find => TextId::MenuFind,
            Self::ScrollToTask => TextId::RibbonScrollToTask,
            Self::AssignResources => TextId::MenuAssignResources,
            Self::ProjectInformation => TextId::MenuProjectInfo,
            Self::WorkingTime => TextId::MenuWorkingTime,
            Self::SetBaseline => TextId::MenuBaseline,
            Self::UpdateProject => TextId::MenuReschedule,
            Self::PrintMode => TextId::MenuPrintMode,
            Self::PageSetup => TextId::MenuPageSetup,
            Self::ExportPdf => TextId::MenuExportPdf,
            // Listes de l'onglet Affichage : entrées à plat du menu
            // Affichage (`task_view::view_menu`) ; libellé de la première.
            Self::SortMenu => TextId::MenuSort,
            Self::OutlineMenu => TextId::MenuOutlineLevel,
            Self::FilterMenu => TextId::MenuFilter,
            Self::TimescaleMenu => TextId::MenuZoom,
            Self::ZoomIn => TextId::RibbonZoomIn,
            Self::ZoomOut => TextId::RibbonZoomOut,
            Self::EntireProject => TextId::ToolbarFit,
            Self::TimelineVisible => TextId::RibbonTimeline,
            Self::AiPanel => TextId::MenuAiPanel,
            Self::Assistant => TextId::MenuAssistant,
            Self::Diagnostics => TextId::MenuDiagnostics,
            Self::ThemeMenu => TextId::MenuTheme,
            Self::LanguageMenu => TextId::MenuLanguage,
            Self::CollapseRibbon => TextId::RibbonCollapse,
            Self::ColumnsMenu => TextId::RibbonColumns,
            Self::CriticalTasks => TextId::MenuHighlightCritical,
            Self::RelativeMonths => TextId::MenuRelativeMonths,
            Self::ChartOnly => TextId::MenuChartOnly,
            Self::FillDown => TextId::MenuFillDown,
            Self::ClearContents => TextId::MenuClearContents,
            Self::SelectAll => TextId::MenuSelectAll,
            Self::DeleteTask => TextId::StructureDelete,
            Self::DetailsPane => TextId::MenuDetailsPane,
            Self::InsertColumn => TextId::MenuInsertColumn,
            Self::FieldSettings => TextId::MenuFieldSettings,
            Self::CustomFields => TextId::MenuCustomFields,
            Self::ShiftMenu => TextId::RibbonShift,
            Self::Shift(step) => step.label(),
            Self::ShiftCustom => TextId::ShiftCustom,
            Self::Inspect => TextId::RibbonInspect,
            Self::ZoomSelected => TextId::RibbonZoomSelected,
            Self::Bold => TextId::RibbonBold,
            Self::Italic => TextId::RibbonItalic,
            Self::TextColorMenu => TextId::RibbonTextColor,
            Self::HighlightMenu => TextId::RibbonHighlight,
            Self::FormatPainter => TextId::RibbonFormatPainter,
            Self::ClearFormatting => TextId::RibbonClearFormatting,
            Self::GroupByMenu => TextId::MenuGroupBy,
        }
    }

    /// Chemin de l'entrée jumelle dans la barre de menus : menu, puis
    /// sous-menus éventuels (H-UI-70, colonne « Jumeau » du § 2.2).
    #[must_use]
    pub const fn menu_path(self) -> &'static [TextId] {
        match self {
            Self::Save | Self::PrintMode | Self::PageSetup => &[TextId::MenuFile],
            Self::ExportPdf => &[TextId::MenuFile, TextId::MenuExport],
            Self::Undo
            | Self::Redo
            | Self::Paste
            | Self::Cut
            | Self::Copy
            | Self::PasteSpecial
            | Self::Indent
            | Self::Outdent
            | Self::InsertTask
            | Self::Find
            | Self::ScrollToTask
            | Self::AssignResources
            | Self::FillDown
            | Self::ClearContents
            | Self::SelectAll
            | Self::DeleteTask => &[TextId::MenuEdit],
            Self::ZoomIn | Self::ZoomOut | Self::EntireProject | Self::ZoomSelected => {
                &[TextId::MenuView, TextId::MenuZoom]
            }
            // Listes : leur jumeau est le sous-menu (Zoom, Thème, Langue,
            // Colonnes) ou la première entrée à plat du menu.
            Self::TimescaleMenu
            | Self::ViewMenu
            | Self::GanttChart
            | Self::TrackingGantt
            | Self::ResourceSheet
            | Self::ResourceUsage
            | Self::SortMenu
            | Self::OutlineMenu
            | Self::FilterMenu
            | Self::TimelineVisible
            | Self::AiPanel
            | Self::Assistant
            | Self::Diagnostics
            | Self::ThemeMenu
            | Self::LanguageMenu
            | Self::CollapseRibbon
            | Self::CriticalTasks
            | Self::ChartOnly
            | Self::DetailsPane
            | Self::GroupByMenu => &[TextId::MenuView],
            Self::MarkPercent(_)
            | Self::InsertSummary
            | Self::InsertMilestone
            | Self::Link
            | Self::Unlink
            | Self::Inactivate
            | Self::ManualSchedule
            | Self::TaskInformation
            | Self::TaskNotes
            | Self::AddToTimeline
            | Self::ShiftMenu
            | Self::Inspect => &[TextId::MenuTask],
            Self::Shift(_) | Self::ShiftCustom => &[TextId::MenuTask, TextId::RibbonShift],
            Self::ProjectInformation
            | Self::WorkingTime
            | Self::SetBaseline
            | Self::UpdateProject
            | Self::CustomFields => &[TextId::MenuProject],
            Self::FormatBars
            | Self::Gallery
            | Self::RelativeMonths
            | Self::ColumnsMenu
            | Self::InsertColumn
            | Self::FieldSettings
            | Self::Bold
            | Self::Italic
            | Self::TextColorMenu
            | Self::HighlightMenu
            | Self::FormatPainter
            | Self::ClearFormatting => &[TextId::MenuFormat],
        }
    }

    /// Case à cocher dans la barre de menus (préférences d'affichage ;
    /// Gras et Italique, état de la tâche active, finitions P2) ; les
    /// bascules de tâche y restent des commandes (« Activer / désactiver »,
    /// « Basculer manuelle / automatique »).
    #[must_use]
    pub const fn menu_is_check(self) -> bool {
        matches!(
            self,
            Self::Bold
                | Self::Italic
                | Self::TimelineVisible
                | Self::DetailsPane
                | Self::AiPanel
                | Self::Assistant
                | Self::Diagnostics
                | Self::CollapseRibbon
                | Self::CriticalTasks
                | Self::RelativeMonths
                | Self::ChartOnly
        )
    }

    /// Vue courante (choix exclusif du menu Affichage et de la liste
    /// « Diagramme de Gantt ▾ ») ; faux sans document.
    #[must_use]
    pub fn view_selected(self, fc: &FrameCtx<'_>) -> bool {
        let view = fc.state.view;
        fc.doc.is_some()
            && match self {
                Self::GanttChart | Self::TrackingGantt => {
                    view == ViewKind::Gantt && self.gantt_style() == Some(fc.state.gantt_style)
                }
                Self::ResourceSheet => view == ViewKind::ResourceSheet,
                Self::ResourceUsage => view == ViewKind::ResourceUsage,
                _ => false,
            }
    }

    /// Liaison de [`crate::keymap::KEYMAP`] dont le raccourci est affiché
    /// par le ruban, la barre de menus et les menus contextuels (H-UI-73,
    /// H-UI-100 : même raccourci partout ; un test vérifie que chaque
    /// élément de [`RIBBON`] qui en affiche un affiche celui-ci).
    #[must_use]
    pub const fn binding(self) -> Option<Binding> {
        Some(match self {
            Self::Save => command("project.save"),
            Self::Undo => command("history.undo"),
            Self::Redo => command("history.redo"),
            Self::Link => command("link.add"),
            Self::Unlink => command("link.remove"),
            Self::Indent => command("task.indent"),
            Self::Outdent => command("task.outdent"),
            Self::InsertTask => command("task.add"),
            Self::DeleteTask => command("task.delete"),
            Self::TaskInformation => ui(UiBinding::TaskInformation),
            Self::Find => ui(UiBinding::Find),
            Self::ZoomIn => ui(UiBinding::ZoomFiner),
            Self::ZoomOut => ui(UiBinding::ZoomCoarser),
            Self::CollapseRibbon => ui(UiBinding::ToggleRibbon),
            Self::FillDown => ui(UiBinding::FillDown),
            Self::ClearContents => ui(UiBinding::ClearCells),
            Self::SelectAll => ui(UiBinding::SelectAll),
            Self::FieldSettings => ui(UiBinding::FieldSettings),
            Self::Bold => ui(UiBinding::ToggleBold),
            Self::Italic => ui(UiBinding::ToggleItalic),
            _ => return None,
        })
    }

    /// Raccourci affiché pour le système `os` : celui de [`Self::binding`],
    /// sinon Ctrl/Cmd+X, C, V pour Couper, Copier, Coller (événements du
    /// presse-papiers livrés par egui, hors de `KEYMAP`, H-UI-39).
    #[must_use]
    pub fn shortcut(self, os: egui::os::OperatingSystem) -> Option<Shortcut> {
        if let Some(shortcut) = self.binding().and_then(|binding| shortcut_for_os(binding, os)) {
            return Some(*shortcut);
        }
        let key = match self {
            Self::Cut => egui::Key::X,
            Self::Copy => egui::Key::C,
            Self::Paste => egui::Key::V,
            _ => return None,
        };
        Some(Shortcut::new(egui::Modifiers::COMMAND, key))
    }

    /// Libellé dans un menu contextuel (H-UI-100) : celui de l'élément du
    /// ruban pour les cases (« Désactiver », « Planifier manuellement ») et
    /// pour le menu de l'en-tête de l'échelle (« Échelle de temps »,
    /// « Projet entier ») ; celui du jumeau de la barre de menus sinon.
    #[must_use]
    pub const fn context_label(self) -> TextId {
        match self {
            Self::Inactivate => TextId::RibbonInactivate,
            Self::ManualSchedule => TextId::RibbonManualSchedule,
            Self::TimescaleMenu => TextId::RibbonTimescale,
            Self::EntireProject => TextId::RibbonEntireProject,
            _ => self.menu_label(),
        }
    }

    /// Libellé de l'entrée de la liste « Remplir ▾ » ou « Effacer ▾ » du
    /// groupe Édition (passe UX, lot L5) : « Vers le bas », « Contenu » ;
    /// `None` pour une commande qui n'est pas une telle liste. L'entrée
    /// pousse [`Self::action`] et affiche [`Self::shortcut`].
    #[must_use]
    pub const fn dropdown_entry(self) -> Option<TextId> {
        match self {
            Self::FillDown => Some(TextId::RibbonFillDown),
            Self::ClearContents => Some(TextId::RibbonClearContents),
            _ => None,
        }
    }

    /// Élément dessiné en icône seule à tous les paliers (groupe Texte,
    /// finitions P2, H-UI-127 : petits boutons empilés, comme le groupe
    /// Police de MS Project) ; libellé dans l'infobulle et le nom
    /// accessible.
    #[must_use]
    pub const fn icon_only(self) -> bool {
        matches!(
            self,
            Self::Bold
                | Self::Italic
                | Self::TextColorMenu
                | Self::HighlightMenu
                | Self::FormatPainter
                | Self::ClearFormatting
        )
    }

    /// Petit élément posé sur la même rangée que le précédent (finitions P4,
    /// H-UI-133) : Délier à côté de Lier, Désindenter à côté d'Indenter,
    /// sous la rangée des pourcentages, pour que Planification tienne en
    /// deux colonnes.
    #[must_use]
    pub const fn joins_previous(self) -> bool {
        matches!(self, Self::Unlink | Self::Outdent)
    }

    /// Case cochable dans un menu contextuel (H-UI-74 : état de la tâche
    /// active).
    #[must_use]
    pub const fn context_is_check(self) -> bool {
        matches!(self, Self::Inactivate | Self::ManualSchedule)
    }
}

/// Menu contextuel de la table et du Gantt (H-UI-100, ADR-0018 D10) :
/// groupes séparés par un trait, dans l'ordre ; chaque entrée est la
/// [`RibbonCmd`] de son élément du ruban (même action, même prédicat, même
/// raccourci). « Mettre en forme les barres » le précède sur une barre.
pub const CONTEXT_MENU: &[&[RibbonCmd]] = &[
    &[
        RibbonCmd::Cut,
        RibbonCmd::Copy,
        RibbonCmd::Paste,
        RibbonCmd::PasteSpecial,
    ],
    &[RibbonCmd::FillDown, RibbonCmd::ClearContents],
    &[RibbonCmd::InsertTask, RibbonCmd::DeleteTask],
    &[RibbonCmd::Indent, RibbonCmd::Outdent],
    &[RibbonCmd::Link, RibbonCmd::Unlink],
    &[RibbonCmd::Inactivate, RibbonCmd::ManualSchedule],
    &[
        RibbonCmd::TaskInformation,
        RibbonCmd::TaskNotes,
        RibbonCmd::AssignResources,
        RibbonCmd::ScrollToTask,
        RibbonCmd::AddToTimeline,
    ],
];

/// Menu contextuel de l'en-tête de l'échelle de temps (H-UI-100) : les
/// commandes du groupe Zoom de l'onglet Affichage.
pub const TIMESCALE_MENU: &[RibbonCmd] = &[
    RibbonCmd::TimescaleMenu,
    RibbonCmd::ZoomIn,
    RibbonCmd::ZoomOut,
    RibbonCmd::EntireProject,
];

/// Tâche active du document (H-UI-74), si elle existe encore.
fn active_task<'a>(fc: &FrameCtx<'a>) -> Option<&'a plannercraft_core::Task> {
    let uid = fc.selection?;
    fc.doc?.project().tasks.get(uid)
}

/// Mise en forme du texte de la tâche active (style résolu de l'image,
/// finitions P2) ; vide sans tâche active.
fn active_text(fc: &FrameCtx<'_>) -> plannercraft_render::style::TaskText {
    match (fc.doc, fc.selection) {
        (Some(_), Some(uid)) => fc
            .task_view
            .style
            .style
            .task_text
            .get(&uid.get())
            .copied()
            .unwrap_or_default(),
        _ => plannercraft_render::style::TaskText::default(),
    }
}

/// La tâche active est sur la chronologie (liste du style du document).
fn on_timeline(fc: &FrameCtx<'_>) -> bool {
    let (Some(_), Some(uid)) = (fc.doc, fc.selection) else {
        return false;
    };
    // Style résolu de l'image (comme le menu contextuel) : pas de relecture
    // du JSON du document à chaque image.
    fc.task_view.style.style.timeline.tasks.contains(&uid.get())
}

/// « Mois relatifs » du style effectif du document (faux sans document).
fn relative_months(fc: &FrameCtx<'_>) -> bool {
    fc.doc.is_some() && fc.task_view.style.style.scale.relative_months
}

/// « Mettre en forme les barres » : type de la barre de la tâche active,
/// sinon les barres de tâches (même règle que le menu Format, que L2 fait
/// passer par cette fonction).
fn format_bars_action(fc: &FrameCtx<'_>) -> Action {
    let role = fc
        .selection
        .and_then(|uid| {
            let rows = fc.visible()?;
            let row = (0..rows.len()).find_map(|i| {
                rows.row(crate::row_view::DisplayRow::at(i))
                    .filter(|r| r.uid == uid)
            })?;
            crate::view::gantt::row_role(row, fc.task_view.state.highlight_critical)
        })
        .unwrap_or(BarRole::Task);
    Action::Format(FormatOp::OpenBars {
        role,
        task: fc.selection,
    })
}

impl RibbonTab {
    /// Libellé de l'onglet (S80 : Task, Resource, Project, View, Format).
    #[must_use]
    pub const fn label(self) -> TextId {
        match self {
            Self::Task => TextId::RibbonTabTask,
            Self::Resource => TextId::RibbonTabResource,
            Self::Project => TextId::RibbonTabProject,
            Self::View => TextId::RibbonTabView,
            Self::Format => TextId::RibbonTabFormat,
        }
    }

    /// Groupes de l'onglet, dans l'ordre ([`RIBBON`]).
    #[must_use]
    pub fn groups(self) -> &'static [GroupSpec] {
        RIBBON
            .iter()
            .find(|(tab, _)| *tab == self)
            .map_or(&[], |(_, groups)| *groups)
    }
}

/// Élément d'une ligne de la table.
const fn item(
    size: ItemSize,
    icon: Option<Icon>,
    label: TextId,
    tip: TextId,
    binding: Option<Binding>,
    cmd: RibbonCmd,
) -> ItemSpec {
    ItemSpec {
        size,
        icon,
        label,
        tip,
        binding,
        cmd,
    }
}

/// Élément sans raccourci.
const fn plain(size: ItemSize, icon: Option<Icon>, label: TextId, tip: TextId, cmd: RibbonCmd) -> ItemSpec {
    item(size, icon, label, tip, None, cmd)
}

/// Liaison d'une commande du registre.
const fn command(id: &'static str) -> Binding {
    Binding::Command(id)
}

/// Liaison propre à l'interface.
const fn ui(binding: UiBinding) -> Binding {
    Binding::Ui(binding)
}

use ItemSize::{Check, Dropdown, Large, PercentRow, Small};

/// Barre d'accès rapide (H-UI-76) : icônes seules, à gauche des onglets.
pub const QUICK_ACCESS: &[ItemSpec] = &[
    item(
        Small,
        Some(Icon::Save),
        TextId::MenuSave,
        TextId::RibbonTipSave,
        Some(command("project.save")),
        RibbonCmd::Save,
    ),
    item(
        Small,
        Some(Icon::Undo2),
        TextId::MenuUndo,
        TextId::RibbonTipUndo,
        Some(command("history.undo")),
        RibbonCmd::Undo,
    ),
    item(
        Small,
        Some(Icon::Redo2),
        TextId::MenuRedo,
        TextId::RibbonTipRedo,
        Some(command("history.redo")),
        RibbonCmd::Redo,
    ),
];

/// Vues du Gantt et des ressources (onglets Tâche, Ressource, Affichage).
const GANTT_CHART: ItemSpec = plain(
    Large,
    Some(Icon::ChartGantt),
    TextId::MenuGanttStandard,
    TextId::RibbonTipGanttChart,
    RibbonCmd::GanttChart,
);
const TRACKING_GANTT: ItemSpec = plain(
    Large,
    Some(Icon::ChartNoAxesGantt),
    TextId::MenuGanttTracking,
    TextId::RibbonTipTrackingGantt,
    RibbonCmd::TrackingGantt,
);
const RESOURCE_SHEET_LARGE: ItemSpec = plain(
    Large,
    Some(Icon::Users),
    TextId::MenuViewResourceSheet,
    TextId::RibbonTipResourceSheet,
    RibbonCmd::ResourceSheet,
);
const RESOURCE_USAGE_LARGE: ItemSpec = plain(
    Large,
    Some(Icon::ChartColumn),
    TextId::MenuViewResourceUsage,
    TextId::RibbonTipResourceUsage,
    RibbonCmd::ResourceUsage,
);
const FORMAT_BARS: ItemSpec = plain(
    Large,
    Some(Icon::Paintbrush),
    TextId::MenuFormatBars,
    TextId::RibbonTipFormatBars,
    RibbonCmd::FormatBars,
);

/// Pourcentage de la rangée.
const fn percent(step: PercentStep) -> ItemSpec {
    plain(
        PercentRow,
        None,
        TextId::RibbonPercent,
        TextId::MenuMarkPercent,
        RibbonCmd::MarkPercent(step),
    )
}

/// Onglet Tâche.
const TASK_GROUPS: &[GroupSpec] = &[
    GroupSpec {
        label: TextId::RibbonGroupView,
        icon: Icon::ChartGantt,
        items: &[plain(
            Dropdown,
            Some(Icon::ChartGantt),
            TextId::MenuGanttStandard,
            TextId::RibbonTipViewMenu,
            RibbonCmd::ViewMenu,
        )],
        shrink_rank: 9,
    },
    GroupSpec {
        label: TextId::RibbonGroupClipboard,
        icon: Icon::ClipboardPaste,
        items: &[
            plain(
                Large,
                Some(Icon::ClipboardPaste),
                TextId::MenuPaste,
                TextId::RibbonTipPaste,
                RibbonCmd::Paste,
            ),
            plain(
                Small,
                Some(Icon::Scissors),
                TextId::MenuCut,
                TextId::RibbonTipCut,
                RibbonCmd::Cut,
            ),
            plain(
                Small,
                Some(Icon::Copy),
                TextId::MenuCopy,
                TextId::RibbonTipCopy,
                RibbonCmd::Copy,
            ),
            plain(
                Small,
                Some(Icon::ClipboardList),
                TextId::MenuPasteSpecial,
                TextId::RibbonTipPasteSpecial,
                RibbonCmd::PasteSpecial,
            ),
        ],
        shrink_rank: 8,
    },
    // Finitions P2 (H-UI-127) : groupe Texte, icônes seules empilées.
    GroupSpec {
        label: TextId::RibbonGroupText,
        icon: Icon::Bold,
        items: &[
            item(
                Check,
                Some(Icon::Bold),
                TextId::RibbonBold,
                TextId::RibbonTipBold,
                Some(ui(UiBinding::ToggleBold)),
                RibbonCmd::Bold,
            ),
            item(
                Check,
                Some(Icon::Italic),
                TextId::RibbonItalic,
                TextId::RibbonTipItalic,
                Some(ui(UiBinding::ToggleItalic)),
                RibbonCmd::Italic,
            ),
            plain(
                Dropdown,
                Some(Icon::Baseline),
                TextId::RibbonTextColor,
                TextId::RibbonTipTextColor,
                RibbonCmd::TextColorMenu,
            ),
            plain(
                Dropdown,
                Some(Icon::Highlighter),
                TextId::RibbonHighlight,
                TextId::RibbonTipHighlight,
                RibbonCmd::HighlightMenu,
            ),
            plain(
                Check,
                Some(Icon::PaintbrushVertical),
                TextId::RibbonFormatPainter,
                TextId::RibbonTipFormatPainter,
                RibbonCmd::FormatPainter,
            ),
            plain(
                Small,
                Some(Icon::RemoveFormatting),
                TextId::RibbonClearFormatting,
                TextId::RibbonTipClearFormatting,
                RibbonCmd::ClearFormatting,
            ),
        ],
        shrink_rank: 7,
    },
    GroupSpec {
        label: TextId::RibbonGroupFormatting,
        icon: Icon::Paintbrush,
        items: &[
            FORMAT_BARS,
            plain(
                Small,
                Some(Icon::SwatchBook),
                TextId::RibbonGalleryShort,
                TextId::RibbonTipGallery,
                RibbonCmd::Gallery,
            ),
        ],
        shrink_rank: 2,
    },
    GroupSpec {
        label: TextId::RibbonGroupSchedule,
        icon: Icon::Link,
        items: &[
            percent(PercentStep::P0),
            percent(PercentStep::P25),
            percent(PercentStep::P50),
            percent(PercentStep::P75),
            percent(PercentStep::P100),
            item(
                Small,
                Some(Icon::Link),
                TextId::RibbonLink,
                TextId::RibbonTipLink,
                Some(command("link.add")),
                RibbonCmd::Link,
            ),
            item(
                Small,
                Some(Icon::Unlink),
                TextId::RibbonUnlink,
                TextId::RibbonTipUnlink,
                Some(command("link.remove")),
                RibbonCmd::Unlink,
            ),
            item(
                Small,
                Some(Icon::IndentIncrease),
                TextId::RibbonIndent,
                TextId::RibbonTipIndent,
                Some(command("task.indent")),
                RibbonCmd::Indent,
            ),
            item(
                Small,
                Some(Icon::IndentDecrease),
                TextId::RibbonOutdent,
                TextId::RibbonTipOutdent,
                Some(command("task.outdent")),
                RibbonCmd::Outdent,
            ),
            plain(
                Check,
                Some(Icon::CircleOff),
                TextId::RibbonInactivate,
                TextId::RibbonTipInactivate,
                RibbonCmd::Inactivate,
            ),
            // Finitions P1 (H-UI-121, R-267) : Décaler ▾.
            plain(
                Dropdown,
                Some(Icon::MoveHorizontal),
                TextId::RibbonShift,
                TextId::RibbonTipShift,
                RibbonCmd::ShiftMenu,
            ),
        ],
        shrink_rank: 6,
    },
    GroupSpec {
        label: TextId::RibbonGroupTasks,
        icon: Icon::Pin,
        items: &[
            plain(
                Large,
                Some(Icon::Pin),
                TextId::RibbonManualSchedule,
                TextId::RibbonTipManualSchedule,
                RibbonCmd::ManualSchedule,
            ),
            // Finitions P1 (H-UI-123) : Inspecter.
            plain(
                Small,
                Some(Icon::ScanSearch),
                TextId::RibbonInspect,
                TextId::RibbonTipInspect,
                RibbonCmd::Inspect,
            ),
        ],
        shrink_rank: 3,
    },
    GroupSpec {
        label: TextId::RibbonGroupInsert,
        icon: Icon::ListPlus,
        items: &[
            item(
                Large,
                Some(Icon::ListPlus),
                TextId::RibbonInsertTask,
                TextId::RibbonTipInsertTask,
                Some(command("task.add")),
                RibbonCmd::InsertTask,
            ),
            plain(
                Small,
                Some(Icon::ListTree),
                TextId::RibbonInsertSummary,
                TextId::RibbonTipInsertSummary,
                RibbonCmd::InsertSummary,
            ),
            plain(
                Small,
                Some(Icon::Diamond),
                TextId::RibbonInsertMilestone,
                TextId::RibbonTipInsertMilestone,
                RibbonCmd::InsertMilestone,
            ),
        ],
        shrink_rank: 5,
    },
    GroupSpec {
        label: TextId::RibbonGroupProperties,
        icon: Icon::Info,
        items: &[
            item(
                Small,
                Some(Icon::Info),
                TextId::RibbonInformation,
                TextId::RibbonTipInformation,
                Some(ui(UiBinding::TaskInformation)),
                RibbonCmd::TaskInformation,
            ),
            plain(
                Small,
                Some(Icon::NotebookPen),
                TextId::RibbonNotes,
                TextId::RibbonTipNotes,
                RibbonCmd::TaskNotes,
            ),
            plain(
                Check,
                Some(Icon::Timeline),
                // Finitions P4 (H-UI-133) : libellé court de la bascule.
                TextId::RibbonAddToTimeline,
                TextId::RibbonTipAddToTimeline,
                RibbonCmd::AddToTimeline,
            ),
        ],
        shrink_rank: 4,
    },
    GroupSpec {
        label: TextId::RibbonGroupEditing,
        icon: Icon::Search,
        items: &[
            item(
                Large,
                Some(Icon::Search),
                TextId::RibbonFind,
                TextId::RibbonTipFind,
                Some(ui(UiBinding::Find)),
                RibbonCmd::Find,
            ),
            plain(
                Small,
                Some(Icon::Locate),
                TextId::RibbonScrollToTask,
                TextId::RibbonTipScrollToTask,
                RibbonCmd::ScrollToTask,
            ),
            // Passe UX (ADR-0018 D10, lot L5) : Remplir ▾ (Vers le bas) et
            // Effacer ▾ (Contenu), listes à une entrée comme le groupe
            // Editing de Project (S80) ; l'entrée est le jumeau du menu
            // Édition ([`RibbonCmd::dropdown_entry`]).
            item(
                Dropdown,
                Some(Icon::ArrowDownToLine),
                TextId::RibbonFill,
                TextId::RibbonTipFill,
                Some(ui(UiBinding::FillDown)),
                RibbonCmd::FillDown,
            ),
            item(
                Dropdown,
                Some(Icon::Eraser),
                TextId::RibbonClear,
                TextId::RibbonTipClear,
                Some(ui(UiBinding::ClearCells)),
                RibbonCmd::ClearContents,
            ),
        ],
        shrink_rank: 1,
    },
];

/// Onglet Ressource.
const RESOURCE_GROUPS: &[GroupSpec] = &[
    GroupSpec {
        label: TextId::RibbonGroupView,
        icon: Icon::Users,
        items: &[RESOURCE_SHEET_LARGE, RESOURCE_USAGE_LARGE],
        shrink_rank: 2,
    },
    GroupSpec {
        label: TextId::RibbonGroupAssignments,
        icon: Icon::UserPlus,
        items: &[plain(
            Large,
            Some(Icon::UserPlus),
            TextId::MenuAssignResources,
            TextId::RibbonTipAssignResources,
            RibbonCmd::AssignResources,
        )],
        shrink_rank: 1,
    },
];

/// Onglet Projet.
const PROJECT_GROUPS: &[GroupSpec] = &[
    GroupSpec {
        label: TextId::RibbonGroupProperties,
        icon: Icon::FileText,
        items: &[
            plain(
                Large,
                Some(Icon::FileText),
                TextId::MenuProjectInfo,
                TextId::RibbonTipProjectInformation,
                RibbonCmd::ProjectInformation,
            ),
            // Seconde passe UX (L10, H-UI-118) : Champs personnalisés,
            // groupe Propriétés comme Project.
            plain(
                Large,
                Some(Icon::Tags),
                TextId::RibbonCustomFields,
                TextId::RibbonTipCustomFields,
                RibbonCmd::CustomFields,
            ),
            plain(
                Large,
                Some(Icon::CalendarCog),
                TextId::MenuWorkingTime,
                TextId::RibbonTipWorkingTime,
                RibbonCmd::WorkingTime,
            ),
        ],
        shrink_rank: 4,
    },
    GroupSpec {
        label: TextId::RibbonGroupSchedule,
        icon: Icon::CalendarCheck,
        items: &[plain(
            Large,
            Some(Icon::CalendarCheck),
            TextId::RibbonSetBaseline,
            TextId::RibbonTipSetBaseline,
            RibbonCmd::SetBaseline,
        )],
        shrink_rank: 3,
    },
    GroupSpec {
        label: TextId::RibbonGroupStatus,
        icon: Icon::RefreshCw,
        items: &[plain(
            Large,
            Some(Icon::RefreshCw),
            TextId::RibbonUpdateProject,
            TextId::RibbonTipUpdateProject,
            RibbonCmd::UpdateProject,
        )],
        shrink_rank: 2,
    },
    GroupSpec {
        label: TextId::RibbonGroupPrint,
        icon: Icon::Printer,
        items: &[
            plain(
                Large,
                Some(Icon::Printer),
                TextId::MenuPrintMode,
                TextId::RibbonTipPrintMode,
                RibbonCmd::PrintMode,
            ),
            plain(
                Small,
                Some(Icon::FileCog),
                TextId::MenuPageSetup,
                TextId::RibbonTipPageSetup,
                RibbonCmd::PageSetup,
            ),
            plain(
                Small,
                Some(Icon::FileDown),
                TextId::RibbonExportPdf,
                TextId::RibbonTipExportPdf,
                RibbonCmd::ExportPdf,
            ),
        ],
        shrink_rank: 1,
    },
];

/// Onglet Affichage.
const VIEW_GROUPS: &[GroupSpec] = &[
    GroupSpec {
        label: TextId::RibbonGroupTaskViews,
        icon: Icon::ChartGantt,
        items: &[GANTT_CHART, TRACKING_GANTT],
        shrink_rank: 6,
    },
    GroupSpec {
        label: TextId::RibbonGroupResourceViews,
        icon: Icon::Users,
        items: &[
            plain(
                Small,
                Some(Icon::Users),
                TextId::MenuViewResourceSheet,
                TextId::RibbonTipResourceSheet,
                RibbonCmd::ResourceSheet,
            ),
            plain(
                Small,
                Some(Icon::ChartColumn),
                TextId::MenuViewResourceUsage,
                TextId::RibbonTipResourceUsage,
                RibbonCmd::ResourceUsage,
            ),
        ],
        shrink_rank: 5,
    },
    GroupSpec {
        label: TextId::RibbonGroupData,
        icon: Icon::Filter,
        items: &[
            plain(
                Dropdown,
                Some(Icon::ArrowUpDown),
                TextId::RibbonSort,
                TextId::RibbonTipSort,
                RibbonCmd::SortMenu,
            ),
            plain(
                Dropdown,
                Some(Icon::ChevronsDownUp),
                TextId::RibbonOutline,
                TextId::RibbonTipOutline,
                RibbonCmd::OutlineMenu,
            ),
            plain(
                Dropdown,
                Some(Icon::Filter),
                TextId::RibbonFilter,
                TextId::RibbonTipFilter,
                RibbonCmd::FilterMenu,
            ),
            // Finitions P3 (H-UI-129) : Regrouper par ▾ après Filtre.
            plain(
                Dropdown,
                Some(Icon::Group),
                TextId::RibbonGroupBy,
                TextId::RibbonTipGroupBy,
                RibbonCmd::GroupByMenu,
            ),
        ],
        shrink_rank: 4,
    },
    GroupSpec {
        label: TextId::RibbonGroupZoom,
        icon: Icon::Maximize,
        items: &[
            plain(
                Dropdown,
                Some(Icon::CalendarRange),
                TextId::RibbonTimescale,
                TextId::RibbonTipTimescale,
                RibbonCmd::TimescaleMenu,
            ),
            item(
                Small,
                Some(Icon::ZoomIn),
                TextId::RibbonZoomIn,
                TextId::RibbonTipZoomIn,
                Some(ui(UiBinding::ZoomFiner)),
                RibbonCmd::ZoomIn,
            ),
            item(
                Small,
                Some(Icon::ZoomOut),
                TextId::RibbonZoomOut,
                TextId::RibbonTipZoomOut,
                Some(ui(UiBinding::ZoomCoarser)),
                RibbonCmd::ZoomOut,
            ),
            plain(
                Large,
                Some(Icon::Maximize),
                TextId::RibbonEntireProject,
                TextId::RibbonTipEntireProject,
                RibbonCmd::EntireProject,
            ),
            // Finitions P1 (H-UI-124) : Tâches sélectionnées, grand bouton
            // à côté de Projet entier.
            plain(
                Large,
                Some(Icon::Focus),
                TextId::RibbonZoomSelected,
                TextId::RibbonTipZoomSelected,
                RibbonCmd::ZoomSelected,
            ),
        ],
        shrink_rank: 3,
    },
    GroupSpec {
        label: TextId::RibbonGroupSplitView,
        icon: Icon::Timeline,
        items: &[
            plain(
                Check,
                None,
                TextId::RibbonTimeline,
                TextId::RibbonTipTimeline,
                RibbonCmd::TimelineVisible,
            ),
            // Seconde passe UX (L10, H-UI-112) : Détails à côté de
            // Chronologie, comme le groupe Split View de Project.
            plain(
                Check,
                None,
                TextId::RibbonDetails,
                TextId::RibbonTipDetails,
                RibbonCmd::DetailsPane,
            ),
            plain(
                Check,
                None,
                TextId::MenuAiPanel,
                TextId::RibbonTipAiPanel,
                RibbonCmd::AiPanel,
            ),
            // ADR-0024, lot A1 (H-UI-138) : assistant IA, à côté du volet
            // Opérations IA.
            plain(
                Check,
                None,
                TextId::MenuAssistant,
                TextId::RibbonTipAssistant,
                RibbonCmd::Assistant,
            ),
            plain(
                Check,
                None,
                TextId::MenuDiagnostics,
                TextId::RibbonTipDiagnostics,
                RibbonCmd::Diagnostics,
            ),
        ],
        shrink_rank: 2,
    },
    GroupSpec {
        label: TextId::RibbonGroupInterface,
        icon: Icon::SunMoon,
        items: &[
            plain(
                Dropdown,
                Some(Icon::SunMoon),
                TextId::MenuTheme,
                TextId::RibbonTipTheme,
                RibbonCmd::ThemeMenu,
            ),
            plain(
                Dropdown,
                Some(Icon::Languages),
                TextId::MenuLanguage,
                TextId::RibbonTipLanguage,
                RibbonCmd::LanguageMenu,
            ),
            item(
                Check,
                Some(Icon::ChevronsUpDown),
                TextId::RibbonCollapse,
                TextId::RibbonTipCollapse,
                Some(ui(UiBinding::ToggleRibbon)),
                RibbonCmd::CollapseRibbon,
            ),
        ],
        shrink_rank: 1,
    },
];

/// Onglet Format.
const FORMAT_GROUPS: &[GroupSpec] = &[
    GroupSpec {
        label: TextId::RibbonGroupColumns,
        icon: Icon::Columns3,
        items: &[
            // Seconde passe UX (L10, H-UI-114, H-UI-115) : Insérer une
            // colonne et Paramètres de champ, comme le groupe Columns de
            // Project ; la liste garde l'affichage du tableau.
            plain(
                Large,
                Some(Icon::BetweenVerticalStart),
                TextId::RibbonInsertColumn,
                TextId::RibbonTipInsertColumn,
                RibbonCmd::InsertColumn,
            ),
            item(
                Small,
                Some(Icon::Columns3Cog),
                TextId::RibbonFieldSettings,
                TextId::RibbonTipFieldSettings,
                Some(ui(UiBinding::FieldSettings)),
                RibbonCmd::FieldSettings,
            ),
            plain(
                Dropdown,
                Some(Icon::Columns3),
                TextId::RibbonColumns,
                TextId::RibbonTipColumns,
                RibbonCmd::ColumnsMenu,
            ),
        ],
        shrink_rank: 4,
    },
    GroupSpec {
        label: TextId::RibbonGroupBarStyles,
        icon: Icon::Paintbrush,
        items: &[
            FORMAT_BARS,
            plain(
                Check,
                None,
                TextId::RibbonCriticalTasks,
                TextId::RibbonTipCriticalTasks,
                RibbonCmd::CriticalTasks,
            ),
            plain(
                Check,
                None,
                TextId::MenuRelativeMonths,
                TextId::RibbonTipRelativeMonths,
                RibbonCmd::RelativeMonths,
            ),
        ],
        shrink_rank: 3,
    },
    GroupSpec {
        label: TextId::RibbonGroupChartStyle,
        icon: Icon::SwatchBook,
        items: &[plain(
            Large,
            Some(Icon::SwatchBook),
            TextId::MenuGallery,
            TextId::RibbonTipGallery,
            RibbonCmd::Gallery,
        )],
        shrink_rank: 2,
    },
    GroupSpec {
        label: TextId::RibbonGroupShowHide,
        icon: Icon::PanelLeftClose,
        items: &[plain(
            Check,
            Some(Icon::PanelLeftClose),
            TextId::MenuChartOnly,
            TextId::RibbonTipChartOnly,
            RibbonCmd::ChartOnly,
        )],
        shrink_rank: 1,
    },
];

/// Contenu du ruban : onglet → groupes, dans l'ordre d'affichage (§ 2.2).
pub const RIBBON: &[(RibbonTab, &[GroupSpec])] = &[
    (RibbonTab::Task, TASK_GROUPS),
    (RibbonTab::Resource, RESOURCE_GROUPS),
    (RibbonTab::Project, PROJECT_GROUPS),
    (RibbonTab::View, VIEW_GROUPS),
    (RibbonTab::Format, FORMAT_GROUPS),
];

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use std::collections::BTreeSet;

    use super::{ItemSize, MAX_GROUP_ITEMS, PERCENT_STEPS, QUICK_ACCESS, RIBBON, percent_value};
    use crate::i18n::{Language, TextId, Texts};
    use crate::keymap::{KEYMAP, binding_action};
    use crate::ribbon::RibbonTab;

    /// Tous les textes de la table (onglets, groupes, libellés, aides,
    /// libellés des jumeaux).
    fn spec_text_ids() -> BTreeSet<TextId> {
        let mut ids = BTreeSet::new();
        for (tab, groups) in RIBBON {
            ids.insert(tab.label());
            for group in *groups {
                ids.insert(group.label);
                for item in group.items {
                    ids.extend([item.label, item.tip, item.cmd.menu_label()]);
                }
            }
        }
        for item in QUICK_ACCESS {
            ids.extend([item.label, item.tip, item.cmd.menu_label()]);
        }
        ids
    }

    /// ADR-0017 (lot L0) : chaque texte de la table existe en français et
    /// en anglais (non vide, sans clé brute).
    #[test]
    fn ribbon_spec_text_ids_exist() {
        let ids = spec_text_ids();
        assert!(ids.len() > 60, "{}", ids.len());
        for language in [Language::Fr, Language::En] {
            let texts = Texts::new(language);
            for id in &ids {
                let text = texts.get(*id);
                assert!(!text.is_empty() && text != id.key(), "{language:?} : {id:?}");
            }
        }
    }

    /// H-UI-69 : cinq onglets dans l'ordre, groupes non vides, plafonds
    /// écrits ; rangs de réduction distincts par onglet (H-UI-75).
    #[test]
    fn ribbon_spec_shape() {
        let tabs: Vec<RibbonTab> = RIBBON.iter().map(|(tab, _)| *tab).collect();
        assert_eq!(tabs, RibbonTab::ALL);
        for (tab, groups) in RIBBON {
            assert!(!groups.is_empty(), "{tab:?}");
            assert_eq!(tab.groups(), *groups);
            let ranks: BTreeSet<u8> = groups.iter().map(|g| g.shrink_rank).collect();
            assert_eq!(ranks.len(), groups.len(), "{tab:?}");
            for group in *groups {
                assert!(!group.items.is_empty() && group.items.len() <= MAX_GROUP_ITEMS);
                for item in group.items {
                    // Grands et petits boutons ont une icône (H-UI-77).
                    if matches!(item.size, ItemSize::Large | ItemSize::Small) {
                        assert!(item.icon.is_some(), "{:?}", item.cmd);
                    }
                }
            }
        }
        assert_eq!(QUICK_ACCESS.len(), 3);
    }

    /// Raccourcis affichés : liaisons présentes dans `KEYMAP`, et leur
    /// action est celle de l'élément quand elle ne dépend pas de l'état.
    #[test]
    fn ribbon_spec_bindings_in_keymap() {
        for item in RIBBON
            .iter()
            .flat_map(|(_, groups)| groups.iter())
            .flat_map(|g| g.items.iter())
            .chain(QUICK_ACCESS)
        {
            if let Some(binding) = item.binding {
                assert!(KEYMAP.iter().any(|(_, _, b)| *b == binding), "{binding:?}");
                assert!(binding_action(binding).is_some(), "{binding:?}");
            }
        }
    }

    /// H-UI-100 : chaque élément qui affiche un raccourci affiche celui de
    /// sa commande ([`super::RibbonCmd::binding`], lu aussi par les menus
    /// contextuels) ; les commandes des menus contextuels ont une liaison
    /// présente dans `KEYMAP`.
    #[test]
    fn h_ui_100_bindings_shared_with_menus() {
        for item in RIBBON
            .iter()
            .flat_map(|(_, groups)| groups.iter())
            .flat_map(|g| g.items.iter())
            .chain(QUICK_ACCESS)
        {
            if item.binding.is_some() {
                assert_eq!(item.binding, item.cmd.binding(), "{:?}", item.cmd);
            }
        }
        for cmd in super::CONTEXT_MENU.iter().flat_map(|g| g.iter()) {
            if let Some(binding) = cmd.binding() {
                assert!(KEYMAP.iter().any(|(_, _, b)| *b == binding), "{binding:?}");
            }
        }
    }

    #[test]
    fn ribbon_percent_values() {
        for (step, pct) in PERCENT_STEPS {
            assert_eq!(percent_value(step), pct);
        }
    }
}
