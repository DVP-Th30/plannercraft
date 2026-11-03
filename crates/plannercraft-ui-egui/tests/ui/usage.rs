//! Utilisation des ressources (J7, lot L11) : valeurs du moteur
//! (R-188, R-245, R-246), ressources dépliées en affectations, colonnes de
//! la graduation fine (H-UI-25), cache des blocs, surutilisation en rouge
//! (H-UI-23), navigation au clavier, captures `usage_fr_light` et
//! `usage_fr_dark`. Le refus `usage.limit` est testé dans la crate
//! (`view::usage::tests::usage_limit_message`).
//!
//! Fichier : `tests/mcp/expected/resources_j5.xml` (commité) : Maçon (UID 1,
//! 200 %, six affectations), Électricien, Béton (matériel, m³), Location.

#![allow(
    clippy::arithmetic_side_effects,
    clippy::float_cmp,
    reason = "tests : valeurs bornées par les fixtures"
)]

use std::collections::BTreeMap;

use chrono::Datelike as _;

use egui::{Event, Key, Modifiers, PointerButton, Pos2, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_automation::Batch;
use plannercraft_automation::command::params::UsageParams;
use plannercraft_automation::json::GranularityJson;
use plannercraft_core::resource::usage::Granularity;
use plannercraft_core::{Moment, ResourceUid, Work};
use plannercraft_ui_egui::grid::{self, Sheet as _};
use plannercraft_ui_egui::i18n::{Arg, TextId};
use plannercraft_ui_egui::theme::Palette;
use plannercraft_ui_egui::timescale::{TimeAxis, tier_cells};
use plannercraft_ui_egui::view::table::table_id;
use plannercraft_ui_egui::view::usage::{self, COLUMNS, UsageColumn, UsageKey, UsageSheet};
use plannercraft_ui_egui::{
    Action, App, DocOrigin, EditOrigin, LanguagePref, ThemePref, TimeScaleZoom, ViewKind,
};

use crate::common::{harness as snapshot_harness, snapshot};
use crate::harness;

/// Fichier de ressources commité.
const RESOURCES: &[u8] = include_bytes!("../../../../tests/mcp/expected/resources_j5.xml");

/// Maçon : six affectations.
const MASON: u32 = 1;

/// Lot lu depuis son JSON.
fn batch(json: serde_json::Value) -> Batch {
    serde_json::from_value(json).unwrap()
}

/// Ouvre le fichier de ressources, zoom Semaine, vue Utilisation.
fn open_usage(harness: &mut Harness<'static, App>) {
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
        .dispatch(Action::SetView(ViewKind::ResourceUsage));
    harness.state_mut().dispatch(Action::SetZoom(TimeScaleZoom::Week));
    harness.run();
    assert_eq!(harness.state().state.view, ViewKind::ResourceUsage);
}

/// Harnais sans rendu, vue Utilisation ouverte.
fn usage_harness() -> Harness<'static, App> {
    let mut harness = harness();
    harness.run();
    open_usage(&mut harness);
    harness
}

/// Maçon à 50 % : surutilisé (R-186).
fn overallocate_mason(harness: &mut Harness<'static, App>) {
    let lot = batch(serde_json::json!({
        "format_version": 1,
        "ops": [{ "op": "resource.update", "uid": MASON, "max_units": { "text": "50%" } }]
    }));
    harness.state_mut().execute(&lot, EditOrigin::User).unwrap();
    harness.run();
}

/// Libellé français d'un message avec le nom `name`.
fn named(app: &App, id: TextId, name: &str) -> String {
    app.texts()
        .get_args(id, &[("name", Arg::Str(name.to_owned().into()))])
}

/// Clique au point `p`.
fn click_at(harness: &mut Harness<'static, App>, p: Pos2) {
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
}

/// Déplie (ou replie) Maçon par un clic sur son « + » (« - »).
fn toggle_mason(harness: &mut Harness<'static, App>, open: bool) {
    let id = if open {
        TextId::UsageExpand
    } else {
        TextId::UsageCollapse
    };
    let label = named(harness.state(), id, "Maçon");
    let center = harness.get_by_label(&label).rect().center();
    click_at(harness, center);
}

/// Cellules dessinées de la ligne de clé `key`, par début de période.
fn cells_of(app: &App, key: UsageKey) -> BTreeMap<Moment, (Moment, Work, bool)> {
    app.view()
        .usage
        .last_cells
        .iter()
        .filter(|c| c.key == key)
        .map(|c| (c.start, (c.finish, c.work, c.over)))
        .collect()
}

/// Ligne d'une ressource.
fn resource(uid: u32) -> UsageKey {
    UsageKey::Resource(ResourceUid::new(uid))
}

