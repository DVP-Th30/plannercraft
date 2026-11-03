//! Barre de recherche (H-UI-33, Ctrl/Cmd+F ; ADR-0016 D2). Une recherche
//! est une vue : rien n'est écrit (P50). Contrat posé par le lot L0,
//! implémenté en L9.
//!
//! La requête (au plus [`MAX_QUERY_CHARS`] caractères) est repliée par
//! `fold_text` (sans casse ni accents, comme le critère « nom contient » du
//! filtre partagé, R-251) et cherchée dans le nom des tâches, ou dans les
//! textes des colonnes visibles. Les textes repliés sont calculés une fois
//! par génération du document, langue et colonnes (cache de
//! [`SearchState`]) : une frappe ne coûte qu'une sous-chaîne par ligne.
//! Entrée ou Ctrl/Cmd+G : correspondance suivante ; Maj+Entrée ou
//! Ctrl/Cmd+Maj+G : précédente (ordre des ID, avec retour au début) ; Échap
//! ferme la barre. Les correspondances masquées par le filtre sont comptées
//! mais pas parcourues ; une correspondance repliée déplie ses ancêtres
//! (voir `App::apply_task_view`).

use plannercraft_automation::filter::fold_text;

use crate::action::Action;
use crate::column::Column;
use crate::i18n::{Arg, Language, TextId};
use crate::keymap::{Binding, UiBinding, shortcut_for_os, shortcut_text};
use crate::row_view::ModelRow;
use crate::rows::{RowKind, RowModel};

use super::FrameCtx;
use super::task_view::TaskViewOp;

/// Longueur maximale d'une requête, en caractères (règle 10 de J8).
pub const MAX_QUERY_CHARS: usize = 256;

/// Portée de la recherche.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchScope {
    /// Nom des tâches.
    #[default]
    Name,
    /// Colonnes visibles.
    VisibleColumns,
}

/// État de la barre de recherche.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchState {
    /// Requête (≤ [`MAX_QUERY_CHARS`]).
    pub query: String,
    /// Portée.
    pub scope: SearchScope,
    /// Correspondances, dans l'ordre du modèle (masquées par le filtre
    /// comprises).
    pub hits: Vec<ModelRow>,
    /// Correspondance courante (indice dans `hits`).
    pub cursor: usize,
    /// Donner le focus au champ à la prochaine image (ouverture).
    pub focus_pending: bool,
    /// Textes repliés de chaque ligne du modèle et leur clé.
    folded: Option<(FoldKey, Vec<String>)>,
}

/// Clé des textes repliés : document (ouverture, génération), langue,
/// portée et colonnes cherchées.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoldKey {
    /// Numéro d'ouverture et génération du document.
    pub document: (u64, u64),
    /// Langue des textes du `RowModel`.
    pub language: Language,
    /// Colonnes cherchées.
    pub columns: Vec<Column>,
}

impl SearchState {
    /// Barre ouverte, vide, focus demandé.
    #[must_use]
    pub fn opened() -> Self {
        Self {
            focus_pending: true,
            ..Self::default()
        }
    }

    /// Pose la requête (tronquée à [`MAX_QUERY_CHARS`] caractères) ; rend
    /// `true` si elle a changé.
    pub fn set_query(&mut self, query: &str) -> bool {
        let query = truncate_chars(query, MAX_QUERY_CHARS);
        if self.query == query {
            return false;
        }
        query.clone_into(&mut self.query);
        true
    }

    /// Recalcule les textes repliés si `key` a changé, puis les
    /// correspondances ; le curseur est borné.
    pub fn refresh(&mut self, model: &RowModel, key: FoldKey) {
        if self.folded.as_ref().is_none_or(|(k, _)| *k != key) {
            let folded = (0..model.len())
                .map(|i| fold_row(model, i, &key.columns))
                .collect();
            self.folded = Some((key, folded));
        }
        let needle = fold_text(self.query.trim());
        self.hits = match &self.folded {
            Some((_, folded)) if !needle.is_empty() => folded
                .iter()
                .enumerate()
                .filter(|(i, text)| {
                    model.row(*i).is_some_and(|row| row.kind != RowKind::Null) && text.contains(&needle)
                })
                .map(|(i, _)| ModelRow::at(i))
                .collect(),
            _ => Vec::new(),
        };
        if self.cursor >= self.hits.len() {
            self.cursor = 0;
        }
    }

