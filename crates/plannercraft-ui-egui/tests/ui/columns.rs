//! Colonnes de la table (J7, lot L6) : % achevé éditable, Noms ressources,
//! Début et Fin de référence, Variation de fin ; menu de l'en-tête
//! (afficher, masquer), largeur au bord de l'en-tête, défilement horizontal
//! de la table qui suit le curseur (H-UI-17).

#![allow(
    clippy::arithmetic_side_effects,
    clippy::float_cmp,
    reason = "tests : valeurs bornées par les fixtures ; positions exactes"
)]

use egui::{Event, Key, Modifiers, PointerButton, Pos2, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use plannercraft_automation::Batch;
use plannercraft_core::baseline::variance;
use plannercraft_core::{BaselineNumber, CalendarSet, TaskUid};
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::format::{format_date, format_slack};
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::nav::NavMove;
use plannercraft_ui_egui::{Action, App, DocOrigin};

use crate::{fr, harness, open_demo};

/// Fichier de suivi commité (lignes de base, avancement, affectations).
const TRACKING: &[u8] = include_bytes!("../../../../tests/mcp/expected/tracking_j6.xml");

fn texts() -> Texts {
    Texts::new(Language::Fr)
}

/// Libellé « Colonne Durée » du titre d'une colonne.
fn header_label(column: Column) -> String {
    let t = texts();
    t.get_args(
        TextId::ColumnHeaderLabel,
        &[("column", Arg::Str(t.get(column.title()).into()))],
    )
}

/// Libellé de la poignée de largeur d'une colonne.
fn grip_label(column: Column) -> String {
    let t = texts();
    t.get_args(
        TextId::ColumnResizeLabel,
        &[("column", Arg::Str(t.get(column.title()).into()))],
    )
}

fn visible(app: &App, column: Column) -> bool {
    app.state.shown(column)
}

/// Affiche `column` par le réducteur (une image).
fn show(harness: &mut Harness<'static, App>, column: Column) {
    if !visible(harness.state(), column) {
        harness.state_mut().dispatch(Action::ToggleColumn(column));
        harness.run();
    }
    assert!(visible(harness.state(), column));
}

/// Ouvre le fichier de suivi.
fn open_tracking(harness: &mut Harness<'static, App>) {
    let origin = DocOrigin {
        name: "tracking_j6.xml".to_owned(),
        path: None,
        demo: false,
    };
    harness.state_mut().open_bytes(TRACKING, origin);
    harness.run();
    assert!(
        harness.state().document.is_some(),
        "{:?}",
        harness.state().messages()
    );
}

fn index_of(app: &App, uid: u32) -> usize {
    app.rows().unwrap().index_of(TaskUid::new(uid)).unwrap()
}

/// Sélectionne la tâche `uid` et pose le curseur dans `column`, table
/// focalisée.
fn select_cell(harness: &mut Harness<'static, App>, uid: u32, column: Column) {
    harness
        .state_mut()
        .dispatch(Action::Select(Some(TaskUid::new(uid))));
    harness
        .state_mut()
        .dispatch(Action::FocusPane(plannercraft_ui_egui::view::Pane::Table));
    harness.run();
    let app = harness.state_mut();
    app.dispatch(Action::Nav {
        mv: NavMove::RowStart,
        extend: false,
    });
    harness.run();
    let target = harness
        .state()
        .state
        .visible_columns()
        .position(|c| c.column == column)
        .unwrap();
    for _ in 0..target {
        harness.state_mut().dispatch(Action::Nav {
            mv: NavMove::Right,
            extend: false,
        });
        harness.run();
    }
    assert_eq!(harness.state().view().cursor.column, column);
    assert_eq!(harness.state().selection, Some(TaskUid::new(uid)));
}

fn type_and_enter(harness: &mut Harness<'static, App>, text: &str) {
    harness.event(Event::Text(text.to_owned()));
    harness.run();
    assert!(
        harness.state().view().edit.editing.is_some(),
        "saisie non ouverte"
    );
    harness.key_press(Key::Enter);
    harness.run();
}

fn task_percent(app: &App, uid: u32) -> i64 {
    app.document
        .as_ref()
        .unwrap()
        .project()
        .tasks
        .get(TaskUid::new(uid))
        .unwrap()
        .progress
        .percent_complete
        .get()
}

fn cell(app: &App, uid: u32, column: Column) -> String {
    app.rows().unwrap().text(index_of(app, uid), column).to_owned()
}

/// % achevé saisi « 50 » sur une feuille : `progress.update`, texte
/// « 50 % », une entrée d'annulation.
#[test]
fn column_percent_edit_updates_progress() {
    let mut harness = harness();
    open_demo(&mut harness);
    show(&mut harness, Column::PercentComplete);
    assert_eq!(cell(harness.state(), 3, Column::PercentComplete), "0\u{a0}%");
    select_cell(&mut harness, 3, Column::PercentComplete);
    type_and_enter(&mut harness, "50");
    let app = harness.state();
    assert!(app.messages().is_empty(), "{:?}", app.messages());
    assert_eq!(task_percent(app, 3), 5_000);
    assert_eq!(cell(app, 3, Column::PercentComplete), "50\u{a0}%");
    assert_eq!(app.view().edit.editing, None);
    // Entrée descend d'une ligne (H-UI-13).
    assert_ne!(app.selection, Some(TaskUid::new(3)));
    // « 12,5 % » est lu, l'affichage est entier (⌊ ⌋).
    select_cell(&mut harness, 3, Column::PercentComplete);
    type_and_enter(&mut harness, "12,5 %");
    assert_eq!(task_percent(harness.state(), 3), 1_250);
    assert_eq!(cell(harness.state(), 3, Column::PercentComplete), "12\u{a0}%");
    // Texte illisible : message sous la cellule, saisie gardée, rien envoyé.
    select_cell(&mut harness, 3, Column::PercentComplete);
    type_and_enter(&mut harness, "150");
    let app = harness.state();
    assert!(
        app.view()
            .edit
            .editing
            .as_ref()
            .is_some_and(|e| e.error.is_some())
    );
    assert_eq!(task_percent(app, 3), 1_250);
}

/// % achevé saisi sur une récapitulative : refus du registre affiché
/// (bandeau), rien n'est modifié.
#[test]
fn column_percent_on_summary_shows_error() {
    let mut harness = harness();
    open_demo(&mut harness);
    show(&mut harness, Column::PercentComplete);
    let generation = harness.state().document.as_ref().unwrap().generation();
    select_cell(&mut harness, 1, Column::PercentComplete);
    type_and_enter(&mut harness, "50");
    let app = harness.state();
    assert_eq!(app.document.as_ref().unwrap().generation(), generation);
    assert_eq!(task_percent(app, 1), 0);
    let message = app.messages().last().expect("refus affiché");
    assert_eq!(
        message.severity,
        plannercraft_core::Severity::Error,
        "{message:?}"
    );
    assert_ne!(message.text, "");
}

/// Noms ressources (N11) : nom seul à 100 %, unités sinon, séparateur de
/// la langue ; lecture seule.
#[test]
fn column_resource_names_format() {
    let mut harness = harness();
    open_tracking(&mut harness);
    show(&mut harness, Column::ResourceNames);
    assert_eq!(cell(harness.state(), 3, Column::ResourceNames), "Maçon");
    assert_eq!(cell(harness.state(), 10, Column::ResourceNames), "Plaquiste");
    assert_eq!(cell(harness.state(), 2, Column::ResourceNames), "");
    let batch: Batch = serde_json::from_str(
        r#"{"format_version":1,"ops":[
            {"op":"assignment.add","task":3,"resource":3,"units":{"text":"50%"}}
        ]}"#,
    )
    .unwrap();
    harness.state_mut().dispatch(Action::Exec(batch));
    harness.run();
    assert!(
        harness.state().messages().is_empty(),
        "{:?}",
        harness.state().messages()
    );
    assert_eq!(
        cell(harness.state(), 3, Column::ResourceNames),
        "Maçon;Électricien[50\u{a0}%]"
    );
    // Anglais : séparateur « , », pourcentage collé.
    harness
        .state_mut()
        .dispatch(Action::SetLanguage(plannercraft_ui_egui::LanguagePref::En));
    harness.run();
    assert_eq!(
        cell(harness.state(), 3, Column::ResourceNames),
        "Maçon,Électricien[50%]"
    );
    // Lecture seule en J7 : la frappe n'ouvre pas de saisie.
    harness
        .state_mut()
        .dispatch(Action::SetLanguage(plannercraft_ui_egui::LanguagePref::Fr));
    harness.run();
    select_cell(&mut harness, 3, Column::ResourceNames);
    harness.event(Event::Text("x".to_owned()));
    harness.run();
    assert_eq!(harness.state().view().edit.editing, None);
}

