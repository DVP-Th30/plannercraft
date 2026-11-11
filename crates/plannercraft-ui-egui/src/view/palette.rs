//! Palette de commandes et champ « Rechercher une commande… » de la barre
//! du haut (interface adoucie, ADR-0026 lot U3, H-UI-177 à H-UI-183).
//!
//! Gabarit de la maquette validée (planche « palette ») : fenêtre de 580
//! points centrée sous la barre du haut (110 points du haut à 900 points de
//! haut), coins de 14, bordure d'un point au filet, ombre portée, voile sur
//! l'application ; en-tête de 52 points (loupe, saisie en 15 points) ; lignes
//! de 40 points au moins (icône 17, libellé, origine en 11,5 points atténués,
//! raccourci dans une pastille de 11 points), ligne choisie sur l'accent
//! doux ; pied de 34 points (aides au clavier). Liste, classement et
//! exécution : `crate::palette`.
//!
//! Champ de la barre du haut : 300 points (plus si le libellé l'exige),
//! loupe, « Rechercher une commande… », pastille du raccourci (⌘K sur
//! macOS, Ctrl+K ailleurs), collé à gauche du bouton de l'assistant ; quand
//! la place manque, il rétrécit jusqu'à sa largeur naturelle, puis devient
//! une loupe seule (30 points), puis disparaît. Le bouton de l'assistant
//! garde sa règle (lot A5) : le champ ne prend que la place restante.

use egui::{
    Align2, FontId, Id, Rect, Role, Sense, Shadow, Stroke, StrokeKind, Ui, Vec2, WidgetInfo, pos2, vec2,
};

use crate::action::Action;
use crate::i18n::{Arg, TextId};
use crate::keymap::{Binding, UiBinding, shortcut_for, shortcut_text};
use crate::palette::{Entry, Listed, PaletteOp, PaletteState, Run, entries, rank};
use crate::ribbon::{Icon, paint_icon};
use crate::theme::ChromeTokens;

use super::FrameCtx;

/// Largeur de la fenêtre (maquette).
const WIDTH: f32 = 580.0;
/// Haut de la fenêtre au plus (maquette, fenêtre de 900 points).
const TOP: f32 = 110.0;
/// Coins de la fenêtre.
const RADIUS: f32 = 14.0;
/// Hauteur de l'en-tête.
const HEADER_H: f32 = 52.0;
/// Hauteur du pied.
const FOOTER_H: f32 = 34.0;
/// Hauteur minimale d'une ligne.
const ROW_H: f32 = 40.0;
/// Lignes visibles avant défilement.
const VISIBLE_ROWS: f32 = 8.0;
/// Marge de la liste.
const LIST_PAD: f32 = 6.0;
/// Marge horizontale d'une ligne.
const ROW_PAD: f32 = 12.0;
/// Écart entre les colonnes d'une ligne.
const ROW_GAP: f32 = 12.0;
/// Côté des icônes.
const ICON: f32 = 17.0;
/// Corps du libellé d'une ligne.
const LABEL_SIZE: f32 = 13.0;
/// Corps de l'origine, de la raison et du pied.
const META_SIZE: f32 = 11.5;
/// Corps des pastilles de raccourci.
const KEY_SIZE: f32 = 11.0;
/// Largeur minimale d'une pastille de raccourci.
const KEY_MIN_W: f32 = 34.0;
/// Corps de la saisie.
const QUERY_SIZE: f32 = 15.0;
/// Caractères au plus dans la saisie (règle 10 : plafond écrit).
const MAX_QUERY_CHARS: usize = 200;

