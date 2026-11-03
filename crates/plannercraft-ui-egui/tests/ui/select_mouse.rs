//! Sélection à la souris (passe UX, ADR-0018, lot L1) : appui, glisser,
//! Maj et Ctrl/Cmd+appui, ID, en-têtes de colonne, défilement automatique,
//! Gantt (H-UI-88 à H-UI-91, H-UI-101, H-UI-102). Le pointeur est conduit
//! par de vrais événements egui (appui, déplacements, relâcher dans des
//! images distinctes) ; le bloc est relu par `block::resolve`, la seule
//! résolution de la sélection.

#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_precision_loss,
    reason = "tests : positions du pointeur et indices bornés par l'exemple"
)]

use egui::{Event, Key, Modifiers, PointerButton, Pos2, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_core::TaskUid;
use plannercraft_ui_egui::block::{Block, resolve};
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::nav::RowRef;
use plannercraft_ui_egui::row_view::DisplayRow;
use plannercraft_ui_egui::rows::RowKind;
use plannercraft_ui_egui::selection::SelectionKind;
use plannercraft_ui_egui::view::table::table_id;
use plannercraft_ui_egui::{App, DocOrigin};

use crate::{harness, harness_for, new_app, open_demo, screen_axis};

/// Libellé d'accessibilité de la ligne affichée `i`.
pub fn row_label(app: &App, i: usize) -> String {
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
pub fn cell_point(harness: &Harness<'static, App>, i: usize, column: Column) -> Pos2 {
    let app = harness.state();
    let rect = harness.get_by_label(&row_label(app, i)).rect();
    let mut x = rect.left() - app.view().table_scroll_x;
    for state in app.state.visible_columns() {
        let w = f32::from(state.width);
        if state.column == column {
            return pos2(x + w / 2.0, rect.center().y);
        }
        x += w;
    }
    panic!("colonne {column:?} masquée");
}

/// Point au centre du titre de la colonne `column`.
pub fn header_point(harness: &Harness<'static, App>, column: Column) -> Pos2 {
    let texts = Texts::new(Language::Fr);
    let label = texts.get_args(
        TextId::ColumnHeaderLabel,
        &[("column", Arg::Str(texts.get(column.title()).into()))],
    );
    harness.get_by_label(&label).rect().center()
}

/// Appui du bouton principal en `p` (pointeur amené d'abord), avec
/// `modifiers` tenus pendant l'appui ; le bouton reste enfoncé.
pub fn press_at(harness: &mut Harness<'static, App>, p: Pos2, modifiers: Modifiers) {
    harness.hover_at(p);
    harness.step();
    let event = Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed: true,
        modifiers,
    };
    if modifiers.is_none() {
        harness.event(event);
    } else {
        harness.event_modifiers(event, modifiers);
    }
    harness.step();
}

/// Déplace le pointeur en `p` (bouton tenu ou non), une image.
pub fn move_to(harness: &mut Harness<'static, App>, p: Pos2) {
    harness.hover_at(p);
    harness.step();
}

/// Relâche le bouton principal en `p`, puis stabilise.
pub fn release_at(harness: &mut Harness<'static, App>, p: Pos2) {
    harness.drop_at(p);
    harness.run();
}

/// Clic (appui et relâcher dans deux images) en `p` avec `modifiers`.
pub fn click_at(harness: &mut Harness<'static, App>, p: Pos2, modifiers: Modifiers) {
    press_at(harness, p, modifiers);
    release_at(harness, p);
}

/// Clic sur la cellule (ligne `i` visible, `column`) avec `modifiers`.
pub fn click_cell(harness: &mut Harness<'static, App>, i: usize, column: Column, modifiers: Modifiers) {
    let p = cell_point(harness, i, column);
    click_at(harness, p, modifiers);
}

/// Appui en `from`, glisser en quatre pas jusqu'à `to`, relâcher.
fn drag(harness: &mut Harness<'static, App>, from: Pos2, to: Pos2) {
    press_at(harness, from, Modifiers::NONE);
    for k in 1..=4_u8 {
        let t = f32::from(k) / 4.0;
        move_to(harness, from + (to - from) * t);
    }
    release_at(harness, to);
}

/// Bloc résolu de la sélection courante.
pub fn block(app: &App) -> Block {
    let visible: Vec<Column> = app.state.visible_columns().map(|c| c.column).collect();
    resolve(
        app.selected(),
        &app.view().cursor,
        app.visible_rows().unwrap(),
        &visible,
    )
}

/// UID de la ligne affichée `i`.
pub fn uid_at(app: &App, i: usize) -> TaskUid {
    app.visible_rows().unwrap().uid(DisplayRow::at(i)).unwrap()
}

/// Lignes affichées `range`.
fn display(range: std::ops::RangeInclusive<u32>) -> Vec<DisplayRow> {
    range.map(DisplayRow).collect()
}

/// La table a le focus clavier d'egui.
pub fn table_focused(harness: &Harness<'static, App>) -> bool {
    harness.ctx.memory(|memory| memory.has_focus(table_id()))
}

/// Exemple ouvert, stabilisé.
pub fn demo() -> Harness<'static, App> {
    let mut harness = harness();
    open_demo(&mut harness);
    harness
}

