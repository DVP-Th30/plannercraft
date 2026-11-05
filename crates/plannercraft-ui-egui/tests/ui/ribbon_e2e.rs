//! Scénario de bout en bout du ruban et de la chronologie (ADR-0017, lot
//! L6, vérification 2 de `docs/plans/ribbon.md`) : un seul test,
//! `e2e_ribbon`, qui enchaîne sur le projet « P » du cahier (§ 2 :
//! 1 « Gros œuvre » ⊃ 2, 3, 4 ; 5 « Second œuvre » ⊃ 6, 7 ; UID = ID) les
//! gestes du ruban **sans barre de menus ni raccourci clavier**, sauf
//! Ctrl+F1 (repli du ruban, H-UI-72) : Lier, pourcentage, Planifier
//! manuellement, Insérer (jalon, récapitulative), Ajouter à la chronologie,
//! Mode plan, Filtre, Projet entier, clic dans la frise, surimpression du
//! ruban replié, Définir la référence, Mode impression et PDF, galerie de
//! modèles ; puis annule tout par la barre d'accès rapide (projet égal à
//! l'ouverture), rétablit tout, enregistre par un `FileService` en mémoire
//! et relit les octets écrits (mêmes tâches, même style, frise comprise).
//!
//! La sélection des lignes passe par le réducteur (`Select`, `SelectWith`),
//! comme un clic et un Maj+clic dans la table ; les éléments du ruban sont
//! cliqués par leur nom accessible. Aucune date n'est calculée ici : les
//! attendus viennent du moteur (bornes de l'axe) ou de l'exemple du cahier.

#![allow(
    clippy::arithmetic_side_effects,
    reason = "tests : compteurs bornés par le scénario"
)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::Sender;

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use plannercraft_automation::command::style::doc_style;
use plannercraft_core::{Moment, Project, SchedulingMode, TaskUid};
use plannercraft_render::style::TemplateId;
use plannercraft_tests::daily::plan_p;
use plannercraft_ui_egui::action::DialogKind;
use plannercraft_ui_egui::dialog::DialogBody;
use plannercraft_ui_egui::dialog::page_setup::PageSetupState;
use plannercraft_ui_egui::document;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts, template_name};
use plannercraft_ui_egui::io::{OpenEvent, SaveEvent, SaveKind, SaveRequest};
use plannercraft_ui_egui::ribbon::RibbonTab;
use plannercraft_ui_egui::selection::SelectMode;
use plannercraft_ui_egui::{Action, App, DocOrigin, Env, FileService, LanguagePref, ThemePref, UiState};

use crate::filters::shown;
use crate::{click, fr, in_ribbon, now, ribbon_click, ribbon_menu, screen_axis};

/// Demandes d'enregistrement reçues par le service en mémoire.
type Saved = Rc<RefCell<Vec<SaveRequest>>>;

/// Service de fichiers en mémoire : garde les octets de chaque
/// enregistrement ou export ; aucune ouverture.
struct MemFiles {
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
}

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

/// (nom, niveau) de chaque tâche, dans l'ordre du projet.
fn outline(app: &App) -> Vec<(String, u16)> {
    project(app)
        .tasks
        .iter()
        .map(|t| (t.name.clone(), t.outline_level.get()))
        .collect()
}

/// UID de la tâche nommée `name` (unique).
fn uid_of(app: &App, name: &str) -> TaskUid {
    let mut found = project(app).tasks.iter().filter(|t| t.name == name);
    let task = found.next().unwrap_or_else(|| panic!("« {name} » absente"));
    assert!(found.next().is_none(), "« {name} » en double");
    plannercraft_core::HasUid::uid(task)
}

/// UID de la chronologie (style effectif du document).
fn timeline(app: &App) -> Vec<u32> {
    app.state.effective_style(project(app)).timeline.tasks
}

/// Sélectionne `first`, puis étend jusqu'à `last` (Maj+clic).
/// Harnais 1 600 × 720 : depuis les finitions P1 (Décaler ▾, Inspecter),
/// l'onglet Tâche replie Propriétés à 1 280 px ; le scénario clique ses
/// éléments dans le ruban déplié.
fn wide(app: App) -> Harness<'static, App> {
    Harness::builder()
        .with_size(egui::vec2(1600.0, 720.0))
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .with_os(egui::os::OperatingSystem::Nix)
        .with_max_steps(8)
        .build_ui_state(plannercraft_ui_egui::ui, app)
}

