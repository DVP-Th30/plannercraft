//! « Champs personnalisés » (Projet → Champs personnalisés…, seconde passe
//! UX, ADR-0019 D8, lot L9, H-UI-118 ; S94 : renommer un champ = lui
//! donner un alias).
//!
//! Fenêtre modale : type choisi (Texte, Nombre, Date, Indicateur), liste
//! des champs de ce type avec leur alias éditable (50 caractères au plus,
//! vide : le champ reprend son nom), « Renommer… » place la saisie sur le
//! champ sélectionné. Un champ défini par le fichier (formule, table de
//! choix, R-265) est listé grisé. « OK » produit **un** lot de
//! `custom_field.rename` (une entrée d'annulation, libellé « Champs
//! personnalisés ») ; rien de changé ⇒ fermée sans lot. Brouillon figé à
//! l'ouverture (clé du document) : document changé entre-temps ⇒ relu,
//! avertissement, rien appliqué (comme les fenêtres du lot L12).

use std::collections::{BTreeMap, BTreeSet};

use plannercraft_automation::command::CustomFieldRename;
use plannercraft_automation::text::Language;
use plannercraft_automation::{Batch, Command, Session};
use plannercraft_core::model::custom::{CustomFieldId, CustomKind, MAX_CUSTOM_ALIAS_CHARS};

use super::task_info::Outcome;
use super::{batch_of, error_line, text_field};
use crate::field::{aliases, custom_title};
use crate::i18n::{TextId, Texts};

/// Brouillon de la fenêtre.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomFieldsState {
    /// Clé du document à l'ouverture.
    pub key: (u64, u64),
    /// Type affiché.
    pub kind: CustomKind,
    /// Alias saisis, un par champ (vide : pas d'alias).
    pub aliases: BTreeMap<CustomFieldId, String>,
    /// Alias à l'ouverture.
    original: BTreeMap<CustomFieldId, String>,
    /// Champs définis par le fichier (formule, table de choix) : grisés.
    pub hidden: BTreeSet<CustomFieldId>,
    /// Champ sélectionné dans la liste.
    pub selected: Option<CustomFieldId>,
    /// Saisie à focaliser à la prochaine image (« Renommer… »).
    pub focus: Option<CustomFieldId>,
    /// Refus (alias trop long, registre).
    pub error: Option<String>,
    /// Le document avait changé : rien appliqué.
    pub stale: bool,
}

impl CustomFieldsState {
    /// Brouillon lu sur la session `s` (clé `key`) : alias de chaque champ,
    /// champs étrangers, Texte affiché, Texte1 sélectionné.
    #[must_use]
    pub fn open(s: &Session, key: (u64, u64)) -> Self {
        let project = s.project();
        let current = aliases(project);
        let all: BTreeMap<CustomFieldId, String> = CustomFieldId::all()
            .map(|id| (id, current.get(&id).cloned().unwrap_or_default()))
            .collect();
        Self {
            key,
            kind: CustomKind::Text,
            original: all.clone(),
            aliases: all,
            hidden: plannercraft_mspdi::custom::foreign_fields(project),
            selected: Some(CustomFieldId::TEXT1),
            focus: None,
            error: None,
            stale: false,
        }
    }

    /// Lot de « OK » : un `custom_field.rename` par alias changé (espaces
    /// de bord retirés ; vide ⇒ alias retiré), libellé `label` ; `Ok(None)`
    /// si rien n'a changé.
    ///
    /// # Errors
    ///
    /// Message traduit si un alias a plus de 50 caractères (le champ
    /// fautif est sélectionné et son type affiché).
    pub fn to_batch(&mut self, texts: &Texts, lang: Language) -> Result<Option<Batch>, String> {
        let mut ops = Vec::new();
        for (id, alias) in &self.aliases {
            let wanted = alias.trim();
            let before = self.original.get(id).map_or("", |a| a.trim());
            if wanted == before || self.hidden.contains(id) {
                continue;
            }
            if wanted.chars().nth(MAX_CUSTOM_ALIAS_CHARS).is_some() {
                let id = *id;
                self.kind = id.kind();
                self.selected = Some(id);
                self.focus = Some(id);
                return Err(texts.get(TextId::CustomFieldsAliasTooLong));
            }
            ops.push(Command::CustomFieldRename(CustomFieldRename {
                field: id.to_string(),
                alias: (!wanted.is_empty()).then(|| wanted.to_owned()),
            }));
        }
        if ops.is_empty() {
            return Ok(None);
        }
        let mut batch = batch_of(ops, lang);
        batch.label = Some(texts.get(TextId::CustomFieldsBatchLabel));
        Ok(Some(batch))
    }
}

/// Types, dans l'ordre de la fenêtre, et leur libellé.
pub const KINDS: [(CustomKind, TextId); 4] = [
    (CustomKind::Text, TextId::CustomTypeText),
    (CustomKind::Number, TextId::CustomTypeNumber),
    (CustomKind::Date, TextId::CustomTypeDate),
    (CustomKind::Flag, TextId::CustomTypeFlag),
];

