//! « Informations sur la tâche » (J7, lot L9, H-UI-19) : brouillon et lot
//! (`TaskInfoDraft::to_batch`, testé sans egui), ouverture par Maj+F2, par
//! un double-clic sur l'ID ou par le menu contextuel, « OK » en un seul
//! lot annulable, Échap, document changé entre-temps, touches retenues par
//! la fenêtre.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_automation::text::Language as TextLanguage;
use plannercraft_automation::{ApplyMode, Batch, Command, Session};
use plannercraft_core::{ConstraintKind, LinkType, ResourceUid, TaskUid};
use plannercraft_ui_egui::action::DialogKind;
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::dialog::task_info::{AssignRow, LinkRow, Tab, TaskInfoDraft, TaskInfoState};
use plannercraft_ui_egui::dialog::{DialogBody, DialogState};
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::keymap::KEYMAP;
use plannercraft_ui_egui::{Action, App};

use crate::{fr, harness, new_app, open_demo};

/// Session de l'exemple (UID n = ID n) avec deux ressources : « Maçon »
/// (UID 1, travail) et « Ciment » (UID 2, matériel), aucune affectation.
fn session() -> Session {
    let mut s = plannercraft_ui_egui::document::load(plannercraft_ui_egui::DEMO_MSPDI, crate::now())
        .unwrap()
        .session;
    let batch: Batch = serde_json::from_value(serde_json::json!({
        "format_version": 1,
        "ops": [
            { "op": "resource.add", "name": "Maçon" },
            { "op": "resource.add", "name": "Ciment", "kind": "material", "material_label": "t" },
            { "op": "assignment.add", "task": 9, "resource": 1, "units": { "text": "50%" } }
        ]
    }))
    .unwrap();
    s.apply(&batch, ApplyMode::Commit).unwrap();
    s
}

fn uid(n: u32) -> TaskUid {
    TaskUid::new(n)
}

fn draft(s: &Session, n: u32) -> TaskInfoDraft {
    TaskInfoDraft::from_session(s, uid(n), (1, s.generation()), TextLanguage::Fr).unwrap()
}

fn ids(batch: &Batch) -> Vec<&'static str> {
    batch.ops.iter().map(Command::id).collect()
}

#[test]
fn task_info_to_batch_unchanged_none() {
    let s = session();
    for n in [1, 3, 9] {
        let d = draft(&s, n);
        assert_eq!(d.to_batch(&s, TextLanguage::Fr), Ok(None), "tâche {n}");
        // En anglais aussi (textes écrits et relus dans la même langue).
        let en = TaskInfoDraft::from_session(&s, uid(n), (1, s.generation()), TextLanguage::En).unwrap();
        assert_eq!(en.to_batch(&s, TextLanguage::En), Ok(None), "tâche {n} (en)");
    }
    let d = draft(&s, 9);
    assert_eq!(d.links.len(), 2);
    assert_eq!(d.assignments.len(), 1);
    assert_eq!(d.assignments[0].units, "50%");
    // Récapitulative relue comme telle.
    assert!(draft(&s, 1).summary);
    assert!(!d.summary);
    // Tâche absente : aucun brouillon.
    assert!(TaskInfoDraft::from_session(&s, uid(999), (1, 0), TextLanguage::Fr).is_none());
}

