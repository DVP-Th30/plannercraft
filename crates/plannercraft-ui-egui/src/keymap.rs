//! Raccourcis clavier : table [`KEYMAP`], vérifiée contre le registre des
//! commandes (ADR-0012 D11).
//!
//! Une entrée associe un contexte, une combinaison de touches et une
//! liaison : une commande du registre ([`Binding::Command`], id à point
//! comme `history.undo`) ou une action propre à l'interface
//! ([`Binding::Ui`]). Les tests vérifient que chaque id existe dans
//! `plannercraft_automation::REGISTRY` et que chaque entrée déclenche son action.
//!
//! Ordre : egui compare les modificateurs « logiquement » (Maj et Alt en
//! trop ignorés) ; la combinaison la plus précise d'une touche vient donc
//! avant la moins précise (Ctrl+Maj+Z avant Ctrl+Z), ce qu'un test vérifie.
//! Aucun raccourci n'est lu pendant la saisie dans un champ de texte : ses
//! propres Ctrl+Z et Ctrl+Y lui appartiennent.
//!
//! Navigation (J7, lot L1, H-UI-13 à H-UI-18, H-UI-29) : contexte
//! [`KeyContext::Table`], actif seulement si la table a le focus clavier,
//! sans saisie, menu ni fenêtre ouverts ([`KeyState`]). Alt+échelle de
//! temps, zoom au clavier et Alt+Maj+↑/↓ sont du contexte `Document`.

use egui::{Key, KeyboardShortcut, Modifiers};

use crate::action::{Action, CellOp, SelectOp};
use crate::nav::{InBlockMove, NavMove};
use crate::structure::StructureOp;
use crate::view::task_view::TaskViewOp;

/// Raccourci de navigation dans la table.
const fn nav(modifiers: Modifiers, key: Key, mv: NavMove, extend: bool) -> (KeyContext, Shortcut, Binding) {
    (
        KeyContext::Table,
        Shortcut::new(modifiers, key),
        Binding::Ui(UiBinding::Nav { mv, extend }),
    )
}

/// Raccourci propre à l'interface, contexte `Document`.
const fn doc_ui(modifiers: Modifiers, key: Key, binding: UiBinding) -> (KeyContext, Shortcut, Binding) {
    (
        KeyContext::Document,
        Shortcut::new(modifiers, key),
        Binding::Ui(binding),
    )
}

/// Raccourci propre à l'interface, contexte `Table` (passe UX, ADR-0018).
const fn table_ui(modifiers: Modifiers, key: Key, binding: UiBinding) -> (KeyContext, Shortcut, Binding) {
    (
        KeyContext::Table,
        Shortcut::new(modifiers, key),
        Binding::Ui(binding),
    )
}

/// Combinaison de touches (`Modifiers::COMMAND` : Ctrl, ou Cmd sur macOS).
pub type Shortcut = KeyboardShortcut;

/// Contexte où un raccourci est actif.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyContext {
    /// Partout (accueil compris), hors saisie de texte.
    Global,
    /// Un document est ouvert, hors saisie de texte.
    Document,
    /// La table a le focus, sans saisie en cours, sans menu ni fenêtre
    /// ouverts (J7, ADR-0015 D2 ; actif à partir du lot L1).
    Table,
}

/// Action propre à l'interface (sans commande du registre).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiBinding {
    /// Ouvre le sélecteur de fichiers.
    OpenFile,
    /// Enregistre sous un autre nom (`project.save` avec un chemin choisi
    /// dans la boîte de dialogue).
    SaveAs,
    /// Quitte l'application (desktop), par le bandeau de confirmation si le
    /// document est modifié. Sur macOS, Cmd+Q n'arrive ici que parce que
    /// l'application retire le menu par défaut de winit (`apps/plannercraft`).
    Quit,
    /// Déplacement dans la grille (J7, H-UI-13 à H-UI-16) ; `extend` :
    /// étend la sélection de lignes.
    Nav {
        /// Déplacement.
        mv: NavMove,
        /// Extension de la sélection (Maj).
        extend: bool,
    },
    /// « Informations sur la tâche » (Maj+F2, H-UI-19).
    TaskInformation,
    /// Une graduation fine de l'échelle de temps (Alt+←/→, H-UI-18).
    TimescaleStep(i8),
    /// Une largeur visible (Alt+Page haut/bas, H-UI-18).
    TimescalePage(i8),
    /// Début du projet (Alt+Origine, H-UI-18).
    TimescaleToStart,
    /// Fin du projet (Alt+Fin, H-UI-18).
    TimescaleToEnd,
    /// Unités de temps plus fines (Ctrl+/, Ctrl+=, H-UI-18).
    ZoomFiner,
    /// Unités de temps plus grossières (Ctrl+-, H-UI-18).
    ZoomCoarser,
    /// F3 : retire le filtre (J8, H-UI-30, S79).
    ClearFilter,
    /// Maj+F3 : ordre des ID (J8, H-UI-31, S75).
    SortById,
    /// Ctrl/Cmd+F : barre de recherche (J8, H-UI-33, S61).
    Find,
    /// Ctrl/Cmd+G : correspondance suivante (J8, H-UI-33).
    FindNext,
    /// Ctrl/Cmd+Maj+G : correspondance précédente (J8, H-UI-33).
    FindPrevious,
    /// Alt+Maj+− : masque les sous-tâches (J8, H-UI-34, S61).
    HideSubtasks,
    /// Alt+Maj++ ou = : affiche les sous-tâches (J8, H-UI-34, S61).
    ShowSubtasks,
    /// Ctrl/Cmd+F1 : replie ou déplie le ruban (ADR-0017, H-UI-72, S83).
    ToggleRibbon,
    // --- Passe UX (ADR-0018, contrat L0 ; H-UI-92, H-UI-96, H-UI-97).
    /// Ctrl/Cmd+A, Ctrl+Maj+Espace : tout sélectionner, en lignes entières.
    SelectAll,
    /// Maj+Espace : lignes entières de la sélection.
    SelectRows,
    /// Ctrl+Espace (Ctrl aussi sur macOS) : colonnes entières de la
    /// sélection.
    SelectColumns,
    /// Maj+Retour arrière : réduit la sélection à la cellule active.
    CollapseSelection,
    /// Ctrl/Cmd+D : remplir vers le bas (R-258).
    FillDown,
    /// Ctrl/Cmd+Suppr : effacer le contenu du bloc (R-259).
    ClearCells,
    // --- Seconde passe UX (ADR-0019, contrat L0 ; H-UI-109, H-UI-111,
    // H-UI-115, H-UI-120).
    /// Ctrl/Cmd+W, Ctrl+F4 (hors macOS) : fermer le document, gardé.
    CloseDocument,
    /// Ctrl/Cmd+P : mode impression (« Mise en page » en mode impression).
    PrintMode,
    /// Ctrl+Maj+F5 : défiler jusqu'à la tâche.
    ScrollToTask,
    /// Ctrl/Cmd+R : remplir à droite (R-258 amendée).
    FillRight,
    /// Alt+F3 : « Paramètres de champ » de la colonne active.
    FieldSettings,
    /// F6 : volet suivant.
    FocusNextPane,
    /// Maj+F6 : volet précédent.
    FocusPreviousPane,
    // --- Finitions avant v1.0.0 (ADR-0022, lot P2, H-UI-127).
    /// Ctrl/Cmd+B : gras du texte des tâches sélectionnées.
    ToggleBold,
    /// Ctrl/Cmd+I : italique (finitions P4, H-UI-135).
    ToggleItalic,
    // --- Lot A5 d'ADR-0024 (H-UI-162).
    /// Ctrl/Cmd+J : ouvre (focus sur la saisie) ou ferme le volet de
    /// l'assistant ; lu aussi pendant une saisie de texte (`App::read_keys`).
    ToggleAssistant,
}

