//! Gantt de suivi, jours chômés, infobulle `explain_date` (J7, lot L7).
//!
//! Les formes sont relevées dans la sortie de la dernière image
//! (`Harness::output`) et reconnues à leur couleur de palette ; leurs
//! positions attendues viennent de l'axe de l'interface (`TimeAxis`, sens
//! date → pixel) et des dates du moteur, jamais d'un calcul du test.
//! Captures sur `tests/mcp/expected/tracking_j6.xml` (commité) et sur
//! l'exemple.

#![allow(
    clippy::arithmetic_side_effects,
    clippy::float_cmp,
    clippy::assert_is_empty,
    clippy::cast_precision_loss,
    reason = "tests : valeurs bornées par les fixtures ; positions exactes ; listes de formes vérifiées vides"
)]

use egui::{Color32, Rect, Shape};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_automation::Batch;
use plannercraft_automation::command::TaskRef;
use plannercraft_automation::command::params::ExplainParams;
use plannercraft_automation::json::SideJson;
use plannercraft_core::{Day, TaskUid};
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::query::{Query, QueryValue};
use plannercraft_ui_egui::theme::Palette;
use plannercraft_ui_egui::view::gantt::explain_tip_runs;
use plannercraft_ui_egui::{
    Action, App, DocOrigin, EditOrigin, GanttStyle, LanguagePref, ThemePref, TimeScaleZoom,
};

use crate::common::{harness as snapshot_harness, snapshot};
use crate::{fr, harness, screen_axis};

/// Fichier de suivi commité (lignes de base, avancement).
const TRACKING: &[u8] = include_bytes!("../../../../tests/mcp/expected/tracking_j6.xml");

/// Maçonnerie : commencée (50 %), ligne de base, `Stop` le 09/11 au soir.
const MASONRY: u32 = 3;

/// Charpente et couverture : non commencée, ligne de base.
const ROOFING: u32 = 8;

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

/// Style, zoom et défilement horizontal à l'origine, puis stabilisation.
fn show(harness: &mut Harness<'static, App>, style: GanttStyle, zoom: TimeScaleZoom) {
    harness.state_mut().dispatch(Action::SetGanttStyle(style));
    harness.state_mut().dispatch(Action::SetZoom(zoom));
    harness.run();
    harness.state_mut().dispatch(Action::ScrollGanttX(-1.0e6));
    harness.run();
    assert_eq!(harness.state().state.gantt_style, style);
    assert_eq!(harness.state().state.zoom, zoom);
}

/// Rectangles pleins de couleur `color` de la dernière image.
fn rects(harness: &Harness<'static, App>, color: Color32) -> Vec<Rect> {
    harness
        .output()
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            Shape::Rect(r) if r.fill == color => Some(r.rect),
            _ => None,
        })
        .collect()
}

/// Rectangles de `color` contenus verticalement dans `band`.
fn rects_in(harness: &Harness<'static, App>, color: Color32, band: Rect) -> Vec<Rect> {
    rects(harness, color)
        .into_iter()
        .filter(|r| r.top() >= band.top() - 0.01 && r.bottom() <= band.bottom() + 0.01)
        .collect()
}

/// Indice de ligne de la tâche `uid`.
fn index_of(app: &App, uid: u32) -> usize {
    app.rows().unwrap().index_of(TaskUid::new(uid)).unwrap()
}

/// Rectangle écran de la ligne de la tâche `uid`, relevé sur la ligne de la
/// table (même hauteur et même ordonnée que dans le Gantt).
fn row_rect(harness: &Harness<'static, App>, uid: u32) -> Rect {
    let app = harness.state();
    let rows = app.rows().unwrap();
    let i = index_of(app, uid);
    let row = rows.row(i).unwrap();
    let label = Texts::new(Language::Fr).get_args(
        TextId::RowLabel,
        &[
            ("id", Arg::Count(i64::from(row.id))),
            ("name", Arg::Str(rows.text(i, Column::Name).into())),
        ],
    );
    harness.get_by_label(&label).rect()
}

