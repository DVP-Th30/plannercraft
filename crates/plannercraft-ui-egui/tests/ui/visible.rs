//! Lot visible de J8 (L15) : menus Tâche et Projet dans la barre, boutons
//! 0 à 100 %, bascules du mode et de l'activité (H-UI-35, H-UI-36), galerie
//! des modèles (H-UI-52), volet de mise en forme directe (H-UI-59 à
//! H-UI-61), retours des plannings DCE (H-UI-62 à H-UI-67), mode
//! impression et mise en page (H-UI-68). Modèle par défaut : Sobre (comme
//! l'application), sauf mention.

#![allow(
    clippy::arithmetic_side_effects,
    clippy::float_cmp,
    reason = "tests : positions du pointeur et compteurs bornés"
)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::Sender;

use egui::{Color32, Pos2, Rect, Shape, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_automation::command::style::doc_style;
use plannercraft_automation::helpers::PercentStep;
use plannercraft_core::{Moment, TaskUid};
use plannercraft_render::style::{BarField, BarRole, Rgb, TableColumn, TemplateId, Variant, template};
use plannercraft_ui_egui::action::{DialogKind, FormatOp, TaskCmd};
use plannercraft_ui_egui::dialog::DialogBody;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::io::{OpenEvent, SaveEvent, SaveKind, SaveRequest};
use plannercraft_ui_egui::{Action, App, Env, FileService, LanguagePref, ThemePref, UiState};

use crate::filters::open_p_in;
use plannercraft_ui_egui::ribbon::RibbonTab;

use crate::{click, fr, harness_for, now, ribbon_click, screen_axis};

/// Demandes d'enregistrement reçues par le service en mémoire.
type Saved = Rc<RefCell<Vec<SaveRequest>>>;

/// Service de fichiers en mémoire : garde les octets des exports.
struct MemFiles(Saved);

impl FileService for MemFiles {
    fn pick_and_open(&mut self, ctx: &egui::Context, _: Moment, _: String, _: String, tx: Sender<OpenEvent>) {
        tx.send(OpenEvent::Cancelled).unwrap();
        ctx.request_repaint();
    }

    fn save(&mut self, ctx: &egui::Context, request: SaveRequest, tx: Sender<SaveEvent>) {
        self.0.borrow_mut().push(request);
        tx.send(SaveEvent::Saved {
            name: "export".to_owned(),
            path: None,
        })
        .unwrap();
        ctx.request_repaint();
    }
}

/// Application française claire, modèle par défaut Sobre (défaut de
/// l'application depuis L15), projet « P » ouvert, service de fichiers en
/// mémoire.
fn sobre_p() -> (Harness<'static, App>, Saved) {
    let saved: Saved = Rc::new(RefCell::new(Vec::new()));
    let env = Env {
        files: Box::new(MemFiles(Rc::clone(&saved))),
        ..Env::fixed(now())
    };
    let state = UiState {
        language: LanguagePref::Fr,
        theme: ThemePref::Light,
        ..UiState::default()
    };
    assert_eq!(state.default_template, TemplateId::Sobre);
    let mut app = App::new(state, env);
    open_p_in(&mut app);
    let mut harness = harness_for(app);
    harness.run();
    harness.run();
    (harness, saved)
}

fn uid(n: u32) -> TaskUid {
    TaskUid::new(n)
}

fn select(harness: &mut Harness<'static, App>, n: u32) {
    harness.state_mut().dispatch(Action::Select(Some(uid(n))));
    harness.run();
}

/// La tâche est en planification manuelle.
fn manual(task: &plannercraft_core::Task) -> bool {
    matches!(task.mode, plannercraft_core::SchedulingMode::Manual { .. })
}

fn percent(app: &App, n: u32) -> i64 {
    app.document
        .as_ref()
        .unwrap()
        .project()
        .tasks
        .get(uid(n))
        .unwrap()
        .progress
        .percent_complete
        .get()
}

fn history(app: &App) -> usize {
    app.document.as_ref().unwrap().session.history().entries.len()
}

fn label(id: TextId, args: &[(&str, Arg<'_>)]) -> String {
    Texts::new(Language::Fr).get_args(id, args)
}

fn pct_label(id: TextId, pct: i64) -> String {
    label(id, &[("percent", Arg::Count(pct))])
}

/// Formes à plat de la dernière image.
fn shapes(harness: &Harness<'static, App>) -> Vec<Shape> {
    fn flat(shape: &Shape, out: &mut Vec<Shape>) {
        if let Shape::Vec(inner) = shape {
            for s in inner {
                flat(s, out);
            }
        } else {
            out.push(shape.clone());
        }
    }
    let mut out = Vec::new();
    for c in &harness.output().shapes {
        flat(&c.shape, &mut out);
    }
    out
}

/// Textes dessinés (position, texte).
fn texts(harness: &Harness<'static, App>) -> Vec<(Pos2, String)> {
    shapes(harness)
        .into_iter()
        .filter_map(|s| match s {
            Shape::Text(t) => Some((t.pos, t.galley.job.text.clone())),
            _ => None,
        })
        .collect()
}

/// Rectangle de la ligne de la tâche `n` dans la table (widget de ligne).
fn row_rect(harness: &Harness<'static, App>, n: u32, name: &str) -> Rect {
    let app = harness.state();
    let rows = app.rows().unwrap();
    let id = (0..rows.len())
        .find_map(|i| rows.row(i).filter(|r| r.uid == uid(n)).map(|r| r.id))
        .unwrap();
    let label = label(
        TextId::RowLabel,
        &[
            ("id", Arg::Count(i64::from(id))),
            ("name", Arg::Str(name.to_owned().into())),
        ],
    );
    harness.get_by_label(&label).rect()
}

/// Centre écran de la barre de la tâche `n` (dates du moteur projetées par
/// l'axe de l'interface).
fn bar_center(harness: &Harness<'static, App>, n: u32, name: &str) -> Pos2 {
    let app = harness.state();
    let rows = app.rows().unwrap();
    let (start, finish) = (0..rows.len())
        .find_map(|i| rows.row(i).filter(|r| r.uid == uid(n)).and_then(|r| r.bar))
        .unwrap();
    let axis = screen_axis(app);
    let x = f32::midpoint(axis.x(start), axis.x(finish));
    pos2(x, row_rect(harness, n, name).center().y)
}

/// Clic droit en `p`.
fn right_click(harness: &mut Harness<'static, App>, p: Pos2) {
    harness.hover_at(p);
    harness.step();
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        harness.step();
    }
    harness.run();
}

/// Style du document ouvert (`None` : aucun style choisi).
fn doc(app: &App) -> Option<plannercraft_render::style::DocStyle> {
    doc_style(app.document.as_ref().unwrap().project())
}

/// Style effectif du document ouvert.
fn effective(app: &App) -> plannercraft_render::style::PlanStyle {
    app.state
        .effective_style(app.document.as_ref().unwrap().project())
}

const fn rgb(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

/// H-UI-35 : bouton « 50 % » du ruban (Tâche → Planification ; barre
/// d'outils jusqu'au lot L5) sur une feuille ⇒ 50 %,
/// une entrée d'annulation.
#[test]
fn h_ui_35_percent_buttons() {
    let (mut harness, _) = sobre_p();
    select(&mut harness, 2);
    let before = history(harness.state());
    ribbon_click(
        &mut harness,
        RibbonTab::Task,
        &pct_label(TextId::RibbonPercent, 50),
    );
    assert_eq!(percent(harness.state(), 2), 5_000);
    assert_eq!(history(harness.state()), before + 1);
    ribbon_click(
        &mut harness,
        RibbonTab::Task,
        &pct_label(TextId::RibbonPercent, 100),
    );
    assert_eq!(percent(harness.state(), 2), 10_000);
    // Rien à faire : aucun lot, un message.
    ribbon_click(
        &mut harness,
        RibbonTab::Task,
        &pct_label(TextId::RibbonPercent, 100),
    );
    assert_eq!(history(harness.state()), before + 2);
    assert!(
        harness
            .state()
            .messages()
            .iter()
            .any(|m| m.text == fr(TextId::TaskCmdNothing))
    );
}

/// H-UI-35 (R-252) : « Marquer à 100 % » du menu Tâche sur la
/// récapitulative 1 marque ses feuilles 2 et 3 (le jalon 4 aussi), en un
/// lot.
#[test]
fn h_ui_35_percent_on_summary_marks_leaves() {
    let (mut harness, _) = sobre_p();
    select(&mut harness, 1);
    let before = history(harness.state());
    click(&mut harness, &fr(TextId::MenuTask));
    click(&mut harness, &pct_label(TextId::MenuMarkPercent, 100));
    for n in [2, 3, 4] {
        assert_eq!(percent(harness.state(), n), 10_000, "tâche {n}");
    }
    assert_eq!(percent(harness.state(), 6), 0);
    assert_eq!(history(harness.state()), before + 1);
}

/// Menu Tâche dans la barre : %, mode, actif, informations, lier, délier ;
/// grisé sans sélection.
#[test]
fn menu_task_in_bar() {
    let (mut harness, _) = sobre_p();
    harness.state_mut().dispatch(Action::Select(None));
    harness.run();
    click(&mut harness, &fr(TextId::MenuTask));
    for id in [
        TextId::MenuToggleMode,
        TextId::MenuToggleActive,
        TextId::TaskInfoMenu,
        TextId::StructureLink,
        TextId::StructureUnlink,
    ] {
        assert!(
            harness.query_by_label_contains(&fr(id)).is_some(),
            "{id:?} absent"
        );
    }
    harness.key_press(egui::Key::Escape);
    harness.run();
    select(&mut harness, 6);
    click(&mut harness, &fr(TextId::MenuTask));
    click(&mut harness, &fr(TextId::MenuToggleMode));
    let task = |app: &App| {
        app.document
            .as_ref()
            .unwrap()
            .project()
            .tasks
            .get(uid(6))
            .unwrap()
            .clone()
    };
    assert!(manual(&task(harness.state())), "6 manuelle");
}

/// H-UI-36, R-255 : bascules « Planifier manuellement » et « Désactiver »
/// du ruban (« Mode » et « Actif » de la barre d'outils jusqu'au lot L5),
/// une entrée d'annulation chacune ; Annuler revient en arrière.
#[test]
fn ribbon_toggles() {
    let (mut harness, _) = sobre_p();
    select(&mut harness, 6);
    let task = |app: &App| {
        app.document
            .as_ref()
            .unwrap()
            .project()
            .tasks
            .get(uid(6))
            .unwrap()
            .clone()
    };
    let before = history(harness.state());
    ribbon_click(&mut harness, RibbonTab::Task, &fr(TextId::RibbonManualSchedule));
    assert!(manual(&task(harness.state())));
    ribbon_click(&mut harness, RibbonTab::Task, &fr(TextId::RibbonInactivate));
    assert!(!task(harness.state()).active);
    assert_eq!(history(harness.state()), before + 2);
    harness.state_mut().dispatch(Action::Undo);
    harness.run();
    assert!(task(harness.state()).active);
    harness.state_mut().dispatch(Action::Undo);
    harness.run();
    assert!(!manual(&task(harness.state())));
    // Sans sélection : message, rien écrit.
    harness
        .state_mut()
        .dispatch(Action::TaskCmd(TaskCmd::MarkPercent(PercentStep::P50)));
    harness.state_mut().dispatch(Action::Select(None));
    harness.run();
}

/// H-UI-52 : galerie, « Appliquer Présentation » ⇒ style du document ;
/// Ctrl+Z ⇒ style d'avant (aucun).
#[test]
fn h_ui_52_gallery_apply_undo() {
    let (mut harness, _) = sobre_p();
    assert!(doc(harness.state()).is_none());
    click(&mut harness, &fr(TextId::MenuFormat));
    click(&mut harness, &fr(TextId::MenuGallery));
    assert!(matches!(
        harness.state().view().dialog.as_ref().map(|d| &d.body),
        Some(DialogBody::Gallery(_))
    ));
    let apply = format!(
        "{} {}",
        fr(TextId::GalleryApply),
        fr(plannercraft_ui_egui::i18n::template_name(
            TemplateId::Presentation
        ))
    );
    let before = history(harness.state());
    click(&mut harness, &apply);
    assert_eq!(
        doc(harness.state()).map(|d| d.base),
        Some(TemplateId::Presentation)
    );
    assert_eq!(history(harness.state()), before + 1);
    harness.key_press(egui::Key::Escape);
    harness.run();
    harness.state_mut().dispatch(Action::Undo);
    harness.run();
    assert!(doc(harness.state()).is_none(), "style d'avant");
}

/// Relecture finale de J8 (F5) : appliquer un modèle depuis la galerie
/// garde les couleurs de phase et la mise en page enregistrées du
/// document (cartouche saisi, colonnes d'impression) ; le reste suit le
/// modèle ; une seule entrée d'annulation.
#[test]
fn h_ui_52_gallery_apply_keeps_phases_and_page() {
    let (mut harness, _) = sobre_p();
    let batch = plannercraft_automation::Batch::from_json_str(
        r##"{"format_version":1,"ops":[{"op":"style.apply","template":"sobre","patch":{
            "phases":{"5":{"fill":"#D55E00","children":true}},
            "bars":{"task":{"shape":"bar","height_pct":80,"radius16":0,"outline16":0,"pattern":"solid"}}}},
          {"op":"style.apply","patch":{"page":{"paper":"a3","landscape":true,"margins_mm":[10,10,10,10],
            "fit":{"pages_wide":1},"repeat_table":true,"header":["","Chantier Beaujeu",""],
            "footer":["","",""],"logo":null,"columns":["id","name"],"title_block":true}}}]}"##,
    )
    .unwrap();
    harness
        .state_mut()
        .execute(&batch, plannercraft_ui_egui::EditOrigin::User)
        .unwrap();
    harness.run();
    let before = doc(harness.state()).unwrap();
    assert!(before.over.phases.is_some() && before.over.page.is_some());
    click(&mut harness, &fr(TextId::MenuFormat));
    click(&mut harness, &fr(TextId::MenuGallery));
    let apply = format!(
        "{} {}",
        fr(TextId::GalleryApply),
        fr(plannercraft_ui_egui::i18n::template_name(
            TemplateId::Presentation
        ))
    );
    let undo_before = history(harness.state());
    click(&mut harness, &apply);
    let after = doc(harness.state()).unwrap();
    assert_eq!(after.base, TemplateId::Presentation);
    assert_eq!(after.over.phases, before.over.phases, "couleurs de phase gardées");
    assert_eq!(after.over.page, before.over.page, "mise en page gardée");
    assert!(
        after.over.bars.is_empty(),
        "les autres surcharges suivent le modèle"
    );
    assert_eq!(history(harness.state()), undo_before + 1);
}

