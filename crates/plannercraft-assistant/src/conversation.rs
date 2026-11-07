//! Conversation avec le modèle : tours (question, raisonnement, lectures,
//! réponse), boucle d'outils bornée, sans fil propre.
//!
//! La conversation est une machine à états tenue par l'interface :
//! [`Conversation::send`] lance une requête (le transport ne bloque jamais
//! l'appelant), les évènements HTTP arrivent dans une file partagée
//! (réveil de l'interface par [`Waker`]), et [`Conversation::poll`],
//! appelé à chaque image, les lit : raisonnement et texte en flux, puis, si
//! le modèle demande des outils, exécution par l'hôte (lecture sur une
//! copie de la session, sur le fil de l'interface) et tour suivant. Même
//! code sur le desktop et le web.
//!
//! Lot A1b : la transcription est une suite de [`Turn`] ; chaque tour garde
//! ses messages au format de l'API ([`Turn::wire`]) pour la mémoire de
//! conversation ([`crate::memory`]) ; [`Conversation::saved`] et
//! [`Conversation::restore`] servent la persistance par document
//! ([`crate::archive`]). L'heure n'est jamais lue ici : l'interface donne
//! ses secondes par [`Conversation::set_now`].
//!
//! Bornes (règle 10) : [`MAX_ROUNDS`] tours d'outils par demande (le
//! dernier sans outil permis : `tool_choice: "none"`), réponse de
//! [`MAX_RESPONSE_BYTES`] octets au plus par tour, file de
//! [`MAX_QUEUED_EVENTS`] évènements, [`MAX_TURNS`] tours gardés.

use std::collections::VecDeque;
use std::fmt;
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use plannercraft_automation::text::Language;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::error::AssistantError;
use crate::http::{HttpEvent, Transport};
use crate::memory;
use crate::openrouter;
use crate::proposal::{self, PROPOSE_TOOL, ProposalCard, ProposalStatus};
use crate::relay;
use crate::sse::SseParser;
use crate::stream::{StreamAccumulator, ToolCall, Usage};
use crate::tools::{self, ToolHost};

/// Tours au plus par demande (appels d'outils compris).
pub const MAX_ROUNDS: u32 = 6;
/// Octets reçus au plus par tour.
pub const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
/// Évènements en attente au plus dans la file.
pub const MAX_QUEUED_EVENTS: usize = 8192;
/// Corps d'erreur HTTP gardé au plus.
const MAX_ERROR_BODY: usize = 64 * 1024;
/// Tours gardés au plus dans la conversation (les plus anciens tombent).
pub const MAX_TURNS: usize = 100;
/// Arguments d'un appel d'outil gardés pour l'affichage (octets).
const MAX_SHOWN_ARGUMENTS: usize = 512;
/// Version du format sérialisé d'une conversation.
pub const SAVED_VERSION: u32 = 1;

/// Réveil de l'interface (egui : `Context::request_repaint`).
pub type Waker = Arc<dyn Fn() + Send + Sync>;

/// Destination des complétions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Endpoint {
    /// Mode « ma clé » : OpenRouter directement, avec la clé de
    /// l'utilisateur.
    #[default]
    OpenRouter,
    /// Mode « Compte PlannerCraft » (lot A4) : relais de l'éditeur, avec
    /// le jeton de session ; aucune clé OpenRouter.
    Relay {
        /// Adresse du relais, sans `/` final.
        base_url: String,
        /// Identifiant de la demande (`X-PlannerCraft-Request`), tiré à
        /// chaque envoi, régénération ou reprise ; vide avant.
        request_id: String,
    },
}

impl Endpoint {
    /// Relais de PlannerCraft d'adresse `base_url`.
    #[must_use]
    pub fn relay(base_url: &str) -> Self {
        Self::Relay {
            base_url: relay::normalize_base(base_url),
            request_id: String::new(),
        }
    }

    /// Complétions par le relais.
    #[must_use]
    pub const fn is_relay(&self) -> bool {
        matches!(self, Self::Relay { .. })
    }
}

/// Réglages d'une demande. La clé n'apparaît jamais dans `Debug` et n'est
/// jamais gardée dans un tour.
#[derive(Clone)]
pub struct RequestConfig {
    /// Clé OpenRouter (mode « ma clé ») ou jeton de session du relais
    /// (mode « Compte PlannerCraft »).
    pub api_key: String,
    /// Destination des complétions.
    pub endpoint: Endpoint,
    /// Modèle (liste fermée).
    pub model: String,
    /// Langue de l'interface (invite système, messages des outils).
    pub lang: Language,
    /// Jetons de sortie au plus par tour.
    pub max_tokens: u32,
    /// Contexte de l'interface (sélection, vue) joint à la question en
    /// cours seulement, jamais gardé dans l'historique.
    pub context: Option<String>,
    /// Heure de la question, affichée en infobulle (« 14:32 »).
    pub asked_at: Option<String>,
    /// Budget de réflexion demandé (`reasoning.max_tokens`), `None` : le
    /// paramètre n'est pas envoyé (défaut, lot A2).
    pub reasoning_budget: Option<u32>,
}

