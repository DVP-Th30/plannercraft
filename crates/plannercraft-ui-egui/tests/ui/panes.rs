//! Seconde passe UX (ADR-0019 D5) : volets, séparateurs, chronologie
//! fermable (H-UI-110, H-UI-111), lot L4.
//!
//! Harnais (ruban) sur le projet « P » du cahier (UID = ID ; 1 ⊃ 2, 3, 4 ;
//! 5 ⊃ 6, 7), 1 440 × 900, frise affichée (défaut). Relevés :
//! `ViewTransient::panes` (rectangles des volets et des séparateurs) et
//! `ViewTransient::timeline` (frise, × peint).

#![allow(
    clippy::arithmetic_side_effects,
    reason = "tests : écarts et compteurs bornés par le scénario"
)]

use egui::{CursorIcon, Event, Key, Modifiers, PointerButton, Pos2, Rect, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_automation::Batch;
use plannercraft_core::Project;
use plannercraft_ui_egui::action::PaneOp;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::panes::{DETAILS_MAX_SHARE, TIMELINE_MAX_SHARE, VIEW_MIN, drag_split, layout};
use plannercraft_ui_egui::ribbon::RibbonTab;
use plannercraft_ui_egui::state::{
    DETAILS_HEIGHT_DEFAULT, DETAILS_HEIGHT_MIN, DetailsForm, PANE_HEIGHT_MAX, TIMELINE_HEIGHT_DEFAULT,
    TIMELINE_HEIGHT_MIN,
};
use plannercraft_ui_egui::view::main_view::{PanesDrawn, details_close_id};
use plannercraft_ui_egui::view::timeline::{TimelineDrawn, close_id};
use plannercraft_ui_egui::{Action, App, DocOrigin, UiState};
use proptest::prelude::*;

use plannercraft_ui_egui::view::cards::GUTTER;

use crate::filters::project_p;
use crate::fr;

/// Harnais 1 440 × 900 ouvert sur `project`, préférences `state`.
fn open_with(project: Project, state: UiState) -> Harness<'static, App> {
    let mut app = App::new(state, plannercraft_ui_egui::Env::fixed(crate::now()));
    app.open_project(
        project,
        DocOrigin {
            name: "p.xml".to_owned(),
            path: None,
            demo: false,
        },
    );
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

/// « P », préférences de test.
fn p() -> Harness<'static, App> {
    open_with(project_p(), crate::fr_light())
}

fn dispatch(harness: &mut Harness<'static, App>, action: Action) {
    harness.state_mut().dispatch(action);
    harness.run();
}

/// Relevé des volets à la dernière image.
fn panes(harness: &Harness<'static, App>) -> PanesDrawn {
    harness.state().view().panes.drawn.expect("volets dessinés")
}

/// Relevé de la frise à la dernière image.
fn frise(harness: &Harness<'static, App>) -> TimelineDrawn {
    harness
        .state()
        .view()
        .timeline
        .drawn
        .clone()
        .expect("frise dessinée")
}

/// Haut de la ligne de la tâche `n` (nom accessible de la table).
fn row_top(harness: &Harness<'static, App>, n: u32, name: &str) -> f32 {
    let label = Texts::new(Language::Fr).get_args(
        TextId::RowLabel,
        &[
            ("id", Arg::Count(i64::from(n))),
            ("name", Arg::Str(name.to_owned().into())),
        ],
    );
    harness.get_by_label(&label).rect().top()
}

/// « P » avec la tâche 3 sur la frise : frise à la hauteur choisie et
/// séparateur tirable (une frise sans tâche est basse et fixe, H-UI-137).
fn framed(harness: &mut Harness<'static, App>) {
    let json = serde_json::json!({
        "format_version": 1,
        "ops": [{ "op": "style.apply", "patch": { "timeline": { "tasks": [3] } } }]
    });
    dispatch(
        harness,
        Action::Exec(Batch::from_json_str(&json.to_string()).unwrap()),
    );
}

