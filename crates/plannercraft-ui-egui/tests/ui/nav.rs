//! Navigation au clavier dans la table (J7, lot L1) : conventions H-UI-13
//! à H-UI-18 et H-UI-29 (`docs/features/ui.md` § 1.11), focus de la table
//! (verrou de focus d'egui), contextes désactivés (saisie, menu), curseur
//! dessiné et accessible, propriété P45 côté application. Chaque touche
//! passe par `KEYMAP` (`check_keymap_entry`, appelé par `session.rs` pour
//! chaque entrée J7).

#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_precision_loss,
    reason = "tests : valeurs bornées par l'exemple et les stratégies"
)]

use egui::{Event, Key, Modifiers, PointerButton, Pos2, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_core::TaskUid;
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::keymap::{Binding, KEYMAP, KeyContext, UiBinding};
use plannercraft_ui_egui::nav::{self, GridPos, GridShape, NavMove, RowRef};
use plannercraft_ui_egui::timescale::TimeAxis;
use plannercraft_ui_egui::view::ROW_H;
use plannercraft_ui_egui::view::table::table_id;
use plannercraft_ui_egui::{Action, App, TimeScaleZoom};
use proptest::prelude::*;

use plannercraft_ui_egui::ribbon::RibbonTab;

use crate::{fr, harness, harness_for, new_app, open_demo, ribbon_click};

const SHIFT: Modifiers = Modifiers::SHIFT;
const COMMAND: Modifiers = Modifiers::COMMAND;
const ALT_SHIFT: Modifiers = Modifiers {
    alt: true,
    shift: true,
    ..Modifiers::NONE
};

/// Colonnes visibles, dans l'ordre d'affichage.
fn visible(app: &App) -> Vec<Column> {
    app.state.visible_columns().map(|state| state.column).collect()
}

/// Indice de la ligne courante (ligne vide : nombre de tâches).
fn row_index(app: &App) -> usize {
    let rows = app.rows().unwrap();
    match app.view().cursor.row {
        RowRef::Item(uid) => rows.index_of(uid).unwrap(),
        RowRef::Blank => rows.len(),
    }
}

/// (ligne, colonne) du curseur ; la ligne du curseur est toujours celle de
/// la sélection.
fn at(app: &App) -> (usize, Column) {
    let rows = app.rows().unwrap();
    let expected = app
        .selection
        .and_then(|uid| rows.index_of(uid))
        .unwrap_or(rows.len());
    assert_eq!(row_index(app), expected, "curseur ≠ sélection");
    (expected, app.view().cursor.column)
}

/// UID de la ligne `i`.
fn uid_at(app: &App, i: usize) -> TaskUid {
    app.rows().unwrap().row(i).unwrap().uid
}

/// Libellé d'accessibilité de la ligne `i`.
fn row_label(app: &App, i: usize) -> String {
    let rows = app.rows().unwrap();
    let row = rows.row(i).unwrap();
    Texts::new(Language::Fr).get_args(
        TextId::RowLabel,
        &[
            ("id", Arg::Count(i64::from(row.id))),
            ("name", Arg::Str(rows.text(i, Column::Name).to_owned().into())),
        ],
    )
}

/// Point au centre de la cellule (ligne `i` visible, `column`).
fn cell_point(harness: &Harness<'static, App>, i: usize, column: Column) -> Pos2 {
    let app = harness.state();
    let rect = harness.get_by_label(&row_label(app, i)).rect();
    let mut x = rect.left();
    for state in app.state.visible_columns() {
        let w = f32::from(state.width);
        if state.column == column {
            return pos2(x + w / 2.0, rect.center().y);
        }
        x += w;
    }
    panic!("colonne {column:?} masquée");
}

/// Clic sur la cellule (ligne `i` visible, `column`) : sélection, focus de
/// la table, colonne courante.
fn click_cell(harness: &mut Harness<'static, App>, i: usize, column: Column) {
    let p = cell_point(harness, i, column);
    harness.event(Event::PointerMoved(p));
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run();
    assert_eq!(at(harness.state()), (i, column));
    assert!(table_focused(harness));
}

/// La table a le focus clavier d'egui.
fn table_focused(harness: &Harness<'static, App>) -> bool {
    harness.ctx.memory(|memory| memory.has_focus(table_id()))
}

/// Appuie sur `key` avec `modifiers`.
fn press(harness: &mut Harness<'static, App>, modifiers: Modifiers, key: Key) {
    harness.key_press_modifiers(modifiers, key);
    harness.run();
}

/// Exemple ouvert, cellule (ligne `i`, `column`) courante.
fn demo_at(i: usize, column: Column) -> Harness<'static, App> {
    let mut harness = harness();
    open_demo(&mut harness);
    click_cell(&mut harness, i, column);
    harness
}

