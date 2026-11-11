//! Volet de l'assistant toujours à portée, jamais gênant (ADR-0024, lot
//! A5, H-UI-161 à H-UI-166) : bouton du ruban, Ctrl/Cmd+J, Échap,
//! pastille, réponse qui continue volet fermé, largeur tirée et
//! mémorisée, volet par-dessus la vue sur fenêtre étroite, onglet par
//! défaut. Lot R1c (H-UI-190, H-UI-191) : volet rétractable, jamais
//! fermé (rail du volet réduit, croix « Réduire le volet »).

#![allow(
    clippy::arithmetic_side_effects,
    reason = "tests : écarts et compteurs bornés par le scénario"
)]

use std::sync::Arc;

use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use plannercraft_assistant::testing::{ScriptedTransport, sse_response, text_chunks, tool_call_chunks};
use plannercraft_ui_egui::action::{Action, DialogKind};
use plannercraft_ui_egui::assistant::AssistantServices;
use plannercraft_ui_egui::i18n::TextId;
use plannercraft_ui_egui::keymap::KEYMAP;
use plannercraft_ui_egui::ribbon::{RibbonOp, RibbonTab};
use plannercraft_ui_egui::state::{SIDE_WIDTH_DEFAULT, SIDE_WIDTH_MAX, SIDE_WIDTH_MIN};
use plannercraft_ui_egui::view::side_panel::{
    RAIL_WIDTH, SideOp, SideTab, overlays, panel_width, rail_drawn, rail_outer_width,
};
use plannercraft_ui_egui::{App, UiState};
use serde_json::json;

use crate::assistant::{ask, assistant_harness, enable_with_key, settle, wait_key_loaded};
use crate::panes::{double_click, drag};
use crate::{fr, harness, open_demo, ribbon_click};

/// Rectangle du bouton de l'assistant et de sa pastille à la dernière
/// image.
fn button(harness: &Harness<'static, App>) -> (Rect, Option<Rect>) {
    plannercraft_ui_egui::view::ribbon::drawn(&harness.ctx)
        .assistant
        .expect("bouton de l'assistant dessiné")
}

/// Contour « réponse en cours » dessiné autour du bouton (H-UI-166).
fn ring(harness: &Harness<'static, App>) -> bool {
    plannercraft_ui_egui::view::ribbon::drawn(&harness.ctx).assistant_ring
}

/// Clic (appui, relâcher) en `p`.
fn click_at(harness: &mut Harness<'static, App>, p: Pos2) {
    harness.hover_at(p);
    harness.step();
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        harness.step();
    }
    harness.run();
}

/// Clique le bouton de l'assistant.
fn press_button(harness: &mut Harness<'static, App>) {
    let rect = button(harness).0;
    click_at(harness, rect.center());
    harness.remove_cursor();
    harness.run();
}

/// Nœud accessible du bouton (rôle bouton, au rectangle relevé).
fn button_selected(harness: &Harness<'static, App>) -> bool {
    let rect = button(harness).0;
    let name = fr(TextId::AssistantButton);
    let node = harness
        .query_all_by_label(&name)
        .chain(harness.query_all_by(|n| {
            n.label()
                .is_some_and(|l| l.starts_with(&format!("{} (", fr(TextId::AssistantButton))))
        }))
        .find(|n| {
            n.accesskit_node().role() == Role::Button && n.rect().center().distance(rect.center()) < 0.5
        })
        .expect("nœud du bouton de l'assistant");
    node.accesskit_node().toggled() == Some(egui::accesskit::Toggled::True)
}

fn cmd_j(harness: &mut Harness<'static, App>) {
    harness.key_press_modifiers(Modifiers::COMMAND, Key::J);
    harness.run();
}

fn open(harness: &Harness<'static, App>) -> bool {
    harness.state().side_panel().open
}

fn question_focused(harness: &Harness<'static, App>) -> bool {
    let id = plannercraft_ui_egui::view::assistant::question_id();
    harness.ctx.memory(|m| m.has_focus(id))
}

/// Vérification de l'entrée `i` de `KEYMAP` (Ctrl/Cmd+J,
/// `session::ui_keymap_each_shortcut_dispatches_*`) : sans document
/// (contexte global), ouvre le volet sur l'Assistant puis le referme.
pub(crate) fn check_keymap_entry(i: usize) {
    let (_, shortcut, _) = KEYMAP[i];
    let mut harness = harness();
    harness.run();
    assert!(!open(&harness));
    harness.key_press_modifiers(shortcut.modifiers, shortcut.logical_key);
    harness.run();
    assert!(open(&harness));
    assert_eq!(harness.state().side_panel().tab, SideTab::Assistant);
    harness.key_press_modifiers(shortcut.modifiers, shortcut.logical_key);
    harness.run();
    assert!(!open(&harness));
}

