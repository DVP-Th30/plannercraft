//! Mode « Compte PlannerCraft » de l'onglet Assistant (ADR-0024, lot A4,
//! H-UI-155 à H-UI-160) : choix de l'accès à l'activation, connexion par
//! e-mail et code, solde et achat de crédits sous l'en-tête, consommation,
//! mention du relais. Le volet ne calcule rien : solde, coûts et prix
//! viennent du relais, en centimes d'euro.

use egui::{Align, Id, Key, Layout, RichText, Sense, Stroke, Vec2, vec2};
use plannercraft_assistant::relay::{self, Me, PACKS};

use super::FrameCtx;
use super::assistant::{META_SIZE, euros, icon_button, model_short, pack_label, push};
use crate::assistant::{AccountInfo, AccountStage, AssistantOp, AssistantPanel};
use crate::i18n::{Arg, TextId};
use crate::ribbon::Icon;
use crate::ribbon::icons::paint_icon;
use crate::state::AssistantAccess;
use crate::theme::ChatTokens;

/// Guide de l'utilisateur, section du compte (lien « En savoir plus »).
pub const ACCOUNT_DOC_URL: &str =
    "https://github.com/DVP-Th30/plannercraft/blob/main/docs/user/assistant.md#compte-plannercraft";

/// Identifiant du champ de l'adresse.
#[must_use]
pub fn email_id() -> Id {
    Id::unique("plannercraft.assistant.account.email")
}

/// Identifiant du champ du code.
#[must_use]
pub fn code_id() -> Id {
    Id::unique("plannercraft.assistant.account.code")
}

/// Bouton principal (fond de la couleur d'envoi).
fn primary(ui: &mut egui::Ui, t: &ChatTokens, text: &str, enabled: bool) -> egui::Response {
    let (fill, color) = if enabled {
        (t.send_bg, t.send_fg)
    } else {
        (t.send_disabled_bg, t.send_disabled_fg)
    };
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text).color(color).strong())
            .fill(fill)
            .corner_radius(6.0)
            .min_size(vec2(0.0, 28.0)),
    )
}

/// Avant l'activation (H-UI-155) : deux accès, le compte PlannerCraft mis
/// en avant (« Recommandé »), la clé OpenRouter à côté.
pub fn access_choice(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, t: &ChatTokens) {
    let texts = fc.texts;
    ui.add_space(12.0);
    ui.label(
        RichText::new(texts.get(TextId::AssistantAccessTitle))
            .size(13.0)
            .strong()
            .color(t.text),
    );
    ui.add_space(8.0);
    let card = |stroke: Stroke| {
        egui::Frame::new()
            .fill(t.surface)
            .stroke(stroke)
            .corner_radius(8.0)
            .inner_margin(egui::Margin::same(12))
    };
    card(Stroke::new(1.5, t.accent)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(texts.get(TextId::AssistantAccessAccount))
                    .size(14.0)
                    .strong()
                    .color(t.text),
            );
            egui::Frame::new()
                .fill(t.accent)
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(6, 1))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(texts.get(TextId::AssistantAccessRecommended))
                            .size(11.0)
                            .color(t.send_fg),
                    );
                });
        });
        ui.add_space(4.0);
        ui.label(
            RichText::new(texts.get(TextId::AssistantAccessAccountHint))
                .size(META_SIZE)
                .color(t.meta),
        );
        ui.add_space(8.0);
        if primary(ui, t, &texts.get(TextId::AssistantUseAccount), true).clicked() {
            push(fc, AssistantOp::ChooseAccess(AssistantAccess::Account));
        }
    });
    ui.add_space(8.0);
    card(Stroke::new(1.0, t.border)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(
            RichText::new(texts.get(TextId::AssistantAccessOwnKey))
                .size(14.0)
                .strong()
                .color(t.text),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new(texts.get(TextId::AssistantAccessOwnKeyHint))
                .size(META_SIZE)
                .color(t.meta),
        );
        ui.add_space(8.0);
        if ui.button(texts.get(TextId::AssistantUseOwnKey)).clicked() {
            push(fc, AssistantOp::ChooseAccess(AssistantAccess::OwnKey));
        }
    });
}

