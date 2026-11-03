//! Commandes nouvelles du ruban (ADR-0017 D4, lot L3) : Insérer → Jalon et
//! Récapitulative (H-UI-83), Ajouter à la chronologie (H-UI-79),
//! Défiler jusqu'à la tâche et Notes (H-UI-84).
//!
//! Projet « P » du cahier (§ 2) : 1 « Gros œuvre » ⊃ 2, 3, 4 ; 5 « Second
//! œuvre » ⊃ 6, 7. Chaque commande est un lot ordinaire : une entrée
//! d'annulation, Ctrl+Z rend P.

#![allow(
    clippy::arithmetic_side_effects,
    reason = "tests : compteurs, un débordement doit paniquer"
)]

use egui_kittest::Harness;
use plannercraft_automation::command::style::doc_style;
use plannercraft_core::calendar::BuiltinCalendarNames;
use plannercraft_core::{
    Duration, DurationUnit, Minutes, OutlineLevel, Project, ProjectInit, ScheduleAnchor, Task, TaskUid,
    TimeBasis,
};
use plannercraft_render::style::TemplateId;
use plannercraft_ui_egui::action::{DialogKind, TimelineOp};
use plannercraft_ui_egui::dialog::DialogBody;
use plannercraft_ui_egui::dialog::task_info::TaskInfoTab;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::selection::SelectMode;
use plannercraft_ui_egui::state::{SortKey, SortSpec};
use plannercraft_ui_egui::structure::StructureOp;
use plannercraft_ui_egui::view::task_view::TaskViewOp;
use plannercraft_ui_egui::{Action, App, DocOrigin, TimeScaleZoom};

use crate::filters::{harness_p, view_op};
use crate::{fr, harness_for, new_app, screen_axis};

fn uid(n: u32) -> TaskUid {
    TaskUid::new(n)
}

/// Sélectionne `first`, puis étend jusqu'à `last` (Maj+clic).
fn select_range(harness: &mut Harness<'static, App>, first: u32, last: u32) {
    harness.state_mut().dispatch(Action::Select(Some(uid(first))));
    if last != first {
        harness.state_mut().dispatch(Action::SelectWith {
            uid: uid(last),
            mode: SelectMode::Range,
        });
    }
    harness.run();
}

fn dispatch(harness: &mut Harness<'static, App>, action: Action) {
    harness.state_mut().dispatch(action);
    harness.run();
}

fn project(app: &App) -> &Project {
    app.document.as_ref().unwrap().project()
}

/// (ID, nom, niveau) de chaque tâche, dans l'ordre.
fn outline(app: &App) -> Vec<(usize, String, u16)> {
    project(app)
        .tasks
        .iter()
        .enumerate()
        .map(|(i, t)| (i + 1, t.name.clone(), t.outline_level.get()))
        .collect()
}

fn history_len(app: &App) -> usize {
    app.document.as_ref().unwrap().session.history().entries.len()
}

fn last_message(app: &App) -> Option<String> {
    app.messages().last().map(|m| m.text.clone())
}

/// UID de la chronologie (style effectif du document).
fn timeline(app: &App) -> Vec<u32> {
    app.state.effective_style(project(app)).timeline.tasks
}

/// H-UI-83 : Insérer → Jalon avec 6 sélectionnée ⇒ jalon « Nouveau
/// jalon » de durée nulle avant 6 (même niveau), sélectionné ; une seule
/// annulation rend P.
#[test]
fn h_ui_83_insert_milestone() {
    let mut harness = harness_p();
    let before = project(harness.state()).clone();
    select_range(&mut harness, 6, 6);
    dispatch(&mut harness, Action::Structure(StructureOp::InsertMilestone));
    let app = harness.state();
    assert_eq!(history_len(app), 1);
    let p = project(app);
    let created = p.tasks.at(5).unwrap();
    assert_eq!(created.name, fr(TextId::NewMilestoneName));
    assert!(created.milestone);
    assert!(created.duration.is_zero());
    assert_eq!(created.outline_level.get(), 2);
    assert_eq!(p.tasks.at(6).unwrap().uid(), uid(6), "6 suit le jalon");
    assert_eq!(app.selection, Some(created.uid()));
    dispatch(&mut harness, Action::Undo);
    assert_eq!(project(harness.state()), &before);
}

