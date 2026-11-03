//! Relais côté MCP (J7, lot L2, ADR-0015 D9, C-CTL-06, C-CTL-10) :
//! rattachement à l'application, appels relayés, attente de la validation,
//! délais bornés, détachement. L'application est simulée par un faux
//! écouteur (socket Unix, protocole du canal) dont les réponses sont
//! écrites par chaque test.

#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::missing_panics_doc,
    reason = "tests : un échec doit s'arrêter net"
)]

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use plannercraft_automation::control::{
    Frame, Method, PROTOCOL_VERSION, Rendezvous, RendezvousGuard, Reply, canonical_project, new_token,
    publish_in, to_line,
};
use plannercraft_automation::journal::parse_line;
use plannercraft_automation::text::Language;
use plannercraft_core::Moment;
use plannercraft_mcp::{PlannerCraftServer, ServeOptions, serve_with};
use rmcp::ServiceExt;
use rmcp::model::CallToolResult;
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
        let dir = std::env::temp_dir().join(format!("pr-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    /// Copie de `house.xml` sous le nom `name`.
    fn plan(&self, name: &str) -> PathBuf {
        let p = self.0.join(name);
        std::fs::copy(house_path(), &p).unwrap();
        p
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Étape scriptée de la réponse du faux écouteur à une requête.
enum Step {
    /// Envoie une réponse.
    Send(Reply),
    /// Envoie une ligne brute (sans le `\n`).
    Raw(String),
    /// Attend.
    Sleep(Duration),
    /// Ferme la connexion (application partie).
    Close,
}

type Script = Arc<dyn Fn(&Frame) -> Vec<Step> + Send + Sync>;

/// Fausse application : écouteur du canal (hello avec jeton, puis
/// requêtes servies par `script`), rendez-vous publié.
struct FakeApp {
    stop: Arc<AtomicBool>,
    socket: PathBuf,
    thread: Option<JoinHandle<()>>,
    /// Requêtes reçues après `hello`.
    log: Arc<Mutex<Vec<Frame>>>,
    _rendezvous: RendezvousGuard,
}

impl FakeApp {
    fn start(dir: &Path, name: &str, project: Option<&Path>, script: Script) -> Self {
        let socket = dir.join(format!("{name}.sock"));
        let listener = UnixListener::bind(&socket).unwrap();
        let token = new_token().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let log = Arc::new(Mutex::new(Vec::new()));
        let thread = {
            let (stop, log, token) = (Arc::clone(&stop), Arc::clone(&log), token.clone());
            thread::spawn(move || {
                for conn in listener.incoming() {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let Ok(stream) = conn else { continue };
                    let (log, token, script) = (Arc::clone(&log), token.clone(), Arc::clone(&script));
                    thread::spawn(move || serve(&stream, &token, &log, &script));
                }
            })
        };
        let r = Rendezvous {
            v: PROTOCOL_VERSION,
            pid: std::process::id(),
            endpoint: socket.to_str().unwrap().to_owned(),
            token,
            project: project.and_then(canonical_project),
        };
        let guard = publish_in(dir, &r).unwrap();
        Self {
            stop,
            socket,
            thread: Some(thread),
            log,
            _rendezvous: guard,
        }
    }

    fn frames(&self) -> Vec<Frame> {
        self.log.lock().unwrap().clone()
    }
}

impl Drop for FakeApp {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = UnixStream::connect(&self.socket);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        let _ = std::fs::remove_file(&self.socket);
    }
}

fn serve(stream: &UnixStream, token: &str, log: &Mutex<Vec<Frame>>, script: &Script) {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        let frame: Frame = serde_json::from_str(line.trim_end()).unwrap();
        let steps = if frame.method == Method::Hello {
            if frame.params["token"] == json!(token) {
                vec![Step::Send(Reply::Ok {
                    id: frame.id,
                    result: json!({ "app": "plannercraft" }),
                })]
            } else {
                vec![
                    Step::Send(Reply::error(frame.id, "control.bad_token", "jeton")),
                    Step::Close,
                ]
            }
        } else {
            log.lock().unwrap().push(frame.clone());
            script(&frame)
        };
        for step in steps {
            match step {
                Step::Send(reply) => {
                    if (&*stream).write_all(&to_line(&reply).unwrap()).is_err() {
                        return;
                    }
                }
                Step::Raw(text) => {
                    if (&*stream).write_all(format!("{text}\n").as_bytes()).is_err() {
                        return;
                    }
                }
                Step::Sleep(d) => thread::sleep(d),
                Step::Close => return,
            }
        }
    }
}

/// Outil relayé (`execute`) : `tool` du registre.
fn tool_of(frame: &Frame) -> &str {
    frame.params["tool"].as_str().unwrap_or("")
}

/// Script : `project.summary` répond un résumé factice, toute autre
/// requête `{"relayed": <outil>}`.
fn echo() -> Script {
    Arc::new(|f: &Frame| {
        let result = match (f.method, tool_of(f)) {
            (Method::Execute, "project.summary") => json!({ "name": "Appli", "tasks": 42 }),
            (Method::Execute, tool) => json!({ "relayed": tool }),
            (Method::UiState, _) => json!({ "view": "gantt" }),
            _ => json!(null),
        };
        vec![Step::Send(Reply::Ok { id: f.id, result })]
    })
}

fn server(control: &Path) -> PlannerCraftServer {
    PlannerCraftServer::new(&ServeOptions::new(now(), Language::En))
        .unwrap()
        .with_control_dir(Some(control.to_path_buf()))
}

fn args(v: &Value) -> Option<serde_json::Map<String, Value>> {
    v.as_object().cloned()
}

fn call(s: &PlannerCraftServer, tool: &str, v: &Value) -> CallToolResult {
    s.dispatch(tool, args(v)).unwrap()
}

fn ok(r: &CallToolResult) -> Value {
    assert_eq!(r.is_error, Some(false), "{:?}", r.structured_content);
    r.structured_content.clone().unwrap()
}

fn code(r: &CallToolResult) -> String {
    assert_eq!(r.is_error, Some(true), "{:?}", r.structured_content);
    r.structured_content.as_ref().unwrap()["code"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn relay_attach_single_rendezvous() {
    let dir = TempDir::new("single");
    let plan = dir.plan("a.xml");
    let app = FakeApp::start(&dir.0, "a", Some(&plan), echo());
    let s = server(&dir.0);
    assert!(s.attach_at_startup());
    assert!(s.is_attached());
    let v = ok(&call(&s, "project_summary", &json!({})));
    assert_eq!(v, json!({ "name": "Appli", "tasks": 42 }));
    let frames = app.frames();
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].method, Method::Execute);
    // Délai de validation transmis à l'application (relecture finale J7,
    // relais F1 : un seul côté tranche l'expiration).
    assert_eq!(
        frames[0].params,
        json!({ "tool": "project.summary", "params": {}, "approval_timeout_ms": 120_000 })
    );

    // Deux applications vivantes : aucun rattachement au démarrage.
    let other = FakeApp::start(&dir.0, "b", None, echo());
    let s = server(&dir.0);
    assert!(!s.attach_at_startup());
    assert_eq!(code(&call(&s, "project_summary", &json!({}))), "session.none");

    // Headless : jamais de rattachement.
    let s = server(&dir.0).with_relay(false);
    drop(other);
    assert!(!s.attach_at_startup());
}

#[test]
fn relay_attach_on_open_held_file() {
    let dir = TempDir::new("open");
    let plan = dir.plan("a.xml");
    let app = FakeApp::start(&dir.0, "a", Some(&plan), echo());
    let s = server(&dir.0);
    assert!(!s.is_attached());
    let v = ok(&call(
        &s,
        "open_project",
        &json!({ "path": plan.to_str().unwrap() }),
    ));
    assert_eq!(v["attached"], json!(true));
    assert_eq!(v["summary"]["tasks"], json!(42));
    assert_eq!(v["diagnostics_total"], json!(0));
    assert!(s.is_attached());

    // Écriture relayée telle quelle (`dry_run` compris).
    let params = json!({ "name": "Coulage", "duration": { "text": "2j" }, "dry_run": true });
    let v = ok(&call(&s, "add_task", &params));
    assert_eq!(v, json!({ "relayed": "task.add" }));
    // Réouverture du même fichier : rattachement gardé, résumé relayé.
    let v = ok(&call(
        &s,
        "open_project",
        &json!({ "path": plan.to_str().unwrap() }),
    ));
    assert_eq!(v["attached"], json!(true));
    let frames = app.frames();
    let tools: Vec<&str> = frames.iter().map(tool_of).collect();
    assert_eq!(tools, ["project.summary", "task.add", "project.summary"]);
    assert_eq!(frames[1].params["params"], params);
}

#[test]
fn relay_detach_on_open_other_file() {
    let dir = TempDir::new("detach");
    let held = dir.plan("a.xml");
    let free = dir.plan("b.xml");
    let _app = FakeApp::start(&dir.0, "a", Some(&held), echo());
    let s = server(&dir.0);
    assert!(s.attach_at_startup());
    // Ouverture impossible : le rattachement reste.
    let missing = dir.0.join("absent.xml");
    let r = call(&s, "open_project", &json!({ "path": missing.to_str().unwrap() }));
    assert_eq!(code(&r), "file.read");
    assert!(s.is_attached());
    // Fichier non tenu : détachement, ouverture locale.
    let v = ok(&call(
        &s,
        "open_project",
        &json!({ "path": free.to_str().unwrap() }),
    ));
    assert_eq!(v["attached"], json!(false));
    assert!(v["summary"]["tasks"].as_u64().unwrap() > 0);
    assert!(!s.is_attached());
    // Les appels suivants sont locaux.
    let v = ok(&call(&s, "project_summary", &json!({})));
    assert_ne!(v["name"], json!("Appli"));
    // Ouverture locale hors rattachement : résultat inchangé (pas de
    // champ `attached`).
    let v = ok(&call(
        &s,
        "open_project",
        &json!({ "path": free.to_str().unwrap() }),
    ));
    assert!(v.get("attached").is_none());
}

#[test]
fn relay_headless_keeps_open_in_app_guard() {
    let dir = TempDir::new("headless");
    let plan = dir.plan("a.xml");
    let app = FakeApp::start(&dir.0, "a", Some(&plan), echo());
    let s = server(&dir.0).with_relay(false);
    let r = call(&s, "open_project", &json!({ "path": plan.to_str().unwrap() }));
    assert_eq!(code(&r), "file.open_in_app");
    assert!(!s.is_attached());
    s.dispatch(
        "new_project",
        args(&json!({ "name": "P", "start": "2026-11-02T08:00" })),
    )
    .unwrap();
    let r = call(&s, "save_project", &json!({ "path": plan.to_str().unwrap() }));
    assert_eq!(code(&r), "file.open_in_app");
    assert_eq!(app.frames().len(), 0);
    assert_eq!(
        std::fs::read(&plan).unwrap(),
        std::fs::read(house_path()).unwrap()
    );
}

#[test]
fn relay_waits_pending_then_final() {
    let dir = TempDir::new("pending");
    let plan = dir.plan("a.xml");
    let script: Script = Arc::new(|f: &Frame| {
        if tool_of(f) == "task.add" {
            vec![
                Step::Send(Reply::PendingApproval { id: f.id }),
                Step::Sleep(Duration::from_millis(300)),
                Step::Send(Reply::Ok {
                    id: f.id,
                    result: json!({ "committed": true }),
                }),
            ]
        } else {
            vec![Step::Send(Reply::Ok {
                id: f.id,
                result: json!({ "tasks": 1 }),
            })]
        }
    });
    let _app = FakeApp::start(&dir.0, "a", Some(&plan), script);
    let s = server(&dir.0);
    assert!(s.attach_at_startup());
    let t = Instant::now();
    let v = ok(&call(&s, "add_task", &json!({ "name": "T" })));
    assert!(t.elapsed() >= Duration::from_millis(300));
    assert_eq!(v, json!({ "committed": true }));
    assert!(s.is_attached());
}

#[test]
fn relay_app_error_passed_through() {
    let dir = TempDir::new("apperr");
    let script: Script = Arc::new(|f: &Frame| {
        let line = format!(
            r#"{{"err":{{"id":{},"error":{{"code":"control.rejected","message":"refusé","field":"x"}}}}}}"#,
            f.id
        );
        vec![Step::Raw(line)]
    });
    let _app = FakeApp::start(&dir.0, "a", None, script);
    let s = server(&dir.0);
    assert!(s.attach_at_startup());
    let r = call(&s, "add_task", &json!({ "name": "T" }));
    assert_eq!(code(&r), "control.rejected");
    // L'erreur de l'application est rendue telle quelle, champs au-delà de
    // `code` et `message` compris.
    let e = r.structured_content.unwrap();
    assert_eq!(e["message"], json!("refusé"));
    assert_eq!(e["field"], json!("x"));
    assert!(s.is_attached());
}

#[test]
fn relay_timeout_bounded() {
    let dir = TempDir::new("timeout");
    // Application qui ne répond jamais à une requête (hello compris pour
    // la sonde : seul le hello est servi).
    let mute: Script = Arc::new(|_f: &Frame| Vec::new());
    let _app = FakeApp::start(&dir.0, "a", None, mute);
    let s = server(&dir.0).with_relay_timeouts(Duration::from_millis(200), Duration::from_millis(200));
    assert!(s.attach_at_startup());
    let t = Instant::now();
    let r = call(&s, "add_task", &json!({ "name": "T" }));
    let took = t.elapsed();
    assert!(took >= Duration::from_millis(400), "{took:?}");
    assert!(took < Duration::from_secs(5), "{took:?}");
    // Aucune réponse du tout : application tenue pour partie.
    assert_eq!(code(&r), "control.app_gone");
    assert!(!s.is_attached());

    // Validation demandée puis jamais donnée : délai de validation, le
    // rattachement reste.
    let dir = TempDir::new("approval");
    let pending: Script = Arc::new(|f: &Frame| vec![Step::Send(Reply::PendingApproval { id: f.id })]);
    let _app = FakeApp::start(&dir.0, "a", None, pending);
    let s = server(&dir.0).with_relay_timeouts(Duration::from_millis(200), Duration::from_millis(200));
    assert!(s.attach_at_startup());
    let t = Instant::now();
    let r = call(&s, "add_task", &json!({ "name": "T" }));
    assert!(t.elapsed() < Duration::from_secs(5));
    assert_eq!(code(&r), "control.approval_timeout");
    assert!(s.is_attached());
}

#[test]
fn c_ctl_06_app_gone_detaches() {
    let dir = TempDir::new("gone");
    let plan = dir.plan("a.xml");
    let script: Script = Arc::new(|f: &Frame| {
        if tool_of(f) == "task.add" {
            vec![Step::Send(Reply::PendingApproval { id: f.id }), Step::Close]
        } else {
            vec![Step::Send(Reply::Ok {
                id: f.id,
                result: json!({ "tasks": 1 }),
            })]
        }
    });
    let app = FakeApp::start(&dir.0, "a", Some(&plan), script);
    let s = server(&dir.0);
    let v = ok(&call(
        &s,
        "open_project",
        &json!({ "path": plan.to_str().unwrap() }),
    ));
    assert_eq!(v["attached"], json!(true));
    // Fermeture pendant l'appel.
    let r = call(&s, "add_task", &json!({ "name": "T" }));
    assert_eq!(code(&r), "control.app_gone");
    assert!(!s.is_attached());
    // Retour en local, sans projet.
    assert_eq!(code(&call(&s, "project_summary", &json!({}))), "session.none");

    // Application fermée entre deux appels.
    let s = server(&dir.0);
    assert!(s.attach_at_startup());
    drop(app);
    let r = call(&s, "project_summary", &json!({}));
    assert_eq!(code(&r), "control.app_gone");
    assert!(!s.is_attached());
    assert_eq!(code(&call(&s, "list_tasks", &json!({}))), "session.none");
}

/// Relecture finale J7, relais F2 : l'application a changé de document
/// (nouveau jeton) ⇒ `control.document_changed`, détachement, rien n'est
/// envoyé à l'application ; `open_project` du nouveau fichier rattache.
#[test]
fn relay_document_changed_detaches() {
    let dir = TempDir::new("docchg");
    let plan = dir.plan("a.xml");
    let other = dir.plan("b.xml");
    let app = FakeApp::start(&dir.0, "a", Some(&plan), echo());
    let s = server(&dir.0);
    let v = ok(&call(
        &s,
        "open_project",
        &json!({ "path": plan.to_str().unwrap() }),
    ));
    assert_eq!(v["attached"], json!(true));
    // Même écouteur (même socket), nouveau jeton et nouveau document.
    drop(app);
    let app = FakeApp::start(&dir.0, "a", Some(&other), echo());
    let r = call(&s, "update_task", &json!({ "uid": 5, "name": "Ailleurs" }));
    assert_eq!(code(&r), "control.document_changed");
    assert!(!s.is_attached());
    assert!(app.frames().is_empty(), "rien n'atteint le nouveau document");
    assert_eq!(code(&call(&s, "project_summary", &json!({}))), "session.none");
    let v = ok(&call(
        &s,
        "open_project",
        &json!({ "path": other.to_str().unwrap() }),
    ));
    assert_eq!(v["attached"], json!(true));
}

#[tokio::test]
async fn c_ctl_10_app_tools_not_attached() {
    let dir = TempDir::new("app");
    let names =
        |tools: &[rmcp::model::Tool]| -> Vec<String> { tools.iter().map(|t| t.name.to_string()).collect() };

    // Headless : `app.*` ni listés ni servis.
    let headless = server(&dir.0).with_relay(false);
    let r = headless.dispatch("ui_state", None).unwrap();
    assert_eq!(code(&r), "control.not_attached");
    let (client_io, server_io) = tokio::io::duplex(1 << 20);
    let (rd, wr) = tokio::io::split(server_io);
    let task = tokio::spawn(serve_with(headless, rd, wr, 8 << 20));
    let client = ().serve(client_io).await.unwrap();
    let listed = names(&client.list_all_tools().await.unwrap());
    assert!(
        !listed
            .iter()
            .any(|n| n == "ui_state" || n == "screenshot" || n == "ui_action")
    );
    client.cancel().await.unwrap();
    assert_eq!(task.await.unwrap(), Ok(()));

    // Relais permis mais non rattaché : listés, refusés `not_attached`.
    let s = server(&dir.0);
    let r = s.dispatch("screenshot", None).unwrap();
    assert_eq!(code(&r), "control.not_attached");
    let (client_io, server_io) = tokio::io::duplex(1 << 20);
    let (rd, wr) = tokio::io::split(server_io);
    let task = tokio::spawn(serve_with(s, rd, wr, 8 << 20));
    let client = ().serve(client_io).await.unwrap();
    let listed = names(&client.list_all_tools().await.unwrap());
    for n in ["screenshot", "ui_action", "ui_state"] {
        assert!(listed.iter().any(|l| l == n), "{n}");
    }
    client.cancel().await.unwrap();
    assert_eq!(task.await.unwrap(), Ok(()));

    // Rattaché : relayés par leur méthode.
    let app = FakeApp::start(&dir.0, "a", None, echo());
    let s = server(&dir.0);
    assert!(s.attach_at_startup());
    let v = ok(&s.dispatch("ui_state", None).unwrap());
    assert_eq!(v, json!({ "view": "gantt" }));
    let _ = s
        .dispatch("screenshot", args(&json!({ "scale_down": 1 })))
        .unwrap();
    let _ = s
        .dispatch("ui_action", args(&json!({ "theme": "dark" })))
        .unwrap();
    let frames = app.frames();
    assert_eq!(frames[0].method, Method::UiState);
    assert_eq!(frames[1].method, Method::Screenshot);
    assert_eq!(frames[1].params, json!({ "scale_down": 1 }));
    assert_eq!(frames[2].method, Method::Execute);
    assert_eq!(tool_of(&frames[2]), "app.ui");
}

/// J7 L14 (C-CTL-08) : la capture relayée devient un contenu image MCP ;
/// le contenu structuré garde la taille, sans les données de l'image.
#[test]
fn relay_screenshot_is_image_content() {
    let dir = TempDir::new("screenshot");
    let script: Script = Arc::new(|f: &Frame| {
        let result = match f.method {
            Method::Screenshot => json!({
                "mime_type": "image/png", "data": "iVBORw0KGgo=", "width": 2, "height": 1,
                "scale_down": 0, "bytes": 8
            }),
            _ => json!(null),
        };
        vec![Step::Send(Reply::Ok { id: f.id, result })]
    });
    let _app = FakeApp::start(&dir.0, "a", None, script);
    let s = server(&dir.0);
    assert!(s.attach_at_startup());
    let r = s.dispatch("screenshot", None).unwrap();
    assert_ne!(r.is_error, Some(true));
    let image = r
        .content
        .iter()
        .find_map(|c| c.as_image())
        .expect("contenu image");
    assert_eq!(image.data, "iVBORw0KGgo=");
    assert_eq!(image.mime_type, "image/png");
    let structured = r.structured_content.unwrap();
    assert_eq!(structured["width"], 2);
    assert!(structured.get("data").is_none());
}

#[test]
fn relay_journal_records_relayed_calls() {
    let dir = TempDir::new("journal");
    let journal = dir.0.join("run.jsonl");
    let _app = FakeApp::start(&dir.0, "a", None, echo());
    let mut opts = ServeOptions::new(now(), Language::En);
    opts.journal = Some(journal.clone());
    let s = PlannerCraftServer::new(&opts)
        .unwrap()
        .with_control_dir(Some(dir.0.clone()));
    assert!(s.attach_at_startup());
    ok(&call(&s, "add_task", &json!({ "name": "T" })));
    let text = std::fs::read_to_string(&journal).unwrap();
    let entries: Vec<_> = text.lines().map(|l| parse_line(l).unwrap()).collect();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].tool, "add_task");
    assert_eq!(entries[0].params, json!({ "name": "T" }));
    assert_eq!(
        entries[1].result,
        Some(json!({ "ok": { "relayed": "task.add" } }))
    );
}
