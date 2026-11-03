//! Scénario de bout en bout de J8 (lot L16, vérification 5 de
//! `docs/plans/j8.md`) : un seul test, `e2e_daily_j8`, qui enchaîne sur le
//! projet d'exemple « P » du cahier (§ 2 : 1 « Gros œuvre » ⊃ 2, 3, 4 ;
//! 5 « Second œuvre » ⊃ 6, 7 ; UID = ID) les gestes livrés par les lots L2
//! à L15 — filtre, tri, repli, recherche, couper/copier/coller, % en un
//! clic, bascules du mode et de l'activité, surbrillance critique, import
//! CSV, galerie, mise en page et export PDF — puis annule tout (projet égal
//! à l'ouverture), rétablit tout, enregistre par un `FileService` en
//! mémoire et relit les octets écrits (mêmes tâches, même style).
//!
//! Modèle par défaut : Sobre (défaut de l'application depuis L15). Les
//! menus et boutons sont cliqués par leur nom accessible ; les touches sont
//! de vrais événements egui ; les commandes de la vue sans bouton propre
//! (niveau « Tous ») passent par le réducteur, comme le menu. Aucune date
//! n'est calculée ici : les attendus de dates viennent du moteur (R-254 :
//! fin d'une tâche manuelle) ou de l'exemple du cahier.

#![allow(
    clippy::arithmetic_side_effects,
    reason = "tests : compteurs bornés par le scénario"
)]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::mpsc::Sender;

use egui::{Event, Key, Modifiers, PointerButton, Pos2, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_automation::command::style::doc_style;
use plannercraft_core::{Moment, Project, SchedulingMode, Severity, TaskUid};
use plannercraft_render::style::{Fit, Paper, TemplateId};
use plannercraft_tests::daily::plan_p;
use plannercraft_ui_egui::action::DialogKind;
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::dialog::DialogBody;
use plannercraft_ui_egui::dialog::page_setup::build_pages;
use plannercraft_ui_egui::document;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts, template_name};
use plannercraft_ui_egui::io::{OpenEvent, PickRequest, PickedFile, SaveEvent, SaveKind, SaveRequest};
use plannercraft_ui_egui::selection::SelectMode;
use plannercraft_ui_egui::state::{SortKey, SortSpec};
use plannercraft_ui_egui::view::task_view::TaskViewOp;
use plannercraft_ui_egui::{Action, App, DocOrigin, Env, FileService, LanguagePref, ThemePref, UiState};

use crate::filters::{shown, view_op};
use plannercraft_ui_egui::ribbon::RibbonTab;

use crate::{click, fr, harness_for, now, ribbon_click};

/// Demandes d'enregistrement reçues par le service en mémoire.
type Saved = Rc<RefCell<Vec<SaveRequest>>>;

/// Service de fichiers en mémoire : rend les CSV du script au sélecteur
/// d'import, garde les octets de chaque enregistrement ou export.
struct MemFiles {
    picks: VecDeque<PickedFile>,
    saved: Saved,
}

impl FileService for MemFiles {
    fn pick_and_open(&mut self, ctx: &egui::Context, _: Moment, _: String, _: String, tx: Sender<OpenEvent>) {
        tx.send(OpenEvent::Cancelled).unwrap();
        ctx.request_repaint();
    }

    fn save(&mut self, ctx: &egui::Context, request: SaveRequest, tx: Sender<SaveEvent>) {
        self.saved.borrow_mut().push(request);
        tx.send(SaveEvent::Saved {
            name: "p.xml".to_owned(),
            path: Some("/projets/p.xml".into()),
        })
        .unwrap();
        ctx.request_repaint();
    }

    fn pick_bytes(&mut self, ctx: &egui::Context, _: PickRequest, tx: Sender<PickedFile>) {
        tx.send(self.picks.pop_front().unwrap_or(PickedFile::Cancelled))
            .unwrap();
        ctx.request_repaint();
    }
}

/// CSV importé au pas « Importer CSV » (deux tâches, en-têtes français).
const CSV: &[u8] = "Nom;Durée\nCoffrage;3j\nFerraillage;2j\n".as_bytes();

