//! Garde ADR-0002 du serveur MCP : un fichier tenu par une application
//! vivante n'est ni ouvert ni écrit (`file.open_in_app`) depuis l'espace
//! local (lot L8 de J4). Depuis J7 (lot L2), sans `--headless`, ouvrir un
//! tel fichier rattache le serveur à l'application (`tests/relay.rs`) : la
//! garde est testée ici relais coupé (`--headless`), et levée seulement par
//! `ignore_app_guard`. L'application est simulée par l'écouteur `hello` et
//! le rendez-vous du canal de contrôle (L10), sous Unix.

#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net"
)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use plannercraft_automation::control::{
    ListenerHandle, RendezvousGuard, canonical_project, new_token, publish_in, spawn_hello_listener_in,
};
use plannercraft_automation::text::Language;
use plannercraft_core::Moment;
use plannercraft_mcp::{McpServeError, PlannerCraftServer, ServeOptions, serve_with};
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, CallToolResult};
use serde_json::{Value, json};

fn now() -> Moment {
    "2026-10-19T08:00".parse().unwrap()
}

fn house_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../plannercraft-ui-egui/assets/demo/house.xml")
}

/// Dossier temporaire propre à un test (court : chemin de socket Unix).
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("pm-{tag}-{}-{n}", std::process::id()));
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

/// Application factice vivante qui tient `project`.
fn live_app(dir: &Path, project: &Path) -> (ListenerHandle, RendezvousGuard) {
    let listener = spawn_hello_listener_in(dir, new_token().unwrap(), Arc::new(Mutex::new(None))).unwrap();
    let guard = publish_in(dir, &listener.rendezvous(canonical_project(project))).unwrap();
    (listener, guard)
}

/// Serveur `--headless` (relais coupé, garde active).
fn server(control: Option<PathBuf>) -> PlannerCraftServer {
    PlannerCraftServer::new(&ServeOptions::new(now(), Language::En))
        .unwrap()
        .with_control_dir(control)
        .with_relay(false)
}

fn args(v: &Value) -> Option<serde_json::Map<String, Value>> {
    v.as_object().cloned()
}

fn code(r: &CallToolResult) -> Option<String> {
    (r.is_error == Some(true))
        .then(|| r.structured_content.as_ref()?["code"].as_str().map(str::to_owned))
        .flatten()
}

#[tokio::test]
async fn mcp_guard_open_in_app_refused() {
    let dir = TempDir::new("guard");
    let house = house_path();
    let (_listener, _rv) = live_app(&dir.0, &house);

    // Par le protocole : client rmcp en mémoire.
    let (client_io, server_io) = tokio::io::duplex(1 << 20);
    let (r, w) = tokio::io::split(server_io);
    let task = tokio::spawn(serve_with(server(Some(dir.0.clone())), r, w, 8 << 20));
    let client = ().serve(client_io).await.unwrap();
    let path = house.to_str().unwrap();
    let res = client
        .call_tool(
            CallToolRequestParams::new("open_project")
                .with_arguments(args(&json!({ "path": path })).unwrap()),
        )
        .await
        .unwrap();
    assert_eq!(code(&res).as_deref(), Some("file.open_in_app"));
    let e = res.structured_content.unwrap();
    assert_eq!(e["field"], json!("path"));
    assert!(e["message"].as_str().unwrap().contains("--headless"));
    client.cancel().await.unwrap();
    assert_eq!(task.await.unwrap(), Ok(()));

    // Écrire sur le fichier tenu est refusé aussi.
    let s = server(Some(dir.0.clone()));
    s.dispatch(
        "new_project",
        args(&json!({ "name": "P", "start": "2026-11-02T08:00" })),
    )
    .unwrap();
    // J8 (L7) : un export PDF ne doit pas écraser un plan tenu non plus.
    for tool in ["save_project", "export_mspdi", "export_pdf"] {
        let r = s.dispatch(tool, args(&json!({ "path": path }))).unwrap();
        assert_eq!(code(&r).as_deref(), Some("file.open_in_app"), "{tool}");
    }
    // Un autre fichier passe.
    let other = dir.0.join("autre.xml");
    let r = s
        .dispatch("save_project", args(&json!({ "path": other.to_str().unwrap() })))
        .unwrap();
    assert_eq!(r.is_error, Some(false), "{:?}", r.structured_content);
}

#[test]
fn mcp_guard_ignored_opens() {
    let dir = TempDir::new("headless");
    let house = house_path();
    let (_listener, _rv) = live_app(&dir.0, &house);
    // `ignore_app_guard` (tests, rejeux) : la garde est inactive.
    let mut opts = ServeOptions::new(now(), Language::En);
    opts.ignore_app_guard = true;
    let s = PlannerCraftServer::new(&opts).unwrap();
    let r = s
        .dispatch("open_project", args(&json!({ "path": house.to_str().unwrap() })))
        .unwrap();
    assert_eq!(r.is_error, Some(false), "{:?}", r.structured_content);
}

#[test]
fn mcp_guard_open_initial_refused() {
    let dir = TempDir::new("initial");
    let house = house_path();
    let (_listener, _rv) = live_app(&dir.0, &house);
    match server(Some(dir.0.clone())).open_initial(&house) {
        Err(McpServeError::Io(m)) => assert!(m.contains("file.open_in_app"), "{m}"),
        other => panic!("refus attendu : {other:?}"),
    }
}

#[test]
fn mcp_guard_dead_app_ignored() {
    let dir = TempDir::new("dead");
    let house = house_path();
    let (listener, rv) = live_app(&dir.0, &house);
    // Application morte : écouteur arrêté, rendez-vous resté sur le disque.
    drop(listener);
    std::mem::forget(rv);
    server(Some(dir.0.clone())).open_initial(&house).unwrap();
}

#[test]
fn mcp_guard_relative_path_under_root() {
    // `--root R` et dossier courant ≠ R : `{"path":"a.xml"}` vise R/a.xml,
    // tenu par l'application ; la garde le voit (ouverture, écriture,
    // export, `--open`).
    let dir = TempDir::new("root");
    let root = dir.0.join("plans");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let held = root.join("a.xml");
    std::fs::copy(house_path(), &held).unwrap();
    assert_ne!(std::env::current_dir().unwrap(), root);
    let (_listener, _rv) = live_app(&dir.0, &held);
    let s = server(Some(dir.0.clone())).with_root(root.clone());
    let r = s
        .dispatch("open_project", args(&json!({ "path": "a.xml" })))
        .unwrap();
    assert_eq!(code(&r).as_deref(), Some("file.open_in_app"));
    s.dispatch(
        "new_project",
        args(&json!({ "name": "P", "start": "2026-11-02T08:00" })),
    )
    .unwrap();
    for tool in ["save_project", "export_mspdi"] {
        let r = s.dispatch(tool, args(&json!({ "path": "a.xml" }))).unwrap();
        assert_eq!(code(&r).as_deref(), Some("file.open_in_app"), "{tool}");
    }
    assert_eq!(
        std::fs::read(&held).unwrap(),
        std::fs::read(house_path()).unwrap()
    );
    let s = server(Some(dir.0.clone())).with_root(root);
    match s.open_initial(Path::new("a.xml")) {
        Err(McpServeError::Io(m)) => assert!(m.contains("file.open_in_app"), "{m}"),
        other => panic!("refus attendu : {other:?}"),
    }
}
