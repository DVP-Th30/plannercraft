//! Commandes de cellule sur le bloc de la sélection (ADR-0018 D3 à D5 ;
//! R-257 coller, R-258 remplir vers le bas et à droite, R-259 effacer ;
//! H-UI-95 à H-UI-98 ; seconde passe UX, ADR-0019 : R-262 source répétée
//! sur une destination multiple, valeurs hors de la table comptées à part,
//! Ctrl/Cmd+R, H-UI-120).
//!
//! Fonctions pures sur le [`Block`] résolu (`block::resolve`) : chaque
//! cellule écrite passe par la lecture de la saisie
//! ([`edit::commit_edit`], jamais une lecture dupliquée) puis le registre,
//! opérations concaténées dans **un** lot (une entrée d'annulation) ;
//! copier écrit les textes de saisie du moteur ([`edit::initial_text_in`])
//! ou le texte affiché, jamais une date calculée ici (règle 3). Une cellule
//! que le registre n'écrirait pas sur sa ligne ([`edit::writable`]) est
//! sautée et comptée ; une lecture refusée refuse tout, rien n'est écrit.
//! Aucune indexation directe, blocs et textes bornés (règle 10).
//!
//! `App::cells_op` (réducteur, `Action::Cells`) : Remplir vers le bas,
//! Effacer le contenu, Ctrl/Cmd+Entrée et Suppr selon la cible
//! ([`delete_target`]), compte rendu au bandeau.

use std::collections::BTreeSet;

use plannercraft_automation::clip::{MAX_PASTE_BYTES, MAX_PASTE_LINES};
use plannercraft_automation::command::TaskUpdate;
use plannercraft_automation::text::{Language as TextLanguage, TextContext};
use plannercraft_automation::{Batch, Command, CommandError, Patch, Session, TaskRef};
use plannercraft_core::{Severity, TaskUid};

use crate::action::{Action, CellOp};
use crate::app::OpenDocument;
use crate::block::{Block, MAX_BLOCK_CELLS};
use crate::column::Column;
use crate::edit::{self, CellClear, EditError, EditState, commit_edit, editable, writable};
use crate::i18n::{Arg, TextId};
use crate::row_view::{DisplayRow, VisibleRows};
use crate::rows::Row;
use crate::state::ViewKind;
use crate::structure::StructureOp;
use crate::view::Pane;

use crate::selection::SelectionKind;

/// Lot préparé par une commande de cellule.
#[derive(Clone, Debug, PartialEq)]
pub struct CellPlan {
    /// Lot unique (une entrée d'annulation).
    pub batch: Batch,
    /// Cellules écrites.
    pub written: usize,
    /// Cellules sautées (non modifiables), comptées dans le compte rendu.
    pub skipped: usize,
    /// Valeurs collées au-delà de la dernière colonne visible, comptées à
    /// part (« 2 valeurs hors de la table n'ont pas été collées »).
    pub outside: usize,
    /// Zone écrite par un collage de plusieurs cellules (sélectionnée
    /// après le collage, H-UI-106) ; `None` : sélection gardée.
    pub area: Option<PasteArea>,
}

/// Zone de cellules écrite par un collage : lignes affichées à partir de
/// `top` (les tâches ajoutées en fin comprises), colonnes visibles de
/// `first` à `last`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PasteArea {
    /// Première ligne affichée.
    pub top: DisplayRow,
    /// Nombre de lignes (au moins 1).
    pub rows: usize,
    /// Première colonne.
    pub first: Column,
    /// Dernière colonne.
    pub last: Column,
}

/// Refus d'une commande de cellule : rien n'est écrit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellError {
    /// Commande pas encore servie (squelette du contrat L0).
    NotAvailable,
    /// Bloc de plus de [`crate::block::MAX_BLOCK_CELLS`] cellules, ou texte
    /// collé au-delà des bornes de R-256.
    TooLarge,
    /// Texte illisible pour une cellule (première cellule fautive).
    Parse {
        /// ID de la ligne.
        id: u32,
        /// Colonne.
        column: Column,
        /// Texte refusé, tel que collé (nommé dans le message, H-UI-105).
        text: String,
        /// Détail de la lecture refusée (« Détails » repliable).
        detail: String,
    },
    /// Lignes au-delà de la dernière tâche sous un tri ou un filtre.
    PastEnd,
    /// Remplir vers le bas sur une ligne sans ligne affichée au-dessus.
    NothingAbove,
}

impl CellError {
    /// Message du refus.
    #[must_use]
    pub const fn text_id(&self) -> TextId {
        match self {
            Self::NotAvailable => TextId::UxNotAvailable,
            Self::TooLarge => TextId::CellsTooLarge,
            Self::Parse { .. } => TextId::CellsParse,
            Self::PastEnd => TextId::CellsPastEnd,
            Self::NothingAbove => TextId::CellsNothingAbove,
        }
    }
}

/// Compte rendu au bandeau d'une commande de cellule acceptée (L2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellNotice {
    /// Cellules non modifiables sautées.
    Skipped(usize),
    /// Cellules qui ne peuvent pas être vides (R-259).
    NotClearable(usize),
    /// Tâches supprimées par Suppr (H-UI-97), annulables.
    TasksDeleted(usize),
}

impl CellNotice {
    /// Message du compte rendu (argument `count`).
    #[must_use]
    pub const fn text_id(self) -> TextId {
        match self {
            Self::Skipped(_) => TextId::CellsSkipped,
            Self::NotClearable(_) => TextId::CellsNotClearable,
            Self::TasksDeleted(_) => TextId::TasksDeleted,
        }
    }

    /// Nombre compté.
    #[must_use]
    pub const fn count(self) -> usize {
        match self {
            Self::Skipped(n) | Self::NotClearable(n) | Self::TasksDeleted(n) => n,
        }
    }
}

/// Libellés du groupe Édition du ruban (Remplir ▾ Vers le bas, Effacer ▾
/// Contenu, ADR-0018 D10) et leurs info-bulles, posés en L0 et placés dans
/// la table `RIBBON` par le lot L5.
pub const RIBBON_EDITING_TEXTS: [TextId; 6] = [
    TextId::RibbonFill,
    TextId::RibbonFillDown,
    TextId::RibbonClear,
    TextId::RibbonClearContents,
    TextId::RibbonTipFill,
    TextId::RibbonTipClear,
];

impl CellOp {
    /// Libellé du jumeau de menu (menu Édition et menus contextuels,
    /// H-UI-100) ; `None` pour les gestes sans entrée de menu.
    #[must_use]
    pub const fn menu_label(&self) -> Option<TextId> {
        match self {
            Self::FillDown => Some(TextId::MenuFillDown),
            Self::FillRight => Some(TextId::MenuFillRight),
            Self::Clear => Some(TextId::MenuClearContents),
            Self::DeleteKey | Self::FillTyped(_) => None,
        }
    }
}

/// Suppr selon la cible (H-UI-97, ADR-0018 D5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeleteTarget {
    /// Supprimer les tâches des lignes de la sélection.
    Tasks,
    /// Effacer le contenu du bloc (R-259).
    Contents,
}

/// Cible de Suppr : lignes entières, ou bloc d'**une** cellule dans l'ID
/// ou le Nom ⇒ supprimer les tâches ; tout autre bloc (plusieurs cellules,
/// colonne entière du Nom comprise, quelle que soit la cellule active) ⇒
/// effacer son contenu (H-UI-97, relecture finale n° 3).
#[must_use]
pub fn delete_target(block: &Block) -> DeleteTarget {
    if block.kind == SelectionKind::Rows
        || (block.is_single() && matches!(block.active.1, Column::Id | Column::Name))
    {
        DeleteTarget::Tasks
    } else {
        DeleteTarget::Contents
    }
}

/// Libellé d'historique d'une commande de cellule.
const fn history_label(op: PlanKind, lang: TextLanguage) -> &'static str {
    match (op, lang) {
        (PlanKind::Paste, TextLanguage::Fr) => "Coller",
        (PlanKind::Paste, TextLanguage::En) => "Paste",
        (PlanKind::Fill, TextLanguage::Fr) => "Remplir",
        (PlanKind::Fill, TextLanguage::En) => "Fill",
        (PlanKind::Clear, TextLanguage::Fr) => "Effacer le contenu",
        (PlanKind::Clear, TextLanguage::En) => "Clear contents",
    }
}

/// Nature d'un lot de cellules (libellé d'historique).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlanKind {
    /// Coller (R-257).
    Paste,
    /// Remplir vers le bas, Ctrl/Cmd+Entrée (R-258).
    Fill,
    /// Effacer le contenu (R-259).
    Clear,
}

/// Lot en construction : opérations, cellules écrites et sautées.
struct Builder {
    /// Opérations concaténées.
    ops: Vec<Command>,
    /// Cellules qui ont produit au moins une opération.
    written: usize,
    /// Cellules sautées et comptées.
    skipped: usize,
    /// Valeurs au-delà de la dernière colonne visible.
    outside: usize,
    /// Tâches créées en fin de projet (étiquettes `$cell1`, `$cell2`…).
    created: usize,
    /// Zone écrite (collage de plusieurs cellules).
    area: Option<PasteArea>,
    /// Tâches dont une date (Début ou Fin) a déjà été écrite par ce lot :
    /// la date suivante remplace sa contrainte (R-158), jamais un
    /// non-changement (R-260 d).
    dated: BTreeSet<TaskUid>,
}

impl Builder {
    const fn new() -> Self {
        Self {
            ops: Vec::new(),
            written: 0,
            skipped: 0,
            outside: 0,
            created: 0,
            area: None,
            dated: BTreeSet::new(),
        }
    }

    /// Ajoute les opérations d'une cellule.
    fn push(&mut self, ops: Vec<Command>) {
        if !ops.is_empty() {
            self.written = self.written.saturating_add(1);
            self.ops.extend(ops);
        }
    }

    fn skip(&mut self) {
        self.skipped = self.skipped.saturating_add(1);
    }

    /// Plan rendu (lot unique, même vide).
    fn finish(self, kind: PlanKind, lang: TextLanguage) -> CellPlan {
        CellPlan {
            batch: Batch {
                format_version: Batch::FORMAT_VERSION,
                lang: Some(lang),
                ops: self.ops,
                label: Some(history_label(kind, lang).to_owned()),
            },
            written: self.written,
            skipped: self.skipped,
            outside: self.outside,
            area: self.area,
        }
    }

    /// Plan rendu, `None` s'il n'y a ni opération ni cellule sautée.
    fn finish_opt(self, kind: PlanKind, lang: TextLanguage) -> Option<CellPlan> {
        (!self.ops.is_empty() || self.skipped > 0 || self.outside > 0).then(|| self.finish(kind, lang))
    }
}

