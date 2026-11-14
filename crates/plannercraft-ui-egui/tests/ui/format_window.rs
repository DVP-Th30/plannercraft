//! Fenêtre « Mise en forme » (lot F1, ADR-0035, H-UI-216 à H-UI-229) :
//! entrées, onglets, réglages (une commande, une entrée d'annulation
//! chacun), jumeaux critiques, compte des barres, aperçu, petit écran,
//! modèles, interface adoucie ; comportements repris de l'ancienne fenêtre
//! « Mettre en forme les barres » (H-UI-59 à H-UI-65, H-UI-192 à
//! H-UI-197). Modèle par défaut : Sobre (comme l'application).

#![allow(
    clippy::arithmetic_side_effects,
    clippy::float_cmp,
    reason = "tests : positions du pointeur et compteurs bornés"
)]

use egui::{Key, Modifiers, Rect, Shape, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use plannercraft_render::style::{
    BarField, BarRole, Density, Pattern, Rgb, RowBanding, TemplateId, Variant, template,
};
use plannercraft_ui_egui::action::{Action, FormatOp};
use plannercraft_ui_egui::i18n::{Arg, TextId};
use plannercraft_ui_egui::view::format_window::{FormatTab, commands, preview_drawn, role_label, window_id};
use plannercraft_ui_egui::{App, EditOrigin, LanguagePref, ThemePref};

use crate::visible::{
    bar_center, demo_capture, doc, drag_window, effective, history, label, rgb, right_click, row_rect,
    select, shapes, sobre_p, texts, uid,
};
use crate::{click, fr, ribbon_click};

/// Ouvre la fenêtre sur `tab`, type `role` (tâche `task`).
fn open(harness: &mut Harness<'static, App>, tab: FormatTab, role: BarRole, task: Option<u32>) {
    harness.state_mut().dispatch(Action::Format(FormatOp::Open {
        tab,
        role,
        task: task.map(uid),
    }));
    harness.run();
}

/// Ouvre la fenêtre sur l'onglet Graphique, type `role`.
fn open_chart(harness: &mut Harness<'static, App>, role: BarRole) {
    open(harness, FormatTab::Chart, role, None);
}

/// État de la fenêtre (onglet, type) ; `None` : fermée.
fn window(harness: &Harness<'static, App>) -> Option<(FormatTab, BarRole)> {
    harness
        .state()
        .view()
        .format_window
        .as_ref()
        .map(|w| (w.tab, w.role))
}

/// Rectangle de la fenêtre à la dernière image.
fn window_rect(harness: &Harness<'static, App>) -> Rect {
    harness
        .ctx
        .memory(|m| m.area_rect(window_id()))
        .expect("fenêtre « Mise en forme »")
}

/// Exécute un lot JSON comme l'utilisateur.
fn exec(harness: &mut Harness<'static, App>, json: &str) {
    let batch = plannercraft_automation::Batch::from_json_str(json).unwrap();
    harness.state_mut().execute(&batch, EditOrigin::User).unwrap();
    harness.run();
    harness.run();
}

/// Couleur de remplissage claire du rôle dans le style effectif.
fn fill(harness: &Harness<'static, App>, role: BarRole, v: Variant) -> Rgb {
    effective(harness.state()).tokens(v).bars[&role].fill
}

/// Un rectangle de cette couleur est-il peint ?
fn painted(harness: &Harness<'static, App>, c: Rgb) -> bool {
    shapes(harness)
        .iter()
        .any(|s| matches!(s, Shape::Rect(r) if r.fill == rgb(c)))
}

/// Libellé « Couleur #RRGGBB » d'une pastille.
fn swatch(c: Rgb) -> String {
    label(TextId::FpSwatch, &[("color", Arg::Str(c.hex().into()))])
}

/// Rectangle et état « choisi » de l'unique nœud nommé `name` hors du
/// ruban (les groupes du ruban portent des noms de la fenêtre : « Tâches »,
/// « Mise en forme »).
fn node(harness: &Harness<'static, App>, name: &str) -> (Rect, Option<bool>) {
    let mut found = harness
        .query_all_by_label(name)
        .filter(|n| !crate::in_ribbon(harness, n));
    let n = found
        .next()
        .unwrap_or_else(|| panic!("« {name} » absent hors du ruban"));
    assert!(found.next().is_none(), "« {name} » en double hors du ruban");
    // egui écrit l'état « choisi » d'un bouton en `toggled`.
    let on = n
        .accesskit_node()
        .toggled()
        .map(|t| t == egui::accesskit::Toggled::True);
    (n.rect(), on)
}

/// État « choisi » du nœud nommé `name`.
fn is_selected(harness: &Harness<'static, App>, name: &str) -> Option<bool> {
    node(harness, name).1
}

/// Fait défiler les réglages jusqu'au nœud nommé `name`, puis clique
/// dessus (un réglage plus bas que la zone visible ne reçoit pas le clic du
/// pointeur, comme pour l'utilisateur).
fn press(harness: &mut Harness<'static, App>, name: &str) {
    {
        let n = harness
            .query_all_by_label(name)
            .find(|n| !crate::in_ribbon(harness, n))
            .unwrap_or_else(|| panic!("« {name} » absent hors du ruban"));
        n.scroll_to_me();
    }
    harness.run();
    click(harness, name);
}

/// Ouvre la liste de choix nommée `name` (le libellé visible à côté porte
/// le même nom : recherche par rôle).
fn open_combo(harness: &mut Harness<'static, App>, name: &str) {
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, name)
        .scroll_to_me();
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, name)
        .click();
    harness.run();
}

/// Ouvre une section repliée par son titre.
fn unfold(harness: &mut Harness<'static, App>, title: TextId) {
    press(harness, &fr(title));
}

// --- Entrées et onglets (H-UI-216, H-UI-59) ---------------------------------

/// H-UI-59, amendée par H-UI-216 : clic droit sur la barre critique de
/// « Murs » ⇒ « Mettre en forme les barres de ce type » ⇒ fenêtre « Mise en
/// forme », onglet Graphique, type « Tâches critiques ».
#[test]
fn h_ui_59_right_click_bar_opens_role_panel() {
    let (mut harness, _) = sobre_p();
    let p = bar_center(&harness, 3, "Murs");
    right_click(&mut harness, p);
    click(&mut harness, &fr(TextId::ContextFormatBars));
    assert_eq!(window(&harness), Some((FormatTab::Chart, BarRole::Critical)));
    let state = harness.state().view().format_window.clone().unwrap();
    assert_eq!(state.task, Some(uid(3)));
    assert!(harness.query_all_by_label(&fr(TextId::FwTitle)).next().is_some());
    // La carte du type est marquée choisie.
    assert_eq!(is_selected(&harness, &fr(TextId::FpRoleCritical)), Some(true));
}

