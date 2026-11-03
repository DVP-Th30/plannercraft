//! Canal de contrôle de l'application (ADR-0002) : protocole, rendez-vous,
//! écouteur, relais des appels d'un agent (hors wasm ; lot L10 de J4,
//! relais J7 aux lots L2 et L3).
//!
//! Déroulé (détail dans `docs/features/control.md`) :
//! 1. l'application crée un jeton ([`new_token`]), lance l'écouteur du
//!    relais ([`spawn_listener_in`], file bornée vidée par l'interface)
//!    puis publie son rendez-vous ([`publish`], fichier 0600 dans un
//!    dossier 0700) ;
//! 2. `plannercraft mcp`, avant d'ouvrir un fichier, appelle [`find_live_for`] :
//!    un rendez-vous vivant qui tient ce fichier ⇒ rattachement
//!    ([`RelayClient`]) ou, en `--headless`, `file.open_in_app` ;
//! 3. chaque appel relayé est exécuté par l'application sur la session de
//!    son document (`workspace::call_with`), toute écriture après la
//!    validation de l'utilisateur ;
//! 4. à la fermeture, le garde et la poignée suppriment fichier et socket.
//!
//! Transport (J8, lot L8, ADR-0016 D12) : socket Unix sous Unix, TCP en
//! boucle locale sous Windows ([`tcp`], [`Endpoint`], [`Transport`]) ; même
//! protocole, même jeton par document, client commun.

mod client;
mod endpoint;
mod frame;
mod listener;
mod relay;
mod rendezvous;
pub mod tcp;

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

pub use client::{RELAY_MARGIN, RelayClient, RelayError};
pub use endpoint::{ControlStream, Endpoint, Transport};
pub use frame::{
    Frame, LineError, MAX_LINE_BYTES, Method, PROTOCOL_VERSION, Reply, WireError, codes, read_line_bounded,
    read_line_bounded_with, to_line,
};
pub use listener::{
    APPROVAL_TIMEOUT, ListenerHandle, ListenerOptions, RELAY_QUEUE, spawn_hello_listener,
    spawn_hello_listener_in, spawn_hello_listener_on, spawn_listener_in, spawn_listener_on,
    spawn_listener_with,
};
pub use relay::{CallState, RelayCall, RelayKind, Waker};
pub use rendezvous::{
    Rendezvous, RendezvousGuard, STATE_DIR_ENV, adopt_legacy_state_dir, adopt_legacy_state_dir_in,
    canonical_project, find_live_for, find_live_for_in, probe, publish, publish_in, state_dir,
};

/// Erreur du canal.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ControlError {
    /// Pas de dossier d'état sur cette plateforme.
    #[error("dossier d'état introuvable")]
    NoStateDir,
    /// Entrée/sortie.
    #[error("canal de contrôle : {0}")]
    Io(String),
    /// Non pris en charge (plateforme, ou squelette L0).
    #[error("canal de contrôle non pris en charge")]
    Unsupported,
    /// Jeton qui n'est pas fait de 64 chiffres hexadécimaux.
    #[error("jeton du canal de contrôle invalide")]
    InvalidToken,
    /// Rendez-vous incohérent (version, chemin non UTF-8…).
    #[error("rendez-vous invalide : {0}")]
    Invalid(String),
    /// L'application a refusé la requête.
    #[error("canal de contrôle : refus {}", .0.code)]
    Refused(WireError),
    /// Adresse de rendez-vous illisible ou hors boucle locale
    /// (`control.bad_endpoint`, J8, C-CTL-11).
    #[error("adresse du canal de contrôle invalide : {0}")]
    BadEndpoint(String),
}

impl ControlError {
    fn io(context: &str, e: &std::io::Error) -> Self {
        Self::Io(format!("{context} : {e}"))
    }
}

/// Octets du jeton.
const TOKEN_BYTES: usize = 32;

