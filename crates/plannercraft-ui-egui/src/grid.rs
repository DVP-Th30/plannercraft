//! Grille commune de la table des tâches et de la feuille des ressources
//! (J7, ADR-0015 D1 ; extraite au lot L10) : peinture des cellules et des
//! titres, cadre du curseur, éditeur de cellule et navigation de
//! [`crate::nav`].
//!
//! Une grille est décrite par le trait [`Sheet`] (lignes, colonnes visibles
//! et leurs largeurs, textes pré-formatés, saisie) ; les fonctions de ce
//! module ne connaissent ni les tâches ni les ressources. Chaque vue garde
//! sa boucle de lignes (sélection, libellés d'accessibilité, clics propres)
//! et appelle ces briques : la table des tâches dessine ainsi exactement ce
//! qu'elle dessinait avant l'extraction (mêmes formes, mêmes galleys).
//!
//! Aucune date ni aucun calcul ici (règle 3) : les textes viennent tout
//! formatés du modèle de la vue, la saisie est lue par `plannercraft-automation`
//! dans [`Sheet::commit`]. Indices bornés par [`nav::step`], flottants non
//! finis écartés (règle 10).

use std::sync::Arc;

use egui::text::{CCursor, CCursorRange, LayoutJob, TextFormat, TextWrapping};
use egui::{
    Color32, FontId, Galley, Id, Key, Modifiers, Order, Painter, Rect, Stroke, StrokeKind, UiBuilder, pos2,
};
use plannercraft_automation::Batch;
use plannercraft_automation::Session;
use plannercraft_automation::text::Language;

use crate::column::CellAlign;
use crate::i18n::TextId;
use crate::nav::{self, CellCursor, GridPos, GridShape, NavMove, RowRef};
use crate::theme::Palette;

/// Données d'une grille : lignes, colonnes, textes, saisie.
pub trait Sheet {
    /// Clé d'une ligne (UID).
    type Key: Copy + Ord;
    /// Colonne.
    type Col: Copy + Eq;

    /// Nombre de lignes, ligne vide de fin non comprise.
    fn len(&self) -> usize;

    /// Vrai sans ligne (hors ligne vide).
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clé de la ligne `row`, `None` hors grille.
    fn key(&self, row: usize) -> Option<Self::Key>;

    /// Indice de la ligne de clé `key` (ajout du lot L10 au contrat L0 :
    /// le curseur désigne une ligne par sa clé).
    fn index_of(&self, key: Self::Key) -> Option<usize>;

    /// Colonnes visibles et leur largeur (points logiques).
    fn columns(&self) -> &[(Self::Col, u16)];

    /// Titre de la colonne (ajout du lot L10 : en-tête, libellés
    /// d'accessibilité).
    fn title(&self, col: Self::Col) -> TextId;

    /// Alignement du texte de la colonne (défaut : à gauche).
    fn align(&self, col: Self::Col) -> CellAlign {
        let _ = col;
        CellAlign::Left
    }

    /// Couleur, gras simulé et niveau de retrait de la cellule (défaut :
    /// texte normal, sans retrait).
    fn style(&self, row: usize, col: Self::Col, palette: &Palette) -> CellStyle {
        let _ = (row, col);
        CellStyle::plain(palette.text)
    }

    /// Texte de la cellule (vide hors grille).
    fn text(&self, row: usize, col: Self::Col) -> &str;

    /// La colonne se saisit-elle ?
    fn editable(&self, col: Self::Col) -> bool;

    /// Lot de la saisie `text` dans la cellule, lu par `plannercraft-automation`.
    ///
    /// # Errors
    ///
    /// Message à afficher sous la cellule.
    fn commit(
        &self,
        row: RowRef<Self::Key>,
        col: Self::Col,
        text: &str,
        s: &Session,
        lang: Language,
    ) -> Result<Batch, String>;
}

/// Apparence du texte d'une cellule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellStyle {
    /// Couleur du texte.
    pub color: Color32,
    /// Gras simulé (second tracé décalé ; la police par défaut n'a pas de
    /// graisse grasse).
    pub faux_bold: bool,
    /// Niveau hiérarchique (retrait au-delà du premier niveau, plafonné par
    /// [`name_indent`]) ; 0 ou 1 : aucun retrait.
    pub level: u16,
    /// Texte barré (tâche inactive, H-UI-37, J8 lot L11).
    pub strike: bool,
    /// Italique (oblique d'egui ; texte des tâches, finitions P2,
    /// H-UI-126).
    pub italic: bool,
}

impl CellStyle {
    /// Texte de couleur `color`, sans gras ni retrait.
    #[must_use]
    pub const fn plain(color: Color32) -> Self {
        Self {
            color,
            faux_bold: false,
            level: 0,
            strike: false,
            italic: false,
        }
    }
}

