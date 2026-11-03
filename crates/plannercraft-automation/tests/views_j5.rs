//! Lot L8 J5 (ADR-0013 D9) : vues JSON des ressources et des affectations,
//! filtre `resource` de `task.list`, surutilisations du diff, utilisation
//! par période, au travers du `Workspace` (même routeur que le MCP et la
//! CLI). Références : `tests/golden/views/*.json`, régénérées par
//! `PLANNERCRAFT_BLESS=1`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

use std::path::PathBuf;

use plannercraft_automation::Workspace;
use plannercraft_automation::error::codes;
use plannercraft_automation::text::Language;
use plannercraft_core::Moment;
use serde_json::{Value, json};

fn now() -> Moment {
    "2026-10-19T08:00".parse().unwrap()
}

/// Référence d'une vue : `tests/golden/views/<name>.json`.
fn golden(name: &str, value: &Value) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/views")
        .join(format!("{name}.json"));
    let mut text = serde_json::to_string_pretty(value).unwrap();
    text.push('\n');
    if std::env::var_os("PLANNERCRAFT_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &text).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("référence absente : {} (PLANNERCRAFT_BLESS=1)", path.display()));
    assert_eq!(
        text, expected,
        "vue {name} différente de la référence (PLANNERCRAFT_BLESS=1 pour régénérer)"
    );
}

/// Projet neuf (lun 19/10/2026 08:00, Standard), deux ressources (Maçon
/// 100 %, Électricien), trois tâches de 5 j : A (Maçon), B (Électricien),
/// C (sans ressource). Rend l'espace et le résultat du lot.
fn site() -> (Workspace, Value) {
    let mut ws = Workspace::new(now(), Language::Fr);
    ws.call(
        "project.new",
        json!({ "name": "Chantier", "start": "2026-10-19T08:00" }),
    )
    .unwrap();
    let out = ws
        .call(
            "batch.apply",
            json!({ "ops": [
                { "op": "resource.add", "name": "Maçon", "as": "$m", "initials": "M" },
                { "op": "resource.add", "name": "Électricien", "as": "$e" },
                { "op": "task.add", "name": "A", "as": "$a", "duration": { "text": "5j" } },
                { "op": "task.add", "name": "B", "as": "$b", "duration": { "text": "5j" } },
                { "op": "task.add", "name": "C", "as": "$c", "duration": { "text": "5j" } },
                { "op": "assignment.add", "task": "$a", "resource": "$m" },
                { "op": "assignment.add", "task": "$b", "resource": "$e", "units": { "text": "50%" } }
            ] }),
        )
        .unwrap();
    (ws, out)
}

fn uid(v: &Value, label: &str, key: &str) -> u64 {
    v[key][label].as_u64().unwrap()
}

#[test]
fn views_golden() {
    let (mut ws, out) = site();
    golden("outcome_resources_batch", &out);
    let m = uid(&out, "$m", "resource_labels");
    let a = uid(&out, "$a", "labels");
    // Travail fixe sur A : type et pilotage montrés par `task.get`.
    ws.call("task.update", json!({ "uid": a, "type": "fixed_work" }))
        .unwrap();
    let task = ws.call("task.get", json!({ "uid": a })).unwrap();
    assert_eq!(task["resources"][0]["uid"], json!(m));
    assert_eq!(task["work"]["minutes"], json!(2_400));
    assert_eq!(
        task["scheduling"],
        json!({ "type": "fixed_work", "effort_driven": true })
    );
    golden("task_get_assigned", &task);
    // Tâche sans ressource : ni `resources`, ni `work`, ni `scheduling`.
    let c = uid(&out, "$c", "labels");
    let plain = ws.call("task.get", json!({ "uid": c })).unwrap();
    for key in ["resources", "work", "scheduling"] {
        assert!(plain.get(key).is_none(), "{key}");
    }
    let list = ws.call("resource.list", json!({})).unwrap();
    assert_eq!(list["total"], json!(2));
    golden("resource_list", &list);
    let usage = ws
        .call("analysis.resource_usage", json!({ "by": "day" }))
        .unwrap();
    golden("resource_usage_day", &usage);
}

