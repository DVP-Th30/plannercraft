//! `plannercraft resources <plan> [--overallocated] [--json]` : liste des
//! ressources (`resource.list`, J5).
//!
//! Toutes les pages sont lues et réunies en une seule vue
//! `ResourceListView` (sans `next_cursor`). Travail, pic et surutilisation
//! viennent du moteur (R-184 à R-186).

use std::process::ExitCode;

use clap::Args;
use plannercraft_automation::error::codes;
use plannercraft_automation::{ToolError, Workspace};
use serde_json::{Map, Value, json};

use super::{Ctx, Run, duration_text, finish, open, out, out_json, report, s};

/// Taille de page lue (borne du registre).
const PAGE: u32 = 500;
/// Pages lues au plus : garde contre un curseur qui ne progresserait pas.
const MAX_PAGES: usize = 2_000;

/// Arguments de `plannercraft resources`.
#[derive(Debug, Args)]
pub struct ResourcesArgs {
    /// Fichier MSPDI (`.xml`).
    pub plan: String,
    /// Ressources surutilisées seulement.
    #[arg(long)]
    pub overallocated: bool,
    /// Sortie JSON (vue `ResourceListView` du MCP).
    #[arg(long)]
    pub json: bool,
}

/// Exécute la sous-commande.
pub fn run(ctx: &Ctx, args: &ResourcesArgs) -> ExitCode {
    finish(resources(ctx, args))
}

fn resources(ctx: &Ctx, args: &ResourcesArgs) -> Run {
    let mut ws = ctx.workspace();
    open(&mut ws, &args.plan).map_err(|e| report(&e, args.json))?;
    let mut filter = Map::new();
    if args.overallocated {
        filter.insert("overallocated".to_owned(), Value::Bool(true));
    }
    let v = all_pages(&mut ws, &filter).map_err(|e| report(&e, args.json))?;
    if args.json {
        out_json(&v)
    } else {
        out(&text(ctx, &v))
    }
}

/// Toutes les pages de `resource.list` réunies : `{resources, total}`.
///
/// # Errors
///
/// L'erreur d'outil de `resource.list`.
pub fn all_pages(ws: &mut Workspace, filter: &Map<String, Value>) -> Result<Value, ToolError> {
    let mut resources: Vec<Value> = Vec::new();
    let mut total = Value::from(0);
    let mut cursor: Option<Value> = None;
    for _ in 0..MAX_PAGES {
        let mut params = filter.clone();
        params.insert("limit".to_owned(), json!(PAGE));
        if let Some(c) = cursor.take() {
            params.insert("cursor".to_owned(), c);
        }
        let mut page = ws.call("resource.list", Value::Object(params))?;
        if let Some(t) = page.get("total") {
            total = t.clone();
        }
        if let Some(Value::Array(list)) = page.get_mut("resources").map(Value::take) {
            resources.extend(list);
        }
        match page.get("next_cursor") {
            Some(c) if !c.is_null() => cursor = Some(c.clone()),
            _ => break,
        }
    }
    Ok(json!({ "resources": resources, "total": total }))
}

/// UID de la ressource d'ID `id` (position + 1, colonne ID de
/// `plannercraft resources`), lue dans la session de `ws`.
///
/// # Errors
///
/// `resource.not_found` (champ `resource`) si aucune ressource n'a cet ID ;
/// l'erreur d'outil de `resource.list`.
pub fn uid_of_id(ws: &mut Workspace, id: u32) -> Result<u64, ToolError> {
    let all = all_pages(ws, &Map::new())?;
    all.get("resources")
        .and_then(Value::as_array)
        .and_then(|list| {
            list.iter()
                .find(|r| r.get("id").and_then(Value::as_u64) == Some(u64::from(id)))
        })
        .and_then(|r| r.get("uid").and_then(Value::as_u64))
        .ok_or_else(|| {
            let message = match ws.lang() {
                plannercraft_automation::text::Language::Fr => format!("aucune ressource d'ID {id}"),
                plannercraft_automation::text::Language::En => format!("no resource with ID {id}"),
            };
            ToolError::simple(codes::RESOURCE_NOT_FOUND, message).with_field("resource")
        })
}

/// Tableau lisible : ID, nom, type, capacité, travail, pic ; `!` devant
/// une ressource surutilisée.
fn text(ctx: &Ctx, v: &Value) -> String {
    let t = |fr, en| ctx.tr(fr, en);
    let empty = Vec::new();
    let list = v.get("resources").and_then(Value::as_array).unwrap_or(&empty);
    let units_text = |r: &Value, key: &str| -> String {
        r.get(key)
            .and_then(|u| u.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    };
    let rows: Vec<[String; 6]> = list
        .iter()
        .map(|r| {
            let mark = if r.get("overallocated").and_then(Value::as_bool) == Some(true) {
                "!"
            } else {
                " "
            };
            let id = r.get("id").and_then(Value::as_u64).unwrap_or(0);
            let kind = match s(r, "kind") {
                "material" => t("matérielle", "material"),
                "cost" => t("coût", "cost"),
                _ => t("travail", "work"),
            };
            [
                format!("{mark}{id}"),
                s(r, "name").to_owned(),
                kind.to_owned(),
                units_text(r, "max_units"),
                duration_text(r, "work").to_owned(),
                units_text(r, "peak"),
            ]
        })
        .collect();
    let header = [
        t("ID", "ID").to_owned(),
        t("Nom", "Name").to_owned(),
        t("Type", "Type").to_owned(),
        t("Capacité", "Max units").to_owned(),
        t("Travail", "Work").to_owned(),
        t("Pic", "Peak").to_owned(),
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
    lines.push(format!("{} {}", list.len(), t("ressource(s)", "resource(s)")));
    lines.push(String::new());
    lines.join("\n")
}
