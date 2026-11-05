//! Couper, copier, coller de lignes dans l'interface (H-UI-39, ADR-0016
//! D3). egui livre `Event::Copy`, `Event::Cut` et `Event::Paste(String)`
//! (jamais Ctrl/Cmd+C/X/V comme touches) ; le tampon interne et son
//! empreinte permettent de reconnaître au collage le dernier texte émis
//! (collage complet, R-244), sinon le collage externe réduit (R-256).
//! Contrat posé par le lot L0, câblé en L10.
//!
//! - Les trois événements sont pris par l'application seulement quand la
//!   table a le focus, sans saisie, menu ni fenêtre ouverts
//!   ([`App::take_clipboard_events`]) ; sinon ils restent à l'éditeur de
//!   texte qui a le focus.
//! - Copier : [`RowClip::capture`] de la sélection (descendants des
//!   récapitulatives inclus), texte tabulé des colonnes visibles
//!   ([`tsv_columns`]) écrit par `ctx.copy_text`, [`ClipboardSlot`] gardé
//!   par l'application (il survit à l'ouverture d'un autre document).
//! - Couper : copier, puis la suppression de la structure dans un lot à
//!   part (une entrée d'annulation), sans confirmation pour une
//!   récapitulative : son bloc est dans le tampon.
//! - Coller : empreinte du texte égale à celle du tampon ⇒
//!   [`paste_batch`] (options par défaut, ou celles de « Collage
//!   spécial… ») ; sinon [`paste_text`]. Insertion avant la tâche active
//!   (fin du projet sur la ligne vide) ; un lot, compte rendu au bandeau.
//! - Menu Édition : Couper et Copier agissent directement (rien n'attend un
//!   événement du système) ; Coller demande le texte du presse-papiers
//!   (`ViewportCommand::RequestPaste`, desktop), dont l'événement est
//!   accepté à l'image suivante même si la table n'a plus le focus. Le
//!   navigateur ne rend pas le presse-papiers à un menu : sur le web, Coller
//!   colle le tampon interne, ou invite à Ctrl+V.
//!
//! Passe UX (ADR-0018 D3, H-UI-95, lot L2) : le geste suit le type de la
//! sélection ([`ClipKind`]). Lignes entières : tout ce qui précède (tampon
//! `RowClip`, H-UI-39). Bloc de cellules : copier écrit le texte tabulé du
//! bloc (`cells::copy_text`), couper = copier puis effacer le contenu
//! (R-259, une entrée d'annulation). Coller : tampon de lignes reconnu par
//! son empreinte ⇒ lignes (R-244) ; texte à en-tête reconnu ⇒ R-256 a ;
//! lignes entières sélectionnées, ou texte d'une colonne sur la ligne vide
//! ⇒ R-256 ; sinon cellules à partir du coin du bloc (R-257). Le clavier
//! reste à la table après un clic dans le Gantt (H-UI-101) : les trois
//! événements y sont pris aussi.
//!
//! Seconde passe UX (ADR-0019 D2, R-261, H-UI-106) : la **destination**
//! décide du collage ([`paste_route`], fonction pure). Bloc de cellules ⇒
//! toujours un collage de cellules, jamais d'insertion (un tampon de lignes
//! entières y est refusé, rien n'est écrit) ; lignes entières ⇒ tampon de
//! lignes inséré avant, texte à en-tête importé, autre texte en cellules à
//! partir de la cellule active ; ligne vide ⇒ tampon ou texte à en-tête en
//! tâches à la fin, autre texte en cellules dans la colonne active. Après
//! un collage de lignes, les lignes collées restent sélectionnées en lignes
//! entières ; après un collage de cellules, le bloc écrit est sélectionné :
//! un second Ctrl/Cmd+V suit le même chemin.

use plannercraft_automation::clip::{
    self, ClipError, MAX_PASTE_BYTES, MAX_PASTE_LINES, PasteOptions, PasteReport, RowClip, paste_batch,
    paste_text,
};
use plannercraft_automation::csv::{self, CsvColumn, Recognized};
use plannercraft_automation::text::TextContext;
use plannercraft_automation::{Batch, Placement, TaskRef};
use plannercraft_core::{Severity, TaskUid};

