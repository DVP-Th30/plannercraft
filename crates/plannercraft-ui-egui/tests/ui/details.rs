//! Seconde passe UX (ADR-0019 D6) : volet de détails « Formulaire tâche »
//! (H-UI-112, H-UI-113), lot L5.
//!
//! Harnais 1 440 × 900 sur l'exemple (maison : 11 « Fouilles des
//! fondations », prédécesseur 10, successeur 12 ; 8 récapitulative), volet
//! ouvert et haut de 400 points (toutes les lignes à l'écran). La cellule
//! active se pose par `Action::Select` ; les saisies passent par les champs
//! nommés du volet (`retype`) ou, pour les cas de fond, par le brouillon
//! (`view_mut().details`).

use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, PointerButton, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use plannercraft_automation::Batch;
use plannercraft_automation::text::Language as TextLanguage;
use plannercraft_core::{LinkType, TaskUid};
use plannercraft_ui_egui::action::{DetailsOp, PaneOp};
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::state::DetailsForm;
use plannercraft_ui_egui::task_form::TaskFormDraft;
use plannercraft_ui_egui::view::details::{DetailsTransient, sync};
use plannercraft_ui_egui::{Action, App, DocOrigin, EditOrigin, UiState};

use crate::fr;

fn uid(n: u32) -> TaskUid {
    TaskUid::new(n)
}

/// Exemple ouvert, volet de détails affiché (400 points), forme `form`.
fn open(form: DetailsForm) -> Harness<'static, App> {
    let mut app = crate::new_app();
    let demo = plannercraft_ui_egui::document::load(plannercraft_ui_egui::DEMO_MSPDI, crate::now()).unwrap();
    app.open_project(
        demo.session.project().clone(),
        DocOrigin {
            name: "maison.xml".to_owned(),
            path: None,
            demo: false,
        },
    );
    let mut harness = Harness::builder()
        .with_size(vec2(1440.0, 900.0))
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .with_os(egui::os::OperatingSystem::Nix)
        .with_step_dt(1.0 / 60.0)
        .with_max_steps(8)
        .build_ui_state(plannercraft_ui_egui::ui, app);
    harness.run();
    assert!(harness.state().document.is_some(), "exemple ouvert");
    for action in [
        Action::Pane(PaneOp::ShowDetails(true)),
        Action::Pane(PaneOp::SetDetailsHeight(400)),
        Action::Pane(PaneOp::SetDetailsForm(form)),
    ] {
        dispatch(&mut harness, action);
    }
    harness
}

fn dispatch(harness: &mut Harness<'static, App>, action: Action) {
    harness.state_mut().dispatch(action);
    harness.run();
}

fn select(harness: &mut Harness<'static, App>, n: Option<u32>) {
    dispatch(harness, Action::Select(n.map(uid)));
}

fn details(app: &App) -> &DetailsTransient {
    app.view().details.as_ref().expect("volet synchronisé")
}

fn draft(app: &App) -> &TaskFormDraft {
    details(app).draft.as_ref().expect("brouillon")
}

fn history_len(app: &App) -> usize {
    app.document.as_ref().unwrap().session.history().entries.len()
}

fn task_name(app: &App, n: u32) -> String {
    app.document
        .as_ref()
        .unwrap()
        .project()
        .tasks
        .get(uid(n))
        .unwrap()
        .name
        .clone()
}

/// Lien `pred` → `succ` présent.
fn linked(app: &App, pred: u32, succ: u32) -> bool {
    let project = app.document.as_ref().unwrap().project();
    project
        .tasks
        .get(uid(succ))
        .is_some_and(|t| t.predecessors.iter().any(|l| l.predecessor == uid(pred)))
}

