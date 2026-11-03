//! Serveur MCP de PlannerCraft (SDK officiel `rmcp`, transport stdio).
//!
//! Il expose les commandes de `plannercraft-automation` à un agent IA. Il ne
//! calcule jamais une date lui-même et ne renvoie jamais de résultat estimé :
//! tout vient du moteur (règle 3). Les outils sont générés depuis
//! `plannercraft_automation::REGISTRY`, au-dessus de `Workspace` (ADR-0012 D7,
//! D8). Sans `--headless`, le serveur se rattache à l'application qui tient
//! le fichier et lui relaie les appels (ADR-0002, ADR-0015 D9, lot L2 de
//! J7) ; en `--headless`, un fichier tenu par une application vivante est
//! refusé (`file.open_in_app`).
//!
//! Transport stdio (tokio à fil unique), lignes d'entrée bornées
//! (C-MCP-06), journal de rejeu `--journal` facultatif. Transport HTTP
//! (« streamable HTTP ») derrière la feature `http` (lot L17). Détail :
//! `docs/features/mcp.md` et ADR-0012.

mod bounded;
#[cfg(feature = "http")]
mod http;
mod journal_file;
mod server;
mod tools;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use plannercraft_automation::text::Language;
use plannercraft_core::Moment;
use rmcp::service::ServerInitializeError;
use tokio::io::{AsyncRead, AsyncWrite};

#[cfg(feature = "http")]
pub use http::{HttpOptions, HttpServer, MCP_PATH, MIN_TOKEN_BYTES, parse_addr, serve_http};
pub use server::PlannerCraftServer;

/// Options du serveur.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServeOptions {
    /// « Maintenant », injecté.
    pub now: Moment,
    /// Langue par défaut des textes (`en` pour le MCP, ADR-0012).
    pub lang: Language,
    /// Journal de rejeu (`--journal`).
    pub journal: Option<PathBuf>,
    /// Projet ouvert au démarrage (`--open`).
    pub open: Option<PathBuf>,
    /// Longueur maximale d'une ligne d'entrée (8 Mio).
    pub max_line_bytes: usize,
    /// Ignore la garde ADR-0002 et coupe le relais (tests, rejeux : aucun
    /// rendez-vous lu).
    pub ignore_app_guard: bool,
    /// `--headless` : aucun relais vers l'application ; la garde reste
    /// active (`file.open_in_app`).
    pub headless: bool,
    /// Attente de la validation d'une écriture relayée
    /// (`--approval-timeout`, défaut 120 s), plus 10 s pour la réponse.
    pub approval_timeout: Duration,
}

/// Délai de validation par défaut d'une écriture relayée (C-CTL-04).
pub const DEFAULT_APPROVAL_TIMEOUT: Duration = Duration::from_secs(120);

impl ServeOptions {
    /// Options par défaut à l'instant `now`.
    #[must_use]
    pub const fn new(now: Moment, lang: Language) -> Self {
        Self {
            now,
            lang,
            journal: None,
            open: None,
            max_line_bytes: 8 * 1024 * 1024,
            ignore_app_guard: false,
            headless: false,
            approval_timeout: DEFAULT_APPROVAL_TIMEOUT,
        }
    }
}

/// Échec du serveur.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum McpServeError {
    /// Entrée/sortie (runtime, journal, fichier `--open`).
    #[error("entrée/sortie : {0}")]
    Io(String),
    /// Options de lancement invalides (MCP HTTP : adresse, `--root` absent,
    /// adresse non locale sans `--allow-remote` ou sans jeton) : la CLI sort
    /// avec le code 2.
    #[error("usage : {0}")]
    Usage(String),
    /// Transport fermé sur une ligne trop longue (C-MCP-06).
    #[error("ligne d'entrée trop longue")]
    LineTooLong,
    /// Squelette L0 ; plus renvoyé depuis L8 (gardé pour ne pas casser
    /// les `match` des appelants).
    #[error("serveur MCP pas encore disponible (J4, L8)")]
    NotYetAvailable,
}

/// Sert sur l'entrée et la sortie standard (runtime tokio à fil unique).
///
/// # Errors
///
/// [`McpServeError::Io`] (runtime, journal, fichier `--open`),
/// [`McpServeError::LineTooLong`] si la session s'est close sur une ligne
/// trop longue (C-MCP-06 : la CLI sort avec le code 3).
pub fn serve_stdio(opts: ServeOptions) -> Result<(), McpServeError> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| McpServeError::Io(format!("runtime tokio : {e}")))?;
    let res = rt.block_on(serve_on(tokio::io::stdin(), tokio::io::stdout(), opts));
    // La lecture de stdin occupe un fil bloquant qui ne rend pas la main :
    // ne pas l'attendre.
    rt.shutdown_background();
    res
}

/// Sert sur un couple lecteur/écrivain (tests : `tokio::io::duplex`).
///
/// # Errors
///
/// Celles de [`PlannerCraftServer::new`], [`PlannerCraftServer::open_initial`] et
/// [`serve_with`].
///
/// Sans `--open`, le serveur se rattache à l'application s'il y a un seul
/// rendez-vous vivant ([`PlannerCraftServer::attach_at_startup`]) ; avec
/// `--open`, l'ouverture décide (fichier tenu : rattachement).
pub async fn serve_on<R, W>(r: R, w: W, opts: ServeOptions) -> Result<(), McpServeError>
where
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    let server = PlannerCraftServer::new(&opts)?;
    if let Some(path) = &opts.open {
        server.open_initial(path)?;
    } else if server.attach_at_startup() {
        eprintln!("plannercraft mcp : rattaché à l'application PlannerCraft ouverte, appels relayés");
    }
    serve_with(server, r, w, opts.max_line_bytes).await
}

/// Sert `server` sur un couple lecteur/écrivain, lignes d'entrée bornées à
/// `max_line_bytes` octets. Rend la main quand le client ferme la
/// connexion.
///
/// # Errors
///
/// [`McpServeError::LineTooLong`] (C-MCP-06), [`McpServeError::Io`]
/// (transport, tâche du service).
pub async fn serve_with<R, W>(
    server: PlannerCraftServer,
    r: R,
    w: W,
    max_line_bytes: usize,
) -> Result<(), McpServeError>
where
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    let tripped = Arc::new(AtomicBool::new(false));
    let reader = bounded::BoundedLines::new(r, max_line_bytes, Arc::clone(&tripped));
    let res = match rmcp::serve_server(server, (reader, w)).await {
        Ok(running) => match running.waiting().await {
            Ok(_) => Ok(()),
            Err(e) => Err(McpServeError::Io(format!("service MCP : {e}"))),
        },
        // Entrée fermée avant l'initialisation : fin normale.
        Err(ServerInitializeError::ConnectionClosed(_)) => Ok(()),
        Err(e) => Err(McpServeError::Io(format!("initialisation MCP : {e}"))),
    };
    if tripped.load(Ordering::SeqCst) {
        return Err(McpServeError::LineTooLong);
    }
    res
}
