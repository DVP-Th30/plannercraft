//! Relais de PlannerCraft (mode « Compte PlannerCraft », lot A4, ADR-0024
//! et ADR-0025) : contrat d'API v1 du cahier `docs/plans/relay.md` § 2.
//!
//! - Connexion par lien magique : `POST /v1/auth/magic-link { email }`
//!   (toujours `204`), puis `POST /v1/auth/exchange` avec `{ email, code }`
//!   (code à 8 caractères de l'e-mail) ou `{ token }` (lien
//!   `…/login?token=…`, web) → jeton de session opaque ;
//!   `POST /v1/auth/logout` le révoque.
//! - `GET /v1/me` : adresse, solde en centimes d'euro, modèles proposés et
//!   leurs prix, alerte de solde bas ; `GET /v1/usage` : consommation (30
//!   derniers jours par défaut, bornes calculées par le relais : aucune
//!   horloge lue ici).
//! - `POST /v1/assistant/chat` : même requête Chat Completions que le mode
//!   « ma clé », **sans** `provider` (imposé par le relais), avec
//!   `Authorization: Bearer <jeton>`, `X-PlannerCraft-Request` (UUID v4 par
//!   demande) et `X-PlannerCraft-Turn` (numéro de complétion dans la
//!   demande, à partir de 1). Flux SSE d'OpenRouter, plus le commentaire
//!   `: plannercraft-cost <centimes> <solde>` avant `data: [DONE]`
//!   ([`crate::sse::parse_relay_cost`]). Les trailers
//!   `X-PlannerCraft-Cost-Cents` ne sont pas lus (ni `ehttp` ni `fetch` ne
//!   les rendent).
//! - `POST /v1/credits/checkout { pack }` → `{ url }` (Stripe Checkout).
//! - Erreurs `{ code, message }` ([`api_error`]).
//!
//! Le jeton de session n'apparaît jamais dans un `Debug` ni dans un
//! journal ; aucune clé OpenRouter n'est utilisée dans ce mode.

use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::{AssistantError, truncate_message};
use crate::http::{HttpRequest, Method};

/// Adresse du relais de l'éditeur.
pub const RELAY_URL: &str = "https://api.plannercraft.app";
/// Variable d'environnement qui remplace [`RELAY_URL`] sur le desktop
/// (essais contre un relais local : `http://127.0.0.1:8080`).
pub const RELAY_URL_ENV: &str = "PLANNERCRAFT_RELAY_URL";
/// Solde sous lequel l'alerte « solde bas » s'affiche (centimes), en plus
/// de l'indication `low_balance` du relais.
pub const LOW_BALANCE_CENTS: i64 = 50;
/// Longueur au plus d'une adresse e-mail saisie (caractères).
pub const MAX_EMAIL_CHARS: usize = 254;
/// Longueur au plus d'un code saisi (caractères : 8, plus espaces et
/// tirets tolérés par le relais).
pub const MAX_CODE_CHARS: usize = 16;

/// Pack de crédits proposé à l'achat (cahier § 2 : `p5`, `p20`, `p50`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pack {
    /// Identifiant du relais.
    pub id: &'static str,
    /// Prix affiché, en euros.
    pub euros: u32,
}

/// Packs proposés.
pub const PACKS: &[Pack] = &[
    Pack { id: "p5", euros: 5 },
    Pack { id: "p20", euros: 20 },
    Pack { id: "p50", euros: 50 },
];

/// Pack d'identifiant `id`.
#[must_use]
pub fn pack(id: &str) -> Option<&'static Pack> {
    PACKS.iter().find(|p| p.id == id)
}

/// Adresse du relais : [`RELAY_URL_ENV`] si elle est posée et non vide
/// (desktop seulement), sinon [`RELAY_URL`] ; sans `/` final.
#[must_use]
pub fn base_url() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    let configured = std::env::var(RELAY_URL_ENV).ok();
    #[cfg(target_arch = "wasm32")]
    let configured: Option<String> = None;
    normalize_base(configured.as_deref().unwrap_or(RELAY_URL))
}

/// `url` sans espaces ni `/` final ; vide : [`RELAY_URL`].
#[must_use]
pub fn normalize_base(url: &str) -> String {
    let url = url.trim().trim_end_matches('/');
    if url.is_empty() {
        RELAY_URL.to_owned()
    } else {
        url.to_owned()
    }
}