impl RequestConfig {
    /// Réglages minimaux (sans contexte ni heure).
    #[must_use]
    pub fn new(api_key: &str, model: &str, lang: Language, max_tokens: u32) -> Self {
        Self {
            api_key: api_key.to_owned(),
            endpoint: Endpoint::OpenRouter,
            model: model.to_owned(),
            lang,
            max_tokens,
            context: None,
            asked_at: None,
            reasoning_budget: None,
        }
    }
}

impl fmt::Debug for RequestConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestConfig")
            .field("api_key", &"<masquée>")
            .field("endpoint", &self.endpoint)
            .field("model", &self.model)
            .field("lang", &self.lang)
            .field("max_tokens", &self.max_tokens)
            .field("context", &self.context.is_some())
            .finish_non_exhaustive()
    }
}

/// État d'un tour.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "error", rename_all = "snake_case")]
pub enum TurnState {
    /// Réponse en cours.
    Streaming,
    /// Réponse complète.
    Done,
    /// Demande en échec.
    Failed(AssistantError),
    /// Arrêtée par l'utilisateur (texte partiel gardé).
    Stopped,
}

/// Lecture faite par le modèle pendant un tour.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolStep {
    /// Nom de l'outil (`critical_path`) : l'interface le traduit en
    /// libellé métier, jamais affiché tel quel.
    pub name: String,
    /// Arguments (JSON, tronqués pour l'affichage).
    #[serde(default)]
    pub arguments: String,
    /// Erreur rendue au modèle (code et message), `None` si la lecture a
    /// réussi.
    #[serde(default)]
    pub error: Option<String>,
    /// Proposition soumise par cet appel (outil
    /// [`crate::proposal::PROPOSE_TOOL`] accepté en file, lot A2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal: Option<ProposalCard>,
}

/// Tour de conversation : une question et ce qui y répond.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Turn {
    /// Question de l'utilisateur.
    pub question: String,
    /// Heure de la question (infobulle).
    #[serde(default)]
    pub asked_at: Option<String>,
    /// Modèle interrogé.
    #[serde(default)]
    pub model: String,
    /// Raisonnement lisible renvoyé par le modèle (vide s'il n'en donne
    /// pas).
    #[serde(default)]
    pub reasoning: String,
    /// Durée du raisonnement, en secondes.
    #[serde(default)]
    pub reasoning_secs: Option<u32>,
    /// Lectures du planning, dans l'ordre.
    #[serde(default)]
    pub tools: Vec<ToolStep>,
    /// Texte de la réponse (Markdown).
    #[serde(default)]
    pub answer: String,
    /// Jetons et coût (somme des tours d'outils).
    #[serde(default)]
    pub usage: Option<Usage>,
    /// État.
    pub state: TurnState,
    /// Messages du tour au format de l'API (question, appels d'outils et
    /// résultats, réponse), renvoyés à la demande suivante.
    #[serde(default)]
    pub wire: Vec<Value>,
    /// Début du raisonnement en cours (secondes de l'interface).
    #[serde(skip)]
    reasoning_started: Option<f64>,
}

impl Turn {
    /// Tour neuf, en cours.
    #[must_use]
    pub fn new(question: &str, model: &str, asked_at: Option<String>) -> Self {
        Self {
            question: question.to_owned(),
            asked_at,
            model: model.to_owned(),
            reasoning: String::new(),
            reasoning_secs: None,
            tools: Vec::new(),
            answer: String::new(),
            usage: None,
            state: TurnState::Streaming,
            wire: vec![json!({ "role": "user", "content": question })],
            reasoning_started: None,
        }
    }

    /// Réponse en cours.
    #[must_use]
    pub const fn streaming(&self) -> bool {
        matches!(self.state, TurnState::Streaming)
    }

    fn reasoning_tick(&mut self, now: f64) {
        if self.reasoning_started.is_none() {
            self.reasoning_started = Some(now);
        }
    }

    fn reasoning_end(&mut self, now: f64) {
        if let Some(start) = self.reasoning_started.take() {
            let secs = whole_seconds(now - start);
            self.reasoning_secs = Some(self.reasoning_secs.unwrap_or(0).saturating_add(secs));
        }
    }
}

