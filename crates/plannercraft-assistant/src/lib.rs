//! Assistant IA de PlannerCraft (ADR-0024, mode « ma propre clé » ; lot
//! A1 : lectures ; lot A2 : propositions soumises à validation).
//!
//! Sous `plannercraft-ui-egui` dans les couches (règle 9) : ni egui ni
//! eframe ici. Le même code sert le desktop et le web :
//! - [`http`] : transport injectable (réel : `ehttp`, ureq dans un fil sur
//!   le desktop, `fetch` en wasm ; tests : [`testing::ScriptedTransport`]) ;
//! - [`sse`] et [`stream`] : lecture tolérante du flux Chat Completions ;
//! - [`openrouter`] : requête, en-têtes d'attribution, confidentialité
//!   imposée, erreurs HTTP, liste fermée de modèles ;
//! - [`tools`] : outils en lecture pris dans le registre, exécutés sur une
//!   copie de la session (règle 3 : les dates viennent du moteur) ;
//! - [`proposal`] : l'outil d'écriture « `propose_changes` » (lot A2) : lot
//!   contrôlé (aucune date), soumis à la validation de l'utilisateur ;
//! - [`conversation`] : tours (question, raisonnement, lectures, réponse),
//!   boucle d'outils bornée, menée par l'interface ;
//! - [`memory`] : historique renvoyé, allégé, points de cache (lot A1b) ;
//! - [`archive`] : conversations gardées par document, hors du fichier
//!   projet et de `UiState` (lot A1b) ;
//! - [`pkce`], [`login`], `web` : connexion OAuth PKCE d'OpenRouter ;
//! - [`secrets`] : clé dans le trousseau du système, jamais dans `UiState`.
//!
//! Documentation : `docs/features/assistant.md`.

#![forbid(unsafe_code)]

pub mod archive;
pub mod conversation;
pub mod error;
pub mod http;
pub mod login;
pub mod memory;
pub mod openrouter;
pub mod pkce;
pub mod platform;
pub mod proposal;
pub mod secrets;
pub mod sse;
pub mod stream;
pub mod testing;
pub mod tools;
#[cfg(target_arch = "wasm32")]
pub mod web;

pub use conversation::{
    Activity, Conversation, RequestConfig, SavedConversation, ToolStep, Turn, TurnState, Waker,
};
pub use error::AssistantError;
pub use http::{EhttpTransport, HttpEvent, HttpRequest, SharedTransport, Transport};
pub use stream::Usage;
