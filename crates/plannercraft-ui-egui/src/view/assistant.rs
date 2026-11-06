//! Onglet « Assistant » du volet latéral (ADR-0024, lot A1, H-UI-138 à
//! H-UI-142) : activation, connexion OpenRouter, modèle, conversation en
//! flux, arrêt, coût. Le volet ne calcule rien (règle 3) : il affiche le
//! texte du modèle, qui cite le moteur par ses outils.

use egui::{Key, Modifiers, RichText};
use plannercraft_assistant::conversation::{Activity, Entry};
use plannercraft_assistant::error::AssistantError;
use plannercraft_assistant::openrouter::{self, MODELS};
use plannercraft_assistant::stream::Usage;

use super::FrameCtx;
use crate::action::Action;
use crate::assistant::{AssistantOp, AssistantPanel, MAX_KEY_CHARS, MAX_QUESTION_CHARS, Notice};
use crate::i18n::{Arg, Language, TextId, Texts};

/// Hauteur réservée sous la conversation (activité, saisie, boutons).
const INPUT_RESERVE: f32 = 120.0;

/// Identifiant du champ de la question.
#[must_use]
pub fn question_id() -> egui::Id {
    egui::Id::unique("plannercraft.assistant.question")
}

fn push(fc: &mut FrameCtx<'_>, op: AssistantOp) {
    fc.actions.push(Action::Assistant(op));
}

/// Dessine l'onglet.
pub fn tab(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, panel: &mut AssistantPanel) {
    let texts = fc.texts;
    if !fc.state.assistant.enabled {
        intro(ui, fc);
        return;
    }
    let weak = fc.palette.text_weak;
    ui.label(
        RichText::new(texts.get(TextId::AssistantDisclosure))
            .small()
            .color(weak),
    );
    ui.separator();
    connection(ui, fc, panel);
    model_row(ui, fc);
    if let Some(notice) = &panel.notice {
        let (text, color) = match notice {
            Notice::KeyringMissing => (texts.get(TextId::AssistantKeyringMissing), weak),
            Notice::Error(error) => (error_text(texts, error), ui.visuals().error_fg_color),
        };
        ui.label(RichText::new(text).color(color));
    }
    ui.separator();
    if fc.doc.is_none() {
        ui.label(RichText::new(texts.get(TextId::AssistantNoDocument)).color(weak));
    }
    let height = (ui.available_height() - INPUT_RESERVE).max(80.0);
    egui::ScrollArea::vertical()
        .id_salt("plannercraft_assistant_transcript")
        .max_height(height)
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| transcript(ui, fc, panel));
    activity(ui, fc, panel.conversation.activity());
    input(ui, fc, panel);
    ui.horizontal(|ui| {
        if ui.small_button(texts.get(TextId::AssistantClear)).clicked() {
            push(fc, AssistantOp::Clear);
        }
        if ui.small_button(texts.get(TextId::AssistantDisable)).clicked() {
            push(fc, AssistantOp::Disable);
        }
    });
}

/// Avant l'activation : mention de transparence (AI Act art. 50) et
/// bouton « Activer l'assistant » (H-UI-138).
fn intro(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    let texts = fc.texts;
    ui.label(RichText::new(texts.get(TextId::AssistantIntroTitle)).strong());
    ui.label(texts.get(TextId::AssistantDisclosure));
    ui.label(texts.get(TextId::AssistantReadOnly));
    ui.add_space(6.0);
    if ui.button(texts.get(TextId::AssistantEnable)).clicked() {
        push(fc, AssistantOp::Enable);
    }
}

/// Connexion (H-UI-139) : se connecter, saisir une clé, se déconnecter.
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
    if panel.connected() {
        ui.horizontal_wrapped(|ui| {
            let status = if panel.key_persistent() {
                TextId::AssistantConnected
            } else {
                TextId::AssistantConnectedSession
            };
            ui.label(texts.get(status));
            if ui.small_button(texts.get(TextId::AssistantLogout)).clicked() {
                push(fc, AssistantOp::Logout);
            }
        });
        return;
    }
    if panel.key_entry {
        let label = texts.get(TextId::AssistantKeyHint);
        ui.label(&label);
        let response = ui.add(
            egui::TextEdit::singleline(&mut panel.key_draft)
                .id(egui::Id::unique("plannercraft.assistant.key"))
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

/// Liste fermée des modèles (H-UI-140).
fn model_row(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    let texts = fc.texts;
    let current = openrouter::model_or_default(&fc.state.assistant.model);
    let label = texts.get(TextId::AssistantModel);
    let mut chosen = None;
    ui.horizontal(|ui| {
        ui.label(&label);
        let response = egui::ComboBox::from_id_salt("plannercraft.assistant.model")
            .selected_text(current.label)
            .show_ui(ui, |ui| {
                for model in MODELS {
                    if ui.selectable_label(model.id == current.id, model.label).clicked() {
                        chosen = Some(model.id);
                    }
                }
            })
            .response;
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, label.clone()));
    });
    if let Some(id) = chosen.filter(|id| *id != current.id) {
        push(fc, AssistantOp::SetModel(id));
    }
}

