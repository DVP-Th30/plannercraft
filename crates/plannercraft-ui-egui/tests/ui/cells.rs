//! Édition sur sélection (passe UX, ADR-0018, lot L2) : coller, remplir
//! vers le bas, effacer, Suppr selon la cible, Ctrl/Cmd+Entrée (R-257 à
//! R-259, H-UI-95 à H-UI-98), de bout en bout dans l'application : gestes
//! clavier et presse-papiers, une entrée d'annulation par geste, messages
//! au bandeau. Repère : projet « P » (`plannercraft_tests::daily::plan_p`, UID
//! n = ID n).

use egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use plannercraft_core::TaskUid;
use plannercraft_tests::daily::plan_p;
use plannercraft_ui_egui::action::{PressMode, PressTarget, SelectOp};
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::nav::RowRef;
use plannercraft_ui_egui::view::Pane;
use plannercraft_ui_egui::{Action, App, DocOrigin};

use crate::{harness_for, new_app};

/// UID `n`.
const fn uid(n: u32) -> TaskUid {
    TaskUid::new(n)
}

/// Harnais sur le projet « P », table focalisée.
fn p_harness() -> Harness<'static, App> {
    let mut app = new_app();
    app.open_project(
        plan_p().project().clone(),
        DocOrigin {
            name: "P".to_owned(),
            path: None,
            demo: false,
        },
    );
    let mut harness = harness_for(app);
    harness.run();
    harness
}

/// Appui sur la cellule (`n`, `column`) (ou son ID pour `RowHeader`).
fn press(harness: &mut Harness<'static, App>, n: u32, column: Column, target: PressTarget, mode: PressMode) {
    harness.state_mut().dispatch(Action::SelectOp(SelectOp::Press {
        row: RowRef::Item(uid(n)),
        column,
        target,
        mode,
    }));
    harness.state_mut().dispatch(Action::FocusPane(Pane::Table));
    harness.run();
}

/// Bloc de la cellule (`first`, `column`) à (`last`, `column`).
fn cells(harness: &mut Harness<'static, App>, first: u32, last: u32, column: Column) {
    press(harness, first, column, PressTarget::Cell, PressMode::Replace);
    if last != first {
        press(harness, last, column, PressTarget::Cell, PressMode::Extend);
    }
}

/// Lignes entières `first` à `last`.
fn whole_rows(harness: &mut Harness<'static, App>, first: u32, last: u32) {
    press(
        harness,
        first,
        Column::Name,
        PressTarget::RowHeader,
        PressMode::Replace,
    );
    if last != first {
        press(
            harness,
            last,
            Column::Name,
            PressTarget::RowHeader,
            PressMode::Extend,
        );
    }
}

/// Envoie `event` et rend le texte écrit dans le presse-papiers par cette
/// image.
fn copy_event(harness: &mut Harness<'static, App>, event: Event) -> Option<String> {
    harness.event(event);
    harness.step();
    let copied = harness
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|c| match c {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        });
    harness.run();
    copied
}

fn paste(harness: &mut Harness<'static, App>, text: &str) {
    harness.event(Event::Paste(text.to_owned()));
    harness.run();
}

fn key(harness: &mut Harness<'static, App>, modifiers: Modifiers, k: Key) {
    harness.key_press_modifiers(modifiers, k);
    harness.run();
}

fn history_len(app: &App) -> usize {
    app.document.as_ref().unwrap().session.history().entries.len()
}

fn task_count(app: &App) -> usize {
    app.document.as_ref().unwrap().project().tasks.len()
}

fn days(app: &App, n: u32) -> i64 {
    app.document
        .as_ref()
        .unwrap()
        .project()
        .tasks
        .get(uid(n))
        .unwrap()
        .duration
        .minutes()
        .get()
        / 480
}

fn preds(app: &App, n: u32) -> Vec<u32> {
    app.document
        .as_ref()
        .unwrap()
        .project()
        .tasks
        .get(uid(n))
        .unwrap()
        .predecessors
        .iter()
        .map(|l| l.predecessor.get())
        .collect()
}

fn names(app: &App) -> Vec<String> {
    app.document
        .as_ref()
        .unwrap()
        .project()
        .tasks
        .iter()
        .map(|t| t.name.clone())
        .collect()
}

fn has_message(app: &App, text: &str) -> bool {
    app.messages().iter().any(|m| m.text == text)
}

fn fr_count(id: TextId, n: i64) -> String {
    Texts::new(Language::Fr).get_args(id, &[("count", Arg::Count(n))])
}

