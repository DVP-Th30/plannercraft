//! Captures de référence (`tests/snapshots/*.png`), comparées seulement si
//! `PLANNERCRAFT_UI_SNAPSHOTS=1` (voir `common`). Chaque test vérifie aussi l'état
//! obtenu, dans tous les modes.
//!
//! J7 lot L16 (critère de fin « 5 000 tâches fluide à l'écran, vérifié par
//! captures ») : `net5000_*`, le réseau généré de 5 000 tâches et 10 000
//! liens (le même que `tests/perf.rs` et `snapshot --generate 5000`),
//! début, milieu, fin × Semaine, Trimestre × clair, sombre, 1280×720,
//! « maintenant » fixe (début du réseau) ; la ligne visée est sélectionnée
//! et le Gantt défilé au début de sa barre, comme `--row` de l'exemple.
//!
//! ADR-0017 lot L5 : ruban (un onglet par capture, anglais sombre, replié,
//! surimpression, 800 px, deux pixels par point) et chronologie (« P » avec 2, 3, 4, 6
//! et 7 sur la frise, à l'écran et en mode impression) ; toutes les autres
//! captures montrent désormais le ruban à la place des deux rangées
//! d'outils, et une frise (vide hors « P ») en tête des vues Gantt.
//!
//! Passe UX (ADR-0018, lot L5) : bloc de cellules en clair et en sombre,
//! lignes entières, trait d'insertion d'un déplacement de lignes, liste
//! « Remplir ▾ » du groupe Édition ; toutes les autres références
//! régénérées une fois (teinte de sélection plus marquée, ID appuyés,
//! lignes sélectionnées du Gantt en `header_selected`, groupe Édition).
//!
//! Mise à jour des références (Linux, lavapipe) :
//! `PLANNERCRAFT_UI_SNAPSHOTS=1 UPDATE_SNAPSHOTS=1 WGPU_BACKEND=vulkan cargo test -p plannercraft-ui-egui --test ui`.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_core::Moment;
use plannercraft_tests::generate::ui_network;
use plannercraft_ui_egui::action::{PressMode, PressTarget, SelectOp};
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::i18n::{TextId, Texts};
use plannercraft_ui_egui::nav::RowRef;
use plannercraft_ui_egui::ribbon::{RibbonOp, RibbonTab};
use plannercraft_ui_egui::timescale::TimeAxis;
use plannercraft_ui_egui::view::ribbon::drawn;
use plannercraft_ui_egui::view::table::selection_drawn;
use plannercraft_ui_egui::{
    Action, App, DocOrigin, Env, Language, LanguagePref, ThemePref, TimeScaleZoom, UiState,
};

use crate::common::{harness, harness_sized, harness_with, snapshot};

/// Ouvre l'exemple et laisse le premier dessin ajuster le zoom.
fn demo(language: LanguagePref, theme: ThemePref) -> Harness<'static, App> {
    let mut harness = harness(language, theme);
    harness.run();
    harness.state_mut().open_demo();
    harness.run();
    let app = harness.state();
    assert!(
        app.document.is_some(),
        "exemple non ouvert : {:?}",
        app.messages()
    );
    assert!(!app.view().needs_fit, "zoom ajusté pas encore appliqué");
    assert!(app.messages().is_empty(), "{:?}", app.messages());
    assert_eq!(app.last_frame.table_rows, app.last_frame.gantt_rows);
    assert!(app.last_frame.table_rows.end > app.last_frame.table_rows.start);
    harness
}

/// Change le zoom et laisse l'interface se stabiliser.
fn set_zoom(harness: &mut Harness<'static, App>, zoom: TimeScaleZoom) {
    harness.state_mut().dispatch(Action::SetZoom(zoom));
    harness.run();
    assert_eq!(harness.state().state.zoom, zoom);
}

#[test]
fn snapshot_welcome_fr_light() {
    let mut harness = harness(LanguagePref::Fr, ThemePref::Light);
    harness.run();
    assert!(harness.state().document.is_none());
    let open_demo = Texts::new(Language::Fr).get(TextId::WelcomeOpenDemo);
    assert!(harness.query_by_label(&open_demo).is_some());
    snapshot(&mut harness, "welcome_fr_light");
}

#[test]
fn snapshot_main_fr_light() {
    let mut harness = demo(LanguagePref::Fr, ThemePref::Light);
    assert_eq!(harness.state().state.zoom, TimeScaleZoom::Month);
    snapshot(&mut harness, "main_fr_light");
}

#[test]
fn snapshot_main_en_dark() {
    let mut harness = demo(LanguagePref::En, ThemePref::Dark);
    assert_eq!(harness.ctx.theme(), egui::Theme::Dark);
    assert_eq!(harness.state().texts().language(), Language::En);
    snapshot(&mut harness, "main_en_dark");
}

#[test]
fn snapshot_main_fr_light_week() {
    let mut harness = demo(LanguagePref::Fr, ThemePref::Light);
    set_zoom(&mut harness, TimeScaleZoom::Week);
    snapshot(&mut harness, "main_fr_light_week");
}

#[test]
fn snapshot_main_fr_light_day() {
    let mut harness = demo(LanguagePref::Fr, ThemePref::Light);
    set_zoom(&mut harness, TimeScaleZoom::Day);
    // Défile jusqu'à deux jours avant la date d'état (abscisse calculée par
    // l'axe de l'interface, jamais par le test).
    let doc = harness.state().document.as_ref().unwrap();
    let axis = TimeAxis::new(doc.range, TimeScaleZoom::Day, 0.0);
    let target = axis.x(doc.project().effective_status_date()) - 2.0 * axis.px_per_day();
    let target = target.max(0.0);
    harness.state_mut().dispatch(Action::ScrollGanttX(target));
    harness.run();
    let scroll = harness.state().view().gantt_scroll_x;
    assert!(scroll.is_finite() && scroll > 0.0, "aucun défilement : {scroll}");
    snapshot(&mut harness, "main_fr_light_day");
}