/// Lignes de la table : 2 à 5 de l'exemple sur Durée et Début.
const DUR: Column = Column::Duration;
const START: Column = Column::Start;

/// H-UI-88 (U01, bug signalé) : appui sur (2, Durée), glisser jusqu'à
/// (5, Début), relâcher ⇒ bloc de 8 cellules, cellule active (5, Début),
/// ancre (2, Durée) ; la table a le clavier.
#[test]
fn h_ui_88_drag_selects_block() {
    let mut harness = demo();
    let from = cell_point(&harness, 2, DUR);
    let to = cell_point(&harness, 5, START);
    drag(&mut harness, from, to);
    let app = harness.state();
    let b = block(app);
    assert_eq!(b.kind, SelectionKind::Cells);
    assert_eq!(b.rows, display(2..=5));
    assert_eq!(b.columns, [DUR, START]);
    assert_eq!(b.cells(), 8);
    assert_eq!(b.active, (RowRef::Item(uid_at(app, 5)), START));
    assert_eq!(app.selected().anchor(), Some(uid_at(app, 2)));
    assert_eq!(app.view().cursor.anchor_column, Some(DUR));
    assert!(app.view().select_drag.is_none(), "glisser fermé au relâcher");
    assert!(table_focused(&harness));
    // Glisser dans l'autre sens : le bloc suit l'ancre.
    let from = cell_point(&harness, 6, START);
    let to = cell_point(&harness, 4, DUR);
    drag(&mut harness, from, to);
    let b = block(harness.state());
    assert_eq!(b.rows, display(4..=6));
    assert_eq!(b.columns, [DUR, START]);
    assert_eq!(b.active.1, DUR);
}

/// H-UI-88 : l'appui sélectionne, avant tout relâcher.
#[test]
fn h_ui_88_press_selects_before_release() {
    let mut harness = demo();
    let p = cell_point(&harness, 4, Column::Name);
    press_at(&mut harness, p, Modifiers::NONE);
    let app = harness.state();
    assert!(harness.ctx.input(|i| i.pointer.primary_down()));
    assert_eq!(app.selection, Some(uid_at(app, 4)));
    assert_eq!(app.view().cursor.column, Column::Name);
    assert!(app.view().select_drag.is_some());
    release_at(&mut harness, p);
    assert_eq!(harness.state().selection, Some(uid_at(harness.state(), 4)));
}

/// H-UI-88 (U05) : un clic qui bouge de 8 points (au-delà du seuil de
/// glisser d'egui) dans la même cellule la sélectionne quand même.
#[test]
fn h_ui_88_jitter_click_selects() {
    let mut harness = demo();
    let p = cell_point(&harness, 6, Column::Name);
    press_at(&mut harness, p, Modifiers::NONE);
    move_to(&mut harness, p + egui::vec2(8.0, 2.0));
    release_at(&mut harness, p + egui::vec2(8.0, 2.0));
    let app = harness.state();
    let b = block(app);
    assert!(b.is_single(), "{b:?}");
    assert_eq!(b.active, (RowRef::Item(uid_at(app, 6)), Column::Name));
}

