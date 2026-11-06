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

/// Résultat d'une tâche courte lancée hors du fil de l'interface (lecture
/// ou écriture du trousseau, qui peut attendre D-Bus ou un déverrouillage).
/// Web : exécutée tout de suite (magasin en mémoire, rien n'attend).
pub struct Background<R>(std::sync::Arc<std::sync::Mutex<Option<R>>>);

impl<R> std::fmt::Debug for Background<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Background")
    }
}

impl<R: Send + 'static> Background<R> {
    /// Lance `task` ; `waker` est appelé quand le résultat est prêt.
    #[must_use]
    pub fn spawn(
        task: impl FnOnce() -> R + Send + 'static,
        waker: Option<crate::conversation::Waker>,
    ) -> Self {
        let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
        let done = std::sync::Arc::clone(&slot);
        let run = move || {
            let result = task();
            if let Ok(mut s) = done.lock() {
                *s = Some(result);
            }
            if let Some(w) = &waker {
                w();
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            // L'échec de création du fil est rare ; la tâche est alors
            // abandonnée et le résultat ne vient jamais : on la refait ici.
            let shared = std::sync::Arc::new(std::sync::Mutex::new(Some(run)));
            let in_thread = std::sync::Arc::clone(&shared);
            let spawned = std::thread::Builder::new()
                .name("plannercraft-assistant-task".to_owned())
                .spawn(move || {
                    if let Some(run) = in_thread.lock().ok().and_then(|mut r| r.take()) {
                        run();
                    }
                });
            if spawned.is_err()
                && let Some(run) = shared.lock().ok().and_then(|mut r| r.take())
            {
                run();
            }
        }
        #[cfg(target_arch = "wasm32")]
        run();
        Self(slot)
    }

    /// Résultat, une fois.
    #[must_use]
    pub fn take(&self) -> Option<R> {
        self.0.lock().ok().and_then(|mut s| s.take())
    }
}
