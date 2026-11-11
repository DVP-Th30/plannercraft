//! Commandes de la vue des tâches (J8, lot L9) : filtres, tri, repli et
//! recherche dans l'interface (H-UI-30 à H-UI-34, ADR-0016 D2).
//!
//! Toutes sont des vues : ni génération du document, ni octets enregistrés
//! ne changent (P50). Les vues poussent `Action::TaskView` ; le réducteur
//! d'`App` appelle [`App::apply_task_view`]. Menus (Affichage → Filtre,
//! Trier…, Niveau hiérarchique ; Édition → Rechercher…), indicateur de la
//! barre d'état, fenêtres « Trier » et « Filtre » (hébergées ici, hors des
//! fenêtres du document) et triangles de repli (dessinés par la table
//! derrière `App::outline_triangles`, faux jusqu'au lot L15).
//!
//! Repli : « Afficher jusqu'au niveau n » est gardé dans les préférences
//! (`UiState::task_view`) ; le repli à la main (`collapsed`) ne l'est pas.
//! Déplier une récapitulative masquée par le niveau (triangle, Alt+Maj++,
//! correspondance de recherche) convertit le niveau en repli explicite :
//! toutes les récapitulatives de niveau ≥ n repliées, niveau retiré (même
//! affichage), puis la récapitulative visée dépliée.

use std::collections::BTreeSet;

use plannercraft_core::TaskUid;

use crate::action::Action;
use crate::app::App;
use crate::dialog::filter::{self, FilterDialog, FilterDialogMode, FilterOutcome};
use crate::dialog::sort::{self, SortDialog, SortOutcome};
use crate::geom::index_to_f32;
use crate::i18n::{Arg, TextId};
use crate::row_view::ModelRow;
use crate::rows::{RowKind, RowModel};
use crate::selection::Selection;
use crate::state::{FilterChoice, MAX_OUTLINE_LEVEL, SortSpec, TaskViewState};
use crate::structure::StructureOp;
use plannercraft_automation::filter::FilterPreset;

use super::FrameCtx;
use super::search::{self, FoldKey, SearchBar, SearchScope, SearchState};

/// Commande de la vue des tâches (aucune écriture dans le document).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskViewOp {
    /// Applique un filtre (le même filtre choisi à nouveau est réappliqué).
    SetFilter(FilterChoice),
    /// Réapplique le filtre courant (H-UI-30 : les lignes gardées
    /// disparaissent si elles ne le satisfont plus).
    ReapplyFilter,
    /// F3 : retire le filtre (S79).
    ClearFilter,
    /// Pose un tri (`None` : ordre des ID).
    SetSort(Option<SortSpec>),
    /// Maj+F3 : ordre des ID (S75).
    ResetSort,
    /// « Afficher jusqu'au niveau n » (`None` : tous), repli à la main
    /// retiré ; gardé dans les préférences.
    SetOutlineLevel(Option<u8>),
    /// Triangle de repli d'une récapitulative.
    ToggleCollapsed(TaskUid),
    /// Alt+Maj+− : masque les sous-tâches de la sélection.
    HideSubtasks,
    /// Alt+Maj++ (ou =) : affiche les sous-tâches de la sélection.
    ShowSubtasks,
    /// Ctrl/Cmd+F : ouvre la barre de recherche (ou y remet le focus).
    OpenSearch,
    /// Échap : ferme la barre de recherche.
    CloseSearch,
    /// Le champ de recherche a reçu le focus demandé.
    SearchFocused,
    /// Nouvelle requête (tronquée à 256 caractères).
    SetQuery(String),
    /// Portée de la recherche.
    SetSearchScope(SearchScope),
    /// Entrée, Ctrl/Cmd+G : correspondance suivante.
    SearchNext,
    /// Maj+Entrée, Ctrl/Cmd+Maj+G : correspondance précédente.
    SearchPrev,
    /// Ouvre la fenêtre « Trier ».
    OpenSortDialog,
    /// Ouvre la fenêtre « Filtre » sous la forme donnée.
    OpenFilterDialog(FilterDialogMode),
}