/// Détail d'une lecture refusée (dans la langue du lot).
fn parse_detail(error: &EditError, lang: TextLanguage) -> String {
    match error {
        EditError::Text(e) => CommandError::text(e.clone()).message(lang),
        EditError::UnknownId(id) => match lang {
            TextLanguage::Fr => format!("aucune tâche d'ID {id}"),
            TextLanguage::En => format!("no task with ID {id}"),
        },
        EditError::BadChoice(choices) => match lang {
            TextLanguage::Fr => format!("valeurs admises : {choices}"),
            TextLanguage::En => format!("accepted values: {choices}"),
        },
        EditError::NotEditable | EditError::NotClearable | EditError::Computed => String::new(),
    }
}

/// Écrit `text` dans la cellule (`row`, `column`) d'une tâche existante par
/// la lecture de la saisie ; cellule non modifiable (ou qui ne peut pas
/// être vide) : sautée.
fn write_existing(
    b: &mut Builder,
    row: &Row,
    column: Column,
    text: &str,
    s: &Session,
    lang: TextLanguage,
) -> Result<(), CellError> {
    if !writable(row, column) {
        b.skip();
        return Ok(());
    }
    let date = matches!(column, Column::Start | Column::Finish);
    let state = EditState::typed(Some(row.uid), column, text);
    let result = if date && b.dated.contains(&row.uid) {
        edit::commit_edit_dates_forced(&state, s, lang)
    } else {
        commit_edit(&state, s, lang)
    };
    match result {
        Ok(batch) => {
            if date && !batch.ops.is_empty() {
                b.dated.insert(row.uid);
            }
            b.push(batch.ops);
        }
        Err(EditError::NotEditable | EditError::NotClearable | EditError::Computed) => b.skip(),
        Err(error) => {
            return Err(CellError::Parse {
                id: row.id,
                column,
                text: text.to_owned(),
                detail: parse_detail(&error, lang),
            });
        }
    }
    Ok(())
}

/// Écrit une ligne au-delà de la dernière tâche (R-257) ou la ligne vide
/// (Ctrl/Cmd+Entrée) : chaque cellule lue comme une saisie sur la ligne
/// vide ([`commit_edit`] sans tâche) ; la première crée la tâche (étiquette
/// propre au lot), les suivantes la modifient par son étiquette. `id` :
/// ID que la tâche aurait (message d'erreur).
fn write_new_row(
    b: &mut Builder,
    cells: &[(Column, &str)],
    id: u32,
    s: &Session,
    lang: TextLanguage,
) -> Result<(), CellError> {
    let label = format!("$cell{}", b.created.saturating_add(1));
    let mut created = false;
    for (column, text) in cells {
        let column = *column;
        // Le % d'une tâche se saisit une fois créée (refus de la ligne vide).
        if !editable(column) || column == Column::PercentComplete {
            b.skip();
            continue;
        }
        let batch = commit_edit(&EditState::typed(None, column, text), s, lang).map_err(|error| {
            CellError::Parse {
                id,
                column,
                text: (*text).to_owned(),
                detail: parse_detail(&error, lang),
            }
        })?;
        let mut ops = Vec::with_capacity(batch.ops.len());
        for op in batch.ops {
            match op {
                Command::TaskAdd(mut add) if !created => {
                    add.label = Some(label.clone());
                    created = true;
                    ops.push(Command::TaskAdd(add));
                }
                Command::TaskAdd(add) => {
                    // Champs portés par la création : modification de la
                    // tâche déjà créée par ce lot.
                    let mut update = empty_update(TaskRef::Label(label.clone()));
                    if column == Column::Name {
                        update.name = Patch::Set(add.name);
                    }
                    if let Some(d) = add.duration {
                        update.duration = Patch::Set(d);
                    }
                    if let Some(start) = add.start {
                        update.start = Patch::Set(start);
                    }
                    if let Some(finish) = add.finish {
                        update.finish = Patch::Set(finish);
                    }
                    ops.push(Command::TaskUpdate(update));
                }
                Command::LinkAdd(mut spec) => {
                    if matches!(spec.succ, TaskRef::Label(_)) {
                        spec.succ = TaskRef::Label(label.clone());
                    }
                    ops.push(Command::LinkAdd(spec));
                }
                other => ops.push(other),
            }
        }
        b.push(ops);
    }
    if created {
        b.created = b.created.saturating_add(1);
    }
    Ok(())
}

/// `task.update` vide sur `uid`.
const fn empty_update(uid: TaskRef) -> TaskUpdate {
    TaskUpdate {
        uid,
        name: Patch::Keep,
        duration: Patch::Keep,
        milestone: Patch::Keep,
        notes: Patch::Keep,
        start: Patch::Keep,
        finish: Patch::Keep,
        active: Patch::Keep,
        task_type: Patch::Keep,
        effort_driven: Patch::Keep,
        work: Patch::Keep,
        mode: Patch::Keep,
        custom: std::collections::BTreeMap::new(),
    }
}

/// Bloc trop grand (règle 10).
fn check_size(block: &Block) -> Result<(), CellError> {
    if block.cells() > MAX_BLOCK_CELLS {
        Err(CellError::TooLarge)
    } else {
        Ok(())
    }
}

/// Grille d'un texte collé (R-257) : bornes de R-256, lignes `\n` ou
/// `\r\n` (ligne vide finale ignorée), cellules par tabulation.
///
/// # Errors
/// [`CellError::TooLarge`] : plus de 4 Mio, de 10 000 lignes ou de
/// [`MAX_BLOCK_CELLS`] cellules.
pub fn grid(text: &str) -> Result<Vec<Vec<&str>>, CellError> {
    if text.len() > MAX_PASTE_BYTES {
        return Err(CellError::TooLarge);
    }
    let body = text.strip_suffix('\n').unwrap_or(text);
    let body = body.strip_suffix('\r').unwrap_or(body);
    let mut rows = Vec::new();
    let mut cells = 0_usize;
    for line in body.split('\n') {
        if rows.len() >= MAX_PASTE_LINES {
            return Err(CellError::TooLarge);
        }
        let line = line.strip_suffix('\r').unwrap_or(line);
        let row: Vec<&str> = line.split('\t').collect();
        cells = cells.saturating_add(row.len());
        if cells > MAX_BLOCK_CELLS {
            return Err(CellError::TooLarge);
        }
        rows.push(row);
    }
    Ok(rows)
}

/// Texte d'une cellule copiée : sans tabulation ni fin de ligne (elles
/// casseraient la grille).
fn clean_cell(text: &str) -> String {
    if text.contains(['\t', '\n', '\r']) {
        text.replace(['\t', '\n', '\r'], " ")
    } else {
        text.to_owned()
    }
}

/// Texte tabulé des cellules du bloc (H-UI-95) : texte de saisie
/// ([`edit::initial_text_in`]) pour une colonne modifiable, texte affiché
/// sinon ; une ligne par ligne du bloc (ordre d'affichage), cellules
/// séparées par une tabulation, sans fin de ligne finale ; la ligne vide
/// donne des cellules vides.
#[must_use]
pub fn copy_text(block: &Block, doc: &OpenDocument, rows: VisibleRows<'_>, lang: TextLanguage) -> String {
    if block.cells() > MAX_BLOCK_CELLS {
        return String::new();
    }
    let project = doc.project();
    let ctx = TextContext::for_project(project, lang);
    let mut out = String::new();
    for (i, d) in block.rows.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let model = rows.model_row(*d);
        for (j, column) in block.columns.iter().enumerate() {
            if j > 0 {
                out.push('\t');
            }
            let Some(m) = model else {
                continue;
            };
            let text = if editable(*column) {
                edit::initial_text_in(project, rows.model(), m.index(), *column, &ctx)
            } else {
                rows.text(*d, *column).to_owned()
            };
            out.push_str(&clean_cell(&text));
        }
    }
    out
}

/// Coller un texte dans les cellules du bloc (R-257, R-262).
///
/// `visible` : colonnes visibles, dans l'ordre de la vue (la grille déborde
/// du bloc vers la droite sur ces colonnes). Texte d'une seule cellule :
/// recopié dans tout le bloc ; grille `h × l` sur un bloc `H × L` avec
/// `H % h == 0` et `L % l == 0` (et plus grand) : répétée pour couvrir le
/// bloc (R-262) ; sinon posée une fois à partir du coin haut-gauche, sur
/// les lignes affichées suivantes et les colonnes visibles suivantes.
/// Valeurs au-delà de la dernière colonne visible : comptées à part
/// (`outside`) ; cellules non modifiables : sautées et comptées ; lignes
/// au-delà de la dernière tâche : tâches ajoutées en fin (refusé sous tri,
/// filtre ou repli). La zone écrite (`area`) est rendue pour être
/// sélectionnée (H-UI-106).
///
/// # Errors
/// [`CellError`] : rien n'est écrit.
#[allow(
    clippy::too_many_lines,
    reason = "suite d'étapes d'un collage, en-têtes de groupe sautés compris"
)]
pub fn paste_plan(
    block: &Block,
    visible: &[Column],
    text: &str,
    s: &Session,
    rows: VisibleRows<'_>,
    lang: TextLanguage,
) -> Result<CellPlan, CellError> {
    check_size(block)?;
    let grid = grid(text)?;
    let mut b = Builder::new();
    let single = match grid.as_slice() {
        [row] => match row.as_slice() {
            [value] => Some(*value),
            _ => None,
        },
        _ => None,
    };
    let (Some(top), Some(first_col), Some(last_col)) =
        (block.rows.first(), block.columns.first(), block.columns.last())
    else {
        return Ok(b.finish(PlanKind::Paste, lang));
    };
    if let Some(value) = single {
        fill_cells(&mut b, block, value, s, rows, lang)?;
        // Bloc contigu sur des tâches : il reste (ou devient, depuis des
        // lignes entières) la sélection.
        let contiguous = block
            .rows
            .windows(2)
            .all(|w| matches!(w, [a, z] if a.index().checked_add(1) == Some(z.index())));
        if contiguous && block.active.0 != crate::nav::RowRef::Blank {
            b.area = Some(PasteArea {
                top: *top,
                rows: block.rows.len(),
                first: *first_col,
                last: *last_col,
            });
        }
        return Ok(b.finish(PlanKind::Paste, lang));
    }
    if tiles(&grid, block) {
        // R-262 : source répétée sur les lignes et les colonnes du bloc.
        for (i, d) in block.rows.iter().enumerate() {
            let Some(line) = i.checked_rem(grid.len()).and_then(|k| grid.get(k)) else {
                continue;
            };
            let Some(row) = rows.row(*d) else {
                continue;
            };
            for (j, column) in block.columns.iter().enumerate() {
                let Some(text) = j.checked_rem(line.len()).and_then(|k| line.get(k)) else {
                    continue;
                };
                write_existing(&mut b, row, *column, text, s, lang)?;
            }
        }
        b.area = Some(PasteArea {
            top: *top,
            rows: block.rows.len(),
            first: *first_col,
            last: *last_col,
        });
        return Ok(b.finish(PlanKind::Paste, lang));
    }
    let start = visible.iter().position(|c| c == first_col).unwrap_or(0);
    let len = rows.len();
    let mut next_id = u32::try_from(s.project().tasks.len())
        .unwrap_or(u32::MAX)
        .saturating_add(1);
    let mut widest = 0_usize;
    // Vue regroupée (finitions P3) : les en-têtes de groupe sont sautés
    // sans consommer de ligne collée.
    let mut next_row = top.index();
    for line in &grid {
        while next_row < len && rows.header(DisplayRow::at(next_row)).is_some() {
            next_row = next_row.saturating_add(1);
        }
        let d = next_row;
        next_row = next_row.saturating_add(1);
        let cells: Vec<(Column, &str)> = line
            .iter()
            .enumerate()
            .filter_map(|(j, text)| visible.get(start.saturating_add(j)).map(|c| (*c, *text)))
            .collect();
        widest = widest.max(cells.len());
        b.outside = b.outside.saturating_add(line.len().saturating_sub(cells.len()));
        if d < len {
            let Some(row) = rows.row(DisplayRow::at(d)) else {
                continue;
            };
            for (column, text) in cells {
                write_existing(&mut b, row, column, text, s, lang)?;
            }
        } else {
            if cells.iter().all(|(_, t)| t.trim().is_empty()) {
                continue;
            }
            if !rows.view().is_identity() {
                return Err(CellError::PastEnd);
            }
            write_new_row(&mut b, &cells, next_id, s, lang)?;
            next_id = next_id.saturating_add(1);
        }
    }
    let last = visible
        .get(start.saturating_add(widest.saturating_sub(1)))
        .copied()
        .unwrap_or(*first_col);
    b.area = Some(PasteArea {
        top: *top,
        rows: grid.len().max(1),
        first: *first_col,
        last,
    });
    Ok(b.finish(PlanKind::Paste, lang))
}

