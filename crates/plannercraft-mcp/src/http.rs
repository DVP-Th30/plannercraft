//! Transport MCP « streamable HTTP » (feature `http`, lot L17).
//!
//! `plannercraft mcp --http ADDR --root DOSSIER` : le service `StreamableHttpService`
//! de rmcp, servi par hyper 1 en HTTP/1.1 sur `ADDR`, chemin `/mcp`. Toutes
//! les sessions HTTP partagent le même espace de travail (une fabrique qui
//! clone le serveur, donc son `Arc<Mutex<Workspace>>`) : les appels sont
//! sérialisés comme en stdio.
//!
//! Sécurité (ADR-0012 D8) :
//! - `--root` obligatoire : tout chemin de fichier y est confiné
//!   (`file.outside_root`) ;
//! - adresse de boucle locale par défaut (`:8080` → `127.0.0.1:8080`) ;
//!   `Host` limité à la boucle locale (anti-rebinding DNS), `Origin`
//!   limité à `http://127.0.0.1:*` et `http://localhost:*` (une page web
//!   étrangère est refusée en 403) ;
//! - adresse non locale : `--allow-remote` et `--token-file` obligatoires ;
//!   jeton porteur (`Authorization: Bearer …`) comparé en temps constant,
//!   `Host` élargi à l'adresse donnée. Un jeton donné en boucle locale est
//!   exigé aussi.
//! - corps de requête borné à `max_line_bytes` (8 Mio, comme C-MCP-06).

use std::convert::Infallible;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::header::{AUTHORIZATION, HeaderValue, WWW_AUTHENTICATE};
use hyper::server::conn::http1;
use hyper::service::{Service, service_fn};
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use hyper_util::service::TowerToHyperService;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService};
use tokio::net::TcpListener;

use crate::{McpServeError, PlannerCraftServer, ServeOptions};

/// Chemin servi.
pub const MCP_PATH: &str = "/mcp";

/// Longueur minimale du jeton porteur, en octets (hypothèse : 16 octets
/// aléatoires au moins, soit 128 bits).
pub const MIN_TOKEN_BYTES: usize = 16;

/// Origines de navigateur admises (boucle locale seulement).
const ALLOWED_ORIGINS: [&str; 2] = ["http://127.0.0.1:*", "http://localhost:*"];

/// Hôtes admis en boucle locale (`Host`).
const LOOPBACK_HOSTS: [&str; 3] = ["localhost", "127.0.0.1", "::1"];

/// Options du transport HTTP (arguments de la CLI, non validés).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HttpOptions {
    /// Adresse d'écoute : `:PORT`, `localhost:PORT`, `IPv4:PORT`,
    /// `[IPv6]:PORT`.
    pub addr: String,
    /// Dossier auquel tous les chemins sont confinés (`--root`, obligatoire).
    pub root: Option<PathBuf>,
    /// Autorise une adresse non locale (`--allow-remote`).
    pub allow_remote: bool,
    /// Fichier du jeton porteur (`--token-file`), obligatoire hors boucle
    /// locale.
    pub token_file: Option<PathBuf>,
}

/// Configuration validée.
#[derive(Clone, Debug)]
struct HttpConfig {
    addr: SocketAddr,
    root: PathBuf,
    token: Option<Arc<[u8]>>,
    allowed_hosts: Vec<String>,
}

/// Lit `:8080`, `localhost:8080`, `127.0.0.1:8080`, `[::1]:8080`.
///
/// # Errors
///
/// [`McpServeError::Usage`] si l'adresse n'est pas une IP suivie d'un port
/// (un nom d'hôte autre que `localhost` est refusé : l'adresse d'écoute
/// doit être explicite).
pub fn parse_addr(addr: &str) -> Result<SocketAddr, McpServeError> {
    let addr = addr.trim();
    let bad = || {
        McpServeError::Usage(format!(
            "adresse HTTP invalide : « {addr} » (attendu :PORT ou IP:PORT)"
        ))
    };
    if let Some(port) = addr.strip_prefix(':') {
        let port = port.parse::<u16>().map_err(|_| bad())?;
        return Ok(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port));
    }
    if let Some(port) = addr.strip_prefix("localhost:") {
        let port = port.parse::<u16>().map_err(|_| bad())?;
        return Ok(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port));
    }
    addr.parse::<SocketAddr>().map_err(|_| bad())
}

