//! Historique d'annulation : une entrée par lot commité (R-164, lot L2).
//!
//! Chaque entrée porte un identifiant unique (1, 2, …, jamais réutilisé) :
//! l'état courant est désigné par l'identifiant de la dernière entrée
//! annulable, ou par la base (état d'ouverture, puis dernière entrée
//! abandonnée par le budget). `saved_at` de la session retient cet
//! identifiant : après une annulation suivie d'un nouveau lot, la même
//! profondeur de pile ne passe pas pour l'état enregistré.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::command::Batch;
use crate::delta::Delta;
use crate::session::SessionLimits;

/// Historique d'une session : pile d'annulation, pile de rétablissement,
/// entrées abandonnées par le budget.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct History {
    undo: VecDeque<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    dropped: u64,
    /// Identifiant de l'état au bas de la pile d'annulation (0 : ouverture).
    base: u64,
    /// Dernier identifiant attribué.
    last_id: u64,
}

/// Origine d'un lot de l'historique (J7, ADR-0015 D10) : l'utilisateur, ou
/// un agent dont l'écriture relayée a été acceptée dans l'application.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditSource {
    /// Utilisateur (interface, CLI, MCP local).
    #[default]
    User,
    /// Agent, par le relais du canal de contrôle.
    Agent,
}

impl EditSource {
    /// Vrai pour [`EditSource::User`] (omis du JSON).
    #[must_use]
    pub const fn is_user(&self) -> bool {
        matches!(self, Self::User)
    }
}

/// Entrée d'historique.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HistoryEntry {
    pub label: Option<String>,
    /// Origine du lot (J7, ADR-0015 D10).
    pub source: EditSource,
    pub ops: Batch,
    pub delta: Delta,
    pub bytes: usize,
    /// Identifiant de l'état après ce lot.
    pub id: u64,
}

/// Vue de l'historique (`history.list`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct HistoryView {
    /// Entrées annulables, de la plus ancienne à la plus récente.
    pub entries: Vec<HistoryItem>,
    /// Entrées rétablissables, de la prochaine à la plus lointaine.
    pub redo: Vec<HistoryItem>,
    /// Entrées abandonnées par le budget (R-164).
    pub dropped: u64,
}

/// Résumé d'une entrée.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HistoryItem {
    /// Libellé du lot.
    pub label: Option<String>,
    /// Nombre d'opérations.
    pub ops: usize,
    /// Taille approchée du delta.
    pub bytes: usize,
    /// Origine du lot (J7) ; omise pour l'utilisateur (JSON J6 inchangé).
    #[serde(skip_serializing_if = "EditSource::is_user")]
    pub source: EditSource,
}

impl History {
    /// Identifiant de l'état courant (clé de `saved_at`).
    pub(crate) fn position(&self) -> u64 {
        self.undo.back().map_or(self.base, |e| e.id)
    }

    /// Dernier identifiant attribué (journal de reprise, J7 lot L15) : il
    /// ne change qu'à un nouveau commit.
    pub(crate) const fn last_id(&self) -> u64 {
        self.last_id
    }

    /// Dernière entrée annulable (journal de reprise, J7 lot L15).
    pub(crate) fn top(&self) -> Option<&HistoryEntry> {
        self.undo.back()
    }

    /// Vrai si l'état `id` est encore atteignable par annuler/rétablir.
    pub(crate) fn reachable(&self, id: u64) -> bool {
        id == self.base || self.undo.iter().chain(&self.redo).any(|e| e.id == id)
    }

    /// Nombre d'entrées annulables.
    pub(crate) fn len(&self) -> usize {
        self.undo.len()
    }

    /// Empile un lot commité (R-164) d'origine `source` : vide la pile de
    /// rétablissement, puis abandonne les plus anciennes entrées tant que le
    /// budget (octets ou nombre) est dépassé, en gardant toujours la
    /// dernière.
    pub(crate) fn push(
        &mut self,
        label: Option<String>,
        source: EditSource,
        ops: Batch,
        delta: Delta,
        bytes: usize,
        limits: &SessionLimits,
    ) {
        self.redo.clear();
        self.last_id = self.last_id.saturating_add(1);
        self.undo.push_back(HistoryEntry {
            label,
            source,
            ops,
            delta,
            bytes,
            id: self.last_id,
        });
        let mut total: usize = self.undo.iter().fold(0, |acc, e| acc.saturating_add(e.bytes));
        while self.undo.len() > 1
            && (total > limits.history_bytes || self.undo.len() > limits.history_entries)
        {
            let Some(oldest) = self.undo.pop_front() else {
                break;
            };
            total = total.saturating_sub(oldest.bytes);
            self.base = oldest.id;
            self.dropped = self.dropped.saturating_add(1);
        }
    }

    /// Retire l'entrée à annuler.
    pub(crate) fn pop_undo(&mut self) -> Option<HistoryEntry> {
        self.undo.pop_back()
    }

    /// Range une entrée annulée (rétablissable).
    pub(crate) fn push_redo(&mut self, e: HistoryEntry) {
        self.redo.push(e);
    }

    /// Retire l'entrée à rétablir.
    pub(crate) fn pop_redo(&mut self) -> Option<HistoryEntry> {
        self.redo.pop()
    }

    /// Remet une entrée rétablie (ou une annulation échouée) en haut de la
    /// pile d'annulation, sans toucher à la pile de rétablissement.
    pub(crate) fn push_undo_back(&mut self, e: HistoryEntry) {
        self.undo.push_back(e);
    }

    /// Vue de l'historique.
    pub(crate) fn view(&self) -> HistoryView {
        let item = |e: &HistoryEntry| HistoryItem {
            label: e.label.clone(),
            ops: e.ops.ops.len(),
            bytes: e.bytes,
            source: e.source,
        };
        HistoryView {
            entries: self.undo.iter().map(item).collect(),
            redo: self.redo.iter().rev().map(item).collect(),
            dropped: self.dropped,
        }
    }
}
