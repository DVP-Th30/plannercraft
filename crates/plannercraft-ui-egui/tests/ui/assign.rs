//! « Affecter des ressources » (J7, lot L13, S70) : fenêtre non modale sur
//! la sélection (plusieurs tâches), un seul lot par bouton (Affecter,
//! Retirer, Modifier les unités), unités par défaut du registre, filtre,
//! relecture quand la sélection ou le document changent, refus affiché
//! dans la fenêtre. Exemple : UID = ID à l'ouverture.

use egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use plannercraft_automation::Batch;
use plannercraft_core::{Project, ResourceUid, TaskUid};
use plannercraft_ui_egui::action::DialogKind;
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::dialog::DialogBody;
use plannercraft_ui_egui::dialog::assign::{AssignOp, AssignResource, AssignState};
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::keymap::{Binding, KEYMAP};
use plannercraft_ui_egui::selection::SelectMode;
use plannercraft_ui_egui::{Action, App};

use crate::{fr, harness, open_demo};

/// Ressources ajoutées par [`setup`].
const MASON: u32 = 1;
const CARPENTER: u32 = 2;
const TRAINEE: u32 = 3;
const CEMENT: u32 = 4;

fn uid(n: u32) -> TaskUid {
    TaskUid::new(n)
}

fn project(app: &App) -> &Project {
    app.document.as_ref().unwrap().project()
}

fn history_entries(app: &App) -> usize {
    app.document.as_ref().unwrap().session.history().entries.len()
}

/// Applique un lot JSON comme un agent (aucun message).
fn agent(harness: &mut Harness<'static, App>, ops: &serde_json::Value) {
    let batch: Batch =
        serde_json::from_value(serde_json::json!({ "format_version": 1, "ops": ops })).unwrap();
    harness
        .state_mut()
        .execute(&batch, plannercraft_ui_egui::EditOrigin::Agent)
        .unwrap();
    harness.run();
}

/// Exemple ouvert avec quatre ressources : « Maçon » (UID 1, capacité
/// 100 %), « Charpentier » (UID 2, 50 %), « Stagiaire » (UID 3, 0 %),
/// « Ciment » (UID 4, matériel) ; Maçon affecté à 50 % à la tâche 10.
fn setup() -> Harness<'static, App> {
    let mut harness = harness();
    open_demo(&mut harness);
    agent(
        &mut harness,
        &serde_json::json!([
            { "op": "resource.add", "name": "Maçon" },
            { "op": "resource.add", "name": "Charpentier", "max_units": { "text": "50%" } },
            { "op": "resource.add", "name": "Stagiaire", "max_units": { "text": "0%" } },
            { "op": "resource.add", "name": "Ciment", "kind": "material", "material_label": "t" },
            { "op": "assignment.add", "task": 10, "resource": MASON, "units": { "text": "50%" } }
        ]),
    );
    harness
}

/// Sélectionne les tâches `uids` (la première seule, puis Ctrl+clic).
fn select(harness: &mut Harness<'static, App>, uids: &[u32]) {
    let app = harness.state_mut();
    app.dispatch(Action::Select(uids.first().map(|n| uid(*n))));
    for n in uids.iter().skip(1) {
        app.dispatch(Action::SelectWith {
            uid: uid(*n),
            mode: SelectMode::Toggle,
        });
    }
    harness.run();
}

fn open_assign(harness: &mut Harness<'static, App>) {
    harness
        .state_mut()
        .dispatch(Action::ShowDialog(DialogKind::AssignResources));
    harness.run();
}

fn state(app: &App) -> &AssignState {
    match app.view().dialog.as_ref().map(|d| &d.body) {
        Some(DialogBody::Assign(state)) => state,
        other => panic!("fenêtre d'affectation absente : {other:?}"),
    }
}

fn is_open(app: &App) -> bool {
    matches!(
        app.view().dialog.as_ref().map(|d| &d.body),
        Some(DialogBody::Assign(_))
    )
}

fn row(app: &App, resource: u32) -> &AssignResource {
    state(app)
        .rows
        .iter()
        .find(|r| r.resource == ResourceUid::new(resource))
        .unwrap()
}

