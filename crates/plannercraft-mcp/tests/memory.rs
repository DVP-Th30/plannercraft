//! Serveur MCP en mémoire : client rmcp sur `tokio::io::duplex` (lot L8,
//! C-MCP-04 à C-MCP-07).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::missing_panics_doc,
    reason = "tests : un échec doit s'arrêter net"
)]

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use plannercraft_automation::REGISTRY;
use plannercraft_automation::journal::parse_line;
use plannercraft_automation::text::Language;
use plannercraft_core::Moment;
use plannercraft_mcp::{McpServeError, ServeOptions, serve_on};
use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::service::RunningService;
use rmcp::{RoleClient, ServiceExt};
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tokio::task::JoinHandle;

fn now() -> Moment {
    "2026-10-19T08:00".parse().unwrap()
}

fn opts() -> ServeOptions {
    // Garde ADR-0002 testée à part (guard.rs) : ici, aucun rendez-vous lu.
    let mut o = ServeOptions::new(now(), Language::En);
    o.ignore_app_guard = true;
    o
}

/// Dossier temporaire propre à un test (retiré à la fin du test).
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("plannercraft-mcp-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

type Client = RunningService<RoleClient, ()>;

/// Serveur sur un `duplex`, client rmcp connecté.
async fn connect(opts: ServeOptions) -> (Client, JoinHandle<Result<(), McpServeError>>) {
    let (client_io, server_io) = tokio::io::duplex(1 << 20);
    let (r, w) = tokio::io::split(server_io);
    let server = tokio::spawn(serve_on(r, w, opts));
    let client = ().serve(client_io).await.unwrap();
    (client, server)
}

async fn call(client: &Client, name: &'static str, args: Value) -> CallToolResult {
    let Value::Object(args) = args else {
        panic!("arguments : objet attendu")
    };
    client
        .call_tool(CallToolRequestParams::new(name).with_arguments(args))
        .await
        .unwrap()
}

/// Appel réussi : contenu structuré.
async fn ok(client: &Client, name: &'static str, args: Value) -> Value {
    let r = call(client, name, args).await;
    assert_eq!(r.is_error, Some(false), "{name} : {:?}", r.structured_content);
    r.structured_content.unwrap()
}

/// Appel refusé : erreur structurée.
async fn refused(client: &Client, name: &'static str, args: Value) -> Value {
    let r = call(client, name, args).await;
    assert_eq!(r.is_error, Some(true), "{name} : {:?}", r.structured_content);
    r.structured_content.unwrap()
}

async fn new_project(client: &Client) {
    ok(
        client,
        "new_project",
        json!({ "name": "Maison", "start": "2026-11-02T08:00" }),
    )
    .await;
}

#[tokio::test]
async fn mcp_list_tools_from_registry() {
    let (client, _server) = connect(opts()).await;
    let tools = client.list_all_tools().await.unwrap();
    let listed: BTreeSet<String> = tools.iter().map(|t| t.name.to_string()).collect();
    let expected: BTreeSet<String> = REGISTRY
        .iter()
        .filter(|s| s.is_listed())
        .map(|s| s.mcp_name.to_owned())
        .collect();
    assert_eq!(listed, expected);
    assert_eq!(tools.len(), expected.len());
    for t in &tools {
        // C-MCP-07.
        assert!(
            (1..=64).contains(&t.name.len())
                && t.name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
            "{}",
            t.name
        );
        let spec = plannercraft_automation::registry::by_mcp_name(&t.name).unwrap();
        assert_eq!(t.input_schema.get("type"), Some(&json!("object")));
        let props = t.input_schema.get("properties").and_then(Value::as_object);
        let write = spec.kind == plannercraft_automation::Kind::Write;
        assert_eq!(
            props.is_some_and(|p| p.contains_key("dry_run")),
            write,
            "{}",
            t.name
        );
        let read_only = t.annotations.as_ref().and_then(|a| a.read_only_hint);
        let reads = matches!(
            spec.kind,
            plannercraft_automation::Kind::Read | plannercraft_automation::Kind::Analysis
        );
        assert_eq!(read_only, Some(reads), "{}", t.name);
        assert!(t.description.as_deref().is_some_and(|d| !d.is_empty()));
    }
    let info = client.peer_info().unwrap();
    assert!(info.capabilities.tools.is_some());
    assert!(
        info.instructions
            .as_deref()
            .is_some_and(|i| i.contains("dry_run"))
    );
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn mcp_new_project_add_task_roundtrip() {
    let (client, _server) = connect(opts()).await;
    new_project(&client).await;
    let out = ok(
        &client,
        "add_task",
        json!({ "name": "Fondations", "duration": { "text": "5d" } }),
    )
    .await;
    assert_eq!(out["committed"], json!(true));
    assert_eq!(out["history_len"], json!(1));
    let uid = out["created_uids"][0].as_u64().unwrap();
    let created = &out["created"][0];
    assert_eq!(created["uid"].as_u64(), Some(uid));
    assert_eq!(created["name"], json!("Fondations"));
    // Dates du moteur (calendrier Standard : 08:00-12:00, 13:00-17:00).
    assert_eq!(created["start"], json!("2026-11-02T08:00"));
    assert_eq!(created["finish"], json!("2026-11-06T17:00"));
    assert_eq!(created["duration"]["minutes"], json!(2400));

    let got = ok(&client, "get_task", json!({ "uid": uid })).await;
    assert_eq!(&got, created);
    let summary = ok(&client, "project_summary", json!({})).await;
    assert_eq!(summary["tasks"], json!(1));
    assert_eq!(summary["finish"], json!("2026-11-06T17:00"));
    assert_eq!(summary["history_len"], json!(1));
    // Le texte du résultat est le même JSON (clients sans structuredContent).
    let r = call(&client, "project_summary", json!({})).await;
    let text = r.content[0].as_text().unwrap().text.clone();
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), summary);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn mcp_dry_run_no_history() {
    let (client, _server) = connect(opts()).await;
    new_project(&client).await;
    let out = ok(
        &client,
        "add_task",
        json!({ "name": "A", "duration": { "text": "2d" }, "dry_run": true }),
    )
    .await;
    assert_eq!(out["committed"], json!(false));
    assert_eq!(out["history_len"], json!(0));
    assert_eq!(out["diff"]["finish_after"], json!("2026-11-03T17:00"));
    let batch = ok(
        &client,
        "apply_batch",
        json!({ "ops": [
            { "op": "task.add", "name": "B", "as": "$b" },
            { "op": "task.add", "name": "C", "as": "$c" },
            { "op": "link.add", "pred": "$b", "succ": "$c" }
        ], "dry_run": true }),
    )
    .await;
    assert_eq!(batch["committed"], json!(false));
    // C-MCP-04 : aucune entrée d'historique, aucune tâche.
    let history = ok(&client, "history", json!({})).await;
    assert_eq!(ok(&client, "project_summary", json!({})).await["tasks"], json!(0));
    assert_eq!(
        ok(&client, "project_summary", json!({})).await["history_len"],
        json!(0)
    );
    assert_eq!(history["entries"], json!([]));
    // Puis un vrai lot : une seule entrée pour trois commandes.
    let batch = ok(
        &client,
        "apply_batch",
        json!({ "ops": [
            { "op": "task.add", "name": "B", "as": "$b" },
            { "op": "task.add", "name": "C", "as": "$c" },
            { "op": "link.add", "pred": "$b", "succ": "$c" }
        ] }),
    )
    .await;
    assert_eq!(batch["history_len"], json!(1));
    assert_eq!(batch["labels"].as_object().unwrap().len(), 2);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn mcp_error_is_structured_not_rpc() {
    let (client, _server) = connect(opts()).await;
    // Aucun projet : erreur métier, pas JSON-RPC (C-MCP-05).
    let e = refused(&client, "add_task", json!({ "name": "A" })).await;
    assert_eq!(e["code"], json!("session.none"));
    assert!(e["message"].as_str().is_some_and(|m| m.contains("new_project")));
    new_project(&client).await;
    let e = refused(&client, "delete_task", json!({ "uid": 999 })).await;
    assert_eq!(e["code"], json!("task.not_found"));
    assert_eq!(e["op_index"], json!(0));
    let e = refused(&client, "add_task", json!({ "nom": "A" })).await;
    assert_eq!(e["code"], json!("param.invalid"));
    // Cycle : refus R-160 avec diagnostics.
    let out = ok(
        &client,
        "apply_batch",
        json!({ "ops": [
            { "op": "task.add", "name": "A", "as": "$a" },
            { "op": "task.add", "name": "B", "as": "$b" },
            { "op": "link.add", "pred": "$a", "succ": "$b" }
        ] }),
    )
    .await;
    let a = out["labels"]["$a"].clone();
    let b = out["labels"]["$b"].clone();
    let e = refused(&client, "add_link", json!({ "pred": b, "succ": a })).await;
    assert_eq!(e["code"], json!("batch.would_create_errors"));
    assert_ne!(e["diagnostics"].as_array().map(Vec::len), Some(0));
    // Refus métier d'une commande J6 (ligne de base déjà posée) : erreur
    // structurée, jamais une erreur JSON-RPC.
    ok(&client, "set_baseline", json!({})).await;
    let e = refused(&client, "set_baseline", json!({})).await;
    assert_eq!(e["code"], json!("baseline.exists"));
    assert_eq!(e["field"], json!("overwrite"));
    // Seul un nom inconnu est une erreur JSON-RPC.
    let rpc = client.call_tool(CallToolRequestParams::new("no_such_tool")).await;
    assert!(rpc.is_err());
    // La session reste utilisable.
    assert_eq!(ok(&client, "project_summary", json!({})).await["tasks"], json!(2));
    client.cancel().await.unwrap();
}

/// Lot L8 J5 : ressources créées par un lot, affectées, puis
/// `resource_usage` en contenu structuré (travail et surutilisation par
/// période, calculés par le moteur) ; `list_resources`, filtre `resource`
/// de `list_tasks`, `delete_resource` sans `cascade` refusé.
#[tokio::test]
async fn mcp_resource_usage_structured() {
    let (client, _server) = connect(opts()).await;
    new_project(&client).await;
    let out = ok(
        &client,
        "apply_batch",
        json!({ "ops": [
            { "op": "resource.add", "name": "Mason", "as": "$m" },
            { "op": "task.add", "name": "A", "as": "$a", "duration": { "text": "5d" } },
            { "op": "task.add", "name": "B", "as": "$b", "duration": { "text": "5d" } },
            { "op": "assignment.add", "task": "$a", "resource": "$m" }
        ] }),
    )
    .await;
    let m = out["resource_labels"]["$m"].clone();
    let b = out["labels"]["$b"].clone();
    assert_eq!(out["created_resources"], json!([m.clone()]));
    let added = ok(
        &client,
        "assign_resource",
        json!({ "task": b, "resource": m, "units": { "text": "150%" } }),
    )
    .await;
    assert_eq!(added["diff"]["overallocated"]["new"], json!([m.clone()]));
    let r = call(
        &client,
        "resource_usage",
        json!({ "by": "week", "resources": [m.clone()] }),
    )
    .await;
    assert_eq!(r.is_error, Some(false));
    let usage = r.structured_content.unwrap();
    assert_eq!(usage["by"], json!("week"));
    assert_eq!(usage["periods"][0], json!("2026-11-02T00:00"));
    let row = &usage["rows"][0];
    assert_eq!(row["uid"], m);
    assert_eq!(row["name"], json!("Mason"));
    // 40 h + 60 h la semaine du 02/11, surutilisée.
    assert_eq!(row["work"][0]["minutes"], json!(6_000));
    assert_eq!(row["overallocated"][0], json!(true));
    let list = ok(&client, "list_resources", json!({ "overallocated": true })).await;
    assert_eq!(list["resources"][0]["uid"], m);
    let tasks = ok(&client, "list_tasks", json!({ "resource": m.clone() })).await;
    assert_eq!(tasks["total"], json!(2));
    let e = refused(&client, "delete_resource", json!({ "uid": m.clone() })).await;
    assert_eq!(e["code"], json!("resource.has_assignments"));
    let deleted = ok(&client, "delete_resource", json!({ "uid": m, "cascade": true })).await;
    assert_eq!(deleted["committed"], json!(true));
    assert_ne!(deleted["diff"]["overallocated"]["resolved"], json!([]));
    client.cancel().await.unwrap();
}

/// J6 (lot L7) : `explain_date` listé, réponse structurée (étapes,
/// chaîne, résumés), erreurs structurées.
#[tokio::test]
async fn mcp_explain_structured() {
    let (client, _server) = connect(opts()).await;
    new_project(&client).await;
    let out = ok(
        &client,
        "apply_batch",
        json!({ "ops": [
            { "op": "task.add", "name": "A", "as": "$a", "duration": { "text": "1d" } },
            { "op": "task.add", "name": "B", "as": "$b", "duration": { "text": "2d" } },
            { "op": "task.add", "name": "C", "as": "$c", "duration": { "text": "1d" } },
            { "op": "link.add", "pred": "$a", "succ": "$c" },
            { "op": "link.add", "pred": "$b", "succ": "$c" }
        ] }),
    )
    .await;
    let (b, c) = (out["labels"]["$b"].clone(), out["labels"]["$c"].clone());
    let tools = client.list_all_tools().await.unwrap();
    assert!(tools.iter().any(|t| t.name == "explain_date"));
    let v = ok(
        &client,
        "explain_date",
        json!({ "task": c.clone(), "side": "finish" }),
    )
    .await;
    assert_eq!(v["date"], json!("2026-11-04T17:00"));
    assert_eq!(v["side"], json!("finish"));
    assert_eq!(
        v["steps"][0],
        json!({ "step": "driver", "driver": { "kind": "link", "predecessor": b.clone(), "link": "fs" },
                "raw": "2026-11-03T17:00" })
    );
    assert_eq!(v["chain"][0]["uid"], b);
    assert_eq!(v["late"]["critical"], json!(true));
    assert!(
        v["summary_en"]
            .as_str()
            .unwrap()
            .starts_with("C finishes on Wed 11/04/2026 17:00")
    );
    assert!(
        v["summary_fr"]
            .as_str()
            .unwrap()
            .starts_with("C finit le mer 04/11/2026 17:00")
    );
    let e = refused(&client, "explain_date", json!({ "task": c, "depth": 0 })).await;
    assert_eq!(e["code"], json!("param.invalid"));
    let e = refused(&client, "explain_date", json!({ "task": 77 })).await;
    assert_eq!(e["code"], json!("task.not_found"));
    let info = client.peer_info().unwrap();
    assert!(
        info.instructions
            .as_deref()
            .is_some_and(|i| i.contains("explain_date"))
    );
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn mcp_undo_redo() {
    let (client, _server) = connect(opts()).await;
    new_project(&client).await;
    ok(
        &client,
        "add_task",
        json!({ "name": "A", "duration": { "text": "1d" } }),
    )
    .await;
    ok(
        &client,
        "add_task",
        json!({ "name": "B", "duration": { "text": "1d" } }),
    )
    .await;
    let tasks = |v: Value| v["tasks"].clone();
    assert_eq!(tasks(ok(&client, "project_summary", json!({})).await), json!(2));
    let u = ok(&client, "undo", json!({})).await;
    assert_eq!(u["history_len"], json!(1));
    assert_eq!(tasks(ok(&client, "project_summary", json!({})).await), json!(1));
    ok(&client, "undo", json!({})).await;
    assert_eq!(tasks(ok(&client, "project_summary", json!({})).await), json!(0));
    let e = refused(&client, "undo", json!({})).await;
    assert_eq!(e["code"], json!("history.empty"));
    ok(&client, "redo", json!({})).await;
    ok(&client, "redo", json!({})).await;
    let s = ok(&client, "project_summary", json!({})).await;
    assert_eq!(tasks(s.clone()), json!(2));
    assert_eq!(s["finish"], json!("2026-11-02T17:00"));
    let e = refused(&client, "redo", json!({})).await;
    assert_eq!(e["code"], json!("history.empty"));
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn mcp_list_tasks_paginated() {
    let (client, _server) = connect(opts()).await;
    new_project(&client).await;
    let ops: Vec<Value> = (1..=60)
        .map(|i| json!({ "op": "task.add", "name": format!("T{i}"), "duration": { "text": "1d" } }))
        .collect();
    ok(&client, "apply_batch", json!({ "ops": ops })).await;
    let p1 = ok(&client, "list_tasks", json!({})).await;
    assert_eq!(p1["tasks"].as_array().unwrap().len(), 50);
    assert_eq!(p1["total"], json!(60));
    let cursor = p1["next_cursor"].clone();
    assert!(cursor.is_u64());
    let p2 = ok(&client, "list_tasks", json!({ "cursor": cursor })).await;
    let page2 = p2["tasks"].as_array().unwrap();
    assert_eq!(page2.len(), 10);
    assert_eq!(page2[0]["name"], json!("T51"));
    assert_eq!(p2["next_cursor"], Value::Null);
    let e = refused(&client, "list_tasks", json!({ "limit": 501 })).await;
    assert_eq!(e["code"], json!("limit.exceeded"));
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn mcp_render_gantt_bounded() {
    let (client, _server) = connect(opts()).await;
    new_project(&client).await;
    let ops: Vec<Value> = (1..=10)
        .map(|i| json!({ "op": "task.add", "name": format!("T{i}"), "duration": { "text": "1d" } }))
        .collect();
    ok(&client, "apply_batch", json!({ "ops": ops })).await;
    let r = call(&client, "render_gantt", json!({ "max_rows": 4 })).await;
    assert_eq!(r.is_error, Some(false));
    let report = r.structured_content.clone().unwrap();
    assert_eq!(report["shown"], json!(4));
    assert_eq!(report["hidden"], json!(6));
    assert!(report.get("svg").is_none());
    let summary = &r.content[0].as_text().unwrap().text;
    assert_eq!(
        summary,
        "10 tasks, 10 critical, finish 11/02/2026 17:00 (6 rows not shown)"
    );
    let svg = &r.content[1].as_text().unwrap().text;
    assert!(
        svg.starts_with("<svg") || svg.starts_with("<?xml"),
        "{}",
        svg.chars().take(40).collect::<String>()
    );
    assert_eq!(report["bytes"].as_u64(), Some(svg.len() as u64));
    // Troisième bloc : le rapport structuré en JSON (spécification MCP).
    assert_eq!(r.content.len(), 3);
    let text: Value = serde_json::from_str(&r.content[2].as_text().unwrap().text).unwrap();
    assert_eq!(text, report);
    let e = refused(&client, "render_gantt", json!({ "max_rows": 201 })).await;
    assert_eq!(e["code"], json!("limit.exceeded"));
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn mcp_journal_written_before_apply() {
    let dir = TempDir::new("journal");
    let path = dir.0.join("run.jsonl");
    let mut o = opts();
    o.journal = Some(path.clone());
    let (client, _server) = connect(o).await;
    new_project(&client).await;
    ok(&client, "add_task", json!({ "name": "A" })).await;
    refused(&client, "delete_task", json!({ "uid": 999 })).await;
    client.cancel().await.unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let entries: Vec<_> = text.lines().map(|l| parse_line(l).unwrap()).collect();
    assert_eq!(entries.len(), 6);
    for (i, pair) in entries.chunks(2).enumerate() {
        let (req, res) = (&pair[0], &pair[1]);
        let seq = u64::try_from(i).unwrap() + 1;
        // La requête précède le résultat, même `seq`, sans résultat.
        assert_eq!((req.seq, res.seq), (seq, seq));
        assert_eq!(req.result, None);
        assert_ne!(req.params, Value::Null);
        assert_eq!(res.params, Value::Null);
        assert_eq!(req.tool, res.tool);
    }
    assert_eq!(entries[0].tool, "new_project");
    assert_eq!(entries[2].params, json!({ "name": "A" }));
    let added = entries[3].result.as_ref().unwrap();
    assert_eq!(added["ok"]["committed"], json!(true));
    let refused = entries[5].result.as_ref().unwrap();
    assert_eq!(refused["error"]["code"], json!("task.not_found"));
}

/// Un journal qu'on ne peut pas écrire n'empêche pas le travail.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn mcp_journal_write_failure_does_not_block() {
    let mut o = opts();
    o.journal = Some(PathBuf::from("/dev/full"));
    let (client, _server) = connect(o).await;
    new_project(&client).await;
    let out = ok(&client, "add_task", json!({ "name": "A" })).await;
    assert_eq!(out["committed"], json!(true));
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn mcp_journal_unopenable_fails_at_start() {
    let dir = TempDir::new("nojournal");
    let mut o = opts();
    o.journal = Some(dir.0.join("absent").join("run.jsonl"));
    let (_c, s) = tokio::io::duplex(64);
    let (r, w) = tokio::io::split(s);
    let res = serve_on(r, w, o).await;
    assert!(matches!(res, Err(McpServeError::Io(_))), "{res:?}");
}

/// C-MCP-06 : une ligne de plus de 8 Mio ferme la session proprement.
#[tokio::test]
async fn mcp_line_too_long_closes() {
    let (client_io, server_io) = tokio::io::duplex(1 << 16);
    let (r, w) = tokio::io::split(server_io);
    let server = tokio::spawn(serve_on(r, w, opts()));
    let init = json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "protocolVersion": "2025-06-18", "capabilities": {},
                    "clientInfo": { "name": "test", "version": "0" } }
    });
    let (mut read_half, mut write_half) = tokio::io::split(client_io);
    // Les réponses du serveur sont lues pour ne pas remplir le tube.
    let drain = async {
        let mut sink = Vec::new();
        let _ = tokio::io::AsyncReadExt::read_to_end(&mut read_half, &mut sink).await;
    };
    let feed = async {
        write_half
            .write_all(format!("{init}\n").as_bytes())
            .await
            .unwrap();
        write_half
            .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
            .await
            .unwrap();
        // 8 Mio + 1 octets sans fin de ligne.
        let chunk = vec![b'x'; 1 << 16];
        let total = 8 * 1024 * 1024 + 1;
        let mut sent = 0usize;
        while sent < total {
            let n = chunk.len().min(total - sent);
            if write_half.write_all(&chunk[..n]).await.is_err() {
                break;
            }
            sent += n;
        }
    };
    let finished = async { server.await.unwrap() };
    let ((), (), res) = tokio::join!(
        async {
            tokio::select! {
                () = drain => {}
                () = tokio::time::sleep(std::time::Duration::from_secs(30)) => {}
            }
        },
        feed,
        finished
    );
    assert_eq!(res, Err(McpServeError::LineTooLong));
}

/// Entrée fermée avant l'initialisation : fin normale.
#[tokio::test]
async fn mcp_eof_before_initialize_is_clean() {
    let (client_io, server_io) = tokio::io::duplex(64);
    let (r, w) = tokio::io::split(server_io);
    drop(client_io);
    assert_eq!(serve_on(r, w, opts()).await, Ok(()));
}

/// J6 (lot L8) : les outils de suivi listés (44 outils en J6, 49 depuis le
/// lot L5 de J7), `update_progress`,
/// `set_baseline`, `baseline_variance` et `what_if` en contenu structuré ;
/// `what_if` ne change pas la session et rend un refus comme donnée ;
/// consignes du serveur.
#[tokio::test]
#[allow(
    clippy::many_single_char_names,
    reason = "tests : noms des tâches de l'exemple (A, B)"
)]
async fn mcp_tracking_tools_structured() {
    let (client, _server) = connect(opts()).await;
    new_project(&client).await;
    let out = ok(
        &client,
        "apply_batch",
        json!({ "ops": [
            { "op": "task.add", "name": "A", "as": "$a", "duration": { "text": "5d" } },
            { "op": "task.add", "name": "B", "as": "$b", "duration": { "text": "1d" } },
            { "op": "link.add", "pred": "$a", "succ": "$b" }
        ] }),
    )
    .await;
    let (a, b) = (out["labels"]["$a"].clone(), out["labels"]["$b"].clone());
    let tools = client.list_all_tools().await.unwrap();
    // 49 en J7, plus les six `style.*` du lot L6 de J8, `export_pdf`
    // (lot L7), `rename_custom_field` (seconde passe UX, lot L8) et
    // `shift_tasks` (finitions P1).
    assert_eq!(tools.len(), 58);
    for name in [
        "update_progress",
        "reschedule_remaining",
        "set_baseline",
        "baseline_variance",
        "what_if",
    ] {
        assert!(tools.iter().any(|t| t.name == name), "{name}");
    }
    let set = ok(&client, "set_baseline", json!({})).await;
    assert_eq!(set["side_effects"][0]["baseline_set"]["number"], json!(0));
    let v = ok(&client, "baseline_variance", json!({})).await;
    assert_eq!(v["total"], json!(2));
    assert_eq!(v["tasks"][0]["variance"]["finish"]["minutes"], json!(0));
    let before = ok(&client, "project_summary", json!({})).await;
    let w = ok(
        &client,
        "what_if",
        json!({
            "ops": [{ "op": "task.update", "uid": a.clone(), "duration": { "text": "7d" } }],
            "baseline": 0,
            "explain": [b.clone()]
        }),
    )
    .await;
    assert_eq!(w["accepted"], json!(true));
    assert_eq!(w["project_finish_variance"]["minutes"], json!(960));
    assert_eq!(w["explanations"][0]["task"]["uid"], b);
    assert_eq!(ok(&client, "project_summary", json!({})).await, before);
    let w = ok(
        &client,
        "what_if",
        json!({ "ops": [{ "op": "link.add", "pred": b.clone(), "succ": a.clone() }] }),
    )
    .await;
    assert_eq!(w["accepted"], json!(false));
    assert_eq!(w["errors"][0]["code"], json!("batch.would_create_errors"));
    let p = ok(
        &client,
        "update_progress",
        json!({ "task": a.clone(), "percent_complete": { "text": "40%" } }),
    )
    .await;
    assert_eq!(p["committed"], json!(true));
    let info = client.peer_info().unwrap();
    assert!(
        info.instructions
            .as_deref()
            .is_some_and(|i| i.contains("what_if") && i.contains("baseline_variance"))
    );
    client.cancel().await.unwrap();
}