/// Largeur du champ de la barre du haut (maquette).
const FIELD_W: f32 = 300.0;
/// Hauteur du champ.
const FIELD_H: f32 = 30.0;
/// Marge horizontale du champ.
const FIELD_PAD: f32 = 10.0;
/// Écart entre la loupe, le libellé et la pastille.
const FIELD_GAP: f32 = 8.0;
/// Loupe du champ.
const FIELD_ICON: f32 = 15.0;
/// Corps du libellé du champ (celui des onglets).
const FIELD_TEXT: f32 = 13.0;
/// Écart du champ avec les onglets et le bouton de l'assistant.
const FIELD_MARGIN: f32 = 8.0;
/// Loupe seule : bouton carré de la barre du haut.
const FIELD_ICON_ONLY: f32 = 30.0;

/// Identifiant de la saisie de la palette (focus, tests).
#[must_use]
pub fn query_id() -> Id {
    Id::unique("plannercraft.palette.query")
}

/// Identifiant de la fenêtre de la palette.
fn palette_id() -> Id {
    Id::unique("plannercraft.palette")
}

/// Couche de la palette (modale, premier plan).
#[must_use]
pub fn layer_id() -> egui::LayerId {
    egui::LayerId::new(egui::Order::Foreground, palette_id())
}

/// Identifiant du champ de la barre du haut.
fn field_id() -> Id {
    Id::unique("plannercraft.topbar.palette")
}

/// Liaison de Ctrl/Cmd+K.
const BINDING: Binding = Binding::Ui(UiBinding::CommandPalette);

/// Texte du raccourci de la palette (« Ctrl+K », « ⌘K »).
fn hint_text(ctx: &egui::Context) -> String {
    shortcut_for(BINDING).map_or_else(String::new, |shortcut| shortcut_text(ctx, shortcut))
}

/// Pastille d'un raccourci : texte en 11 points dans un cadre d'un point.
fn paint_key(
    painter: &egui::Painter,
    tokens: &ChromeTokens,
    galley: std::sync::Arc<egui::Galley>,
    rect: Rect,
) {
    painter.rect_stroke(rect, 5.0, Stroke::new(1.0, tokens.rule), StrokeKind::Inside);
    let size = galley.size();
    painter.galley(
        pos2(rect.center().x - size.x / 2.0, rect.center().y - size.y / 2.0),
        galley,
        tokens.text_weak,
    );
}

/// Champ « Rechercher une commande… » de la barre du haut `bar`, entre la
/// fin des onglets `tabs_end` et le bord gauche du bouton de l'assistant
/// `right` ; rend son rectangle s'il est dessiné. Un clic ouvre la palette.
pub(crate) fn top_field(
    ui: &mut Ui,
    fc: &mut FrameCtx<'_>,
    bar: Rect,
    tabs_end: f32,
    right: f32,
) -> Option<Rect> {
    let ctx = ui.ctx().clone();
    let tokens = ChromeTokens::for_theme(ctx.theme());
    let label = fc.texts.get(TextId::PaletteField);
    let painter = ui.painter().clone();
    let text = painter.layout_no_wrap(label.clone(), FontId::proportional(FIELD_TEXT), tokens.text_weak);
    let key = painter.layout_no_wrap(hint_text(&ctx), FontId::proportional(KEY_SIZE), tokens.text_weak);
    let key_w = key.size().x + 10.0;
    let natural = FIELD_PAD + FIELD_ICON + FIELD_GAP + text.size().x + FIELD_GAP + key_w + FIELD_PAD;
    let room = right - FIELD_MARGIN - (tabs_end + FIELD_MARGIN);
    if !room.is_finite() || !bar.is_finite() {
        return None;
    }
    let full = natural.max(FIELD_W);
    let (width, icon_only) = if room >= full {
        (full, false)
    } else if room >= natural {
        (room, false)
    } else if room >= FIELD_ICON_ONLY {
        (FIELD_ICON_ONLY, true)
    } else {
        return None;
    };
    let rect = Rect::from_min_size(
        pos2(right - FIELD_MARGIN - width, bar.center().y - FIELD_H / 2.0),
        vec2(width, FIELD_H),
    );
    let response = ui.interact(rect, field_id(), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, &label));
    if icon_only {
        if response.hovered() || response.is_pointer_button_down_on() {
            painter.rect_filled(rect, 7.0, tokens.hover);
        }
    } else {
        let border = if response.hovered() {
            tokens.control_border
        } else {
            tokens.rule
        };
        painter.rect(
            rect,
            8.0,
            tokens.surface,
            Stroke::new(1.0, border),
            StrokeKind::Inside,
        );
    }
    if response.has_focus() {
        painter.rect_stroke(rect, 8.0, Stroke::new(1.5, tokens.accent), StrokeKind::Inside);
    }
    let icon_x = if icon_only {
        rect.center().x - FIELD_ICON / 2.0
    } else {
        rect.left() + FIELD_PAD
    };
    let icon = Rect::from_min_size(
        pos2(icon_x, rect.center().y - FIELD_ICON / 2.0),
        Vec2::splat(FIELD_ICON),
    );
    paint_icon(&painter, icon, Icon::Search, tokens.text_weak);
    if !icon_only {
        let key_rect = Rect::from_min_size(
            pos2(rect.right() - FIELD_PAD - key_w, rect.center().y - 9.0),
            vec2(key_w, 18.0),
        );
        paint_key(&painter, &tokens, key, key_rect);
        let size = text.size();
        painter.galley(
            pos2(icon.right() + FIELD_GAP, rect.center().y - size.y / 2.0),
            text,
            tokens.text_weak,
        );
    }
    let title = super::ribbon::tooltip_title(&ctx, &label, Some(BINDING));
    let tip = fc.texts.get(TextId::PaletteFieldTip);
    let response = response.on_hover_ui(|ui| super::ribbon::tooltip(ui, &title, &tip, None));
    if response.clicked() {
        fc.actions.push(Action::Palette(PaletteOp::Open));
    }
    Some(rect)
}

