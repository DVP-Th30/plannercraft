//! Version web (J8, lot L13) : garde des raccourcis (H-UI-55), fichiers
//! déposés (H-UI-56), téléchargements par genre (`SaveKind`).
//!
//! La garde elle-même (écouteur `keydown` de `apps/plannercraft-web`) ne tourne
//! que dans un navigateur ; sa décision est la fonction pure
//! [`web_intercepts`], testée ici sur [`KEYMAP`] et sur le périmètre rendu
//! par l'application ([`plannercraft_ui_egui::App::key_scope`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use plannercraft_core::Severity;
use plannercraft_ui_egui::action::ConfirmChoice;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::io::{SaveKind, SaveTarget};
use plannercraft_ui_egui::keymap::{
    Binding, KEYMAP, KeyScope, KeyState, Shortcut, UiBinding, WEB_NEVER, web_interceptable, web_intercepts,
};

use plannercraft_ui_egui::{Action, App, DEMO_MSPDI};

use crate::files::{SaveReply, edit_first_leaf, sim};
use crate::{harness, open_demo};

/// Document ouvert, table focalisée.
const TABLE: KeyState = KeyState {
    document: true,
    table: true,
};

/// Document ouvert, table sans focus.
const DOCUMENT: KeyState = KeyState {
    document: true,
    table: false,
};

/// Ctrl/Cmd + Maj.
const COMMAND_SHIFT: Modifiers = Modifiers {
    shift: true,
    ..Modifiers::COMMAND
};

/// H-UI-55 : liste attendue des entrées retenues au navigateur ; exclues :
/// Ctrl/Cmd+N (nouvelle fenêtre), Tab et Maj+Tab, Ctrl/Cmd+Q (Quitter) ;
/// seconde passe UX (H-UI-109) : Ctrl/Cmd+W et Ctrl+F4 (fermer le
/// document), Ctrl/Cmd+P (impression) et Ctrl/Cmd+R (remplir à droite)
/// restent au navigateur ; aucune entrée à T, et W seulement pour fermer le
/// document (le navigateur ne les rend jamais).
#[test]
fn h_ui_55_interceptable_flags() {
    let excluded: Vec<&Shortcut> = KEYMAP
        .iter()
        .filter(|(_, shortcut, binding)| !web_interceptable(shortcut, *binding))
        .map(|(_, shortcut, _)| shortcut)
        .collect();
    let expected = [
        Shortcut::new(Modifiers::COMMAND, Key::Q),
        Shortcut::new(Modifiers::COMMAND, Key::N),
        Shortcut::new(Modifiers::SHIFT, Key::Tab),
        Shortcut::new(Modifiers::NONE, Key::Tab),
        Shortcut::new(Modifiers::COMMAND, Key::W),
        Shortcut::new(Modifiers::CTRL, Key::F4),
        Shortcut::new(Modifiers::COMMAND, Key::P),
        Shortcut::new(Modifiers::COMMAND, Key::R),
    ];
    assert_eq!(excluded, expected.iter().collect::<Vec<_>>());
    assert!(
        !KEYMAP
            .iter()
            .any(|(_, shortcut, binding)| shortcut.logical_key == Key::T
                || (shortcut.logical_key == Key::W && *binding != Binding::Ui(UiBinding::CloseDocument))),
        "Ctrl/Cmd+T, ou W hors « fermer le document », dans KEYMAP : le navigateur ne les rend pas"
    );
    assert_eq!(WEB_NEVER, &[Key::N, Key::W, Key::T, Key::Tab]);
    // Entrées que le navigateur prendrait pour lui : recherche dans la
    // page, page précédente ou suivante, zoom de la page, infos de la page.
    for (modifiers, key) in [
        (Modifiers::COMMAND, Key::F),
        (Modifiers::COMMAND, Key::G),
        (COMMAND_SHIFT, Key::G),
        (Modifiers::NONE, Key::F3),
        (Modifiers::SHIFT, Key::F3),
        (Modifiers::ALT, Key::ArrowLeft),
        (Modifiers::ALT, Key::ArrowRight),
        (Modifiers::ALT, Key::Home),
        (Modifiers::COMMAND, Key::Equals),
        (Modifiers::COMMAND, Key::Minus),
        (Modifiers::COMMAND, Key::I),
        (COMMAND_SHIFT, Key::I),
        (Modifiers::COMMAND, Key::S),
        (Modifiers::COMMAND, Key::O),
    ] {
        let shortcut = Shortcut::new(modifiers, key);
        let (_, _, binding) = KEYMAP
            .iter()
            .find(|(_, s, _)| *s == shortcut)
            .unwrap_or_else(|| panic!("{shortcut:?} absent de KEYMAP"));
        assert!(web_interceptable(&shortcut, *binding), "{shortcut:?}");
    }
    assert!(!web_interceptable(
        &Shortcut::new(Modifiers::COMMAND, Key::Q),
        Binding::Ui(UiBinding::Quit)
    ));
}

