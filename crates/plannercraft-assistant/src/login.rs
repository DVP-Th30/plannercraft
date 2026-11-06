//! Connexion à OpenRouter : réponse HTTP collectée ([`PendingResponse`],
//! commun), écouteur `localhost` et parcours complet du desktop
//! ([`DesktopLogin`]), parcours par redirection du web (`crate::web`).

use std::sync::{Arc, Mutex};

use crate::conversation::Waker;
use crate::error::AssistantError;
use crate::http::{HttpEvent, HttpRequest, Transport};

/// Corps de réponse gardé au plus (échange de code : quelques octets).
const MAX_BODY: usize = 64 * 1024;

#[derive(Default)]
struct Collected {
    status: Option<u16>,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    outcome: Option<Result<(), String>>,
}

/// Réponse complète attendue d'une requête courte (échange du code OAuth).
#[derive(Clone, Default)]
pub struct PendingResponse(Arc<Mutex<Collected>>);

/// Réponse reçue : statut, en-têtes, corps.
pub type Received = (u16, Vec<(String, String)>, Vec<u8>);

impl PendingResponse {
    /// Envoie `request` par `transport` ; `waker` est appelé à la fin.
    #[must_use]
    pub fn send(transport: &dyn Transport, request: HttpRequest, waker: Option<Waker>) -> Self {
        let pending = Self::default();
        let shared = Arc::clone(&pending.0);
        transport.send(
            request,
            Box::new(move |event| {
                let mut done = false;
                if let Ok(mut c) = shared.lock() {
                    match event {
                        HttpEvent::Head { status, headers } => {
                            c.status = Some(status);
                            c.headers = headers;
                        }
                        HttpEvent::Chunk(bytes) => {
                            let room = MAX_BODY.saturating_sub(c.body.len());
                            c.body.extend(bytes.iter().take(room));
                        }
                        HttpEvent::End => {
                            c.outcome = Some(Ok(()));
                            done = true;
                        }
                        HttpEvent::Failed(e) => {
                            c.outcome = Some(Err(e));
                            done = true;
                        }
                    }
                }
                if done && let Some(w) = &waker {
                    w();
                }
                if done {
                    std::ops::ControlFlow::Break(())
                } else {
                    std::ops::ControlFlow::Continue(())
                }
            }),
        );
        pending
    }

    /// Réponse, si elle est complète.
    ///
    /// # Errors
    ///
    /// [`AssistantError::Network`] pour un échec du transport.
    #[must_use]
    pub fn poll(&self) -> Option<Result<Received, AssistantError>> {
        let c = self.0.lock().ok()?;
        match c.outcome.as_ref()? {
            Ok(()) => Some(Ok((c.status.unwrap_or(0), c.headers.clone(), c.body.clone()))),
            Err(e) => Some(Err(AssistantError::Network {
                detail: crate::error::truncate_message(e),
            })),
        }
    }
}

/// Échange `code` contre une clé (réponse attendue par [`PendingResponse`]).
#[must_use]
pub fn start_exchange(
    transport: &dyn Transport,
    code: &str,
    verifier: &str,
    waker: Option<Waker>,
) -> PendingResponse {
    PendingResponse::send(
        transport,
        crate::openrouter::exchange_request(code, verifier),
        waker,
    )
}

/// Clé lue dans une réponse d'échange.
///
/// # Errors
///
/// Voir [`crate::openrouter::exchange_key`].
pub fn key_of(received: Result<Received, AssistantError>) -> Result<String, AssistantError> {
    let (status, headers, body) = received?;
    crate::openrouter::exchange_key(status, &headers, &body)
}

#[cfg(not(target_arch = "wasm32"))]
pub use desktop::{CALLBACK_PATH, DesktopLogin, LOGIN_TIMEOUT, LoginHandle, Loopback};

#[cfg(not(target_arch = "wasm32"))]
mod desktop {
    use std::io::{Read as _, Write as _};
    use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use plannercraft_automation::text::Language;

