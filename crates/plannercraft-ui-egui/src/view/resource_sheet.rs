//! Feuille des ressources (J7, lot L10 ; `ViewKind::ResourceSheet`, menu
//! Affichage) : grille commune ([`crate::grid`]) sur les lignes de
//! [`ResourceRows`].
//!
//! - Navigation au clavier de la table (H-UI-13 à H-UI-17) : le réducteur
//!   appelle [`ResourceSheetState::navigate`] ; la feuille prend le focus
//!   de la table (`table::table_id`), une seule grille étant affichée.
//! - Saisie : F2, double-clic ou frappe directe ; la ligne vide crée une
//!   ressource (`resource.add`) ; la colonne Calendrier s'édite dans une
//!   liste (↑/↓, Entrée, Échap, clic).
//! - Suppr : `resource.delete`, confirmé par une fenêtre si le registre
//!   refuse une ressource affectée (`resource.has_assignments`).
//! - Ressource surutilisée (R-186) : indicateur « ! » et ligne en rouge
//!   (H-UI-23) ; taux dans leur unité d'origine (H-UI-24).
//!
//! La vue ne modifie que son état transitoire ([`ResourceSheetState`]) et
//! pousse des `Action` (lots `ExecOn`, `Nav`, `FocusPane`).

use egui::scroll_area::ScrollBarVisibility;
use egui::{Id, Key, Modifiers, Order, Pos2, Rect, Response, Sense, Stroke, pos2, vec2};
use plannercraft_core::ResourceUid;

use super::table::{NAV_FOCUS_FILTER, focus_elsewhere, table_id, take_typed};
use super::{FrameCtx, OVERSCAN, Pane};
use crate::action::Action;
use crate::app::OpenDocument;
use crate::edit::text_language;
use crate::geom::{self, index_to_f32};
use crate::grid::{
    self, CellEdit, EditorSpec, Finish, GridEdit, HeaderPlace, Move, RowPlace, Sheet, cell_font,
};
use crate::i18n::{Arg, TextId, Texts};
use crate::nav::{self, CellCursor, NavMove, RowRef};
use crate::resource_rows::{self, ResourceColumn, ResourceRows, ResourceSheet};
use crate::theme::Palette;

/// Curseur de la feuille des ressources.
pub type ResourceCursor = CellCursor<ResourceUid, ResourceColumn>;

/// Suppression d'une ressource affectée, en attente de confirmation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingResourceDelete {
    /// Ressource.
    pub uid: ResourceUid,
    /// Nom affiché.
    pub name: String,
    /// Affectations retirées avec elle (compte du registre).
    pub count: usize,
    /// Clé du document à la demande : une édition entre-temps rend la
    /// demande caduque.
    pub cache_key: (u64, u64),
}

/// État transitoire de la feuille des ressources (jamais sérialisé).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResourceSheetState {
    /// Cellule courante ; sa ligne est la ressource sélectionnée.
    pub cursor: ResourceCursor,
    /// Saisie en cours.
    pub edit: GridEdit<ResourceUid, ResourceColumn>,
    /// Lignes et textes, reconstruits quand le document ou la langue change.
    pub rows: ResourceRows,
    /// Suppression en attente de confirmation.
    pub pending_delete: Option<PendingResourceDelete>,
    /// Défilement vertical à imposer une fois.
    pub pending_scroll_y: Option<f32>,
    /// Défilement vertical à l'image précédente.
    pub last_scroll_y: f32,
    /// Hauteur visible du corps à l'image précédente.
    pub last_body_height: f32,
    /// Entrée surlignée de la liste des calendriers (saisie de la colonne
    /// Calendrier).
    pub list_index: usize,
}

impl ResourceSheetState {
    /// Reconstruit les lignes si le document ou la langue a changé ; une
    /// ressource disparue sous le curseur le ramène sur la ligne de même
    /// indice (la ligne vide au-delà).
    pub fn ensure_rows(&mut self, doc: &OpenDocument, texts: &Texts) {
        if self.rows.is_current(doc, texts.language()) {
            return;
        }
        let before = match self.cursor.row {
            RowRef::Item(uid) => self.rows.index_of(uid),
            RowRef::Blank => None,
        };
        self.rows = ResourceRows::build(doc, texts);
        if let RowRef::Item(uid) = self.cursor.row
            && self.rows.index_of(uid).is_none()
        {
            let sheet = ResourceSheet::new(&self.rows, texts);
            self.cursor.row = before.map_or(RowRef::Blank, |i| grid::row_ref(&sheet, i));
        }
    }

