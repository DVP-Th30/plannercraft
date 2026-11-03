//! `plannercraft style list|get|apply <plan>` : style du document (J8, lot L6 ;
//! `style.list`, `style.get`, `style.apply`, mêmes JSON que le MCP).
//!
//! - `list` : modèles intégrés, style du document ;
//! - `get` : style effectif (modèle, surcharges, contrastes insuffisants) ;
//! - `apply` : `--template T`, `--patch fichier.json` ou `--reset` ;
//!   `--dry-run` n'écrit rien ; sinon `<plan>` est remplacé de façon
//!   atomique, ou `-o out.xml` est écrit. Le fichier porte l'instruction
//!   `<?plannercraft-style?>` si un style est choisi (H-R12).

use std::process::ExitCode;

use clap::{Args, Subcommand, ValueEnum};
use plannercraft_automation::ToolError;
use plannercraft_automation::error::codes;
use serde_json::{Map, Value, json};

use super::{Ctx, Run, err_line, finish, open, out, out_json, report, s};

/// Taille maximale d'un fichier de surcharge lu (64 Kio, borne du style).
pub const MAX_PATCH_BYTES: u64 = 64 * 1024;

/// Sous-commandes de `plannercraft style`.
#[derive(Debug, Subcommand)]
pub enum StyleCommand {
    /// Modèles intégrés et style du document.
    List(ReadArgs),
    /// Style effectif du document.
    Get(ReadArgs),
    /// Applique un modèle, une surcharge, ou retire le style.
    Apply(ApplyArgs),
}

/// Arguments de `plannercraft style`.
#[derive(Debug, Args)]
pub struct StyleArgs {
    /// Action.
    #[command(subcommand)]
    pub command: StyleCommand,
}

/// Arguments d'une lecture.
#[derive(Debug, Args)]
pub struct ReadArgs {
    /// Fichier MSPDI (`.xml`).
    pub plan: String,
    /// Sortie JSON (vue du MCP).
    #[arg(long)]
    pub json: bool,
}

/// Modèle intégré (`template` du registre).
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum TemplateArg {
    /// Apparence de J7.
    Classique,
    /// Sobre.
    Sobre,
    /// Contraste élevé.
    ContrasteEleve,
    /// Présentation client.
    Presentation,
    /// Chantier.
    Chantier,
    /// Impression noir et blanc.
    ImpressionNb,
}

impl TemplateArg {
    /// Nom du modèle dans le registre.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Classique => "classique",
            Self::Sobre => "sobre",
            Self::ContrasteEleve => "contraste_eleve",
            Self::Presentation => "presentation",
            Self::Chantier => "chantier",
            Self::ImpressionNb => "impression_nb",
        }
    }
}

