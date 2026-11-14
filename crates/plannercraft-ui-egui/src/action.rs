//! Intentions de l'utilisateur.
//!
//! Les vues ne modifient jamais l'application : elles poussent des [`Action`]
//! dans une file, appliquée par le réducteur unique d'`App` en fin d'image.
//! Une action sérialisée préfigure aussi le canal de contrôle (ADR-0002).

use plannercraft_automation::Batch;
use plannercraft_automation::gesture::GestureError;
use plannercraft_core::TaskUid;

use crate::clipboard::ClipboardOp;
use crate::column::Column;
use crate::nav::{NavMove, RowRef};
use crate::ribbon::RibbonOp;
use crate::selection::SelectMode;
use crate::state::{GanttStyle, LanguagePref, ThemePref, ViewKind};
use crate::structure::StructureOp;
use crate::timescale::TimeScaleZoom;
use crate::view::Pane;
use crate::view::side_panel::SideOp;
use crate::view::task_view::TaskViewOp;

/// Intention de l'utilisateur, appliquée en fin d'image. Pas `Copy` :
/// [`Action::Exec`] porte un lot de commandes (ADR-0012 D11).
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Ouvre le projet d'exemple embarqué.
    OpenDemo,
    /// Ouvre le sélecteur de fichiers (rfd natif ou téléversement web).
    OpenDialog,
    /// Ferme le document courant.
    CloseDocument,
    /// Sélectionne une tâche (ou aucune).
    Select(Option<TaskUid>),
    /// Change la préférence de langue.
    SetLanguage(LanguagePref),
    /// Change la préférence de thème.
    SetTheme(ThemePref),
    /// « Réduire les animations » (lot A1b, H-UI-147).
    SetReduceMotion(bool),
    /// Change le zoom de l'échelle de temps, ancré au centre du Gantt
    /// visible (la zone affichée reste à l'écran).
    SetZoom(TimeScaleZoom),
    /// Réapplique le zoom ajusté à la plage du document, ancré au centre du
    /// Gantt visible.
    FitZoom,
    /// Pas de zoom (Ctrl/Cmd + molette), qui garde sous le pointeur le même
    /// point du contenu ; arithmétique en pixels seulement.
    ZoomStep {
        /// Nombre de pas : positif vers le plus fin, négatif vers le plus
        /// grossier.
        steps: i8,
        /// Abscisse du pointeur **relative au bord gauche du Gantt**
        /// visible (0 = bord gauche), bornée par le réducteur à la largeur
        /// du Gantt ; une valeur non finie vaut 0.
        anchor_x: f32,
    },
    /// Largeur de la table (séparateur déplacé), en points logiques.
    SetTableWidth(u16),
    /// Défilement horizontal du Gantt, delta en points.
    ScrollGanttX(f32),
    /// Fait défiler jusqu'à la ligne d'indice donné.
    ScrollToRow(usize),
    /// Ouvre ou referme le panneau des diagnostics.
    ToggleDiagnostics,
    /// Ferme tous les messages affichés (bandeau et notifications ;
    /// l'historique est gardé). Le « Fermer » de chaque message pousse
    /// `Message(MessageOp::Dismiss)`.
    DismissMessage,
    /// Applique un lot de commandes à la session du document (toute
    /// édition passe par le registre). Sans effet jusqu'au lot L11.
    Exec(Batch),
    /// Comme [`Action::Exec`], pour le document de numéro d'ouverture
    /// `opening` seulement (saisie de cellule) : abandonné si un autre
    /// document l'a remplacé entre-temps (relecture finale J4, F2).
    ExecOn {
        /// Numéro d'ouverture du document visé (`OpenDocument::opening`).
        opening: u64,
        /// Lot de commandes.
        batch: Batch,
    },
    /// Annule le dernier lot. Sans effet jusqu'au lot L11.
    Undo,
    /// Rétablit le dernier lot annulé. Sans effet jusqu'au lot L11.
    Redo,
    /// Crée un projet vide (« Projet1 »), après confirmation si le
    /// document courant est modifié.
    NewProject,
    /// Enregistre sur le fichier du document, ou demande un chemin s'il
    /// n'en a pas.
    Save,
    /// Enregistre sous un autre nom (boîte de dialogue ou téléchargement).
    SaveAs,
    /// Quitte l'application (desktop : Ctrl/Cmd+Q, menu Fichier), après
    /// confirmation si le document est modifié ; sans effet sur le web.
    Quit,
    /// Réponse au bandeau « Enregistrer les modifications ? ».
    Confirm(ConfirmChoice),
    /// Clic de sélection avec modificateur (Ctrl/Cmd : ajout ou retrait ;
    /// Maj : plage), lot L14.
    SelectWith {
        /// Tâche cliquée.
        uid: TaskUid,
        /// Mode du clic.
        mode: SelectMode,
    },
    /// Commande de structure sur la sélection (Inser, Suppr, indentation,
    /// liens), lot L14.
    Structure(StructureOp),
    /// Réponse à la confirmation d'une suppression de récapitulative.
    ConfirmDelete(bool),
    // --- J7 (contrat L0, ADR-0015) : sans effet tant que leur lot ne les
    // câble pas (réducteur d'`App`).
    /// Déplacement au clavier dans la grille (H-UI-13 à H-UI-16, lot L1) ;
    /// `extend` : étend la sélection de lignes depuis l'ancre.
    Nav {
        /// Déplacement.
        mv: NavMove,
        /// Extension de la sélection (Maj).
        extend: bool,
    },
    /// Donne le focus clavier à un volet (lot L1).
    FocusPane(Pane),
    /// Change la vue principale (lots L10, L11).
    SetView(ViewKind),
    /// Change le style du Gantt (lot L7).
    SetGanttStyle(GanttStyle),
    /// Ouvre une fenêtre (lots L8, L9, L12, L13).
    ShowDialog(DialogKind),
    /// « Informations sur la tâche » de la tâche active (Maj+F2, H-UI-19,
    /// lot L9) : la tâche est résolue par le réducteur.
    ShowTaskInformation,
    /// Ferme la fenêtre ouverte (Échap, Annuler).
    CloseDialog,
    /// Affiche ou masque une colonne (lot L6 ; ID et Nom restent visibles).
    ToggleColumn(Column),
    /// Largeur d'une colonne (bord de l'en-tête glissé, lot L6), bornée par
    /// `UiState::normalized`.
    SetColumnWidth {
        /// Colonne.
        column: Column,
        /// Largeur en points logiques.
        width: u16,
    },
    /// Défilement horizontal de la table, delta en points (lot L6).
    ScrollTableX(f32),
    /// Alt+←/→ : une graduation fine de l'échelle (H-UI-18, lot L1).
    TimescaleStep(i8),
    /// Alt+Page haut/bas : une largeur visible du Gantt (H-UI-18, lot L1).
    TimescalePage(i8),
    /// Alt+Origine : début du projet (H-UI-18, lot L1).
    TimescaleToStart,
    /// Alt+Fin : fin du projet (H-UI-18, lot L1).
    TimescaleToEnd,
    /// Zoom au pas voisin, ancré au centre du Gantt (positif : plus fin ;
    /// H-UI-18, lot L1).
    ZoomBy(i8),
    /// Accepte l'écriture relayée `id` (bandeau, H-UI-27, lot L3).
    Approve(u64),
    /// Refuse l'écriture relayée `id` (lot L3).
    Reject(u64),
    /// Geste sur une proposition de l'assistant (carte de la conversation
    /// ou panneau Opérations IA ; lot A2 d'ADR-0024).
    Proposal(crate::approvals::ProposalOp),
    /// Ouvre le fichier récent d'indice donné (H-UI-28, lot L14).
    OpenRecent(usize),
    /// Réponse au bandeau de reprise après plantage (lot L15).
    Recover(RecoverChoice),
    /// Action d'interface demandée par un agent (`app.ui`, lot L3).
    Ui(UiOp),
    /// Geste du Gantt refusé au lâcher (J7, lot L8 : barre commencée,
    /// jalon, écart hors limites) : message au bandeau.
    GestureRefused(GestureError),
    // --- J8 L9 : vue des tâches (filtres, tri, repli, recherche).
    /// Commande de la vue des tâches (aucune écriture dans le document,
    /// P50).
    TaskView(TaskViewOp),
    // --- J8 L10 : presse-papiers et import CSV (H-UI-39, H-UI-40).
    /// Couper, copier, coller ou « Collage spécial… » des lignes
    /// sélectionnées (menu Édition ; les touches arrivent en
    /// `Event::Cut/Copy/Paste`).
    Clipboard(ClipboardOp),
    /// Fichier → Importer CSV… : sélecteur de fichiers, puis aperçu.
    ImportCsv,
    /// Fichier → Importer un PDF… : sélecteur de fichiers, puis aperçu
    /// (lot P1b, H-UI-205).
    ImportPdf,
    // --- J8 L13 : exports téléchargés (web) ou enregistrés (desktop).
    /// Fichier → Exporter → PDF, SVG ou CSV : octets produits par
    /// `plannercraft_automation`, remis au `FileService` (boîte « Enregistrer
    /// sous » sur le desktop, téléchargement sur le web).
    Export(crate::io::SaveKind),
    // --- J8 L12 : volet latéral (H-UI-41, H-UI-58).
    /// Ouvre, referme ou change d'onglet le volet latéral.
    SidePanel(SideOp),
    /// Commande du volet Assistant (ADR-0024, lot A1).
    Assistant(crate::assistant::AssistantOp),
    // --- J8 L15 : menu Tâche, barre d'outils, mise en forme.
    /// Commande de tâche sur la sélection (menu Tâche, barre d'outils) :
    /// un seul lot, une entrée d'annulation (R-252, R-253, R-255).
    TaskCmd(TaskCmd),
    /// Mise en forme : volet des barres, modèles personnels (H-UI-52,
    /// H-UI-59 à H-UI-61, H-UI-65).
    Format(FormatOp),
    /// Affichage → « Graphique seul » (H-UI-64) : table réduite au N° et
    /// au Nom à l'écran.
    SetChartOnly(bool),
    /// Affichage → surbrillance du chemin critique (H-UI-38).
    SetHighlightCritical(bool),
    /// Aide → À propos : nom et version au bandeau.
    About,
    // --- Ruban et chronologie (ADR-0017, contrat L0) : servis par leur
    // lot (L2 ruban, L3 commandes, L4 chronologie).
    /// État du ruban : onglet, repli, surimpression (H-UI-69, H-UI-72).
    Ribbon(RibbonOp),
    /// Chronologie : visibilité, ajout et retrait de tâches, clic sur un
    /// élément (H-UI-78, H-UI-79, H-UI-81).
    Timeline(TimelineOp),
    /// « Défiler jusqu'à la tâche » active (H-UI-84, lot L3).
    ScrollToTask,
    /// « Informations sur la tâche » de la tâche active, ouverte à
    /// l'onglet Notes (H-UI-84, lot L3).
    ShowTaskNotes,
    // --- Passe UX (ADR-0018, contrat L0) : bras posés une fois dans le
    // réducteur ; les fonctions appelées sont servies par leur lot (L1
    // sélection, L2 cellules, L3 lignes et colonnes). `Select` et
    // `SelectWith` restent (Gantt, chronologie, agents) : ils posent une
    // sélection `Cells` d'une cellule.
    /// Geste de sélection de la table (appui, glisser, raccourcis ;
    /// H-UI-87 à H-UI-92) : `App::select_op` (`selection.rs`).
    SelectOp(SelectOp),
    /// Commande de cellule sur le bloc (remplir, effacer, Suppr, Ctrl+Entrée ;
    /// H-UI-96 à H-UI-98) : `App::cells_op` (`cells.rs`).
    Cells(CellOp),
    /// Déplace les lignes sélectionnées en lignes entières avant la tâche
    /// `before` (`None` : en fin), un lot de `task.move` (H-UI-94) :
    /// `App::move_rows` (`structure.rs`).
    MoveRows {
        /// Tâche devant laquelle les lignes vont (`None` : fin du projet).
        before: Option<TaskUid>,
    },
    /// Ajuste la largeur d'une colonne à son contenu (double-clic sur la
    /// bordure de l'en-tête, H-UI-99) : `App::fit_column` (`column.rs`).
    FitColumn(Column),
    // --- Seconde passe UX (ADR-0019, contrat L0) : bras posés une fois
    // dans le réducteur ; les fonctions appelées sont servies par leur lot
    // (`panes.rs` L4, `column.rs` L6, `view/details.rs` L5, `notify.rs`
    // L1).
    /// Volets : chronologie, volet de détails, F6 (H-UI-110 à H-UI-112) :
    /// `App::pane_op` (`panes.rs`).
    Pane(PaneOp),
    /// Colonnes affichées : insérer, masquer, déplacer, paramètres
    /// (H-UI-114 à H-UI-116) : `App::column_op` (`column.rs`).
    Column(ColumnOp),
    /// OK ou Annuler du volet de détails (H-UI-113) : `App::details_op`
    /// (`view/details.rs`).
    Details(DetailsOp),
    /// Notifications et historique des messages (H-UI-104).
    Message(MessageOp),
    /// Fermer le document, après confirmation s'il est modifié (Ctrl/Cmd+W,
    /// Ctrl+F4, H-UI-109) ; la fenêtre reste ouverte (écran d'accueil).
    CloseDocumentGuarded,
    // --- Interface adoucie (ADR-0026, lot U3) : palette de commandes.
    /// Ouvre ou ferme la palette de commandes (H-UI-177).
    Palette(crate::palette::PaletteOp),
    /// « Demander à l'assistant : « … » » (H-UI-182) : volet ouvert sur
    /// l'onglet Assistant, question préremplie avec le texte ; rien n'est
    /// envoyé.
    AskAssistant(String),
    // --- Finitions avant v1.0.0 (ADR-0022, lot P1).
    /// Affichage → Zoom → Tâches sélectionnées (H-UI-124) : échelle la plus
    /// fine où les barres de la sélection tiennent dans la largeur du
    /// Gantt, puis défilement jusqu'à elles.
    ZoomToSelection,
    // --- Finitions avant v1.0.0 (ADR-0022, lot P2).
    /// Mise en forme du texte des lignes sélectionnées (H-UI-126 à
    /// H-UI-128).
    TextStyle(TextStyleOp),
    // --- Finitions avant v1.0.0 (ADR-0022, lot P3).
    /// Regrouper par, replier un groupe (H-UI-129 à H-UI-131).
    Group(crate::group_view::GroupOp),
}

