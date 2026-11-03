//! `plannercraft import-csv <in.csv> -o out.xml` : projet construit depuis un CSV
//! de tâches (`project.import_csv`, R-144 à R-146), planifié depuis
//! « maintenant », puis enregistré en MSPDI (`project.save`, écriture
//! atomique).

use std::process::ExitCode;

use clap::Args;
use plannercraft_automation::ToolError;
use serde_json::{Value, json};

use super::{Ctx, Run, err_line, finish, n, out, report, s};

/// Arguments de `plannercraft import-csv`.
#[derive(Debug, Args)]
pub struct ImportCsvArgs {
    /// Fichier CSV des tâches (séparateur et langue des en-têtes détectés).
    pub input: String,
    /// Fichier MSPDI écrit.
    #[arg(short = 'o', long = "output", value_name = "FICHIER")]
    pub output: String,
}

/// Exécute la sous-commande.
pub fn run(ctx: &Ctx, args: &ImportCsvArgs) -> ExitCode {
    finish(import(ctx, args))
}

fn import(ctx: &Ctx, args: &ImportCsvArgs) -> Run {
    let fail = |e: ToolError| report(&e, false);
    let mut ws = ctx.workspace();
    let opened = ws
        .call("project.import_csv", json!({ "path": args.input }))
        .map_err(fail)?;
    let saved = ws
        .call("project.save", json!({ "path": args.output }))
        .map_err(fail)?;
    if let Some(ds) = opened.get("diagnostics").and_then(Value::as_array) {
        for d in ds {
            err_line(&format!("  {} : {}", s(d, "code"), s(d, "message")));
        }
    }
    let null = Value::Null;
    let summary = opened.get("summary").unwrap_or(&null);
    out(&format!(
        "{} : {} {}, {} {} ({} {})\n",
        s(&saved, "path"),
        n(summary, "tasks"),
        ctx.tr("tâche(s)", "task(s)"),
        n(summary, "links"),
        ctx.tr("lien(s)", "link(s)"),
        n(&opened, "diagnostics_total"),
        ctx.tr("diagnostic(s)", "diagnostic(s)"),
    ))
}