/// H-UI-85 (correctif de la saisie de cellule, 2026-10-27) : cellule en
/// saisie ouverte par F2, dessinée dans la cellule (police, graisse,
/// alignement et retrait de la cellule, fond uni, cadre du curseur),
/// curseur de texte en fin de texte (clignotement coupé pour une capture
/// stable). Clair : nom indenté d'une tâche ; sombre : durée alignée à
/// droite d'une récapitulative (gras simulé).
fn cell_edit_capture(theme: ThemePref, uid: u32, column: plannercraft_ui_egui::column::Column, name: &str) {
    let mut harness = demo(LanguagePref::Fr, theme);
    harness
        .ctx
        .all_styles_mut(|style| style.visuals.text_cursor.blink = false);
    let uid = plannercraft_core::TaskUid::new(uid);
    harness.state_mut().dispatch(Action::Select(Some(uid)));
    harness.run();
    harness.state_mut().view_mut().cursor.column = column;
    harness.key_press(egui::Key::F2);
    harness.run();
    let app = harness.state();
    let editing = app.view().edit.editing.as_ref().expect("saisie ouverte");
    assert_eq!((editing.uid, editing.column), (Some(uid), column));
    assert!(
        harness
            .ctx
            .memory(|memory| memory.has_focus(plannercraft_ui_egui::grid::editor_id()))
    );
    snapshot(&mut harness, name);
}

#[test]
fn snapshot_cell_edit_fr_light() {
    cell_edit_capture(
        ThemePref::Light,
        3,
        plannercraft_ui_egui::column::Column::Name,
        "cell_edit_fr_light",
    );
}

#[test]
fn snapshot_cell_edit_fr_dark() {
    cell_edit_capture(
        ThemePref::Dark,
        1,
        plannercraft_ui_egui::column::Column::Duration,
        "cell_edit_fr_dark",
    );
}

#[test]
fn snapshot_main_fr_light_selected_scrolled() {
    let mut harness = demo(LanguagePref::Fr, ThemePref::Light);
    let uid = harness.state().rows().unwrap().row(29).unwrap().uid;
    harness.state_mut().dispatch(Action::ScrollToRow(29));
    harness.state_mut().dispatch(Action::Select(Some(uid)));
    harness.run();
    let app = harness.state();
    assert_eq!(app.selection, Some(uid));
    let rows = &app.last_frame.table_rows;
    assert!(
        rows.start > 0 && rows.contains(&29),
        "ligne 30 non visible : {rows:?}"
    );
    assert_eq!(app.last_frame.table_rows, app.last_frame.gantt_rows);
    snapshot(&mut harness, "main_fr_light_selected_scrolled");
}

#[test]
fn snapshot_diagnostics_open_fr_light() {
    let mut harness = harness(LanguagePref::Fr, ThemePref::Light);
    harness.run();
    harness.state_mut().open_bytes(b"<x>", DocOrigin::default());
    harness.run();
    assert_eq!(harness.state().messages().len(), 1, "bandeau d'erreur attendu");
    let title = Texts::new(Language::Fr).get(TextId::OpenErrorTitle);
    assert!(harness.query_by_label(&title).is_some());
    harness.state_mut().dispatch(Action::ToggleDiagnostics);
    harness.run();
    assert!(harness.state().state.diagnostics_open);
    snapshot(&mut harness, "diagnostics_open_fr_light");
}

/// Récapitulatives × (1 + feuilles) du réseau de 5 000 tâches (`perf.rs`).
const NET_SUMMARIES: usize = 250;
const NET_LEAVES: usize = 19;
const NET_LINKS: usize = 10_000;

/// Position de la ligne visée dans le réseau.
#[derive(Clone, Copy, Debug)]
enum Position {
    Start,
    Middle,
    End,
}

impl Position {
    const fn name(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Middle => "middle",
            Self::End => "end",
        }
    }

    const fn row(self, len: usize) -> usize {
        match self {
            Self::Start => 0,
            Self::Middle => len / 2,
            Self::End => len.saturating_sub(1),
        }
    }
}

/// « Maintenant » du réseau généré : son début (comme `snapshot --generate`).
fn net_now() -> Moment {
    "2026-01-05T08:00".parse().unwrap()
}

/// Réseau de 5 000 tâches ouvert (français, `theme`), premier dessin fait.
fn net5000(theme: ThemePref) -> Harness<'static, App> {
    let state = UiState {
        language: LanguagePref::Fr,
        theme,
        ..UiState::default()
    };
    let mut app = App::new(state, Env::fixed(net_now()));
    let origin = DocOrigin {
        name: "reseau-5000.xml".to_owned(),
        path: None,
        demo: false,
    };
    app.open_project(ui_network(NET_SUMMARIES, NET_LEAVES, NET_LINKS), origin);
    let mut harness = harness_with(app, theme);
    harness.run();
    let app = harness.state();
    let doc = app
        .document
        .as_ref()
        .unwrap_or_else(|| panic!("réseau refusé : {:?}", app.messages()));
    assert_eq!(doc.project().tasks.len(), NET_SUMMARIES * (NET_LEAVES + 1));
    assert!(
        doc.schedule().diagnostics.is_empty(),
        "{:?}",
        doc.schedule().diagnostics.first()
    );
    assert!(app.messages().is_empty(), "{:?}", app.messages());
    harness
}

/// Capture `net5000_<position>_<zoom>_<thème>` : zoom imposé, ligne visée
/// sélectionnée et montrée, Gantt défilé au début de sa barre moins deux
/// jours (abscisse par l'axe de l'interface, jamais calculée ici).
fn net5000_capture(position: Position, zoom: TimeScaleZoom, theme: ThemePref) {
    let mut harness = net5000(theme);
    set_zoom(&mut harness, zoom);
    let len = harness.state().rows().unwrap().len();
    let row = position.row(len);
    let (uid, bar) = {
        let r = harness.state().rows().unwrap().row(row).unwrap();
        (r.uid, r.bar)
    };
    harness.state_mut().dispatch(Action::Select(Some(uid)));
    harness.state_mut().dispatch(Action::ScrollToRow(row));
    harness.run();
    let app = harness.state();
    let doc = app.document.as_ref().unwrap();
    let axis = TimeAxis::new(doc.range, zoom, 0.0);
    let (start, _) = bar.expect("ligne visée sans barre");
    let target = (axis.x(start) - 2.0 * axis.px_per_day()).max(0.0);
    let delta = target - app.view().gantt_scroll_x;
    harness.state_mut().dispatch(Action::ScrollGanttX(delta));
    harness.run();
    let app = harness.state();
    assert_eq!(app.selection, Some(uid));
    assert_eq!(app.state.zoom, zoom);
    assert!(
        app.last_frame.table_rows.contains(&row),
        "ligne {row} non peinte : {:?}",
        app.last_frame.table_rows
    );
    assert_eq!(app.last_frame.table_rows, app.last_frame.gantt_rows);
    let theme_name = match theme {
        ThemePref::Dark => "dark",
        ThemePref::Light | ThemePref::System => "light",
    };
    let zoom_name = format!("{zoom:?}").to_lowercase();
    snapshot(
        &mut harness,
        &format!("net5000_{}_{zoom_name}_{theme_name}", position.name()),
    );
}