/// Adresse e-mail plausible (forme seulement ; le relais vérifie).
#[must_use]
pub fn plausible_email(email: &str) -> bool {
    let email = email.trim();
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !domain.contains('@')
        && email.chars().count() <= MAX_EMAIL_CHARS
        && !email.chars().any(char::is_whitespace)
}

/// Code saisi plausible : 8 lettres ou chiffres une fois retirés espaces
/// et tirets.
#[must_use]
pub fn plausible_code(code: &str) -> bool {
    let kept: Vec<char> = code.chars().filter(|c| !c.is_whitespace() && *c != '-').collect();
    kept.len() == 8 && kept.iter().all(char::is_ascii_alphanumeric)
}

fn json_headers() -> Vec<(String, String)> {
    vec![
        ("Content-Type".to_owned(), "application/json".to_owned()),
        ("Accept".to_owned(), "application/json".to_owned()),
    ]
}

fn bearer(headers: &mut Vec<(String, String)>, token: &str) {
    headers.push(("Authorization".to_owned(), format!("Bearer {token}")));
}

fn post_json(base: &str, path: &str, body: &Value) -> HttpRequest {
    HttpRequest {
        method: Method::Post,
        url: format!("{base}{path}"),
        headers: json_headers(),
        body: body.to_string().into_bytes(),
    }
}

/// `POST /v1/auth/magic-link { email }`.
#[must_use]
pub fn magic_link_request(base: &str, email: &str) -> HttpRequest {
    post_json(base, "/v1/auth/magic-link", &json!({ "email": email.trim() }))
}

/// `POST /v1/auth/exchange { email, code }` (code de l'e-mail).
#[must_use]
pub fn exchange_code_request(base: &str, email: &str, code: &str) -> HttpRequest {
    post_json(
        base,
        "/v1/auth/exchange",
        &json!({ "email": email.trim(), "code": code.trim() }),
    )
}

/// `POST /v1/auth/exchange { token }` (lien `…/login?token=…`, web).
#[must_use]
pub fn exchange_token_request(base: &str, token: &str) -> HttpRequest {
    post_json(base, "/v1/auth/exchange", &json!({ "token": token.trim() }))
}

/// `POST /v1/auth/logout` (révoque le jeton).
#[must_use]
pub fn logout_request(base: &str, session: &str) -> HttpRequest {
    let mut request = post_json(base, "/v1/auth/logout", &json!({}));
    request.body.clear();
    request.headers.retain(|(name, _)| name != "Content-Type");
    bearer(&mut request.headers, session);
    request
}

fn get(base: &str, path: &str, session: &str) -> HttpRequest {
    let mut headers = vec![("Accept".to_owned(), "application/json".to_owned())];
    bearer(&mut headers, session);
    HttpRequest {
        method: Method::Get,
        url: format!("{base}{path}"),
        headers,
        body: Vec::new(),
    }
}

/// `GET /v1/me`.
#[must_use]
pub fn me_request(base: &str, session: &str) -> HttpRequest {
    get(base, "/v1/me", session)
}

/// `GET /v1/usage` (30 derniers jours, bornes du relais).
#[must_use]
pub fn usage_request(base: &str, session: &str) -> HttpRequest {
    get(base, "/v1/usage", session)
}

/// `POST /v1/credits/checkout { pack }`.
#[must_use]
pub fn checkout_request(base: &str, session: &str, pack: &str) -> HttpRequest {
    let mut request = post_json(base, "/v1/credits/checkout", &json!({ "pack": pack }));
    bearer(&mut request.headers, session);
    request
}

/// `POST /v1/assistant/chat` : `body` est la requête du mode « ma clé »
/// ([`crate::openrouter::chat_body`]), dont `provider` est retiré (le
/// relais refuse toute clé inconnue et impose lui-même la
/// confidentialité). `turn` part de 1.
#[must_use]
pub fn chat_request(base: &str, session: &str, request_id: &str, turn: u32, body: &Value) -> HttpRequest {
    let mut body = body.clone();
    if let Value::Object(map) = &mut body {
        map.remove("provider");
    }
    let mut headers = vec![
        ("Content-Type".to_owned(), "application/json".to_owned()),
        ("Accept".to_owned(), "text/event-stream".to_owned()),
        ("X-PlannerCraft-Request".to_owned(), request_id.to_owned()),
        ("X-PlannerCraft-Turn".to_owned(), turn.to_string()),
    ];
    bearer(&mut headers, session);
    HttpRequest {
        method: Method::Post,
        url: format!("{base}/v1/assistant/chat"),
        headers,
        body: body.to_string().into_bytes(),
    }
}

