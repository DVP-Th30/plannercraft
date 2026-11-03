//! Lot L7 (J6, R-230 à R-233) : vue `analysis.explain_date` par
//! l'espace de travail — forme JSON, durées `{minutes, unit, text}`,
//! erreurs structurées.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

use plannercraft_automation::Workspace;
use plannercraft_automation::error::codes;
use plannercraft_automation::text::Language;
use plannercraft_core::Moment;
use serde_json::{Value, json};

fn now() -> Moment {
    "2026-10-19T08:00".parse().unwrap()
}

/// A 1 j, B 2 j, C 1 j FS de A et de B ; R-233.
fn workspace() -> (Workspace, Value) {
    let mut ws = Workspace::new(now(), Language::Fr);
    ws.call(
        "project.new",
        json!({ "name": "Maison", "start": "2026-10-19T08:00" }),
    )
    .unwrap();
    let out = ws
        .call(
            "batch.apply",
            json!({ "format_version": 1, "ops": [
                { "op": "task.add", "name": "A", "as": "$a", "duration": { "text": "1j" } },
                { "op": "task.add", "name": "B", "as": "$b", "duration": { "text": "2j" } },
                { "op": "task.add", "name": "C", "as": "$c", "duration": { "text": "1j" } },
                { "op": "link.add", "pred": "$a", "succ": "$c" },
                { "op": "link.add", "pred": "$b", "succ": "$c" }
            ] }),
        )
        .unwrap();
    (ws, out["labels"].clone())
}

#[test]
fn explain_view_json() {
    let (mut ws, labels) = workspace();
    let (b, c) = (labels["$b"].clone(), labels["$c"].clone());
    let v = ws.call("analysis.explain_date", json!({ "task": c })).unwrap();
    assert_eq!(v["task"], json!({ "uid": c, "id": 3, "name": "C" }));
    assert_eq!(v["side"], json!("start"));
    assert_eq!(v["date"], json!("2026-10-21T08:00"));
    assert_eq!(
        v["steps"],
        json!([
            { "step": "driver", "driver": { "kind": "link", "predecessor": b, "link": "fs" },
              "raw": "2026-10-20T17:00" },
            { "step": "snap", "raw": "2026-10-20T17:00", "snapped": "2026-10-21T08:00",
              "calendar": { "uid": 1, "name": "Standard" } }
        ])
    );
    assert_eq!(v["chain"][0]["uid"], b);
    assert_eq!(v["chain"][0]["name"], json!("B"));
    assert_eq!(v["chain"][0]["driver"], json!({ "kind": "project_start" }));
    assert_eq!(v["truncated"], json!(false));
    assert_eq!(v["hypothesis"], json!(false));
    assert_eq!(v["late"]["critical"], json!(true));
    assert_eq!(v["late"]["driver"], json!({ "kind": "project_finish" }));
    assert_eq!(v["late"]["total_slack"]["minutes"], json!(0));
    assert!(
        v["summary_fr"]
            .as_str()
            .unwrap()
            .starts_with("C commence le mer 21/10/2026 08:00 : fin de B")
    );
    assert!(
        v["summary_en"]
            .as_str()
            .unwrap()
            .starts_with("C starts on Wed 10/21/2026")
    );
    // Fin : durée au format {minutes, unit, text}.
    let v = ws
        .call("analysis.explain_date", json!({ "task": c, "side": "finish" }))
        .unwrap();
    assert_eq!(v["date"], json!("2026-10-21T17:00"));
    let last = v["steps"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["step"], json!("duration"));
    assert_eq!(last["duration"]["minutes"], json!(480));
    assert_eq!(last["duration"]["text"], json!("1j"));
    // Récapitulative du projet (UID 0) : agrégat.
    let v = ws.call("analysis.explain_date", json!({ "task": 0 })).unwrap();
    assert_eq!(v["steps"][0]["driver"], json!({ "kind": "rollup" }));
    // Lecture seule : l'historique ne bouge pas.
    let h = ws.call("history.list", json!({})).unwrap();
    assert_eq!(h["entries"].as_array().map(Vec::len), Some(1));
}

#[test]
fn explain_view_errors() {
    let (mut ws, labels) = workspace();
    let c = labels["$c"].clone();
    let e = ws
        .call("analysis.explain_date", json!({ "task": 99 }))
        .unwrap_err();
    assert_eq!(e.code, codes::TASK_NOT_FOUND);
    for depth in [0, 51] {
        let e = ws
            .call("analysis.explain_date", json!({ "task": c, "depth": depth }))
            .unwrap_err();
        assert_eq!(e.code, codes::PARAM_INVALID, "{depth}");
    }
    let e = ws
        .call("analysis.explain_date", json!({ "task": c, "bogus": 1 }))
        .unwrap_err();
    assert_eq!(e.code, codes::PARAM_INVALID);
    let e = ws
        .call("analysis.explain_date", json!({ "task": "$c" }))
        .unwrap_err();
    assert_eq!(e.code, codes::BATCH_UNKNOWN_LABEL);
    // Tâche inactive : non planifiée.
    ws.call("task.update", json!({ "uid": c, "active": false }))
        .unwrap();
    let e = ws
        .call("analysis.explain_date", json!({ "task": c }))
        .unwrap_err();
    assert_eq!(e.code, codes::EXPLAIN_NOT_SCHEDULED);
    // Sans session.
    let mut empty = Workspace::new(now(), Language::Fr);
    assert!(empty.call("analysis.explain_date", json!({ "task": 1 })).is_err());
}
