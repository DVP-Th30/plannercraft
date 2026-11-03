//! R-236 — Filtre `late` de `list_tasks` : une tâche planifiée est en
//! retard si sa fin planifiée dépasse son échéance (J4) ou la fin de sa
//! ligne de base 0, ou si, à la date d'état effective, elle n'est pas
//! achevée alors que sa fin planifiée est atteinte, ou pas commencée alors
//! que son début planifié est passé. Repère : calendrier Standard, début du
//! projet lun 02/11/2026 08:00, date d'état ven 13/11/2026 17:00.
//! Hypothèse.

use plannercraft_automation::Workspace;
use plannercraft_automation::text::Language;
use plannercraft_tests::moment;
use serde_json::{Value, json};

fn late(ws: &mut Workspace) -> Vec<String> {
    let v = ws.call("task.list", json!({ "late": true })).unwrap();
    v["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn r236_late_filter() {
    let mut ws = Workspace::new(moment("2026-11-02T08:00"), Language::Fr);
    ws.call(
        "project.new",
        json!({ "name": "Retards", "start": "2026-11-02T08:00" }),
    )
    .unwrap();
    let task = |name: &str, days: &str| -> Value {
        json!({ "op": "task.add", "name": name, "as": format!("${}", name.to_lowercase()), "duration": { "text": days } })
    };
    let snet = |name: &str, date: &str| -> Value {
        json!({ "op": "task.set_constraint", "uid": format!("${}", name.to_lowercase()), "kind": "snet", "date": date })
    };
    ws.call(
        "batch.apply",
        json!({ "ops": [
            // U : non commencée, prévue jeu 12/11 (exemple de R-236).
            task("U", "1j"), snet("U", "2026-11-12T08:00"),
            // V : achevée (début et fin passés) ; W : future.
            task("V", "1j"),
            task("W", "1j"), snet("W", "2026-11-16T08:00"),
            // X : future, échéance dépassée par sa fin (J4).
            task("X", "1j"), snet("X", "2026-11-16T08:00"),
            { "op": "task.set_deadline", "uid": "$x", "deadline": "2026-11-16T12:00" },
            // Y : future, retardée après la ligne 0.
            task("Y", "1j"), snet("Y", "2026-11-16T08:00"),
            // Z : en cours, fin après la date d'état ; Q : en cours, fin
            // atteinte à la date d'état.
            task("Z", "15j"), task("Q", "10j"),
            { "op": "baseline.set" },
            { "op": "project.set_status_date", "date": "2026-11-13T17:00" },
            { "op": "progress.update", "task": "$v", "percent_complete": { "text": "100%" } },
            { "op": "progress.update", "task": "$z", "percent_complete": { "text": "50%" } },
            { "op": "progress.update", "task": "$q", "percent_complete": { "text": "50%" } }
        ] }),
    )
    .unwrap();
    assert_eq!(late(&mut ws), ["U", "X", "Q"]);
    // Y glisse d'un jour après sa ligne 0 : en retard.
    let list = ws.call("task.list", json!({})).unwrap();
    let y = list["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == json!("Y"))
        .unwrap()["uid"]
        .clone();
    ws.call(
        "task.set_constraint",
        json!({ "uid": y, "kind": "snet", "date": "2026-11-17T08:00" }),
    )
    .unwrap();
    assert_eq!(late(&mut ws), ["U", "X", "Y", "Q"]);
    // Date d'état retirée : la date du jour du projet (lun 02/11 08:00)
    // fait foi ; plus rien n'est en retard par la date d'état.
    ws.call("project.set_status_date", json!({ "date": null }))
        .unwrap();
    assert_eq!(late(&mut ws), ["X", "Y"]);
}
