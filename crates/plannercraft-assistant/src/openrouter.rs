//! Protocole d'OpenRouter (mode « ma propre clé », ADR-0024) : requête
//! Chat Completions en flux, en-têtes d'attribution, options de
//! confidentialité imposées, lecture des erreurs HTTP.
//!
//! Docs relues le 2026-11-06 : `https://openrouter.ai/docs/api-reference/streaming`
//! (commentaires `:`, `[DONE]`, `usage` dans le dernier morceau, erreur en
//! cours de flux), `https://openrouter.ai/docs/use-cases/usage-accounting`
//! (`usage` toujours inclus ; `usage.include` déprécié, donc absent).

use serde_json::{Value, json};

use crate::error::{AssistantError, truncate_message};
use crate::http::{HttpRequest, Method};

/// Adresse de l'API.
pub const API_BASE: &str = "https://openrouter.ai/api/v1";
/// Page d'attribution (`HTTP-Referer`).
pub const REFERER: &str = "https://github.com/DVP-Th30/plannercraft";
/// Titre d'attribution (`X-OpenRouter-Title`).
pub const TITLE: &str = "PlannerCraft";

/// Jetons de sortie au plus par tour.
pub const MAX_TOKENS: u32 = 2048;

/// Modèle de la liste fermée (identifiants vérifiés sur
/// `https://openrouter.ai/api/v1/models` le 2026-11-06).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Model {
    /// Identifiant OpenRouter.
    pub id: &'static str,
    /// Nom affiché.
    pub label: &'static str,
    /// Nom court (pied de message, en-tête du volet).
    pub short: &'static str,
}

/// Liste fermée des modèles proposés (ADR-0024 § 9) ; le premier est le
/// modèle par défaut.
pub const MODELS: &[Model] = &[
    Model {
        id: "anthropic/claude-sonnet-5.5",
        label: "Claude Sonnet 5.5",
        short: "Sonnet 5.5",
    },
    Model {
        id: "anthropic/claude-haiku-5.5",
        label: "Claude Haiku 5.5",
        short: "Haiku 5.5",
    },
    Model {
        id: "google/gemini-3.8-flash",
        label: "Gemini 3.8 Flash",
        short: "Gemini 3.8 Flash",
    },
];

/// Modèle par défaut.
pub const DEFAULT_MODEL: &str = "anthropic/claude-sonnet-5.5";

/// Modèle de la liste fermée d'identifiant `id`, sinon le modèle par
/// défaut (un état ancien ou modifié à la main ne choisit jamais un
/// modèle hors liste).
#[must_use]
pub fn model_or_default(id: &str) -> &'static Model {
    MODELS
        .iter()
        .find(|m| m.id == id)
        .or_else(|| MODELS.first())
        .unwrap_or(&Model {
            id: DEFAULT_MODEL,
            label: "Claude Sonnet 5.5",
            short: "Sonnet 5.5",
        })
}

/// En-têtes communs (attribution) et `Content-Type`.
fn base_headers() -> Vec<(String, String)> {
    vec![
        ("Content-Type".to_owned(), "application/json".to_owned()),
        ("HTTP-Referer".to_owned(), REFERER.to_owned()),
        ("X-OpenRouter-Title".to_owned(), TITLE.to_owned()),
    ]
}

/// Corps de la requête Chat Completions : flux, outils, bornes,
/// confidentialité (`provider.zdr`, `provider.data_collection: "deny"`).
/// `allow_tools` faux (dernier tour permis) : outils toujours décrits (un
/// historique qui contient des appels les exige chez certains
/// fournisseurs) mais `tool_choice: "none"`, la réponse est du texte.
#[must_use]
pub fn chat_body(
    model: &str,
    messages: &[Value],
    tools: &[Value],
    max_tokens: u32,
    allow_tools: bool,
) -> Value {
    let mut body = json!({
        "model": model,
        "messages": messages,
        "stream": true,
        "max_tokens": max_tokens,
        "provider": { "zdr": true, "data_collection": "deny" },
    });
    if !tools.is_empty()
        && let Value::Object(map) = &mut body
    {
        map.insert("tools".to_owned(), Value::Array(tools.to_vec()));
        let choice = if allow_tools { "auto" } else { "none" };
        map.insert("tool_choice".to_owned(), Value::String(choice.to_owned()));
    }
    body
}

/// Requête HTTP Chat Completions portant la clé `api_key`.
#[must_use]
pub fn chat_request(api_key: &str, body: &Value) -> HttpRequest {
    let mut headers = base_headers();
    headers.push(("Authorization".to_owned(), format!("Bearer {api_key}")));
    headers.push(("Accept".to_owned(), "text/event-stream".to_owned()));
    HttpRequest {
        method: Method::Post,
        url: format!("{API_BASE}/chat/completions"),
        headers,
        body: body.to_string().into_bytes(),
    }
}

/// Requête d'échange du code OAuth contre une clé
/// (`POST /api/v1/auth/keys`, `code`, `code_verifier`,
/// `code_challenge_method`).
#[must_use]
pub fn exchange_request(code: &str, verifier: &str) -> HttpRequest {
    let body = json!({
        "code": code,
        "code_verifier": verifier,
        "code_challenge_method": "S256",
    });
    HttpRequest {
        method: Method::Post,
        url: format!("{API_BASE}/auth/keys"),
        headers: base_headers(),
        body: body.to_string().into_bytes(),
    }
}

