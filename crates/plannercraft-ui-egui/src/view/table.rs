//! Table des tâches dessinée au `Painter` (en-tête et corps virtualisé).
//!
//! Seules les lignes de la plage visible sont dessinées ; chaque ligne déclare
//! un nœud d'accessibilité (`row-label`) pour être retrouvée par les tests.
//! Les textes viennent tout formatés du `RowModel`, lus à travers la vue
//! des lignes (`VisibleRows`, J8 lot L5 : indice de ligne = ligne affichée) :
//! aucune mise en forme ni
//! aucun parcours proportionnel au nombre de tâches pendant l'image. Chaque
//! cellule est une galley du cache d'egui (`LayoutJob` tronqué par « … »),
//! colorée au dessin (`Color32::PLACEHOLDER`), donc partagée entre les thèmes.

use std::ops::Range;

use egui::{
    Event, EventFilter, Id, Key, Modifiers, Painter, Pos2, Rect, Response, Sense, Stroke, pos2, vec2,
};
use plannercraft_automation::text::Language as TextLanguage;
use plannercraft_automation::{Batch, Session};
use plannercraft_core::TaskUid;
use plannercraft_render::style::RowBanding;

use super::main_view::HBarSpec;
use super::task_view::TaskViewOp;
use super::{FrameCtx, Pane, RowDrag, SelectDrag};
use crate::action::{Action, CellOp, ColumnOp, DialogKind, PressMode, PressTarget, SelectOp};
use crate::column::{CellAlign, Column};
use crate::edit::{EditState, TableEdit, commit_edit, editable, initial_text, text_language};
use crate::geom::index_to_f32;
pub use crate::grid::scroll_to_column;
use crate::grid::{
    self, CellStyle, EditorSpec, Finish, HeaderPlace, Move, RowPlace, Sheet, cell_font, column_at,
    column_span, finish_keys,
};
#[cfg(test)]
use crate::grid::{NAME_MIN_TEXT_ROOM, name_indent};
use crate::i18n::{Arg, TextId, Texts};
use crate::nav::{CellCursor, InBlockMove, NavMove, RowRef};
use crate::row_view::{DisplayRow, VisibleRows};
use crate::rows::{Row, RowKind};
use crate::state::{SortKey, SortSpec, UiState};
use crate::theme::Palette;

/// Curseur de la table (cellule courante).
pub type TableCursor = CellCursor<TaskUid, Column>;

/// Identifiant focalisable de la table (J7, lot L1) : tant qu'il a le
/// focus clavier, les raccourcis de navigation (`KeyContext::Table`) sont
/// actifs. La feuille des ressources (lot L10) prend le même identifiant :
/// une seule grille est affichée à la fois.
#[must_use]
pub fn table_id() -> Id {
    Id::unique("plannercraft.table")
}

/// Verrou de focus de la table, rappelé à chaque image : Tab et les
/// flèches restent à la table (egui les traite en début d'image, avant
/// l'application) ; Échap rend le focus (ADR-0015 D2).
pub(crate) const NAV_FOCUS_FILTER: EventFilter = EventFilter {
    tab: true,
    horizontal_arrows: true,
    vertical_arrows: true,
    escape: false,
};

/// Vrai si un autre widget que la table a le focus clavier (zone de
/// saisie, bouton parcouru au clavier) : la frappe ne s'adresse pas à la
/// table.
pub(crate) fn focus_elsewhere(ctx: &egui::Context) -> bool {
    ctx.memory(|memory| memory.focused().is_some_and(|id| id != table_id()))
}

/// Couleur du texte d'une cellule. Comme MS Project, la table n'est pas
/// colorée par la criticité (D-fix-7) : seule la colonne « Critique » d'une
/// tâche critique est en rouge (la barre du Gantt l'est aussi) ; les
/// récapitulatives sont en texte appuyé (et en gras simulé au dessin).
fn cell_text_color(row: &Row, column: Column, palette: &Palette) -> egui::Color32 {
    match row.kind {
        RowKind::Null | RowKind::Excluded => palette.text_weak,
        _ if row.critical && column == Column::Critical => palette.critical,
        RowKind::Summary => palette.text_strong,
        RowKind::Leaf | RowKind::Milestone => palette.text,
    }
}

/// La table des tâches vue comme une grille ([`Sheet`], J7 lot L10) : lignes
/// affichées du `RowModel` (indice de la grille = ligne affichée, J8 lot
/// L5), colonnes visibles de l'état d'interface, nom indenté par niveau,
/// récapitulative en gras simulé.
pub struct TaskSheet<'a> {
    /// Lignes affichées et textes pré-formatés.
    rows: VisibleRows<'a>,
    /// Colonnes visibles et leurs largeurs.
    columns: Vec<(Column, u16)>,
    /// Messages (refus d'une saisie).
    texts: &'a Texts,
    /// Triangles de repli (J8, lot L9) : un cran de retrait de plus pour
    /// chaque nom, où se dessine le triangle des récapitulatives.
    outline: bool,
    /// Alignements des données choisis dans « Paramètres de champ »
    /// (H-UI-115) ; vide le plus souvent.
    aligns: Vec<(Column, CellAlign)>,
    /// Mise en forme du texte des tâches (finitions P2, H-UI-126), résolue
    /// pour le thème de l'écran.
    looks: std::sync::Arc<crate::theme::TextLooks>,
    /// Ligne affichée du curseur (vue regroupée, finitions P3 : une tâche
    /// répétée sous plusieurs groupes y garde son curseur).
    hint: Option<DisplayRow>,
}

impl<'a> TaskSheet<'a> {
    /// Grille des lignes `rows` avec les colonnes visibles de `state`.
    #[must_use]
    pub fn new(rows: VisibleRows<'a>, state: &UiState, texts: &'a Texts) -> Self {
        Self {
            rows,
            columns: visible_columns(state),
            texts,
            outline: false,
            aligns: state
                .visible_columns()
                .filter_map(|c| c.align.map(|align| (c.column, align)))
                .collect(),
            looks: std::sync::Arc::default(),
            hint: None,
        }
    }

    /// Avec la ligne du curseur (vue regroupée, finitions P3).
    #[must_use]
    pub const fn with_hint(mut self, hint: Option<DisplayRow>) -> Self {
        self.hint = hint;
        self
    }

    /// La ligne `row` est un en-tête de groupe (finitions P3, H-UI-129) :
    /// ni sélection, ni saisie.
    #[must_use]
    pub fn is_header(&self, row: usize) -> bool {
        self.rows.header(DisplayRow::at(row)).is_some()
    }

    /// Avec la mise en forme du texte des tâches (H-UI-126).
    #[must_use]
    pub fn with_looks(mut self, looks: std::sync::Arc<crate::theme::TextLooks>) -> Self {
        self.looks = looks;
        self
    }

    /// Avec ou sans triangles de repli (H-UI-34, J8 lot L9).
    #[must_use]
    pub const fn with_outline(mut self, outline: bool) -> Self {
        self.outline = outline;
        self
    }
}

/// Colonnes visibles de la table et leurs largeurs, dans l'ordre
/// d'affichage.
fn visible_columns(state: &UiState) -> Vec<(Column, u16)> {
    state
        .visible_columns()
        .map(|state| (state.column, state.width))
        .collect()
}

impl Sheet for TaskSheet<'_> {
    type Key = TaskUid;
    type Col = Column;

    fn len(&self) -> usize {
        self.rows.len()
    }

    fn key(&self, row: usize) -> Option<TaskUid> {
        self.rows.uid(DisplayRow::at(row))
    }

    fn index_of(&self, key: TaskUid) -> Option<usize> {
        self.rows.display_near(key, self.hint).map(DisplayRow::index)
    }

    fn columns(&self) -> &[(Column, u16)] {
        &self.columns
    }

    fn title(&self, col: Column) -> TextId {
        col.title()
    }

    fn align(&self, col: Column) -> CellAlign {
        self.aligns
            .iter()
            .find(|(c, _)| *c == col)
            .map_or_else(|| col.align(), |(_, align)| *align)
    }

    fn style(&self, row: usize, col: Column, palette: &Palette) -> CellStyle {
        let grouped = self.rows.view().is_grouped();
        // En-tête de groupe (finitions P3, H-UI-129) : texte appuyé, Nom
        // décalé d'un cran (triangle de repli devant).
        if self.is_header(row) {
            return CellStyle {
                faux_bold: true,
                level: if col == Column::Name { 2 } else { 0 },
                ..CellStyle::plain(palette.text_strong)
            };
        }
        let Some(r) = self.rows.row(DisplayRow::at(row)) else {
            return CellStyle::plain(palette.text);
        };
        // Mise en forme du texte de la tâche (finitions P2, H-UI-126) :
        // toutes les colonnes sauf N° et Indicateurs ; le rouge de la
        // colonne Critique d'une tâche critique est gardé.
        let look = (r.kind != RowKind::Null && !matches!(col, Column::Id | Column::Indicators))
            .then(|| self.looks.get(&r.uid.get()).copied())
            .flatten()
            .unwrap_or_default();
        let color = match look.color {
            Some(rgb) if !(r.critical && col == Column::Critical) => {
                egui::Color32::from_rgb(rgb.0, rgb.1, rgb.2)
            }
            _ => cell_text_color(r, col, palette),
        };
        CellStyle {
            color,
            italic: look.italic,
            faux_bold: r.kind == RowKind::Summary || look.bold,
            // Tâche inactive : grisée (exclue du calcul) et barrée (H-UI-37).
            strike: !r.active && r.kind != RowKind::Null,
            level: match col {
                // Vue regroupée : toutes les tâches au cran du nom du groupe.
                Column::Name if grouped => 2,
                Column::Name if self.outline => r.level.saturating_add(1),
                Column::Name => r.level,
                _ => 0,
            },
        }
    }

    fn text(&self, row: usize, col: Column) -> &str {
        if let Some(h) = self.rows.header(DisplayRow::at(row)) {
            return match col {
                Column::Name => &h.name,
                Column::Start => &h.start,
                Column::Finish => &h.finish,
                _ => "",
            };
        }
        self.rows.text(DisplayRow::at(row), col)
    }

    fn editable(&self, col: Column) -> bool {
        editable(col)
    }

    /// Même lecture que la saisie de la table (`edit::commit_edit`, frappe
    /// directe : le texte est toujours envoyé) ; le refus est traduit dans
    /// la langue de l'interface.
    fn commit(
        &self,
        row: RowRef<TaskUid>,
        col: Column,
        text: &str,
        s: &Session,
        lang: TextLanguage,
    ) -> Result<Batch, String> {
        commit_edit(&EditState::typed(row.item(), col, text), s, lang).map_err(|e| e.message(self.texts))
    }
}

/// Demi-largeur de la poignée de redimensionnement au bord droit d'un
/// en-tête de colonne, en points (H-UI-99 : ±4 px).
pub const RESIZE_GRIP: f32 = 4.0;

/// Colonne « Ajouter une nouvelle colonne » en bout de table (H-UI-114) :
/// affichée depuis le lot L10 ; un test peut l'éteindre par
/// [`set_add_column_slot`].
pub const SHOW_ADD_COLUMN: bool = true;

/// Largeur ajoutée aux colonnes par la colonne « Ajouter une nouvelle
/// colonne » pour le défilement horizontal de la table (0 en graphique
/// seul ou si elle est éteinte dans `ctx` ; [`SHOW_ADD_COLUMN`] sans
/// contexte).
#[must_use]
pub fn add_column_extra(ctx: Option<&egui::Context>, chart_only: bool) -> f32 {
    let on = ctx.map_or(SHOW_ADD_COLUMN, add_column_slot);
    if on && !chart_only { ADD_COLUMN_W } else { 0.0 }
}

/// Largeur du contenu de la table : colonnes affichées, plus la colonne
/// « Ajouter une nouvelle colonne » (défilement horizontal, H-UI-114).
#[must_use]
pub fn content_width(ctx: &egui::Context, state: &crate::UiState) -> f32 {
    state.visible_columns().map(|c| f32::from(c.width)).sum::<f32>()
        + add_column_extra(Some(ctx), state.chart_only)
}

/// Largeur de la colonne « Ajouter une nouvelle colonne » (H-UI-114).
pub const ADD_COLUMN_W: f32 = 120.0;

/// Clé de la bascule de la colonne « Ajouter une nouvelle colonne ».
fn add_column_id() -> Id {
    Id::unique("plannercraft.table.add_column")
}

/// Allume ou éteint la colonne « Ajouter une nouvelle colonne » pour ce
/// contexte (tests du lot L6 ; par défaut [`SHOW_ADD_COLUMN`]).
pub fn set_add_column_slot(ctx: &egui::Context, on: bool) {
    ctx.data_mut(|data| data.insert_temp(add_column_id(), on));
}

/// La colonne « Ajouter une nouvelle colonne » est-elle affichée ?
#[must_use]
pub fn add_column_slot(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<bool>(add_column_id()))
        .unwrap_or(SHOW_ADD_COLUMN)
}

/// Clé, dans la mémoire d'egui, des colonnes sélectionnées entières à la
/// dernière image (bloc de cellules sur toutes les lignes affichées),
/// écrite par [`paint_body`] et lue par l'en-tête (H-UI-116).
fn whole_columns_id() -> Id {
    Id::unique("plannercraft.table.whole_columns")
}

/// Clé du déplacement de colonne en cours (H-UI-116).
fn column_move_id() -> Id {
    Id::unique("plannercraft.table.column_move")
}

