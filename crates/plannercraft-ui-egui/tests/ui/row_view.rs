//! Lignes affichées ≠ lignes du modèle (J8, lot L5, ADR-0016 D1) : vue
//! identité de J7, clic et glisser sous filtre et tri, liens renumérotés,
//! sélection masquée retirée, opérations de vue sans écriture (P50) et repli
//! idempotent dans l'application (P53).
//!
//! Filtre, tri et repli sont posés par les méthodes d'`App`
//! (`set_task_view`, `set_collapsed`) : leurs commandes d'interface
//! (menus, raccourcis, triangles) arrivent au lot L9.

#![allow(
    clippy::arithmetic_side_effects,
    reason = "tests : positions du pointeur et compteurs, un débordement doit paniquer"
)]

use std::collections::BTreeSet;

use egui::{Event, Key, Modifiers, PointerButton, Pos2, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_automation::filter::{FilterCtx, FilterPreset, TaskFilter, fold_text};
use plannercraft_core::{Constraint, Minutes, TaskUid};
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::row_view::{DisplayRow, ModelRow};
use plannercraft_ui_egui::rows::RowKind;
use plannercraft_ui_egui::state::{FilterChoice, SortKey, SortSpec, TaskViewState};
use plannercraft_ui_egui::{Action, App, TimeScaleZoom};
use proptest::prelude::*;

use crate::{fr, harness, new_app, open_demo, screen_axis};

/// Libellé d'accessibilité de la ligne affichée `d`.
fn row_label(app: &App, d: DisplayRow) -> String {
    let rows = app.visible_rows().unwrap();
    let row = rows.row(d).unwrap();
    Texts::new(Language::Fr).get_args(
        TextId::RowLabel,
        &[
            ("id", Arg::Count(i64::from(row.id))),
            ("name", Arg::Str(rows.text(d, Column::Name).to_owned().into())),
        ],
    )
}

/// Rectangle écran de la ligne affichée `d` de la table.
fn row_rect(harness: &Harness<'static, App>, d: DisplayRow) -> egui::Rect {
    let label = row_label(harness.state(), d);
    harness.get_by_label(&label).rect()
}

/// Clic simple au point `p`.
fn click_at(harness: &mut Harness<'static, App>, p: Pos2) {
    harness.event(Event::PointerMoved(p));
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run();
}

fn view_state(filter: FilterPreset, sort: Option<SortSpec>) -> TaskViewState {
    TaskViewState {
        filter: FilterChoice::Preset(filter),
        sort,
        ..TaskViewState::default()
    }
}

fn sort_by(key: SortKey, descending: bool, keep_outline: bool) -> SortSpec {
    SortSpec {
        key,
        descending,
        keep_outline,
    }
}

fn task(app: &App, uid: TaskUid) -> &plannercraft_core::Task {
    app.document.as_ref().unwrap().project().tasks.get(uid).unwrap()
}

/// UID des lignes affichées, dans l'ordre.
fn shown_uids(app: &App) -> Vec<TaskUid> {
    let rows = app.visible_rows().unwrap();
    (0..rows.len())
        .map(|d| rows.uid(DisplayRow::at(d)).unwrap())
        .collect()
}

/// Sans filtre, tri ni repli, la vue est l'identité : mêmes lignes, mêmes
/// liens (renumérotés à l'identique), même plage dessinée dans les deux
/// volets qu'en J7.
#[test]
fn row_view_identity_is_j7() {
    let mut harness = harness();
    open_demo(&mut harness);
    let app = harness.state();
    let view = app.row_view();
    let model = app.rows().unwrap();
    assert!(view.is_identity());
    assert_eq!(view.len(), model.len());
    let shown: Vec<(u32, u32)> = view.links().iter().map(|l| (l.from.0, l.to.0)).collect();
    let all: Vec<(u32, u32)> = model.links().iter().map(|l| (l.from, l.to)).collect();
    assert_eq!(shown, all);
    assert_eq!(shown.len(), 46);
    for d in 0..model.len() {
        assert_eq!(view.model_row(DisplayRow::at(d)), Some(ModelRow::at(d)));
    }
    assert_eq!(app.last_frame.table_rows, app.last_frame.gantt_rows);
    assert!(app.last_frame.links_painted > 0);
}

/// Filtre « Critiques », tri par Nom : le clic sur la 2e ligne affichée
/// sélectionne la tâche attendue (calculée à part : filtre partagé, puis
/// tri par nom replié), et la saisie qui suit modifie cette tâche-là.
#[test]
fn click_under_filter_and_sort_hits_right_task() {
    let mut harness = harness();
    open_demo(&mut harness);
    harness.state_mut().set_task_view(view_state(
        FilterPreset::Critical,
        Some(sort_by(SortKey::Name, false, false)),
    ));
    harness.run();
    // Ordre attendu, calculé sans `RowView`.
    let expected: Vec<TaskUid> = {
        let app = harness.state();
        let doc = app.document.as_ref().unwrap();
        let project = doc.project();
        let ctx = FilterCtx {
            project,
            schedule: doc.schedule(),
            status: project.effective_status_date(),
        };
        let hits = TaskFilter::from_preset(&FilterPreset::Critical).hits(&ctx);
        let mut kept: Vec<(String, usize, TaskUid)> = project
            .tasks
            .iter()
            .enumerate()
            .filter(|(i, _)| hits.shown[*i])
            .map(|(i, t)| (fold_text(&t.name), i, plannercraft_core::HasUid::uid(t)))
            .collect();
        kept.sort();
        kept.into_iter().map(|(_, _, uid)| uid).collect()
    };
    assert!(expected.len() >= 3 && expected.len() < 49, "{}", expected.len());
    assert_eq!(shown_uids(harness.state()), expected);
    let second = expected[1];
    // La 2e ligne affichée est bien sous la 1re (une ligne plus bas), et
    // ce n'est pas la ligne 2 du modèle.
    let first_rect = row_rect(&harness, DisplayRow(0));
    let second_rect = row_rect(&harness, DisplayRow(1));
    assert!((second_rect.top() - first_rect.top() - 22.0).abs() < 0.5);
    assert_ne!(harness.state().rows().unwrap().row(1).unwrap().uid, second);
    // Clic dans la colonne Nom de la 2e ligne affichée.
    let id_w = harness.state().state.visible_columns().next().unwrap().width;
    let p = pos2(
        second_rect.left() + f32::from(id_w) + 20.0,
        second_rect.center().y,
    );
    click_at(&mut harness, p);
    assert_eq!(harness.state().selection, Some(second));
    assert_eq!(harness.state().view().cursor.column, Column::Name);
    // Saisie du nom : la frappe ouvre la cellule de la tâche cliquée.
    harness.event(Event::Text("Zz renommée".to_owned()));
    harness.run();
    harness.key_press_modifiers(Modifiers::NONE, Key::Enter);
    harness.run();
    let app = harness.state();
    assert_eq!(task(app, second).name, "Zz renommée", "{:?}", app.messages());
    // Tri par nom réappliqué après l'édition : la tâche renommée finit la liste.
    assert_eq!(shown_uids(app).last(), Some(&second));
}

/// Un glisser de barre sous tri vise la tâche de la ligne affichée (sa
/// ligne du modèle), jamais la tâche du modèle au même rang.
#[test]
fn drag_under_sort_uses_model_row() {
    let mut harness = harness();
    open_demo(&mut harness);
    harness.state_mut().set_task_view(view_state(
        FilterPreset::All,
        Some(sort_by(SortKey::Name, true, false)),
    ));
    harness.run();
    // Feuille non commencée d'au moins 3 jours, déplacée par le tri.
    let (d, uid) = {
        let app = harness.state();
        let rows = app.visible_rows().unwrap();
        (0..rows.len())
            .find_map(|d| {
                let d = DisplayRow::at(d);
                let row = rows.row(d)?;
                let (start, finish) = row.bar?;
                let long = start.minutes_until(finish) >= Minutes::new(3 * 1_440);
                let t = task(app, row.uid);
                let fresh = t.progress.percent_complete.get() == 0 && t.progress.actual_start.is_none();
                let moved = rows.model_row(d) != Some(ModelRow(d.0));
                (row.kind == RowKind::Leaf && long && fresh && moved).then_some((d, row.uid))
            })
            .unwrap()
    };
    let other = harness.state().rows().unwrap().row(d.index()).unwrap().uid;
    assert_ne!(other, uid);
    let other_constraint = task(harness.state(), other).constraint;
    crate::ribbon_zoom(&mut harness, TimeScaleZoom::Day);
    harness.state_mut().dispatch(Action::ScrollToRow(d.index()));
    harness.run();
    let (start, finish) = harness
        .state()
        .visible_rows()
        .unwrap()
        .row(d)
        .unwrap()
        .bar
        .unwrap();
    let dx = screen_axis(harness.state()).x(start) - harness.state().view().last_gantt_left - 150.0;
    harness.state_mut().dispatch(Action::ScrollGanttX(dx));
    harness.run();
    let y = row_rect(&harness, d).center().y;
    let axis = screen_axis(harness.state());
    let from = pos2(f32::midpoint(axis.x(start), axis.x(finish)), y);
    let day = harness.state().state.zoom.px_per_day();
    let to = from + egui::vec2(2.0 * day, 0.0);
    harness.hover_at(from);
    harness.step();
    harness.drag_at(from);
    harness.step();
    for k in 1..=4_u8 {
        harness.hover_at(from + (to - from) * (f32::from(k) / 4.0));
        harness.step();
    }
    harness.drop_at(to);
    harness.run();
    let app = harness.state();
    let expected = start.checked_add(Minutes::new(2 * 1_440)).unwrap();
    assert_eq!(
        task(app, uid).constraint,
        Constraint::StartNoEarlierThan { date: expected },
        "{:?}",
        app.messages()
    );
    assert_eq!(task(app, other).constraint, other_constraint);
}

/// Repli de « Études et permis » : liens renumérotés sur les lignes
/// affichées, aucun lien vers une ligne masquée, tri par plus petite ligne
/// affichée ; les liens dessinés sont pris parmi ceux de la fenêtre.
#[test]
fn links_renumbered_culling() {
    let mut harness = harness();
    open_demo(&mut harness);
    let summary = harness.state().rows().unwrap().row(0).unwrap().uid;
    harness.state_mut().set_collapsed(summary, true);
    harness.run();
    let app = harness.state();
    let model = app.rows().unwrap();
    let view = app.row_view();
    assert_eq!(view.len(), model.len() - 6);
    let hidden: BTreeSet<u32> = (1..7).collect();
    let expected: Vec<u32> = model
        .links()
        .iter()
        .enumerate()
        .filter(|(_, l)| !hidden.contains(&l.from) && !hidden.contains(&l.to))
        .map(|(k, _)| u32::try_from(k).unwrap())
        .collect();
    let mut got: Vec<u32> = view.links().iter().map(|l| l.model_link).collect();
    got.sort_unstable();
    assert_eq!(got, expected);
    assert!(got.len() < 46);
    for l in view.links() {
        let link = &model.links()[usize::try_from(l.model_link).unwrap()];
        assert_eq!(view.model_row(l.from), Some(ModelRow(link.from)));
        assert_eq!(view.model_row(l.to), Some(ModelRow(link.to)));
    }
    assert!(
        view.links()
            .windows(2)
            .all(|w| w[0].from.min(w[0].to) <= w[1].from.min(w[1].to))
    );
    let touching = view.links_touching(app.last_frame.gantt_rows.clone()).count();
    assert!(app.last_frame.links_painted <= touching);
    assert!(app.last_frame.links_painted > 0);
    assert_eq!(app.last_frame.table_rows, app.last_frame.gantt_rows);
    // Déplier rend la vue identité.
    harness.state_mut().set_collapsed(summary, false);
    harness.run();
    assert!(harness.state().row_view().is_identity());
}

/// Une sélection qui pointe une ligne masquée est retirée : la tâche active
/// devient la prochaine tâche affichée (ordre des ID) ; une tâche masquée
/// sélectionnée en plus de la tâche active est retirée, l'active reste.
#[test]
fn selection_dropped_when_hidden() {
    let mut harness = harness();
    open_demo(&mut harness);
    let uid_at = |app: &App, i: usize| app.rows().unwrap().row(i).unwrap().uid;
    let (summary, child, next_phase, later) = {
        let app = harness.state();
        (uid_at(app, 0), uid_at(app, 2), uid_at(app, 7), uid_at(app, 9))
    };
    // Tâche active masquée.
    harness.state_mut().dispatch(Action::Select(Some(child)));
    harness.run();
    harness.state_mut().set_collapsed(summary, true);
    harness.run();
    let app = harness.state();
    assert_eq!(app.selection, Some(next_phase));
    assert_eq!(app.selected().click_order(), [next_phase]);
    // Tâche active visible, autre tâche masquée.
    harness.state_mut().set_collapsed(summary, false);
    harness.run();
    harness.state_mut().dispatch(Action::Select(Some(child)));
    harness.run();
    harness.state_mut().dispatch(Action::SelectWith {
        uid: later,
        mode: plannercraft_ui_egui::selection::SelectMode::Toggle,
    });
    harness.run();
    assert_eq!(harness.state().selected().click_order(), [child, later]);
    harness.state_mut().set_collapsed(summary, true);
    harness.run();
    let app = harness.state();
    assert_eq!(app.selection, Some(later));
    assert_eq!(app.selected().click_order(), [later]);
    // Filtre qui ne montre rien : aucune sélection, la ligne vide reste.
    harness.state_mut().set_task_view(TaskViewState {
        filter: FilterChoice::Custom(TaskFilter {
            name_contains: Some("aucune tâche ne s'appelle ainsi".to_owned()),
            ..TaskFilter::default()
        }),
        ..TaskViewState::default()
    });
    harness.run();
    let app = harness.state();
    assert!(app.row_view().is_empty());
    assert_eq!(app.selection, None);
    assert!(harness.query_by_label(&fr(TextId::BlankRowLabel)).is_some());
}

/// Navigation au clavier sous repli : ↓ passe à la ligne affichée
/// suivante, par-dessus les lignes masquées.
#[test]
fn nav_down_skips_hidden_rows() {
    let mut harness = harness();
    open_demo(&mut harness);
    let (summary, next_phase) = {
        let rows = harness.state().rows().unwrap();
        (rows.row(0).unwrap().uid, rows.row(7).unwrap().uid)
    };
    harness.state_mut().set_collapsed(summary, true);
    harness.state_mut().dispatch(Action::Select(Some(summary)));
    harness.run();
    harness.state_mut().dispatch(Action::Nav {
        mv: plannercraft_ui_egui::nav::NavMove::Down,
        extend: false,
    });
    harness.run();
    assert_eq!(harness.state().selection, Some(next_phase));
}

/// Opération de vue générée.
#[derive(Clone, Debug)]
enum ViewOp {
    Filter(u8),
    Sort(Option<(u8, bool, bool)>),
    Level(Option<u8>),
    Fold(u8),
    Select(u8),
}

fn arb_op() -> impl Strategy<Value = ViewOp> {
    prop_oneof![
        (0u8..7).prop_map(ViewOp::Filter),
        proptest::option::of((0u8..7, any::<bool>(), any::<bool>())).prop_map(ViewOp::Sort),
        proptest::option::of(1u8..4).prop_map(ViewOp::Level),
        (0u8..49).prop_map(ViewOp::Fold),
        (0u8..49).prop_map(ViewOp::Select),
    ]
}

fn preset(k: u8) -> FilterPreset {
    match k {
        1 => FilterPreset::Critical,
        2 => FilterPreset::Late,
        3 => FilterPreset::Incomplete,
        4 => FilterPreset::Milestones,
        5 => FilterPreset::Summaries,
        6 => FilterPreset::UsingResource("Maçon".to_owned()),
        _ => FilterPreset::All,
    }
}

const KEYS: [SortKey; 7] = [
    SortKey::Id,
    SortKey::Name,
    SortKey::Start,
    SortKey::Finish,
    SortKey::Duration,
    SortKey::PercentComplete,
    SortKey::TotalSlack,
];

fn apply_op(app: &mut App, op: &ViewOp) {
    let mut state = app.task_view().clone();
    match op {
        ViewOp::Filter(k) => state.filter = FilterChoice::Preset(preset(*k)),
        ViewOp::Sort(s) => {
            state.sort = s.map(|(k, descending, keep_outline)| SortSpec {
                key: KEYS[usize::from(k % 7)],
                descending,
                keep_outline,
            });
        }
        ViewOp::Level(l) => state.outline_level = *l,
        ViewOp::Fold(i) => {
            if let Some(uid) = app.rows().unwrap().row(usize::from(*i)).map(|r| r.uid) {
                app.toggle_collapsed(uid);
            }
            return;
        }
        // Clic sur une ligne affichée (une ligne masquée ne se clique pas).
        ViewOp::Select(i) => {
            let rows = app.visible_rows().unwrap();
            let d = usize::from(*i).checked_rem(rows.len()).map(DisplayRow::at);
            app.selection = d.and_then(|d| rows.uid(d));
            return;
        }
    }
    app.set_task_view(state);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// P50 : filtre, tri, repli : génération de la session et octets
    /// enregistrés inchangés pour toute suite d'opérations de vue ; la vue
    /// reste une permutation partielle et la sélection ne pointe jamais une
    /// ligne masquée.
    #[test]
    fn p50_view_operations_write_nothing(ops in proptest::collection::vec(arb_op(), 1..24)) {
        let mut app = new_app();
        app.open_demo();
        let doc = app.document.as_ref().unwrap();
        let generation = doc.generation();
        let bytes = doc.session.save_bytes(&plannercraft_mspdi::write::WriteOptions::default()).xml;
        for op in &ops {
            apply_op(&mut app, op);
            let rows = app.visible_rows().unwrap();
            if let Some(uid) = app.selection {
                prop_assert!(rows.display_of(uid).is_some(), "sélection masquée après {:?}", op);
            }
            for &uid in app.selected().uids() {
                prop_assert!(rows.display_of(uid).is_some());
            }
        }
        let doc = app.document.as_ref().unwrap();
        prop_assert_eq!(doc.generation(), generation);
        prop_assert!(!doc.is_dirty());
        let after = doc.session.save_bytes(&plannercraft_mspdi::write::WriteOptions::default()).xml;
        prop_assert_eq!(after, bytes);
    }

    /// P53 dans l'application : replier puis déplier une récapitulative
    /// rend la vue d'avant ; niveau n puis Tous ⇒ vue initiale.
    #[test]
    fn p53_app_fold_unfold_restores_view(ops in proptest::collection::vec(arb_op(), 0..8), pick in 0usize..49, level in 1u8..4) {
        let mut app = new_app();
        app.open_demo();
        for op in &ops {
            apply_op(&mut app, op);
        }
        let initial = app.row_view().clone();
        let uid = app.rows().unwrap().row(pick).unwrap().uid;
        app.toggle_collapsed(uid);
        app.toggle_collapsed(uid);
        prop_assert_eq!(app.row_view(), &initial);
        let mut state = app.task_view().clone();
        let before = state.outline_level;
        state.outline_level = Some(level);
        app.set_task_view(state.clone());
        state.outline_level = before;
        app.set_task_view(state);
        prop_assert_eq!(app.row_view(), &initial);
    }
}
