//! Ouverture des fichiers MS Project `.mpp` et `.mpt` (lot M3, ADR-0038,
//! `docs/features/ui.md` § 1.56, H-UI-230 à H-UI-236).
//!
//! Fichiers synthétiques construits dans le test par l'écrivain des tests
//! de `plannercraft-mpp` (`crate::mpp_common`, conteneur écrit par la crate
//! `cfb`, ADR-0036 D2) : ils tournent toujours. Les fichiers réels du
//! corpus versionné par manifeste (`tests/corpus-mpp/manifest.toml`,
//! `cargo xtask corpus fetch-mpp`) servent quand ils sont présents ; un
//! fichier absent ne compte ni pour ni contre.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::Sender;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_core::{Moment, SchedulingMode, Severity, TaskUid};
use plannercraft_mpp::Layout;
use plannercraft_ui_egui::action::MppOp;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::io::{OPEN_EXTENSIONS, OpenEvent, SaveEvent, SaveRequest, SaveTarget};
use plannercraft_ui_egui::{Action, App, DocOrigin, Env, FileService, LanguagePref, ThemePref, UiState};

use crate::mpp_common::{Mpp, T, stamp};
use crate::{fr, fr_light, harness_for, now};

/// Planning de chantier synthétique (Project 2013 et suivants), projet au
/// lun 05/01/2026 :
/// 1 Gros œuvre (récapitulative) { 2 Terrassement 2 j ; 3 Fondations 3 j
/// FD 2 ; 4 Murs 3 j FD 3, enregistrée trois jours plus tard (nivellement
/// dans MS Project) } ; 5 Charpente 2 j FD 4 ; 6 Réunion de chantier 1 j
/// sans lien, enregistrée le mercredi ; 7 Électricité 1 j, une minute plus
/// tard (arrondi) ; 8 Réception, jalon FD 5.
/// Écarts réels : 1, 4, 5, 6, 8 ; arrondi : 7.
pub fn chantier() -> Vec<u8> {
    let mut m = Mpp::new(Layout::Project2013);
    let task = |uid: u32, name: &'static str, level: u16, days: i32, start: &str, finish: &str| {
        let mut t = T::new(uid, uid, name);
        t.level = level;
        t.duration = days.saturating_mul(4_800);
        t.start = stamp(start);
        t.finish = stamp(finish);
        t
    };
    let mut reception = task(8, "Réception", 1, 0, "2026-01-20T17:00", "2026-01-20T17:00");
    reception.milestone = true;
    m.tasks.extend([
        task(1, "Gros œuvre", 1, 10, "2026-01-05T08:00", "2026-01-16T17:00"),
        task(2, "Terrassement", 2, 2, "2026-01-05T08:00", "2026-01-06T17:00"),
        task(3, "Fondations", 2, 3, "2026-01-07T08:00", "2026-01-09T17:00"),
        task(4, "Murs", 2, 3, "2026-01-14T08:00", "2026-01-16T17:00"),
        task(5, "Charpente", 1, 2, "2026-01-19T08:00", "2026-01-20T17:00"),
        task(
            6,
            "Réunion de chantier",
            1,
            1,
            "2026-01-07T08:00",
            "2026-01-07T17:00",
        ),
        task(7, "Électricité", 1, 1, "2026-01-05T08:01", "2026-01-05T17:01"),
        reception,
    ]);
    m.links = vec![(2, 3, 1, 0, 7), (3, 4, 1, 0, 7), (4, 5, 1, 0, 7), (5, 8, 1, 0, 7)];
    m.bytes()
}

/// Même planning, sans écart : tout est enregistré comme le moteur le
/// calcule (Électricité garde son arrondi).
fn chantier_sans_ecart() -> Vec<u8> {
    let mut m = Mpp::new(Layout::Project2013);
    let mut a = T::new(1, 1, "Terrassement");
    a.duration = 9_600;
    a.finish = stamp("2026-01-06T17:00");
    let mut b = T::new(2, 2, "Électricité");
    b.start = stamp("2026-01-05T08:01");
    b.finish = stamp("2026-01-05T17:01");
    m.tasks.extend([a, b]);
    m.bytes()
}

