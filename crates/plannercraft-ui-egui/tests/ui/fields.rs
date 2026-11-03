//! Seconde passe UX (ADR-0019, lot L7) : valeurs et saisie des nouveaux
//! champs (H-UI-117, R-259 amendée), de bout en bout dans l'application :
//! colonne insérée par `ColumnOp::Insert`, valeur lue du modèle ou du
//! `Schedule` du moteur, saisie au clavier (une entrée d'annulation par
//! geste), copier-coller d'un bloc. Repères : projet « P »
//! (`plannercraft_tests::daily::plan_p`, UID n = ID n) et l'exemple embarqué.

#![allow(
    clippy::arithmetic_side_effects,
    clippy::many_single_char_names,
    reason = "tests : rangs bornés par l'exemple, noms courts des formules"
)]

use egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use plannercraft_automation::text::{Language as TextLanguage, TextContext};
use plannercraft_automation::{Batch, Command};
use plannercraft_core::baseline::variance;
use plannercraft_core::{
    BaselineNumber, ConstraintKind, Moment, Project, SchedulingMode, TaskOutcome, TaskSchedule, TaskUid,
};
use plannercraft_tests::daily::plan_p;
use plannercraft_ui_egui::action::{ColumnOp, PressMode, PressTarget, SelectOp};
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::edit::{
    CellClear, EditError, EditState, clear_cell, commit_edit, editable, writable,
};
use plannercraft_ui_egui::field::{Availability, availability};
use plannercraft_ui_egui::format::{format_date, format_slack, format_work};
use plannercraft_ui_egui::i18n::{Language, TextId, Texts};
use plannercraft_ui_egui::nav::RowRef;
use plannercraft_ui_egui::view::Pane;
use plannercraft_ui_egui::{Action, App, DocOrigin};

use crate::{fr, harness_for, new_app, open_demo};

/// UID `n`.
const fn uid(n: u32) -> TaskUid {
    TaskUid::new(n)
}

/// Harnais sur le projet « P ».
fn p_harness() -> Harness<'static, App> {
    let mut app = new_app();
    app.open_project(
        plan_p().project().clone(),
        DocOrigin {
            name: "P".to_owned(),
            path: None,
            demo: false,
        },
    );
    let mut harness = harness_for(app);
    harness.run();
    harness
}

/// Insère `columns` juste après le Nom (à l'écran), dans cet ordre.
fn show_columns(harness: &mut Harness<'static, App>, columns: &[Column]) {
    for (k, column) in columns.iter().enumerate() {
        harness.state_mut().dispatch(Action::Column(ColumnOp::Insert {
            at: 2 + k,
            column: *column,
        }));
    }
    harness.run();
}

/// Projet du document ouvert.
fn project(app: &App) -> &Project {
    app.document.as_ref().unwrap().project()
}

/// Entrées d'annulation.
fn history_len(app: &App) -> usize {
    app.document.as_ref().unwrap().session.history().entries.len()
}

/// Planning de la tâche `uid` (planifiée).
fn schedule_of(app: &App, uid: TaskUid) -> TaskSchedule {
    match app.document.as_ref().unwrap().schedule().tasks.get(&uid) {
        Some(TaskOutcome::Scheduled(s)) => *s,
        other => panic!("{other:?}"),
    }
}

/// Texte affiché de la cellule (tâche `uid`, `column`).
fn cell(app: &App, uid: TaskUid, column: Column) -> String {
    let rows = app.rows().unwrap();
    rows.text(rows.index_of(uid).unwrap(), column).to_owned()
}

/// Cellule active (`n`, `column`), table focalisée.
fn select(harness: &mut Harness<'static, App>, n: u32, column: Column, mode: PressMode) {
    harness.state_mut().dispatch(Action::SelectOp(SelectOp::Press {
        row: RowRef::Item(uid(n)),
        column,
        target: PressTarget::Cell,
        mode,
    }));
    harness.state_mut().dispatch(Action::FocusPane(Pane::Table));
    harness.run();
}

/// Frappe `text` dans la cellule (`n`, `column`) puis Entrée.
fn type_into(harness: &mut Harness<'static, App>, n: u32, column: Column, text: &str) {
    select(harness, n, column, PressMode::Replace);
    harness.event(Event::Text(text.to_owned()));
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();
}

