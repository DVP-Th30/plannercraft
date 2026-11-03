//! Sélection au clavier (passe UX, ADR-0018) : raccourcis de H-UI-92,
//! H-UI-96, H-UI-97 (contrat L0 : entrées de `KEYMAP` vérifiées par
//! `check_keymap_entry`, appelé par `session.rs`) ; tests du lot L1
//! `h_ui_92_*`, `h_ui_93_*`, `h_ui_101_*`.
//!
//! Lot L1 : Ctrl/Cmd+A sélectionne tout en lignes entières ; Maj+Espace
//! passe la sélection en lignes entières ; Ctrl+Espace en colonnes
//! entières ; Maj+Retour arrière la réduit à la cellule active ; Ctrl/Cmd+D
//! et Ctrl/Cmd+Suppr sont servis par le lot L2 (« pas encore disponible »
//! tant qu'il ne l'est pas).

#![allow(
    clippy::arithmetic_side_effects,
    reason = "tests : indices bornés par l'exemple"
)]

use egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::i18n::{Language, TextId, Texts};
use plannercraft_ui_egui::keymap::{Binding, KEYMAP, KeyContext, KeyScope, UiBinding, web_interceptable};
use plannercraft_ui_egui::nav::RowRef;
use plannercraft_ui_egui::row_view::DisplayRow;
use plannercraft_ui_egui::selection::SelectionKind;
use plannercraft_ui_egui::view::Pane;
use plannercraft_ui_egui::view::table::table_id;
use plannercraft_ui_egui::{Action, App};

use crate::select_mouse::{block, cell_point, click_at, header_point, table_focused, uid_at};
use crate::{harness, open_demo};

/// Exemple ouvert, ligne `i` sélectionnée, table focalisée.
fn demo_on_row(i: usize) -> Harness<'static, App> {
    let mut harness = harness();
    open_demo(&mut harness);
    let uid = harness.state().rows().unwrap().row(i).unwrap().uid;
    harness.state_mut().dispatch(Action::Select(Some(uid)));
    harness.state_mut().dispatch(Action::FocusPane(Pane::Table));
    harness.run();
    assert!(harness.ctx.memory(|memory| memory.has_focus(table_id())));
    harness
}

/// Vrai si l'entrée de [`KEYMAP`] est vérifiée par [`check_keymap_entry`].
pub fn is_ux_entry(binding: Binding) -> bool {
    matches!(
        binding,
        Binding::Ui(
            UiBinding::SelectAll
                | UiBinding::SelectRows
                | UiBinding::SelectColumns
                | UiBinding::CollapseSelection
                | UiBinding::FillDown
                | UiBinding::ClearCells
        )
    )
}

