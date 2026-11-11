//! Seconde passe UX (ADR-0019) : scénario de bout en bout de la seconde
//! passe UX (lot L11, vérification 2 de `docs/plans/ux2.md`).
//!
//! `e2e_ux2` enchaîne sur l'exemple embarqué (la maison de la capture du
//! propriétaire, 49 tâches, UID = ID à l'ouverture ; 9 → 10 → 11 → 12 → 13
//! liées, 13 → 14 non : relu dans `assets/demo/house.xml` par le lot L2, le
//! cahier disait « 11 → 12 non ») les gestes de l'audit **dans l'ordre du
//! § 0**, à la souris sauf les raccourcis qu'ils citent : n° 1, 2, 4 et 18,
//! 9, 10, 11, 12, 20, OK-01, 3, 13 et 14, 15 et 16, 5, 6, 7 et 17, 8, 21.
//! Chaque geste d'édition ajoute exactement une entrée d'annulation, chaque
//! non-changement aucune. Puis annuler tout (barre d'accès rapide) ⇒ projet
//! de l'ouverture, rétablir tout ⇒ projet édité, enregistrer par un
//! `FileService` en mémoire et relire ⇒ mêmes tâches, liens et champs
//! personnalisés ; relance (préférences enregistrées puis relues) ⇒ volets,
//! hauteurs et colonnes restaurés.
//!
//! Tests nommés des conventions du cahier qui n'en avaient pas encore à
//! leur nom (vérification 1) : `h_ui_106_*` (collage selon la
//! destination), `h_ui_107_*` (non-changement), `h_ui_120_*` (raccourcis
//! complémentaires).
//!
//! Préparations qui ne sont pas des gestes de l'audit, faites par action :
//! cellule active posée dans une colonne quand la sélection est en lignes
//! entières (le N° pose la colonne active sur l'ID), largeur de la table
//! (place de « Ajouter une nouvelle colonne »). Le pointeur est conduit par
//! de vrais événements egui (`select_mouse.rs`) ; les tâches sont désignées
//! par leur UID (les ID changent à l'insertion) et retrouvées dans la vue
//! par `VisibleRows::display_of`. Aucune date n'est calculée ici.

#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_precision_loss,
    reason = "tests : positions du pointeur et compteurs bornés par le scénario"
)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::Sender;

use egui::accesskit::Role;
use egui::{CursorIcon, Event, Key, Modifiers, PointerButton, Pos2, ViewportCommand, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use plannercraft_core::model::custom::{CustomFieldId, CustomValue};
use plannercraft_core::{LinkType, Moment, Project, TaskUid};
use plannercraft_ui_egui::action::PaneOp;
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::document;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::io::{OpenEvent, SaveEvent, SaveKind, SaveRequest};
use plannercraft_ui_egui::notify::MessageKind;
use plannercraft_ui_egui::ribbon::RibbonTab;
use plannercraft_ui_egui::selection::SelectionKind;
use plannercraft_ui_egui::state::DetailsForm;
use plannercraft_ui_egui::view::Pane;
use plannercraft_ui_egui::{Action, App, DocOrigin, Env, FileService, UiState};

use crate::select_mouse::{block, cell_point, click_at, move_to, press_at, release_at};
use crate::{click_where, fr, fr_light, in_ribbon, now, ribbon_click, ribbon_menu, screen_axis};

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
            name: "maison.xml".to_owned(),
            path: Some("/projets/maison.xml".into()),
        })
        .unwrap();
        ctx.request_repaint();
    }
}

/// Application (préférences `state`) ouverte sur l'exemple embarqué, dont
/// les enregistrements tombent dans `saved`.
fn demo_app(saved: &Saved, state: UiState) -> App {
    let env = Env {
        files: Box::new(MemFiles {
            saved: Rc::clone(saved),
        }),
        ..Env::fixed(now())
    };
    let mut app = App::new(state, env);
    let demo = document::load(plannercraft_ui_egui::DEMO_MSPDI, now()).unwrap();
    app.open_project(
        demo.session.project().clone(),
        DocOrigin {
            name: "maison.xml".to_owned(),
            path: None,
            demo: false,
        },
    );
    app
}

/// Harnais 1 440 × 900 (fenêtre du cahier) à images de 1/60 s (durées des
/// notifications en images, H-UI-104).
fn harness_of(app: App) -> Harness<'static, App> {
    let mut harness = Harness::builder()
        .with_size(vec2(1440.0, 900.0))
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .with_os(egui::os::OperatingSystem::Nix)
        .with_step_dt(1.0 / 60.0)
        .with_max_steps(8)
        .build_ui_state(plannercraft_ui_egui::ui, app);
    harness.run();
    harness
}

/// Exemple ouvert, préférences de test, aucun enregistrement attendu.
fn demo() -> Harness<'static, App> {
    harness_of(demo_app(&Rc::new(RefCell::new(Vec::new())), fr_light()))
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

/// Entrées annulables.
fn entries(app: &App) -> usize {
    history(app).0
}

/// Messages publiés depuis l'ouverture (historique, ≤ 20 : le scénario en
/// publie moins).
fn posted(app: &App) -> usize {
    app.message_store().history().len()
}

/// Ligne affichée de la tâche `n` (UID).
fn row(app: &App, n: u32) -> usize {
    app.visible_rows()
        .unwrap()
        .display_of(uid(n))
        .unwrap_or_else(|| panic!("tâche {n} non affichée"))
        .index()
}

