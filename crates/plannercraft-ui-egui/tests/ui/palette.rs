//! Palette de commandes (interface adoucie, ADR-0026 lot U3, H-UI-177 à
//! H-UI-183) : champ de la barre du haut, Ctrl/Cmd+K, filtrage, exécution,
//! focus rendu à la vue, commandes grisées, ligne de l'assistant, aucune
//! commande du ruban ni des menus absente, accessibilité, captures.

#![allow(
    clippy::arithmetic_side_effects,
    reason = "tests : compteurs bornés par le scénario"
)]

use std::collections::BTreeSet;

use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use plannercraft_ui_egui::i18n::{Language, TextId, Texts};
use plannercraft_ui_egui::keymap::KEYMAP;
use plannercraft_ui_egui::palette::{Listed, PaletteOp};
use plannercraft_ui_egui::ribbon::RibbonTab;
use plannercraft_ui_egui::ribbon::spec::{ItemSize, QUICK_ACCESS, RIBBON, RibbonCmd};
use plannercraft_ui_egui::view::ribbon::{drawn, label_of};
use plannercraft_ui_egui::view::side_panel::SideTab;
use plannercraft_ui_egui::{Action, App, Env, LanguagePref, ThemePref, UiState};

use crate::common::{harness_sized, snapshot};
use crate::{click_at_point, fr, fr_light, open_demo};

/// Application de test sur l'exemple, préférences `state`, fenêtre `size`.
fn demo_in(state: UiState, size: egui::Vec2) -> Harness<'static, App> {
    let app = App::new(state, Env::fixed(crate::now()));
    let mut harness = Harness::builder()
        .with_size(size)
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .with_os(egui::os::OperatingSystem::Nix)
        .with_max_steps(8)
        .build_ui_state(plannercraft_ui_egui::ui, app);
    harness.run();
    open_demo(&mut harness);
    harness
}

/// Exemple ouvert, 1 280 × 720, préférences de test.
fn demo() -> Harness<'static, App> {
    demo_in(fr_light(), vec2(1280.0, 720.0))
}

/// Préférences de test, assistant activé.
fn with_assistant() -> UiState {
    let mut state = fr_light();
    state.assistant.enabled = true;
    state
}

fn dispatch(harness: &mut Harness<'static, App>, action: Action) {
    harness.state_mut().dispatch(action);
    harness.run();
}

fn is_open(harness: &Harness<'static, App>) -> bool {
    harness.state().palette().open
}

fn listed(harness: &Harness<'static, App>) -> Vec<Listed> {
    harness.state().palette().listed.clone()
}

fn labels(harness: &Harness<'static, App>) -> Vec<String> {
    listed(harness).into_iter().map(|l| l.label).collect()
}

/// Tape `text` dans la saisie (focalisée).
fn type_text(harness: &mut Harness<'static, App>, text: &str) {
    harness.event(Event::Text(text.to_owned()));
    harness.run();
}

/// Ouvre la palette par Ctrl/Cmd+K et tape `query`.
fn open_with(harness: &mut Harness<'static, App>, query: &str) {
    harness.key_press_modifiers(Modifiers::COMMAND, Key::K);
    harness.run();
    assert!(is_open(harness), "Ctrl/Cmd+K n'ouvre pas la palette");
    if !query.is_empty() {
        type_text(harness, query);
    }
}

fn key(harness: &mut Harness<'static, App>, key: Key) {
    harness.key_press(key);
    harness.run();
}

fn history(harness: &Harness<'static, App>) -> usize {
    harness
        .state()
        .document
        .as_ref()
        .map_or(0, |d| d.session.history().entries.len())
}

fn tasks(harness: &Harness<'static, App>) -> usize {
    harness
        .state()
        .document
        .as_ref()
        .map_or(0, |d| d.project().tasks.len())
}

fn table_focused(harness: &Harness<'static, App>) -> bool {
    let id = plannercraft_ui_egui::view::table::table_id();
    harness.ctx.memory(|m| m.has_focus(id))
}