/// Déplacement d'une colonne par son en-tête (H-UI-116), de l'appui au
/// relâcher ; tenu dans la mémoire d'egui (vue seulement).
#[derive(Clone, Copy, Debug, PartialEq)]
struct ColumnMove {
    /// Colonne appuyée.
    column: Column,
    /// Rang de départ, place du trait d'insertion, refus.
    drag: crate::column::ColumnDrag,
    /// Seuil de glisser franchi.
    armed: bool,
}

/// Déplacement de colonne en cours, s'il y en a un (tests).
#[must_use]
pub fn column_move(ctx: &egui::Context) -> Option<crate::column::ColumnDrag> {
    ctx.data(|data| data.get_temp::<ColumnMove>(column_move_id()))
        .filter(|m| m.armed)
        .map(|m| m.drag)
}

/// Titres affichés et alignements de titre des colonnes visibles
/// (« Paramètres de champ », H-UI-115 ; alias d'un champ personnalisé
/// relu à chaque image, H-UI-118).
fn header_titles(fc: &FrameCtx<'_>) -> Vec<(Column, String, CellAlign)> {
    let project = fc.doc.map(crate::app::OpenDocument::project);
    fc.state
        .visible_columns()
        .map(|c| {
            (
                c.column,
                crate::field::display_title(c, fc.texts, project),
                c.title_align(),
            )
        })
        .collect()
}

/// En-tête de la table : fond, titres des colonnes visibles (titre et
/// alignement de « Paramètres de champ », centrés par défaut, tronqués),
/// séparateurs verticaux et trait bas, décalés de `scroll_x` (défilement
/// horizontal de la table, J7 lot L6).
///
/// Chaque titre est une zone d'appui et de glisser (« Colonne Durée ») :
/// l'appui sélectionne la colonne entière, le glisser plusieurs colonnes,
/// le titre de l'ID tout (H-UI-90, `drag` : glisser de sélection) ; sur une
/// colonne déjà sélectionnée entière, le glisser la **déplace** (H-UI-116,
/// trait d'insertion, Échap annule) ; son clic droit ouvre le menu des
/// colonnes ([`column_menu`]) ; le bord droit de chaque titre se glisse
/// pour changer la largeur de la colonne (`Action::SetColumnWidth`, bornée
/// par `UiState::set_column_width`). En bout de table, la colonne « Ajouter
/// une nouvelle colonne » ([`add_column_slot`]) ouvre le sélecteur.
pub fn paint_header(
    ui: &mut egui::Ui,
    fc: &mut FrameCtx<'_>,
    rect: Rect,
    scroll_x: f32,
    drag: &mut Option<SelectDrag>,
) {
    if !rect.is_finite() || !rect.is_positive() {
        return;
    }
    let scroll_x = if scroll_x.is_finite() { scroll_x } else { 0.0 };
    let clip = rect.intersect(ui.clip_rect());
    let painter = ui.painter().with_clip_rect(clip);
    let palette = fc.palette;
    let font = grid::title_font(ui.style());
    let mut shapes = 0_usize;
    painter.rect_filled(rect, 0.0, palette.header_bg);
    shapes = shapes.saturating_add(1);
    let columns = visible_columns(fc.state);
    let titles = header_titles(fc);
    let title_of = |column: Column| {
        titles
            .iter()
            .find(|(c, _, _)| *c == column)
            .map_or_else(String::new, |(_, title, _)| title.clone())
    };
    let align_of = |column: Column| {
        titles
            .iter()
            .find(|(c, _, _)| *c == column)
            .map_or(CellAlign::Center, |(_, _, align)| *align)
    };
    // Indicateurs (H-UI-122) : titre dessiné en icône ⓘ ; le titre texte
    // reste le nom accessible de l'en-tête.
    let painted_title = |column: Column| {
        if column == Column::Indicators {
            String::new()
        } else {
            title_of(column)
        }
    };
    // Interface adoucie (lot U1, H-UI-168) : séparateur vertical après la
    // colonne figée (N°) seulement.
    let soft = fc.task_view.style.soft;
    let (edges, drawn) = grid::paint_titles_aligned(
        &painter,
        &columns,
        painted_title,
        align_of,
        |column| !soft || column == Column::Id,
        palette,
        HeaderPlace {
            rect,
            left: rect.left() - scroll_x,
            pad: fc.metrics.pad_x,
        },
        &font,
    );
    shapes = shapes.saturating_add(drawn);
    for &(column, left, right) in &edges {
        if column == Column::Indicators {
            let size = INDICATOR_ICON.min(right - left - 2.0);
            if size.is_finite() && size > 4.0 {
                let icon = Rect::from_center_size(
                    pos2(f32::midpoint(left, right), rect.center().y),
                    vec2(size, size),
                );
                crate::ribbon::paint_icon(&painter, icon, crate::ribbon::Icon::Info, palette.text_strong);
                shapes = shapes.saturating_add(1);
            }
        }
    }
    let whole: Vec<Column> = ui
        .ctx()
        .data(|data| data.get_temp::<Vec<Column>>(whole_columns_id()))
        .unwrap_or_default();
    for &(column, left, right) in &edges {
        let cell = Rect::from_min_max(pos2(left, rect.top()), pos2(right, rect.bottom())).intersect(clip);
        if cell.is_positive() {
            let title = title_of(column);
            let movable = whole.contains(&column);
            header_cell(
                ui,
                fc,
                &HeaderHit {
                    column,
                    cell,
                    title: &title,
                    movable,
                },
                drag,
            );
        }
    }
    drag_over_headers(ui, fc, &edges, drag);
    shapes = shapes.saturating_add(track_column_move(ui, &painter, fc, &edges, rect));
    if add_column_slot(ui.ctx()) && !fc.state.chart_only {
        let content: f32 = columns.iter().map(|(_, w)| f32::from(*w)).sum();
        let left = rect.left() - scroll_x + content;
        shapes = shapes.saturating_add(add_column_header(ui, &painter, fc, left, rect, clip, &font));
    }
    painter.hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0, palette.grid_major),
    );
    shapes = shapes.saturating_add(1);
    fc.stats.shapes = fc.stats.shapes.saturating_add(shapes);
    // Poignées après les titres : elles passent au-dessus pour le pointeur.
    for (column, cell_left, right) in edges {
        resize_grip(ui, fc, column, cell_left, right, rect, clip);
    }
}

/// Titre grisé et zone de clic de la colonne « Ajouter une nouvelle
/// colonne » (H-UI-114) à partir de l'abscisse `left` : un clic ouvre le
/// sélecteur pour une insertion en fin de table. Rend le nombre de formes.
fn add_column_header(
    ui: &egui::Ui,
    painter: &Painter,
    fc: &mut FrameCtx<'_>,
    left: f32,
    rect: Rect,
    clip: Rect,
    font: &egui::FontId,
) -> usize {
    let slot = Rect::from_min_max(pos2(left, rect.top()), pos2(left + ADD_COLUMN_W, rect.bottom()));
    let hit = slot.intersect(clip);
    if !slot.is_finite() || !hit.is_positive() {
        return 0;
    }
    let texts = fc.texts;
    let label = texts.get(TextId::AddNewColumn);
    let max_w = ADD_COLUMN_W - 2.0 * fc.metrics.pad_x;
    let galley = grid::truncated_galley(painter, &label, font.clone(), max_w);
    let size = galley.size();
    let x = grid::aligned_x(CellAlign::Center, left, ADD_COLUMN_W, fc.metrics.pad_x, size.x);
    painter.galley(
        pos2(x, rect.center().y - size.y * 0.5),
        galley,
        fc.palette.text_weak,
    );
    if !fc.task_view.style.soft {
        painter.vline(
            slot.right() - 0.5,
            rect.y_range(),
            Stroke::new(1.0, fc.palette.grid_major),
        );
    }
    let response = ui.interact(
        hit,
        Id::unique("plannercraft.table.add_column.hit"),
        Sense::click(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label.clone()));
    if response.clicked() {
        fc.actions.push(Action::Column(ColumnOp::OpenPicker {
            at: fc.state.columns.len(),
        }));
    }
    2
}

/// Rang d'une colonne dans `UiState.columns` (ordre d'affichage).
fn rank_of(fc: &FrameCtx<'_>, column: Column) -> Option<usize> {
    fc.state.columns.iter().position(|c| c.column == column)
}

/// Rang devant lequel une colonne glissée irait, pointeur en `x` (bord des
/// titres dessinés `edges`) : avant la première colonne dont le milieu est
/// à droite de `x`, sinon après la dernière dessinée ; et abscisse du trait
/// d'insertion. `None` sans titre dessiné ou `x` non fini.
fn insertion_rank(fc: &FrameCtx<'_>, edges: &[(Column, f32, f32)], x: f32) -> Option<(usize, f32)> {
    if !x.is_finite() {
        return None;
    }
    for &(column, left, right) in edges {
        if x < left.midpoint(right) {
            return rank_of(fc, column).map(|rank| (rank, left));
        }
    }
    let &(last, _, right) = edges.last()?;
    rank_of(fc, last).map(|rank| (rank.saturating_add(1), right))
}

/// Déplacement de colonne en cours (H-UI-116) : seuil de glisser, trait
/// d'insertion vertical de 2 px, curseur (interdit devant l'ID), Échap
/// annule ; au relâcher, `ColumnOp::Move` si la place change et n'est pas
/// refusée. Rend le nombre de formes.
fn track_column_move(
    ui: &egui::Ui,
    painter: &Painter,
    fc: &mut FrameCtx<'_>,
    edges: &[(Column, f32, f32)],
    rect: Rect,
) -> usize {
    let id = column_move_id();
    let Some(mut moving) = ui.ctx().data(|data| data.get_temp::<ColumnMove>(id)) else {
        return 0;
    };
    let clear = |ui: &egui::Ui| ui.ctx().data_mut(|data| data.remove::<ColumnMove>(id));
    if rank_of(fc, moving.column) != Some(moving.drag.from) {
        clear(ui);
        return 0;
    }
    if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        clear(ui);
        return 0;
    }
    let (down, pos) = ui.input(|i| (i.pointer.primary_down(), i.pointer.interact_pos()));
    let pos = pos.filter(|p| p.is_finite());
    if !down {
        clear(ui);
        let to = moving.drag.to;
        let from = moving.drag.from;
        if moving.armed && !moving.drag.refused && to != from && to != from.saturating_add(1) {
            let to = if to > from { to.saturating_sub(1) } else { to };
            fc.actions.push(Action::Column(ColumnOp::Move { from, to }));
        }
        return 0;
    }
    let Some(p) = pos else {
        return 0;
    };
    if !moving.armed {
        let threshold = ui.ctx().options(|o| o.input_options.max_click_dist);
        moving.armed = ui
            .input(|i| i.pointer.press_origin())
            .filter(|o| o.is_finite())
            .is_some_and(|o| o.distance(p) >= threshold);
    }
    let mut shapes = 0;
    if moving.armed
        && let Some((to, x)) = insertion_rank(fc, edges, p.x)
    {
        moving.drag.to = to;
        moving.drag.refused = to == 0;
        let icon = if moving.drag.refused {
            egui::CursorIcon::NotAllowed
        } else {
            egui::CursorIcon::Grabbing
        };
        ui.ctx().set_cursor_icon(icon);
        if !moving.drag.refused && x.is_finite() {
            let line = Rect::from_min_max(pos2(x - 1.0, rect.top()), pos2(x + 1.0, rect.bottom()));
            painter.rect_filled(line, 0.0, fc.palette.cursor);
            shapes = 1;
        }
    }
    ui.ctx().data_mut(|data| data.insert_temp(id, moving));
    shapes
}

/// Mode d'un appui de sélection d'après les modificateurs (H-UI-87) :
/// Ctrl/Cmd ajoute ou retire, Maj étend, sinon remplace.
pub(crate) const fn press_mode(modifiers: Modifiers) -> PressMode {
    if modifiers.command {
        PressMode::Toggle
    } else if modifiers.shift {
        PressMode::Extend
    } else {
        PressMode::Replace
    }
}

/// Appui du bouton principal sur `response` dans cette image (ADR-0018
/// D1 : l'appui sélectionne, sans attendre le relâcher) : appui vu par egui
/// sur ce widget, ou appui et relâcher dans la même image (clic éclair).
fn pressed_now(ui: &egui::Ui, response: &Response) -> bool {
    ui.input(|i| i.pointer.primary_pressed()) && (response.is_pointer_button_down_on() || response.clicked())
}

/// Titre de colonne touché à une image.
struct HeaderHit<'t> {
    /// Colonne.
    column: Column,
    /// Rectangle du titre (découpé).
    cell: Rect,
    /// Titre affiché.
    title: &'t str,
    /// Colonne sélectionnée entière à la dernière image : l'appui suivi
    /// d'un glisser la déplace (H-UI-116).
    movable: bool,
}

