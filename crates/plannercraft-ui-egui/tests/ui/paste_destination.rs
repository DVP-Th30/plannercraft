//! Seconde passe UX (ADR-0019) : collage selon la destination et non-changement (R-260 à R-262, H-UI-106, H-UI-107).
//! Module posé vide par le contrat (lot L0), rempli par le lot L2.
//!
//! De bout en bout dans l'application : gestes à la souris et au clavier,
//! presse-papiers par les événements d'egui, une entrée d'annulation par
//! geste d'édition, aucune pour un non-changement. Repères : projet « P »
//! (`plannercraft_tests::daily::plan_p`, UID n = ID n) et exemple embarqué (la
//! maison de la capture du propriétaire, 49 tâches ; 9 → 10 → 11 → 12 →
//! 13 liées, 13 → 14 non : le cahier disait « 11 → 12 non », relu dans
//! `assets/demo/house.xml`, d'où la chaîne 11-14).

#![allow(
    clippy::arithmetic_side_effects,
    reason = "tests : ID bornés par l'exemple embarqué"
)]

use egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use plannercraft_automation::gesture::GestureError;
use plannercraft_core::{LinkType, TaskUid};
use plannercraft_tests::daily::plan_p;
use plannercraft_ui_egui::action::{PressMode, PressTarget, SelectOp};
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::nav::RowRef;
use plannercraft_ui_egui::notify::MessageKind;
use plannercraft_ui_egui::selection::{SelectMode, Selection, SelectionKind};
use plannercraft_ui_egui::structure::StructureOp;
use plannercraft_ui_egui::view::Pane;
use plannercraft_ui_egui::{Action, App, DocOrigin};

use crate::select_mouse::{block, cell_point, click_at, press_at, release_at};
use crate::{fr, harness, harness_for, new_app, open_demo};

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

/// Harnais sur l'exemple embarqué.
fn demo_harness() -> Harness<'static, App> {
    let mut harness = harness();
    open_demo(&mut harness);
    harness
}

/// UID de la tâche d'ID `id` (position + 1).
fn uid_of(app: &App, id: u32) -> TaskUid {
    let position = usize::try_from(id - 1).unwrap();
    app.document
        .as_ref()
        .unwrap()
        .project()
        .tasks
        .at(position)
        .unwrap()
        .uid()
}

/// Appui sur (`id`, `column`), cellule ou N° de ligne.
fn press(harness: &mut Harness<'static, App>, id: u32, column: Column, target: PressTarget, mode: PressMode) {
    let row = RowRef::Item(uid_of(harness.state(), id));
    harness.state_mut().dispatch(Action::SelectOp(SelectOp::Press {
        row,
        column,
        target,
        mode,
    }));
    harness.state_mut().dispatch(Action::FocusPane(Pane::Table));
    harness.run();
}

/// Bloc (`first`, `column`) à (`last`, `column`).
fn cells(harness: &mut Harness<'static, App>, first: u32, last: u32, column: Column) {
    press(harness, first, column, PressTarget::Cell, PressMode::Replace);
    if last != first {
        press(harness, last, column, PressTarget::Cell, PressMode::Extend);
    }
}

/// Lignes entières `first` à `last` (appui sur le N°).
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

/// Ligne vide de fin, colonne `column`.
fn blank(harness: &mut Harness<'static, App>, column: Column) {
    harness.state_mut().dispatch(Action::Select(None));
    harness.state_mut().view_mut().cursor.column = column;
    harness.state_mut().dispatch(Action::FocusPane(Pane::Table));
    harness.run();
}

/// Envoie `event` (Copier, Couper) et rend le texte écrit dans le
/// presse-papiers.
fn copy_event(harness: &mut Harness<'static, App>, event: Event) -> String {
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
    copied.expect("rien écrit dans le presse-papiers")
}

/// Ctrl/Cmd+V avec `text`.
fn paste(harness: &mut Harness<'static, App>, text: &str) {
    harness.event(Event::Paste(text.to_owned()));
    harness.run();
}

fn history_len(app: &App) -> usize {
    app.document.as_ref().unwrap().session.history().entries.len()
}

fn task_count(app: &App) -> usize {
    app.document.as_ref().unwrap().project().tasks.len()
}