fn query_focused(harness: &Harness<'static, App>) -> bool {
    let id = plannercraft_ui_egui::view::palette::query_id();
    harness.ctx.memory(|m| m.has_focus(id))
}

/// Vérification de l'entrée `i` de `KEYMAP` (Ctrl/Cmd+K,
/// `session::ui_keymap_each_shortcut_dispatches_*`) : sans document
/// (contexte global), ouvre la palette, puis la referme.
pub(crate) fn check_keymap_entry(i: usize) {
    let (_, shortcut, _) = KEYMAP[i];
    let mut harness = crate::harness();
    harness.run();
    assert!(!is_open(&harness));
    harness.key_press_modifiers(shortcut.modifiers, shortcut.logical_key);
    harness.run();
    assert!(is_open(&harness));
    harness.key_press_modifiers(shortcut.modifiers, shortcut.logical_key);
    harness.run();
    assert!(!is_open(&harness));
}

/// H-UI-177 : champ « Rechercher une commande… » entre les onglets et le
/// bouton de l'assistant, 300 points, indice Ctrl+K ; un clic ouvre la
/// palette (saisie focalisée) ; Ctrl/Cmd+K aussi, accueil compris.
#[test]
fn h_ui_177_top_field_and_shortcut() {
    let mut harness = demo();
    let d = drawn(&harness.ctx);
    let field = d.palette.expect("champ de la palette dessiné");
    let (assistant, _) = d.assistant.expect("bouton de l'assistant");
    let last_tab = harness
        .query_all_by_role(Role::Tab)
        .map(|n| n.rect().right())
        .fold(0.0, f32::max);
    assert!((field.width() - 300.0).abs() < 0.5, "{field:?}");
    assert!((field.height() - 30.0).abs() < 0.5);
    assert!(
        field.left() > last_tab && field.right() < assistant.left(),
        "{field:?}"
    );
    assert!(d.bar.contains_rect(field));
    let name = fr(TextId::PaletteField);
    let node = harness.get_by_role_and_label(Role::Button, &name);
    assert_eq!(node.rect(), field);
    click_at_point(&mut harness, field.center());
    assert!(is_open(&harness));
    harness.run();
    assert!(query_focused(&harness), "saisie non focalisée");
    key(&mut harness, Key::Escape);
    assert!(!is_open(&harness));
    // Accueil sans document : Ctrl/Cmd+K ouvre aussi.
    let mut welcome = crate::harness();
    welcome.run();
    open_with(&mut welcome, "");
    assert_ne!(listed(&welcome).len(), 0);
}

/// H-UI-177 : fenêtre étroite : le champ rétrécit, puis loupe seule, puis
/// disparaît ; il ne chevauche jamais les onglets ni le bouton de
/// l'assistant, dont la règle (libellé ou icône seule) est inchangée.
#[test]
fn h_ui_177_field_shrinks_when_narrow() {
    let mut seen_icon = false;
    for width in [1280.0, 1024.0, 900.0, 800.0, 700.0, 600.0] {
        let harness = demo_in(fr_light(), vec2(width, 720.0));
        let d = drawn(&harness.ctx);
        let (assistant, _) = d.assistant.expect("bouton de l'assistant");
        let last_tab = harness
            .query_all_by_role(Role::Tab)
            .map(|n| n.rect().right())
            .fold(0.0, f32::max);
        if let Some(field) = d.palette {
            assert!(field.left() > last_tab, "{width} : {field:?} sur les onglets");
            assert!(
                field.right() < assistant.left(),
                "{width} : {field:?} sur l'assistant"
            );
            assert!(field.width() >= 30.0 - 0.5);
            seen_icon |= (field.width() - 30.0).abs() < 0.5;
        }
    }
    assert!(seen_icon, "jamais en loupe seule");
}