/// H-UI-55 : la garde ne retient que ce que l'image lirait.
#[test]
fn h_ui_55_intercepts_follow_scope() {
    let all = KeyScope::All(TABLE);
    assert!(web_intercepts(all, Modifiers::COMMAND, Key::F));
    assert!(web_intercepts(all, Modifiers::NONE, Key::F3));
    assert!(web_intercepts(all, Modifiers::SHIFT, Key::F3));
    assert!(web_intercepts(all, COMMAND_SHIFT, Key::G));
    assert!(web_intercepts(all, Modifiers::ALT, Key::ArrowLeft));
    // Jamais : nouvelle fenêtre, onglet, presse-papiers (évènements à part).
    assert!(!web_intercepts(all, Modifiers::COMMAND, Key::N));
    assert!(!web_intercepts(all, Modifiers::COMMAND, Key::W));
    assert!(!web_intercepts(all, Modifiers::COMMAND, Key::T));
    assert!(!web_intercepts(all, Modifiers::NONE, Key::Tab));
    assert!(!web_intercepts(all, Modifiers::COMMAND, Key::C));
    assert!(!web_intercepts(all, Modifiers::COMMAND, Key::V));
    assert!(!web_intercepts(all, Modifiers::COMMAND, Key::X));
    assert!(!web_intercepts(all, Modifiers::COMMAND, Key::Q));
    // Sans document : Ctrl+F (contexte Document) reste au navigateur,
    // Ctrl+O (Global) est retenu.
    let welcome = KeyScope::All(KeyState::default());
    assert!(!web_intercepts(welcome, Modifiers::COMMAND, Key::F));
    assert!(web_intercepts(welcome, Modifiers::COMMAND, Key::O));
    // Table sans focus : la navigation (contexte Table) n'est pas lue.
    assert!(!web_intercepts(
        KeyScope::All(DOCUMENT),
        Modifiers::NONE,
        Key::Enter
    ));
    assert!(web_intercepts(KeyScope::All(TABLE), Modifiers::NONE, Key::Enter));
    // Fenêtre non modale : seuls les raccourcis gardés à côté d'elle.
    let beside = KeyScope::BesideWindow(DOCUMENT);
    assert!(web_intercepts(beside, Modifiers::COMMAND, Key::S));
    assert!(web_intercepts(beside, Modifiers::COMMAND, Key::Z));
    assert!(!web_intercepts(beside, Modifiers::COMMAND, Key::F));
    // Saisie de texte, fenêtre modale : rien.
    assert!(!web_intercepts(KeyScope::Nothing, Modifiers::COMMAND, Key::S));
    assert!(!web_intercepts(KeyScope::Nothing, Modifiers::COMMAND, Key::F));
}