/// Largeur en dessous de laquelle une cellule n'affiche pas de texte.
pub const MIN_TEXT_WIDTH: f32 = 4.0;

/// Décalage du second tracé qui simule le gras.
pub const FAUX_BOLD_DX: f32 = 0.6;

/// Largeur d'une colonne toujours laissée au texte, quel que soit le niveau
/// hiérarchique : l'indentation est plafonnée à `largeur − 40` (un nom au
/// niveau 200 reste lisible).
pub const NAME_MIN_TEXT_ROOM: f32 = 40.0;

/// Épaisseur du cadre de la cellule courante, en points.
pub const CURSOR_STROKE: f32 = 2.0;

/// Police des cellules et des titres.
#[must_use]
pub fn cell_font(ui: &egui::Ui) -> FontId {
    egui::TextStyle::Body.resolve(ui.style())
}

/// Galley d'un texte tronqué à `max_w` (« … »), couleur fixée au dessin.
#[must_use]
pub fn truncated_galley(painter: &Painter, text: &str, font: FontId, max_w: f32) -> Arc<Galley> {
    truncated_galley_styled(painter, text, font, max_w, false)
}

/// [`truncated_galley`], en italique si `italic` (finitions P2, H-UI-126).
#[must_use]
pub fn truncated_galley_styled(
    painter: &Painter,
    text: &str,
    font: FontId,
    max_w: f32,
    italic: bool,
) -> Arc<Galley> {
    let mut format = TextFormat::simple(font, Color32::PLACEHOLDER);
    format.italics = italic;
    let mut job = LayoutJob::single_section(text.to_owned(), format);
    job.wrap = TextWrapping::truncate_at_width(max_w);
    painter.layout_job(job)
}

/// Abscisse d'un texte de largeur `text_w` dans la cellule `[left, left + w)`.
#[must_use]
pub fn aligned_x(align: CellAlign, left: f32, w: f32, pad: f32, text_w: f32) -> f32 {
    match align {
        CellAlign::Left => left + pad,
        CellAlign::Right => (left + w - pad - text_w).max(left + pad),
        CellAlign::Center => (left + (w - text_w) * 0.5).max(left + pad),
    }
}

/// Retrait au niveau `level`, plafonné pour laisser [`NAME_MIN_TEXT_ROOM`]
/// au texte dans une colonne de largeur `w`.
#[must_use]
pub fn name_indent(indent_per_level: f32, level: u16, w: f32) -> f32 {
    let wanted = indent_per_level * f32::from(level.saturating_sub(1));
    let cap = (w - NAME_MIN_TEXT_ROOM).max(0.0);
    if wanted.is_finite() { wanted.min(cap) } else { 0.0 }
}

/// Colonne sous l'abscisse écran `x`, la grille commençant à `left`.
#[must_use]
pub fn column_at<C: Copy>(columns: &[(C, u16)], left: f32, x: f32) -> Option<C> {
    let mut l = left;
    for &(column, width) in columns {
        let w = f32::from(width);
        if x >= l && x < l + w {
            return Some(column);
        }
        l += w;
    }
    None
}

/// Bord gauche et largeur de la colonne `column`.
#[must_use]
pub fn column_span<C: Copy + Eq>(columns: &[(C, u16)], left: f32, column: C) -> Option<(f32, f32)> {
    let mut l = left;
    for &(c, width) in columns {
        let w = f32::from(width);
        if c == column {
            return Some((l, w));
        }
        l += w;
    }
    None
}

/// Indice de la ligne désignée (ligne vide : `sheet.len()`) ; `None` si
/// sa clé n'est plus dans la grille.
pub fn row_index<S: Sheet>(sheet: &S, row: RowRef<S::Key>) -> Option<usize> {
    match row {
        RowRef::Item(key) => sheet.index_of(key),
        RowRef::Blank => Some(sheet.len()),
    }
}

/// Ligne d'indice `index` (au-delà des lignes : la ligne vide).
pub fn row_ref<S: Sheet>(sheet: &S, index: usize) -> RowRef<S::Key> {
    RowRef::from_selection(sheet.key(index))
}

/// Déplacement du curseur dans la grille (H-UI-13 à H-UI-15) : [`nav::step`]
/// sur (ligne, colonne visible), ligne vide comprise ; une ligne disparue
/// repart de la première, une colonne masquée de la première visible. Rend
/// la cellule d'arrivée, ancre effacée (pas d'extension de sélection).
pub fn nav_target<S: Sheet>(
    sheet: &S,
    cursor: &CellCursor<S::Key, S::Col>,
    mv: NavMove,
    page: usize,
) -> CellCursor<S::Key, S::Col> {
    let columns = sheet.columns();
    let row = row_index(sheet, cursor.row).unwrap_or(0);
    let col = columns
        .iter()
        .position(|(column, _)| *column == cursor.column)
        .unwrap_or(0);
    let shape = GridShape {
        rows: sheet.len().saturating_add(1),
        cols: columns.len(),
        page,
    };
    let target = nav::step(GridPos { row, col }, mv, shape);
    let column = columns
        .get(target.col)
        .map_or(cursor.column, |(column, _)| *column);
    CellCursor {
        row: row_ref(sheet, target.row),
        column,
        anchor: None,
        anchor_column: None,
    }
}

