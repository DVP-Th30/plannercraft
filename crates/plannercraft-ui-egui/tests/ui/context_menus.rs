//! Menus contextuels jumeaux du ruban (passe UX, ADR-0018, lot L4 ;
//! H-UI-100). Table, Gantt et en-tête de l'échelle : entrées dans l'ordre
//! du cahier, en français et en anglais ; clic droit hors de la sélection
//! ⇒ la cellule cliquée devient la sélection, dans la sélection ⇒ gardée ;
//! chaque entrée fait ce que fait son jumeau de la barre de menus (même
//! `RibbonCmd`) ; menu Édition : Remplir vers le bas, Effacer le contenu,
//! Tout sélectionner. Repère : projet « P » (`plannercraft_tests::daily::plan_p`,
//! UID n = ID n = ligne affichée n − 1).

#![allow(
    clippy::arithmetic_side_effects,
    reason = "tests : positions du pointeur bornées par l'exemple"
)]

use egui::accesskit::Role;
use egui::{Event, Key, KeyboardShortcut, Modifiers, PointerButton, Pos2, pos2};
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_kittest::{Harness, Node};
use plannercraft_core::TaskUid;
use plannercraft_tests::daily::plan_p;
use plannercraft_ui_egui::action::{PressMode, PressTarget, SelectOp};
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::i18n::{Language, TextId, Texts};
use plannercraft_ui_egui::keymap::KEYMAP;
use plannercraft_ui_egui::nav::RowRef;
use plannercraft_ui_egui::ribbon::spec::{CONTEXT_MENU, RibbonCmd};
use plannercraft_ui_egui::row_view::DisplayRow;
use plannercraft_ui_egui::selection::SelectionKind;
use plannercraft_ui_egui::timescale::TimeScaleZoom;
use plannercraft_ui_egui::view::Pane;
use plannercraft_ui_egui::{Action, App, DocOrigin, LanguagePref};

use crate::select_mouse::{block, cell_point, header_point, row_label};
use crate::{fr, harness_for, in_ribbon, new_app, screen_axis};

/// UID `n`.
const fn uid(n: u32) -> TaskUid {
    TaskUid::new(n)
}

/// Ligne affichée de la tâche `n` dans « P » (tout déplié).
fn index(n: u32) -> usize {
    usize::try_from(n - 1).unwrap()
}

/// Harnais sur le projet « P », stabilisé.
fn p() -> Harness<'static, App> {
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

/// Appui de sélection sur (`n`, `column`), focus à la table.
fn press(harness: &mut Harness<'static, App>, n: u32, column: Column, mode: PressMode) {
    harness.state_mut().dispatch(Action::SelectOp(SelectOp::Press {
        row: RowRef::Item(uid(n)),
        column,
        target: PressTarget::Cell,
        mode,
    }));
    harness.state_mut().dispatch(Action::FocusPane(Pane::Table));
    harness.run();
}

/// Bloc de (`a`, `from`) à (`b`, `to`).
fn select_block(harness: &mut Harness<'static, App>, (a, from): (u32, Column), (b, to): (u32, Column)) {
    press(harness, a, from, PressMode::Replace);
    press(harness, b, to, PressMode::Extend);
}

/// Clic droit (appui et relâcher) en `p`, puis stabilise.
fn right_click(harness: &mut Harness<'static, App>, p: Pos2) {
    harness.hover_at(p);
    harness.step();
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: p,
            button: PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        harness.step();
    }
    harness.run();
}

/// Clic droit sur la cellule (`n`, `column`).
fn right_click_cell(harness: &mut Harness<'static, App>, n: u32, column: Column) {
    let p = cell_point(harness, index(n), column);
    right_click(harness, p);
}

/// Point du Gantt sur la ligne de `n`, au milieu de sa barre (`bar`) ou
/// au bord droit du Gantt, hors des barres (sinon).
fn gantt_point(harness: &Harness<'static, App>, n: u32, bar: bool) -> Pos2 {
    let app = harness.state();
    let y = harness.get_by_label(&row_label(app, index(n))).rect().center().y;
    let view = app.view();
    if !bar {
        return pos2(view.last_gantt_left + view.last_gantt_width - 4.0, y);
    }
    let rows = app.rows().unwrap();
    let (start, finish) = rows.row(index(n)).and_then(|r| r.bar).unwrap();
    let axis = screen_axis(app);
    pos2(f32::midpoint(axis.x(start), axis.x(finish)), y)
}