/// Fichier enregistré par Project 2007 (dossier « `   112` »).
fn project_2007() -> Vec<u8> {
    let mut m = Mpp::new(Layout::Project2013);
    m.dir = "   112";
    m.bytes()
}

/// Dossier temporaire du test `name`, vide.
fn temp_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("mpp-open")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Écrit `bytes` dans `dir/name` et rend le chemin.
fn write(dir: &std::path::Path, name: &str, bytes: &[u8]) -> String {
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path.to_str().unwrap().to_owned()
}

/// Demandes reçues par le service simulé : titre et filtre du sélecteur,
/// enregistrements.
#[derive(Default)]
struct Seen {
    picker: Vec<(String, String)>,
    saves: Vec<SaveRequest>,
}

/// Service de fichiers simulé : « Ouvrir » rend le fichier `name` lu par
/// le même chargement que le sélecteur réel (`document::load_named`) ;
/// « Enregistrer » réussit dans `/projets/<nom proposé>`.
struct Files {
    file: Option<(String, Vec<u8>)>,
    seen: Rc<RefCell<Seen>>,
}

impl FileService for Files {
    fn pick_and_open(
        &mut self,
        ctx: &egui::Context,
        now: Moment,
        title: String,
        filter: String,
        tx: Sender<OpenEvent>,
    ) {
        self.seen.borrow_mut().picker.push((title, filter));
        let event = match self.file.take() {
            None => OpenEvent::Cancelled,
            Some((name, bytes)) => OpenEvent::Opened {
                origin: DocOrigin {
                    name: name.clone(),
                    path: None,
                    demo: false,
                },
                result: plannercraft_ui_egui::document::load_named(&bytes, &name, now),
            },
        };
        tx.send(event).unwrap();
        ctx.request_repaint();
    }

    fn save(&mut self, ctx: &egui::Context, request: SaveRequest, tx: Sender<SaveEvent>) {
        let name = match &request.target {
            SaveTarget::Path(path) => path.clone(),
            SaveTarget::Dialog { suggested_name, .. } | SaveTarget::File { suggested_name, .. } => {
                suggested_name.clone()
            }
        };
        self.seen.borrow_mut().saves.push(request);
        tx.send(SaveEvent::Saved {
            path: Some(format!("/projets/{name}")),
            name,
        })
        .unwrap();
        ctx.request_repaint();
    }
}

/// Harnais dont « Ouvrir » rend `file` (nom, octets).
fn harness_with_file(file: Option<(&str, Vec<u8>)>) -> (Harness<'static, App>, Rc<RefCell<Seen>>) {
    let seen = Rc::new(RefCell::new(Seen::default()));
    let files = Files {
        file: file.map(|(name, bytes)| (name.to_owned(), bytes)),
        seen: Rc::clone(&seen),
    };
    let env = Env {
        files: Box::new(files),
        ..Env::fixed(now())
    };
    (harness_for(App::new(fr_light(), env)), seen)
}

/// Harnais ouvert sur le planning de chantier par « Ouvrir ».
fn opened() -> Harness<'static, App> {
    let (mut harness, _) = harness_with_file(Some(("Chantier B.mpp", chantier())));
    harness.state_mut().dispatch(Action::OpenDialog);
    harness.run();
    assert!(harness.state().document.is_some(), "planning ouvert");
    harness
}

/// Le compte rendu MS Project du document ouvert.
fn mpp(app: &App) -> &plannercraft_ui_egui::mpp_open::MppOpened {
    app.document.as_ref().unwrap().mpp.as_deref().unwrap()
}

/// Un texte affiché ailleurs que dans le ruban, tel quel.
fn shown(harness: &Harness<'static, App>, text: &str) -> bool {
    harness.query_all_by_label(text).next().is_some()
}

fn texts() -> Texts {
    Texts::new(Language::Fr)
}

