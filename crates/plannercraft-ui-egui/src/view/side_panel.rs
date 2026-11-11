//! Volet latéral (ADR-0016 D10, J8 lot L12) : onglets « Opérations IA »
//! (H-UI-41 : écriture proposée par un agent en tête de file, table
//! avant/après, Accepter / Refuser, historique de session) et « Pourquoi
//! cette date ? » (H-UI-58 : `explain_date` de la tâche active, pilotes
//! cliquables).
//!
//! Règle 3 : le volet ne calcule rien. Les lignes avant/après viennent du
//! [`DiffView`] calculé **une fois** par le relais à l'arrivée de la demande
//! (`PendingApproval::diff`) ; les explications viennent du moteur
//! (`plannercraft_automation::explain::explain_date`), relues seulement quand la
//! tâche, la génération du document ou la langue changent
//! ([`SidePanelState::refresh_explain`]). Le volet ne prend jamais le focus
//! clavier : Accepter et Refuser suivent les règles du bandeau J7
//! ([`super::approval::decision_row`]).
//!
//! Onglet « Assistant » (ADR-0024, lot A1, H-UI-138 à H-UI-142) :
//! conversation avec l'assistant IA intégré, dessinée par
//! [`super::assistant`].
//!
//! Ouvert d'office à chaque demande d'un agent (onglet « Opérations IA »),
//! refermable (bouton du volet, Affichage → Opérations IA) ; fermé par
//! défaut. Le bandeau J7 reste affiché quand le volet est fermé ou sur
//! l'autre onglet.
//!
//! Lot A5 (H-UI-161 à H-UI-166) : toujours à portée, jamais gênant.
//! Bouton de l'assistant en haut à droite du ruban, Ctrl/Cmd+J et
//! Affichage → Assistant IA basculent le volet ([`SideOp::Toggle`],
//! onglet Assistant à l'ouverture) ; ouvert ou fermé et largeur mémorisés
//! dans `UiState.side_panel` ; largeur tirée par un séparateur
//! ([`panel_width`] : 320 points à 50 % de la fenêtre, double-clic ⇒
//! largeur par défaut) ; sous [`NARROW_WINDOW`] points de large, le volet
//! s'ouvre par-dessus la vue ([`overlays`]) au lieu de la rétrécir.

use std::collections::VecDeque;

use plannercraft_automation::command::TaskRef;
use plannercraft_automation::command::params::ExplainParams;
use plannercraft_automation::diff::{CriticalChange, DiffRow, DiffView, MAX_VIEW_ROWS, RowChange};
use plannercraft_automation::explain::{ExplainView, explain_date};
use plannercraft_automation::text::Language as ToolLanguage;
use plannercraft_core::{ConversionFactors, Minutes, Moment, TaskUid};

use super::FrameCtx;
use super::approval::{ApprovalBanner, decision_row};
use crate::action::{Action, UiOp};
use crate::app::OpenDocument;
use crate::format::{FormatLabels, format_date, format_slack};
use crate::i18n::{Arg, TextId};

/// Entrées au plus dans l'historique du panneau (règle 10 de J8).
pub const MAX_HISTORY: usize = 50;
/// Lignes au plus dans un diff affiché (règle 10 de J8) : celles du
/// [`DiffView`] du moteur.
pub const MAX_DIFF_ROWS: usize = MAX_VIEW_ROWS;

/// Largeur du volet par défaut, en points logiques.
pub const DEFAULT_WIDTH: f32 = 460.0;

/// Fenêtre plus étroite que ceci (points) : le volet s'ouvre par-dessus
/// la vue au lieu de la rétrécir (lot A5, H-UI-164).
pub const NARROW_WINDOW: f32 = 900.0;

/// Part maximale de la fenêtre donnée au volet (lot A5, H-UI-163).
pub const MAX_SHARE: f32 = 0.5;

/// Zone de saisie du séparateur : points à gauche du bord du volet, puis
/// dans le volet. Interface adoucie (lot U2, H-UI-172) : le bord du volet
/// est celui de la gouttière qui le sépare de la vue ; la zone (8 points,
/// inchangée) couvre exactement la gouttière.
const GRAB_OUT: f32 = 0.0;
const GRAB_IN: f32 = super::cards::GUTTER;

