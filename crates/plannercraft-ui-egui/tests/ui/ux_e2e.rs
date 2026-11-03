//! Scénario de bout en bout de la passe UX (ADR-0018, lot L6, vérification
//! 2 de `docs/plans/ux.md`) : `e2e_ux` enchaîne sur le projet « P » du
//! cahier (§ 2 : 1 « Gros œuvre » ⊃ 2, 3, 4 ; 5 « Second œuvre » ⊃ 6, 7 ;
//! UID = ID à l'ouverture) les gestes de l'audit **dans l'ordre du § 0**,
//! à la souris sauf les raccourcis qu'ils citent : U01, U05, U02, U03, U04,
//! U06, U07, U08, U12, U11, U10, U13, U14, U15, U16, U17, U20, U19, U21,
//! GANTT-02, U24 ; chaque geste d'édition ajoute exactement une entrée
//! d'annulation, les autres aucune. Puis annuler tout (barre d'accès
//! rapide) ⇒ projet égal à l'ouverture, rétablir tout ⇒ projet édité,
//! enregistrer par un `FileService` en mémoire et relire ⇒ mêmes tâches.
//! `e2e_ux_demo` rejoue les gestes qui dépendent de la taille du projet
//! (défilement automatique, tout sélectionner et copier, remplir, coller,
//! déplacer une ligne) sur l'exemple embarqué, avec le même aller-retour.
//!
//! Le pointeur est conduit par de vrais événements egui (appui,
//! déplacements, relâcher dans des images distinctes, `select_mouse.rs`) ;
//! les tâches sont désignées par leur UID (les ID changent au déplacement
//! de lignes) et retrouvées dans la vue par `VisibleRows::display_of`.
//! Aucune date n'est calculée ici : les durées attendues viennent des
//! textes saisis ou de l'exemple du cahier.

#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_precision_loss,
    reason = "tests : positions du pointeur et compteurs bornés par le scénario"
)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::Sender;

use egui::accesskit::Role;
use egui::{CursorIcon, Event, Key, Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use plannercraft_core::{Moment, Project, TaskUid};
use plannercraft_tests::daily::plan_p;
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::document;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::io::{OpenEvent, SaveEvent, SaveKind, SaveRequest};
use plannercraft_ui_egui::keymap::{KeyContext, web_intercepts};
use plannercraft_ui_egui::nav::RowRef;
use plannercraft_ui_egui::row_view::DisplayRow;
use plannercraft_ui_egui::rows::RowKind;
use plannercraft_ui_egui::selection::SelectionKind;
use plannercraft_ui_egui::view::Pane;
use plannercraft_ui_egui::{App, DocOrigin, Env, FileService};

use crate::select_mouse::{block, cell_point, header_point, move_to, press_at, release_at, table_focused};
use crate::{click_where, fr, fr_light, in_ribbon, now, open_demo, screen_axis};

/// Demandes d'enregistrement reçues par le service en mémoire.
type Saved = Rc<RefCell<Vec<SaveRequest>>>;

/// Service de fichiers en mémoire : garde les octets de chaque
/// enregistrement ; aucune ouverture.
struct MemFiles {
    saved: Saved,
}

impl FileService for MemFiles {
    fn pick_and_open(&mut self, ctx: &egui::Context, _: Moment, _: String, _: String, tx: Sender<OpenEvent>) {
        tx.send(OpenEvent::Cancelled).unwrap();
        ctx.request_repaint();
    }

    fn save(&mut self, ctx: &egui::Context, request: SaveRequest, tx: Sender<SaveEvent>) {
        self.saved.borrow_mut().push(request);
        tx.send(SaveEvent::Saved {
            name: "p.xml".to_owned(),
            path: Some("/projets/p.xml".into()),
        })
        .unwrap();
        ctx.request_repaint();
    }
}

/// Application de test (français, clair, Classique) dont les
/// enregistrements tombent dans `saved`.
fn app_with(saved: &Saved) -> App {
    let env = Env {
        files: Box::new(MemFiles {
            saved: Rc::clone(saved),
        }),
        ..Env::fixed(now())
    };
    App::new(fr_light(), env)
}

/// Harnais 1280×720 à images de 1/60 s (deux clics d'une même suite
/// d'événements forment un double-clic, délai d'egui 0,3 s).
fn harness_of(app: App) -> Harness<'static, App> {
    let mut harness = Harness::builder()
        .with_size(vec2(1280.0, 720.0))
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .with_os(egui::os::OperatingSystem::Nix)
        .with_step_dt(1.0 / 60.0)
        .with_max_steps(8)
        .build_ui_state(plannercraft_ui_egui::ui, app);
    harness.run();
    harness
}

