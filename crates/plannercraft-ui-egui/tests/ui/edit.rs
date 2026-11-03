//! Édition des cellules et ligne vide de fin (lot L12) : saisie ouverte par
//! la frappe, F2 ou un double-clic, validée par Entrée (ligne suivante),
//! Tab (cellule suivante), un défilement ; Échap annule. Chaque saisie
//! devient un lot du registre (`Action::Exec`), annulable exactement.

#![allow(
    clippy::arithmetic_side_effects,
    reason = "tests : valeurs bornées par les stratégies et les fixtures"
)]

use egui::{Event, Key, Modifiers, PointerButton, Pos2, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_automation::text::{
    DateRole, TextContext, TextError, parse_date, parse_duration, parse_predecessors,
};
use plannercraft_core::{
    Constraint, ConversionFactors, Decimal, Duration, DurationUnit, Lag, LagValue, Minutes, Moment,
    TaskOutcome, TaskUid, TimeBasis,
};
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::edit::{EditError, date_edit_text, duration_edit_text, text_language};
use plannercraft_ui_egui::format::{format_date, format_duration};
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::nav::RowRef;
use plannercraft_ui_egui::{Action, App};
use proptest::prelude::*;

use crate::{fr, harness, open_demo};

/// Libellé d'accessibilité de la ligne `id` (nom de la tâche).
fn row_label(app: &App, index: usize) -> String {
    let rows = app.rows().unwrap();
    let row = rows.row(index).unwrap();
    Texts::new(Language::Fr).get_args(
        TextId::RowLabel,
        &[
            ("id", Arg::Count(i64::from(row.id))),
            ("name", Arg::Str(rows.text(index, Column::Name).to_owned().into())),
        ],
    )
}

/// Abscisse du centre de la colonne `column` dans une ligne de bord gauche
/// `left`.
fn column_center(app: &App, left: f32, column: Column) -> f32 {
    let mut x = left;
    for state in app.state.visible_columns() {
        let w = f32::from(state.width);
        if state.column == column {
            return x + w / 2.0;
        }
        x += w;
    }
    panic!("colonne {column:?} masquée");
}

/// `clicks` clics au point `p` dans la même image (deux : double-clic).
fn clicks_at(harness: &mut Harness<'static, App>, p: Pos2, clicks: usize) {
    harness.event(Event::PointerMoved(p));
    for _ in 0..clicks {
        for pressed in [true, false] {
            harness.event(Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
        }
    }
    harness.run();
}

/// Clic simple au point `p`.
fn click_at(harness: &mut Harness<'static, App>, p: Pos2) {
    clicks_at(harness, p, 1);
}

/// Point au centre de la cellule (ligne de libellé `label`, `column`).
fn cell_point(harness: &Harness<'static, App>, label: &str, column: Column) -> Pos2 {
    let rect = harness.get_by_label(label).rect();
    pos2(
        column_center(harness.state(), rect.left(), column),
        rect.center().y,
    )
}

/// Clique la cellule (ligne `index`, `column`) : la tâche est sélectionnée,
/// la colonne devient courante.
fn click_cell(harness: &mut Harness<'static, App>, index: usize, column: Column) {
    let label = row_label(harness.state(), index);
    let p = cell_point(harness, &label, column);
    click_at(harness, p);
    let uid = harness.state().rows().unwrap().row(index).unwrap().uid;
    assert_eq!(harness.state().selection, Some(uid));
    assert_eq!(harness.state().view().cursor.column, column);
}

/// Frappe `text` (ouvre la saisie si aucune n'est ouverte).
fn type_text(harness: &mut Harness<'static, App>, text: &str) {
    harness.event(Event::Text(text.to_owned()));
    harness.run();
}

/// Appuie sur `key`.
fn press(harness: &mut Harness<'static, App>, key: Key) {
    harness.key_press(key);
    harness.run();
}

/// Indice et UID de la tâche d'UID `n` de l'exemple.
fn index_of(app: &App, n: u32) -> usize {
    app.rows().unwrap().index_of(TaskUid::new(n)).unwrap()
}

/// Planning de la tâche `uid`.
fn schedule_of(app: &App, uid: TaskUid) -> plannercraft_core::TaskSchedule {
    match app.document.as_ref().unwrap().schedule().tasks.get(&uid) {
        Some(TaskOutcome::Scheduled(s)) => *s,
        other => panic!("{other:?}"),
    }
}

/// Exemple ouvert, la tâche d'UID 3 (« Plans de l'architecte ») cliquée
/// dans `column`.
fn demo_cell(column: Column) -> (Harness<'static, App>, usize, TaskUid) {
    let mut harness = harness();
    open_demo(&mut harness);
    let index = index_of(harness.state(), 3);
    click_cell(&mut harness, index, column);
    (harness, index, TaskUid::new(3))
}

#[test]
fn ui_edit_duration_updates_bar() {
    let (mut harness, index, uid) = demo_cell(Column::Duration);
    let before = harness.state().rows().unwrap().row(index).unwrap().bar;
    type_text(&mut harness, "3j");
    assert!(harness.state().view().edit.editing.is_some());
    assert!(
        harness
            .query_by_label(&cell_edit_label(Column::Duration))
            .is_some()
    );
    press(&mut harness, Key::Enter);
    let app = harness.state();
    assert!(app.view().edit.editing.is_none());
    assert!(app.messages().is_empty(), "{:?}", app.messages());
    let s = schedule_of(app, uid);
    assert_eq!(s.duration, Minutes::new(3 * 480));
    let row = app.rows().unwrap().row(index).unwrap();
    assert_eq!(row.bar, Some((s.start, s.finish)));
    assert_ne!(row.bar, before);
    // Barre de 3 jours ouvrés : du lundi 8 h au mercredi 17 h, ou décalée
    // par un week-end ; jamais plus de 5 jours calendaires.
    let days = i64::from(s.finish.day().index()) - i64::from(s.start.day().index());
    assert!((2..=4).contains(&days), "{:?} → {:?}", s.start, s.finish);
    assert!(app.rows().unwrap().text(index, Column::Duration).starts_with('3'));
    // Entrée descend d'une ligne.
    let next = app.rows().unwrap().row(index + 1).unwrap().uid;
    assert_eq!(app.selection, Some(next));
    // Le lot venait de l'interface : un commit, document modifié.
    assert_eq!(app.document.as_ref().unwrap().generation(), 1);
    assert!(app.window_title().starts_with('*'));
}

/// Libellé d'accessibilité de la zone de saisie de `column`.
fn cell_edit_label(column: Column) -> String {
    let texts = Texts::new(Language::Fr);
    texts.get_args(
        TextId::CellEditLabel,
        &[("column", Arg::Str(texts.get(column.title()).into()))],
    )
}

#[test]
fn ui_edit_invalid_stays_in_edit() {
    let (mut harness, _, uid) = demo_cell(Column::Duration);
    let before = harness.state().document.as_ref().unwrap().project().clone();
    type_text(&mut harness, "abc");
    press(&mut harness, Key::Enter);
    let app = harness.state();
    let editing = app.view().edit.editing.as_ref().expect("saisie gardée");
    assert_eq!(editing.buffer, "abc");
    let error = editing.error.clone().expect("refus affiché");
    assert!(
        matches!(error, EditError::Text(TextError::BadNumber { .. })),
        "{error:?}"
    );
    let message = error.message(&Texts::new(Language::Fr));
    assert!(
        harness.query_by_label(&message).is_some(),
        "message absent : {message}"
    );
    assert_eq!(app.document.as_ref().unwrap().project(), &before);
    assert_eq!(app.selection, Some(uid));
    // Une correction efface le message ; Échap abandonne la saisie.
    type_text(&mut harness, "x");
    assert!(
        harness
            .state()
            .view()
            .edit
            .editing
            .as_ref()
            .unwrap()
            .error
            .is_none()
    );
    assert!(harness.query_by_label(&message).is_none());
    press(&mut harness, Key::Escape);
    let app = harness.state();
    assert!(app.view().edit.editing.is_none());
    assert_eq!(app.document.as_ref().unwrap().project(), &before);
    assert_eq!(app.document.as_ref().unwrap().generation(), 0);
}

/// Relecture finale J4, F2 : saisie ouverte sur un document intact, puis
/// clic sur « Ouvrir l'exemple » (barre d'outils, dessinée avant la table).
/// La saisie, validée par la perte du focus dans la même image, modifie
/// d'abord le document : le bandeau de confirmation s'affiche au lieu de
/// remplacer le document et d'appliquer le lot au nouveau.
#[test]
fn ui_edit_focus_loss_commits_before_document_replaced() {
    let (mut harness, _, uid) = demo_cell(Column::Duration);
    assert!(!harness.state().document.as_ref().unwrap().is_dirty());
    // Valeur différente de la durée en place (R-260 d : la même valeur ne
    // modifierait rien).
    type_text(&mut harness, "11j");
    assert!(harness.state().view().edit.editing.is_some());
    // Fichier → Ouvrir l'exemple (bouton de la barre d'outils jusqu'au lot
    // L5) : le clic sur le menu retire déjà le focus de la cellule.
    crate::click(&mut harness, &fr(TextId::MenuFile));
    let open_demo = fr(TextId::MenuOpenDemo);
    crate::click_where(&mut harness, |text| text.starts_with(&open_demo));
    let app = harness.state();
    assert!(
        app.confirmation_pending(),
        "la saisie doit rendre le document modifié"
    );
    let doc = app.document.as_ref().unwrap();
    assert_eq!(doc.opening, 1, "le document ne doit pas être remplacé");
    assert!(doc.is_dirty());
    assert_eq!(schedule_of(app, uid).duration, Minutes::new(11 * 480));
}

#[test]
fn ui_blank_row_creates_task() {
    let mut harness = harness();
    open_demo(&mut harness);
    harness.state_mut().dispatch(Action::ScrollToRow(usize::MAX));
    harness.run();
    let blank = fr(TextId::BlankRowLabel);
    let p = cell_point(&harness, &blank, Column::Name);
    click_at(&mut harness, p);
    assert!(harness.state().selection.is_none());
    assert_eq!(harness.state().view().cursor.row, RowRef::Blank);
    let count = harness.state().document.as_ref().unwrap().project().tasks.len();
    for (k, name) in ["Réception", "Nettoyage"].into_iter().enumerate() {
        type_text(&mut harness, name);
        press(&mut harness, Key::Enter);
        let app = harness.state();
        assert!(app.messages().is_empty(), "{:?}", app.messages());
        let project = app.document.as_ref().unwrap().project();
        assert_eq!(project.tasks.len(), count + k + 1);
        let last = project.tasks.iter().last().unwrap();
        assert_eq!(last.name, name);
        assert_eq!(last.outline_level.get(), 1);
        // La ligne vide reste la ligne courante : saisie à la suite.
        assert!(app.selection.is_none());
        assert_eq!(app.view().cursor.row, RowRef::Blank);
    }
    let rows = harness.state().rows().unwrap();
    assert_eq!(rows.text(rows.len() - 1, Column::Name), "Nettoyage");
}

#[test]
fn ui_edit_start_sets_snet() {
    let mut harness = harness();
    open_demo(&mut harness);
    // Première tâche feuille sans prédécesseur (UID 2) : aucun lien ne la
    // pousse plus tard que la date saisie.
    let index = index_of(harness.state(), 2);
    click_cell(&mut harness, index, Column::Start);
    let uid = TaskUid::new(2);
    let start = schedule_of(harness.state(), uid).start;
    let wanted = Moment::from_minutes(start.minutes() + 28 * 24 * 60).unwrap();
    let date = wanted.to_naive().date();
    type_text(&mut harness, &date.format("%d/%m/%Y").to_string());
    press(&mut harness, Key::Enter);
    let app = harness.state();
    assert!(app.messages().is_empty(), "{:?}", app.messages());
    let task = app.document.as_ref().unwrap().project().tasks.get(uid).unwrap();
    let expected: Moment = format!("{}T08:00", date.format("%Y-%m-%d")).parse().unwrap();
    assert_eq!(task.constraint, Constraint::StartNoEarlierThan { date: expected });
    // R-158 visible dans la colonne Début.
    let s = schedule_of(app, uid);
    assert!(s.start >= expected, "{:?}", s.start);
    let shown = app.rows().unwrap().text(index, Column::Start);
    assert_eq!(
        shown,
        format_date(s.start, Texts::new(Language::Fr).format_labels())
    );
    assert_eq!(s.start.day(), expected.day(), "{shown}");
}

#[test]
fn ui_edit_predecessors_replaces_links() {
    let mut harness = harness();
    open_demo(&mut harness);
    // Tâche 9 « Installation du chantier » : prédécesseurs 7 et 6.
    let index = index_of(harness.state(), 9);
    let uid = TaskUid::new(9);
    assert_eq!(
        harness.state().rows().unwrap().text(index, Column::Predecessors),
        "7;6"
    );
    click_cell(&mut harness, index, Column::Predecessors);
    // F2 ouvre la saisie avec le texte affiché, curseur en fin.
    press(&mut harness, Key::F2);
    assert_eq!(
        harness.state().view().edit.editing.as_ref().unwrap().buffer,
        "7;6"
    );
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    type_text(&mut harness, "3;4DD+2j");
    press(&mut harness, Key::Enter);
    let app = harness.state();
    assert!(app.messages().is_empty(), "{:?}", app.messages());
    let task = app.document.as_ref().unwrap().project().tasks.get(uid).unwrap();
    let preds: Vec<(u32, plannercraft_core::LinkType)> = task
        .predecessors
        .iter()
        .map(|l| (l.predecessor.get(), l.kind))
        .collect();
    assert_eq!(
        preds,
        [
            (3, plannercraft_core::LinkType::FinishToStart),
            (4, plannercraft_core::LinkType::StartToStart)
        ]
    );
    assert_eq!(
        app.rows().unwrap().text(index, Column::Predecessors),
        "3;4DD+2 jours"
    );
    assert_eq!(app.document.as_ref().unwrap().generation(), 1, "un seul lot");
}

#[test]
fn ui_edit_then_undo_exact() {
    // Pas d'image court (1/60 s) : deux clics à moins de 0,3 s font un
    // double-clic (le harnais commun avance le temps par grands pas).
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1280.0, 720.0))
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .with_os(egui::os::OperatingSystem::Nix)
        .with_step_dt(1.0 / 60.0)
        .with_max_steps(8)
        .build_ui_state(plannercraft_ui_egui::ui, crate::new_app());
    open_demo(&mut harness);
    let index = index_of(harness.state(), 3);
    let project = harness.state().document.as_ref().unwrap().project().clone();
    let rows = harness.state().rows().unwrap().clone();
    // Double-clic sur la cellule Durée : saisie ouverte avec le texte affiché.
    let label = row_label(harness.state(), index);
    let p = cell_point(&harness, &label, Column::Duration);
    clicks_at(&mut harness, p, 2);
    let editing = harness.state().view().edit.editing.clone().expect("double-clic");
    assert_eq!(editing.column, Column::Duration);
    assert_eq!(editing.buffer, rows.text(index, Column::Duration));
    // Tab sans modification : rien n'est envoyé, la cellule suivante devient
    // courante.
    press(&mut harness, Key::Tab);
    assert_eq!(harness.state().view().cursor.column, Column::Start);
    assert_eq!(harness.state().document.as_ref().unwrap().generation(), 0);
    type_text(&mut harness, "04/01/2027");
    press(&mut harness, Key::Tab);
    assert_eq!(harness.state().view().cursor.column, Column::Finish);
    assert_eq!(harness.state().document.as_ref().unwrap().generation(), 1);
    assert_ne!(harness.state().document.as_ref().unwrap().project(), &project);
    harness.state_mut().dispatch(Action::Undo);
    harness.run();
    let app = harness.state();
    assert_eq!(app.document.as_ref().unwrap().project(), &project);
    assert_eq!(app.rows().unwrap(), &rows);
    assert!(!app.window_title().starts_with('*'));
}

