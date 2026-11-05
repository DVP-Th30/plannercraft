//! Navigation au clavier dans une grille (J7, ADR-0015 D1 ; H-UI-13 à
//! H-UI-17, P45) : fonctions pures sur des indices, sans egui, partagées
//! par la table des tâches et la feuille des ressources.
//!
//! Le déplacement se calcule sur (indice de ligne, indice de colonne
//! visible) dans une [`GridShape`] ; la vue traduit ensuite l'indice en
//! [`CellCursor`] (UID ou ligne vide). Aucune indexation directe : toute
//! position rendue reste dans la grille (règle 10).
//!
//! Conventions (MS Project, S61 ; `docs/features/ui.md` § 1.11) :
//! - ↑/↓ (Entrée, Maj+Entrée), ←/→ (Tab, Maj+Tab) : une cellule, sans
//!   retour à la ligne ni bouclage ; au bord, rien ne bouge (H-UI-13,
//!   H-UI-14) ;
//! - Origine/Fin : premier / dernier champ de la ligne ; Ctrl+↑/↓ :
//!   première / dernière ligne (colonne gardée) ; Ctrl+Origine/Fin :
//!   premier champ de la première ligne / dernier champ de la dernière
//!   ligne, ligne vide comprise (H-UI-15) ;
//! - Page haut/bas : `page − 1` lignes, au moins une (hypothèse : une page
//!   de 0 ou 1 ligne avance quand même d'une ligne, comme ↑/↓) ;
//! - passe UX (ADR-0018, lot L1) : Entrée, Tab et leurs Maj
//!   ([`NavMove::Within`]) circulent dans un bloc de plus d'une cellule
//!   (H-UI-93), les extensions (Maj) s'étendent aussi en colonnes (H-UI-92),
//!   défilement automatique pendant un glisser ([`autoscroll_speed`],
//!   [`autoscroll_offset`], H-UI-91).

use crate::column::Column;
use crate::geom::{f32_to_index, index_to_f32};
use crate::row_view::{DisplayRow, VisibleRows};
use crate::selection::Selection;

/// Déplacement demandé (H-UI-13 à H-UI-15).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavMove {
    /// Ligne précédente (↑, Maj+Entrée).
    Up,
    /// Ligne suivante (↓, Entrée).
    Down,
    /// Colonne précédente (←, Maj+Tab).
    Left,
    /// Colonne suivante (→, Tab).
    Right,
    /// Page précédente (lignes visibles − 1).
    PageUp,
    /// Page suivante.
    PageDown,
    /// Premier champ de la ligne (Origine, Ctrl+←).
    RowStart,
    /// Dernier champ de la ligne (Fin, Ctrl+→).
    RowEnd,
    /// Première ligne (Ctrl+↑).
    FirstRow,
    /// Dernière ligne (Ctrl+↓).
    LastRow,
    /// Premier champ de la première ligne (Ctrl+Origine).
    First,
    /// Dernier champ de la dernière ligne (Ctrl+Fin).
    Last,
    /// Entrée, Maj+Entrée, Tab, Maj+Tab (et fin de saisie par ces touches) :
    /// dans un bloc de plus d'une cellule, la cellule active circule dans
    /// le bloc sans changer la sélection (H-UI-93, `Block::step`) ;
    /// ailleurs, même déplacement que ↓, ↑, →, ← ([`NavMove::base`]).
    Within(InBlockMove),
}

impl NavMove {
    /// Déplacement dans la grille d'un [`NavMove::Within`] hors d'un bloc
    /// (Entrée ⇒ ↓, Maj+Entrée ⇒ ↑, Tab ⇒ →, Maj+Tab ⇒ ←) ; les autres
    /// déplacements sont rendus tels quels.
    #[must_use]
    pub const fn base(self) -> Self {
        match self {
            Self::Within(InBlockMove::Down) => Self::Down,
            Self::Within(InBlockMove::Up) => Self::Up,
            Self::Within(InBlockMove::Right) => Self::Right,
            Self::Within(InBlockMove::Left) => Self::Left,
            other => other,
        }
    }

    /// Tous les déplacements (tests, propriété P45).
    pub const ALL: [Self; 12] = [
        Self::Up,
        Self::Down,
        Self::Left,
        Self::Right,
        Self::PageUp,
        Self::PageDown,
        Self::RowStart,
        Self::RowEnd,
        Self::FirstRow,
        Self::LastRow,
        Self::First,
        Self::Last,
    ];
}

/// Position dans la grille : indice de ligne, indice de colonne visible.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct GridPos {
    /// Ligne (ligne vide de fin comprise).
    pub row: usize,
    /// Colonne visible.
    pub col: usize,
}

/// Forme de la grille.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GridShape {
    /// Lignes, ligne vide de fin comprise.
    pub rows: usize,
    /// Colonnes visibles.
    pub cols: usize,
    /// Lignes visibles (pas d'une page).
    pub page: usize,
}

