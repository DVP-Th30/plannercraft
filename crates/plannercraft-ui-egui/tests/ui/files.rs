//! Fichiers (lot L13) : Nouveau, Enregistrer, Enregistrer sous, document
//! modifié. Le service de fichiers est simulé ([`SimFiles`]) : il garde les
//! demandes d'enregistrement (octets et destination) et répond selon le
//! script, sans toucher au disque.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::Sender;

use egui::{Key, Modifiers, ViewportCommand, ViewportEvent, ViewportId};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_automation::Batch;
use plannercraft_core::{Moment, TaskUid};
use plannercraft_mspdi::write::WriteOptions;
use plannercraft_ui_egui::document;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::io::{OpenEvent, SaveEvent, SaveRequest, SaveTarget};
use plannercraft_ui_egui::rows::RowKind;
use plannercraft_ui_egui::{Action, App, Env, FileService, LanguagePref, ThemePref, UiState};

use crate::{fr, fr_light, harness_for, now, open_demo};

/// Réponse du service d'enregistrement simulé.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveReply {
    /// Écriture réussie (boîte : `/projets/<nom proposé>`).
    Ok,
    /// Écriture impossible.
    Fail,
    /// Boîte refermée sans choisir.
    Cancel,
    /// Aucune réponse : l'émetteur est gardé (enregistrement lent).
    Hold,
}

/// Demandes reçues par le service simulé.
pub type Requests = Rc<RefCell<Vec<SaveRequest>>>;

/// Service de fichiers simulé : n'ouvre rien, enregistre selon `reply`.
struct SimFiles {
    reply: Rc<Cell<SaveReply>>,
    requests: Requests,
    held: Rc<RefCell<Vec<Sender<SaveEvent>>>>,
}

impl FileService for SimFiles {
    fn pick_and_open(&mut self, ctx: &egui::Context, _: Moment, _: String, _: String, tx: Sender<OpenEvent>) {
        tx.send(OpenEvent::Cancelled).unwrap();
        ctx.request_repaint();
    }

    fn save(&mut self, ctx: &egui::Context, request: SaveRequest, tx: Sender<SaveEvent>) {
        let event = match self.reply.get() {
            SaveReply::Ok => Some(saved_event(&request.target)),
            SaveReply::Fail => Some(SaveEvent::Failed("disque plein".to_owned())),
            SaveReply::Cancel => Some(SaveEvent::Cancelled),
            SaveReply::Hold => None,
        };
        self.requests.borrow_mut().push(request);
        match event {
            Some(event) => tx.send(event).unwrap(),
            None => self.held.borrow_mut().push(tx),
        }
        ctx.request_repaint();
    }
}

/// Succès simulé pour une destination.
fn saved_event(target: &SaveTarget) -> SaveEvent {
    let path = match target {
        SaveTarget::Path(path) => path.clone(),
        SaveTarget::Dialog { suggested_name, .. } => format!("/projets/{suggested_name}"),
    };
    let name = path.rsplit('/').next().unwrap().to_owned();
    SaveEvent::Saved {
        name,
        path: Some(path),
    }
}

/// Harnais simulé : réponse modifiable, demandes et émetteurs gardés.
pub struct Sim {
    pub harness: Harness<'static, App>,
    pub reply: Rc<Cell<SaveReply>>,
    pub requests: Requests,
    held: Rc<RefCell<Vec<Sender<SaveEvent>>>>,
}

/// Harnais dont le service d'enregistrement répond `reply`.
pub fn sim(reply: SaveReply) -> Sim {
    sim_with(fr_light(), reply)
}

/// Comme [`sim`], avec des préférences données.
fn sim_with(state: UiState, reply: SaveReply) -> Sim {
    let reply = Rc::new(Cell::new(reply));
    let requests: Requests = Rc::new(RefCell::new(Vec::new()));
    let held = Rc::new(RefCell::new(Vec::new()));
    let files = SimFiles {
        reply: Rc::clone(&reply),
        requests: Rc::clone(&requests),
        held: Rc::clone(&held),
    };
    let env = Env {
        files: Box::new(files),
        ..Env::fixed(now())
    };
    Sim {
        harness: harness_for(App::new(state, env)),
        reply,
        requests,
        held,
    }
}

