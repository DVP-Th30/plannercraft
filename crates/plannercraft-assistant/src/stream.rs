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
//!   resté 200) ;
//! - raisonnement (lot A1b) : `delta.reasoning_details[]` (`reasoning.text`
//!   → `text`, `reasoning.summary` → `summary`, `reasoning.encrypted` →
//!   `data`, jamais affiché), fusionnés par `type` et `index` pour être
//!   renvoyés tels quels avec un appel d'outil ; à défaut,
//!   `delta.reasoning` (texte). Doc relue le 2026-11-06 :
//!   `https://openrouter.ai/docs/use-cases/reasoning-tokens`.
//!
//! Règle 10 : textes, arguments et nombre d'appels bornés ; un morceau
//! illisible est une erreur, jamais une panique.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AssistantError, truncate_message};

/// Texte de réponse gardé au plus (octets) ; la suite est ignorée.
pub const MAX_TEXT_BYTES: usize = 64 * 1024;
/// Arguments d'un appel d'outil au plus (octets).
pub const MAX_ARGUMENT_BYTES: usize = 64 * 1024;
/// Appels d'outils au plus dans une réponse.
pub const MAX_TOOL_CALLS: usize = 16;
/// Raisonnement gardé au plus (octets, texte affiché et détails renvoyés).
pub const MAX_REASONING_BYTES: usize = 64 * 1024;

/// Jetons et coût d'une réponse (`usage`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Usage {
    /// Jetons d'entrée.
    pub prompt_tokens: u64,
    /// Jetons de sortie.
    pub completion_tokens: u64,
    /// Jetons d'entrée lus dans le cache de prompt
    /// (`prompt_tokens_details.cached_tokens`), compris dans
    /// `prompt_tokens`.
    pub cached_tokens: u64,
    /// Jetons d'entrée écrits dans le cache
    /// (`prompt_tokens_details.cache_write_tokens`).
    pub cache_write_tokens: u64,
    /// Jetons de raisonnement (`completion_tokens_details.reasoning_tokens`),
    /// compris dans `completion_tokens`.
    pub reasoning_tokens: u64,
    /// Coût en crédits OpenRouter (dollars US), s'il est donné : coût réel,
    /// cache compris.
    pub cost: Option<f64>,
}

/// Ajouts d'un morceau : texte de la réponse et texte du raisonnement.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Delta {
    /// Texte de réponse ajouté.
    pub text: String,
    /// Texte de raisonnement ajouté (affiché replié).
    pub reasoning: String,
}

impl Delta {
    /// Rien d'ajouté.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.text.is_empty() && self.reasoning.is_empty()
    }
}

impl Usage {
    /// Somme de deux relevés (tours d'une même demande).
    #[must_use]
    pub fn plus(self, other: Self) -> Self {
        Self {
            prompt_tokens: self.prompt_tokens.saturating_add(other.prompt_tokens),
            completion_tokens: self.completion_tokens.saturating_add(other.completion_tokens),
            cached_tokens: self.cached_tokens.saturating_add(other.cached_tokens),
            cache_write_tokens: self.cache_write_tokens.saturating_add(other.cache_write_tokens),
            reasoning_tokens: self.reasoning_tokens.saturating_add(other.reasoning_tokens),
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
    reasoning: String,
    reasoning_details: Vec<Value>,
    reasoning_bytes: usize,
    /// Détails tronqués : ils ne sont plus renvoyés (signature invalide).
    reasoning_truncated: bool,
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

    /// Lit la charge `data` d'un évènement SSE ; rend le texte et le
    /// raisonnement ajoutés.
    ///
    /// # Errors
    ///
    /// [`AssistantError::Provider`] pour une erreur annoncée dans le flux,
    /// [`AssistantError::Protocol`] pour un morceau illisible ou un appel
    /// d'outil au-delà des bornes.
    pub fn push(&mut self, data: &str) -> Result<Delta, AssistantError> {
        let data = data.trim();
        if data == "[DONE]" {
            self.done = true;
            return Ok(Delta::default());
        }
        if data.is_empty() {
            return Ok(Delta::default());
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
            return Ok(Delta::default());
        };
        if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
            self.finish_reason = Some(reason.to_owned());
        }
        let Some(delta) = choice.get("delta") else {
            return Ok(Delta::default());
        };
        if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
            for call in calls {
                self.tool_delta(call)?;
            }
        }
        let reasoning = self.reasoning_delta(delta);
        let added = delta.get("content").and_then(Value::as_str).unwrap_or_default();
        Ok(Delta {
            text: self.add_text(added),
            reasoning,
        })
    }

