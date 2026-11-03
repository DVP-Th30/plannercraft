//! Messages à l'utilisateur (seconde passe UX, ADR-0019 D1, H-UI-104).
//!
//! Trois voies : les **notifications** (en bas à droite, effacées seules
//! après [`lifetime`]), le **bandeau** (en haut, jusqu'à décision) et
//! l'**historique** (les [`MAX_HISTORY`] derniers messages, quelle que soit
//! leur voie). Le temps est celui d'egui (`ctx.input(|i| i.time)`, en
//! secondes) : déterministe sous kittest.
//!
//! Le dessin (zone des notifications ancrée en bas à droite, bandeau,
//! bouton et fenêtre de l'historique) est dans `view::chrome` ; `App`
//! appelle [`MessageStore::expire`] une fois par image avec la
//! notification survolée de l'image précédente.

use plannercraft_core::Severity;

use crate::i18n::TextId;

/// Messages gardés au bandeau au plus (les plus récents).
pub const MAX_MESSAGES: usize = 3;

/// Notifications affichées au plus (la plus ancienne part, H-UI-104).
pub const MAX_TOASTS: usize = 3;

/// Messages gardés dans l'historique au plus (H-UI-104).
pub const MAX_HISTORY: usize = 20;

/// Sorte d'un message : décide de sa voie et de sa durée (H-UI-104).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MessageKind {
    /// Information (compte rendu d'un collage…) : notification de 5 s.
    #[default]
    Info,
    /// Avertissement : notification de 8 s.
    Warning,
    /// Refus d'une saisie ou d'une commande (erreur non bloquante) :
    /// notification de 10 s.
    Refusal,
    /// Attend une décision (confirmation de sortie, reprise, ouverture ou
    /// enregistrement échoués, état illisible) : bandeau, sans délai.
    Blocking,
}

/// Durée de vie d'une notification, en secondes ; `None` pour le bandeau
/// (`Blocking`), qui reste jusqu'à décision.
#[must_use]
pub const fn lifetime(kind: MessageKind) -> Option<f64> {
    match kind {
        MessageKind::Info => Some(5.0),
        MessageKind::Warning => Some(8.0),
        MessageKind::Refusal => Some(10.0),
        MessageKind::Blocking => None,
    }
}

/// Sorte d'un message publié par `App::push_titled` : ouverture,
/// enregistrement et export échoués ⇒ `Blocking` (une décision ou un
/// constat qui ne doit pas s'effacer seul) ; erreur d'une commande ⇒
/// `Refusal` ; avertissement ⇒ `Warning` ; le reste ⇒ `Info` (compte rendu
/// d'un collage compris, H-UI-104). La confirmation des modifications et
/// la reprise ont leurs propres bandeaux ; l'état illisible est publié
/// `Blocking` par `App::report_state_load`.
#[must_use]
pub const fn kind_for(severity: Severity, title: Option<TextId>) -> MessageKind {
    match (title, severity) {
        (Some(TextId::OpenErrorTitle | TextId::SaveErrorTitle | TextId::ExportErrorTitle), _) => {
            MessageKind::Blocking
        }
        (Some(TextId::PasteReportTitle), _) | (_, Severity::Info) => MessageKind::Info,
        (_, Severity::Error) => MessageKind::Refusal,
        (_, Severity::Warning) => MessageKind::Warning,
    }
}

/// Message affiché à l'utilisateur.
#[derive(Clone, Debug, PartialEq)]
pub struct UserMessage {
    /// Sorte (voie et durée).
    pub kind: MessageKind,
    /// Gravité.
    pub severity: Severity,
    /// Titre affiché au-dessus du message (« Impossible d'ouvrir le
    /// fichier », « Modification refusée »), s'il y en a un.
    pub title: Option<TextId>,
    /// Texte déjà traduit, en langage d'utilisateur.
    pub text: String,
    /// Détail technique repliable (code, opération, texte du moteur ;
    /// H-UI-105), s'il y en a un.
    pub detail: Option<String>,
    /// Instant de publication (secondes d'egui), relancé par un doublon.
    pub posted_at: f64,
    /// Identifiant, unique dans la vie de la pile (posé par
    /// [`MessageStore::post`]).
    pub id: u64,
}

