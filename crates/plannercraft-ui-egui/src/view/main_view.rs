//! Vue principale : en-têtes, séparateur, `ScrollArea` verticale unique pour
//! la table et le Gantt (alignés dans la même image), défilement horizontal
//! du Gantt.
//!
//! Le défilement vertical vit dans la mémoire de la `ScrollArea` ; il n'est
//! réimposé qu'une fois par `ViewTransient::pending_scroll_y`. Le défilement
//! horizontal du Gantt est l'état transitoire `ViewTransient::gantt_scroll_x`,
//! lu par l'en-tête et par le corps dans la même image. Les gestes produisent
//! des `Action` (`ScrollGanttX`, `ZoomStep`, `SetTableWidth`, `Select`) ;
//! aucune conversion pixel → date : seuls des indices de ligne sont déduits
//! d'une ordonnée.
//!
//! Sélection multiple et menu contextuel (lot L14) : un clic de ligne du
//! Gantt avec Ctrl/Cmd ou Maj devient `SelectWith` (le Gantt ne pousse que
//! `Select`, réécrit ici d'après les modificateurs de l'image ; la table
//! pousse ses propres gestes `SelectOp`, ADR-0018) ; un clic droit hors de
//! la sélection sélectionne la cellule (table) ou la ligne (Gantt) cliquée,
//! puis ouvre le menu contextuel jumeau du ruban (`CONTEXT_MENU`, H-UI-100) ;
//! un clic droit sur l'en-tête de l'échelle ouvre le menu du zoom
//! (`TIMESCALE_MENU`). Un clic dans le Gantt
//! laisse le clavier à la table (ADR-0018 D8, H-UI-101) ; pendant un
//! glisser de sélection, de barre ou de lien, la vue défile seule près des
//! bords ([`autoscroll`], H-UI-91).
//!
//! Gestes du Gantt (J7, lot L8, H-UI-20, H-UI-21, R-240 à R-243) : un
//! glisser qui part d'une barre ([`gantt::hit_test`]) devient un
//! [`BarDrag`] ; l'interface n'en garde qu'un écart en jours entiers
//! (`TimeAxis::days_for_dx`), l'aperçu vient de
//! `plannercraft_automation::gesture::preview` (recalculé quand l'écart change,
//! jamais de recalcul du planning) et le lâcher pousse un seul lot
//! (`ExecOn`). Glisser dans le vide : défilement, inchangé. Double-clic sur
//! une barre : « Informations sur la tâche » ; sur un lien : boîte
//! « Dépendance ». Échap annule le glisser.
//!
//! Volets (ADR-0019 D5, H-UI-110, H-UI-111, lot L4) : la zone est découpée
//! par `panes::layout` en chronologie, vue et volet de détails ; deux
//! séparateurs tirés (±4 points sur la frontière, hauteur écrite au
//! relâcher, double-clic ⇒ hauteur par défaut) ; volet de détails avec
//! en-tête « Formulaire tâche » et × ; F6 / Maj+F6 servis ici (focus
//! d'egui donné au × d'un volet ou à la table).

use egui::containers::menu::menu_style;
use egui::scroll_area::ScrollBarVisibility;
use egui::{
    Align, CursorIcon, Id, Layout, PointerButton, Popup, PopupAnchor, PopupKind, Pos2, Rect, Sense,
    SetOpenCommand, Stroke, pos2, vec2,
};

use plannercraft_automation::gesture::{self, GestureKind};
use plannercraft_core::TaskUid;

use super::{BarDrag, FrameCtx, OVERSCAN, Pane, ViewTransient, chrome, gantt, table, timeline, timescale};
use crate::action::{Action, DialogKind, PaneOp, PressMode, PressTarget, SelectOp};
use crate::app::OpenDocument;
use crate::column::Column;
use crate::geom::{self, GanttHit, MainLayout};
use crate::i18n::TextId;
use crate::nav::RowRef;
use crate::panes::{self, PaneFocus, SplitDrag, Splitter};
use crate::ribbon::spec::{CONTEXT_MENU, RibbonCmd, TIMESCALE_MENU};
use crate::row_view::{DisplayRow, VisibleRows};
use crate::rows::RowKind;
use crate::selection::SelectMode;
use crate::state::{
    DETAILS_HEIGHT_DEFAULT, DETAILS_HEIGHT_MIN, TIMELINE_HEIGHT_DEFAULT, TIMELINE_HEIGHT_MIN,
};
use crate::timescale::TimeAxis;

/// Largeur minimale du curseur de la barre horizontale, en points.
const HBAR_MIN_THUMB: f32 = 16.0;

/// Pas de zoom à la molette : un pas par facteur 1,25 cumulé.
const ZOOM_STEP_FACTOR: f32 = 1.25;

/// Défilement horizontal du Gantt borné à `[0, max_scroll]` (0 si non fini).
fn clamp_scroll(scroll: f32, max_scroll: f32) -> f32 {
    if scroll.is_finite() {
        scroll.clamp(0.0, max_scroll)
    } else {
        0.0
    }
}

/// Défilement horizontal maximal : contenu moins partie visible, jamais
/// négatif ni non fini.
fn max_scroll_x(content_w: f32, visible_w: f32) -> f32 {
    let max = content_w - visible_w;
    if max.is_finite() { max.max(0.0) } else { 0.0 }
}

/// Entrées lues au-dessus du Gantt pour cette image.
#[derive(Clone, Copy, Debug)]
struct GanttInput {
    /// Pointeur au-dessus du Gantt (corps ou en-tête).
    hovered: bool,
    /// Position du pointeur.
    pointer: Option<Pos2>,
    /// Facteur de zoom (Ctrl/Cmd + molette, pincement) ; 1 = rien.
    zoom_delta: f32,
}