const fn uid(n: u32) -> TaskUid {
    TaskUid::new(n)
}

fn project(app: &App) -> &Project {
    app.document.as_ref().unwrap().project()
}

/// (entrées annulables, entrées rétablissables).
fn history(app: &App) -> (usize, usize) {
    let h = app.document.as_ref().unwrap().session.history();
    (h.entries.len(), h.redo.len())
}

/// Ligne affichée de la tâche `n` (UID).
fn row(app: &App, n: u32) -> usize {
    app.visible_rows()
        .unwrap()
        .display_of(uid(n))
        .unwrap_or_else(|| panic!("tâche {n} non affichée"))
        .index()
}

/// Défilement vertical maximal du corps (lignes et ligne vide de fin).
fn max_scroll_y(app: &App) -> f32 {
    let len = app.visible_rows().unwrap().len() + 1;
    (app.row_h() * len as f32 - app.view().last_body_height).max(0.0)
}

/// Durée de la tâche `n` en jours de 8 h.
fn days(app: &App, n: u32) -> i64 {
    project(app).tasks.get(uid(n)).unwrap().duration.minutes().get() / 480
}

/// Prédécesseurs (UID) de la tâche `n`.
fn preds(app: &App, n: u32) -> Vec<u32> {
    project(app)
        .tasks
        .get(uid(n))
        .unwrap()
        .predecessors
        .iter()
        .map(|l| l.predecessor.get())
        .collect()
}

/// (UID, niveau) de chaque tâche, dans l'ordre du projet.
fn outline(app: &App) -> Vec<(u32, u16)> {
    project(app)
        .tasks
        .iter()
        .map(|t| (t.uid().get(), t.outline_level.get()))
        .collect()
}

/// Cellule active du bloc, en (UID, colonne).
fn active(app: &App) -> (u32, Column) {
    match block(app).active {
        (RowRef::Item(u), c) => (u.get(), c),
        other => panic!("cellule active sur la ligne vide : {other:?}"),
    }
}

/// Lignes affichées `first..=last`.
fn display(first: usize, last: usize) -> Vec<DisplayRow> {
    (first..=last).map(DisplayRow::at).collect()
}

/// Point au centre de la cellule (tâche `n`, `column`).
fn cell(harness: &Harness<'static, App>, n: u32, column: Column) -> Pos2 {
    cell_point(harness, row(harness.state(), n), column)
}

/// Clic (appui, relâcher) en `p` avec `modifiers`.
fn click_at(harness: &mut Harness<'static, App>, p: Pos2, modifiers: Modifiers) {
    press_at(harness, p, modifiers);
    release_at(harness, p);
}

/// Clic sur la cellule (tâche `n`, `column`).
fn click_cell(harness: &mut Harness<'static, App>, n: u32, column: Column, modifiers: Modifiers) {
    let p = cell(harness, n, column);
    click_at(harness, p, modifiers);
}

/// Appui en `from`, glisser en quatre pas jusqu'à `to` (bouton tenu).
fn drag_held(harness: &mut Harness<'static, App>, from: Pos2, to: Pos2) {
    press_at(harness, from, Modifiers::NONE);
    for k in 1..=4_u8 {
        let t = f32::from(k) / 4.0;
        move_to(harness, from + (to - from) * t);
    }
}

/// Glisser de `from` à `to`, puis relâcher.
fn drag(harness: &mut Harness<'static, App>, from: Pos2, to: Pos2) {
    drag_held(harness, from, to);
    release_at(harness, to);
}

/// Touche avec `modifiers`, puis stabilise.
fn key(harness: &mut Harness<'static, App>, modifiers: Modifiers, k: Key) {
    harness.key_press_modifiers(modifiers, k);
    harness.run();
}

/// Frappe `text` (événement de texte), puis stabilise.
fn type_text(harness: &mut Harness<'static, App>, text: &str) {
    harness.event(Event::Text(text.to_owned()));
    harness.run();
}