/// H-UI-161 : bouton en haut à droite de la bande d'onglets, présent sur
/// l'accueil sans document, ruban replié compris ; il bascule le volet
/// (onglet Assistant), enfoncé quand le volet est ouvert ; jumeaux :
/// Affichage → Assistant IA (menu et ruban), même état coché.
#[test]
fn h_ui_161_button_menu_ribbon_toggle() {
    let mut harness = harness();
    harness.run();
    // Accueil sans document : bouton au bord droit de la bande d'onglets.
    let drawn = plannercraft_ui_egui::view::ribbon::drawn(&harness.ctx);
    let (rect, dot) = button(&harness);
    assert!(dot.is_none());
    // Lot U2 (H-UI-173) : au bord droit de la barre du haut.
    assert!(
        rect.right() <= drawn.bar.right() && rect.right() > drawn.bar.right() - 16.0,
        "{rect:?}"
    );
    assert!(
        rect.top() >= drawn.bar.top() && rect.bottom() <= drawn.bar.bottom(),
        "{rect:?}"
    );
    assert!(!button_selected(&harness));
    press_button(&mut harness);
    assert!(open(&harness));
    assert_eq!(harness.state().side_panel().tab, SideTab::Assistant);
    assert!(button_selected(&harness));
    press_button(&mut harness);
    assert!(!open(&harness));
    // Ruban replié : le bouton reste.
    harness
        .state_mut()
        .dispatch(Action::Ribbon(RibbonOp::ToggleCollapsed));
    harness.run();
    assert!(harness.state().state.ribbon.collapsed());
    press_button(&mut harness);
    assert!(open(&harness));
    press_button(&mut harness);
    assert!(!open(&harness));
    harness
        .state_mut()
        .dispatch(Action::Ribbon(RibbonOp::ToggleCollapsed));
    harness.run();
    // Document ouvert : ruban Affichage → Assistant IA, même bascule.
    open_demo(&mut harness);
    ribbon_click(&mut harness, RibbonTab::View, &fr(TextId::MenuAssistant));
    assert!(open(&harness));
    assert_eq!(harness.state().side_panel().tab, SideTab::Assistant);
    assert!(button_selected(&harness));
    // Depuis un autre onglet : la bascule referme le volet.
    harness
        .state_mut()
        .dispatch(Action::SidePanel(SideOp::Open(SideTab::AiOperations)));
    harness.run();
    ribbon_click(&mut harness, RibbonTab::View, &fr(TextId::MenuAssistant));
    assert!(!open(&harness));
    // Menu Affichage → Assistant IA (case, cochée volet ouvert) ; lot U2 :
    // dans le bouton « Menu » de la barre du haut.
    let menu = fr(TextId::MenuView);
    crate::open_menu_for(&mut harness, &menu);
    harness
        .query_all_by_label(&menu)
        .filter(|n| n.accesskit_node().role() != Role::Tab && !crate::in_ribbon(&harness, n))
        .min_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
        .expect("menu Affichage")
        .click();
    harness.run();
    let entry = fr(TextId::MenuAssistant);
    crate::click(&mut harness, &entry);
    assert!(open(&harness));
    assert_eq!(harness.state().side_panel().tab, SideTab::Assistant);
}

/// H-UI-162 : Ctrl/Cmd+J bascule le volet et met le focus dans la saisie
/// à l'ouverture, saisie de texte comprise (la question le referme).
#[test]
fn h_ui_162_cmd_j_toggles_and_focuses() {
    let transport = Arc::new(ScriptedTransport::new(vec![]));
    let mut harness = assistant_harness(transport);
    enable_with_key(&mut harness);
    cmd_j(&mut harness);
    assert!(!open(&harness));
    cmd_j(&mut harness);
    assert!(open(&harness));
    harness.run();
    assert!(question_focused(&harness), "focus à la saisie à l'ouverture");
    // Saisie en cours : Ctrl/Cmd+J referme quand même.
    harness.event(Event::Text("brouillon".to_owned()));
    harness.run();
    assert_eq!(harness.state().assistant().draft, "brouillon");
    cmd_j(&mut harness);
    assert!(!open(&harness));
    assert_eq!(harness.state().assistant().draft, "brouillon", "brouillon gardé");
}