/// Affiche la vue principale dans la place restante de `ui`.
pub fn show(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, view: &mut ViewTransient) {
    let full = ui.available_rect_before_wrap();
    // Volets (ADR-0019 D5, H-UI-111) : chronologie en tête (ADR-0017,
    // H-UI-78), vue (table et Gantt) au milieu, volet de détails en bas ;
    // hauteurs bornées par `panes::layout` (pendant un glisser de
    // séparateur : hauteur suivie au pointeur, écrite au relâcher).
    let slots = pane_slots(ui, fc, view, full);
    let timeline_rect = slots.timeline;
    let avail = slots.view;
    let metrics = *fc.metrics;
    let layout = geom::main_layout(avail, f32::from(fc.state.effective_table_width()), &metrics);
    let Some(doc) = fc.doc else {
        view.timeline.drawn = None;
        view.panes.drawn = None;
        view.split_drag = None;
        ui.advance_cursor_after_rect(full);
        return;
    };
    let gantt_w = layout.gantt_clip.width();
    view.last_gantt_width = gantt_w;
    view.last_gantt_left = layout.gantt_clip.left();
    let content_w = TimeAxis::new(doc.range, fc.state.zoom, 0.0).content_width();
    let max_scroll = max_scroll_x(content_w, gantt_w);
    let scroll_x = clamp_scroll(view.gantt_scroll_x, max_scroll);
    view.gantt_scroll_x = scroll_x;
    let axis = TimeAxis::new(doc.range, fc.state.zoom, layout.gantt_clip.left() - scroll_x);
    match timeline_rect {
        Some(rect) => timeline::show(ui, fc, &mut view.timeline, rect, Some((&axis, layout.gantt_clip))),
        None => view.timeline.drawn = None,
    }
    // Défilement horizontal de la table (J7, lot L6) : borné à chaque image
    // (une colonne masquée ou rétrécie réduit le maximum).
    let table_w = layout.table_clip.width();
    view.last_table_width = table_w;
    let columns_w = super::table::content_width(ui.ctx(), fc.state);
    let table_max = max_scroll_x(columns_w, table_w);
    let table_x = clamp_scroll(view.table_scroll_x, table_max);
    view.table_scroll_x = table_x;
    let table_zone = layout.table_clip.union(layout.table_header);
    let table_hovered = ui.rect_contains_pointer(table_zone);

    let gantt_zone = layout.gantt_clip.union(layout.gantt_header);
    let hovered = ui.rect_contains_pointer(gantt_zone);
    let input = ui.input(|i| GanttInput {
        hovered,
        pointer: i.pointer.hover_pos(),
        zoom_delta: i.zoom_delta(),
    });

    table::paint_header(ui, fc, layout.table_header, table_x, &mut view.select_drag);
    timescale::paint_header(ui, fc, &axis, layout.gantt_header);
    paint_corners(ui, fc, &layout, avail);

    let RowsOut {
        dx: mut delta_x,
        context_click,
        scroll_y,
        view_h,
    } = paint_rows(ui, fc, view, &layout, &axis);
    view.last_scroll_y = scroll_y;
    view.last_body_height = view_h;
    autoscroll(ui, fc, view, &layout);
    // Fin du glisser de sélection au relâcher (après le dessin : en-tête et
    // corps ont vu le même glisser pendant toute l'image).
    if !ui.input(|i| i.pointer.primary_down()) {
        view.select_drag = None;
    }
    if std::mem::take(&mut view.edit.reveal) {
        reveal_edited_row(fc, view);
    }
    context_menu(ui, fc, &*view, context_click);
    timescale_menu(ui, fc, secondary_click(ui, layout.gantt_header).is_some());

    // Molette horizontale (ou Maj + molette) au-dessus du Gantt : la
    // `ScrollArea` verticale ne consomme pas l'axe x (vérifié par le test
    // `shift_wheel_over_gantt_scrolls_x`), on le lit donc après elle.
    if input.hovered {
        let wheel_x = ui.input(|i| i.smooth_scroll_delta.x);
        if wheel_x.is_finite() && wheel_x.abs() > f32::EPSILON {
            delta_x -= wheel_x;
            ui.input_mut(|i| i.smooth_scroll_delta.x = 0.0);
        }
        zoom_from_wheel(fc, view, &layout, input);
    }
    // Maj + molette au-dessus de la table : défilement horizontal de la
    // table (H-UI-17, lot L6).
    let mut wheel_table = 0.0;
    if table_hovered && !input.hovered {
        let wheel_x = ui.input(|i| i.smooth_scroll_delta.x);
        if wheel_x.is_finite() && wheel_x.abs() > f32::EPSILON {
            wheel_table -= wheel_x;
            ui.input_mut(|i| i.smooth_scroll_delta.x = 0.0);
        }
    }

    paint_splitter(ui, fc, &layout, avail);
    delta_x += paint_hbar(ui, fc, &layout, scroll_x, content_w);
    let under_table = Rect::from_min_max(
        pos2(layout.table_clip.left(), layout.gantt_hbar.top()),
        pos2(layout.table_clip.right(), layout.gantt_hbar.bottom()),
    );
    wheel_table += table::paint_hbar(ui, fc, under_table, table_x, table_w);
    let table_target = clamp_scroll(table_x + wheel_table, table_max);
    let table_step = table_target - table_x;
    if table_step.abs() > f32::EPSILON {
        fc.actions.push(Action::ScrollTableX(table_step));
    }

    let target = clamp_scroll(scroll_x + delta_x, max_scroll);
    let dx = target - scroll_x;
    if dx.abs() > f32::EPSILON {
        fc.actions.push(Action::ScrollGanttX(dx));
    }
    // Volet de détails, séparateurs (après la table et le Gantt : ils
    // l'emportent sur les bords qu'ils chevauchent), puis F6.
    if let Some(rect) = slots.details {
        details_pane(ui, fc, &mut view.details, rect);
    }
    let drawn = pane_splitters(ui, fc, view, &slots);
    serve_focus_request(ui, view, &drawn);
    view.panes.drawn = Some(drawn);
    ui.advance_cursor_after_rect(full);
}

/// Relevé des volets de la dernière image (H-UI-111 ; F6, tests).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanesDrawn {
    /// Chronologie, si affichée.
    pub timeline: Option<Rect>,
    /// Vue (table et Gantt).
    pub view: Rect,
    /// Volet de détails (en-tête compris), si affiché.
    pub details: Option<Rect>,
    /// Zone de saisie du séparateur sous la chronologie.
    pub timeline_splitter: Option<Rect>,
    /// Zone de saisie du séparateur au-dessus du volet de détails.
    pub details_splitter: Option<Rect>,
}

/// État transitoire des volets (jamais persisté ; H-UI-111).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PanesView {
    /// Relevé de la dernière image (`None` : vue sans document).
    pub drawn: Option<PanesDrawn>,
    /// F6 (`Some(false)`) ou Maj+F6 (`Some(true)`) en attente : servi à
    /// l'image suivante, quand les volets affichés sont connus.
    pub focus_request: Option<bool>,
}

/// Hauteur de l'en-tête du volet de détails (titre et ×), en points.
pub const DETAILS_HEADER_H: f32 = 22.0;

/// Côté du bouton × des volets, en points.
const CLOSE_SIDE: f32 = 16.0;

/// Rectangles des volets de l'image.
#[derive(Clone, Copy, Debug)]
struct PaneSlots {
    /// Chronologie, si affichée.
    timeline: Option<Rect>,
    /// Vue (table et Gantt).
    view: Rect,
    /// Volet de détails, si affiché.
    details: Option<Rect>,
}

/// Hauteur d'un volet : pendant le glisser de son séparateur, suivie au
/// pointeur depuis la hauteur du début (bornée à `[min, part × zone]`) ;
/// sinon la hauteur enregistrée.
fn live_height(ui: &egui::Ui, view: &ViewTransient, splitter: Splitter, saved: u16, total: f32) -> f32 {
    let (min, share) = match splitter {
        Splitter::Timeline => (TIMELINE_HEIGHT_MIN, panes::TIMELINE_MAX_SHARE),
        Splitter::Details => (DETAILS_HEIGHT_MIN, panes::DETAILS_MAX_SHARE),
    };
    let Some(d) = view.split_drag.filter(|d| d.splitter == splitter) else {
        return f32::from(saved);
    };
    let Some(y) = ui.input(|i| i.pointer.latest_pos()).map(|p| p.y) else {
        return d.start_height;
    };
    // Séparateur du bas : tirer vers le haut agrandit le volet.
    let delta = match splitter {
        Splitter::Timeline => y - d.start_y,
        Splitter::Details => d.start_y - y,
    };
    let min = f32::from(min);
    panes::drag_split(d.start_height, delta, min, (share * total).max(min))
}

/// Découpe `full` en volets (`panes::layout`).
fn pane_slots(ui: &egui::Ui, fc: &FrameCtx<'_>, view: &ViewTransient, full: Rect) -> PaneSlots {
    let total = full.height();
    let timeline = timeline::wanted(fc)
        .then(|| live_height(ui, view, Splitter::Timeline, fc.state.timeline_height, total));
    let details = (fc.state.details.visible && fc.doc.is_some())
        .then(|| live_height(ui, view, Splitter::Details, fc.state.details.height, total));
    let h = panes::layout(total, timeline, details);
    let view_top = full.top() + h.timeline;
    let view_rect = Rect::from_min_size(pos2(full.left(), view_top), vec2(full.width(), h.view));
    PaneSlots {
        timeline: (h.timeline > 0.0).then(|| Rect::from_min_size(full.min, vec2(full.width(), h.timeline))),
        view: view_rect,
        details: (h.details > 0.0).then(|| {
            Rect::from_min_size(
                pos2(full.left(), view_rect.bottom()),
                vec2(full.width(), h.details),
            )
        }),
    }
}

/// Hauteur d'écran en points entiers pour `UiState` (non finie ⇒ 0, bornée
/// ensuite par `App::pane_op`).
fn height_u16(h: f32) -> u16 {
    u16::try_from(geom::f32_to_index(h.round())).unwrap_or(u16::MAX)
}

/// Identifiant d'un séparateur de volets.
fn splitter_id(which: Splitter) -> Id {
    Id::unique("plannercraft.pane_splitter").with(which == Splitter::Details)
}

/// Identifiant du × du volet de détails (focus de F6).
#[must_use]
pub fn details_close_id() -> Id {
    Id::unique("plannercraft.details.close")
}

/// Séparateur horizontal à l'ordonnée `y` (H-UI-111) : zone de ±4 points,
/// curseur `ResizeVertical`, glisser suivi par `view.split_drag`,
/// double-clic ⇒ hauteur par défaut. `height` : hauteur affichée du volet.
fn splitter(
    ui: &mut egui::Ui,
    fc: &mut FrameCtx<'_>,
    view: &mut ViewTransient,
    which: Splitter,
    (y, x, height): (f32, egui::Rangef, f32),
) -> Option<Rect> {
    if !y.is_finite() || !height.is_finite() || x.span() < 1.0 {
        return None;
    }
    let grab = Rect::from_x_y_ranges(x, (y - panes::SPLITTER_GRAB)..=(y + panes::SPLITTER_GRAB));
    let texts = fc.texts;
    let response = ui.interact(grab, splitter_id(which), Sense::click_and_drag());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Splitter, true, texts.get(TextId::SplitterTip))
    });
    let response = response.on_hover_and_drag_cursor(CursorIcon::ResizeVertical);
    if response.hovered() || response.dragged() {
        ui.painter().line_segment(
            [pos2(x.min, y), pos2(x.max, y)],
            Stroke::new(2.0, fc.palette.grid_major),
        );
        fc.stats.shapes = fc.stats.shapes.saturating_add(1);
    }
    if response.double_clicked() {
        view.split_drag = None;
        fc.actions.push(Action::Pane(match which {
            Splitter::Timeline => PaneOp::SetTimelineHeight(TIMELINE_HEIGHT_DEFAULT),
            Splitter::Details => PaneOp::SetDetailsHeight(DETAILS_HEIGHT_DEFAULT),
        }));
    } else if response.drag_started() {
        let start_y = ui
            .input(|i| i.pointer.press_origin())
            .or_else(|| response.interact_pointer_pos())
            .map_or(y, |p| p.y);
        view.split_drag = Some(SplitDrag {
            splitter: which,
            start_height: height,
            start_y,
        });
    }
    Some(grab)
}

