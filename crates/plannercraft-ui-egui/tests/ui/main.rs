//! Tests d'interface `egui_kittest` **sans rendu** (aucun GPU) : parcours de
//! l'accueil à l'exemple, menus, ruban, langue, thème, zoom,
//! ouverture annulée, bandeau d'erreur, sélection et défilement synchronisé ;
//! session d'édition, annuler/rétablir et raccourcis (`session.rs`) ;
//! édition des cellules et ligne vide de fin (`edit.rs`) ;
//! Nouveau, Enregistrer, document modifié (`files.rs`) ; commandes de
//! structure sur la sélection (`structure.rs`). Modules J7 (`nav`, `relay`,
//! `columns`, `tracking`, `gestures`, `task_info`, `resource_sheet`,
//! `usage`, `dialogs`, `assign`, `desktop`, `recovery`, `e2e`) déclarés
//! par le contrat (lot L0), remplis par leur lot. Modules J8 (`row_view`,
//! `filters`, `search`, `outline`, `clipboard`, `csv_import`, `styled`,
//! `ai_panel`, `pseudo`, `web_keys`, `j8_e2e`) déclarés vides par le contrat
//! J8 (lot L0, ADR-0016), remplis par leur lot. Modules du ruban et de la
//! chronologie (`ribbon`, `ribbon_commands`, `timeline`) posés par le
//! contrat ADR-0017 (lot L0), remplis par L2, L3 et L4 ; scénario de bout
//! en bout du ruban (`ribbon_e2e`) ajouté par le lot L6. Modules de la
//! passe UX (`select_mouse`, `select_keys`, `cells`, `rows_columns`,
//! `context_menus`, `ux_e2e`) posés par le contrat ADR-0018 (lot L0),
//! remplis par L1 à L6. Modules de la seconde passe UX (`messages`,
//! `paste_destination`, `quit`, `panes`, `details`, `columns_insert`,
//! `fields`, `custom_fields`, `ux2_e2e`) posés vides par le contrat
//! ADR-0019 (lot L0), remplis par leur lot ; `ux2_contract` : touches,
//! actions et état posés par L0.
//!
//! Assistant IA (ADR-0024, lot A1) : `assistant` (activation, connexion
//! simulée, flux, arrêt, erreurs, modèle, capture).
//!
//! Cible unique `ui` (un seul binaire lié à wgpu, ADR-0011, décision 12).
//! Les libellés cherchés sont construits par `Texts` (jamais en dur : le
//! français contient des espaces insécables U+00A0 avant « : »). Le harnais
//! panique sur un glyphe absent des polices et sur un widget sans nom
//! accessible : ces pannes sont couvertes pour les textes **effectivement
//! rendus** par ces tests (accueil, vue principale de l'exemple, bandeau en
//! français, diagnostics d'un lien pendant), pas pour des noms de tâches non
//! latins ni pour des messages jamais affichés ici.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

mod ai_panel;
mod assign;
mod assistant;
mod cells;
mod clipboard;
mod columns;
mod columns_insert;
mod common;
mod context_menus;
mod csv_import;
mod custom_fields;
mod desktop;
mod details;
mod dialogs;
mod e2e;
mod edit;
mod fields;
mod files;
mod filters;
mod gestures;
mod group;
mod j8_e2e;
mod messages;
mod nav;
mod outline;
mod panes;
mod paste_destination;
mod polish;
mod pseudo;
mod quit;
mod recovery;
mod relay;
mod resource_sheet;
mod ribbon;
mod ribbon_commands;
mod ribbon_e2e;
mod row_view;
mod rows_columns;
mod search;
mod select_keys;
mod select_mouse;
mod session;
mod snapshots;
mod structure;
mod styled;
mod task_info;
mod text_style;
mod timeline;
mod tracking;
mod usage;
mod ux2_contract;
mod ux2_e2e;
mod ux_e2e;
mod visible;
mod web_keys;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::mpsc::Sender;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_core::Moment;
use plannercraft_ui_egui::document::{self, OpenError};
use plannercraft_ui_egui::i18n::{Arg, TextId, Texts};
use plannercraft_ui_egui::io::OpenEvent;
use plannercraft_ui_egui::ribbon::RibbonTab;
use plannercraft_ui_egui::timescale::{TimeAxis, zoom_anchor};
use plannercraft_ui_egui::{
    Action, App, DocOrigin, Env, FileService, Language, LanguagePref, ThemePref, TimeScaleZoom, UiState,
};

