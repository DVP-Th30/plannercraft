//! Grille commune et feuille des ressources (J7, lot L10) : la table des
//! tâches vue comme un `Sheet` (navigation identique à celle de L1), ajout
//! par la ligne vide, saisie de la capacité, suppression avec confirmation
//! de la cascade, navigation au clavier, H-UI-23 (surutilisation) et
//! H-UI-24 (taux dans leur unité d'origine), captures `resources_*`.
//!
//! Fichier : `tests/mcp/expected/resources_j5.xml` (commité) : Maçon (UID 1,
//! 200 %, six affectations), Électricien, Béton (matériel, m³), Location.

#![allow(
    clippy::arithmetic_side_effects,
    reason = "tests : valeurs bornées par les fixtures"
)]

use egui::{Event, Key, Modifiers, PointerButton, Pos2, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_automation::Batch;
use plannercraft_core::{ResourceUid, Units};
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::grid::{self, Sheet as _};
use plannercraft_ui_egui::i18n::{Arg, TextId};
use plannercraft_ui_egui::nav::{self, NavMove, RowRef};
use plannercraft_ui_egui::resource_rows::{COLUMNS, OVERALLOCATED_MARK, ResourceColumn, ResourceSheet};
use plannercraft_ui_egui::theme::Palette;
use plannercraft_ui_egui::view::ROW_H;
use plannercraft_ui_egui::view::table::{TaskSheet, table_id};
use plannercraft_ui_egui::{Action, App, DocOrigin, EditOrigin, LanguagePref, ThemePref, ViewKind};

use crate::common::{harness as snapshot_harness, snapshot};
use crate::{fr, harness, open_demo};

/// Fichier de ressources commité.
const RESOURCES: &[u8] = include_bytes!("../../../../tests/mcp/expected/resources_j5.xml");

/// Maçon : six affectations.
const MASON: u32 = 1;

/// Lot lu depuis son JSON.
fn batch(json: serde_json::Value) -> Batch {
    serde_json::from_value(json).unwrap()
}

/// Ouvre le fichier de ressources et affiche la feuille des ressources.
fn open_sheet(harness: &mut Harness<'static, App>) {
    let origin = DocOrigin {
        name: "resources_j5.xml".to_owned(),
        path: None,
        demo: false,
    };
    harness.state_mut().open_bytes(RESOURCES, origin);
    harness.run();
    assert!(
        harness.state().document.is_some(),
        "{:?}",
        harness.state().messages()
    );
    harness
        .state_mut()
        .dispatch(Action::SetView(ViewKind::ResourceSheet));
    harness.run();
    assert_eq!(harness.state().state.view, ViewKind::ResourceSheet);
}

/// Harnais sans rendu, feuille des ressources ouverte.
fn sheet_harness() -> Harness<'static, App> {
    let mut harness = harness();
    harness.run();
    open_sheet(&mut harness);
    harness
}

/// Nombre de ressources du projet.
fn resource_count(app: &App) -> usize {
    app.document.as_ref().unwrap().project().resources.len()
}

/// Génération du document.
fn generation(app: &App) -> u64 {
    app.document.as_ref().unwrap().generation()
}

/// (indice de ligne, colonne) du curseur de la feuille.
fn at(app: &App) -> (usize, ResourceColumn) {
    let rs = &app.view().resources;
    let index = match rs.cursor.row {
        RowRef::Item(uid) => rs.rows.index_of(uid).unwrap(),
        RowRef::Blank => rs.rows.len(),
    };
    (index, rs.cursor.column)
}

/// Libellé d'accessibilité de la ligne `i` (ligne vide au-delà).
fn row_label(app: &App, i: usize) -> String {
    let rows = &app.view().resources.rows;
    let texts = app.texts();
    match rows.row(i) {
        Some(row) => texts.get_args(
            TextId::ResRowLabel,
            &[
                ("id", Arg::Count(i64::try_from(row.id).unwrap())),
                (
                    "name",
                    Arg::Str(rows.text(i, ResourceColumn::Name).to_owned().into()),
                ),
            ],
        ),
        None => texts.get(TextId::ResBlankRowLabel),
    }
}