/// Ligne affichée : une entrée de la liste ou la demande à l'assistant.
enum Row<'a> {
    /// Entrée de la palette.
    Entry(&'a Entry),
    /// « Demander à l'assistant : « … » ».
    Ask,
}

/// Choix de l'utilisateur dans l'image.
enum Pick {
    /// Ligne d'indice donné (dans les lignes affichées).
    Row(usize),
}

/// Dessine la palette si elle est ouverte et traite ses touches (lues
/// avant le dessin par `App::palette_keys`), ses clics et sa fermeture.
#[allow(
    clippy::too_many_lines,
    reason = "en-tête, liste, pied et exécution dans l'ordre du dessin"
)]
pub fn show(ctx: &egui::Context, fc: &mut FrameCtx<'_>, state: &mut PaletteState) {
    if !state.open {
        state.listed.clear();
        return;
    }
    let tokens = ChromeTokens::for_theme(ctx.theme());
    let texts = fc.texts;
    let all = entries(ctx, fc);
    if state.query != state.last_query {
        state.last_query.clone_from(&state.query);
        state.selected = 0;
        state.scroll = true;
    }
    let query = state.query.trim().to_owned();
    let ask = fc.state.assistant.enabled && !query.is_empty();
    let mut rows: Vec<Row<'_>> = rank(&all, &state.query)
        .into_iter()
        .filter_map(|i| all.get(i).map(Row::Entry))
        .collect();
    if ask {
        rows.push(Row::Ask);
    }
    let keys = std::mem::take(&mut state.keys);
    let last = rows.len().saturating_sub(1);
    if keys.moves != 0 {
        let moved = i64::try_from(state.selected)
            .unwrap_or(0)
            .saturating_add(i64::from(keys.moves));
        state.selected = usize::try_from(moved.max(0)).unwrap_or(0).min(last);
        state.scroll = true;
    }
    state.selected = state.selected.min(last);
    let mut pick: Option<Pick> = None;
    if keys.run && !rows.is_empty() {
        pick = Some(Pick::Row(state.selected));
    }
    if keys.ask && ask {
        pick = Some(Pick::Row(last));
    }
    let content = ctx.content_rect();
    let width = WIDTH.min(content.width() - 32.0).max(200.0);
    let top = (content.height() * 0.12).clamp(16.0, TOP);
    let list_max = (ROW_H * VISIBLE_ROWS + 2.0 * LIST_PAD)
        .min(content.height() - top - HEADER_H - FOOTER_H - 24.0)
        .max(ROW_H + 2.0 * LIST_PAD);
    let id = palette_id();
    let area = egui::Modal::default_area(id).anchor(Align2::CENTER_TOP, vec2(0.0, top));
    let frame = egui::Frame::NONE
        .fill(tokens.surface)
        .stroke(Stroke::new(1.0, tokens.rule))
        .corner_radius(RADIUS)
        .shadow(Shadow {
            offset: [0, 24],
            blur: 60,
            spread: 0,
            color: tokens.float_shadow,
        });
    let mut listed = Vec::with_capacity(rows.len());
    let scroll = std::mem::take(&mut state.scroll);
    let selected = state.selected;
    let modal = egui::Modal::new(id)
        .area(area)
        .backdrop_color(tokens.scrim)
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let builder = egui::UiBuilder::new()
                .accessibility_role(Role::Dialog)
                .accessibility_label(texts.get(TextId::PaletteName));
            ui.scope_builder(builder, |ui| {
                header(ui, &tokens, texts, &mut state.query);
                let list_builder = egui::UiBuilder::new()
                    .accessibility_role(Role::ListBox)
                    .accessibility_label(texts.get(TextId::PaletteList));
                let mut clicked = None;
                ui.scope_builder(list_builder, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("plannercraft.palette.list")
                        .max_height(list_max)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            egui::Frame::NONE.inner_margin(LIST_PAD).show(ui, |ui| {
                                if rows.is_empty() {
                                    let galley = ui.painter().layout_no_wrap(
                                        texts.get(TextId::PaletteNoMatch),
                                        FontId::proportional(LABEL_SIZE),
                                        tokens.text_weak,
                                    );
                                    let (rect, _) = ui.allocate_exact_size(
                                        vec2(ui.available_width(), ROW_H),
                                        Sense::hover(),
                                    );
                                    let size = galley.size();
                                    ui.painter().galley(
                                        pos2(rect.left() + ROW_PAD, rect.center().y - size.y / 2.0),
                                        galley,
                                        tokens.text_weak,
                                    );
                                }
                                for (i, row) in rows.iter().enumerate() {
                                    let data = row_data(ctx, fc, row, &query);
                                    let (response, hovered_move) =
                                        draw_row(ui, &tokens, &data, i == selected);
                                    if i == selected && scroll {
                                        response.scroll_to_me(None);
                                    }
                                    if hovered_move {
                                        clicked = clicked.or(Some((i, false)));
                                    }
                                    if response.clicked() {
                                        clicked = Some((i, true));
                                    }
                                    listed.push(data.listed);
                                }
                            });
                        });
                });
                footer(ui, &tokens, texts, fc.state.assistant.enabled);
                clicked
            })
            .inner
        });
    match modal.inner {
        Some((i, true)) => {
            state.selected = i;
            pick = Some(Pick::Row(i));
        }
        Some((i, false)) => state.selected = i,
        None => {}
    }
    state.listed = listed;
    if let Some(Pick::Row(i)) = pick {
        match rows.get(i) {
            Some(Row::Entry(entry)) if entry.enabled.yes => {
                match &entry.run {
                    Run::Cmd(cmd) => super::chrome::push_cmd(fc, *cmd),
                    Run::Do(action) => fc.actions.push(action.clone()),
                }
                state.close();
                return;
            }
            Some(Row::Ask) => {
                fc.actions.push(Action::AskAssistant(query));
                state.close();
                return;
            }
            // Commande grisée : rien (la raison est affichée).
            _ => {}
        }
    }
    if modal.should_close() {
        state.close();
    }
}