macro_rules! net5000_snapshots {
    ($($name:ident: $position:ident, $zoom:ident, $theme:ident;)*) => {
        $(
            #[test]
            fn $name() {
                net5000_capture(Position::$position, TimeScaleZoom::$zoom, ThemePref::$theme);
            }
        )*
    };
}

net5000_snapshots! {
    snapshot_net5000_start_week_light: Start, Week, Light;
    snapshot_net5000_start_week_dark: Start, Week, Dark;
    snapshot_net5000_start_quarter_light: Start, Quarter, Light;
    snapshot_net5000_start_quarter_dark: Start, Quarter, Dark;
    snapshot_net5000_middle_week_light: Middle, Week, Light;
    snapshot_net5000_middle_week_dark: Middle, Week, Dark;
    snapshot_net5000_middle_quarter_light: Middle, Quarter, Light;
    snapshot_net5000_middle_quarter_dark: Middle, Quarter, Dark;
    snapshot_net5000_end_week_light: End, Week, Light;
    snapshot_net5000_end_week_dark: End, Week, Dark;
    snapshot_net5000_end_quarter_light: End, Quarter, Light;
    snapshot_net5000_end_quarter_dark: End, Quarter, Dark;
}

// --- ADR-0017, lot L5 : ruban et chronologie ---

/// « P » ouvert (`language`, `theme`), fenêtre `size` à `ppp`, tâche 3
/// sélectionnée (éléments de sélection actifs), tâches `timeline` sur la
/// frise (un lot `style.apply`, le geste « Ajouter à la chronologie »).
fn p_capture(
    language: LanguagePref,
    theme: ThemePref,
    size: egui::Vec2,
    ppp: f32,
    timeline: &[u32],
) -> Harness<'static, App> {
    let state = UiState {
        language,
        theme,
        ..UiState::default()
    };
    let mut app = App::new(state, Env::fixed(crate::now()));
    app.open_project(
        crate::filters::project_p(),
        DocOrigin {
            name: "P.xml".to_owned(),
            path: None,
            demo: false,
        },
    );
    let mut harness = harness_sized(app, theme, size, ppp);
    harness.run();
    if !timeline.is_empty() {
        let json = serde_json::json!({
            "format_version": 1,
            "ops": [{
                "op": "style.apply",
                "template": harness.state().state.default_template.key(),
                "patch": { "timeline": { "tasks": timeline } }
            }]
        });
        let batch = plannercraft_automation::Batch::from_json_str(&json.to_string()).unwrap();
        harness.state_mut().dispatch(Action::Exec(batch));
    }
    harness
        .state_mut()
        .dispatch(Action::Select(Some(plannercraft_core::TaskUid::new(3))));
    harness.run();
    let app = harness.state();
    assert!(app.document.is_some(), "{:?}", app.messages());
    assert_eq!(app.selection, Some(plannercraft_core::TaskUid::new(3)));
    harness
}

/// « P » 1 280 × 720, français, clair, frise vide.
fn p_fr_light() -> Harness<'static, App> {
    p_capture(
        LanguagePref::Fr,
        ThemePref::Light,
        egui::vec2(1280.0, 720.0),
        1.0,
        &[],
    )
}

/// Choisit l'onglet `tab` du ruban.
fn select_tab(harness: &mut Harness<'static, App>, tab: RibbonTab) {
    harness
        .state_mut()
        .dispatch(Action::Ribbon(RibbonOp::Select(tab)));
    harness.run();
    assert_eq!(harness.state().state.ribbon.tab, tab);
    assert!(!drawn(&harness.ctx).items.is_empty(), "ruban vide sur {tab:?}");
}

macro_rules! ribbon_tab_snapshots {
    ($($name:ident: $tab:ident, $file:literal;)*) => {
        $(
            #[test]
            fn $name() {
                let mut harness = p_fr_light();
                select_tab(&mut harness, RibbonTab::$tab);
                snapshot(&mut harness, $file);
            }
        )*
    };
}

ribbon_tab_snapshots! {
    snapshot_ribbon_task_fr_light: Task, "ribbon_task_fr_light";
    snapshot_ribbon_resource_fr_light: Resource, "ribbon_resource_fr_light";
    snapshot_ribbon_project_fr_light: Project, "ribbon_project_fr_light";
    snapshot_ribbon_view_fr_light: View, "ribbon_view_fr_light";
    snapshot_ribbon_format_fr_light: Format, "ribbon_format_fr_light";
}

#[test]
fn snapshot_ribbon_task_en_dark() {
    let mut harness = p_capture(
        LanguagePref::En,
        ThemePref::Dark,
        egui::vec2(1280.0, 720.0),
        1.0,
        &[],
    );
    assert_eq!(harness.ctx.theme(), egui::Theme::Dark);
    assert_eq!(harness.state().texts().language(), Language::En);
    select_tab(&mut harness, RibbonTab::Task);
    snapshot(&mut harness, "ribbon_task_en_dark");
}

/// H-UI-72 : replié, seule la bande d'onglets reste.
#[test]
fn snapshot_ribbon_collapsed_fr_light() {
    let mut harness = p_fr_light();
    harness
        .state_mut()
        .dispatch(Action::Ribbon(RibbonOp::ToggleCollapsed));
    harness.run();
    assert!(harness.state().state.ribbon.collapsed);
    assert!(drawn(&harness.ctx).body.is_none());
    snapshot(&mut harness, "ribbon_collapsed_fr_light");
}