/// Textes des raccourcis que peut afficher une entrée : ceux de `KEYMAP`
/// et Ctrl/Cmd+X, C, V.
fn shortcut_texts(ctx: &egui::Context) -> Vec<String> {
    let clipboard = [Key::X, Key::C, Key::V].map(|k| KeyboardShortcut::new(Modifiers::COMMAND, k));
    KEYMAP
        .iter()
        .map(|(_, shortcut, _)| *shortcut)
        .chain(clipboard)
        .map(|shortcut| ctx.format_shortcut(&shortcut))
        .collect()
}

/// Le nœud porte-t-il le libellé `label`, seul ou suivi d'un raccourci
/// (« Copier Ctrl+C », mais pas « Coller » pour « Collage spécial… ») ?
fn labelled(ctx: &egui::Context, node: &Node<'_>, label: &str) -> bool {
    let name = node.accesskit_node().label().unwrap_or_default();
    name == label
        || name
            .strip_prefix(label)
            .and_then(|rest| rest.strip_prefix(' '))
            .is_some_and(|rest| shortcut_texts(ctx).iter().any(|s| s == rest))
}

/// Entrée de menu (bouton ou case) hors du ruban.
fn is_entry(harness: &Harness<'static, App>, node: &Node<'_>) -> bool {
    matches!(node.accesskit_node().role(), Role::Button | Role::CheckBox) && !in_ribbon(harness, node)
}

/// Unique entrée de menu ouverte de libellé `label`.
fn entry<'a>(harness: &'a Harness<'static, App>, label: &str) -> Node<'a> {
    let prefix = label.to_owned();
    let mut found = harness
        .query_all_by(move |node| node.label().is_some_and(|l| l.starts_with(&prefix)))
        .filter(|n| labelled(&harness.ctx, n, label) && is_entry(harness, n));
    let node = found
        .next()
        .unwrap_or_else(|| panic!("entrée « {label} » absente"));
    assert!(found.next().is_none(), "entrée « {label} » en double");
    node
}

/// Clique l'entrée `label` du menu ouvert, puis stabilise.
fn click_entry(harness: &mut Harness<'static, App>, label: &str) {
    entry(harness, label).click();
    harness.run();
}

/// Libellés attendus du menu de la table (H-UI-100), dans l'ordre.
const TABLE_MENU: [TextId; 19] = [
    TextId::MenuCut,
    TextId::MenuCopy,
    TextId::MenuPaste,
    TextId::MenuPasteSpecial,
    TextId::MenuFillDown,
    TextId::MenuClearContents,
    TextId::StructureInsert,
    TextId::StructureDelete,
    TextId::StructureIndent,
    TextId::StructureOutdent,
    TextId::StructureLink,
    TextId::StructureUnlink,
    TextId::RibbonInactivate,
    TextId::RibbonManualSchedule,
    TextId::TaskInfoMenu,
    TextId::MenuTaskNotes,
    TextId::MenuAssignResources,
    TextId::RibbonScrollToTask,
    TextId::TimelineAdd,
];

/// Entrées du menu ouvert, de haut en bas : chaque libellé de `expected`
/// présent une fois, dans l'ordre, et rien d'autre dans la colonne du menu.
fn assert_menu(harness: &Harness<'static, App>, expected: &[String]) {
    let nodes: Vec<Node<'_>> = expected.iter().map(|label| entry(harness, label)).collect();
    let tops: Vec<f32> = nodes.iter().map(|n| n.rect().top()).collect();
    assert!(
        tops.windows(2).all(|w| w[0] < w[1]),
        "ordre {expected:?} : {tops:?}"
    );
    let left = nodes[0].rect().left();
    let (top, bottom) = (tops[0], nodes[nodes.len() - 1].rect().bottom());
    let column = harness
        .query_all_by(|_| true)
        .filter(|n| is_entry(harness, n))
        .filter(|n| {
            let r = n.rect();
            (r.left() - left).abs() < 0.5 && r.top() >= top - 0.5 && r.bottom() <= bottom + 0.5
        })
        .count();
    assert_eq!(column, expected.len(), "entrées en trop dans {expected:?}");
}