/// Forme de la grille de l'exemple (ligne vide comprise).
fn shape(app: &App) -> GridShape {
    GridShape {
        rows: app.rows().unwrap().len() + 1,
        cols: visible(app).len(),
        page: nav::page_rows(app.view().last_body_height, ROW_H),
    }
}

/// Position attendue après `mv` depuis la cellule courante.
fn expected(app: &App, mv: NavMove) -> (usize, Column) {
    let (row, column) = at(app);
    let cols = visible(app);
    let col = cols.iter().position(|c| *c == column).unwrap();
    let to = nav::step(GridPos { row, col }, mv, shape(app));
    (to.row, cols[to.col])
}

#[test]
fn h_ui_13_enter_moves_down_without_editor() {
    let mut harness = demo_at(3, Column::Duration);
    press(&mut harness, Modifiers::NONE, Key::Enter);
    let app = harness.state();
    assert_eq!(at(app), (4, Column::Duration));
    assert!(
        app.view().edit.editing.is_none(),
        "Entrée hors saisie n'ouvre rien"
    );
    assert_eq!(app.document.as_ref().unwrap().generation(), 0);
    // ↓ comme Entrée ; au bord (ligne vide), rien ne bouge.
    press(&mut harness, Modifiers::NONE, Key::ArrowDown);
    assert_eq!(at(harness.state()), (5, Column::Duration));
    let last = harness.state().rows().unwrap().len();
    press(&mut harness, COMMAND, Key::ArrowDown);
    assert_eq!(at(harness.state()), (last, Column::Duration));
    press(&mut harness, Modifiers::NONE, Key::Enter);
    assert_eq!(at(harness.state()), (last, Column::Duration));
    assert!(harness.state().view().edit.editing.is_none());
}

#[test]
fn h_ui_13_shift_enter_moves_up() {
    let mut harness = demo_at(3, Column::Name);
    press(&mut harness, SHIFT, Key::Enter);
    assert_eq!(at(harness.state()), (2, Column::Name));
    // Maj+Entrée n'étend pas la sélection.
    assert_eq!(harness.state().selected().len(), 1);
    press(&mut harness, Modifiers::NONE, Key::ArrowUp);
    press(&mut harness, Modifiers::NONE, Key::ArrowUp);
    press(&mut harness, SHIFT, Key::Enter);
    assert_eq!(at(harness.state()), (0, Column::Name), "au bord, rien ne bouge");
}

#[test]
fn h_ui_14_tab_next_visible_column() {
    let mut harness = demo_at(2, Column::Name);
    press(&mut harness, Modifiers::NONE, Key::Tab);
    assert_eq!(at(harness.state()), (2, Column::Duration));
    press(&mut harness, SHIFT, Key::Tab);
    press(&mut harness, SHIFT, Key::Tab);
    // Colonne calculée (ID) comprise.
    assert_eq!(at(harness.state()), (2, Column::Id));
    press(&mut harness, SHIFT, Key::Tab);
    assert_eq!(at(harness.state()), (2, Column::Id), "sans retour à la ligne");
    press(&mut harness, Modifiers::NONE, Key::ArrowRight);
    assert_eq!(at(harness.state()), (2, Column::Name));
    press(&mut harness, Modifiers::NONE, Key::ArrowLeft);
    assert_eq!(at(harness.state()), (2, Column::Id));
}

#[test]
fn h_ui_14_tab_at_last_column_stays() {
    let mut harness = demo_at(2, Column::Predecessors);
    let last = *visible(harness.state()).last().unwrap();
    assert_eq!(last, Column::Predecessors);
    press(&mut harness, Modifiers::NONE, Key::Tab);
    assert_eq!(at(harness.state()), (2, Column::Predecessors));
    assert!(table_focused(&harness));
}

/// En fin de saisie, Tab valide et passe à la colonne visible suivante
/// (change le « colonne éditable suivante » de J4).
#[test]
fn h_ui_14_tab_commits_and_moves() {
    let mut harness = demo_at(3, Column::Duration);
    harness.event(Event::Text("3j".to_owned()));
    harness.run();
    assert!(harness.state().view().edit.editing.is_some());
    press(&mut harness, Modifiers::NONE, Key::Tab);
    let app = harness.state();
    assert!(app.view().edit.editing.is_none());
    assert_eq!(app.document.as_ref().unwrap().generation(), 1);
    assert_eq!(at(app), (3, Column::Start));
    assert!(table_focused(&harness), "la table reprend le clavier");
    // Maj+Tab en fin de saisie sur Nom : vers l'ID (calculé).
    press(&mut harness, Modifiers::NONE, Key::Home);
    press(&mut harness, Modifiers::NONE, Key::Tab);
    assert_eq!(at(harness.state()), (3, Column::Name));
    harness.event(Event::Text("Plans".to_owned()));
    harness.run();
    press(&mut harness, SHIFT, Key::Tab);
    assert_eq!(at(harness.state()), (3, Column::Id));
    assert_eq!(harness.state().document.as_ref().unwrap().generation(), 2);
}

