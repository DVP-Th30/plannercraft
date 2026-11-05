//! Mise en forme du texte des lignes (finitions avant v1.0.0, lot P2,
//! ADR-0022 D3) : groupe Texte de l'onglet Tâche (H-UI-127), gestes et
//! annulation (H-UI-126), palette et contraste, pinceau (H-UI-128),
//! raccourcis Ctrl/Cmd+B et Ctrl/Cmd+Maj+I, saisie d'une cellule intacte.
//!
//! Projet « P » (`filters::project_p`) : 1 « Gros œuvre » ⊃ 2 « Fondations »,
//! 3 « Murs », 4 jalon ; 5 « Second œuvre » ⊃ 6, 7.

use egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use plannercraft_automation::command::text_style::task_text_of;
use plannercraft_core::TaskUid;
use plannercraft_render::style::{Rgb, TaskText, contrast_ratio};
use plannercraft_ui_egui::action::TextStyleOp;
use plannercraft_ui_egui::i18n::TextId;
use plannercraft_ui_egui::ribbon::RibbonTab;
use plannercraft_ui_egui::view::ribbon::Stage;
use plannercraft_ui_egui::{Action, App};

use crate::filters::harness_p;
use crate::{click, fr, ribbon_click, ribbon_menu};

fn uid(n: u32) -> TaskUid {
    TaskUid::new(n)
}

fn text_of(app: &App, n: u32) -> TaskText {
    task_text_of(app.document.as_ref().unwrap().project(), uid(n))
}

fn history_len(app: &App) -> usize {
    app.document.as_ref().unwrap().session.history().entries.len()
}

fn dispatch(harness: &mut Harness<'static, App>, action: Action) {
    harness.state_mut().dispatch(action);
    harness.run();
}

fn select(harness: &mut Harness<'static, App>, n: u32) {
    dispatch(harness, Action::Select(Some(uid(n))));
}

/// Nœud du ruban nommé `label` (unique).
fn toggled(harness: &Harness<'static, App>, label: &str) -> Option<egui::accesskit::Toggled> {
    harness
        .query_all_by_label(label)
        .find_map(|n| n.accesskit_node().toggled())
}

/// H-UI-127 : groupe Texte juste après Presse-papiers, six éléments en
/// icônes seules ; à 1 280 px, il garde tous ses boutons (pas replié).
#[test]
fn h_ui_127_text_group_after_clipboard_icons_only() {
    let groups = RibbonTab::Task.groups();
    let names: Vec<TextId> = groups.iter().map(|g| g.label).collect();
    let clipboard = names
        .iter()
        .position(|n| *n == TextId::RibbonGroupClipboard)
        .unwrap();
    assert_eq!(names.get(clipboard + 1), Some(&TextId::RibbonGroupText));
    let text = groups.get(clipboard + 1).unwrap();
    assert_eq!(text.items.len(), 6);
    assert!(text.items.iter().all(|i| i.cmd.icon_only() && i.icon.is_some()));
    let harness = crate::ribbon::p_at(1280.0);
    let d = plannercraft_ui_egui::view::ribbon::drawn(&harness.ctx);
    let stage = d
        .groups
        .iter()
        .find(|(n, _, _)| *n == fr(TextId::RibbonGroupText))
        .map(|(_, s, _)| *s);
    assert_ne!(stage, Some(Stage::Folded));
    assert!(stage.is_some());
}

/// H-UI-126, H-UI-127 : Gras au ruban sur 3 ⇒ gras, une entrée
/// d'annulation, bouton enfoncé ; second clic ⇒ retiré ; Ctrl+Z rend
/// l'état d'avant.
#[test]
fn h_ui_127_bold_ribbon_toggle_one_undo() {
    let mut harness = harness_p();
    select(&mut harness, 3);
    let bold = fr(TextId::RibbonBold);
    assert_eq!(toggled(&harness, &bold), Some(egui::accesskit::Toggled::False));
    ribbon_click(&mut harness, RibbonTab::Task, &bold);
    assert!(text_of(harness.state(), 3).bold);
    assert_eq!(history_len(harness.state()), 1);
    assert_eq!(toggled(&harness, &bold), Some(egui::accesskit::Toggled::True));
    ribbon_click(&mut harness, RibbonTab::Task, &bold);
    assert!(!text_of(harness.state(), 3).bold);
    assert_eq!(history_len(harness.state()), 2);
    dispatch(&mut harness, Action::Undo);
    assert!(text_of(harness.state(), 3).bold);
}

