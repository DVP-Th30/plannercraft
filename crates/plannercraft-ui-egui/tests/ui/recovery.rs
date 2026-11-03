//! Reprise après plantage (J7, lot L15) : bandeau « Récupérer … »
//! (Restaurer, Ignorer, Plus tard), journal du document ouvert, disque plein
//! sans jamais bloquer l'édition.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_automation::recovery::{Journal, Recorder, Store, orphans};
use plannercraft_automation::{ApplyMode, Batch, Session};
use plannercraft_core::TaskUid;
use plannercraft_ui_egui::app::EditOrigin;
use plannercraft_ui_egui::document;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::{Action, App};

use crate::{fr, harness_for, new_app, now, open_demo};

/// Dossier temporaire propre au test, supprimé à la fin.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("pu-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn rename(uid: TaskUid, name: &str) -> Batch {
    Batch::from_json_str(&format!(
        r#"{{"format_version":1,"ops":[{{"op":"task.update","uid":{},"name":"{name}"}}]}}"#,
        uid.get()
    ))
    .unwrap()
}

/// Session de l'exemple, sa première tâche.
fn demo_session() -> (Session, TaskUid) {
    let session = document::load(plannercraft_ui_egui::DEMO_MSPDI, now())
        .unwrap()
        .session;
    let uid = session.project().tasks.uids().next().unwrap();
    (session, uid)
}

/// Orphelin de l'exemple dans `root` : deux renommages de la première
/// tâche, journal abandonné sans fermeture (plantage).
fn crashed_demo(root: &Path) -> (Session, TaskUid) {
    let (mut session, uid) = demo_session();
    session.set_origin(plannercraft_automation::DocOrigin {
        name: "maison.xml".to_owned(),
        path: Some("/projets/maison.xml".to_owned()),
    });
    let mut journal = Journal::create(root, &session).unwrap();
    for name in ["Avant", "Repris"] {
        session.apply(&rename(uid, name), ApplyMode::Commit).unwrap();
        journal.record(&session, now()).unwrap();
    }
    drop(journal);
    (session, uid)
}

/// Application avec un journal dans `root` et les orphelins de `root`
/// proposés.
fn recovering_app(root: &Path) -> Harness<'static, App> {
    let mut app = new_app();
    app.offer_recovery(orphans(root));
    app.set_recovery(Box::new(Recorder::in_dir(root.to_path_buf())));
    let mut harness = harness_for(app);
    harness.run();
    harness
}

fn title(name: &str, count: i64) -> String {
    Texts::new(Language::Fr).get_args(
        TextId::RecoveryTitle,
        &[("name", Arg::Str(name.into())), ("count", Arg::Count(count))],
    )
}

/// Restaurer : le document du journal s'ouvre, modifié, avec son
/// historique (annuler revient à l'état d'avant) ; le journal orphelin est
/// supprimé, le document tient le sien.
#[test]
fn recovery_banner_restore_marks_dirty() {
    let tmp = TempDir::new("restore");
    let (crashed, uid) = crashed_demo(tmp.path());
    let mut harness = recovering_app(tmp.path());
    assert_eq!(harness.state().recovery_offers().len(), 1);
    harness.get_by_label(&title("maison.xml", 2));
    crate::click(&mut harness, &fr(TextId::RecoveryRestore));
    let app = harness.state();
    let doc = app.document.as_ref().expect("document restauré");
    assert!(doc.is_dirty(), "document restauré non marqué modifié");
    assert!(app.window_title().starts_with('*'));
    assert_eq!(doc.origin.name, "maison.xml");
    assert_eq!(doc.origin.path.as_deref(), Some("/projets/maison.xml"));
    assert_eq!(doc.project(), crashed.project());
    assert_eq!(doc.session.history().entries.len(), 2);
    assert_eq!(app.recovery_offers().len(), 0);
    assert!(harness.query_by_label(&title("maison.xml", 2)).is_none());
    assert!(
        orphans(tmp.path()).is_empty(),
        "orphelin gardé ou journal non verrouillé"
    );
    assert_eq!(
        std::fs::read_dir(tmp.path()).unwrap().count(),
        1,
        "journal du document restauré"
    );
    let app = harness.state_mut();
    app.undo().unwrap();
    let name = &app
        .document
        .as_ref()
        .unwrap()
        .project()
        .tasks
        .get(uid)
        .unwrap()
        .name;
    assert_eq!(name, "Avant");
}

