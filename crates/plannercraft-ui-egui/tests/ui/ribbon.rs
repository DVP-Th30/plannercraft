//! Ruban (ADR-0017, H-UI-69 à H-UI-77) : contrat (lot L0) puis moteur,
//! contenu et menus jumeaux (lot L2).
//!
//! L0 : Ctrl/Cmd+F1 (L5 : le ruban est le seul habillage, par défaut). L2 :
//! harnais sur le projet « P » du
//! cahier (UID = ID ; 1 ⊃ 2, 3, 4 ; 5 ⊃ 6, 7) ; éléments cliqués par leur
//! nom accessible, cherchés dans le rectangle du ruban (les menus portent
//! souvent le même libellé) ; géométrie lue dans le relevé
//! `view::ribbon::drawn`.

#![allow(
    clippy::arithmetic_side_effects,
    reason = "tests : compteurs bornés par le scénario"
)]

use egui::accesskit::{Role, Toggled};
use egui::{Event, Key, Modifiers, PointerButton, Rect, Shape, vec2};
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_kittest::{Harness, Node};
use plannercraft_core::{LinkType, Project, SchedulingMode, TaskUid};
use plannercraft_ui_egui::action::{DialogKind, PressMode, PressTarget, SelectOp, TaskCmd};
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::keymap::{Binding, shortcut_for_os};
use plannercraft_ui_egui::nav::RowRef;
use plannercraft_ui_egui::ribbon::spec::{QUICK_ACCESS, RIBBON, RibbonCmd, percent_value};
use plannercraft_ui_egui::ribbon::{RibbonOp, RibbonTab};
use plannercraft_ui_egui::selection::SelectMode;
use plannercraft_ui_egui::state::{SortKey, SortSpec, ViewKind};
use plannercraft_ui_egui::view::ribbon::{Kind, RibbonDrawn, Stage, TAB_BAND_H, drawn, label_of};
use plannercraft_ui_egui::view::task_view::TaskViewOp;
use plannercraft_ui_egui::{Action, App, LanguagePref, UiState};

use crate::filters::open_p_in;
use crate::{fr, harness};

/// H-UI-69 (lot L5) : le ruban est le seul habillage, dès l'accueil ;
/// l'ancienne barre d'outils (« Ouvrir… », « Ajuster ») n'existe plus,
/// la barre de menus reste.
#[test]
fn h_ui_69_default_chrome_is_ribbon() {
    let mut harness = harness();
    harness.run();
    let drawn = drawn(&harness.ctx);
    assert!(drawn.panel.is_positive(), "ruban dessiné par défaut");
    assert_ne!(drawn.items.len(), 0);
    for id in [
        TextId::RibbonTabTask,
        TextId::RibbonTabResource,
        TextId::RibbonTabProject,
        TextId::RibbonTabView,
        TextId::RibbonTabFormat,
    ] {
        let _ = tab(&harness, &fr(id));
    }
    assert!(
        crate::shown_off_ribbon(&harness, &fr(TextId::MenuFile)),
        "barre de menus gardée"
    );
    // « Ajuster » n'est plus qu'une entrée du menu Affichage → Zoom (fermé).
    assert!(
        harness.query_by_label(&fr(TextId::ToolbarFit)).is_none(),
        "barre d'outils retirée"
    );
}

/// H-UI-72 (touche, lot L0) : Ctrl+F1 replie puis déplie le ruban, accueil
/// compris ; l'état est celui des préférences persistées.
#[test]
fn ribbon_l0_ctrl_f1_toggles_collapsed() {
    let mut harness = harness();
    harness.run();
    assert!(!harness.state().state.ribbon.collapsed);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::F1);
    harness.run();
    assert!(harness.state().state.ribbon.collapsed);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::F1);
    harness.run();
    assert!(!harness.state().state.ribbon.collapsed);
}

/// H-UI-69, H-UI-72 : onglet retenu dans les préférences ; surimpression
/// ouverte seulement ruban replié.
#[test]
fn ribbon_l0_reducer_arms() {
    let mut harness = harness();
    harness
        .state_mut()
        .dispatch(Action::Ribbon(RibbonOp::Select(RibbonTab::Format)));
    harness.run();
    assert_eq!(harness.state().state.ribbon.tab, RibbonTab::Format);
    harness
        .state_mut()
        .dispatch(Action::Ribbon(RibbonOp::ToggleCollapsed));
    harness
        .state_mut()
        .dispatch(Action::Ribbon(RibbonOp::OpenOverlay(RibbonTab::Project)));
    harness.run();
    assert_eq!(harness.state().view().ribbon_overlay, Some(RibbonTab::Project));
    harness
        .state_mut()
        .dispatch(Action::Ribbon(RibbonOp::CloseOverlay));
    harness.run();
    assert_eq!(harness.state().view().ribbon_overlay, None);
}

// --- Lot L2 : moteur, contenu, menus jumeaux ---

const fn uid(n: u32) -> TaskUid {
    TaskUid::new(n)
}

/// Application pour `state` (ruban, seul habillage depuis le lot L5).
fn ribbon_app_with(state: UiState) -> App {
    App::new(state, plannercraft_ui_egui::Env::fixed(crate::now()))
}

/// Application de test (français, thème clair).
fn ribbon_app() -> App {
    ribbon_app_with(crate::fr_light())
}

/// Harnais `width` × 900, pas d'image court (deux clics rapprochés font un
/// double-clic), OS `os`.
fn sized(app: App, width: f32, os: egui::os::OperatingSystem) -> Harness<'static, App> {
    let mut harness = Harness::builder()
        .with_size(vec2(width, 900.0))
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .with_os(os)
        .with_step_dt(1.0 / 60.0)
        .with_max_steps(8)
        .build_ui_state(plannercraft_ui_egui::ui, app);
    harness.run();
    harness
}

/// Ruban sur l'accueil (aucun document), 1 440 px.
fn welcome() -> Harness<'static, App> {
    sized(ribbon_app(), 1440.0, egui::os::OperatingSystem::Nix)
}