#[test]
fn h_ui_15_home_end() {
    let mut harness = demo_at(4, Column::Duration);
    press(&mut harness, Modifiers::NONE, Key::End);
    assert_eq!(at(harness.state()), (4, Column::Predecessors));
    press(&mut harness, Modifiers::NONE, Key::Home);
    assert_eq!(at(harness.state()), (4, Column::Id));
    press(&mut harness, COMMAND, Key::ArrowRight);
    assert_eq!(at(harness.state()), (4, Column::Predecessors));
    press(&mut harness, COMMAND, Key::ArrowLeft);
    assert_eq!(at(harness.state()), (4, Column::Id));
}

#[test]
fn h_ui_15_ctrl_home_end() {
    let mut harness = demo_at(4, Column::Duration);
    let blank = harness.state().rows().unwrap().len();
    press(&mut harness, COMMAND, Key::End);
    // Dernier champ de la dernière ligne, ligne vide comprise.
    assert_eq!(at(harness.state()), (blank, Column::Predecessors));
    assert!(harness.state().selection.is_none());
    press(&mut harness, COMMAND, Key::Home);
    assert_eq!(at(harness.state()), (0, Column::Id));
}

#[test]
fn h_ui_15_ctrl_up_down() {
    let mut harness = demo_at(4, Column::Start);
    let blank = harness.state().rows().unwrap().len();
    press(&mut harness, COMMAND, Key::ArrowDown);
    assert_eq!(at(harness.state()), (blank, Column::Start));
    press(&mut harness, COMMAND, Key::ArrowUp);
    assert_eq!(at(harness.state()), (0, Column::Start));
}

/// Page bas : lignes visibles − 1 (H-UI-15, hypothèse).
#[test]
fn h_ui_15_page_down() {
    let mut harness = demo_at(5, Column::Name);
    let page = nav::page_rows(harness.state().view().last_body_height, ROW_H);
    assert!(page > 3, "{page}");
    press(&mut harness, Modifiers::NONE, Key::PageDown);
    assert_eq!(at(harness.state()), (5 + page - 1, Column::Name));
    press(&mut harness, Modifiers::NONE, Key::PageUp);
    assert_eq!(at(harness.state()), (5, Column::Name));
    press(&mut harness, Modifiers::NONE, Key::PageUp);
    assert_eq!(at(harness.state()), (0, Column::Name));
}

/// Ctrl = Cmd sur Mac : un Mac envoie `mac_cmd` et `command` (egui-winit).
#[test]
fn h_ui_15_command_on_mac() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1280.0, 720.0))
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .with_os(egui::os::OperatingSystem::Mac)
        .with_max_steps(8)
        .build_ui_state(plannercraft_ui_egui::ui, new_app());
    open_demo(&mut harness);
    click_cell(&mut harness, 4, Column::Duration);
    let cmd = Modifiers::MAC_CMD | Modifiers::COMMAND;
    let blank = harness.state().rows().unwrap().len();
    press(&mut harness, cmd, Key::ArrowDown);
    assert_eq!(at(harness.state()), (blank, Column::Duration));
    press(&mut harness, cmd, Key::Home);
    assert_eq!(at(harness.state()), (0, Column::Id));
    press(&mut harness, cmd, Key::ArrowRight);
    assert_eq!(at(harness.state()), (0, Column::Predecessors));
    // Ctrl physique sur Mac : pas l'équivalent de Cmd ; aucun raccourci
    // (egui n'ignore pas un Ctrl en trop).
    press(&mut harness, Modifiers::CTRL, Key::ArrowUp);
    assert_eq!(at(harness.state()), (0, Column::Predecessors));
}