/// H-UI-126 : geste sans effet (Effacer sur une tâche sans mise en forme,
/// couleur déjà posée) ⇒ aucune entrée d'annulation, aucun message.
#[test]
fn h_ui_126_no_op_gesture_writes_nothing() {
    let mut harness = harness_p();
    select(&mut harness, 3);
    let messages = harness.state().messages().len();
    ribbon_click(&mut harness, RibbonTab::Task, &fr(TextId::RibbonClearFormatting));
    assert_eq!(history_len(harness.state()), 0);
    assert_eq!(harness.state().messages().len(), messages);
    let red = Rgb(0xC0, 0, 0);
    dispatch(&mut harness, Action::TextStyle(TextStyleOp::Color(Some(red))));
    assert_eq!(history_len(harness.state()), 1);
    dispatch(&mut harness, Action::TextStyle(TextStyleOp::Color(Some(red))));
    assert_eq!(history_len(harness.state()), 1, "même couleur : rien");
    ribbon_click(&mut harness, RibbonTab::Task, &fr(TextId::RibbonClearFormatting));
    assert_eq!(text_of(harness.state(), 3), TaskText::default());
    assert_eq!(history_len(harness.state()), 2);
}

/// H-UI-127 : Ctrl/Cmd+B et Ctrl/Cmd+Maj+I sur la sélection ; Ctrl/Cmd+I
/// insère toujours une tâche (raccourci gardé).
#[test]
fn h_ui_127_shortcuts_bold_italic() {
    let mut harness = harness_p();
    select(&mut harness, 3);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::B);
    harness.run();
    assert!(text_of(harness.state(), 3).bold);
    let shift_command = Modifiers {
        shift: true,
        ..Modifiers::COMMAND
    };
    harness.key_press_modifiers(shift_command, Key::I);
    harness.run();
    let t = text_of(harness.state(), 3);
    assert!(t.bold && t.italic, "{t:?}");
    let tasks = harness.state().document.as_ref().unwrap().project().tasks.len();
    assert_eq!(history_len(harness.state()), 2);
    // Ctrl/Cmd+I seul : insertion (aucune mise en forme de plus).
    harness.key_press_modifiers(Modifiers::COMMAND, Key::I);
    harness.run();
    assert_eq!(
        harness.state().document.as_ref().unwrap().project().tasks.len(),
        tasks + 1
    );
}

/// Pendant la saisie d'une cellule, Ctrl/Cmd+B et Ctrl/Cmd+Maj+I ne
/// mettent rien en forme et la saisie continue.
#[test]
fn h_ui_127_shortcuts_ignored_while_editing() {
    let mut harness = harness_p();
    select(&mut harness, 3);
    harness.event(Event::Text("Mur".to_owned()));
    harness.run();
    assert!(harness.state().view().edit.editing.is_some(), "saisie ouverte");
    harness.key_press_modifiers(Modifiers::COMMAND, Key::B);
    harness.run();
    harness.key_press_modifiers(
        Modifiers {
            shift: true,
            ..Modifiers::COMMAND
        },
        Key::I,
    );
    harness.run();
    assert!(
        harness.state().view().edit.editing.is_some(),
        "saisie toujours ouverte"
    );
    harness.event(Event::Text("s".to_owned()));
    harness.run();
    let editing = harness.state().view().edit.editing.as_ref().unwrap();
    assert_eq!(editing.buffer, "Murs");
    assert_eq!(text_of(harness.state(), 3), TaskText::default());
    assert_eq!(history_len(harness.state()), 0);
}

/// H-UI-127 : Couleur du texte ▾ → une case de la palette ; couleur peu
/// contrastée ⇒ avertissement et teinte corrigée proposée, qui s'applique
/// en un geste ; « Automatique » la retire.
#[test]
fn h_ui_127_text_color_palette_and_contrast() {
    let mut harness = harness_p();
    select(&mut harness, 3);
    let yellow = Rgb(0xF0, 0xE4, 0x42);
    let swatch = plannercraft_ui_egui::i18n::Texts::new(plannercraft_ui_egui::i18n::Language::Fr).get_args(
        TextId::FpSwatch,
        &[("color", plannercraft_ui_egui::i18n::Arg::Str(yellow.hex().into()))],
    );
    ribbon_menu(
        &mut harness,
        RibbonTab::Task,
        &fr(TextId::RibbonTextColor),
        &swatch,
    );
    assert_eq!(text_of(harness.state(), 3).color, Some(yellow));
    assert_eq!(history_len(harness.state()), 1);
    ribbon_click(&mut harness, RibbonTab::Task, &fr(TextId::RibbonTextColor));
    let warning = harness
        .query_all_by(|n| {
            n.role() == egui::accesskit::Role::Label
                && n.value().is_some_and(|l| l.starts_with("Contraste insuffisant"))
        })
        .count();
    assert_eq!(warning, 1);
    let suggestion = harness
        .query_all_by(|n| n.label().is_some_and(|l| l.starts_with("Utiliser #")))
        .next()
        .unwrap()
        .accesskit_node()
        .label()
        .unwrap_or_default();
    click(&mut harness, &suggestion);
    let better = text_of(harness.state(), 3).color.unwrap();
    assert_ne!(better, yellow);
    let bg = plannercraft_render::style::template(plannercraft_render::style::TemplateId::Classique)
        .tokens(plannercraft_render::style::Variant::Light)
        .bg;
    assert!(contrast_ratio(better, bg) >= 4_500);
    assert_eq!(history_len(harness.state()), 2);
    ribbon_menu(
        &mut harness,
        RibbonTab::Task,
        &fr(TextId::RibbonTextColor),
        &fr(TextId::TextColorAuto),
    );
    assert_eq!(text_of(harness.state(), 3).color, None);
}

