//! Barre de recherche (J8, lot L9, H-UI-33) : Ctrl/Cmd+F, suivant et
//! précédent, Échap ; correspondance repliée dépliée ; correspondances
//! masquées par le filtre comptées ; requête bornée à 256 caractères.

use egui::{Event, Key, Modifiers};
use egui_kittest::kittest::Queryable as _;
use plannercraft_automation::filter::FilterPreset;
use plannercraft_core::TaskUid;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::state::FilterChoice;
use plannercraft_ui_egui::view::search::MAX_QUERY_CHARS;
use plannercraft_ui_egui::view::task_view::TaskViewOp;

use crate::filters::{harness_p, shown, view_op};
use crate::fr;

fn counter(position: i64, count: i64) -> String {
    Texts::new(Language::Fr).get_args(
        TextId::SearchCounter,
        &[("id", Arg::Count(position)), ("count", Arg::Count(count))],
    )
}

fn selected(harness: &egui_kittest::Harness<'static, plannercraft_ui_egui::App>) -> Option<u32> {
    harness.state().selection.map(TaskUid::get)
}

/// H-UI-33 : Ctrl+F ouvre la barre, « peint » sélectionne 7 « Peinture »
/// (« 1/1 ») ; « e » : Entrée suivant, Maj+Entrée précédent, Ctrl+G et
/// Ctrl+Maj+G aussi ; Échap ferme la barre et rend le focus à la table.
#[test]
fn h_ui_33_ctrl_f_find_next_prev() {
    let mut harness = harness_p();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::F);
    harness.run();
    assert!(harness.state().search().is_some(), "barre ouverte");
    assert!(
        harness
            .ctx
            .memory(|m| m.has_focus(plannercraft_ui_egui::view::search::field_id())),
        "le champ doit avoir le focus"
    );
    harness.event(Event::Text("peint".to_owned()));
    harness.run();
    assert_eq!(harness.state().search().unwrap().query, "peint");
    assert_eq!(selected(&harness), Some(7));
    assert!(harness.query_by_label(&counter(1, 1)).is_some(), "compteur 1/1");
    // Nouvelle requête « r » : 1 Gros œuvre, 4 Livraison, 5 Second
    // œuvre, 6 Plâtrerie, 7 Peinture ; « Murs » aussi (3).
    view_op(&mut harness, TaskViewOp::SetQuery("r".to_owned()));
    let hits = harness.state().search().unwrap().hits.len();
    assert_eq!(hits, 6);
    // La sélection (7) correspond encore : elle reste.
    assert_eq!(selected(&harness), Some(7));
    harness.key_press(Key::Enter);
    harness.run();
    assert_eq!(selected(&harness), Some(1), "suivant avec retour au début");
    harness.key_press(Key::Enter);
    harness.run();
    assert_eq!(selected(&harness), Some(3));
    harness.key_press_modifiers(Modifiers::SHIFT, Key::Enter);
    harness.run();
    assert_eq!(selected(&harness), Some(1));
    harness.key_press_modifiers(Modifiers::COMMAND, Key::G);
    harness.run();
    assert_eq!(selected(&harness), Some(3));
    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::G);
    harness.run();
    assert_eq!(selected(&harness), Some(1));
    assert!(harness.query_by_label(&counter(1, 6)).is_some(), "compteur 1/6");
    harness.key_press(Key::Escape);
    harness.run();
    assert!(harness.state().search().is_none(), "Échap ferme la barre");
    // Rien n'a été écrit.
    assert_eq!(harness.state().document.as_ref().unwrap().generation(), 0);
}

/// H-UI-33 : une correspondance sous une récapitulative repliée (à la main
/// ou par le niveau affiché) déplie ses ancêtres.
#[test]
fn h_ui_33_match_in_collapsed_expands() {
    let mut harness = harness_p();
    view_op(&mut harness, TaskViewOp::ToggleCollapsed(TaskUid::new(5)));
    assert_eq!(shown(harness.state()), [1, 2, 3, 4, 5]);
    view_op(&mut harness, TaskViewOp::OpenSearch);
    view_op(&mut harness, TaskViewOp::SetQuery("peint".to_owned()));
    assert_eq!(selected(&harness), Some(7));
    assert_eq!(shown(harness.state()), [1, 2, 3, 4, 5, 6, 7]);
    // Niveau 1 : seules 1 et 5 ; « fond » déplie 1, laisse 5 repliée.
    view_op(&mut harness, TaskViewOp::SetOutlineLevel(Some(1)));
    assert_eq!(shown(harness.state()), [1, 5]);
    view_op(&mut harness, TaskViewOp::SetQuery("fond".to_owned()));
    assert_eq!(selected(&harness), Some(2));
    assert_eq!(shown(harness.state()), [1, 2, 3, 4, 5]);
}

/// H-UI-33 : sous « Tâches critiques », « livr » (4, non critique) est
/// comptée « masquée par le filtre », jamais sélectionnée.
#[test]
fn h_ui_33_hidden_by_filter_counted() {
    let mut harness = harness_p();
    view_op(
        &mut harness,
        TaskViewOp::SetFilter(FilterChoice::Preset(FilterPreset::Critical)),
    );
    let before = selected(&harness);
    view_op(&mut harness, TaskViewOp::OpenSearch);
    view_op(&mut harness, TaskViewOp::SetQuery("livr".to_owned()));
    let search = harness.state().search().unwrap();
    assert_eq!(search.hits.len(), 1);
    assert_eq!(selected(&harness), before);
    let hidden = Texts::new(Language::Fr).get_args(TextId::SearchHidden, &[("count", Arg::Count(1))]);
    let label = format!("{} {hidden}", fr(TextId::SearchNone));
    assert!(harness.query_by_label(&label).is_some(), "{label} absent");
    view_op(&mut harness, TaskViewOp::SearchNext);
    assert_eq!(selected(&harness), before);
    // « r » sous le filtre : 1, 3, 5, 6, 7 parcourables, 4 masquée.
    view_op(&mut harness, TaskViewOp::SetQuery("r".to_owned()));
    let label = format!(
        "{} {}",
        counter(1, 5),
        Texts::new(Language::Fr).get_args(TextId::SearchHidden, &[("count", Arg::Count(1))])
    );
    let found = harness.query_by_label(&label).is_some();
    let search = harness.state().search().unwrap();
    assert!(found, "{label} absent ; {search:?}");
}

/// Règle 10 de J8 : requête bornée à 256 caractères, sans panique ;
/// recherche dans les colonnes visibles.
#[test]
fn search_query_capped_and_visible_columns() {
    let mut harness = harness_p();
    view_op(&mut harness, TaskViewOp::OpenSearch);
    view_op(&mut harness, TaskViewOp::SetQuery("é".repeat(1_000)));
    let search = harness.state().search().unwrap();
    assert_eq!(search.query.chars().count(), MAX_QUERY_CHARS);
    assert_eq!(search.hits.len(), 0);
    // « 10 jours » n'est pas dans un nom : seulement dans la durée de 3.
    view_op(&mut harness, TaskViewOp::SetQuery("10 j".to_owned()));
    assert_eq!(harness.state().search().unwrap().hits.len(), 0);
    view_op(
        &mut harness,
        TaskViewOp::SetSearchScope(plannercraft_ui_egui::view::search::SearchScope::VisibleColumns),
    );
    assert_eq!(harness.state().search().unwrap().hits.len(), 1);
    assert_eq!(selected(&harness), Some(3));
}
