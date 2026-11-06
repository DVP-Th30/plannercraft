//! `PlannerCraftServer` et son `ServerHandler` (lot L8 de J4 ; relais : lot L2
//! de J7).
//!
//! Chaque appel d'outil passe par [`PlannerCraftServer::dispatch`] : nom MCP →
//! entrée du registre → [`Backend`]. En local, garde ADR-0002 puis
//! `Workspace::call` ; rattaché à l'application (ADR-0002, ADR-0015 D9),
//! l'appel lui est relayé par le canal de contrôle (`RelayClient`) et
//! l'application, seule source de vérité, le valide. Une erreur métier est
//! un résultat d'outil en erreur structurée
//! (`CallToolResult::structured_error`), jamais une erreur JSON-RPC
//! (C-MCP-05) ; seul un nom inconnu est refusé en `invalid_params`.
//!
//! Rattachement (sans `--headless`, hors `--root`) : au démarrage s'il y a
//! un seul rendez-vous vivant ([`PlannerCraftServer::attach_at_startup`]), ou à
//! `project.open` d'un fichier tenu par une application. `project.open`
//! d'un fichier non tenu pendant un rattachement détache et ouvre en
//! local (`attached: false` dans le résultat) ; une application partie
//! (`control.app_gone`) détache et laisse l'espace local sans projet
//! (C-CTL-06).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use plannercraft_automation::control::{RELAY_MARGIN, RelayClient, RelayError, Rendezvous};
use plannercraft_automation::error::codes;
use plannercraft_automation::registry::{Availability, by_mcp_name};
use plannercraft_automation::text::{Language, TextContext, format_date};
use plannercraft_automation::{CommandSpec, REGISTRY, Session, ToolError, Workspace, view};
use plannercraft_core::Moment;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation, JsonObject,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::{MaybeSendFuture, RequestContext};
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use serde_json::{Value, json};

use crate::journal_file::JournalFile;
use crate::{McpServeError, ServeOptions, tools};

/// Délai du `hello` d'un rattachement et de chaque appel relayé.
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);

/// Où s'exécutent les appels : espace local, ou application rattachée.
#[derive(Debug)]
enum Backend {
    /// Espace de travail du serveur (headless, ou aucune application).
    Local(Box<Workspace>),
    /// Application rattachée : tout appel lui est relayé.
    Relay {
        /// Client du canal.
        client: RelayClient,
        /// Rendez-vous de l'application (identité du rattachement).
        rendezvous: Rendezvous,
    },
}

impl Backend {
    /// Session locale (aucune quand rattaché).
    const fn session(&self) -> Option<&Session> {
        match self {
            Self::Local(ws) => ws.session(),
            Self::Relay { .. } => None,
        }
    }
}

/// Échec d'un appel : erreur d'outil locale ou du canal, ou refus de
/// l'application tel qu'envoyé.
enum Failure {
    /// Erreur d'outil (locale, ou `control.*` du client).
    Tool(ToolError),
    /// Erreur JSON de l'application, rendue telle quelle.
    App(Value),
}

impl From<ToolError> for Failure {
    fn from(e: ToolError) -> Self {
        Self::Tool(e)
    }
}

impl From<RelayError> for Failure {
    fn from(e: RelayError) -> Self {
        match e {
            RelayError::App(v) => Self::App(v),
            RelayError::Control(e) => Self::Tool(e),
        }
    }
}

impl Failure {
    /// Code et message (erreur de `--open`).
    fn describe(&self) -> String {
        match self {
            Self::Tool(e) => format!("{} : {}", e.code, e.message),
            Self::App(v) => format!(
                "{} : {}",
                v.get("code").and_then(Value::as_str).unwrap_or("?"),
                v.get("message").and_then(Value::as_str).unwrap_or("")
            ),
        }
    }
}

