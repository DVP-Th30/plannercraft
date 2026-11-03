//! `plannercraft render <plan> -o x.svg [--from D] [--to D] [--critical]
//! [--zoom Z]` : Gantt en SVG (`render.gantt`, même rendu que le MCP,
//! 200 lignes au plus), écrit de façon atomique.

use std::process::ExitCode;

use clap::{Args, ValueEnum};
use plannercraft_automation::ToolError;
use plannercraft_core::Day;
use serde_json::{Map, Value};

use super::tasks::parse_day;
use super::{Ctx, Run, finish, n, open, out, report, warn};

/// Échelle de temps (`zoom` du registre).
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ZoomArg {
    /// Choisie selon l'étendue (1 200 px visés).
    Auto,
    /// Jours.
    Day,
    /// Semaines.
    Week,
    /// Mois.
    Month,
    /// Trimestres.
    Quarter,
}

impl ZoomArg {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
            Self::Quarter => "quarter",
        }
    }
}

/// Arguments de `plannercraft render`.
#[derive(Debug, Args)]
pub struct RenderArgs {
    /// Fichier MSPDI (`.xml`).
    pub plan: String,
    /// Fichier SVG écrit.
    #[arg(short = 'o', long = "output", value_name = "FICHIER")]
    pub output: String,
    /// Premier jour affiché (`AAAA-MM-JJ`, défaut : début du projet).
    #[arg(long, value_name = "AAAA-MM-JJ", value_parser = parse_day)]
    pub from: Option<Day>,
    /// Dernier jour affiché (`AAAA-MM-JJ`, défaut : fin du projet).
    #[arg(long, value_name = "AAAA-MM-JJ", value_parser = parse_day)]
    pub to: Option<Day>,
    /// Tâches critiques seulement.
    #[arg(long)]
    pub critical: bool,
    /// Échelle de temps.
    #[arg(long, value_enum)]
    pub zoom: Option<ZoomArg>,
}

/// Exécute la sous-commande.
pub fn run(ctx: &Ctx, args: &RenderArgs) -> ExitCode {
    finish(render(ctx, args))
}

fn render(ctx: &Ctx, args: &RenderArgs) -> Run {
    let fail = |e: ToolError| report(&e, false);
    let mut ws = ctx.workspace();
    open(&mut ws, &args.plan).map_err(fail)?;
    let mut params = Map::new();
    if let Some(d) = args.from {
        params.insert("from".to_owned(), Value::String(d.to_string()));
    }
    if let Some(d) = args.to {
        params.insert("to".to_owned(), Value::String(d.to_string()));
    }
    if args.critical {
        params.insert("critical_only".to_owned(), Value::Bool(true));
    }
    if let Some(z) = args.zoom {
        params.insert("zoom".to_owned(), Value::String(z.as_str().to_owned()));
    }
    let v = ws.call("render.gantt", Value::Object(params)).map_err(fail)?;
    let svg = v.get("svg").and_then(Value::as_str).unwrap_or("");
    plannercraft_automation::files::write_atomic(std::path::Path::new(&args.output), svg.as_bytes())
        .map_err(|e| fail(ToolError::simple(e.code(), e.message(ctx.lang)).with_field("output")))?;
    let t = |fr, en| ctx.tr(fr, en);
    if v.get("truncated_days").and_then(Value::as_bool) == Some(true) {
        warn(t("plage de dates tronquée", "date range truncated"));
    }
    if v.get("truncated_bytes").and_then(Value::as_bool) == Some(true) {
        warn(t("rendu tronqué (taille)", "render truncated (size)"));
    }
    out(&format!(
        "{} : {} {}, {} {}, {} {}\n",
        args.output,
        n(&v, "shown"),
        t("ligne(s)", "row(s)"),
        n(&v, "hidden"),
        t("masquée(s)", "hidden"),
        n(&v, "bytes"),
        t("octets", "bytes"),
    ))
}