/// Ce que les vues lisent de la vue des tâches à une image (copie prise
/// par `App` avant le dessin : la vue transitoire est empruntée ailleurs).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TaskViewFrame {
    /// Filtre, tri, niveau.
    pub state: TaskViewState,
    /// Barre de recherche ouverte.
    pub search: Option<SearchBar>,
    /// Triangles de repli dessinés (H-UI-34 ; faux jusqu'au lot L15).
    pub outline_triangles: bool,
    /// Récapitulatives repliées (copiées seulement avec les triangles).
    pub collapsed: BTreeSet<TaskUid>,
    /// Style du planning à l'écran (J8, lot L11) : style effectif du
    /// document et variante du thème (Classique clair par défaut).
    pub style: crate::theme::ScreenStyle,
    /// Pinceau « Reproduire la mise en forme » armé (finitions P2,
    /// H-UI-128) : bouton enfoncé.
    pub painter: bool,
    /// Regroupement de la vue (finitions P3, H-UI-129).
    pub group_by: Option<plannercraft_render::style::GroupBy>,
    /// Ligne affichée du curseur (tâche répétée sous plusieurs groupes).
    pub cursor_row: Option<crate::row_view::DisplayRow>,
}

impl TaskViewFrame {
    /// La récapitulative `uid` de niveau `level` est-elle repliée (à la
    /// main, ou par le niveau affiché) ?
    #[must_use]
    pub fn is_folded(&self, uid: TaskUid, level: u16) -> bool {
        self.collapsed.contains(&uid)
            || self
                .state
                .outline_level
                .is_some_and(|max| level >= u16::from(max))
    }
}

/// Opération de structure qui déplace des lignes (grisée sous tri ou
/// filtre, H-UI-32) : indenter, désindenter, monter, descendre.
#[must_use]
pub const fn reorders(op: StructureOp) -> bool {
    matches!(
        op,
        StructureOp::Indent | StructureOp::Outdent | StructureOp::MoveUp | StructureOp::MoveDown
    )
}

/// Indices des ancêtres de la ligne `i` du modèle (du plus proche au plus
/// lointain) : lignes précédentes de niveau strictement décroissant.
fn ancestors(model: &RowModel, i: usize) -> Vec<usize> {
    let Some(mut level) = model.row(i).map(|row| row.level) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for j in (0..i).rev() {
        if level <= 1 {
            break;
        }
        if let Some(row) = model.row(j)
            && row.level < level
        {
            out.push(j);
            level = row.level;
        }
    }
    out
}

impl App {
    /// Applique une commande de la vue des tâches ; rien n'est écrit dans
    /// le document (P50).
    pub fn apply_task_view(&mut self, op: TaskViewOp) {
        match op {
            TaskViewOp::SetFilter(choice) => self.apply_filter(choice),
            TaskViewOp::ReapplyFilter => {
                let choice = self.view.task_view.filter.clone();
                self.apply_filter(choice);
            }
            TaskViewOp::ClearFilter => self.apply_filter(FilterChoice::Preset(FilterPreset::All)),
            TaskViewOp::SetSort(sort) => {
                let state = TaskViewState {
                    sort,
                    ..self.view.task_view.clone()
                };
                self.set_task_view(state);
            }
            TaskViewOp::ResetSort => self.apply_task_view(TaskViewOp::SetSort(None)),
            TaskViewOp::SetOutlineLevel(level) => self.set_outline_level(level),
            TaskViewOp::ToggleCollapsed(uid) => self.toggle_fold(uid),
            TaskViewOp::HideSubtasks => self.hide_subtasks(),
            TaskViewOp::ShowSubtasks => self.show_subtasks(),
            TaskViewOp::OpenSearch => {
                if self.document.is_some() {
                    match &mut self.view.search {
                        Some(search) => search.focus_pending = true,
                        None => self.view.search = Some(SearchState::opened()),
                    }
                }
            }
            TaskViewOp::CloseSearch => {
                if self.view.search.take().is_some() && self.document.is_some() {
                    self.view.focus_table_pending = true;
                }
            }
            TaskViewOp::SearchFocused => {
                if let Some(search) = &mut self.view.search {
                    search.focus_pending = false;
                }
            }
            TaskViewOp::SetQuery(query) => {
                let changed = self.view.search.as_mut().is_some_and(|s| s.set_query(&query));
                if changed {
                    self.refresh_search();
                    self.search_from_selection();
                }
            }
            TaskViewOp::SetSearchScope(scope) => {
                if let Some(search) = &mut self.view.search
                    && search.scope != scope
                {
                    search.scope = scope;
                    self.refresh_search();
                    self.search_from_selection();
                }
            }
            TaskViewOp::SearchNext => self.search_step(true),
            TaskViewOp::SearchPrev => self.search_step(false),
            TaskViewOp::OpenSortDialog => {
                if self.document.is_some() {
                    self.view.filter_dialog = None;
                    self.view.sort_dialog = Some(SortDialog::open(self.view.task_view.sort));
                }
            }
            TaskViewOp::OpenFilterDialog(mode) => {
                let lang = self.text_language();
                if let Some(document) = &self.document {
                    self.view.sort_dialog = None;
                    self.view.filter_dialog = Some(FilterDialog::open(
                        document.project(),
                        lang,
                        mode,
                        &self.view.task_view.filter,
                    ));
                }
            }
        }
    }

