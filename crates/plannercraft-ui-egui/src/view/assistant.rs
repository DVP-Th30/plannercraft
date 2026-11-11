//! Onglet « Assistant » du volet latéral (ADR-0024 ; lot A1, H-UI-138 à
//! H-UI-142 ; lot A1b, H-UI-143 à H-UI-148, cahier
//! `docs/plans/assistant-chat.md`).
//!
//! De haut en bas : en-tête (icône, « Assistant IA », modèle, « Nouvelle
//! conversation », options), connexion si aucune clé, conversation dans un
//! seul `ScrollArea` (défilement ancré, [`crate::assistant::ChatScroll`]),
//! barre d'état du flux, saisie. Le volet ne calcule rien (règle 3) : il
//! affiche le texte du modèle, qui cite le moteur par ses outils.

use egui::{
    Align, Color32, CornerRadius, FontId, Id, Key, Layout, Modifiers, Rect, RichText, Sense, Stroke,
    TextStyle, Vec2, pos2, vec2,
};
use plannercraft_assistant::conversation::{Activity, ToolStep, Turn, TurnState};
use plannercraft_assistant::error::AssistantError;
use plannercraft_assistant::openrouter::MODELS;
use plannercraft_assistant::proposal::{PROPOSE_TOOL, ProposalCard, ProposalStatus};
use plannercraft_assistant::stream::Usage;

use super::FrameCtx;
use crate::action::Action;
use crate::approvals::ProposalOp;
use crate::assistant::{
    AssistantOp, AssistantPanel, ChatLayout, MAX_KEY_CHARS, MAX_QUESTION_CHARS, Notice, ScrollMode,
};
use crate::i18n::{Arg, Language, TextId, Texts};
use crate::ribbon::Icon;
use crate::ribbon::icons::paint_icon;
use crate::state::AssistantAccess;
use crate::theme::ChatTokens;
use crate::view::assistant_account;

/// Marge latérale de la conversation.
const MARGIN: f32 = 14.0;
/// Espace entre deux tours.
const TURN_GAP: f32 = 20.0;
/// Espace entre une question et sa réponse.
const QA_GAP: f32 = 10.0;
/// Texte des messages.
const BODY_SIZE: f32 = 14.0;
/// Méta (pied, lignes d'outils, barre d'état).
pub(crate) const META_SIZE: f32 = 12.0;
/// Texte du raisonnement déplié.
const REASONING_SIZE: f32 = 12.5;
/// Hauteur de l'en-tête.
const HEADER_H: f32 = 36.0;
/// Hauteur de la barre d'état du flux.
const STATUS_H: f32 = 24.0;
/// Côté du bouton Envoyer / Arrêter.
const SEND_SIZE: f32 = 28.0;
/// Hauteur d'une ligne d'outil.
const TOOL_ROW_H: f32 = 22.0;
/// Marge au-dessus de la question ancrée.
pub const ANCHOR_MARGIN: f32 = 8.0;
/// Écart entre la pastille et la saisie.
const PILL_GAP: f32 = 12.0;
/// Distance sous la vue au-delà de laquelle la pastille apparaît.
pub const PILL_THRESHOLD: f32 = 48.0;
/// Délai d'apparition de la pastille (secondes).
pub const PILL_DELAY: f64 = 0.3;
/// Délai avant « En attente… » (secondes).
const WAIT_DELAY: f64 = 0.8;
/// Lectures consécutives regroupées à partir de ce nombre.
const GROUP_FROM: usize = 3;
/// Hauteur au plus du raisonnement déplié (défilement interne au-delà).
const REASONING_MAX_H: f32 = 200.0;
/// Lignes au plus de la saisie avant défilement.
const INPUT_MAX_ROWS: f32 = 8.0;
/// Durée de l'ancrage animé (secondes ; nulle si animations réduites).
const ANCHOR_ANIM: f64 = 0.18;
/// Durée du retour « Copié » (secondes).
const COPIED_FOR: f64 = 1.5;
/// Largeur au plus d'un message de l'utilisateur (part du volet).
const USER_MAX_SHARE: f32 = 0.85;

/// Identifiant du champ de la question.
#[must_use]
pub fn question_id() -> Id {
    Id::unique("plannercraft.assistant.question")
}

/// Identifiant du bouton « Copier » du tour `turn`.
#[must_use]
pub fn copy_id(turn: usize) -> Id {
    Id::unique(("plannercraft.assistant.copy", turn))
}

/// Identifiant du bouton « Régénérer ».
#[must_use]
pub fn regenerate_id() -> Id {
    Id::unique("plannercraft.assistant.regenerate")
}

pub(crate) fn push(fc: &mut FrameCtx<'_>, op: AssistantOp) {
    fc.actions.push(Action::Assistant(op));
}

/// Gestes du fil, appliqués après le dessin (le dessin n'emprunte la
/// conversation qu'en lecture).
enum ChatEvent {
    Copy(usize, String),
    Op(AssistantOp),
    Suggest(String),
    Jump,
    /// Geste sur la carte d'une proposition (lot A2).
    Proposal(ProposalOp),
    /// Refuser, avec le motif saisi sur la carte.
    RejectWith(u64, String),
    /// « Voir les ressources » sur la carte d'une proposition acceptée
    /// (lot R1, H-UI-188).
    ShowResources,
}

/// Dessine l'onglet.
pub fn tab(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, panel: &mut AssistantPanel) {
    let t = ChatTokens::for_theme(ui.ctx().theme());
    if !fc.state.assistant.enabled {
        // Pas de saisie avant l'activation : la demande de focus tombe.
        panel.focus_request = false;
        intro(ui, fc, &t);
        return;
    }
    header(ui, fc, panel, &t);
    let account = fc.state.assistant.access == AssistantAccess::Account;
    if account {
        if panel.account().signed_in() {
            assistant_account::balance_bar(ui, fc, panel, &t);
        } else {
            assistant_account::login(ui, fc, panel, &t);
        }
    } else if !panel.connected() {
        connection(ui, fc, panel);
    }
    if let Some(notice) = &panel.notice {
        let (text, color) = match notice {
            Notice::KeyringMissing => (fc.texts.get(TextId::AssistantKeyringMissing), t.meta),
            Notice::Error(error) => (error_text(fc.texts, error), t.error_text),
        };
        ui.label(RichText::new(text).size(META_SIZE).color(color));
    }
    if account {
        assistant_account::info(ui, fc, panel, &t);
        if panel.account().usage_open && panel.account().signed_in() {
            assistant_account::usage_view(ui, fc, panel, &t);
            return;
        }
    }
    let mut events = Vec::new();
    let height = egui::Panel::bottom("plannercraft_assistant_bottom")
        .frame(egui::Frame::NONE)
        .show_separator_line(false)
        .show(ui, |ui| {
            let top = ui.min_rect().top();
            status_bar(ui, fc, panel, &t);
            input(ui, fc, panel, &t);
            ui.min_rect().bottom() - top
        })
        .inner;
    // La saisie grandit (lignes, barre d'état) : le panneau du bas prend sa
    // nouvelle hauteur dans la même image, sans débordement d'une image.
    let key = Id::unique("plannercraft.assistant.bottom_height");
    let previous = ui.data(|d| d.get_temp::<f32>(key));
    ui.data_mut(|d| d.insert_temp(key, height));
    if previous.is_some_and(|p| (p - height).abs() > 0.5) {
        ui.ctx().request_discard("hauteur de la saisie de l'assistant");
    }
    conversation(ui, fc, panel, &t, &mut events);
    apply_events(ui, fc, panel, events);
}

fn apply_events(ui: &egui::Ui, fc: &mut FrameCtx<'_>, panel: &mut AssistantPanel, events: Vec<ChatEvent>) {
    let now = ui.input(|i| i.time);
    for event in events {
        match event {
            ChatEvent::Copy(turn, text) => {
                ui.ctx().copy_text(text);
                panel.copied = Some((turn, now));
            }
            ChatEvent::Op(op) => push(fc, op),
            ChatEvent::Suggest(text) => {
                panel.draft = text;
                ui.memory_mut(|m| m.request_focus(question_id()));
            }
            ChatEvent::Jump => panel.scroll.mode = ScrollMode::Following,
            ChatEvent::Proposal(op) => fc.actions.push(Action::Proposal(op)),
            ChatEvent::RejectWith(id, reason) => {
                panel.reject_reason = Some((id, reason));
                fc.actions.push(Action::Proposal(ProposalOp::Reject(id)));
            }
            ChatEvent::ShowResources => fc
                .actions
                .push(Action::SetView(crate::state::ViewKind::ResourceSheet)),
        }
    }
}