/// Opération sur les volets (seconde passe UX, ADR-0019 D5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneOp {
    /// Hauteur de la chronologie, en points (séparateur relâché).
    SetTimelineHeight(u16),
    /// Affiche ou masque le volet de détails (Affichage → Détails, ×).
    ShowDetails(bool),
    /// Hauteur du volet de détails, en points.
    SetDetailsHeight(u16),
    /// Forme du volet de détails (clic droit dans le volet).
    SetDetailsForm(crate::state::DetailsForm),
    /// F6 / Maj+F6 : clavier au volet suivant / précédent.
    FocusNext {
        /// Maj+F6 : précédent.
        back: bool,
    },
}

/// Rang de colonne « colonne active » de [`ColumnOp::OpenSettings`]
/// (Alt+F3) : résolu par le réducteur sur la colonne de la cellule active.
pub const ACTIVE_COLUMN: usize = usize::MAX;

/// Opération sur les colonnes affichées (`UiState.columns`, seconde passe
/// UX, ADR-0019 D7) : une préférence d'affichage, hors de l'historique
/// d'annulation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColumnOp {
    /// Insère `column` au rang `at` (à gauche de la colonne cliquée ; déjà
    /// affichée : déplacée).
    Insert {
        /// Rang d'insertion.
        at: usize,
        /// Champ inséré.
        column: Column,
    },
    /// Masque la colonne de rang donné.
    Hide(usize),
    /// Déplace la colonne de rang `from` au rang `to`.
    Move {
        /// Rang de départ.
        from: usize,
        /// Rang d'arrivée.
        to: usize,
    },
    /// Réglages de « Paramètres de champ » pour la colonne de rang `at`.
    Settings {
        /// Rang de la colonne.
        at: usize,
        /// Réglages (champ, titre, alignements, largeur).
        settings: crate::column::ColumnState,
    },
    /// Ouvre le sélecteur de champ pour une insertion au rang `at`.
    OpenPicker {
        /// Rang d'insertion.
        at: usize,
    },
    /// Ouvre « Paramètres de champ » de la colonne de rang donné
    /// ([`ACTIVE_COLUMN`] : colonne de la cellule active).
    OpenSettings(usize),
}

