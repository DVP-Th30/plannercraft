//! Fenêtre « Mise en page » (H-UI-50, H-UI-51) : papier, orientation,
//! marges, ajustement, en-tête et pied de page à trois zones. Contrat posé
//! par le lot L0, implémenté en L15.
//!
//! La même fenêtre porte le **mode impression** (H-UI-68) : ouverte par
//! Fichier → Mode impression ou le bouton de la barre d'outils, elle
//! applique au brouillon les colonnes d'impression (N°, Nom, Durée, Début,
//! Fin par défaut, indépendantes des colonnes de l'écran) et l'ajustement
//! « 1 page en largeur ». Réglages : format (A4, A3, Letter, Tabloid),
//! orientation, marges, ajustement (échelle, N pages en largeur, N × M),
//! table répétée, en-tête et pied par zones (jetons insérables), cartouche
//! (H-UI-66), colonnes imprimées, « graphique seul » (table masquée,
//! H-UI-64), chronologie en tête de la première page (H-UI-82, cochée
//! d'office par le mode impression quand la frise est affichée). L'aperçu est paginé page par page depuis `paginate`, sur la
//! même scène que le PDF : l'export PDF ou SVG de la fenêtre est
//! identique à l'aperçu. « Enregistrer avec le projet » écrit la mise en
//! page dans le style du document (`style.apply { patch: { page } }`, une
//! entrée d'annulation). Fermer ne change rien : la vue écran (colonnes,
//! zoom) n'est jamais touchée. Logo (`page.logo`) non proposé : non
//! dessiné par le PDF (coupe de L7).
//!
//! Lignes imprimées (relecture finale de J8, F1) : celles de la vue
//! affichée, comme MS Project qui imprime la vue courante. Filtre, niveau
//! de plan et récapitulatives repliées s'appliquent à l'aperçu et à ses
//! exports (`RowFilter.uids`) ; l'ordre reste celui des ID (un tri de
//! l'écran n'est pas imprimé).

use std::collections::BTreeSet;

use plannercraft_automation::Session;
use plannercraft_automation::render_text::SessionTexts;
use plannercraft_automation::text::Language;
use plannercraft_core::TaskUid;
use plannercraft_render::geometry::{Token, Viewport};
use plannercraft_render::paginate::{Pages, PaginateOptions, fit_zoom, layout_pages_timeline};
use plannercraft_render::style::{Fit, PageStyle, Paper, PlanStyle, Slot, TableColumn, Variant};
use plannercraft_render::{Lang, RenderLimits, RowFilter, Zoom};

use crate::i18n::{Arg, TextId, Texts, token_label};

/// Lignes du planning paginées au plus (comme l'export PDF de
/// `plannercraft-automation`).
const MAX_ROWS: u32 = 20_000;

/// Jetons proposés à l'insertion.
const TOKENS: [(Token, &str); 7] = [
    (Token::Project, "{projet}"),
    (Token::StatusDate, "{date_etat}"),
    (Token::Page, "{page}"),
    (Token::Pages, "{pages}"),
    (Token::PrintDate, "{date_impression}"),
    (Token::File, "{fichier}"),
    (Token::ScheduleTitle, "{planning}"),
];

/// Formats de papier proposés.
const PAPERS: [(Paper, &str); 4] = [
    (Paper::A4, "A4"),
    (Paper::A3, "A3"),
    (Paper::Letter, "Letter"),
    (Paper::Tabloid, "Tabloid"),
];

/// Forme de l'ajustement (H-UI-50).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FitKind {
    /// Échelle en %.
    Scale,
    /// N pages en largeur.
    Wide,
    /// N pages en largeur, M en hauteur.
    WideTall,
}

/// Tâches imprimées : lignes de la vue affichée (`None` : toutes).
pub type PrintRows = Option<BTreeSet<TaskUid>>;

/// Aperçu calculé : clé (brouillon, document, langue, lignes imprimées) et
/// pages (ou refus).
type Preview = (
    (PageStyle, (u64, u64), Language, PrintRows),
    Result<Pages, String>,
);

/// Brouillon de la fenêtre.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageSetupState {
    /// Ouverte en mode impression (H-UI-68).
    pub print_mode: bool,
    /// Mise en page en cours de réglage.
    pub page: PageStyle,
    /// Page de l'aperçu affichée (0 = première).
    pub page_index: usize,
    /// Aperçu de la dernière image.
    preview: Option<Preview>,
}

