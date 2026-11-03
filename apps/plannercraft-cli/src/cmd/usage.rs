//! `plannercraft usage <plan> [--resource N]… [--by day|week|month] [--from D]
//! [--to D] [--json]` : utilisation des ressources par période
//! (`analysis.resource_usage`, R-188, J5).
//!
//! `--resource N` désigne une ressource par son ID (colonne ID de
//! `plannercraft resources`), converti en UID par sa position ; sans
//! `--resource`, toutes les ressources. `--from`/`--to` : `AAAA-MM-JJ`
//! (minuit) ou `AAAA-MM-JJTHH:MM` ; défaut : dates du projet. Travail et
//! surutilisation de chaque période viennent du moteur.

use std::process::ExitCode;

use clap::{Args, ValueEnum};
use plannercraft_core::Moment;
use serde_json::{Map, Value};

use super::{Ctx, Run, finish, open, out, out_json, report, resources, s};

/// Granularité (`--by`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum ByArg {
    /// Jour.
    Day,
    /// Semaine (défaut).
    #[default]
    Week,
    /// Mois.
    Month,
}

impl ByArg {
    const fn json(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
        }
    }
}

/// Arguments de `plannercraft usage`.
#[derive(Debug, Args)]
pub struct UsageArgs {
    /// Fichier MSPDI (`.xml`).
    pub plan: String,
    /// Ressource, par son ID (répétable) ; défaut : toutes.
    #[arg(long = "resource", value_name = "ID")]
    pub resources: Vec<u32>,
    /// Granularité des périodes.
    #[arg(long, value_enum, default_value_t = ByArg::Week)]
    pub by: ByArg,
    /// Début de la plage (`AAAA-MM-JJ` ou `AAAA-MM-JJTHH:MM`).
    #[arg(long, value_name = "DATE", value_parser = parse_moment)]
    pub from: Option<Moment>,
    /// Fin de la plage, exclue (`AAAA-MM-JJ` ou `AAAA-MM-JJTHH:MM`).
    #[arg(long, value_name = "DATE", value_parser = parse_moment)]
    pub to: Option<Moment>,
    /// Sortie JSON (vue `UsageView` du MCP).
    #[arg(long)]
    pub json: bool,
}

/// Instant `AAAA-MM-JJTHH:MM`, ou jour `AAAA-MM-JJ` (minuit).
///
/// # Errors
///
/// Message d'usage.
pub fn parse_moment(text: &str) -> Result<Moment, String> {
    let full = if text.len() == 10 {
        format!("{text}T00:00")
    } else {
        text.to_owned()
    };
    full.parse::<Moment>().map_err(|e| e.to_string())
}

/// Exécute la sous-commande.
pub fn run(ctx: &Ctx, args: &UsageArgs) -> ExitCode {
    finish(usage(ctx, args))
}

fn usage(ctx: &Ctx, args: &UsageArgs) -> Run {
    let mut ws = ctx.workspace();
    open(&mut ws, &args.plan).map_err(|e| report(&e, args.json))?;
    let mut params = Map::new();
    params.insert("by".to_owned(), Value::String(args.by.json().to_owned()));
    if let Some(m) = args.from {
        params.insert("from".to_owned(), Value::String(m.to_string()));
    }
    if let Some(m) = args.to {
        params.insert("to".to_owned(), Value::String(m.to_string()));
    }
    if !args.resources.is_empty() {
        let mut uids = Vec::with_capacity(args.resources.len());
        for id in &args.resources {
            uids.push(Value::from(
                resources::uid_of_id(&mut ws, *id).map_err(|e| report(&e, args.json))?,
            ));
        }
        params.insert("resources".to_owned(), Value::Array(uids));
    }
    let v = ws
        .call("analysis.resource_usage", Value::Object(params))
        .map_err(|e| report(&e, args.json))?;
    if args.json {
        out_json(&v)
    } else {
        out(&text(ctx, &v))
    }
}

/// Une section par ressource : une ligne par période (début, travail), `!`
/// sur une période surutilisée.
fn text(ctx: &Ctx, v: &Value) -> String {
    let empty = Vec::new();
    let periods = v.get("periods").and_then(Value::as_array).unwrap_or(&empty);
    let rows = v.get("rows").and_then(Value::as_array).unwrap_or(&empty);
    let mut lines = Vec::new();
    for row in rows {
        lines.push(s(row, "name").to_owned());
        let work = row.get("work").and_then(Value::as_array).unwrap_or(&empty);
        let over = row
            .get("overallocated")
            .and_then(Value::as_array)
            .unwrap_or(&empty);
        for (i, start) in periods.iter().enumerate() {
            let w = work
                .get(i)
                .and_then(|w| w.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let mark = if over.get(i).and_then(Value::as_bool) == Some(true) {
                "!"
            } else {
                " "
            };
            lines.push(format!("{mark} {}  {w}", start.as_str().unwrap_or("")));
        }
    }
    if v.get("truncated").and_then(Value::as_bool) == Some(true) {
        lines.push(
            ctx.tr(
                "(surutilisation coupée : trop d'intervalles)",
                "(overallocation truncated: too many intervals)",
            )
            .to_owned(),
        );
    }
    lines.push(String::new());
    lines.join("\n")
}