/// Avant l'activation : mention de transparence (AI Act art. 50) et
/// bouton « Activer l'assistant » (H-UI-138).
fn intro(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, t: &ChatTokens) {
    let texts = fc.texts;
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
        paint_icon(ui.painter(), rect, Icon::Sparkles, t.accent);
        ui.label(
            RichText::new(texts.get(TextId::AssistantIntroTitle))
                .size(BODY_SIZE)
                .strong(),
        );
    });
    ui.add_space(6.0);
    ui.label(texts.get(TextId::AssistantDisclosure));
    ui.add_space(4.0);
    ui.label(RichText::new(texts.get(TextId::AssistantReadOnly)).color(t.meta));
    // Lot A4 (H-UI-155) : l'activation passe par le choix de l'accès.
    assistant_account::access_choice(ui, fc, t);
}

/// Bouton carré à icône, focusable, libellé accessible et infobulle.
pub(crate) fn icon_button(
    ui: &mut egui::Ui,
    id: Id,
    icon: Icon,
    label: &str,
    t: &ChatTokens,
    enabled: bool,
) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let response = ui.interact(rect, id, sense);
    let visuals = ui.visuals();
    if enabled && (response.hovered() || response.has_focus()) {
        ui.painter()
            .rect_filled(rect, 6.0, visuals.widgets.hovered.weak_bg_fill);
    }
    if response.has_focus() {
        ui.painter()
            .rect_stroke(rect, 6.0, Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
    }
    let color = if enabled { t.text } else { t.send_disabled_fg };
    paint_icon(ui.painter(), rect.shrink(5.0), icon, color);
    let label = label.to_owned();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label.clone()));
    response.on_hover_text(label)
}

/// En-tête (H-UI-143) : icône IA et « Assistant IA », modèle, « Nouvelle
/// conversation », options (connexion, désactivation).
fn header(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, panel: &AssistantPanel, t: &ChatTokens) {
    let texts = fc.texts;
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, HEADER_H), Sense::hover());
    let mut row = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    let (icon, _) = row.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
    paint_icon(row.painter(), icon, Icon::Sparkles, t.accent);
    row.add_space(4.0);
    row.label(
        RichText::new(texts.get(TextId::AssistantName))
            .size(BODY_SIZE)
            .strong()
            .color(t.text),
    );
    row.with_layout(Layout::right_to_left(Align::Center), |ui| {
        options_menu(ui, fc, panel, t);
        let label = texts.get(TextId::AssistantClear);
        let enabled = !panel.conversation.is_empty();
        if icon_button(
            ui,
            Id::unique("plannercraft.assistant.new"),
            Icon::SquarePen,
            &label,
            t,
            enabled,
        )
        .clicked()
        {
            push(fc, AssistantOp::Clear);
        }
        model_combo(ui, fc, panel);
    });
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
    );
    ui.add_space(4.0);
}

/// Menu « … » : état de la connexion, Se déconnecter, Désactiver.
fn options_menu(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, panel: &AssistantPanel, t: &ChatTokens) {
    let texts = fc.texts;
    let label = texts.get(TextId::AssistantMenu);
    let response = icon_button(
        ui,
        Id::unique("plannercraft.assistant.menu"),
        Icon::Ellipsis,
        &label,
        t,
        true,
    );
    egui::Popup::menu(&response).show(|ui| {
        ui.set_min_width(220.0);
        let account = fc.state.assistant.access == AssistantAccess::Account;
        if account {
            assistant_account::menu_items(ui, fc, panel, t);
        }
        if !account && panel.connected() {
            let status = if panel.key_persistent() {
                TextId::AssistantConnected
            } else {
                TextId::AssistantConnectedSession
            };
            ui.label(RichText::new(texts.get(status)).size(META_SIZE).color(t.meta));
            if ui.button(texts.get(TextId::AssistantLogout)).clicked() {
                push(fc, AssistantOp::Logout);
            }
            ui.separator();
        }
        assistant_account::access_items(ui, fc, t);
        let mut show = fc.state.assistant.show_reasoning;
        if ui
            .checkbox(&mut show, texts.get(TextId::AssistantReasoningToggle))
            .on_hover_text(texts.get(TextId::AssistantReasoningToggleHint))
            .clicked()
        {
            push(fc, AssistantOp::ToggleReasoning);
        }
        ui.separator();
        if ui.button(texts.get(TextId::AssistantDisable)).clicked() {
            push(fc, AssistantOp::Disable);
        }
    });
}

/// Nom court d'un modèle (pied, consommation) : celui de la liste fermée,
/// sinon l'identifiant.
#[must_use]
pub fn model_short(id: &str) -> String {
    MODELS
        .iter()
        .find(|m| m.id == id)
        .map_or_else(|| id.to_owned(), |m| m.short.to_owned())
}

/// Liste fermée des modèles (H-UI-140), nom court ; en mode compte, celle
/// du relais (`/v1/me`), prix en infobulle (H-UI-157).
fn model_combo(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, panel: &AssistantPanel) {
    let texts = fc.texts;
    let current = panel.model_for(&fc.state.assistant);
    let label = texts.get(TextId::AssistantModel);
    let relay_models = (fc.state.assistant.access == AssistantAccess::Account)
        .then(|| panel.account().me.as_ref().map(|m| m.models.clone()))
        .flatten()
        .filter(|models| !models.is_empty());
    let mut chosen: Option<String> = None;
    let response = egui::ComboBox::from_id_salt("plannercraft.assistant.model")
        .selected_text(model_short(&current))
        .show_ui(ui, |ui| match &relay_models {
            Some(models) => {
                for model in models {
                    let price = texts.get_args(
                        TextId::AssistantModelPrice,
                        &[
                            ("input", Arg::Str(euros(texts, model.input_cents_per_mtok).into())),
                            (
                                "output",
                                Arg::Str(euros(texts, model.output_cents_per_mtok).into()),
                            ),
                        ],
                    );
                    if ui
                        .selectable_label(model.id == current, &model.label)
                        .on_hover_text(price)
                        .clicked()
                    {
                        chosen = Some(model.id.clone());
                    }
                }
            }
            None => {
                for model in MODELS {
                    if ui.selectable_label(model.id == current, model.label).clicked() {
                        chosen = Some(model.id.to_owned());
                    }
                }
            }
        })
        .response;
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, label.clone()));
    if let Some(id) = chosen.filter(|id| *id != current) {
        push(fc, AssistantOp::SetModel(id));
    }
}

/// Connexion (H-UI-139) : se connecter, saisir une clé.
fn connection(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, panel: &mut AssistantPanel) {
    let texts = fc.texts;
    if panel.key_loading() {
        ui.label(texts.get(TextId::AssistantKeyLoading));
        return;
    }
    if panel.logging_in() {
        ui.horizontal_wrapped(|ui| {
            ui.label(texts.get(TextId::AssistantLoginWaiting));
            if ui.small_button(texts.get(TextId::AssistantCancel)).clicked() {
                push(fc, AssistantOp::CancelLogin);
            }
        });
        return;
    }
    if panel.key_entry {
        let label = texts.get(TextId::AssistantKeyHint);
        ui.label(&label);
        let response = ui.add(
            egui::TextEdit::singleline(&mut panel.key_draft)
                .id(Id::unique("plannercraft.assistant.key"))
                .password(true)
                .char_limit(MAX_KEY_CHARS)
                .hint_text("sk-or-v1-…")
                .desired_width(f32::INFINITY),
        );
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, label.clone()));
        let entered = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        ui.horizontal(|ui| {
            let can_save = !panel.key_draft.trim().is_empty();
            if ui
                .add_enabled(can_save, egui::Button::new(texts.get(TextId::AssistantKeySave)))
                .clicked()
                || (entered && can_save)
            {
                push(fc, AssistantOp::SaveKey);
            }
            if ui.button(texts.get(TextId::AssistantCancel)).clicked() {
                push(fc, AssistantOp::KeyEntry(false));
            }
        });
        return;
    }
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                panel.can_login(),
                egui::Button::new(texts.get(TextId::AssistantLogin)),
            )
            .clicked()
        {
            push(fc, AssistantOp::Login);
        }
        if ui.button(texts.get(TextId::AssistantEnterKey)).clicked() {
            push(fc, AssistantOp::KeyEntry(true));
        }
    });
}