/// Zone d'un titre de colonne : nœud d'accessibilité, menu des colonnes
/// au clic droit, appui de sélection (H-UI-90 : colonne entière ; Maj :
/// de l'ancre de colonnes à celle-ci ; titre de l'ID : tout) qui laisse le
/// clavier à la table (H-UI-101). Sur une colonne déjà sélectionnée
/// entière (hors ID), l'appui ouvre un déplacement (H-UI-116) au lieu d'un
/// glisser de sélection.
fn header_cell(ui: &egui::Ui, fc: &mut FrameCtx<'_>, hit: &HeaderHit<'_>, drag: &mut Option<SelectDrag>) {
    let &HeaderHit {
        column,
        cell,
        title,
        movable,
    } = hit;
    let texts = fc.texts;
    let response = ui.interact(
        cell,
        ui.make_persistent_id(("plannercraft.table.header", column)),
        Sense::click_and_drag(),
    );
    let pressed = pressed_now(ui, &response);
    // Clic sans appui vu (accessibilité) : même geste.
    if pressed || (response.clicked() && drag.is_none()) {
        let mode = match press_mode(ui.input(|i| i.modifiers)) {
            PressMode::Extend => PressMode::Extend,
            PressMode::Replace | PressMode::Toggle => PressMode::Replace,
        };
        if column == Column::Id {
            fc.actions.push(Action::SelectOp(SelectOp::All));
        } else {
            fc.actions.push(Action::SelectOp(SelectOp::Press {
                row: RowRef::Blank,
                column,
                target: PressTarget::ColumnHeader,
                mode,
            }));
            let from = rank_of(fc, column);
            if pressed
                && movable
                && mode == PressMode::Replace
                && !fc.state.chart_only
                && let Some(from) = from
            {
                let moving = ColumnMove {
                    column,
                    drag: crate::column::ColumnDrag {
                        from,
                        to: from,
                        refused: false,
                    },
                    armed: false,
                };
                ui.ctx()
                    .data_mut(|data| data.insert_temp(column_move_id(), moving));
            } else if pressed {
                *drag = Some(SelectDrag {
                    target: PressTarget::ColumnHeader,
                    last: (RowRef::Blank, column),
                    armed: false,
                    toggle: false,
                });
            }
        }
        fc.actions.push(Action::FocusPane(Pane::Table));
    }
    response.widget_info(|| {
        let label = texts.get_args(TextId::ColumnHeaderLabel, &[("column", Arg::Str(title.into()))]);
        egui::WidgetInfo::labeled(egui::Role::ColumnHeader, true, label)
    });
    let mut chosen = None;
    response.context_menu(|ui| chosen = column_menu(ui, fc, column));
    if let Some(action) = chosen {
        fc.actions.push(action);
    }
}

/// Glisser commencé sur un titre de colonne (H-UI-90) : la colonne sous le
/// pointeur (bornée aux titres dessinés) étend la sélection de colonnes
/// depuis l'ancre (`Press { ColumnHeader, Extend }`), poussé seulement
/// quand elle change, une fois le seuil de glisser franchi
/// ([`arm_select_drag`]).
fn drag_over_headers(
    ui: &egui::Ui,
    fc: &mut FrameCtx<'_>,
    edges: &[(Column, f32, f32)],
    drag: &mut Option<SelectDrag>,
) {
    let Some(d) = drag.as_mut().filter(|d| d.target == PressTarget::ColumnHeader) else {
        return;
    };
    let Some(p) = ui
        .input(|i| {
            i.pointer
                .primary_down()
                .then(|| i.pointer.interact_pos())
                .flatten()
        })
        .filter(|p| p.is_finite())
    else {
        return;
    };
    if !arm_select_drag(ui, d, p) {
        return;
    }
    let x = p.x;
    let (Some(first), Some(last)) = (edges.first(), edges.last()) else {
        return;
    };
    let column = if x < first.1 {
        first.0
    } else if x >= last.2 {
        last.0
    } else {
        edges
            .iter()
            .find(|(_, left, right)| x >= *left && x < *right)
            .map_or(d.last.1, |(column, _, _)| *column)
    };
    if column != d.last.1 {
        d.last.1 = column;
        fc.actions.push(Action::SelectOp(SelectOp::Press {
            row: RowRef::Blank,
            column,
            target: PressTarget::ColumnHeader,
            mode: PressMode::Extend,
        }));
    }
}

/// Menu des colonnes (clic droit sur un titre, H-UI-114, H-UI-99) :
/// « Insérer une colonne » (sélecteur, insertion à gauche de la colonne
/// cliquée), « Masquer la colonne » (grisé pour l'ID et le Nom, H-UI-12),
/// « Paramètres de champ… » (H-UI-115), « Ajuster la largeur »
/// (`FitColumn`, comme le double-clic sur la bordure), « Trier par ordre
/// croissant » et « décroissant » (`TaskViewOp::SetSort` sur la clé de
/// [`crate::column::sort_key`] ; grisés, raison en info-bulle, pour une
/// colonne sans clé de tri). Les cases de colonnes d'avant la seconde passe
/// sont remplacées par l'insertion (H-UI-114) ; elles restent au menu
/// Format → Colonnes → Affichage du tableau. Rend l'action choisie.
fn column_menu(ui: &mut egui::Ui, fc: &FrameCtx<'_>, clicked: Column) -> Option<Action> {
    let texts = fc.texts;
    let mut chosen = None;
    let rank = rank_of(fc, clicked);
    if ui
        .add_enabled(
            rank.is_some(),
            egui::Button::new(texts.get(TextId::MenuInsertColumn)),
        )
        .clicked()
        && let Some(at) = rank
    {
        chosen = Some(Action::Column(ColumnOp::OpenPicker { at }));
    }
    let hide = egui::Button::new(texts.get(TextId::ColumnMenuHide));
    if ui.add_enabled(clicked.hideable(), hide).clicked()
        && let Some(at) = rank
    {
        chosen = Some(Action::Column(ColumnOp::Hide(at)));
    }
    let mut settings = egui::Button::new(texts.get(TextId::MenuFieldSettings));
    if let Some(shortcut) = crate::keymap::shortcut_for_os(
        crate::keymap::Binding::Ui(crate::keymap::UiBinding::FieldSettings),
        ui.ctx().os(),
    ) {
        settings = settings.shortcut_text(crate::keymap::shortcut_text(ui.ctx(), shortcut));
    }
    if ui.add_enabled(rank.is_some(), settings).clicked()
        && let Some(at) = rank
    {
        chosen = Some(Action::Column(ColumnOp::OpenSettings(at)));
    }
    if ui.button(texts.get(TextId::MenuFitColumn)).clicked() {
        chosen = Some(Action::FitColumn(clicked));
    }
    let key = crate::column::sort_key(clicked);
    for (id, descending) in [
        (TextId::MenuSortAscending, false),
        (TextId::MenuSortDescending, true),
    ] {
        let response = ui
            .add_enabled(key.is_some(), egui::Button::new(texts.get(id)))
            .on_disabled_hover_text(texts.get(TextId::ColumnNotSortable));
        if response.clicked()
            && let Some(key) = key
        {
            chosen = Some(sort_action(fc, key, descending));
        }
    }
    chosen
}

/// Tri de la vue des tâches par `key` depuis le menu d'en-tête (H-UI-99) :
/// « Conserver la structure » repris du tri en cours (coché par défaut,
/// comme la fenêtre « Trier ») ; l'ID croissant revient à l'ordre des ID.
fn sort_action(fc: &FrameCtx<'_>, key: SortKey, descending: bool) -> Action {
    let keep_outline = fc.task_view.state.sort.is_none_or(|spec| spec.keep_outline);
    let spec = (key != SortKey::Id || descending).then_some(SortSpec {
        key,
        descending,
        keep_outline,
    });
    Action::TaskView(TaskViewOp::SetSort(spec))
}

/// Poignée du bord droit d'un titre (`right`, ±[`RESIZE_GRIP`]) : glisser
/// pousse `SetColumnWidth` d'après la position du pointeur (largeur entière
/// ≥ 0, bornée ensuite par l'état) ; double-clic ⇒ `FitColumn` (H-UI-99,
/// S88) ; curseur de redimensionnement et info-bulle au survol (H-UI-103).
fn resize_grip(
    ui: &egui::Ui,
    fc: &mut FrameCtx<'_>,
    column: Column,
    cell_left: f32,
    right: f32,
    rect: Rect,
    clip: Rect,
) {
    let grip = Rect::from_min_max(
        pos2(right - RESIZE_GRIP, rect.top()),
        pos2(right + RESIZE_GRIP, rect.bottom()),
    )
    .intersect(clip);
    if !grip.is_finite() || !grip.is_positive() {
        return;
    }
    let texts = fc.texts;
    let response = ui.interact(
        grip,
        ui.make_persistent_id(("plannercraft.table.resize", column)),
        Sense::click_and_drag(),
    );
    response.widget_info(|| {
        let label = texts.get_args(
            TextId::ColumnResizeLabel,
            &[("column", Arg::Str(texts.get(column.title()).into()))],
        );
        egui::WidgetInfo::labeled(egui::Role::Splitter, true, label)
    });
    let response = response
        .on_hover_and_drag_cursor(egui::CursorIcon::ResizeColumn)
        .on_hover_text(texts.get(TextId::TipColumnResize));
    // H-UI-101 : la poignée laisse le clavier à la table.
    if pressed_now(ui, &response) {
        fc.actions.push(Action::FocusPane(Pane::Table));
    }
    if response.double_clicked() {
        fc.actions.push(Action::FitColumn(column));
        return;
    }
    if !response.dragged() {
        return;
    }
    let Some(p) = response.interact_pointer_pos() else {
        return;
    };
    let Some(width) = width_from_drag(p.x - cell_left) else {
        return;
    };
    let current = fc.state.column(column).map(|state| state.width);
    if current != Some(width) {
        fc.actions.push(Action::SetColumnWidth { column, width });
    }
}

/// Largeur entière d'une colonne d'après la distance entre son bord gauche
/// et le pointeur : arrondie, bornée à `[Column::MIN_WIDTH,
/// Column::MAX_WIDTH]` ; `None` si la distance n'est pas finie.
#[must_use]
pub fn width_from_drag(dx: f32) -> Option<u16> {
    if !dx.is_finite() {
        return None;
    }
    let w = dx
        .round()
        .clamp(f32::from(Column::MIN_WIDTH), f32::from(Column::MAX_WIDTH));
    // Borné à [24, 2 000] et fini : la conversion par `f32_to_index` est
    // exacte.
    u16::try_from(crate::geom::f32_to_index(w)).ok()
}

/// Barre de défilement horizontale de la table, dans la bande sous la
/// table : bande unie (comme avant J7) tant que les colonnes tiennent, barre
/// glissable sinon. Rend le défilement demandé, en points.
pub fn paint_hbar(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, track: Rect, scroll_x: f32, view_w: f32) -> f32 {
    let slot = if add_column_slot(ui.ctx()) && !fc.state.chart_only {
        ADD_COLUMN_W
    } else {
        0.0
    };
    let content_w: f32 = fc
        .state
        .visible_columns()
        .map(|state| f32::from(state.width))
        .sum::<f32>()
        + slot;
    if !track.is_finite() || !track.is_positive() {
        return 0.0;
    }
    if content_w.is_nan() || content_w <= view_w {
        ui.painter().rect_filled(track, 0.0, fc.palette.header_bg);
        fc.stats.shapes = fc.stats.shapes.saturating_add(1);
        return 0.0;
    }
    super::main_view::hbar(
        ui,
        fc,
        track,
        &HBarSpec {
            id: Id::unique("plannercraft.table-hbar"),
            label: TextId::TableScrollLabel,
            scroll: scroll_x,
            content_w,
            visible_w: view_w,
        },
    )
}