/// H-UI-72 : replié, onglet Projet ouvert en surimpression sur la vue.
#[test]
fn snapshot_ribbon_overlay_fr_light() {
    let mut harness = p_fr_light();
    harness
        .state_mut()
        .dispatch(Action::Ribbon(RibbonOp::ToggleCollapsed));
    harness
        .state_mut()
        .dispatch(Action::Ribbon(RibbonOp::OpenOverlay(RibbonTab::Project)));
    harness.run();
    assert_eq!(harness.state().view().ribbon_overlay, Some(RibbonTab::Project));
    assert!(drawn(&harness.ctx).body.is_some());
    // Capture sans pointeur : la surimpression reste ouverte (seuls une
    // action, Échap ou un clic la referment).
    harness.remove_cursor();
    harness.run();
    assert_eq!(harness.state().view().ribbon_overlay, Some(RibbonTab::Project));
    snapshot(&mut harness, "ribbon_overlay_fr_light");
}

/// H-UI-75 : 800 px, groupes réduits de droite à gauche.
#[test]
fn snapshot_ribbon_800px_fr_light() {
    let mut harness = p_capture(
        LanguagePref::Fr,
        ThemePref::Light,
        egui::vec2(800.0, 600.0),
        1.0,
        &[],
    );
    select_tab(&mut harness, RibbonTab::Task);
    let d = drawn(&harness.ctx);
    assert!(
        d.items.iter().all(|i| d.panel.contains_rect(i.rect)),
        "débordement à 800 px"
    );
    snapshot(&mut harness, "ribbon_800px_fr_light");
}

/// H-UI-77 : icônes vectorielles nettes à 2 pixels par point.
#[test]
fn snapshot_ribbon_hidpi_fr_light() {
    let mut harness = p_capture(
        LanguagePref::Fr,
        ThemePref::Light,
        egui::vec2(1024.0, 600.0),
        2.0,
        &[],
    );
    assert!((harness.ctx.pixels_per_point() - 2.0).abs() < f32::EPSILON);
    select_tab(&mut harness, RibbonTab::Task);
    snapshot(&mut harness, "ribbon_hidpi_fr_light");
}

/// Tâches de la frise des captures (cahier du ruban, lot L5).
const TIMELINE: [u32; 5] = [2, 3, 4, 6, 7];

/// H-UI-78, H-UI-80 : frise de « P » avec 2, 3, 4, 6 et 7.
fn timeline_capture(language: LanguagePref, theme: ThemePref, name: &str) {
    let mut harness = p_capture(language, theme, egui::vec2(1280.0, 720.0), 1.0, &TIMELINE);
    let app = harness.state();
    let doc = app.document.as_ref().unwrap();
    assert_eq!(
        app.state.effective_style(doc.project()).timeline.tasks,
        TIMELINE.to_vec()
    );
    let drawn = app.view().timeline.drawn.clone().expect("frise dessinée");
    assert_eq!(drawn.elements.len(), TIMELINE.len(), "{:?}", drawn.elements);
    snapshot(&mut harness, name);
}

#[test]
fn snapshot_timeline_fr_light() {
    timeline_capture(LanguagePref::Fr, ThemePref::Light, "timeline_fr_light");
}

#[test]
fn snapshot_timeline_en_dark() {
    timeline_capture(LanguagePref::En, ThemePref::Dark, "timeline_en_dark");
}

/// H-UI-82 : mode impression, frise affichée ⇒ case cochée d'office, frise
/// en tête de la page 1 de l'aperçu.
#[test]
fn snapshot_print_timeline_fr_light() {
    use plannercraft_ui_egui::action::DialogKind;
    use plannercraft_ui_egui::dialog::DialogBody;
    let mut harness = p_capture(
        LanguagePref::Fr,
        ThemePref::Light,
        egui::vec2(1280.0, 720.0),
        1.0,
        &TIMELINE,
    );
    harness
        .state_mut()
        .dispatch(Action::ShowDialog(DialogKind::PageSetup { print_mode: true }));
    harness.run();
    let Some(DialogBody::PageSetup(state)) = harness.state().view().dialog.as_ref().map(|d| &d.body) else {
        panic!("mode impression non ouvert");
    };
    assert!(state.page.timeline, "frise cochée d'office");
    snapshot(&mut harness, "print_timeline_fr_light");
}

// --- Passe UX (ADR-0018 D11, lot L5) : retours visuels de la sélection ---

/// Appui de sélection sur (`n`, `column`) ; colonne ID : en-tête de ligne.
fn press(harness: &mut Harness<'static, App>, n: u32, column: Column, mode: PressMode) {
    let target = if column == Column::Id {
        PressTarget::RowHeader
    } else {
        PressTarget::Cell
    };
    harness.state_mut().dispatch(Action::SelectOp(SelectOp::Press {
        row: RowRef::Item(plannercraft_core::TaskUid::new(n)),
        column,
        target,
        mode,
    }));
    harness.run();
}

/// Bloc de (`a`) à (`b`) : appui puis Maj+appui.
fn select_block(harness: &mut Harness<'static, App>, a: (u32, Column), b: (u32, Column)) {
    press(harness, a.0, a.1, PressMode::Replace);
    press(harness, b.0, b.1, PressMode::Extend);
}

/// Ligne affichée de la tâche `n`.
fn shown(app: &App, n: u32) -> usize {
    app.visible_rows()
        .unwrap()
        .display_of(plannercraft_core::TaskUid::new(n))
        .unwrap()
        .index()
}

/// « P », thème `theme`, bloc 2-5 × Durée-Début : sept cellules teintées,
/// (5, Début) active non teintée (fond de la ligne), ID 2 à 5 appuyés, une
/// bordure (exemple de H-UI-103).
fn block_2_5(theme: ThemePref) -> Harness<'static, App> {
    let mut harness = p_capture(LanguagePref::Fr, theme, egui::vec2(1280.0, 720.0), 1.0, &[]);
    select_block(&mut harness, (2, Column::Duration), (5, Column::Start));
    let app = harness.state();
    let d = selection_drawn(&harness.ctx);
    let rows: Vec<usize> = (2..=5).map(|n| shown(app, n)).collect();
    assert_eq!(d.tinted.len(), 7, "{d:?}");
    assert!(
        d.tinted
            .iter()
            .all(|(r, c)| rows.contains(r) && matches!(c, Column::Duration | Column::Start))
    );
    let (row, column, fill) = d.active.unwrap();
    assert_eq!((row, column), (shown(app, 5), Column::Start));
    let variant = plannercraft_ui_egui::theme::variant_for(harness.ctx.theme());
    let style = app
        .state
        .effective_style(app.document.as_ref().unwrap().project());
    let palette = plannercraft_ui_egui::theme::Palette::from_style(&style, variant);
    assert_ne!(fill, palette.selection, "cellule active teintée");
    assert_eq!(d.pressed_ids, rows);
    assert_eq!(d.borders.len(), 1);
    assert_eq!(d.full_rows, Vec::<usize>::new());
    harness
}