/// Annule (Ctrl/Cmd+Z).
fn undo(harness: &mut Harness<'static, App>) {
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run();
}

/// Applique un lot JSON au document (préparation d'un scénario).
fn exec(harness: &mut Harness<'static, App>, ops: &serde_json::Value) {
    let batch = Batch::from_json(serde_json::json!({ "format_version": 1, "ops": ops })).unwrap();
    harness
        .state_mut()
        .execute(&batch, plannercraft_ui_egui::app::EditOrigin::User)
        .unwrap();
    harness.run();
}

/// H-UI-117 : Marge libre, Fin au plus tard, Début au plus tôt et écarts
/// (début, durée, travail) de la ligne de base 0 égaux aux valeurs du
/// `Schedule` et de `baseline::variance`, formatées comme la table, sur
/// « P » (Murs allongés après la ligne de base) et sur l'exemple embarqué.
#[test]
fn h_ui_117_values_from_engine() {
    let columns = [
        Column::FreeSlack,
        Column::LateFinish,
        Column::EarlyStart,
        Column::StartVariance,
        Column::DurationVariance,
        Column::WorkVariance,
    ];
    let mut p = p_harness();
    exec(
        &mut p,
        &serde_json::json!([
            { "op": "baseline.set" },
            { "op": "task.update", "uid": 3, "duration": { "text": "12j" } }
        ]),
    );
    let mut demo = harness_for(new_app());
    open_demo(&mut demo);
    for harness in [&mut p, &mut demo] {
        show_columns(harness, &columns);
        let app = harness.state();
        let doc = app.document.as_ref().unwrap();
        let labels = Texts::new(Language::Fr).format_labels().clone();
        let f = &doc.project().options.factors;
        let variances = variance(
            doc.project(),
            doc.schedule(),
            &doc.calendars(),
            BaselineNumber::PRIMARY,
        );
        let mut checked = 0;
        for task in doc.project().tasks.iter().filter(|t| !t.is_null) {
            let Some(TaskOutcome::Scheduled(s)) = doc.schedule().tasks.get(&task.uid()) else {
                continue;
            };
            let u = task.uid();
            assert_eq!(
                cell(app, u, Column::FreeSlack),
                format_slack(s.free_slack, f, &labels)
            );
            assert_eq!(
                cell(app, u, Column::LateFinish),
                format_date(s.late.finish, &labels)
            );
            assert_eq!(
                cell(app, u, Column::EarlyStart),
                format_date(s.early.start, &labels)
            );
            let v = variances.tasks.iter().find(|v| v.uid == u);
            let expected = |pick: &dyn Fn(&plannercraft_core::baseline::Variance) -> String| {
                v.filter(|v| v.baseline.is_some())
                    .map(|v| pick(&v.variance))
                    .unwrap_or_default()
            };
            assert_eq!(
                cell(app, u, Column::DurationVariance),
                expected(&|x| format_slack(x.duration, f, &labels))
            );
            assert_eq!(
                cell(app, u, Column::WorkVariance),
                expected(&|x| format_work(x.work, f, &labels))
            );
            assert_eq!(
                cell(app, u, Column::StartVariance),
                expected(&|x| format_slack(x.start, f, &labels))
            );
            checked += 1;
        }
        assert!(checked >= 7, "{checked}");
    }
    // « P » : Murs allongés de 2 j ⇒ écart de durée « 2 jours », Plâtrerie
    // décalée de 2 j ⇒ écart de début « 2 jours » ; Marge libre de
    // Livraison (jalon après Murs, rien derrière) calculée par le moteur.
    let app = p.state();
    assert_eq!(cell(app, uid(3), Column::DurationVariance), "2 jours");
    assert_eq!(cell(app, uid(6), Column::StartVariance), "2 jours");
    assert_eq!(cell(app, uid(2), Column::FreeSlack), "0 jour");
    // Exemple : aucune ligne de base ⇒ écarts vides.
    let app = demo.state();
    assert_eq!(cell(app, uid(3), Column::DurationVariance), "");
}

