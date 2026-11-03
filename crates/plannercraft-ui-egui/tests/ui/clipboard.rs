//! Couper, copier, coller de lignes (J8, lot L10, H-UI-39 ; R-244 et R-256
//! de bout en bout) : `Event::Copy/Cut/Paste` pris quand la table a le
//! focus, texte tabulé des colonnes visibles dans la sortie de la plateforme
//! (`PlatformOutput`), collage complet par l'empreinte du dernier texte
//! émis, collage externe réduit, un lot par geste, compte rendu au bandeau,
//! tampon gardé à l'ouverture d'un autre document. Repère : projet « P »
//! (`plannercraft_tests::daily::plan_p`).

use egui::{Event, Key};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_automation::clip::fingerprint;
use plannercraft_core::{Severity, TaskUid};
use plannercraft_tests::daily::plan_p;
use plannercraft_ui_egui::action::{PressMode, PressTarget, SelectOp};
use plannercraft_ui_egui::clipboard::ClipboardOp;
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::nav::RowRef;
use plannercraft_ui_egui::{Action, App, DocOrigin};

use crate::{fr, harness_for, new_app};

/// UID `n`.
const fn uid(n: u32) -> TaskUid {
    TaskUid::new(n)
}

/// Harnais sur le projet « P », table focalisée, première ligne active.
fn p_harness() -> Harness<'static, App> {
    let mut app = new_app();
    let session = plan_p();
    app.open_project(
        session.project().clone(),
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

/// Sélectionne les tâches `first` à `last` en lignes entières (appui sur
/// l'ID puis Maj+appui, H-UI-89) : le presse-papiers de lignes (H-UI-39)
/// suit les lignes entières depuis la passe UX (H-UI-95, ADR-0018 D3).
fn select(harness: &mut Harness<'static, App>, first: u32, last: u32) {
    press_id(harness, first, PressMode::Replace);
    if last != first {
        press_id(harness, last, PressMode::Extend);
    }
}

/// Appui sur l'ID de la tâche `n` (en-tête de ligne), cellule active au
/// Nom (F2 y ouvre la saisie).
fn press_id(harness: &mut Harness<'static, App>, n: u32, mode: PressMode) {
    harness.state_mut().dispatch(Action::SelectOp(SelectOp::Press {
        row: RowRef::Item(uid(n)),
        column: Column::Name,
        target: PressTarget::RowHeader,
        mode,
    }));
    harness.run();
}

/// Envoie `event` et rend le texte écrit dans le presse-papiers par cette
/// image (`OutputCommand::CopyText`), s'il y en a un.
fn copy_event(harness: &mut Harness<'static, App>, event: Event) -> Option<String> {
    harness.event(event);
    harness.step();
    let copied = harness
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|c| match c {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        });
    harness.run();
    copied
}

/// Colle `text` comme le ferait Ctrl+V.
fn paste(harness: &mut Harness<'static, App>, text: &str) {
    harness.event(Event::Paste(text.to_owned()));
    harness.run();
}

/// Noms et niveaux des tâches du document.
fn names(app: &App) -> Vec<(String, u16)> {
    app.document
        .as_ref()
        .unwrap()
        .project()
        .tasks
        .iter()
        .map(|t| (t.name.clone(), t.outline_level.get()))
        .collect()
}

/// Noms seuls.
fn plain_names(app: &App) -> Vec<String> {
    names(app).into_iter().map(|(n, _)| n).collect()
}

/// Nombre d'entrées d'annulation.
fn history_len(app: &App) -> usize {
    app.document.as_ref().unwrap().session.history().entries.len()
}

/// Message du bandeau titré `title`.
fn titled(app: &App, title: TextId) -> Option<(Severity, String)> {
    app.messages()
        .iter()
        .find(|m| m.title == Some(title))
        .map(|m| (m.severity, m.text.clone()))
}