fn banner_first_line() -> String {
    let t = texts();
    let tasks = t.get_args(TextId::MppCountTasks, &[("count", Arg::Count(8))]);
    let links = t.get_args(TextId::MppCountLinks, &[("count", Arg::Count(4))]);
    t.get_args(
        TextId::MppBannerOpened,
        &[
            ("tasks", Arg::Str(tasks.into())),
            ("links", Arg::Str(links.into())),
        ],
    )
}

fn gaps_line(n: i64) -> String {
    texts().get_args(TextId::MppGaps, &[("count", Arg::Count(n))])
}

/// H-UI-230 : « Ouvrir » propose `.mpp`, `.mpt` et `.xml` (filtre, titre,
/// libellés du menu et de l'accueil) ; un fichier MS Project choisi s'ouvre
/// tout de suite, sans fenêtre intermédiaire.
#[test]
fn h_ui_230_open_offers_project_files_and_opens_at_once() {
    assert_eq!(OPEN_EXTENSIONS, ["xml", "mpp", "mpt"]);
    for language in [Language::Fr, Language::En] {
        let t = Texts::new(language);
        for id in [TextId::MenuOpen, TextId::WelcomeOpenFile, TextId::FileFilterOpen] {
            let text = t.get(id);
            assert!(text.contains(".mpp") && text.contains(".xml"), "{text}");
        }
    }
    let (mut harness, seen) = harness_with_file(Some(("Chantier B.mpp", chantier())));
    harness.run();
    crate::click(&mut harness, &fr(TextId::WelcomeOpenFile));
    assert_eq!(
        seen.borrow().picker,
        [(fr(TextId::DialogOpenTitle), fr(TextId::FileFilterOpen))]
    );
    let app = harness.state();
    let document = app.document.as_ref().unwrap();
    assert_eq!(document.project().tasks.len(), 8);
    assert!(app.view().dialog.is_none());
    assert!(!app.messages().iter().any(|m| m.severity == Severity::Error));
    assert!(shown(&harness, &banner_first_line()));
}

/// H-UI-230 : le type se reconnaît au contenu, pas à l'extension : un
/// `.mpp` sans extension s'ouvre comme un fichier MS Project, un XML nommé
/// `.mpp` comme un XML ; un fichier déposé suit le même chemin.
#[test]
fn h_ui_230_type_recognised_by_content() {
    let mut app = App::new(fr_light(), Env::fixed(now()));
    app.open_bytes(
        &chantier(),
        DocOrigin {
            name: "planning".to_owned(),
            ..DocOrigin::default()
        },
    );
    assert!(app.document.as_ref().unwrap().mpp.is_some());
    app.open_bytes(
        plannercraft_ui_egui::DEMO_MSPDI,
        DocOrigin {
            name: "maison.mpp".to_owned(),
            ..DocOrigin::default()
        },
    );
    let document = app.document.as_ref().unwrap();
    assert!(document.mpp.is_none());
    assert_eq!(document.project().tasks.len(), 49);
}

/// H-UI-230 : l'ouverture par le chemin réel de l'application (argument de
/// la ligne de commande, fichier récent, fichier déposé du desktop : fichier
/// lu sur le disque, borné, puis reconnu).
#[test]
fn h_ui_230_open_path_reads_the_file_on_disk() {
    let dir = temp_dir("open-path");
    let path = write(&dir, "Chantier B.mpp", &chantier());
    let mut app = App::new(fr_light(), Env::fixed(now()));
    app.open_path(&path);
    let document = app.document.as_ref().unwrap();
    assert_eq!(document.origin.name, "Chantier B");
    assert_eq!(document.origin.path, None, "jamais lié au .mpp");
    assert!(
        app.state.recent.is_empty(),
        "un .mpp n'entre pas aux fichiers récents"
    );
    assert_eq!(mpp(&app).real_gaps(), 5);
}

