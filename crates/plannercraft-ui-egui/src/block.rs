//! Bloc de la sélection (ADR-0018, contrat L0, H-UI-87, H-UI-93, H-UI-97) :
//! résolution unique de la sélection de la table en lignes affichées ×
//! colonnes visibles, lue par le dessin, le presse-papiers, le remplissage,
//! l'effacement et les tests.
//!
//! Fonctions pures, sans egui : une [`Selection`] (lignes, type) et le
//! curseur de la table (colonne active, ancre de colonnes) donnent un
//! [`Block`]. Invariants (P60) : la cellule active est dans le bloc ; les
//! lignes sont des lignes affichées, dans l'ordre de la vue ; les colonnes
//! forment une plage contiguë de colonnes visibles ; la ligne vide n'est
//! que dans un bloc d'une seule cellule ; des lignes entières couvrent
//! toutes les colonnes visibles. Aucune indexation directe (règle 10).

use plannercraft_core::TaskUid;

use crate::column::Column;
use crate::nav::{InBlockMove, RowRef};
use crate::row_view::{DisplayRow, VisibleRows};
use crate::selection::{Selection, SelectionKind};
use crate::view::table::TableCursor;

/// Cellules au plus dans un bloc traité par une commande de cellule
/// (copier, coller, remplir, effacer) : au-delà, message, rien écrit
/// (règle 10).
pub const MAX_BLOCK_CELLS: usize = 200_000;

/// Bloc résolu de la sélection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// Type de la sélection (la ligne vide seule est toujours `Cells`).
    pub kind: SelectionKind,
    /// Lignes affichées, dans l'ordre d'affichage, sans doublon ; la ligne
    /// vide de fin vaut `DisplayRow::at(lignes affichées)`.
    pub rows: Vec<DisplayRow>,
    /// Colonnes visibles du bloc, contiguës, dans l'ordre de la vue.
    pub columns: Vec<Column>,
    /// Cellule active (toujours dans le bloc).
    pub active: (RowRef<TaskUid>, Column),
}

/// Résout la sélection `sel` et le curseur `cursor` sur les lignes
/// affichées `rows` et les colonnes visibles `visible` (ordre de la vue).
///
/// - Ligne active : celle du curseur ; une tâche masquée par la vue cède la
///   place à la première ligne sélectionnée affichée, sinon à la ligne vide.
/// - Colonne active : celle du curseur si elle est visible, sinon la
///   première visible (le Nom si aucune ne l'est, jamais atteint).
/// - Lignes : celles de la sélection affichées (ordre de la vue) ; la ligne
///   active si elle n'y est pas (sélection vide, ligne vide) ; la ligne
///   vide est toujours seule, en bloc d'une cellule.
/// - Colonnes : `Rows` ⇒ toutes les visibles ; `Cells` ⇒ la plage figée de
///   la sélection (`Selection::column_span`, H-UI-93) si ses bornes sont
///   visibles et encadrent la colonne active, sinon de l'ancre de colonnes
///   (`anchor_column`, ignorée si masquée) à la colonne active.
#[must_use]
pub fn resolve(sel: &Selection, cursor: &TableCursor, rows: VisibleRows<'_>, visible: &[Column]) -> Block {
    let blank = DisplayRow::at(rows.len());
    let mut selected: Vec<DisplayRow> = sel
        .uids()
        .iter()
        .filter_map(|uid| rows.display_of(*uid))
        .collect();
    selected.sort_unstable();
    selected.dedup();
    let active_row = match cursor.row {
        RowRef::Item(uid) => rows
            .display_of(uid)
            .map(|d| (RowRef::Item(uid), d))
            .or_else(|| {
                selected
                    .first()
                    .and_then(|d| rows.uid(*d).map(|uid| (RowRef::Item(uid), *d)))
            })
            .unwrap_or((RowRef::Blank, blank)),
        RowRef::Blank => (RowRef::Blank, blank),
    };
    let active_column = if visible.contains(&cursor.column) {
        cursor.column
    } else {
        visible.first().copied().unwrap_or(Column::Name)
    };
    let on_blank = active_row.0 == RowRef::Blank;
    let kind = if on_blank {
        SelectionKind::Cells
    } else {
        sel.kind()
    };
    let block_rows = if on_blank {
        vec![blank]
    } else if selected.binary_search(&active_row.1).is_ok() {
        selected
    } else {
        vec![active_row.1]
    };
    let columns = match kind {
        SelectionKind::Rows if !visible.is_empty() => visible.to_vec(),
        SelectionKind::Rows => vec![active_column],
        SelectionKind::Cells => {
            let anchor = cursor
                .anchor_column
                .filter(|c| !on_blank && visible.contains(c))
                .unwrap_or(active_column);
            let mut a = visible.iter().position(|c| *c == anchor);
            let mut b = visible.iter().position(|c| *c == active_column);
            // Plage figée (H-UI-93, Ctrl+Espace) : gardée si ses deux bornes
            // sont visibles et encadrent la colonne active.
            if let Some((from, to)) = sel.column_span().filter(|_| !on_blank) {
                let from = visible.iter().position(|c| *c == from);
                let to = visible.iter().position(|c| *c == to);
                if let (Some(from), Some(to), Some(active)) = (from, to, b)
                    && from.min(to) <= active
                    && active <= from.max(to)
                {
                    (a, b) = (Some(from), Some(to));
                }
            }
            match (a, b) {
                (Some(a), Some(b)) => {
                    let (low, high) = if a <= b { (a, b) } else { (b, a) };
                    visible
                        .get(low..=high)
                        .map_or_else(|| vec![active_column], <[Column]>::to_vec)
                }
                _ => vec![active_column],
            }
        }
    };
    Block {
        kind,
        rows: block_rows,
        columns,
        active: (active_row.0, active_column),
    }
}

