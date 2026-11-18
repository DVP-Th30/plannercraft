//! Fenêtre « Mon compte » du mode « Compte PlannerCraft » (H-UI-238 à
//! H-UI-243), identique sur le desktop et le web.
//!
//! - Entrées : menu « … » de l'assistant, palette de commandes (« Mon
//!   compte… »), clic sur le solde sous l'en-tête de l'assistant.
//! - Contenu : adresse et « Se déconnecter » ; solde et « Acheter des
//!   crédits » (packs et flux d'achat du volet) ; **Achats**
//!   (`/v1/purchases` : date, libellé, montant signé, « Reçu ↗ » ; les
//!   ajustements, petites corrections de débit nombreuses, regroupés en
//!   une ligne « n ajustements » et leur somme) ;
//!   **Consommation (30 j)** (`/v1/usage`, résumé par modèle : demandes et
//!   coût, total). Hors connexion : invitation à se connecter, qui renvoie
//!   au formulaire du volet.
//! - Un relais antérieur à `/v1/purchases` (`404`) : section remplacée par
//!   un message discret, jamais d'erreur bloquante.
//! - Le volet ne calcule rien : montants en centimes du relais, sommes
//!   entières par modèle.

use std::collections::BTreeMap;

use egui::{Align, Id, Layout, RichText, Sense, Vec2};
use plannercraft_assistant::relay::{self, Me, Purchase, PurchaseKind, UsageReport};

use super::FrameCtx;
use super::assistant::{META_SIZE, error_text, euros, model_short, push};
use super::assistant_account::{buy_button, info};
use crate::action::Action;
use crate::assistant::{AssistantOp, AssistantPanel, Purchases};
use crate::i18n::{Arg, Language, TextId, Texts};
use crate::ribbon::Icon;
use crate::ribbon::icons::paint_icon;
use crate::state::AssistantAccess;
use crate::theme::ChatTokens;
use crate::view::side_panel::{SideOp, SideTab};

/// Largeur du contenu de la fenêtre, en points.
pub const WINDOW_W: f32 = 380.0;
/// Hauteur au plus de la liste des achats avant défilement, en points.
const PURCHASES_MAX_H: f32 = 180.0;
/// Hauteur au plus du résumé de consommation avant défilement, en points.
const USAGE_MAX_H: f32 = 140.0;

/// Identifiant de la fenêtre.
#[must_use]
pub fn window_id() -> Id {
    Id::unique("plannercraft.account-window")
}

/// Dessine la fenêtre si elle est ouverte (une image).
pub fn window(ctx: &egui::Context, fc: &mut FrameCtx<'_>, panel: &AssistantPanel) {
    if !panel.account().window_open() {
        return;
    }
    let area = super::format_window::work_area(ctx);
    let mut open = true;
    egui::Window::new(fc.texts.get(TextId::AccountWindowTitle))
        .id(window_id())
        .open(&mut open)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(area.center())
        .constrain(true)
        .movable(true)
        .resizable(false)
        .collapsible(false)
        .default_width(WINDOW_W)
        .show(ctx, |ui| {
            ui.set_width(WINDOW_W);
            body(ui, fc, panel);
        });
    if !open {
        push(fc, AssistantOp::AccountWindow(false));
    }
}

fn body(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, panel: &AssistantPanel) {
    let t = ChatTokens::for_theme(ui.ctx().theme());
    let account = panel.account();
    let texts = fc.texts;
    if fc.state.assistant.access != AssistantAccess::Account || !account.signed_in() {
        signed_out(ui, fc, panel, &t);
        return;
    }
    let me: Option<&Me> = account.me.as_ref();
    // Adresse, « Se déconnecter » (H-UI-239).
    ui.horizontal(|ui| {
        ui.set_min_height(28.0);
        let email = me.map_or_else(|| "…".to_owned(), |m| m.email.clone());
        ui.label(RichText::new(email).strong().color(t.text));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button(texts.get(TextId::AssistantLogout)).clicked() {
                push(fc, AssistantOp::AccountLogout);
            }
        });
    });
    // Solde, « Acheter des crédits » (H-UI-239).
    ui.horizontal(|ui| {
        ui.set_min_height(30.0);
        ui.label(RichText::new(texts.get(TextId::AccountWindowBalance)).color(t.meta));
        let value = me.map_or_else(|| "…".to_owned(), |m| euros(texts, m.balance_cents));
        ui.label(RichText::new(value).size(15.0).strong().color(t.text));
        if me.is_some_and(Me::low) {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
            paint_icon(ui.painter(), rect, Icon::TriangleAlert, t.warn);
            ui.label(
                RichText::new(texts.get(TextId::AssistantBalanceLow))
                    .size(META_SIZE)
                    .color(t.warn),
            );
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let working = account.working();
            buy_button(ui, fc, &t, !working);
            if working {
                ui.spinner();
            }
        });
    });
    info(ui, fc, panel, &t);
    if let Some(crate::assistant::Notice::Error(error)) = &panel.notice {
        ui.label(
            RichText::new(error_text(texts, error))
                .size(META_SIZE)
                .color(t.error_text),
        );
    }
    ui.separator();
    section_title(ui, &texts.get(TextId::AccountWindowPurchases), &t);
    purchases(ui, fc, account.purchases(), &t);
    ui.add_space(6.0);
    ui.separator();
    section_title(ui, &texts.get(TextId::AccountWindowUsage), &t);
    usage(ui, texts, account.usage.as_ref(), account.usage_loading(), &t);
}

