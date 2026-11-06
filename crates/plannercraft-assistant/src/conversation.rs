//! Conversation avec le modèle : boucle d'outils bornée, sans fil propre.
//!
//! La conversation est une machine à états tenue par l'interface :
//! [`Conversation::send`] lance une requête (le transport ne bloque jamais
//! l'appelant), les évènements HTTP arrivent dans une file partagée
//! (réveil de l'interface par [`Waker`]), et [`Conversation::poll`],
//! appelé à chaque image, les lit : texte en flux, puis, si le modèle
//! demande des outils, exécution par l'hôte (lecture sur une copie de la
//! session, sur le fil de l'interface) et tour suivant. Même code sur le
//! desktop et le web.
//!
//! Bornes (règle 10) : [`MAX_ROUNDS`] tours par demande (le dernier sans
//! outil permis : `tool_choice: "none"`), réponse de [`MAX_RESPONSE_BYTES`]
//! octets au plus, file de [`MAX_QUEUED_EVENTS`] évènements, historique
//! élagué au-delà de [`MAX_HISTORY_BYTES`] octets par tours entiers.

use std::collections::VecDeque;
use std::fmt;
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use plannercraft_automation::text::Language;
use serde_json::{Value, json};

use crate::error::AssistantError;
use crate::http::{HttpEvent, Transport};
use crate::openrouter;
use crate::sse::SseParser;
use crate::stream::{StreamAccumulator, ToolCall, Usage};
use crate::tools::{self, ToolHost};

/// Tours au plus par demande (appels d'outils compris).
pub const MAX_ROUNDS: u32 = 6;
/// Octets reçus au plus par tour.
pub const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
/// Évènements en attente au plus dans la file.
pub const MAX_QUEUED_EVENTS: usize = 8192;
/// Historique envoyé au plus (octets de JSON), hors invite système.
pub const MAX_HISTORY_BYTES: usize = 256 * 1024;
/// Corps d'erreur HTTP gardé au plus.
const MAX_ERROR_BODY: usize = 64 * 1024;
/// Entrées au plus dans la transcription affichée.
pub const MAX_TRANSCRIPT: usize = 200;

/// Réveil de l'interface (egui : `Context::request_repaint`).
pub type Waker = Arc<dyn Fn() + Send + Sync>;

/// Réglages d'une demande. La clé n'apparaît jamais dans `Debug`.
#[derive(Clone)]
pub struct RequestConfig {
    /// Clé OpenRouter.
    pub api_key: String,
    /// Modèle (liste fermée).
    pub model: String,
    /// Langue de l'interface (invite système, messages des outils).
    pub lang: Language,
    /// Jetons de sortie au plus par tour.
    pub max_tokens: u32,
}

impl fmt::Debug for RequestConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestConfig")
            .field("api_key", &"<masquée>")
            .field("model", &self.model)
            .field("lang", &self.lang)
            .field("max_tokens", &self.max_tokens)
            .finish()
    }
}

/// Entrée de la transcription affichée.
#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    /// Message de l'utilisateur.
    User(String),
    /// Réponse du modèle (complétée au fil du flux).
    Assistant(String),
    /// Outils appelés pendant un tour (noms).
    Tools(Vec<String>),
    /// Erreur de la demande.
    Error(AssistantError),
    /// Demande arrêtée par l'utilisateur.
    Stopped,
    /// Jetons et coût de la demande (somme des tours).
    Cost(Usage),
}

/// Activité en cours (indicateur du volet).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    /// Rien en cours.
    Idle,
    /// Requête envoyée, rien reçu encore.
    Waiting,
    /// Le modèle lit le planning (tour qui suit des appels d'outils).
    ReadingPlan,
    /// Réponse en cours d'écriture.
    Writing,
}

type Inbox = Arc<Mutex<VecDeque<(u64, HttpEvent)>>>;

/// Tour en cours.
struct Running {
    request: u64,
    round: u32,
    config: RequestConfig,
    cancel: Arc<AtomicBool>,
    usage: Usage,
    status: Option<u16>,
    headers: Vec<(String, String)>,
    error_body: Vec<u8>,
    received: usize,
    sse: SseParser,
    acc: StreamAccumulator,
    /// Index dans la transcription de la réponse en cours, si du texte est
    /// arrivé.
    text_entry: Option<usize>,
}

