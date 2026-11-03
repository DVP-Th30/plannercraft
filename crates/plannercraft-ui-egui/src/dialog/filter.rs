//! Fenêtre « Filtre personnalisé » (H-UI-30, R-251) : conjonction des
//! critères de `plannercraft_automation::filter::TaskFilter`. Contrat posé par le
//! lot L0, implémenté en L9.
//!
//! Trois formes : filtre personnalisé (cases à cocher « doit être… »,
//! ressource, dates, niveau maximal, nom contenant, récapitulatives liées),
//! « Utilisant la ressource… » et « Plage de dates… » (filtres prédéfinis
//! paramétrés). Un filtre est une vue (P50) : rien n'est écrit. Les dates
//! sont lues par `parse_date` (R-143) dans la langue de l'interface ; seule
//! la journée compte (R-251 : dates planifiées touchant le jour). Hébergée
//! par la vue des tâches (`ViewTransient::filter_dialog`).

use plannercraft_automation::filter::{FilterPreset, TaskFilter};
use plannercraft_automation::text::{DateRole, Language, TextContext, format_date, parse_date};
use plannercraft_core::{Day, Moment, Project};

use super::{error_line, text_field};
use crate::edit::EditError;
use crate::i18n::{TextId, Texts};
use crate::state::{FilterChoice, MAX_OUTLINE_LEVEL};
use crate::view::search::{MAX_QUERY_CHARS, truncate_chars};

/// Forme de la fenêtre.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FilterDialogMode {
    /// « Filtre personnalisé… ».
    #[default]
    Custom,
    /// « Utilisant la ressource… ».
    Resource,
    /// « Plage de dates… ».
    DateRange,
}

/// Brouillon de la fenêtre.
#[allow(clippy::struct_excessive_bools, reason = "une case à cocher par critère")]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FilterDialog {
    /// Forme.
    pub mode: FilterDialogMode,
    /// Critiques seulement.
    pub critical: bool,
    /// En retard seulement.
    pub late: bool,
    /// Inachevées seulement.
    pub incomplete: bool,
    /// Jalons seulement.
    pub milestone: bool,
    /// Récapitulatives seulement.
    pub summary: bool,
    /// Actives seulement.
    pub active: bool,
    /// Critère « ressource » posé (filtre personnalisé).
    pub use_resource: bool,
    /// Ressource choisie (nom).
    pub resource: Option<String>,
    /// Noms des ressources du projet (triés, sans doublon).
    pub resources: Vec<String>,
    /// Premier jour saisi.
    pub from: String,
    /// Dernier jour saisi.
    pub to: String,
    /// Niveau hiérarchique maximal.
    pub outline_max: Option<u16>,
    /// Nom contenant.
    pub name_contains: String,
    /// Afficher les récapitulatives liées.
    pub related: bool,
    /// Refus (date illisible, plage inversée, ressource absente).
    pub error: Option<String>,
}

impl FilterDialog {
    /// Fenêtre de forme `mode`, préremplie par le filtre courant.
    #[must_use]
    pub fn open(project: &Project, lang: Language, mode: FilterDialogMode, current: &FilterChoice) -> Self {
        let mut resources: Vec<String> = project
            .resources
            .iter()
            .map(|r| r.name.trim().to_owned())
            .filter(|name| !name.is_empty())
            .collect();
        resources.sort();
        resources.dedup();
        let filter = match current {
            FilterChoice::Preset(preset) => TaskFilter::from_preset(preset),
            FilterChoice::Custom(filter) => filter.clone(),
        };
        let ctx = TextContext::for_project(project, lang);
        let day_text = |day: Option<Day>, end: bool| {
            day.and_then(|d| {
                let time = if end {
                    ctx.default_finish_time
                } else {
                    ctx.default_start_time
                };
                Moment::at(d, time).ok()
            })
            .map(|m| format_date(m, &ctx))
            .unwrap_or_default()
        };
        Self {
            mode,
            critical: filter.critical == Some(true),
            late: filter.late == Some(true),
            incomplete: filter.incomplete == Some(true),
            milestone: filter.milestone == Some(true),
            summary: filter.summary == Some(true),
            active: filter.active == Some(true),
            use_resource: filter.resource.is_some(),
            resource: filter.resource.clone().or_else(|| resources.first().cloned()),
            resources,
            from: day_text(filter.from, false),
            to: day_text(filter.to, true),
            outline_max: filter.outline_max,
            name_contains: filter.name_contains.clone().unwrap_or_default(),
            related: filter.show_related_summaries || filter.is_empty(),
            error: None,
        }
    }