/// En-tête : loupe et saisie (focus gardé tant que la palette est
/// ouverte), filet bas.
fn header(ui: &mut Ui, tokens: &ChromeTokens, texts: &crate::i18n::Texts, query: &mut String) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, HEADER_H), Sense::hover());
    let painter = ui.painter().clone();
    let icon = Rect::from_min_size(
        pos2(rect.left() + 16.0, rect.center().y - ICON / 2.0),
        Vec2::splat(ICON),
    );
    paint_icon(&painter, icon, Icon::Search, tokens.text_weak);
    let field = Rect::from_min_max(
        pos2(icon.right() + 10.0, rect.top()),
        pos2(rect.right() - 16.0, rect.bottom()),
    );
    let label = texts.get(TextId::PaletteField);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(field)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let response = child.add(
        egui::TextEdit::singleline(query)
            .id(query_id())
            .frame(egui::Frame::NONE)
            .font(FontId::proportional(QUERY_SIZE))
            .text_color(tokens.text)
            .hint_text(label.clone())
            .char_limit(MAX_QUERY_CHARS)
            .desired_width(field.width()),
    );
    response.widget_info(|| WidgetInfo::labeled(Role::SearchInput, true, label.clone()));
    if !response.has_focus() {
        response.request_focus();
    }
    painter.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, tokens.rule));
}