/// Largeur affichée du volet (lot A5, H-UI-163) : `wanted` (non finie ⇒
/// [`DEFAULT_WIDTH`]) bornée à `[320, 50 % de window]` (le plafond ne
/// descend pas sous 320), jamais plus large que la fenêtre ; fenêtre non
/// finie ou vide ⇒ 320. Toujours finie.
#[must_use]
pub fn panel_width(wanted: f32, window: f32) -> f32 {
    let min = f32::from(crate::state::SIDE_WIDTH_MIN);
    let wanted = if wanted.is_finite() { wanted } else { DEFAULT_WIDTH };
    if !window.is_finite() || window <= 0.0 {
        return min;
    }
    let max = (MAX_SHARE * window).max(min);
    wanted.clamp(min, max).min(window)
}

/// Le volet s'ouvre-t-il par-dessus la vue (fenêtre de moins de
/// [`NARROW_WINDOW`] points, H-UI-164) ?
#[must_use]
pub fn overlays(window: f32) -> bool {
    window.is_finite() && window < NARROW_WINDOW
}

/// Onglet du volet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SideTab {
    /// Opérations IA.
    AiOperations,
    /// Pourquoi cette date ?
    WhyThisDate,
    /// Assistant IA (ADR-0024, lot A1, H-UI-138) : onglet par défaut
    /// depuis le lot A5 (H-UI-165).
    #[default]
    Assistant,
}

/// Commande du volet (réducteur d'`App`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideOp {
    /// Bascule le volet (lot A5, H-UI-161) : fermé ⇒ ouvert sur l'onglet
    /// Assistant ; ouvert (quel que soit l'onglet) ⇒ fermé. Bouton de
    /// l'assistant, Ctrl/Cmd+J, Affichage → Assistant IA.
    Toggle,
    /// Ouvre le volet sur l'onglet donné.
    Open(SideTab),
    /// Referme le volet.
    Close,
    /// Ouvre le volet sur l'onglet donné, ou le referme s'il y est déjà
    /// (Affichage → Opérations IA depuis le lot A5).
    ToggleTab(SideTab),
    /// Largeur choisie, en points (séparateur, lot A5) : écrite dans
    /// `UiState.side_panel` par `App`, bornée ; sans effet ici.
    SetWidth(u16),
}

/// Issue d'une proposition d'un agent ou de l'assistant (H-UI-41).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AiOutcome {
    /// Acceptée et appliquée.
    Accepted,
    /// Refusée par l'utilisateur.
    Rejected,
    /// Délai de l'écouteur écoulé, ou agent parti, avant la réponse.
    Expired,
    /// Document fermé ou remplacé avant la réponse : rien d'appliqué.
    Stale,
    /// Acceptée mais refusée par le moteur à l'application (code stable).
    Failed(String),
}

/// Entrée de l'historique de session du panneau (H-UI-41).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiHistoryEntry {
    /// Outil demandé.
    pub tool: String,
    /// Libellé d'une proposition de l'assistant (origine « Assistant »,
    /// lot A2) ; `None` pour un agent MCP.
    pub label: Option<String>,
    /// Nom du document visé.
    pub document: String,
    /// « Maintenant » injecté de la demande (règle 5).
    pub at: Moment,
    /// Issue.
    pub outcome: AiOutcome,
    /// Entrée d'annulation du lot accepté : (numéro d'ouverture, position
    /// de l'historique après le lot) ; `None` si rien n'a été appliqué.
    pub undo: Option<(u64, u64)>,
}

/// Ajoute `entry` à `history`, en retirant les plus anciennes au-delà de
/// [`MAX_HISTORY`].
pub fn push_history(history: &mut VecDeque<AiHistoryEntry>, entry: AiHistoryEntry) {
    history.push_back(entry);
    while history.len() > MAX_HISTORY {
        drop(history.pop_front());
    }
}

/// Clé d'une explication : document (ouverture, génération), tâche, langue.
type ExplainKey = (u64, u64, TaskUid, ToolLanguage);

/// Explication en cache (H-UI-58) : vue du moteur, ou message d'erreur.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ExplainCache {
    key: ExplainKey,
    result: Result<ExplainView, String>,
}

