//! Mode « Compte PlannerCraft » du volet Assistant (ADR-0024, lot A4,
//! H-UI-155 à H-UI-160) : connexion par e-mail (lien magique et code à 8
//! caractères), jeton de session, solde, achat de crédits, consommation.
//!
//! - Le jeton de session vit ici et dans le magasin des secrets (trousseau
//!   du système, entrée `plannercraft-session-token`, distincte de la clé
//!   OpenRouter ; mémoire sur le web), **jamais** dans
//!   [`crate::UiState`], `app.ui_state` ni un journal.
//! - Chaque appel au relais est une requête courte ([`PendingResponse`])
//!   lue à l'image suivante ; rien ne bloque l'interface.
//! - Une session refusée (`auth.expired`, `auth.invalid`) ramène à l'écran
//!   de connexion avec un message ; un résultat arrivé après une
//!   déconnexion est ignoré (génération de session).
//! - Fenêtre « Mon compte » (H-UI-238 à H-UI-243) : achats
//!   (`/v1/purchases`, facultatif : un relais ancien répond `404`),
//!   consommation, et après l'ouverture de la page de paiement, solde relu
//!   au retour du focus et toutes les [`CHECKOUT_POLL_SECS`] secondes
//!   pendant [`CHECKOUT_WATCH_SECS`] secondes au plus, jusqu'à ce qu'il
//!   augmente (« Crédits ajoutés »). L'heure est celle d'egui, passée par
//!   l'application ([`AssistantPanel::tick_account`]) : aucune horloge lue
//!   ici.

use std::sync::Arc;

use plannercraft_assistant::error::AssistantError;
use plannercraft_assistant::login::PendingResponse;
use plannercraft_assistant::platform::Background;
use plannercraft_assistant::relay::{self, Me, PurchasesReport, UsageReport};
use plannercraft_assistant::secrets::SharedSecrets;

use super::{AssistantPanel, Loaded, Notice};
use crate::state::{AssistantAccess, AssistantPrefs};

/// Information du compte affichée sous l'en-tête.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountInfo {
    /// Page de paiement ouverte dans le navigateur.
    CheckoutOpened,
    /// Retour de la page de paiement : paiement abouti (web).
    CheckoutReturn,
    /// Retour de la page de paiement : paiement abandonné (web).
    CheckoutCancelled,
    /// Solde augmenté après l'ouverture de la page de paiement : montant
    /// ajouté, centimes (H-UI-242).
    CreditsAdded(i64),
}

/// Intervalle de relecture du solde après l'ouverture de la page de
/// paiement, secondes (H-UI-242).
pub const CHECKOUT_POLL_SECS: f64 = 5.0;
/// Durée au plus de la relecture après l'ouverture de la page de paiement,
/// secondes (10 minutes : un paiement abandonné ne fait pas tourner de
/// requêtes indéfiniment).
pub const CHECKOUT_WATCH_SECS: f64 = 600.0;

/// Achats de la fenêtre « Mon compte » (`/v1/purchases`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Purchases {
    /// Jamais demandés.
    #[default]
    Idle,
    /// Première lecture en cours.
    Loading,
    /// Lus.
    Ready(PurchasesReport),
    /// Relais antérieur à la route (`404`) : section remplacée par un
    /// message discret.
    Unavailable,
    /// Lecture refusée ou impossible (réseau…) : message dans la section,
    /// rien de bloquant.
    Failed(AssistantError),
}

/// Relecture du solde après l'ouverture de la page de paiement.
#[derive(Clone, Copy, Debug, PartialEq)]
struct CheckoutWatch {
    /// Heure d'egui de la première image après l'ouverture.
    started: Option<f64>,
    /// Heure de la dernière relecture.
    last_poll: Option<f64>,
    /// Solde de référence (le plus bas vu depuis l'ouverture), centimes.
    baseline: Option<i64>,
}

/// Étape de la connexion au compte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountStage {
    /// Lecture du magasin en cours.
    Loading,
    /// Saisie de l'adresse.
    Email,
    /// Lien et code envoyés à l'adresse : saisie du code.
    Code(String),
    /// Connecté.
    SignedIn,
}