    /// Déplacement au clavier (H-UI-13 à H-UI-15) ; le défilement suit le
    /// curseur avec une ligne de marge (H-UI-17). Hypothèse : Maj+flèches
    /// n'étendent pas la sélection dans la feuille (une ressource à la fois
    /// en J7).
    pub fn navigate(&mut self, doc: &OpenDocument, texts: &Texts, mv: NavMove, row_h: f32) {
        self.ensure_rows(doc, texts);
        let sheet = ResourceSheet::new(&self.rows, texts);
        let page = nav::page_rows(self.last_body_height, row_h);
        let target = grid::nav_target(&sheet, &self.cursor, mv, page);
        let index = grid::row_index(&sheet, target.row).unwrap_or(0);
        self.cursor = target;
        let len = sheet.len().saturating_add(1);
        if let Some(y) = nav::ensure_visible(self.last_scroll_y, index, row_h, self.last_body_height, len) {
            self.pending_scroll_y = Some(y);
            self.last_scroll_y = y;
        }
    }

    /// Abandonne une confirmation devenue caduque (document modifié, fermé
    /// ou remplacé).
    pub fn drop_stale_delete(&mut self, current: Option<(u64, u64)>) {
        if self
            .pending_delete
            .as_ref()
            .is_some_and(|pending| Some(pending.cache_key) != current)
        {
            self.pending_delete = None;
        }
    }
}

/// Dessine la feuille des ressources dans la place restante de `ui`.
pub fn show(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, rs: &mut ResourceSheetState) {
    let avail = ui.available_rect_before_wrap();
    let Some(doc) = fc.doc else {
        ui.advance_cursor_after_rect(avail);
        return;
    };
    rs.ensure_rows(doc, fc.texts);
    let header = Rect::from_min_size(avail.min, vec2(avail.width(), fc.metrics.header_h));
    let body = Rect::from_min_max(pos2(avail.left(), header.bottom()), avail.max);
    paint_header(ui, fc, header);
    paint_body(ui, fc, rs, body);
    ui.advance_cursor_after_rect(avail);
}

/// En-tête : fond, titres, séparateurs et trait bas.
fn paint_header(ui: &egui::Ui, fc: &mut FrameCtx<'_>, rect: Rect) {
    if !rect.is_finite() || !rect.is_positive() {
        return;
    }
    let clip = rect.intersect(ui.clip_rect());
    let painter = ui.painter().with_clip_rect(clip);
    let palette = fc.palette;
    let texts = fc.texts;
    painter.rect_filled(rect, 0.0, palette.header_bg);
    let (_, titles) = grid::paint_titles(
        &painter,
        &resource_rows::COLUMNS,
        |column: ResourceColumn| texts.get(column.title()),
        palette,
        HeaderPlace {
            rect,
            left: rect.left(),
            pad: fc.metrics.pad_x,
        },
        &cell_font(ui),
    );
    painter.hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0, palette.grid_major),
    );
    fc.stats.shapes = fc.stats.shapes.saturating_add(titles.saturating_add(2));
}

/// Corps virtualisé : lignes visibles seulement, ligne vide de fin.
fn paint_body(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, rs: &mut ResourceSheetState, body: Rect) {
    let row_h = fc.metrics.row_h;
    let len = rs.rows.len().saturating_add(1);
    let content_h = index_to_f32(len) * row_h;
    let content_h = if content_h.is_finite() {
        content_h.max(0.0)
    } else {
        0.0
    };
    let mut area = egui::ScrollArea::vertical()
        .id_salt("plannercraft.resources")
        .auto_shrink([false, false])
        .content_margin(egui::Margin::ZERO)
        .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible);
    if let Some(y) = rs.pending_scroll_y.take().filter(|y| y.is_finite()) {
        let max = (content_h - body.height()).max(0.0);
        area = area.vertical_scroll_offset(y.clamp(0.0, if max.is_finite() { max } else { 0.0 }));
    }
    let mut body_ui = ui.new_child(egui::UiBuilder::new().max_rect(body));
    let (scroll_y, view_h) = area
        .show_viewport(&mut body_ui, |ui, viewport| {
            let origin = ui.max_rect().min;
            ui.set_min_size(vec2(ui.available_width().max(0.0), content_h));
            let range = geom::visible_rows(viewport.top(), viewport.bottom(), row_h, len, OVERSCAN);
            paint_rows(ui, fc, rs, origin, range);
            (viewport.top(), viewport.height())
        })
        .inner;
    rs.last_scroll_y = scroll_y;
    rs.last_body_height = view_h;
}

