//! Assistant IA de PlannerCraft (ADR-0024, lot A1 : mode « ma propre
//! clé », lecture seule).
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
//! - [`conversation`] : boucle d'outils bornée, menée par l'interface ;
//! - [`pkce`], [`login`], `web` : connexion OAuth PKCE d'OpenRouter ;
//! - [`secrets`] : clé dans le trousseau du système, jamais dans `UiState`.
//!
//! Documentation : `docs/features/assistant.md`.

#![forbid(unsafe_code)]

pub mod conversation;
pub mod error;
pub mod http;
pub mod login;
pub mod openrouter;
pub mod pkce;
pub mod platform;
pub mod secrets;
pub mod sse;
pub mod stream;
pub mod testing;
pub mod tools;
#[cfg(target_arch = "wasm32")]
pub mod web;

pub use conversation::{Activity, Conversation, Entry, RequestConfig, Waker};
pub use error::AssistantError;
pub use http::{EhttpTransport, HttpEvent, HttpRequest, SharedTransport, Transport};
pub use stream::Usage;
