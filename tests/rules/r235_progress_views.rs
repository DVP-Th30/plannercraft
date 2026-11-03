//! R-235 — Vues de l'avancement et des lignes de base : `TaskView` +=
//! `progress { state, percent }` (omis si non commencée) ; `get_task` +=
//! avancement détaillé et lignes de base (au plus 11) ; `project_summary`
//! += date d'état effective, % du projet, numéros des lignes de base
//! présentes. Repère : calendrier Standard, début du projet lun 19/10/2026
//! 08:00. Hypothèse.

use plannercraft_automation::Workspace;
use plannercraft_automation::text::Language;
use plannercraft_tests::moment;
use serde_json::{Value, json};

/// A 2 j → B 2 j (FS). Rend l'espace et les UID de A et B.
fn ab() -> (Workspace, u64, u64) {
    let mut ws = Workspace::new(moment("2026-10-17T08:00"), Language::Fr);
    ws.call(
        "project.new",
        json!({ "name": "Vues", "start": "2026-10-19T08:00" }),
    )
    .unwrap();
    let out = ws
        .call(
            "batch.apply",
            json!({ "ops": [
                { "op": "task.add", "name": "A", "as": "$a", "duration": { "text": "2j" } },
                { "op": "task.add", "name": "B", "as": "$b", "duration": { "text": "2j" } },
                { "op": "link.add", "pred": "$a", "succ": "$b" }
            ] }),
        )
        .unwrap();
    (
        ws,
        out["labels"]["$a"].as_u64().unwrap(),
        out["labels"]["$b"].as_u64().unwrap(),
    )
}

fn row(list: &Value, uid: u64) -> &Value {
    list["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["uid"] == json!(uid))
        .unwrap()
}

#[test]
fn r235_task_progress_view() {
    let (mut ws, a, b) = ab();
    // Aucun avancement : clé absente partout.
    let list = ws.call("task.list", json!({})).unwrap();
    assert!(row(&list, a).get("progress").is_none());
    ws.call(
        "progress.update",
        json!({ "task": a, "percent_complete": { "text": "50%" } }),
    )
    .unwrap();
    let list = ws.call("task.list", json!({})).unwrap();
    assert_eq!(
        row(&list, a)["progress"],
        json!({ "state": "started", "percent": { "value": 5_000, "text": "50%" } })
    );
    assert!(row(&list, b).get("progress").is_none());
    ws.call(
        "progress.update",
        json!({ "task": a, "percent_complete": { "value": 10_000 } }),
    )
    .unwrap();
    let list = ws.call("task.list", json!({})).unwrap();
    assert_eq!(row(&list, a)["progress"]["state"], json!("complete"));
    assert_eq!(row(&list, a)["progress"]["percent"]["text"], json!("100%"));
    // Détail : valeurs du moteur.
    let detail = ws.call("task.get", json!({ "uid": a })).unwrap();
    let g = &detail["progress"];
    assert_eq!(g["actual_start"], json!("2026-10-19T08:00"));
    assert_eq!(g["actual_finish"], json!("2026-10-20T17:00"));
    assert_eq!(g["actual_duration"]["minutes"], json!(960));
    assert_eq!(g["remaining_duration"]["minutes"], json!(0));
    assert_eq!(g["stop"], json!("2026-10-20T17:00"));
    assert_eq!(g["resume"], json!("2026-10-20T17:00"));
    assert_eq!(g["physical_percent_complete"]["value"], json!(0));
    assert!(
        ws.call("task.get", json!({ "uid": b }))
            .unwrap()
            .get("progress")
            .is_none()
    );
}

#[test]
fn r235_get_task_baselines() {
    let (mut ws, a, b) = ab();
    let summary = ws.call("project.summary", json!({})).unwrap();
    // Date d'état effective : la date du jour du projet tant qu'aucune
    // n'est posée (R-034) ; pas de ligne de base, pas de % du projet.
    assert_eq!(summary["status_date"], json!("2026-10-17T08:00"));
    assert!(summary.get("baselines").is_none());
    assert!(summary.get("percent_complete").is_none());
    assert!(
        ws.call("task.get", json!({ "uid": a }))
            .unwrap()
            .get("baselines")
            .is_none()
    );
    ws.call(
        "batch.apply",
        json!({ "ops": [
            { "op": "baseline.set" },
            { "op": "baseline.set", "number": 2, "tasks": [b] },
            { "op": "project.set_status_date", "date": "2026-10-20T17:00" },
            { "op": "progress.update", "task": a, "percent_complete": { "text": "50%" } }
        ] }),
    )
    .unwrap();
    let detail = ws.call("task.get", json!({ "uid": b })).unwrap();
    assert_eq!(
        detail["baselines"],
        json!([
            {
                "number": 0,
                "start": "2026-10-21T08:00",
                "finish": "2026-10-22T17:00",
                "duration": { "minutes": 960, "unit": "day", "text": "2j", "elapsed": false, "estimated": false }
            },
            {
                "number": 2,
                "start": "2026-10-21T08:00",
                "finish": "2026-10-22T17:00",
                "duration": { "minutes": 960, "unit": "day", "text": "2j", "elapsed": false, "estimated": false }
            }
        ])
    );
    let summary = ws.call("project.summary", json!({})).unwrap();
    assert_eq!(summary["status_date"], json!("2026-10-20T17:00"));
    assert_eq!(summary["baselines"], json!([0, 2]));
    // % du projet (R-210) : 480 / 1 920 min = 25 %.
    assert_eq!(summary["percent_complete"]["value"], json!(2_500));
}