/// H-UI-162 : Échap ferme le volet seulement si la saisie est vide,
/// qu'aucune réponse n'est en cours et qu'aucune fenêtre n'est ouverte ;
/// sinon comportement d'avant (réponse arrêtée, fenêtre fermée).
#[test]
fn h_ui_162_escape_three_conditions() {
    let transport = Arc::new(ScriptedTransport::manual(vec![sse_response(&text_chunks(
        "un deux trois",
        0.0,
    ))]));
    let mut harness = assistant_harness(Arc::clone(&transport));
    enable_with_key(&mut harness);
    // Saisie non vide : Échap ne ferme pas.
    cmd_j(&mut harness);
    cmd_j(&mut harness);
    assert!(question_focused(&harness));
    harness.event(Event::Text("question".to_owned()));
    harness.run();
    harness.key_press(Key::Escape);
    harness.run();
    assert!(open(&harness), "saisie non vide : volet gardé");
    // Réponse en cours : Échap l'arrête, le volet reste.
    ask(&mut harness, "compte");
    assert!(harness.state().assistant().conversation.busy());
    harness
        .get_by_role_and_label(Role::MultilineTextInput, &fr(TextId::AssistantPlaceholder))
        .focus();
    harness.run();
    harness.key_press(Key::Escape);
    harness.run();
    assert!(!harness.state().assistant().conversation.busy());
    assert!(open(&harness), "réponse arrêtée, volet gardé");
    // Fenêtre ouverte : Échap la ferme, le volet reste.
    harness
        .state_mut()
        .dispatch(Action::ShowDialog(DialogKind::ProjectInformation));
    harness.run();
    assert!(harness.state().view().dialog.is_some());
    harness.key_press(Key::Escape);
    harness.run();
    assert!(harness.state().view().dialog.is_none());
    assert!(open(&harness), "fenêtre fermée, volet gardé");
    // Saisie vide, rien en cours, aucune fenêtre : Échap ferme.
    assert_eq!(harness.state().assistant().draft, "");
    harness
        .get_by_role_and_label(Role::MultilineTextInput, &fr(TextId::AssistantPlaceholder))
        .focus();
    harness.run();
    harness.key_press(Key::Escape);
    harness.run();
    assert!(!open(&harness));
    assert!(!harness.state().state.side_panel.open, "fermé mémorisé");
}

/// H-UI-166 : réponse terminée volet fermé ⇒ pastille ; elle continue
/// en arrière-plan (la boucle tourne dans `App`, pas dans la vue) ;
/// ouverture ⇒ pastille effacée.
#[test]
fn h_ui_166_answer_continues_closed_then_badge() {
    let transport = Arc::new(ScriptedTransport::manual(vec![sse_response(&text_chunks(
        "un deux trois quatre",
        0.0,
    ))]));
    let mut harness = assistant_harness(Arc::clone(&transport));
    enable_with_key(&mut harness);
    ask(&mut harness, "compte");
    assert!(harness.state().assistant().conversation.busy());
    // Volet ouvert : la réponse s'écrit sous les yeux, le bouton reste fixe.
    assert!(!ring(&harness), "pas de contour volet ouvert");
    // Fermé pendant la réponse : rien n'est interrompu, pas de pastille
    // tant qu'elle court ; contour du bouton (« réponse en cours »).
    press_button(&mut harness);
    assert!(!open(&harness));
    assert!(harness.state().assistant().conversation.busy());
    assert_eq!(harness.state().assistant_badge(), 0);
    assert!(button(&harness).1.is_none());
    assert!(ring(&harness), "contour pendant la réponse, volet fermé");
    transport.step(40);
    settle(&mut harness);
    harness.run();
    assert_eq!(
        harness.state().assistant().conversation.turns()[0].answer,
        "un deux trois quatre"
    );
    assert_eq!(harness.state().assistant_badge(), 1);
    assert!(!ring(&harness), "plus de contour, réponse terminée");
    let (rect, dot) = button(&harness);
    let dot = dot.expect("pastille dessinée");
    assert!(rect.intersects(dot), "{rect:?} {dot:?}");
    let label = format!("{} (", fr(TextId::AssistantButton));
    assert!(
        harness
            .query_all_by(|n| n.label().is_some_and(|l| l.starts_with(&label)))
            .next()
            .is_some(),
        "nouveauté dite au lecteur d'écran"
    );
    // Ouverture : pastille effacée.
    cmd_j(&mut harness);
    assert!(open(&harness));
    assert_eq!(harness.state().assistant_badge(), 0);
    assert!(button(&harness).1.is_none());
}