/// Réponse de la fenêtre à une image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageSetupOutcome {
    /// Rien.
    Open,
    /// Fermer sans rien écrire.
    Close,
    /// Écrire la mise en page dans le style du document.
    Save,
    /// Exporter en PDF (aperçu).
    ExportPdf,
    /// Exporter en SVG.
    ExportSvg,
}

impl PageSetupState {
    /// Fenêtre ouverte sur la mise en page du style `style` ; mode
    /// impression : colonnes d'impression par défaut si le style n'en a pas,
    /// ajustement à une page en largeur si l'échelle était libre, case
    /// « Chronologie » cochée d'office si la frise est affichée à l'écran
    /// (`timeline_shown`, H-UI-82).
    #[must_use]
    pub fn open(style: &PlanStyle, print_mode: bool, timeline_shown: bool) -> Self {
        let mut page = style.page.clone();
        if print_mode {
            page.timeline |= timeline_shown;
            if page.columns.is_none() {
                page.columns = Some(TableColumn::PRINT_DEFAULT.to_vec());
            }
            if matches!(page.fit, Fit::Scale(_)) {
                page.fit = Fit::PagesWide(1);
            }
        }
        Self {
            print_mode,
            page,
            page_index: 0,
            preview: None,
        }
    }

    /// Pages de l'aperçu (recalculées si le brouillon, le document ou les
    /// lignes imprimées ont changé).
    ///
    /// # Errors
    ///
    /// Texte du refus de la pagination (plus de 1 000 pages).
    pub fn pages(
        &mut self,
        session: &Session,
        style: &PlanStyle,
        key: (u64, u64),
        lang: Language,
        rows: Option<&BTreeSet<TaskUid>>,
    ) -> Result<&Pages, &str> {
        let fresh = self.preview.as_ref().is_some_and(|((page, k, l, r), _)| {
            *page == self.page && *k == key && *l == lang && r.as_ref() == rows
        });
        if !fresh {
            let mut st = style.clone();
            st.page = self.page.clone();
            let pages = build_pages_rows(session, &st, lang, rows);
            let wanted = (self.page.clone(), key, lang, rows.cloned());
            self.preview = Some((wanted, pages));
        }
        match &self.preview {
            Some((_, Ok(pages))) => Ok(pages),
            Some((_, Err(e))) => Err(e.as_str()),
            None => Err(""),
        }
    }

    /// Pages de la dernière image de l'aperçu (`None` : pas encore
    /// calculées, ou refus).
    #[must_use]
    pub fn preview_pages(&self) -> Option<&Pages> {
        self.preview.as_ref().and_then(|(_, pages)| pages.as_ref().ok())
    }

    /// Style du document avec la mise en page du brouillon.
    #[must_use]
    pub fn styled(&self, style: &PlanStyle) -> PlanStyle {
        let mut st = style.clone();
        st.page = self.page.clone();
        st
    }
}

/// Langue du rendu.
const fn render_lang(lang: Language) -> Lang {
    match lang {
        Language::Fr => Lang::Fr,
        Language::En => Lang::En,
    }
}

/// Pages du planning de `session` avec le style `st`, toutes les tâches
/// (voir [`build_pages_rows`]).
///
/// # Errors
///
/// Texte du refus de la pagination (plus de 1 000 pages).
pub fn build_pages(session: &Session, st: &PlanStyle, lang: Language) -> Result<Pages, String> {
    build_pages_rows(session, st, lang, None)
}

/// Pages du planning de `session` avec le style `st`, tâches `rows` seules
/// (`None` : toutes) : même chemin que l'export PDF (`fit_zoom`,
/// `layout_pages_timeline`, variante d'impression), textes de la session
/// (règle 3) ; chronologie en tête de la première page si la mise en page
/// la demande (`page.timeline`, données de
/// `plannercraft_automation::timeline::print_input`, H-UI-82).
///
/// # Errors
///
/// Texte du refus de la pagination (plus de 1 000 pages).
pub fn build_pages_rows(
    session: &Session,
    st: &PlanStyle,
    lang: Language,
    rows: Option<&BTreeSet<TaskUid>>,
) -> Result<Pages, String> {
    let vp = Viewport {
        zoom: fit_zoom(&st.page).unwrap_or(Zoom::Week),
        filter: RowFilter {
            uids: rows.cloned(),
            ..RowFilter::default()
        },
        limits: RenderLimits {
            max_rows: MAX_ROWS,
            ..RenderLimits::default()
        },
        ..Viewport::new(Variant::Print, render_lang(lang))
    };
    let texts = SessionTexts { session, lang };
    let options = PaginateOptions {
        legend_every_page: st.legend.every_page,
    };
    let timeline = plannercraft_automation::timeline::print_input(session, st, lang);
    layout_pages_timeline(
        session.project(),
        session.schedule(),
        st,
        &texts,
        &vp,
        options,
        timeline.as_ref(),
    )
    .0
    .map_err(|e| e.to_string())
}