/// « Maintenant » fixe des tests (date d'état de l'exemple).
fn now() -> Moment {
    "2027-01-15T08:00".parse().unwrap()
}

/// Préférences de test : français, thème clair, modèle Classique (les
/// tests J7 lisent la palette de J7, `Palette::LIGHT` ; Sobre, défaut
/// depuis J8 lot L15, est vérifié par les captures et `visible.rs`).
fn fr_light() -> UiState {
    without_indicators(UiState {
        language: LanguagePref::Fr,
        theme: ThemePref::Light,
        default_template: plannercraft_render::style::TemplateId::Classique,
        ..UiState::default()
    })
}

/// État sans la colonne Indicateurs (finitions P1, H-UI-122), table
/// rétrécie d'autant : les tests d'avant le lot gardent la géométrie de
/// leurs colonnes (N° puis Nom) ; la colonne est vérifiée par `polish.rs`
/// et les captures.
fn without_indicators(mut state: UiState) -> UiState {
    use plannercraft_ui_egui::column::Column;
    if let Some(at) = state.columns.iter().position(|c| c.column == Column::Indicators) {
        let removed = state.columns.remove(at);
        state.table_width = state.table_width.saturating_sub(removed.width);
    }
    state
}

/// Application de test : français, thème clair, aucun sélecteur de fichiers.
fn new_app() -> App {
    App::new(fr_light(), Env::fixed(now()))
}

/// Harnais 1280×720, échelle 1, thème clair, sans rendu, pour `app`.
fn harness_for(app: App) -> Harness<'static, App> {
    Harness::builder()
        .with_size(egui::vec2(1280.0, 720.0))
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .with_os(egui::os::OperatingSystem::Nix)
        .with_max_steps(8)
        .build_ui_state(plannercraft_ui_egui::ui, app)
}

/// Harnais de l'application de test par défaut.
fn harness() -> Harness<'static, App> {
    harness_for(new_app())
}

/// Émetteurs gardés par les demandes d'ouverture sans réponse.
type Held = Rc<RefCell<Vec<Sender<OpenEvent>>>>;

/// Réponse scriptée d'un sélecteur de fichiers de test.
#[derive(Clone, Copy, Debug)]
enum Reply {
    /// Le fichier choisi est l'exemple : document chargé.
    Demo,
    /// La lecture échoue.
    ReadError,
    /// Le sélecteur ne répond jamais (web sans événement d'annulation) :
    /// l'émetteur est gardé pour répondre plus tard.
    Never,
}

/// Sélecteur de fichiers de test : répond selon le script, garde les
/// émetteurs des demandes sans réponse.
struct ScriptedFiles {
    replies: VecDeque<Reply>,
    held: Held,
}

impl FileService for ScriptedFiles {
    fn pick_and_open(
        &mut self,
        ctx: &egui::Context,
        now: Moment,
        _: String,
        _: String,
        tx: Sender<OpenEvent>,
    ) {
        let origin = DocOrigin {
            name: "maison.xml".to_owned(),
            path: None,
            demo: false,
        };
        let event = match self.replies.pop_front().unwrap_or(Reply::Never) {
            Reply::Demo => OpenEvent::Opened {
                origin,
                result: document::load(plannercraft_ui_egui::DEMO_MSPDI, now),
            },
            Reply::ReadError => OpenEvent::Opened {
                origin,
                result: Err(OpenError::Read("accès refusé".to_owned())),
            },
            Reply::Never => {
                self.held.borrow_mut().push(tx);
                return;
            }
        };
        tx.send(event).unwrap();
        ctx.request_repaint();
    }
}

/// Harnais dont le sélecteur suit `replies` ; rend aussi les émetteurs
/// gardés par les demandes sans réponse.
fn scripted(replies: &[Reply]) -> (Harness<'static, App>, Held) {
    let held = Rc::new(RefCell::new(Vec::new()));
    let files = ScriptedFiles {
        replies: replies.iter().copied().collect(),
        held: Rc::clone(&held),
    };
    let env = Env {
        files: Box::new(files),
        ..Env::fixed(now())
    };
    (harness_for(App::new(fr_light(), env)), held)
}

