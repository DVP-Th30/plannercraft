//! Assemblage d'une réponse Chat Completions reçue en flux (morceaux
//! `chat.completion.chunk` d'OpenRouter, format compatible `OpenAI`).
//!
//! - texte : `choices[0].delta.content`, concaténé ;
//! - appels d'outils : `choices[0].delta.tool_calls[]`, rangés par `index`,
//!   `id` et `function.name` au premier morceau, `function.arguments`
//!   (texte JSON) en plusieurs morceaux à concaténer ;
//! - `finish_reason` (`stop`, `tool_calls`, `length`, `error`…) ;
//! - `usage` dans le dernier morceau avant `[DONE]` (qui a un `choices`
//!   au `delta` vide) : jetons et coût (`cost`, en crédits OpenRouter) ;
//! - erreur en cours de flux : `error` au sommet du morceau (statut HTTP
//!   resté 200).
//!
//! Règle 10 : textes, arguments et nombre d'appels bornés ; un morceau
//! illisible est une erreur, jamais une panique.

use serde_json::Value;

use crate::error::{AssistantError, truncate_message};

/// Texte de réponse gardé au plus (octets) ; la suite est ignorée.
pub const MAX_TEXT_BYTES: usize = 64 * 1024;
/// Arguments d'un appel d'outil au plus (octets).
pub const MAX_ARGUMENT_BYTES: usize = 64 * 1024;
/// Appels d'outils au plus dans une réponse.
pub const MAX_TOOL_CALLS: usize = 16;

/// Jetons et coût d'une réponse (`usage`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Usage {
    /// Jetons d'entrée.
    pub prompt_tokens: u64,
    /// Jetons de sortie.
    pub completion_tokens: u64,
    /// Coût en crédits OpenRouter (dollars US), s'il est donné.
    pub cost: Option<f64>,
}

impl Usage {
    /// Somme de deux relevés (tours d'une même demande).
    #[must_use]
    pub fn plus(self, other: Self) -> Self {
        Self {
            prompt_tokens: self.prompt_tokens.saturating_add(other.prompt_tokens),
            completion_tokens: self.completion_tokens.saturating_add(other.completion_tokens),
            cost: match (self.cost, other.cost) {
                (Some(a), Some(b)) => Some(a + b),
                (a, b) => a.or(b),
            },
        }
    }
}

/// Appel d'outil demandé par le modèle.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolCall {
    /// Identifiant de l'appel (renvoyé avec le résultat).
    pub id: String,
    /// Nom de l'outil.
    pub name: String,
    /// Arguments, texte JSON (peut être invalide : le modèle en est averti).
    pub arguments: String,
}

/// Réponse assemblée.
#[derive(Debug, Default)]
pub struct StreamAccumulator {
    text: String,
    text_truncated: bool,
    calls: Vec<(u64, ToolCall)>,
    finish_reason: Option<String>,
    usage: Option<Usage>,
    done: bool,
}

