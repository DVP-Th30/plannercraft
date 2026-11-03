//! Fichier → Importer CSV… (J8, lot L10, H-UI-40) : sélecteur scripté
//! (`FileService::pick_bytes`), aperçu (séparateur, encodage, langue
//! modifiables ; sort des colonnes ; lignes), puis « Importer » = nouveau
//! document, derrière le bandeau « Enregistrer les modifications ? » si le
//! document courant est modifié.

use std::collections::VecDeque;
use std::sync::mpsc::Sender;

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use plannercraft_automation::csv::{CsvEncoding, CsvLocale, Recognized};
use plannercraft_core::{Moment, Severity};
use plannercraft_ui_egui::action::DialogKind;
use plannercraft_ui_egui::dialog::DialogBody;
use plannercraft_ui_egui::dialog::csv_import::{CsvImportState, SeparatorChoice, document_name};
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::io::{OpenEvent, PickError, PickRequest, PickedFile};
use plannercraft_ui_egui::{Action, App, Env, FileService};

use crate::{fr, fr_light, harness_for, now, open_demo};

/// Sélecteur de test : rend les fichiers du script, dans l'ordre.
struct PickedFiles(VecDeque<PickedFile>);

impl FileService for PickedFiles {
    fn pick_and_open(&mut self, ctx: &egui::Context, _: Moment, _: String, _: String, tx: Sender<OpenEvent>) {
        tx.send(OpenEvent::Cancelled).unwrap();
        ctx.request_repaint();
    }

    fn pick_bytes(&mut self, ctx: &egui::Context, request: PickRequest, tx: Sender<PickedFile>) {
        assert_eq!(request.extensions, ["csv", "txt"]);
        tx.send(self.0.pop_front().unwrap_or(PickedFile::Cancelled))
            .unwrap();
        ctx.request_repaint();
    }
}

/// Fichier choisi `name` de contenu `bytes`.
fn picked(name: &str, bytes: &[u8]) -> PickedFile {
    PickedFile::Picked {
        name: name.to_owned(),
        result: Ok(bytes.to_vec()),
    }
}

/// Harnais dont le sélecteur rend `files`.
fn harness_with(files: Vec<PickedFile>) -> Harness<'static, App> {
    let env = Env {
        files: Box::new(PickedFiles(files.into())),
        ..Env::fixed(now())
    };
    harness_for(App::new(fr_light(), env))
}

/// Ouvre la liste nommée `combo` et choisit l'entrée `entry`.
fn choose(harness: &mut Harness<'static, App>, combo: &str, entry: &str) {
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, combo)
        .click();
    harness.run();
    crate::click(harness, entry);
}

/// Brouillon de l'aperçu ouvert.
fn state(app: &App) -> &CsvImportState {
    match app.view().dialog.as_ref().map(|d| (&d.kind, &d.body)) {
        Some((DialogKind::CsvImport, DialogBody::CsvImport(state))) => state,
        other => panic!("aperçu non ouvert : {other:?}"),
    }
}

/// Clique l'entrée du menu Fichier dont le libellé commence par `item`.
fn file_menu(harness: &mut Harness<'static, App>, item: TextId) {
    crate::click(harness, &fr(TextId::MenuFile));
    let label = fr(item);
    harness
        .get_by(|node| node.label().is_some_and(|text| text.starts_with(&label)))
        .click();
    harness.run();
}

/// CSV « Nom;Durée » de deux tâches (UTF-8).
const TWO_TASKS: &[u8] = "Nom;Durée;Couleur\nCoffrage;3j;bleu\nFerraillage;2j;\n".as_bytes();