/// H-UI-216 : Format → « Mise en forme… » (menu), le ruban (onglet Format)
/// et la palette de commandes ouvrent la même fenêtre, onglet Graphique,
/// type de la tâche active ; l'ancienne entrée n'existe plus ; sans
/// document, rien ne s'ouvre.
#[test]
fn h_ui_216_open_from_every_entry() {
    let (mut harness, _) = sobre_p();
    select(&mut harness, 3);
    click(&mut harness, &fr(TextId::MenuFormat));
    click(&mut harness, &fr(TextId::MenuFormatWindow));
    assert_eq!(window(&harness), Some((FormatTab::Chart, BarRole::Critical)));
    click(&mut harness, &fr(TextId::FwDone));
    assert_eq!(window(&harness), None);
    // Ruban.
    ribbon_click(
        &mut harness,
        plannercraft_ui_egui::ribbon::RibbonTab::Format,
        &fr(TextId::MenuFormatWindow),
    );
    assert_eq!(window(&harness), Some((FormatTab::Chart, BarRole::Critical)));
    harness.state_mut().dispatch(Action::Format(FormatOp::Close));
    harness.run();
    // Palette de commandes.
    harness.key_press_modifiers(Modifiers::COMMAND, Key::K);
    harness.run();
    harness.event(egui::Event::Text("Mise en forme".to_owned()));
    harness.run();
    let listed = harness.state().palette().listed.clone();
    assert_eq!(
        listed.first().map(|l| l.label.clone()),
        Some(fr(TextId::MenuFormatWindow))
    );
    harness.key_press(Key::Enter);
    harness.run();
    assert_eq!(window(&harness), Some((FormatTab::Chart, BarRole::Critical)));
    // Plus de fenêtre concurrente.
    assert!(
        !listed
            .iter()
            .any(|l| l.label.starts_with("Mettre en forme les barres")),
        "{listed:?}"
    );
    // Sans document : rien.
    let mut empty = crate::harness();
    empty.run();
    empty.state_mut().dispatch(Action::Format(FormatOp::Open {
        tab: FormatTab::Chart,
        role: BarRole::Task,
        task: None,
    }));
    empty.run();
    assert!(empty.state().view().format_window.is_none());
}

/// H-UI-216 : les onglets « Tableau » et « Graphique » basculent les
/// réglages ; la croix et « Terminé » ferment sans rien écrire.
#[test]
fn h_ui_216_tabs_switch_and_close() {
    let (mut harness, _) = sobre_p();
    open_chart(&mut harness, BarRole::Task);
    let before = history(harness.state());
    assert!(harness.query_by_label(&fr(TextId::FwWhat)).is_some());
    click(&mut harness, &fr(TextId::FwTabTable));
    assert_eq!(window(&harness).map(|w| w.0), Some(FormatTab::Table));
    assert!(harness.query_by_label(&fr(TextId::FwColumns)).is_some());
    assert!(harness.query_by_label(&fr(TextId::FwWhat)).is_none());
    click(&mut harness, &fr(TextId::FwTabChart));
    assert_eq!(window(&harness).map(|w| w.0), Some(FormatTab::Chart));
    click(&mut harness, &fr(TextId::FwDone));
    assert_eq!(window(&harness), None);
    assert_eq!(history(harness.state()), before);
}

// --- Application immédiate, annulation (H-UI-218, H-UI-60) ------------------

/// H-UI-60 et H-UI-218 : une couleur du nuancier sur les tâches critiques
/// ⇒ barres repeintes à l'image suivante, une entrée d'annulation ;
/// Ctrl/Cmd+Z, fenêtre ouverte ⇒ couleur d'avant, fenêtre toujours
/// ouverte.
#[test]
fn h_ui_60_live_color_change_one_undo() {
    let (mut harness, _) = sobre_p();
    open_chart(&mut harness, BarRole::Critical);
    let before = history(harness.state());
    let old = fill(&harness, BarRole::Critical, Variant::Light);
    let green = Rgb(0x00, 0x9E, 0x73);
    press(&mut harness, &swatch(green));
    assert_eq!(history(harness.state()), before + 1);
    assert_eq!(fill(&harness, BarRole::Critical, Variant::Light), green);
    assert!(painted(&harness, green), "barre critique non repeinte");
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run();
    assert_eq!(fill(&harness, BarRole::Critical, Variant::Light), old);
    assert!(window(&harness).is_some(), "fenêtre fermée par Ctrl/Cmd+Z");
    // La phrase du pied le dit.
    let status = label(TextId::FwStatus, &[("undo", Arg::Str("Ctrl+Z".into()))]);
    assert!(texts(&harness).iter().any(|(_, t)| *t == status), "{status}");
}

/// H-UI-61 : jaune clair sur fond blanc ⇒ avertissement de contraste et
/// couleur conforme proposée ; la proposition s'applique.
#[test]
fn h_ui_61_low_contrast_warned() {
    let (mut harness, _) = sobre_p();
    open_chart(&mut harness, BarRole::Task);
    let yellow = Rgb(0xF0, 0xE4, 0x42);
    press(&mut harness, &swatch(yellow));
    let warned = texts(&harness)
        .iter()
        .any(|(_, t)| t.starts_with("Contraste insuffisant sur le fond clair"));
    assert!(warned, "aucun avertissement de contraste sur le fond clair");
    let suggestion =
        plannercraft_automation::command::style::nearest_compliant(yellow, Rgb(0xFF, 0xFF, 0xFF), 3_000)
            .unwrap();
    press(
        &mut harness,
        &label(
            TextId::FpUseSuggestion,
            &[("color", Arg::Str(suggestion.hex().into()))],
        ),
    );
    assert_eq!(fill(&harness, BarRole::Task, Variant::Light), suggestion);
    assert!(
        !texts(&harness)
            .iter()
            .any(|(_, t)| t.starts_with("Contraste insuffisant"))
    );
}

/// « Rétablir ce type » (H-UI-61) : les surcharges du type choisi seul
/// sont retirées, une entrée d'annulation.
#[test]
fn h_ui_61_reset_role() {
    let (mut harness, _) = sobre_p();
    exec(
        &mut harness,
        r##"{"format_version":1,"ops":[{"op":"style.set_bar_role","role":"task","color":"#123456","base":"sobre"},
            {"op":"style.set_bar_role","role":"critical","color":"#654321"}]}"##,
    );
    open_chart(&mut harness, BarRole::Task);
    let before = history(harness.state());
    press(&mut harness, &fr(TextId::FwResetRole));
    assert_eq!(history(harness.state()), before + 1);
    let st = effective(harness.state());
    assert_eq!(
        st.tokens(Variant::Light).bars[&BarRole::Task].fill,
        template(TemplateId::Sobre).tokens(Variant::Light).bars[&BarRole::Task].fill
    );
    assert_eq!(
        st.tokens(Variant::Light).bars[&BarRole::Critical].fill,
        Rgb(0x65, 0x43, 0x21)
    );
}

/// Positions des pastilles du nuancier des barres (nom accessible
/// « Couleur #RRGGBB » ⇒ centre).
fn swatch_positions(harness: &Harness<'static, App>) -> std::collections::BTreeMap<String, egui::Pos2> {
    let prefix = label(TextId::FpSwatch, &[("color", Arg::Str("#".into()))]);
    harness
        .query_all_by(|n| n.label().is_some_and(|l| l.starts_with(&prefix)))
        .map(|n| (n.accesskit_node().label().unwrap_or_default(), n.rect().center()))
        .collect()
}

/// H-UI-193 (lot R1c, gardée par le lot F1) : cliquer chaque pastille des
/// deux premières rangées (modèle, puis palette) ne déplace aucune d'elles ;
/// une couleur libre déjà utilisée apparaît en troisième rangée, sous les
/// deux autres.
#[test]
fn h_ui_193_swatches_stable() {
    let (mut harness, _) = sobre_p();
    open_chart(&mut harness, BarRole::Task);
    let before = swatch_positions(&harness);
    assert!(before.len() >= 11, "{before:?}");
    for name in before.keys() {
        let node = harness
            .query_all_by_label(name)
            .next()
            .unwrap_or_else(|| panic!("« {name} » absente"));
        node.click();
        harness.run();
        harness.run();
        let now = swatch_positions(&harness);
        for (other, at) in &before {
            assert_eq!(now.get(other), Some(at), "« {other} » déplacée après « {name} »");
        }
    }
    exec(
        &mut harness,
        r##"{"format_version":1,"ops":[{"op":"style.set_bar_role","role":"task","color":"#123456"}]}"##,
    );
    let now = swatch_positions(&harness);
    let free = label(TextId::FpSwatch, &[("color", Arg::Str("#123456".into()))]);
    let at = now.get(&free).expect("couleur libre en troisième rangée");
    let lowest = before.values().map(|p| p.y).fold(f32::MIN, f32::max);
    assert!(at.y > lowest + 4.0, "{at:?} sous {lowest}");
    for (other, p) in &before {
        assert_eq!(now.get(other), Some(p), "« {other} » déplacée");
    }
}