/// Mesures d'une ligne à dessiner.
#[derive(Clone, Copy, Debug)]
pub struct RowPlace {
    /// Rectangle écran de la ligne (largeur de la découpe).
    pub rect: Rect,
    /// Abscisse écran du bord gauche de la première colonne (défilement
    /// horizontal déduit).
    pub cols_left: f32,
    /// Marge horizontale du texte.
    pub pad: f32,
    /// Retrait par niveau hiérarchique.
    pub indent: f32,
}

/// Textes des cellules visibles de la ligne `i` (retrait, troncature au
/// bord visible, alignement de la colonne, gras simulé) ; rend le nombre de
/// formes ajoutées. Les colonnes entièrement hors de la découpe du
/// `painter` ne sont pas dessinées.
pub fn paint_cells<S: Sheet>(
    painter: &Painter,
    sheet: &S,
    palette: &Palette,
    i: usize,
    at: RowPlace,
    font: &FontId,
) -> usize {
    let pad = at.pad;
    let clip_left = painter.clip_rect().left();
    let right = painter.clip_rect().right();
    let mut shapes = 0_usize;
    let mut x = at.cols_left;
    for &(column, width) in sheet.columns() {
        if x >= right {
            break;
        }
        let w = f32::from(width);
        let cell_left = x;
        x += w;
        if x <= clip_left {
            // Colonne entièrement à gauche de la vue (défilement horizontal).
            continue;
        }
        let text = sheet.text(i, column);
        if text.is_empty() {
            continue;
        }
        let style = sheet.style(i, column, palette);
        let indent = name_indent(at.indent, style.level, w);
        // Troncature au bord visible de la grille si la colonne le dépasse.
        let visible_w = w.min(right - cell_left);
        let max_w = visible_w - 2.0 * pad - indent;
        if !max_w.is_finite() || max_w < MIN_TEXT_WIDTH {
            continue;
        }
        let galley = truncated_galley_styled(painter, text, font.clone(), max_w, style.italic);
        let size = galley.size();
        let text_x = aligned_x(sheet.align(column), cell_left + indent, w - indent, pad, size.x);
        let text_y = at.rect.top() + (at.rect.height() - size.y) * 0.5;
        if style.faux_bold {
            painter.galley(
                pos2(text_x + FAUX_BOLD_DX, text_y),
                Arc::clone(&galley),
                style.color,
            );
            shapes = shapes.saturating_add(1);
        }
        if style.strike {
            // Trait au milieu de la hauteur d'x, sur la largeur du texte.
            let y = (text_y + size.y * 0.55).round() + 0.5;
            painter.hline(text_x..=text_x + size.x, y, Stroke::new(1.0, style.color));
            shapes = shapes.saturating_add(1);
        }
        painter.galley(pos2(text_x, text_y), galley, style.color);
        shapes = shapes.saturating_add(1);
    }
    shapes
}

/// Place d'un en-tête de grille.
#[derive(Clone, Copy, Debug)]
pub struct HeaderPlace {
    /// Rectangle de l'en-tête.
    pub rect: Rect,
    /// Bord gauche écran de la première colonne (défilement déduit).
    pub left: f32,
    /// Marge horizontale du texte.
    pub pad: f32,
}

/// Titres d'un en-tête de grille (centrés, tronqués) et séparateurs
/// verticaux, à partir de `at.left` ; s'arrête au bord droit de `at.rect`.
/// Rend les bords `(colonne, gauche, droite)` des colonnes au moins en
/// partie visibles et le nombre de formes ajoutées. Le fond et le trait bas
/// restent à la vue.
pub fn paint_titles<C: Copy>(
    painter: &Painter,
    columns: &[(C, u16)],
    title: impl Fn(C) -> String,
    palette: &Palette,
    at: HeaderPlace,
    font: &FontId,
) -> (Vec<(C, f32, f32)>, usize) {
    paint_titles_aligned(painter, columns, title, |_| CellAlign::Center, palette, at, font)
}