/// Bords gauche et droit écran de la colonne `column` (ligne `i` visible).
fn cell_x_span(harness: &Harness<'static, App>, i: usize, column: Column) -> (f32, f32) {
    let app = harness.state();
    let center = cell_point(harness, i, column).x;
    let w = app
        .state
        .visible_columns()
        .find(|c| c.column == column)
        .map(|c| f32::from(c.width))
        .unwrap();
    (center - w / 2.0, center + w / 2.0)
}

/// H-UI-88 (relecture finale, n° 1) : un tremblement de 3 points sous le
/// seuil de glisser qui franchit une bordure reste la cellule appuyée :
/// cellule (bordure droite du Nom), ID (bordure basse) et titre de colonne
/// (bordure gauche de la Durée, hors de la poignée de largeur).
#[test]
fn h_ui_88_jitter_across_border_stays() {
    let mut harness = demo();
    let (_, right) = cell_x_span(&harness, 6, Column::Name);
    let p = pos2(right - 2.0, cell_point(&harness, 6, Column::Name).y);
    press_at(&mut harness, p, Modifiers::NONE);
    move_to(&mut harness, p + egui::vec2(3.0, 0.0));
    release_at(&mut harness, p + egui::vec2(3.0, 0.0));
    let app = harness.state();
    let b = block(app);
    assert!(b.is_single(), "{b:?}");
    assert_eq!(b.active, (RowRef::Item(uid_at(app, 6)), Column::Name));
    // ID : bordure basse de la ligne 6.
    let row_h = harness.state().row_h();
    let id = cell_point(&harness, 6, Column::Id);
    let p = pos2(id.x, id.y + row_h / 2.0 - 2.0);
    press_at(&mut harness, p, Modifiers::NONE);
    move_to(&mut harness, p + egui::vec2(0.0, 3.0));
    release_at(&mut harness, p + egui::vec2(0.0, 3.0));
    let app = harness.state();
    assert_eq!(app.selected().kind(), SelectionKind::Rows);
    assert_eq!(app.selected().click_order(), [uid_at(app, 6)]);
    // Titre : 5 points dans la Durée, puis 0,5 point dans le Nom.
    let (left, _) = cell_x_span(&harness, 6, DUR);
    let y = header_point(&harness, DUR).y;
    press_at(&mut harness, pos2(left + 5.0, y), Modifiers::NONE);
    move_to(&mut harness, pos2(left - 0.5, y));
    release_at(&mut harness, pos2(left - 0.5, y));
    assert_eq!(block(harness.state()).columns, [DUR]);
    // Au-delà du seuil, le premier changement de cellule compte.
    let p = pos2(right - 2.0, cell_point(&harness, 6, Column::Name).y);
    press_at(&mut harness, p, Modifiers::NONE);
    move_to(&mut harness, p + egui::vec2(7.0, 0.0));
    release_at(&mut harness, p + egui::vec2(7.0, 0.0));
    assert_eq!(block(harness.state()).columns, [Column::Name, DUR]);
}

/// H-UI-88 : Maj+appui étend depuis l'ancre (bloc 2-5 × Durée-Début).
#[test]
fn h_ui_88_shift_press_extends() {
    let mut harness = demo();
    let a = cell_point(&harness, 2, DUR);
    click_at(&mut harness, a, Modifiers::NONE);
    let b = cell_point(&harness, 5, START);
    click_at(&mut harness, b, Modifiers::SHIFT);
    let app = harness.state();
    let blk = block(app);
    assert_eq!(blk.rows, display(2..=5));
    assert_eq!(blk.columns, [DUR, START]);
    assert_eq!(blk.active, (RowRef::Item(uid_at(app, 5)), START));
    assert_eq!(app.selected().anchor(), Some(uid_at(app, 2)));
}