/// H-UI-117 : ligne de base 3 posée ⇒ colonnes Début3, Fin3, Durée3 et
/// Travail3 relues dans le modèle ; ligne de base 1 absente ⇒ vides ;
/// colonnes calculées après l'insertion et après une modification.
#[test]
fn h_ui_117_baseline_n_columns() {
    let mut harness = p_harness();
    exec(
        &mut harness,
        &serde_json::json!([{ "op": "baseline.set", "number": 3 }]),
    );
    show_columns(
        &mut harness,
        &[
            Column::Baseline3Start,
            Column::Baseline3Finish,
            Column::Baseline3Duration,
            Column::Baseline1Start,
        ],
    );
    let labels = Texts::new(Language::Fr).format_labels().clone();
    let app = harness.state();
    let three = BaselineNumber::new(3).unwrap();
    for n in [2, 3, 6, 7] {
        let task = project(app).tasks.get(uid(n)).unwrap();
        let base = task.baselines.get(&three).unwrap();
        assert_eq!(
            cell(app, uid(n), Column::Baseline3Start),
            format_date(base.start.unwrap(), &labels)
        );
        assert_eq!(
            cell(app, uid(n), Column::Baseline3Finish),
            format_date(base.finish.unwrap(), &labels)
        );
        assert_eq!(cell(app, uid(n), Column::Baseline1Start), "");
    }
    assert_eq!(cell(app, uid(3), Column::Baseline3Duration), "10 jours");
    // Après une modification (nouvelle génération, modèle reconstruit) :
    // la ligne de base ne bouge pas, la colonne est toujours servie.
    type_into(&mut harness, 3, Column::Duration, "12j");
    let app = harness.state();
    assert_eq!(cell(app, uid(3), Column::Baseline3Duration), "10 jours");
    assert_eq!(cell(app, uid(3), Column::Duration), "12 jours");
}

/// H-UI-117 : Type et Date de contrainte, Échéance, Travail, Actif,
/// Jalon, Notes et Successeurs se saisissent dans la table ; chaque saisie
/// est une entrée d'annulation, Ctrl+Z la défait exactement.
#[test]
#[allow(clippy::too_many_lines, reason = "un bloc par champ modifiable")]
fn h_ui_117_editable_fields_one_undo() {
    let mut harness = p_harness();
    show_columns(
        &mut harness,
        &[
            Column::ConstraintType,
            Column::ConstraintDate,
            Column::Deadline,
            Column::Work,
            Column::Active,
            Column::Successors,
            Column::Notes,
            Column::Milestone,
        ],
    );
    // Travail : la tâche 2 reçoit une ressource de travail (le registre
    // refuse un travail saisi sans affectation).
    exec(
        &mut harness,
        &serde_json::json!([
            { "op": "resource.add", "name": "Maçon" },
            { "op": "assignment.add", "task": 2, "resource": 1 }
        ]),
    );
    let before = project(harness.state()).clone();
    let base = history_len(harness.state());
    let mut entries = base;
    let mut step = |harness: &mut Harness<'static, App>, n: u32, column: Column, text: &str| {
        type_into(harness, n, column, text);
        entries += 1;
        let app = harness.state();
        assert_eq!(
            history_len(app),
            entries,
            "{column:?} « {text} » : {:?}",
            app.messages()
        );
        assert!(
            app.view().edit.editing.is_none(),
            "{column:?} : saisie restée ouverte"
        );
    };
    // Type de contrainte : « Doit commencer le » à la date planifiée.
    let start = schedule_of(harness.state(), uid(6)).start;
    step(&mut harness, 6, Column::ConstraintType, "Doit commencer le");
    let task = project(harness.state()).tasks.get(uid(6)).unwrap();
    assert_eq!(task.constraint.kind(), ConstraintKind::MustStartOn);
    assert_eq!(task.constraint.date(), Some(start));
    assert_eq!(
        cell(harness.state(), uid(6), Column::ConstraintType),
        "Doit commencer le"
    );
    // Date de contrainte d'une tâche « dès que possible » ⇒ « Début au plus
    // tôt le » à cette date.
    step(&mut harness, 7, Column::ConstraintDate, "30/11/2026");
    let task = project(harness.state()).tasks.get(uid(7)).unwrap();
    assert_eq!(task.constraint.kind(), ConstraintKind::StartNoEarlierThan);
    assert_eq!(
        cell(harness.state(), uid(7), Column::ConstraintDate),
        "lun 30/11/26"
    );
    // Échéance.
    step(&mut harness, 3, Column::Deadline, "15/12/2026");
    let deadline = project(harness.state()).tasks.get(uid(3)).unwrap().deadline;
    assert_eq!(
        deadline.map(Moment::day),
        Some("2026-12-15T17:00".parse::<Moment>().unwrap().day())
    );
    // Travail : nombre sans unité ⇒ heures.
    step(&mut harness, 2, Column::Work, "16");
    let work = project(harness.state()).tasks.get(uid(2)).unwrap().work;
    assert_eq!(work.minutes().get(), 16 * 60);
    // Actif : « non » ⇒ tâche inactive (barrée), relue « Non ».
    step(&mut harness, 7, Column::Active, "non");
    assert!(!project(harness.state()).tasks.get(uid(7)).unwrap().active);
    assert_eq!(cell(harness.state(), uid(7), Column::Active), "Non");
    // Successeurs : 2 → 6 ajouté (2 a 3 pour successeur).
    assert_eq!(cell(harness.state(), uid(2), Column::Successors), "3");
    step(&mut harness, 2, Column::Successors, "3;6");
    let preds: Vec<u32> = project(harness.state())
        .tasks
        .get(uid(6))
        .unwrap()
        .predecessors
        .iter()
        .map(|l| l.predecessor.get())
        .collect();
    assert_eq!(preds, [3, 2]);
    assert_eq!(cell(harness.state(), uid(2), Column::Successors), "3;6");
    // Notes, Jalon.
    step(&mut harness, 4, Column::Notes, "Remise des clés");
    assert_eq!(
        project(harness.state()).tasks.get(uid(4)).unwrap().notes,
        "Remise des clés"
    );
    step(&mut harness, 3, Column::Milestone, "o");
    assert!(project(harness.state()).tasks.get(uid(3)).unwrap().milestone);
    // Valeur identique : rien (R-260 d).
    type_into(&mut harness, 3, Column::Milestone, "Oui");
    type_into(&mut harness, 2, Column::Successors, "3;6");
    assert_eq!(history_len(harness.state()), entries);
    // Chaque Ctrl+Z défait une saisie ; toutes défaites ⇒ projet d'origine.
    for k in (base..entries).rev() {
        undo(&mut harness);
        assert_eq!(history_len(harness.state()), k);
    }
    assert_eq!(project(harness.state()), &before);
}