/// H-UI-231 : le planning ouvert est un nouveau planning PlannerCraft,
/// comme un import PDF ou CSV : nommé d'après le fichier, non modifié,
/// sans chemin ; « Enregistrer » demande où écrire, en XML, jamais le
/// `.mpp` d'origine. Un gabarit `.mpt` s'ouvre comme un nouveau planning
/// sans titre de fichier (« Projet1 »).
#[test]
fn h_ui_231_opened_as_a_new_plan_saved_as_xml() {
    let (mut harness, seen) = harness_with_file(Some(("Chantier B.mpp", chantier())));
    harness.state_mut().dispatch(Action::OpenDialog);
    harness.run();
    let document = harness.state().document.as_ref().unwrap();
    assert_eq!(document.origin.name, "Chantier B");
    assert!(document.origin.path.is_none() && document.web_file.is_none() && !document.is_dirty());
    assert!(harness.state().window_title().starts_with("Chantier B \u{2014}"));
    harness.state_mut().dispatch(Action::Save);
    harness.run();
    let saves = &seen.borrow().saves;
    assert_eq!(saves.len(), 1);
    assert_eq!(saves[0].kind, plannercraft_ui_egui::io::SaveKind::Mspdi);
    match &saves[0].target {
        SaveTarget::Dialog { suggested_name, .. } => assert_eq!(suggested_name, "Chantier B.xml"),
        other => panic!("boîte « Enregistrer sous » attendue, pas {other:?}"),
    }
    assert!(saves[0].bytes.starts_with(b"<?xml"), "MSPDI écrit");

    let (mut harness, _) = harness_with_file(Some(("Modèle chantier.MPT", chantier())));
    harness.state_mut().dispatch(Action::OpenDialog);
    harness.run();
    let app = harness.state();
    assert_eq!(
        app.document.as_ref().unwrap().origin.name,
        fr(TextId::NewProjectName)
    );
    assert!(mpp(app).template);
    let t = texts();
    let tasks = t.get_args(TextId::MppCountTasks, &[("count", Arg::Count(8))]);
    let links = t.get_args(TextId::MppCountLinks, &[("count", Arg::Count(4))]);
    let line = t.get_args(
        TextId::MppBannerOpenedTemplate,
        &[
            ("tasks", Arg::Str(tasks.into())),
            ("links", Arg::Str(links.into())),
        ],
    );
    assert!(shown(&harness, &line), "{line}");
}

/// H-UI-232 : bandeau non bloquant : ce qui a été ouvert, puis, seulement
/// s'il y a lieu, les tâches en écart réel (l'arrondi n'est pas compté) et
/// « Voir » ; « Masquer » le ferme pour de bon.
#[test]
fn h_ui_232_banner_says_what_was_opened_and_what_differs() {
    let mut harness = opened();
    assert!(shown(&harness, &banner_first_line()));
    assert!(shown(&harness, &gaps_line(5)));
    assert!(shown(&harness, &fr(TextId::MppBannerShow)));
    // Rien de non repris constaté sur ce fichier : pas de phrase.
    let not_taken = fr(TextId::MppBannerNotTaken);
    let prefix = not_taken
        .split('{')
        .next()
        .unwrap_or_default()
        .trim_end()
        .to_owned();
    assert!(
        !harness
            .query_all_by(|n| n
                .label()
                .is_some_and(|l| l.starts_with(prefix.trim_end_matches(':').trim_end())))
            .any(|_| true),
        "pas de « Non repris » inventé"
    );
    crate::click(&mut harness, &fr(TextId::MppBannerDismiss));
    assert!(!shown(&harness, &banner_first_line()));
    // « Non repris » seulement quand c'est vrai pour ce fichier : une
    // affectation à répartition modifiée à la main.
    let mut m = Mpp::new(Layout::Project2013);
    m.tasks.push(T::new(1, 1, "Coffrage"));
    let mut a = crate::mpp_common::A::new(10, 1, -65_535, 10_000.0, 480.0);
    a.contoured = true;
    m.assignments = vec![a];
    let mut app = App::new(fr_light(), Env::fixed(now()));
    app.open_bytes(
        &m.bytes(),
        DocOrigin {
            name: "Coffrage.mpp".to_owned(),
            ..DocOrigin::default()
        },
    );
    let not_taken = mpp(&app).not_taken;
    assert_eq!(not_taken.contoured, 1);
    let expected = texts().get_args(
        TextId::MppBannerNotTaken,
        &[("items", Arg::Str(fr(TextId::MppNotContoured).into()))],
    );
    let mut with_not_taken = harness_for(app);
    with_not_taken.run();
    assert!(shown(&with_not_taken, &expected), "{expected}");
    // Ne revient pas : ni après une modification annulée, ni après un
    // enregistrement.
    harness.state_mut().dispatch(Action::Exec(rename(2, "Décapage")));
    harness.run();
    harness.state_mut().dispatch(Action::Undo);
    harness.run();
    assert!(!shown(&harness, &banner_first_line()));

    // Sans écart réel (une minute d'arrondi seulement) : la première phrase
    // seule.
    let (mut harness, _) = harness_with_file(Some(("Sans écart.mpp", chantier_sans_ecart())));
    harness.state_mut().dispatch(Action::OpenDialog);
    harness.run();
    assert_eq!(mpp(harness.state()).real_gaps(), 0);
    assert_eq!(mpp(harness.state()).rounding(), 1);
    assert!(!shown(&harness, &fr(TextId::MppBannerShow)));
    assert!(!file_menu_has_differences(&mut harness));
}