/// Séparateurs des volets affichés ; au relâcher d'un glisser, la hauteur
/// bornée de l'image est écrite dans `UiState` (`PaneOp::Set*Height`).
fn pane_splitters(
    ui: &mut egui::Ui,
    fc: &mut FrameCtx<'_>,
    view: &mut ViewTransient,
    slots: &PaneSlots,
) -> PanesDrawn {
    let x = slots.view.x_range();
    let timeline_splitter = slots
        .timeline
        .and_then(|r| splitter(ui, fc, view, Splitter::Timeline, (r.bottom(), x, r.height())));
    let details_splitter = slots
        .details
        .and_then(|r| splitter(ui, fc, view, Splitter::Details, (r.top(), x, r.height())));
    if let Some(d) = view.split_drag
        && !ui.input(|i| i.pointer.primary_down())
    {
        view.split_drag = None;
        let op = match d.splitter {
            Splitter::Timeline => slots
                .timeline
                .map(|r| PaneOp::SetTimelineHeight(height_u16(r.height()))),
            Splitter::Details => slots
                .details
                .map(|r| PaneOp::SetDetailsHeight(height_u16(r.height()))),
        };
        if let Some(op) = op {
            fc.actions.push(Action::Pane(op));
        }
    }
    PanesDrawn {
        timeline: slots.timeline,
        view: slots.view,
        details: slots.details,
        timeline_splitter,
        details_splitter,
    }
}

/// Bouton × de 16 points d'un volet (chronologie, détails) : peint si
/// `shown` ou s'il a le focus clavier ; nom accessible et info-bulle
/// `tip`. Rend (cliqué, peint).
pub(crate) fn close_button(
    ui: &mut egui::Ui,
    fc: &mut FrameCtx<'_>,
    rect: Rect,
    id: Id,
    tip: TextId,
    shown: bool,
) -> (bool, bool) {
    if !rect.is_finite() {
        return (false, false);
    }
    let texts = fc.texts;
    let response = ui.interact(rect, id, Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, texts.get(tip)));
    let focused = response.has_focus();
    let visible = shown || focused;
    if visible {
        let p = ui.painter();
        if response.hovered() || focused {
            p.rect_filled(rect, 3.0, fc.palette.grid);
        }
        let r = rect.shrink(4.5);
        let stroke = Stroke::new(1.5, fc.palette.text);
        p.line_segment([r.left_top(), r.right_bottom()], stroke);
        p.line_segment([r.right_top(), r.left_bottom()], stroke);
        fc.stats.shapes = fc.stats.shapes.saturating_add(3);
    }
    let response = response.on_hover_text(texts.get(tip));
    (response.clicked(), visible)
}

/// Volet de détails dans `rect` (H-UI-112, cadre du lot L4) : en-tête de 22
/// points « Formulaire tâche » et ×, puis le contenu (`details::show`).
fn details_pane(
    ui: &mut egui::Ui,
    fc: &mut FrameCtx<'_>,
    details: &mut Option<super::details::DetailsTransient>,
    rect: Rect,
) {
    if !rect.is_finite() || rect.height() < 1.0 {
        return;
    }
    let header = Rect::from_min_size(rect.min, vec2(rect.width(), DETAILS_HEADER_H.min(rect.height())));
    let painter = ui.painter_at(rect);
    painter.rect_filled(header, 0.0, fc.palette.header_bg);
    painter.line_segment(
        [rect.left_top(), rect.right_top()],
        Stroke::new(1.0, fc.palette.grid_major),
    );
    painter.text(
        pos2(header.left() + 8.0, header.center().y),
        egui::Align2::LEFT_CENTER,
        fc.texts.get(TextId::DetailsTitle),
        egui::FontId::proportional(12.0),
        fc.palette.text_strong,
    );
    fc.stats.shapes = fc.stats.shapes.saturating_add(3);
    let close = Rect::from_center_size(
        pos2(header.right() - 6.0 - CLOSE_SIDE * 0.5, header.center().y),
        vec2(CLOSE_SIDE, CLOSE_SIDE),
    );
    let (clicked, _) = close_button(ui, fc, close, details_close_id(), TextId::DetailsCloseTip, true);
    if clicked {
        fc.actions.push(Action::Pane(PaneOp::ShowDetails(false)));
    }
    let body = Rect::from_min_max(pos2(rect.left(), header.bottom()), rect.max);
    super::details::show(ui, fc, details, body);
}

/// F6 / Maj+F6 en attente (H-UI-111) : volet courant lu au focus d'egui
/// (rectangle du widget focalisé), clavier donné au volet suivant ou
/// précédent : table (focus de la table), chronologie ou détails (leur ×).
fn serve_focus_request(ui: &egui::Ui, view: &mut ViewTransient, drawn: &PanesDrawn) {
    let Some(back) = view.panes.focus_request.take() else {
        return;
    };
    let ctx = ui.ctx();
    let current = ctx
        .memory(egui::Memory::focused)
        .and_then(|id| ctx.read_response(id))
        .map_or(PaneFocus::Table, |r| {
            let c = r.rect.center();
            if drawn.timeline.is_some_and(|t| t.contains(c)) {
                PaneFocus::Timeline
            } else if drawn.details.is_some_and(|d| d.contains(c)) {
                PaneFocus::Details
            } else {
                PaneFocus::Table
            }
        });
    let target = panes::next(current, (drawn.timeline.is_some(), drawn.details.is_some()), back);
    match target {
        PaneFocus::Table => {
            view.focus = Pane::Table;
            ctx.memory_mut(|m| m.request_focus(table::table_id()));
        }
        PaneFocus::Timeline => ctx.memory_mut(|m| m.request_focus(timeline::close_id())),
        PaneFocus::Details => ctx.memory_mut(|m| m.request_focus(details_close_id())),
    }
    ctx.request_repaint();
}

/// Saisie ouverte (F2, frappe, double-clic) : sa ligne, si elle n'est pas
/// entièrement à l'écran, y est amenée avec une ligne de marge (H-UI-17) ;
/// la zone de saisie est dessinée dès l'image suivante (relecture finale J7,
/// F2 : une saisie ouverte hors de l'écran ne recevait pas la frappe).
fn reveal_edited_row(fc: &FrameCtx<'_>, view: &mut ViewTransient) {
    let (Some(rows), Some(state)) = (fc.visible(), view.edit.editing.as_ref()) else {
        return;
    };
    let index = match state.uid {
        Some(uid) => match rows.display_of(uid) {
            Some(d) => d.index(),
            None => return,
        },
        None => rows.len(),
    };
    let row_h = fc.metrics.row_h;
    let top = geom::index_to_f32(index) * row_h;
    let (offset, height) = (view.last_scroll_y, view.last_body_height);
    if top >= offset && top + row_h <= offset + height {
        return;
    }
    if let Some(y) = crate::nav::ensure_visible(offset, index, row_h, height, rows.len().saturating_add(1)) {
        view.pending_scroll_y = Some(y);
        view.last_scroll_y = y;
    }
}

/// Résultat du corps : défilement horizontal demandé et clic droit.
struct RowsOut {
    /// Défilement horizontal demandé par un glisser sur le Gantt.
    dx: f32,
    /// Clic droit sur le corps, s'il y en a eu un.
    context_click: Option<ContextClick>,
    /// Haut de la fenêtre visible, en points depuis la ligne 0.
    scroll_y: f32,
    /// Hauteur de la fenêtre visible.
    view_h: f32,
}

/// Clic droit sur le corps de la vue.
#[derive(Clone, Copy, Debug)]
struct ContextClick {
    /// Ligne affichée cliquée (`rows.len()` : la ligne vide ; `None` sous
    /// la ligne vide).
    row: Option<usize>,
    /// Tâche de la ligne cliquée (`None` sous la dernière ligne).
    uid: Option<TaskUid>,
    /// Colonne cliquée dans la table (`None` : Gantt, ou au-delà de la
    /// dernière colonne).
    column: Option<Column>,
    /// Barre cliquée : son type et sa tâche (H-UI-59, J8 lot L15).
    bar: Option<(plannercraft_render::style::BarRole, TaskUid)>,
}

