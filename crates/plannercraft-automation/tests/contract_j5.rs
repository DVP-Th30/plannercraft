//! Contrat J5 (lot L0, ADR-0013) : types, squelettes et registre. Les
//! commandes nouvelles se lisent et se relisent ; exécutées dans un lot
//! depuis le lot L7 (R-195 à R-197), elles sont listées et appelables par
//! l'espace de travail depuis le lot L8 (huit entrées J5 du registre) ;
//! les types du cœur se construisent et répondent sans paniquer.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

use std::collections::BTreeSet;

use plannercraft_automation::command::{
    AnchorSpec, Command, Patch, ResourceCalendarJson, ResourceKindJson, ResourceRef, TaskRef, TaskTypeJson,
};
use plannercraft_automation::error::codes;
use plannercraft_automation::registry::{Availability, Kind, by_mcp_name, listed, lookup};
use plannercraft_automation::text::Language;
use plannercraft_automation::{ApplyMode, Batch, NewProject, REGISTRY, Session, Workspace};
use plannercraft_core::calendar::CompiledCalendar;
use plannercraft_core::resource::{Granularity, usage};
use plannercraft_core::{CalendarUid, Moment, ResourceUid, TaskOutcome, TaskUid};
use serde_json::json;

fn now() -> Moment {
    "2026-10-19T08:00".parse().unwrap()
}

fn session_with_task() -> Session {
    let init = NewProject {
        name: "Projet1".to_owned(),
        anchor: AnchorSpec::Start("2026-10-19T08:00".parse().unwrap()),
        lang: None,
    };
    let mut s = Session::new_project(init, now()).unwrap();
    let batch = Batch::from_json(json!({
        "format_version": 1,
        "ops": [{ "op": "task.add", "name": "A", "duration": { "text": "2j" } }]
    }))
    .unwrap();
    s.apply(&batch, ApplyMode::Commit).unwrap();
    s
}

/// Les huit entrées J5 (ADR-0013 D9) : quatre réservées en J4, quatre
/// créées en L0.
const J5_ENTRIES: [(&str, &str); 8] = [
    ("analysis.resource_usage", "resource_usage"),
    ("assignment.add", "assign_resource"),
    ("assignment.remove", "unassign_resource"),
    ("assignment.update", "update_assignment"),
    ("resource.add", "add_resource"),
    ("resource.delete", "delete_resource"),
    ("resource.list", "list_resources"),
    ("resource.update", "update_resource"),
];