/// Liaison d'un raccourci.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Binding {
    /// Commande du registre, par son id (`history.undo`).
    Command(&'static str),
    /// Action de l'interface.
    Ui(UiBinding),
}

/// Ctrl/Cmd + Maj.
const COMMAND_SHIFT: Modifiers = Modifiers {
    shift: true,
    ..Modifiers::COMMAND
};

/// Alt + Maj.
const ALT_SHIFT: Modifiers = Modifiers {
    alt: true,
    shift: true,
    ..Modifiers::NONE
};

/// Alt.
const ALT: Modifiers = Modifiers::ALT;

/// Maj.
const SHIFT: Modifiers = Modifiers::SHIFT;

/// Ctrl (Cmd sur macOS).
const COMMAND: Modifiers = Modifiers::COMMAND;

/// Ctrl, sur macOS aussi (Ctrl+Espace, H-UI-92).
const CTRL: Modifiers = Modifiers::CTRL;

/// Aucun modificateur.
const NONE: Modifiers = Modifiers::NONE;

/// Table des raccourcis, la combinaison la plus précise d'une touche
/// d'abord.
pub const KEYMAP: &[(KeyContext, Shortcut, Binding)] = &[
    (
        KeyContext::Document,
        Shortcut::new(COMMAND_SHIFT, Key::Z),
        Binding::Command("history.redo"),
    ),
    (
        KeyContext::Document,
        Shortcut::new(Modifiers::COMMAND, Key::Z),
        Binding::Command("history.undo"),
    ),
    (
        KeyContext::Document,
        Shortcut::new(Modifiers::COMMAND, Key::Y),
        Binding::Command("history.redo"),
    ),
    (
        KeyContext::Global,
        Shortcut::new(Modifiers::COMMAND, Key::O),
        Binding::Ui(UiBinding::OpenFile),
    ),
    (
        KeyContext::Global,
        Shortcut::new(Modifiers::COMMAND, Key::Q),
        Binding::Ui(UiBinding::Quit),
    ),
    (
        KeyContext::Global,
        Shortcut::new(Modifiers::COMMAND, Key::N),
        Binding::Command("project.new"),
    ),
    (
        KeyContext::Document,
        Shortcut::new(COMMAND_SHIFT, Key::S),
        Binding::Ui(UiBinding::SaveAs),
    ),
    (
        KeyContext::Document,
        Shortcut::new(Modifiers::COMMAND, Key::S),
        Binding::Command("project.save"),
    ),
    // Structure (lot L14), sur la sélection de la vue principale.
    (
        KeyContext::Document,
        Shortcut::new(Modifiers::NONE, Key::Insert),
        Binding::Command("task.add"),
    ),
    // Insérer sans touche Inser (clavier Mac, relecture finale J7, F6) :
    // Cmd+Maj+I (Ctrl+Maj+I ailleurs) depuis les finitions P4 (H-UI-135,
    // décision du propriétaire du 2026-11-06 : Ctrl/Cmd+I = italique comme
    // Office ; Cmd+Entrée, demandé, est déjà l'acceptation du bandeau de
    // l'agent et le remplissage du bloc en saisie), affiché dans les menus
    // sur macOS. Avant Ctrl/Cmd+I : egui ignore Maj en trop.
    (
        KeyContext::Document,
        Shortcut::new(COMMAND_SHIFT, Key::I),
        Binding::Command("task.add"),
    ),
    // Finitions P4 (H-UI-135) : italique en Ctrl/Cmd+I (P2 : Ctrl/Cmd+Maj+I).
    doc_ui(COMMAND, Key::I, UiBinding::ToggleItalic),
    // Ctrl/Cmd+Suppr : effacer le contenu (ADR-0018, H-UI-97, S61 « Clear
    // or reset the selected field. »), avant Suppr.
    table_ui(COMMAND, Key::Delete, UiBinding::ClearCells),
    // Suppr seul : `Action::Cells(CellOp::DeleteKey)` (`entry_action`),
    // supprimer ou effacer selon la cible (H-UI-97) ; le raccourci reste
    // affiché pour « Supprimer la tâche ».
    (
        KeyContext::Document,
        Shortcut::new(Modifiers::NONE, Key::Delete),
        Binding::Command("task.delete"),
    ),
    // Clavier Mac sans Suppr : Cmd+delete (Cmd+Retour arrière), comme
    // « Placer dans la corbeille » du Finder. Retour arrière seul ne
    // supprime plus la tâche (relecture finale J7, F6, contre la relecture
    // finale J4, F5) : il ouvre la saisie vide de la cellule courante (lu
    // par la table, comme F2).
    (
        KeyContext::Document,
        Shortcut::new(Modifiers::COMMAND, Key::Backspace),
        Binding::Command("task.delete"),
    ),
    // Maj+Retour arrière : cellule active seule (S61 « Reduce the selection
    // to one field. », ADR-0018).
    table_ui(SHIFT, Key::Backspace, UiBinding::CollapseSelection),
    (
        KeyContext::Document,
        Shortcut::new(ALT_SHIFT, Key::ArrowRight),
        Binding::Command("task.indent"),
    ),
    (
        KeyContext::Document,
        Shortcut::new(ALT_SHIFT, Key::ArrowLeft),
        Binding::Command("task.outdent"),
    ),
    (
        KeyContext::Document,
        Shortcut::new(COMMAND_SHIFT, Key::F2),
        Binding::Command("link.remove"),
    ),
    (
        KeyContext::Document,
        Shortcut::new(Modifiers::COMMAND, Key::F2),
        Binding::Command("link.add"),
    ),
    // J7 L9 : « Informations sur la tâche » de la tâche active (S61,
    // H-UI-19), après Ctrl+Maj+F2 et Ctrl+F2 ; F2 seul reste la saisie de
    // la cellule (lu par la table, après les raccourcis).
    doc_ui(SHIFT, Key::F2, UiBinding::TaskInformation),
    // --- J7 L1 : navigation (H-UI-13 à H-UI-16), échelle de temps
    // (H-UI-18), déplacement des lignes (H-UI-29). Maj et Alt en trop étant
    // ignorés par egui, chaque touche liste ses combinaisons de la plus
    // précise à la moins précise.
    (
        KeyContext::Document,
        Shortcut::new(ALT_SHIFT, Key::ArrowUp),
        Binding::Command("task.move"),
    ),
    nav(COMMAND_SHIFT, Key::ArrowUp, NavMove::FirstRow, true),
    nav(COMMAND, Key::ArrowUp, NavMove::FirstRow, false),
    nav(SHIFT, Key::ArrowUp, NavMove::Up, true),
    nav(NONE, Key::ArrowUp, NavMove::Up, false),
    (
        KeyContext::Document,
        Shortcut::new(ALT_SHIFT, Key::ArrowDown),
        Binding::Command("task.move"),
    ),
    nav(COMMAND_SHIFT, Key::ArrowDown, NavMove::LastRow, true),
    nav(COMMAND, Key::ArrowDown, NavMove::LastRow, false),
    nav(SHIFT, Key::ArrowDown, NavMove::Down, true),
    nav(NONE, Key::ArrowDown, NavMove::Down, false),
    doc_ui(ALT, Key::ArrowLeft, UiBinding::TimescaleStep(-1)),
    // ADR-0018 (H-UI-92) : Maj+←/→ et Ctrl+Maj+←/→ étendent la sélection
    // (en colonnes à partir du lot L1), avant ←/→ et Ctrl+←/→.
    nav(COMMAND_SHIFT, Key::ArrowLeft, NavMove::RowStart, true),
    nav(COMMAND, Key::ArrowLeft, NavMove::RowStart, false),
    nav(SHIFT, Key::ArrowLeft, NavMove::Left, true),
    nav(NONE, Key::ArrowLeft, NavMove::Left, false),
    doc_ui(ALT, Key::ArrowRight, UiBinding::TimescaleStep(1)),
    nav(COMMAND_SHIFT, Key::ArrowRight, NavMove::RowEnd, true),
    nav(COMMAND, Key::ArrowRight, NavMove::RowEnd, false),
    nav(SHIFT, Key::ArrowRight, NavMove::Right, true),
    nav(NONE, Key::ArrowRight, NavMove::Right, false),
    // Entrée et Tab circulent dans un bloc de plus d'une cellule (ADR-0018,
    // H-UI-93, lot L1) ; ailleurs, comme ↓, ↑, →, ← (`NavMove::base`).
    nav(SHIFT, Key::Tab, NavMove::Within(InBlockMove::Left), false),
    nav(NONE, Key::Tab, NavMove::Within(InBlockMove::Right), false),
    nav(SHIFT, Key::Enter, NavMove::Within(InBlockMove::Up), false),
    nav(NONE, Key::Enter, NavMove::Within(InBlockMove::Down), false),
    doc_ui(ALT, Key::Home, UiBinding::TimescaleToStart),
    // S61 : « Extend the selection to the first field of the first row. »
    // (Ctrl+Maj+Origine), « … to the first field in a row. » (Maj+Origine).
    nav(COMMAND_SHIFT, Key::Home, NavMove::First, true),
    nav(COMMAND, Key::Home, NavMove::First, false),
    nav(SHIFT, Key::Home, NavMove::RowStart, true),
    nav(NONE, Key::Home, NavMove::RowStart, false),
    doc_ui(ALT, Key::End, UiBinding::TimescaleToEnd),
    nav(COMMAND_SHIFT, Key::End, NavMove::Last, true),
    nav(COMMAND, Key::End, NavMove::Last, false),
    nav(SHIFT, Key::End, NavMove::RowEnd, true),
    nav(NONE, Key::End, NavMove::RowEnd, false),
    doc_ui(ALT, Key::PageUp, UiBinding::TimescalePage(-1)),
    nav(SHIFT, Key::PageUp, NavMove::PageUp, true),
    nav(NONE, Key::PageUp, NavMove::PageUp, false),
    doc_ui(ALT, Key::PageDown, UiBinding::TimescalePage(1)),
    nav(SHIFT, Key::PageDown, NavMove::PageDown, true),
    nav(NONE, Key::PageDown, NavMove::PageDown, false),
    // « Ctrl+/ » du pavé numérique (S61) ; Ctrl+= et Ctrl++ en plus (pas de
    // « * » du pavé dans `egui::Key`) ; pris avant le zoom de l'interface
    // d'egui (Ctrl+±, traité en fin d'image), document ouvert seulement.
    doc_ui(COMMAND, Key::Slash, UiBinding::ZoomFiner),
    doc_ui(COMMAND, Key::Equals, UiBinding::ZoomFiner),
    doc_ui(COMMAND, Key::Plus, UiBinding::ZoomFiner),
    doc_ui(COMMAND, Key::Minus, UiBinding::ZoomCoarser),
    // --- J8 L9 : filtres, tri, recherche, repli (S61, S75, S79 ; H-UI-30 à
    // H-UI-34). Maj+F3 avant F3 ; Ctrl/Cmd+Maj+G avant Ctrl/Cmd+G.
    table_ui(ALT, Key::F3, UiBinding::FieldSettings),
    doc_ui(SHIFT, Key::F3, UiBinding::SortById),
    doc_ui(NONE, Key::F3, UiBinding::ClearFilter),
    doc_ui(COMMAND, Key::F, UiBinding::Find),
    doc_ui(COMMAND_SHIFT, Key::G, UiBinding::FindPrevious),
    doc_ui(COMMAND, Key::G, UiBinding::FindNext),
    doc_ui(ALT_SHIFT, Key::Minus, UiBinding::HideSubtasks),
    doc_ui(ALT_SHIFT, Key::Plus, UiBinding::ShowSubtasks),
    doc_ui(ALT_SHIFT, Key::Equals, UiBinding::ShowSubtasks),
    // --- J8 L10 ---
    // --- J8 L12 ---
    // --- J8 L13 ---
    // --- J8 L15 ---
    // --- Ruban (ADR-0017, L0) : Ctrl+F1 (Cmd+F1 sur macOS) replie ou
    // déplie le ruban (S83), partout, accueil compris.
    (
        KeyContext::Global,
        Shortcut::new(COMMAND, Key::F1),
        Binding::Ui(UiBinding::ToggleRibbon),
    ),
    // --- Passe UX (ADR-0018, contrat L0) : sélection et remplissage, table
    // seulement. Ctrl+Maj+Espace avant Maj+Espace et Ctrl+Espace (egui
    // ignore Maj en trop : Ctrl+Espace répondrait à Ctrl+Maj+Espace).
    table_ui(COMMAND, Key::A, UiBinding::SelectAll),
    table_ui(COMMAND_SHIFT, Key::Space, UiBinding::SelectAll),
    table_ui(SHIFT, Key::Space, UiBinding::SelectRows),
    table_ui(CTRL, Key::Space, UiBinding::SelectColumns),
    table_ui(COMMAND, Key::D, UiBinding::FillDown),
    // --- Seconde passe UX (ADR-0019, contrat L0) : fermer le document
    // (S95 « Close window. » Cmd+W ; Ctrl+F4 hors macOS), mode impression
    // (S95 « Print workbook. »), défiler jusqu'à la tâche, remplir à droite
    // (S95 « Fill cells right. »), « Paramètres de champ » (S61 « Display
    // the Field Settings dialog box. » Alt+F3, avant F3 : egui ignore Alt
    // en trop), volets (F6 ; Maj+F6 d'abord).
    (
        KeyContext::Global,
        Shortcut::new(COMMAND, Key::W),
        Binding::Ui(UiBinding::CloseDocument),
    ),
    (
        KeyContext::Global,
        Shortcut::new(CTRL, Key::F4),
        Binding::Ui(UiBinding::CloseDocument),
    ),
    (
        KeyContext::Global,
        Shortcut::new(COMMAND, Key::P),
        Binding::Ui(UiBinding::PrintMode),
    ),
    doc_ui(COMMAND_SHIFT, Key::F5, UiBinding::ScrollToTask),
    table_ui(COMMAND, Key::R, UiBinding::FillRight),
    doc_ui(SHIFT, Key::F6, UiBinding::FocusPreviousPane),
    doc_ui(NONE, Key::F6, UiBinding::FocusNextPane),
    // --- Finitions P2 (ADR-0022, H-UI-127) : gras en Ctrl/Cmd+B
    // (hypothèse : convention d'Office ; touche libre dans cette table).
    doc_ui(COMMAND, Key::B, UiBinding::ToggleBold),
    // --- Lot A5 d'ADR-0024 (H-UI-162) : volet de l'assistant, partout,
    // accueil compris (hypothèse : convention de « Ctrl+J » des volets de
    // conversation ; touche libre dans cette table). Sur le web, retenue
    // au navigateur (Chrome et Firefox : téléchargements) quand l'image la
    // lit, saisie de texte comprise (`KeyScope::Typing`).
    (
        KeyContext::Global,
        Shortcut::new(COMMAND, Key::J),
        Binding::Ui(UiBinding::ToggleAssistant),
    ),
];

