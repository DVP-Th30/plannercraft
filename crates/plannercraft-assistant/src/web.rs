//! Connexion OAuth du web : redirection de la page vers OpenRouter, retour
//! sur la même page avec `?code=…&state=…`.
//!
//! Le vérifieur PKCE et l'état doivent survivre au rechargement de la
//! page : ils sont gardés dans `sessionStorage` (propre à l'onglet) le
//! temps de l'aller-retour, puis effacés à la lecture du retour. Ce ne sont
//! pas des secrets durables (le code expire en 10 minutes, il ne sert
//! qu'une fois et seulement avec ce vérifieur). La clé obtenue reste en
//! mémoire. L'adresse est nettoyée (`history.replaceState`) pour que le
//! code ne reste ni dans la barre d'adresse ni dans l'historique.

use web_sys::wasm_bindgen::JsValue;

use crate::error::AssistantError;
use crate::pkce::{Pkce, callback_code, query_param};

const VERIFIER_KEY: &str = "plannercraft.oauth.verifier";
const STATE_KEY: &str = "plannercraft.oauth.state";

fn fail(detail: &str) -> AssistantError {
    AssistantError::Login {
        detail: detail.to_owned(),
    }
}

/// Adresse de la page sans requête ni fragment (retour d'OpenRouter).
fn page_url(location: &web_sys::Location) -> Option<String> {
    let origin = location.origin().ok()?;
    let path = location.pathname().ok()?;
    Some(format!("{origin}{path}"))
}

/// Part vers la page d'autorisation d'OpenRouter (la page de l'app est
/// quittée : un document non enregistré est protégé par la garde
/// `beforeunload` de l'application).
///
/// # Errors
///
/// [`AssistantError::Login`] si le navigateur refuse (stockage, aléa).
pub fn begin_login() -> Result<(), AssistantError> {
    let window = web_sys::window().ok_or_else(|| fail("pas de fenêtre"))?;
    let location = window.location();
    let callback = page_url(&location).ok_or_else(|| fail("adresse de la page illisible"))?;
    let pkce = Pkce::generate()?;
    let storage = window
        .session_storage()
        .ok()
        .flatten()
        .ok_or_else(|| fail("sessionStorage indisponible"))?;
    storage
        .set_item(VERIFIER_KEY, &pkce.verifier)
        .and_then(|()| storage.set_item(STATE_KEY, &pkce.state))
        .map_err(|_| fail("sessionStorage refusé"))?;
    location
        .assign(&pkce.auth_url(&callback))
        .map_err(|_| fail("redirection refusée"))
}

/// Retour d'OpenRouter dans l'adresse de la page, s'il y en a un : le code
/// et le vérifieur à échanger. L'adresse est nettoyée et le stockage vidé
/// dans tous les cas.
#[must_use]
pub fn take_callback() -> Option<Result<(String, String), AssistantError>> {
    let window = web_sys::window()?;
    let location = window.location();
    let search = location.search().ok()?;
    let query = search.strip_prefix('?').unwrap_or(&search);
    if query_param(query, "code").is_none() && query_param(query, "error").is_none() {
        return None;
    }
    let storage = window.session_storage().ok().flatten();
    let stored = |key: &str| storage.as_ref().and_then(|s| s.get_item(key).ok().flatten());
    let verifier = stored(VERIFIER_KEY);
    let state = stored(STATE_KEY);
    if let Some(storage) = &storage {
        let _ = storage.remove_item(VERIFIER_KEY);
        let _ = storage.remove_item(STATE_KEY);
    }
    if let (Ok(history), Some(clean)) = (window.history(), page_url(&location)) {
        let _ = history.replace_state_with_url(&JsValue::NULL, "", Some(&clean));
    }
    let (Some(verifier), Some(state)) = (verifier, state) else {
        return Some(Err(fail("connexion commencée dans un autre onglet")));
    };
    Some(callback_code(query, &state).map(|code| (code, verifier)))
}