/// H-UI-178 : filtrage tolérant aux accents et à la casse, sous-séquence,
/// classement (« lier » : « Lier les tâches » en tête).
#[test]
fn h_ui_178_filtering() {
    let mut harness = demo();
    open_with(&mut harness, "");
    let all = listed(&harness).len();
    assert!(all > 100, "{all} entrées seulement");
    type_text(&mut harness, "lier");
    let first = labels(&harness);
    assert_eq!(
        first.first().map(String::as_str),
        Some(fr(TextId::StructureLink).as_str())
    );
    assert!(first.len() < all);
    // Accents et casse.
    key(&mut harness, Key::Escape);
    open_with(&mut harness, "ECHELLE");
    let found = labels(&harness);
    assert!(
        found.contains(&fr(TextId::ZoomWeek)) || found.iter().any(|l| l.contains("chelle")),
        "{found:?}"
    );
    key(&mut harness, Key::Escape);
    open_with(&mut harness, "insrecap");
    assert_eq!(
        labels(&harness).first().map(String::as_str),
        Some(fr(TextId::MenuInsertSummary).as_str()),
        "sous-séquence"
    );
    key(&mut harness, Key::Escape);
    open_with(&mut harness, "zzzzqqq");
    assert_eq!(listed(&harness), Vec::<Listed>::new());
}

/// H-UI-179 : Entrée exécute la commande choisie, comme le ruban (même
/// lot d'annulation, même effet) ; la palette se ferme et le focus revient
/// à la table.
#[test]
fn h_ui_179_run_same_as_ribbon() {
    let mut by_palette = demo();
    let before = (tasks(&by_palette), history(&by_palette));
    open_with(&mut by_palette, &fr(TextId::MenuInsertMilestone));
    assert_eq!(
        labels(&by_palette).first(),
        Some(&fr(TextId::MenuInsertMilestone))
    );
    key(&mut by_palette, Key::Enter);
    by_palette.run();
    assert!(!is_open(&by_palette));
    let mut by_ribbon = demo();
    let item = RIBBON
        .iter()
        .flat_map(|(_, groups)| groups.iter())
        .flat_map(|g| g.items.iter())
        .find(|i| i.cmd == RibbonCmd::InsertMilestone)
        .expect("élément Jalon du ruban");
    let label = label_of(&Texts::new(Language::Fr), item);
    crate::ribbon_click(&mut by_ribbon, RibbonTab::Task, &label);
    assert_eq!(tasks(&by_palette), before.0 + 1);
    assert_eq!(history(&by_palette), before.1 + 1, "un seul lot d'annulation");
    assert_eq!(
        (tasks(&by_palette), history(&by_palette)),
        (tasks(&by_ribbon), history(&by_ribbon))
    );
    assert_eq!(by_palette.state().selection, by_ribbon.state().selection);
    // ↓ puis Entrée : la deuxième ligne (échelle de temps : Semaine).
    open_with(&mut by_palette, "semaine");
    let rows = labels(&by_palette);
    let week = rows
        .iter()
        .position(|l| *l == fr(TextId::ZoomWeek))
        .expect("Semaine");
    for _ in 0..week {
        key(&mut by_palette, Key::ArrowDown);
    }
    assert_eq!(by_palette.state().palette().selected, week);
    key(&mut by_palette, Key::Enter);
    assert_eq!(
        by_palette.state().state.zoom,
        plannercraft_ui_egui::TimeScaleZoom::Week
    );
}

/// H-UI-180 : Échap ferme et rend le focus à la table ; un clic hors de
/// la palette ferme aussi ; ↑ ne remonte pas au-dessus de la première
/// ligne.
#[test]
fn h_ui_180_escape_and_outside_click_return_focus() {
    let mut harness = demo();
    harness.run();
    assert!(table_focused(&harness), "table non focalisée au départ");
    open_with(&mut harness, "li");
    assert!(query_focused(&harness));
    assert!(!table_focused(&harness));
    key(&mut harness, Key::ArrowUp);
    assert_eq!(harness.state().palette().selected, 0);
    key(&mut harness, Key::Escape);
    harness.run();
    assert!(!is_open(&harness));
    assert!(table_focused(&harness), "focus non rendu à la table");
    // Clic hors de la palette (coin bas gauche).
    open_with(&mut harness, "");
    click_at_point(&mut harness, egui::pos2(20.0, 700.0));
    harness.run();
    assert!(!is_open(&harness));
    assert!(table_focused(&harness));
}