const fn uid(n: u32) -> TaskUid {
    TaskUid::new(n)
}

fn project(app: &App) -> &Project {
    app.document.as_ref().unwrap().project()
}

/// (entrées annulables, entrées rétablissables).
fn history(app: &App) -> (usize, usize) {
    let h = app.document.as_ref().unwrap().session.history();
    (h.entries.len(), h.redo.len())
}

/// Génération du document (une écriture l'incrémente).
fn generation(app: &App) -> u64 {
    app.document.as_ref().unwrap().generation()
}

/// Noms des tâches dans l'ordre du projet.
fn names(app: &App) -> Vec<String> {
    project(app).tasks.iter().map(|t| t.name.clone()).collect()
}

/// UID de la tâche nommée `name` (unique).
fn uid_of(app: &App, name: &str) -> TaskUid {
    let mut found = project(app).tasks.iter().filter(|t| t.name == name);
    let task = found.next().unwrap_or_else(|| panic!("« {name} » absente"));
    assert!(found.next().is_none(), "« {name} » en double");
    task.uid()
}

/// % achevé (centièmes) de la tâche `n`.
fn percent(app: &App, n: TaskUid) -> i64 {
    project(app).tasks.get(n).unwrap().progress.percent_complete.get()
}

/// Sélectionne les tâches `first` à `last` (plage dans l'ordre des lignes).
fn select(harness: &mut Harness<'static, App>, first: TaskUid, last: TaskUid) {
    harness.state_mut().dispatch(Action::Select(Some(first)));
    harness.run();
    if last != first {
        harness.state_mut().dispatch(Action::SelectWith {
            uid: last,
            mode: SelectMode::Range,
        });
        harness.run();
    }
}

/// Sélectionne les lignes entières `first` à `last` (appui sur l'ID puis
/// Maj+appui) : couper, copier et coller de lignes suivent les lignes
/// entières depuis la passe UX (H-UI-95, ADR-0018 D3).
fn select_rows(harness: &mut Harness<'static, App>, first: TaskUid, last: TaskUid) {
    for (n, mode) in [
        (first, plannercraft_ui_egui::action::PressMode::Replace),
        (last, plannercraft_ui_egui::action::PressMode::Extend),
    ] {
        harness
            .state_mut()
            .dispatch(Action::SelectOp(plannercraft_ui_egui::action::SelectOp::Press {
                row: plannercraft_ui_egui::nav::RowRef::Item(n),
                column: Column::Name,
                target: plannercraft_ui_egui::action::PressTarget::RowHeader,
                mode,
            }));
        harness.run();
    }
}

/// Envoie `event` (copier ou couper) et rend le texte écrit dans le
/// presse-papiers par cette image.
fn clip_event(harness: &mut Harness<'static, App>, event: Event) -> String {
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
    copied.expect("rien écrit dans le presse-papiers")
}

/// Colle `text` comme Ctrl/Cmd+V.
fn paste(harness: &mut Harness<'static, App>, text: &str) {
    harness.event(Event::Paste(text.to_owned()));
    harness.run();
}

/// Message du bandeau titré `title`.
fn titled(app: &App, title: TextId) -> Option<(Severity, String)> {
    app.messages()
        .iter()
        .find(|m| m.title == Some(title))
        .map(|m| (m.severity, m.text.clone()))
}