/// Un défilement pendant la saisie la valide sur place.
#[test]
fn ui_edit_scroll_commits() {
    let (mut harness, _, uid) = demo_cell(Column::Name);
    type_text(&mut harness, "Plans");
    harness.event(Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -30.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
        modifiers: Modifiers::NONE,
    });
    harness.run();
    let app = harness.state();
    assert!(app.view().edit.editing.is_none());
    let task = app.document.as_ref().unwrap().project().tasks.get(uid).unwrap();
    assert_eq!(task.name, "Plans");
    assert_eq!(app.selection, Some(uid));
}

/// Les raccourcis de la table des raccourcis restent inactifs pendant la
/// saisie (Ctrl+Z appartient à la zone de saisie), puis reviennent.
#[test]
fn ui_edit_keymap_inactive_while_editing() {
    let (mut harness, _, _) = demo_cell(Column::Duration);
    type_text(&mut harness, "4j");
    press(&mut harness, Key::Enter);
    assert_eq!(harness.state().document.as_ref().unwrap().generation(), 1);
    type_text(&mut harness, "5j");
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run();
    assert!(harness.state().view().edit.editing.is_some());
    assert_eq!(history_entries(harness.state()), 1, "Ctrl+Z pris par la saisie");
    press(&mut harness, Key::Escape);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run();
    assert_eq!(history_entries(harness.state()), 0, "Ctrl+Z rendu à l'historique");
}

