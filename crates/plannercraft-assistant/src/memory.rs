//! Mémoire de conversation (lot A1b, cahier `docs/plans/assistant-chat.md`
//! § 3) : ce qui est renvoyé au modèle à chaque demande.
//!
//! Ordre des messages : consignes système (et définitions d'outils, portées
//! par le corps de la requête), **point de cache n° 1** ; tours précédents,
//! **point de cache n° 2** sur le dernier message d'historique ; puis le
//! tour en cours, dont la question porte seule le contexte de l'interface
//! (sélection, vue).
//!
//! Allègement : les tours anciens ne gardent que la question et le texte
//! final de la réponse (appels d'outils et résultats retirés) ; les
//! [`KEEP_INTACT`] derniers tours restent entiers ; au-delà de
//! [`MAX_HISTORY_TURNS`] tours ou d'environ [`HISTORY_TOKEN_BUDGET`] jetons
//! (estimés à [`BYTES_PER_TOKEN`] octets de JSON par jeton), les tours les
//! plus anciens sont abandonnés.
//!
//! Cache de prompt (doc relue le 2026-11-06 :
//! `https://openrouter.ai/docs/guides/best-practices/prompt-caching`) :
//! `cache_control: {"type": "ephemeral"}` (5 min) sur un bloc de texte,
//! posé seulement pour les modèles qui le demandent explicitement
//! (Anthropic) ; Gemini et les autres mettent en cache sans marque.

use serde_json::{Value, json};

use crate::conversation::Turn;

/// Tours précédents gardés entiers (appels d'outils et résultats).
pub const KEEP_INTACT: usize = 2;
/// Tours précédents renvoyés au plus.
pub const MAX_HISTORY_TURNS: usize = 20;
/// Budget de l'historique (jetons estimés).
pub const HISTORY_TOKEN_BUDGET: usize = 16_000;
/// Octets de JSON comptés pour un jeton (estimation prudente).
pub const BYTES_PER_TOKEN: usize = 4;

/// Le modèle `model` prend des points de cache explicites
/// (`cache_control`) : modèles Anthropic.
#[must_use]
pub fn supports_cache_control(model: &str) -> bool {
    model.starts_with("anthropic/")
}

/// Jetons estimés de `messages`.
#[must_use]
pub fn estimate_tokens(messages: &[Value]) -> usize {
    messages
        .iter()
        .map(|m| m.to_string().len())
        .sum::<usize>()
        .div_ceil(BYTES_PER_TOKEN)
}

/// Forme allégée d'un tour : la question et le texte final de la réponse
/// (rien si la réponse est vide : une question sans réponse n'apprend rien
/// au modèle).
#[must_use]
pub fn reduced(turn: &Turn) -> Vec<Value> {
    if turn.answer.trim().is_empty() {
        return Vec::new();
    }
    vec![
        json!({ "role": "user", "content": turn.question }),
        json!({ "role": "assistant", "content": turn.answer }),
    ]
}

/// Messages d'historique des tours `previous` (du plus ancien au plus
/// récent), allégés et bornés.
#[must_use]
pub fn history(previous: &[Turn]) -> Vec<Value> {
    let start = previous.len().saturating_sub(MAX_HISTORY_TURNS);
    let kept = previous.get(start..).unwrap_or_default();
    let intact_from = kept.len().saturating_sub(KEEP_INTACT);
    let mut blocks: Vec<Vec<Value>> = kept
        .iter()
        .enumerate()
        .map(|(i, turn)| {
            if i >= intact_from && !turn.wire.is_empty() {
                turn.wire.clone()
            } else {
                reduced(turn)
            }
        })
        .collect();
    let total = |blocks: &[Vec<Value>]| blocks.iter().map(|b| estimate_tokens(b)).sum::<usize>();
    // Les plus anciens abandonnés au-delà du budget ; le plus récent est
    // gardé (allégé s'il dépasse seul).
    while blocks.len() > 1 && total(&blocks) > HISTORY_TOKEN_BUDGET {
        blocks.remove(0);
    }
    if total(&blocks) > HISTORY_TOKEN_BUDGET
        && let (Some(block), Some(turn)) = (blocks.first_mut(), kept.last())
    {
        *block = reduced(turn);
    }
    blocks.into_iter().flatten().collect()
}

