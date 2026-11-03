//! Client du relais côté `plannercraft mcp` (J7, ADR-0015 D9, lot L2) : se
//! rattache à l'application qui tient le fichier et lui relaie les appels
//! d'outils.
//!
//! Une connexion par appel : `hello` avec le jeton du rendez-vous, puis la
//! requête (`execute { tool, params }`, `ui_state` ou `screenshot`). Une
//! écriture reçoit d'abord `pending_approval`, puis la réponse finale sur la
//! même connexion et le même `id`, le tout en `approval` plus une marge au
//! plus. Une connexion neuve par appel évite la fermeture d'une connexion
//! inactive par l'application (elle ne pourrait pas être distinguée d'une
//! application partie, et rejouer une écriture déjà reçue la doublerait).
//!
//! Erreurs (C-CTL-06) : connexion impossible, flux fermé ou application
//! muette ⇒ `control.app_gone` (le serveur MCP se détache) ; jeton refusé
//! (l'application a changé de document, son écouteur a pris un nouveau
//! jeton) ⇒ `control.document_changed` (détachement aussi) ; application
//! occupée (`control.busy` de l'écouteur) ⇒ `control.busy` (rattachement
//! gardé) ; validation attendue au-delà du délai ⇒
//! `control.approval_timeout` (le rattachement est gardé). Le délai de
//! validation est transmis à l'application (`approval_timeout_ms` d'un
//! `execute`), qui le prend s'il est plus court que le sien : l'expiration
//! est tranchée d'un seul côté. Transport (J8, lot L8) : celui de
//! l'adresse du rendez-vous, socket Unix ou TCP en boucle locale
//! ([`super::Endpoint::connect`]), sur tous les systèmes.

use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use super::ControlError;
use super::Rendezvous;
use super::{Frame, Method, PROTOCOL_VERSION};
use crate::error::codes;
use crate::text::Language;
use crate::workspace::ToolError;

/// Marge ajoutée au délai de validation pour la réponse finale.
pub const RELAY_MARGIN: Duration = Duration::from_secs(10);

/// Rendez-vous examinés au plus pour le rattachement au démarrage.
const MAX_SCANNED: usize = 64;

/// Taille maximale d'un fichier de rendez-vous lu (comme `rendezvous.rs`).
const MAX_RENDEZVOUS_BYTES: u64 = 64 * 1024;

/// Échec d'un appel relayé.
#[derive(Clone, Debug, PartialEq)]
pub enum RelayError {
    /// Refus de l'application (commande refusée, validation refusée…) :
    /// l'erreur JSON telle qu'envoyée (`code`, `message` et tout autre
    /// champ), à rendre à l'agent sans la retoucher.
    App(Value),
    /// Échec du canal (`control.app_gone`, `control.approval_timeout`).
    Control(ToolError),
}

impl RelayError {
    /// Code stable de l'erreur.
    #[must_use]
    pub fn code(&self) -> &str {
        match self {
            Self::App(v) => v.get("code").and_then(Value::as_str).unwrap_or(""),
            Self::Control(e) => e.code,
        }
    }

    /// Vrai si l'application est partie (le client doit se détacher).
    #[must_use]
    pub fn is_app_gone(&self) -> bool {
        matches!(self, Self::Control(e) if e.code == codes::CONTROL_APP_GONE)
    }

    /// Vrai si le client doit se détacher : application partie, ou document
    /// de l'application changé depuis le rattachement.
    #[must_use]
    pub fn detaches(&self) -> bool {
        matches!(
            self,
            Self::Control(e) if e.code == codes::CONTROL_APP_GONE || e.code == codes::CONTROL_DOCUMENT_CHANGED
        )
    }
}

/// Connexion de `plannercraft mcp` à l'application rattachée.
#[derive(Debug)]
pub struct RelayClient {
    /// Rendez-vous de l'application.
    rendezvous: Rendezvous,
    /// Identifiant de la prochaine requête.
    next_id: u64,
    /// Délai du `hello` de chaque appel.
    hello_timeout: Duration,
    /// Marge ajoutée au délai de validation.
    margin: Duration,
    /// Langue des messages du canal.
    lang: Language,
}

/// Texte selon la langue.
const fn tr(lang: Language, fr: &'static str, en: &'static str) -> &'static str {
    match lang {
        Language::Fr => fr,
        Language::En => en,
    }
}

