//! Planning ouvert depuis MS Project (lot M3, ADR-0038, `docs/features/ui.md`
//! § 1.56) : bandeau de compte rendu (H-UI-232) et fenêtre « Différences
//! avec MS Project » (H-UI-233 à H-UI-235).
//!
//! Le bandeau est celui de « Planning retrouvé dans ce navigateur »
//! (`view/recovery.rs`) : même panneau en haut, fond neutre, une phrase et
//! ses boutons. La fenêtre reprend la table d'écarts de la fenêtre d'import
//! PDF (cellules de largeur fixe coupées par « … », défilement virtualisé,
//! dates en écart en couleur d'avertissement) ; elle est non modale, comme
//! « Mise en forme » : le planning reste visible et une ligne cliquée y
//! sélectionne sa tâche. Aucune date n'est calculée ici : la comparaison
//! vient de `plannercraft-mpp`, les dates calculées du planning de la
//! session (règle 3).

use chrono::Timelike as _;
use plannercraft_core::Moment;
use plannercraft_mpp::compare::GapCause;
use plannercraft_mpp::{FileFormat, MppError};

use crate::action::{Action, MppOp};
use crate::app::{App, OpenDocument};
use crate::format::format_date;
use crate::i18n::{Arg, TextId, Texts};
use crate::mpp_open::{DiffRow, NotTaken};
use crate::view::FrameCtx;

/// Texte d'un refus du lecteur `.mpp` (H-UI-236) : ce que c'est, et quoi
/// faire quand c'est utile.
#[must_use]
pub const fn refusal_text(error: &MppError) -> TextId {
    match error {
        MppError::Unsupported(FileFormat::Mpp12) => TextId::MppRefusedProject2007,
        MppError::Unsupported(FileFormat::Mpp9) => TextId::MppRefusedProject2003,
        MppError::Unsupported(FileFormat::Mpp8) => TextId::MppRefusedProject98,
        MppError::TooLarge => TextId::MppRefusedTooLarge,
        MppError::NotCompound => TextId::MppRefusedNotProjectFile,
        MppError::NotProject => TextId::MppRefusedOtherDocument,
        // MPP14 refusé (ne devrait pas l'être) ou fichier abîmé, protégé,
        // d'une disposition non reconnue : même conduite.
        MppError::Unsupported(FileFormat::Mpp14)
        | MppError::Corrupt
        | MppError::UnknownLayout
        | MppError::MissingStream(_)
        | MppError::LayoutUnrecognized { .. } => TextId::MppRefusedUnreadable,
    }
}

/// Contenu du bandeau de compte rendu (H-UI-232).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Banner {
    /// Gabarit (`.mpt`).
    pub template: bool,
    /// Tâches (lignes vides exclues).
    pub tasks: usize,
    /// Liens.
    pub links: usize,
    /// Tâches en écart réel (au-delà de deux minutes) ; 0 si le planning
    /// a été modifié depuis l'ouverture.
    pub gaps: u32,
    /// Non repris, constaté pour ce fichier.
    pub not_taken: NotTaken,
}

impl Banner {
    /// Bandeau du document `document`, s'il vient de MS Project et que le
    /// bandeau n'a pas été fermé.
    #[must_use]
    pub fn of(document: Option<&OpenDocument>) -> Option<Self> {
        let mpp = document?.mpp.as_deref()?;
        (!mpp.banner_closed).then_some(Self {
            template: mpp.template,
            tasks: mpp.tasks,
            links: mpp.links,
            gaps: mpp.real_gaps(),
            not_taken: mpp.not_taken,
        })
    }
}

/// Nombre en `Arg::Count`.
fn count(n: usize) -> Arg<'static> {
    Arg::Count(i64::try_from(n).unwrap_or(i64::MAX))
}

/// Première phrase du bandeau : « Planning MS Project ouvert : n tâches,
/// m liens. »
#[must_use]
pub fn opened_text(texts: &Texts, banner: &Banner) -> String {
    let tasks = texts.get_args(TextId::MppCountTasks, &[("count", count(banner.tasks))]);
    let links = texts.get_args(TextId::MppCountLinks, &[("count", count(banner.links))]);
    let id = if banner.template {
        TextId::MppBannerOpenedTemplate
    } else {
        TextId::MppBannerOpened
    };
    texts.get_args(
        id,
        &[
            ("tasks", Arg::Str(tasks.into())),
            ("links", Arg::Str(links.into())),
        ],
    )
}