/// Pose un point de cache sur `message` : son contenu texte devient un
/// bloc `{"type":"text","text":…,"cache_control":{"type":"ephemeral"}}`.
/// Faux si le message n'a pas de texte.
pub fn mark_cache(message: &mut Value) -> bool {
    let Some(map) = message.as_object_mut() else {
        return false;
    };
    let Some(text) = map.get("content").and_then(Value::as_str).map(str::to_owned) else {
        return false;
    };
    if text.is_empty() {
        return false;
    }
    map.insert(
        "content".to_owned(),
        json!([{ "type": "text", "text": text, "cache_control": { "type": "ephemeral" } }]),
    );
    true
}

/// Pose le point de cache n° 2 sur le dernier message d'historique qui
/// porte du texte (question ou réponse).
pub fn mark_history_cache(history: &mut [Value]) {
    for message in history.iter_mut().rev() {
        let role = message.get("role").and_then(Value::as_str);
        if matches!(role, Some("user" | "assistant")) && mark_cache(message) {
            return;
        }
    }
}

/// Ajoute le contexte de l'interface à la question (premier message
/// `user` du tour en cours), jamais gardé dans l'historique.
pub fn attach_context(current: &mut [Value], context: &str) {
    if context.trim().is_empty() {
        return;
    }
    let question = current
        .iter_mut()
        .find(|m| m.get("role").and_then(Value::as_str) == Some("user"));
    if let Some(Value::Object(map)) = question
        && let Some(text) = map.get("content").and_then(Value::as_str)
    {
        let joined = format!("{text}\n\n{context}");
        map.insert("content".to_owned(), Value::String(joined));
    }
}

