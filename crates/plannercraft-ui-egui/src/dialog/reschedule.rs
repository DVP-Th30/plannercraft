//! « Replanifier le travail restant » (J7, lot L12, coupable) :
//! `progress.reschedule` (R-217) après une date (défaut : la date d'état
//! effective, R-219), sur le projet entier ou sur les tâches sélectionnées
//! à l'ouverture. La date proposée est celle du projet, écrite par le
//! moteur ; laissée telle quelle, le lot ne la donne pas (le registre prend
//! la date d'état effective).

use plannercraft_automation::command::ProgressReschedule;
use plannercraft_automation::text::{DateRole, Language, TextContext, format_date, parse_date};
use plannercraft_automation::{Batch, Command, Session, TaskRef};
use plannercraft_core::TaskUid;

use super::task_info::Outcome;
use super::{batch_of, error_line, text_field};
use crate::edit::EditError;
use crate::i18n::{Arg, TextId, Texts};

/// Brouillon de la fenêtre.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RescheduleState {
    /// Clé du document à l'ouverture.
    pub key: (u64, u64),
    /// Date saisie.
    pub after: String,
    /// Date proposée (date d'état effective).
    original: String,
    /// Tâches sélectionnées seulement.
    pub selection_only: bool,
    /// Tâches sélectionnées à l'ouverture.
    pub selected: Vec<TaskUid>,
    /// Refus (date illisible ou registre).
    pub error: Option<String>,
    /// Le document avait changé : rien appliqué.
    pub stale: bool,
}

impl RescheduleState {
    /// Fenêtre relue sur la session ; date proposée : date d'état effective.
    #[must_use]
    pub fn open(s: &Session, key: (u64, u64), lang: Language, selected: Vec<TaskUid>) -> Self {
        let p = s.project();
        let ctx = TextContext::for_project(p, lang);
        let date = format_date(p.effective_status_date(), &ctx);
        Self {
            key,
            original: date.clone(),
            after: date,
            selection_only: false,
            selected,
            error: None,
            stale: false,
        }
    }

    /// Lot de « OK » : `progress.reschedule` ; `after` absent si la date
    /// proposée n'a pas changé.
    ///
    /// # Errors
    ///
    /// Message traduit si la date est illisible.
    pub fn to_batch(&self, s: &Session, texts: &Texts, lang: Language) -> Result<Batch, String> {
        let after = if self.after.trim() == self.original.trim() {
            None
        } else {
            let ctx = TextContext::for_project(s.project(), lang);
            Some(
                parse_date(&self.after, DateRole::Start, &ctx)
                    .map_err(|e| EditError::from(e).message(texts))?,
            )
        };
        let tasks = (self.selection_only && !self.selected.is_empty())
            .then(|| self.selected.iter().copied().map(TaskRef::Uid).collect());
        Ok(batch_of(
            vec![Command::ProgressReschedule(ProgressReschedule { after, tasks })],
            lang,
        ))
    }
}

/// Identifiant de la fenêtre (couche modale d'egui).
#[must_use]
pub fn modal_id() -> egui::Id {
    egui::Id::unique("plannercraft.reschedule")
}

/// Dessine la fenêtre (une image) ; rend la réponse de l'utilisateur.
pub fn show(ctx: &egui::Context, texts: &Texts, state: &mut RescheduleState) -> Outcome {
    let mut outcome = Outcome::Open;
    let armed = super::enter_armed(ctx, None);
    let response = egui::Modal::new(modal_id()).show(ctx, |ui| {
        ui.set_width(420.0);
        ui.heading(texts.get(TextId::RsTitle));
        if state.stale {
            ui.colored_label(ui.visuals().warn_fg_color, texts.get(TextId::TaskInfoStale));
        }
        let label = texts.get(TextId::RsAfter);
        ui.horizontal(|ui| {
            ui.label(&label);
            text_field(ui, &label, &mut state.after, true, 200.0);
        });
        ui.radio_value(&mut state.selection_only, false, texts.get(TextId::ScopeProject));
        let count = i64::try_from(state.selected.len()).unwrap_or(i64::MAX);
        ui.add_enabled_ui(!state.selected.is_empty(), |ui| {
            ui.radio_value(
                &mut state.selection_only,
                true,
                texts.get_args(TextId::ScopeSelection, &[("count", Arg::Count(count))]),
            );
        });
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
