//! Structure (lot L14) : Inser, Suppr (confirmation d'une récapitulative,
//! `cascade`), Alt+Maj+→/←, Ctrl+F2 et Ctrl+Maj+F2 sur la sélection,
//! sélection multiple (Ctrl+clic, Maj+clic), menu contextuel, annulation
//! exacte. Exemple : UID = ID à l'ouverture (`assets/demo/house.xml`).

use egui::Modifiers;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use plannercraft_core::{LinkType, Project, TaskUid};
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::keymap::{Binding, KEYMAP};
use plannercraft_ui_egui::rows::RowModel;
use plannercraft_ui_egui::{Action, App};

use crate::{fr, harness, open_demo};

/// Commandes de structure liées dans [`KEYMAP`].
pub const COMMANDS: [&str; 6] = [
    "task.add",
    "task.delete",
    "task.indent",
    "task.outdent",
    "link.add",
    "link.remove",
];

/// UID de l'exemple (égal à l'ID à l'ouverture).
fn uid(n: u32) -> TaskUid {
    TaskUid::new(n)
}

fn project(app: &App) -> &Project {
    app.document.as_ref().unwrap().project()
}

fn rows(app: &App) -> &RowModel {
    app.rows().unwrap()
}

fn generation(app: &App) -> u64 {
    app.document.as_ref().unwrap().generation()
}

/// Niveau hiérarchique de `uid`.
fn level(app: &App, uid: TaskUid) -> u16 {
    project(app).tasks.get(uid).unwrap().outline_level.get()
}

/// Prédécesseurs de `uid` : (UID, type).
fn preds(app: &App, uid: TaskUid) -> Vec<(TaskUid, LinkType)> {
    project(app)
        .tasks
        .get(uid)
        .unwrap()
        .predecessors
        .iter()
        .map(|link| (link.predecessor, link.kind))
        .collect()
}

/// Clique la ligne de `uid` (défilée à l'écran d'abord) avec `modifiers`.
fn click_row(harness: &mut Harness<'static, App>, uid: TaskUid, modifiers: Modifiers) {
    let i = rows(harness.state()).index_of(uid).unwrap();
    harness
        .state_mut()
        .dispatch(Action::ScrollToRow(i.saturating_sub(2)));
    harness.run();
    let app = harness.state();
    let row = rows(app).row(i).unwrap();
    let label = Texts::new(Language::Fr).get_args(
        TextId::RowLabel,
        &[
            ("id", Arg::Count(i64::from(row.id))),
            (
                "name",
                Arg::Str(rows(app).text(i, Column::Name).to_owned().into()),
            ),
        ],
    );
    harness.get_by_label(&label).click_modifiers(modifiers);
    harness.run();
}

/// Clic droit sur la ligne de `uid`.
fn right_click_row(harness: &mut Harness<'static, App>, uid: TaskUid) {
    let i = rows(harness.state()).index_of(uid).unwrap();
    let app = harness.state();
    let row = rows(app).row(i).unwrap();
    let label = Texts::new(Language::Fr).get_args(
        TextId::RowLabel,
        &[
            ("id", Arg::Count(i64::from(row.id))),
            (
                "name",
                Arg::Str(rows(app).text(i, Column::Name).to_owned().into()),
            ),
        ],
    );
    harness.get_by_label(&label).click_secondary();
    harness.run();
}

/// Appuie sur le raccourci lié à la commande `id` dans [`KEYMAP`].
fn press(harness: &mut Harness<'static, App>, id: &str) {
    delete_from_name(harness, id);
    let (_, shortcut, _) = KEYMAP
        .iter()
        .find(|(_, _, binding)| matches!(binding, Binding::Command(command) if *command == id))
        .unwrap_or_else(|| panic!("« {id} » sans raccourci"));
    harness.key_press_modifiers(shortcut.modifiers, shortcut.logical_key);
    harness.run();
}

/// Suppr supprime la tâche depuis l'ID ou le Nom (H-UI-97, lot L2 de la
/// passe UX ; ailleurs il efface la cellule) : la cellule active passe au
/// Nom de la ligne cliquée avant `task.delete`.
fn delete_from_name(harness: &mut Harness<'static, App>, id: &str) {
    if id == "task.delete" {
        let cursor = &mut harness.state_mut().view_mut().cursor;
        cursor.column = Column::Name;
        cursor.anchor_column = None;
    }
}