/// Position après le déplacement `mv` dans `shape` (P45 : toujours dans la
/// grille, idempotente au bord ; grille vide ⇒ (0, 0)). Une position de
/// départ hors de la grille est d'abord ramenée dans la grille.
#[must_use]
pub fn step(pos: GridPos, mv: NavMove, shape: GridShape) -> GridPos {
    if shape.rows == 0 || shape.cols == 0 {
        return GridPos::default();
    }
    let last_row = shape.rows.saturating_sub(1);
    let last_col = shape.cols.saturating_sub(1);
    let row = pos.row.min(last_row);
    let col = pos.col.min(last_col);
    // Une page avance de `page − 1` lignes, au moins une.
    let page = shape.page.saturating_sub(1).max(1);
    let (row, col) = match mv.base() {
        NavMove::Up => (row.saturating_sub(1), col),
        NavMove::Down => (row.saturating_add(1).min(last_row), col),
        NavMove::Left => (row, col.saturating_sub(1)),
        NavMove::Right => (row, col.saturating_add(1).min(last_col)),
        NavMove::PageUp => (row.saturating_sub(page), col),
        NavMove::PageDown => (row.saturating_add(page).min(last_row), col),
        NavMove::RowStart => (row, 0),
        NavMove::RowEnd => (row, last_col),
        NavMove::FirstRow => (0, col),
        NavMove::LastRow => (last_row, col),
        NavMove::First => (0, 0),
        NavMove::Last => (last_row, last_col),
        // Ramené par `base` (jamais atteint).
        NavMove::Within(_) => (row, col),
    };
    GridPos { row, col }
}

/// Ligne désignée par le curseur (défaut : la ligne vide).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RowRef<U> {
    /// Ligne d'un élément (UID).
    Item(U),
    /// Ligne vide de fin (nouvelle tâche, nouvelle ressource).
    #[default]
    Blank,
}

impl<U: Copy> RowRef<U> {
    /// Ligne d'une sélection : l'élément sélectionné, ou la ligne vide
    /// sans sélection.
    #[must_use]
    pub fn from_selection(selection: Option<U>) -> Self {
        selection.map_or(Self::Blank, Self::Item)
    }

    /// UID de la ligne (aucun pour la ligne vide).
    #[must_use]
    pub const fn item(self) -> Option<U> {
        match self {
            Self::Item(uid) => Some(uid),
            Self::Blank => None,
        }
    }
}

/// Cellule courante d'une grille ; défaut : ligne vide, colonne par
/// défaut, aucune ancre.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellCursor<U, C> {
    /// Ligne.
    pub row: RowRef<U>,
    /// Colonne.
    pub column: C,
    /// Ancre de l'extension de la sélection (Maj+flèches, H-UI-16).
    pub anchor: Option<U>,
    /// Ancre de colonnes du bloc de cellules (ADR-0018, H-UI-87) : le bloc
    /// couvre les colonnes visibles de cette colonne à `column` ; `None` :
    /// la colonne active seule.
    pub anchor_column: Option<C>,
}

impl<U, C: Default> Default for CellCursor<U, C> {
    fn default() -> Self {
        Self {
            row: RowRef::Blank,
            column: C::default(),
            anchor: None,
            anchor_column: None,
        }
    }
}

/// Déplacement **dans** un bloc de plus d'une cellule (H-UI-93, S61 « Move
/// within a selection … ») : Entrée, Maj+Entrée, Tab, Maj+Tab ; servi par
/// `Block::step`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InBlockMove {
    /// Entrée : cellule du dessous ; en bas, haut de la colonne suivante ;
    /// à la fin, première cellule.
    Down,
    /// Maj+Entrée : l'inverse de `Down`.
    Up,
    /// Tab : cellule de droite ; en bout, début de la ligne suivante ; à la
    /// fin, première cellule.
    Right,
    /// Maj+Tab : l'inverse de `Right`.
    Left,
}

/// Vitesse plancher du défilement automatique, en lignes par seconde.
pub const AUTOSCROLL_MIN: f32 = 4.0;
/// Vitesse plafond du défilement automatique, en lignes par seconde.
pub const AUTOSCROLL_MAX: f32 = 40.0;

