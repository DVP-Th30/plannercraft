//! « Informations sur la tâche » (J7, lot L9, H-UI-19) : fenêtre modale à
//! cinq onglets (Général, Prédécesseurs, Ressources, Avancé, Notes) dont
//! « OK » produit **un seul lot** du registre (une entrée d'annulation).
//!
//! Le brouillon ([`TaskInfoDraft`]) est celui du « Formulaire tâche »
//! (`crate::task_form`, sorti de ce module au lot L5 de la seconde passe
//! UX sans changement de comportement) : lu à l'ouverture par
//! [`TaskInfoDraft::from_session`], traduit en lot par
//! [`TaskInfoDraft::to_batch`], fonctions pures testées sans egui ; ordre
//! du lot et comparaison par texte décrits dans `task_form`.

use plannercraft_automation::Session;
use plannercraft_automation::text::Language;
use plannercraft_core::{ResourceUid, TaskUid};

use crate::i18n::{Arg, TextId, Texts};
pub use crate::task_form::{
    Advanced, AssignRow, CONSTRAINTS, FieldError, General, LINK_TYPES, LinkRow, TASK_TYPES, Tab,
    command_message, locate,
};
pub(crate) use crate::task_form::{units_json, units_text};

/// Brouillon de la fenêtre, figé à l'ouverture : celui du « Formulaire
/// tâche » (seconde passe UX, lot L5, `task_form`), dont la fenêtre
/// n'affiche ni les successeurs, ni le mode, ni le travail des affectations
/// (inchangés, ils n'envoient rien).
pub type TaskInfoDraft = crate::task_form::TaskFormDraft;

// --- Fenêtre (egui) -------------------------------------------------------

/// Ressource proposée dans l'onglet Ressources (relevée à l'ouverture).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceChoice {
    /// UID.
    pub uid: ResourceUid,
    /// Nom affiché.
    pub name: String,
}

/// État de la fenêtre ouverte : brouillon, onglet, refus affichés.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskInfoState {
    /// Brouillon.
    pub draft: TaskInfoDraft,
    /// ID de la tâche à l'ouverture (titre).
    pub id: u32,
    /// Nom de la tâche à l'ouverture (titre).
    pub name: String,
    /// Onglet affiché.
    pub tab: Tab,
    /// Refus de la dernière tentative (champs, ou registre).
    pub errors: Vec<FieldError>,
    /// Le projet avait changé au dernier « OK » : brouillon relu, rien
    /// appliqué.
    pub stale: bool,
    /// Ressources du projet, dans l'ordre du projet.
    pub resources: Vec<ResourceChoice>,
}

impl TaskInfoState {
    /// État d'ouverture de la tâche `uid` (`None` : tâche absente).
    #[must_use]
    pub fn open(s: &Session, uid: TaskUid, key: (u64, u64), lang: Language) -> Option<Self> {
        let draft = TaskInfoDraft::from_session(s, uid, key, lang)?;
        let project = s.project();
        let id = project.task_id(uid).map_or(0, plannercraft_core::TaskId::get);
        let resources = project
            .resources
            .iter()
            .map(|r| ResourceChoice {
                uid: plannercraft_core::HasUid::uid(r),
                name: r.name.clone(),
            })
            .collect();
        Some(Self {
            name: draft.general.name.clone(),
            draft,
            id,
            tab: Tab::General,
            errors: Vec::new(),
            stale: false,
            resources,
        })
    }

    /// Refus du champ `field` (ligne `row`) de l'onglet courant ou donné.
    fn error_for(&self, tab: Tab, field: &str, row: Option<usize>) -> Option<&str> {
        self.errors
            .iter()
            .find(|e| e.tab == tab && e.field == field && e.row == row)
            .map(|e| e.message.as_str())
    }
}

/// Onglet de « Informations sur la tâche » (nom du contrat ADR-0017).
pub type TaskInfoTab = Tab;

/// Ouvre « Informations sur la tâche » de `uid` sur l'onglet `tab`
/// (Tâche → Notes du ruban, H-UI-84) ; `None` : tâche absente. Même
/// brouillon que [`TaskInfoState::open`], seul l'onglet affiché change.
#[must_use]
pub fn open_on(
    tab: TaskInfoTab,
    s: &Session,
    uid: TaskUid,
    key: (u64, u64),
    lang: Language,
) -> Option<TaskInfoState> {
    TaskInfoState::open(s, uid, key, lang).map(|state| TaskInfoState { tab, ..state })
}