/// H-UI-137 (finitions P4, seconde passe) : frise sans tâche ⇒ 36 points,
/// sans séparateur, la place rendue à la vue ; une tâche ajoutée ⇒ hauteur
/// choisie (72 points) et séparateur ; la hauteur enregistrée n'a pas
/// bougé.
#[test]
fn h_ui_137_empty_timeline_compact() {
    let mut harness = p();
    let before = panes(&harness);
    assert!(
        (before.timeline.unwrap().height() - 36.0).abs() < 0.01,
        "{before:?}"
    );
    assert!(before.timeline_splitter.is_none());
    assert_eq!(harness.state().state.timeline_height, TIMELINE_HEIGHT_DEFAULT);
    framed(&mut harness);
    let after = panes(&harness);
    assert!(
        (after.timeline.unwrap().height() - 72.0).abs() < 0.01,
        "{after:?}"
    );
    assert!(after.timeline_splitter.is_some());
    assert!((before.view.height() - after.view.height() - 36.0).abs() < 0.5);
}

fn history_len(app: &App) -> usize {
    app.document.as_ref().unwrap().session.history().entries.len()
}

fn generation(app: &App) -> u64 {
    app.document.as_ref().unwrap().generation()
}

/// Clic droit (appui et relâcher) en `p`.
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

/// Glisse de `from` à `to` (quatre pas) et lâche.
pub(crate) fn drag(harness: &mut Harness<'static, App>, from: Pos2, to: Pos2) {
    harness.hover_at(from);
    harness.step();
    harness.drag_at(from);
    harness.step();
    for k in 1..=4_u8 {
        let t = f32::from(k) / 4.0;
        harness.hover_at(from + (to - from) * t);
        harness.step();
    }
    harness.drop_at(to);
    harness.run();
}