/// Lignes `range` de la grille, curseur et saisie.
fn paint_rows(
    ui: &mut egui::Ui,
    fc: &mut FrameCtx<'_>,
    rs: &mut ResourceSheetState,
    origin: Pos2,
    range: std::ops::Range<usize>,
) {
    let clip = ui.clip_rect();
    if !clip.is_finite() || !clip.is_positive() || !origin.x.is_finite() || !origin.y.is_finite() {
        return;
    }
    let ResourceSheetState {
        cursor,
        edit,
        rows,
        list_index,
        ..
    } = rs;
    let sheet = ResourceSheet::new(rows, fc.texts);
    let painter = ui.painter().with_clip_rect(clip);
    let palette = fc.palette;
    let metrics = fc.metrics;
    let row_h = metrics.row_h;
    let font = cell_font(ui);
    let mut shapes = 1_usize;
    painter.rect_filled(clip, 0.0, palette.background);
    let zebra = palette.background.lerp_to_gamma(palette.header_bg, 0.6);
    focus_widget(ui, fc, clip);
    open_from_keys(ui, fc, &sheet, edit, cursor, list_index);
    let blank = sheet.len();
    let current = grid::row_index(&sheet, cursor.row);
    let cols_left = clip.left();
    // Clic pendant une saisie : validée d'abord (H-UI-86).
    let mut deferred = None;
    for i in range.clone() {
        let top = origin.y + index_to_f32(i) * row_h;
        let row_rect = Rect::from_min_size(pos2(clip.left(), top), vec2(clip.width(), row_h));
        if !row_rect.is_finite() || (i > blank) {
            continue;
        }
        let selected = current == Some(i);
        if selected {
            painter.rect_filled(row_rect, 0.0, palette.selection);
            shapes = shapes.saturating_add(1);
        } else if !i.is_multiple_of(2) {
            painter.rect_filled(row_rect, 0.0, zebra);
            shapes = shapes.saturating_add(1);
        }
        painter.hline(
            row_rect.x_range(),
            row_rect.bottom() - 0.5,
            Stroke::new(1.0, palette.grid),
        );
        shapes = shapes.saturating_add(1);
        if i < blank {
            let at = RowPlace {
                rect: row_rect,
                cols_left,
                pad: metrics.pad_x,
                indent: 0.0,
            };
            shapes = shapes.saturating_add(grid::paint_cells(&painter, &sheet, palette, i, at, &font));
        }
        let hit = row_rect.intersect(clip);
        if hit.is_positive() {
            let response = ui.interact(
                hit,
                ui.make_persistent_id(("plannercraft-resource-row", i)),
                Sense::click(),
            );
            let texts = fc.texts;
            response.widget_info(|| {
                let label = match sheet.rows().row(i) {
                    Some(row) => texts.get_args(
                        TextId::ResRowLabel,
                        &[
                            ("id", Arg::Count(i64::try_from(row.id).unwrap_or(i64::MAX))),
                            ("name", Arg::Str(sheet.text(i, ResourceColumn::Name).into())),
                        ],
                    ),
                    None => texts.get(TextId::ResBlankRowLabel),
                };
                egui::WidgetInfo::selected(egui::Role::Row, true, selected, label)
            });
            deferred =
                row_clicked(fc, &sheet, edit, cursor, list_index, &response, i, cols_left).or(deferred);
        }
    }
    shapes = shapes.saturating_add(grid::paint_column_lines(
        &painter,
        sheet.columns(),
        cols_left,
        clip,
        palette.grid,
    ));
    if edit.editing.is_none()
        && let Some(index) = current.filter(|i| range.contains(i))
    {
        shapes = shapes.saturating_add(paint_cursor(ui, &painter, fc, &sheet, cursor, origin, index));
    }
    fc.stats.shapes = fc.stats.shapes.saturating_add(shapes);
    let at = EditAt {
        origin,
        range: &range,
        font: &font,
        click: deferred,
    };
    edit_cell(ui, fc, &sheet, edit, cursor, list_index, &at);
}