/// H-UI-16 : ligne 3, Maj+↓ ×2 ⇒ lignes 3 à 5, curseur ligne 5 ; ↓ seul
/// ensuite ⇒ ligne 6 seule.
#[test]
fn h_ui_16_shift_arrows_extend_rows() {
    let mut harness = demo_at(3, Column::Name);
    press(&mut harness, SHIFT, Key::ArrowDown);
    press(&mut harness, SHIFT, Key::ArrowDown);
    let app = harness.state();
    assert_eq!(at(app), (5, Column::Name));
    let range: Vec<TaskUid> = (3..=5).map(|i| uid_at(app, i)).collect();
    assert_eq!(app.selected().click_order(), range.as_slice());
    assert_eq!(app.view().cursor.anchor, Some(uid_at(app, 3)));
    // Maj+↑ réduit la plage depuis la même ancre.
    press(&mut harness, SHIFT, Key::ArrowUp);
    let app = harness.state();
    let range: Vec<TaskUid> = (3..=4).map(|i| uid_at(app, i)).collect();
    assert_eq!(app.selected().click_order(), range.as_slice());
    press(&mut harness, Modifiers::NONE, Key::ArrowDown);
    let app = harness.state();
    assert_eq!(at(app), (5, Column::Name));
    assert_eq!(app.selected().click_order(), [uid_at(app, 5)]);
    assert_eq!(app.view().cursor.anchor, None);
    // Ctrl+Maj+↓ : jusqu'à la dernière tâche (jamais la ligne vide) ;
    // Maj+Page haut depuis là.
    press(&mut harness, COMMAND | SHIFT, Key::ArrowDown);
    let app = harness.state();
    let last = app.rows().unwrap().len() - 1;
    assert_eq!(at(app), (last, Column::Name));
    assert_eq!(app.selected().len(), last - 5 + 1);
    press(&mut harness, SHIFT, Key::PageUp);
    let app = harness.state();
    let page = nav::page_rows(app.view().last_body_height, ROW_H);
    assert_eq!(at(app), (last - (page - 1), Column::Name));
    assert_eq!(app.selected().len(), last - (page - 1) - 5 + 1);
}

/// H-UI-17 : le défilement suit le curseur, une ligne de marge sauf aux
/// bords.
#[test]
fn h_ui_17_cursor_scrolls_into_view() {
    let mut harness = demo_at(0, Column::Name);
    let len = harness.state().rows().unwrap().len() + 1;
    for _ in 0..len {
        press(&mut harness, Modifiers::NONE, Key::ArrowDown);
        let app = harness.state();
        let (i, _) = at(app);
        let (top, h) = (app.view().last_scroll_y, app.view().last_body_height);
        let below = ((i + 2).min(len)) as f32 * ROW_H;
        assert!(below <= top + h + 0.5, "ligne {i} : bas {below} > {top} + {h}");
        assert!(i.saturating_sub(1) as f32 * ROW_H >= top - 0.5, "ligne {i}");
    }
    assert!(harness.state().view().last_scroll_y > 0.0);
    for _ in 0..len {
        press(&mut harness, Modifiers::NONE, Key::ArrowUp);
        let app = harness.state();
        let (i, _) = at(app);
        assert!(
            i.saturating_sub(1) as f32 * ROW_H >= app.view().last_scroll_y - 0.5,
            "ligne {i}"
        );
    }
    assert!(harness.state().view().last_scroll_y.abs() < 0.5);
    // Grand saut : Ctrl+Fin montre la ligne vide.
    press(&mut harness, COMMAND, Key::End);
    let app = harness.state();
    assert!(
        app.last_frame.table_rows.contains(&(len - 1)),
        "{:?}",
        app.last_frame.table_rows
    );
}

/// Défilement horizontal maximal du Gantt (au zoom courant).
fn max_scroll(app: &App) -> f32 {
    crate::max_scroll(app)
}

/// H-UI-18 : Alt+←/→ un jour, Alt+Page une largeur, Alt+Origine/Fin début
/// et fin du projet ; Ctrl+/, Ctrl+=, Ctrl++ plus fin, Ctrl+- plus
/// grossier (zoom ancré au centre).
#[test]
fn h_ui_18_alt_scrolls_timescale() {
    let mut harness = demo_at(2, Column::Name);
    harness.state_mut().dispatch(Action::SetZoom(TimeScaleZoom::Day));
    harness.run();
    harness.state_mut().dispatch(Action::ScrollGanttX(1_000.0));
    harness.run();
    let x0 = harness.state().view().gantt_scroll_x;
    assert!(x0 > 100.0 && x0 < max_scroll(harness.state()) - 100.0, "{x0}");
    press(&mut harness, Modifiers::ALT, Key::ArrowRight);
    let x1 = harness.state().view().gantt_scroll_x;
    assert!(
        (x1 - x0 - TimeScaleZoom::Day.px_per_day()).abs() < 0.01,
        "{x0} → {x1}"
    );
    press(&mut harness, Modifiers::ALT, Key::ArrowLeft);
    assert!((harness.state().view().gantt_scroll_x - x0).abs() < 0.01);
    // Alt+flèche ne déplace pas le curseur de la table.
    assert_eq!(at(harness.state()), (2, Column::Name));
    let width = harness.state().view().last_gantt_width;
    press(&mut harness, Modifiers::ALT, Key::PageDown);
    let x2 = harness.state().view().gantt_scroll_x;
    assert!((x2 - (x0 + width).min(max_scroll(harness.state()))).abs() < 0.01);
    press(&mut harness, Modifiers::ALT, Key::PageUp);
    assert!((harness.state().view().gantt_scroll_x - x0).abs() < 0.01);
    press(&mut harness, Modifiers::ALT, Key::Home);
    let app = harness.state();
    let doc = app.document.as_ref().unwrap();
    let axis = TimeAxis::new(doc.range, app.state.zoom, 0.0);
    let start = axis.x(doc.schedule().project_start);
    assert!((app.view().gantt_scroll_x - start.min(max_scroll(app))).abs() < 0.01);
    press(&mut harness, Modifiers::ALT, Key::End);
    let app = harness.state();
    let doc = app.document.as_ref().unwrap();
    let finish = axis.x(doc.schedule().project_finish) - app.view().last_gantt_width;
    assert!((app.view().gantt_scroll_x - finish.clamp(0.0, max_scroll(app))).abs() < 0.01);
    // Zoom au clavier.
    press(&mut harness, COMMAND, Key::Minus);
    assert_eq!(harness.state().state.zoom, TimeScaleZoom::Week);
    press(&mut harness, COMMAND, Key::Minus);
    assert_eq!(harness.state().state.zoom, TimeScaleZoom::Month);
    press(&mut harness, COMMAND, Key::Slash);
    assert_eq!(harness.state().state.zoom, TimeScaleZoom::Week);
    press(&mut harness, COMMAND, Key::Equals);
    assert_eq!(harness.state().state.zoom, TimeScaleZoom::Day);
    press(&mut harness, COMMAND, Key::Minus);
    press(&mut harness, COMMAND, Key::Plus);
    assert_eq!(harness.state().state.zoom, TimeScaleZoom::Day);
    // Le zoom de l'interface d'egui (Ctrl+±) n'a pas joué.
    assert!((harness.ctx.zoom_factor() - 1.0).abs() < f32::EPSILON);
}