/// Réseau généré de 80 tâches, tâche de la ligne 2 sélectionnée (colonne
/// Nom), table défilée jusqu'à la ligne 60 : la ligne 2 est hors de l'écran.
fn offscreen_selection() -> (Harness<'static, App>, TaskUid) {
    let mut app = crate::new_app();
    let origin = plannercraft_ui_egui::DocOrigin {
        name: "réseau".to_owned(),
        path: None,
        demo: false,
    };
    app.open_project(plannercraft_tests::generate::ui_network(4, 19, 50), origin);
    let mut harness = crate::harness_for(app);
    harness.run();
    let uid = harness.state().rows().unwrap().row(2).unwrap().uid;
    harness.state_mut().dispatch(Action::Select(Some(uid)));
    harness.run();
    harness.state_mut().dispatch(Action::ScrollToRow(60));
    harness.run();
    assert_eq!(harness.state().view().cursor.column, Column::Name);
    assert!(
        !harness.state().last_frame.table_rows.contains(&2),
        "{:?}",
        harness.state().last_frame.table_rows
    );
    (harness, uid)
}

/// Nom de la tâche `uid`.
fn task_name(app: &App, uid: TaskUid) -> Option<String> {
    app.document
        .as_ref()
        .unwrap()
        .project()
        .tasks
        .get(uid)
        .map(|task| task.name.clone())
}