/// Fil de la conversation, ou état vide.
fn conversation(
    ui: &mut egui::Ui,
    fc: &FrameCtx<'_>,
    panel: &mut AssistantPanel,
    t: &ChatTokens,
    events: &mut Vec<ChatEvent>,
) {
    if panel.conversation.is_empty() {
        // Défile si l'écran de connexion au compte (lot A4) laisse peu de
        // hauteur : l'état vide ne recouvre jamais la saisie.
        egui::ScrollArea::vertical()
            .id_salt("plannercraft_assistant_empty")
            .auto_shrink([false, false])
            .show(ui, |ui| empty_state(ui, fc, t, events));
        return;
    }
    let now = ui.input(|i| i.time);
    let reduce = fc.state.reduce_motion;
    let mut scroll = std::mem::take(&mut panel.scroll);
    // 1. Geste de l'utilisateur sur la zone (image précédente) : la vue lui
    //    revient.
    if let Some(dy) = gesture(ui, &scroll.layout) {
        scroll.mode = match scroll.mode {
            ScrollMode::Following if dy < 0.0 => ScrollMode::Following,
            _ => ScrollMode::Free,
        };
    }
    // 2. Décalage imposé : question ancrée en haut, animée une fois.
    // Jamais au-delà du bas connu : pas de rebond élastique d'egui.
    let known_bottom = (scroll.layout.content_height - scroll.layout.view_height).max(0.0);
    let imposed = anchored_offset(&mut scroll, now, reduce, ui.ctx()).map(|offset| offset.min(known_bottom));
    let margin = bottom_margin(&scroll.layout);
    let mut area = egui::ScrollArea::vertical()
        .id_salt("plannercraft_assistant_transcript")
        .auto_shrink([false, false]);
    if let Some(offset) = imposed {
        area = area.vertical_scroll_offset(offset);
    }
    if scroll.mode == ScrollMode::Following {
        // Suit le bas : amené au bas connu, puis collé par egui.
        area = area.vertical_scroll_offset(known_bottom).stick_to_bottom(true);
    }
    let busy = panel.conversation.busy();
    let copied = panel
        .copied
        .filter(|(_, at)| now - at < COPIED_FOR)
        .map(|(turn, _)| turn);
    let output = area.show(ui, |ui| {
        let measures = transcript(ui, fc, panel, t, copied, events);
        ui.add_space(margin);
        measures
    });
    let (tops, last_height, content_end) = output.inner;
    let view_height = output.inner_rect.height();
    let actual = output.state.clamped_offset().y;
    // 3. Décalage différent de celui imposé (barre tirée, clavier) : libre.
    if let Some(offset) = imposed {
        let max = (output.content_size.y - view_height).max(0.0);
        if (actual - offset.min(max)).abs() > 1.0 {
            scroll.mode = ScrollMode::Free;
        }
    }
    scroll.layout = ChatLayout {
        turn_tops: tops,
        last_turn_height: last_height,
        content_end,
        view_height,
        view_rect: Some(output.inner_rect),
        offset: actual,
        content_height: output.content_size.y,
    };
    pill(ui, fc, &mut scroll, t, busy, now, events);
    if panel.copied.is_some() && copied.is_none() {
        panel.copied = None;
    } else if copied.is_some() {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs_f64(COPIED_FOR));
    }
    panel.scroll = scroll;
}

/// Geste de défilement de l'utilisateur sur la zone de la conversation :
/// molette ou pavé (déplacement vertical, positif vers le haut du fil),
/// Page précédente / suivante.
fn gesture(ui: &egui::Ui, layout: &ChatLayout) -> Option<f32> {
    let rect = layout.view_rect?;
    ui.input(|i| {
        let over = i.pointer.hover_pos().is_some_and(|p| rect.contains(p));
        let wheel: f32 = i
            .events
            .iter()
            .filter_map(|e| match e {
                egui::Event::MouseWheel { delta, .. } => Some(delta.y),
                _ => None,
            })
            .sum();
        if over && (wheel != 0.0 || i.smooth_scroll_delta.y != 0.0) {
            return Some(if wheel == 0.0 {
                i.smooth_scroll_delta.y
            } else {
                wheel
            });
        }
        if over && i.key_pressed(Key::PageUp) {
            return Some(1.0);
        }
        if over && i.key_pressed(Key::PageDown) {
            return Some(-1.0);
        }
        None
    })
}

/// Décalage imposé en mode ancré : haut de la question moins 8 points,
/// atteint en [`ANCHOR_ANIM`] secondes (aussitôt si animations réduites).
fn anchored_offset(
    scroll: &mut crate::assistant::ChatScroll,
    now: f64,
    reduce: bool,
    ctx: &egui::Context,
) -> Option<f32> {
    let ScrollMode::Anchored(turn) = scroll.mode else {
        return None;
    };
    let top = scroll.layout.turn_tops.get(turn).copied()?;
    let target = (top - ANCHOR_MARGIN).max(0.0);
    let (from, start) = *scroll.anim.get_or_insert((scroll.layout.offset, now));
    let progress = if reduce || ANCHOR_ANIM <= 0.0 {
        1.0
    } else {
        ((now - start) / ANCHOR_ANIM).clamp(0.0, 1.0)
    };
    if progress >= 1.0 {
        return Some(target);
    }
    ctx.request_repaint();
    // Décélération (ease-out cubique).
    #[allow(clippy::cast_possible_truncation, reason = "progression bornée à [0, 1]")]
    let eased = (1.0 - (1.0 - progress).powi(3)) as f32;
    Some(from + (target - from) * eased)
}

/// Marge basse après le dernier message (cahier § 2.2) : de quoi tenir la
/// dernière question en haut de la vue, gardée jusqu'au prochain envoi.
fn bottom_margin(layout: &ChatLayout) -> f32 {
    // Un point de jeu : les arrondis ne laissent jamais la cible hors
    // d'atteinte.
    (layout.view_height - layout.last_turn_height - ANCHOR_MARGIN + 1.0).max(0.0)
}

/// Pastille « ↓ Suite de la réponse » / « ↓ Dernier message » (cahier
/// § 2.5) : centrée, 12 points au-dessus de la saisie, si la fin du
/// contenu est à plus de 48 points sous la vue depuis 300 ms.
fn pill(
    ui: &egui::Ui,
    fc: &FrameCtx<'_>,
    scroll: &mut crate::assistant::ChatScroll,
    t: &ChatTokens,
    busy: bool,
    now: f64,
    events: &mut Vec<ChatEvent>,
) {
    let layout = &scroll.layout;
    let below = layout.content_end - (layout.offset + layout.view_height) > PILL_THRESHOLD;
    if !below {
        scroll.below_since = None;
        return;
    }
    let since = *scroll.below_since.get_or_insert(now);
    let waited = now - since;
    if waited < PILL_DELAY {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs_f64(PILL_DELAY - waited));
        return;
    }
    let Some(view) = layout.view_rect else {
        return;
    };
    let label = fc.texts.get(if busy {
        TextId::AssistantJumpStream
    } else {
        TextId::AssistantJumpLast
    });
    let font = FontId::proportional(META_SIZE);
    let galley = ui.painter().layout_no_wrap(label.clone(), font, t.text);
    let size = vec2(galley.size().x + 40.0, galley.size().y + 12.0);
    let rect = Rect::from_center_size(
        pos2(view.center().x, view.bottom() - PILL_GAP - size.y / 2.0),
        size,
    );
    let response = ui.interact(rect, Id::unique("plannercraft.assistant.pill"), Sense::click());
    let painter = ui.painter();
    painter.add(egui::epaint::RectShape::new(
        rect,
        CornerRadius::same(255),
        t.surface,
        Stroke::new(1.0, t.border),
        egui::StrokeKind::Inside,
    ));
    if response.hovered() || response.has_focus() {
        painter.rect_stroke(
            rect,
            CornerRadius::same(255),
            Stroke::new(1.5, t.accent),
            egui::StrokeKind::Inside,
        );
    }
    let arrow = Rect::from_min_size(pos2(rect.left() + 10.0, rect.center().y - 6.0), Vec2::splat(12.0));
    paint_icon(painter, arrow, Icon::ArrowDown, t.text);
    painter.galley(
        pos2(rect.left() + 26.0, rect.center().y - galley.size().y / 2.0),
        galley,
        t.text,
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label.clone()));
    if response.clicked() {
        events.push(ChatEvent::Jump);
    }
}