/// H-UI-100 (U19) : clic droit sur (6, Durée) ⇒ menu de la table, entrées
/// et séparateurs dans l'ordre du cahier, en français puis en anglais ;
/// Échap le referme.
#[test]
fn h_ui_100_table_menu_entries() {
    let mut harness = p();
    // Point relevé en français (libellé de la ligne) ; la langue ne change
    // pas la géométrie.
    let at = cell_point(&harness, index(6), Column::Duration);
    for (pref, language) in [(LanguagePref::Fr, Language::Fr), (LanguagePref::En, Language::En)] {
        harness.state_mut().dispatch(Action::SetLanguage(pref));
        harness.run();
        right_click(&mut harness, at);
        assert!(egui::Popup::is_any_open(&harness.ctx), "menu ouvert");
        let texts = Texts::new(language);
        let expected: Vec<String> = TABLE_MENU.iter().map(|id| texts.get(*id)).collect();
        assert_menu(&harness, &expected);
        harness.key_press(Key::Escape);
        harness.run();
        assert!(!egui::Popup::is_any_open(&harness.ctx), "{language:?}");
    }
    // Le cahier et la liste partagée disent la même chose.
    let flat: Vec<TextId> = CONTEXT_MENU
        .iter()
        .flat_map(|g| g.iter())
        .map(|cmd| cmd.context_label())
        .collect();
    assert_eq!(flat, TABLE_MENU);
    assert_eq!(CONTEXT_MENU.len(), 7, "sept groupes séparés");
}

/// H-UI-100 : clic droit dans le Gantt ⇒ mêmes entrées que la table ; sur
/// une barre, « Mettre en forme les barres de ce type » d'abord (H-UI-59) ;
/// hors des barres, la ligne cliquée devient la sélection.
#[test]
fn h_ui_100_gantt_menu_same_entries() {
    let mut harness = p();
    let texts = Texts::new(Language::Fr);
    let table: Vec<String> = TABLE_MENU.iter().map(|id| texts.get(*id)).collect();
    let p = gantt_point(&harness, 6, true);
    right_click(&mut harness, p);
    let mut with_bar = vec![fr(TextId::ContextFormatBars)];
    with_bar.extend(table.iter().cloned());
    assert_menu(&harness, &with_bar);
    assert_eq!(harness.state().selection, Some(uid(6)));
    harness.key_press(Key::Escape);
    harness.run();
    let p = gantt_point(&harness, 7, false);
    right_click(&mut harness, p);
    assert_menu(&harness, &table);
    assert_eq!(harness.state().selection, Some(uid(7)));
    assert_eq!(harness.state().selected().len(), 1);
}

/// H-UI-100 : (2, Nom) sélectionnée, clic droit sur (6, Durée) hors de la
/// sélection ⇒ (6, Durée) seule sélectionnée (colonne posée), table au
/// clavier, « Copier (Ctrl+C) » actif ; clic droit sur l'ID de 4 ⇒ la ligne
/// 4 entière.
#[test]
fn h_ui_100_right_click_outside_selects_cell() {
    let mut harness = p();
    press(&mut harness, 2, Column::Name, PressMode::Replace);
    right_click_cell(&mut harness, 6, Column::Duration);
    let b = block(harness.state());
    assert_eq!(b.kind, SelectionKind::Cells);
    assert_eq!(b.rows, [DisplayRow::at(index(6))]);
    assert_eq!(b.columns, [Column::Duration]);
    assert_eq!(b.active, (RowRef::Item(uid(6)), Column::Duration));
    assert_eq!(harness.state().view().focus, Pane::Table);
    let shortcut = harness
        .ctx
        .format_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::C));
    let copy = format!("{} {shortcut}", fr(TextId::MenuCopy));
    let node = entry(&harness, &fr(TextId::MenuCopy));
    assert_eq!(node.accesskit_node().label().unwrap_or_default(), copy);
    assert!(!node.accesskit_node().is_disabled());
    harness.key_press(Key::Escape);
    harness.run();
    right_click_cell(&mut harness, 4, Column::Id);
    let b = block(harness.state());
    assert_eq!(b.kind, SelectionKind::Rows);
    assert_eq!(b.rows, [DisplayRow::at(index(4))]);
}