/// État du volet ; ouvert ou fermé est recopié dans
/// `UiState.side_panel.open` à chaque image (lot A5), l'onglet et
/// l'explication ne sont jamais sérialisés.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SidePanelState {
    /// Volet ouvert.
    pub open: bool,
    /// Onglet affiché.
    pub tab: SideTab,
    /// Explication de la tâche active (onglet « Pourquoi cette date ? »).
    explain: Option<ExplainCache>,
    /// Explications calculées depuis le lancement (tests : une par clé).
    explain_builds: u64,
}

impl SidePanelState {
    /// Volet ouvert (`open`) sur l'onglet par défaut, Assistant
    /// (lancement, état mémorisé ; lot A5).
    #[must_use]
    pub fn opened(open: bool) -> Self {
        Self {
            open,
            ..Self::default()
        }
    }

    /// Applique une commande du volet.
    pub fn apply(&mut self, op: SideOp) {
        match op {
            SideOp::Toggle => {
                if self.open {
                    self.open = false;
                } else {
                    self.open = true;
                    self.tab = SideTab::Assistant;
                }
            }
            SideOp::Open(tab) => {
                self.open = true;
                self.tab = tab;
            }
            SideOp::Close => self.open = false,
            SideOp::ToggleTab(tab) => {
                if self.open && self.tab == tab {
                    self.open = false;
                } else {
                    self.open = true;
                    self.tab = tab;
                }
            }
            SideOp::SetWidth(_) => {}
        }
    }

    /// Vrai si l'onglet Assistant est visible.
    #[must_use]
    pub fn shows_assistant(&self) -> bool {
        self.open && self.tab == SideTab::Assistant
    }

    /// Vrai si l'onglet « Opérations IA » est visible (le bandeau J7 est
    /// alors remplacé par la tête du volet).
    #[must_use]
    pub fn shows_ai(&self) -> bool {
        self.open && self.tab == SideTab::AiOperations
    }

    /// Explications calculées depuis le lancement.
    #[must_use]
    pub const fn explain_builds(&self) -> u64 {
        self.explain_builds
    }

    /// Relit l'explication de la tâche `active` (H-UI-58) si l'onglet est
    /// visible et que la clé a changé ; rien sinon.
    pub fn refresh_explain(
        &mut self,
        doc: Option<&OpenDocument>,
        active: Option<TaskUid>,
        lang: ToolLanguage,
    ) {
        if !(self.open && self.tab == SideTab::WhyThisDate) {
            return;
        }
        let (Some(doc), Some(uid)) = (doc, active) else {
            self.explain = None;
            return;
        };
        let key = (doc.opening, doc.generation(), uid, lang);
        if self.explain.as_ref().is_some_and(|c| c.key == key) {
            return;
        }
        let params = ExplainParams {
            task: TaskRef::Uid(uid),
            side: None,
            depth: None,
        };
        let result = explain_date(&doc.session, &params).map_err(|e| e.message(lang));
        self.explain_builds = self.explain_builds.saturating_add(1);
        self.explain = Some(ExplainCache { key, result });
    }

    /// Explication en cache pour la tâche `active`, si elle est à jour de
    /// cette tâche.
    fn explanation(&self, active: Option<TaskUid>) -> Option<&Result<ExplainView, String>> {
        self.explain
            .as_ref()
            .filter(|c| Some(c.key.2) == active)
            .map(|c| &c.result)
    }
}

/// Données du relais lues par l'onglet « Opérations IA ».
#[derive(Clone, Copy, Debug)]
pub struct AiPanelInput<'a> {
    /// Proposition en tête de file, avec son résumé.
    pub banner: Option<ApprovalBanner<'a>>,
    /// Diff affiché de la proposition en tête de file.
    pub diff: Option<&'a DiffView>,
    /// Historique de session (le plus ancien d'abord).
    pub history: Option<&'a VecDeque<AiHistoryEntry>>,
}

impl AiPanelInput<'_> {
    /// Aucune donnée (web : pas de canal).
    pub const EMPTY: AiPanelInput<'static> = AiPanelInput {
        banner: None,
        diff: None,
        history: None,
    };
}

