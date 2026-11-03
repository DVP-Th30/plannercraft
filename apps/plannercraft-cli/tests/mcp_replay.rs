//! Rejeu des scénarios `build_30`, `resources_j5` (J5, lot L9), `tracking_j6` (J6, lot L10) et `calendars_j7` (J7, lot L5) sur le
//! vrai binaire (lot L18, vérification 4) : `plannercraft --now 2026-10-19T08:00 --lang fr mcp
//! --headless --journal J` (`--now 2026-11-02T08:00` pour `tracking_j6`) lancé en processus fils, client rmcp sur ses
//! tubes (stdio). Mêmes résultats que le rejeu en mémoire
//! (`crates/plannercraft-mcp/tests/replay.rs`), journal produit égal au scénario
//! octet pour octet (déterminisme).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::missing_panics_doc,
    reason = "tests : un échec doit s'arrêter net"
)]

#[path = "../../../crates/plannercraft-mcp/tests/common/replay.rs"]
mod replay;

use std::process::Stdio;
use std::time::Duration;

use replay::{Scenario, TempDir, logged, normalized, now_of, repo_root, substitute};
use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use serde_json::Value;

#[tokio::test]
async fn replay_build_30_stdio_binary() {
    replay_stdio("build_30").await;
}

/// Scénario J5 (lot L9) : ressources, affectations, surutilisation.
#[tokio::test]
async fn replay_resources_j5_stdio() {
    replay_stdio("resources_j5").await;
}

/// Scénario de suivi J6 (lot L10) : avancement, lignes de base, analyse.
#[tokio::test]
async fn replay_tracking_j6_stdio() {
    replay_stdio("tracking_j6").await;
}

/// Scénario J7 (lot L5) : calendriers, calendrier du projet, taux,
/// effacement d'une ligne de base.
#[tokio::test]
async fn replay_calendars_j7_stdio() {
    replay_stdio("calendars_j7").await;
}

/// Scénario J8 (lot L16) : usage quotidien par un agent (filtres de
/// `list_tasks`, mode manuel, dates manuelles, inactivation, export PDF).
#[tokio::test]
async fn replay_daily_j8_stdio() {
    replay_stdio("daily_j8").await;
}

/// Rejoue le scénario `name` sur le binaire, en stdio.
async fn replay_stdio(name: &str) {
    let root = repo_root(env!("CARGO_MANIFEST_DIR"));
    let scenario = Scenario::load(&root, name);
    let tmp = TempDir::new("stdio");
    let journal = tmp.path().join("run.jsonl");
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_plannercraft"))
        .args([
            "--now",
            now_of(name),
            "--lang",
            "fr",
            "mcp",
            "--headless",
            "--journal",
        ])
        .arg(&journal)
        // `--headless` garde la garde ADR-0002 (J7) : dossier de
        // rendez-vous vide, résultat indépendant du poste.
        .env("PLANNERCRAFT_CONTROL_DIR", tmp.path().join("control"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let stdin = child.stdin.take().unwrap();
    let client = ().serve((stdout, stdin)).await.unwrap();

    let mut got = Vec::with_capacity(scenario.calls.len());
    for c in &scenario.calls {
        let mut req = CallToolRequestParams::new(c.tool.clone());
        if let Value::Object(args) = substitute(&c.params, tmp.path()) {
            req = req.with_arguments(args);
        }
        let r = client.call_tool(req).await.unwrap();
        got.push(normalized(&logged(r.is_error, r.structured_content), tmp.path()));
    }
    // Entrée fermée : le serveur rend la main et sort avec le code 0.
    client.cancel().await.unwrap();
    let status = tokio::time::timeout(Duration::from_secs(30), child.wait())
        .await
        .expect("le serveur ne s'est pas arrêté après la fermeture de son entrée")
        .unwrap();
    assert!(status.success(), "{status}");

    replay::assert_results(&scenario, &got);
    let produced = std::fs::read_to_string(&journal).unwrap();
    replay::assert_journal(&scenario, &produced, tmp.path());
}
