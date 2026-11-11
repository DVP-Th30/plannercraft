//! Interface adoucie, lot U2 (ADR-0026, H-UI-172 à H-UI-176) : volets en
//! cartes, barre du haut, ruban compact, Formulaire tâche en grille.
//!
//! Harnais 1 440 × 900 sur « P » (filtres) ; le harnais de kittest pose
//! une marge de 8 points autour de l'`Ui` racine, déduite de la marge des
//! cartes (`view::cards::set_inset`) : les marges mesurées ici sont celles
//! de l'application, comptées depuis le bord de la fenêtre.

#![allow(
    clippy::arithmetic_side_effects,
    reason = "tests : compteurs bornés par le scénario"
)]

use egui::accesskit::Role;
use egui::{Key, Rect, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use plannercraft_core::TaskUid;
use plannercraft_ui_egui::action::PaneOp;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::ribbon::spec::{ItemSize, RibbonCmd, compact_split};
use plannercraft_ui_egui::ribbon::{RibbonMode, RibbonOp, RibbonTab};
use plannercraft_ui_egui::view::cards::{GUTTER, OUTER, OUTER_TIGHT};
use plannercraft_ui_egui::view::ribbon::{COMPACT_H, Kind, TAB_BAND_H, drawn, label_of};
use plannercraft_ui_egui::{Action, App, UiState};

use crate::filters::open_p_in;
use crate::fr;

/// Harnais `size` sur « P », préférences `state`.
fn open(state: UiState, size: egui::Vec2) -> Harness<'static, App> {
    let mut app = App::new(state, plannercraft_ui_egui::Env::fixed(crate::now()));
    open_p_in(&mut app);
    let mut harness = Harness::builder()
        .with_size(size)
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .with_os(egui::os::OperatingSystem::Nix)
        .with_step_dt(1.0 / 60.0)
        .with_max_steps(8)
        .build_ui_state(plannercraft_ui_egui::ui, app);
    harness.run();
    harness
}

/// « P » à 1 440 × 900, préférences de test.
fn p() -> Harness<'static, App> {
    open(crate::fr_light(), vec2(1440.0, 900.0))
}

fn dispatch(harness: &mut Harness<'static, App>, action: Action) {
    harness.state_mut().dispatch(action);
    harness.run();
}

/// H-UI-172 : ruban, chronologie, vue, Formulaire tâche et volet latéral en
/// cartes : marge extérieure de 8 points depuis le bord de la fenêtre,
/// gouttières de 8 points entre deux cartes ; séparateurs au milieu des
/// gouttières ; marges de 4 points sous 900 points de large.
#[test]
fn h_ui_172_cards_and_gutters() {
    let mut harness = p();
    dispatch(&mut harness, Action::Pane(PaneOp::ShowDetails(true)));
    dispatch(
        &mut harness,
        Action::SidePanel(plannercraft_ui_egui::view::side_panel::SideOp::Open(
            plannercraft_ui_egui::view::side_panel::SideTab::WhyThisDate,
        )),
    );
    let ribbon = drawn(&harness.ctx);
    let panes = harness.state().view().panes.drawn.expect("volets dessinés");
    let card = ribbon.body.expect("carte du ruban");
    assert!((ribbon.bar.left() - OUTER).abs() < 0.5, "{:?}", ribbon.bar);
    assert!((ribbon.bar.top() - OUTER).abs() < 0.5, "{:?}", ribbon.bar);
    assert!((card.top() - ribbon.bar.bottom() - GUTTER).abs() < 0.5);
    assert!((card.left() - OUTER).abs() < 0.5 && (card.right() - (1440.0 - OUTER)).abs() < 0.5);
    let timeline = panes.timeline.expect("frise");
    let details = panes.details.expect("volet de détails");
    assert!(
        (timeline.top() - card.bottom() - GUTTER).abs() < 0.5,
        "{timeline:?}"
    );
    assert!((timeline.left() - OUTER).abs() < 0.5, "{timeline:?}");
    assert!((panes.view.top() - timeline.bottom() - GUTTER).abs() < 0.5);
    assert!((details.top() - panes.view.bottom() - GUTTER).abs() < 0.5);
    // Séparateurs : zone de ± 4 points au milieu de la gouttière.
    let split = panes.details_splitter.expect("séparateur");
    assert!((split.top() - panes.view.bottom()).abs() < 0.5 && (split.bottom() - details.top()).abs() < 0.5);
    // Volet latéral : gouttière entre la vue et lui, marge à droite.
    let tip = fr(TextId::SideSplitterTip);
    let side_split = harness.get_by_role_and_label(Role::Splitter, &tip).rect();
    assert!(
        (side_split.left() - panes.view.right()).abs() < 0.5,
        "{side_split:?}"
    );
    assert!((side_split.width() - GUTTER).abs() < 0.5, "{side_split:?}");
    // Fenêtre étroite : marges de 4 points (fonction pure ; le harnais
    // garde sa propre marge de 8 points, plus large).
    assert!((plannercraft_ui_egui::view::cards::outer_for(860.0, false) - OUTER_TIGHT).abs() < f32::EPSILON);
    let narrow = open(crate::fr_light(), vec2(860.0, 700.0));
    assert!(plannercraft_ui_egui::view::cards::outer_margin(&narrow.ctx) < 0.5);
}