/// Serveur MCP : un espace de travail ou une application rattachée,
/// partagé ; un journal facultatif.
#[derive(Clone, Debug)]
pub struct PlannerCraftServer {
    backend: Arc<Mutex<Backend>>,
    journal: Option<Arc<Mutex<JournalFile>>>,
    lang: Language,
    now: Moment,
    /// Dossier des rendez-vous du canal de contrôle (garde ADR-0002 et
    /// rattachement) ; `None` : garde et relais inactifs
    /// (`ignore_app_guard`, ou aucun dossier d'état).
    control_dir: Option<PathBuf>,
    /// Relais permis (ni `--headless`, ni `--root`).
    relay: bool,
    /// Attente de la validation d'une écriture relayée.
    approval: Duration,
    /// Marge après `approval` pour la réponse finale.
    margin: Duration,
}

/// Texte selon la langue.
const fn tr(lang: Language, fr: &'static str, en: &'static str) -> &'static str {
    match lang {
        Language::Fr => fr,
        Language::En => en,
    }
}

impl PlannerCraftServer {
    /// Serveur sans projet ouvert : espace de travail, journal (ouvert ici),
    /// garde sur le dossier d'état de la plateforme sauf
    /// `ignore_app_guard`, relais permis sauf `headless`. Le projet
    /// `opts.open` n'est pas ouvert ici (voir [`Self::open_initial`]), et
    /// aucun rattachement n'a lieu ici (voir [`Self::attach_at_startup`]).
    ///
    /// # Errors
    ///
    /// [`McpServeError::Io`] si le journal ne s'ouvre pas.
    pub fn new(opts: &ServeOptions) -> Result<Self, McpServeError> {
        let journal = match &opts.journal {
            None => None,
            Some(p) => Some(Arc::new(Mutex::new(
                JournalFile::open(p).map_err(McpServeError::Io)?,
            ))),
        };
        let control_dir = if opts.ignore_app_guard {
            None
        } else {
            plannercraft_automation::control::state_dir()
        };
        Ok(Self {
            backend: Arc::new(Mutex::new(Backend::Local(Box::new(Workspace::new(
                opts.now, opts.lang,
            ))))),
            journal,
            lang: opts.lang,
            now: opts.now,
            control_dir,
            relay: !opts.headless,
            approval: opts.approval_timeout,
            margin: RELAY_MARGIN,
        })
    }

    /// Remplace le dossier des rendez-vous de la garde et du relais
    /// (`None` : garde et relais inactifs). Tests et installations
    /// portables.
    #[must_use]
    pub fn with_control_dir(mut self, dir: Option<PathBuf>) -> Self {
        self.control_dir = dir;
        self
    }

    /// Permet ou interdit le relais vers l'application (`false` :
    /// `--headless` ; la garde reste active).
    #[must_use]
    pub const fn with_relay(mut self, enabled: bool) -> Self {
        self.relay = enabled;
        self
    }

    /// Délais d'un appel relayé : validation attendue `approval` au plus,
    /// plus `margin` pour la réponse finale (défaut : `--approval-timeout`
    /// et 10 s ; tests).
    #[must_use]
    pub const fn with_relay_timeouts(mut self, approval: Duration, margin: Duration) -> Self {
        self.approval = approval;
        self.margin = margin;
        self
    }

    /// Confine tous les chemins de fichiers à `root` (MCP HTTP, `--root`) :
    /// un chemin qui en sort est refusé `file.outside_root`. À appeler
    /// avant [`Self::open_initial`] pour que `--open` soit confiné aussi.
    /// Le relais est coupé : l'application ne connaît pas la racine et
    /// écrirait hors d'elle (la garde reste active).
    #[must_use]
    pub fn with_root(mut self, root: PathBuf) -> Self {
        {
            // Aucun appel n'a encore eu lieu : un verrou empoisonné est
            // impossible ici ; s'il l'était, la racine est posée quand même.
            let mut b = self.backend.lock().unwrap_or_else(PoisonError::into_inner);
            let current = std::mem::replace(&mut *b, Backend::Local(self.fresh()));
            let ws = match current {
                Backend::Local(ws) => *ws,
                Backend::Relay { .. } => Workspace::new(self.now, self.lang),
            };
            *b = Backend::Local(Box::new(ws.with_root(root)));
        }
        self.relay = false;
        self
    }