/// Le menu Fichier montre-t-il « Différences avec MS Project… » ? Menu
/// refermé ensuite.
fn file_menu_has_differences(harness: &mut Harness<'static, App>) -> bool {
    crate::click(harness, &fr(TextId::MenuFile));
    let found = shown(harness, &fr(TextId::MenuMppDifferences));
    close_menus(harness);
    found
}

/// Referme les menus ouverts.
fn close_menus(harness: &mut Harness<'static, App>) {
    for _ in 0..4 {
        if !egui::Popup::is_any_open(&harness.ctx) {
            break;
        }
        harness.key_press(egui::Key::Escape);
        harness.run();
    }
}

/// Lot qui renomme la tâche `uid`.
fn rename(uid: u32, name: &str) -> plannercraft_automation::Batch {
    plannercraft_automation::Batch::from_json(serde_json::json!({
        "format_version": 1,
        "ops": [{ "op": "task.update", "uid": uid, "name": name }]
    }))
    .unwrap()
}

/// H-UI-233 : « Voir » ouvre la fenêtre « Différences avec MS Project » ;
/// elle se rouvre par Fichier → « Différences avec MS Project… » tant que le
/// planning n'a pas été modifié autrement ; l'entrée est absente (pas
/// grisée) sinon.
#[test]
fn h_ui_233_differences_window_and_menu_entry() {
    let mut harness = opened();
    crate::click(&mut harness, &fr(TextId::MppBannerShow));
    assert!(shown(&harness, &fr(TextId::MppDiffTitle)));
    crate::click(&mut harness, &fr(TextId::MppDiffClose));
    assert!(!shown(&harness, &fr(TextId::MppDiffTitle)));
    assert!(file_menu_has_differences(&mut harness));
    crate::click(&mut harness, &fr(TextId::MenuFile));
    crate::click(&mut harness, &fr(TextId::MenuMppDifferences));
    assert!(shown(&harness, &fr(TextId::MppDiffTitle)));
    // Une modification d'une autre nature : fenêtre refermée, entrée
    // retirée, phrase des écarts retirée du bandeau.
    harness.state_mut().dispatch(Action::Exec(rename(2, "Décapage")));
    harness.run();
    assert!(!shown(&harness, &fr(TextId::MppDiffTitle)));
    assert!(!file_menu_has_differences(&mut harness));
    assert!(!shown(&harness, &gaps_line(5)));
    assert!(shown(&harness, &banner_first_line()));
    // Annulée : le planning est celui du fichier, tout revient.
    harness.state_mut().dispatch(Action::Undo);
    harness.run();
    assert!(file_menu_has_differences(&mut harness));
    assert!(shown(&harness, &gaps_line(5)));
}