/// H-UI-52 : « Enregistrer comme modèle personnel » garde le style du
/// document sous son nom ; le modèle se réapplique.
#[test]
fn h_ui_52_user_template_saved() {
    let (mut harness, _) = sobre_p();
    harness
        .state_mut()
        .dispatch(Action::ShowDialog(DialogKind::Gallery));
    harness.run();
    let name = fr(TextId::GallerySaveName);
    harness
        .get_by_role_and_label(egui::accesskit::Role::TextInput, &name)
        .focus();
    harness.run();
    harness.event(egui::Event::Text("Agence".to_owned()));
    harness.run();
    click(&mut harness, &fr(TextId::GallerySave));
    let app = harness.state();
    assert_eq!(app.state.user_templates.len(), 1);
    assert_eq!(app.state.user_templates[0].name, "Agence");
    assert_eq!(app.state.user_templates[0].style.base, TemplateId::Sobre);
    let saved = label(TextId::GallerySaved, &[("name", Arg::Str("Agence".into()))]);
    assert!(harness.query_by_label(&saved).is_some());
}

/// H-UI-59 : clic droit sur la barre critique de « Murs » ⇒ entrée
/// « Mettre en forme les barres de ce type » ⇒ volet « Barres critiques ».
#[test]
fn h_ui_59_right_click_bar_opens_role_panel() {
    let (mut harness, _) = sobre_p();
    let p = bar_center(&harness, 3, "Murs");
    right_click(&mut harness, p);
    click(&mut harness, &fr(TextId::ContextFormatBars));
    let panel = harness.state().view().format_panel.clone().expect("volet ouvert");
    assert_eq!(panel.role, BarRole::Critical);
    assert_eq!(panel.task, Some(uid(3)));
    let title = label(
        TextId::FpTitle,
        &[("role", Arg::Str(fr(TextId::FpRoleCritical).into()))],
    );
    assert!(
        harness.query_by_label(&title).is_some(),
        "titre « {title} » absent"
    );
}