/// Bouton du volet de détails (H-UI-113).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailsOp {
    /// OK (ou Entrée hors grille) : écrit le brouillon en un lot.
    Ok,
    /// Annuler (ou Échap) : relit le brouillon.
    Cancel,
}

/// Opération sur les messages (H-UI-104).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageOp {
    /// Ferme le message d'identifiant donné (notification ou bandeau) ;
    /// `Action::DismissMessage` ferme tous les messages affichés.
    Dismiss(u64),
    /// Ouvre ou ferme l'historique des messages.
    ToggleHistory,
}

/// Geste de sélection de la table (ADR-0018 D1, D2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectOp {
    /// Appui du bouton principal (H-UI-88 à H-UI-90).
    Press {
        /// Ligne appuyée (la ligne vide comprise).
        row: RowRef<TaskUid>,
        /// Colonne appuyée.
        column: Column,
        /// Cellule, en-tête de ligne (colonne ID) ou en-tête de colonne.
        target: PressTarget,
        /// Modificateur.
        mode: PressMode,
    },
    /// Glisser : la cellule sous le pointeur a changé (H-UI-88).
    DragTo {
        /// Ligne sous le pointeur (bornée au corps).
        row: RowRef<TaskUid>,
        /// Colonne sous le pointeur (bornée aux colonnes visibles).
        column: Column,
    },
    /// Tout (Ctrl/Cmd+A, Ctrl+Maj+Espace, coin de l'en-tête ; H-UI-92).
    All,
    /// Lignes entières de la sélection (Maj+Espace).
    EntireRows,
    /// Colonnes entières de la sélection (Ctrl+Espace).
    EntireColumns,
    /// Réduit à la cellule active (Maj+Retour arrière).
    Collapse,
}