fn label(id: TextId, args: &[(&str, Arg<'_>)]) -> String {
    Texts::new(Language::Fr).get_args(id, args)
}

/// Point au centre de la cellule (`column`) de la ligne de la tâche `n`.
fn cell_point(harness: &Harness<'static, App>, n: TaskUid, column: Column) -> Pos2 {
    let app = harness.state();
    let rows = app.rows().unwrap();
    let index = rows.index_of(n).unwrap();
    let row = rows.row(index).unwrap();
    let row_label = label(
        TextId::RowLabel,
        &[
            ("id", Arg::Count(i64::from(row.id))),
            ("name", Arg::Str(rows.text(index, Column::Name).to_owned().into())),
        ],
    );
    let rect = harness.get_by_label(&row_label).rect();
    let mut x = rect.left();
    for state in app.state.visible_columns() {
        let w = f32::from(state.width);
        if state.column == column {
            return pos2(x + w / 2.0, rect.center().y);
        }
        x += w;
    }
    panic!("colonne {column:?} masquée");
}

/// Clic simple au point `p`.
fn click_at(harness: &mut Harness<'static, App>, p: Pos2) {
    harness.event(Event::PointerMoved(p));
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run();
}

/// Brouillon de la fenêtre « Mise en page » ouverte.
fn page_setup<'a>(
    harness: &'a mut Harness<'static, App>,
) -> &'a mut plannercraft_ui_egui::dialog::page_setup::PageSetupState {
    match harness
        .state_mut()
        .view_mut()
        .dialog
        .as_mut()
        .map(|d| &mut d.body)
    {
        Some(DialogBody::PageSetup(state)) => state,
        _ => panic!("mise en page non ouverte"),
    }
}

/// Résumé comparé après relecture : nom, niveau, mode, actif ; durée et
/// % achevé des feuilles (ceux des récapitulatives sont recalculés par le
/// moteur et écrits tels quels dans le fichier).
/// Ligne du résumé : nom, niveau, (durée, % achevé) d'une feuille,
/// manuelle, active.
type SummaryRow = (String, u16, Option<(i64, i64)>, bool, bool);