/// Type et tâche de la barre sous le pointeur `p` (H-UI-59) : même
/// géométrie que le dessin et le survol (`gantt::hit_test`), type déduit
/// de la ligne (`gantt::row_role`, surbrillance du critique comprise).
#[must_use]
pub fn bar_under(
    fc: &FrameCtx<'_>,
    axis: &TimeAxis,
    origin: Pos2,
    p: Pos2,
) -> Option<(plannercraft_render::style::BarRole, TaskUid)> {
    let rows = fc.visible()?;
    let screen = fc.task_view.style.clone();
    let geo = gantt::GanttGeo::new(fc, &screen.style);
    let uid = match gantt::hit_test(rows, axis, origin, &geo, p) {
        GanttHit::Bar(uid) | GanttHit::BarStart(uid) | GanttHit::BarEnd(uid) => uid,
        GanttHit::Link(..) | GanttHit::Empty => return None,
    };
    let row = (0..rows.len()).find_map(|i| rows.row(DisplayRow::at(i)).filter(|r| r.uid == uid))?;
    let role = gantt::row_role(row, geo.highlight_critical)?;
    Some((role, uid))
}

/// Barre du dernier clic droit, gardée pendant que le menu est ouvert.
fn context_bar_id() -> Id {
    Id::unique("plannercraft.context-bar")
}

/// Réécrit les `Select(Some(uid))` poussés depuis `start` en `SelectWith`
/// si Ctrl/Cmd (ajout ou retrait) ou Maj (plage) est enfoncé.
fn apply_click_modifiers(actions: &mut [Action], start: usize, modifiers: egui::Modifiers) {
    let mode = if modifiers.command {
        SelectMode::Toggle
    } else if modifiers.shift {
        SelectMode::Range
    } else {
        return;
    };
    for action in actions.iter_mut().skip(start) {
        if let Action::Select(Some(uid)) = *action {
            *action = Action::SelectWith { uid, mode };
        }
    }
}

/// Clic sur le fond du corps : ligne `uid` du Gantt ⇒ même appui qu'une
/// cellule de la table sur cette ligne, dans la colonne active `column`
/// (relecture finale, n° 4 : un clic simple réduit toujours la sélection,
/// Maj étend, Ctrl/Cmd ajoute ou retire) ; hors d'une ligne : rien de
/// sélectionné.
fn gantt_click(uid: Option<TaskUid>, column: Column, modifiers: egui::Modifiers) -> Action {
    match uid {
        Some(uid) => Action::SelectOp(SelectOp::Press {
            row: RowRef::Item(uid),
            column,
            target: PressTarget::Cell,
            mode: table::press_mode(modifiers),
        }),
        None => Action::Select(None),
    }
}

/// Défilement vertical demandé, borné au défilement maximal : un décalage
/// au-delà ferait revenir la zone par un rebond animé sur une dizaine
/// d'images. egui borne d'abord par le maximum de l'image précédente : si le
/// contenu vient de grandir (ligne ajoutée), le reste du défilement attend
/// l'image suivante (la demande reste dans `pending_scroll_y`).
fn take_pending_scroll_y(view: &mut ViewTransient, body_h: f32, content_h: f32) -> Option<f32> {
    let y = view.pending_scroll_y.take().filter(|y| y.is_finite())?;
    let max_of = |content: f32| {
        let max = (content - body_h).max(0.0);
        if max.is_finite() { max } else { 0.0 }
    };
    let max_now = max_of(content_h);
    let max_y = if view.last_content_h > 0.0 {
        max_now.min(max_of(view.last_content_h))
    } else {
        max_now
    };
    if y > max_y && max_now > max_y {
        view.pending_scroll_y = Some(y);
    }
    Some(y.clamp(0.0, max_y))
}

/// `ScrollArea` verticale unique : corps de la table et du Gantt sur la même
/// plage de lignes, fond cliquable (sélection, glisser horizontal du Gantt).
fn paint_rows(
    ui: &mut egui::Ui,
    fc: &mut FrameCtx<'_>,
    view: &mut ViewTransient,
    layout: &MainLayout,
    axis: &TimeAxis,
) -> RowsOut {
    let row_h = fc.metrics.row_h;
    // Une ligne de plus que de tâches : la ligne vide de fin (création).
    let len = fc.visible().map_or(0, |rows| rows.len().saturating_add(1));
    let content_h = geom::index_to_f32(len) * row_h;
    let content_h = if content_h.is_finite() {
        content_h.max(0.0)
    } else {
        0.0
    };

    let mut area = egui::ScrollArea::vertical()
        .id_salt("plannercraft.rows")
        .auto_shrink([false, false])
        .content_margin(egui::Margin::ZERO)
        .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible);
    if let Some(y) = take_pending_scroll_y(view, layout.body.height(), content_h) {
        area = area.vertical_scroll_offset(y);
    }
    view.last_content_h = content_h;
    let mut body_ui = ui.new_child(egui::UiBuilder::new().max_rect(layout.body));
    // Barre verticale pleine, toujours visible (relecture B, I4) : elle
    // occupe `Metrics::vbar_w` à droite du corps, que la mise en page
    // retire au Gantt.
    body_ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
    let table_clip = layout.table_clip;
    let gantt_clip = layout.gantt_clip;
    let table_scroll_x = view.table_scroll_x;
    let edit = &mut view.edit;
    let cursor = &mut view.cursor;
    let select_drag = &mut view.select_drag;
    let row_drag = &mut view.row_drag;
    let drag = &mut view.drag;
    let out = area.show_viewport(&mut body_ui, |ui, viewport| {
        let origin = ui.max_rect().min;
        ui.set_min_size(vec2(ui.available_width().max(0.0), content_h));
        let range = geom::visible_rows(viewport.top(), viewport.bottom(), row_h, len, OVERSCAN);

        // Fond : déclaré avant les lignes, qui passent donc au-dessus.
        let background = ui.interact(
            ui.clip_rect(),
            ui.make_persistent_id("plannercraft-body-bg"),
            Sense::click_and_drag(),
        );
        let first_action = fc.actions.len();
        table::paint_body(
            ui,
            fc,
            edit,
            cursor,
            table::Drags {
                select: select_drag,
                rows: row_drag,
            },
            origin,
            range.clone(),
            table_clip,
            table_scroll_x,
        );
        gantt::paint_body(ui, fc, axis, origin, range, gantt_clip);

        let mut dx = 0.0;
        if background.clicked()
            && let Some(p) = background.interact_pointer_pos()
        {
            let in_gantt = gantt_clip.contains(p);
            let uid = if in_gantt {
                geom::row_at(p.y - origin.y, row_h, len)
                    .and_then(|i| fc.visible().and_then(|rows| rows.uid(DisplayRow::at(i))))
            } else {
                None
            };
            let modifiers = ui.input(|i| i.modifiers);
            fc.actions.push(gantt_click(uid, cursor.column, modifiers));
            // Clavier unique (ADR-0018 D8, H-UI-101) : un clic dans le Gantt
            // laisse le clavier à la table.
            fc.actions.push(Action::FocusPane(Pane::Table));
        }
        let modifiers = ui.input(|i| i.modifiers);
        apply_click_modifiers(fc.actions, first_action, modifiers);
        let context_click = secondary_click(ui, table_clip.union(gantt_clip)).map(|p| {
            let left = table_clip.left() - table_scroll_x;
            context_click_at(fc, axis, (origin, left), (table_clip, gantt_clip), p)
        });
        let from_gantt = ui
            .input(|i| i.pointer.press_origin())
            .is_some_and(|p| gantt_clip.contains(p));
        let gesture = bar_gesture(ui, fc, drag, &background, origin, axis, gantt_clip, from_gantt);
        if let Some(d) = drag.as_ref() {
            let pointer = ui.input(|i| i.pointer.interact_pos());
            gantt::paint_drag(ui, fc, axis, origin, gantt_clip, d, pointer);
        }
        gantt_double_click(fc, &background, origin, axis, gantt_clip);
        if !gesture && background.dragged_by(PointerButton::Primary) && from_gantt {
            let drag = background.drag_delta().x;
            if drag.is_finite() {
                dx -= drag;
            }
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        }
        RowsOut {
            dx,
            context_click,
            scroll_y: viewport.top(),
            view_h: viewport.height(),
        }
    });
    // Les plages dessinées sont relevées par chaque volet lui-même.
    out.inner
}

/// Largeur d'une colonne du défilement automatique horizontal de la table,
/// en points (H-UI-91).
const AUTOSCROLL_COLUMN_W: f32 = 80.0;

