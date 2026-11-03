//! Options du collage (R-244 : ressources par nom, liens internes) et
//! compte rendu (`paste.links_dropped`). Contrat posé par le lot L0,
//! implémenté en L10.
//!
//! « Collage spécial… » (menu Édition) colle le **tampon interne** (dernière
//! copie de l'application) avec les options choisies : recréer ou non les
//! liens entre les tâches collées ; affectations par nom de ressource
//! (Correspondance : omises si la ressource n'existe pas ; Créer : ressource
//! de travail créée ; Ignorer : aucune affectation). « OK » produit un seul
//! lot, comme Ctrl+V ; le compte rendu va au bandeau.

use plannercraft_automation::clip::{PasteOptions, ResourcePaste};

use super::task_info::Outcome;
use crate::app::App;
use crate::i18n::{Arg, TextId, Texts};

/// Brouillon de la fenêtre.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasteSpecialState {
    /// Options choisies.
    pub options: PasteOptions,
    /// Nombre de tâches du tampon (affiché).
    pub tasks: usize,
}

impl PasteSpecialState {
    /// Fenêtre sur un tampon de `tasks` tâches, options par défaut.
    #[must_use]
    pub fn open(tasks: usize) -> Self {
        Self {
            options: PasteOptions::default(),
            tasks,
        }
    }
}

/// Choix des ressources, avec leur libellé.
const RESOURCE_CHOICES: [(ResourcePaste, TextId); 3] = [
    (ResourcePaste::Match, TextId::PsResMatch),
    (ResourcePaste::MatchOrCreate, TextId::PsResCreate),
    (ResourcePaste::Drop, TextId::PsResDrop),
];

/// Identifiant de la fenêtre (couche modale d'egui).
#[must_use]
pub fn modal_id() -> egui::Id {
    egui::Id::unique("plannercraft.paste-special")
}

/// Dessine la fenêtre (une image) ; rend la réponse de l'utilisateur.
pub fn show(ctx: &egui::Context, texts: &Texts, state: &mut PasteSpecialState) -> Outcome {
    let mut outcome = Outcome::Open;
    let armed = super::enter_armed(ctx, None);
    let response = egui::Modal::new(modal_id()).show(ctx, |ui| {
        ui.set_width(460.0);
        ui.heading(texts.get(TextId::PsTitle));
        let count = i64::try_from(state.tasks).unwrap_or(i64::MAX);
        ui.label(texts.get_args(TextId::PsSource, &[("count", Arg::Count(count))]));
        ui.checkbox(&mut state.options.links, texts.get(TextId::PsLinks));
        ui.label(texts.get(TextId::PsResources));
        for (choice, label) in RESOURCE_CHOICES {
            ui.radio_value(&mut state.options.resources, choice, texts.get(label));
        }
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button(texts.get(TextId::TaskInfoOk)).clicked() {
                outcome = Outcome::Ok;
            }
            if ui.button(texts.get(TextId::TaskInfoCancel)).clicked() {
                outcome = Outcome::Cancel;
            }
        });
    });
    let escape = response.is_top_modal
        && !response.any_popup_open
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if escape {
        outcome = Outcome::Cancel;
    }
    if outcome == Outcome::Open
        && super::enter_pressed(ctx, armed, response.is_top_modal, response.any_popup_open)
    {
        outcome = Outcome::Ok;
    }
    outcome
}

impl App {
    /// Ouvre « Collage spécial… » sur le tampon interne (sans document :
    /// rien ; sans tampon : invitation à copier d'abord, au bandeau).
    pub(crate) fn open_paste_special(&mut self) {
        if self.document.is_none() {
            return;
        }
        let Some(slot) = &self.clipboard else {
            let text = self.texts.get(TextId::PsNothing);
            self.push_titled(
                plannercraft_core::Severity::Info,
                Some(TextId::PasteReportTitle),
                text,
            );
            return;
        };
        let body = super::DialogBody::PasteSpecial(Box::new(PasteSpecialState::open(slot.clip.tasks.len())));
        self.view.edit.editing = None;
        self.view.drag = None;
        self.view.dialog = Some(super::DialogState {
            kind: crate::action::DialogKind::PasteSpecial,
            body,
        });
    }

    /// Dessine « Collage spécial… » et applique sa réponse : « OK » ferme
    /// la fenêtre puis colle le tampon avec les options choisies.
    pub(crate) fn show_paste_special(&mut self, ctx: &egui::Context) {
        let Some(super::DialogState {
            body: super::DialogBody::PasteSpecial(state),
            ..
        }) = &mut self.view.dialog
        else {
            return;
        };
        let outcome = show(ctx, &self.texts, state);
        let options = state.options;
        match outcome {
            Outcome::Open => {}
            Outcome::Cancel => self.close_dialog(),
            Outcome::Ok => {
                self.close_dialog();
                self.paste_internal(options);
            }
        }
    }
}
