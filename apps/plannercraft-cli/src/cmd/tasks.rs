//! `plannercraft tasks <plan> [--critical] [--late] [--from D] [--to D]
//! [--resource N] [--limit N] [--json]` : liste des tâches (`task.list`).
//!
//! `--resource N` : tâches affectées à la ressource d'ID `N` (colonne ID
//! de `plannercraft resources`), convertie en UID par sa position (J5).
//!
//! Sans `--limit`, toutes les pages sont lues et réunies en une seule vue
//! `ListView` (sans `next_cursor`) ; avec `--limit N`, une seule page de
//! `N` tâches au plus, telle que le MCP la rend (`next_cursor` s'il en
//! reste).

use std::process::ExitCode;

use clap::Args;
use plannercraft_automation::Workspace;
use plannercraft_core::Day;
use serde_json::{Map, Value, json};

use super::{Ctx, Run, duration_text, finish, open, out, out_json, report, resources, s};

/// Taille de page lue quand toutes les tâches sont demandées (borne du
/// registre).
const PAGE: u32 = 500;
/// Pages lues au plus (500 × 2 000 = 1 000 000 de tâches) : garde contre un
/// curseur qui ne progresserait pas.
const MAX_PAGES: usize = 2_000;

/// Arguments de `plannercraft tasks`.
#[derive(Debug, Args)]
pub struct TasksArgs {
    /// Fichier MSPDI (`.xml`).
    pub plan: String,
    /// Tâches critiques seulement.
    #[arg(long)]
    pub critical: bool,
    /// Tâches en retard seulement (R-236) : fin planifiée après l'échéance
    /// ou après la fin de la ligne de base 0 ; non achevée avec une fin
    /// planifiée au plus tard à la date d'état ; non commencée avec un début
    /// planifié avant la date d'état. Date d'état : celle du projet, sinon la
    /// date du jour (`--now`).
    #[arg(long)]
    pub late: bool,
    /// Tâches qui finissent ce jour ou après (`AAAA-MM-JJ`).
    #[arg(long, value_name = "AAAA-MM-JJ", value_parser = parse_day)]
    pub from: Option<Day>,
    /// Tâches qui commencent ce jour ou avant (`AAAA-MM-JJ`).
    #[arg(long, value_name = "AAAA-MM-JJ", value_parser = parse_day)]
    pub to: Option<Day>,
    /// Tâches affectées à cette ressource (ID de `plannercraft resources`).
    #[arg(long, value_name = "ID")]
    pub resource: Option<u32>,
    /// Une seule page de N tâches au plus (1 à 500). Hors plage : erreur
    /// d'usage (code 2), avant toute lecture du plan.
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(1..=500))]
    pub limit: Option<u32>,
    /// Sortie JSON (vue `ListView` du MCP).
    #[arg(long)]
    pub json: bool,
}

/// Jour `AAAA-MM-JJ` (option de la ligne de commande).
///
/// # Errors
///
/// Message d'usage.
pub fn parse_day(s: &str) -> Result<Day, String> {
    s.parse::<Day>().map_err(|e| e.to_string())
}

/// Exécute la sous-commande.
pub fn run(ctx: &Ctx, args: &TasksArgs) -> ExitCode {
    finish(tasks(ctx, args))
}

fn tasks(ctx: &Ctx, args: &TasksArgs) -> Run {
    let mut ws = ctx.workspace();
    open(&mut ws, &args.plan).map_err(|e| report(&e, args.json))?;
    let mut filter = Map::new();
    if args.critical {
        filter.insert("critical".to_owned(), Value::Bool(true));
    }
    if args.late {
        filter.insert("late".to_owned(), Value::Bool(true));
    }
    if let Some(d) = args.from {
        filter.insert("from".to_owned(), Value::String(d.to_string()));
    }
    if let Some(d) = args.to {
        filter.insert("to".to_owned(), Value::String(d.to_string()));
    }
    if let Some(id) = args.resource {
        let uid = resources::uid_of_id(&mut ws, id).map_err(|e| report(&e, args.json))?;
        filter.insert("resource".to_owned(), json!(uid));
    }
    let v = match args.limit {
        Some(limit) => {
            filter.insert("limit".to_owned(), json!(limit));
            ws.call("task.list", Value::Object(filter))
                .map_err(|e| report(&e, args.json))?
        }
        None => all_pages(&mut ws, &filter).map_err(|e| report(&e, args.json))?,
    };
    if args.json {
        out_json(&v)
    } else {
        out(&text(ctx, &v))
    }
}