/// Vérifie l'entrée `i` de [`KEYMAP`] de la passe UX (contexte `Table`) :
/// son effet en L0, et rien sans le focus de la table.
pub fn check_keymap_entry(i: usize) {
    let (context, shortcut, binding) = KEYMAP[i];
    assert_eq!(context, KeyContext::Table, "{shortcut:?}");
    let mut harness = demo_on_row(3);
    let before = (harness.state().selected().clone(), harness.state().view().cursor);
    harness.key_press_modifiers(shortcut.modifiers, shortcut.logical_key);
    harness.run();
    let app = harness.state();
    assert!(app.view().edit.editing.is_none(), "{shortcut:?}");
    // Ctrl/Cmd+D et Ctrl/Cmd+Suppr écrivent un lot depuis le lot L2 ;
    // les gestes de sélection n'écrivent rien.
    let writes = matches!(binding, Binding::Ui(UiBinding::FillDown | UiBinding::ClearCells));
    assert_eq!(
        app.document.as_ref().unwrap().generation(),
        u64::from(writes),
        "{shortcut:?}"
    );
    match binding {
        Binding::Ui(UiBinding::SelectAll) => {
            let rows = app.rows().unwrap().len();
            assert_eq!(app.selected().len(), rows, "{shortcut:?}");
            assert_eq!(app.selected().kind(), SelectionKind::Rows);
            assert_eq!(app.selection, Some(app.rows().unwrap().row(0).unwrap().uid));
        }
        Binding::Ui(UiBinding::SelectRows) => {
            assert_eq!(app.selected().kind(), SelectionKind::Rows, "{shortcut:?}");
            assert_eq!(app.selected().uids(), before.0.uids());
        }
        Binding::Ui(UiBinding::SelectColumns) => {
            assert_eq!(app.selected().len(), app.rows().unwrap().len());
            assert_eq!(app.selected().kind(), SelectionKind::Cells);
            assert_eq!(app.selection, before.0.focus(), "cellule active gardée");
            assert_eq!(block(app).columns, [before.1.column]);
        }
        Binding::Ui(UiBinding::CollapseSelection) => {
            // Une cellule déjà seule : rien ne change.
            assert_eq!(
                (app.selected().clone(), app.view().cursor),
                before,
                "{shortcut:?}"
            );
        }
        Binding::Ui(UiBinding::FillDown | UiBinding::ClearCells) => {
            // Lot L2 : servis (Nom de la ligne 3 rempli depuis la ligne 2,
            // ou vidé), une entrée d'annulation.
            let text = Texts::new(Language::Fr).get(TextId::UxNotAvailable);
            assert!(
                !app.messages().iter().any(|m| m.text == text),
                "{shortcut:?} : {:?}",
                app.messages()
            );
        }
        other => panic!("entrée {i} hors de la passe UX : {other:?}"),
    }
    // Sans le focus de la table : rien.
    let mut harness = demo_on_row(3);
    harness.state_mut().dispatch(Action::FocusPane(Pane::Gantt));
    harness.run();
    let before = (
        harness.state().selected().clone(),
        harness.state().messages().len(),
    );
    harness.key_press_modifiers(shortcut.modifiers, shortcut.logical_key);
    harness.run();
    let app = harness.state();
    assert_eq!(
        (app.selected().clone(), app.messages().len()),
        before,
        "{shortcut:?} sans focus"
    );
}

/// Exemple ouvert, cellule (`i`, `column`) cliquée.
fn demo_at(i: usize, column: Column) -> Harness<'static, App> {
    let mut harness = harness();
    open_demo(&mut harness);
    let p = cell_point(&harness, i, column);
    click_at(&mut harness, p, Modifiers::NONE);
    assert!(table_focused(&harness));
    harness
}

/// Appuie sur `key` avec `modifiers`, puis stabilise.
fn press(harness: &mut Harness<'static, App>, modifiers: Modifiers, key: Key) {
    harness.key_press_modifiers(modifiers, key);
    harness.run();
}

/// Lignes affichées `from..=to`.
fn display(from: u32, to: u32) -> Vec<DisplayRow> {
    (from..=to).map(DisplayRow).collect()
}

const DUR: Column = Column::Duration;
const START: Column = Column::Start;
const FINISH: Column = Column::Finish;

/// H-UI-92 (U02) : (3, Durée), Maj+→ ⇒ bloc Durée-Début de la ligne 3 ;
/// encore ⇒ Durée-Fin ; Maj+← ⇒ Durée-Début ; Maj+↓ ⇒ lignes 3-4, mêmes
/// colonnes ; Maj+Fin ⇒ jusqu'au dernier champ ; ← seul ⇒ une cellule.
#[test]
fn h_ui_92_shift_right_extends() {
    let mut harness = demo_at(3, DUR);
    press(&mut harness, Modifiers::SHIFT, Key::ArrowRight);
    let app = harness.state();
    let b = block(app);
    assert_eq!(b.rows, display(3, 3));
    assert_eq!(b.columns, [DUR, START]);
    assert_eq!(b.active, (RowRef::Item(uid_at(app, 3)), START));
    press(&mut harness, Modifiers::SHIFT, Key::ArrowRight);
    assert_eq!(block(harness.state()).columns, [DUR, START, FINISH]);
    press(&mut harness, Modifiers::SHIFT, Key::ArrowLeft);
    assert_eq!(block(harness.state()).columns, [DUR, START]);
    press(&mut harness, Modifiers::SHIFT, Key::ArrowDown);
    let b = block(harness.state());
    assert_eq!(b.rows, display(3, 4));
    assert_eq!(b.columns, [DUR, START]);
    press(&mut harness, Modifiers::SHIFT, Key::End);
    let app = harness.state();
    let visible: Vec<Column> = app.state.visible_columns().map(|c| c.column).collect();
    let b = block(app);
    assert_eq!(b.columns.first(), Some(&DUR));
    assert_eq!(b.columns.last(), visible.last());
    press(&mut harness, Modifiers::NONE, Key::ArrowLeft);
    assert!(block(harness.state()).is_single());
}