/// Change la durée de la première feuille (document modifié) ; rend son
/// UID.
pub fn edit_first_leaf(harness: &mut Harness<'static, App>, text: &str) -> TaskUid {
    let rows = harness.state().rows().unwrap();
    let uid = (0..rows.len())
        .find_map(|i| {
            rows.row(i)
                .filter(|row| row.kind == RowKind::Leaf)
                .map(|row| row.uid)
        })
        .unwrap();
    let json = format!(
        r#"{{"format_version":1,"ops":[{{"op":"task.update","uid":{},"duration":{{"text":"{text}"}}}}]}}"#,
        uid.get()
    );
    harness
        .state_mut()
        .dispatch(Action::Exec(Batch::from_json_str(&json).unwrap()));
    harness.run();
    assert!(harness.state().document.as_ref().unwrap().is_dirty());
    uid
}

/// Ctrl/Cmd + S.
fn press_save(harness: &mut Harness<'static, App>) {
    harness.key_press_modifiers(Modifiers::COMMAND, Key::S);
    harness.run();
}

/// Ouvre le menu Fichier et clique l'entrée dont le libellé commence par
/// `item` (le raccourci affiché suit le libellé), sans confondre
/// « Enregistrer » et « Enregistrer sous… ».
fn file_menu(harness: &mut Harness<'static, App>, item: TextId) {
    crate::click(harness, &fr(TextId::MenuFile));
    let label = fr(item);
    let save_as = fr(TextId::MenuSaveAs);
    // « Nouveau projet » de l'accueil commence aussi par « Nouveau ».
    let welcome_new = fr(TextId::WelcomeNewProject);
    crate::click_where(harness, |text| {
        text.starts_with(&label)
            && (item == TextId::MenuSaveAs || !text.starts_with(&save_as))
            && text != welcome_new
    });
}

/// Titre du bandeau de confirmation pour le document `name`.
fn confirm_title(name: &str) -> String {
    Texts::new(Language::Fr).get_args(TextId::ConfirmUnsavedTitle, &[("name", Arg::Str(name.into()))])
}

/// Commandes de fenêtre émises par la dernière image.
pub fn viewport_commands(harness: &Harness<'static, App>) -> Vec<ViewportCommand> {
    harness
        .output()
        .viewport_output
        .get(&ViewportId::ROOT)
        .map(|output| output.commands.clone())
        .unwrap_or_default()
}

#[test]
fn ui_save_bytes_reread_equal() {
    let mut s = sim(SaveReply::Ok);
    open_demo(&mut s.harness);
    let uid = edit_first_leaf(&mut s.harness, "17j");
    press_save(&mut s.harness);
    let requests = s.requests.borrow();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    // L'exemple n'a pas de fichier : boîte de dialogue, nom assaini.
    let SaveTarget::Dialog { suggested_name, .. } = &request.target else {
        panic!("boîte attendue : {:?}", request.target);
    };
    let extension = std::path::Path::new(suggested_name).extension();
    assert!(extension.is_some_and(|e| e == "xml"), "{suggested_name}");
    assert!(!suggested_name.contains(':'), "{suggested_name}");
    let app = s.harness.state();
    let doc = app.document.as_ref().unwrap();
    // Octets écrits = octets de la session.
    assert_eq!(
        request.bytes,
        doc.session.save_bytes(&WriteOptions::default()).xml
    );
    // Relus : même planning, même durée modifiée, et réécriture identique.
    let reread = document::load(&request.bytes, now()).unwrap();
    assert_eq!(
        reread.session.save_bytes(&WriteOptions::default()).xml,
        request.bytes
    );
    assert_eq!(reread.session.project().tasks.len(), doc.project().tasks.len());
    assert_eq!(
        reread.session.schedule().project_finish,
        doc.schedule().project_finish
    );
    let duration = |project: &plannercraft_core::Project| project.tasks.get(uid).unwrap().duration;
    assert_eq!(duration(reread.session.project()), duration(doc.project()));
    assert!(!doc.is_dirty());
}