/// Mention du relais (mode compte) et lien vers le guide.
pub fn relay_notice(ui: &mut egui::Ui, fc: &FrameCtx<'_>, t: &ChatTokens) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(
            RichText::new(fc.texts.get(TextId::AssistantRelayNotice))
                .size(META_SIZE)
                .color(t.meta),
        );
        ui.hyperlink_to(
            RichText::new(fc.texts.get(TextId::AssistantRelayMore)).size(META_SIZE),
            ACCOUNT_DOC_URL,
        );
    });
}

/// Connexion au compte (H-UI-156) : adresse, puis code reçu par e-mail.
pub fn login(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, panel: &mut AssistantPanel, t: &ChatTokens) {
    let texts = fc.texts;
    let working = panel.account().working();
    ui.add_space(4.0);
    match panel.account().stage() {
        AccountStage::SignedIn => {}
        AccountStage::Loading => {
            ui.label(RichText::new(texts.get(TextId::AssistantKeyLoading)).color(t.meta));
        }
        AccountStage::Email => {
            ui.label(RichText::new(texts.get(TextId::AssistantAccountSignIn)).color(t.text));
            ui.add_space(6.0);
            let label = texts.get(TextId::AssistantAccountEmail);
            ui.label(RichText::new(&label).size(META_SIZE).color(t.meta));
            let response = ui.add(
                egui::TextEdit::singleline(&mut panel.account_mut().email_draft)
                    .id(email_id())
                    .char_limit(relay::MAX_EMAIL_CHARS)
                    .desired_width(f32::INFINITY),
            );
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, label.clone()));
            let entered = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            let can = !working && relay::plausible_email(&panel.account().email_draft);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if primary(ui, t, &texts.get(TextId::AssistantAccountSendLink), can).clicked()
                    || (entered && can)
                {
                    push(fc, AssistantOp::AccountSendLink);
                }
                if working {
                    ui.spinner();
                    ui.label(
                        RichText::new(texts.get(TextId::AssistantAccountWorking))
                            .size(META_SIZE)
                            .color(t.meta),
                    );
                }
            });
        }
        AccountStage::Code(email) => {
            ui.label(
                RichText::new(texts.get_args(
                    TextId::AssistantAccountCheckMail,
                    &[("email", Arg::Str(email.into()))],
                ))
                .color(t.text),
            );
            ui.add_space(6.0);
            let label = texts.get(TextId::AssistantAccountCode);
            ui.label(RichText::new(&label).size(META_SIZE).color(t.meta));
            let response = ui.add(
                egui::TextEdit::singleline(&mut panel.account_mut().code_draft)
                    .id(code_id())
                    .char_limit(relay::MAX_CODE_CHARS)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY),
            );
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, label.clone()));
            let entered = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            let can = !working && relay::plausible_code(&panel.account().code_draft);
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                if primary(ui, t, &texts.get(TextId::AssistantAccountVerify), can).clicked()
                    || (entered && can)
                {
                    push(fc, AssistantOp::AccountVerify);
                }
                if working {
                    ui.spinner();
                    ui.label(
                        RichText::new(texts.get(TextId::AssistantAccountWorking))
                            .size(META_SIZE)
                            .color(t.meta),
                    );
                }
                if ui
                    .add_enabled(
                        !working,
                        egui::Button::new(texts.get(TextId::AssistantAccountResend)).small(),
                    )
                    .clicked()
                {
                    push(fc, AssistantOp::AccountResend);
                }
                if ui
                    .add_enabled(
                        !working,
                        egui::Button::new(texts.get(TextId::AssistantAccountChangeEmail)).small(),
                    )
                    .clicked()
                {
                    push(fc, AssistantOp::AccountChangeEmail);
                }
            });
        }
    }
    ui.add_space(4.0);
}