/// Début et Fin de référence (ligne 0), Variation de fin en jours ouvrés
/// signés : textes du moteur (`baseline::variance`), données de ligne
/// (`Row::baseline`, `Row::finish_variance`, `Row::stop`).
#[test]
fn column_baseline_and_variance() {
    let mut harness = harness();
    open_tracking(&mut harness);
    for column in [
        Column::BaselineStart,
        Column::BaselineFinish,
        Column::FinishVariance,
    ] {
        show(&mut harness, column);
    }
    let app = harness.state();
    let doc = app.document.as_ref().unwrap();
    let project = doc.project();
    let labels = texts().format_labels().clone();
    let (set, _) = CalendarSet::compile(project);
    let all = variance(project, doc.schedule(), &set, BaselineNumber::PRIMARY);
    let rows = app.rows().unwrap();
    let mut varied = 0;
    let mut checked = 0;
    for task in &project.tasks {
        let i = rows.index_of(task.uid()).unwrap();
        let row = rows.row(i).unwrap();
        let base = task.baselines.get(&BaselineNumber::PRIMARY);
        let start = base.and_then(|b| b.start);
        let finish = base.and_then(|b| b.finish);
        assert_eq!(
            rows.text(i, Column::BaselineStart),
            start.map(|m| format_date(m, &labels)).unwrap_or_default(),
            "{}",
            task.name
        );
        assert_eq!(
            rows.text(i, Column::BaselineFinish),
            finish.map(|m| format_date(m, &labels)).unwrap_or_default()
        );
        assert_eq!(row.baseline, start.zip(finish));
        let v = all.tasks.iter().find(|v| v.uid == task.uid());
        let expected = v.filter(|_| finish.is_some()).map(|v| v.variance.finish);
        assert_eq!(row.finish_variance, expected, "{}", task.name);
        let text = expected
            .map(|m| format_slack(m, &project.options.factors, &labels))
            .unwrap_or_default();
        assert_eq!(rows.text(i, Column::FinishVariance), text);
        if expected.is_some_and(|m| m.get() != 0) {
            varied += 1;
        }
        if finish.is_some() {
            checked += 1;
        }
    }
    assert!(checked >= 10, "{checked} tâches avec une fin de base");
    assert!(varied > 0, "aucune variation non nulle dans le fichier de suivi");
    // Avancement : la tâche 3 (50 %) a une fin de partie réelle.
    let i = rows.index_of(TaskUid::new(3)).unwrap();
    let row = rows.row(i).unwrap();
    assert_eq!(row.percent.get(), 5_000);
    assert!(row.stop.is_some());
    assert_eq!(rows.text(i, Column::PercentComplete), "50\u{a0}%");
    // Variation affichée : la valeur de la ligne, signée, en jours.
    let first = rows.text(rows.index_of(TaskUid::new(1)).unwrap(), Column::FinishVariance);
    assert!(first.ends_with("jour") || first.ends_with("jours"), "{first}");
}

