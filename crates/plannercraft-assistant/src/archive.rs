//! Conversations gardées par document (lot A1b, cahier
//! `docs/plans/assistant-chat.md` § 3), dans le stockage local de
//! l'application : jamais dans le fichier projet (un planning partagé ne
//! transporte pas la conversation), jamais dans `UiState` (envoyé aux
//! agents MCP rattachés).
//!
//! - Clé : identifiant du document donné par l'interface (chemin canonique,
//!   ou nom pour l'exemple et le web).
//! - Une conversation **active** par document ; « Nouvelle conversation »
//!   l'archive (gardée, pas relue : la liste des anciennes viendra plus
//!   tard).
//! - Au plus [`MAX_CONVERSATIONS`] conversations par appareil, la plus
//!   ancienne (dernière modification) supprimée ; une conversation de plus
//!   de [`MAX_CONVERSATION_BYTES`] perd ses tours les plus anciens ;
//!   l'ensemble est borné à [`MAX_ARCHIVE_BYTES`] (stockage du navigateur).
//! - Aucun secret : les tours ne portent ni clé ni jeton (test
//!   `no_secret_in_archive`).

use serde::{Deserialize, Serialize};

use crate::conversation::SavedConversation;

/// Conversations gardées au plus par appareil.
pub const MAX_CONVERSATIONS: usize = 20;
/// Taille au plus d'une conversation gardée (JSON).
pub const MAX_CONVERSATION_BYTES: usize = 256 * 1024;
/// Taille au plus de l'ensemble (JSON).
pub const MAX_ARCHIVE_BYTES: usize = 1024 * 1024;
/// Longueur au plus d'une clé de document.
pub const MAX_KEY_CHARS: usize = 1024;
/// Version du format.
pub const ARCHIVE_VERSION: u32 = 1;

/// Conversation gardée pour un document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArchiveEntry {
    /// Clé du document.
    pub key: String,
    /// Ordre de dernière modification (plus grand : plus récent).
    pub seq: u64,
    /// Conversation active du document (relue à l'ouverture), ou archivée
    /// par « Nouvelle conversation ».
    pub active: bool,
    /// Conversation.
    pub conversation: SavedConversation,
}

/// Ensemble des conversations gardées.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Archive {
    /// Version du format.
    #[serde(default)]
    pub version: u32,
    /// Prochain numéro d'ordre.
    #[serde(default)]
    pub next_seq: u64,
    /// Conversations.
    #[serde(default)]
    pub entries: Vec<ArchiveEntry>,
}

impl Archive {
    /// Relit `json` ; un texte illisible donne un ensemble vide (jamais
    /// d'erreur bloquante : ce n'est qu'un confort).
    #[must_use]
    pub fn from_json(json: &str) -> Self {
        let mut archive: Self = serde_json::from_str(json).unwrap_or_default();
        archive.version = ARCHIVE_VERSION;
        archive.entries.retain(|e| e.key.chars().count() <= MAX_KEY_CHARS);
        archive.enforce_bounds();
        archive
    }

    /// JSON de l'ensemble.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Conversation active du document `key`.
    #[must_use]
    pub fn active(&self, key: &str) -> Option<&SavedConversation> {
        self.entries
            .iter()
            .find(|e| e.active && e.key == key)
            .map(|e| &e.conversation)
    }

    /// Range la conversation active de `key` (remplace la précédente) ; une
    /// conversation vide retire l'entrée active.
    pub fn put_active(&mut self, key: &str, conversation: SavedConversation) {
        if key.chars().count() > MAX_KEY_CHARS {
            return;
        }
        let existing = self.entries.iter().position(|e| e.active && e.key == key);
        if conversation.turns.is_empty() {
            if let Some(i) = existing {
                self.entries.remove(i);
            }
            return;
        }
        if existing
            .and_then(|i| self.entries.get(i))
            .is_some_and(|e| e.conversation == conversation)
        {
            return;
        }
        let seq = self.bump();
        let entry = ArchiveEntry {
            key: key.to_owned(),
            seq,
            active: true,
            conversation: shrink(conversation),
        };
        match existing.and_then(|i| self.entries.get_mut(i)) {
            Some(slot) => *slot = entry,
            None => self.entries.push(entry),
        }
        self.enforce_bounds();
    }

    /// Archive la conversation active de `key` (« Nouvelle conversation ») :
    /// gardée, plus relue à l'ouverture.
    pub fn archive_active(&mut self, key: &str) {
        let seq = self.bump();
        if let Some(entry) = self.entries.iter_mut().find(|e| e.active && e.key == key) {
            entry.active = false;
            entry.seq = seq;
        }
    }