/// Bouton « Acheter des crédits » et son menu des packs (H-UI-158).
pub fn buy_button(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, t: &ChatTokens, enabled: bool) {
    let texts = fc.texts;
    let label = texts.get(TextId::AssistantBuyCredits);
    let button = egui::Button::new(RichText::new(format!("{label}    ")).size(13.0).color(t.text))
        .fill(t.surface)
        .stroke(Stroke::new(1.0, t.border))
        .corner_radius(6.0);
    let response = ui.add_enabled(enabled, button);
    let r = response.rect;
    let chevron = egui::Rect::from_center_size(egui::pos2(r.right() - 12.0, r.center().y), Vec2::splat(12.0));
    paint_icon(ui.painter(), chevron, Icon::ChevronDown, t.meta);
    egui::Popup::menu(&response).show(|ui| {
        ui.set_min_width(180.0);
        for pack in PACKS {
            if ui.button(pack_label(texts, pack)).clicked() {
                push(fc, AssistantOp::Checkout(pack.id));
            }
        }
        ui.separator();
        ui.label(
            RichText::new(texts.get(TextId::AssistantBuyHint))
                .size(11.0)
                .color(t.meta),
        );
    });
}

/// Bande du compte sous l'en-tête (H-UI-157) : solde, alerte de solde bas,
/// achat de crédits, actualisation.
pub fn balance_bar(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, panel: &AssistantPanel, t: &ChatTokens) {
    let texts = fc.texts;
    let me: Option<&Me> = panel.account().me.as_ref();
    let value = me.map_or_else(|| "…".to_owned(), |m| euros(texts, m.balance_cents));
    let working = panel.account().working();
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.set_min_height(30.0);
        ui.label(
            RichText::new(texts.get_args(TextId::AssistantBalance, &[("value", Arg::Str(value.into()))]))
                .size(13.0)
                .strong()
                .color(t.text),
        );
        if me.is_some_and(Me::low) {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
            paint_icon(ui.painter(), rect, Icon::TriangleAlert, t.warn);
            ui.label(
                RichText::new(texts.get(TextId::AssistantBalanceLow))
                    .size(META_SIZE)
                    .color(t.warn),
            )
            .on_hover_text(texts.get(TextId::AssistantBalanceLowHint));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let label = texts.get(TextId::AssistantRefreshBalance);
            if icon_button(
                ui,
                Id::unique("plannercraft.assistant.account.refresh"),
                Icon::RefreshCw,
                &label,
                t,
                true,
            )
            .clicked()
            {
                push(fc, AssistantOp::AccountRefresh);
            }
            buy_button(ui, fc, t, !working);
            if working {
                ui.spinner();
            }
        });
    });
    ui.painter().hline(
        ui.min_rect().x_range(),
        ui.cursor().top() + 2.0,
        Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
    );
    ui.add_space(6.0);
}

/// Information du compte (paiement ouvert, retour de paiement).
pub fn info(ui: &mut egui::Ui, fc: &FrameCtx<'_>, panel: &AssistantPanel, t: &ChatTokens) {
    let Some(info) = panel.account().info else {
        return;
    };
    let id = match info {
        AccountInfo::CheckoutOpened => TextId::AssistantCheckoutOpened,
        AccountInfo::CheckoutReturn => TextId::AssistantCheckoutReturn,
        AccountInfo::CheckoutCancelled => TextId::AssistantCheckoutCancelled,
    };
    ui.label(RichText::new(fc.texts.get(id)).size(META_SIZE).color(t.meta));
}

