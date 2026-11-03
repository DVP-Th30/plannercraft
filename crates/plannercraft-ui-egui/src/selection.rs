//! Sélection de tâches de la vue principale (lot L14) : ensemble d'UID,
//! ancre et tâche active.
//!
//! Un clic simple sélectionne une seule tâche ; Ctrl/Cmd+clic ajoute ou
//! retire une tâche ([`SelectMode::Toggle`]) ; Maj+clic sélectionne la plage
//! de lignes entre l'ancre et la tâche cliquée ([`SelectMode::Range`]).
//! L'ordre des clics est gardé : `link.add` en chaîne (Ctrl+F2) suit l'ordre
//! des clics pour une sélection faite au Ctrl+clic, l'ordre des ID pour une
//! plage (une plage s'enregistre dans l'ordre des lignes **affichées** : une
//! vue filtrée, triée ou repliée ne prend que les lignes visibles, J8 lot
//! L5).
//!
//! La tâche active (`focus`) est celle de `App::selection` : l'application
//! resynchronise la sélection quand elles divergent (clic simple, tâche
//! supprimée, nouveau document).
//!
//! Passe UX (ADR-0018, contrat L0, H-UI-87) : une sélection est un bloc de
//! cellules ([`SelectionKind::Cells`]) ou des lignes entières
//! ([`SelectionKind::Rows`]) ; les lignes sont celles de ce module, les
//! colonnes du bloc viennent du curseur de la table (`CellCursor::column`
//! et `anchor_column`), résolues par `block::resolve`. Ctrl/Cmd+clic sur la
//! seule tâche sélectionnée est sans effet (U08 : la cellule active ne passe
//! plus sur la ligne vide). Les gestes ([`SelectOp`]) passent par
//! [`apply_op`], fonction pure servie à l'application par
//! `App::select_op` (L0 : appui et « tout » ; L1 : le reste).

use std::collections::BTreeSet;

use plannercraft_core::TaskUid;

use crate::action::{PressMode, PressTarget, SelectOp};
use crate::column::Column;
use crate::nav::RowRef;
use crate::row_view::{DisplayRow, VisibleRows};
use crate::view::table::TableCursor;

/// Type d'une sélection (H-UI-87).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectionKind {
    /// Bloc de cellules : lignes de la sélection × colonnes visibles de
    /// l'ancre de colonnes à la colonne active.
    #[default]
    Cells,
    /// Lignes entières : toutes les colonnes visibles.
    Rows,
}

/// Mode d'un clic de sélection avec modificateur.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectMode {
    /// Ctrl/Cmd+clic : ajoute la tâche, ou la retire si elle était
    /// sélectionnée.
    Toggle,
    /// Maj+clic : plage de lignes de l'ancre à la tâche cliquée.
    Range,
}

/// Tâches sélectionnées.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    /// UID sélectionnés.
    uids: BTreeSet<TaskUid>,
    /// Ancre d'une plage (Maj+clic) : dernier clic simple ou Ctrl+clic.
    anchor: Option<TaskUid>,
    /// Tâche active (dernière cliquée), toujours dans `uids` si présente.
    focus: Option<TaskUid>,
    /// Mêmes UID que `uids`, dans l'ordre des clics (ordre des lignes pour
    /// une plage).
    order: Vec<TaskUid>,
    /// Bloc de cellules ou lignes entières (H-UI-87).
    kind: SelectionKind,
    /// Plage de colonnes figée du bloc (première, dernière), posée quand la
    /// cellule active circule dans le bloc (Entrée, Tab : H-UI-93) ou par
    /// Ctrl+Espace : le bloc ne se résout plus de l'ancre de colonnes à la
    /// colonne active, la cellule active pouvant être au milieu. `None` :
    /// résolution par l'ancre de colonnes (`block::resolve`).
    span: Option<(Column, Column)>,
}

impl Selection {
    /// Sélection d'au plus une tâche.
    #[must_use]
    pub fn single(uid: Option<TaskUid>) -> Self {
        uid.map_or_else(Self::default, |uid| Self {
            uids: BTreeSet::from([uid]),
            anchor: Some(uid),
            focus: Some(uid),
            order: vec![uid],
            kind: SelectionKind::Cells,
            span: None,
        })
    }

    /// Type de la sélection.
    #[must_use]
    pub const fn kind(&self) -> SelectionKind {
        self.kind
    }

    /// Même sélection, de type `kind`.
    #[must_use]
    pub const fn with_kind(mut self, kind: SelectionKind) -> Self {
        self.kind = kind;
        self
    }

    /// Toutes les lignes affichées, en lignes entières (Ctrl/Cmd+A,
    /// H-UI-92) ; ancre et tâche active : la première ligne affichée. Vue
    /// sans ligne : sélection vide.
    #[must_use]
    pub fn all(rows: VisibleRows<'_>) -> Self {
        let order: Vec<TaskUid> = (0..rows.len())
            .filter_map(|i| rows.uid(DisplayRow::at(i)))
            .collect();
        let first = order.first().copied();
        Self {
            uids: order.iter().copied().collect(),
            anchor: first,
            focus: first,
            order,
            kind: SelectionKind::Rows,
            span: None,
        }
    }

    /// Plage des lignes affichées de `anchor` à `to` (bornes comprises,
    /// ordre des lignes), de type `kind` ; ancre `anchor`, tâche active
    /// `to`. Une borne absente de la vue ramène la plage à `to` seule.
    #[must_use]
    pub fn range(anchor: TaskUid, to: TaskUid, kind: SelectionKind, rows: VisibleRows<'_>) -> Self {
        let mut selection = Self::single(Some(anchor));
        selection.extend_to(to, rows);
        selection.with_kind(kind)
    }