/// Relecture finale J7, F2 : une frappe sur une ligne hors de l'écran
/// amène la ligne à l'écran et la saisie reçoit tout le texte (« Revue »,
/// pas « R ») ; Retour arrière pendant la saisie efface un caractère, il ne
/// supprime pas la tâche.
#[test]
fn ui_edit_offscreen_row_is_revealed_and_keeps_typing() {
    let (mut harness, uid) = offscreen_selection();
    type_text(&mut harness, "R");
    type_text(&mut harness, "evue");
    assert!(
        harness.state().last_frame.table_rows.contains(&2),
        "ligne amenée à l'écran"
    );
    press(&mut harness, Key::Enter);
    assert_eq!(task_name(harness.state(), uid).as_deref(), Some("Revue"));

    let (mut harness, uid) = offscreen_selection();
    let tasks = harness.state().document.as_ref().unwrap().project().tasks.len();
    type_text(&mut harness, "R");
    // Retour arrière et frappe dans l'image qui suit l'ouverture (zone de
    // saisie pas encore focalisée).
    harness.key_press(Key::Backspace);
    harness.event(Event::Text("Z".to_owned()));
    harness.run();
    press(&mut harness, Key::Enter);
    let doc = harness.state().document.as_ref().unwrap();
    assert_eq!(doc.project().tasks.len(), tasks, "aucune tâche supprimée");
    assert_eq!(task_name(harness.state(), uid).as_deref(), Some("Z"));
}