/// Bandes de la ligne `uid` : (ligne, ordonnée du début du tiers inférieur).
fn row_and_lower_third(harness: &Harness<'static, App>, uid: u32) -> (Rect, f32) {
    let row = row_rect(harness, uid);
    (row, row.top() + row.height() * 2.0 / 3.0)
}

/// Barres courantes (normales ou critiques) de la ligne `band`.
fn bars_in(harness: &Harness<'static, App>, band: Rect) -> Vec<Rect> {
    let p = Palette::LIGHT;
    let mut out = rects_in(harness, p.bar, band);
    out.extend(rects_in(harness, p.critical, band));
    out
}

#[test]
fn h_ui_22_baseline_bar_lower_third() {
    let mut harness = harness();
    open_tracking(&mut harness);
    show(&mut harness, GanttStyle::Tracking, TimeScaleZoom::Week);
    let (row, lower) = row_and_lower_third(&harness, MASONRY);
    let app = harness.state();
    let line = app.rows().unwrap().row(index_of(app, MASONRY)).unwrap();
    let (base_start, base_finish) = line.baseline.unwrap();
    let axis = screen_axis(app);
    let baselines = rects_in(&harness, Palette::LIGHT.baseline, row);
    assert_eq!(baselines.len(), 1, "{baselines:?}");
    let b = baselines[0];
    assert!(
        b.top() >= lower && b.bottom() <= row.bottom(),
        "{b:?} hors du tiers inférieur de {row:?}"
    );
    assert!((b.left() - axis.x(base_start)).abs() < 0.01, "{b:?}");
    assert!((b.right() - axis.x(base_finish)).abs() < 0.01, "{b:?}");
    // Barre courante au-dessus du tiers inférieur.
    let bars = bars_in(&harness, row);
    assert_eq!(bars.len(), 1, "{bars:?}");
    assert!(bars[0].bottom() < lower && bars[0].top() >= row.top(), "{bars:?}");
    // Diagramme de Gantt (style standard) : aucune ligne de base, barre centrée.
    show(&mut harness, GanttStyle::Standard, TimeScaleZoom::Week);
    assert!(rects(&harness, Palette::LIGHT.baseline).is_empty());
    let bars = bars_in(&harness, row);
    assert_eq!(bars.len(), 1, "{bars:?}");
    assert!((bars[0].center().y - row.center().y).abs() < 0.01, "{bars:?}");
}

#[test]
fn h_ui_22_progress_band_to_stop() {
    let mut harness = harness();
    open_tracking(&mut harness);
    show(&mut harness, GanttStyle::Tracking, TimeScaleZoom::Week);
    let (row, _) = row_and_lower_third(&harness, MASONRY);
    let app = harness.state();
    let line = app.rows().unwrap().row(index_of(app, MASONRY)).unwrap();
    let (start, _) = line.bar.unwrap();
    let stop = line.stop.unwrap();
    assert!(stop > start);
    let axis = screen_axis(app);
    let bands = rects_in(&harness, Palette::LIGHT.progress, row);
    assert_eq!(bands.len(), 1, "{bands:?}");
    let band = bands[0];
    assert!((band.left() - axis.x(start)).abs() < 0.01, "{band:?}");
    assert!((band.right() - axis.x(stop)).abs() < 0.01, "{band:?}");
    // Dans la barre courante.
    let bar = bars_in(&harness, row)[0];
    assert!(bar.contains_rect(band), "{band:?} hors de {bar:?}");
    // Tâche non commencée : pas de bande.
    let (roofing, _) = row_and_lower_third(&harness, ROOFING);
    assert!(rects_in(&harness, Palette::LIGHT.progress, roofing).is_empty());
    // Style standard : aucune bande.
    show(&mut harness, GanttStyle::Standard, TimeScaleZoom::Week);
    assert!(rects(&harness, Palette::LIGHT.progress).is_empty());
}