/// Mesures du fil : haut de chaque question, hauteur du dernier tour, fin
/// du contenu (coordonnées du contenu).
type Measures = (Vec<f32>, f32, f32);

/// Tours de la conversation (H-UI-143).
fn transcript(
    ui: &mut egui::Ui,
    fc: &FrameCtx<'_>,
    panel: &AssistantPanel,
    t: &ChatTokens,
    copied: Option<usize>,
    events: &mut Vec<ChatEvent>,
) -> Measures {
    let origin = ui.min_rect().top();
    let width = ui.available_width();
    let mut tops = Vec::with_capacity(panel.conversation.turns().len());
    let mut last_height = 0.0;
    let turns = panel.conversation.turns();
    let last = turns.len().saturating_sub(1);
    let pending = panel.conversation.pending_tools();
    ui.style_mut()
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(BODY_SIZE));
    ui.style_mut().visuals.widgets.noninteractive.fg_stroke.color = t.text;
    ui.style_mut().visuals.widgets.active.fg_stroke.color = t.strong;
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(14, 0))
        .show(ui, |ui| {
            ui.set_width((width - 2.0 * MARGIN).max(80.0));
            ui.add_space(8.0);
            for (i, turn) in turns.iter().enumerate() {
                if i > 0 {
                    ui.add_space(TURN_GAP);
                }
                let top = ui.cursor().top() - origin;
                tops.push(top);
                let ctx = TurnCtx {
                    index: i,
                    last: i == last,
                    pending: if i == last { pending.as_slice() } else { &[] },
                    copied: copied == Some(i),
                    applied: &panel.applied,
                };
                ui.push_id(("plannercraft_assistant_turn", i), |ui| {
                    turn_ui(ui, fc, t, turn, &ctx, events);
                });
                if i == last {
                    last_height = ui.cursor().top() - origin - top;
                }
            }
        });
    let end = ui.cursor().top() - origin;
    (tops, last_height, end)
}

/// Place d'un tour dans le fil.
struct TurnCtx<'a> {
    index: usize,
    last: bool,
    pending: &'a [String],
    copied: bool,
    /// Résultat des propositions acceptées (H-UI-188).
    applied: &'a std::collections::BTreeMap<u64, crate::applied::Applied>,
}

fn turn_ui(
    ui: &mut egui::Ui,
    fc: &FrameCtx<'_>,
    t: &ChatTokens,
    turn: &Turn,
    ctx: &TurnCtx<'_>,
    events: &mut Vec<ChatEvent>,
) {
    let top = ui.cursor().top();
    user_message(ui, fc, t, turn);
    ui.add_space(QA_GAP);
    if !turn.reasoning.is_empty() {
        reasoning(ui, fc, t, turn, ctx.index);
        ui.add_space(4.0);
    }
    tools(ui, fc, t, &turn.tools, ctx.pending, ctx.index);
    if !turn.answer.is_empty() {
        answer(ui, fc, t, turn);
    }
    for card in turn.tools.iter().filter_map(|step| step.proposal.as_ref()) {
        ui.add_space(6.0);
        proposal_card(ui, fc, t, card, ctx.applied.get(&card.id), events);
    }
    match &turn.state {
        TurnState::Failed(error) => error_box(ui, fc, t, error, ctx.last, events),
        TurnState::Stopped => stopped(ui, fc, t, ctx.last, events),
        TurnState::Streaming | TurnState::Done => {}
    }
    if !turn.streaming() {
        footer(ui, fc, t, turn, ctx, top, events);
    }
}

/// Message de l'utilisateur : bloc teinté aligné à droite, 85 % au plus,
/// coins 12 points (4 en bas à droite), heure en infobulle.
fn user_message(ui: &mut egui::Ui, fc: &FrameCtx<'_>, t: &ChatTokens, turn: &Turn) {
    let width = ui.available_width();
    let wrap = (width * USER_MAX_SHARE - 24.0).max(40.0);
    let galley = ui.painter().layout(
        turn.question.clone(),
        FontId::proportional(BODY_SIZE),
        t.user_text,
        wrap,
    );
    let size = vec2(galley.size().x + 24.0, galley.size().y + 16.0);
    let (row, response) = ui.allocate_exact_size(vec2(width, size.y), Sense::hover());
    let bubble = Rect::from_min_size(pos2(row.right() - size.x, row.top()), size);
    ui.painter().rect_filled(
        bubble,
        CornerRadius {
            nw: 12,
            ne: 12,
            sw: 12,
            se: 4,
        },
        t.user_bg,
    );
    ui.painter()
        .galley(pos2(bubble.min.x + 12.0, bubble.min.y + 8.0), galley, t.user_text);
    let question = turn.question.clone();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, question.clone()));
    if let Some(at) = &turn.asked_at
        && ui.rect_contains_pointer(bubble)
    {
        let tip = fc.texts.get_args(
            TextId::AssistantAskedAt,
            &[("time", Arg::Str(at.as_str().into()))],
        );
        response.on_hover_text(tip);
    }
}

/// Bloc réflexion (H-UI-143) : replié par défaut ; « Réflexion… » pulsé
/// pendant le flux, « Réflexion · 6 s » ensuite ; déplié, texte atténué
/// avec filet gauche, défilement interne au-delà de 200 points.
fn reasoning(ui: &mut egui::Ui, fc: &FrameCtx<'_>, t: &ChatTokens, turn: &Turn, index: usize) {
    let texts = fc.texts;
    let id = Id::unique(("plannercraft.assistant.reasoning", index));
    let open = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    let live = turn.streaming() && turn.answer.is_empty() && turn.tools.is_empty();
    let label = if live {
        texts.get(TextId::AssistantReasoningLive)
    } else if let Some(secs) = turn.reasoning_secs.filter(|s| *s > 0) {
        texts.get_args(
            TextId::AssistantReasoningDone,
            &[("count", Arg::Count(i64::from(secs)))],
        )
    } else {
        texts.get(TextId::AssistantReasoningShort)
    };
    let mut color = t.meta;
    if live && !fc.state.reduce_motion {
        let phase = ui.input(|i| i.time).rem_euclid(1.4) / 1.4;
        #[allow(clippy::cast_possible_truncation, reason = "valeur bornée à [0,45 ; 1]")]
        let alpha = (0.725 + 0.275 * (phase * std::f64::consts::TAU).cos()) as f32;
        color = color.gamma_multiply(alpha);
        ui.ctx().request_repaint_after(crate::assistant::REPAINT_AFTER);
    }
    if disclosure_row(ui, id, open, &label, color, t).clicked() {
        ui.data_mut(|d| d.insert_temp(id, !open));
    }
    if open {
        egui::ScrollArea::vertical()
            .id_salt(("plannercraft.assistant.reasoning.scroll", index))
            .max_height(REASONING_MAX_H)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let response = egui::Frame::NONE
                    .inner_margin(egui::Margin {
                        left: 10,
                        right: 4,
                        top: 2,
                        bottom: 2,
                    })
                    .show(ui, |ui| {
                        ui.label(RichText::new(&turn.reasoning).size(REASONING_SIZE).color(t.meta));
                    })
                    .response;
                let rect = response.rect;
                ui.painter()
                    .vline(rect.left() + 1.0, rect.y_range(), Stroke::new(2.0, t.rule));
            });
    }
}