/// R-262 : la grille (rectangulaire, `h × l`) se répète sur le bloc
/// `H × L` (lignes de la sélection, ligne vide exclue) si `H % h == 0`,
/// `L % l == 0` et le bloc est plus grand que la grille.
fn tiles(grid: &[Vec<&str>], block: &Block) -> bool {
    let h = grid.len();
    let Some(l) = grid.first().map(Vec::len) else {
        return false;
    };
    let (big_h, big_l) = (block.rows.len(), block.columns.len());
    grid.iter().all(|line| line.len() == l)
        && big_h.checked_rem(h) == Some(0)
        && big_l.checked_rem(l) == Some(0)
        && (big_h > h || big_l > l)
        && block.active.0 != crate::nav::RowRef::Blank
}

/// Le même texte dans toutes les cellules du bloc (ligne vide : saisie qui
/// crée la tâche).
fn fill_cells(
    b: &mut Builder,
    block: &Block,
    text: &str,
    s: &Session,
    rows: VisibleRows<'_>,
    lang: TextLanguage,
) -> Result<(), CellError> {
    for d in &block.rows {
        if let Some(row) = rows.row(*d) {
            for column in &block.columns {
                write_existing(b, row, *column, text, s, lang)?;
            }
        } else {
            let cells: Vec<(Column, &str)> = block.columns.iter().map(|c| (*c, text)).collect();
            let id = u32::try_from(s.project().tasks.len())
                .unwrap_or(u32::MAX)
                .saturating_add(1);
            write_new_row(b, &cells, id, s, lang)?;
        }
    }
    Ok(())
}

/// Remplir vers le bas (R-258) ; `Ok(None)` : rien à écrire (ligne vide).
///
/// Pour chaque colonne du bloc, texte de saisie de la première ligne du
/// bloc recopié dans les lignes suivantes ; bloc d'une seule ligne : source
/// = ligne affichée au-dessus.
///
/// # Errors
/// [`CellError`] : rien n'est écrit ([`CellError::NothingAbove`] : bloc
/// d'une ligne sur la première ligne affichée).
pub fn fill_down_plan(
    block: &Block,
    s: &Session,
    rows: VisibleRows<'_>,
    lang: TextLanguage,
) -> Result<Option<CellPlan>, CellError> {
    check_size(block)?;
    let Some(first) = block.rows.first() else {
        return Ok(None);
    };
    let (source, targets): (DisplayRow, &[DisplayRow]) = if block.rows.len() > 1 {
        (*first, block.rows.get(1..).unwrap_or(&[]))
    } else {
        if rows.row(*first).is_none() {
            // Ligne vide de fin : rien (aucune tâche à remplir).
            return Ok(None);
        }
        let Some(above) = first.index().checked_sub(1) else {
            return Err(CellError::NothingAbove);
        };
        (DisplayRow::at(above), block.rows.as_slice())
    };
    let Some(model) = rows.model_row(source) else {
        return Ok(None);
    };
    let project = s.project();
    let ctx = TextContext::for_project(project, lang);
    let texts: Vec<(Column, String)> = block
        .columns
        .iter()
        .map(|c| {
            (
                *c,
                edit::initial_text_in(project, rows.model(), model.index(), *c, &ctx),
            )
        })
        .collect();
    let mut b = Builder::new();
    for d in targets {
        let Some(row) = rows.row(*d) else {
            continue;
        };
        for (column, text) in &texts {
            write_existing(&mut b, row, *column, text, s, lang)?;
        }
    }
    Ok(b.finish_opt(PlanKind::Fill, lang))
}

/// Remplir à droite (Ctrl/Cmd+R, R-258 amendée) ; `Ok(None)` : rien à
/// écrire.
///
/// Pour chaque ligne du bloc, texte de la première colonne du bloc
/// (texte de saisie si elle est modifiable, texte affiché sinon) recopié
/// dans les colonnes suivantes ; bloc d'une colonne : source = colonne
/// visible à sa gauche. L'ID n'est jamais une source (un numéro de ligne
/// recopié en durée ou en nom n'a pas de sens ; hypothèse) : bloc qui
/// commence à l'ID ⇒ source = colonne suivante ; colonne seule juste à
/// droite de l'ID ⇒ rien. Mêmes cellules sautées et comptes que Remplir
/// vers le bas ; ligne vide : rien.
///
/// # Errors
/// [`CellError`] : rien n'est écrit (lecture refusée nommée).
pub fn fill_right_plan(
    block: &Block,
    visible: &[Column],
    s: &Session,
    rows: VisibleRows<'_>,
    lang: TextLanguage,
) -> Result<Option<CellPlan>, CellError> {
    check_size(block)?;
    let columns: &[Column] = match block.columns.first() {
        Some(Column::Id) => block.columns.get(1..).unwrap_or(&[]),
        _ => &block.columns,
    };
    let (source, targets): (Column, Vec<Column>) = match columns {
        [] => return Ok(None),
        [only] => {
            let Some(at) = visible.iter().position(|c| c == only) else {
                return Ok(None);
            };
            match at.checked_sub(1).and_then(|i| visible.get(i)) {
                Some(Column::Id) | None => return Ok(None),
                Some(left) => (*left, vec![*only]),
            }
        }
        [first, rest @ ..] => (*first, rest.to_vec()),
    };
    let project = s.project();
    let ctx = TextContext::for_project(project, lang);
    let mut b = Builder::new();
    for d in &block.rows {
        let (Some(row), Some(model)) = (rows.row(*d), rows.model_row(*d)) else {
            continue;
        };
        let text = if editable(source) {
            edit::initial_text_in(project, rows.model(), model.index(), source, &ctx)
        } else {
            rows.text(*d, source).to_owned()
        };
        for column in &targets {
            write_existing(&mut b, row, *column, &text, s, lang)?;
        }
    }
    Ok(b.finish_opt(PlanKind::Fill, lang))
}

/// Ctrl/Cmd+Entrée : le texte saisi dans toutes les cellules modifiables du
/// bloc (H-UI-98, R-258), même lecture que la saisie.
///
/// # Errors
/// [`CellError`] : rien n'est écrit.
pub fn fill_with(
    block: &Block,
    text: &str,
    s: &Session,
    rows: VisibleRows<'_>,
    lang: TextLanguage,
) -> Result<CellPlan, CellError> {
    check_size(block)?;
    let mut b = Builder::new();
    fill_cells(&mut b, block, text, s, rows, lang)?;
    Ok(b.finish(PlanKind::Fill, lang))
}

/// Effacer le contenu du bloc (R-259) ; `Ok(None)` : rien à effacer.
/// Cellules qui ne peuvent pas être vides : comptées (`skipped`) ; une
/// contrainte remise par défaut une fois par tâche (Début et Fin).
///
/// # Errors
/// [`CellError::TooLarge`] : rien n'est écrit.
pub fn clear_plan(
    block: &Block,
    s: &Session,
    rows: VisibleRows<'_>,
    lang: TextLanguage,
) -> Result<Option<CellPlan>, CellError> {
    check_size(block)?;
    let project = s.project();
    let ctx = TextContext::for_project(project, lang);
    let mut b = Builder::new();
    let mut constraint_reset: BTreeSet<TaskUid> = BTreeSet::new();
    for d in &block.rows {
        let Some(task) = rows.uid(*d).and_then(|uid| project.tasks.get(uid)) else {
            continue;
        };
        for column in &block.columns {
            let date = matches!(column, Column::Start | Column::Finish);
            match edit::clear_cell(task, project, *column, &ctx) {
                CellClear::Ops(ops) => {
                    if date && !constraint_reset.insert(task.uid()) {
                        continue;
                    }
                    b.push(ops);
                }
                CellClear::Nothing => {}
                CellClear::NotClearable => b.skip(),
            }
        }
    }
    Ok(b.finish_opt(PlanKind::Clear, lang))
}

/// Argument de compte d'un message.
fn count_arg(n: usize) -> Arg<'static> {
    Arg::Count(i64::try_from(n).unwrap_or(i64::MAX))
}