/// H-UI-181 : commande indisponible grisée avec la raison du ruban ;
/// Entrée sans effet (la palette reste ouverte).
#[test]
fn h_ui_181_disabled_with_reason() {
    let mut harness = demo();
    dispatch(&mut harness, Action::Select(None));
    let before = history(&harness);
    open_with(&mut harness, "lier");
    let first = listed(&harness).into_iter().next().expect("ligne");
    assert_eq!(first.label, fr(TextId::StructureLink));
    assert!(!first.enabled);
    assert_eq!(first.reason, Some(fr(TextId::RibbonReasonNoSelection)));
    key(&mut harness, Key::Enter);
    assert!(is_open(&harness), "Entrée sur une commande grisée");
    assert_eq!(history(&harness), before);
    // Accessible : grisée, nom avec la raison.
    let name = format!("{}, {}", first.label, fr(TextId::RibbonReasonNoSelection));
    let node = harness.get_by_role_and_label(Role::ListBoxOption, &name);
    assert!(node.accesskit_node().is_disabled());
}

/// H-UI-182 : dernière ligne « Demander à l'assistant : « … » » si
/// l'assistant est activé et la saisie non vide ; Tab ou Entrée sur elle
/// ouvre le volet sur l'onglet Assistant, question préremplie, rien
/// d'envoyé. Absente si l'assistant n'est pas activé.
#[test]
fn h_ui_182_ask_assistant_row() {
    let mut off = demo();
    open_with(&mut off, "lier");
    assert!(listed(&off).iter().all(|l| !l.ask), "assistant non activé");
    let mut on = demo_in(with_assistant(), vec2(1280.0, 720.0));
    open_with(&mut on, "");
    assert!(listed(&on).iter().all(|l| !l.ask), "saisie vide");
    type_text(&mut on, "lier deux tâches");
    let rows = listed(&on);
    let last = rows.last().expect("ligne");
    assert!(last.ask);
    assert!(last.label.contains("lier deux tâches"));
    let count = rows.len();
    for _ in 0..count {
        key(&mut on, Key::ArrowDown);
    }
    assert_eq!(
        on.state().palette().selected,
        count - 1,
        "↓ s'arrête à la dernière ligne"
    );
    key(&mut on, Key::Enter);
    on.run();
    assert!(!is_open(&on));
    assert!(on.state().side_panel().open);
    assert_eq!(on.state().side_panel().tab, SideTab::Assistant);
    assert_eq!(on.state().assistant().draft, "lier deux tâches");
    assert!(on.state().assistant().conversation.is_empty(), "rien envoyé");
    // Tab : même effet.
    let mut tab = demo_in(with_assistant(), vec2(1280.0, 720.0));
    open_with(&mut tab, "zoom");
    key(&mut tab, Key::Tab);
    tab.run();
    assert_eq!(tab.state().assistant().draft, "zoom");
}