/// H-UI-117 : un champ calculé (Marge libre, Fin au plus tard, Priorité
/// sans opération du registre) n'ouvre pas de saisie, ne change rien ;
/// sa lecture refuse « Ce champ est calculé. » ; collé, il est sauté.
#[test]
fn h_ui_117_computed_refused() {
    for column in [
        Column::FreeSlack,
        Column::LateFinish,
        Column::Priority,
        Column::UniqueId,
    ] {
        assert!(!editable(column), "{column:?}");
        assert_eq!(availability(column), Availability::ReadOnly, "{column:?}");
    }
    assert_eq!(availability(Column::Deadline), Availability::Ready);
    assert!(matches!(availability(Column::Cost), Availability::Unavailable(_)));
    let mut harness = p_harness();
    show_columns(&mut harness, &[Column::FreeSlack, Column::Priority]);
    let free = cell(harness.state(), uid(2), Column::FreeSlack);
    type_into(&mut harness, 2, Column::FreeSlack, "3j");
    let app = harness.state();
    assert_eq!(history_len(app), 0);
    assert!(app.view().edit.editing.is_none());
    assert_eq!(cell(app, uid(2), Column::FreeSlack), free);
    assert_eq!(cell(app, uid(2), Column::Priority), "500");
    // Lecture d'une saisie : refus nommé, sans jargon.
    let session = &app.document.as_ref().unwrap().session;
    let err = commit_edit(
        &EditState::typed(Some(uid(2)), Column::FreeSlack, "3j"),
        session,
        TextLanguage::Fr,
    )
    .unwrap_err();
    assert_eq!(err, EditError::Computed);
    assert_eq!(err.message(app.texts()), fr(TextId::EditErrorComputed));
    assert_eq!(err.message(app.texts()), "Ce champ est calculé.");
    // Collage sur la colonne calculée : sauté, rien écrit.
    select(&mut harness, 2, Column::FreeSlack, PressMode::Replace);
    harness.event(Event::Paste("4j".to_owned()));
    harness.run();
    assert_eq!(history_len(harness.state()), 0);
}