use crate::app::{App, EditOrigin};
use crate::column::Column;
use crate::i18n::{Arg, TextId, error_text_id};
use crate::selection::{Selection, SelectionKind};
use crate::state::UiState;
use crate::structure::{self, StructureOp};
use crate::view::Pane;

/// Dernière copie de l'application : tampon et empreinte du texte écrit
/// dans le presse-papiers du système.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClipboardSlot {
    /// Tampon interne.
    pub clip: RowClip,
    /// Empreinte FNV-1a du texte émis (`plannercraft_automation::clip::fingerprint`).
    pub fingerprint: u64,
    /// Ce qui a été copié : lignes entières (tampon `clip`, H-UI-39) ou
    /// cellules d'un bloc (texte seul, H-UI-95 ; ADR-0018 D3, lot L2).
    pub kind: ClipKind,
    /// Texte émis pour un bloc de cellules (collé par le menu Coller du
    /// web et « Collage spécial… », qui n'ont pas le presse-papiers du
    /// système) ; vide pour des lignes entières (le tampon `clip` suffit).
    pub text: String,
}

/// Contenu de la dernière copie (ADR-0018 D3, H-UI-95).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClipKind {
    /// Lignes entières : le tampon se colle en lignes (R-244).
    #[default]
    Rows,
    /// Cellules d'un bloc : le texte se colle en cellules (R-257).
    Cells,
}

/// Commande du presse-papiers (menu Édition).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardOp {
    /// Couper la sélection.
    Cut,
    /// Copier la sélection.
    Copy,
    /// Coller le presse-papiers.
    Paste,
    /// « Collage spécial… » : options du collage du tampon interne.
    PasteSpecial,
}

/// Destination d'un collage (R-261), lue de la sélection de la table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteDest {
    /// Bloc de cellules (une cellule ou plus) sur des tâches.
    Cells,
    /// Lignes entières (sélection par le N°).
    Rows,
    /// Ligne vide de fin (cellule seule).
    Blank,
}

/// Chemin d'un collage (R-261).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteRoute {
    /// Tampon de lignes inséré (R-244) : avant la tâche active, en fin sur
    /// la ligne vide.
    InsertRows,
    /// Texte à en-tête reconnu importé en lignes (R-256 a).
    ImportRows,
    /// Collage de cellules (R-257, R-262).
    Cells,
    /// Refus : rien n'est écrit.
    Refuse(PasteRefusal),
}

/// Raison d'un collage refusé.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteRefusal {
    /// Lignes entières copiées, collées sur un bloc de cellules.
    RowsOnCells,
}

impl PasteRefusal {
    /// Message du refus.
    #[must_use]
    pub const fn text_id(self) -> TextId {
        match self {
            Self::RowsOnCells => TextId::PasteRowsOnCells,
        }
    }
}

/// Routage d'un collage (R-261), fonction pure : `dest` la destination,
/// `slot` le type de la dernière copie de l'application si le texte collé
/// est le sien (empreinte reconnue ; `None` : texte venu d'ailleurs),
/// `text` le texte collé.
///
/// - `Cells` ⇒ `Cells` (texte ou tampon de cellules), `Refuse(RowsOnCells)`
///   (tampon de lignes) ;
/// - `Rows` et `Blank` ⇒ `InsertRows` (tampon de lignes), `ImportRows`
///   (texte à en-tête reconnu), `Cells` sinon (cellule active, ou colonne
///   active de la ligne vide : tâches ajoutées en fin).
#[must_use]
pub fn paste_route(dest: PasteDest, slot: Option<ClipKind>, text: &str) -> PasteRoute {
    match (dest, slot) {
        (PasteDest::Cells, Some(ClipKind::Rows)) => PasteRoute::Refuse(PasteRefusal::RowsOnCells),
        (PasteDest::Rows | PasteDest::Blank, Some(ClipKind::Rows)) => PasteRoute::InsertRows,
        (PasteDest::Rows | PasteDest::Blank, None) if has_name_header(text) => PasteRoute::ImportRows,
        _ => PasteRoute::Cells,
    }
}