/// Largeur de la zone du défilement automatique horizontal de la table, à
/// l'intérieur du bord gauche ou droit, en points (H-UI-91) : étroite, pour
/// qu'un glisser vertical dans la dernière colonne visible ne fasse pas
/// défiler la table.
const AUTOSCROLL_EDGE_X: f32 = 20.0;

/// Défilement automatique pendant un glisser de la vue (ADR-0018 D1,
/// H-UI-91) : sélection dans la table (cellules, ID ; en-têtes de colonne
/// pour l'horizontal), barre ou lien du Gantt. Pointeur à moins d'une ligne
/// du bord haut ou bas du corps, ou au-delà : défilement vertical de
/// `nav::autoscroll_speed` lignes par seconde (`dt` de l'image), posé dans
/// `pending_scroll_y` et borné à `[0, contenu − corps]` ; bloc de la table
/// à moins de 20 points du bord gauche ou droit, ou au-delà : même formule
/// (`d` rapporté à ces 20 points) en colonnes de 80 points (`ScrollTableX`,
/// borné par le réducteur). Seulement dans le sens où le pointeur s'est
/// éloigné du point d'appui d'au moins le seuil de glisser ([`toward`]), et
/// pour un glisser de sélection, seulement une fois armé
/// ([`table::arm_select_drag`]). La sélection ou la cible suit
/// la ligne sous le pointeur borné au corps à l'image suivante. Une image
/// est redemandée tant que la vue défile.
fn autoscroll(ui: &egui::Ui, fc: &mut FrameCtx<'_>, view: &mut ViewTransient, layout: &MainLayout) {
    use crate::action::PressTarget;
    // Glisser de sélection pas encore armé (sous le seuil) : rien.
    let selecting = view.select_drag.filter(|d| d.armed).map(|d| d.target);
    // Barre ou lien du Gantt, déplacement de lignes par l'ID (H-UI-94) :
    // défilement vertical.
    let bar = view.drag.is_some() || view.row_drag.is_some();
    if selecting.is_none() && !bar {
        return;
    }
    let (down, pointer, origin, dt) = ui.input(|i| {
        (
            i.pointer.primary_down(),
            i.pointer.interact_pos(),
            i.pointer.press_origin(),
            i.stable_dt,
        )
    });
    let Some(p) = pointer.filter(|p| down && p.is_finite()) else {
        return;
    };
    let origin = origin.filter(|o| o.is_finite()).unwrap_or(p);
    let threshold = ui.ctx().options(|o| o.input_options.max_click_dist);
    let row_h = fc.metrics.row_h;
    let mut moving = false;
    if bar || matches!(selecting, Some(PressTarget::Cell | PressTarget::RowHeader)) {
        let body = layout.body;
        let speed = toward(
            crate::nav::autoscroll_speed(p.y, body.top(), body.bottom(), row_h),
            p.y - origin.y,
            threshold,
        );
        let max = max_scroll_x(view.last_content_h, view.last_body_height);
        if let Some(y) = crate::nav::autoscroll_offset(view.last_scroll_y, speed, row_h, dt, max) {
            view.pending_scroll_y = Some(y);
            view.last_scroll_y = y;
            moving = true;
        }
    }
    if matches!(selecting, Some(PressTarget::Cell | PressTarget::ColumnHeader)) {
        let clip = layout.table_clip;
        let speed = toward(
            crate::nav::autoscroll_speed(p.x, clip.left(), clip.right(), AUTOSCROLL_EDGE_X),
            p.x - origin.x,
            threshold,
        );
        let columns_w = super::table::content_width(ui.ctx(), fc.state);
        let max = max_scroll_x(columns_w, clip.width());
        if let Some(x) =
            crate::nav::autoscroll_offset(view.table_scroll_x, speed, AUTOSCROLL_COLUMN_W, dt, max)
        {
            fc.actions.push(Action::ScrollTableX(x - view.table_scroll_x));
            moving = true;
        }
    }
    if moving {
        ui.ctx().request_repaint();
    }
}

/// Vitesse de défilement automatique gardée seulement dans le sens où le
/// pointeur s'est éloigné du point d'appui (`moved`) d'au moins `threshold`
/// (seuil de glisser d'egui) : un glisser commencé près d'un bord (barre de
/// la première ligne visible glissée de côté) ou un glisser vertical qui
/// dérive d'un point vers la droite ne fait pas défiler la vue dans cet
/// axe.
fn toward(speed: f32, moved: f32, threshold: f32) -> f32 {
    if moved.is_finite() && speed * moved > 0.0 && moved.abs() >= threshold {
        speed
    } else {
        0.0
    }
}

/// Écart en jours du glisser `d` pour l'abscisse écran `x` : depuis le
/// point de départ (déplacer, redimensionner), depuis le début de la barre
/// (avancement, R-242). `left` : bord gauche du Gantt.
fn gesture_days(d: &BarDrag, rows: VisibleRows<'_>, axis: &TimeAxis, left: f32, x: f32) -> i64 {
    match d.kind {
        GestureKind::Move | GestureKind::Resize => axis.days_for_dx(x - left - d.origin_x),
        GestureKind::Progress => {
            let start_x = rows
                .display_of(d.uid)
                .and_then(|i| rows.row(i))
                .and_then(|row| row.bar)
                .map_or(left + d.origin_x, |(start, _)| axis.x(start));
            axis.days_for_dx(x - start_x)
        }
    }
}

/// Aperçu du glisser `d` (calendrier seul, sans recalcul) ; refus : aucun.
fn gesture_preview(doc: &OpenDocument, d: &BarDrag) -> Option<gesture::Preview> {
    gesture::preview(&doc.session, &doc.calendars(), d.uid, d.kind, d.days).ok()
}

/// Glisser d'une barre : début sur une zone de barre (H-UI-20), suivi de
/// l'écart et de la ligne survolée, lâcher ou Échap. Rend vrai tant qu'un
/// geste tient le glisser du fond (pas de défilement).
#[expect(
    clippy::too_many_arguments,
    reason = "contexte de l'image passé tel quel, comme les autres volets"
)]
fn bar_gesture(
    ui: &egui::Ui,
    fc: &mut FrameCtx<'_>,
    drag: &mut Option<BarDrag>,
    background: &egui::Response,
    origin: Pos2,
    axis: &TimeAxis,
    gantt_clip: Rect,
    from_gantt: bool,
) -> bool {
    let (Some(doc), Some(rows)) = (fc.doc, fc.visible()) else {
        *drag = None;
        return false;
    };
    let screen = fc.task_view.style.clone();
    let geo = gantt::GanttGeo::new(fc, &screen.style);
    let left = gantt_clip.left();
    let pointer = ui.input(|i| i.pointer.interact_pos());
    if drag.is_none()
        && from_gantt
        && background.drag_started_by(PointerButton::Primary)
        && let Some(p) = ui.input(|i| i.pointer.press_origin())
    {
        let start = match gantt::hit_test(rows, axis, origin, &geo, p) {
            GanttHit::Bar(uid) => Some((uid, GestureKind::Move)),
            GanttHit::BarStart(uid) => Some((uid, GestureKind::Progress)),
            GanttHit::BarEnd(uid) => Some((uid, GestureKind::Resize)),
            GanttHit::Link(..) | GanttHit::Empty => None,
        };
        if let Some((uid, kind)) = start {
            let mut d = BarDrag {
                uid,
                kind,
                origin_x: p.x - left,
                days: 0,
                over_row: link_target(rows, axis, origin, &geo, p, uid),
                preview: None,
            };
            d.days = gesture_days(&d, rows, axis, left, p.x);
            d.preview = gesture_preview(doc, &d);
            *drag = Some(d);
            // H-UI-102 : la barre d'une tâche déjà sélectionnée garde la
            // sélection (multiple comprise) ; sinon la tâche seule.
            if !fc.selected.contains(uid) {
                fc.actions.push(Action::Select(Some(uid)));
            }
            fc.actions.push(Action::FocusPane(Pane::Table));
        }
    }
    let Some(current) = drag.as_mut() else {
        if !from_gantt
            && let Some(p) = ui.ctx().pointer_hover_pos()
            && gantt_clip.contains(p)
        {
            hover_cursor(ui, gantt::hit_test(rows, axis, origin, &geo, p));
        }
        return false;
    };
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        *drag = None;
        return true;
    }
    if let Some(p) = pointer.filter(|p| p.is_finite()) {
        let days = gesture_days(current, rows, axis, left, p.x);
        if days != current.days {
            current.days = days;
            current.preview = gesture_preview(doc, current);
        }
        current.over_row = link_target(rows, axis, origin, &geo, p, current.uid);
    }
    let released = background.drag_stopped() || !ui.input(|i| i.pointer.primary_down());
    if released {
        let done = *current;
        *drag = None;
        let moved = pointer.is_some_and(|p| axis.days_for_dx(p.x - left - done.origin_x) != 0);
        drop_gesture(fc, doc, rows, &done, moved);
    } else {
        let icon = match current.kind {
            GestureKind::Move => CursorIcon::Grabbing,
            GestureKind::Resize | GestureKind::Progress => CursorIcon::ResizeHorizontal,
        };
        ui.ctx().set_cursor_icon(icon);
    }
    true
}