/// Éléments du menu « … » propres au compte (H-UI-159) : adresse,
/// Consommation, Actualiser le solde, Se déconnecter du compte.
pub fn menu_items(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, panel: &AssistantPanel, t: &ChatTokens) {
    let texts = fc.texts;
    let Some(email) = panel.account().signed_in().then(|| {
        panel
            .account()
            .me
            .as_ref()
            .map_or_else(String::new, |m| m.email.clone())
    }) else {
        return;
    };
    if !email.is_empty() {
        ui.label(
            RichText::new(texts.get_args(
                TextId::AssistantAccountConnected,
                &[("email", Arg::Str(email.into()))],
            ))
            .size(META_SIZE)
            .color(t.meta),
        );
    }
    if ui.button(texts.get(TextId::AssistantUsageMenu)).clicked() {
        push(fc, AssistantOp::ShowUsage(true));
    }
    if ui.button(texts.get(TextId::AssistantRefreshBalance)).clicked() {
        push(fc, AssistantOp::AccountRefresh);
    }
    if ui.button(texts.get(TextId::AssistantAccountLogout)).clicked() {
        push(fc, AssistantOp::AccountLogout);
    }
    ui.separator();
}

/// Choix de l'accès dans le menu « … » (H-UI-155).
pub fn access_items(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, t: &ChatTokens) {
    let texts = fc.texts;
    ui.label(
        RichText::new(texts.get(TextId::AssistantAccessMenu))
            .size(META_SIZE)
            .color(t.meta),
    );
    let current = fc.state.assistant.access;
    for (access, id) in [
        (AssistantAccess::Account, TextId::AssistantAccessAccount),
        (AssistantAccess::OwnKey, TextId::AssistantAccessOwnKey),
    ] {
        if ui.radio(current == access, texts.get(id)).clicked() && current != access {
            push(fc, AssistantOp::SetAccess(access));
        }
    }
    ui.separator();
}

/// « Consommation » (H-UI-159) : demandes des 30 derniers jours (relais),
/// total, retour à la conversation.
pub fn usage_view(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, panel: &AssistantPanel, t: &ChatTokens) {
    let texts = fc.texts;
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(texts.get(TextId::AssistantUsageTitle))
                .size(14.0)
                .strong()
                .color(t.text),
        );
    });
    ui.label(
        RichText::new(texts.get(TextId::AssistantUsageUtc))
            .size(11.0)
            .color(t.meta),
    );
    ui.add_space(6.0);
    match &panel.account().usage {
        None => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new(texts.get(TextId::AssistantUsageLoading)).color(t.meta));
            });
        }
        Some(report) => {
            let total = euros(texts, report.total_cents);
            ui.label(
                RichText::new(
                    texts.get_args(TextId::AssistantUsageTotal, &[("value", Arg::Str(total.into()))]),
                )
                .size(13.0)
                .strong()
                .color(t.text),
            );
            ui.add_space(4.0);
            if report.items.is_empty() {
                ui.label(RichText::new(texts.get(TextId::AssistantUsageEmpty)).color(t.meta));
            }
            let height = (ui.available_height() - 44.0).max(80.0);
            egui::ScrollArea::vertical()
                .id_salt("plannercraft_assistant_usage")
                .max_height(height)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for item in &report.items {
                        let when = item.at.get(..16).unwrap_or(&item.at).replace('T', " ");
                        let tokens = item.prompt_tokens.saturating_add(item.completion_tokens);
                        let tokens =
                            texts.get_args(TextId::AssistantTokens, &[("count", Arg::Count(tokens))]);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(when).size(META_SIZE).color(t.meta));
                            ui.label(
                                RichText::new(model_short(&item.model))
                                    .size(META_SIZE)
                                    .color(t.text),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    RichText::new(euros(texts, item.cost_cents))
                                        .size(META_SIZE)
                                        .color(t.text),
                                );
                                ui.label(RichText::new(tokens).size(META_SIZE).color(t.meta));
                            });
                        });
                    }
                });
        }
    }
    ui.add_space(8.0);
    if ui.button(texts.get(TextId::AssistantBack)).clicked() {
        push(fc, AssistantOp::ShowUsage(false));
    }
}