/// Appel au relais en attente.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Call {
    /// Lien magique demandé pour l'adresse.
    MagicLink(String),
    /// Échange du code ou du lien.
    Exchange,
    /// Compte (`/v1/me`).
    Me,
    /// Consommation (`/v1/usage`).
    Usage,
    /// Paiement (`/v1/credits/checkout`).
    Checkout,
    /// Achats (`/v1/purchases`).
    Purchases,
    /// Déconnexion (révocation, sans suite).
    Logout,
}

/// Lecture du magasin du jeton.
enum SessionLoad {
    NotStarted,
    Loading(Background<Loaded>),
    Done,
}

/// État du compte (jamais sérialisé).
#[allow(
    clippy::struct_excessive_bools,
    reason = "drapeaux indépendants (vue Consommation, fenêtre, focus, relecture au focus)"
)]
pub struct Account {
    token: Option<String>,
    store: Option<SharedSecrets>,
    load: SessionLoad,
    save: Option<Background<Result<(), AssistantError>>>,
    calls: Vec<(Call, u64, PendingResponse)>,
    /// Génération de session : change à chaque connexion et déconnexion.
    generation: u64,
    /// Adresse à laquelle le dernier lien a été envoyé.
    sent_to: Option<String>,
    /// Adresse en cours de saisie.
    pub email_draft: String,
    /// Code en cours de saisie.
    pub code_draft: String,
    /// Dernier état du compte lu (`/v1/me`), solde mis à jour par le coût
    /// de chaque complétion.
    pub me: Option<Me>,
    /// Consommation lue (`/v1/usage`).
    pub usage: Option<UsageReport>,
    /// Vue « Consommation » affichée.
    pub usage_open: bool,
    /// Information à afficher.
    pub info: Option<AccountInfo>,
    /// Solde à relire quand la fenêtre reprend le focus (après l'ouverture
    /// de la page de paiement).
    refresh_on_focus: bool,
    focused: bool,
    /// Adresses ouvertes dans le navigateur (tests : navigateur simulé).
    opened: Vec<String>,
    /// Fenêtre « Mon compte » ouverte (H-UI-238).
    window_open: bool,
    /// Achats lus pour la fenêtre.
    purchases: Purchases,
    /// Relecture du solde après l'ouverture de la page de paiement.
    watch: Option<CheckoutWatch>,
    /// Crédits ajoutés à annoncer (notification de l'application).
    credits_added: Option<i64>,
}

impl Default for Account {
    fn default() -> Self {
        Self {
            token: None,
            store: None,
            load: SessionLoad::NotStarted,
            save: None,
            calls: Vec::new(),
            generation: 0,
            sent_to: None,
            email_draft: String::new(),
            code_draft: String::new(),
            me: None,
            usage: None,
            usage_open: false,
            info: None,
            refresh_on_focus: false,
            focused: true,
            opened: Vec::new(),
            window_open: false,
            purchases: Purchases::Idle,
            watch: None,
            credits_added: None,
        }
    }
}

impl std::fmt::Debug for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Account")
            .field("signed_in", &self.token.is_some())
            .field("me", &self.me)
            .finish_non_exhaustive()
    }
}

impl Account {
    /// Connecté (jeton connu).
    #[must_use]
    pub const fn signed_in(&self) -> bool {
        self.token.is_some()
    }

    /// Étape affichée.
    #[must_use]
    pub fn stage(&self) -> AccountStage {
        if self.token.is_some() {
            AccountStage::SignedIn
        } else if matches!(self.load, SessionLoad::Loading(_)) {
            AccountStage::Loading
        } else if let Some(email) = &self.sent_to {
            AccountStage::Code(email.clone())
        } else {
            AccountStage::Email
        }
    }

    /// Lecture du magasin en cours.
    #[must_use]
    pub const fn loading(&self) -> bool {
        matches!(self.load, SessionLoad::Loading(_))
    }