#[test]
fn h_ui_22_no_baseline_plain_bar() {
    // L'exemple n'a aucune ligne de base : en suivi, mêmes barres qu'en
    // standard, centrées, et aucune barre de base.
    let mut harness = harness();
    crate::open_demo(&mut harness);
    assert!(
        harness
            .state()
            .rows()
            .map(|rows| (0..rows.len()).all(|i| rows.row(i).unwrap().baseline.is_none()))
            .unwrap()
    );
    show(&mut harness, GanttStyle::Standard, TimeScaleZoom::Week);
    let p = Palette::LIGHT;
    let standard = [rects(&harness, p.bar), rects(&harness, p.critical)];
    show(&mut harness, GanttStyle::Tracking, TimeScaleZoom::Week);
    let tracking = [rects(&harness, p.bar), rects(&harness, p.critical)];
    assert!(!standard[0].is_empty() || !standard[1].is_empty());
    assert_eq!(standard, tracking);
    assert!(rects(&harness, p.baseline).is_empty());
}

/// Lundi 2 novembre 2026 + `n` jours.
fn day(n: i32) -> Day {
    "2026-11-02".parse::<Day>().unwrap().checked_add_days(n).unwrap()
}

#[test]
fn n3_non_working_days_shaded_week_zoom() {
    let mut harness = harness();
    open_tracking(&mut harness);
    show(&mut harness, GanttStyle::Standard, TimeScaleZoom::Week);
    let shaded = rects(&harness, Palette::LIGHT.non_working);
    assert!(!shaded.is_empty());
    // Samedi 7 et dimanche 8 novembre : un seul rectangle, du minuit du
    // samedi au minuit du lundi.
    let axis = screen_axis(harness.state());
    let (sat, mon) = (axis.x_day(day(5)), axis.x_day(day(7)));
    let weekend: Vec<&Rect> = shaded
        .iter()
        .filter(|r| (r.left() - sat).abs() < 0.01 && (r.right() - mon).abs() < 0.01)
        .collect();
    assert_eq!(weekend.len(), 1, "{shaded:?}");
    // Aucun jour ouvré grisé : le lundi 2 novembre ne l'est pas.
    let monday = axis.x_day(day(0)) + 1.0;
    assert!(
        shaded.iter().all(|r| !(r.left() < monday && r.right() > monday)),
        "{shaded:?}"
    );
    // Au plus un rectangle par jour visible.
    let width = harness.state().view().last_gantt_width;
    let visible_days = (width / TimeScaleZoom::Week.px_per_day()).ceil() + 1.0;
    assert!((shaded.len() as f32) <= visible_days, "{}", shaded.len());
    // Zoom Jour : grisé aussi ; Mois et Trimestre : aucun.
    show(&mut harness, GanttStyle::Standard, TimeScaleZoom::Day);
    assert!(!rects(&harness, Palette::LIGHT.non_working).is_empty());
    for zoom in [TimeScaleZoom::Month, TimeScaleZoom::Quarter] {
        show(&mut harness, GanttStyle::Standard, zoom);
        assert!(rects(&harness, Palette::LIGHT.non_working).is_empty(), "{zoom:?}");
    }
}

#[test]
fn n3_holiday_shaded() {
    // Exemple : Armistice (mercredi 11 novembre 2026), jour chômé de son
    // calendrier du projet, grisé seul au zoom Jour.
    let mut harness = harness();
    crate::open_demo(&mut harness);
    show(&mut harness, GanttStyle::Standard, TimeScaleZoom::Day);
    scroll_to_day(&mut harness, day(5));
    let axis = screen_axis(harness.state());
    let (wed, thu) = (axis.x_day(day(9)), axis.x_day(day(10)));
    let shaded = rects(&harness, Palette::LIGHT.non_working);
    assert!(
        shaded
            .iter()
            .any(|r| (r.left() - wed).abs() < 0.01 && (r.right() - thu).abs() < 0.01),
        "{shaded:?}"
    );
}

/// Défile le Gantt pour que le jour `d` soit à deux jours du bord gauche
/// (abscisse calculée par l'axe de l'interface).
fn scroll_to_day(harness: &mut Harness<'static, App>, d: Day) {
    let app = harness.state();
    let doc = app.document.as_ref().unwrap();
    let axis = plannercraft_ui_egui::timescale::TimeAxis::new(doc.range, app.state.zoom, 0.0);
    let target = (axis.x_day(d) - 2.0 * axis.px_per_day()).max(0.0);
    harness.state_mut().dispatch(Action::ScrollGanttX(-1.0e6));
    harness.state_mut().dispatch(Action::ScrollGanttX(target));
    harness.run();
}

