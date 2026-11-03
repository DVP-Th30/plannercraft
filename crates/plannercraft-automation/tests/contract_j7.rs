//! Contrat J7 (lot L0, ADR-0015) : types, squelettes et registre. Les
//! commandes nouvelles (`calendar.add`, `calendar.update`,
//! `calendar.set_week`, `calendar.delete`, `baseline.clear`) se lisent et
//! se relisent ; servies depuis le lot L5 (règles R-247 à R-250 dans
//! `tests/rules`), comme les champs ajoutés aux commandes existantes
//! (calendrier du projet, taux, nom d'exception) ; les
//! entrées `app.*` sont refusées `control.not_attached` sans application ;
//! les squelettes (gestes, utilisation par affectation, trimestre,
//! `call_with`) répondent sans paniquer ni rien changer.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

use plannercraft_automation::command::params::{ScreenshotParams, UiActionParams, UsageParams};
use plannercraft_automation::command::{
    BaselineClear, CalendarAdd, CalendarRef, CalendarSetWeek, Command, DaySettingJson, GranularityJson,
    Patch, RangeJson, WeekSettingJson, WeekdayJson,
};
use plannercraft_automation::error::codes;
use plannercraft_automation::gesture::{self, GestureError, GestureKind};
use plannercraft_automation::registry::{Availability, Kind, lookup};
use plannercraft_automation::text::Language;
use plannercraft_automation::{
    AnchorSpec, ApplyMode, ApplyOptions, Batch, CallEnv, EditSource, NewProject, Session, Workspace,
    call_with,
};
use plannercraft_core::calendar::CalendarSet;
use plannercraft_core::resource::usage::MAX_RESOURCES;
use plannercraft_core::resource::{
    Granularity, MAX_ASSIGNMENT_ROWS, UsageError, usage, usage_by_assignment, usage_with,
};
use plannercraft_core::{Diagnostic, ResourceUid, Severity, TaskUid};
use serde_json::json;

fn now() -> plannercraft_core::Moment {
    "2026-10-19T08:00".parse().unwrap()
}

/// Projet : tâche 1 (2 j) affectée à « Maçon », tâche 2 (1 j).
fn session() -> Session {
    let init = NewProject {
        name: "Projet1".to_owned(),
        anchor: AnchorSpec::Start(now()),
        lang: None,
    };
    let mut s = Session::new_project(init, now()).unwrap();
    let batch = Batch::from_json(json!({
        "format_version": 1,
        "ops": [
            { "op": "task.add", "name": "T1", "duration": { "text": "2j" } },
            { "op": "task.add", "name": "T2", "duration": { "text": "1j" } },
            { "op": "resource.add", "name": "Maçon", "as": "$m" },
            { "op": "assignment.add", "task": 1, "resource": "$m" }
        ]
    }))
    .unwrap();
    s.apply(&batch, ApplyMode::Commit).unwrap();
    s
}

fn one(op: &serde_json::Value) -> Batch {
    Batch::from_json(json!({ "format_version": 1, "ops": [op] })).unwrap()
}