/// H-UI-92 (U06) : Ctrl/Cmd+A ⇒ 49 lignes entières sur 49, cellule active
/// (première ligne, Nom).
#[test]
fn h_ui_92_cmd_a_selects_all() {
    let mut harness = demo_at(5, DUR);
    press(&mut harness, Modifiers::COMMAND, Key::A);
    let app = harness.state();
    assert_eq!(app.rows().unwrap().len(), 49);
    assert_eq!(app.selected().len(), 49);
    let b = block(app);
    assert_eq!(b.kind, SelectionKind::Rows);
    assert_eq!(b.rows.len(), 49);
    assert_eq!(b.active, (RowRef::Item(uid_at(app, 0)), Column::Name));
    assert!(app.view().edit.editing.is_none());
}

/// Maj+Espace (ou Ctrl+Espace) dans une seule image, comme un clavier :
/// la touche et le texte de l'espace.
fn chorded_space(harness: &mut Harness<'static, App>, modifiers: Modifiers) {
    let events = &mut harness.input_mut().events;
    events.push(Event::ModifiersChanged(modifiers));
    events.push(Event::Key {
        key: Key::Space,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    });
    events.push(Event::Text(" ".to_owned()));
    events.push(Event::Key {
        key: Key::Space,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers,
    });
    events.push(Event::ModifiersChanged(Modifiers::NONE));
    harness.step();
    harness.run();
}

/// H-UI-92 : Maj+Espace ⇒ lignes entières de la sélection, sans taper
/// d'espace (aucune saisie ouverte).
#[test]
fn h_ui_92_shift_space_rows_no_typing() {
    let mut harness = demo_at(3, DUR);
    press(&mut harness, Modifiers::SHIFT, Key::ArrowDown);
    chorded_space(&mut harness, Modifiers::SHIFT);
    let app = harness.state();
    assert!(app.view().edit.editing.is_none(), "aucune frappe");
    let b = block(app);
    assert_eq!(b.kind, SelectionKind::Rows);
    assert_eq!(b.rows, display(3, 4));
    let generation = app.document.as_ref().unwrap().generation();
    assert_eq!(generation, 0);
    // Un espace seul ouvre toujours la saisie (frappe).
    let mut harness = demo_at(3, Column::Name);
    harness.event(Event::Text(" ".to_owned()));
    harness.run();
    assert!(harness.state().view().edit.editing.is_some());
}

/// H-UI-92 : Ctrl+Espace ⇒ colonnes entières du bloc (toutes les lignes
/// affichées), cellule active gardée.
#[test]
fn h_ui_92_ctrl_space_columns() {
    let mut harness = demo_at(3, DUR);
    press(&mut harness, Modifiers::SHIFT, Key::ArrowRight);
    chorded_space(&mut harness, Modifiers::CTRL);
    let app = harness.state();
    assert!(app.view().edit.editing.is_none());
    let b = block(app);
    assert_eq!(b.kind, SelectionKind::Cells);
    assert_eq!(b.columns, [DUR, START]);
    assert_eq!(b.rows.len(), 49);
    assert_eq!(b.active, (RowRef::Item(uid_at(app, 3)), START));
    // Depuis des lignes entières : toutes les colonnes.
    chorded_space(&mut harness, Modifiers::SHIFT);
    chorded_space(&mut harness, Modifiers::CTRL);
    let app = harness.state();
    let visible: Vec<Column> = app.state.visible_columns().map(|c| c.column).collect();
    let b = block(app);
    assert_eq!(b.kind, SelectionKind::Cells);
    assert_eq!(b.columns, visible);
}