    /// Plage de colonnes figée du bloc (H-UI-93), voir le champ `span`.
    #[must_use]
    pub const fn column_span(&self) -> Option<(Column, Column)> {
        self.span
    }

    /// Même sélection, cellule active déplacée **dans** le bloc (H-UI-93) :
    /// tâche active `focus` (ignorée si elle n'est pas sélectionnée), ancre
    /// et ordre gardés, colonnes du bloc figées à `span`.
    #[must_use]
    pub fn moved_within(mut self, focus: TaskUid, span: (Column, Column)) -> Self {
        if self.uids.contains(&focus) {
            self.focus = Some(focus);
        }
        self.span = Some(span);
        self
    }

    /// UID sélectionnés.
    #[must_use]
    pub const fn uids(&self) -> &BTreeSet<TaskUid> {
        &self.uids
    }

    /// Tâche active.
    #[must_use]
    pub const fn focus(&self) -> Option<TaskUid> {
        self.focus
    }

    /// Ancre des plages.
    #[must_use]
    pub const fn anchor(&self) -> Option<TaskUid> {
        self.anchor
    }

    /// Vrai si `uid` est sélectionnée.
    #[must_use]
    pub fn contains(&self, uid: TaskUid) -> bool {
        self.uids.contains(&uid)
    }

    /// Nombre de tâches sélectionnées.
    #[must_use]
    pub fn len(&self) -> usize {
        self.uids.len()
    }

    /// Aucune tâche sélectionnée.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.uids.is_empty()
    }

    /// UID dans l'ordre des clics (ordre des lignes pour une plage).
    #[must_use]
    pub fn click_order(&self) -> &[TaskUid] {
        &self.order
    }

    /// Applique un clic avec modificateur sur `uid` ; `rows` donne l'ordre
    /// des lignes affichées d'une plage. Une ancre absente de la vue (tâche
    /// supprimée ou masquée) ramène la plage à la seule tâche cliquée.
    pub fn click(&mut self, uid: TaskUid, mode: SelectMode, rows: VisibleRows<'_>) {
        self.span = None;
        match mode {
            SelectMode::Toggle => self.toggle(uid),
            SelectMode::Range => self.extend_to(uid, rows),
        }
    }

    /// Ctrl/Cmd+clic ; sans effet sur la seule tâche sélectionnée (U08,
    /// H-UI-87 : jamais la dernière).
    fn toggle(&mut self, uid: TaskUid) {
        if self.uids.len() == 1 && self.uids.contains(&uid) {
            return;
        }
        if self.uids.remove(&uid) {
            self.order.retain(|u| *u != uid);
            self.focus = self.order.last().copied();
            self.anchor = self.focus;
        } else {
            self.uids.insert(uid);
            self.order.push(uid);
            self.focus = Some(uid);
            self.anchor = Some(uid);
        }
    }

    /// Maj+clic : remplace la sélection par la plage ancre → `uid` (ancre
    /// gardée), dans l'ordre des lignes.
    fn extend_to(&mut self, uid: TaskUid, rows: VisibleRows<'_>) {
        let anchor = self.anchor.unwrap_or(uid);
        let (Some(a), Some(b)) = (rows.display_of(anchor), rows.display_of(uid)) else {
            *self = Self::single(Some(uid)).with_kind(self.kind);
            return;
        };
        let (low, high) = if a <= b { (a, b) } else { (b, a) };
        let order: Vec<TaskUid> = (low.0..=high.0).filter_map(|d| rows.uid(DisplayRow(d))).collect();
        self.uids = order.iter().copied().collect();
        self.order = order;
        self.anchor = Some(anchor);
        self.focus = Some(uid);
    }

    /// Garde les tâches pour lesquelles `keep` est vrai (après une
    /// édition) ; une tâche active ou une ancre retirées passent à la
    /// dernière tâche cliquée restante.
    pub fn retain(&mut self, mut keep: impl FnMut(TaskUid) -> bool) {
        self.order.retain(|uid| keep(*uid));
        self.uids = self.order.iter().copied().collect();
        if self.focus.is_some_and(|uid| !self.uids.contains(&uid)) {
            self.focus = self.order.last().copied();
        }
        if self.anchor.is_some_and(|uid| !self.uids.contains(&uid)) {
            self.anchor = self.focus;
        }
    }
}