/// Point au centre de la cellule (tâche `n`, `column`).
fn cell(harness: &Harness<'static, App>, n: u32, column: Column) -> Pos2 {
    cell_point(harness, row(harness.state(), n), column)
}

/// Clic sur la cellule (tâche `n`, `column`) avec `modifiers`.
fn click_cell(harness: &mut Harness<'static, App>, n: u32, column: Column, modifiers: Modifiers) {
    let p = cell(harness, n, column);
    click_at(harness, p, modifiers);
}

/// Clic sur le N° (ID) de la tâche `n` : ligne entière.
fn click_row(harness: &mut Harness<'static, App>, n: u32, modifiers: Modifiers) {
    click_cell(harness, n, Column::Id, modifiers);
}

/// Glisser de `from` à `to` en quatre pas, puis relâcher.
fn drag(harness: &mut Harness<'static, App>, from: Pos2, to: Pos2) {
    press_at(harness, from, Modifiers::NONE);
    for k in 1..=4_u8 {
        let t = f32::from(k) / 4.0;
        move_to(harness, from + (to - from) * t);
    }
    release_at(harness, to);
}

/// Molette de `dy` points au-dessus de `at`, puis stabilise.
fn wheel(harness: &mut Harness<'static, App>, at: Pos2, dy: f32) {
    harness.hover_at(at);
    harness.event(Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, dy),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
        modifiers: Modifiers::NONE,
    });
    // Défilement animé : une seconde d'images, puis stabilise.
    harness.run_steps(60);
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

/// Envoie Copier (Ctrl/Cmd+C) et rend le texte écrit dans le
/// presse-papiers par cette image.
fn copy(harness: &mut Harness<'static, App>) -> String {
    harness.event(Event::Copy);
    harness.step();
    let text = harness
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|c| match c {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        });
    harness.run();
    text.expect("rien écrit dans le presse-papiers")
}

/// Colle `text` (Ctrl/Cmd+V), puis stabilise.
fn paste(harness: &mut Harness<'static, App>, text: &str) {
    harness.event(Event::Paste(text.to_owned()));
    harness.run();
}

/// Frappe `text` (événement de texte), puis stabilise.
fn type_text(harness: &mut Harness<'static, App>, text: &str) {
    harness.event(Event::Text(text.to_owned()));
    harness.run();
}

/// Appui (avec `modifiers`) dans une seule image, sans relâcher : la
/// réponse est lue dans la sortie de cette image (H-UI-108).
fn press_once(harness: &mut Harness<'static, App>, modifiers: Modifiers, key: Key) {
    harness.input_mut().events.push(Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    });
    harness.step();
}

/// La fenêtre se ferme dans la dernière image.
fn closed(harness: &Harness<'static, App>) -> bool {
    crate::files::viewport_commands(harness).contains(&ViewportCommand::Close)
}