/// H-UI-87, H-UI-88 : Ctrl/Cmd+appui ajoute ou retire une ligne (bloc
/// réduit à la colonne cliquée), jamais la dernière (U08).
#[test]
fn h_ui_88_ctrl_press_toggles_row() {
    let mut harness = demo();
    click_cell(&mut harness, 3, DUR, Modifiers::NONE);
    click_cell(&mut harness, 6, START, Modifiers::COMMAND);
    let app = harness.state();
    let (three, six) = (uid_at(app, 3), uid_at(app, 6));
    assert_eq!(app.selected().click_order(), [three, six]);
    let b = block(app);
    assert_eq!(b.rows, [DisplayRow(3), DisplayRow(6)]);
    assert_eq!(b.columns, [START]);
    // Retrait de 6, puis 3 seule : sans effet.
    click_cell(&mut harness, 6, START, Modifiers::COMMAND);
    assert_eq!(harness.state().selected().click_order(), [three]);
    click_cell(&mut harness, 3, START, Modifiers::COMMAND);
    let app = harness.state();
    assert_eq!(app.selected().click_order(), [three]);
    assert_eq!(app.selection, Some(three));
}

/// Relecture finale n° 6 : {2, 6} par Ctrl/Cmd+clic, puis Ctrl/Cmd+appui
/// sur 9 et glisser jusqu'à 11 ⇒ 9 ajoutée seule, 2 et 6 gardées (pas de
/// plage qui remplace la multi-sélection).
#[test]
fn h_ui_88_ctrl_drag_keeps_multi_selection() {
    let mut harness = demo();
    click_cell(&mut harness, 2, Column::Name, Modifiers::NONE);
    click_cell(&mut harness, 6, Column::Name, Modifiers::COMMAND);
    let from = cell_point(&harness, 9, Column::Name);
    let to = cell_point(&harness, 11, Column::Name);
    press_at(&mut harness, from, Modifiers::COMMAND);
    for k in 1..=4_u8 {
        move_to(&mut harness, from + (to - from) * (f32::from(k) / 4.0));
    }
    release_at(&mut harness, to);
    let app = harness.state();
    assert_eq!(
        app.selected().click_order(),
        [uid_at(app, 2), uid_at(app, 6), uid_at(app, 9)]
    );
}

/// H-UI-86 gardée : saisie ouverte, un appui ailleurs valide d'abord, puis
/// sélectionne la cellule appuyée (une seule cellule, aucun glisser) ;
/// refusée, la saisie reste ouverte et la sélection ne bouge pas.
#[test]
fn h_ui_88_editing_click_commits_first() {
    let mut harness = demo();
    click_cell(&mut harness, 3, Column::Name, Modifiers::NONE);
    let generation = harness.state().document.as_ref().unwrap().generation();
    harness.key_press(Key::F2);
    harness.run();
    assert!(harness.state().view().edit.editing.is_some());
    harness.key_press(Key::End);
    harness.event(Event::Text("bis".to_owned()));
    harness.run();
    let target = cell_point(&harness, 6, Column::Name);
    let to = cell_point(&harness, 8, START);
    press_at(&mut harness, target, Modifiers::NONE);
    move_to(&mut harness, to);
    release_at(&mut harness, to);
    let app = harness.state();
    assert!(app.view().edit.editing.is_none());
    assert_eq!(app.document.as_ref().unwrap().generation(), generation + 1);
    assert!(app.rows().unwrap().text(3, Column::Name).ends_with("bis"));
    assert_eq!(app.selection, Some(uid_at(app, 6)));
    assert!(block(app).is_single(), "aucun glisser pendant une saisie");
    // Refus : la saisie reste, la sélection aussi.
    let leaf = (0..20)
        .find(|&i| app.rows().unwrap().row(i).unwrap().kind == RowKind::Leaf)
        .unwrap();
    click_cell(&mut harness, leaf, DUR, Modifiers::NONE);
    harness.event(Event::Text("abc".to_owned()));
    harness.run();
    assert!(harness.state().view().edit.editing.is_some());
    let other = if leaf == 1 { 2 } else { 1 };
    click_cell(&mut harness, other, DUR, Modifiers::NONE);
    let app = harness.state();
    assert!(app.view().edit.editing.is_some(), "saisie refusée gardée");
    assert_eq!(app.selection, Some(uid_at(app, leaf)));
}

