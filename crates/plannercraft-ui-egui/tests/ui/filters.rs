//! Filtres et tri dans l'interface (J8, lot L9) : H-UI-30 (un filtre est
//! une vue ; ligne éditée gardée jusqu'à la réapplication ; F3), H-UI-31
//! (tri des frères, Maj+F3), H-UI-32 (structure grisée sous tri ou
//! filtre), préférences de la vue persistées (niveau seulement) et P50
//! (opérations de vue générées : ni génération ni octets changés).
//!
//! Projet « P » du cahier J8 (§ 2) : 1 « Gros œuvre » ⊃ 2, 3, 4 ; 5
//! « Second œuvre » ⊃ 6, 7 ; chemin critique 2, 3, 6, 7.

#![allow(
    clippy::arithmetic_side_effects,
    reason = "tests : compteurs, un débordement doit paniquer"
)]

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use plannercraft_automation::Batch;
use plannercraft_automation::filter::{FilterPreset, TaskFilter};
use plannercraft_core::calendar::BuiltinCalendarNames;
use plannercraft_core::{
    Duration, DurationUnit, Lag, Link, LinkType, Minutes, OutlineLevel, Project, ProjectInit, ScheduleAnchor,
    Task, TaskUid, TimeBasis,
};
use plannercraft_mspdi::write::WriteOptions;
use plannercraft_ui_egui::dialog::filter::FilterDialogMode;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::keymap::{Binding, KEYMAP, UiBinding};
use plannercraft_ui_egui::row_view::DisplayRow;
use plannercraft_ui_egui::state::{FilterChoice, SortKey, SortSpec};
use plannercraft_ui_egui::structure::StructureOp;
use plannercraft_ui_egui::view::task_view::TaskViewOp;
use plannercraft_ui_egui::{Action, App, DocOrigin, UiState};
use proptest::prelude::*;

use crate::{click, fr, fr_light, harness, harness_for, new_app, now};

/// Projet « P » du cahier J8 (repère des exemples), début lun 19/10/2026.
pub fn project_p() -> Project {
    let start = "2026-10-19T08:00".parse().unwrap();
    let mut p = Project::new(ProjectInit {
        name: "P".to_owned(),
        anchor: ScheduleAnchor::FromStart(start),
        current_date: start,
        calendar_names: BuiltinCalendarNames::default(),
    });
    let mut add = |name: &str, days: i64, level: u16| {
        let uid = TaskUid::new(u32::try_from(p.tasks.len() + 1).unwrap());
        let options = p.options;
        let mut t = Task::new(uid, name.to_owned(), &options);
        t.outline_level = OutlineLevel::new(level).unwrap();
        t.duration = Duration::new(
            Minutes::new(days * 480),
            DurationUnit::Day,
            TimeBasis::Working,
            false,
        )
        .unwrap();
        p.tasks.push(t).unwrap();
        uid
    };
    add("Gros œuvre", 0, 1);
    let t2 = add("Fondations", 5, 2);
    let t3 = add("Murs", 10, 2);
    let t4 = add("Livraison", 0, 2);
    add("Second œuvre", 0, 1);
    let t6 = add("Plâtrerie", 4, 2);
    let t7 = add("Peinture", 3, 2);
    for (pred, succ) in [(t2, t3), (t3, t4), (t3, t6), (t6, t7)] {
        p.tasks.get_mut(succ).unwrap().predecessors.push(Link {
            predecessor: pred,
            kind: LinkType::FinishToStart,
            lag: Lag::ZERO,
        });
    }
    p
}

/// Ouvre P dans l'application.
pub fn open_p_in(app: &mut App) {
    app.open_project(
        project_p(),
        DocOrigin {
            name: "p.xml".to_owned(),
            path: None,
            demo: false,
        },
    );
}

/// Harnais ouvert sur P.
pub fn harness_p() -> Harness<'static, App> {
    let mut app = new_app();
    open_p_in(&mut app);
    let mut harness = harness_for(app);
    harness.run();
    harness
}