/// Double-clic en `p`.
pub(crate) fn double_click(harness: &mut Harness<'static, App>, p: Pos2) {
    harness.hover_at(p);
    harness.step();
    for _ in 0..2 {
        for pressed in [true, false] {
            harness.event(Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
            harness.step();
        }
    }
    harness.run();
}

/// Point vide de la frise (sans élément ni ×) : bord gauche, mi-hauteur.
fn blank_point(d: &TimelineDrawn) -> Pos2 {
    let p = pos2(d.rect.left() + 6.0, d.rect.center().y);
    assert!(d.elements.iter().all(|(_, r)| !r.contains(p)), "{d:?}");
    p
}

/// Centre de la zone de saisie d'un séparateur.
fn grip(r: Option<Rect>) -> Pos2 {
    r.expect("séparateur affiché").center()
}

/// H-UI-110 (TL-01) : clic droit dans le vide de la frise ⇒ menu
/// « Masquer la chronologie » ⇒ frise masquée, table remontée de sa
/// hauteur, aucune entrée d'annulation.
#[test]
fn h_ui_110_right_click_blank_hides() {
    let mut harness = p();
    let shown = row_top(&harness, 1, "Gros œuvre");
    let height = frise(&harness).rect.height();
    let p = blank_point(&frise(&harness));
    right_click(&mut harness, p);
    crate::click(&mut harness, &fr(TextId::TimelineHide));
    let app = harness.state();
    assert!(!app.state.timeline_visible);
    assert!(app.view().timeline.drawn.is_none());
    assert_eq!(history_len(app), 0);
    let hidden = row_top(&harness, 1, "Gros œuvre");
    // Lot U2 (H-UI-172) : la carte de la frise et sa gouttière.
    assert!(
        (shown - hidden - height - GUTTER).abs() < 0.5,
        "{shown} → {hidden}"
    );
    assert!(
        harness.query_by_label(&fr(TextId::TimelineHide)).is_none(),
        "menu refermé"
    );
}

/// H-UI-110 : × invisible sans survol (captures inchangées), peint au
/// survol de la frise, au coin haut droit ; clic ⇒ frise masquée.
#[test]
fn h_ui_110_close_button_on_hover() {
    let mut harness = p();
    let d = frise(&harness);
    assert!(d.close.is_none(), "× peint sans survol : {d:?}");
    harness.hover_at(blank_point(&d));
    harness.run();
    let close = frise(&harness).close.expect("× peint au survol");
    assert!((close.width() - 16.0).abs() < 0.01 && (close.height() - 16.0).abs() < 0.01);
    assert!(d.rect.contains_rect(close));
    assert!(
        close.right() > d.rect.right() - 24.0 && close.top() < d.rect.top() + 8.0,
        "{close:?}"
    );
    // Survol hors de la frise : × retiré.
    harness.hover_at(pos2(d.rect.center().x, d.rect.bottom() + 200.0));
    harness.run();
    assert!(frise(&harness).close.is_none());
    // Clic sur le × (nom accessible : info-bulle).
    harness.hover_at(close.center());
    harness.run();
    crate::click(&mut harness, &fr(TextId::TimelineCloseTip));
    assert!(!harness.state().state.timeline_visible);
    assert_eq!(history_len(harness.state()), 0);
}

/// H-UI-110 : clic droit dans le vide, × et case Affichage → Chronologie
/// mènent au même état (frise masquée, table à la même place, document
/// intact, aucune entrée d'annulation).
#[test]
fn h_ui_110_three_paths_same_action() {
    let mut results = Vec::new();
    for path in 0..3 {
        let mut harness = p();
        match path {
            0 => {
                let p = blank_point(&frise(&harness));
                right_click(&mut harness, p);
                crate::click(&mut harness, &fr(TextId::TimelineHide));
            }
            1 => {
                let p = blank_point(&frise(&harness));
                harness.hover_at(p);
                harness.run();
                crate::click(&mut harness, &fr(TextId::TimelineCloseTip));
            }
            _ => crate::ribbon_click(&mut harness, RibbonTab::View, &fr(TextId::RibbonTimeline)),
        }
        let app = harness.state();
        assert!(!app.state.timeline_visible, "chemin {path}");
        assert_eq!((history_len(app), generation(app)), (0, 0), "chemin {path}");
        results.push(row_top(&harness, 1, "Gros œuvre").round());
    }
    assert!(
        results.windows(2).all(|w| (w[0] - w[1]).abs() < 0.5),
        "{results:?}"
    );
}

/// H-UI-111 (TL-02) : glisser le séparateur sous la frise de 80 points ⇒
/// frise de 152 points (72 ⇒ 152), curseur `ResizeVertical`, hauteur gardée
/// dans `UiState` et relue après relance.
#[test]
fn h_ui_111_timeline_splitter_drag() {
    let mut harness = p();
    framed(&mut harness);
    let base = history_len(harness.state());
    assert_eq!(harness.state().state.timeline_height, TIMELINE_HEIGHT_DEFAULT);
    let before = panes(&harness);
    assert!(
        (before.timeline.unwrap().height() - 72.0).abs() < 0.01,
        "{before:?}"
    );
    let g = grip(before.timeline_splitter);
    // Lot U2 (H-UI-172) : séparateur au milieu de la gouttière.
    assert!((g.y - before.timeline.unwrap().bottom() - GUTTER / 2.0).abs() < 0.01);
    harness.hover_at(g);
    harness.run();
    assert_eq!(
        harness.output().platform_output.cursor_icon,
        CursorIcon::ResizeVertical
    );
    let shown = row_top(&harness, 1, "Gros œuvre");
    drag(&mut harness, g, g + vec2(0.0, 80.0));
    let app = harness.state();
    assert_eq!(app.state.timeline_height, 152);
    assert!(app.view().split_drag.is_none());
    assert_eq!(history_len(app), base);
    let after = panes(&harness);
    assert!(
        (after.timeline.unwrap().height() - 152.0).abs() < 0.01,
        "{after:?}"
    );
    assert!((frise(&harness).rect.height() - 152.0).abs() < 0.01);
    assert!((row_top(&harness, 1, "Gros œuvre") - shown - 80.0).abs() < 0.5);
    // Relance : préférences enregistrées puis relues.
    let json = harness.state().state.to_json().unwrap();
    let (read, warning) = UiState::from_json_lenient(&json);
    assert!(warning.is_none(), "{warning:?}");
    assert_eq!(read.timeline_height, 152);
    let mut again = open_with(project_p(), read);
    framed(&mut again);
    assert!((panes(&again).timeline.unwrap().height() - 152.0).abs() < 0.01);
}

/// H-UI-111 : glisser hors de la fenêtre ⇒ hauteurs bornées (48 points ;
/// 40 % de la zone ; détails 120 points à 60 %) ; valeurs extrêmes de
/// `UiState` bornées à l'affichage ; non fini ⇒ inchangé.
#[test]
fn h_ui_111_splitter_bounds() {
    let mut harness = p();
    framed(&mut harness);
    let total = {
        let d = panes(&harness);
        d.timeline.unwrap().height() + d.view.height()
    };
    let span = zone(&panes(&harness));
    let g = grip(panes(&harness).timeline_splitter);
    drag(&mut harness, g, pos2(g.x, -500.0));
    assert_eq!(harness.state().state.timeline_height, TIMELINE_HEIGHT_MIN);
    let g = grip(panes(&harness).timeline_splitter);
    drag(&mut harness, g, pos2(g.x, 5_000.0));
    let max = (TIMELINE_MAX_SHARE * total).round();
    let h = harness.state().state.timeline_height;
    assert!((f32::from(h) - max).abs() <= 1.0, "{h} vs {max}");
    assert!(panes(&harness).view.height() >= VIEW_MIN);
    // État extrême : affichage borné, vue gardée.
    dispatch(&mut harness, Action::Pane(PaneOp::SetTimelineHeight(u16::MAX)));
    dispatch(&mut harness, Action::Pane(PaneOp::ShowDetails(true)));
    dispatch(&mut harness, Action::Pane(PaneOp::SetDetailsHeight(u16::MAX)));
    assert_eq!(harness.state().state.timeline_height, PANE_HEIGHT_MAX);
    let d = panes(&harness);
    assert!(d.view.height() >= VIEW_MIN - 0.01, "{d:?}");
    assert!((zone(&d) - span).abs() < 0.5, "{d:?}");
    // Détails tirés vers le bas au-delà de la fenêtre : 120 points.
    let g = grip(d.details_splitter);
    drag(&mut harness, g, pos2(g.x, 5_000.0));
    assert_eq!(harness.state().state.details.height, DETAILS_HEIGHT_MIN);
    // Non fini : rien ne bouge (fonction pure du glisser).
    assert!((drag_split(72.0, f32::NAN, 48.0, 400.0) - 72.0).abs() < f32::EPSILON);
    assert!((drag_split(72.0, f32::INFINITY, 48.0, 400.0) - 72.0).abs() < f32::EPSILON);
    let h = layout(f32::NAN, Some(72.0), Some(220.0));
    assert!(h.timeline.is_finite() && h.view.is_finite() && h.details.is_finite());
}

/// H-UI-111 : double-clic sur un séparateur ⇒ hauteur par défaut (72 et
/// 360 points, bornée à la fenêtre).
#[test]
fn h_ui_111_double_click_resets() {
    let mut harness = p();
    framed(&mut harness);
    dispatch(&mut harness, Action::Pane(PaneOp::SetTimelineHeight(200)));
    dispatch(&mut harness, Action::Pane(PaneOp::ShowDetails(true)));
    dispatch(&mut harness, Action::Pane(PaneOp::SetDetailsHeight(300)));
    assert!((panes(&harness).timeline.unwrap().height() - 200.0).abs() < 0.01);
    let g = grip(panes(&harness).timeline_splitter);
    double_click(&mut harness, g);
    assert_eq!(harness.state().state.timeline_height, TIMELINE_HEIGHT_DEFAULT);
    // Une seconde plus tard : un nouveau double-clic, pas un triple-clic.
    for _ in 0..60 {
        harness.step();
    }
    assert!((panes(&harness).timeline.unwrap().height() - 72.0).abs() < 0.01);
    let g = grip(panes(&harness).details_splitter);
    double_click(&mut harness, g);
    assert_eq!(harness.state().state.details.height, DETAILS_HEIGHT_DEFAULT);
    assert!((panes(&harness).details.unwrap().height() - details_default(&harness)).abs() < 0.01);
    // Seule l'entrée de la frise (`framed`) : les séparateurs n'en ajoutent
    // aucune.
    assert_eq!(history_len(harness.state()), 1);
}

/// H-UI-111, H-UI-112 (cadre) : volet de détails fermé par défaut ; ouvert
/// ⇒ hauteur par défaut (360 points, bornée à la fenêtre) en bas, en-tête
/// « Formulaire tâche » et × ; séparateur tiré de 60 points vers le bas ⇒
/// 60 de moins ; × ⇒ fermé, vue rendue.
#[test]
fn h_ui_111_details_splitter() {
    let mut harness = p();
    assert!(!harness.state().state.details.visible);
    let closed = panes(&harness);
    assert!(closed.details.is_none() && closed.details_splitter.is_none());
    dispatch(&mut harness, Action::Pane(PaneOp::ShowDetails(true)));
    let d = panes(&harness);
    let details = d.details.expect("volet affiché");
    let start = details_default(&harness);
    assert!((details.height() - start).abs() < 0.01, "{d:?}");
    assert!((details.top() - d.view.bottom() - GUTTER).abs() < 0.01);
    assert!((closed.view.bottom() - details.bottom()).abs() < 0.01);
    assert!(harness.query_by_label(&fr(TextId::DetailsCloseTip)).is_some());
    let painted: Vec<String> = harness
        .output()
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::epaint::Shape::Text(t) if details.contains(t.pos) => Some(t.galley.text().to_owned()),
            _ => None,
        })
        .collect();
    assert!(painted.contains(&fr(TextId::DetailsTitle)), "{painted:?}");
    // Glisser vers le haut : le volet grandit.
    let g = grip(d.details_splitter);
    harness.hover_at(g);
    harness.run();
    assert_eq!(
        harness.output().platform_output.cursor_icon,
        CursorIcon::ResizeVertical
    );
    drag(&mut harness, g, g + vec2(0.0, 60.0));
    let want = start - 60.0;
    assert_eq!(f32::from(harness.state().state.details.height), want);
    assert!((panes(&harness).details.unwrap().height() - want).abs() < 0.01);
    // Forme et hauteur gardées après relance.
    dispatch(
        &mut harness,
        Action::Pane(PaneOp::SetDetailsForm(DetailsForm::ResourcesSuccessors)),
    );
    let (read, _) = UiState::from_json_lenient(&harness.state().state.to_json().unwrap());
    assert_eq!(read.details, harness.state().state.details);
    // × : volet fermé, vue rendue.
    crate::click(&mut harness, &fr(TextId::DetailsCloseTip));
    assert!(!harness.state().state.details.visible);
    let reopened = panes(&harness);
    assert!(reopened.details.is_none());
    assert!((reopened.view.bottom() - closed.view.bottom()).abs() < 0.01);
    assert_eq!(history_len(harness.state()), 0);
}