    /// Vrai si les appels sont relayés à une application.
    #[must_use]
    pub fn is_attached(&self) -> bool {
        self.backend
            .lock()
            .is_ok_and(|b| matches!(*b, Backend::Relay { .. }))
    }

    /// Rattachement au démarrage (ADR-0015 D9) : s'il y a **un seul**
    /// rendez-vous vivant et que le relais est permis, les appels lui sont
    /// relayés. Rend vrai si le serveur est rattaché.
    #[must_use = "vrai si le serveur est rattaché"]
    pub fn attach_at_startup(&self) -> bool {
        let Some(dir) = self.relay_dir() else {
            return false;
        };
        let Some(r) = RelayClient::single_live_in(dir) else {
            return false;
        };
        let Ok(client) = self.connect(&r) else {
            return false;
        };
        match self.backend.lock() {
            Ok(mut b) => {
                *b = Backend::Relay {
                    client,
                    rendezvous: r,
                };
                true
            }
            Err(_) => false,
        }
    }

    /// Ouvre `path` au démarrage (`--open`), garde comprise ; un fichier
    /// tenu par une application rattache le serveur (sauf headless).
    /// Hors journal.
    ///
    /// # Errors
    ///
    /// [`McpServeError::Io`] avec le code et le message de l'outil
    /// (`file.open_in_app`, `file.read`, MSPDI illisible…).
    pub fn open_initial(&self, path: &Path) -> Result<(), McpServeError> {
        let params = json!({ "path": path.to_string_lossy() });
        let Ok(mut b) = self.backend.lock() else {
            return Err(McpServeError::Io(Failure::Tool(poisoned(self.lang)).describe()));
        };
        self.execute(&mut b, "project.open", params)
            .map(drop)
            .map_err(|e| McpServeError::Io(e.describe()))
    }

    /// Exécute un appel d'outil (cœur de `call_tool`, synchrone ; un appel
    /// relayé attend la réponse de l'application, validation comprise).
    ///
    /// # Errors
    ///
    /// `invalid_params` si `name` n'est pas un outil du registre ; toute
    /// autre erreur est un résultat en erreur structurée.
    pub fn dispatch(&self, name: &str, arguments: Option<JsonObject>) -> Result<CallToolResult, McpError> {
        let Some(spec) = by_mcp_name(name) else {
            return Err(McpError::invalid_params(format!("unknown tool: {name}"), None));
        };
        let params = arguments.map_or(Value::Null, Value::Object);
        let lang = self.lang;
        let Ok(mut backend) = self.backend.lock() else {
            return Ok(error_result(&poisoned(lang)));
        };
        // Verrou du journal pris sous celui de l'espace : l'ordre des `seq`
        // est l'ordre d'exécution. Les appels relayés sont journalisés
        // comme les locaux.
        let mut journal = self.journal.as_ref().and_then(|j| {
            if let Ok(g) = j.lock() {
                Some(g)
            } else {
                eprintln!(
                    "plannercraft mcp : journal inutilisable (verrou empoisonné), appel non journalisé"
                );
                None
            }
        });
        let seq = journal.as_mut().map(|j| j.begin(name, &params));
        let res = self.execute(&mut backend, spec.id, params);
        let result = match res {
            Ok(value) if spec.id == "render.gantt" => render_result(value, backend.session(), lang),
            Ok(value) if spec.id == "app.screenshot" => screenshot_result(value),
            Ok(value) => CallToolResult::structured(value),
            Err(Failure::Tool(e)) => error_result(&e),
            Err(Failure::App(v)) => CallToolResult::structured_error(v),
        };
        if let (Some(j), Some(seq)) = (journal.as_mut(), seq) {
            let logged = match (&result.structured_content, result.is_error) {
                (Some(v), Some(true)) => json!({ "error": v }),
                (Some(v), _) => json!({ "ok": v }),
                (None, _) => json!({ "ok": null }),
            };
            j.finish(seq, name, logged);
        }
        Ok(result)
    }