/// PDF des pages de l'aperçu (H-R13 : date de création = « maintenant »
/// de la session ; `plannercraft_automation::pdf::pages_pdf`).
///
/// # Errors
///
/// Texte du refus de `plannercraft-pdf`.
pub fn pdf_bytes(session: &Session, pages: &Pages, lang: Language) -> Result<Vec<u8>, String> {
    plannercraft_automation::pdf::pages_pdf(session, pages, lang)
}

/// Identifiant de la fenêtre (couche modale d'egui).
#[must_use]
pub fn modal_id() -> egui::Id {
    egui::Id::unique("plannercraft.page-setup")
}

/// Libellé d'une colonne imprimée (titres de la table de l'écran).
const fn column_label(c: TableColumn) -> TextId {
    match c {
        TableColumn::Id => TextId::ColId,
        TableColumn::Name => TextId::PsColumnName,
        TableColumn::Duration => TextId::ColDuration,
        TableColumn::Start => TextId::ColStart,
        TableColumn::Finish => TextId::ColFinish,
        TableColumn::Percent => TextId::ColPercentComplete,
        TableColumn::Resources => TextId::ColResourceNames,
    }
}

/// Entrées de la fenêtre.
pub struct PageSetupInput<'a> {
    /// Session du document.
    pub session: &'a Session,
    /// Style effectif du document.
    pub style: &'a PlanStyle,
    /// Clé du document.
    pub key: (u64, u64),
    /// Langue.
    pub lang: Language,
    /// Tâches imprimées (lignes de la vue affichée ; `None` : toutes).
    pub rows: Option<&'a BTreeSet<TaskUid>>,
}

