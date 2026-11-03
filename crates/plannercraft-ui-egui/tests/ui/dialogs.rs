//! Fenêtres du lot L12 (J7) : « Modifier le temps de travail » (mois lu sur
//! le moteur, exceptions nommées, semaine type héritée, Nouveau et
//! Supprimer), « Informations sur le projet », « Ligne de base » (Définir
//! avec confirmation du remplacement, Effacer sur la sélection),
//! « Replanifier le travail restant » ; un seul lot par « OK », touches
//! retenues par la fenêtre, sous-menu Projet, capture `working_time_fr_light`.

use egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_automation::text::{Language as TextLanguage, TextContext, format_date};
use plannercraft_automation::{ApplyMode, Batch, Command, Session};
use plannercraft_core::calendar::{CalendarKind, DayRule};
use plannercraft_core::{BaselineNumber, CalendarUid, TaskUid, Weekday};
use plannercraft_ui_egui::action::DialogKind;
use plannercraft_ui_egui::dialog::baseline::BaselineState;
use plannercraft_ui_egui::dialog::project_info::ProjectInfoState;
use plannercraft_ui_egui::dialog::reschedule::RescheduleState;
use plannercraft_ui_egui::dialog::working_time::{
    DayChoice, DayMark, ExceptionRow, WorkingTimeState, day_choices, ranges_text, week_order,
};
use plannercraft_ui_egui::dialog::{DialogBody, DialogState};
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::{Action, App, LanguagePref, ThemePref};

use crate::common;
use crate::{fr, harness, open_demo};

fn uid(n: u32) -> TaskUid {
    TaskUid::new(n)
}

fn generation(app: &App) -> u64 {
    app.document.as_ref().unwrap().generation()
}

fn history_entries(app: &App) -> usize {
    app.document.as_ref().unwrap().session.history().entries.len()
}

fn session(app: &App) -> &Session {
    &app.document.as_ref().unwrap().session
}

fn click_label(harness: &mut Harness<'static, App>, label: &str) {
    crate::click(harness, label);
}

/// Remplace le texte du champ nommé `label` par `text`.
fn retype(harness: &mut Harness<'static, App>, label: &str, text: &str) {
    harness
        .get_by_role_and_label(egui::accesskit::Role::TextInput, label)
        .focus();
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.key_press(Key::Backspace);
    if !text.is_empty() {
        harness.event(Event::Text(text.to_owned()));
    }
    harness.run();
}

/// Ouvre la liste nommée `combo` et choisit l'entrée `entry`.
fn choose(harness: &mut Harness<'static, App>, combo: &str, entry: &str) {
    open_combo(harness, combo);
    click_label(harness, entry);
}

/// Ouvre la liste nommée `combo`.
fn open_combo(harness: &mut Harness<'static, App>, combo: &str) {
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, combo)
        .click();
    harness.run();
}

fn open(harness: &mut Harness<'static, App>, kind: DialogKind) {
    harness.state_mut().dispatch(Action::ShowDialog(kind));
    harness.run();
}

fn body(app: &App) -> Option<&DialogBody> {
    app.view().dialog.as_ref().map(|d| &d.body)
}

fn wt(app: &App) -> Option<&WorkingTimeState> {
    match body(app) {
        Some(DialogBody::WorkingTime(s)) => Some(s),
        _ => None,
    }
}

fn pi(app: &App) -> Option<&ProjectInfoState> {
    match body(app) {
        Some(DialogBody::ProjectInfo(s)) => Some(s),
        _ => None,
    }
}

fn bl(app: &App) -> Option<&BaselineState> {
    match body(app) {
        Some(DialogBody::Baseline(s)) => Some(s),
        _ => None,
    }
}

fn rs(app: &App) -> Option<&RescheduleState> {
    match body(app) {
        Some(DialogBody::Reschedule(s)) => Some(s),
        _ => None,
    }
}

/// Applique un lot JSON comme un agent (aucun message).
fn agent(harness: &mut Harness<'static, App>, ops: &serde_json::Value) {
    let batch: Batch =
        serde_json::from_value(serde_json::json!({ "format_version": 1, "ops": ops })).unwrap();
    harness
        .state_mut()
        .execute(&batch, plannercraft_ui_egui::EditOrigin::Agent)
        .unwrap();
    harness.run();
}