/// Arguments de `plannercraft style apply`.
#[derive(Debug, Args)]
pub struct ApplyArgs {
    /// Fichier MSPDI (`.xml`).
    pub plan: String,
    /// Modèle de base.
    #[arg(long, value_enum)]
    pub template: Option<TemplateArg>,
    /// Surcharge JSON (`StylePatch`, 64 Kio au plus).
    #[arg(long, value_name = "FICHIER")]
    pub patch: Option<String>,
    /// Retire le style du document (modèle par défaut).
    #[arg(long, conflicts_with_all = ["template", "patch"])]
    pub reset: bool,
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
pub fn run(ctx: &Ctx, args: &StyleArgs) -> ExitCode {
    finish(match &args.command {
        StyleCommand::List(a) => read(ctx, a, "style.list"),
        StyleCommand::Get(a) => read(ctx, a, "style.get"),
        StyleCommand::Apply(a) => apply(ctx, a),
    })
}

fn read(ctx: &Ctx, args: &ReadArgs, id: &str) -> Run {
    let fail = |e: ToolError| report(&e, args.json);
    let mut ws = ctx.workspace();
    open(&mut ws, &args.plan).map_err(fail)?;
    let v = ws.call(id, json!({})).map_err(fail)?;
    if args.json {
        return out_json(&v);
    }
    if id == "style.list" {
        out(&list_text(ctx, &v))
    } else {
        out(&get_text(ctx, &v))
    }
}

/// Liste lisible : un modèle par ligne, `*` devant celui du document.
fn list_text(ctx: &Ctx, v: &Value) -> String {
    let doc = v
        .get("document")
        .and_then(|d| d.get("base"))
        .and_then(Value::as_str);
    let default = s(v, "default");
    let mut lines = Vec::new();
    for t in v.get("templates").and_then(Value::as_array).into_iter().flatten() {
        let id = s(t, "id");
        let mark = if Some(id) == doc { '*' } else { ' ' };
        let note = if id == default {
            ctx.tr(" (défaut)", " (default)")
        } else {
            ""
        };
        lines.push(format!("{mark} {id} : {}{note}", s(t, "name")));
    }
    if doc.is_none() {
        lines.push(
            ctx.tr("Document sans style choisi.", "No style chosen for the document.")
                .to_owned(),
        );
    }
    lines.push(String::new());
    lines.join("\n")
}

/// Style lisible : modèle, provenance, surcharges, contrastes.
fn get_text(ctx: &Ctx, v: &Value) -> String {
    let t = |fr, en| ctx.tr(fr, en);
    let source = if s(v, "source") == "document" {
        t("document", "document")
    } else {
        t("défaut", "default")
    };
    let over = v.get("over").and_then(Value::as_object).map_or(0, Map::len);
    let mut lines = vec![
        format!("{} : {} ({source})", t("Modèle", "Template"), s(v, "base")),
        format!("  {} : {over}", t("Sections surchargées", "Overridden sections")),
    ];
    for i in v
        .get("contrast_issues")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let role = i.get("role").and_then(Value::as_str).unwrap_or("text");
        err_line(&format!(
            "  ! {} {role} ({}) : {} / {} ; {} {}",
            t("contraste", "contrast"),
            s(i, "variant"),
            i.get("ratio").and_then(Value::as_u64).unwrap_or(0),
            i.get("min").and_then(Value::as_u64).unwrap_or(0),
            t("proposé", "suggested"),
            s(i, "suggestion"),
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}

fn apply(ctx: &Ctx, args: &ApplyArgs) -> Run {
    let fail = |e: ToolError| report(&e, args.json);
    let mut params = Map::new();
    if let Some(t) = args.template {
        params.insert("template".to_owned(), Value::String(t.as_str().to_owned()));
    }
    if let Some(path) = &args.patch {
        params.insert("patch".to_owned(), read_patch(ctx, path).map_err(fail)?);
    }
    if args.reset {
        params.insert("reset".to_owned(), Value::Bool(true));
    }
    if args.dry_run {
        params.insert("dry_run".to_owned(), Value::Bool(true));
    }
    let mut ws = ctx.workspace();
    open(&mut ws, &args.plan).map_err(fail)?;
    let outcome = ws.call("style.apply", Value::Object(params)).map_err(fail)?;
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
        return out_json(&outcome);
    }
    let t = |fr, en| ctx.tr(fr, en);
    match written {
        Some(path) => out(&format!(
            "{}\n  {} : {path}\n",
            t("Style appliqué.", "Style applied."),
            t("Écrit", "Written")
        )),
        None => out(&format!(
            "{}\n",
            t("Simulation : rien n'est écrit.", "Dry run: nothing written.")
        )),
    }
}

/// Lit la surcharge JSON. Fichier illisible : `file.*` (code 3) ; JSON mal
/// formé : `param.invalid` (code 1).
fn read_patch(ctx: &Ctx, path: &str) -> Result<Value, ToolError> {
    let bytes = plannercraft_automation::files::read_path(std::path::Path::new(path), MAX_PATCH_BYTES)
        .map_err(|e| ToolError::simple(e.code(), e.message(ctx.lang)).with_field("patch"))?;
    serde_json::from_slice::<Value>(&bytes).map_err(|e| {
        ToolError::simple(
            codes::PARAM_INVALID,
            format!(
                "{} {path} : {e}",
                ctx.tr("surcharge JSON illisible :", "unreadable JSON override:")
            ),
        )
        .with_field("patch")
    })
}