/// Comme [`paint_titles`], chaque titre aligné selon `align` (« Paramètres
/// de champ », H-UI-115, seconde passe UX lot L6).
pub fn paint_titles_aligned<C: Copy>(
    painter: &Painter,
    columns: &[(C, u16)],
    title: impl Fn(C) -> String,
    align: impl Fn(C) -> CellAlign,
    palette: &Palette,
    at: HeaderPlace,
    font: &FontId,
) -> (Vec<(C, f32, f32)>, usize) {
    let HeaderPlace { rect, left, pad } = at;
    let mut shapes = 0_usize;
    let mut edges = Vec::new();
    let mut left = left;
    for &(column, width) in columns {
        if left >= rect.right() {
            break;
        }
        let w = f32::from(width);
        let right = left + w;
        if right <= rect.left() {
            left = right;
            continue;
        }
        let max_w = w - 2.0 * pad;
        if max_w >= MIN_TEXT_WIDTH {
            let galley = truncated_galley(painter, &title(column), font.clone(), max_w);
            let size = galley.size();
            let x = aligned_x(align(column), left, w, pad, size.x);
            let y = rect.center().y - size.y * 0.5;
            painter.galley(pos2(x, y), galley, palette.text_strong);
            shapes = shapes.saturating_add(1);
        }
        painter.vline(right - 0.5, rect.y_range(), Stroke::new(1.0, palette.grid_major));
        shapes = shapes.saturating_add(1);
        edges.push((column, left, right));
        left = right;
    }
    (edges, shapes)
}

/// Séparateurs verticaux des colonnes sur toute la hauteur de `clip`, à
/// partir de `cols_left` ; rend le nombre de formes ajoutées.
pub fn paint_column_lines<C>(
    painter: &Painter,
    columns: &[(C, u16)],
    cols_left: f32,
    clip: Rect,
    color: Color32,
) -> usize {
    let mut shapes = 0_usize;
    let mut x = cols_left;
    for (_, width) in columns {
        x += f32::from(*width);
        if x > clip.right() {
            break;
        }
        if x <= clip.left() {
            continue;
        }
        painter.vline(x - 0.5, clip.y_range(), Stroke::new(1.0, color));
        shapes = shapes.saturating_add(1);
    }
    shapes
}

/// Cadre de 2 px (`Palette::cursor`) de la cellule `rect` ; rend vrai s'il
/// a été dessiné (rectangle fini, non vide, dans la découpe).
#[must_use]
pub fn paint_cursor_frame(painter: &Painter, rect: Rect, palette: &Palette) -> bool {
    if !rect.is_finite() || !rect.is_positive() || !rect.intersects(painter.clip_rect()) {
        return false;
    }
    painter.rect_stroke(
        rect.shrink(CURSOR_STROKE * 0.5),
        0.0,
        Stroke::new(CURSOR_STROKE, palette.cursor),
        StrokeKind::Inside,
    );
    true
}

/// Fin d'une saisie demandée par l'utilisateur.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finish {
    /// Échap : saisie abandonnée.
    Cancel,
    /// Validation, puis déplacement de la cellule courante.
    Commit(Move),
    /// Ctrl/Cmd+Entrée : le texte saisi est écrit dans tout le bloc
    /// (H-UI-98, ADR-0018 D4) ; la table le traduit en
    /// `Action::Cells(CellOp::FillTyped)`, la saisie reste ouverte tant que
    /// le lot n'est pas accepté. Sans bloc (feuille des ressources) : rien.
    FillTyped,
}

/// Déplacement après validation (H-UI-14 : Tab passe à la colonne
/// **visible** voisine, colonnes calculées comprises).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    /// Entrée, flèche bas : ligne suivante.
    Down,
    /// Flèche haut, Maj+Entrée : ligne précédente.
    Up,
    /// Tab : colonne visible suivante.
    Next,
    /// Maj+Tab : colonne visible précédente.
    Previous,
    /// Défilement, perte du focus : la cellule reste.
    Stay,
}

impl Move {
    /// Déplacement du curseur correspondant (`None` : la cellule reste).
    #[must_use]
    pub const fn nav(self) -> Option<NavMove> {
        match self {
            Self::Down => Some(NavMove::Down),
            Self::Up => Some(NavMove::Up),
            Self::Next => Some(NavMove::Right),
            Self::Previous => Some(NavMove::Left),
            Self::Stay => None,
        }
    }
}

