//! Finitions avant v1.0.0 (ADR-0022, lot P1, cahier `docs/plans/polish.md`) :
//! Décaler ▾ (H-UI-121, R-267), colonne Indicateurs (H-UI-122), Inspecter
//! (H-UI-123), Zoom → Tâches sélectionnées (H-UI-124), libellés (H-UI-125).
//!
//! Projet « P » (`filters::project_p`) : 1 « Gros œuvre » ⊃ 2 « Fondations »
//! lun 19/10 – ven 23/10, 3 « Murs », 4 jalon ; 5 « Second œuvre » ⊃ 6, 7.

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use plannercraft_automation::Batch;
use plannercraft_automation::helpers::PercentStep;
use plannercraft_core::{Constraint, ConstraintKind, Moment, Project, TaskUid};
use plannercraft_ui_egui::action::{DialogKind, ShiftStep, TaskCmd};
use plannercraft_ui_egui::column::{Column, ColumnState};
use plannercraft_ui_egui::dialog::DialogBody;
use plannercraft_ui_egui::dialog::task_info::TaskInfoTab;
use plannercraft_ui_egui::i18n::{Language, TextId, Texts};
use plannercraft_ui_egui::indicators::{self, Indicator};
use plannercraft_ui_egui::ribbon::RibbonTab;
use plannercraft_ui_egui::row_view::DisplayRow;
use plannercraft_ui_egui::view::side_panel::SideTab;
use plannercraft_ui_egui::{Action, App, TimeScaleZoom, UiState};
use serde_json::json;

use crate::filters::harness_p;
use crate::{click, fr, ribbon_click, ribbon_menu};

fn uid(n: u32) -> TaskUid {
    TaskUid::new(n)
}

fn moment(s: &str) -> Moment {
    s.parse().unwrap()
}

fn project(app: &App) -> &Project {
    app.document.as_ref().unwrap().project()
}

fn history_len(app: &App) -> usize {
    app.document.as_ref().unwrap().session.history().entries.len()
}

fn last_message(app: &App) -> Option<String> {
    app.messages().last().map(|m| m.text.clone())
}

fn dispatch(harness: &mut Harness<'static, App>, action: Action) {
    harness.state_mut().dispatch(action);
    harness.run();
}

fn select(harness: &mut Harness<'static, App>, n: u32) {
    dispatch(harness, Action::Select(Some(uid(n))));
}

/// Applique `ops` comme un lot de l'interface.
fn exec(harness: &mut Harness<'static, App>, ops: &serde_json::Value) {
    let batch = Batch::from_json(json!({ "format_version": 1, "ops": ops.clone() })).unwrap();
    dispatch(harness, Action::Exec(batch));
}

/// H-UI-121 : Tâche → Planification → Décaler ▾ → +1 jour sur 2 ⇒
/// contrainte « Début au plus tôt le » mar 20/10 08:00, une entrée
/// d'annulation, compte rendu « 1 tâche décalée de +1 jour. » ; Ctrl+Z
/// rend P.
#[test]
fn h_ui_121_shift_ribbon_one_undo_and_report() {
    let mut harness = harness_p();
    let before = project(harness.state()).clone();
    select(&mut harness, 2);
    ribbon_menu(
        &mut harness,
        RibbonTab::Task,
        &fr(TextId::RibbonShift),
        &fr(TextId::ShiftPlusDay),
    );
    let app = harness.state();
    assert_eq!(history_len(app), 1);
    assert_eq!(
        project(app).tasks.get(uid(2)).unwrap().constraint,
        Constraint::StartNoEarlierThan {
            date: moment("2026-10-20T08:00")
        }
    );
    let text = last_message(app).unwrap();
    assert!(
        text.starts_with('1') && text.ends_with("tâche décalée de +1 jour."),
        "{text:?}"
    );
    dispatch(&mut harness, Action::Undo);
    assert_eq!(project(harness.state()), &before);
}

