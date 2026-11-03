//! Fenêtres modales (J7, ADR-0015 D6) : « Informations sur la tâche »
//! (lot L9), temps de travail, informations du projet, ligne de base,
//! replanifier le reste (lot L12), affectation des ressources (lot L13),
//! boîte d'un lien (lot L8).
//!
//! Une fenêtre à la fois, hébergée par `App` ; son brouillon est figé à
//! l'ouverture (`cache_key`) et « OK » produit un seul lot (une entrée
//! d'annulation). Aucune fenêtre n'est persistée. Pendant qu'une fenêtre
//! est ouverte, aucun raccourci de `KEYMAP` n'est lu et les touches non
//! prises par la fenêtre sont retirées de l'image (rien n'atteint la
//! table, lot L9).
//!
//! Les quatre fenêtres du lot L12 sont hébergées ici ([`App::open_window`],
//! [`App::show_window`]) : document changé entre l'ouverture et « OK » ⇒
//! brouillon relu, avertissement, rien appliqué ; refus du registre
//! (`EditOrigin::Dialog`) affiché dans la fenêtre, jamais dans le bandeau.
//!
//! J8, lot L10 : « Collage spécial… » (`paste.rs`) et l'aperçu de
//! l'import CSV (`csv_import.rs`, ouvert sans document) sont dessinés par
//! `App::show_dialog` avant les fenêtres du lot L12.
//!
//! « Affecter des ressources » (lot L13) est hébergée de même mais **non
//! modale** (`egui::Window`) : la table et le Gantt restent cliquables, la
//! fenêtre est relue sur la sélection et le document à chaque image où ils
//! ont changé, et chacun de ses boutons applique aussitôt son lot.
//!
//! Seconde passe UX (ADR-0019, lot L9) : « Champs personnalisés »
//! (`custom_fields.rs`) est hébergée comme les fenêtres du lot L12.
//!
//! Seconde passe UX (ADR-0019, lot L6) : le sélecteur de champ
//! (`field_picker.rs`) et « Paramètres de champ » (`field_settings.rs`)
//! règlent des préférences d'affichage (`UiState.columns`), sans brouillon
//! du document : hébergés par `ViewTransient::picker`, hors de
//! [`DialogState`].

pub mod assign;
pub mod baseline;
pub mod csv_import;
pub mod custom_fields;
pub mod field_picker;
pub mod field_settings;
pub mod filter;
pub mod gallery;
pub mod link;
pub mod page_setup;
pub mod paste;
pub mod project_info;
pub mod reschedule;
pub mod sort;
pub mod task_info;
pub mod working_time;

use plannercraft_automation::error::codes;
use plannercraft_automation::text::Language;
use plannercraft_automation::{Batch, Command, SideEffect};
use plannercraft_core::TaskUid;

use crate::action::DialogKind;
use crate::app::{App, EditOrigin, OpenDocument};

/// Lu **avant** le dessin d'une fenêtre : Entrée la validera-t-elle à cette
/// image ? Aucun widget focalisé, ou une zone de saisie d'une ligne (Entrée y
/// termine le champ, puis valide la fenêtre) ; jamais la zone multiligne
/// `multiline` (Notes : Entrée y va à la ligne), ni un bouton, une case ou
/// une liste focalisés (Entrée les actionne). Relecture finale J7, F8 :
/// aucune fenêtre ne se validait par Entrée, contrairement à MS Project.
pub(crate) fn enter_armed(ctx: &egui::Context, multiline: Option<egui::Id>) -> bool {
    let focused = ctx.memory(egui::Memory::focused);
    let on_multiline = multiline.is_some_and(|id| focused == Some(id));
    !on_multiline && (focused.is_none() || ctx.text_edit_focused())
}

