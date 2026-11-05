//! Regrouper par (finitions avant v1.0.0, lot P3, ADR-0022 D4) : liste du
//! ruban et jumeau du menu Affichage (H-UI-129), en-têtes de groupe,
//! annulation, filtre avant le groupement, impression ; gestes de structure
//! refusés et saisie permise (H-UI-130) ; repli, navigation et sélection
//! sur les en-têtes (H-UI-131).
//!
//! Projet « P » (`filters::project_p`) : 1 « Gros œuvre » ⊃ 2 « Fondations »,
//! 3 « Murs », 4 jalon « Livraison » ; 5 « Second œuvre » ⊃ 6 « Plâtrerie »,
//! 7 « Peinture ». Texte1 renommé « Lot » : 2, 3 « Lot 10 » ; 4, 7 « Lot 2 » ;
//! 6 sans valeur.

use egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_core::{CustomFieldId, TaskUid};
use plannercraft_render::style::GroupBy;
use plannercraft_ui_egui::group_view::GroupOp;
use plannercraft_ui_egui::i18n::TextId;
use plannercraft_ui_egui::ribbon::RibbonTab;
use plannercraft_ui_egui::ribbon::spec::RibbonCmd;
use plannercraft_ui_egui::row_view::DisplayRow;
use plannercraft_ui_egui::structure::StructureOp;
use plannercraft_ui_egui::view::Pane;
use plannercraft_ui_egui::{Action, App};

use crate::filters::harness_p;
use crate::{click, fr, ribbon_menu};

const LOT: GroupBy = GroupBy::Custom(CustomFieldId::TEXT1);

fn uid(n: u32) -> TaskUid {
    TaskUid::new(n)
}

fn dispatch(harness: &mut Harness<'static, App>, action: Action) {
    harness.state_mut().dispatch(action);
    harness.run();
}

fn history_len(app: &App) -> usize {
    app.document.as_ref().unwrap().session.history().entries.len()
}

/// P avec le champ « Lot » (Texte1) renseigné ; l'historique est vidé de
/// ces préparations par la mesure relative des tests.
fn harness_lots() -> Harness<'static, App> {
    let mut harness = harness_p();
    let mut ops = vec![serde_json::json!({ "op": "custom_field.rename", "field": "text1", "alias": "Lot" })];
    for (n, lot) in [(2, "Lot 10"), (3, "Lot 10"), (4, "Lot 2"), (7, "Lot 2")] {
        ops.push(serde_json::json!({ "op": "task.update", "uid": n, "custom": { "text1": lot } }));
    }
    let batch =
        plannercraft_automation::Batch::from_json(serde_json::json!({ "format_version": 1, "ops": ops }))
            .unwrap();
    dispatch(&mut harness, Action::Exec(batch));
    harness
}

/// Lignes affichées : `Err(nom de l'en-tête)` ou `Ok(UID)`.
fn lines(app: &App) -> Vec<Result<u32, String>> {
    let rows = app.visible_rows().unwrap();
    (0..rows.len())
        .map(|d| {
            let d = DisplayRow::at(d);
            rows.header(d)
                .map_or_else(|| Ok(rows.uid(d).unwrap().get()), |h| Err(h.name.clone()))
        })
        .collect()
}

fn header(name: &str) -> Result<u32, String> {
    Err(name.to_owned())
}

/// H-UI-129 : Regrouper par ▾ juste après Filtre dans Affichage → Données,
/// jumeau « Regrouper par » au menu Affichage.
#[test]
fn h_ui_129_group_by_list_after_filter_with_menu_twin() {
    let data = RibbonTab::View
        .groups()
        .iter()
        .find(|g| g.label == TextId::RibbonGroupData)
        .unwrap();
    let cmds: Vec<RibbonCmd> = data.items.iter().map(|i| i.cmd).collect();
    let filter = cmds.iter().position(|c| *c == RibbonCmd::FilterMenu).unwrap();
    assert_eq!(cmds.get(filter + 1), Some(&RibbonCmd::GroupByMenu));
    assert_eq!(RibbonCmd::GroupByMenu.menu_path(), &[TextId::MenuView]);
    assert_eq!(RibbonCmd::GroupByMenu.menu_label(), TextId::MenuGroupBy);
}