/// H-UI-103 : la cellule active d'un bloc n'est pas teintée (fond de sa
/// ligne dans son cadre) ; une cellule seule n'est pas teintée du tout,
/// son ID est appuyé, aucune bordure de bloc.
#[test]
fn h_ui_103_active_cell_unfilled() {
    let mut harness = block_2_5(ThemePref::Light);
    press(&mut harness, 3, Column::Name, PressMode::Replace);
    let d = selection_drawn(&harness.ctx);
    assert!(
        d.tinted.is_empty() && d.full_rows.is_empty() && d.borders.is_empty(),
        "{d:?}"
    );
    assert_eq!(d.active, None);
    assert_eq!(d.pressed_ids, vec![shown(harness.state(), 3)]);
}

/// H-UI-103 : lignes entières 6-7 teintées sur toute la largeur, ID
/// appuyés, une bordure ; aucune cellule teintée à part.
#[test]
fn h_ui_103_rows_tinted_full_width() {
    let mut harness = p_fr_light();
    select_block(&mut harness, (6, Column::Id), (7, Column::Id));
    let app = harness.state();
    let d = selection_drawn(&harness.ctx);
    let rows = vec![shown(app, 6), shown(app, 7)];
    assert_eq!(d.full_rows, rows);
    assert_eq!(d.pressed_ids, rows);
    assert_eq!(d.tinted, Vec::<(usize, Column)>::new());
    assert_eq!(d.borders.len(), 1);
}

/// H-UI-103 : survol discret de la ligne sous le pointeur ; aucun pendant
/// un glisser de sélection (bouton tenu), de retour au relâcher.
#[test]
fn h_ui_103_hover_not_during_drag() {
    let mut harness = p_fr_light();
    let three = shown(harness.state(), 3);
    let from = crate::select_mouse::cell_point(&harness, three, Column::Name);
    crate::select_mouse::move_to(&mut harness, from);
    harness.run();
    assert_eq!(selection_drawn(&harness.ctx).hover, Some(three));
    crate::select_mouse::press_at(&mut harness, from, egui::Modifiers::NONE);
    assert_eq!(selection_drawn(&harness.ctx).hover, None);
    let five = shown(harness.state(), 5);
    let to = crate::select_mouse::cell_point(&harness, five, Column::Name);
    crate::select_mouse::move_to(&mut harness, to);
    assert_eq!(selection_drawn(&harness.ctx).hover, None);
    assert_eq!(selection_drawn(&harness.ctx).full_rows.len(), 0);
    // Relâcher (kittest retire aussi le pointeur), puis survoler de nouveau.
    crate::select_mouse::release_at(&mut harness, to);
    crate::select_mouse::move_to(&mut harness, to);
    harness.run();
    assert_eq!(selection_drawn(&harness.ctx).hover, Some(five));
    // Pointeur retiré : plus de survol.
    harness.remove_cursor();
    harness.run();
    assert_eq!(selection_drawn(&harness.ctx).hover, None);
}

#[test]
fn snapshot_select_block_fr_light() {
    let mut harness = block_2_5(ThemePref::Light);
    snapshot(&mut harness, "select_block_fr_light");
}

#[test]
fn snapshot_select_block_fr_dark() {
    let mut harness = block_2_5(ThemePref::Dark);
    assert_eq!(harness.ctx.theme(), egui::Theme::Dark);
    snapshot(&mut harness, "select_block_fr_dark");
}

#[test]
fn snapshot_select_rows_fr_light() {
    let mut harness = p_fr_light();
    select_block(&mut harness, (6, Column::Id), (7, Column::Id));
    assert_eq!(selection_drawn(&harness.ctx).full_rows.len(), 2);
    snapshot(&mut harness, "select_rows_fr_light");
}

/// H-UI-94 : lignes 6-7 entières, ID 6 glissé au-dessus de la ligne 2,
/// bouton tenu : trait d'insertion de 2 px, curseur de déplacement (capture
/// prise pointeur présent : le retirer finirait le geste).
#[test]
fn snapshot_row_drag_indicator_fr_light() {
    let mut harness = p_fr_light();
    select_block(&mut harness, (6, Column::Id), (7, Column::Id));
    let app = harness.state();
    let from = crate::select_mouse::cell_point(&harness, shown(app, 6), Column::Id);
    let two = crate::select_mouse::cell_point(&harness, shown(app, 2), Column::Id);
    let row_h = harness.state().row_h();
    let to = egui::pos2(two.x, two.y - row_h * 0.4);
    crate::select_mouse::press_at(&mut harness, from, egui::Modifiers::NONE);
    for k in 1..=4_u8 {
        let t = f32::from(k) / 4.0;
        crate::select_mouse::move_to(&mut harness, from + (to - from) * t);
    }
    harness.step();
    assert!(
        harness.state().view().row_drag.is_some(),
        "déplacement non commencé"
    );
    if crate::common::snapshot_mode() == crate::common::SnapshotMode::Required {
        harness.snapshot("row_drag_indicator_fr_light");
    }
    crate::select_mouse::release_at(&mut harness, to);
}