/// H-UI-100 : bloc 2-5 × Durée-Début, clic droit sur (3, Début) ⇒ bloc
/// gardé ; lignes 6-7 entières, clic droit sur (7, Nom) ⇒ gardées.
#[test]
fn h_ui_100_right_click_inside_keeps_block() {
    let mut harness = p();
    select_block(&mut harness, (2, Column::Duration), (5, Column::Start));
    let before = block(harness.state());
    assert_eq!(before.rows.len() * before.columns.len(), 8);
    right_click_cell(&mut harness, 3, Column::Start);
    assert_eq!(block(harness.state()), before);
    assert!(egui::Popup::is_any_open(&harness.ctx));
    harness.key_press(Key::Escape);
    harness.run();
    harness.state_mut().dispatch(Action::SelectOp(SelectOp::Press {
        row: RowRef::Item(uid(6)),
        column: Column::Name,
        target: PressTarget::RowHeader,
        mode: PressMode::Replace,
    }));
    harness.state_mut().dispatch(Action::SelectOp(SelectOp::Press {
        row: RowRef::Item(uid(7)),
        column: Column::Name,
        target: PressTarget::RowHeader,
        mode: PressMode::Extend,
    }));
    harness.run();
    let rows = block(harness.state());
    right_click_cell(&mut harness, 7, Column::Name);
    assert_eq!(block(harness.state()), rows);
    assert_eq!(rows.kind, SelectionKind::Rows);
}

/// État observable d'une commande : projet, sélection, fenêtre, volet de
/// mise en forme, tampon de copie, messages, lots annulables, défilement.
fn digest(app: &App) -> String {
    let doc = app.document.as_ref().unwrap();
    format!(
        "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{}",
        doc.project(),
        app.selected(),
        app.view().dialog.as_ref().map(|d| d.kind),
        app.view().format_panel.as_ref().map(|p| p.role),
        app.clipboard().map(|slot| slot.clip.tasks.len()),
        app.messages().iter().map(|m| m.text.clone()).collect::<Vec<_>>(),
        app.view().pending_scroll_y,
        doc.session.history().entries.len(),
    )
}

/// « P » sans le lien 2→3 (Indenter 3 sous 2 ferait sinon d'une
/// récapitulative le prédécesseur de sa sous-tâche : diagnostic dont la
/// flèche manque à la police du harnais, ADR-0018 point 6), cellule active
/// (`n`, Durée), bloc Durée de `from` à `n`.
fn prepared(from: u32, n: u32) -> Harness<'static, App> {
    let mut harness = p();
    press(&mut harness, 3, Column::Predecessors, PressMode::Replace);
    harness
        .state_mut()
        .dispatch(Action::Cells(plannercraft_ui_egui::action::CellOp::Clear));
    harness.run();
    select_block(&mut harness, (from, Column::Duration), (n, Column::Duration));
    harness
}

/// Ouvre le chemin de la barre de menus `path` (menus et sous-menus).
fn open_menu_path(harness: &mut Harness<'static, App>, path: &[TextId]) {
    for step in path {
        let label = fr(*step);
        // Lot U2 (H-UI-173) : menus dans le bouton « Menu » de la barre du
        // haut.
        crate::open_menu_for(harness, &label);
        let node = harness
            .query_all_by_label(&label)
            .filter(|n| n.accesskit_node().role() != Role::Tab && !in_ribbon(harness, n))
            .min_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
            .unwrap_or_else(|| panic!("menu « {label} » absent"));
        node.hover();
        node.click();
        harness.run();
    }
}