/// Ruban sur « P », `width` px.
fn p_at(width: f32) -> Harness<'static, App> {
    let mut app = ribbon_app();
    open_p_in(&mut app);
    sized(app, width, egui::os::OperatingSystem::Nix)
}

/// Ruban sur « P », 1 920 px (aucun groupe réduit).
fn p() -> Harness<'static, App> {
    p_at(1920.0)
}

fn project(app: &App) -> &Project {
    app.document.as_ref().unwrap().project()
}

/// Lots annulables.
fn undoable(app: &App) -> usize {
    app.document.as_ref().unwrap().session.history().entries.len()
}

/// Sélectionne `first`, puis ajoute `more` (Ctrl+clic).
fn select(harness: &mut Harness<'static, App>, first: u32, more: &[u32]) {
    harness.state_mut().dispatch(Action::Select(Some(uid(first))));
    for n in more {
        harness.state_mut().dispatch(Action::SelectWith {
            uid: uid(*n),
            mode: SelectMode::Toggle,
        });
    }
    harness.run();
}

fn dispatch(harness: &mut Harness<'static, App>, action: Action) {
    harness.state_mut().dispatch(action);
    harness.run();
}

/// Relevé du ruban de la dernière image.
fn ribbon(harness: &Harness<'static, App>) -> RibbonDrawn {
    drawn(&harness.ctx)
}

/// Rectangle du ruban (panneau, et corps en surimpression).
fn ribbon_area(harness: &Harness<'static, App>) -> Vec<Rect> {
    let d = ribbon(harness);
    let mut areas = vec![d.panel];
    areas.extend(d.body);
    areas
}

/// Chaîne de durée statique (les requêtes de kittest lient le libellé à
/// l'arbre) ; tests seulement.
fn leak(label: &str) -> &'static str {
    Box::leak(label.to_owned().into_boxed_str())
}

/// Nœud du ruban de nom `label` (hors onglets) : unique dans le ruban.
fn node<'a>(harness: &'a Harness<'static, App>, label: &str) -> Node<'a> {
    let label = leak(label);
    let areas = ribbon_area(harness);
    let mut found = harness.query_all_by_label(label).filter(|n| {
        n.accesskit_node().role() != Role::Tab && areas.iter().any(|a| a.contains(n.rect().center()))
    });
    let first = found
        .next()
        .unwrap_or_else(|| panic!("« {label} » absent du ruban"));
    assert!(found.next().is_none(), "« {label} » en double dans le ruban");
    first
}

/// Onglet de nom `label`.
fn tab<'a>(harness: &'a Harness<'static, App>, label: &str) -> Node<'a> {
    let label = leak(label);
    harness.get_by_role_and_label(Role::Tab, label)
}

/// Bouton de la barre de menus `label` (le plus haut des homonymes hors
/// onglets).
fn menu<'a>(harness: &'a Harness<'static, App>, label: &str) -> Node<'a> {
    let label = leak(label);
    named(harness, label)
        .into_iter()
        .filter(|n| n.accesskit_node().role() != Role::Tab)
        .min_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
        .unwrap_or_else(|| panic!("menu « {label} » absent"))
}

/// Nœuds nommés `label`, raccourci affiché compris (une entrée de menu
/// s'appelle « Lier les tâches Ctrl+F2 »).
fn named<'a>(harness: &'a Harness<'static, App>, label: &str) -> Vec<Node<'a>> {
    let label = leak(label);
    let prefix = format!("{label} ");
    harness
        .query_all_by_label_contains(label)
        .filter(|n| {
            let name = n.accesskit_node().label().unwrap_or_default();
            name == label || name.starts_with(&prefix)
        })
        .collect()
}

fn disabled(node: &Node<'_>) -> bool {
    node.accesskit_node().is_disabled()
}

fn toggled(node: &Node<'_>) -> Option<Toggled> {
    node.accesskit_node().toggled()
}

/// Libellés des onglets, de gauche à droite.
fn tab_labels(harness: &Harness<'static, App>) -> Vec<String> {
    let mut tabs: Vec<(f32, String)> = harness
        .query_all_by_role(Role::Tab)
        .map(|n| (n.rect().left(), n.accesskit_node().label().unwrap_or_default()))
        .collect();
    tabs.sort_by(|a, b| a.0.total_cmp(&b.0));
    tabs.into_iter().map(|(_, l)| l).collect()
}

/// H-UI-69 : cinq onglets Tâche, Ressource, Projet, Affichage, Format
/// (EN : Task, Resource, Project, View, Format), dans l'ordre.
#[test]
fn h_ui_69_five_tabs_in_order() {
    let mut harness = welcome();
    assert_eq!(
        tab_labels(&harness),
        ["Tâche", "Ressource", "Projet", "Affichage", "Format"]
    );
    dispatch(&mut harness, Action::SetLanguage(LanguagePref::En));
    harness.run();
    assert_eq!(
        tab_labels(&harness),
        ["Task", "Resource", "Project", "View", "Format"]
    );
}

/// H-UI-69 : onglet Tâche au premier lancement (groupe Planification
/// visible), puis le dernier choisi, relu au relancement.
#[test]
fn h_ui_69_tab_remembered() {
    let mut harness = p();
    let groups = |h: &Harness<'static, App>| -> Vec<String> {
        ribbon(h).groups.into_iter().map(|(name, _, _)| name).collect()
    };
    assert!(groups(&harness).contains(&fr(TextId::RibbonGroupSchedule)));
    tab(&harness, &fr(TextId::RibbonTabFormat)).click();
    harness.run();
    assert_eq!(harness.state().state.ribbon.tab, RibbonTab::Format);
    assert_eq!(groups(&harness).first(), Some(&fr(TextId::RibbonGroupColumns)));
    // Relancement : préférences sérialisées puis relues.
    let json = serde_json::to_string(&harness.state().state).unwrap();
    let state: UiState = serde_json::from_str(&json).unwrap();
    let mut app = ribbon_app_with(state);
    open_p_in(&mut app);
    let harness = sized(app, 1920.0, egui::os::OperatingSystem::Nix);
    assert_eq!(harness.state().state.ribbon.tab, RibbonTab::Format);
    assert_eq!(groups(&harness).first(), Some(&fr(TextId::RibbonGroupColumns)));
    assert_eq!(
        toggled(&tab(&harness, &fr(TextId::RibbonTabFormat))),
        Some(Toggled::True)
    );
}