/// Corps de la table : lignes `range`, `origin` étant le coin haut-gauche
/// écran de la ligne 0, découpe `clip`, colonnes décalées de `scroll_x`
/// (défilement horizontal, J7 lot L6).
///
/// Fond alterné léger, ligne sélectionnée surlignée, texte en couleur
/// normale (rouge seulement dans la colonne « Critique »), nom indenté par
/// niveau, récapitulative en texte appuyé, ligne vide sans texte. Chaque
/// ligne est une zone d'appui et de glisser (ADR-0018 D1 : l'appui
/// sélectionne, `SelectOp::Press` ; le glisser étend, `SelectOp::DragTo`,
/// suivi par `select_drag` de l'appui au relâcher, voir [`row_pointer`] et
/// [`track_select_drag`]) qui porte le libellé d'accessibilité `row-label`. La ligne
/// d'indice `rows.len()` est la ligne vide de fin (création d'une tâche,
/// libellé `blank-row-label`). La table entière est un widget focalisable
/// ([`table_id`]) ; la cellule courante porte un cadre de 2 px et un nœud
/// d'accessibilité (« Ligne 3, Durée, 5 jours »). Saisie des cellules : voir
/// [`edit_cells`]. Écrit `FrameStats::table_rows` (et lui seul : le Gantt
/// écrit `gantt_rows`).
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "contexte d'image découpé en emprunts disjoints ; suite d'étapes du dessin d'une image"
)]
pub fn paint_body(
    ui: &mut egui::Ui,
    fc: &mut FrameCtx<'_>,
    edit: &mut TableEdit,
    cursor: &mut TableCursor,
    drags: Drags<'_>,
    origin: Pos2,
    range: Range<usize>,
    clip: Rect,
    scroll_x: f32,
) {
    let Drags {
        select: select_drag,
        rows: row_drag,
    } = drags;
    let clip = clip.intersect(ui.clip_rect());
    let scroll_x = if scroll_x.is_finite() { scroll_x } else { 0.0 };
    if !clip.is_finite() || !clip.is_positive() || !origin.x.is_finite() || !origin.y.is_finite() {
        return;
    }
    let painter = ui.painter().with_clip_rect(clip);
    let palette = fc.palette;
    let metrics = fc.metrics;
    let row_h = metrics.row_h;
    let (font, bands) = RowFill::of(ui, fc);
    let mut shapes = 0_usize;

    painter.rect_filled(clip, 0.0, palette.background);
    shapes = shapes.saturating_add(1);

    let Some(rows) = fc.visible() else {
        fc.stats.shapes = fc.stats.shapes.saturating_add(shapes);
        return;
    };
    focus_widget(ui, fc, clip);
    let sheet = TaskSheet::new(rows, fc.state, fc.texts)
        .with_outline(fc.task_view.outline_triangles)
        .with_looks(std::sync::Arc::clone(&fc.task_view.style.text_looks))
        .with_hint(fc.task_view.cursor_row);
    open_from_keys(ui, fc, &sheet, edit, cursor);
    fc.stats.table_rows = range.clone();
    let left = clip.left();
    let cols_left = left - scroll_x;
    let blank = rows.len();
    // Clic sur une cellule pendant une saisie : la saisie est validée
    // d'abord (`edit_cells`), la sélection ne bouge qu'après un accord.
    let mut deferred = None;
    let mut place = RowPlace {
        rect: Rect::NOTHING,
        cols_left,
        pad: metrics.pad_x,
        indent: metrics.indent,
    };
    // Bloc de la sélection (H-UI-103) : une résolution par image, lue par
    // le dessin de chaque ligne ; survol hors de tout glisser.
    let marks = BlockMarks::of(fc, &sheet, cursor, cols_left);
    // Colonnes sélectionnées entières (bloc de cellules sur toutes les
    // lignes affichées), lues par l'en-tête de l'image suivante pour le
    // déplacement d'une colonne (H-UI-116).
    let whole: Vec<Column> = if marks.block.kind == crate::selection::SelectionKind::Cells
        && blank > 0
        && marks.block.rows.len() == blank
    {
        marks.block.columns.clone()
    } else {
        Vec::new()
    };
    ui.ctx()
        .data_mut(|data| data.insert_temp(whole_columns_id(), whole));
    let hover = hovered_row(
        ui,
        clip,
        origin,
        row_h,
        blank,
        select_drag.is_none() && row_drag.is_none(),
    );
    let mut drawn = SelectionDrawn {
        hover,
        ..SelectionDrawn::default()
    };
    // Colonne Indicateurs affichée : place (décalage, largeur) et échéances
    // dépassées signalées par le moteur, lues une fois par image.
    let indicator_cell = indicator_place(sheet.columns());
    let missed = match (indicator_cell, fc.doc) {
        (Some(_), Some(doc)) => crate::indicators::missed_deadlines(doc.session.diagnostics()),
        _ => std::collections::BTreeMap::new(),
    };
    for i in range.clone() {
        let top = origin.y + index_to_f32(i) * row_h;
        let row_rect = Rect::from_min_size(pos2(left, top), vec2(clip.width(), row_h));
        if !row_rect.is_finite() {
            continue;
        }
        // En-tête de groupe (finitions P3, H-UI-129) : fond de l'en-tête,
        // textes du groupe, triangle ; un clic replie ou déplie.
        if let Some(header) = rows.header(DisplayRow::at(i)) {
            place.rect = row_rect;
            let at = GroupRowPlace {
                row_rect,
                cols_left,
                clip,
                place,
            };
            shapes = shapes.saturating_add(paint_group_row(ui, &painter, fc, &sheet, header, i, &at, &font));
            continue;
        }
        let row = rows.row(DisplayRow::at(i));
        if row.is_none() && i != blank {
            continue;
        }
        let selected = match row {
            Some(row) => fc.selected.contains(row.uid),
            None => fc.selection.is_none(),
        };
        let base = if hover == Some(i) && !marks.full_row(i) {
            Some(palette.hover_row)
        } else {
            bands.fill(rows, i)
        };
        shapes = shapes.saturating_add(marks.paint_row(&painter, palette, i, row_rect, base, &mut drawn));
        painter.hline(
            row_rect.x_range(),
            row_rect.bottom() - 0.5,
            Stroke::new(1.0, palette.grid),
        );
        shapes = shapes.saturating_add(1);

        if let Some(row) = row
            && row.kind != RowKind::Null
        {
            place.rect = row_rect;
            shapes = shapes.saturating_add(grid::paint_cells(&painter, &sheet, palette, i, place, &font));
        }

        let hit = row_rect.intersect(clip);
        if hit.is_positive() {
            let response = row_widget(ui, fc, rows, hit, i, selected);
            // Indicateurs (H-UI-122) : icônes, infobulle et clic sur la
            // note, au-dessus de la ligne.
            if let (Some(row), Some((offset, width))) = (row, indicator_cell)
                && row.kind != RowKind::Null
            {
                let cell = Rect::from_min_size(pos2(cols_left + offset, top), vec2(width, row_h));
                shapes = shapes.saturating_add(paint_indicators(ui, &painter, fc, row, cell, clip, &missed));
            }
            let at = RowHit {
                response: &response,
                i,
                left: cols_left,
            };
            let drags = Drags {
                select: &mut *select_drag,
                rows: &mut *row_drag,
            };
            deferred = row_pointer(ui, fc, &sheet, edit, cursor, drags, &at).or(deferred);
        }
        // Triangle de repli (J8, lot L9, H-UI-34), interactif au-dessus de
        // la ligne : déclaré après elle.
        if sheet.outline
            && let Some(row) = row
            && row.kind == RowKind::Summary
        {
            let at = TrianglePlace {
                row_rect,
                cols_left,
                clip,
            };
            shapes = shapes.saturating_add(outline_triangle(ui, &painter, fc, &sheet, row, i, &at));
        }
    }

    // Glisser de sélection (H-UI-88, H-UI-89) : suivi d'après la position
    // du pointeur, bornée au corps, que la ligne appuyée soit encore
    // dessinée ou non (défilement automatique, H-UI-91).
    let body_at = BodyPlace {
        origin,
        cols_left,
        clip,
    };
    track_select_drag(ui, fc, &sheet, select_drag, &body_at);
    // Déplacement de lignes par l'ID (H-UI-94) : trait d'insertion,
    // curseur, refus, lâcher.
    shapes = shapes.saturating_add(track_row_drag(ui, &painter, fc, sheet.rows, row_drag, &body_at));

    // Séparateurs verticaux de colonnes, sur toute la hauteur visible ;
    // interface adoucie (lot U1, H-UI-168) : après la colonne figée (N°)
    // seulement, en filet de la grille marquée.
    let soft = fc.task_view.style.soft;
    shapes = shapes.saturating_add(grid::paint_column_lines_where(
        &painter,
        sheet.columns(),
        cols_left,
        clip,
        if soft { palette.grid_major } else { palette.grid },
        |column| !soft || *column == Column::Id,
    ));
    // Bord droit de la colonne « Ajouter une nouvelle colonne » (H-UI-114).
    if add_column_slot(ui.ctx()) && !fc.state.chart_only && !soft {
        let content: f32 = sheet.columns().iter().map(|(_, w)| f32::from(*w)).sum();
        let x = cols_left + content + ADD_COLUMN_W;
        if x.is_finite() && x > clip.left() && x <= clip.right() {
            painter.vline(x - 0.5, clip.y_range(), Stroke::new(1.0, palette.grid));
            shapes = shapes.saturating_add(1);
        }
    }
    // Bordure du bloc, au-dessus des séparateurs, sous le cadre actif.
    shapes = shapes.saturating_add(marks.paint_borders(&painter, palette, origin, row_h, &range, &mut drawn));
    ui.ctx()
        .data_mut(|data| data.insert_temp(selection_drawn_id(), drawn));
    let at = Place {
        origin,
        cols_left,
        range: &range,
        font: &font,
    };
    shapes = shapes.saturating_add(paint_cursor(ui, &painter, fc, &sheet, edit, cursor, &at));
    fc.stats.shapes = fc.stats.shapes.saturating_add(shapes);
    edit_cells(ui, fc, &sheet, edit, cursor, &at, clip, deferred);
}

/// Côté des icônes de la colonne Indicateurs, en points (H-UI-122).
const INDICATOR_ICON: f32 = 14.0;

/// Écart entre deux icônes de la colonne Indicateurs, en points.
const INDICATOR_GAP: f32 = 2.0;

/// Place de la colonne Indicateurs parmi les colonnes affichées :
/// décalage depuis le bord gauche des colonnes et largeur.
fn indicator_place(columns: &[(Column, u16)]) -> Option<(f32, f32)> {
    let mut x = 0.0_f32;
    for &(column, width) in columns {
        let w = f32::from(width);
        if column == Column::Indicators {
            return Some((x, w));
        }
        x += w;
    }
    None
}

/// Icônes d'une ligne dans la cellule Indicateurs (H-UI-122) : autant
/// qu'il en tient, dans l'ordre ; infobulle d'une ligne par indicateur sur
/// toute la cellule ; clic sur l'icône de note ⇒ tâche sélectionnée et
/// « Notes » ouvertes. Rend le nombre de formes.
fn paint_indicators(
    ui: &egui::Ui,
    painter: &egui::Painter,
    fc: &mut FrameCtx<'_>,
    row: &Row,
    cell: Rect,
    clip: Rect,
    missed: &std::collections::BTreeMap<TaskUid, plannercraft_core::Moment>,
) -> usize {
    let Some(doc) = fc.doc else {
        return 0;
    };
    let project = doc.project();
    let Some(task) = project.tasks.get(row.uid) else {
        return 0;
    };
    let list = crate::indicators::of(task, row, project.anchor, missed);
    let visible = cell.intersect(clip);
    if list.is_empty() || !visible.is_positive() || !cell.is_finite() {
        return 0;
    }
    let mut shapes = 0_usize;
    // Autant d'icônes qu'il en tient, le groupe centré dans la cellule.
    let room = cell.width() - 2.0;
    let mut fit = 0_usize;
    let mut used = 0.0_f32;
    for _ in &list {
        let next = used + INDICATOR_ICON + if fit == 0 { 0.0 } else { INDICATOR_GAP };
        if next > room {
            break;
        }
        used = next;
        fit = fit.saturating_add(1);
    }
    let mut x = cell.center().x - used / 2.0;
    let color = fc.palette.text;
    let mut note_rect = None;
    for indicator in list.iter().take(fit) {
        let r = Rect::from_min_size(
            pos2(x, cell.center().y - INDICATOR_ICON / 2.0),
            vec2(INDICATOR_ICON, INDICATOR_ICON),
        );
        crate::ribbon::paint_icon(painter, r, indicator.icon(), color);
        shapes = shapes.saturating_add(1);
        if *indicator == crate::indicators::Indicator::Note {
            note_rect = Some(r);
        }
        x += INDICATOR_ICON + INDICATOR_GAP;
    }
    let texts = fc.texts;
    let id = Id::unique(("plannercraft.indicators", row.uid));
    let hover = ui.interact(visible, id, Sense::hover());
    let shown = hover.hovered();
    if shown {
        let fields = crate::field::field_labels(texts);
        let text = crate::indicators::tooltip(&list, texts, &fields, texts.format_labels());
        hover.on_hover_text(text);
    }
    if let Some(r) = note_rect.map(|r| r.intersect(clip)).filter(Rect::is_positive) {
        let note = ui.interact(r, id.with("note"), Sense::click());
        let name = texts.get(TextId::IndicatorNote);
        note.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
        if note.clicked() {
            fc.actions.push(Action::Select(Some(row.uid)));
            fc.actions.push(Action::ShowTaskNotes);
        }
    }
    shapes
}

/// Ce que la table a dessiné de la sélection à la dernière image
/// (H-UI-103), relu par les tests ([`selection_drawn`]) : lignes affichées
/// et colonnes, jamais de pixels.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectionDrawn {
    /// Cellules teintées d'un bloc de cellules (ligne affichée, colonne).
    pub tinted: Vec<(usize, Column)>,
    /// Lignes entières teintées sur toute la largeur.
    pub full_rows: Vec<usize>,
    /// Lignes dont l'ID est appuyé (`header_selected`).
    pub pressed_ids: Vec<usize>,
    /// Cellule active d'un bloc de plus d'une cellule, laissée non teintée
    /// dans son cadre : ligne affichée, colonne, fond dessiné.
    pub active: Option<(usize, Column, egui::Color32)>,
    /// Bordures de 2 px du bloc (une par suite de lignes voisines).
    pub borders: Vec<Rect>,
    /// Ligne survolée (fond `hover_row`), jamais pendant un glisser.
    pub hover: Option<usize>,
}

/// Clé de [`SelectionDrawn`] dans la mémoire d'egui.
fn selection_drawn_id() -> Id {
    Id::unique("plannercraft.table.selection_drawn")
}

/// Dessin de la sélection de la table à la dernière image (tests,
/// H-UI-103).
#[must_use]
pub fn selection_drawn(ctx: &egui::Context) -> SelectionDrawn {
    ctx.data(|data| data.get_temp::<SelectionDrawn>(selection_drawn_id()))
        .unwrap_or_default()
}

/// Ligne sous le pointeur (ligne vide comprise), si `idle` (aucun glisser
/// de la table), aucun bouton tenu (barre, lien, poignée, défilement) et
/// le pointeur sur le corps sans fenêtre ni menu par-dessus. Toute valeur
/// non finie : aucune.
fn hovered_row(
    ui: &egui::Ui,
    clip: Rect,
    origin: Pos2,
    row_h: f32,
    blank: usize,
    idle: bool,
) -> Option<usize> {
    if !idle {
        return None;
    }
    let (pos, down) = ui.input(|i| (i.pointer.hover_pos(), i.pointer.any_down()));
    let pos = pos.filter(|p| !down && p.is_finite() && clip.contains(*p))?;
    if !ui.rect_contains_pointer(clip) {
        return None;
    }
    crate::geom::row_at(pos.y - origin.y, row_h, blank.saturating_add(1))
}

