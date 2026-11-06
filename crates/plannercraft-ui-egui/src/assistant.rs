//! Assistant IA dans l'application (ADR-0024, lot A1, H-UI-138 à H-UI-142) :
//! services injectés (transport, magasin de la clé, navigateur), état du
//! volet, réducteur des commandes et menée de la conversation à chaque
//! image. Le dessin est dans [`crate::view::assistant`].
//!
//! - Rien ne part tant que l'utilisateur n'a pas activé l'assistant
//!   (mention « vous échangez avec une IA », AI Act art. 50) et écrit une
//!   question.
//! - La clé vit ici et dans le magasin des secrets, **jamais** dans
//!   [`crate::UiState`] (envoyé aux agents MCP rattachés) ni dans un
//!   journal : `Debug` de [`AssistantPanel`] la masque.
//! - Aucun appel bloquant sur le fil de l'interface : le transport répond
//!   par une file, le trousseau est lu et écrit par
//!   [`plannercraft_assistant::platform::Background`], la connexion du
//!   desktop tourne dans son propre fil.
//! - Lecture seule : les outils travaillent sur une copie de la session
//!   (`SessionHost`), le document n'est jamais modifié par l'assistant.

use std::sync::Arc;

use plannercraft_assistant::conversation::{Conversation, RequestConfig, Waker};
use plannercraft_assistant::error::AssistantError;
use plannercraft_assistant::http::{EventSink, HttpEvent, HttpRequest, SharedTransport, Transport};
#[cfg(target_arch = "wasm32")]
use plannercraft_assistant::login::{PendingResponse, key_of};
use plannercraft_assistant::openrouter::{self, MAX_TOKENS};
use plannercraft_assistant::platform::Background;
use plannercraft_assistant::secrets::{MemoryStore, SharedSecrets};

/// Commande du volet Assistant (réducteur d'`App`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssistantOp {
    /// Active l'assistant (après la mention de transparence).
    Enable,
    /// Désactive l'assistant (la conversation est oubliée).
    Disable,
    /// Envoie la question saisie.
    Send,
    /// Arrête la réponse en cours.
    Stop,
    /// Nouvelle conversation.
    Clear,
    /// « Se connecter à OpenRouter » (OAuth PKCE).
    Login,
    /// Abandonne la connexion en cours.
    CancelLogin,
    /// Montre ou cache le champ de saisie d'une clé.
    KeyEntry(bool),
    /// Enregistre la clé saisie.
    SaveKey,
    /// « Se déconnecter » : clé oubliée et effacée du magasin.
    Logout,
    /// Choisit un modèle de la liste fermée.
    SetModel(&'static str),
}

/// Transport sans réseau (application de test, harnais) : toute requête
/// échoue aussitôt.
#[derive(Clone, Copy, Debug, Default)]
pub struct OfflineTransport;

impl Transport for OfflineTransport {
    fn send(&self, _request: HttpRequest, sink: EventSink) {
        let _ignored = sink(HttpEvent::Failed("réseau non configuré".to_owned()));
    }
}

/// Services de l'assistant, injectés par l'application.
#[derive(Clone)]
pub struct AssistantServices {
    /// Transport HTTP.
    pub transport: SharedTransport,
    /// Magasin de la clé ; `None` : trousseau du système ouvert à la
    /// première activation (hors du fil de l'interface).
    pub secrets: Option<SharedSecrets>,
    /// Navigateur de la connexion OAuth (desktop) ; `None` : bouton
    /// « Se connecter » grisé.
    #[cfg(not(target_arch = "wasm32"))]
    pub browser: Option<Arc<dyn plannercraft_assistant::platform::BrowserOpener>>,
    /// Connexion par redirection de la page (web).
    #[cfg(target_arch = "wasm32")]
    pub web_login: bool,
}

impl std::fmt::Debug for AssistantServices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AssistantServices").finish_non_exhaustive()
    }
}

impl AssistantServices {
    /// Sans réseau, clé en mémoire, sans connexion OAuth (défaut d'`App`,
    /// tests).
    #[must_use]
    pub fn offline() -> Self {
        Self::with_transport(Arc::new(OfflineTransport))
    }