/// Réglages (colonne de gauche de la fenêtre).
#[allow(clippy::too_many_lines, reason = "suite de réglages, un bloc chacun")]
fn settings(ui: &mut egui::Ui, texts: &Texts, page: &mut PageStyle) {
    egui::Grid::new("plannercraft.page-setup-grid")
        .num_columns(2)
        .spacing(egui::vec2(8.0, 6.0))
        .show(ui, |ui| {
            // Colonnes imprimées d'abord (mode impression, H-UI-68).
            ui.label(texts.get(TextId::PsColumns));
            ui.vertical(|ui| {
                let mut chart_only = page.columns.as_ref().is_some_and(Vec::is_empty);
                if ui
                    .checkbox(&mut chart_only, texts.get(TextId::PsChartOnly))
                    .changed()
                {
                    page.columns = Some(if chart_only {
                        Vec::new()
                    } else {
                        TableColumn::PRINT_DEFAULT.to_vec()
                    });
                }
                ui.add_enabled_ui(!chart_only, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for c in TableColumn::ALL {
                            let current = page
                                .columns
                                .clone()
                                .unwrap_or_else(|| TableColumn::CHART_ONLY.to_vec());
                            let mut on = current.contains(&c);
                            if ui.checkbox(&mut on, texts.get(column_label(c))).changed() {
                                let next: Vec<TableColumn> = TableColumn::ALL
                                    .into_iter()
                                    .filter(|x| if *x == c { on } else { current.contains(x) })
                                    .collect();
                                page.columns = Some(next);
                            }
                        }
                    });
                });
            });
            ui.end_row();
            // Chronologie en tête de la première page (H-UI-82).
            ui.label("");
            ui.checkbox(&mut page.timeline, texts.get(TextId::RibbonTimeline));
            ui.end_row();
            ui.label(texts.get(TextId::PsPaper));
            ui.horizontal(|ui| {
                for (paper, label) in PAPERS {
                    ui.radio_value(&mut page.paper, paper, label);
                }
            });
            ui.end_row();
            ui.label("");
            ui.horizontal(|ui| {
                ui.radio_value(&mut page.landscape, true, texts.get(TextId::PsLandscape));
                ui.radio_value(&mut page.landscape, false, texts.get(TextId::PsPortrait));
            });
            ui.end_row();
            ui.label(texts.get(TextId::PsMargins));
            ui.horizontal(|ui| {
                for (i, label) in [
                    TextId::PsMarginTop,
                    TextId::PsMarginRight,
                    TextId::PsMarginBottom,
                    TextId::PsMarginLeft,
                ]
                .into_iter()
                .enumerate()
                {
                    if let Some(m) = page.margins_mm.get_mut(i) {
                        let name = format!("{} — {}", texts.get(TextId::PsMargins), texts.get(label));
                        ui.label(texts.get(label));
                        named(&ui.add(egui::DragValue::new(m).range(0..=50)), &name);
                    }
                }
            });
            ui.end_row();
            ui.label(texts.get(TextId::PsFit));
            ui.vertical(|ui| {
                let kind = match page.fit {
                    Fit::Scale(_) => FitKind::Scale,
                    Fit::PagesWide(_) => FitKind::Wide,
                    Fit::PagesWideTall(..) => FitKind::WideTall,
                };
                let mut chosen = kind;
                ui.radio_value(&mut chosen, FitKind::Wide, texts.get(TextId::PsFitWide));
                ui.radio_value(&mut chosen, FitKind::WideTall, texts.get(TextId::PsFitWideTall));
                ui.radio_value(&mut chosen, FitKind::Scale, texts.get(TextId::PsFitScale));
                if chosen != kind {
                    page.fit = match chosen {
                        FitKind::Scale => Fit::Scale(100),
                        FitKind::Wide => Fit::PagesWide(1),
                        FitKind::WideTall => Fit::PagesWideTall(1, 1),
                    };
                }
                ui.horizontal(|ui| match &mut page.fit {
                    Fit::Scale(p) => {
                        let r = ui.add(egui::DragValue::new(p).range(10..=400).suffix(" %"));
                        named(&r, &texts.get(TextId::PsFitScale));
                    }
                    Fit::PagesWide(n) => {
                        let r = ui.add(egui::DragValue::new(n).range(1..=20));
                        named(&r, &texts.get(TextId::PsFitWide));
                    }
                    Fit::PagesWideTall(n, m) => {
                        let r = ui.add(egui::DragValue::new(n).range(1..=20));
                        named(&r, &texts.get(TextId::PsFitWide));
                        ui.label(texts.get(TextId::PsPagesTall));
                        let r = ui.add(egui::DragValue::new(m).range(1..=20));
                        named(&r, &texts.get(TextId::PsPagesTall));
                    }
                });
            });
            ui.end_row();
            ui.label("");
            ui.checkbox(&mut page.repeat_table, texts.get(TextId::PsRepeatTable));
            ui.end_row();
            for (title, slots, lines) in [
                (TextId::PsHeader, &mut page.header, 5_usize),
                (TextId::PsFooter, &mut page.footer, 3_usize),
            ] {
                ui.label(texts.get(title));
                ui.vertical(|ui| {
                    for (slot, zone) in
                        slots
                            .iter_mut()
                            .zip([TextId::PsZoneLeft, TextId::PsZoneCenter, TextId::PsZoneRight])
                    {
                        ui.horizontal(|ui| {
                            let name = format!("{} — {}", texts.get(title), texts.get(zone));
                            ui.label(texts.get(zone));
                            let response = ui.add(
                                egui::TextEdit::multiline(&mut slot.0)
                                    .desired_rows(1)
                                    .desired_width(260.0),
                            );
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(egui::Role::TextInput, true, name.clone())
                            });
                        });
                        bound_slot(slot, lines);
                    }
                });
                ui.end_row();
            }
            ui.label("");
            ui.menu_button(texts.get(TextId::PsInsertToken), |ui| {
                for (token, text) in TOKENS {
                    if ui.button(texts.get(token_label(token))).clicked()
                        && let Some(center) = page.header.get_mut(1)
                    {
                        center.0.push_str(text);
                        bound_slot(center, 5);
                    }
                }
            });
            ui.end_row();
            ui.label("");
            ui.checkbox(&mut page.title_block, texts.get(TextId::PsTitleBlock));
            ui.end_row();
        });
}

/// Nom accessible d'un champ numérique.
fn named(response: &egui::Response, name: &str) {
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::SpinButton, true, name));
}