fn section_title(ui: &mut egui::Ui, text: &str, t: &ChatTokens) {
    ui.label(RichText::new(text).size(13.0).strong().color(t.text));
    ui.add_space(2.0);
}

/// Hors connexion (H-UI-238) : invitation à se connecter, qui active le
/// mode compte et ouvre le volet sur le formulaire existant.
fn signed_out(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, panel: &AssistantPanel, t: &ChatTokens) {
    let texts = fc.texts;
    if fc.state.assistant.access == AssistantAccess::Account && panel.account().loading() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(RichText::new(texts.get(TextId::AssistantKeyLoading)).color(t.meta));
        });
        return;
    }
    ui.label(RichText::new(texts.get(TextId::AccountWindowSignedOut)).color(t.text));
    ui.add_space(8.0);
    if ui.button(texts.get(TextId::AccountWindowSignIn)).clicked() {
        push(fc, AssistantOp::ChooseAccess(AssistantAccess::Account));
        fc.actions
            .push(Action::SidePanel(SideOp::Open(SideTab::Assistant)));
        push(fc, AssistantOp::AccountWindow(false));
    }
}

/// Section « Achats » (H-UI-240, H-UI-241).
fn purchases(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, purchases: &Purchases, t: &ChatTokens) {
    let texts = fc.texts;
    let meta = |ui: &mut egui::Ui, id: TextId| {
        ui.label(RichText::new(texts.get(id)).size(META_SIZE).color(t.meta));
    };
    match purchases {
        Purchases::Idle | Purchases::Loading => {
            ui.horizontal(|ui| {
                ui.spinner();
                meta(ui, TextId::AccountWindowPurchasesLoading);
            });
        }
        Purchases::Unavailable => meta(ui, TextId::AccountWindowPurchasesUnavailable),
        Purchases::Failed(error) => {
            let text = texts.get_args(
                TextId::AccountWindowPurchasesFailed,
                &[("detail", Arg::Str(error_text(texts, error).into()))],
            );
            ui.label(RichText::new(text).size(META_SIZE).color(t.meta));
        }
        Purchases::Ready(report) if report.items.is_empty() => {
            meta(ui, TextId::AccountWindowPurchasesEmpty);
        }
        Purchases::Ready(report) => {
            let (adjustments, shown): (Vec<&Purchase>, Vec<&Purchase>) = report
                .items
                .iter()
                .partition(|p| p.kind == PurchaseKind::Adjustment);
            let mut opened: Option<String> = None;
            egui::ScrollArea::vertical()
                .id_salt("plannercraft_account_purchases")
                .max_height(PURCHASES_MAX_H)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    egui::Grid::new("plannercraft_account_purchases_grid")
                        .num_columns(4)
                        .spacing([12.0, 4.0])
                        .show(ui, |ui| {
                            for item in shown {
                                if let Some(url) = purchase_row(ui, texts, item, t) {
                                    opened = Some(url);
                                }
                                ui.end_row();
                            }
                            // Ajustements (petites corrections de débit,
                            // nombreuses) : une seule ligne, somme signée.
                            if let Some((count, cents)) = adjustments_total(&adjustments) {
                                ui.label("");
                                let label = texts
                                    .get_args(TextId::AccountKindAdjustment, &[("count", Arg::Count(count))]);
                                ui.label(RichText::new(label).size(META_SIZE).color(t.meta));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(
                                        RichText::new(signed_euros(texts, cents))
                                            .size(META_SIZE)
                                            .color(t.meta),
                                    );
                                });
                                ui.end_row();
                            }
                        });
                });
            meta(ui, TextId::AccountWindowDatesUtc);
            if let Some(url) = opened {
                push(fc, AssistantOp::OpenReceipt(url));
            }
        }
    }
}

