//! Relais des appels d'un agent vers l'application (J7, ADR-0015 D9, D10 ;
//! contrat du lot L0, écouteur et réducteur au lot L3).
//!
//! L'écouteur ne touche jamais la session : chaque requête devient un
//! [`RelayCall`] poussé dans une file bornée, vidée par le réducteur de
//! l'application en début d'image ; l'interface est réveillée par un
//! rappel injecté ([`Waker`] : `plannercraft-automation` ignore egui, règle 9).
//! La réponse revient par `reply` sur la même connexion. Une écriture en
//! attente de validation peut expirer : [`CallState`] tranche la course
//! entre « Accepter » et le délai (C-CTL-04).

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::SyncSender;

use serde_json::Value;

use super::Reply;

/// Rappel qui réveille l'interface (desktop : `ctx.request_repaint`).
pub type Waker = Arc<dyn Fn() + Send + Sync>;

/// En attente.
const PENDING: u8 = 0;
/// Pris par l'application (accepté, refusé ou servi).
const TAKEN: u8 = 1;
/// Expiré côté écouteur.
const EXPIRED: u8 = 2;

/// État d'un appel relayé, partagé par l'écouteur et l'application :
/// `Pending` puis, une seule fois, `Taken` ou `Expired`.
#[derive(Debug, Default)]
pub struct CallState(AtomicU8);

impl CallState {
    /// Nouvel appel en attente.
    #[must_use]
    pub const fn new() -> Self {
        Self(AtomicU8::new(PENDING))
    }

    /// `Pending` → `Taken` ; faux si l'appel a déjà expiré ou été pris.
    pub fn claim(&self) -> bool {
        self.0
            .compare_exchange(PENDING, TAKEN, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    /// `Pending` → `Expired` ; faux si l'application l'a déjà pris.
    pub fn expire(&self) -> bool {
        self.0
            .compare_exchange(PENDING, EXPIRED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    /// Vrai tant que l'appel n'est ni pris ni expiré.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        self.0.load(Ordering::Acquire) == PENDING
    }
}

/// Nature d'un appel relayé.
#[derive(Clone, Debug, PartialEq)]
pub enum RelayKind {
    /// Outil du registre (`execute { tool, params }` ; `query` aussi).
    Execute {
        /// Id du registre (`task.add`).
        tool: String,
        /// Paramètres JSON.
        params: Value,
    },
    /// État de l'interface (`app.ui_state`).
    UiState,
    /// Capture de la fenêtre (`app.screenshot`), réduite `scale_down`
    /// fois de moitié.
    Screenshot {
        /// Réductions de moitié demandées.
        scale_down: u8,
    },
    /// L'écouteur abandonne l'appel `id` (délai expiré, client parti) :
    /// l'application retire son bandeau. Le `state` du [`RelayCall`] qui
    /// porte ce `Cancel` est celui de l'appel abandonné : seul l'appel de
    /// même `id` **et** de même état est retiré (deux agents numérotent
    /// leurs appels de la même façon).
    Cancel(u64),
}

/// Appel relayé, poussé par l'écouteur dans la file de l'application.
#[derive(Debug)]
pub struct RelayCall {
    /// Identifiant de la requête (celui du client).
    pub id: u64,
    /// Nature.
    pub kind: RelayKind,
    /// État partagé avec l'écouteur.
    pub state: Arc<CallState>,
    /// Réponse (`pending_approval`, puis `ok` ou `err`).
    pub reply: SyncSender<Reply>,
}

#[cfg(test)]
mod tests {
    use super::CallState;

    #[test]
    fn call_state_claim_and_expire_are_exclusive() {
        let s = CallState::new();
        assert!(s.is_pending());
        assert!(s.claim());
        assert!(!s.expire());
        assert!(!s.claim());
        let s = CallState::default();
        assert!(s.expire());
        assert!(!s.claim());
        assert!(!s.is_pending());
    }
}
