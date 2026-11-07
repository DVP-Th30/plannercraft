//! Transport scripté pour les tests (crate et interface) : réponses SSE
//! écrites à la main d'après la documentation d'OpenRouter, livrées sans
//! réseau, d'un coup ou pas à pas (affichage en flux, bouton Arrêter).
//! Lot A4 : [`FakeRelay`], faux relais de PlannerCraft qui sert le contrat
//! d'API v1 (`docs/plans/relay.md` § 2) sans réseau.

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

/// Morceaux d'un raisonnement (`reasoning_details`, format d'Anthropic),
/// un morceau par mot, signature au dernier.
#[must_use]
pub fn reasoning_chunks(text: &str) -> Vec<Value> {
    let words: Vec<&str> = text.split_inclusive(' ').collect();
    let last = words.len().saturating_sub(1);
    words
        .iter()
        .enumerate()
        .map(|(i, word)| {
            let mut detail = json!({
                "type": "reasoning.text", "text": word, "format": "anthropic-claude-v1", "index": 0,
            });
            if i == last
                && let Value::Object(map) = &mut detail
            {
                map.insert("signature".to_owned(), Value::String("sig-test".to_owned()));
            }
            json!({ "choices": [{ "index": 0, "delta": { "reasoning": word, "reasoning_details": [detail] } }] })
        })
        .collect()
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

/// Code de connexion accepté par [`FakeRelay`] (8 caractères de
/// l'alphabet du relais, sans 0/O, 1/I/L, U).
pub const FAKE_CODE: &str = "K7QF3MZD";
/// Jeton de lien magique accepté par [`FakeRelay`] (`/login?token=…`).
pub const FAKE_LINK_TOKEN: &str = "ml_fake-link-token-0123456789";
/// Jeton de session rendu par [`FakeRelay`].
pub const FAKE_SESSION: &str = "pcs_fake-session-token-abcdef0123456789";
/// Adresse de paiement rendue par [`FakeRelay`] (suivie du pack).
pub const FAKE_CHECKOUT_URL: &str = "https://checkout.stripe.com/c/pay/cs_test_";

#[derive(Default)]
#[allow(clippy::struct_excessive_bools, reason = "état d'un faux serveur de test")]
struct RelayState {
    /// Adresses qui ont demandé un lien.
    mails: Vec<String>,
    /// Session valide.
    signed_in: bool,
    /// Session expirée (`auth.expired`).
    expired: bool,
    /// Session révoquée par `logout`.
    revoked: bool,
    /// Refus `credits.insufficient` au prochain chat.
    insufficient: bool,
    /// Solde, centimes.
    balance: i64,
    /// Réponses de chat scriptées : morceaux et coût (centimes).
    chats: VecDeque<(Vec<Value>, i64)>,
    /// Lignes de consommation.
    usage: Vec<Value>,
}

/// Faux relais de PlannerCraft (lot A4) : connexion par lien magique et
/// code ([`FAKE_CODE`], [`FAKE_LINK_TOKEN`]), session ([`FAKE_SESSION`]),
/// `/v1/me`, `/v1/usage`, achat (`/v1/credits/checkout`), chat relayé
/// avec le commentaire `: plannercraft-cost <centimes> <solde>` et débit
/// du solde, erreurs `{ code, message }` du contrat. Aucun réseau ;
/// requêtes gardées pour les vérifications.
#[derive(Default)]
pub struct FakeRelay {
    state: Mutex<RelayState>,
    requests: Mutex<Vec<HttpRequest>>,
}

fn json_events(status: u16, body: &Value, headers: &[(&str, &str)]) -> Vec<HttpEvent> {
    let mut head: Vec<(String, String)> = vec![("content-type".to_owned(), "application/json".to_owned())];
    head.extend(
        headers
            .iter()
            .map(|(k, v)| ((*k).to_ascii_lowercase(), (*v).to_owned())),
    );
    vec![
        HttpEvent::Head {
            status,
            headers: head,
        },
        HttpEvent::Chunk(body.to_string().into_bytes()),
        HttpEvent::End,
    ]
}

fn relay_error(status: u16, code: &str, message: &str) -> Vec<HttpEvent> {
    let headers: &[(&str, &str)] = if code == "rate.limited" {
        &[("retry-after", "12")]
    } else {
        &[]
    };
    json_events(status, &json!({ "code": code, "message": message }), headers)
}

fn no_content() -> Vec<HttpEvent> {
    vec![
        HttpEvent::Head {
            status: 204,
            headers: Vec::new(),
        },
        HttpEvent::End,
    ]
}

/// Modèles de la liste fermée avec leurs prix affichés (marge comprise).
#[must_use]
pub fn fake_relay_models() -> Value {
    json!([
        { "id": "anthropic/claude-sonnet-5.5", "label": "Claude Sonnet 5.5",
          "input_cents_per_mtok": 400, "output_cents_per_mtok": 2000 },
        { "id": "anthropic/claude-haiku-5.5", "label": "Claude Haiku 5.5",
          "input_cents_per_mtok": 20, "output_cents_per_mtok": 100 },
        { "id": "google/gemini-3.8-flash", "label": "Gemini 3.8 Flash",
          "input_cents_per_mtok": 165, "output_cents_per_mtok": 825 },
    ])
}

impl FakeRelay {
    /// Relais dont le compte aura `balance_cents` à la connexion.
    #[must_use]
    pub fn new(balance_cents: i64) -> Self {
        let relay = Self::default();
        if let Ok(mut s) = relay.state.lock() {
            s.balance = balance_cents;
        }
        relay
    }

    fn with_state<R>(&self, f: impl FnOnce(&mut RelayState) -> R) -> Option<R> {
        self.state.lock().ok().map(|mut s| f(&mut s))
    }

    /// Ajoute une réponse de chat (morceaux de [`text_chunks`],
    /// [`tool_call_chunks`]…) débitée de `cost_cents`.
    pub fn push_chat(&self, chunks: Vec<Value>, cost_cents: i64) {
        self.with_state(|s| s.chats.push_back((chunks, cost_cents)));
    }

    /// La session expire : toute route authentifiée rend `auth.expired`.
    pub fn expire_session(&self) {
        self.with_state(|s| s.expired = true);
    }

    /// Le prochain chat est refusé `credits.insufficient` tant que vrai.
    pub fn set_insufficient(&self, on: bool) {
        self.with_state(|s| s.insufficient = on);
    }

    /// Solde du compte (achat de crédits simulé).
    pub fn set_balance(&self, cents: i64) {
        self.with_state(|s| s.balance = cents);
    }

    /// Solde du compte.
    #[must_use]
    pub fn balance(&self) -> i64 {
        self.with_state(|s| s.balance).unwrap_or(0)
    }

    /// Adresses qui ont demandé un lien.
    #[must_use]
    pub fn mails(&self) -> Vec<String> {
        self.with_state(|s| s.mails.clone()).unwrap_or_default()
    }

    /// La session a été révoquée par `POST /v1/auth/logout`.
    #[must_use]
    pub fn revoked(&self) -> bool {
        self.with_state(|s| s.revoked).unwrap_or(false)
    }

    /// Requêtes reçues.
    #[must_use]
    pub fn requests(&self) -> Vec<HttpRequest> {
        self.requests.lock().map(|r| r.clone()).unwrap_or_default()
    }

    /// Chemins des requêtes reçues (`POST /v1/me`…).
    #[must_use]
    pub fn routes(&self) -> Vec<String> {
        self.requests()
            .iter()
            .map(|r| format!("{:?} {}", r.method, route_of(&r.url)))
            .collect()
    }

    fn session_error(s: &RelayState, request: &HttpRequest) -> Option<Vec<HttpEvent>> {
        let expected = format!("Bearer {FAKE_SESSION}");
        if request.header("authorization") != Some(expected.as_str()) {
            return Some(relay_error(401, "auth.required", "Connexion requise."));
        }
        if !s.signed_in || s.revoked {
            return Some(relay_error(401, "auth.invalid", "Session inconnue ou révoquée."));
        }
        if s.expired {
            return Some(relay_error(
                401,
                "auth.expired",
                "Session expirée, reconnectez-vous.",
            ));
        }
        None
    }

    fn me(s: &RelayState) -> Value {
        json!({
            "email": s.mails.last().cloned().unwrap_or_else(|| "marie@exemple.fr".to_owned()),
            "balance_cents": s.balance,
            "currency": "EUR",
            "models": fake_relay_models(),
            "low_balance": s.balance < 100,
        })
    }

    #[allow(
        clippy::too_many_lines,
        reason = "table des routes du contrat, une branche par route"
    )]
    fn respond(&self, request: &HttpRequest) -> Vec<HttpEvent> {
        let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
        let route = route_of(&request.url);
        let Ok(mut s) = self.state.lock() else {
            return vec![HttpEvent::Failed("faux relais inutilisable".to_owned())];
        };
        match route.as_str() {
            "/v1/auth/magic-link" => {
                let email = body.get("email").and_then(Value::as_str).unwrap_or_default();
                s.mails.push(email.trim().to_lowercase());
                no_content()
            }
            "/v1/auth/exchange" => {
                let by_code = body.get("code").and_then(Value::as_str).is_some_and(|code| {
                    let code: String = code
                        .chars()
                        .filter(|c| !c.is_whitespace() && *c != '-')
                        .collect::<String>()
                        .to_uppercase();
                    let email = body
                        .get("email")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .trim()
                        .to_lowercase();
                    code == FAKE_CODE && s.mails.contains(&email)
                });
                let by_link = body.get("token").and_then(Value::as_str) == Some(FAKE_LINK_TOKEN);
                if !(by_code || by_link) {
                    return relay_error(401, "auth.invalid", "Code incorrect.");
                }
                s.signed_in = true;
                s.expired = false;
                s.revoked = false;
                let email = s
                    .mails
                    .last()
                    .cloned()
                    .unwrap_or_else(|| "marie@exemple.fr".to_owned());
                json_events(
                    200,
                    &json!({
                        "session_token": FAKE_SESSION,
                        "expires_at": "2027-02-05T10:00:00Z",
                        "account": { "email": email, "balance_cents": s.balance },
                    }),
                    &[],
                )
            }
            "/v1/auth/logout" => {
                if let Some(error) = Self::session_error(&s, request) {
                    return error;
                }
                s.revoked = true;
                no_content()
            }
            "/v1/me" => {
                Self::session_error(&s, request).unwrap_or_else(|| json_events(200, &Self::me(&s), &[]))
            }
            "/v1/usage" => {
                if let Some(error) = Self::session_error(&s, request) {
                    return error;
                }
                let total: i64 = s
                    .usage
                    .iter()
                    .filter_map(|i| i.get("cost_cents").and_then(Value::as_i64))
                    .sum();
                let items: Vec<Value> = s.usage.iter().rev().cloned().collect();
                json_events(200, &json!({ "items": items, "total_cents": total }), &[])
            }
            "/v1/credits/checkout" => {
                if let Some(error) = Self::session_error(&s, request) {
                    return error;
                }
                match body.get("pack").and_then(Value::as_str) {
                    Some(pack @ ("p5" | "p20" | "p50")) => {
                        json_events(200, &json!({ "url": format!("{FAKE_CHECKOUT_URL}{pack}") }), &[])
                    }
                    _ => relay_error(400, "request.invalid", "Pack inconnu."),
                }
            }
            "/v1/assistant/chat" => {
                if let Some(error) = Self::session_error(&s, request) {
                    return error;
                }
                if s.insufficient {
                    return relay_error(
                        402,
                        "credits.insufficient",
                        "Crédits insuffisants pour cette demande.",
                    );
                }
                if body.get("provider").is_some() {
                    return relay_error(400, "request.invalid", "Clé non permise : provider.");
                }
                let Some((chunks, cost)) = s.chats.pop_front() else {
                    return vec![HttpEvent::Failed("aucune réponse prévue".to_owned())];
                };
                s.balance = s.balance.saturating_sub(cost).max(0);
                let at = format!("2026-11-07T10:{:02}:00Z", s.usage.len() % 60);
                let model = body.get("model").cloned().unwrap_or(Value::Null);
                s.usage.push(json!({
                    "at": at, "model": model, "prompt_tokens": 1200, "completion_tokens": 40,
                    "cached_tokens": 0, "cost_cents": cost,
                }));
                let mut events = sse_response(&chunks);
                let done = events.len().saturating_sub(2);
                events.insert(
                    done,
                    HttpEvent::Chunk(format!(": plannercraft-cost {cost} {}\n\n", s.balance).into_bytes()),
                );
                events
            }
            _ => relay_error(404, "not_found", "Route inconnue."),
        }
    }
}

/// Chemin d'une adresse (`https://hôte/v1/me?x` → `/v1/me`).
fn route_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let path = rest.find('/').and_then(|i| rest.get(i..)).unwrap_or("/");
    path.split('?').next().unwrap_or(path).to_owned()
}

impl Transport for FakeRelay {
    fn send(&self, request: HttpRequest, sink: EventSink) {
        let events = self.respond(&request);
        if let Ok(mut r) = self.requests.lock() {
            r.push(request);
        }
        for event in events {
            if sink(event) == ControlFlow::Break(()) {
                break;
            }
        }
    }
}