fn fr_args(id: TextId, args: &[(&str, Arg<'_>)]) -> String {
    Texts::new(Language::Fr).get_args(id, args)
}

/// Nom accessible d'une cellule d'une grille du volet.
fn cell(grid: TextId, column: TextId, row: usize) -> String {
    let column = format!("{} {}", fr(grid), fr(column));
    fr_args(
        TextId::TaskInfoRowField,
        &[
            ("column", Arg::Str(column.into())),
            ("count", Arg::Count(i64::try_from(row).unwrap())),
        ],
    )
}

/// Champ de texte du volet nommé `label` (le seul sous ce nom dans le
/// volet).
fn text_value(harness: &Harness<'static, App>, label: &str) -> String {
    harness
        .get_by_role_and_label(Role::TextInput, label)
        .value()
        .unwrap_or_default()
}

/// Remplace le texte du champ nommé `label` par `text`.
fn retype(harness: &mut Harness<'static, App>, label: &str, text: &str) {
    harness.get_by_role_and_label(Role::TextInput, label).focus();
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.event(Event::Text(text.to_owned()));
    harness.run();
}

/// Textes peints dans le volet de détails.
fn painted(harness: &Harness<'static, App>) -> Vec<String> {
    let pane = harness.state().view().panes.drawn.unwrap().details.unwrap();
    harness
        .output()
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::epaint::Shape::Text(t) if pane.contains(t.pos) => Some(t.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

/// Clic droit au milieu du volet, sous les grilles.
fn right_click_pane(harness: &mut Harness<'static, App>) {
    let pane = harness.state().view().panes.drawn.unwrap().details.unwrap();
    let p = egui::pos2(pane.right() - 20.0, pane.bottom() - 10.0);
    for pressed in [true, false] {
        harness.event(Event::PointerMoved(p));
        harness.event(Event::PointerButton {
            pos: p,
            button: PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        harness.run();
    }
}

/// H-UI-112 (DET-01) : le volet suit la cellule active ; ligne vide ou
/// rien ⇒ « Sélectionnez une tâche ».
#[test]
fn h_ui_112_follows_active_cell() {
    let mut harness = open(DetailsForm::default());
    select(&mut harness, None);
    assert!(details(harness.state()).draft.is_none());
    assert!(painted(&harness).contains(&fr(TextId::DetailsSelectTask)));
    select(&mut harness, Some(11));
    let d = draft(harness.state());
    assert_eq!(d.uid, uid(11));
    assert_eq!(d.general.name, "Fouilles des fondations");
    assert_eq!(
        text_value(&harness, &fr(TextId::DetailsName)),
        "Fouilles des fondations"
    );
    assert!(!painted(&harness).contains(&fr(TextId::DetailsSelectTask)));
    select(&mut harness, Some(12));
    assert_eq!(
        text_value(&harness, &fr(TextId::DetailsName)),
        "Coulage des fondations"
    );
    // Tâche absente (UID inconnu) : invite.
    select(&mut harness, Some(9_999));
    assert!(details(harness.state()).draft.is_none());
    assert_eq!(history_len(harness.state()), 0);
}

/// H-UI-112 : forme « Prédécesseurs et successeurs » sur 11 : 10 FD sans
/// décalage, successeur 12 FD, noms lus.
#[test]
fn h_ui_112_shows_successors() {
    let mut harness = open(DetailsForm::PredecessorsSuccessors);
    select(&mut harness, Some(11));
    let d = draft(harness.state());
    assert_eq!(d.links.len(), 1);
    assert_eq!(
        (d.links[0].id.as_str(), d.links[0].kind, d.links[0].lag.as_str()),
        ("10", LinkType::FinishToStart, "")
    );
    assert_eq!(d.successors.len(), 1);
    assert_eq!(
        (d.successors[0].id.as_str(), d.successors[0].kind),
        ("12", LinkType::FinishToStart)
    );
    assert_eq!(
        text_value(
            &harness,
            &cell(TextId::DetailsPredecessors, TextId::DetailsColId, 1)
        ),
        "10"
    );
    assert_eq!(
        text_value(
            &harness,
            &cell(TextId::DetailsSuccessors, TextId::DetailsColId, 1)
        ),
        "12"
    );
    let shown = painted(&harness);
    for name in ["Décapage et terrassement", "Coulage des fondations"] {
        assert!(shown.iter().any(|t| t == name), "{name} absent de {shown:?}");
    }
    // 12 a deux successeurs (13 et 14), dans l'ordre des ID.
    select(&mut harness, Some(12));
    let ids: Vec<&str> = draft(harness.state())
        .successors
        .iter()
        .map(|r| r.id.as_str())
        .collect();
    assert_eq!(ids, ["13", "14"]);
}

/// H-UI-112 : clic droit dans le volet ⇒ les trois formes ; la forme
/// choisie est gardée dans `UiState` et change les grilles.
#[test]
fn h_ui_112_forms_switch() {
    let mut harness = open(DetailsForm::default());
    select(&mut harness, Some(11));
    let resources = cell(TextId::DetailsResources, TextId::DetailsColName, 1);
    let preds = cell(TextId::DetailsPredecessors, TextId::DetailsColId, 1);
    let succs = cell(TextId::DetailsSuccessors, TextId::DetailsColId, 1);
    let has = |h: &Harness<'static, App>, label: &str| h.query_by_label(label).is_some();
    assert!(has(&harness, &resources) && has(&harness, &preds) && !has(&harness, &succs));
    for (form, text, want) in [
        (
            DetailsForm::ResourcesSuccessors,
            TextId::DetailsFormResourcesSuccessors,
            [true, false, true],
        ),
        (
            DetailsForm::PredecessorsSuccessors,
            TextId::DetailsFormPredecessorsSuccessors,
            [false, true, true],
        ),
        (
            DetailsForm::ResourcesPredecessors,
            TextId::DetailsFormResourcesPredecessors,
            [true, true, false],
        ),
    ] {
        right_click_pane(&mut harness);
        crate::click(&mut harness, &fr(text));
        assert_eq!(harness.state().state.details.form, form);
        let got = [
            has(&harness, &resources),
            has(&harness, &preds),
            has(&harness, &succs),
        ];
        assert_eq!(got, want, "{form:?}");
    }
    // Gardée après relance.
    dispatch(
        &mut harness,
        Action::Pane(PaneOp::SetDetailsForm(DetailsForm::PredecessorsSuccessors)),
    );
    let (read, _) = UiState::from_json_lenient(&harness.state().state.to_json().unwrap());
    assert_eq!(read.details.form, DetailsForm::PredecessorsSuccessors);
    assert_eq!(history_len(harness.state()), 0);
}

/// H-UI-112 (H-UI-73) : récapitulative ⇒ durée, dates, % achevé, type et
/// cases grisés ; le nom reste modifiable.
#[test]
fn h_ui_112_summary_greyed() {
    let mut harness = open(DetailsForm::default());
    select(&mut harness, Some(8));
    assert!(draft(harness.state()).summary);
    for label in [
        TextId::DetailsDuration,
        TextId::DetailsStart,
        TextId::DetailsFinish,
        TextId::DetailsPercent,
    ] {
        let name = fr(label);
        let node = harness.get_by_role_and_label(Role::TextInput, &name);
        assert!(node.accesskit_node().is_disabled(), "{label:?} modifiable");
    }
    assert!(
        !harness
            .get_by_role_and_label(Role::TextInput, &fr(TextId::DetailsName))
            .accesskit_node()
            .is_disabled()
    );
    assert!(
        harness
            .get_by_label(&fr(TextId::DetailsEffortDriven))
            .accesskit_node()
            .is_disabled()
    );
    assert!(
        harness
            .get_by_label(&fr(TextId::DetailsManual))
            .accesskit_node()
            .is_disabled()
    );
    // Feuille : modifiable.
    select(&mut harness, Some(11));
    assert!(
        !harness
            .get_by_role_and_label(Role::TextInput, &fr(TextId::DetailsDuration))
            .accesskit_node()
            .is_disabled()
    );
}

/// H-UI-113 : ligne vide en bas de chaque grille ; la remplir ajoute une
/// ligne au brouillon (et une nouvelle ligne vide).
#[test]
fn h_ui_112_blank_row_placeholder() {
    let mut harness = open(DetailsForm::PredecessorsSuccessors);
    select(&mut harness, Some(11));
    let blank = cell(TextId::DetailsSuccessors, TextId::DetailsColId, 2);
    assert_eq!(text_value(&harness, &blank), "");
    assert!(
        harness
            .query_by_label(&cell(TextId::DetailsSuccessors, TextId::DetailsColId, 3))
            .is_none()
    );
    retype(&mut harness, &blank, "14");
    let st = details(harness.state());
    assert!(st.dirty);
    let ids: Vec<&str> = draft(harness.state())
        .successors
        .iter()
        .map(|r| r.id.as_str())
        .collect();
    assert_eq!(ids, ["12", "14"]);
    assert!(
        harness
            .query_by_label(&cell(TextId::DetailsSuccessors, TextId::DetailsColId, 3))
            .is_some()
    );
    assert!(
        painted(&harness).iter().any(|t| t == "Réseaux enterrés"),
        "nom lu de 14"
    );
    // Rien d'écrit avant OK.
    assert_eq!(history_len(harness.state()), 0);
}

/// H-UI-113 : nom, durée et successeur changés puis OK ⇒ **une** entrée
/// libellée « Formulaire tâche » ; Ctrl+Z ⇒ tout revient.
#[test]
fn h_ui_113_ok_one_undo() {
    let mut harness = open(DetailsForm::PredecessorsSuccessors);
    select(&mut harness, Some(11));
    retype(&mut harness, &fr(TextId::DetailsName), "Fouilles");
    retype(&mut harness, &fr(TextId::DetailsDuration), "4j");
    retype(
        &mut harness,
        &cell(TextId::DetailsSuccessors, TextId::DetailsColId, 2),
        "14",
    );
    crate::click(&mut harness, &fr(TextId::DetailsOk));
    let app = harness.state();
    assert_eq!(history_len(app), 1, "une entrée");
    let entries = &app.document.as_ref().unwrap().session.history().entries;
    assert_eq!(
        entries[0].label.as_deref(),
        Some(fr(TextId::DetailsBatchLabel).as_str())
    );
    assert_eq!(task_name(app, 11), "Fouilles");
    let minutes = app
        .document
        .as_ref()
        .unwrap()
        .project()
        .tasks
        .get(uid(11))
        .unwrap()
        .duration
        .minutes()
        .get();
    assert_eq!(minutes, 4 * 480);
    assert!(linked(app, 11, 14));
    assert!(!details(app).dirty, "brouillon relu");
    assert_eq!(draft(app).successors.len(), 2);
    // Annuler (Édition → Annuler) : tout revient.
    harness.state_mut().undo().unwrap();
    harness.run();
    let app = harness.state();
    assert_eq!(task_name(app, 11), "Fouilles des fondations");
    assert!(!linked(app, 11, 14));
    assert_eq!(draft(app).general.name, "Fouilles des fondations", "volet relu");
}

/// H-UI-113 : Annuler (et Échap dans un champ) ⇒ brouillon relu, rien
/// d'écrit.
#[test]
fn h_ui_113_cancel_restores() {
    let mut harness = open(DetailsForm::default());
    select(&mut harness, Some(11));
    retype(&mut harness, &fr(TextId::DetailsName), "Autre nom");
    assert!(details(harness.state()).dirty);
    crate::click(&mut harness, &fr(TextId::DetailsCancel));
    assert!(!details(harness.state()).dirty);
    assert_eq!(
        text_value(&harness, &fr(TextId::DetailsName)),
        "Fouilles des fondations"
    );
    // Échap dans un champ.
    retype(&mut harness, &fr(TextId::DetailsDuration), "9j");
    harness.key_press(Key::Escape);
    harness.run();
    assert!(!details(harness.state()).dirty);
    assert_eq!(
        draft(harness.state()).general.duration,
        draft_of(&harness, 11).general.duration
    );
    assert_eq!(history_len(harness.state()), 0);
}

/// Brouillon relu de la tâche `n` sur la session.
fn draft_of(harness: &Harness<'static, App>, n: u32) -> TaskFormDraft {
    let doc = harness.state().document.as_ref().unwrap();
    TaskFormDraft::from_session(&doc.session, uid(n), doc.cache_key(), TextLanguage::Fr).unwrap()
}

/// H-UI-113 : Entrée dans un champ du haut ⇒ OK ; changer de cellule
/// active avec un brouillon modifié ⇒ OK implicite, puis le volet suit.
#[test]
fn h_ui_113_selection_change_commits() {
    let mut harness = open(DetailsForm::default());
    select(&mut harness, Some(11));
    retype(&mut harness, &fr(TextId::DetailsName), "Fouilles (Entrée)");
    harness.key_press(Key::Enter);
    harness.run();
    assert_eq!(task_name(harness.state(), 11), "Fouilles (Entrée)");
    assert_eq!(history_len(harness.state()), 1);
    retype(&mut harness, &fr(TextId::DetailsName), "Fouilles creusées");
    select(&mut harness, Some(12));
    harness.run();
    let app = harness.state();
    assert_eq!(task_name(app, 11), "Fouilles creusées");
    assert_eq!(history_len(app), 2, "une entrée de plus");
    assert_eq!(app.selection, Some(uid(12)));
    assert_eq!(draft(app).uid, uid(12));
    assert!(!details(app).dirty);
}

/// H-UI-113 (H-UI-86) : successeur 99 inconnu, puis autre cellule active ⇒
/// refus nommé « Il n'y a pas de tâche 99. », sélection gardée sur 11,
/// rien d'écrit.
#[test]
fn h_ui_113_refusal_keeps_selection() {
    let mut harness = open(DetailsForm::PredecessorsSuccessors);
    select(&mut harness, Some(11));
    retype(
        &mut harness,
        &cell(TextId::DetailsSuccessors, TextId::DetailsColId, 2),
        "99",
    );
    select(&mut harness, Some(12));
    harness.run();
    let app = harness.state();
    let want = fr_args(TextId::DetailsUnknownTask, &[("id", Arg::Count(99))]);
    assert_eq!(app.selection, Some(uid(11)), "sélection gardée");
    assert_eq!(draft(app).uid, uid(11));
    assert!(details(app).dirty, "brouillon gardé");
    assert!(
        details(app)
            .errors
            .iter()
            .any(|e| e.message == want && e.field == "successor"),
        "{:?}",
        details(app).errors
    );
    assert!(
        app.messages().iter().any(|m| m.text == want),
        "{:?}",
        app.messages()
    );
    assert_eq!(history_len(app), 0);
    // Sous le champ et en notification.
    assert_eq!(harness.query_all_by_label(&want).count(), 2);
}

/// H-UI-113 (R-260 d) : OK sans changement, ou lien réécrit à l'identique
/// (« 10 » sur 10), ⇒ aucune entrée, aucun message.
#[test]
fn h_ui_113_identical_noop() {
    let mut harness = open(DetailsForm::PredecessorsSuccessors);
    select(&mut harness, Some(11));
    let before = harness.state().messages().len();
    crate::click(&mut harness, &fr(TextId::DetailsOk));
    retype(
        &mut harness,
        &cell(TextId::DetailsPredecessors, TextId::DetailsColId, 1),
        "10",
    );
    let duration = draft_of(&harness, 11).general.duration;
    retype(&mut harness, &fr(TextId::DetailsDuration), &duration);
    assert!(!details(harness.state()).dirty);
    crate::click(&mut harness, &fr(TextId::DetailsOk));
    // Brouillon modifié puis remis : pas de lot non plus.
    let st = harness.state_mut().view_mut().details.as_mut().unwrap();
    st.dirty = true;
    harness.state_mut().dispatch(Action::Details(DetailsOp::Ok));
    harness.run();
    let app = harness.state();
    assert_eq!(history_len(app), 0);
    assert_eq!(app.messages().len(), before);
    assert_eq!(
        details(app).errors,
        [] as [plannercraft_ui_egui::task_form::FieldError; 0]
    );
}

/// H-UI-113 : grille Ressources : ressource choisie sur la ligne vide,
/// unités, puis travail ; chaque OK = une entrée.
#[test]
fn h_ui_113_resources_units_work() {
    let mut harness = open(DetailsForm::default());
    let add: Batch = serde_json::from_value(serde_json::json!({
        "format_version": 1,
        "ops": [{ "op": "resource.add", "name": "Maçon" }]
    }))
    .unwrap();
    harness.state_mut().execute(&add, EditOrigin::Agent).unwrap();
    select(&mut harness, Some(11));
    let entries = history_len(harness.state());
    crate::click(
        &mut harness,
        &cell(TextId::DetailsResources, TextId::DetailsColName, 1),
    );
    crate::click(&mut harness, "Maçon");
    assert_eq!(draft(harness.state()).assignments.len(), 1);
    retype(
        &mut harness,
        &cell(TextId::DetailsResources, TextId::DetailsColUnits, 1),
        "50",
    );
    crate::click(&mut harness, &fr(TextId::DetailsOk));
    let app = harness.state();
    assert_eq!(history_len(app), entries + 1);
    let project = app.document.as_ref().unwrap().project();
    let a = project
        .assignments
        .iter()
        .find(|a| a.task == uid(11))
        .expect("affectée");
    assert_eq!(a.units.get(), 5_000, "50 %");
    let units = text_value(
        &harness,
        &cell(TextId::DetailsResources, TextId::DetailsColUnits, 1),
    );
    assert_eq!(units, "50%");
    // Travail : 16 h.
    retype(
        &mut harness,
        &cell(TextId::DetailsResources, TextId::DetailsColWork, 1),
        "16h",
    );
    crate::click(&mut harness, &fr(TextId::DetailsOk));
    let app = harness.state();
    assert_eq!(history_len(app), entries + 2);
    let project = app.document.as_ref().unwrap().project();
    let a = project.assignments.iter().find(|a| a.task == uid(11)).unwrap();
    assert_eq!(a.work.minutes().get(), 16 * 60);
    // Retirer : affectation supprimée au OK.
    crate::click(&mut harness, &fr(TextId::TaskInfoRemoveRow));
    crate::click(&mut harness, &fr(TextId::DetailsOk));
    let project = harness.state().document.as_ref().unwrap().project();
    assert!(project.assignments.iter().all(|a| a.task != uid(11)));
}

/// H-UI-113 : document modifié ailleurs (agent, annuler) : brouillon non
/// modifié relu ; modifié gardé (champs touchés seulement) avec
/// l'avertissement.
#[test]
fn h_ui_113_external_change_reloads() {
    let mut harness = open(DetailsForm::default());
    select(&mut harness, Some(11));
    let rename = |name: &str| -> Batch {
        serde_json::from_value(serde_json::json!({
            "format_version": 1,
            "ops": [{ "op": "task.update", "uid": 11, "name": name }]
        }))
        .unwrap()
    };
    harness
        .state_mut()
        .execute(&rename("Par l'agent"), EditOrigin::Agent)
        .unwrap();
    harness.run();
    assert_eq!(draft(harness.state()).general.name, "Par l'agent");
    assert!(!details(harness.state()).kept);
    // Brouillon modifié (durée), puis l'agent renomme encore.
    retype(&mut harness, &fr(TextId::DetailsDuration), "6j");
    harness
        .state_mut()
        .execute(&rename("Encore l'agent"), EditOrigin::Agent)
        .unwrap();
    harness.run();
    let st = details(harness.state());
    assert!(st.kept && st.dirty);
    assert_eq!(
        draft(harness.state()).general.name,
        "Encore l'agent",
        "champ non touché relu"
    );
    assert_eq!(draft(harness.state()).general.duration, "6j", "saisie gardée");
    assert!(harness.query_by_label(&fr(TextId::DetailsDraftKept)).is_some());
    // OK : seule la durée part.
    crate::click(&mut harness, &fr(TextId::DetailsOk));
    let app = harness.state();
    assert_eq!(task_name(app, 11), "Encore l'agent");
    assert_eq!(
        app.document
            .as_ref()
            .unwrap()
            .project()
            .tasks
            .get(uid(11))
            .unwrap()
            .duration
            .minutes()
            .get(),
        6 * 480
    );
    assert!(!details(app).kept);
}

/// `sync` sans egui : OK implicite demandé seulement pour un brouillon
/// modifié d'une tâche encore présente.
#[test]
fn details_sync_pure() {
    let doc = plannercraft_ui_egui::document::load(plannercraft_ui_egui::DEMO_MSPDI, crate::now()).unwrap();
    let s = &doc.session;
    let key = (1, s.generation());
    let mut state = None;
    assert!(!sync(&mut state, s, Some(uid(11)), key, TextLanguage::Fr));
    assert_eq!(state.as_ref().unwrap().draft.as_ref().unwrap().uid, uid(11));
    assert!(!sync(&mut state, s, Some(uid(12)), key, TextLanguage::Fr));
    let st = state.as_mut().unwrap();
    st.draft.as_mut().unwrap().general.name = "x".to_owned();
    st.dirty = true;
    assert!(
        sync(&mut state, s, Some(uid(11)), key, TextLanguage::Fr),
        "OK implicite"
    );
    assert_eq!(
        state.as_ref().unwrap().draft.as_ref().unwrap().uid,
        uid(12),
        "gardé"
    );
    assert!(sync(&mut state, s, None, key, TextLanguage::Fr));
}

/// Refus en langage d'utilisateur (relecture finale ux2, B1) : texte d'un
/// refus du registre sans code, UID, « (opération n) » ni « erreur(s) ».
fn assert_user_language(text: &str) {
    for jargon in ["opération", "erreur(s)", "UID", "lot refusé", "link."] {
        assert!(!text.contains(jargon), "jargon « {jargon} » dans « {text} »");
    }
}

/// H-UI-105 (relecture finale ux2, B1) : 12 saisie en prédécesseur de 11,
/// déjà son successeur, puis OK ⇒ boucle nommée par les ID des tâches,
/// sous les champs et en notification (refus, détail repliable), rien
/// d'écrit.
#[test]
fn h_ui_105_details_refusal_user_language() {
    let mut harness = open(DetailsForm::PredecessorsSuccessors);
    select(&mut harness, Some(11));
    retype(
        &mut harness,
        &cell(TextId::DetailsPredecessors, TextId::DetailsColId, 2),
        "12",
    );
    crate::click(&mut harness, &fr(TextId::DetailsOk));
    let app = harness.state();
    let want = fr_args(TextId::RefusalLinkCycle, &[("tasks", Arg::Str("11, 12".into()))]);
    assert_eq!(history_len(app), 0, "rien d'écrit");
    assert!(details(app).dirty, "brouillon gardé");
    let errors = &details(app).errors;
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].message, want);
    assert_user_language(&errors[0].message);
    let message = app
        .messages()
        .iter()
        .find(|m| m.text == want)
        .unwrap_or_else(|| panic!("{:?}", app.messages()));
    assert_eq!(message.severity, plannercraft_core::Severity::Error, "refus (M2)");
    assert_eq!(message.title, Some(TextId::CommandErrorTitle));
    assert!(
        message.detail.as_deref().is_some_and(|d| d.contains("batch.")),
        "détail technique repliable : {:?}",
        message.detail
    );
}

/// Masquer le volet (×, Affichage) avec un brouillon modifié : OK implicite
/// (relecture finale ux2, M3) ; refus ⇒ le volet reste affiché.
#[test]
fn h_ui_113_hide_commits_draft() {
    let mut harness = open(DetailsForm::default());
    select(&mut harness, Some(11));
    retype(&mut harness, &fr(TextId::DetailsName), "Fouilles masquées");
    dispatch(&mut harness, Action::Pane(PaneOp::ShowDetails(false)));
    let app = harness.state();
    assert!(!app.state.details.visible);
    assert_eq!(task_name(app, 11), "Fouilles masquées");
    assert_eq!(history_len(app), 1);
    // Refus : le volet reste, brouillon et refus gardés.
    dispatch(&mut harness, Action::Pane(PaneOp::ShowDetails(true)));
    retype(&mut harness, &fr(TextId::DetailsDuration), "abc");
    dispatch(&mut harness, Action::Pane(PaneOp::ShowDetails(false)));
    let app = harness.state();
    assert!(app.state.details.visible, "volet gardé");
    assert!(details(app).dirty);
    assert_ne!(details(app).errors.len(), 0, "refus affiché");
    assert_eq!(history_len(app), 1);
}

/// H-UI-112 (relecture finale ux2, C5 ; constat 1 du lot L11) : à la
/// hauteur par défaut, dans une fenêtre de 1 440 × 900, la première ligne
/// des deux grilles de la forme par défaut est entièrement dans le volet
/// (pas de défilement ni de séparateur à tirer).
#[test]
fn h_ui_112_default_height_shows_grids() {
    let mut harness = open(DetailsForm::default());
    dispatch(
        &mut harness,
        Action::Pane(PaneOp::SetDetailsHeight(
            plannercraft_ui_egui::state::DETAILS_HEIGHT_DEFAULT,
        )),
    );
    select(&mut harness, Some(11));
    let pane = harness.state().view().panes.drawn.unwrap().details.unwrap();
    for (grid, column) in [
        (TextId::DetailsResources, TextId::DetailsColName),
        (TextId::DetailsPredecessors, TextId::DetailsColId),
    ] {
        let label = cell(grid, column, 1);
        let rect = harness.get_by_label(&label).rect();
        assert!(
            rect.top() >= pane.top() && rect.bottom() <= pane.bottom(),
            "{label} : {rect:?} hors du volet {pane:?}"
        );
    }
}