/// Colonne du texte tabulé pour une colonne de la table : seules les
/// colonnes qu'un import CSV sait relire (ID, Nom, Durée, Début, Fin,
/// Prédécesseurs, % achevé) ; les autres (marge, critique, ressources,
/// ligne de base, variation) sont calculées ou propres à l'affichage.
#[must_use]
pub const fn tsv_column(column: Column) -> Option<CsvColumn> {
    match column {
        Column::Id => Some(CsvColumn::Id),
        Column::Name => Some(CsvColumn::Name),
        Column::Duration => Some(CsvColumn::Duration),
        Column::Start => Some(CsvColumn::Start),
        Column::Finish => Some(CsvColumn::Finish),
        Column::Predecessors => Some(CsvColumn::Predecessors),
        Column::PercentComplete => Some(CsvColumn::PercentComplete),
        // Marge, critique, ressources, ligne de base, variation, mode et
        // champs de la seconde passe UX (copiés à partir du lot L7).
        _ => None,
    }
}

/// Colonnes du texte tabulé : colonnes visibles de la table, dans leur
/// ordre, réduites à celles de [`tsv_column`].
#[must_use]
pub fn tsv_columns(state: &UiState) -> Vec<CsvColumn> {
    state
        .visible_columns()
        .filter_map(|c| tsv_column(c.column))
        .collect()
}

/// Première ligne non vide du texte à en-tête reconnu (une cellule lue
/// comme Nom, R-144) : collage de lignes par l'import (R-256 a).
fn has_name_header(text: &str) -> bool {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .is_some_and(|line| {
            line.split('\t')
                .any(|h| csv::recognize(h) == Recognized::Column(CsvColumn::Name))
        })
}

/// Images après une demande de collage par le menu pendant lesquelles
/// l'événement `Paste` est accepté hors du focus de la table.
const MENU_PASTE_FRAMES: u64 = 2;

impl App {
    /// Dernière copie (tampon interne et empreinte), s'il y en a une.
    #[must_use]
    pub const fn clipboard(&self) -> Option<&ClipboardSlot> {
        self.clipboard.as_ref()
    }

    /// Retire de l'image et traite `Event::Copy`, `Event::Cut` et
    /// `Event::Paste` quand `table` (la table a le focus, sans saisie, menu
    /// ni fenêtre) ou juste après un « Coller » du menu ; sinon ils restent
    /// aux widgets (éditeur de texte).
    pub(crate) fn take_clipboard_events(&mut self, ctx: &egui::Context, table: bool) {
        let frame = ctx.cumulative_frame_nr();
        let menu_paste = self
            .menu_paste_at
            .is_some_and(|at| frame <= at.saturating_add(MENU_PASTE_FRAMES))
            && self.document.is_some()
            && self.view.dialog.is_none()
            && self.view.edit.editing.is_none()
            && !ctx.text_edit_focused();
        if self
            .menu_paste_at
            .is_some_and(|at| frame > at.saturating_add(MENU_PASTE_FRAMES))
        {
            self.menu_paste_at = None;
        }
        // H-UI-101 : un clic dans le Gantt laisse le clavier à la table ;
        // le presse-papiers y agit sur la cellule active.
        let table = table || self.gantt_keeps_table_keys(ctx);
        if !table && !menu_paste {
            return;
        }
        let mut taken = Vec::new();
        ctx.input_mut(|input| {
            input.events.retain(|event| match event {
                egui::Event::Copy | egui::Event::Cut if table => {
                    taken.push(event.clone());
                    false
                }
                egui::Event::Paste(_) => {
                    taken.push(event.clone());
                    false
                }
                _ => true,
            });
        });
        for event in taken {
            match event {
                egui::Event::Copy => {
                    self.copy_selection(ctx);
                }
                egui::Event::Cut => self.cut_selection(ctx),
                egui::Event::Paste(text) => {
                    self.menu_paste_at = None;
                    self.paste_clipboard_text(&text);
                }
                _ => {}
            }
        }
    }