/// Pied : aides au clavier ; à droite, Tab pour l'assistant s'il est
/// activé.
fn footer(ui: &mut Ui, tokens: &ChromeTokens, texts: &crate::i18n::Texts, assistant: bool) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, FOOTER_H), Sense::hover());
    let painter = ui.painter().clone();
    painter.hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, tokens.rule));
    let font = FontId::proportional(META_SIZE);
    let mut x = rect.left() + 16.0;
    for id in [
        TextId::PaletteHintMove,
        TextId::PaletteHintRun,
        TextId::PaletteHintClose,
    ] {
        let galley = painter.layout_no_wrap(texts.get(id), font.clone(), tokens.text_weak);
        let size = galley.size();
        painter.galley(pos2(x, rect.center().y - size.y / 2.0), galley, tokens.text_weak);
        x += size.x + 16.0;
    }
    if assistant {
        let galley = painter.layout_no_wrap(texts.get(TextId::PaletteHintAsk), font, tokens.text_weak);
        let size = galley.size();
        let left = (rect.right() - 16.0 - size.x).max(x);
        painter.galley(
            pos2(left, rect.center().y - size.y / 2.0),
            galley,
            tokens.text_weak,
        );
    }
}

/// Contenu d'une ligne.
struct RowData {
    /// Icône.
    icon: Option<Icon>,
    /// Relevé (libellé, origine, raccourci, activation, raison, état).
    listed: Listed,
}

/// Contenu de la ligne `row`.
fn row_data(ctx: &egui::Context, fc: &FrameCtx<'_>, row: &Row<'_>, query: &str) -> RowData {
    match row {
        Row::Entry(entry) => RowData {
            icon: if entry.on == Some(true) {
                Some(Icon::Check)
            } else {
                entry.icon
            },
            listed: Listed {
                label: entry.label.clone(),
                origin: entry.origin.clone(),
                shortcut: entry.shortcut.map(|s| shortcut_text(ctx, &s)),
                enabled: entry.enabled.yes,
                reason: (!entry.enabled.yes).then(|| {
                    fc.texts
                        .get(entry.enabled.reason.unwrap_or(TextId::PaletteUnavailable))
                }),
                on: entry.on,
                ask: false,
            },
        },
        Row::Ask => RowData {
            icon: Some(Icon::Sparkles),
            listed: Listed {
                label: fc.texts.get_args(
                    TextId::PaletteAsk,
                    &[("query", Arg::Str(query.to_owned().into()))],
                ),
                origin: fc.texts.get(TextId::AssistantButton),
                shortcut: Some(shortcut_text(
                    ctx,
                    &egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Tab),
                )),
                enabled: true,
                reason: None,
                on: None,
                ask: true,
            },
        },
    }
}