/// Clic droit sur un titre : menu des colonnes ; « Insérer une colonne »
/// puis « % achevé » l'affiche à gauche de la colonne cliquée (seconde
/// passe UX, H-UI-114 : les cases de colonnes du menu d'en-tête sont
/// remplacées par l'insertion) ; affichée en fin par la case du menu
/// Format, elle demande de défiler ; « Masquer la colonne » masque la
/// colonne cliquée.
#[test]
fn column_toggle_from_header_menu() {
    let mut harness = harness();
    open_demo(&mut harness);
    assert!(!visible(harness.state(), Column::PercentComplete));
    harness
        .get_by_label(&header_label(Column::Duration))
        .click_secondary();
    harness.run();
    crate::click(&mut harness, &fr(TextId::MenuInsertColumn));
    harness.event(Event::Text("% ach".to_owned()));
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();
    assert!(visible(harness.state(), Column::PercentComplete));
    let _ = harness.get_by_label(&header_label(Column::PercentComplete));
    harness
        .state_mut()
        .dispatch(Action::ToggleColumn(Column::PercentComplete));
    harness.run();
    assert!(!visible(harness.state(), Column::PercentComplete));
    // Case d'affichage (Format → Colonnes) : la colonne va au bout de la
    // table, qui défile désormais ; son titre apparaît une fois la table
    // défilée.
    show(&mut harness, Column::PercentComplete);
    assert!(
        harness
            .query_by_label(&header_label(Column::PercentComplete))
            .is_none()
    );
    harness.state_mut().dispatch(Action::ScrollTableX(1e9));
    harness.run();
    assert!(harness.state().view().table_scroll_x > 0.0);
    let _ = harness.get_by_label(&header_label(Column::PercentComplete));
    harness.state_mut().dispatch(Action::ScrollTableX(-1e9));
    harness.run();
    // Masquer la colonne Durée depuis son propre menu.
    harness
        .get_by_label(&header_label(Column::Duration))
        .click_secondary();
    harness.run();
    crate::click(&mut harness, &fr(TextId::ColumnMenuHide));
    assert!(!visible(harness.state(), Column::Duration));
    assert!(harness.query_by_label(&header_label(Column::Duration)).is_none());
}

