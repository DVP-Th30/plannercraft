//! Erreurs de l'assistant, classées pour un message en langage
//! d'utilisateur (l'interface traduit chaque genre, `assistant-error-*`).
//!
//! Aucune erreur ne porte la clé : les détails viennent du serveur
//! (message d'OpenRouter) ou du système (réseau, trousseau).

use std::fmt;

use serde::{Deserialize, Serialize};

/// Erreur affichable de l'assistant (sérialisée avec la conversation
/// gardée par document : aucune ne porte de secret).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum AssistantError {
    /// Aucune clé : l'utilisateur doit se connecter ou en saisir une.
    NoKey,
    /// Clé refusée par OpenRouter (HTTP 401).
    Unauthorized,
    /// Crédits épuisés (HTTP 402).
    PaymentRequired,
    /// Limite de débit (HTTP 429), avec le délai d'attente annoncé
    /// (`Retry-After`, en secondes) s'il est lisible.
    RateLimited {
        /// Secondes à attendre.
        retry_after: Option<u64>,
    },
    /// Autre statut HTTP, avec le message du serveur.
    Http {
        /// Statut.
        status: u16,
        /// Message (`error.message` du corps), tronqué.
        message: String,
    },
    /// Erreur signalée dans le flux par le fournisseur du modèle.
    Provider {
        /// Message, tronqué.
        message: String,
    },
    /// Réseau : connexion impossible, coupure, délai dépassé.
    Network {
        /// Détail technique.
        detail: String,
    },
    /// Réponse illisible (JSON invalide, flux sans fin, taille dépassée).
    Protocol {
        /// Détail technique.
        detail: String,
    },
    /// Trop de tours d'outils pour une demande (borne de la boucle).
    TooManyRounds,
    /// Une demande est déjà en cours.
    Busy,
    /// Connexion OpenRouter (OAuth) refusée, expirée ou abandonnée.
    Login {
        /// Détail (paramètre `error`, délai, état CSRF).
        detail: String,
    },
    /// Trousseau du système indisponible ou en échec.
    Keyring {
        /// Détail du système.
        detail: String,
    },
    /// Mode « Compte PlannerCraft » sans session : l'utilisateur doit se
    /// connecter à son compte (lot A4).
    NoSession,
    /// Session du compte refusée par le relais (`auth.expired`,
    /// `auth.invalid`, `auth.required`) : retour à l'écran de connexion.
    SessionEnded {
        /// Vrai pour une session expirée (`auth.expired`), faux pour une
        /// session inconnue, révoquée ou un compte bloqué.
        expired: bool,
    },
    /// Crédits du compte PlannerCraft épuisés (`credits.insufficient`).
    CreditsExhausted,
    /// Autre refus du relais (`{ code, message }`) : `model.not_allowed`,
    /// `upstream.error`, `upstream.unavailable`, `payment.unavailable`,
    /// `request.invalid`, `mail.unavailable`, `internal`…
    Relay {
        /// Code stable du relais.
        relay_code: String,
        /// Message du relais (français), tronqué.
        message: String,
    },
}

/// Longueur au plus d'un message de serveur gardé (caractères).
pub const MAX_MESSAGE_CHARS: usize = 300;

/// `text` tronqué à [`MAX_MESSAGE_CHARS`] caractères.
#[must_use]
pub fn truncate_message(text: &str) -> String {
    let mut out: String = text.chars().take(MAX_MESSAGE_CHARS).collect();
    if text.chars().nth(MAX_MESSAGE_CHARS).is_some() {
        out.push('…');
    }
    out
}

impl AssistantError {
    /// Code stable du genre (clé de traduction `assistant-error-<code>`).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NoKey => "no-key",
            Self::Unauthorized => "unauthorized",
            Self::PaymentRequired => "payment-required",
            Self::RateLimited { .. } => "rate-limited",
            Self::Http { .. } => "http",
            Self::Provider { .. } => "provider",
            Self::Network { .. } => "network",
            Self::Protocol { .. } => "protocol",
            Self::TooManyRounds => "too-many-rounds",
            Self::Busy => "busy",
            Self::Login { .. } => "login",
            Self::Keyring { .. } => "keyring",
            Self::NoSession => "no-session",
            Self::SessionEnded { .. } => "session-ended",
            Self::CreditsExhausted => "credits-exhausted",
            Self::Relay { .. } => "relay",
        }
    }

    /// Détail technique éventuel (affiché après le message traduit).
    #[must_use]
    pub fn detail(&self) -> Option<String> {
        match self {
            Self::RateLimited { retry_after } => retry_after.map(|s| s.to_string()),
            Self::Http { status, message } => Some(format!("{status} {message}")),
            Self::Provider { message } => Some(message.clone()),
            Self::Network { detail }
            | Self::Protocol { detail }
            | Self::Login { detail }
            | Self::Keyring { detail } => Some(detail.clone()),
            Self::Relay { relay_code, message } => Some(format!("{relay_code} {message}")),
            Self::NoKey
            | Self::Unauthorized
            | Self::PaymentRequired
            | Self::TooManyRounds
            | Self::Busy
            | Self::NoSession
            | Self::SessionEnded { .. }
            | Self::CreditsExhausted => None,
        }
    }
}

impl fmt::Display for AssistantError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.detail() {
            Some(detail) => write!(f, "{} : {detail}", self.code()),
            None => f.write_str(self.code()),
        }
    }
}

impl std::error::Error for AssistantError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_keeps_short_and_marks_long() {
        assert_eq!(truncate_message("court"), "court");
        let long = "é".repeat(MAX_MESSAGE_CHARS + 5);
        let cut = truncate_message(&long);
        assert_eq!(cut.chars().count(), MAX_MESSAGE_CHARS + 1);
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn codes_are_stable() {
        assert_eq!(
            AssistantError::RateLimited { retry_after: Some(3) }.to_string(),
            "rate-limited : 3"
        );
        assert_eq!(AssistantError::Unauthorized.to_string(), "unauthorized");
    }
}