/// H-UI-166 : proposition arrivée volet fermé ⇒ pastille (avec la fin de
/// la réponse : deux nouveautés, nombre affiché) ; l'onglet ne bascule
/// pas sur Opérations IA (la carte suffit, H-UI-165).
#[test]
fn h_ui_166_proposal_badge_no_tab_switch() {
    let args = json!({ "label": "Renommer", "ops": [{ "op": "task.update", "uid": 3, "name": "Plans" }] });
    let transport = Arc::new(ScriptedTransport::manual(vec![
        sse_response(&tool_call_chunks(
            "p1",
            plannercraft_assistant::proposal::PROPOSE_TOOL,
            &args.to_string(),
        )),
        sse_response(&text_chunks("Je propose de renommer.", 0.0)),
    ]));
    let mut harness = assistant_harness(Arc::clone(&transport));
    enable_with_key(&mut harness);
    ask(&mut harness, "Renomme la tâche 3");
    harness.state_mut().dispatch(Action::SidePanel(SideOp::Close));
    harness.run();
    // Deux réponses du modèle (proposition, puis texte) : pas à pas.
    for _ in 0..20 {
        transport.step(50);
        harness.run();
        if !harness.state().assistant().conversation.busy() {
            break;
        }
    }
    settle(&mut harness);
    harness.run();
    assert_eq!(harness.state().approvals().pending().len(), 1);
    assert_eq!(harness.state().assistant_badge(), 2);
    assert!(!open(&harness));
    let dot = button(&harness).1.expect("pastille");
    assert!(dot.width() > dot.height() - 0.5, "pastille à nombre : {dot:?}");
    press_button(&mut harness);
    assert_eq!(harness.state().side_panel().tab, SideTab::Assistant);
    assert_eq!(harness.state().assistant_badge(), 0);
}

/// H-UI-165 : le volet s'ouvre sur l'Assistant (activé ou non) ; une
/// demande d'agent MCP l'ouvre sur Opérations IA comme avant (relais,
/// `relay.rs`, inchangé) ; Opérations IA et Pourquoi cette date restent
/// des onglets.
#[test]
fn h_ui_165_default_tab_assistant() {
    let mut harness = harness();
    harness
        .state_mut()
        .set_assistant_services(AssistantServices::offline());
    open_demo(&mut harness);
    assert!(!harness.state().state.assistant.enabled);
    cmd_j(&mut harness);
    assert_eq!(harness.state().side_panel().tab, SideTab::Assistant);
    // Pas encore activé : mention et choix d'accès, pas de saisie.
    harness.get_by_label(&fr(TextId::AssistantDisclosure));
    // Onglets toujours là.
    for id in [TextId::SideTabAi, TextId::SideTabWhy, TextId::SideTabAssistant] {
        harness.get_by_label(&fr(id));
    }
    crate::click(&mut harness, &fr(TextId::SideTabAi));
    assert_eq!(harness.state().side_panel().tab, SideTab::AiOperations);
    // Fermé puis rouvert par la bascule : Assistant de nouveau.
    cmd_j(&mut harness);
    cmd_j(&mut harness);
    assert_eq!(harness.state().side_panel().tab, SideTab::Assistant);
    // Lancement avec le volet mémorisé ouvert : onglet Assistant.
    let mut state = crate::fr_light();
    state.side_panel.open = true;
    let mut again = crate::harness_for(App::new(state, plannercraft_ui_egui::Env::fixed(crate::now())));
    again.run();
    assert!(open(&again));
    assert_eq!(again.state().side_panel().tab, SideTab::Assistant);
    wait_key_loaded(&mut again);
}

/// Bord gauche du volet, lu sur le séparateur (nœud accessible ; lot U2,
/// H-UI-172 : la zone de saisie couvre la gouttière, bord gauche du volet).
fn edge_x(harness: &Harness<'static, App>) -> f32 {
    let label = fr(TextId::SideSplitterTip);
    let node = harness.get_by_role_and_label(Role::Splitter, &label);
    node.rect().left()
}

/// Point de saisie du séparateur : dans le volet, 3 points après le bord
/// (le bord exact est partagé avec la vue, dessinée après).
fn splitter_grip(harness: &Harness<'static, App>) -> Pos2 {
    let label = fr(TextId::SideSplitterTip);
    let y = harness
        .get_by_role_and_label(Role::Splitter, &label)
        .rect()
        .center()
        .y;
    pos2(edge_x(harness) + 3.0, y)
}