/// Ouvre le volet du type `role` (tâche `task`).
fn open_panel(harness: &mut Harness<'static, App>, role: BarRole, task: Option<u32>) {
    harness.state_mut().dispatch(Action::Format(FormatOp::OpenBars {
        role,
        task: task.map(uid),
    }));
    harness.run();
}

/// H-UI-60 : une couleur du nuancier sur les barres critiques ⇒ barres
/// repeintes à l'image suivante, une entrée d'annulation ; Ctrl+Z ⇒
/// couleur d'avant.
#[test]
fn h_ui_60_live_color_change_one_undo() {
    let (mut harness, _) = sobre_p();
    open_panel(&mut harness, BarRole::Critical, None);
    let before = history(harness.state());
    let old = effective(harness.state()).tokens(Variant::Light).bars[&BarRole::Critical].fill;
    let green = Rgb(0x00, 0x9E, 0x73);
    click(
        &mut harness,
        &label(TextId::FpSwatch, &[("color", Arg::Str(green.hex().into()))]),
    );
    harness.run();
    assert_eq!(history(harness.state()), before + 1);
    let st = effective(harness.state());
    assert_eq!(st.tokens(Variant::Light).bars[&BarRole::Critical].fill, green);
    // Repeint : une barre de cette couleur dans l'image.
    let painted = shapes(&harness)
        .iter()
        .any(|s| matches!(s, Shape::Rect(r) if r.fill == rgb(green)));
    assert!(painted, "barre critique non repeinte");
    harness.state_mut().dispatch(Action::Undo);
    harness.run();
    assert_eq!(
        effective(harness.state()).tokens(Variant::Light).bars[&BarRole::Critical].fill,
        old
    );
}