#[test]
fn contract_compiles() {
    let mut s = session_with_task();
    let before = s.project().clone();
    let generation = s.generation();

    // Commandes nouvelles : lues, relues à l'identique, refusées sans effet.
    let ops = [
        json!({"op": "resource.add", "name": "Dev", "as": "$dev", "kind": "work",
               "max_units": {"value": 10_000}, "calendar": "own", "initials": "D", "group": "G"}),
        json!({"op": "resource.update", "uid": "$dev", "name": "Dev 2", "max_units": {"text": "50%"},
               "calendar": 3, "notes": null, "inactive": true}),
        json!({"op": "resource.delete", "uid": 4, "cascade": true}),
        json!({"op": "assignment.add", "task": 1, "resource": "$dev", "units": {"text": "50%"},
               "work": {"text": "16h"}, "delay": {"text": "1j"}}),
        json!({"op": "assignment.update", "task": 1, "resource": 2, "quantity": {"value": 30_000},
               "delay": null}),
        json!({"op": "assignment.remove", "task": 1, "resource": 2}),
    ];
    for (i, op) in ops.iter().enumerate() {
        let batch = Batch::from_json(json!({ "format_version": 1, "ops": [op] })).unwrap();
        let again: Batch = serde_json::from_value(serde_json::to_value(&batch).unwrap()).unwrap();
        assert_eq!(again, batch, "aller-retour {i}");
        assert_eq!(batch.ops[0].id(), op["op"].as_str().unwrap());
    }
    // Lot L7 : exécutées dans un lot (simulé ici, la session reste
    // intacte) ; la ressource 4 n'existe pas.
    let batch = Batch::from_json(json!({ "format_version": 1, "ops": [ops[2]] })).unwrap();
    assert_eq!(
        s.apply(&batch, ApplyMode::DryRun).unwrap_err().code,
        codes::RESOURCE_NOT_FOUND
    );
    let batch = Batch::from_json(json!({ "format_version": 1, "ops": [ops[0], ops[1], ops[3]] })).unwrap();
    let o = s.apply(&batch, ApplyMode::DryRun).unwrap();
    assert!(!o.committed);
    assert_eq!(o.created_resources, [ResourceUid::new(1)]);
    let batch = Batch::from_json(json!({ "format_version": 1, "ops": [ops[0]] })).unwrap();
    let Command::ResourceAdd(add) = &batch.ops[0] else {
        panic!("resource.add attendu");
    };
    assert_eq!(add.label.as_deref(), Some("$dev"));
    assert_eq!(add.kind, Some(ResourceKindJson::Work));
    assert_eq!(add.calendar, Some(ResourceCalendarJson::Own));

    // Champs étendus (lot L7) : exécutés, simulés ici ; le travail saisi
    // sur une tâche sans affectation de travail est refusé sur son champ.
    for op in [
        json!({"op": "task.update", "uid": 1, "type": "fixed_work"}),
        json!({"op": "task.update", "uid": 1, "effort_driven": true}),
        json!({"op": "project.set_info", "default_task_type": "fixed_duration"}),
        json!({"op": "project.set_info", "new_tasks_effort_driven": false}),
    ] {
        let batch = Batch::from_json(json!({ "format_version": 1, "ops": [op] })).unwrap();
        assert!(!s.apply(&batch, ApplyMode::DryRun).unwrap().committed, "{op}");
    }
    let batch = Batch::from_json(json!({
        "format_version": 1,
        "ops": [{"op": "task.update", "uid": 1, "work": {"text": "16h"}}]
    }))
    .unwrap();
    let err = s.apply(&batch, ApplyMode::Commit).unwrap_err();
    assert_eq!(err.code, codes::PARAM_INVALID);
    assert_eq!(err.field.as_deref(), Some("work"));
    assert_ne!(err.message(Language::Fr), err.message(Language::En));
    let batch = Batch::from_json(json!({
        "format_version": 1,
        "ops": [{"op": "task.update", "uid": 1, "type": "fixed_units"}]
    }))
    .unwrap();
    let Command::TaskUpdate(update) = &batch.ops[0] else {
        panic!("task.update attendu");
    };
    assert_eq!(update.task_type, Patch::Set(TaskTypeJson::FixedUnits));
    assert_eq!(update.uid, TaskRef::Uid(TaskUid::new(1)));
    assert_eq!(s.project(), &before);
    assert_eq!(s.generation(), generation);

    // Désignations : étiquette mal formée, calendrier inconnu refusés.
    assert_eq!(
        serde_json::from_value::<ResourceRef>(json!(0)).unwrap(),
        ResourceRef::Uid(ResourceUid::new(0))
    );
    assert!(serde_json::from_value::<ResourceRef>(json!("dev")).is_err());
    assert_eq!(
        serde_json::from_value::<ResourceCalendarJson>(json!("project")).unwrap(),
        ResourceCalendarJson::Project
    );
    assert_eq!(
        serde_json::from_value::<ResourceCalendarJson>(json!(7)).unwrap(),
        ResourceCalendarJson::Uid(CalendarUid::new(7))
    );
    assert!(serde_json::from_value::<ResourceCalendarJson>(json!("mine")).is_err());
    assert!(
        Batch::from_json(json!({"format_version": 1, "ops": [{"op": "resource.add", "name": "R", "x": 1}]}))
            .is_err()
    );

    core_contract(&s);

    // Surfaces (lot L8) : entrées activées, refusées sans projet ouvert
    // comme toute commande sur la session.
    let mut ws = Workspace::new(now(), Language::Fr);
    for (id, _) in J5_ENTRIES {
        let err = ws.call(id, json!({})).unwrap_err();
        assert_ne!(err.code, codes::COMMAND_NOT_YET_AVAILABLE, "{id}");
    }
}