#[test]
fn ui_dirty_star_and_clear_after_save() {
    let mut s = sim(SaveReply::Ok);
    open_demo(&mut s.harness);
    assert!(!s.harness.state().window_title().starts_with('*'));
    edit_first_leaf(&mut s.harness, "17j");
    assert!(s.harness.state().window_title().starts_with('*'));
    // Menu Fichier → Enregistrer : boîte (pas de chemin), puis étoile retirée.
    file_menu(&mut s.harness, TextId::MenuSave);
    {
        let app = s.harness.state();
        let doc = app.document.as_ref().unwrap();
        assert!(!doc.is_dirty());
        assert!(!app.window_title().starts_with('*'), "{}", app.window_title());
        let path = doc.origin.path.clone().unwrap();
        assert!(path.starts_with("/projets/"), "{path}");
        assert!(app.window_title().starts_with(&doc.origin.name));
        assert!(!doc.origin.demo);
        assert_eq!(doc.session.origin().path.as_deref(), Some(path.as_str()));
    }
    // Nouvelle modification : étoile ; Ctrl+S écrit sur le chemin connu.
    edit_first_leaf(&mut s.harness, "18j");
    assert!(s.harness.state().window_title().starts_with('*'));
    press_save(&mut s.harness);
    let path = s.harness.state().document.as_ref().unwrap().origin.path.clone();
    assert_eq!(
        s.requests.borrow().last().unwrap().target,
        SaveTarget::Path(path.unwrap())
    );
    assert!(!s.harness.state().window_title().starts_with('*'));
    // Enregistrer sous : toujours une boîte, même avec un chemin connu.
    file_menu(&mut s.harness, TextId::MenuSaveAs);
    assert!(matches!(
        s.requests.borrow().last().unwrap().target,
        SaveTarget::Dialog { .. }
    ));
    assert_eq!(s.requests.borrow().len(), 3);

    // Échec : le document reste modifié, l'erreur s'affiche.
    edit_first_leaf(&mut s.harness, "19j");
    s.reply.set(SaveReply::Fail);
    press_save(&mut s.harness);
    let app = s.harness.state();
    assert!(app.document.as_ref().unwrap().is_dirty());
    assert!(app.window_title().starts_with('*'));
    assert!(
        app.messages()
            .iter()
            .any(|m| m.title == Some(TextId::SaveErrorTitle) && m.text.contains("disque plein")),
        "{:?}",
        app.messages()
    );
    // Annulation : rien ne change.
    s.reply.set(SaveReply::Cancel);
    press_save(&mut s.harness);
    assert!(s.harness.state().document.as_ref().unwrap().is_dirty());
    // Un succès retire le message d'erreur.
    s.reply.set(SaveReply::Ok);
    press_save(&mut s.harness);
    let app = s.harness.state();
    assert!(!app.document.as_ref().unwrap().is_dirty());
    assert!(
        app.messages()
            .iter()
            .all(|m| m.title != Some(TextId::SaveErrorTitle))
    );
}

/// Enregistrement lent : une modification faite pendant l'écriture n'est
/// pas marquée enregistrée ; un second enregistrement est refusé tant que
/// le premier n'a pas répondu (desktop).
#[test]
fn ui_save_during_edit_stays_dirty() {
    let mut s = sim(SaveReply::Hold);
    open_demo(&mut s.harness);
    edit_first_leaf(&mut s.harness, "17j");
    press_save(&mut s.harness);
    assert!(s.harness.state().is_saving());
    press_save(&mut s.harness);
    assert_eq!(s.requests.borrow().len(), 1, "second enregistrement refusé");
    let busy = fr(TextId::SaveInProgress);
    assert!(s.harness.state().messages().iter().any(|m| m.text == busy));
    edit_first_leaf(&mut s.harness, "18j");
    let target = s.requests.borrow()[0].target.clone();
    let tx = s.held.borrow_mut().pop().unwrap();
    tx.send(saved_event(&target)).unwrap();
    s.harness.run();
    let app = s.harness.state();
    assert!(!app.is_saving());
    let doc = app.document.as_ref().unwrap();
    assert!(doc.is_dirty(), "l'état écrit n'est plus l'état courant");
    assert!(doc.origin.path.is_some(), "le fichier existe désormais");
}