/// « Non repris : … » pour ce fichier, ou `None` s'il n'y a rien à dire.
#[must_use]
pub fn not_taken_text(texts: &Texts, not: &NotTaken) -> Option<String> {
    let mut items = Vec::new();
    if not.contoured > 0 {
        items.push(texts.get(TextId::MppNotContoured));
    }
    if not.lag_percent > 0 {
        items.push(texts.get(TextId::MppNotLagPercent));
    }
    if not.resource_calendars > 0 {
        items.push(texts.get_args(
            TextId::MppNotResourceCalendars,
            &[("count", Arg::Count(i64::from(not.resource_calendars)))],
        ));
    }
    (!items.is_empty()).then(|| {
        texts.get_args(
            TextId::MppBannerNotTaken,
            &[("items", Arg::Str(items.join(", ").into()))],
        )
    })
}

/// Phrase des tâches en écart : « 12 tâches n'ont pas les mêmes dates que
/// dans MS Project. »
#[must_use]
pub fn gaps_text(texts: &Texts, gaps: u32) -> String {
    texts.get_args(TextId::MppGaps, &[("count", Arg::Count(i64::from(gaps)))])
}

/// Dessine le bandeau (H-UI-232) : rien hors d'un planning MS Project ou
/// une fois fermé. « Voir » ouvre la fenêtre des différences, « Masquer »
/// ferme le bandeau pour de bon.
pub fn banner(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, banner: Option<&Banner>) {
    let Some(banner) = banner else {
        return;
    };
    let texts = fc.texts;
    let frame = egui::Frame::side_top_panel(ui.style());
    egui::Panel::top("plannercraft_mpp_banner")
        .frame(frame)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(opened_text(texts, banner));
                if banner.gaps > 0 {
                    ui.label(gaps_text(texts, banner.gaps));
                    if ui.button(texts.get(TextId::MppBannerShow)).clicked() {
                        fc.actions.push(Action::Mpp(MppOp::ShowDifferences));
                    }
                }
                if let Some(text) = not_taken_text(texts, &banner.not_taken) {
                    ui.label(text);
                }
                if ui.button(texts.get(TextId::MppBannerDismiss)).clicked() {
                    fc.actions.push(Action::Mpp(MppOp::DismissBanner));
                }
            });
        });
}

/// Cause en clair (H-UI-234).
#[must_use]
pub const fn cause_text(cause: GapCause) -> TextId {
    match cause {
        GapCause::ContouredAssignment => TextId::MppCauseContoured,
        GapCause::StaggeredAssignments => TextId::MppCauseStaggered,
        GapCause::UnexplainedStart => TextId::MppCauseUnexplainedStart,
        GapCause::SubMinute => TextId::MppCauseSubMinute,
    }
}

/// Textes d'une paire de dates (MS Project, calculée) : la date seule, et
/// l'heure aussi quand les deux tombent le même jour à des heures
/// différentes (sinon l'écart serait invisible).
#[must_use]
pub fn pair_texts(texts: &Texts, recorded: Option<Moment>, computed: Option<Moment>) -> (String, String) {
    let labels = texts.format_labels();
    let with_time = matches!((recorded, computed), (Some(a), Some(b)) if a != b && a.day() == b.day());
    let one = |m: Option<Moment>| {
        m.map(|m| {
            let date = format_date(m, labels);
            if with_time {
                let t = m.to_naive().time();
                format!("{date} {:02}:{:02}", t.hour(), t.minute())
            } else {
                date
            }
        })
        .unwrap_or_default()
    };
    (one(recorded), one(computed))
}

/// Identifiant de la fenêtre.
#[must_use]
pub fn window_id() -> egui::Id {
    egui::Id::unique("plannercraft.mpp-differences")
}

/// Largeurs minimales des colonnes (points) ; les quatre dates tiennent
/// avec l'heure (« jeu 07/08/25 08:00 »). N°, Nom et « Fin calculée »
/// : mots de la table de l'import PDF.
const COLUMN_WIDTHS: [f32; 7] = [40.0, 200.0, 140.0, 140.0, 140.0, 140.0, 300.0];