/// Dessine le volet (rien s'il est fermé) et pousse ses actions : à
/// droite de la vue, ou par-dessus elle sur une fenêtre étroite
/// ([`overlays`]) ; séparateur de largeur sur son bord gauche.
pub fn panel(
    ui: &mut egui::Ui,
    fc: &mut FrameCtx<'_>,
    state: &SidePanelState,
    ai: AiPanelInput<'_>,
    assistant: &mut crate::assistant::AssistantPanel,
) {
    if !state.open {
        return;
    }
    let window = ui.ctx().content_rect().width();
    let width = panel_width(f32::from(fc.state.side_panel.width), window);
    if overlays(window) {
        overlay(ui, fc, state, ai, assistant, width);
        return;
    }
    // Interface adoucie (lot U2, H-UI-172) : carte sur le fond de
    // l'application ; la gouttière qui la sépare de la vue est à gauche,
    // dans le volet (le séparateur de largeur y vit), la marge extérieure à
    // droite. Contenu aux marges d'avant (8 et 2 points).
    let outer = super::cards::outer_margin(ui.ctx());
    let frame = egui::Frame::NONE.inner_margin(egui::Margin {
        left: super::cards::px(super::cards::GUTTER),
        right: super::cards::px(outer),
        top: 0,
        bottom: super::cards::px(super::cards::GUTTER),
    });
    let rect = egui::Panel::right("plannercraft_side_panel")
        .resizable(false)
        .show_separator_line(false)
        .frame(frame)
        .exact_size(width)
        .show(ui, |panel| {
            let card = panel.max_rect();
            super::cards::paint_fill(panel.painter(), card);
            let inner = super::cards::inner(card).shrink2(egui::vec2(7.0, 1.0));
            let mut body = panel.new_child(
                egui::UiBuilder::new()
                    .max_rect(inner)
                    .layout(egui::Layout::top_down(egui::Align::Min))
                    .id_salt("plannercraft_side_panel_body"),
            );
            body.set_clip_rect(inner.intersect(panel.clip_rect()));
            contents(&mut body, fc, state, ai, assistant);
            super::cards::finish(panel.painter(), card);
            panel.advance_cursor_after_rect(card);
        })
        .response
        .rect;
    width_splitter(ui, fc, rect, window);
}

/// Volet par-dessus la vue (fenêtre étroite, H-UI-164) : zone au premier
/// plan collée au bord droit de la place restante, ombre portée ; la vue
/// garde toute sa largeur dessous.
fn overlay(
    ui: &mut egui::Ui,
    fc: &mut FrameCtx<'_>,
    state: &SidePanelState,
    ai: AiPanelInput<'_>,
    assistant: &mut crate::assistant::AssistantPanel,
    width: f32,
) {
    let room = ui.available_rect_before_wrap();
    if !room.is_finite() || room.height() <= 0.0 {
        return;
    }
    let width = width.min(room.width()).max(0.0);
    let rect = egui::Rect::from_min_max(egui::pos2(room.right() - width, room.top()), room.max);
    let window = ui.ctx().content_rect().width();
    egui::Area::new(egui::Id::unique("plannercraft_side_overlay"))
        .order(egui::Order::Middle)
        .fixed_pos(rect.min)
        .constrain(false)
        .show(ui.ctx(), |ui| {
            let frame = egui::Frame::side_top_panel(ui.style())
                .shadow(ui.style().visuals.popup_shadow)
                .stroke(ui.style().visuals.widgets.noninteractive.bg_stroke);
            let inner = frame.inner_margin;
            let content = egui::Rect::from_min_max(
                egui::pos2(
                    rect.min.x + f32::from(inner.left),
                    rect.min.y + f32::from(inner.top),
                ),
                egui::pos2(
                    rect.max.x - f32::from(inner.right),
                    rect.max.y - f32::from(inner.bottom),
                ),
            );
            ui.painter().add(frame.shadow.as_shape(rect, frame.corner_radius));
            ui.painter().rect(
                rect,
                frame.corner_radius,
                frame.fill,
                frame.stroke,
                egui::StrokeKind::Inside,
            );
            let (_, _) = ui.allocate_exact_size(rect.size(), egui::Sense::hover());
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(content)
                    .layout(egui::Layout::top_down(egui::Align::Min))
                    .id_salt("plannercraft_side_overlay_body"),
            );
            child.set_clip_rect(rect);
            contents(&mut child, fc, state, ai, assistant);
            width_splitter(ui, fc, rect, window);
        });
}