fn select(harness: &mut Harness<'static, App>, first: u32, last: u32) {
    harness.state_mut().dispatch(Action::Select(Some(uid(first))));
    harness.run();
    if last != first {
        harness.state_mut().dispatch(Action::SelectWith {
            uid: uid(last),
            mode: SelectMode::Range,
        });
        harness.run();
    }
}

fn label(id: TextId, args: &[(&str, Arg<'_>)]) -> String {
    Texts::new(Language::Fr).get_args(id, args)
}

/// Clique sur l'unique élément du ruban nommé `name` hors des onglets :
/// barre d'accès rapide ou corps en surimpression (ruban replié).
fn ribbon_item(harness: &mut Harness<'static, App>, name: &str) {
    {
        let mut found = harness
            .query_all_by_label(name)
            .filter(|n| n.accesskit_node().role() != Role::Tab && in_ribbon(harness, n));
        let node = found
            .next()
            .unwrap_or_else(|| panic!("« {name} » absent du ruban"));
        assert!(found.next().is_none(), "« {name} » en double dans le ruban");
        node.click();
    }
    harness.run();
}

/// Brouillon de la fenêtre « Mise en page » ouverte.
fn page_setup(app: &App) -> PageSetupState {
    match app.view().dialog.as_ref().map(|d| &d.body) {
        Some(DialogBody::PageSetup(state)) => (**state).clone(),
        other => panic!("mise en page non ouverte : {other:?}"),
    }
}

/// Ligne du résumé comparé après relecture : nom, niveau, (durée, %
/// achevé) d'une feuille, manuelle, active, jalon.
type SummaryRow = (String, u16, Option<(i64, i64)>, bool, bool, bool);

/// Résumé des tâches ; durée et % des récapitulatives exclus (recalculés
/// par le moteur et écrits tels quels).
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
                t.milestone,
            )
        })
        .collect()
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "un scénario unique, lu de haut en bas comme la liste du cahier"
)]
fn e2e_ribbon() {
    let saved: Saved = Rc::new(RefCell::new(Vec::new()));
    let env = Env {
        files: Box::new(MemFiles {
            saved: Rc::clone(&saved),
        }),
        ..Env::fixed(now())
    };
    let state = UiState {
        language: LanguagePref::Fr,
        theme: ThemePref::Light,
        ..UiState::default()
    };
    let mut app = App::new(state, env);

    // 0. Ouvrir « P » : ruban déplié sur l'onglet Tâche, frise affichée.
    app.open_project(
        plan_p().project().clone(),
        DocOrigin {
            name: "p.xml".to_owned(),
            path: None,
            demo: false,
        },
    );
    let harness = &mut wide(app);
    harness.run();
    let initial = project(harness.state()).clone();
    assert_eq!(harness.state().state.ribbon.tab, RibbonTab::Task);
    assert!(!harness.state().state.ribbon.collapsed);
    assert!(harness.state().state.timeline_visible);
    assert!(harness.state().view().timeline.drawn.is_some(), "frise dessinée");
    assert_eq!(history(harness.state()), (0, 0));

    // 1. Tâche → Planification → Lier sur {2, 3}, déjà liées (2 → 3 FS) :
    // non-changement (R-260 b, seconde passe UX) : aucun lot, aucun
    // message, rien n'est écrit, aucune entrée d'annulation.
    select(harness, 2, 3);
    let links_before = project(harness.state())
        .tasks
        .get(uid(3))
        .unwrap()
        .predecessors
        .clone();
    ribbon_click(harness, RibbonTab::Task, &fr(TextId::RibbonLink));
    // Aucun refus (même chemin que Édition → Lier).
    assert!(
        !harness
            .state()
            .messages()
            .iter()
            .any(|m| m.title == Some(TextId::CommandErrorTitle)),
        "{:?}",
        harness.state().messages()
    );
    assert_eq!(history(harness.state()), (0, 0), "lien existant : rien d'écrit");
    assert_eq!(
        project(harness.state()).tasks.get(uid(3)).unwrap().predecessors,
        links_before
    );

    // 2. 50 % sur la récapitulative 1 (R-252) : ses feuilles 2 et 3 à 50 %.
    select(harness, 1, 1);
    ribbon_click(
        harness,
        RibbonTab::Task,
        &label(TextId::RibbonPercent, &[("percent", Arg::Count(50))]),
    );
    for n in [2, 3] {
        let pc = project(harness.state())
            .tasks
            .get(uid(n))
            .unwrap()
            .progress
            .percent_complete
            .get();
        assert_eq!(pc, 5_000, "tâche {n}");
    }
    assert_eq!(history(harness.state()), (1, 0));

    // 3. Planifier manuellement sur 6, puis à nouveau (R-253) : deux lots,
    // 6 revient en automatique.
    select(harness, 6, 6);
    ribbon_click(harness, RibbonTab::Task, &fr(TextId::RibbonManualSchedule));
    assert!(matches!(
        project(harness.state()).tasks.get(uid(6)).unwrap().mode,
        SchedulingMode::Manual { .. }
    ));
    ribbon_click(harness, RibbonTab::Task, &fr(TextId::RibbonManualSchedule));
    assert!(!matches!(
        project(harness.state()).tasks.get(uid(6)).unwrap().mode,
        SchedulingMode::Manual { .. }
    ));
    assert_eq!(history(harness.state()), (3, 0));

    // 4. Insérer → Jalon avant 5 (H-UI-83) : « Nouveau jalon », niveau 1,
    // durée nulle, sélectionné.
    select(harness, 5, 5);
    ribbon_click(harness, RibbonTab::Task, &fr(TextId::RibbonInsertMilestone));
    let milestone_name = fr(TextId::NewMilestoneName);
    let app = harness.state();
    let milestone = uid_of(app, &milestone_name);
    let created = project(app).tasks.get(milestone).unwrap();
    assert!(created.milestone && created.duration.is_zero());
    assert_eq!(app.selection, Some(milestone));
    assert_eq!(outline(app)[4], (milestone_name.clone(), 1));
    assert_eq!(outline(app)[5], ("Second œuvre".to_owned(), 1));
    assert_eq!(history(app), (4, 0));

    // 5. Insérer → Récapitulative sur {6, 7} (exemple de H-UI-83) : un lot.
    select(harness, 6, 7);
    ribbon_click(harness, RibbonTab::Task, &fr(TextId::RibbonInsertSummary));
    let summary_name = fr(TextId::NewSummaryName);
    let app = harness.state();
    assert_eq!(
        outline(app)[5..],
        [
            ("Second œuvre".to_owned(), 1),
            (summary_name.clone(), 2),
            ("Plâtrerie".to_owned(), 3),
            ("Peinture".to_owned(), 3),
        ]
    );
    assert_eq!(history(app), (5, 0));

    // 6. Propriétés → Ajouter à la chronologie sur {3, 4} (H-UI-79) :
    // « Murs » et « Livraison » sur la frise, un lot.
    select(harness, 3, 4);
    ribbon_click(harness, RibbonTab::Task, &fr(TextId::TimelineAdd));
    assert_eq!(timeline(harness.state()), [3, 4]);
    assert_eq!(history(harness.state()), (6, 0));
    let generation = harness.state().document.as_ref().unwrap().generation();

    // 7. Affichage → Mode plan → Niveau 1, puis Tous (vue seulement).
    ribbon_menu(
        harness,
        RibbonTab::View,
        &fr(TextId::RibbonOutline),
        &label(TextId::OutlineLevelN, &[("count", Arg::Count(1))]),
    );
    let app = harness.state();
    assert_eq!(shown(app), [1, milestone.get(), 5]);
    ribbon_menu(
        harness,
        RibbonTab::View,
        &fr(TextId::RibbonOutline),
        &fr(TextId::OutlineAll),
    );
    assert_eq!(shown(harness.state()).len(), 9);

    // 8. Filtre « Tâches critiques », puis Effacer le filtre (vue seulement).
    ribbon_menu(
        harness,
        RibbonTab::View,
        &fr(TextId::RibbonFilter),
        &fr(TextId::FilterCritical),
    );
    assert!(harness.state().task_view().is_filtered());
    ribbon_click(harness, RibbonTab::View, &fr(TextId::RibbonFilter));
    let clear = fr(TextId::FilterClear);
    crate::click_where(harness, |text| text.starts_with(&clear));
    assert!(!harness.state().task_view().is_filtered());
    assert_eq!(shown(harness.state()).len(), 9);

    // 9. Zoom → Projet entier : tout le projet tient dans le Gantt.
    ribbon_click(harness, RibbonTab::View, &fr(TextId::RibbonEntireProject));
    let app = harness.state();
    let schedule = app.document.as_ref().unwrap().schedule();
    let axis = screen_axis(app);
    let span = axis.x(schedule.project_finish) - axis.x(schedule.project_start);
    assert!(
        span > 0.0 && span <= app.view().last_gantt_width,
        "{span} > {}",
        app.view().last_gantt_width
    );
    assert_eq!(
        harness.state().document.as_ref().unwrap().generation(),
        generation,
        "étapes 7 à 9 : vue seulement"
    );

    // 10. Clic sur « Murs » dans la frise (H-UI-81) ⇒ 3 active.
    let frise = harness
        .state()
        .view()
        .timeline
        .drawn
        .clone()
        .expect("frise dessinée")
        .rect;
    {
        let murs = harness.get_by_label("Murs");
        assert!(frise.contains(murs.rect().center()), "« Murs » hors de la frise");
        murs.click();
    }
    harness.run();
    assert_eq!(harness.state().selection, Some(uid(3)));

    // 11. Ctrl+F1 replie le ruban ; clic sur « Projet » ⇒ corps en
    // surimpression ; Définir la référence ⇒ fenêtre, surimpression
    // refermée ; OK ⇒ référence 0 enregistrée (un lot) ; Ctrl+F1 déplie.
    harness.key_press_modifiers(Modifiers::COMMAND, Key::F1);
    harness.run();
    assert!(harness.state().state.ribbon.collapsed);
    // Au-delà du délai du double-clic sur un onglet (qui replierait).
    for _ in 0..30 {
        harness.step();
    }
    harness
        .get_by_role_and_label(Role::Tab, &fr(TextId::RibbonTabProject))
        .click();
    harness.run();
    assert_eq!(harness.state().view().ribbon_overlay, Some(RibbonTab::Project));
    ribbon_item(harness, &fr(TextId::RibbonSetBaseline));
    assert_eq!(harness.state().view().ribbon_overlay, None);
    assert_eq!(
        harness.state().view().dialog.as_ref().map(|d| d.kind),
        Some(DialogKind::Baseline)
    );
    click(harness, &fr(TextId::TaskInfoOk));
    assert!(harness.state().view().dialog.is_none());
    assert_eq!(history(harness.state()), (7, 0));
    harness.key_press_modifiers(Modifiers::COMMAND, Key::F1);
    harness.run();
    assert!(!harness.state().state.ribbon.collapsed);

    // 12. Projet → Mode impression (H-UI-82) : case Chronologie cochée
    // d'office, frise sur la page 1 de l'aperçu seulement ; Exporter en PDF
    // ⇒ octets remis au service de fichiers ; Fermer.
    ribbon_click(harness, RibbonTab::Project, &fr(TextId::MenuPrintMode));
    let draft = page_setup(harness.state());
    assert!(draft.page.timeline, "Mode impression coche la chronologie");
    let pages = draft.preview_pages().expect("aperçu calculé");
    assert!(pages.timeline.is_some(), "frise imprimée");
    assert!(pages.pages.first().unwrap().timeline);
    assert!(pages.pages.iter().skip(1).all(|p| !p.timeline));
    click(harness, &fr(TextId::PsExportPdf));
    {
        let saved = saved.borrow();
        let pdf = saved.last().expect("PDF remis au service de fichiers");
        assert_eq!(pdf.kind, SaveKind::Pdf);
        assert!(pdf.bytes.starts_with(b"%PDF-"));
    }
    if harness.state().view().dialog.is_some() {
        click(harness, &fr(TextId::PsClose));
    }
    assert!(harness.state().view().dialog.is_none());
    assert_eq!(history(harness.state()), (7, 0), "aperçu et PDF : rien d'écrit");

    // 13. Format → Modèles de mise en forme…, appliquer « Présentation » :
    // un lot ; la frise du document est gardée.
    ribbon_click(harness, RibbonTab::Format, &fr(TextId::MenuGallery));
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
    assert_eq!(timeline(harness.state()), [3, 4], "frise gardée par la galerie");
    assert_eq!(history(harness.state()), (8, 0));
    click(harness, &fr(TextId::GalleryClose));
    assert!(harness.state().view().dialog.is_none());
    let edited = project(harness.state()).clone();
    assert_ne!(edited, initial);

    // 14. Annuler tout (barre d'accès rapide) ⇒ projet égal à l'ouverture.
    let undo = fr(TextId::MenuUndo);
    while history(harness.state()).0 > 0 {
        ribbon_item(harness, &undo);
    }
    let app = harness.state();
    assert_eq!(project(app), &initial);
    assert_eq!(history(app), (0, 8));
    assert_eq!(timeline(app), Vec::<u32>::new());
    assert!(!app.document.as_ref().unwrap().is_dirty());

    // 15. Rétablir tout ⇒ projet égal à l'état édité.
    let redo = fr(TextId::MenuRedo);
    while history(harness.state()).1 > 0 {
        ribbon_item(harness, &redo);
    }
    let app = harness.state();
    assert_eq!(project(app), &edited);
    assert_eq!(history(app), (8, 0));

    // 16. Enregistrer (barre d'accès rapide, service en mémoire) et relire :
    // mêmes tâches, même style, frise comprise (H-R12, ADR-0017 D7).
    let before = saved.borrow().len();
    ribbon_item(harness, &fr(TextId::MenuSave));
    let saved = saved.borrow();
    assert_eq!(saved.len(), before + 1, "une demande d'enregistrement");
    let request = saved.last().unwrap();
    assert_eq!(request.kind, SaveKind::Mspdi);
    let reread = document::load(&request.bytes, now()).unwrap();
    let back = reread.session.project();
    assert_eq!(summary(back), summary(&edited));
    assert_eq!(doc_style(back), doc_style(&edited));
    let style = doc_style(back).expect("style écrit").resolve();
    assert_eq!(style.timeline.tasks, [3, 4]);
    assert_eq!(doc_style(back).map(|d| d.base), Some(TemplateId::Presentation));
}