/// H-UI-69 : ruban présent sur l'accueil, commandes du document grisées ;
/// préférences d'affichage actives.
#[test]
fn h_ui_69_ribbon_on_welcome_disabled() {
    let mut harness = welcome();
    assert_eq!(tab_labels(&harness).len(), 5);
    for id in [TextId::MenuPaste, TextId::RibbonInsertTask, TextId::RibbonLink] {
        assert!(disabled(&node(&harness, &fr(id))), "{id:?}");
    }
    assert!(disabled(&node(&harness, &fr(TextId::MenuSave))));
    dispatch(&mut harness, Action::Ribbon(RibbonOp::Select(RibbonTab::View)));
    assert!(!disabled(&node(&harness, &fr(TextId::MenuTheme))));
    assert!(!disabled(&node(&harness, &fr(TextId::RibbonTimeline))));
    assert!(disabled(&node(&harness, &fr(TextId::RibbonEntireProject))));
}

/// Libellés attendus d'un onglet : ceux de la table, dans l'ordre.
fn spec_labels(texts: &Texts, tab: RibbonTab) -> Vec<String> {
    tab.groups()
        .iter()
        .flat_map(|g| g.items.iter())
        .map(|item| label_of(texts, item))
        .collect()
}

/// H-UI-70 : chaque onglet montre les éléments du § 2.2 dans l'ordre (barre
/// d'accès rapide en tête), FR et EN.
#[test]
fn h_ui_70_ribbon_matches_spec() {
    let mut harness = p();
    for (language, pref) in [(Language::Fr, LanguagePref::Fr), (Language::En, LanguagePref::En)] {
        dispatch(&mut harness, Action::SetLanguage(pref));
        let texts = Texts::new(language);
        for (tab, _) in RIBBON {
            dispatch(&mut harness, Action::Ribbon(RibbonOp::Select(*tab)));
            let d = ribbon(&harness);
            let quick: Vec<String> = d
                .items
                .iter()
                .filter(|i| i.kind == Some(Kind::Quick))
                .map(|i| i.label.clone())
                .collect();
            let expected_quick: Vec<String> = QUICK_ACCESS.iter().map(|i| texts.get(i.label)).collect();
            assert_eq!(quick, expected_quick);
            let shown: Vec<String> = d
                .items
                .iter()
                .filter(|i| i.kind != Some(Kind::Quick))
                .map(|i| i.label.clone())
                .collect();
            assert_eq!(shown, spec_labels(&texts, *tab), "{language:?} {tab:?}");
            assert!(d.groups.iter().all(|(_, stage, _)| *stage == Stage::Full));
        }
    }
    // Quelques libellés du cahier, écrits en toutes lettres.
    dispatch(&mut harness, Action::SetLanguage(LanguagePref::Fr));
    dispatch(&mut harness, Action::Ribbon(RibbonOp::Select(RibbonTab::Task)));
    for label in [
        "Coller",
        "Lier",
        "Indenter",
        "Planifier manuellement",
        "Récapitulative",
        "Jalon",
        "50 %",
    ] {
        node(&harness, label);
    }
}

/// Commandes distinctes de la table (accès rapide compris).
fn all_cmds() -> Vec<RibbonCmd> {
    let mut seen = Vec::new();
    for item in RIBBON
        .iter()
        .flat_map(|(_, groups)| groups.iter())
        .flat_map(|g| g.items.iter())
        .chain(QUICK_ACCESS)
    {
        if !seen.contains(&item.cmd) {
            seen.push(item.cmd);
        }
    }
    seen
}

/// Libellé de l'entrée jumelle d'une commande.
fn twin_label(cmd: RibbonCmd) -> String {
    match cmd {
        RibbonCmd::MarkPercent(step) => Texts::new(Language::Fr).get_args(
            TextId::MenuMarkPercent,
            &[("percent", Arg::Count(percent_value(step)))],
        ),
        _ => fr(cmd.menu_label()),
    }
}

/// H-UI-70 : chaque commande du ruban a son entrée jumelle dans la barre de
/// menus, au chemin de [`RibbonCmd::menu_path`] (le menu dessine l'entrée
/// avec la même `RibbonCmd` : même action, même activation).
#[test]
fn h_ui_70_every_item_has_menu_twin() {
    let mut harness = p();
    select(&mut harness, 2, &[]);
    let cmds = all_cmds();
    assert!(cmds.len() > 50, "{}", cmds.len());
    for cmd in cmds {
        let label = twin_label(cmd);
        let before = named(&harness, &label).len();
        for step in cmd.menu_path() {
            let name = fr(*step);
            let entry = menu(&harness, &name);
            entry.hover();
            entry.click();
            harness.run();
        }
        let after = named(&harness, &label).len();
        assert!(
            after > before,
            "{cmd:?} : « {label} » absent du menu {:?}",
            cmd.menu_path()
        );
        harness.key_press(Key::Escape);
        harness.run();
        harness.key_press(Key::Escape);
        harness.run();
        assert!(
            !egui::Popup::is_any_open(&harness.ctx),
            "menu resté ouvert après {cmd:?}"
        );
    }
}

/// Prédécesseurs de `n`.
fn preds(app: &App, n: u32) -> Vec<(TaskUid, LinkType)> {
    project(app)
        .tasks
        .get(uid(n))
        .unwrap()
        .predecessors
        .iter()
        .map(|l| (l.predecessor, l.kind))
        .collect()
}