/// Libellé « champ, ligne n » d'une ligne de fenêtre.
fn row_field(field: TextId, row: usize) -> String {
    Texts::new(Language::Fr).get_args(
        TextId::TaskInfoRowField,
        &[
            ("column", Arg::Str(fr(field).into())),
            ("count", Arg::Count(i64::try_from(row).unwrap())),
        ],
    )
}

/// Le mois affiché est celui du calendrier compilé du moteur, jour par
/// jour ; les exceptions du fichier sont marquées ; le mois suivant est
/// relu.
#[test]
fn working_time_month_from_engine() {
    let mut harness = harness();
    open_demo(&mut harness);
    open(&mut harness, DialogKind::WorkingTime);
    let app = harness.state();
    let state = wt(app).expect("fenêtre ouverte");
    let project = session(app).project();
    assert_eq!(state.selected, project.calendar);
    // Mois de la date d'état (15/01/2027).
    assert_eq!((state.year, state.month), (2027, 1));
    let cache = state.month_view.as_ref().expect("mois lu");
    assert_eq!(
        (cache.calendar, cache.year, cache.month),
        (project.calendar, 2027, 1)
    );
    let view = cache.view.as_ref().unwrap();
    assert_eq!(view.days.len(), 31);
    let set = app.document.as_ref().unwrap().calendars();
    let compiled = set.get(project.calendar).unwrap();
    for d in &view.days {
        let ranges = compiled.day_ranges(d.day).unwrap();
        assert_eq!(d.ranges, ranges_text(ranges), "jour {}", d.number);
        let working = matches!(d.mark, DayMark::Working | DayMark::ExceptionWorking);
        assert_eq!(working, !ranges.is_empty(), "jour {}", d.number);
    }
    // 1er janvier : exception « Jour de l'an » ; samedi 2 : chômé ; lundi 4 :
    // ouvré sur les plages du Standard.
    assert_eq!(view.days[0].mark, DayMark::ExceptionNonWorking);
    assert_eq!(view.days[1].mark, DayMark::NonWorking);
    assert_eq!(view.days[3].mark, DayMark::Working);
    assert_eq!(view.days[3].ranges, "08:00-12:00, 13:00-17:00");
    // Première case : un vendredi, dans la colonne du vendredi.
    assert_eq!(view.days[0].day.weekday(), Weekday::Friday);
    assert_eq!(
        week_order(project.options.week_start_day)[view.lead],
        Weekday::Friday
    );
    click_label(&mut harness, &fr(TextId::WtNextMonth));
    let state = wt(harness.state()).unwrap();
    assert_eq!((state.year, state.month), (2027, 2));
    let cache = state.month_view.as_ref().unwrap();
    assert_eq!(cache.month, 2);
    assert_eq!(cache.view.as_ref().unwrap().days.len(), 28);
    assert_eq!(generation(harness.state()), 0, "aucune écriture");
}

/// Exception nommée ajoutée par la fenêtre : un seul lot, nom gardé,
/// annulée par un seul « Annuler » ; le mois la montre ensuite.
#[test]
fn working_time_exception_named() {
    let mut harness = harness();
    open_demo(&mut harness);
    let before = session(harness.state()).project().clone();
    open(&mut harness, DialogKind::WorkingTime);
    click_label(&mut harness, &fr(TextId::WtExcAdd));
    // Quatre exceptions dans l'exemple : la nouvelle est la ligne 5.
    retype(
        &mut harness,
        &row_field(TextId::WtExcName, 5),
        "Pont de l'Ascension",
    );
    retype(&mut harness, &row_field(TextId::WtExcFrom, 5), "07/05/2027");
    retype(&mut harness, &row_field(TextId::WtExcTo, 5), "08/05/2027");
    click_label(&mut harness, &fr(TextId::TaskInfoOk));
    let app = harness.state();
    assert!(
        wt(app).is_none(),
        "fenêtre fermée : {:?}",
        wt(app).map(|s| &s.errors)
    );
    assert!(app.messages().is_empty(), "{:?}", app.messages());
    assert_eq!(history_entries(app), 1, "un seul lot");
    let project = session(app).project();
    let cal = project.calendars.get(project.calendar).unwrap();
    let added = cal
        .exceptions()
        .iter()
        .find(|e| e.name == "Pont de l'Ascension")
        .expect("exception nommée");
    assert_eq!(added.period.first().to_string(), "2027-05-07");
    assert_eq!(added.period.last().to_string(), "2027-05-08");
    assert_eq!(cal.exceptions().len(), 5);
    // Le mois de mai la montre (lecture du moteur).
    open(&mut harness, DialogKind::WorkingTime);
    for _ in 0..4 {
        click_label(&mut harness, &fr(TextId::WtNextMonth));
    }
    let state = wt(harness.state()).unwrap();
    let cache = state.month_view.as_ref().unwrap();
    assert_eq!(cache.month, 5);
    let view = cache.view.as_ref().unwrap();
    assert_eq!(view.days[6].mark, DayMark::ExceptionNonWorking);
    assert_eq!(view.days[5].mark, DayMark::Working);
    click_label(&mut harness, &fr(TextId::TaskInfoCancel));
    harness.state_mut().dispatch(Action::Undo);
    harness.run();
    assert_eq!(session(harness.state()).project(), &before);
}