/// Durée en jours de la tâche d'ID `id`.
fn days(app: &App, id: u32) -> i64 {
    let project = app.document.as_ref().unwrap().project();
    let position = usize::try_from(id - 1).unwrap();
    project.tasks.at(position).unwrap().duration.minutes().get() / 480
}

/// Prédécesseurs (ID, type) de la tâche d'ID `id`.
fn preds(app: &App, id: u32) -> Vec<(u32, LinkType)> {
    let project = app.document.as_ref().unwrap().project();
    let position = usize::try_from(id - 1).unwrap();
    let id_of = |u: TaskUid| {
        project
            .tasks
            .position(u)
            .and_then(|p| u32::try_from(p + 1).ok())
            .unwrap()
    };
    project
        .tasks
        .at(position)
        .unwrap()
        .predecessors
        .iter()
        .map(|l| (id_of(l.predecessor), l.kind))
        .collect()
}

fn fr_count(id: TextId, n: i64) -> String {
    Texts::new(Language::Fr).get_args(id, &[("count", Arg::Count(n))])
}

/// Précondition des tests sur l'exemple embarqué : 9 → 10 → 11 → 12 → 13
/// liées en FS, 13 → 14 non (ni 14 → 13).
fn assert_demo_links(app: &App) {
    assert_eq!(task_count(app), 49);
    for (p, s) in [(9, 10), (10, 11), (11, 12), (12, 13)] {
        assert!(
            preds(app, s).contains(&(p, LinkType::FinishToStart)),
            "{s} : {:?}",
            preds(app, s)
        );
    }
    assert!(
        !preds(app, 14).iter().any(|(p, _)| *p == 13),
        "{:?}",
        preds(app, 14)
    );
    assert!(
        !preds(app, 13).iter().any(|(p, _)| *p == 14),
        "{:?}",
        preds(app, 13)
    );
}

/// Lier sur la sélection (Tâche → Lier, Ctrl/Cmd+F2).
fn link(harness: &mut Harness<'static, App>) {
    harness.state_mut().dispatch(Action::Structure(StructureOp::Link));
    harness.run();
}

/// R-260 b : 11-14 sélectionnées (11 → 12 → 13 liées), Lier ⇒ seul
/// 13 → 14 est créé, une entrée, aucun message.
#[test]
fn r260_link_skips_existing() {
    let mut harness = demo_harness();
    assert_demo_links(harness.state());
    let before_12 = preds(harness.state(), 12);
    let before_13 = preds(harness.state(), 13);
    whole_rows(&mut harness, 11, 14);
    link(&mut harness);
    let app = harness.state();
    assert_eq!(history_len(app), 1, "{:?}", app.messages());
    assert!(preds(app, 14).contains(&(13, LinkType::FinishToStart)));
    assert_eq!(preds(app, 12), before_12);
    assert_eq!(preds(app, 13), before_13);
    assert!(app.messages().is_empty(), "{:?}", app.messages());
    // Lier de nouveau : tout est lié, rien.
    link(&mut harness);
    assert_eq!(history_len(harness.state()), 1);
    assert_eq!(harness.state().messages(), []);
}

/// R-260 b : 9-12 déjà liées ⇒ aucun lot, aucune entrée, aucun message.
#[test]
fn r260_link_all_linked_noop() {
    let mut harness = demo_harness();
    assert_demo_links(harness.state());
    let project = harness.state().document.as_ref().unwrap().project().clone();
    whole_rows(&mut harness, 9, 12);
    link(&mut harness);
    let app = harness.state();
    assert_eq!(history_len(app), 0);
    assert_eq!(app.document.as_ref().unwrap().project(), &project);
    assert!(app.messages().is_empty(), "{:?}", app.messages());
}