    /// Transport donné, clé en mémoire, sans navigateur (tests).
    #[must_use]
    pub fn with_transport(transport: SharedTransport) -> Self {
        Self {
            transport,
            secrets: Some(Arc::new(MemoryStore::default())),
            #[cfg(not(target_arch = "wasm32"))]
            browser: None,
            #[cfg(target_arch = "wasm32")]
            web_login: false,
        }
    }

    /// Desktop : réseau réel, trousseau du système, navigateur par défaut.
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn desktop() -> Self {
        Self {
            transport: Arc::new(plannercraft_assistant::EhttpTransport),
            secrets: None,
            browser: Some(Arc::new(plannercraft_assistant::platform::SystemBrowser)),
        }
    }

    /// Web : réseau réel (`fetch`), clé en mémoire, connexion par
    /// redirection.
    #[cfg(target_arch = "wasm32")]
    #[must_use]
    pub fn web() -> Self {
        Self {
            transport: Arc::new(plannercraft_assistant::EhttpTransport),
            secrets: Some(Arc::new(MemoryStore::default())),
            web_login: true,
        }
    }

    /// Navigateur donné (tests de la connexion du desktop).
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn with_browser(mut self, browser: Arc<dyn plannercraft_assistant::platform::BrowserOpener>) -> Self {
        self.browser = Some(browser);
        self
    }

    /// La connexion OAuth est possible.
    #[must_use]
    pub const fn can_login(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.browser.is_some()
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.web_login
        }
    }
}

/// Avis affiché sous la connexion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    /// Trousseau du système absent : clé gardée pour la session.
    KeyringMissing,
    /// Erreur de connexion, de clé ou de demande refusée avant envoi.
    Error(AssistantError),
}

/// Lecture du magasin : magasin ouvert, avertissement, clé lue.
type Loaded = (
    SharedSecrets,
    Option<AssistantError>,
    Result<Option<String>, AssistantError>,
);

/// État de la clé.
enum KeyLoad {
    /// Magasin pas encore lu.
    NotStarted,
    /// Lecture en cours.
    Loading(Background<Loaded>),
    /// Lu.
    Done,
}

/// Connexion en cours.
enum Login {
    Idle,
    #[cfg(not(target_arch = "wasm32"))]
    Desktop(plannercraft_assistant::login::LoginHandle),
    #[cfg(target_arch = "wasm32")]
    Exchange(PendingResponse),
}

/// État du volet Assistant (jamais sérialisé).
pub struct AssistantPanel {
    services: AssistantServices,
    /// Conversation en cours.
    pub conversation: Conversation,
    key: Option<String>,
    store: Option<SharedSecrets>,
    key_load: KeyLoad,
    login: Login,
    save: Option<Background<Result<(), AssistantError>>>,
    /// Question en cours de saisie.
    pub draft: String,
    /// Clé en cours de saisie (champ masqué).
    pub key_draft: String,
    /// Champ de saisie de la clé affiché.
    pub key_entry: bool,
    /// Avis à afficher.
    pub notice: Option<Notice>,
    waker: Option<Waker>,
    #[cfg(target_arch = "wasm32")]
    callback_checked: bool,
}

impl std::fmt::Debug for AssistantPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AssistantPanel")
            .field("connected", &self.key.is_some())
            .field("conversation", &self.conversation)
            .field("notice", &self.notice)
            .finish_non_exhaustive()
    }
}

/// Longueur au plus d'une question (caractères).
pub const MAX_QUESTION_CHARS: usize = 4_000;
/// Longueur au plus d'une clé saisie (caractères).
pub const MAX_KEY_CHARS: usize = 512;

impl AssistantPanel {
    /// Volet sur `services`.
    #[must_use]
    pub fn new(services: AssistantServices) -> Self {
        Self {
            store: services.secrets.clone(),
            services,
            conversation: Conversation::new(),
            key: None,
            key_load: KeyLoad::NotStarted,
            login: Login::Idle,
            save: None,
            draft: String::new(),
            key_draft: String::new(),
            key_entry: false,
            notice: None,
            waker: None,
            #[cfg(target_arch = "wasm32")]
            callback_checked: false,
        }
    }