    /// Triangles de repli dessinés dans la colonne Nom (H-UI-34) : vrais
    /// depuis le lot L15.
    #[must_use]
    pub const fn outline_triangles(&self) -> bool {
        self.outline_triangles
    }

    /// Montre ou masque les triangles de repli (tests, lot L15).
    pub const fn set_outline_triangles(&mut self, on: bool) {
        self.outline_triangles = on;
    }

    /// Barre de recherche ouverte et son état.
    #[must_use]
    pub const fn search(&self) -> Option<&SearchState> {
        self.view.search.as_ref()
    }

    /// Fenêtre « Trier » ouverte.
    #[must_use]
    pub const fn sort_dialog(&self) -> Option<&SortDialog> {
        self.view.sort_dialog.as_ref()
    }

    /// Fenêtre « Filtre » ouverte.
    #[must_use]
    pub const fn filter_dialog(&self) -> Option<&FilterDialog> {
        self.view.filter_dialog.as_ref()
    }

    /// Une fenêtre de la vue des tâches est ouverte.
    #[must_use]
    pub const fn view_dialog_open(&self) -> bool {
        self.view.sort_dialog.is_some() || self.view.filter_dialog.is_some()
    }

    /// L'opération de structure `op` est refusée par la vue (H-UI-32 :
    /// déplacer, hausser, abaisser sous tri ou filtre).
    #[must_use]
    pub fn structure_locked(&self, op: StructureOp) -> bool {
        (reorders(op) && self.view.task_view.reorder_locked())
            || (self.view.group_by.is_some() && crate::group_view::group_locks(op))
    }

    /// Raison d'un refus de structure par la vue : regroupement (finitions
    /// P3, H-UI-130) d'abord, sinon tri ou filtre.
    #[must_use]
    pub const fn structure_lock_text(&self) -> TextId {
        if self.view.group_by.is_some() {
            TextId::ViewGroupLocked
        } else {
            TextId::ViewReorderLocked
        }
    }

    /// Applique (ou réapplique) un filtre : lignes gardées oubliées
    /// (H-UI-30) ; un filtre personnalisé vide vaut « Toutes les tâches ».
    fn apply_filter(&mut self, choice: FilterChoice) {
        let choice = match choice {
            FilterChoice::Custom(filter) if filter.is_empty() => FilterChoice::Preset(FilterPreset::All),
            other => other,
        };
        self.view.reapply_version = self.view.reapply_version.wrapping_add(1);
        let state = TaskViewState {
            filter: choice,
            ..self.view.task_view.clone()
        };
        self.set_task_view(state);
        self.ensure_row_view();
        self.sync_selection();
    }

    /// « Afficher jusqu'au niveau n » : repli à la main retiré, niveau
    /// gardé dans les préférences.
    fn set_outline_level(&mut self, level: Option<u8>) {
        let level = level.map(|l| l.clamp(1, MAX_OUTLINE_LEVEL));
        self.state.task_view.outline_level = level;
        if !self.view.collapsed.is_empty() {
            self.view.collapsed.clear();
            self.bump_collapse();
        }
        let state = TaskViewState {
            outline_level: level,
            ..self.view.task_view.clone()
        };
        self.set_task_view(state);
        self.ensure_row_view();
        self.sync_selection();
    }

    /// Version du repli changée (clé du cache de la vue des lignes).
    fn bump_collapse(&mut self) {
        self.view.collapse_version = self.view.collapse_version.wrapping_add(1);
    }

