//! Transport HTTP injectable : [`EhttpTransport`] dans l'application
//! (ureq dans un fil sur le desktop, `fetch` du navigateur en wasm), un
//! transport scripté dans les tests ([`crate::testing`]) : aucune requête
//! réseau réelle en CI.
//!
//! Le transport ne connaît ni OpenRouter ni SSE : il envoie une requête et
//! rend sa réponse en évènements (en-tête, morceaux, fin ou échec) à un
//! puits qui peut l'interrompre ([`ControlFlow::Break`] : bouton Arrêter).

use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;

/// Méthode HTTP utilisée par l'assistant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    /// GET.
    Get,
    /// POST.
    Post,
}

/// Requête.
#[derive(Clone, PartialEq, Eq)]
pub struct HttpRequest {
    /// Méthode.
    pub method: Method,
    /// Adresse complète.
    pub url: String,
    /// En-têtes (nom, valeur).
    pub headers: Vec<(String, String)>,
    /// Corps.
    pub body: Vec<u8>,
}

impl std::fmt::Debug for HttpRequest {
    /// Sans les valeurs des en-têtes ni le corps : la clé (en-tête
    /// `Authorization`) ne va jamais dans un journal.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.headers.iter().map(|(name, _)| name.as_str()).collect();
        f.debug_struct("HttpRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &names)
            .field("body_bytes", &self.body.len())
            .finish()
    }
}

impl HttpRequest {
    /// Valeur de l'en-tête `name` (casse ignorée).
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// Évènement de réponse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HttpEvent {
    /// Statut et en-têtes (une fois, en premier).
    Head {
        /// Statut HTTP.
        status: u16,
        /// En-têtes, noms en minuscules.
        headers: Vec<(String, String)>,
    },
    /// Morceau du corps.
    Chunk(Vec<u8>),
    /// Fin normale du corps.
    End,
    /// Échec (réseau, TLS, délai) : la réponse s'arrête là.
    Failed(String),
}

/// Puits des évènements : [`ControlFlow::Break`] interrompt la requête.
pub type EventSink = Box<dyn Fn(HttpEvent) -> ControlFlow<()> + Send>;

/// Transport.
pub trait Transport: Send + Sync {
    /// Envoie `request` ; les évènements arrivent à `sink`, depuis un autre
    /// fil (desktop) ou depuis l'exécuteur du navigateur (web). Ne bloque
    /// jamais l'appelant.
    fn send(&self, request: HttpRequest, sink: EventSink);
}

/// Transport partagé.
pub type SharedTransport = Arc<dyn Transport>;

/// Durée au plus de réception d'une réponse (un modèle lent qui écrit une
/// longue réponse reste sous ce délai).
pub const RESPONSE_TIMEOUT: Duration = Duration::from_secs(300);

/// Transport réel par `ehttp`.
#[derive(Clone, Copy, Debug, Default)]
pub struct EhttpTransport;

impl Transport for EhttpTransport {
    fn send(&self, request: HttpRequest, sink: EventSink) {
        let mut headers = ehttp::Headers::default();
        for (name, value) in &request.headers {
            headers.insert(name, value);
        }
        let method = match request.method {
            Method::Get => ehttp::Method::GET,
            Method::Post => ehttp::Method::POST,
        };
        let mut req = ehttp::Request::get(&request.url);
        req.method = method;
        req.headers = headers;
        req.body = request.body;
        req.timeout = Some(RESPONSE_TIMEOUT);
        let on_data = move |part: ehttp::Result<ehttp::streaming::Part>| match part {
            Err(error) => {
                let _ignored = sink(HttpEvent::Failed(error));
                ControlFlow::Break(())
            }
            Ok(ehttp::streaming::Part::Response(head)) => sink(HttpEvent::Head {
                status: head.status,
                headers: head
                    .headers
                    .into_iter()
                    .map(|(k, v)| (k.to_ascii_lowercase(), v))
                    .collect(),
            }),
            Ok(ehttp::streaming::Part::Chunk(chunk)) if chunk.is_empty() => {
                let _ignored = sink(HttpEvent::End);
                ControlFlow::Break(())
            }
            Ok(ehttp::streaming::Part::Chunk(chunk)) => sink(HttpEvent::Chunk(chunk)),
        };
        spawn_fetch(req, on_data);
    }
}

/// Desktop : fil nommé à nous (son échec de création est signalé, pas une
/// panique comme le fil interne d'ehttp).
#[cfg(not(target_arch = "wasm32"))]
fn spawn_fetch(
    req: ehttp::Request,
    on_data: impl Fn(ehttp::Result<ehttp::streaming::Part>) -> ControlFlow<()> + Send + 'static,
) {
    let on_data: Box<dyn Fn(ehttp::Result<ehttp::streaming::Part>) -> ControlFlow<()> + Send> =
        Box::new(on_data);
    let shared = Arc::new(std::sync::Mutex::new(Some((req, on_data))));
    let in_thread = Arc::clone(&shared);
    let spawned = std::thread::Builder::new()
        .name("plannercraft-assistant-http".to_owned())
        .spawn(move || {
            let taken = in_thread.lock().ok().and_then(|mut slot| slot.take());
            if let Some((req, on_data)) = taken {
                ehttp::streaming::fetch_streaming_blocking(req, on_data);
            }
        });
    if let Err(error) = spawned {
        let taken = shared.lock().ok().and_then(|mut slot| slot.take());
        if let Some((_, on_data)) = taken {
            let _ignored = on_data(Err(format!("fil réseau non créé : {error}")));
        }
    }
}

/// Web : `fetch` sur l'exécuteur du navigateur.
#[cfg(target_arch = "wasm32")]
fn spawn_fetch(
    req: ehttp::Request,
    on_data: impl Fn(ehttp::Result<ehttp::streaming::Part>) -> ControlFlow<()> + Send + 'static,
) {
    ehttp::streaming::fetch(req, on_data);
}