/// Une ligne d'achat ; rend l'adresse du reçu cliqué.
fn purchase_row(ui: &mut egui::Ui, texts: &Texts, item: &Purchase, t: &ChatTokens) -> Option<String> {
    ui.label(
        RichText::new(short_date(texts, &item.at))
            .size(META_SIZE)
            .color(t.meta),
    );
    ui.label(
        RichText::new(kind_label(texts, item))
            .size(META_SIZE)
            .color(t.text),
    );
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.label(
            RichText::new(signed_euros(texts, item.cents))
                .size(META_SIZE)
                .strong()
                .color(t.text),
        );
    });
    let url = item.receipt_url.as_deref()?;
    let link = ui
        .link(RichText::new(texts.get(TextId::AccountWindowReceipt)).size(META_SIZE))
        .on_hover_text(texts.get(TextId::AccountWindowReceiptHint));
    link.clicked().then(|| url.to_owned())
}

/// Section « Consommation (30 j) » (H-UI-241) : demandes et coût par
/// modèle, total du relais.
fn usage(ui: &mut egui::Ui, texts: &Texts, report: Option<&UsageReport>, loading: bool, t: &ChatTokens) {
    let Some(report) = report else {
        let id = if loading {
            TextId::AccountWindowUsageLoading
        } else {
            TextId::AccountWindowUsageUnavailable
        };
        ui.horizontal(|ui| {
            if loading {
                ui.spinner();
            }
            ui.label(RichText::new(texts.get(id)).size(META_SIZE).color(t.meta));
        });
        return;
    };
    if report.items.is_empty() {
        ui.label(
            RichText::new(texts.get(TextId::AssistantUsageEmpty))
                .size(META_SIZE)
                .color(t.meta),
        );
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt("plannercraft_account_usage")
        .max_height(USAGE_MAX_H)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            egui::Grid::new("plannercraft_account_usage_grid")
                .num_columns(3)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    for (model, requests, cents) in usage_by_model(report) {
                        ui.label(RichText::new(model_short(&model)).size(META_SIZE).color(t.text));
                        let requests =
                            texts.get_args(TextId::AccountWindowRequests, &[("count", Arg::Count(requests))]);
                        ui.label(RichText::new(requests).size(META_SIZE).color(t.meta));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(euros(texts, cents)).size(META_SIZE).color(t.text));
                        });
                        ui.end_row();
                    }
                });
        });
    let total = euros(texts, report.total_cents);
    ui.label(
        RichText::new(texts.get_args(TextId::AssistantUsageTotal, &[("value", Arg::Str(total.into()))]))
            .size(META_SIZE)
            .strong()
            .color(t.text),
    );
}

/// Ajustements regroupés : nombre et somme signée (centimes) ; `None`
/// s'il n'y en a pas.
#[must_use]
pub fn adjustments_total(adjustments: &[&Purchase]) -> Option<(i64, i64)> {
    if adjustments.is_empty() {
        return None;
    }
    let count = i64::try_from(adjustments.len()).unwrap_or(i64::MAX);
    let cents = adjustments
        .iter()
        .fold(0i64, |sum, p| sum.saturating_add(p.cents));
    Some((count, cents))
}

/// Demandes et coût (centimes) par modèle, du plus coûteux au moins
/// coûteux, puis par nom.
#[must_use]
pub fn usage_by_model(report: &UsageReport) -> Vec<(String, i64, i64)> {
    let mut by_model: BTreeMap<&str, (i64, i64)> = BTreeMap::new();
    for item in &report.items {
        let entry = by_model.entry(item.model.as_str()).or_insert((0, 0));
        entry.0 = entry.0.saturating_add(1);
        entry.1 = entry.1.saturating_add(item.cost_cents);
    }
    let mut rows: Vec<(String, i64, i64)> = by_model
        .into_iter()
        .map(|(model, (count, cents))| (model.to_owned(), count, cents))
        .collect();
    rows.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
    rows
}

/// Libellé d'un mouvement : « Pack 5 € » (pack de [`relay::PACKS`]),
/// « Achat » (pack inconnu), « Essai », « Crédit offert », « Remboursement »,
/// « Ajustement », « Autre mouvement » (sorte inconnue de cette version).
#[must_use]
pub fn kind_label(texts: &Texts, item: &Purchase) -> String {
    match &item.kind {
        PurchaseKind::Purchase => item.pack.as_deref().and_then(relay::pack).map_or_else(
            || texts.get(TextId::AccountKindPurchase),
            |pack| {
                let value = texts.get_args(
                    TextId::AssistantEuros,
                    &[("value", Arg::Str(pack.euros.to_string().into()))],
                );
                texts.get_args(TextId::AccountKindPack, &[("value", Arg::Str(value.into()))])
            },
        ),
        PurchaseKind::Trial => texts.get(TextId::AccountKindTrial),
        PurchaseKind::Grant => texts.get(TextId::AccountKindGrant),
        PurchaseKind::Refund => texts.get(TextId::AccountKindRefund),
        PurchaseKind::Adjustment => {
            texts.get_args(TextId::AccountKindAdjustment, &[("count", Arg::Count(1))])
        }
        PurchaseKind::Other(_) => texts.get(TextId::AccountKindOther),
    }
}