impl SelectOp {
    /// Libellé du jumeau de menu (menu Édition, H-UI-100) ; `None` pour
    /// les gestes sans entrée de menu.
    #[must_use]
    pub const fn menu_label(self) -> Option<crate::i18n::TextId> {
        match self {
            Self::All => Some(crate::i18n::TextId::MenuSelectAll),
            Self::Press { .. }
            | Self::DragTo { .. }
            | Self::EntireRows
            | Self::EntireColumns
            | Self::Collapse => None,
        }
    }
}

/// Cible d'un appui de sélection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PressTarget {
    /// Cellule du corps.
    Cell,
    /// Cellule de la colonne ID (en-tête de ligne, H-UI-89).
    RowHeader,
    /// En-tête de colonne (H-UI-90).
    ColumnHeader,
}

/// Modificateur d'un appui de sélection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PressMode {
    /// Sans modificateur : remplace la sélection.
    Replace,
    /// Maj : étend depuis l'ancre.
    Extend,
    /// Ctrl/Cmd : ajoute ou retire la ligne.
    Toggle,
}

/// Commande de cellule sur le bloc de la sélection (ADR-0018 D4, D5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellOp {
    /// Remplir vers le bas (Ctrl/Cmd+D, R-258).
    FillDown,
    /// Effacer le contenu du bloc (Ctrl+Suppr, R-259).
    Clear,
    /// Suppr : supprimer les tâches ou effacer selon la cible (H-UI-97).
    DeleteKey,
    /// Ctrl/Cmd+Entrée : texte saisi écrit dans tout le bloc (H-UI-98).
    FillTyped(String),
    /// Ctrl/Cmd+R : remplir à droite (R-258 amendée, H-UI-120 ; seconde
    /// passe UX, servi par le lot L2).
    FillRight,
}