impl RelayClient {
    /// Se connecte au rendez-vous `r` (`hello` avec son jeton), en
    /// `timeout` au plus ; le même délai borne le `hello` de chaque appel.
    ///
    /// # Errors
    ///
    /// [`ControlError::Io`] (connexion, délai, réponse illisible),
    /// [`ControlError::Refused`] (jeton refusé),
    /// [`ControlError::BadEndpoint`] (adresse du rendez-vous illisible).
    pub fn connect(r: &Rendezvous, timeout: Duration) -> Result<Self, ControlError> {
        let mut client = Self {
            rendezvous: r.clone(),
            next_id: 1,
            hello_timeout: timeout,
            margin: RELAY_MARGIN,
            lang: Language::En,
        };
        client.check()?;
        Ok(client)
    }

    /// Même client, marge `margin` après le délai de validation (défaut
    /// [`RELAY_MARGIN`] ; tests).
    #[must_use]
    pub const fn with_margin(mut self, margin: Duration) -> Self {
        self.margin = margin;
        self
    }

    /// Même client, messages du canal dans `lang` (défaut : anglais).
    #[must_use]
    pub const fn with_lang(mut self, lang: Language) -> Self {
        self.lang = lang;
        self
    }

    /// Rendez-vous de l'application rattachée.
    #[must_use]
    pub const fn rendezvous(&self) -> &Rendezvous {
        &self.rendezvous
    }

    /// Relaie l'outil `tool` (id du registre, `task.add`) avec `params`
    /// (forme du schéma, `dry_run` et `detail` compris) ; une écriture
    /// attend la validation de l'utilisateur `approval` au plus (plus la
    /// marge). `app.ui_state` et `app.screenshot` passent par les méthodes
    /// `ui_state` et `screenshot` ; tout le reste par `execute`.
    ///
    /// # Errors
    ///
    /// [`RelayError::App`] : refus de l'application, tel quel ;
    /// [`RelayError::Control`] : `control.app_gone` (connexion, jeton,
    /// flux fermé, aucune réponse dans le délai) ou
    /// `control.approval_timeout` (validation attendue au-delà du délai).
    pub fn call(&mut self, tool: &str, params: Value, approval: Duration) -> Result<Value, RelayError> {
        let hello_id = self.take_id();
        let id = self.take_id();
        let approval_ms = u64::try_from(approval.as_millis()).unwrap_or(u64::MAX).max(1);
        let (method, params) = match tool {
            "app.ui_state" => (Method::UiState, params),
            "app.screenshot" => (Method::Screenshot, params),
            _ => (
                Method::Execute,
                serde_json::json!({ "tool": tool, "params": params, "approval_timeout_ms": approval_ms }),
            ),
        };
        let frame = Frame {
            v: PROTOCOL_VERSION,
            id,
            method,
            params,
        };
        let deadline = approval.saturating_add(self.margin);
        wire::call(self, hello_id, &frame, deadline)
    }

    /// Rendez-vous vivant **unique** de `dir` (rattachement au démarrage,
    /// ADR-0015 D9) : `None` s'il n'y en a aucun ou plusieurs.
    ///
    /// Les fichiers `.json` de version 1 sont sondés par un `hello` ; rien
    /// n'est supprimé (le nettoyage reste à `find_live_for_in`). Au plus
    /// 64 rendez-vous examinés.
    #[must_use]
    pub fn single_live_in(dir: &Path) -> Option<Rendezvous> {
        let mut files: Vec<_> = std::fs::read_dir(dir)
            .ok()?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
            .collect();
        files.sort();
        let mut live = files
            .iter()
            .take(MAX_SCANNED)
            .filter_map(|f| read_v1(f))
            .filter(super::rendezvous::answers);
        let first = live.next()?;
        live.next().is_none().then_some(first)
    }

    /// Vérifie que l'application répond (`hello`).
    fn check(&mut self) -> Result<(), ControlError> {
        let id = self.take_id();
        wire::open(&self.rendezvous, id, self.hello_timeout).map(drop)
    }

    fn take_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        id
    }

    /// `control.app_gone` avec le message dans la langue du client.
    fn gone(&self, fr: &'static str, en: &'static str) -> RelayError {
        RelayError::Control(ToolError::simple(
            codes::CONTROL_APP_GONE,
            tr(self.lang, fr, en).to_owned(),
        ))
    }
}