/// H-UI-70 : Lier {2, 4} par le ruban et par le menu Tâche ⇒ même lot
/// (même projet, une entrée d'annulation chacun).
#[test]
fn h_ui_70_link_same_batch_as_menu() {
    let mut harness = p();
    select(&mut harness, 2, &[4]);
    assert!(!preds(harness.state(), 4).contains(&(uid(2), LinkType::FinishToStart)));
    node(&harness, &fr(TextId::RibbonLink)).click();
    harness.run();
    let by_ribbon = project(harness.state()).clone();
    assert!(preds(harness.state(), 4).contains(&(uid(2), LinkType::FinishToStart)));
    assert_eq!(undoable(harness.state()), 1);
    dispatch(&mut harness, Action::Undo);
    assert!(!preds(harness.state(), 4).contains(&(uid(2), LinkType::FinishToStart)));
    menu(&harness, &fr(TextId::MenuTask)).click();
    harness.run();
    menu(&harness, &fr(TextId::StructureLink)).click();
    harness.run();
    assert_eq!(project(harness.state()), &by_ribbon);
    assert_eq!(undoable(harness.state()), 1);
}

/// H-UI-70 : sans sélection, « Lier » grisé dans le ruban et dans le menu
/// (même prédicat).
#[test]
fn h_ui_70_disabled_without_selection() {
    let mut harness = p();
    dispatch(&mut harness, Action::Select(None));
    assert!(disabled(&node(&harness, &fr(TextId::RibbonLink))));
    menu(&harness, &fr(TextId::MenuTask)).click();
    harness.run();
    assert!(disabled(&menu(&harness, &fr(TextId::StructureLink))));
    harness.key_press(Key::Escape);
    harness.run();
    select(&mut harness, 2, &[3]);
    assert!(!disabled(&node(&harness, &fr(TextId::RibbonLink))));
}

/// H-UI-70 : tâche 3 sélectionnée, Feuille des ressources puis onglet
/// Tâche ⇒ les commandes des tâches sont grisées (ruban et menu, même
/// prédicat) avec la raison « vue des tâches » ; un clic sur 50 % ne
/// modifie pas la tâche invisible. Retour au Gantt ⇒ actives.
#[test]
fn h_ui_70_task_cmds_disabled_in_resource_views() {
    let mut harness = p();
    select(&mut harness, 3, &[]);
    for view in [ViewKind::ResourceSheet, ViewKind::ResourceUsage] {
        dispatch(&mut harness, Action::SetView(view));
        dispatch(&mut harness, Action::Ribbon(RibbonOp::Select(RibbonTab::Task)));
        for id in [
            TextId::RibbonLink,
            TextId::RibbonIndent,
            TextId::RibbonInactivate,
            TextId::RibbonManualSchedule,
            TextId::TimelineAdd,
        ] {
            assert!(disabled(&node(&harness, &fr(id))), "{view:?} {id:?}");
        }
        assert!(disabled(&node(&harness, "50 %")), "{view:?}");
        node(&harness, "50 %").click();
        harness.run();
        let pct = project(harness.state())
            .tasks
            .get(uid(3))
            .unwrap()
            .progress
            .percent_complete
            .get();
        assert_eq!(pct, 0, "{view:?}");
        assert_eq!(undoable(harness.state()), 0);
        menu(&harness, &fr(TextId::MenuTask)).click();
        harness.run();
        assert!(disabled(&menu(&harness, &fr(TextId::StructureLink))), "{view:?}");
        harness.key_press(Key::Escape);
        harness.run();
    }
    dispatch(&mut harness, Action::SetView(ViewKind::Gantt));
    assert!(!disabled(&node(&harness, &fr(TextId::RibbonLink))));
}

/// Rectangle de l'élément de nom `label` dans le relevé.
fn item_rect(d: &RibbonDrawn, label: &str) -> Rect {
    d.items
        .iter()
        .find(|i| i.label == label)
        .unwrap_or_else(|| panic!("« {label} » non dessiné"))
        .rect
}

/// H-UI-71 : grand bouton ≥ 52 × 76 px ; petits boutons de 22 px de haut,
/// trois par colonne ; Insérer : « Tâche » grand, « Récapitulative » et
/// « Jalon » empilés à sa droite ; bande d'onglets de 26 px, corps de 92 px.
#[test]
fn h_ui_71_large_small_geometry() {
    let harness = p();
    let d = ribbon(&harness);
    assert!(
        (d.panel.height() - (TAB_BAND_H + 92.0)).abs() < 1.0,
        "{:?}",
        d.panel
    );
    let body = d.body.unwrap();
    assert!((body.height() - 92.0).abs() < 1.0);
    let task = item_rect(&d, &fr(TextId::RibbonInsertTask));
    assert!(task.width() >= 52.0 && task.height() >= 76.0, "{task:?}");
    let summary = item_rect(&d, &fr(TextId::RibbonInsertSummary));
    let milestone = item_rect(&d, &fr(TextId::RibbonInsertMilestone));
    for r in [summary, milestone] {
        assert!((r.height() - 22.0).abs() < 0.01, "{r:?}");
        assert!(r.left() >= task.right());
    }
    assert!((summary.left() - milestone.left()).abs() < 0.01);
    assert!(summary.bottom() <= milestone.top());
    // Trois par colonne : Indenter, Désindenter, Désactiver.
    let column: Vec<Rect> = [
        TextId::RibbonIndent,
        TextId::RibbonOutdent,
        TextId::RibbonInactivate,
    ]
    .map(|id| item_rect(&d, &fr(id)))
    .to_vec();
    for pair in column.windows(2) {
        assert!((pair[0].left() - pair[1].left()).abs() < 0.01);
        assert!(pair[0].bottom() <= pair[1].top());
    }
    for item in &d.items {
        if item.kind == Some(Kind::Large) {
            assert!(
                item.rect.width() >= 52.0 && item.rect.height() >= 76.0,
                "{item:?}"
            );
        }
    }
}