/// Trois sœurs consécutives sans enfant (positions dans l'ordre des ID).
fn three_leaf_siblings(app: &App) -> usize {
    let project = app.document.as_ref().unwrap().project();
    let level = |i: usize| project.tasks.at(i).map(|t| t.outline_level.get());
    (0..project.tasks.len())
        .find(|&i| {
            let l = level(i);
            l.is_some() && l == level(i + 1) && l == level(i + 2) && level(i + 3) <= l
        })
        .expect("trois sœurs feuilles dans l'exemple")
}

/// H-UI-29 : lignes 4-5, Alt+Maj+↑ ⇒ 3-4 ; Alt+Maj+↓ les ramène.
#[test]
fn h_ui_29_alt_shift_up_moves_task() {
    let mut harness = harness();
    open_demo(&mut harness);
    let p = three_leaf_siblings(harness.state());
    let (prev, a, b) = {
        let app = harness.state();
        (uid_at(app, p), uid_at(app, p + 1), uid_at(app, p + 2))
    };
    click_cell(&mut harness, p + 1, Column::Name);
    press(&mut harness, SHIFT, Key::ArrowDown);
    assert_eq!(harness.state().selected().len(), 2);
    press(&mut harness, ALT_SHIFT, Key::ArrowUp);
    let app = harness.state();
    assert!(app.messages().is_empty(), "{:?}", app.messages());
    let rows = app.rows().unwrap();
    assert_eq!(
        [rows.index_of(a), rows.index_of(b), rows.index_of(prev)],
        [Some(p), Some(p + 1), Some(p + 2)]
    );
    // Une seule entrée d'historique ; sélection et curseur suivent les UID.
    assert_eq!(app.document.as_ref().unwrap().session.history().entries.len(), 1);
    assert_eq!(app.selected().len(), 2);
    assert_eq!(at(app).0, p + 1);
    press(&mut harness, ALT_SHIFT, Key::ArrowDown);
    let rows = harness.state().rows().unwrap();
    assert_eq!(
        [rows.index_of(prev), rows.index_of(a), rows.index_of(b)],
        [Some(p), Some(p + 1), Some(p + 2)]
    );
    // Première tâche du projet : rien à monter.
    click_cell(&mut harness, 0, Column::Name);
    let generation = harness.state().document.as_ref().unwrap().generation();
    press(&mut harness, ALT_SHIFT, Key::ArrowUp);
    assert_eq!(
        harness.state().document.as_ref().unwrap().generation(),
        generation
    );
}

/// Tab ne sort jamais de la table (verrou de focus d'egui) : focus gardé
/// sur toute une ligne et au bord.
#[test]
fn nav_tab_not_stolen_by_egui_focus() {
    let mut harness = demo_at(1, Column::Id);
    let cols = visible(harness.state()).len();
    for k in 1..cols + 3 {
        press(&mut harness, Modifiers::NONE, Key::Tab);
        assert!(table_focused(&harness), "Tab n° {k}");
        let column = visible(harness.state())[k.min(cols - 1)];
        assert_eq!(at(harness.state()), (1, column));
    }
    for _ in 0..cols + 3 {
        press(&mut harness, SHIFT, Key::Tab);
        assert!(table_focused(&harness));
    }
    assert_eq!(at(harness.state()), (1, Column::Id));
    // Les flèches non plus ne déplacent pas le focus d'egui.
    for key in [Key::ArrowDown, Key::ArrowRight, Key::ArrowUp, Key::ArrowLeft] {
        press(&mut harness, Modifiers::NONE, key);
        assert!(table_focused(&harness), "{key:?}");
    }
}