/// Opération de la chronologie (ADR-0017 D7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimelineOp {
    /// Affiche ou masque la frise (Affichage → Chronologie, H-UI-78).
    SetVisible(bool),
    /// Bascule la sélection : si une tâche sélectionnée n'y est pas, toutes
    /// y vont, sinon toutes en sortent (H-UI-79) ; un `style.apply`.
    ToggleSelection,
    /// Retire une tâche de la frise (menu contextuel de la frise, H-UI-81).
    Remove(TaskUid),
    /// Clic sur un élément de la frise : sélection et défilement (H-UI-81).
    Activate(TaskUid),
}

/// Commande de tâche du menu Tâche et de la barre d'outils (J8 lot L15).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskCmd {
    /// Marquer n % (H-UI-35, R-252).
    MarkPercent(plannercraft_automation::helpers::PercentStep),
    /// Basculer manuel ↔ automatique (H-UI-36, R-253).
    ToggleMode,
    /// Activer ou désactiver (R-255).
    ToggleActive,
    /// Décaler la sélection d'un pas prédéfini (H-UI-121, R-267).
    Shift(ShiftStep),
}

/// Pas prédéfini de « Décaler ▾ » (finitions P1, H-UI-121) : jours ou
/// semaines ouvrés, signés.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ShiftStep {
    /// +1 jour.
    PlusDay,
    /// +1 semaine.
    PlusWeek,
    /// +4 semaines.
    PlusFourWeeks,
    /// −1 jour.
    MinusDay,
    /// −1 semaine.
    MinusWeek,
}