/// UID (numéros) des lignes affichées, dans l'ordre.
pub fn shown(app: &App) -> Vec<u32> {
    let rows = app.visible_rows().unwrap();
    (0..rows.len())
        .map(|d| rows.uid(DisplayRow::at(d)).unwrap().get())
        .collect()
}

/// Applique une commande de la vue et laisse l'interface se stabiliser.
pub fn view_op(harness: &mut Harness<'static, App>, op: TaskViewOp) {
    harness.state_mut().dispatch(Action::TaskView(op));
    harness.run();
}

fn preset(p: FilterPreset) -> TaskViewOp {
    TaskViewOp::SetFilter(FilterChoice::Preset(p))
}

/// Octets enregistrés et génération du document ouvert.
fn written(app: &App) -> (Vec<u8>, u64) {
    let doc = app.document.as_ref().unwrap();
    (
        doc.session.save_bytes(&WriteOptions::default()).xml,
        doc.generation(),
    )
}

fn exec(harness: &mut Harness<'static, App>, json: &str) {
    let batch = Batch::from_json_str(json).unwrap();
    harness.state_mut().dispatch(Action::Exec(batch));
    harness.run();
}

/// H-UI-30 : « Tâches critiques » choisi au menu Affichage → Filtre ;
/// rien n'est écrit, récapitulatives liées affichées, indicateur de la
/// barre d'état.
#[test]
fn h_ui_30_filter_is_view() {
    let mut harness = harness_p();
    let before = written(harness.state());
    click(&mut harness, &fr(TextId::MenuView));
    click(&mut harness, &fr(TextId::MenuFilter));
    click(&mut harness, &fr(TextId::FilterCritical));
    assert_eq!(shown(harness.state()), [1, 2, 3, 5, 6, 7]);
    assert_eq!(written(harness.state()), before);
    let indicator = Texts::new(Language::Fr).get_args(
        TextId::StatusFilter,
        &[("name", Arg::Str(fr(TextId::FilterCritical).into()))],
    );
    assert!(harness.query_by_label(&indicator).is_some(), "indicateur absent");
    // « Inachevées » avec 2 à 100 % : 3, 4, 6, 7 et les récapitulatives 1, 5.
    exec(
        &mut harness,
        r#"{"format_version":1,"ops":[{"op":"progress.update","task":2,"percent_complete":{"value":10000}}]}"#,
    );
    view_op(&mut harness, preset(FilterPreset::Incomplete));
    assert_eq!(shown(harness.state()), [1, 3, 4, 5, 6, 7]);
}

/// H-UI-30 : sous « Inachevées », 7 passée à 100 % reste affichée ;
/// réappliquer le filtre la retire. Une tâche insérée sous le filtre reste
/// visible aussi.
#[test]
fn h_ui_30_edited_row_stays_until_reapply() {
    let mut harness = harness_p();
    view_op(&mut harness, preset(FilterPreset::Incomplete));
    assert_eq!(shown(harness.state()), [1, 2, 3, 4, 5, 6, 7]);
    exec(
        &mut harness,
        r#"{"format_version":1,"ops":[{"op":"progress.update","task":7,"percent_complete":{"value":10000}}]}"#,
    );
    assert_eq!(shown(harness.state()), [1, 2, 3, 4, 5, 6, 7], "7 doit rester");
    // Encore une édition : 7 reste gardée.
    exec(
        &mut harness,
        r#"{"format_version":1,"ops":[{"op":"task.update","uid":2,"name":"Fondations bis"}]}"#,
    );
    assert!(shown(harness.state()).contains(&7));
    view_op(&mut harness, TaskViewOp::ReapplyFilter);
    assert_eq!(shown(harness.state()), [1, 2, 3, 4, 5, 6]);
    // Le même filtre choisi au menu le réapplique aussi.
    exec(
        &mut harness,
        r#"{"format_version":1,"ops":[{"op":"progress.update","task":6,"percent_complete":{"value":10000}}]}"#,
    );
    assert!(shown(harness.state()).contains(&6));
    view_op(&mut harness, preset(FilterPreset::Incomplete));
    assert!(!shown(harness.state()).contains(&6));
}