/// Colonne de départ d'un collage de cellules sur des lignes entières
/// (R-261, relecture finale ux2, C7) : la colonne active, sauf l'ID (jamais
/// modifiable) ; alors le Nom s'il est affiché, sinon la première colonne
/// affichée après l'ID.
#[must_use]
pub fn rows_paste_column(active: Column, visible: &[Column]) -> Column {
    if active != Column::Id {
        return active;
    }
    if visible.contains(&Column::Name) {
        return Column::Name;
    }
    visible
        .iter()
        .copied()
        .find(|c| *c != Column::Id)
        .unwrap_or(active)
}

impl crate::App {
    /// Bloc de la sélection de la table (lignes affichées × colonnes
    /// visibles), `None` sans document.
    #[must_use]
    pub fn table_block(&self) -> Option<Block> {
        let rows = self.visible_rows()?;
        let visible = self.visible_column_list();
        Some(crate::block::resolve(
            &self.selected,
            &self.view.cursor,
            rows,
            &visible,
        ))
    }

    /// Colonnes visibles de la table, dans l'ordre de la vue.
    pub(crate) fn visible_column_list(&self) -> Vec<Column> {
        self.state.visible_columns().map(|state| state.column).collect()
    }

    /// Commande de cellule (`Action::Cells`). Hors de la vue Gantt, Suppr
    /// garde son chemin (`Action::Structure(Delete)`, feuille des
    /// ressources) ; le reste n'y arrive pas (gardé par le réducteur).
    pub(crate) fn cells_op(&mut self, op: CellOp, ctx: &egui::Context) {
        if self.state.view != ViewKind::Gantt {
            if op == CellOp::DeleteKey {
                self.apply(Action::Structure(StructureOp::Delete), ctx);
            }
            return;
        }
        if self.document.is_none() {
            return;
        }
        self.sync_selection();
        match op {
            CellOp::DeleteKey => match self.table_block().map(|block| delete_target(&block)) {
                Some(DeleteTarget::Contents) => self.clear_cells(),
                Some(DeleteTarget::Tasks) | None => self.delete_tasks_with_notice(ctx),
            },
            CellOp::Clear => self.clear_cells(),
            CellOp::FillDown => self.fill_down(),
            CellOp::FillTyped(text) => self.fill_typed(&text, ctx),
            CellOp::FillRight => self.fill_right(),
        }
    }

    /// Suppr sur des lignes entières, ou une seule cellule de l'ID ou du
    /// Nom : supprime les tâches
    /// (confirmation des récapitulatives inchangée), puis le message
    /// « n tâche(s) supprimée(s) — Ctrl/Cmd+Z annule » (H-UI-97).
    fn delete_tasks_with_notice(&mut self, ctx: &egui::Context) {
        let count = |app: &Self| app.document.as_ref().map_or(0, |d| d.project().tasks.len());
        let before = count(self);
        self.apply(Action::Structure(StructureOp::Delete), ctx);
        let removed = before.saturating_sub(count(self));
        if removed == 0 {
            return;
        }
        let undo = crate::keymap::shortcut_for(crate::keymap::Binding::Command("history.undo"))
            .map(|shortcut| crate::keymap::shortcut_text(ctx, shortcut))
            .unwrap_or_default();
        let text = self.texts.get_args(
            CellNotice::TasksDeleted(removed).text_id(),
            &[("count", count_arg(removed)), ("undo", Arg::Str(undo.into()))],
        );
        self.push_message(Severity::Info, text);
    }

    /// Effacer le contenu du bloc (R-259).
    pub(crate) fn clear_cells(&mut self) {
        let lang = self.text_language();
        let result = match (self.table_block(), &self.document, self.visible_rows()) {
            (Some(block), Some(doc), Some(rows)) => clear_plan(&block, &doc.session, rows, lang),
            _ => return,
        };
        match result {
            Ok(Some(plan)) => {
                self.apply_cell_plan(&plan, CellNotice::NotClearable);
            }
            Ok(None) => {}
            Err(error) => self.report_cell_error(&error),
        }
    }

    /// Remplir vers le bas (R-258).
    fn fill_down(&mut self) {
        let lang = self.text_language();
        let result = match (self.table_block(), &self.document, self.visible_rows()) {
            (Some(block), Some(doc), Some(rows)) => fill_down_plan(&block, &doc.session, rows, lang),
            _ => return,
        };
        match result {
            Ok(Some(plan)) => {
                self.apply_cell_plan(&plan, CellNotice::Skipped);
            }
            Ok(None) => {}
            Err(error) => self.report_cell_error(&error),
        }
    }

    /// Remplir à droite (Ctrl/Cmd+R, R-258 amendée).
    fn fill_right(&mut self) {
        let lang = self.text_language();
        let visible = self.visible_column_list();
        let result = match (self.table_block(), &self.document, self.visible_rows()) {
            (Some(block), Some(doc), Some(rows)) => {
                fill_right_plan(&block, &visible, &doc.session, rows, lang)
            }
            _ => return,
        };
        match result {
            Ok(Some(plan)) => {
                self.apply_cell_plan(&plan, CellNotice::Skipped);
            }
            Ok(None) => {}
            Err(error) => self.report_cell_error(&error),
        }
    }

    /// Ctrl/Cmd+Entrée dans une saisie (H-UI-98) : accepté, la saisie se
    /// ferme et le clavier revient à la table ; refusé (lecture ou
    /// registre), la saisie reste ouverte avec le message.
    fn fill_typed(&mut self, text: &str, ctx: &egui::Context) {
        let lang = self.text_language();
        let result = match (self.table_block(), &self.document, self.visible_rows()) {
            (Some(block), Some(doc), Some(rows)) => fill_with(&block, text, &doc.session, rows, lang),
            _ => return,
        };
        let accepted = match result {
            Ok(plan) => self.apply_cell_plan(&plan, CellNotice::Skipped),
            Err(error) => {
                self.report_cell_error(&error);
                false
            }
        };
        if accepted {
            let column = self.view.edit.editing.as_ref().map(|state| state.column);
            self.view.edit.editing = None;
            self.view.edit.focus_pending = false;
            if let Some(column) = column {
                self.view.cursor.column = column;
            }
            ctx.memory_mut(|memory| memory.surrender_focus(crate::grid::editor_id()));
            self.apply(Action::FocusPane(Pane::Table), ctx);
        } else {
            self.view.edit.focus_pending = true;
        }
    }

    /// Colle `text` dans les cellules du bloc (R-257, R-262) ; le bloc
    /// écrit devient la sélection (H-UI-106).
    pub(crate) fn paste_cells(&mut self, text: &str) {
        let lang = self.text_language();
        let visible = self.visible_column_list();
        let rows = self.visible_rows();
        let block = self.table_block().map(|mut block| {
            // Lignes entières (R-261) : le collage de cellules part de la
            // cellule active seule (bloc d'une cellule) ; jamais de l'ID,
            // jamais modifiable (ligne choisie par son N°, relecture
            // finale ux2, C7) : le Nom, ou à défaut la première colonne
            // affichée après l'ID.
            if block.kind == SelectionKind::Rows {
                let active = match (block.active.0, rows) {
                    (crate::nav::RowRef::Item(uid), Some(rows)) => rows.display_of(uid),
                    _ => None,
                };
                block.rows = active.map_or_else(Vec::new, |d| vec![d]);
                block.columns = vec![rows_paste_column(block.active.1, &visible)];
                block.kind = SelectionKind::Cells;
            }
            block
        });
        let result = match (block, &self.document, rows) {
            (Some(block), Some(doc), Some(rows)) => {
                paste_plan(&block, &visible, text, &doc.session, rows, lang)
            }
            _ => return,
        };
        match result {
            Ok(plan) => {
                if self.apply_cell_plan(&plan, CellNotice::Skipped)
                    && let Some(area) = plan.area
                {
                    self.select_area(area);
                }
            }
            Err(error) => self.report_cell_error(&error),
        }
    }

    /// Sélectionne la zone écrite par un collage (H-UI-106) : bloc de
    /// cellules, cellule active au coin haut-gauche, bornée aux lignes
    /// affichées après le collage.
    fn select_area(&mut self, area: PasteArea) {
        let Some(rows) = self.visible_rows() else {
            return;
        };
        let last_row = area
            .top
            .index()
            .saturating_add(area.rows.saturating_sub(1))
            .min(rows.len().saturating_sub(1));
        let (Some(top), Some(bottom)) = (rows.uid(area.top), rows.uid(DisplayRow::at(last_row))) else {
            return;
        };
        self.selected = crate::selection::Selection::range(bottom, top, SelectionKind::Cells, rows);
        self.selection = Some(top);
        self.view.cursor.row = crate::nav::RowRef::Item(top);
        self.view.cursor.column = area.first;
        self.view.cursor.anchor_column = (area.last != area.first).then_some(area.last);
        self.view.cursor.anchor = (bottom != top).then_some(bottom);
        self.sync_selection();
    }

    /// Applique le lot d'une commande de cellule (une entrée d'annulation ;
    /// rien si le lot est vide) puis le compte rendu des cellules sautées ;
    /// rend faux si le registre a refusé le lot (bandeau, rien écrit).
    pub(crate) fn apply_cell_plan(&mut self, plan: &CellPlan, notice: fn(usize) -> CellNotice) -> bool {
        if !plan.batch.ops.is_empty() && self.execute(&plan.batch, crate::app::EditOrigin::User).is_err() {
            return false;
        }
        if plan.skipped > 0 {
            let notice = notice(plan.skipped);
            let text = self
                .texts
                .get_args(notice.text_id(), &[("count", count_arg(notice.count()))]);
            self.push_message(Severity::Info, text);
        }
        if plan.outside > 0 {
            let text = self
                .texts
                .get_args(TextId::PasteValuesOutside, &[("count", count_arg(plan.outside))]);
            self.push_message(Severity::Info, text);
        }
        true
    }

    /// Refus d'une commande de cellule au bandeau (remplace le refus
    /// précédent, comme un refus du registre).
    pub(crate) fn report_cell_error(&mut self, error: &CellError) {
        let detail = match error {
            CellError::Parse { detail, .. } if !detail.is_empty() => Some(detail.clone()),
            _ => None,
        };
        let text = match error {
            CellError::TooLarge => self
                .texts
                .get_args(error.text_id(), &[("max", count_arg(MAX_BLOCK_CELLS))]),
            CellError::Parse { id, column, text, .. } => {
                // H-UI-105 : ID, titre de colonne et texte refusé dans la
                // phrase ; le texte du moteur dans « Détails ».
                let column = self.texts.get(column.title());
                self.texts.get_args(
                    error.text_id(),
                    &[
                        ("id", Arg::Count(i64::from(*id))),
                        ("column", Arg::Str(column.into())),
                        ("detail", Arg::Str(text.clone().into())),
                    ],
                )
            }
            CellError::NotAvailable | CellError::PastEnd | CellError::NothingAbove => {
                self.texts.get(error.text_id())
            }
        };
        let severity = match error {
            CellError::NothingAbove | CellError::NotAvailable => Severity::Info,
            CellError::TooLarge | CellError::Parse { .. } | CellError::PastEnd => Severity::Error,
        };
        self.clear_titled(TextId::CommandErrorTitle);
        self.push_detailed(severity, Some(TextId::CommandErrorTitle), text, detail);
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::panic,
    reason = "tests : un échec doit s'arrêter net et lisiblement ; indices bornés par les stratégies"
)]
mod tests {
    use std::collections::BTreeSet;