/// Ligne de la barre d'une **autre** tâche sous le pointeur `p` (corps,
/// bords compris), cible d'un lien (R-243 : lâcher sur la barre de B ;
/// relecture finale J7, F7 : lâcher n'importe où sur une autre ligne,
/// même dans le vide, liait au lieu de déplacer).
fn link_target(
    rows: VisibleRows<'_>,
    axis: &TimeAxis,
    origin: Pos2,
    geo: &gantt::GanttGeo<'_>,
    p: Pos2,
    from: TaskUid,
) -> Option<DisplayRow> {
    match gantt::hit_test(rows, axis, origin, geo, p) {
        GanttHit::Bar(uid) | GanttHit::BarStart(uid) | GanttHit::BarEnd(uid) if uid != from => {
            rows.display_of(uid)
        }
        _ => None,
    }
}

/// Curseur au survol d'une barre : bords redimensionnables, intérieur
/// saisissable.
fn hover_cursor(ui: &egui::Ui, hit: GanttHit) {
    match hit {
        GanttHit::BarStart(_) | GanttHit::BarEnd(_) => ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal),
        GanttHit::Bar(_) => ui.ctx().set_cursor_icon(CursorIcon::Grab),
        GanttHit::Link(..) => ui.ctx().set_cursor_icon(CursorIcon::PointingHand),
        GanttHit::Empty => {}
    }
}

/// Lâcher du glisser `d` : un seul lot (`ExecOn`), ou le refus du geste au
/// bandeau ; lien vers la barre d'une autre tâche sous le pointeur (R-243 ;
/// ailleurs, même sur une autre ligne, le geste reste un déplacement), déplacement (R-240 ; récapitulative : rien),
/// redimensionnement (R-241), avancement (R-242, seulement si le pointeur
/// a bougé d'au moins un demi-jour).
fn drop_gesture(fc: &mut FrameCtx<'_>, doc: &OpenDocument, rows: VisibleRows<'_>, d: &BarDrag, moved: bool) {
    let s = &doc.session;
    let origin_row = rows.display_of(d.uid);
    let result = match d.kind {
        GestureKind::Move => {
            if let Some(target) = d.over_row.filter(|&r| Some(r) != origin_row) {
                let batch = rows
                    .row(target)
                    .and_then(|row| gesture::link_drag(s, d.uid, row.uid).ok());
                Ok(batch)
            } else if origin_row
                .and_then(|i| rows.row(i))
                .is_some_and(|row| row.kind == RowKind::Summary)
            {
                Ok(None)
            } else {
                gesture::bar_move(s, d.uid, d.days)
            }
        }
        GestureKind::Resize => gesture::bar_resize(s, &doc.calendars(), d.uid, d.days),
        GestureKind::Progress if moved => gesture::progress_drag(s, &doc.calendars(), d.uid, d.days),
        GestureKind::Progress => Ok(None),
    };
    match result {
        Ok(Some(batch)) => fc.actions.push(Action::ExecOn {
            opening: doc.opening,
            batch,
        }),
        Ok(None) => {}
        Err(error) => fc.actions.push(Action::GestureRefused(error)),
    }
}

/// Double-clic dans le Gantt : « Informations sur la tâche » sur une barre
/// (H-UI-19), boîte « Dépendance » sur un lien (H-UI-21).
fn gantt_double_click(
    fc: &mut FrameCtx<'_>,
    background: &egui::Response,
    origin: Pos2,
    axis: &TimeAxis,
    gantt_clip: Rect,
) {
    // Un triple-clic compte aussi (un clic précédent, ailleurs, à moins du
    // délai d'egui, transforme le double-clic en triple, comme dans la table).
    if !(background.double_clicked() || background.triple_clicked()) {
        return;
    }
    let (Some(rows), Some(p)) = (fc.visible(), background.interact_pointer_pos()) else {
        return;
    };
    if !gantt_clip.contains(p) {
        return;
    }
    let screen = fc.task_view.style.clone();
    let geo = gantt::GanttGeo::new(fc, &screen.style);
    let kind = match gantt::hit_test(rows, axis, origin, &geo, p) {
        GanttHit::Bar(uid) | GanttHit::BarStart(uid) | GanttHit::BarEnd(uid) => {
            DialogKind::TaskInformation(uid)
        }
        GanttHit::Link(predecessor, successor) => DialogKind::Link {
            predecessor,
            successor,
        },
        GanttHit::Empty => return,
    };
    fc.actions.push(Action::ShowDialog(kind));
}

/// Position d'un clic droit relâché dans cette image au-dessus de `zone`, sur
/// la couche de `ui` (un clic dans un menu ou une fenêtre au-dessus ne compte
/// pas).
fn secondary_click(ui: &egui::Ui, zone: Rect) -> Option<Pos2> {
    let pos = ui.input(|i| {
        i.pointer
            .button_clicked(PointerButton::Secondary)
            .then(|| i.pointer.interact_pos())
            .flatten()
    })?;
    let on_layer = ui.ctx().layer_id_at(pos) == Some(ui.layer_id());
    (zone.contains(pos) && ui.clip_rect().contains(pos) && on_layer).then_some(pos)
}

/// Clic droit en `p` sur le corps : ligne et tâche sous le pointeur (corps
/// d'origine `origin`), colonne de la table (première colonne à
/// `cols_left`) ou barre du Gantt.
fn context_click_at(
    fc: &FrameCtx<'_>,
    axis: &TimeAxis,
    (origin, cols_left): (Pos2, f32),
    (table_clip, gantt_clip): (Rect, Rect),
    p: Pos2,
) -> ContextClick {
    let len = fc.visible().map_or(0, |rows| rows.len().saturating_add(1));
    let row = geom::row_at(p.y - origin.y, fc.metrics.row_h, len);
    ContextClick {
        row,
        uid: row.and_then(|i| fc.visible().and_then(|rows| rows.uid(DisplayRow::at(i)))),
        column: table_clip
            .contains(p)
            .then(|| table_column_at(fc, cols_left, p.x))
            .flatten(),
        bar: gantt_clip
            .contains(p)
            .then(|| bar_under(fc, axis, origin, p))
            .flatten(),
    }
}

/// Colonne visible de la table sous l'abscisse écran `x`, la première
/// colonne commençant à `left` (défilement horizontal déduit) ; `None` si
/// `x` n'est pas fini ou tombe au-delà de la dernière colonne.
fn table_column_at(fc: &FrameCtx<'_>, left: f32, x: f32) -> Option<Column> {
    if !(left.is_finite() && x.is_finite()) {
        return None;
    }
    let columns: Vec<(Column, u16)> = fc
        .state
        .visible_columns()
        .map(|state| (state.column, state.width))
        .collect();
    crate::grid::column_at(&columns, left, x)
}

/// Sélection d'un clic droit (H-UI-100, ADR-0018 D10) : dans la sélection,
/// elle est gardée ; hors d'elle, la cellule cliquée (table : l'ID donne la
/// ligne entière, comme un appui, H-UI-89) ou la ligne (Gantt) devient la
/// sélection, par le même `SelectOp::Press` qu'un appui sans modificateur.
fn context_select(fc: &FrameCtx<'_>, view: &ViewTransient, click: ContextClick) -> Option<SelectOp> {
    let rows = fc.visible()?;
    let i = click.row?;
    let row = click.uid.map_or(RowRef::Blank, RowRef::Item);
    if let Some(column) = click.column {
        let visible: Vec<Column> = fc.state.visible_columns().map(|state| state.column).collect();
        let block = crate::block::resolve(fc.selected, &view.cursor, rows, &visible);
        if block.contains(DisplayRow::at(i), column) {
            return None;
        }
        let target = if column == Column::Id {
            PressTarget::RowHeader
        } else {
            PressTarget::Cell
        };
        return Some(SelectOp::Press {
            row,
            column,
            target,
            mode: PressMode::Replace,
        });
    }
    // Gantt : la ligne, la colonne active gardée.
    let uid = click.uid?;
    (!fc.selected.contains(uid)).then_some(SelectOp::Press {
        row,
        column: view.cursor.column,
        target: PressTarget::Cell,
        mode: PressMode::Replace,
    })
}