/// R-260 b : chaîne 10 puis 9 (Ctrl/Cmd+clic) alors que 9 → 10 existe :
/// paire liée dans l'autre sens, sautée ; avec 14 en troisième, seul
/// 9 → 14 est créé.
#[test]
fn r260_link_reverse_pair_skipped() {
    let mut harness = demo_harness();
    assert_demo_links(harness.state());
    let (u9, u10, u12) = {
        let app = harness.state();
        (uid_of(app, 9), uid_of(app, 10), uid_of(app, 14))
    };
    let order = |app: &mut App, uids: &[TaskUid]| {
        let rows = app.visible_rows().unwrap();
        let mut sel = Selection::single(uids.first().copied());
        for u in uids.iter().skip(1) {
            sel.click(*u, SelectMode::Toggle, rows);
        }
        sel
    };
    let sel = order(harness.state_mut(), &[u10, u9]);
    let project = harness.state().document.as_ref().unwrap().project().clone();
    assert_eq!(
        plannercraft_ui_egui::structure::plan(StructureOp::Link, &project, &sel),
        None
    );
    let sel = order(harness.state_mut(), &[u10, u9, u12]);
    let plan = plannercraft_ui_egui::structure::plan(StructureOp::Link, &project, &sel).unwrap();
    assert_eq!(plan.batch.ops.len(), 1, "{:?}", plan.batch.ops);
    let plannercraft_automation::Command::LinkAdd(spec) = &plan.batch.ops[0] else {
        panic!("{:?}", plan.batch.ops)
    };
    assert_eq!(
        (spec.pred.clone(), spec.succ.clone()),
        (
            plannercraft_automation::TaskRef::Uid(u9),
            plannercraft_automation::TaskRef::Uid(u12)
        )
    );
}

/// R-260 a : « 6;6 » et « 6;6DD » collés dans Prédécesseurs ⇒ un seul lien.
#[test]
fn r260_duplicate_ids_merged() {
    let mut harness = p_harness();
    cells(&mut harness, 7, 7, Column::Predecessors);
    assert_eq!(preds(harness.state(), 7), [(6, LinkType::FinishToStart)]);
    paste(&mut harness, "6;6");
    assert_eq!(history_len(harness.state()), 0, "même liste : rien");
    assert!(
        harness.state().messages().is_empty(),
        "{:?}",
        harness.state().messages()
    );
    paste(&mut harness, "3;6;6DD");
    let app = harness.state();
    assert_eq!(history_len(app), 1, "{:?}", app.messages());
    // 6 modifié à sa place (`link.update`), 3 ajouté après.
    assert_eq!(
        preds(app, 7),
        [(6, LinkType::StartToStart), (3, LinkType::FinishToStart)]
    );
}

/// R-260 d : coller la valeur en place ⇒ aucune entrée, aucun message.
#[test]
fn r260_same_value_no_entry() {
    let mut harness = p_harness();
    cells(&mut harness, 3, 3, Column::Duration);
    assert_eq!(days(harness.state(), 3), 10);
    for text in ["10 jours", "10j", "10"] {
        paste(&mut harness, text);
        assert_eq!(history_len(harness.state()), 0, "{text}");
    }
    cells(&mut harness, 3, 3, Column::Name);
    paste(&mut harness, "Murs");
    assert_eq!(history_len(harness.state()), 0);
    assert!(
        harness.state().messages().is_empty(),
        "{:?}",
        harness.state().messages()
    );
    // Une autre valeur : une entrée.
    cells(&mut harness, 3, 3, Column::Duration);
    paste(&mut harness, "11j");
    assert_eq!(history_len(harness.state()), 1);
    assert_eq!(days(harness.state(), 3), 11);
}

/// OK-01 (non-régression) : recoller un bloc sur lui-même, « 9 » sur 9,
/// « 9;9 » sur 9, « 4 jours » sur 4 jours ⇒ aucun lot, aucun message.
#[test]
fn ok01_identical_paste_noop() {
    let mut harness = demo_harness();
    assert_demo_links(harness.state());
    // Bloc Nom-Durée de 9 à 11, copié puis recollé sur lui-même.
    press(
        &mut harness,
        9,
        Column::Name,
        PressTarget::Cell,
        PressMode::Replace,
    );
    press(
        &mut harness,
        11,
        Column::Duration,
        PressTarget::Cell,
        PressMode::Extend,
    );
    let text = copy_event(&mut harness, Event::Copy);
    assert_eq!(text.lines().count(), 3, "{text:?}");
    paste(&mut harness, &text);
    assert_eq!(history_len(harness.state()), 0);
    // « 9 » et « 9;9 » sur les prédécesseurs de 10 (9 FS).
    cells(&mut harness, 10, 10, Column::Predecessors);
    let only_9 = preds(harness.state(), 10) == [(9, LinkType::FinishToStart)];
    if only_9 {
        paste(&mut harness, "9");
        paste(&mut harness, "9;9");
        assert_eq!(history_len(harness.state()), 0);
    }
    // La durée en place, recollée sur elle-même.
    cells(&mut harness, 10, 10, Column::Duration);
    let text = copy_event(&mut harness, Event::Copy);
    paste(&mut harness, &text);
    let app = harness.state();
    assert_eq!(history_len(app), 0);
    assert!(app.messages().is_empty(), "{:?}", app.messages());
}

