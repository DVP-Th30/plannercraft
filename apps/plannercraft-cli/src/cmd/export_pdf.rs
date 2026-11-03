//! `plannercraft export-pdf <plan> <sortie.pdf> [--style T] [--paper A3]
//! [--landscape | --portrait] [--pages-wide N] [--from D] [--to D]
//! [--critical] [--outline-max N] [--json]` : planning en PDF
//! (`project.export_pdf`, même export que le MCP ; J8, lot L7). Le plan
//! n'est pas modifié ; le PDF est écrit de façon atomique.

use std::process::ExitCode;

use clap::{Args, ValueEnum};
use plannercraft_automation::ToolError;
use plannercraft_automation::error::codes;
use plannercraft_core::Day;
use serde_json::{Map, Value, json};

use super::style::TemplateArg;
use super::tasks::parse_day;
use super::{Ctx, Run, finish, n, open, out, out_json, report, warn};

/// Papier (`page.paper` du registre).
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum PaperArg {
    /// A4.
    #[value(name = "A4", alias = "a4")]
    A4,
    /// A3.
    #[value(name = "A3", alias = "a3")]
    A3,
    /// Lettre US.
    Letter,
    /// Tabloïd US.
    Tabloid,
}

impl PaperArg {
    const fn as_str(self) -> &'static str {
        match self {
            Self::A4 => "a4",
            Self::A3 => "a3",
            Self::Letter => "letter",
            Self::Tabloid => "tabloid",
        }
    }
}

/// Arguments de `plannercraft export-pdf`.
#[derive(Debug, Args)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "drapeaux indépendants de la ligne de commande"
)]
pub struct ExportPdfArgs {
    /// Fichier MSPDI (`.xml`).
    pub plan: String,
    /// Fichier PDF écrit.
    pub output: String,
    /// Modèle de mise en forme (défaut : style du document, sinon
    /// Classique).
    #[arg(long, value_enum)]
    pub style: Option<TemplateArg>,
    /// Papier (défaut : celui du style).
    #[arg(long, value_enum)]
    pub paper: Option<PaperArg>,
    /// Paysage.
    #[arg(long, conflicts_with = "portrait")]
    pub landscape: bool,
    /// Portrait.
    #[arg(long)]
    pub portrait: bool,
    /// Pages en largeur (1 à 20).
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u16).range(1..=20))]
    pub pages_wide: Option<u16>,
    /// Premier jour exporté (`AAAA-MM-JJ`).
    #[arg(long, value_name = "AAAA-MM-JJ", value_parser = parse_day, requires = "to")]
    pub from: Option<Day>,
    /// Dernier jour exporté (`AAAA-MM-JJ`).
    #[arg(long, value_name = "AAAA-MM-JJ", value_parser = parse_day, requires = "from")]
    pub to: Option<Day>,
    /// Tâches critiques seulement.
    #[arg(long)]
    pub critical: bool,
    /// Niveau hiérarchique maximal.
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u16).range(1..))]
    pub outline_max: Option<u16>,
    /// Sortie JSON (vue du MCP).
    #[arg(long)]
    pub json: bool,
}

/// Exécute la sous-commande.
pub fn run(ctx: &Ctx, args: &ExportPdfArgs) -> ExitCode {
    finish(export(ctx, args))
}

/// Paramètres de `project.export_pdf`.
fn params(args: &ExportPdfArgs) -> Value {
    let mut p = Map::new();
    p.insert("path".to_owned(), Value::String(args.output.clone()));
    if let Some(t) = args.style {
        p.insert("style".to_owned(), json!(t.as_str()));
    }
    let mut page = Map::new();
    if let Some(paper) = args.paper {
        page.insert("paper".to_owned(), json!(paper.as_str()));
    }
    if args.landscape || args.portrait {
        page.insert("landscape".to_owned(), Value::Bool(args.landscape));
    }
    if let Some(w) = args.pages_wide {
        page.insert("pages_wide".to_owned(), json!(w));
    }
    if !page.is_empty() {
        p.insert("page".to_owned(), Value::Object(page));
    }
    if let (Some(from), Some(to)) = (args.from, args.to) {
        p.insert(
            "range".to_owned(),
            json!({ "from": from.to_string(), "to": to.to_string() }),
        );
    }
    let mut rows = Map::new();
    if args.critical {
        rows.insert("critical_only".to_owned(), Value::Bool(true));
    }
    if let Some(level) = args.outline_max {
        rows.insert("outline_max".to_owned(), json!(level));
    }
    if !rows.is_empty() {
        p.insert("rows".to_owned(), Value::Object(rows));
    }
    Value::Object(p)
}

fn export(ctx: &Ctx, args: &ExportPdfArgs) -> Run {
    let fail = |e: ToolError| report(&e, args.json);
    let mut ws = ctx.workspace();
    open(&mut ws, &args.plan).map_err(fail)?;
    let v = ws.call("project.export_pdf", params(args)).map_err(fail)?;
    if args.json {
        return out_json(&v);
    }
    let t = |fr, en| ctx.tr(fr, en);
    let missing = v.get("missing_glyphs").and_then(Value::as_str).unwrap_or("");
    if !missing.is_empty() {
        warn(&format!(
            "{} : {} {missing}",
            codes::PDF_GLYPH_MISSING,
            t(
                "caractères absents des polices",
                "characters missing from the fonts"
            ),
        ));
    }
    if v.get("truncated_days").and_then(Value::as_bool) == Some(true) {
        warn(t("plage de dates tronquée", "date range truncated"));
    }
    out(&format!(
        "{} : {} {}, {} {}, {} {}, {} %\n",
        v.get("path").and_then(Value::as_str).unwrap_or(&args.output),
        n(&v, "pages"),
        t("page(s)", "page(s)"),
        n(&v, "shown"),
        t("tâche(s)", "task(s)"),
        n(&v, "bytes"),
        t("octets", "bytes"),
        n(&v, "scale_pct"),
    ))
}
