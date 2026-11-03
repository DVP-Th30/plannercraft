//! « Informations sur le projet » (J7, lot L12 ; brief § 6) : sens de
//! planification et date pilote (`project.set_anchor`), date d'état
//! (`project.set_status_date`, vide = NA), calendrier du projet
//! (`project.set_info { calendar }`, R-249). « OK » produit un seul lot,
//! des seuls champs changés (comparaison par texte, comme « Informations sur
//! la tâche »). Aucune date n'est calculée ici : au changement de sens, la
//! date proposée est la fin (ou le début) du projet lue sur le planning du
//! moteur.

use plannercraft_automation::command::ProjectInfoPatch;
use plannercraft_automation::command::SetStatusDate;
use plannercraft_automation::text::{DateRole, Language, TextContext, format_date, parse_date};
use plannercraft_automation::{AnchorSpec, Batch, Command, Patch, Session};
use plannercraft_core::{CalendarUid, ScheduleAnchor};

use super::{batch_of, error_line, text_field};
use crate::edit::EditError;
use crate::i18n::{TextId, Texts};

/// Brouillon de la fenêtre.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectInfoState {
    /// Clé du document à l'ouverture.
    pub key: (u64, u64),
    /// Planification à rebours depuis la fin.
    pub from_finish: bool,
    /// Date pilote (début, ou fin à rebours).
    pub date: String,
    /// Date d'état (vide : NA).
    pub status: String,
    /// Calendrier du projet.
    pub calendar: CalendarUid,
    /// Sens relu.
    original_from_finish: bool,
    /// Date pilote relue.
    original_date: String,
    /// Date d'état relue.
    original_status: String,
    /// Calendrier relu.
    original_calendar: CalendarUid,
    /// Début et fin du projet calculés par le moteur (proposés au
    /// changement de sens).
    computed: (String, String),
    /// Calendriers proposés (UID, nom), dans l'ordre du projet.
    pub calendars: Vec<(CalendarUid, String)>,
    /// Refus des champs (`"date"`, `"status"`) ou du registre (`""`).
    pub errors: Vec<(&'static str, String)>,
    /// Le document avait changé : brouillon relu, rien appliqué.
    pub stale: bool,
}

impl ProjectInfoState {
    /// Brouillon relu sur la session, textes dans la langue `lang`.
    #[must_use]
    pub fn open(s: &Session, key: (u64, u64), lang: Language) -> Self {
        let p = s.project();
        let ctx = TextContext::for_project(p, lang);
        let (from_finish, anchor) = match p.anchor {
            ScheduleAnchor::FromStart(m) => (false, m),
            ScheduleAnchor::FromFinish(m) => (true, m),
        };
        let date = format_date(anchor, &ctx);
        let status = p.status_date.map(|m| format_date(m, &ctx)).unwrap_or_default();
        let schedule = s.schedule();
        Self {
            key,
            from_finish,
            original_from_finish: from_finish,
            original_date: date.clone(),
            date,
            original_status: status.clone(),
            status,
            calendar: p.calendar,
            original_calendar: p.calendar,
            computed: (
                format_date(schedule.project_start, &ctx),
                format_date(schedule.project_finish, &ctx),
            ),
            calendars: p.calendars.iter().map(|c| (c.uid(), c.name.clone())).collect(),
            errors: Vec::new(),
            stale: false,
        }
    }

    /// Change le sens ; une date pilote non modifiée est remplacée par la
    /// date du planning du moteur pour ce sens (fin à rebours, début sinon).
    pub fn set_from_finish(&mut self, from_finish: bool) {
        if from_finish == self.from_finish {
            return;
        }
        let untouched = self.date.trim() == self.original_date.trim()
            || self.date.trim() == self.computed.0.trim()
            || self.date.trim() == self.computed.1.trim();
        self.from_finish = from_finish;
        if untouched {
            self.date = if from_finish == self.original_from_finish {
                self.original_date.clone()
            } else if from_finish {
                self.computed.1.clone()
            } else {
                self.computed.0.clone()
            };
        }
    }