/// H-UI-30 : F3 retire le filtre (S79), le menu le propose grisé sans
/// filtre.
#[test]
fn h_ui_30_f3_clears() {
    let mut harness = harness_p();
    view_op(&mut harness, preset(FilterPreset::Summaries));
    assert_eq!(shown(harness.state()), [1, 5]);
    harness.key_press(Key::F3);
    harness.run();
    assert!(!harness.state().task_view().is_filtered());
    assert_eq!(shown(harness.state()).len(), 7);
}

/// H-UI-31 : fenêtre « Trier » (Début par défaut, structure conservée),
/// Décroissant ⇒ 3 avant 2 sous 1, 5 avant 1 ; ID inchangés.
#[test]
fn h_ui_31_sort_keep_outline() {
    let mut harness = harness_p();
    let before = written(harness.state());
    click(&mut harness, &fr(TextId::MenuView));
    click(&mut harness, &fr(TextId::MenuSort));
    let dialog = *harness.state().sort_dialog().expect("fenêtre ouverte");
    assert_eq!(dialog.key, SortKey::Start);
    assert!(dialog.keep_outline);
    click(&mut harness, &fr(TextId::SortDescending));
    click(&mut harness, &fr(TextId::SortApply));
    assert!(harness.state().sort_dialog().is_none());
    assert_eq!(
        harness.state().task_view().sort,
        Some(SortSpec {
            key: SortKey::Start,
            descending: true,
            keep_outline: true,
        })
    );
    let order = shown(harness.state());
    assert_eq!(order, [5, 7, 6, 1, 4, 3, 2]);
    // ID inchangés : la ligne de 3 porte toujours l'ID 3.
    let rows = harness.state().visible_rows().unwrap();
    let d = rows.display_of(TaskUid::new(3)).unwrap();
    assert_eq!(rows.row(d).unwrap().id, 3);
    assert_eq!(written(harness.state()), before);
}

/// H-UI-31 : Maj+F3 revient à l'ordre des ID.
#[test]
fn h_ui_31_shift_f3_resets() {
    let mut harness = harness_p();
    view_op(
        &mut harness,
        TaskViewOp::SetSort(Some(SortSpec {
            key: SortKey::Name,
            descending: false,
            keep_outline: false,
        })),
    );
    assert_ne!(shown(harness.state()), [1, 2, 3, 4, 5, 6, 7]);
    harness.key_press_modifiers(Modifiers::SHIFT, Key::F3);
    harness.run();
    assert_eq!(harness.state().task_view().sort, None);
    assert_eq!(shown(harness.state()), [1, 2, 3, 4, 5, 6, 7]);
}