/// Cellule cliquée pendant une saisie : (ligne, colonne sous le pointeur),
/// rejointe seulement si la saisie est acceptée (H-UI-86).
type EditClick = (RowRef<ResourceUid>, Option<ResourceColumn>);

/// Place de la saisie dans l'image.
struct EditAt<'r> {
    /// Coin haut-gauche écran de la ligne 0.
    origin: Pos2,
    /// Lignes dessinées.
    range: &'r std::ops::Range<usize>,
    /// Police des cellules.
    font: &'r egui::FontId,
    /// Clic sur une cellule pendant la saisie.
    click: Option<EditClick>,
}

/// Widget focalisable de la feuille (identifiant de la table : la
/// navigation au clavier du contexte `Table` s'y applique).
fn focus_widget(ui: &egui::Ui, fc: &FrameCtx<'_>, clip: Rect) {
    let response = ui.interact(clip, table_id(), Sense::focusable_noninteractive());
    let texts = fc.texts;
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Table, true, texts.get(TextId::ResSheetLabel)));
    if response.has_focus() {
        ui.memory_mut(|memory| memory.set_focus_lock_filter(table_id(), NAV_FOCUS_FILTER));
    }
}

/// Cadre et nœud d'accessibilité de la cellule courante (ligne `index`).
fn paint_cursor(
    ui: &egui::Ui,
    painter: &egui::Painter,
    fc: &FrameCtx<'_>,
    sheet: &ResourceSheet<'_>,
    cursor: &ResourceCursor,
    origin: Pos2,
    index: usize,
) -> usize {
    let Some((left, w)) = grid::column_span(sheet.columns(), painter.clip_rect().left(), cursor.column)
    else {
        return 0;
    };
    let top = origin.y + index_to_f32(index) * fc.metrics.row_h;
    let rect = Rect::from_min_size(pos2(left, top), vec2(w, fc.metrics.row_h));
    if !grid::paint_cursor_frame(painter, rect, fc.palette) {
        return 0;
    }
    let hit = rect.intersect(painter.clip_rect());
    if hit.is_positive() {
        let response = ui.interact(hit, Id::unique("plannercraft.resources.cursor"), Sense::hover());
        let texts = fc.texts;
        let column = texts.get(cursor.column.title());
        response.widget_info(|| {
            let label = match sheet.rows().row(index) {
                Some(row) => {
                    let value = sheet.text(index, cursor.column);
                    let value = if value.is_empty() {
                        texts.get(TextId::NavCursorEmptyValue)
                    } else {
                        value.to_owned()
                    };
                    texts.get_args(
                        TextId::NavCursorLabel,
                        &[
                            ("id", Arg::Count(i64::try_from(row.id).unwrap_or(i64::MAX))),
                            ("column", Arg::Str(column.clone().into())),
                            ("value", Arg::Str(value.into())),
                        ],
                    )
                }
                None => texts.get_args(
                    TextId::NavCursorBlankLabel,
                    &[("column", Arg::Str(column.clone().into()))],
                ),
            };
            egui::WidgetInfo::labeled(egui::Role::Cell, true, label)
        });
    }
    1
}

/// Clic sur la ligne `i` : curseur (ligne, colonne sous le pointeur), focus
/// de la feuille ; double-clic : saisie de la cellule. Saisie ouverte : le
/// clic est rendu à [`edit_cell`], qui valide d'abord (H-UI-86).
#[allow(
    clippy::too_many_arguments,
    reason = "emprunts disjoints de l'état de la feuille"
)]
fn row_clicked(
    fc: &mut FrameCtx<'_>,
    sheet: &ResourceSheet<'_>,
    edit: &mut GridEdit<ResourceUid, ResourceColumn>,
    cursor: &mut ResourceCursor,
    list_index: &mut usize,
    response: &Response,
    i: usize,
    left: f32,
) -> Option<EditClick> {
    if !response.clicked() {
        return None;
    }
    let row = grid::row_ref(sheet, i);
    let column = response
        .interact_pointer_pos()
        .and_then(|p| grid::column_at(sheet.columns(), left, p.x));
    if edit.editing.is_some() {
        return Some((row, column));
    }
    cursor.row = row;
    if let Some(column) = column {
        cursor.column = column;
    }
    fc.actions.push(Action::FocusPane(Pane::Table));
    if (response.double_clicked() || response.triple_clicked()) && !fc.window_open {
        open_editor(sheet, edit, cursor, list_index, fc.doc);
    }
    None
}