    fn bump(&mut self) -> u64 {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        seq
    }

    /// Au plus [`MAX_CONVERSATIONS`] entrées et [`MAX_ARCHIVE_BYTES`] : les
    /// plus anciennes tombent.
    fn enforce_bounds(&mut self) {
        self.entries.sort_by_key(|e| e.seq);
        let extra = self.entries.len().saturating_sub(MAX_CONVERSATIONS);
        self.entries.drain(..extra);
        while self.entries.len() > 1 && self.to_json().len() > MAX_ARCHIVE_BYTES {
            self.entries.remove(0);
        }
        if let Some(max) = self.entries.iter().map(|e| e.seq).max() {
            self.next_seq = self.next_seq.max(max.saturating_add(1));
        }
    }
}

/// Retire les tours les plus anciens tant que la conversation dépasse
/// [`MAX_CONVERSATION_BYTES`] (le dernier tour est gardé).
fn shrink(mut conversation: SavedConversation) -> SavedConversation {
    let size = |c: &SavedConversation| serde_json::to_string(c).map_or(usize::MAX, |s| s.len());
    while conversation.turns.len() > 1 && size(&conversation) > MAX_CONVERSATION_BYTES {
        conversation.turns.remove(0);
    }
    conversation
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : un échec doit s'arrêter net"
    )]

    use super::*;
    use crate::conversation::{Turn, TurnState};

    fn conversation(question: &str, answer_len: usize) -> SavedConversation {
        let mut turn = Turn::new(question, "anthropic/claude-sonnet-5.5", Some("14:32".to_owned()));
        turn.answer = "r".repeat(answer_len);
        turn.state = TurnState::Done;
        SavedConversation {
            version: 1,
            turns: vec![turn],
        }
    }

    #[test]
    fn one_active_per_document_and_new_conversation_archives() {
        let mut archive = Archive::default();
        archive.put_active("file:/a.xml", conversation("Q1", 10));
        archive.put_active("file:/b.xml", conversation("Q2", 10));
        assert_eq!(archive.active("file:/a.xml").unwrap().turns[0].question, "Q1");
        archive.put_active("file:/a.xml", conversation("Q1bis", 10));
        assert_eq!(archive.entries.len(), 2);
        archive.archive_active("file:/a.xml");
        assert!(archive.active("file:/a.xml").is_none());
        assert_eq!(archive.entries.len(), 2, "archivée, pas supprimée");
        archive.put_active("file:/a.xml", conversation("Q3", 10));
        assert_eq!(archive.entries.len(), 3);
        // Relu à l'identique.
        let again = Archive::from_json(&archive.to_json());
        assert_eq!(again.active("file:/a.xml"), archive.active("file:/a.xml"));
        assert_eq!(again.entries.len(), 3);
        // Vide : l'entrée active disparaît.
        archive.put_active("file:/b.xml", SavedConversation::default());
        assert!(archive.active("file:/b.xml").is_none());
    }

    #[test]
    fn at_most_twenty_oldest_dropped() {
        let mut archive = Archive::default();
        for i in 0..25 {
            archive.put_active(&format!("doc{i}"), conversation("Q", 10));
        }
        assert_eq!(archive.entries.len(), MAX_CONVERSATIONS);
        assert!(archive.active("doc0").is_none());
        assert!(archive.active("doc24").is_some());
        // Modifier une ancienne la rend récente.
        archive.put_active("doc5", conversation("Q encore", 10));
        archive.put_active("new", conversation("Q", 10));
        assert!(archive.active("doc5").is_some());
        assert!(archive.active("doc6").is_none());
    }

    #[test]
    fn size_bounds() {
        let mut archive = Archive::default();
        let mut big = conversation("Q0", MAX_CONVERSATION_BYTES / 2);
        big.turns
            .extend(conversation("Q1", MAX_CONVERSATION_BYTES / 2).turns);
        big.turns.extend(conversation("Q2", 10).turns);
        archive.put_active("big", big);
        let kept = archive.active("big").unwrap();
        assert_eq!(kept.turns.last().unwrap().question, "Q2");
        assert!(serde_json::to_string(kept).unwrap().len() <= MAX_CONVERSATION_BYTES);
        for i in 0..10 {
            archive.put_active(&format!("d{i}"), conversation("Q", MAX_CONVERSATION_BYTES / 2));
        }
        assert!(archive.to_json().len() <= MAX_ARCHIVE_BYTES);
        assert!(archive.active("d9").is_some());
    }

    #[test]
    fn unreadable_json_gives_empty() {
        assert_eq!(Archive::from_json("{oops").entries.len(), 0);
        assert_eq!(Archive::from_json("").entries.len(), 0);
    }
}