    /// Raisonnement d'un `delta` : `reasoning_details[]` s'il est là
    /// (fusionné pour être renvoyé), sinon `reasoning` (texte seul). Rend le
    /// texte lisible ajouté (jamais le contenu chiffré).
    fn reasoning_delta(&mut self, delta: &Value) -> String {
        let mut shown = String::new();
        if let Some(details) = delta.get("reasoning_details").and_then(Value::as_array) {
            for detail in details {
                shown.push_str(&self.reasoning_detail(detail));
            }
        } else if let Some(text) = delta.get("reasoning").and_then(Value::as_str) {
            shown.push_str(text);
        }
        if shown.is_empty() {
            return shown;
        }
        let room = MAX_REASONING_BYTES.saturating_sub(self.reasoning.len());
        let cut = prefix_within(&shown, room);
        self.reasoning.push_str(&cut);
        cut
    }

    /// Fusionne un détail de raisonnement avec le précédent de même `type`
    /// et `index` (morceaux d'un même bloc) ; rend son texte lisible.
    fn reasoning_detail(&mut self, detail: &Value) -> String {
        let Some(object) = detail.as_object() else {
            return String::new();
        };
        let kind = object.get("type").and_then(Value::as_str).unwrap_or_default();
        let field = match kind {
            "reasoning.text" => "text",
            "reasoning.summary" => "summary",
            "reasoning.encrypted" => "data",
            _ => "",
        };
        let piece = object
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let shown = if kind == "reasoning.encrypted" {
            String::new()
        } else {
            piece.clone()
        };
        let size = detail.to_string().len();
        self.reasoning_bytes = self.reasoning_bytes.saturating_add(size);
        if self.reasoning_bytes > MAX_REASONING_BYTES {
            self.reasoning_truncated = true;
            self.reasoning_details.clear();
            return shown;
        }
        if self.reasoning_truncated {
            return shown;
        }
        let index = object.get("index").cloned();
        let same_block = self.reasoning_details.last().is_some_and(|last| {
            last.get("type").and_then(Value::as_str) == Some(kind) && last.get("index").cloned() == index
        });
        match self.reasoning_details.last_mut().and_then(Value::as_object_mut) {
            Some(last) if same_block => {
                if !field.is_empty() {
                    let mut joined = last
                        .get(field)
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned();
                    joined.push_str(&piece);
                    last.insert(field.to_owned(), Value::String(joined));
                }
                // Signature, identifiant, format : la dernière valeur non vide.
                for (key, value) in object {
                    if key != field && key != "type" && key != "index" && !is_empty_value(value) {
                        last.insert(key.clone(), value.clone());
                    }
                }
            }
            _ => self.reasoning_details.push(detail.clone()),
        }
        shown
    }