/// H-UI-121 : le jumeau du menu Tâche → Décaler → +1 semaine écrit le même
/// lot que le ruban.
#[test]
fn h_ui_121_shift_menu_twin_same_batch() {
    let mut harness = harness_p();
    select(&mut harness, 3);
    ribbon_menu(
        &mut harness,
        RibbonTab::Task,
        &fr(TextId::RibbonShift),
        &fr(TextId::ShiftPlusWeek),
    );
    let by_ribbon = project(harness.state()).clone();
    dispatch(&mut harness, Action::Undo);
    click(&mut harness, &fr(TextId::MenuTask));
    {
        let label = fr(TextId::RibbonShift);
        let node = harness
            .query_all_by_label(&label)
            .find(|n| !crate::in_ribbon(&harness, n))
            .expect("sous-menu Décaler absent du menu Tâche");
        node.hover();
        node.click();
    }
    harness.run();
    click(&mut harness, &fr(TextId::ShiftPlusWeek));
    assert_eq!(project(harness.state()), &by_ribbon);
    assert_eq!(history_len(harness.state()), 1);
}

/// H-UI-121 : un geste sans effet (tâche achevée) n'ajoute aucune entrée ;
/// une information dit pourquoi.
#[test]
fn h_ui_121_shift_complete_task_no_entry() {
    let mut harness = harness_p();
    select(&mut harness, 2);
    dispatch(
        &mut harness,
        Action::TaskCmd(TaskCmd::MarkPercent(PercentStep::P100)),
    );
    assert_eq!(history_len(harness.state()), 1);
    let before = project(harness.state()).clone();
    dispatch(&mut harness, Action::TaskCmd(TaskCmd::Shift(ShiftStep::PlusDay)));
    assert_eq!(history_len(harness.state()), 1);
    assert_eq!(project(harness.state()), &before);
    assert_eq!(
        last_message(harness.state()),
        Some(fr(TextId::ShiftReportNothing))
    );
}

/// H-UI-121 : −1 jour sur 3 (retenue par 2) ⇒ décalage fait, compte rendu
/// « limité par les prédécesseurs : tâches 3 » en avertissement.
#[test]
fn h_ui_121_shift_limited_reported() {
    let mut harness = harness_p();
    select(&mut harness, 3);
    dispatch(&mut harness, Action::TaskCmd(TaskCmd::Shift(ShiftStep::MinusDay)));
    let app = harness.state();
    assert_eq!(history_len(app), 1);
    let message = app.messages().last().unwrap();
    assert_eq!(message.severity, plannercraft_core::Severity::Warning);
    assert!(message.text.contains("prédécesseurs"), "{}", message.text);
    assert!(message.text.ends_with("tâches 3."), "{}", message.text);
}

/// H-UI-121 : Décaler ▾ → Personnalisé… ; « 50% » refusé dans la fenêtre
/// (rien d'écrit) ; « -1j » sur 6 ⇒ contrainte au ven 06/11, fenêtre
/// fermée, une entrée.
#[test]
fn h_ui_121_shift_custom_dialog() {
    let mut harness = harness_p();
    select(&mut harness, 6);
    dispatch(&mut harness, Action::ShowDialog(DialogKind::Shift));
    let set_text = |harness: &mut Harness<'static, App>, text: &str| {
        let dialog = harness.state_mut().view_mut().dialog.as_mut().unwrap();
        let DialogBody::Shift(state) = &mut dialog.body else {
            panic!("autre fenêtre");
        };
        state.text = text.to_owned();
    };
    set_text(&mut harness, "50%");
    click(&mut harness, &fr(TextId::TaskInfoOk));
    let dialog = harness.state().view().dialog.as_ref().unwrap();
    let DialogBody::Shift(state) = &dialog.body else {
        panic!("autre fenêtre");
    };
    assert_eq!(state.error, Some(fr(TextId::ShiftNoPercent)));
    assert_eq!(history_len(harness.state()), 0);
    set_text(&mut harness, "-1j");
    click(&mut harness, &fr(TextId::TaskInfoOk));
    assert!(harness.state().view().dialog.is_none());
    assert_eq!(history_len(harness.state()), 1);
    assert_eq!(
        project(harness.state())
            .tasks
            .get(uid(6))
            .unwrap()
            .constraint
            .kind(),
        ConstraintKind::StartNoEarlierThan
    );
}