/// H-UI-173 : barre du haut unique de 40 points : symbole, « Menu »,
/// Enregistrer, Annuler, Rétablir, onglets, bouton de l'assistant à
/// droite ; pas de champ de recherche des commandes (lot U3) ; le menu
/// ouvre Fichier à Aide, mêmes entrées qu'avant.
#[test]
fn h_ui_173_top_bar() {
    let mut harness = p();
    let d = drawn(&harness.ctx);
    assert!((d.bar.height() - TAB_BAND_H).abs() < 0.5);
    let menu = d.menu.expect("bouton « Menu »");
    let quick: Vec<Rect> = d
        .items
        .iter()
        .filter(|i| i.kind == Some(Kind::Quick))
        .map(|i| i.rect)
        .collect();
    assert_eq!(quick.len(), 3);
    assert!(
        quick
            .iter()
            .all(|r| r.left() >= menu.right() && d.bar.contains_rect(*r))
    );
    let tabs: Vec<Rect> = harness.query_all_by_role(Role::Tab).map(|n| n.rect()).collect();
    assert_eq!(tabs.len(), 5);
    let last_quick = quick.iter().map(Rect::right).fold(0.0, f32::max);
    assert!(
        tabs.iter()
            .all(|t| t.left() > last_quick && d.bar.contains_rect(*t))
    );
    let (assistant, _) = d.assistant.expect("bouton de l'assistant");
    let last_tab = tabs.iter().map(Rect::right).fold(0.0, f32::max);
    assert!(assistant.left() > last_tab && (assistant.right() - d.bar.right()).abs() < 4.0);
    // Lot U3 (H-UI-177) : champ de la palette entre les onglets et le
    // bouton de l'assistant.
    let field = d.palette.expect("champ « Rechercher une commande… »");
    assert!(field.left() > last_tab && field.right() < assistant.left());
    // Menu : Fichier à Aide, et Fichier → Ouvrir… comme avant.
    crate::click(&mut harness, &fr(TextId::MenuFile));
    assert!(
        harness
            .query_all_by_label_contains(&fr(TextId::MenuOpen))
            .next()
            .is_some(),
        "Fichier → Ouvrir"
    );
    harness.key_press(Key::Escape);
    harness.run();
    harness.key_press(Key::Escape);
    harness.run();
    assert!(!egui::Popup::is_any_open(&harness.ctx));
}