/// Centre de la cellule (ligne `i`, `column`).
fn cell_point(harness: &Harness<'static, App>, i: usize, column: ResourceColumn) -> Pos2 {
    let rect = harness.get_by_label(&row_label(harness.state(), i)).rect();
    let left = grid::column_span(&COLUMNS, rect.left(), column).unwrap();
    pos2(left.0 + left.1 / 2.0, rect.center().y)
}

/// Clic (simple ou double) sur la cellule (ligne `i`, `column`).
fn click_cell(harness: &mut Harness<'static, App>, i: usize, column: ResourceColumn, double: bool) {
    let p = cell_point(harness, i, column);
    harness.event(Event::PointerMoved(p));
    for _ in 0..if double { 2 } else { 1 } {
        for pressed in [true, false] {
            harness.event(Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
        }
    }
    harness.run();
    assert_eq!(at(harness.state()), (i, column));
    assert!(harness.ctx.memory(|memory| memory.has_focus(table_id())) || double);
}

/// Appuie sur `key` avec `modifiers`.
fn press(harness: &mut Harness<'static, App>, modifiers: Modifiers, key: Key) {
    harness.key_press_modifiers(modifiers, key);
    harness.run();
}

/// Frappe `text` puis Entrée.
fn type_enter(harness: &mut Harness<'static, App>, text: &str) {
    harness.event(Event::Text(text.to_owned()));
    harness.run();
    assert!(
        harness.state().view().resources.edit.editing.is_some(),
        "saisie ouverte"
    );
    press(harness, Modifiers::NONE, Key::Enter);
}

/// La table des tâches, vue comme un `Sheet`, rend les textes du
/// `RowModel` et se déplace exactement comme la navigation de L1 (le
/// réducteur), pour chaque déplacement depuis plusieurs cellules ; la suite
/// `nav.rs` (L1) passe inchangée.
#[test]
fn sheet_task_table_unchanged() {
    let mut harness = harness();
    open_demo(&mut harness);
    let app = harness.state();
    let rows = app.rows().unwrap();
    let texts = app.texts();
    let sheet = TaskSheet::new(app.visible_rows().unwrap(), &app.state, texts);
    assert_eq!(sheet.len(), rows.len());
    let visible: Vec<(Column, u16)> = app.state.visible_columns().map(|c| (c.column, c.width)).collect();
    assert_eq!(sheet.columns(), visible.as_slice());
    for i in 0..rows.len() {
        assert_eq!(sheet.key(i), Some(rows.row(i).unwrap().uid));
        assert_eq!(sheet.index_of(rows.row(i).unwrap().uid), Some(i));
        for (column, _) in &visible {
            assert_eq!(sheet.text(i, *column), rows.text(i, *column));
        }
    }
    let len = rows.len();
    let starts = [
        (0, Column::Id),
        (5, Column::Duration),
        (len, Column::Predecessors),
    ];
    for (row, column) in starts {
        for mv in NavMove::ALL {
            let uid = harness.state().rows().unwrap().row(row).map(|r| r.uid);
            harness.state_mut().dispatch(Action::Select(uid));
            harness.state_mut().dispatch(Action::Nav {
                mv: NavMove::RowStart,
                extend: false,
            });
            harness.run();
            for _ in 0..Column::COUNT {
                if harness.state().view().cursor.column == column {
                    break;
                }
                harness.state_mut().dispatch(Action::Nav {
                    mv: NavMove::Right,
                    extend: false,
                });
                harness.run();
            }
            let app = harness.state();
            assert_eq!(app.view().cursor.column, column);
            let sheet = TaskSheet::new(app.visible_rows().unwrap(), &app.state, app.texts());
            let page = nav::page_rows(app.view().last_body_height, ROW_H);
            let want = grid::nav_target(&sheet, &app.view().cursor, mv, page);
            harness.state_mut().dispatch(Action::Nav { mv, extend: false });
            harness.run();
            let app = harness.state();
            assert_eq!(
                RowRef::from_selection(app.selection),
                want.row,
                "{mv:?} depuis {row}"
            );
            assert_eq!(app.view().cursor.column, want.column, "{mv:?} depuis {row}");
        }
    }
}

/// Ligne vide : le nom saisi crée une ressource (`resource.add`, une entrée
/// d'annulation) ; la ligne vide reste courante pour saisir à la suite ;
/// une autre colonne de la ligne vide demande d'abord le nom.
#[test]
fn resource_sheet_add_on_blank_row() {
    let mut harness = sheet_harness();
    let before = resource_count(harness.state());
    let blank = harness.state().view().resources.rows.len();
    click_cell(&mut harness, blank, ResourceColumn::Name, false);
    type_enter(&mut harness, "Grutier");
    let app = harness.state();
    assert_eq!(resource_count(app), before + 1);
    assert_eq!(generation(app), 1);
    let rows = &app.view().resources.rows;
    assert_eq!(rows.text(rows.len() - 1, ResourceColumn::Name), "Grutier");
    assert_eq!(at(app), (rows.len(), ResourceColumn::Name), "ligne vide gardée");
    // Capacité sur la ligne vide : refus sous la cellule, rien de créé.
    press(&mut harness, Modifiers::NONE, Key::End);
    let blank = harness.state().view().resources.rows.len();
    click_cell(&mut harness, blank, ResourceColumn::MaxUnits, false);
    type_enter(&mut harness, "50");
    let app = harness.state();
    let editing = app.view().resources.edit.editing.as_ref().expect("saisie gardée");
    assert_eq!(editing.error.as_deref(), Some(fr(TextId::ResNameFirst).as_str()));
    assert_eq!(resource_count(app), before + 1);
    press(&mut harness, Modifiers::NONE, Key::Escape);
    assert!(harness.state().view().resources.edit.editing.is_none());
    // Une seule annulation retire la ressource.
    harness.state_mut().undo().unwrap();
    assert_eq!(resource_count(harness.state()), before);
}

/// Capacité saisie « 50 » ⇒ 50 % (lue par `plannercraft-automation`) ; texte
/// illisible : refus sous la cellule, document inchangé.
#[test]
fn resource_sheet_edit_max_units() {
    let mut harness = sheet_harness();
    click_cell(&mut harness, 0, ResourceColumn::MaxUnits, false);
    assert_eq!(
        harness
            .state()
            .view()
            .resources
            .rows
            .text(0, ResourceColumn::MaxUnits),
        "200\u{a0}%"
    );
    type_enter(&mut harness, "abc");
    let app = harness.state();
    assert!(
        app.view()
            .resources
            .edit
            .editing
            .as_ref()
            .unwrap()
            .error
            .is_some()
    );
    assert_eq!(generation(app), 0);
    press(&mut harness, Modifiers::NONE, Key::Escape);
    click_cell(&mut harness, 0, ResourceColumn::MaxUnits, false);
    type_enter(&mut harness, "50");
    let app = harness.state();
    let mason = app
        .document
        .as_ref()
        .unwrap()
        .project()
        .resources
        .get(ResourceUid::new(MASON))
        .unwrap();
    assert_eq!(mason.max_units, Units::new(5_000).unwrap());
    assert_eq!(
        app.view().resources.rows.text(0, ResourceColumn::MaxUnits),
        "50\u{a0}%"
    );
    assert_eq!(at(app), (1, ResourceColumn::MaxUnits), "Entrée descend");
    // F2 puis Entrée sans rien changer : aucun lot.
    press(&mut harness, Modifiers::NONE, Key::ArrowUp);
    press(&mut harness, Modifiers::NONE, Key::F2);
    assert!(harness.state().view().resources.edit.editing.is_some());
    press(&mut harness, Modifiers::NONE, Key::Enter);
    assert_eq!(generation(harness.state()), 1);
}

/// Suppr sur une ressource affectée : fenêtre de confirmation (refus
/// `resource.has_assignments` du registre), rien de supprimé avant
/// « Supprimer » ; puis suppression en cascade, une seule annulation. Une
/// ressource sans affectation est supprimée sans fenêtre.
#[test]
fn resource_sheet_delete_confirms_cascade() {
    let mut harness = sheet_harness();
    let before = resource_count(harness.state());
    let assignments = |app: &App| app.document.as_ref().unwrap().project().assignments.len();
    let assigned = assignments(harness.state());
    click_cell(&mut harness, 0, ResourceColumn::Name, false);
    press(&mut harness, Modifiers::NONE, Key::Delete);
    let app = harness.state();
    let pending = app
        .view()
        .resources
        .pending_delete
        .as_ref()
        .expect("confirmation demandée");
    assert_eq!((pending.uid, pending.count), (ResourceUid::new(MASON), 6));
    assert_eq!(resource_count(app), before);
    assert!(app.messages().is_empty(), "aucun bandeau : {:?}", app.messages());
    assert!(harness.query_by_label(&fr(TextId::ResDeleteTitle)).is_some());
    // Navigation coupée pendant la confirmation.
    press(&mut harness, Modifiers::NONE, Key::ArrowDown);
    assert_eq!(at(harness.state()).0, 0);
    crate::click(&mut harness, &fr(TextId::ConfirmDeleteNo));
    assert!(harness.state().view().resources.pending_delete.is_none());
    assert_eq!(resource_count(harness.state()), before);
    press(&mut harness, Modifiers::NONE, Key::Delete);
    crate::click(&mut harness, &fr(TextId::ConfirmDeleteYes));
    let app = harness.state();
    assert_eq!(resource_count(app), before - 1);
    assert_eq!(assignments(app), assigned - 6);
    assert_eq!(generation(app), 1);
    assert_eq!(at(app).0, 0, "curseur sur la ligne suivante");
    harness.state_mut().undo().unwrap();
    harness.run();
    assert_eq!(resource_count(harness.state()), before);
    assert_eq!(assignments(harness.state()), assigned);
    // Nouvelle ressource sans affectation : supprimée tout de suite.
    let blank = harness.state().view().resources.rows.len();
    click_cell(&mut harness, blank, ResourceColumn::Name, false);
    type_enter(&mut harness, "Grutier");
    assert_eq!(
        resource_count(harness.state()),
        before + 1,
        "{:?}",
        harness.state().messages()
    );
    let last = harness.state().view().resources.rows.len() - 1;
    click_cell(&mut harness, last, ResourceColumn::Name, false);
    press(&mut harness, Modifiers::NONE, Key::Delete);
    assert!(harness.state().view().resources.pending_delete.is_none());
    assert_eq!(resource_count(harness.state()), before);
}

/// Navigation de la table dans la feuille : flèches, Entrée, Tab,
/// Origine/Fin, Ctrl+Origine/Fin (ligne vide comprise) ; la sélection des
/// tâches ne bouge pas.
#[test]
fn resource_sheet_navigation_keys() {
    let mut harness = sheet_harness();
    let selection = harness.state().selection;
    click_cell(&mut harness, 0, ResourceColumn::Name, false);
    press(&mut harness, Modifiers::NONE, Key::ArrowDown);
    assert_eq!(at(harness.state()), (1, ResourceColumn::Name));
    press(&mut harness, Modifiers::NONE, Key::Tab);
    assert_eq!(at(harness.state()), (1, ResourceColumn::Kind));
    press(&mut harness, Modifiers::SHIFT, Key::Tab);
    press(&mut harness, Modifiers::NONE, Key::ArrowLeft);
    assert_eq!(at(harness.state()), (1, ResourceColumn::Overallocated));
    press(&mut harness, Modifiers::NONE, Key::End);
    assert_eq!(at(harness.state()), (1, ResourceColumn::Calendar));
    press(&mut harness, Modifiers::NONE, Key::Home);
    assert_eq!(at(harness.state()), (1, ResourceColumn::Id));
    press(&mut harness, Modifiers::NONE, Key::Enter);
    assert_eq!(at(harness.state()), (2, ResourceColumn::Id));
    press(&mut harness, Modifiers::SHIFT, Key::Enter);
    assert_eq!(at(harness.state()), (1, ResourceColumn::Id));
    let blank = harness.state().view().resources.rows.len();
    press(&mut harness, Modifiers::COMMAND, Key::End);
    assert_eq!(at(harness.state()), (blank, ResourceColumn::Calendar));
    press(&mut harness, Modifiers::NONE, Key::ArrowDown);
    assert_eq!(at(harness.state()), (blank, ResourceColumn::Calendar), "au bord");
    press(&mut harness, Modifiers::COMMAND, Key::Home);
    assert_eq!(at(harness.state()), (0, ResourceColumn::Id));
    let app = harness.state();
    assert!(app.view().resources.edit.editing.is_none());
    assert_eq!(app.selection, selection, "sélection des tâches inchangée");
    assert_eq!(generation(app), 0);
}

/// Liste de la colonne Calendrier : F2 l'ouvre sur le calendrier courant,
/// ↓ puis Entrée en choisit un autre (`resource.update { calendar }`).
#[test]
fn resource_sheet_calendar_list() {
    let mut harness = sheet_harness();
    click_cell(&mut harness, 1, ResourceColumn::Calendar, false);
    let before = harness
        .state()
        .view()
        .resources
        .rows
        .text(1, ResourceColumn::Calendar)
        .to_owned();
    press(&mut harness, Modifiers::NONE, Key::F2);
    assert!(harness.state().view().resources.edit.editing.is_some());
    assert!(harness.query_by_label(&fr(TextId::ResCalendarNone)).is_some());
    press(&mut harness, Modifiers::NONE, Key::ArrowUp);
    press(&mut harness, Modifiers::NONE, Key::ArrowUp);
    press(&mut harness, Modifiers::NONE, Key::Home);
    // Origine n'est pas une touche de la liste : elle reste ouverte.
    assert!(harness.state().view().resources.edit.editing.is_some());
    let first = harness
        .state()
        .document
        .as_ref()
        .unwrap()
        .project()
        .calendars
        .iter()
        .next()
        .unwrap()
        .name
        .clone();
    crate::click(&mut harness, &first);
    let app = harness.state();
    assert!(app.view().resources.edit.editing.is_none());
    assert_eq!(app.view().resources.rows.text(1, ResourceColumn::Calendar), first);
    assert_ne!(
        before, first,
        "le fichier donne un calendrier propre à chaque ressource"
    );
    assert_eq!(generation(app), 1);
}

/// H-UI-23 : une ressource surutilisée selon le moteur porte l'indicateur
/// « ! » et sa ligne est en rouge ; les autres non.
#[test]
fn h_ui_23_overallocated_indicator() {
    let mut harness = sheet_harness();
    let lot = batch(serde_json::json!({
        "format_version": 1,
        "ops": [{ "op": "resource.update", "uid": MASON, "max_units": { "text": "50%" } }]
    }));
    harness.state_mut().execute(&lot, EditOrigin::User).unwrap();
    harness.run();
    let app = harness.state();
    let doc = app.document.as_ref().unwrap();
    let load = |uid: u32| {
        doc.schedule()
            .resources
            .get(&ResourceUid::new(uid))
            .is_some_and(|l| l.overallocated)
    };
    assert!(load(MASON), "le moteur signale la surutilisation");
    let rows = &app.view().resources.rows;
    let sheet = ResourceSheet::new(rows, app.texts());
    let palette = Palette::LIGHT;
    for i in 0..rows.len() {
        let row = rows.row(i).unwrap();
        assert_eq!(row.overallocated, load(row.uid.get()));
        let mark = rows.text(i, ResourceColumn::Overallocated);
        let color = sheet.style(i, ResourceColumn::Name, &palette).color;
        if row.overallocated {
            assert_eq!(mark, OVERALLOCATED_MARK);
            assert_eq!(color, palette.critical);
        } else {
            assert_eq!(mark, "");
            assert_eq!(color, palette.text);
        }
    }
    // Le texte rouge est bien dessiné.
    let red = harness.output().shapes.iter().any(|clipped| {
        matches!(&clipped.shape, egui::Shape::Text(text) if text.fallback_color == palette.critical
            || text.override_text_color == Some(palette.critical))
    });
    assert!(red, "aucun texte en rouge dans l'image");
}

/// H-UI-24 : taux affichés dans l'unité de leur format d'origine, deux
/// décimales, sans conversion (120 par jour ⇒ « 120,00/j » ; « 120.00/day »
/// en anglais) ; ressource coût sans taux, matérielle sans unité de temps.
#[test]
fn h_ui_24_rate_in_original_unit() {
    let mut harness = sheet_harness();
    let lot = batch(serde_json::json!({
        "format_version": 1,
        "ops": [
            { "op": "resource.update", "uid": MASON,
              "standard_rate": { "amount": 12000, "per": "day" },
              "overtime_rate": { "amount": 2500, "per": "hour" },
              "cost_per_use": { "amount": 5000 } },
            { "op": "resource.update", "uid": 3, "standard_rate": { "amount": 9550 } }
        ]
    }));
    harness.state_mut().execute(&lot, EditOrigin::User).unwrap();
    harness.run();
    let rows = &harness.state().view().resources.rows;
    let mason = rows.index_of(ResourceUid::new(MASON)).unwrap();
    let concrete = rows.index_of(ResourceUid::new(3)).unwrap();
    assert_eq!(rows.text(mason, ResourceColumn::StandardRate), "120,00/j");
    assert_eq!(rows.text(mason, ResourceColumn::OvertimeRate), "25,00/h");
    assert_eq!(rows.text(mason, ResourceColumn::CostPerUse), "50,00");
    assert_eq!(rows.text(concrete, ResourceColumn::StandardRate), "95,50");
    assert_eq!(rows.text(concrete, ResourceColumn::OvertimeRate), "");
    harness
        .state_mut()
        .dispatch(Action::SetLanguage(LanguagePref::En));
    harness.run();
    harness.run();
    let rows = &harness.state().view().resources.rows;
    assert_eq!(rows.text(mason, ResourceColumn::StandardRate), "120.00/day");
    assert_eq!(rows.text(mason, ResourceColumn::Kind), "Work");
}

/// Capture : ressources commitées, taux, une ressource surutilisée.
fn resources_capture(language: LanguagePref, theme: ThemePref, name: &str) {
    let mut harness = snapshot_harness(language, theme);
    harness.run();
    open_sheet(&mut harness);
    let lot = batch(serde_json::json!({
        "format_version": 1,
        "ops": [
            { "op": "resource.update", "uid": MASON, "max_units": { "text": "50%" },
              "standard_rate": { "amount": 12000, "per": "day" }, "initials": "M", "group": "Gros œuvre" },
            { "op": "resource.update", "uid": 2, "standard_rate": { "amount": 4150 },
              "overtime_rate": { "amount": 6200 }, "initials": "E" },
            { "op": "resource.update", "uid": 3, "standard_rate": { "amount": 9550 }, "cost_per_use": { "amount": 1500 } }
        ]
    }));
    harness.state_mut().execute(&lot, EditOrigin::User).unwrap();
    harness.run();
    click_cell(&mut harness, 1, ResourceColumn::Name, false);
    snapshot(&mut harness, name);
}

#[test]
fn snapshot_resources_fr_light() {
    resources_capture(LanguagePref::Fr, ThemePref::Light, "resources_fr_light");
}

#[test]
fn snapshot_resources_en_dark() {
    resources_capture(LanguagePref::En, ThemePref::Dark, "resources_en_dark");
}