/// Appuie sur F6 (Maj+F6 si `back`) dans une image.
fn f6(harness: &mut Harness<'static, App>, back: bool) {
    let modifiers = if back { Modifiers::SHIFT } else { Modifiers::NONE };
    harness.key_press_modifiers(modifiers, Key::F6);
    harness.run();
}

fn focused(harness: &Harness<'static, App>, id: egui::Id) -> bool {
    harness.ctx.memory(|m| m.has_focus(id))
}

/// H-UI-111 : F6 passe le clavier au volet suivant dans l'ordre de l'écran
/// (table ⇒ détails ⇒ chronologie ⇒ table), Maj+F6 à rebours ; × de la
/// frise peint au focus clavier ; volet masqué sauté.
#[test]
fn h_ui_111_f6_cycles_panes() {
    let mut harness = p();
    dispatch(&mut harness, Action::Pane(PaneOp::ShowDetails(true)));
    let table = plannercraft_ui_egui::view::table::table_id();
    f6(&mut harness, false);
    assert!(focused(&harness, details_close_id()));
    f6(&mut harness, false);
    assert!(focused(&harness, close_id()));
    assert!(frise(&harness).close.is_some(), "× peint au focus clavier");
    f6(&mut harness, false);
    assert!(focused(&harness, table));
    // Maj+F6 : à rebours.
    f6(&mut harness, true);
    assert!(focused(&harness, close_id()));
    f6(&mut harness, true);
    assert!(focused(&harness, details_close_id()));
    f6(&mut harness, true);
    assert!(focused(&harness, table));
    // Détails fermés : table ⇔ chronologie.
    dispatch(&mut harness, Action::Pane(PaneOp::ShowDetails(false)));
    f6(&mut harness, false);
    assert!(focused(&harness, close_id()));
    f6(&mut harness, false);
    assert!(focused(&harness, table));
    assert!(
        harness.state().messages().is_empty(),
        "{:?}",
        harness.state().messages()
    );
    // Entrée sur le × focalisé : la frise se masque.
    f6(&mut harness, false);
    harness.key_press(Key::Enter);
    harness.run();
    assert!(!harness.state().state.timeline_visible);
}