/// H-UI-174 : ruban compact par défaut (une rangée de 52 points, onglets et
/// groupes gardés) ; chevron ⇒ ruban complet, puis retour ; Ctrl/Cmd+F1
/// replie et déplie ; mode gardé dans les préférences et relu.
#[test]
fn h_ui_174_compact_default_chevron_and_memory() {
    let mut harness = p();
    assert_eq!(harness.state().state.ribbon.mode, RibbonMode::Compact);
    let d = drawn(&harness.ctx);
    assert_eq!(d.mode, RibbonMode::Compact);
    assert!((d.body.unwrap().height() - COMPACT_H).abs() < 0.5);
    let chevron = d.chevron.expect("chevron");
    assert!(d.body.unwrap().contains_rect(chevron));
    // Libellés des éléments compacts : 30 points de haut, dans la carte.
    let items: Vec<_> = d.items.iter().filter(|i| i.kind != Some(Kind::Quick)).collect();
    assert!(items.len() > 15);
    for item in &items {
        assert!((item.rect.height() - 30.0).abs() < 4.5, "{item:?}");
        assert!(d.body.unwrap().contains_rect(item.rect), "{item:?}");
    }
    harness.get_by_label(&fr(TextId::RibbonShowFull)).click();
    harness.run();
    assert_eq!(harness.state().state.ribbon.mode, RibbonMode::Full);
    assert!(drawn(&harness.ctx).body.unwrap().height() > 90.0);
    harness.get_by_label(&fr(TextId::RibbonShowCompact)).click();
    harness.run();
    assert_eq!(harness.state().state.ribbon.mode, RibbonMode::Compact);
    // Ctrl/Cmd+F1 : repli, puis compact.
    harness.key_press_modifiers(egui::Modifiers::COMMAND, Key::F1);
    harness.run();
    assert_eq!(harness.state().state.ribbon.mode, RibbonMode::Collapsed);
    assert!(drawn(&harness.ctx).body.is_none());
    harness.key_press_modifiers(egui::Modifiers::COMMAND, Key::F1);
    harness.run();
    assert_eq!(harness.state().state.ribbon.mode, RibbonMode::Compact);
    // Mémorisé : complet, relu au lancement suivant.
    dispatch(&mut harness, Action::Ribbon(RibbonOp::ToggleFull));
    let (read, warning) = UiState::from_json_lenient(&harness.state().state.to_json().unwrap());
    assert!(warning.is_none(), "{warning:?}");
    assert_eq!(read.ribbon.mode, RibbonMode::Full);
    let again = open(read, vec2(1440.0, 900.0));
    assert_eq!(drawn(&again.ctx).mode, RibbonMode::Full);
}