    /// Convertit le niveau affiché en repli explicite (même affichage) :
    /// toutes les récapitulatives de niveau ≥ n repliées, niveau retiré de
    /// la vue (les préférences le gardent pour la prochaine ouverture).
    fn materialize_level(&mut self) {
        let (Some(level), Some(model)) = (self.view.task_view.outline_level, &self.rows) else {
            return;
        };
        let level = u16::from(level);
        let folded: Vec<TaskUid> = (0..model.len())
            .filter_map(|i| model.row(i))
            .filter(|row| row.kind == RowKind::Summary && row.level >= level)
            .map(|row| row.uid)
            .collect();
        self.view.collapsed.extend(folded);
        self.view.task_view.outline_level = None;
        self.bump_collapse();
    }

    /// Le niveau affiché masque-t-il les enfants de la ligne de niveau
    /// `level` ?
    fn level_hides_children(&self, level: u16) -> bool {
        self.view
            .task_view
            .outline_level
            .is_some_and(|max| level >= u16::from(max))
    }

    /// Triangle : replie ou déplie la récapitulative `uid`.
    fn toggle_fold(&mut self, uid: TaskUid) {
        let Some(row) = self
            .rows
            .as_ref()
            .and_then(|m| m.index_of(uid).and_then(|i| m.row(i)))
        else {
            return;
        };
        if row.kind != RowKind::Summary {
            return;
        }
        if self.level_hides_children(row.level) {
            self.materialize_level();
        }
        let folded = self.view.collapsed.contains(&uid);
        self.set_collapsed(uid, !folded);
        self.ensure_row_view();
        self.sync_selection();
    }

    /// Tâches visées par Alt+Maj+−/+ : la sélection, ou la tâche active.
    fn targets(&self) -> Vec<TaskUid> {
        let mut uids: Vec<TaskUid> = self.selected().uids().iter().copied().collect();
        if uids.is_empty() {
            uids.extend(self.selection);
        }
        uids
    }

    /// Alt+Maj+− : chaque récapitulative sélectionnée est repliée ; une
    /// sous-tâche sélectionnée replie sa récapitulative, qui devient la
    /// tâche active (hypothèse, S61 « Hide subtasks. »).
    fn hide_subtasks(&mut self) {
        let Some(model) = &self.rows else {
            return;
        };
        let mut fold = Vec::new();
        let mut parent_of_active = None;
        for uid in self.targets() {
            let Some(i) = model.index_of(uid) else {
                continue;
            };
            match model.row(i) {
                Some(row) if row.kind == RowKind::Summary => fold.push(uid),
                Some(_) => {
                    if let Some(parent) = ancestors(model, i).first().and_then(|&p| model.row(p)) {
                        fold.push(parent.uid);
                        if self.selection == Some(uid) {
                            parent_of_active = Some(parent.uid);
                        }
                    }
                }
                None => {}
            }
        }
        if fold.is_empty() {
            return;
        }
        if let Some(parent) = parent_of_active {
            self.selection = Some(parent);
            self.selected = Selection::single(Some(parent));
        }
        let mut changed = false;
        for uid in fold {
            changed |= self.view.collapsed.insert(uid);
        }
        if changed {
            self.bump_collapse();
        }
        self.ensure_row_view();
        self.sync_selection();
    }

    /// Alt+Maj++ : chaque récapitulative sélectionnée est dépliée (niveau
    /// affiché converti en repli explicite au besoin).
    fn show_subtasks(&mut self) {
        let Some(model) = &self.rows else {
            return;
        };
        let summaries: Vec<(TaskUid, u16)> = self
            .targets()
            .into_iter()
            .filter_map(|uid| model.index_of(uid).and_then(|i| model.row(i)))
            .filter(|row| row.kind == RowKind::Summary)
            .map(|row| (row.uid, row.level))
            .collect();
        if summaries.is_empty() {
            return;
        }
        if summaries
            .iter()
            .any(|&(_, level)| self.level_hides_children(level))
        {
            self.materialize_level();
        }
        let mut changed = false;
        for (uid, _) in summaries {
            changed |= self.view.collapsed.remove(&uid);
        }
        if changed {
            self.bump_collapse();
        }
        self.ensure_row_view();
        self.sync_selection();
    }