/// Tous les onglets changés : un seul lot, dans l'ordre du contrat, qui
/// s'applique et donne les valeurs saisies.
#[test]
fn task_info_to_batch_order() {
    let mut s = session();
    let mut d = draft(&s, 9);
    d.general.name = "Gros œuvre".to_owned();
    d.general.duration = "4j".to_owned();
    d.general.percent = "25".to_owned();
    // Prédécesseurs « 7;6 » → 6 gardé en DD, 7 retiré, 3 ajouté.
    d.links.retain(|row| row.id == "6");
    d.links[0].kind = LinkType::StartToStart;
    d.links.push(LinkRow {
        id: "3".to_owned(),
        kind: LinkType::FinishToStart,
        lag: "+1j".to_owned(),
    });
    // Maçon passé à 100 %, Ciment ajouté (quantité 5).
    d.assignments[0].units = "100".to_owned();
    d.assignments.push(AssignRow {
        resource: ResourceUid::new(2),
        units: "5".to_owned(),
        work: String::new(),
    });
    d.advanced.constraint = ConstraintKind::StartNoEarlierThan;
    d.advanced.constraint_date = "01/03/2027".to_owned();
    d.advanced.deadline = "30/04/2027".to_owned();
    d.notes = "Béton C25".to_owned();
    let batch = d.to_batch(&s, TextLanguage::Fr).unwrap().unwrap();
    assert_eq!(
        ids(&batch),
        [
            "task.update",
            "link.add",
            "link.update",
            "link.remove",
            "assignment.add",
            "assignment.update",
            "task.set_constraint",
            "task.set_deadline",
            "progress.update",
        ]
    );
    assert_eq!(batch.lang, Some(TextLanguage::Fr));
    s.apply(&batch, ApplyMode::Commit).unwrap();
    let task = s.project().tasks.get(uid(9)).unwrap();
    assert_eq!(task.name, "Gros œuvre");
    assert_eq!(task.duration.minutes().get(), 4 * 480);
    assert_eq!(task.notes, "Béton C25");
    assert_eq!(task.constraint.kind(), ConstraintKind::StartNoEarlierThan);
    assert!(task.deadline.is_some());
    assert_eq!(task.progress.percent_complete.get(), 2_500);
    let preds: Vec<(u32, LinkType)> = task
        .predecessors
        .iter()
        .map(|l| (l.predecessor.get(), l.kind))
        .collect();
    assert_eq!(preds.len(), 2);
    assert!(preds.contains(&(6, LinkType::StartToStart)));
    assert!(preds.contains(&(3, LinkType::FinishToStart)));
    let assigned: Vec<_> = s
        .project()
        .assignments
        .iter()
        .filter(|a| a.task == uid(9))
        .collect();
    assert_eq!(assigned.len(), 2);
    // Relu après application : plus rien à envoyer.
    let again = draft(&s, 9);
    assert_eq!(again.to_batch(&s, TextLanguage::Fr), Ok(None));
    // Retrait de toutes les affectations et de tous les liens.
    let mut d = again;
    d.links.clear();
    d.assignments.clear();
    let batch = d.to_batch(&s, TextLanguage::Fr).unwrap().unwrap();
    assert_eq!(
        ids(&batch),
        [
            "link.remove",
            "link.remove",
            "assignment.remove",
            "assignment.remove"
        ]
    );
}

#[test]
fn task_info_field_errors_per_tab() {
    let s = session();
    let mut d = draft(&s, 9);
    d.general.duration = "abc".to_owned();
    d.general.start = "31/02/2027".to_owned();
    d.links.push(LinkRow {
        id: "900".to_owned(),
        kind: LinkType::FinishToStart,
        lag: String::new(),
    });
    d.links.push(LinkRow {
        id: "7".to_owned(),
        kind: LinkType::FinishToStart,
        lag: String::new(),
    });
    d.assignments[0].units = "beaucoup".to_owned();
    d.advanced.constraint = ConstraintKind::MustStartOn;
    d.advanced.constraint_date = String::new();
    d.advanced.deadline = "demain".to_owned();
    let errors = d.to_batch(&s, TextLanguage::Fr).unwrap_err();
    let got: Vec<(Tab, &str, Option<usize>)> = errors.iter().map(|e| (e.tab, e.field, e.row)).collect();
    for want in [
        (Tab::General, "duration", None),
        (Tab::General, "start", None),
        (Tab::Predecessors, "predecessor", Some(2)),
        (Tab::Predecessors, "predecessor", Some(3)),
        (Tab::Resources, "units", Some(0)),
        (Tab::Advanced, "constraint_date", None),
        (Tab::Advanced, "deadline", None),
    ] {
        assert!(got.contains(&want), "{want:?} absent de {got:?}");
    }
    assert!(errors.iter().all(|e| !e.message.is_empty()));
    // L'ID inconnu a le message de la table.
    let texts = Texts::new(Language::Fr);
    let unknown = texts.get_args(TextId::EditErrorUnknownId, &[("id", Arg::Count(900))]);
    assert!(errors.iter().any(|e| e.message == unknown), "{errors:?}");
}