    /// Remplace les services (application : réseau réel, trousseau).
    pub fn set_services(&mut self, services: AssistantServices) {
        *self = Self::new(services);
    }

    /// Une clé est connue.
    #[must_use]
    pub const fn connected(&self) -> bool {
        self.key.is_some()
    }

    /// Le magasin est en cours de lecture.
    #[must_use]
    pub const fn key_loading(&self) -> bool {
        matches!(self.key_load, KeyLoad::Loading(_))
    }

    /// Une connexion OAuth est en cours.
    #[must_use]
    pub const fn logging_in(&self) -> bool {
        !matches!(self.login, Login::Idle)
    }

    /// La connexion OAuth est possible.
    #[must_use]
    pub const fn can_login(&self) -> bool {
        self.services.can_login()
    }

    /// La clé survit à la fermeture (trousseau).
    #[must_use]
    pub fn key_persistent(&self) -> bool {
        self.store.as_ref().is_some_and(|s| s.persistent())
    }

    /// Réveil de l'image (posé une fois).
    pub fn ensure_waker(&mut self, ctx: &egui::Context) {
        if self.waker.is_none() {
            let ctx = ctx.clone();
            let waker: Waker = Arc::new(move || ctx.request_repaint());
            self.conversation.set_waker(Arc::clone(&waker));
            self.waker = Some(waker);
        }
    }

    /// Lance la lecture du magasin si elle n'a pas eu lieu.
    pub fn start_key_load(&mut self) {
        if !matches!(self.key_load, KeyLoad::NotStarted) {
            return;
        }
        let store = self.store.clone();
        self.key_load = KeyLoad::Loading(Background::spawn(
            move || {
                let (store, warning) = match store {
                    Some(store) => (store, None),
                    None => plannercraft_assistant::secrets::open_default(),
                };
                let key = store.load();
                (store, warning, key)
            },
            self.waker.clone(),
        ));
    }

    /// Lit les résultats en attente (magasin, connexion, enregistrement) ;
    /// vrai si quelque chose a changé.
    pub fn poll_services(&mut self) -> bool {
        let mut changed = false;
        #[cfg(target_arch = "wasm32")]
        if !self.callback_checked {
            self.callback_checked = true;
            if let Some(result) = plannercraft_assistant::web::take_callback() {
                changed = true;
                match result {
                    Ok((code, verifier)) => {
                        self.login = Login::Exchange(plannercraft_assistant::login::start_exchange(
                            self.services.transport.as_ref(),
                            &code,
                            &verifier,
                            self.waker.clone(),
                        ));
                    }
                    Err(error) => self.notice = Some(Notice::Error(error)),
                }
            }
        }
        if let KeyLoad::Loading(task) = &self.key_load
            && let Some((store, warning, key)) = task.take()
        {
            changed = true;
            self.key_load = KeyLoad::Done;
            self.store = Some(Arc::clone(&store));
            if warning.is_some() {
                self.notice = Some(Notice::KeyringMissing);
            }
            match key {
                // Une clé saisie pendant la lecture l'emporte (et s'enregistre).
                Ok(_) if self.key.is_some() => {
                    if let Some(key) = self.key.clone() {
                        self.save_key(&key);
                    }
                }
                Ok(found) => self.key = found.filter(|k| !k.trim().is_empty()),
                Err(error) => self.notice = Some(Notice::Error(error)),
            }
        }
        let finished = match &self.login {
            Login::Idle => None,
            #[cfg(not(target_arch = "wasm32"))]
            Login::Desktop(handle) => handle.take(),
            #[cfg(target_arch = "wasm32")]
            Login::Exchange(pending) => pending.poll().map(key_of),
        };
        if let Some(result) = finished {
            changed = true;
            self.login = Login::Idle;
            match result {
                Ok(key) => self.set_key(&key),
                Err(error) => self.notice = Some(Notice::Error(error)),
            }
        }
        if let Some(save) = &self.save
            && let Some(result) = save.take()
        {
            changed = true;
            self.save = None;
            if let Err(error) = result {
                self.notice = Some(Notice::Error(error));
            }
        }
        changed
    }