/// Chaque cellule d'une ressource est la valeur de `analysis.resource_usage`
/// (la même lecture que le MCP) sur la même période ; le travail total du
/// volet gauche est celui du moteur.
#[test]
fn usage_values_from_engine() {
    let harness = usage_harness();
    let app = harness.state();
    let doc = app.document.as_ref().unwrap();
    let cells = &app.view().usage.last_cells;
    assert_ne!(cells.len(), 0);
    assert!(
        cells.iter().any(|c| c.work != Work::ZERO),
        "aucun travail visible"
    );
    let mut checked = 0;
    for cell in cells {
        let UsageKey::Resource(uid) = cell.key else {
            continue;
        };
        let params = UsageParams {
            from: Some(cell.start),
            to: Some(cell.finish),
            by: GranularityJson::Week,
            resources: vec![uid.get()],
            assignments: false,
        };
        let view = plannercraft_automation::view::resource_usage(&doc.session, &params).unwrap();
        let row = &view.rows[0];
        assert_eq!(row.work.len(), 1, "une période par cellule");
        assert_eq!(row.work[0].minutes, Some(cell.work.minutes().get()), "{cell:?}");
        assert_eq!(row.overallocated[0], cell.over, "{cell:?}");
        checked += 1;
    }
    assert!(checked > 10, "{checked} cellules vérifiées");
    // Travail total : `ResourceLoad::work` écrit en heures.
    let rows = &app.view().usage.rows;
    let mason = rows.index_of(resource(MASON)).unwrap();
    let load = doc.schedule().resources[&ResourceUid::new(MASON)].work;
    let expected = usage::work_text(load, None, &doc.project().options.factors, rows.labels());
    assert_eq!(rows.text(mason, UsageColumn::Work), expected);
    assert!(expected.ends_with("\u{a0}h"), "{expected}");
    assert_eq!(generation(app), 0, "lecture seule");
}

/// Relecture finale J7 (performances, F2) : une fenêtre de lignes qui
/// commence au milieu des affectations d'une ressource dépliée (sans la
/// ligne de la ressource) la compte dépliée : ses cellules d'affectation
/// sont demandées au moteur au lieu de rester vides.
#[test]
fn usage_window_inside_assignments_counts_expanded() {
    let mut harness = usage_harness();
    toggle_mason(&mut harness, true);
    let rows = &harness.state().view().usage.rows;
    let mason = rows.index_of(resource(MASON)).unwrap();
    let (resources, expanded) = rows.resources_in(mason + 1..mason + 4);
    assert_eq!(resources, [ResourceUid::new(MASON)]);
    assert_eq!(
        expanded,
        [ResourceUid::new(MASON)],
        "affectations vues sans leur ressource"
    );
    let (_, whole) = rows.resources_in(0..rows.len());
    assert!(whole.contains(&ResourceUid::new(MASON)));
}

/// Génération du document.
fn generation(app: &App) -> u64 {
    app.document.as_ref().unwrap().generation()
}

/// « + » déplie Maçon en ses six affectations (tâche, travail du moteur) ;
/// période par période, Σ des affectations = ligne de la ressource
/// (R-245) ; « - » replie.
#[test]
fn usage_expand_assignments() {
    let mut harness = usage_harness();
    let before = harness.state().view().usage.rows.len();
    toggle_mason(&mut harness, true);
    let app = harness.state();
    assert!(app.view().expanded.contains(&ResourceUid::new(MASON)));
    let rows = &app.view().usage.rows;
    assert_eq!(rows.len(), before + 6);
    let mason = rows.index_of(resource(MASON)).unwrap();
    assert_eq!(rows.text(mason, UsageColumn::Expand), usage::COLLAPSE_MARK);
    let doc = app.document.as_ref().unwrap();
    let assignments: Vec<UsageKey> = (mason + 1..mason + 7)
        .map(|i| rows.line(i).unwrap().key)
        .collect();
    for (k, key) in assignments.iter().enumerate() {
        let UsageKey::Assignment(uid) = *key else {
            panic!("ligne {k} : {key:?}")
        };
        let a = &doc.schedule().assignments[&uid];
        assert_eq!(a.resource, Some(ResourceUid::new(MASON)));
        let i = mason + 1 + k;
        let task = &doc.project().tasks.get(a.task).unwrap().name;
        assert_eq!(rows.text(i, UsageColumn::Name), task);
        let label = app.texts().get_args(
            TextId::UsageAssignmentLabel,
            &[
                ("name", Arg::Str(task.clone().into())),
                ("detail", Arg::Str("Maçon".into())),
            ],
        );
        assert!(harness.query_by_label(&label).is_some(), "{label}");
    }
    // Σ des affectations = la ressource, période par période (R-245, P46).
    let total = cells_of(app, resource(MASON));
    assert!(total.values().any(|(_, w, _)| *w != Work::ZERO));
    for (start, (_, work, _)) in &total {
        let sum: i64 = assignments
            .iter()
            .map(|key| cells_of(app, *key)[start].1.minutes().get())
            .sum();
        assert_eq!(sum, work.minutes().get(), "période {start}");
    }
    assert!(
        app.view().usage.cache.key().unwrap().expanded == vec![ResourceUid::new(MASON)],
        "affectations demandées pour la seule ressource dépliée"
    );
    toggle_mason(&mut harness, false);
    let app = harness.state();
    assert_eq!(app.view().expanded.len(), 0);
    assert_eq!(app.view().usage.rows.len(), before);
    assert_eq!(generation(app), 0);
}

