//! `plannercraft mcp [--journal F] [--open plan.xml] [--headless]
//! [--approval-timeout S] [--http ADDR --root DOSSIER [--allow-remote]
//! [--token-file F]]` :
//! serveur MCP sur l'entrée et la sortie standard (`plannercraft_mcp::serve_stdio`)
//! ou en HTTP (`plannercraft_mcp::serve_http`, lot L17).
//!
//! Langue par défaut des textes : anglais (ADR-0012, arbitrage 6), sauf
//! `--lang` explicite. En HTTP, `--root` est obligatoire (code 2 sinon) ;
//! une adresse non locale exige `--allow-remote` et `--token-file`.
//!
//! Sans `--headless` (stdio), le serveur relaie les appels à l'application
//! PlannerCraft qui tient le fichier (ADR-0015 D9) ; `--headless` coupe ce relais
//! et garde le refus `file.open_in_app` d'un fichier tenu.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::Args;
use plannercraft_automation::text::Language;
use plannercraft_mcp::{HttpOptions, McpServeError, ServeOptions};

use super::{Ctx, warn};
use crate::exit;

/// Arguments de `plannercraft mcp`.
#[derive(Debug, Args)]
pub struct McpArgs {
    /// Journal de rejeu (JSONL), écrit avant chaque appel.
    #[arg(long, value_name = "FICHIER")]
    pub journal: Option<PathBuf>,
    /// Projet ouvert au démarrage.
    #[arg(long, value_name = "PLAN")]
    pub open: Option<PathBuf>,
    /// Travaille seul : aucun relais vers l'application PlannerCraft ouverte ; un
    /// fichier qu'elle tient reste refusé (ADR-0002).
    #[arg(long)]
    pub headless: bool,
    /// Attente maximale, en secondes, de la validation d'une écriture
    /// relayée à l'application (plus 10 s pour la réponse).
    #[arg(long, value_name = "SECONDES", default_value_t = 120,
          value_parser = clap::value_parser!(u64).range(1..=3600))]
    pub approval_timeout: u64,
    /// Transport HTTP sur ADRESSE (`:8080` = `127.0.0.1:8080`), chemin `/mcp`.
    #[arg(long, value_name = "ADRESSE")]
    pub http: Option<String>,
    /// Dossier auquel les chemins de fichiers sont confinés (obligatoire avec
    /// `--http`).
    #[arg(long, value_name = "DOSSIER", requires = "http")]
    pub root: Option<PathBuf>,
    /// Autorise une adresse HTTP non locale (jeton `--token-file` exigé).
    #[arg(long, requires = "http")]
    pub allow_remote: bool,
    /// Fichier du jeton porteur (`Authorization: Bearer …`).
    #[arg(long, value_name = "FICHIER", requires = "http")]
    pub token_file: Option<PathBuf>,
}

/// Exécute la sous-commande.
pub fn run(ctx: &Ctx, args: McpArgs) -> ExitCode {
    let lang = if ctx.lang_explicit { ctx.lang } else { Language::En };
    let mut opts = ServeOptions::new(ctx.now, lang);
    opts.journal = args.journal;
    opts.open = args.open;
    opts.headless = args.headless;
    opts.approval_timeout = Duration::from_secs(args.approval_timeout);
    let res = match args.http {
        None => plannercraft_mcp::serve_stdio(opts),
        Some(addr) => {
            let http = HttpOptions {
                addr,
                root: args.root,
                allow_remote: args.allow_remote,
                token_file: args.token_file,
            };
            plannercraft_mcp::serve_http(&opts, &http)
        }
    };
    match res {
        Ok(()) => ExitCode::from(exit::SUCCESS),
        Err(e @ (McpServeError::NotYetAvailable | McpServeError::Usage(_))) => {
            warn(&format!("mcp : {e}"));
            ExitCode::from(exit::USAGE)
        }
        // Entrée/sortie, ligne trop longue (C-MCP-06 : code 3) et toute
        // erreur ajoutée par le serveur : erreur de fichier ou de flux.
        Err(e) => {
            warn(&format!("mcp : {e}"));
            ExitCode::from(exit::FILE)
        }
    }
}
