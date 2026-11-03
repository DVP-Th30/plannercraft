//! « Ligne de base » (J7, lot L12 ; brief § 6, S58/S72) : Définir
//! (`baseline.set`, R-227) ou Effacer (`baseline.clear`, R-250) la ligne de
//! base 0 à 10, sur le projet entier ou sur les tâches sélectionnées à
//! l'ouverture, avec le cumul vers les récapitulatives pour une sélection
//! (Définir seulement). « OK » produit un seul lot.
//!
//! Remplacement (R-223) : le premier « OK » part sans `overwrite` ; si le
//! registre répond `baseline.exists`, la fenêtre affiche son message et
//! demande confirmation ; « Remplacer » renvoie le même lot avec
//! `overwrite`. L'interface ne cherche pas elle-même si la ligne existe.

use plannercraft_automation::command::{BaselineClear, BaselineSet};
use plannercraft_automation::json::RollupJson;
use plannercraft_automation::text::Language;
use plannercraft_automation::{Batch, Command, TaskRef};
use plannercraft_core::TaskUid;

use super::batch_of;
use super::error_line;
use super::task_info::Outcome;
use crate::i18n::{Arg, TextId, Texts};

/// Action de la fenêtre.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BaselineAction {
    /// Définir la ligne de base.
    #[default]
    Set,
    /// Effacer la ligne de base.
    Clear,
}

/// Choix de cumul proposés (S58).
pub const ROLLUPS: [(RollupJson, TextId); 3] = [
    (RollupJson::None, TextId::BlRollupNone),
    (RollupJson::ToAllSummaries, TextId::BlRollupAll),
    (RollupJson::FromSubtasks, TextId::BlRollupFrom),
];

/// Brouillon de la fenêtre.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaselineState {
    /// Clé du document à l'ouverture.
    pub key: (u64, u64),
    /// Définir ou effacer.
    pub action: BaselineAction,
    /// Numéro de la ligne (0 à 10).
    pub number: u8,
    /// Tâches sélectionnées seulement (sinon projet entier).
    pub selection_only: bool,
    /// Tâches sélectionnées à l'ouverture (ordre des UID).
    pub selected: Vec<TaskUid>,
    /// Cumul (Définir sur une sélection).
    pub rollup: RollupJson,
    /// Remplacement confirmé.
    pub overwrite: bool,
    /// Message de `baseline.exists` en attente de confirmation.
    pub confirm: Option<String>,
    /// Refus du registre.
    pub error: Option<String>,
    /// Le document avait changé : rien appliqué.
    pub stale: bool,
}

impl BaselineState {
    /// Fenêtre sur la sélection `selected` (vide : projet entier seul).
    #[must_use]
    pub fn open(key: (u64, u64), selected: Vec<TaskUid>) -> Self {
        Self {
            key,
            action: BaselineAction::Set,
            number: 0,
            selection_only: false,
            selected,
            rollup: RollupJson::None,
            overwrite: false,
            confirm: None,
            error: None,
            stale: false,
        }
    }

    /// Lot de « OK » (ou de « Remplacer »).
    #[must_use]
    pub fn to_batch(&self, lang: Language) -> Batch {
        let tasks = (self.selection_only && !self.selected.is_empty())
            .then(|| self.selected.iter().copied().map(TaskRef::Uid).collect());
        let op = match self.action {
            BaselineAction::Set => Command::BaselineSet(BaselineSet {
                number: Some(self.number),
                rollup: (tasks.is_some() && self.rollup != RollupJson::None).then_some(self.rollup),
                tasks,
                overwrite: self.overwrite,
            }),
            BaselineAction::Clear => Command::BaselineClear(BaselineClear {
                number: Some(self.number),
                tasks,
            }),
        };
        batch_of(vec![op], lang)
    }
}

/// Libellé de la ligne de base `n`.
#[must_use]
pub fn number_label(texts: &Texts, n: u8) -> String {
    if n == 0 {
        texts.get(TextId::BlBaseline0)
    } else {
        texts.get_args(TextId::BlBaselineN, &[("count", Arg::Count(i64::from(n)))])
    }
}

/// Identifiant de la fenêtre (couche modale d'egui).
#[must_use]
pub fn modal_id() -> egui::Id {
    egui::Id::unique("plannercraft.baseline")
}

/// Dessine la fenêtre (une image) ; rend la réponse de l'utilisateur.
pub fn show(ctx: &egui::Context, texts: &Texts, state: &mut BaselineState) -> Outcome {
    let mut outcome = Outcome::Open;
    let armed = super::enter_armed(ctx, None);
    let response = egui::Modal::new(modal_id()).show(ctx, |ui| {
        ui.set_width(460.0);
        ui.heading(texts.get(TextId::BlTitle));
        if state.stale {
            ui.colored_label(ui.visuals().warn_fg_color, texts.get(TextId::TaskInfoStale));
        }
        if let Some(message) = state.confirm.clone() {
            ui.label(texts.get_args(TextId::BlConfirm, &[("detail", Arg::Str(message.into()))]));
            ui.horizontal(|ui| {
                if ui.button(texts.get(TextId::BlReplace)).clicked() {
                    state.overwrite = true;
                    state.confirm = None;
                    outcome = Outcome::Ok;
                }
                if ui.button(texts.get(TextId::TaskInfoCancel)).clicked() {
                    state.confirm = None;
                }
            });
            return;
        }
        ui.horizontal(|ui| {
            ui.radio_value(&mut state.action, BaselineAction::Set, texts.get(TextId::BlSet));
            ui.radio_value(
                &mut state.action,
                BaselineAction::Clear,
                texts.get(TextId::BlClear),
            );
        });
        let label = texts.get(TextId::BlNumber);
        ui.horizontal(|ui| {
            ui.label(&label);
            let response = egui::ComboBox::from_id_salt("plannercraft.baseline.number")
                .selected_text(number_label(texts, state.number))
                .show_ui(ui, |ui| {
                    for n in 0..=10_u8 {
                        ui.selectable_value(&mut state.number, n, number_label(texts, n));
                    }
                })
                .response;
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, label.clone()));
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
        if state.action == BaselineAction::Set && state.selection_only {
            ui.label(texts.get(TextId::BlRollup));
            for (r, t) in ROLLUPS {
                ui.radio_value(&mut state.rollup, r, texts.get(t));
            }
        }
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
        if state.confirm.is_some() {
            state.confirm = None;
        } else {
            outcome = Outcome::Cancel;
        }
    }
    if outcome == Outcome::Open
        && state.confirm.is_none()
        && super::enter_pressed(ctx, armed, response.is_top_modal, response.any_popup_open)
    {
        outcome = Outcome::Ok;
    }
    outcome
}