/// H-UI-129 : « Lot » choisi au ruban ⇒ un en-tête par valeur (ordre
/// naturel, « (sans valeur) » en dernier), feuilles en ordre des ID,
/// récapitulatives masquées, début et fin du groupe ; une entrée
/// d'annulation ; le même choix une seconde fois n'écrit rien ; Annuler
/// rend la vue d'avant.
#[test]
fn h_ui_129_group_by_lot_from_ribbon_one_undo() {
    let mut harness = harness_lots();
    let before = history_len(harness.state());
    ribbon_menu(&mut harness, RibbonTab::View, &fr(TextId::RibbonGroupBy), "Lot");
    assert_eq!(harness.state().group_by(), Some(LOT));
    assert_eq!(history_len(harness.state()), before + 1);
    assert_eq!(
        lines(harness.state()),
        [
            header("Lot 2 (2 tâches)"),
            Ok(4),
            Ok(7),
            header("Lot 10 (2 tâches)"),
            Ok(2),
            Ok(3),
            header("(sans valeur) (1 tâche)"),
            Ok(6),
        ]
    );
    // Début et Fin du groupe : textes des tâches qui les portent.
    let app = harness.state();
    let rows = app.visible_rows().unwrap();
    let model = rows.model();
    let lot10 = rows.header(DisplayRow::at(3)).unwrap();
    let start_of = |n: u32| {
        model.text(
            model.index_of(uid(n)).unwrap(),
            plannercraft_ui_egui::column::Column::Start,
        )
    };
    let finish_of = |n: u32| {
        model.text(
            model.index_of(uid(n)).unwrap(),
            plannercraft_ui_egui::column::Column::Finish,
        )
    };
    assert_eq!(lot10.start, start_of(2));
    assert_eq!(lot10.finish, finish_of(3));
    assert!(lot10.bar.is_some());
    // Même choix : rien.
    dispatch(&mut harness, Action::Group(GroupOp::Set(Some(LOT))));
    assert_eq!(history_len(harness.state()), before + 1);
    dispatch(&mut harness, Action::Undo);
    assert_eq!(harness.state().group_by(), None);
    assert_eq!(harness.state().visible_rows().unwrap().len(), 7);
}

/// H-UI-129 : « Aucun groupe » sans regroupement n'écrit rien (le fichier
/// reste sans style) ; jumeau du menu Affichage → Regrouper par.
#[test]
fn h_ui_129_menu_twin_and_no_op() {
    let mut harness = harness_lots();
    let before = history_len(harness.state());
    dispatch(&mut harness, Action::Group(GroupOp::Set(None)));
    assert_eq!(history_len(harness.state()), before);
    click(&mut harness, &fr(TextId::MenuView));
    click(&mut harness, &fr(TextId::MenuGroupBy));
    click(&mut harness, &fr(TextId::GroupByCritical));
    assert_eq!(harness.state().group_by(), Some(GroupBy::Critical));
    let lines = lines(harness.state());
    assert!(
        matches!(lines.first(), Some(Err(name)) if name.starts_with("Critique (")),
        "{lines:?}"
    );
}

/// H-UI-129 : le filtre s'applique avant le groupement (Tâches critiques :
/// seules les feuilles critiques sont regroupées) ; l'impression suit
/// (tâches des groupes).
#[test]
fn h_ui_129_filter_before_grouping_and_print() {
    use plannercraft_automation::filter::FilterPreset;
    use plannercraft_ui_egui::state::FilterChoice;
    use plannercraft_ui_egui::view::task_view::TaskViewOp;
    let mut harness = harness_lots();
    dispatch(
        &mut harness,
        Action::TaskView(TaskViewOp::SetFilter(FilterChoice::Preset(
            FilterPreset::Critical,
        ))),
    );
    dispatch(&mut harness, Action::Group(GroupOp::Set(Some(LOT))));
    let app = harness.state();
    let schedule = app.document.as_ref().unwrap().schedule();
    let critical = |n: u32| {
        matches!(
            schedule.tasks.get(&uid(n)),
            Some(plannercraft_core::TaskOutcome::Scheduled(t)) if t.critical
        )
    };
    let expected: Vec<u32> = [2, 3, 4, 6, 7].into_iter().filter(|n| critical(*n)).collect();
    assert!(!expected.is_empty() && expected.len() < 5, "{expected:?}");
    let mut grouped: Vec<u32> = lines(app).into_iter().filter_map(Result::ok).collect();
    grouped.sort_unstable();
    assert_eq!(grouped, expected);
    let printed: Vec<u32> = app
        .printed_rows()
        .unwrap()
        .into_iter()
        .map(TaskUid::get)
        .collect();
    assert_eq!(printed, expected);
}