    /// Exécute la commande `id` sur `backend` : ouverture (rattachement
    /// éventuel), appel relayé ou appel local gardé.
    fn execute(&self, backend: &mut Backend, id: &'static str, params: Value) -> Result<Value, Failure> {
        if id == "project.open" && self.relay_dir().is_some() {
            return self.open_relaying(backend, params);
        }
        match backend {
            Backend::Local(ws) => {
                self.guard(id, &params, ws)?;
                Ok(ws.call(id, params)?)
            }
            Backend::Relay { client, .. } => match client.call(id, params, self.approval) {
                Ok(v) => Ok(v),
                Err(e) => {
                    if e.detaches() {
                        // C-CTL-06 : retour en local, sans projet (document
                        // de l'application changé : même retour).
                        *backend = Backend::Local(self.fresh());
                    }
                    Err(e.into())
                }
            },
        }
    }

    /// `project.open` quand le relais est permis : fichier tenu par une
    /// application ⇒ rattachement ; sinon ouverture locale (détachement
    /// s'il y avait un rattachement, `attached: false`).
    fn open_relaying(&self, backend: &mut Backend, params: Value) -> Result<Value, Failure> {
        let given = params.get("path").and_then(Value::as_str).map(str::to_owned);
        let held = match (&given, self.relay_dir()) {
            (Some(p), Some(dir)) => {
                let path = self.fresh().resolve_path(p)?;
                plannercraft_automation::control::find_live_for_in(dir, &path)
            }
            _ => None,
        };
        if let Some(r) = held {
            return self.attach_open(backend, r);
        }
        match backend {
            Backend::Local(ws) => Ok(ws.call("project.open", params)?),
            Backend::Relay { .. } => {
                // Détachement seulement si l'ouverture locale réussit :
                // un échec laisse le rattachement intact.
                let mut ws = self.fresh();
                let mut value = ws.call("project.open", params)?;
                *backend = Backend::Local(ws);
                if let Value::Object(map) = &mut value {
                    map.insert("attached".to_owned(), Value::Bool(false));
                }
                Ok(value)
            }
        }
    }

    /// Rattachement à l'application de `r` à l'ouverture de son fichier :
    /// résultat de forme `project.open` (résumé de l'application,
    /// `attached: true`). Un échec laisse `backend` intact.
    fn attach_open(&self, backend: &mut Backend, r: Rendezvous) -> Result<Value, Failure> {
        let same = matches!(backend, Backend::Relay { rendezvous, .. } if *rendezvous == r);
        let summary = if same {
            match backend {
                Backend::Relay { client, .. } => self.relayed_summary(client),
                Backend::Local(_) => return Err(Failure::Tool(poisoned(self.lang))),
            }
        } else {
            let mut client = self.connect(&r).map_err(|e| {
                ToolError::simple(
                    codes::FILE_OPEN_IN_APP,
                    format!(
                        "{} ({e})",
                        tr(
                            self.lang,
                            "Le fichier est ouvert dans PlannerCraft, qui ne répond pas au relais",
                            "The file is open in PlannerCraft, which does not answer the relay",
                        )
                    ),
                )
                .with_field("path")
            })?;
            let summary = self.relayed_summary(&mut client);
            if summary.is_ok() {
                *backend = Backend::Relay {
                    client,
                    rendezvous: r,
                };
            }
            summary
        };
        match summary {
            Ok(summary) => Ok(json!({
                "summary": summary,
                "diagnostics": [],
                "diagnostics_total": 0,
                "attached": true,
            })),
            Err(e) => {
                if same && e.detaches() {
                    *backend = Backend::Local(self.fresh());
                }
                Err(e.into())
            }
        }
    }