impl ShiftStep {
    /// Pas, dans l'ordre de la liste (cahier `docs/plans/polish.md`, P1).
    pub const ALL: [Self; 5] = [
        Self::PlusDay,
        Self::PlusWeek,
        Self::PlusFourWeeks,
        Self::MinusDay,
        Self::MinusWeek,
    ];

    /// Nombre signé d'unités et unité (jour ou semaine).
    #[must_use]
    pub const fn amount(self) -> (i64, plannercraft_automation::command::DurationUnitJson) {
        use plannercraft_automation::command::DurationUnitJson::{Day, Week};
        match self {
            Self::PlusDay => (1, Day),
            Self::PlusWeek => (1, Week),
            Self::PlusFourWeeks => (4, Week),
            Self::MinusDay => (-1, Day),
            Self::MinusWeek => (-1, Week),
        }
    }

    /// Libellé de l'entrée (« +1 jour »).
    #[must_use]
    pub const fn label(self) -> crate::i18n::TextId {
        use crate::i18n::TextId;
        match self {
            Self::PlusDay => TextId::ShiftPlusDay,
            Self::PlusWeek => TextId::ShiftPlusWeek,
            Self::PlusFourWeeks => TextId::ShiftPlusFourWeeks,
            Self::MinusDay => TextId::ShiftMinusDay,
            Self::MinusWeek => TextId::ShiftMinusWeek,
        }
    }
}

/// Mise en forme du texte des lignes (finitions P2, H-UI-126 à H-UI-128) :
/// un geste = un lot `task.set_text_style` sur la sélection, ou rien s'il
/// ne change rien.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextStyleOp {
    /// Gras : inverse l'état de la tâche active, appliqué à la sélection
    /// (Ctrl/Cmd+B).
    ToggleBold,
    /// Italique : comme le gras (Ctrl/Cmd+I, finitions P4).
    ToggleItalic,
    /// Couleur du texte (`None` : automatique).
    Color(Option<plannercraft_render::style::Rgb>),
    /// Surlignage (`None` : aucun).
    Fill(Option<plannercraft_render::style::Rgb>),
    /// « Reproduire la mise en forme » : arme le pinceau avec la mise en
    /// forme de la tâche active, ou le désarme.
    Painter,
    /// « Effacer la mise en forme » de la sélection.
    Clear,
}