/// Ligne repliable (chevron toujours visible), focusable.
fn disclosure_row(
    ui: &mut egui::Ui,
    id: Id,
    open: bool,
    label: &str,
    color: Color32,
    t: &ChatTokens,
) -> egui::Response {
    let font = FontId::proportional(META_SIZE);
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, color);
    let width = 18.0 + galley.size().x + 4.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, TOOL_ROW_H), Sense::hover());
    let response = ui.interact(rect, id.with("row"), Sense::click());
    let chevron = Rect::from_min_size(pos2(rect.left(), rect.center().y - 7.0), Vec2::splat(14.0));
    let icon = if open {
        Icon::ChevronDown
    } else {
        Icon::ChevronRight
    };
    let ink = if response.hovered() { t.text } else { color };
    paint_icon(ui.painter(), chevron, icon, ink);
    ui.painter().galley(
        pos2(rect.left() + 18.0, rect.center().y - galley.size().y / 2.0),
        galley,
        ink,
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.expand(1.0),
            4.0,
            Stroke::new(1.5, t.accent),
            egui::StrokeKind::Outside,
        );
    }
    let label = label.to_owned();
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, open, label.clone()));
    response
}

/// Libellé métier de la lecture `name` (« le chemin critique »), jamais
/// l'identifiant technique.
#[must_use]
pub fn tool_subject(texts: &Texts, name: &str) -> String {
    texts.get(match name {
        "project_summary" => TextId::AssistantToolProjectSummary,
        "list_tasks" => TextId::AssistantToolListTasks,
        "get_task" => TextId::AssistantToolGetTask,
        "explain_date" => TextId::AssistantToolExplainDate,
        "critical_path" => TextId::AssistantToolCriticalPath,
        "list_resources" => TextId::AssistantToolListResources,
        "baseline_variance" => TextId::AssistantToolBaselineVariance,
        "ui_state" => TextId::AssistantToolUiState,
        _ => TextId::AssistantToolOther,
    })
}

/// Libellé d'une proposition : celui de sa carte, sinon celui des
/// arguments (appel refusé), sinon rien.
fn proposal_label(step: &ToolStep) -> Option<String> {
    if let Some(card) = &step.proposal {
        return Some(card.label.clone());
    }
    serde_json::from_str::<serde_json::Value>(&step.arguments)
        .ok()?
        .get("label")?
        .as_str()
        .map(str::to_owned)
}

/// Texte de la barre d'état ou d'une ligne en cours pour l'outil `name`.
fn reading_text(texts: &Texts, name: &str) -> String {
    if name == PROPOSE_TOOL {
        return texts.get(TextId::AssistantToolProposing);
    }
    texts.get_args(
        TextId::AssistantToolReading,
        &[("what", Arg::Str(tool_subject(texts, name).into()))],
    )
}

/// Lignes d'outils (H-UI-143) : une ligne de 22 points par lecture, au
/// passé, état (en cours, ✓, ⚠ avec infobulle) ; à partir de 3 lectures,
/// une ligne « ▸ 4 lectures du planning » dépliable.
fn tools(
    ui: &mut egui::Ui,
    fc: &FrameCtx<'_>,
    t: &ChatTokens,
    done: &[ToolStep],
    pending: &[String],
    index: usize,
) {
    let count = done.len().saturating_add(pending.len());
    if count == 0 {
        return;
    }
    if count >= GROUP_FROM {
        let id = Id::unique(("plannercraft.assistant.tools", index));
        let open = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        let label = fc.texts.get_args(
            TextId::AssistantToolGroup,
            &[("count", Arg::Count(i64::try_from(count).unwrap_or(i64::MAX)))],
        );
        if disclosure_row(ui, id, open, &label, t.meta, t).clicked() {
            ui.data_mut(|d| d.insert_temp(id, !open));
        }
        if !open {
            return;
        }
        ui.indent(id, |ui| tool_rows(ui, fc, t, done, pending));
    } else {
        tool_rows(ui, fc, t, done, pending);
    }
    ui.add_space(4.0);
}

fn tool_rows(ui: &mut egui::Ui, fc: &FrameCtx<'_>, t: &ChatTokens, done: &[ToolStep], pending: &[String]) {
    let texts = fc.texts;
    for step in done {
        if step.name == PROPOSE_TOOL {
            match (&step.error, proposal_label(step)) {
                (None, label) => tool_row(
                    ui,
                    Icon::Sparkles,
                    t.accent,
                    &texts.get_args(
                        TextId::AssistantToolPropose,
                        &[("what", Arg::Str(label.unwrap_or_default().into()))],
                    ),
                    t,
                    None,
                ),
                (Some(message), _) => tool_row(
                    ui,
                    Icon::TriangleAlert,
                    t.warn,
                    &texts.get(TextId::AssistantToolProposeFailed),
                    t,
                    Some(message),
                ),
            }
            continue;
        }
        let what = Arg::Str(tool_subject(texts, &step.name).into());
        match &step.error {
            None => tool_row(
                ui,
                Icon::Check,
                t.ok,
                &texts.get_args(TextId::AssistantToolRead, &[("what", what)]),
                t,
                None,
            ),
            Some(message) => tool_row(
                ui,
                Icon::TriangleAlert,
                t.warn,
                &texts.get_args(TextId::AssistantToolFailed, &[("what", what)]),
                t,
                Some(message),
            ),
        }
    }
    for name in pending {
        tool_row(ui, Icon::Ellipsis, t.meta, &reading_text(texts, name), t, None);
    }
}

fn tool_row(ui: &mut egui::Ui, icon: Icon, color: Color32, text: &str, t: &ChatTokens, tip: Option<&str>) {
    let font = FontId::proportional(META_SIZE);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, t.meta);
    let (rect, response) = ui.allocate_exact_size(vec2(18.0 + galley.size().x, TOOL_ROW_H), Sense::hover());
    let mark = Rect::from_min_size(pos2(rect.left() + 1.0, rect.center().y - 6.0), Vec2::splat(12.0));
    paint_icon(ui.painter(), mark, icon, color);
    ui.painter().galley(
        pos2(rect.left() + 18.0, rect.center().y - galley.size().y / 2.0),
        galley,
        t.meta,
    );
    let text = text.to_owned();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, text.clone()));
    if let Some(tip) = tip {
        response.on_hover_text(tip);
    }
}

/// Réponse en Markdown, pleine largeur ; texte accessible publié à la fin
/// seulement (« Réponse en cours » pendant le flux).
fn answer(ui: &mut egui::Ui, fc: &FrameCtx<'_>, t: &ChatTokens, turn: &Turn) {
    let response = ui
        .scope(|ui| {
            ui.visuals_mut().widgets.noninteractive.fg_stroke.color = t.text;
            ui.visuals_mut().widgets.active.fg_stroke.color = t.strong;
            super::markdown::render_markdown(ui, &turn.answer);
        })
        .response;
    let label = if turn.streaming() {
        fc.texts.get(TextId::AssistantAnswerInProgress)
    } else {
        format!(
            "{}. {}",
            fc.texts.get(TextId::AssistantAnswerLabel),
            super::markdown::plain_text(&turn.answer)
        )
    };
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Article, true, label.clone()));
}