/// Événement d'ouverture réussie de l'exemple (pour un émetteur gardé).
fn demo_event() -> OpenEvent {
    OpenEvent::Opened {
        origin: DocOrigin::default(),
        result: document::load(plannercraft_ui_egui::DEMO_MSPDI, now()),
    }
}

/// Axe de la dernière image : projection écran du Gantt (bord gauche
/// relevé par la vue, défilement et zoom courants).
fn screen_axis(app: &App) -> TimeAxis {
    let doc = app.document.as_ref().unwrap();
    let view = app.view();
    TimeAxis::new(
        doc.range,
        app.state.zoom,
        view.last_gantt_left - view.gantt_scroll_x,
    )
}

/// Défilement horizontal maximal au zoom courant.
fn max_scroll(app: &App) -> f32 {
    let doc = app.document.as_ref().unwrap();
    let content = TimeAxis::new(doc.range, app.state.zoom, 0.0).content_width();
    (content - app.view().last_gantt_width).max(0.0)
}

/// Libellé français d'un message.
fn fr(id: TextId) -> String {
    Texts::new(Language::Fr).get(id)
}

/// Le nœud `node` est-il un élément du ruban : onglet (rôle `Tab` dans le
/// panneau du ruban), ou
/// rectangle d'un élément relevé par `view::ribbon::drawn` (barre d'accès
/// rapide, corps, surimpression) ? Un menu ouvert passe au-dessus du ruban :
/// la position seule ne suffit pas.
fn in_ribbon(harness: &Harness<'static, App>, node: &egui_kittest::Node<'_>) -> bool {
    use egui_kittest::kittest::NodeT as _;
    let drawn = plannercraft_ui_egui::view::ribbon::drawn(&harness.ctx);
    let rect = node.rect();
    if node.accesskit_node().role() == egui::accesskit::Role::Tab && drawn.panel.contains(rect.center()) {
        return true;
    }
    drawn
        .items
        .iter()
        .any(|item| item.rect.min.distance(rect.min) < 0.5 && item.rect.max.distance(rect.max) < 0.5)
}

/// Clique sur l'unique widget portant `label` **hors du ruban**, puis laisse
/// l'interface se stabiliser. Lot L5 : les onglets Tâche, Projet,
/// Affichage, Format et la barre d'accès rapide (« Enregistrer »,
/// « Annuler ») portent le nom d'un menu ou d'un bouton de confirmation ;
/// un élément du ruban se clique par [`ribbon_click`].
fn click(harness: &mut Harness<'static, App>, label: &str) {
    {
        let mut found = harness
            .query_all_by_label(label)
            .filter(|n| !in_ribbon(harness, n));
        let node = found
            .next()
            .unwrap_or_else(|| panic!("« {label} » absent hors du ruban"));
        assert!(found.next().is_none(), "« {label} » en double hors du ruban");
        node.click();
    }
    harness.run();
}

/// Un nœud nommé `label` existe-t-il hors du ruban (onglet « Notes » de
/// la fenêtre des informations, homonyme d'un élément du ruban) ?
fn shown_off_ribbon(harness: &Harness<'static, App>, label: &str) -> bool {
    harness.query_all_by_label(label).any(|n| !in_ribbon(harness, &n))
}

/// Comme [`click`], pour l'unique nœud hors du ruban dont le nom vérifie
/// `pred` (entrée de menu suivie de son raccourci).
fn click_where(harness: &mut Harness<'static, App>, pred: impl Fn(&str) -> bool) {
    {
        let mut found = harness
            .query_all_by(|node| node.label().is_some_and(|text| pred(&text)))
            .filter(|n| !in_ribbon(harness, n));
        let node = found.next().expect("aucun nœud hors du ruban");
        assert!(found.next().is_none(), "plusieurs nœuds hors du ruban");
        node.click();
    }
    harness.run();
}

/// Échelle de temps par le ruban (Affichage → Échelle de temps ▾ ; boutons
/// Jour à Trimestre de la barre d'outils jusqu'au lot L5).
fn ribbon_zoom(harness: &mut Harness<'static, App>, zoom: TimeScaleZoom) {
    ribbon_menu(
        harness,
        RibbonTab::View,
        &fr(TextId::RibbonTimescale),
        &fr(zoom.label()),
    );
}