/// H-UI-174 : un fichier de préférences d'avant le lot U2 (`collapsed`)
/// est relu sans avertissement : replié ⇒ replié, sinon compact.
#[test]
fn h_ui_174_old_prefs_file() {
    for (collapsed, mode) in [(true, RibbonMode::Collapsed), (false, RibbonMode::Compact)] {
        let json = format!(r#"{{"version":2,"ribbon":{{"tab":"view","collapsed":{collapsed}}}}}"#);
        let (state, warning) = UiState::from_json_lenient(&json);
        assert!(warning.is_none(), "{warning:?}");
        assert_eq!(state.ribbon.tab, RibbonTab::View);
        assert_eq!(state.ribbon.mode, mode);
    }
}

/// Libellés des entrées du menu ouvert (nœuds hors du ruban), raccourci
/// retiré.
fn open_menu_labels(harness: &Harness<'static, App>) -> Vec<String> {
    harness
        .query_all_by(|n| n.label().is_some())
        .filter(|n| !crate::in_ribbon(harness, n))
        .filter_map(|n| n.accesskit_node().label())
        .collect()
}

/// H-UI-175 : aucune commande perdue : dans chaque onglet du ruban compact,
/// chaque élément du ruban complet est visible dans la rangée (même
/// commande) ou dans le menu « … » de son groupe (même libellé), à 1 440
/// points ; à 860 points, chaque groupe replié ouvre un menu de tous ses
/// éléments.
#[test]
fn h_ui_175_every_command_reachable_in_compact() {
    let texts = Texts::new(Language::Fr);
    for width in [1440.0, 860.0] {
        let mut harness = open(crate::fr_light(), vec2(width, 900.0));
        for tab in RibbonTab::ALL {
            dispatch(&mut harness, Action::Ribbon(RibbonOp::Select(tab)));
            let d = drawn(&harness.ctx);
            let visible: Vec<RibbonCmd> = d.items.iter().filter_map(|i| i.cmd).collect();
            for (gi, group) in tab.groups().iter().enumerate() {
                let name = texts.get(group.label);
                let more_label =
                    texts.get_args(TextId::RibbonMore, &[("group", Arg::Str(name.clone().into()))]);
                // Bouton du menu du groupe : « … », ou groupe replié.
                let button = d
                    .items
                    .iter()
                    .find(|i| i.cmd.is_none() && (i.label == more_label || i.label == name))
                    .map(|i| i.rect);
                let menu: Vec<String> = match button {
                    Some(rect) => {
                        crate::click_at_point(&mut harness, rect.center());
                        let labels = open_menu_labels(&harness);
                        harness.key_press(Key::Escape);
                        harness.run();
                        labels
                    }
                    None => Vec::new(),
                };
                let (shown, more) = compact_split(group, tab.compact_groups().get(gi));
                let folded =
                    button.is_some_and(|_| d.items.iter().any(|i| i.cmd.is_none() && i.label == name));
                for item in shown.iter().map(|(i, _)| *i).chain(more.iter().copied()) {
                    let label = label_of(&texts, item);
                    let in_row = !folded && visible.contains(&item.cmd);
                    let in_menu = menu
                        .iter()
                        .any(|l| l == &label || l.starts_with(&format!("{label} ")));
                    assert!(
                        in_row || in_menu,
                        "{width} {tab:?} {name} : {:?} « {label} » ni visible ni dans le menu {menu:?}",
                        item.cmd
                    );
                    if item.size == ItemSize::PercentRow && !folded && width > 1000.0 {
                        assert!(in_row, "{tab:?} : pourcentages visibles");
                    }
                }
            }
        }
    }
}

/// H-UI-174 : avancement 0 / 25 / 50 / 75 / 100 % en groupe segmenté
/// (contigus, même hauteur) ; un clic sur « 50 % » marque la tâche.
#[test]
fn h_ui_174_percent_segment_marks() {
    let mut harness = p();
    dispatch(&mut harness, Action::Select(Some(TaskUid::new(2))));
    let d = drawn(&harness.ctx);
    let percents: Vec<Rect> = d
        .items
        .iter()
        .filter(|i| i.kind == Some(Kind::CompactPercent))
        .map(|i| i.rect)
        .collect();
    assert_eq!(percents.len(), 5);
    for w in percents.windows(2) {
        assert!((w[1].left() - w[0].right()).abs() < 0.5, "{w:?}");
        assert!((w[1].top() - w[0].top()).abs() < 0.5);
    }
    crate::ribbon_click(&mut harness, RibbonTab::Task, "50 %");
    let percent = harness
        .state()
        .document
        .as_ref()
        .unwrap()
        .project()
        .tasks
        .get(TaskUid::new(2))
        .unwrap()
        .progress
        .percent_complete
        .get();
    assert_eq!(percent, 50 * 100);
}

/// H-UI-176 : Formulaire tâche : en-tête « Tâche N », nom, pastille
/// « Récapitulative » (ou « Jalon ») ; Durée, Début, Fin, % achevé, Date
/// de contrainte et Contrainte sur une rangée de six colonnes, libellé
/// au-dessus du champ ; Type de tâche sur la rangée suivante ; cases à
/// cocher en dernière ligne.
#[test]
fn h_ui_176_task_form_grid() {
    let mut harness = p();
    dispatch(&mut harness, Action::Pane(PaneOp::ShowDetails(true)));
    dispatch(&mut harness, Action::Select(Some(TaskUid::new(1))));
    let number = Texts::new(Language::Fr).get_args(TextId::DetailsTaskNumber, &[("id", Arg::Count(1))]);
    assert!(harness.query_by_label(&number).is_some(), "« Tâche 1 »");
    assert!(
        harness
            .query_all_by_label(&fr(TextId::DetailsBadgeSummary))
            .next()
            .is_some(),
        "pastille Récapitulative"
    );
    let field = |label: TextId| {
        harness
            .query_all_by_label(&fr(label))
            .find(|n| matches!(n.accesskit_node().role(), Role::TextInput | Role::ComboBox))
            .unwrap_or_else(|| panic!("champ {label:?}"))
            .rect()
    };
    let row = [
        TextId::DetailsDuration,
        TextId::DetailsStart,
        TextId::DetailsFinish,
        TextId::DetailsPercent,
        TextId::DetailsConstraintDate,
        TextId::DetailsConstraint,
    ]
    .map(field);
    for w in row.windows(2) {
        assert!((w[0].top() - w[1].top()).abs() < 1.0, "{w:?}");
        assert!(w[1].left() > w[0].right(), "{w:?}");
    }
    let name = field(TextId::DetailsName);
    assert!(name.bottom() <= row[0].top());
    let kind = field(TextId::DetailsTaskType);
    assert!(kind.top() > row[0].bottom());
    assert!((kind.left() - row[0].left()).abs() < 1.0);
    let effort = harness.get_by_label(&fr(TextId::DetailsEffortDriven)).rect();
    assert!(effort.top() > kind.bottom());
    // Libellé au-dessus du champ (texte peint juste au-dessus).
    let caption = fr(TextId::DetailsDuration);
    let above = harness.output().shapes.iter().any(|c| match &c.shape {
        egui::epaint::Shape::Text(t) => {
            t.galley.text() == caption && t.pos.y < row[0].top() && (t.pos.x - row[0].left()).abs() < 2.0
        }
        _ => false,
    });
    assert!(above, "libellé « Durée » au-dessus de son champ");
    // Jalon inséré (Insérer → Jalon, menu « … » du ruban compact) : il
    // est suivi par le volet, pastille « Jalon ».
    dispatch(&mut harness, Action::Select(Some(TaskUid::new(3))));
    let before = harness.state().document.as_ref().unwrap().project().tasks.len();
    crate::ribbon_click(&mut harness, RibbonTab::Task, &fr(TextId::RibbonInsertMilestone));
    let project = harness.state().document.as_ref().unwrap().project();
    assert_eq!(project.tasks.len(), before + 1);
    let uid = project
        .tasks
        .iter()
        .find(|t| t.milestone)
        .map(plannercraft_core::Task::uid)
        .expect("jalon inséré");
    dispatch(&mut harness, Action::Select(Some(uid)));
    assert!(
        harness
            .query_all_by_label(&fr(TextId::DetailsBadgeMilestone))
            .next()
            .is_some(),
        "pastille Jalon"
    );
}

/// H-UI-176 : même ordre de tabulation qu'avant le lot U2 : Nom, Durée,
/// Début, Fin, % achevé, Contrainte, Type de tâche, Pilotée par l'effort,
/// Planifiée manuellement (Date de contrainte grisée sans contrainte
/// datée : sautée).
#[test]
fn h_ui_176_tab_order_unchanged() {
    let mut harness = p();
    dispatch(&mut harness, Action::Pane(PaneOp::ShowDetails(true)));
    dispatch(&mut harness, Action::Select(Some(TaskUid::new(2))));
    let name = fr(TextId::DetailsName);
    harness.get_by_role_and_label(Role::TextInput, &name).click();
    harness.run();
    let mut order = Vec::new();
    for _ in 0..9 {
        let focused = harness
            .query_all_by(
                #[allow(clippy::redundant_closure_for_method_calls, reason = "durée de vie du nœud")]
                |n| n.is_focused(),
            )
            .next()
            .and_then(|n| n.accesskit_node().label())
            .unwrap_or_default();
        order.push(focused);
        harness.key_press(Key::Tab);
        harness.run();
    }
    let expected: Vec<String> = [
        TextId::DetailsName,
        TextId::DetailsDuration,
        TextId::DetailsStart,
        TextId::DetailsFinish,
        TextId::DetailsPercent,
        TextId::DetailsConstraint,
        TextId::DetailsTaskType,
        TextId::DetailsEffortDriven,
        TextId::DetailsManual,
    ]
    .map(fr)
    .into();
    assert_eq!(order, expected);
}
