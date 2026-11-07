//! Outils donnés au modèle (lot A1 : lectures ; lot A2 : plus une seule
//! écriture, [`crate::proposal::PROPOSE_TOOL`], qui ne s'applique jamais
//! directement), pris dans le
//! registre des commandes (ADR-0012 D7) : mêmes noms que les outils MCP,
//! mêmes schémas allégés (sans `$schema` ni `title`, descriptions
//! raccourcies), exécutés par [`plannercraft_automation::call_with`] sur une
//! **copie** de la session : une écriture est impossible par construction
//! (liste blanche de lectures, genre vérifié, copie jetée après l'appel).
//!
//! `ui_state` est servi par l'interface (tâche sélectionnée, vue) : le
//! registre le réserve à une application rattachée.

use plannercraft_automation::text::Language;
use plannercraft_automation::{CallEnv, CommandSpec, Kind, REGISTRY, Session, ToolError, call_with};
use serde_json::{Map, Value, json};

use crate::stream::ToolCall;

/// Lectures du registre exposées au modèle.
pub const READ_TOOL_IDS: &[&str] = &[
    "project.summary",
    "task.list",
    "task.get",
    "analysis.explain_date",
    "analysis.critical_path",
    "resource.list",
    "analysis.baseline_variance",
];

/// Nom de l'outil d'état de l'interface.
pub const UI_STATE_TOOL: &str = "ui_state";
/// Id (registre) de l'outil d'état de l'interface.
pub const UI_STATE_ID: &str = "app.ui_state";

/// Résultat d'outil envoyé au plus (octets) : au-delà, tronqué avec une
/// note (le modèle peut paginer ou filtrer).
pub const MAX_RESULT_BYTES: usize = 24 * 1024;

/// Description d'outil gardée au plus (caractères).
pub const MAX_DESCRIPTION_CHARS: usize = 160;

/// Hôte des outils : exécute une lecture sur le projet ouvert.
pub trait ToolHost {
    /// Appelle la commande `id` du registre (ou [`UI_STATE_ID`]).
    ///
    /// # Errors
    ///
    /// L'erreur structurée de la commande (rendue au modèle).
    fn call(&mut self, id: &str, params: Value) -> Result<Value, ToolError>;

    /// Soumet une proposition d'écriture (lot A2) : `label` et paramètres
    /// de `batch.apply` déjà contrôlés ([`crate::proposal::check`]). L'hôte
    /// fait le `dry_run`, met la proposition en attente de l'utilisateur et
    /// rend son identifiant ; rien n'est appliqué. Défaut : refus (hôte en
    /// lecture seule).
    ///
    /// # Errors
    ///
    /// Le refus du registre (validation, boucle, champ calculé…), rendu au
    /// modèle pour qu'il corrige.
    fn propose(&mut self, label: &str, params: Value) -> Result<crate::proposal::Proposed, ToolError> {
        let _ = (label, params);
        Err(ToolError::simple(
            "tool.unknown",
            "propositions indisponibles ici".to_owned(),
        ))
    }
}

/// Hôte sur une session tenue par l'interface : chaque appel travaille sur
/// une copie (règle « lecture seule » du lot A1). Une proposition (lot A2)
/// est essayée en `dry_run` sur une copie puis **mise de côté**
/// ([`SessionHost::proposals`]) : l'interface en fait une écriture en
/// attente de validation ; rien n'est jamais appliqué ici.
pub struct SessionHost<'a> {
    /// Session du document ouvert, s'il y en a un.
    pub session: Option<&'a Session>,
    /// Environnement des appels (maintenant, langue).
    pub env: CallEnv,
    /// État de l'interface rendu par `ui_state`.
    pub ui_state: Value,
    /// Identifiant de la prochaine proposition.
    pub next_proposal: u64,
    /// Propositions acceptées en file pendant ce passage, à reprendre par
    /// l'interface.
    pub proposals: Vec<QueuedProposal>,
}

impl<'a> SessionHost<'a> {
    /// Hôte sur `session` ; propositions numérotées à partir de 1.
    #[must_use]
    pub const fn new(session: Option<&'a Session>, env: CallEnv, ui_state: Value) -> Self {
        Self {
            session,
            env,
            ui_state,
            next_proposal: 1,
            proposals: Vec::new(),
        }
    }
}

/// Proposition essayée et mise en file (lot A2) : à valider par
/// l'utilisateur.
#[derive(Clone, Debug, PartialEq)]
pub struct QueuedProposal {
    /// Identifiant.
    pub id: u64,
    /// Libellé.
    pub label: String,
    /// Paramètres de `batch.apply` (sans `dry_run`).
    pub params: Value,
    /// Résultat du `dry_run` (JSON de `batch.apply`).
    pub outcome: Value,
}