/// En-têtes des colonnes.
const HEADERS: [TextId; 7] = [
    TextId::PdfColId,
    TextId::PdfColName,
    TextId::MppColStartRecorded,
    TextId::MppColFinishRecorded,
    TextId::MppColStartComputed,
    TextId::PdfColFinishComputed,
    TextId::MppColCause,
];

/// Largeurs des colonnes : au moins leur en-tête (jamais coupé).
fn column_widths(ctx: &egui::Context, texts: &Texts) -> [f32; 7] {
    let font = egui::TextStyle::Body.resolve(&ctx.global_style());
    let mut widths = COLUMN_WIDTHS;
    for (width, id) in widths.iter_mut().zip(HEADERS) {
        let text = texts.get(id);
        let measured = ctx.fonts_mut(|f| {
            f.layout_no_wrap(text, font.clone(), egui::Color32::PLACEHOLDER)
                .size()
                .x
        });
        *width = width.max(measured + 6.0);
    }
    widths
}

/// Ce que la fenêtre affiche à cette image.
pub struct DiffView<'a> {
    /// Lignes en écart réel.
    pub rows: &'a [DiffRow],
    /// Tâches à l'arrondi (non listées).
    pub rounding: u32,
    /// Au moins une ligne est une feuille (« Garder les dates » a de quoi
    /// faire).
    pub can_keep: bool,
    /// « Garder les dates » a été appliqué et il ne reste rien.
    pub kept: bool,
    /// Tâche active du planning (ligne surlignée).
    pub active: Option<plannercraft_core::TaskUid>,
    /// Raccourci d'annulation de la plateforme (« Ctrl+Z », « ⌘Z »).
    pub undo: String,
}

/// Dessine la fenêtre (une image) ; rend les gestes de l'utilisateur.
pub fn window(ctx: &egui::Context, texts: &Texts, view: &DiffView<'_>) -> Vec<Action> {
    let mut actions = Vec::new();
    let widths = column_widths(ctx, texts);
    let spacing = ctx.global_style().spacing.item_spacing.x;
    let table_width: f32 = widths.iter().sum::<f32>() + 6.0 * spacing;
    let area = super::format_window::work_area(ctx);
    let width = table_width.min((area.width() - 40.0).max(320.0));
    let mut open = true;
    egui::Window::new(texts.get(TextId::MppDiffTitle))
        .id(window_id())
        .open(&mut open)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(area.center())
        .constrain(true)
        .movable(true)
        .resizable(false)
        .collapsible(false)
        .default_width(width)
        .show(ctx, |ui| {
            ui.set_width(width);
            body(ui, texts, view, widths, &mut actions);
        });
    if !open {
        actions.push(Action::Mpp(MppOp::CloseDifferences));
    }
    actions
}

/// Contenu de la fenêtre : phrase, table, action, « Fermer ».
fn body(ui: &mut egui::Ui, texts: &Texts, view: &DiffView<'_>, widths: [f32; 7], actions: &mut Vec<Action>) {
    let gaps = u32::try_from(view.rows.len()).unwrap_or(u32::MAX);
    if gaps == 0 {
        ui.strong(texts.get(TextId::MppDiffNone));
        if view.kept {
            ui.label(texts.get_args(
                TextId::MppDiffKept,
                &[("undo", Arg::Str(view.undo.as_str().into()))],
            ));
        }
    } else {
        ui.strong(gaps_text(texts, gaps));
        ui.label(texts.get(TextId::MppDiffExplain));
        ui.separator();
        let height = (ui.ctx().content_rect().height() * 0.45).clamp(110.0, 360.0);
        table(ui, texts, view, widths, height, actions);
    }
    if view.rounding > 0 {
        ui.label(texts.get_args(
            TextId::MppDiffRounding,
            &[("count", Arg::Count(i64::from(view.rounding)))],
        ));
    }
    ui.separator();
    if view.can_keep {
        if ui.button(texts.get(TextId::MppDiffKeep)).clicked() {
            actions.push(Action::Mpp(MppOp::KeepDates));
        }
        ui.label(texts.get_args(
            TextId::MppDiffKeepHint,
            &[("undo", Arg::Str(view.undo.as_str().into()))],
        ));
    } else if gaps > 0 {
        ui.label(texts.get(TextId::MppDiffSummariesOnly));
    }
    ui.horizontal(|ui| {
        if ui.button(texts.get(TextId::MppDiffClose)).clicked() {
            actions.push(Action::Mpp(MppOp::CloseDifferences));
        }
    });
}