/// Menu contextuel de la table et du Gantt (H-UI-100, ADR-0018 D10),
/// ouvert au pointeur par un clic droit sur le corps : « Mettre en forme
/// les barres » sur une barre (H-UI-59), puis les groupes de
/// [`CONTEXT_MENU`], chaque entrée étant la commande de son élément du
/// ruban (`chrome::context_item` : même action, même prédicat, même
/// raccourci). Le clic droit pose d'abord la sélection ([`context_select`])
/// et laisse le clavier à la table (H-UI-101).
fn context_menu(ui: &egui::Ui, fc: &mut FrameCtx<'_>, view: &ViewTransient, click: Option<ContextClick>) {
    let open = click.map(|click| {
        if let Some(op) = context_select(fc, view, click) {
            fc.actions.push(Action::SelectOp(op));
        }
        fc.actions.push(Action::FocusPane(Pane::Table));
        ui.ctx().data_mut(|d| d.insert_temp(context_bar_id(), click.bar));
        SetOpenCommand::Bool(true)
    });
    let bar: Option<(plannercraft_render::style::BarRole, TaskUid)> =
        ui.ctx().data(|d| d.get_temp(context_bar_id())).flatten();
    let texts = fc.texts;
    let mut format_bar = None;
    Popup::new(
        Id::unique("plannercraft.structure-menu"),
        ui.ctx().clone(),
        PopupAnchor::PointerFixed,
        ui.layer_id(),
    )
    .kind(PopupKind::Menu)
    .layout(Layout::top_down_justified(Align::Min))
    .style(menu_style)
    .open_memory(open)
    .show(|ui| {
        // Mise en forme directe (H-UI-59, J8 lot L15) : clic droit sur une
        // barre ⇒ volet du type de barre.
        if let Some((role, uid)) = bar {
            if ui.button(texts.get(TextId::ContextFormatBars)).clicked() {
                format_bar = Some((role, uid));
            }
            ui.separator();
        }
        for (i, group) in CONTEXT_MENU.iter().enumerate() {
            if i > 0 {
                ui.separator();
            }
            for cmd in *group {
                chrome::context_item(ui, fc, *cmd);
            }
        }
    });
    if let Some((role, uid)) = format_bar {
        fc.actions.push(Action::Format(crate::action::FormatOp::OpenBars {
            role,
            task: Some(uid),
        }));
    }
}

/// Menu contextuel de l'en-tête de l'échelle de temps (H-UI-100), ouvert au
/// pointeur par un clic droit sur l'en-tête du Gantt : « Échelle de temps »
/// (sous-menu des unités, celui de la liste du ruban), Zoom avant, Zoom
/// arrière, Projet entier ([`TIMESCALE_MENU`], commandes du groupe Zoom).
fn timescale_menu(ui: &egui::Ui, fc: &mut FrameCtx<'_>, clicked: bool) {
    let open = clicked.then_some(SetOpenCommand::Bool(true));
    let texts = fc.texts;
    Popup::new(
        Id::unique("plannercraft.timescale-menu"),
        ui.ctx().clone(),
        PopupAnchor::PointerFixed,
        ui.layer_id(),
    )
    .kind(PopupKind::Menu)
    .layout(Layout::top_down_justified(Align::Min))
    .style(menu_style)
    .open_memory(open)
    .show(|ui| {
        for cmd in TIMESCALE_MENU {
            if *cmd == RibbonCmd::TimescaleMenu {
                let enabled = cmd.enabled(fc);
                ui.add_enabled_ui(enabled.yes, |ui| {
                    ui.menu_button(texts.get(cmd.context_label()), |ui| {
                        chrome::dropdown_contents(ui, fc, *cmd);
                    });
                });
            } else {
                chrome::context_item(ui, fc, *cmd);
            }
        }
    });
}

/// Coins à droite de l'en-tête et de la barre horizontale du Gantt, au-dessus
/// et au-dessous de la barre verticale : même fond que l'en-tête.
pub(crate) fn paint_corners(ui: &egui::Ui, fc: &mut FrameCtx<'_>, layout: &MainLayout, avail: Rect) {
    let corners = [
        Rect::from_min_max(
            pos2(layout.gantt_header.right(), layout.gantt_header.top()),
            pos2(avail.right(), layout.gantt_header.bottom()),
        ),
        Rect::from_min_max(
            pos2(layout.gantt_hbar.right(), layout.gantt_hbar.top()),
            pos2(avail.right(), layout.gantt_hbar.bottom()),
        ),
    ];
    for corner in corners {
        if corner.is_finite() && corner.is_positive() {
            ui.painter().rect_filled(corner, 0.0, fc.palette.header_bg);
            fc.stats.shapes = fc.stats.shapes.saturating_add(1);
        }
    }
}

/// Ctrl/Cmd + molette au-dessus du Gantt : cumule `ln(zoom_delta)` et émet un
/// pas de zoom par franchissement de ±ln(1,25), ancré sous le pointeur
/// (abscisse relative au bord gauche du Gantt).
fn zoom_from_wheel(fc: &mut FrameCtx<'_>, view: &mut ViewTransient, layout: &MainLayout, input: GanttInput) {
    let d = input.zoom_delta;
    if !d.is_finite() || d <= 0.0 || (d - 1.0).abs() <= f32::EPSILON {
        return;
    }
    if !view.zoom_accum.is_finite() {
        view.zoom_accum = 0.0;
    }
    view.zoom_accum += d.ln();
    let threshold = ZOOM_STEP_FACTOR.ln();
    let steps: i8 = if view.zoom_accum >= threshold {
        1
    } else if view.zoom_accum <= -threshold {
        -1
    } else {
        return;
    };
    view.zoom_accum = 0.0;
    let clip = layout.gantt_clip;
    let anchor_x = input
        .pointer
        .map_or(0.0, |p| (p.x - clip.left()).clamp(0.0, clip.width().max(0.0)));
    let anchor_x = if anchor_x.is_finite() { anchor_x } else { 0.0 };
    fc.actions.push(Action::ZoomStep { steps, anchor_x });
}

/// Séparateur table / Gantt, glissable : `SetTableWidth` d'après la position
/// du pointeur (pas de perte d'arrondi d'une image à l'autre).
pub(crate) fn paint_splitter(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, layout: &MainLayout, avail: Rect) {
    let rect = layout.splitter;
    if !rect.is_finite() || !rect.is_positive() {
        return;
    }
    let texts = fc.texts;
    let response = ui.interact(rect, Id::unique("plannercraft.splitter"), Sense::drag());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Splitter, true, texts.get(TextId::SplitterLabel))
    });
    let response = response.on_hover_and_drag_cursor(CursorIcon::ResizeHorizontal);
    let color = if response.hovered() || response.dragged() {
        fc.palette.grid_major
    } else {
        fc.palette.splitter
    };
    ui.painter().rect_filled(rect, 0.0, color);
    fc.stats.shapes = fc.stats.shapes.saturating_add(1);
    // « Graphique seul » (H-UI-64) : largeur imposée, le séparateur ne la
    // change pas (la largeur choisie revient à la sortie du mode).
    if response.dragged() && !fc.state.chart_only {
        let splitter_w = rect.width();
        let wanted = response.interact_pointer_pos().map_or_else(
            || layout.table_clip.width() + response.drag_delta().x,
            |p| p.x - avail.left() - splitter_w * 0.5,
        );
        let width = geom::clamp_table_width(wanted, avail.width(), splitter_w).round();
        let width = u16::try_from(geom::f32_to_index(width)).unwrap_or(u16::MAX);
        if width != fc.state.table_width {
            fc.actions.push(Action::SetTableWidth(width));
        }
    }
}

/// Barre horizontale du Gantt : rend le défilement demandé, en pixels de
/// contenu.
fn paint_hbar(
    ui: &mut egui::Ui,
    fc: &mut FrameCtx<'_>,
    layout: &MainLayout,
    scroll_x: f32,
    content_w: f32,
) -> f32 {
    hbar(
        ui,
        fc,
        layout.gantt_hbar,
        &HBarSpec {
            id: Id::unique("plannercraft.gantt-hbar"),
            label: TextId::GanttScrollLabel,
            scroll: scroll_x,
            content_w,
            visible_w: layout.gantt_clip.width(),
        },
    )
}

/// Paramètres d'une barre de défilement horizontale ([`hbar`]).
#[derive(Clone, Copy)]
pub(crate) struct HBarSpec {
    /// Identifiant de la zone interactive.
    pub id: Id,
    /// Nom accessible.
    pub label: TextId,
    /// Défilement courant, en points de contenu.
    pub scroll: f32,
    /// Largeur du contenu.
    pub content_w: f32,
    /// Largeur visible.
    pub visible_w: f32,
}