/// H-UI-32 : sous tri, Alt+Maj+↑ et Abaisser ne font rien (message),
/// Insérer reste permis ; l'entrée du menu Édition est grisée.
#[test]
fn h_ui_32_structure_disabled_under_sort() {
    let mut harness = harness_p();
    view_op(
        &mut harness,
        TaskViewOp::SetSort(Some(SortSpec {
            key: SortKey::Start,
            descending: true,
            keep_outline: true,
        })),
    );
    harness
        .state_mut()
        .dispatch(Action::Select(Some(TaskUid::new(3))));
    harness.run();
    let generation = written(harness.state()).1;
    harness.key_press_modifiers(Modifiers::ALT | Modifiers::SHIFT, Key::ArrowUp);
    harness.run();
    harness
        .state_mut()
        .dispatch(Action::Structure(StructureOp::Indent));
    harness.run();
    assert_eq!(written(harness.state()).1, generation, "rien ne doit bouger");
    let locked = fr(TextId::ViewReorderLocked);
    assert!(
        harness.state().messages().iter().any(|m| m.text == locked),
        "{:?}",
        harness.state().messages()
    );
    click(&mut harness, &fr(TextId::MenuEdit));
    let (indent_label, insert_label) = (fr(TextId::StructureIndent), fr(TextId::StructureInsert));
    let indent = harness.get_by(|node| node.label().is_some_and(|l| l.starts_with(&indent_label)));
    assert!(indent.accesskit_node().is_disabled(), "Abaisser doit être grisé");
    let insert = harness.get_by(|node| node.label().is_some_and(|l| l.starts_with(&insert_label)));
    assert!(!insert.accesskit_node().is_disabled());
    harness.key_press(Key::Escape);
    harness.run();
    harness
        .state_mut()
        .dispatch(Action::Structure(StructureOp::InsertBefore));
    harness.run();
    assert_eq!(written(harness.state()).1, generation + 1, "Insérer reste permis");
    // Sans tri ni filtre : de nouveau permis.
    view_op(&mut harness, TaskViewOp::ResetSort);
    assert!(!harness.state().structure_locked(StructureOp::Indent));
}

/// Fenêtre « Filtre personnalisé » : case « Tâches critiques » cochée,
/// Appliquer ⇒ filtre personnalisé critique ; « Utilisant la ressource »
/// sans ressource ⇒ refus affiché dans la fenêtre.
#[test]
fn filter_dialog_custom_and_resource() {
    let mut harness = harness_p();
    view_op(
        &mut harness,
        TaskViewOp::OpenFilterDialog(FilterDialogMode::Custom),
    );
    click(&mut harness, &fr(TextId::FilterCritical));
    click(&mut harness, &fr(TextId::FilterApply));
    assert!(harness.state().filter_dialog().is_none());
    let expected = TaskFilter {
        critical: Some(true),
        show_related_summaries: true,
        ..TaskFilter::default()
    };
    assert_eq!(harness.state().task_view().filter, FilterChoice::Custom(expected));
    assert_eq!(shown(harness.state()), [1, 2, 3, 5, 6, 7]);
    view_op(
        &mut harness,
        TaskViewOp::OpenFilterDialog(FilterDialogMode::Resource),
    );
    click(&mut harness, &fr(TextId::FilterApply));
    let dialog = harness.state().filter_dialog().expect("refus : fenêtre ouverte");
    assert_eq!(
        dialog.error.as_deref(),
        Some(fr(TextId::FilterResourceMissing).as_str())
    );
    harness.key_press(Key::Escape);
    harness.run();
    assert!(harness.state().filter_dialog().is_none());
}

/// Niveau affiché gardé dans les préférences (relu à la prochaine
/// ouverture) ; filtre et tri ne le sont pas (ADR-0016 D2).
#[test]
fn task_view_persisted() {
    let mut harness = harness_p();
    view_op(&mut harness, TaskViewOp::SetOutlineLevel(Some(1)));
    view_op(&mut harness, preset(FilterPreset::Critical));
    let state = harness.state().state.clone();
    assert_eq!(state.task_view.outline_level, Some(1));
    let json = serde_json::to_string(&state).unwrap();
    let read: UiState = serde_json::from_str(&json).unwrap();
    let mut app = App::new(read, plannercraft_ui_egui::Env::fixed(now()));
    open_p_in(&mut app);
    assert_eq!(app.task_view().outline_level, Some(1));
    assert!(!app.task_view().is_filtered());
    assert_eq!(shown(&app), [1, 5]);
    // « Tous » efface la préférence.
    let mut app = App::new(fr_light(), plannercraft_ui_egui::Env::fixed(now()));
    open_p_in(&mut app);
    app.apply_task_view(TaskViewOp::SetOutlineLevel(Some(2)));
    app.apply_task_view(TaskViewOp::SetOutlineLevel(None));
    assert_eq!(app.state.task_view.outline_level, None);
}