/// Groupe Édition (ADR-0018 D10) : liste « Remplir ▾ » ouverte sur un bloc
/// Durée 2-3 ; 1 920 px (onglet Tâche entier sans réduction, H-UI-75).
#[test]
fn snapshot_ribbon_task_editing_fr_light() {
    use egui_kittest::kittest::NodeT as _;
    let mut harness = p_capture(
        LanguagePref::Fr,
        ThemePref::Light,
        egui::vec2(1920.0, 720.0),
        1.0,
        &[],
    );
    select_tab(&mut harness, RibbonTab::Task);
    select_block(&mut harness, (2, Column::Duration), (3, Column::Duration));
    let texts = Texts::new(Language::Fr);
    harness.get_by_label(&texts.get(TextId::RibbonFill)).click();
    harness.run();
    let entry = texts.get(TextId::RibbonFillDown);
    assert!(
        harness.query_by_label_contains(&entry).is_some(),
        "liste « Remplir » fermée"
    );
    snapshot(&mut harness, "ribbon_task_editing_fr_light");
    assert!(harness.query_by_label_contains(&entry).is_some());
    // Entrée « Vers le bas Ctrl+D » : raccourci de la commande affiché.
    let name = harness
        .get_by_label_contains(&entry)
        .accesskit_node()
        .label()
        .unwrap_or_default();
    assert!(name.ends_with("Ctrl+D"), "{name}");
}

/// Exemple `snapshot`, inclus tel quel : `--block` sélectionne le bloc par
/// les mêmes appuis que la table (passe UX, lot L5).
#[allow(
    dead_code,
    clippy::duplicate_mod,
    reason = "`main` et `run` de l'exemple ne servent pas au test ; `desktop` l'inclut aussi, en module privé"
)]
#[path = "../../examples/snapshot.rs"]
mod snapshot_example;

#[test]
fn snapshot_example_block_option() {
    let args = |list: &[&str]| snapshot_example::parse_args(list.iter().map(std::ffi::OsString::from));
    let parsed = args(&["--p", "main", "--block", "2:duration-5:start"]).unwrap();
    assert_eq!(parsed.block, Some([(2, Column::Duration), (5, Column::Start)]));
    for bad in ["2:duration", "2-5", "x:duration-5:start", "2:durée-5:start"] {
        assert!(args(&["--p", "main", "--block", bad]).is_err(), "{bad}");
    }
    let harness = snapshot_example::prepared(&parsed, None).unwrap();
    let d = selection_drawn(&harness.ctx);
    assert_eq!(d.tinted.len(), 7, "{d:?}");
    let rows = args(&["--p", "main", "--block", "6:id-7:id"]).unwrap();
    let harness = snapshot_example::prepared(&rows, None).unwrap();
    assert_eq!(selection_drawn(&harness.ctx).full_rows.len(), 2);
    let missing = args(&["--p", "main", "--block", "2:name-99:name"]).unwrap();
    assert!(snapshot_example::prepared(&missing, None).is_err());
}

// --- Seconde passe UX (ADR-0019, lot L10) : volets, colonnes, champs
// personnalisés, notifications ; volet des diagnostics (H-UI-119).

/// UID de la tâche nommée `name` dans le document ouvert.
fn uid_named(app: &App, name: &str) -> plannercraft_core::TaskUid {
    let project = app.document.as_ref().unwrap().project();
    project.tasks.iter().find(|t| t.name == name).map_or_else(
        || panic!("tâche « {name} » absente"),
        plannercraft_core::Task::uid,
    )
}

/// Applique un lot JSON au document (préparation d'une capture).
fn exec_ops(harness: &mut Harness<'static, App>, ops: &serde_json::Value) {
    let batch =
        plannercraft_automation::Batch::from_json(serde_json::json!({ "format_version": 1, "ops": ops }))
            .unwrap();
    harness
        .state_mut()
        .execute(&batch, plannercraft_ui_egui::EditOrigin::User)
        .unwrap();
    harness.run();
}

/// H-UI-112 : volet de détails (Formulaire tâche) ouvert sur « Fouilles
/// des fondations » de l'exemple, forme par défaut (ressources et
/// prédécesseurs), 1 440 × 900, volet tiré à 360 points.
fn details_capture(theme: ThemePref, name: &str) {
    // Fenêtre du cahier (1 440 × 900) : formulaire et grilles lisibles.
    let app = crate::common::app(LanguagePref::Fr, theme);
    let mut harness = harness_sized(app, theme, egui::vec2(1440.0, 900.0), 1.0);
    harness.run();
    harness.state_mut().open_demo();
    harness.run();
    let uid = uid_named(harness.state(), "Fouilles des fondations");
    harness.state_mut().dispatch(Action::SelectOp(SelectOp::Press {
        row: RowRef::Item(uid),
        column: Column::Name,
        target: PressTarget::Cell,
        mode: PressMode::Replace,
    }));
    harness
        .state_mut()
        .dispatch(Action::Pane(plannercraft_ui_egui::action::PaneOp::ShowDetails(
            true,
        )));
    // Volet tiré à 360 points : les grilles du bas visibles sans défiler
    // (à 220 points, hauteur par défaut, elles demandent un défilement).
    harness.state_mut().dispatch(Action::Pane(
        plannercraft_ui_egui::action::PaneOp::SetDetailsHeight(360),
    ));
    harness.run();
    // Table défilée jusqu'à la tâche suivie.
    let index = harness.state().rows().unwrap().index_of(uid).unwrap();
    harness.state_mut().dispatch(Action::ScrollToRow(index));
    harness.run();
    let app = harness.state();
    assert!(app.state.details.visible);
    assert_eq!(app.state.details.height, 360);
    assert!(app.last_frame.table_rows.contains(&index));
    assert_eq!(app.selection, Some(uid));
    let title = Texts::new(Language::Fr).get(TextId::DetailsTitle);
    assert!(harness.query_by_label(&title).is_some(), "volet absent");
    snapshot(&mut harness, name);
}

#[test]
fn snapshot_details_pane_fr_light() {
    details_capture(ThemePref::Light, "details_pane_fr_light");
}

#[test]
fn snapshot_details_pane_fr_dark() {
    details_capture(ThemePref::Dark, "details_pane_fr_dark");
}