/// Table des tâches en écart, virtualisée, `height` points au plus ; une
/// ligne cliquée sélectionne sa tâche dans le planning (H-UI-235).
fn table(
    ui: &mut egui::Ui,
    texts: &Texts,
    view: &DiffView<'_>,
    widths: [f32; 7],
    height: f32,
    actions: &mut Vec<Action>,
) {
    let cell = |ui: &mut egui::Ui, width: f32, text: egui::RichText| {
        let size = egui::vec2(width, ui.spacing().interact_size.y);
        ui.allocate_ui_with_layout(size, egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_width(width);
            ui.add(egui::Label::new(text).truncate().selectable(false));
        });
    };
    ui.horizontal(|ui| {
        for (id, width) in HEADERS.iter().zip(widths) {
            cell(ui, width, egui::RichText::new(texts.get(*id)).strong());
        }
    });
    let row_h = ui.spacing().interact_size.y;
    let warn = ui.visuals().warn_fg_color;
    let selected_fill = ui.visuals().selection.bg_fill.gamma_multiply(0.35);
    let hover_fill = ui.visuals().widgets.hovered.weak_bg_fill;
    egui::ScrollArea::vertical()
        .id_salt("plannercraft.mpp-differences.rows")
        .max_height(height)
        .auto_shrink([false, true])
        .show_rows(ui, row_h, view.rows.len(), |ui, range| {
            for row in view.rows.get(range).unwrap_or_default() {
                let (start_rec, start_calc) = pair_texts(texts, row.recorded.0, row.computed.0);
                let (finish_rec, finish_calc) = pair_texts(texts, row.recorded.1, row.computed.1);
                let mark = |text: String, on: bool| {
                    let t = egui::RichText::new(text);
                    if on { t.color(warn) } else { t }
                };
                let start_off = row.recorded.0 != row.computed.0;
                let finish_off = row.recorded.1 != row.computed.1;
                let mut name = egui::RichText::new(row.name.as_str());
                if row.summary {
                    name = name.strong();
                }
                let cause = row.cause.map(|c| texts.get(cause_text(c))).unwrap_or_default();
                let label = texts.get_args(
                    TextId::MppDiffRow,
                    &[
                        ("id", Arg::Count(i64::from(row.id))),
                        ("name", Arg::Str(row.name.as_str().into())),
                    ],
                );
                // Fond de la ligne posé avant ses textes (rempli après, quand
                // son rectangle est connu).
                let background = ui.painter().add(egui::Shape::Noop);
                let response = ui
                    .scope_builder(egui::UiBuilder::new().sense(egui::Sense::click()), |ui| {
                        ui.horizontal(|ui| {
                            cell(ui, widths[0], egui::RichText::new(row.id.to_string()));
                            cell(ui, widths[1], name);
                            cell(ui, widths[2], egui::RichText::new(start_rec));
                            cell(ui, widths[3], egui::RichText::new(finish_rec));
                            cell(ui, widths[4], mark(start_calc, start_off));
                            cell(ui, widths[5], mark(finish_calc, finish_off));
                            cell(ui, widths[6], egui::RichText::new(cause));
                        });
                    })
                    .response;
                let active = view.active == Some(row.uid);
                if active || response.hovered() {
                    let fill = if active {
                        selected_fill
                    } else {
                        hover_fill.gamma_multiply(0.5)
                    };
                    ui.painter()
                        .set(background, egui::Shape::rect_filled(response.rect, 2.0, fill));
                }
                response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, active, &label));
                if response.clicked() {
                    actions.push(Action::Mpp(MppOp::Reveal(row.uid)));
                }
            }
        });
}

