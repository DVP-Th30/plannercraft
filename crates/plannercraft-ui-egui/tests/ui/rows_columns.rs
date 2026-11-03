//! Lignes et colonnes (passe UX, ADR-0018, lot L3) : déplacer des lignes
//! par leur ID, ajuster une colonne, menu d'en-tête, curseurs (H-UI-94,
//! H-UI-99, H-UI-103). Le pointeur est conduit par de vrais événements egui
//! (appui, déplacements, relâcher dans des images distinctes), comme
//! `select_mouse.rs`. Repère : projet « P » (`plannercraft_tests::daily::plan_p`,
//! UID n = ID n, ligne affichée n − 1) : 1 « Gros œuvre » ⊃ 2, 3, 4 ;
//! 5 « Second œuvre » ⊃ 6, 7.

#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_precision_loss,
    reason = "tests : positions du pointeur et indices bornés par l'exemple"
)]

use egui::{CursorIcon, Key, Modifiers, Pos2, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use plannercraft_core::{Project, TaskUid};
use plannercraft_tests::daily::plan_p;
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::row_view::DisplayRow;
use plannercraft_ui_egui::selection::SelectionKind;
use plannercraft_ui_egui::state::{SortKey, SortSpec};
use plannercraft_ui_egui::view::table::RESIZE_GRIP;
use plannercraft_ui_egui::view::task_view::TaskViewOp;
use plannercraft_ui_egui::{Action, App, DocOrigin};

use crate::select_mouse::{cell_point, move_to, press_at, release_at};
use crate::{fr, new_app};

/// UID `n`.
const fn uid(n: u32) -> TaskUid {
    TaskUid::new(n)
}

/// Harnais sur le projet « P », images de 1/60 s (deux clics d'une même
/// suite d'événements forment un double-clic, délai d'egui 0,3 s).
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
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1280.0, 720.0))
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .with_os(egui::os::OperatingSystem::Nix)
        .with_step_dt(1.0 / 60.0)
        .with_max_steps(8)
        .build_ui_state(plannercraft_ui_egui::ui, app);
    harness.run();
    harness
}

fn project(app: &App) -> &Project {
    app.document.as_ref().unwrap().project()
}

/// (UID, niveau) de chaque tâche, dans l'ordre du projet.
fn outline(app: &App) -> Vec<(u32, u16)> {
    project(app)
        .tasks
        .iter()
        .map(|t| (plannercraft_core::HasUid::uid(t).get(), t.outline_level.get()))
        .collect()
}

/// Entrées annulables.
fn undo_len(app: &App) -> usize {
    app.document.as_ref().unwrap().session.history().entries.len()
}

/// Dernier message du bandeau.
fn last_message(app: &App) -> Option<String> {
    app.messages().last().map(|m| m.text.clone())
}

/// Curseur de souris demandé par la dernière image.
fn cursor_icon(harness: &Harness<'static, App>) -> CursorIcon {
    harness.output().platform_output.cursor_icon
}

/// Point de l'ID de la tâche `n` (ligne affichée `n − 1`).
fn id_point(harness: &Harness<'static, App>, n: usize) -> Pos2 {
    cell_point(harness, n - 1, Column::Id)
}

/// Hauteur d'une ligne de la table.
fn row_h(harness: &Harness<'static, App>) -> f32 {
    let a = cell_point(harness, 0, Column::Id);
    let b = cell_point(harness, 1, Column::Id);
    b.y - a.y
}

/// Point à `dy` du haut de la ligne de la tâche `n`, colonne ID.
fn row_top(harness: &Harness<'static, App>, n: usize, dy: f32) -> Pos2 {
    let p = id_point(harness, n);
    pos2(p.x, p.y - row_h(harness) / 2.0 + dy)
}

/// Lignes entières `first` à `last` par l'ID : appui, puis Maj+appui.
fn select_rows(harness: &mut Harness<'static, App>, first: usize, last: usize) {
    let p = id_point(harness, first);
    press_at(harness, p, Modifiers::NONE);
    release_at(harness, p);
    if last != first {
        let q = id_point(harness, last);
        press_at(harness, q, Modifiers::SHIFT);
        release_at(harness, q);
    }
    let app = harness.state();
    assert_eq!(app.selected().kind(), SelectionKind::Rows);
    assert_eq!(app.selected().len(), last - first + 1);
}