fn assert_no_message(app: &App) {
    assert!(app.messages().is_empty(), "{:?}", app.messages());
}

#[test]
fn ui_insert_before() {
    let mut harness = harness();
    open_demo(&mut harness);
    click_row(&mut harness, uid(3), Modifiers::NONE);
    press(&mut harness, "task.add");
    let app = harness.state();
    assert_no_message(app);
    let new = app.selection.expect("la tâche insérée est sélectionnée");
    assert_ne!(new, uid(3));
    assert_eq!(rows(app).index_of(new), Some(2), "insérée à la place de l'ID 3");
    assert_eq!(rows(app).index_of(uid(3)), Some(3));
    assert_eq!(level(app, new), level(app, uid(3)));
    assert_eq!(project(app).tasks.get(new).unwrap().name, "");
    assert_eq!(project(app).tasks.len(), 50);
    assert_eq!(app.selected().len(), 1);

    // Sans sélection : à la fin.
    harness.state_mut().dispatch(Action::Select(None));
    harness.run();
    press(&mut harness, "task.add");
    let app = harness.state();
    let last = app.selection.unwrap();
    assert_eq!(rows(app).index_of(last), Some(50));
    assert_eq!(level(app, last), 1);
}

#[test]
fn ui_delete_summary_confirms_cascade() {
    let mut harness = harness();
    open_demo(&mut harness);
    // « Couverture » (23) : récapitulative de 24, 25, 26.
    click_row(&mut harness, uid(23), Modifiers::NONE);
    press(&mut harness, "task.delete");
    assert_eq!(generation(harness.state()), 0, "rien avant la confirmation");
    assert_eq!(harness.state().pending_delete().unwrap().count, 4);
    assert!(harness.query_by_label(&fr(TextId::ConfirmDeleteTitle)).is_some());
    // Annuler : rien ne change.
    crate::click(&mut harness, &fr(TextId::ConfirmDeleteNo));
    assert!(harness.state().pending_delete().is_none());
    assert_eq!(generation(harness.state()), 0);
    assert!(harness.query_by_label(&fr(TextId::ConfirmDeleteTitle)).is_none());

    // Confirmer : le bloc part, `cascade` ; la tâche qui suit le bloc prend
    // sa ligne et devient active (relecture finale J7, F4).
    let at = rows(harness.state()).index_of(uid(23)).unwrap();
    let after = rows(harness.state()).row(at + 4).unwrap().uid;
    press(&mut harness, "task.delete");
    crate::click(&mut harness, &fr(TextId::ConfirmDeleteYes));
    let app = harness.state();
    assert_no_message(app);
    assert_eq!(generation(app), 1);
    for n in 23..=26 {
        assert!(!project(app).tasks.contains(uid(n)), "{n}");
    }
    assert_eq!(project(app).tasks.len(), 45);
    assert_eq!(app.selection, Some(after));
    assert_eq!(rows(app).index_of(after), Some(at));
    let history = app.document.as_ref().unwrap().session.history();
    assert_eq!(history.entries.len(), 1);

    // Une feuille : supprimée sans confirmation.
    click_row(&mut harness, uid(27), Modifiers::NONE);
    press(&mut harness, "task.delete");
    let app = harness.state();
    assert!(app.pending_delete().is_none());
    assert!(!project(app).tasks.contains(uid(27)));
    assert_eq!(generation(app), 2);

    // La dernière tâche supprimée : la précédente devient active, pas la
    // ligne vide.
    let len = rows(harness.state()).len();
    let last = rows(harness.state()).row(len - 1).unwrap().uid;
    let before_last = rows(harness.state()).row(len - 2).unwrap().uid;
    click_row(&mut harness, last, Modifiers::NONE);
    press(&mut harness, "task.delete");
    let app = harness.state();
    assert!(app.pending_delete().is_none(), "dernière ligne : une feuille");
    assert_eq!(app.selection, Some(before_last));
}