/// H-UI-61 : jaune clair sur fond blanc ⇒ avertissement de contraste et
/// couleur conforme proposée ; la proposition s'applique.
#[test]
fn h_ui_61_low_contrast_warned() {
    let (mut harness, _) = sobre_p();
    open_panel(&mut harness, BarRole::Task, None);
    let yellow = Rgb(0xF0, 0xE4, 0x42);
    click(
        &mut harness,
        &label(TextId::FpSwatch, &[("color", Arg::Str(yellow.hex().into()))]),
    );
    harness.run();
    let warned = texts(&harness)
        .iter()
        .any(|(_, t)| t.starts_with("Contraste insuffisant"));
    assert!(warned, "aucun avertissement de contraste");
    let suggestion =
        plannercraft_automation::command::style::nearest_compliant(yellow, Rgb(0xFF, 0xFF, 0xFF), 3_000)
            .unwrap();
    click(
        &mut harness,
        &label(
            TextId::FpUseSuggestion,
            &[("color", Arg::Str(suggestion.hex().into()))],
        ),
    );
    harness.run();
    let st = effective(harness.state());
    assert_eq!(st.tokens(Variant::Light).bars[&BarRole::Task].fill, suggestion);
    assert!(
        !texts(&harness)
            .iter()
            .any(|(_, t)| t.starts_with("Contraste insuffisant"))
    );
}

/// Zone du Gantt à la dernière image (à droite de la table).
fn gantt_zone(harness: &Harness<'static, App>) -> Rect {
    let view = harness.state().view();
    Rect::from_min_size(
        pos2(view.last_gantt_left, 0.0),
        egui::vec2(view.last_gantt_width, 2_000.0),
    )
}

/// H-UI-62 : en Sobre, aucun trait horizontal de toute la largeur du
/// Gantt (lignes de la grille) ni fond de ligne dans la zone graphique ;
/// verticales de l'échelle présentes.
#[test]
fn h_ui_62_vertical_grid_only() {
    let (harness, _) = sobre_p();
    let zone = gantt_zone(&harness);
    // Haut du corps : première ligne (l'en-tête de l'échelle est au-dessus).
    let body_top = row_rect(&harness, 1, "Gros œuvre").top();
    let body_bottom = row_rect(&harness, 7, "Peinture").bottom();
    let tokens = template(TemplateId::Sobre).tokens(Variant::Light).clone();
    let wide = zone.width() * 0.8;
    let mut verticals = 0;
    for s in shapes(&harness) {
        match s {
            Shape::LineSegment { points: [a, b], .. } if a.x >= zone.left() && b.x >= zone.left() => {
                if (a.y - b.y).abs() < 0.01 && (a.x - b.x).abs() > wide {
                    // Lignes du corps seulement (en-tête et barre de
                    // défilement hors du corps).
                    assert!(
                        a.y < body_top || a.y > body_bottom,
                        "trait horizontal dans le Gantt à y = {}",
                        a.y
                    );
                }
                if (a.x - b.x).abs() < 0.01 && (a.y - b.y).abs() > 100.0 {
                    verticals += 1;
                }
            }
            Shape::Rect(r)
                if r.rect.left() >= zone.left()
                    && r.rect.width() > wide
                    && r.rect.top() >= body_top
                    && r.rect.height() >= 15.0 =>
            {
                assert!(
                    r.fill != rgb(tokens.band[1]),
                    "bande de fond dans le Gantt : {:?}",
                    r.rect
                );
            }
            _ => {}
        }
    }
    assert!(verticals > 0, "aucune verticale");
}

/// H-UI-63 : case « durée après le nom » (barres critiques : 2, 3, 6, 7
/// dans P) ⇒ « Fondations (5 j) » sur la barre ; décochée ⇒
/// « Fondations ».
#[test]
fn h_ui_63_duration_after_name_toggle() {
    let (mut harness, _) = sobre_p();
    open_panel(&mut harness, BarRole::Critical, None);
    click(&mut harness, &fr(TextId::FpDurationAfterName));
    harness.run();
    let st = effective(harness.state());
    assert_eq!(st.texts[&BarRole::Critical].right, Some(BarField::NameDuration));
    let drawn = |h: &Harness<'static, App>, t: &str| texts(h).iter().any(|(_, x)| x == t);
    assert!(drawn(&harness, "Fondations (5\u{a0}j)"), "{:?}", texts(&harness));
    click(&mut harness, &fr(TextId::FpDurationAfterName));
    harness.run();
    assert_eq!(
        effective(harness.state()).texts[&BarRole::Critical].right,
        Some(BarField::Name)
    );
    assert!(drawn(&harness, "Fondations"));
}