/// H-UI-234 : la fenêtre liste les tâches en écart réel dans l'ordre du
/// planning, avec les dates de MS Project, les dates calculées et la
/// cause en clair ; « Garder les dates de MS Project » les passe en
/// planification manuelle aux dates du fichier, en un lot annulable, et la
/// fenêtre montre qu'il ne reste plus d'écart.
#[test]
fn h_ui_234_list_then_keep_the_ms_project_dates() {
    let mut harness = opened();
    harness.state_mut().dispatch(Action::Mpp(MppOp::ShowDifferences));
    harness.run();
    let app = harness.state();
    let document = app.document.as_ref().unwrap();
    let rows = mpp(app).rows(&document.session);
    let ids: Vec<u32> = rows.iter().map(|r| r.id).collect();
    assert_eq!(ids, [1, 4, 5, 6, 8]);
    assert!(rows[0].summary && !rows[1].summary);
    assert_eq!(rows[1].recorded.0, Some("2026-01-14T08:00".parse().unwrap()));
    assert_eq!(rows[1].computed.0, Some("2026-01-12T08:00".parse().unwrap()));
    for name in [
        "Gros œuvre",
        "Murs",
        "Charpente",
        "Réunion de chantier",
        "Réception",
    ] {
        let label = texts().get_args(
            TextId::MppDiffRow,
            &[
                (
                    "id",
                    Arg::Count(i64::from(rows.iter().find(|r| r.name == name).unwrap().id)),
                ),
                ("name", Arg::Str(name.into())),
            ],
        );
        assert!(shown(&harness, &label), "{label}");
    }
    // Réunion sans lien, enregistrée le mercredi : cause en clair.
    assert!(shown(&harness, &fr(TextId::MppCauseUnexplainedStart)));
    // L'arrondi d'Électricité est dit, pas listé.
    assert!(shown(
        &harness,
        &texts().get_args(TextId::MppDiffRounding, &[("count", Arg::Count(1))])
    ));
    let hint = texts().get_args(TextId::MppDiffKeepHint, &[("undo", Arg::Str("Ctrl+Z".into()))]);
    assert!(shown(&harness, &hint), "{hint}");
    let before = harness.state().document.as_ref().unwrap().project().clone();
    let generation = harness.state().document.as_ref().unwrap().generation();

    crate::click(&mut harness, &fr(TextId::MppDiffKeep));
    let app = harness.state();
    let document = app.document.as_ref().unwrap();
    assert_eq!(document.generation(), generation + 1, "un seul lot");
    assert!(document.is_dirty());
    let murs = document.project().tasks.get(TaskUid::new(4)).unwrap();
    assert!(matches!(
        murs.mode,
        SchedulingMode::Manual { start: Some(s), finish: Some(f), .. }
            if s == "2026-01-14T08:00".parse().unwrap() && f == "2026-01-16T17:00".parse().unwrap()
    ));
    let gros_oeuvre = document.schedule().scheduled(TaskUid::new(1)).unwrap();
    assert_eq!(gros_oeuvre.finish, "2026-01-16T17:00".parse().unwrap());
    assert_eq!(mpp(app).real_gaps(), 0);
    assert!(shown(&harness, &fr(TextId::MppDiffNone)));
    assert!(shown(
        &harness,
        &texts().get_args(TextId::MppDiffKept, &[("undo", Arg::Str("Ctrl+Z".into()))])
    ));
    assert!(!shown(&harness, &fr(TextId::MppDiffKeep)));
    assert!(!shown(&harness, &gaps_line(5)), "le bandeau suit");
    // La fenêtre se rouvre après l'action (planning laissé par elle).
    assert!(mpp(harness.state()).differences_available());
    // Une seule entrée d'annulation.
    harness.state_mut().dispatch(Action::Undo);
    harness.run();
    assert_eq!(harness.state().document.as_ref().unwrap().project(), &before);
    assert_eq!(mpp(harness.state()).real_gaps(), 5);
    assert!(shown(&harness, &fr(TextId::MppDiffKeep)));
}