/// Libellés des entrées des menus de la barre du haut (sous-menus
/// compris, sauf fichiers récents et couleurs, hors de la palette) ; les
/// en-têtes de sous-menus ne sont pas des commandes.
fn menu_entries(harness: &mut Harness<'static, App>) -> BTreeSet<String> {
    let headers: BTreeSet<String> = [
        TextId::MenuFile,
        TextId::MenuEdit,
        TextId::MenuView,
        TextId::MenuTask,
        TextId::MenuProject,
        TextId::MenuFormat,
        TextId::MenuHelp,
        TextId::MenuRecent,
        TextId::MenuExport,
        TextId::MenuZoom,
        TextId::MenuTheme,
        TextId::MenuLanguage,
        TextId::MenuFilter,
        TextId::MenuOutlineLevel,
        TextId::MenuGroupBy,
        TextId::RibbonShift,
        TextId::RibbonColumns,
        TextId::MenuTableDisplay,
        TextId::RibbonTextColor,
        TextId::RibbonHighlight,
    ]
    .into_iter()
    .map(fr)
    .collect();
    let all = |h: &Harness<'static, App>| -> BTreeSet<String> {
        h.query_all_by(|n| n.label().is_some())
            .filter_map(|n| n.accesskit_node().label())
            .collect()
    };
    let close = |h: &mut Harness<'static, App>| {
        for _ in 0..4 {
            if !egui::Popup::is_any_open(&h.ctx) {
                break;
            }
            h.key_press(Key::Escape);
            h.run();
        }
    };
    let base = all(harness);
    let mut found = BTreeSet::new();
    let subs: &[(TextId, &[TextId])] = &[
        (TextId::MenuFile, &[TextId::MenuExport]),
        (TextId::MenuEdit, &[]),
        (
            TextId::MenuView,
            &[
                TextId::MenuZoom,
                TextId::MenuTheme,
                TextId::MenuLanguage,
                TextId::MenuFilter,
                TextId::MenuOutlineLevel,
                TextId::MenuGroupBy,
            ],
        ),
        (TextId::MenuTask, &[TextId::RibbonShift]),
        (TextId::MenuProject, &[]),
        (TextId::MenuFormat, &[TextId::RibbonColumns]),
        (TextId::MenuHelp, &[]),
    ];
    for (top, nested) in subs {
        crate::click(harness, &fr(*top));
        let opened = all(harness);
        found.extend(opened.difference(&base).cloned());
        for sub in *nested {
            let before = all(harness);
            crate::click(harness, &fr(*sub));
            if *sub == TextId::RibbonColumns {
                crate::click(harness, &fr(TextId::MenuTableDisplay));
            }
            found.extend(all(harness).difference(&before).cloned());
        }
        close(harness);
    }
    found
        .into_iter()
        .filter(|label| !headers.contains(label))
        .collect()
}

/// H-UI-183 : aucune commande du ruban (accès rapide compris) ni des menus
/// n'est absente de la palette (saisie vide : toute la liste).
#[test]
fn h_ui_183_no_command_missing() {
    let mut harness = demo();
    let menus = menu_entries(&mut harness);
    assert!(menus.len() > 80, "{} entrées de menus relevées", menus.len());
    open_with(&mut harness, "");
    let palette: BTreeSet<String> = labels(&harness).into_iter().collect();
    // Une entrée de menu à raccourci est nommée « libellé raccourci » :
    // même libellé et même raccourci dans la palette.
    let with_keys: BTreeSet<String> = listed(&harness)
        .into_iter()
        .filter_map(|l| l.shortcut.map(|key| format!("{} {key}", l.label)))
        .collect();
    let missing: Vec<&String> = menus
        .iter()
        .filter(|l| !palette.contains(*l) && !with_keys.contains(*l))
        .collect();
    assert!(
        missing.is_empty(),
        "entrées de menus absentes de la palette : {missing:?}"
    );
    // Ruban : chaque commande à action (les listes sont dépliées, vérifiées
    // par leurs jumeaux de menus ci-dessus).
    let texts = Texts::new(Language::Fr);
    let mut checked = 0;
    for item in RIBBON
        .iter()
        .flat_map(|(_, groups)| groups.iter())
        .flat_map(|g| g.items.iter())
        .chain(QUICK_ACCESS)
    {
        let cmd = item.cmd;
        if matches!(cmd, RibbonCmd::MarkPercent(_) | RibbonCmd::AddToTimeline)
            || (item.size == ItemSize::Dropdown && cmd.dropdown_entry().is_none())
        {
            continue;
        }
        let label = texts.get(cmd.menu_label());
        assert!(
            palette.contains(&label),
            "{cmd:?} : « {label} » absent de la palette"
        );
        checked += 1;
    }
    assert!(checked > 40, "{checked}");
    assert!(
        palette.iter().filter(|l| l.contains('%')).count() >= 5,
        "« Marquer à n % »"
    );
}