/// Ouvre la liste `list` de l'onglet `tab` du ruban et choisit `entry`.
fn ribbon_menu(harness: &mut Harness<'static, App>, tab: RibbonTab, list: &str, entry: &str) {
    ribbon_click(harness, tab, list);
    click(harness, entry);
}

/// Clique sur l'élément `label` de l'onglet `tab` du ruban (lot L5 : les
/// tests J7 et J8 qui cliquaient l'ancienne barre d'outils passent par
/// lui) : onglet choisi par son nœud (rôle `Tab`), élément cherché dans le
/// rectangle du ruban (les menus fermés n'ont pas de nœud, mais l'écran
/// peut porter un homonyme ailleurs).
fn ribbon_click(harness: &mut Harness<'static, App>, tab: RibbonTab, label: &str) {
    use egui::accesskit::Role;
    use egui_kittest::kittest::NodeT as _;
    let tab_label = fr(match tab {
        RibbonTab::Task => TextId::RibbonTabTask,
        RibbonTab::Resource => TextId::RibbonTabResource,
        RibbonTab::Project => TextId::RibbonTabProject,
        RibbonTab::View => TextId::RibbonTabView,
        RibbonTab::Format => TextId::RibbonTabFormat,
    });
    if harness.state().state.ribbon.tab != tab {
        harness.get_by_role_and_label(Role::Tab, &tab_label).click();
        harness.run();
    }
    assert_eq!(harness.state().state.ribbon.tab, tab);
    let panel = plannercraft_ui_egui::view::ribbon::drawn(&harness.ctx).panel;
    {
        let mut found = harness
            .query_all_by_label(label)
            .filter(|n| n.accesskit_node().role() != Role::Tab && panel.contains(n.rect().center()));
        let node = found
            .next()
            .unwrap_or_else(|| panic!("« {label} » absent de l'onglet {tab:?} du ruban"));
        assert!(found.next().is_none(), "« {label} » en double dans le ruban");
        node.click();
    }
    harness.run();
}

/// Ouvre l'exemple depuis l'écran d'accueil.
fn open_demo(harness: &mut Harness<'static, App>) {
    click(harness, &fr(TextId::WelcomeOpenDemo));
    assert!(
        harness.state().document.is_some(),
        "exemple non ouvert : {:?}",
        harness.state().messages()
    );
}

#[test]
fn welcome_open_demo_loads_project() {
    let mut harness = harness();
    assert!(harness.state().document.is_none());
    assert!(harness.query_by_label(&fr(TextId::StatusNoDocument)).is_some());
    open_demo(&mut harness);
    let app = harness.state();
    let document = app.document.as_ref().unwrap();
    assert!(document.origin.demo);
    assert_eq!(document.origin.name, fr(TextId::DemoFileName));
    assert_eq!(document.opening, 1);
    assert_eq!(document.project().tasks.len(), 49);
    assert!(app.messages().is_empty(), "{:?}", app.messages());
    assert_eq!(app.last_frame.gantt_rows, app.last_frame.table_rows);
    // L'accueil a disparu ; la barre d'état affiche le nom de l'exemple.
    assert!(harness.query_by_label(&fr(TextId::WelcomeOpenDemo)).is_none());
    assert!(harness.query_by_label(&fr(TextId::DemoFileName)).is_some());
}

#[test]
fn ui_settles() {
    let mut harness = harness();
    assert!(
        harness.try_run().is_ok(),
        "l'accueil redemande des images sans fin"
    );
    open_demo(&mut harness);
    assert!(
        harness.try_run().is_ok(),
        "la vue principale redemande des images sans fin"
    );
}

#[test]
fn language_switch_relabels() {
    let mut harness = harness();
    let file_fr = fr(TextId::MenuFile);
    let file_en = Texts::new(Language::En).get(TextId::MenuFile);
    assert!(harness.query_by_label(&file_fr).is_some());
    harness
        .state_mut()
        .dispatch(Action::SetLanguage(LanguagePref::En));
    harness.run();
    assert_eq!(harness.state().texts().language(), Language::En);
    assert!(harness.query_by_label(&file_en).is_some(), "« File » absent");
    assert!(
        harness.query_by_label(&file_fr).is_none(),
        "« Fichier » encore présent"
    );
}