/// Clique deux fois au centre de `rect` dans la même image.
fn double_click(harness: &mut Harness<'static, App>, rect: Rect) {
    let p = rect.center();
    harness.event(Event::PointerMoved(p));
    for _ in 0..2 {
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
}

/// H-UI-72 : double-clic sur un onglet ⇒ seule la bande d'onglets reste
/// (la vue gagne 92 px) ; à nouveau ⇒ déplié.
#[test]
fn h_ui_72_double_click_collapses() {
    let mut harness = p();
    let rect = tab(&harness, &fr(TextId::RibbonTabTask)).rect();
    double_click(&mut harness, rect);
    assert!(harness.state().state.ribbon.collapsed);
    let d = ribbon(&harness);
    assert!((d.panel.height() - TAB_BAND_H).abs() < 1.0, "{:?}", d.panel);
    assert!(d.body.is_none());
    // Au-delà du délai du double-clic avant le suivant.
    for _ in 0..30 {
        harness.step();
    }
    double_click(&mut harness, rect);
    assert!(!harness.state().state.ribbon.collapsed);
    assert!(ribbon(&harness).body.is_some());
}

/// H-UI-72 : Ctrl+F1 replie le ruban dessiné, puis le déplie.
#[test]
fn h_ui_72_ctrl_f1_toggles() {
    let mut harness = p();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::F1);
    harness.run();
    assert!((ribbon(&harness).panel.height() - TAB_BAND_H).abs() < 1.0);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::F1);
    harness.run();
    assert!(ribbon(&harness).panel.height() > TAB_BAND_H + 90.0);
}

/// Nom accessible de la ligne de la tâche `n` de « P ».
fn row_rect(harness: &Harness<'static, App>, name: &str) -> Rect {
    harness.get_by_label_contains(name).rect()
}

/// H-UI-72 : replié, un clic sur « Projet » ouvre le corps en
/// surimpression sans déplacer la vue ; « Définir la référence » ouvre sa
/// fenêtre et referme la surimpression.
#[test]
fn h_ui_72_collapsed_click_opens_overlay() {
    let mut harness = p();
    dispatch(&mut harness, Action::Ribbon(RibbonOp::ToggleCollapsed));
    let row = row_rect(&harness, "Fondations");
    tab(&harness, &fr(TextId::RibbonTabProject)).click();
    harness.run();
    assert_eq!(harness.state().view().ribbon_overlay, Some(RibbonTab::Project));
    let d = ribbon(&harness);
    let body = d.body.unwrap();
    assert!(
        body.top() >= d.panel.bottom() - 1.0,
        "{body:?} sous {:?}",
        d.panel
    );
    assert_eq!(row_rect(&harness, "Fondations"), row, "la vue a bougé");
    node(&harness, &fr(TextId::RibbonSetBaseline)).click();
    harness.run();
    assert_eq!(harness.state().view().ribbon_overlay, None);
    let dialog = harness.state().view().dialog.as_ref().map(|d| d.kind);
    assert_eq!(dialog, Some(DialogKind::Baseline));
}

/// Ruban replié : ouvre l'onglet Affichage en surimpression.
#[track_caller]
fn open_view_overlay(harness: &mut Harness<'static, App>) {
    // Au-delà du délai du double-clic : deux clics rapprochés sur l'onglet
    // replieraient le ruban.
    for _ in 0..30 {
        harness.step();
    }
    tab(harness, &fr(TextId::RibbonTabView)).click();
    harness.run();
    assert_eq!(harness.state().view().ribbon_overlay, Some(RibbonTab::View));
}

/// H-UI-72 : la surimpression se referme par Échap et par un clic ailleurs.
#[test]
fn h_ui_72_overlay_closes_on_action_and_escape() {
    let mut harness = p();
    dispatch(&mut harness, Action::Ribbon(RibbonOp::ToggleCollapsed));
    open_view_overlay(&mut harness);
    harness.key_press(Key::Escape);
    harness.run();
    assert_eq!(harness.state().view().ribbon_overlay, None);
    open_view_overlay(&mut harness);
    let row = row_rect(&harness, "Peinture");
    harness.event(Event::PointerMoved(row.center()));
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: row.center(),
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run();
    assert_eq!(harness.state().view().ribbon_overlay, None);
    // Action du ruban : Zoom avant.
    open_view_overlay(&mut harness);
    let zoom = harness.state().state.zoom;
    node(&harness, &fr(TextId::RibbonZoomIn)).click();
    harness.run();
    assert_ne!(harness.state().state.zoom, zoom);
    assert_eq!(harness.state().view().ribbon_overlay, None);
}

/// H-UI-72 : repli gardé dans `UiState.ribbon`, relu au relancement.
#[test]
fn h_ui_72_collapsed_persisted() {
    let mut harness = p();
    dispatch(&mut harness, Action::Ribbon(RibbonOp::Select(RibbonTab::View)));
    node(&harness, &fr(TextId::RibbonCollapse)).click();
    harness.run();
    assert!(harness.state().state.ribbon.collapsed);
    let json = serde_json::to_string(&harness.state().state).unwrap();
    let state: UiState = serde_json::from_str(&json).unwrap();
    assert!(state.ribbon.collapsed);
    let mut app = ribbon_app_with(state);
    open_p_in(&mut app);
    let harness = sized(app, 1920.0, egui::os::OperatingSystem::Nix);
    assert!((ribbon(&harness).panel.height() - TAB_BAND_H).abs() < 1.0);
}

/// Survole `label` du ruban (infobulle immédiate).
fn hover(harness: &mut Harness<'static, App>, label: &str) {
    harness
        .ctx
        .global_style_mut(|style| style.interaction.tooltip_delay = 0.0);
    let center = node(harness, label).rect().center();
    harness.hover_at(center);
    harness.run();
    harness.step();
    harness.run();
}

/// Titre d'infobulle attendu : « libellé (raccourci) ».
fn titled(harness: &Harness<'static, App>, label: &str, binding: Binding) -> String {
    let shortcut = shortcut_for_os(binding, harness.ctx.os()).unwrap();
    format!("{label} ({})", harness.ctx.format_shortcut(shortcut))
}