/// Une commande de la vue, tirée au hasard.
fn arb_op() -> impl Strategy<Value = TaskViewOp> {
    prop_oneof![
        Just(preset(FilterPreset::Critical)),
        Just(preset(FilterPreset::Incomplete)),
        Just(preset(FilterPreset::Milestones)),
        Just(preset(FilterPreset::Summaries)),
        Just(TaskViewOp::ClearFilter),
        Just(TaskViewOp::ReapplyFilter),
        (0usize..7, any::<bool>(), any::<bool>()).prop_map(|(k, descending, keep_outline)| {
            TaskViewOp::SetSort(Some(SortSpec {
                key: SortKey::ALL[k],
                descending,
                keep_outline,
            }))
        }),
        Just(TaskViewOp::ResetSort),
        (0u8..=10).prop_map(|l| TaskViewOp::SetOutlineLevel((l > 0).then_some(l))),
        (1u32..=7).prop_map(|u| TaskViewOp::ToggleCollapsed(TaskUid::new(u))),
        Just(TaskViewOp::HideSubtasks),
        Just(TaskViewOp::ShowSubtasks),
        Just(TaskViewOp::OpenSearch),
        "[a-zé]{0,4}".prop_map(TaskViewOp::SetQuery),
        Just(TaskViewOp::SearchNext),
        Just(TaskViewOp::SearchPrev),
        Just(TaskViewOp::CloseSearch),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// P50 (lot L9) : filtre, tri, repli et recherche, suite générée :
    /// génération et octets enregistrés inchangés, vue bornée.
    #[test]
    fn p50_view_ops_write_nothing(
        ops in prop::collection::vec(arb_op(), 1..24),
        selected in 1u32..=7,
    ) {
        let mut app = App::new(fr_light(), plannercraft_ui_egui::Env::fixed(now()));
        open_p_in(&mut app);
        app.selection = Some(TaskUid::new(selected));
        let before = written(&app);
        for op in ops {
            app.apply_task_view(op);
            prop_assert!(app.row_view().len() <= 7);
        }
        prop_assert_eq!(written(&app), before);
    }
}

/// Entrées de `KEYMAP` du lot L9 (vérifiées par [`check_keymap_entry`]).
pub fn is_view_entry(binding: Binding) -> bool {
    matches!(
        binding,
        Binding::Ui(
            UiBinding::ClearFilter
                | UiBinding::SortById
                | UiBinding::Find
                | UiBinding::FindNext
                | UiBinding::FindPrevious
                | UiBinding::HideSubtasks
                | UiBinding::ShowSubtasks
        )
    )
}

/// Vérifie l'entrée `i` de `KEYMAP` (lot L9) sur P.
pub fn check_keymap_entry(i: usize) {
    let (_, shortcut, binding) = KEYMAP[i];
    let mut harness = harness_p();
    let press = |harness: &mut Harness<'static, App>| {
        harness.key_press_modifiers(shortcut.modifiers, shortcut.logical_key);
        harness.run();
    };
    match binding {
        Binding::Ui(UiBinding::ClearFilter) => {
            view_op(&mut harness, preset(FilterPreset::Critical));
            press(&mut harness);
            assert!(!harness.state().task_view().is_filtered());
        }
        Binding::Ui(UiBinding::SortById) => {
            view_op(
                &mut harness,
                TaskViewOp::SetSort(Some(SortSpec {
                    key: SortKey::Name,
                    descending: true,
                    keep_outline: false,
                })),
            );
            press(&mut harness);
            assert_eq!(harness.state().task_view().sort, None);
        }
        Binding::Ui(UiBinding::Find) => {
            press(&mut harness);
            assert!(harness.state().search().is_some());
        }
        Binding::Ui(UiBinding::FindNext | UiBinding::FindPrevious) => {
            view_op(&mut harness, TaskViewOp::OpenSearch);
            view_op(&mut harness, TaskViewOp::SetQuery("e".to_owned()));
            // Rend le focus à la table : le champ de recherche lit lui-même
            // ses touches.
            harness
                .state_mut()
                .dispatch(Action::FocusPane(plannercraft_ui_egui::view::Pane::Table));
            harness.run();
            let before = harness.state().selection;
            press(&mut harness);
            assert_ne!(harness.state().selection, before, "{shortcut:?}");
        }
        Binding::Ui(UiBinding::HideSubtasks) => {
            harness
                .state_mut()
                .dispatch(Action::Select(Some(TaskUid::new(1))));
            harness.run();
            press(&mut harness);
            assert_eq!(shown(harness.state()), [1, 5, 6, 7]);
        }
        Binding::Ui(UiBinding::ShowSubtasks) => {
            harness
                .state_mut()
                .dispatch(Action::Select(Some(TaskUid::new(1))));
            harness.run();
            view_op(&mut harness, TaskViewOp::HideSubtasks);
            press(&mut harness);
            assert_eq!(shown(harness.state()).len(), 7, "{shortcut:?}");
        }
        other => panic!("entrée {i} hors du lot L9 : {other:?}"),
    }
}