#[test]
fn language_switch_renames_demo_document() {
    let mut harness = harness();
    open_demo(&mut harness);
    // Ruban, Affichage → Langue ▾ → English (bouton de la barre d'outils
    // jusqu'au lot L5).
    ribbon_menu(
        &mut harness,
        RibbonTab::View,
        &fr(TextId::MenuLanguage),
        &fr(TextId::LangEn),
    );
    let app = harness.state();
    assert_eq!(app.state.language, LanguagePref::En);
    let name = &app.document.as_ref().unwrap().origin.name;
    assert_eq!(name, &Texts::new(Language::En).get(TextId::DemoFileName));
}

#[test]
fn zoom_buttons_change_zoom() {
    let mut harness = harness();
    open_demo(&mut harness);
    for zoom in [
        TimeScaleZoom::Day,
        TimeScaleZoom::Week,
        TimeScaleZoom::Month,
        TimeScaleZoom::Quarter,
    ] {
        ribbon_zoom(&mut harness, zoom);
        assert_eq!(harness.state().state.zoom, zoom);
    }
}

#[test]
fn zoom_step_and_horizontal_scroll_are_bounded() {
    let mut harness = harness();
    open_demo(&mut harness);
    ribbon_zoom(&mut harness, TimeScaleZoom::Week);
    harness.state_mut().dispatch(Action::ScrollGanttX(-1.0e6));
    harness.run();
    assert!(harness.state().view().gantt_scroll_x.abs() < f32::EPSILON);
    harness.state_mut().dispatch(Action::ScrollGanttX(1.0e9));
    harness.run();
    let far = harness.state().view().gantt_scroll_x;
    assert!(far.is_finite() && far >= 0.0, "défilement non borné : {far}");
    // Borne haute atteinte exactement : contenu moins largeur du Gantt.
    let max = max_scroll(harness.state());
    assert!(max > 0.0);
    assert!((far - max).abs() <= 1.0, "défilement {far}, maximum {max}");
    harness.state_mut().dispatch(Action::ZoomStep {
        steps: 1,
        anchor_x: 300.0,
    });
    harness.run();
    assert_eq!(harness.state().state.zoom, TimeScaleZoom::Day);
    harness.state_mut().dispatch(Action::ZoomStep {
        steps: -10,
        anchor_x: f32::NAN,
    });
    harness.run();
    assert_eq!(harness.state().state.zoom, TimeScaleZoom::Quarter);
    let scroll = harness.state().view().gantt_scroll_x;
    assert!(scroll.is_finite() && scroll >= 0.0);
}

#[test]
fn theme_switch_changes_ctx_theme() {
    let mut harness = harness();
    open_demo(&mut harness);
    assert_eq!(harness.ctx.theme(), egui::Theme::Light);
    ribbon_menu(
        &mut harness,
        RibbonTab::View,
        &fr(TextId::MenuTheme),
        &fr(TextId::ThemeDark),
    );
    assert_eq!(harness.state().state.theme, ThemePref::Dark);
    assert_eq!(harness.ctx.theme(), egui::Theme::Dark);
    ribbon_menu(
        &mut harness,
        RibbonTab::View,
        &fr(TextId::MenuTheme),
        &fr(TextId::ThemeLight),
    );
    assert_eq!(harness.ctx.theme(), egui::Theme::Light);
}

#[test]
fn open_dialog_cancelled_keeps_welcome() {
    let mut harness = harness();
    click(&mut harness, &fr(TextId::MenuOpen));
    let app = harness.state();
    assert!(!app.is_opening(), "l'annulation doit libérer l'ouverture");
    assert!(app.document.is_none());
    assert!(app.messages().is_empty(), "{:?}", app.messages());
}

#[test]
fn open_error_shows_dismissable_banner() {
    let mut harness = harness();
    open_demo(&mut harness);
    harness
        .state_mut()
        .open_bytes(b"pas du MSPDI", plannercraft_ui_egui::DocOrigin::default());
    harness.run();
    assert_eq!(harness.state().messages().len(), 1);
    // Le document précédent reste affiché.
    assert_eq!(harness.state().document.as_ref().unwrap().opening, 1);
    assert!(harness.query_by_label(&fr(TextId::OpenErrorTitle)).is_some());
    click(&mut harness, &fr(TextId::MessageDismiss));
    assert_eq!(harness.state().messages().len(), 0);
    assert!(harness.query_by_label(&fr(TextId::OpenErrorTitle)).is_none());
}