/// H-UI-106, geste de la capture à la souris : ligne 11 copiée par son N°,
/// bloc Prédécesseurs 9-11, Ctrl/Cmd+V ⇒ refus en notification, 49 tâches,
/// aucune entrée d'annulation.
#[test]
fn r261_rows_clip_on_cells_refused() {
    let mut harness = demo_harness();
    assert_demo_links(harness.state());
    let p = cell_point(&harness, 10, Column::Id);
    click_at(&mut harness, p, Modifiers::NONE);
    assert_eq!(harness.state().selected().kind(), SelectionKind::Rows);
    let text = copy_event(&mut harness, Event::Copy);
    assert!(text.contains('\t'), "{text:?}");
    let from = cell_point(&harness, 8, Column::Predecessors);
    let to = cell_point(&harness, 10, Column::Predecessors);
    press_at(&mut harness, from, Modifiers::NONE);
    release_at(&mut harness, from);
    press_at(&mut harness, to, Modifiers::SHIFT);
    release_at(&mut harness, to);
    let b = block(harness.state());
    assert_eq!(
        (b.kind, b.rows.len(), b.columns.as_slice()),
        (SelectionKind::Cells, 3, &[Column::Predecessors][..])
    );
    let before = harness.state().document.as_ref().unwrap().project().clone();
    paste(&mut harness, &text);
    let app = harness.state();
    assert_eq!(task_count(app), 49);
    assert_eq!(history_len(app), 0);
    assert_eq!(app.document.as_ref().unwrap().project(), &before);
    let refusal = fr(TextId::PasteRowsOnCells);
    let message = app
        .messages()
        .iter()
        .find(|m| m.text == refusal)
        .expect("refus affiché");
    assert_eq!(message.kind, MessageKind::Refusal);
    assert_eq!(app.message_store().banner(), [], "notification, pas de bandeau");
    // La même copie sur la ligne entière 12 : insérée (R-244), une entrée.
    whole_rows(&mut harness, 12, 12);
    paste(&mut harness, &text);
    assert_eq!(task_count(harness.state()), 50);
    assert_eq!(history_len(harness.state()), 1);
}

/// R-261 : « 5j » sur des lignes entières 6-7 ⇒ Durée de la cellule active
/// seulement (bloc d'une cellule), qui devient la sélection.
#[test]
fn r261_cell_text_on_rows_pastes_cells() {
    let mut harness = p_harness();
    whole_rows(&mut harness, 6, 7);
    harness.state_mut().view_mut().cursor.column = Column::Duration;
    let (d6, d7) = (days(harness.state(), 6), days(harness.state(), 7));
    assert_ne!(d7, 5);
    paste(&mut harness, "5j");
    let app = harness.state();
    assert_eq!(task_count(app), 7, "aucune tâche insérée");
    assert_eq!((days(app, 6), days(app, 7)), (d6, 5));
    assert_eq!(history_len(app), 1);
    let b = block(app);
    assert_eq!(b.kind, SelectionKind::Cells);
    assert_eq!(b.columns, [Column::Duration]);
    assert_eq!(b.rows.len(), 1);
}

/// R-261 : « 7;6\n9\n10 » sur la ligne vide, colonne Prédécesseurs ⇒ trois
/// tâches sans nom en fin avec ces prédécesseurs, une entrée.
#[test]
fn r261_single_column_on_blank_adds_tasks() {
    let mut harness = demo_harness();
    blank(&mut harness, Column::Predecessors);
    paste(&mut harness, "7;6\n9\n10");
    let app = harness.state();
    assert_eq!(task_count(app), 52, "{:?}", app.messages());
    assert_eq!(history_len(app), 1);
    let project = app.document.as_ref().unwrap().project();
    let names: Vec<&str> = project.tasks.iter().skip(49).map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["", "", ""]);
    let ids = |id: u32| -> Vec<u32> { preds(app, id).into_iter().map(|(p, _)| p).collect() };
    assert_eq!(ids(50), [7, 6]);
    assert_eq!(ids(51), [9]);
    assert_eq!(ids(52), [10]);
}