/// Bandeau « Enregistrer les modifications ? » affiché : « Annuler »
/// (on reste, rien de perdu).
fn cancel_confirmation(harness: &mut Harness<'static, App>) {
    harness.run();
    assert!(harness.state().confirmation_pending(), "pas de bandeau");
    crate::click(harness, &fr(TextId::ConfirmCancel));
    assert!(!harness.state().confirmation_pending());
    assert!(harness.state().document.is_some());
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

/// Lien `pred` → `succ` (UID) présent.
fn linked(app: &App, pred: u32, succ: u32) -> bool {
    preds(app, succ).contains(&pred)
}

/// Colonnes affichées, dans l'ordre.
fn order(app: &App) -> Vec<Column> {
    app.state.columns.iter().map(|c| c.column).collect()
}

/// Libellé « Colonne … » du titre affiché `title`.
fn header_label_of(title: &str) -> String {
    Texts::new(Language::Fr).get_args(TextId::ColumnHeaderLabel, &[("column", Arg::Str(title.into()))])
}

/// Libellé du titre par défaut de `column`.
fn header_label(column: Column) -> String {
    header_label_of(&fr(column.title()))
}

/// Nom accessible de la ligne d'ID `id`.
fn row_label(id: i64, name: &str) -> String {
    Texts::new(Language::Fr).get_args(
        TextId::RowLabel,
        &[("id", Arg::Count(id)), ("name", Arg::Str(name.to_owned().into()))],
    )
}

/// Rectangles des nœuds d'accessibilité nommés `label`.
fn rects(harness: &Harness<'static, App>, label: &str) -> Vec<egui::Rect> {
    harness.query_all_by_label(label).map(|n| n.rect()).collect()
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

/// Champ de texte nommé `label` focalisé par un clic, texte remplacé par
/// `text`.
fn retype(harness: &mut Harness<'static, App>, label: &str, text: &str) {
    harness.get_by_role_and_label(Role::TextInput, label).click();
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    type_text(harness, text);
}

/// « OK » de la fenêtre ouverte (le volet de détails, affiché, a le sien,
/// dans sa rangée d'en-tête depuis le lot U2).
fn dialog_ok(harness: &mut Harness<'static, App>) {
    // OK du volet : rangée d'en-tête (bordure, marge de 14 points, rangée
    // de 30 points), à droite.
    let pane = harness
        .state()
        .view()
        .panes
        .drawn
        .and_then(|d| d.details)
        .map(|r| {
            let y = r.top() + 1.0 + 14.0 + 15.0;
            egui::Rect::from_min_max(
                egui::pos2(r.right() - 200.0, y - 3.0),
                egui::pos2(r.right(), y + 3.0),
            )
        });
    {
        let label = fr(TextId::TaskInfoOk);
        let mut found = harness
            .query_all_by_label(&label)
            .filter(|n| pane.is_none_or(|r| !r.contains(n.rect().center())));
        let node = found.next().expect("« OK » de la fenêtre");
        assert!(found.next().is_none(), "« OK » en double hors du volet");
        node.click();
    }
    harness.run();
}

/// Nom accessible d'une cellule d'une grille du volet de détails.
fn details_cell(grid: TextId, column: TextId, row: usize) -> String {
    let column = format!("{} {}", fr(grid), fr(column));
    Texts::new(Language::Fr).get_args(
        TextId::TaskInfoRowField,
        &[
            ("column", Arg::Str(column.into())),
            ("count", Arg::Count(i64::try_from(row).unwrap())),
        ],
    )
}

/// Ligne comparée après relecture : UID, nom, niveau, durée d'une feuille
/// (celle d'une récapitulative est recalculée par le moteur à la lecture),
/// prédécesseurs (UID, type), champs personnalisés.
type SummaryRow = (
    u32,
    String,
    u16,
    Option<i64>,
    Vec<(u32, LinkType)>,
    Vec<(CustomFieldId, CustomValue)>,
);

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
                t.predecessors
                    .iter()
                    .map(|l| (l.predecessor.get(), l.kind))
                    .collect(),
                t.custom.iter().map(|(k, v)| (*k, v.clone())).collect(),
            )
        })
        .collect()
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "un scénario unique, lu de haut en bas comme la liste du cahier"
)]
fn e2e_ux2() {
    let saved: Saved = Rc::new(RefCell::new(Vec::new()));
    let harness = &mut harness_of(demo_app(&saved, fr_light()));
    let initial = project(harness.state()).clone();
    assert_eq!(initial.tasks.len(), 49);
    assert_eq!(history(harness.state()), (0, 0));
    assert!(linked(harness.state(), 12, 13) && !linked(harness.state(), 13, 14));
    let (dur, preds_col, name) = (Column::Duration, Column::Predecessors, Column::Name);
    let mut expected = 0_usize;

    // n° 1 (capture du propriétaire, H-UI-106, R-261) : N° 11, Ctrl/Cmd+C,
    // glisser Prédécesseurs 9-11, Ctrl/Cmd+V ⇒ refus en notification,
    // 49 tâches, rien écrit, aucun bandeau.
    click_row(harness, 11, Modifiers::NONE);
    assert_eq!(harness.state().selected().kind(), SelectionKind::Rows);
    let line_11_clip = copy(harness);
    let (from, to) = (cell(harness, 9, preds_col), cell(harness, 11, preds_col));
    drag(harness, from, to);
    let b = block(harness.state());
    assert_eq!(
        (b.kind, b.rows.len(), b.columns.as_slice()),
        (SelectionKind::Cells, 3, &[preds_col][..])
    );
    paste(harness, &line_11_clip);
    let app = harness.state();
    assert_eq!(project(app), &initial, "n° 1 : rien écrit");
    assert_eq!(entries(app), expected);
    let refusal = app
        .messages()
        .iter()
        .find(|m| m.text == fr(TextId::PasteRowsOnCells))
        .expect("n° 1 : refus affiché");
    assert_eq!(refusal.kind, MessageKind::Refusal);
    assert_eq!(app.message_store().banner(), [], "notification, pas de bandeau");

    // n° 2 (H-UI-107, R-260 b) : 11-14 en lignes entières, Tâche → Lier ⇒
    // seul 13 → 14 est créé, une entrée, aucun message ; Lier de nouveau ⇒
    // rien.
    click_row(harness, 11, Modifiers::NONE);
    click_row(harness, 14, Modifiers::SHIFT);
    assert_eq!(harness.state().selected().len(), 4);
    let messages = posted(harness.state());
    let before_12 = preds(harness.state(), 12);
    ribbon_click(harness, RibbonTab::Task, &fr(TextId::RibbonLink));
    expected += 1;
    let app = harness.state();
    assert_eq!(entries(app), expected, "n° 2 : une entrée");
    assert!(linked(app, 13, 14));
    assert_eq!(preds(app, 12), before_12);
    assert_eq!(posted(app), messages, "n° 2 : aucun message");
    ribbon_click(harness, RibbonTab::Task, &fr(TextId::RibbonLink));
    assert_eq!(entries(harness.state()), expected, "n° 2 : Lier de nouveau");
    assert_eq!(posted(harness.state()), messages);

    // n° 4 et 18 (H-UI-104) : lignes 3-4 copiées (le lien 2 → 3 vient d'une
    // tâche non copiée), collées avant la ligne 20 ⇒ 2 tâches, une entrée,
    // compte rendu en notification `Info` ; ruban et lignes au-dessus à la
    // même place ; partie avant 10 s.
    click_row(harness, 3, Modifiers::NONE);
    click_row(harness, 4, Modifiers::SHIFT);
    let two_rows_clip = copy(harness);
    click_row(harness, 20, Modifiers::NONE);
    let watched = [
        fr(TextId::RibbonTabTask),
        fr(TextId::RibbonTabView),
        row_label(1, "Études et permis"),
        row_label(11, "Fouilles des fondations"),
    ];
    let before: Vec<Vec<egui::Rect>> = watched.iter().map(|l| rects(harness, l)).collect();
    assert!(before.iter().all(|r| !r.is_empty()), "{before:?}");
    paste(harness, &two_rows_clip);
    expected += 1;
    let app = harness.state();
    assert_eq!(project(app).tasks.len(), 51, "{:?}", app.messages());
    assert_eq!(entries(app), expected, "n° 4 : une entrée");
    let report = app
        .messages()
        .iter()
        .find(|m| m.title == Some(TextId::PasteReportTitle))
        .cloned()
        .expect("n° 4 : compte rendu");
    assert_eq!(report.kind, MessageKind::Info);
    assert!(!report.text.contains('('), "{}", report.text);
    assert_eq!(app.message_store().banner(), []);
    assert_eq!(
        app.selected().kind(),
        SelectionKind::Rows,
        "lignes collées sélectionnées"
    );
    let after: Vec<Vec<egui::Rect>> = watched.iter().map(|l| rects(harness, l)).collect();
    assert_eq!(before, after, "n° 4 : aucune vue ne bouge");
    harness.run_steps(600);
    assert_eq!(harness.state().messages(), [], "n° 18 : parties en 10 s");
    assert_eq!(rects(harness, &report.text), []);

    // n° 9 (H-UI-105) : 12 puis Ctrl/Cmd+clic sur 9, Lier ⇒ 12 → 9 ferait
    // une boucle : refus nommé par les ID, détails repliés, rien écrit.
    click_row(harness, 12, Modifiers::NONE);
    click_row(harness, 9, Modifiers::COMMAND);
    ribbon_click(harness, RibbonTab::Task, &fr(TextId::RibbonLink));
    let app = harness.state();
    assert_eq!(entries(app), expected, "n° 9 : rien écrit");
    let cycle = app.messages().last().cloned().expect("n° 9 : refus");
    assert_eq!(cycle.kind, MessageKind::Refusal);
    let sentence = app.texts().get_args(
        TextId::RefusalLinkCycle,
        &[("tasks", Arg::Str("9, 10, 11, 12".into()))],
    );
    assert_eq!(cycle.text, sentence);
    for jargon in ["link.", "opération", "UID"] {
        assert!(!cycle.text.contains(jargon), "{jargon} : {}", cycle.text);
    }
    let detail = cycle.detail.clone().expect("détails");
    assert!(detail.contains("link.cycle"), "{detail}");
    assert!(harness.query_by_label(&detail).is_none(), "replié par défaut");
    harness.get_by_label(&fr(TextId::NotifyDetails)).click();
    harness.run();
    assert!(harness.query_by_label(&detail).is_some(), "déplié");

    // n° 10 (R-261) : « 6j » collé sur la ligne entière 13, cellule active
    // dans Durée ⇒ Durée de 13 seulement (5 jours à l'ouverture), aucune tâche insérée.
    click_row(harness, 13, Modifiers::NONE);
    harness.state_mut().view_mut().cursor.column = dur;
    paste(harness, "6j");
    expected += 1;
    let app = harness.state();
    assert_eq!(project(app).tasks.len(), 51, "n° 10 : aucune tâche insérée");
    assert_eq!(days(app, 13), 6);
    assert_eq!(entries(app), expected);
    assert_eq!(block(app).kind, SelectionKind::Cells, "bloc écrit sélectionné");

    // n° 11 (R-261) : défiler jusqu'à la ligne vide, clic dans sa cellule
    // Prédécesseurs, coller une colonne de texte ⇒ trois tâches sans nom en
    // fin, avec ces prédécesseurs (jamais nommées « 7;6 »).
    let last = row(harness.state(), 49);
    let table = harness.state().view().last_body_height;
    let over = cell(harness, 2, name);
    wheel(harness, over, -40.0 * table);
    let below_last = cell_point(harness, last, preds_col) + vec2(0.0, harness.state().row_h());
    click_at(harness, below_last, Modifiers::NONE);
    assert_eq!(harness.state().selection, None, "ligne vide choisie");
    paste(harness, "7;6\n9\n10");
    expected += 1;
    let app = harness.state();
    assert_eq!(project(app).tasks.len(), 54, "{:?}", app.messages());
    assert_eq!(entries(app), expected);
    let added: Vec<(&str, Vec<u32>)> = project(app)
        .tasks
        .iter()
        .skip(51)
        .map(|t| {
            (
                t.name.as_str(),
                t.predecessors.iter().map(|l| l.predecessor.get()).collect(),
            )
        })
        .collect();
    assert_eq!(added, [("", vec![7, 6]), ("", vec![9]), ("", vec![10])], "n° 11");
    let over = cell(harness, 49, name);
    wheel(harness, over, 40.0 * table);

    // n° 12 (R-260 a) : « 13;13 » collé sur les prédécesseurs de 17 ⇒ un
    // seul lien 13 → 17, une entrée, aucun refus.
    click_cell(harness, 17, preds_col, Modifiers::NONE);
    let messages = posted(harness.state());
    paste(harness, "13;13");
    expected += 1;
    let app = harness.state();
    assert_eq!(preds(app, 17), [13], "{:?}", app.messages());
    assert_eq!(entries(app), expected);
    assert_eq!(posted(app), messages, "n° 12 : aucun message");

    // n° 20 (R-262) : une cellule (Durée de 10, 4 jours) copiée, collée sur
    // le bloc Durée 11-12 ⇒ répétée, une entrée.
    click_cell(harness, 10, dur, Modifiers::NONE);
    let one = copy(harness);
    let (from, to) = (cell(harness, 11, dur), cell(harness, 12, dur));
    drag(harness, from, to);
    paste(harness, &one);
    expected += 1;
    let app = harness.state();
    assert_eq!((days(app, 11), days(app, 12)), (4, 4), "n° 20 : {one:?}");
    assert_eq!(entries(app), expected);

    // OK-01 (R-260) : bloc Nom-Durée 9-11 recollé sur lui-même, « 10 » et
    // « 10;10 » sur les prédécesseurs de 11 ⇒ aucun lot, aucun message.
    let (from, to) = (cell(harness, 9, name), cell(harness, 11, dur));
    drag(harness, from, to);
    let same = copy(harness);
    let messages = posted(harness.state());
    paste(harness, &same);
    click_cell(harness, 11, preds_col, Modifiers::NONE);
    paste(harness, "10");
    paste(harness, "10;10");
    assert_eq!(entries(harness.state()), expected, "OK-01 : aucun lot");
    assert_eq!(posted(harness.state()), messages, "OK-01 : aucun message");

    // n° 3 (H-UI-108) : Ctrl/Cmd+Q dans la vue, document modifié ⇒ bandeau,
    // la fenêtre reste ; Annuler.
    click_cell(harness, 2, name, Modifiers::NONE);
    press_once(harness, Modifiers::COMMAND, Key::Q);
    assert!(!closed(harness), "n° 3 : fermée sans garde");
    cancel_confirmation(harness);
    // n° 13 : sous « Trier » (Affichage → Trier ▾ → Trier…) ⇒ fenêtre
    // fermée comme par Annuler, puis bandeau.
    ribbon_menu(
        harness,
        RibbonTab::View,
        &fr(TextId::RibbonSort),
        &fr(TextId::MenuSort),
    );
    assert!(harness.state().sort_dialog().is_some(), "« Trier » ouverte");
    press_once(harness, Modifiers::COMMAND, Key::Q);
    assert!(harness.state().sort_dialog().is_none(), "n° 13 : fenêtre fermée");
    assert!(!closed(harness));
    cancel_confirmation(harness);
    assert_eq!(entries(harness.state()), expected, "n° 13 : rien appliqué");
    // n° 13 : pendant une saisie (Durée de 10, « 6 ») ⇒ saisie validée
    // d'abord (une entrée), puis bandeau.
    click_cell(harness, 10, dur, Modifiers::NONE);
    type_text(harness, "6");
    assert!(harness.state().view().edit.editing.is_some(), "saisie ouverte");
    press_once(harness, Modifiers::COMMAND, Key::Q);
    expected += 1;
    assert!(harness.state().view().edit.editing.is_none());
    assert!(!closed(harness));
    cancel_confirmation(harness);
    assert_eq!(days(harness.state(), 10), 6);
    assert_eq!(entries(harness.state()), expected, "n° 13 : saisie validée");
    // n° 14 (H-UI-109) : Ctrl/Cmd+W ⇒ Fermer le document, gardé ; Annuler.
    press_once(harness, Modifiers::COMMAND, Key::W);
    cancel_confirmation(harness);
    assert!(!closed(harness));

    // n° 15 (H-UI-110) : clic droit dans le vide de la frise ⇒ « Masquer la
    // chronologie » ; la case Affichage → Chronologie la réaffiche.
    let frise = harness.state().view().timeline.drawn.clone().expect("frise");
    let blank = pos2(frise.rect.left() + 6.0, frise.rect.center().y);
    assert!(frise.elements.iter().all(|(_, r)| !r.contains(blank)));
    right_click(harness, blank);
    crate::click(harness, &fr(TextId::TimelineHide));
    assert!(!harness.state().state.timeline_visible, "n° 15 : masquée");
    assert!(harness.state().view().timeline.drawn.is_none());
    ribbon_click(harness, RibbonTab::View, &fr(TextId::RibbonTimeline));
    assert!(harness.state().state.timeline_visible, "n° 15 : réaffichée");
    // n° 16 (H-UI-111) : séparateur sous la frise tiré de 80 points ⇒ frise
    // de 152 points, curseur de redimensionnement. Frise sans tâche : basse
    // et sans séparateur (H-UI-137) ; 2 y est d'abord ajoutée (une entrée).
    assert!(
        harness
            .state()
            .view()
            .panes
            .drawn
            .and_then(|d| d.timeline_splitter)
            .is_none(),
        "n° 16 : frise vide sans séparateur"
    );
    click_cell(harness, 2, name, Modifiers::NONE);
    ribbon_click(harness, RibbonTab::Task, &fr(TextId::RibbonAddToTimeline));
    expected += 1;
    assert_eq!(entries(harness.state()), expected, "n° 16 : 2 sur la frise");
    let grip = harness
        .state()
        .view()
        .panes
        .drawn
        .and_then(|d| d.timeline_splitter)
        .expect("séparateur")
        .center();
    harness.hover_at(grip);
    harness.run();
    assert_eq!(
        harness.output().platform_output.cursor_icon,
        CursorIcon::ResizeVertical
    );
    drag(harness, grip, grip + vec2(0.0, 80.0));
    assert_eq!(harness.state().state.timeline_height, 152, "n° 16");
    assert_eq!(entries(harness.state()), expected, "volets : aucune entrée");

    // n° 5 (H-UI-112, H-UI-113) : Affichage → Détails ; le volet suit 11 ;
    // forme Prédécesseurs et successeurs (clic droit) ; successeur 14 ajouté,
    // OK ⇒ lien 11 → 14, une entrée ; Ctrl/Cmd+Z ⇒ retiré.
    // Tâche 11 choisie d'abord : le volet (360 points par défaut depuis la
    // relecture finale, C5) et la frise de 152 points laissent peu de
    // lignes à la table.
    click_cell(harness, 11, name, Modifiers::NONE);
    ribbon_click(harness, RibbonTab::View, &fr(TextId::RibbonDetails));
    assert!(harness.state().state.details.visible, "n° 5 : volet affiché");
    // Séparateur au-dessus du volet tiré de 40 points vers le haut ⇒
    // volet de 400 points.
    let grip = harness
        .state()
        .view()
        .panes
        .drawn
        .and_then(|d| d.details_splitter)
        .expect("séparateur du volet")
        .center();
    drag(harness, grip, grip - vec2(0.0, 40.0));
    assert_eq!(harness.state().state.details.height, 400, "n° 5 : volet agrandi");
    let pane = harness.state().view().panes.drawn.unwrap().details.unwrap();
    right_click(harness, pos2(pane.right() - 20.0, pane.bottom() - 10.0));
    crate::click(harness, &fr(TextId::DetailsFormPredecessorsSuccessors));
    assert_eq!(
        harness.state().state.details.form,
        DetailsForm::PredecessorsSuccessors
    );
    let draft = harness
        .state()
        .view()
        .details
        .as_ref()
        .unwrap()
        .draft
        .as_ref()
        .unwrap();
    assert_eq!(draft.uid, uid(11));
    assert_eq!(draft.successors.len(), 1);
    retype(
        harness,
        &details_cell(TextId::DetailsSuccessors, TextId::DetailsColId, 2),
        "14",
    );
    crate::click(harness, &fr(TextId::DetailsOk));
    expected += 1;
    assert!(
        linked(harness.state(), 11, 14),
        "n° 5 : {:?}",
        harness.state().messages()
    );
    assert_eq!(entries(harness.state()), expected, "n° 5 : une entrée");
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run();
    expected -= 1;
    assert!(!linked(harness.state(), 11, 14), "n° 5 : annulé");
    assert_eq!(history(harness.state()), (expected, 1));
    // Séparateur ramené (180 points vers le bas) ⇒ hauteur par défaut.
    let grip = harness
        .state()
        .view()
        .panes
        .drawn
        .and_then(|d| d.details_splitter)
        .unwrap()
        .center();
    drag(harness, grip, grip + vec2(0.0, 180.0));
    assert_eq!(harness.state().state.details.height, 220);

    // n° 6 (H-UI-114) : clic droit sur « Durée » → Insérer une colonne →
    // « trav » → Entrée ⇒ « Travail » entre Nom et Durée, rien d'écrit.
    harness.get_by_label(&header_label(dur)).click_secondary();
    harness.run();
    crate::click(harness, &fr(TextId::MenuInsertColumn));
    assert!(harness.state().view().picker.is_some(), "sélecteur ouvert");
    type_text(harness, "trav");
    harness.key_press(Key::Enter);
    harness.run();
    assert!(harness.state().view().picker.is_none());
    assert_eq!(
        order(harness.state())[..4],
        [Column::Id, name, Column::Work, dur],
        "n° 6"
    );
    // n° 7 : « Ajouter une nouvelle colonne » en bout de table ⇒ « Marge
    // libre » à sa place, en fin (table élargie : préparation).
    harness.state_mut().dispatch(Action::SetTableWidth(1000));
    harness.run();
    harness.get_by_label(&fr(TextId::AddNewColumn)).click();
    harness.run();
    type_text(harness, "marge libre");
    crate::click(harness, &fr(TextId::ColFreeSlack));
    assert_eq!(order(harness.state()).last(), Some(&Column::FreeSlack), "n° 7");
    // n° 17 (H-UI-115) : Paramètres de champ par le menu d'en-tête : titre
    // « Durée prévue ».
    harness.get_by_label(&header_label(dur)).click_secondary();
    harness.run();
    click_where(harness, |l| l.starts_with(&fr(TextId::MenuFieldSettings)));
    let _ = harness.get_by_label(&fr(TextId::FieldSettingsTitle));
    type_text(harness, "Durée prévue");
    dialog_ok(harness);
    assert_eq!(
        harness.state().state.column(dur).unwrap().title.as_deref(),
        Some("Durée prévue"),
        "n° 17"
    );
    let _ = harness.get_by_label(&header_label_of("Durée prévue"));
    // n° 17 (H-UI-116) : « Fin » sélectionnée par son titre, glissée devant
    // « Durée prévue » ⇒ déplacée.
    let fin = harness
        .get_by_label(&header_label(Column::Finish))
        .rect()
        .center();
    click_at(harness, fin, Modifiers::NONE);
    assert_eq!(harness.state().view().cursor.column, Column::Finish);
    let target = {
        let r = harness.get_by_label(&header_label_of("Durée prévue")).rect();
        pos2(r.left() + 12.0, r.center().y)
    };
    drag(harness, fin, target);
    assert_eq!(
        order(harness.state()),
        [
            Column::Id,
            name,
            Column::Work,
            Column::Finish,
            dur,
            Column::Start,
            preds_col,
            Column::FreeSlack,
        ],
        "n° 17 : Fin déplacée"
    );
    assert_eq!(
        history(harness.state()),
        (expected, 1),
        "colonnes : aucune entrée"
    );

    // n° 8 (H-UI-118) : Projet → Champs personnalisés : Texte1 renommé
    // « Lot » (une entrée) ; colonne insérée devant Début par le sélecteur
    // (« lot » trouve aussi « Pilotée par l'effort » : clic sur « Lot
    // (Texte1) ») ; « Gros œuvre » saisi sur 9 (une entrée).
    ribbon_click(harness, RibbonTab::Project, &fr(TextId::RibbonCustomFields));
    harness.get_by_role_and_label(Role::TextInput, "Texte1").click();
    harness.run();
    type_text(harness, "Lot");
    dialog_ok(harness);
    expected += 1;
    assert_eq!(history(harness.state()), (expected, 0), "n° 8 : renommé");
    let text1 = Column::Custom(CustomFieldId::TEXT1);
    harness
        .get_by_label(&header_label(Column::Start))
        .click_secondary();
    harness.run();
    crate::click(harness, &fr(TextId::MenuInsertColumn));
    type_text(harness, "lot");
    crate::click(harness, "Lot (Texte1)");
    let at = order(harness.state()).iter().position(|c| *c == text1);
    assert_eq!(at, Some(5), "n° 8 : {:?}", order(harness.state()));
    let _ = harness.get_by_label(&header_label_of("Lot (Texte1)"));
    click_cell(harness, 9, text1, Modifiers::NONE);
    type_text(harness, "Gros œuvre");
    harness.key_press(Key::Enter);
    harness.run();
    expected += 1;
    assert_eq!(
        project(harness.state())
            .tasks
            .get(uid(9))
            .unwrap()
            .custom
            .get(&CustomFieldId::TEXT1),
        Some(&CustomValue::Text("Gros œuvre".to_owned())),
        "n° 8 : {:?}",
        harness.state().messages()
    );
    assert_eq!(entries(harness.state()), expected);

    // n° 21 (H-UI-119) : case Affichage → Diagnostics cochée ; volet dessiné
    // seulement s'il y a des diagnostics (jamais vide en doublon des
    // pastilles de la barre d'état).
    ribbon_click(harness, RibbonTab::View, &fr(TextId::MenuDiagnostics));
    assert!(harness.state().state.diagnostics_open);
    let has_diagnostics = !harness.state().diagnostics_view().entries.is_empty();
    assert_eq!(
        plannercraft_ui_egui::view::diagnostics::drawn(&harness.ctx),
        has_diagnostics,
        "n° 21"
    );
    ribbon_click(harness, RibbonTab::View, &fr(TextId::MenuDiagnostics));
    assert!(!harness.state().state.diagnostics_open);

    // Annuler tout puis rétablir tout (barre d'accès rapide).
    let edited = project(harness.state()).clone();
    assert_eq!(history(harness.state()), (expected, 0));
    let undo = fr(TextId::MenuUndo);
    while entries(harness.state()) > 0 {
        quick_access(harness, &undo);
    }
    assert_eq!(project(harness.state()), &initial, "annuler tout ⇒ ouverture");
    assert_eq!(history(harness.state()), (0, expected));
    let redo = fr(TextId::MenuRedo);
    while history(harness.state()).1 > 0 {
        quick_access(harness, &redo);
    }
    assert_eq!(project(harness.state()), &edited, "rétablir tout ⇒ édité");
    assert_eq!(history(harness.state()), (expected, 0));

    // Enregistrer (barre d'accès rapide) et relire : mêmes tâches, liens,
    // champs personnalisés et alias.
    quick_access(harness, &fr(TextId::MenuSave));
    let request = saved.borrow().last().cloned().expect("enregistré");
    assert_eq!(request.kind, SaveKind::Mspdi);
    let reread = document::load(&request.bytes, now()).unwrap();
    let back = reread.session.project();
    assert_eq!(summary(back), summary(&edited));
    assert_eq!(back.custom_fields, edited.custom_fields);
    assert_eq!(
        back.custom_fields
            .get(&CustomFieldId::TEXT1)
            .and_then(|d| d.alias.as_deref()),
        Some("Lot")
    );

    // Relance : préférences enregistrées puis relues ⇒ volets, hauteurs et
    // colonnes restaurés.
    let state = harness.state().state.clone();
    let (read, warning) = UiState::from_json_lenient(&state.to_json().unwrap());
    assert!(warning.is_none(), "{warning:?}");
    let again = harness_of(demo_app(&Rc::new(RefCell::new(Vec::new())), read));
    let app = again.state();
    assert!(app.state.timeline_visible && app.state.details.visible);
    assert_eq!(app.state.timeline_height, 152);
    assert_eq!(app.state.details.height, 220);
    assert_eq!(app.state.details.form, DetailsForm::PredecessorsSuccessors);
    assert_eq!(app.state.columns, state.columns);
    // L'exemple rouvert a une frise vide : basse (H-UI-137), la hauteur
    // enregistrée revient avec une tâche.
    let drawn = app.view().panes.drawn.expect("volets dessinés");
    assert!(
        (drawn.timeline.unwrap().height() - plannercraft_ui_egui::view::timeline::COMPACT_H).abs() < 0.01,
        "{drawn:?}"
    );
    assert!(drawn.details.is_some());
    let _ = again.get_by_label(&header_label_of("Durée prévue"));
    let _ = again.get_by_label(&header_label(Column::FreeSlack));
}

