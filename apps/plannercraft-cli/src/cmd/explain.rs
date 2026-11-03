//! `plannercraft explain <plan> --task N [--finish] [--depth N] [--json]` :
//! explication d'une date (`analysis.explain_date`, R-230 à R-233, J6).
//!
//! `--task N` désigne la tâche par son ID (colonne ID de `plannercraft tasks`),
//! converti en UID par `Project::task_by_id` (ID 0 : récapitulative du
//! projet). Texte par défaut : le résumé du moteur dans la langue choisie,
//! puis le côté au plus tard ; `--json` : la vue `ExplainView` du MCP. Toute
//! date vient du moteur (règle 3).

use std::process::ExitCode;

use clap::Args;
use plannercraft_automation::error::codes;
use plannercraft_automation::text::Language;
use plannercraft_automation::{ToolError, Workspace};
use plannercraft_core::{HasUid, TaskId, TaskUid};
use serde_json::{Map, Value};

use super::{Ctx, Run, duration_text, finish, open, out, out_json, report, s};

/// Arguments de `plannercraft explain`.
#[derive(Debug, Args)]
pub struct ExplainArgs {
    /// Fichier MSPDI (`.xml`).
    pub plan: String,
    /// Tâche, par son ID (0 : récapitulative du projet).
    #[arg(long, value_name = "ID")]
    pub task: u32,
    /// Explique la fin (défaut : le début).
    #[arg(long)]
    pub finish: bool,
    /// Profondeur de la chaîne des causes (1 à 50, défaut 5).
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u8).range(1..=50))]
    pub depth: Option<u8>,
    /// Sortie JSON (vue `ExplainView` du MCP).
    #[arg(long)]
    pub json: bool,
}

/// Exécute la sous-commande.
pub fn run(ctx: &Ctx, args: &ExplainArgs) -> ExitCode {
    finish(explain(ctx, args))
}

fn explain(ctx: &Ctx, args: &ExplainArgs) -> Run {
    let mut ws = ctx.workspace();
    open(&mut ws, &args.plan).map_err(|e| report(&e, args.json))?;
    let uid = uid_of_id(&ws, args.task).map_err(|e| report(&e, args.json))?;
    let mut params = Map::new();
    params.insert("task".to_owned(), Value::from(uid.get()));
    if args.finish {
        params.insert("side".to_owned(), Value::String("finish".to_owned()));
    }
    if let Some(d) = args.depth {
        params.insert("depth".to_owned(), Value::from(d));
    }
    let v = ws
        .call("analysis.explain_date", Value::Object(params))
        .map_err(|e| report(&e, args.json))?;
    if args.json {
        out_json(&v)
    } else {
        out(&text(ctx, &v))
    }
}

/// UID de la tâche d'ID `id` (`Project::task_by_id`) ; l'ID 0 désigne la
/// récapitulative du projet.
fn uid_of_id(ws: &Workspace, id: u32) -> Result<TaskUid, ToolError> {
    if id == 0 {
        return Ok(TaskUid::PROJECT_SUMMARY);
    }
    ws.session()
        .and_then(|s| s.project().task_by_id(TaskId::new(id)))
        .filter(|t| !t.is_null)
        .map(HasUid::uid)
        .ok_or_else(|| {
            let message = match ws.lang() {
                Language::Fr => format!("aucune tâche d'ID {id}"),
                Language::En => format!("no task with ID {id}"),
            };
            ToolError::simple(codes::TASK_NOT_FOUND, message).with_field("task")
        })
}

/// Résumé du moteur, puis le côté au plus tard (marges, criticité).
fn text(ctx: &Ctx, v: &Value) -> String {
    let t = |fr, en| ctx.tr(fr, en);
    let summary = match ctx.lang {
        Language::Fr => s(v, "summary_fr"),
        Language::En => s(v, "summary_en"),
    };
    let late = v.get("late").unwrap_or(&Value::Null);
    let critical = late.get("critical").and_then(Value::as_bool) == Some(true);
    let lines = [
        summary.to_owned(),
        format!(
            "  {} : {} – {}",
            t("Au plus tard", "Late"),
            s(late, "start"),
            s(late, "finish")
        ),
        format!(
            "  {} : {} ; {} : {} ; {}",
            t("Marge totale", "Total slack"),
            duration_text(late, "total_slack"),
            t("marge libre", "free slack"),
            duration_text(late, "free_slack"),
            if critical {
                t("critique", "critical")
            } else {
                t("non critique", "not critical")
            }
        ),
        String::new(),
    ];
    lines.join("\n")
}