/// H-UI-73 : info-bulle « Indenter (Alt+Shift+⏵) » et phrase d'aide ;
/// macOS simulé : glyphes de `format_shortcut`.
#[test]
fn h_ui_73_tooltip_shows_shortcut() {
    let indent = fr(TextId::RibbonIndent);
    let binding = Binding::Command("task.indent");
    let mut harness = p();
    select(&mut harness, 3, &[]);
    hover(&mut harness, &indent);
    let expected = titled(&harness, &indent, binding);
    assert!(expected.contains("Alt"), "{expected}");
    assert!(
        harness.query_by_label(&expected).is_some(),
        "infobulle absente : {expected}"
    );
    assert!(harness.query_by_label(&fr(TextId::RibbonTipIndent)).is_some());
    // macOS.
    let mut app = ribbon_app();
    open_p_in(&mut app);
    let mut mac = sized(app, 1920.0, egui::os::OperatingSystem::Mac);
    select(&mut mac, 3, &[]);
    hover(&mut mac, &indent);
    let expected_mac = titled(&mac, &indent, binding);
    assert_ne!(expected_mac, expected);
    assert!(
        mac.query_by_label(&expected_mac).is_some(),
        "infobulle absente : {expected_mac}"
    );
}

/// H-UI-73 : élément grisé ⇒ la raison (tri actif : « Retirez le tri ou le
/// filtre » de H-UI-32 ; sans document : ouvrir un projet).
#[test]
fn h_ui_73_disabled_reason() {
    let mut harness = p();
    select(&mut harness, 3, &[]);
    dispatch(
        &mut harness,
        Action::TaskView(TaskViewOp::SetSort(Some(SortSpec {
            key: SortKey::Name,
            descending: false,
            keep_outline: false,
        }))),
    );
    let indent = fr(TextId::RibbonIndent);
    assert!(disabled(&node(&harness, &indent)));
    hover(&mut harness, &indent);
    assert!(harness.query_by_label(&fr(TextId::ViewReorderLocked)).is_some());
    let mut harness = welcome();
    let paste = fr(TextId::MenuPaste);
    hover(&mut harness, &paste);
    assert!(
        harness
            .query_by_label(&fr(TextId::RibbonReasonNoDocument))
            .is_some()
    );
}

/// H-UI-74 : bascules selon la tâche active (6 manuelle ⇒ « Planifier
/// manuellement » enfoncé ; 7 automatique ⇒ relâché ; désactivée ⇒
/// « Désactiver » enfoncé) ; cases d'affichage selon `UiState`.
#[test]
fn h_ui_74_toggle_reflects_active_task() {
    let mut harness = p();
    let manual = fr(TextId::RibbonManualSchedule);
    let inactive = fr(TextId::RibbonInactivate);
    select(&mut harness, 6, &[]);
    assert_eq!(toggled(&node(&harness, &manual)), Some(Toggled::False));
    dispatch(&mut harness, Action::TaskCmd(TaskCmd::ToggleMode));
    assert!(matches!(
        project(harness.state()).tasks.get(uid(6)).unwrap().mode,
        SchedulingMode::Manual { .. }
    ));
    assert_eq!(toggled(&node(&harness, &manual)), Some(Toggled::True));
    select(&mut harness, 7, &[]);
    assert_eq!(toggled(&node(&harness, &manual)), Some(Toggled::False));
    node(&harness, &inactive).click();
    harness.run();
    assert!(!project(harness.state()).tasks.get(uid(7)).unwrap().active);
    assert_eq!(toggled(&node(&harness, &inactive)), Some(Toggled::True));
    // Sélection mixte : état de la tâche active (6, manuelle).
    select(&mut harness, 6, &[7]);
    assert_eq!(harness.state().selection, Some(uid(7)));
    // Cases d'affichage.
    dispatch(&mut harness, Action::Ribbon(RibbonOp::Select(RibbonTab::View)));
    let timeline = fr(TextId::RibbonTimeline);
    assert_eq!(toggled(&node(&harness, &timeline)), Some(Toggled::True));
    node(&harness, &timeline).click();
    harness.run();
    assert!(!harness.state().state.timeline_visible);
    assert_eq!(toggled(&node(&harness, &timeline)), Some(Toggled::False));
}

/// H-UI-75 : de 800 à 3 840 px, FR et EN, chaque onglet : aucun élément
/// hors du corps, aucun chevauchement ; réduit seulement quand il le faut.
#[test]
fn h_ui_75_no_overflow_widths() {
    for width in [800.0, 1024.0, 1440.0, 1920.0, 3840.0] {
        let mut harness = p_at(width);
        for pref in [LanguagePref::Fr, LanguagePref::En] {
            dispatch(&mut harness, Action::SetLanguage(pref));
            for tab in RibbonTab::ALL {
                dispatch(&mut harness, Action::Ribbon(RibbonOp::Select(tab)));
                let d = ribbon(&harness);
                let body = d.body.unwrap();
                assert!(body.right() <= width + 0.5, "{width} {tab:?}");
                let items: Vec<_> = d.items.iter().filter(|i| i.kind != Some(Kind::Quick)).collect();
                assert_ne!(items.len(), 0);
                for (i, a) in items.iter().enumerate() {
                    assert!(
                        body.expand(0.5).contains_rect(a.rect),
                        "{width} {pref:?} {tab:?} {a:?} hors de {body:?}"
                    );
                    for b in items.iter().skip(i + 1) {
                        assert!(
                            !a.rect.shrink(0.5).intersects(b.rect.shrink(0.5)),
                            "{width} {pref:?} {tab:?} : {a:?} ∩ {b:?}"
                        );
                    }
                }
                for (_, _, g) in &d.groups {
                    assert!(body.expand(0.5).contains_rect(*g), "{width} {tab:?} groupe {g:?}");
                }
                if width >= 1920.0 {
                    assert!(
                        d.groups.iter().all(|(_, s, _)| *s == Stage::Full),
                        "{width} {tab:?}"
                    );
                }
            }
        }
    }
}