#[test]
fn close_document_returns_to_welcome() {
    let mut harness = harness();
    open_demo(&mut harness);
    harness.state_mut().dispatch(Action::CloseDocument);
    harness.run();
    assert!(harness.state().document.is_none());
    assert!(harness.query_by_label(&fr(TextId::WelcomeOpenDemo)).is_some());
}

#[test]
fn diagnostics_panel_toggles() {
    let mut harness = harness();
    open_demo(&mut harness);
    let no_errors = Texts::new(Language::Fr).get_args(TextId::StatusErrors, &[("count", Arg::Count(0))]);
    click(&mut harness, &no_errors);
    assert!(harness.state().state.diagnostics_open);
    // Exemple sans diagnostic : aucun volet (H-UI-119, lot L10), la
    // pastille seule dit « aucune erreur ».
    assert_eq!(harness.query_all_by_label(&no_errors).count(), 1);
    harness.state_mut().dispatch(Action::ToggleDiagnostics);
    harness.run();
    assert!(!harness.state().state.diagnostics_open);
}

/// Le menu Affichage ouvre et referme le panneau des diagnostics, même sans
/// document (un panneau resté ouvert après « Fermer » se referme).
#[test]
fn diagnostics_menu_entry_toggles_panel() {
    let mut harness = harness();
    open_demo(&mut harness);
    click(&mut harness, &fr(TextId::MenuView));
    click(&mut harness, &fr(TextId::MenuDiagnostics));
    assert!(harness.state().state.diagnostics_open);
    harness.state_mut().dispatch(Action::CloseDocument);
    harness.run();
    click(&mut harness, &fr(TextId::MenuView));
    click(&mut harness, &fr(TextId::MenuDiagnostics));
    assert!(!harness.state().state.diagnostics_open);
}

#[test]
fn click_row_selects_task() {
    let mut harness = harness();
    open_demo(&mut harness);
    let label = Texts::new(Language::Fr).get_args(
        TextId::RowLabel,
        &[
            ("id", Arg::Count(3)),
            ("name", Arg::Str("Plans de l'architecte".into())),
        ],
    );
    click(&mut harness, &label);
    let app = harness.state();
    let expected = app.rows().unwrap().row(2).unwrap().uid;
    assert_eq!(app.selection, Some(expected));
}

#[test]
fn wheel_scroll_keeps_panes_aligned() {
    let mut harness = harness();
    open_demo(&mut harness);
    harness.hover_at(egui::pos2(1000.0, 400.0));
    for _ in 0..20 {
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -5.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers: egui::Modifiers::NONE,
        });
        harness.step();
    }
    harness.run();
    // Chaque volet relève lui-même sa plage (D-fix-5) : la comparaison
    // porte sur deux chemins de dessin distincts.
    let stats = &harness.state().last_frame;
    assert!(stats.table_rows.start > 0, "aucun défilement : {stats:?}");
    assert_eq!(stats.table_rows, stats.gantt_rows);
}

/// Relecture A, M9 : un défilement imposé (y compris au-delà de la fin) se
/// stabilise dans le budget d'images du harnais et montre la ligne voulue.
#[test]
fn scroll_to_row_settles_and_shows_row() {
    let mut harness = harness();
    open_demo(&mut harness);
    harness.state_mut().dispatch(Action::ScrollToRow(29));
    harness.run();
    let rows = harness.state().last_frame.table_rows.clone();
    assert!(rows.start > 0 && rows.contains(&29), "{rows:?}");
    harness.state_mut().dispatch(Action::ScrollToRow(usize::MAX));
    harness.run();
    let stats = &harness.state().last_frame;
    assert!(stats.table_rows.contains(&48), "{stats:?}");
    assert_eq!(stats.table_rows, stats.gantt_rows);
}