/// Relecture finale J7, F6 : Retour arrière (« delete » d'un Mac) sur la
/// cellule Durée ouvre sa saisie vide ; la tâche n'est pas supprimée, Échap
/// abandonne sans rien changer.
#[test]
fn ui_backspace_clears_cell_not_task() {
    let (mut harness, _index, uid) = demo_cell(Column::Duration);
    let tasks = harness.state().document.as_ref().unwrap().project().tasks.len();
    press(&mut harness, Key::Backspace);
    let app = harness.state();
    assert_eq!(
        app.document.as_ref().unwrap().project().tasks.len(),
        tasks,
        "tâche supprimée"
    );
    let editing = app.view().edit.editing.clone().expect("saisie ouverte");
    assert_eq!(
        (editing.uid, editing.column, editing.buffer.as_str()),
        (Some(uid), Column::Duration, "")
    );
    press(&mut harness, Key::Escape);
    assert!(harness.state().view().edit.editing.is_none());
    assert_eq!(harness.state().document.as_ref().unwrap().generation(), 0);
}

fn history_entries(app: &App) -> usize {
    app.document.as_ref().unwrap().session.history().entries.len()
}

/// Libellés d'affichage d'une langue.
fn labels(language: Language) -> plannercraft_ui_egui::format::FormatLabels {
    Texts::new(language).format_labels().clone()
}

/// Contexte de lecture d'un projet par défaut dans `language`.
fn context(language: Language) -> TextContext {
    TextContext::for_options(
        &plannercraft_core::ProjectOptions::default(),
        text_language(language),
    )
}

fn unit_strategy() -> impl Strategy<Value = DurationUnit> {
    prop_oneof![
        Just(DurationUnit::Minute),
        Just(DurationUnit::Hour),
        Just(DurationUnit::Day),
        Just(DurationUnit::Week),
        Just(DurationUnit::Month),
    ]
}

/// Durée « saisissable » : une valeur entière ou à une décimale dans son
/// unité (ce qu'affiche MS Project), ouvrée ou écoulée, estimée ou non.
fn duration_strategy() -> impl Strategy<Value = Duration> {
    (unit_strategy(), 0_i64..2_000, any::<bool>(), any::<bool>()).prop_map(
        |(unit, tenths, elapsed, estimated)| {
            let basis = if elapsed {
                TimeBasis::Elapsed
            } else {
                TimeBasis::Working
            };
            let factors = ConversionFactors::default();
            let value = Decimal::new(tenths, 1).unwrap();
            let minutes = factors.to_minutes(value, unit, basis).unwrap();
            Duration::new(minutes, unit, basis, estimated).unwrap()
        },
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Affichage relu = valeur, en français et en anglais : le texte
    /// initial de la saisie (l'affichage, ou l'écriture R-141/R-143 du
    /// moteur s'il ne se relit pas) redonne la valeur ; l'affichage
    /// lui-même se relit pour toute durée ouvrée et toute date de 1984 à
    /// 2083.
    #[test]
    fn ui_parse_display_roundtrip(
        d in duration_strategy(),
        day in 0_i64..(100 * 365),
        minute_of_day in 0_i64..(24 * 60),
        fr_first in any::<bool>(),
        lag_days in -20_i64..20,
    ) {
        for language in if fr_first { [Language::Fr, Language::En] } else { [Language::En, Language::Fr] } {
            let l = labels(language);
            let ctx = context(language);
            let factors = ConversionFactors::default();

            let display = format_duration(&d, &factors, &l);
            let text = duration_edit_text(&d, &display, &ctx);
            prop_assert_eq!(parse_duration(&text, &ctx), Ok(d), "{} / {}", display, text);
            if d.basis() == TimeBasis::Working || language == Language::En {
                prop_assert_eq!(&text, &display);
            }

            // Dates de 1984 à 2083 : l'affichage (année sur deux chiffres) se
            // relit au même jour ; au-delà, l'écriture complète.
            let origin: Moment = "1984-01-01T00:00".parse().unwrap();
            let m = Moment::from_minutes(origin.minutes() + day * 1_440 + minute_of_day).unwrap();
            for role in [DateRole::Start, DateRole::Finish] {
                let display = format_date(m, &l);
                let text = date_edit_text(m, &display, role, &ctx);
                prop_assert_eq!(&text, &display);
                prop_assert_eq!(parse_date(&text, role, &ctx).unwrap().day(), m.day());
            }
            let late = Moment::from_minutes(m.minutes() + 100 * 365 * 1_440).unwrap_or(Moment::MAX);
            let text = date_edit_text(late, &format_date(late, &l), DateRole::Start, &ctx);
            prop_assert_eq!(parse_date(&text, DateRole::Start, &ctx).unwrap().day(), late.day());

            // Décalage affiché (« +2 jours ») relu par le texte des
            // prédécesseurs.
            let lag = Lag::new(
                TimeBasis::Working,
                LagValue::Time { minutes: Minutes::new(lag_days * 480), unit: DurationUnit::Day },
            )
            .unwrap();
            let mut shown = String::from("3");
            shown.push_str(if language == Language::Fr { "DD" } else { "SS" });
            plannercraft_ui_egui::format::write_lag(&mut shown, &lag, &factors, &l);
            let (refs, errors) = parse_predecessors(&shown, &ctx);
            prop_assert!(errors.is_empty(), "{} : {:?}", shown, errors);
            prop_assert_eq!(refs.len(), 1);
            prop_assert_eq!(refs[0].lag, lag, "{}", shown);
        }
    }
}

// H-UI-85 et H-UI-86 (correctif « saisie de cellule », 2026-10-27) : la
// zone de saisie a réellement le focus clavier (curseur de texte, flèches
// dans le texte) et ne reste jamais ouverte sur une autre cellule que la
// cellule active.

/// Harnais à pas d'image court (deux clics rapprochés font un double-clic),
/// exemple ouvert.
fn quick_harness() -> Harness<'static, App> {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1280.0, 720.0))
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .with_os(egui::os::OperatingSystem::Nix)
        .with_step_dt(1.0 / 60.0)
        .with_max_steps(8)
        .build_ui_state(plannercraft_ui_egui::ui, crate::new_app());
    open_demo(&mut harness);
    harness
}

