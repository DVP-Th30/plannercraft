//! Outils MCP générés depuis `REGISTRY` : nom, schéma d'entrée (plus
//! `dry_run` ou `detail`), annotations (lot L8).
//!
//! Seules les entrées disponibles (`Availability::Now`) sont listées ; une
//! entrée réservée à un jalon futur (aucune depuis le lot L7 de J8, qui a
//! activé `export_pdf`) resterait appelable par son nom et répondrait
//! `command.not_yet_available` (erreur structurée, C-MCP-05). Les outils de style (J8, lot L6 : `list_styles`,
//! `get_style`, `apply_style`, `set_bar_style`, `reset_style`,
//! `save_style_as_template`) sont listés ; aucun n'est destructif (une
//! annulation rend le style d'avant).

use std::sync::Arc;

use plannercraft_automation::text::Language;
use plannercraft_automation::{CommandSpec, Kind, REGISTRY};
use rmcp::model::{JsonObject, Tool, ToolAnnotations};
use serde_json::{Map, Value, json};

/// Outils listés, dans l'ordre du registre.
pub(crate) fn listed(lang: Language) -> Vec<Tool> {
    REGISTRY
        .iter()
        .filter(|s| s.is_listed())
        .map(|s| tool_for(s, lang))
        .collect()
}

/// Outil MCP d'une entrée du registre.
pub(crate) fn tool_for(spec: &CommandSpec, lang: Language) -> Tool {
    let annotations = match spec.kind {
        Kind::Read | Kind::Analysis => ToolAnnotations::new().read_only(true),
        Kind::Write | Kind::History | Kind::File => ToolAnnotations::new()
            .read_only(false)
            .destructive(is_destructive(spec.id))
            .idempotent(false)
            .open_world(false),
        // `app.*` (J7) : interface de l'application rattachée, sans effet
        // sur le projet ; `app.ui` change l'affichage.
        Kind::Ui => ToolAnnotations::new()
            .read_only(spec.id != "app.ui")
            .destructive(false)
            .open_world(false),
    };
    Tool::new(
        spec.mcp_name,
        spec.summary(lang),
        Arc::new(input_schema(spec, lang)),
    )
    .with_annotations(annotations)
}

/// Suppression d'un élément du modèle : annotation `destructive_hint`
/// (J5 : `delete_resource` ; J7 : `delete_calendar`, `clear_baseline`).
fn is_destructive(id: &str) -> bool {
    matches!(
        id,
        "task.delete" | "resource.delete" | "calendar.delete" | "baseline.clear"
    )
}

/// Schéma d'entrée : schéma des paramètres, plus `dry_run` et `detail`
/// pour une écriture (acceptés par `Workspace::call`).
pub(crate) fn input_schema(spec: &CommandSpec, lang: Language) -> JsonObject {
    let mut obj = match Value::from((spec.schema)()) {
        Value::Object(m) => m,
        // Schéma booléen (`true`) : tout objet.
        _ => Map::new(),
    };
    obj.insert("type".to_owned(), Value::String("object".to_owned()));
    if spec.dry_run {
        let props = obj
            .entry("properties")
            .or_insert_with(|| Value::Object(Map::new()));
        if let Value::Object(props) = props {
            props.insert(
                "dry_run".to_owned(),
                json!({ "type": "boolean", "default": false, "description": dry_run_text(lang) }),
            );
            props.insert(
                "detail".to_owned(),
                json!({ "type": "boolean", "default": false, "description": detail_text(lang) }),
            );
        }
    }
    obj
}

const fn dry_run_text(lang: Language) -> &'static str {
    match lang {
        Language::Fr => {
            "Simule sans rien modifier : renvoie le diff du planning calculé par le moteur, sans entrée d'historique."
        }
        Language::En => {
            "Simulate without changing anything: returns the schedule diff computed by the engine, with no history entry."
        }
    }
}

const fn detail_text(lang: Language) -> &'static str {
    match lang {
        Language::Fr => "Diff détaillé (jusqu'à 1 000 tâches au lieu des 10 plus décalées).",
        Language::En => "Detailed diff (up to 1,000 tasks instead of the 10 most shifted).",
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests : un échec doit s'arrêter net")]

    use super::*;

    /// C-MCP-07 : noms conformes à `^[a-zA-Z0-9_-]{1,64}$`.
    fn valid_name(n: &str) -> bool {
        (1..=64).contains(&n.len())
            && n.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    }

    #[test]
    fn tools_names_valid_and_unique() {
        let tools = listed(Language::En);
        assert_ne!(tools.len(), 0);
        let mut names: Vec<_> = tools.iter().map(|t| t.name.to_string()).collect();
        assert!(names.iter().all(|n| valid_name(n)), "{names:?}");
        names.sort();
        let before = names.len();
        names.dedup();
        assert_eq!(names.len(), before);
    }

    #[test]
    fn tools_destructive_hint_on_deletions() {
        for spec in REGISTRY.iter().filter(|s| s.is_listed()) {
            let tool = tool_for(spec, Language::Fr);
            let hint = tool.annotations.and_then(|a| a.destructive_hint);
            let expected = matches!(
                spec.id,
                "task.delete" | "resource.delete" | "calendar.delete" | "baseline.clear"
            );
            assert_eq!(hint == Some(true), expected, "{}", spec.id);
        }
    }

    #[test]
    fn tools_write_schema_has_dry_run_and_detail() {
        for spec in REGISTRY.iter().filter(|s| s.is_listed()) {
            let schema = input_schema(spec, Language::Fr);
            assert_eq!(schema.get("type"), Some(&json!("object")), "{}", spec.id);
            assert!(!schema.contains_key("$ref"), "{}", spec.id);
            let props = schema.get("properties").and_then(Value::as_object);
            let has = |k: &str| props.is_some_and(|p| p.contains_key(k));
            assert_eq!(has("dry_run"), spec.kind == Kind::Write, "{}", spec.id);
            // `what_if` (J6, R-234) porte son propre `detail` (diff détaillé
            // du lot simulé), sans `dry_run`.
            let detail = spec.kind == Kind::Write || spec.id == "analysis.what_if";
            assert_eq!(has("detail"), detail, "{}", spec.id);
        }
    }
}