/// Appui sur l'ID de la tâche `n`, puis glisser en quatre pas jusqu'à `to`
/// (bouton tenu).
fn drag_id_to(harness: &mut Harness<'static, App>, n: usize, to: Pos2) {
    let from = id_point(harness, n);
    press_at(harness, from, Modifiers::NONE);
    for k in 1..=4_u8 {
        let t = f32::from(k) / 4.0;
        move_to(harness, from + (to - from) * t);
    }
}

/// Insertion courante du déplacement de lignes (ouvert).
fn insert_before(app: &App) -> Option<DisplayRow> {
    app.view()
        .row_drag
        .as_ref()
        .expect("déplacement ouvert")
        .insert_before
}

/// H-UI-94 (U16) : lignes 6-7 entières, glisser l'ID 6 au-dessus de 2 ⇒ 6
/// et 7 sous 1, avant 2 ; UID gardés ; une entrée d'annulation ; Ctrl+Z ⇒
/// P ; la table garde le clavier et la sélection.
#[test]
fn h_ui_94_drag_rows_moves_one_undo() {
    let mut harness = p_harness();
    let before = outline(harness.state());
    let undo = undo_len(harness.state());
    select_rows(&mut harness, 6, 7);
    let target = row_top(&harness, 2, 2.0);
    drag_id_to(&mut harness, 6, target);
    let app = harness.state();
    let drag = app.view().row_drag.as_ref().expect("déplacement ouvert");
    assert_eq!(drag.uids, [uid(6), uid(7)]);
    assert_eq!(drag.insert_before, Some(DisplayRow(1)));
    assert_eq!(drag.refusal, None);
    assert!(app.view().select_drag.is_none(), "pas de glisser de sélection");
    assert_eq!(cursor_icon(&harness), CursorIcon::Move);
    release_at(&mut harness, target);
    let app = harness.state();
    assert!(app.view().row_drag.is_none());
    assert_eq!(
        outline(app),
        [(1, 1), (6, 2), (7, 2), (2, 2), (3, 2), (4, 2), (5, 1)]
    );
    assert_eq!(undo_len(app), undo + 1, "un geste = une entrée");
    let names: Vec<&str> = project(app).tasks.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names[1], "Plâtrerie");
    assert_eq!(names[2], "Peinture");
    // Sélection gardée sur les tâches déplacées, clavier à la table.
    assert!(app.selected().contains(uid(6)) && app.selected().contains(uid(7)));
    assert!(crate::select_mouse::table_focused(&harness));
    harness.state_mut().dispatch(Action::Undo);
    harness.run();
    assert_eq!(outline(harness.state()), before);
}

/// H-UI-94 : le trait d'insertion suit l'intervalle le plus proche du
/// pointeur (moitié haute d'une ligne : devant elle ; moitié basse : devant
/// la suivante ; sous la dernière ligne : en fin, `None`).
#[test]
fn h_ui_94_insert_indicator_position() {
    let mut harness = p_harness();
    select_rows(&mut harness, 6, 7);
    let h = row_h(&harness);
    let upper = row_top(&harness, 4, h * 0.25);
    drag_id_to(&mut harness, 6, upper);
    assert_eq!(insert_before(harness.state()), Some(DisplayRow(3)));
    let lower = row_top(&harness, 4, h * 0.75);
    move_to(&mut harness, lower);
    assert_eq!(insert_before(harness.state()), Some(DisplayRow(4)));
    let app = harness.state();
    assert_eq!(app.view().row_drag.as_ref().unwrap().refusal, None);
    // Sous la dernière tâche (ligne vide) : en fin.
    let below = row_top(&harness, 7, h * 1.4);
    move_to(&mut harness, below);
    assert_eq!(insert_before(harness.state()), None);
    // Lâcher en fin : 6 et 7 au premier niveau, à la fin.
    release_at(&mut harness, below);
    assert_eq!(
        outline(harness.state()),
        [(1, 1), (2, 2), (3, 2), (4, 2), (5, 1), (6, 1), (7, 1)]
    );
}

/// H-UI-94 : lâcher dans son propre bloc ⇒ curseur interdit, raison en
/// info-bulle, refus au bandeau, rien d'écrit.
#[test]
fn h_ui_94_drop_inside_own_block_refused() {
    let mut harness = p_harness();
    let before = outline(harness.state());
    let undo = undo_len(harness.state());
    select_rows(&mut harness, 6, 7);
    let inside = row_top(&harness, 7, 3.0);
    drag_id_to(&mut harness, 6, inside);
    let reason = fr(TextId::RowsMoveInsideBlock);
    let app = harness.state();
    assert_eq!(
        app.view().row_drag.as_ref().unwrap().refusal,
        Some(TextId::RowsMoveInsideBlock)
    );
    assert_eq!(cursor_icon(&harness), CursorIcon::NotAllowed);
    assert!(harness.query_by_label(&reason).is_some(), "raison affichée");
    release_at(&mut harness, inside);
    let app = harness.state();
    assert_eq!(outline(app), before);
    assert_eq!(undo_len(app), undo);
    assert_eq!(last_message(app), Some(reason));
}