impl HttpConfig {
    /// Valide les options : adresse, racine, distance, jeton.
    fn from_options(http: &HttpOptions) -> Result<Self, McpServeError> {
        let addr = parse_addr(&http.addr)?;
        let Some(root) = &http.root else {
            return Err(McpServeError::Usage(
                "--http exige --root DOSSIER (les chemins de fichiers y sont confinés)".to_owned(),
            ));
        };
        let root = canonical_dir(root)?;
        let ip = addr.ip();
        let remote = !ip.is_loopback();
        if remote && ip.is_unspecified() {
            return Err(McpServeError::Usage(format!(
                "adresse {ip} refusée : donner l'adresse de l'interface à servir"
            )));
        }
        if remote && !http.allow_remote {
            return Err(McpServeError::Usage(format!(
                "adresse non locale {ip} : ajouter --allow-remote et --token-file"
            )));
        }
        if remote && http.token_file.is_none() {
            return Err(McpServeError::Usage(format!(
                "adresse non locale {ip} : --token-file obligatoire (jeton porteur)"
            )));
        }
        let token = match &http.token_file {
            None => None,
            Some(p) => Some(read_token(p)?),
        };
        let mut allowed_hosts: Vec<String> = LOOPBACK_HOSTS.iter().map(|h| (*h).to_owned()).collect();
        if remote {
            allowed_hosts.push(ip.to_string());
        }
        Ok(Self {
            addr,
            root,
            token,
            allowed_hosts,
        })
    }
}

/// Dossier canonique ; introuvable ou pas un dossier : erreur de fichier.
fn canonical_dir(root: &Path) -> Result<PathBuf, McpServeError> {
    let c = std::fs::canonicalize(root)
        .map_err(|e| McpServeError::Io(format!("--root {} : {e}", root.display())))?;
    if c.is_dir() {
        Ok(c)
    } else {
        Err(McpServeError::Io(format!(
            "--root {} : pas un dossier",
            root.display()
        )))
    }
}

/// Jeton porteur : contenu du fichier sans les blancs de bord, au moins
/// [`MIN_TOKEN_BYTES`] octets, sans blanc intérieur.
fn read_token(p: &Path) -> Result<Arc<[u8]>, McpServeError> {
    let raw = std::fs::read_to_string(p)
        .map_err(|e| McpServeError::Io(format!("--token-file {} : {e}", p.display())))?;
    let token = raw.trim();
    if token.len() < MIN_TOKEN_BYTES || token.chars().any(char::is_whitespace) {
        return Err(McpServeError::Usage(format!(
            "--token-file {} : jeton d'au moins {MIN_TOKEN_BYTES} caractères, sans espace",
            p.display()
        )));
    }
    Ok(Arc::from(token.as_bytes()))
}

/// Égalité en temps constant (pour une longueur attendue donnée) : chaque
/// octet du jeton attendu est comparé, quelle que soit la première
/// différence. Seule la longueur peut fuiter.
fn constant_time_eq(expected: &[u8], given: &[u8]) -> bool {
    let mut diff = u8::from(expected.len() != given.len());
    for (i, e) in expected.iter().enumerate() {
        let g = given.get(i).copied().unwrap_or(0);
        diff |= e ^ g;
    }
    std::hint::black_box(diff) == 0
}

/// `Authorization: Bearer <jeton>` correct ?
fn authorized(req: &Request<Incoming>, token: &[u8]) -> bool {
    let Some(value) = req.headers().get(AUTHORIZATION) else {
        return false;
    };
    let value = value.as_bytes();
    let Some((scheme, rest)) = value.split_at_checked(7) else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case(b"bearer ") {
        return false;
    }
    constant_time_eq(token, rest.trim_ascii())
}

type BoxResponse = Response<BoxBody<Bytes, Infallible>>;

/// Réponse simple (texte) avec un statut.
fn plain(status: StatusCode, text: &'static str) -> BoxResponse {
    let mut resp = Response::new(Full::new(Bytes::from_static(text.as_bytes())).boxed());
    *resp.status_mut() = status;
    resp
}

/// Serveur HTTP lié à son adresse, prêt à servir.
pub struct HttpServer {
    listener: TcpListener,
    service: TowerToHyperService<StreamableHttpService<PlannerCraftServer, LocalSessionManager>>,
    token: Option<Arc<[u8]>>,
}

impl std::fmt::Debug for HttpServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpServer")
            .field("listener", &self.listener)
            .field("token", &self.token.as_ref().map(|_| "<jeton>"))
            .finish_non_exhaustive()
    }
}