/// Une demande de confirmation devenue caduque (document modifié entre-temps)
/// n'est pas appliquée.
#[test]
fn ui_delete_confirmation_stale_is_dropped() {
    let mut harness = harness();
    open_demo(&mut harness);
    click_row(&mut harness, uid(23), Modifiers::NONE);
    press(&mut harness, "task.delete");
    assert!(harness.state().pending_delete().is_some());
    // Un lot appliqué entre-temps (ici par un agent) change la génération.
    let rename = plannercraft_automation::Batch::from_json_str(
        r#"{"format_version":1,"ops":[{"op":"task.update","uid":2,"name":"X"}]}"#,
    )
    .unwrap();
    harness
        .state_mut()
        .execute(&rename, plannercraft_ui_egui::EditOrigin::Agent)
        .unwrap();
    harness.state_mut().answer_delete(true);
    harness.run();
    let app = harness.state();
    assert!(project(app).tasks.contains(uid(23)));
    assert_eq!(generation(app), 1);

    // Document fermé pendant la confirmation : la fenêtre disparaît.
    click_row(&mut harness, uid(23), Modifiers::NONE);
    press(&mut harness, "task.delete");
    assert!(harness.state().pending_delete().is_some());
    harness.state_mut().dispatch(Action::CloseDocument);
    // Document modifié (lot L13) : la fermeture passe par « Ne pas enregistrer ».
    harness.state_mut().dispatch(Action::Confirm(
        plannercraft_ui_egui::action::ConfirmChoice::Discard,
    ));
    harness.run();
    assert!(harness.state().document.is_none());
    assert!(harness.state().pending_delete().is_none());
    assert!(harness.query_by_label(&fr(TextId::ConfirmDeleteTitle)).is_none());
}

#[test]
fn ui_indent_outdent_shortcuts() {
    let mut harness = harness();
    open_demo(&mut harness);
    // Plage 32..33 (Maj+clic) sous « Plomberie » (31).
    click_row(&mut harness, uid(32), Modifiers::NONE);
    click_row(&mut harness, uid(33), Modifiers::SHIFT);
    assert_eq!(harness.state().selected().len(), 2);
    press(&mut harness, "task.indent");
    let app = harness.state();
    assert_no_message(app);
    assert_eq!((level(app, uid(32)), level(app, uid(33))), (3, 3));
    assert_eq!(level(app, uid(31)), 2);
    assert_eq!(generation(app), 1, "un seul lot");
    press(&mut harness, "task.outdent");
    let app = harness.state();
    assert_no_message(app);
    assert_eq!((level(app, uid(32)), level(app, uid(33))), (2, 2));
    assert_eq!(generation(app), 2);

    // Récapitulative et enfant sélectionnés : le bloc ne bouge que d'un
    // niveau. « Extérieur » (42) sous « Intérieur » (37).
    click_row(&mut harness, uid(42), Modifiers::NONE);
    click_row(&mut harness, uid(43), Modifiers::COMMAND);
    press(&mut harness, "task.indent");
    let app = harness.state();
    assert_no_message(app);
    assert_eq!((level(app, uid(42)), level(app, uid(43))), (3, 4));

    // Refus du registre (aucune tâche au-dessus) : bandeau, rien ne change.
    click_row(&mut harness, uid(1), Modifiers::NONE);
    let before = generation(harness.state());
    press(&mut harness, "task.indent");
    let app = harness.state();
    assert_eq!(generation(app), before);
    assert_eq!(app.messages().len(), 1, "{:?}", app.messages());
    assert_eq!(app.messages()[0].title, Some(TextId::CommandErrorTitle));
}

#[test]
fn ui_link_selection_fs_chain() {
    let mut harness = harness();
    open_demo(&mut harness);
    let original_20 = preds(harness.state(), uid(20));
    let original_44 = preds(harness.state(), uid(44));
    // Ctrl+clic : ordre des clics 14 → 20 → 44.
    click_row(&mut harness, uid(14), Modifiers::NONE);
    click_row(&mut harness, uid(20), Modifiers::COMMAND);
    click_row(&mut harness, uid(44), Modifiers::COMMAND);
    assert_eq!(
        harness.state().selected().click_order(),
        [uid(14), uid(20), uid(44)]
    );
    press(&mut harness, "link.add");
    let app = harness.state();
    assert_no_message(app);
    assert_eq!(generation(app), 1);
    assert!(preds(app, uid(20)).contains(&(uid(14), LinkType::FinishToStart)));
    assert!(preds(app, uid(44)).contains(&(uid(20), LinkType::FinishToStart)));

    // Déjà liées : non-changement (R-260 b), aucun lot ni message.
    press(&mut harness, "link.add");
    let app = harness.state();
    assert_eq!(generation(app), 1);
    assert_no_message(app);

    // Délier : seuls les liens entre tâches sélectionnées partent.
    press(&mut harness, "link.remove");
    let app = harness.state();
    assert_eq!(generation(app), 2);
    assert_eq!(preds(app, uid(20)), original_20);
    assert_eq!(preds(app, uid(44)), original_44);

    // Plage (Maj+clic de bas en haut) : ordre des ID, 13 → 14.
    click_row(&mut harness, uid(14), Modifiers::NONE);
    click_row(&mut harness, uid(13), Modifiers::SHIFT);
    press(&mut harness, "link.add");
    let app = harness.state();
    assert_no_message(app);
    assert!(preds(app, uid(14)).contains(&(uid(13), LinkType::FinishToStart)));
    assert!(!preds(app, uid(13)).iter().any(|(p, _)| *p == uid(14)));

    // Cycle (20 → 14 alors que 14 précède 20) : refusé.
    let before = generation(harness.state());
    click_row(&mut harness, uid(20), Modifiers::NONE);
    click_row(&mut harness, uid(14), Modifiers::COMMAND);
    press(&mut harness, "link.add");
    let app = harness.state();
    assert_eq!(generation(app), before);
    assert!(!preds(app, uid(14)).iter().any(|(p, _)| *p == uid(20)));
    assert_eq!(app.messages().len(), 1, "{:?}", app.messages());
}