/// Montant signé : « +5,00 € », « -5,00 € », « 0,00 € ».
#[must_use]
pub fn signed_euros(texts: &Texts, cents: i64) -> String {
    let value = euros(texts, cents);
    if cents > 0 { format!("+{value}") } else { value }
}

/// Date d'un horodatage RFC 3339 (UTC, sans conversion) : « 18/11/2026 »
/// en français, « 2026-11-18 » en anglais ; texte brut s'il ne commence
/// pas par une date.
#[must_use]
pub fn short_date(texts: &Texts, at: &str) -> String {
    let date = at.get(..10).unwrap_or(at);
    let parts: Vec<&str> = date.split('-').collect();
    let well_formed = parts.len() == 3
        && parts
            .iter()
            .zip([4, 2, 2])
            .all(|(p, len)| p.len() == len && p.bytes().all(|b| b.is_ascii_digit()));
    match (well_formed, texts.language(), parts.as_slice()) {
        (true, Language::Fr, [year, month, day]) => format!("{day}/{month}/{year}"),
        (true, _, _) => date.to_owned(),
        _ => at.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : un échec doit s'arrêter net"
    )]

    use super::*;
    use plannercraft_assistant::relay::UsageItem;

    fn purchase(kind: PurchaseKind, cents: i64, pack: Option<&str>) -> Purchase {
        Purchase {
            at: "2026-11-18T10:05:25Z".to_owned(),
            kind,
            cents,
            pack: pack.map(str::to_owned),
            receipt_url: None,
        }
    }

    #[test]
    fn h_ui_240_labels_amounts_dates() {
        let fr = Texts::new(Language::Fr);
        let en = Texts::new(Language::En);
        let nb = '\u{a0}';
        assert_eq!(
            kind_label(&fr, &purchase(PurchaseKind::Purchase, 500, Some("p5"))),
            format!("Pack 5{nb}€")
        );
        assert_eq!(
            kind_label(&en, &purchase(PurchaseKind::Purchase, 500, Some("p5"))),
            "€5 pack"
        );
        assert_eq!(
            kind_label(&fr, &purchase(PurchaseKind::Purchase, 500, Some("starter"))),
            "Achat"
        );
        assert_eq!(
            kind_label(&fr, &purchase(PurchaseKind::Adjustment, -3, None)),
            "Ajustement"
        );
        let adjustments = [
            purchase(PurchaseKind::Adjustment, -3, None),
            purchase(PurchaseKind::Adjustment, -2, None),
        ];
        let refs: Vec<&Purchase> = adjustments.iter().collect();
        assert_eq!(adjustments_total(&refs), Some((2, -5)));
        assert_eq!(adjustments_total(&[]), None);
        assert_eq!(
            fr.get_args(TextId::AccountKindAdjustment, &[("count", Arg::Count(2))]),
            format!("2{nb}ajustements")
        );
        assert_eq!(kind_label(&fr, &purchase(PurchaseKind::Trial, 50, None)), "Essai");
        assert_eq!(
            kind_label(
                &fr,
                &purchase(PurchaseKind::Other("cashback".to_owned()), 1, None)
            ),
            "Autre mouvement"
        );
        assert_eq!(signed_euros(&fr, 500), format!("+5,00{nb}€"));
        assert_eq!(signed_euros(&fr, -250), format!("-2,50{nb}€"));
        assert_eq!(signed_euros(&en, 0), "€0.00");
        assert_eq!(short_date(&fr, "2026-11-18T10:05:25Z"), "18/11/2026");
        assert_eq!(short_date(&fr, "2026-11-18T10:05:25.123456Z"), "18/11/2026");
        assert_eq!(short_date(&en, "2026-11-18T10:05:25Z"), "2026-11-18");
        assert_eq!(short_date(&fr, "hier"), "hier");
        assert_eq!(short_date(&fr, "2026-1-189"), "2026-1-189");
    }

    #[test]
    fn h_ui_241_usage_grouped_by_model() {
        let item = |model: &str, cents: i64| UsageItem {
            at: "2026-11-18T10:00:00Z".to_owned(),
            model: model.to_owned(),
            prompt_tokens: 1,
            completion_tokens: 1,
            cached_tokens: 0,
            cost_cents: cents,
        };
        let report = UsageReport {
            items: vec![item("b", 3), item("a", 2), item("b", 4), item("c", 7)],
            total_cents: 16,
        };
        assert_eq!(
            usage_by_model(&report),
            [
                ("b".to_owned(), 2, 7),
                ("c".to_owned(), 1, 7),
                ("a".to_owned(), 1, 2)
            ]
        );
    }
}