/// H-UI-106 (R-261, S89) : le collage suit la destination. Ligne 11 copiée
/// par son N° : sur le bloc Prédécesseurs 9-11 ⇒ refus en notification
/// (49 tâches, aucune entrée) ; sur la ligne entière 12 ⇒ insérée avant
/// elle, une entrée ; Ctrl/Cmd+V de nouveau ⇒ insérée encore (les lignes
/// collées restent sélectionnées en lignes entières).
#[test]
fn h_ui_106_paste_follows_destination() {
    let mut harness = demo();
    let h = &mut harness;
    click_row(h, 11, Modifiers::NONE);
    let clip = copy(h);
    click_cell(h, 9, Column::Predecessors, Modifiers::NONE);
    click_cell(h, 11, Column::Predecessors, Modifiers::SHIFT);
    assert_eq!(block(h.state()).kind, SelectionKind::Cells);
    paste(h, &clip);
    assert_eq!(project(h.state()).tasks.len(), 49);
    assert_eq!(entries(h.state()), 0);
    let refusal = h.state().messages().last().cloned().unwrap();
    assert_eq!(
        (refusal.kind, refusal.text),
        (MessageKind::Refusal, fr(TextId::PasteRowsOnCells))
    );
    click_row(h, 12, Modifiers::NONE);
    paste(h, &clip);
    assert_eq!(project(h.state()).tasks.len(), 50);
    assert_eq!(entries(h.state()), 1);
    assert_eq!(h.state().selected().kind(), SelectionKind::Rows);
    paste(h, &clip);
    assert_eq!(project(h.state()).tasks.len(), 51);
    assert_eq!(entries(h.state()), 2);
}