/// H-UI-63 : jalon « Livraison » : date courte à gauche du losange, nom à
/// droite (Sobre) ; récapitulative : nom à gauche, durée courte à droite.
#[test]
fn h_ui_63_milestone_date_left() {
    let (harness, _) = sobre_p();
    let app = harness.state();
    let s = app.document.as_ref().unwrap().schedule();
    let finish = match s.tasks.get(&uid(4)) {
        Some(plannercraft_core::TaskOutcome::Scheduled(t)) => t.finish,
        other => panic!("{other:?}"),
    };
    let date = finish.day().to_naive().format("%d/%m/%y").to_string();
    let drawn = texts(&harness);
    let at = |t: &str| drawn.iter().find(|(_, x)| x == t).map(|(p, _)| *p);
    let zone = gantt_zone(&harness);
    let d = at(&date).unwrap_or_else(|| panic!("date {date} absente : {drawn:?}"));
    // Nom de la tâche dans le Gantt (la table porte aussi « Livraison »).
    let n = drawn
        .iter()
        .find(|(p, x)| x == "Livraison" && p.x > zone.left())
        .map(|(p, _)| *p)
        .unwrap();
    assert!(d.x > zone.left() && d.x < n.x, "{d:?} {n:?}");
    // Récapitulative « Second œuvre » : nom à gauche de sa durée courte.
    let name = drawn
        .iter()
        .find(|(p, x)| x == "Second œuvre" && p.x > zone.left())
        .map(|(p, _)| *p)
        .unwrap();
    let duration = at("7\u{a0}j").unwrap();
    assert!(name.x < duration.x);
}

/// H-UI-64 : les liens sont peints avant les étiquettes des barres
/// (étiquettes au-dessus), chaque étiquette sur un fond.
#[test]
fn h_ui_64_labels_above_links() {
    let (harness, _) = sobre_p();
    // Sobre tel que l'écran le dessine (interface adoucie, lot U1).
    let screen = plannercraft_ui_egui::theme::screen_style(template(TemplateId::Sobre)).0;
    let tokens = screen.tokens(Variant::Light).clone();
    let all = shapes(&harness);
    let zone = gantt_zone(&harness);
    let link = |s: &Shape| match s {
        Shape::Path(p) => {
            p.points.first().is_some_and(|q| q.x > zone.left())
                && (p.stroke.color == egui::epaint::ColorMode::Solid(rgb(tokens.link))
                    || p.stroke.color == egui::epaint::ColorMode::Solid(rgb(tokens.link_critical)))
        }
        _ => false,
    };
    let label =
        |s: &Shape| matches!(s, Shape::Text(t) if t.galley.job.text == "Murs" && t.pos.x > zone.left());
    let last_link = all.iter().rposition(link).expect("aucun lien");
    let first_label = all.iter().position(label).expect("étiquette « Murs » absente");
    assert!(last_link < first_label, "{last_link} ≥ {first_label}");
}

/// H-UI-64 : « graphique seul » réduit la table au N° et au Nom à
/// l'écran ; la sortie du mode rend les colonnes choisies.
#[test]
fn h_ui_64_chart_only_mode() {
    let (mut harness, _) = sobre_p();
    let duration = fr(TextId::ColDuration);
    let has = |h: &Harness<'static, App>, t: &str| texts(h).iter().any(|(_, x)| x == t);
    assert!(has(&harness, &duration));
    click(&mut harness, &fr(TextId::MenuView));
    click(&mut harness, &fr(TextId::MenuChartOnly));
    harness.run();
    assert!(harness.state().state.chart_only);
    assert!(!has(&harness, &duration), "colonne Durée encore dessinée");
    assert!(has(&harness, &fr(TextId::ColName)));
    harness.state_mut().dispatch(Action::SetChartOnly(false));
    harness.run();
    assert!(has(&harness, &duration));
}

/// H-UI-65 : couleur de phase sur la récapitulative « Second œuvre » ⇒
/// barre et sous-barres de la couleur, fond de sa ligne dans la table en
/// teinte lisible ; une entrée d'annulation.
#[test]
fn h_ui_65_phase_color_row_and_bar() {
    let (mut harness, _) = sobre_p();
    open_panel(&mut harness, BarRole::Summary, Some(5));
    let red = Rgb(0xC0, 0x00, 0x00);
    let before = history(harness.state());
    click(
        &mut harness,
        &label(TextId::FpPhaseSwatch, &[("color", Arg::Str(red.hex().into()))]),
    );
    harness.run();
    assert_eq!(history(harness.state()), before + 1);
    let st = effective(harness.state());
    assert_eq!(st.phases.get(&5).map(|p| p.fill), Some(red));
    assert_eq!(doc(harness.state()).map(|d| d.base), Some(TemplateId::Sobre));
    let zone = gantt_zone(&harness);
    let all = shapes(&harness);
    let bars = all
        .iter()
        .filter(|s| matches!(s, Shape::Rect(r) if r.fill == rgb(red) && r.rect.left() > zone.left()))
        .count();
    assert!(bars >= 2, "sous-barres de la couleur : {bars}");
    // Fond de l'écran : Sobre adouci (lot U1).
    let screen = plannercraft_ui_egui::theme::screen_style(st.clone()).0;
    let tint = plannercraft_render::style::phase_row_tint(&screen, Variant::Light, red);
    let row = row_rect(&harness, 5, "Second œuvre");
    let filled = all
        .iter()
        .any(|s| matches!(s, Shape::Rect(r) if r.fill == rgb(tint) && (r.rect.center().y - row.center().y).abs() < 2.0));
    assert!(filled, "fond de ligne de phase absent");
}