/// H-UI-194 (lot R1c, gardée par le lot F1) : une couleur choisie est
/// mesurée sur les fonds clair, sombre et impression ; bleu nuit `#123456`,
/// lisible sur le blanc, est signalé « sur le fond sombre » et la
/// proposition convient aux trois ; couleur du modèle : fond affiché seul.
#[test]
fn h_ui_194_contrast_three_backgrounds() {
    let (mut harness, _) = sobre_p();
    open_chart(&mut harness, BarRole::Task);
    let warned = |h: &Harness<'static, App>| {
        texts(h)
            .iter()
            .any(|(_, t)| t.starts_with("Contraste insuffisant"))
    };
    assert!(!warned(&harness), "couleur du modèle");
    let navy = Rgb(0x12, 0x34, 0x56);
    exec(
        &mut harness,
        r##"{"format_version":1,"ops":[{"op":"style.set_bar_role","role":"task","color":"#123456"}]}"##,
    );
    assert!(
        texts(&harness)
            .iter()
            .any(|(_, t)| t.starts_with("Contraste insuffisant sur le fond sombre")),
        "fond en cause non nommé"
    );
    let st = effective(harness.state());
    let bgs = [Variant::Light, Variant::Dark, Variant::Print].map(|v| st.tokens(v).bg);
    assert!(plannercraft_render::style::contrast_ratio(navy, bgs[0]) >= 3_000);
    let worst = *bgs
        .iter()
        .min_by_key(|bg| plannercraft_render::style::contrast_ratio(navy, **bg))
        .unwrap();
    let better = plannercraft_automation::command::style::nearest_compliant(navy, worst, 3_000)
        .filter(|b| {
            bgs.iter()
                .all(|bg| plannercraft_render::style::contrast_ratio(*b, *bg) >= 3_000)
        })
        .expect("couleur conforme sur les trois fonds");
    press(
        &mut harness,
        &label(
            TextId::FpUseSuggestion,
            &[("color", Arg::Str(better.hex().into()))],
        ),
    );
    for v in [Variant::Light, Variant::Dark, Variant::Print] {
        assert_eq!(fill(&harness, BarRole::Task, v), better, "{v:?}");
    }
    assert!(!warned(&harness));
}

// --- Cartes et compte des barres (H-UI-220, H-UI-197) ------------------------

/// Nombre de barres du type `role` que le Gantt dessine, compté ici ligne
/// par ligne avec sa règle (`gantt::row_role`) et la surbrillance du
/// critique de la vue ; un en-tête de groupe qui a une barre compte pour
/// une récapitulative (H-UI-197).
fn bars_of(app: &App, role: BarRole) -> usize {
    use plannercraft_ui_egui::row_view::DisplayRow;
    use plannercraft_ui_egui::view::gantt::row_role;
    let rows = app.visible_rows().unwrap();
    let critical = app.view().task_view.highlight_critical;
    (0..rows.len())
        .filter_map(|i| {
            let d = DisplayRow::at(i);
            rows.header_bar_row(d).or_else(|| rows.row(d).copied())
        })
        .filter(|row| row_role(row, critical) == Some(role))
        .count()
}

/// Texte du compte d'une carte pour `count` barres.
fn count_label(count: usize) -> String {
    label(
        TextId::FwKindCount,
        &[("count", Arg::Count(i64::try_from(count).unwrap()))],
    )
}

/// Textes dessinés dans la carte du type `role`.
fn card_texts(harness: &Harness<'static, App>, role: BarRole) -> Vec<String> {
    let card = node(harness, &fr(role_label(role))).0;
    texts(harness)
        .into_iter()
        .filter(|(p, _)| card.contains(*p + egui::vec2(1.0, 1.0)))
        .map(|(_, t)| t)
        .collect()
}

/// H-UI-197, amendée par H-UI-220 : chaque carte d'un type de ligne dit
/// combien de barres de ce type le planning affiché contient (même règle et
/// même surbrillance du critique que le Gantt) ; avancement et ligne de base
/// n'ont pas de compte.
#[test]
fn h_ui_197_role_count_shown() {
    let (mut harness, _) = sobre_p();
    open_chart(&mut harness, BarRole::SummaryCritical);
    for role in [
        BarRole::Task,
        BarRole::Critical,
        BarRole::Summary,
        BarRole::Milestone,
        BarRole::SummaryCritical,
        BarRole::MilestoneCritical,
        BarRole::Inactive,
        BarRole::Manual,
    ] {
        let n = bars_of(harness.state(), role);
        assert!(
            card_texts(&harness, role).contains(&count_label(n)),
            "{role:?} : {:?}",
            card_texts(&harness, role)
        );
    }
    for role in [BarRole::Progress, BarRole::Baseline] {
        let shown = card_texts(&harness, role);
        assert!(
            !shown.iter().any(|t| t.ends_with("dans ce planning")),
            "{role:?} : {shown:?}"
        );
    }
    // Marge et échéance, que l'application ne dessine pas : aucune carte.
    for role in [BarRole::Slack, BarRole::Deadline] {
        assert!(
            harness.query_by_label(&fr(role_label(role))).is_none(),
            "{role:?}"
        );
    }
}

/// H-UI-197 et H-UI-220 : surbrillance du critique coupée ⇒ « aucune dans
/// ce planning » sur la carte des tâches critiques, grisée mais toujours
/// choisissable ; les tâches sont comptées avec les tâches critiques.
#[test]
fn h_ui_197_role_count_none() {
    let (mut harness, _) = sobre_p();
    open_chart(&mut harness, BarRole::Task);
    assert!(bars_of(harness.state(), BarRole::Critical) > 0);
    let all = bars_of(harness.state(), BarRole::Task) + bars_of(harness.state(), BarRole::Critical);
    harness.state_mut().dispatch(Action::SetHighlightCritical(false));
    harness.run();
    assert_eq!(count_label(0), "pas dans ce planning");
    assert!(card_texts(&harness, BarRole::Critical).contains(&count_label(0)));
    assert!(card_texts(&harness, BarRole::Task).contains(&count_label(all)));
    press(&mut harness, &fr(TextId::FpRoleCritical));
    assert_eq!(window(&harness).map(|w| w.1), Some(BarRole::Critical));
}

/// H-UI-197 : planning regroupé ⇒ les barres des en-têtes de groupe, que le
/// Gantt dessine en récapitulatives (H-UI-129), sont comptées avec les
/// récapitulatives.
#[test]
fn h_ui_197_role_count_group_headers() {
    use plannercraft_render::style::GroupBy;
    use plannercraft_ui_egui::group_view::GroupOp;
    use plannercraft_ui_egui::row_view::DisplayRow;
    let (mut harness, _) = sobre_p();
    harness
        .state_mut()
        .dispatch(Action::Group(GroupOp::Set(Some(GroupBy::Critical))));
    harness.run();
    open_chart(&mut harness, BarRole::Summary);
    let rows = harness.state().visible_rows().unwrap();
    let headers = (0..rows.len())
        .filter(|&i| rows.header_bar_row(DisplayRow::at(i)).is_some())
        .count();
    assert!(headers > 0);
    let n = bars_of(harness.state(), BarRole::Summary);
    assert!(n >= headers);
    assert!(card_texts(&harness, BarRole::Summary).contains(&count_label(n)));
}