/// Réponse de la fenêtre pour une image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Rien : la fenêtre reste ouverte.
    Open,
    /// « OK ».
    Ok,
    /// « Annuler » ou Échap.
    Cancel,
}

/// Identifiant de la fenêtre (couche modale d'egui).
#[must_use]
pub fn modal_id() -> egui::Id {
    egui::Id::unique("plannercraft.task-info")
}

/// Largeur des champs de texte.
const FIELD_W: f32 = 260.0;

/// Champ de texte d'une ligne, nommé `label` pour l'accessibilité ; grisé
/// si `enabled` est faux.
pub(crate) fn text_field(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    enabled: bool,
    width: f32,
) -> egui::Response {
    let response = ui.add_enabled(enabled, egui::TextEdit::singleline(value).desired_width(width));
    let name = label.to_owned();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, enabled, name.clone()));
    response
}

/// Message de refus (couleur d'erreur), s'il y en a un.
pub(crate) fn error_line(ui: &mut egui::Ui, message: Option<&str>) {
    if let Some(message) = message {
        ui.colored_label(ui.visuals().error_fg_color, message);
    }
}

/// Liste déroulante de `choices`, nommée `label`.
pub(crate) fn combo<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    texts: &Texts,
    value: &mut T,
    choices: &[(T, TextId)],
    enabled: bool,
) {
    let selected = choices
        .iter()
        .find(|(v, _)| *v == *value)
        .map(|(_, t)| texts.get(*t))
        .unwrap_or_default();
    ui.add_enabled_ui(enabled, |ui| {
        let response = egui::ComboBox::from_id_salt(salt)
            .selected_text(selected)
            .show_ui(ui, |ui| {
                for (v, t) in choices {
                    ui.selectable_value(value, *v, texts.get(*t));
                }
            })
            .response;
        let name = label.to_owned();
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, enabled, name.clone()));
    });
}