#[test]
fn h_ui_39_copy_writes_tsv() {
    let mut harness = p_harness();
    select(&mut harness, 3, 3);
    let text = copy_event(&mut harness, Event::Copy).expect("rien écrit dans le presse-papiers");
    // Colonnes visibles par défaut : ID, Nom, Durée, Début, Fin,
    // Prédécesseurs ; une ligne par tâche, sans en-tête.
    let cells: Vec<&str> = text.split('\t').collect();
    assert_eq!(cells.len(), 6, "{text:?}");
    assert_eq!(&cells[..3], ["3", "Murs", "10j"], "{text:?}");
    assert_eq!(cells[5], "2", "{text:?}");
    assert!(!text.contains('\n'));
    let slot = harness.state().clipboard().unwrap();
    assert_eq!(slot.fingerprint, fingerprint(&text));
    assert_eq!(slot.clip.tasks.len(), 1);
    // Rien d'écrit dans le document.
    assert_eq!(history_len(harness.state()), 0);
    // Une récapitulative entraîne ses descendants ; une colonne masquée
    // disparaît du texte (Prédécesseurs).
    harness.state_mut().dispatch(Action::ToggleColumn(
        plannercraft_ui_egui::column::Column::Predecessors,
    ));
    harness.run();
    select(&mut harness, 1, 1);
    let text = copy_event(&mut harness, Event::Copy).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 4, "{text:?}");
    assert!(lines.iter().all(|l| l.split('\t').count() == 5), "{text:?}");
    assert!(lines[1].starts_with("2\tFondations\t5j\t"), "{text:?}");
}

#[test]
fn h_ui_39_paste_internal_full() {
    // R-244 : copier 3 et 4 (lien 3→4 interne, 2→3 et 3→6 externes),
    // coller avant 6 ⇒ « Murs » et « Livraison » au niveau de 6, lien
    // Murs′→Livraison′, deux liens non recopiés.
    let mut harness = p_harness();
    select(&mut harness, 3, 4);
    let text = copy_event(&mut harness, Event::Copy).unwrap();
    select(&mut harness, 6, 6);
    paste(&mut harness, &text);
    let app = harness.state();
    let got = names(app);
    let tail: Vec<(&str, u16)> = got.iter().skip(4).map(|(n, l)| (n.as_str(), *l)).collect();
    assert_eq!(
        tail,
        [
            ("Second œuvre", 1),
            ("Murs", 2),
            ("Livraison", 2),
            ("Plâtrerie", 2),
            ("Peinture", 2)
        ]
    );
    let project = app.document.as_ref().unwrap().project();
    let murs = project.tasks.iter().nth(5).unwrap();
    let livraison = project.tasks.iter().nth(6).unwrap();
    assert!(murs.uid().get() > 7 && livraison.uid().get() > 7, "UID neufs");
    assert_eq!(
        livraison
            .predecessors
            .iter()
            .map(|l| l.predecessor)
            .collect::<Vec<_>>(),
        [murs.uid()]
    );
    assert!(murs.predecessors.is_empty(), "lien externe 2→3 recréé");
    // Un seul lot ; la première tâche collée devient active.
    assert_eq!(history_len(app), 1);
    assert_eq!(app.selection, Some(murs.uid()));
    // Texte collé avec des fins de ligne Windows : même empreinte.
    let mut harness = p_harness();
    select(&mut harness, 3, 4);
    let text = copy_event(&mut harness, Event::Copy).unwrap();
    harness.state_mut().dispatch(Action::Select(None));
    harness.run();
    paste(&mut harness, &text.replace('\n', "\r\n"));
    let got = plain_names(harness.state());
    assert_eq!(got.len(), 9);
    // Ligne vide sélectionnée : fin du projet, au niveau 1.
    assert_eq!(names(harness.state())[7], ("Murs".to_owned(), 1));
    assert_eq!(names(harness.state())[8], ("Livraison".to_owned(), 1));
}