#[test]
fn h_ui_40_preview_and_import() {
    let mut harness = harness_with(vec![picked("taches.csv", TWO_TASKS)]);
    // Sans document : l'import reste proposé.
    file_menu(&mut harness, TextId::MenuImportCsv);
    let app = harness.state();
    let s = state(app);
    assert_eq!(s.name, "taches.csv");
    let preview = s.preview.as_ref().unwrap();
    assert_eq!(preview.detected.separator, b';');
    assert_eq!(preview.detected.locale, CsvLocale::Fr);
    let sorts: Vec<bool> = preview
        .columns
        .iter()
        .map(|(_, r)| matches!(r, Recognized::Column(_)))
        .collect();
    assert_eq!(sorts, [true, true, false]);
    assert_eq!(preview.rows.len(), 2);
    // Sort des colonnes et cellules affichés.
    assert_eq!(harness.query_all_by_label(&fr(TextId::CiRecognized)).count(), 2);
    assert_eq!(harness.query_all_by_label(&fr(TextId::CiUnknown)).count(), 1);
    assert!(harness.query_by_label("Ferraillage").is_some());
    // Séparateur imposé : virgule ⇒ un seul en-tête, ni Nom ni Durée ⇒
    // refus affiché, « Importer » grisé.
    choose(&mut harness, &fr(TextId::CiSeparator), &fr(TextId::CiSepComma));
    assert_eq!(state(harness.state()).separator, SeparatorChoice::Comma);
    assert!(state(harness.state()).preview.is_err());
    let refused = fr(TextId::ErrorCsvErrorNoKnownColumn);
    assert!(harness.query_by_label(&refused).is_some(), "refus non affiché");
    let import = fr(TextId::CiImport);
    assert!(harness.get_by_label(&import).accesskit_node().is_disabled());
    // Point-virgule imposé (celui du fichier) : aperçu rétabli ; encodage
    // imposé : retenu par l'aperçu.
    choose(
        &mut harness,
        &fr(TextId::CiSeparator),
        &fr(TextId::CiSepSemicolon),
    );
    assert!(state(harness.state()).preview.is_ok());
    choose(
        &mut harness,
        &fr(TextId::CiEncoding),
        &fr(TextId::CiEncWindows1252),
    );
    let p = state(harness.state()).preview.as_ref().unwrap();
    assert_eq!(p.detected.encoding, CsvEncoding::Windows1252);
    // UTF-8 relu en Windows-1252 : « Durée » devient « DurÃ©e », non reconnue.
    assert!(!matches!(p.columns[1].1, Recognized::Column(_)));
    choose(&mut harness, &fr(TextId::CiEncoding), &fr(TextId::CiEncUtf8));
    // « Importer ».
    crate::click(&mut harness, &fr(TextId::CiImport));
    let app = harness.state();
    assert!(app.view().dialog.is_none());
    let document = app.document.as_ref().expect("document importé");
    assert_eq!(document.origin.name, "taches");
    assert_eq!(document.origin.path, None);
    let names: Vec<&str> = document.project().tasks.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["Coffrage", "Ferraillage"]);
    assert!(!document.is_dirty());
    // La colonne inconnue est une remarque de l'import, signalée.
    let expected = Texts::new(Language::Fr).get_args(
        TextId::CiImported,
        &[("count", Arg::Count(1)), ("name", Arg::Str("taches.csv".into()))],
    );
    assert!(
        app.messages()
            .iter()
            .any(|m| m.severity == Severity::Warning && m.text == expected),
        "{:?}",
        app.messages()
    );
    assert_eq!(document_name("a.b.csv"), "a.b");
}

#[test]
fn h_ui_40_unsaved_guard() {
    let mut harness = harness_with(vec![
        picked("taches.csv", TWO_TASKS),
        picked("taches.csv", TWO_TASKS),
    ]);
    open_demo(&mut harness);
    // Document modifié (une tâche ajoutée).
    harness.state_mut().dispatch(Action::Structure(
        plannercraft_ui_egui::structure::StructureOp::InsertBefore,
    ));
    harness.run();
    assert!(harness.state().document.as_ref().unwrap().is_dirty());
    harness.state_mut().dispatch(Action::ImportCsv);
    harness.run();
    crate::click(&mut harness, &fr(TextId::CiImport));
    let app = harness.state();
    assert!(app.confirmation_pending(), "aucune confirmation demandée");
    assert!(
        app.document.as_ref().unwrap().origin.demo,
        "document remplacé sans confirmation"
    );
    // Annuler : rien de perdu.
    crate::click(&mut harness, &fr(TextId::ConfirmCancel));
    assert!(harness.state().document.as_ref().unwrap().origin.demo);
    // Nouvel import, « Ne pas enregistrer » : document remplacé.
    harness.state_mut().dispatch(Action::ImportCsv);
    harness.run();
    crate::click(&mut harness, &fr(TextId::CiImport));
    crate::click(&mut harness, &fr(TextId::ConfirmDiscard));
    let document = harness.state().document.as_ref().unwrap();
    assert_eq!(document.origin.name, "taches");
    assert_eq!(document.project().tasks.len(), 2);
}

#[test]
fn csv_import_read_errors_and_cancel() {
    let mut harness = harness_with(vec![
        PickedFile::Cancelled,
        PickedFile::Picked {
            name: "gros.csv".to_owned(),
            result: Err(PickError::TooLarge),
        },
        picked("vide.csv", b""),
    ]);
    // Sélecteur refermé : rien.
    harness.state_mut().dispatch(Action::ImportCsv);
    harness.run();
    assert!(harness.state().view().dialog.is_none());
    assert_eq!(harness.state().messages(), []);
    // Fichier trop grand : refus au bandeau, aucune fenêtre.
    harness.state_mut().dispatch(Action::ImportCsv);
    harness.run();
    assert!(harness.state().view().dialog.is_none());
    let message = &harness.state().messages()[0];
    assert_eq!(message.text, fr(TextId::ErrorCsvErrorTooLarge));
    // Fichier vide : aperçu refusé, Échap referme.
    harness.state_mut().dispatch(Action::ImportCsv);
    harness.run();
    assert!(state(harness.state()).preview.is_err());
    assert!(harness.query_by_label(&fr(TextId::ErrorCsvErrorEmpty)).is_some());
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(harness.state().view().dialog.is_none());
    assert!(harness.state().document.is_none());
}