/// Envoie `event` (copier, couper) et rend le texte écrit dans le
/// presse-papiers par cette image.
fn clip_event(harness: &mut Harness<'static, App>, event: Event) -> Option<String> {
    harness.event(event);
    harness.step();
    let text = copied(harness);
    harness.run();
    text
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

/// Colle `text` (événement du presse-papiers), puis stabilise.
fn paste(harness: &mut Harness<'static, App>, text: &str) {
    harness.event(Event::Paste(text.to_owned()));
    harness.run();
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

/// Libellé « Colonne Durée » d'un titre (`id` : `ColumnHeaderLabel`) ou
/// d'une poignée (`ColumnResizeLabel`).
fn column_label(id: TextId, column: Column) -> String {
    let t = Texts::new(Language::Fr);
    t.get_args(id, &[("column", Arg::Str(t.get(column.title()).into()))])
}

/// Message au bandeau commençant par `prefix` ?
fn has_message(app: &App, prefix: &str) -> bool {
    app.messages().iter().any(|m| m.text.starts_with(prefix))
}

/// Clique l'unique élément de la barre d'accès rapide nommé `name`.
fn quick_access(harness: &mut Harness<'static, App>, name: &str) {
    {
        let mut found = harness
            .query_all_by_label(name)
            .filter(|n| n.accesskit_node().role() != Role::Tab && in_ribbon(harness, n));
        let node = found
            .next()
            .unwrap_or_else(|| panic!("« {name} » absent du ruban"));
        assert!(found.next().is_none(), "« {name} » en double dans le ruban");
        node.click();
    }
    harness.run();
}

/// Clique l'entrée de menu ouverte `label` (seule ou suivie de son
/// raccourci), hors du ruban.
fn menu_entry(harness: &mut Harness<'static, App>, label: &str) {
    let with_key = format!("{label} ");
    click_where(harness, |text| text == label || text.starts_with(&with_key));
}

/// Point du Gantt sur la ligne de la tâche `n` : milieu de sa barre
/// (`bar`) ou bord droit du Gantt, hors des barres.
fn gantt_point(harness: &Harness<'static, App>, n: u32, bar: bool) -> Pos2 {
    let app = harness.state();
    let i = row(app, n);
    let y = cell_point(harness, i, Column::Name).y;
    let view = app.view();
    if !bar {
        return pos2(view.last_gantt_left + view.last_gantt_width - 4.0, y);
    }
    let (start, finish) = app.rows().unwrap().row(i).and_then(|r| r.bar).unwrap();
    let axis = screen_axis(app);
    pos2(f32::midpoint(axis.x(start), axis.x(finish)), y)
}

/// Annule tout puis rétablit tout par la barre d'accès rapide : projet
/// égal à `initial`, puis à l'état édité ; rend le projet édité.
fn undo_redo_all(harness: &mut Harness<'static, App>, initial: &Project) -> Project {
    let edited = project(harness.state()).clone();
    let entries = history(harness.state()).0;
    assert!(entries > 0, "rien à annuler");
    let undo = fr(TextId::MenuUndo);
    while history(harness.state()).0 > 0 {
        quick_access(harness, &undo);
    }
    let app = harness.state();
    assert_eq!(project(app), initial, "annuler tout ⇒ projet de l'ouverture");
    assert_eq!(history(app), (0, entries));
    let redo = fr(TextId::MenuRedo);
    while history(harness.state()).1 > 0 {
        quick_access(harness, &redo);
    }
    let app = harness.state();
    assert_eq!(project(app), &edited, "rétablir tout ⇒ projet édité");
    assert_eq!(history(app), (entries, 0));
    edited
}

/// Ligne comparée après relecture : UID, nom, niveau, durée d'une feuille
/// (celle d'une récapitulative est recalculée par le moteur à la lecture),
/// prédécesseurs.
type SummaryRow = (u32, String, u16, Option<i64>, Vec<u32>);

fn summary(p: &Project) -> Vec<SummaryRow> {
    let levels: Vec<u16> = p.tasks.iter().map(|t| t.outline_level.get()).collect();
    p.tasks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let level = t.outline_level.get();
            let parent = levels.get(i + 1).is_some_and(|next| *next > level);
            (
                t.uid().get(),
                t.name.clone(),
                level,
                (!parent).then(|| t.duration.minutes().get()),
                t.predecessors.iter().map(|l| l.predecessor.get()).collect(),
            )
        })
        .collect()
}