/// Applique le geste de sélection `op` à la sélection `sel` et au curseur
/// de la table `cursor` (lignes affichées `rows`, colonnes visibles
/// `visible`, dans l'ordre de la vue) ; rend vrai si le geste a été servi.
///
/// Fonction pure du contrat L0 (ADR-0018) : la ligne du curseur suit la
/// tâche active de `sel` (la ligne vide sans sélection). Servis en L0 :
/// - `Press { target: Cell | RowHeader, .. }` (H-UI-87, H-UI-89) :
///   `Replace` pose la cellule (ou la ligne entière) seule et l'ancre de
///   colonnes ; `Extend` (Maj) étend la plage de lignes depuis l'ancre,
///   l'ancre de colonnes gardée ; `Toggle` (Ctrl/Cmd) ajoute ou retire la
///   ligne (bloc réduit à la colonne cliquée, jamais la dernière ligne). La
///   ligne vide n'entre jamais dans un bloc de plus d'une ligne : `Replace`
///   la sélectionne seule, `Extend` et `Toggle` sur elle sont sans effet.
/// - `Press { target: ColumnHeader, .. }` (H-UI-90) : colonne entière
///   (toutes les lignes affichées, cellule active en première ligne) ;
///   `Extend` : de l'ancre de colonnes à la colonne cliquée.
/// - `All` (H-UI-92) : toutes les lignes en lignes entières, cellule active
///   (première ligne, Nom).
/// - `DragTo` (H-UI-88, H-UI-89, lot L1) : plage de lignes de l'ancre à la
///   ligne sous le pointeur (type gardé : cellules ou lignes entières),
///   colonnes de l'ancre de colonnes à la colonne sous le pointeur ; la
///   ligne vide est ramenée à la dernière tâche affichée (jamais dans un
///   bloc de plus d'une ligne) ; sans ancre (ligne vide appuyée) : rien.
/// - `EntireRows` (Maj+Espace) : mêmes lignes, en lignes entières ; ligne
///   vide : rien.
/// - `EntireColumns` (Ctrl+Espace) : toutes les lignes affichées, colonnes
///   du bloc courant (toutes les colonnes visibles depuis des lignes
///   entières), cellule active gardée.
/// - `Collapse` (Maj+Retour arrière) : la cellule active seule.
#[allow(clippy::too_many_lines, reason = "un bras court par geste de `SelectOp`")]
pub fn apply_op(
    sel: &mut Selection,
    cursor: &mut TableCursor,
    op: SelectOp,
    rows: VisibleRows<'_>,
    visible: &[Column],
) -> bool {
    let column_ok = |c: Column| visible.contains(&c);
    match op {
        SelectOp::Press {
            row,
            column,
            target: target @ (PressTarget::Cell | PressTarget::RowHeader),
            mode,
        } => {
            let kind = if target == PressTarget::RowHeader {
                SelectionKind::Rows
            } else {
                SelectionKind::Cells
            };
            let column = if column_ok(column) { column } else { cursor.column };
            match (row, mode) {
                (RowRef::Blank, PressMode::Replace) => {
                    *sel = Selection::default();
                    cursor.column = column;
                    cursor.anchor_column = Some(column);
                }
                (RowRef::Blank, _) => return false,
                (RowRef::Item(uid), _) if rows.display_of(uid).is_none() => return false,
                (RowRef::Item(uid), PressMode::Replace) => {
                    *sel = Selection::single(Some(uid)).with_kind(kind);
                    cursor.column = column;
                    cursor.anchor_column = Some(column);
                }
                (RowRef::Item(uid), PressMode::Extend) => {
                    let anchor = sel.anchor().or(sel.focus()).unwrap_or(uid);
                    if cursor.anchor_column.is_none_or(|c| !column_ok(c)) {
                        cursor.anchor_column = Some(cursor.column);
                    }
                    *sel = Selection::range(anchor, uid, kind, rows);
                    cursor.column = column;
                }
                (RowRef::Item(uid), PressMode::Toggle) if sel.len() == 1 && sel.contains(uid) => {
                    // U08 : jamais la dernière ligne ; rien ne change.
                    return false;
                }
                (RowRef::Item(uid), PressMode::Toggle) => {
                    sel.click(uid, SelectMode::Toggle, rows);
                    cursor.column = column;
                    cursor.anchor_column = None;
                }
            }
        }
        SelectOp::Press {
            column,
            target: PressTarget::ColumnHeader,
            mode,
            ..
        } => {
            if !column_ok(column) {
                return false;
            }
            let all = Selection::all(rows).with_kind(SelectionKind::Cells);
            if mode == PressMode::Extend && cursor.anchor_column.is_some_and(column_ok) {
                // L'ancre de colonnes est gardée.
            } else {
                cursor.anchor_column = Some(column);
            }
            *sel = all;
            cursor.column = column;
        }
        SelectOp::All => {
            *sel = Selection::all(rows);
            if column_ok(Column::Name) {
                cursor.column = Column::Name;
            }
            cursor.anchor_column = None;
        }
        SelectOp::DragTo { row, column } => {
            let Some(anchor) = sel.anchor().or(sel.focus()) else {
                return false;
            };
            let to = match row {
                RowRef::Item(uid) if rows.display_of(uid).is_some() => uid,
                RowRef::Item(_) => return false,
                // Ligne vide : bornée à la dernière tâche affichée.
                RowRef::Blank => match rows
                    .len()
                    .checked_sub(1)
                    .and_then(|i| rows.uid(DisplayRow::at(i)))
                {
                    Some(uid) => uid,
                    None => return false,
                },
            };
            let column = if column_ok(column) { column } else { cursor.column };
            if cursor.anchor_column.is_none_or(|c| !column_ok(c)) {
                cursor.anchor_column = Some(cursor.column);
            }
            *sel = Selection::range(anchor, to, sel.kind(), rows);
            cursor.column = column;
        }
        SelectOp::EntireRows => {
            if sel.is_empty() {
                return false;
            }
            let mut rows_sel = sel.clone().with_kind(SelectionKind::Rows);
            rows_sel.span = None;
            *sel = rows_sel;
            cursor.anchor_column = None;
        }
        SelectOp::EntireColumns => {
            let (Some(&first), Some(&last)) = (visible.first(), visible.last()) else {
                return false;
            };
            if rows.is_empty() {
                return false;
            }
            // Colonnes du bloc courant : figées par `span` (la cellule
            // active peut être au milieu) ; lignes entières ⇒ toutes.
            let span = if sel.kind() == SelectionKind::Rows {
                Some((first, last))
            } else {
                sel.column_span()
            };
            let mut all = Selection::all(rows).with_kind(SelectionKind::Cells);
            if let Some(focus) = sel.focus().filter(|uid| rows.display_of(*uid).is_some()) {
                all.focus = Some(focus);
            }
            all.span = span.filter(|(a, b)| column_ok(*a) && column_ok(*b));
            *sel = all;
            if cursor.anchor_column.is_none_or(|c| !column_ok(c)) {
                cursor.anchor_column = Some(cursor.column);
            }
        }
        SelectOp::Collapse => {
            *sel = Selection::single(sel.focus());
            cursor.anchor_column = None;
        }
    }
    cursor.row = RowRef::from_selection(sel.focus());
    if sel.len() <= 1 {
        cursor.anchor = None;
    } else {
        cursor.anchor = sel.anchor();
    }
    true
}