    /// Filtre demandé.
    ///
    /// # Errors
    ///
    /// Message traduit : date illisible, plage inversée, plage incomplète
    /// (« Plage de dates… »), aucune ressource (« Utilisant la
    /// ressource… »).
    pub fn to_choice(
        &self,
        project: &Project,
        texts: &Texts,
        lang: Language,
    ) -> Result<FilterChoice, String> {
        let ctx = TextContext::for_project(project, lang);
        let day = |s: &str| -> Result<Option<Day>, String> {
            if s.trim().is_empty() {
                return Ok(None);
            }
            parse_date(s, DateRole::Start, &ctx)
                .map(|m| Some(m.day()))
                .map_err(|e| EditError::from(e).message(texts))
        };
        let (from, to) = (day(&self.from)?, day(&self.to)?);
        if let (Some(from), Some(to)) = (from, to)
            && from > to
        {
            return Err(texts.get(TextId::FilterDateOrder));
        }
        match self.mode {
            FilterDialogMode::Resource => self
                .resource
                .clone()
                .filter(|name| !name.is_empty())
                .map(|name| FilterChoice::Preset(FilterPreset::UsingResource(name)))
                .ok_or_else(|| texts.get(TextId::FilterResourceMissing)),
            FilterDialogMode::DateRange => match (from, to) {
                (Some(from), Some(to)) => Ok(FilterChoice::Preset(FilterPreset::DateRange { from, to })),
                _ => Err(texts.get(TextId::FilterDatesRequired)),
            },
            FilterDialogMode::Custom => {
                let on = |b: bool| b.then_some(true);
                let name = truncate_chars(self.name_contains.trim(), MAX_QUERY_CHARS);
                let filter = TaskFilter {
                    critical: on(self.critical),
                    late: on(self.late),
                    incomplete: on(self.incomplete),
                    milestone: on(self.milestone),
                    summary: on(self.summary),
                    active: on(self.active),
                    resource: self.resource.clone().filter(|_| self.resource_used()),
                    from,
                    to,
                    outline_max: self.outline_max,
                    name_contains: (!name.is_empty()).then(|| name.to_owned()),
                    show_related_summaries: self.related,
                };
                Ok(if filter.is_empty() {
                    FilterChoice::Preset(FilterPreset::All)
                } else {
                    FilterChoice::Custom(filter)
                })
            }
        }
    }

    /// Le critère « ressource » du filtre personnalisé est posé : case
    /// cochée et ressource choisie dans la liste.
    fn resource_used(&self) -> bool {
        self.resource
            .as_ref()
            .is_some_and(|name| self.resources.iter().any(|r| r == name))
            && self.use_resource
    }
}

/// Réponse de la fenêtre pour une image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterOutcome {
    /// Rien.
    Open,
    /// « Annuler » ou Échap.
    Cancel,
    /// « Appliquer » (ou Entrée).
    Apply,
}

/// Identifiant de la fenêtre (couche modale d'egui).
#[must_use]
pub fn modal_id() -> egui::Id {
    egui::Id::unique("plannercraft.filter")
}

/// Titre de la fenêtre selon sa forme.
const fn title(mode: FilterDialogMode) -> TextId {
    match mode {
        FilterDialogMode::Custom => TextId::FilterCustomTitle,
        FilterDialogMode::Resource => TextId::FilterResourceTitle,
        FilterDialogMode::DateRange => TextId::FilterDateRangeTitle,
    }
}

