//! Canal de contrôle (lot L10 de J4, ADR-0002, ADR-0012 D9) : trames,
//! bornes de ligne, rendez-vous 0600, nettoyage des rendez-vous périmés,
//! jeton, correspondance par chemin canonique, écouteur J4 ; écouteur du
//! relais J7 (lot L3, ADR-0015 D9) : jeton (C-CTL-01), file pleine
//! (`control.busy`), expiration et `Cancel` (C-CTL-04), lectures seules
//! par `query`. Le déroulé complet avec l'application est testé dans
//! `plannercraft-ui-egui/tests/ui/relay.rs`.
//!
//! Transport TCP en boucle locale (J8, lot L8, C-CTL-11 à C-CTL-14) : les
//! tests de l'écouteur et du client sont rejoués sur les deux transports
//! (suffixe `_tcp`, compilé et exécuté sur tous les systèmes, Windows
//! compris) ; les tests propres aux fichiers de socket restent Unix.

#![cfg(not(target_arch = "wasm32"))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

use std::io::{BufReader, Cursor};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use plannercraft_automation::control::{
    Frame, LineError, MAX_LINE_BYTES, Method, Reply, Transport, codes, read_line_bounded, to_line,
};
use serde_json::{Value, json};

/// Dossier temporaire propre au test, supprimé à la fin.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        // Chemin court : une adresse de socket Unix tient en ~100 octets.
        let dir = std::env::temp_dir().join(format!("pc-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn control_frame_roundtrip() {
    for (i, method) in Method::ALL.into_iter().enumerate() {
        let frame = Frame {
            v: 1,
            id: u64::try_from(i).unwrap() + 40,
            method,
            params: json!({ "token": "x", "n": [1, 2] }),
        };
        let line = to_line(&frame).unwrap();
        assert_eq!(line.last(), Some(&b'\n'));
        let text = std::str::from_utf8(&line).unwrap();
        assert!(
            text.contains(&format!("\"method\":\"{}\"", method.wire_name())),
            "{text}"
        );
        let mut reader = Cursor::new(line.clone());
        let read = read_line_bounded(&mut reader, MAX_LINE_BYTES).unwrap().unwrap();
        let back: Frame = serde_json::from_slice(&read).unwrap();
        assert_eq!(back, frame);
    }
    assert_eq!(
        Method::ALL.map(Method::wire_name),
        ["hello", "query", "execute", "ui_state", "screenshot"]
    );
    // J7 : `undo`, `redo` et `history` retirées (`execute` de
    // `history.undo`…).
    for retired in ["undo", "redo", "history"] {
        let line = format!(r#"{{"v":1,"id":3,"method":"{retired}"}}"#);
        assert!(serde_json::from_str::<Frame>(&line).is_err(), "{retired}");
    }

    // Forme exacte sur le fil ; `params` absent vaut `null`.
    let frame: Frame = serde_json::from_str(r#"{"v":1,"id":3,"method":"hello"}"#).unwrap();
    assert_eq!(frame.params, Value::Null);
    assert_eq!(frame.method, Method::Hello);
    // Champ inconnu ou méthode inconnue : refus.
    assert!(serde_json::from_str::<Frame>(r#"{"v":1,"id":3,"method":"hello","x":1}"#).is_err());
    assert!(serde_json::from_str::<Frame>(r#"{"v":1,"id":3,"method":"open"}"#).is_err());

    let replies = [
        Reply::Ok {
            id: 1,
            result: json!({ "a": 1 }),
        },
        Reply::error(2, codes::BAD_TOKEN, "jeton refusé"),
        Reply::PendingApproval { id: 3 },
    ];
    let expected = [
        r#"{"ok":{"id":1,"result":{"a":1}}}"#,
        r#"{"err":{"id":2,"error":{"code":"control.bad_token","message":"jeton refusé"}}}"#,
        r#"{"pending_approval":{"id":3}}"#,
    ];
    for ((reply, wire), id) in replies.iter().zip(expected).zip(1..) {
        assert_eq!(serde_json::to_string(reply).unwrap(), wire);
        assert_eq!(serde_json::from_str::<Reply>(wire).unwrap(), *reply);
        assert_eq!(reply.id(), id);
    }

    // Erreur d'outil : tous ses champs passent sur le fil (C-CTL-09).
    let tool = plannercraft_automation::ToolError::simple("task.not_found", "x".to_owned()).with_field("uid");
    let reply = Reply::tool_error(9, &tool);
    let wire: Value = serde_json::from_str(&serde_json::to_string(&reply).unwrap()).unwrap();
    let mut local = serde_json::to_value(&tool).unwrap();
    assert_eq!(wire["err"]["error"], local);
    local["code"] = json!("other");
    assert_ne!(wire["err"]["error"], local);
    assert_eq!(serde_json::from_value::<Reply>(wire).unwrap(), reply);
}

#[test]
fn control_line_too_long_refused() {
    // Lecteur borné : exactement la borne passe, un octet de plus non.
    let max = 16;
    let mut ok = Cursor::new(b"0123456789abcdef\nsuite\r\nfin".to_vec());
    assert_eq!(
        read_line_bounded(&mut ok, max).unwrap().unwrap(),
        b"0123456789abcdef"
    );
    assert_eq!(read_line_bounded(&mut ok, max).unwrap().unwrap(), b"suite");
    assert_eq!(read_line_bounded(&mut ok, max).unwrap().unwrap(), b"fin");
    assert!(read_line_bounded(&mut ok, max).unwrap().is_none());

    let mut long = Cursor::new(b"0123456789abcdefg\n".to_vec());
    assert!(matches!(
        read_line_bounded(&mut long, max),
        Err(LineError::TooLong)
    ));
    // Sans fin de ligne, la borne s'applique aussi (petit tampon : la ligne
    // arrive en plusieurs morceaux).
    let mut chunked = BufReader::with_capacity(3, Cursor::new(vec![b'a'; 40]));
    assert!(matches!(
        read_line_bounded(&mut chunked, max),
        Err(LineError::TooLong)
    ));
    let mut chunked = BufReader::with_capacity(3, Cursor::new(vec![b'a'; 16]));
    assert_eq!(read_line_bounded(&mut chunked, max).unwrap().unwrap().len(), 16);
    assert_eq!(MAX_LINE_BYTES, 8 * 1024 * 1024);

    #[cfg(unix)]
    live::line_too_long_over_socket(Transport::Unix);
}

#[test]
fn control_line_too_long_refused_tcp() {
    live::line_too_long_over_socket(Transport::Tcp);
}

/// Test du canal sur les deux transports : `$unix` sur la socket Unix
/// (Unix seulement), `$tcp` en TCP sur la boucle locale (tous systèmes).
macro_rules! on_both_transports {
    ($( $(#[$doc:meta])* $unix:ident, $tcp:ident => $body:path; )*) => {$(
        $(#[$doc])*
        #[cfg(unix)]
        #[test]
        fn $unix() {
            $body(Transport::Unix);
        }

        $(#[$doc])*
        #[test]
        fn $tcp() {
            $body(Transport::Tcp);
        }
    )*};
}

#[cfg(unix)]
#[test]
fn control_rendezvous_mode_0600() {
    live::rendezvous_mode_0600();
}

#[cfg(unix)]
#[test]
fn control_stale_rendezvous_removed() {
    live::stale_rendezvous_removed();
}

on_both_transports! {
    control_busy_app_kept_live, control_busy_app_kept_live_tcp => live::busy_app_kept_live;
    control_hello_wrong_token_refused, control_hello_wrong_token_refused_tcp => live::hello_wrong_token_refused;
}

#[cfg(unix)]
#[test]
fn control_find_live_matches_canonical_path() {
    live::find_live_matches_canonical_path();
}

on_both_transports! {
    control_unsupported_methods_in_j4, control_unsupported_methods_in_j4_tcp => live::unsupported_methods_in_j4;
}

on_both_transports! {
    /// C-CTL-01 (relais J7) : jeton faux ou absent, ou requête avant `hello` ⇒
    /// refus et connexion fermée ; rien n'atteint la file de l'application.
    c_ctl_01_bad_token_closes, c_ctl_01_bad_token_closes_tcp => live::relay_bad_token_closes;
    /// File de l'application pleine ⇒ `control.busy`, connexion maintenue ;
    /// l'appel en file reçoit sa réponse quand l'application la vide.
    relay_queue_full_busy, relay_queue_full_busy_tcp => live::relay_queue_full_busy;
    /// C-CTL-04 (côté écouteur) : sans réponse finale dans le délai, l'appel
    /// expire (`claim` faux ensuite), `Cancel(id)` suit dans la file et l'agent
    /// reçoit `control.approval_timeout` après `pending_approval`.
    c_ctl_04_listener_expires_and_cancels, c_ctl_04_listener_expires_and_cancels_tcp => live::relay_expires_and_cancels;
    /// Relecture finale J7, relais F1 : le délai demandé par le client
    /// (`approval_timeout_ms`), plus court que celui de l'écouteur, l'emporte ;
    /// l'agent reçoit `control.approval_timeout` et le `Cancel` porte l'état de
    /// l'appel abandonné.
    relay_asked_timeout_shorter_wins, relay_asked_timeout_shorter_wins_tcp => live::relay_asked_timeout_shorter_wins;
    /// Relecture finale J7, relais F1 : un client parti pendant l'attente
    /// (processus mort, délai du client écoulé) fait expirer l'appel, bandeau
    /// retiré (`Cancel`) : plus rien ne peut être accepté.
    relay_client_hangup_expires_call, relay_client_hangup_expires_call_tcp => live::relay_client_hangup_expires_call;
    /// Relecture finale J7, relais F4 : connexions pleines ⇒ `control.busy`
    /// dit au client, qui ne se détache pas (pas `control.app_gone`).
    relay_connections_full_busy, relay_connections_full_busy_tcp => live::relay_connections_full_busy;
    /// Relecture finale J8 (sécurité F1) : les connexions sans `hello` sont
    /// quatre au plus (la plus ancienne évincée) ; un client muni du jeton
    /// est servi quoi qu'elles fassent.
    relay_silent_connections_never_block, relay_silent_connections_never_block_tcp => live::relay_silent_connections_never_block;
    /// Relecture finale J8 (sécurité F1) : échéance absolue du `hello`, une
    /// ligne envoyée octet par octet ne la repousse pas.
    hello_drip_deadline_absolute, hello_drip_deadline_absolute_tcp => live::hello_drip_deadline_absolute;
    /// Relecture finale J7, relais F2 : jeton remplacé (document neuf) ⇒ un
    /// client du jeton précédent reçoit `control.document_changed` (et se
    /// détache) ; le nouveau rendez-vous sert normalement.
    relay_rotated_token_refuses_previous_document, relay_rotated_token_refuses_previous_document_tcp => live::relay_rotated_token_refuses_previous_document;
    /// `query` relaie les seules lectures du registre (`project.summary` par
    /// défaut) ; `execute`, `ui_state`, `screenshot` donnent leur appel ;
    /// paramètres mal formés refusés sans fermer.
    relay_methods_become_calls, relay_methods_become_calls_tcp => live::relay_methods_become_calls;
}

/// `call_with` (ADR-0015 D9) : même JSON que `Workspace::call` ; une
/// écriture commitée porte l'origine de l'environnement (`Agent` : entrée
/// d'historique marquée « agent », JSON `source`), un `dry_run` ne laisse
/// rien.
#[test]
fn relay_call_with_marks_agent_source() {
    use plannercraft_automation::text::Language;
    use plannercraft_automation::{CallEnv, EditSource, Session, Workspace, call_with};

    let now: plannercraft_core::Moment = "2026-10-19T08:00".parse().unwrap();
    let mut ws = Workspace::new(now, Language::Fr);
    ws.call("project.new", json!({ "name": "P", "start": "2026-10-19T08:00" }))
        .unwrap();
    let mut session: Session = ws.session().unwrap().clone();
    let env = CallEnv {
        source: EditSource::Agent,
        ..CallEnv::new(now, Language::Fr)
    };
    let add = json!({ "name": "T", "duration": {"text": "2j"} });
    let dry = call_with(
        &mut session,
        &env,
        "task.add",
        json!({ "name": "T", "duration": {"text": "2j"}, "dry_run": true }),
    )
    .unwrap();
    assert_eq!(dry["committed"], false);
    assert_eq!(session.generation(), 0);
    let relayed = call_with(&mut session, &env, "task.add", add.clone()).unwrap();
    let local = ws.call("task.add", add).unwrap();
    assert_eq!(relayed, local);
    let history = call_with(&mut session, &env, "history.list", json!({})).unwrap();
    assert_eq!(history["entries"][0]["source"], "agent");
    let local_history = ws.call("history.list", json!({})).unwrap();
    assert!(local_history["entries"][0].get("source").is_none());
    assert_eq!(
        serde_json::to_value(session.project()).unwrap(),
        serde_json::to_value(ws.session().unwrap().project()).unwrap()
    );
}

/// C-CTL-13 : sous TCP, un rendez-vous dont la sonde `hello` échoue en
/// 2 s est retiré (port fermé, programme muet sur le port, jeton refusé,
/// adresse illisible) ; une réponse (`hello` accepté ou `control.busy`)
/// le garde. Remplace la règle des 24 h de Windows (J4).
#[test]
fn c_ctl_13_dead_rendezvous_removed_by_probe() {
    live::dead_tcp_rendezvous_removed_by_probe();
}

on_both_transports! {
    /// C-CTL-14 : une première ligne qui n'est pas une trame (requête HTTP
    /// d'une page web, texte quelconque, ligne trop longue) ferme la
    /// connexion sans réponse ; rien n'atteint l'application.
    c_ctl_14_http_first_line_closed_unix, c_ctl_14_http_first_line_closed => live::first_line_not_a_frame_closed;
    /// C-CTL-11 : adresse publiée dans le rendez-vous (`tcp:127.0.0.1:<port>`
    /// ou chemin de la socket) et client commun (`RelayClient`, `probe`,
    /// `find_live_for_in`) ; arrêt de l'écouteur ⇒ adresse injoignable.
    c_ctl_11_rendezvous_and_common_client_unix, c_ctl_11_rendezvous_and_common_client => live::rendezvous_and_common_client;
}

/// Tests de l'écouteur, du rendez-vous et du client ; transport en
/// paramètre quand le test vaut pour les deux.
mod live {
    use std::io::{BufReader, ErrorKind, Write};
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use std::sync::mpsc::{Receiver, sync_channel};

    use plannercraft_automation::control::{
        ControlError, ControlStream, Endpoint, ListenerHandle, ListenerOptions, RelayCall, RelayClient,
        RelayKind, Rendezvous, WireError, canonical_project, find_live_for_in, new_token, probe, publish_in,
        spawn_hello_listener_in, spawn_hello_listener_on, spawn_listener_on,
    };
    use plannercraft_automation::error::codes as tool_codes;

    use super::*;

    fn listener(dir: &Path, t: Transport) -> (ListenerHandle, String) {
        let token = new_token().unwrap();
        let handle = spawn_hello_listener_on(dir, token.clone(), Arc::new(Mutex::new(None)), t).unwrap();
        (handle, token)
    }

    /// Connexion de test : envoie des trames, lit les réponses.
    struct Client {
        stream: ControlStream,
        reader: BufReader<ControlStream>,
    }

    impl Client {
        fn connect(endpoint: &str) -> Self {
            let stream = Endpoint::from_rendezvous(endpoint)
                .unwrap()
                .connect(Duration::from_secs(5))
                .unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
            let reader = BufReader::new(stream.try_clone().unwrap());
            Self { stream, reader }
        }

        fn send_raw(&mut self, bytes: &[u8]) {
            self.stream.write_all(bytes).unwrap();
        }

        fn call(&mut self, id: u64, method: Method, params: Value) -> Reply {
            let frame = Frame {
                v: 1,
                id,
                method,
                params,
            };
            self.send_raw(&to_line(&frame).unwrap());
            self.reply()
        }

        fn reply(&mut self) -> Reply {
            let line = read_line_bounded(&mut self.reader, MAX_LINE_BYTES)
                .unwrap()
                .expect("réponse attendue");
            serde_json::from_slice(&line).unwrap()
        }

        /// La connexion a été fermée par l'application, sans réponse de
        /// plus (fin de flux ; en TCP, une remise à zéro vaut fermeture).
        fn is_closed(&mut self) -> bool {
            match read_line_bounded(&mut self.reader, MAX_LINE_BYTES) {
                Ok(None) => true,
                Err(LineError::Io(e)) => {
                    matches!(
                        e.kind(),
                        ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted
                    )
                }
                _ => false,
            }
        }

        fn hello(&mut self, token: &str) -> Reply {
            self.call(1, Method::Hello, json!({ "token": token }))
        }
    }

    fn err_code(reply: &Reply) -> &str {
        match reply {
            Reply::Err {
                error: WireError { code, .. },
                ..
            } => code,
            other => panic!("erreur attendue : {other:?}"),
        }
    }

    fn project_file(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"<Project/>").unwrap();
        path
    }

    fn json_files(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| Path::new(n).extension().is_some_and(|e| e == "json"))
            .collect();
        names.sort();
        names
    }

    pub(super) fn line_too_long_over_socket(t: Transport) {
        let tmp = TempDir::new("long");
        let (handle, token) = listener(tmp.path(), t);
        let mut client = Client::connect(handle.endpoint());
        assert!(matches!(client.hello(&token), Reply::Ok { .. }));
        // Écriture dans un fil : l'application coupe avant la fin.
        let mut writer = client.stream.try_clone().unwrap();
        let sender = std::thread::spawn(move || {
            let mut line = vec![b' '; MAX_LINE_BYTES + 1];
            line.push(b'\n');
            let _ = writer.write_all(&line);
        });
        let reply = client.reply();
        assert_eq!(err_code(&reply), codes::LINE_TOO_LONG);
        assert!(client.is_closed());
        sender.join().unwrap();
    }

    #[cfg(unix)]
    pub(super) fn rendezvous_mode_0600() {
        let tmp = TempDir::new("mode");
        // Dossier existant trop ouvert : resserré à 0700.
        let dir = tmp.path().join("state").join("control");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        let r = Rendezvous {
            v: 1,
            pid: std::process::id(),
            endpoint: "/nulle/part.sock".to_owned(),
            token: new_token().unwrap(),
            project: Some("/p/a.xml".to_owned()),
        };
        let guard = publish_in(&dir, &r).unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(guard.path()), 0o600);
        assert_eq!(mode(&dir), 0o700);
        let back: Rendezvous = serde_json::from_slice(&std::fs::read(guard.path()).unwrap()).unwrap();
        assert_eq!(back, r);
        // Aucun fichier temporaire ne reste.
        let names: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
        assert_eq!(names.len(), 1);
        // Le jeton n'apparaît pas dans la trace.
        assert!(!format!("{r:?}").contains(&r.token));

        // Dossier créé de zéro, parents compris, en 0700.
        let fresh = tmp.path().join("neuf").join("control");
        let guard2 = publish_in(&fresh, &r).unwrap();
        assert_eq!(mode(&fresh), 0o700);
        assert_eq!(mode(guard2.path()), 0o600);
        assert_ne!(guard.path(), guard2.path());

        // Suppression à la destruction.
        let path = guard.path().to_path_buf();
        drop(guard);
        assert!(!path.exists());

        // Refus : jeton mal formé, version inconnue.
        let bad = Rendezvous {
            token: "abc".to_owned(),
            ..r.clone()
        };
        assert_eq!(publish_in(&dir, &bad).unwrap_err(), ControlError::InvalidToken);
        let mut v2 = r;
        v2.v = 2;
        assert!(matches!(publish_in(&dir, &v2), Err(ControlError::Invalid(_))));
        assert_eq!(json_files(&dir), Vec::<String>::new());
    }

    #[cfg(unix)]
    pub(super) fn stale_rendezvous_removed() {
        let tmp = TempDir::new("stale");
        let dir = tmp.path().join("control");
        let file = project_file(tmp.path(), "plan.xml");
        let project = canonical_project(&file);
        assert!(project.is_some());

        // 1. Socket absente (application morte).
        let dead = Rendezvous {
            v: 1,
            pid: 999_999,
            endpoint: dir.join("mort.sock").to_str().unwrap().to_owned(),
            token: new_token().unwrap(),
            project: project.clone(),
        };
        std::mem::forget(publish_in(&dir, &dead).unwrap());
        assert_eq!(json_files(&dir).len(), 1);
        assert!(find_live_for_in(&dir, &file).is_none());
        assert_eq!(json_files(&dir), Vec::<String>::new());

        // 2. Écouteur vivant mais jeton du rendez-vous faux.
        let (handle, _token) = listener(&dir, Transport::Unix);
        let mut wrong = handle.rendezvous(project.clone());
        wrong.token = new_token().unwrap();
        std::mem::forget(publish_in(&dir, &wrong).unwrap());
        assert!(find_live_for_in(&dir, &file).is_none());
        assert_eq!(json_files(&dir), Vec::<String>::new());

        // 3. Écouteur arrêté après publication : le rendez-vous périme.
        let guard = publish_in(&dir, &handle.rendezvous(project.clone())).unwrap();
        assert!(find_live_for_in(&dir, &file).is_some());
        let endpoint = handle.endpoint().to_owned();
        drop(handle);
        assert!(!Path::new(&endpoint).exists(), "socket supprimée");
        std::mem::forget(guard);
        assert!(find_live_for_in(&dir, &file).is_none());
        assert_eq!(json_files(&dir), Vec::<String>::new());

        // 4. Fichier v1 illisible : supprimé ; autre version : conservé.
        std::fs::write(dir.join("1-0.json"), b"{pas du json").unwrap();
        std::fs::write(dir.join("2-0.json"), br#"{"v":1,"pid":"x"}"#).unwrap();
        std::fs::write(dir.join("3-0.json"), br#"{"v":2,"futur":true}"#).unwrap();
        std::fs::write(dir.join("4-0.tmp"), b"en cours").unwrap();
        assert!(find_live_for_in(&dir, &file).is_none());
        assert_eq!(json_files(&dir), ["3-0.json"]);
        assert!(dir.join("4-0.tmp").exists());

        // 5. Rendez-vous d'un autre projet dont la socket a disparu :
        // nettoyé au passage.
        let mut other = dead;
        other.project = Some("/ailleurs/autre.xml".to_owned());
        std::mem::forget(publish_in(&dir, &other).unwrap());
        assert!(find_live_for_in(&dir, &file).is_none());
        assert_eq!(json_files(&dir), ["3-0.json"]);
    }

    pub(super) fn busy_app_kept_live(t: Transport) {
        // Application vivante qui sert déjà ses 8 connexions : la sonde
        // reçoit `control.busy` puis est fermée, le rendez-vous reste et
        // l'application est tenue pour vivante (la garde ADR-0002 tient).
        let tmp = TempDir::new("busy");
        let dir = tmp.path().join("control");
        let file = project_file(tmp.path(), "plan.xml");
        let (handle, token) = listener(&dir, t);
        let _guard = publish_in(&dir, &handle.rendezvous(canonical_project(&file))).unwrap();
        // Huit connexions authentifiées (des connexions sans `hello` ne
        // peuvent plus prendre toutes les places : relecture finale J8).
        let idle: Vec<Client> = (0..8)
            .map(|_| {
                let mut c = Client::connect(handle.endpoint());
                assert!(matches!(c.hello(&token), Reply::Ok { .. }));
                c
            })
            .collect();
        // La neuvième reçoit `control.busy` puis est fermée : le plafond
        // est atteint (relecture finale J7, relais F4).
        let mut ninth = Client::connect(handle.endpoint());
        assert_eq!(err_code(&ninth.reply()), codes::BUSY);
        assert!(ninth.is_closed());
        assert!(probe(&handle.rendezvous(None)).is_err());
        assert!(find_live_for_in(&dir, &file).is_some());
        assert_eq!(json_files(&dir).len(), 1);
        drop(idle);
    }

    pub(super) fn hello_wrong_token_refused(t: Transport) {
        let tmp = TempDir::new("tok");
        let (handle, token) = listener(tmp.path(), t);

        // Jeton faux : refus puis fermeture.
        let mut client = Client::connect(handle.endpoint());
        let reply = client.hello(&new_token().unwrap());
        assert_eq!(err_code(&reply), codes::BAD_TOKEN);
        assert_eq!(reply.id(), 1);
        assert!(client.is_closed());

        // Jeton mal formé ou absent.
        for params in [json!({ "token": "zz" }), json!({}), Value::Null] {
            let mut client = Client::connect(handle.endpoint());
            let reply = client.call(5, Method::Hello, params);
            assert_eq!(err_code(&reply), codes::BAD_TOKEN);
            assert!(client.is_closed());
        }

        // Requête avant `hello` : refus puis fermeture.
        let mut client = Client::connect(handle.endpoint());
        let reply = client.call(2, Method::Query, Value::Null);
        assert_eq!(err_code(&reply), codes::UNAUTHENTICATED);
        assert!(client.is_closed());

        // Sonde avec un jeton faux.
        let mut r = handle.rendezvous(None);
        r.token = new_token().unwrap();
        match probe(&r) {
            Err(ControlError::Refused(e)) => assert_eq!(e.code, codes::BAD_TOKEN),
            other => panic!("refus attendu : {other:?}"),
        }

        // Le bon jeton passe, en majuscules aussi.
        let mut client = Client::connect(handle.endpoint());
        assert!(matches!(client.hello(&token.to_uppercase()), Reply::Ok { .. }));
        let ok = probe(&handle.rendezvous(None)).unwrap();
        assert_eq!(ok["app"], "plannercraft");
        assert_eq!(ok["pid"], std::process::id());

        // Écouteur refusé pour un jeton mal formé.
        assert_eq!(
            spawn_hello_listener_in(tmp.path(), "abc".to_owned(), Arc::new(Mutex::new(None))).unwrap_err(),
            ControlError::InvalidToken
        );
    }

    #[cfg(unix)]
    pub(super) fn find_live_matches_canonical_path() {
        let tmp = TempDir::new("find");
        let dir = tmp.path().join("control");
        std::fs::create_dir_all(tmp.path().join("sub")).unwrap();
        let file = project_file(tmp.path(), "plan.xml");
        let other = project_file(tmp.path(), "autre.xml");
        let (handle, _token) = listener(&dir, Transport::Unix);
        let r = handle.rendezvous(canonical_project(&file));
        let _guard = publish_in(&dir, &r).unwrap();

        // Chemin non canonique (`sub/..`, lien symbolique) : même fichier.
        let detour = tmp.path().join("sub").join("..").join("plan.xml");
        assert_eq!(find_live_for_in(&dir, &detour), Some(r.clone()));
        let link = tmp.path().join("lien.xml");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        assert_eq!(find_live_for_in(&dir, &link), Some(r.clone()));
        assert_eq!(find_live_for_in(&dir, &file), Some(r));
        // Autre fichier, fichier absent, dossier absent : rien.
        assert!(find_live_for_in(&dir, &other).is_none());
        assert!(find_live_for_in(&dir, &tmp.path().join("absent.xml")).is_none());
        assert!(find_live_for_in(&tmp.path().join("rien"), &file).is_none());
        // Le rendez-vous vivant est conservé.
        assert_eq!(json_files(&dir).len(), 1);
    }

    pub(super) fn unsupported_methods_in_j4(t: Transport) {
        let tmp = TempDir::new("meth");
        let (handle, token) = listener(tmp.path(), t);
        let mut client = Client::connect(handle.endpoint());
        match client.hello(&token) {
            Reply::Ok { id, result } => {
                assert_eq!(id, 1);
                assert_eq!(result["v"], 1);
                assert_eq!(result["methods"], json!(["hello", "query"]));
            }
            other => panic!("hello attendu : {other:?}"),
        }
        // Méthodes réservées : refus, la connexion reste ouverte.
        let reserved = [Method::Execute, Method::UiState, Method::Screenshot];
        for (method, id) in reserved.into_iter().zip(10..) {
            assert!(!method.available_in_j4());
            let reply = client.call(id, method, json!({}));
            assert_eq!(err_code(&reply), codes::UNSUPPORTED, "{method:?}");
            assert_eq!(reply.id(), id);
        }
        // `query` : résumé (aucun document ouvert ⇒ null).
        for params in [Value::Null, json!({}), json!({ "command": "project.summary" })] {
            assert_eq!(
                client.call(20, Method::Query, params),
                Reply::Ok {
                    id: 20,
                    result: Value::Null
                }
            );
        }
        let reply = client.call(21, Method::Query, json!({ "command": "task.list" }));
        assert_eq!(err_code(&reply), codes::UNSUPPORTED);
        let reply = client.call(22, Method::Query, json!([1]));
        assert_eq!(err_code(&reply), codes::INVALID_FRAME);
        // Version inconnue, trame illisible : refus, identifiant repris.
        client.send_raw(b"{\"v\":2,\"id\":23,\"method\":\"query\",\"params\":null}\n");
        let reply = client.reply();
        assert_eq!(err_code(&reply), codes::UNSUPPORTED_VERSION);
        assert_eq!(reply.id(), 23);
        client.send_raw(b"{\"v\":1,\"id\":24,\"method\":\"open\"}\n");
        let reply = client.reply();
        assert_eq!(err_code(&reply), codes::INVALID_FRAME);
        assert_eq!(reply.id(), 24);
        client.send_raw(b"pas du json\n");
        assert_eq!(err_code(&client.reply()), codes::INVALID_FRAME);
        // Toujours ouverte après ces refus.
        assert!(matches!(
            client.call(25, Method::Query, Value::Null),
            Reply::Ok { id: 25, .. }
        ));
    }

    /// Écouteur du relais sur une file de `capacity` appels, délai
    /// `timeout` ; le rappel compte les réveils.
    fn relay_listener(
        dir: &Path,
        capacity: usize,
        timeout: Duration,
        t: Transport,
    ) -> (ListenerHandle, String, Receiver<RelayCall>, Arc<AtomicU64>) {
        let token = new_token().unwrap();
        let (tx, rx) = sync_channel(capacity);
        let woken = Arc::new(AtomicU64::new(0));
        let counter = Arc::clone(&woken);
        let wake: plannercraft_automation::control::Waker = Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        let options = ListenerOptions {
            approval_timeout: timeout,
            reply_grace: Duration::from_millis(200),
        };
        let handle = spawn_listener_on(dir, token.clone(), tx, wake, options, t).unwrap();
        (handle, token, rx, woken)
    }

    fn execute(tool: &str) -> Value {
        json!({ "tool": tool, "params": {} })
    }

    pub(super) fn relay_bad_token_closes(t: Transport) {
        let tmp = TempDir::new("rtok");
        let (handle, token, rx, _) = relay_listener(tmp.path(), 4, Duration::from_secs(5), t);
        for bad in [json!({ "token": "0".repeat(64) }), json!({}), Value::Null] {
            let mut client = Client::connect(handle.endpoint());
            let reply = client.call(1, Method::Hello, bad);
            assert_eq!(err_code(&reply), codes::BAD_TOKEN);
            assert!(client.is_closed());
        }
        let mut client = Client::connect(handle.endpoint());
        let reply = client.call(2, Method::Execute, execute("project.summary"));
        assert_eq!(err_code(&reply), codes::UNAUTHENTICATED);
        assert!(client.is_closed());
        assert!(rx.try_recv().is_err(), "rien ne doit atteindre la file");
        // Le bon jeton : méthodes du relais annoncées.
        let mut client = Client::connect(handle.endpoint());
        match client.hello(&token) {
            Reply::Ok { result, .. } => assert_eq!(
                result["methods"],
                json!(["hello", "query", "execute", "ui_state", "screenshot"])
            ),
            other => panic!("hello attendu : {other:?}"),
        }
    }

    pub(super) fn relay_queue_full_busy(t: Transport) {
        let tmp = TempDir::new("rbusy");
        let (handle, token, rx, woken) = relay_listener(tmp.path(), 1, Duration::from_secs(10), t);
        let endpoint = handle.endpoint().to_owned();
        let first_token = token.clone();
        let first = std::thread::spawn(move || {
            let mut client = Client::connect(&endpoint);
            client.hello(&first_token);
            client.call(5, Method::Execute, execute("project.summary"))
        });
        // Le premier appel occupe la seule place de la file.
        while woken.load(Ordering::SeqCst) < 1 {
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut client = Client::connect(handle.endpoint());
        client.hello(&token);
        let reply = client.call(7, Method::Execute, execute("task.list"));
        assert_eq!(err_code(&reply), codes::BUSY);
        assert_eq!(reply.id(), 7);
        // Connexion maintenue : toujours pleine, même réponse.
        let reply = client.call(8, Method::UiState, Value::Null);
        assert_eq!(err_code(&reply), codes::BUSY);
        // L'application vide la file et répond au premier appel.
        let call = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(call.id, 5);
        assert!(call.state.claim());
        call.reply
            .send(Reply::Ok {
                id: 5,
                result: json!("a"),
            })
            .unwrap();
        assert_eq!(
            first.join().unwrap(),
            Reply::Ok {
                id: 5,
                result: json!("a")
            }
        );
        // La place libérée sert l'appel suivant de la même connexion.
        let frame = Frame {
            v: 1,
            id: 9,
            method: Method::UiState,
            params: Value::Null,
        };
        client.send_raw(&to_line(&frame).unwrap());
        let call = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!((call.id, &call.kind), (9, &RelayKind::UiState));
        call.reply
            .send(Reply::Ok {
                id: 9,
                result: json!("b"),
            })
            .unwrap();
        assert_eq!(
            client.reply(),
            Reply::Ok {
                id: 9,
                result: json!("b")
            }
        );
    }

    pub(super) fn relay_expires_and_cancels(t: Transport) {
        let tmp = TempDir::new("rexp");
        let (handle, token, rx, _) = relay_listener(tmp.path(), 4, Duration::from_millis(300), t);
        let mut client = Client::connect(handle.endpoint());
        client.hello(&token);
        let frame = Frame {
            v: 1,
            id: 9,
            method: Method::Execute,
            params: execute("task.add"),
        };
        client.send_raw(&to_line(&frame).unwrap());
        let call = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(call.id, 9);
        call.reply.send(Reply::PendingApproval { id: 9 }).unwrap();
        assert_eq!(client.reply(), Reply::PendingApproval { id: 9 });
        let reply = client.reply();
        assert_eq!(err_code(&reply), tool_codes::CONTROL_APPROVAL_TIMEOUT);
        assert_eq!(reply.id(), 9);
        // Course Accepter/délai tranchée : l'application ne peut plus le
        // prendre ; le bandeau est retiré par `Cancel`.
        assert!(!call.state.claim());
        let cancel = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(cancel.kind, RelayKind::Cancel(9));
        // Une réponse tardive n'arrive jamais à l'agent ; la connexion
        // sert la requête suivante.
        let _ = call.reply.send(Reply::Ok {
            id: 9,
            result: Value::Null,
        });
        let frame = Frame {
            v: 1,
            id: 10,
            method: Method::UiState,
            params: Value::Null,
        };
        client.send_raw(&to_line(&frame).unwrap());
        let call = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(call.kind, RelayKind::UiState);
        call.reply
            .send(Reply::Ok {
                id: 10,
                result: json!({}),
            })
            .unwrap();
        assert_eq!(
            client.reply(),
            Reply::Ok {
                id: 10,
                result: json!({})
            }
        );
    }

    pub(super) fn relay_asked_timeout_shorter_wins(t: Transport) {
        let tmp = TempDir::new("rask");
        let (handle, token, rx, _) = relay_listener(tmp.path(), 4, Duration::from_secs(30), t);
        let mut client = Client::connect(handle.endpoint());
        client.hello(&token);
        let frame = Frame {
            v: 1,
            id: 9,
            method: Method::Execute,
            params: json!({ "tool": "task.add", "params": {}, "approval_timeout_ms": 300 }),
        };
        let start = std::time::Instant::now();
        client.send_raw(&to_line(&frame).unwrap());
        let call = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        call.reply.send(Reply::PendingApproval { id: 9 }).unwrap();
        assert_eq!(client.reply(), Reply::PendingApproval { id: 9 });
        let reply = client.reply();
        assert_eq!(err_code(&reply), tool_codes::CONTROL_APPROVAL_TIMEOUT);
        assert!(start.elapsed() < Duration::from_secs(10), "{:?}", start.elapsed());
        assert!(!call.state.claim(), "expiré : plus rien ne peut être accepté");
        let cancel = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(cancel.kind, RelayKind::Cancel(9));
        assert!(Arc::ptr_eq(&cancel.state, &call.state), "Cancel de cet appel-là");
        // Délai mal formé : refusé sans fermer.
        let frame = Frame {
            v: 1,
            id: 10,
            method: Method::Execute,
            params: json!({ "tool": "task.add", "approval_timeout_ms": "vite" }),
        };
        client.send_raw(&to_line(&frame).unwrap());
        assert_eq!(err_code(&client.reply()), codes::INVALID_FRAME);
    }

    pub(super) fn relay_client_hangup_expires_call(t: Transport) {
        let tmp = TempDir::new("rhup");
        let (handle, token, rx, _) = relay_listener(tmp.path(), 4, Duration::from_secs(30), t);
        let mut client = Client::connect(handle.endpoint());
        client.hello(&token);
        let frame = Frame {
            v: 1,
            id: 9,
            method: Method::Execute,
            params: execute("task.add"),
        };
        client.send_raw(&to_line(&frame).unwrap());
        let call = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        call.reply.send(Reply::PendingApproval { id: 9 }).unwrap();
        assert_eq!(client.reply(), Reply::PendingApproval { id: 9 });
        drop(client);
        let cancel = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(cancel.kind, RelayKind::Cancel(9));
        assert!(Arc::ptr_eq(&cancel.state, &call.state));
        assert!(
            !call.state.claim(),
            "appelant parti : plus rien ne peut être accepté"
        );
    }

    pub(super) fn relay_connections_full_busy(t: Transport) {
        let tmp = TempDir::new("rconn");
        let (handle, token, _rx, _) = relay_listener(tmp.path(), 4, Duration::from_secs(5), t);
        let rendezvous = handle.rendezvous(None);
        let mut agent = RelayClient::connect(&rendezvous, Duration::from_secs(2)).unwrap();
        // Huit connexions authentifiées occupent toutes les places (des
        // connexions muettes ne le peuvent plus : relecture finale J8,
        // `relay_silent_connections_never_block`).
        let mut held = Vec::new();
        for _ in 0..8 {
            let mut c = Client::connect(handle.endpoint());
            assert!(matches!(c.hello(&token), Reply::Ok { .. }));
            held.push(c);
        }
        std::thread::sleep(Duration::from_millis(100));
        let mut ninth = Client::connect(handle.endpoint());
        let reply = ninth.reply();
        assert_eq!(err_code(&reply), codes::BUSY);
        assert_eq!(reply.id(), 0);
        let err = agent
            .call("project.summary", json!({}), Duration::from_secs(2))
            .unwrap_err();
        assert_eq!(err.code(), tool_codes::CONTROL_BUSY);
        assert!(!err.detaches(), "application vivante : rattachement gardé");
        drop(held);
    }

    pub(super) fn relay_silent_connections_never_block(t: Transport) {
        // Relecture finale J8 (sécurité F1) : douze connexions sans `hello`
        // (un autre compte de la machine, sous TCP) n'en gardent que quatre,
        // les plus récentes ; un client muni du jeton est toujours servi.
        // Les évincées reçoivent `control.busy` avant la coupure.
        let tmp = TempDir::new("rsilent");
        let (handle, token, rx, _) = relay_listener(tmp.path(), 4, Duration::from_secs(5), t);
        let mut silent: Vec<Client> = (0..12).map(|_| Client::connect(handle.endpoint())).collect();
        std::thread::sleep(Duration::from_millis(200));
        let mut client = Client::connect(handle.endpoint());
        assert!(matches!(client.hello(&token), Reply::Ok { .. }));
        // Les plus anciennes ont reçu `control.busy` (un client légitime
        // évincé ne se détache pas) puis ont été coupées.
        for evicted in &mut silent[..2] {
            assert_eq!(err_code(&evicted.reply()), codes::BUSY);
            assert!(evicted.is_closed());
        }
        drop(silent);
        drop(rx);
    }

    pub(super) fn hello_drip_deadline_absolute(t: Transport) {
        // Relecture finale J8 (sécurité F1) : un octet toutes les 2 s sans
        // fin de ligne ne garde pas la connexion au-delà de l'échéance du
        // `hello` (5 s depuis l'acceptation).
        let tmp = TempDir::new("drip");
        let (handle, _token) = listener(tmp.path(), t);
        let mut client = Client::connect(handle.endpoint());
        let start = std::time::Instant::now();
        let mut closed = false;
        for _ in 0..6 {
            if client.stream.write_all(b"{").is_err() {
                closed = true;
                break;
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        // Fermée par l'écouteur : lecture en fin de flux.
        closed = closed || client.is_closed();
        assert!(closed, "connexion gardée par le goutte-à-goutte");
        assert!(start.elapsed() < Duration::from_secs(13));
    }

    pub(super) fn relay_rotated_token_refuses_previous_document(t: Transport) {
        let tmp = TempDir::new("rrot");
        let (mut handle, token, rx, _) = relay_listener(tmp.path(), 4, Duration::from_secs(5), t);
        let old = handle.rendezvous(None);
        assert_eq!(old.token, token);
        let mut agent = RelayClient::connect(&old, Duration::from_secs(2)).unwrap();
        handle.rotate_token().unwrap();
        let new = handle.rendezvous(None);
        assert_ne!(new.token, old.token);
        let err = agent
            .call("project.summary", json!({}), Duration::from_secs(2))
            .unwrap_err();
        assert_eq!(err.code(), tool_codes::CONTROL_DOCUMENT_CHANGED);
        assert!(err.detaches());
        assert!(rx.try_recv().is_err(), "rien n'atteint l'application");
        let mut client = Client::connect(handle.endpoint());
        assert!(matches!(client.hello(&new.token), Reply::Ok { .. }));
        let mut stale = Client::connect(handle.endpoint());
        assert_eq!(err_code(&stale.hello(&old.token)), codes::BAD_TOKEN);
    }

    pub(super) fn relay_methods_become_calls(t: Transport) {
        let tmp = TempDir::new("rmeth");
        let (handle, token, rx, _) = relay_listener(tmp.path(), 8, Duration::from_secs(5), t);
        let mut client = Client::connect(handle.endpoint());
        client.hello(&token);
        let cases = [
            (
                Method::Query,
                Value::Null,
                RelayKind::Execute {
                    tool: "project.summary".to_owned(),
                    params: json!({}),
                },
            ),
            (
                Method::Query,
                json!({ "command": "task.get", "params": { "uid": 3 } }),
                RelayKind::Execute {
                    tool: "task.get".to_owned(),
                    params: json!({ "uid": 3 }),
                },
            ),
            (
                Method::Execute,
                json!({ "tool": "history.undo" }),
                RelayKind::Execute {
                    tool: "history.undo".to_owned(),
                    params: json!({}),
                },
            ),
            (Method::UiState, json!({}), RelayKind::UiState),
            (
                Method::Screenshot,
                json!({ "scale_down": 1 }),
                RelayKind::Screenshot { scale_down: 1 },
            ),
        ];
        for ((method, params, kind), id) in cases.into_iter().zip(30..) {
            let frame = Frame {
                v: 1,
                id,
                method,
                params,
            };
            client.send_raw(&to_line(&frame).unwrap());
            let call = rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!((call.id, &call.kind), (id, &kind));
            assert!(call.state.claim());
            call.reply
                .send(Reply::Ok {
                    id,
                    result: json!(id),
                })
                .unwrap();
            assert_eq!(
                client.reply(),
                Reply::Ok {
                    id,
                    result: json!(id)
                }
            );
        }
        let refused = [
            (
                Method::Query,
                json!({ "command": "task.add" }),
                codes::UNSUPPORTED,
            ),
            (Method::Query, json!({ "command": "nope" }), codes::UNSUPPORTED),
            (Method::Query, json!({ "command": 3 }), codes::INVALID_FRAME),
            (Method::Execute, json!({}), codes::INVALID_FRAME),
            (
                Method::Execute,
                json!({ "tool": "x", "y": 1 }),
                codes::INVALID_FRAME,
            ),
            (Method::Execute, json!([1]), codes::INVALID_FRAME),
            (Method::UiState, json!({ "x": 1 }), codes::INVALID_FRAME),
            (
                Method::Screenshot,
                json!({ "scale_down": "a" }),
                codes::INVALID_FRAME,
            ),
        ];
        for ((method, params, code), id) in refused.into_iter().zip(50..) {
            let reply = client.call(id, method, params);
            assert_eq!(err_code(&reply), code, "{id}");
        }
        assert!(rx.try_recv().is_err());
        // L'application partie (file fermée) : réponse propre.
        drop(rx);
        let reply = client.call(60, Method::UiState, Value::Null);
        assert_eq!(err_code(&reply), tool_codes::CONTROL_APP_GONE);
    }

    /// C-CTL-11, sur `t` : adresse de l'écouteur publiée telle quelle,
    /// rendez-vous trouvé, rattachement et appel relayé par le client
    /// commun ; écouteur arrêté ⇒ sonde en échec, rendez-vous retiré,
    /// `control.app_gone`.
    pub(super) fn rendezvous_and_common_client(t: Transport) {
        let tmp = TempDir::new("common");
        let dir = tmp.path().join("control");
        let sockets = tmp.path().join("sock");
        let file = project_file(tmp.path(), "plan.xml");
        let (handle, _token, rx, _) = relay_listener(&sockets, 4, Duration::from_secs(5), t);
        let endpoint = handle.endpoint().to_owned();
        let parsed = Endpoint::from_rendezvous(&endpoint).unwrap();
        assert_eq!(parsed.transport(), t);
        match t {
            Transport::Tcp => {
                assert!(endpoint.starts_with("tcp:127.0.0.1:"), "{endpoint}");
                // Aucun fichier de socket : le dossier n'est pas créé.
                assert!(!sockets.exists());
            }
            Transport::Unix => {
                assert!(Path::new(&endpoint).is_absolute(), "{endpoint}");
                assert!(
                    Path::new(&endpoint).extension().is_some_and(|e| e == "sock"),
                    "{endpoint}"
                );
            }
        }
        let guard = publish_in(&dir, &handle.rendezvous(canonical_project(&file))).unwrap();
        let written: Value = serde_json::from_slice(&std::fs::read(guard.path()).unwrap()).unwrap();
        assert_eq!(written["endpoint"], json!(endpoint));
        let found = find_live_for_in(&dir, &file).unwrap();
        assert_eq!(found.endpoint, endpoint);
        assert_eq!(RelayClient::single_live_in(&dir), Some(found.clone()));
        assert_eq!(probe(&found).unwrap()["app"], "plannercraft");

        // L'application répond au premier appel relayé.
        let app = std::thread::spawn(move || {
            let call = rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(
                call.kind,
                RelayKind::Execute {
                    tool: "project.summary".to_owned(),
                    params: json!({}),
                }
            );
            assert!(call.state.claim());
            call.reply
                .send(Reply::Ok {
                    id: call.id,
                    result: json!({ "name": "P" }),
                })
                .unwrap();
            rx
        });
        let mut agent = RelayClient::connect(&found, Duration::from_secs(2)).unwrap();
        let result = agent
            .call("project.summary", json!({}), Duration::from_secs(5))
            .unwrap();
        assert_eq!(result, json!({ "name": "P" }));
        let rx = app.join().unwrap();

        // Écouteur arrêté : plus personne ne répond à cette adresse.
        drop(handle);
        drop(rx);
        std::mem::forget(guard);
        assert!(probe(&found).is_err());
        assert!(find_live_for_in(&dir, &file).is_none());
        assert_eq!(json_files(&dir), Vec::<String>::new());
        let err = agent
            .call("project.summary", json!({}), Duration::from_secs(2))
            .unwrap_err();
        assert!(err.is_app_gone(), "{err:?}");
    }

    /// C-CTL-13 (TCP seulement).
    pub(super) fn dead_tcp_rendezvous_removed_by_probe() {
        let tmp = TempDir::new("tdead");
        let dir = tmp.path().join("control");
        let file = project_file(tmp.path(), "plan.xml");
        let project = canonical_project(&file);
        let at = |endpoint: String, project: Option<String>| Rendezvous {
            v: 1,
            pid: 999_999,
            endpoint,
            token: new_token().unwrap(),
            project,
        };

        // 1. Port fermé (application morte).
        let closed = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        std::mem::forget(publish_in(&dir, &at(format!("tcp:127.0.0.1:{closed}"), project.clone())).unwrap());
        assert!(find_live_for_in(&dir, &file).is_none());
        assert_eq!(json_files(&dir), Vec::<String>::new());

        // 2. Programme muet sur le port (port repris) : la sonde échoue en
        // 2 s, le rendez-vous est retiré (une socket Unix muette, elle, le
        // garderait : `control_busy_app_kept_live`).
        let mute = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = mute.local_addr().unwrap().port();
        let holder = std::thread::spawn(move || mute.accept().map(|(stream, _)| stream));
        std::mem::forget(publish_in(&dir, &at(format!("tcp:127.0.0.1:{port}"), project.clone())).unwrap());
        let start = std::time::Instant::now();
        assert!(find_live_for_in(&dir, &file).is_none());
        let waited = start.elapsed();
        assert!(waited >= Duration::from_millis(1_500), "{waited:?}");
        assert!(waited < Duration::from_secs(10), "{waited:?}");
        assert_eq!(json_files(&dir), Vec::<String>::new());
        drop(holder.join().unwrap());

        // 3. Écouteur vivant, jeton du rendez-vous refusé.
        let (handle, _token) = listener(tmp.path(), Transport::Tcp);
        let mut wrong = handle.rendezvous(project.clone());
        wrong.token = new_token().unwrap();
        std::mem::forget(publish_in(&dir, &wrong).unwrap());
        assert!(find_live_for_in(&dir, &file).is_none());
        assert_eq!(json_files(&dir), Vec::<String>::new());

        // 4. Adresse illisible ou hors boucle locale, pour ce fichier ou un
        // autre : retirée sans connexion.
        for (i, bad) in ["tcp:10.0.0.1:80", "tcp:127.0.0.1:0", "ftp:x", "relatif.sock"]
            .into_iter()
            .enumerate()
        {
            let other = (i % 2 == 1).then(|| "/ailleurs/autre.xml".to_owned());
            let target = if other.is_some() { other } else { project.clone() };
            std::mem::forget(publish_in(&dir, &at(bad.to_owned(), target)).unwrap());
        }
        assert!(find_live_for_in(&dir, &file).is_none());
        assert_eq!(json_files(&dir), Vec::<String>::new());

        // 5. Application vivante : gardée ; le rendez-vous TCP d'un autre
        // fichier n'est ni sondé ni retiré.
        let live = handle.rendezvous(project);
        let _live = publish_in(&dir, &live).unwrap();
        let _other = publish_in(
            &dir,
            &at(
                format!("tcp:127.0.0.1:{closed}"),
                Some("/ailleurs/b.xml".to_owned()),
            ),
        )
        .unwrap();
        assert_eq!(find_live_for_in(&dir, &file), Some(live));
        assert_eq!(json_files(&dir).len(), 2);
    }

    /// C-CTL-14, sur `t`.
    pub(super) fn first_line_not_a_frame_closed(t: Transport) {
        let tmp = TempDir::new("http");
        let (handle, token, rx, _) = relay_listener(tmp.path(), 4, Duration::from_secs(5), t);
        let firsts: [&[u8]; 5] = [
            b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nOrigin: http://exemple.test\r\n\r\n",
            b"POST /execute HTTP/1.1\r\nContent-Length: 2\r\n\r\n{}",
            b"pas du json\n",
            b"{\"v\":1,\"id\":3}\n",
            b"\n",
        ];
        for first in firsts {
            let mut client = Client::connect(handle.endpoint());
            client.send_raw(first);
            assert!(client.is_closed(), "{:?}", String::from_utf8_lossy(first));
        }
        // Première ligne trop longue : fermée sans réponse elle aussi.
        let mut client = Client::connect(handle.endpoint());
        let mut writer = client.stream.try_clone().unwrap();
        let sender = std::thread::spawn(move || {
            let line = vec![b'G'; MAX_LINE_BYTES + 1];
            let _ = writer.write_all(&line);
        });
        assert!(client.is_closed());
        sender.join().unwrap();
        assert!(rx.try_recv().is_err(), "rien ne doit atteindre la file");
        // L'écouteur sert toujours ; après le `hello`, une trame illisible
        // reçoit sa réponse (`control.invalid_frame`), connexion maintenue.
        let mut client = Client::connect(handle.endpoint());
        assert!(matches!(client.hello(&token), Reply::Ok { .. }));
        client.send_raw(b"GET / HTTP/1.1\r\n");
        assert_eq!(err_code(&client.reply()), codes::INVALID_FRAME);
        assert!(matches!(client.hello(&token), Reply::Ok { .. }));
    }
}