/// Carte d'une proposition (lot A2) : « Proposition : <libellé> —
/// n tâche(s) touchée(s) » ; en attente : Voir (onglet Opérations IA),
/// Accepter, Refuser (mêmes actions que le panneau) et un motif de refus
/// facultatif ; ensuite, l'issue (acceptée, refusée, périmée, expirée,
/// échouée). La carte ne calcule rien : le nombre de tâches vient du
/// `dry_run` du moteur.
fn proposal_card(
    ui: &mut egui::Ui,
    fc: &FrameCtx<'_>,
    t: &ChatTokens,
    card: &ProposalCard,
    applied: Option<&crate::applied::Applied>,
    events: &mut Vec<ChatEvent>,
) {
    let texts = fc.texts;
    let pending = card.status.is_pending();
    let stroke = if pending { t.accent } else { t.border };
    // Titre en Inter SemiBold (vraie graisse) quand la police est chargée.
    let title_font = if crate::theme::plan_fonts_ready(ui.ctx()) {
        FontId::new(
            BODY_SIZE,
            egui::FontFamily::Name(crate::theme::PLAN_FAMILY_STRONG.into()),
        )
    } else {
        FontId::proportional(BODY_SIZE)
    };
    let response = egui::Frame::new()
        .fill(t.surface)
        .stroke(Stroke::new(1.0, stroke))
        .corner_radius(10.0)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (icon, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                paint_icon(ui.painter(), icon, Icon::Sparkles, t.accent);
                ui.add(
                    egui::Label::new(
                        RichText::new(texts.get_args(
                            TextId::AssistantProposalTitle,
                            &[("name", Arg::Str(card.label.as_str().into()))],
                        ))
                        .font(title_font)
                        .color(t.text),
                    )
                    .wrap(),
                );
            });
            // « 10 tâches modifiées · aucune date déplacée » (R-163 amendée).
            let changed = texts.get_args(
                TextId::AssistantProposalChanged,
                &[
                    ("touched", Arg::Count(i64::from(card.touched()))),
                    ("moved", Arg::Count(i64::from(card.changed_tasks))),
                ],
            );
            ui.label(RichText::new(changed).size(META_SIZE).color(t.meta));
            ui.add_space(6.0);
            if pending {
                proposal_actions(ui, fc, t, card.id, events);
            } else {
                proposal_outcome(ui, texts, t, &card.status);
                if let Some(applied) = applied {
                    applied_lines(ui, fc, t, applied, events);
                }
            }
        })
        .response;
    let label = texts.get_args(
        TextId::AssistantProposalTitle,
        &[("name", Arg::Str(card.label.as_str().into()))],
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Group, true, label.clone()));
}

/// Boutons d'une proposition en attente et motif de refus facultatif.
fn proposal_actions(
    ui: &mut egui::Ui,
    fc: &FrameCtx<'_>,
    t: &ChatTokens,
    id: u64,
    events: &mut Vec<ChatEvent>,
) {
    let texts = fc.texts;
    let reason_id = Id::unique(("plannercraft.assistant.reason", id));
    let mut reason = ui.data(|d| d.get_temp::<String>(reason_id)).unwrap_or_default();
    ui.horizontal_wrapped(|ui| {
        let accept =
            egui::Button::new(RichText::new(texts.get(TextId::AssistantProposalAccept)).color(t.send_fg))
                .fill(t.send_bg)
                .corner_radius(6.0);
        if ui
            .push_id(("plannercraft.assistant.proposal.accept", id), |ui| {
                ui.add(accept)
            })
            .inner
            .clicked()
        {
            events.push(ChatEvent::Proposal(ProposalOp::Accept(id)));
        }
        if ui
            .push_id(("plannercraft.assistant.proposal.reject", id), |ui| {
                ui.button(texts.get(TextId::AssistantProposalReject))
            })
            .inner
            .clicked()
        {
            events.push(ChatEvent::RejectWith(id, reason.clone()));
        }
        if ui
            .push_id(("plannercraft.assistant.proposal.view", id), |ui| {
                ui.button(texts.get(TextId::AssistantProposalView))
            })
            .inner
            .clicked()
        {
            events.push(ChatEvent::Proposal(ProposalOp::View(id)));
        }
    });
    ui.add_space(4.0);
    let hint = texts.get(TextId::AssistantProposalReason);
    let response = egui::Frame::new()
        .stroke(Stroke::new(1.0, t.border))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut reason)
                    .id(reason_id.with("field"))
                    .frame(egui::Frame::NONE)
                    .char_limit(crate::assistant::MAX_REASON_CHARS)
                    .hint_text(hint.as_str())
                    .desired_width(f32::INFINITY),
            )
        })
        .inner;
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, hint.clone()));
    if response.changed() {
        ui.data_mut(|d| d.insert_temp(reason_id, reason));
    }
    ui.label(
        RichText::new(texts.get(TextId::AssistantProposalPending))
            .size(META_SIZE)
            .color(t.meta),
    );
}

/// Résultat d'une proposition acceptée (H-UI-188) : colonnes affichées,
/// « Voir les ressources ».
fn applied_lines(
    ui: &mut egui::Ui,
    fc: &FrameCtx<'_>,
    t: &ChatTokens,
    applied: &crate::applied::Applied,
    events: &mut Vec<ChatEvent>,
) {
    ui.scope(|ui| {
        ui.style_mut().override_font_id = Some(FontId::proportional(META_SIZE));
        if super::side_panel::applied_result(ui, fc, applied, Some(t.meta)) {
            events.push(ChatEvent::ShowResources);
        }
    });
}

/// Issue d'une proposition traitée.
fn proposal_outcome(ui: &mut egui::Ui, texts: &Texts, t: &ChatTokens, status: &ProposalStatus) {
    let (icon, color, text) = match status {
        ProposalStatus::Pending => return,
        ProposalStatus::Accepted => (Icon::Check, t.ok, texts.get(TextId::AssistantProposalAccepted)),
        ProposalStatus::Rejected { .. } => (
            Icon::CircleX,
            t.meta,
            texts.get(TextId::AssistantProposalRejected),
        ),
        ProposalStatus::Stale => (Icon::CircleOff, t.meta, texts.get(TextId::AssistantProposalStale)),
        ProposalStatus::Expired => (
            Icon::CircleOff,
            t.meta,
            texts.get(TextId::AssistantProposalExpired),
        ),
        ProposalStatus::Failed { code } => (
            Icon::TriangleAlert,
            t.warn,
            texts.get_args(
                TextId::AssistantProposalFailed,
                &[("detail", Arg::Str(code.as_str().into()))],
            ),
        ),
    };
    ui.horizontal(|ui| {
        let (mark, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
        paint_icon(ui.painter(), mark, icon, color);
        ui.add(egui::Label::new(RichText::new(text).size(META_SIZE).color(color)).wrap());
    });
}

/// Encadré d'erreur (ce qui s'est passé) et « Réessayer » (dernier tour).
fn error_box(
    ui: &mut egui::Ui,
    fc: &FrameCtx<'_>,
    t: &ChatTokens,
    error: &AssistantError,
    last: bool,
    events: &mut Vec<ChatEvent>,
) {
    ui.add_space(4.0);
    egui::Frame::new()
        .fill(t.error_bg)
        .stroke(Stroke::new(1.0, t.error_border))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(error_text(fc.texts, error)).color(t.error_text));
            if last {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    // Lot A4 (H-UI-158) : crédits épuisés ⇒ « Acheter des
                    // crédits » à côté de « Réessayer ».
                    if *error == AssistantError::CreditsExhausted {
                        for pack in plannercraft_assistant::relay::PACKS {
                            if ui.button(pack_label(fc.texts, pack)).clicked() {
                                events.push(ChatEvent::Op(AssistantOp::Checkout(pack.id)));
                            }
                        }
                    }
                    if ui.button(fc.texts.get(TextId::AssistantRetry)).clicked() {
                        events.push(ChatEvent::Op(AssistantOp::Regenerate));
                    }
                });
            }
        });
}

/// Arrêt : « Réponse interrompue » et « Continuer » (dernier tour).
fn stopped(ui: &mut egui::Ui, fc: &FrameCtx<'_>, t: &ChatTokens, last: bool, events: &mut Vec<ChatEvent>) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(fc.texts.get(TextId::AssistantStopped))
                .size(META_SIZE)
                .color(t.meta),
        );
        if last && ui.small_button(fc.texts.get(TextId::AssistantContinue)).clicked() {
            events.push(ChatEvent::Op(AssistantOp::Continue));
        }
    });
}