    use plannercraft_automation::command::{ConstraintKindJson, SetConstraint};
    use plannercraft_automation::text::Language as TextLanguage;
    use plannercraft_automation::{ApplyMode, Batch, Command, Session, TaskRef};
    use plannercraft_core::{Constraint, Moment, TaskUid};
    use proptest::prelude::*;

    use super::{
        CellError, CellNotice, CellPlan, DeleteTarget, PasteArea, RIBBON_EDITING_TEXTS, clear_plan,
        copy_text, delete_target, fill_down_plan, fill_right_plan, fill_with, grid, paste_plan,
    };
    use crate::action::CellOp;
    use crate::app::{App, DocOrigin, Env};
    use crate::block::Block;
    use crate::column::Column;
    use crate::nav::RowRef;
    use crate::row_view::{DisplayRow, RowView, ViewInput, ViewRow, VisibleRows};
    use crate::selection::SelectionKind;

    const FR: TextLanguage = TextLanguage::Fr;

    /// Colonnes visibles par défaut.
    const VISIBLE: [Column; 6] = [
        Column::Id,
        Column::Name,
        Column::Duration,
        Column::Start,
        Column::Finish,
        Column::Predecessors,
    ];

    fn now() -> Moment {
        "2026-11-02T08:00".parse().unwrap()
    }

    /// Application ouverte sur « P » (UID n = ID n, ligne affichée n − 1).
    fn p_app() -> App {
        let mut app = App::new(crate::UiState::default(), Env::fixed(now()));
        app.open_project(
            plannercraft_tests::daily::plan_p().project().clone(),
            DocOrigin::default(),
        );
        app
    }

    fn session(app: &App) -> &Session {
        &app.document.as_ref().unwrap().session
    }