/// Brouillon pur : nom, semaine, exception retirée, exception déplacée ;
/// un seul lot dans l'ordre (nom, semaine, retraits, ajouts) qui
/// s'applique.
#[test]
fn working_time_to_batch_order() {
    let mut s = plannercraft_ui_egui::document::load(plannercraft_ui_egui::DEMO_MSPDI, crate::now())
        .unwrap()
        .session;
    let texts = Texts::new(Language::Fr);
    let mut state = WorkingTimeState::open(&s, (1, 0), TextLanguage::Fr, None);
    assert_eq!(state.to_batch(&s, &texts, TextLanguage::Fr), Ok(None));
    let draft = state.draft_mut().unwrap();
    draft.name = "Chantier".to_owned();
    let saturday = draft
        .week
        .iter_mut()
        .find(|d| d.day == Weekday::Saturday)
        .unwrap();
    saturday.choice = DayChoice::Working;
    saturday.ranges = "08:00-12:00".to_owned();
    // « Noël » retiré, « Armistice » déplacé au 12/11 et ouvré le matin.
    let noel = draft
        .exceptions
        .iter()
        .position(|r| r.fields.name == "Noël")
        .unwrap();
    draft.remove_row(noel);
    let armistice = draft
        .exceptions
        .iter_mut()
        .find(|r| r.fields.name == "Armistice")
        .unwrap();
    armistice.fields.from = "12/11/2026".to_owned();
    armistice.fields.to = String::new();
    armistice.fields.working = true;
    armistice.fields.ranges = "8:00-12:00".to_owned();
    let batch = state.to_batch(&s, &texts, TextLanguage::Fr).unwrap().unwrap();
    let ids: Vec<&str> = batch.ops.iter().map(Command::id).collect();
    assert_eq!(
        ids,
        [
            "calendar.update",
            "calendar.set_week",
            "calendar.remove_exception",
            "calendar.remove_exception",
            "calendar.set_exception",
        ]
    );
    s.apply(&batch, ApplyMode::Commit).unwrap();
    let cal = s.project().calendars.get(CalendarUid::new(1)).unwrap();
    assert_eq!(cal.name, "Chantier");
    assert_eq!(cal.exceptions().len(), 3);
    let moved = cal.exceptions().iter().find(|e| e.name == "Armistice").unwrap();
    assert_eq!(moved.period.first().to_string(), "2026-11-12");
    assert_eq!(moved.period.last().to_string(), "2026-11-12");
    assert_eq!(moved.setting.total().get(), 240);
    // Plages illisibles : refus sur le champ, aucun lot.
    let mut state = WorkingTimeState::open(&s, (1, 1), TextLanguage::Fr, None);
    let draft = state.draft_mut().unwrap();
    let row = draft.add_row();
    draft.exceptions[row].fields.from = "40/40/2027".to_owned();
    draft.exceptions[row].fields.working = true;
    draft.exceptions[row].fields.ranges = "matin".to_owned();
    let errors = state.to_batch(&s, &texts, TextLanguage::Fr).unwrap_err();
    let fields: Vec<&str> = errors.iter().map(|e| e.field).collect();
    assert_eq!(fields, ["from", "ranges"]);
    assert!(errors.iter().all(|e| e.row == row));
    // Une ligne récurrente n'est jamais retirée.
    let mut recurring = ExceptionRow::blank();
    recurring.recurring = true;
    let draft = state.draft_mut().unwrap();
    draft.exceptions.push(recurring);
    let last = draft.exceptions.len() - 1;
    draft.remove_row(last);
    assert_eq!(draft.exceptions.len(), last + 1);
}

