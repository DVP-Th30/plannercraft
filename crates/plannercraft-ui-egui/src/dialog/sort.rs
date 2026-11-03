//! Fenêtre « Trier » (H-UI-31, S75) : clé, sens, « conserver la
//! structure » coché par défaut. Contrat posé par le lot L0, implémenté en
//! L9.
//!
//! Un tri est une vue (P50) : rien n'est écrit, les ID ne sont jamais
//! renumérotés. « Réinitialiser » revient à l'ordre des ID (comme Maj+F3).
//! Hébergée par la vue des tâches (`ViewTransient::sort_dialog`), hors des
//! fenêtres du document : aucun brouillon du document à relire.

use crate::i18n::{TextId, Texts};
use crate::state::{SortKey, SortSpec};

/// Brouillon de la fenêtre.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SortDialog {
    /// Clé choisie.
    pub key: SortKey,
    /// Décroissant.
    pub descending: bool,
    /// « Conserver la structure ».
    pub keep_outline: bool,
}

impl SortDialog {
    /// Fenêtre ouverte sur le tri courant ; sans tri : Début croissant,
    /// structure conservée (S75 : « Keep outline structure » recommandé).
    #[must_use]
    pub fn open(current: Option<SortSpec>) -> Self {
        current.map_or(
            Self {
                key: SortKey::Start,
                descending: false,
                keep_outline: true,
            },
            |spec| Self {
                key: spec.key,
                descending: spec.descending,
                keep_outline: spec.keep_outline,
            },
        )
    }

    /// Tri demandé ; l'ordre des ID croissant n'est pas un tri (`None`).
    #[must_use]
    pub const fn spec(&self) -> Option<SortSpec> {
        if matches!(self.key, SortKey::Id) && !self.descending {
            None
        } else {
            Some(SortSpec {
                key: self.key,
                descending: self.descending,
                keep_outline: self.keep_outline,
            })
        }
    }
}

/// Réponse de la fenêtre pour une image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortOutcome {
    /// Rien.
    Open,
    /// « Annuler » ou Échap.
    Cancel,
    /// « Trier » (ou Entrée) : tri à poser (`None` : ordre des ID).
    Apply(Option<SortSpec>),
}

/// Identifiant de la fenêtre (couche modale d'egui).
#[must_use]
pub fn modal_id() -> egui::Id {
    egui::Id::unique("plannercraft.sort")
}

/// Dessine la fenêtre (une image) ; rend la réponse de l'utilisateur.
pub fn show(ctx: &egui::Context, texts: &Texts, state: &mut SortDialog) -> SortOutcome {
    let mut outcome = SortOutcome::Open;
    let armed = super::enter_armed(ctx, None);
    let response = egui::Modal::new(modal_id()).show(ctx, |ui| {
        ui.set_width(360.0);
        ui.heading(texts.get(TextId::SortTitle));
        ui.horizontal(|ui| {
            let label = texts.get(TextId::SortBy);
            ui.label(&label);
            egui::ComboBox::from_id_salt("plannercraft.sort.key")
                .selected_text(texts.get(state.key.column().title()))
                .show_ui(ui, |ui| {
                    for key in SortKey::ALL {
                        ui.selectable_value(&mut state.key, key, texts.get(key.column().title()));
                    }
                })
                .response
                .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, label.clone()));
        });
        ui.radio_value(&mut state.descending, false, texts.get(TextId::SortAscending));
        ui.radio_value(&mut state.descending, true, texts.get(TextId::SortDescending));
        ui.checkbox(&mut state.keep_outline, texts.get(TextId::SortKeepOutline));
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button(texts.get(TextId::SortApply)).clicked() {
                outcome = SortOutcome::Apply(state.spec());
            }
            if ui.button(texts.get(TextId::SortReset)).clicked() {
                outcome = SortOutcome::Apply(None);
            }
            if ui.button(texts.get(TextId::TaskInfoCancel)).clicked() {
                outcome = SortOutcome::Cancel;
            }
        });
    });
    let escape = response.is_top_modal
        && !response.any_popup_open
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if escape {
        outcome = SortOutcome::Cancel;
    }
    if outcome == SortOutcome::Open
        && super::enter_pressed(ctx, armed, response.is_top_modal, response.any_popup_open)
    {
        outcome = SortOutcome::Apply(state.spec());
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::SortDialog;
    use crate::state::{SortKey, SortSpec};

    #[test]
    fn sort_dialog_defaults_and_id_order() {
        let d = SortDialog::open(None);
        assert_eq!(d.key, SortKey::Start);
        assert!(d.keep_outline && !d.descending);
        let id = SortDialog {
            key: SortKey::Id,
            descending: false,
            keep_outline: true,
        };
        assert_eq!(id.spec(), None);
        let spec = SortSpec {
            key: SortKey::Finish,
            descending: true,
            keep_outline: false,
        };
        assert_eq!(SortDialog::open(Some(spec)).spec(), Some(spec));
    }
}