/// Raccourcis que le navigateur ne rend jamais à la page (Ctrl/Cmd+N, W,
/// T, Tab : H-UI-55) : jamais marqués interceptables.
pub const WEB_NEVER: &[Key] = &[Key::N, Key::W, Key::T, Key::Tab];

/// Vrai si l'entrée de [`KEYMAP`] doit être retenue au navigateur sur le
/// web (H-UI-55 : Ctrl/Cmd+F, G, F3, Alt+←/→, Ctrl/Cmd+=…), pour que
/// PlannerCraft la reçoive au lieu de l'action du navigateur (recherche dans la
/// page, page précédente, zoom de la page…).
///
/// Écart au contrat écrit (« `Binding` gagne `web_interceptable` ») :
/// `Binding` est une énumération et [`KEYMAP`] une table de triplets lus
/// par les tests ; le drapeau est cette fonction (lot L13). Toute entrée
/// l'est, sauf :
/// - les touches de [`WEB_NEVER`] (Ctrl/Cmd+N, W, T, Tab) : le navigateur
///   les garde de toute façon (nouvelle fenêtre, onglet), Tab reste à
///   eframe ;
/// - « Quitter » : une page ne ferme pas son onglet, et Ctrl/Cmd+Q doit
///   rester au navigateur ;
/// - fermer le document, mode impression et remplir à droite (seconde
///   passe UX, H-UI-109) : Ctrl/Cmd+W, Ctrl+F4, Ctrl/Cmd+P et Ctrl/Cmd+R
///   restent au navigateur (fermer l'onglet, imprimer, recharger).
#[must_use]
pub fn web_interceptable(shortcut: &Shortcut, binding: Binding) -> bool {
    !WEB_NEVER.contains(&shortcut.logical_key)
        && !matches!(
            binding,
            Binding::Ui(
                UiBinding::Quit | UiBinding::CloseDocument | UiBinding::PrintMode | UiBinding::FillRight
            )
        )
}