    fn add_text(&mut self, added: &str) -> String {
        if added.is_empty() || self.text_truncated {
            return String::new();
        }
        let room = MAX_TEXT_BYTES.saturating_sub(self.text.len());
        let cut = prefix_within(added, room);
        if cut.len() < added.len() {
            self.text_truncated = true;
        }
        self.text.push_str(&cut);
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

    /// Raisonnement lisible reçu.
    #[must_use]
    pub fn reasoning(&self) -> &str {
        &self.reasoning
    }

    /// Détails de raisonnement à renvoyer avec un appel d'outil (vide s'ils
    /// ont été tronqués : une signature incomplète serait refusée).
    #[must_use]
    pub fn reasoning_details(&self) -> &[Value] {
        if self.reasoning_truncated {
            &[]
        } else {
            &self.reasoning_details
        }
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

/// Préfixe de `text` d'au plus `room` octets, coupé entre deux caractères.
fn prefix_within(text: &str, room: usize) -> String {
    if text.len() <= room {
        return text.to_owned();
    }
    let mut cut = String::new();
    for c in text.chars() {
        if cut.len().saturating_add(c.len_utf8()) > room {
            break;
        }
        cut.push(c);
    }
    cut
}

/// Valeur JSON vide (nulle ou chaîne vide).
fn is_empty_value(value: &Value) -> bool {
    value.is_null() || value.as_str().is_some_and(str::is_empty)
}

/// Lit `usage` (entiers absents : 0 ; `cost` facultatif ; détails du cache
/// et du raisonnement s'ils sont donnés).
fn read_usage(usage: &Value) -> Usage {
    let tokens = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
    let detail = |group: &str, key: &str| {
        usage
            .get(group)
            .and_then(|g| g.get(key))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    Usage {
        prompt_tokens: tokens("prompt_tokens"),
        completion_tokens: tokens("completion_tokens"),
        cached_tokens: detail("prompt_tokens_details", "cached_tokens"),
        cache_write_tokens: detail("prompt_tokens_details", "cache_write_tokens"),
        reasoning_tokens: detail("completion_tokens_details", "reasoning_tokens"),
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
                .unwrap()
                .text,
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
            cached_tokens: 1,
            cost: Some(0.5),
            ..Usage::default()
        };
        let b = Usage {
            prompt_tokens: 3,
            completion_tokens: 4,
            cached_tokens: 2,
            cost: None,
            ..Usage::default()
        };
        assert_eq!(
            a.plus(b),
            Usage {
                prompt_tokens: 4,
                completion_tokens: 6,
                cached_tokens: 3,
                cost: Some(0.5),
                ..Usage::default()
            }
        );
    }

    /// Lot A1b : jetons en cache et de raisonnement lus dans `usage`.
    #[test]
    fn usage_reads_cache_and_reasoning_details() {
        let mut acc = StreamAccumulator::new();
        acc.push(
            r#"{"choices":[{"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":3000,"completion_tokens":200,"prompt_tokens_details":{"cached_tokens":2400,"cache_write_tokens":0},"completion_tokens_details":{"reasoning_tokens":120},"cost":0.0021}}"#,
        )
        .unwrap();
        let usage = acc.usage().unwrap();
        assert_eq!(usage.cached_tokens, 2400);
        assert_eq!(usage.cache_write_tokens, 0);
        assert_eq!(usage.reasoning_tokens, 120);
    }

    /// Lot A1b : raisonnement en `reasoning_details` (texte fragmenté,
    /// signature au dernier morceau, bloc chiffré jamais affiché), fusionné
    /// par `type` et `index` pour être renvoyé.
    #[test]
    fn reasoning_details_streamed_and_merged() {
        let mut acc = StreamAccumulator::new();
        let d1 = acc
            .push(r#"{"choices":[{"delta":{"reasoning":"Je lis","reasoning_details":[{"type":"reasoning.text","text":"Je lis","format":"anthropic-claude-v1","index":0}]}}]}"#)
            .unwrap();
        assert_eq!(d1.reasoning, "Je lis");
        assert_eq!(d1.text, "");
        let d2 = acc
            .push(r#"{"choices":[{"delta":{"reasoning_details":[{"type":"reasoning.text","text":" le chemin critique.","signature":"sig-1","format":"anthropic-claude-v1","index":0}]}}]}"#)
            .unwrap();
        assert_eq!(d2.reasoning, " le chemin critique.");
        let d3 = acc
            .push(r#"{"choices":[{"delta":{"reasoning_details":[{"type":"reasoning.encrypted","data":"[REDACTED]","format":"anthropic-claude-v1","index":1}]}}]}"#)
            .unwrap();
        assert_eq!(d3.reasoning, "");
        let d4 = acc
            .push(r#"{"choices":[{"delta":{"content":"Réponse"}}]}"#)
            .unwrap();
        assert_eq!(d4.text, "Réponse");
        assert_eq!(acc.reasoning(), "Je lis le chemin critique.");
        let details = acc.reasoning_details();
        assert_eq!(details.len(), 2);
        assert_eq!(details[0]["text"], "Je lis le chemin critique.");
        assert_eq!(details[0]["signature"], "sig-1");
        assert_eq!(details[1]["type"], "reasoning.encrypted");
    }

    /// `reasoning` seul (sans détails) : texte affiché, rien à renvoyer.
    #[test]
    fn plain_reasoning_field() {
        let mut acc = StreamAccumulator::new();
        let d = acc
            .push(r#"{"choices":[{"delta":{"reasoning":"Hmm."}}]}"#)
            .unwrap();
        assert_eq!(d.reasoning, "Hmm.");
        assert_eq!(acc.reasoning_details(), &[] as &[Value]);
    }

    /// Raisonnement borné ; détails tronqués jamais renvoyés.
    #[test]
    fn reasoning_bounded() {
        let mut acc = StreamAccumulator::new();
        let big = "x".repeat(MAX_REASONING_BYTES);
        for _ in 0..3 {
            let chunk = serde_json::json!({"choices":[{"delta":{"reasoning_details":[{"type":"reasoning.text","text": big, "index": 0}]}}]}).to_string();
            acc.push(&chunk).unwrap();
        }
        assert!(acc.reasoning().len() <= MAX_REASONING_BYTES);
        assert_eq!(acc.reasoning_details(), &[] as &[Value]);
    }
}