#[test]
fn ui_close_dirty_asks_confirmation() {
    let mut s = sim(SaveReply::Ok);
    let h = &mut s.harness;
    open_demo(h);
    // Document intact : fermeture immédiate.
    h.state_mut().dispatch(Action::CloseDocument);
    h.run();
    assert!(h.state().document.is_none());
    assert!(!h.state().confirmation_pending());

    // Modifié : bandeau, document gardé ; « Annuler » ne perd rien.
    open_demo(h);
    edit_first_leaf(h, "17j");
    let name = h.state().document.as_ref().unwrap().origin.name.clone();
    h.state_mut().dispatch(Action::CloseDocument);
    h.run();
    assert!(h.state().document.is_some());
    assert!(h.state().confirmation_pending());
    assert!(h.query_by_label(&confirm_title(&name)).is_some());
    crate::click(h, &fr(TextId::ConfirmCancel));
    assert!(!h.state().confirmation_pending());
    assert!(h.state().document.as_ref().unwrap().is_dirty());
    assert!(h.query_by_label(&confirm_title(&name)).is_none());

    // « Ne pas enregistrer » : fermé sans écrire.
    h.state_mut().dispatch(Action::CloseDocument);
    h.run();
    crate::click(h, &fr(TextId::ConfirmDiscard));
    assert!(h.state().document.is_none());
    assert!(s.requests.borrow().is_empty());

    // « Enregistrer » : écrit, puis ferme.
    let h = &mut s.harness;
    open_demo(h);
    edit_first_leaf(h, "17j");
    h.state_mut().dispatch(Action::CloseDocument);
    h.run();
    crate::click(h, &fr(TextId::ConfirmSave));
    assert!(h.state().document.is_none());
    assert_eq!(s.requests.borrow().len(), 1);

    // « Enregistrer » annulé : le document reste ouvert et modifié.
    let h = &mut s.harness;
    open_demo(h);
    edit_first_leaf(h, "17j");
    s.reply.set(SaveReply::Cancel);
    h.state_mut().dispatch(Action::CloseDocument);
    h.run();
    crate::click(h, &fr(TextId::ConfirmSave));
    assert!(h.state().document.as_ref().unwrap().is_dirty());
    assert!(!h.state().confirmation_pending());

    // Ouvrir l'exemple ou créer un projet demande aussi confirmation.
    for action in [Action::OpenDemo, Action::NewProject] {
        h.state_mut().dispatch(action);
        h.run();
        assert!(h.state().confirmation_pending());
        assert!(h.state().document.as_ref().unwrap().is_dirty());
        h.state_mut().dispatch(Action::Confirm(
            plannercraft_ui_egui::action::ConfirmChoice::Cancel,
        ));
        h.run();
    }

    // Fermeture de la fenêtre (desktop) : annulée jusqu'à la réponse.
    h.input_mut()
        .viewports
        .entry(ViewportId::ROOT)
        .or_default()
        .events
        .push(ViewportEvent::Close);
    h.step();
    assert!(viewport_commands(h).contains(&ViewportCommand::CancelClose));
    assert!(h.state().confirmation_pending());
    h.state_mut().dispatch(Action::Confirm(
        plannercraft_ui_egui::action::ConfirmChoice::Discard,
    ));
    h.step();
    assert!(viewport_commands(h).contains(&ViewportCommand::Close));
    // La fermeture suivante n'est plus annulée.
    h.input_mut()
        .viewports
        .entry(ViewportId::ROOT)
        .or_default()
        .events
        .push(ViewportEvent::Close);
    h.step();
    assert!(!viewport_commands(h).contains(&ViewportCommand::CancelClose));
}