/// H-UI-127 : Surlignage ▾ par le jumeau du menu Format ; « Aucun
/// surlignage » le retire.
#[test]
fn h_ui_127_highlight_menu_twin() {
    let mut harness = harness_p();
    select(&mut harness, 6);
    let gold = Rgb(0xFF, 0xC0, 0x00);
    let swatch = plannercraft_ui_egui::i18n::Texts::new(plannercraft_ui_egui::i18n::Language::Fr).get_args(
        TextId::FpSwatch,
        &[("color", plannercraft_ui_egui::i18n::Arg::Str(gold.hex().into()))],
    );
    for step in [TextId::MenuFormat, TextId::RibbonHighlight] {
        click(&mut harness, &fr(step));
    }
    click(&mut harness, &swatch);
    assert_eq!(text_of(harness.state(), 6).fill, Some(gold));
    dispatch(&mut harness, Action::TextStyle(TextStyleOp::Fill(None)));
    assert_eq!(text_of(harness.state(), 6).fill, None);
    assert_eq!(history_len(harness.state()), 2);
}

/// H-UI-128 : pinceau : mise en forme de 3 copiée, la sélection suivante
/// (6) la reçoit exactement, un geste ; le pinceau se désarme. Échap
/// annule un pinceau armé.
#[test]
fn h_ui_128_format_painter_copies_then_disarms() {
    let mut harness = harness_p();
    select(&mut harness, 3);
    dispatch(&mut harness, Action::TextStyle(TextStyleOp::ToggleBold));
    dispatch(
        &mut harness,
        Action::TextStyle(TextStyleOp::Fill(Some(Rgb(0xFF, 0xF2, 0xCC)))),
    );
    let source = text_of(harness.state(), 3);
    let painter = fr(TextId::RibbonFormatPainter);
    ribbon_click(&mut harness, RibbonTab::Task, &painter);
    assert!(harness.state().view().painter.is_some());
    assert_eq!(toggled(&harness, &painter), Some(egui::accesskit::Toggled::True));
    let before = history_len(harness.state());
    select(&mut harness, 6);
    harness.run();
    assert_eq!(text_of(harness.state(), 6), source);
    assert_eq!(history_len(harness.state()), before + 1);
    assert!(harness.state().view().painter.is_none());
    // Échap : désarmé, rien appliqué.
    select(&mut harness, 3);
    ribbon_click(&mut harness, RibbonTab::Task, &painter);
    assert!(harness.state().view().painter.is_some());
    harness.key_press(Key::Escape);
    harness.run();
    assert!(harness.state().view().painter.is_none());
    select(&mut harness, 7);
    harness.run();
    assert_eq!(text_of(harness.state(), 7), TaskText::default());
}

/// H-UI-126 : le texte et le surlignage sont résolus pour le thème de
/// l'écran (sombre : couleur peu contrastée éclaircie, même teinte).
#[test]
fn h_ui_126_screen_looks_follow_theme() {
    let mut harness = harness_p();
    select(&mut harness, 3);
    let navy = Rgb(0x1F, 0x3A, 0x93);
    dispatch(&mut harness, Action::TextStyle(TextStyleOp::Color(Some(navy))));
    let style = harness
        .state()
        .state
        .effective_style(harness.state().document.as_ref().unwrap().project());
    let light = plannercraft_ui_egui::theme::text_looks(&style, plannercraft_render::style::Variant::Light);
    let dark = plannercraft_ui_egui::theme::text_looks(&style, plannercraft_render::style::Variant::Dark);
    assert_eq!(light.get(&3).and_then(|l| l.color), Some(navy));
    let shown = dark.get(&3).and_then(|l| l.color).unwrap();
    assert_ne!(shown, navy);
    assert!(contrast_ratio(shown, style.tokens(plannercraft_render::style::Variant::Dark).bg) >= 4_500);
}