/// H-UI-183 : accessibilité : fenêtre nommée, saisie de recherche nommée,
/// liste et options nommées, ligne choisie sélectionnée, cases avec leur
/// état.
#[test]
fn h_ui_183_accessibility() {
    let mut harness = demo();
    open_with(&mut harness, "chronologie");
    harness.get_by_role_and_label(Role::Dialog, &fr(TextId::PaletteName));
    let field = fr(TextId::PaletteField);
    let input = harness.get_by_role_and_label(Role::SearchInput, &field);
    assert!(input.accesskit_node().is_focused() || query_focused(&harness));
    harness.get_by_role_and_label(Role::ListBox, &fr(TextId::PaletteList));
    let options: Vec<_> = harness.query_all_by_role(Role::ListBoxOption).collect();
    assert_eq!(options.len(), listed(&harness).len());
    assert!(
        options
            .iter()
            .all(|o| o.accesskit_node().label().is_some_and(|l| !l.is_empty()))
    );
    let selected: Vec<_> = options
        .iter()
        .filter(|o| o.accesskit_node().is_selected() == Some(true))
        .collect();
    assert_eq!(selected.len(), 1);
    let rows = listed(&harness);
    assert_eq!(
        selected[0].accesskit_node().label().as_deref(),
        Some(rows[0].label.as_str())
    );
    // Case « Chronologie » (Affichage) : état exposé.
    let timeline = fr(TextId::RibbonTimeline);
    let node = options
        .iter()
        .find(|o| o.accesskit_node().label().as_deref() == Some(timeline.as_str()))
        .expect("case Chronologie");
    assert_eq!(
        node.accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::True)
    );
}

/// H-UI-183 : clic sur une ligne : même exécution qu'Entrée.
#[test]
fn h_ui_183_click_row_runs() {
    let mut harness = demo();
    open_with(&mut harness, &fr(TextId::ZoomMonth));
    let label = fr(TextId::ZoomMonth);
    harness
        .query_all_by_role(Role::ListBoxOption)
        .find(|n| n.accesskit_node().label().as_deref() == Some(label.as_str()))
        .expect("ligne Mois")
        .click();
    harness.run();
    assert!(!is_open(&harness));
    assert_eq!(
        harness.state().state.zoom,
        plannercraft_ui_egui::TimeScaleZoom::Month
    );
}

/// Capture de la palette : exemple, aucune tâche choisie (« Lier les
/// tâches » grisée), saisie `query` (« li » : commandes actives et grisées).
fn capture(language: LanguagePref, theme: ThemePref, size: egui::Vec2, query: &str, name: &str) {
    let state = UiState {
        language,
        theme,
        ..UiState::default()
    };
    let app = App::new(state, Env::fixed(crate::now()));
    let mut harness = harness_sized(app, theme, size, 1.0);
    harness.run();
    harness.state_mut().open_demo();
    harness.run();
    dispatch(&mut harness, Action::Select(None));
    dispatch(&mut harness, Action::Palette(PaletteOp::Open));
    type_text(&mut harness, query);
    let rows = listed(&harness);
    assert!(rows.iter().any(|l| !l.enabled), "{name} : aucune commande grisée");
    assert!(rows.iter().any(|l| l.enabled), "{name} : aucune commande active");
    snapshot(&mut harness, name);
}

#[test]
fn snapshot_palette_fr_light() {
    capture(
        LanguagePref::Fr,
        ThemePref::Light,
        vec2(1280.0, 720.0),
        "li",
        "palette_fr_light",
    );
}

#[test]
fn snapshot_palette_en_dark() {
    capture(
        LanguagePref::En,
        ThemePref::Dark,
        vec2(1280.0, 720.0),
        "li",
        "palette_en_dark",
    );
}

#[test]
fn snapshot_palette_800_fr() {
    capture(
        LanguagePref::Fr,
        ThemePref::Light,
        vec2(800.0, 600.0),
        "li",
        "palette_800_fr",
    );
}