impl crate::App {
    /// Geste de sélection (ADR-0018, `Action::SelectOp`) : [`apply_op`]
    /// sur la sélection et le curseur de la table, puis tâche active
    /// resynchronisée ([`crate::App::sync_selection`]). Sans document, ou
    /// pendant une saisie ouverte (H-UI-86 : le clic passe par la saisie) :
    /// rien.
    pub(crate) fn select_op(&mut self, op: SelectOp) {
        if self.view.edit.editing.is_some() {
            return;
        }
        let visible: Vec<Column> = self.state.visible_columns().map(|state| state.column).collect();
        let Some(rows) = self.visible_rows() else {
            return;
        };
        let mut selected = self.selected.clone();
        let mut cursor = self.view.cursor;
        if !apply_op(&mut selected, &mut cursor, op, rows, &visible) {
            return;
        }
        self.selected = selected;
        self.selection = self.selected.focus();
        self.view.cursor = cursor;
        self.sync_selection();
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "tests : un échec doit s'arrêter net et lisiblement ; indices bornés par les stratégies"
)]
mod tests {
    use plannercraft_core::{Moment, TaskUid};

    use std::collections::BTreeSet;

    use proptest::prelude::*;

    use super::{SelectMode, Selection, SelectionKind, apply_op};
    use crate::action::{PressMode, PressTarget, SelectOp};
    use crate::app::{App, Env};
    use crate::block::{Block, resolve};
    use crate::column::Column;
    use crate::nav::RowRef;
    use crate::row_view::{DisplayRow, RowView, SortInput, SortValue, ViewInput, ViewRow, VisibleRows};
    use crate::rows::{RowKind, RowModel};
    use crate::view::table::TableCursor;

    fn demo_rows() -> (RowModel, RowView) {
        let now: Moment = "2027-01-15T08:00".parse().unwrap();
        let mut app = App::new(crate::UiState::default(), Env::fixed(now));
        app.open_demo();
        (
            app.rows().expect("exemple chargé").clone(),
            app.row_view().clone(),
        )
    }

    fn uid_at(rows: VisibleRows<'_>, i: u32) -> TaskUid {
        rows.uid(DisplayRow(i)).unwrap()
    }

    #[test]
    fn single_and_empty() {
        let empty = Selection::single(None);
        assert!(empty.is_empty());
        assert_eq!(empty.focus(), None);
        let one = Selection::single(Some(TaskUid::new(4)));
        assert_eq!(one.len(), 1);
        assert_eq!(one.focus(), Some(TaskUid::new(4)));
        assert_eq!(one.anchor(), Some(TaskUid::new(4)));
        assert_eq!(one.click_order(), [TaskUid::new(4)]);
    }

    #[test]
    fn toggle_keeps_click_order() {
        let (model, view) = demo_rows();
        let rows = VisibleRows::new(&model, &view);
        let (a, b, c) = (uid_at(rows, 5), uid_at(rows, 1), uid_at(rows, 3));
        let mut s = Selection::single(Some(a));
        s.click(b, SelectMode::Toggle, rows);
        s.click(c, SelectMode::Toggle, rows);
        assert_eq!(s.click_order(), [a, b, c]);
        assert_eq!(s.focus(), Some(c));
        s.click(b, SelectMode::Toggle, rows);
        assert_eq!(s.click_order(), [a, c]);
        assert!(!s.contains(b));
        assert_eq!(s.focus(), Some(c));
        s.click(c, SelectMode::Toggle, rows);
        assert_eq!(s.click_order(), [a]);
        assert_eq!(s.focus(), Some(a));
        // ADR-0018 (U08, H-UI-87) : la dernière tâche n'est jamais retirée.
        s.click(a, SelectMode::Toggle, rows);
        assert_eq!(s, Selection::single(Some(a)));
    }

    #[test]
    fn range_in_row_order_from_anchor() {
        let (model, view) = demo_rows();
        let rows = VisibleRows::new(&model, &view);
        let mut s = Selection::single(Some(uid_at(rows, 6)));
        s.click(uid_at(rows, 3), SelectMode::Range, rows);
        let expected: Vec<TaskUid> = (3..=6).map(|i| uid_at(rows, i)).collect();
        assert_eq!(s.click_order(), expected.as_slice());
        assert_eq!(s.anchor(), Some(uid_at(rows, 6)));
        assert_eq!(s.focus(), Some(uid_at(rows, 3)));
        // Nouvelle plage depuis la même ancre : remplace la précédente.
        s.click(uid_at(rows, 8), SelectMode::Range, rows);
        let expected: Vec<TaskUid> = (6..=8).map(|i| uid_at(rows, i)).collect();
        assert_eq!(s.click_order(), expected.as_slice());
        // Ancre inconnue : la plage se réduit à la tâche cliquée.
        let mut s = Selection::single(Some(TaskUid::new(999_999)));
        s.click(uid_at(rows, 2), SelectMode::Range, rows);
        assert_eq!(s, Selection::single(Some(uid_at(rows, 2))));
    }

    #[test]
    fn retain_moves_focus_to_last_click() {
        let (model, view) = demo_rows();
        let rows = VisibleRows::new(&model, &view);
        let (a, b) = (uid_at(rows, 1), uid_at(rows, 2));
        let mut s = Selection::single(Some(a));
        s.click(b, SelectMode::Toggle, rows);
        s.retain(|uid| uid != b);
        assert_eq!(s.click_order(), [a]);
        assert_eq!(s.focus(), Some(a));
        assert_eq!(s.anchor(), Some(a));
        s.retain(|_| false);
        assert_eq!(s, Selection::default());
    }