/// Bord droit de la zone de l'application (celui du ruban).
fn right_edge(harness: &Harness<'static, App>) -> f32 {
    plannercraft_ui_egui::view::ribbon::drawn(&harness.ctx)
        .panel
        .right()
}

/// Largeur affichée du volet : du bord gauche au bord droit.
fn shown_width(harness: &Harness<'static, App>) -> f32 {
    right_edge(harness) - edge_x(harness)
}

/// H-UI-163 : séparateur tirable (curseur, largeur suivie), bornes 320
/// points ≤ largeur ≤ 50 % de la fenêtre, double-clic ⇒ largeur par
/// défaut ; largeur et ouvert ou fermé mémorisés et relus au lancement.
#[test]
fn h_ui_163_width_drag_bounds_remembered() {
    let mut harness = sized(vec2(1280.0, 720.0));
    cmd_j(&mut harness);
    harness.run();
    assert!(harness.state().state.side_panel.open, "ouvert mémorisé");
    assert!(
        (shown_width(&harness) - 460.0).abs() < 0.5,
        "{}",
        shown_width(&harness)
    );
    let g = splitter_grip(&harness);
    harness.hover_at(g);
    harness.run();
    assert_eq!(
        harness.output().platform_output.cursor_icon,
        egui::CursorIcon::ResizeHorizontal
    );
    drag(&mut harness, g, g - vec2(100.0, 0.0));
    let w = harness.state().state.side_panel.width;
    assert!(
        (i32::from(w) - 560).abs() <= 1,
        "{w} ; affiché {}",
        shown_width(&harness)
    );
    assert!((shown_width(&harness) - f32::from(w)).abs() < 0.5);
    // Bornes : 50 % de 1 280 = 640, puis 320.
    let g = splitter_grip(&harness);
    drag(&mut harness, g, pos2(-200.0, g.y));
    assert_eq!(harness.state().state.side_panel.width, 640);
    let g = splitter_grip(&harness);
    drag(&mut harness, g, pos2(5_000.0, g.y));
    assert_eq!(harness.state().state.side_panel.width, SIDE_WIDTH_MIN);
    assert!(
        (shown_width(&harness) - 320.0).abs() < 0.5,
        "{}",
        shown_width(&harness)
    );
    // Double-clic : largeur par défaut.
    let g = splitter_grip(&harness);
    double_click(&mut harness, g);
    assert_eq!(harness.state().state.side_panel.width, SIDE_WIDTH_DEFAULT);
    for _ in 0..60 {
        harness.step();
    }
    // Valeur extrême : enregistrée bornée, affichée à 50 % au plus.
    harness
        .state_mut()
        .dispatch(Action::SidePanel(SideOp::SetWidth(u16::MAX)));
    harness.run();
    assert_eq!(harness.state().state.side_panel.width, SIDE_WIDTH_MAX);
    assert!((shown_width(&harness) - 640.0).abs() < 0.5);
    harness
        .state_mut()
        .dispatch(Action::SidePanel(SideOp::SetWidth(500)));
    harness.run();
    // Relance : préférences enregistrées puis relues.
    let json = harness.state().state.to_json().unwrap();
    let (read, warning) = UiState::from_json_lenient(&json);
    assert!(warning.is_none(), "{warning:?}");
    assert!(read.side_panel.open);
    assert_eq!(read.side_panel.width, 500);
    let mut again = crate::harness_for(App::new(read, plannercraft_ui_egui::Env::fixed(crate::now())));
    again.run();
    open_demo(&mut again);
    assert!(open(&again));
    assert!((shown_width(&again) - 500.0).abs() < 0.5);
    // Fermé : mémorisé fermé.
    cmd_j(&mut again);
    assert!(!again.state().state.side_panel.open);
    // Largeur de JSON hors bornes : bornée à la lecture.
    let (read, _) = UiState::from_json_lenient(r#"{"version":2,"side_panel":{"open":true,"width":3}}"#);
    assert_eq!(read.side_panel.width, SIDE_WIDTH_MIN);
}

/// H-UI-163 (règle 10) : largeur toujours finie, bornée, quelles que
/// soient les entrées.
#[test]
fn h_ui_163_panel_width_finite() {
    for wanted in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -5.0, 0.0, 1e9, 460.0] {
        for window in [f32::NAN, f32::INFINITY, -1.0, 0.0, 200.0, 800.0, 1280.0, 1e9] {
            let w = panel_width(wanted, window);
            assert!(w.is_finite(), "{wanted} {window}");
            assert!(w >= 0.0);
            if window.is_finite() && window >= 640.0 {
                assert!(w >= 320.0 && w <= window * 0.5 + 0.01, "{wanted} {window} ⇒ {w}");
            }
            if window.is_finite() && window > 0.0 {
                assert!(w <= window, "{wanted} {window} ⇒ {w}");
            }
        }
    }
    assert!((panel_width(f32::NAN, 1280.0) - 460.0).abs() < f32::EPSILON);
    assert!(!overlays(f32::NAN));
    assert!(!overlays(900.0));
    assert!(overlays(899.0));
}