/// H-UI-122 : la colonne Indicateurs est affichée par défaut après N° ;
/// un état mémorisé sans elle la reçoit une fois (après l'ID) ; masquée
/// ensuite, elle ne revient pas.
#[test]
fn h_ui_122_indicators_default_and_migrated_once() {
    let default = UiState::default();
    let order: Vec<Column> = default.columns.iter().map(|c| c.column).collect();
    assert_eq!(&order[..3], &[Column::Id, Column::Indicators, Column::Name]);
    assert!(default.indicators_added);
    // État mémorisé d'avant le lot (version 2, sans le drapeau).
    let mut old = UiState::default();
    old.columns.retain(|c| c.column != Column::Indicators);
    let mut json: serde_json::Value = serde_json::from_str(&old.to_json().unwrap()).unwrap();
    json.as_object_mut().unwrap().remove("indicators_added");
    let (read, warning) = UiState::from_json_lenient(&json.to_string());
    assert!(warning.is_none());
    let order: Vec<Column> = read.columns.iter().map(|c| c.column).collect();
    assert_eq!(&order[..3], &[Column::Id, Column::Indicators, Column::Name]);
    assert!(read.indicators_added);
    // Masquée puis relue : elle reste masquée.
    let mut hidden = read;
    hidden.columns.retain(|c| c.column != Column::Indicators);
    let (again, _) = UiState::from_json_lenient(&hidden.to_json().unwrap());
    assert!(!again.columns.iter().any(|c| c.column == Column::Indicators));
    assert_eq!(
        ColumnState::default_for(Column::Indicators).width,
        28,
        "28 px par défaut"
    );
}

/// H-UI-122 : indicateurs dans l'ordre (note, contrainte, échéance
/// dépassée, manuelle, inactive, achevée) et infobulle d'une ligne par
/// indicateur, avec les dates.
#[test]
fn h_ui_122_indicators_order_and_tooltip() {
    let mut harness = harness_p();
    exec(
        &mut harness,
        &json!([
            { "op": "task.update", "uid": 3, "notes": "Voir l'architecte" },
            { "op": "task.set_constraint", "uid": 3, "kind": "snet", "date": "2026-10-28T08:00" },
            { "op": "task.set_deadline", "uid": 3, "deadline": "2026-10-30T17:00" },
            { "op": "task.update", "uid": 3, "mode": "manual" },
            { "op": "task.update", "uid": 7, "active": false },
            { "op": "progress.update", "task": 2, "percent_complete": { "value": 10000 } }
        ]),
    );
    let app = harness.state();
    let doc = app.document.as_ref().unwrap();
    let p = doc.project();
    let missed = indicators::missed_deadlines(doc.session.diagnostics());
    let rows = app.visible_rows().unwrap();
    let row = |n: u32| {
        (0..rows.len())
            .filter_map(|i| rows.row(DisplayRow::at(i)))
            .find(|r| r.uid == uid(n))
            .unwrap()
    };
    let three = indicators::of(p.tasks.get(uid(3)).unwrap(), row(3), p.anchor, &missed);
    assert_eq!(
        three,
        vec![
            Indicator::Note,
            Indicator::Constraint {
                kind: ConstraintKind::StartNoEarlierThan,
                date: Some(moment("2026-10-28T08:00"))
            },
            Indicator::DeadlineMissed {
                deadline: moment("2026-10-30T17:00")
            },
            Indicator::Manual,
        ]
    );
    let texts = Texts::new(Language::Fr);
    let fields = plannercraft_ui_egui::field::field_labels(&texts);
    let tip = indicators::tooltip(&three, &texts, &fields, texts.format_labels());
    let lines: Vec<&str> = tip.lines().collect();
    assert_eq!(lines.len(), 4, "{tip}");
    assert_eq!(lines[0], fr(TextId::IndicatorNote));
    assert!(
        lines[1].contains("Début au plus tôt le") && lines[1].contains("28/10"),
        "{tip}"
    );
    assert!(lines[2].contains("30/10"), "{tip}");
    let seven = indicators::of(p.tasks.get(uid(7)).unwrap(), row(7), p.anchor, &missed);
    assert_eq!(seven, vec![Indicator::Inactive]);
    let two = indicators::of(p.tasks.get(uid(2)).unwrap(), row(2), p.anchor, &missed);
    assert_eq!(two, vec![Indicator::Complete]);
    let six = indicators::of(p.tasks.get(uid(6)).unwrap(), row(6), p.anchor, &missed);
    assert_eq!(six, Vec::new());
}