/// H-UI-94 : lignes non sœurs voisines (2 et 6) ⇒ refus, rien d'écrit.
#[test]
fn h_ui_94_non_adjacent_refused() {
    let mut harness = p_harness();
    let before = outline(harness.state());
    select_rows(&mut harness, 2, 2);
    let p = id_point(&harness, 6);
    press_at(&mut harness, p, Modifiers::COMMAND);
    release_at(&mut harness, p);
    assert_eq!(harness.state().selected().len(), 2);
    let target = row_top(&harness, 1, 2.0);
    drag_id_to(&mut harness, 6, target);
    assert_eq!(
        harness.state().view().row_drag.as_ref().unwrap().refusal,
        Some(TextId::RowsMoveNotAdjacent)
    );
    assert_eq!(cursor_icon(&harness), CursorIcon::NotAllowed);
    release_at(&mut harness, target);
    let app = harness.state();
    assert_eq!(outline(app), before);
    assert_eq!(last_message(app), Some(fr(TextId::RowsMoveNotAdjacent)));
}

/// H-UI-94 et H-UI-32 : sous un tri, survol et glisser de l'ID ⇒ curseur
/// interdit, raison ; rien d'écrit.
#[test]
fn h_ui_94_locked_under_sort() {
    let mut harness = p_harness();
    let before = outline(harness.state());
    harness
        .state_mut()
        .dispatch(Action::TaskView(TaskViewOp::SetSort(Some(SortSpec {
            key: SortKey::Name,
            descending: true,
            keep_outline: true,
        }))));
    harness.run();
    assert!(harness.state().view().task_view.reorder_locked());
    // Lignes affichées 5 et 6 (ordre du tri), lignes entières.
    let p = cell_point(&harness, 5, Column::Id);
    press_at(&mut harness, p, Modifiers::NONE);
    release_at(&mut harness, p);
    let q = cell_point(&harness, 6, Column::Id);
    press_at(&mut harness, q, Modifiers::SHIFT);
    release_at(&mut harness, q);
    move_to(&mut harness, p);
    assert_eq!(cursor_icon(&harness), CursorIcon::NotAllowed);
    let to = cell_point(&harness, 1, Column::Id);
    press_at(&mut harness, p, Modifiers::NONE);
    for k in 1..=4_u8 {
        move_to(&mut harness, p + (to - p) * (f32::from(k) / 4.0));
    }
    assert_eq!(
        harness.state().view().row_drag.as_ref().unwrap().refusal,
        Some(TextId::RowsMoveLocked)
    );
    assert_eq!(cursor_icon(&harness), CursorIcon::NotAllowed);
    release_at(&mut harness, to);
    let app = harness.state();
    assert_eq!(outline(app), before);
    assert_eq!(last_message(app), Some(fr(TextId::RowsMoveLocked)));
    // Le réducteur refuse aussi (action arrivée par un autre chemin).
    harness
        .state_mut()
        .dispatch(Action::MoveRows { before: Some(uid(1)) });
    harness.run();
    assert_eq!(outline(harness.state()), before);
}

/// H-UI-94 : Échap pendant le glisser annule ; relâcher ensuite n'écrit
/// rien.
#[test]
fn h_ui_94_escape_cancels() {
    let mut harness = p_harness();
    let before = outline(harness.state());
    let undo = undo_len(harness.state());
    select_rows(&mut harness, 6, 7);
    let target = row_top(&harness, 2, 2.0);
    drag_id_to(&mut harness, 6, target);
    assert!(harness.state().view().row_drag.is_some());
    harness.key_press(Key::Escape);
    harness.step();
    assert!(harness.state().view().row_drag.is_none());
    release_at(&mut harness, target);
    let app = harness.state();
    assert_eq!(outline(app), before);
    assert_eq!(undo_len(app), undo);
    assert!(crate::select_mouse::table_focused(&harness));
}