/// Harnais `size`, pas de 1/60 s (double-clic), exemple ouvert.
fn sized(size: egui::Vec2) -> Harness<'static, App> {
    let mut harness = Harness::builder()
        .with_size(size)
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .with_os(egui::os::OperatingSystem::Nix)
        .with_step_dt(1.0 / 60.0)
        .with_max_steps(8)
        .build_ui_state(plannercraft_ui_egui::ui, crate::new_app());
    harness.run();
    open_demo(&mut harness);
    harness
}

/// Harnais 800 × 600, exemple ouvert.
fn narrow() -> Harness<'static, App> {
    sized(vec2(800.0, 600.0))
}

/// H-UI-164 : fenêtre de moins de 900 points ⇒ le volet s'ouvre
/// par-dessus la vue (la table garde sa largeur) ; au-delà, il la
/// rétrécit.
#[test]
fn h_ui_164_overlay_under_900() {
    let mut harness = narrow();
    let before = harness.state().view().panes.drawn.expect("vue").view;
    cmd_j(&mut harness);
    harness.run();
    assert!(open(&harness));
    let after = harness.state().view().panes.drawn.expect("vue").view;
    assert!(
        (before.width() - after.width()).abs() < 0.5,
        "{before:?} {after:?}"
    );
    // Le volet est bien là, à 400 points (50 % de 800) au plus.
    let w = shown_width(&harness);
    assert!((319.5..=400.5).contains(&w), "{w}");
    harness.get_by_label(&fr(TextId::SideTabAssistant));
    // Fenêtre large : la vue rétrécit de la largeur du volet.
    let mut wide = harness_wide();
    let before = wide.state().view().panes.drawn.expect("vue").view;
    cmd_j(&mut wide);
    wide.run();
    let after = wide.state().view().panes.drawn.expect("vue").view;
    assert!(before.width() - after.width() > 400.0, "{before:?} {after:?}");
}

fn harness_wide() -> Harness<'static, App> {
    let mut harness = harness();
    open_demo(&mut harness);
    harness
}

/// Captures : bouton et rail à pastille (volet réduit, réponse terminée ;
/// lot R1c) et volet par-dessus la vue à 800 points.
#[test]
fn snapshot_assistant_badge_fr_light() {
    use plannercraft_ui_egui::{LanguagePref, ThemePref};
    let transport = Arc::new(ScriptedTransport::new(vec![sse_response(&text_chunks(
        "Le projet compte 49 tâches.",
        0.001,
    ))]));
    let mut harness = crate::assistant::chat_capture_harness(transport, LanguagePref::Fr, ThemePref::Light);
    harness.run();
    crate::assistant::send_now(&mut harness, "Combien de tâches ?");
    harness.state_mut().dispatch(Action::SidePanel(SideOp::Close));
    for _ in 0..8 {
        harness.run();
    }
    assert!(!harness.state().assistant().conversation.busy());
    assert_eq!(harness.state().assistant_badge(), 1);
    // Lot R1c (H-UI-190) : la pastille est aussi sur le rail.
    assert!(rail(&harness).dot.is_some());
    crate::common::snapshot(&mut harness, "assistant_badge_fr_light");
}

#[test]
fn snapshot_assistant_overlay_fr_light() {
    use plannercraft_ui_egui::{LanguagePref, ThemePref};
    let app = crate::common::app(LanguagePref::Fr, ThemePref::Light);
    let mut harness = crate::common::harness_sized(app, ThemePref::Light, vec2(800.0, 600.0), 1.0);
    harness
        .state_mut()
        .set_assistant_services(AssistantServices::offline());
    harness.run();
    harness.state_mut().open_demo();
    harness.run();
    harness.state_mut().dispatch(Action::SidePanel(SideOp::Toggle));
    for _ in 0..4 {
        harness.run();
    }
    assert!(open(&harness));
    crate::common::snapshot(&mut harness, "assistant_overlay_fr_light");
}