    /// Colonnes visibles par défaut (ID, Nom, Durée, Début, Fin,
    /// Prédécesseurs…).
    fn default_columns() -> Vec<Column> {
        crate::UiState::default()
            .visible_columns()
            .map(|c| c.column)
            .collect()
    }

    fn press(row: RowRef<TaskUid>, column: Column, target: PressTarget, mode: PressMode) -> SelectOp {
        SelectOp::Press {
            row,
            column,
            target,
            mode,
        }
    }

    /// H-UI-87 (exemple du cahier, sur les lignes 2 à 5 de l'exemple) :
    /// appui (2, Durée), Maj+appui (5, Début) ⇒ bloc lignes 2 à 5 × Durée à
    /// Début, cellule active (5, Début), ancre (2, Durée).
    #[test]
    fn h_ui_87_block_resolution() {
        let (model, view) = demo_rows();
        let rows = VisibleRows::new(&model, &view);
        let visible = default_columns();
        let (two, five) = (uid_at(rows, 2), uid_at(rows, 5));
        let mut sel = Selection::default();
        let mut cursor = TableCursor::default();
        let op = press(
            RowRef::Item(two),
            Column::Duration,
            PressTarget::Cell,
            PressMode::Replace,
        );
        assert!(apply_op(&mut sel, &mut cursor, op, rows, &visible));
        let single = resolve(&sel, &cursor, rows, &visible);
        assert!(single.is_single());
        assert_eq!(single.active, (RowRef::Item(two), Column::Duration));
        let op = press(
            RowRef::Item(five),
            Column::Start,
            PressTarget::Cell,
            PressMode::Extend,
        );
        assert!(apply_op(&mut sel, &mut cursor, op, rows, &visible));
        let block = resolve(&sel, &cursor, rows, &visible);
        assert_eq!(block.kind, SelectionKind::Cells);
        assert_eq!(block.rows, (2..=5).map(DisplayRow).collect::<Vec<_>>());
        assert_eq!(block.columns, [Column::Duration, Column::Start]);
        assert_eq!(block.cells(), 8);
        assert_eq!(block.active, (RowRef::Item(five), Column::Start));
        assert_eq!(sel.anchor(), Some(two));
        assert!(block.contains(DisplayRow(3), Column::Duration));
        assert!(!block.contains(DisplayRow(3), Column::Name));
        assert!(!block.contains(DisplayRow(6), Column::Start));
        // Ancre de colonnes masquée : bloc réduit à la colonne active.
        let without_duration: Vec<Column> = visible
            .iter()
            .copied()
            .filter(|c| *c != Column::Duration)
            .collect();
        let narrowed = resolve(&sel, &cursor, rows, &without_duration);
        assert_eq!(narrowed.columns, [Column::Start]);
        // Ctrl/Cmd+A : tout, en lignes entières, active (première ligne, Nom).
        assert!(apply_op(&mut sel, &mut cursor, SelectOp::All, rows, &visible));
        let all = resolve(&sel, &cursor, rows, &visible);
        assert_eq!(all.kind, SelectionKind::Rows);
        assert_eq!(all.rows.len(), rows.len());
        assert_eq!(all.active, (RowRef::Item(uid_at(rows, 0)), Column::Name));
    }

    /// U08 (H-UI-87) : Ctrl/Cmd+clic sur la seule ligne sélectionnée ⇒ rien
    /// ne change (la cellule active ne passe pas sur la ligne vide) ; sur
    /// une autre ligne ⇒ ajoutée, bloc réduit à la colonne cliquée.
    #[test]
    fn h_ui_87_toggle_last_row_noop() {
        let (model, view) = demo_rows();
        let rows = VisibleRows::new(&model, &view);
        let visible = default_columns();
        let three = uid_at(rows, 3);
        let mut sel = Selection::default();
        let mut cursor = TableCursor::default();
        let op = press(
            RowRef::Item(three),
            Column::Duration,
            PressTarget::Cell,
            PressMode::Replace,
        );
        apply_op(&mut sel, &mut cursor, op, rows, &visible);
        let before = (sel.clone(), cursor);
        let op = press(
            RowRef::Item(three),
            Column::Duration,
            PressTarget::Cell,
            PressMode::Toggle,
        );
        apply_op(&mut sel, &mut cursor, op, rows, &visible);
        assert_eq!((sel.clone(), cursor), before);
        assert_eq!(cursor.row, RowRef::Item(three));
        let six = uid_at(rows, 6);
        let op = press(
            RowRef::Item(six),
            Column::Start,
            PressTarget::Cell,
            PressMode::Toggle,
        );
        apply_op(&mut sel, &mut cursor, op, rows, &visible);
        assert_eq!(sel.click_order(), [three, six]);
        let block = resolve(&sel, &cursor, rows, &visible);
        assert_eq!(block.rows, [DisplayRow(3), DisplayRow(6)]);
        assert_eq!(block.columns, [Column::Start]);
        // La ligne vide n'entre pas dans un bloc de plus d'une ligne.
        let op = press(RowRef::Blank, Column::Name, PressTarget::Cell, PressMode::Toggle);
        assert!(!apply_op(&mut sel, &mut cursor, op, rows, &visible));
        let op = press(RowRef::Blank, Column::Name, PressTarget::Cell, PressMode::Extend);
        assert!(!apply_op(&mut sel, &mut cursor, op, rows, &visible));
        assert_eq!(sel.len(), 2);
    }