/// H-UI-107 (R-260) : Lier sur 9-12, déjà liées ⇒ ni lot, ni entrée, ni
/// message ; durée réécrite à l'identique (« 4 jours » sur 4 jours) ⇒
/// rien ; glisser un lien sur une paire liée ⇒ seule exception, information
/// « Ces tâches sont déjà liées. ».
#[test]
fn h_ui_107_identical_rewrite_is_noop() {
    let mut harness = demo();
    let h = &mut harness;
    let initial = project(h.state()).clone();
    click_row(h, 9, Modifiers::NONE);
    click_row(h, 12, Modifiers::SHIFT);
    ribbon_click(h, RibbonTab::Task, &fr(TextId::RibbonLink));
    assert_eq!(project(h.state()), &initial);
    assert_eq!(entries(h.state()), 0);
    assert_eq!(posted(h.state()), 0);
    click_cell(h, 10, Column::Duration, Modifiers::NONE);
    let text = h
        .state()
        .rows()
        .unwrap()
        .text(row(h.state(), 10), Column::Duration)
        .to_owned();
    paste(h, &text);
    assert_eq!(entries(h.state()), 0, "{text}");
    assert_eq!(posted(h.state()), 0);
    h.state_mut().dispatch(Action::GestureRefused(
        plannercraft_automation::gesture::GestureError::AlreadyLinked,
    ));
    h.run();
    let info = h.state().messages().last().cloned().unwrap();
    assert_eq!(
        (info.kind, info.text),
        (MessageKind::Info, fr(TextId::LinkAlreadyLinked))
    );
    assert_eq!(project(h.state()), &initial);
}