/// Conversation.
pub struct Conversation {
    messages: Vec<Value>,
    transcript: Vec<Entry>,
    running: Option<Running>,
    inbox: Inbox,
    next_request: u64,
    waker: Option<Waker>,
    requests_sent: u64,
}

impl Default for Conversation {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Conversation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Conversation")
            .field("messages", &self.messages.len())
            .field("transcript", &self.transcript.len())
            .field("busy", &self.busy())
            .finish_non_exhaustive()
    }
}

impl Conversation {
    /// Conversation vide.
    #[must_use]
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            transcript: Vec::new(),
            running: None,
            inbox: Arc::new(Mutex::new(VecDeque::new())),
            next_request: 1,
            waker: None,
            requests_sent: 0,
        }
    }

    /// Réveil appelé à chaque évènement reçu.
    pub fn set_waker(&mut self, waker: Waker) {
        self.waker = Some(waker);
    }

    /// Un réveil est posé.
    #[must_use]
    pub const fn has_waker(&self) -> bool {
        self.waker.is_some()
    }

    /// Transcription affichée.
    #[must_use]
    pub fn transcript(&self) -> &[Entry] {
        &self.transcript
    }

    /// Requêtes HTTP envoyées depuis la création (tests).
    #[must_use]
    pub const fn requests_sent(&self) -> u64 {
        self.requests_sent
    }

    /// Une demande est en cours.
    #[must_use]
    pub const fn busy(&self) -> bool {
        self.running.is_some()
    }

    /// Activité en cours.
    #[must_use]
    pub fn activity(&self) -> Activity {
        match &self.running {
            None => Activity::Idle,
            Some(r) if r.text_entry.is_some() => Activity::Writing,
            Some(r) if r.round > 0 => Activity::ReadingPlan,
            Some(_) => Activity::Waiting,
        }
    }

    /// Oublie la conversation (arrête la demande en cours).
    pub fn clear(&mut self) {
        if let Some(running) = self.running.take() {
            running.cancel.store(true, Ordering::Relaxed);
        }
        self.messages.clear();
        self.transcript.clear();
    }

    /// Envoie un message de l'utilisateur.
    ///
    /// # Errors
    ///
    /// [`AssistantError::Busy`] si une demande est en cours,
    /// [`AssistantError::NoKey`] sans clé (rien n'est envoyé).
    pub fn send(
        &mut self,
        text: &str,
        config: RequestConfig,
        transport: &dyn Transport,
    ) -> Result<(), AssistantError> {
        if self.running.is_some() {
            return Err(AssistantError::Busy);
        }
        if config.api_key.trim().is_empty() {
            return Err(AssistantError::NoKey);
        }
        let text = text.trim();
        if text.is_empty() {
            return Ok(());
        }
        self.push_entry(Entry::User(text.to_owned()));
        self.messages.push(json!({ "role": "user", "content": text }));
        self.start_round(0, config, Usage::default(), transport);
        Ok(())
    }

    /// Arrête la demande en cours : la connexion est interrompue au prochain
    /// évènement, les évènements tardifs sont ignorés.
    pub fn stop(&mut self) {
        let Some(running) = self.running.take() else {
            return;
        };
        running.cancel.store(true, Ordering::Relaxed);
        let text = running.acc.text();
        if !text.is_empty() {
            self.messages
                .push(json!({ "role": "assistant", "content": text }));
        }
        self.push_entry(Entry::Stopped);
        if let Ok(mut inbox) = self.inbox.lock() {
            inbox.clear();
        }
    }

    /// Lit les évènements reçus ; exécute les outils demandés et lance le
    /// tour suivant. Rend vrai si la transcription ou l'état a changé.
    pub fn poll(&mut self, host: &mut dyn ToolHost, transport: &dyn Transport) -> bool {
        let Some(request) = self.running.as_ref().map(|r| r.request) else {
            return false;
        };
        let events: Vec<HttpEvent> = match self.inbox.lock() {
            Ok(mut inbox) => {
                let all: Vec<(u64, HttpEvent)> = inbox.drain(..).collect();
                all.into_iter()
                    .filter(|(id, _)| *id == request)
                    .map(|(_, e)| e)
                    .collect()
            }
            Err(_) => vec![HttpEvent::Failed("file des évènements inutilisable".to_owned())],
        };
        if events.is_empty() {
            return false;
        }
        for event in events {
            if self.running.is_none() {
                break;
            }
            match event {
                HttpEvent::Head { status, headers } => {
                    if let Some(r) = self.running.as_mut() {
                        r.status = Some(status);
                        r.headers = headers;
                    }
                }
                HttpEvent::Chunk(bytes) => self.chunk(&bytes),
                HttpEvent::End => self.finish_round(host, transport),
                HttpEvent::Failed(detail) => self.fail(AssistantError::Network {
                    detail: crate::error::truncate_message(&detail),
                }),
            }
        }
        true
    }

    fn push_entry(&mut self, entry: Entry) {
        self.transcript.push(entry);
        if self.transcript.len() > MAX_TRANSCRIPT {
            let extra = self.transcript.len().saturating_sub(MAX_TRANSCRIPT);
            self.transcript.drain(..extra);
            if let Some(r) = self.running.as_mut() {
                r.text_entry = r.text_entry.and_then(|i| i.checked_sub(extra));
            }
        }
    }

    fn chunk(&mut self, bytes: &[u8]) {
        let Some(r) = self.running.as_mut() else {
            return;
        };
        r.received = r.received.saturating_add(bytes.len());
        if r.received > MAX_RESPONSE_BYTES {
            self.fail(AssistantError::Protocol {
                detail: "réponse trop longue".to_owned(),
            });
            return;
        }
        if !r.status.is_some_and(|s| (200..300).contains(&s)) {
            let room = MAX_ERROR_BODY.saturating_sub(r.error_body.len());
            r.error_body.extend(bytes.iter().take(room));
            return;
        }
        let Ok(events) = r.sse.feed(bytes) else {
            self.fail(AssistantError::Protocol {
                detail: "ligne SSE trop longue".to_owned(),
            });
            return;
        };
        for event in events {
            if !self.data(&event.data) {
                return;
            }
        }
    }

    /// Une charge `data` ; faux si la demande a échoué.
    fn data(&mut self, data: &str) -> bool {
        let Some(r) = self.running.as_mut() else {
            return false;
        };
        match r.acc.push(data) {
            Ok(added) if added.is_empty() => true,
            Ok(added) => {
                let entry = r.text_entry;
                if let Some(Entry::Assistant(text)) = entry.and_then(|i| self.transcript.get_mut(i)) {
                    text.push_str(&added);
                } else {
                    self.push_entry(Entry::Assistant(added));
                    let index = self.transcript.len().checked_sub(1);
                    if let Some(r) = self.running.as_mut() {
                        r.text_entry = index;
                    }
                }
                true
            }
            Err(error) => {
                self.fail(error);
                false
            }
        }
    }

    fn fail(&mut self, error: AssistantError) {
        let Some(running) = self.running.take() else {
            return;
        };
        running.cancel.store(true, Ordering::Relaxed);
        let usage = running.usage.plus(running.acc.usage().unwrap_or_default());
        self.push_entry(Entry::Error(error));
        if usage != Usage::default() {
            self.push_entry(Entry::Cost(usage));
        }
    }

    fn finish_round(&mut self, host: &mut dyn ToolHost, transport: &dyn Transport) {
        let Some(mut r) = self.running.take() else {
            return;
        };
        let status = r.status.unwrap_or(0);
        if !(200..300).contains(&status) {
            self.running = Some(r);
            let (headers, body) = self
                .running
                .as_ref()
                .map(|r| (r.headers.clone(), r.error_body.clone()))
                .unwrap_or_default();
            self.fail(openrouter::http_error(status, &headers, &body));
            return;
        }
        let tail = r.sse.finish();
        self.running = Some(r);
        match tail {
            Ok(Some(event)) => {
                if !self.data(&event.data) {
                    return;
                }
            }
            Ok(None) => {}
            Err(_) => {
                self.fail(AssistantError::Protocol {
                    detail: "ligne SSE trop longue".to_owned(),
                });
                return;
            }
        }
        let Some(r) = self.running.take() else {
            return;
        };
        if !r.acc.complete() {
            self.running = Some(r);
            self.fail(AssistantError::Network {
                detail: "réponse interrompue".to_owned(),
            });
            return;
        }
        let usage = r.usage.plus(r.acc.usage().unwrap_or_default());
        let text = r.acc.text().to_owned();
        let calls = r.acc.tool_calls();
        if calls.is_empty() {
            self.messages
                .push(json!({ "role": "assistant", "content": text }));
            self.push_entry(Entry::Cost(usage));
            return;
        }
        let next = r.round.saturating_add(1);
        if next >= MAX_ROUNDS {
            self.running = Some(r);
            self.fail(AssistantError::TooManyRounds);
            return;
        }
        self.run_tools(&calls, &text, host, r.config.lang);
        self.start_round(next, r.config, usage, transport);
    }

    fn run_tools(&mut self, calls: &[ToolCall], text: &str, host: &mut dyn ToolHost, lang: Language) {
        let wire: Vec<Value> = calls
            .iter()
            .map(|c| {
                json!({
                    "id": c.id,
                    "type": "function",
                    "function": { "name": c.name, "arguments": c.arguments },
                })
            })
            .collect();
        let content = if text.is_empty() {
            Value::Null
        } else {
            Value::String(text.to_owned())
        };
        self.messages
            .push(json!({ "role": "assistant", "content": content, "tool_calls": wire }));
        self.push_entry(Entry::Tools(calls.iter().map(|c| c.name.clone()).collect()));
        for call in calls {
            let result = tools::execute(host, call, lang);
            self.messages
                .push(json!({ "role": "tool", "tool_call_id": call.id, "content": result }));
        }
    }

    /// Élague les tours les plus anciens tant que l'historique dépasse
    /// [`MAX_HISTORY_BYTES`] (coupure sur un message de l'utilisateur : un
    /// appel d'outil n'est jamais séparé de son résultat).
    fn trim_history(&mut self) {
        loop {
            let size: usize = self.messages.iter().map(|m| m.to_string().len()).sum();
            if size <= MAX_HISTORY_BYTES {
                return;
            }
            let next_user = self
                .messages
                .iter()
                .enumerate()
                .skip(1)
                .find(|(_, m)| m.get("role").and_then(Value::as_str) == Some("user"))
                .map(|(i, _)| i);
            match next_user {
                Some(i) => {
                    self.messages.drain(..i);
                }
                None => return,
            }
        }
    }

    fn start_round(&mut self, round: u32, config: RequestConfig, usage: Usage, transport: &dyn Transport) {
        self.trim_history();
        let mut messages = Vec::with_capacity(self.messages.len().saturating_add(1));
        messages.push(json!({
            "role": "system",
            "content": plannercraft_automation::agent::assistant_instructions(config.lang),
        }));
        messages.extend(self.messages.iter().cloned());
        let allow_tools = round.saturating_add(1) < MAX_ROUNDS;
        let body = openrouter::chat_body(
            &config.model,
            &messages,
            &tools::definitions(config.lang),
            config.max_tokens,
            allow_tools,
        );
        let request = openrouter::chat_request(&config.api_key, &body);
        let id = self.next_request;
        self.next_request = self.next_request.saturating_add(1);
        let cancel = Arc::new(AtomicBool::new(false));
        self.running = Some(Running {
            request: id,
            round,
            config,
            cancel: Arc::clone(&cancel),
            usage,
            status: None,
            headers: Vec::new(),
            error_body: Vec::new(),
            received: 0,
            sse: SseParser::new(),
            acc: StreamAccumulator::new(),
            text_entry: None,
        });
        let inbox = Arc::clone(&self.inbox);
        let waker = self.waker.clone();
        let sink = Box::new(move |event: HttpEvent| {
            if cancel.load(Ordering::Relaxed) {
                return ControlFlow::Break(());
            }
            let flow = match inbox.lock() {
                Ok(mut queue) if queue.len() < MAX_QUEUED_EVENTS => {
                    queue.push_back((id, event));
                    ControlFlow::Continue(())
                }
                Ok(mut queue) => {
                    queue.push_back((id, HttpEvent::Failed("file des évènements pleine".to_owned())));
                    ControlFlow::Break(())
                }
                Err(_) => ControlFlow::Break(()),
            };
            if let Some(waker) = &waker {
                waker();
            }
            flow
        });
        self.requests_sent = self.requests_sent.saturating_add(1);
        transport.send(request, sink);
    }
}