/// La zone de saisie a-t-elle le focus clavier d'egui ?
fn editor_focused(harness: &Harness<'static, App>) -> bool {
    harness
        .ctx
        .memory(|memory| memory.has_focus(plannercraft_ui_egui::grid::editor_id()))
}

/// Position du curseur de texte (en caractères) de la zone de saisie.
fn caret(harness: &Harness<'static, App>) -> Option<usize> {
    egui::text_edit::TextEditState::load(&harness.ctx, plannercraft_ui_egui::grid::editor_id())
        .and_then(|state| state.cursor.char_range())
        .map(|range| range.primary.index.0)
}

/// Texte de la saisie ouverte.
fn buffer(harness: &Harness<'static, App>) -> Option<String> {
    harness
        .state()
        .view()
        .edit
        .editing
        .as_ref()
        .map(|editing| editing.buffer.clone())
}

/// Double-clic sur la cellule (ligne `index`, `column`).
fn double_click_cell(harness: &mut Harness<'static, App>, index: usize, column: Column) {
    let label = row_label(harness.state(), index);
    let p = cell_point(harness, &label, column);
    clicks_at(harness, p, 2);
}

/// Point du Gantt sur la ligne `index` (à droite de la table).
fn gantt_point(harness: &Harness<'static, App>, index: usize) -> Pos2 {
    let rect = harness.get_by_label(&row_label(harness.state(), index)).rect();
    pos2(rect.right() + 200.0, rect.center().y)
}

/// H-UI-85 : F2, la frappe et le double-clic donnent le focus clavier à la
/// zone de saisie (curseur de texte en fin de texte), et le gardent les
/// images suivantes. Le double-clic le perdait : `FocusPane(Table)`,
/// poussé par le même clic et appliqué après le dessin, le rendait à la
/// table.
#[test]
fn h_ui_85_editor_takes_keyboard_focus() {
    let mut harness = quick_harness();
    let index = index_of(harness.state(), 3);
    let name = harness
        .state()
        .rows()
        .unwrap()
        .text(index, Column::Name)
        .to_owned();
    click_cell(&mut harness, index, Column::Name);

    press(&mut harness, Key::F2);
    assert_eq!(buffer(&harness).as_deref(), Some(name.as_str()));
    assert!(editor_focused(&harness), "F2 : zone sans focus");
    assert!(harness.ctx.text_edit_focused());
    assert_eq!(caret(&harness), Some(name.chars().count()));
    press(&mut harness, Key::Escape);
    assert!(harness.state().view().edit.editing.is_none());

    type_text(&mut harness, "X");
    assert!(editor_focused(&harness), "frappe : zone sans focus");
    assert_eq!(buffer(&harness).as_deref(), Some("X"));
    assert_eq!(caret(&harness), Some(1));
    press(&mut harness, Key::Escape);

    double_click_cell(&mut harness, index, Column::Duration);
    let duration = harness
        .state()
        .rows()
        .unwrap()
        .text(index, Column::Duration)
        .to_owned();
    assert_eq!(buffer(&harness).as_deref(), Some(duration.as_str()));
    for _ in 0..3 {
        harness.run();
        assert!(editor_focused(&harness), "double-clic : zone sans focus");
    }
    assert_eq!(caret(&harness), Some(duration.chars().count()));
    // La table n'a pas le focus : ses raccourcis restent muets.
    assert!(
        !harness
            .ctx
            .memory(|memory| memory.has_focus(plannercraft_ui_egui::view::table::table_id()))
    );
}

