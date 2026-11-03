//! Lot L8 J6 : vues JSON du suivi — tâche avec avancement et lignes de
//! base (R-235), résumé du projet (date d'état, %, lignes), écarts à une
//! ligne de base (R-228) et simulation `what_if` (R-234) — au travers du
//! `Workspace` (même routeur que le MCP et la CLI). Références :
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
use plannercraft_automation::text::Language;
use plannercraft_core::Moment;
use serde_json::{Value, json};

fn now() -> Moment {
    "2026-11-02T08:00".parse().unwrap()
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

/// Chantier suivi : Terrassement 3 j → Maçonnerie 5 j (Maçon) → Réception
/// (jalon), ligne 0 posée au lun 02/11/2026, date d'état ven 06/11 17:00 ;
/// Terrassement achevé, Maçonnerie commencée mer 04/11 (40 %).
fn site() -> (Workspace, Value) {
    let mut ws = Workspace::new(now(), Language::Fr);
    ws.call(
        "project.new",
        json!({ "name": "Chantier", "start": "2026-11-02T08:00" }),
    )
    .unwrap();
    let out = ws
        .call(
            "batch.apply",
            json!({ "ops": [
                { "op": "resource.add", "name": "Maçon", "as": "$m" },
                { "op": "task.add", "name": "Terrassement", "as": "$t", "duration": { "text": "3j" } },
                { "op": "task.add", "name": "Maçonnerie", "as": "$ma", "duration": { "text": "5j" } },
                { "op": "task.add", "name": "Réception", "as": "$r", "duration": { "text": "0j" } },
                { "op": "link.add", "pred": "$t", "succ": "$ma" },
                { "op": "link.add", "pred": "$ma", "succ": "$r" },
                { "op": "assignment.add", "task": "$ma", "resource": "$m" },
                { "op": "baseline.set" }
            ] }),
        )
        .unwrap();
    ws.call(
        "batch.apply",
        json!({ "ops": [
            { "op": "project.set_status_date", "date": "2026-11-06T17:00" },
            { "op": "progress.update", "task": out["labels"]["$t"], "percent_complete": { "text": "100%" } },
            { "op": "progress.update", "task": out["labels"]["$ma"], "actual_start": "2026-11-04T08:00",
              "percent_complete": { "text": "40%" } }
        ] }),
    )
    .unwrap();
    (ws, out)
}

#[test]
fn views_golden() {
    let (mut ws, out) = site();
    let ma = out["labels"]["$ma"].clone();
    let r = out["labels"]["$r"].clone();
    golden(
        "task_get_progress",
        &ws.call("task.get", json!({ "uid": ma.clone() })).unwrap(),
    );
    golden(
        "summary_tracking",
        &ws.call("project.summary", json!({})).unwrap(),
    );
    golden(
        "baseline_variance",
        &ws.call("analysis.baseline_variance", json!({})).unwrap(),
    );
    golden(
        "what_if",
        &ws.call(
            "analysis.what_if",
            json!({
                "ops": [{ "op": "task.update", "uid": ma, "duration": { "text": "7j" } }],
                "baseline": 0,
                "explain": [r]
            }),
        )
        .unwrap(),
    );
    golden("task_list_progress", &ws.call("task.list", json!({})).unwrap());
}