impl StreamAccumulator {
    /// Réponse vide.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Lit la charge `data` d'un évènement SSE ; rend le texte ajouté.
    ///
    /// # Errors
    ///
    /// [`AssistantError::Provider`] pour une erreur annoncée dans le flux,
    /// [`AssistantError::Protocol`] pour un morceau illisible ou un appel
    /// d'outil au-delà des bornes.
    pub fn push(&mut self, data: &str) -> Result<String, AssistantError> {
        let data = data.trim();
        if data == "[DONE]" {
            self.done = true;
            return Ok(String::new());
        }
        if data.is_empty() {
            return Ok(String::new());
        }
        let chunk: Value = serde_json::from_str(data).map_err(|e| AssistantError::Protocol {
            detail: truncate_message(&format!("morceau JSON illisible : {e}")),
        })?;
        if let Some(error) = chunk.get("error") {
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .map_or_else(|| error.to_string(), str::to_owned);
            return Err(AssistantError::Provider {
                message: truncate_message(&message),
            });
        }
        if let Some(usage) = chunk.get("usage").filter(|u| u.is_object()) {
            self.usage = Some(read_usage(usage));
        }
        let Some(choice) = chunk.get("choices").and_then(|c| c.get(0)) else {
            return Ok(String::new());
        };
        if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
            self.finish_reason = Some(reason.to_owned());
        }
        let Some(delta) = choice.get("delta") else {
            return Ok(String::new());
        };
        if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
            for call in calls {
                self.tool_delta(call)?;
            }
        }
        let added = delta.get("content").and_then(Value::as_str).unwrap_or_default();
        Ok(self.add_text(added))
    }

    fn add_text(&mut self, added: &str) -> String {
        if added.is_empty() || self.text_truncated {
            return String::new();
        }
        let room = MAX_TEXT_BYTES.saturating_sub(self.text.len());
        if added.len() <= room {
            self.text.push_str(added);
            return added.to_owned();
        }
        let mut cut = String::new();
        for c in added.chars() {
            if cut.len().saturating_add(c.len_utf8()) > room {
                break;
            }
            cut.push(c);
        }
        self.text.push_str(&cut);
        self.text_truncated = true;
        cut
    }

    fn tool_delta(&mut self, call: &Value) -> Result<(), AssistantError> {
        let protocol = |detail: &str| AssistantError::Protocol {
            detail: detail.to_owned(),
        };
        let index = call.get("index").and_then(Value::as_u64).unwrap_or(0);
        let slot = if let Some(pos) = self.calls.iter().position(|(i, _)| *i == index) {
            pos
        } else {
            if self.calls.len() >= MAX_TOOL_CALLS {
                return Err(protocol("trop d'appels d'outils dans une réponse"));
            }
            self.calls.push((index, ToolCall::default()));
            self.calls.len().saturating_sub(1)
        };
        let Some((_, entry)) = self.calls.get_mut(slot) else {
            return Err(protocol("appel d'outil introuvable"));
        };
        if let Some(id) = call.get("id").and_then(Value::as_str)
            && entry.id.is_empty()
        {
            id.clone_into(&mut entry.id);
        }
        if let Some(function) = call.get("function") {
            if let Some(name) = function.get("name").and_then(Value::as_str) {
                if entry.name.len().saturating_add(name.len()) > 128 {
                    return Err(protocol("nom d'outil trop long"));
                }
                entry.name.push_str(name);
            }
            if let Some(arguments) = function.get("arguments").and_then(Value::as_str) {
                if entry.arguments.len().saturating_add(arguments.len()) > MAX_ARGUMENT_BYTES {
                    return Err(protocol("arguments d'outil trop longs"));
                }
                entry.arguments.push_str(arguments);
            }
        }
        Ok(())
    }

    /// Texte reçu.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Texte tronqué à [`MAX_TEXT_BYTES`].
    #[must_use]
    pub const fn text_truncated(&self) -> bool {
        self.text_truncated
    }

    /// Appels d'outils, dans l'ordre de leur `index`.
    #[must_use]
    pub fn tool_calls(&self) -> Vec<ToolCall> {
        let mut calls = self.calls.clone();
        calls.sort_by_key(|(i, _)| *i);
        calls.into_iter().map(|(_, c)| c).collect()
    }

    /// Raison de fin, si reçue.
    #[must_use]
    pub fn finish_reason(&self) -> Option<&str> {
        self.finish_reason.as_deref()
    }

    /// Jetons et coût, si reçus.
    #[must_use]
    pub const fn usage(&self) -> Option<Usage> {
        self.usage
    }

    /// `[DONE]` reçu.
    #[must_use]
    pub const fn done(&self) -> bool {
        self.done
    }

    /// Réponse complète : `[DONE]` ou une raison de fin reçue. Un flux
    /// fermé avant est une coupure.
    #[must_use]
    pub const fn complete(&self) -> bool {
        self.done || self.finish_reason.is_some()
    }
}