/// Unités (1/10 000) de l'affectation de `resource` à `task`, s'il y en a
/// une.
fn units(app: &App, task: u32, resource: u32) -> Option<i64> {
    project(app)
        .assignments
        .iter()
        .find(|a| a.task == uid(task) && a.resource == Some(ResourceUid::new(resource)))
        .map(|a| a.units.get())
}

/// Clique sur le nom d'une ressource dans la fenêtre.
fn choose(harness: &mut Harness<'static, App>, name: &str, modifiers: Modifiers) {
    harness.get_by_label(name).click_modifiers(modifiers);
    harness.run();
}

fn click_label(harness: &mut Harness<'static, App>, label: &str) {
    crate::click(harness, label);
}

/// Remplace le texte du champ nommé `label` par `text`.
fn retype(harness: &mut Harness<'static, App>, label: &str, text: &str) {
    harness
        .get_by_role_and_label(egui::accesskit::Role::TextInput, label)
        .focus();
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.key_press(Key::Backspace);
    if !text.is_empty() {
        harness.event(Event::Text(text.to_owned()));
    }
    harness.run();
}

/// Libellé du champ des unités d'une ressource.
fn units_label(name: &str) -> String {
    Texts::new(Language::Fr).get_args(TextId::AsUnitsOf, &[("name", Arg::Str(name.to_owned().into()))])
}

/// Clique la ligne de `task` dans la table (fenêtre ouverte : non modale).
fn click_row(harness: &mut Harness<'static, App>, task: u32) {
    let app = harness.state();
    let rows = app.rows().unwrap();
    let i = rows.index_of(uid(task)).unwrap();
    let id = rows.row(i).unwrap().id;
    let label = Texts::new(Language::Fr).get_args(
        TextId::RowLabel,
        &[
            ("id", Arg::Count(i64::from(id))),
            ("name", Arg::Str(rows.text(i, Column::Name).to_owned().into())),
        ],
    );
    crate::click(harness, &label);
}

/// Plusieurs tâches, deux ressources (Ctrl+clic), unités saisies pour
/// l'une : un seul lot, une seule entrée d'annulation ; une affectation
/// déjà présente est gardée telle quelle ; la fenêtre reste ouverte et se
/// relit ; une annulation retire tout le lot.
#[test]
fn assign_multiple_tasks_single_batch() {
    let mut harness = setup();
    select(&mut harness, &[9, 10, 11]);
    open_assign(&mut harness);
    let app = harness.state();
    assert_eq!(state(app).tasks, vec![uid(9), uid(10), uid(11)]);
    assert!(
        harness
            .query_by_label(
                &Texts::new(Language::Fr).get_args(TextId::AsTaskMany, &[("count", Arg::Count(3))])
            )
            .is_some()
    );
    // Maçon : affecté à 1 tâche sur 3, unités communes de la sélection.
    assert_eq!(row(app, MASON).on.len(), 1);
    assert_eq!(row(app, MASON).read, "50%");
    assert!(harness.query_by_label("1 sur 3").is_some());
    // Rien de choisi : boutons inactifs.
    assert!(!state(app).can(AssignOp::Assign));

    choose(&mut harness, "Maçon", Modifiers::NONE);
    choose(&mut harness, "Ciment", Modifiers::COMMAND);
    assert_eq!(
        state(harness.state())
            .chosen
            .iter()
            .map(|r| r.get())
            .collect::<Vec<_>>(),
        vec![MASON, CEMENT]
    );
    retype(&mut harness, &units_label("Maçon"), "75");
    retype(&mut harness, &units_label("Ciment"), "2");
    let before = history_entries(harness.state());
    click_label(&mut harness, &fr(TextId::AsAssign));

    let app = harness.state();
    assert_eq!(history_entries(app), before + 1, "un seul lot");
    assert!(is_open(app), "la fenêtre reste ouverte");
    assert_eq!(state(app).error, None);
    assert_eq!(units(app, 9, MASON), Some(7_500));
    assert_eq!(units(app, 11, MASON), Some(7_500));
    assert_eq!(units(app, 10, MASON), Some(5_000), "affectation existante gardée");
    for task in [9, 10, 11] {
        assert!(units(app, task, CEMENT).is_some(), "Ciment sur {task}");
    }
    // Relue : affectées partout ; unités mixtes (Maçon) ⇒ vides ; quantité
    // commune (Ciment) relue sur le modèle.
    assert_eq!(row(app, MASON).on.len(), 3);
    assert_eq!(row(app, MASON).read, "");
    assert_eq!(row(app, CEMENT).read, "2");
    assert_eq!(row(app, CEMENT).units, "2");

    harness.state_mut().dispatch(Action::Undo);
    harness.run();
    let app = harness.state();
    assert_eq!(history_entries(app), before);
    assert_eq!(units(app, 9, MASON), None);
    assert_eq!(units(app, 11, MASON), None);
    assert_eq!(units(app, 10, MASON), Some(5_000));
    assert!((9..=11).all(|t| units(app, t, CEMENT).is_none()));
    assert_eq!(row(app, MASON).on.len(), 1, "fenêtre relue après l'annulation");
}