#[test]
fn h_ui_39_paste_external_names() {
    // R-256 (b), destination de R-261 : une colonne sans tabulation sur la
    // ligne vide (colonne Nom) ⇒ une tâche par ligne non vide, en fin.
    let mut harness = p_harness();
    select(&mut harness, 2, 2);
    harness.state_mut().dispatch(Action::Select(None));
    harness.run();
    paste(&mut harness, "A\nB\n\nC");
    let got = plain_names(harness.state());
    assert_eq!(&got[6..], ["Peinture", "A", "B", "C"]);
    assert_eq!(history_len(harness.state()), 1);
    // R-256 (a) : en-tête reconnu ⇒ lignes de l'import CSV (durées lues).
    select(&mut harness, 5, 5);
    paste(&mut harness, "Nom\tDurée\nCoffrage\t3j\nFerraillage\t2j");
    let app = harness.state();
    let project = app.document.as_ref().unwrap().project();
    let added: Vec<(String, i64)> = project
        .tasks
        .iter()
        .filter(|t| t.name == "Coffrage" || t.name == "Ferraillage")
        .map(|t| (t.name.clone(), t.duration.minutes().get()))
        .collect();
    assert_eq!(
        added,
        [
            ("Coffrage".to_owned(), 3 * 480),
            ("Ferraillage".to_owned(), 2 * 480)
        ]
    );
    assert_eq!(history_len(app), 2);
    // R-261 (amende R-256 c) : plusieurs colonnes sans en-tête sur des
    // lignes entières (les deux lignes importées, restées sélectionnées) ⇒
    // collage de cellules à partir de la cellule active (Nom), un lot.
    paste(&mut harness, "A2\t4j\nB2\t1j");
    let app = harness.state();
    let project = app.document.as_ref().unwrap().project();
    let renamed: Vec<(String, i64)> = project
        .tasks
        .iter()
        .filter(|t| t.name == "A2" || t.name == "B2")
        .map(|t| (t.name.clone(), t.duration.minutes().get()))
        .collect();
    assert_eq!(renamed, [("A2".to_owned(), 4 * 480), ("B2".to_owned(), 480)]);
    assert_eq!(history_len(app), 3);
    let before = plain_names(harness.state());
    // Plafond : plus de 10 000 lignes ⇒ `paste.too_large`, rien écrit.
    let huge = "x\n".repeat(10_001);
    paste(&mut harness, &huge);
    assert_eq!(plain_names(harness.state()), before);
    let (_, text) = titled(harness.state(), TextId::CommandErrorTitle).unwrap();
    let expected = Texts::new(Language::Fr).get_args(
        TextId::ErrorPasteTooLarge,
        &[("detail", Arg::Str(fr(TextId::PasteHintTooLarge).into()))],
    );
    assert_eq!(text, expected);
}

#[test]
fn h_ui_39_cut_two_undo_entries() {
    // Couper = copier (rien écrit) puis suppression dans un lot à part ;
    // le collage est un second lot : deux entrées, annulables une à une.
    let mut harness = p_harness();
    select(&mut harness, 3, 3);
    let text = copy_event(&mut harness, Event::Cut).expect("couper écrit le texte");
    assert!(text.starts_with("3\tMurs\t"));
    assert_eq!(history_len(harness.state()), 1);
    assert!(!plain_names(harness.state()).contains(&"Murs".to_owned()));
    // Après la suppression, la tâche active est celle qui occupe la ligne.
    select(&mut harness, 6, 6);
    paste(&mut harness, &text);
    assert_eq!(history_len(harness.state()), 2);
    let pasted = plain_names(harness.state());
    assert_eq!(
        pasted,
        [
            "Gros œuvre",
            "Fondations",
            "Livraison",
            "Second œuvre",
            "Murs",
            "Plâtrerie",
            "Peinture"
        ]
    );
    harness.state_mut().dispatch(Action::Undo);
    harness.run();
    let after_one = plain_names(harness.state());
    assert_eq!(after_one.len(), 6);
    assert!(!after_one.contains(&"Murs".to_owned()));
    harness.state_mut().dispatch(Action::Undo);
    harness.run();
    let app = harness.state();
    assert_eq!(
        plain_names(app),
        [
            "Gros œuvre",
            "Fondations",
            "Murs",
            "Livraison",
            "Second œuvre",
            "Plâtrerie",
            "Peinture"
        ]
    );
    // L'annulation rend la tâche d'origine (UID 3).
    let project = app.document.as_ref().unwrap().project();
    assert!(project.tasks.position(uid(3)).is_some());
    // Menu Édition → Couper : même effet, sans événement du système.
    select(&mut harness, 7, 7);
    harness.state_mut().dispatch(Action::Clipboard(ClipboardOp::Cut));
    harness.run();
    assert!(!plain_names(harness.state()).contains(&"Peinture".to_owned()));
    assert_eq!(harness.state().clipboard().unwrap().clip.tasks.len(), 1);
}