impl UserMessage {
    /// Message de sorte `kind`, sans détail ; instant et identifiant posés
    /// à la publication.
    #[must_use]
    pub const fn new(kind: MessageKind, severity: Severity, title: Option<TextId>, text: String) -> Self {
        Self {
            kind,
            severity,
            title,
            text,
            detail: None,
            posted_at: 0.0,
            id: 0,
        }
    }
}

/// Pile des messages : notifications ([`MAX_TOASTS`]), bandeau
/// ([`MAX_MESSAGES`]) et historique ([`MAX_HISTORY`]), compte des non lus
/// et ouverture de l'historique (H-UI-104).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MessageStore {
    /// Notifications, la plus ancienne d'abord.
    toasts: Vec<UserMessage>,
    /// Bandeau (`Blocking`), le plus ancien d'abord.
    banner: Vec<UserMessage>,
    /// Messages affichés (bandeau et notifications), le plus ancien
    /// d'abord : refait après chaque changement des deux voies.
    live: Vec<UserMessage>,
    /// Historique, le plus ancien d'abord.
    history: Vec<UserMessage>,
    /// Messages publiés depuis la dernière ouverture de l'historique.
    unread: usize,
    /// L'historique est ouvert (fenêtre de la barre d'état).
    history_open: bool,
    /// Prochain identifiant.
    next_id: u64,
}

impl MessageStore {
    /// Publie `m` : identifiant neuf ; `Blocking` au bandeau, le reste aux
    /// notifications ; un message de même texte déjà dans sa voie est
    /// retiré (le nouveau le remplace, minuterie relancée, pas de
    /// doublon) ; voie pleine : le plus ancien part. Copie dans
    /// l'historique ; compté non lu tant que l'historique est fermé.
    pub fn post(&mut self, mut m: UserMessage) {
        self.next_id = self.next_id.wrapping_add(1);
        m.id = self.next_id;
        if !m.posted_at.is_finite() {
            m.posted_at = 0.0;
        }
        let (lane, cap) = if m.kind == MessageKind::Blocking {
            (&mut self.banner, MAX_MESSAGES)
        } else {
            (&mut self.toasts, MAX_TOASTS)
        };
        lane.retain(|old| old.text != m.text);
        lane.push(m.clone());
        let excess = lane.len().saturating_sub(cap);
        lane.drain(..excess);
        self.history.push(m);
        let excess = self.history.len().saturating_sub(MAX_HISTORY);
        self.history.drain(..excess);
        if !self.history_open {
            self.unread = self.unread.saturating_add(1).min(MAX_HISTORY);
        }
        self.rebuild_live();
    }

    /// Refait [`MessageStore::live`] : les deux voies, par identifiant
    /// (ordre de publication).
    fn rebuild_live(&mut self) {
        self.live.clear();
        self.live.extend(self.banner.iter().cloned());
        self.live.extend(self.toasts.iter().cloned());
        self.live.sort_by_key(|m| m.id);
    }

    /// Retire les notifications échues à l'instant `now` (secondes) ; la
    /// notification survolée `hovered` est gardée et sa minuterie relancée
    /// (survol = pause). `now` non fini : rien.
    pub fn expire(&mut self, now: f64, hovered: Option<u64>) {
        if !now.is_finite() {
            return;
        }
        for toast in &mut self.toasts {
            if Some(toast.id) == hovered {
                toast.posted_at = now;
            }
        }
        let before = self.toasts.len();
        self.toasts
            .retain(|toast| lifetime(toast.kind).is_none_or(|life| now - toast.posted_at < life));
        if self.toasts.len() != before || hovered.is_some() {
            self.rebuild_live();
        }
    }

    /// Délai (secondes, fini et positif) avant l'échéance de la prochaine
    /// notification, vu de `now` ; `None` sans notification (aucun repeint
    /// à demander).
    #[must_use]
    pub fn next_expiry(&self, now: f64) -> Option<f64> {
        if !now.is_finite() {
            return None;
        }
        self.toasts
            .iter()
            .filter_map(|toast| lifetime(toast.kind).map(|life| toast.posted_at + life - now))
            .filter(|left| left.is_finite())
            .map(|left| left.max(0.0))
            .reduce(f64::min)
    }

    /// Ferme le message `id` (notification ou bandeau) ; les autres restent.
    pub fn dismiss(&mut self, id: u64) {
        self.toasts.retain(|m| m.id != id);
        self.banner.retain(|m| m.id != id);
        self.rebuild_live();
    }