/// H-UI-61 et H-UI-65, relecture finale de J8 : une couleur de phase
/// trop pâle sur le fond (jaune sur blanc, 1,3:1) est signalée dans le
/// volet, avec une couleur conforme sur les fonds des trois variantes ;
/// `style.get` (`contrast_issues`) la signale aussi.
#[test]
fn h_ui_61_phase_color_contrast_warned() {
    let (mut harness, _) = sobre_p();
    open_panel(&mut harness, BarRole::Summary, Some(5));
    let yellow = Rgb(0xF0, 0xE4, 0x42);
    click(
        &mut harness,
        &label(TextId::FpPhaseSwatch, &[("color", Arg::Str(yellow.hex().into()))]),
    );
    harness.run();
    let warned = |h: &Harness<'static, App>| {
        texts(h)
            .iter()
            .any(|(_, t)| t.starts_with("Contraste insuffisant"))
    };
    assert!(warned(&harness), "aucun avertissement pour la phase");
    let st = effective(harness.state());
    let issues = plannercraft_automation::command::style::contrast_issues(&st);
    assert!(
        issues.iter().any(|i| i.phase == Some(5) && i.color == yellow),
        "{issues:?}"
    );
    let bgs = [Variant::Light, Variant::Dark, Variant::Print].map(|v| st.tokens(v).bg);
    let worst = *bgs
        .iter()
        .min_by_key(|bg| plannercraft_render::style::contrast_ratio(yellow, **bg))
        .unwrap();
    let better = plannercraft_automation::command::style::nearest_compliant(yellow, worst, 3_000)
        .filter(|b| {
            bgs.iter()
                .all(|bg| plannercraft_render::style::contrast_ratio(*b, *bg) >= 3_000)
        })
        .expect("couleur conforme sur les trois fonds");
    click(
        &mut harness,
        &label(
            TextId::FpUseSuggestion,
            &[("color", Arg::Str(better.hex().into()))],
        ),
    );
    harness.run();
    let st = effective(harness.state());
    assert_eq!(st.phases.get(&5).map(|p| p.fill), Some(better));
    assert!(!warned(&harness));
}

/// H-UI-67 : « Planning travaux » ⇒ niveau des mois relatifs (« M1 ») en
/// haut de l'échelle.
#[test]
fn h_ui_67_relative_months_tier() {
    let (mut harness, _) = sobre_p();
    let has_m1 = |h: &Harness<'static, App>| texts(h).iter().any(|(_, t)| t == "M1");
    assert!(!has_m1(&harness));
    let batch = plannercraft_automation::Batch::from_json_str(
        r#"{"format_version":1,"ops":[{"op":"style.apply","template":"chantier"}]}"#,
    )
    .unwrap();
    harness
        .state_mut()
        .execute(&batch, plannercraft_ui_egui::EditOrigin::User)
        .unwrap();
    harness.run();
    assert!(has_m1(&harness), "{:?}", texts(&harness));
}

/// H-UI-67 « en option », relecture finale de J8 : Format → « Mois
/// relatifs » les affiche sous Sobre (une entrée d'annulation), puis les
/// retire.
#[test]
fn h_ui_67_relative_months_menu_toggle() {
    let (mut harness, _) = sobre_p();
    let has_m1 = |h: &Harness<'static, App>| texts(h).iter().any(|(_, t)| t == "M1");
    assert!(!has_m1(&harness));
    let before = history(harness.state());
    click(&mut harness, &fr(TextId::MenuFormat));
    click(&mut harness, &fr(TextId::MenuRelativeMonths));
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(has_m1(&harness), "{:?}", texts(&harness));
    assert_eq!(history(harness.state()), before + 1);
    let style = doc(harness.state()).expect("style écrit");
    assert_eq!(style.base, TemplateId::Sobre);
    assert!(style.resolve().scale.relative_months);
    click(&mut harness, &fr(TextId::MenuFormat));
    click(&mut harness, &fr(TextId::MenuRelativeMonths));
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(!has_m1(&harness));
}

/// État de la fenêtre « Mise en page » ouverte.
fn page_state(app: &App) -> plannercraft_ui_egui::dialog::page_setup::PageSetupState {
    match app.view().dialog.as_ref().map(|d| &d.body) {
        Some(DialogBody::PageSetup(state)) => (**state).clone(),
        other => panic!("mise en page non ouverte : {other:?}"),
    }
}

/// H-UI-68 : mode impression en un clic (barre d'outils) ⇒ colonnes
/// d'impression N°, Nom, Durée, Début, Fin.
#[test]
fn h_ui_68_print_mode_default_columns() {
    let (mut harness, _) = sobre_p();
    ribbon_click(&mut harness, RibbonTab::Project, &fr(TextId::MenuPrintMode));
    let state = page_state(harness.state());
    assert!(state.print_mode);
    assert_eq!(state.page.columns, Some(TableColumn::PRINT_DEFAULT.to_vec()));
    // Titre de la fenêtre (même texte que le bouton de la barre d'outils).
    assert_eq!(harness.query_all_by_label(&fr(TextId::PrintModeTitle)).count(), 2);
}

/// H-UI-68 : ajustement « 1 page en largeur » : toutes les pages de
/// l'aperçu sont dans la première colonne de pages.
#[test]
fn h_ui_68_print_mode_fit_one_page_wide() {
    let (mut harness, _) = sobre_p();
    harness
        .state_mut()
        .dispatch(Action::ShowDialog(DialogKind::PageSetup { print_mode: true }));
    harness.run();
    let state = page_state(harness.state());
    assert_eq!(state.page.fit, plannercraft_render::style::Fit::PagesWide(1));
    let app = harness.state();
    let session = &app.document.as_ref().unwrap().session;
    let st = state.styled(&effective(app));
    let pages = plannercraft_ui_egui::dialog::page_setup::build_pages(
        session,
        &st,
        plannercraft_automation::text::Language::Fr,
    )
    .unwrap();
    assert_ne!(pages.pages.len(), 0);
    assert!(
        pages.pages.iter().all(|p| p.col == 0),
        "plus d'une page en largeur"
    );
    // Table d'impression : cinq colonnes.
    assert_eq!(pages.scene.table16, (40 + 240 + 64 * 3) * 16);
}