/// Lit la clé dans la réponse de l'échange (`{"key": "..."}`).
///
/// # Errors
///
/// L'erreur HTTP lue par [`http_error`] (403 : code expiré ou invalide),
/// ou [`AssistantError::Login`] pour un corps sans clé.
pub fn exchange_key(
    status: u16,
    headers: &[(String, String)],
    body: &[u8],
) -> Result<String, AssistantError> {
    if !(200..300).contains(&status) {
        return Err(match http_error(status, headers, body) {
            AssistantError::Http { status, message } => AssistantError::Login {
                detail: format!("{status} {message}"),
            },
            other => other,
        });
    }
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|v| v.get("key").and_then(Value::as_str).map(str::to_owned))
        .filter(|key| !key.trim().is_empty())
        .ok_or_else(|| AssistantError::Login {
            detail: "réponse sans clé".to_owned(),
        })
}

/// Erreur d'une réponse HTTP non 2xx : 401 clé refusée, 402 crédits
/// épuisés, 429 limite de débit (avec `Retry-After` en secondes s'il est
/// entier), sinon le message `error.message` du corps.
#[must_use]
pub fn http_error(status: u16, headers: &[(String, String)], body: &[u8]) -> AssistantError {
    match status {
        401 => AssistantError::Unauthorized,
        402 => AssistantError::PaymentRequired,
        429 => AssistantError::RateLimited {
            retry_after: headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("retry-after"))
                .and_then(|(_, value)| value.trim().parse::<u64>().ok()),
        },
        _ => {
            let message = serde_json::from_slice::<Value>(body)
                .ok()
                .and_then(|v| {
                    v.get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| String::from_utf8_lossy(body).trim().to_owned());
            AssistantError::Http {
                status,
                message: truncate_message(&message),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : un échec doit s'arrêter net"
    )]

    use super::*;

    #[test]
    fn body_has_privacy_stream_and_tools() {
        let tools = vec![json!({"type": "function"})];
        let body = chat_body("m", &[json!({"role": "user", "content": "x"})], &tools, 100, true);
        assert_eq!(
            body["provider"],
            json!({ "zdr": true, "data_collection": "deny" })
        );
        assert_eq!(body["stream"], json!(true));
        assert_eq!(body["max_tokens"], json!(100));
        assert_eq!(body["tool_choice"], json!("auto"));
        let without = chat_body("m", &[], &[], 100, true);
        assert!(without.get("tools").is_none());
        assert_eq!(
            chat_body("m", &[], &tools, 1, false)["tool_choice"],
            json!("none")
        );
    }

    #[test]
    fn request_headers_attribution_and_bearer() {
        let req = chat_request("sk-or-v1-secret", &json!({}));
        assert_eq!(req.header("http-referer"), Some(REFERER));
        assert_eq!(req.header("X-OpenRouter-Title"), Some("PlannerCraft"));
        assert_eq!(req.header("authorization"), Some("Bearer sk-or-v1-secret"));
        assert_eq!(req.url, "https://openrouter.ai/api/v1/chat/completions");
        // Jamais la clé dans le texte de débogage (journaux).
        assert!(!format!("{req:?}").contains("secret"));
    }

    #[test]
    fn http_errors_mapped() {
        assert_eq!(http_error(401, &[], b""), AssistantError::Unauthorized);
        assert_eq!(http_error(402, &[], b""), AssistantError::PaymentRequired);
        let headers = vec![("retry-after".to_owned(), "12".to_owned())];
        assert_eq!(
            http_error(429, &headers, b""),
            AssistantError::RateLimited {
                retry_after: Some(12)
            }
        );
        let http_date = vec![(
            "Retry-After".to_owned(),
            "Wed, 21 Oct 2026 07:28:00 GMT".to_owned(),
        )];
        assert_eq!(
            http_error(429, &http_date, b""),
            AssistantError::RateLimited { retry_after: None }
        );
        assert_eq!(
            http_error(
                400,
                &[],
                br#"{"error":{"code":400,"message":"Invalid model specified"}}"#
            ),
            AssistantError::Http {
                status: 400,
                message: "Invalid model specified".to_owned()
            }
        );
    }

    #[test]
    fn exchange_reads_key_or_login_error() {
        assert_eq!(
            exchange_key(200, &[], br#"{"key":"sk-or-1"}"#).unwrap(),
            "sk-or-1"
        );
        assert!(matches!(
            exchange_key(200, &[], b"{}"),
            Err(AssistantError::Login { .. })
        ));
        assert!(matches!(
            exchange_key(403, &[], br#"{"error":{"message":"expired"}}"#),
            Err(AssistantError::Login { .. })
        ));
        let req = exchange_request("c", "v");
        let body: Value = serde_json::from_slice(&req.body).unwrap();
        assert_eq!(
            body,
            json!({"code": "c", "code_verifier": "v", "code_challenge_method": "S256"})
        );
        assert!(req.header("authorization").is_none());
    }

    #[test]
    fn closed_model_list() {
        assert_eq!(
            model_or_default("google/gemini-3.8-flash").label,
            "Gemini 3.8 Flash"
        );
        assert_eq!(model_or_default("openai/whatever").id, DEFAULT_MODEL);
        assert_eq!(MODELS.first().unwrap().id, DEFAULT_MODEL);
    }
}