/// Types du cœur (contrat J5) : planning sans affectation ni charge, JSON
/// inchangé, squelettes qui répondent sans paniquer, combinaisons de
/// calendriers implémentées (L1).
fn core_contract(s: &Session) {
    let schedule = s.schedule().clone();
    assert!(schedule.assignments.is_empty() && schedule.resources.is_empty());
    let Some(TaskOutcome::Scheduled(t)) = schedule.get(TaskUid::new(1)) else {
        panic!("tâche 1 planifiée attendue");
    };
    assert_eq!(t.work, plannercraft_core::Work::ZERO);
    let text = serde_json::to_string(&schedule).unwrap();
    assert!(!text.contains("assignments") && !text.contains("\"work\""));
    assert_eq!(
        serde_json::from_str::<plannercraft_core::Schedule>(&text).unwrap(),
        schedule
    );
    let report = usage(
        s.project(),
        &schedule,
        schedule.project_start,
        schedule.project_finish,
        Granularity::Week,
        &[],
    )
    .unwrap();
    assert!(report.rows.is_empty() && !report.truncated);
    let cal = CompiledCalendar::compile(&s.project().calendars, s.project().calendar).unwrap();
    // Implémentées par le lot L1 (R-172) : a ∩ a = a et a ∪ a = a.
    assert_eq!(cal.intersect(&cal).unwrap(), cal);
    assert_eq!(cal.union(&cal).unwrap(), cal);
}

#[test]
fn registry_sorted_and_unique() {
    let ids: Vec<&str> = REGISTRY.iter().map(|s| s.id).collect();
    for pair in ids.windows(2) {
        assert!(pair[0] < pair[1], "REGISTRY non trié ou en double : {pair:?}");
    }
    let names: BTreeSet<&str> = REGISTRY.iter().map(|s| s.mcp_name).collect();
    assert_eq!(names.len(), REGISTRY.len(), "noms MCP en double");
    for (id, mcp) in J5_ENTRIES {
        let spec = lookup(id).unwrap();
        assert_eq!(spec.mcp_name, mcp);
        assert_eq!(by_mcp_name(mcp).unwrap().id, id);
        // Schéma du contrat déjà fixé : un objet, généré sans panique.
        let schema = serde_json::to_value((spec.schema)()).unwrap();
        assert_eq!(schema["type"], "object", "{id}");
        assert!(schema.get("properties").is_some(), "{id}");
        let expected = match id {
            "analysis.resource_usage" => Kind::Analysis,
            "resource.list" => Kind::Read,
            _ => Kind::Write,
        };
        assert_eq!(spec.kind, expected, "{id}");
        assert_eq!(spec.undoable, expected == Kind::Write, "{id}");
    }
}

/// Lot L8 : les huit entrées J5 sont disponibles et listées ; les six
/// commandes d'écriture entrent dans le schéma des lots.
#[test]
fn registry_j5_entries_listed() {
    for (id, _) in J5_ENTRIES {
        let spec = lookup(id).unwrap();
        assert_eq!(spec.available, Availability::Now, "{id}");
        assert!(spec.is_listed(), "{id}");
        assert!(listed().any(|s| s.id == id), "{id} non listé");
    }
    let batch = serde_json::to_string(&(lookup("batch.apply").unwrap().schema)()).unwrap();
    for (id, _) in J5_ENTRIES {
        let write = lookup(id).unwrap().kind == Kind::Write;
        assert_eq!(
            batch.contains(&format!("\"const\":\"{id}\"")),
            write,
            "{id} dans batch.apply"
        );
    }
}