#[test]
fn ui_structure_undo_exact() {
    let mut harness = harness();
    open_demo(&mut harness);
    let original = project(harness.state()).clone();
    let original_rows = rows(harness.state()).clone();
    let mut states = vec![(original.clone(), original_rows.clone())];
    let snapshot =
        |harness: &Harness<'static, App>| (project(harness.state()).clone(), rows(harness.state()).clone());

    click_row(&mut harness, uid(3), Modifiers::NONE);
    press(&mut harness, "task.add");
    states.push(snapshot(&harness));
    click_row(&mut harness, uid(32), Modifiers::NONE);
    press(&mut harness, "task.indent");
    states.push(snapshot(&harness));
    click_row(&mut harness, uid(14), Modifiers::NONE);
    click_row(&mut harness, uid(20), Modifiers::COMMAND);
    press(&mut harness, "link.add");
    states.push(snapshot(&harness));
    click_row(&mut harness, uid(37), Modifiers::NONE);
    press(&mut harness, "task.delete");
    crate::click(&mut harness, &fr(TextId::ConfirmDeleteYes));
    assert_no_message(harness.state());
    assert_eq!(generation(harness.state()), 4);

    // Chaque annulation rend exactement l'état d'avant le lot.
    while let Some(expected) = states.pop() {
        harness.state_mut().dispatch(Action::Undo);
        harness.run();
        let got = snapshot(&harness);
        assert_eq!(got.0, expected.0, "projet différent après annulation");
        assert_eq!(got.1, expected.1);
    }
    assert_eq!(project(harness.state()), &original);
    assert!(!harness.state().window_title().starts_with('*'));
}

#[test]
fn ui_context_menu_runs_structure() {
    let mut harness = harness();
    open_demo(&mut harness);
    click_row(&mut harness, uid(32), Modifiers::NONE);
    // Clic droit sur une autre ligne : elle devient la sélection.
    right_click_row(&mut harness, uid(34));
    assert_eq!(harness.state().selection, Some(uid(34)));
    harness
        .get_by_label_contains(&fr(TextId::StructureIndent))
        .click();
    harness.run();
    let app = harness.state();
    assert_no_message(app);
    assert_eq!(level(app, uid(34)), 3);
    assert_eq!(level(app, uid(32)), 2);
    assert!(
        harness
            .query_by_label_contains(&fr(TextId::StructureIndent))
            .is_none(),
        "menu refermé"
    );

    // Clic droit dans la sélection multiple : elle est gardée.
    click_row(&mut harness, uid(13), Modifiers::NONE);
    click_row(&mut harness, uid(14), Modifiers::SHIFT);
    right_click_row(&mut harness, uid(13));
    assert_eq!(harness.state().selected().len(), 2);
    harness.get_by_label_contains(&fr(TextId::StructureLink)).click();
    harness.run();
    assert!(preds(harness.state(), uid(14)).contains(&(uid(13), LinkType::FinishToStart)));
}