/// Opération de mise en forme (J8 lot L15).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormatOp {
    /// Ouvre le volet de mise en forme des barres d'un type (H-UI-59) ;
    /// `task` : tâche cliquée (couleur de phase d'une récapitulative,
    /// H-UI-65).
    OpenBars {
        /// Type de barre.
        role: plannercraft_render::style::BarRole,
        /// Tâche de la barre cliquée.
        task: Option<TaskUid>,
    },
    /// Referme le volet.
    CloseBars,
    /// Change le type de barre du volet.
    SetRole(plannercraft_render::style::BarRole),
    /// « Enregistrer comme modèle personnel » : style du document sous ce
    /// nom (H-UI-52, H-UI-61).
    SaveUserTemplate(String),
    /// Applique le modèle personnel d'indice donné (une entrée
    /// d'annulation).
    ApplyUserTemplate(usize),
    /// Modèle des documents sans style (H-UI-43).
    SetDefaultTemplate(plannercraft_render::style::TemplateId),
    /// Mois relatifs M1… en haut de l'échelle (H-UI-67, « en option » :
    /// case du menu Format, relecture finale de J8) ; une entrée
    /// d'annulation.
    SetRelativeMonths(bool),
}

/// Fenêtre à ouvrir (J7, ADR-0015 D6 ; `Action::OpenDialog` reste le
/// sélecteur de fichiers).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogKind {
    /// « Informations sur la tâche » (lot L9).
    TaskInformation(TaskUid),
    /// Boîte d'un lien (H-UI-21, lot L8).
    Link {
        /// Prédécesseur.
        predecessor: TaskUid,
        /// Successeur.
        successor: TaskUid,
    },
    /// « Informations sur le projet » (lot L12).
    ProjectInformation,
    /// « Modifier le temps de travail » (lot L12).
    WorkingTime,
    /// « Ligne de base » : définir ou effacer (lot L12).
    Baseline,
    /// « Replanifier le reste » (lot L12).
    Reschedule,
    /// « Affecter des ressources » (lot L13).
    AssignResources,
    /// « Collage spécial… » (J8, lot L10) : options du collage du tampon
    /// interne.
    PasteSpecial,
    /// Aperçu de l'import CSV (J8, lot L10, H-UI-40), ouvert à l'arrivée
    /// du fichier choisi.
    CsvImport,
    /// Aperçu de l'import PDF (lot P1b, H-UI-205), ouvert à l'arrivée du
    /// fichier choisi.
    PdfImport,
    /// Galerie des modèles de mise en forme (J8, lot L15, H-UI-52).
    Gallery,
    /// « Mise en page » ; `print_mode` : ouverte en mode impression
    /// (H-UI-68), colonnes d'impression appliquées à l'aperçu.
    PageSetup {
        /// Mode impression.
        print_mode: bool,
    },
    /// « Champs personnalisés » (Projet → Champs personnalisés…, seconde
    /// passe UX, lot L9, H-UI-118).
    CustomFields,
    /// « Décaler les tâches » : décalage personnalisé (Décaler ▾ →
    /// Personnalisé…, H-UI-121).
    Shift,
}

/// Action d'interface d'un agent (`app.ui`, J7) : sans historique, sans
/// effet sur le projet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiOp {
    /// Sélectionne la tâche d'UID donné (ou aucune).
    Select(Option<u32>),
    /// Fait défiler jusqu'à la tâche d'UID donné.
    ScrollTo(u32),
    /// Zoom de l'échelle de temps.
    Zoom(TimeScaleZoom),
    /// Vue principale.
    View(ViewKind),
    /// Thème.
    Theme(ThemePref),
    /// Langue.
    Language(LanguagePref),
}

/// Réponse au bandeau de reprise après plantage (J7, lot L15) ou au
/// bandeau du planning rouvert par la sauvegarde du navigateur (lot W1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecoverChoice {
    /// Restaure l'orphelin d'identifiant donné.
    Restore(String),
    /// Jette l'orphelin d'identifiant donné.
    Discard(String),
    /// Décide plus tard (le bandeau revient au prochain démarrage).
    Later,
    /// Masque le bandeau « Planning retrouvé dans ce navigateur » (version
    /// web, lot W1, H-UI-199).
    DismissRestored,
}

/// Réponse au bandeau de confirmation d'un document modifié (fermer, ouvrir
/// un autre document, créer un projet, quitter).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmChoice {
    /// Enregistrer, puis poursuivre si l'enregistrement réussit.
    Save,
    /// Poursuivre sans enregistrer : les modifications sont perdues.
    Discard,
    /// Renoncer : le document reste ouvert, rien n'est perdu.
    Cancel,
}