/// Bloc de la sélection prêt à dessiner pour une image (H-UI-103) : lignes
/// affichées du bloc, abscisses de ses colonnes, cellule active.
struct BlockMarks {
    /// Bloc résolu (`block::resolve`, seule résolution).
    block: crate::block::Block,
    /// Plus d'une cellule : teinte et bordure ; sinon seul l'ID est appuyé
    /// et le cadre de la cellule active la montre.
    multi: bool,
    /// Ligne affichée de la cellule active (ligne vide : `rows.len()`).
    active_row: Option<usize>,
    /// Abscisses écran (gauche, largeur) de la colonne active.
    active_x: Option<(f32, f32)>,
    /// Abscisses écran (gauche, droite) des colonnes du bloc.
    span_x: Option<(f32, f32)>,
    /// Abscisses écran (gauche, largeur) de la colonne ID.
    id_x: Option<(f32, f32)>,
}

impl BlockMarks {
    /// Bloc de la sélection de `fc` et du curseur `cursor` sur la grille
    /// `sheet`, colonnes à partir de l'abscisse `cols_left`.
    fn of(fc: &FrameCtx<'_>, sheet: &TaskSheet<'_>, cursor: &TableCursor, cols_left: f32) -> Self {
        let visible: Vec<Column> = sheet.columns().iter().map(|(c, _)| *c).collect();
        let block = crate::block::resolve(fc.selected, cursor, sheet.rows, &visible);
        let span = |c: Column| column_span(sheet.columns(), cols_left, c);
        let span_x = match (block.columns.first(), block.columns.last()) {
            (Some(first), Some(last)) => span(*first).zip(span(*last)).map(|((l, _), (r, w))| (l, r + w)),
            _ => None,
        };
        let active_row = match block.active.0 {
            RowRef::Item(uid) => sheet.rows.display_of(uid).map(DisplayRow::index),
            RowRef::Blank => Some(sheet.rows.len()),
        };
        Self {
            multi: !block.is_single(),
            active_x: span(block.active.1),
            id_x: span(Column::Id),
            span_x,
            active_row,
            block,
        }
    }

    /// La ligne affichée `i` est dans le bloc.
    fn contains(&self, i: usize) -> bool {
        self.block.rows.binary_search(&DisplayRow::at(i)).is_ok()
    }

    /// La ligne `i` est teintée sur toute la largeur (lignes entières).
    fn full_row(&self, i: usize) -> bool {
        self.multi && self.block.kind == crate::selection::SelectionKind::Rows && self.contains(i)
    }

    /// Fonds de la ligne `i` (`row_rect`) : fond de base `base` (bande,
    /// survol), teinte du bloc (lignes entières : toute la largeur ;
    /// cellules : colonnes du bloc), cellule active laissée au fond de
    /// base, ID appuyé. Rend le nombre de formes.
    fn paint_row(
        &self,
        painter: &Painter,
        palette: &Palette,
        i: usize,
        row_rect: Rect,
        base: Option<egui::Color32>,
        drawn: &mut SelectionDrawn,
    ) -> usize {
        let mut shapes = 0_usize;
        let mut fill = |rect: Rect, color: egui::Color32| {
            if rect.is_finite() && rect.is_positive() {
                painter.rect_filled(rect, 0.0, color);
                shapes = shapes.saturating_add(1);
            }
        };
        let cell =
            |(l, w): (f32, f32)| Rect::from_min_max(pos2(l, row_rect.top()), pos2(l + w, row_rect.bottom()));
        let in_block = self.contains(i);
        if self.full_row(i) {
            fill(row_rect, palette.selection);
            drawn.full_rows.push(i);
        } else if let Some(color) = base {
            fill(row_rect, color);
        }
        if in_block && self.multi && self.block.kind == crate::selection::SelectionKind::Cells {
            if let Some((l, r)) = self.span_x {
                fill(cell((l, r - l)), palette.selection);
            }
            drawn.tinted.extend(self.block.columns.iter().map(|c| (i, *c)));
        }
        if in_block && self.multi && self.active_row == Some(i) {
            let under = base.unwrap_or(palette.background);
            if let Some(x) = self.active_x {
                fill(cell(x), under);
            }
            drawn.active = Some((i, self.block.active.1, under));
            drawn.tinted.retain(|&(r, c)| (r, c) != (i, self.block.active.1));
        }
        if in_block && self.block.active.0 != RowRef::Blank {
            if let Some(x) = self.id_x {
                fill(cell(x), palette.header_selected);
            }
            drawn.pressed_ids.push(i);
        }
        shapes
    }

    /// Bordures de 2 px du bloc (`selection_border`), une par suite de
    /// lignes voisines du bloc dans la plage dessinée ; une suite qui
    /// continue hors de la plage déborde d'une ligne (bord coupé par le
    /// clip, jamais dessiné au milieu du bloc). Rend le nombre de formes.
    fn paint_borders(
        &self,
        painter: &Painter,
        palette: &Palette,
        origin: Pos2,
        row_h: f32,
        range: &Range<usize>,
        drawn: &mut SelectionDrawn,
    ) -> usize {
        if !self.multi {
            return 0;
        }
        let clip = painter.clip_rect();
        let (left, right) = if self.block.kind == crate::selection::SelectionKind::Rows {
            (clip.left(), clip.right())
        } else {
            match self.span_x {
                Some(x) => x,
                None => return 0,
            }
        };
        let mut shapes = 0_usize;
        let mut run: Option<(usize, usize)> = None;
        let mut close = |(first, last): (usize, usize)| {
            let above = first == range.start && first > 0 && self.contains(first.saturating_sub(1));
            let below = last.saturating_add(1) == range.end && self.contains(last.saturating_add(1));
            let mut top = origin.y + index_to_f32(first) * row_h;
            let mut bottom = origin.y + index_to_f32(last.saturating_add(1)) * row_h;
            if above {
                top -= row_h;
            }
            if below {
                bottom += row_h;
            }
            let rect = Rect::from_min_max(pos2(left, top), pos2(right, bottom));
            if rect.is_finite() && rect.is_positive() && rect.intersects(clip) {
                painter.rect_stroke(
                    rect,
                    0.0,
                    Stroke::new(BLOCK_BORDER, palette.selection_border),
                    egui::StrokeKind::Inside,
                );
                drawn.borders.push(rect);
                shapes = shapes.saturating_add(1);
            }
        };
        for i in range.clone() {
            run = match (run, self.contains(i)) {
                (Some((first, _)), true) => Some((first, i)),
                (None, true) => Some((i, i)),
                (Some(r), false) => {
                    close(r);
                    None
                }
                (None, false) => None,
            };
        }
        if let Some(r) = run {
            close(r);
        }
        shapes
    }
}

/// Épaisseur de la bordure du bloc (H-UI-103).
const BLOCK_BORDER: f32 = 2.0;

/// Fond des lignes non sélectionnées de la table : bandes du style
/// (H-UI-53, J8 lot L11), alignées sur celles du Gantt ; sans bandes, le
/// zébrage de J7.
struct RowFill {
    /// Bandes du style.
    banding: RowBanding,
    /// Couleurs des deux bandes.
    band: [plannercraft_render::style::Rgb; 2],
    /// Fond du style (une bande de cette couleur n'est pas dessinée).
    bg: plannercraft_render::style::Rgb,
    /// Zébrage J7 (lignes impaires) ; aucun dans l'interface adoucie (lot
    /// U1, H-UI-168).
    zebra: Option<egui::Color32>,
    /// Phases du modèle (bandes de phase).
    phases: std::sync::Arc<Vec<u8>>,
    /// Couleurs de phase des tâches (H-UI-65, J8 lot L15).
    phase_colors: std::sync::Arc<crate::theme::PhaseColors>,
    /// Encres du texte (texte, appuyé, atténué) et seuil du style : la
    /// teinte d'une ligne de phase les garde lisibles.
    inks: [plannercraft_render::style::Rgb; 3],
    /// Contraste minimal du texte (H-UI-45 : 7:1 en Contraste élevé).
    min: u32,
    /// Surlignage des tâches (finitions P2, H-UI-126).
    looks: std::sync::Arc<crate::theme::TextLooks>,
}

impl RowFill {
    /// Police des cellules (Inter si le style la demande, J8 lot L11) et
    /// fonds des lignes de l'image.
    fn of(ui: &egui::Ui, fc: &FrameCtx<'_>) -> (egui::FontId, Self) {
        let mut font = cell_font(ui);
        let screen = &fc.task_view.style;
        if screen.inter() {
            font.family = egui::FontFamily::Name(crate::theme::PLAN_FAMILY.into());
        }
        let tokens = screen.style.tokens(screen.variant);
        let fill = Self {
            banding: screen.style.grid.rows,
            band: tokens.band,
            bg: tokens.bg,
            zebra: (!screen.soft).then(|| fc.palette.background.lerp_to_gamma(fc.palette.header_bg, 0.6)),
            phases: std::sync::Arc::clone(&screen.phases),
            phase_colors: std::sync::Arc::clone(&screen.phase_colors),
            inks: [tokens.text, tokens.text_strong, tokens.text_muted],
            min: plannercraft_render::style::min_text_contrast(&screen.style),
            looks: std::sync::Arc::clone(&screen.text_looks),
        };
        (font, fill)
    }

    /// Fond de la ligne affichée `i` (`None` : fond de la table).
    fn fill(&self, rows: VisibleRows<'_>, i: usize) -> Option<egui::Color32> {
        // Surlignage d'une tâche (finitions P2, H-UI-126) : prime sur la
        // couleur de phase et les bandes.
        if !self.looks.is_empty()
            && let Some(row) = rows.row(crate::row_view::DisplayRow::at(i))
            && row.kind != RowKind::Null
            && let Some(rgb) = self.looks.get(&row.uid.get()).and_then(|l| l.fill)
        {
            return Some(egui::Color32::from_rgb(rgb.0, rgb.1, rgb.2));
        }
        // Ligne d'une tâche colorée (H-UI-65) : teinte lisible de sa couleur.
        if !self.phase_colors.is_empty()
            && let Some(row) = rows.row(crate::row_view::DisplayRow::at(i))
            && let Some((rgb, true)) = self.phase_colors.get(&row.uid).copied()
        {
            let tint = plannercraft_render::style::readable_tint(rgb, self.bg, &self.inks, self.min);
            return Some(egui::Color32::from_rgb(tint.0, tint.1, tint.2));
        }
        if self.banding == RowBanding::None {
            return self.zebra.filter(|_| !i.is_multiple_of(2));
        }
        let band = super::gantt::band_of(self.banding, rows, i, &self.phases)?;
        let rgb = if band == 0 { self.band[0] } else { self.band[1] };
        (rgb != self.bg).then(|| egui::Color32::from_rgb(rgb.0, rgb.1, rgb.2))
    }
}

/// Position du triangle de repli d'une ligne.
struct TrianglePlace {
    /// Rectangle écran de la ligne.
    row_rect: Rect,
    /// Bord gauche écran de la première colonne (défilement déduit).
    cols_left: f32,
    /// Zone visible de la table.
    clip: Rect,
}

/// Triangle de repli d'une récapitulative (H-UI-34, J8 lot L9) : dans le
/// cran de retrait qui précède son nom, pointe vers la droite si elle est
/// repliée (à la main ou par le niveau affiché), vers le bas sinon ; un
/// clic replie ou déplie (`TaskViewOp::ToggleCollapsed`). Libellé
/// d'accessibilité « Déplier … » / « Replier … ». Rend le nombre de formes.
fn outline_triangle(
    ui: &egui::Ui,
    painter: &Painter,
    fc: &mut FrameCtx<'_>,
    sheet: &TaskSheet<'_>,
    row: &Row,
    i: usize,
    at: &TrianglePlace,
) -> usize {
    let metrics = fc.metrics;
    let Some((left, w)) = column_span(sheet.columns(), at.cols_left, Column::Name) else {
        return 0;
    };
    let x0 = left + metrics.pad_x + grid::name_indent(metrics.indent, row.level, w);
    let rect = Rect::from_min_size(
        pos2(x0, at.row_rect.top()),
        vec2(metrics.indent, at.row_rect.height()),
    );
    let hit = rect.intersect(at.clip);
    if !rect.is_finite() || !hit.is_positive() {
        return 0;
    }
    let folded = fc.task_view.is_folded(row.uid, row.level);
    let c = rect.center();
    let r = 4.0_f32;
    let points = if folded {
        vec![
            pos2(c.x - r * 0.6, c.y - r),
            pos2(c.x - r * 0.6, c.y + r),
            pos2(c.x + r * 0.8, c.y),
        ]
    } else {
        vec![
            pos2(c.x - r, c.y - r * 0.6),
            pos2(c.x + r, c.y - r * 0.6),
            pos2(c.x, c.y + r * 0.8),
        ]
    };
    painter.add(egui::Shape::convex_polygon(points, fc.palette.text, Stroke::NONE));
    let response = ui.interact(
        hit,
        ui.make_persistent_id(("plannercraft-outline", row.uid)),
        Sense::click(),
    );
    let texts = fc.texts;
    let name = sheet.rows.text(DisplayRow::at(i), Column::Name).to_owned();
    response.widget_info(|| {
        let id = if folded {
            TextId::OutlineExpandRow
        } else {
            TextId::OutlineCollapseRow
        };
        let label = texts.get_args(id, &[("name", Arg::Str(name.clone().into()))]);
        egui::WidgetInfo::labeled(egui::Role::Button, true, label)
    });
    if response.clicked() {
        fc.actions
            .push(Action::TaskView(super::task_view::TaskViewOp::ToggleCollapsed(
                row.uid,
            )));
    }
    1
}

/// Place d'une ligne d'en-tête de groupe.
struct GroupRowPlace {
    /// Rectangle écran de la ligne.
    row_rect: Rect,
    /// Bord gauche écran de la première colonne (défilement déduit).
    cols_left: f32,
    /// Découpe du corps.
    clip: Rect,
    /// Mesures des cellules.
    place: RowPlace,
}