/// H-UI-81 (non-régression) : clic droit sur un élément de la frise ⇒
/// « Retirer de la chronologie » (et « Masquer la chronologie ») ;
/// Retirer ne retire que l'élément, la frise reste affichée.
#[test]
fn h_ui_81_remove_item_still_works() {
    let mut harness = p();
    let json = serde_json::json!({
        "format_version": 1,
        "ops": [{ "op": "style.apply", "patch": { "timeline": { "tasks": [3, 4] } } }]
    });
    dispatch(
        &mut harness,
        Action::Exec(Batch::from_json_str(&json.to_string()).unwrap()),
    );
    let murs = frise(&harness)
        .elements
        .iter()
        .find(|(uid, _)| uid.get() == 3)
        .map(|(_, r)| r.center())
        .expect("Murs sur la frise");
    right_click(&mut harness, murs);
    assert!(harness.query_by_label(&fr(TextId::TimelineHide)).is_some());
    crate::click(&mut harness, &fr(TextId::TimelineRemove));
    let app = harness.state();
    let doc = app.document.as_ref().unwrap();
    assert_eq!(app.state.effective_style(doc.project()).timeline.tasks, [4]);
    assert!(app.state.timeline_visible);
    assert!(
        harness.query_by_label(&fr(TextId::TimelineHide)).is_none(),
        "menu refermé"
    );
}