/// Vitesse du défilement automatique pendant un glisser (H-UI-91), en
/// lignes (ou colonnes) par seconde, signée : négative vers le haut (ou la
/// gauche), positive vers le bas (ou la droite), 0 hors des zones.
///
/// `pointer` : coordonnée du pointeur ; `top`, `bottom` : bords du corps ;
/// `extent` : hauteur d'une ligne (zone de 20 px au bord pour
/// l'horizontal de la table). Zone : à moins de `extent` d'un bord, ou au-delà ;
/// `d` = distance au seuil (bord ± `extent`), vitesse
/// `clamp(4 + 8·d / extent, 4, 40)`. Une valeur non finie, une ligne non
/// positive ou un corps inversé ⇒ 0 (règle 10). Corps plus petit que deux
/// lignes : le bord le plus proche décide.
#[must_use]
pub fn autoscroll_speed(pointer: f32, top: f32, bottom: f32, extent: f32) -> f32 {
    if !pointer.is_finite() || !top.is_finite() || !bottom.is_finite() || !extent.is_finite() {
        return 0.0;
    }
    if extent <= 0.0 || bottom < top {
        return 0.0;
    }
    let speed = |d: f32| {
        let v = AUTOSCROLL_MIN + 8.0 * d / extent;
        if v.is_finite() {
            v.clamp(AUTOSCROLL_MIN, AUTOSCROLL_MAX)
        } else {
            AUTOSCROLL_MAX
        }
    };
    let upper = top + extent;
    let lower = bottom - extent;
    let middle = top + (bottom - top) / 2.0;
    if pointer < upper && pointer <= middle {
        -speed(upper - pointer)
    } else if pointer > lower && pointer >= middle {
        speed(pointer - lower)
    } else {
        0.0
    }
}

/// Pas d'image retenu au plus par le défilement automatique, en secondes :
/// une image lente ne fait pas sauter la vue (règle 10).
pub const AUTOSCROLL_MAX_DT: f32 = 0.1;

/// Défilement après une image de défilement automatique (H-UI-91) :
/// `offset + speed × extent × dt` (`speed` en lignes ou colonnes par
/// seconde, signée, [`autoscroll_speed`] ; `extent` : hauteur d'une ligne
/// ou largeur d'une colonne), borné à `[0, max]`. `dt` non fini ou non
/// positif ⇒ 1/60 s, plafonné à [`AUTOSCROLL_MAX_DT`]. `None` : rien ne
/// bouge (vitesse nulle, déjà au bord) ou valeur non finie.
#[must_use]
pub fn autoscroll_offset(offset: f32, speed: f32, extent: f32, dt: f32, max: f32) -> Option<f32> {
    if !offset.is_finite() || !speed.is_finite() || !extent.is_finite() || !max.is_finite() {
        return None;
    }
    if speed == 0.0 || extent <= 0.0 {
        return None;
    }
    let dt = if dt.is_finite() && dt > 0.0 {
        dt.min(AUTOSCROLL_MAX_DT)
    } else {
        1.0 / 60.0
    };
    let target = (offset + speed * extent * dt).clamp(0.0, max.max(0.0));
    (target.is_finite() && (target - offset).abs() > f32::EPSILON).then_some(target)
}

/// Défilement qui rend visible l'élément `index` (étendue `extent` par
/// élément, `len` éléments, fenêtre de `view` à partir de `offset`), avec
/// une ligne de marge sauf aux bords (H-UI-17) ; `None` : rien à faire, ou
/// valeur non finie. Le défilement rendu est minimal : la fenêtre ne bouge
/// que du nécessaire, et n'est jamais négative. Une fenêtre trop petite
/// pour la marge (moins de trois éléments) montre l'élément seul.
#[must_use]
pub fn ensure_visible(offset: f32, index: usize, extent: f32, view: f32, len: usize) -> Option<f32> {
    if !offset.is_finite() || !extent.is_finite() || !view.is_finite() || extent <= 0.0 || view <= 0.0 {
        return None;
    }
    if len == 0 {
        return None;
    }
    let index = index.min(len.saturating_sub(1));
    let margin = usize::from(view >= 3.0 * extent);
    let top = index_to_f32(index.saturating_sub(margin)) * extent;
    let bottom = index_to_f32(index.saturating_add(1).saturating_add(margin).min(len)) * extent;
    let target = if top < offset {
        top
    } else if bottom > offset + view {
        (bottom - view).max(0.0)
    } else {
        return None;
    };
    (target.is_finite() && (target - offset).abs() > f32::EPSILON).then_some(target)
}

/// Nombre de lignes entièrement visibles dans une fenêtre de `view` points
/// (lignes de `extent` points) : pas d'une page ; 0 si une valeur n'est pas
/// finie ou pas positive.
#[must_use]
pub fn page_rows(view: f32, extent: f32) -> usize {
    if !view.is_finite() || !extent.is_finite() || view <= 0.0 || extent <= 0.0 {
        return 0;
    }
    f32_to_index((view / extent).floor())
}

/// Défilement horizontal du Gantt qui place l'abscisse `x` (pixels depuis
/// l'origine de l'échelle, [`crate::timescale::TimeAxis::x`] d'un axe sans
/// décalage) au huitième de la largeur visible `width` (H-UI-84) ; `None`
/// si une valeur n'est pas finie ou si la largeur est nulle. Non borné :
/// l'appelant ramène le défilement dans `[0, max]`.
#[must_use]
pub fn scroll_for_x(x: f32, width: f32) -> Option<f32> {
    if !x.is_finite() || !width.is_finite() || width <= 0.0 {
        return None;
    }
    let target = x - width / 8.0;
    target.is_finite().then_some(target)
}

