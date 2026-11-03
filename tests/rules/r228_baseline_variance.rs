//! R-228 — `baseline_variance` : ligne du projet toujours, puis une ligne
//! par tâche planifiée `{uid, id, name, start, finish, duration, work,
//! baseline | null, variance?}`, écarts du moteur (R-224, R-225) au format
//! durée, triés par |écart de fin| décroissant puis ID, filtre
//! `only_varied`, pagination (50 par défaut, 500 au plus, curseur).
//! Repère : calendrier Standard, début du projet lun 19/10/2026 08:00.
//! Hypothèse.

use plannercraft_automation::Workspace;
use plannercraft_automation::text::Language;
use plannercraft_tests::{moment, tests_path};
use serde_json::{Value, json};

/// Cinq tâches indépendantes de 2 j (lun 19 – mar 20/10), ligne 0 posée,
/// puis durées changées : T1 2 j (0), T2 1,5 j (−240), T3 3 j (+480),
/// T4 1 j (−480), T5 4 j (+960). Rend l'espace et les UID dans l'ordre.
fn five() -> (Workspace, Vec<u64>) {
    let mut ws = Workspace::new(moment("2026-10-19T08:00"), Language::Fr);
    ws.call(
        "project.new",
        json!({ "name": "Écarts", "start": "2026-10-19T08:00" }),
    )
    .unwrap();
    let mut ops: Vec<Value> = (1..=5)
        .map(|i| json!({ "op": "task.add", "name": format!("T{i}"), "as": format!("$t{i}"), "duration": { "text": "2j" } }))
        .collect();
    ops.push(json!({ "op": "baseline.set" }));
    let out = ws.call("batch.apply", json!({ "ops": ops })).unwrap();
    let uids: Vec<u64> = (1..=5)
        .map(|i| out["labels"][format!("$t{i}")].as_u64().unwrap())
        .collect();
    let edits: Vec<Value> = [(1, "2j"), (2, "1,5j"), (3, "3j"), (4, "1j"), (5, "4j")]
        .iter()
        .map(|(i, d)| json!({ "op": "task.update", "uid": uids[i - 1], "duration": { "text": d } }))
        .collect();
    ws.call("batch.apply", json!({ "ops": edits })).unwrap();
    (ws, uids)
}

fn finish_minutes(row: &Value) -> i64 {
    row["variance"]["finish"]["minutes"].as_i64().unwrap()
}