    /// Ferme tous les messages affichés (bandeau et notifications ;
    /// l'historique est gardé).
    pub fn clear_live(&mut self) {
        self.toasts.clear();
        self.banner.clear();
        self.live.clear();
    }

    /// Garde, au bandeau et dans les notifications, les messages pour
    /// lesquels `keep` est vrai (l'historique est inchangé).
    pub fn retain(&mut self, mut keep: impl FnMut(&UserMessage) -> bool) {
        self.toasts.retain(&mut keep);
        self.banner.retain(&mut keep);
        self.rebuild_live();
    }

    /// Ouvre ou ferme l'historique ; l'ouvrir marque tout comme lu.
    pub fn toggle_history(&mut self) {
        self.history_open = !self.history_open;
        if self.history_open {
            self.unread = 0;
        }
    }

    /// L'historique est-il ouvert ?
    #[must_use]
    pub const fn history_open(&self) -> bool {
        self.history_open
    }

    /// Messages publiés depuis la dernière ouverture de l'historique (au
    /// plus [`MAX_HISTORY`]).
    #[must_use]
    pub const fn unread(&self) -> usize {
        self.unread
    }

    /// Messages du bandeau, le plus ancien d'abord.
    #[must_use]
    pub fn banner(&self) -> &[UserMessage] {
        &self.banner
    }

    /// Notifications, la plus ancienne d'abord.
    #[must_use]
    pub fn toasts(&self) -> &[UserMessage] {
        &self.toasts
    }

    /// Messages affichés (bandeau et notifications), dans l'ordre de
    /// publication.
    #[must_use]
    pub fn live(&self) -> &[UserMessage] {
        &self.live
    }

    /// Historique, le plus ancien d'abord.
    #[must_use]
    pub fn history(&self) -> &[UserMessage] {
        &self.history
    }
}

/// Textes posés par le lot L0 pour les messages, servis par le lot L1
/// (notifications, historique, détails repliables) : titre de la cloche,
/// compte des non lus, « Détails », « Fermer », historique vide.
pub const L1_TEXTS: [TextId; 6] = [
    TextId::NotifyHistory,
    TextId::NotifyUnread,
    TextId::NotifyDetails,
    TextId::NotifyClose,
    TextId::NotifyHistoryEmpty,
    TextId::NotifyHistoryTitle,
];