/// UUID v4 (RFC 9562 § 5.4) à partir de 16 octets d'aléa.
#[must_use]
pub fn uuid_v4(mut bytes: [u8; 16]) -> String {
    if let Some(b) = bytes.get_mut(6) {
        *b = (*b & 0x0f) | 0x40;
    }
    if let Some(b) = bytes.get_mut(8) {
        *b = (*b & 0x3f) | 0x80;
    }
    let hex: String = bytes.iter().fold(String::with_capacity(32), |mut out, b| {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
        out
    });
    let part = |from: usize, to: usize| hex.get(from..to).unwrap_or_default();
    format!(
        "{}-{}-{}-{}-{}",
        part(0, 8),
        part(8, 12),
        part(12, 16),
        part(16, 20),
        part(20, 32)
    )
}

/// Identifiant neuf d'une demande (`X-PlannerCraft-Request`).
///
/// # Errors
///
/// Aléa du système indisponible ([`crate::platform::random`]).
pub fn new_request_id() -> Result<String, AssistantError> {
    let mut bytes = [0u8; 16];
    crate::platform::random(&mut bytes)?;
    Ok(uuid_v4(bytes))
}

/// Modèle proposé par le relais (`/v1/me`), prix marge comprise.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct RelayModel {
    /// Identifiant (celui d'OpenRouter).
    pub id: String,
    /// Nom affiché.
    pub label: String,
    /// Prix d'entrée, centimes d'euro par million de jetons.
    #[serde(default)]
    pub input_cents_per_mtok: i64,
    /// Prix de sortie, centimes d'euro par million de jetons.
    #[serde(default)]
    pub output_cents_per_mtok: i64,
}

/// Compte (`GET /v1/me`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Me {
    /// Adresse du compte.
    pub email: String,
    /// Solde, centimes d'euro.
    pub balance_cents: i64,
    /// Devise (`EUR`).
    #[serde(default)]
    pub currency: String,
    /// Modèles proposés.
    #[serde(default)]
    pub models: Vec<RelayModel>,
    /// Solde bas selon le relais.
    #[serde(default)]
    pub low_balance: bool,
}

impl Me {
    /// Alerte « solde bas » : indication du relais ou solde sous
    /// [`LOW_BALANCE_CENTS`].
    #[must_use]
    pub const fn low(&self) -> bool {
        self.low_balance || self.balance_cents < LOW_BALANCE_CENTS
    }
}

/// Session obtenue par l'échange. `Debug` masque le jeton.
#[derive(Clone, PartialEq, Eq)]
pub struct Session {
    /// Jeton opaque (`Authorization: Bearer`).
    pub token: String,
    /// Adresse du compte.
    pub email: String,
    /// Solde à la connexion, centimes.
    pub balance_cents: i64,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("token", &"<masqué>")
            .field("email", &self.email)
            .field("balance_cents", &self.balance_cents)
            .finish()
    }
}

/// Ligne de consommation (`GET /v1/usage`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct UsageItem {
    /// Horodatage RFC 3339 (UTC).
    pub at: String,
    /// Modèle.
    pub model: String,
    /// Jetons d'entrée.
    #[serde(default)]
    pub prompt_tokens: i64,
    /// Jetons de sortie.
    #[serde(default)]
    pub completion_tokens: i64,
    /// Jetons lus dans le cache.
    #[serde(default)]
    pub cached_tokens: i64,
    /// Coût débité, centimes.
    #[serde(default)]
    pub cost_cents: i64,
}

/// Consommation (`GET /v1/usage`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct UsageReport {
    /// Lignes, plus récentes d'abord.
    #[serde(default)]
    pub items: Vec<UsageItem>,
    /// Total de la période, centimes.
    #[serde(default)]
    pub total_cents: i64,
}