/// U11 : (3, Durée) active, coller « 7j » ⇒ 3 dure 7 j, toujours 7
/// tâches, une annulation.
#[test]
fn h_ui_95_paste_value_into_cell() {
    let mut harness = p_harness();
    cells(&mut harness, 3, 3, Column::Duration);
    paste(&mut harness, "7j");
    let app = harness.state();
    assert_eq!(days(app, 3), 7);
    assert_eq!(task_count(app), 7);
    assert_eq!(history_len(app), 1);
    // Bloc Durée 6-7, « 2j » : les deux cellules.
    cells(&mut harness, 6, 7, Column::Duration);
    paste(&mut harness, "2j");
    let app = harness.state();
    assert_eq!((days(app, 6), days(app, 7)), (2, 2));
    assert_eq!(history_len(app), 2);
    // Texte illisible : refus au bandeau, rien écrit.
    cells(&mut harness, 2, 2, Column::Duration);
    paste(&mut harness, "abc");
    let app = harness.state();
    assert_eq!(history_len(app), 2);
    assert!(
        app.messages()
            .iter()
            .any(|m| m.title == Some(TextId::CommandErrorTitle)),
        "refus au bandeau"
    );
}

/// U12 : copier (3, Durée) ⇒ « 10 jours » (texte de saisie : l'affichage,
/// qui se relit à la même valeur), sans tampon de
/// lignes ; recollé sur (6, Durée) ⇒ 6 dure 10 j.
#[test]
fn h_ui_95_copy_cell_text() {
    let mut harness = p_harness();
    cells(&mut harness, 3, 3, Column::Duration);
    let text = copy_event(&mut harness, Event::Copy).expect("rien copié");
    assert_eq!(text, "10 jours");
    let slot = harness.state().clipboard().unwrap();
    assert_eq!(slot.kind, plannercraft_ui_egui::clipboard::ClipKind::Cells);
    assert_eq!(slot.clip.tasks.len(), 0);
    assert_eq!(history_len(harness.state()), 0);
    cells(&mut harness, 6, 6, Column::Duration);
    paste(&mut harness, &text);
    assert_eq!(days(harness.state(), 6), 10);
    assert_eq!(task_count(harness.state()), 7);
    // Bloc de deux colonnes : texte tabulé.
    press(
        &mut harness,
        2,
        Column::Name,
        PressTarget::Cell,
        PressMode::Replace,
    );
    harness.state_mut().view_mut().cursor.column = Column::Duration;
    let text = copy_event(&mut harness, Event::Copy).unwrap();
    assert_eq!(text, "Fondations\t5 jours");
}

/// H-UI-39 gardée : lignes 6-7 entières copiées puis collées sur la ligne
/// entière 2 ⇒ deux tâches insérées avant 2 (destination de R-261 : sur
/// une cellule, le collage serait refusé).
#[test]
fn h_ui_95_rows_clip_still_inserts() {
    let mut harness = p_harness();
    whole_rows(&mut harness, 6, 7);
    let text = copy_event(&mut harness, Event::Copy).unwrap();
    assert!(text.starts_with("6\tPlâtrerie\t"), "{text:?}");
    whole_rows(&mut harness, 2, 2);
    paste(&mut harness, &text);
    let app = harness.state();
    assert_eq!(
        names(app)[..4],
        ["Gros œuvre", "Plâtrerie", "Peinture", "Fondations"]
    );
    assert_eq!(history_len(app), 1);
}

/// R-256 a gardée : un texte à en-tête reconnu collé sur une ligne entière
/// crée des lignes (R-261 : sur un bloc de cellules, il se colle en
/// cellules).
#[test]
fn h_ui_95_header_text_imports_rows() {
    let mut harness = p_harness();
    whole_rows(&mut harness, 5, 5);
    paste(&mut harness, "Nom\tDurée\nCoffrage\t3j\nFerraillage\t2j");
    let app = harness.state();
    assert_eq!(task_count(app), 9);
    assert!(names(app).contains(&"Coffrage".to_owned()));
    assert_eq!(history_len(app), 1);
    // Une colonne de texte sur la ligne vide : des tâches (R-256 b).
    harness.state_mut().dispatch(Action::Select(None));
    harness.run();
    paste(&mut harness, "A\nB");
    assert_eq!(task_count(harness.state()), 11);
}