/// En-tête (onglets, « Fermer le volet ») et onglet affiché.
fn contents(
    ui: &mut egui::Ui,
    fc: &mut FrameCtx<'_>,
    state: &SidePanelState,
    ai: AiPanelInput<'_>,
    assistant: &mut crate::assistant::AssistantPanel,
) {
    let texts = fc.texts;
    // Lot A5 : rangée qui passe à la ligne sous la largeur minimale (320
    // points) au lieu de déborder du volet ; « Fermer le volet » reste à
    // droite tant qu'il tient sur la rangée des onglets.
    ui.horizontal_wrapped(|ui| {
        for (tab, id) in [
            (SideTab::AiOperations, TextId::SideTabAi),
            (SideTab::WhyThisDate, TextId::SideTabWhy),
            (SideTab::Assistant, TextId::SideTabAssistant),
        ] {
            if ui.selectable_label(state.tab == tab, texts.get(id)).clicked() && state.tab != tab {
                fc.actions.push(Action::SidePanel(SideOp::Open(tab)));
            }
        }
        let close = texts.get(TextId::SideClose);
        if super::approval::row_room(ui) < super::approval::button_width(ui, &close) {
            ui.end_row();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(close).clicked() {
                fc.actions.push(Action::SidePanel(SideOp::Close));
            }
        });
    });
    ui.separator();
    match state.tab {
        SideTab::AiOperations => ai_tab(ui, fc, ai),
        SideTab::WhyThisDate => why_tab(ui, fc, state),
        SideTab::Assistant => super::assistant::tab(ui, fc, assistant),
    }
}

/// Identifiant du séparateur de largeur du volet.
#[must_use]
pub fn splitter_id() -> egui::Id {
    egui::Id::unique("plannercraft.side.splitter")
}

/// Séparateur vertical du bord gauche du volet `rect` (H-UI-163) :
/// curseur `ResizeHorizontal`, glisser ⇒ largeur suivie au pointeur
/// (bornée par [`panel_width`]), double-clic ⇒ largeur par défaut.
fn width_splitter(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, rect: egui::Rect, window: f32) {
    if !rect.is_finite() || rect.height() <= 0.0 {
        return;
    }
    let x = rect.left();
    let grab = egui::Rect::from_x_y_ranges((x - GRAB_OUT)..=(x + GRAB_IN), rect.y_range());
    let texts = fc.texts;
    let response = ui.interact(grab, splitter_id(), egui::Sense::click_and_drag());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Splitter, true, texts.get(TextId::SideSplitterTip))
    });
    let response = response.on_hover_and_drag_cursor(egui::CursorIcon::ResizeHorizontal);
    if response.hovered() || response.dragged() {
        ui.painter().vline(
            grab.center().x,
            rect.y_range(),
            egui::Stroke::new(2.0, fc.palette.grid_major),
        );
        fc.stats.shapes = fc.stats.shapes.saturating_add(1);
    }
    // Début du glisser : largeur et abscisse de l'appui retenues, la
    // largeur suit ensuite l'écart (pas de saut de la zone de saisie ni du
    // seuil de glisser).
    let start_id = splitter_id().with("start");
    if response.drag_started()
        && let Some(p) = ui.ctx().input(|i| i.pointer.press_origin())
    {
        ui.ctx()
            .data_mut(|d| d.insert_temp(start_id, (rect.width(), p.x)));
    }
    if response.double_clicked() {
        fc.actions.push(Action::SidePanel(SideOp::SetWidth(
            crate::state::SIDE_WIDTH_DEFAULT,
        )));
    } else if response.dragged()
        && let Some(p) = response.interact_pointer_pos()
        && let Some((start_width, start_x)) = ui.ctx().data(|d| d.get_temp::<(f32, f32)>(start_id))
    {
        let width = panel_width(start_width + (start_x - p.x), window).round();
        let width = u16::try_from(crate::geom::f32_to_index(width)).unwrap_or(u16::MAX);
        if width != fc.state.side_panel.width {
            fc.actions.push(Action::SidePanel(SideOp::SetWidth(width)));
        }
    }
}

/// Facteurs de conversion du document (marges en jours ouvrés).
fn factors(fc: &FrameCtx<'_>) -> ConversionFactors {
    fc.doc.map(|d| d.project().options.factors).unwrap_or_default()
}