/// Pied du message (modèle · jetons · coût) et actions Copier /
/// Régénérer, visibles au survol du tour ou au focus clavier.
fn footer(
    ui: &mut egui::Ui,
    fc: &FrameCtx<'_>,
    t: &ChatTokens,
    turn: &Turn,
    ctx: &TurnCtx<'_>,
    top: f32,
    events: &mut Vec<ChatEvent>,
) {
    let texts = fc.texts;
    let copy = copy_id(ctx.index);
    let regenerate = regenerate_id();
    let focused = ui.memory(|m| m.has_focus(copy) || (ctx.last && m.has_focus(regenerate)));
    let span = Rect::from_min_max(
        pos2(ui.max_rect().left(), top),
        pos2(ui.max_rect().right(), ui.cursor().top() + 28.0),
    );
    let hovered = ui.rect_contains_pointer(span);
    ui.horizontal(|ui| {
        ui.set_height(26.0);
        if let Some(usage) = turn.usage {
            let model = model_short(&turn.model);
            let text = format!("{model} · {}", usage_text(texts, usage));
            let response = ui.label(RichText::new(text).size(META_SIZE).color(t.meta));
            response.on_hover_text(usage_detail(texts, usage));
        }
        if turn.answer.is_empty() {
            return;
        }
        ui.scope(|ui| {
            ui.set_opacity(if hovered || focused || ctx.copied {
                1.0
            } else {
                0.0
            });
            let (icon, label) = if ctx.copied {
                (Icon::Check, texts.get(TextId::AssistantCopied))
            } else {
                (Icon::Copy, texts.get(TextId::AssistantCopy))
            };
            if icon_button(ui, copy, icon, &label, t, true).clicked() {
                events.push(ChatEvent::Copy(ctx.index, turn.answer.clone()));
            }
            if ctx.last {
                let label = texts.get(TextId::AssistantRegenerate);
                if icon_button(ui, regenerate, Icon::RefreshCw, &label, t, true).clicked() {
                    events.push(ChatEvent::Op(AssistantOp::Regenerate));
                }
            }
        });
    });
}

/// État vide (H-UI-143) : transparence (AI Act art. 50) et suggestions
/// métier qui préremplissent la saisie.
fn empty_state(ui: &mut egui::Ui, fc: &FrameCtx<'_>, t: &ChatTokens, events: &mut Vec<ChatEvent>) {
    let texts = fc.texts;
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(14, 16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(texts.get(TextId::AssistantEmptyTitle))
                    .size(16.0)
                    .strong()
                    .color(t.text),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new(texts.get(TextId::AssistantDisclosure))
                    .size(META_SIZE)
                    .color(t.meta),
            );
            if fc.state.assistant.access == AssistantAccess::Account {
                ui.add_space(2.0);
                assistant_account::relay_notice(ui, fc, t);
            }
            if fc.doc.is_none() {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(texts.get(TextId::AssistantNoDocument))
                        .size(META_SIZE)
                        .color(t.meta),
                );
            }
            ui.add_space(14.0);
            for id in [
                TextId::AssistantSuggestCritical,
                TextId::AssistantSuggestLate,
                TextId::AssistantSuggestOverload,
                TextId::AssistantSuggestSummary,
            ] {
                let text = texts.get(id);
                let button = egui::Button::new(RichText::new(&text).size(13.0).color(t.text))
                    .fill(t.surface)
                    .stroke(Stroke::new(1.0, t.border))
                    .corner_radius(8.0)
                    .min_size(vec2(ui.available_width(), 30.0));
                if ui
                    .with_layout(Layout::top_down_justified(Align::LEFT), |ui| ui.add(button))
                    .inner
                    .clicked()
                {
                    events.push(ChatEvent::Suggest(text));
                }
                ui.add_space(6.0);
            }
        });
}

/// Barre d'état du flux (H-UI-143), seulement pendant une génération :
/// « En attente… » (après 800 ms sans rien), « Réflexion… », « Lecture :
/// le chemin critique… », « Rédaction… », chronomètre.
fn status_bar(ui: &mut egui::Ui, fc: &FrameCtx<'_>, panel: &AssistantPanel, t: &ChatTokens) {
    let Some(elapsed) = panel.conversation.elapsed() else {
        return;
    };
    let texts = fc.texts;
    let text = match panel.conversation.activity() {
        Activity::Idle => String::new(),
        Activity::Waiting if elapsed < WAIT_DELAY => {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(WAIT_DELAY - elapsed));
            String::new()
        }
        Activity::Waiting => texts.get(TextId::AssistantThinking),
        Activity::Reasoning => texts.get(TextId::AssistantReasoningLive),
        Activity::Reading(name) => reading_text(texts, &name),
        Activity::Writing => texts.get(TextId::AssistantWriting),
    };
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, STATUS_H), Sense::hover());
    let font = FontId::proportional(META_SIZE);
    let painter = ui.painter();
    let left = rect.left() + MARGIN;
    if !text.is_empty() {
        painter.text(
            pos2(left, rect.center().y),
            egui::Align2::LEFT_CENTER,
            &text,
            font.clone(),
            t.meta,
        );
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "secondes bornées, positives"
    )]
    let secs = elapsed.clamp(0.0, 86_400.0).floor() as u32;
    let chrono = format!("{secs} s");
    painter.text(
        pos2(rect.right() - MARGIN, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        chrono,
        font,
        t.meta,
    );
    let label = if text.is_empty() {
        texts.get(TextId::AssistantAnswerInProgress)
    } else {
        text
    };
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Status, true, label.clone()));
    let next = 1.0 - elapsed.fract();
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_secs_f64(next.clamp(0.05, 1.0)));
}

/// Saisie (H-UI-143) : 2 à 8 lignes puis défilement ; Entrée envoie,
/// Maj+Entrée va à la ligne, Échap arrête ; bouton rond à l'accent qui
/// devient « Arrêter » pendant la génération (la saisie reste possible,
/// l'envoi grisé).
fn input(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, panel: &mut AssistantPanel, t: &ChatTokens) {
    let texts = fc.texts;
    let busy = panel.conversation.busy();
    let id = question_id();
    // Ouverture du volet (bouton, Ctrl/Cmd+J, menu ; lot A5) : focus à la
    // saisie.
    if std::mem::take(&mut panel.focus_request) {
        ui.memory_mut(|m| m.request_focus(id));
    }
    let focused = ui.memory(|m| m.has_focus(id));
    // egui retire le focus sur Échap avant le dessin : l'état de l'image
    // précédente compte aussi.
    let was_focused = ui
        .data(|d| d.get_temp::<bool>(id.with("focused")))
        .unwrap_or(false);
    let enter = focused && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
    let escape =
        (focused || was_focused) && busy && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
    let label = texts.get(TextId::AssistantPlaceholder);
    let can_send = !busy && !panel.draft.trim().is_empty();
    let mut send = enter && can_send;
    let mut stop = escape;
    egui::Frame::NONE
        .inner_margin(egui::Margin {
            left: 10,
            right: 10,
            top: 2,
            bottom: 10,
        })
        .show(ui, |ui| {
            egui::Frame::new()
                .fill(t.surface)
                .stroke(Stroke::new(1.0, if focused { t.accent } else { t.border }))
                .corner_radius(12.0)
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    // Rangée bornée à son contenu (un `with_layout` prendrait
                    // toute la hauteur disponible du panneau).
                    let width = ui.available_width();
                    let row_layout = Layout::left_to_right(Align::Max);
                    ui.allocate_ui_with_layout(vec2(width, 0.0), row_layout, |ui| {
                        let row = ui.text_style_height(&TextStyle::Body);
                        let max_height = row * INPUT_MAX_ROWS + 4.0;
                        let text_width = (ui.available_width() - SEND_SIZE - 8.0).max(40.0);
                        let column = Layout::top_down(Align::Min);
                        ui.allocate_ui_with_layout(vec2(text_width, max_height), column, |ui| {
                            egui::ScrollArea::vertical()
                                .id_salt("plannercraft.assistant.question.scroll")
                                .max_height(max_height)
                                .auto_shrink([false, true])
                                .show(ui, |ui| {
                                    let response = ui.add(
                                        egui::TextEdit::multiline(&mut panel.draft)
                                            .id(id)
                                            .frame(egui::Frame::NONE)
                                            .char_limit(MAX_QUESTION_CHARS)
                                            .desired_rows(2)
                                            .hint_text(label.as_str())
                                            .desired_width(f32::INFINITY),
                                    );
                                    response.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::Role::MultilineTextInput,
                                            true,
                                            label.clone(),
                                        )
                                    });
                                });
                        });
                        ui.add_space(8.0);
                        if send_button(ui, texts, t, busy, can_send) {
                            if busy {
                                stop = true;
                            } else {
                                send = true;
                            }
                        }
                    });
                });
        });
    let now_focused = ui.memory(|m| m.has_focus(id));
    ui.data_mut(|d| d.insert_temp(id.with("focused"), now_focused));
    if stop {
        push(fc, AssistantOp::Stop);
    } else if send {
        push(fc, AssistantOp::Send);
    }
}