/// R-261 (R-256 a gardée) : texte à en-tête sur une ligne entière ⇒
/// tâches importées avant elle, restées sélectionnées en lignes entières.
#[test]
fn r261_header_text_on_rows_imports() {
    let mut harness = p_harness();
    whole_rows(&mut harness, 5, 5);
    paste(&mut harness, "Nom\tDurée\nCoffrage\t3j\nFerraillage\t2j");
    let app = harness.state();
    assert_eq!(task_count(app), 9);
    assert_eq!(history_len(app), 1);
    let project = app.document.as_ref().unwrap().project();
    assert_eq!(project.tasks.at(4).unwrap().name, "Coffrage");
    assert_eq!(project.tasks.at(5).unwrap().name, "Ferraillage");
    assert_eq!(app.selected().kind(), SelectionKind::Rows);
    assert_eq!(app.selected().len(), 2);
    assert_eq!(app.selection, Some(project.tasks.at(4).unwrap().uid()));
    // Le même texte sur un bloc de cellules : collé en cellules, refusé à
    // la lecture (« Nom » n'est pas une durée), rien écrit.
    cells(&mut harness, 2, 2, Column::Duration);
    paste(&mut harness, "Nom\tDurée\nCoffrage\t3j");
    assert_eq!(task_count(harness.state()), 9);
    assert_eq!(history_len(harness.state()), 1);
}

/// H-UI-106 : Ctrl/Cmd+V deux fois de suite suit le même chemin : lignes
/// collées restées sélectionnées en lignes entières (un second collage
/// insère encore) ; bloc de cellules écrit resté sélectionné.
#[test]
fn r261_second_paste_same_route() {
    let mut harness = p_harness();
    whole_rows(&mut harness, 6, 6);
    let rows_text = copy_event(&mut harness, Event::Copy);
    whole_rows(&mut harness, 2, 2);
    paste(&mut harness, &rows_text);
    assert_eq!(task_count(harness.state()), 8);
    assert_eq!(harness.state().selected().kind(), SelectionKind::Rows);
    paste(&mut harness, &rows_text);
    let app = harness.state();
    assert_eq!(task_count(app), 9);
    assert_eq!(history_len(app), 2);
    // Cellules : « 2j\n3j » sur (8, Durée) ⇒ bloc 8-9 (Plâtrerie,
    // Peinture, décalées par les deux insertions) écrit et sélectionné.
    cells(&mut harness, 8, 8, Column::Duration);
    paste(&mut harness, "2j\n3j");
    let app = harness.state();
    assert_eq!((days(app, 8), days(app, 9)), (2, 3));
    let b = block(app);
    assert_eq!(
        (b.kind, b.rows.len(), b.columns.as_slice()),
        (SelectionKind::Cells, 2, &[Column::Duration][..])
    );
    assert_eq!(history_len(app), 3);
    // Second collage sur le bloc écrit : cellules encore (« 4j » partout).
    paste(&mut harness, "4j");
    let app = harness.state();
    assert_eq!((days(app, 8), days(app, 9)), (4, 4));
    assert_eq!(task_count(app), 9);
    assert_eq!(history_len(app), 4);
}

/// R-257 (compte rendu, seconde passe UX) : valeurs au-delà de la dernière
/// colonne visible comptées à part, en information.
#[test]
fn r257_values_outside_reported() {
    let mut harness = p_harness();
    cells(&mut harness, 7, 7, Column::Predecessors);
    paste(&mut harness, "2\tx\ty");
    let app = harness.state();
    assert_eq!(history_len(app), 1);
    let text = fr_count(TextId::PasteValuesOutside, 2);
    let message = app
        .messages()
        .iter()
        .find(|m| m.text == text)
        .expect("compte rendu");
    assert_eq!(message.kind, MessageKind::Info);
    assert!(
        !app.messages()
            .iter()
            .any(|m| m.text == fr_count(TextId::CellsSkipped, 2))
    );
}