/// H-UI-89 (U03) : appui sur l'ID 2, glisser jusqu'à l'ID 6 ⇒ lignes 2 à
/// 6 entières (toutes les colonnes visibles).
#[test]
fn h_ui_89_id_drag_selects_rows() {
    let mut harness = demo();
    let from = cell_point(&harness, 2, Column::Id);
    let to = cell_point(&harness, 6, Column::Id) + egui::vec2(60.0, 0.0);
    drag(&mut harness, from, to);
    let app = harness.state();
    let b = block(app);
    assert_eq!(b.kind, SelectionKind::Rows);
    assert_eq!(b.rows, display(2..=6));
    let visible: Vec<Column> = app.state.visible_columns().map(|c| c.column).collect();
    assert_eq!(b.columns, visible);
    assert_eq!(app.selected().len(), 5);
    // Appui sur l'ID d'une ligne déjà sélectionnée en lignes entières,
    // relâché sans glisser : elle seule (D6 : l'appui est différé).
    let p = cell_point(&harness, 4, Column::Id);
    press_at(&mut harness, p, Modifiers::NONE);
    assert_eq!(harness.state().selected().len(), 5, "appui différé");
    release_at(&mut harness, p);
    let app = harness.state();
    assert_eq!(app.selected().click_order(), [uid_at(app, 4)]);
    assert_eq!(app.selected().kind(), SelectionKind::Rows);
}

/// H-UI-90 (U07) : clic sur le titre « Durée » ⇒ colonne entière (lignes
/// affichées), cellule active en première ligne ; le clavier reste à la
/// table.
#[test]
fn h_ui_90_header_click_selects_column() {
    let mut harness = demo();
    let p = header_point(&harness, DUR);
    click_at(&mut harness, p, Modifiers::NONE);
    let app = harness.state();
    let b = block(app);
    assert_eq!(b.kind, SelectionKind::Cells);
    assert_eq!(b.columns, [DUR]);
    assert_eq!(b.rows.len(), app.rows().unwrap().len());
    assert_eq!(b.active, (RowRef::Item(uid_at(app, 0)), DUR));
    assert!(table_focused(&harness));
}

/// H-UI-90 : glisser sur les titres de Durée à Fin ⇒ trois colonnes ;
/// Maj+clic sur Nom ⇒ de l'ancre (Durée) à Nom.
#[test]
fn h_ui_90_header_drag_columns() {
    let mut harness = demo();
    let from = header_point(&harness, DUR);
    let to = header_point(&harness, Column::Finish);
    drag(&mut harness, from, to);
    let b = block(harness.state());
    assert_eq!(b.columns, [DUR, START, Column::Finish]);
    assert_eq!(b.rows.len(), harness.state().rows().unwrap().len());
    let name = header_point(&harness, Column::Name);
    click_at(&mut harness, name, Modifiers::SHIFT);
    let b = block(harness.state());
    assert_eq!(b.columns, [Column::Name, DUR]);
}

/// H-UI-90, H-UI-92 : clic sur le titre de l'ID (le coin) ⇒ tout, en
/// lignes entières.
#[test]
fn h_ui_90_corner_selects_all() {
    let mut harness = demo();
    let p = header_point(&harness, Column::Id);
    click_at(&mut harness, p, Modifiers::NONE);
    let app = harness.state();
    assert_eq!(app.selected().kind(), SelectionKind::Rows);
    assert_eq!(app.selected().len(), app.rows().unwrap().len());
    assert!(table_focused(&harness));
}

/// Réseau généré de `n × 20` tâches ouvert dans le harnais.
fn generated(summaries: usize) -> Harness<'static, App> {
    let mut app = new_app();
    app.open_project(
        plannercraft_tests::generate::ui_network(summaries, 19, 40),
        DocOrigin {
            name: "réseau".to_owned(),
            path: None,
            demo: false,
        },
    );
    let mut harness = harness_for(app);
    harness.run();
    harness
}