    /// Le Gantt a le focus, sans saisie, menu, fenêtre ni confirmation :
    /// le clavier appartient à la table (H-UI-101).
    fn gantt_keeps_table_keys(&self, ctx: &egui::Context) -> bool {
        self.document.is_some()
            && self.view.focus == Pane::Gantt
            && !ctx.text_edit_focused()
            && !ctx.any_popup_open()
            && self.view.edit.editing.is_none()
            && self.view.dialog.is_none()
            && self.pending_delete.is_none()
            && self.view.resources.edit.editing.is_none()
            && self.view.resources.pending_delete.is_none()
    }

    /// Commande du menu Édition.
    pub(crate) fn clipboard_op(&mut self, op: ClipboardOp, ctx: &egui::Context) {
        if self.document.is_none() {
            return;
        }
        match op {
            ClipboardOp::Copy => {
                self.copy_selection(ctx);
            }
            ClipboardOp::Cut => self.cut_selection(ctx),
            ClipboardOp::Paste => {
                if cfg!(target_arch = "wasm32") {
                    // Le navigateur ne rend le presse-papiers qu'à un
                    // événement `paste` (Ctrl+V) : tampon interne, sinon
                    // invitation au raccourci.
                    if self.clipboard.is_some() {
                        self.paste_internal(PasteOptions::default());
                    } else {
                        let text = self.texts.get(TextId::PasteUseShortcut);
                        self.push_titled(Severity::Info, Some(TextId::PasteReportTitle), text);
                    }
                } else {
                    self.menu_paste_at = Some(ctx.cumulative_frame_nr());
                    ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
                }
            }
            ClipboardOp::PasteSpecial => self.open_paste_special(),
        }
    }

    /// Copie la sélection : lignes entières ⇒ tampon interne et texte
    /// tabulé des colonnes visibles ; bloc de cellules ⇒ texte tabulé des
    /// cellules (H-UI-95). Rend le type copié (`None` : rien, sélection
    /// vide ou ligne vide).
    pub(crate) fn copy_selection(&mut self, ctx: &egui::Context) -> Option<ClipKind> {
        self.sync_selection();
        if self.selected.kind() == SelectionKind::Cells {
            return self.copy_cells(ctx).then_some(ClipKind::Cells);
        }
        let lang = self.text_language();
        let document = self.document.as_ref()?;
        let uids: Vec<TaskUid> = self.selected.uids().iter().copied().collect();
        if uids.is_empty() {
            return None;
        }
        match RowClip::capture(&document.session, &uids) {
            Ok(clip) if clip.tasks.is_empty() => None,
            Ok(clip) => {
                let text_ctx = TextContext::for_project(document.project(), lang);
                let text = clip.to_tsv(&tsv_columns(&self.state), &text_ctx);
                let fingerprint = clip::fingerprint(&text);
                ctx.copy_text(text);
                self.clipboard = Some(ClipboardSlot {
                    clip,
                    fingerprint,
                    kind: ClipKind::Rows,
                    text: String::new(),
                });
                Some(ClipKind::Rows)
            }
            Err(error) => {
                self.report_clip_error(&error);
                None
            }
        }
    }

    /// Copie le bloc de cellules (`cells::copy_text`) ; la ligne vide seule
    /// ne copie rien.
    fn copy_cells(&mut self, ctx: &egui::Context) -> bool {
        let lang = self.text_language();
        let Some(block) = self.table_block() else {
            return false;
        };
        let (Some(document), Some(rows)) = (&self.document, self.visible_rows()) else {
            return false;
        };
        if block.rows.iter().all(|d| rows.row(*d).is_none()) {
            return false;
        }
        if block.cells() > crate::block::MAX_BLOCK_CELLS {
            self.report_cell_error(&crate::cells::CellError::TooLarge);
            return false;
        }
        let text = crate::cells::copy_text(&block, document, rows, lang);
        let fingerprint = clip::fingerprint(&text);
        ctx.copy_text(text.clone());
        self.clipboard = Some(ClipboardSlot {
            clip: RowClip::default(),
            fingerprint,
            kind: ClipKind::Cells,
            text,
        });
        true
    }