/// H-UI-220 : la carte d'un type montre sa barre dans son style courant :
/// une couleur choisie repeint la miniature de la carte ; la carte d'un type
/// absent est grisée (miniature atténuée).
#[test]
fn h_ui_220_cards_drawn_in_current_style() {
    let (mut harness, _) = sobre_p();
    // Tâches critiques : présentes dans « P » (carte non grisée).
    open_chart(&mut harness, BarRole::Critical);
    let orange = Rgb(0xE6, 0x9F, 0x00);
    let card = node(&harness, &fr(TextId::FpRoleCritical)).0;
    let in_card = |h: &Harness<'static, App>, c: Rgb| {
        shapes(h)
            .iter()
            .any(|s| matches!(s, Shape::Rect(r) if r.fill == rgb(c) && card.contains_rect(r.rect)))
    };
    assert!(!in_card(&harness, orange));
    press(&mut harness, &swatch(orange));
    assert!(in_card(&harness, orange), "miniature non repeinte");
    // « P » n'a aucune barre de tâche : miniature atténuée.
    let task = node(&harness, &fr(TextId::FpRoleTask)).0;
    let soft_task = commands::shown_color(
        &plannercraft_ui_egui::theme::screen_style(effective(harness.state())).0,
        BarRole::Task,
        Variant::Light,
    )
    .unwrap();
    let full = shapes(&harness)
        .iter()
        .any(|s| matches!(s, Shape::Rect(r) if r.fill == rgb(soft_task) && task.contains_rect(r.rect)));
    assert!(!full, "carte d'un type absent non grisée");
}

// --- Jumeaux critiques (H-UI-221) --------------------------------------------

/// H-UI-221 : case « Même couleur pour les tâches critiques ». Décochée au
/// départ (Sobre : couleurs différentes). Cochée ⇒ les tâches critiques
/// prennent la couleur des tâches, à l'écran et à l'impression, une entrée
/// d'annulation ; une couleur choisie ensuite vaut pour les deux.
/// Décochée ⇒ les tâches critiques reviennent à la couleur du modèle, les
/// tâches gardent la leur ; une entrée d'annulation.
#[test]
fn h_ui_221_twins_checkbox_both_ways() {
    let (mut harness, _) = sobre_p();
    open_chart(&mut harness, BarRole::Task);
    let twins = fr(TextId::FwTwinsTask);
    let checked = |h: &Harness<'static, App>| {
        h.get_by_label(&twins).accesskit_node().toggled() == Some(egui::accesskit::Toggled::True)
    };
    assert!(!checked(&harness));
    let before = history(harness.state());
    press(&mut harness, &twins);
    assert_eq!(history(harness.state()), before + 1);
    assert!(checked(&harness), "état déduit du style");
    for v in [Variant::Light, Variant::Dark, Variant::Print] {
        assert_eq!(
            fill(&harness, BarRole::Critical, v),
            fill(&harness, BarRole::Task, v),
            "{v:?}"
        );
    }
    // Une couleur choisie vaut pour les deux, un lot.
    let green = Rgb(0x00, 0x9E, 0x73);
    press(&mut harness, &swatch(green));
    assert_eq!(history(harness.state()), before + 2);
    for v in [Variant::Light, Variant::Dark, Variant::Print] {
        assert_eq!(fill(&harness, BarRole::Critical, v), green, "{v:?}");
        assert_eq!(fill(&harness, BarRole::Task, v), green, "{v:?}");
    }
    assert!(
        painted(&harness, green),
        "barres critiques de l'exemple repeintes"
    );
    // Décochée : couleur du modèle pour les critiques.
    press(&mut harness, &twins);
    assert_eq!(history(harness.state()), before + 3);
    assert!(!checked(&harness));
    let sobre = template(TemplateId::Sobre);
    for v in [Variant::Light, Variant::Dark, Variant::Print] {
        assert_eq!(
            fill(&harness, BarRole::Critical, v),
            sobre.tokens(v).bars[&BarRole::Critical].fill,
            "{v:?}"
        );
        assert_eq!(fill(&harness, BarRole::Task, v), green, "{v:?}");
    }
}

/// H-UI-221 : la règle vaut pour les trois paires (libellé de la case
/// propre à chacune) et pour l'impression : la scène du PDF peint les
/// barres des deux types de la même couleur ; décocher garde la forme et
/// les textes propres du jumeau.
#[test]
fn h_ui_221_twins_print_and_pairs() {
    use plannercraft_render::geometry::Class;
    let (mut harness, _) = sobre_p();
    open_chart(&mut harness, BarRole::Summary);
    assert!(harness.query_by_label(&fr(TextId::FwTwinsSummary)).is_some());
    press(&mut harness, &fr(TextId::FwTwinsSummary));
    let st = effective(harness.state());
    let scene = {
        let app = harness.state();
        let d = app.document.as_ref().unwrap();
        let texts = plannercraft_automation::render_text::SessionTexts {
            session: &d.session,
            lang: plannercraft_automation::text::Language::Fr,
        };
        let vp = plannercraft_render::geometry::Viewport::new(Variant::Print, plannercraft_render::Lang::Fr);
        plannercraft_render::geometry::layout(d.project(), d.schedule(), &st, &texts, &vp).0
    };
    let paint = |role| scene.paints.get(&Class::Bar(role)).and_then(|p| p.fill);
    // L'exemple « P » n'a que des récapitulatives critiques : leur peinture
    // du PDF est la couleur des récapitulatives.
    assert_eq!(
        paint(BarRole::SummaryCritical),
        Some(st.tokens(Variant::Print).bars[&BarRole::Summary].fill)
    );
    // Jalons : la case porte leur nom ; forme propre du jumeau gardée.
    harness
        .state_mut()
        .dispatch(Action::Format(FormatOp::SetRole(BarRole::MilestoneCritical)));
    harness.run();
    exec(
        &mut harness,
        r#"{"format_version":1,"ops":[{"op":"style.set_bar_role","role":"milestone_critical","thickness":80}]}"#,
    );
    press(&mut harness, &fr(TextId::FwTwinsMilestone));
    press(&mut harness, &fr(TextId::FwTwinsMilestone));
    let st = effective(harness.state());
    assert_eq!(st.bars[&BarRole::MilestoneCritical].height_pct, 80);
    assert_eq!(
        st.tokens(Variant::Light).bars[&BarRole::MilestoneCritical].fill,
        template(TemplateId::Sobre).tokens(Variant::Light).bars[&BarRole::MilestoneCritical].fill
    );
}

// --- Forme, textes, échelle, fond (H-UI-222 à H-UI-224) ----------------------

/// H-UI-222 : extrémités et remplissage, une entrée d'annulation chacun ;
/// épaisseur par la commande de la fenêtre (curseur appliqué au relâcher).
#[test]
fn h_ui_222_shape_settings() {
    let (mut harness, _) = sobre_p();
    open_chart(&mut harness, BarRole::Critical);
    let before = history(harness.state());
    press(&mut harness, &fr(TextId::FpEndsSquare));
    assert_eq!(effective(harness.state()).bars[&BarRole::Critical].radius16, 0);
    press(&mut harness, &fr(TextId::FwFillHatch));
    assert_eq!(
        effective(harness.state()).bars[&BarRole::Critical].pattern,
        Pattern::Hatch
    );
    press(&mut harness, &fr(TextId::FwFillOutline));
    assert_eq!(
        effective(harness.state()).bars[&BarRole::Critical].pattern,
        Pattern::Outline
    );
    assert_eq!(history(harness.state()), before + 3);
    // Les autres réglages du rôle sont gardés.
    assert_eq!(effective(harness.state()).bars[&BarRole::Critical].radius16, 0);
    let thickness = fr(TextId::FpThickness);
    assert!(
        harness
            .query_by_role_and_label(egui::accesskit::Role::Slider, &thickness)
            .is_some()
    );
}