/// R-258 amendée, H-UI-120 : Ctrl/Cmd+R sur (6, Durée)-(6, Début) ⇒ refus
/// nommé (Début ne lit pas « 4 jours »), rien écrit ; (2, Début)-(2, Fin)
/// ⇒ une entrée.
#[test]
fn r258_fill_right_keys() {
    let mut harness = p_harness();
    press(
        &mut harness,
        6,
        Column::Duration,
        PressTarget::Cell,
        PressMode::Replace,
    );
    press(
        &mut harness,
        6,
        Column::Start,
        PressTarget::Cell,
        PressMode::Extend,
    );
    assert_eq!(block(harness.state()).columns, [Column::Duration, Column::Start]);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::R);
    harness.run();
    let app = harness.state();
    assert_eq!(history_len(app), 0);
    let refusal = app
        .messages()
        .iter()
        .find(|m| m.title == Some(TextId::CommandErrorTitle))
        .expect("refus nommé");
    assert!(refusal.text.contains("4 jours"), "{}", refusal.text);
    assert!(refusal.text.contains('6'), "{}", refusal.text);
    assert!(refusal.detail.is_some(), "détail du moteur repliable");
    press(
        &mut harness,
        2,
        Column::Start,
        PressTarget::Cell,
        PressMode::Replace,
    );
    press(
        &mut harness,
        2,
        Column::Finish,
        PressTarget::Cell,
        PressMode::Extend,
    );
    harness.key_press_modifiers(Modifiers::COMMAND, Key::R);
    harness.run();
    assert_eq!(history_len(harness.state()), 1);
}

/// R-260 c, H-UI-107 : glisser un lien sur une paire liée ⇒ information
/// « Ces tâches sont déjà liées. », rien écrit.
#[test]
fn r260_link_drag_already_linked_info() {
    let mut harness = p_harness();
    harness
        .state_mut()
        .dispatch(Action::GestureRefused(GestureError::AlreadyLinked));
    harness.run();
    let app = harness.state();
    let text = fr(TextId::LinkAlreadyLinked);
    let message = app
        .messages()
        .iter()
        .find(|m| m.text == text)
        .expect("information");
    assert_eq!(message.kind, MessageKind::Info);
    assert_eq!(history_len(app), 0);
    // Le geste lui-même : aucun lot sur 2 → 3 (déjà liées).
    let session = &harness.state().document.as_ref().unwrap().session;
    assert_eq!(
        plannercraft_automation::gesture::link_drag(session, uid(2), uid(3)),
        Err(GestureError::AlreadyLinked)
    );
}

/// R-261 (relecture finale ux2, C7 ; constat 2 du lot L11) : ligne 6
/// choisie par son N° (cellule active dans l'ID), texte collé ⇒ écrit dans
/// le Nom, jamais sauté sur l'ID non modifiable ; une entrée.
#[test]
fn r261_rows_by_id_paste_not_skipped() {
    let mut harness = p_harness();
    press(
        &mut harness,
        6,
        Column::Id,
        PressTarget::RowHeader,
        PressMode::Replace,
    );
    assert_eq!(block(harness.state()).active.1, Column::Id);
    paste(&mut harness, "Gros œuvre");
    let app = harness.state();
    let project = app.document.as_ref().unwrap().project();
    assert_eq!(project.tasks.at(5).unwrap().name, "Gros œuvre");
    assert_eq!(history_len(app), 1);
    assert_eq!(task_count(app), 7, "aucune tâche insérée");
    let skipped = fr_count(TextId::CellsSkipped, 1);
    assert!(
        app.messages().iter().all(|m| m.text != skipped),
        "{:?}",
        app.messages()
    );
    // Fonction pure : Nom, sinon première colonne après l'ID.
    let all = [Column::Id, Column::Name, Column::Duration];
    assert_eq!(
        plannercraft_ui_egui::cells::rows_paste_column(Column::Id, &all),
        Column::Name
    );
    assert_eq!(
        plannercraft_ui_egui::cells::rows_paste_column(Column::Duration, &all),
        Column::Duration
    );
    assert_eq!(
        plannercraft_ui_egui::cells::rows_paste_column(Column::Id, &[Column::Id, Column::Duration]),
        Column::Duration
    );
}