    /// H-UI-87, H-UI-89 : des lignes entières couvrent toutes les colonnes
    /// visibles, quelle que soit la colonne active ; la ligne vide seule est
    /// toujours un bloc d'une cellule.
    #[test]
    fn h_ui_87_rows_kind_all_columns() {
        let (model, view) = demo_rows();
        let rows = VisibleRows::new(&model, &view);
        let visible = default_columns();
        let mut sel = Selection::default();
        let mut cursor = TableCursor::default();
        let op = press(
            RowRef::Item(uid_at(rows, 2)),
            Column::Id,
            PressTarget::RowHeader,
            PressMode::Replace,
        );
        apply_op(&mut sel, &mut cursor, op, rows, &visible);
        let op = press(
            RowRef::Item(uid_at(rows, 6)),
            Column::Id,
            PressTarget::RowHeader,
            PressMode::Extend,
        );
        apply_op(&mut sel, &mut cursor, op, rows, &visible);
        let block = resolve(&sel, &cursor, rows, &visible);
        assert_eq!(block.kind, SelectionKind::Rows);
        assert_eq!(block.rows.len(), 5);
        assert_eq!(block.columns, visible);
        assert_eq!(
            Selection::range(uid_at(rows, 6), uid_at(rows, 2), SelectionKind::Rows, rows).len(),
            5
        );
        // En-tête de colonne : la colonne entière, active en première ligne.
        let op = press(
            RowRef::Blank,
            Column::Duration,
            PressTarget::ColumnHeader,
            PressMode::Replace,
        );
        apply_op(&mut sel, &mut cursor, op, rows, &visible);
        let column = resolve(&sel, &cursor, rows, &visible);
        assert_eq!(column.kind, SelectionKind::Cells);
        assert_eq!(column.columns, [Column::Duration]);
        assert_eq!(column.rows.len(), rows.len());
        assert_eq!(column.active, (RowRef::Item(uid_at(rows, 0)), Column::Duration));
        // Ligne vide : une cellule, même avec un type « lignes ».
        let op = press(
            RowRef::Blank,
            Column::Id,
            PressTarget::RowHeader,
            PressMode::Replace,
        );
        apply_op(&mut sel, &mut cursor, op, rows, &visible);
        let blank = resolve(
            &sel.clone().with_kind(SelectionKind::Rows),
            &cursor,
            rows,
            &visible,
        );
        assert_eq!(blank.kind, SelectionKind::Cells);
        assert_eq!(blank.rows, [DisplayRow::at(rows.len())]);
        assert!(blank.is_single());
    }

    /// H-UI-88, H-UI-89 (lot L1) : `DragTo` étend le bloc depuis l'ancre
    /// (lignes et colonnes), garde les lignes entières d'un appui sur l'ID,
    /// borne la ligne vide à la dernière tâche, ne fait rien sans ancre.
    #[test]
    fn h_ui_88_drag_to_extends_block() {
        let (model, view) = demo_rows();
        let rows = VisibleRows::new(&model, &view);
        let visible = default_columns();
        let mut sel = Selection::default();
        let mut cursor = TableCursor::default();
        let two = uid_at(rows, 2);
        let op = press(
            RowRef::Item(two),
            Column::Duration,
            PressTarget::Cell,
            PressMode::Replace,
        );
        assert!(apply_op(&mut sel, &mut cursor, op, rows, &visible));
        let drag = SelectOp::DragTo {
            row: RowRef::Item(uid_at(rows, 5)),
            column: Column::Start,
        };
        assert!(apply_op(&mut sel, &mut cursor, drag, rows, &visible));
        let block = resolve(&sel, &cursor, rows, &visible);
        assert_eq!(block.rows, (2..=5).map(DisplayRow).collect::<Vec<_>>());
        assert_eq!(block.columns, [Column::Duration, Column::Start]);
        assert_eq!(block.active, (RowRef::Item(uid_at(rows, 5)), Column::Start));
        // Retour en arrière : le bloc se resserre.
        let back = SelectOp::DragTo {
            row: RowRef::Item(two),
            column: Column::Duration,
        };
        apply_op(&mut sel, &mut cursor, back, rows, &visible);
        assert!(resolve(&sel, &cursor, rows, &visible).is_single());
        // Ligne vide : la dernière tâche.
        let blank = SelectOp::DragTo {
            row: RowRef::Blank,
            column: Column::Name,
        };
        apply_op(&mut sel, &mut cursor, blank, rows, &visible);
        assert_eq!(
            sel.focus(),
            Some(uid_at(rows, u32::try_from(rows.len() - 1).unwrap()))
        );
        assert_eq!(sel.len(), rows.len() - 2);
        // Appui sur l'ID : lignes entières gardées.
        let op = press(
            RowRef::Item(two),
            Column::Id,
            PressTarget::RowHeader,
            PressMode::Replace,
        );
        apply_op(&mut sel, &mut cursor, op, rows, &visible);
        let drag = SelectOp::DragTo {
            row: RowRef::Item(uid_at(rows, 4)),
            column: Column::Id,
        };
        apply_op(&mut sel, &mut cursor, drag, rows, &visible);
        assert_eq!(sel.kind(), SelectionKind::Rows);
        assert_eq!(sel.len(), 3);
        // Sans ancre (ligne vide appuyée) : rien.
        let mut empty = Selection::default();
        assert!(!apply_op(&mut empty, &mut cursor, drag, rows, &visible));
    }

