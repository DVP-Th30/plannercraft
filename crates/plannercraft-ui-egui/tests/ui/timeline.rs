//! Chronologie à l'écran et à l'impression (ADR-0017 D7, D8, H-UI-78 à
//! H-UI-82) : module posé par le contrat (lot L0), rempli par le lot L4.
//!
//! Harnais (ruban, seul habillage depuis le lot L5) sur le projet « P » du
//! cahier (UID = ID ; 1 ⊃ 2, 3, 4 ; 5 ⊃ 6,
//! 7), 1 440 × 900. Les tâches de la frise sont posées par un lot
//! `style.apply { patch: { timeline } }` (même écriture que le geste
//! « Ajouter à la chronologie »). Relevé de la frise :
//! `ViewTransient::timeline` (disposition en cache, rectangles peints,
//! zones cliquables, partie visible du Gantt).

#![allow(
    clippy::arithmetic_side_effects,
    reason = "tests : compteurs et écarts bornés par le scénario"
)]

use egui::accesskit::Role;
use egui::epaint::Shape;
use egui::{Rect, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use plannercraft_automation::Batch;
use plannercraft_core::{Project, TaskUid};
use plannercraft_render::timeline::{TimelineLayout, TimelineShape};
use plannercraft_ui_egui::action::{DialogKind, TimelineOp};
use plannercraft_ui_egui::dialog::DialogBody;
use plannercraft_ui_egui::dialog::page_setup::PageSetupState;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::view::timeline::{COMPACT_H, TIMELINE_H, TimelineDrawn};
use plannercraft_ui_egui::{Action, App, DocOrigin, ViewKind};

use crate::filters::project_p;
use crate::{fr, harness, open_demo};

/// H-UI-78 (préférence, lot L0) : visible par défaut, `SetVisible` la
/// change ; `Activate` sélectionne la tâche.
#[test]
fn timeline_l0_visibility_and_activate() {
    let mut harness = harness();
    assert!(harness.state().state.timeline_visible);
    harness
        .state_mut()
        .dispatch(Action::Timeline(TimelineOp::SetVisible(false)));
    harness.run();
    assert!(!harness.state().state.timeline_visible);
    open_demo(&mut harness);
    let uid = harness
        .state()
        .document
        .as_ref()
        .unwrap()
        .project()
        .tasks
        .iter()
        .nth(2)
        .map(plannercraft_core::HasUid::uid)
        .unwrap();
    harness
        .state_mut()
        .dispatch(Action::Timeline(TimelineOp::Activate(uid)));
    harness.run();
    assert_eq!(harness.state().selection, Some(uid));
}

// --- Lot L4 : frise à l'écran et à l'impression ---

const fn uid(n: u32) -> TaskUid {
    TaskUid::new(n)
}

/// Harnais `width` × 900, ouvert sur `project`.
fn open_at(project: Project, width: f32) -> Harness<'static, App> {
    let mut app = App::new(crate::fr_light(), plannercraft_ui_egui::Env::fixed(crate::now()));
    app.open_project(
        project,
        DocOrigin {
            name: "p.xml".to_owned(),
            path: None,
            demo: false,
        },
    );
    let mut harness = Harness::builder()
        .with_size(vec2(width, 900.0))
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .with_os(egui::os::OperatingSystem::Nix)
        .with_step_dt(1.0 / 60.0)
        .with_max_steps(8)
        .build_ui_state(plannercraft_ui_egui::ui, app);
    harness.run();
    harness
}

/// « P » en ruban, 1 440 px.
fn p() -> Harness<'static, App> {
    open_at(project_p(), 1440.0)
}

/// « P » en ruban avec les tâches `tasks` sur la frise.
fn p_with(tasks: &[u32]) -> Harness<'static, App> {
    let mut harness = p();
    put(&mut harness, tasks);
    harness
}

/// Pose la liste de la frise par un lot `style.apply` (une annulation).
fn put(harness: &mut Harness<'static, App>, tasks: &[u32]) {
    let json = serde_json::json!({
        "format_version": 1,
        "ops": [{ "op": "style.apply", "patch": { "timeline": { "tasks": tasks } } }]
    });
    let batch = Batch::from_json_str(&json.to_string()).unwrap();
    harness.state_mut().dispatch(Action::Exec(batch));
    harness.run();
    assert_eq!(
        timeline_tasks(harness.state()),
        tasks,
        "{:?}",
        harness.state().messages()
    );
}