/// H-UI-94 et H-UI-89 : appui sur l'ID d'une ligne **non** sélectionnée
/// puis glisser ⇒ sélection de lignes entières, aucun déplacement.
#[test]
fn h_ui_94_press_unselected_id_selects() {
    let mut harness = p_harness();
    let before = outline(harness.state());
    select_rows(&mut harness, 6, 7);
    let to = id_point(&harness, 4);
    drag_id_to(&mut harness, 2, to);
    assert!(harness.state().view().row_drag.is_none());
    release_at(&mut harness, to);
    let app = harness.state();
    assert_eq!(outline(app), before);
    assert_eq!(app.selected().kind(), SelectionKind::Rows);
    let uids: Vec<TaskUid> = app.selected().uids().iter().copied().collect();
    assert_eq!(uids, [uid(2), uid(3), uid(4)]);
}

/// Libellé « Colonne Durée » du titre d'une colonne.
fn grip_label(column: Column) -> String {
    let t = Texts::new(Language::Fr);
    t.get_args(
        TextId::ColumnResizeLabel,
        &[("column", Arg::Str(t.get(column.title()).into()))],
    )
}

fn width(app: &App, column: Column) -> u16 {
    app.state.column(column).unwrap().width
}

/// Largeur attendue de « Nom » ajusté sur « P » : plus large des noms
/// (retrait d'un cran par niveau, triangles de repli compris ; gras simulé
/// des récapitulatives) et du titre, plus les marges de 4 points.
fn expected_name_width(harness: &Harness<'static, App>) -> f32 {
    let font = egui::TextStyle::Body.resolve(&harness.ctx.global_style());
    let p = project(harness.state()).clone();
    harness.ctx.fonts_mut(|fonts| {
        let mut measure = |text: &str| {
            fonts
                .layout_no_wrap(text.to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
                .size()
                .x
        };
        let levels: Vec<u16> = p.tasks.iter().map(|t| t.outline_level.get()).collect();
        let title = measure(&fr(TextId::ColName));
        let widest = p
            .tasks
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let level = t.outline_level.get();
                let summary = levels.get(i + 1).is_some_and(|next| *next > level);
                measure(&t.name) + 14.0 * f32::from(level) + if summary { 0.6 } else { 0.0 }
            })
            .fold(title, f32::max);
        widest + 8.0
    })
}