/// Haut et bas écran du corps (lignes) d'après la première ligne dessinée
/// et la hauteur relevée.
fn body_span(harness: &Harness<'static, App>) -> (f32, f32) {
    let app = harness.state();
    let row_h = app.row_h();
    let scroll = app.view().last_scroll_y;
    // Deuxième ligne visible : entièrement dans le corps.
    let first = plannercraft_ui_egui::geom::f32_to_index((scroll / row_h).floor()) + 1;
    let top = harness.get_by_label(&row_label(app, first)).rect().top();
    let top = top - (row_h * first as f32 - scroll);
    (top, top + app.view().last_body_height)
}

/// H-UI-91 (U04) : 200 tâches ; appui sur (3, Nom), pointeur tenu au bord
/// bas du corps 30 images ⇒ la vue défile et le bloc s'étend aux lignes
/// devenues visibles ; aucune valeur non finie.
#[test]
fn h_ui_91_autoscroll_down() {
    let mut harness = generated(10);
    assert_eq!(harness.state().rows().unwrap().len(), 200);
    let p = cell_point(&harness, 3, Column::Name);
    let (_, bottom) = body_span(&harness);
    press_at(&mut harness, p, Modifiers::NONE);
    let last_visible = harness.state().last_frame.table_rows.end;
    let edge = pos2(p.x, bottom - 2.0);
    for _ in 0..30 {
        move_to(&mut harness, edge);
    }
    let app = harness.state();
    let scroll = app.view().last_scroll_y;
    assert!(scroll > 0.0 && scroll.is_finite(), "défilement {scroll}");
    let b = block(app);
    let end = b.rows.last().unwrap().index();
    assert!(
        end >= last_visible,
        "bloc jusqu'à {end}, visible jusqu'à {last_visible}"
    );
    assert_eq!(b.rows.first(), Some(&DisplayRow(3)));
    assert_eq!(b.columns, [Column::Name]);
    // Au-dessus du corps : la vue remonte.
    let (top, _) = body_span(&harness);
    for _ in 0..10 {
        move_to(&mut harness, pos2(p.x, top - 40.0));
    }
    assert!(harness.state().view().last_scroll_y < scroll);
    release_at(&mut harness, pos2(p.x, top - 40.0));
    assert!(harness.state().view().select_drag.is_none());
    // Relâché : plus de défilement.
    let still = harness.state().view().last_scroll_y;
    harness.run();
    assert_eq!(harness.state().view().last_scroll_y, still);
}

/// Bord droit écran de la table (corps), d'après la colonne ID et la
/// largeur relevée.
fn table_right(harness: &Harness<'static, App>) -> f32 {
    let app = harness.state();
    let (left, _) = cell_x_span(harness, 0, Column::Id);
    left + app.view().table_scroll_x + app.view().last_table_width
}

/// H-UI-91 (relecture finale, n° 2) : glisser vertical à 40 points du bord
/// droit de la table, avec 1 point de dérive vers la droite, 30 images ⇒
/// la table ne défile pas de côté ; au-delà du bord droit, elle défile.
#[test]
fn h_ui_91_no_horizontal_autoscroll_on_drift() {
    let mut harness = demo();
    // Table plus étroite que ses colonnes.
    harness
        .state_mut()
        .dispatch(plannercraft_ui_egui::Action::SetTableWidth(400));
    harness.run();
    let app = harness.state();
    let columns_w: f32 = app.state.visible_columns().map(|c| f32::from(c.width)).sum();
    assert!(
        columns_w > app.view().last_table_width + 100.0,
        "colonnes {columns_w}, table {}",
        app.view().last_table_width
    );
    let right = table_right(&harness);
    let y = cell_point(&harness, 2, Column::Name).y;
    let row_h = harness.state().row_h();
    let from = pos2(right - 40.0, y);
    press_at(&mut harness, from, Modifiers::NONE);
    for k in 1..=30_u8 {
        let dy = (f32::from(k) * row_h / 6.0).min(row_h * 3.0);
        move_to(&mut harness, from + egui::vec2(1.0, dy));
    }
    assert_eq!(harness.state().view().table_scroll_x, 0.0);
    release_at(&mut harness, from + egui::vec2(1.0, row_h * 3.0));
    assert_eq!(harness.state().view().table_scroll_x, 0.0);
    // Au-delà du bord droit : défilement.
    press_at(&mut harness, from, Modifiers::NONE);
    for _ in 0..10 {
        move_to(&mut harness, pos2(right + 10.0, y));
    }
    let x = harness.state().view().table_scroll_x;
    assert!(x > 0.0 && x.is_finite(), "défilement {x}");
    release_at(&mut harness, pos2(right + 10.0, y));
}