/// Nouveau jeton aléatoire : 32 octets du système, en 64 chiffres
/// hexadécimaux minuscules.
///
/// # Errors
///
/// [`ControlError::Io`] si le générateur du système est indisponible.
pub fn new_token() -> Result<String, ControlError> {
    let mut bytes = [0u8; TOKEN_BYTES];
    getrandom::fill(&mut bytes).map_err(|e| ControlError::Io(format!("aléa du système : {e}")))?;
    Ok(encode_hex(&bytes))
}

fn encode_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len().saturating_mul(2));
    for &b in bytes {
        for nibble in [b.checked_shr(4).unwrap_or(0), b & 0x0f] {
            if let Some(&d) = DIGITS.get(usize::from(nibble)) {
                out.push(char::from(d));
            }
        }
    }
    out
}

/// Décode un jeton de 64 chiffres hexadécimaux (casse indifférente).
fn decode_token(token: &str) -> Option<[u8; TOKEN_BYTES]> {
    let digits = token.as_bytes();
    if digits.len() != TOKEN_BYTES.saturating_mul(2) {
        return None;
    }
    let mut out = [0u8; TOKEN_BYTES];
    let (pairs, _) = digits.as_chunks::<2>();
    for (slot, [hi, lo]) in out.iter_mut().zip(pairs) {
        *slot = hex_value(*hi)?.checked_shl(4)? | hex_value(*lo)?;
    }
    Some(out)
}

fn hex_value(d: u8) -> Option<u8> {
    match d {
        b'0'..=b'9' => d.checked_sub(b'0'),
        b'a'..=b'f' => d.checked_sub(b'a').and_then(|v| v.checked_add(10)),
        b'A'..=b'F' => d.checked_sub(b'A').and_then(|v| v.checked_add(10)),
        _ => None,
    }
}

/// Comparaison en temps constant (boucle XOR sur les 32 octets).
fn token_matches(expected: &[u8; TOKEN_BYTES], given: &str) -> bool {
    // Un jeton mal formé est comparé à zéro pour garder la même boucle.
    let (well_formed, given) = match decode_token(given) {
        Some(bytes) => (true, bytes),
        None => (false, [0u8; TOKEN_BYTES]),
    };
    let mut diff = 0u8;
    for (a, b) in expected.iter().zip(given.iter()) {
        diff |= a ^ b;
    }
    well_formed && diff == 0
}

/// Suffixe unique par processus des fichiers du canal (`<pid>-<n>`).
fn next_stem() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{}-{n}", std::process::id())
}

/// Crée le dossier d'état (0700 sous Unix) et resserre ses droits s'il
/// existait déjà.
fn ensure_dir(dir: &Path) -> Result<(), ControlError> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        builder.mode(0o700);
        builder
            .create(dir)
            .map_err(|e| ControlError::io("création du dossier d'état", &e))?;
        let meta = std::fs::metadata(dir).map_err(|e| ControlError::io("dossier d'état", &e))?;
        if meta.permissions().mode() & 0o777 != 0o700 {
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| ControlError::io("droits du dossier d'état", &e))?;
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        builder
            .create(dir)
            .map_err(|e| ControlError::io("création du dossier d'état", &e))
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::string_slice,
        reason = "tests : un échec doit s'arrêter net"
    )]

    use super::*;

    #[test]
    fn control_token_hex_roundtrip() {
        let token = new_token().unwrap();
        assert_eq!(token.len(), 64);
        let bytes = decode_token(&token).unwrap();
        assert_eq!(encode_hex(&bytes), token);
        assert!(token_matches(&bytes, &token));
        assert!(token_matches(&bytes, &token.to_uppercase()));
    }

    #[test]
    fn control_token_mismatch_and_malformed() {
        let bytes = [7u8; TOKEN_BYTES];
        let good = encode_hex(&bytes);
        let mut bad = good.clone();
        bad.replace_range(63.., "8");
        assert!(!token_matches(&bytes, &bad));
        assert!(!token_matches(&bytes, ""));
        assert!(!token_matches(&bytes, &"g".repeat(64)));
        assert!(!token_matches(&bytes, &good[..62]));
        assert!(!token_matches(&[0u8; TOKEN_BYTES], "zz"));
    }
}