    use super::{PendingResponse, key_of};
    use crate::conversation::Waker;
    use crate::error::AssistantError;
    use crate::http::SharedTransport;
    use crate::pkce::{Pkce, callback_code};
    use crate::platform::BrowserOpener;

    /// Chemin de retour sur l'écouteur.
    pub const CALLBACK_PATH: &str = "/callback";
    /// Délai au plus pour se connecter dans le navigateur.
    pub const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
    /// Délai au plus de l'échange du code.
    const EXCHANGE_TIMEOUT: Duration = Duration::from_secs(60);
    /// Requête lue au plus sur l'écouteur.
    const MAX_REQUEST: usize = 8 * 1024;

    /// Écouteur d'une seule redirection sur `127.0.0.1`, port libre.
    #[derive(Debug)]
    pub struct Loopback {
        listener: TcpListener,
        port: u16,
    }

    impl Loopback {
        /// Écouteur sur un port libre de `127.0.0.1`.
        ///
        /// # Errors
        ///
        /// [`AssistantError::Login`] si aucun port n'est disponible.
        pub fn bind() -> Result<Self, AssistantError> {
            let fail = |e: std::io::Error| AssistantError::Login {
                detail: format!("écouteur local impossible : {e}"),
            };
            let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).map_err(fail)?;
            listener.set_nonblocking(true).map_err(fail)?;
            let port = listener.local_addr().map_err(fail)?.port();
            Ok(Self { listener, port })
        }

        /// Port de l'écouteur.
        #[must_use]
        pub const fn port(&self) -> u16 {
            self.port
        }

        /// Adresse de retour donnée à OpenRouter (`localhost`, tout port
        /// permis par la documentation).
        #[must_use]
        pub fn callback_url(&self) -> String {
            format!("http://localhost:{}{CALLBACK_PATH}", self.port)
        }