/// H-UI-91 : glisser une barre (ou tracer un lien) près du bord bas fait
/// défiler la vue ; le glisser continue, Échap l'annule sans rien écrire.
#[test]
fn h_ui_91_autoscroll_bar_link_drag() {
    let mut harness = generated(10);
    let app = harness.state();
    let i = (2..12)
        .find(|&i| {
            let row = app.rows().unwrap().row(i).unwrap();
            row.kind == RowKind::Leaf && row.bar.is_some()
        })
        .unwrap();
    let (start, finish) = app.rows().unwrap().row(i).unwrap().bar.unwrap();
    let axis = screen_axis(app);
    let gantt_left = app.view().last_gantt_left;
    let x = f32::midpoint(axis.x(start), axis.x(finish)).max(gantt_left + 4.0);
    let y = cell_point(&harness, i, Column::Name).y;
    let generation = app.document.as_ref().unwrap().generation();
    let (_, bottom) = body_span(&harness);
    press_at(&mut harness, pos2(x, y), Modifiers::NONE);
    move_to(&mut harness, pos2(x + 2.0, y + 10.0));
    move_to(&mut harness, pos2(x + 4.0, y + 20.0));
    assert!(harness.state().view().drag.is_some(), "glisser de barre commencé");
    for _ in 0..30 {
        move_to(&mut harness, pos2(x + 4.0, bottom - 2.0));
    }
    let app = harness.state();
    assert!(app.view().drag.is_some(), "le glisser continue");
    assert!(app.view().last_scroll_y > 0.0, "{}", app.view().last_scroll_y);
    harness.key_press(Key::Escape);
    harness.step();
    release_at(&mut harness, pos2(x + 4.0, bottom - 2.0));
    let app = harness.state();
    assert!(app.view().drag.is_none());
    assert_eq!(app.document.as_ref().unwrap().generation(), generation);
}

/// Feuilles de l'exemple avec une barre, dans les 20 premières lignes.
fn leaves_with_bars(app: &App) -> Vec<usize> {
    (0..20)
        .filter(|&i| {
            let row = app.rows().unwrap().row(i).unwrap();
            row.kind == RowKind::Leaf && row.bar.is_some()
        })
        .collect()
}

/// Point au milieu de la barre de la ligne `i`, amenée d'abord à 150
/// points du bord gauche du Gantt.
fn bar_point(harness: &mut Harness<'static, App>, i: usize) -> Pos2 {
    let app = harness.state();
    let (start, _) = app.rows().unwrap().row(i).unwrap().bar.unwrap();
    let dx = screen_axis(app).x(start) - app.view().last_gantt_left - 150.0;
    harness
        .state_mut()
        .dispatch(plannercraft_ui_egui::Action::ScrollGanttX(dx));
    harness.run();
    let app = harness.state();
    let (start, finish) = app.rows().unwrap().row(i).unwrap().bar.unwrap();
    let axis = screen_axis(app);
    let x = f32::midpoint(axis.x(start), axis.x(finish));
    pos2(x, cell_point(harness, i, Column::Name).y)
}