/// Touches de fin de saisie, consommées avant la zone de saisie (qui ne
/// les voit donc pas). Les plus précises d'abord (Maj+Tab avant Tab).
#[must_use]
pub fn finish_keys(ui: &egui::Ui) -> Option<Finish> {
    ui.input_mut(|input| {
        if input.consume_key(Modifiers::NONE, Key::Escape) {
            Some(Finish::Cancel)
        } else if input.consume_key(Modifiers::COMMAND, Key::Enter) {
            // Ctrl/Cmd+Entrée (H-UI-98), avant Maj+Entrée et Entrée.
            Some(Finish::FillTyped)
        } else if input.consume_key(Modifiers::SHIFT, Key::Enter) {
            // Maj+Entrée valide et monte (H-UI-13) ; avant Entrée, qu'egui
            // reconnaîtrait aussi avec Maj.
            Some(Finish::Commit(Move::Up))
        } else if input.consume_key(Modifiers::SHIFT, Key::Tab) {
            Some(Finish::Commit(Move::Previous))
        } else if input.consume_key(Modifiers::NONE, Key::Tab) {
            Some(Finish::Commit(Move::Next))
        } else if input.consume_key(Modifiers::NONE, Key::Enter)
            || input.consume_key(Modifiers::NONE, Key::ArrowDown)
        {
            Some(Finish::Commit(Move::Down))
        } else if input.consume_key(Modifiers::NONE, Key::ArrowUp) {
            Some(Finish::Commit(Move::Up))
        } else {
            None
        }
    })
}

/// Un défilement (molette, défilement doux) a eu lieu dans cette image : une
/// saisie ouverte est validée sur place.
#[must_use]
pub fn scrolled(ui: &egui::Ui) -> bool {
    ui.input(|i| {
        i.smooth_scroll_delta.y.abs() > 0.0
            || i.events
                .iter()
                .any(|e| matches!(e, egui::Event::MouseWheel { delta, .. } if delta.y.abs() > 0.0))
    })
}

/// Identifiant de la zone de saisie (une seule saisie à la fois, toutes
/// grilles confondues).
#[must_use]
pub fn editor_id() -> Id {
    Id::unique("plannercraft-cell-editor")
}

/// Apparence de la zone de saisie : celle du texte de la cellule (police,
/// taille, couleur, gras simulé, alignement, retrait), sur un fond uni de
/// cellule encadré comme le curseur (H-UI-85).
#[derive(Clone, Debug, PartialEq)]
pub struct EditorLook {
    /// Police des cellules de la grille (Inter si le style la demande).
    pub font: FontId,
    /// Alignement de la colonne.
    pub align: CellAlign,
    /// Marge horizontale du texte (celle de [`paint_cells`]).
    pub pad: f32,
    /// Retrait de niveau, déjà plafonné par [`name_indent`].
    pub indent: f32,
    /// Couleur, gras simulé et texte barré de la cellule.
    pub style: CellStyle,
    /// Fond de la cellule en saisie.
    pub fill: Color32,
    /// Cadre de la cellule (couleur du curseur).
    pub frame: Color32,
}

impl EditorLook {
    /// Apparence de la cellule (ligne `row`, colonne `col`) d'une grille :
    /// celle que lui donne [`paint_cells`] avec `font`, `pad` et le retrait
    /// par niveau `indent`, dans une colonne de largeur `w`.
    #[must_use]
    pub fn of<S: Sheet>(
        sheet: &S,
        (row, col): (usize, S::Col),
        palette: &Palette,
        font: FontId,
        pad: f32,
        indent: f32,
        w: f32,
    ) -> Self {
        let style = sheet.style(row, col, palette);
        Self {
            font,
            align: sheet.align(col),
            pad,
            indent: name_indent(indent, style.level, w),
            style,
            fill: palette.background,
            frame: palette.cursor,
        }
    }
}

/// Zone de saisie d'une cellule à dessiner.
pub struct EditorSpec {
    /// Rectangle de la cellule.
    pub rect: Rect,
    /// Découpe de la grille.
    pub clip: Rect,
    /// Nom accessible (« Saisie : Durée »).
    pub label: String,
    /// Apparence du texte et de la cellule.
    pub look: EditorLook,
}

/// Résultat d'une image de la zone de saisie.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EditorOut {
    /// Le texte a changé (le refus affiché est alors effacé par l'appelant).
    pub changed: bool,
    /// La zone vient de perdre le focus.
    pub lost: bool,
}

/// Marge en points → marge d'egui (entière, bornée à `[0, 127]`).
fn margin_points(v: f32) -> i8 {
    let v = if v.is_finite() {
        v.round().clamp(0.0, 127.0)
    } else {
        0.0
    };
    i8::try_from(crate::geom::f32_to_index(v)).unwrap_or(0)
}

/// Alignement egui d'une colonne.
const fn egui_align(align: CellAlign) -> egui::Align {
    match align {
        CellAlign::Left => egui::Align::LEFT,
        CellAlign::Right => egui::Align::RIGHT,
        CellAlign::Center => egui::Align::Center,
    }
}

