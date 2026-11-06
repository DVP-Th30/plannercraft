//! Garde de la clé OpenRouter, **hors** de l'état d'interface (`UiState`
//! est envoyé aux agents MCP rattachés) et des journaux.
//!
//! - Desktop : trousseau du système par `keyring-core` (Secret Service
//!   par zbus sous Linux, Trousseau de macOS, Gestionnaire
//!   d'identification de Windows). Sans trousseau (Secret Service absent,
//!   session sans D-Bus), [`open_default`] rend un magasin en mémoire et
//!   l'interface avertit : la clé vaut pour la session seulement.
//! - Web : mémoire seulement (aucun stockage sûr dans un navigateur).

use std::sync::{Arc, Mutex};

use crate::error::AssistantError;

/// Service et compte de l'entrée du trousseau.
pub const KEYRING_SERVICE: &str = "PlannerCraft";
/// Compte de l'entrée du trousseau.
pub const KEYRING_USER: &str = "openrouter-api-key";

/// Magasin de la clé.
pub trait SecretStore: Send + Sync {
    /// Clé enregistrée, s'il y en a une.
    ///
    /// # Errors
    ///
    /// [`AssistantError::Keyring`] si le magasin est inutilisable.
    fn load(&self) -> Result<Option<String>, AssistantError>;
    /// Enregistre la clé.
    ///
    /// # Errors
    ///
    /// [`AssistantError::Keyring`] si le magasin refuse.
    fn save(&self, key: &str) -> Result<(), AssistantError>;
    /// Efface la clé (sans erreur si elle n'existe pas).
    ///
    /// # Errors
    ///
    /// [`AssistantError::Keyring`] si le magasin refuse.
    fn delete(&self) -> Result<(), AssistantError>;
    /// Vrai si la clé survit à la fermeture de l'application.
    fn persistent(&self) -> bool;
}

/// Magasin partagé.
pub type SharedSecrets = Arc<dyn SecretStore>;

/// Magasin en mémoire (web, tests, repli sans trousseau).
#[derive(Default)]
pub struct MemoryStore(Mutex<Option<String>>);

impl std::fmt::Debug for MemoryStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MemoryStore(<masquée>)")
    }
}

impl SecretStore for MemoryStore {
    fn load(&self) -> Result<Option<String>, AssistantError> {
        self.0.lock().map(|k| k.clone()).map_err(|_| poisoned())
    }

    fn save(&self, key: &str) -> Result<(), AssistantError> {
        *self.0.lock().map_err(|_| poisoned())? = Some(key.to_owned());
        Ok(())
    }

    fn delete(&self) -> Result<(), AssistantError> {
        *self.0.lock().map_err(|_| poisoned())? = None;
        Ok(())
    }

    fn persistent(&self) -> bool {
        false
    }
}

fn poisoned() -> AssistantError {
    AssistantError::Keyring {
        detail: "magasin en mémoire inutilisable".to_owned(),
    }
}

/// Magasin du système, ou en mémoire s'il manque (avec la raison).
#[must_use]
pub fn open_default() -> (SharedSecrets, Option<AssistantError>) {
    #[cfg(not(target_arch = "wasm32"))]
    match keyring::KeyringStore::open() {
        Ok(store) => (Arc::new(store), None),
        Err(error) => {
            log::warn!("trousseau du système indisponible, clé en mémoire : {error}");
            (Arc::new(MemoryStore::default()), Some(error))
        }
    }
    #[cfg(target_arch = "wasm32")]
    (Arc::new(MemoryStore::default()), None)
}

#[cfg(not(target_arch = "wasm32"))]
mod keyring {
    use std::sync::Arc;

    use keyring_core::{CredentialStore, Entry, Error};

    use super::{KEYRING_SERVICE, KEYRING_USER, SecretStore};
    use crate::error::AssistantError;

    /// Entrée du trousseau du système.
    pub struct KeyringStore {
        entry: Entry,
    }

    fn fail(error: &Error) -> AssistantError {
        AssistantError::Keyring {
            detail: crate::error::truncate_message(&error.to_string()),
        }
    }

    /// Magasin du système d'exploitation courant.
    fn platform_store() -> Result<Arc<CredentialStore>, Error> {
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        {
            let store: Arc<CredentialStore> = zbus_secret_service_keyring_store::Store::new()?;
            Ok(store)
        }
        #[cfg(target_os = "macos")]
        {
            let store: Arc<CredentialStore> = apple_native_keyring_store::keychain::Store::new()?;
            Ok(store)
        }
        #[cfg(target_os = "windows")]
        {
            let store: Arc<CredentialStore> = windows_native_keyring_store::Store::new()?;
            Ok(store)
        }
        #[cfg(not(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "macos",
            target_os = "windows"
        )))]
        {
            Err(Error::NoDefaultStore)
        }
    }

    impl KeyringStore {
        /// Ouvre l'entrée de PlannerCraft dans le trousseau, et vérifie
        /// qu'il répond (lecture).
        pub fn open() -> Result<Self, AssistantError> {
            let store = platform_store().map_err(|e| fail(&e))?;
            let entry = store
                .build(KEYRING_SERVICE, KEYRING_USER, None)
                .map_err(|e| fail(&e))?;
            let this = Self { entry };
            this.load()?;
            Ok(this)
        }
    }

    impl SecretStore for KeyringStore {
        fn load(&self) -> Result<Option<String>, AssistantError> {
            match self.entry.get_password() {
                Ok(key) => Ok(Some(key)),
                Err(Error::NoEntry) => Ok(None),
                Err(e) => Err(fail(&e)),
            }
        }

        fn save(&self, key: &str) -> Result<(), AssistantError> {
            self.entry.set_password(key).map_err(|e| fail(&e))
        }

        fn delete(&self) -> Result<(), AssistantError> {
            match self.entry.delete_credential() {
                Ok(()) | Err(Error::NoEntry) => Ok(()),
                Err(e) => Err(fail(&e)),
            }
        }

        fn persistent(&self) -> bool {
            true
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : un échec doit s'arrêter net"
    )]

    use super::*;

    #[test]
    fn memory_store_round_trip() {
        let store = MemoryStore::default();
        assert_eq!(store.load().unwrap(), None);
        store.save("sk-or-v1-x").unwrap();
        assert_eq!(store.load().unwrap().as_deref(), Some("sk-or-v1-x"));
        assert!(!format!("{store:?}").contains("sk-or"));
        store.delete().unwrap();
        assert_eq!(store.load().unwrap(), None);
        assert!(!store.persistent());
    }
}