/// Ligne d'en-tête d'un groupe (finitions P3, H-UI-129) : fond de
/// l'en-tête, Nom (valeur et nombre de tâches), Début et Fin du groupe en
/// texte appuyé, triangle de repli devant le Nom ; toute la ligne est un
/// bouton « Replier le groupe … » / « Déplier le groupe … » (ni sélection
/// ni saisie). Rend le nombre de formes.
#[allow(
    clippy::too_many_arguments,
    reason = "contexte d'image découpé en emprunts disjoints"
)]
fn paint_group_row(
    ui: &egui::Ui,
    painter: &Painter,
    fc: &mut FrameCtx<'_>,
    sheet: &TaskSheet<'_>,
    header: &crate::row_view::GroupHeader,
    i: usize,
    at: &GroupRowPlace,
    font: &egui::FontId,
) -> usize {
    let palette = fc.palette;
    let metrics = fc.metrics;
    let mut shapes = 0_usize;
    painter.rect_filled(at.row_rect, 0.0, palette.header_bg);
    painter.hline(
        at.row_rect.x_range(),
        at.row_rect.bottom() - 0.5,
        Stroke::new(1.0, palette.grid),
    );
    shapes = shapes.saturating_add(2);
    shapes = shapes.saturating_add(grid::paint_cells(painter, sheet, palette, i, at.place, font));
    if let Some((left, _)) = column_span(sheet.columns(), at.cols_left, Column::Name) {
        let x0 = left + metrics.pad_x;
        let rect = Rect::from_min_size(
            pos2(x0, at.row_rect.top()),
            vec2(metrics.indent, at.row_rect.height()),
        );
        if rect.is_finite() && rect.intersect(at.clip).is_positive() {
            let c = rect.center();
            let r = 4.0_f32;
            let points = if header.collapsed {
                vec![
                    pos2(c.x - r * 0.6, c.y - r),
                    pos2(c.x - r * 0.6, c.y + r),
                    pos2(c.x + r * 0.8, c.y),
                ]
            } else {
                vec![
                    pos2(c.x - r, c.y - r * 0.6),
                    pos2(c.x + r, c.y - r * 0.6),
                    pos2(c.x, c.y + r * 0.8),
                ]
            };
            painter.add(egui::Shape::convex_polygon(points, palette.text, Stroke::NONE));
            shapes = shapes.saturating_add(1);
        }
    }
    let hit = at.row_rect.intersect(at.clip);
    if hit.is_positive() {
        let response = ui.interact(
            hit,
            ui.make_persistent_id(("plannercraft-group-row", i)),
            Sense::click(),
        );
        let texts = fc.texts;
        let collapsed = header.collapsed;
        let name = header.name.clone();
        response.widget_info(|| {
            let id = if collapsed {
                TextId::GroupExpand
            } else {
                TextId::GroupCollapse
            };
            let label = texts.get_args(id, &[("name", Arg::Str(name.clone().into()))]);
            egui::WidgetInfo::labeled(egui::Role::Button, true, label)
        });
        if response.clicked() {
            fc.actions.push(Action::Group(crate::group_view::GroupOp::Toggle(
                header.key.clone(),
            )));
            fc.actions.push(Action::FocusPane(Pane::Table));
        }
    }
    shapes
}

/// Zone cliquable de la ligne `i` (ou de la ligne vide) et son libellé
/// d'accessibilité (`row-label`, `blank-row-label`).
fn row_widget(
    ui: &egui::Ui,
    fc: &FrameCtx<'_>,
    rows: VisibleRows<'_>,
    hit: Rect,
    i: usize,
    selected: bool,
) -> Response {
    let response = ui.interact(
        hit,
        ui.make_persistent_id(("plannercraft-table-row", i)),
        Sense::click_and_drag(),
    );
    let texts = fc.texts;
    response.widget_info(|| {
        let d = DisplayRow::at(i);
        let label = match rows.row(d) {
            Some(row) => texts.get_args(
                TextId::RowLabel,
                &[
                    ("id", Arg::Count(i64::from(row.id))),
                    ("name", Arg::Str(rows.text(d, Column::Name).into())),
                ],
            ),
            None => texts.get(TextId::BlankRowLabel),
        };
        egui::WidgetInfo::selected(egui::Role::Row, true, selected, label)
    });
    response
}

/// Widget focalisable de la table (sans clic : les lignes, dessinées
/// au-dessus, reçoivent les clics) ; verrou de focus rappelé tant qu'il a
/// le focus.
fn focus_widget(ui: &egui::Ui, fc: &FrameCtx<'_>, clip: Rect) {
    let response = ui.interact(clip, table_id(), Sense::focusable_noninteractive());
    let texts = fc.texts;
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Table, true, texts.get(TextId::TableLabel)));
    if response.has_focus() {
        ui.memory_mut(|memory| memory.set_focus_lock_filter(table_id(), NAV_FOCUS_FILTER));
    }
}

/// Position des lignes et des colonnes à l'écran pour une image.
struct Place<'r> {
    /// Coin haut-gauche écran de la ligne 0.
    origin: Pos2,
    /// Abscisse écran du bord gauche de la première colonne visible
    /// (défilement horizontal déduit).
    cols_left: f32,
    /// Lignes dessinées.
    range: &'r Range<usize>,
    /// Police des cellules (celle de la zone de saisie, H-UI-85).
    font: &'r egui::FontId,
}

