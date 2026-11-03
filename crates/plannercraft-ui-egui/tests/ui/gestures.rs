//! Gestes dans le Gantt : déplacer, redimensionner, avancement, lien (J7,
//! lot L8, H-UI-20, H-UI-21, R-240 à R-243). Le pointeur est conduit par
//! de vrais événements egui ; les abscisses viennent de `TimeAxis::x`
//! (sens date → pixel seulement) et les ordonnées de la ligne de la table.

#![allow(
    clippy::arithmetic_side_effects,
    reason = "tests : positions du pointeur et compteurs, un débordement doit paniquer"
)]

use egui::{Pos2, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_core::{Constraint, Minutes, Moment, TaskUid};
use plannercraft_ui_egui::action::DialogKind;
use plannercraft_ui_egui::dialog::DialogBody;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::rows::RowKind;
use plannercraft_ui_egui::{Action, App, TimeScaleZoom};

use crate::{click, fr, harness, open_demo, screen_axis};

/// Feuille non commencée d'au moins 3 jours de l'exemple : (indice de
/// ligne, UID).
fn unstarted_leaf(app: &App) -> (usize, TaskUid) {
    let rows = app.rows().unwrap();
    (0..rows.len())
        .find_map(|i| {
            let row = rows.row(i)?;
            let (start, finish) = row.bar?;
            let long = start.minutes_until(finish) >= Minutes::new(3 * 1_440);
            let task = app.document.as_ref()?.project().tasks.get(row.uid)?;
            let fresh = task.progress.percent_complete.get() == 0 && task.progress.actual_start.is_none();
            (row.kind == RowKind::Leaf && long && fresh).then_some((i, row.uid))
        })
        .expect("aucune feuille non commencée de 3 jours dans l'exemple")
}

/// Exemple ouvert au zoom Jour, la ligne `i` visible et le début de sa
/// barre à 150 points du bord gauche du Gantt.
fn show_row(harness: &mut Harness<'static, App>, i: usize) {
    crate::ribbon_zoom(harness, TimeScaleZoom::Day);
    harness.state_mut().dispatch(Action::ScrollToRow(i));
    harness.run();
    let app = harness.state();
    let (start, _) = app.rows().unwrap().row(i).unwrap().bar.unwrap();
    let left = app.view().last_gantt_left;
    let dx = screen_axis(app).x(start) - left - 150.0;
    harness.state_mut().dispatch(Action::ScrollGanttX(dx));
    harness.run();
}

/// Ordonnée écran du centre de la ligne `i` (ligne de la table).
fn row_y(harness: &Harness<'static, App>, i: usize) -> f32 {
    let app = harness.state();
    let row = app.rows().unwrap().row(i).unwrap();
    let name = app
        .document
        .as_ref()
        .unwrap()
        .project()
        .tasks
        .get(row.uid)
        .unwrap()
        .name
        .clone();
    let label = Texts::new(Language::Fr).get_args(
        TextId::RowLabel,
        &[
            ("id", Arg::Count(i64::from(row.id))),
            ("name", Arg::Str(name.into())),
        ],
    );
    harness.get_by_label(&label).rect().center().y
}

/// Abscisses écran du début et de la fin de la barre de la ligne `i`.
fn bar_x(harness: &Harness<'static, App>, i: usize) -> (f32, f32) {
    let app = harness.state();
    let (start, finish) = app.rows().unwrap().row(i).unwrap().bar.unwrap();
    let axis = screen_axis(app);
    (axis.x(start), axis.x(finish))
}

/// Points par jour du zoom courant.
fn px_per_day(harness: &Harness<'static, App>) -> f32 {
    harness.state().state.zoom.px_per_day()
}

/// Presse en `from`, glisse en quatre pas jusqu'à `to` sans lâcher.
fn press_and_move(harness: &mut Harness<'static, App>, from: Pos2, to: Pos2) {
    harness.hover_at(from);
    harness.step();
    harness.drag_at(from);
    harness.step();
    for k in 1..=4_u8 {
        let t = f32::from(k) / 4.0;
        harness.hover_at(from + (to - from) * t);
        harness.step();
    }
}

/// Glisse de `from` à `to` et lâche.
fn drag(harness: &mut Harness<'static, App>, from: Pos2, to: Pos2) {
    press_and_move(harness, from, to);
    harness.drop_at(to);
    harness.run();
}

fn generation(app: &App) -> u64 {
    app.document.as_ref().unwrap().generation()
}

fn history_entries(app: &App) -> usize {
    app.document.as_ref().unwrap().session.history().entries.len()
}

fn task(app: &App, uid: TaskUid) -> &plannercraft_core::Task {
    app.document.as_ref().unwrap().project().tasks.get(uid).unwrap()
}

fn scheduled_start(app: &App, uid: TaskUid) -> Moment {
    app.document
        .as_ref()
        .unwrap()
        .schedule()
        .scheduled(uid)
        .unwrap()
        .start
}

/// H-UI-20 : intérieur = déplacer (SNET au début + 2 jours, R-240), bord
/// droit = durée (R-241), bord gauche = avancement (R-242).
#[test]
fn h_ui_20_zones() {
    let mut harness = harness();
    open_demo(&mut harness);
    let (i, uid) = unstarted_leaf(harness.state());
    show_row(&mut harness, i);
    let day = px_per_day(&harness);
    let y = row_y(&harness, i);
    let (x0, x1) = bar_x(&harness, i);
    assert!(x1 - x0 >= 12.0, "barre trop étroite : {x0} – {x1}");
    let start = scheduled_start(harness.state(), uid);

    // Intérieur, deux jours vers la droite.
    let from = pos2(f32::midpoint(x0, x1), y);
    drag(&mut harness, from, from + egui::vec2(2.0 * day, 0.0));
    let expected = start.checked_add(Minutes::new(2 * 1_440)).unwrap();
    assert_eq!(
        task(harness.state(), uid).constraint,
        Constraint::StartNoEarlierThan { date: expected },
        "{:?}",
        harness.state().messages()
    );

    // Bord droit, trois jours de plus (une fin du vendredi + 1 jour
    // tomberait un samedi : durée inchangée, R-241) : la durée change.
    let before = task(harness.state(), uid).duration.minutes();
    let (x0, x1) = bar_x(&harness, i);
    let from = pos2(x1 - 1.0, y);
    drag(&mut harness, from, from + egui::vec2(3.0 * day, 0.0));
    let after = task(harness.state(), uid).duration.minutes();
    assert!(after > before, "durée {before:?} → {after:?}");

    // Bord gauche, un jour vers la droite : avancement posé.
    let from = pos2(x0 + 1.0, y);
    drag(&mut harness, from, from + egui::vec2(day, 0.0));
    let percent = task(harness.state(), uid).progress.percent_complete.get();
    assert!(percent > 0 && percent < 10_000, "{percent}");
    assert!(
        harness.state().messages().is_empty(),
        "{:?}",
        harness.state().messages()
    );
}

/// H-UI-20 : glisser dans le vide fait défiler, sans aucun lot.
#[test]
fn h_ui_20_drag_empty_scrolls() {
    let mut harness = harness();
    open_demo(&mut harness);
    let (i, _) = unstarted_leaf(harness.state());
    show_row(&mut harness, i);
    let y = row_y(&harness, i);
    let (x0, _) = bar_x(&harness, i);
    let generation_before = generation(harness.state());
    let scroll_before = harness.state().view().gantt_scroll_x;
    // 100 points avant le début de la barre : aucune barre ni lien.
    let from = pos2(x0 - 100.0, y);
    drag(&mut harness, from, from - egui::vec2(60.0, 0.0));
    let app = harness.state();
    assert_eq!(generation(app), generation_before);
    assert!(app.view().drag.is_none());
    assert!(
        app.view().gantt_scroll_x > scroll_before + 30.0,
        "{scroll_before} → {}",
        app.view().gantt_scroll_x
    );
}

/// Un geste appliqué = une seule entrée d'annulation ; annuler rend le
/// début d'origine.
#[test]
fn gesture_single_undo_entry() {
    let mut harness = harness();
    open_demo(&mut harness);
    let (i, uid) = unstarted_leaf(harness.state());
    show_row(&mut harness, i);
    let day = px_per_day(&harness);
    let y = row_y(&harness, i);
    let (x0, x1) = bar_x(&harness, i);
    let start = scheduled_start(harness.state(), uid);
    let entries = history_entries(harness.state());
    let from = pos2(f32::midpoint(x0, x1), y);
    drag(&mut harness, from, from + egui::vec2(3.0 * day, 0.0));
    assert_eq!(history_entries(harness.state()), entries + 1);
    assert_ne!(scheduled_start(harness.state(), uid), start);
    harness.state_mut().dispatch(Action::Undo);
    harness.run();
    assert_eq!(history_entries(harness.state()), entries);
    assert_eq!(scheduled_start(harness.state(), uid), start);
}

/// Pendant le glisser, l'aperçu change sans recalcul (génération
/// inchangée) ; le lâcher applique un lot.
#[test]
fn gesture_preview_no_recalc() {
    let mut harness = harness();
    open_demo(&mut harness);
    let (i, uid) = unstarted_leaf(harness.state());
    show_row(&mut harness, i);
    let day = px_per_day(&harness);
    let y = row_y(&harness, i);
    let (x0, x1) = bar_x(&harness, i);
    let start = scheduled_start(harness.state(), uid);
    let generation_before = generation(harness.state());
    let from = pos2(f32::midpoint(x0, x1), y);
    let to = from + egui::vec2(7.0 * day, 0.0);
    press_and_move(&mut harness, from, to);
    let app = harness.state();
    assert_eq!(generation(app), generation_before, "recalcul pendant le glisser");
    let d = app.view().drag.expect("glisser en cours");
    assert_eq!(d.uid, uid);
    assert_eq!(d.days, 7);
    let preview = d.preview.expect("aperçu");
    assert_eq!(preview.start, start.checked_add(Minutes::new(7 * 1_440)).unwrap());
    harness.drop_at(to);
    harness.run();
    assert_eq!(generation(harness.state()), generation_before + 1);
    assert!(harness.state().view().drag.is_none());
}

/// R-243 par le pointeur : de l'intérieur d'une barre vers la ligne
/// suivante ⇒ un lien FS ; Échap pendant un glisser l'annule.
#[test]
fn gesture_link_by_drag_and_escape() {
    let mut harness = harness();
    open_demo(&mut harness);
    let (i, uid) = unstarted_leaf(harness.state());
    show_row(&mut harness, i);
    let y = row_y(&harness, i);
    let (x0, x1) = bar_x(&harness, i);
    let from = pos2(f32::midpoint(x0, x1), y);
    // Échap : rien.
    let generation_before = generation(harness.state());
    press_and_move(&mut harness, from, from + egui::vec2(40.0, 0.0));
    harness.key_press(egui::Key::Escape);
    harness.step();
    assert!(harness.state().view().drag.is_none());
    harness.drop_at(from + egui::vec2(40.0, 0.0));
    harness.run();
    assert_eq!(generation(harness.state()), generation_before);
    // Vers une des lignes suivantes (feuille sans lien depuis celle-ci).
    let target = (i + 1..i + 6)
        .find(|&j| {
            let app = harness.state();
            app.rows().unwrap().row(j).is_some_and(|row| {
                row.kind == RowKind::Leaf
                    && !task(app, row.uid)
                        .predecessors
                        .iter()
                        .any(|l| l.predecessor == uid)
            })
        })
        .unwrap();
    let target_uid = harness.state().rows().unwrap().row(target).unwrap().uid;
    let ty = row_y(&harness, target);
    // Lâcher sur la barre de la cible (R-243).
    let (tx0, tx1) = bar_x(&harness, target);
    drag(&mut harness, from, pos2(f32::midpoint(tx0, tx1), ty));
    let links = &task(harness.state(), target_uid).predecessors;
    assert!(
        links
            .iter()
            .any(|l| l.predecessor == uid && l.kind == plannercraft_core::LinkType::FinishToStart),
        "{links:?} ; {:?}",
        harness.state().messages()
    );
}

/// Relecture finale J7, F7 (R-243) : un déplacement qui dérive sur une
/// autre ligne mais lâché hors de toute barre reste un déplacement (SNET),
/// sans lien.
#[test]
fn gesture_drop_off_bar_moves_not_links() {
    let mut harness = harness();
    open_demo(&mut harness);
    let (i, uid) = unstarted_leaf(harness.state());
    show_row(&mut harness, i);
    let day = px_per_day(&harness);
    let y = row_y(&harness, i);
    let (x0, x1) = bar_x(&harness, i);
    let start = scheduled_start(harness.state(), uid);
    let from = pos2(f32::midpoint(x0, x1), y);
    let x = from.x + 3.0 * day;
    // Une ligne voisine dont la barre ne passe pas sous le point de lâcher.
    let off = (i + 1..i + 8)
        .find(|&j| {
            let app = harness.state();
            app.rows().unwrap().row(j).is_some_and(|row| {
                row.bar.is_some() && {
                    let (tx0, tx1) = bar_x(&harness, j);
                    x < tx0 - 6.0 || x > tx1 + 6.0
                }
            })
        })
        .expect("ligne voisine sans barre sous le point");
    let target_uid = harness.state().rows().unwrap().row(off).unwrap().uid;
    let links_before = task(harness.state(), target_uid).predecessors.len();
    let to = pos2(x, row_y(&harness, off));
    drag(&mut harness, from, to);
    let expected = start.checked_add(Minutes::new(3 * 1_440)).unwrap();
    assert_eq!(
        task(harness.state(), uid).constraint,
        Constraint::StartNoEarlierThan { date: expected },
        "{:?}",
        harness.state().messages()
    );
    assert_eq!(
        task(harness.state(), target_uid).predecessors.len(),
        links_before,
        "lien créé"
    );
}

/// H-UI-19 (laissé par le lot L9) : double-clic sur une barre ⇒
/// « Informations sur la tâche ».
#[test]
fn h_ui_19_double_click_bar_opens_task_info() {
    // Pas d'image court : deux clics rapprochés font un double-clic.
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1280.0, 720.0))
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .with_os(egui::os::OperatingSystem::Nix)
        .with_step_dt(1.0 / 60.0)
        .with_max_steps(8)
        .build_ui_state(plannercraft_ui_egui::ui, crate::new_app());
    open_demo(&mut harness);
    let (i, uid) = unstarted_leaf(harness.state());
    show_row(&mut harness, i);
    let y = row_y(&harness, i);
    let (x0, x1) = bar_x(&harness, i);
    let p = pos2(f32::midpoint(x0, x1), y);
    harness.hover_at(p);
    for _ in 0..2 {
        for pressed in [true, false] {
            harness.event(egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
    }
    harness.run();
    let dialog = harness.state().view().dialog.as_ref().expect("fenêtre ouverte");
    assert_eq!(dialog.kind, DialogKind::TaskInformation(uid));
}

/// Remplace le décalage de la boîte « Dépendance » par `text`.
fn retype_lag(harness: &mut Harness<'static, App>, text: &str) {
    let label = fr(TextId::LinkDialogLag);
    let node = harness.get_by_role_and_label(egui::accesskit::Role::TextInput, &label);
    node.focus();
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.event(egui::Event::Text(text.to_owned()));
    harness.run();
}

/// H-UI-21 : boîte « Dépendance » ; le décalage saisi « 2j » donne un
/// `link.update { lag }` (une entrée d'annulation) ; « Supprimer » retire
/// le lien.
#[test]
fn h_ui_21_link_dialog_updates_lag() {
    let mut harness = harness();
    open_demo(&mut harness);
    let (pred, succ) = {
        let project = harness.state().document.as_ref().unwrap().project();
        project
            .tasks
            .iter()
            .find_map(|t| {
                t.predecessors
                    .first()
                    .map(|l| (l.predecessor, plannercraft_core::HasUid::uid(t)))
            })
            .unwrap()
    };
    harness.state_mut().dispatch(Action::ShowDialog(DialogKind::Link {
        predecessor: pred,
        successor: succ,
    }));
    harness.run();
    assert!(matches!(
        harness.state().view().dialog.as_ref().map(|d| &d.body),
        Some(DialogBody::Link(_))
    ));
    assert!(harness.query_by_label(&fr(TextId::LinkDialogTitle)).is_some());
    let entries = history_entries(harness.state());
    retype_lag(&mut harness, "2j");
    click(&mut harness, &fr(TextId::TaskInfoOk));
    let app = harness.state();
    assert!(app.view().dialog.is_none(), "{:?}", app.messages());
    assert_eq!(history_entries(app), entries + 1);
    let link = *task(app, succ)
        .predecessors
        .iter()
        .find(|l| l.predecessor == pred)
        .unwrap();
    assert!(matches!(
        link.lag.value,
        plannercraft_core::LagValue::Time { minutes, .. } if minutes == Minutes::new(960)
    ));
    // Décalage illisible : message dans la boîte, rien appliqué.
    harness.state_mut().dispatch(Action::ShowDialog(DialogKind::Link {
        predecessor: pred,
        successor: succ,
    }));
    harness.run();
    retype_lag(&mut harness, "xyz");
    click(&mut harness, &fr(TextId::TaskInfoOk));
    let app = harness.state();
    assert!(matches!(
        app.view().dialog.as_ref().map(|d| &d.body),
        Some(DialogBody::Link(state)) if state.error.is_some()
    ));
    assert_eq!(history_entries(app), entries + 1);
    // Supprimer.
    click(&mut harness, &fr(TextId::LinkDialogDelete));
    let app = harness.state();
    assert!(app.view().dialog.is_none());
    assert!(!task(app, succ).predecessors.iter().any(|l| l.predecessor == pred));
    assert_eq!(history_entries(app), entries + 2);
}

/// Geste refusé au lâcher (tâche commencée) : message au bandeau, rien
/// appliqué.
#[test]
fn gesture_started_refused_banner() {
    let mut harness = harness();
    open_demo(&mut harness);
    let (i, uid) = unstarted_leaf(harness.state());
    harness.state_mut().dispatch(Action::Exec(
        serde_json::from_value(serde_json::json!({
            "format_version": 1,
            "ops": [{ "op": "progress.update", "task": uid.get(), "percent_complete": { "value": 2000 } }]
        }))
        .unwrap(),
    ));
    harness.run();
    show_row(&mut harness, i);
    let day = px_per_day(&harness);
    let y = row_y(&harness, i);
    let (x0, x1) = bar_x(&harness, i);
    let generation_before = generation(harness.state());
    let from = pos2(f32::midpoint(x0, x1), y);
    drag(&mut harness, from, from + egui::vec2(2.0 * day, 0.0));
    let app = harness.state();
    assert_eq!(generation(app), generation_before);
    assert_eq!(app.messages().len(), 1, "{:?}", app.messages());
}