/// Lu **après** le dessin : Entrée (sans modificateur) valide la fenêtre
/// du dessus si elle était armée ([`enter_armed`]) et qu'aucune liste
/// déroulante n'est ouverte ; la touche est consommée.
pub(crate) fn enter_pressed(
    ctx: &egui::Context,
    armed: bool,
    is_top_modal: bool,
    any_popup_open: bool,
) -> bool {
    armed
        && is_top_modal
        && !any_popup_open
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
}

/// Fenêtre ouverte (état transitoire de la vue, jamais sérialisé).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogState {
    /// Fenêtre.
    pub kind: DialogKind,
    /// Contenu propre à la fenêtre (brouillon).
    pub body: DialogBody,
}

/// Contenu d'une fenêtre ouverte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogBody {
    /// Fenêtre sans contenu propre (pas encore câblée par son lot).
    Empty,
    /// « Informations sur la tâche » (lot L9).
    TaskInfo(Box<task_info::TaskInfoState>),
    /// Boîte « Dépendance » (lot L8, H-UI-21).
    Link(Box<link::LinkState>),
    /// « Modifier le temps de travail » (lot L12).
    WorkingTime(Box<working_time::WorkingTimeState>),
    /// « Informations sur le projet » (lot L12).
    ProjectInfo(Box<project_info::ProjectInfoState>),
    /// « Ligne de base » (lot L12).
    Baseline(Box<baseline::BaselineState>),
    /// « Replanifier le travail restant » (lot L12).
    Reschedule(Box<reschedule::RescheduleState>),
    /// « Affecter des ressources » (lot L13, non modale).
    Assign(Box<assign::AssignState>),
    /// « Collage spécial… » (J8, lot L10).
    PasteSpecial(Box<paste::PasteSpecialState>),
    /// Aperçu de l'import CSV (J8, lot L10, H-UI-40).
    CsvImport(Box<csv_import::CsvImportState>),
    /// Galerie des modèles (J8, lot L15, H-UI-52).
    Gallery(Box<gallery::GalleryState>),
    /// « Mise en page » et mode impression (J8, lot L15, H-UI-68).
    PageSetup(Box<page_setup::PageSetupState>),
    /// « Champs personnalisés » (seconde passe UX, lot L9, H-UI-118),
    /// hébergée comme les fenêtres du lot L12.
    CustomFields(Box<custom_fields::CustomFieldsState>),
}

impl DialogBody {
    /// Fenêtre du lot L12, hébergée par [`App::show_window`].
    #[must_use]
    pub const fn is_window(&self) -> bool {
        matches!(
            self,
            Self::WorkingTime(_)
                | Self::ProjectInfo(_)
                | Self::Baseline(_)
                | Self::Reschedule(_)
                | Self::Assign(_)
                | Self::CustomFields(_)
        )
    }

    /// Clé du document vue par le brouillon (fenêtres du lot L12 ;
    /// « Affecter des ressources » se relit seule, sans avertissement).
    const fn key(&self) -> Option<(u64, u64)> {
        match self {
            Self::WorkingTime(s) => Some(s.key),
            Self::ProjectInfo(s) => Some(s.key),
            Self::Baseline(s) => Some(s.key),
            Self::Reschedule(s) => Some(s.key),
            Self::CustomFields(s) => Some(s.key),
            Self::Empty
            | Self::TaskInfo(_)
            | Self::Link(_)
            | Self::Assign(_)
            | Self::PasteSpecial(_)
            | Self::CsvImport(_)
            | Self::Gallery(_)
            | Self::PageSetup(_) => None,
        }
    }
}

/// Lot des opérations `ops`, dans la langue `lang` (textes saisis).
pub(crate) fn batch_of(ops: Vec<Command>, lang: Language) -> Batch {
    Batch {
        format_version: Batch::FORMAT_VERSION,
        lang: Some(lang),
        ops,
        label: None,
    }
}