/// Onglet « Opérations IA » : tête de file, diff, historique.
fn ai_tab(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, ai: AiPanelInput<'_>) {
    let texts = fc.texts;
    if let Some(banner) = ai.banner {
        decision_row(ui, fc, banner);
        if let Some(diff) = ai.diff {
            diff_table(ui, fc, diff);
        } else {
            let detail = banner.path.unwrap_or(banner.tool);
            ui.label(texts.get_args(TextId::AiNoPreview, &[("detail", Arg::Str(detail.into()))]));
        }
    } else {
        ui.label(texts.get(TextId::AiNoPending));
    }
    ui.separator();
    ui.label(egui::RichText::new(texts.get(TextId::AiHistory)).strong());
    let history = ai.history.filter(|h| !h.is_empty());
    let Some(history) = history else {
        ui.label(texts.get(TextId::AiHistoryEmpty));
        return;
    };
    // Position courante de l'historique du document : seul le dernier lot
    // annulable garde son lien d'annulation.
    let top = fc.doc.map(|d| (d.opening, d.session.history_position()));
    let labels = texts.format_labels();
    egui::ScrollArea::vertical()
        .id_salt("plannercraft_ai_history")
        .max_height(220.0)
        .show(ui, |ui| {
            for (i, entry) in history.iter().enumerate().rev() {
                ui.push_id(("plannercraft_ai_history_entry", i), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(history_line(fc, labels, entry));
                        if entry.undo.is_some()
                            && entry.undo == top
                            && ui.small_button(texts.get(TextId::AiUndoEntry)).clicked()
                        {
                            fc.actions.push(Action::Undo);
                        }
                    });
                });
            }
        });
}

/// Ligne d'historique : heure, outil, document, issue.
fn history_line(fc: &FrameCtx<'_>, labels: &FormatLabels, entry: &AiHistoryEntry) -> String {
    let texts = fc.texts;
    let outcome = match &entry.outcome {
        AiOutcome::Accepted => texts.get(TextId::AiOutcomeAccepted),
        AiOutcome::Rejected => texts.get(TextId::AiOutcomeRejected),
        AiOutcome::Expired => texts.get(TextId::AiOutcomeExpired),
        AiOutcome::Stale => texts.get(TextId::AiOutcomeStale),
        AiOutcome::Failed(code) => texts.get_args(
            TextId::AiOutcomeFailed,
            &[("detail", Arg::Str(code.as_str().into()))],
        ),
    };
    let time = entry.at.to_naive().time().format("%H:%M").to_string();
    let at = format!("{} {time}", format_date(entry.at, labels));
    if let Some(label) = &entry.label {
        return texts.get_args(
            TextId::AiHistoryLineAssistant,
            &[
                ("value", Arg::Str(at.into())),
                ("name", Arg::Str(label.as_str().into())),
                ("detail", Arg::Str(outcome.into())),
            ],
        );
    }
    texts.get_args(
        TextId::AiHistoryLine,
        &[
            ("value", Arg::Str(at.into())),
            ("name", Arg::Str(entry.tool.as_str().into())),
            ("document", Arg::Str(entry.document.as_str().into())),
            ("detail", Arg::Str(outcome.into())),
        ],
    )
}

/// Table avant/après (H-UI-41) : ID, nom, début, fin, marge, critique.
/// Chaque cellule montre la valeur d'après ; si elle change, la valeur
/// d'avant suit, barrée et estompée.
fn diff_table(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, diff: &DiffView) {
    let texts = fc.texts;
    let labels = texts.format_labels();
    let factors = factors(fc);
    let muted = fc.palette.text_weak;
    egui::ScrollArea::both()
        .id_salt("plannercraft_ai_diff")
        .max_height(ui.available_height() * 0.6)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            egui::Grid::new("plannercraft_ai_diff_grid")
                .striped(true)
                .num_columns(6)
                .show(ui, |ui| {
                    for id in [
                        TextId::ColId,
                        TextId::ColName,
                        TextId::ColStart,
                        TextId::ColFinish,
                        TextId::ColTotalSlack,
                        TextId::ColCritical,
                    ] {
                        ui.label(egui::RichText::new(texts.get(id)).strong());
                    }
                    ui.end_row();
                    for row in diff.rows.iter().take(MAX_DIFF_ROWS) {
                        diff_row(ui, fc, row, labels, factors, muted);
                        ui.end_row();
                    }
                });
        });
    if diff.more > 0 {
        let more = i64::try_from(diff.more).unwrap_or(i64::MAX);
        ui.label(texts.get_args(TextId::AiMore, &[("count", Arg::Count(more))]));
    }
}