/// Erreur d'une réponse non 2xx du relais (`{ code, message }`) :
/// `auth.*` ⇒ [`AssistantError::SessionEnded`], `credits.insufficient` ⇒
/// [`AssistantError::CreditsExhausted`], `rate.limited` ⇒
/// [`AssistantError::RateLimited`] (`Retry-After`), `turn.limit` ⇒
/// [`AssistantError::TooManyRounds`], autre code ⇒
/// [`AssistantError::Relay`] ; corps illisible ⇒ [`AssistantError::Http`].
#[must_use]
pub fn api_error(status: u16, headers: &[(String, String)], body: &[u8]) -> AssistantError {
    let parsed: Option<(String, String)> = serde_json::from_slice::<Value>(body).ok().and_then(|v| {
        let code = v.get("code").and_then(Value::as_str)?.to_owned();
        let message = v
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        Some((code, message))
    });
    let retry_after = || {
        headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("retry-after"))
            .and_then(|(_, value)| value.trim().parse::<u64>().ok())
    };
    let Some((code, message)) = parsed else {
        return match status {
            401 => AssistantError::SessionEnded { expired: false },
            402 => AssistantError::CreditsExhausted,
            429 => AssistantError::RateLimited {
                retry_after: retry_after(),
            },
            _ => AssistantError::Http {
                status,
                message: truncate_message(String::from_utf8_lossy(body).trim()),
            },
        };
    };
    match code.as_str() {
        "auth.expired" => AssistantError::SessionEnded { expired: true },
        "auth.invalid" | "auth.required" => AssistantError::SessionEnded { expired: false },
        "credits.insufficient" => AssistantError::CreditsExhausted,
        "rate.limited" => AssistantError::RateLimited {
            retry_after: retry_after(),
        },
        "turn.limit" => AssistantError::TooManyRounds,
        _ => AssistantError::Relay {
            relay_code: truncate_message(&code),
            message: truncate_message(&message),
        },
    }
}

/// Erreur d'un échange de code ou de lien : un refus `auth.*` n'est pas
/// une session terminée mais un code ou un lien refusé (« demandez-en un
/// nouveau ») : [`AssistantError::Relay`] garde le code et le message du
/// relais.
#[must_use]
pub fn exchange_error(status: u16, headers: &[(String, String)], body: &[u8]) -> AssistantError {
    match api_error(status, headers, body) {
        AssistantError::SessionEnded { expired } => AssistantError::Relay {
            relay_code: if expired { "auth.expired" } else { "auth.invalid" }.to_owned(),
            message: serde_json::from_slice::<Value>(body)
                .ok()
                .and_then(|v| v.get("message").and_then(Value::as_str).map(truncate_message))
                .unwrap_or_default(),
        },
        other => other,
    }
}

fn ok_json<T: for<'de> Deserialize<'de>>(
    status: u16,
    headers: &[(String, String)],
    body: &[u8],
    what: &str,
) -> Result<T, AssistantError> {
    if !(200..300).contains(&status) {
        return Err(api_error(status, headers, body));
    }
    serde_json::from_slice(body).map_err(|e| AssistantError::Protocol {
        detail: truncate_message(&format!("{what} illisible : {e}")),
    })
}

/// Lit la réponse de `/v1/auth/magic-link` (`204` attendu).
///
/// # Errors
///
/// Voir [`api_error`].
pub fn read_magic_link(status: u16, headers: &[(String, String)], body: &[u8]) -> Result<(), AssistantError> {
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(api_error(status, headers, body))
    }
}