/// H-UI-83 (exemple du cahier) : sélection {6, 7}, Récapitulative ⇒ 6
/// « Nouvelle tâche récapitulative » (niveau 2, sous 5) ⊃ 7 « Plâtrerie »,
/// 8 « Peinture » ; un lot, Ctrl+Z ⇒ P.
#[test]
fn h_ui_83_insert_summary_one_undo() {
    let mut harness = harness_p();
    let before = project(harness.state()).clone();
    select_range(&mut harness, 6, 7);
    dispatch(&mut harness, Action::Structure(StructureOp::InsertSummary));
    let app = harness.state();
    assert_eq!(history_len(app), 1, "un seul lot");
    let got = outline(app);
    assert_eq!(got.len(), 8);
    assert_eq!(got[4], (5, "Second œuvre".to_owned(), 1));
    assert_eq!(got[5], (6, fr(TextId::NewSummaryName), 2));
    assert_eq!(got[6], (7, "Plâtrerie".to_owned(), 3));
    assert_eq!(got[7], (8, "Peinture".to_owned(), 3));
    dispatch(&mut harness, Action::Undo);
    assert_eq!(project(harness.state()), &before);
}

/// H-UI-83 : sans sélection, la récapitulative vide va en fin de projet.
#[test]
fn h_ui_83_insert_summary_no_selection_at_end() {
    let mut harness = harness_p();
    dispatch(&mut harness, Action::Select(None));
    dispatch(&mut harness, Action::Structure(StructureOp::InsertSummary));
    let got = outline(harness.state());
    assert_eq!(got.len(), 8);
    assert_eq!(got[7], (8, fr(TextId::NewSummaryName), 1));
    assert_eq!(got[6].2, 2, "Peinture inchangée");
    // Jalon sans sélection : fin du projet aussi.
    dispatch(&mut harness, Action::Select(None));
    dispatch(&mut harness, Action::Structure(StructureOp::InsertMilestone));
    let got = outline(harness.state());
    assert_eq!(got[8], (9, fr(TextId::NewMilestoneName), 1));
}

/// H-UI-83 : {3, 6} ne sont pas voisines ⇒ refus, message, rien d'écrit ;
/// {4, 5} non plus (niveaux différents).
#[test]
fn h_ui_83_insert_summary_non_contiguous_refused() {
    let mut harness = harness_p();
    let before = project(harness.state()).clone();
    for (a, b) in [(3, 6), (4, 5)] {
        dispatch(&mut harness, Action::Select(Some(uid(a))));
        dispatch(
            &mut harness,
            Action::SelectWith {
                uid: uid(b),
                mode: SelectMode::Toggle,
            },
        );
        dispatch(&mut harness, Action::DismissMessage);
        dispatch(&mut harness, Action::Structure(StructureOp::InsertSummary));
        assert_eq!(
            last_message(harness.state()),
            Some(fr(TextId::InsertSummaryNotAdjacent)),
            "{a}, {b}"
        );
        assert_eq!(project(harness.state()), &before);
        assert_eq!(history_len(harness.state()), 0);
    }
}

/// H-UI-83 : sous tri, Jalon et Récapitulative sont refusés (H-UI-32) ;
/// le ruban les grise avec la même raison.
#[test]
fn h_ui_83_locked_under_sort() {
    let mut harness = harness_p();
    view_op(
        &mut harness,
        TaskViewOp::SetSort(Some(SortSpec {
            key: SortKey::Start,
            descending: true,
            keep_outline: true,
        })),
    );
    select_range(&mut harness, 6, 6);
    let before = project(harness.state()).clone();
    for op in [StructureOp::InsertMilestone, StructureOp::InsertSummary] {
        dispatch(&mut harness, Action::DismissMessage);
        dispatch(&mut harness, Action::Structure(op));
        assert_eq!(
            last_message(harness.state()),
            Some(fr(TextId::ViewReorderLocked)),
            "{op:?}"
        );
        assert_eq!(project(harness.state()), &before, "{op:?}");
    }
    // Sans tri : de nouveau permis.
    view_op(&mut harness, TaskViewOp::ResetSort);
    dispatch(&mut harness, Action::Structure(StructureOp::InsertMilestone));
    assert_eq!(history_len(harness.state()), 1);
}