/// Zoom Mois (en-tête trimestres / mois) : colonnes mensuelles ; zoom
/// Trimestre (années / trimestres) : colonnes trimestrielles (R-246).
#[test]
fn usage_quarter_zoom_monthly_columns() {
    let mut harness = usage_harness();
    harness
        .state_mut()
        .dispatch(Action::SetZoom(TimeScaleZoom::Month));
    harness.run();
    let app = harness.state();
    assert_eq!(app.view().usage.cache.key().unwrap().by, Granularity::Month);
    let mason = cells_of(app, resource(MASON));
    for (start, (finish, _, _)) in &mason {
        let (s, f) = (start.day().to_naive(), finish.day().to_naive());
        assert_eq!(s.day(), 1, "{start}");
        assert_eq!(f.day(), 1, "{finish}");
        let months = |d: chrono::NaiveDate| i64::from(d.year()) * 12 + i64::from(d.month0());
        assert_eq!(months(f) - months(s), 1, "{start}");
    }
    // Novembre et décembre 2026 portent le travail du projet.
    let nov: Moment = "2026-11-01T00:00".parse().unwrap();
    assert!(mason[&nov].1 != Work::ZERO);
    harness
        .state_mut()
        .dispatch(Action::SetZoom(TimeScaleZoom::Quarter));
    harness.run();
    let app = harness.state();
    assert_eq!(app.view().usage.cache.key().unwrap().by, Granularity::Quarter);
    let q4: Moment = "2026-10-01T00:00".parse().unwrap();
    let q1: Moment = "2027-01-01T00:00".parse().unwrap();
    let mason = cells_of(app, resource(MASON));
    assert_eq!(mason[&q4].0, q1, "trimestre civil");
    let load = app.document.as_ref().unwrap().schedule().resources[&ResourceUid::new(MASON)].work;
    assert_eq!(mason[&q4].1, load, "tout le travail tient dans T4 2026");
}

/// Images sans changement et petit défilement horizontal : aucune requête
/// au moteur (blocs en cache) ; une édition (nouvelle génération) en refait.
#[test]
fn usage_cache_hit_no_query() {
    let mut harness = usage_harness();
    let queries = harness.state().view().usage.cache.queries();
    assert!(queries > 0);
    for _ in 0..5 {
        harness.step();
    }
    harness.state_mut().dispatch(Action::ScrollGanttX(10.0));
    harness.run();
    assert!(harness.state().view().gantt_scroll_x > 0.0);
    assert_eq!(harness.state().view().usage.cache.queries(), queries);
    overallocate_mason(&mut harness);
    assert!(harness.state().view().usage.cache.queries() > queries);
}

/// H-UI-23 : Maçon surutilisé : nom et travail en rouge, cellules des
/// périodes surutilisées en rouge, les autres non.
#[test]
fn h_ui_23_over_cells_red() {
    let mut harness = usage_harness();
    overallocate_mason(&mut harness);
    let app = harness.state();
    let palette = Palette::LIGHT;
    let rows = &app.view().usage.rows;
    let sheet = UsageSheet::new(rows, app.texts());
    let mason = rows.index_of(resource(MASON)).unwrap();
    assert!(rows.line(mason).unwrap().overallocated);
    for column in [UsageColumn::Name, UsageColumn::Work] {
        assert_eq!(sheet.style(mason, column, &palette).color, palette.critical);
    }
    let electrician = rows.index_of(resource(2)).unwrap();
    assert_eq!(
        sheet.style(electrician, UsageColumn::Name, &palette).color,
        palette.text
    );
    let cells = cells_of(app, resource(MASON));
    assert!(
        cells.values().any(|(_, _, over)| *over),
        "aucune période surutilisée"
    );
    for (_, work, over) in cells.values() {
        let color = usage::cell_color(*over, &palette);
        if *over {
            assert_eq!(color, palette.critical);
            assert!(*work != Work::ZERO);
        } else {
            assert_eq!(color, palette.text);
        }
    }
}