/// Ouvre la fenêtre de la tâche `n` par l'action du réducteur.
fn open_info(harness: &mut Harness<'static, App>, n: u32) {
    harness
        .state_mut()
        .dispatch(Action::ShowDialog(DialogKind::TaskInformation(uid(n))));
    harness.run();
}

/// État de la fenêtre ouverte.
fn info(app: &App) -> Option<&TaskInfoState> {
    match &app.view().dialog {
        Some(DialogState {
            body: DialogBody::TaskInfo(state),
            ..
        }) => Some(state),
        _ => None,
    }
}

fn generation(app: &App) -> u64 {
    app.document.as_ref().unwrap().generation()
}

fn history_entries(app: &App) -> usize {
    app.document.as_ref().unwrap().session.history().entries.len()
}

/// Remplace le texte du champ nommé `label` par `text`.
fn retype(harness: &mut Harness<'static, App>, label: &str, text: &str) {
    let node = harness
        .query_by_role_and_label(egui::accesskit::Role::TextInput, label)
        .unwrap_or_else(|| harness.get_by_role_and_label(egui::accesskit::Role::MultilineTextInput, label));
    node.focus();
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.event(Event::Text(text.to_owned()));
    harness.run();
}

/// Document changé entre l'ouverture et « OK » : rien n'est appliqué, le
/// brouillon est relu et l'avertissement affiché ; le second « OK »
/// applique.
#[test]
fn task_info_stale_key_refused() {
    let mut harness = harness();
    open_demo(&mut harness);
    open_info(&mut harness, 3);
    retype(&mut harness, &fr(TextId::TaskInfoName), "Plans revus");
    // Une autre édition (agent, autre fenêtre…) change le document.
    let other: Batch = serde_json::from_value(serde_json::json!({
        "format_version": 1,
        "ops": [{ "op": "task.update", "uid": 4, "name": "Dépôt" }]
    }))
    .unwrap();
    harness
        .state_mut()
        .execute(&other, plannercraft_ui_egui::EditOrigin::Agent)
        .unwrap();
    harness.run();
    assert_eq!(generation(harness.state()), 1);
    click_label(&mut harness, &fr(TextId::TaskInfoOk));
    let app = harness.state();
    assert_eq!(generation(app), 1, "rien appliqué");
    let state = info(app).expect("fenêtre gardée");
    assert!(state.stale);
    assert_eq!(
        state.draft.general.name, "Plans de l'architecte",
        "brouillon relu"
    );
    assert_eq!(state.draft.key, app.document.as_ref().unwrap().cache_key());
    assert!(harness.query_by_label(&fr(TextId::TaskInfoStale)).is_some());
    // Second essai sur le brouillon relu : appliqué.
    retype(&mut harness, &fr(TextId::TaskInfoName), "Plans revus");
    click_label(&mut harness, &fr(TextId::TaskInfoOk));
    let app = harness.state();
    assert!(info(app).is_none());
    assert_eq!(generation(app), 2);
    let task = app
        .document
        .as_ref()
        .unwrap()
        .project()
        .tasks
        .get(uid(3))
        .unwrap();
    assert_eq!(task.name, "Plans revus");
}

fn click_label(harness: &mut Harness<'static, App>, label: &str) {
    crate::click(harness, label);
}

/// Libellé d'accessibilité de la ligne `index`.
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

/// Point au centre de la cellule (ligne `index`, `column`).
fn cell_point(harness: &Harness<'static, App>, index: usize, column: Column) -> Pos2 {
    let rect = harness.get_by_label(&row_label(harness.state(), index)).rect();
    let mut x = rect.left();
    for state in harness.state().state.visible_columns() {
        let w = f32::from(state.width);
        if state.column == column {
            return pos2(x + w / 2.0, rect.center().y);
        }
        x += w;
    }
    panic!("colonne {column:?} masquée");
}