/// Vérification d'une entrée de [`KEYMAP`] liée à une commande de structure
/// (appelée par `session.rs`, un test par entrée).
pub fn check_keymap_entry(id: &str, shortcut: egui::KeyboardShortcut) {
    let press = |harness: &mut Harness<'static, App>, id: &str| {
        delete_from_name(harness, id);
        harness.key_press_modifiers(shortcut.modifiers, shortcut.logical_key);
        harness.run();
    };
    let mut harness = harness();
    open_demo(&mut harness);
    match id {
        "task.add" => {
            click_row(&mut harness, uid(3), Modifiers::NONE);
            press(&mut harness, id);
            assert_eq!(project(harness.state()).tasks.len(), 50);
        }
        "task.delete" => {
            click_row(&mut harness, uid(3), Modifiers::NONE);
            press(&mut harness, id);
            assert!(!project(harness.state()).tasks.contains(uid(3)));
            // Suppr : message « 1 tâche supprimée — Ctrl/Cmd+Z annule »
            // (H-UI-97, lot L2 de la passe UX) ; Cmd+Retour arrière : aucun.
            if shortcut.logical_key == egui::Key::Delete {
                assert_eq!(harness.state().messages().len(), 1);
                harness.state_mut().dispatch(Action::DismissMessage);
                harness.run();
            }
        }
        "task.indent" => {
            click_row(&mut harness, uid(32), Modifiers::NONE);
            press(&mut harness, id);
            assert_eq!(level(harness.state(), uid(32)), 3);
        }
        "task.outdent" => {
            click_row(&mut harness, uid(49), Modifiers::NONE);
            press(&mut harness, id);
            assert_eq!(level(harness.state(), uid(49)), 1);
        }
        "link.add" => {
            click_row(&mut harness, uid(13), Modifiers::NONE);
            click_row(&mut harness, uid(14), Modifiers::COMMAND);
            press(&mut harness, id);
            assert!(preds(harness.state(), uid(14)).contains(&(uid(13), LinkType::FinishToStart)));
        }
        "link.remove" => {
            click_row(&mut harness, uid(2), Modifiers::NONE);
            click_row(&mut harness, uid(3), Modifiers::SHIFT);
            press(&mut harness, id);
            assert_eq!(preds(harness.state(), uid(3)), []);
        }
        other => panic!("« {other} » n'est pas une commande de structure"),
    }
    assert_no_message(harness.state());
}

/// Ouvre le menu Édition et clique l'entrée dont le libellé commence par
/// `item` (le raccourci affiché suit le libellé).
fn click_edit_menu(harness: &mut Harness<'static, App>, item: TextId) {
    crate::click(harness, &fr(TextId::MenuEdit));
    let label = fr(item);
    harness
        .get_by(|node| node.label().is_some_and(|text| text.starts_with(&label)))
        .click();
    harness.run();
}

/// Relecture finale J4, F5 : les six commandes de structure sont dans le
/// menu Édition, grisées sans sélection (sauf l'insertion), et y
/// déclenchent leur commande.
#[test]
fn ui_edit_menu_structure_entries() {
    let mut harness = harness();
    open_demo(&mut harness);
    // L'ouverture sélectionne la première ligne (J7 L1) : sans sélection.
    harness.state_mut().dispatch(Action::Select(None));
    harness.run();
    crate::click(&mut harness, &fr(TextId::MenuEdit));
    for (text, enabled) in [
        (TextId::StructureInsert, true),
        (TextId::StructureDelete, false),
        (TextId::StructureIndent, false),
        (TextId::StructureOutdent, false),
        (TextId::StructureLink, false),
        (TextId::StructureUnlink, false),
    ] {
        let label = fr(text);
        let node = harness.get_by(|node| node.label().is_some_and(|l| l.starts_with(&label)));
        assert_eq!(!node.accesskit_node().is_disabled(), enabled, "{label}");
    }
    harness.key_press(egui::Key::Escape);
    harness.run();
    click_row(&mut harness, uid(3), Modifiers::NONE);
    click_edit_menu(&mut harness, TextId::StructureDelete);
    assert!(!project(harness.state()).tasks.contains(uid(3)));
    assert_no_message(harness.state());
}

/// Relecture finale J4, F5 : l'accueil propose « Nouveau projet » (Ctrl+N
/// est pris par le navigateur sur le web).
#[test]
fn ui_welcome_new_project_button() {
    let mut harness = harness();
    harness.run();
    crate::click(&mut harness, &fr(TextId::WelcomeNewProject));
    let doc = harness.state().document.as_ref().expect("projet créé");
    assert_eq!(doc.project().tasks.len(), 0);
}