/// H-UI-63, amendée par H-UI-223 : « Nom (durée) » choisi à droite des
/// tâches critiques (la case « Durée après le nom » est remplacée par ce
/// champ des listes) ⇒ « Fondations (5 j) » sur la barre ; « Nom » ⇒
/// « Fondations ».
#[test]
fn h_ui_63_duration_after_name_toggle() {
    let (mut harness, _) = sobre_p();
    open_chart(&mut harness, BarRole::Critical);
    assert!(harness.query_by_label("Durée après le nom").is_none());
    open_combo(&mut harness, &fr(TextId::FwTextRight));
    click(&mut harness, &fr(TextId::FieldNameDuration));
    let st = effective(harness.state());
    assert_eq!(st.texts[&BarRole::Critical].right, Some(BarField::NameDuration));
    let drawn = |h: &Harness<'static, App>, t: &str| texts(h).iter().any(|(_, x)| x == t);
    assert!(drawn(&harness, "Fondations (5\u{a0}j)"), "{:?}", texts(&harness));
    open_combo(&mut harness, &fr(TextId::FwTextRight));
    click(&mut harness, &fr(TextId::FieldName));
    assert_eq!(
        effective(harness.state()).texts[&BarRole::Critical].right,
        Some(BarField::Name)
    );
    assert!(drawn(&harness, "Fondations"));
}

/// Lecture d'un drapeau du style.
type ReadFlag = fn(&plannercraft_render::style::PlanStyle) -> bool;

/// Lot d'un réglage, construit sur le style courant.
type MakeBatch = fn(&commands::StyleCtx<'_>) -> Option<plannercraft_automation::Batch>;

/// H-UI-224 : échelle de temps et fond (mois relatifs, numéros de semaine,
/// jours chômés, traits verticaux, aujourd'hui, étiquette de la date
/// d'état) : une case = un drapeau du style, une entrée d'annulation ; les
/// autres champs de la section gardés.
#[test]
fn h_ui_224_scale_and_background_toggles() {
    let (mut harness, _) = sobre_p();
    open_chart(&mut harness, BarRole::Task);
    unfold(&mut harness, TextId::FwScale);
    unfold(&mut harness, TextId::FwBackground);
    let start = effective(harness.state());
    let checks: [(TextId, ReadFlag); 7] = [
        (TextId::FwRelativeMonths, |s| s.scale.relative_months),
        (TextId::FwWeekNumbers, |s| s.scale.week_numbers),
        (TextId::FwNonWorking, |s| s.scale.shade_non_working),
        (TextId::FwMajorLines, |s| s.grid.major_v),
        (TextId::FwMinorLines, |s| s.grid.minor_v),
        (TextId::FwToday, |s| s.scale.today_mark),
        (TextId::FwStatusLabel, |s| s.scale.status_label),
    ];
    for (n, (id, read)) in checks.iter().enumerate() {
        let before = history(harness.state());
        let was = read(&effective(harness.state()));
        press(&mut harness, &fr(*id));
        assert_eq!(history(harness.state()), before + 1, "{id:?}");
        assert_eq!(read(&effective(harness.state())), !was, "{id:?}");
        // Les cases d'avant gardent leur nouvel état.
        for (prev, read_prev) in checks.iter().take(n) {
            assert_eq!(
                read_prev(&effective(harness.state())),
                !read_prev(&start),
                "{prev:?} après {id:?}"
            );
        }
    }
    // Mois relatifs : niveau « M1 » dessiné.
    assert!(texts(&harness).iter().any(|(_, t)| t == "M1"));
}

/// H-UI-65 : couleur de phase sur la récapitulative « Second œuvre » ⇒
/// barre et sous-barres de la couleur, fond de sa ligne dans la table en
/// teinte lisible ; une entrée d'annulation. Section dans l'onglet
/// Graphique, avec la phrase sur la ligne du tableau.
#[test]
fn h_ui_65_phase_color_row_and_bar() {
    let (mut harness, _) = sobre_p();
    select(&mut harness, 5);
    open(&mut harness, FormatTab::Chart, BarRole::Summary, Some(5));
    assert!(harness.query_by_label(&fr(TextId::FwPhaseHint)).is_some());
    let red = Rgb(0xC0, 0x00, 0x00);
    let before = history(harness.state());
    press(
        &mut harness,
        &label(TextId::FpPhaseSwatch, &[("color", Arg::Str(red.hex().into()))]),
    );
    assert_eq!(history(harness.state()), before + 1);
    let st = effective(harness.state());
    assert_eq!(st.phases.get(&5).map(|p| p.fill), Some(red));
    assert_eq!(doc(harness.state()).map(|d| d.base), Some(TemplateId::Sobre));
    let zone = crate::visible::gantt_zone(&harness);
    let all = shapes(&harness);
    let bars = all
        .iter()
        .filter(|s| matches!(s, Shape::Rect(r) if r.fill == rgb(red) && r.rect.left() > zone.left()))
        .count();
    assert!(bars >= 2, "sous-barres de la couleur : {bars}");
    let screen = plannercraft_ui_egui::theme::screen_style(st.clone()).0;
    let tint = plannercraft_render::style::phase_row_tint(&screen, Variant::Light, red);
    let row = row_rect(&harness, 5, "Second œuvre");
    let filled = all
        .iter()
        .any(|s| matches!(s, Shape::Rect(r) if r.fill == rgb(tint) && (r.rect.center().y - row.center().y).abs() < 2.0));
    assert!(filled, "fond de ligne de phase absent");
}

/// H-UI-61 et H-UI-65 : une couleur de phase trop pâle (jaune sur blanc)
/// est signalée, avec une couleur conforme sur les fonds des trois
/// variantes ; `style.get` la signale aussi.
#[test]
fn h_ui_61_phase_color_contrast_warned() {
    let (mut harness, _) = sobre_p();
    select(&mut harness, 5);
    open(&mut harness, FormatTab::Chart, BarRole::Summary, Some(5));
    let yellow = Rgb(0xF0, 0xE4, 0x42);
    press(
        &mut harness,
        &label(TextId::FpPhaseSwatch, &[("color", Arg::Str(yellow.hex().into()))]),
    );
    let warned = |h: &Harness<'static, App>| {
        texts(h)
            .iter()
            .any(|(_, t)| t.starts_with("Contraste insuffisant"))
    };
    assert!(warned(&harness));
    let st = effective(harness.state());
    let issues = plannercraft_automation::command::style::contrast_issues(&st);
    assert!(issues.iter().any(|i| i.phase == Some(5) && i.color == yellow));
    let bgs = [Variant::Light, Variant::Dark, Variant::Print].map(|v| st.tokens(v).bg);
    let worst = *bgs
        .iter()
        .min_by_key(|bg| plannercraft_render::style::contrast_ratio(yellow, **bg))
        .unwrap();
    let better = plannercraft_automation::command::style::nearest_compliant(yellow, worst, 3_000)
        .filter(|b| {
            bgs.iter()
                .all(|bg| plannercraft_render::style::contrast_ratio(*b, *bg) >= 3_000)
        })
        .unwrap();
    press(
        &mut harness,
        &label(
            TextId::FpUseSuggestion,
            &[("color", Arg::Str(better.hex().into()))],
        ),
    );
    assert_eq!(
        effective(harness.state()).phases.get(&5).map(|p| p.fill),
        Some(better)
    );
    assert!(!warned(&harness));
}

// --- Fenêtre : position, taille, petit écran (H-UI-192, H-UI-217) ------------

