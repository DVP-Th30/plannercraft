//! Quitter et fermer (seconde passe UX, ADR-0019 D4, H-UI-108, H-UI-109).
//!
//! [`App::global_keys`] est appelée en tête de la lecture des raccourcis de
//! chaque image (`App::read_keys`), avant les fenêtres, la saisie de
//! cellule et le `KEYMAP` : Ctrl/Cmd+Q (Quitter), Ctrl/Cmd+W et Ctrl+F4
//! hors macOS (Fermer le document) et Ctrl/Cmd+P (mode impression) y sont
//! lus dans **tous** les états. Avant l'action, l'état en cours est réglé
//! comme le ferait l'utilisateur ([`App::settle_for_global`]) :
//!
//! - saisie de cellule ouverte (table des tâches ou feuille des
//!   ressources) : validée comme par Entrée (H-UI-86) ; refusée ⇒ le refus
//!   s'affiche sous la cellule, la saisie reste ouverte et rien d'autre ne
//!   se passe (on reste) ;
//! - formulaire du volet de détails modifié : OK implicite (H-UI-113,
//!   servi par le lot L5 dans `App::details_op`) ;
//! - fenêtre ouverte (« Informations », « Collage spécial », « Mise en
//!   page »…, « Trier », « Filtre ») ou confirmation de suppression :
//!   fermée comme par Annuler ;
//!
//! puis `Action::Quit` (garde des modifications : bandeau « Enregistrer les
//! modifications ? » si le document est modifié), `Action::CloseDocumentGuarded`
//! ou « Mise en page » en mode impression.
//!
//! Le raccourci de sortie intégré d'egui (`egui::Options::quit_shortcuts`,
//! Cmd+Q ⇒ `ViewportCommand::Close` en fin d'image, hors de toute garde) est
//! vidé à la première image ([`install_quit_policy`]).
//!
//! [`App::shell_quit`] est la demande du menu natif macOS (« Quitter
//! PlannerCraft », `apps/plannercraft`) : `Action::Quit` mise en file, réglée par la
//! même [`App::settle_for_global`] à l'image suivante (même chemin que
//! Cmd+Q).
//!
//! Web : rien n'est pris ici (Ctrl/Cmd+W, P et Q restent au navigateur,
//! `keymap::web_interceptable` ; le `KEYMAP` garde son comportement).
//!
//! Journal des touches (`PLANNERCRAFT_LOG_KEYS=1`, desktop) : chaque appui reçu
//! par l'image (touche, touche physique, modificateurs) est écrit par
//! `log::info!` sur la cible `plannercraft::keys` ; l'application desktop installe
//! le journaliseur qui l'imprime sur la sortie d'erreur.

use crate::action::{Action, DetailsOp, DialogKind};
use crate::app::App;
use crate::i18n::TextId;
use crate::keymap::{Binding, KEYMAP, KeyContext, UiBinding, os_allows};

// Web : rien n'est pris ici (voir le module) ; le réglage de l'état et la
// lecture des touches ne servent qu'au desktop.
#[cfg(target_arch = "wasm32")]
const _: () = {
    let _ = take_global_keys;
    let _ = surrender_editor;
};

/// Demande globale lue par [`App::global_keys`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlobalRequest {
    /// Ctrl/Cmd+Q : quitter (gardé).
    Quit,
    /// Ctrl/Cmd+W, Ctrl+F4 (hors macOS) : fermer le document (gardé).
    CloseDocument,
    /// Ctrl/Cmd+P : « Mise en page » en mode impression.
    PrintMode,
}

impl GlobalRequest {
    /// Demande d'une liaison du `KEYMAP` (`None` : touche non globale).
    #[must_use]
    pub const fn from_binding(binding: Binding) -> Option<Self> {
        match binding {
            Binding::Ui(UiBinding::Quit) => Some(Self::Quit),
            Binding::Ui(UiBinding::CloseDocument) => Some(Self::CloseDocument),
            Binding::Ui(UiBinding::PrintMode) => Some(Self::PrintMode),
            _ => None,
        }
    }

    /// Action poussée une fois l'état réglé.
    #[must_use]
    pub const fn action(self) -> Action {
        match self {
            Self::Quit => Action::Quit,
            Self::CloseDocument => Action::CloseDocumentGuarded,
            Self::PrintMode => Action::ShowDialog(DialogKind::PageSetup { print_mode: true }),
        }
    }

