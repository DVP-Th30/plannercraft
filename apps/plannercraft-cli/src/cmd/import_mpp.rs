//! `plannercraft import-mpp <projet.mpp> -o out.xml [--json] [--dry-run]` :
//! fichier binaire de MS Project (MPP14 : Project 2010 et suivants) lu par
//! `plannercraft-mpp` (ADR-0036, lot M1), dates recalculées par le moteur
//! et comparées aux dates enregistrées par MS Project (R-291), puis projet
//! enregistré en MSPDI (écriture atomique). Rapport sur la sortie standard
//! (texte, ou JSON avec `--json`). Fichier refusé (pas un `.mpp`, endommagé,
//! version non prise en charge) : code de sortie 4 ([`exit::IMPORT_REFUSED`]).

use std::path::Path;
use std::process::ExitCode;

use clap::Args;
use plannercraft_automation::files::{read_path, write_atomic};
use plannercraft_automation::text::Language;
use plannercraft_automation::{DocOrigin, Session};
use plannercraft_mpp::{ReadOptions, compare, read, report};
use serde_json::json;

use super::{Ctx, Run, err_line, finish, out, out_json, warn};
use crate::exit;

/// Arguments de `plannercraft import-mpp`.
#[derive(Debug, Args)]
pub struct ImportMppArgs {
    /// Fichier `.mpp` (MS Project 2010 et suivants).
    pub input: String,
    /// Fichier MSPDI écrit (obligatoire sauf avec `--dry-run`).
    #[arg(
        short = 'o',
        long = "output",
        value_name = "FICHIER",
        required_unless_present = "dry_run"
    )]
    pub output: Option<String>,
    /// Rapport JSON sur la sortie standard.
    #[arg(long)]
    pub json: bool,
    /// Lit et compare sans écrire de fichier.
    #[arg(long)]
    pub dry_run: bool,
}

/// Taille lue au plus (borne de la lecture).
const MAX_MPP_BYTES: u64 = 256 * 1024 * 1024;

/// Exécute la sous-commande.
pub fn run(ctx: &Ctx, args: &ImportMppArgs) -> ExitCode {
    finish(import_mpp(ctx, args))
}

fn import_mpp(ctx: &Ctx, args: &ImportMppArgs) -> Run {
    let t = |fr, en| ctx.tr(fr, en);
    let fr = ctx.lang == Language::Fr;
    let path = Path::new(&args.input);
    let bytes = read_path(path, MAX_MPP_BYTES).map_err(|e| {
        warn(&format!("{} : {}", args.input, e.message(ctx.lang)));
        ExitCode::from(exit::FILE)
    })?;
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let doc = match read(&bytes, &ReadOptions::new(name.clone(), ctx.now)) {
        Ok(d) => d,
        Err(e) => {
            warn(&format!("{} : {}", e.code(), e.message(fr)));
            if args.json {
                out_json(&json!({ "error": { "code": e.code(), "message": e.message(fr) } }))?;
            }
            return Err(ExitCode::from(exit::IMPORT_REFUSED));
        }
    };
    let session = Session::from_project(
        doc.project.clone(),
        DocOrigin { name, path: None },
        ctx.now,
        ctx.lang,
    );
    let schedule = plannercraft_core::schedule(session.project());
    let comparison = compare(session.project(), &schedule, &doc.recorded);
    let mut saved = None;
    if let (Some(output), false) = (&args.output, args.dry_run) {
        let written = session.save_bytes(&plannercraft_mspdi::write::WriteOptions::default());
        write_atomic(Path::new(output), &written.xml).map_err(|e| {
            warn(&format!("{output} : {}", e.message(ctx.lang)));
            ExitCode::from(exit::FILE)
        })?;
        saved = Some(output.clone());
    }
    if args.json {
        let mut v = serde_json::to_value(report::view(&doc, Some(&comparison))).map_err(|e| {
            warn(&e.to_string());
            ExitCode::from(exit::REFUSED)
        })?;
        if let (Some(obj), Some(p)) = (v.as_object_mut(), &saved) {
            obj.insert("path".to_owned(), json!(p));
        }
        return out_json(&v);
    }
    out(&report::text(&doc, Some(&comparison), fr))?;
    if let Some(p) = saved {
        err_line(&format!("{} : {p}", t("Projet enregistré", "Project saved")));
    }
    Ok(())
}