/// Pendant une saisie, la navigation est inactive : Origine, Fin, Page
/// restent à la zone de saisie (←/→ au texte).
#[test]
fn nav_disabled_while_editing() {
    let mut harness = demo_at(3, Column::Name);
    harness.event(Event::Text("Abc".to_owned()));
    harness.run();
    for (modifiers, key) in [
        (Modifiers::NONE, Key::Home),
        (Modifiers::NONE, Key::End),
        (Modifiers::NONE, Key::PageDown),
        (Modifiers::NONE, Key::ArrowLeft),
        (Modifiers::NONE, Key::ArrowRight),
        (COMMAND, Key::End),
        (SHIFT, Key::PageDown),
    ] {
        press(&mut harness, modifiers, key);
        let app = harness.state();
        assert!(app.view().edit.editing.is_some(), "{modifiers:?} {key:?}");
        assert_eq!(at(app), (3, Column::Name), "{modifiers:?} {key:?}");
    }
    assert_eq!(
        harness.state().view().edit.editing.as_ref().unwrap().buffer,
        "Abc"
    );
}

/// Menu ouvert : la navigation est inactive.
#[test]
fn nav_disabled_with_menu_open() {
    let mut harness = demo_at(3, Column::Name);
    crate::click(&mut harness, &fr(TextId::MenuFile));
    assert!(harness.ctx.any_popup_open() || !table_focused(&harness));
    // (Ni Entrée ni Tab : ils activeraient ou parcourraient le menu.)
    for key in [Key::ArrowDown, Key::PageDown, Key::End] {
        press(&mut harness, Modifiers::NONE, key);
        assert_eq!(at(harness.state()), (3, Column::Name), "{key:?}");
    }
}

/// À l'ouverture : focus à la table, curseur ligne 1, colonne Nom ; ↓
/// descend aussitôt, sans clic.
#[test]
fn nav_after_open_focuses_table() {
    let mut harness = harness();
    open_demo(&mut harness);
    assert!(table_focused(&harness));
    assert_eq!(at(harness.state()), (0, Column::Name));
    press(&mut harness, Modifiers::NONE, Key::ArrowDown);
    assert_eq!(at(harness.state()), (1, Column::Name));
    // Un clic dans le Gantt laisse le clavier à la table (ADR-0018 D8,
    // H-UI-101, remplace J7 L1) : Fin agit sur la ligne cliquée.
    let gantt = {
        let app = harness.state();
        pos2(app.view().last_gantt_left + 50.0, 300.0)
    };
    harness.event(Event::PointerMoved(gantt));
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: gantt,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run();
    assert!(table_focused(&harness));
    let (row, _) = at(harness.state());
    press(&mut harness, Modifiers::NONE, Key::End);
    let last = *visible(harness.state()).last().unwrap();
    assert_eq!(at(harness.state()), (row, last));
    // Échap rend le focus ; un clic dans la table le reprend.
    click_cell(&mut harness, 2, Column::Name);
    press(&mut harness, Modifiers::NONE, Key::Escape);
    assert!(!table_focused(&harness));
    click_cell(&mut harness, 2, Column::Duration);
    press(&mut harness, Modifiers::NONE, Key::ArrowDown);
    assert_eq!(at(harness.state()), (3, Column::Duration));
}

/// Relecture finale J7, F5 : un clic sur le ruban (barre d'outils jusqu'au
/// lot L5 ; ou un menu, ou
/// la réponse d'une confirmation) retirait le focus de la table sans le
/// rendre : ↓ ne faisait plus rien. La table le reprend ; un clic dans le
/// Gantt, lui, le laisse.
#[test]
fn nav_focus_back_after_ribbon_click() {
    let mut harness = harness();
    open_demo(&mut harness);
    assert!(table_focused(&harness));
    ribbon_click(&mut harness, RibbonTab::View, &fr(TextId::RibbonEntireProject));
    assert!(table_focused(&harness), "focus perdu après le ruban");
    let before = at(harness.state());
    press(&mut harness, Modifiers::NONE, Key::ArrowDown);
    assert_eq!(at(harness.state()).0, before.0 + 1);
    // Menu Édition, puis Échap qui le ferme : la navigation reprend.
    crate::click(&mut harness, &crate::fr(TextId::MenuEdit));
    harness.key_press(Key::Escape);
    harness.run();
    ribbon_click(&mut harness, RibbonTab::View, &fr(TextId::RibbonEntireProject));
    let before = at(harness.state());
    press(&mut harness, Modifiers::NONE, Key::ArrowDown);
    assert_eq!(at(harness.state()).0, before.0 + 1);
}