/// H-UI-25 : à chaque zoom, une colonne par graduation fine, aux mêmes
/// abscisses que l'en-tête de l'échelle de temps (même axe, même
/// défilement).
#[test]
fn h_ui_25_columns_follow_tier() {
    let mut harness = usage_harness();
    for zoom in TimeScaleZoom::ALL {
        harness.state_mut().dispatch(Action::SetZoom(zoom));
        harness.state_mut().dispatch(Action::ScrollGanttX(30.0));
        harness.run();
        let app = harness.state();
        let doc = app.document.as_ref().unwrap();
        let view = app.view();
        let axis = TimeAxis::new(
            doc.range,
            app.state.zoom,
            view.last_gantt_left - view.gantt_scroll_x,
        );
        let left = view.last_gantt_left;
        let clip = (left, left + view.last_gantt_width);
        let tiers = tier_cells(
            &axis,
            zoom.tiers().1,
            doc.project().options.week_start_day,
            clip,
            app.texts().format_labels(),
        );
        let columns = &view.usage.last_columns;
        assert_ne!(columns.len(), 0, "{zoom:?}");
        let mut matched = 0;
        for &(x0, x1) in columns.iter().filter(|(x0, _)| *x0 >= axis.left()) {
            assert!(
                tiers
                    .iter()
                    .any(|t| (t.x0 - x0).abs() < 0.5 && (t.x1 - x1).abs() < 0.5),
                "{zoom:?} : colonne [{x0}, {x1}] hors de l'en-tête"
            );
            matched += 1;
        }
        assert!(matched > 0, "{zoom:?}");
    }
}

/// Navigation de L1 dans le volet gauche (sans ligne vide) ; + et -
/// déplient et replient la ressource courante.
#[test]
fn usage_navigation_keys() {
    let mut harness = usage_harness();
    let app = harness.state();
    let rows = &app.view().usage.rows;
    let mason = rows.index_of(resource(MASON)).unwrap();
    // Clic sur le nom de Maçon : curseur et focus.
    let label = app.texts().get_args(
        TextId::ResRowLabel,
        &[("id", Arg::Count(1)), ("name", Arg::Str("Maçon".into()))],
    );
    let rect = harness.get_by_label(&label).rect();
    let (x, w) = grid::column_span(&COLUMNS, rect.left(), UsageColumn::Name).unwrap();
    click_at(&mut harness, pos2(x + w / 2.0, rect.center().y));
    assert!(harness.ctx.memory(|m| m.has_focus(table_id())));
    assert_eq!(harness.state().view().usage.cursor_index(), Some(mason));
    let press = |harness: &mut Harness<'static, App>, m: Modifiers, k: Key| {
        harness.key_press_modifiers(m, k);
        harness.run();
    };
    press(&mut harness, Modifiers::NONE, Key::ArrowDown);
    assert_eq!(harness.state().view().usage.cursor_index(), Some(mason + 1));
    press(&mut harness, Modifiers::NONE, Key::ArrowUp);
    press(&mut harness, Modifiers::NONE, Key::Plus);
    assert!(harness.state().view().expanded.contains(&ResourceUid::new(MASON)));
    press(&mut harness, Modifiers::NONE, Key::Tab);
    assert_eq!(harness.state().view().usage.cursor.column, UsageColumn::Work);
    press(&mut harness, Modifiers::COMMAND, Key::End);
    let len = harness.state().view().usage.rows.len();
    assert_eq!(harness.state().view().usage.cursor_index(), Some(len - 1));
    press(&mut harness, Modifiers::NONE, Key::ArrowDown);
    assert_eq!(
        harness.state().view().usage.cursor_index(),
        Some(len - 1),
        "au bord, sans ligne vide"
    );
    press(&mut harness, Modifiers::COMMAND, Key::Home);
    assert_eq!(harness.state().view().usage.cursor_index(), Some(0));
    // Retour sur Maçon, puis - replie.
    for _ in 0..mason {
        press(&mut harness, Modifiers::NONE, Key::ArrowDown);
    }
    press(&mut harness, Modifiers::NONE, Key::Minus);
    assert_eq!(harness.state().view().expanded.len(), 0);
    assert_eq!(generation(harness.state()), 0);
}

/// Capture : ressources commitées, Maçon surutilisé et déplié.
fn usage_capture(language: LanguagePref, theme: ThemePref, name: &str) {
    let mut harness = snapshot_harness(language, theme);
    harness.run();
    open_usage(&mut harness);
    overallocate_mason(&mut harness);
    toggle_mason(&mut harness, true);
    // Début de la plage : tout le travail du projet est visible.
    harness.state_mut().dispatch(Action::ScrollGanttX(-1.0e6));
    harness.run();
    snapshot(&mut harness, name);
}

#[test]
fn snapshot_usage_fr_light() {
    usage_capture(LanguagePref::Fr, ThemePref::Light, "usage_fr_light");
}

#[test]
fn snapshot_usage_fr_dark() {
    usage_capture(LanguagePref::Fr, ThemePref::Dark, "usage_fr_dark");
}