#[test]
fn r228_variance_pagination() {
    let (mut ws, uids) = five();
    let v = ws.call("analysis.baseline_variance", json!({})).unwrap();
    assert_eq!(v["baseline"], json!(0));
    assert_eq!(v["total"], json!(5));
    assert_eq!(v["next_cursor"], Value::Null);
    // Tri : |écart de fin| décroissant, puis ID.
    let order: Vec<(u64, i64)> = v["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["uid"].as_u64().unwrap(), finish_minutes(r)))
        .collect();
    assert_eq!(
        order,
        [
            (uids[4], 960),
            (uids[2], 480),
            (uids[3], -480),
            (uids[1], -240),
            (uids[0], 0)
        ]
    );
    // Ligne d'une tâche : valeurs, ligne de base, écarts au format durée.
    let t2 = &v["tasks"][3];
    assert_eq!(t2["finish"], json!("2026-10-20T12:00"));
    assert_eq!(t2["baseline"]["finish"], json!("2026-10-20T17:00"));
    assert_eq!(t2["baseline"]["duration"]["text"], json!("2j"));
    assert_eq!(t2["variance"]["finish"]["text"], json!("-0,5j"));
    assert_eq!(t2["variance"]["start"]["minutes"], json!(0));
    assert_eq!(t2["variance"]["duration"]["minutes"], json!(-240));
    // Ligne du projet (déduite : projet créé par PlannerCraft) : fin de base mar
    // 20/10 17:00, fin courante jeu 22/10 17:00 (T5) ⇒ +2 j.
    let project = &v["project"];
    assert_eq!(project["uid"], json!(0));
    assert_eq!(project["derived"], json!(true));
    assert_eq!(project["baseline"]["finish"], json!("2026-10-20T17:00"));
    assert_eq!(finish_minutes(project), 960);
    // Pages de 2 : curseur = UID de la dernière ligne de la page.
    let mut seen = Vec::new();
    let mut cursor = Value::Null;
    loop {
        let mut params = json!({ "limit": 2 });
        if !cursor.is_null() {
            params["cursor"] = cursor.clone();
        }
        let page = ws.call("analysis.baseline_variance", params).unwrap();
        assert_eq!(page["total"], json!(5));
        assert_eq!(page["project"], v["project"]);
        for r in page["tasks"].as_array().unwrap() {
            seen.push(r["uid"].as_u64().unwrap());
        }
        cursor = page["next_cursor"].clone();
        if cursor.is_null() {
            break;
        }
    }
    let all: Vec<u64> = order.iter().map(|(u, _)| *u).collect();
    assert_eq!(seen, all);
    // Refus.
    for (params, code, field) in [
        (json!({ "limit": 0 }), "param.invalid", "limit"),
        (json!({ "limit": 501 }), "limit.exceeded", "limit"),
        (json!({ "baseline": 11 }), "param.invalid", "baseline"),
        (json!({ "cursor": 999 }), "task.not_found", "cursor"),
    ] {
        let e = ws.call("analysis.baseline_variance", params).unwrap_err();
        assert_eq!((e.code, e.field.as_deref()), (code, Some(field)));
    }
}

#[test]
fn r228_only_varied() {
    let (mut ws, uids) = five();
    // Une tâche ajoutée après la ligne de base : sans ligne (`null`, écarts
    // omis), listée par défaut, écartée par `only_varied`.
    let out = ws
        .call(
            "task.add",
            json!({ "name": "Nouvelle", "duration": { "text": "1j" } }),
        )
        .unwrap();
    let fresh = out["created_uids"][0].as_u64().unwrap();
    let v = ws.call("analysis.baseline_variance", json!({})).unwrap();
    assert_eq!(v["total"], json!(6));
    let row = v["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["uid"] == json!(fresh))
        .unwrap();
    assert_eq!(row["baseline"], Value::Null);
    assert!(row.get("variance").is_none());
    let v = ws
        .call("analysis.baseline_variance", json!({ "only_varied": true }))
        .unwrap();
    let kept: Vec<u64> = v["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["uid"].as_u64().unwrap())
        .collect();
    assert_eq!(kept, [uids[4], uids[2], uids[3], uids[1]]);
    assert_eq!(v["total"], json!(4));
    // Ligne absente : projet sans ligne, tâches sans ligne.
    let v = ws
        .call("analysis.baseline_variance", json!({ "baseline": 1 }))
        .unwrap();
    assert_eq!(v["project"]["baseline"], Value::Null);
    assert!(
        v["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["baseline"].is_null())
    );
    let v = ws
        .call(
            "analysis.baseline_variance",
            json!({ "baseline": 1, "only_varied": true }),
        )
        .unwrap();
    assert_eq!(v["total"], json!(0));
}

#[test]
fn r228_corpus_a7_stored_project_line() {
    // A7 (O42) : ligne 0 égale aux valeurs courantes, récapitulative du
    // projet stockée ⇒ ligne du projet lue (non déduite), écarts de dates
    // nuls partout.
    let path = tests_path("corpus/files/projinteg.xml");
    if !path.exists() {
        return;
    }
    let mut ws = Workspace::new(moment("2026-01-01T00:00"), Language::Fr);
    ws.call("project.open", json!({ "path": path.display().to_string() }))
        .unwrap();
    let v = ws
        .call("analysis.baseline_variance", json!({ "limit": 500 }))
        .unwrap();
    assert!(v["project"].get("derived").is_none());
    assert!(!v["project"]["baseline"].is_null());
    assert_eq!(v["project"]["variance"]["finish"]["minutes"], json!(0));
    assert_eq!(v["total"], json!(25));
    for r in v["tasks"].as_array().unwrap() {
        assert_eq!(r["variance"]["start"]["minutes"], json!(0), "{r}");
        assert_eq!(r["variance"]["finish"]["minutes"], json!(0), "{r}");
    }
}