/// Lit un rendez-vous de version 1 (fichier ≤ 64 Kio), sans rien supprimer.
fn read_v1(file: &Path) -> Option<Rendezvous> {
    let meta = std::fs::metadata(file).ok()?;
    if !meta.is_file() || meta.len() > MAX_RENDEZVOUS_BYTES {
        return None;
    }
    let bytes = std::fs::read(file).ok()?;
    let r = serde_json::from_slice::<Rendezvous>(&bytes).ok()?;
    (r.v == super::PROTOCOL_VERSION).then_some(r)
}

/// Échanges sur le fil, communs aux deux transports.
mod wire {
    use std::io::{BufReader, ErrorKind, Write};
    use std::time::{Duration, Instant};

    use serde_json::Value;

    use super::super::{
        ControlError, ControlStream, Endpoint, Frame, LineError, MAX_LINE_BYTES, Method, PROTOCOL_VERSION,
        Rendezvous, Reply, read_line_bounded, to_line,
    };
    use super::{RelayClient, RelayError, tr};
    use crate::error::codes;
    use crate::workspace::ToolError;

    /// Connexion authentifiée : `hello` envoyé et accepté en `timeout`.
    pub(super) fn open(r: &Rendezvous, id: u64, timeout: Duration) -> Result<ControlStream, ControlError> {
        let stream = Endpoint::from_rendezvous(&r.endpoint)?
            .connect(non_zero(timeout))
            .map_err(|e| ControlError::io("connexion à l'application", &e))?;
        stream
            .set_read_timeout(Some(non_zero(timeout)))
            .and_then(|()| stream.set_write_timeout(Some(non_zero(timeout))))
            .map_err(|e| ControlError::io("délai du canal", &e))?;
        let hello = Frame {
            v: PROTOCOL_VERSION,
            id,
            method: Method::Hello,
            params: serde_json::json!({ "token": r.token }),
        };
        // Un envoi refusé n'est pas encore une application partie : un
        // écouteur saturé répond `control.busy` puis ferme, parfois avant
        // d'avoir lu le hello ; sa réponse reste lisible.
        let sent = send(&stream, &hello);
        let mut reader = BufReader::new(&stream);
        loop {
            let line = match read_line_bounded(&mut reader, MAX_LINE_BYTES) {
                Ok(Some(line)) => line,
                Ok(None) => {
                    return Err(match &sent {
                        Err(e) => ControlError::io("envoi du hello", e),
                        Ok(()) => ControlError::Io("connexion fermée".to_owned()),
                    });
                }
                Err(LineError::TooLong) => return Err(ControlError::Io("réponse trop longue".to_owned())),
                Err(LineError::Io(e)) => return Err(ControlError::io("réponse au hello", &e)),
            };
            match serde_json::from_slice::<Reply>(&line) {
                Ok(Reply::Ok { id: got, .. }) if got == id => break,
                Ok(Reply::Err { error, .. }) => return Err(ControlError::Refused(error)),
                // Réponse d'un autre id : ignorée.
                Ok(_) => {}
                Err(e) => return Err(ControlError::Io(format!("réponse illisible : {e}"))),
            }
        }
        // Le lecteur tamponné ne garde rien d'utile : l'application ne
        // parle qu'en réponse à une requête.
        drop(reader);
        Ok(stream)
    }