/// Liste de la frise du style effectif du document.
fn timeline_tasks(app: &App) -> Vec<u32> {
    let doc = app.document.as_ref().unwrap();
    app.state.effective_style(doc.project()).timeline.tasks
}

/// Relevé de la frise à la dernière image.
fn drawn(harness: &Harness<'static, App>) -> TimelineDrawn {
    harness
        .state()
        .view()
        .timeline
        .drawn
        .clone()
        .expect("frise dessinée")
}

/// Disposition en cache.
fn layout(harness: &Harness<'static, App>) -> TimelineLayout {
    harness.state().view().timeline.cache.clone().unwrap().1
}

/// Textes peints dans le rectangle de la frise (coin haut-gauche de la
/// galley : jusqu'à un point au-dessus de la bande haute).
fn frise_texts(harness: &Harness<'static, App>) -> Vec<String> {
    let rect = drawn(harness).rect.expand(2.0);
    harness
        .output()
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            Shape::Text(t) if rect.contains(t.pos) => Some(t.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

fn dispatch(harness: &mut Harness<'static, App>, action: Action) {
    harness.state_mut().dispatch(action);
    harness.run();
}

/// Rectangle de la ligne de la tâche `n` (nom accessible de la table).
fn row_rect(harness: &Harness<'static, App>, n: u32, name: &str) -> Rect {
    let label = Texts::new(Language::Fr).get_args(
        TextId::RowLabel,
        &[
            ("id", Arg::Count(i64::from(n))),
            ("name", Arg::Str(name.to_owned().into())),
        ],
    );
    harness.get_by_label(&label).rect()
}

/// H-UI-78 : P ⇒ « Début lun 19/10/26 » à gauche, « Fin mar 17/11/26 » à
/// droite, graduation « nov. » ; frise sur toute la largeur, sous le
/// ruban : basse sans tâche (H-UI-137, 36 points), 72 points dès qu'une
/// tâche y est.
#[test]
fn h_ui_78_timeline_shows_project_bounds() {
    let mut harness = p();
    let d = drawn(&harness);
    assert!((d.rect.height() - COMPACT_H).abs() < 0.01, "{d:?}");
    put(&mut harness, &[3]);
    assert!((drawn(&harness).rect.height() - TIMELINE_H).abs() < 0.01);
    put(&mut harness, &[]);
    let d = drawn(&harness);
    assert!((d.rect.height() - COMPACT_H).abs() < 0.01, "{d:?}");
    // Toute la largeur de la vue (marges du panneau central comprises).
    // Lot U2 : frise sur toute sa carte, table dans l'intérieur de la
    // sienne (bordure d'un point).
    let table_left = row_rect(&harness, 1, "Gros œuvre").left();
    assert!((d.rect.left() + 1.0 - table_left).abs() < 0.5, "{d:?}");
    assert!(d.rect.width() > 1400.0, "{d:?}");
    let texts = frise_texts(&harness);
    assert!(texts.iter().any(|t| t == "Début lun 19/10/26"), "{texts:?}");
    assert!(texts.iter().any(|t| t == "Fin mar 17/11/26"), "{texts:?}");
    assert!(texts.iter().any(|t| t == "nov."), "{texts:?}");
    // « Début » à gauche de « Fin ».
    let x_of = |s: &str| {
        harness
            .output()
            .shapes
            .iter()
            .find_map(|c| match &c.shape {
                Shape::Text(t) if t.galley.text() == s => Some(t.pos.x),
                _ => None,
            })
            .unwrap()
    };
    assert!(x_of("Début lun 19/10/26") < d.rect.center().x);
    assert!(x_of("Fin mar 17/11/26") > d.rect.center().x);
}

/// H-UI-78 : frise vide ⇒ texte atténué au centre qui dit comment y
/// ajouter des tâches ; retiré dès qu'une tâche y est.
#[test]
fn h_ui_78_empty_timeline_hint() {
    let hint = fr(TextId::TimelineEmptyHint);
    let mut harness = p();
    assert!(
        frise_texts(&harness).contains(&hint),
        "{:?}",
        frise_texts(&harness)
    );
    put(&mut harness, &[3]);
    assert!(!frise_texts(&harness).contains(&hint));
}

/// H-UI-78 : case décochée ⇒ frise retirée, table remontée de sa hauteur
/// (36 points sans tâche, H-UI-137) ; cochée à nouveau ⇒ redescendue.
#[test]
fn h_ui_78_toggle_hides() {
    let mut harness = p();
    let shown = row_rect(&harness, 1, "Gros œuvre").top();
    dispatch(&mut harness, Action::Timeline(TimelineOp::SetVisible(false)));
    assert!(harness.state().view().timeline.drawn.is_none());
    let hidden = row_rect(&harness, 1, "Gros œuvre").top();
    // Lot U2 (H-UI-172) : la carte de la frise et sa gouttière.
    let gutter = plannercraft_ui_egui::view::cards::GUTTER;
    assert!(
        (shown - hidden - COMPACT_H - gutter).abs() < 0.5,
        "{shown} → {hidden}"
    );
    dispatch(&mut harness, Action::Timeline(TimelineOp::SetVisible(true)));
    assert!((row_rect(&harness, 1, "Gros œuvre").top() - shown).abs() < 0.5);
}

/// H-UI-78 : absente des vues des ressources, présente dans le Gantt de
/// suivi ; absente avec la barre d'outils de J8 (ADR-0017 D9).
#[test]
fn h_ui_78_absent_from_resource_views() {
    let mut harness = p();
    let bound = "Début lun 19/10/26";
    let painted = |h: &Harness<'static, App>| {
        h.output()
            .shapes
            .iter()
            .any(|c| matches!(&c.shape, Shape::Text(t) if t.galley.text() == bound))
    };
    assert!(painted(&harness));
    for view in [ViewKind::ResourceSheet, ViewKind::ResourceUsage] {
        dispatch(&mut harness, Action::SetView(view));
        assert!(!painted(&harness), "{view:?}");
    }
    dispatch(&mut harness, Action::SetView(ViewKind::Gantt));
    dispatch(
        &mut harness,
        Action::SetGanttStyle(plannercraft_ui_egui::state::GanttStyle::Tracking),
    );
    assert!(harness.state().view().timeline.drawn.is_some(), "Gantt de suivi");
    assert!(painted(&harness));
}

/// H-UI-78 (lot L5) : frise affichée par défaut (préférence cochée, aucun
/// `set_chrome`) en tête de la vue Gantt dès l'ouverture, avec les bornes
/// du projet ; la table commence sous elle.
#[test]
fn h_ui_78_default_visible() {
    let harness = p();
    assert!(harness.state().state.timeline_visible);
    let drawn = drawn(&harness);
    assert!(drawn.rect.height() >= COMPACT_H - 0.5, "{:?}", drawn.rect);
    let texts = frise_texts(&harness);
    assert!(texts.iter().any(|t| t.starts_with("Début")), "{texts:?}");
    assert!(texts.iter().any(|t| t.starts_with("Fin")), "{texts:?}");
    let row = row_rect(&harness, 2, "Fondations");
    assert!(row.top() >= drawn.rect.bottom(), "{row:?} sous {:?}", drawn.rect);
}

/// H-UI-78 : date d'état en trait tireté étiqueté « État 06/11 ».
#[test]
fn h_ui_78_status_date_line() {
    let mut project = project_p();
    project.status_date = Some("2026-11-06T17:00".parse().unwrap());
    let harness = open_at(project, 1440.0);
    let texts = frise_texts(&harness);
    assert!(texts.iter().any(|t| t == "État 06/11"), "{texts:?}");
    let status_x = layout(&harness)
        .shapes
        .iter()
        .find_map(|s| match s {
            TimelineShape::StatusLine { x16 } => Some(*x16),
            _ => None,
        })
        .unwrap();
    let d = drawn(&harness);
    let x = d.rect.left() + plannercraft_ui_egui::geom::i64_to_f32(status_x) / 16.0;
    // Tirets : plusieurs segments verticaux à l'abscisse de la date d'état.
    let dashes = harness
        .output()
        .shapes
        .iter()
        .filter(|c| match &c.shape {
            Shape::LineSegment { points, .. } => {
                (points[0].x - x).abs() < 0.01 && (points[1].x - x).abs() < 0.01 && d.rect.contains(points[0])
            }
            _ => false,
        })
        .count();
    assert!(dashes >= 3, "{dashes} segment(s)");
}

/// H-UI-79 : clic droit sur 3 ⇒ « Ajouter à la chronologie » (3 absente) ;
/// l'entrée pousse la bascule de la sélection. 3 sur la frise ⇒ libellé
/// « Retirer de la chronologie » et barre « Murs » sur la frise.
#[test]
fn h_ui_79_context_menu_entry() {
    let mut harness = p();
    harness.get_by_label(&row_label(3, "Murs")).click_secondary();
    harness.run();
    assert_eq!(harness.state().selection, Some(uid(3)));
    assert_eq!(menu_entries(&harness, TextId::TimelineRemove), 0);
    assert_eq!(menu_entries(&harness, TextId::TimelineAdd), 1);
    harness
        .query_all_by_label(&fr(TextId::TimelineAdd))
        .find(|n| n.accesskit_node().role() == Role::Button)
        .unwrap()
        .click();
    harness.run();
    assert_eq!(timeline_tasks(harness.state()), [3]);
    assert_eq!(menu_entries(&harness, TextId::TimelineAdd), 0, "menu refermé");
    assert!(drawn(&harness).elements.iter().any(|(u, _)| *u == uid(3)));
    assert!(frise_texts(&harness).iter().any(|t| t == "Murs"));
    harness.get_by_label(&row_label(3, "Murs")).click_secondary();
    harness.run();
    assert_eq!(menu_entries(&harness, TextId::TimelineAdd), 0);
    assert_eq!(menu_entries(&harness, TextId::TimelineRemove), 1);
}

/// Entrées de menu (boutons) de libellé `id` ; la case du ruban, de même
/// libellé, n'est pas comptée.
fn menu_entries(harness: &Harness<'static, App>, id: TextId) -> usize {
    harness
        .query_all_by_label(&fr(id))
        .filter(|n| n.accesskit_node().role() == Role::Button)
        .count()
}

/// Nom accessible de la ligne de la tâche `n`.
fn row_label(n: u32, name: &str) -> String {
    Texts::new(Language::Fr).get_args(
        TextId::RowLabel,
        &[
            ("id", Arg::Count(i64::from(n))),
            ("name", Arg::Str(name.to_owned().into())),
        ],
    )
}

/// H-UI-80 (écran) : rectangles peints = `TimelineLayout` à 1/16 px près ;
/// P avec {2, 3, 4, 6} : 2, 3, 6 en barres, 4 en losange.
#[test]
fn h_ui_80_screen_matches_layout() {
    let harness = p_with(&[2, 3, 4, 6]);
    let d = drawn(&harness);
    let out = layout(&harness);
    let o = d.rect.min;
    let expected: Vec<Rect> = out
        .shapes
        .iter()
        .filter_map(|s| match s {
            TimelineShape::Band { rect16 } | TimelineShape::Bar { rect16, .. } => Some(*rect16),
            _ => None,
        })
        .map(|[x, y, w, h]| {
            let f = |v: i64| plannercraft_ui_egui::geom::i64_to_f32(v) / 16.0;
            Rect::from_min_size(o + vec2(f(x), f(y)), vec2(f(w), f(h)))
        })
        .collect();
    assert_eq!(d.painted.len(), expected.len(), "{:?}", d.painted);
    for (got, want) in d.painted.iter().zip(&expected) {
        for (a, b) in [
            (got.min.x, want.min.x),
            (got.min.y, want.min.y),
            (got.max.x, want.max.x),
            (got.max.y, want.max.y),
        ] {
            assert!((a - b).abs() <= 1.0 / 16.0, "{got:?} ≠ {want:?}");
        }
    }
    let bars: Vec<u32> = out
        .shapes
        .iter()
        .filter_map(|s| match s {
            TimelineShape::Bar { uid, .. } => Some(uid.get()),
            _ => None,
        })
        .collect();
    assert_eq!(bars, [2, 3, 6]);
    assert!(
        out.shapes
            .iter()
            .any(|s| matches!(s, TimelineShape::Diamond { uid, .. } if uid.get() == 4))
    );
    let mut ids: Vec<u32> = d.elements.iter().map(|(u, _)| u.get()).collect();
    ids.sort_unstable();
    assert_eq!(ids, [2, 3, 4, 6]);
}

/// H-UI-81 : clic sur la barre « Murs » ⇒ 3 sélectionnée (et défilement,
/// servi par `App::scroll_to_task`, lot L3) ; clic ailleurs : rien.
#[test]
fn h_ui_81_click_selects_and_scrolls() {
    let mut harness = p_with(&[2, 3, 4]);
    dispatch(&mut harness, Action::Select(Some(uid(1))));
    // Clic dans la frise hors de tout élément (bande des graduations).
    let d = drawn(&harness);
    harness
        .get_by_label("Murs")
        .rect()
        .contains(d.rect.center())
        .then(|| panic!("point d'essai sur une barre"));
    harness.input_mut().events.extend([
        egui::Event::PointerMoved(d.rect.right_bottom() - vec2(4.0, 4.0)),
        egui::Event::PointerButton {
            pos: d.rect.right_bottom() - vec2(4.0, 4.0),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos: d.rect.right_bottom() - vec2(4.0, 4.0),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    ]);
    harness.run();
    assert_eq!(harness.state().selection, Some(uid(1)), "clic ailleurs : rien");
    let murs = harness.get_by_label("Murs");
    assert!(d.rect.contains(murs.rect().center()), "élément de la frise");
    murs.click();
    harness.run();
    assert_eq!(harness.state().selection, Some(uid(3)));
    // Ligne de 3 visible dans la table.
    let row = row_rect(&harness, 3, "Murs");
    assert!(row.top() >= d.rect.bottom() && row.bottom() <= 900.0, "{row:?}");
}

/// H-UI-81 : rectangle de la partie visible suivi image par image :
/// défilement du Gantt de 120 points ⇒ bord gauche décalé de 120 × rapport
/// des échelles (longueur de l'axe de la frise / longueur du projet dans
/// le Gantt, deux longueurs en pixels).
#[test]
fn h_ui_81_visible_window_tracks_scroll() {
    let mut harness = p_with(&[3]);
    dispatch(
        &mut harness,
        Action::SetZoom(plannercraft_ui_egui::TimeScaleZoom::Day),
    );
    dispatch(&mut harness, Action::ScrollGanttX(300.0));
    let before = drawn(&harness).window.expect("partie visible");
    let (a0, a1) = layout(&harness)
        .shapes
        .iter()
        .find_map(|s| match s {
            TimelineShape::Axis { x16, .. } => Some(*x16),
            _ => None,
        })
        .unwrap();
    let app = harness.state();
    let schedule = app.document.as_ref().unwrap().schedule();
    let axis = crate::screen_axis(app);
    let gantt_span = axis.x(schedule.project_finish) - axis.x(schedule.project_start);
    let ratio = plannercraft_ui_egui::geom::i64_to_f32(a1 - a0) / 16.0 / gantt_span;
    assert!(ratio > 0.0, "{ratio}");
    let rect = drawn(&harness).rect;
    assert!(
        before.left() > rect.left() + 1.0,
        "bord gauche non borné : {before:?}"
    );
    dispatch(&mut harness, Action::ScrollGanttX(120.0));
    let after = drawn(&harness).window.unwrap();
    assert!(
        (after.left() - before.left() - 120.0 * ratio).abs() < 0.05,
        "{before:?} → {after:?} (rapport {ratio})"
    );
    // Rectangle peint (contour au curseur) dans la frise.
    assert!(rect.contains_rect(after));
}

/// H-UI-81 : clic droit sur « Murs » dans la frise ⇒ « Retirer de la
/// chronologie » ⇒ `Timeline(Remove(3))`.
#[test]
fn h_ui_81_right_click_removes() {
    let mut harness = p_with(&[3, 4]);
    harness.get_by_label("Murs").click_secondary();
    harness.run();
    crate::click(&mut harness, &fr(TextId::TimelineRemove));
    assert_eq!(timeline_tasks(harness.state()), [4]);
    assert!(
        harness.query_by_label(&fr(TextId::TimelineRemove)).is_none(),
        "menu refermé"
    );
}

/// État de la fenêtre « Mise en page » ouverte.
fn page_state(app: &App) -> PageSetupState {
    match app.view().dialog.as_ref().map(|d| &d.body) {
        Some(DialogBody::PageSetup(state)) => (**state).clone(),
        other => panic!("mise en page non ouverte : {other:?}"),
    }
}

/// H-UI-82 : Mode impression avec la frise affichée ⇒ case cochée, aperçu
/// page 1 avec frise (sous l'en-tête), page 2 sans ; PDF de l'aperçu.
#[test]
fn h_ui_82_print_mode_includes_timeline() {
    let mut harness = p_with(&[2, 3, 4, 6]);
    dispatch(
        &mut harness,
        Action::ShowDialog(DialogKind::PageSetup { print_mode: true }),
    );
    let state = page_state(harness.state());
    assert!(state.page.timeline);
    let pages = state.preview_pages().expect("aperçu calculé");
    let tl = pages.timeline.as_ref().expect("frise imprimée");
    assert!(pages.pages.first().unwrap().timeline);
    assert!(pages.pages.iter().skip(1).all(|p| !p.timeline));
    assert!(pages.timeline16.1 > 0);
    assert!(tl.items.len() > 2);
    // Aperçu peint : textes de la frise sur la page 1.
    let texts: Vec<String> = harness
        .output()
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            Shape::Text(t) => Some(t.galley.text().to_owned()),
            _ => None,
        })
        .collect();
    assert!(texts.iter().any(|t| t.starts_with("Début")), "{texts:?}");
    // PDF de l'aperçu (même chemin que « Exporter en PDF… »).
    let session = &harness.state().document.as_ref().unwrap().session;
    let pdf = plannercraft_ui_egui::dialog::page_setup::pdf_bytes(
        session,
        pages,
        plannercraft_automation::text::Language::Fr,
    )
    .unwrap();
    assert!(pdf.starts_with(b"%PDF"));
}