/// H-UI-92 : Maj+Retour arrière réduit le bloc à la cellule active, sans
/// ouvrir de saisie.
#[test]
fn h_ui_92_shift_backspace_collapses() {
    let mut harness = demo_at(3, DUR);
    press(&mut harness, Modifiers::SHIFT, Key::ArrowDown);
    press(&mut harness, Modifiers::SHIFT, Key::ArrowDown);
    press(&mut harness, Modifiers::SHIFT, Key::ArrowRight);
    assert_eq!(block(harness.state()).cells(), 6);
    press(&mut harness, Modifiers::SHIFT, Key::Backspace);
    let app = harness.state();
    let b = block(app);
    assert!(b.is_single(), "{b:?}");
    assert_eq!(b.active, (RowRef::Item(uid_at(app, 5)), START));
    assert!(app.view().edit.editing.is_none());
}

/// H-UI-92 : sur le web, les raccourcis de sélection de la table sont
/// retenus au navigateur quand la table a le clavier (`KeyScope`), et
/// relâchés pendant une saisie.
#[test]
fn h_ui_92_web_intercepts() {
    let mut harness = demo_at(3, DUR);
    let scope = harness.state().key_scope(&harness.ctx);
    let mut seen = 0;
    for (context, shortcut, binding) in KEYMAP {
        let ux = is_ux_entry(*binding) || matches!(binding, Binding::Ui(UiBinding::Nav { extend: true, .. }));
        if !ux {
            continue;
        }
        seen += 1;
        assert_eq!(*context, KeyContext::Table, "{shortcut:?}");
        assert!(web_interceptable(shortcut, *binding), "{shortcut:?}");
        assert!(scope.reads(*context, *binding), "{shortcut:?}");
    }
    assert!(seen >= 10, "{seen}");
    // Saisie ouverte : tout reste au navigateur.
    harness.key_press(Key::F2);
    harness.run();
    let scope = harness.state().key_scope(&harness.ctx);
    assert_eq!(scope, KeyScope::Nothing);
}

/// H-UI-93 : bloc 2-3 × Durée-Début, cellule active (3, Début) : Entrée
/// circule dans le bloc (bas, puis haut de la colonne suivante, puis retour
/// au début), Tab à droite puis au début de la ligne suivante, Maj+Entrée
/// et Maj+Tab à l'inverse ; la sélection ne change pas. Une cellule active
/// au milieu d'un bloc de trois colonnes le garde entier. Une flèche
/// réduit le bloc.
#[test]
fn h_ui_93_enter_tab_cycle_in_block() {
    let mut harness = demo_at(2, DUR);
    press(&mut harness, Modifiers::SHIFT, Key::ArrowDown);
    press(&mut harness, Modifiers::SHIFT, Key::ArrowRight);
    let app = harness.state();
    let (two, three) = (uid_at(app, 2), uid_at(app, 3));
    let at = |harness: &Harness<'static, App>| {
        let b = block(harness.state());
        assert_eq!(b.rows, display(2, 3), "lignes gardées");
        assert_eq!(b.columns, [DUR, START], "colonnes gardées");
        assert_eq!(harness.state().selected().len(), 2);
        b.active
    };
    assert_eq!(at(&harness), (RowRef::Item(three), START));
    press(&mut harness, Modifiers::NONE, Key::Enter);
    assert_eq!(at(&harness), (RowRef::Item(two), DUR), "fin ⇒ début");
    press(&mut harness, Modifiers::NONE, Key::Enter);
    assert_eq!(at(&harness), (RowRef::Item(three), DUR));
    press(&mut harness, Modifiers::NONE, Key::Enter);
    assert_eq!(at(&harness), (RowRef::Item(two), START));
    press(&mut harness, Modifiers::NONE, Key::Tab);
    assert_eq!(at(&harness), (RowRef::Item(three), DUR));
    press(&mut harness, Modifiers::SHIFT, Key::Tab);
    assert_eq!(at(&harness), (RowRef::Item(two), START));
    press(&mut harness, Modifiers::SHIFT, Key::Enter);
    assert_eq!(at(&harness), (RowRef::Item(three), DUR));
    assert!(harness.state().view().edit.editing.is_none());
    // Trois colonnes : la cellule active passe au milieu, le bloc reste.
    press(&mut harness, Modifiers::SHIFT, Key::Backspace);
    press(&mut harness, Modifiers::SHIFT, Key::ArrowRight);
    press(&mut harness, Modifiers::SHIFT, Key::ArrowRight);
    let b = block(harness.state());
    assert_eq!(b.columns, [DUR, START, FINISH]);
    press(&mut harness, Modifiers::NONE, Key::Tab);
    press(&mut harness, Modifiers::NONE, Key::Tab);
    let b = block(harness.state());
    assert_eq!(b.active.1, START, "{b:?}");
    assert_eq!(b.columns, [DUR, START, FINISH]);
    // Une flèche : cellule seule.
    press(&mut harness, Modifiers::NONE, Key::ArrowDown);
    assert!(block(harness.state()).is_single());
}