#[test]
fn list_tasks_resource_filter() {
    let (mut ws, out) = site();
    let m = uid(&out, "$m", "resource_labels");
    let e = uid(&out, "$e", "resource_labels");
    let a = uid(&out, "$a", "labels");
    let b = uid(&out, "$b", "labels");
    let page = ws.call("task.list", json!({ "resource": m })).unwrap();
    let uids: Vec<u64> = page["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["uid"].as_u64().unwrap())
        .collect();
    assert_eq!(uids, vec![a]);
    assert_eq!(page["total"], json!(1));
    let page = ws.call("task.list", json!({ "resource": e })).unwrap();
    assert_eq!(page["tasks"][0]["uid"], json!(b));
    assert_eq!(page["tasks"][0]["resources"][0]["units"]["text"], json!("50%"));
    let err = ws.call("task.list", json!({ "resource": 77 })).unwrap_err();
    assert_eq!(err.code, codes::RESOURCE_NOT_FOUND);
    assert_eq!(err.field.as_deref(), Some("resource"));
}

#[test]
fn diff_overallocated_new_and_resolved() {
    let (mut ws, out) = site();
    let m = uid(&out, "$m", "resource_labels");
    let b = uid(&out, "$b", "labels");
    let c = uid(&out, "$c", "labels");
    // Maçon aussi sur C, en parallèle de A : surutilisé dès le `dry_run`.
    let add = json!({ "task": c, "resource": m, "units": { "text": "150%" } });
    let mut dry = add.clone();
    dry["dry_run"] = json!(true);
    let preview = ws.call("assignment.add", dry).unwrap();
    assert_eq!(preview["committed"], json!(false));
    assert_eq!(
        preview["diff"]["overallocated"],
        json!({ "new": [m], "resolved": [] })
    );
    let applied = ws.call("assignment.add", add).unwrap();
    assert_eq!(applied["diff"]["overallocated"]["new"], json!([m]));
    let list = ws
        .call("resource.list", json!({ "overallocated": true }))
        .unwrap();
    assert_eq!(list["total"], json!(1));
    assert_eq!(list["resources"][0]["uid"], json!(m));
    assert_eq!(list["resources"][0]["peak"]["text"], json!("250%"));
    // Utilisation par semaine : la première semaine est surutilisée.
    let usage = ws
        .call("analysis.resource_usage", json!({ "resources": [m] }))
        .unwrap();
    assert_eq!(usage["by"], json!("week"));
    assert_eq!(usage["rows"][0]["overallocated"][0], json!(true));
    // Une écriture sans effet sur la charge : rien de signalé.
    let quiet = ws.call("task.update", json!({ "uid": b, "name": "B2" })).unwrap();
    assert!(quiet["diff"].get("overallocated").is_none());
    // Retrait : résolu.
    let removed = ws
        .call("assignment.remove", json!({ "task": c, "resource": m }))
        .unwrap();
    assert_eq!(
        removed["diff"]["overallocated"],
        json!({ "new": [], "resolved": [m] })
    );
    // Annulation : de nouveau surutilisé.
    let undone = ws.call("history.undo", json!({})).unwrap();
    assert_eq!(undone["diff"]["overallocated"]["new"], json!([m]));
}

#[test]
fn resource_list_pagination_and_limits() {
    let (mut ws, out) = site();
    let m = uid(&out, "$m", "resource_labels");
    let e = uid(&out, "$e", "resource_labels");
    let first = ws.call("resource.list", json!({ "limit": 1 })).unwrap();
    assert_eq!(first["resources"][0]["uid"], json!(m.min(e)));
    assert_eq!(first["next_cursor"], json!(m.min(e)));
    let second = ws
        .call("resource.list", json!({ "limit": 1, "cursor": m.min(e) }))
        .unwrap();
    assert_eq!(second["resources"][0]["uid"], json!(m.max(e)));
    assert!(second["next_cursor"].is_null());
    let err = ws.call("resource.list", json!({ "limit": 501 })).unwrap_err();
    assert_eq!(err.code, codes::LIMIT_EXCEEDED);
    let err = ws.call("resource.list", json!({ "cursor": 77 })).unwrap_err();
    assert_eq!(err.code, codes::RESOURCE_NOT_FOUND);
    // Utilisation : bornes et ressources contrôlées.
    let err = ws
        .call(
            "analysis.resource_usage",
            json!({ "from": "2026-11-01T08:00", "to": "2026-10-01T08:00" }),
        )
        .unwrap_err();
    assert_eq!(err.code, codes::PARAM_INVALID);
    let err = ws
        .call("analysis.resource_usage", json!({ "resources": [77] }))
        .unwrap_err();
    assert_eq!(err.code, codes::RESOURCE_NOT_FOUND);
    let err = ws
        .call(
            "analysis.resource_usage",
            json!({ "from": "2026-01-01T00:00", "to": "2030-01-01T00:00", "by": "day" }),
        )
        .unwrap_err();
    assert_eq!(err.code, codes::LIMIT_EXCEEDED);
}

#[test]
fn resource_ids_follow_ms_project() {
    // Fichier MS Project : ressource UID 0 cachée (ID 0, O34), puis UID 1 et
    // 2 d'ID 1 et 2. Les vues rendent les ID de MS Project (et non
    // position + 1, décalé d'un cran par l'UID 0), relus tels quels après
    // l'écriture.
    use plannercraft_core::{Resource, ResourceUid};
    let mut p = plannercraft_core::Project::new(plannercraft_core::ProjectInit {
        name: "P".to_owned(),
        anchor: plannercraft_core::ScheduleAnchor::FromStart(now()),
        current_date: now(),
        calendar_names: plannercraft_core::BuiltinCalendarNames::default(),
    });
    for (uid, name) in [(0, ""), (1, "Maçon"), (2, "Électricien")] {
        p.resources
            .push(Resource::new(ResourceUid::new(uid), name.to_owned()))
            .unwrap();
    }
    let xml = plannercraft_mspdi::write::write(
        &p,
        plannercraft_mspdi::write::Computed::Omit,
        &plannercraft_mspdi::write::WriteOptions::default(),
    )
    .xml;
    let text = String::from_utf8(xml).unwrap();
    for (uid, id) in [(0, 0), (1, 1), (2, 2)] {
        assert!(
            text.contains(&format!("<UID>{uid}</UID>\n\t\t\t<ID>{id}</ID>"))
                || text.contains(&format!("<UID>{uid}</UID><ID>{id}</ID>")),
            "UID {uid} : ID {id} attendu"
        );
    }
    let session = plannercraft_automation::Session::from_project(
        p,
        plannercraft_automation::DocOrigin {
            name: "p.xml".to_owned(),
            path: None,
        },
        now(),
        Language::Fr,
    );
    let mut ws = Workspace::new(now(), Language::Fr);
    ws.set_session(session);
    let list = ws.call("resource.list", json!({})).unwrap();
    let ids: Vec<(u64, u64)> = list["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["uid"].as_u64().unwrap(), r["id"].as_u64().unwrap()))
        .collect();
    assert_eq!(ids, [(1, 1), (2, 2)]);
}

#[test]
fn task_resources_bounded() {
    // Tâche à 150 affectations (fichier hostile) : `resources` coupée à 100
    // (C-MCP-03), `resources_total` = 150 ; absent sous la borne.
    use plannercraft_core::{Assignment, AssignmentUid, Resource, ResourceUid, Task, TaskUid};
    let mut p = plannercraft_core::Project::new(plannercraft_core::ProjectInit {
        name: "P".to_owned(),
        anchor: plannercraft_core::ScheduleAnchor::FromStart(now()),
        current_date: now(),
        calendar_names: plannercraft_core::BuiltinCalendarNames::default(),
    });
    for uid in 1..=2 {
        let t = Task::new(TaskUid::new(uid), format!("T{uid}"), &p.options);
        p.tasks.push(t).unwrap();
    }
    for uid in 1..=150 {
        p.resources
            .push(Resource::new(ResourceUid::new(uid), format!("R{uid}")))
            .unwrap();
        p.assignments
            .push(Assignment::new(
                AssignmentUid::new(uid),
                TaskUid::new(1),
                Some(ResourceUid::new(uid)),
            ))
            .unwrap();
    }
    p.assignments
        .push(Assignment::new(
            AssignmentUid::new(151),
            TaskUid::new(2),
            Some(ResourceUid::new(1)),
        ))
        .unwrap();
    let session = plannercraft_automation::Session::from_project(
        p,
        plannercraft_automation::DocOrigin {
            name: "p.xml".to_owned(),
            path: None,
        },
        now(),
        Language::Fr,
    );
    let mut ws = Workspace::new(now(), Language::Fr);
    ws.set_session(session);
    let big = ws.call("task.get", json!({ "uid": 1 })).unwrap();
    assert_eq!(
        big["resources"].as_array().unwrap().len(),
        plannercraft_automation::view::MAX_TASK_RESOURCES
    );
    assert_eq!(big["resources_total"], json!(150));
    let small = ws.call("task.get", json!({ "uid": 2 })).unwrap();
    assert_eq!(small["resources"].as_array().unwrap().len(), 1);
    assert!(small.get("resources_total").is_none());
}
