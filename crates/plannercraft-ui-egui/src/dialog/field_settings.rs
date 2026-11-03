//! Fenêtre « Paramètres de champ » (H-UI-115, S92, lot L6) : champ de la
//! colonne (en changer garde sa place), titre (vide : titre par défaut,
//! [`MAX_COLUMN_TITLE`] caractères), alignement du titre et des données,
//! largeur bornée, « Ajuster ».
//!
//! Une préférence d'affichage (`UiState.columns`, version 2) : rien n'est
//! écrit dans le document, aucune entrée d'annulation. Ouverte par le menu
//! de l'en-tête, Format → Colonnes ou Alt+F3 ; hébergée par
//! `ViewTransient::picker` ([`crate::field::FieldPicker::settings`]) et
//! dessinée par [`crate::dialog::field_picker::show`].

use crate::action::{Action, ColumnOp};
use crate::column::{CellAlign, Column, ColumnState, MAX_COLUMN_TITLE};
use crate::field::{Availability, catalog};
use crate::i18n::{TextId, Texts};

/// Brouillon de la fenêtre : réglages de la colonne et texte du titre en
/// cours de saisie.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldSettingsDraft {
    /// Réglages proposés (champ, largeur, alignements ; le titre est lu de
    /// [`Self::title`] à la validation).
    pub settings: ColumnState,
    /// Titre saisi (vide : titre par défaut).
    pub title: String,
}

impl FieldSettingsDraft {
    /// Brouillon des réglages courants d'une colonne.
    #[must_use]
    pub fn of(state: &ColumnState) -> Self {
        Self {
            title: state.title.clone().unwrap_or_default(),
            settings: state.clone(),
        }
    }

    /// Réglages à appliquer : titre coupé à [`MAX_COLUMN_TITLE`]
    /// caractères, vide ⇒ titre par défaut (`None`) ; largeur bornée par
    /// l'état (`UiState::set_column_settings`).
    #[must_use]
    pub fn to_settings(&self) -> ColumnState {
        let title: String = self.title.trim().chars().take(MAX_COLUMN_TITLE).collect();
        let title = title.trim_end().to_owned();
        ColumnState {
            title: (!title.is_empty()).then_some(title),
            ..self.settings.clone()
        }
    }
}

/// Réponse de la fenêtre pour une image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsOutcome {
    /// Rien.
    Open,
    /// « Annuler » ou Échap.
    Cancel,
    /// « OK » ou Entrée.
    Ok,
    /// « Ajuster » : réglages appliqués puis largeur ajustée au contenu.
    BestFit,
}

/// Identifiant de la fenêtre (couche modale d'egui).
#[must_use]
pub fn modal_id() -> egui::Id {
    egui::Id::unique("plannercraft.field_settings")
}

/// Identifiant de la zone du titre (focalisée à l'ouverture).
#[must_use]
pub fn title_id() -> egui::Id {
    egui::Id::unique("plannercraft.field_settings.title")
}

/// Libellé d'un alignement.
const fn align_label(align: CellAlign) -> TextId {
    match align {
        CellAlign::Left => TextId::AlignLeft,
        CellAlign::Center => TextId::AlignCenter,
        CellAlign::Right => TextId::AlignRight,
    }
}

/// Liste d'alignements nommée `label` ; `current` vaut `default` tant qu'il
/// n'est pas choisi (`None` gardé si l'on choisit l'alignement par défaut).
fn align_combo(
    ui: &mut egui::Ui,
    texts: &Texts,
    label: TextId,
    current: &mut Option<CellAlign>,
    default: CellAlign,
) {
    let name = texts.get(label);
    let shown = current.unwrap_or(default);
    ui.label(&name);
    egui::ComboBox::from_id_salt(("plannercraft.field_settings.align", label.key()))
        .selected_text(texts.get(align_label(shown)))
        .show_ui(ui, |ui| {
            for align in [CellAlign::Left, CellAlign::Center, CellAlign::Right] {
                if ui
                    .selectable_label(shown == align, texts.get(align_label(align)))
                    .clicked()
                {
                    *current = (align != default).then_some(align);
                }
            }
        })
        .response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, name.clone()));
    ui.end_row();
}

/// Champs proposés pour la colonne : le champ courant, puis ceux du
/// catalogue qui ne sont ni grisés ni déjà affichés (`shown`) ; l'ID reste
/// l'ID.
fn field_options(current: Column, shown: &[Column]) -> Vec<Column> {
    if current == Column::Id {
        return vec![Column::Id];
    }
    let mut out = vec![current];
    out.extend(
        catalog()
            .iter()
            .filter(|d| {
                d.column != Column::Id
                    && d.column != current
                    && !shown.contains(&d.column)
                    && !matches!(d.availability, Availability::Unavailable(_))
            })
            .map(|d| d.column),
    );
    out
}