/// Rail du volet réduit à la dernière image.
fn rail(harness: &Harness<'static, App>) -> plannercraft_ui_egui::view::side_panel::RailDrawn {
    rail_drawn(&harness.ctx).expect("rail dessiné")
}

/// Nœud accessible du rail : rôle bouton, nom « Déployer le volet
/// Assistant IA » (suivi des nouveautés entre parenthèses s'il y en a).
fn rail_label(harness: &Harness<'static, App>) -> String {
    let name = fr(TextId::SideRailExpand);
    let rect = rail(harness).rect;
    let node = harness
        .query_all_by(|n| n.label().is_some_and(|l| l.starts_with(&name)))
        .find(|n| {
            n.accesskit_node().role() == Role::Button && n.rect().center().distance(rect.center()) < 0.5
        })
        .expect("nœud du rail");
    node.accesskit_node().label().unwrap_or_default()
}

/// Bord droit de la vue (table et Gantt) à la dernière image.
fn view_right(harness: &Harness<'static, App>) -> f32 {
    harness.state().view().panes.drawn.expect("vue").view.right()
}

/// H-UI-190 : volet réduit ⇒ rail toujours visible (accueil sans
/// document, document ouvert, fenêtre de 800 points), au bord droit de la
/// zone sous le ruban, de la largeur du rail ; la vue s'arrête avant lui ;
/// un clic n'importe où sur le rail déploie le volet sur l'onglet
/// Assistant (même depuis un autre onglet mémorisé) ; déployé à côté de la
/// vue, plus de rail.
#[test]
fn h_ui_190_rail_always_visible_click_expands() {
    let mut harness = harness();
    harness.run();
    assert!(!open(&harness));
    // Accueil sans document.
    let r = rail(&harness).rect;
    assert!((r.width() - RAIL_WIDTH).abs() < 0.5, "{r:?}");
    assert!(r.right() <= right_edge(&harness) && r.right() > right_edge(&harness) - 16.0);
    let ribbon = plannercraft_ui_egui::view::ribbon::drawn(&harness.ctx).panel;
    assert!(r.top() >= ribbon.bottom() - 0.5, "{r:?} {ribbon:?}");
    assert_eq!(rail_label(&harness), fr(TextId::SideRailExpand));
    // Document ouvert : la vue s'arrête avant le rail.
    open_demo(&mut harness);
    let r = rail(&harness).rect;
    assert!(view_right(&harness) <= r.left(), "{} {r:?}", view_right(&harness));
    // Onglet mémorisé « Opérations IA », volet réduit : le rail déploie
    // sur l'Assistant, clic au bas du rail.
    harness
        .state_mut()
        .dispatch(Action::SidePanel(SideOp::Open(SideTab::AiOperations)));
    harness.run();
    harness.state_mut().dispatch(Action::SidePanel(SideOp::Close));
    harness.run();
    click_at(&mut harness, pos2(r.center().x, r.bottom() - 20.0));
    assert!(open(&harness));
    assert_eq!(harness.state().side_panel().tab, SideTab::Assistant);
    assert!(rail_drawn(&harness.ctx).is_none(), "déployé : plus de rail");
    // Fenêtre de 800 points : le rail reste, le volet déployé passe
    // par-dessus.
    let mut narrow = narrow();
    let r = rail(&narrow).rect;
    assert!((r.width() - RAIL_WIDTH).abs() < 0.5);
    let before = view_right(&narrow);
    assert!(before <= r.left());
    click_at(&mut narrow, r.center());
    assert!(open(&narrow));
    assert!(
        (view_right(&narrow) - before).abs() < 0.5,
        "la vue garde sa largeur"
    );
}