/// Calendrier dérivé d'une ressource : « Hérité de la base » proposé et
/// relu ; un jour réglé chômé puis rendu à l'héritage, chacun par
/// `calendar.set_week` ; un calendrier de base ne propose pas l'héritage.
#[test]
fn working_time_week_inherit_on_derived() {
    assert!(day_choices(true).iter().all(|(c, _)| *c != DayChoice::Inherit));
    assert!(day_choices(false).iter().any(|(c, _)| *c == DayChoice::Inherit));
    let mut harness = harness();
    open_demo(&mut harness);
    agent(
        &mut harness,
        &serde_json::json!([{ "op": "resource.add", "name": "Maçon", "calendar": "own" }]),
    );
    let derived = {
        let p = session(harness.state()).project();
        p.resources.iter().next().unwrap().calendar.unwrap()
    };
    let texts = Texts::new(Language::Fr);
    let entry = texts.get_args(TextId::WtResourceCalendar, &[("name", Arg::Str("Maçon".into()))]);
    let monday = texts.format_labels().weekdays_short[1].clone();
    let setting = texts.get_args(
        TextId::WtWeekSetting,
        &[("name", Arg::Str(monday.clone().into()))],
    );
    for (choice, expected) in [
        (TextId::WtWeekNonWorking, DayRule::NonWorking),
        (TextId::WtWeekInherit, DayRule::Inherit),
    ] {
        open(&mut harness, DialogKind::WorkingTime);
        choose(&mut harness, &fr(TextId::WtCalendar), &entry);
        let state = wt(harness.state()).unwrap();
        assert_eq!(state.selected, derived);
        let draft = state.draft().unwrap();
        assert!(!draft.base);
        let day = draft.week.iter().find(|d| d.day == Weekday::Monday).unwrap();
        assert_eq!(day.inherited.as_deref(), Some("08:00-12:00, 13:00-17:00"));
        click_label(&mut harness, &fr(TextId::WtTabWeek));
        choose(&mut harness, &setting, &fr(choice));
        let history = history_entries(harness.state());
        click_label(&mut harness, &fr(TextId::TaskInfoOk));
        let app = harness.state();
        assert!(wt(app).is_none(), "{:?}", wt(app).map(|s| (&s.errors, &s.error)));
        assert_eq!(history_entries(app), history + 1);
        let cal = session(app).project().calendars.get(derived).unwrap();
        let CalendarKind::Derived { week, .. } = cal.kind() else {
            panic!("calendrier dérivé attendu");
        };
        assert_eq!(week.get(Weekday::Monday), &expected);
        assert_eq!(week.get(Weekday::Tuesday), &DayRule::Inherit);
    }
    // Calendrier de base (Standard) : pas d'héritage dans la liste.
    open(&mut harness, DialogKind::WorkingTime);
    click_label(&mut harness, &fr(TextId::WtTabWeek));
    open_combo(&mut harness, &setting);
    assert!(harness.query_by_label(&fr(TextId::WtWeekNonWorking)).is_some());
    assert!(harness.query_by_label(&fr(TextId::WtWeekInherit)).is_none());
}

