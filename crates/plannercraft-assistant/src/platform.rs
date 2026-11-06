//! Services du système : aléa, navigateur.

use crate::error::AssistantError;

/// Remplit `buf` d'aléa cryptographique.
///
/// # Errors
///
/// [`AssistantError::Login`] si l'aléa du système est indisponible.
#[cfg(not(target_arch = "wasm32"))]
pub fn random(buf: &mut [u8]) -> Result<(), AssistantError> {
    getrandom::fill(buf).map_err(|e| AssistantError::Login {
        detail: format!("aléa du système indisponible : {e}"),
    })
}

/// Remplit `buf` d'aléa cryptographique (`crypto.getRandomValues`).
///
/// # Errors
///
/// [`AssistantError::Login`] si le navigateur ne fournit pas d'aléa.
#[cfg(target_arch = "wasm32")]
pub fn random(buf: &mut [u8]) -> Result<(), AssistantError> {
    let failed = || AssistantError::Login {
        detail: "crypto.getRandomValues indisponible".to_owned(),
    };
    let crypto = web_sys::window()
        .ok_or_else(failed)?
        .crypto()
        .map_err(|_| failed())?;
    crypto
        .get_random_values_with_u8_array(buf)
        .map_err(|_| failed())?;
    Ok(())
}

/// Ouvre `url` dans le navigateur (desktop) ; injectable pour les tests.
pub trait BrowserOpener: Send + Sync {
    /// Ouvre `url`.
    ///
    /// # Errors
    ///
    /// Détail du système si aucun navigateur ne s'ouvre.
    fn open(&self, url: &str) -> Result<(), String>;
}

/// Navigateur par défaut du système (crate `webbrowser`).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemBrowser;

#[cfg(not(target_arch = "wasm32"))]
impl BrowserOpener for SystemBrowser {
    fn open(&self, url: &str) -> Result<(), String> {
        webbrowser::open(url).map_err(|e| e.to_string())
    }
}