    /// H-UI-92 (lot L1) : Maj+Espace (lignes entières), Ctrl+Espace
    /// (colonnes entières du bloc, cellule active gardée ; depuis des
    /// lignes entières : toutes les colonnes), Maj+Retour arrière (cellule
    /// active seule).
    #[test]
    fn h_ui_92_entire_rows_columns_collapse() {
        let (model, view) = demo_rows();
        let rows = VisibleRows::new(&model, &view);
        let visible = default_columns();
        let mut sel = Selection::default();
        let mut cursor = TableCursor::default();
        assert!(!apply_op(
            &mut sel,
            &mut cursor,
            SelectOp::EntireRows,
            rows,
            &visible
        ));
        let (three, five) = (uid_at(rows, 3), uid_at(rows, 5));
        let op = press(
            RowRef::Item(three),
            Column::Duration,
            PressTarget::Cell,
            PressMode::Replace,
        );
        apply_op(&mut sel, &mut cursor, op, rows, &visible);
        let op = press(
            RowRef::Item(five),
            Column::Start,
            PressTarget::Cell,
            PressMode::Extend,
        );
        apply_op(&mut sel, &mut cursor, op, rows, &visible);
        assert!(apply_op(
            &mut sel,
            &mut cursor,
            SelectOp::EntireColumns,
            rows,
            &visible
        ));
        let block = resolve(&sel, &cursor, rows, &visible);
        assert_eq!(block.kind, SelectionKind::Cells);
        assert_eq!(block.rows.len(), rows.len());
        assert_eq!(block.columns, [Column::Duration, Column::Start]);
        assert_eq!(block.active, (RowRef::Item(five), Column::Start));
        assert!(apply_op(
            &mut sel,
            &mut cursor,
            SelectOp::Collapse,
            rows,
            &visible
        ));
        let block = resolve(&sel, &cursor, rows, &visible);
        assert!(block.is_single());
        assert_eq!(block.active, (RowRef::Item(five), Column::Start));
        assert!(apply_op(
            &mut sel,
            &mut cursor,
            SelectOp::EntireRows,
            rows,
            &visible
        ));
        let block = resolve(&sel, &cursor, rows, &visible);
        assert_eq!(block.kind, SelectionKind::Rows);
        assert_eq!(block.columns, visible);
        assert!(apply_op(
            &mut sel,
            &mut cursor,
            SelectOp::EntireColumns,
            rows,
            &visible
        ));
        let block = resolve(&sel, &cursor, rows, &visible);
        assert_eq!(block.kind, SelectionKind::Cells);
        assert_eq!(
            block.columns, visible,
            "toutes les colonnes depuis des lignes entières"
        );
        assert_eq!(block.active.0, RowRef::Item(five));
    }

    /// H-UI-93 : une plage de colonnes figée garde le bloc entier quand la
    /// cellule active est au milieu ; ignorée si la cellule active en sort
    /// ou si une borne est masquée.
    #[test]
    fn h_ui_93_column_span_kept() {
        let (model, view) = demo_rows();
        let rows = VisibleRows::new(&model, &view);
        let visible = default_columns();
        let (two, three) = (uid_at(rows, 2), uid_at(rows, 3));
        let sel = Selection::range(two, three, SelectionKind::Cells, rows)
            .moved_within(two, (Column::Duration, Column::Finish));
        assert_eq!(sel.focus(), Some(two));
        assert_eq!(sel.anchor(), Some(two));
        let cursor = TableCursor {
            row: RowRef::Item(two),
            column: Column::Start,
            anchor: Some(two),
            anchor_column: Some(Column::Duration),
        };
        let block = resolve(&sel, &cursor, rows, &visible);
        assert_eq!(block.columns, [Column::Duration, Column::Start, Column::Finish]);
        let outside = TableCursor {
            column: Column::Name,
            ..cursor
        };
        assert_eq!(
            resolve(&sel, &outside, rows, &visible).columns,
            [Column::Name, Column::Duration]
        );
        let hidden: Vec<Column> = visible.iter().copied().filter(|c| *c != Column::Finish).collect();
        assert_eq!(
            resolve(&sel, &cursor, rows, &hidden).columns,
            [Column::Duration, Column::Start]
        );
        // Un clic efface la plage figée.
        let mut clicked = sel;
        clicked.click(three, SelectMode::Toggle, rows);
        assert_eq!(clicked.column_span(), None);
        // Tâche active hors de la sélection : ignorée.
        let other = Selection::single(Some(two)).moved_within(uid_at(rows, 9), (Column::Name, Column::Start));
        assert_eq!(other.focus(), Some(two));
    }

    /// Vue générée sur le modèle de l'exemple : lignes masquées par un
    /// filtre, récapitulatives repliées, tri par une clé tirée.
    fn generated_view(
        model: &RowModel,
        hidden: &[bool],
        fold: &[bool],
        sort: Option<(bool, bool)>,
        keys: &[u32],
    ) -> RowView {
        let rows: Vec<ViewRow> = (0..model.len())
            .filter_map(|i| model.row(i).map(|row| (i, row)))
            .map(|(i, row)| ViewRow {
                uid: row.uid,
                level: row.level,
                summary: row.kind == RowKind::Summary,
                shown: !hidden.get(i).copied().unwrap_or(false),
            })
            .collect();
        let collapsed: BTreeSet<TaskUid> = rows
            .iter()
            .enumerate()
            .filter(|(i, row)| row.summary && fold.get(*i).copied().unwrap_or(false))
            .map(|(_, row)| row.uid)
            .collect();
        let keys: Vec<Option<SortValue>> = (0..rows.len())
            .map(|i| keys.get(i).map(|k| SortValue::Id(*k)))
            .collect();
        RowView::compose(&ViewInput {
            rows: &rows,
            sort: sort.map(|(descending, keep_outline)| SortInput {
                keys: &keys,
                descending,
                keep_outline,
            }),
            outline_level: None,
            collapsed: &collapsed,
            links: &[],
        })
    }