    /// Recalcule les correspondances de la recherche si le document, la
    /// langue, la portée ou les colonnes ont changé (et après une nouvelle
    /// requête).
    fn refresh_search(&mut self) {
        let (Some(document), Some(model), Some(search)) = (&self.document, &self.rows, &mut self.view.search)
        else {
            return;
        };
        let key = FoldKey {
            document: document.cache_key(),
            language: self.texts.language(),
            columns: search::scope_columns(search.scope, self.state.visible_columns().map(|c| c.column)),
        };
        search.refresh(model, key);
    }

    /// La correspondance `k` (indice dans `hits`) est-elle parcourable (non
    /// masquée par le filtre) ?
    fn reachable(&self, k: usize) -> bool {
        let (Some(search), Some(model)) = (&self.view.search, &self.rows) else {
            return false;
        };
        search
            .hits
            .get(k)
            .and_then(|m| model.row(m.index()))
            .is_some_and(|row| !self.view.filter_memo.hides(row.uid))
    }

    /// Après une nouvelle requête : première correspondance parcourable à
    /// partir de la tâche active (comprise), avec retour au début.
    fn search_from_selection(&mut self) {
        let (Some(search), Some(model)) = (&self.view.search, &self.rows) else {
            return;
        };
        let from = self.selection.and_then(|uid| model.index_of(uid)).unwrap_or(0);
        let n = search.hits.len();
        let start = search.hits.partition_point(|m| m.index() < from);
        let found = (0..n)
            .map(|k| start.saturating_add(k).checked_rem(n).unwrap_or(0))
            .find(|&k| self.reachable(k));
        if let Some(k) = found {
            self.go_to_hit(k);
        }
    }

    /// Entrée / Maj+Entrée : correspondance parcourable suivante ou
    /// précédente, avec retour au début ; barre fermée : ouverte.
    fn search_step(&mut self, forward: bool) {
        if self.view.search.is_none() {
            self.apply_task_view(TaskViewOp::OpenSearch);
            return;
        }
        self.refresh_search();
        let Some(search) = &self.view.search else {
            return;
        };
        let n = search.hits.len();
        if n == 0 {
            return;
        }
        let here = search.cursor.min(n.saturating_sub(1));
        let current_shown = self
            .selection
            .zip(search.hits.get(here))
            .zip(self.rows.as_ref())
            .is_some_and(|((uid, m), model)| model.row(m.index()).is_some_and(|row| row.uid == uid));
        let found = (0..n)
            .map(|k| {
                // Sans correspondance courante sous la sélection : on part
                // de la correspondance courante elle-même.
                let step = if current_shown { k.saturating_add(1) } else { k };
                if forward {
                    here.saturating_add(step).checked_rem(n).unwrap_or(0)
                } else {
                    here.saturating_add(n.saturating_mul(2))
                        .saturating_sub(step)
                        .checked_rem(n)
                        .unwrap_or(0)
                }
            })
            .find(|&k| self.reachable(k));
        if let Some(k) = found {
            self.go_to_hit(k);
        }
    }

    /// Sélectionne la correspondance `k`, déplie ses ancêtres repliés et
    /// la fait défiler à l'écran si elle n'y est pas.
    fn go_to_hit(&mut self, k: usize) {
        let Some(search) = &mut self.view.search else {
            return;
        };
        search.cursor = k;
        let Some(i) = search.hits.get(k).map(|m| m.index()) else {
            return;
        };
        let Some(model) = &self.rows else {
            return;
        };
        let Some(uid) = model.row(i).map(|row| row.uid) else {
            return;
        };
        let chain: Vec<(TaskUid, u16)> = ancestors(model, i)
            .into_iter()
            .filter_map(|j| model.row(j))
            .map(|row| (row.uid, row.level))
            .collect();
        if chain.iter().any(|&(_, level)| self.level_hides_children(level)) {
            self.materialize_level();
        }
        let mut changed = false;
        for (ancestor, _) in &chain {
            changed |= self.view.collapsed.remove(ancestor);
        }
        if changed {
            self.bump_collapse();
        }
        self.selection = Some(uid);
        self.selected = Selection::single(Some(uid));
        self.ensure_row_view();
        self.sync_selection();
        self.scroll_into_view(uid);
    }