/// Secondes entières de `d` (arrondi, bornées à un jour).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "valeur bornée à [0, 86 400] avant la conversion"
)]
fn whole_seconds(d: f64) -> u32 {
    if d.is_finite() {
        d.clamp(0.0, 86_400.0).round() as u32
    } else {
        0
    }
}

/// Conversation gardée (persistance par document) : tours terminés, aucun
/// secret.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedConversation {
    /// Version du format.
    #[serde(default)]
    pub version: u32,
    /// Tours.
    #[serde(default)]
    pub turns: Vec<Turn>,
}

/// Activité en cours (barre d'état du volet).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Activity {
    /// Rien en cours.
    Idle,
    /// Requête envoyée, rien reçu encore.
    Waiting,
    /// Le modèle raisonne (raisonnement en flux, pas encore de texte).
    Reasoning,
    /// Le modèle lit le planning : outil demandé (nom) en cours ou dont il
    /// lit le résultat.
    Reading(String),
    /// Réponse en cours d'écriture.
    Writing,
}

type Inbox = Arc<Mutex<VecDeque<(u64, HttpEvent)>>>;

/// Tour d'API en cours.
struct Running {
    request: u64,
    round: u32,
    config: RequestConfig,
    cancel: Arc<AtomicBool>,
    usage: Usage,
    status: Option<u16>,
    headers: Vec<(String, String)>,
    error_body: Vec<u8>,
    received: usize,
    sse: SseParser,
    acc: StreamAccumulator,
    /// Du texte est arrivé dans ce tour d'API.
    text_started: bool,
    /// Le texte de ce tour d'API se sépare du précédent par un paragraphe
    /// (tour qui suit des lectures).
    separate: bool,
    /// Secondes de l'interface au départ de la demande.
    started_at: f64,
}

impl Running {
    /// Jetons et coût de la demande (tours précédents et tour en cours).
    /// Relais : le coût d'OpenRouter en dollars (`usage.cost`, relayé tel
    /// quel) n'est pas ce que paie l'utilisateur ; seul le coût débité
    /// ([`Usage::cost_cents`]) est gardé.
    fn total_usage(&self) -> Usage {
        let mut usage = self.usage.plus(self.acc.usage().unwrap_or_default());
        if self.config.endpoint.is_relay() {
            usage.cost = None;
        }
        usage
    }
}

/// Conversation.
pub struct Conversation {
    turns: Vec<Turn>,
    running: Option<Running>,
    inbox: Inbox,
    next_request: u64,
    waker: Option<Waker>,
    requests_sent: u64,
    now: f64,
    /// Compteur de modifications (persistance, cache de rendu).
    revision: u64,
    /// Décisions de l'utilisateur sur des propositions (libellé, issue),
    /// pas encore dites au modèle : jointes au prochain message envoyé.
    decisions: Vec<(String, ProposalStatus)>,
    /// Solde annoncé par le relais (centimes), pas encore lu par
    /// l'interface (lot A4).
    balance: Option<i64>,
    /// Une demande vient d'échouer sur une session refusée par le relais
    /// (lot A4) : l'interface revient à l'écran de connexion.
    session_ended: bool,
}

impl Default for Conversation {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Conversation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Conversation")
            .field("turns", &self.turns.len())
            .field("busy", &self.busy())
            .finish_non_exhaustive()
    }
}

impl Conversation {
    /// Conversation vide.
    #[must_use]
    pub fn new() -> Self {
        Self {
            turns: Vec::new(),
            running: None,
            inbox: Arc::new(Mutex::new(VecDeque::new())),
            next_request: 1,
            waker: None,
            requests_sent: 0,
            now: 0.0,
            revision: 0,
            decisions: Vec::new(),
            balance: None,
            session_ended: false,
        }
    }

    /// Solde du compte annoncé par le relais depuis la dernière lecture
    /// (centimes, une fois ; lot A4).
    pub const fn take_balance(&mut self) -> Option<i64> {
        self.balance.take()
    }

    /// Vrai une fois après un échec `SessionEnded` (session du relais
    /// expirée ou refusée, lot A4).
    pub const fn take_session_ended(&mut self) -> bool {
        std::mem::replace(&mut self.session_ended, false)
    }

    /// Réveil appelé à chaque évènement reçu.
    pub fn set_waker(&mut self, waker: Waker) {
        self.waker = Some(waker);
    }

    /// Un réveil est posé.
    #[must_use]
    pub const fn has_waker(&self) -> bool {
        self.waker.is_some()
    }

    /// Heure de l'interface (secondes, monotone) : durées du raisonnement
    /// et chronomètre. Jamais lue ailleurs.
    pub fn set_now(&mut self, now: f64) {
        if now.is_finite() {
            self.now = now;
        }
    }