    /// Connexion en cours (lien demandé ou échange en attente).
    #[must_use]
    pub fn working(&self) -> bool {
        self.calls
            .iter()
            .any(|(c, _, _)| matches!(c, Call::MagicLink(_) | Call::Exchange | Call::Checkout))
    }

    /// Le jeton survit à la fermeture (trousseau).
    #[must_use]
    pub fn persistent(&self) -> bool {
        self.store.as_ref().is_some_and(|s| s.persistent())
    }

    /// Adresses ouvertes dans le navigateur (page de paiement, reçus).
    #[must_use]
    pub fn opened(&self) -> &[String] {
        &self.opened
    }

    /// Fenêtre « Mon compte » ouverte.
    #[must_use]
    pub const fn window_open(&self) -> bool {
        self.window_open
    }

    /// Achats lus pour la fenêtre « Mon compte ».
    #[must_use]
    pub const fn purchases(&self) -> &Purchases {
        &self.purchases
    }

    /// Consommation en cours de lecture.
    #[must_use]
    pub fn usage_loading(&self) -> bool {
        self.pending(&Call::Usage)
    }

    /// Le solde est relu en attendant le crédit d'un paiement (H-UI-242).
    #[must_use]
    pub const fn watching_checkout(&self) -> bool {
        self.watch.is_some()
    }

    fn pending(&self, call: &Call) -> bool {
        self.calls.iter().any(|(c, _, _)| c == call)
    }

    /// Jeton de session (demande relayée ; jamais affiché ni sérialisé).
    pub(super) fn token(&self) -> Option<String> {
        self.token.clone()
    }

    /// Solde connu, centimes.
    #[must_use]
    pub fn balance(&self) -> Option<i64> {
        self.me.as_ref().map(|m| m.balance_cents)
    }
}

impl AssistantPanel {
    /// Lance la lecture du magasin de l'accès choisi (assistant activé).
    pub fn start_loads(&mut self, prefs: &AssistantPrefs) {
        if !prefs.enabled {
            return;
        }
        match prefs.access {
            AssistantAccess::OwnKey => self.start_key_load(),
            AssistantAccess::Account => self.start_session_load(),
        }
    }

    /// Lit le jeton de session dans le magasin (hors du fil de
    /// l'interface).
    pub fn start_session_load(&mut self) {
        if !matches!(self.account.load, SessionLoad::NotStarted) {
            return;
        }
        let store = self.services.session_secrets.clone();
        self.account.load = SessionLoad::Loading(Background::spawn(
            move || {
                let (store, warning) = match store {
                    Some(store) => (store, None),
                    None => plannercraft_assistant::secrets::open_session(),
                };
                let token = store.load();
                (store, warning, token)
            },
            self.waker.clone(),
        ));
    }

    /// Lecture du jeton en cours (tests : attendre sa fin).
    #[must_use]
    pub const fn session_loading(&self) -> bool {
        self.account.loading()
    }

    /// Compte PlannerCraft (affichage, tests).
    #[must_use]
    pub const fn account(&self) -> &Account {
        &self.account
    }

    /// Compte PlannerCraft, modifiable (saisie de l'adresse et du code).
    pub const fn account_mut(&mut self) -> &mut Account {
        &mut self.account
    }

    fn call(&mut self, call: Call, request: plannercraft_assistant::http::HttpRequest) {
        let pending = PendingResponse::send(self.services.transport.as_ref(), request, self.waker.clone());
        self.account.calls.push((call, self.account.generation, pending));
    }

    fn base(&self) -> String {
        self.services.relay_url.clone()
    }

    /// « Recevoir le lien » / « Renvoyer le code ».
    pub(super) fn send_link(&mut self, resend: bool) {
        if self.account.working() {
            return;
        }
        let email = if resend {
            self.account.sent_to.clone().unwrap_or_default()
        } else {
            self.account
                .email_draft
                .trim()
                .chars()
                .take(relay::MAX_EMAIL_CHARS)
                .collect()
        };
        if !relay::plausible_email(&email) {
            return;
        }
        self.clear_notice();
        let request = relay::magic_link_request(&self.base(), &email);
        self.call(Call::MagicLink(email), request);
    }