    /// Coupe la sélection : copie, puis, pour des lignes entières,
    /// suppression dans un lot à part (annulable seul ; une récapitulative
    /// part avec son bloc, sans confirmation : le bloc est dans le tampon) ;
    /// pour un bloc de cellules, effacement du contenu (R-259, une entrée
    /// d'annulation).
    pub(crate) fn cut_selection(&mut self, ctx: &egui::Context) {
        // Regroupement (finitions P3, H-UI-130) : couper des lignes entières
        // est refusé, rien n'est copié ; couper des cellules reste permis.
        self.sync_selection();
        if self.view.group_by.is_some() && self.selected.kind() == SelectionKind::Rows {
            let text = self.texts.get(TextId::ViewGroupLocked);
            self.push_message(Severity::Info, text);
            return;
        }
        match self.copy_selection(ctx) {
            None => {}
            Some(ClipKind::Cells) => self.clear_cells(),
            Some(ClipKind::Rows) => {
                let Some(document) = &self.document else {
                    return;
                };
                let Some(plan) = structure::plan(StructureOp::Delete, document.project(), &self.selected)
                else {
                    return;
                };
                // Un refus est affiché par `execute`.
                drop(self.execute(&plan.batch, EditOrigin::User));
            }
        }
    }

    /// Destination du collage (R-261) : lignes entières, ligne vide de fin
    /// (cellule seule) ou bloc de cellules.
    #[must_use]
    pub fn paste_dest(&self) -> PasteDest {
        match self.table_block() {
            Some(block) if block.kind == SelectionKind::Rows => PasteDest::Rows,
            Some(block) if block.active.0 != crate::nav::RowRef::Blank => PasteDest::Cells,
            _ => PasteDest::Blank,
        }
    }

    /// Colle un texte reçu du presse-papiers du système selon la
    /// destination ([`paste_route`], R-261) : le tampon de la dernière
    /// copie est reconnu par l'empreinte du texte.
    pub(crate) fn paste_clipboard_text(&mut self, text: &str) {
        if self.document.is_none() {
            return;
        }
        // Bornes de R-256 pour tous les chemins (4 Mio, 10 000 lignes) :
        // même refus, quel que soit le collage.
        if text.len() > MAX_PASTE_BYTES || text.lines().nth(MAX_PASTE_LINES).is_some() {
            self.report_clip_error(&ClipError::TooLarge);
            return;
        }
        self.sync_selection();
        let print = clip::fingerprint(text);
        let slot = self
            .clipboard
            .as_ref()
            .filter(|slot| slot.fingerprint == print)
            .map(|slot| slot.kind);
        let route = paste_route(self.paste_dest(), slot, text);
        self.paste_by_route(route, text, PasteOptions::default());
    }

    /// Colle le tampon interne avec `options` (« Collage spécial… », menu
    /// Coller sur le web), selon la destination (R-261). Sans tampon : rien.
    pub(crate) fn paste_internal(&mut self, options: PasteOptions) {
        let Some(slot) = &self.clipboard else {
            return;
        };
        let (kind, text) = (slot.kind, slot.text.clone());
        self.sync_selection();
        let route = paste_route(self.paste_dest(), Some(kind), &text);
        self.paste_by_route(route, &text, options);
    }