/// Cadre du curseur dessiné (une forme de plus) et cellule courante
/// accessible : « Ligne 3, Durée, 5 jours ».
#[test]
fn cursor_drawn_and_accessible() {
    let mut harness = demo_at(2, Column::Duration);
    let texts = Texts::new(Language::Fr);
    let label = |app: &App, i: usize, column: Column| {
        let rows = app.rows().unwrap();
        texts.get_args(
            TextId::NavCursorLabel,
            &[
                ("id", Arg::Count(i64::from(rows.row(i).unwrap().id))),
                ("column", Arg::Str(texts.get(column.title()).into())),
                ("value", Arg::Str(rows.text(i, column).to_owned().into())),
            ],
        )
    };
    let expected = label(harness.state(), 2, Column::Duration);
    assert!(expected.contains("Durée"), "{expected}");
    let node = harness.get_by_label(&expected);
    let p = cell_point(&harness, 2, Column::Duration);
    assert!(node.rect().contains(p));
    press(&mut harness, Modifiers::NONE, Key::ArrowDown);
    assert!(harness.query_by_label(&expected).is_none());
    let next = label(harness.state(), 3, Column::Duration);
    assert!(harness.query_by_label(&next).is_some(), "{next}");
    // Ligne vide.
    press(&mut harness, COMMAND, Key::ArrowDown);
    let blank = texts.get_args(
        TextId::NavCursorBlankLabel,
        &[("column", Arg::Str(texts.get(Column::Duration.title()).into()))],
    );
    assert!(harness.query_by_label(&blank).is_some(), "{blank}");
    // Pendant une saisie, ni cadre ni nœud.
    press(&mut harness, COMMAND, Key::ArrowUp);
    let first = label(harness.state(), 0, Column::Duration);
    let shapes = harness.state().last_frame.shapes;
    press(&mut harness, Modifiers::NONE, Key::F2);
    assert!(harness.state().view().edit.editing.is_some());
    assert!(harness.query_by_label(&first).is_none());
    press(&mut harness, Modifiers::NONE, Key::Escape);
    assert!(harness.query_by_label(&first).is_some());
    assert_eq!(harness.state().last_frame.shapes, shapes);
}

/// Vérifie l'entrée `i` de [`KEYMAP`] liée à J7 L1 (navigation, échelle de
/// temps, zoom, `task.move`) : son action au clavier, et pour une entrée
/// de la table, rien sans le focus de la table.
pub fn check_keymap_entry(i: usize) {
    let (context, shortcut, binding) = KEYMAP[i];
    let mut harness = harness_for(new_app());
    open_demo(&mut harness);
    click_cell(&mut harness, 6, Column::Duration);
    match binding {
        Binding::Ui(UiBinding::Nav { mv, extend }) => {
            assert_eq!(context, KeyContext::Table);
            let want = expected(harness.state(), mv);
            let want = if extend {
                (want.0.min(harness.state().rows().unwrap().len() - 1), want.1)
            } else {
                want
            };
            press(&mut harness, shortcut.modifiers, shortcut.logical_key);
            let app = harness.state();
            assert_eq!(at(app), want, "{shortcut:?}");
            assert!(app.view().edit.editing.is_none());
            let span = want.0.abs_diff(6) + 1;
            assert_eq!(
                app.selected().len(),
                if extend {
                    span
                } else {
                    usize::from(app.selection.is_some())
                }
            );
            // Sans le focus de la table : rien.
            let before = at(harness.state());
            press(&mut harness, Modifiers::NONE, Key::Escape);
            assert!(!table_focused(&harness));
            press(&mut harness, shortcut.modifiers, shortcut.logical_key);
            assert_eq!(at(harness.state()), before, "{shortcut:?} sans focus");
        }
        Binding::Ui(
            ui @ (UiBinding::TimescaleStep(_)
            | UiBinding::TimescalePage(_)
            | UiBinding::TimescaleToStart
            | UiBinding::TimescaleToEnd),
        ) => {
            harness.state_mut().dispatch(Action::SetZoom(TimeScaleZoom::Day));
            harness.run();
            harness.state_mut().dispatch(Action::ScrollGanttX(1_500.0));
            harness.run();
            let x0 = harness.state().view().gantt_scroll_x;
            press(&mut harness, shortcut.modifiers, shortcut.logical_key);
            let app = harness.state();
            let x = app.view().gantt_scroll_x;
            let width = app.view().last_gantt_width;
            let doc = app.document.as_ref().unwrap();
            let axis = TimeAxis::new(doc.range, app.state.zoom, 0.0);
            let want = match ui {
                UiBinding::TimescaleStep(n) => x0 + f32::from(n) * TimeScaleZoom::Day.px_per_day(),
                UiBinding::TimescalePage(n) => x0 + f32::from(n) * width,
                UiBinding::TimescaleToStart => axis.x(doc.schedule().project_start),
                _ => axis.x(doc.schedule().project_finish) - width,
            };
            assert!(
                (x - want.clamp(0.0, max_scroll(app))).abs() < 0.01,
                "{shortcut:?} : {x} ≠ {want}"
            );
            assert_eq!(at(app), (6, Column::Duration), "le curseur ne bouge pas");
        }
        Binding::Ui(ui @ (UiBinding::ZoomFiner | UiBinding::ZoomCoarser)) => {
            harness.state_mut().dispatch(Action::SetZoom(TimeScaleZoom::Week));
            harness.run();
            press(&mut harness, shortcut.modifiers, shortcut.logical_key);
            let want = if ui == UiBinding::ZoomFiner {
                TimeScaleZoom::Day
            } else {
                TimeScaleZoom::Month
            };
            assert_eq!(harness.state().state.zoom, want, "{shortcut:?}");
        }
        Binding::Command("task.move") => {
            let p = three_leaf_siblings(harness.state());
            click_cell(&mut harness, p + 1, Column::Name);
            let uid = uid_at(harness.state(), p + 1);
            press(&mut harness, shortcut.modifiers, shortcut.logical_key);
            let index = harness.state().rows().unwrap().index_of(uid);
            let want = if shortcut.logical_key == Key::ArrowUp {
                p
            } else {
                p + 2
            };
            assert_eq!(index, Some(want), "{shortcut:?}");
        }
        other => panic!("entrée {i} hors du lot L1 : {other:?}"),
    }
}