/// H-UI-122 : clic sur l'icône de note ⇒ tâche sélectionnée et
/// « Informations sur la tâche » ouverte à l'onglet Notes.
#[test]
fn h_ui_122_note_click_opens_notes() {
    let mut harness = harness_p();
    // Les tests ouvrent sans la colonne (`without_indicators`) : insérée
    // ici après N°, comme dans un état neuf.
    dispatch(
        &mut harness,
        Action::Column(plannercraft_ui_egui::action::ColumnOp::Insert {
            at: 1,
            column: Column::Indicators,
        }),
    );
    exec(
        &mut harness,
        &json!([{ "op": "task.update", "uid": 6, "notes": "Placo hydrofuge" }]),
    );
    select(&mut harness, 2);
    harness.get_by_label(&fr(TextId::IndicatorNote)).click();
    harness.run();
    let app = harness.state();
    assert_eq!(app.selection, Some(uid(6)));
    let dialog = app.view().dialog.as_ref().unwrap();
    assert_eq!(dialog.kind, DialogKind::TaskInformation(uid(6)));
    let DialogBody::TaskInfo(state) = &dialog.body else {
        panic!("autre fenêtre");
    };
    assert_eq!(state.tab, TaskInfoTab::Notes);
}

/// H-UI-123 : Tâche → Tâches → Inspecter ⇒ volet latéral ouvert sur
/// « Pourquoi cette date ? », qui suit la tâche active.
#[test]
fn h_ui_123_inspect_opens_why_this_date() {
    let mut harness = harness_p();
    select(&mut harness, 3);
    assert!(!harness.state().side_panel().open);
    ribbon_click(&mut harness, RibbonTab::Task, &fr(TextId::RibbonInspect));
    let side = harness.state().side_panel();
    assert!(side.open);
    assert_eq!(side.tab, SideTab::WhyThisDate);
    // Sans sélection : grisé (ruban complet : Inspecter est dans le menu
    // « … » du ruban compact, lot U2).
    dispatch(&mut harness, Action::Select(None));
    dispatch(
        &mut harness,
        Action::Ribbon(plannercraft_ui_egui::ribbon::RibbonOp::ToggleFull),
    );
    let panel = plannercraft_ui_egui::view::ribbon::drawn(&harness.ctx).panel;
    let label = fr(TextId::RibbonInspect);
    let node = harness
        .query_all_by_label(&label)
        .find(|n| panel.contains(n.rect().center()))
        .unwrap();
    assert!(node.accesskit_node().is_disabled());
}

/// H-UI-124 : Affichage → Zoom → Tâches sélectionnées sur 2 (5 jours) ⇒
/// échelle Jour, Gantt défilé jusqu'à la barre de 2 ; sur toute la plage
/// de 1 (≈ 3 semaines) dans une fenêtre étroite ⇒ échelle plus grossière.
#[test]
fn h_ui_124_zoom_to_selection() {
    let mut harness = harness_p();
    dispatch(&mut harness, Action::SetZoom(TimeScaleZoom::Quarter));
    select(&mut harness, 2);
    ribbon_click(&mut harness, RibbonTab::View, &fr(TextId::RibbonZoomSelected));
    let app = harness.state();
    assert_eq!(app.state.zoom, TimeScaleZoom::Day);
    let axis = crate::screen_axis(app);
    let x = axis.x(moment("2026-10-19T08:00"));
    // Abscisse à l'écran du début de 2 : dans le premier quart du Gantt.
    let (left, width) = (app.view().last_gantt_left, app.view().last_gantt_width);
    assert!(
        x >= left && x < left + width / 4.0,
        "barre visible : x {x}, Gantt {left} + {width}"
    );
    // Sans barre (aucune sélection) : rien ne change.
    dispatch(&mut harness, Action::Select(None));
    dispatch(&mut harness, Action::ZoomToSelection);
    assert_eq!(harness.state().state.zoom, TimeScaleZoom::Day);
}

/// H-UI-125 : libellés français « Atteindre la tâche » et « Définir le
/// planning de référence » (ruban et jumeaux) ; l'anglais est inchangé.
#[test]
fn h_ui_125_french_labels() {
    assert_eq!(fr(TextId::RibbonScrollToTask), "Atteindre la tâche");
    assert_eq!(fr(TextId::RibbonSetBaseline), "Définir le planning de référence");
    let en = Texts::new(Language::En);
    assert_eq!(en.get(TextId::RibbonScrollToTask), "Scroll to Task");
    assert_eq!(en.get(TextId::RibbonSetBaseline), "Set Baseline");
}