/// H-UI-75 : la réduction avance par rang avec un rang d'écart entre
/// « sans libellés » et « replié ». À 1 280 px, Planification, Insérer et
/// Presse-papiers gardent leurs libellés ; à 1 024 px (fenêtre réduite,
/// web), Presse-papiers et Mise en forme les gardent et Planification
/// n'est pas repliée (l'onglet entier demande ~1 540 px).
#[test]
fn h_ui_75_task_tab_keeps_labels_when_narrow() {
    for width in [1280.0, 1024.0] {
        let harness = p_at(width);
        let d = ribbon(&harness);
        let stage = |name: TextId| {
            d.groups
                .iter()
                .find(|(n, _, _)| *n == fr(name))
                .map(|(_, s, _)| *s)
                .unwrap()
        };
        assert_eq!(stage(TextId::RibbonGroupClipboard), Stage::Full, "{width}");
        assert_eq!(stage(TextId::RibbonGroupFormatting), Stage::Full, "{width}");
        assert_ne!(stage(TextId::RibbonGroupSchedule), Stage::Folded, "{width}");
        if width >= 1280.0 {
            assert_eq!(stage(TextId::RibbonGroupSchedule), Stage::Full, "{width}");
            assert_eq!(stage(TextId::RibbonGroupInsert), Stage::Full, "{width}");
        }
    }
}

/// H-UI-75 : onglet Tâche à 800 px ⇒ Édition et Propriétés (et, mesuré,
/// Insérer, Tâches, Planification) repliés en boutons ; « Propriétés ▾ » →
/// Informations ⇒ fenêtre ouverte.
#[test]
fn h_ui_75_collapsed_group_menu_runs_action() {
    let mut harness = p_at(800.0);
    select(&mut harness, 3, &[]);
    let d = ribbon(&harness);
    let stage = |name: TextId| {
        d.groups
            .iter()
            .find(|(n, _, _)| *n == fr(name))
            .map(|(_, s, _)| *s)
            .unwrap()
    };
    assert_eq!(stage(TextId::RibbonGroupEditing), Stage::Folded);
    assert_eq!(stage(TextId::RibbonGroupProperties), Stage::Folded);
    // Les groupes de gauche ne sont pas repliés : Affichage,
    // Presse-papiers, Mise en forme.
    assert_ne!(stage(TextId::RibbonGroupClipboard), Stage::Folded);
    node(&harness, &fr(TextId::RibbonGroupProperties)).click();
    harness.run();
    let info = named(&harness, &fr(TextId::RibbonInformation));
    assert_eq!(info.len(), 1);
    info[0].click();
    harness.run();
    let dialog = harness.state().view().dialog.as_ref().map(|d| d.kind);
    assert_eq!(dialog, Some(DialogKind::TaskInformation(uid(3))));
}

/// H-UI-76 : barre d'accès rapide (Enregistrer, Annuler, Rétablir), mêmes
/// prédicats que le menu Édition ; aucun lot annulable ⇒ Annuler grisé.
#[test]
fn h_ui_76_quick_access_undo_disabled() {
    let mut harness = p();
    let undo = fr(TextId::MenuUndo);
    let redo = fr(TextId::MenuRedo);
    assert!(disabled(&node(&harness, &undo)));
    assert!(disabled(&node(&harness, &redo)));
    assert!(!disabled(&node(&harness, &fr(TextId::MenuSave))));
    select(&mut harness, 2, &[]);
    dispatch(
        &mut harness,
        Action::TaskCmd(TaskCmd::MarkPercent(
            plannercraft_automation::helpers::PercentStep::P25,
        )),
    );
    assert!(!disabled(&node(&harness, &undo)));
    node(&harness, &undo).click();
    harness.run();
    assert_eq!(undoable(harness.state()), 0);
    assert!(!disabled(&node(&harness, &redo)));
}

/// Opacité la plus haute des formes d'icône (disques des bouts de trait)
/// peintes dans `rect`.
fn icon_alpha(harness: &Harness<'static, App>, rect: Rect) -> u8 {
    harness
        .output()
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            Shape::Circle(c) if rect.contains(c.center) => Some(c.fill.a()),
            _ => None,
        })
        .max()
        .unwrap_or_else(|| panic!("aucune icône dans {rect:?}"))
}

/// H-UI-77 : « Coller » grisé ⇒ icône atténuée ; actif ⇒ pleine couleur.
#[test]
fn h_ui_77_disabled_icon_dimmed() {
    let paste = fr(TextId::MenuPaste);
    let mut harness = welcome();
    harness.remove_cursor();
    harness.run();
    let rect = item_rect(&ribbon(&harness), &paste);
    let dim = icon_alpha(&harness, rect);
    let mut harness = p_at(1440.0);
    harness.remove_cursor();
    harness.run();
    let rect = item_rect(&ribbon(&harness), &paste);
    let full = icon_alpha(&harness, rect);
    assert!(dim < full, "grisé {dim} ≥ actif {full}");
}

/// R-252 par le ruban : « 50 % » sur la récapitulative 1 ⇒ feuilles 2 et 3
/// à 50 %, un lot.
#[test]
fn ribbon_percent_row_marks_50() {
    let mut harness = p();
    select(&mut harness, 1, &[]);
    node(&harness, "50 %").click();
    harness.run();
    for n in [2, 3] {
        let pct = project(harness.state())
            .tasks
            .get(uid(n))
            .unwrap()
            .progress
            .percent_complete
            .get();
        assert_eq!(pct, 5_000, "tâche {n}");
    }
    assert_eq!(undoable(harness.state()), 1);
}