    /// Fait défiler la table et le Gantt pour montrer la tâche `uid` si
    /// elle est hors de la fenêtre visible de l'image précédente.
    fn scroll_into_view(&mut self, uid: TaskUid) {
        let Some(d) = self.visible_rows().and_then(|rows| rows.display_of(uid)) else {
            return;
        };
        let row_h = self.row_h();
        let top = index_to_f32(d.index()) * row_h;
        let view = &self.view;
        let (scroll, height) = (view.last_scroll_y, view.last_body_height);
        let visible = scroll.is_finite()
            && height.is_finite()
            && height > row_h
            && top >= scroll
            && top + row_h <= scroll + height;
        if !visible {
            let third = if height.is_finite() { height / 3.0 } else { 0.0 };
            self.view.pending_scroll_y = Some((top - third).max(0.0));
        }
    }

    /// Copie de la vue des tâches pour les vues de l'image ; recalcule au
    /// passage les correspondances de la recherche si le document a changé.
    pub(crate) fn task_view_frame(&mut self) -> TaskViewFrame {
        if self.view.search.is_some() {
            let stale = match (&self.document, &self.view.search) {
                (Some(document), Some(search)) => search.fold_key().is_none_or(|key| {
                    key.document != document.cache_key() || key.language != self.texts.language()
                }),
                _ => false,
            };
            if stale {
                self.refresh_search();
            }
        }
        let search = self.view.search.as_ref().map(|search| {
            let reachable: Vec<usize> = (0..search.hits.len()).filter(|&k| self.reachable(k)).collect();
            let position = reachable
                .iter()
                .position(|&k| k == search.cursor)
                .map(|p| p.saturating_add(1));
            SearchBar {
                query: search.query.clone(),
                scope: search.scope,
                reachable: reachable.len(),
                position,
                hidden: search.hits.len().saturating_sub(reachable.len()),
                focus: search.focus_pending,
            }
        });
        TaskViewFrame {
            state: self.view.task_view.clone(),
            search,
            outline_triangles: self.outline_triangles,
            collapsed: if self.outline_triangles {
                self.view.collapsed.clone()
            } else {
                BTreeSet::new()
            },
            // Posé par `App::show` (style effectif de l'image).
            style: crate::theme::ScreenStyle::default(),
            painter: self.view.painter.is_some(),
            group_by: self.view.group_by,
            cursor_row: self.view.cursor_row,
        }
    }

    /// Dessine la fenêtre « Trier » ou « Filtre » ouverte et applique sa
    /// réponse.
    pub(crate) fn show_view_dialog(&mut self, ctx: &egui::Context) {
        if let Some(state) = &mut self.view.sort_dialog {
            match sort::show(ctx, &self.texts, state) {
                SortOutcome::Open => {}
                SortOutcome::Cancel => self.close_view_dialog(),
                SortOutcome::Apply(spec) => {
                    self.close_view_dialog();
                    self.apply_task_view(TaskViewOp::SetSort(spec));
                }
            }
            return;
        }
        let lang = self.text_language();
        let (Some(document), Some(state)) = (&self.document, &mut self.view.filter_dialog) else {
            self.view.filter_dialog = None;
            return;
        };
        match filter::show(ctx, &self.texts, state) {
            FilterOutcome::Open => {}
            FilterOutcome::Cancel => self.close_view_dialog(),
            FilterOutcome::Apply => match state.to_choice(document.project(), &self.texts, lang) {
                Ok(choice) => {
                    self.close_view_dialog();
                    self.apply_task_view(TaskViewOp::SetFilter(choice));
                }
                Err(message) => state.error = Some(message),
            },
        }
    }

    /// Ferme la fenêtre de la vue des tâches ; la table reprend le focus.
    fn close_view_dialog(&mut self) {
        let was_open = self.view_dialog_open();
        self.view.sort_dialog = None;
        self.view.filter_dialog = None;
        if was_open && self.document.is_some() {
            self.view.focus_table_pending = true;
        }
    }
}

/// Filtres prédéfinis sans paramètre du menu, avec leur libellé.
pub(crate) const PRESETS: [(FilterPreset, TextId); 6] = [
    (FilterPreset::All, TextId::FilterAll),
    (FilterPreset::Critical, TextId::FilterCritical),
    (FilterPreset::Late, TextId::FilterLate),
    (FilterPreset::Incomplete, TextId::FilterIncomplete),
    (FilterPreset::Milestones, TextId::FilterMilestones),
    (FilterPreset::Summaries, TextId::FilterSummaries),
];