#[test]
fn contract_compiles() {
    let mut s = session();
    let before = s.project().clone();
    let generation = s.generation();

    // Commandes nouvelles et champs nouveaux : lus, relus à l'identique,
    // servis depuis le lot L5 (simulés ici, jamais `not_yet_available`).
    let ops = [
        json!({"op": "calendar.add", "name": "Équipe B", "copy_of": 1}),
        json!({"op": "calendar.add", "name": "Dérivé", "base": 1}),
        json!({"op": "calendar.update", "uid": 2, "name": "Nuit", "base": null}),
        json!({"op": "calendar.set_week", "calendar": "project", "days": [
            {"day": "saturday", "setting": {"ranges": [{"from": "08:00", "to": "12:00"}]}},
            {"day": "sunday", "setting": "non_working"},
            {"day": "monday", "setting": "inherit"}
        ]}),
        json!({"op": "calendar.delete", "uid": 1}),
        json!({"op": "baseline.clear"}),
        json!({"op": "baseline.clear", "number": 1, "tasks": [1]}),
        json!({"op": "project.set_info", "calendar": 1}),
        json!({"op": "resource.update", "uid": 1, "standard_rate": {"amount": 12_000, "per": "day"}}),
        json!({"op": "resource.update", "uid": 1, "cost_per_use": {"amount": 500}}),
        json!({"op": "calendar.set_exception", "cal": "project", "from": "2026-11-11",
               "to": "2026-11-11", "name": "Armistice"}),
    ];
    for (i, op) in ops.iter().enumerate() {
        let batch = one(op);
        let again: Batch = serde_json::from_value(serde_json::to_value(&batch).unwrap()).unwrap();
        assert_eq!(again, batch, "aller-retour {i}");
        assert_eq!(batch.ops[0].id(), op["op"].as_str().unwrap());
        if let Err(err) = s.apply(&batch, ApplyMode::DryRun) {
            assert_ne!(err.code, codes::COMMAND_NOT_YET_AVAILABLE, "{i}");
            assert_eq!(err.op_index, Some(0), "{i}");
        }
    }
    let Command::CalendarSetWeek(week) = &one(&ops[3]).ops[0] else {
        panic!("calendar.set_week attendu");
    };
    assert_eq!(
        week,
        &CalendarSetWeek {
            calendar: CalendarRef::Project,
            days: vec![
                DaySettingJson {
                    day: WeekdayJson::Saturday,
                    setting: WeekSettingJson::Ranges(vec![RangeJson {
                        from: "08:00".parse().unwrap(),
                        to: "12:00".parse().unwrap(),
                    }]),
                },
                DaySettingJson {
                    day: WeekdayJson::Sunday,
                    setting: WeekSettingJson::NonWorking,
                },
                DaySettingJson {
                    day: WeekdayJson::Monday,
                    setting: WeekSettingJson::Inherit,
                },
            ],
        }
    );
    let Command::CalendarAdd(add) = &one(&ops[0]).ops[0] else {
        panic!("calendar.add attendu");
    };
    assert_eq!(
        add,
        &CalendarAdd {
            name: "Équipe B".to_owned(),
            base: None,
            copy_of: Some(1),
        }
    );
    // Champ inconnu refusé ; valeurs par défaut absentes du JSON.
    assert!(
        Batch::from_json(json!({"format_version": 1, "ops": [{"op": "baseline.clear", "x": 1}]})).is_err()
    );
    assert_eq!(serde_json::to_string(&BaselineClear::default()).unwrap(), "{}");
    let Command::ProjectSetInfo(info) = &one(&ops[7]).ops[0] else {
        panic!("project.set_info attendu");
    };
    assert_eq!(info.calendar, Patch::Set(1));
    assert_eq!(s.project(), &before);
    assert_eq!(s.generation(), generation);

    // Les commandes J4 à J6 sans champ J7 restent servies.
    let rename = one(&json!({"op": "resource.update", "uid": 1, "name": "Maçon B"}));
    s.apply(&rename, ApplyMode::DryRun).unwrap();
}

/// Lot L5 : les champs J7 des commandes existantes entrent dans leur
/// schéma ; les nouvelles commandes entrent dans le schéma des lots et
/// sont listées.
#[test]
fn contract_schemas_after_l5() {
    let props =
        |id: &str| serde_json::to_value((lookup(id).unwrap().schema)()).unwrap()["properties"].clone();
    assert!(props("project.set_info").get("calendar").is_some());
    for field in ["standard_rate", "overtime_rate", "cost_per_use"] {
        assert!(props("resource.update").get(field).is_some(), "{field}");
    }
    assert!(props("calendar.set_exception").get("name").is_some());
    let batch = serde_json::to_string(&(lookup("batch.apply").unwrap().schema)()).unwrap();
    for id in [
        "calendar.add",
        "calendar.update",
        "calendar.set_week",
        "calendar.delete",
        "baseline.clear",
    ] {
        assert!(
            batch.contains(&format!("\"{id}\"")),
            "{id} absent du schéma des lots"
        );
        let spec = lookup(id).unwrap();
        assert_eq!(spec.available, Availability::Now, "{id}");
        assert_eq!(spec.kind, Kind::Write, "{id}");
        assert!(props(id).is_object(), "{id} : schéma vide");
    }
}

/// `app.*` sans application : `control.not_attached` ; `calendar.add` est
/// servi depuis le lot L5 ; le trimestre de `resource_usage` depuis le lot
/// L4 (R-246, `views_j7.rs`).
#[test]
fn contract_workspace_refusals() {
    let mut w = Workspace::new(now(), Language::Fr);
    w.set_session(session());
    for id in ["app.ui_state", "app.ui", "app.screenshot"] {
        let err = w.call(id, json!({})).unwrap_err();
        assert_eq!(err.code, codes::CONTROL_NOT_ATTACHED, "{id}");
    }
    w.call("calendar.add", json!({"name": "B"})).unwrap();
    w.call("analysis.resource_usage", json!({"by": "quarter"}))
        .unwrap();
    w.call("analysis.resource_usage", json!({"by": "week"})).unwrap();
    // Paramètres des entrées `app.*`.
    let p: UiActionParams =
        serde_json::from_value(json!({"select": null, "view": "resource_sheet"})).unwrap();
    assert_eq!(p.select, Patch::Clear);
    assert!(serde_json::from_value::<UiActionParams>(json!({"x": 1})).is_err());
    let p: ScreenshotParams = serde_json::from_value(json!({"scale_down": 1})).unwrap();
    assert_eq!(p.scale_down, Some(1));
    let p: UsageParams = serde_json::from_value(json!({"by": "quarter"})).unwrap();
    assert_eq!(p.by, GranularityJson::Quarter);
    assert_eq!(Granularity::from(GranularityJson::Quarter), Granularity::Quarter);
    assert_eq!(
        GranularityJson::from(Granularity::Quarter),
        GranularityJson::Quarter
    );

    // `call_with` (corps au lot L3) : lecture servie ; ouvertures et
    // `app.*` refusées, session intacte.
    let mut s = session();
    let before = s.project().clone();
    let env = CallEnv::new(now(), Language::En);
    assert!(call_with(&mut s, &env, "project.summary", json!({})).is_ok());
    let err = call_with(&mut s, &env, "project.open", json!({"path": "x.xml"})).unwrap_err();
    assert_eq!(err.code, codes::CONTROL_NOT_RELAYABLE);
    let err = call_with(&mut s, &env, "app.ui_state", json!({})).unwrap_err();
    assert_eq!(err.code, codes::CONTROL_NOT_ATTACHED);
    assert_eq!(s.project(), &before);
}