/// Lit `usage` (entiers absents : 0 ; `cost` facultatif).
fn read_usage(usage: &Value) -> Usage {
    let tokens = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
    Usage {
        prompt_tokens: tokens("prompt_tokens"),
        completion_tokens: tokens("completion_tokens"),
        cost: usage
            .get("cost")
            .and_then(Value::as_f64)
            .filter(|c| c.is_finite() && *c >= 0.0),
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
    fn text_and_usage_in_last_chunk() {
        let mut acc = StreamAccumulator::new();
        assert_eq!(
            acc.push(r#"{"choices":[{"index":0,"delta":{"role":"assistant","content":"Bon"}}]}"#)
                .unwrap(),
            "Bon"
        );
        acc.push(r#"{"choices":[{"index":0,"delta":{"content":"jour"},"finish_reason":"stop"}]}"#)
            .unwrap();
        acc.push(
            r#"{"choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":2,"total_tokens":12,"cost":0.0012}}"#,
        )
        .unwrap();
        acc.push("[DONE]").unwrap();
        assert_eq!(acc.text(), "Bonjour");
        assert_eq!(acc.finish_reason(), Some("stop"));
        let usage = acc.usage().unwrap();
        assert_eq!((usage.prompt_tokens, usage.completion_tokens), (10, 2));
        assert_eq!(usage.cost, Some(0.0012));
        assert!(acc.done() && acc.complete());
    }

    #[test]
    fn fragmented_tool_call_arguments() {
        let mut acc = StreamAccumulator::new();
        for chunk in [
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"get_task","arguments":""}}]}}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"ta"}}]}}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":1,"id":"call_2","function":{"name":"project_summary","arguments":"{}"}}]}}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"sk\": 3}"}}]}}]}"#,
            r#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#,
        ] {
            acc.push(chunk).unwrap();
        }
        let calls = acc.tool_calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].name, "get_task");
        assert_eq!(calls[0].arguments, "{\"task\": 3}");
        assert_eq!(calls[1].name, "project_summary");
        assert_eq!(acc.finish_reason(), Some("tool_calls"));
    }

    #[test]
    fn mid_stream_error() {
        let mut acc = StreamAccumulator::new();
        let err = acc
            .push(r#"{"error":{"code":"server_error","message":"Provider disconnected unexpectedly"},"choices":[{"delta":{"content":""},"finish_reason":"error"}]}"#)
            .unwrap_err();
        assert_eq!(
            err,
            AssistantError::Provider {
                message: "Provider disconnected unexpectedly".to_owned()
            }
        );
    }

    #[test]
    fn invalid_json_is_protocol_error() {
        let mut acc = StreamAccumulator::new();
        assert!(matches!(acc.push("{oops"), Err(AssistantError::Protocol { .. })));
        assert!(!acc.complete());
    }

    #[test]
    fn too_many_calls_refused() {
        let mut acc = StreamAccumulator::new();
        for i in 0..MAX_TOOL_CALLS {
            acc.push(&format!(r#"{{"choices":[{{"delta":{{"tool_calls":[{{"index":{i},"id":"c{i}","function":{{"name":"x"}}}}]}}}}]}}"#))
                .unwrap();
        }
        let over = format!(
            r#"{{"choices":[{{"delta":{{"tool_calls":[{{"index":{MAX_TOOL_CALLS},"function":{{"name":"x"}}}}]}}}}]}}"#
        );
        assert!(acc.push(&over).is_err());
    }

    #[test]
    fn text_truncated_at_bound() {
        let mut acc = StreamAccumulator::new();
        let big = "é".repeat(MAX_TEXT_BYTES);
        let chunk = serde_json::json!({"choices":[{"delta":{"content": big}}]}).to_string();
        acc.push(&chunk).unwrap();
        assert!(acc.text().len() <= MAX_TEXT_BYTES);
        assert!(acc.text_truncated());
    }

    #[test]
    fn usage_sum() {
        let a = Usage {
            prompt_tokens: 1,
            completion_tokens: 2,
            cost: Some(0.5),
        };
        let b = Usage {
            prompt_tokens: 3,
            completion_tokens: 4,
            cost: None,
        };
        assert_eq!(
            a.plus(b),
            Usage {
                prompt_tokens: 4,
                completion_tokens: 6,
                cost: Some(0.5)
            }
        );
    }
}