    /// Résumé du document de l'application (`project.summary` relayé).
    /// Paramètres `{}` et non `null` : l'application lit `NoParams` par
    /// le routeur du registre, qui refuse `null` (`param.invalid`, constaté
    /// par `relay_app_real_mcp`, J7 lot L16).
    fn relayed_summary(&self, client: &mut RelayClient) -> Result<Value, RelayError> {
        client.call("project.summary", json!({}), self.approval)
    }

    /// Client du canal vers `r`, délais et langue du serveur.
    fn connect(&self, r: &Rendezvous) -> Result<RelayClient, plannercraft_automation::control::ControlError> {
        Ok(RelayClient::connect(r, HELLO_TIMEOUT)?
            .with_margin(self.margin)
            .with_lang(self.lang))
    }

    /// Dossier des rendez-vous si le relais est permis.
    fn relay_dir(&self) -> Option<&Path> {
        self.control_dir.as_deref().filter(|_| self.relay)
    }

    /// Espace local neuf (sans projet ni racine : le relais est coupé dès
    /// qu'il y a une racine).
    fn fresh(&self) -> Box<Workspace> {
        Box::new(Workspace::new(self.now, self.lang))
    }

    /// Outils listés : entrées disponibles, plus `app.*` quand le relais
    /// est permis (sans `--headless` ; appelées hors rattachement, elles
    /// répondent `control.not_attached`, C-CTL-10).
    fn listed(&self) -> Vec<Tool> {
        if self.relay_dir().is_none() {
            return tools::listed(self.lang);
        }
        REGISTRY
            .iter()
            .filter(|s| self.is_listed(s))
            .map(|s| tools::tool_for(s, self.lang))
            .collect()
    }

    fn is_listed(&self, spec: &CommandSpec) -> bool {
        spec.is_listed() || (spec.available == Availability::Attached && self.relay_dir().is_some())
    }

    /// Garde ADR-0002 : refuse d'ouvrir ou d'écrire un fichier tenu par une
    /// application vivante (`file.open_in_app`) depuis l'espace local.
    fn guard(&self, id: &str, params: &Value, ws: &Workspace) -> Result<(), ToolError> {
        let Some(dir) = &self.control_dir else {
            return Ok(());
        };
        // Chemin effectif (confiné à `--root` s'il y en a une) : la garde
        // vise le fichier que l'outil touchera, pas le chemin brut relatif
        // au dossier courant du processus.
        let given = match params.get("path").and_then(Value::as_str) {
            Some(p) => Some(ws.resolve_path(p)?),
            None => None,
        };
        let target = match id {
            "project.open" | "project.export_mspdi" | "project.export_pdf" => given,
            "project.save" => given.or_else(|| {
                ws.session()
                    .and_then(|s| s.origin().path.clone())
                    .map(PathBuf::from)
            }),
            _ => None,
        };
        let Some(target) = target else {
            return Ok(());
        };
        if plannercraft_automation::control::find_live_for_in(dir, &target).is_some() {
            return Err(ToolError::simple(
                codes::FILE_OPEN_IN_APP,
                tr(
                    self.lang,
                    "Le fichier est ouvert dans plannercraft : fermez-le, ou relancez plannercraft mcp sans --headless \
                     (stdio) pour relayer les appels à l'application",
                    "The file is open in plannercraft: close it, or restart plannercraft mcp without --headless (stdio) \
                     to relay calls to the application",
                )
                .to_owned(),
            )
            .with_field("path"));
        }
        Ok(())
    }

    /// Consignes du serveur pour l'agent.
    const fn instructions(&self) -> &'static str {
        // Texte partagé avec l'assistant intégré (ADR-0024, lot A1).
        plannercraft_automation::agent::mcp_instructions(self.lang)
    }
}

/// Erreur `session.poisoned` (verrou empoisonné par une panique).
fn poisoned(lang: Language) -> ToolError {
    ToolError::simple(
        codes::SESSION_POISONED,
        tr(
            lang,
            "session inutilisable après une erreur interne : relancer le serveur",
            "session unusable after an internal error: restart the server",
        )
        .to_owned(),
    )
}