/// Ouvre la saisie de la cellule courante avec son texte (F2, double-clic) ;
/// rien sur une colonne calculée.
fn open_editor(
    sheet: &ResourceSheet<'_>,
    edit: &mut GridEdit<ResourceUid, ResourceColumn>,
    cursor: &ResourceCursor,
    list_index: &mut usize,
    doc: Option<&OpenDocument>,
) {
    if !sheet.editable(cursor.column) {
        return;
    }
    let text = grid::row_index(sheet, cursor.row)
        .map_or_else(String::new, |i| sheet.text(i, cursor.column).to_owned());
    if cursor.column == ResourceColumn::Calendar
        && let Some(doc) = doc
    {
        *list_index = calendar_list(doc)
            .iter()
            .position(|(name, _)| *name == text)
            .unwrap_or(0);
    }
    edit.start(CellEdit::open(cursor.row, cursor.column, text));
}

/// F2 ou frappe d'un caractère, hors autre widget focalisé et menu ouvert :
/// saisie de la cellule courante (la frappe remplace la valeur ; elle
/// n'ouvre pas la liste des calendriers).
fn open_from_keys(
    ui: &egui::Ui,
    fc: &FrameCtx<'_>,
    sheet: &ResourceSheet<'_>,
    edit: &mut GridEdit<ResourceUid, ResourceColumn>,
    cursor: &ResourceCursor,
    list_index: &mut usize,
) {
    let ctx = ui.ctx();
    if edit.editing.is_some()
        || fc.window_open
        || focus_elsewhere(ctx)
        || ctx.any_popup_open()
        || fc.doc.is_none()
    {
        return;
    }
    if !sheet.editable(cursor.column) {
        return;
    }
    if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::F2)) {
        open_editor(sheet, edit, cursor, list_index, fc.doc);
        return;
    }
    if cursor.column == ResourceColumn::Calendar {
        return;
    }
    // Retour arrière : saisie vide (comme la table des tâches ; Cmd+delete
    // supprime la ressource, relecture finale J7, F6).
    if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Backspace)) {
        edit.start(CellEdit::typed(cursor.row, cursor.column, ""));
        return;
    }
    let typed = take_typed(ctx);
    if !typed.is_empty() {
        edit.start(CellEdit::typed(cursor.row, cursor.column, &typed));
    }
}

/// Entrées de la liste des calendriers : (nom, libellé affiché), « aucun »
/// d'abord.
fn calendar_list(doc: &OpenDocument) -> Vec<(String, Option<String>)> {
    std::iter::once((String::new(), None))
        .chain(
            resource_rows::calendar_choices(doc.project())
                .into_iter()
                .map(|(_, name)| (name.clone(), Some(name))),
        )
        .collect()
}