/// Lot qui renomme la tâche `uid`.
fn rename(uid: u32, name: &str) -> Batch {
    Batch::from_json_str(&format!(
        r#"{{"format_version":1,"ops":[{{"op":"task.update","uid":{uid},"name":"{name}"}}]}}"#
    ))
    .unwrap()
}

#[test]
fn n3_compile_once_per_generation() {
    let mut harness = harness();
    open_tracking(&mut harness);
    // La première image (zoom Semaine par défaut, avant l'ajustement) a pu
    // compiler ; le zoom Mois n'en demande pas d'autre.
    show(&mut harness, GanttStyle::Standard, TimeScaleZoom::Month);
    let compiles = |h: &Harness<'static, App>| h.state().document.as_ref().unwrap().calendar_compiles();
    assert!(compiles(&harness) <= 1);
    show(&mut harness, GanttStyle::Standard, TimeScaleZoom::Week);
    assert_eq!(compiles(&harness), 1);
    // Défilement, zoom, style : même génération, aucune recompilation.
    harness.state_mut().dispatch(Action::ScrollGanttX(300.0));
    harness.run();
    show(&mut harness, GanttStyle::Tracking, TimeScaleZoom::Day);
    assert_eq!(compiles(&harness), 1);
    // Une édition : une compilation de plus, une seule.
    harness
        .state_mut()
        .execute(&rename(MASONRY, "Maçonnerie bis"), EditOrigin::User)
        .unwrap();
    harness.run();
    harness.state_mut().dispatch(Action::ScrollGanttX(-100.0));
    harness.run();
    assert_eq!(compiles(&harness), 2);
}

/// Relecture finale J7 (performances, F5) : avec une ligne de base, les
/// écarts de `RowModel::build` (reconstruit à chaque édition) prennent les
/// calendriers du cache du document : une compilation par génération, pas
/// une de plus à chaque reconstruction des lignes.
#[test]
fn n3_row_model_uses_document_calendars() {
    let mut harness = harness();
    open_tracking(&mut harness);
    let doc = harness.state().document.as_ref().unwrap();
    assert!(
        doc.project().tasks.iter().any(|t| t
            .baselines
            .contains_key(&plannercraft_core::BaselineNumber::PRIMARY)),
        "ligne de base 0 attendue dans tracking_j6.xml"
    );
    let before = doc.calendar_compiles();
    // Une édition, sans image : seule la reconstruction des lignes a pu
    // demander les calendriers de la nouvelle génération.
    harness
        .state_mut()
        .execute(&rename(MASONRY, "Maçonnerie ter"), EditOrigin::User)
        .unwrap();
    let doc = harness.state().document.as_ref().unwrap();
    assert_eq!(
        doc.calendar_compiles(),
        before + 1,
        "écarts calculés hors du cache du document"
    );
    let labels = Texts::new(Language::Fr).format_labels().clone();
    drop(plannercraft_ui_egui::rows::RowModel::build(doc, &labels));
    assert_eq!(
        doc.calendar_compiles(),
        before + 1,
        "recompilé à la reconstruction"
    );
}

/// Résumé du moteur pour le début de la tâche `uid` (gabarit de R-233).
fn engine_summary(app: &App, uid: u32) -> String {
    let query = Query::Explain(ExplainParams {
        task: TaskRef::Uid(TaskUid::new(uid)),
        side: Some(SideJson::Start),
        depth: Some(1),
    });
    let Ok(QueryValue::Explain(view)) = query.run(&app.document.as_ref().unwrap().session) else {
        panic!("explication refusée");
    };
    view.summary_fr
}

/// Survole le milieu de la barre courante de la tâche `uid`.
fn hover_bar(harness: &mut Harness<'static, App>, uid: u32) {
    let (row, _) = row_and_lower_third(harness, uid);
    let bars = bars_in(harness, row);
    assert_eq!(bars.len(), 1, "{bars:?}");
    harness.hover_at(bars[0].center());
    harness.run();
    harness.step();
    harness.run();
}