    /// Geste tiré de P60 : appui (cellule, ID, en-tête ; trois modes), tout,
    /// gestes de L1 (sans effet en L0) ; `row` est un rang dans la vue (au-
    /// delà : la ligne vide), `col` un rang dans les colonnes de la table.
    #[derive(Clone, Copy, Debug)]
    enum Gesture {
        Press(usize, usize, u8, u8),
        Drag(usize, usize),
        All,
        Rows,
        Columns,
        Collapse,
        Nav(usize),
    }

    fn gesture_strategy() -> impl Strategy<Value = Gesture> {
        prop_oneof![
            6 => (0_usize..60, 0_usize..16, 0_u8..3, 0_u8..3).prop_map(|(r, c, t, m)| Gesture::Press(r, c, t, m)),
            1 => (0_usize..60, 0_usize..16).prop_map(|(r, c)| Gesture::Drag(r, c)),
            1 => Just(Gesture::All),
            1 => Just(Gesture::Rows),
            1 => Just(Gesture::Columns),
            1 => Just(Gesture::Collapse),
            2 => (0_usize..12).prop_map(Gesture::Nav),
        ]
    }

    /// P60 : invariants du bloc résolu (cellule active dedans ; lignes
    /// affichées, triées, sans doublon ; colonnes = plage contiguë de
    /// colonnes visibles ; ligne vide seulement en bloc d'une cellule ;
    /// `Rows` ⇒ toutes les colonnes visibles).
    fn check_p60(block: &Block, rows: VisibleRows<'_>, visible: &[Column]) -> Result<(), TestCaseError> {
        let blank = DisplayRow::at(rows.len());
        let active_d = match block.active.0 {
            RowRef::Item(uid) => rows.display_of(uid),
            RowRef::Blank => Some(blank),
        };
        let active_d = active_d.ok_or_else(|| TestCaseError::fail("cellule active masquée"))?;
        prop_assert!(block.contains(active_d, block.active.1), "{:?}", block);
        prop_assert!(block.rows.windows(2).all(|w| w[0] < w[1]), "{:?}", block.rows);
        for d in &block.rows {
            prop_assert!(*d == blank || rows.uid(*d).is_some(), "{:?}", d);
        }
        if block.rows.contains(&blank) {
            prop_assert!(block.is_single(), "{:?}", block);
        }
        if !visible.is_empty() {
            let first = visible.iter().position(|c| Some(c) == block.columns.first());
            let first = first.ok_or_else(|| TestCaseError::fail("colonne masquée"))?;
            prop_assert_eq!(
                visible.get(first..first + block.columns.len()),
                Some(block.columns.as_slice())
            );
        }
        if block.kind == SelectionKind::Rows {
            prop_assert_eq!(block.columns.as_slice(), visible);
        }
        Ok(())
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// P60 (ADR-0018, contrat L0) : pour toute vue générée (filtre, tri,
        /// repli, colonnes masquées) et toute suite (≤ 64) de gestes de
        /// sélection et de navigation, aucune panique et le bloc résolu tient
        /// ses invariants après chaque geste.
        #[test]
        fn p60_selection_invariants(
            hidden in proptest::collection::vec(proptest::bool::weighted(0.2), 49),
            fold in proptest::collection::vec(proptest::bool::weighted(0.3), 49),
            sort in proptest::option::of((any::<bool>(), any::<bool>())),
            keys in proptest::collection::vec(0_u32..20, 49),
            shown_cols in proptest::collection::vec(any::<bool>(), 16),
            gestures in proptest::collection::vec(gesture_strategy(), 0..64),
        ) {
            let (model, _) = demo_rows();
            let view = generated_view(&model, &hidden, &fold, sort, &keys);
            let rows = VisibleRows::new(&model, &view);
            let all_columns = default_columns();
            // ID et Nom toujours visibles (comme `UiState`).
            let visible: Vec<Column> = all_columns
                .iter()
                .enumerate()
                .filter(|(i, c)| matches!(c, Column::Id | Column::Name) || shown_cols.get(*i).copied().unwrap_or(true))
                .map(|(_, c)| *c)
                .collect();
            let mut sel = Selection::default();
            let mut cursor = TableCursor::default();
            let row_at = |r: usize| rows.uid(DisplayRow::at(r)).map_or(RowRef::Blank, RowRef::Item);
            let col_at = |c: usize| all_columns.get(c % all_columns.len().max(1)).copied().unwrap_or(Column::Name);
            for gesture in gestures {
                let op = match gesture {
                    Gesture::Press(r, c, t, m) => SelectOp::Press {
                        row: row_at(r),
                        column: col_at(c),
                        target: [PressTarget::Cell, PressTarget::RowHeader, PressTarget::ColumnHeader][usize::from(t % 3)],
                        mode: [PressMode::Replace, PressMode::Extend, PressMode::Toggle][usize::from(m % 3)],
                    },
                    Gesture::Drag(r, c) => SelectOp::DragTo { row: row_at(r), column: col_at(c) },
                    Gesture::All => SelectOp::All,
                    Gesture::Rows => SelectOp::EntireRows,
                    Gesture::Columns => SelectOp::EntireColumns,
                    Gesture::Collapse => SelectOp::Collapse,
                    Gesture::Nav(r) => {
                        // Navigation (L0 : lignes) : cellule simple sur la
                        // ligne d'arrivée, comme `App::navigate`.
                        sel = Selection::single(row_at(r).item());
                        cursor.row = RowRef::from_selection(sel.focus());
                        cursor.anchor_column = None;
                        let block = resolve(&sel, &cursor, rows, &visible);
                        check_p60(&block, rows, &visible)?;
                        continue;
                    }
                };
                apply_op(&mut sel, &mut cursor, op, rows, &visible);
                prop_assert_eq!(cursor.row, RowRef::from_selection(sel.focus()));
                let block = resolve(&sel, &cursor, rows, &visible);
                check_p60(&block, rows, &visible)?;
            }
        }
    }
}
