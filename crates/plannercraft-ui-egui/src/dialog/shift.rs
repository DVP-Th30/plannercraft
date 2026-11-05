//! « Décaler les tâches » (finitions P1, H-UI-121) : décalage personnalisé
//! de Décaler ▾ → Personnalisé…. Une zone de saisie d'une durée **signée**
//! (« 3j », « -2sem », « +1j ») lue par `plannercraft_automation`
//! (`parse_lag`, R-142) ; « OK » envoie le lot `task.shift` de la sélection
//! (R-267), un refus reste affiché dans la fenêtre.

use plannercraft_automation::command::DurationJson;
use plannercraft_automation::text::{Language, TextContext, parse_lag};
use plannercraft_core::{LagValue, Project};

use super::task_info::Outcome;
use super::{error_line, text_field};
use crate::edit::EditError;
use crate::i18n::{TextId, Texts};

/// Brouillon de la fenêtre.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShiftState {
    /// Décalage saisi.
    pub text: String,
    /// Refus (saisie illisible ou registre).
    pub error: Option<String>,
}

impl ShiftState {
    /// Décalage JSON de la saisie, contrôlée d'abord par le lecteur des
    /// décalages (signe, nombre, unité) : le texte part tel quel, le
    /// registre le relit dans la même langue.
    ///
    /// # Errors
    ///
    /// Message traduit : saisie vide, illisible, ou en pourcentage.
    pub fn amount(&self, project: &Project, texts: &Texts, lang: Language) -> Result<DurationJson, String> {
        let text = self.text.trim();
        if text.is_empty() {
            return Err(texts.get(TextId::ShiftEmpty));
        }
        let ctx = TextContext::for_project(project, lang);
        let parsed = parse_lag(text, &ctx).map_err(|e| EditError::from(e).message(texts))?;
        if matches!(parsed.value, LagValue::Percent { .. }) {
            return Err(texts.get(TextId::ShiftNoPercent));
        }
        Ok(DurationJson {
            text: Some(text.to_owned()),
            ..DurationJson::default()
        })
    }
}

/// Identifiant de la fenêtre (couche modale d'egui).
#[must_use]
pub fn modal_id() -> egui::Id {
    egui::Id::unique("plannercraft.shift")
}

/// Dessine la fenêtre (une image) ; rend la réponse de l'utilisateur.
pub fn show(ctx: &egui::Context, texts: &Texts, state: &mut ShiftState) -> Outcome {
    let mut outcome = Outcome::Open;
    let armed = super::enter_armed(ctx, None);
    let response = egui::Modal::new(modal_id()).show(ctx, |ui| {
        ui.set_width(380.0);
        ui.heading(texts.get(TextId::ShiftTitle));
        let label = texts.get(TextId::ShiftBy);
        ui.horizontal(|ui| {
            ui.label(&label);
            let field = text_field(ui, &label, &mut state.text, true, 140.0);
            if state.text.is_empty() && !field.has_focus() && !field.lost_focus() {
                field.request_focus();
            }
        });
        ui.label(texts.get(TextId::ShiftHint));
        error_line(ui, state.error.as_deref());
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