/// Ligne `target` ramenée hors des en-têtes de groupe (finitions P3,
/// H-UI-131) : la première ligne qui n'en est pas un dans le sens du
/// déplacement depuis `from` (vers le bas si `target` ≥ `from`), sinon
/// dans l'autre sens, sinon `from`. Sans en-tête : `target` tel quel.
#[must_use]
pub fn skip_headers(rows: VisibleRows<'_>, target: usize, from: usize) -> usize {
    let header = |i: usize| rows.header(DisplayRow::at(i)).is_some();
    if !header(target) {
        return target;
    }
    let last = rows.len();
    let down = (target..=last).find(|&i| !header(i));
    let up = (0..target).rev().find(|&i| !header(i));
    let found = if target >= from { down.or(up) } else { up.or(down) };
    found.unwrap_or(from)
}

impl crate::App {
    /// Navigation au clavier (H-UI-13 à H-UI-17) : [`step`] sur
    /// (ligne, colonne visible), ligne vide comprise ; la sélection suit la
    /// ligne du curseur (`extend` : plage depuis l'ancre, sans aller sur la
    /// ligne vide) ; le défilement vertical suit le curseur avec une ligne
    /// de marge ([`ensure_visible`]).
    pub(crate) fn navigate(&mut self, mv: NavMove, extend: bool) {
        // Entrée et Tab dans un bloc de plus d'une cellule (H-UI-93).
        if let NavMove::Within(within) = mv
            && !extend
            && self.move_in_block(within)
        {
            return;
        }
        let mv = mv.base();
        let Some(model) = &self.rows else {
            return;
        };
        // Lignes affichées (J8, lot L5) : la navigation suit la vue.
        let rows = VisibleRows::new(model, &self.row_view);
        let len = rows.len();
        let visible: Vec<Column> = self.state.visible_columns().map(|state| state.column).collect();
        let row = match self.selection {
            // Vue regroupée (finitions P3) : une tâche répétée part de la
            // ligne où est le curseur.
            Some(uid) => rows
                .display_near(uid, self.view.cursor_row)
                .map_or(0, DisplayRow::index),
            None => len,
        };
        let col = visible
            .iter()
            .position(|column| *column == self.view.cursor.column)
            .unwrap_or(0);
        let shape = GridShape {
            rows: len.saturating_add(1),
            cols: visible.len(),
            page: page_rows(self.view.last_body_height, self.row_h()),
        };
        let mut target = step(GridPos { row, col }, mv, shape);
        // En-têtes de groupe (finitions P3, H-UI-131) : jamais une cellule
        // du curseur ; le déplacement continue jusqu'à la tâche suivante
        // dans son sens, sinon dans l'autre, sinon rien ne bouge.
        target.row = skip_headers(rows, target.row, row);
        let extend = extend && len > 0;
        if extend {
            // L'extension s'arrête à la dernière tâche.
            target.row = target.row.min(len.saturating_sub(1));
        }
        let from_column = self.view.cursor.column;
        if let Some(column) = visible.get(target.col) {
            self.view.cursor.column = *column;
        }
        let uid = rows.uid(DisplayRow::at(target.row));
        if rows.view().is_grouped() {
            self.view.cursor_row = Some(DisplayRow::at(target.row));
        }
        match uid {
            Some(uid) if extend => {
                // H-UI-92 : plage de lignes depuis l'ancre (type gardé) et
                // plage de colonnes depuis l'ancre de colonnes, posée sur la
                // colonne de départ si le bloc n'en a pas.
                let anchor = self.selected.anchor().or(self.selection).unwrap_or(uid);
                let kind = self.selected.kind();
                self.selected = Selection::range(anchor, uid, kind, rows);
                self.selection = Some(uid);
                self.view.cursor.anchor = Some(anchor);
                if self
                    .view
                    .cursor
                    .anchor_column
                    .is_none_or(|c| !visible.contains(&c))
                {
                    self.view.cursor.anchor_column = Some(from_column);
                }
            }
            _ => {
                self.selection = uid;
                self.selected = Selection::single(uid);
                // Bloc réduit à la cellule active (ADR-0018).
                self.view.cursor.anchor_column = None;
            }
        }
        self.sync_selection();
        if extend {
            self.view.cursor.anchor = self.selected.anchor();
        }
        let view_h = self.view.last_body_height;
        if let Some(y) = ensure_visible(
            self.view.last_scroll_y,
            target.row,
            self.row_h(),
            view_h,
            shape.rows,
        ) {
            self.view.pending_scroll_y = Some(y);
            self.view.last_scroll_y = y;
        }
        self.follow_cursor_x();
    }