/// « Supprimer » le calendrier du projet : `calendar.in_use` affiché dans
/// la fenêtre avec ses utilisateurs, rien appliqué ; « Nouveau » crée un
/// calendrier (lot immédiat), qui se supprime ensuite.
#[test]
fn working_time_delete_in_use_message() {
    let mut harness = harness();
    open_demo(&mut harness);
    open(&mut harness, DialogKind::WorkingTime);
    click_label(&mut harness, &fr(TextId::WtDelete));
    let app = harness.state();
    let error = wt(app).unwrap().error.clone().expect("refus affiché");
    assert!(error.starts_with("Calendrier utilisé"), "{error}");
    assert!(error.contains("projet"), "{error}");
    assert!(harness.query_by_label(&error).is_some());
    assert_eq!(generation(app), 0);
    assert!(app.messages().is_empty(), "rien dans le bandeau");
    // Nouveau calendrier de base.
    click_label(&mut harness, &fr(TextId::WtNew));
    retype(&mut harness, &fr(TextId::WtNewName), "Chantier");
    click_label(&mut harness, &fr(TextId::WtCreate));
    let app = harness.state();
    let state = wt(app).expect("fenêtre gardée");
    assert!(state.new_form.is_none());
    assert!(state.error.is_none(), "{:?}", state.error);
    let p = session(app).project();
    let created = p.calendars.iter().find(|c| c.name == "Chantier").unwrap().uid();
    assert_eq!(state.selected, created);
    assert_eq!(state.key, app.document.as_ref().unwrap().cache_key());
    assert_eq!(history_entries(app), 1);
    // Nom déjà pris : refus du registre dans la fenêtre.
    click_label(&mut harness, &fr(TextId::WtNew));
    retype(&mut harness, &fr(TextId::WtNewName), "chantier");
    click_label(&mut harness, &fr(TextId::WtCreate));
    let error = wt(harness.state()).unwrap().error.clone().unwrap();
    assert!(error.starts_with("Nom de calendrier déjà pris"), "{error}");
    click_label(&mut harness, &fr(TextId::WtNewCancel));
    // Le calendrier neuf ne sert à rien : supprimé.
    click_label(&mut harness, &fr(TextId::WtDelete));
    let app = harness.state();
    let state = wt(app).unwrap();
    assert!(state.error.is_none(), "{:?}", state.error);
    assert!(!session(app).project().calendars.contains(created));
    assert_eq!(state.selected, session(app).project().calendar);
    assert_eq!(history_entries(app), 2);
}

/// « Informations sur le projet » : calendrier et date d'état changés en
/// un seul lot, annulé par un seul « Annuler ».
#[test]
fn project_info_calendar_single_undo() {
    let mut harness = harness();
    open_demo(&mut harness);
    agent(
        &mut harness,
        &serde_json::json!([{ "op": "calendar.add", "name": "Chantier", "copy_of": 1 }]),
    );
    let before = session(harness.state()).project().clone();
    open(&mut harness, DialogKind::ProjectInformation);
    let state = pi(harness.state()).unwrap();
    assert!(!state.from_finish);
    assert_eq!(state.date, "02/11/2026 08:00");
    assert_eq!(state.status, "15/01/2027 17:00");
    choose(&mut harness, &fr(TextId::PiCalendar), "Chantier");
    retype(&mut harness, &fr(TextId::PiStatus), "22/01/2027 17:00");
    click_label(&mut harness, &fr(TextId::TaskInfoOk));
    let app = harness.state();
    assert!(pi(app).is_none(), "{:?}", pi(app).map(|s| &s.errors));
    assert_eq!(history_entries(app), 2, "un seul lot de plus");
    let p = session(app).project();
    let chantier = p.calendars.iter().find(|c| c.name == "Chantier").unwrap().uid();
    assert_eq!(p.calendar, chantier);
    assert_eq!(p.status_date, Some("2027-01-22T17:00".parse().unwrap()));
    harness.state_mut().dispatch(Action::Undo);
    harness.run();
    assert_eq!(session(harness.state()).project(), &before);
    // Rien de changé : aucun lot ; date illisible : refus sous le champ.
    open(&mut harness, DialogKind::ProjectInformation);
    click_label(&mut harness, &fr(TextId::TaskInfoOk));
    assert!(pi(harness.state()).is_none());
    assert_eq!(history_entries(harness.state()), 1);
    open(&mut harness, DialogKind::ProjectInformation);
    retype(&mut harness, &fr(TextId::PiStatus), "demain");
    click_label(&mut harness, &fr(TextId::TaskInfoOk));
    let state = pi(harness.state()).expect("fenêtre gardée");
    assert_eq!(state.errors.len(), 1);
    assert_eq!(state.errors[0].0, "status");
    // Vide : NA.
    retype(&mut harness, &fr(TextId::PiStatus), "");
    click_label(&mut harness, &fr(TextId::TaskInfoOk));
    assert_eq!(session(harness.state()).project().status_date, None);
    // Sens à rebours : la date proposée est la fin calculée par le moteur.
    let s = session(harness.state());
    let mut state = ProjectInfoState::open(s, (1, s.generation()), TextLanguage::Fr);
    state.set_from_finish(true);
    let ctx = TextContext::for_project(s.project(), TextLanguage::Fr);
    assert_eq!(state.date, format_date(s.schedule().project_finish, &ctx));
}