/// H-UI-117 : champ à choix lu par son préfixe : « man » ⇒ Manuelle,
/// « au » ⇒ Automatique ; « Durée f » ⇒ Durée fixe ; préfixe ambigu ou
/// inconnu ⇒ saisie gardée avec la liste des valeurs admises.
#[test]
fn h_ui_117_choice_prefix() {
    let mut harness = p_harness();
    show_columns(&mut harness, &[Column::Mode, Column::TaskType]);
    type_into(&mut harness, 3, Column::Mode, "man");
    let app = harness.state();
    assert!(matches!(
        project(app).tasks.get(uid(3)).unwrap().mode,
        SchedulingMode::Manual { .. }
    ));
    assert_eq!(cell(app, uid(3), Column::Mode), "Manuelle");
    assert_eq!(history_len(app), 1);
    type_into(&mut harness, 3, Column::Mode, "AU");
    assert!(matches!(
        project(harness.state()).tasks.get(uid(3)).unwrap().mode,
        SchedulingMode::Auto
    ));
    type_into(&mut harness, 2, Column::TaskType, "durée f");
    assert_eq!(cell(harness.state(), uid(2), Column::TaskType), "Durée fixe");
    assert_eq!(history_len(harness.state()), 3);
    // Inconnu : saisie gardée, refus sous la cellule, rien écrit.
    type_into(&mut harness, 2, Column::Mode, "zz");
    let app = harness.state();
    assert_eq!(history_len(app), 3);
    let state = app.view().edit.editing.as_ref().expect("saisie gardée");
    let Some(EditError::BadChoice(list)) = &state.error else {
        panic!("{:?}", state.error);
    };
    assert_eq!(list, "Automatique, Manuelle");
    assert_eq!(
        state.error.as_ref().unwrap().message(app.texts()),
        "Valeur non reconnue\u{a0}: saisissez Automatique, Manuelle."
    );
    // Préfixe ambigu (« Capacité fixe », « Durée fixe », « Travail fixe »
    // n'ont pas de préfixe commun ; « D » sur les contraintes : « Dès que
    // possible », « Doit… », « Début… ») : refus.
    let session = &app.document.as_ref().unwrap().session;
    let ambiguous = EditState::typed(Some(uid(2)), Column::ConstraintType, "d");
    assert!(matches!(
        commit_edit(&ambiguous, session, TextLanguage::Fr),
        Err(EditError::BadChoice(_))
    ));
    // Anglais : libellés de la langue du lot.
    let en = EditState::typed(Some(uid(2)), Column::Mode, "man");
    let batch = commit_edit(&en, session, TextLanguage::En).unwrap();
    assert_eq!(batch.ops.len(), 1);
}