/// H-UI-55 : périmètre rendu par l'application : accueil, document avec
/// table focalisée, puis barre de recherche en saisie (rien de retenu :
/// Ctrl+F y reste au navigateur, la saisie garde ses touches).
#[test]
fn h_ui_55_scope_from_app() {
    let mut harness = harness();
    harness.run();
    assert_eq!(
        harness.state().key_scope(&harness.ctx),
        KeyScope::All(KeyState::default())
    );
    open_demo(&mut harness);
    let scope = harness.state().key_scope(&harness.ctx);
    assert!(matches!(scope, KeyScope::All(keys) if keys.document), "{scope:?}");
    assert!(web_intercepts(scope, Modifiers::COMMAND, Key::F));
    harness.key_press_modifiers(Modifiers::COMMAND, Key::F);
    harness.run();
    assert!(harness.state().search().is_some(), "barre de recherche ouverte");
    assert_eq!(harness.state().key_scope(&harness.ctx), KeyScope::Nothing);
}

/// Fichier déposé simulé : chemin relatif (comme sur le web) et octets.
#[derive(Debug)]
struct Dropped {
    path: PathBuf,
    bytes: Result<Vec<u8>, String>,
}

impl egui::DroppedFile for Dropped {
    fn path(&self) -> &Path {
        &self.path
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        self.bytes.clone()
    }
}

/// Dépose `files` (nom, octets) sur la fenêtre à la prochaine image.
fn drop_files(harness: &mut Harness<'static, App>, files: &[(&str, Result<Vec<u8>, String>)]) {
    for (name, bytes) in files {
        harness.input_mut().dropped_files.push(Arc::new(Dropped {
            path: PathBuf::from(name),
            bytes: bytes.clone(),
        }));
    }
    harness.run();
}

/// Nom du document ouvert.
fn doc_name(harness: &Harness<'static, App>) -> Option<String> {
    harness
        .state()
        .document
        .as_ref()
        .map(|document| document.origin.name.clone())
}

/// H-UI-56 : un fichier déposé sur l'accueil est ouvert comme par
/// « Ouvrir » ; plusieurs ⇒ le premier, message ; illisible ⇒ message
/// d'ouverture, document courant gardé.
#[test]
fn h_ui_56_dropped_file_opens() {
    let mut harness = harness();
    harness.run();
    drop_files(&mut harness, &[("maison.xml", Ok(DEMO_MSPDI.to_vec()))]);
    assert_eq!(doc_name(&harness).as_deref(), Some("maison.xml"));
    let document = harness.state().document.as_ref().unwrap();
    assert!(document.origin.path.is_none() && !document.origin.demo);
    assert!(!document.is_dirty());

    drop_files(
        &mut harness,
        &[
            ("b.xml", Ok(DEMO_MSPDI.to_vec())),
            ("c.xml", Ok(DEMO_MSPDI.to_vec())),
        ],
    );
    assert_eq!(doc_name(&harness).as_deref(), Some("b.xml"));
    let warned = Texts::new(Language::Fr).get_args(
        TextId::DroppedFirstOnly,
        &[("count", Arg::Count(2)), ("name", Arg::Str("b.xml".into()))],
    );
    assert!(
        harness.state().messages().iter().any(|m| m.text == warned),
        "message « seul le premier »"
    );

    drop_files(&mut harness, &[("vide.xml", Err("lecture refusée".to_owned()))]);
    assert_eq!(doc_name(&harness).as_deref(), Some("b.xml"));
    assert!(
        harness
            .state()
            .messages()
            .iter()
            .any(|m| m.severity == Severity::Error && m.text.contains("lecture refusée")),
        "erreur de lecture affichée"
    );
}