impl ToolHost for SessionHost<'_> {
    fn call(&mut self, id: &str, params: Value) -> Result<Value, ToolError> {
        if id == UI_STATE_ID {
            return Ok(self.ui_state.clone());
        }
        let Some(session) = self.session else {
            return Err(no_session(self.env.lang));
        };
        let mut copy = session.clone();
        call_with(&mut copy, &self.env, id, params)
    }

    fn propose(&mut self, label: &str, params: Value) -> Result<crate::proposal::Proposed, ToolError> {
        let Some(session) = self.session else {
            return Err(no_session(self.env.lang));
        };
        let mut dry = params.clone();
        if let Value::Object(map) = &mut dry {
            map.insert("dry_run".to_owned(), Value::Bool(true));
        }
        let mut copy = session.clone();
        let outcome = call_with(&mut copy, &self.env, "batch.apply", dry)?;
        let id = self.next_proposal;
        self.next_proposal = self.next_proposal.saturating_add(1);
        let changed = outcome
            .pointer("/diff/changed_total")
            .and_then(Value::as_u64)
            .map_or(0, |n| u32::try_from(n).unwrap_or(u32::MAX));
        let result = crate::proposal::pending_result(id, label, &outcome, self.env.lang);
        self.proposals.push(QueuedProposal {
            id,
            label: label.to_owned(),
            params,
            outcome,
        });
        Ok(crate::proposal::Proposed {
            id,
            label: label.to_owned(),
            changed_tasks: changed,
            result,
        })
    }
}

/// Refus sans projet ouvert.
fn no_session(lang: Language) -> ToolError {
    ToolError::simple(
        "session.none",
        match lang {
            Language::Fr => "aucun projet ouvert dans l'application".to_owned(),
            Language::En => "no project open in the application".to_owned(),
        },
    )
}

/// Entrée du registre d'un outil exposé, par son nom.
fn spec_by_name(name: &str) -> Option<&'static CommandSpec> {
    REGISTRY.iter().find(|s| {
        s.mcp_name == name && READ_TOOL_IDS.contains(&s.id) && matches!(s.kind, Kind::Read | Kind::Analysis)
    })
}

/// Définitions d'outils au format Chat Completions
/// (`{"type":"function","function":{name, description, parameters}}`).
#[must_use]
pub fn definitions(lang: Language) -> Vec<Value> {
    let mut tools: Vec<Value> = REGISTRY
        .iter()
        .filter(|s| READ_TOOL_IDS.contains(&s.id) && s.is_listed())
        .map(|s| {
            let mut parameters = Value::from((s.schema)());
            lighten(&mut parameters);
            if let Value::Object(map) = &mut parameters {
                map.insert("type".to_owned(), Value::String("object".to_owned()));
                map.entry("properties")
                    .or_insert_with(|| Value::Object(Map::new()));
            }
            function(s.mcp_name, &short(s.summary(lang)), &parameters)
        })
        .collect();
    let ui = match lang {
        Language::Fr => "Tâche active et sélection de l'interface, vue affichée, lignes visibles.",
        Language::En => "Active task and selection in the interface, current view, visible rows.",
    };
    tools.push(function(
        UI_STATE_TOOL,
        ui,
        &json!({ "type": "object", "properties": {} }),
    ));
    tools.push(crate::proposal::definition(lang));
    tools
}

fn function(name: &str, description: &str, parameters: &Value) -> Value {
    json!({
        "type": "function",
        "function": { "name": name, "description": description, "parameters": parameters },
    })
}

