//! Serveur MCP HTTP (feature `http`, lot L17) : client HTTP/1.1 minimal
//! (hyper) sur `127.0.0.1:0`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::missing_panics_doc,
    clippy::arithmetic_side_effects,
    clippy::string_slice,
    reason = "tests : un échec doit s'arrêter net"
)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{HeaderMap, Request, StatusCode};
use hyper_util::rt::TokioIo;
use plannercraft_automation::text::Language;
use plannercraft_core::Moment;
use plannercraft_mcp::{HttpOptions, HttpServer, McpServeError, ServeOptions};
use serde_json::{Value, json};

const PROTOCOL: &str = "2025-06-18";

fn now() -> Moment {
    "2026-10-19T08:00".parse().unwrap()
}

fn opts() -> ServeOptions {
    let mut o = ServeOptions::new(now(), Language::En);
    o.ignore_app_guard = true;
    o
}

fn house_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../plannercraft-ui-egui/assets/demo/house.xml")
}

/// Dossier temporaire propre à un test (retiré à la fin du test).
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("plannercraft-http-{tag}-{}-{n}", std::process::id()));
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

fn http_opts(root: &Path) -> HttpOptions {
    HttpOptions {
        addr: "127.0.0.1:0".to_owned(),
        root: Some(root.to_path_buf()),
        allow_remote: false,
        token_file: None,
    }
}

/// Lie le serveur et le lance en tâche de fond ; rend son adresse.
async fn start(http: &HttpOptions) -> SocketAddr {
    let server = HttpServer::bind(&opts(), http).await.unwrap();
    let addr = server.local_addr().unwrap();
    tokio::spawn(server.run());
    addr
}

/// Réponse : statut, en-têtes, message JSON-RPC (premier `data:` JSON du
/// flux SSE, ou corps JSON).
struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    message: Option<Value>,
}

/// Un POST sur `/mcp` (connexion neuve), en-têtes supplémentaires donnés.
async fn post(addr: SocketAddr, extra: &[(&str, &str)], body: &Value) -> Reply {
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (mut sender, conn) = hyper::client::conn::http1::handshake::<_, Full<Bytes>>(TokioIo::new(stream))
        .await
        .unwrap();
    tokio::spawn(conn);
    let mut req = Request::post("/mcp");
    if !extra.iter().any(|(k, _)| k.eq_ignore_ascii_case("host")) {
        req = req.header("host", addr.to_string());
    }
    req = req
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream");
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

/// Premier `data:` du flux SSE qui est un objet JSON-RPC.
fn first_message(buf: &str) -> Option<Value> {
    buf.lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .filter_map(|d| serde_json::from_str::<Value>(d.trim()).ok())
        .find(|v| v.get("jsonrpc").is_some())
}

fn initialize_body() -> Value {
    json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {
            "protocolVersion": PROTOCOL,
            "capabilities": {},
            "clientInfo": { "name": "plannercraft-http-test", "version": "0" }
        }
    })
}

/// Session MCP HTTP initialisée (en-têtes supplémentaires : jeton).
struct Session {
    addr: SocketAddr,
    id: String,
    extra: Vec<(String, String)>,
    next: u64,
}

impl Session {
    async fn open(addr: SocketAddr, extra: &[(&str, &str)]) -> Self {
        let mut headers: Vec<(&str, &str)> = extra.to_vec();
        headers.push(("mcp-protocol-version", PROTOCOL));
        let r = post(addr, &headers, &initialize_body()).await;
        assert_eq!(r.status, StatusCode::OK);
        let id = r.headers["mcp-session-id"].to_str().unwrap().to_owned();
        assert!(r.message.unwrap()["result"]["serverInfo"].is_object());
        let mut s = Self {
            addr,
            id,
            extra: extra
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            next: 2,
        };
        let r = s
            .send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
            .await;
        assert_eq!(r.status, StatusCode::ACCEPTED);
        s.next = 2;
        s
    }

    async fn send(&self, body: &Value) -> Reply {
        let mut headers: Vec<(&str, &str)> =
            self.extra.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        headers.push(("mcp-session-id", &self.id));
        headers.push(("mcp-protocol-version", PROTOCOL));
        post(self.addr, &headers, body).await
    }