    /// « Valider » : échange de l'adresse et du code.
    pub(super) fn verify_code(&mut self) {
        let Some(email) = self.account.sent_to.clone() else {
            return;
        };
        let code: String = self
            .account
            .code_draft
            .trim()
            .chars()
            .take(relay::MAX_CODE_CHARS)
            .collect();
        if !relay::plausible_code(&code) || self.account.working() {
            return;
        }
        self.clear_notice();
        let request = relay::exchange_code_request(&self.base(), &email, &code);
        self.call(Call::Exchange, request);
    }

    /// Lien magique ouvert (web : `…/login?token=…`, lu au démarrage de la
    /// page) : l'accès passe au compte, l'assistant est activé (le lien a
    /// été demandé depuis l'onglet, après la mention de transparence) et le
    /// jeton du lien est échangé contre une session.
    pub fn sign_in_with_link(&mut self, token: &str, prefs: &mut AssistantPrefs) {
        let token = token.trim();
        if token.is_empty() {
            return;
        }
        prefs.access = AssistantAccess::Account;
        prefs.enabled = true;
        self.start_session_load();
        let request = relay::exchange_token_request(&self.base(), token);
        self.call(Call::Exchange, request);
    }

    /// « Changer d'adresse ».
    pub(super) fn change_email(&mut self) {
        self.account.sent_to = None;
        self.account.code_draft.clear();
    }

    /// Relit le compte (`/v1/me`).
    pub fn refresh_account(&mut self) {
        let Some(token) = self.account.token.clone() else {
            return;
        };
        if self.account.calls.iter().any(|(c, _, _)| *c == Call::Me) {
            return;
        }
        let request = relay::me_request(&self.base(), &token);
        self.call(Call::Me, request);
    }

    /// Ouvre ou ferme la vue « Consommation » (relue à chaque ouverture).
    pub(super) fn show_usage(&mut self, open: bool) {
        self.account.usage_open = open;
        if !open {
            return;
        }
        let Some(token) = self.account.token.clone() else {
            self.account.usage_open = false;
            return;
        };
        self.account.usage = None;
        let request = relay::usage_request(&self.base(), &token);
        self.call(Call::Usage, request);
    }

    /// « Acheter des crédits » : session de paiement (origine `client`
    /// des services, H-UI-243), puis navigateur.
    pub(super) fn checkout(&mut self, pack: &str) {
        let Some(token) = self.account.token.clone() else {
            return;
        };
        if relay::pack(pack).is_none() || self.account.working() {
            return;
        }
        self.clear_notice();
        let request = relay::checkout_request(&self.base(), &token, pack, self.services.client);
        self.call(Call::Checkout, request);
    }

    /// Ouvre ou ferme la fenêtre « Mon compte » (H-UI-238) ; à
    /// l'ouverture, compte, consommation et achats sont relus.
    pub(super) fn show_account_window(&mut self, open: bool) {
        self.account.window_open = open;
        if open {
            self.refresh_account();
            self.fetch_window_data();
        }
    }

    /// Consommation et achats de la fenêtre (connecté seulement ; un appel
    /// déjà en cours n'est pas doublé ; les données lues restent affichées
    /// pendant la relecture).
    fn fetch_window_data(&mut self) {
        let Some(token) = self.account.token.clone() else {
            return;
        };
        if !self.account.pending(&Call::Usage) {
            let request = relay::usage_request(&self.base(), &token);
            self.call(Call::Usage, request);
        }
        self.fetch_purchases();
    }

    fn fetch_purchases(&mut self) {
        let Some(token) = self.account.token.clone() else {
            return;
        };
        if self.account.pending(&Call::Purchases) {
            return;
        }
        if !matches!(self.account.purchases, Purchases::Ready(_)) {
            self.account.purchases = Purchases::Loading;
        }
        let request = relay::purchases_request(&self.base(), &token);
        self.call(Call::Purchases, request);
    }