/// Saisie ouverte : zone de texte (ou liste des calendriers) posée sur la
/// cellule, refus dessous, fin de saisie comme dans la table (Entrée, Tab,
/// Échap, défilement, perte du focus).
fn edit_cell(
    ui: &mut egui::Ui,
    fc: &mut FrameCtx<'_>,
    sheet: &ResourceSheet<'_>,
    edit: &mut GridEdit<ResourceUid, ResourceColumn>,
    cursor: &mut ResourceCursor,
    list_index: &mut usize,
    at: &EditAt<'_>,
) {
    let (origin, range) = (at.origin, at.range);
    let Some(state) = edit.editing.as_ref() else {
        return;
    };
    let index = grid::row_index(sheet, state.row);
    let span = grid::column_span(sheet.columns(), ui.clip_rect().left(), state.column);
    let (Some(index), Some((cell_left, cell_w)), Some(doc)) = (index, span, fc.doc) else {
        close_editor(ui, edit);
        return;
    };
    let top = origin.y + index_to_f32(index) * fc.metrics.row_h;
    let rect = Rect::from_min_size(pos2(cell_left, top), vec2(cell_w, fc.metrics.row_h));
    let list = state.column == ResourceColumn::Calendar;
    let mut finish = if list {
        calendar_keys(ui, list_index, calendar_list(doc).len())
    } else {
        grid::finish_keys(ui)
    };
    if finish.is_none() && (at.click.is_some() || grid::scrolled(ui)) {
        finish = Some(Finish::Commit(Move::Stay));
    }
    if range.contains(&index) {
        if list {
            if let Some(picked) = show_calendar_list(ui, fc, doc, edit, *list_index, rect) {
                *list_index = picked;
                finish = Some(Finish::Commit(Move::Stay));
            }
        } else if show_text_editor(ui, fc, sheet, edit, rect, (index, at.font)) && finish.is_none() {
            finish = Some(Finish::Commit(Move::Stay));
        }
    }
    if list
        && let Some(Finish::Commit(_)) = finish
        && let Some(state) = edit.editing.as_mut()
        && let Some((name, _)) = calendar_list(doc).into_iter().nth(*list_index)
    {
        state.buffer = name;
    }
    match finish {
        // Ctrl/Cmd+Entrée (H-UI-98) : aucun bloc dans la feuille des
        // ressources (ADR-0018, reporté) ; la saisie reste ouverte.
        None | Some(Finish::FillTyped) => {}
        Some(Finish::Cancel) => {
            if let Some(state) = edit.editing.as_ref() {
                cursor.row = state.row;
                cursor.column = state.column;
            }
            close_editor(ui, edit);
            fc.actions.push(Action::FocusPane(Pane::Table));
        }
        Some(Finish::Commit(motion)) => commit(ui, fc, sheet, edit, cursor, motion, at.click),
    }
}

/// Touches de la liste des calendriers : ↑/↓ changent l'entrée surlignée
/// (bornée), Entrée valide, Échap abandonne ; aucune autre touche ne sort
/// de la liste.
fn calendar_keys(ui: &egui::Ui, list_index: &mut usize, len: usize) -> Option<Finish> {
    ui.input_mut(|input| {
        if input.consume_key(Modifiers::NONE, Key::Escape) {
            return Some(Finish::Cancel);
        }
        if input.consume_key(Modifiers::NONE, Key::Enter) {
            return Some(Finish::Commit(Move::Down));
        }
        if input.consume_key(Modifiers::NONE, Key::ArrowDown) {
            *list_index = list_index.saturating_add(1).min(len.saturating_sub(1));
        }
        if input.consume_key(Modifiers::NONE, Key::ArrowUp) {
            *list_index = list_index.saturating_sub(1);
        }
        None
    })
}

/// Liste des calendriers sous la cellule `rect` ; rend l'entrée cliquée.
fn show_calendar_list(
    ui: &egui::Ui,
    fc: &FrameCtx<'_>,
    doc: &OpenDocument,
    edit: &GridEdit<ResourceUid, ResourceColumn>,
    list_index: usize,
    rect: Rect,
) -> Option<usize> {
    let texts = fc.texts;
    let entries = calendar_list(doc);
    let error = edit.editing.as_ref().and_then(|state| state.error.clone());
    let mut picked = None;
    egui::Area::new(Id::unique("plannercraft.resources.calendar-list"))
        .order(Order::Foreground)
        .fixed_pos(pos2(rect.left(), rect.bottom()))
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(rect.width());
                egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                    for (i, (_, label)) in entries.iter().enumerate() {
                        let text = label
                            .clone()
                            .unwrap_or_else(|| texts.get(TextId::ResCalendarNone));
                        if ui.selectable_label(i == list_index, text).clicked() {
                            picked = Some(i);
                        }
                    }
                });
                if let Some(message) = &error {
                    ui.label(egui::RichText::new(message.clone()).color(fc.palette.critical));
                }
            });
        });
    picked
}