/// Libellé du filtre courant pour la barre d'état (`None` : aucun).
#[must_use]
pub fn filter_label(state: &TaskViewState, texts: &crate::i18n::Texts) -> Option<String> {
    if !state.is_filtered() {
        return None;
    }
    let name = match &state.filter {
        FilterChoice::Preset(FilterPreset::UsingResource(name)) => texts.get_args(
            TextId::FilterUsingNamed,
            &[("name", Arg::Str(name.as_str().into()))],
        ),
        FilterChoice::Preset(FilterPreset::DateRange { .. }) => texts.get(TextId::FilterDateRangeTitle),
        FilterChoice::Preset(preset) => PRESETS
            .iter()
            .find(|(p, _)| p == preset)
            .map_or_else(String::new, |(_, label)| texts.get(*label)),
        FilterChoice::Custom(_) => texts.get(TextId::FilterCustomTitle),
    };
    Some(texts.get_args(TextId::StatusFilter, &[("name", Arg::Str(name.into()))]))
}

/// Indicateur « Filtre : Critiques » de la barre d'état (rien sans
/// filtre : les captures J7 ne changent pas) ; un clic retire le filtre.
pub fn status_indicator(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    let Some(text) = filter_label(&fc.task_view.state, fc.texts) else {
        return;
    };
    ui.separator();
    let hint = fc.texts.get(TextId::FilterClear);
    if ui.selectable_label(true, text).on_hover_text(hint).clicked() {
        fc.actions.push(Action::TaskView(TaskViewOp::ClearFilter));
    }
}

/// Entrées du menu Affichage : Filtre (prédéfinis, ressource, dates,
/// personnalisé, réappliquer, retirer), Trier…, Trier par ID, Niveau
/// hiérarchique, masquer et afficher les sous-tâches. Grisées sans
/// document. Lot U3 : entrées décrites une fois par
/// `chrome::list_choices` (mêmes que les listes du ruban et la palette).
pub fn view_menu(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    use super::chrome::{ChoiceLook, draw_choices, list_choices};
    use crate::ribbon::spec::RibbonCmd;
    let texts = fc.texts;
    ui.separator();
    ui.menu_button(texts.get(TextId::MenuFilter), |ui| {
        draw_choices(ui, fc, &list_choices(fc, RibbonCmd::FilterMenu));
    });
    draw_choices(ui, fc, &list_choices(fc, RibbonCmd::SortMenu));
    let (levels, mut subtasks): (Vec<_>, Vec<_>) = list_choices(fc, RibbonCmd::OutlineMenu)
        .into_iter()
        .partition(|entry| entry.look == ChoiceLook::Selectable);
    ui.menu_button(texts.get(TextId::MenuOutlineLevel), |ui| {
        draw_choices(ui, fc, &levels);
    });
    for entry in &mut subtasks {
        entry.sep_before = false;
    }
    draw_choices(ui, fc, &subtasks);
}

/// Ligne du modèle de la correspondance courante (tests).
#[must_use]
pub fn current_hit(search: &SearchState) -> Option<ModelRow> {
    search.hits.get(search.cursor).copied()
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        reason = "tests : un échec doit s'arrêter net et lisiblement"
    )]

    use super::{TaskViewFrame, reorders};
    use crate::structure::StructureOp;
    use plannercraft_core::TaskUid;

    #[test]
    fn only_moving_ops_are_locked() {
        for op in [
            StructureOp::Indent,
            StructureOp::Outdent,
            StructureOp::MoveUp,
            StructureOp::MoveDown,
        ] {
            assert!(reorders(op));
        }
        for op in [
            StructureOp::InsertBefore,
            StructureOp::Delete,
            StructureOp::Link,
            StructureOp::Unlink,
        ] {
            assert!(!reorders(op));
        }
    }

    #[test]
    fn folded_by_level_or_by_hand() {
        let mut frame = TaskViewFrame::default();
        let uid = TaskUid::new(5);
        assert!(!frame.is_folded(uid, 1));
        frame.state.outline_level = Some(1);
        assert!(frame.is_folded(uid, 1));
        assert!(!frame.is_folded(uid, 0));
        frame.state.outline_level = None;
        frame.collapsed.insert(uid);
        assert!(frame.is_folded(uid, 3));
    }
}