/// Bornes d'une zone (H-UI-51) : `lines` lignes et 256 caractères au plus.
fn bound_slot(slot: &mut Slot, lines: usize) {
    if slot.0.split('\n').count() > lines {
        slot.0 = slot.0.split('\n').take(lines).collect::<Vec<_>>().join("\n");
    }
    if slot.0.chars().count() > plannercraft_render::style::MAX_SLOT_CHARS {
        slot.0 = slot
            .0
            .chars()
            .take(plannercraft_render::style::MAX_SLOT_CHARS)
            .collect();
    }
}

/// Dessine la fenêtre (une image) ; rend la réponse de l'utilisateur.
pub fn show(
    ctx: &egui::Context,
    texts: &Texts,
    state: &mut PageSetupState,
    input: &PageSetupInput<'_>,
) -> PageSetupOutcome {
    let mut outcome = PageSetupOutcome::Open;
    let inter = ctx.fonts(|f| {
        f.families()
            .contains(&egui::FontFamily::Name(crate::theme::PLAN_FAMILY.into()))
    });
    let title = if state.print_mode {
        TextId::PrintModeTitle
    } else {
        TextId::PageSetupTitle
    };
    let response = egui::Modal::new(modal_id()).show(ctx, |ui| {
        ui.set_width(1_120.0_f32.min(ctx.content_rect().width() - 40.0).max(400.0));
        ui.heading(texts.get(title));
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(480.0);
                egui::ScrollArea::vertical()
                    .max_height(400.0)
                    .show(ui, |ui| settings(ui, texts, &mut state.page));
            });
            ui.add_space(16.0);
            ui.vertical(|ui| {
                ui.strong(texts.get(TextId::PsPreview));
                let index = state.page_index;
                let mut count = 0_usize;
                match state.pages(input.session, input.style, input.key, input.lang, input.rows) {
                    Ok(pages) => {
                        count = pages.pages.len();
                        let index = index.min(count.saturating_sub(1));
                        let room = egui::vec2(540.0, 340.0);
                        let (rect, _) = ui.allocate_exact_size(room, egui::Sense::hover());
                        let page_rect = crate::view::scene_paint::page_rect(pages, rect);
                        if let Some(page) = pages.pages.get(index) {
                            let painter = ui.painter_at(rect);
                            crate::view::scene_paint::paint_page(&painter, pages, page, page_rect, inter);
                        }
                        let scale = i64::from(pages.scale / 100);
                        ui.label(texts.get_args(TextId::PsScale, &[("scale", Arg::Count(scale))]));
                    }
                    Err(detail) => {
                        let text = texts
                            .get_args(TextId::PsError, &[("detail", Arg::Str(detail.to_owned().into()))]);
                        super::error_line(ui, Some(&text));
                    }
                }
                state.page_index = index.min(count.saturating_sub(1));
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(state.page_index > 0, egui::Button::new(texts.get(TextId::PsPrev)))
                        .clicked()
                    {
                        state.page_index = state.page_index.saturating_sub(1);
                    }
                    let page = i64::try_from(state.page_index.saturating_add(1)).unwrap_or(i64::MAX);
                    let pages = i64::try_from(count).unwrap_or(i64::MAX);
                    ui.label(texts.get_args(
                        TextId::PsPageOf,
                        &[("page", Arg::Count(page)), ("pages", Arg::Count(pages))],
                    ));
                    if ui
                        .add_enabled(
                            state.page_index.saturating_add(1) < count,
                            egui::Button::new(texts.get(TextId::PsNext)),
                        )
                        .clicked()
                    {
                        state.page_index = state.page_index.saturating_add(1);
                    }
                });
            });
        });
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button(texts.get(TextId::PsExportPdf)).clicked() {
                outcome = PageSetupOutcome::ExportPdf;
            }
            if ui.button(texts.get(TextId::PsExportSvg)).clicked() {
                outcome = PageSetupOutcome::ExportSvg;
            }
            ui.add_space(24.0);
            if ui.button(texts.get(TextId::PsSave)).clicked() {
                outcome = PageSetupOutcome::Save;
            }
            if ui.button(texts.get(TextId::PsClose)).clicked() {
                outcome = PageSetupOutcome::Close;
            }
        });
    });
    let escape = response.is_top_modal
        && !response.any_popup_open
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if escape {
        outcome = PageSetupOutcome::Close;
    }
    outcome
}