/// ID et Nom ne se masquent pas (H-UI-12) : action sans effet, entrée du
/// menu grisée ; masquer la colonne du curseur le ramène sur le Nom.
#[test]
fn column_id_name_not_hideable() {
    let mut harness = harness();
    open_demo(&mut harness);
    for column in [Column::Id, Column::Name] {
        harness.state_mut().dispatch(Action::ToggleColumn(column));
        harness.run();
        assert!(visible(harness.state(), column), "{column:?}");
    }
    harness
        .get_by_label(&header_label(Column::Name))
        .click_secondary();
    harness.run();
    let hide_label = fr(TextId::ColumnMenuHide);
    let hide = harness.get_by_label(&hide_label);
    assert!(hide.accesskit_node().is_disabled());
    harness.key_press(Key::Escape);
    harness.run();
    // Curseur sur Durée, Durée masquée : curseur sur Nom.
    select_cell(&mut harness, 3, Column::Duration);
    harness
        .state_mut()
        .dispatch(Action::ToggleColumn(Column::Duration));
    harness.run();
    assert_eq!(harness.state().view().cursor.column, Column::Name);
}

/// Glisse le bord droit d'un titre de `dx` points.
fn drag_grip(harness: &mut Harness<'static, App>, column: Column, dx: f32) {
    let start = harness.get_by_label(&grip_label(column)).rect().center();
    let press = |pos: Pos2, pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    harness.event(Event::PointerMoved(start));
    harness.run();
    harness.event(press(start, true));
    harness.run();
    for k in [1.0_f32, 2.0, 3.0, 4.0] {
        let p = pos2(start.x + dx * k / 4.0, start.y);
        harness.event(Event::PointerMoved(p));
        harness.run();
    }
    let end = pos2(start.x + dx, start.y);
    harness.event(press(end, false));
    harness.run();
}