/// H-UI-130 : pendant le regroupement, Insérer, Indenter, Désindenter et
/// les insertions sont grisés avec la raison ; un geste de structure
/// arrivé quand même est refusé avec le message, rien d'écrit ; la saisie
/// d'une cellule reste permise (une entrée).
#[test]
fn h_ui_130_structure_locked_cells_editable() {
    let mut harness = harness_lots();
    dispatch(&mut harness, Action::Group(GroupOp::Set(Some(LOT))));
    dispatch(&mut harness, Action::Select(Some(uid(3))));
    let before = history_len(harness.state());
    let tasks = harness.state().document.as_ref().unwrap().project().tasks.len();
    for op in [
        StructureOp::InsertBefore,
        StructureOp::Indent,
        StructureOp::Outdent,
        StructureOp::MoveUp,
        StructureOp::InsertMilestone,
    ] {
        dispatch(&mut harness, Action::Structure(op));
        assert_eq!(history_len(harness.state()), before, "{op:?}");
    }
    assert_eq!(
        harness.state().document.as_ref().unwrap().project().tasks.len(),
        tasks
    );
    let reason = fr(TextId::ViewGroupLocked);
    assert!(
        harness.state().messages().iter().any(|m| m.text == reason),
        "{:?}",
        harness.state().messages()
    );
    // Ruban : Abaisser grisé (onglet Tâche affiché).
    {
        use egui_kittest::kittest::NodeT as _;
        let label = fr(TextId::RibbonIndent);
        let disabled: Vec<bool> = harness
            .query_all_by_label(&label)
            .map(|n| n.accesskit_node().is_disabled())
            .collect();
        assert!(
            !disabled.is_empty() && disabled.iter().all(|d| *d),
            "{disabled:?}"
        );
    }
    // Saisie d'une cellule : nom de 3 changé, une entrée.
    dispatch(&mut harness, Action::FocusPane(Pane::Table));
    harness.event(Event::Text("Voiles".to_owned()));
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();
    let name = &harness
        .state()
        .document
        .as_ref()
        .unwrap()
        .project()
        .tasks
        .get(uid(3))
        .unwrap()
        .name;
    assert_eq!(name, "Voiles");
    assert_eq!(history_len(harness.state()), before + 1);
    // Couper des lignes entières : refusé, rien d'écrit ni de copié.
    dispatch(
        &mut harness,
        Action::SelectOp(plannercraft_ui_egui::action::SelectOp::All),
    );
    let n = history_len(harness.state());
    dispatch(
        &mut harness,
        Action::Clipboard(plannercraft_ui_egui::clipboard::ClipboardOp::Cut),
    );
    assert_eq!(history_len(harness.state()), n);
    assert_eq!(
        harness.state().document.as_ref().unwrap().project().tasks.len(),
        tasks
    );
    assert!(harness.state().clipboard().is_none());
    // Retirer le regroupement rend les gestes.
    dispatch(&mut harness, Action::Group(GroupOp::Set(None)));
    dispatch(&mut harness, Action::Select(Some(uid(3))));
    let n = history_len(harness.state());
    dispatch(&mut harness, Action::Structure(StructureOp::MoveUp));
    assert_eq!(history_len(harness.state()), n + 1);
}

/// H-UI-131 : un clic sur un en-tête replie le groupe (ses tâches
/// masquées, rien d'écrit), un second le déplie ; la flèche haut depuis la
/// première tâche d'un groupe saute l'en-tête vers la dernière tâche du
/// groupe précédent ; la flèche haut sous le premier en-tête ne bouge pas.
#[test]
fn h_ui_131_fold_and_keyboard_skip_headers() {
    let mut harness = harness_lots();
    dispatch(&mut harness, Action::Group(GroupOp::Set(Some(LOT))));
    let before = history_len(harness.state());
    click(&mut harness, &fr_args(TextId::GroupCollapse, "Lot 2 (2 tâches)"));
    assert_eq!(
        lines(harness.state())[..2],
        [header("Lot 2 (2 tâches)"), header("Lot 10 (2 tâches)")]
    );
    assert_eq!(history_len(harness.state()), before, "replier n'écrit rien");
    click(&mut harness, &fr_args(TextId::GroupExpand, "Lot 2 (2 tâches)"));
    assert_eq!(lines(harness.state()).len(), 8);
    // Clavier : 2 (première de Lot 10) ⇒ ↑ ⇒ 7.
    dispatch(&mut harness, Action::Select(Some(uid(2))));
    dispatch(&mut harness, Action::FocusPane(Pane::Table));
    harness.key_press_modifiers(Modifiers::NONE, Key::ArrowUp);
    harness.run();
    assert_eq!(harness.state().selection, Some(uid(7)));
    harness.key_press_modifiers(Modifiers::NONE, Key::ArrowUp);
    harness.run();
    assert_eq!(harness.state().selection, Some(uid(4)));
    harness.key_press_modifiers(Modifiers::NONE, Key::ArrowUp);
    harness.run();
    assert_eq!(
        harness.state().selection,
        Some(uid(4)),
        "en-tête du haut : rien ne bouge"
    );
    // ↓ depuis 7 saute l'en-tête de Lot 10.
    dispatch(&mut harness, Action::Select(Some(uid(7))));
    harness.key_press_modifiers(Modifiers::NONE, Key::ArrowDown);
    harness.run();
    assert_eq!(harness.state().selection, Some(uid(2)));
}