/// Enregistre par la barre d'accès rapide et relit les octets écrits :
/// mêmes tâches que `edited`.
fn save_and_reread(harness: &mut Harness<'static, App>, saved: &Saved, edited: &Project) {
    let before = saved.borrow().len();
    quick_access(harness, &fr(TextId::MenuSave));
    let saved = saved.borrow();
    assert_eq!(saved.len(), before + 1, "une demande d'enregistrement");
    let request = saved.last().unwrap();
    assert_eq!(request.kind, SaveKind::Mspdi);
    let reread = document::load(&request.bytes, now()).unwrap();
    assert_eq!(summary(reread.session.project()), summary(edited));
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "un scénario unique, lu de haut en bas comme la liste du cahier"
)]
fn e2e_ux() {
    let saved: Saved = Rc::new(RefCell::new(Vec::new()));
    let mut app = app_with(&saved);
    app.open_project(
        plan_p().project().clone(),
        DocOrigin {
            name: "p.xml".to_owned(),
            path: None,
            demo: false,
        },
    );
    let harness = &mut harness_of(app);
    let initial = project(harness.state()).clone();
    assert_eq!(history(harness.state()), (0, 0));
    let (dur, start, name, preds_col) = (
        Column::Duration,
        Column::Start,
        Column::Name,
        Column::Predecessors,
    );

    // U01 (bug signalé, H-UI-88) : appui sur (2, Durée), glisser jusqu'à
    // (5, Début), relâcher ⇒ bloc de 8 cellules, active (5, Début), ancre
    // (2, Durée) ; la table a le clavier.
    let (from, to) = (cell(harness, 2, dur), cell(harness, 5, start));
    drag(harness, from, to);
    let app = harness.state();
    let b = block(app);
    assert_eq!(b.kind, SelectionKind::Cells);
    assert_eq!(b.rows, display(1, 4));
    assert_eq!(b.columns, [dur, start]);
    assert_eq!(active(app), (5, start));
    assert!(app.view().select_drag.is_none(), "glisser fermé au relâcher");
    assert!(table_focused(harness));

    // U05 : clic tremblé de 8 points sur (6, Nom) ⇒ (6, Nom) seule.
    let p = cell(harness, 6, name);
    press_at(harness, p, Modifiers::NONE);
    move_to(harness, p + vec2(8.0, 2.0));
    release_at(harness, p + vec2(8.0, 2.0));
    let app = harness.state();
    assert!(block(app).is_single());
    assert_eq!(active(app), (6, name));

    // U02 (H-UI-92) : (3, Durée), Maj+→ ⇒ bloc Durée-Début de la ligne 3.
    click_cell(harness, 3, dur, Modifiers::NONE);
    key(harness, Modifiers::SHIFT, Key::ArrowRight);
    let b = block(harness.state());
    assert_eq!(b.rows, display(2, 2));
    assert_eq!(b.columns, [dur, start]);

    // U03 (H-UI-89) : glisser les ID 2 → 6 ⇒ lignes 2 à 6 entières.
    let (from, to) = (cell(harness, 2, Column::Id), cell(harness, 6, Column::Id));
    drag(harness, from, to);
    let app = harness.state();
    assert_eq!(app.selected().kind(), SelectionKind::Rows);
    assert_eq!(block(app).rows, display(1, 5));

    // U04 (H-UI-91, sur « P ») : glisser au-delà du bas du corps ⇒ bloc
    // borné à la dernière tâche (jamais la ligne vide), défilement dans
    // [0, max] ; le défilement réel est rejoué sur l'exemple (`e2e_ux_demo`).
    let from = cell(harness, 3, name);
    let below = pos2(from.x, 700.0);
    press_at(harness, from, Modifiers::NONE);
    for _ in 0..10 {
        move_to(harness, below);
    }
    release_at(harness, below);
    let app = harness.state();
    assert_eq!(block(app).rows, display(2, 6), "jusqu'à la dernière tâche");
    let scroll = app.view().last_scroll_y;
    assert!(
        scroll.is_finite() && (0.0..=max_scroll_y(app)).contains(&scroll),
        "{scroll}"
    );

    // U06 (H-UI-92) : Ctrl/Cmd+A ⇒ 7/7 lignes entières, active (1, Nom).
    key(harness, Modifiers::COMMAND, Key::A);
    let app = harness.state();
    assert_eq!(app.selected().kind(), SelectionKind::Rows);
    assert_eq!(app.selected().len(), 7);
    assert_eq!(active(app), (1, name));
    // Web : Ctrl/Cmd+A, D, Ctrl+Espace, Maj+Espace retenus au navigateur ;
    // Ctrl/Cmd+C, X, V jamais (événements du presse-papiers).
    let scope = app.key_scope(&harness.ctx);
    assert!(scope.reads(
        KeyContext::Table,
        plannercraft_ui_egui::keymap::Binding::Ui(plannercraft_ui_egui::keymap::UiBinding::SelectAll)
    ));
    for (modifiers, k) in [
        (Modifiers::COMMAND, Key::A),
        (Modifiers::COMMAND, Key::D),
        (Modifiers::CTRL, Key::Space),
        (Modifiers::SHIFT, Key::Space),
    ] {
        assert!(web_intercepts(scope, modifiers, k), "{modifiers:?}+{k:?}");
    }
    for k in [Key::C, Key::X, Key::V] {
        assert!(!web_intercepts(scope, Modifiers::COMMAND, k), "{k:?}");
    }

    // U07 (H-UI-90) : clic sur « Durée » ⇒ colonne Durée de 1 à 7,
    // active (1, Durée), clavier à la table.
    let p = header_point(harness, dur);
    click_at(harness, p, Modifiers::NONE);
    let app = harness.state();
    let b = block(app);
    assert_eq!(b.rows, display(0, 6));
    assert_eq!(b.columns, [dur]);
    assert_eq!(active(app), (1, dur));
    assert!(table_focused(harness));

    // U08 (H-UI-87) : Ctrl/Cmd+clic sur la seule ligne sélectionnée ⇒ rien
    // ne change (jamais de ligne vide active).
    click_cell(harness, 3, name, Modifiers::NONE);
    click_cell(harness, 3, name, Modifiers::COMMAND);
    let app = harness.state();
    assert_eq!(app.selected().click_order(), [uid(3)]);
    assert_eq!(active(app), (3, name));
    assert_eq!(history(app), (0, 0), "sélection : rien d'écrit");

    // U12 (H-UI-95) : copier (3, Durée) ⇒ son texte, pas la ligne.
    click_cell(harness, 3, dur, Modifiers::NONE);
    let text = clip_event(harness, Event::Copy).expect("copie écrite");
    assert_eq!(text, "10 jours");
    // U11 (R-257) : collé sur (6, Durée) ⇒ 6 dure 10 j, toujours 7 tâches ;
    // puis « 7j » sur (3, Durée) ⇒ 3 dure 7 j ; une entrée chacun.
    click_cell(harness, 6, dur, Modifiers::NONE);
    paste(harness, &text);
    let app = harness.state();
    assert_eq!(days(app, 6), 10);
    assert_eq!(project(app).tasks.len(), 7);
    assert_eq!(history(app), (1, 0));
    click_cell(harness, 3, dur, Modifiers::NONE);
    paste(harness, "7j");
    let app = harness.state();
    assert_eq!(days(app, 3), 7);
    assert_eq!(project(app).tasks.len(), 7);
    assert_eq!(history(app), (2, 0));

    // U10 (R-258) : bloc (2, Durée)-(3, Durée) à la souris, Ctrl/Cmd+D ⇒ 3
    // dure 5 j comme 2.
    let (from, to) = (cell(harness, 2, dur), cell(harness, 3, dur));
    drag(harness, from, to);
    key(harness, Modifiers::COMMAND, Key::D);
    let app = harness.state();
    assert_eq!((days(app, 2), days(app, 3)), (5, 5));
    assert_eq!(history(app), (3, 0));

    // U13 (H-UI-97, R-259) : Suppr sur (3, Durée) ⇒ rien d'écrit, la tâche
    // reste, message « 1 cellule ne peut pas être vide ».
    click_cell(harness, 3, dur, Modifiers::NONE);
    key(harness, Modifiers::NONE, Key::Delete);
    let app = harness.state();
    assert_eq!(project(app).tasks.len(), 7);
    assert_eq!(history(app), (3, 0));
    let not_clearable =
        Texts::new(Language::Fr).get_args(TextId::CellsNotClearable, &[("count", Arg::Count(1))]);
    assert!(has_message(app, &not_clearable), "{:?}", app.messages());

    // U14 : Retour arrière puis Entrée sur (7, Prédécesseurs) ⇒ lien 6→7
    // retiré, une entrée.
    assert_eq!(preds(harness.state(), 7), [6]);
    click_cell(harness, 7, preds_col, Modifiers::NONE);
    key(harness, Modifiers::NONE, Key::Backspace);
    assert!(
        harness.state().view().edit.editing.is_some(),
        "saisie vide ouverte"
    );
    key(harness, Modifiers::NONE, Key::Enter);
    let app = harness.state();
    assert!(app.view().edit.editing.is_none());
    assert_eq!(preds(app, 7), Vec::<u32>::new());
    assert_eq!(history(app), (4, 0));

    // U15 (H-UI-98) : bloc Durée 2-3 à la souris, frappe « 2 »,
    // Ctrl/Cmd+Entrée ⇒ 2 et 3 durent 2 j, une entrée, saisie fermée.
    let (from, to) = (cell(harness, 2, dur), cell(harness, 3, dur));
    drag(harness, from, to);
    type_text(harness, "2");
    assert!(harness.state().view().edit.editing.is_some());
    key(harness, Modifiers::COMMAND, Key::Enter);
    let app = harness.state();
    assert!(app.view().edit.editing.is_none());
    assert_eq!((days(app, 2), days(app, 3)), (2, 2));
    assert_eq!(history(app), (5, 0));
    assert!(table_focused(harness));

    // U16 (H-UI-94) : lignes 6-7 entières par leur ID (clic, Maj+clic),
    // glisser l'ID 6 au-dessus de 2 ⇒ 6 et 7 sous 1, avant 2 ; UID gardés,
    // une entrée ; curseur de déplacement pendant le glisser.
    click_cell(harness, 6, Column::Id, Modifiers::NONE);
    click_cell(harness, 7, Column::Id, Modifiers::SHIFT);
    assert_eq!(harness.state().selected().kind(), SelectionKind::Rows);
    let top_of_2 = {
        let p = cell(harness, 2, Column::Id);
        let h = harness.state().row_h();
        pos2(p.x, p.y - h / 2.0 + 2.0)
    };
    let from = cell(harness, 6, Column::Id);
    drag_held(harness, from, top_of_2);
    assert_eq!(harness.output().platform_output.cursor_icon, CursorIcon::Move);
    assert!(harness.state().view().row_drag.is_some());
    release_at(harness, top_of_2);
    let app = harness.state();
    assert_eq!(
        outline(app),
        [(1, 1), (6, 2), (7, 2), (2, 2), (3, 2), (4, 2), (5, 1)]
    );
    assert_eq!(history(app), (6, 0));
    assert!(app.selected().contains(uid(6)) && app.selected().contains(uid(7)));

    // U17 (H-UI-99) : double-clic sur la bordure de « Nom » ⇒ largeur
    // ajustée (préférence, hors historique).
    let before = app.state.column(name).unwrap().width;
    let grip = harness
        .get_by_label(&column_label(TextId::ColumnResizeLabel, name))
        .rect()
        .center();
    // Plus de 0,3 s depuis le dernier clic : les deux clics forment un
    // double-clic, pas un triple.
    for _ in 0..24 {
        move_to(harness, grip);
    }
    for _ in 0..2 {
        for pressed in [true, false] {
            harness.event(Event::PointerButton {
                pos: grip,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
        }
    }
    harness.run();
    let app = harness.state();
    assert_ne!(app.state.column(name).unwrap().width, before, "largeur ajustée");
    assert_eq!(history(app), (6, 0));

    // U20 (H-UI-99) : en-tête « Durée » → Trier par ordre croissant ⇒ tri
    // posé ; Maj+F3 ⇒ ordre des ID ; vue seulement, rien d'écrit.
    harness
        .get_by_label(&column_label(TextId::ColumnHeaderLabel, dur))
        .click_secondary();
    harness.run();
    menu_entry(harness, &fr(TextId::MenuSortAscending));
    assert!(harness.state().view().task_view.is_sorted());
    key(harness, Modifiers::SHIFT, Key::F3);
    let app = harness.state();
    assert!(!app.view().task_view.is_sorted());
    assert_eq!(history(app), (6, 0));

    // U19 (H-UI-100) : clic droit sur (7, Durée) hors de la sélection ⇒
    // cette cellule seule ; « Copier » écrit son texte.
    click_cell(harness, 2, name, Modifiers::NONE);
    let p = cell(harness, 7, dur);
    right_click(harness, p);
    let app = harness.state();
    assert!(block(app).is_single());
    assert_eq!(active(app), (7, dur));
    assert!(harness.ctx.any_popup_open(), "menu contextuel ouvert");
    let copy = fr(TextId::MenuCopy);
    let with_key = format!("{copy} ");
    {
        let mut found = harness
            .query_all_by(|n| n.label().is_some_and(|l| l == copy || l.starts_with(&with_key)))
            .filter(|n| n.accesskit_node().role() == Role::Button && !in_ribbon(harness, n));
        let node = found.next().expect("« Copier » au menu contextuel");
        assert!(found.next().is_none(), "« Copier » en double");
        node.click();
    }
    harness.step();
    assert_eq!(copied(harness).as_deref(), Some("3 jours"));
    harness.run();

    // U21 (H-UI-101) : clic dans le vide du Gantt sur la ligne de 2 ⇒ 2
    // sélectionnée, clavier à la table ; ↓ ⇒ 3 ; la frappe ouvre une saisie
    // visible dans la table ; Échap l'abandonne, rien d'écrit.
    let p = gantt_point(harness, 2, false);
    click_at(harness, p, Modifiers::NONE);
    let app = harness.state();
    assert_eq!(app.selection, Some(uid(2)));
    assert_eq!(app.view().focus, Pane::Table);
    assert!(table_focused(harness));
    key(harness, Modifiers::NONE, Key::ArrowDown);
    assert_eq!(harness.state().selection, Some(uid(3)));
    type_text(harness, "x");
    let editing = harness
        .state()
        .view()
        .edit
        .editing
        .as_ref()
        .expect("saisie ouverte");
    assert_eq!(editing.uid, Some(uid(3)));
    key(harness, Modifiers::NONE, Key::Escape);
    let app = harness.state();
    assert!(app.view().edit.editing.is_none());
    assert_eq!(history(app), (6, 0));

    // GANTT-02 (H-UI-102) : sélection {2, 3} (clic, Ctrl/Cmd+clic), glisser
    // la barre de 3 ⇒ 3 déplacée (une entrée), {2, 3} gardée.
    click_cell(harness, 2, name, Modifiers::NONE);
    click_cell(harness, 3, name, Modifiers::COMMAND);
    let constraint_before = project(harness.state()).tasks.get(uid(3)).unwrap().constraint;
    let from = gantt_point(harness, 3, true);
    drag(harness, from, from + vec2(60.0, 0.0));
    let app = harness.state();
    assert_eq!(history(app), (7, 0), "{:?}", app.messages());
    assert_ne!(
        project(app).tasks.get(uid(3)).unwrap().constraint,
        constraint_before
    );
    let mut kept: Vec<u32> = app.selected().uids().iter().map(|u| u.get()).collect();
    kept.sort_unstable();
    assert_eq!(kept, [2, 3], "sélection multiple gardée");
    assert!(table_focused(harness));

    // U24 (non-régression) : Maj+clic, Ctrl/Cmd+clic, clic droit dans la
    // multi-sélection (gardée), Maj+↓, Inser (une entrée).
    click_cell(harness, 6, name, Modifiers::NONE);
    click_cell(harness, 2, name, Modifiers::SHIFT);
    assert_eq!(harness.state().selected().len(), 3);
    click_cell(harness, 4, name, Modifiers::COMMAND);
    assert_eq!(harness.state().selected().len(), 4);
    let p = cell(harness, 7, name);
    right_click(harness, p);
    assert_eq!(harness.state().selected().len(), 4, "clic droit dedans : gardée");
    key(harness, Modifiers::NONE, Key::Escape);
    click_cell(harness, 2, name, Modifiers::NONE);
    key(harness, Modifiers::SHIFT, Key::ArrowDown);
    assert_eq!(block(harness.state()).rows, display(3, 4));
    key(harness, Modifiers::NONE, Key::Insert);
    let app = harness.state();
    assert_eq!(project(app).tasks.len(), 8);
    assert_eq!(history(app), (8, 0));

    // Annuler tout, rétablir tout, enregistrer et relire.
    let edited = undo_redo_all(harness, &initial);
    save_and_reread(harness, &saved, &edited);
}

/// Exemple embarqué : défilement automatique (U04), Ctrl/Cmd+A et copie
/// (U06), remplir (U10), coller (U11), déplacer une ligne (U16) ; une
/// entrée par geste d'édition ; annuler tout, rétablir tout, enregistrer
/// et relire.
#[test]
fn e2e_ux_demo() {
    let saved: Saved = Rc::new(RefCell::new(Vec::new()));
    let harness = &mut harness_of(app_with(&saved));
    open_demo(harness);
    let initial = project(harness.state()).clone();
    let len = harness.state().rows().unwrap().len();
    assert!(max_scroll_y(harness.state()) > 0.0, "l'exemple défile");

    // U01 : glisser un bloc de quatre lignes sur Durée-Début.
    let (from, to) = (
        cell_point(harness, 2, Column::Duration),
        cell_point(harness, 5, Column::Start),
    );
    drag(harness, from, to);
    let b = block(harness.state());
    assert_eq!(b.rows, display(2, 5));
    assert_eq!(b.columns, [Column::Duration, Column::Start]);

    // U04 : appui sur (3, Nom), pointeur tenu au bas du corps ⇒ la vue
    // défile et le bloc suit ; relâché ⇒ plus de défilement.
    let p = cell_point(harness, 3, Column::Name);
    let bottom = harness.state().view().last_body_height;
    press_at(harness, p, Modifiers::NONE);
    let last_visible = harness.state().last_frame.table_rows.end;
    // Bas du corps : sous la dernière ligne entièrement dessinée.
    let edge = pos2(p.x, p.y + bottom);
    for _ in 0..30 {
        move_to(harness, edge);
    }
    let app = harness.state();
    let scroll = app.view().last_scroll_y;
    assert!(scroll > 0.0 && scroll.is_finite(), "défilement {scroll}");
    let b = block(app);
    assert_eq!(b.rows.first(), Some(&DisplayRow(3)));
    assert!(b.rows.last().unwrap().index() >= last_visible);
    release_at(harness, edge);
    let still = harness.state().view().last_scroll_y;
    harness.run();
    assert_eq!(harness.state().view().last_scroll_y, still);
    assert_eq!(history(harness.state()), (0, 0));

    // U06 : Ctrl/Cmd+A puis copier ⇒ toutes les lignes entières, une
    // ligne de texte au moins par tâche.
    key(harness, Modifiers::COMMAND, Key::A);
    assert_eq!(harness.state().selected().len(), len);
    let text = clip_event(harness, Event::Copy).expect("copie écrite");
    assert!(text.lines().count() >= len, "{} lignes", text.lines().count());

    // Deux feuilles voisines de même niveau, avec une barre.
    let rows = harness.state().rows().unwrap();
    let pair = (1..len)
        .find(|&i| {
            let (a, b) = (rows.row(i - 1).unwrap(), rows.row(i).unwrap());
            a.kind == RowKind::Leaf && b.kind == RowKind::Leaf && a.level == b.level
        })
        .expect("deux feuilles voisines");
    let (ua, ub) = (rows.row(pair - 1).unwrap().uid, rows.row(pair).unwrap().uid);
    harness
        .state_mut()
        .dispatch(plannercraft_ui_egui::Action::ScrollToRow(pair - 1));
    harness.run();

    // U10 : bloc Durée des deux feuilles à la souris, Ctrl/Cmd+D ⇒ la
    // seconde reçoit la durée de la première.
    let (from, to) = (
        cell_point(harness, pair - 1, Column::Duration),
        cell_point(harness, pair, Column::Duration),
    );
    drag(harness, from, to);
    key(harness, Modifiers::COMMAND, Key::D);
    let app = harness.state();
    let duration = |app: &App, u: TaskUid| project(app).tasks.get(u).unwrap().duration;
    assert_eq!(duration(app, ub), duration(app, ua));
    assert_eq!(history(app), (1, 0));

    // U11 : « 3j » collé sur la Durée de la première ⇒ 3 jours.
    let p = cell_point(harness, pair - 1, Column::Duration);
    click_at(harness, p, Modifiers::NONE);
    paste(harness, "3j");
    let app = harness.state();
    assert_eq!(duration(app, ua).minutes().get(), 3 * 480);
    assert_eq!(history(app), (2, 0));

    // U16 : la seconde feuille, ligne entière par son ID, glissée au-dessus
    // de la première ⇒ ordre échangé, une entrée.
    let p = cell_point(harness, pair, Column::Id);
    click_at(harness, p, Modifiers::NONE);
    assert_eq!(harness.state().selected().kind(), SelectionKind::Rows);
    let target = {
        let q = cell_point(harness, pair - 1, Column::Id);
        pos2(q.x, q.y - harness.state().row_h() / 2.0 + 2.0)
    };
    drag(harness, p, target);
    let app = harness.state();
    let order: Vec<TaskUid> = project(app)
        .tasks
        .iter()
        .map(plannercraft_core::Task::uid)
        .collect();
    let pos = |u: TaskUid| order.iter().position(|x| *x == u).unwrap();
    assert!(pos(ub) < pos(ua), "{:?}", app.messages());
    assert_eq!(history(app), (3, 0));

    // Annuler tout, rétablir tout, enregistrer et relire.
    let edited = undo_redo_all(harness, &initial);
    save_and_reread(harness, &saved, &edited);
}