    /// « Reçu ↗ » (H-UI-240) : seule l'adresse d'un reçu de la liste lue
    /// (déjà filtrée `https`) est ouverte.
    pub(super) fn open_receipt(&mut self, url: &str) {
        let known = match &self.account.purchases {
            Purchases::Ready(report) => report.items.iter().any(|p| p.receipt_url.as_deref() == Some(url)),
            _ => false,
        };
        if !known || !relay::safe_https(url) {
            return;
        }
        if let Err(detail) = self.open_in_browser(url) {
            self.notice = Some(Notice::Error(AssistantError::Network { detail }));
        }
    }

    /// Relecture du solde après l'ouverture de la page de paiement
    /// (H-UI-242), à chaque image : `now` est l'heure d'egui (secondes).
    /// Rend le délai avant la prochaine relecture (l'application demande
    /// une image à ce moment), `None` hors attente.
    pub fn tick_account(&mut self, now: f64) -> Option<f64> {
        let watch = self.account.watch.as_mut()?;
        let started = *watch.started.get_or_insert(now);
        let last = *watch.last_poll.get_or_insert(now);
        if now - started > CHECKOUT_WATCH_SECS {
            self.account.watch = None;
            return None;
        }
        if now - last >= CHECKOUT_POLL_SECS {
            watch.last_poll = Some(now);
            self.refresh_account();
            return Some(CHECKOUT_POLL_SECS);
        }
        Some((CHECKOUT_POLL_SECS - (now - last)).max(0.0))
    }

    /// Crédits ajoutés depuis la dernière lecture (centimes), à annoncer
    /// une fois par une notification.
    pub const fn take_credits_added(&mut self) -> Option<i64> {
        self.account.credits_added.take()
    }

    /// Compte lu (`/v1/me`) : pendant l'attente d'un paiement, un solde
    /// plus haut que la référence est annoncé (« Crédits ajoutés »),
    /// l'attente s'arrête et les achats sont relus si la fenêtre est
    /// ouverte ; un solde plus bas (demande payée entre-temps) devient la
    /// référence.
    fn receive_me(&mut self, me: Me) {
        let balance = me.balance_cents;
        self.account.me = Some(me);
        let Some(watch) = &mut self.account.watch else {
            return;
        };
        match watch.baseline {
            Some(base) if balance > base => {
                let added = balance.saturating_sub(base);
                self.account.watch = None;
                self.account.refresh_on_focus = false;
                self.account.info = Some(AccountInfo::CreditsAdded(added));
                self.account.credits_added = Some(added);
                if self.account.window_open {
                    self.fetch_window_data();
                }
            }
            Some(base) if balance >= base => {}
            _ => watch.baseline = Some(balance),
        }
    }

    /// « Se déconnecter du compte » : jeton révoqué sur le relais (sans
    /// attendre), puis oublié ici et dans le magasin.
    pub(super) fn account_logout(&mut self) {
        if let Some(token) = self.account.token.clone() {
            let request = relay::logout_request(&self.base(), &token);
            self.call(Call::Logout, request);
        }
        self.conversation.stop();
        self.forget_session();
    }

    /// Session refusée par le relais : retour à l'écran de connexion avec
    /// le message.
    pub(super) fn end_session(&mut self, error: AssistantError) {
        self.forget_session();
        self.notice = Some(Notice::Error(error));
    }

    fn forget_session(&mut self) {
        self.account.token = None;
        self.account.me = None;
        self.account.usage = None;
        self.account.usage_open = false;
        self.account.info = None;
        self.account.refresh_on_focus = false;
        self.account.purchases = Purchases::Idle;
        self.account.watch = None;
        self.account.credits_added = None;
        self.account.generation = self.account.generation.wrapping_add(1);
        if let Some(store) = self.account.store.clone() {
            self.account.save = Some(Background::spawn(move || store.delete(), self.waker.clone()));
        }
    }