    /// La demande a-t-elle un sens sans document ? Quitter oui ; fermer et
    /// imprimer non (touche prise, rien fait).
    #[must_use]
    pub const fn needs_document(self) -> bool {
        !matches!(self, Self::Quit)
    }
}

/// Coupe le raccourci de sortie intégré d'egui (Cmd+Q ⇒
/// `ViewportCommand::Close` sans garde) : la sortie passe par
/// [`App::global_keys`] seulement (H-UI-108). Idempotent : rien n'est
/// écrit quand la liste est déjà vide.
pub fn install_quit_policy(ctx: &egui::Context) {
    if ctx.options(|options| !options.quit_shortcuts.is_empty()) {
        ctx.options_mut(|options| options.quit_shortcuts.clear());
    }
}

/// Lit (et consomme) les touches globales de l'image sur le système `os` ;
/// rend la première demande trouvée dans l'ordre du `KEYMAP` (Quitter
/// d'abord). Toutes les touches globales de l'image sont consommées : aucune
/// n'atteint ensuite un widget ni le `KEYMAP`.
fn take_global_keys(ctx: &egui::Context) -> Option<GlobalRequest> {
    let os = ctx.os();
    let mut found = None;
    for (context, shortcut, binding) in KEYMAP {
        if *context != KeyContext::Global || !os_allows(shortcut, os) {
            continue;
        }
        let Some(request) = GlobalRequest::from_binding(*binding) else {
            continue;
        };
        let presses =
            ctx.input_mut(|input| input.count_and_consume_key(shortcut.modifiers, shortcut.logical_key));
        if presses > 0 && found.is_none() {
            found = Some(request);
        }
    }
    found
}

/// Journal des touches demandé (`PLANNERCRAFT_LOG_KEYS=1`), lu une fois.
#[cfg(not(target_arch = "wasm32"))]
fn key_log_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("PLANNERCRAFT_LOG_KEYS").is_some_and(|value| value == "1"))
}

/// Écrit les appuis de l'image au journal (cible `plannercraft::keys`).
#[cfg(not(target_arch = "wasm32"))]
fn log_keys(ctx: &egui::Context) {
    if !key_log_enabled() {
        return;
    }
    ctx.input(|input| {
        for event in &input.events {
            if let egui::Event::Key {
                key,
                physical_key,
                pressed: true,
                repeat,
                modifiers,
            } = event
            {
                log::info!(
                    target: "plannercraft::keys",
                    "touche {key:?} (physique {physical_key:?}) modificateurs {modifiers:?} répétition {repeat} os {:?}",
                    ctx.os()
                );
            }
        }
    });
}