    /// Lot de « OK » : `project.set_anchor`, `project.set_status_date`,
    /// `project.set_info { calendar }`, champs changés seulement ;
    /// `Ok(None)` si rien n'a changé.
    ///
    /// # Errors
    ///
    /// Refus des dates illisibles (champ, message).
    pub fn to_batch(
        &self,
        s: &Session,
        texts: &Texts,
        lang: Language,
    ) -> Result<Option<Batch>, Vec<(&'static str, String)>> {
        let ctx = TextContext::for_project(s.project(), lang);
        let mut ops = Vec::new();
        let mut errors = Vec::new();
        let anchor_changed =
            self.from_finish != self.original_from_finish || self.date.trim() != self.original_date.trim();
        if anchor_changed {
            let role = if self.from_finish {
                DateRole::Finish
            } else {
                DateRole::Start
            };
            match parse_date(&self.date, role, &ctx) {
                Ok(m) => ops.push(Command::ProjectSetAnchor(if self.from_finish {
                    AnchorSpec::Finish(m)
                } else {
                    AnchorSpec::Start(m)
                })),
                Err(e) => errors.push(("date", EditError::from(e).message(texts))),
            }
        }
        if self.status.trim() != self.original_status.trim() {
            if self.status.trim().is_empty() {
                ops.push(Command::ProjectSetStatusDate(SetStatusDate {
                    date: Patch::Clear,
                }));
            } else {
                match parse_date(&self.status, DateRole::Finish, &ctx) {
                    Ok(m) => ops.push(Command::ProjectSetStatusDate(SetStatusDate {
                        date: Patch::Set(m),
                    })),
                    Err(e) => errors.push(("status", EditError::from(e).message(texts))),
                }
            }
        }
        if self.calendar != self.original_calendar {
            ops.push(Command::ProjectSetInfo(ProjectInfoPatch {
                calendar: Patch::Set(self.calendar.get()),
                ..ProjectInfoPatch::default()
            }));
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        Ok((!ops.is_empty()).then(|| batch_of(ops, lang)))
    }

    /// Refus du champ `field`.
    fn error_for(&self, field: &str) -> Option<&str> {
        self.errors
            .iter()
            .find(|(f, _)| *f == field)
            .map(|(_, m)| m.as_str())
    }
}

/// Identifiant de la fenêtre (couche modale d'egui).
#[must_use]
pub fn modal_id() -> egui::Id {
    egui::Id::unique("plannercraft.project-info")
}

/// Dessine la fenêtre (une image) ; rend la réponse de l'utilisateur.
pub fn show(ctx: &egui::Context, texts: &Texts, state: &mut ProjectInfoState) -> super::task_info::Outcome {
    use super::task_info::Outcome;
    let mut outcome = Outcome::Open;
    let armed = super::enter_armed(ctx, None);
    let response = egui::Modal::new(modal_id()).show(ctx, |ui| {
        ui.set_width(460.0);
        ui.heading(texts.get(TextId::PiTitle));
        if state.stale {
            ui.colored_label(ui.visuals().warn_fg_color, texts.get(TextId::TaskInfoStale));
        }
        egui::Grid::new("plannercraft.project-info.grid")
            .num_columns(2)
            .show(ui, |ui| {
                let direction = texts.get(TextId::PiDirection);
                ui.label(&direction);
                let mut from_finish = state.from_finish;
                let choices = [(false, TextId::PiFromStart), (true, TextId::PiFromFinish)];
                let selected = choices
                    .iter()
                    .find(|(v, _)| *v == from_finish)
                    .map(|(_, t)| texts.get(*t))
                    .unwrap_or_default();
                let response = egui::ComboBox::from_id_salt("plannercraft.project-info.direction")
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        for (v, t) in choices {
                            ui.selectable_value(&mut from_finish, v, texts.get(t));
                        }
                    })
                    .response;
                response
                    .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, direction.clone()));
                state.set_from_finish(from_finish);
                ui.end_row();
                let date = texts.get(if state.from_finish {
                    TextId::PiFinish
                } else {
                    TextId::PiStart
                });
                ui.label(&date);
                ui.vertical(|ui| {
                    text_field(ui, &date, &mut state.date, true, 200.0);
                    error_line(ui, state.error_for("date"));
                });
                ui.end_row();
                let status = texts.get(TextId::PiStatus);
                ui.label(&status);
                ui.vertical(|ui| {
                    text_field(ui, &status, &mut state.status, true, 200.0);
                    ui.weak(texts.get(TextId::PiStatusHint));
                    error_line(ui, state.error_for("status"));
                });
                ui.end_row();
                let calendar = texts.get(TextId::PiCalendar);
                ui.label(&calendar);
                let selected = state
                    .calendars
                    .iter()
                    .find(|(uid, _)| *uid == state.calendar)
                    .map(|(_, n)| n.clone())
                    .unwrap_or_default();
                let mut chosen = state.calendar;
                let response = egui::ComboBox::from_id_salt("plannercraft.project-info.calendar")
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        for (uid, name) in &state.calendars {
                            ui.selectable_value(&mut chosen, *uid, name);
                        }
                    })
                    .response;
                response
                    .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, calendar.clone()));
                state.calendar = chosen;
                ui.end_row();
            });
        error_line(ui, state.error_for(""));
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