/// H-UI-56, relecture finale de J8 : un fichier déposé plus grand que la
/// borne du lecteur MSPDI (256 Mio) est refusé **avant** d'être lu, par le
/// message `mspdi.error.too_large` ; le document ouvert reste. Fichier
/// creux (aucun octet écrit) et illisible (droits retirés) : une lecture
/// complète échouerait sur les droits, le refus vient donc de la taille.
#[test]
fn h_ui_56_dropped_file_too_large_refused_before_read() {
    let mut harness = harness();
    open_demo(&mut harness);
    let demo = doc_name(&harness);
    let dir = std::env::temp_dir().join(format!("plannercraft-drop-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("enorme.xml");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(plannercraft_ui_egui::document::max_open_bytes() + 1)
        .unwrap();
    drop(file);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
    }
    harness.input_mut().dropped_files.push(Arc::new(Dropped {
        path: path.clone(),
        bytes: Err("jamais lu".to_owned()),
    }));
    harness.run();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir(&dir).unwrap();
    assert_eq!(doc_name(&harness), demo, "document courant gardé");
    let refused = harness
        .state()
        .messages()
        .iter()
        .find(|m| m.severity == Severity::Error)
        .map(|m| m.text.clone())
        .expect("refus affiché");
    assert!(refused.contains("document trop grand"), "{refused}");
    // Taille d'un `Blob` (web) : même borne, valeurs hostiles refusées.
    let check_blob_size = plannercraft_ui_egui::document::check_blob_size;
    assert!(check_blob_size(1_024.0).is_ok());
    assert!(check_blob_size(3.0e9).is_err());
    assert!(check_blob_size(f64::NAN).is_err());
    assert!(check_blob_size(-1.0).is_err());
}

/// H-UI-56 : document modifié ⇒ bandeau de confirmation d'abord ; Annuler
/// garde le document, Ne pas enregistrer ouvre le fichier déposé.
#[test]
fn h_ui_56_dropped_file_unsaved_guard() {
    let mut harness = harness();
    open_demo(&mut harness);
    let demo = doc_name(&harness);
    edit_first_leaf(&mut harness, "3j");
    assert!(harness.state().document.as_ref().unwrap().is_dirty());

    drop_files(&mut harness, &[("autre.xml", Ok(DEMO_MSPDI.to_vec()))]);
    assert!(harness.state().confirmation_pending());
    assert_eq!(doc_name(&harness), demo);
    harness
        .state_mut()
        .dispatch(Action::Confirm(ConfirmChoice::Cancel));
    harness.run();
    assert!(!harness.state().confirmation_pending());
    assert_eq!(doc_name(&harness), demo);
    assert!(harness.state().document.as_ref().unwrap().is_dirty());

    drop_files(&mut harness, &[("autre.xml", Ok(DEMO_MSPDI.to_vec()))]);
    assert!(harness.state().confirmation_pending());
    harness
        .state_mut()
        .dispatch(Action::Confirm(ConfirmChoice::Discard));
    harness.run();
    assert_eq!(doc_name(&harness).as_deref(), Some("autre.xml"));
    assert!(!harness.state().document.as_ref().unwrap().is_dirty());
}

/// Fichier → Exporter (J8, lot L13) : PDF, SVG et CSV remis au
/// `FileService` avec leur genre et un nom proposé à la bonne extension ;
/// message « Exporté » ; le document reste lié à son origine et non
/// modifié (un export n'est pas un enregistrement).
#[test]
fn save_kind_export_requests() {
    let mut sim = sim(SaveReply::Ok);
    open_demo(&mut sim.harness);
    let origin = sim.harness.state().document.as_ref().unwrap().origin.clone();
    for (kind, magic) in [
        (SaveKind::Pdf, &b"%PDF-"[..]),
        (SaveKind::Svg, &b"<svg"[..]),
        (SaveKind::Csv, &b"\xEF\xBB\xBF"[..]),
    ] {
        sim.harness.state_mut().dispatch(Action::Export(kind));
        sim.harness.run();
        let requests = sim.requests.borrow();
        let request = requests.last().unwrap();
        assert_eq!(request.kind, kind);
        assert!(
            request.bytes.windows(magic.len()).take(64).any(|w| w == magic),
            "{kind:?}"
        );
        let SaveTarget::Dialog { suggested_name, .. } = &request.target else {
            panic!("export vers un chemin connu : {:?}", request.target);
        };
        assert!(
            suggested_name.ends_with(&format!(".{}", kind.extension())),
            "{suggested_name}"
        );
        let done = Texts::new(Language::Fr).get_args(
            TextId::ExportDone,
            &[("name", Arg::Str(suggested_name.clone().into()))],
        );
        assert!(
            sim.harness.state().messages().iter().any(|m| m.text == done),
            "message « Exporté » pour {kind:?}"
        );
    }
    let document = sim.harness.state().document.as_ref().unwrap();
    assert_eq!(document.origin, origin);
    assert!(!document.is_dirty());
}