/// Identifiant de la fenêtre (couche modale d'egui).
#[must_use]
pub fn modal_id() -> egui::Id {
    egui::Id::unique("plannercraft.custom_fields")
}

/// Largeur de la saisie d'un alias, en points.
const ALIAS_WIDTH: f32 = 220.0;

/// Dessine la fenêtre (une image) ; rend la réponse de l'utilisateur.
pub fn show(ctx: &egui::Context, texts: &Texts, state: &mut CustomFieldsState) -> Outcome {
    let mut outcome = Outcome::Open;
    let armed = super::enter_armed(ctx, None);
    let focus = state.focus.take();
    let response = egui::Modal::new(modal_id()).show(ctx, |ui| {
        ui.set_width(420.0);
        ui.heading(texts.get(TextId::CustomFieldsTitle));
        if state.stale {
            ui.colored_label(ui.visuals().warn_fg_color, texts.get(TextId::TaskInfoStale));
        }
        ui.horizontal(|ui| {
            for (kind, label) in KINDS {
                if ui.radio_value(&mut state.kind, kind, texts.get(label)).changed() {
                    state.selected = CustomFieldId::new(kind, 1);
                }
            }
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .max_height(300.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                egui::Grid::new("plannercraft.custom_fields.grid")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        let kind = state.kind;
                        for id in CustomFieldId::all().filter(|id| id.kind() == kind) {
                            let name = custom_title(id, texts, None);
                            let enabled = !state.hidden.contains(&id);
                            let selected = state.selected == Some(id);
                            if ui.selectable_label(selected, &name).clicked() {
                                state.selected = Some(id);
                            }
                            if let Some(alias) = state.aliases.get_mut(&id) {
                                let response = text_field(ui, &name, alias, enabled, ALIAS_WIDTH);
                                if response.gained_focus() {
                                    state.selected = Some(id);
                                }
                                if focus == Some(id) {
                                    response.request_focus();
                                    response.scroll_to_me(None);
                                }
                            }
                            ui.end_row();
                        }
                    });
            });
        let can_rename = state.selected.is_some_and(|id| !state.hidden.contains(&id));
        if ui
            .add_enabled(
                can_rename,
                egui::Button::new(texts.get(TextId::CustomFieldsRename)),
            )
            .clicked()
        {
            state.focus = state.selected;
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
    if state.focus.is_some() {
        ctx.request_repaint();
    }
    if response.is_top_modal
        && !response.any_popup_open
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        outcome = Outcome::Cancel;
    }
    if super::enter_pressed(ctx, armed, response.is_top_modal, response.any_popup_open) {
        outcome = Outcome::Ok;
    }
    outcome
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use plannercraft_automation::Command;
    use plannercraft_automation::text::Language;
    use plannercraft_core::model::custom::{CustomFieldId, CustomKind};

    use super::CustomFieldsState;
    use crate::i18n::{Language as UiLanguage, TextId, Texts};

    /// H-UI-118 : rien changé ⇒ aucun lot ; deux alias ⇒ un lot de deux
    /// `custom_field.rename` (vide ⇒ retiré), libellé « Champs
    /// personnalisés » ; 51 caractères ⇒ refus, champ fautif sélectionné.
    #[test]
    fn h_ui_118_custom_fields_batch() {
        let session = crate::document::load(crate::DEMO_MSPDI, "2027-01-15T08:00".parse().unwrap())
            .unwrap()
            .session;
        let texts = Texts::new(UiLanguage::Fr);
        let mut state = CustomFieldsState::open(&session, (0, 0));
        assert_eq!(state.aliases.len(), 80);
        assert_eq!(state.to_batch(&texts, Language::Fr), Ok(None));
        let lot = CustomFieldId::TEXT1;
        let number2 = CustomFieldId::new(CustomKind::Number, 2).unwrap();
        *state.aliases.get_mut(&lot).unwrap() = "  Lot ".to_owned();
        *state.aliases.get_mut(&number2).unwrap() = "Quantité".to_owned();
        let batch = state.to_batch(&texts, Language::Fr).unwrap().unwrap();
        assert_eq!(batch.label.as_deref(), Some("Champs personnalisés"));
        let renames: Vec<(String, Option<String>)> = batch
            .ops
            .iter()
            .map(|op| match op {
                Command::CustomFieldRename(r) => (r.field.clone(), r.alias.clone()),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            renames,
            [
                ("text1".to_owned(), Some("Lot".to_owned())),
                ("number2".to_owned(), Some("Quantité".to_owned())),
            ]
        );
        *state.aliases.get_mut(&number2).unwrap() = "x".repeat(51);
        assert_eq!(
            state.to_batch(&texts, Language::Fr),
            Err(texts.get(TextId::CustomFieldsAliasTooLong))
        );
        assert_eq!(state.selected, Some(number2));
        assert_eq!(state.kind, CustomKind::Number);
    }
}