/// Libellé de l'entrée jumelle de la barre de menus.
fn twin_label(cmd: RibbonCmd) -> String {
    match cmd {
        RibbonCmd::AddToTimeline => fr(TextId::TimelineAdd),
        _ => fr(cmd.menu_label()),
    }
}

/// H-UI-100, H-UI-70 : chaque entrée du menu contextuel, cliquée sur
/// (3, Durée) (Désindenter : (4, Durée) ; Lier, Délier : blocs de deux
/// lignes), laisse l'application dans le même état que son jumeau de la
/// barre de menus (même `RibbonCmd` : même action, même lot).
#[test]
fn h_ui_100_entries_match_ribbon_actions() {
    for cmd in CONTEXT_MENU.iter().flat_map(|g| g.iter()).copied() {
        // Hausser 3 en ferait la récapitulative de 4, qu'elle précède :
        // refus dont le message porte une flèche (police du harnais) ;
        // 4, dernière de son groupe, se hausse seule.
        // Lier : bloc Durée 2-3 (le lien 2→3 a été retiré) ; délier : bloc
        // 3-4 (lien 3→4) ; les deux commandes visent les tâches du bloc.
        let (from, n) = match cmd {
            RibbonCmd::Outdent => (4, 4),
            RibbonCmd::Link => (2, 3),
            RibbonCmd::Unlink => (3, 4),
            _ => (3, 3),
        };
        let prepared = || prepared(from, n);
        let mut by_context = prepared();
        right_click_cell(&mut by_context, n, Column::Duration);
        click_entry(&mut by_context, &fr(cmd.context_label()));
        let after_context = digest(by_context.state());

        let mut by_menu = prepared();
        open_menu_path(&mut by_menu, cmd.menu_path());
        click_entry(&mut by_menu, &twin_label(cmd));
        assert_eq!(after_context, digest(by_menu.state()), "{cmd:?}");
        let untouched = digest(prepared().state());
        // Sans effet visible ici : Coller sans tampon (le texte du système
        // n'arrive que par l'événement Ctrl+V), défiler (ligne déjà à
        // l'écran) ; les autres changent l'état.
        if !matches!(cmd, RibbonCmd::Paste | RibbonCmd::ScrollToTask) {
            assert_ne!(after_context, untouched, "{cmd:?} sans effet");
        }
    }
}

/// H-UI-100 : bloc Durée 2-3, clic droit dedans ⇒ « Copier » écrit le même
/// texte tabulé que Ctrl+C (cellules, pas les lignes).
#[test]
fn h_ui_100_copy_from_menu_copies_block() {
    let mut harness = p();
    select_block(&mut harness, (2, Column::Duration), (3, Column::Duration));
    harness.event(Event::Copy);
    harness.step();
    let by_key = copied(&harness).expect("Ctrl+C écrit le texte");
    harness.run();
    right_click_cell(&mut harness, 2, Column::Duration);
    entry(&harness, &fr(TextId::MenuCopy)).click();
    harness.step();
    let by_menu = copied(&harness).expect("Copier écrit le texte");
    harness.run();
    assert_eq!(by_menu, by_key);
    assert_eq!(by_menu.lines().count(), 2, "{by_menu:?}");
    assert!(!by_menu.contains('\t'), "une colonne : {by_menu:?}");
}

/// Texte écrit dans le presse-papiers par la dernière image.
fn copied(harness: &Harness<'static, App>) -> Option<String> {
    harness
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|c| match c {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        })
}