        /// Attend la redirection (une requête sur [`CALLBACK_PATH`]),
        /// répond par une page statique et rend le code. Les autres
        /// requêtes (icône du site) reçoivent un 404.
        ///
        /// # Errors
        ///
        /// [`AssistantError::Login`] : délai écoulé, connexion annulée,
        /// refus, état différent.
        pub fn wait(
            &self,
            state: &str,
            timeout: Duration,
            cancel: &AtomicBool,
            lang: Language,
        ) -> Result<String, AssistantError> {
            let deadline = Instant::now().checked_add(timeout);
            loop {
                if cancel.load(Ordering::Relaxed) {
                    return Err(AssistantError::Login {
                        detail: "connexion annulée".to_owned(),
                    });
                }
                if deadline.is_none_or(|d| Instant::now() >= d) {
                    return Err(AssistantError::Login {
                        detail: "délai de connexion écoulé".to_owned(),
                    });
                }
                match self.listener.accept() {
                    Ok((stream, _)) => {
                        if let Some(result) = serve(stream, state, lang) {
                            return result;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    Err(e) => {
                        return Err(AssistantError::Login {
                            detail: format!("écouteur local : {e}"),
                        });
                    }
                }
            }
        }
    }

    /// Sert une connexion ; `None` si ce n'était pas la redirection.
    fn serve(mut stream: TcpStream, state: &str, lang: Language) -> Option<Result<String, AssistantError>> {
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
        let mut buf = Vec::new();
        let mut chunk = [0u8; 1024];
        while buf.len() < MAX_REQUEST && !buf.windows(4).any(|w| w == b"\r\n\r\n") {
            match stream.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => buf.extend(chunk.iter().take(n)),
            }
        }
        let text = String::from_utf8_lossy(&buf);
        let target = text
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("GET "))
            .and_then(|rest| rest.split(' ').next())
            .unwrap_or_default();
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        if path != CALLBACK_PATH {
            let _ =
                stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            return None;
        }
        let result = callback_code(query, state);
        let page = page(result.is_ok(), lang);
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
             Cache-Control: no-store\r\nConnection: close\r\n\r\n",
            page.len()
        );
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(page.as_bytes());
        let _ = stream.flush();
        Some(result)
    }

    /// Page de retour minimale, HTML statique sans script.
    fn page(ok: bool, lang: Language) -> String {
        let (title, body) = match (ok, lang) {
            (true, Language::Fr) => (
                "PlannerCraft — connecté",
                "Connexion à OpenRouter réussie. Vous pouvez fermer cet onglet et revenir à PlannerCraft.",
            ),
            (false, Language::Fr) => (
                "PlannerCraft — échec",
                "La connexion à OpenRouter n'a pas abouti. Revenez à PlannerCraft pour réessayer.",
            ),
            (true, Language::En) => (
                "PlannerCraft — connected",
                "Connected to OpenRouter. You can close this tab and return to PlannerCraft.",
            ),
            (false, Language::En) => (
                "PlannerCraft — failed",
                "Connecting to OpenRouter did not succeed. Return to PlannerCraft to try again.",
            ),
        };
        format!(
            "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title></head>\
             <body style=\"font-family:sans-serif;margin:3em\"><h1>{title}</h1><p>{body}</p></body></html>"
        )
    }

    /// Connexion du desktop en cours.
    #[derive(Clone, Debug)]
    pub struct LoginHandle {
        result: Arc<Mutex<Option<Result<String, AssistantError>>>>,
        cancel: Arc<AtomicBool>,
    }

    impl LoginHandle {
        /// Résultat (une fois) : la clé, ou l'erreur.
        #[must_use]
        pub fn take(&self) -> Option<Result<String, AssistantError>> {
            self.result.lock().ok().and_then(|mut r| r.take())
        }

        /// Abandonne la connexion (l'écouteur s'arrête sous 100 ms).
        pub fn cancel(&self) {
            self.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Parcours complet du desktop, dans un fil : écouteur, navigateur,
    /// attente de la redirection, échange du code.
    pub struct DesktopLogin;

    impl DesktopLogin {
        /// Lance la connexion ; le résultat arrive dans le
        /// [`LoginHandle`], `waker` est appelé à la fin.
        #[must_use]
        pub fn start(
            transport: SharedTransport,
            opener: Arc<dyn BrowserOpener>,
            lang: Language,
            waker: Option<Waker>,
        ) -> LoginHandle {
            let handle = LoginHandle {
                result: Arc::new(Mutex::new(None)),
                cancel: Arc::new(AtomicBool::new(false)),
            };
            let thread_handle = handle.clone();
            let thread_waker = waker;
            let spawned = std::thread::Builder::new()
                .name("plannercraft-assistant-login".to_owned())
                .spawn(move || {
                    let result = run(transport.as_ref(), opener.as_ref(), lang, &thread_handle.cancel);
                    if let Ok(mut slot) = thread_handle.result.lock() {
                        *slot = Some(result);
                    }
                    if let Some(w) = &thread_waker {
                        w();
                    }
                });
            if let Err(e) = spawned
                && let Ok(mut slot) = handle.result.lock()
            {
                *slot = Some(Err(AssistantError::Login {
                    detail: format!("fil de connexion non créé : {e}"),
                }));
            }
            handle
        }
    }

    fn run(
        transport: &dyn crate::http::Transport,
        opener: &dyn BrowserOpener,
        lang: Language,
        cancel: &AtomicBool,
    ) -> Result<String, AssistantError> {
        let pkce = Pkce::generate()?;
        let loopback = Loopback::bind()?;
        opener
            .open(&pkce.auth_url(&loopback.callback_url()))
            .map_err(|e| AssistantError::Login {
                detail: format!("navigateur non ouvert : {e}"),
            })?;
        let code = loopback.wait(&pkce.state, LOGIN_TIMEOUT, cancel, lang)?;
        let pending = PendingResponse::send(
            transport,
            crate::openrouter::exchange_request(&code, &pkce.verifier),
            None,
        );
        let deadline = Instant::now().checked_add(EXCHANGE_TIMEOUT);
        loop {
            if let Some(received) = pending.poll() {
                return key_of(received);
            }
            if cancel.load(Ordering::Relaxed) || deadline.is_none_or(|d| Instant::now() >= d) {
                return Err(AssistantError::Login {
                    detail: "échange du code sans réponse".to_owned(),
                });
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : un échec doit s'arrêter net"
    )]

    use std::io::{Read as _, Write as _};
    use std::net::TcpStream;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use plannercraft_automation::text::Language;
    use serde_json::json;

    use super::*;
    use crate::http::SharedTransport;
    use crate::platform::BrowserOpener;
    use crate::testing::ScriptedTransport;

    fn get(port: u16, target: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(stream, "GET {target} HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        let mut out = String::new();
        let _ = stream.read_to_string(&mut out);
        out
    }

    #[test]
    fn loopback_ignores_favicon_then_reads_code() {
        let loopback = Loopback::bind().unwrap();
        let port = loopback.port();
        assert!(loopback.callback_url().starts_with("http://localhost:"));
        let client = std::thread::spawn(move || {
            let not_found = get(port, "/favicon.ico");
            let page = get(port, "/callback?code=the-code&state=s1");
            (not_found, page)
        });
        let cancel = AtomicBool::new(false);
        let code = loopback
            .wait("s1", Duration::from_secs(10), &cancel, Language::Fr)
            .unwrap();
        assert_eq!(code, "the-code");
        let (not_found, page) = client.join().unwrap();
        assert!(not_found.starts_with("HTTP/1.1 404"));
        assert!(page.contains("Connexion à OpenRouter réussie"));
        assert!(!page.contains("<script"));
    }

    #[test]
    fn loopback_refuses_wrong_state_and_times_out() {
        let loopback = Loopback::bind().unwrap();
        let port = loopback.port();
        let client = std::thread::spawn(move || get(port, "/callback?code=c&state=evil"));
        let cancel = AtomicBool::new(false);
        assert!(
            loopback
                .wait("s1", Duration::from_secs(10), &cancel, Language::En)
                .is_err()
        );
        assert!(client.join().unwrap().contains("did not succeed"));
        let quiet = Loopback::bind().unwrap();
        assert!(
            quiet
                .wait("s", Duration::from_millis(150), &cancel, Language::Fr)
                .is_err()
        );
    }

    /// Navigateur simulé : suit la redirection comme OpenRouter le ferait.
    struct FakeBrowser(Mutex<Vec<String>>);
    impl BrowserOpener for FakeBrowser {
        fn open(&self, url: &str) -> Result<(), String> {
            self.0.lock().unwrap().push(url.to_owned());
            let query = url.split_once('?').unwrap().1;
            let callback = crate::pkce::query_param(query, "callback_url").unwrap();
            let state = crate::pkce::query_param(query, "state").unwrap();
            let port: u16 = callback
                .trim_start_matches("http://localhost:")
                .trim_end_matches("/callback")
                .parse()
                .unwrap();
            std::thread::spawn(move || get(port, &format!("/callback?code=oauth-code&state={state}")));
            Ok(())
        }
    }

    #[test]
    fn desktop_login_end_to_end_without_network() {
        let transport = Arc::new(ScriptedTransport::new(vec![vec![
            crate::http::HttpEvent::Head {
                status: 200,
                headers: vec![],
            },
            crate::http::HttpEvent::Chunk(json!({ "key": "sk-or-v1-test" }).to_string().into_bytes()),
            crate::http::HttpEvent::End,
        ]]));
        let browser = Arc::new(FakeBrowser(Mutex::new(Vec::new())));
        let shared: SharedTransport = transport.clone();
        let handle = DesktopLogin::start(shared, browser.clone(), Language::Fr, None);
        let mut result = None;
        for _ in 0..200 {
            if let Some(r) = handle.take() {
                result = Some(r);
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        assert_eq!(result.unwrap().unwrap(), "sk-or-v1-test");
        let requests = transport.requests();
        assert_eq!(requests.len(), 1);
        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(body["code"], "oauth-code");
        let url = browser.0.lock().unwrap()[0].clone();
        let challenge = crate::pkce::query_param(url.split_once('?').unwrap().1, "code_challenge").unwrap();
        assert_eq!(
            crate::pkce::challenge_of(body["code_verifier"].as_str().unwrap()),
            challenge
        );
    }
}