/// H-UI-82 : frise masquée à l'écran ⇒ Mode impression la laisse
/// décochée, aucune frise imprimée ; la cocher dans la fenêtre ⇒ aperçu
/// avec frise.
#[test]
fn h_ui_82_unchecked_no_timeline() {
    let mut harness = p_with(&[3]);
    dispatch(&mut harness, Action::Timeline(TimelineOp::SetVisible(false)));
    dispatch(
        &mut harness,
        Action::ShowDialog(DialogKind::PageSetup { print_mode: true }),
    );
    let state = page_state(harness.state());
    assert!(!state.page.timeline);
    let pages = state.preview_pages().unwrap();
    assert!(pages.timeline.is_none() && pages.pages.iter().all(|p| !p.timeline));
    crate::click(&mut harness, &fr(TextId::RibbonTimeline));
    let state = page_state(harness.state());
    assert!(state.page.timeline);
    assert!(state.preview_pages().unwrap().timeline.is_some());
}

/// H-UI-82 : « Enregistrer avec le projet » ⇒ `page.timeline` dans le
/// style du document, une entrée d'annulation.
#[test]
fn h_ui_82_saved_with_project() {
    let mut harness = p_with(&[3]);
    let undo = |app: &App| app.document.as_ref().unwrap().session.history().entries.len();
    let before = undo(harness.state());
    dispatch(
        &mut harness,
        Action::ShowDialog(DialogKind::PageSetup { print_mode: false }),
    );
    assert!(!page_state(harness.state()).page.timeline);
    crate::click(&mut harness, &fr(TextId::RibbonTimeline));
    crate::click(&mut harness, &fr(TextId::PsSave));
    let app = harness.state();
    assert!(app.view().dialog.is_none());
    assert!(
        app.state
            .effective_style(app.document.as_ref().unwrap().project())
            .page
            .timeline
    );
    assert_eq!(undo(app), before + 1);
    dispatch(&mut harness, Action::Undo);
    let app = harness.state();
    assert!(
        !app.state
            .effective_style(app.document.as_ref().unwrap().project())
            .page
            .timeline
    );
}

/// Cache : aucune nouvelle disposition sur 10 images immobiles ; une
/// largeur nouvelle ou une édition ⇒ une seule.
#[test]
fn timeline_cache_reused() {
    let mut harness = p_with(&[2, 3]);
    let computed = |h: &Harness<'static, App>| h.state().view().timeline.layouts_computed;
    let start = computed(&harness);
    for _ in 0..10 {
        harness.step();
    }
    assert_eq!(computed(&harness), start);
    let wide = drawn(&harness).rect.width();
    harness.set_size(vec2(1200.0, 900.0));
    harness.run();
    assert_eq!(computed(&harness), start + 1);
    assert!((wide - drawn(&harness).rect.width() - 240.0).abs() < 0.01);
    put(&mut harness, &[2, 3, 6]);
    assert_eq!(computed(&harness), start + 2);
}