    /// Clé des textes repliés courante (`None` : jamais calculés).
    #[must_use]
    pub fn fold_key(&self) -> Option<&FoldKey> {
        self.folded.as_ref().map(|(k, _)| k)
    }
}

/// Colonnes cherchées pour une portée (colonnes visibles de la table, dans
/// l'ordre d'affichage, pour [`SearchScope::VisibleColumns`]).
#[must_use]
pub fn scope_columns(scope: SearchScope, visible: impl Iterator<Item = Column>) -> Vec<Column> {
    match scope {
        SearchScope::Name => vec![Column::Name],
        SearchScope::VisibleColumns => visible.collect(),
    }
}

/// Texte replié d'une ligne : ses cellules des colonnes cherchées, séparées
/// par un caractère de contrôle (aucune correspondance à cheval sur deux
/// cellules).
fn fold_row(model: &RowModel, i: usize, columns: &[Column]) -> String {
    let mut out = String::new();
    for (k, column) in columns.iter().enumerate() {
        if k > 0 {
            out.push('\u{1F}');
        }
        out.push_str(&fold_text(model.text(i, *column)));
    }
    out
}

/// Préfixe de `s` d'au plus `max` caractères.
#[must_use]
pub fn truncate_chars(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((end, _)) => s.get(..end).unwrap_or(s),
        None => s,
    }
}

/// Ce que la barre affiche à une image (copie prise par `App`, la vue ne
/// lit jamais l'état transitoire).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchBar {
    /// Requête.
    pub query: String,
    /// Portée.
    pub scope: SearchScope,
    /// Correspondances parcourables (non masquées par le filtre).
    pub reachable: usize,
    /// Rang (1, 2…) de la correspondance courante parmi les parcourables.
    pub position: Option<usize>,
    /// Correspondances masquées par le filtre.
    pub hidden: usize,
    /// Donner le focus au champ.
    pub focus: bool,
}

/// Identifiant du champ de recherche.
#[must_use]
pub fn field_id() -> egui::Id {
    egui::Id::unique("plannercraft.search.field")
}

/// Barre de recherche (panneau sous la barre d'outils), dessinée seulement
/// quand elle est ouverte. Le champ reçoit Entrée (suivant), Maj+Entrée
/// (précédent), Ctrl/Cmd+G et Ctrl/Cmd+Maj+G, Échap (ferme) : `KEYMAP`
/// n'est pas lu pendant une saisie.
pub fn search_bar(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    let Some(bar) = fc.task_view.search.clone() else {
        return;
    };
    let texts = fc.texts;
    egui::Panel::top("plannercraft_search_bar").show(ui, |ui| {
        ui.horizontal(|ui| {
            let label = texts.get(TextId::SearchLabel);
            ui.label(&label);
            let id = field_id();
            if ui.memory(|m| m.has_focus(id)) {
                field_keys(ui, fc);
            }
            let mut query = bar.query.clone();
            let response = ui.add(
                egui::TextEdit::singleline(&mut query)
                    .id(id)
                    .char_limit(MAX_QUERY_CHARS)
                    .desired_width(240.0),
            );
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, label.clone()));
            if bar.focus {
                response.request_focus();
                fc.actions.push(Action::TaskView(TaskViewOp::SearchFocused));
            }
            if response.changed() {
                fc.actions.push(Action::TaskView(TaskViewOp::SetQuery(query)));
            }
            // egui retire le focus sur Échap avant le dessin : la barre se
            // ferme sur la perte de focus qui l'accompagne.
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                fc.actions.push(Action::TaskView(TaskViewOp::CloseSearch));
            }
            ui.label(counter_text(fc, &bar));
            let previous = shortcut_for_os(Binding::Ui(UiBinding::FindPrevious), ui.ctx().os());
            let next = shortcut_for_os(Binding::Ui(UiBinding::FindNext), ui.ctx().os());
            for (text, shortcut, op) in [
                (TextId::SearchPrevious, previous, TaskViewOp::SearchPrev),
                (TextId::SearchNext, next, TaskViewOp::SearchNext),
            ] {
                let mut button = egui::Button::new(texts.get(text));
                if let Some(shortcut) = shortcut {
                    button = button.shortcut_text(shortcut_text(ui.ctx(), shortcut));
                }
                if ui.add_enabled(bar.reachable > 0, button).clicked() {
                    fc.actions.push(Action::TaskView(op));
                }
            }
            let mut all = bar.scope == SearchScope::VisibleColumns;
            if ui
                .checkbox(&mut all, texts.get(TextId::SearchAllColumns))
                .changed()
            {
                let scope = if all {
                    SearchScope::VisibleColumns
                } else {
                    SearchScope::Name
                };
                fc.actions
                    .push(Action::TaskView(TaskViewOp::SetSearchScope(scope)));
            }
            if ui.button(texts.get(TextId::SearchClose)).clicked() {
                fc.actions.push(Action::TaskView(TaskViewOp::CloseSearch));
            }
        });
    });
}