/// Une ligne de la table avant/après.
fn diff_row(
    ui: &mut egui::Ui,
    fc: &FrameCtx<'_>,
    row: &DiffRow,
    labels: &FormatLabels,
    factors: ConversionFactors,
    muted: egui::Color32,
) {
    let texts = fc.texts;
    ui.add(egui::Label::new(row.id.map(|id| id.get().to_string()).unwrap_or_default()).extend());
    let name = match row.change {
        RowChange::Added => texts.get_args(TextId::AiAdded, &[("name", Arg::Str(row.name.as_str().into()))]),
        RowChange::Removed => {
            texts.get_args(TextId::AiRemoved, &[("name", Arg::Str(row.name.as_str().into()))])
        }
        RowChange::Changed | RowChange::CriticalOnly => row.name.clone(),
    };
    ui.add(egui::Label::new(name).extend());
    let date = |m: Option<Moment>| m.map(|m| format_date(m, labels)).unwrap_or_default();
    let slack = |m: Option<Minutes>| m.map(|m| format_slack(m, &factors, labels)).unwrap_or_default();
    pair_cell(ui, &date(row.start.0), &date(row.start.1), muted);
    pair_cell(ui, &date(row.finish.0), &date(row.finish.1), muted);
    pair_cell(ui, &slack(row.total_slack.0), &slack(row.total_slack.1), muted);
    let critical = match row.critical {
        CriticalChange::Same => String::new(),
        CriticalChange::Entered => texts.get(TextId::AiCriticalEntered),
        CriticalChange::Left => texts.get(TextId::AiCriticalLeft),
    };
    ui.label(critical);
}

/// Cellule avant/après : la valeur d'après, puis celle d'avant barrée si
/// elle diffère.
fn pair_cell(ui: &mut egui::Ui, before: &str, after: &str, muted: egui::Color32) {
    // Jamais coupée sur plusieurs lignes (« mar / 23/03/ / 27 ») : la
    // table défile horizontalement si le volet est étroit (lot A2).
    ui.vertical(|ui| {
        if after.is_empty() {
            ui.add(egui::Label::new(egui::RichText::new(before).strikethrough().color(muted)).extend());
            return;
        }
        ui.add(egui::Label::new(after).extend());
        if before != after && !before.is_empty() {
            ui.add(
                egui::Label::new(egui::RichText::new(before).strikethrough().small().color(muted)).extend(),
            );
        }
    });
}

/// Onglet « Pourquoi cette date ? » (H-UI-58) : résumé du moteur
/// (gabarits R-233), puis les pilotes, cliquables (sélection de la tâche).
fn why_tab(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, state: &SidePanelState) {
    let texts = fc.texts;
    let Some(result) = state.explanation(fc.selection) else {
        ui.label(texts.get(TextId::WhyNoTask));
        return;
    };
    let view = match result {
        Ok(view) => view,
        Err(message) => {
            ui.label(texts.get_args(
                TextId::WhyUnavailable,
                &[("detail", Arg::Str(message.as_str().into()))],
            ));
            return;
        }
    };
    let id = view.task.id.map(|id| id.to_string()).unwrap_or_default();
    ui.label(egui::RichText::new(format!("{id} {}", view.task.name)).strong());
    let summary = match texts.language() {
        crate::Language::Fr => &view.summary_fr,
        crate::Language::En => &view.summary_en,
    };
    ui.label(summary.as_str());
    if view.chain.is_empty() {
        return;
    }
    ui.separator();
    ui.label(texts.get(TextId::WhyDrivers));
    for (i, link) in view.chain.iter().enumerate() {
        let id = link.id.map(|id| id.to_string()).unwrap_or_default();
        let clicked = ui
            .push_id(("plannercraft_why_driver", i), |ui| {
                ui.link(format!("{id} {}", link.name))
            })
            .inner
            .clicked();
        if clicked {
            fc.actions.push(Action::Ui(UiOp::Select(Some(link.uid))));
            fc.actions.push(Action::Ui(UiOp::ScrollTo(link.uid)));
        }
    }
}