/// Dessine la zone de saisie **dans** la cellule (H-UI-85) : fond uni et
/// cadre du curseur, texte dans la police, la couleur, l'alignement et le
/// retrait de la cellule, sans cadre arrondi d'egui ; curseur de texte et
/// sélection d'egui. `focus_pending` : la zone prend le focus, état de
/// saisie remis à neuf (ni défilement ni historique d'annulation d'une
/// saisie précédente : l'identifiant est partagé), curseur en fin de texte
/// (F2, double-clic comme la frappe d'un caractère).
pub fn show_editor(
    ui: &mut egui::Ui,
    buffer: &mut String,
    focus_pending: &mut bool,
    spec: &EditorSpec,
) -> EditorOut {
    let rect = spec.rect;
    if !rect.is_finite() || !rect.is_positive() {
        return EditorOut::default();
    }
    let look = &spec.look;
    let clip = spec.clip.intersect(ui.clip_rect());
    let painter = ui.painter().with_clip_rect(clip);
    painter.rect_filled(rect, 0.0, look.fill);
    painter.rect_stroke(
        rect.shrink(CURSOR_STROKE * 0.5),
        0.0,
        Stroke::new(CURSOR_STROKE, look.frame),
        StrokeKind::Inside,
    );
    if *focus_pending {
        let mut state = egui::text_edit::TextEditState::default();
        let end = CCursor::new(buffer.chars().count());
        state.cursor.set_char_range(Some(CCursorRange::one(end)));
        state.store(ui.ctx(), editor_id());
    }
    // Zone du texte : la cellule moins le retrait de niveau ; la marge
    // horizontale est celle de `paint_cells`.
    let indent = if look.indent.is_finite() {
        look.indent
            .clamp(0.0, (rect.width() - 2.0 * look.pad - MIN_TEXT_WIDTH).max(0.0))
    } else {
        0.0
    };
    let text_rect = Rect::from_min_max(pos2(rect.left() + indent, rect.top()), rect.max);
    let mut child = ui.new_child(UiBuilder::new().max_rect(text_rect));
    child.set_clip_rect(clip);
    let pad = margin_points(look.pad);
    let output = egui::TextEdit::singleline(buffer)
        .id(editor_id())
        .lock_focus(true)
        .frame(egui::Frame::NONE.inner_margin(egui::Margin {
            left: pad,
            right: pad,
            top: 0,
            bottom: 0,
        }))
        .font(look.font.clone())
        .text_color(look.style.color)
        .align(egui::Align2([egui_align(look.align), egui::Align::Center]))
        .desired_width(text_rect.width())
        .min_size(text_rect.size())
        .show(&mut child);
    // Gras simulé et texte barré de la cellule, repeints sur le texte
    // saisi : `galley_pos` est le bord gauche visible du texte (défilement
    // horizontal et alignement compris). Le gras simulé est une galley
    // neuve du même texte, sans le surlignage de la sélection que porte
    // celle de la zone.
    let text_painter = painter.with_clip_rect(output.text_clip_rect.expand(1.0).intersect(clip));
    let size = output.galley.size();
    let (x, y) = (output.galley_pos.x, output.galley_pos.y);
    if look.style.faux_bold && size.x > 0.0 {
        let bold = text_painter.layout_no_wrap(buffer.clone(), look.font.clone(), look.style.color);
        text_painter.galley(pos2(x + FAUX_BOLD_DX, y), bold, look.style.color);
    }
    if look.style.strike && size.x > 0.0 {
        let y = (y + size.y * 0.55).round() + 0.5;
        text_painter.hline(x..=x + size.x, y, Stroke::new(1.0, look.style.color));
    }
    let response = output.response.response;
    let label = spec.label.clone();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, label.clone()));
    if *focus_pending {
        response.request_focus();
        *focus_pending = false;
    }
    EditorOut {
        changed: response.changed(),
        lost: response.lost_focus(),
    }
}

/// Refus d'une saisie, affiché sous la cellule `rect` (couche au premier
/// plan, non interactive).
pub fn show_cell_error(ctx: &egui::Context, rect: Rect, message: String, color: Color32) {
    egui::Area::new(Id::unique("plannercraft-cell-error"))
        .order(Order::Foreground)
        .fixed_pos(pos2(rect.left(), rect.bottom()))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.label(egui::RichText::new(message).color(color));
            });
        });
}

/// Rend le focus de la zone de saisie (les raccourcis redeviennent actifs).
pub fn surrender_editor(ui: &egui::Ui) {
    ui.memory_mut(|memory| memory.surrender_focus(editor_id()));
}

/// Saisie en cours dans une cellule d'une grille générique (feuille des
/// ressources, J7 lot L10 ; la table des tâches garde `edit::EditState`,
/// dont le refus est typé).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellEdit<K, C> {
    /// Ligne éditée (ligne vide : création).
    pub row: RowRef<K>,
    /// Colonne éditée.
    pub column: C,
    /// Texte saisi.
    pub buffer: String,
    /// Texte à l'ouverture par F2 ou double-clic : valider sans rien changer
    /// n'envoie aucun lot. `None` (frappe d'un caractère) : toujours envoyé.
    pub original: Option<String>,
    /// Refus de la dernière validation, affiché sous la cellule.
    pub error: Option<String>,
}