    fn set_session(&mut self, token: &str) {
        let token = token.trim().to_owned();
        if token.is_empty() {
            return;
        }
        self.account.generation = self.account.generation.wrapping_add(1);
        self.account.token = Some(token.clone());
        self.account.sent_to = None;
        self.account.code_draft.clear();
        self.clear_notice();
        match (&self.account.load, self.account.store.clone()) {
            (SessionLoad::Done, Some(store)) => {
                self.account.save = Some(Background::spawn(move || store.save(&token), self.waker.clone()));
            }
            // Enregistré quand le magasin aura été lu.
            _ => self.start_session_load(),
        }
        self.refresh_account();
        if self.account.window_open {
            self.fetch_window_data();
        }
    }

    fn clear_notice(&mut self) {
        self.notice = self.notice.take().filter(|n| *n == Notice::KeyringMissing);
    }

    /// Focus de la fenêtre : après l'ouverture de la page de paiement, le
    /// solde est relu au retour dans l'application.
    pub fn set_focused(&mut self, focused: bool) {
        let regained = focused && !self.account.focused;
        self.account.focused = focused;
        if regained && (self.account.refresh_on_focus || self.account.watch.is_some()) {
            self.account.refresh_on_focus = false;
            self.refresh_account();
        }
    }

    /// Après une image de conversation : solde annoncé par le relais,
    /// session refusée, `/v1/me` relu à la fin de chaque demande.
    pub fn after_conversation_poll(&mut self, was_busy: bool, access: AssistantAccess) {
        if let Some(balance) = self.conversation.take_balance() {
            if let Some(me) = &mut self.account.me {
                me.balance_cents = balance;
            }
            // Demande payée pendant l'attente d'un paiement : la référence
            // suit le solde vers le bas.
            if let Some(watch) = &mut self.account.watch {
                watch.baseline = Some(watch.baseline.map_or(balance, |b| b.min(balance)));
            }
        }
        if self.conversation.take_session_ended() {
            let error = self
                .conversation
                .turns()
                .last()
                .and_then(|t| match &t.state {
                    plannercraft_assistant::TurnState::Failed(e) => Some(e.clone()),
                    _ => None,
                })
                .unwrap_or(AssistantError::SessionEnded { expired: false });
            self.end_session(error);
            return;
        }
        if was_busy && !self.conversation.busy() && access == AssistantAccess::Account {
            self.refresh_account();
        }
    }

    /// Lit le magasin et les réponses du relais ; vrai si quelque chose a
    /// changé.
    pub(super) fn poll_account(&mut self, prefs: &mut AssistantPrefs) -> bool {
        let mut changed = false;
        #[cfg(target_arch = "wasm32")]
        if !self.account_returns_checked {
            self.account_returns_checked = true;
            if let Some(token) = plannercraft_assistant::web::take_magic_token() {
                self.sign_in_with_link(&token, prefs);
                changed = true;
            }
            if let Some(paid) = plannercraft_assistant::web::take_credits_return() {
                self.account.info = Some(if paid {
                    AccountInfo::CheckoutReturn
                } else {
                    AccountInfo::CheckoutCancelled
                });
                changed = true;
            }
        }
        let _ = &prefs;
        if let SessionLoad::Loading(task) = &self.account.load
            && let Some((store, warning, token)) = task.take()
        {
            changed = true;
            self.account.load = SessionLoad::Done;
            self.account.store = Some(Arc::clone(&store));
            if warning.is_some() {
                self.notice = Some(Notice::KeyringMissing);
            }
            match token {
                // Une session ouverte pendant la lecture l'emporte.
                Ok(_) if self.account.token.is_some() => {
                    if let Some(token) = self.account.token.clone() {
                        self.account.save =
                            Some(Background::spawn(move || store.save(&token), self.waker.clone()));
                    }
                }
                Ok(found) => {
                    self.account.token = found.filter(|t| !t.trim().is_empty());
                    self.refresh_account();
                }
                Err(error) => self.notice = Some(Notice::Error(error)),
            }
        }
        if let Some(save) = &self.account.save
            && let Some(result) = save.take()
        {
            changed = true;
            self.account.save = None;
            if let Err(error) = result {
                self.notice = Some(Notice::Error(error));
            }
        }
        let calls = std::mem::take(&mut self.account.calls);
        for (call, generation, pending) in calls {
            let Some(received) = pending.poll() else {
                self.account.calls.push((call, generation, pending));
                continue;
            };
            changed = true;
            if generation != self.account.generation && call != Call::Exchange {
                continue;
            }
            self.finish_call(call, received);
        }
        changed
    }