    /// Tours de la conversation.
    #[must_use]
    pub fn turns(&self) -> &[Turn] {
        &self.turns
    }

    /// Aucun tour.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.turns.is_empty()
    }

    /// Compteur de modifications : change à chaque évènement lu, envoi,
    /// arrêt, effacement.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    /// Requêtes HTTP envoyées depuis la création (tests).
    #[must_use]
    pub const fn requests_sent(&self) -> u64 {
        self.requests_sent
    }

    /// Une demande est en cours.
    #[must_use]
    pub const fn busy(&self) -> bool {
        self.running.is_some()
    }

    /// Secondes écoulées depuis le départ de la demande en cours.
    #[must_use]
    pub fn elapsed(&self) -> Option<f64> {
        self.running.as_ref().map(|r| (self.now - r.started_at).max(0.0))
    }

    /// Lectures demandées par le modèle dans le flux en cours, pas encore
    /// exécutées (noms).
    #[must_use]
    pub fn pending_tools(&self) -> Vec<String> {
        self.running
            .as_ref()
            .map(|r| {
                r.acc
                    .tool_calls()
                    .into_iter()
                    .map(|c| c.name)
                    .filter(|n| !n.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Activité en cours.
    #[must_use]
    pub fn activity(&self) -> Activity {
        let Some(r) = &self.running else {
            return Activity::Idle;
        };
        if let Some(name) = self.pending_tools().pop() {
            return Activity::Reading(name);
        }
        if r.text_started {
            return Activity::Writing;
        }
        if !r.acc.reasoning().is_empty() {
            return Activity::Reasoning;
        }
        if r.round > 0
            && let Some(step) = self.turns.last().and_then(|t| t.tools.last())
        {
            return Activity::Reading(step.name.clone());
        }
        Activity::Waiting
    }

    /// Oublie la conversation (arrête la demande en cours).
    pub fn clear(&mut self) {
        if let Some(running) = self.running.take() {
            running.cancel.store(true, Ordering::Relaxed);
        }
        self.turns.clear();
        self.touch();
    }

    /// Envoie une question.
    ///
    /// # Errors
    ///
    /// [`AssistantError::Busy`] si une demande est en cours,
    /// [`AssistantError::NoKey`] sans clé (rien n'est envoyé).
    pub fn send(
        &mut self,
        text: &str,
        config: RequestConfig,
        transport: &dyn Transport,
    ) -> Result<(), AssistantError> {
        Self::check(self.busy(), &config)?;
        let text = text.trim();
        if text.is_empty() {
            return Ok(());
        }
        let config = Self::fresh_request(config)?;
        self.push_turn(Turn::new(text, &config.model, config.asked_at.clone()));
        self.start_round(0, config, Usage::default(), false, transport);
        Ok(())
    }

    fn check(busy: bool, config: &RequestConfig) -> Result<(), AssistantError> {
        if busy {
            return Err(AssistantError::Busy);
        }
        if config.api_key.trim().is_empty() {
            return Err(if config.endpoint.is_relay() {
                AssistantError::NoSession
            } else {
                AssistantError::NoKey
            });
        }
        Ok(())
    }

    /// Relais : identifiant neuf de la demande (envoi, régénération,
    /// reprise) ; les tours d'outils qui suivent le gardent.
    fn fresh_request(mut config: RequestConfig) -> Result<RequestConfig, AssistantError> {
        if let Endpoint::Relay { request_id, .. } = &mut config.endpoint {
            *request_id = relay::new_request_id()?;
        }
        Ok(config)
    }

    /// Régénère la dernière réponse (ou réessaie après une erreur) : le
    /// dernier tour est retiré et sa question renvoyée.
    ///
    /// # Errors
    ///
    /// Comme [`Conversation::send`] ; rien à régénérer : `Ok` sans envoi.
    pub fn regenerate(
        &mut self,
        config: RequestConfig,
        transport: &dyn Transport,
    ) -> Result<(), AssistantError> {
        Self::check(self.busy(), &config)?;
        let Some(last) = self.turns.pop() else {
            return Ok(());
        };
        self.touch();
        self.send(&last.question, config, transport)
    }

    /// Reprend une réponse arrêtée : le texte partiel est gardé, la suite
    /// s'y ajoute. `prompt` (« continue là où tu t'es arrêté ») est envoyé
    /// au modèle sans être affiché.
    ///
    /// # Errors
    ///
    /// Comme [`Conversation::send`] ; dernier tour non arrêté : `Ok` sans
    /// envoi.
    pub fn continue_last(
        &mut self,
        prompt: &str,
        config: RequestConfig,
        transport: &dyn Transport,
    ) -> Result<(), AssistantError> {
        Self::check(self.busy(), &config)?;
        let Some(turn) = self.turns.last_mut() else {
            return Ok(());
        };
        if turn.state != TurnState::Stopped {
            return Ok(());
        }
        let config = Self::fresh_request(config)?;
        let Some(turn) = self.turns.last_mut() else {
            return Ok(());
        };
        turn.state = TurnState::Streaming;
        turn.wire.push(json!({ "role": "user", "content": prompt }));
        let usage = turn.usage.take().unwrap_or_default();
        self.touch();
        self.start_round(0, config, usage, false, transport);
        Ok(())
    }

    /// Arrête la demande en cours : la connexion est interrompue au prochain
    /// évènement, les évènements tardifs sont ignorés ; le texte partiel est
    /// gardé.
    pub fn stop(&mut self) {
        let Some(running) = self.running.take() else {
            return;
        };
        running.cancel.store(true, Ordering::Relaxed);
        let now = self.now;
        let usage = running.total_usage();
        if let Some(turn) = self.turns.last_mut() {
            let text = running.acc.text();
            if !text.is_empty() {
                turn.wire.push(json!({ "role": "assistant", "content": text }));
            }
            turn.reasoning_end(now);
            turn.state = TurnState::Stopped;
            turn.usage = (usage != Usage::default()).then_some(usage);
        }
        if let Ok(mut inbox) = self.inbox.lock() {
            inbox.clear();
        }
        self.touch();
    }

    /// Lit les évènements reçus ; exécute les outils demandés et lance le
    /// tour suivant. Rend vrai si la conversation a changé.
    pub fn poll(&mut self, host: &mut dyn ToolHost, transport: &dyn Transport) -> bool {
        let Some(request) = self.running.as_ref().map(|r| r.request) else {
            return false;
        };
        let events: Vec<HttpEvent> = match self.inbox.lock() {
            Ok(mut inbox) => {
                let all: Vec<(u64, HttpEvent)> = inbox.drain(..).collect();
                all.into_iter()
                    .filter(|(id, _)| *id == request)
                    .map(|(_, e)| e)
                    .collect()
            }
            Err(_) => vec![HttpEvent::Failed("file des évènements inutilisable".to_owned())],
        };
        if events.is_empty() {
            return false;
        }
        for event in events {
            if self.running.is_none() {
                break;
            }
            match event {
                HttpEvent::Head { status, headers } => {
                    if let Some(r) = self.running.as_mut() {
                        r.status = Some(status);
                        r.headers = headers;
                    }
                }
                HttpEvent::Chunk(bytes) => self.chunk(&bytes),
                HttpEvent::End => self.finish_round(host, transport),
                HttpEvent::Failed(detail) => self.fail(AssistantError::Network {
                    detail: crate::error::truncate_message(&detail),
                }),
            }
        }
        self.touch();
        true
    }

    /// Conversation à garder : tours terminés (un tour en cours est gardé
    /// comme arrêté) ; les tours anciens sont allégés comme pour la
    /// mémoire ([`memory::reduced`]). Aucun secret : ni clé ni contexte.
    #[must_use]
    pub fn saved(&self) -> SavedConversation {
        let intact_from = self
            .turns
            .len()
            .saturating_sub(memory::KEEP_INTACT.saturating_add(1));
        let turns = self
            .turns
            .iter()
            .enumerate()
            .map(|(i, turn)| {
                let mut turn = turn.clone();
                if turn.streaming() {
                    turn.state = TurnState::Stopped;
                    if !turn.answer.is_empty() {
                        turn.wire
                            .push(json!({ "role": "assistant", "content": turn.answer }));
                    }
                }
                if i < intact_from {
                    turn.wire = memory::reduced(&turn);
                }
                turn
            })
            .collect();
        SavedConversation {
            version: SAVED_VERSION,
            turns,
        }
    }

    /// Conversation relue (aucune demande en cours).
    #[must_use]
    pub fn restore(saved: SavedConversation) -> Self {
        let mut conversation = Self::new();
        let start = saved.turns.len().saturating_sub(MAX_TURNS);
        conversation.turns = saved.turns.into_iter().skip(start).collect();
        for turn in &mut conversation.turns {
            if turn.streaming() {
                turn.state = TurnState::Stopped;
            }
            // La file des propositions n'est pas gardée : une proposition
            // en attente à la fermeture est expirée (rien d'appliqué).
            for step in &mut turn.tools {
                if let Some(card) = &mut step.proposal
                    && card.status.is_pending()
                {
                    card.status = ProposalStatus::Expired;
                }
            }
        }
        conversation
    }

    /// Identifiants des propositions en attente dans la conversation.
    #[must_use]
    pub fn pending_proposals(&self) -> Vec<u64> {
        self.turns
            .iter()
            .flat_map(|t| &t.tools)
            .filter_map(|s| s.proposal.as_ref())
            .filter(|c| c.status.is_pending())
            .map(|c| c.id)
            .collect()
    }

    /// Issue de la proposition `id` (encore en attente seulement) : la
    /// carte change et la décision sera dite au modèle au prochain message
    /// (ou au prochain tour d'API de la demande en cours). Rend vrai si une
    /// proposition en attente portait cet identifiant.
    pub fn set_proposal_status(&mut self, id: u64, status: ProposalStatus) -> bool {
        let card = self
            .turns
            .iter_mut()
            .flat_map(|t| &mut t.tools)
            .filter_map(|s| s.proposal.as_mut())
            .find(|c| c.id == id && c.status.is_pending());
        let Some(card) = card else {
            return false;
        };
        card.status = status.clone();
        let label = card.label.clone();
        self.decisions.push((label, status));
        self.touch();
        true
    }

    /// Décisions pas encore dites au modèle (tests).
    #[must_use]
    pub fn untold_decisions(&self) -> usize {
        self.decisions.len()
    }

    fn push_turn(&mut self, turn: Turn) {
        self.turns.push(turn);
        if self.turns.len() > MAX_TURNS {
            let extra = self.turns.len().saturating_sub(MAX_TURNS);
            self.turns.drain(..extra);
        }
        self.touch();
    }

    fn chunk(&mut self, bytes: &[u8]) {
        let Some(r) = self.running.as_mut() else {
            return;
        };
        r.received = r.received.saturating_add(bytes.len());
        if r.received > MAX_RESPONSE_BYTES {
            self.fail(AssistantError::Protocol {
                detail: "réponse trop longue".to_owned(),
            });
            return;
        }
        if !r.status.is_some_and(|s| (200..300).contains(&s)) {
            let room = MAX_ERROR_BODY.saturating_sub(r.error_body.len());
            r.error_body.extend(bytes.iter().take(room));
            return;
        }
        let Ok(events) = r.sse.feed(bytes) else {
            self.fail(AssistantError::Protocol {
                detail: "ligne SSE trop longue".to_owned(),
            });
            return;
        };
        self.read_relay_cost();
        for event in events {
            if !self.data(&event.data) {
                return;
            }
        }
    }

    /// Coût et solde annoncés par le relais (`: plannercraft-cost`).
    fn read_relay_cost(&mut self) {
        let Some(r) = self.running.as_mut() else {
            return;
        };
        if let Some(cost) = r.sse.take_relay_cost() {
            r.usage.cost_cents = Some(r.usage.cost_cents.unwrap_or(0).saturating_add(cost.cents));
            if cost.balance.is_some() {
                self.balance = cost.balance;
            }
        }
    }

    /// Une charge `data` ; faux si la demande a échoué.
    fn data(&mut self, data: &str) -> bool {
        let now = self.now;
        let Some(r) = self.running.as_mut() else {
            return false;
        };
        match r.acc.push(data) {
            Ok(delta) if delta.is_empty() => {
                // Un appel d'outil qui commence clôt le raisonnement.
                if !r.acc.tool_calls().is_empty()
                    && let Some(turn) = self.turns.last_mut()
                {
                    turn.reasoning_end(now);
                }
                true
            }
            Ok(delta) => {
                let separate = r.separate && !r.text_started;
                if !delta.text.is_empty() {
                    r.text_started = true;
                }
                let Some(turn) = self.turns.last_mut() else {
                    return true;
                };
                if !delta.reasoning.is_empty() {
                    if !turn.reasoning.is_empty() && turn.reasoning_started.is_none() {
                        turn.reasoning.push_str("\n\n");
                    }
                    turn.reasoning_tick(now);
                    turn.reasoning.push_str(&delta.reasoning);
                }
                if !delta.text.is_empty() {
                    turn.reasoning_end(now);
                    if separate && !turn.answer.is_empty() {
                        turn.answer.push_str("\n\n");
                    }
                    turn.answer.push_str(&delta.text);
                }
                true
            }
            Err(error) => {
                self.fail(error);
                false
            }
        }
    }

    fn fail(&mut self, error: AssistantError) {
        let Some(running) = self.running.take() else {
            return;
        };
        running.cancel.store(true, Ordering::Relaxed);
        let now = self.now;
        let usage = running.total_usage();
        if let Some(turn) = self.turns.last_mut() {
            let text = running.acc.text();
            if !text.is_empty() {
                turn.wire.push(json!({ "role": "assistant", "content": text }));
            }
            turn.reasoning_end(now);
            if matches!(error, AssistantError::SessionEnded { .. }) {
                self.session_ended = true;
            }
            turn.state = TurnState::Failed(error);
            turn.usage = (usage != Usage::default()).then_some(usage);
        }
    }

    fn finish_round(&mut self, host: &mut dyn ToolHost, transport: &dyn Transport) {
        let Some(mut r) = self.running.take() else {
            return;
        };
        let status = r.status.unwrap_or(0);
        if !(200..300).contains(&status) {
            let error = if r.config.endpoint.is_relay() {
                relay::api_error(status, &r.headers, &r.error_body)
            } else {
                openrouter::http_error(status, &r.headers, &r.error_body)
            };
            self.running = Some(r);
            self.fail(error);
            return;
        }
        let tail = r.sse.finish();
        self.running = Some(r);
        self.read_relay_cost();
        match tail {
            Ok(Some(event)) => {
                if !self.data(&event.data) {
                    return;
                }
            }
            Ok(None) => {}
            Err(_) => {
                self.fail(AssistantError::Protocol {
                    detail: "ligne SSE trop longue".to_owned(),
                });
                return;
            }
        }
        let Some(r) = self.running.take() else {
            return;
        };
        if !r.acc.complete() {
            self.running = Some(r);
            self.fail(AssistantError::Network {
                detail: "réponse interrompue".to_owned(),
            });
            return;
        }
        let now = self.now;
        let usage = r.total_usage();
        let text = r.acc.text().to_owned();
        let calls = r.acc.tool_calls();
        let next = r.round.saturating_add(1);
        if !calls.is_empty() && next >= MAX_ROUNDS {
            self.running = Some(r);
            self.fail(AssistantError::TooManyRounds);
            return;
        }
        let Some(turn) = self.turns.last_mut() else {
            return;
        };
        turn.reasoning_end(now);
        if calls.is_empty() {
            turn.wire.push(json!({ "role": "assistant", "content": text }));
            turn.usage = Some(usage);
            turn.state = TurnState::Done;
            return;
        }
        Self::run_tools(
            turn,
            &calls,
            &text,
            r.acc.reasoning_details(),
            host,
            r.config.lang,
        );
        let started_at = r.started_at;
        self.start_round(next, r.config, usage, true, transport);
        if let Some(running) = self.running.as_mut() {
            running.started_at = started_at;
        }
    }

    fn run_tools(
        turn: &mut Turn,
        calls: &[ToolCall],
        text: &str,
        reasoning_details: &[Value],
        host: &mut dyn ToolHost,
        lang: Language,
    ) {
        let wire: Vec<Value> = calls
            .iter()
            .map(|c| {
                json!({
                    "id": c.id,
                    "type": "function",
                    "function": { "name": c.name, "arguments": c.arguments },
                })
            })
            .collect();
        let content = if text.is_empty() {
            Value::Null
        } else {
            Value::String(text.to_owned())
        };
        let mut message = json!({ "role": "assistant", "content": content, "tool_calls": wire });
        // Raisonnement renvoyé tel quel avec l'appel (continuité exigée par
        // certains fournisseurs, doc « reasoning tokens » d'OpenRouter).
        if !reasoning_details.is_empty()
            && let Value::Object(map) = &mut message
        {
            map.insert(
                "reasoning_details".to_owned(),
                Value::Array(reasoning_details.to_vec()),
            );
        }
        turn.wire.push(message);
        for call in calls {
            let (result, proposal) = if call.name == PROPOSE_TOOL {
                Self::propose(turn, call, host, lang)
            } else {
                (tools::execute(host, call, lang), None)
            };
            turn.tools.push(ToolStep {
                name: call.name.clone(),
                arguments: call.arguments.chars().take(MAX_SHOWN_ARGUMENTS).collect(),
                error: tool_error(&result),
                proposal,
            });
            turn.wire
                .push(json!({ "role": "tool", "tool_call_id": call.id, "content": result }));
        }
    }

    /// Appel de [`PROPOSE_TOOL`] : garde-fous, puis l'hôte (`dry_run`, mise
    /// en attente). Une seule proposition soumise par demande ; au plus
    /// [`proposal::MAX_CORRECTIONS`] corrections après un refus.
    fn propose(
        turn: &Turn,
        call: &ToolCall,
        host: &mut dyn ToolHost,
        lang: Language,
    ) -> (String, Option<ProposalCard>) {
        let mine = turn.tools.iter().filter(|s| s.name == PROPOSE_TOOL);
        let submitted = mine.clone().any(|s| s.proposal.is_some());
        let refused = mine.filter(|s| s.error.is_some()).count();
        if submitted || refused > proposal::MAX_CORRECTIONS {
            return (proposal::blocked_result(submitted, lang).to_string(), None);
        }
        let left = proposal::MAX_CORRECTIONS.saturating_sub(refused);
        let outcome = proposal::check(&call.arguments, lang, &turn.question)
            .and_then(|checked| host.propose(&checked.label, checked.params));
        match outcome {
            Ok(done) => {
                let card = ProposalCard {
                    id: done.id,
                    label: done.label,
                    changed_tasks: done.changed_tasks,
                    status: ProposalStatus::Pending,
                };
                (done.result.to_string(), Some(card))
            }
            Err(error) => (proposal::refused_result(&error, left, lang).to_string(), None),
        }
    }

    /// Décisions en attente, en une note dans la langue `lang` (vidées).
    fn take_notes(&mut self, lang: Language) -> Option<String> {
        if self.decisions.is_empty() {
            return None;
        }
        let notes: Vec<String> = self
            .decisions
            .drain(..)
            .map(|(label, status)| proposal::decision_note(&label, &status, lang))
            .collect();
        Some(notes.join("\n"))
    }

    fn start_round(
        &mut self,
        round: u32,
        config: RequestConfig,
        usage: Usage,
        separate: bool,
        transport: &dyn Transport,
    ) {
        // Décisions de l'utilisateur pas encore dites : jointes au message
        // envoyé (question ou consigne de reprise) au premier tour d'API,
        // ou en message à part entre deux tours d'outils.
        if let Some(note) = self.take_notes(config.lang)
            && let Some(turn) = self.turns.last_mut()
        {
            let last_user = turn
                .wire
                .iter_mut()
                .rev()
                .find(|m| m.get("role").and_then(Value::as_str) == Some("user"));
            match last_user {
                Some(Value::Object(message)) if round == 0 => {
                    let text = message
                        .get("content")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned();
                    message.insert("content".to_owned(), Value::String(format!("{note}\n\n{text}")));
                }
                _ => turn.wire.push(json!({ "role": "user", "content": note })),
            }
        }
        let (previous, current) = match self.turns.split_last() {
            Some((current, previous)) => (previous, current.wire.as_slice()),
            None => (&[][..], &[][..]),
        };
        let messages = memory::request_messages(
            plannercraft_automation::agent::assistant_instructions(config.lang),
            previous,
            current,
            config.context.as_deref(),
            memory::supports_cache_control(&config.model),
        );
        let allow_tools = round.saturating_add(1) < MAX_ROUNDS;
        let mut body = openrouter::chat_body(
            &config.model,
            &messages,
            &tools::definitions(config.lang),
            config.max_tokens,
            allow_tools,
        );
        openrouter::with_reasoning(&mut body, config.reasoning_budget);
        let request = match &config.endpoint {
            Endpoint::OpenRouter => openrouter::chat_request(&config.api_key, &body),
            Endpoint::Relay { base_url, request_id } => relay::chat_request(
                base_url,
                &config.api_key,
                request_id,
                round.saturating_add(1),
                &body,
            ),
        };
        let id = self.next_request;
        self.next_request = self.next_request.saturating_add(1);
        let cancel = Arc::new(AtomicBool::new(false));
        self.running = Some(Running {
            request: id,
            round,
            config,
            cancel: Arc::clone(&cancel),
            usage,
            status: None,
            headers: Vec::new(),
            error_body: Vec::new(),
            received: 0,
            sse: SseParser::new(),
            acc: StreamAccumulator::new(),
            text_started: false,
            separate,
            started_at: self.now,
        });
        let inbox = Arc::clone(&self.inbox);
        let waker = self.waker.clone();
        let sink = Box::new(move |event: HttpEvent| {
            if cancel.load(Ordering::Relaxed) {
                return ControlFlow::Break(());
            }
            let flow = match inbox.lock() {
                Ok(mut queue) if queue.len() < MAX_QUEUED_EVENTS => {
                    queue.push_back((id, event));
                    ControlFlow::Continue(())
                }
                Ok(mut queue) => {
                    queue.push_back((id, HttpEvent::Failed("file des évènements pleine".to_owned())));
                    ControlFlow::Break(())
                }
                Err(_) => ControlFlow::Break(()),
            };
            if let Some(waker) = &waker {
                waker();
            }
            flow
        });
        self.requests_sent = self.requests_sent.saturating_add(1);
        self.touch();
        transport.send(request, sink);
    }
}

/// Erreur structurée d'un résultat d'outil (`{"error": {code, message}}`),
/// en texte court.
fn tool_error(result: &str) -> Option<String> {
    let value: Value = serde_json::from_str(result).ok()?;
    let error = value.get("error")?;
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| error.get("code").and_then(Value::as_str))
        .unwrap_or("erreur");
    Some(crate::error::truncate_message(message))
}