/// Touches du champ focalisé, lues avant lui (sinon Entrée et Échap lui
/// retireraient le focus).
fn field_keys(ui: &egui::Ui, fc: &mut FrameCtx<'_>) {
    use egui::{Key, Modifiers};
    let command_shift = Modifiers::COMMAND | Modifiers::SHIFT;
    let ops = ui.input_mut(|input| {
        let mut ops = Vec::new();
        if input.consume_key(command_shift, Key::G) || input.consume_key(Modifiers::SHIFT, Key::Enter) {
            ops.push(TaskViewOp::SearchPrev);
        }
        if input.consume_key(Modifiers::COMMAND, Key::G) || input.consume_key(Modifiers::NONE, Key::Enter) {
            ops.push(TaskViewOp::SearchNext);
        }
        if input.consume_key(Modifiers::NONE, Key::Escape) {
            ops.push(TaskViewOp::CloseSearch);
        }
        ops
    });
    fc.actions.extend(ops.into_iter().map(Action::TaskView));
}

/// Compteur : « 2/5 », « 2/5 (1 masquée par le filtre) », « Aucun
/// résultat ».
fn counter_text(fc: &FrameCtx<'_>, bar: &SearchBar) -> String {
    let texts = fc.texts;
    if bar.query.trim().is_empty() {
        return String::new();
    }
    let count = |n: usize| Arg::Count(i64::try_from(n).unwrap_or(i64::MAX));
    let main = if bar.reachable == 0 {
        texts.get(TextId::SearchNone)
    } else {
        texts.get_args(
            TextId::SearchCounter,
            &[
                ("id", count(bar.position.unwrap_or(0))),
                ("count", count(bar.reachable)),
            ],
        )
    };
    if bar.hidden == 0 {
        main
    } else {
        let hidden = texts.get_args(TextId::SearchHidden, &[("count", count(bar.hidden))]);
        format!("{main} {hidden}")
    }
}

/// Entrée « Rechercher… » du menu Édition (Ctrl/Cmd+F).
pub fn edit_menu_entries(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    let mut button = egui::Button::new(fc.texts.get(TextId::MenuFind));
    if let Some(shortcut) = shortcut_for_os(Binding::Ui(UiBinding::Find), ui.ctx().os()) {
        button = button.shortcut_text(shortcut_text(ui.ctx(), shortcut));
    }
    if ui.add_enabled(fc.doc.is_some(), button).clicked() {
        fc.actions.push(Action::TaskView(TaskViewOp::OpenSearch));
    }
}

#[cfg(test)]
mod tests {
    use super::truncate_chars;

    #[test]
    fn query_truncated_on_char_boundary() {
        assert_eq!(truncate_chars("éèà", 2), "éè");
        assert_eq!(truncate_chars("abc", 5), "abc");
        assert_eq!(truncate_chars("", 0), "");
        let long = "é".repeat(300);
        assert_eq!(truncate_chars(&long, super::MAX_QUERY_CHARS).chars().count(), 256);
    }
}