/// Résultat en erreur structurée.
fn error_result(e: &ToolError) -> CallToolResult {
    let value = serde_json::to_value(e).unwrap_or_else(|_| json!({ "code": e.code, "message": e.message }));
    CallToolResult::structured_error(value)
}

/// Résultat de `screenshot` (J7 lot L14, C-CTL-08) : PNG de l'application
/// en contenu image (`data` base64, `mime_type`), puis le reste du résultat
/// (taille, réductions) en contenu structuré et texte, sans les données de
/// l'image (journal et contexte de l'agent légers).
fn screenshot_result(value: Value) -> CallToolResult {
    let Value::Object(mut report) = value else {
        return CallToolResult::structured(value);
    };
    let (Some(Value::String(data)), Some(Value::String(mime))) =
        (report.remove("data"), report.get("mime_type").cloned())
    else {
        return CallToolResult::structured(Value::Object(report));
    };
    let report = Value::Object(report);
    let content = vec![
        ContentBlock::image(data, mime),
        ContentBlock::text(report.to_string()),
    ];
    let mut result = CallToolResult::success(content);
    result.structured_content = Some(report);
    result
}

/// Résultat de `render_gantt` : résumé texte puis SVG en contenu texte,
/// rapport du rendu en contenu structuré (sans le SVG).
fn render_result(value: Value, session: Option<&Session>, lang: Language) -> CallToolResult {
    let Value::Object(mut report) = value else {
        return CallToolResult::structured(value);
    };
    let svg = match report.remove("svg") {
        Some(Value::String(s)) => s,
        _ => String::new(),
    };
    let mut content = Vec::with_capacity(3);
    // Rattaché : pas de session locale, le rapport seul.
    if let Some(s) = session {
        let summary = view::summary(s);
        let finish = format_date(
            s.schedule().project_finish,
            &TextContext::for_project(s.project(), lang),
        );
        let hidden = report.get("hidden").and_then(Value::as_u64).unwrap_or(0);
        let mut text = match lang {
            Language::Fr => format!(
                "{} tâches, {} critiques, fin {finish}",
                summary.tasks, summary.critical
            ),
            Language::En => format!(
                "{} tasks, {} critical, finish {finish}",
                summary.tasks, summary.critical
            ),
        };
        if hidden > 0 {
            let more = match lang {
                Language::Fr => format!(" ({hidden} lignes non affichées)"),
                Language::En => format!(" ({hidden} rows not shown)"),
            };
            text.push_str(&more);
        }
        content.push(ContentBlock::text(text));
    }
    content.push(ContentBlock::text(svg));
    // Spécification MCP : le contenu structuré est aussi rendu en texte
    // (clients sans `structuredContent` : `truncated_days`,
    // `truncated_bytes` visibles).
    let report = Value::Object(report);
    content.push(ContentBlock::text(report.to_string()));
    let mut result = CallToolResult::success(content);
    result.structured_content = Some(report);
    result
}

impl ServerHandler for PlannerCraftServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                plannercraft_core::PRODUCT_ID,
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(self.instructions())
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, McpError>> + MaybeSendFuture + '_ {
        std::future::ready(Ok(ListToolsResult::with_all_items(self.listed())))
    }

    // Appel bloquant (un appel relayé attend la réponse de l'application,
    // validation comprise : jusqu'à `--approval-timeout` plus 10 s) : il
    // s'exécute hors du fil du runtime, qui continue de lire l'entrée.
    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResponse, McpError>> + MaybeSendFuture + '_ {
        let this = self.clone();
        async move {
            let name = request.name.into_owned();
            let arguments = request.arguments;
            let lang = this.lang;
            match tokio::task::spawn_blocking(move || this.dispatch(&name, arguments)).await {
                Ok(res) => res.map(Into::into),
                // Panique du fil (jamais attendue) : erreur structurée.
                Err(_) => Ok(error_result(&poisoned(lang)).into()),
            }
        }
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        by_mcp_name(name)
            .filter(|s| self.is_listed(s))
            .map(|s| tools::tool_for(s, self.lang))
    }
}
