//! Lot L4 J7 (ADR-0015 D7) : vue `analysis.resource_usage` par trimestre
//! (R-246) et lignes par affectation (R-245), au travers du `Workspace`
//! (même routeur que le MCP et la CLI). Références :
//! `tests/golden/views/*.json`, régénérées par `PLANNERCRAFT_BLESS=1`.

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
    "2026-03-30T08:00".parse().unwrap()
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

/// Projet neuf (lun 30/03/2026 08:00, Standard) : T 4 j (lun 30/03 - jeu
/// 02/04) au Maçon 100 % et au Ciment (matérielle, 5 sacs) ; U 1 j au
/// Maçon 50 %.
fn site() -> (Workspace, Value) {
    let mut ws = Workspace::new(now(), Language::Fr);
    ws.call(
        "project.new",
        json!({ "name": "Trimestre", "start": "2026-03-30T08:00" }),
    )
    .unwrap();
    let out = ws
        .call(
            "batch.apply",
            json!({ "ops": [
                { "op": "resource.add", "name": "Maçon", "as": "$m" },
                { "op": "resource.add", "name": "Ciment", "as": "$c", "kind": "material", "material_label": "sac" },
                { "op": "task.add", "name": "T", "as": "$t", "duration": { "text": "4j" } },
                { "op": "task.add", "name": "U", "as": "$u", "duration": { "text": "1j" } },
                { "op": "assignment.add", "task": "$t", "resource": "$m" },
                { "op": "assignment.add", "task": "$t", "resource": "$c", "quantity": { "text": "5" } },
                { "op": "assignment.add", "task": "$u", "resource": "$m", "units": { "text": "50%" } }
            ] }),
        )
        .unwrap();
    (ws, out)
}

/// R-246 : périodes civiles du 01/01 et du 01/04 ; 16 h au T1, 16 h au T2
/// pour T ; sans `assignments`, ni lignes d'affectation ni totaux (JSON J5).
#[test]
fn view_usage_quarter_json() {
    let (mut ws, _) = site();
    let q = ws
        .call(
            "analysis.resource_usage",
            json!({ "by": "quarter", "from": "2026-02-15T00:00", "to": "2026-05-20T00:00" }),
        )
        .unwrap();
    assert_eq!(q["by"], json!("quarter"));
    assert_eq!(q["periods"], json!(["2026-01-01T00:00", "2026-04-01T00:00"]));
    assert!(q.get("assignments").is_none() && q.get("task_totals").is_none());
    golden("resource_usage_quarter", &q);
    let full = ws
        .call(
            "analysis.resource_usage",
            json!({ "by": "quarter", "from": "2026-02-15T00:00", "to": "2026-05-20T00:00", "assignments": true }),
        )
        .unwrap();
    assert_eq!(full["rows"], q["rows"]);
    golden("resource_usage_quarter_assignments", &full);
    // Le schéma de l'outil annonce le trimestre et `assignments`.
    let schema = serde_json::to_value((plannercraft_automation::registry::lookup(
        "analysis.resource_usage",
    )
    .unwrap()
    .schema)())
    .unwrap();
    let text = schema.to_string();
    assert!(text.contains("\"quarter\""), "{text}");
    assert!(schema["properties"].get("assignments").is_some());
}

/// R-245 : Σ des lignes d'affectation du Maçon = sa ligne ; total de T sans
/// la quantité de Ciment ; ressource inconnue refusée.
#[test]
fn view_usage_assignments_sum() {
    let (mut ws, out) = site();
    let m = out["resource_labels"]["$m"].as_u64().unwrap();
    let t = out["labels"]["$t"].as_u64().unwrap();
    let v = ws
        .call(
            "analysis.resource_usage",
            json!({ "by": "day", "assignments": true }),
        )
        .unwrap();
    let minutes = |row: &Value| -> Vec<i64> {
        row["work"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| w["minutes"].as_i64().unwrap())
            .collect()
    };
    let rows = v["rows"].as_array().unwrap();
    let mason = rows.iter().find(|r| r["uid"] == json!(m)).unwrap();
    let assignments = v["assignments"].as_array().unwrap();
    assert_eq!(assignments.len(), 3);
    let n = v["periods"].as_array().unwrap().len();
    let mut sum = vec![0_i64; n];
    for a in assignments.iter().filter(|a| a["resource"] == json!(m)) {
        for (s, w) in sum.iter_mut().zip(minutes(a)) {
            *s += w;
        }
    }
    assert_eq!(sum, minutes(mason));
    let totals = v["task_totals"].as_array().unwrap();
    let total_t = totals.iter().find(|x| x["uid"] == json!(t)).unwrap();
    assert_eq!(total_t["name"], json!("T"));
    assert_eq!(minutes(total_t).iter().sum::<i64>(), 1_920);
    // Ressource inconnue : refus du J5 inchangé.
    let err = ws
        .call(
            "analysis.resource_usage",
            json!({ "resources": [77], "assignments": true }),
        )
        .unwrap_err();
    assert_eq!(err.code, codes::RESOURCE_NOT_FOUND);
}
