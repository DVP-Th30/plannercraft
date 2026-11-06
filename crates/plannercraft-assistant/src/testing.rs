//! Transport scripté pour les tests (crate et interface) : réponses SSE
//! écrites à la main d'après la documentation d'OpenRouter, livrées sans
//! réseau, d'un coup ou pas à pas (affichage en flux, bouton Arrêter).

use std::collections::VecDeque;
use std::ops::ControlFlow;
use std::sync::Mutex;

use serde_json::{Value, json};

use crate::http::{EventSink, HttpEvent, HttpRequest, Transport};

/// Transport scripté : chaque requête reçoit la réponse suivante de la
/// liste ; sans réponse prévue, un échec réseau.
#[derive(Default)]
pub struct ScriptedTransport {
    responses: Mutex<VecDeque<Vec<HttpEvent>>>,
    requests: Mutex<Vec<HttpRequest>>,
    /// Mode pas à pas : les évènements restent en attente jusqu'à
    /// [`ScriptedTransport::step`].
    manual: bool,
    pending: Mutex<VecDeque<(EventSink, VecDeque<HttpEvent>)>>,
}

impl ScriptedTransport {
    /// Transport qui livre chaque réponse en entier dès l'envoi.
    #[must_use]
    pub fn new(responses: Vec<Vec<HttpEvent>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            ..Self::default()
        }
    }

    /// Transport pas à pas.
    #[must_use]
    pub fn manual(responses: Vec<Vec<HttpEvent>>) -> Self {
        Self {
            manual: true,
            ..Self::new(responses)
        }
    }

    /// Ajoute une réponse à la fin de la liste.
    pub fn push(&self, response: Vec<HttpEvent>) {
        if let Ok(mut r) = self.responses.lock() {
            r.push_back(response);
        }
    }

    /// Requêtes reçues.
    #[must_use]
    pub fn requests(&self) -> Vec<HttpRequest> {
        self.requests.lock().map(|r| r.clone()).unwrap_or_default()
    }

    /// Livre au plus `n` évènements de la réponse en attente la plus
    /// ancienne ; rend le nombre livré.
    pub fn step(&self, n: usize) -> usize {
        let Ok(mut pending) = self.pending.lock() else {
            return 0;
        };
        let mut delivered = 0usize;
        while delivered < n {
            let Some((sink, events)) = pending.front_mut() else {
                break;
            };
            let Some(event) = events.pop_front() else {
                pending.pop_front();
                continue;
            };
            delivered = delivered.saturating_add(1);
            if sink(event).is_break() {
                pending.pop_front();
            }
        }
        delivered
    }

    /// Évènements encore en attente.
    #[must_use]
    pub fn pending_events(&self) -> usize {
        self.pending
            .lock()
            .map_or(0, |p| p.iter().map(|(_, e)| e.len()).sum())
    }
}

impl Transport for ScriptedTransport {
    fn send(&self, request: HttpRequest, sink: EventSink) {
        if let Ok(mut r) = self.requests.lock() {
            r.push(request);
        }
        let next = self.responses.lock().ok().and_then(|mut r| r.pop_front());
        let events = next.unwrap_or_else(|| vec![HttpEvent::Failed("aucune réponse prévue".to_owned())]);
        if self.manual {
            if let Ok(mut pending) = self.pending.lock() {
                pending.push_back((sink, events.into()));
            }
            return;
        }
        for event in events {
            if sink(event) == ControlFlow::Break(()) {
                break;
            }
        }
    }
}

/// Réponse HTTP 200 en SSE : une ligne de commentaire d'OpenRouter, un
/// évènement par morceau, `[DONE]`, fin.
#[must_use]
pub fn sse_response(chunks: &[Value]) -> Vec<HttpEvent> {
    let mut events = vec![
        HttpEvent::Head {
            status: 200,
            headers: vec![("content-type".to_owned(), "text/event-stream".to_owned())],
        },
        HttpEvent::Chunk(b": OPENROUTER PROCESSING\n\n".to_vec()),
    ];
    for chunk in chunks {
        events.push(HttpEvent::Chunk(format!("data: {chunk}\n\n").into_bytes()));
    }
    events.push(HttpEvent::Chunk(b"data: [DONE]\n\n".to_vec()));
    events.push(HttpEvent::End);
    events
}

/// Morceaux d'une réponse texte (un morceau par mot), puis le morceau
/// d'`usage` (coût `cost`).
#[must_use]
pub fn text_chunks(text: &str, cost: f64) -> Vec<Value> {
    let mut chunks: Vec<Value> = text
        .split_inclusive(' ')
        .map(|word| json!({ "choices": [{ "index": 0, "delta": { "content": word } }] }))
        .collect();
    chunks.push(json!({ "choices": [{ "index": 0, "delta": {}, "finish_reason": "stop" }] }));
    chunks.push(json!({
        "choices": [{ "index": 0, "delta": {}, "finish_reason": "stop" }],
        "usage": { "prompt_tokens": 1200, "completion_tokens": 40, "total_tokens": 1240, "cost": cost },
    }));
    chunks
}

/// Morceaux d'un appel d'outil, arguments coupés en deux morceaux.
#[must_use]
pub fn tool_call_chunks(id: &str, name: &str, arguments: &str) -> Vec<Value> {
    let half = arguments.len() / 2;
    let split = (half..=arguments.len())
        .find(|i| arguments.is_char_boundary(*i))
        .unwrap_or(0);
    let (a, b) = arguments.split_at(split);
    vec![
        json!({ "choices": [{ "index": 0, "delta": { "tool_calls": [
            { "index": 0, "id": id, "type": "function", "function": { "name": name, "arguments": a } }
        ] } }] }),
        json!({ "choices": [{ "index": 0, "delta": { "tool_calls": [
            { "index": 0, "function": { "arguments": b } }
        ] } }] }),
        json!({ "choices": [{ "index": 0, "delta": {}, "finish_reason": "tool_calls" }],
                "usage": { "prompt_tokens": 1000, "completion_tokens": 20, "cost": 0.001 } }),
    ]
}

/// Réponse d'erreur HTTP (statut, en-têtes, corps JSON d'OpenRouter).
#[must_use]
pub fn error_response(status: u16, headers: &[(&str, &str)], message: &str) -> Vec<HttpEvent> {
    let body = json!({ "error": { "code": status, "message": message } }).to_string();
    vec![
        HttpEvent::Head {
            status,
            headers: headers
                .iter()
                .map(|(k, v)| ((*k).to_ascii_lowercase(), (*v).to_owned()))
                .collect(),
        },
        HttpEvent::Chunk(body.into_bytes()),
        HttpEvent::End,
    ]
}