/// Retire `$schema` et `title`, raccourcit les descriptions (récursif).
/// Les clés d'une table `properties` sont des noms de champs (un champ
/// peut s'appeler `title`) : seules leurs valeurs sont allégées.
fn lighten(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("$schema");
            map.remove("title");
            if let Some(Value::String(text)) = map.get_mut("description") {
                *text = short(text);
            }
            for (key, child) in map.iter_mut() {
                match (key.as_str(), child) {
                    ("properties" | "$defs" | "definitions", Value::Object(fields)) => {
                        fields.values_mut().for_each(lighten);
                    }
                    (_, child) => lighten(child),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(lighten),
        _ => {}
    }
}

/// Première phrase de `text`, au plus [`MAX_DESCRIPTION_CHARS`] caractères.
fn short(text: &str) -> String {
    let first = text.split_inclusive(". ").next().unwrap_or(text).trim_end();
    let mut out: String = first.chars().take(MAX_DESCRIPTION_CHARS).collect();
    if first.chars().nth(MAX_DESCRIPTION_CHARS).is_some() {
        out.push('…');
    }
    out
}

/// Taille en octets du JSON des définitions (mesure du budget d'outils).
#[must_use]
pub fn definitions_bytes(lang: Language) -> usize {
    Value::Array(definitions(lang)).to_string().len()
}

/// Exécute l'appel `call` du modèle ; rend le texte du résultat (JSON),
/// une erreur structurée comprise : le modèle la lit et se corrige.
pub fn execute(host: &mut dyn ToolHost, call: &ToolCall, lang: Language) -> String {
    let id = if call.name == UI_STATE_TOOL {
        UI_STATE_ID
    } else if let Some(spec) = spec_by_name(&call.name) {
        spec.id
    } else {
        return error_json(
            "tool.unknown",
            &match lang {
                Language::Fr => format!("outil inconnu ou non permis en lecture : {}", call.name),
                Language::En => format!("unknown or non-read tool: {}", call.name),
            },
        );
    };
    let params = if call.arguments.trim().is_empty() {
        Value::Object(Map::new())
    } else {
        match serde_json::from_str::<Value>(&call.arguments) {
            Ok(Value::Null) => Value::Object(Map::new()),
            Ok(value) => value,
            Err(error) => {
                return error_json(
                    "param.invalid",
                    &match lang {
                        Language::Fr => format!("arguments JSON invalides : {error}"),
                        Language::En => format!("invalid JSON arguments: {error}"),
                    },
                );
            }
        }
    };
    match host.call(id, params) {
        Ok(value) => bounded(&value.to_string(), lang),
        Err(error) => serde_json::to_value(&error).map_or_else(
            |_| error_json(error.code, &error.message),
            |v| json!({ "error": v }).to_string(),
        ),
    }
}

fn error_json(code: &str, message: &str) -> String {
    json!({ "error": { "code": code, "message": message } }).to_string()
}

/// `text` borné à [`MAX_RESULT_BYTES`] (coupé sur un caractère), avec une
/// note de troncature.
fn bounded(text: &str, lang: Language) -> String {
    if text.len() <= MAX_RESULT_BYTES {
        return text.to_owned();
    }
    let mut out = String::with_capacity(MAX_RESULT_BYTES.saturating_add(120));
    for c in text.chars() {
        if out.len().saturating_add(c.len_utf8()) > MAX_RESULT_BYTES {
            break;
        }
        out.push(c);
    }
    out.push_str(match lang {
        Language::Fr => "\n[résultat tronqué : paginer (offset, limit) ou filtrer]",
        Language::En => "\n[result truncated: paginate (offset, limit) or filter]",
    });
    out
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
    fn whitelist_is_read_only_and_known() {
        for id in READ_TOOL_IDS {
            let spec = REGISTRY.iter().find(|s| s.id == *id).unwrap();
            assert!(matches!(spec.kind, Kind::Read | Kind::Analysis), "{id}");
            assert!(spec.is_listed(), "{id}");
            assert!(!spec.dry_run && !spec.undoable, "{id}");
        }
    }

    #[test]
    fn definitions_light_and_under_budget() {
        for lang in [Language::Fr, Language::En] {
            let defs = definitions(lang);
            assert_eq!(defs.len(), READ_TOOL_IDS.len() + 2);
            let text = Value::Array(defs).to_string();
            assert!(!text.contains("$schema"), "{text}");
            assert!(!text.contains("\"title\""));
            // Objectif < 15 000 jetons d'outils ; 4 octets par jeton est une
            // estimation prudente pour du JSON (souvent moins).
            let bytes = definitions_bytes(lang);
            println!(
                "outils de l'assistant ({lang:?}) : {bytes} octets ≈ {} jetons",
                bytes / 4
            );
            assert!(bytes < 15_000 * 4, "{bytes}");
        }
    }

    #[test]
    fn short_keeps_first_sentence() {
        assert_eq!(short("Un. Deux."), "Un.");
        assert_eq!(short("Sans point"), "Sans point");
        assert_eq!(short(&"x".repeat(200)).chars().count(), MAX_DESCRIPTION_CHARS + 1);
    }

    struct Recorder(Vec<(String, Value)>);
    impl ToolHost for Recorder {
        fn call(&mut self, id: &str, params: Value) -> Result<Value, ToolError> {
            self.0.push((id.to_owned(), params));
            Ok(json!({ "ok": true }))
        }
    }

    fn call(name: &str, arguments: &str) -> ToolCall {
        ToolCall {
            id: "c".to_owned(),
            name: name.to_owned(),
            arguments: arguments.to_owned(),
        }
    }

    #[test]
    fn write_tools_refused_before_host() {
        let mut host = Recorder(Vec::new());
        for name in [
            "add_task",
            "apply_batch",
            "save_project",
            "ui_action",
            "save_style_as_template",
            "nope",
        ] {
            let out = execute(&mut host, &call(name, "{}"), Language::Fr);
            assert!(out.contains("tool.unknown"), "{name} : {out}");
        }
        assert_eq!(host.0.len(), 0);
    }

    #[test]
    fn arguments_parsed_or_reported() {
        let mut host = Recorder(Vec::new());
        assert_eq!(
            execute(&mut host, &call("get_task", "{\"task\":3}"), Language::En),
            "{\"ok\":true}"
        );
        assert_eq!(
            execute(&mut host, &call("project_summary", ""), Language::En),
            "{\"ok\":true}"
        );
        assert!(execute(&mut host, &call("get_task", "{oops"), Language::En).contains("param.invalid"));
        assert_eq!(
            execute(&mut host, &call("ui_state", "null"), Language::En),
            "{\"ok\":true}"
        );
        assert_eq!(host.0[0], ("task.get".to_owned(), json!({ "task": 3 })));
        assert_eq!(host.0[2].0, UI_STATE_ID);
    }

    #[test]
    fn long_result_truncated() {
        let text = "é".repeat(MAX_RESULT_BYTES);
        let out = bounded(&text, Language::Fr);
        assert!(out.len() < MAX_RESULT_BYTES + 200);
        assert!(out.ends_with("filtrer]"));
    }
}