/// Champ de texte d'une ligne, nommé `label` pour l'accessibilité ; grisé
/// si `enabled` est faux.
pub(crate) fn text_field(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    enabled: bool,
    width: f32,
) -> egui::Response {
    // Largeur réservée : dans une cellule de `Grid`, la largeur disponible
    // est celle de la colonne à l'image précédente (le champ ne grandirait
    // jamais).
    let response = ui
        .scope(|ui| {
            ui.set_min_width(width);
            ui.add_enabled(enabled, egui::TextEdit::singleline(value).desired_width(width))
        })
        .inner;
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

/// Réponse d'une fenêtre du lot L12 pour une image.
enum Shown {
    /// Rien.
    Open,
    /// « OK ».
    Ok,
    /// « Annuler » ou Échap.
    Cancel,
    /// Lot immédiat du temps de travail : « Créer » (vrai) ou « Supprimer ».
    Immediate(bool),
    /// Bouton à lot d'« Affecter des ressources ».
    Assign(assign::AssignOp),
}

impl App {
    /// Ouvre une fenêtre du lot L12 (une à la fois ; une saisie de cellule
    /// ou un geste en cours sont abandonnés). Sans document : rien.
    pub(crate) fn open_window(&mut self, kind: DialogKind) {
        let lang = self.text_language();
        let Some(document) = &self.document else {
            return;
        };
        let key = document.cache_key();
        let s = &document.session;
        let selected: Vec<TaskUid> = self.selected.uids().iter().copied().collect();
        let body = match kind {
            DialogKind::WorkingTime => {
                DialogBody::WorkingTime(Box::new(working_time::WorkingTimeState::open(s, key, lang, None)))
            }
            DialogKind::ProjectInformation => {
                DialogBody::ProjectInfo(Box::new(project_info::ProjectInfoState::open(s, key, lang)))
            }
            DialogKind::Baseline => {
                DialogBody::Baseline(Box::new(baseline::BaselineState::open(key, selected)))
            }
            DialogKind::Reschedule => DialogBody::Reschedule(Box::new(reschedule::RescheduleState::open(
                s, key, lang, selected,
            ))),
            DialogKind::AssignResources => DialogBody::Assign(Box::new(assign::AssignState::open(
                s,
                key,
                lang,
                self.selected.uids(),
            ))),
            DialogKind::CustomFields => {
                DialogBody::CustomFields(Box::new(custom_fields::CustomFieldsState::open(s, key)))
            }
            DialogKind::TaskInformation(_)
            | DialogKind::Link { .. }
            | DialogKind::PasteSpecial
            | DialogKind::CsvImport
            | DialogKind::Gallery
            | DialogKind::PageSetup { .. } => return,
        };
        self.view.edit.editing = None;
        self.view.drag = None;
        self.view.dialog = Some(DialogState { kind, body });
    }

    /// Dessine la fenêtre du lot L12 ouverte et applique sa réponse.
    pub(crate) fn show_window(&mut self, ctx: &egui::Context) {
        let lang = self.text_language();
        self.refresh_month_view();
        self.refresh_assign();
        let shown = match (
            self.document.as_ref(),
            self.view.dialog.as_mut().map(|d| &mut d.body),
        ) {
            (Some(document), Some(DialogBody::WorkingTime(state))) => {
                match working_time::show(ctx, &self.texts, &document.session, lang, state) {
                    working_time::WtOutcome::Open => Shown::Open,
                    working_time::WtOutcome::Ok => Shown::Ok,
                    working_time::WtOutcome::Cancel => Shown::Cancel,
                    working_time::WtOutcome::Create => Shown::Immediate(true),
                    working_time::WtOutcome::Delete => Shown::Immediate(false),
                }
            }
            (Some(_), Some(DialogBody::ProjectInfo(state))) => {
                simple(project_info::show(ctx, &self.texts, state))
            }
            (Some(_), Some(DialogBody::Baseline(state))) => simple(baseline::show(ctx, &self.texts, state)),
            (Some(_), Some(DialogBody::Reschedule(state))) => {
                simple(reschedule::show(ctx, &self.texts, state))
            }
            (Some(_), Some(DialogBody::CustomFields(state))) => {
                simple(custom_fields::show(ctx, &self.texts, state))
            }
            (Some(_), Some(DialogBody::Assign(state))) => match assign::show(ctx, &self.texts, state) {
                assign::AssignOutcome::Open => Shown::Open,
                assign::AssignOutcome::Close => Shown::Cancel,
                assign::AssignOutcome::Apply(op) => Shown::Assign(op),
            },
            _ => Shown::Cancel,
        };
        match shown {
            Shown::Open => {}
            Shown::Cancel => self.close_dialog(),
            Shown::Ok => self.commit_window(),
            Shown::Immediate(create) => self.working_time_immediate(create),
            Shown::Assign(op) => self.assign_apply(op),
        }
        // Mois changé pendant l'image : relu à la suivante.
        if let Some(DialogState {
            body: DialogBody::WorkingTime(state),
            ..
        }) = &self.view.dialog
            && !state
                .month_view
                .as_ref()
                .is_some_and(|c| c.shows(None, state.selected, state.year, state.month))
        {
            ctx.request_repaint();
        }
    }

    /// Relit « Affecter des ressources » si la sélection ou le document ont
    /// changé depuis l'image précédente (fenêtre non modale).
    fn refresh_assign(&mut self) {
        let lang = self.text_language();
        let (
            Some(document),
            Some(DialogState {
                body: DialogBody::Assign(state),
                ..
            }),
        ) = (&self.document, &mut self.view.dialog)
        else {
            return;
        };
        let key = document.cache_key();
        let selection = self.selected.uids();
        if !state.shows(key, selection) {
            state.reread(&document.session, key, lang, selection);
        }
    }

    /// Bouton à lot d'« Affecter des ressources » : un seul lot (une
    /// entrée d'annulation), rien s'il n'y a rien à faire ; refus du
    /// registre affiché dans la fenêtre, qui reste ouverte.
    fn assign_apply(&mut self, op: assign::AssignOp) {
        let lang = self.text_language();
        let Some(DialogState {
            body: DialogBody::Assign(state),
            ..
        }) = &self.view.dialog
        else {
            return;
        };
        let Some(batch) = state.to_batch(op, lang) else {
            return;
        };
        let result = self.execute(&batch, EditOrigin::Dialog);
        let message = result.as_ref().err().map(|error| {
            task_info::command_message(
                &self.texts,
                error,
                self.document.as_ref().map(OpenDocument::project),
                lang,
            )
        });
        let Some(DialogState {
            body: DialogBody::Assign(state),
            ..
        }) = &mut self.view.dialog
        else {
            return;
        };
        match message {
            Some(message) => state.error = Some(message),
            None => state.applied(),
        }
        // Relue tout de suite : l'image suivante montre le résultat.
        self.refresh_assign();
    }

    /// Relit sur le moteur le mois affiché par « Modifier le temps de
    /// travail » s'il a changé (document, calendrier, mois).
    fn refresh_month_view(&mut self) {
        let (
            Some(document),
            Some(DialogState {
                body: DialogBody::WorkingTime(state),
                ..
            }),
        ) = (&self.document, &mut self.view.dialog)
        else {
            return;
        };
        let key = document.cache_key();
        let current = state
            .month_view
            .as_ref()
            .is_some_and(|c| c.shows(Some(key), state.selected, state.year, state.month));
        if current {
            return;
        }
        let set = document.calendars();
        let view =
            working_time::month_view(document.project(), &set, state.selected, state.year, state.month);
        state.month_view = Some(working_time::MonthCache {
            key,
            calendar: state.selected,
            year: state.year,
            month: state.month,
            view,
        });
    }

    /// Document changé depuis l'ouverture : brouillon relu, avertissement ;
    /// rend vrai si c'est le cas (rien n'est alors appliqué).
    fn reread_if_stale(&mut self) -> bool {
        let lang = self.text_language();
        let (Some(document), Some(dialog)) = (&self.document, &mut self.view.dialog) else {
            return false;
        };
        let key = document.cache_key();
        if dialog.body.key().is_none_or(|k| k == key) {
            return false;
        }
        let s = &document.session;
        let selected: Vec<TaskUid> = self.selected.uids().iter().copied().collect();
        match &mut dialog.body {
            DialogBody::WorkingTime(state) => {
                let mut fresh = working_time::WorkingTimeState::open(s, key, lang, Some(state.selected));
                fresh.tab = state.tab;
                (fresh.year, fresh.month) = (state.year, state.month);
                fresh.stale = true;
                **state = fresh;
            }
            DialogBody::ProjectInfo(state) => {
                let mut fresh = project_info::ProjectInfoState::open(s, key, lang);
                fresh.stale = true;
                **state = fresh;
            }
            DialogBody::Baseline(state) => {
                state.key = key;
                state.selected = selected;
                state.confirm = None;
                state.overwrite = false;
                state.stale = true;
            }
            DialogBody::Reschedule(state) => {
                let mut fresh = reschedule::RescheduleState::open(s, key, lang, selected);
                fresh.selection_only = state.selection_only && !fresh.selected.is_empty();
                fresh.stale = true;
                **state = fresh;
            }
            DialogBody::CustomFields(state) => {
                let mut fresh = custom_fields::CustomFieldsState::open(s, key);
                fresh.kind = state.kind;
                fresh.selected = state.selected;
                fresh.stale = true;
                **state = fresh;
            }
            DialogBody::Empty
            | DialogBody::TaskInfo(_)
            | DialogBody::Link(_)
            | DialogBody::Assign(_)
            | DialogBody::PasteSpecial(_)
            | DialogBody::CsvImport(_)
            | DialogBody::Gallery(_)
            | DialogBody::PageSetup(_) => {}
        }
        true
    }

    /// « OK » d'une fenêtre du lot L12 : un seul lot (rien de changé ⇒
    /// fenêtre fermée sans lot) ; refus des champs ou du registre affichés
    /// dans la fenêtre. Ligne de base existante (`baseline.exists`) :
    /// confirmation demandée, rien appliqué.
    #[allow(
        clippy::too_many_lines,
        reason = "une branche par fenêtre hébergée, lot et refus"
    )]
    fn commit_window(&mut self) {
        if self.reread_if_stale() {
            return;
        }
        let lang = self.text_language();
        let (Some(document), Some(dialog)) = (&self.document, &mut self.view.dialog) else {
            self.close_dialog();
            return;
        };
        let s = &document.session;
        let batch = match &mut dialog.body {
            DialogBody::WorkingTime(state) => match state.to_batch(s, &self.texts, lang) {
                Ok(batch) => batch,
                Err(errors) => {
                    if let Some(first) = errors.first() {
                        let (calendar, tab) = (first.calendar, first.tab);
                        state.select(s, calendar, lang);
                        state.tab = tab;
                    }
                    state.errors = errors;
                    state.error = None;
                    return;
                }
            },
            DialogBody::ProjectInfo(state) => match state.to_batch(s, &self.texts, lang) {
                Ok(batch) => batch,
                Err(errors) => {
                    state.errors = errors;
                    return;
                }
            },
            DialogBody::Baseline(state) => Some(state.to_batch(lang)),
            DialogBody::Reschedule(state) => match state.to_batch(s, &self.texts, lang) {
                Ok(batch) => Some(batch),
                Err(message) => {
                    state.error = Some(message);
                    return;
                }
            },
            DialogBody::CustomFields(state) => match state.to_batch(&self.texts, lang) {
                Ok(batch) => batch,
                Err(message) => {
                    state.error = Some(message);
                    return;
                }
            },
            DialogBody::Empty
            | DialogBody::TaskInfo(_)
            | DialogBody::Link(_)
            | DialogBody::Assign(_)
            | DialogBody::PasteSpecial(_)
            | DialogBody::CsvImport(_)
            | DialogBody::Gallery(_)
            | DialogBody::PageSetup(_) => {
                return;
            }
        };
        let Some(batch) = batch else {
            self.close_dialog();
            return;
        };
        match self.execute(&batch, EditOrigin::Dialog) {
            Ok(_) => self.close_dialog(),
            Err(error) => {
                let message = task_info::command_message(
                    &self.texts,
                    &error,
                    self.document.as_ref().map(OpenDocument::project),
                    lang,
                );
                let Some(dialog) = &mut self.view.dialog else {
                    return;
                };
                match &mut dialog.body {
                    DialogBody::WorkingTime(state) => {
                        state.errors.clear();
                        if let Some((calendar, tab)) = working_time::locate(&batch, &error)
                            && let Some(document) = &self.document
                        {
                            state.select(&document.session, calendar, lang);
                            if let Some(tab) = tab {
                                state.tab = tab;
                            }
                        }
                        state.error = Some(message);
                    }
                    DialogBody::ProjectInfo(state) => state.errors = vec![("", message)],
                    DialogBody::Baseline(state) => {
                        if error.code == codes::BASELINE_EXISTS && !state.overwrite {
                            state.confirm = Some(error.message(lang));
                            state.error = None;
                        } else {
                            state.error = Some(message);
                        }
                    }
                    DialogBody::Reschedule(state) => state.error = Some(message),
                    DialogBody::CustomFields(state) => state.error = Some(message),
                    DialogBody::Empty
                    | DialogBody::TaskInfo(_)
                    | DialogBody::Link(_)
                    | DialogBody::Assign(_)
                    | DialogBody::PasteSpecial(_)
                    | DialogBody::CsvImport(_)
                    | DialogBody::Gallery(_)
                    | DialogBody::PageSetup(_) => {}
                }
            }
        }
    }

    /// « Créer » (`create`) ou « Supprimer » de « Modifier le temps de
    /// travail » : lot immédiat (une entrée d'annulation) ; les brouillons
    /// des autres calendriers sont gardés. Refus affiché dans la fenêtre
    /// (`calendar.in_use` avec ses utilisateurs, `calendar.name_taken`…).
    fn working_time_immediate(&mut self, create: bool) {
        if self.reread_if_stale() {
            return;
        }
        let lang = self.text_language();
        let Some(DialogState {
            body: DialogBody::WorkingTime(state),
            ..
        }) = &self.view.dialog
        else {
            return;
        };
        let batch = if create {
            match state.create_batch(lang) {
                Some(batch) => batch,
                None => return,
            }
        } else {
            state.delete_batch(lang)
        };
        let result = self.execute(&batch, EditOrigin::Dialog);
        let message = result.as_ref().err().map(|error| {
            task_info::command_message(
                &self.texts,
                error,
                self.document.as_ref().map(OpenDocument::project),
                lang,
            )
        });
        let created = result.ok().and_then(|outcome| {
            outcome.side_effects.iter().find_map(|e| match e {
                SideEffect::CalendarCreated(uid) => Some(*uid),
                _ => None,
            })
        });
        let (
            Some(document),
            Some(DialogState {
                body: DialogBody::WorkingTime(state),
                ..
            }),
        ) = (&self.document, &mut self.view.dialog)
        else {
            return;
        };
        if let Some(message) = message {
            state.error = Some(message);
            return;
        }
        state.after_own_change(&document.session, document.cache_key(), lang, created);
        if create {
            state.new_form = None;
        }
    }
}

/// Réponse d'une fenêtre simple.
const fn simple(outcome: task_info::Outcome) -> Shown {
    match outcome {
        task_info::Outcome::Open => Shown::Open,
        task_info::Outcome::Ok => Shown::Ok,
        task_info::Outcome::Cancel => Shown::Cancel,
    }
}