/// H-UI-192, amendée par H-UI-217 : la fenêtre s'ouvre dans la zone de
/// travail, à gauche du rail sans le recouvrir ; elle se déplace par sa
/// barre de titre, reste dans l'écran, et garde sa position refermée puis
/// rouverte.
#[test]
fn h_ui_192_format_window_moves() {
    let (mut harness, _) = sobre_p();
    open_chart(&mut harness, BarRole::Task);
    let win = window_rect(&harness);
    let rail = plannercraft_ui_egui::view::side_panel::rail_drawn(&harness.ctx).expect("rail");
    assert!(win.right() <= rail.rect.left(), "{win:?} {rail:?}");
    let grip = pos2(win.left() + 16.0, win.top() + 14.0);
    drag_window(&mut harness, grip, grip + egui::vec2(-60.0, 30.0));
    let moved = window_rect(&harness);
    let (dx, dy) = (moved.left() - win.left(), moved.top() - win.top());
    assert!(dx < -40.0 && dy > 0.0, "{win:?} ⇒ {moved:?}");
    let grip = pos2(moved.left() + 16.0, moved.top() + 14.0);
    drag_window(&mut harness, grip, pos2(-2_000.0, -2_000.0));
    let screen = harness.ctx.content_rect();
    let kept = window_rect(&harness);
    assert!(screen.contains_rect(kept), "{kept:?} hors de {screen:?}");
    harness.state_mut().dispatch(Action::Format(FormatOp::Close));
    harness.run();
    open_chart(&mut harness, BarRole::Critical);
    let again = window_rect(&harness);
    assert!(
        (again.left() - kept.left()).abs() < 1.0 && (again.top() - kept.top()).abs() < 1.0,
        "{kept:?} ⇒ {again:?}"
    );
}

/// H-UI-217 : à 1 280 × 720, la fenêtre tient dans l'écran, « Terminé » et
/// les onglets sont visibles et atteignables, l'aperçu à droite des
/// réglages ; sous 760 points de large, l'aperçu passe au-dessus.
#[test]
fn h_ui_217_fits_1280x720_and_narrow() {
    let (mut harness, _) = sobre_p();
    open_chart(&mut harness, BarRole::Task);
    let screen = harness.ctx.content_rect();
    let win = window_rect(&harness);
    assert!(screen.contains_rect(win), "{win:?} hors de {screen:?}");
    let done = node(&harness, &fr(TextId::FwDone)).0;
    assert!(win.contains_rect(done) && screen.contains_rect(done), "{done:?}");
    let tab = node(&harness, &fr(TextId::FwTabTable)).0;
    let drawn = preview_drawn(&harness.ctx).expect("aperçu");
    assert!(drawn.sheet.left() > tab.right(), "aperçu à droite des réglages");
    assert!(win.contains_rect(drawn.sheet));
    click(&mut harness, &fr(TextId::FwDone));
    assert_eq!(window(&harness), None);
    // Fenêtre étroite (800 × 600) : aperçu au-dessus des réglages.
    let mut narrow = crate::common::harness_sized(
        {
            let mut app = crate::common::app(LanguagePref::Fr, ThemePref::Light);
            app.open_demo();
            app
        },
        ThemePref::Light,
        egui::vec2(800.0, 600.0),
        1.0,
    );
    narrow.run();
    open_chart(&mut narrow, BarRole::Task);
    let screen = narrow.ctx.content_rect();
    let win = window_rect(&narrow);
    assert!(screen.contains_rect(win), "{win:?} hors de {screen:?}");
    let drawn = preview_drawn(&narrow.ctx).expect("aperçu");
    let tab = node(&narrow, &fr(TextId::FwTabTable)).0;
    assert!(
        drawn.sheet.bottom() < tab.top(),
        "aperçu au-dessus : {drawn:?} {tab:?}"
    );
    let done = node(&narrow, &fr(TextId::FwDone)).0;
    assert!(screen.contains_rect(done));
}

// --- Aperçu (H-UI-219) --------------------------------------------------------

/// H-UI-219 : l'aperçu montre les lignes du planning ouvert, change à
/// l'image qui suit un réglage (scène relue, barres de la nouvelle
/// couleur) ; un clic sur une barre choisit son type et l'onglet
/// Graphique, un clic dans le tableau ouvre l'onglet Tableau.
#[test]
fn h_ui_219_preview_updates_and_clicks() {
    let (mut harness, _) = sobre_p();
    open(&mut harness, FormatTab::Table, BarRole::Task, None);
    let drawn = preview_drawn(&harness.ctx).expect("aperçu");
    assert!(!drawn.bars.is_empty(), "aucune barre dans l'aperçu");
    assert!(
        drawn.headers.contains(&fr(TextId::ColName)),
        "{:?}",
        drawn.headers
    );
    let builds = drawn.builds;
    exec(
        &mut harness,
        r##"{"format_version":1,"ops":[{"op":"style.set_bar_role","role":"critical","color":"#009E73","base":"sobre"}]}"##,
    );
    let drawn = preview_drawn(&harness.ctx).expect("aperçu");
    assert!(drawn.builds > builds, "scène non relue");
    let green = Rgb(0x00, 0x9E, 0x73);
    let in_preview = shapes(&harness)
        .iter()
        .any(|s| matches!(s, Shape::Rect(r) if r.fill == rgb(green) && drawn.sheet.contains_rect(r.rect)));
    assert!(in_preview, "barres critiques de l'aperçu non repeintes");
    // Clic sur une barre critique : son type, onglet Graphique.
    let (_, bar) = *drawn
        .bars
        .iter()
        .find(|(role, r)| *role == BarRole::Critical && drawn.sheet.contains_rect(*r))
        .expect("barre critique dans l'aperçu");
    crate::click_at_point(&mut harness, bar.center());
    assert_eq!(window(&harness), Some((FormatTab::Chart, BarRole::Critical)));
    // Clic dans le tableau : onglet Tableau.
    let drawn = preview_drawn(&harness.ctx).unwrap();
    crate::click_at_point(
        &mut harness,
        pos2(drawn.sheet.left() + 20.0, drawn.sheet.top() + 60.0),
    );
    assert_eq!(window(&harness).map(|w| w.0), Some(FormatTab::Table));
}

/// H-UI-219 : l'aperçu part de la tâche active quand elle est plus bas que
/// les premières lignes (trois lignes au-dessus).
#[test]
fn h_ui_219_preview_around_active_task() {
    let mut harness = demo_capture();
    let app = harness.state();
    let rows = app.visible_rows().unwrap();
    let uids: Vec<_> = (0..rows.len())
        .filter_map(|i| rows.row(plannercraft_ui_egui::row_view::DisplayRow::at(i)))
        .map(|r| r.uid)
        .collect();
    assert!(uids.len() > 20, "exemple trop court");
    let target = uids[20];
    harness.state_mut().dispatch(Action::Select(Some(target)));
    harness.run();
    open_chart(&mut harness, BarRole::Task);
    let name = harness
        .state()
        .document
        .as_ref()
        .unwrap()
        .project()
        .tasks
        .get(target)
        .unwrap()
        .name
        .clone();
    let drawn = preview_drawn(&harness.ctx).unwrap();
    let at = texts(&harness)
        .into_iter()
        .any(|(p, t)| t == name && drawn.sheet.contains(p));
    assert!(at, "« {name} » absente de l'aperçu");
}

// --- Onglet Tableau (H-UI-225, H-UI-227, H-UI-228) ---------------------------

/// Colonnes affichées (champs).
fn columns(harness: &Harness<'static, App>) -> Vec<plannercraft_ui_egui::column::Column> {
    harness.state().state.columns.iter().map(|c| c.column).collect()
}