/// H-UI-79 (exemple du cahier) : sélection {3, 4}, Ajouter ⇒ chronologie
/// [3, 4], une annulation ; Ctrl+Z ⇒ frise vide.
#[test]
fn h_ui_79_add_selection_one_undo() {
    let mut harness = harness_p();
    select_range(&mut harness, 3, 4);
    dispatch(&mut harness, Action::Timeline(TimelineOp::ToggleSelection));
    assert_eq!(timeline(harness.state()), vec![3, 4]);
    assert_eq!(history_len(harness.state()), 1);
    dispatch(&mut harness, Action::Undo);
    assert_eq!(timeline(harness.state()), Vec::<u32>::new());
}

/// H-UI-79 : bascule ⇒ une tâche absente fait tout ajouter ; toutes
/// présentes ⇒ toutes retirées ; Retirer (menu de la frise) une seule.
#[test]
fn h_ui_79_toggle_removes_when_all_present() {
    let mut harness = harness_p();
    select_range(&mut harness, 3, 3);
    dispatch(&mut harness, Action::Timeline(TimelineOp::ToggleSelection));
    assert_eq!(timeline(harness.state()), vec![3]);
    select_range(&mut harness, 3, 4);
    dispatch(&mut harness, Action::Timeline(TimelineOp::ToggleSelection));
    assert_eq!(timeline(harness.state()), vec![3, 4], "4 absente : toutes y vont");
    dispatch(&mut harness, Action::Timeline(TimelineOp::ToggleSelection));
    assert_eq!(
        timeline(harness.state()),
        Vec::<u32>::new(),
        "toutes présentes : toutes sortent"
    );
    select_range(&mut harness, 2, 4);
    dispatch(&mut harness, Action::Timeline(TimelineOp::ToggleSelection));
    dispatch(&mut harness, Action::Timeline(TimelineOp::Remove(uid(3))));
    assert_eq!(timeline(harness.state()), vec![2, 4]);
    // Retirer une tâche absente : rien d'écrit.
    let entries = history_len(harness.state());
    dispatch(&mut harness, Action::Timeline(TimelineOp::Remove(uid(3))));
    assert_eq!(history_len(harness.state()), entries);
    assert_eq!(entries, 5, "une entrée par geste");
}

/// Projet de `n` tâches d'un jour, sans style.
fn project_n(n: u32) -> Project {
    let start = "2026-10-19T08:00".parse().unwrap();
    let mut p = Project::new(ProjectInit {
        name: "N".to_owned(),
        anchor: ScheduleAnchor::FromStart(start),
        current_date: start,
        calendar_names: BuiltinCalendarNames::default(),
    });
    for i in 1..=n {
        let options = p.options;
        let mut t = Task::new(uid(i), format!("T{i}"), &options);
        t.outline_level = OutlineLevel::new(1).unwrap();
        t.duration = Duration::new(Minutes::new(480), DurationUnit::Day, TimeBasis::Working, false).unwrap();
        p.tasks.push(t).unwrap();
    }
    p
}

/// H-UI-79 : 201 tâches sélectionnées ⇒ message de limite, rien d'écrit ;
/// 200 passent.
#[test]
fn h_ui_79_limit_200() {
    let mut app = new_app();
    app.open_project(
        project_n(201),
        DocOrigin {
            name: "n.xml".to_owned(),
            path: None,
            demo: false,
        },
    );
    let mut harness = harness_for(app);
    harness.run();
    select_range(&mut harness, 1, 201);
    dispatch(&mut harness, Action::Timeline(TimelineOp::ToggleSelection));
    let max = Texts::new(Language::Fr).get_args(TextId::TimelineTooMany, &[("max", Arg::Count(200))]);
    assert_eq!(last_message(harness.state()), Some(max));
    assert_eq!(history_len(harness.state()), 0);
    assert!(doc_style(project(harness.state())).is_none(), "rien d'écrit");
    select_range(&mut harness, 1, 200);
    dispatch(&mut harness, Action::Timeline(TimelineOp::ToggleSelection));
    assert_eq!(timeline(harness.state()).len(), 200);
}