/// L'entrée vaut-elle sur le système `os` ? Ctrl+F4 (fermer le document)
/// hors macOS seulement (H-UI-109) ; toutes les autres partout.
#[must_use]
pub fn os_allows(shortcut: &Shortcut, os: egui::os::OperatingSystem) -> bool {
    !(os == egui::os::OperatingSystem::Mac && shortcut.logical_key == Key::F4)
}

/// Raccourcis lus par l'image courante, vus du navigateur (H-UI-55) :
/// photographie de [`crate::App::key_scope`] après chaque image, lue par
/// la garde `keydown` de la version web.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyScope {
    /// Aucun raccourci lu (saisie de texte, fenêtre modale, cellule en
    /// cours d'édition) : tout reste au navigateur.
    #[default]
    Nothing,
    /// Fenêtre non modale ouverte : seuls les raccourcis gardés à côté
    /// d'elle (enregistrer, annuler, ouvrir…).
    BesideWindow(KeyState),
    /// Saisie de texte en cours (cellule, recherche, question de
    /// l'assistant), sans fenêtre : seul Ctrl/Cmd+J (volet de
    /// l'assistant, lot A5) ; tout le reste au navigateur.
    Typing,
    /// Tous les raccourcis des contextes actifs.
    All(KeyState),
}

impl KeyScope {
    /// L'entrée est-elle lue par l'image dans ce périmètre ?
    #[must_use]
    pub fn reads(self, context: KeyContext, binding: Binding) -> bool {
        match self {
            Self::Nothing => false,
            Self::Typing => binding == Binding::Ui(UiBinding::ToggleAssistant),
            Self::BesideWindow(keys) => keys.allows(context) && BESIDE_WINDOW.contains(&binding),
            Self::All(keys) => keys.allows(context),
        }
    }
}

/// Garde des raccourcis du web (H-UI-55) : vrai si l'appui `key` avec
/// `modifiers` (tels qu'eframe les lit dans l'évènement `keydown`) doit
/// être retenu au navigateur (`preventDefault`). La première entrée de
/// [`KEYMAP`] lue dans `scope` qui correspond décide, comme
/// [`dispatch`] (comparaison « logique » d'egui : Maj et Alt en trop
/// ignorés) ; aucune entrée ⇒ faux, le navigateur garde la touche
/// (Ctrl/Cmd+C, V, X compris : le presse-papiers passe par ses
/// évènements).
#[must_use]
pub fn web_intercepts(scope: KeyScope, modifiers: Modifiers, key: Key) -> bool {
    KEYMAP
        .iter()
        .find(|(context, shortcut, binding)| {
            scope.reads(*context, *binding)
                && shortcut.logical_key == key
                && modifiers.matches_logically(shortcut.modifiers)
        })
        .is_some_and(|(_, shortcut, binding)| web_interceptable(shortcut, *binding))
}

/// État du clavier pour une image : contextes actifs de [`KEYMAP`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyState {
    /// Un document est ouvert.
    pub document: bool,
    /// La table a le focus, sans saisie, menu ni fenêtre ouverts.
    pub table: bool,
}

impl KeyState {
    /// Le contexte est-il actif ?
    #[must_use]
    pub const fn allows(self, context: KeyContext) -> bool {
        match context {
            KeyContext::Global => true,
            KeyContext::Document => self.document,
            KeyContext::Table => self.document && self.table,
        }
    }
}