impl Block {
    /// Vrai si la cellule (`d`, `c`) est dans le bloc.
    #[must_use]
    pub fn contains(&self, d: DisplayRow, c: Column) -> bool {
        self.rows.binary_search(&d).is_ok() && self.columns.contains(&c)
    }

    /// Nombre de cellules (lignes × colonnes, saturé).
    #[must_use]
    pub fn cells(&self) -> usize {
        self.rows.len().saturating_mul(self.columns.len())
    }

    /// Bloc d'une seule cellule (ou vide, jamais rendu par [`resolve`]).
    #[must_use]
    pub fn is_single(&self) -> bool {
        self.cells() <= 1
    }

    /// Cellule suivante dans le bloc (H-UI-93) : Entrée descend (en bas :
    /// haut de la colonne suivante ; à la fin : première cellule), Tab va à
    /// droite (en bout : début de la ligne suivante ; à la fin : première
    /// cellule), Maj+Entrée et Maj+Tab l'inverse. Une cellule `from` hors
    /// du bloc rend la première cellule du bloc ; un bloc vide rend `from`.
    #[must_use]
    pub fn step(&self, from: (DisplayRow, Column), mv: InBlockMove) -> (DisplayRow, Column) {
        let (Some(&first_row), Some(&first_col)) = (self.rows.first(), self.columns.first()) else {
            return from;
        };
        let (Ok(r), Some(c)) = (
            self.rows.binary_search(&from.0),
            self.columns.iter().position(|column| *column == from.1),
        ) else {
            return (first_row, first_col);
        };
        let last_r = self.rows.len().saturating_sub(1);
        let last_c = self.columns.len().saturating_sub(1);
        let (r, c) = match mv {
            InBlockMove::Down if r < last_r => (r.saturating_add(1), c),
            InBlockMove::Down if c < last_c => (0, c.saturating_add(1)),
            InBlockMove::Up if r > 0 => (r.saturating_sub(1), c),
            InBlockMove::Up if c > 0 => (last_r, c.saturating_sub(1)),
            InBlockMove::Right if c < last_c => (r, c.saturating_add(1)),
            InBlockMove::Right if r < last_r => (r.saturating_add(1), 0),
            InBlockMove::Left if c > 0 => (r, c.saturating_sub(1)),
            InBlockMove::Left if r > 0 => (r.saturating_sub(1), last_c),
            // Fin du bloc : retour à la première cellule ; début : à la
            // dernière.
            InBlockMove::Down | InBlockMove::Right => (0, 0),
            InBlockMove::Up | InBlockMove::Left => (last_r, last_c),
        };
        (
            self.rows.get(r).copied().unwrap_or(first_row),
            self.columns.get(c).copied().unwrap_or(first_col),
        )
    }
}