/// H-UI-79 : un document sans style reçoit le modèle par défaut des
/// préférences avec sa chronologie (comme la couleur de phase).
#[test]
fn h_ui_79_unstyled_doc_gets_default_template() {
    let mut harness = harness_p();
    assert!(doc_style(project(harness.state())).is_none());
    let default = harness.state().state.default_template;
    assert_eq!(default, TemplateId::Classique);
    select_range(&mut harness, 6, 6);
    dispatch(&mut harness, Action::Timeline(TimelineOp::ToggleSelection));
    let style = doc_style(project(harness.state())).unwrap();
    assert_eq!(style.base, default);
    assert_eq!(style.resolve().timeline.tasks, vec![6]);
}

/// H-UI-84 (exemple du cahier) : 7 active, zoom Jour, Gantt au début du
/// projet ⇒ début de la barre de 7 au huitième de la largeur visible.
#[test]
fn h_ui_84_scroll_to_task() {
    let mut harness = harness_p();
    dispatch(&mut harness, Action::SetZoom(TimeScaleZoom::Day));
    dispatch(&mut harness, Action::ScrollGanttX(-1.0e6));
    assert!(harness.state().view().gantt_scroll_x.abs() < f32::EPSILON);
    select_range(&mut harness, 7, 7);
    dispatch(&mut harness, Action::ScrollToTask);
    let app = harness.state();
    let start = match app.document.as_ref().unwrap().schedule().get(uid(7)) {
        Some(plannercraft_core::TaskOutcome::Scheduled(s)) => s.start,
        other => panic!("7 non planifiée : {other:?}"),
    };
    let view = app.view();
    let x = screen_axis(app).x(start) - view.last_gantt_left;
    let width = view.last_gantt_width;
    assert!(view.gantt_scroll_x > 0.0, "le Gantt a défilé");
    assert!(
        (x - width / 8.0).abs() <= 1.0,
        "barre de 7 à {x} px, attendu {} (largeur {width})",
        width / 8.0
    );
}

/// H-UI-84 : tâche masquée par la vue (mode plan niveau 1) atteinte par
/// la frise (H-UI-81 : `Timeline(Activate)` sélectionne puis défile) ⇒
/// message, rien ne défile. (Une sélection ordinaire d'une tâche masquée
/// est ramenée par la vue sur son ancêtre affiché.)
#[test]
fn h_ui_84_hidden_task_message() {
    let mut harness = harness_p();
    dispatch(&mut harness, Action::SetZoom(TimeScaleZoom::Day));
    view_op(&mut harness, TaskViewOp::SetOutlineLevel(Some(1)));
    assert_eq!(harness.state().visible_rows().unwrap().display_of(uid(7)), None);
    let scroll = harness.state().view().gantt_scroll_x;
    harness
        .state_mut()
        .dispatch(Action::Timeline(TimelineOp::Activate(uid(7))));
    harness.run();
    assert_eq!(last_message(harness.state()), Some(fr(TextId::ScrollTaskHidden)));
    assert!((harness.state().view().gantt_scroll_x - scroll).abs() < f32::EPSILON);
}

/// H-UI-84 : Notes ⇒ « Informations sur la tâche » de la tâche active,
/// ouverte à l'onglet Notes.
#[test]
fn h_ui_84_notes_tab() {
    let mut harness = harness_p();
    select_range(&mut harness, 3, 3);
    dispatch(&mut harness, Action::ShowTaskNotes);
    let dialog = harness.state().view().dialog.as_ref().unwrap();
    assert_eq!(dialog.kind, DialogKind::TaskInformation(uid(3)));
    let DialogBody::TaskInfo(state) = &dialog.body else {
        panic!("autre fenêtre : {:?}", dialog.kind);
    };
    assert_eq!(state.tab, TaskInfoTab::Notes);
    assert!(crate::shown_off_ribbon(&harness, &fr(TextId::TaskInfoTabNotes)));
}