/// R-259 (amendée, lot L7) : Notes ⇒ vides ; Échéance ⇒ retirée ;
/// Successeurs ⇒ liens retirés ; Type ou Date de contrainte ⇒ contrainte
/// par défaut ; % travail achevé ⇒ 0 % ; champs à choix, Travail, dates
/// et durées réelles ⇒ non effaçables ; champ calculé ⇒ non effaçable.
#[test]
fn r259_clear_new_fields() {
    let mut harness = p_harness();
    exec(
        &mut harness,
        &serde_json::json!([
            { "op": "task.update", "uid": 3, "notes": "Voir plan" },
            { "op": "task.set_deadline", "uid": 3, "deadline": "2026-12-15T17:00" },
            { "op": "task.set_constraint", "uid": 3, "kind": "snet", "date": "2026-10-26T08:00" }
        ]),
    );
    let app = harness.state();
    let doc = app.document.as_ref().unwrap();
    let p = doc.project();
    let ctx = TextContext::for_project(p, TextLanguage::Fr);
    let task = p.tasks.get(uid(3)).unwrap();
    let ops = |column| match clear_cell(task, p, column, &ctx) {
        CellClear::Ops(ops) => ops,
        other => panic!("{column:?} : {other:?}"),
    };
    assert!(matches!(&ops(Column::Notes)[..], [Command::TaskUpdate(_)]));
    assert!(matches!(
        &ops(Column::Deadline)[..],
        [Command::TaskSetDeadline(_)]
    ));
    assert!(matches!(
        &ops(Column::ConstraintType)[..],
        [Command::TaskSetConstraint(_)]
    ));
    assert!(matches!(
        &ops(Column::ConstraintDate)[..],
        [Command::TaskSetConstraint(_)]
    ));
    // Murs a deux successeurs (4 et 6) : deux retraits.
    let succ = ops(Column::Successors);
    assert_eq!(succ.len(), 2);
    assert!(succ.iter().all(|op| matches!(op, Command::LinkRemove(_))));
    for column in [
        Column::Work,
        Column::Milestone,
        Column::Mode,
        Column::TaskType,
        Column::ActualStart,
        Column::RemainingDuration,
        Column::FreeSlack,
    ] {
        assert_eq!(
            clear_cell(task, p, column, &ctx),
            CellClear::NotClearable,
            "{column:?}"
        );
    }
    // Déjà vides : rien.
    let fresh = p.tasks.get(uid(7)).unwrap();
    for column in [
        Column::Notes,
        Column::Deadline,
        Column::Successors,
        Column::PercentWorkComplete,
    ] {
        assert_eq!(
            clear_cell(fresh, p, column, &ctx),
            CellClear::Nothing,
            "{column:?}"
        );
    }
    // Dans l'application : bloc Notes-Échéance de 3, Ctrl+Suppr ⇒ une
    // entrée, les deux vidés.
    show_columns(&mut harness, &[Column::Notes, Column::Deadline]);
    let entries = history_len(harness.state());
    select(&mut harness, 3, Column::Notes, PressMode::Replace);
    select(&mut harness, 3, Column::Deadline, PressMode::Extend);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Delete);
    harness.run();
    let app = harness.state();
    assert_eq!(history_len(app), entries + 1, "{:?}", app.messages());
    let task = project(app).tasks.get(uid(3)).unwrap();
    assert_eq!((task.notes.as_str(), task.deadline), ("", None));
    assert_eq!(cell(app, uid(3), Column::Notes), "");
}

/// H-UI-117, R-257 : bloc Notes et Actif de 2 à 4 copié, collé sur 5 à 7
/// ⇒ valeurs recopiées en une entrée d'annulation ; récapitulative 5 :
/// Notes et Actif modifiables ; recollé ⇒ rien (R-260 d).
#[test]
fn h_ui_117_copy_paste_new_columns() {
    let mut harness = p_harness();
    exec(
        &mut harness,
        &serde_json::json!([
            { "op": "task.update", "uid": 2, "notes": "Terrassement" },
            { "op": "task.update", "uid": 3, "notes": "Parpaings", "active": false },
            { "op": "task.update", "uid": 4, "notes": "Clés" }
        ]),
    );
    show_columns(&mut harness, &[Column::Notes, Column::Active]);
    let entries = history_len(harness.state());
    select(&mut harness, 2, Column::Notes, PressMode::Replace);
    select(&mut harness, 4, Column::Active, PressMode::Extend);
    harness.event(Event::Copy);
    harness.step();
    let copied = harness
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|c| match c {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        })
        .expect("texte copié");
    harness.run();
    assert_eq!(copied, "Terrassement\tOui\nParpaings\tNon\nClés\tOui");
    select(&mut harness, 5, Column::Notes, PressMode::Replace);
    harness.event(Event::Paste(copied.clone()));
    harness.run();
    let app = harness.state();
    assert_eq!(history_len(app), entries + 1, "{:?}", app.messages());
    let p = project(app);
    let got: Vec<(&str, bool)> = [5, 6, 7]
        .iter()
        .map(|n| {
            let t = p.tasks.get(uid(*n)).unwrap();
            (t.notes.as_str(), t.active)
        })
        .collect();
    assert_eq!(
        got,
        [("Terrassement", true), ("Parpaings", false), ("Clés", true)]
    );
    // Recollé tel quel : aucune entrée.
    select(&mut harness, 5, Column::Notes, PressMode::Replace);
    harness.event(Event::Paste(copied));
    harness.run();
    assert_eq!(history_len(harness.state()), entries + 1);
    // Une récapitulative n'a ni Travail ni Mode modifiables.
    let rows = harness.state().rows().unwrap();
    let summary = rows.row(rows.index_of(uid(5)).unwrap()).unwrap();
    assert!(writable(summary, Column::Notes) && writable(summary, Column::Active));
    assert!(!writable(summary, Column::Work) && !writable(summary, Column::Mode));
}