impl<K, C> CellEdit<K, C> {
    /// Saisie ouverte avec le texte de la cellule (F2, double-clic).
    #[must_use]
    pub fn open(row: RowRef<K>, column: C, text: String) -> Self {
        Self {
            row,
            column,
            buffer: text.clone(),
            original: Some(text),
            error: None,
        }
    }

    /// Saisie ouverte par la frappe : `typed` remplace la valeur.
    #[must_use]
    pub fn typed(row: RowRef<K>, column: C, typed: &str) -> Self {
        Self {
            row,
            column,
            buffer: typed.to_owned(),
            original: None,
            error: None,
        }
    }
}

/// Saisie d'une grille générique : état transitoire de la vue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridEdit<K, C> {
    /// Saisie ouverte, s'il y en a une.
    pub editing: Option<CellEdit<K, C>>,
    /// Donner le focus à la zone de saisie à la prochaine image.
    pub focus_pending: bool,
}

impl<K, C> Default for GridEdit<K, C> {
    fn default() -> Self {
        Self {
            editing: None,
            focus_pending: false,
        }
    }
}

impl<K, C> GridEdit<K, C> {
    /// Ouvre une saisie et demande le focus.
    pub fn start(&mut self, edit: CellEdit<K, C>) {
        self.editing = Some(edit);
        self.focus_pending = true;
    }
}

/// Lot d'une saisie validée : `None` si le texte n'a pas changé depuis
/// l'ouverture (F2 puis Entrée), sinon [`Sheet::commit`] sur une colonne
/// éditable.
///
/// # Errors
///
/// Le refus de [`Sheet::commit`] ; colonne non éditable : `not_editable`.
pub fn commit_cell<S: Sheet>(
    sheet: &S,
    edit: &CellEdit<S::Key, S::Col>,
    session: &Session,
    lang: Language,
    not_editable: &str,
) -> Result<Option<Batch>, String> {
    if !sheet.editable(edit.column) {
        return Err(not_editable.to_owned());
    }
    if edit.original.as_deref() == Some(edit.buffer.as_str()) {
        return Ok(None);
    }
    sheet
        .commit(edit.row, edit.column, &edit.buffer, session, lang)
        .map(|batch| (!batch.ops.is_empty()).then_some(batch))
}

/// Défilement horizontal qui montre la colonne visible `col` (largeurs
/// `widths` dans l'ordre d'affichage), avec la colonne voisine en marge du
/// côté du déplacement quand elle tient (H-UI-17 horizontal, J7 lot L6) ;
/// une colonne plus large que la vue s'aligne à gauche. Rend `scroll` borné
/// à `[0, Σ largeurs − view_w]` si la colonne est déjà visible ou absente ;
/// valeurs non finies : 0.
#[must_use]
pub fn scroll_to_column(widths: &[u16], col: usize, scroll: f32, view_w: f32) -> f32 {
    let scroll = if scroll.is_finite() { scroll.max(0.0) } else { 0.0 };
    let view_w = if view_w.is_finite() { view_w.max(0.0) } else { 0.0 };
    let total: f32 = widths.iter().map(|&w| f32::from(w)).sum();
    let max = (total - view_w).max(0.0);
    let Some(&w) = widths.get(col) else {
        return scroll.min(max);
    };
    let left: f32 = widths.iter().take(col).map(|&w| f32::from(w)).sum();
    let right = left + f32::from(w);
    if right - left >= view_w {
        return left.min(max);
    }
    let prev = col
        .checked_sub(1)
        .and_then(|i| widths.get(i))
        .map_or(0.0, |&w| f32::from(w));
    let next = col
        .checked_add(1)
        .and_then(|i| widths.get(i))
        .map_or(0.0, |&w| f32::from(w));
    // Marge du côté où l'on va, si la colonne et sa voisine tiennent.
    let target = if left < scroll {
        if right - (left - prev) <= view_w {
            left - prev
        } else {
            left
        }
    } else if right > scroll + view_w {
        if right + next - left <= view_w {
            right + next - view_w
        } else {
            right - view_w
        }
    } else {
        scroll
    };
    target.clamp(0.0, max)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::float_cmp,
    reason = "tests : un échec doit s'arrêter net et lisiblement ; valeurs exactes"
)]
mod tests {
    use std::collections::BTreeMap;

    use plannercraft_automation::text::Language;
    use plannercraft_automation::{Batch, Session};

    use super::{CellEdit, Sheet, column_at, column_span, commit_cell, nav_target};
    use crate::i18n::TextId;
    use crate::nav::{CellCursor, NavMove, RowRef};

    /// Grille de test : trois lignes de clés 10, 20, 30, deux colonnes.
    struct Grid {
        keys: Vec<u32>,
        index: BTreeMap<u32, usize>,
        columns: Vec<(u8, u16)>,
    }