/// H-UI-114 : sélecteur de champ ouvert depuis la colonne Durée, « trav »
/// tapé (Travail, Travail réel…).
#[test]
fn snapshot_field_picker_fr_light() {
    let mut harness = demo(LanguagePref::Fr, ThemePref::Light);
    let uid = uid_named(harness.state(), "Fouilles des fondations");
    harness.state_mut().dispatch(Action::SelectOp(SelectOp::Press {
        row: RowRef::Item(uid),
        column: Column::Duration,
        target: PressTarget::Cell,
        mode: PressMode::Replace,
    }));
    harness.run();
    harness.state_mut().dispatch(Action::Column(
        plannercraft_ui_egui::action::ColumnOp::OpenPicker {
            at: plannercraft_ui_egui::action::ACTIVE_COLUMN,
        },
    ));
    harness.run();
    harness.event(egui::Event::Text("trav".to_owned()));
    harness.run();
    let picker = harness.state().view().picker.clone().expect("sélecteur ouvert");
    assert_eq!(picker.query, "trav");
    snapshot(&mut harness, "field_picker_fr_light");
}

/// H-UI-104, H-UI-105 : refus d'un lien déjà présent en notification (en
/// bas à droite, liseré et icône du refus, « Détails » replié), sans
/// bandeau.
fn toast_capture(theme: ThemePref, name: &str) {
    let mut harness = demo(LanguagePref::Fr, theme);
    let (pred, succ) = {
        let project = harness.state().document.as_ref().unwrap().project();
        project
            .tasks
            .iter()
            .find_map(|t| {
                t.predecessors
                    .first()
                    .map(|l| (l.predecessor.get(), t.uid().get()))
            })
            .unwrap()
    };
    let batch = plannercraft_automation::Batch::from_json(serde_json::json!({
        "format_version": 1,
        "ops": [{ "op": "link.add", "pred": pred, "succ": succ }]
    }))
    .unwrap();
    assert!(
        harness
            .state_mut()
            .execute(&batch, plannercraft_ui_egui::EditOrigin::User)
            .is_err()
    );
    harness.run();
    let store = harness.state().message_store();
    assert_eq!(store.banner().len(), 0);
    assert_eq!(store.toasts().len(), 1);
    let text = store.toasts()[0].text.clone();
    assert!(!text.contains("link.") && !text.contains("opération"), "{text}");
    assert!(harness.query_by_label(&text).is_some());
    snapshot(&mut harness, name);
}

#[test]
fn snapshot_toast_fr_light() {
    toast_capture(ThemePref::Light, "toast_fr_light");
}

#[test]
fn snapshot_toast_fr_dark() {
    toast_capture(ThemePref::Dark, "toast_fr_dark");
}

/// H-UI-111 : frise de « P » agrandie à 152 points (séparateur tiré de
/// 80 points).
#[test]
fn snapshot_timeline_tall_fr_light() {
    let mut harness = p_capture(
        LanguagePref::Fr,
        ThemePref::Light,
        egui::vec2(1280.0, 720.0),
        1.0,
        &TIMELINE,
    );
    harness.state_mut().dispatch(Action::Pane(
        plannercraft_ui_egui::action::PaneOp::SetTimelineHeight(152),
    ));
    harness.run();
    assert_eq!(harness.state().state.timeline_height, 152);
    let drawn = harness
        .state()
        .view()
        .timeline
        .drawn
        .clone()
        .expect("frise dessinée");
    assert_eq!(drawn.elements.len(), TIMELINE.len());
    snapshot(&mut harness, "timeline_tall_fr_light");
}

/// H-UI-118 : Texte1 renommé « Lot » et Nombre1, insérés après le Nom,
/// valeurs saisies sur quatre tâches de l'exemple.
#[test]
fn snapshot_custom_columns_fr_light() {
    use plannercraft_core::{CustomFieldId, CustomKind};
    let mut harness = demo(LanguagePref::Fr, ThemePref::Light);
    let names = [
        ("Installation du chantier", "Lot 1", "12,5"),
        ("Décapage et terrassement", "Lot 1", "40"),
        ("Fouilles des fondations", "Lot 2", "3,25"),
        ("Coulage des fondations", "Lot 2", "18"),
    ];
    let mut ops = vec![serde_json::json!({ "op": "custom_field.rename", "field": "text1", "alias": "Lot" })];
    for (name, lot, number) in names {
        let uid = uid_named(harness.state(), name).get();
        ops.push(serde_json::json!({
            "op": "task.update", "uid": uid, "custom": { "text1": lot, "number1": number }
        }));
    }
    exec_ops(&mut harness, &serde_json::Value::Array(ops));
    let text1 = Column::Custom(CustomFieldId::new(CustomKind::Text, 1).unwrap());
    let number1 = Column::Custom(CustomFieldId::new(CustomKind::Number, 1).unwrap());
    // Après N°, Indicateurs (finitions P1) et Nom.
    for (at, column) in [(3, text1), (4, number1)] {
        harness
            .state_mut()
            .dispatch(Action::Column(plannercraft_ui_egui::action::ColumnOp::Insert {
                at,
                column,
            }));
    }
    let uid = uid_named(harness.state(), "Fouilles des fondations");
    harness.state_mut().dispatch(Action::Select(Some(uid)));
    harness.run();
    let shown: Vec<Column> = harness
        .state()
        .state
        .visible_columns()
        .map(|c| c.column)
        .collect();
    assert_eq!(
        &shown[..5],
        &[Column::Id, Column::Indicators, Column::Name, text1, number1]
    );
    let rows = harness.state().rows().unwrap();
    let index = rows.index_of(uid).unwrap();
    assert_eq!(rows.text(index, text1), "Lot 2");
    assert_eq!(rows.text(index, number1), "3,25");
    assert!(
        harness
            .query_all_by_label_contains("Lot (Texte1)")
            .next()
            .is_some()
    );
    snapshot(&mut harness, "custom_columns_fr_light");
}

/// H-UI-114 (L10) : la colonne « Ajouter une nouvelle colonne » est
/// affichée par défaut en bout de table et comptée par le défilement.
#[test]
fn h_ui_114_add_new_column_shown_by_default() {
    let harness = demo(LanguagePref::Fr, ThemePref::Light);
    assert!(plannercraft_ui_egui::view::table::add_column_slot(&harness.ctx));
    let state = &harness.state().state;
    let columns: f32 = state.visible_columns().map(|c| f32::from(c.width)).sum();
    let content = plannercraft_ui_egui::view::table::content_width(&harness.ctx, state);
    assert!((content - columns - plannercraft_ui_egui::view::table::ADD_COLUMN_W).abs() < 0.01);
}