/// Bord droit d'un titre glissé : `SetColumnWidth`, borné.
#[test]
fn column_resize_by_edge() {
    let mut harness = harness();
    open_demo(&mut harness);
    let width = |app: &App| app.state.column(Column::Duration).unwrap().width;
    assert_eq!(width(harness.state()), 80);
    drag_grip(&mut harness, Column::Duration, 40.0);
    assert_eq!(width(harness.state()), 120);
    // Vers la gauche au-delà du bord gauche : largeur minimale.
    drag_grip(&mut harness, Column::Duration, -400.0);
    assert_eq!(width(harness.state()), Column::MIN_WIDTH);
    // Réducteur : bornes de l'état.
    harness.state_mut().dispatch(Action::SetColumnWidth {
        column: Column::Duration,
        width: u16::MAX,
    });
    harness.run();
    assert_eq!(width(harness.state()), Column::MAX_WIDTH);
}

/// Toutes les colonnes affichées (Σ > largeur de la table) : Fin du clavier
/// défile la table jusqu'à la dernière colonne, Origine revient au début ;
/// la cellule du curseur est toujours dans la partie visible (H-UI-17).
#[test]
fn table_horizontal_scroll_follows_cursor() {
    let mut harness = harness();
    open_demo(&mut harness);
    for column in Column::ALL {
        show(&mut harness, column);
    }
    let app = harness.state();
    let total: f32 = app.state.visible_columns().map(|c| f32::from(c.width)).sum();
    let view_w = app.view().last_table_width;
    assert!(total > view_w && view_w > 0.0, "{total} {view_w}");
    assert_eq!(app.view().table_scroll_x, 0.0);
    select_cell(&mut harness, 3, Column::Id);
    let in_view = |app: &App| {
        let mut left = 0.0;
        for c in app.state.visible_columns() {
            let w = f32::from(c.width);
            if c.column == app.view().cursor.column {
                let s = app.view().table_scroll_x;
                return left >= s - 0.5 && left + w <= s + app.view().last_table_width + 0.5;
            }
            left += w;
        }
        false
    };
    // Pas à pas vers la droite : le curseur reste visible.
    for _ in 1..Column::COUNT {
        harness.state_mut().dispatch(Action::Nav {
            mv: NavMove::Right,
            extend: false,
        });
        harness.run();
        assert!(
            in_view(harness.state()),
            "{:?}",
            harness.state().view().cursor.column
        );
    }
    // Dernière colonne : « Mode » (J8, lot L11).
    assert_eq!(harness.state().view().cursor.column, Column::Mode);
    let max = total - view_w;
    assert!((harness.state().view().table_scroll_x - max).abs() < 0.5);
    // Défilement borné : colonne « Ajouter une nouvelle colonne » comptée
    // (H-UI-114, lot L10).
    let max = max + plannercraft_ui_egui::view::table::ADD_COLUMN_W;
    // Origine : premier champ, défilement ramené à 0.
    harness.state_mut().dispatch(Action::Nav {
        mv: NavMove::RowStart,
        extend: false,
    });
    harness.run();
    assert_eq!(harness.state().view().table_scroll_x, 0.0);
    // Défilement demandé au-delà des bornes : borné.
    harness.state_mut().dispatch(Action::ScrollTableX(1e9));
    harness.run();
    assert!((harness.state().view().table_scroll_x - max).abs() < 0.5);
    harness.state_mut().dispatch(Action::ScrollTableX(f32::NAN));
    harness.run();
    assert!(harness.state().view().table_scroll_x.is_finite());
    // Colonnes masquées : tout tient, défilement ramené à 0.
    for column in Column::ALL.into_iter().filter(|c| c.hideable()) {
        if visible(harness.state(), column) {
            harness.state_mut().dispatch(Action::ToggleColumn(column));
        }
    }
    harness.run();
    harness.run();
    assert_eq!(harness.state().view().table_scroll_x, 0.0);
}