/// « Définir la ligne de base » sur une ligne déjà enregistrée : le
/// registre refuse, la fenêtre demande confirmation, rien n'est appliqué ;
/// « Remplacer » applique en un seul lot.
#[test]
fn baseline_set_overwrite_confirm() {
    let mut harness = harness();
    open_demo(&mut harness);
    let n0 = BaselineNumber::new(0).unwrap();
    assert!(
        session(harness.state())
            .project()
            .tasks
            .iter()
            .all(|t| t.baselines.is_empty())
    );
    open(&mut harness, DialogKind::Baseline);
    click_label(&mut harness, &fr(TextId::TaskInfoOk));
    let app = harness.state();
    assert!(bl(app).is_none());
    assert_eq!(history_entries(app), 1);
    assert!(
        session(app)
            .project()
            .tasks
            .iter()
            .any(|t| t.baselines.contains_key(&n0))
    );
    open(&mut harness, DialogKind::Baseline);
    click_label(&mut harness, &fr(TextId::TaskInfoOk));
    let app = harness.state();
    let state = bl(app).expect("fenêtre gardée");
    assert!(state.confirm.is_some(), "confirmation demandée");
    assert!(state.error.is_none());
    assert_eq!(history_entries(app), 1, "rien appliqué");
    assert!(app.messages().is_empty(), "{:?}", app.messages());
    click_label(&mut harness, &fr(TextId::BlReplace));
    let app = harness.state();
    assert!(bl(app).is_none(), "{:?}", bl(app));
    assert_eq!(history_entries(app), 2);
    // Ligne 3 : neuve, aucune confirmation.
    open(&mut harness, DialogKind::Baseline);
    choose(
        &mut harness,
        &fr(TextId::BlNumber),
        &Texts::new(Language::Fr).get_args(TextId::BlBaselineN, &[("count", Arg::Count(3))]),
    );
    click_label(&mut harness, &fr(TextId::TaskInfoOk));
    let app = harness.state();
    assert!(bl(app).is_none());
    let n3 = BaselineNumber::new(3).unwrap();
    assert!(
        session(app)
            .project()
            .tasks
            .iter()
            .any(|t| t.baselines.contains_key(&n3))
    );
}

/// « Effacer la ligne de base » sur la tâche sélectionnée : seule sa ligne
/// est effacée, en un seul lot.
#[test]
fn baseline_clear_selection() {
    let mut harness = harness();
    open_demo(&mut harness);
    agent(&mut harness, &serde_json::json!([{ "op": "baseline.set" }]));
    let n0 = BaselineNumber::new(0).unwrap();
    harness.state_mut().dispatch(Action::Select(Some(uid(3))));
    harness.run();
    open(&mut harness, DialogKind::Baseline);
    assert_eq!(bl(harness.state()).unwrap().selected, [uid(3)]);
    click_label(&mut harness, &fr(TextId::BlClear));
    let selection = Texts::new(Language::Fr).get_args(TextId::ScopeSelection, &[("count", Arg::Count(1))]);
    click_label(&mut harness, &selection);
    click_label(&mut harness, &fr(TextId::TaskInfoOk));
    let app = harness.state();
    assert!(bl(app).is_none(), "{:?}", bl(app));
    assert_eq!(history_entries(app), 2);
    let p = session(app).project();
    assert!(!p.tasks.get(uid(3)).unwrap().baselines.contains_key(&n0));
    assert!(p.tasks.get(uid(4)).unwrap().baselines.contains_key(&n0));
    // Lot pur : effacer sur le projet entier ne cite aucune tâche.
    let mut state = BaselineState::open((1, 0), vec![uid(3)]);
    state.action = plannercraft_ui_egui::dialog::baseline::BaselineAction::Clear;
    let batch = state.to_batch(TextLanguage::Fr);
    let Command::BaselineClear(op) = &batch.ops[0] else {
        panic!("baseline.clear attendu");
    };
    assert_eq!(op.tasks, None);
    assert_eq!(op.number, Some(0));
}