/// Harnais à pas d'image court (deux clics rapprochés = double-clic).
fn fast_harness() -> Harness<'static, App> {
    Harness::builder()
        .with_size(egui::vec2(1280.0, 720.0))
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .with_os(egui::os::OperatingSystem::Nix)
        .with_step_dt(1.0 / 60.0)
        .with_max_steps(8)
        .build_ui_state(plannercraft_ui_egui::ui, new_app())
}

/// Deux clics au point `p` dans la même image.
fn double_click(harness: &mut Harness<'static, App>, p: Pos2) {
    harness.event(Event::PointerMoved(p));
    for _ in 0..2 {
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

fn index_of(app: &App, n: u32) -> usize {
    app.rows().unwrap().index_of(uid(n)).unwrap()
}

#[test]
fn h_ui_19_shift_f2_opens() {
    let mut harness = harness();
    open_demo(&mut harness);
    harness.state_mut().dispatch(Action::Select(Some(uid(3))));
    harness.run();
    harness.key_press_modifiers(Modifiers::SHIFT, Key::F2);
    harness.run();
    let app = harness.state();
    assert_eq!(
        app.view().dialog.as_ref().map(|d| d.kind),
        Some(DialogKind::TaskInformation(uid(3)))
    );
    assert!(app.view().edit.editing.is_none(), "F2 seul n'a pas joué");
    let title = Texts::new(Language::Fr).get_args(
        TextId::TaskInfoTitle,
        &[
            ("id", Arg::Count(3)),
            ("name", Arg::Str("Plans de l'architecte".into())),
        ],
    );
    assert!(harness.query_by_label(&title).is_some());
    // Les cinq onglets sont là.
    for tab in Tab::ALL {
        assert!(crate::shown_off_ribbon(&harness, &fr(tab.title())), "{tab:?}");
    }
    // Sans tâche active : rien.
    let mut harness = crate::harness();
    open_demo(&mut harness);
    harness.state_mut().dispatch(Action::Select(None));
    harness.run();
    harness.key_press_modifiers(Modifiers::SHIFT, Key::F2);
    harness.run();
    assert!(harness.state().view().dialog.is_none());
}

#[test]
fn h_ui_19_double_click_id_opens() {
    let mut harness = fast_harness();
    open_demo(&mut harness);
    let index = index_of(harness.state(), 3);
    let p = cell_point(&harness, index, Column::Id);
    double_click(&mut harness, p);
    let app = harness.state();
    assert_eq!(
        app.view().dialog.as_ref().map(|d| d.kind),
        Some(DialogKind::TaskInformation(uid(3)))
    );
    assert!(app.view().edit.editing.is_none());
}

#[test]
fn h_ui_19_double_click_cell_edits() {
    let mut harness = fast_harness();
    open_demo(&mut harness);
    let index = index_of(harness.state(), 3);
    let p = cell_point(&harness, index, Column::Duration);
    double_click(&mut harness, p);
    let app = harness.state();
    assert!(app.view().dialog.is_none());
    let editing = app.view().edit.editing.as_ref().expect("saisie ouverte");
    assert_eq!(editing.column, Column::Duration);
}

/// « OK » avec plusieurs onglets changés : une seule entrée d'historique,
/// annulée exactement par un seul « Annuler ».
#[test]
fn task_info_ok_single_undo() {
    let mut harness = harness();
    open_demo(&mut harness);
    let project = harness.state().document.as_ref().unwrap().project().clone();
    open_info(&mut harness, 3);
    retype(&mut harness, &fr(TextId::TaskInfoName), "Plans définitifs");
    retype(&mut harness, &fr(TextId::TaskInfoDuration), "12j");
    click_label(&mut harness, &fr(TextId::TaskInfoTabNotes));
    retype(
        &mut harness,
        &fr(TextId::TaskInfoNotes),
        "Avec l'ingénieur structure",
    );
    click_label(&mut harness, &fr(TextId::TaskInfoOk));
    let app = harness.state();
    assert!(info(app).is_none(), "fenêtre fermée");
    assert!(app.messages().is_empty(), "{:?}", app.messages());
    assert_eq!(history_entries(app), 1, "un seul lot");
    let task = app
        .document
        .as_ref()
        .unwrap()
        .project()
        .tasks
        .get(uid(3))
        .unwrap();
    assert_eq!(task.name, "Plans définitifs");
    assert_eq!(task.duration.minutes().get(), 12 * 480);
    assert_eq!(task.notes, "Avec l'ingénieur structure");
    harness.state_mut().dispatch(Action::Undo);
    harness.run();
    assert_eq!(harness.state().document.as_ref().unwrap().project(), &project);
    // « OK » sans rien changer : aucun lot.
    open_info(&mut harness, 3);
    click_label(&mut harness, &fr(TextId::TaskInfoOk));
    assert!(info(harness.state()).is_none());
    assert_eq!(history_entries(harness.state()), 0);
}

/// Relecture finale J7, F8 : Entrée dans un champ valide la fenêtre comme
/// « OK » ; dans les notes (multiligne), Entrée va à la ligne et la fenêtre
/// reste ouverte.
#[test]
fn task_info_enter_validates() {
    let mut harness = harness();
    open_demo(&mut harness);
    open_info(&mut harness, 3);
    retype(&mut harness, &fr(TextId::TaskInfoDuration), "12j");
    harness.key_press(Key::Enter);
    harness.run();
    let app = harness.state();
    assert!(info(app).is_none(), "Entrée n'a pas validé la fenêtre");
    assert_eq!(history_entries(app), 1);
    let task = app
        .document
        .as_ref()
        .unwrap()
        .project()
        .tasks
        .get(uid(3))
        .unwrap();
    assert_eq!(task.duration.minutes().get(), 12 * 480);
    // Notes : Entrée reste dans la zone multiligne.
    open_info(&mut harness, 3);
    click_label(&mut harness, &fr(TextId::TaskInfoTabNotes));
    retype(&mut harness, &fr(TextId::TaskInfoNotes), "Ligne 1");
    harness.key_press(Key::Enter);
    harness.run();
    let state = info(harness.state()).expect("fenêtre gardée : Entrée dans les notes");
    assert!(
        state.draft.notes.starts_with("Ligne 1"),
        "{:?}",
        state.draft.notes
    );
    assert_eq!(history_entries(harness.state()), 1);
}

/// Un refus du registre s'affiche dans la fenêtre, sur l'onglet du champ ;
/// rien n'est appliqué ni affiché dans le bandeau.
#[test]
fn task_info_registry_refusal_in_dialog() {
    let mut harness = harness();
    open_demo(&mut harness);
    open_info(&mut harness, 3);
    // Nom de plus de 512 caractères : refusé par le registre.
    retype(&mut harness, &fr(TextId::TaskInfoName), &"x".repeat(600));
    click_label(&mut harness, &fr(TextId::TaskInfoOk));
    let app = harness.state();
    let state = info(app).expect("fenêtre gardée");
    assert_eq!(generation(app), 0);
    assert!(app.messages().is_empty(), "{:?}", app.messages());
    assert_eq!(state.errors.len(), 1, "{:?}", state.errors);
    assert_eq!(state.errors[0].tab, Tab::General);
    assert_eq!(state.errors[0].field, "name");
}

/// H-UI-105 (relecture finale ux2, B1) : un refus du registre dans
/// « Informations sur la tâche » (12 en prédécesseur de 11, déjà son
/// successeur : boucle) se dit en langage d'utilisateur, tâches nommées
/// par leur ID, sans code, UID ni numéro d'opération.
#[test]
fn h_ui_105_task_info_refusal_user_language() {
    let mut harness = harness();
    open_demo(&mut harness);
    open_info(&mut harness, 11);
    if let Some(DialogState {
        body: DialogBody::TaskInfo(state),
        ..
    }) = &mut harness.state_mut().view_mut().dialog
    {
        state.draft.links.push(LinkRow {
            id: "12".to_owned(),
            kind: LinkType::FinishToStart,
            lag: String::new(),
        });
    }
    harness.run();
    click_label(&mut harness, &fr(TextId::TaskInfoOk));
    let app = harness.state();
    let state = info(app).expect("fenêtre gardée");
    assert_eq!(generation(app), 0);
    let want =
        Texts::new(Language::Fr).get_args(TextId::RefusalLinkCycle, &[("tasks", Arg::Str("11, 12".into()))]);
    assert_eq!(state.errors.len(), 1, "{:?}", state.errors);
    assert_eq!(state.errors[0].message, want);
    assert_eq!(state.errors[0].tab, Tab::Predecessors);
}

#[test]
fn task_info_escape_closes_without_change() {
    let mut harness = harness();
    open_demo(&mut harness);
    open_info(&mut harness, 3);
    retype(&mut harness, &fr(TextId::TaskInfoName), "Jamais appliqué");
    // Premier Échap : egui retire le focus du champ ; le second ferme.
    for _ in 0..2 {
        harness.key_press(Key::Escape);
        harness.run();
        if info(harness.state()).is_none() {
            break;
        }
    }
    let app = harness.state();
    assert!(info(app).is_none(), "Échap ferme la fenêtre");
    assert_eq!(generation(app), 0);
    let task = app
        .document
        .as_ref()
        .unwrap()
        .project()
        .tasks
        .get(uid(3))
        .unwrap();
    assert_eq!(task.name, "Plans de l'architecte");
    // « Annuler » aussi.
    open_info(&mut harness, 3);
    retype(&mut harness, &fr(TextId::TaskInfoName), "Jamais appliqué");
    click_label(&mut harness, &fr(TextId::TaskInfoCancel));
    assert!(info(harness.state()).is_none());
    assert_eq!(generation(harness.state()), 0);
}

/// Fenêtre ouverte : ni la navigation, ni les commandes du document, ni la
/// frappe directe n'atteignent la table.
#[test]
fn task_info_keys_do_not_leak_to_table() {
    let mut harness = harness();
    open_demo(&mut harness);
    // La table a le focus (clic sur la ligne), puis Maj+F2.
    let index = index_of(harness.state(), 3);
    let label = row_label(harness.state(), index);
    click_label(&mut harness, &label);
    harness.key_press_modifiers(Modifiers::SHIFT, Key::F2);
    harness.run();
    assert!(info(harness.state()).is_some());
    // Aucun champ focalisé : la frappe irait à la table sans la garde
    // (Entrée valide la fenêtre : `task_info_enter_validates`).
    for key in [Key::ArrowDown, Key::Delete, Key::F2, Key::Tab] {
        harness.key_press(key);
        harness.run();
    }
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.event(Event::Text("x".to_owned()));
    harness.run();
    let app = harness.state();
    assert!(info(app).is_some(), "fenêtre toujours ouverte");
    assert_eq!(app.selection, Some(uid(3)), "le curseur n'a pas bougé");
    assert!(app.view().edit.editing.is_none(), "aucune saisie ouverte");
    assert_eq!(generation(app), 0, "aucune commande");
    assert_eq!(
        app.document.as_ref().unwrap().project().tasks.len(),
        49,
        "rien supprimé"
    );
}

/// Menu contextuel : « Informations sur la tâche… » ouvre la fenêtre de la
/// ligne cliquée.
#[test]
fn task_info_context_menu_opens() {
    let mut harness = harness();
    open_demo(&mut harness);
    let index = index_of(harness.state(), 3);
    harness
        .get_by_label(&row_label(harness.state(), index))
        .click_secondary();
    harness.run();
    harness.get_by_label_contains(&fr(TextId::TaskInfoMenu)).click();
    harness.run();
    assert_eq!(
        harness.state().view().dialog.as_ref().map(|d| d.kind),
        Some(DialogKind::TaskInformation(uid(3)))
    );
}

/// Entrée Maj+F2 de [`KEYMAP`] (appelée par `session.rs`).
pub fn check_keymap_entry(i: usize) {
    let (_, shortcut, _) = KEYMAP[i];
    let mut harness = harness();
    open_demo(&mut harness);
    harness.state_mut().dispatch(Action::Select(Some(uid(9))));
    harness.run();
    harness.key_press_modifiers(shortcut.modifiers, shortcut.logical_key);
    harness.run();
    assert_eq!(
        harness.state().view().dialog.as_ref().map(|d| d.kind),
        Some(DialogKind::TaskInformation(uid(9)))
    );
}