/// Couper un bloc = copier puis effacer le contenu (R-259) : une entrée
/// d'annulation ; Ctrl+Z rend les liens.
#[test]
fn h_ui_95_cut_block_one_undo() {
    let mut harness = p_harness();
    cells(&mut harness, 3, 3, Column::Predecessors);
    let text = copy_event(&mut harness, Event::Cut).expect("couper écrit le texte");
    assert_eq!(text, "2");
    let app = harness.state();
    assert_eq!(preds(app, 3).len(), 0);
    assert_eq!(task_count(app), 7, "la tâche reste");
    assert_eq!(history_len(app), 1);
    cells(&mut harness, 7, 7, Column::Predecessors);
    paste(&mut harness, &text);
    assert_eq!(preds(harness.state(), 7), [2]);
    harness.state_mut().dispatch(Action::Undo);
    harness.state_mut().dispatch(Action::Undo);
    harness.run();
    assert_eq!(preds(harness.state(), 3), [2]);
    assert_eq!(preds(harness.state(), 7), [6]);
}

/// U10 : bloc (2, Durée)-(3, Durée), Ctrl/Cmd+D ⇒ 3 dure 5 j, une
/// annulation ; (7, Durée) seule ⇒ copiée de 6.
#[test]
fn h_ui_96_ctrl_d_one_undo() {
    let mut harness = p_harness();
    cells(&mut harness, 2, 3, Column::Duration);
    key(&mut harness, Modifiers::COMMAND, Key::D);
    let app = harness.state();
    assert_eq!(days(app, 3), 5);
    assert_eq!(history_len(app), 1);
    cells(&mut harness, 7, 7, Column::Duration);
    key(&mut harness, Modifiers::COMMAND, Key::D);
    assert_eq!(days(harness.state(), 7), 4);
    assert_eq!(history_len(harness.state()), 2);
    harness.state_mut().dispatch(Action::Undo);
    harness.run();
    assert_eq!(days(harness.state(), 7), 3);
    // Récapitulative dans le bloc : sautée, comptée au bandeau.
    cells(&mut harness, 4, 6, Column::Duration);
    key(&mut harness, Modifiers::COMMAND, Key::D);
    let app = harness.state();
    assert_eq!(days(app, 6), 0);
    assert!(has_message(app, &fr_count(TextId::CellsSkipped, 1)));
}

/// U13 : (3, Prédécesseurs), Suppr ⇒ lien 2→3 retiré, 3 reste ; (3,
/// Durée), Suppr ⇒ rien, message ; Ctrl+Suppr efface toujours.
#[test]
fn h_ui_97_delete_on_duration_clears() {
    let mut harness = p_harness();
    cells(&mut harness, 3, 3, Column::Predecessors);
    key(&mut harness, Modifiers::NONE, Key::Delete);
    let app = harness.state();
    assert_eq!(preds(app, 3).len(), 0);
    assert_eq!(task_count(app), 7);
    assert_eq!(history_len(app), 1);
    cells(&mut harness, 3, 3, Column::Duration);
    key(&mut harness, Modifiers::NONE, Key::Delete);
    let app = harness.state();
    assert_eq!(history_len(app), 1, "rien écrit");
    assert!(has_message(app, &fr_count(TextId::CellsNotClearable, 1)));
    // Ctrl+Suppr sur le Nom : effacé (pas supprimé).
    cells(&mut harness, 7, 7, Column::Name);
    key(&mut harness, Modifiers::COMMAND, Key::Delete);
    let app = harness.state();
    assert_eq!(task_count(app), 7);
    assert_eq!(names(app)[6], "");
}

/// (3, Nom), Suppr ⇒ 3 supprimée, message « 1 tâche supprimée — Ctrl+Z
/// annule » ; lignes entières ⇒ supprimées aussi.
#[test]
fn h_ui_97_delete_on_name_deletes_with_message() {
    let mut harness = p_harness();
    cells(&mut harness, 3, 3, Column::Name);
    key(&mut harness, Modifiers::NONE, Key::Delete);
    let app = harness.state();
    assert_eq!(task_count(app), 6);
    assert!(!names(app).contains(&"Murs".to_owned()));
    let undo = app
        .messages()
        .iter()
        .find(|m| m.text.starts_with("1 tâche supprimée"));
    assert!(
        undo.is_some_and(|m| m.text.ends_with("annule.")),
        "{:?}",
        app.messages()
    );
    whole_rows(&mut harness, 6, 7);
    harness.state_mut().view_mut().cursor.column = Column::Duration;
    key(&mut harness, Modifiers::NONE, Key::Delete);
    assert_eq!(task_count(harness.state()), 4);
}