    /// Exécute le chemin `route` d'un collage.
    fn paste_by_route(&mut self, route: PasteRoute, text: &str, options: PasteOptions) {
        let Some(document) = &self.document else {
            return;
        };
        // Regroupement (finitions P3, H-UI-130) : coller des lignes (insérer
        // des tâches) refusé ; le collage de cellules reste permis.
        if self.view.group_by.is_some() && matches!(route, PasteRoute::InsertRows | PasteRoute::ImportRows) {
            let text = self.texts.get(TextId::ViewGroupLocked);
            self.push_message(Severity::Info, text);
            return;
        }
        let at = self.paste_placement();
        let result = match route {
            PasteRoute::Cells => {
                self.paste_cells(text);
                return;
            }
            PasteRoute::Refuse(refusal) => {
                // H-UI-106 : refus en notification, rien n'est écrit.
                let text = self.texts.get(refusal.text_id());
                self.clear_titled(TextId::PasteReportTitle);
                self.clear_titled(TextId::CommandErrorTitle);
                self.push_titled(Severity::Error, Some(TextId::CommandErrorTitle), text);
                return;
            }
            PasteRoute::InsertRows => match &self.clipboard {
                Some(slot) => paste_batch(&document.session, &slot.clip, &at, &options),
                None => return,
            },
            PasteRoute::ImportRows => paste_text(&document.session, text, &at, self.text_language()),
        };
        self.apply_paste(result);
    }

    /// Point d'insertion du collage : avant la tâche active, fin du projet
    /// sur la ligne vide (R-244).
    fn paste_placement(&self) -> Placement {
        self.selection
            .map_or(Placement::End, |uid| Placement::Before(TaskRef::Uid(uid)))
    }

    /// Applique le lot d'un collage (une entrée d'annulation), sélectionne
    /// la première tâche collée et affiche le compte rendu ; un refus
    /// s'affiche au bandeau.
    fn apply_paste(&mut self, result: Result<(Batch, PasteReport), ClipError>) {
        let (batch, report) = match result {
            Ok(pasted) => pasted,
            Err(error) => {
                self.report_clip_error(&error);
                return;
            }
        };
        // Un refus du registre est affiché par `execute`.
        let Ok(outcome) = self.execute(&batch, EditOrigin::User) else {
            return;
        };
        self.select_pasted_rows(&outcome.created);
        self.report_paste(&report);
    }

    /// Lignes collées sélectionnées en lignes entières, la première active
    /// (H-UI-106 : un second collage insère encore des lignes).
    fn select_pasted_rows(&mut self, created: &[TaskUid]) {
        let Some(first) = created.first().copied() else {
            return;
        };
        let range = self.visible_rows().and_then(|rows| {
            let mut shown: Vec<_> = created.iter().filter_map(|u| rows.display_of(*u)).collect();
            shown.sort_unstable();
            let top = shown.first().and_then(|d| rows.uid(*d))?;
            let bottom = shown.last().and_then(|d| rows.uid(*d))?;
            Some((top, Selection::range(bottom, top, SelectionKind::Rows, rows)))
        });
        let (active, selected) = range.unwrap_or_else(|| {
            (
                first,
                Selection::single(Some(first)).with_kind(SelectionKind::Rows),
            )
        });
        self.selection = Some(active);
        self.selected = selected;
        self.sync_selection();
    }

    /// Compte rendu d'un collage réussi, toujours en information (H-UI-104,
    /// titre « Collage ») : tâches collées, affectations omises, suivi non
    /// repris, ressources créées, puis la phrase des liens non repris ;
    /// remplace le compte rendu précédent.
    fn report_paste(&mut self, report: &PasteReport) {
        let count = |n: u32| Arg::Count(i64::from(n));
        let mut parts = vec![
            self.texts
                .get_args(TextId::PasteReportTasks, &[("count", count(report.tasks))]),
        ];
        if report.assignments_dropped > 0 {
            parts.push(self.texts.get_args(
                TextId::PasteReportAssignments,
                &[("count", count(report.assignments_dropped))],
            ));
        }
        if report.tracking_dropped > 0 {
            parts.push(self.texts.get_args(
                TextId::PasteReportTracking,
                &[("count", count(report.tracking_dropped))],
            ));
        }
        if report.resources_created > 0 {
            parts.push(self.texts.get_args(
                TextId::PasteReportResources,
                &[("count", count(report.resources_created))],
            ));
        }
        let mut text = parts.join(", ");
        if report.links_dropped > 0 {
            text.push_str(". ");
            text.push_str(&self.texts.get_args(
                TextId::PasteLinksDropped,
                &[("count", count(report.links_dropped))],
            ));
        }
        self.clear_titled(TextId::PasteReportTitle);
        self.push_titled(Severity::Info, Some(TextId::PasteReportTitle), text);
    }