/// H-UI-86 : ← → Origine Fin déplacent le curseur dans le texte, sans
/// toucher à la sélection ni à la cellule active ; Échap abandonne et garde
/// la cellule active.
#[test]
fn h_ui_86_horizontal_keys_move_caret_in_text() {
    let mut harness = quick_harness();
    let index = index_of(harness.state(), 3);
    let uid = TaskUid::new(3);
    let name = harness
        .state()
        .rows()
        .unwrap()
        .text(index, Column::Name)
        .to_owned();
    let n = name.chars().count();
    let project = harness.state().document.as_ref().unwrap().project().clone();
    double_click_cell(&mut harness, index, Column::Name);
    press(&mut harness, Key::ArrowLeft);
    press(&mut harness, Key::ArrowLeft);
    assert_eq!(caret(&harness), Some(n - 2));
    press(&mut harness, Key::Home);
    assert_eq!(caret(&harness), Some(0));
    type_text(&mut harness, "Z");
    assert_eq!(buffer(&harness), Some(format!("Z{name}")));
    press(&mut harness, Key::End);
    assert_eq!(caret(&harness), Some(n + 1));
    press(&mut harness, Key::ArrowRight);
    assert_eq!(caret(&harness), Some(n + 1));
    let app = harness.state();
    assert_eq!(app.selection, Some(uid));
    assert_eq!(app.view().cursor.column, Column::Name);
    assert!(editor_focused(&harness));
    press(&mut harness, Key::Escape);
    let app = harness.state();
    assert!(app.view().edit.editing.is_none());
    assert_eq!(app.document.as_ref().unwrap().project(), &project);
    assert_eq!(app.selection, Some(uid));
    assert_eq!(app.view().cursor.column, Column::Name);
}

/// H-UI-86 : ↓ valide puis descend, Maj+Entrée valide puis monte, ↑ valide
/// puis monte ; la colonne reste.
#[test]
fn h_ui_86_vertical_keys_commit_then_move() {
    let mut harness = quick_harness();
    let index = index_of(harness.state(), 3);
    let uid_at =
        |harness: &Harness<'static, App>, i: usize| harness.state().rows().unwrap().row(i).unwrap().uid;
    let (above, below) = (uid_at(&harness, index - 1), uid_at(&harness, index + 1));
    click_cell(&mut harness, index, Column::Duration);
    type_text(&mut harness, "3j");
    press(&mut harness, Key::ArrowDown);
    let app = harness.state();
    assert!(app.view().edit.editing.is_none());
    assert_eq!(schedule_of(app, TaskUid::new(3)).duration, Minutes::new(3 * 480));
    assert_eq!(app.selection, Some(below));
    assert_eq!(app.view().cursor.column, Column::Duration);

    type_text(&mut harness, "2j");
    harness.key_press_modifiers(Modifiers::SHIFT, Key::Enter);
    harness.run();
    let app = harness.state();
    assert!(app.view().edit.editing.is_none());
    assert_eq!(schedule_of(app, below).duration, Minutes::new(2 * 480));
    assert_eq!(app.selection, Some(TaskUid::new(3)));

    type_text(&mut harness, "4j");
    press(&mut harness, Key::ArrowUp);
    let app = harness.state();
    assert!(app.view().edit.editing.is_none());
    assert_eq!(schedule_of(app, TaskUid::new(3)).duration, Minutes::new(4 * 480));
    assert_eq!(app.selection, Some(above));
    assert_eq!(app.view().cursor.column, Column::Duration);
}

/// H-UI-86 (capture du propriétaire, 2026-10-27) : saisie ouverte par un
/// double-clic, puis clic sur une autre cellule : la saisie est validée et
/// fermée, la cellule cliquée devient active ; aucune zone de saisie ne
/// reste sur l'ancienne cellule.
#[test]
fn h_ui_86_click_other_cell_commits_and_closes() {
    let mut harness = quick_harness();
    let index = index_of(harness.state(), 3);
    let name = harness
        .state()
        .rows()
        .unwrap()
        .text(index, Column::Name)
        .to_owned();
    let other = harness.state().rows().unwrap().row(index + 2).unwrap().uid;
    double_click_cell(&mut harness, index, Column::Name);
    press(&mut harness, Key::End);
    type_text(&mut harness, " bis");
    let label = row_label(harness.state(), index + 2);
    let p = cell_point(&harness, &label, Column::Start);
    click_at(&mut harness, p);
    harness.run();
    let app = harness.state();
    assert!(app.view().edit.editing.is_none(), "zone de saisie périmée");
    assert_eq!(
        task_name(app, TaskUid::new(3)).as_deref(),
        Some(format!("{name} bis").as_str())
    );
    assert_eq!(app.selection, Some(other));
    assert_eq!(app.view().cursor.column, Column::Start);
    assert!(harness.query_by_label(&cell_edit_label(Column::Name)).is_none());
    // La table reprend le focus : les flèches naviguent de nouveau.
    press(&mut harness, Key::ArrowDown);
    let next = harness.state().rows().unwrap().row(index + 3).unwrap().uid;
    assert_eq!(harness.state().selection, Some(next));
}