/// Relecture finale n° 3 : un bloc de plusieurs cellules s'efface, quelle
/// que soit sa cellule active : colonne entière du Nom (clic sur son titre)
/// ⇒ noms effacés, aucune tâche supprimée, une annulation ; bloc glissé de
/// (2, Durée) vers (5, Nom) ⇒ aucune tâche supprimée.
#[test]
fn h_ui_97_delete_multi_cell_block_clears() {
    let mut harness = p_harness();
    let before = names(harness.state());
    harness.state_mut().dispatch(Action::SelectOp(SelectOp::Press {
        row: RowRef::Blank,
        column: Column::Name,
        target: PressTarget::ColumnHeader,
        mode: PressMode::Replace,
    }));
    harness.state_mut().dispatch(Action::FocusPane(Pane::Table));
    harness.run();
    key(&mut harness, Modifiers::NONE, Key::Delete);
    let app = harness.state();
    assert_eq!(task_count(app), 7, "{:?}", app.messages());
    assert_eq!(history_len(app), 1);
    assert!(names(app).iter().all(String::is_empty), "{:?}", names(app));
    key(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(names(harness.state()), before);
    press(
        &mut harness,
        2,
        Column::Duration,
        PressTarget::Cell,
        PressMode::Replace,
    );
    press(
        &mut harness,
        5,
        Column::Name,
        PressTarget::Cell,
        PressMode::Extend,
    );
    key(&mut harness, Modifiers::NONE, Key::Delete);
    assert_eq!(task_count(harness.state()), 7);
}

/// U14 : Retour arrière puis Entrée sur Prédécesseurs ⇒ liens retirés ;
/// sur la Durée ⇒ saisie gardée avec le message.
#[test]
fn h_ui_97_backspace_enter_clears() {
    let mut harness = p_harness();
    cells(&mut harness, 7, 7, Column::Predecessors);
    key(&mut harness, Modifiers::NONE, Key::Backspace);
    assert!(harness.state().view().edit.editing.is_some());
    key(&mut harness, Modifiers::NONE, Key::Enter);
    let app = harness.state();
    assert_eq!(preds(app, 7).len(), 0);
    assert_eq!(history_len(app), 1);
    cells(&mut harness, 3, 3, Column::Duration);
    key(&mut harness, Modifiers::NONE, Key::Backspace);
    key(&mut harness, Modifiers::NONE, Key::Enter);
    let app = harness.state();
    let editing = app.view().edit.editing.as_ref().expect("saisie gardée");
    assert_eq!(
        editing.error,
        Some(plannercraft_ui_egui::edit::EditError::NotClearable)
    );
    assert_eq!(history_len(app), 1);
}

/// U15 : bloc Durée 2-3, frappe « 2 », Ctrl/Cmd+Entrée ⇒ 2 et 3 durent
/// 2 j, une annulation, saisie fermée ; texte refusé ⇒ saisie gardée.
#[test]
fn h_ui_98_ctrl_enter_fills_block() {
    let mut harness = p_harness();
    cells(&mut harness, 2, 3, Column::Duration);
    harness.event(Event::Text("2".to_owned()));
    harness.run();
    assert!(harness.state().view().edit.editing.is_some(), "saisie ouverte");
    key(&mut harness, Modifiers::COMMAND, Key::Enter);
    let app = harness.state();
    assert_eq!((days(app, 2), days(app, 3)), (2, 2));
    assert_eq!(history_len(app), 1);
    assert!(app.view().edit.editing.is_none(), "saisie fermée");
    cells(&mut harness, 2, 3, Column::Duration);
    harness.event(Event::Text("x".to_owned()));
    harness.run();
    key(&mut harness, Modifiers::COMMAND, Key::Enter);
    let app = harness.state();
    assert!(app.view().edit.editing.is_some(), "saisie gardée");
    assert_eq!(history_len(app), 1);
}

/// H-UI-101 : après un clic dans le Gantt, Ctrl+C copie la cellule active.
#[test]
fn h_ui_101_copy_after_gantt_click() {
    let mut harness = p_harness();
    cells(&mut harness, 3, 3, Column::Duration);
    harness.state_mut().dispatch(Action::FocusPane(Pane::Gantt));
    harness.run();
    let text = copy_event(&mut harness, Event::Copy).expect("copie prise dans le Gantt");
    assert_eq!(text, "10 jours");
    paste(&mut harness, "8j");
    assert_eq!(days(harness.state(), 3), 8);
}