    /// `tools/call` : rend `result` (`CallToolResult`).
    async fn call(&mut self, name: &str, args: Value) -> Value {
        let id = self.next;
        self.next += 1;
        let body = json!({
            "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": name, "arguments": args }
        });
        let r = self.send(&body).await;
        assert_eq!(r.status, StatusCode::OK);
        let m = r.message.unwrap();
        assert_eq!(m["id"], json!(id));
        m["result"].clone()
    }

    async fn ok(&mut self, name: &str, args: Value) -> Value {
        let r = self.call(name, args).await;
        assert_ne!(r["isError"], json!(true), "{name} : {r}");
        r["structuredContent"].clone()
    }

    async fn refused(&mut self, name: &str, args: Value) -> Value {
        let r = self.call(name, args).await;
        assert_eq!(r["isError"], json!(true), "{name} : {r}");
        r["structuredContent"].clone()
    }
}

#[tokio::test]
async fn http_loopback_roundtrip() {
    let root = TempDir::new("rt");
    let addr = start(&http_opts(&root.0)).await;
    assert!(addr.ip().is_loopback());

    let mut a = Session::open(addr, &[]).await;
    a.ok(
        "new_project",
        json!({ "name": "Maison", "start": "2026-11-02T08:00" }),
    )
    .await;
    a.ok(
        "add_task",
        json!({ "name": "Fondations", "duration": { "text": "5d" } }),
    )
    .await;
    let summary = a.ok("project_summary", json!({})).await;
    assert_eq!(summary["tasks"], json!(1));
    assert_eq!(summary["finish"], json!("2026-11-06T17:00"));

    // Chemin relatif : lu depuis la racine.
    a.ok("save_project", json!({ "path": "maison.xml" })).await;
    assert!(root.0.join("maison.xml").is_file());

    // Seconde session HTTP : même espace de travail (état partagé).
    let mut b = Session::open(addr, &[]).await;
    let summary = b.ok("project_summary", json!({})).await;
    assert_eq!(summary["tasks"], json!(1));
    assert_eq!(summary["history_len"], json!(1));

    // Chemin autre que /mcp : 404.
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (mut sender, conn) = hyper::client::conn::http1::handshake::<_, Full<Bytes>>(TokioIo::new(stream))
        .await
        .unwrap();
    tokio::spawn(conn);
    let req = Request::post("/autre")
        .header("host", addr.to_string())
        .body(Full::new(Bytes::new()))
        .unwrap();
    assert_eq!(
        sender.send_request(req).await.unwrap().status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn http_foreign_origin_refused() {
    let root = TempDir::new("origin");
    let addr = start(&http_opts(&root.0)).await;
    let headers = [
        ("mcp-protocol-version", PROTOCOL),
        ("origin", "http://evil.example"),
    ];
    let r = post(addr, &headers, &initialize_body()).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let headers = [
        ("mcp-protocol-version", PROTOCOL),
        ("origin", "http://evil.example:8080"),
    ];
    let r = post(addr, &headers, &initialize_body()).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    // Boucle locale, port quelconque : admise.
    let headers = [
        ("mcp-protocol-version", PROTOCOL),
        ("origin", "http://127.0.0.1:9"),
    ];
    assert_eq!(
        post(addr, &headers, &initialize_body()).await.status,
        StatusCode::OK
    );
    // Mauvais schéma (https) refusé aussi ; boucle locale admise.
    let headers = [
        ("mcp-protocol-version", PROTOCOL),
        ("origin", "https://localhost:3000"),
    ];
    assert_eq!(
        post(addr, &headers, &initialize_body()).await.status,
        StatusCode::FORBIDDEN
    );
    let headers = [
        ("mcp-protocol-version", PROTOCOL),
        ("origin", "http://localhost:3000"),
    ];
    assert_eq!(
        post(addr, &headers, &initialize_body()).await.status,
        StatusCode::OK
    );
    // Hôte étranger (rebinding DNS) refusé.
    let headers = [("mcp-protocol-version", PROTOCOL), ("host", "evil.example")];
    let r = post(addr, &headers, &initialize_body()).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn http_outside_root_refused() {
    let root = TempDir::new("root");
    let addr = start(&http_opts(&root.0)).await;
    let mut s = Session::open(addr, &[]).await;
    let e = s
        .refused("open_project", json!({ "path": house_path().to_string_lossy() }))
        .await;
    assert_eq!(e["code"], json!("file.outside_root"));
    s.ok(
        "new_project",
        json!({ "name": "Maison", "start": "2026-11-02T08:00" }),
    )
    .await;
    let e = s
        .refused("save_project", json!({ "path": "../dehors.xml" }))
        .await;
    assert_eq!(e["code"], json!("file.outside_root"));
    assert!(!root.0.parent().unwrap().join("dehors.xml").exists());

    // Dans la racine : admis.
    std::fs::copy(house_path(), root.0.join("house.xml")).unwrap();
    s.ok("open_project", json!({ "path": "house.xml" })).await;

    // `--open` hors de la racine : refusé au démarrage.
    let mut o = opts();
    o.open = Some(house_path());
    let err = HttpServer::bind(&o, &http_opts(&root.0)).await.unwrap_err();
    assert!(
        matches!(&err, McpServeError::Io(m) if m.contains("file.outside_root")),
        "{err:?}"
    );
}

#[tokio::test]
async fn http_remote_without_token_refused() {
    let root = TempDir::new("remote");
    let usage = |r: Result<HttpServer, McpServeError>| matches!(r, Err(McpServeError::Usage(_)));
    let remote = |allow: bool, token: Option<PathBuf>| HttpOptions {
        addr: "192.0.2.1:8080".to_owned(),
        root: Some(root.0.clone()),
        allow_remote: allow,
        token_file: token,
    };
    // Adresse non locale : --allow-remote puis jeton exigés (avant liaison).
    assert!(usage(HttpServer::bind(&opts(), &remote(false, None)).await));
    assert!(usage(HttpServer::bind(&opts(), &remote(true, None)).await));
    // Adresse non spécifiée refusée.
    let mut any = remote(true, Some(root.0.join("t")));
    any.addr = "0.0.0.0:8080".to_owned();
    assert!(usage(HttpServer::bind(&opts(), &any).await));
    // --root absent : usage.
    let mut no_root = http_opts(&root.0);
    no_root.root = None;
    assert!(usage(HttpServer::bind(&opts(), &no_root).await));
    // Jeton trop court : usage.
    std::fs::write(root.0.join("short"), "abc\n").unwrap();
    let mut short = http_opts(&root.0);
    short.token_file = Some(root.0.join("short"));
    assert!(usage(HttpServer::bind(&opts(), &short).await));
}

#[tokio::test]
async fn http_bearer_token_checked() {
    let root = TempDir::new("token");
    let token = "0123456789abcdef0123456789abcdef";
    let file = root.0.join("token");
    std::fs::write(&file, format!("{token}\n")).unwrap();
    let mut http = http_opts(&root.0);
    http.token_file = Some(file);
    let addr = start(&http).await;

    let base = [("mcp-protocol-version", PROTOCOL)];
    let r = post(addr, &base, &initialize_body()).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert_eq!(r.headers["www-authenticate"], "Bearer");
    let wrong = format!("Bearer {}x", &token[..token.len() - 1]);
    let headers = [
        ("mcp-protocol-version", PROTOCOL),
        ("authorization", wrong.as_str()),
    ];
    assert_eq!(
        post(addr, &headers, &initialize_body()).await.status,
        StatusCode::UNAUTHORIZED
    );
    let prefix = format!("Bearer {}", &token[..16]);
    let headers = [
        ("mcp-protocol-version", PROTOCOL),
        ("authorization", prefix.as_str()),
    ];
    assert_eq!(
        post(addr, &headers, &initialize_body()).await.status,
        StatusCode::UNAUTHORIZED
    );

    let good = format!("Bearer {token}");
    let mut s = Session::open(addr, &[("authorization", good.as_str())]).await;
    let summary = s.call("project_summary", json!({})).await;
    assert_eq!(summary["structuredContent"]["code"], json!("session.none"));
}