/// Action d'une entrée de [`KEYMAP`] : celle de sa liaison
/// ([`binding_action`]), sauf `task.move` dont le sens vient de la touche
/// (Alt+Maj+↑ monte, Alt+Maj+↓ descend, H-UI-29), et Suppr sans
/// modificateur, qui supprime ou efface selon la cible
/// (`Action::Cells(CellOp::DeleteKey)`, H-UI-97 ; les menus gardent
/// `task.delete`).
#[must_use]
pub fn entry_action(shortcut: &Shortcut, binding: Binding) -> Option<Action> {
    match (binding, shortcut.logical_key) {
        (Binding::Command("task.move"), Key::ArrowUp) => Some(Action::Structure(StructureOp::MoveUp)),
        (Binding::Command("task.move"), Key::ArrowDown) => Some(Action::Structure(StructureOp::MoveDown)),
        (Binding::Command("task.delete"), Key::Delete) if shortcut.modifiers == Modifiers::NONE => {
            Some(Action::Cells(CellOp::DeleteKey))
        }
        _ => binding_action(binding),
    }
}

/// Action de l'interface pour une liaison ; `None` pour un id du registre
/// que l'interface ne sait pas exécuter sans la touche (`task.move`, voir
/// [`entry_action`]) ou pas encore du tout (un test garantit qu'aucune
/// entrée de [`KEYMAP`] n'est sans action).
#[must_use]
pub fn binding_action(binding: Binding) -> Option<Action> {
    match binding {
        Binding::Command("history.undo") => Some(Action::Undo),
        Binding::Command("history.redo") => Some(Action::Redo),
        Binding::Command("project.new") => Some(Action::NewProject),
        Binding::Command("project.save") => Some(Action::Save),
        Binding::Command("task.add") => Some(Action::Structure(StructureOp::InsertBefore)),
        Binding::Command("task.delete") => Some(Action::Structure(StructureOp::Delete)),
        Binding::Command("task.indent") => Some(Action::Structure(StructureOp::Indent)),
        Binding::Command("task.outdent") => Some(Action::Structure(StructureOp::Outdent)),
        Binding::Command("link.add") => Some(Action::Structure(StructureOp::Link)),
        Binding::Command("link.remove") => Some(Action::Structure(StructureOp::Unlink)),
        Binding::Command(_) => None,
        Binding::Ui(ui) => Some(ui_action(ui)),
    }
}

/// Action d'une liaison propre à l'interface : toute [`UiBinding`] en a
/// une (`keymap_bindings_resolve`).
#[must_use]
pub const fn ui_action(binding: UiBinding) -> Action {
    match binding {
        UiBinding::OpenFile => Action::OpenDialog,
        UiBinding::SaveAs => Action::SaveAs,
        UiBinding::Quit => Action::Quit,
        UiBinding::Nav { mv, extend } => Action::Nav { mv, extend },
        UiBinding::TaskInformation => Action::ShowTaskInformation,
        UiBinding::TimescaleStep(n) => Action::TimescaleStep(n),
        UiBinding::TimescalePage(n) => Action::TimescalePage(n),
        UiBinding::TimescaleToStart => Action::TimescaleToStart,
        UiBinding::TimescaleToEnd => Action::TimescaleToEnd,
        UiBinding::ZoomFiner => Action::ZoomBy(1),
        UiBinding::ZoomCoarser => Action::ZoomBy(-1),
        UiBinding::ClearFilter => Action::TaskView(TaskViewOp::ClearFilter),
        UiBinding::SortById => Action::TaskView(TaskViewOp::ResetSort),
        UiBinding::Find => Action::TaskView(TaskViewOp::OpenSearch),
        UiBinding::FindNext => Action::TaskView(TaskViewOp::SearchNext),
        UiBinding::FindPrevious => Action::TaskView(TaskViewOp::SearchPrev),
        UiBinding::HideSubtasks => Action::TaskView(TaskViewOp::HideSubtasks),
        UiBinding::ShowSubtasks => Action::TaskView(TaskViewOp::ShowSubtasks),
        UiBinding::ToggleRibbon => Action::Ribbon(crate::ribbon::RibbonOp::ToggleCollapsed),
        UiBinding::SelectAll => Action::SelectOp(SelectOp::All),
        UiBinding::SelectRows => Action::SelectOp(SelectOp::EntireRows),
        UiBinding::SelectColumns => Action::SelectOp(SelectOp::EntireColumns),
        UiBinding::CollapseSelection => Action::SelectOp(SelectOp::Collapse),
        UiBinding::FillDown => Action::Cells(CellOp::FillDown),
        UiBinding::ClearCells => Action::Cells(CellOp::Clear),
        UiBinding::CloseDocument => Action::CloseDocumentGuarded,
        UiBinding::PrintMode => Action::ShowDialog(crate::action::DialogKind::PageSetup { print_mode: true }),
        UiBinding::ScrollToTask => Action::ScrollToTask,
        UiBinding::FillRight => Action::Cells(CellOp::FillRight),
        UiBinding::FieldSettings => Action::Column(crate::action::ColumnOp::OpenSettings(
            crate::action::ACTIVE_COLUMN,
        )),
        UiBinding::FocusNextPane => Action::Pane(crate::action::PaneOp::FocusNext { back: false }),
        UiBinding::FocusPreviousPane => Action::Pane(crate::action::PaneOp::FocusNext { back: true }),
        UiBinding::ToggleBold => Action::TextStyle(crate::action::TextStyleOp::ToggleBold),
        UiBinding::ToggleItalic => Action::TextStyle(crate::action::TextStyleOp::ToggleItalic),
        UiBinding::ToggleAssistant => Action::SidePanel(crate::view::side_panel::SideOp::Toggle),
    }
}

/// Texte d'un raccourci pour les menus (« Ctrl+Z », « ⌘Z »).
#[must_use]
pub fn shortcut_text(ctx: &egui::Context, shortcut: &Shortcut) -> String {
    ctx.format_shortcut(shortcut)
}

/// Premier raccourci de [`KEYMAP`] lié à `binding` (affiché dans les menus).
#[must_use]
pub fn shortcut_for(binding: Binding) -> Option<&'static Shortcut> {
    KEYMAP
        .iter()
        .find(|(_, _, entry)| *entry == binding)
        .map(|(_, shortcut, _)| shortcut)
}

/// Raccourci affiché dans les menus pour `binding` sur le système `os` : sur
/// macOS, le premier qui n'utilise ni Inser ni Suppr (touches absentes d'un
/// clavier Mac : Cmd+Maj+I, Cmd+delete) ; ailleurs, le premier.
#[must_use]
pub fn shortcut_for_os(binding: Binding, os: egui::os::OperatingSystem) -> Option<&'static Shortcut> {
    let mac_key = |shortcut: &Shortcut| !matches!(shortcut.logical_key, Key::Insert | Key::Delete);
    let matching = || {
        KEYMAP
            .iter()
            .filter(move |(_, _, entry)| *entry == binding)
            .map(|(_, shortcut, _)| shortcut)
    };
    if os == egui::os::OperatingSystem::Mac {
        matching()
            .find(|shortcut| mac_key(shortcut))
            .or_else(|| matching().next())
    } else {
        matching().next()
    }
}

/// Liaisons gardées à côté d'une fenêtre non modale (« Affecter des
/// ressources ») : à Ctrl/Cmd, sans effet sur la structure ni la sélection
/// (la fenêtre relit la sélection ; Suppr ou Inser pendant une saisie
/// d'unités, ADR-0015 L13, restent exclus).
const BESIDE_WINDOW: [Binding; 7] = [
    Binding::Command("history.undo"),
    Binding::Command("history.redo"),
    Binding::Command("project.save"),
    Binding::Command("project.new"),
    Binding::Ui(UiBinding::SaveAs),
    Binding::Ui(UiBinding::OpenFile),
    Binding::Ui(UiBinding::Quit),
];