/// H-UI-225 : étiquettes des colonnes ; retirer (sauf N° et nom), déplacer
/// par les flèches, « + Ajouter » ouvre le sélecteur de champs,
/// « Titre, largeur, alignement… » ouvre « Paramètres de champ » ; aucune
/// entrée d'annulation (préférence de l'application, phrase affichée).
#[test]
fn h_ui_225_columns_tab() {
    use plannercraft_ui_egui::column::Column;
    let (mut harness, _) = sobre_p();
    open(&mut harness, FormatTab::Table, BarRole::Task, None);
    let before = history(harness.state());
    assert!(harness.query_by_label(&fr(TextId::FwColumnsApp)).is_some());
    let cols = columns(&harness);
    assert!(cols.contains(&Column::Duration) && cols.contains(&Column::Start));
    // N° et nom : pas de croix.
    for fixed in [TextId::ColId, TextId::ColName] {
        let x = label(TextId::FwRemoveColumn, &[("name", Arg::Str(fr(fixed).into()))]);
        assert!(harness.query_by_label(&x).is_none(), "{fixed:?}");
    }
    // Retirer « Durée ».
    press(
        &mut harness,
        &label(
            TextId::FwRemoveColumn,
            &[("name", Arg::Str(fr(TextId::ColDuration).into()))],
        ),
    );
    assert!(!columns(&harness).contains(&Column::Duration));
    // Déplacer « Début » vers la droite.
    let at = columns(&harness)
        .iter()
        .position(|c| *c == Column::Start)
        .unwrap();
    let start = label(
        TextId::FwColumnChip,
        &[("name", Arg::Str(fr(TextId::ColStart).into()))],
    );
    press(&mut harness, &start);
    press(&mut harness, &fr(TextId::FwMoveRight));
    assert_eq!(
        columns(&harness).iter().position(|c| *c == Column::Start),
        Some(at + 1)
    );
    press(&mut harness, &fr(TextId::FwMoveLeft));
    assert_eq!(
        columns(&harness).iter().position(|c| *c == Column::Start),
        Some(at)
    );
    assert_eq!(history(harness.state()), before, "préférence, pas le document");
    // Paramètres de la colonne choisie.
    press(&mut harness, &fr(TextId::FwColumnSettings));
    assert!(
        harness.state().view().picker.is_some(),
        "Paramètres de champ non ouverts"
    );
    harness.key_press(Key::Escape);
    harness.run();
    assert!(harness.state().view().picker.is_none(), "Échap ne ferme pas");
    // + Ajouter : sélecteur de champs.
    press(&mut harness, &fr(TextId::FwAddColumn));
    assert!(harness.state().view().picker.is_some(), "sélecteur non ouvert");
}

/// H-UI-227 : hauteur des lignes et fond des lignes écrits dans le style
/// du document, une entrée d'annulation chacun ; la table de l'écran suit.
#[test]
fn h_ui_227_rows_settings() {
    let (mut harness, _) = sobre_p();
    open(&mut harness, FormatTab::Table, BarRole::Task, None);
    let before = history(harness.state());
    let h0 = row_rect(&harness, 2, "Fondations").height();
    press(&mut harness, &fr(TextId::FwDensityAiry));
    assert_eq!(effective(harness.state()).density, Density::Airy);
    let h1 = row_rect(&harness, 2, "Fondations").height();
    assert!(h1 > h0, "{h0} ⇒ {h1}");
    press(&mut harness, &fr(TextId::FwBandZebra));
    assert_eq!(effective(harness.state()).grid.rows, RowBanding::Zebra);
    press(&mut harness, &fr(TextId::FwBandPhase));
    assert_eq!(effective(harness.state()).grid.rows, RowBanding::Phase);
    assert_eq!(history(harness.state()), before + 3);
}

/// H-UI-228 : gras, italique et effacement des tâches sélectionnées depuis
/// l'onglet Tableau (opérations de l'onglet Tâche du ruban) ; sans
/// sélection, grisés avec une phrase.
#[test]
fn h_ui_228_selected_tasks_format() {
    let (mut harness, _) = sobre_p();
    select(&mut harness, 2);
    open(&mut harness, FormatTab::Table, BarRole::Task, None);
    let bold = |h: &Harness<'static, App>| effective(h.state()).task_text.get(&2).is_some_and(|t| t.bold);
    press(&mut harness, &fr(TextId::RibbonBold));
    assert!(bold(&harness));
    press(&mut harness, &fr(TextId::RibbonClearFormatting));
    assert!(!bold(&harness));
    harness.state_mut().dispatch(Action::Select(None));
    harness.run();
    assert!(harness.query_by_label(&fr(TextId::FwSelectedNone)).is_some());
}

// --- Modèles (H-UI-226) -------------------------------------------------------

/// H-UI-226 : « Partir d'un modèle » applique un modèle intégré en gardant
/// la part du document (couleurs de phase), une entrée d'annulation ;
/// « Enregistrer comme modèle… » garde le style sans la part du document ;
/// le modèle personnel apparaît dans l'en-tête, marqué ; « Revenir au
/// modèle » garde la part du document.
#[test]
fn h_ui_226_templates_from_window() {
    let (mut harness, _) = sobre_p();
    exec(
        &mut harness,
        r##"{"format_version":1,"ops":[{"op":"style.apply","template":"sobre","patch":{
            "phases":{"5":{"fill":"#D55E00","children":true}},"density":"airy"}}]}"##,
    );
    open_chart(&mut harness, BarRole::Task);
    let before = history(harness.state());
    click(&mut harness, &fr(TextId::TemplateClassique));
    assert_eq!(history(harness.state()), before + 1);
    let d = doc(harness.state()).unwrap();
    assert_eq!(d.base, TemplateId::Classique);
    assert!(d.over.phases.is_some(), "phases gardées");
    assert!(d.over.density.is_none(), "le reste suit le modèle");
    assert_eq!(is_selected(&harness, &fr(TextId::TemplateClassique)), Some(true));
    // Enregistrer comme modèle.
    click(&mut harness, &fr(TextId::FwTabChart));
    let green = Rgb(0x00, 0x9E, 0x73);
    press(&mut harness, &swatch(green));
    click(&mut harness, &fr(TextId::FpSaveTemplate));
    harness
        .get_by_role_and_label(egui::accesskit::Role::TextInput, &fr(TextId::GallerySaveName))
        .focus();
    harness.run();
    harness.event(egui::Event::Text("Agence".to_owned()));
    harness.run();
    click(&mut harness, &fr(TextId::FwSave));
    let saved = &harness.state().state.user_templates;
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].name, "Agence");
    assert!(
        saved[0].style.over.phases.is_none(),
        "part du document non enregistrée"
    );
    assert!(saved[0].style.over.colors.is_some());
    let message = label(TextId::GallerySaved, &[("name", Arg::Str("Agence".into()))]);
    assert!(texts(&harness).iter().any(|(_, t)| *t == message));
    assert_eq!(is_selected(&harness, "Agence"), Some(true));
    // Revenir au modèle : part du document gardée.
    click(&mut harness, &fr(TextId::FpRevert));
    let d = doc(harness.state()).unwrap();
    assert!(d.over.phases.is_some() && d.over.colors.is_none());
    // Tous les modèles… : la galerie.
    click(&mut harness, &fr(TextId::FwAllTemplates));
    assert!(matches!(
        harness.state().view().dialog.as_ref().map(|d| d.kind),
        Some(plannercraft_ui_egui::action::DialogKind::Gallery)
    ));
}

// --- Interface adoucie (H-UI-195, H-UI-229) ----------------------------------

/// H-UI-195 : une couleur choisie par la fenêtre sur Sobre garde l'écran
/// adouci, repeint les barres du rôle, vaut pour le clair, le sombre et
/// l'impression.
#[test]
fn h_ui_195_panel_color_keeps_soft_screen() {
    let (mut harness, _) = sobre_p();
    let soft_critical = plannercraft_ui_egui::theme::soft_tokens(Variant::Light)
        .unwrap()
        .bars[&BarRole::Critical]
        .fill;
    assert!(painted(&harness, soft_critical));
    open_chart(&mut harness, BarRole::Task);
    let orange = Rgb(0xE6, 0x9F, 0x00);
    press(&mut harness, &swatch(orange));
    let st = effective(harness.state());
    for v in [Variant::Light, Variant::Dark, Variant::Print] {
        assert_eq!(st.tokens(v).bars[&BarRole::Task].fill, orange, "{v:?}");
    }
    assert!(
        plannercraft_ui_egui::theme::screen_style(st).1,
        "écran adouci gardé"
    );
    assert!(painted(&harness, soft_critical), "barres critiques inchangées");
}