/// Cadre de 2 px de la cellule courante (couleur `Palette::cursor`) et
/// son nœud d'accessibilité, si elle est dans la plage dessinée, sa colonne
/// visible et aucune saisie ouverte ; rend le nombre de formes ajoutées.
fn paint_cursor(
    ui: &egui::Ui,
    painter: &Painter,
    fc: &FrameCtx<'_>,
    sheet: &TaskSheet<'_>,
    edit: &TableEdit,
    cursor: &TableCursor,
    at: &Place<'_>,
) -> usize {
    if edit.editing.is_some() {
        return 0;
    }
    let clip = painter.clip_rect();
    let Some(index) = grid::row_index(sheet, cursor.row).filter(|i| at.range.contains(i)) else {
        return 0;
    };
    let Some((left, w)) = column_span(sheet.columns(), at.cols_left, cursor.column) else {
        return 0;
    };
    let top = at.origin.y + index_to_f32(index) * fc.metrics.row_h;
    let rect = Rect::from_min_size(pos2(left, top), vec2(w, fc.metrics.row_h));
    if !grid::paint_cursor_frame(painter, rect, fc.palette) {
        return 0;
    }
    let hit = rect.intersect(clip);
    if hit.is_positive() {
        let response = ui.interact(hit, Id::unique("plannercraft.table.cursor"), Sense::hover());
        let texts = fc.texts;
        let rows = sheet.rows;
        let column = texts.get(cursor.column.title());
        response.widget_info(|| {
            let d = DisplayRow::at(index);
            let label = match rows.row(d) {
                Some(row) => {
                    let value = rows.text(d, cursor.column);
                    let value = if value.is_empty() {
                        texts.get(TextId::NavCursorEmptyValue)
                    } else {
                        value.to_owned()
                    };
                    texts.get_args(
                        TextId::NavCursorLabel,
                        &[
                            ("id", Arg::Count(i64::from(row.id))),
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

/// Cellule cliquée pendant une saisie : rejointe seulement si la saisie
/// est acceptée (H-UI-86).
#[derive(Clone, Copy, Debug)]
struct EditClick {
    /// Tâche de la ligne cliquée (`None` : ligne vide).
    uid: Option<TaskUid>,
    /// Colonne sous le pointeur.
    column: Option<Column>,
}

/// Ligne sous le pointeur dans une image : réponse de sa zone, indice
/// affiché, bord gauche écran de la première colonne.
struct RowHit<'r> {
    /// Réponse de la zone de la ligne.
    response: &'r Response,
    /// Ligne affichée (`rows.len()` : la ligne vide).
    i: usize,
    /// Bord gauche écran de la première colonne (défilement déduit).
    left: f32,
}

/// Cible d'un appui dans la colonne `column` : l'ID est l'en-tête de ligne
/// (H-UI-89), le reste une cellule.
fn press_target(column: Column) -> PressTarget {
    if column == Column::Id {
        PressTarget::RowHeader
    } else {
        PressTarget::Cell
    }
}

/// Appui sans modificateur sur l'ID d'une ligne déjà sélectionnée en lignes
/// entières : différé (ADR-0018 D6, H-UI-89). Relâché sans glisser, il la
/// sélectionne seule ; glissé, il commence le déplacement des lignes
/// (H-UI-94, [`track_row_drag`]).
fn defers_press(fc: &FrameCtx<'_>, uid: Option<TaskUid>, target: PressTarget, mode: PressMode) -> bool {
    target == PressTarget::RowHeader
        && mode == PressMode::Replace
        && fc.selected.kind() == crate::selection::SelectionKind::Rows
        && uid.is_some_and(|uid| fc.selected.contains(uid))
}

/// Appui, clic et double-clic sur la ligne `hit.i` (ou sur la ligne vide),
/// ADR-0018 D1 :
/// - **appui** (image de l'appui, H-UI-88) : `SelectOp::Press` sur la
///   cellule sous le point d'appui (ID : lignes entières, H-UI-89 ; mode
///   selon Maj et Ctrl/Cmd), focus de la table, glisser de sélection
///   ouvert (`select_drag`) ; un bougé sous le seuil reste la même cellule ;
/// - **clic sans appui vu** (accessibilité) ou appui différé relâché
///   ([`defers_press`]) : même appui au relâcher ; appui différé glissé :
///   déplacement des lignes (H-UI-94, `drags.rows`) ; survol de l'ID d'une
///   telle ligne : curseur de déplacement (H-UI-103) ;
/// - **double-clic** : sur l'ID, « Informations sur la tâche » (H-UI-19) ;
///   ailleurs, la saisie de la cellule.
///
/// Saisie ouverte : rien de tout cela, l'appui (ou le clic) est rendu à
/// [`edit_cells`] qui valide d'abord (H-UI-86 : jamais de saisie ouverte
/// sur une autre cellule que la cellule active ; aucun glisser).
fn row_pointer(
    ui: &egui::Ui,
    fc: &mut FrameCtx<'_>,
    sheet: &TaskSheet<'_>,
    edit: &mut TableEdit,
    cursor: &mut TableCursor,
    drags: Drags<'_>,
    hit: &RowHit<'_>,
) -> Option<EditClick> {
    let Drags {
        select: select_drag,
        rows: row_drag,
    } = drags;
    let response = hit.response;
    let uid = sheet.key(hit.i);
    let row = RowRef::from_selection(uid);
    let column_of = |p: Option<Pos2>| p.and_then(|p| column_at(sheet.columns(), hit.left, p.x));
    if edit.editing.is_some() {
        // Dès l'appui (ou au clic sans appui vu) : la saisie est validée
        // d'abord, sans glisser de sélection.
        let pressed = pressed_now(ui, response);
        return (pressed || response.clicked()).then(|| EditClick {
            uid,
            column: column_of(if pressed {
                ui.input(|i| i.pointer.press_origin())
            } else {
                response.interact_pointer_pos()
            }),
        });
    }
    let (modifiers, press_origin) = ui.input(|i| (i.modifiers, i.pointer.press_origin()));
    let mode = press_mode(modifiers);
    // Vue regroupée (finitions P3) : la ligne appuyée devient celle du
    // curseur (une tâche peut être sous plusieurs groupes).
    if fc.task_view.group_by.is_some() && (pressed_now(ui, response) || response.clicked()) {
        fc.actions
            .push(Action::Group(crate::group_view::GroupOp::CursorRow(hit.i)));
    }
    if pressed_now(ui, response) {
        let column = column_of(press_origin).unwrap_or(cursor.column);
        let target = press_target(column);
        if !defers_press(fc, uid, target, mode) {
            fc.actions.push(Action::SelectOp(SelectOp::Press {
                row,
                column,
                target,
                mode,
            }));
            *select_drag = Some(SelectDrag {
                target,
                last: (row, column),
                armed: false,
                toggle: mode == PressMode::Toggle,
            });
        }
        fc.actions.push(Action::FocusPane(Pane::Table));
    } else if response.clicked() && select_drag.is_none() {
        let column = column_of(response.interact_pointer_pos()).unwrap_or(cursor.column);
        let target = press_target(column);
        fc.actions.push(Action::SelectOp(SelectOp::Press {
            row,
            column,
            target,
            mode,
        }));
        fc.actions.push(Action::FocusPane(Pane::Table));
    } else if response.drag_started_by(egui::PointerButton::Primary)
        && select_drag.is_none()
        && row_drag.is_none()
        && defers_press(
            fc,
            uid,
            press_target(column_of(press_origin).unwrap_or(Column::Name)),
            mode,
        )
    {
        // Appui différé puis glisser au-delà du seuil : déplacement des
        // lignes sélectionnées (H-UI-94), le clavier reste à la table.
        start_row_drag(fc, sheet.rows, row_drag, hit.i);
        fc.actions.push(Action::FocusPane(Pane::Table));
    }
    if select_drag.is_none() && row_drag.is_none() && response.hovered() {
        let over_id = column_of(response.hover_pos()) == Some(Column::Id);
        if over_id && defers_press(fc, uid, PressTarget::RowHeader, PressMode::Replace) {
            row_move_hover(ui, fc, response);
        }
    }
    // Double-clic (ou triple : deux clics suivant de près un premier) :
    // sur l'ID, « Informations sur la tâche » (H-UI-19, J7 lot L9) ;
    // ailleurs, la saisie de la cellule.
    if (response.double_clicked() || response.triple_clicked()) && !fc.window_open {
        if let Some(column) = column_of(response.interact_pointer_pos()) {
            cursor.column = column;
        }
        match uid {
            Some(uid) if cursor.column == Column::Id => {
                fc.actions
                    .push(Action::ShowDialog(DialogKind::TaskInformation(uid)));
            }
            _ => open_editor(fc, sheet.rows, edit, cursor.column, uid, hit.i),
        }
    }
    None
}

/// Glissers de la table tenus d'une image à l'autre (`ViewTransient`) :
/// sélection (H-UI-88 à H-UI-90) et déplacement de lignes (H-UI-94).
pub struct Drags<'d> {
    /// Glisser de sélection.
    pub select: &'d mut Option<SelectDrag>,
    /// Déplacement de lignes par leur ID.
    pub rows: &'d mut Option<RowDrag>,
}

/// Survol de l'ID d'une ligne sélectionnée en lignes entières (H-UI-103) :
/// curseur de déplacement et info-bulle du geste ; sous tri ou filtre,
/// curseur interdit et raison (H-UI-32).
fn row_move_hover(ui: &egui::Ui, fc: &FrameCtx<'_>, response: &Response) {
    let (icon, tip) = if fc.task_view.group_by.is_some() {
        (egui::CursorIcon::NotAllowed, TextId::ViewGroupLocked)
    } else if fc.task_view.state.reorder_locked() {
        (
            egui::CursorIcon::NotAllowed,
            crate::structure::StructureRefusal::ViewReorderLocked.move_text_id(),
        )
    } else {
        (egui::CursorIcon::Move, crate::structure::ROW_MOVE_TIP)
    };
    ui.ctx().set_cursor_icon(icon);
    drop(response.clone().on_hover_text(fc.texts.get(tip)));
}

/// Refus d'un lâcher devant la ligne affichée `before` (`None` : en fin),
/// d'après la sélection de l'image (H-UI-94) : message du refus, `None` si
/// le lâcher est permis.
fn drop_refusal(fc: &FrameCtx<'_>, rows: VisibleRows<'_>, before: Option<DisplayRow>) -> Option<TextId> {
    let doc = fc.doc?;
    // Regroupement (finitions P3, H-UI-130) : aucun déplacement de lignes.
    if fc.task_view.group_by.is_some() {
        return Some(TextId::ViewGroupLocked);
    }
    let target = before.and_then(|d| rows.uid(d));
    crate::structure::move_refusal(
        doc.project(),
        fc.selected,
        target,
        fc.task_view.state.reorder_locked(),
    )
    .map(crate::structure::StructureRefusal::move_text_id)
}

/// Ouvre le déplacement des lignes sélectionnées (H-UI-94) depuis la ligne
/// affichée `i` : tâches déplacées ([`crate::structure::move_roots`]),
/// trait d'insertion devant la ligne appuyée (dans son propre bloc : refusé
/// tant que le pointeur n'en sort pas).
fn start_row_drag(fc: &FrameCtx<'_>, rows: VisibleRows<'_>, row_drag: &mut Option<RowDrag>, i: usize) {
    let Some(doc) = fc.doc else {
        return;
    };
    let insert_before = (i < rows.len()).then(|| DisplayRow::at(i));
    *row_drag = Some(RowDrag {
        uids: crate::structure::move_roots(doc.project(), fc.selected),
        insert_before,
        refusal: drop_refusal(fc, rows, insert_before),
    });
}

/// Intervalle d'insertion le plus proche de l'ordonnée écran `y` : `k`
/// désigne le trait entre les lignes affichées `k − 1` et `k` (`len` : après
/// la dernière) ; `y` borné au corps ; `None` pour une valeur non finie.
fn insertion_gap(y: f32, at: &BodyPlace, row_h: f32, len: usize) -> Option<usize> {
    let y = bounded(y, at.clip.top(), at.clip.bottom());
    let rel = (y - at.origin.y) / row_h + 0.5;
    rel.is_finite()
        .then(|| crate::geom::f32_to_index(rel.floor()).min(len))
}

/// Déplacement de lignes en cours (H-UI-94, H-UI-103) : à chaque image, le
/// trait d'insertion de 2 px (`Palette::cursor`) suit l'intervalle le plus
/// proche du pointeur (borné au corps ; le défilement automatique est mené
/// par `main_view::autoscroll`) ; le refus n'est recalculé que quand
/// l'intervalle change ; curseur de déplacement, ou interdit avec la
/// raison en info-bulle. Échap annule ; au relâcher, `Action::MoveRows`
/// (la tâche devant laquelle lâcher, `None` : en fin), que le réducteur
/// valide de nouveau (refus au bandeau, rien d'écrit). Un bouton relâché
/// hors de la table (aucun relâcher vu) annule. Rend le nombre de formes.
fn track_row_drag(
    ui: &egui::Ui,
    painter: &Painter,
    fc: &mut FrameCtx<'_>,
    rows: VisibleRows<'_>,
    row_drag: &mut Option<RowDrag>,
    at: &BodyPlace,
) -> usize {
    let Some(d) = row_drag.as_mut() else {
        return 0;
    };
    if ui.input(|i| i.key_pressed(Key::Escape)) {
        *row_drag = None;
        fc.actions.push(Action::FocusPane(Pane::Table));
        return 0;
    }
    let (down, released, at_pos) = ui.input(|i| {
        (
            i.pointer.primary_down(),
            i.pointer.primary_released(),
            i.pointer.interact_pos(),
        )
    });
    let row_h = fc.metrics.row_h;
    let len = rows.len();
    let gap = at_pos
        .filter(|p| p.is_finite())
        .and_then(|p| insertion_gap(p.y, at, row_h, len));
    if let Some(gap) = gap {
        let before = (gap < len).then(|| DisplayRow::at(gap));
        if before != d.insert_before {
            d.insert_before = before;
            d.refusal = drop_refusal(fc, rows, before);
        }
    }
    if !down {
        if released && let Some(done) = row_drag.take() {
            let before = done.insert_before.and_then(|d| rows.uid(d));
            fc.actions.push(Action::MoveRows { before });
            fc.actions.push(Action::FocusPane(Pane::Table));
        }
        *row_drag = None;
        return 0;
    }
    let ctx = ui.ctx();
    match d.refusal {
        Some(reason) => {
            ctx.set_cursor_icon(egui::CursorIcon::NotAllowed);
            let text = fc.texts.get(reason);
            drop(
                egui::Tooltip::always_open(
                    ctx.clone(),
                    ui.layer_id(),
                    Id::unique("plannercraft.table.row-drag"),
                    egui::PopupAnchor::Pointer,
                )
                .show(|ui| ui.label(text)),
            );
        }
        None => ctx.set_cursor_icon(egui::CursorIcon::Move),
    }
    let k = d.insert_before.map_or(len, DisplayRow::index);
    let y = at.origin.y + index_to_f32(k) * row_h;
    if !y.is_finite() || y < at.clip.top() - 1.0 || y > at.clip.bottom() + 1.0 {
        return 0;
    }
    painter.hline(at.clip.x_range(), y, Stroke::new(2.0, fc.palette.cursor));
    1
}

/// Position du corps à l'écran pour une image.
struct BodyPlace {
    /// Coin haut-gauche écran de la ligne 0.
    origin: Pos2,
    /// Bord gauche écran de la première colonne (défilement déduit).
    cols_left: f32,
    /// Zone visible du corps.
    clip: Rect,
}

/// `v` ramené dans `[lo, hi]` sans jamais paniquer (`hi < lo` ⇒ `lo`).
fn bounded(v: f32, lo: f32, hi: f32) -> f32 {
    v.min(hi).max(lo)
}

/// Seuil du glisser de sélection (H-UI-88) : arme `d` dès que `p` est à
/// au moins `max_click_dist` d'egui (6 points par défaut) du point d'appui,
/// et rend vrai tant qu'il est armé. Sans point d'appui connu (clic
/// d'accessibilité), valeur non finie ou appui Ctrl/Cmd (`toggle`) :
/// jamais armé.
pub(crate) fn arm_select_drag(ui: &egui::Ui, d: &mut SelectDrag, p: Pos2) -> bool {
    if d.toggle {
        return false;
    }
    if !d.armed {
        let threshold = ui.ctx().options(|o| o.input_options.max_click_dist);
        d.armed = ui
            .input(|i| i.pointer.press_origin())
            .filter(|o| o.is_finite())
            .is_some_and(|o| o.distance(p) >= threshold);
    }
    d.armed
}

/// Glisser de sélection ouvert par un appui sur une cellule ou un ID
/// (H-UI-88, H-UI-89) : tant que le bouton est tenu, la cellule sous le
/// pointeur, **bornée au corps** (au-dessus : première ligne visible ; au-
/// dessous ou sur la ligne vide : dernière ligne ; à gauche, à droite : la
/// colonne du bord), devient l'extrémité du bloc (`SelectOp::DragTo`,
/// poussé seulement quand elle change). Un glisser commencé sur l'ID reste
/// en lignes entières (colonne ID). Rien tant que le pointeur n'a pas
/// franchi le seuil de glisser ([`arm_select_drag`]). Toute valeur non
/// finie : rien.
fn track_select_drag(
    ui: &egui::Ui,
    fc: &mut FrameCtx<'_>,
    sheet: &TaskSheet<'_>,
    select_drag: &mut Option<SelectDrag>,
    at: &BodyPlace,
) {
    let Some(d) = select_drag
        .as_mut()
        .filter(|d| matches!(d.target, PressTarget::Cell | PressTarget::RowHeader))
    else {
        return;
    };
    let Some(p) = ui
        .input(|i| {
            i.pointer
                .primary_down()
                .then(|| i.pointer.interact_pos())
                .flatten()
        })
        .filter(|p| p.is_finite())
    else {
        return;
    };
    if !arm_select_drag(ui, d, p) {
        return;
    }
    let row_h = fc.metrics.row_h;
    let len = sheet.len().saturating_add(1);
    let y = bounded(p.y, at.clip.top(), at.clip.bottom() - 0.5);
    let index = crate::geom::row_at(y - at.origin.y, row_h, len).unwrap_or(len.saturating_sub(1));
    // En-tête de groupe sous le pointeur (finitions P3) : le bloc ne change
    // pas (un en-tête n'est ni une tâche ni la ligne vide).
    if sheet.is_header(index) {
        return;
    }
    let row = grid::row_ref(sheet, index);
    let column = if d.target == PressTarget::RowHeader {
        d.last.1
    } else {
        let width: f32 = sheet.columns().iter().map(|(_, w)| f32::from(*w)).sum();
        let x = bounded(
            p.x,
            at.cols_left.max(at.clip.left()),
            (at.cols_left + width).min(at.clip.right()) - 0.5,
        );
        column_at(sheet.columns(), at.cols_left, x).unwrap_or(d.last.1)
    };
    if (row, column) != d.last {
        d.last = (row, column);
        fc.actions
            .push(Action::SelectOp(SelectOp::DragTo { row, column }));
    }
}

/// Ouvre la saisie de la cellule (`uid`, `column`) de la ligne affichée
/// `i` avec son texte initial (F2, double-clic) ; rien sur une colonne
/// calculée.
fn open_editor(
    fc: &FrameCtx<'_>,
    rows: VisibleRows<'_>,
    edit: &mut TableEdit,
    column: Column,
    uid: Option<TaskUid>,
    i: usize,
) {
    let Some(doc) = fc.doc else { return };
    if !editable(column) {
        return;
    }
    let model_index = rows.model_index_or_blank(DisplayRow::at(i));
    let text = initial_text(doc, rows.model(), model_index, column, fc.texts.language());
    edit.start(EditState::open(uid, column, text));
}

/// Ligne courante pour la saisie (ligne affichée) : tâche sélectionnée, ou
/// ligne vide sans sélection.
fn current_row(fc: &FrameCtx<'_>, rows: VisibleRows<'_>) -> Option<(Option<TaskUid>, usize)> {
    match fc.selection {
        Some(uid) => rows.display_of(uid).map(|d| (Some(uid), d.index())),
        None => Some((None, rows.len())),
    }
}

/// Texte frappé dans cette image hors modificateur Ctrl/Cmd (caractères
/// imprimables seulement), retiré des événements ; vide sinon.
pub(crate) fn take_typed(ctx: &egui::Context) -> String {
    ctx.input_mut(|input| {
        let mut typed = String::new();
        if input.modifiers.command || input.modifiers.ctrl || input.modifiers.mac_cmd {
            return typed;
        }
        input.events.retain(|event| match event {
            Event::Text(text) if !text.is_empty() && !text.chars().any(char::is_control) => {
                typed.push_str(text);
                false
            }
            _ => true,
        });
        typed
    })
}

/// F2, Retour arrière ou frappe d'un caractère (hors champ de texte ou
/// autre widget focalisé, menu ouvert ou modificateur Ctrl/Cmd) : ouvre la
/// saisie de la cellule courante. Le caractère frappé remplace la valeur ;
/// Retour arrière l'efface (saisie vide).
fn open_from_keys(
    ui: &egui::Ui,
    fc: &FrameCtx<'_>,
    sheet: &TaskSheet<'_>,
    edit: &mut TableEdit,
    cursor: &TableCursor,
) {
    let ctx = ui.ctx();
    if edit.editing.is_some() || fc.window_open || focus_elsewhere(ctx) || ctx.any_popup_open() {
        return;
    }
    let Some((uid, i)) = current_row(fc, sheet.rows) else {
        return;
    };
    let column = cursor.column;
    if !editable(column) || !sheet.columns().iter().any(|(c, _)| *c == column) {
        return;
    }
    if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::F2)) {
        open_editor(fc, sheet.rows, edit, column, uid, i);
        return;
    }
    // Retour arrière (« delete » d'un Mac) : saisie vide de la cellule, à
    // valider comme une frappe (relecture finale J7, F6 ; Cmd+delete
    // supprime la tâche).
    if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Backspace)) {
        edit.start(EditState::typed(uid, column, ""));
        return;
    }
    // Maj+Espace (lignes entières, H-UI-92) : l'espace de l'appui n'ouvre
    // pas de saisie.
    drop_chorded_space(ctx);
    let typed = take_typed(ctx);
    if !typed.is_empty() {
        edit.start(EditState::typed(uid, column, &typed));
    }
}

/// Retire de l'image le texte espace d'un Espace frappé avec Maj (ou Ctrl,
/// Cmd) : raccourci de sélection, jamais une frappe (H-UI-92).
fn drop_chorded_space(ctx: &egui::Context) {
    ctx.input_mut(|input| {
        let chorded = input.raw.events.iter().any(|event| {
            matches!(
                event,
                Event::Key { key: Key::Space, pressed: true, modifiers, .. }
                    if modifiers.shift || modifiers.ctrl || modifiers.command
            )
        });
        if chorded {
            input
                .events
                .retain(|event| !matches!(event, Event::Text(text) if text == " "));
        }
    });
}

/// Saisie de la cellule ouverte : zone de saisie dessinée dans la cellule
/// (H-UI-85), message d'erreur dessous, fin de saisie. Entrée valide et
/// descend, Maj+Entrée valide et monte, Tab valide et passe à la cellule
/// visible suivante, Échap abandonne ; un défilement ou la perte du focus
/// valident sur place ; un clic sur une autre cellule (`click`) valide puis
/// la rend active (H-UI-86). Une saisie refusée
/// ([`commit_edit`]) reste ouverte avec son message ; acceptée, son lot est
/// poussé dans `Action::ExecOn` (refus du registre : bandeau), suivi du
/// déplacement du curseur (`Action::Nav`, appliqué après le lot).
#[allow(
    clippy::too_many_arguments,
    reason = "contexte d'image découpé en emprunts disjoints"
)]
fn edit_cells(
    ui: &mut egui::Ui,
    fc: &mut FrameCtx<'_>,
    sheet: &TaskSheet<'_>,
    edit: &mut TableEdit,
    cursor: &mut TableCursor,
    at: &Place<'_>,
    clip: Rect,
    click: Option<EditClick>,
) {
    let Some(state) = edit.editing.as_ref() else {
        return;
    };
    let column = state.column;
    let index = grid::row_index(sheet, RowRef::from_selection(state.uid));
    let span = column_span(sheet.columns(), at.cols_left, column);
    let (Some(index), Some((cell_left, cell_w))) = (index, span) else {
        // Tâche disparue ou colonne masquée : saisie abandonnée.
        close_editor(ui, edit);
        return;
    };
    // Zone de saisie sans le focus (pas encore dessinée : ligne hors de la
    // plage, ou focus demandé pour cette image) : la frappe va au tampon,
    // rien n'est perdu (relecture finale J7, F2).
    if !ui.memory(|memory| memory.has_focus(grid::editor_id())) {
        absorb_keys(ui, edit);
    }
    let mut finish = finish_keys(ui);
    if finish.is_none() && (click.is_some() || grid::scrolled(ui)) {
        finish = Some(Finish::Commit(Move::Stay));
    }
    if at.range.contains(&index) {
        let top = at.origin.y + index_to_f32(index) * fc.metrics.row_h;
        let rect = Rect::from_min_size(pos2(cell_left, top), vec2(cell_w, fc.metrics.row_h));
        let look = grid::EditorLook::of(
            sheet,
            (index, column),
            fc.palette,
            at.font.clone(),
            fc.metrics.pad_x,
            fc.metrics.indent,
            cell_w,
        );
        let lost = show_editor(ui, fc, edit, rect, clip, look);
        if finish.is_none() && lost {
            finish = Some(Finish::Commit(Move::Stay));
        }
    }
    match finish {
        None => {}
        Some(Finish::Cancel) => {
            // Échap : la cellule éditée reste la cellule active.
            if let Some(state) = edit.editing.as_ref() {
                cursor.column = state.column;
            }
            close_editor(ui, edit);
            // Échap rend la main à la table (navigation au clavier).
            fc.actions.push(Action::FocusPane(Pane::Table));
        }
        Some(Finish::Commit(motion)) => commit(ui, fc, edit, cursor, motion, click),
        Some(Finish::FillTyped) => {
            if let Some(state) = edit.editing.as_ref() {
                fc.actions
                    .push(Action::Cells(CellOp::FillTyped(state.buffer.clone())));
            }
        }
    }
}