/// Raccourcis de [`BESIDE_WINDOW`] seulement (fenêtre non modale ouverte,
/// aucun de ses champs focalisé : l'appelant le vérifie), comme
/// [`dispatch`] (relecture finale J7, F9 : Cmd+S, Cmd+Z et Cmd+Q étaient
/// perdus tant que la fenêtre était ouverte).
pub fn dispatch_beside_window(ctx: &egui::Context, keys: KeyState, actions: &mut Vec<Action>) {
    for (context, shortcut, binding) in KEYMAP {
        if !keys.allows(*context) || !BESIDE_WINDOW.contains(binding) {
            continue;
        }
        if ctx.input_mut(|input| input.consume_shortcut(shortcut))
            && let Some(action) = entry_action(shortcut, *binding)
        {
            actions.push(action);
        }
    }
}

/// Appuis d'une même touche gardés par image (répétition du clavier) :
/// au-delà, ignorés (une image lente ne fait pas sauter la table).
const MAX_PRESSES_PER_FRAME: usize = 8;

/// Lit les raccourcis de l'image et pousse leurs actions dans `actions`
/// (touches consommées : aucun widget ne les reçoit ensuite), une par
/// appui (répétition comprise). Rien pendant la saisie dans un champ de
/// texte.
pub fn dispatch(ctx: &egui::Context, keys: KeyState, actions: &mut Vec<Action>) {
    if ctx.text_edit_focused() {
        return;
    }
    let os = ctx.os();
    for (context, shortcut, binding) in KEYMAP {
        if !keys.allows(*context) || !os_allows(shortcut, os) {
            continue;
        }
        let presses =
            ctx.input_mut(|input| input.count_and_consume_key(shortcut.modifiers, shortcut.logical_key));
        if presses == 0 {
            continue;
        }
        match entry_action(shortcut, *binding) {
            Some(action) => {
                for _ in 0..presses.min(MAX_PRESSES_PER_FRAME) {
                    actions.push(action.clone());
                }
            }
            None => log::warn!("raccourci sans action : {binding:?}"),
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::expect_used,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use egui::Key;

    use super::{Binding, KEYMAP, KeyContext, KeyState, UiBinding, binding_action, entry_action, ui_action};
    use crate::action::Action;
    use crate::nav::NavMove;
    use crate::structure::StructureOp;

    /// Modificateurs de `a` contenant ceux de `b` (au moins autant de
    /// touches enfoncées).
    fn covers(a: egui::Modifiers, b: egui::Modifiers) -> bool {
        (a.alt || !b.alt)
            && (a.ctrl || !b.ctrl)
            && (a.shift || !b.shift)
            && (a.mac_cmd || !b.mac_cmd)
            && (a.command || !b.command)
    }

    #[test]
    fn keymap_most_specific_first_and_unique() {
        for (i, (_, earlier, _)) in KEYMAP.iter().enumerate() {
            for (_, later, _) in &KEYMAP[i + 1..] {
                assert_ne!(earlier, later, "raccourci en double : {earlier:?}");
                if earlier.logical_key == later.logical_key {
                    assert!(
                        !covers(later.modifiers, earlier.modifiers) || later.modifiers == earlier.modifiers,
                        "{later:?} doit précéder {earlier:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn keymap_every_binding_has_action() {
        for (_, shortcut, binding) in KEYMAP {
            assert!(
                entry_action(shortcut, *binding).is_some(),
                "{shortcut:?} → {binding:?}"
            );
        }
        // `task.move` : le sens vient de la touche.
        assert!(binding_action(Binding::Command("task.move")).is_none());
    }

    /// H-UI-29 : Alt+Maj+↑/↓ (`task.move`) avant Maj+↑/↓ et ↑/↓ (egui
    /// ignore Alt en trop : sinon Alt+Maj+↑ étendrait la sélection).
    #[test]
    fn keymap_alt_shift_arrows_before_shift_arrows() {
        for (key, op) in [
            (Key::ArrowUp, StructureOp::MoveUp),
            (Key::ArrowDown, StructureOp::MoveDown),
        ] {
            let entries: Vec<usize> = KEYMAP
                .iter()
                .enumerate()
                .filter(|(_, (_, shortcut, _))| shortcut.logical_key == key)
                .map(|(i, _)| i)
                .collect();
            let (_, first, binding) = KEYMAP[entries[0]];
            assert_eq!(first.modifiers, super::ALT_SHIFT, "{key:?}");
            assert_eq!(binding, Binding::Command("task.move"));
            assert_eq!(
                entry_action(&first, binding),
                Some(crate::action::Action::Structure(op))
            );
            let shift = entries
                .iter()
                .position(|i| KEYMAP[*i].1.modifiers == super::SHIFT)
                .expect("Maj+flèche");
            assert!(shift > 0);
        }
    }

    /// Contextes : la table n'est active qu'avec un document et le focus.
    #[test]
    fn keymap_table_context_needs_focus_and_document() {
        let none = KeyState::default();
        assert!(none.allows(KeyContext::Global));
        assert!(!none.allows(KeyContext::Document));
        assert!(!none.allows(KeyContext::Table));
        let doc = KeyState {
            document: true,
            table: false,
        };
        assert!(doc.allows(KeyContext::Document) && !doc.allows(KeyContext::Table));
        let table = KeyState {
            document: true,
            table: true,
        };
        assert!(table.allows(KeyContext::Table));
        let orphan = KeyState {
            document: false,
            table: true,
        };
        assert!(!orphan.allows(KeyContext::Table));
    }

    /// Contrat J7 (lot L0) : chaque `UiBinding` a son action.
    #[test]
    fn keymap_bindings_resolve() {
        let all = [
            UiBinding::OpenFile,
            UiBinding::SaveAs,
            UiBinding::Quit,
            UiBinding::Nav {
                mv: NavMove::Down,
                extend: true,
            },
            UiBinding::TaskInformation,
            UiBinding::TimescaleStep(-1),
            UiBinding::TimescalePage(1),
            UiBinding::TimescaleToStart,
            UiBinding::TimescaleToEnd,
            UiBinding::ZoomFiner,
            UiBinding::ZoomCoarser,
        ];
        for binding in all {
            assert_eq!(binding_action(Binding::Ui(binding)), Some(ui_action(binding)));
        }
        assert_eq!(
            ui_action(UiBinding::Nav {
                mv: NavMove::PageDown,
                extend: false
            }),
            Action::Nav {
                mv: NavMove::PageDown,
                extend: false
            }
        );
        assert_eq!(ui_action(UiBinding::ZoomFiner), Action::ZoomBy(1));
        assert_eq!(ui_action(UiBinding::ZoomCoarser), Action::ZoomBy(-1));
    }

    /// ADR-0017 (L0), H-UI-72 : Ctrl/Cmd+F1 résolu (contexte global) vers
    /// le repli du ruban, retenu au navigateur sur le web (H-UI-55), et
    /// affiché dans les menus.
    #[test]
    fn keymap_ctrl_f1_toggles_ribbon() {
        use super::{KeyScope, shortcut_for, web_interceptable, web_intercepts};
        use crate::ribbon::RibbonOp;

        let binding = Binding::Ui(UiBinding::ToggleRibbon);
        let (context, shortcut, _) = KEYMAP
            .iter()
            .find(|(_, s, _)| s.logical_key == Key::F1)
            .copied()
            .expect("Ctrl+F1 dans KEYMAP");
        assert_eq!(context, KeyContext::Global);
        assert_eq!(shortcut.modifiers, egui::Modifiers::COMMAND);
        assert_eq!(shortcut_for(binding), Some(&shortcut));
        assert_eq!(
            entry_action(&shortcut, binding),
            Some(Action::Ribbon(RibbonOp::ToggleCollapsed))
        );
        assert!(web_interceptable(&shortcut, binding));
        let scope = KeyScope::All(KeyState::default());
        assert!(web_intercepts(scope, egui::Modifiers::COMMAND, Key::F1));
        assert!(!web_intercepts(
            KeyScope::Nothing,
            egui::Modifiers::COMMAND,
            Key::F1
        ));
    }

    /// Relecture finale J7, F6 : sur macOS, les menus montrent Cmd+Maj+I
    /// (finitions P4, H-UI-135 ; Cmd+I avant) et
    /// Cmd+delete (pas Inser ni Suppr) ; ailleurs, Inser et Suppr ; Retour
    /// arrière seul n'est plus lié à `task.delete`.
    #[test]
    fn keymap_mac_insert_delete_shown() {
        use egui::os::OperatingSystem;

        use super::shortcut_for_os;

        let add = Binding::Command("task.add");
        let delete = Binding::Command("task.delete");
        let mac_add = shortcut_for_os(add, OperatingSystem::Mac).expect("raccourci affiché");
        assert_eq!(mac_add.logical_key, Key::I);
        assert_eq!(mac_add.modifiers, super::COMMAND_SHIFT, "Cmd+Maj+I (H-UI-135)");
        assert_eq!(
            shortcut_for_os(delete, OperatingSystem::Mac)
                .expect("raccourci affiché")
                .logical_key,
            Key::Backspace
        );
        assert_eq!(
            shortcut_for_os(add, OperatingSystem::Nix)
                .expect("raccourci affiché")
                .logical_key,
            Key::Insert
        );
        assert_eq!(
            shortcut_for_os(delete, OperatingSystem::Windows)
                .expect("raccourci affiché")
                .logical_key,
            Key::Delete
        );
        assert!(
            !KEYMAP.iter().any(|(_, s, b)| s.logical_key == Key::Backspace
                && s.modifiers == egui::Modifiers::NONE
                && *b == delete),
            "Retour arrière seul supprime encore la tâche"
        );
    }

    /// Première entrée de [`KEYMAP`] lue pour l'appui `key` avec
    /// `modifiers` (table focalisée, document ouvert), comme [`super::dispatch`]
    /// (comparaison « logique » d'egui).
    fn resolve(modifiers: egui::Modifiers, key: Key) -> Option<(KeyContext, Binding, Option<Action>)> {
        KEYMAP
            .iter()
            .find(|(_, shortcut, _)| {
                shortcut.logical_key == key && modifiers.matches_logically(shortcut.modifiers)
            })
            .map(|(context, shortcut, binding)| (*context, *binding, entry_action(shortcut, *binding)))
    }

    /// Ctrl seul (Linux, Windows) : `ctrl` et `command`.
    const CTRL_PRESSED: egui::Modifiers = egui::Modifiers {
        ctrl: true,
        command: true,
        ..egui::Modifiers::NONE
    };

    /// ADR-0018 (contrat L0, H-UI-92, H-UI-96, H-UI-97) : chaque nouvelle
    /// entrée est résolue vers son action, avant les entrées moins précises
    /// de la même touche, retenue au navigateur sur le web ; Suppr seul
    /// passe par `CellOp::DeleteKey`, Cmd+Retour arrière garde la
    /// suppression ; Ctrl+Espace est Ctrl sur macOS aussi.
    #[test]
    fn keymap_ux_bindings_resolve() {
        use super::{SHIFT, web_interceptable};
        use crate::action::{CellOp, SelectOp};

        let shift_ctrl = egui::Modifiers {
            shift: true,
            ..CTRL_PRESSED
        };
        let cases = [
            (
                CTRL_PRESSED,
                Key::A,
                UiBinding::SelectAll,
                Action::SelectOp(SelectOp::All),
            ),
            (
                shift_ctrl,
                Key::Space,
                UiBinding::SelectAll,
                Action::SelectOp(SelectOp::All),
            ),
            (
                SHIFT,
                Key::Space,
                UiBinding::SelectRows,
                Action::SelectOp(SelectOp::EntireRows),
            ),
            (
                CTRL_PRESSED,
                Key::Space,
                UiBinding::SelectColumns,
                Action::SelectOp(SelectOp::EntireColumns),
            ),
            (
                SHIFT,
                Key::Backspace,
                UiBinding::CollapseSelection,
                Action::SelectOp(SelectOp::Collapse),
            ),
            (
                CTRL_PRESSED,
                Key::D,
                UiBinding::FillDown,
                Action::Cells(CellOp::FillDown),
            ),
            (
                CTRL_PRESSED,
                Key::Delete,
                UiBinding::ClearCells,
                Action::Cells(CellOp::Clear),
            ),
        ];
        for (modifiers, key, binding, action) in cases {
            let (context, found, resolved) = resolve(modifiers, key).expect("entrée trouvée");
            assert_eq!(context, KeyContext::Table, "{key:?}");
            assert_eq!(found, Binding::Ui(binding), "{modifiers:?} {key:?}");
            assert_eq!(resolved, Some(action.clone()));
            assert_eq!(ui_action(binding), action);
            assert_eq!(binding_action(Binding::Ui(binding)), Some(action));
            let (_, shortcut, _) = KEYMAP
                .iter()
                .find(|(_, s, b)| *b == Binding::Ui(binding) && s.logical_key == key)
                .expect("entrée");
            assert!(web_interceptable(shortcut, Binding::Ui(binding)), "{key:?}");
        }
        // Ctrl+Espace : Ctrl, pas Cmd, sur macOS (Cmd+Espace reste au
        // système).
        let mac_ctrl = egui::Modifiers {
            ctrl: true,
            ..egui::Modifiers::NONE
        };
        let mac_cmd = egui::Modifiers::MAC_CMD;
        assert_eq!(
            resolve(mac_ctrl, Key::Space).map(|(_, b, _)| b),
            Some(Binding::Ui(UiBinding::SelectColumns))
        );
        assert!(resolve(mac_cmd, Key::Space).is_none());
        // Suppr : supprimer ou effacer selon la cible ; le menu garde
        // `task.delete` ; Cmd+Retour arrière supprime toujours.
        let (_, binding, action) = resolve(egui::Modifiers::NONE, Key::Delete).expect("Suppr");
        assert_eq!(binding, Binding::Command("task.delete"));
        assert_eq!(action, Some(Action::Cells(CellOp::DeleteKey)));
        assert_eq!(
            binding_action(Binding::Command("task.delete")),
            Some(Action::Structure(StructureOp::Delete))
        );
        let (_, _, action) = resolve(CTRL_PRESSED, Key::Backspace).expect("Ctrl+Retour arrière");
        assert_eq!(action, Some(Action::Structure(StructureOp::Delete)));
        // Retour arrière seul reste à la table (saisie vide, lue après).
        assert!(resolve(egui::Modifiers::NONE, Key::Backspace).is_none());
        // Ordre : la plus précise d'abord.
        let index = |m: egui::Modifiers, k: Key| {
            KEYMAP
                .iter()
                .position(|(_, s, _)| s.modifiers == m && s.logical_key == k)
                .expect("entrée")
        };
        assert!(index(super::COMMAND_SHIFT, Key::Space) < index(super::CTRL, Key::Space));
        assert!(index(super::COMMAND, Key::Delete) < index(super::NONE, Key::Delete));
    }

    /// U02 (H-UI-92) : Maj+→ ne résout plus `Right` non étendu ; Maj+←,
    /// Maj+Origine/Fin, Ctrl+Maj+Origine/Fin et Ctrl+Maj+←/→ étendent ; les
    /// touches sans Maj sont inchangées ; Alt+Maj+← reste « Désindenter ».
    #[test]
    fn keymap_shift_arrow_extends() {
        use super::SHIFT;

        let nav = |mv, extend| Some(Action::Nav { mv, extend });
        let shift_ctrl = egui::Modifiers {
            shift: true,
            ..CTRL_PRESSED
        };
        let cases = [
            (SHIFT, Key::ArrowRight, nav(NavMove::Right, true)),
            (SHIFT, Key::ArrowLeft, nav(NavMove::Left, true)),
            (SHIFT, Key::Home, nav(NavMove::RowStart, true)),
            (SHIFT, Key::End, nav(NavMove::RowEnd, true)),
            (shift_ctrl, Key::Home, nav(NavMove::First, true)),
            (shift_ctrl, Key::End, nav(NavMove::Last, true)),
            (shift_ctrl, Key::ArrowLeft, nav(NavMove::RowStart, true)),
            (shift_ctrl, Key::ArrowRight, nav(NavMove::RowEnd, true)),
            (egui::Modifiers::NONE, Key::ArrowRight, nav(NavMove::Right, false)),
            (CTRL_PRESSED, Key::ArrowRight, nav(NavMove::RowEnd, false)),
            (CTRL_PRESSED, Key::Home, nav(NavMove::First, false)),
            (egui::Modifiers::NONE, Key::Home, nav(NavMove::RowStart, false)),
        ];
        for (modifiers, key, want) in cases {
            let (_, _, action) = resolve(modifiers, key).expect("entrée");
            assert_eq!(action, want, "{modifiers:?} {key:?}");
        }
        let (_, binding, _) = resolve(super::ALT_SHIFT, Key::ArrowLeft).expect("Alt+Maj+←");
        assert_eq!(binding, Binding::Command("task.outdent"));
    }

    /// Seconde passe UX (contrat L0 ; H-UI-109, H-UI-111, H-UI-115,
    /// H-UI-120) : chaque entrée nouvelle est résolue vers son action et son
    /// contexte ; Ctrl/Cmd+W, Ctrl+F4, Ctrl/Cmd+P et Ctrl/Cmd+R restent au
    /// navigateur (H-UI-109), Ctrl+Maj+F5, Alt+F3 et F6 lui sont retenus ;
    /// Alt+F3 passe avant F3 ; Ctrl+F4 hors macOS seulement.
    #[test]
    fn keymap_ux2_bindings_resolve() {
        use egui::os::OperatingSystem;

        use super::{ALT, SHIFT, os_allows, shortcut_for, web_interceptable};
        use crate::action::{ACTIVE_COLUMN, CellOp, ColumnOp, DialogKind, PaneOp};

        let shift_ctrl = egui::Modifiers {
            shift: true,
            ..CTRL_PRESSED
        };
        let cases = [
            (
                CTRL_PRESSED,
                Key::W,
                KeyContext::Global,
                UiBinding::CloseDocument,
                Action::CloseDocumentGuarded,
                false,
            ),
            (
                CTRL_PRESSED,
                Key::F4,
                KeyContext::Global,
                UiBinding::CloseDocument,
                Action::CloseDocumentGuarded,
                false,
            ),
            (
                CTRL_PRESSED,
                Key::P,
                KeyContext::Global,
                UiBinding::PrintMode,
                Action::ShowDialog(DialogKind::PageSetup { print_mode: true }),
                false,
            ),
            (
                shift_ctrl,
                Key::F5,
                KeyContext::Document,
                UiBinding::ScrollToTask,
                Action::ScrollToTask,
                true,
            ),
            (
                CTRL_PRESSED,
                Key::R,
                KeyContext::Table,
                UiBinding::FillRight,
                Action::Cells(CellOp::FillRight),
                false,
            ),
            (
                ALT,
                Key::F3,
                KeyContext::Table,
                UiBinding::FieldSettings,
                Action::Column(ColumnOp::OpenSettings(ACTIVE_COLUMN)),
                true,
            ),
            (
                egui::Modifiers::NONE,
                Key::F6,
                KeyContext::Document,
                UiBinding::FocusNextPane,
                Action::Pane(PaneOp::FocusNext { back: false }),
                true,
            ),
            (
                SHIFT,
                Key::F6,
                KeyContext::Document,
                UiBinding::FocusPreviousPane,
                Action::Pane(PaneOp::FocusNext { back: true }),
                true,
            ),
        ];
        for (modifiers, key, context, binding, action, web) in cases {
            let (found_context, found, resolved) = resolve(modifiers, key).expect("entrée trouvée");
            assert_eq!(found_context, context, "{key:?}");
            assert_eq!(found, Binding::Ui(binding), "{modifiers:?} {key:?}");
            assert_eq!(resolved, Some(action.clone()));
            assert_eq!(ui_action(binding), action);
            let (_, shortcut, _) = KEYMAP
                .iter()
                .find(|(_, s, b)| *b == Binding::Ui(binding) && s.logical_key == key)
                .expect("entrée");
            assert_eq!(web_interceptable(shortcut, Binding::Ui(binding)), web, "{key:?}");
            assert_eq!(os_allows(shortcut, OperatingSystem::Mac), key != Key::F4);
            assert!(os_allows(shortcut, OperatingSystem::Windows));
            assert!(shortcut_for(Binding::Ui(binding)).is_some());
        }
        // Alt+F3 avant Maj+F3 et F3 (egui ignore Alt en trop).
        let index = |m: egui::Modifiers, k: Key| {
            KEYMAP
                .iter()
                .position(|(_, s, _)| s.modifiers == m && s.logical_key == k)
                .expect("entrée")
        };
        assert!(index(ALT, Key::F3) < index(egui::Modifiers::NONE, Key::F3));
        assert!(index(SHIFT, Key::F6) < index(egui::Modifiers::NONE, Key::F6));
        // F3 seul reste « retirer le filtre ».
        let (_, binding, _) = resolve(egui::Modifiers::NONE, Key::F3).expect("F3");
        assert_eq!(binding, Binding::Ui(UiBinding::ClearFilter));
    }
}