/// H-UI-100 : clic droit sur l'en-tête de l'échelle ⇒ Échelle de temps,
/// Zoom avant, Zoom arrière, Projet entier ; Zoom avant fait ce que fait le
/// ruban (`ZoomBy(1)`) ; le sous-menu « Échelle de temps » choisit l'unité.
#[test]
fn h_ui_100_timescale_menu_zoom() {
    let mut harness = p();
    let header = pos2(
        harness.state().view().last_gantt_left + 40.0,
        header_point(&harness, Column::Name).y,
    );
    right_click(&mut harness, header);
    let expected: Vec<String> = [
        TextId::RibbonTimescale,
        TextId::RibbonZoomIn,
        TextId::RibbonZoomOut,
        TextId::RibbonEntireProject,
    ]
    .iter()
    .map(|id| fr(*id))
    .collect();
    assert_menu(&harness, &expected);
    let mut reference = p();
    reference.state_mut().dispatch(Action::ZoomBy(1));
    reference.run();
    click_entry(&mut harness, &fr(TextId::RibbonZoomIn));
    assert_eq!(harness.state().state.zoom, reference.state().state.zoom);
    // Sous-menu des unités.
    right_click(&mut harness, header);
    let sub = entry(&harness, &fr(TextId::RibbonTimescale));
    sub.hover();
    sub.click();
    harness.run();
    let target = TimeScaleZoom::ALL
        .into_iter()
        .find(|z| *z != harness.state().state.zoom)
        .unwrap();
    click_entry(&mut harness, &fr(target.label()));
    assert_eq!(harness.state().state.zoom, target);
}

/// H-UI-100 : menu Édition ⇒ Remplir vers le bas (Ctrl+D), Effacer le
/// contenu (Ctrl+Suppr), Tout sélectionner (Ctrl+A), raccourcis de
/// `KEYMAP` affichés, actions des gestes clavier (une annulation chacun).
#[test]
fn h_ui_100_edit_menu_twins() {
    let mut harness = p();
    let shortcut = |key, mods| harness.ctx.format_shortcut(&KeyboardShortcut::new(mods, key));
    let fill = format!(
        "{} {}",
        fr(TextId::MenuFillDown),
        shortcut(Key::D, Modifiers::COMMAND)
    );
    let clear = format!(
        "{} {}",
        fr(TextId::MenuClearContents),
        shortcut(Key::Delete, Modifiers::COMMAND)
    );
    let all = format!(
        "{} {}",
        fr(TextId::MenuSelectAll),
        shortcut(Key::A, Modifiers::COMMAND)
    );
    open_menu_path(&mut harness, &[TextId::MenuEdit]);
    for label in [&fill, &clear, &all] {
        assert!(
            harness.query_by_label(label).is_some(),
            "« {label} » absent du menu Édition"
        );
    }
    harness.key_press(Key::Escape);
    harness.run();

    // Remplir : bloc Durée 2-3 ⇒ 3 dure 5 j comme 2, une annulation.
    select_block(&mut harness, (2, Column::Duration), (3, Column::Duration));
    let undo_before = history(harness.state());
    open_menu_path(&mut harness, &[TextId::MenuEdit]);
    click_entry(&mut harness, &fr(TextId::MenuFillDown));
    assert_eq!(duration(harness.state(), 3), duration(harness.state(), 2));
    assert_eq!(history(harness.state()), undo_before + 1);

    // Effacer : (3, Prédécesseurs) ⇒ lien 2→3 retiré, 3 reste.
    press(&mut harness, 3, Column::Predecessors, PressMode::Replace);
    assert!(!preds_empty(harness.state(), 3));
    open_menu_path(&mut harness, &[TextId::MenuEdit]);
    click_entry(&mut harness, &fr(TextId::MenuClearContents));
    assert!(preds_empty(harness.state(), 3));
    assert!(project_has(harness.state(), 3));
    assert_eq!(history(harness.state()), undo_before + 2);

    // Tout sélectionner : 7/7 lignes entières.
    open_menu_path(&mut harness, &[TextId::MenuEdit]);
    click_entry(&mut harness, &fr(TextId::MenuSelectAll));
    let app = harness.state();
    assert_eq!(app.selected().len(), 7);
    assert_eq!(block(app).kind, SelectionKind::Rows);
}

fn history(app: &App) -> usize {
    app.document.as_ref().unwrap().session.history().entries.len()
}

fn duration(app: &App, n: u32) -> i64 {
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
}

fn preds_empty(app: &App, n: u32) -> bool {
    app.document
        .as_ref()
        .unwrap()
        .project()
        .tasks
        .get(uid(n))
        .unwrap()
        .predecessors
        .is_empty()
}

fn project_has(app: &App, n: u32) -> bool {
    app.document.as_ref().unwrap().project().tasks.contains(uid(n))
}