/// Pose la liste de la frise par le geste du ruban (Tâche → Propriétés →
/// Ajouter à la chronologie) sur les tâches `first` à `last`.
fn add_to_timeline(harness: &mut Harness<'static, App>, first: u32, last: u32) {
    select(harness, first, last);
    ribbon_click(harness, RibbonTab::Task, &fr(TextId::TimelineAdd));
}

/// Ouvre Format → Modèles de mise en forme… par le ruban.
fn open_gallery(harness: &mut Harness<'static, App>) {
    ribbon_click(harness, RibbonTab::Format, &fr(TextId::MenuGallery));
    assert_eq!(
        harness.state().view().dialog.as_ref().map(|d| d.kind),
        Some(DialogKind::Gallery)
    );
}

/// H-UI-79 (écart relevé en L6) : la liste de la chronologie appartient au
/// projet, comme les couleurs de phase. Appliquer un modèle intégré la
/// garde ; un modèle personnel enregistré n'en emporte aucune ; appliquer
/// un modèle personnel qui en porterait une (fichier de préférences écrit
/// à la main) garde celle du projet ouvert.
#[test]
fn h_ui_79_gallery_keeps_timeline() {
    let mut app = crate::new_app();
    app.open_project(
        plan_p().project().clone(),
        DocOrigin {
            name: "p.xml".to_owned(),
            path: None,
            demo: false,
        },
    );
    let harness = &mut wide(app);
    harness.run();
    add_to_timeline(harness, 3, 4);
    assert_eq!(timeline(harness.state()), [3, 4]);

    // Modèle intégré.
    open_gallery(harness);
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
    assert_eq!(timeline(harness.state()), [3, 4]);

    // Enregistrer comme modèle personnel : sans chronologie.
    match harness
        .state_mut()
        .view_mut()
        .dialog
        .as_mut()
        .map(|d| &mut d.body)
    {
        Some(DialogBody::Gallery(state)) => state.name = "Client".to_owned(),
        _ => panic!("galerie non ouverte"),
    }
    click(harness, &fr(TextId::GallerySave));
    let saved = harness
        .state()
        .state
        .user_templates
        .iter()
        .find(|t| t.name == "Client")
        .expect("modèle personnel enregistré")
        .style
        .clone();
    assert_eq!(saved.base, TemplateId::Presentation);
    assert!(saved.over.timeline.is_none(), "{:?}", saved.over.timeline);

    // Modèle personnel portant une liste étrangère (UID 7) : la liste du
    // projet ouvert reste.
    let mut foreign = saved;
    foreign.base = TemplateId::Classique;
    foreign.over.timeline = Some(plannercraft_render::style::TimelineSpec { tasks: vec![7] });
    harness
        .state_mut()
        .state
        .user_templates
        .push(plannercraft_ui_egui::state::NamedStyle {
            name: "Ailleurs".to_owned(),
            style: foreign,
        });
    harness.run();
    click(harness, &format!("{} Ailleurs", fr(TextId::GalleryApply)));
    assert_eq!(
        doc_style(project(harness.state())).map(|d| d.base),
        Some(TemplateId::Classique)
    );
    assert_eq!(timeline(harness.state()), [3, 4]);
}