/// Un document intact se ferme sans bandeau, fenêtre comprise.
#[test]
fn ui_close_clean_window_not_cancelled() {
    let mut s = sim(SaveReply::Ok);
    open_demo(&mut s.harness);
    s.harness
        .input_mut()
        .viewports
        .entry(ViewportId::ROOT)
        .or_default()
        .events
        .push(ViewportEvent::Close);
    s.harness.step();
    assert!(!viewport_commands(&s.harness).contains(&ViewportCommand::CancelClose));
    assert!(!s.harness.state().confirmation_pending());
}

#[test]
fn ui_new_project_empty_with_blank_row() {
    let mut s = sim(SaveReply::Ok);
    let h = &mut s.harness;
    file_menu(h, TextId::MenuNew);
    let app = h.state();
    let doc = app.document.as_ref().expect("projet créé");
    assert_eq!(doc.project().tasks.len(), 0);
    assert_eq!(doc.origin.name, "Projet1");
    assert!(doc.origin.path.is_none());
    assert!(!doc.is_dirty());
    assert!(!app.window_title().starts_with('*'));
    // `now` = vendredi 15 janvier 2027 : début le lundi 18, 08:00.
    let monday: Moment = "2027-01-18T08:00".parse().unwrap();
    assert_eq!(doc.schedule().project_start, monday);
    assert!(app.messages().is_empty(), "{:?}", app.messages());
    // Vue principale (barre d'état : nom, zéro tâche), plus d'accueil.
    assert!(h.query_by_label(&fr(TextId::WelcomeOpenDemo)).is_none());
    assert!(h.query_by_label("Projet1").is_some());
    let none = Texts::new(Language::Fr).get_args(TextId::StatusTasks, &[("count", Arg::Count(0))]);
    assert!(h.query_by_label(&none).is_some(), "{none}");
    assert_eq!(app.rows().map(plannercraft_ui_egui::rows::RowModel::len), Some(0));
    // Enregistrer un projet neuf : boîte, nom proposé « Projet1.xml ».
    press_save(h);
    let requests = s.requests.borrow();
    let SaveTarget::Dialog { suggested_name, .. } = &requests[0].target else {
        panic!("boîte attendue");
    };
    assert_eq!(suggested_name, "Projet1.xml");
    let reread = document::load(&requests[0].bytes, now()).unwrap();
    assert_eq!(reread.session.project().tasks.len(), 0);
}

/// Nom du projet neuf traduit.
#[test]
fn ui_new_project_name_translated() {
    let state = UiState {
        language: LanguagePref::En,
        theme: ThemePref::Light,
        ..UiState::default()
    };
    let mut s = sim_with(state, SaveReply::Ok);
    s.harness.state_mut().dispatch(Action::NewProject);
    s.harness.run();
    let doc = s.harness.state().document.as_ref().unwrap();
    assert_eq!(doc.origin.name, "Project1");
}

/// Appuie sur `shortcut` dans **une seule** image (événement posé
/// directement dans l'entrée : `key_press_modifiers` étale ses événements
/// sur plusieurs images, et seule la sortie de la dernière est lisible).
fn press_in_one_frame(h: &mut Harness<'static, App>, shortcut: egui::KeyboardShortcut) {
    h.input_mut().events.push(egui::Event::Key {
        key: shortcut.logical_key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: shortcut.modifiers,
    });
    h.step();
}

/// Demande de fermeture de la fenêtre (bouton rouge) dans une image : rend
/// vrai si elle est annulée (`CancelClose`).
fn close_window_cancelled(h: &mut Harness<'static, App>) -> bool {
    h.input_mut()
        .viewports
        .entry(ViewportId::ROOT)
        .or_default()
        .events
        .push(ViewportEvent::Close);
    h.step();
    viewport_commands(h).contains(&ViewportCommand::CancelClose)
}