#[test]
fn h_ui_39_paste_in_editor_goes_to_text() {
    // Saisie ouverte : Ctrl+V appartient à l'éditeur de texte.
    let mut harness = p_harness();
    select(&mut harness, 2, 2);
    harness.key_press(Key::F2);
    harness.run();
    assert!(harness.state().view().edit.editing.is_some());
    paste(&mut harness, "xyz");
    assert_eq!(plain_names(harness.state()).len(), 7, "aucune tâche collée");
    assert_eq!(history_len(harness.state()), 0);
    harness.key_press(Key::Enter);
    harness.run();
    let app = harness.state();
    assert_eq!(plain_names(app)[1], "Fondationsxyz");
    assert_eq!(history_len(app), 1);
    // Copier dans la saisie : la table ne copie pas de lignes.
    select(&mut harness, 2, 2);
    harness.key_press(Key::F2);
    harness.run();
    copy_event(&mut harness, Event::Copy);
    assert!(
        harness.state().clipboard().is_none(),
        "lignes copiées depuis la saisie"
    );
}

#[test]
fn h_ui_39_clip_survives_open() {
    // Le tampon survit à l'ouverture d'un autre document : collage complet
    // dans l'exemple.
    let mut harness = p_harness();
    select(&mut harness, 3, 3);
    let text = copy_event(&mut harness, Event::Copy).unwrap();
    harness
        .state_mut()
        .open_bytes(plannercraft_ui_egui::DEMO_MSPDI, DocOrigin::default());
    harness.run();
    assert_eq!(plain_names(harness.state()).len(), 49);
    assert!(harness.state().clipboard().is_some());
    // Destination de R-261 : la première ligne, entière.
    let first = harness.state().rows().unwrap().row(0).unwrap().uid;
    press_id(&mut harness, first.get(), PressMode::Replace);
    paste(&mut harness, &text);
    let app = harness.state();
    let got = plain_names(app);
    assert_eq!(got.len(), 50);
    assert_eq!(got[0], "Murs", "collé avant la première ligne active");
    // Collage complet (empreinte reconnue) : la durée vient du tampon.
    let project = app.document.as_ref().unwrap().project();
    assert_eq!(
        project.tasks.iter().next().unwrap().duration.minutes().get(),
        10 * 480
    );
}

#[test]
fn paste_report_banner() {
    let mut harness = p_harness();
    select(&mut harness, 3, 4);
    let text = copy_event(&mut harness, Event::Copy).unwrap();
    select(&mut harness, 6, 6);
    paste(&mut harness, &text);
    // H-UI-104 : compte rendu toujours en information, liens non repris
    // dans leur propre phrase (sans « un bout »).
    let texts = Texts::new(Language::Fr);
    let expected = [
        texts.get_args(TextId::PasteReportTasks, &[("count", Arg::Count(2))]),
        texts.get_args(TextId::PasteLinksDropped, &[("count", Arg::Count(2))]),
    ]
    .join(". ");
    let (severity, got) = titled(harness.state(), TextId::PasteReportTitle).unwrap();
    assert_eq!(severity, Severity::Info);
    assert_eq!(got, expected);
    assert!(harness.query_by_label(&fr(TextId::PasteReportTitle)).is_some());
    // Un second collage remplace le compte rendu (un seul « Collage ») :
    // texte à en-tête sur les lignes collées, restées sélectionnées.
    paste(&mut harness, "Nom\nZ");
    let reports: Vec<_> = harness
        .state()
        .messages()
        .iter()
        .filter(|m| m.title == Some(TextId::PasteReportTitle))
        .collect();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].severity, Severity::Info);
    assert_eq!(
        reports[0].text,
        texts.get_args(TextId::PasteReportTasks, &[("count", Arg::Count(1))])
    );
    // « Collage spécial… » sans lien : le lien interne est compté aussi.
    harness
        .state_mut()
        .dispatch(Action::Clipboard(ClipboardOp::PasteSpecial));
    harness.run();
    assert!(harness.state().view().dialog.is_some());
    crate::click(&mut harness, &fr(TextId::PsLinks));
    crate::click(&mut harness, &fr(TextId::TaskInfoOk));
    let app = harness.state();
    assert!(app.view().dialog.is_none());
    let (_, got) = titled(app, TextId::PasteReportTitle).unwrap();
    let expected = [
        texts.get_args(TextId::PasteReportTasks, &[("count", Arg::Count(2))]),
        texts.get_args(TextId::PasteLinksDropped, &[("count", Arg::Count(3))]),
    ]
    .join(". ");
    assert_eq!(got, expected);
    assert_eq!(history_len(app), 3);
}

