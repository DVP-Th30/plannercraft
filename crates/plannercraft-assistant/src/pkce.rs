//! OAuth PKCE d'OpenRouter (doc relue le 2026-11-06 :
//! `https://openrouter.ai/docs/use-cases/oauth-pkce`).
//!
//! 1. L'application tire un vérifieur (32 octets d'aléa, base64url sans
//!    remplissage : 43 caractères, RFC 7636) et son défi
//!    `BASE64URL(SHA-256(vérifieur))`, plus un `state` (CSRF).
//! 2. Le navigateur ouvre `https://openrouter.ai/auth?callback_url=…&
//!    code_challenge=…&code_challenge_method=S256&state=…`.
//! 3. OpenRouter renvoie vers `callback_url?code=…&state=…` (desktop :
//!    écouteur `localhost`, tout port permis ; web : la page de l'app).
//! 4. `POST /api/v1/auth/keys` avec `code`, `code_verifier`,
//!    `code_challenge_method` rend `{"key": …}` ([`crate::openrouter`]).

use std::fmt::Write as _;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest as _, Sha256};

use crate::error::AssistantError;

/// Page d'autorisation d'OpenRouter.
pub const AUTH_URL: &str = "https://openrouter.ai/auth";

/// Paire PKCE et état CSRF d'une connexion.
#[derive(Clone, PartialEq, Eq)]
pub struct Pkce {
    /// Vérifieur (gardé par l'application jusqu'à l'échange).
    pub verifier: String,
    /// Défi S256 (envoyé dans l'adresse d'autorisation).
    pub challenge: String,
    /// État CSRF rendu tel quel par OpenRouter.
    pub state: String,
}

impl std::fmt::Debug for Pkce {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pkce")
            .field("challenge", &self.challenge)
            .finish_non_exhaustive()
    }
}

impl Pkce {
    /// Paire tirée de l'aléa du système.
    ///
    /// # Errors
    ///
    /// [`AssistantError::Login`] si l'aléa du système est indisponible.
    pub fn generate() -> Result<Self, AssistantError> {
        let mut verifier = [0u8; 32];
        let mut state = [0u8; 16];
        crate::platform::random(&mut verifier)?;
        crate::platform::random(&mut state)?;
        Ok(Self::from_bytes(&verifier, &state))
    }

    /// Paire déterministe (tests).
    #[must_use]
    pub fn from_bytes(verifier: &[u8], state: &[u8]) -> Self {
        let verifier = URL_SAFE_NO_PAD.encode(verifier);
        Self {
            challenge: challenge_of(&verifier),
            verifier,
            state: URL_SAFE_NO_PAD.encode(state),
        }
    }

    /// Adresse d'autorisation pour `callback_url`.
    #[must_use]
    pub fn auth_url(&self, callback_url: &str) -> String {
        format!(
            "{AUTH_URL}?callback_url={}&code_challenge={}&code_challenge_method=S256&state={}",
            percent_encode(callback_url),
            percent_encode(&self.challenge),
            percent_encode(&self.state),
        )
    }
}

/// Défi S256 d'un vérifieur.
#[must_use]
pub fn challenge_of(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Encodage « pour cent » d'un composant d'adresse (RFC 3986, caractères
/// non réservés gardés).
#[must_use]
pub fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// Décodage « pour cent » (et `+` en espace) d'un composant de requête ;
/// une séquence invalide est gardée telle quelle.
#[must_use]
pub fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while let Some(&b) = bytes.get(i) {
        let hex = |j: usize| bytes.get(j).and_then(|c| char::from(*c).to_digit(16));
        match b {
            b'%' => match (hex(i.saturating_add(1)), hex(i.saturating_add(2))) {
                (Some(h), Some(l)) => {
                    out.push(u8::try_from(h.saturating_mul(16).saturating_add(l)).unwrap_or(b'?'));
                    i = i.saturating_add(3);
                    continue;
                }
                _ => out.push(b),
            },
            b'+' => out.push(b' '),
            _ => out.push(b),
        }
        i = i.saturating_add(1);
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Paramètre `name` d'une chaîne de requête (`a=1&b=2`, sans `?`).
#[must_use]
pub fn query_param(query: &str, name: &str) -> Option<String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('=').or(Some((pair, ""))))
        .find(|(k, _)| percent_decode(k) == name)
        .map(|(_, v)| percent_decode(v))
}

/// Code de la redirection de retour, `state` vérifié.
///
/// # Errors
///
/// [`AssistantError::Login`] : refus de l'utilisateur (`error`), état
/// différent (CSRF), code absent.
pub fn callback_code(query: &str, expected_state: &str) -> Result<String, AssistantError> {
    let login = |detail: &str| AssistantError::Login {
        detail: detail.to_owned(),
    };
    if let Some(error) = query_param(query, "error") {
        return Err(login(&crate::error::truncate_message(&error)));
    }
    if query_param(query, "state").as_deref() != Some(expected_state) {
        return Err(login("état de connexion différent (state)"));
    }
    query_param(query, "code")
        .filter(|c| !c.is_empty())
        .ok_or_else(|| login("code absent"))
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : un échec doit s'arrêter net"
    )]

    use super::*;

    /// Vecteur de l'annexe B de la RFC 7636.
    #[test]
    fn rfc7636_appendix_b() {
        assert_eq!(
            challenge_of("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn verifier_shape_and_generated_pairs_differ() {
        let pkce = Pkce::from_bytes(&[7u8; 32], &[1u8; 16]);
        assert_eq!(pkce.verifier.len(), 43);
        assert!(
            pkce.verifier
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        );
        assert_eq!(pkce.challenge, challenge_of(&pkce.verifier));
        let a = Pkce::generate().unwrap();
        let b = Pkce::generate().unwrap();
        assert_ne!(a.verifier, b.verifier);
        assert_ne!(a.state, b.state);
        assert!(!format!("{a:?}").contains(&a.verifier));
    }

    #[test]
    fn auth_url_encoded() {
        let pkce = Pkce::from_bytes(&[0u8; 32], &[0u8; 16]);
        let url = pkce.auth_url("http://localhost:51423/callback");
        assert!(url.starts_with(
            "https://openrouter.ai/auth?callback_url=http%3A%2F%2Flocalhost%3A51423%2Fcallback&code_challenge="
        ));
        assert!(url.contains("&code_challenge_method=S256&state="));
    }

    #[test]
    fn decode_and_query() {
        assert_eq!(percent_decode("a%20b+c%zz"), "a b c%zz");
        assert_eq!(query_param("code=x%2Dy&state=s", "code").as_deref(), Some("x-y"));
        assert_eq!(query_param("flag&code=1", "flag").as_deref(), Some(""));
        assert_eq!(query_param("code=1", "state"), None);
    }

    #[test]
    fn callback_checks_state_and_error() {
        assert_eq!(callback_code("code=abc&state=s1", "s1").unwrap(), "abc");
        assert!(callback_code("code=abc&state=other", "s1").is_err());
        assert!(callback_code("state=s1", "s1").is_err());
        assert!(callback_code("error=access_denied&state=s1", "s1").is_err());
    }
}