/// Unités laissées vides : le lot ne les donne pas, le registre prend sa
/// valeur par défaut (capacité, au plus 100 % ; capacité 0 : ce que rend
/// la règle), affichée telle quelle dans la fenêtre.
#[test]
fn assign_default_units_from_registry() {
    let mut harness = setup();
    // Une ressource par tâche (une seconde ressource sur une tâche pilotée
    // par l'effort répartirait le travail, R-196).
    for (task, name, resource) in [
        (9, "Maçon", MASON),
        (11, "Charpentier", CARPENTER),
        (12, "Stagiaire", TRAINEE),
    ] {
        select(&mut harness, &[task]);
        if !is_open(harness.state()) {
            open_assign(&mut harness);
        }
        choose(&mut harness, name, Modifiers::NONE);
        // Lot sans unités.
        let batch = state(harness.state())
            .to_batch(AssignOp::Assign, plannercraft_automation::text::Language::Fr)
            .unwrap();
        let json = serde_json::to_value(&batch).unwrap();
        let ops = json["ops"].as_array().unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0]["op"], "assignment.add");
        assert!(ops[0].get("units").is_none(), "{}", ops[0]);
        let before = history_entries(harness.state());
        click_label(&mut harness, &fr(TextId::AsAssign));
        assert_eq!(history_entries(harness.state()), before + 1);
        assert!(units(harness.state(), task, resource).is_some(), "{name}");
    }
    let app = harness.state();
    assert_eq!(units(app, 9, MASON), Some(10_000));
    assert_eq!(units(app, 11, CARPENTER), Some(5_000));
    // Capacité 0 : valeur rendue par le registre, affichée telle quelle.
    let trainee = units(app, 12, TRAINEE).unwrap();
    assert_eq!(row(app, TRAINEE).read, format!("{}%", trainee / 100));
    select(&mut harness, &[11]);
    assert_eq!(row(harness.state(), CARPENTER).read, "50%");
    // Valeur affichée dans le champ des unités.
    let field = harness
        .get_by_role_and_label(egui::accesskit::Role::TextInput, &units_label("Charpentier"))
        .accesskit_node()
        .value();
    assert_eq!(field.as_deref(), Some("50%"));
    select(&mut harness, &[9]);
    assert_eq!(row(harness.state(), MASON).read, "100%");
}

/// « Retirer » : toutes les affectations des ressources choisies sur la
/// sélection, en un seul lot ; rien à retirer ⇒ bouton inactif.
#[test]
fn assign_remove() {
    let mut harness = setup();
    agent(
        &mut harness,
        &serde_json::json!([{ "op": "assignment.add", "task": 9, "resource": MASON }]),
    );
    select(&mut harness, &[9, 10, 11]);
    open_assign(&mut harness);
    choose(&mut harness, "Maçon", Modifiers::NONE);
    assert!(state(harness.state()).can(AssignOp::Remove));
    let before = history_entries(harness.state());
    click_label(&mut harness, &fr(TextId::AsRemove));
    let app = harness.state();
    assert_eq!(history_entries(app), before + 1, "un seul lot");
    assert!((9..=11).all(|t| units(app, t, MASON).is_none()));
    assert!(row(app, MASON).on.is_empty());
    assert!(!state(app).can(AssignOp::Remove));
    assert!(!state(app).can(AssignOp::UpdateUnits));
    // Bouton inactif : un clic ne fait rien.
    click_label(&mut harness, &fr(TextId::AsRemove));
    assert_eq!(history_entries(harness.state()), before + 1);
}