/// Harnais d'accueil sans document : les raccourcis du lot sont sans effet
/// (vérifié par `session.rs`) ; ici, le menu Affichage les liste grisés.
#[test]
fn view_menu_disabled_without_document() {
    let mut harness = harness();
    click(&mut harness, &fr(TextId::MenuView));
    let label = fr(TextId::MenuSort);
    let sort = harness.get_by_label(&label);
    assert!(sort.accesskit_node().is_disabled());
}

/// « Plage de dates… » : deux jours lus par `parse_date` (R-143) ⇒ filtre
/// prédéfini ; plage inversée, jour manquant ou date illisible ⇒ refus.
#[test]
fn filter_dialog_date_range() {
    use plannercraft_automation::text::Language as TextLanguage;
    use plannercraft_ui_egui::dialog::filter::FilterDialog;
    let p = project_p();
    let texts = Texts::new(Language::Fr);
    let lang = TextLanguage::Fr;
    let mut d = FilterDialog::open(
        &p,
        lang,
        FilterDialogMode::DateRange,
        &FilterChoice::Preset(FilterPreset::All),
    );
    d.from = "26/10/2026".to_owned();
    d.to = "30/10/2026".to_owned();
    let from = "2026-10-26T08:00"
        .parse::<plannercraft_core::Moment>()
        .unwrap()
        .day();
    let to = "2026-10-30T08:00"
        .parse::<plannercraft_core::Moment>()
        .unwrap()
        .day();
    assert_eq!(
        d.to_choice(&p, &texts, lang),
        Ok(FilterChoice::Preset(FilterPreset::DateRange { from, to }))
    );
    // Rouvert sur ce filtre : les jours sont relus.
    let again = FilterDialog::open(
        &p,
        lang,
        FilterDialogMode::DateRange,
        &FilterChoice::Preset(FilterPreset::DateRange { from, to }),
    );
    assert_eq!(
        again.to_choice(&p, &texts, lang),
        Ok(FilterChoice::Preset(FilterPreset::DateRange { from, to }))
    );
    d.to = "20/10/2026".to_owned();
    assert_eq!(d.to_choice(&p, &texts, lang), Err(fr(TextId::FilterDateOrder)));
    d.to = String::new();
    assert_eq!(
        d.to_choice(&p, &texts, lang),
        Err(fr(TextId::FilterDatesRequired))
    );
    d.to = "pas une date".to_owned();
    assert!(d.to_choice(&p, &texts, lang).is_err());
    // Filtre personnalisé sans critère : « Toutes les tâches ».
    d.mode = FilterDialogMode::Custom;
    d.from.clear();
    d.to.clear();
    assert_eq!(
        d.to_choice(&p, &texts, lang),
        Ok(FilterChoice::Preset(FilterPreset::All))
    );
}