/// H-UI-93 en fin de saisie : bloc de deux lignes sur le Nom, frappe puis
/// Entrée ⇒ valeur écrite, cellule active à la ligne suivante du bloc,
/// sélection gardée.
#[test]
fn h_ui_93_enter_after_typing_stays_in_block() {
    let mut harness = demo_at(2, Column::Name);
    press(&mut harness, Modifiers::SHIFT, Key::ArrowDown);
    press(&mut harness, Modifiers::SHIFT, Key::ArrowUp);
    press(&mut harness, Modifiers::SHIFT, Key::ArrowDown);
    // Active (3, Nom) ; ancre 2 : remonter dans le bloc.
    press(&mut harness, Modifiers::SHIFT, Key::Enter);
    let app = harness.state();
    let (two, three) = (uid_at(app, 2), uid_at(app, 3));
    assert_eq!(block(app).active, (RowRef::Item(two), Column::Name));
    harness.event(Event::Text("Tâche renommée".to_owned()));
    harness.run();
    assert!(harness.state().view().edit.editing.is_some());
    press(&mut harness, Modifiers::NONE, Key::Enter);
    let app = harness.state();
    assert!(app.view().edit.editing.is_none());
    assert_eq!(app.rows().unwrap().text(2, Column::Name), "Tâche renommée");
    let b = block(app);
    assert_eq!(b.rows, display(2, 3));
    assert_eq!(b.active, (RowRef::Item(three), Column::Name));
}

/// H-UI-101 (U21) : un clic dans le Gantt (vide, ligne 5) sélectionne la
/// ligne et laisse le clavier à la table : ↓ passe à la ligne 6, la frappe
/// ouvre une saisie (visible, dans la table).
#[test]
fn h_ui_101_gantt_click_keyboard_to_table() {
    let mut harness = demo_at(1, Column::Name);
    let left = harness.state().view().last_gantt_left;
    let y = cell_point(&harness, 5, Column::Name).y;
    // Vide du Gantt à droite (la ligne 5 finit tôt dans le projet).
    let right = left + harness.state().view().last_gantt_width - 8.0;
    click_at(&mut harness, egui::pos2(right, y), Modifiers::NONE);
    let app = harness.state();
    assert_eq!(app.selection, Some(uid_at(app, 5)));
    assert_eq!(app.view().focus, Pane::Table);
    assert!(table_focused(&harness));
    press(&mut harness, Modifiers::NONE, Key::ArrowDown);
    assert_eq!(harness.state().selection, Some(uid_at(harness.state(), 6)));
    harness.event(Event::Text("x".to_owned()));
    harness.run();
    let app = harness.state();
    let editing = app.view().edit.editing.as_ref().expect("saisie ouverte");
    assert_eq!(editing.uid, Some(uid_at(app, 6)));
    assert_eq!(editing.buffer, "x");
}

/// H-UI-101 : un clic sur un titre de colonne laisse le clavier à la
/// table (↓ part de la cellule active du bloc et le réduit).
#[test]
fn h_ui_101_header_click_keeps_keys() {
    let mut harness = demo_at(4, Column::Name);
    let p = header_point(&harness, DUR);
    click_at(&mut harness, p, Modifiers::NONE);
    assert!(table_focused(&harness));
    assert_eq!(harness.state().view().focus, Pane::Table);
    press(&mut harness, Modifiers::NONE, Key::ArrowDown);
    let app = harness.state();
    let b = block(app);
    assert!(b.is_single());
    assert_eq!(b.active, (RowRef::Item(uid_at(app, 1)), DUR));
}