#[cfg_attr(
    target_arch = "wasm32",
    allow(
        dead_code,
        reason = "web : réglage de l'état propre au desktop (voir le module)"
    )
)]
impl App {
    /// Touches globales de sortie et de fermeture, lues avant tout autre
    /// raccourci (voir le module) ; vrai si l'image ne doit lire aucun
    /// autre raccourci (touche prise, ou « Quitter » du menu natif en
    /// attente). `actions` est la file de l'image : `Action::Quit` mise par
    /// [`App::shell_quit`] y est déjà.
    pub(crate) fn global_keys(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) -> bool {
        install_quit_policy(ctx);
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (&self.view, actions);
            false
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            log_keys(ctx);
            // « Quitter » venu d'ailleurs que du clavier (menu natif macOS,
            // `shell_quit`) : même réglage que Cmd+Q, avant l'action.
            if let Some(at) = actions.iter().position(|action| matches!(action, Action::Quit)) {
                let quit = actions.remove(at);
                if self.settle_for_global(ctx, actions) {
                    actions.push(quit);
                }
                return true;
            }
            let Some(request) = take_global_keys(ctx) else {
                return false;
            };
            if request.needs_document() && self.document.is_none() {
                return true;
            }
            if self.settle_for_global(ctx, actions) {
                actions.push(request.action());
            }
            true
        }
    }

    /// Règle l'état avant une demande globale (voir le module) : saisie
    /// validée (lot poussé dans `actions`, avant la demande), formulaire du
    /// volet validé, fenêtres fermées comme par Annuler. Faux si la saisie
    /// est refusée : la demande n'est pas faite (on reste, le refus est
    /// affiché sous la cellule).
    pub(crate) fn settle_for_global(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) -> bool {
        if !self.commit_open_cell(ctx, actions) {
            return false;
        }
        // Volet de détails (H-UI-113) : OK implicite ; refus ⇒ on reste,
        // brouillon et refus affichés (H-UI-108).
        if !self.details_op(DetailsOp::Ok) {
            return false;
        }
        self.cancel_windows();
        true
    }

    /// Valide la saisie de cellule ouverte (table des tâches, feuille des
    /// ressources), comme Entrée sans déplacement : lot poussé dans
    /// `actions` s'il n'est pas vide, saisie fermée. Faux si la saisie est
    /// refusée (refus affiché sous la cellule, saisie gardée, focus rendu).
    fn commit_open_cell(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) -> bool {
        let lang = self.text_language();
        if let Some(state) = self.view.edit.editing.as_mut() {
            let Some(doc) = self.document.as_ref() else {
                self.view.edit.editing = None;
                return true;
            };
            match crate::edit::commit_edit(state, &doc.session, lang) {
                Ok(batch) => {
                    let opening = doc.opening;
                    self.view.edit.editing = None;
                    self.view.edit.focus_pending = false;
                    surrender_editor(ctx);
                    if !batch.ops.is_empty() {
                        actions.push(Action::ExecOn { opening, batch });
                    }
                }
                Err(error) => {
                    state.error = Some(error);
                    self.view.edit.focus_pending = true;
                    return false;
                }
            }
        }
        if self.view.resources.edit.editing.is_some() {
            let Some(doc) = self.document.as_ref() else {
                self.view.resources.edit.editing = None;
                return true;
            };
            let not_editable = self.texts.get(TextId::EditErrorNotEditable);
            let sheet = crate::resource_rows::ResourceSheet::new(&self.view.resources.rows, &self.texts);
            let Some(state) = self.view.resources.edit.editing.as_ref() else {
                return true;
            };
            match crate::grid::commit_cell(&sheet, state, &doc.session, lang, &not_editable) {
                Ok(batch) => {
                    let opening = doc.opening;
                    self.view.resources.edit.editing = None;
                    self.view.resources.edit.focus_pending = false;
                    surrender_editor(ctx);
                    if let Some(batch) = batch {
                        actions.push(Action::ExecOn { opening, batch });
                    }
                }
                Err(message) => {
                    if let Some(state) = self.view.resources.edit.editing.as_mut() {
                        state.error = Some(message);
                    }
                    self.view.resources.edit.focus_pending = true;
                    return false;
                }
            }
        }
        true
    }

    /// Ferme, comme par Annuler (ou Échap), la fenêtre ouverte, les
    /// fenêtres « Trier » et « Filtre », le sélecteur de champ (et
    /// « Paramètres de champ ») et les confirmations de suppression ; la
    /// table reprend le focus.
    fn cancel_windows(&mut self) {
        let open = self.view.dialog.is_some()
            || self.view_dialog_open()
            || self.view.picker.is_some()
            || self.pending_delete.is_some()
            || self.view.resources.pending_delete.is_some();
        self.close_dialog();
        self.view.sort_dialog = None;
        self.view.filter_dialog = None;
        self.view.picker = None;
        self.pending_delete = None;
        self.view.resources.pending_delete = None;
        if open && self.document.is_some() {
            self.view.focus_table_pending = true;
        }
    }

    /// « Quitter » du menu applicatif natif (macOS, `apps/plannercraft`) : même
    /// chemin que Ctrl/Cmd+Q (`Action::Quit`, réglée par
    /// [`App::settle_for_global`] puis garde des modifications), appliqué à
    /// la fin de la prochaine image.
    pub fn shell_quit(&mut self) {
        self.actions.push(Action::Quit);
    }
}

/// Rend le focus de la zone de saisie des grilles (les raccourcis
/// redeviennent actifs).
fn surrender_editor(ctx: &egui::Context) {
    ctx.memory_mut(|memory| memory.surrender_focus(crate::grid::editor_id()));
}

/// Textes posés par le lot L0 pour le menu natif macOS et « Fermer le
/// document », servis par les lots L3 (menu natif, `apps/plannercraft`) et L10.
pub const L3_TEXTS: [TextId; 9] = [
    TextId::MacMenuAbout,
    TextId::MacMenuHide,
    TextId::MacMenuHideOthers,
    TextId::MacMenuShowAll,
    TextId::MacMenuQuit,
    TextId::MacMenuWindow,
    TextId::MacMenuMinimize,
    TextId::MacMenuZoom,
    TextId::MenuCloseDocument,
];