    /// Entrée, Tab et leurs Maj dans un bloc de plus d'une cellule
    /// (H-UI-93) : la cellule active passe à la suivante du bloc
    /// ([`crate::block::Block::step`]), la sélection (lignes, ancre,
    /// colonnes) ne change pas ; le défilement suit. Rend faux (rien fait)
    /// pour un bloc d'une seule cellule ou d'une seule ligne entière.
    fn move_in_block(&mut self, mv: InBlockMove) -> bool {
        let visible: Vec<Column> = self.state.visible_columns().map(|state| state.column).collect();
        let (uid, column, span, index, len) = {
            let Some(rows) = self.visible_rows() else {
                return false;
            };
            let block = crate::block::resolve(&self.selected, &self.view.cursor, rows, &visible);
            // Une seule ligne entière (clic sur un ID : « la tâche ») se
            // parcourt comme sa cellule active (H-UI-13, H-UI-14).
            let one_row = block.kind == crate::selection::SelectionKind::Rows && block.rows.len() == 1;
            if block.is_single() || one_row {
                return false;
            }
            let Some(from) = block.active.0.item().and_then(|uid| rows.display_of(uid)) else {
                return false;
            };
            let (d, column) = block.step((from, block.active.1), mv);
            let Some(uid) = rows.uid(d) else {
                return false;
            };
            let (Some(&first), Some(&last)) = (block.columns.first(), block.columns.last()) else {
                return false;
            };
            (uid, column, (first, last), d.index(), rows.len())
        };
        self.selected = self.selected.clone().moved_within(uid, span);
        self.selection = Some(uid);
        self.view.cursor.column = column;
        self.sync_selection();
        if let Some(y) = ensure_visible(
            self.view.last_scroll_y,
            index,
            self.row_h(),
            self.view.last_body_height,
            len.saturating_add(1),
        ) {
            self.view.pending_scroll_y = Some(y);
            self.view.last_scroll_y = y;
        }
        self.follow_cursor_x();
        true
    }

    /// « Défiler jusqu'à la tâche » active (ADR-0017, H-UI-84) : la table
    /// défile jusqu'à sa ligne (marge d'une ligne, [`ensure_visible`]) et
    /// le Gantt place le début de sa barre (jalon : son instant ; tâche
    /// inactive : son placement d'affichage, R-062) à 1/8 de la largeur
    /// visible, par `TimeAxis::x` (date → pixel, dates du moteur). Le
    /// réducteur borne ensuite le défilement horizontal. Tâche masquée par
    /// la vue (filtre, mode plan, récapitulative repliée) : message, rien
    /// ne bouge. Sans tâche active, hors des vues Gantt : rien.
    pub(crate) fn scroll_to_task(&mut self) {
        use plannercraft_core::schedule::TaskOutcome;

        if self.state.view != crate::state::ViewKind::Gantt {
            return;
        }
        let Some(uid) = self.selection else {
            return;
        };
        let Some((row, len)) = self
            .visible_rows()
            .map(|rows| (rows.display_of(uid).map(DisplayRow::index), rows.len()))
        else {
            return;
        };
        let Some(row) = row else {
            let text = self.texts.get(crate::i18n::TextId::ScrollTaskHidden);
            self.push_message(plannercraft_core::Severity::Info, text);
            return;
        };
        if let Some(y) = ensure_visible(
            self.view.last_scroll_y,
            row,
            self.row_h(),
            self.view.last_body_height,
            len.saturating_add(1),
        ) {
            self.view.pending_scroll_y = Some(y);
            self.view.last_scroll_y = y;
        }
        let Some(document) = &self.document else {
            return;
        };
        let start = match document.schedule().get(uid) {
            Some(TaskOutcome::Scheduled(s)) => Some(s.start),
            Some(TaskOutcome::Excluded { display, .. }) => display.map(|span| span.start),
            None => None,
        };
        let Some(start) = start else {
            return;
        };
        let axis = crate::timescale::TimeAxis::new(document.range, self.state.zoom, 0.0);
        if let Some(x) = scroll_for_x(axis.x(start), self.view.last_gantt_width) {
            self.view.gantt_scroll_x = x;
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::cast_precision_loss,
    reason = "tests : un échec doit s'arrêter net et lisiblement ; valeurs exactes et petites"
)]
mod tests {
    use proptest::prelude::*;

    use super::{
        AUTOSCROLL_MAX, AUTOSCROLL_MIN, CellCursor, GridPos, GridShape, InBlockMove, NavMove, RowRef,
        autoscroll_offset, autoscroll_speed, ensure_visible, page_rows, scroll_for_x, step,
    };

    const fn shape(rows: usize, cols: usize, page: usize) -> GridShape {
        GridShape { rows, cols, page }
    }

    const fn pos(row: usize, col: usize) -> GridPos {
        GridPos { row, col }
    }

    #[test]
    fn nav_step_contract_stays_in_grid() {
        let s = shape(3, 2, 20);
        let far = pos(9, 9);
        assert_eq!(step(far, NavMove::Down, s), pos(2, 1));
        assert_eq!(step(far, NavMove::Up, GridShape::default()), GridPos::default());
    }