/// Relecture B, I2 et C, B1 : Ctrl/Cmd + molette garde sous le pointeur la
/// date qui s'y trouvait (abscisses obtenues par `TimeAxis::x`, sens
/// date → pixel seulement).
#[test]
fn ctrl_wheel_zoom_is_anchored_under_pointer() {
    let mut harness = harness();
    open_demo(&mut harness);
    assert_eq!(harness.state().state.zoom, TimeScaleZoom::Month);
    // Un léger défilement : une ancre ramenée à 0 se verrait.
    harness.state_mut().dispatch(Action::ScrollGanttX(150.0));
    harness.run();
    let date = harness
        .state()
        .document
        .as_ref()
        .unwrap()
        .project()
        .effective_status_date();
    let x_before = screen_axis(harness.state()).x(date);
    let view = harness.state().view();
    let (left, width) = (view.last_gantt_left, view.last_gantt_width);
    assert!(
        x_before > left + 50.0 && x_before < left + width,
        "{x_before} hors du Gantt"
    );
    harness.hover_at(egui::pos2(x_before, 300.0));
    for _ in 0..6 {
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 200.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers: egui::Modifiers::COMMAND,
        });
        harness.step();
        if harness.state().state.zoom != TimeScaleZoom::Month {
            break;
        }
    }
    harness.run();
    assert_eq!(harness.state().state.zoom, TimeScaleZoom::Week);
    let x_after = screen_axis(harness.state()).x(date);
    assert!(
        (x_after - x_before).abs() < 1.0,
        "la date sous le pointeur a bougé : {x_before} → {x_after}"
    );
}

/// L'action `ZoomStep` prend une ancre relative au bord gauche du Gantt.
#[test]
fn zoom_step_action_keeps_date_under_anchor() {
    let mut harness = harness();
    open_demo(&mut harness);
    harness.state_mut().dispatch(Action::ScrollGanttX(100.0));
    harness.run();
    let app = harness.state();
    let date = app.document.as_ref().unwrap().project().effective_status_date();
    let x_before = screen_axis(app).x(date);
    let anchor_x = x_before - app.view().last_gantt_left;
    harness
        .state_mut()
        .dispatch(Action::ZoomStep { steps: 2, anchor_x });
    harness.run();
    assert_eq!(harness.state().state.zoom, TimeScaleZoom::Day);
    let x_after = screen_axis(harness.state()).x(date);
    assert!((x_after - x_before).abs() < 1.0, "{x_before} → {x_after}");
}

/// Relecture B, I3 : un bouton de zoom garde le centre du Gantt.
#[test]
fn zoom_button_keeps_center() {
    let mut harness = harness();
    open_demo(&mut harness);
    harness.state_mut().dispatch(Action::ScrollGanttX(120.0));
    harness.run();
    let view = harness.state().view();
    let (scroll, width) = (view.gantt_scroll_x, view.last_gantt_width);
    assert!(scroll > 0.0);
    ribbon_zoom(&mut harness, TimeScaleZoom::Week);
    let app = harness.state();
    assert_eq!(app.state.zoom, TimeScaleZoom::Week);
    let expected =
        zoom_anchor(scroll, width / 2.0, TimeScaleZoom::Month, TimeScaleZoom::Week).min(max_scroll(app));
    let got = app.view().gantt_scroll_x;
    assert!(
        (got - expected).abs() < 0.5,
        "défilement {got}, attendu {expected}"
    );
}

/// Relecture C, T2 : « Ajuster » revient au zoom ajusté après un zoom
/// manuel, ancré au centre.
#[test]
fn fit_zoom_restores_fitted_zoom() {
    let mut harness = harness();
    open_demo(&mut harness);
    let fitted = harness.state().state.zoom;
    assert_eq!(fitted, TimeScaleZoom::Month);
    ribbon_zoom(&mut harness, TimeScaleZoom::Day);
    harness.state_mut().dispatch(Action::ScrollGanttX(2_000.0));
    harness.run();
    let view = harness.state().view();
    let (scroll, width) = (view.gantt_scroll_x, view.last_gantt_width);
    ribbon_click(&mut harness, RibbonTab::View, &fr(TextId::RibbonEntireProject));
    let app = harness.state();
    assert_eq!(app.state.zoom, fitted);
    assert!(!app.view().needs_fit);
    let expected = zoom_anchor(scroll, width / 2.0, TimeScaleZoom::Day, fitted).min(max_scroll(app));
    assert!((app.view().gantt_scroll_x - expected).abs() < 0.5);
}