/// Messages complets d'une requête : système (point de cache n° 1 si
/// `cache`), historique allégé (point n° 2), tour en cours avec le
/// contexte.
#[must_use]
pub fn request_messages(
    system: &str,
    previous: &[Turn],
    current: &[Value],
    context: Option<&str>,
    cache: bool,
) -> Vec<Value> {
    let mut messages = Vec::new();
    let mut system = json!({ "role": "system", "content": system });
    if cache {
        mark_cache(&mut system);
    }
    messages.push(system);
    let mut past = history(previous);
    if cache {
        mark_history_cache(&mut past);
    }
    messages.extend(past);
    let mut now: Vec<Value> = current.to_vec();
    if let Some(context) = context {
        attach_context(&mut now, context);
    }
    messages.extend(now);
    messages
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : un échec doit s'arrêter net"
    )]

    use super::*;
    use crate::conversation::TurnState;

    fn turn(question: &str, answer: &str, tool_result: &str) -> Turn {
        let mut t = Turn::new(question, "anthropic/claude-sonnet-5.5", None);
        t.answer = answer.to_owned();
        t.state = TurnState::Done;
        t.wire = vec![
            json!({ "role": "user", "content": question }),
            json!({ "role": "assistant", "content": null, "tool_calls": [
                { "id": "c1", "type": "function", "function": { "name": "critical_path", "arguments": "{}" } }
            ] }),
            json!({ "role": "tool", "tool_call_id": "c1", "content": tool_result }),
            json!({ "role": "assistant", "content": answer }),
        ];
        t
    }

    /// Fixture : ordre et points de cache d'une requête de troisième tour.
    #[test]
    fn request_order_and_cache_points() {
        let previous = vec![
            turn("Q1", "R1", "{\"a\":1}"),
            turn("Q2", "R2", "{\"b\":2}"),
            turn("Q3", "R3", "{\"c\":3}"),
        ];
        let current = vec![json!({ "role": "user", "content": "Q4" })];
        let messages = request_messages(
            "SYS",
            &previous,
            &current,
            Some("[Interface] {\"active\":12}"),
            true,
        );
        let expected = json!([
            { "role": "system", "content": [
                { "type": "text", "text": "SYS", "cache_control": { "type": "ephemeral" } } ] },
            // Tour 1 allégé : question et réponse seulement.
            { "role": "user", "content": "Q1" },
            { "role": "assistant", "content": "R1" },
            // Tours 2 et 3 intacts.
            { "role": "user", "content": "Q2" },
            { "role": "assistant", "content": null, "tool_calls": [
                { "id": "c1", "type": "function", "function": { "name": "critical_path", "arguments": "{}" } } ] },
            { "role": "tool", "tool_call_id": "c1", "content": "{\"b\":2}" },
            { "role": "assistant", "content": "R2" },
            { "role": "user", "content": "Q3" },
            { "role": "assistant", "content": null, "tool_calls": [
                { "id": "c1", "type": "function", "function": { "name": "critical_path", "arguments": "{}" } } ] },
            { "role": "tool", "tool_call_id": "c1", "content": "{\"c\":3}" },
            // Point de cache n° 2 : dernier message d'historique.
            { "role": "assistant", "content": [
                { "type": "text", "text": "R3", "cache_control": { "type": "ephemeral" } } ] },
            // Question en cours, seule à porter le contexte.
            { "role": "user", "content": "Q4\n\n[Interface] {\"active\":12}" },
        ]);
        assert_eq!(Value::Array(messages), expected);
    }

    /// Sans cache explicite (Gemini) : contenus en texte simple.
    #[test]
    fn no_cache_marks_for_other_models() {
        assert!(supports_cache_control("anthropic/claude-haiku-5.5"));
        assert!(!supports_cache_control("google/gemini-3.8-flash"));
        let previous = vec![turn("Q1", "R1", "{}")];
        let messages = request_messages(
            "SYS",
            &previous,
            &[json!({"role":"user","content":"Q2"})],
            None,
            false,
        );
        assert!(
            !Value::Array(messages.clone())
                .to_string()
                .contains("cache_control")
        );
        assert_eq!(messages[0]["content"], "SYS");
    }

    /// Au-delà de 20 tours, les plus anciens sont abandonnés.
    #[test]
    fn at_most_twenty_turns() {
        let previous: Vec<Turn> = (0..25).map(|i| turn(&format!("Q{i}"), "R", "{}")).collect();
        let messages = history(&previous);
        assert_eq!(messages[0]["content"], "Q5");
        let users = messages.iter().filter(|m| m["role"] == "user").count();
        assert_eq!(users, MAX_HISTORY_TURNS);
    }

    /// Budget d'environ 16 k jetons : les tours anciens volumineux tombent ;
    /// le plus récent est gardé, allégé s'il dépasse seul.
    #[test]
    fn token_budget_drops_oldest() {
        let big = "x".repeat(HISTORY_TOKEN_BUDGET * BYTES_PER_TOKEN / 3);
        let previous = vec![
            turn("Q1", &big, "{}"),
            turn("Q2", &big, "{}"),
            turn("Q3", &big, "{}"),
            turn("Q4", "R4", "{}"),
        ];
        let messages = history(&previous);
        assert!(estimate_tokens(&messages) <= HISTORY_TOKEN_BUDGET);
        assert_eq!(messages.last().unwrap()["content"], "R4");
        assert!(!messages.iter().any(|m| m["content"] == "Q1"));
        let huge = "y".repeat(HISTORY_TOKEN_BUDGET * BYTES_PER_TOKEN * 2);
        let messages = history(&[turn("Q", "R", &huge)]);
        assert_eq!(messages.len(), 2, "allégé : question et réponse");
    }

    /// Un tour sans réponse (erreur, arrêt immédiat) n'est pas renvoyé
    /// allégé.
    #[test]
    fn reduced_skips_unanswered() {
        assert_eq!(reduced(&turn("Q", "", "{}")), Vec::<Value>::new());
    }
}