    /// Refus d'une copie ou d'un collage au bandeau, traduit par son code
    /// (`paste.unrecognized`, `paste.too_large`…).
    fn report_clip_error(&mut self, error: &ClipError) {
        let detail = match error {
            ClipError::Unrecognized => self.texts.get(TextId::PasteHintUnrecognized),
            ClipError::TooLarge => self.texts.get(TextId::PasteHintTooLarge),
            ClipError::UnknownTask(_) | ClipError::NotYetAvailable => error.to_string(),
        };
        let text = self.texts.get_args(
            error_text_id(error.code()),
            &[("detail", Arg::Str(detail.into()))],
        );
        // Comme un refus du registre : remplace le refus précédent (un lot
        // réussi les retire de même, `App::execute`).
        self.clear_titled(TextId::PasteReportTitle);
        self.clear_titled(TextId::CommandErrorTitle);
        self.push_titled(Severity::Error, Some(TextId::CommandErrorTitle), text);
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use super::{ClipKind, PasteDest, PasteRefusal, PasteRoute, paste_route};
    use crate::i18n::{Arg, Language, TextId, Texts};

    /// R-261 : table complète du routage (destination × dernière copie ×
    /// texte).
    #[test]
    fn r261_paste_route_table() {
        let header = "Nom\tDurée\nA\t2j";
        let plain = "7;6\n9\n10";
        let refuse = PasteRoute::Refuse(PasteRefusal::RowsOnCells);
        let cases = [
            (PasteDest::Cells, Some(ClipKind::Rows), plain, refuse),
            (PasteDest::Cells, Some(ClipKind::Cells), plain, PasteRoute::Cells),
            (PasteDest::Cells, None, plain, PasteRoute::Cells),
            (PasteDest::Cells, None, header, PasteRoute::Cells),
            (
                PasteDest::Rows,
                Some(ClipKind::Rows),
                plain,
                PasteRoute::InsertRows,
            ),
            (PasteDest::Rows, Some(ClipKind::Cells), header, PasteRoute::Cells),
            (PasteDest::Rows, None, header, PasteRoute::ImportRows),
            (PasteDest::Rows, None, "5j", PasteRoute::Cells),
            (
                PasteDest::Blank,
                Some(ClipKind::Rows),
                plain,
                PasteRoute::InsertRows,
            ),
            (PasteDest::Blank, None, header, PasteRoute::ImportRows),
            (PasteDest::Blank, None, plain, PasteRoute::Cells),
            (PasteDest::Blank, Some(ClipKind::Cells), plain, PasteRoute::Cells),
        ];
        for (dest, slot, text, want) in cases {
            assert_eq!(paste_route(dest, slot, text), want, "{dest:?} {slot:?} {text:?}");
        }
        assert_eq!(PasteRefusal::RowsOnCells.text_id(), TextId::PasteRowsOnCells);
    }

    /// Textes reformulés (H-UI-104, lot L0) : le compte rendu dit les liens
    /// non repris dans une phrase, sans l'ancien fragment « non repris » ni
    /// « un bout ».
    #[test]
    fn paste_report_links_sentence() {
        for lang in [Language::Fr, Language::En] {
            let texts = Texts::new(lang);
            let new = texts.get_args(TextId::PasteLinksDropped, &[("count", Arg::Count(2))]);
            let old = texts.get_args(TextId::PasteReportLinks, &[("count", Arg::Count(2))]);
            assert_ne!(new, old);
            assert!(!new.contains("bout"), "{new}");
            assert!(new.ends_with('.'), "{new}");
        }
    }
}