/// Lit la session dans la réponse de `/v1/auth/exchange`.
///
/// # Errors
///
/// [`exchange_error`] pour un refus, [`AssistantError::Protocol`] pour un
/// corps sans jeton.
pub fn read_exchange(
    status: u16,
    headers: &[(String, String)],
    body: &[u8],
) -> Result<Session, AssistantError> {
    if !(200..300).contains(&status) {
        return Err(exchange_error(status, headers, body));
    }
    let value: Value = serde_json::from_slice(body).map_err(|_| AssistantError::Protocol {
        detail: "réponse de connexion illisible".to_owned(),
    })?;
    let token = value
        .get("session_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| AssistantError::Protocol {
            detail: "réponse de connexion sans jeton".to_owned(),
        })?;
    let account = value.get("account");
    Ok(Session {
        token: token.to_owned(),
        email: account
            .and_then(|a| a.get("email"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        balance_cents: account
            .and_then(|a| a.get("balance_cents"))
            .and_then(Value::as_i64)
            .unwrap_or(0),
    })
}

/// Lit `/v1/me`.
///
/// # Errors
///
/// Voir [`api_error`] ; corps illisible : [`AssistantError::Protocol`].
pub fn read_me(status: u16, headers: &[(String, String)], body: &[u8]) -> Result<Me, AssistantError> {
    ok_json(status, headers, body, "compte")
}

/// Lit `/v1/usage`.
///
/// # Errors
///
/// Voir [`api_error`] ; corps illisible : [`AssistantError::Protocol`].
pub fn read_usage(
    status: u16,
    headers: &[(String, String)],
    body: &[u8],
) -> Result<UsageReport, AssistantError> {
    ok_json(status, headers, body, "consommation")
}

/// Lit l'adresse de paiement de `/v1/credits/checkout` ; seule une adresse
/// `https://` est acceptée (elle est ouverte dans le navigateur).
///
/// # Errors
///
/// Voir [`api_error`] ; adresse absente ou non `https` :
/// [`AssistantError::Protocol`].
pub fn read_checkout(
    status: u16,
    headers: &[(String, String)],
    body: &[u8],
) -> Result<String, AssistantError> {
    let value: Value = ok_json(status, headers, body, "paiement")?;
    value
        .get("url")
        .and_then(Value::as_str)
        .filter(|u| u.starts_with("https://") && !u.chars().any(char::is_whitespace))
        .map(str::to_owned)
        .ok_or_else(|| AssistantError::Protocol {
            detail: "adresse de paiement absente ou non sécurisée".to_owned(),
        })
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : un échec doit s'arrêter net"
    )]

    use super::*;

    fn body_of(request: &HttpRequest) -> Value {
        serde_json::from_slice(&request.body).unwrap()
    }

    #[test]
    fn requests_follow_the_contract() {
        let base = normalize_base(" http://127.0.0.1:8080/ ");
        assert_eq!(base, "http://127.0.0.1:8080");
        assert_eq!(normalize_base(""), RELAY_URL);
        let link = magic_link_request(&base, " a@b.fr ");
        assert_eq!(link.url, "http://127.0.0.1:8080/v1/auth/magic-link");
        assert_eq!(body_of(&link), json!({ "email": "a@b.fr" }));
        assert!(link.header("authorization").is_none());
        let code = exchange_code_request(&base, "a@b.fr", "ABCD-EFGH");
        assert_eq!(body_of(&code), json!({ "email": "a@b.fr", "code": "ABCD-EFGH" }));
        let token = exchange_token_request(&base, "ml_x");
        assert_eq!(body_of(&token), json!({ "token": "ml_x" }));
        let me = me_request(&base, "sess-secret");
        assert_eq!(me.method, Method::Get);
        assert_eq!(me.header("authorization"), Some("Bearer sess-secret"));
        let out = logout_request(&base, "sess-secret");
        assert_eq!(out.body, Vec::<u8>::new());
        assert_eq!(out.url, "http://127.0.0.1:8080/v1/auth/logout");
        let buy = checkout_request(&base, "sess-secret", "p20");
        assert_eq!(body_of(&buy), json!({ "pack": "p20" }));
        assert_eq!(usage_request(&base, "s").url, "http://127.0.0.1:8080/v1/usage");
        // Jamais le jeton dans un journal.
        for r in [&me, &out, &buy] {
            assert!(!format!("{r:?}").contains("secret"));
        }
    }

    #[test]
    fn chat_without_provider_with_request_headers() {
        let body = crate::openrouter::chat_body(
            "anthropic/claude-haiku-5.5",
            &[json!({"role":"user","content":"x"})],
            &[json!({"type":"function"})],
            100,
            true,
        );
        assert!(body.get("provider").is_some());
        let req = chat_request(
            "https://r.test",
            "sess",
            "0b6e8f5c-1f7a-4c3e-9a2b-1234567890ab",
            2,
            &body,
        );
        assert_eq!(req.url, "https://r.test/v1/assistant/chat");
        let sent = body_of(&req);
        assert!(sent.get("provider").is_none());
        let keys: Vec<&str> = sent.as_object().unwrap().keys().map(String::as_str).collect();
        for key in keys {
            assert!(
                [
                    "model",
                    "messages",
                    "stream",
                    "max_tokens",
                    "reasoning",
                    "tools",
                    "tool_choice"
                ]
                .contains(&key),
                "clé refusée par le relais : {key}"
            );
        }
        assert_eq!(req.header("x-plannercraft-turn"), Some("2"));
        assert_eq!(
            req.header("X-PlannerCraft-Request"),
            Some("0b6e8f5c-1f7a-4c3e-9a2b-1234567890ab")
        );
        assert_eq!(req.header("authorization"), Some("Bearer sess"));
        assert_eq!(req.header("accept"), Some("text/event-stream"));
    }

    #[test]
    fn uuid_v4_format() {
        let id = uuid_v4([0xff; 16]);
        assert_eq!(id, "ffffffff-ffff-4fff-bfff-ffffffffffff");
        let id = uuid_v4([0; 16]);
        assert_eq!(id, "00000000-0000-4000-8000-000000000000");
        let fresh = new_request_id().unwrap();
        assert_eq!(fresh.len(), 36);
        assert_eq!(fresh.as_bytes()[14], b'4');
        assert_ne!(fresh, new_request_id().unwrap());
    }

    #[test]
    fn errors_mapped_to_user_kinds() {
        let err = |status: u16, code: &str| {
            api_error(
                status,
                &[("retry-after".to_owned(), "7".to_owned())],
                json!({ "code": code, "message": "m" }).to_string().as_bytes(),
            )
        };
        assert_eq!(
            err(401, "auth.expired"),
            AssistantError::SessionEnded { expired: true }
        );
        assert_eq!(
            err(401, "auth.invalid"),
            AssistantError::SessionEnded { expired: false }
        );
        assert_eq!(
            err(403, "auth.invalid"),
            AssistantError::SessionEnded { expired: false }
        );
        assert_eq!(
            err(401, "auth.required"),
            AssistantError::SessionEnded { expired: false }
        );
        assert_eq!(err(402, "credits.insufficient"), AssistantError::CreditsExhausted);
        assert_eq!(
            err(429, "rate.limited"),
            AssistantError::RateLimited { retry_after: Some(7) }
        );
        assert_eq!(err(400, "turn.limit"), AssistantError::TooManyRounds);
        assert_eq!(
            err(400, "model.not_allowed"),
            AssistantError::Relay {
                relay_code: "model.not_allowed".to_owned(),
                message: "m".to_owned()
            }
        );
        assert!(matches!(
            api_error(502, &[], b"<html>bad gateway</html>"),
            AssistantError::Http { status: 502, .. }
        ));
        // Échange refusé : code refusé, pas session terminée.
        assert!(matches!(
            exchange_error(401, &[], br#"{"code":"auth.invalid","message":"Code incorrect."}"#),
            AssistantError::Relay { ref relay_code, ref message } if relay_code == "auth.invalid" && message == "Code incorrect."
        ));
    }

    #[test]
    fn responses_read() {
        let session = read_exchange(
            200,
            &[],
            br#"{"session_token":"sess-secret","expires_at":"2027-02-05T10:00:00Z","account":{"email":"a@b.fr","balance_cents":50}}"#,
        )
        .unwrap();
        assert_eq!(session.token, "sess-secret");
        assert_eq!(session.balance_cents, 50);
        assert!(!format!("{session:?}").contains("secret"));
        assert!(read_exchange(200, &[], br#"{"account":{}}"#).is_err());
        let me = read_me(
            200,
            &[],
            br#"{"email":"a@b.fr","balance_cents":437,"currency":"EUR","models":[{"id":"anthropic/claude-sonnet-5.5","label":"Claude Sonnet 5.5","input_cents_per_mtok":400,"output_cents_per_mtok":2000}],"low_balance":false}"#,
        )
        .unwrap();
        assert_eq!(me.balance_cents, 437);
        assert_eq!(me.models[0].output_cents_per_mtok, 2000);
        assert!(!me.low());
        assert!(
            Me {
                balance_cents: 49,
                ..me.clone()
            }
            .low()
        );
        let usage = read_usage(
            200,
            &[],
            br#"{"items":[{"at":"2026-11-07T10:00:00Z","model":"m","prompt_tokens":1,"completion_tokens":2,"cached_tokens":0,"cost_cents":3}],"total_cents":3}"#,
        )
        .unwrap();
        assert_eq!(usage.total_cents, 3);
        assert_eq!(
            read_checkout(200, &[], br#"{"url":"https://checkout.stripe.com/c/pay/x"}"#).unwrap(),
            "https://checkout.stripe.com/c/pay/x"
        );
        assert!(read_checkout(200, &[], br#"{"url":"javascript:alert(1)"}"#).is_err());
        assert!(read_magic_link(204, &[], b"").is_ok());
        assert_eq!(
            read_me(401, &[], br#"{"code":"auth.expired","message":"x"}"#),
            Err(AssistantError::SessionEnded { expired: true })
        );
    }

    #[test]
    fn email_and_code_forms() {
        assert!(plausible_email("marie@exemple.fr"));
        assert!(!plausible_email("marie@exemple"));
        assert!(!plausible_email("marie exemple.fr"));
        assert!(!plausible_email("@exemple.fr"));
        assert!(plausible_code("ABCD-EFGH"));
        assert!(plausible_code("abcd efgh"));
        assert!(!plausible_code("ABC"));
        assert_eq!(pack("p20").map(|p| p.euros), Some(20));
        assert!(pack("p7").is_none());
    }
}