/// Barre horizontale minimale (piste et curseur proportionnel), glissable ;
/// un clic sur la piste avance d'une page. Rend le défilement demandé, en
/// points de contenu (Gantt, et table au lot L6 de J7).
pub(crate) fn hbar(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, track: Rect, spec: &HBarSpec) -> f32 {
    let palette = fc.palette;
    if !track.is_finite() || !track.is_positive() {
        return 0.0;
    }
    let painter = ui.painter();
    painter.rect_filled(track, 0.0, palette.header_bg);
    painter.hline(track.x_range(), track.top() + 0.5, Stroke::new(1.0, palette.grid));
    fc.stats.shapes = fc.stats.shapes.saturating_add(2);

    let HBarSpec {
        id,
        label,
        scroll: scroll_x,
        content_w,
        visible_w,
    } = *spec;
    let max_scroll = max_scroll_x(content_w, visible_w);
    let ratio = if content_w > 0.0 && content_w.is_finite() {
        (visible_w / content_w).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let track_w = track.width();
    let thumb_w = (track_w * ratio).max(HBAR_MIN_THUMB).min(track_w);
    let free = (track_w - thumb_w).max(0.0);
    let t = if max_scroll > 0.0 {
        (scroll_x / max_scroll).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let inset = (track.height() * 0.2).min(2.0);
    let thumb = Rect::from_min_max(
        pos2(track.left() + free * t, track.top() + inset),
        pos2(track.left() + free * t + thumb_w, track.bottom() - inset),
    );

    let texts = fc.texts;
    let response = ui.interact(track, id, Sense::click_and_drag());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ScrollBar, true, texts.get(label)));
    let active = response.hovered() || response.dragged();
    if thumb.is_finite() && thumb.is_positive() && max_scroll > 0.0 {
        let color = if active {
            palette.text_weak
        } else {
            palette.grid_major
        };
        ui.painter().rect_filled(thumb, 2.0, color);
        fc.stats.shapes = fc.stats.shapes.saturating_add(1);
    }
    if max_scroll <= 0.0 {
        return 0.0;
    }
    let mut dx = 0.0;
    if response.dragged_by(PointerButton::Primary) && free > 0.0 {
        let drag = response.drag_delta().x * max_scroll / free;
        if drag.is_finite() {
            dx += drag;
        }
    } else if response.clicked()
        && let Some(p) = response.interact_pointer_pos()
    {
        if p.x < thumb.left() {
            dx -= visible_w;
        } else if p.x > thumb.right() {
            dx += visible_w;
        }
    }
    dx
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use egui::{Event, Modifiers, MouseWheelUnit, Pos2, RawInput, Rect, TouchPhase, pos2, vec2};
    use plannercraft_core::Moment;

    use crate::action::Action;
    use crate::app::{App, Env};
    use crate::i18n::{Language, Texts};
    use crate::state::{LanguagePref, ThemePref, UiState};
    use crate::theme::Palette;
    use crate::view::{DiagnosticsView, FrameCtx, FrameStats, Metrics, ViewTransient};

    fn now() -> Moment {
        "2027-01-15T08:00".parse().unwrap()
    }

    fn state() -> UiState {
        UiState {
            language: LanguagePref::Fr,
            theme: ThemePref::Light,
            ..UiState::default()
        }
    }

    fn input(mut events: Vec<Event>, modifiers: Modifiers) -> RawInput {
        events.insert(0, Event::ModifiersChanged(modifiers));
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 720.0))),
            events,
            ..RawInput::default()
        }
    }

    fn wheel(delta: egui::Vec2, modifiers: Modifiers) -> Event {
        Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta,
            phase: TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers,
        }
    }

    /// Une image de `main_view::show` seule, avec un `FrameCtx` fait main ;
    /// rend les actions émises et le relevé.
    fn frame(
        ctx: &egui::Context,
        app: &App,
        view: &mut ViewTransient,
        raw: RawInput,
    ) -> (Vec<Action>, FrameStats) {
        let texts = Texts::new(Language::Fr);
        let palette = Palette::LIGHT;
        let metrics = Metrics::default();
        let mut actions = Vec::new();
        let mut stats = FrameStats::default();
        let diagnostics = DiagnosticsView::default();
        let mut output = ctx.run_ui(raw, |ui| {
            let mut fc = FrameCtx {
                state: &app.state,
                doc: app.document.as_ref(),
                rows: app.rows(),
                row_view: Some(app.row_view()),
                texts: &texts,
                palette: &palette,
                metrics: &metrics,
                diagnostics: &diagnostics,
                selection: None,
                selected: &crate::selection::Selection::default(),
                actions: &mut actions,
                stats: &mut stats,
                window_open: false,
                task_view: crate::view::task_view::TaskViewFrame::default(),
            };
            super::show(ui, &mut fc, view);
        });
        // Aucun moteur de rendu : les textures sont ignorées.
        output.textures_delta.clear();
        (actions, stats)
    }

    fn demo_app() -> App {
        let mut app = App::new(state(), Env::fixed(now()));
        app.open_demo();
        assert!(app.document.is_some());
        app
    }

    /// Expérience R-6 : la `ScrollArea` verticale laisse l'axe x de la
    /// molette (Maj + molette) au Gantt, qui en fait un `ScrollGanttX`.
    #[test]
    fn shift_wheel_over_gantt_scrolls_x() {
        let ctx = egui::Context::default();
        let mut app = demo_app();
        // Zoom Jour : le contenu dépasse largement la fenêtre.
        app.state.zoom = crate::timescale::TimeScaleZoom::Day;
        let mut view = ViewTransient::default();
        let over_gantt = pos2(1000.0, 300.0);
        let _ = frame(
            &ctx,
            &app,
            &mut view,
            input(vec![Event::PointerMoved(over_gantt)], Modifiers::NONE),
        );
        let mut got = Vec::new();
        for _ in 0..4 {
            let events = vec![
                Event::PointerMoved(over_gantt),
                wheel(vec2(0.0, -50.0), Modifiers::SHIFT),
            ];
            let (actions, _) = frame(&ctx, &app, &mut view, input(events, Modifiers::SHIFT));
            got.extend(actions);
        }
        let dx: f32 = got
            .iter()
            .filter_map(|a| match a {
                Action::ScrollGanttX(dx) => Some(*dx),
                _ => None,
            })
            .sum();
        assert!(
            dx > 0.0,
            "Maj + molette vers le bas doit avancer le Gantt : {got:?}"
        );
    }

    /// Ctrl + molette au-dessus du Gantt : un pas de zoom vers le plus fin.
    #[test]
    fn ctrl_wheel_over_gantt_zooms() {
        let ctx = egui::Context::default();
        let app = demo_app();
        let mut view = ViewTransient::default();
        let over_gantt = pos2(1000.0, 300.0);
        let _ = frame(
            &ctx,
            &app,
            &mut view,
            input(vec![Event::PointerMoved(over_gantt)], Modifiers::NONE),
        );
        let mut got = Vec::new();
        for _ in 0..4 {
            let events = vec![
                Event::PointerMoved(over_gantt),
                wheel(vec2(0.0, 200.0), Modifiers::COMMAND),
            ];
            let (actions, _) = frame(&ctx, &app, &mut view, input(events, Modifiers::COMMAND));
            got.extend(actions);
        }
        // Ancre relative au bord gauche du Gantt (relecture B, I2).
        let expected = over_gantt.x - view.last_gantt_left;
        assert!(expected > 0.0 && expected < view.last_gantt_width);
        assert!(
            got.iter()
                .any(|a| matches!(a, Action::ZoomStep { steps: 1, anchor_x }
                if (*anchor_x - expected).abs() < 0.5)),
            "{got:?}"
        );
    }

    /// Les deux volets, qui relèvent chacun leur plage, dessinent les mêmes
    /// lignes (modèle de lignes réel de l'exemple), en haut puis après un
    /// défilement imposé.
    #[test]
    fn panes_share_row_range() {
        let ctx = egui::Context::default();
        let app = demo_app();
        let mut view = ViewTransient::default();
        let (_, stats) = frame(&ctx, &app, &mut view, input(Vec::new(), Modifiers::NONE));
        assert!(!stats.gantt_rows.is_empty(), "{stats:?}");
        assert_eq!(stats.gantt_rows.start, 0);
        assert_eq!(stats.table_rows, stats.gantt_rows);
        assert!(view.last_gantt_width > 0.0);
        view.pending_scroll_y = Some(20.0 * crate::view::ROW_H);
        let _ = frame(&ctx, &app, &mut view, input(Vec::new(), Modifiers::NONE));
        let (_, stats) = frame(&ctx, &app, &mut view, input(Vec::new(), Modifiers::NONE));
        assert!(stats.gantt_rows.start > 0, "{stats:?}");
        assert_eq!(stats.table_rows, stats.gantt_rows);
    }
}