    fn set_key(&mut self, key: &str) {
        let key = key.trim().to_owned();
        if key.is_empty() {
            return;
        }
        if !matches!(self.notice, Some(Notice::KeyringMissing)) {
            self.notice = None;
        }
        self.key = Some(key.clone());
        if matches!(self.key_load, KeyLoad::Done) {
            self.save_key(&key);
        } else {
            // Enregistrée quand le magasin aura été lu.
            self.start_key_load();
        }
    }

    fn save_key(&mut self, key: &str) {
        if let Some(store) = self.store.clone() {
            let key = key.to_owned();
            self.save = Some(Background::spawn(move || store.save(&key), self.waker.clone()));
        }
    }

    /// Applique `op` ; l'activation et le modèle sont mémorisés dans
    /// `prefs` (état d'interface), `lang` est la langue des textes.
    pub fn apply(
        &mut self,
        op: AssistantOp,
        prefs: &mut crate::state::AssistantPrefs,
        lang: plannercraft_automation::text::Language,
    ) {
        match op {
            AssistantOp::Enable => {
                prefs.enabled = true;
                self.start_key_load();
            }
            AssistantOp::Disable => {
                prefs.enabled = false;
                self.conversation.clear();
                self.cancel_login();
            }
            AssistantOp::Send => self.send(prefs, lang),
            AssistantOp::Stop => self.conversation.stop(),
            AssistantOp::Clear => {
                self.conversation.clear();
                self.notice = self.notice.take().filter(|n| *n == Notice::KeyringMissing);
            }
            AssistantOp::Login => self.login(lang),
            AssistantOp::CancelLogin => self.cancel_login(),
            AssistantOp::KeyEntry(show) => {
                self.key_entry = show;
                self.key_draft.clear();
            }
            AssistantOp::SaveKey => {
                let key: String = self.key_draft.trim().chars().take(MAX_KEY_CHARS).collect();
                self.key_draft.clear();
                self.key_entry = false;
                self.set_key(&key);
            }
            AssistantOp::Logout => {
                self.key = None;
                self.conversation.clear();
                if let Some(store) = self.store.clone() {
                    self.save = Some(Background::spawn(move || store.delete(), self.waker.clone()));
                }
            }
            AssistantOp::SetModel(id) => {
                id.clone_into(&mut prefs.model);
            }
        }
    }

    fn send(&mut self, prefs: &crate::state::AssistantPrefs, lang: plannercraft_automation::text::Language) {
        let question: String = self.draft.trim().chars().take(MAX_QUESTION_CHARS).collect();
        if question.is_empty() {
            return;
        }
        let Some(api_key) = self.key.clone() else {
            self.notice = Some(Notice::Error(AssistantError::NoKey));
            return;
        };
        let config = RequestConfig {
            api_key,
            model: openrouter::model_or_default(&prefs.model).id.to_owned(),
            lang,
            max_tokens: MAX_TOKENS,
        };
        match self
            .conversation
            .send(&question, config, self.services.transport.as_ref())
        {
            Ok(()) => self.draft.clear(),
            Err(error) => self.notice = Some(Notice::Error(error)),
        }
    }

    fn login(&mut self, lang: plannercraft_automation::text::Language) {
        if self.logging_in() || !self.can_login() {
            return;
        }
        self.notice = self.notice.take().filter(|n| *n == Notice::KeyringMissing);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(browser) = self.services.browser.clone() {
            self.login = Login::Desktop(plannercraft_assistant::login::DesktopLogin::start(
                Arc::clone(&self.services.transport),
                browser,
                lang,
                self.waker.clone(),
            ));
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = lang;
            if let Err(error) = plannercraft_assistant::web::begin_login() {
                self.notice = Some(Notice::Error(error));
            }
        }
    }

    fn cancel_login(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Login::Desktop(handle) = &self.login {
            handle.cancel();
        }
        self.login = Login::Idle;
    }

    /// Transport (menée de la conversation par `App`).
    #[must_use]
    pub fn transport(&self) -> SharedTransport {
        Arc::clone(&self.services.transport)
    }
}
