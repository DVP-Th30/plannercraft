//! `plannercraft summary <plan> [--json]` : résumé du projet (`project.summary`).

use std::process::ExitCode;

use clap::Args;
use serde_json::{Value, json};

use super::{Ctx, Run, duration_text, finish, n, open, out, out_json, report, s};

/// Arguments de `plannercraft summary`.
#[derive(Debug, Args)]
pub struct SummaryArgs {
    /// Fichier MSPDI (`.xml`).
    pub plan: String,
    /// Sortie JSON (vue `SummaryView` du MCP).
    #[arg(long)]
    pub json: bool,
}

/// Exécute la sous-commande.
pub fn run(ctx: &Ctx, args: &SummaryArgs) -> ExitCode {
    finish(summary(ctx, args))
}

fn summary(ctx: &Ctx, args: &SummaryArgs) -> Run {
    let mut ws = ctx.workspace();
    open(&mut ws, &args.plan).map_err(|e| report(&e, args.json))?;
    let v = ws
        .call("project.summary", json!({}))
        .map_err(|e| report(&e, args.json))?;
    if args.json {
        out_json(&v)
    } else {
        out(&text(ctx, &v))
    }
}

/// Résumé lisible, fait des valeurs de la vue.
fn text(ctx: &Ctx, v: &Value) -> String {
    let t = |fr, en| ctx.tr(fr, en);
    let mut lines = vec![
        s(v, "name").to_owned(),
        format!("  {} : {}", t("Début", "Start"), s(v, "start")),
        format!("  {} : {}", t("Fin", "Finish"), s(v, "finish")),
        format!("  {} : {}", t("Durée", "Duration"), duration_text(v, "duration")),
        format!(
            "  {} : {} ({} {}, {} {}, {} {})",
            t("Tâches", "Tasks"),
            n(v, "tasks"),
            n(v, "summaries"),
            t("récapitulatives", "summaries"),
            n(v, "milestones"),
            t("jalons", "milestones"),
            n(v, "critical"),
            t("critiques", "critical"),
        ),
        format!("  {} : {}", t("Liens", "Links"), n(v, "links")),
        format!(
            "  {} : {} {}, {} {}",
            t("Diagnostics", "Diagnostics"),
            n(v, "errors"),
            t("erreur(s)", "error(s)"),
            n(v, "warnings"),
            t("avertissement(s)", "warning(s)"),
        ),
    ];
    lines.push(String::new());
    lines.join("\n")
}