impl App {
    /// Geste du compte rendu MS Project (lot M3).
    pub(crate) fn mpp_op(&mut self, op: MppOp) {
        match op {
            MppOp::ShowDifferences => {
                self.mpp_window = self
                    .document
                    .as_ref()
                    .and_then(|d| d.mpp.as_deref())
                    .is_some_and(crate::mpp_open::MppOpened::differences_available);
            }
            MppOp::CloseDifferences => self.mpp_window = false,
            MppOp::Reveal(uid) => {
                if self
                    .document
                    .as_ref()
                    .is_some_and(|d| d.project().tasks.contains(uid))
                {
                    self.selection = Some(uid);
                    self.sync_selection();
                    self.scroll_to_task();
                    self.clamp_gantt_scroll();
                }
            }
            MppOp::KeepDates => self.keep_mpp_dates(),
            MppOp::DismissBanner => {
                if let Some(mpp) = self.document.as_mut().and_then(|d| d.mpp.as_deref_mut()) {
                    mpp.banner_closed = true;
                }
            }
        }
    }

    /// « Garder les dates de MS Project » (R-299, H-UI-234) : un lot de
    /// commandes, une entrée d'annulation ; un refus s'affiche au bandeau
    /// des messages.
    fn keep_mpp_dates(&mut self) {
        let Some(document) = &self.document else {
            return;
        };
        let Some(mpp) = document.mpp.as_deref() else {
            return;
        };
        let batch = match mpp.keep_batch(&document.session) {
            Ok(Some(batch)) => batch,
            Ok(None) => return,
            Err(error) => {
                log::warn!("garder les dates de MS Project : {error}");
                return;
            }
        };
        if self.execute(&batch, crate::app::EditOrigin::User).is_ok()
            && let Some(document) = self.document.as_mut()
            && let Some(mpp) = document.mpp.as_deref_mut()
        {
            mpp.mark_kept(&document.session);
        }
    }

    /// Dessine la fenêtre « Différences avec MS Project » si elle est
    /// ouverte et a encore un objet (planning MS Project non modifié
    /// autrement) ; sinon la referme.
    pub(crate) fn show_mpp_differences(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        if !self.mpp_window {
            return;
        }
        let Some(document) = &self.document else {
            self.mpp_window = false;
            return;
        };
        let Some(mpp) = document.mpp.as_deref().filter(|m| m.differences_available()) else {
            self.mpp_window = false;
            return;
        };
        let rows = mpp.rows(&document.session);
        let undo = ctx.format_shortcut(&egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND,
            egui::Key::Z,
        ));
        let view = DiffView {
            rows: &rows,
            rounding: mpp.rounding(),
            can_keep: rows.iter().any(|r| !r.summary),
            kept: mpp.kept_applied(),
            active: self.selection,
            undo,
        };
        actions.extend(window(ctx, &self.texts, &view));
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests : un échec doit paniquer")]

    use super::{pair_texts, refusal_text};
    use crate::i18n::{Language, TextId, Texts};
    use plannercraft_mpp::{FileFormat, MppError};

    #[test]
    fn h_ui_236_refusals_say_what_and_what_to_do() {
        assert_eq!(
            refusal_text(&MppError::Unsupported(FileFormat::Mpp12)),
            TextId::MppRefusedProject2007
        );
        assert_eq!(refusal_text(&MppError::Corrupt), TextId::MppRefusedUnreadable);
        assert_eq!(
            refusal_text(&MppError::NotProject),
            TextId::MppRefusedOtherDocument
        );
        let texts = Texts::new(Language::Fr);
        let text = texts.get(TextId::MppRefusedProject2007);
        assert!(text.contains("Project 2007") && text.contains("XML"), "{text}");
    }

    #[test]
    fn h_ui_234_times_shown_only_when_needed() {
        let texts = Texts::new(Language::Fr);
        let at = |s: &str| Some(s.parse().unwrap());
        let (a, b) = pair_texts(&texts, at("2026-01-07T08:00"), at("2026-01-07T17:00"));
        assert!(a.ends_with("08:00") && b.ends_with("17:00"), "{a} / {b}");
        let (a, b) = pair_texts(&texts, at("2026-01-07T08:00"), at("2026-01-05T08:00"));
        assert!(!a.contains(':') && !b.contains(':'), "{a} / {b}");
        assert_eq!(pair_texts(&texts, None, None), (String::new(), String::new()));
    }
}