#[cfg(test)]
mod tests {
    use plannercraft_core::TaskUid;

    use super::Block;
    use crate::column::Column;
    use crate::nav::{InBlockMove, RowRef};
    use crate::row_view::DisplayRow;
    use crate::selection::SelectionKind;

    fn block(rows: &[u32], columns: &[Column]) -> Block {
        Block {
            kind: SelectionKind::Cells,
            rows: rows.iter().map(|r| DisplayRow(*r)).collect(),
            columns: columns.to_vec(),
            active: (RowRef::Item(TaskUid::new(1)), Column::Duration),
        }
    }

    /// H-UI-93 (exemple du cahier) : bloc 2-3 × Durée-Début, active
    /// (2, Durée) : Entrée ⇒ (3, Durée), Entrée ⇒ (2, Début) ; Tab va à
    /// droite puis au début de la ligne suivante ; au bout, retour à la
    /// première cellule ; Maj+Entrée et Maj+Tab font l'inverse.
    #[test]
    fn block_step_in_block() {
        let b = block(&[2, 3], &[Column::Duration, Column::Start]);
        let (d2, d3) = (DisplayRow(2), DisplayRow(3));
        let (dur, start) = (Column::Duration, Column::Start);
        assert_eq!(b.step((d2, dur), InBlockMove::Down), (d3, dur));
        assert_eq!(b.step((d3, dur), InBlockMove::Down), (d2, start));
        assert_eq!(b.step((d3, start), InBlockMove::Down), (d2, dur));
        assert_eq!(b.step((d2, dur), InBlockMove::Up), (d3, start));
        assert_eq!(b.step((d2, start), InBlockMove::Up), (d3, dur));
        assert_eq!(b.step((d2, dur), InBlockMove::Right), (d2, start));
        assert_eq!(b.step((d2, start), InBlockMove::Right), (d3, dur));
        assert_eq!(b.step((d3, start), InBlockMove::Right), (d2, dur));
        assert_eq!(b.step((d2, dur), InBlockMove::Left), (d3, start));
        assert_eq!(b.step((d3, dur), InBlockMove::Left), (d2, start));
        // Lignes non contiguës (Ctrl+clic) : l'ordre du bloc.
        let sparse = block(&[1, 4, 9], &[Column::Name]);
        assert_eq!(
            sparse.step((DisplayRow(4), Column::Name), InBlockMove::Down),
            (DisplayRow(9), Column::Name)
        );
        assert_eq!(
            sparse.step((DisplayRow(9), Column::Name), InBlockMove::Right),
            (DisplayRow(1), Column::Name)
        );
        // Cellule hors du bloc : première cellule ; bloc vide : inchangée.
        assert_eq!(b.step((DisplayRow(7), dur), InBlockMove::Down), (d2, dur));
        assert_eq!(b.step((d2, Column::Name), InBlockMove::Down), (d2, dur));
        let empty = block(&[], &[]);
        assert_eq!(empty.step((d3, start), InBlockMove::Left), (d3, start));
        assert_eq!(empty.cells(), 0);
        assert!(empty.is_single());
        // Une cellule : immobile.
        let one = block(&[5], &[Column::Name]);
        for mv in [
            InBlockMove::Down,
            InBlockMove::Up,
            InBlockMove::Right,
            InBlockMove::Left,
        ] {
            assert_eq!(
                one.step((DisplayRow(5), Column::Name), mv),
                (DisplayRow(5), Column::Name)
            );
        }
    }
}