/// Opération de volet tirée au hasard.
fn any_pane_op() -> impl Strategy<Value = PaneOp> {
    prop_oneof![
        any::<u16>().prop_map(PaneOp::SetTimelineHeight),
        any::<u16>().prop_map(PaneOp::SetDetailsHeight),
        any::<bool>().prop_map(PaneOp::ShowDetails),
        prop_oneof![
            Just(DetailsForm::ResourcesPredecessors),
            Just(DetailsForm::ResourcesSuccessors),
            Just(DetailsForm::PredecessorsSuccessors),
        ]
        .prop_map(PaneOp::SetDetailsForm),
    ]
}

/// Hauteur ou écart tiré au hasard, non finis compris.
fn any_len() -> impl Strategy<Value = f32> {
    prop_oneof![
        -5_000.0_f32..5_000.0,
        Just(f32::NAN),
        Just(f32::INFINITY),
        Just(f32::NEG_INFINITY),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    /// P63 (séparateurs, fonctions pures) : toute suite de glissers sur
    /// toute zone ⇒ hauteurs finies, dans leurs bornes, somme = zone, vue
    /// ≥ 120 points dès que la zone le permet.
    #[test]
    fn p63_split_sequences(
        total in prop_oneof![0.0_f32..3_000.0, Just(f32::NAN), Just(-1.0_f32)],
        drags in proptest::collection::vec((any::<bool>(), any_len()), 0..64),
        shown in (any::<bool>(), any::<bool>()),
    ) {
        let caps = |min: u16, share: f32| {
            let min = f32::from(min);
            if total.is_finite() { (share * total).max(min) } else { min }
        };
        let mut t = f32::from(TIMELINE_HEIGHT_DEFAULT).min(caps(TIMELINE_HEIGHT_MIN, TIMELINE_MAX_SHARE));
        let mut d = f32::from(DETAILS_HEIGHT_DEFAULT).min(caps(DETAILS_HEIGHT_MIN, DETAILS_MAX_SHARE));
        for (timeline, delta) in drags {
            let (min, share) = if timeline {
                (f32::from(TIMELINE_HEIGHT_MIN), TIMELINE_MAX_SHARE)
            } else {
                (f32::from(DETAILS_HEIGHT_MIN), DETAILS_MAX_SHARE)
            };
            let cap = if total.is_finite() { (share * total).max(min) } else { min };
            let slot = if timeline { &mut t } else { &mut d };
            *slot = drag_split(*slot, delta, min, cap);
            prop_assert!(slot.is_finite() && *slot >= min && *slot <= cap, "{slot} {min} {cap}");
            let h = layout(total, shown.0.then_some(t), shown.1.then_some(d));
            prop_assert!(h.timeline.is_finite() && h.view.is_finite() && h.details.is_finite());
            prop_assert!(h.timeline >= 0.0 && h.view >= 0.0 && h.details >= 0.0);
            if total.is_finite() && total > 0.0 {
                prop_assert!((h.timeline + h.view + h.details - total).abs() < 1e-2, "{h:?}");
                prop_assert!(h.view >= VIEW_MIN.min(total) - 1e-3, "{h:?}");
                if h.timeline > 0.0 {
                    prop_assert!(h.timeline >= f32::from(TIMELINE_HEIGHT_MIN) - 1e-3);
                    prop_assert!(h.timeline <= (TIMELINE_MAX_SHARE * total).max(f32::from(TIMELINE_HEIGHT_MIN)) + 1e-3);
                }
                if h.details > 0.0 {
                    prop_assert!(h.details >= f32::from(DETAILS_HEIGHT_MIN) - 1e-3);
                    prop_assert!(h.details <= (DETAILS_MAX_SHARE * total).max(f32::from(DETAILS_HEIGHT_MIN)) + 1e-3);
                }
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 12, ..ProptestConfig::default() })]

    /// P63 (séparateurs, état de l'application) : toute suite (≤ 24)
    /// d'opérations de volets ⇒ hauteurs de `UiState` dans leurs bornes,
    /// volets dessinés finis et couvrant la zone, vue ≥ 120 points ;
    /// sérialisé puis relu ⇒ égal.
    #[test]
    fn p63_split_sequences_app(ops in proptest::collection::vec(any_pane_op(), 0..24)) {
        let mut harness = p();
        let total = zone(&panes(&harness));
        for op in ops {
            harness.state_mut().dispatch(Action::Pane(op));
            harness.run();
            let state = &harness.state().state;
            prop_assert!((TIMELINE_HEIGHT_MIN..=PANE_HEIGHT_MAX).contains(&state.timeline_height));
            prop_assert!((DETAILS_HEIGHT_MIN..=PANE_HEIGHT_MAX).contains(&state.details.height));
            let d = panes(&harness);
            let rects = [d.timeline, Some(d.view), d.details];
            prop_assert!(rects.iter().flatten().all(Rect::is_finite));
            prop_assert!((zone(&d) - total).abs() < 0.5, "{d:?}");
            prop_assert!(d.view.height() >= VIEW_MIN - 0.01, "{d:?}");
            prop_assert_eq!(d.details.is_some(), state.details.visible);
        }
        let state = &harness.state().state;
        let (read, warning) = UiState::from_json_lenient(&state.to_json().unwrap());
        prop_assert!(warning.is_none(), "{warning:?}");
        prop_assert_eq!(&read, state);
    }
}

/// Hauteur de la zone des volets : volets affichés et gouttières entre eux
/// (lot U2, H-UI-172).
fn zone(d: &PanesDrawn) -> f32 {
    let rects: Vec<Rect> = [d.timeline, Some(d.view), d.details]
        .into_iter()
        .flatten()
        .collect();
    let gutters = f32::from(u8::try_from(rects.len().saturating_sub(1)).unwrap());
    rects.iter().map(Rect::height).sum::<f32>() + GUTTER * gutters
}

/// Hauteur affichée du volet de détails à sa hauteur par défaut dans la
/// zone du harnais (bornée par `panes::layout`).
fn details_default(harness: &Harness<'static, App>) -> f32 {
    let d = panes(harness);
    let total =
        d.view.height() + d.details.map_or(0.0, |r| r.height()) + d.timeline.map_or(0.0, |r| r.height());
    let timeline = d.timeline.map(|r| r.height());
    plannercraft_ui_egui::panes::layout(total, timeline, Some(f32::from(DETAILS_HEIGHT_DEFAULT))).details
}
