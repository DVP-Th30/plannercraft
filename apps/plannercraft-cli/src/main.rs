//! Binaire `plannercraft` en ligne de commande.
//!
//! Il expose les mêmes opérations que le serveur MCP (`summary`, `tasks`,
//! `resources`, `usage`, `apply`, `render`, `import-csv`, `mcp`,
//! `explain`, `style`, `export-pdf`, `import-pdf`), toutes via le
//! `Workspace` de `plannercraft-automation` : mêmes JSON que le MCP (ADR-0012 D1).
//! Codes de sortie (C-MCP-08) dans [`exit`] ; « maintenant » et langue dans
//! [`now`] (seule lecture de l'horloge). Détail : `docs/features/cli.md`.

mod cmd;
mod exit;
mod now;

use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use plannercraft_automation::text::Language;
use plannercraft_core::{PRODUCT_ID, PRODUCT_NAME, PRODUCT_VERSION};

/// Planification de projet compatible MS Project, pilotable par IA.
///
/// `--version` imprime `PlannerCraft <version>` (nom affiché et version du
/// workspace, `PRODUCT_VERSION` : C-REL-03) ; l'usage nomme le binaire
/// `plannercraft` (`PRODUCT_ID`).
#[derive(Debug, Parser)]
#[command(name = PRODUCT_NAME, bin_name = PRODUCT_ID, version = PRODUCT_VERSION, arg_required_else_help = true)]
struct Cli {
    /// « Maintenant » (`AAAA-MM-JJTHH:MM`) ; défaut : `PLANNERCRAFT_NOW`, sinon
    /// l'horloge locale.
    #[arg(long, global = true, value_name = "AAAA-MM-JJTHH:MM")]
    now: Option<String>,
    /// Langue des messages et des textes (défaut : `LANG`, sinon `fr`).
    #[arg(long, global = true, value_enum)]
    lang: Option<LangArg>,
    #[command(subcommand)]
    command: Command,
}

/// Langue (`--lang`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum LangArg {
    /// Français.
    Fr,
    /// Anglais.
    En,
}

/// Sous-commandes.
#[derive(Debug, Subcommand)]
enum Command {
    /// Résumé du projet.
    Summary(cmd::summary::SummaryArgs),
    /// Liste des tâches.
    Tasks(cmd::tasks::TasksArgs),
    /// Liste des ressources (travail, pic, surutilisation).
    Resources(cmd::resources::ResourcesArgs),
    /// Utilisation des ressources par période.
    Usage(cmd::usage::UsageArgs),
    /// Applique un lot de commandes.
    Apply(cmd::apply::ApplyArgs),
    /// Gantt en SVG.
    Render(cmd::render::RenderArgs),
    /// Import d'un CSV de tâches.
    ImportCsv(cmd::import_csv::ImportCsvArgs),
    /// Serveur MCP (stdio).
    Mcp(cmd::mcp::McpArgs),
    /// Explication d'une date : pilote, calendrier, avancement, chaîne.
    Explain(cmd::explain::ExplainArgs),
    /// Style du document : modèles, style effectif, application (J8).
    Style(cmd::style::StyleArgs),
    /// Planning en PDF (style, papier, pages en largeur ; J8).
    ExportPdf(cmd::export_pdf::ExportPdfArgs),
    /// Import d'un planning Gantt PDF (export MS Project) en MSPDI.
    ImportPdf(cmd::import_pdf::ImportPdfArgs),
}

fn main() -> ExitCode {
    // Erreur d'usage : clap affiche l'aide et sort avec le code 2 ;
    // `--help` et `--version` sortent avec 0.
    let cli = Cli::parse();
    let now = match now::resolve_now(cli.now.as_deref()) {
        Ok(m) => m,
        Err(message) => {
            cmd::warn(&message);
            return ExitCode::from(exit::USAGE);
        }
    };
    let ctx = cmd::Ctx {
        now,
        lang: match cli.lang {
            Some(LangArg::Fr) => Language::Fr,
            Some(LangArg::En) => Language::En,
            None => now::env_language(),
        },
        lang_explicit: cli.lang.is_some(),
    };
    match cli.command {
        Command::Summary(a) => cmd::summary::run(&ctx, &a),
        Command::Tasks(a) => cmd::tasks::run(&ctx, &a),
        Command::Resources(a) => cmd::resources::run(&ctx, &a),
        Command::Usage(a) => cmd::usage::run(&ctx, &a),
        Command::Apply(a) => cmd::apply::run(&ctx, &a),
        Command::Render(a) => cmd::render::run(&ctx, &a),
        Command::ImportCsv(a) => cmd::import_csv::run(&ctx, &a),
        Command::Mcp(a) => cmd::mcp::run(&ctx, a),
        Command::Explain(a) => cmd::explain::run(&ctx, &a),
        Command::Style(a) => cmd::style::run(&ctx, &a),
        Command::ExportPdf(a) => cmd::export_pdf::run(&ctx, &a),
        Command::ImportPdf(a) => cmd::import_pdf::run(&ctx, &a),
    }
}