/// « Modifier les unités » : seules les tâches où la ressource est
/// affectée ; unités relues ensuite.
#[test]
fn assign_update_units() {
    let mut harness = setup();
    select(&mut harness, &[9, 10]);
    open_assign(&mut harness);
    choose(&mut harness, "Maçon", Modifiers::NONE);
    // Texte inchangé : rien à envoyer.
    assert_eq!(
        state(harness.state()).to_batch(AssignOp::UpdateUnits, plannercraft_automation::text::Language::Fr),
        None
    );
    retype(&mut harness, &units_label("Maçon"), "25%");
    let before = history_entries(harness.state());
    click_label(&mut harness, &fr(TextId::AsUpdateUnits));
    let app = harness.state();
    assert_eq!(history_entries(app), before + 1);
    assert_eq!(units(app, 10, MASON), Some(2_500));
    assert_eq!(units(app, 9, MASON), None, "aucune affectation créée");
    assert_eq!(row(app, MASON).read, "25%");
}

/// Unités illisibles : refus du registre affiché dans la fenêtre (jamais
/// dans le bandeau), rien appliqué, fenêtre ouverte, saisie gardée.
#[test]
fn assign_refusal_shown_in_window() {
    let mut harness = setup();
    select(&mut harness, &[9]);
    open_assign(&mut harness);
    choose(&mut harness, "Maçon", Modifiers::NONE);
    retype(&mut harness, &units_label("Maçon"), "beaucoup");
    let before = history_entries(harness.state());
    let messages = harness.state().messages().len();
    click_label(&mut harness, &fr(TextId::AsAssign));
    let app = harness.state();
    assert_eq!(history_entries(app), before);
    assert_eq!(units(app, 9, MASON), None);
    assert!(is_open(app));
    let error = state(app).error.clone().expect("refus affiché");
    assert!(harness.query_by_label(&error).is_some());
    assert_eq!(app.messages().len(), messages, "pas de bandeau");
    assert_eq!(row(app, MASON).units, "beaucoup");
}

/// Filtre sans casse sur le nom ; une ressource choisie masquée n'est pas
/// visée par les boutons ; filtre vide : toutes les ressources.
#[test]
fn assign_filter() {
    let mut harness = setup();
    select(&mut harness, &[9]);
    open_assign(&mut harness);
    choose(&mut harness, "Ciment", Modifiers::NONE);
    assert!(state(harness.state()).can(AssignOp::Assign));
    retype(&mut harness, &fr(TextId::AsFilter), "MA");
    let app = harness.state();
    let visible: Vec<&str> = state(app).visible().map(|r| r.name.as_str()).collect();
    assert_eq!(visible, vec!["Maçon"]);
    assert!(harness.query_by_label("Maçon").is_some());
    assert!(harness.query_by_label("Ciment").is_none());
    assert!(harness.query_by_label("Charpentier").is_none());
    assert!(!state(app).can(AssignOp::Assign), "choix masqué non visé");
    retype(&mut harness, &fr(TextId::AsFilter), "zzz");
    assert!(harness.query_by_label(&fr(TextId::AsNoResource)).is_some());
    retype(&mut harness, &fr(TextId::AsFilter), "");
    assert_eq!(state(harness.state()).visible().count(), 4);
    assert!(state(harness.state()).can(AssignOp::Assign));
}