/// Ctrl/Cmd+Q (entrée `i` de `KEYMAP`) : document intact, la fenêtre se
/// ferme ; modifié, bandeau d'abord, et rien n'est fermé avant la réponse.
/// Relecture finale J4, F1 : sur macOS, Cmd+Q ne produisait aucun
/// `close_requested` et quittait sans rien demander.
pub fn check_quit_shortcut(i: usize) {
    let (_, shortcut, _) = plannercraft_ui_egui::keymap::KEYMAP[i];
    let mut s = sim(SaveReply::Ok);
    let h = &mut s.harness;
    open_demo(h);
    press_in_one_frame(h, shortcut);
    assert!(viewport_commands(h).contains(&ViewportCommand::Close));
    assert!(!h.state().confirmation_pending());

    let mut s = sim(SaveReply::Ok);
    let h = &mut s.harness;
    open_demo(h);
    edit_first_leaf(h, "17j");
    press_in_one_frame(h, shortcut);
    assert!(!viewport_commands(h).contains(&ViewportCommand::Close));
    assert!(h.state().confirmation_pending());
    h.run();
    let name = h.state().document.as_ref().unwrap().origin.name.clone();
    assert!(h.query_by_label(&confirm_title(&name)).is_some());
    // Pas encore de réponse : la fenêtre reste protégée.
    assert!(close_window_cancelled(h));
    h.state_mut().dispatch(Action::Confirm(
        plannercraft_ui_egui::action::ConfirmChoice::Discard,
    ));
    h.step();
    assert!(viewport_commands(h).contains(&ViewportCommand::Close));
}

/// Menu Fichier → Quitter : même garde que Ctrl/Cmd+Q ; « Enregistrer »
/// écrit d'abord, puis la fermeture est acceptée.
#[test]
fn ui_quit_menu_asks_then_saves_and_closes() {
    let mut s = sim(SaveReply::Ok);
    let h = &mut s.harness;
    open_demo(h);
    edit_first_leaf(h, "17j");
    file_menu(h, TextId::MenuQuit);
    assert!(h.state().confirmation_pending());
    assert!(s.requests.borrow().is_empty());
    let h = &mut s.harness;
    crate::click(h, &fr(TextId::ConfirmSave));
    assert_eq!(s.requests.borrow().len(), 1);
    assert!(!s.harness.state().document.as_ref().unwrap().is_dirty());
    // Fermeture acceptée (`quit_confirmed`) : plus d'annulation.
    assert!(!close_window_cancelled(&mut s.harness));
}

/// Quitter pendant un enregistrement lent : rien ne se ferme avant son
/// succès, puis la fenêtre se ferme.
#[test]
fn ui_quit_during_save_waits_for_success() {
    let mut s = sim(SaveReply::Hold);
    open_demo(&mut s.harness);
    edit_first_leaf(&mut s.harness, "17j");
    press_save(&mut s.harness);
    s.harness.state_mut().dispatch(Action::Quit);
    s.harness.step();
    assert!(!viewport_commands(&s.harness).contains(&ViewportCommand::Close));
    assert!(!s.harness.state().confirmation_pending());
    let target = s.requests.borrow()[0].target.clone();
    let tx = s.held.borrow_mut().pop().unwrap();
    tx.send(saved_event(&target)).unwrap();
    s.harness.step();
    assert!(viewport_commands(&s.harness).contains(&ViewportCommand::Close));
}

/// Relecture finale J4, F4 : la barre d'état marque le document modifié
/// d'un « * » (seul indicateur dans la page sur le web, qui ignore le
/// titre de la fenêtre), retiré après l'enregistrement.
#[test]
fn ui_status_bar_shows_dirty_star() {
    let mut s = sim(SaveReply::Ok);
    open_demo(&mut s.harness);
    let name = s.harness.state().document.as_ref().unwrap().origin.name.clone();
    let starred = format!("*{name}");
    assert!(s.harness.query_by_label(&name).is_some());
    assert!(s.harness.query_by_label(&starred).is_none());
    edit_first_leaf(&mut s.harness, "17j");
    assert!(s.harness.query_by_label(&starred).is_some());
    assert_eq!(
        s.harness.state().window_title(),
        plannercraft_ui_egui::app::window_title_for(s.harness.state().document.as_ref())
    );
    press_save(&mut s.harness);
    let name = s.harness.state().document.as_ref().unwrap().origin.name.clone();
    assert!(s.harness.query_by_label(&format!("*{name}")).is_none());
    assert!(s.harness.query_by_label(&name).is_some());
}
