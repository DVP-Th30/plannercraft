//! `plannercraft apply <plan> <ops.json> [--dry-run] [-o out.xml] [--json]` :
//! applique un lot de commandes (`batch.apply`, format versionné unique,
//! ADR-0012 D6).
//!
//! - `--dry-run` : rien n'est écrit, même avec `-o` ; la sortie montre le
//!   diff qu'aurait produit le lot.
//! - `-o out.xml` : le résultat est écrit dans `out.xml`, `<plan>` reste
//!   intact.
//! - sinon : `<plan>` est remplacé de façon atomique (temporaire du même
//!   dossier, `fsync`, renommage : `files::write_atomic`).
//!
//! Lot refusé : code 1, aucun fichier touché.

use std::process::ExitCode;

use clap::Args;
use plannercraft_automation::ToolError;
use plannercraft_automation::error::codes;
use serde_json::{Value, json};

use super::{Ctx, Run, err_line, finish, n, open, out, out_json, report, s};

/// Taille maximale d'un fichier de lot lu (64 Mio).
pub const MAX_OPS_BYTES: u64 = 64 * 1024 * 1024;

/// Arguments de `plannercraft apply`.
#[derive(Debug, Args)]
pub struct ApplyArgs {
    /// Fichier MSPDI (`.xml`).
    pub plan: String,
    /// Lot JSON : `{"format_version":1,"ops":[{"op":"task.add",…}]}`.
    pub ops: String,
    /// Simulation : rien n'est écrit.
    #[arg(long)]
    pub dry_run: bool,
    /// Écrit le résultat dans ce fichier au lieu de remplacer `<plan>`.
    #[arg(short = 'o', long = "output", value_name = "FICHIER")]
    pub output: Option<String>,
    /// Sortie JSON (vue `OutcomeView` du MCP).
    #[arg(long)]
    pub json: bool,
}

/// Exécute la sous-commande.
pub fn run(ctx: &Ctx, args: &ApplyArgs) -> ExitCode {
    finish(apply(ctx, args))
}

fn apply(ctx: &Ctx, args: &ApplyArgs) -> Run {
    let fail = |e: ToolError| report(&e, args.json);
    let mut params = read_ops(ctx, &args.ops).map_err(fail)?;
    let mut ws = ctx.workspace();
    open(&mut ws, &args.plan).map_err(fail)?;
    if args.dry_run
        && let Value::Object(m) = &mut params
    {
        m.insert("dry_run".to_owned(), Value::Bool(true));
    }
    let outcome = ws.call("batch.apply", params).map_err(fail)?;
    let committed = outcome.get("committed").and_then(Value::as_bool) == Some(true);
    let mut written = None;
    if committed {
        let save = match &args.output {
            Some(path) => json!({ "path": path }),
            None => json!({}),
        };
        let saved = ws.call("project.save", save).map_err(fail)?;
        written = Some(s(&saved, "path").to_owned());
    }
    if args.json {
        out_json(&outcome)
    } else {
        out(&text(ctx, &outcome, written.as_deref()))
    }
}

/// Lit et décode le lot. Fichier illisible : `file.*` (code 3) ; JSON mal
/// formé : `param.invalid` (code 1, comme un lot mal formé du MCP).
fn read_ops(ctx: &Ctx, path: &str) -> Result<Value, ToolError> {
    let bytes = plannercraft_automation::files::read_path(std::path::Path::new(path), MAX_OPS_BYTES)
        .map_err(|e| ToolError::simple(e.code(), e.message(ctx.lang)).with_field("ops"))?;
    serde_json::from_slice::<Value>(&bytes).map_err(|e| {
        ToolError::simple(
            codes::PARAM_INVALID,
            format!(
                "{} {path} : {e}",
                ctx.tr("lot JSON illisible :", "unreadable JSON batch:")
            ),
        )
        .with_field("ops")
    })
}

/// Compte rendu lisible : appliqué ou simulé, fin avant → après, tâches
/// décalées, avertissements, fichier écrit.
fn text(ctx: &Ctx, o: &Value, written: Option<&str>) -> String {
    let t = |fr, en| ctx.tr(fr, en);
    let null = Value::Null;
    let diff = o.get("diff").unwrap_or(&null);
    let mut lines = Vec::new();
    lines.push(
        if written.is_some() {
            t("Lot appliqué.", "Batch applied.")
        } else {
            t("Simulation : rien n'est écrit.", "Dry run: nothing written.")
        }
        .to_owned(),
    );
    let created = o
        .get("created_uids")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if created > 0 {
        lines.push(format!("  {} : {created}", t("Tâches créées", "Tasks created")));
    }
    lines.push(format!(
        "  {} : {} → {} ({})",
        t("Fin du projet", "Project finish"),
        s(diff, "finish_before"),
        s(diff, "finish_after"),
        super::duration_text(diff, "finish_delta"),
    ));
    lines.push(format!(
        "  {} : {}",
        t("Tâches décalées", "Tasks moved"),
        n(diff, "changed_total")
    ));
    if let Some(ws) = o.get("warnings").and_then(Value::as_array) {
        for w in ws {
            let line = format!("  ! {} : {}", s(w, "code"), s(w, "message"));
            err_line(&line);
        }
    }
    if let Some(path) = written {
        lines.push(format!("  {} : {path}", t("Écrit", "Written")));
    }
    lines.push(String::new());
    lines.join("\n")
}