/// Dessine la fenêtre modale (une image) et rend la réponse de
/// l'utilisateur. Échap ferme (sauf liste déroulante ouverte, qu'il ferme
/// d'abord) ; un clic hors de la fenêtre ne la ferme pas (brouillon gardé).
pub fn show(ctx: &egui::Context, texts: &Texts, state: &mut TaskInfoState) -> Outcome {
    let mut outcome = Outcome::Open;
    let armed = super::enter_armed(ctx, Some(notes_id()));
    let response = egui::Modal::new(modal_id()).show(ctx, |ui| {
        ui.set_width(560.0);
        let title = texts.get_args(
            TextId::TaskInfoTitle,
            &[
                ("id", Arg::Count(i64::from(state.id))),
                ("name", Arg::Str(state.name.clone().into())),
            ],
        );
        ui.heading(title);
        if state.stale {
            ui.colored_label(ui.visuals().warn_fg_color, texts.get(TextId::TaskInfoStale));
        }
        ui.horizontal(|ui| {
            for tab in Tab::ALL {
                let failed = state.errors.iter().any(|e| e.tab == tab);
                let mut text = egui::RichText::new(texts.get(tab.title()));
                if failed {
                    text = text.color(ui.visuals().error_fg_color);
                }
                if ui.selectable_label(state.tab == tab, text).clicked() {
                    state.tab = tab;
                }
            }
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .max_height(320.0)
            .show(ui, |ui| match state.tab {
                Tab::General => general_tab(ui, texts, state),
                Tab::Predecessors => predecessors_tab(ui, texts, state),
                Tab::Resources => resources_tab(ui, texts, state),
                Tab::Advanced => advanced_tab(ui, texts, state),
                Tab::Notes => notes_tab(ui, texts, state),
            });
        // Refus sans champ connu (registre) : sous les onglets.
        for e in state.errors.iter().filter(|e| e.field.is_empty()) {
            ui.colored_label(ui.visuals().error_fg_color, e.message.as_str());
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

/// Onglet Général.
fn general_tab(ui: &mut egui::Ui, texts: &Texts, state: &mut TaskInfoState) {
    let editable = !state.draft.summary;
    let errors: Vec<(&'static str, Option<String>)> = ["name", "duration", "percent", "start", "finish"]
        .into_iter()
        .map(|f| (f, state.error_for(Tab::General, f, None).map(str::to_owned)))
        .collect();
    let message = |field: &str| {
        errors
            .iter()
            .find(|(f, _)| *f == field)
            .and_then(|(_, m)| m.clone())
    };
    let g = &mut state.draft.general;
    egui::Grid::new("plannercraft.task-info.general")
        .num_columns(2)
        .show(ui, |ui| {
            for (field, label, value, enabled) in [
                ("name", TextId::TaskInfoName, &mut g.name, true),
                ("duration", TextId::TaskInfoDuration, &mut g.duration, editable),
                ("percent", TextId::TaskInfoPercent, &mut g.percent, editable),
                ("start", TextId::TaskInfoStart, &mut g.start, editable),
                ("finish", TextId::TaskInfoFinish, &mut g.finish, editable),
            ] {
                let label = texts.get(label);
                ui.label(&label);
                ui.vertical(|ui| {
                    text_field(ui, &label, value, enabled, FIELD_W);
                    error_line(ui, message(field).as_deref());
                });
                ui.end_row();
            }
            ui.allocate_space(egui::Vec2::ZERO);
            ui.checkbox(&mut g.active, texts.get(TextId::TaskInfoActive));
            ui.end_row();
        });
}

/// Onglet Prédécesseurs (S67) : ID, nom (lu), type, décalage.
fn predecessors_tab(ui: &mut egui::Ui, texts: &Texts, state: &mut TaskInfoState) {
    let errors = state.errors.clone();
    let mut remove = None;
    let id_label = texts.get(TextId::TaskInfoPredId);
    let type_label = texts.get(TextId::TaskInfoPredType);
    let lag_label = texts.get(TextId::TaskInfoPredLag);
    egui::Grid::new("plannercraft.task-info.links")
        .num_columns(4)
        .show(ui, |ui| {
            ui.strong(&id_label);
            ui.strong(&type_label);
            ui.strong(&lag_label);
            ui.allocate_space(egui::Vec2::ZERO);
            ui.end_row();
            for (i, row) in state.draft.links.iter_mut().enumerate() {
                let n = i.saturating_add(1);
                let named = |label: &str| {
                    texts.get_args(
                        TextId::TaskInfoRowField,
                        &[
                            ("column", Arg::Str(label.to_owned().into())),
                            ("count", Arg::Count(i64::try_from(n).unwrap_or(i64::MAX))),
                        ],
                    )
                };
                ui.vertical(|ui| {
                    text_field(ui, &named(&id_label), &mut row.id, true, 60.0);
                    error_line(ui, find(&errors, Tab::Predecessors, "predecessor", Some(i)));
                });
                combo(
                    ui,
                    ("plannercraft.task-info.link-type", i),
                    &named(&type_label),
                    texts,
                    &mut row.kind,
                    &LINK_TYPES,
                    true,
                );
                ui.vertical(|ui| {
                    text_field(ui, &named(&lag_label), &mut row.lag, true, 90.0);
                    error_line(ui, find(&errors, Tab::Predecessors, "lag", Some(i)));
                });
                if ui.button(texts.get(TextId::TaskInfoRemoveRow)).clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
    if let Some(i) = remove
        && i < state.draft.links.len()
    {
        state.draft.links.remove(i);
        state.errors.retain(|e| e.tab != Tab::Predecessors);
    }
    if ui.button(texts.get(TextId::TaskInfoAddPredecessor)).clicked() {
        state.draft.links.push(LinkRow::blank());
    }
}

/// Refus de `field` (ligne `row`) d'un onglet, dans une liste copiée.
fn find<'a>(errors: &'a [FieldError], tab: Tab, field: &str, row: Option<usize>) -> Option<&'a str> {
    errors
        .iter()
        .find(|e| e.tab == tab && e.field == field && e.row == row)
        .map(|e| e.message.as_str())
}

/// Onglet Ressources : ressource, unités.
fn resources_tab(ui: &mut egui::Ui, texts: &Texts, state: &mut TaskInfoState) {
    if state.resources.is_empty() {
        ui.label(texts.get(TextId::TaskInfoNoResource));
        return;
    }
    let errors = state.errors.clone();
    let resources = state.resources.clone();
    let mut remove = None;
    let name_label = texts.get(TextId::TaskInfoResourceName);
    let units_label = texts.get(TextId::TaskInfoResourceUnits);
    egui::Grid::new("plannercraft.task-info.assignments")
        .num_columns(3)
        .show(ui, |ui| {
            ui.strong(&name_label);
            ui.strong(&units_label);
            ui.allocate_space(egui::Vec2::ZERO);
            ui.end_row();
            for (i, row) in state.draft.assignments.iter_mut().enumerate() {
                let n = i64::try_from(i.saturating_add(1)).unwrap_or(i64::MAX);
                let named = |label: &str| {
                    texts.get_args(
                        TextId::TaskInfoRowField,
                        &[
                            ("column", Arg::Str(label.to_owned().into())),
                            ("count", Arg::Count(n)),
                        ],
                    )
                };
                let current = resources
                    .iter()
                    .find(|r| r.uid == row.resource)
                    .map(|r| r.name.clone())
                    .unwrap_or_default();
                ui.vertical(|ui| {
                    let response = egui::ComboBox::from_id_salt(("plannercraft.task-info.resource", i))
                        .selected_text(current)
                        .show_ui(ui, |ui| {
                            for r in &resources {
                                ui.selectable_value(&mut row.resource, r.uid, &r.name);
                            }
                        })
                        .response;
                    let name = named(&name_label);
                    response
                        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, name.clone()));
                    error_line(ui, find(&errors, Tab::Resources, "resource", Some(i)));
                });
                ui.vertical(|ui| {
                    text_field(ui, &named(&units_label), &mut row.units, true, 90.0);
                    error_line(ui, find(&errors, Tab::Resources, "units", Some(i)));
                });
                if ui.button(texts.get(TextId::TaskInfoRemoveRow)).clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
    if let Some(i) = remove
        && i < state.draft.assignments.len()
    {
        state.draft.assignments.remove(i);
        state.errors.retain(|e| e.tab != Tab::Resources);
    }
    // « Ajouter » : première ressource pas encore affectée.
    let free = resources
        .iter()
        .find(|r| !state.draft.assignments.iter().any(|a| a.resource == r.uid));
    if ui
        .add_enabled(
            free.is_some(),
            egui::Button::new(texts.get(TextId::TaskInfoAddResource)),
        )
        .clicked()
        && let Some(r) = free
    {
        state.draft.assignments.push(AssignRow {
            resource: r.uid,
            units: String::new(),
            work: String::new(),
        });
    }
}

/// Onglet Avancé (S66) : contrainte, échéance, type, pilotage.
fn advanced_tab(ui: &mut egui::Ui, texts: &Texts, state: &mut TaskInfoState) {
    let editable = !state.draft.summary;
    let date_error = state
        .error_for(Tab::Advanced, "constraint_date", None)
        .map(str::to_owned);
    let deadline_error = state
        .error_for(Tab::Advanced, "deadline", None)
        .map(str::to_owned);
    let a = &mut state.draft.advanced;
    egui::Grid::new("plannercraft.task-info.advanced")
        .num_columns(2)
        .show(ui, |ui| {
            let label = texts.get(TextId::TaskInfoConstraint);
            ui.label(&label);
            combo(
                ui,
                "plannercraft.task-info.constraint",
                &label,
                texts,
                &mut a.constraint,
                &CONSTRAINTS,
                true,
            );
            ui.end_row();
            let label = texts.get(TextId::TaskInfoConstraintDate);
            ui.label(&label);
            ui.vertical(|ui| {
                text_field(
                    ui,
                    &label,
                    &mut a.constraint_date,
                    a.constraint.is_dated(),
                    FIELD_W,
                );
                error_line(ui, date_error.as_deref());
            });
            ui.end_row();
            let label = texts.get(TextId::TaskInfoDeadline);
            ui.label(&label);
            ui.vertical(|ui| {
                text_field(ui, &label, &mut a.deadline, true, FIELD_W);
                error_line(ui, deadline_error.as_deref());
            });
            ui.end_row();
            let label = texts.get(TextId::TaskInfoTaskType);
            ui.label(&label);
            combo(
                ui,
                "plannercraft.task-info.task-type",
                &label,
                texts,
                &mut a.task_type,
                &TASK_TYPES,
                editable,
            );
            ui.end_row();
            ui.allocate_space(egui::Vec2::ZERO);
            ui.add_enabled(
                editable,
                egui::Checkbox::new(&mut a.effort_driven, texts.get(TextId::TaskInfoEffortDriven)),
            );
            ui.end_row();
        });
}

/// Onglet Notes.
/// Identifiant de la zone des notes (multiligne : Entrée n'y valide pas la
/// fenêtre).
fn notes_id() -> egui::Id {
    egui::Id::unique("plannercraft.task-info.notes")
}

fn notes_tab(ui: &mut egui::Ui, texts: &Texts, state: &mut TaskInfoState) {
    let label = texts.get(TextId::TaskInfoNotes);
    let response = ui.add(
        egui::TextEdit::multiline(&mut state.draft.notes)
            .id(notes_id())
            .desired_width(f32::INFINITY)
            .desired_rows(10),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::MultilineTextInput, true, label.clone()));
}