/// Relecture finale J7, F3 : le fichier du document a été réécrit après
/// le plantage ; le bandeau le dit, et Restaurer installe le document
/// **sans chemin** (« maison (récupéré).xml ») : Enregistrer ne peut pas
/// écraser la version plus récente.
#[test]
fn recovery_restore_changed_file_drops_path() {
    let tmp = TempDir::new("changed");
    let root = tmp.path().join("journaux");
    let file = tmp.path().join("maison.xml");
    std::fs::write(&file, plannercraft_ui_egui::DEMO_MSPDI).unwrap();
    let (mut session, uid) = demo_session();
    session.set_origin(plannercraft_automation::DocOrigin {
        name: "maison.xml".to_owned(),
        path: Some(file.to_str().unwrap().to_owned()),
    });
    let mut journal = Journal::create(&root, &session).unwrap();
    session.apply(&rename(uid, "Repris"), ApplyMode::Commit).unwrap();
    journal.record(&session, now()).unwrap();
    drop(journal);
    // Version plus récente écrite ailleurs entre-temps.
    std::fs::write(&file, b"<Project><Name>plus recent</Name></Project>").unwrap();
    let mut harness = recovering_app(&root);
    harness.get_by_label(&fr(TextId::RecoveryFileChanged));
    crate::click(&mut harness, &fr(TextId::RecoveryRestore));
    let doc = harness.state().document.as_ref().expect("document restauré");
    assert_eq!(
        doc.origin.path, None,
        "chemin repris : Enregistrer écraserait le fichier"
    );
    let expected =
        Texts::new(Language::Fr).get_args(TextId::RecoveredFileName, &[("stem", Arg::Str("maison".into()))]);
    assert_eq!(doc.origin.name, format!("{expected}.xml"));
    assert!(doc.is_dirty());
    assert_eq!(
        std::fs::read(&file).unwrap(),
        b"<Project><Name>plus recent</Name></Project>"
    );
}

/// Restaurer alors que le document courant est modifié : bandeau de
/// confirmation d'abord.
#[test]
fn recovery_restore_confirms_dirty_document() {
    let tmp = TempDir::new("confirm");
    crashed_demo(tmp.path());
    let mut harness = recovering_app(tmp.path());
    open_demo(&mut harness);
    let uid = harness.state().rows().unwrap().row(0).unwrap().uid;
    harness.state_mut().dispatch(Action::Exec(rename(uid, "Courant")));
    harness.run();
    crate::click(&mut harness, &fr(TextId::RecoveryRestore));
    assert!(harness.state().confirmation_pending());
    crate::click(&mut harness, &fr(TextId::ConfirmDiscard));
    let doc = harness.state().document.as_ref().unwrap();
    assert_eq!(doc.origin.name, "maison.xml");
    assert!(doc.is_dirty());
}

/// Ignorer supprime le journal ; Plus tard retire le bandeau et garde le
/// journal pour le prochain démarrage.
#[test]
fn recovery_discard_and_later() {
    let tmp = TempDir::new("discard");
    crashed_demo(tmp.path());
    let mut harness = recovering_app(tmp.path());
    crate::click(&mut harness, &fr(TextId::RecoveryLater));
    assert_eq!(harness.state().recovery_offers().len(), 0);
    assert!(harness.query_by_label(&fr(TextId::RecoveryRestore)).is_none());
    assert_eq!(orphans(tmp.path()).len(), 1, "Plus tard : journal gardé");
    drop(harness);

    let mut harness = recovering_app(tmp.path());
    crate::click(&mut harness, &fr(TextId::RecoveryDiscard));
    assert_eq!(harness.state().recovery_offers().len(), 0);
    assert!(harness.state().document.is_none());
    assert!(orphans(tmp.path()).is_empty(), "Ignorer : journal supprimé");
}

/// Écrivain simulé : la base passe, toute ligne échoue (disque plein).
struct FullDisk;

impl Store for FullDisk {
    fn replace_base(&mut self, _: &[u8]) -> io::Result<()> {
        Ok(())
    }

    fn append(&mut self, _: &[u8]) -> io::Result<()> {
        Err(io::Error::other("disque plein"))
    }

    fn truncate_ops(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn remove(self: Box<Self>) -> io::Result<()> {
        Ok(())
    }
}

/// Disque plein : le journal est coupé, signalé une fois ; chaque édition
/// passe quand même (aucune n'est refusée ni retardée).
#[test]
fn recovery_disk_full_never_blocks() {
    let mut app = new_app();
    app.set_recovery(Box::new(Recorder::with_factory(Box::new(|| {
        Ok(Box::new(FullDisk) as Box<dyn Store>)
    }))));
    let mut harness = harness_for(app);
    harness.run();
    open_demo(&mut harness);
    let uid = harness.state().rows().unwrap().row(0).unwrap().uid;
    for i in 0..3 {
        let name = format!("N{i}");
        let app = harness.state_mut();
        app.execute(&rename(uid, &name), EditOrigin::User).unwrap();
        let doc = app.document.as_ref().unwrap();
        assert_eq!(doc.project().tasks.get(uid).unwrap().name, name);
    }
    harness.run();
    let cut = Texts::new(Language::Fr).get_args(
        TextId::RecoveryJournalCut,
        &[("detail", Arg::Str("disque plein".into()))],
    );
    let messages = harness.state().messages();
    let shown = messages.iter().filter(|m| m.text == cut).count();
    assert_eq!(shown, 1, "{messages:?}");
    assert_eq!(
        harness
            .state()
            .document
            .as_ref()
            .unwrap()
            .session
            .history()
            .entries
            .len(),
        3
    );
}