    #[test]
    fn nav_step_arrows_and_edges() {
        let s = shape(10, 6, 20);
        assert_eq!(step(pos(3, 2), NavMove::Down, s), pos(4, 2));
        assert_eq!(step(pos(3, 2), NavMove::Up, s), pos(2, 2));
        assert_eq!(step(pos(3, 2), NavMove::Left, s), pos(3, 1));
        assert_eq!(step(pos(3, 2), NavMove::Right, s), pos(3, 3));
        // Au bord, rien ne bouge (H-UI-13, H-UI-14).
        assert_eq!(step(pos(0, 0), NavMove::Up, s), pos(0, 0));
        assert_eq!(step(pos(0, 0), NavMove::Left, s), pos(0, 0));
        assert_eq!(step(pos(9, 5), NavMove::Down, s), pos(9, 5));
        assert_eq!(step(pos(9, 5), NavMove::Right, s), pos(9, 5));
    }

    #[test]
    fn nav_step_home_end_first_last() {
        let s = shape(10, 6, 20);
        assert_eq!(step(pos(4, 3), NavMove::RowStart, s), pos(4, 0));
        assert_eq!(step(pos(4, 3), NavMove::RowEnd, s), pos(4, 5));
        assert_eq!(step(pos(4, 3), NavMove::FirstRow, s), pos(0, 3));
        assert_eq!(step(pos(4, 3), NavMove::LastRow, s), pos(9, 3));
        assert_eq!(step(pos(4, 3), NavMove::First, s), pos(0, 0));
        assert_eq!(step(pos(4, 3), NavMove::Last, s), pos(9, 5));
    }

    /// H-UI-15 : 20 lignes visibles, ligne 5, Page bas ⇒ ligne 24.
    #[test]
    fn nav_step_page() {
        let s = shape(100, 6, 20);
        assert_eq!(step(pos(5, 1), NavMove::PageDown, s), pos(24, 1));
        assert_eq!(step(pos(24, 1), NavMove::PageUp, s), pos(5, 1));
        assert_eq!(step(pos(3, 1), NavMove::PageUp, s), pos(0, 1));
        assert_eq!(step(pos(90, 1), NavMove::PageDown, s), pos(99, 1));
    }

    /// Page de 0 ou 1 ligne : une ligne par appui (jamais immobile).
    #[test]
    fn nav_step_page_zero() {
        for page in [0, 1, 2] {
            let s = shape(10, 3, page);
            assert_eq!(step(pos(4, 0), NavMove::PageDown, s), pos(5, 0), "page {page}");
            assert_eq!(step(pos(4, 0), NavMove::PageUp, s), pos(3, 0), "page {page}");
        }
    }

    #[test]
    fn nav_step_empty_grid() {
        for mv in NavMove::ALL {
            for s in [shape(0, 0, 0), shape(0, 5, 20), shape(5, 0, 20)] {
                assert_eq!(step(pos(3, 3), mv, s), GridPos::default(), "{mv:?} {s:?}");
            }
        }
        // Une seule cellule (la ligne vide d'un projet sans tâche).
        for mv in NavMove::ALL {
            assert_eq!(step(pos(0, 0), mv, shape(1, 1, 20)), pos(0, 0), "{mv:?}");
        }
    }

    #[test]
    fn nav_row_ref_and_cursor_default() {
        assert_eq!(RowRef::from_selection(Some(3_u32)), RowRef::Item(3));
        assert_eq!(RowRef::<u32>::from_selection(None), RowRef::Blank);
        assert_eq!(RowRef::Item(3_u32).item(), Some(3));
        assert_eq!(RowRef::<u32>::Blank.item(), None);
        let c: CellCursor<u32, u8> = CellCursor::default();
        assert_eq!(c.row, RowRef::Blank);
        assert_eq!(c.column, 0);
        assert_eq!(c.anchor, None);
    }

    /// H-UI-17 : lignes visibles 0-19 (22 px), curseur 19 ⇒ première
    /// visible 1 ; marge d'une ligne sauf aux bords.
    #[test]
    fn nav_ensure_visible_margin() {
        let (h, view) = (22.0, 440.0);
        assert_eq!(ensure_visible(0.0, 18, h, view, 100), None);
        assert_eq!(ensure_visible(0.0, 19, h, view, 100), Some(22.0));
        // Vers le haut : la ligne au-dessus reste visible.
        assert_eq!(ensure_visible(220.0, 10, h, view, 100), Some(198.0));
        assert_eq!(ensure_visible(220.0, 11, h, view, 100), None);
        // Bords : pas de marge hors de la grille.
        assert_eq!(ensure_visible(100.0, 0, h, view, 100), Some(0.0));
        assert_eq!(ensure_visible(0.0, 99, h, view, 100), Some(100.0 * h - view));
        // Grand saut : défilement minimal (cible en bas avec sa marge).
        assert_eq!(ensure_visible(0.0, 50, h, view, 100), Some(52.0 * h - view));
        // Indice au-delà de la fin : ramené à la dernière ligne.
        assert_eq!(ensure_visible(0.0, 500, h, view, 100), Some(100.0 * h - view));
    }