    fn finish_call(
        &mut self,
        call: Call,
        received: Result<plannercraft_assistant::login::Received, AssistantError>,
    ) {
        let outcome = received.and_then(|(status, headers, body)| match call {
            Call::MagicLink(_) => relay::read_magic_link(status, &headers, &body).map(|()| Done::Sent),
            Call::Exchange => relay::read_exchange(status, &headers, &body).map(|s| Done::Session(s.token)),
            Call::Me => relay::read_me(status, &headers, &body).map(Done::Me),
            Call::Usage => relay::read_usage(status, &headers, &body).map(Done::Usage),
            Call::Checkout => relay::read_checkout(status, &headers, &body).map(Done::Checkout),
            Call::Purchases => relay::read_purchases(status, &headers, &body).map(Done::Purchases),
            Call::Logout => Ok(Done::Sent),
        });
        match (call, outcome) {
            (Call::Logout, _) => {}
            (Call::MagicLink(email), Ok(_)) => {
                self.account.sent_to = Some(email);
                self.account.code_draft.clear();
            }
            (_, Ok(Done::Session(token))) => self.set_session(&token),
            (_, Ok(Done::Me(me))) => self.receive_me(me),
            (_, Ok(Done::Purchases(Some(report)))) => self.account.purchases = Purchases::Ready(report),
            (_, Ok(Done::Purchases(None))) => self.account.purchases = Purchases::Unavailable,
            (_, Ok(Done::Usage(usage))) => self.account.usage = Some(usage),
            (_, Ok(Done::Checkout(url))) => self.open_checkout(&url),
            (_, Ok(Done::Sent)) => {}
            (Call::Exchange | Call::MagicLink(_), Err(error)) => self.notice = Some(Notice::Error(error)),
            (_, Err(error @ AssistantError::SessionEnded { .. })) => self.end_session(error),
            // Fenêtre « Mon compte » (vue « Consommation » du volet fermée) :
            // section « indisponible », rien de bloquant.
            (Call::Usage, Err(_)) if self.account.window_open && !self.account.usage_open => {
                self.account.usage = None;
            }
            (Call::Usage, Err(error)) => {
                self.account.usage_open = false;
                self.notice = Some(Notice::Error(error));
            }
            (Call::Purchases, Err(error)) => self.account.purchases = Purchases::Failed(error),
            (_, Err(error)) => self.notice = Some(Notice::Error(error)),
        }
    }

    /// Ouvre `url` dans le navigateur du système (desktop) ou un nouvel
    /// onglet (web).
    fn open_in_browser(&mut self, url: &str) -> Result<(), String> {
        #[cfg(not(target_arch = "wasm32"))]
        let opened = match &self.services.browser {
            Some(browser) => browser.open(url),
            None => Err("navigateur indisponible".to_owned()),
        };
        #[cfg(target_arch = "wasm32")]
        let opened = plannercraft_assistant::web::open_tab(url);
        if opened.is_ok() {
            self.account.opened.push(url.to_owned());
        }
        opened
    }

    fn open_checkout(&mut self, url: &str) {
        match self.open_in_browser(url) {
            Ok(()) => {
                self.account.info = Some(AccountInfo::CheckoutOpened);
                self.account.refresh_on_focus = true;
                // Solde relu jusqu'à ce qu'il augmente (H-UI-242).
                self.account.watch = Some(CheckoutWatch {
                    started: None,
                    last_poll: None,
                    baseline: self.account.balance(),
                });
            }
            Err(detail) => {
                self.notice = Some(Notice::Error(AssistantError::Network { detail }));
            }
        }
    }
}

/// Réponse lue d'un appel.
enum Done {
    Sent,
    Session(String),
    Me(Me),
    Usage(UsageReport),
    Checkout(String),
    Purchases(Option<PurchasesReport>),
}