/// Bouton rond de 28 points : flèche (Envoyer) ou carré (Arrêter).
fn send_button(ui: &mut egui::Ui, texts: &Texts, t: &ChatTokens, busy: bool, can_send: bool) -> bool {
    let enabled = busy || can_send;
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(SEND_SIZE), Sense::hover());
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let response = ui.interact(rect, Id::unique("plannercraft.assistant.send"), sense);
    let (bg, fg) = if enabled {
        (t.send_bg, t.send_fg)
    } else {
        (t.send_disabled_bg, t.send_disabled_fg)
    };
    let bg = if enabled && response.hovered() {
        bg.gamma_multiply(0.85)
    } else {
        bg
    };
    ui.painter().circle_filled(rect.center(), SEND_SIZE / 2.0, bg);
    if response.has_focus() {
        ui.painter()
            .circle_stroke(rect.center(), SEND_SIZE / 2.0 + 2.0, Stroke::new(1.5, t.accent));
    }
    if busy {
        ui.painter()
            .rect_filled(Rect::from_center_size(rect.center(), Vec2::splat(10.0)), 2.0, fg);
    } else {
        paint_icon(ui.painter(), rect.shrink(7.0), Icon::ArrowUp, fg);
    }
    let label = texts.get(if busy {
        TextId::AssistantStop
    } else {
        TextId::AssistantSend
    });
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label.clone()));
    response.on_hover_text(label).clicked()
}

/// « Sonnet 5.5 · 2,3 k jetons · 0,007 $ » sans le modèle : jetons
/// (entrée et sortie) et coût réel d'OpenRouter (cache compris).
#[must_use]
pub fn usage_text(texts: &Texts, usage: Usage) -> String {
    let tokens = usage.prompt_tokens.saturating_add(usage.completion_tokens);
    let tokens = if tokens >= 1000 {
        #[allow(
            clippy::cast_precision_loss,
            reason = "affichage arrondi au dixième de millier"
        )]
        let k = tokens as f64 / 1000.0;
        let value = decimal(texts, &format!("{k:.1}"));
        texts.get_args(TextId::AssistantTokensK, &[("value", Arg::Str(value.into()))])
    } else {
        texts.get_args(
            TextId::AssistantTokens,
            &[("count", Arg::Count(i64::try_from(tokens).unwrap_or(i64::MAX)))],
        )
    };
    if let Some(cents) = usage.cost_cents {
        // Lot A4 : coût débité par le relais, en euros.
        let cost = if cents == 0 {
            texts.get(TextId::AssistantCostUnderCent)
        } else {
            euros(texts, cents)
        };
        return format!("{tokens} · {cost}");
    }
    match usage.cost {
        Some(cost) => {
            let digits = if cost >= 1.0 {
                2
            } else if cost >= 0.001 || cost == 0.0 {
                3
            } else {
                4
            };
            let value = decimal(texts, &format!("{cost:.digits$}"));
            let cost = texts.get_args(TextId::AssistantCostValue, &[("value", Arg::Str(value.into()))]);
            format!("{tokens} · {cost}")
        }
        None => tokens,
    }
}

/// Détail de l'infobulle du pied : entrée (dont cache) et sortie.
#[must_use]
pub fn usage_detail(texts: &Texts, usage: Usage) -> String {
    let count = |n: u64| Arg::Str(n.to_string().into());
    texts.get_args(
        TextId::AssistantUsageDetail,
        &[
            ("input", count(usage.prompt_tokens)),
            ("cached", count(usage.cached_tokens)),
            ("output", count(usage.completion_tokens)),
        ],
    )
}

/// Montant en euros (« 4,37 € », « €4.37 ») à partir de centimes.
#[must_use]
pub fn euros(texts: &Texts, cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.unsigned_abs();
    let value = decimal(texts, &format!("{sign}{}.{:02}", abs / 100, abs % 100));
    texts.get_args(TextId::AssistantEuros, &[("value", Arg::Str(value.into()))])
}

/// « 20 € de crédits » : pack à l'achat (prix entier, sans centimes).
#[must_use]
pub fn pack_label(texts: &Texts, pack: &plannercraft_assistant::relay::Pack) -> String {
    let value = texts.get_args(
        TextId::AssistantEuros,
        &[("value", Arg::Str(pack.euros.to_string().into()))],
    );
    texts.get_args(TextId::AssistantBuyPack, &[("value", Arg::Str(value.into()))])
}

/// Séparateur décimal de la langue.
fn decimal(texts: &Texts, value: &str) -> String {
    if texts.language() == Language::Fr {
        value.replace('.', ",")
    } else {
        value.to_owned()
    }
}

/// Message d'erreur en langage d'utilisateur (H-UI-142).
#[must_use]
pub fn error_text(texts: &Texts, error: &AssistantError) -> String {
    let detail = |d: String| [("detail", Arg::Str(d.into()))];
    match error {
        AssistantError::NoKey => texts.get(TextId::AssistantErrorNoKey),
        AssistantError::Unauthorized => texts.get(TextId::AssistantErrorUnauthorized),
        AssistantError::PaymentRequired => texts.get(TextId::AssistantErrorPayment),
        AssistantError::RateLimited { retry_after: Some(s) } => texts.get_args(
            TextId::AssistantErrorRateWait,
            &[("count", Arg::Count(i64::try_from(*s).unwrap_or(i64::MAX)))],
        ),
        AssistantError::RateLimited { retry_after: None } => texts.get(TextId::AssistantErrorRate),
        AssistantError::TooManyRounds => texts.get(TextId::AssistantErrorRounds),
        AssistantError::Busy => texts.get(TextId::AssistantErrorBusy),
        AssistantError::Http { .. } => texts.get_args(
            TextId::AssistantErrorHttp,
            &detail(error.detail().unwrap_or_default()),
        ),
        AssistantError::Provider { message } => {
            texts.get_args(TextId::AssistantErrorProvider, &detail(message.clone()))
        }
        AssistantError::Network { detail: d } => {
            texts.get_args(TextId::AssistantErrorNetwork, &detail(d.clone()))
        }
        AssistantError::Protocol { detail: d } => {
            texts.get_args(TextId::AssistantErrorProtocol, &detail(d.clone()))
        }
        AssistantError::Login { detail: d } => {
            texts.get_args(TextId::AssistantErrorLogin, &detail(d.clone()))
        }
        AssistantError::Keyring { detail: d } => {
            texts.get_args(TextId::AssistantErrorKeyring, &detail(d.clone()))
        }
        AssistantError::NoSession => texts.get(TextId::AssistantErrorNoSession),
        AssistantError::SessionEnded { expired: true } => texts.get(TextId::AssistantErrorSessionExpired),
        AssistantError::SessionEnded { expired: false } => texts.get(TextId::AssistantErrorSessionInvalid),
        AssistantError::CreditsExhausted => texts.get(TextId::AssistantErrorCredits),
        AssistantError::Relay { relay_code, message } => match relay_code.as_str() {
            "model.not_allowed" => texts.get(TextId::AssistantErrorModelNotAllowed),
            "upstream.unavailable" => texts.get(TextId::AssistantErrorUpstreamUnavailable),
            "upstream.error" => texts.get_args(TextId::AssistantErrorUpstream, &detail(message.clone())),
            "payment.unavailable" => texts.get(TextId::AssistantErrorPaymentUnavailable),
            "mail.unavailable" => texts.get(TextId::AssistantErrorMailUnavailable),
            "auth.invalid" | "auth.expired" => {
                texts.get_args(TextId::AssistantErrorCodeRefused, &detail(message.clone()))
            }
            _ => texts.get_args(
                TextId::AssistantErrorRelay,
                &detail(format!("{relay_code} {message}")),
            ),
        },
    }
}