/// Textes posés par le lot L0 pour les comptes rendus et refus du collage
/// et des liens (D2, D3), servis par le lot L2 : liens non repris
/// (reformulé, sans « un bout »), lignes collées sur des cellules, valeurs
/// hors de la table, paire déjà liée, « Remplir à droite ».
pub const L2_TEXTS: [TextId; 5] = [
    TextId::PasteLinksDropped,
    TextId::PasteRowsOnCells,
    TextId::PasteValuesOutside,
    TextId::LinkAlreadyLinked,
    TextId::MenuFillRight,
];

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use plannercraft_core::Severity;

    use super::{MAX_HISTORY, MAX_MESSAGES, MAX_TOASTS, MessageKind, MessageStore, UserMessage, lifetime};

    fn message(kind: MessageKind, text: &str, at: f64) -> UserMessage {
        UserMessage {
            posted_at: at,
            ..UserMessage::new(kind, Severity::Info, None, text.to_owned())
        }
    }

    /// H-UI-104 : 5, 8, 10 s ; `Blocking` jamais ; une notification part
    /// à l'échéance, pas avant ; le survol la garde.
    #[test]
    fn notify_lifetimes() {
        assert_eq!(lifetime(MessageKind::Info), Some(5.0));
        assert_eq!(lifetime(MessageKind::Warning), Some(8.0));
        assert_eq!(lifetime(MessageKind::Refusal), Some(10.0));
        assert_eq!(lifetime(MessageKind::Blocking), None);
        let mut store = MessageStore::default();
        store.post(message(MessageKind::Info, "info", 1.0));
        store.post(message(MessageKind::Warning, "warning", 1.0));
        store.post(message(MessageKind::Refusal, "refus", 1.0));
        store.post(message(MessageKind::Blocking, "décision", 1.0));
        store.expire(5.9, None);
        assert_eq!(store.toasts().len(), 3);
        store.expire(6.0, None);
        assert_eq!(store.toasts().len(), 2);
        store.expire(10.9, None);
        assert_eq!(store.toasts().len(), 1);
        let refusal = store.toasts()[0].id;
        // Survolée à 10,9 s : minuterie relancée, gardée à 11 s et 20,8 s.
        store.expire(10.95, Some(refusal));
        store.expire(20.8, None);
        assert_eq!(store.toasts().len(), 1);
        store.expire(f64::NAN, None);
        assert_eq!(store.toasts().len(), 1);
        store.expire(21.0, None);
        assert_eq!(store.toasts().len(), 0);
        store.expire(1.0e9, None);
        assert_eq!(store.banner().len(), 1, "le bandeau ne s'efface pas seul");
    }

    /// H-UI-104 : 3 notifications au plus (la plus ancienne part), 20
    /// messages d'historique, `dismiss` ne retire que son message ; un
    /// doublon remplace l'ancien ; `live` suit les deux voies ; non lus
    /// remis à zéro à l'ouverture de l'historique.
    #[test]
    fn notify_store_bounds() {
        let mut store = MessageStore::default();
        for i in 0..25 {
            store.post(message(MessageKind::Info, &i.to_string(), 0.0));
        }
        let texts: Vec<&str> = store.toasts().iter().map(|m| m.text.as_str()).collect();
        assert_eq!(texts, ["22", "23", "24"]);
        assert_eq!(store.toasts().len(), MAX_TOASTS);
        assert_eq!(store.history().len(), MAX_HISTORY);
        assert_eq!(store.history()[0].text, "5");
        store.post(message(MessageKind::Blocking, "a", 0.0));
        store.post(message(MessageKind::Blocking, "b", 0.0));
        let a = store.banner()[0].id;
        store.dismiss(a);
        let banner: Vec<&str> = store.banner().iter().map(|m| m.text.as_str()).collect();
        assert_eq!(banner, ["b"]);
        assert_eq!(store.toasts().len(), MAX_TOASTS);
        let middle = store.toasts()[1].id;
        store.dismiss(middle);
        let texts: Vec<&str> = store.toasts().iter().map(|m| m.text.as_str()).collect();
        assert_eq!(texts, ["22", "24"]);
        // Doublon : remplace, identifiant neuf.
        store.post(message(MessageKind::Info, "22", 3.0));
        let texts: Vec<&str> = store.toasts().iter().map(|m| m.text.as_str()).collect();
        assert_eq!(texts, ["24", "22"]);
        assert_eq!(store.toasts()[1].posted_at, 3.0);
        // Ids uniques.
        let mut ids: Vec<u64> = store.history().iter().map(|m| m.id).collect();
        ids.dedup();
        assert_eq!(ids.len(), store.history().len());

        let live: Vec<u64> = store.live().iter().map(|m| m.id).collect();
        let mut expected: Vec<u64> = store
            .banner()
            .iter()
            .chain(store.toasts())
            .map(|m| m.id)
            .collect();
        expected.sort_unstable();
        assert_eq!(live, expected);
        assert_eq!(store.unread(), MAX_HISTORY);
        store.toggle_history();
        assert!(store.history_open());
        assert_eq!(store.unread(), 0);
        store.post(message(MessageKind::Info, "lu", 0.0));
        assert_eq!(store.unread(), 0, "historique ouvert : rien de non lu");
        store.toggle_history();
        store.post(message(MessageKind::Info, "non lu", 0.0));
        assert_eq!(store.unread(), 1);
        store.retain(|m| m.text != "non lu");
        assert!(store.live().iter().all(|m| m.text != "non lu"));
        store.clear_live();
        assert!(store.live().is_empty() && store.banner().is_empty() && store.toasts().is_empty());
        assert_eq!(store.history().len(), MAX_HISTORY);
        assert_eq!(MAX_MESSAGES, 3);
    }

    /// Prochaine échéance : délai de la notification la plus proche, borné
    /// à zéro ; aucune notification ou instant non fini : aucune.
    #[test]
    fn notify_next_expiry() {
        let mut store = MessageStore::default();
        assert_eq!(store.next_expiry(0.0), None);
        store.post(message(MessageKind::Refusal, "refus", 0.0));
        store.post(message(MessageKind::Info, "info", 1.0));
        store.post(message(MessageKind::Blocking, "décision", 1.0));
        assert_eq!(store.next_expiry(2.0), Some(4.0));
        assert_eq!(store.next_expiry(100.0), Some(0.0));
        assert_eq!(store.next_expiry(f64::NAN), None);
    }
}