/// H-UI-120 : Ctrl+Maj+F5 ⇒ Gantt défilé jusqu'à la barre de la tâche
/// active (sélection et document inchangés) ; Ctrl/Cmd+R ⇒ Remplir à
/// droite (R-258 amendée) : Durée de 10 recopiée dans son Début ⇒ refus
/// nommé (« 4 jours » n'est pas une date), rien écrit.
#[test]
fn h_ui_120_scroll_to_task_and_fill_right() {
    let mut harness = demo();
    let h = &mut harness;
    crate::ribbon_zoom(h, plannercraft_ui_egui::TimeScaleZoom::Day);
    click_cell(h, 2, Column::Name, Modifiers::NONE);
    h.state_mut().dispatch(Action::FocusPane(Pane::Table));
    let far = crate::max_scroll(h.state());
    assert!(far > 0.0, "Gantt plus large que sa vue");
    h.state_mut().view_mut().gantt_scroll_x = far;
    h.run();
    let bar = |app: &App| {
        let (start, _) = app.rows().unwrap().row(row(app, 2)).and_then(|r| r.bar).unwrap();
        screen_axis(app).x(start)
    };
    let view = |app: &App| (app.view().last_gantt_left, app.view().last_gantt_width);
    let (left, width) = view(h.state());
    assert!(bar(h.state()) < left, "barre hors de la vue au départ");
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::F5);
    h.run();
    let app = h.state();
    let x = bar(app);
    assert!(
        x >= left && x <= left + width,
        "{x} hors de [{left}, {}]",
        left + width
    );
    assert_eq!(app.selection, Some(uid(2)));
    assert_eq!(app.document.as_ref().unwrap().generation(), 0);
    // Ctrl/Cmd+R : Durée de 10 recopiée à droite dans Début ⇒ refus nommé
    // (« 4 jours » n'est pas une date), rien écrit.
    click_cell(h, 10, Column::Duration, Modifiers::NONE);
    click_cell(h, 10, Column::Start, Modifiers::SHIFT);
    h.key_press_modifiers(Modifiers::COMMAND, Key::R);
    h.run();
    let app = h.state();
    assert_eq!(entries(app), 0);
    let refusal = app.messages().last().cloned().expect("refus nommé");
    assert_eq!(refusal.title, Some(TextId::CommandErrorTitle));
    assert!(refusal.text.contains("10"), "{}", refusal.text);
}

/// H-UI-108 : volet de détails affiché, document intact, Ctrl/Cmd+Q ⇒
/// fermeture sans bandeau.
#[test]
fn h_ui_108_clean_quit_with_details_pane() {
    let mut harness = demo();
    let h = &mut harness;
    h.state_mut().dispatch(Action::Pane(PaneOp::ShowDetails(true)));
    h.run();
    press_once(h, Modifiers::COMMAND, Key::Q);
    assert!(closed(h));
    assert!(!h.state().confirmation_pending());
}