/// H-UI-191 : la tête du volet déployé porte une croix « Réduire le volet »
/// (rôle bouton) au bout de la rangée des onglets ; elle réduit le volet au
/// rail (il ne disparaît pas) ; Ctrl/Cmd+J, le menu et Échap basculent
/// toujours entre déployé et réduit, le rail présent dès que réduit.
#[test]
fn h_ui_191_cross_collapses_to_rail() {
    let mut harness = harness_wide();
    cmd_j(&mut harness);
    assert!(open(&harness));
    let name = fr(TextId::SideCollapse);
    let cross = harness.get_by_role_and_label(Role::Button, &name).rect();
    let tab = harness.get_by_label(&fr(TextId::SideTabAssistant)).rect();
    assert!(
        (cross.center().y - tab.center().y).abs() < 4.0,
        "même rangée que les onglets : {cross:?} {tab:?}"
    );
    assert!(cross.width() <= 24.0 && cross.height() <= 24.0, "{cross:?}");
    assert!(harness.query_by_label("Fermer le volet").is_none());
    click_at(&mut harness, cross.center());
    assert!(!open(&harness));
    assert!(!harness.state().state.side_panel.open, "réduit mémorisé");
    rail(&harness);
    // Ctrl/Cmd+J : déploie, puis réduit.
    cmd_j(&mut harness);
    assert!(open(&harness) && rail_drawn(&harness.ctx).is_none());
    cmd_j(&mut harness);
    assert!(!open(&harness));
    rail(&harness);
    // Menu Affichage → Assistant IA : déploie.
    let menu = fr(TextId::MenuView);
    crate::open_menu_for(&mut harness, &menu);
    harness
        .query_all_by_label(&menu)
        .filter(|n| n.accesskit_node().role() != Role::Tab && !crate::in_ribbon(&harness, n))
        .min_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
        .expect("menu Affichage")
        .click();
    harness.run();
    crate::click(&mut harness, &fr(TextId::MenuAssistant));
    assert!(open(&harness));
    // Échap (question vide, rien en cours) : réduit au rail.
    harness.key_press(Key::Escape);
    harness.run();
    assert!(!open(&harness));
    rail(&harness);
}

/// H-UI-190 (H-UI-166 sur le rail) : réponse en cours volet réduit ⇒
/// contour du rail ; terminée ⇒ pastille sur le rail, nouveauté dans son
/// nom accessible ; le bouton du ruban garde les siens.
#[test]
fn h_ui_190_rail_badge_and_ring() {
    let transport = Arc::new(ScriptedTransport::manual(vec![sse_response(&text_chunks(
        "un deux trois quatre",
        0.0,
    ))]));
    let mut harness = assistant_harness(Arc::clone(&transport));
    enable_with_key(&mut harness);
    ask(&mut harness, "compte");
    assert!(harness.state().assistant().conversation.busy());
    harness.state_mut().dispatch(Action::SidePanel(SideOp::Close));
    harness.run();
    assert!(rail(&harness).ring, "contour sur le rail pendant la réponse");
    assert!(ring(&harness), "et sur le bouton du ruban");
    assert!(rail(&harness).dot.is_none());
    transport.step(40);
    settle(&mut harness);
    harness.run();
    assert_eq!(harness.state().assistant_badge(), 1);
    let drawn = rail(&harness);
    assert!(!drawn.ring);
    let dot = drawn.dot.expect("pastille sur le rail");
    assert!(drawn.rect.intersects(dot), "{drawn:?}");
    assert!(button(&harness).1.is_some(), "pastille du ruban gardée");
    let label = rail_label(&harness);
    assert!(
        label.starts_with(&format!("{} (", fr(TextId::SideRailExpand))),
        "{label}"
    );
    // Déployé par le rail : pastille effacée.
    let r = drawn.rect;
    click_at(&mut harness, r.center());
    assert!(open(&harness));
    assert_eq!(harness.state().assistant_badge(), 0);
}

/// H-UI-190 (règle 10) : largeur du rail finie et bornée quelle que soit
/// la marge extérieure.
#[test]
fn h_ui_190_rail_width_finite() {
    for outer in [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        -5.0,
        0.0,
        4.0,
        8.0,
        1e9,
    ] {
        let w = rail_outer_width(outer);
        assert!(w.is_finite(), "{outer}");
        assert!((RAIL_WIDTH..=RAIL_WIDTH + 80.0).contains(&w), "{outer} ⇒ {w}");
    }
}

/// Captures du lot R1c : volet réduit au rail (sans pastille) et tête du
/// volet déployé avec la croix, en français, thème clair.
#[test]
fn snapshot_side_rail_fr_light() {
    use plannercraft_ui_egui::{LanguagePref, ThemePref};
    let app = crate::common::app(LanguagePref::Fr, ThemePref::Light);
    let mut harness = crate::common::harness_with(app, ThemePref::Light);
    harness
        .state_mut()
        .set_assistant_services(AssistantServices::offline());
    harness.run();
    harness.state_mut().open_demo();
    harness.run();
    rail(&harness);
    crate::common::snapshot(&mut harness, "side_rail_fr_light");
    harness.state_mut().dispatch(Action::SidePanel(SideOp::Toggle));
    for _ in 0..4 {
        harness.run();
    }
    assert!(open(&harness));
    crate::common::snapshot(&mut harness, "side_panel_cross_fr_light");
}