/// H-UI-102 : sélection {a, b} ; glisser la barre de b la déplace et garde
/// {a, b} ; glisser la barre d'une tâche non sélectionnée la sélectionne
/// seule.
#[test]
fn h_ui_102_bar_drag_keeps_multi_selection() {
    let mut harness = demo();
    crate::ribbon_zoom(&mut harness, plannercraft_ui_egui::TimeScaleZoom::Day);
    let leaves = leaves_with_bars(harness.state());
    let (a, b, c) = (leaves[0], leaves[1], leaves[2]);
    click_cell(&mut harness, a, Column::Name, Modifiers::NONE);
    click_cell(&mut harness, b, Column::Name, Modifiers::COMMAND);
    let app = harness.state();
    let (ua, ub, uc) = (uid_at(app, a), uid_at(app, b), uid_at(app, c));
    let generation = app.document.as_ref().unwrap().generation();
    let from = bar_point(&mut harness, b);
    drag(&mut harness, from, from + egui::vec2(60.0, 0.0));
    let app = harness.state();
    assert_eq!(
        app.document.as_ref().unwrap().generation(),
        generation + 1,
        "{:?}",
        app.messages()
    );
    let mut kept: Vec<TaskUid> = app.selected().uids().iter().copied().collect();
    kept.sort_unstable();
    let mut want = vec![ua, ub];
    want.sort_unstable();
    assert_eq!(kept, want, "sélection multiple gardée");
    assert!(table_focused(&harness), "le clavier reste à la table");
    let from = bar_point(&mut harness, c);
    drag(&mut harness, from, from + egui::vec2(60.0, 0.0));
    let app = harness.state();
    assert_eq!(app.selected().click_order(), [uc]);
}

/// Relecture finale n° 4 : bloc 2-5 × Durée-Début, cellule active sur la
/// ligne 5 ; un clic simple sur la ligne 5 du Gantt réduit la sélection à
/// (5, Début), comme dans la table ; Maj+clic sur la ligne 3 étend de
/// l'ancre (5) à 3.
#[test]
fn gantt_click_collapses_block() {
    let mut harness = demo();
    let from = cell_point(&harness, 2, DUR);
    let to = cell_point(&harness, 5, START);
    drag(&mut harness, from, to);
    assert_eq!(block(harness.state()).cells(), 8);
    let x = harness.state().view().last_gantt_left + 3.0;
    let y = cell_point(&harness, 5, Column::Name).y;
    click_at(&mut harness, pos2(x, y), Modifiers::NONE);
    let app = harness.state();
    let b = block(app);
    assert!(b.is_single(), "{b:?}");
    assert_eq!(b.active, (RowRef::Item(uid_at(app, 5)), START));
    assert!(table_focused(&harness));
    // Maj tenue jusqu'au relâcher (le Gantt sélectionne au clic).
    let p = pos2(x, cell_point(&harness, 3, Column::Name).y);
    press_at(&mut harness, p, Modifiers::SHIFT);
    let release = Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::SHIFT,
    };
    harness.event_modifiers(release, Modifiers::SHIFT);
    harness.run();
    let b = block(harness.state());
    assert_eq!(b.rows, display(3..=5));
    assert_eq!(b.columns, [START]);
}

/// U24 (non-régression) : Maj+clic, Ctrl+clic, Maj+↓, clic droit dans une
/// multi-sélection (gardée), Inser.
#[test]
fn u24_non_regression() {
    let mut harness = demo();
    click_cell(&mut harness, 2, Column::Name, Modifiers::NONE);
    click_cell(&mut harness, 4, Column::Name, Modifiers::SHIFT);
    assert_eq!(harness.state().selected().len(), 3);
    click_cell(&mut harness, 7, Column::Name, Modifiers::COMMAND);
    assert_eq!(harness.state().selected().len(), 4);
    // Clic droit dans la multi-sélection : gardée, menu ouvert.
    let p = cell_point(&harness, 3, Column::Name);
    harness.hover_at(p);
    harness.step();
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: p,
            button: PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run();
    assert_eq!(harness.state().selected().len(), 4);
    assert!(harness.ctx.any_popup_open(), "menu contextuel ouvert");
    harness.key_press(Key::Escape);
    harness.run();
    // Maj+↓ : plage de lignes depuis l'ancre.
    click_cell(&mut harness, 5, Column::Name, Modifiers::NONE);
    harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowDown);
    harness.run();
    let app = harness.state();
    assert_eq!(block(app).rows, display(5..=6));
    // Inser : une tâche insérée.
    let before = app.rows().unwrap().len();
    harness.key_press(Key::Insert);
    harness.run();
    assert_eq!(harness.state().rows().unwrap().len(), before + 1);
}