/// H-UI-86 : un clic ailleurs (Gantt, ruban) valide et ferme la saisie.
#[test]
fn h_ui_86_click_elsewhere_commits_and_closes() {
    let mut harness = quick_harness();
    let index = index_of(harness.state(), 3);
    let name = harness
        .state()
        .rows()
        .unwrap()
        .text(index, Column::Name)
        .to_owned();
    click_cell(&mut harness, index, Column::Name);
    press(&mut harness, Key::F2);
    type_text(&mut harness, " A");
    let p = gantt_point(&harness, index);
    click_at(&mut harness, p);
    harness.run();
    let app = harness.state();
    assert!(app.view().edit.editing.is_none());
    assert_eq!(
        task_name(app, TaskUid::new(3)).as_deref(),
        Some(format!("{name} A").as_str())
    );

    click_cell(&mut harness, index, Column::Name);
    press(&mut harness, Key::F2);
    type_text(&mut harness, "B");
    let tab = fr(TextId::RibbonTabView);
    {
        let node = harness
            .query_all_by_label(&tab)
            .find(|node| crate::in_ribbon(&harness, node))
            .expect("onglet Affichage du ruban");
        node.click();
    }
    harness.run();
    let app = harness.state();
    assert!(app.view().edit.editing.is_none());
    assert_eq!(
        task_name(app, TaskUid::new(3)).as_deref(),
        Some(format!("{name} AB").as_str())
    );
}

/// H-UI-86 : saisie illisible (« abc » en Durée) : ni ↓, ni un clic sur
/// une autre cellule, ni un clic sur une autre ligne du Gantt ne déplacent
/// la sélection ; la zone reste ouverte sur sa cellule, avec le focus et le
/// message ; Échap abandonne sans rien changer.
#[test]
fn h_ui_86_invalid_entry_keeps_editor_and_selection() {
    let mut harness = quick_harness();
    let index = index_of(harness.state(), 3);
    let uid = TaskUid::new(3);
    let project = harness.state().document.as_ref().unwrap().project().clone();
    click_cell(&mut harness, index, Column::Duration);
    type_text(&mut harness, "abc");
    let still_open = |harness: &Harness<'static, App>, step: &str| {
        let app = harness.state();
        let editing = app
            .view()
            .edit
            .editing
            .as_ref()
            .unwrap_or_else(|| panic!("{step} : saisie fermée"));
        assert_eq!(
            (editing.uid, editing.column),
            (Some(uid), Column::Duration),
            "{step}"
        );
        assert_eq!(editing.buffer, "abc", "{step}");
        assert!(editing.error.is_some(), "{step} : refus absent");
        assert_eq!(app.selection, Some(uid), "{step} : sélection déplacée");
        assert!(editor_focused(harness), "{step} : zone sans focus");
    };
    press(&mut harness, Key::ArrowDown);
    still_open(&harness, "↓");
    let label = row_label(harness.state(), index + 2);
    let p = cell_point(&harness, &label, Column::Name);
    click_at(&mut harness, p);
    harness.run();
    still_open(&harness, "clic sur une cellule");
    let p = gantt_point(&harness, index + 2);
    click_at(&mut harness, p);
    harness.run();
    still_open(&harness, "clic dans le Gantt");
    press(&mut harness, Key::Escape);
    let app = harness.state();
    assert!(app.view().edit.editing.is_none());
    assert_eq!(app.document.as_ref().unwrap().project(), &project);
    assert_eq!(app.selection, Some(uid));
    assert_eq!(app.view().cursor.column, Column::Duration);
}

/// H-UI-86 : Ctrl/Cmd+Z dans la zone de saisie annule la frappe en cours,
/// pas une modification du projet.
#[test]
fn h_ui_86_undo_in_editor_reverts_text_only() {
    let mut harness = quick_harness();
    let index = index_of(harness.state(), 3);
    click_cell(&mut harness, index, Column::Duration);
    type_text(&mut harness, "4j");
    press(&mut harness, Key::Enter);
    assert_eq!(history_entries(harness.state()), 1);
    let up = harness.state().rows().unwrap().row(index).unwrap().uid;
    harness.state_mut().dispatch(Action::Select(Some(up)));
    harness.run();
    press(&mut harness, Key::F2);
    let original = buffer(&harness).unwrap();
    // Frappe plus d'une seconde plus tard : un pas d'annulation d'egui.
    for _ in 0..90 {
        harness.step();
    }
    type_text(&mut harness, "9");
    assert_eq!(buffer(&harness), Some(format!("{original}9")));
    for _ in 0..90 {
        harness.step();
    }
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run();
    assert_eq!(buffer(&harness), Some(original), "texte de la saisie annulé");
    assert_eq!(history_entries(harness.state()), 1, "projet intact");
}