/// Toutes les pages réunies : `{tasks, total}`.
fn all_pages(
    ws: &mut Workspace,
    filter: &Map<String, Value>,
) -> Result<Value, plannercraft_automation::ToolError> {
    let mut tasks: Vec<Value> = Vec::new();
    let mut total = Value::from(0);
    let mut cursor: Option<Value> = None;
    for _ in 0..MAX_PAGES {
        let mut params = filter.clone();
        params.insert("limit".to_owned(), json!(PAGE));
        if let Some(c) = cursor.take() {
            params.insert("cursor".to_owned(), c);
        }
        let mut page = ws.call("task.list", Value::Object(params))?;
        if let Some(t) = page.get("total") {
            total = t.clone();
        }
        if let Some(Value::Array(list)) = page.get_mut("tasks").map(Value::take) {
            tasks.extend(list);
        }
        match page.get("next_cursor") {
            Some(c) if !c.is_null() => cursor = Some(c.clone()),
            _ => break,
        }
    }
    Ok(json!({ "tasks": tasks, "total": total }))
}

/// Tableau lisible : ID, nom (indenté par niveau), durée, début, fin,
/// marge totale ; `*` devant une tâche critique.
fn text(ctx: &Ctx, v: &Value) -> String {
    let t = |fr, en| ctx.tr(fr, en);
    let empty = Vec::new();
    let list = v.get("tasks").and_then(Value::as_array).unwrap_or(&empty);
    let rows: Vec<[String; 6]> = list
        .iter()
        .map(|task| {
            let level = task
                .get("level")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .saturating_sub(1);
            let indent = "  ".repeat(usize::try_from(level).unwrap_or(0).min(32));
            let mark = if task.get("critical").and_then(Value::as_bool) == Some(true) {
                "*"
            } else {
                " "
            };
            let id = task.get("id").and_then(Value::as_u64).unwrap_or(0);
            [
                format!("{mark}{id}"),
                format!("{indent}{}", s(task, "name")),
                duration_text(task, "duration").to_owned(),
                s(task, "start").to_owned(),
                s(task, "finish").to_owned(),
                duration_text(task, "total_slack").to_owned(),
            ]
        })
        .collect();
    let header = [
        t("ID", "ID").to_owned(),
        t("Nom", "Name").to_owned(),
        t("Durée", "Duration").to_owned(),
        t("Début", "Start").to_owned(),
        t("Fin", "Finish").to_owned(),
        t("Marge totale", "Total slack").to_owned(),
    ];
    let mut widths = [0usize; 6];
    for row in std::iter::once(&header).chain(rows.iter()) {
        for (w, cell) in widths.iter_mut().zip(row.iter()) {
            *w = (*w).max(cell.chars().count());
        }
    }
    let mut lines: Vec<String> = std::iter::once(&header)
        .chain(rows.iter())
        .map(|row| {
            let cells: Vec<String> = row
                .iter()
                .zip(widths.iter())
                .map(|(cell, w)| format!("{cell:<w$}"))
                .collect();
            cells.join("  ").trim_end().to_owned()
        })
        .collect();
    let total = v.get("total").and_then(Value::as_u64).unwrap_or(0);
    let mut footer = format!("{} {}", list.len(), t("tâche(s)", "task(s)"));
    if u64::try_from(list.len()).unwrap_or(u64::MAX) < total {
        footer = format!("{footer} {} {total}", t("sur", "of"));
    }
    lines.push(footer);
    lines.push(String::new());
    lines.join("\n")
}