    impl Grid {
        fn new() -> Self {
            let keys = vec![10, 20, 30];
            let index = keys.iter().enumerate().map(|(i, k)| (*k, i)).collect();
            Self {
                keys,
                index,
                columns: vec![(0, 40), (1, 100)],
            }
        }
    }

    impl Sheet for Grid {
        type Key = u32;
        type Col = u8;
        fn len(&self) -> usize {
            self.keys.len()
        }
        fn key(&self, row: usize) -> Option<u32> {
            self.keys.get(row).copied()
        }
        fn index_of(&self, key: u32) -> Option<usize> {
            self.index.get(&key).copied()
        }
        fn columns(&self) -> &[(u8, u16)] {
            &self.columns
        }
        fn title(&self, _: u8) -> TextId {
            TextId::ColName
        }
        fn text(&self, _: usize, _: u8) -> &'static str {
            "x"
        }
        fn editable(&self, col: u8) -> bool {
            col == 1
        }
        fn commit(
            &self,
            _: RowRef<u32>,
            _: u8,
            text: &str,
            _: &Session,
            lang: Language,
        ) -> Result<Batch, String> {
            if text == "bad" {
                return Err("refus".to_owned());
            }
            Ok(Batch {
                format_version: Batch::FORMAT_VERSION,
                lang: Some(lang),
                ops: Vec::new(),
                label: None,
            })
        }
    }

    fn cursor(row: RowRef<u32>, column: u8) -> CellCursor<u32, u8> {
        CellCursor {
            row,
            column,
            anchor: None,
            anchor_column: None,
        }
    }

    /// La navigation générique suit `nav::step` : ligne vide comprise,
    /// bords sans bouclage, clé disparue ramenée à la première ligne.
    #[test]
    fn grid_nav_target_blank_and_edges() {
        let g = Grid::new();
        let down = nav_target(&g, &cursor(RowRef::Item(30), 0), NavMove::Down, 10);
        assert_eq!(down.row, RowRef::Blank);
        let stay = nav_target(&g, &down, NavMove::Down, 10);
        assert_eq!(stay.row, RowRef::Blank);
        let right = nav_target(&g, &cursor(RowRef::Item(10), 0), NavMove::Right, 10);
        assert_eq!((right.row, right.column), (RowRef::Item(10), 1));
        let edge = nav_target(&g, &right, NavMove::Right, 10);
        assert_eq!(edge.column, 1);
        let gone = nav_target(&g, &cursor(RowRef::Item(99), 7), NavMove::Down, 10);
        assert_eq!((gone.row, gone.column), (RowRef::Item(20), 0));
        let last = nav_target(&g, &cursor(RowRef::Item(10), 0), NavMove::Last, 0);
        assert_eq!((last.row, last.column), (RowRef::Blank, 1));
    }

    #[test]
    fn grid_column_geometry() {
        let columns = [(0_u8, 40_u16), (1, 100)];
        assert_eq!(column_at(&columns, 10.0, 10.0), Some(0));
        assert_eq!(column_at(&columns, 10.0, 49.9), Some(0));
        assert_eq!(column_at(&columns, 10.0, 50.0), Some(1));
        assert_eq!(column_at(&columns, 10.0, 150.0), None);
        assert_eq!(column_at(&columns, 10.0, f32::NAN), None);
        assert_eq!(column_span(&columns, 10.0, 1), Some((50.0, 100.0)));
        assert_eq!(column_span(&columns, 10.0, 9), None);
    }

    /// Texte inchangé depuis F2 : aucun lot ; colonne calculée refusée ;
    /// refus de la grille rendu tel quel.
    #[test]
    fn grid_commit_cell_unchanged_and_refusals() {
        let g = Grid::new();
        let now = "2027-01-15T08:00".parse().unwrap();
        let session = crate::document::load(crate::DEMO_MSPDI, now).unwrap().session;
        let same = CellEdit::open(RowRef::Item(10), 1, "a".to_owned());
        assert_eq!(commit_cell(&g, &same, &session, Language::Fr, "ne"), Ok(None));
        let calc = CellEdit::typed(RowRef::Item(10), 0, "a");
        assert_eq!(
            commit_cell(&g, &calc, &session, Language::Fr, "ne"),
            Err("ne".to_owned())
        );
        let bad = CellEdit::typed(RowRef::Blank, 1, "bad");
        assert_eq!(
            commit_cell(&g, &bad, &session, Language::Fr, "ne"),
            Err("refus".to_owned())
        );
        // Lot vide de la grille : rien à appliquer.
        let empty = CellEdit::typed(RowRef::Blank, 1, "ok");
        assert_eq!(commit_cell(&g, &empty, &session, Language::Fr, "ne"), Ok(None));
    }
}