/// H-UI-229 : chaque réglage de la fenêtre, passé par sa commande sur le
/// modèle Sobre, garde l'aspect adouci de l'écran (`screen_style`), et
/// chacun écrit une seule entrée d'annulation.
#[test]
#[allow(clippy::too_many_lines, reason = "un réglage par ligne de la liste")]
fn h_ui_229_every_setting_keeps_soft_screen() {
    let (mut harness, _) = sobre_p();
    let lang = plannercraft_automation::text::Language::Fr;
    let soft = |h: &Harness<'static, App>| plannercraft_ui_egui::theme::screen_style(effective(h.state())).1;
    assert!(soft(&harness));
    let settings: Vec<(&str, MakeBatch)> = vec![
        ("couleur", |c| {
            Some(commands::color(BarRole::Task, Rgb(0x00, 0x9E, 0x73), false, c))
        }),
        ("jumeaux", |c| {
            Some(commands::twins_on(BarRole::Summary, Rgb(0xD5, 0x5E, 0x00), c))
        }),
        ("jumeaux décochés", |c| commands::twins_off(BarRole::Summary, c)),
        ("épaisseur", |c| {
            Some(commands::thickness(BarRole::Critical, 70, c))
        }),
        ("extrémités", |c| {
            Some(commands::ends(BarRole::Critical, false, c))
        }),
        ("hachures", |c| {
            commands::pattern(BarRole::Task, Pattern::Hatch, c)
        }),
        ("points", |c| {
            commands::pattern(BarRole::Milestone, Pattern::Dots, c)
        }),
        ("contour", |c| {
            commands::pattern(BarRole::Critical, Pattern::Outline, c)
        }),
        ("textes", |c| {
            use plannercraft_automation::command::Patch;
            commands::texts(
                BarRole::Task,
                Patch::Set(BarField::Start),
                Patch::Set(BarField::NameDuration),
                Patch::Set(BarField::Percent),
                c,
            )
        }),
        ("mois relatifs", |c| {
            commands::scale(c, |s| s.relative_months = !s.relative_months)
        }),
        ("semaines", |c| {
            commands::scale(c, |s| s.week_numbers = !s.week_numbers)
        }),
        ("jours chômés", |c| {
            commands::scale(c, |s| s.shade_non_working = !s.shade_non_working)
        }),
        ("aujourd'hui", |c| {
            commands::scale(c, |s| s.today_mark = !s.today_mark)
        }),
        ("date d'état", |c| {
            commands::scale(c, |s| s.status_label = !s.status_label)
        }),
        ("traits principaux", |c| {
            commands::grid(c, |g| g.major_v = !g.major_v)
        }),
        ("traits secondaires", |c| {
            commands::grid(c, |g| g.minor_v = !g.minor_v)
        }),
        ("zébré", |c| commands::banding(c, RowBanding::Zebra)),
        ("par phase", |c| commands::banding(c, RowBanding::Phase)),
        ("compacte", |c| commands::density(c, Density::Compact)),
        ("aérée", |c| commands::density(c, Density::Airy)),
        ("phase", |c| {
            let mut map = c.style.phases.clone();
            map.insert(
                5,
                plannercraft_render::style::PhaseColor {
                    fill: Rgb(0xC0, 0x00, 0x00),
                    children: true,
                },
            );
            Some(commands::phases(&map, c))
        }),
        ("rétablir un type", |c| {
            Some(commands::reset_role(BarRole::Task, c.lang))
        }),
    ];
    for (what, make) in settings {
        let batch = {
            let app = harness.state();
            let d = app.document.as_ref().unwrap();
            let style = effective(app);
            let doc_style = plannercraft_automation::command::style::doc_style(d.project());
            let ctx = commands::StyleCtx {
                style: &style,
                doc: doc_style.as_ref(),
                default: app.state.default_template,
                lang,
            };
            make(&ctx).unwrap_or_else(|| panic!("{what} : aucun lot"))
        };
        let before = history(harness.state());
        harness.state_mut().execute(&batch, EditOrigin::User).unwrap();
        harness.run();
        assert_eq!(history(harness.state()), before + 1, "{what} : une entrée");
        assert!(soft(&harness), "{what} : écran adouci perdu");
    }
    // Modèle personnel et « Revenir au modèle » : Sobre, adouci.
    let batch = commands::reset_all(lang);
    harness.state_mut().execute(&batch, EditOrigin::User).unwrap();
    harness.run();
    assert!(soft(&harness), "revenir au modèle");
}

// --- Captures ----------------------------------------------------------------

/// Planning de l'exemple en Gantt de suivi, tâches en cours d'avancement,
/// couleurs du diagnostic du lot R1c posées (orange, noir, jaune, rouge) :
/// fenêtre ouverte sur l'onglet Graphique (barres de tâches).
fn chart_capture(theme: ThemePref) -> Harness<'static, App> {
    let mut harness = crate::common::harness(LanguagePref::Fr, theme);
    harness.run();
    harness.state_mut().open_demo();
    harness.run();
    harness.state_mut().dispatch(Action::SetGanttStyle(
        plannercraft_ui_egui::state::GanttStyle::Tracking,
    ));
    let mut ops = Vec::new();
    for (task, pct) in [
        (2, 10_000),
        (3, 6_000),
        (5, 5_000),
        (6, 5_000),
        (10, 4_000),
        (13, 3_000),
        (14, 5_000),
    ] {
        ops.push(format!(
            r#"{{"op":"progress.update","task":{task},"percent_complete":{{"value":{pct}}}}}"#
        ));
    }
    for (role, color) in [
        ("task", "#E69F00"),
        ("critical", "#000000"),
        ("summary", "#F0E442"),
        ("milestone", "#C00000"),
    ] {
        ops.push(format!(
            r#"{{"op":"style.set_bar_role","role":"{role}","color":"{color}","base":"sobre"}}"#
        ));
    }
    exec(
        &mut harness,
        &format!(r#"{{"format_version":1,"ops":[{}]}}"#, ops.join(",")),
    );
    open_chart(&mut harness, BarRole::Task);
    harness
}

/// Exemple ouvert, fenêtre sur l'onglet Tableau, une colonne choisie.
fn table_capture(theme: ThemePref) -> Harness<'static, App> {
    let mut harness = crate::common::harness(LanguagePref::Fr, theme);
    harness.run();
    harness.state_mut().open_demo();
    harness.run();
    open(&mut harness, FormatTab::Table, BarRole::Task, None);
    let start = label(
        TextId::FwColumnChip,
        &[("name", Arg::Str(fr(TextId::ColStart).into()))],
    );
    press(&mut harness, &start);
    harness
}

#[test]
fn snapshot_format_window_chart_fr_light() {
    let mut harness = chart_capture(ThemePref::Light);
    crate::common::snapshot(&mut harness, "format_window_chart_fr_light");
}

#[test]
fn snapshot_format_window_chart_fr_dark() {
    let mut harness = chart_capture(ThemePref::Dark);
    crate::common::snapshot(&mut harness, "format_window_chart_fr_dark");
}

#[test]
fn snapshot_format_window_table_fr_light() {
    let mut harness = table_capture(ThemePref::Light);
    crate::common::snapshot(&mut harness, "format_window_table_fr_light");
}

#[test]
fn snapshot_format_window_table_fr_dark() {
    let mut harness = table_capture(ThemePref::Dark);
    crate::common::snapshot(&mut harness, "format_window_table_fr_dark");
}

/// Fenêtre étroite (800 × 600) : aperçu au-dessus des réglages.
#[test]
fn snapshot_format_window_narrow_fr_light() {
    let mut app = crate::common::app(LanguagePref::Fr, ThemePref::Light);
    app.open_demo();
    let mut harness = crate::common::harness_sized(app, ThemePref::Light, egui::vec2(800.0, 600.0), 1.0);
    harness.run();
    open_chart(&mut harness, BarRole::Critical);
    crate::common::snapshot(&mut harness, "format_window_narrow_fr_light");
}