impl crate::app::App {
    /// Ouvre « Mise en page » (ou le mode impression) sur la mise en page
    /// du style effectif du document (sans document : rien).
    pub(crate) fn open_page_setup(&mut self, print_mode: bool) {
        let Some(document) = &self.document else {
            return;
        };
        let style = self.state.effective_style(document.project());
        let timeline_shown = self.state.timeline_visible;
        let body =
            super::DialogBody::PageSetup(Box::new(PageSetupState::open(&style, print_mode, timeline_shown)));
        self.view.edit.editing = None;
        self.view.drag = None;
        self.view.dialog = Some(super::DialogState {
            kind: crate::action::DialogKind::PageSetup { print_mode },
            body,
        });
    }

    /// Dessine « Mise en page » et applique sa réponse : export PDF ou SVG
    /// de l'aperçu, écriture de la mise en page dans le style du document
    /// (une entrée d'annulation), fermeture sans rien changer.
    pub(crate) fn show_page_setup(&mut self, ctx: &egui::Context) {
        use plannercraft_automation::command::Command;
        use plannercraft_automation::command::style::{StyleApply, TemplateJson, doc_style};
        let lang = self.text_language();
        let rows = self.printed_rows();
        let (
            Some(document),
            Some(super::DialogState {
                body: super::DialogBody::PageSetup(state),
                ..
            }),
        ) = (&self.document, &mut self.view.dialog)
        else {
            return;
        };
        let style = self.state.effective_style(document.project());
        let key = document.cache_key();
        let input = PageSetupInput {
            session: &document.session,
            style: &style,
            key,
            lang,
            rows: rows.as_ref(),
        };
        let outcome = show(ctx, &self.texts, state, &input);
        match outcome {
            PageSetupOutcome::Open => {}
            PageSetupOutcome::Close => self.close_dialog(),
            PageSetupOutcome::Save => {
                let patch = serde_json::to_value(&state.page)
                    .ok()
                    .map(|page| serde_json::json!({ "page": page }));
                // Document sans style : modèle par défaut et surcharge (sinon
                // la surcharge partirait de Classique).
                let template = doc_style(document.project())
                    .is_none()
                    .then(|| TemplateJson::from(self.state.default_template));
                let batch = super::batch_of(
                    vec![Command::StyleApply(StyleApply {
                        template,
                        patch,
                        reset: false,
                    })],
                    lang,
                );
                self.close_dialog();
                drop(self.execute(&batch, crate::app::EditOrigin::User));
            }
            PageSetupOutcome::ExportPdf => {
                let bytes = match state.pages(&document.session, &style, key, lang, rows.as_ref()) {
                    Ok(pages) => pdf_bytes(&document.session, pages, lang),
                    Err(e) => Err(e.to_owned()),
                };
                match bytes {
                    Ok(bytes) => self.deliver_export(crate::io::SaveKind::Pdf, bytes, ctx),
                    Err(detail) => self.report_page_error(detail),
                }
            }
            PageSetupOutcome::ExportSvg => {
                let st = state.styled(&style);
                let svg = svg_bytes(&document.session, &st, lang, rows);
                self.deliver_export(crate::io::SaveKind::Svg, svg, ctx);
            }
        }
    }

    /// Refus d'un export de l'aperçu, au bandeau.
    fn report_page_error(&mut self, detail: String) {
        let text = self
            .texts
            .get_args(TextId::PsError, &[("detail", Arg::Str(detail.into()))]);
        self.push_titled(
            plannercraft_core::Severity::Error,
            Some(TextId::ExportErrorTitle),
            text,
        );
    }
}

/// SVG du planning avec la mise en page du brouillon (colonnes, table
/// masquée), même zoom que la page ajustée, lignes de la vue affichée ;
/// chronologie en tête si la mise en page la demande (H-UI-82).
fn svg_bytes(session: &Session, st: &PlanStyle, lang: Language, rows: PrintRows) -> Vec<u8> {
    let req = plannercraft_render::GanttRequest {
        range: plannercraft_render::RangeSpec::Auto,
        zoom: fit_zoom(&st.page).unwrap_or(Zoom::Week),
        filter: RowFilter {
            uids: rows,
            ..RowFilter::default()
        },
        language: render_lang(lang),
        theme: plannercraft_render::Theme::Light,
        limits: RenderLimits::default(),
    };
    let texts = SessionTexts { session, lang };
    let timeline = plannercraft_automation::timeline::print_input(session, st, lang);
    plannercraft_render::svg_styled_timeline(
        session.project(),
        session.schedule(),
        &req,
        st,
        &texts,
        timeline.as_ref(),
    )
    .0
    .into_bytes()
}