impl HttpServer {
    /// Valide les options, crée le serveur (journal, racine, `--open`) et
    /// lie l'adresse. Un port `0` choisit un port libre (tests).
    ///
    /// # Errors
    ///
    /// [`McpServeError::Usage`] (adresse, `--root` absent, adresse non
    /// locale sans `--allow-remote` ou sans jeton, jeton trop court),
    /// [`McpServeError::Io`] (racine, jeton, journal, `--open`, liaison).
    pub async fn bind(opts: &ServeOptions, http: &HttpOptions) -> Result<Self, McpServeError> {
        let cfg = HttpConfig::from_options(http)?;
        let server = PlannerCraftServer::new(opts)?.with_root(cfg.root.clone());
        if let Some(path) = &opts.open {
            server.open_initial(path)?;
        }
        let config = StreamableHttpServerConfig::default()
            .with_allowed_hosts(cfg.allowed_hosts.clone())
            .with_allowed_origins(ALLOWED_ORIGINS)
            .with_legacy_session_mode(true)
            .with_max_request_body_bytes(opts.max_line_bytes);
        let service = StreamableHttpService::new(
            move || Ok(server.clone()),
            Arc::new(LocalSessionManager::default()),
            config,
        );
        let listener = TcpListener::bind(cfg.addr)
            .await
            .map_err(|e| McpServeError::Io(format!("écoute sur {} : {e}", cfg.addr)))?;
        Ok(Self {
            listener,
            service: TowerToHyperService::new(service),
            token: cfg.token,
        })
    }

    /// Adresse effectivement liée.
    ///
    /// # Errors
    ///
    /// [`McpServeError::Io`] si le système ne la donne pas.
    pub fn local_addr(&self) -> Result<SocketAddr, McpServeError> {
        self.listener
            .local_addr()
            .map_err(|e| McpServeError::Io(format!("adresse locale : {e}")))
    }

    /// Accepte les connexions jusqu'à l'arrêt du processus (ou l'abandon du
    /// futur). Une connexion en erreur ne concerne qu'elle.
    pub async fn run(self) {
        let Self {
            listener,
            service,
            token,
        } = self;
        loop {
            let stream = match listener.accept().await {
                Ok((stream, _peer)) => stream,
                Err(e) => {
                    // Erreur passagère (descripteurs épuisés…) : on continue.
                    eprintln!("plannercraft mcp http : connexion refusée : {e}");
                    tokio::task::yield_now().await;
                    continue;
                }
            };
            let service = service.clone();
            let token = token.clone();
            tokio::spawn(async move {
                let handler = service_fn(move |req: Request<Incoming>| {
                    let service = service.clone();
                    let token = token.clone();
                    async move { Ok::<_, Infallible>(handle(&service, token.as_deref(), req).await) }
                });
                // Erreur de connexion (client parti, requête mal formée) :
                // sans effet sur le serveur.
                if let Err(e) = http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), handler)
                    .await
                {
                    eprintln!("plannercraft mcp http : connexion close : {e}");
                }
            });
        }
    }
}

/// Une requête : chemin, jeton, puis le service MCP de rmcp (qui contrôle
/// `Host` et `Origin`).
async fn handle(
    service: &TowerToHyperService<StreamableHttpService<PlannerCraftServer, LocalSessionManager>>,
    token: Option<&[u8]>,
    req: Request<Incoming>,
) -> BoxResponse {
    if req.uri().path() != MCP_PATH {
        return plain(StatusCode::NOT_FOUND, "Not Found: use /mcp");
    }
    if let Some(token) = token
        && !authorized(&req, token)
    {
        let mut resp = plain(StatusCode::UNAUTHORIZED, "Unauthorized: bearer token required");
        resp.headers_mut()
            .insert(WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        return resp;
    }
    match service.call(req).await {
        Ok(resp) => resp,
        Err(never) => match never {},
    }
}

/// Sert en HTTP (runtime tokio à fil unique) jusqu'à l'arrêt du processus.
/// L'adresse liée est écrite sur la sortie d'erreur.
///
/// # Errors
///
/// Celles de [`HttpServer::bind`] et du runtime.
pub fn serve_http(opts: &ServeOptions, http: &HttpOptions) -> Result<(), McpServeError> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| McpServeError::Io(format!("runtime tokio : {e}")))?;
    rt.block_on(async {
        let server = HttpServer::bind(opts, http).await?;
        let addr = server.local_addr()?;
        eprintln!("plannercraft mcp : http://{addr}{MCP_PATH}");
        server.run().await;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_addr_forms() {
        assert_eq!(parse_addr(":8080").ok(), "127.0.0.1:8080".parse().ok());
        assert_eq!(parse_addr("localhost:9").ok(), "127.0.0.1:9".parse().ok());
        assert_eq!(parse_addr("[::1]:80").ok(), "[::1]:80".parse().ok());
        assert!(matches!(
            parse_addr("example.com:80"),
            Err(McpServeError::Usage(_))
        ));
        assert!(matches!(parse_addr(":99999"), Err(McpServeError::Usage(_))));
        assert!(matches!(parse_addr("8080"), Err(McpServeError::Usage(_))));
    }

    #[test]
    fn constant_time_eq_cases() {
        assert!(constant_time_eq(b"abcdef", b"abcdef"));
        assert!(!constant_time_eq(b"abcdef", b"abcdeg"));
        assert!(!constant_time_eq(b"abcdef", b"abcde"));
        assert!(!constant_time_eq(b"abcdef", b"abcdefg"));
        assert!(!constant_time_eq(b"abcdef", b""));
    }
}