/// Dessine la fenêtre (une image) ; rend la réponse de l'utilisateur.
pub fn show(ctx: &egui::Context, texts: &Texts, state: &mut FilterDialog) -> FilterOutcome {
    let mut outcome = FilterOutcome::Open;
    let armed = super::enter_armed(ctx, None);
    let response = egui::Modal::new(modal_id()).show(ctx, |ui| {
        ui.set_width(420.0);
        ui.heading(texts.get(title(state.mode)));
        match state.mode {
            FilterDialogMode::Resource => resource_combo(ui, texts, state),
            FilterDialogMode::DateRange => date_fields(ui, texts, state),
            FilterDialogMode::Custom => custom_fields(ui, texts, state),
        }
        error_line(ui, state.error.as_deref());
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button(texts.get(TextId::FilterApply)).clicked() {
                outcome = FilterOutcome::Apply;
            }
            if ui.button(texts.get(TextId::TaskInfoCancel)).clicked() {
                outcome = FilterOutcome::Cancel;
            }
        });
    });
    let escape = response.is_top_modal
        && !response.any_popup_open
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if escape {
        outcome = FilterOutcome::Cancel;
    }
    if outcome == FilterOutcome::Open
        && super::enter_pressed(ctx, armed, response.is_top_modal, response.any_popup_open)
    {
        outcome = FilterOutcome::Apply;
    }
    outcome
}

/// Liste des ressources (vide : message « aucune ressource »).
fn resource_combo(ui: &mut egui::Ui, texts: &Texts, state: &mut FilterDialog) {
    ui.horizontal(|ui| {
        ui.label(texts.get(TextId::FilterResource));
        if state.resources.is_empty() {
            ui.weak(texts.get(TextId::FilterNoResource));
            return;
        }
        let selected = state.resource.clone().unwrap_or_default();
        let label = texts.get(TextId::FilterResource);
        egui::ComboBox::from_id_salt("plannercraft.filter.resource")
            .selected_text(selected)
            .show_ui(ui, |ui| {
                for name in &state.resources {
                    ui.selectable_value(&mut state.resource, Some(name.clone()), name);
                }
            })
            .response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, label.clone()));
    });
}

/// Champs « Du » et « Au ».
fn date_fields(ui: &mut egui::Ui, texts: &Texts, state: &mut FilterDialog) {
    for (label, value) in [
        (TextId::FilterFrom, &mut state.from),
        (TextId::FilterTo, &mut state.to),
    ] {
        let label = texts.get(label);
        ui.horizontal(|ui| {
            ui.label(&label);
            text_field(ui, &label, value, true, 200.0);
        });
    }
}

/// Critères du filtre personnalisé.
fn custom_fields(ui: &mut egui::Ui, texts: &Texts, state: &mut FilterDialog) {
    for (label, value) in [
        (TextId::FilterCritical, &mut state.critical),
        (TextId::FilterLate, &mut state.late),
        (TextId::FilterIncomplete, &mut state.incomplete),
        (TextId::FilterMilestones, &mut state.milestone),
        (TextId::FilterSummaries, &mut state.summary),
        (TextId::FilterActiveOnly, &mut state.active),
    ] {
        ui.checkbox(value, texts.get(label));
    }
    ui.add_enabled_ui(!state.resources.is_empty(), |ui| {
        ui.checkbox(&mut state.use_resource, texts.get(TextId::FilterUseResource));
    });
    if state.use_resource {
        resource_combo(ui, texts, state);
    }
    date_fields(ui, texts, state);
    ui.horizontal(|ui| {
        let label = texts.get(TextId::FilterOutlineMax);
        ui.label(&label);
        let all = texts.get(TextId::OutlineAll);
        let shown = state
            .outline_max
            .map_or_else(|| all.clone(), |level| level.to_string());
        egui::ComboBox::from_id_salt("plannercraft.filter.outline")
            .selected_text(shown)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut state.outline_max, None, all.as_str());
                for level in 1..=u16::from(MAX_OUTLINE_LEVEL) {
                    ui.selectable_value(&mut state.outline_max, Some(level), level.to_string());
                }
            })
            .response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, label.clone()));
    });
    let label = texts.get(TextId::FilterNameContains);
    ui.horizontal(|ui| {
        ui.label(&label);
        text_field(ui, &label, &mut state.name_contains, true, 200.0);
    });
    ui.checkbox(&mut state.related, texts.get(TextId::FilterRelated));
}