/// H-UI-235 : un clic sur une ligne sélectionne sa tâche dans le planning.
#[test]
fn h_ui_235_click_row_selects_the_task() {
    let mut harness = opened();
    harness.state_mut().dispatch(Action::Mpp(MppOp::ShowDifferences));
    harness.run();
    let label = texts().get_args(
        TextId::MppDiffRow,
        &[("id", Arg::Count(5)), ("name", Arg::Str("Charpente".into()))],
    );
    harness.get_by_label(&label).click();
    harness.run();
    assert_eq!(harness.state().selection, Some(TaskUid::new(5)));
    assert!(
        shown(&harness, &fr(TextId::MppDiffTitle)),
        "la fenêtre reste ouverte"
    );
}

/// H-UI-236 : refus clair dans la forme des erreurs d'ouverture : ce que
/// c'est et quoi faire ; jamais de planning vide, le document courant
/// reste.
#[test]
fn h_ui_236_refusals_are_clear_and_keep_the_current_plan() {
    let error_with = |app: &App, needle: &str| {
        app.messages()
            .iter()
            .any(|m| m.severity == Severity::Error && m.text.contains(needle))
    };
    let (mut harness, _) = harness_with_file(Some(("Ancien.mpp", project_2007())));
    harness.state_mut().dispatch(Action::OpenDialog);
    harness.run();
    assert!(harness.state().document.is_none(), "jamais de planning vide");
    let expected = texts().get_args(
        TextId::OpenErrorMppFile,
        &[
            ("name", Arg::Str("Ancien.mpp".into())),
            ("detail", Arg::Str(fr(TextId::MppRefusedProject2007).into())),
        ],
    );
    assert!(
        harness.state().messages().iter().any(|m| m.text == expected),
        "{expected}"
    );
    assert!(shown(&harness, &fr(TextId::OpenErrorTitle)));

    let mut app = App::new(fr_light(), Env::fixed(now()));
    app.open_demo();
    for (name, bytes, text) in [
        (
            "faux.mpp",
            b"PK\x03\x04 archive".to_vec(),
            TextId::MppRefusedNotProjectFile,
        ),
        (
            "lettre.doc",
            crate::mpp_common::container(&[("/WordDocument".to_owned(), vec![1, 2, 3])]),
            TextId::MppRefusedOtherDocument,
        ),
        (
            "abime.mpp",
            chantier()[..2_000].to_vec(),
            TextId::MppRefusedUnreadable,
        ),
    ] {
        app.open_bytes(
            &bytes,
            DocOrigin {
                name: name.to_owned(),
                ..DocOrigin::default()
            },
        );
        assert!(error_with(&app, &fr(text)), "{name}");
        assert_eq!(app.document.as_ref().unwrap().project().tasks.len(), 49, "{name}");
    }
}

/// H-UI-232 et H-UI-234, en anglais : mêmes clés, aucun texte manquant.
#[test]
fn h_ui_232_english_texts() {
    let state = UiState {
        language: LanguagePref::En,
        ..fr_light()
    };
    let (mut harness, _) = {
        let seen = Rc::new(RefCell::new(Seen::default()));
        let files = Files {
            file: Some(("Site B.mpp".to_owned(), chantier())),
            seen: Rc::clone(&seen),
        };
        let env = Env {
            files: Box::new(files),
            ..Env::fixed(now())
        };
        (harness_for(App::new(state, env)), seen)
    };
    harness.state_mut().dispatch(Action::OpenDialog);
    harness.run();
    let en = Texts::new(Language::En);
    assert!(shown(
        &harness,
        &en.get_args(TextId::MppGaps, &[("count", Arg::Count(5))])
    ));
    harness.state_mut().dispatch(Action::Mpp(MppOp::ShowDifferences));
    harness.run();
    assert!(shown(&harness, &en.get(TextId::MppDiffKeep)));
}