#[test]
fn paste_special_without_copy_and_menu_entries() {
    let mut harness = p_harness();
    // Sans copie : invitation au bandeau, aucune fenêtre.
    harness
        .state_mut()
        .dispatch(Action::Clipboard(ClipboardOp::PasteSpecial));
    harness.run();
    assert!(harness.state().view().dialog.is_none());
    assert_eq!(
        titled(harness.state(), TextId::PasteReportTitle).map(|(_, t)| t),
        Some(fr(TextId::PsNothing))
    );
    // Menu Édition → Copier : tampon posé sans événement du système.
    select(&mut harness, 2, 2);
    crate::click(&mut harness, &fr(TextId::MenuEdit));
    let label = fr(TextId::MenuCopy);
    crate::click_where(&mut harness, |text| text.starts_with(&label));
    let slot = harness.state().clipboard().unwrap();
    assert_eq!(slot.clip.tasks.len(), 1);
    assert_eq!(slot.clip.tasks[0].fields.name, "Fondations");
}

/// Relecture finale de J8 (F6) : couper puis coller une tâche commencée
/// (60 %) recrée une tâche sans avancement (R-244) ; le compte rendu le
/// dit, en avertissement.
#[test]
fn paste_report_warns_tracking_dropped() {
    let mut app = new_app();
    let mut project = plan_p().project().clone();
    project.tasks.get_mut(uid(2)).unwrap().progress.percent_complete =
        plannercraft_core::Percent::new(6_000).unwrap();
    app.open_project(
        project,
        DocOrigin {
            name: "P".to_owned(),
            path: None,
            demo: false,
        },
    );
    let mut harness = harness_for(app);
    harness.run();
    select(&mut harness, 2, 2);
    let text = copy_event(&mut harness, Event::Cut).expect("couper écrit le texte");
    select(&mut harness, 3, 3);
    paste(&mut harness, &text);
    let texts = Texts::new(Language::Fr);
    let tracking = texts.get_args(TextId::PasteReportTracking, &[("count", Arg::Count(1))]);
    let (severity, got) = titled(harness.state(), TextId::PasteReportTitle).unwrap();
    // H-UI-104 : compte rendu d'un collage réussi toujours en information.
    assert_eq!(severity, Severity::Info);
    assert!(got.contains(&tracking), "{got}");
    // La tâche collée est bien repartie de 0 % (R-244).
    let project = harness.state().document.as_ref().unwrap().project();
    let pasted = project.tasks.iter().find(|t| t.name == "Fondations").unwrap();
    assert_eq!(pasted.progress.percent_complete.get(), 0);
    // Une tâche sans suivi : pas de mention.
    select(&mut harness, 7, 7);
    let text = copy_event(&mut harness, Event::Copy).unwrap();
    paste(&mut harness, &text);
    let (_, got) = titled(harness.state(), TextId::PasteReportTitle).unwrap();
    assert!(!got.contains(&tracking), "{got}");
}