/// H-UI-131 : regroupement par ressource, une tâche à deux ressources est
/// sous chacune ; ↓ la parcourt sous les deux groupes sans revenir en
/// arrière ; Tout sélectionner la compte une fois.
#[test]
fn h_ui_131_repeated_task_navigation_and_select_all() {
    let mut harness = harness_lots();
    let batch = plannercraft_automation::Batch::from_json(serde_json::json!({
        "format_version": 1,
        "ops": [
            { "op": "resource.add", "name": "Maçon", "as": "$m" },
            { "op": "resource.add", "name": "Plâtrier", "as": "$p" },
            { "op": "assignment.add", "task": 3, "resource": "$m" },
            { "op": "assignment.add", "task": 3, "resource": "$p" },
            { "op": "assignment.add", "task": 6, "resource": "$p" }
        ]
    }))
    .unwrap();
    dispatch(&mut harness, Action::Exec(batch));
    dispatch(&mut harness, Action::Group(GroupOp::Set(Some(GroupBy::Resource))));
    assert_eq!(
        lines(harness.state()),
        [
            header("Maçon (1 tâche)"),
            Ok(3),
            header("Plâtrier (2 tâches)"),
            Ok(3),
            Ok(6),
            header("(sans valeur) (3 tâches)"),
            Ok(2),
            Ok(4),
            Ok(7),
        ]
    );
    dispatch(&mut harness, Action::Select(Some(uid(3))));
    dispatch(&mut harness, Action::FocusPane(Pane::Table));
    let mut seen = Vec::new();
    for _ in 0..4 {
        harness.key_press_modifiers(Modifiers::NONE, Key::ArrowDown);
        harness.run();
        seen.push(harness.state().selection.unwrap().get());
    }
    assert_eq!(seen, [3, 6, 2, 4]);
    dispatch(
        &mut harness,
        Action::SelectOp(plannercraft_ui_egui::action::SelectOp::All),
    );
    assert_eq!(harness.state().selected().len(), 5);
    assert_eq!(harness.state().selected().click_order().len(), 5);
}

/// Libellé français d'un texte à `$name`.
fn fr_args(id: TextId, name: &str) -> String {
    plannercraft_ui_egui::i18n::Texts::new(plannercraft_ui_egui::i18n::Language::Fr).get_args(
        id,
        &[(
            "name",
            plannercraft_ui_egui::i18n::Arg::Str(name.to_owned().into()),
        )],
    )
}

/// H-UI-129 : le regroupement est gardé dans le document (enregistrer et
/// rouvrir ⇒ même vue regroupée).
#[test]
fn h_ui_129_grouping_saved_with_document() {
    let mut harness = harness_lots();
    dispatch(&mut harness, Action::Group(GroupOp::Set(Some(LOT))));
    let doc = harness.state().document.as_ref().unwrap();
    let xml = doc
        .session
        .save_bytes(&plannercraft_mspdi::write::WriteOptions::default())
        .xml;
    let origin = plannercraft_automation::session::DocOrigin {
        name: "p.xml".to_owned(),
        path: None,
    };
    let (session, _) = plannercraft_automation::Session::open_bytes(
        &xml,
        origin,
        "2026-10-19T08:00".parse().unwrap(),
        plannercraft_automation::text::Language::Fr,
    )
    .unwrap();
    assert_eq!(
        plannercraft_automation::command::group_by::group_by_of(session.project()),
        Some(LOT)
    );
}