/// H-UI-68 : régler les colonnes d'impression ne touche ni les colonnes de
/// l'écran ni le document ; « Fermer » ne garde rien.
#[test]
fn h_ui_68_print_mode_keeps_screen_columns() {
    let (mut harness, _) = sobre_p();
    let columns = harness.state().state.columns.clone();
    let before = history(harness.state());
    ribbon_click(&mut harness, RibbonTab::Project, &fr(TextId::MenuPrintMode));
    click(&mut harness, &fr(TextId::ColDuration));
    let state = page_state(harness.state());
    assert!(!state.page.columns.unwrap().contains(&TableColumn::Duration));
    click(&mut harness, &fr(TextId::PsClose));
    let app = harness.state();
    assert!(app.view().dialog.is_none());
    assert_eq!(app.state.columns, columns);
    assert_eq!(history(app), before);
    assert!(doc(app).is_none());
}

/// Relecture finale de J8, F1 : le mode impression, ses exports et
/// Fichier → Exporter → PDF/SVG impriment les lignes de la vue affichée
/// (ici le niveau de plan 1 : deux récapitulatives), pas tout le projet.
#[test]
fn print_follows_displayed_rows() {
    use plannercraft_ui_egui::view::task_view::TaskViewOp;
    let (mut harness, saved) = sobre_p();
    harness
        .state_mut()
        .dispatch(Action::TaskView(TaskViewOp::SetOutlineLevel(Some(1))));
    harness.run();
    let shown = crate::filters::shown(harness.state());
    assert_eq!(shown, vec![1, 5]);
    // Aperçu du mode impression : deux lignes.
    ribbon_click(&mut harness, RibbonTab::Project, &fr(TextId::MenuPrintMode));
    let state = page_state(harness.state());
    let pages = state.preview_pages().expect("aperçu calculé");
    let printed: Vec<u32> = pages
        .scene
        .rows
        .iter()
        .filter_map(|r| r.uid.map(TaskUid::get))
        .collect();
    assert_eq!(printed, shown);
    // Export SVG de l'aperçu : les sous-tâches n'y sont pas.
    click(&mut harness, &fr(TextId::PsExportSvg));
    {
        let saved = saved.borrow();
        let svg = String::from_utf8(saved.last().unwrap().bytes.clone()).unwrap();
        assert!(svg.contains("Gros œuvre"), "{svg}");
        assert!(!svg.contains("Fondations"), "sous-tâche imprimée");
    }
    click(&mut harness, &fr(TextId::PsClose));
    // Fichier → Exporter → PDF : même filtre.
    harness.state_mut().dispatch(Action::Export(SaveKind::Pdf));
    harness.run();
    let app = harness.state();
    let session = &app.document.as_ref().unwrap().session;
    let params = plannercraft_automation::command::params::ExportPdfParams {
        path: String::new(),
        style: Some(TemplateId::Sobre.into()),
        page: None,
        range: None,
        rows: None,
    };
    let lang = plannercraft_automation::text::Language::Fr;
    let rows = app.printed_rows();
    assert_eq!(rows.as_ref().map(std::collections::BTreeSet::len), Some(2));
    let filtered = plannercraft_automation::pdf::export_rows(session, &params, rows.as_ref(), lang).unwrap();
    assert_eq!(filtered.report.shown, 2);
    let all = plannercraft_automation::pdf::export(session, &params, lang).unwrap();
    assert_ne!(filtered.bytes, all.bytes);
    let saved = saved.borrow();
    assert_eq!(saved.last().unwrap().kind, SaveKind::Pdf);
    assert_eq!(saved.last().unwrap().bytes, filtered.bytes);
}

/// Aperçu paginé : « Page 1 / N » avec N pages de la pagination ; page
/// suivante.
#[test]
fn page_setup_preview_page_count() {
    let (mut harness, _) = sobre_p();
    harness
        .state_mut()
        .dispatch(Action::ShowDialog(DialogKind::PageSetup { print_mode: false }));
    harness.run();
    // Échelle 400 % : plusieurs pages.
    if let Some(plannercraft_ui_egui::dialog::DialogState {
        body: DialogBody::PageSetup(state),
        ..
    }) = &mut harness.state_mut().view_mut().dialog
    {
        state.page.fit = plannercraft_render::style::Fit::Scale(400);
    }
    harness.run();
    let state = page_state(harness.state());
    let app = harness.state();
    let pages = plannercraft_ui_egui::dialog::page_setup::build_pages(
        &app.document.as_ref().unwrap().session,
        &state.styled(&effective(app)),
        plannercraft_automation::text::Language::Fr,
    )
    .unwrap();
    let n = i64::try_from(pages.pages.len()).unwrap();
    assert!(n >= 2, "{n} page(s)");
    let page_of = |p: i64| {
        label(
            TextId::PsPageOf,
            &[("page", Arg::Count(p)), ("pages", Arg::Count(n))],
        )
    };
    assert!(harness.query_by_label(&page_of(1)).is_some());
    click(&mut harness, &fr(TextId::PsNext));
    assert!(harness.query_by_label(&page_of(2)).is_some());
}

/// « Exporter en PDF… » de la mise en page : PDF de l'aperçu remis au
/// service de fichiers (en mémoire) ; « Enregistrer avec le projet » écrit
/// la mise en page dans le style (une entrée d'annulation).
#[test]
fn page_setup_export_pdf() {
    let (mut harness, saved) = sobre_p();
    harness
        .state_mut()
        .dispatch(Action::ShowDialog(DialogKind::PageSetup { print_mode: true }));
    harness.run();
    click(&mut harness, &fr(TextId::PsExportPdf));
    {
        let saved = saved.borrow();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].kind, SaveKind::Pdf);
        assert!(saved[0].bytes.starts_with(b"%PDF-"));
    }
    let before = history(harness.state());
    click(&mut harness, &fr(TextId::PsSave));
    let app = harness.state();
    assert_eq!(history(app), before + 1);
    let style = doc(app).expect("style écrit");
    assert_eq!(style.base, TemplateId::Sobre);
    assert_eq!(
        style.resolve().page.columns,
        Some(TableColumn::PRINT_DEFAULT.to_vec())
    );
}