/// Conversation (H-UI-141).
fn transcript(ui: &mut egui::Ui, fc: &FrameCtx<'_>, panel: &AssistantPanel) {
    let texts = fc.texts;
    let weak = fc.palette.text_weak;
    for (i, entry) in panel.conversation.transcript().iter().enumerate() {
        ui.push_id(("plannercraft_assistant_entry", i), |ui| match entry {
            Entry::User(text) => {
                ui.add_space(4.0);
                ui.label(RichText::new(texts.get(TextId::AssistantYou)).strong());
                ui.label(text.as_str());
            }
            Entry::Assistant(text) => {
                ui.add_space(4.0);
                ui.label(RichText::new(texts.get(TextId::AssistantName)).strong());
                ui.label(text.as_str());
            }
            Entry::Tools(names) => {
                let detail = names.join(", ");
                ui.label(
                    RichText::new(
                        texts.get_args(TextId::AssistantToolsUsed, &[("detail", Arg::Str(detail.into()))]),
                    )
                    .small()
                    .color(weak),
                );
            }
            Entry::Error(error) => {
                ui.label(RichText::new(error_text(texts, error)).color(ui.visuals().error_fg_color));
            }
            Entry::Stopped => {
                ui.label(
                    RichText::new(texts.get(TextId::AssistantStopped))
                        .small()
                        .color(weak),
                );
            }
            Entry::Cost(usage) => {
                ui.label(RichText::new(cost_text(texts, *usage)).small().color(weak));
            }
        });
    }
}

/// Indicateur d'activité (H-UI-141) : « lit le planning… » pendant les
/// appels d'outils.
fn activity(ui: &mut egui::Ui, fc: &FrameCtx<'_>, activity: Activity) {
    let id = match activity {
        Activity::Idle => return,
        Activity::Waiting => TextId::AssistantThinking,
        Activity::ReadingPlan => TextId::AssistantReading,
        Activity::Writing => TextId::AssistantWriting,
    };
    ui.label(
        RichText::new(fc.texts.get(id))
            .italics()
            .color(fc.palette.text_weak),
    );
}

/// Saisie de la question, Envoyer ou Arrêter (H-UI-141). Entrée envoie,
/// Maj+Entrée va à la ligne.
fn input(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, panel: &mut AssistantPanel) {
    let texts = fc.texts;
    let busy = panel.conversation.busy();
    let id = question_id();
    let focused = ui.memory(|m| m.has_focus(id));
    let enter = focused && !busy && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
    let label = texts.get(TextId::AssistantPlaceholder);
    let response = ui.add(
        egui::TextEdit::multiline(&mut panel.draft)
            .id(id)
            .char_limit(MAX_QUESTION_CHARS)
            .desired_rows(2)
            .hint_text(label.as_str())
            .desired_width(f32::INFINITY),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::MultilineTextInput, true, label.clone()));
    ui.horizontal(|ui| {
        if busy {
            if ui.button(texts.get(TextId::AssistantStop)).clicked() {
                push(fc, AssistantOp::Stop);
            }
        } else {
            let can_send = !panel.draft.trim().is_empty();
            if ui
                .add_enabled(can_send, egui::Button::new(texts.get(TextId::AssistantSend)))
                .clicked()
                || (enter && can_send)
            {
                push(fc, AssistantOp::Send);
            }
        }
    });
}

/// Coût d'une demande : `usage.cost` d'OpenRouter (dollars US) et jetons.
#[must_use]
pub fn cost_text(texts: &Texts, usage: Usage) -> String {
    let tokens =
        i64::try_from(usage.prompt_tokens.saturating_add(usage.completion_tokens)).unwrap_or(i64::MAX);
    match usage.cost {
        Some(cost) => {
            let mut value = format!("{cost:.4}");
            if texts.language() == Language::Fr {
                value = value.replace('.', ",");
            }
            texts.get_args(
                TextId::AssistantCost,
                &[("value", Arg::Str(value.into())), ("count", Arg::Count(tokens))],
            )
        }
        None => texts.get_args(TextId::AssistantCostTokens, &[("count", Arg::Count(tokens))]),
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
    }
}