fn summary(p: &Project) -> Vec<SummaryRow> {
    let levels: Vec<u16> = p.tasks.iter().map(|t| t.outline_level.get()).collect();
    p.tasks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let level = t.outline_level.get();
            let parent = levels.get(i + 1).is_some_and(|next| *next > level);
            let own = (!parent).then(|| (t.duration.minutes().get(), t.progress.percent_complete.get()));
            (
                t.name.clone(),
                level,
                own,
                matches!(t.mode, SchedulingMode::Manual { .. }),
                t.active,
            )
        })
        .collect()
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "un scénario unique, lu de haut en bas comme la liste du cahier"
)]
fn e2e_daily_j8() {
    let saved: Saved = Rc::new(RefCell::new(Vec::new()));
    let env = Env {
        files: Box::new(MemFiles {
            picks: VecDeque::from([PickedFile::Picked {
                name: "taches.csv".to_owned(),
                result: Ok(CSV.to_vec()),
            }]),
            saved: Rc::clone(&saved),
        }),
        ..Env::fixed(now())
    };
    let state = UiState {
        language: LanguagePref::Fr,
        theme: ThemePref::Light,
        ..UiState::default()
    };
    assert_eq!(state.default_template, TemplateId::Sobre);
    let mut app = App::new(state, env);

    // 0. Ouvrir l'exemple « P ».
    app.open_project(
        plan_p().project().clone(),
        DocOrigin {
            name: "p.xml".to_owned(),
            path: None,
            demo: false,
        },
    );
    let harness = &mut harness_for(app);
    harness.run();
    let initial = project(harness.state()).clone();
    assert_eq!(history(harness.state()), (0, 0));
    assert_eq!(shown(harness.state()), [1, 2, 3, 4, 5, 6, 7]);

    // 1. Filtre « Inachevées » (menu Affichage → Filtre) : rien n'est
    // écrit ; toutes les feuilles sont à 0 %, tout reste affiché.
    click(harness, &fr(TextId::MenuView));
    click(harness, &fr(TextId::MenuFilter));
    click(harness, &fr(TextId::FilterIncomplete));
    assert!(harness.state().task_view().is_filtered());
    assert_eq!(shown(harness.state()), [1, 2, 3, 4, 5, 6, 7]);

    // 2. Tri Début (structure conservée), décroissant pour que l'effet se
    // voie (H-UI-31) ; Maj+F3 ⇒ ordre des ID.
    click(harness, &fr(TextId::MenuView));
    click(harness, &fr(TextId::MenuSort));
    click(harness, &fr(TextId::SortDescending));
    click(harness, &fr(TextId::SortApply));
    assert_eq!(
        harness.state().task_view().sort,
        Some(SortSpec {
            key: SortKey::Start,
            descending: true,
            keep_outline: true,
        })
    );
    assert_eq!(shown(harness.state()), [5, 7, 6, 1, 4, 3, 2]);
    harness.key_press_modifiers(Modifiers::SHIFT, Key::F3);
    harness.run();
    assert_eq!(harness.state().task_view().sort, None);
    assert_eq!(shown(harness.state()), [1, 2, 3, 4, 5, 6, 7]);
    // F3 retire le filtre : la suite travaille sur la vue complète.
    harness.key_press(Key::F3);
    harness.run();
    assert!(!harness.state().task_view().is_filtered());

    // 3. Niveau 1 (menu Affichage) puis Tous.
    click(harness, &fr(TextId::MenuView));
    click(harness, &fr(TextId::MenuOutlineLevel));
    click(
        harness,
        &label(TextId::OutlineLevelN, &[("count", Arg::Count(1))]),
    );
    assert_eq!(shown(harness.state()), [1, 5]);
    view_op(harness, TaskViewOp::SetOutlineLevel(None));
    assert_eq!(shown(harness.state()), [1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(generation(harness.state()), 0, "vue seulement (P50)");

    // 4. Ctrl/Cmd+F « peint », Entrée ⇒ 7 « Peinture » ; Échap ferme.
    harness.key_press_modifiers(Modifiers::COMMAND, Key::F);
    harness.run();
    harness.event(Event::Text("peint".to_owned()));
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();
    assert_eq!(harness.state().selection, Some(uid(7)));
    assert!(
        harness
            .query_by_label(&label(
                TextId::SearchCounter,
                &[("id", Arg::Count(1)), ("count", Arg::Count(1))]
            ))
            .is_some(),
        "compteur 1/1"
    );
    harness.key_press(Key::Escape);
    harness.run();
    assert!(harness.state().search().is_none());
    assert_eq!(history(harness.state()), (0, 0));

    // 5. Copier 3 et 4, coller avant 6 (R-244) : « Murs » et « Livraison »
    // recréés sous 5, lien interne recréé, rapport « 2 liens » externes.
    select_rows(harness, uid(3), uid(4));
    let text = clip_event(harness, Event::Copy);
    assert_eq!(text.lines().count(), 2, "{text:?}");
    select_rows(harness, uid(6), uid(6));
    paste(harness, &text);
    let app = harness.state();
    assert_eq!(
        names(app),
        [
            "Gros œuvre",
            "Fondations",
            "Murs",
            "Livraison",
            "Second œuvre",
            "Murs",
            "Livraison",
            "Plâtrerie",
            "Peinture"
        ]
    );
    // Seconde passe UX (H-UI-104) : compte rendu toujours en information,
    // liens non repris dans leur propre phrase.
    let expected = [
        label(TextId::PasteReportTasks, &[("count", Arg::Count(2))]),
        label(TextId::PasteLinksDropped, &[("count", Arg::Count(2))]),
    ]
    .join(". ");
    assert_eq!(
        titled(app, TextId::PasteReportTitle),
        Some((Severity::Info, expected))
    );
    assert_eq!(history(app), (1, 0));

    // 6. Couper 7 « Peinture » (un lot), coller en fin (ligne vide
    // sélectionnée : niveau 1, un second lot).
    select_rows(harness, uid(7), uid(7));
    let text = clip_event(harness, Event::Cut);
    assert!(text.contains("Peinture"), "{text:?}");
    assert_eq!(history(harness.state()), (2, 0));
    harness.state_mut().dispatch(Action::Select(None));
    harness.run();
    paste(harness, &text);
    let app = harness.state();
    assert_eq!(history(app), (3, 0));
    let last = project(app).tasks.iter().last().unwrap();
    assert_eq!(last.name, "Peinture");
    assert_eq!(last.outline_level.get(), 1);
    assert_ne!(last.uid(), uid(7), "UID neuf");

    // 7. Marquer 50 % sur la récapitulative 1 (bouton du ruban, Tâche →
    // Planification, R-252) : ses feuilles 2 et 3 à 50 %, un lot ; le jalon 4
    // (durée nulle) n'accepte que 0 ou 100 % et reste à 0.
    select(harness, uid(1), uid(1));
    ribbon_click(
        harness,
        RibbonTab::Task,
        &label(TextId::RibbonPercent, &[("percent", Arg::Count(50))]),
    );
    for n in [2, 3] {
        assert_eq!(percent(harness.state(), uid(n)), 5_000, "tâche {n}");
    }
    assert_eq!(percent(harness.state(), uid(4)), 0, "jalon");
    assert_eq!(percent(harness.state(), uid(6)), 0);
    assert_eq!(history(harness.state()), (4, 0));

    // 8. Basculer 6 en manuel (ruban, « Planifier manuellement », R-253), puis saisir son
    // début dans la colonne Début (R-254) : dates manuelles, pas de
    // contrainte, durée gardée.
    select(harness, uid(6), uid(6));
    ribbon_click(harness, RibbonTab::Task, &fr(TextId::RibbonManualSchedule));
    assert!(matches!(
        project(harness.state()).tasks.get(uid(6)).unwrap().mode,
        SchedulingMode::Manual { .. }
    ));
    assert_eq!(history(harness.state()), (5, 0));
    let p = cell_point(harness, uid(6), Column::Start);
    click_at(harness, p);
    assert_eq!(harness.state().view().cursor.column, Column::Start);
    harness.event(Event::Text("16/11/2026".to_owned()));
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();
    let app = harness.state();
    assert!(
        app.messages().iter().all(|m| m.severity != Severity::Error),
        "{:?}",
        app.messages()
    );
    let six = project(app).tasks.get(uid(6)).unwrap();
    let SchedulingMode::Manual { start, finish, .. } = six.mode else {
        panic!("6 n'est plus manuelle : {:?}", six.mode);
    };
    assert_eq!(start, Some("2026-11-16T08:00".parse::<Moment>().unwrap()));
    // Exemple de R-254 : 4 j sur le calendrier Standard ⇒ jeu 19/11 17:00.
    assert_eq!(finish, Some("2026-11-19T17:00".parse::<Moment>().unwrap()));
    assert_eq!(six.duration.minutes().get(), 4 * 480);
    assert_eq!(history(app), (6, 0));

    // 9. Inactiver 2 puis le réactiver (R-255) : deux lots.
    select(harness, uid(2), uid(2));
    ribbon_click(harness, RibbonTab::Task, &fr(TextId::RibbonInactivate));
    assert!(!project(harness.state()).tasks.get(uid(2)).unwrap().active);
    ribbon_click(harness, RibbonTab::Task, &fr(TextId::RibbonInactivate));
    assert!(project(harness.state()).tasks.get(uid(2)).unwrap().active);
    assert_eq!(history(harness.state()), (8, 0));

    // 10. Décocher la surbrillance critique (menu Affichage, H-UI-38) :
    // préférence de la vue, rien n'est écrit.
    let gen_before = generation(harness.state());
    click(harness, &fr(TextId::MenuView));
    click(harness, &fr(TextId::MenuHighlightCritical));
    assert!(!harness.state().state.task_view.highlight_critical);
    assert_eq!(generation(harness.state()), gen_before);
    harness.key_press(Key::Escape);
    harness.run();

    // 11. Fichier → Importer CSV… : aperçu, « Importer » ; le document
    // modifié demande confirmation (H-UI-40) ; « Annuler » ⇒ rien perdu.
    harness.state_mut().dispatch(Action::ImportCsv);
    harness.run();
    assert!(matches!(
        harness.state().view().dialog.as_ref().map(|d| d.kind),
        Some(DialogKind::CsvImport)
    ));
    assert_eq!(harness.query_all_by_label(&fr(TextId::CiRecognized)).count(), 2);
    click(harness, &fr(TextId::CiImport));
    assert!(harness.state().confirmation_pending(), "confirmation attendue");
    click(harness, &fr(TextId::ConfirmCancel));
    let app = harness.state();
    assert_eq!(app.document.as_ref().unwrap().origin.name, "p.xml");
    assert_eq!(history(app), (8, 0));

    // 12. Galerie : appliquer « Présentation » puis annuler (H-UI-52).
    click(harness, &fr(TextId::MenuFormat));
    click(harness, &fr(TextId::MenuGallery));
    let apply = format!(
        "{} {}",
        fr(TextId::GalleryApply),
        fr(template_name(TemplateId::Presentation))
    );
    click(harness, &apply);
    assert_eq!(
        doc_style(project(harness.state())).map(|d| d.base),
        Some(TemplateId::Presentation)
    );
    assert_eq!(history(harness.state()), (9, 0));
    harness.key_press(Key::Escape);
    harness.run();
    harness.state_mut().dispatch(Action::Undo);
    harness.run();
    assert!(doc_style(project(harness.state())).is_none(), "style d'avant");
    assert_eq!(history(harness.state()), (8, 1));

    // 13. Mise en page : A3 paysage, 2 pages en largeur, exporter en PDF,
    // puis enregistrer la mise en page avec le projet (un lot).
    harness
        .state_mut()
        .dispatch(Action::ShowDialog(DialogKind::PageSetup { print_mode: false }));
    harness.run();
    click(harness, "A3");
    click(harness, &fr(TextId::PsLandscape));
    page_setup(harness).page.fit = Fit::PagesWide(2);
    harness.run();
    let draft = page_setup(harness).clone();
    assert_eq!(draft.page.paper, Paper::A3);
    assert!(draft.page.landscape);
    let app = harness.state();
    let style = app
        .state
        .effective_style(app.document.as_ref().unwrap().project());
    let pages = build_pages(
        &app.document.as_ref().unwrap().session,
        &draft.styled(&style),
        plannercraft_automation::text::Language::Fr,
    )
    .unwrap();
    assert!(
        pages.pages.iter().all(|p| p.col < 2) && pages.pages.iter().any(|p| p.col == 1),
        "2 pages en largeur attendues"
    );
    click(harness, &fr(TextId::PsExportPdf));
    {
        let saved = saved.borrow();
        let pdf = saved.last().expect("PDF remis au service de fichiers");
        assert_eq!(pdf.kind, SaveKind::Pdf);
        assert!(pdf.bytes.starts_with(b"%PDF-"));
    }
    click(harness, &fr(TextId::PsSave));
    let app = harness.state();
    assert_eq!(history(app), (9, 0), "rétablissement de la galerie oublié");
    let written = doc_style(project(app)).expect("mise en page écrite").resolve();
    assert_eq!(written.page.paper, Paper::A3);
    assert!(written.page.landscape);
    assert_eq!(written.page.fit, Fit::PagesWide(2));
    if harness.state().view().dialog.is_some() {
        click(harness, &fr(TextId::PsClose));
    }
    let edited = project(harness.state()).clone();
    assert_ne!(edited, initial);

    // 14. Annuler tout ⇒ projet égal à l'ouverture, document propre.
    while history(harness.state()).0 > 0 {
        harness.state_mut().dispatch(Action::Undo);
        harness.run();
    }
    let app = harness.state();
    assert_eq!(project(app), &initial);
    assert_eq!(history(app), (0, 9));
    assert!(!app.document.as_ref().unwrap().is_dirty());

    // 15. Rétablir tout ⇒ projet égal à l'état édité.
    while history(harness.state()).1 > 0 {
        harness.state_mut().dispatch(Action::Redo);
        harness.run();
    }
    let app = harness.state();
    assert_eq!(project(app), &edited);
    assert_eq!(history(app), (9, 0));

    // 16. Enregistrer (service en mémoire) et relire : mêmes tâches, même
    // style (instruction `<?plannercraft-style?>`, H-R12).
    let before = saved.borrow().len();
    harness.state_mut().dispatch(Action::Save);
    harness.run();
    let saved = saved.borrow();
    assert_eq!(saved.len(), before + 1, "une demande d'enregistrement");
    let request = saved.last().unwrap();
    assert_eq!(request.kind, SaveKind::Mspdi);
    let reread = document::load(&request.bytes, now()).unwrap();
    let back = reread.session.project();
    assert_eq!(summary(back), summary(&edited));
    assert_eq!(doc_style(back), doc_style(&edited));
    assert_eq!(uid_of(harness.state(), "Plâtrerie"), uid(6));
}