/// « Replanifier le travail restant » : date proposée = date d'état
/// effective écrite par le moteur ; laissée telle quelle, le lot ne la
/// donne pas (défaut du registre) et le résultat est celui du registre.
#[test]
fn reschedule_default_status_date() {
    let mut harness = harness();
    open_demo(&mut harness);
    let s = session(harness.state());
    let ctx = TextContext::for_project(s.project(), TextLanguage::Fr);
    let expected = format_date(s.project().effective_status_date(), &ctx);
    let mut reference = s.clone();
    open(&mut harness, DialogKind::Reschedule);
    let state = rs(harness.state()).unwrap();
    assert_eq!(state.after, expected);
    let texts = Texts::new(Language::Fr);
    let batch = state
        .to_batch(session(harness.state()), &texts, TextLanguage::Fr)
        .unwrap();
    let Command::ProgressReschedule(op) = &batch.ops[0] else {
        panic!("progress.reschedule attendu");
    };
    assert_eq!(op.after, None);
    assert_eq!(op.tasks, None);
    click_label(&mut harness, &fr(TextId::TaskInfoOk));
    let app = harness.state();
    assert!(rs(app).is_none(), "{:?}", rs(app));
    assert!(app.messages().is_empty(), "{:?}", app.messages());
    let explicit: Batch = serde_json::from_value(
        serde_json::json!({ "format_version": 1, "ops": [{ "op": "progress.reschedule" }] }),
    )
    .unwrap();
    reference.apply(&explicit, ApplyMode::Commit).unwrap();
    assert_eq!(session(app).project(), reference.project());
    // Date illisible : refus dans la fenêtre.
    open(&mut harness, DialogKind::Reschedule);
    retype(&mut harness, &fr(TextId::RsAfter), "bientôt");
    click_label(&mut harness, &fr(TextId::TaskInfoOk));
    assert!(rs(harness.state()).unwrap().error.is_some());
}

/// Chaque fenêtre ouverte : ni la navigation, ni les commandes du
/// document, ni la frappe directe n'atteignent la table ; Échap ferme
/// (Entrée valide la fenêtre : `dialogs_enter_validates`).
#[test]
fn dialogs_keys_do_not_leak() {
    for kind in [
        DialogKind::WorkingTime,
        DialogKind::ProjectInformation,
        DialogKind::Baseline,
        DialogKind::Reschedule,
    ] {
        let mut harness = harness();
        open_demo(&mut harness);
        harness.state_mut().dispatch(Action::Select(Some(uid(3))));
        harness.run();
        open(&mut harness, kind);
        assert_eq!(harness.state().view().dialog.as_ref().map(|d| d.kind), Some(kind));
        for key in [Key::ArrowDown, Key::Delete, Key::F2, Key::Insert] {
            harness.key_press(key);
            harness.run();
        }
        harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        harness.event(Event::Text("x".to_owned()));
        harness.run();
        let app = harness.state();
        assert_eq!(
            app.view().dialog.as_ref().map(|d| d.kind),
            Some(kind),
            "{kind:?} fermée"
        );
        assert_eq!(app.selection, Some(uid(3)), "{kind:?} : le curseur a bougé");
        assert!(app.view().edit.editing.is_none(), "{kind:?} : saisie ouverte");
        assert_eq!(generation(app), 0, "{kind:?} : commande exécutée");
        assert_eq!(session(app).project().tasks.len(), 49);
        for _ in 0..2 {
            harness.key_press(Key::Escape);
            harness.run();
            if harness.state().view().dialog.is_none() {
                break;
            }
        }
        let app = harness.state();
        assert!(app.view().dialog.is_none(), "{kind:?} : Échap ne ferme pas");
        assert_eq!(generation(app), 0);
    }
}