// --- Captures de référence du lot L15 (comparées seulement avec
// `PLANNERCRAFT_UI_SNAPSHOTS=1`, voir `common`).

/// Plan de suivi de J6 (`tracking_j6.xml`), modèle par défaut `id`, thème
/// `theme`, zoom Semaine.
fn template_capture(id: TemplateId, theme: ThemePref) -> Harness<'static, App> {
    let state = UiState {
        language: LanguagePref::Fr,
        theme,
        default_template: id,
        ..UiState::default()
    };
    let mut harness = crate::common::harness_with(App::new(state, Env::fixed(now())), theme);
    harness.run();
    let origin = plannercraft_ui_egui::DocOrigin {
        name: "tracking_j6.xml".to_owned(),
        path: None,
        demo: false,
    };
    harness.state_mut().open_bytes(
        include_bytes!("../../../../tests/mcp/expected/tracking_j6.xml"),
        origin,
    );
    harness.run();
    harness
        .state_mut()
        .dispatch(Action::SetZoom(plannercraft_ui_egui::TimeScaleZoom::Week));
    harness.run();
    assert!(harness.state().document.is_some());
    harness
}

/// Une capture par modèle et thème sur `tracking_j6.xml` (un harnais par
/// test : kittest ne fusionne pas les résultats de plusieurs harnais).
macro_rules! template_snapshots {
    ($($name:ident => $id:expr, $theme:expr, $file:literal;)*) => {
        $(
            #[test]
            fn $name() {
                let mut harness = template_capture($id, $theme);
                crate::common::snapshot(&mut harness, $file);
            }
        )*
    };
}

template_snapshots! {
    snapshot_template_classique_light => TemplateId::Classique, ThemePref::Light, "template_classique_light";
    snapshot_template_classique_dark => TemplateId::Classique, ThemePref::Dark, "template_classique_dark";
    snapshot_template_sobre_light => TemplateId::Sobre, ThemePref::Light, "template_sobre_light";
    snapshot_template_sobre_dark => TemplateId::Sobre, ThemePref::Dark, "template_sobre_dark";
    snapshot_template_contraste_eleve_light => TemplateId::ContrasteEleve, ThemePref::Light, "template_contraste_eleve_light";
    snapshot_template_contraste_eleve_dark => TemplateId::ContrasteEleve, ThemePref::Dark, "template_contraste_eleve_dark";
    snapshot_template_presentation_light => TemplateId::Presentation, ThemePref::Light, "template_presentation_light";
    snapshot_template_presentation_dark => TemplateId::Presentation, ThemePref::Dark, "template_presentation_dark";
    snapshot_template_chantier_light => TemplateId::Chantier, ThemePref::Light, "template_chantier_light";
    snapshot_template_chantier_dark => TemplateId::Chantier, ThemePref::Dark, "template_chantier_dark";
    snapshot_template_impression_nb_light => TemplateId::ImpressionNb, ThemePref::Light, "template_impression_nb_light";
    snapshot_template_impression_nb_dark => TemplateId::ImpressionNb, ThemePref::Dark, "template_impression_nb_dark";
}

/// Exemple ouvert (Sobre, clair, français).
fn demo_capture() -> Harness<'static, App> {
    let mut harness = crate::common::harness(LanguagePref::Fr, ThemePref::Light);
    harness.run();
    harness.state_mut().open_demo();
    harness.run();
    harness
}

#[test]
fn snapshot_gallery_fr_light() {
    let mut harness = demo_capture();
    harness
        .state_mut()
        .dispatch(Action::ShowDialog(DialogKind::Gallery));
    harness.run();
    crate::common::snapshot(&mut harness, "gallery_fr_light");
}

#[test]
fn snapshot_page_setup_fr_light() {
    let mut harness = demo_capture();
    harness
        .state_mut()
        .dispatch(Action::ShowDialog(DialogKind::PageSetup { print_mode: false }));
    harness.run();
    crate::common::snapshot(&mut harness, "page_setup_fr_light");
}

#[test]
fn snapshot_print_mode_fr_light() {
    let mut harness = demo_capture();
    harness
        .state_mut()
        .dispatch(Action::ShowDialog(DialogKind::PageSetup { print_mode: true }));
    harness.run();
    crate::common::snapshot(&mut harness, "print_mode_fr_light");
}

#[test]
fn snapshot_format_panel_fr_light() {
    let mut harness = demo_capture();
    open_panel(&mut harness, BarRole::Critical, None);
    crate::common::snapshot(&mut harness, "format_panel_fr_light");
}

#[test]
fn snapshot_ai_panel_fr_light() {
    use plannercraft_ui_egui::view::side_panel::{SideOp, SideTab};
    let mut harness = demo_capture();
    harness
        .state_mut()
        .dispatch(Action::SidePanel(SideOp::Open(SideTab::AiOperations)));
    harness.run();
    crate::common::snapshot(&mut harness, "ai_panel_fr_light");
}

#[test]
fn snapshot_search_bar_fr_light() {
    use plannercraft_ui_egui::view::task_view::TaskViewOp;
    let mut harness = demo_capture();
    harness
        .state_mut()
        .dispatch(Action::TaskView(TaskViewOp::OpenSearch));
    harness.run();
    harness
        .state_mut()
        .dispatch(Action::TaskView(TaskViewOp::SetQuery("fond".to_owned())));
    harness.run();
    crate::common::snapshot(&mut harness, "search_bar_fr_light");
}