/// Gestes (N12, lot L8 : leurs règles R-240 à R-243 sont testées dans
/// `tests/rules`) : écart nul sans lot, tâche inconnue et lien vers soi
/// refusés, aperçu d'un écart nul aux dates courantes.
#[test]
fn contract_gestures() {
    let s = session();
    let (set, _) = CalendarSet::compile(s.project());
    let t1 = TaskUid::new(1);
    let nope = TaskUid::new(99);
    assert_eq!(gesture::bar_move(&s, t1, 0).unwrap(), None);
    assert!(gesture::bar_move(&s, t1, 3).unwrap().is_some());
    assert_eq!(gesture::bar_resize(&s, &set, t1, 0).unwrap(), None);
    assert_eq!(
        gesture::bar_move(&s, nope, 1).unwrap_err(),
        GestureError::UnknownTask(nope)
    );
    assert_eq!(
        gesture::bar_move(&s, nope, 1).unwrap_err().code(),
        codes::TASK_NOT_FOUND
    );
    assert_eq!(
        gesture::link_drag(&s, t1, t1).unwrap_err(),
        GestureError::SameTask
    );
    assert_eq!(gesture::link_drag(&s, t1, TaskUid::new(2)).unwrap().ops.len(), 1);
    let t = s.schedule().scheduled(t1).unwrap();
    let p = gesture::preview(&s, &set, t1, GestureKind::Move, 0).unwrap();
    assert_eq!((p.start, p.finish), (t.start, t.finish));
    assert_eq!(
        GestureError::NotDraggable(t1, gesture::NotDraggable::Started).code(),
        codes::GESTURE_NOT_DRAGGABLE
    );
}

/// Cœur (contrat J7) : `usage_with` = `usage` ; trimestre et utilisation
/// par affectation (lot L4, R-245, R-246 : leurs tests sont dans
/// `tests/rules`) ; bornes ; diagnostic `baseline.nothing_to_clear`.
#[test]
fn contract_core_usage() {
    let s = session();
    let (p, sched) = (s.project(), s.schedule());
    let (set, _) = CalendarSet::compile(p);
    let resources: Vec<ResourceUid> = p.resources.iter().map(plannercraft_core::HasUid::uid).collect();
    let (from, to) = (sched.project_start, sched.project_finish);
    for by in [Granularity::Day, Granularity::Week, Granularity::Month] {
        let a = usage(p, sched, from, to, by, &resources).unwrap();
        let b = usage_with(p, sched, &set, from, to, by, &resources).unwrap();
        assert_eq!(a, b, "{by:?}");
        assert_ne!(a.periods.len(), 0);
    }
    let q = usage_with(p, sched, &set, from, to, Granularity::Quarter, &resources).unwrap();
    assert_eq!(q.periods.len(), 1);
    let r = usage_by_assignment(p, sched, &set, from, to, Granularity::Week, &resources).unwrap();
    assert_eq!((r.rows.len(), r.task_totals.len()), (1, 1));
    assert_ne!(r.periods.len(), 0);
    let many = vec![ResourceUid::new(1); MAX_RESOURCES + 1];
    assert!(matches!(
        usage_by_assignment(p, sched, &set, from, to, Granularity::Week, &many),
        Err(UsageError::Limit { .. })
    ));
    assert_eq!(MAX_ASSIGNMENT_ROWS, 10_000);
    let d = Diagnostic::BaselineNothingToClear {
        number: plannercraft_core::BaselineNumber::new(0).unwrap(),
    };
    assert_eq!(
        (d.code(), d.severity()),
        ("baseline.nothing_to_clear", Severity::Info)
    );
}

/// Historique (J7) : origine `User` par défaut, omise du JSON (vues J6
/// inchangées) ; `Agent` sérialisé.
#[test]
fn contract_edit_source() {
    let s = session();
    assert_eq!(ApplyOptions::default().source, EditSource::User);
    let view = serde_json::to_value(s.history()).unwrap();
    assert!(view["entries"][0].get("source").is_none(), "{view}");
    assert_eq!(serde_json::to_value(EditSource::Agent).unwrap(), json!("agent"));
    assert!(EditSource::default().is_user());
}