/// Dessine la fenêtre (une image) ; `shown` : colonnes affichées (un champ
/// affiché ailleurs n'est pas proposé). Rend la réponse de l'utilisateur.
pub fn show(
    ctx: &egui::Context,
    texts: &Texts,
    draft: &mut FieldSettingsDraft,
    shown: &[Column],
    focus: bool,
) -> SettingsOutcome {
    let mut outcome = SettingsOutcome::Open;
    let armed = super::enter_armed(ctx, None);
    let response = egui::Modal::new(modal_id()).show(ctx, |ui| {
        ui.set_width(380.0);
        ui.heading(texts.get(TextId::FieldSettingsTitle));
        egui::Grid::new("plannercraft.field_settings.grid")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                // Champ : en changer garde la place de la colonne.
                let name = texts.get(TextId::FieldSettingsField);
                ui.label(&name);
                let current = draft.settings.column;
                let options = field_options(current, shown);
                ui.add_enabled_ui(options.len() > 1, |ui| {
                    egui::ComboBox::from_id_salt("plannercraft.field_settings.field")
                        .selected_text(current.label(texts))
                        .height(320.0)
                        .show_ui(ui, |ui| {
                            for column in options {
                                if ui
                                    .selectable_label(column == current, column.label(texts))
                                    .clicked()
                                    && column != current
                                {
                                    draft.settings = ColumnState {
                                        column,
                                        width: column.default_width(),
                                        ..draft.settings.clone()
                                    };
                                }
                            }
                        })
                        .response
                        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, name.clone()));
                });
                ui.end_row();
                // Titre (vide : titre par défaut, rappelé en filigrane).
                let name = texts.get(TextId::FieldSettingsColumnTitle);
                ui.label(&name);
                let edit = egui::TextEdit::singleline(&mut draft.title)
                    .id(title_id())
                    .char_limit(MAX_COLUMN_TITLE)
                    .hint_text(draft.settings.column.label(texts))
                    .desired_width(220.0);
                let response = ui.add(edit);
                response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, name.clone()));
                if focus {
                    response.request_focus();
                }
                ui.end_row();
                let default = draft.settings.column.align();
                align_combo(
                    ui,
                    texts,
                    TextId::FieldSettingsAlignTitle,
                    &mut draft.settings.header_align,
                    CellAlign::Center,
                );
                align_combo(
                    ui,
                    texts,
                    TextId::FieldSettingsAlignData,
                    &mut draft.settings.align,
                    default,
                );
                // Largeur bornée.
                let name = texts.get(TextId::FieldSettingsWidth);
                ui.label(&name);
                ui.add(
                    egui::DragValue::new(&mut draft.settings.width)
                        .range(Column::MIN_WIDTH..=Column::MAX_WIDTH)
                        .speed(1.0),
                )
                .widget_info(|| egui::WidgetInfo::labeled(egui::Role::SpinButton, true, name.clone()));
                ui.end_row();
            });
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button(texts.get(TextId::TaskInfoOk)).clicked() {
                outcome = SettingsOutcome::Ok;
            }
            if ui.button(texts.get(TextId::MenuFitColumn)).clicked() {
                outcome = SettingsOutcome::BestFit;
            }
            if ui.button(texts.get(TextId::TaskInfoCancel)).clicked() {
                outcome = SettingsOutcome::Cancel;
            }
        });
    });
    let escape = response.is_top_modal
        && !response.any_popup_open
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if escape {
        outcome = SettingsOutcome::Cancel;
    }
    if outcome == SettingsOutcome::Open
        && super::enter_pressed(ctx, armed, response.is_top_modal, response.any_popup_open)
    {
        outcome = SettingsOutcome::Ok;
    }
    outcome
}

/// Actions de la réponse `outcome` pour la colonne de rang `at` (aucune
/// pour Annuler ou une image sans réponse) : « OK » ⇒
/// [`ColumnOp::Settings`] ; « Ajuster » ⇒ mêmes réglages puis
/// `Action::FitColumn` du champ choisi.
#[must_use]
pub fn actions(outcome: SettingsOutcome, at: usize, draft: &FieldSettingsDraft) -> Vec<Action> {
    let settings = draft.to_settings();
    let column = settings.column;
    let apply = Action::Column(ColumnOp::Settings { at, settings });
    match outcome {
        SettingsOutcome::Open | SettingsOutcome::Cancel => Vec::new(),
        SettingsOutcome::Ok => vec![apply],
        SettingsOutcome::BestFit => vec![apply, Action::FitColumn(column)],
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use super::{FieldSettingsDraft, SettingsOutcome, actions, field_options};
    use crate::action::{Action, ColumnOp};
    use crate::column::{Column, ColumnState, MAX_COLUMN_TITLE};

    /// H-UI-115 : titre vide ⇒ défaut ; titre borné ; « Ajuster » ⇒
    /// réglages puis ajustement ; Annuler ⇒ rien.
    #[test]
    fn h_ui_115_draft_to_settings() {
        let mut draft = FieldSettingsDraft::of(&ColumnState::default_for(Column::Duration));
        assert_eq!(draft.to_settings().title, None);
        draft.title = "  Durée prévue  ".to_owned();
        assert_eq!(draft.to_settings().title.as_deref(), Some("Durée prévue"));
        draft.title = "x".repeat(200);
        assert_eq!(
            draft.to_settings().title.unwrap().chars().count(),
            MAX_COLUMN_TITLE
        );
        assert_eq!(actions(SettingsOutcome::Cancel, 2, &draft), Vec::new());
        let fit = actions(SettingsOutcome::BestFit, 2, &draft);
        assert!(matches!(
            fit.as_slice(),
            [
                Action::Column(ColumnOp::Settings { at: 2, .. }),
                Action::FitColumn(Column::Duration)
            ]
        ));
    }

    /// Champs proposés : le courant d'abord, jamais l'ID ni un champ
    /// affiché ailleurs ni un coût ; l'ID ne change pas de champ.
    #[test]
    fn h_ui_115_field_options() {
        let shown = [Column::Id, Column::Name, Column::Duration, Column::Start];
        let options = field_options(Column::Duration, &shown);
        assert_eq!(options.first(), Some(&Column::Duration));
        assert!(!options.contains(&Column::Id) && !options.contains(&Column::Start));
        assert!(!options.contains(&Column::Cost));
        assert!(options.contains(&Column::Work));
        assert_eq!(field_options(Column::Id, &shown), [Column::Id]);
    }
}