    /// Appel relayé complet, borné par `deadline` après l'envoi.
    pub(super) fn call(
        client: &RelayClient,
        hello_id: u64,
        frame: &Frame,
        deadline: Duration,
    ) -> Result<Value, RelayError> {
        let lang = client.lang;
        let stream = open(&client.rendezvous, hello_id, client.hello_timeout).map_err(|e| {
            let (code, text) = match &e {
                ControlError::Refused(w) if w.code == super::super::codes::BUSY => (
                    codes::CONTROL_BUSY,
                    tr(
                        lang,
                        "application occupée : réessayer",
                        "application busy: try again",
                    ),
                ),
                ControlError::Refused(w) if w.code == super::super::codes::BAD_TOKEN => (
                    codes::CONTROL_DOCUMENT_CHANGED,
                    tr(
                        lang,
                        "l'application a changé de document : rien n'a été fait, rouvrir le projet voulu",
                        "the application switched documents: nothing was done, reopen the intended project",
                    ),
                ),
                _ => (
                    codes::CONTROL_APP_GONE,
                    tr(lang, "application injoignable", "application unreachable"),
                ),
            };
            RelayError::Control(ToolError::simple(code, format!("{text} ({e})")))
        })?;
        if send(&stream, frame).is_err() {
            return Err(client.gone(
                "l'application a fermé la connexion",
                "the application closed the connection",
            ));
        }
        let start = Instant::now();
        let mut pending = false;
        let mut reader = BufReader::new(&stream);
        loop {
            let left = deadline.saturating_sub(start.elapsed());
            if left.is_zero() {
                return Err(timeout(client, pending));
            }
            if stream.set_read_timeout(Some(non_zero(left))).is_err() {
                return Err(client.gone("canal inutilisable", "unusable channel"));
            }
            let line = match read_line_bounded(&mut reader, MAX_LINE_BYTES) {
                Ok(Some(line)) => line,
                Ok(None) => {
                    return Err(client.gone(
                        "l'application a fermé la connexion",
                        "the application closed the connection",
                    ));
                }
                Err(LineError::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                    return Err(timeout(client, pending));
                }
                Err(LineError::Io(_)) => {
                    return Err(client.gone(
                        "connexion à l'application perdue",
                        "connection to the application lost",
                    ));
                }
                Err(LineError::TooLong) => {
                    return Err(client.gone("réponse trop longue", "reply too long"));
                }
            };
            let Ok(reply) = serde_json::from_slice::<Reply>(&line) else {
                return Err(client.gone("réponse illisible", "unreadable reply"));
            };
            if reply.id() != frame.id {
                // Réponse d'une autre requête : ignorée.
                continue;
            }
            match reply {
                Reply::PendingApproval { .. } => pending = true,
                Reply::Ok { result, .. } => return Ok(result),
                Reply::Err { error, .. } => return Err(RelayError::App(raw_error(&line, &error))),
            }
        }
    }

    /// Erreur JSON telle qu'envoyée (champs au-delà de `code` et
    /// `message` compris), sinon reconstruite.
    fn raw_error(line: &[u8], error: &super::super::WireError) -> Value {
        serde_json::from_slice::<Value>(line)
            .ok()
            .and_then(|v| v.get("err")?.get("error").cloned())
            .filter(Value::is_object)
            .unwrap_or_else(|| serde_json::json!({ "code": error.code, "message": error.message }))
    }

    /// Délai dépassé : validation attendue (`approval_timeout`, le
    /// rattachement est gardé) ou application muette (`app_gone`).
    fn timeout(client: &RelayClient, pending: bool) -> RelayError {
        if pending {
            RelayError::Control(ToolError::simple(
                codes::CONTROL_APPROVAL_TIMEOUT,
                tr(
                    client.lang,
                    "aucune réponse de l'utilisateur dans le délai : rien n'a été appliqué",
                    "no answer from the user in time: nothing was applied",
                )
                .to_owned(),
            ))
        } else {
            client.gone("l'application ne répond pas", "the application does not answer")
        }
    }

    fn send(mut stream: &ControlStream, frame: &Frame) -> std::io::Result<()> {
        let line = to_line(frame).map_err(std::io::Error::other)?;
        stream.write_all(&line)
    }

    /// Délai d'E/S jamais nul (`set_read_timeout(Some(0))` est refusé).
    fn non_zero(d: Duration) -> Duration {
        d.max(Duration::from_millis(1))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests : un échec doit s'arrêter net")]

    use serde_json::json;

    use super::*;

    #[test]
    fn relay_error_code_and_gone() {
        let app = RelayError::App(json!({ "code": "task.not_found", "message": "x" }));
        assert_eq!(app.code(), "task.not_found");
        assert!(!app.is_app_gone());
        let gone = RelayError::Control(ToolError::simple(codes::CONTROL_APP_GONE, String::new()));
        assert!(gone.is_app_gone());
        assert_eq!(gone.code(), codes::CONTROL_APP_GONE);
    }

    #[test]
    fn relay_single_live_in_missing_dir_is_none() {
        let dir = std::env::temp_dir().join(format!("plannercraft-none-{}", std::process::id()));
        assert!(RelayClient::single_live_in(&dir).is_none());
    }
}
