//! Rejeu des scénarios `build_30`, `resources_j5` (J5, lot L9) et `tracking_j6` (J6, lot L10) en MCP HTTP (feature `http`, lot L18,
//! vérification 6) : serveur sur `127.0.0.1:0`, `--root` = dossier
//! temporaire du rejeu, client HTTP/1.1 minimal (hyper). Mêmes résultats et
//! même journal que le rejeu en mémoire.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::missing_panics_doc,
    clippy::arithmetic_side_effects,
    reason = "tests : un échec doit s'arrêter net"
)]

#[path = "common/replay.rs"]
mod replay;

use std::net::SocketAddr;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{HeaderMap, Request, StatusCode};
use hyper_util::rt::TokioIo;
use plannercraft_automation::text::Language;
use plannercraft_mcp::{HttpOptions, HttpServer, ServeOptions};
use replay::{Scenario, TempDir, logged, normalized, now_of, repo_root, substitute};
use serde_json::{Value, json};

const PROTOCOL: &str = "2025-06-18";

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    message: Option<Value>,
}

/// Un POST sur `/mcp` (connexion neuve).
async fn post(addr: SocketAddr, extra: &[(&str, &str)], body: &Value) -> Reply {
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (mut sender, conn) = hyper::client::conn::http1::handshake::<_, Full<Bytes>>(TokioIo::new(stream))
        .await
        .unwrap();
    tokio::spawn(conn);
    let mut req = Request::post("/mcp")
        .header("host", addr.to_string())
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", PROTOCOL);
    for (k, v) in extra {
        req = req.header(*k, *v);
    }
    let req = req.body(Full::new(Bytes::from(body.to_string()))).unwrap();
    let resp = sender.send_request(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let mut body = resp.into_body();
    let mut buf = String::new();
    let mut message = None;
    // Lit jusqu'au premier message JSON-RPC (le flux SSE peut rester ouvert).
    let read = async {
        while let Some(frame) = body.frame().await {
            let Ok(frame) = frame else { break };
            if let Ok(data) = frame.into_data() {
                buf.push_str(&String::from_utf8_lossy(&data));
            }
            if let Some(v) = first_message(&buf) {
                message = Some(v);
                break;
            }
        }
    };
    let _ = tokio::time::timeout(Duration::from_secs(10), read).await;
    if message.is_none() {
        message = serde_json::from_str(buf.trim()).ok();
    }
    Reply {
        status,
        headers,
        message,
    }
}

/// Premier `data:` du flux SSE qui est un message JSON-RPC complet (le
/// message est sur une seule ligne `data:`).
fn first_message(buf: &str) -> Option<Value> {
    buf.lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .filter_map(|d| serde_json::from_str::<Value>(d.trim()).ok())
        .find(|v| v.get("jsonrpc").is_some())
}

/// Session MCP HTTP initialisée.
struct Session {
    addr: SocketAddr,
    id: String,
    next: u64,
}

impl Session {
    async fn open(addr: SocketAddr) -> Self {
        let init = json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL,
                "capabilities": {},
                "clientInfo": { "name": "plannercraft-replay", "version": "0" }
            }
        });
        let r = post(addr, &[], &init).await;
        assert_eq!(r.status, StatusCode::OK);
        let id = r.headers["mcp-session-id"].to_str().unwrap().to_owned();
        assert!(r.message.unwrap()["result"]["serverInfo"].is_object());
        let s = Self { addr, id, next: 2 };
        let r = post(
            addr,
            &[("mcp-session-id", &s.id)],
            &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        )
        .await;
        assert_eq!(r.status, StatusCode::ACCEPTED);
        s
    }

    /// `tools/call` ; résultat sous la forme du journal.
    async fn call(&mut self, name: &str, args: &Value) -> Value {
        let id = self.next;
        self.next += 1;
        let mut params = json!({ "name": name });
        if args.is_object() {
            params["arguments"] = args.clone();
        }
        let body = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": params });
        let r = post(self.addr, &[("mcp-session-id", &self.id)], &body).await;
        assert_eq!(r.status, StatusCode::OK, "{name}");
        let m = r.message.unwrap();
        assert_eq!(m["id"], json!(id));
        let result = &m["result"];
        logged(
            result.get("isError").and_then(Value::as_bool),
            result.get("structuredContent").cloned(),
        )
    }
}

#[tokio::test]
async fn replay_build_30_http() {
    replay_http("build_30").await;
}

/// Scénario J5 (lot L9) : ressources, affectations, surutilisation.
#[tokio::test]
async fn replay_resources_j5_http() {
    replay_http("resources_j5").await;
}

/// Scénario de suivi J6 (lot L10) : avancement, lignes de base, analyse.
#[tokio::test]
async fn replay_tracking_j6_http() {
    replay_http("tracking_j6").await;
}

/// Rejoue le scénario `name` en MCP HTTP.
async fn replay_http(name: &str) {
    let root = repo_root(env!("CARGO_MANIFEST_DIR"));
    let scenario = Scenario::load(&root, name);
    let tmp = TempDir::new("http");
    // Journal hors de la racine confinée : il n'est pas un fichier du projet.
    let journal_dir = TempDir::new("http-journal");
    let journal = journal_dir.path().join("run.jsonl");
    let mut opts = ServeOptions::new(now_of(name).parse().unwrap(), Language::Fr);
    opts.ignore_app_guard = true;
    opts.journal = Some(journal.clone());
    let http = HttpOptions {
        addr: "127.0.0.1:0".to_owned(),
        root: Some(tmp.path().to_path_buf()),
        allow_remote: false,
        token_file: None,
    };
    let server = HttpServer::bind(&opts, &http).await.unwrap();
    let addr = server.local_addr().unwrap();
    tokio::spawn(server.run());

    let mut s = Session::open(addr).await;
    let mut got = Vec::with_capacity(scenario.calls.len());
    for c in &scenario.calls {
        let r = s.call(&c.tool, &substitute(&c.params, tmp.path())).await;
        got.push(normalized(&r, tmp.path()));
    }
    replay::assert_results(&scenario, &got);
    let produced = std::fs::read_to_string(&journal).unwrap();
    replay::assert_journal(&scenario, &produced, tmp.path());
}