    fn rows(app: &App) -> VisibleRows<'_> {
        app.visible_rows().unwrap()
    }

    /// Bloc des ID `ids` × colonnes `columns`, cellule active en bas à
    /// droite.
    fn block(ids: &[u32], columns: &[Column]) -> Block {
        let last = *ids.last().unwrap();
        Block {
            kind: SelectionKind::Cells,
            rows: ids.iter().map(|id| DisplayRow(id - 1)).collect(),
            columns: columns.to_vec(),
            active: (RowRef::Item(TaskUid::new(last)), *columns.last().unwrap()),
        }
    }

    /// Session après le lot du plan (une entrée d'annulation).
    fn applied(app: &App, plan: &CellPlan) -> Session {
        let mut s = session(app).clone();
        let before = s.history().entries.len();
        s.apply(&plan.batch, ApplyMode::Commit).unwrap();
        assert_eq!(s.history().entries.len(), before + 1, "un lot, une entrée");
        s
    }

    fn days(s: &Session, n: u32) -> i64 {
        s.project()
            .tasks
            .get(TaskUid::new(n))
            .unwrap()
            .duration
            .minutes()
            .get()
            / 480
    }

    fn preds(s: &Session, n: u32) -> Vec<u32> {
        s.project()
            .tasks
            .get(TaskUid::new(n))
            .unwrap()
            .predecessors
            .iter()
            .map(|l| l.predecessor.get())
            .collect()
    }

    /// R-257 : une valeur seule remplit tout le bloc ; (3, Durée) « 7j » ⇒
    /// 3 dure 7 j, toujours 7 tâches.
    #[test]
    fn r257_single_value_fills_block() {
        let app = p_app();
        let plan = paste_plan(
            &block(&[3], &[Column::Duration]),
            &VISIBLE,
            "7j",
            session(&app),
            rows(&app),
            FR,
        )
        .unwrap();
        let s = applied(&app, &plan);
        assert_eq!(days(&s, 3), 7);
        assert_eq!(s.project().tasks.len(), 7);
        assert_eq!((plan.written, plan.skipped), (1, 0));
        let plan = paste_plan(
            &block(&[6, 7], &[Column::Duration]),
            &VISIBLE,
            "2j\n",
            session(&app),
            rows(&app),
            FR,
        )
        .unwrap();
        let s = applied(&app, &plan);
        assert_eq!((days(&s, 6), days(&s, 7)), (2, 2));
        assert_eq!(plan.batch.label.as_deref(), Some("Coller"));
    }

    /// R-257 : grille posée à partir du coin, lignes affichées et colonnes
    /// visibles suivantes ; colonnes au-delà de la dernière visible
    /// sautées et comptées.
    #[test]
    fn r257_grid_from_corner() {
        let app = p_app();
        let plan = paste_plan(
            &block(&[2], &[Column::Duration]),
            &VISIBLE,
            "3j\r\n4j",
            session(&app),
            rows(&app),
            FR,
        )
        .unwrap();
        let s = applied(&app, &plan);
        assert_eq!((days(&s, 2), days(&s, 3)), (3, 4));
        let plan = paste_plan(
            &block(&[6], &[Column::Name]),
            &VISIBLE,
            "A\t1j\nB\t2j",
            session(&app),
            rows(&app),
            FR,
        )
        .unwrap();
        let s = applied(&app, &plan);
        let name = |n: u32| s.project().tasks.get(TaskUid::new(n)).unwrap().name.clone();
        assert_eq!((name(6), name(7)), ("A".to_owned(), "B".to_owned()));
        assert_eq!((days(&s, 6), days(&s, 7)), (1, 2));
        // Prédécesseurs (dernière colonne visible) : la seconde colonne du
        // texte déborde, sautée et comptée.
        let plan = paste_plan(
            &block(&[7], &[Column::Predecessors]),
            &VISIBLE,
            "2\tx",
            session(&app),
            rows(&app),
            FR,
        )
        .unwrap();
        // Seconde passe UX : la valeur hors de la table est comptée à part.
        assert_eq!((plan.written, plan.skipped, plan.outside), (1, 0, 1));
        let s = applied(&app, &plan);
        assert_eq!(preds(&s, 7), [2]);
    }

    /// R-257 : une lecture refusée refuse tout le collage (ID et colonne de
    /// la première cellule fautive), rien n'est écrit.
    #[test]
    fn r257_parse_error_atomic() {
        let app = p_app();
        let err = paste_plan(
            &block(&[2], &[Column::Duration]),
            &VISIBLE,
            "abc",
            session(&app),
            rows(&app),
            FR,
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                CellError::Parse {
                    id: 2,
                    column: Column::Duration,
                    ..
                }
            ),
            "{err:?}"
        );
        let err = paste_plan(
            &block(&[2], &[Column::Duration]),
            &VISIBLE,
            "3j\nabc",
            session(&app),
            rows(&app),
            FR,
        )
        .unwrap_err();
        assert!(matches!(err, CellError::Parse { id: 3, .. }), "{err:?}");
        let err = paste_plan(
            &block(&[6], &[Column::Predecessors]),
            &VISIBLE,
            "99",
            session(&app),
            rows(&app),
            FR,
        )
        .unwrap_err();
        let CellError::Parse { detail, .. } = err else {
            panic!()
        };
        assert!(detail.contains("99"), "{detail}");
    }

    /// R-257 : récapitulative (durée calculée) et ID sautés et comptés.
    #[test]
    fn r257_skips_computed_counts() {
        let app = p_app();
        let plan = paste_plan(
            &block(&[1], &[Column::Duration]),
            &VISIBLE,
            "3j\n4j",
            session(&app),
            rows(&app),
            FR,
        )
        .unwrap();
        assert_eq!((plan.written, plan.skipped), (1, 1));
        let s = applied(&app, &plan);
        assert_eq!(days(&s, 2), 4);
        let plan = paste_plan(
            &block(&[2], &[Column::Id]),
            &VISIBLE,
            "9\tX",
            session(&app),
            rows(&app),
            FR,
        )
        .unwrap();
        assert_eq!((plan.written, plan.skipped), (1, 1));
        // Tout sauté : lot vide, rien à annuler.
        let plan = paste_plan(
            &block(&[1, 5], &[Column::Start]),
            &VISIBLE,
            "02/11/2026",
            session(&app),
            rows(&app),
            FR,
        )
        .unwrap();
        assert_eq!(plan.batch.ops.len(), 0);
        assert_eq!(plan.skipped, 2);
    }

    /// R-257 : lignes au-delà de la dernière tâche ⇒ tâches ajoutées en fin
    /// comme une saisie sur la ligne vide (plusieurs colonnes : une tâche).
    #[test]
    fn r257_past_end_adds_tasks() {
        let app = p_app();
        let plan = paste_plan(
            &block(&[7], &[Column::Name]),
            &VISIBLE,
            "Finitions\t2j\nRéception\t1j\nNettoyage\t3j\n",
            session(&app),
            rows(&app),
            FR,
        )
        .unwrap();
        let s = applied(&app, &plan);
        let names: Vec<String> = s.project().tasks.iter().map(|t| t.name.clone()).collect();
        assert_eq!(&names[6..], ["Finitions", "Réception", "Nettoyage"]);
        let last: Vec<i64> = s
            .project()
            .tasks
            .iter()
            .skip(7)
            .map(|t| t.duration.minutes().get() / 480)
            .collect();
        assert_eq!(last, [1, 3]);
        // Sur la ligne vide : la tâche se crée par sa première cellule.
        let blank = Block {
            kind: SelectionKind::Cells,
            rows: vec![DisplayRow(7)],
            columns: vec![Column::Duration],
            active: (RowRef::Blank, Column::Duration),
        };
        let plan = paste_plan(&blank, &VISIBLE, "2j\t\t\t3", session(&app), rows(&app), FR).unwrap();
        let s = applied(&app, &plan);
        assert_eq!(s.project().tasks.len(), 8);
        assert_eq!(days(&s, 8), 2);
        assert_eq!(preds(&s, 8), [3]);
    }

    /// R-257 : sous tri, filtre ou repli, ajouter des tâches en fin est
    /// refusé (`cells.past_end`), rien n'est écrit.
    #[test]
    fn r257_past_end_refused_under_filter() {
        let app = p_app();
        let model = app.rows().unwrap();
        let view_rows: Vec<ViewRow> = (0..model.len())
            .filter_map(|i| model.row(i))
            .map(|r| ViewRow {
                uid: r.uid,
                level: r.level,
                summary: r.kind == crate::rows::RowKind::Summary,
                shown: true,
            })
            .collect();
        let view = RowView::compose(&ViewInput {
            rows: &view_rows,
            sort: None,
            outline_level: Some(1),
            collapsed: &BTreeSet::new(),
            links: &[],
        });
        let visible = VisibleRows::new(model, &view);
        assert_eq!(visible.len(), 2);
        let at = Block {
            kind: SelectionKind::Cells,
            rows: vec![DisplayRow(1)],
            columns: vec![Column::Name],
            active: (RowRef::Item(TaskUid::new(5)), Column::Name),
        };
        let err = paste_plan(&at, &VISIBLE, "A\nB", session(&app), visible, FR).unwrap_err();
        assert_eq!(err, CellError::PastEnd);
        // Une seule ligne, dans la vue : accepté.
        assert!(paste_plan(&at, &VISIBLE, "A\n\t", session(&app), visible, FR).is_ok());
    }

    /// R-258 : la première ligne du bloc recopiée dans les suivantes, un
    /// lot.
    #[test]
    fn r258_fill_down_block() {
        let app = p_app();
        let plan = fill_down_plan(
            &block(&[2, 3], &[Column::Duration]),
            session(&app),
            rows(&app),
            FR,
        )
        .unwrap()
        .unwrap();
        let s = applied(&app, &plan);
        assert_eq!(days(&s, 3), 5);
        // Prédécesseurs : liens recopiés (vide ⇒ liens retirés).
        let plan = fill_down_plan(
            &block(&[2, 3], &[Column::Predecessors]),
            session(&app),
            rows(&app),
            FR,
        )
        .unwrap()
        .unwrap();
        let s = applied(&app, &plan);
        assert_eq!(preds(&s, 3).len(), 0);
    }

    /// R-258 : bloc d'une ligne ⇒ source = ligne affichée au-dessus ;
    /// première ligne ⇒ refus `cells.nothing_above`.
    #[test]
    fn r258_single_row_copies_above() {
        let app = p_app();
        let plan = fill_down_plan(&block(&[7], &[Column::Duration]), session(&app), rows(&app), FR)
            .unwrap()
            .unwrap();
        let s = applied(&app, &plan);
        assert_eq!(days(&s, 7), 4);
        let err = fill_down_plan(&block(&[1], &[Column::Name]), session(&app), rows(&app), FR).unwrap_err();
        assert_eq!(err, CellError::NothingAbove);
        // Ligne vide : rien.
        let blank = Block {
            kind: SelectionKind::Cells,
            rows: vec![DisplayRow(7)],
            columns: vec![Column::Name],
            active: (RowRef::Blank, Column::Name),
        };
        assert_eq!(fill_down_plan(&blank, session(&app), rows(&app), FR), Ok(None));
    }

    /// R-258 : une récapitulative du bloc est sautée et comptée.
    #[test]
    fn r258_summary_target_skipped() {
        let app = p_app();
        let plan = fill_down_plan(
            &block(&[4, 5, 6], &[Column::Duration]),
            session(&app),
            rows(&app),
            FR,
        )
        .unwrap()
        .unwrap();
        assert_eq!((plan.written, plan.skipped), (1, 1));
        let s = applied(&app, &plan);
        assert_eq!(days(&s, 6), 0);
        // Ctrl+Entrée : même chemin, texte saisi partout.
        let plan = fill_with(
            &block(&[2, 3], &[Column::Duration]),
            "2",
            session(&app),
            rows(&app),
            FR,
        )
        .unwrap();
        let s = applied(&app, &plan);
        assert_eq!((days(&s, 2), days(&s, 3)), (2, 2));
        assert!(
            fill_with(
                &block(&[2, 3], &[Column::Duration]),
                "x",
                session(&app),
                rows(&app),
                FR
            )
            .is_err()
        );
    }

    /// R-259 : Prédécesseurs effacés ⇒ liens retirés, un lot.
    #[test]
    fn r259_clear_predecessors() {
        let app = p_app();
        let mut b = block(&[3, 6], &[Column::Predecessors]);
        b.rows = vec![DisplayRow(2), DisplayRow(5)];
        let plan = clear_plan(&b, session(&app), rows(&app), FR).unwrap().unwrap();
        let s = applied(&app, &plan);
        assert_eq!((preds(&s, 3).len(), preds(&s, 6).len()), (0, 0));
        assert_eq!(preds(&s, 4), [3], "3→4 gardé");
        // Rien à effacer (aucun lien) : aucun lot.
        assert_eq!(
            clear_plan(
                &block(&[2], &[Column::Predecessors]),
                session(&app),
                rows(&app),
                FR
            ),
            Ok(None)
        );
    }

    /// R-259 : Début ou Fin d'une tâche automatique ⇒ contrainte par
    /// défaut (une opération par tâche) ; récapitulative : non effaçable.
    #[test]
    fn r259_clear_start_removes_constraint() {
        let app = p_app();
        let mut s = session(&app).clone();
        s.apply(
            &Batch {
                format_version: Batch::FORMAT_VERSION,
                lang: None,
                ops: vec![Command::TaskSetConstraint(SetConstraint {
                    uid: TaskRef::Uid(TaskUid::new(6)),
                    kind: ConstraintKindJson::Snet,
                    date: Some("2026-12-01T08:00".parse().unwrap()),
                })],
                label: None,
            },
            ApplyMode::Commit,
        )
        .unwrap();
        let mut app = p_app();
        app.open_project(s.project().clone(), DocOrigin::default());
        let plan = clear_plan(
            &block(&[6], &[Column::Start, Column::Finish]),
            session(&app),
            rows(&app),
            FR,
        )
        .unwrap()
        .unwrap();
        assert_eq!(plan.batch.ops.len(), 1);
        let s = applied(&app, &plan);
        assert_eq!(
            s.project().tasks.get(TaskUid::new(6)).unwrap().constraint,
            Constraint::AsSoonAsPossible
        );
        let plan = clear_plan(&block(&[5], &[Column::Start]), session(&app), rows(&app), FR)
            .unwrap()
            .unwrap();
        assert_eq!(plan.batch.ops.len(), 0);
        assert_eq!(plan.skipped, 1);
    }

    /// R-259 : une durée ne peut pas être vide : rien, compté.
    #[test]
    fn r259_duration_not_clearable() {
        let app = p_app();
        let plan = clear_plan(&block(&[3], &[Column::Duration]), session(&app), rows(&app), FR)
            .unwrap()
            .unwrap();
        assert_eq!(plan.batch.ops.len(), 0);
        assert_eq!((plan.written, plan.skipped), (0, 1));
    }

    /// H-UI-95 : copier le texte de saisie des cellules, tabulé.
    #[test]
    fn copy_text_block_tabulated() {
        let app = p_app();
        let doc = app.document.as_ref().unwrap();
        let text = copy_text(
            &block(&[2, 3], &[Column::Name, Column::Duration]),
            doc,
            rows(&app),
            FR,
        );
        let lines: Vec<&str> = text.split('\n').collect();
        assert_eq!(lines.len(), 2, "{text:?}");
        assert!(lines[0].starts_with("Fondations\t"), "{text:?}");
        assert!(lines[1].starts_with("Murs\t"), "{text:?}");
        // Recollé tel quel : rien ne change de valeur, et (R-260 d,
        // seconde passe UX) aucune opération : aucune entrée d'annulation.
        let plan = paste_plan(
            &block(&[2], &[Column::Name]),
            &VISIBLE,
            &text,
            session(&app),
            rows(&app),
            FR,
        )
        .unwrap();
        assert_eq!(plan.batch.ops, []);
        assert_eq!(plan.written, 0);
    }

    /// Bornes de R-256 pour la grille (règle 10).
    #[test]
    fn cells_grid_bounds() {
        assert_eq!(grid("a\tb\r\nc\n").unwrap(), vec![vec!["a", "b"], vec!["c"]]);
        assert_eq!(grid("").unwrap(), vec![vec![""]]);
        assert_eq!(grid(&"x\n".repeat(10_001)), Err(CellError::TooLarge));
        assert_eq!(grid(&"\t".repeat(200_001)), Err(CellError::TooLarge));
    }

    /// UID visé par une opération de cellule (`None` : création en fin).
    fn target(op: &Command) -> Option<TaskRef> {
        match op {
            Command::TaskUpdate(u) => Some(u.uid.clone()),
            Command::ProgressUpdate(p) => Some(p.task.clone()),
            Command::TaskSetConstraint(c) => Some(c.uid.clone()),
            Command::LinkAdd(l) | Command::LinkUpdate(l) => Some(l.succ.clone()),
            Command::LinkRemove(k) => Some(k.succ.clone()),
            Command::TaskAdd(_) => None,
            other => panic!("opération inattendue {other:?}"),
        }
    }

    fn token() -> impl Strategy<Value = String> {
        prop_oneof![
            Just(String::new()),
            Just("3j".to_owned()),
            Just("0".to_owned()),
            Just("2,5s".to_owned()),
            Just("02/11/2026".to_owned()),
            Just("lun 09/11/26".to_owned()),
            Just("50%".to_owned()),
            Just("2;3FD+1j".to_owned()),
            Just("abc".to_owned()),
            Just("\t".to_owned()),
            Just("\r\n".to_owned()),
            "[a-zé ]{0,6}",
            (0_u32..60).prop_map(|n| n.to_string()),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// P59 : sur l'exemple embarqué (récapitulatives, jalons), toute
        /// vue (identité, repli au niveau 1 ou 2), tout bloc et tout texte :
        /// aucune panique ; `Ok` ⇒ chaque opération vise une tâche d'une
        /// ligne du bloc (collage : des lignes affichées à partir du coin)
        /// ou crée une tâche en fin ; deux appels ⇒ même lot.
        #[test]
        fn p59_cell_plans_props(
            level in prop::option::of(1_u8..3),
            r0 in 0_usize..60,
            h in 1_usize..6,
            c0 in 0_usize..6,
            w in 1_usize..4,
            tokens in prop::collection::vec(token(), 0..40),
        ) {
            let mut app = App::new(crate::UiState::default(), Env::fixed(now()));
            app.open_demo();
            let model = app.rows().unwrap();
            let view_rows: Vec<ViewRow> = (0..model.len())
                .filter_map(|i| model.row(i))
                .map(|r| ViewRow { uid: r.uid, level: r.level, summary: r.kind == crate::rows::RowKind::Summary, shown: true })
                .collect();
            let view = RowView::compose(&ViewInput { rows: &view_rows, sort: None, outline_level: level, collapsed: &BTreeSet::new(), links: &[] });
            let rows = VisibleRows::new(model, &view);
            let s = session(&app);
            let len = rows.len();
            let top = r0 % (len + 1);
            let block = if top == len {
                Block { kind: SelectionKind::Cells, rows: vec![DisplayRow::at(len)], columns: vec![VISIBLE[c0]], active: (RowRef::Blank, VISIBLE[c0]) }
            } else {
                let rs: Vec<DisplayRow> = (top..(top + h).min(len)).map(DisplayRow::at).collect();
                let cs: Vec<Column> = VISIBLE[c0..(c0 + w).min(VISIBLE.len())].to_vec();
                let last = *rs.last().unwrap();
                Block { kind: SelectionKind::Cells, active: (RowRef::Item(rows.uid(last).unwrap()), *cs.last().unwrap()), rows: rs, columns: cs }
            };
            let text: String = tokens.concat();
            let in_block: BTreeSet<TaskUid> = block.rows.iter().filter_map(|d| rows.uid(*d)).collect();
            let below: BTreeSet<TaskUid> = (top..len).filter_map(|i| rows.uid(DisplayRow::at(i))).collect();
            let check = |plan: &CellPlan, allowed: &BTreeSet<TaskUid>, adds: bool| {
                for op in &plan.batch.ops {
                    match target(op) {
                        Some(TaskRef::Uid(uid)) => assert!(allowed.contains(&uid), "{op:?} hors du bloc"),
                        Some(TaskRef::Label(_)) | None => assert!(adds, "{op:?} : création hors collage"),
                    }
                }
            };
            let paste = paste_plan(&block, &VISIBLE, &text, s, rows, FR);
            prop_assert_eq!(&paste, &paste_plan(&block, &VISIBLE, &text, s, rows, FR));
            if let Ok(plan) = &paste { check(plan, &below, true); }
            let fill = fill_with(&block, &text, s, rows, FR);
            prop_assert_eq!(&fill, &fill_with(&block, &text, s, rows, FR));
            if let Ok(plan) = &fill { check(plan, &in_block, true); }
            let down = fill_down_plan(&block, s, rows, FR);
            prop_assert_eq!(&down, &fill_down_plan(&block, s, rows, FR));
            if let Ok(Some(plan)) = &down { check(plan, &in_block, false); }
            let clear = clear_plan(&block, s, rows, FR);
            prop_assert_eq!(&clear, &clear_plan(&block, s, rows, FR));
            if let Ok(Some(plan)) = &clear { check(plan, &in_block, false); }
        }
    }

    /// R-262 : « 2j\n3j » sur Durée 2, 3, 6, 7 ⇒ 2, 3, 2, 3 j (répétée) ;
    /// 1 × 2 sur 3 × 2 ⇒ répétée trois fois ; un lot, zone sélectionnable.
    #[test]
    fn r262_tiles_multiple() {
        let app = p_app();
        let target = block(&[2, 3, 6, 7], &[Column::Duration]);
        let plan = paste_plan(&target, &VISIBLE, "2j\n3j", session(&app), rows(&app), FR).unwrap();
        let s = applied(&app, &plan);
        assert_eq!([2, 3, 6, 7].map(|n| days(&s, n)), [2, 3, 2, 3], "source répétée");
        // 7 durait déjà 3 j : non-changement (R-260 d), trois cellules
        // écrites.
        assert_eq!(plan.written, 3);
        let plan = paste_plan(
            &block(&[2, 3, 6], &[Column::Name, Column::Duration]),
            &VISIBLE,
            "X\t4j",
            session(&app),
            rows(&app),
            FR,
        )
        .unwrap();
        let s = applied(&app, &plan);
        for n in [2, 3, 6] {
            assert_eq!(s.project().tasks.get(TaskUid::new(n)).unwrap().name, "X");
            assert_eq!(days(&s, n), 4);
        }
        assert_eq!(
            plan.area,
            Some(PasteArea {
                top: DisplayRow(1),
                rows: 3,
                first: Column::Name,
                last: Column::Duration,
            })
        );
        // « 2j\t3j » sur Durée-Début 2-5 : répétée, Début ne lit pas « 3j »
        // ⇒ refus nommé, rien écrit.
        let err = paste_plan(
            &block(&[2, 3, 4, 5], &[Column::Duration, Column::Start]),
            &VISIBLE,
            "2j\t3j",
            session(&app),
            rows(&app),
            FR,
        )
        .unwrap_err();
        let CellError::Parse { id, column, text, .. } = err else {
            panic!("{err:?}")
        };
        assert_eq!((id, column, text.as_str()), (2, Column::Start, "3j"));
    }

    /// R-262 : destination qui n'est pas un multiple ⇒ collée une fois
    /// depuis le coin (R-257).
    #[test]
    fn r262_non_multiple_once() {
        let app = p_app();
        let plan = paste_plan(
            &block(&[2, 3, 6], &[Column::Duration]),
            &VISIBLE,
            "2j\n3j",
            session(&app),
            rows(&app),
            FR,
        )
        .unwrap();
        let s = applied(&app, &plan);
        assert_eq!([2, 3, 6].map(|n| days(&s, n)), [2, 3, 4]);
        assert_eq!(plan.written, 2);
        assert_eq!(plan.area.map(|a| (a.top, a.rows)), Some((DisplayRow(1), 2)));
    }

    /// R-260 d et R-158 : Début puis Fin collés sur une même tâche ⇒ la
    /// Fin l'emporte ; recollé, la Fin n'est pas un non-changement (le Début
    /// du même lot a remplacé sa contrainte) : même état final.
    #[test]
    fn r260_start_then_finish_same_lot() {
        let app = p_app();
        let paste = |s: &Session| {
            paste_plan(
                &block(&[2], &[Column::Start]),
                &VISIBLE,
                "09/11/2026\t13/11/2026",
                s,
                rows(&app),
                FR,
            )
            .unwrap()
        };
        let first = paste(session(&app));
        assert_eq!(first.batch.ops.len(), 2);
        let s = applied(&app, &first);
        let constraint = |s: &Session| s.project().tasks.get(TaskUid::new(2)).unwrap().constraint;
        let after_first = constraint(&s);
        assert!(
            matches!(after_first, Constraint::FinishNoEarlierThan { .. }),
            "{after_first:?}"
        );
        let second = paste(&s);
        assert_eq!(second.batch.ops.len(), 2, "{:?}", second.batch.ops);
        let mut again = s.clone();
        again.apply(&second.batch, ApplyMode::Commit).unwrap();
        assert_eq!(constraint(&again), after_first);
    }

    /// R-258 amendée (Ctrl/Cmd+R) : la première colonne du bloc recopiée
    /// vers la droite ; bloc d'une colonne : celle de gauche ; jamais l'ID.
    #[test]
    fn r258_fill_right() {
        let app = p_app();
        // (2, Début)-(2, Fin) : la date de début devient la fin saisie
        // (contrainte de R-158), un lot.
        let plan = fill_right_plan(
            &block(&[2], &[Column::Start, Column::Finish]),
            &VISIBLE,
            session(&app),
            rows(&app),
            FR,
        )
        .unwrap()
        .unwrap();
        assert_eq!(plan.written, 1);
        assert!(
            matches!(&plan.batch.ops[..], [Command::TaskUpdate(u)] if u.finish != plannercraft_automation::Patch::Keep),
            "{:?}",
            plan.batch.ops
        );
        assert_eq!(plan.batch.label.as_deref(), Some("Remplir"));
        applied(&app, &plan);
        // Bloc d'une colonne (Fin) : source = Début, à sa gauche.
        let single = fill_right_plan(
            &block(&[2], &[Column::Finish]),
            &VISIBLE,
            session(&app),
            rows(&app),
            FR,
        )
        .unwrap()
        .unwrap();
        assert_eq!(single.batch.ops, plan.batch.ops);
        // (6, Durée)-(6, Début) : Début ne lit pas « 4 jours » ⇒ refus nommé.
        let err = fill_right_plan(
            &block(&[6], &[Column::Duration, Column::Start]),
            &VISIBLE,
            session(&app),
            rows(&app),
            FR,
        )
        .unwrap_err();
        assert!(
            matches!(&err, CellError::Parse { id: 6, column: Column::Start, text, .. } if text == "4 jours"),
            "{err:?}"
        );
        // Nom seul (l'ID à sa gauche) ou ID seul : rien.
        for columns in [[Column::Name], [Column::Id]] {
            assert_eq!(
                fill_right_plan(&block(&[2], &columns), &VISIBLE, session(&app), rows(&app), FR),
                Ok(None)
            );
        }
    }

    /// Paire (prédécesseur, successeur) d'une opération de lien ajoutée.
    fn added_pair(op: &Command) -> Option<(TaskRef, TaskRef)> {
        match op {
            Command::LinkAdd(spec) => Some((spec.pred.clone(), spec.succ.clone())),
            _ => None,
        }
    }

    /// Projet généré : `n` tâches d'un jour, quelques indentations
    /// (récapitulatives) et des liens vers l'avant (refus ignorés).
    fn generated(n: usize, indents: &[usize], links: &[(usize, usize)]) -> Session {
        let tasks: Vec<(String, &str)> = (1..=n).map(|i| (format!("T{i}"), "1j")).collect();
        let refs: Vec<(&str, &str)> = tasks.iter().map(|(a, b)| (a.as_str(), *b)).collect();
        let mut s = plannercraft_tests::resource_edit::with_tasks(&refs);
        for i in indents {
            let uid = u32::try_from(i % n + 1).unwrap();
            drop(plannercraft_tests::task_edit::try_apply(
                &mut s,
                serde_json::json!([{ "op": "task.indent", "uid": uid }]),
            ));
        }
        for (a, b) in links {
            let (a, b) = (a % n + 1, b % n + 1);
            if a < b {
                drop(plannercraft_tests::task_edit::try_apply(
                    &mut s,
                    serde_json::json!([{ "op": "link.add", "pred": a, "succ": b }]),
                ));
            }
        }
        s
    }

    /// Application ouverte sur la session `s`.
    fn app_on(s: &Session) -> App {
        let mut app = App::new(crate::UiState::default(), Env::fixed(now()));
        app.open_project(s.project().clone(), DocOrigin::default());
        app
    }

    /// P61 : aucun `link.add` d'une paire déjà liée (dans ce sens), ni deux
    /// fois la même paire (Lier : dans un sens ou dans l'autre).
    fn check_links(s: &Session, batch: &plannercraft_automation::Batch, either_way: bool) {
        let p = s.project();
        let linked = |a: TaskUid, b: TaskUid| {
            p.tasks
                .get(b)
                .is_some_and(|t| t.predecessors.iter().any(|l| l.predecessor == a))
        };
        let mut seen = BTreeSet::new();
        for (pred, succ) in batch.ops.iter().filter_map(added_pair) {
            if let (TaskRef::Uid(a), TaskRef::Uid(b)) = (&pred, &succ) {
                assert!(!linked(*a, *b), "lien {a}→{b} déjà présent : {batch:?}");
                if either_way {
                    assert!(!linked(*b, *a), "paire {b}→{a} déjà liée : {batch:?}");
                }
            }
            assert!(
                seen.insert(format!("{pred:?}{succ:?}")),
                "paire en double : {batch:?}"
            );
        }
    }

    fn link_token() -> impl Strategy<Value = String> {
        prop_oneof![
            (1_u32..14).prop_map(|n| n.to_string()),
            (1_u32..14).prop_map(|n| format!("{n}DD")),
            (1_u32..14).prop_map(|n| format!("{n}FF+1j")),
        ]
    }

    fn cell_token() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("3j".to_owned()),
            Just("0".to_owned()),
            Just("2,5s".to_owned()),
            Just("02/11/2026".to_owned()),
            Just("50%".to_owned()),
            Just("Nom".to_owned()),
            (1_u32..14).prop_map(|n| n.to_string()),
        ]
    }

    proptest! {
        // La CI force `PROPTEST_CASES=10000` par-dessus `with_cases` ; un
        // tiers des cas générés est écarté (`prop_assume` : colonnes Début et
        // Fin ensemble), donc le plafond de rejets globaux suit le nombre de
        // cas au lieu des 1 024 par défaut.
        #![proptest_config(ProptestConfig { cases: 256, max_global_rejects: 1_000_000, ..ProptestConfig::default() })]

        /// P61 (seconde passe UX, R-260) : sur un projet généré (≤ 40
        /// tâches, récapitulatives, liens), toute saisie de Prédécesseurs,
        /// toute chaîne de Lier, tout bloc collé, rempli vers le bas ou à
        /// droite : aucun `link.add` d'une paire déjà liée ni deux fois la
        /// même paire ; appliquer le lot puis replanifier le même geste ⇒
        /// lot vide ; aucune panique.
        #[test]
        fn p61_ui_batches_idempotent(
            n in 3_usize..40,
            indents in prop::collection::vec(0_usize..40, 0..8),
            links in prop::collection::vec((0_usize..40, 0_usize..40), 0..30),
            target in 0_usize..40,
            entry in prop::collection::vec(link_token(), 0..5),
            chain in prop::collection::vec(0_usize..40, 0..6),
            r0 in 0_usize..40,
            h in 1_usize..4,
            c0 in 1_usize..6,
            w in 1_usize..3,
            cells in prop::collection::vec(cell_token(), 1..5),
            gesture in 0_u8..4,
        ) {
            let s = generated(n, &indents, &links);
            let app = app_on(&s);
            let s = session(&app).clone();
            let rows = rows(&app);
            let len = rows.len();
            // (a) Prédécesseurs saisis.
            let uid = rows.uid(DisplayRow::at(target % len)).unwrap();
            let state = crate::edit::EditState::typed(Some(uid), Column::Predecessors, &entry.join(";"));
            if let Ok(batch) = crate::edit::commit_edit(&state, &s, FR) {
                check_links(&s, &batch, false);
                let mut after = s.clone();
                if after.apply(&batch, ApplyMode::Commit).is_ok() {
                    let again = crate::edit::commit_edit(&state, &after, FR).unwrap();
                    prop_assert_eq!(again.ops, Vec::<Command>::new());
                }
            }
            // (b) Lier une chaîne.
            let mut sel = crate::selection::Selection::default();
            for i in &chain {
                if let Some(u) = rows.uid(DisplayRow::at(i % len)) {
                    sel.click(u, crate::selection::SelectMode::Toggle, rows);
                }
            }
            if let Some(plan) = crate::structure::plan(crate::structure::StructureOp::Link, s.project(), &sel) {
                check_links(&s, &plan.batch, true);
                let mut after = s.clone();
                if after.apply(&plan.batch, ApplyMode::Commit).is_ok() {
                    prop_assert_eq!(
                        crate::structure::plan(crate::structure::StructureOp::Link, after.project(), &sel),
                        None
                    );
                }
            }
            // (c) Bloc collé ou rempli (lignes existantes seulement).
            let top = r0 % len;
            let rs: Vec<DisplayRow> = (top..(top + h).min(len)).map(DisplayRow::at).collect();
            let cs: Vec<Column> = VISIBLE[c0..(c0 + w).min(VISIBLE.len())].to_vec();
            // Début et Fin saisis sur une même tâche : chacun pose la
            // contrainte (R-158), la dernière l'emporte ; rejouer le geste
            // les repose dans le même ordre (effet net nul, mais deux
            // opérations) : hors de la propriété.
            let reach = VISIBLE.get(c0..(c0 + w.max(cells.len())).min(VISIBLE.len())).unwrap_or(&[]);
            prop_assume!(!(reach.contains(&Column::Start) && reach.contains(&Column::Finish)));
            let last = *rs.last().unwrap();
            let b = Block { kind: SelectionKind::Cells, active: (RowRef::Item(rows.uid(last).unwrap()), *cs.last().unwrap()), rows: rs, columns: cs };
            let text = cells.join("\t");
            let plan = |s: &Session, rows: VisibleRows<'_>| -> Option<CellPlan> {
                match gesture {
                    0 => paste_plan(&b, &VISIBLE, &text, s, rows, FR).ok(),
                    1 => fill_with(&b, &text, s, rows, FR).ok(),
                    2 => fill_down_plan(&b, s, rows, FR).ok().flatten(),
                    _ => fill_right_plan(&b, &VISIBLE, s, rows, FR).ok().flatten(),
                }
            };
            if let Some(first) = plan(&s, rows) {
                check_links(&s, &first.batch, false);
                let creates = first.batch.ops.iter().any(|op| matches!(op, Command::TaskAdd(_)));
                // Remplir vers le bas ou à droite relit sa source après le
                // lot : une source calculée qui dépend des cibles (durée
                // d'une récapitulative remplie dans sa sous-tâche) change ;
                // l'idempotence ne vaut que pour un texte fixé.
                let fixed = gesture < 2;
                let mut after_app = app_on(&s);
                if fixed && !creates && after_app.execute(&first.batch, crate::app::EditOrigin::Agent).is_ok() {
                    let after = session(&after_app).clone();
                    let rows = after_app.visible_rows().unwrap();
                    if let Some(second) = plan(&after, rows) {
                        prop_assert_eq!(second.batch.ops, Vec::<Command>::new(), "geste rejoué");
                    }
                }
            }
        }
    }

    /// H-UI-97 : lignes entières, ou une seule cellule de l'ID ou du Nom ⇒
    /// supprimer les tâches ; Durée (et toute autre colonne) ⇒ effacer.
    #[test]
    fn h_ui_97_delete_target() {
        let block = |kind, column| Block {
            kind,
            rows: vec![DisplayRow(3)],
            columns: vec![column],
            active: (RowRef::Item(TaskUid::new(3)), column),
        };
        assert_eq!(
            delete_target(&block(SelectionKind::Rows, Column::Duration)),
            DeleteTarget::Tasks
        );
        assert_eq!(
            delete_target(&block(SelectionKind::Cells, Column::Id)),
            DeleteTarget::Tasks
        );
        assert_eq!(
            delete_target(&block(SelectionKind::Cells, Column::Name)),
            DeleteTarget::Tasks
        );
        for column in [
            Column::Duration,
            Column::Start,
            Column::Predecessors,
            Column::PercentComplete,
        ] {
            assert_eq!(
                delete_target(&block(SelectionKind::Cells, column)),
                DeleteTarget::Contents
            );
        }
        // Relecture finale n° 3 : plusieurs cellules ⇒ effacer, quelle que
        // soit la cellule active (colonne du Nom entière ; bloc Durée-Nom
        // glissé vers le Nom ou vers la Durée) ; lignes entières de
        // plusieurs lignes ⇒ supprimer.
        let rows = vec![DisplayRow(2), DisplayRow(3), DisplayRow(4), DisplayRow(5)];
        let wide = |kind, columns: Vec<Column>, active| Block {
            kind,
            rows: rows.clone(),
            columns,
            active: (RowRef::Item(TaskUid::new(5)), active),
        };
        assert_eq!(
            delete_target(&wide(SelectionKind::Cells, vec![Column::Name], Column::Name)),
            DeleteTarget::Contents
        );
        for active in [Column::Name, Column::Duration] {
            assert_eq!(
                delete_target(&wide(
                    SelectionKind::Cells,
                    vec![Column::Name, Column::Duration],
                    active
                )),
                DeleteTarget::Contents
            );
        }
        assert_eq!(
            delete_target(&wide(
                SelectionKind::Rows,
                vec![Column::Id, Column::Name, Column::Duration],
                Column::Duration
            )),
            DeleteTarget::Tasks
        );
    }

    /// Contrat L0 : chaque refus et chaque compte rendu a son message ;
    /// les squelettes refusent sans paniquer.
    #[test]
    fn cells_contract_texts_and_skeletons() {
        let errors = [
            CellError::NotAvailable,
            CellError::TooLarge,
            CellError::Parse {
                id: 3,
                column: crate::column::Column::Duration,
                text: String::new(),
                detail: String::new(),
            },
            CellError::PastEnd,
            CellError::NothingAbove,
        ];
        let mut ids: Vec<_> = errors.iter().map(CellError::text_id).collect();
        ids.extend(
            [
                CellNotice::Skipped(1),
                CellNotice::NotClearable(2),
                CellNotice::TasksDeleted(3),
            ]
            .map(CellNotice::text_id),
        );
        ids.extend(RIBBON_EDITING_TEXTS);
        let before = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), before, "un message par cas");
        assert_eq!(CellNotice::TasksDeleted(3).count(), 3);
        assert!(CellOp::FillDown.menu_label().is_some());
        assert!(CellOp::FillRight.menu_label().is_some());
        assert!(CellOp::FillTyped("2".into()).menu_label().is_none());
    }
}