/// Fenêtre non modale : un clic dans la table change la sélection et la
/// fenêtre se relit (tâche, colonne Affectée, unités) ; une écriture
/// d'agent aussi ; une saisie non appliquée survit.
#[test]
fn assign_selection_change_refreshes() {
    let mut harness = setup();
    select(&mut harness, &[9]);
    open_assign(&mut harness);
    let one = |name: &str| {
        Texts::new(Language::Fr).get_args(TextId::AsTaskOne, &[("name", Arg::Str(name.to_owned().into()))])
    };
    assert!(harness.query_by_label(&one("Installation du chantier")).is_some());
    assert!(row(harness.state(), MASON).on.is_empty());
    retype(&mut harness, &units_label("Charpentier"), "30");

    click_row(&mut harness, 10);
    let app = harness.state();
    assert!(is_open(app), "fenêtre non modale, toujours ouverte");
    assert_eq!(state(app).tasks, vec![uid(10)]);
    assert!(harness.query_by_label(&one("Décapage et terrassement")).is_some());
    assert_eq!(row(app, MASON).on.len(), 1);
    assert_eq!(row(app, MASON).read, "50%");
    assert_eq!(row(app, CARPENTER).units, "30", "saisie gardée");

    agent(
        &mut harness,
        &serde_json::json!([{ "op": "assignment.remove", "task": 10, "resource": MASON }]),
    );
    let app = harness.state();
    assert!(row(app, MASON).on.is_empty(), "document relu");
    assert_eq!(row(app, MASON).read, "");

    // Aucune tâche : boutons inactifs, message.
    harness.state_mut().dispatch(Action::Select(None));
    harness.run();
    assert_eq!(state(harness.state()).tasks, Vec::<TaskUid>::new());
    assert!(harness.query_by_label(&fr(TextId::AsNoTask)).is_some());
    choose(&mut harness, "Maçon", Modifiers::NONE);
    assert!(!state(harness.state()).can(AssignOp::Assign));
}

/// Menu Édition → « Affecter des ressources… » ; « Fermer » et Échap
/// ferment sans lot ; aucun raccourci (S61, S70).
#[test]
fn assign_menu_close_and_no_shortcut() {
    let mut harness = setup();
    select(&mut harness, &[9]);
    click_label(&mut harness, &fr(TextId::MenuEdit));
    click_label(&mut harness, &fr(TextId::MenuAssignResources));
    assert!(is_open(harness.state()));
    let before = history_entries(harness.state());
    click_label(&mut harness, &fr(TextId::AsClose));
    assert!(!is_open(harness.state()));
    open_assign(&mut harness);
    harness.key_press(Key::Escape);
    harness.run();
    assert!(!is_open(harness.state()));
    assert_eq!(history_entries(harness.state()), before);
    assert!(
        KEYMAP
            .iter()
            .all(|(_, _, binding)| !matches!(binding, Binding::Command(id) if id.starts_with("assignment."))),
        "aucun raccourci d'affectation"
    );
}

/// Relecture finale J7, F9 : fenêtre ouverte, Ctrl/Cmd+Z (et les autres
/// raccourcis à Ctrl/Cmd sans effet de structure) agit hors de ses champs ;
/// Suppr reste retenu ; un double-clic sur une cellule de la table n'ouvre
/// pas une saisie qui ne recevrait aucune touche.
#[test]
fn assign_window_keeps_command_shortcuts_and_no_dead_editor() {
    let mut harness = setup();
    select(&mut harness, &[9]);
    open_assign(&mut harness);
    let entries = history_entries(harness.state());
    assert!(entries > 0, "lot de `setup` à annuler");
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run();
    assert!(is_open(harness.state()), "fenêtre fermée");
    assert_eq!(history_entries(harness.state()), entries - 1, "Ctrl/Cmd+Z perdu");
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Y);
    harness.run();
    assert_eq!(history_entries(harness.state()), entries);
    let tasks = project(harness.state()).tasks.len();
    harness.key_press(Key::Delete);
    harness.run();
    assert_eq!(
        project(harness.state()).tasks.len(),
        tasks,
        "Suppr appliqué fenêtre ouverte"
    );
    // Double-clic sur une ligne de la table : sélection, aucune saisie.
    let app = harness.state();
    let rows = app.rows().unwrap();
    let i = rows.index_of(uid(10)).unwrap();
    let label = Texts::new(Language::Fr).get_args(
        TextId::RowLabel,
        &[
            ("id", Arg::Count(i64::from(rows.row(i).unwrap().id))),
            ("name", Arg::Str(rows.text(i, Column::Name).to_owned().into())),
        ],
    );
    let p = harness.get_by_label(&label).rect().center();
    harness.event(egui::Event::PointerMoved(p));
    for _ in 0..2 {
        for pressed in [true, false] {
            harness.event(egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
        }
    }
    harness.run();
    assert_eq!(harness.state().selection, Some(uid(10)));
    assert!(
        harness.state().view().edit.editing.is_none(),
        "saisie morte ouverte"
    );
}