/// H-UI-230 : fichiers du corpus versionné (s'ils sont téléchargés), tous :
/// ouverts par le chemin de l'application (lus sur le disque), ou refusés
/// par un message d'erreur sans planning vide ; jamais liés au fichier ;
/// gabarit reconnu ; « Garder les dates de MS Project » sans écart réel
/// restant hors récapitulatives.
#[test]
fn h_ui_230_corpus_files_open() {
    // `PLANNERCRAFT_MPP_OPEN_DIR` : un autre dossier de fichiers (mesure
    // locale, fichiers hors dépôt).
    let dir = std::env::var_os("PLANNERCRAFT_MPP_OPEN_DIR").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus-mpp/files"),
        PathBuf::from,
    );
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("corpus .mpp absent : cargo xtask corpus fetch-mpp");
        return;
    };
    let mut paths: Vec<PathBuf> = entries.map(|e| e.unwrap().path()).collect();
    paths.sort();
    let (mut opened_files, mut refused, mut kept_files) = (0, 0, 0);
    for path in &paths {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let mut app = App::new(fr_light(), Env::fixed(now()));
        app.open_path(path.to_str().unwrap());
        let Some(document) = app.document.as_ref() else {
            assert!(
                app.messages().iter().any(|m| m.severity == Severity::Error),
                "{name} : ni planning ni message"
            );
            refused += 1;
            continue;
        };
        opened_files += 1;
        assert!(document.origin.path.is_none(), "{name}");
        let opened = mpp(&app);
        assert_eq!(
            opened.template,
            plannercraft_ui_egui::mpp_open::is_template_name(&name),
            "{name}"
        );
        if opened.real_gaps() > 0 {
            kept_files += 1;
            // Même lot que le bouton (`App::keep_mpp_dates`), sans dessin :
            // des noms de tâches de ce corpus n'ont pas de glyphe dans les
            // polices du harnais.
            let document = app.document.as_ref().unwrap();
            let batch = mpp(&app).keep_batch(&document.session).unwrap().unwrap();
            app.execute(&batch, plannercraft_ui_egui::EditOrigin::User)
                .unwrap_or_else(|e| panic!("{name} : lot refusé : {e:?}"));
            let document = app.document.as_mut().unwrap();
            document.mpp.as_deref_mut().unwrap().mark_kept(&document.session);
            let app = &app;
            let document = app.document.as_ref().unwrap();
            let rows = mpp(app).rows(&document.session);
            assert!(rows.iter().all(|r| r.summary), "{name} : reste {rows:?}");
        }
    }
    eprintln!(
        "corpus .mpp : {opened_files} ouverts, {refused} refusés, {kept_files} avec « Garder les dates »"
    );
}

/// Captures du bandeau et de la fenêtre, en clair et en sombre (français).
fn snapshot_app(theme: ThemePref) -> Harness<'static, App> {
    let mut app = crate::common::app(LanguagePref::Fr, theme);
    app.open_bytes(
        &chantier(),
        DocOrigin {
            name: "Chantier B.mpp".to_owned(),
            ..DocOrigin::default()
        },
    );
    crate::common::harness_with(app, theme)
}

/// Capture du bandeau (H-UI-232).
fn snapshot_banner(theme: ThemePref, name: &str) {
    let mut harness = snapshot_app(theme);
    harness.run();
    crate::common::snapshot(&mut harness, name);
}

/// Capture de la fenêtre des différences (H-UI-233, H-UI-234).
fn snapshot_differences(theme: ThemePref, name: &str) {
    let mut harness = snapshot_app(theme);
    harness.state_mut().dispatch(Action::Mpp(MppOp::ShowDifferences));
    harness.run();
    crate::common::snapshot(&mut harness, name);
}

#[test]
fn h_ui_232_snapshot_banner_light() {
    snapshot_banner(ThemePref::Light, "mpp_banner_fr_light");
}

#[test]
fn h_ui_232_snapshot_banner_dark() {
    snapshot_banner(ThemePref::Dark, "mpp_banner_fr_dark");
}

#[test]
fn h_ui_233_snapshot_differences_light() {
    snapshot_differences(ThemePref::Light, "mpp_differences_fr_light");
}

#[test]
fn h_ui_233_snapshot_differences_dark() {
    snapshot_differences(ThemePref::Dark, "mpp_differences_fr_dark");
}