/// H-UI-70 : listes déroulantes : mêmes entrées que le menu (Affichage →
/// Échelle de temps ▾ → Semaine).
#[test]
fn ribbon_dropdown_runs_menu_entry() {
    let mut harness = p();
    dispatch(&mut harness, Action::Ribbon(RibbonOp::Select(RibbonTab::View)));
    node(&harness, &fr(TextId::RibbonTimescale)).click();
    harness.run();
    let week = fr(plannercraft_ui_egui::TimeScaleZoom::Week.label());
    harness
        .query_all_by_label(&week)
        .max_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
        .unwrap()
        .click();
    harness.run();
    assert_eq!(
        harness.state().state.zoom,
        plannercraft_ui_egui::TimeScaleZoom::Week
    );
    // Format → Colonnes ▾ → Affichage du tableau : case « Durée » (seconde
    // passe UX, lot L6 : cases dans un sous-menu, COL-02).
    dispatch(&mut harness, Action::Ribbon(RibbonOp::Select(RibbonTab::Format)));
    node(&harness, &fr(TextId::RibbonColumns)).click();
    harness.run();
    let display_label = fr(TextId::MenuTableDisplay);
    let display = harness.get_by_label(&display_label);
    display.hover();
    display.click();
    harness.run();
    let column = plannercraft_ui_egui::column::Column::Duration;
    let visible = |h: &Harness<'static, App>| h.state().state.shown(column);
    let was = visible(&harness);
    harness
        .query_all_by_label(&fr(column.title()))
        .find(|n| n.accesskit_node().role() == Role::CheckBox)
        .unwrap()
        .click();
    harness.run();
    assert_ne!(visible(&harness), was);
}

// --- Passe UX (ADR-0018 D10, lot L5) : groupe Édition ---

/// Appui de sélection sur la cellule (`n`, `column`).
fn press_cell(harness: &mut Harness<'static, App>, n: u32, column: Column, mode: PressMode) {
    dispatch(
        harness,
        Action::SelectOp(SelectOp::Press {
            row: RowRef::Item(uid(n)),
            column,
            target: PressTarget::Cell,
            mode,
        }),
    );
}

/// Durée de `n`, en jours de 8 h.
fn days(app: &App, n: u32) -> i64 {
    project(app).tasks.get(uid(n)).unwrap().duration.minutes().get() / 480
}

/// Ouvre la liste `list` du groupe Édition puis choisit son entrée `entry`.
fn pick(harness: &mut Harness<'static, App>, list: TextId, entry: TextId) {
    node(harness, &fr(list)).click();
    harness.run();
    let entries = named(harness, &fr(entry));
    assert_eq!(entries.len(), 1, "{entry:?} absent de la liste {list:?}");
    entries[0].click();
    harness.run();
}

/// Groupe Édition de l'onglet Tâche (ADR-0018 D10) : Rechercher, Défiler
/// jusqu'à la tâche, Remplir ▾, Effacer ▾ ; les deux listes portent les
/// raccourcis de leur commande (Ctrl+D, Ctrl+Suppr) et leurs icônes
/// Lucide (`arrow-down-to-line`, `eraser`).
#[test]
fn h_ui_70_ribbon_editing_group() {
    let group = RibbonTab::Task
        .groups()
        .iter()
        .find(|g| g.label == TextId::RibbonGroupEditing)
        .unwrap();
    let cmds: Vec<RibbonCmd> = group.items.iter().map(|i| i.cmd).collect();
    assert_eq!(
        cmds,
        [
            RibbonCmd::Find,
            RibbonCmd::ScrollToTask,
            RibbonCmd::FillDown,
            RibbonCmd::ClearContents
        ]
    );
    for item in &group.items[2..] {
        assert_eq!(item.size, plannercraft_ui_egui::ribbon::spec::ItemSize::Dropdown);
        assert_eq!(item.binding, item.cmd.binding(), "{:?}", item.cmd);
        assert!(item.cmd.dropdown_entry().is_some());
    }
    assert_eq!(
        group.items[2].icon.map(plannercraft_ui_egui::ribbon::Icon::name),
        Some("arrow-down-to-line")
    );
    assert_eq!(
        group.items[3].icon.map(plannercraft_ui_egui::ribbon::Icon::name),
        Some("eraser")
    );
    let harness = p();
    for label in ["Remplir", "Effacer"] {
        node(&harness, label);
    }
}

/// H-UI-96 par le ruban : bloc Durée 2-3, Remplir ▾ → Vers le bas ⇒ 3
/// dure 5 j, une entrée d'annulation ; même effet que Ctrl+D.
#[test]
fn h_ui_96_ribbon_fill_down() {
    let mut harness = p();
    press_cell(&mut harness, 2, Column::Duration, PressMode::Replace);
    press_cell(&mut harness, 3, Column::Duration, PressMode::Extend);
    assert_eq!(days(harness.state(), 3), 10);
    pick(&mut harness, TextId::RibbonFill, TextId::RibbonFillDown);
    assert_eq!(days(harness.state(), 3), 5);
    assert_eq!(undoable(harness.state()), 1);
    assert!(!egui::Popup::is_any_open(&harness.ctx));
}

/// H-UI-97 par le ruban : (3, Prédécesseurs), Effacer ▾ → Contenu ⇒ lien
/// 2→3 retiré, la tâche 3 reste ; une entrée d'annulation.
#[test]
fn h_ui_97_ribbon_clear_contents() {
    let mut harness = p();
    press_cell(&mut harness, 3, Column::Predecessors, PressMode::Replace);
    let tasks = project(harness.state()).tasks.len();
    assert!(preds(harness.state(), 3).contains(&(uid(2), LinkType::FinishToStart)));
    pick(&mut harness, TextId::RibbonClear, TextId::RibbonClearContents);
    assert_eq!(preds(harness.state(), 3), Vec::new());
    assert_eq!(project(harness.state()).tasks.len(), tasks);
    assert_eq!(undoable(harness.state()), 1);
}

/// Sans document, Remplir ▾ et Effacer ▾ sont grisés (prédicat de leur
/// commande, comme le menu Édition).
#[test]
fn h_ui_73_editing_lists_disabled_on_welcome() {
    let harness = welcome();
    for id in [TextId::RibbonFill, TextId::RibbonClear] {
        assert!(disabled(&node(&harness, &fr(id))), "{id:?}");
    }
}