/// H-UI-99 (U17, S88) : double-clic sur la bordure de « Nom » ⇒ largeur
/// ajustée au plus large des noms affichés (retrait et marges compris),
/// hors historique ; même chose par « Ajuster la largeur » du menu.
#[test]
fn h_ui_99_double_click_fits() {
    let mut harness = p_harness();
    let undo = undo_len(harness.state());
    assert_eq!(width(harness.state(), Column::Name), 220);
    let grip = harness.get_by_label(&grip_label(Column::Name)).rect().center();
    move_to(&mut harness, grip);
    for _ in 0..2 {
        for pressed in [true, false] {
            harness.event(egui::Event::PointerButton {
                pos: grip,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
        }
    }
    harness.run();
    let fitted = f32::from(width(harness.state(), Column::Name));
    let expected = expected_name_width(&harness);
    assert_ne!(width(harness.state(), Column::Name), 220, "largeur inchangée");
    assert!(
        fitted >= expected && fitted <= expected + 2.0,
        "ajustée à {fitted}, attendu {expected} (+ 1 arrondi)"
    );
    assert_eq!(undo_len(harness.state()), undo, "préférence, hors historique");
    // Durée élargie à la main puis ajustée depuis le menu : plus étroite.
    harness.state_mut().dispatch(Action::SetColumnWidth {
        column: Column::Duration,
        width: 400,
    });
    harness.run();
    harness
        .get_by_label(&header_label(Column::Duration))
        .click_secondary();
    harness.run();
    crate::click(&mut harness, &fr(TextId::MenuFitColumn));
    let w = width(harness.state(), Column::Duration);
    assert!((Column::MIN_WIDTH..120).contains(&w), "Durée ajustée à {w}");
}

/// H-UI-99 : poignée de ±4 points centrée sur le bord droit du titre ;
/// curseur de redimensionnement à 3,5 points du bord.
#[test]
fn h_ui_99_grip_width() {
    let mut harness = p_harness();
    let rect = harness.get_by_label(&grip_label(Column::Duration)).rect();
    assert!((rect.width() - 2.0 * RESIZE_GRIP).abs() < 0.01, "{rect:?}");
    let header = harness.get_by_label(&header_label(Column::Duration)).rect();
    assert!((rect.center().x - header.right()).abs() < 0.01);
    move_to(&mut harness, pos2(header.right() + 3.5, header.center().y));
    assert_eq!(cursor_icon(&harness), CursorIcon::ResizeColumn);
    move_to(&mut harness, pos2(header.right() - 3.5, header.center().y));
    assert_eq!(cursor_icon(&harness), CursorIcon::ResizeColumn);
    move_to(&mut harness, header.center());
    assert_ne!(cursor_icon(&harness), CursorIcon::ResizeColumn);
}

/// Libellé « Colonne Durée » du titre d'une colonne.
fn header_label(column: Column) -> String {
    let t = Texts::new(Language::Fr);
    t.get_args(
        TextId::ColumnHeaderLabel,
        &[("column", Arg::Str(t.get(column.title()).into()))],
    )
}

/// Ouvre le menu de l'en-tête `column`.
fn header_menu(harness: &mut Harness<'static, App>, column: Column) {
    harness.get_by_label(&header_label(column)).click_secondary();
    harness.run();
}

/// H-UI-99 (U20) : menu d'en-tête dans l'ordre Masquer, Ajuster, Trier
/// croissant, décroissant ; « Trier par ordre croissant » sur Durée ⇒
/// `SortSpec { Duration, descending: false }` (structure conservée) ;
/// décroissant ensuite ; l'ID croissant revient à l'ordre des ID.
#[test]
fn h_ui_99_header_menu_sort_and_fit() {
    let mut harness = p_harness();
    header_menu(&mut harness, Column::Duration);
    let entries = [
        TextId::ColumnMenuHide,
        TextId::MenuFitColumn,
        TextId::MenuSortAscending,
        TextId::MenuSortDescending,
    ];
    let ys: Vec<f32> = entries
        .iter()
        .map(|id| harness.get_by_label(&fr(*id)).rect().center().y)
        .collect();
    assert!(ys.windows(2).all(|w| w[0] < w[1]), "ordre du menu : {ys:?}");
    crate::click(&mut harness, &fr(TextId::MenuSortAscending));
    assert_eq!(
        harness.state().view().task_view.sort,
        Some(SortSpec {
            key: SortKey::Duration,
            descending: false,
            keep_outline: true,
        })
    );
    header_menu(&mut harness, Column::Start);
    crate::click(&mut harness, &fr(TextId::MenuSortDescending));
    assert_eq!(
        harness
            .state()
            .view()
            .task_view
            .sort
            .map(|s| (s.key, s.descending)),
        Some((SortKey::Start, true))
    );
    header_menu(&mut harness, Column::Id);
    crate::click(&mut harness, &fr(TextId::MenuSortAscending));
    assert!(!harness.state().view().task_view.is_sorted());
}

/// H-UI-99 : colonne sans clé de tri (Prédécesseurs) ⇒ entrées « Trier »
/// grisées ; « Ajuster la largeur » reste active.
#[test]
fn h_ui_99_unsortable_greyed() {
    let mut harness = p_harness();
    header_menu(&mut harness, Column::Predecessors);
    for id in [TextId::MenuSortAscending, TextId::MenuSortDescending] {
        assert!(
            harness.get_by_label(&fr(id)).accesskit_node().is_disabled(),
            "{id:?}"
        );
    }
    assert!(
        !harness
            .get_by_label(&fr(TextId::MenuFitColumn))
            .accesskit_node()
            .is_disabled()
    );
}

/// H-UI-103 (curseurs) : déplacement sur l'ID d'une ligne sélectionnée en
/// lignes entières ; rien de tel sur l'ID d'une ligne non sélectionnée ni
/// sur une cellule de la ligne sélectionnée.
#[test]
fn h_ui_103_id_move_cursor() {
    let mut harness = p_harness();
    select_rows(&mut harness, 6, 7);
    let on_id = id_point(&harness, 6);
    move_to(&mut harness, on_id);
    assert_eq!(cursor_icon(&harness), CursorIcon::Move);
    let other = id_point(&harness, 2);
    move_to(&mut harness, other);
    assert_ne!(cursor_icon(&harness), CursorIcon::Move);
    let name = cell_point(&harness, 5, Column::Name);
    move_to(&mut harness, name);
    assert_ne!(cursor_icon(&harness), CursorIcon::Move);
    // Bloc de cellules (pas des lignes entières) : pas de déplacement.
    press_at(&mut harness, name, Modifiers::NONE);
    release_at(&mut harness, name);
    move_to(&mut harness, on_id);
    assert_ne!(cursor_icon(&harness), CursorIcon::Move);
}