/// Dessine une ligne ; rend sa réponse et vrai si le pointeur s'y est
/// déplacé (elle devient la ligne choisie).
fn draw_row(ui: &mut Ui, tokens: &ChromeTokens, data: &RowData, selected: bool) -> (egui::Response, bool) {
    let ctx = ui.ctx().clone();
    let painter = ui.painter().clone();
    let l = &data.listed;
    let width = ui.available_width();
    let label_color = if !l.enabled {
        tokens.disabled
    } else if selected {
        tokens.soft_text
    } else {
        tokens.text
    };
    let key_text = l.shortcut.clone().unwrap_or_else(|| "—".to_owned());
    let key = painter.layout_no_wrap(key_text, FontId::proportional(KEY_SIZE), tokens.text_weak);
    let key_w = (key.size().x + 12.0).max(KEY_MIN_W);
    let origin = painter.layout_no_wrap(
        l.origin.clone(),
        FontId::proportional(META_SIZE),
        tokens.text_weak,
    );
    let label_w = (width - 2.0 * ROW_PAD - ICON - 3.0 * ROW_GAP - origin.size().x - key_w).max(60.0);
    let font = if selected && l.enabled {
        crate::theme::strong_font(&ctx, LABEL_SIZE)
    } else {
        FontId::proportional(LABEL_SIZE)
    };
    let label = painter.layout(l.label.clone(), font, label_color, label_w);
    let reason = l.reason.as_ref().map(|reason| {
        painter.layout(
            reason.clone(),
            FontId::proportional(META_SIZE),
            tokens.text_weak,
            label_w,
        )
    });
    let text_h = label.size().y + reason.as_ref().map_or(0.0, |r| r.size().y + 2.0);
    let height = (text_h + 12.0).max(ROW_H);
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    let name = match &l.reason {
        Some(reason) => format!("{}, {reason}", l.label),
        None => l.label.clone(),
    };
    let enabled = l.enabled;
    let on = l.on;
    response.widget_info(|| match on {
        Some(on) => WidgetInfo::selected(Role::ListBoxOption, enabled, on, &name),
        None => WidgetInfo::labeled(Role::ListBoxOption, enabled, &name),
    });
    if selected {
        ctx.accesskit_node_builder(response.id, |node| node.set_selected(true));
        painter.rect_filled(rect, 8.0, tokens.toggle_bg);
    }
    let moved = response.hovered() && ui.input(|i| i.pointer.delta() != Vec2::ZERO);
    let icon_color = if l.ask && l.enabled {
        tokens.accent
    } else {
        label_color
    };
    if let Some(icon) = data.icon {
        let r = Rect::from_min_size(
            pos2(rect.left() + ROW_PAD, rect.center().y - ICON / 2.0),
            Vec2::splat(ICON),
        );
        paint_icon(&painter, r, icon, icon_color);
    }
    let text_x = rect.left() + ROW_PAD + ICON + ROW_GAP;
    let mut y = rect.center().y - text_h / 2.0;
    let label_h = label.size().y;
    painter.galley(pos2(text_x, y), label, label_color);
    y += label_h + 2.0;
    if let Some(reason) = reason {
        painter.galley(pos2(text_x, y), reason, tokens.text_weak);
    }
    let key_rect = Rect::from_min_size(
        pos2(rect.right() - ROW_PAD - key_w, rect.center().y - 9.0),
        vec2(key_w, 18.0),
    );
    paint_key(&painter, tokens, key, key_rect);
    let origin_size = origin.size();
    painter.galley(
        pos2(
            key_rect.left() - ROW_GAP - origin_size.x,
            rect.center().y - origin_size.y / 2.0,
        ),
        origin,
        tokens.text_weak,
    );
    (response, moved)
}