#[test]
fn h_ui_26_tooltip_from_engine_template() {
    let mut harness = harness();
    harness
        .ctx
        .global_style_mut(|style| style.interaction.tooltip_delay = 0.0);
    open_tracking(&mut harness);
    show(&mut harness, GanttStyle::Standard, TimeScaleZoom::Week);
    let expected = engine_summary(harness.state(), MASONRY);
    assert!(!expected.is_empty());
    assert!(harness.query_by_label(&expected).is_none());
    hover_bar(&mut harness, MASONRY);
    assert!(
        harness.query_by_label(&expected).is_some(),
        "infobulle absente : « {expected} »"
    );
    let runs = explain_tip_runs(&harness.ctx);
    assert_eq!(runs, 1);
    // Même tâche, même génération : lecture gardée.
    harness.step();
    harness.run();
    assert_eq!(explain_tip_runs(&harness.ctx), runs);
    // Pointeur retiré : plus d'infobulle.
    harness.remove_cursor();
    harness.run();
    assert!(harness.query_by_label(&expected).is_none());
    // Après une édition, le texte est relu.
    harness
        .state_mut()
        .execute(&rename(MASONRY, "Maçonnerie bis"), EditOrigin::User)
        .unwrap();
    harness.run();
    let renamed = engine_summary(harness.state(), MASONRY);
    hover_bar(&mut harness, MASONRY);
    assert!(harness.query_by_label(&renamed).is_some(), "« {renamed} »");
    assert_eq!(explain_tip_runs(&harness.ctx), runs + 1);
}

/// Le menu Affichage change le style du Gantt.
#[test]
fn h_ui_22_view_menu_switches_style() {
    let mut harness = harness();
    open_tracking(&mut harness);
    crate::click(&mut harness, &fr(TextId::MenuView));
    crate::click(&mut harness, &fr(TextId::MenuGanttTracking));
    assert_eq!(harness.state().state.gantt_style, GanttStyle::Tracking);
    crate::click(&mut harness, &fr(TextId::MenuView));
    crate::click(&mut harness, &fr(TextId::MenuGanttStandard));
    assert_eq!(harness.state().state.gantt_style, GanttStyle::Standard);
}

/// Fichier de suivi en capture : style de suivi, zoom Semaine.
fn tracking_capture(theme: ThemePref) -> Harness<'static, App> {
    let mut harness = snapshot_harness(LanguagePref::Fr, theme);
    harness.run();
    open_tracking(&mut harness);
    show(&mut harness, GanttStyle::Tracking, TimeScaleZoom::Week);
    assert!(
        harness.state().messages().is_empty(),
        "{:?}",
        harness.state().messages()
    );
    harness
}

#[test]
fn snapshot_tracking_fr_light() {
    let mut harness = tracking_capture(ThemePref::Light);
    snapshot(&mut harness, "tracking_fr_light");
}

#[test]
fn snapshot_tracking_fr_dark() {
    let mut harness = tracking_capture(ThemePref::Dark);
    assert_eq!(harness.ctx.theme(), egui::Theme::Dark);
    snapshot(&mut harness, "tracking_fr_dark");
}

#[test]
fn snapshot_main_fr_light_non_working() {
    // Exemple au zoom Jour, défilé sur la semaine de l'Armistice : week-ends
    // et jour férié grisés.
    let mut harness = snapshot_harness(LanguagePref::Fr, ThemePref::Light);
    harness.run();
    harness.state_mut().open_demo();
    harness.run();
    show(&mut harness, GanttStyle::Standard, TimeScaleZoom::Day);
    scroll_to_day(&mut harness, day(5));
    // Modèle par défaut (Sobre depuis J8 lot L15) : sa couleur des jours
    // chômés.
    let sobre = plannercraft_render::style::template(plannercraft_render::style::TemplateId::Sobre);
    let non_working = Palette::from_style(&sobre, plannercraft_render::style::Variant::Light).non_working;
    assert!(!rects(&harness, non_working).is_empty());
    snapshot(&mut harness, "main_fr_light_non_working");
}