/// Relecture finale J7, F8 : Entrée (sans liste ouverte ni widget autre
/// qu'un champ d'une ligne focalisé) valide la fenêtre comme « OK » :
/// informations du projet et temps de travail sans changement ⇒ fermées
/// sans lot ; ligne de base ⇒ posée (un lot), fenêtre fermée. Rien n'atteint
/// la table.
#[test]
fn dialogs_enter_validates() {
    for (kind, lots) in [
        (DialogKind::ProjectInformation, 0),
        (DialogKind::WorkingTime, 0),
        (DialogKind::Baseline, 1),
    ] {
        let mut harness = harness();
        open_demo(&mut harness);
        harness.state_mut().dispatch(Action::Select(Some(uid(3))));
        harness.run();
        open(&mut harness, kind);
        harness.key_press(Key::Enter);
        harness.run();
        let app = harness.state();
        assert!(app.view().dialog.is_none(), "{kind:?} : Entrée ne valide pas");
        assert_eq!(generation(app), lots, "{kind:?}");
        assert_eq!(
            app.selection,
            Some(uid(3)),
            "{kind:?} : Entrée a atteint la table"
        );
    }
}

/// Menu Projet de la barre (J8 lot L15 ; sous-menu Édition → Projet
/// jusque-là) : chaque entrée ouvre sa fenêtre ; sans document, rien ne
/// s'ouvre ; le menu Édition ne le porte plus.
#[test]
fn menu_project_in_bar() {
    let mut harness = harness();
    open_demo(&mut harness);
    click_label(&mut harness, &fr(TextId::MenuEdit));
    assert!(harness.query_by_label(&fr(TextId::MenuProjectInfo)).is_none());
    harness.key_press(Key::Escape);
    harness.run();
    for (label, kind) in [
        (TextId::MenuProjectInfo, DialogKind::ProjectInformation),
        (TextId::MenuWorkingTime, DialogKind::WorkingTime),
        (TextId::MenuBaseline, DialogKind::Baseline),
        (TextId::MenuReschedule, DialogKind::Reschedule),
    ] {
        click_label(&mut harness, &fr(TextId::MenuProject));
        click_label(&mut harness, &fr(label));
        assert_eq!(harness.state().view().dialog.as_ref().map(|d| d.kind), Some(kind));
        harness.state_mut().dispatch(Action::CloseDialog);
        harness.run();
    }
    // Sans document : rien ne s'ouvre.
    harness.state_mut().dispatch(Action::CloseDocument);
    harness.run();
    harness
        .state_mut()
        .dispatch(Action::ShowDialog(DialogKind::WorkingTime));
    harness.run();
    assert!(harness.state().view().dialog.is_none());
}

/// Document changé entre l'ouverture et « OK » : rien appliqué, brouillon
/// relu, avertissement ; le second « OK » applique.
#[test]
fn dialogs_stale_key_refused() {
    let mut harness = harness();
    open_demo(&mut harness);
    open(&mut harness, DialogKind::ProjectInformation);
    retype(&mut harness, &fr(TextId::PiStatus), "22/01/2027 17:00");
    agent(
        &mut harness,
        &serde_json::json!([{ "op": "task.update", "uid": 4, "name": "Dépôt" }]),
    );
    click_label(&mut harness, &fr(TextId::TaskInfoOk));
    let app = harness.state();
    let state = pi(app).expect("fenêtre gardée");
    assert!(state.stale);
    assert_eq!(state.status, "15/01/2027 17:00", "brouillon relu");
    assert_eq!(history_entries(app), 1);
    assert!(harness.query_by_label(&fr(TextId::TaskInfoStale)).is_some());
    retype(&mut harness, &fr(TextId::PiStatus), "22/01/2027 17:00");
    click_label(&mut harness, &fr(TextId::TaskInfoOk));
    assert!(pi(harness.state()).is_none());
    assert_eq!(history_entries(harness.state()), 2);
}

/// Capture de « Modifier le temps de travail » sur l'exemple (janvier
/// 2027, calendrier du projet).
#[test]
fn snapshot_working_time_fr_light() {
    let mut harness = common::harness(LanguagePref::Fr, ThemePref::Light);
    harness.run();
    crate::click(&mut harness, &fr(TextId::WelcomeOpenDemo));
    harness
        .state_mut()
        .dispatch(Action::ShowDialog(DialogKind::WorkingTime));
    harness.run();
    assert!(matches!(
        harness.state().view().dialog,
        Some(DialogState {
            body: DialogBody::WorkingTime(_),
            ..
        })
    ));
    common::snapshot(&mut harness, "working_time_fr_light");
}