/// Relecture C, T4 : une ouverture réussie par le canal installe une
/// nouvelle génération et libère l'ouverture.
#[test]
fn open_event_success_installs_new_generation() {
    let (mut harness, _) = scripted(&[Reply::Demo]);
    open_demo(&mut harness);
    harness.state_mut().open_bytes(b"<x", DocOrigin::default());
    harness.run();
    assert_eq!(harness.state().messages().len(), 1);
    harness.state_mut().dispatch(Action::OpenDialog);
    harness.run();
    let app = harness.state();
    assert!(!app.is_opening());
    let doc = app.document.as_ref().unwrap();
    assert_eq!(doc.opening, 2);
    assert_eq!(doc.origin.name, "maison.xml");
    // J7 L1 : curseur (et sélection) sur la première ligne du document.
    assert_eq!(app.selection, app.rows().unwrap().row(0).map(|row| row.uid));
    assert!(!app.view().needs_fit, "zoom ajusté dès l'image suivante");
    assert!(app.messages().is_empty(), "{:?}", app.messages());
}

/// Relecture C, T3 : un échec d'ouverture par le canal garde le document et
/// libère l'ouverture.
#[test]
fn open_event_error_shows_banner_and_keeps_document() {
    let (mut harness, _) = scripted(&[Reply::ReadError]);
    open_demo(&mut harness);
    harness.state_mut().dispatch(Action::OpenDialog);
    harness.run();
    let app = harness.state();
    assert!(!app.is_opening());
    assert_eq!(app.document.as_ref().unwrap().opening, 1);
    assert_eq!(app.messages().len(), 1);
    assert!(harness.query_by_label(&fr(TextId::OpenErrorTitle)).is_some());
}

/// Relecture A, I3 : une demande sans réponse ne bloque pas « Ouvrir » ;
/// une nouvelle demande relance le sélecteur et la réponse tardive de
/// l'ancienne est ignorée.
#[test]
fn open_dialog_while_opening_relaunches() {
    let (mut harness, held) = scripted(&[Reply::Never, Reply::Never]);
    harness.run();
    harness.state_mut().dispatch(Action::OpenDialog);
    harness.run();
    assert!(harness.state().is_opening());
    assert_eq!(held.borrow().len(), 1);
    harness.state_mut().dispatch(Action::OpenDialog);
    harness.run();
    assert!(harness.state().is_opening());
    assert_eq!(held.borrow().len(), 2, "le sélecteur doit être relancé");
    assert!(
        harness.state().messages().is_empty(),
        "{:?}",
        harness.state().messages()
    );
    // L'ancienne demande répond trop tard : son canal est fermé.
    let stale = held.borrow_mut().remove(0);
    assert!(stale.send(demo_event()).is_err(), "réponse périmée acceptée");
    harness.run();
    assert!(harness.state().document.is_none());
    // La demande en cours aboutit.
    let current = held.borrow_mut().remove(0);
    current.send(demo_event()).unwrap();
    harness.run();
    let app = harness.state();
    assert!(!app.is_opening());
    assert_eq!(app.document.as_ref().unwrap().opening, 1);
}

/// Relecture A, I1 : un diagnostic de lien pendant (texte du cœur avec
/// « → ») s'affiche dans le panneau sans glyphe manquant (le harnais
/// panique sur un glyphe absent).
#[test]
fn dangling_link_diagnostic_renders() {
    let xml = std::str::from_utf8(plannercraft_ui_egui::DEMO_MSPDI)
        .unwrap()
        .replacen("<PredecessorUID>", "<PredecessorUID>777", 1);
    let mut harness = harness();
    harness.run();
    harness
        .state_mut()
        .open_bytes(xml.as_bytes(), DocOrigin::default());
    harness.state_mut().dispatch(Action::ToggleDiagnostics);
    harness.run();
    let app = harness.state();
    assert!(app.state.diagnostics_open);
    let diagnostics = app.diagnostics_view();
    assert_ne!(diagnostics.entries.len(), 0);
    let (_, first) = &diagnostics.entries[0];
    assert!(!first.contains('\u{2192}'), "{first}");
    assert!(
        harness.query_by_label(first).is_some(),
        "diagnostic non affiché : {first}"
    );
}