    #[test]
    fn nav_ensure_visible_degenerate() {
        assert_eq!(ensure_visible(f32::NAN, 3, 22.0, 440.0, 10), None);
        assert_eq!(ensure_visible(0.0, 3, f32::INFINITY, 440.0, 10), None);
        assert_eq!(ensure_visible(0.0, 3, 22.0, f32::NAN, 10), None);
        assert_eq!(ensure_visible(0.0, 3, 0.0, 440.0, 10), None);
        assert_eq!(ensure_visible(0.0, 3, 22.0, 0.0, 10), None);
        assert_eq!(ensure_visible(0.0, 3, 22.0, 440.0, 0), None);
        // Fenêtre plus petite que trois lignes : l'élément seul.
        assert_eq!(ensure_visible(0.0, 5, 22.0, 30.0, 10), Some(6.0 * 22.0 - 30.0));
        assert_eq!(ensure_visible(200.0, 5, 22.0, 30.0, 10), Some(110.0));
    }

    #[test]
    fn nav_page_rows() {
        assert_eq!(page_rows(440.0, 22.0), 20);
        assert_eq!(page_rows(450.0, 22.0), 20);
        assert_eq!(page_rows(21.0, 22.0), 0);
        assert_eq!(page_rows(f32::NAN, 22.0), 0);
        assert_eq!(page_rows(440.0, 0.0), 0);
        assert_eq!(page_rows(f32::INFINITY, 22.0), 0);
    }

    /// H-UI-84 : le début de la barre arrive au huitième de la largeur.
    #[test]
    fn h_ui_84_scroll_for_x() {
        assert_eq!(scroll_for_x(1_000.0, 800.0), Some(900.0));
        // Non borné ici : le réducteur ramène à 0.
        assert_eq!(scroll_for_x(50.0, 800.0), Some(-50.0));
        assert_eq!(scroll_for_x(f32::NAN, 800.0), None);
        assert_eq!(scroll_for_x(10.0, 0.0), None);
        assert_eq!(scroll_for_x(10.0, f32::INFINITY), None);
    }

    /// H-UI-91 : zones d'une ligne aux bords (corps de 100 à 500, lignes de
    /// 20) ; 4 lignes/s au seuil, + 8 par ligne au-delà, plafond 40 ; signe
    /// selon le bord ; 0 pour toute valeur non finie ou dégénérée.
    #[test]
    fn nav_autoscroll_speed_bounds() {
        let (top, bottom, h) = (100.0, 500.0, 20.0);
        assert_eq!(autoscroll_speed(300.0, top, bottom, h), 0.0);
        assert_eq!(autoscroll_speed(120.0, top, bottom, h), 0.0);
        assert_eq!(autoscroll_speed(480.0, top, bottom, h), 0.0);
        assert_eq!(autoscroll_speed(110.0, top, bottom, h), -8.0);
        assert_eq!(autoscroll_speed(490.0, top, bottom, h), 8.0);
        assert_eq!(autoscroll_speed(100.0, top, bottom, h), -12.0);
        assert_eq!(autoscroll_speed(520.0, top, bottom, h), 20.0);
        // Au-delà du bord : plafond.
        assert_eq!(autoscroll_speed(5_000.0, top, bottom, h), AUTOSCROLL_MAX);
        assert_eq!(autoscroll_speed(-5_000.0, top, bottom, h), -AUTOSCROLL_MAX);
        assert_eq!(autoscroll_speed(f32::MAX, top, bottom, h), AUTOSCROLL_MAX);
        // Juste dans la zone : plancher.
        let v = autoscroll_speed(480.001, top, bottom, h);
        assert!((AUTOSCROLL_MIN..AUTOSCROLL_MIN + 0.01).contains(&v), "{v}");
        // Non fini, ligne nulle, corps inversé : 0.
        for (p, t, b, e) in [
            (f32::NAN, top, bottom, h),
            (f32::INFINITY, top, bottom, h),
            (300.0, f32::NEG_INFINITY, bottom, h),
            (300.0, top, f32::NAN, h),
            (300.0, top, bottom, f32::NAN),
            (90.0, top, bottom, 0.0),
            (90.0, top, bottom, -3.0),
            (90.0, bottom, top, h),
        ] {
            assert_eq!(autoscroll_speed(p, t, b, e), 0.0, "{p} {t} {b} {e}");
        }
        // Corps plus petit que deux lignes : le bord le plus proche.
        assert!(autoscroll_speed(101.0, 100.0, 120.0, h) < 0.0);
        assert!(autoscroll_speed(119.0, 100.0, 120.0, h) > 0.0);
    }