/// Zone de saisie de texte et refus dessous ; rend vrai si elle vient de
/// perdre le focus.
fn show_text_editor(
    ui: &mut egui::Ui,
    fc: &FrameCtx<'_>,
    sheet: &ResourceSheet<'_>,
    edit: &mut GridEdit<ResourceUid, ResourceColumn>,
    rect: Rect,
    (index, font): (usize, &egui::FontId),
) -> bool {
    let GridEdit {
        editing,
        focus_pending,
    } = edit;
    let Some(state) = editing.as_mut() else {
        return false;
    };
    let label = fc.texts.get_args(
        TextId::CellEditLabel,
        &[("column", Arg::Str(fc.texts.get(state.column.title()).into()))],
    );
    let look = grid::EditorLook::of(
        sheet,
        (index, state.column),
        fc.palette,
        font.clone(),
        fc.metrics.pad_x,
        0.0,
        rect.width(),
    );
    let spec = EditorSpec {
        rect,
        clip: ui.clip_rect(),
        label,
        look,
    };
    let out = grid::show_editor(ui, &mut state.buffer, focus_pending, &spec);
    if out.changed {
        state.error = None;
    }
    if let Some(error) = &state.error {
        grid::show_cell_error(ui.ctx(), rect, error.clone(), fc.palette.critical);
    }
    out.lost
}

/// Ferme la saisie et rend le focus.
fn close_editor(ui: &egui::Ui, edit: &mut GridEdit<ResourceUid, ResourceColumn>) {
    edit.editing = None;
    edit.focus_pending = false;
    grid::surrender_editor(ui);
}

/// Valide la saisie : refus → message, saisie gardée ; accord → lot poussé
/// (s'il y en a un), saisie fermée, curseur posé sur la cellule éditée puis
/// déplacé (`Action::Nav`, après le lot).
fn commit(
    ui: &egui::Ui,
    fc: &mut FrameCtx<'_>,
    sheet: &ResourceSheet<'_>,
    edit: &mut GridEdit<ResourceUid, ResourceColumn>,
    cursor: &mut ResourceCursor,
    motion: Move,
    click: Option<EditClick>,
) {
    let Some(doc) = fc.doc else {
        close_editor(ui, edit);
        return;
    };
    let Some(state) = edit.editing.as_mut() else {
        return;
    };
    let not_editable = fc.texts.get(TextId::EditErrorNotEditable);
    let lang = text_language(fc.texts.language());
    match grid::commit_cell(sheet, state, &doc.session, lang, &not_editable) {
        Ok(batch) => {
            let (row, column) = (state.row, state.column);
            close_editor(ui, edit);
            if let Some(batch) = batch {
                fc.actions.push(Action::ExecOn {
                    opening: doc.opening,
                    batch,
                });
            }
            cursor.row = row;
            cursor.column = column;
            if let Some(mv) = motion.nav() {
                fc.actions.push(Action::Nav { mv, extend: false });
            }
            if let Some((row, column)) = click {
                cursor.row = row;
                if let Some(column) = column {
                    cursor.column = column;
                }
            }
            fc.actions.push(Action::FocusPane(Pane::Table));
        }
        Err(message) => {
            state.error = Some(message);
            edit.focus_pending = true;
        }
    }
}

/// Fenêtre de confirmation de la suppression d'une ressource affectée :
/// « Supprimer » pousse `ConfirmDelete(true)` ; « Annuler », Échap ou un
/// clic hors de la fenêtre `ConfirmDelete(false)`.
pub fn confirm_delete_modal(
    ctx: &egui::Context,
    texts: &Texts,
    palette: &Palette,
    pending: &PendingResourceDelete,
    actions: &mut Vec<Action>,
) {
    let count = i64::try_from(pending.count).unwrap_or(i64::MAX);
    let response = egui::Modal::new(Id::unique("plannercraft.confirm-resource-delete")).show(ctx, |ui| {
        ui.set_max_width(420.0);
        ui.heading(texts.get(TextId::ResDeleteTitle));
        ui.label(texts.get_args(
            TextId::ResDeleteText,
            &[
                ("name", Arg::Str(pending.name.clone().into())),
                ("count", Arg::Count(count)),
            ],
        ));
        ui.add_space(8.0);
        let mut answer = None;
        ui.horizontal(|ui| {
            let yes = egui::Button::new(
                egui::RichText::new(texts.get(TextId::ConfirmDeleteYes)).color(palette.critical),
            );
            if ui.add(yes).clicked() {
                answer = Some(true);
            }
            if ui.button(texts.get(TextId::ConfirmDeleteNo)).clicked() {
                answer = Some(false);
            }
        });
        answer
    });
    match response.inner {
        Some(answer) => actions.push(Action::ConfirmDelete(answer)),
        None if response.should_close() => actions.push(Action::ConfirmDelete(false)),
        None => {}
    }
}