/// Vrai si l'entrée `i` de [`KEYMAP`] est vérifiée par
/// [`check_keymap_entry`].
pub fn is_nav_entry(binding: Binding) -> bool {
    matches!(
        binding,
        Binding::Ui(
            UiBinding::Nav { .. }
                | UiBinding::TimescaleStep(_)
                | UiBinding::TimescalePage(_)
                | UiBinding::TimescaleToStart
                | UiBinding::TimescaleToEnd
                | UiBinding::ZoomFiner
                | UiBinding::ZoomCoarser
        ) | Binding::Command("task.move")
    )
}

fn nav_move_strategy() -> impl Strategy<Value = (NavMove, bool)> {
    (proptest::sample::select(NavMove::ALL.to_vec()), any::<bool>())
}

/// Nombre de cas de P45 côté application, borné quelle que soit
/// `PROPTEST_CASES` : chaque cas pilote une application `egui_kittest`
/// (≈ 1 s), et la macro `proptest!` remplace `with_cases` par la variable
/// (`contextualize_config`, CI à 10 000 : plus de 1 h 30).
const P45_APP_CASES: u32 = 48;

/// P45 côté application : après toute suite de déplacements, le
/// curseur est dans la grille, la ligne du curseur est la tâche active,
/// et hors extension la sélection est la seule ligne du curseur.
///
/// `TestRunner` construit à la main : `Config::with_cases` fixe `cases`
/// après les variables d'environnement (les autres, graine comprise,
/// restent prises en compte), et rien ne la recontextualise ensuite.
#[test]
fn p45_selection_follows_cursor() {
    let config = ProptestConfig {
        source_file: Some(file!()),
        test_name: Some(concat!(module_path!(), "::p45_selection_follows_cursor")),
        ..ProptestConfig::with_cases(P45_APP_CASES)
    };
    let mut runner = proptest::test_runner::TestRunner::new(config);
    let strategy = proptest::collection::vec(nav_move_strategy(), 1..24);
    let result = runner.run(&strategy, |moves| {
        let mut harness = harness();
        open_demo(&mut harness);
        for (mv, extend) in moves {
            let before = expected(harness.state(), mv);
            harness.state_mut().dispatch(Action::Nav { mv, extend });
            harness.run();
            let app = harness.state();
            let (row, column) = at(app);
            let s = shape(app);
            prop_assert!(row < s.rows);
            prop_assert!(visible(app).contains(&column));
            if extend {
                prop_assert_eq!(column, before.1);
                prop_assert!(app.selection.is_some());
            } else {
                prop_assert_eq!((row, column), before);
                prop_assert_eq!(app.selected().len(), usize::from(app.selection.is_some()));
                prop_assert_eq!(app.selection, app.rows().unwrap().row(row).map(|r| r.uid));
            }
        }
        Ok(())
    });
    if let Err(e) = result {
        panic!("{e}\n{runner}");
    }
}