/// Projet de l'exemple avec un lien pendant (diagnostic d'import) et un
/// lien vers une tâche absente (diagnostic de calcul ou d'import).
fn demo_with_diagnostics() -> Harness<'static, App> {
    let xml = std::str::from_utf8(plannercraft_ui_egui::DEMO_MSPDI)
        .unwrap()
        .replacen("<PredecessorUID>", "<PredecessorUID>777", 1);
    let mut harness = harness(LanguagePref::Fr, ThemePref::Light);
    harness.run();
    harness
        .state_mut()
        .open_bytes(xml.as_bytes(), DocOrigin::default());
    harness.run();
    assert!(harness.state().document.is_some());
    assert_ne!(harness.state().diagnostics_view().entries.len(), 0);
    harness
}

/// H-UI-119 : sans diagnostic (exemple embarqué, ou aucun document), la
/// case Affichage → Diagnostics cochée ne dessine aucun volet (les
/// pastilles disent déjà « aucune erreur ») ; avec un diagnostic, le
/// volet est là.
#[test]
fn h_ui_119_no_empty_diagnostics_pane() {
    use plannercraft_ui_egui::view::diagnostics::drawn;
    let mut harness = demo(LanguagePref::Fr, ThemePref::Light);
    harness.state_mut().dispatch(Action::ToggleDiagnostics);
    harness.run();
    assert!(harness.state().state.diagnostics_open, "case gardée cochée");
    assert!(!drawn(&harness.ctx), "volet vide dessiné");
    let none = Texts::new(Language::Fr).get_args(
        TextId::StatusErrors,
        &[("count", plannercraft_ui_egui::i18n::Arg::Count(0))],
    );
    assert_eq!(harness.query_all_by_label(&none).count(), 1, "pastille seule");
    harness.state_mut().dispatch(Action::CloseDocument);
    harness.run();
    assert!(harness.state().document.is_none());
    assert!(!drawn(&harness.ctx));
    let mut harness = demo_with_diagnostics();
    harness.state_mut().dispatch(Action::ToggleDiagnostics);
    harness.run();
    assert!(drawn(&harness.ctx));
}

/// H-UI-119 : clic sur la pastille d'une gravité présente ⇒ volet ouvert,
/// filtré sur elle (ses seules lignes) ; même pastille ⇒ fermé ; volet
/// fermé ⇒ filtre retiré (la case du menu l'ouvre sur tout) ; « Tout
/// afficher » retire le filtre.
#[test]
fn h_ui_119_pill_opens_filtered() {
    use plannercraft_core::Severity;
    use plannercraft_ui_egui::i18n::Arg;
    use plannercraft_ui_egui::view::diagnostics::{drawn, filter, severity_range};
    let mut harness = demo_with_diagnostics();
    let entries = harness.state().diagnostics_view().entries.clone();
    let (severity, id, count) = [
        (Severity::Error, TextId::StatusErrors),
        (Severity::Warning, TextId::StatusWarnings),
    ]
    .into_iter()
    .map(|(s, id)| (s, id, severity_range(&entries, s).len()))
    .find(|(_, _, n)| *n > 0)
    .expect("erreur ou avertissement attendu");
    let count = i64::try_from(count).unwrap();
    let pill = Texts::new(Language::Fr).get_args(id, &[("count", Arg::Count(count))]);
    harness.get_by_label(&pill).click();
    harness.run();
    assert!(harness.state().state.diagnostics_open);
    assert!(drawn(&harness.ctx));
    assert_eq!(filter(&harness.ctx), Some(severity));
    // Lignes de la gravité affichées, aucune autre.
    for (s, text) in &entries {
        let shown = harness.query_by_label(text).is_some();
        assert_eq!(shown, *s == severity, "{s:?} {text}");
    }
    // « Tout afficher » : filtre retiré, volet ouvert.
    let all = Texts::new(Language::Fr).get(TextId::DiagnosticsShowAll);
    harness.get_by_label(&all).click();
    harness.run();
    assert_eq!(filter(&harness.ctx), None);
    assert!(harness.state().state.diagnostics_open);
    // Pastille de nouveau : filtre posé ; une seconde fois : fermé.
    harness.get_all_by_label(&pill).next().unwrap().click();
    harness.run();
    assert_eq!(filter(&harness.ctx), Some(severity));
    harness.get_all_by_label(&pill).next().unwrap().click();
    harness.run();
    assert!(!harness.state().state.diagnostics_open);
    assert!(!drawn(&harness.ctx));
    assert_eq!(filter(&harness.ctx), None, "filtre retiré à la fermeture");
    // Case du menu : ouvert sur tous les diagnostics.
    harness.state_mut().dispatch(Action::ToggleDiagnostics);
    harness.run();
    assert_eq!(filter(&harness.ctx), None);
    assert!(drawn(&harness.ctx));
}

/// Exemple `snapshot` (lot L10) : `--details`, `--columns`, `--toast`.
#[test]
fn snapshot_example_l10_options() {
    let args = |list: &[&str]| snapshot_example::parse_args(list.iter().map(std::ffi::OsString::from));
    let parsed = args(&[
        "--p",
        "main",
        "--details",
        "--columns",
        "name,work,text1,free_slack",
        "--toast",
    ])
    .unwrap();
    assert!(parsed.details && parsed.toast);
    let text1 = Column::Custom(
        plannercraft_core::CustomFieldId::new(plannercraft_core::CustomKind::Text, 1).unwrap(),
    );
    let columns = parsed.columns.clone().unwrap();
    assert_eq!(columns.len(), 4);
    assert_eq!(columns[0], Column::Name);
    assert_eq!(columns[2], text1);
    for bad in ["", "id", "name,name", "nom"] {
        assert!(args(&["--p", "main", "--columns", bad]).is_err(), "{bad}");
    }
    let harness = snapshot_example::prepared(&parsed, None).unwrap();
    let app = harness.state();
    assert!(app.state.details.visible);
    let shown: Vec<Column> = app.state.visible_columns().map(|c| c.column).collect();
    assert_eq!(shown.len(), 5);
    assert_eq!(shown[0], Column::Id);
    assert_eq!(&shown[1..], columns.as_slice());
    assert_eq!(app.message_store().toasts().len(), 1);
}