/// Frappe reçue alors que la zone de saisie n'a pas encore le focus :
/// caractères ajoutés au tampon, Retour arrière en retire un par appui.
fn absorb_keys(ui: &egui::Ui, edit: &mut TableEdit) {
    let Some(state) = edit.editing.as_mut() else {
        return;
    };
    let typed = take_typed(ui.ctx());
    let erased = ui.input_mut(|input| input.count_and_consume_key(Modifiers::NONE, Key::Backspace));
    if typed.is_empty() && erased == 0 {
        return;
    }
    state.buffer.push_str(&typed);
    for _ in 0..erased {
        state.buffer.pop();
    }
    state.error = None;
}

/// Dessine la zone de saisie dans `rect` (et l'erreur dessous) ; rend vrai
/// si elle vient de perdre le focus.
fn show_editor(
    ui: &mut egui::Ui,
    fc: &FrameCtx<'_>,
    edit: &mut TableEdit,
    rect: Rect,
    clip: Rect,
    look: grid::EditorLook,
) -> bool {
    let TableEdit {
        editing,
        focus_pending,
        ..
    } = edit;
    let Some(state) = editing.as_mut() else {
        return false;
    };
    if !rect.is_finite() || !rect.is_positive() {
        return false;
    }
    let label = fc.texts.get_args(
        TextId::CellEditLabel,
        &[("column", Arg::Str(fc.texts.get(state.column.title()).into()))],
    );
    let spec = EditorSpec {
        rect,
        clip,
        label,
        look,
    };
    let out = grid::show_editor(ui, &mut state.buffer, focus_pending, &spec);
    if out.changed {
        state.error = None;
    }
    if let Some(error) = &state.error {
        grid::show_cell_error(ui.ctx(), rect, error.message(fc.texts), fc.palette.critical);
    }
    out.lost
}

/// Ferme la saisie et rend le focus (les raccourcis redeviennent actifs).
fn close_editor(ui: &egui::Ui, edit: &mut TableEdit) {
    edit.editing = None;
    edit.focus_pending = false;
    grid::surrender_editor(ui);
}

/// Déplacement après une saisie validée par Entrée, Tab et leurs Maj :
/// dans un bloc de plus d'une cellule, la cellule suivante du bloc
/// (H-UI-93, `NavMove::Within`) ; ailleurs, comme avant (H-UI-13, H-UI-14).
const fn within_block(motion: Move) -> Option<NavMove> {
    match motion {
        Move::Down => Some(NavMove::Within(InBlockMove::Down)),
        Move::Up => Some(NavMove::Within(InBlockMove::Up)),
        Move::Next => Some(NavMove::Within(InBlockMove::Right)),
        Move::Previous => Some(NavMove::Within(InBlockMove::Left)),
        Move::Stay => None,
    }
}

/// Valide la saisie : refus → message sous la cellule, saisie gardée (la
/// sélection ne bouge pas, H-UI-86) ; accord → lot poussé (s'il n'est pas
/// vide), saisie fermée, curseur posé sur la cellule éditée puis déplacé
/// (`Action::Nav`, après le lot : une tâche créée par la ligne vide la
/// précède, la ligne vide reste la ligne courante pour saisir des tâches à
/// la suite), ou sur la cellule cliquée (`click`).
fn commit(
    ui: &egui::Ui,
    fc: &mut FrameCtx<'_>,
    edit: &mut TableEdit,
    cursor: &mut TableCursor,
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
    let result = commit_edit(state, &doc.session, text_language(fc.texts.language()));
    let batch = match result {
        Ok(batch) => batch,
        Err(error) => {
            state.error = Some(error);
            edit.focus_pending = true;
            return;
        }
    };
    let column = state.column;
    close_editor(ui, edit);
    if !batch.ops.is_empty() {
        fc.actions.push(Action::ExecOn {
            opening: doc.opening,
            batch,
        });
    }
    cursor.column = column;
    // La ligne éditée est la ligne courante (sélection, ou ligne vide) :
    // le curseur en part.
    if let Some(mv) = within_block(motion) {
        fc.actions.push(Action::Nav { mv, extend: false });
        fc.actions.push(Action::FocusPane(Pane::Table));
    }
    if let Some(click) = click {
        fc.actions.push(Action::Select(click.uid));
        fc.actions.push(Action::FocusPane(Pane::Table));
        if let Some(column) = click.column {
            cursor.column = column;
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::float_cmp,
    reason = "tests : un échec doit s'arrêter net et lisiblement ; valeurs exactes"
)]
mod tests {
    use plannercraft_core::TaskUid;

    use super::{NAME_MIN_TEXT_ROOM, cell_text_color, name_indent, scroll_to_column, width_from_drag};
    use crate::column::Column;
    use crate::rows::{Row, RowKind};
    use crate::theme::Palette;

    fn row(kind: RowKind, critical: bool) -> Row {
        Row {
            uid: TaskUid::new(1),
            id: 1,
            level: 1,
            kind,
            critical,
            bar: None,
            percent: plannercraft_core::Percent::ZERO,
            baseline: None,
            finish_variance: None,
            stop: None,
            manual: false,
            active: true,
        }
    }

    /// D-fix-7 : le texte d'une tâche critique n'est rouge que dans la
    /// colonne « Critique ».
    #[test]
    fn table_critical_red_only_in_critical_column() {
        let p = Palette::LIGHT;
        let leaf = row(RowKind::Leaf, true);
        for column in Column::ALL {
            let expected = if column == Column::Critical {
                p.critical
            } else {
                p.text
            };
            assert_eq!(cell_text_color(&leaf, column, &p), expected, "{column:?}");
        }
        let summary = row(RowKind::Summary, true);
        assert_eq!(cell_text_color(&summary, Column::Name, &p), p.text_strong);
        assert_eq!(cell_text_color(&summary, Column::Critical, &p), p.critical);
        assert_eq!(
            cell_text_color(&row(RowKind::Leaf, false), Column::Critical, &p),
            p.text
        );
        assert_eq!(
            cell_text_color(&row(RowKind::Excluded, true), Column::Critical, &p),
            p.text_weak
        );
    }

    /// H-UI-17 horizontal (J7, lot L6) : une colonne de marge quand elle
    /// tient, alignement à gauche d'une colonne plus large que la vue,
    /// bornes et valeurs non finies.
    #[test]
    fn table_scroll_to_column_margin() {
        let widths = [40, 220, 80, 96, 96, 110];
        // Vue de 300 : colonnes 0-1 visibles (0..260).
        assert_eq!(scroll_to_column(&widths, 1, 0.0, 300.0), 0.0);
        // Colonne 2 (260..340) et sa voisine 3 (..436) : bord droit à 436.
        assert_eq!(scroll_to_column(&widths, 2, 0.0, 300.0), 136.0);
        // Retour à la colonne 1 (40..260) avec la colonne 0 en marge.
        assert_eq!(scroll_to_column(&widths, 1, 136.0, 300.0), 0.0);
        // Dernière colonne : maximum (642 − 300).
        assert_eq!(scroll_to_column(&widths, 5, 0.0, 300.0), 342.0);
        // Marge impossible (vue de 120) : la colonne seule.
        assert_eq!(scroll_to_column(&widths, 2, 0.0, 120.0), 220.0);
        // Colonne plus large que la vue : bord gauche.
        assert_eq!(scroll_to_column(&widths, 1, 0.0, 100.0), 40.0);
        // Colonne absente, valeurs non finies : bornées.
        assert_eq!(scroll_to_column(&widths, 9, 1e9, 300.0), 342.0);
        assert_eq!(scroll_to_column(&widths, 0, f32::NAN, f32::INFINITY), 0.0);
        assert_eq!(scroll_to_column(&[], 0, 5.0, 300.0), 0.0);
    }

    #[test]
    fn table_width_from_drag_bounded() {
        assert_eq!(width_from_drag(120.4), Some(120));
        assert_eq!(width_from_drag(-50.0), Some(Column::MIN_WIDTH));
        assert_eq!(width_from_drag(1e9), Some(Column::MAX_WIDTH));
        assert_eq!(width_from_drag(f32::NAN), None);
        assert_eq!(width_from_drag(f32::INFINITY), None);
    }

    /// Mineur A/M4 : l'indentation est plafonnée, le nom reste visible au
    /// niveau 200 ou 65535.
    #[test]
    fn table_name_indent_is_capped() {
        assert_eq!(name_indent(14.0, 1, 220.0), 0.0);
        assert_eq!(name_indent(14.0, 3, 220.0), 28.0);
        for level in [46, 200, u16::MAX] {
            assert_eq!(name_indent(14.0, level, 220.0), 220.0 - NAME_MIN_TEXT_ROOM);
        }
        // Colonne plus étroite que la réserve : aucun retrait négatif.
        assert_eq!(name_indent(14.0, 10, 24.0), 0.0);
    }
}