    /// H-UI-91 : une image à 60 Hz avance de `vitesse × ligne / 60`,
    /// bornée à `[0, max]` ; `dt` dégénéré ⇒ 1/60 s, plafonné à 0,1 s ;
    /// rien au bord, à vitesse nulle ou pour une valeur non finie.
    #[test]
    fn nav_autoscroll_offset_bounds() {
        let h = 20.0;
        assert_eq!(autoscroll_offset(100.0, 6.0, h, 0.05, 1_000.0), Some(106.0));
        assert_eq!(autoscroll_offset(100.0, -6.0, h, 0.05, 1_000.0), Some(94.0));
        assert_eq!(autoscroll_offset(100.0, 6.0, h, f32::NAN, 1_000.0), Some(102.0));
        assert_eq!(autoscroll_offset(100.0, 6.0, h, -1.0, 1_000.0), Some(102.0));
        assert_eq!(autoscroll_offset(100.0, 6.0, h, 10.0, 1_000.0), Some(112.0));
        // Bornes.
        assert_eq!(autoscroll_offset(990.0, 40.0, h, 0.1, 1_000.0), Some(1_000.0));
        assert_eq!(autoscroll_offset(1_000.0, 40.0, h, 0.1, 1_000.0), None);
        assert_eq!(autoscroll_offset(0.0, -40.0, h, 0.1, 1_000.0), None);
        assert_eq!(autoscroll_offset(5.0, 4.0, h, 0.1, -3.0), Some(0.0));
        // Rien à faire, ou non fini.
        assert_eq!(autoscroll_offset(100.0, 0.0, h, 0.1, 1_000.0), None);
        for (o, v, e, m) in [
            (f32::NAN, 4.0, h, 1_000.0),
            (100.0, f32::INFINITY, h, 1_000.0),
            (100.0, 4.0, f32::NAN, 1_000.0),
            (100.0, 4.0, 0.0, 1_000.0),
            (100.0, 4.0, h, f32::INFINITY),
        ] {
            assert_eq!(autoscroll_offset(o, v, e, 0.1, m), None, "{o} {v} {e} {m}");
        }
    }

    /// H-UI-93 : hors d'un bloc, Entrée, Maj+Entrée, Tab et Maj+Tab
    /// valent ↓, ↑, →, ← (H-UI-13, H-UI-14).
    #[test]
    fn nav_within_base_moves() {
        let s = shape(10, 6, 20);
        for (within, plain) in [
            (InBlockMove::Down, NavMove::Down),
            (InBlockMove::Up, NavMove::Up),
            (InBlockMove::Right, NavMove::Right),
            (InBlockMove::Left, NavMove::Left),
        ] {
            assert_eq!(NavMove::Within(within).base(), plain);
            assert_eq!(
                step(pos(4, 3), NavMove::Within(within), s),
                step(pos(4, 3), plain, s)
            );
        }
        assert_eq!(NavMove::PageDown.base(), NavMove::PageDown);
    }

    fn move_strategy() -> impl Strategy<Value = NavMove> {
        proptest::sample::select(NavMove::ALL.to_vec())
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(4_096))]

        /// P45 : pour toute grille et toute suite de déplacements, la
        /// position reste dans la grille, sans panique ; un déplacement
        /// répété au bord ne bouge plus ; `FirstRow` après `LastRow` vaut
        /// `FirstRow`.
        #[test]
        fn p45_nav_step_stays_in_grid(
            rows in 0_usize..400,
            cols in 0_usize..16,
            page in 0_usize..60,
            start_row in 0_usize..500,
            start_col in 0_usize..20,
            moves in proptest::collection::vec(move_strategy(), 0..40),
        ) {
            let s = shape(rows, cols, page);
            let mut p = pos(start_row, start_col);
            for mv in moves {
                let next = step(p, mv, s);
                if rows == 0 || cols == 0 {
                    prop_assert_eq!(next, GridPos::default());
                } else {
                    prop_assert!(next.row < rows && next.col < cols, "{:?} {:?} {:?}", next, mv, s);
                }
                // Idempotence au bord : un second appui identique ne bouge
                // plus si le premier n'a rien changé.
                if next == step(next, mv, s) {
                    prop_assert_eq!(step(step(next, mv, s), mv, s), next);
                }
                p = next;
            }
            let first = step(p, NavMove::FirstRow, s);
            prop_assert_eq!(step(step(p, NavMove::LastRow, s), NavMove::FirstRow, s), first);
            // Les déplacements absolus sont idempotents.
            for mv in [NavMove::RowStart, NavMove::RowEnd, NavMove::FirstRow, NavMove::LastRow, NavMove::First, NavMove::Last] {
                let once = step(p, mv, s);
                prop_assert_eq!(step(once, mv, s), once);
            }
        }

        /// H-UI-17 : la cible de `ensure_visible` montre l'élément (fenêtre
        /// d'au moins une ligne), reste finie et positive.
        #[test]
        fn p45_ensure_visible_shows_index(
            offset in 0.0_f32..20_000.0,
            index in 0_usize..1_000,
            view_rows in 1_usize..60,
            len in 1_usize..1_000,
        ) {
            let h = 22.0_f32;
            let view = h * view_rows as f32;
            let index = index.min(len - 1);
            let shown = ensure_visible(offset, index, h, view, len).unwrap_or(offset);
            prop_assert!(shown.is_finite() && shown >= 0.0);
            let top = index as f32 * h;
            prop_assert!(top >= shown - 0.01 && top + h <= shown + view + 0.01,
                "{} {} {} {}", shown, top, view, offset);
        }
    }
}
