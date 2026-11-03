//! Trames JSON par lignes du canal (≤ 8 Mio par ligne ; lot L10 de J4,
//! méthodes du relais J7 au lot L3).
//!
//! Une requête [`Frame`] ou une réponse [`Reply`] tient sur une ligne
//! terminée par `\n` (un `\r` final est toléré en lecture). Une ligne plus
//! longue que [`MAX_LINE_BYTES`] est refusée sans être lue en entier : le
//! lecteur [`read_line_bounded`] ne garde jamais plus que la borne en mémoire.

use std::io::{self, BufRead};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::workspace::ToolError;

/// Version du protocole.
pub const PROTOCOL_VERSION: u32 = 1;

/// Taille maximale d'une ligne, sans le `\n` (8 Mio).
pub const MAX_LINE_BYTES: usize = 8 * 1024 * 1024;

/// Codes d'erreur stables du canal (champ `code` de [`WireError`]).
pub mod codes {
    /// Ligne plus longue que [`super::MAX_LINE_BYTES`].
    pub const LINE_TOO_LONG: &str = "control.line_too_long";
    /// Trame illisible (JSON invalide, champ inconnu ou manquant).
    pub const INVALID_FRAME: &str = "control.invalid_frame";
    /// Version de protocole non prise en charge.
    pub const UNSUPPORTED_VERSION: &str = "control.unsupported_version";
    /// Jeton absent ou faux dans `hello`.
    pub const BAD_TOKEN: &str = "control.bad_token";
    /// Requête avant un `hello` accepté.
    pub const UNAUTHENTICATED: &str = "control.unauthenticated";
    /// Méthode ou lecture non prise en charge par cet écouteur.
    pub const UNSUPPORTED: &str = "control.unsupported";
    /// File des appels de l'application pleine (J7) : réessayer ; la
    /// connexion est maintenue.
    pub const BUSY: &str = "control.busy";
    /// Erreur interne de l'application (sérialisation).
    pub const INTERNAL: &str = "control.internal";
}

/// Requête.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    /// Version du protocole (1).
    pub v: u32,
    /// Identifiant de la requête.
    pub id: u64,
    /// Méthode.
    pub method: Method,
    /// Paramètres (`null` si absents).
    #[serde(default)]
    pub params: Value,
}

/// Méthode. Les méthodes `undo`, `redo` et `history` du contrat J4 sont
/// retirées en J7 : `execute { tool: "history.undo" }` (un seul chemin
/// d'exécution, ADR-0015).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    /// Présentation avec le jeton.
    Hello,
    /// Lecture (`{ command, params }`, genre `Read` ou `Analysis`).
    Query,
    /// Outil du registre (`{ tool, params }`).
    Execute,
    /// État de l'interface (`app.ui_state`).
    UiState,
    /// Capture (`app.screenshot`, `{ scale_down }`).
    Screenshot,
}

impl Method {
    /// Toutes les méthodes, dans l'ordre de déclaration.
    pub const ALL: [Self; 5] = [
        Self::Hello,
        Self::Query,
        Self::Execute,
        Self::UiState,
        Self::Screenshot,
    ];

    /// Nom sur le fil (`hello`, `ui_state`…).
    #[must_use]
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Hello => "hello",
            Self::Query => "query",
            Self::Execute => "execute",
            Self::UiState => "ui_state",
            Self::Screenshot => "screenshot",
        }
    }

    /// Méthode servie par l'écouteur J4 (`hello`, `query`).
    #[must_use]
    pub const fn available_in_j4(self) -> bool {
        matches!(self, Self::Hello | Self::Query)
    }
}

/// Réponse.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reply {
    /// Succès.
    Ok {
        /// Requête.
        id: u64,
        /// Résultat.
        result: Value,
    },
    /// Échec.
    Err {
        /// Requête.
        id: u64,
        /// Erreur.
        error: WireError,
    },
    /// Écriture en attente de la validation de l'utilisateur (J7,
    /// C-CTL-02) : la réponse finale suit sur la même connexion, même `id`.
    PendingApproval {
        /// Requête.
        id: u64,
    },
}

impl Reply {
    /// Réponse d'erreur.
    #[must_use]
    pub fn error(id: u64, code: &str, message: impl Into<String>) -> Self {
        Self::Err {
            id,
            error: WireError {
                code: code.to_owned(),
                message: message.into(),
                details: Map::new(),
            },
        }
    }

    /// Réponse d'erreur d'un outil refusé : tous ses champs (`op_index`,
    /// `field`, `subject`, `diagnostics`), comme l'erreur structurée du
    /// serveur MCP local (C-CTL-09).
    #[must_use]
    pub fn tool_error(id: u64, error: &ToolError) -> Self {
        Self::Err {
            id,
            error: WireError::from_tool(error),
        }
    }

    /// Identifiant de la requête à laquelle la réponse se rapporte.
    #[must_use]
    pub const fn id(&self) -> u64 {
        match self {
            Self::Ok { id, .. } | Self::Err { id, .. } | Self::PendingApproval { id } => *id,
        }
    }
}

/// Erreur transmise sur le canal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireError {
    /// Code stable ([`codes`], ou celui de la commande refusée).
    pub code: String,
    /// Message.
    pub message: String,
    /// Autres champs de l'erreur d'outil (`op_index`, `field`, `subject`,
    /// `diagnostics`), à plat ; vide pour une erreur du canal.
    #[serde(flatten)]
    pub details: Map<String, Value>,
}

impl WireError {
    /// Erreur du fil d'une erreur d'outil, champs compris.
    #[must_use]
    pub fn from_tool(error: &ToolError) -> Self {
        let mut details = match serde_json::to_value(error) {
            Ok(Value::Object(map)) => map,
            _ => Map::new(),
        };
        details.remove("code");
        details.remove("message");
        Self {
            code: error.code.to_owned(),
            message: error.message.clone(),
            details,
        }
    }
}

/// Échec de lecture d'une ligne.
#[derive(Debug)]
pub enum LineError {
    /// Ligne plus longue que la borne ; le reste n'est pas lu.
    TooLong,
    /// Entrée/sortie (délai dépassé compris).
    Io(io::Error),
}

/// Lit une ligne d'au plus `max` octets (sans le `\n` ni un `\r` final).
///
/// `Ok(None)` en fin de flux sans octet lu ; une dernière ligne sans `\n`
/// est rendue telle quelle. La mémoire retenue ne dépasse jamais `max`.
///
/// # Errors
///
/// [`LineError::TooLong`] dès que la ligne dépasse `max` ;
/// [`LineError::Io`] sur une erreur du flux.
pub fn read_line_bounded<R: BufRead>(reader: &mut R, max: usize) -> Result<Option<Vec<u8>>, LineError> {
    read_line(reader, max, &mut || Ok(()), false)
}

/// Comme [`read_line_bounded`], `before_fill` appelé avant chaque
/// remplissage du tampon (l'écouteur y pose le délai qui reste avant l'échéance du
/// `hello` : une ligne envoyée octet par octet ne la repousse pas) ; son
/// erreur arrête la lecture. Un délai de lecture écoulé (`WouldBlock`,
/// `TimedOut`) n'arrête pas la lecture : `before_fill` est rappelé et
/// décide (échéance atteinte, connexion évincée) ; la ligne commencée est
/// gardée. L'écouteur lit ainsi par tranches courtes : sous Windows, la
/// coupure (`shutdown`) d'une connexion par un autre fil ne réveille pas
/// une lecture bloquée (CI Windows de la PR #7).
///
/// # Errors
///
/// Celles de [`read_line_bounded`], et celle de `before_fill`
/// ([`LineError::Io`]).
pub fn read_line_bounded_with<R: BufRead>(
    reader: &mut R,
    max: usize,
    before_fill: &mut dyn FnMut() -> io::Result<()>,
) -> Result<Option<Vec<u8>>, LineError> {
    read_line(reader, max, before_fill, true)
}

fn read_line<R: BufRead>(
    reader: &mut R,
    max: usize,
    before_fill: &mut dyn FnMut() -> io::Result<()>,
    retry_timeouts: bool,
) -> Result<Option<Vec<u8>>, LineError> {
    let mut line = Vec::new();
    loop {
        before_fill().map_err(LineError::Io)?;
        let buf = match reader.fill_buf() {
            Ok(buf) => buf,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e)
                if retry_timeouts
                    && matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) =>
            {
                continue;
            }
            Err(e) => return Err(LineError::Io(e)),
        };
        if buf.is_empty() {
            return Ok(if line.is_empty() {
                None
            } else {
                Some(trim_cr(line))
            });
        }
        let (chunk_len, found) = match buf.iter().position(|&b| b == b'\n') {
            Some(at) => (at, true),
            None => (buf.len(), false),
        };
        if line.len().saturating_add(chunk_len) > max {
            return Err(LineError::TooLong);
        }
        line.extend(buf.iter().take(chunk_len));
        let consumed = if found {
            chunk_len.saturating_add(1)
        } else {
            chunk_len
        };
        reader.consume(consumed);
        if found {
            return Ok(Some(trim_cr(line)));
        }
    }
}

fn trim_cr(mut line: Vec<u8>) -> Vec<u8> {
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    line
}

/// Ligne JSON d'une valeur, terminée par `\n`.
///
/// # Errors
///
/// Erreur de sérialisation de `serde_json`.
pub fn to_line<T: Serialize>(value: &T) -> Result<Vec<u8>, serde_json::Error> {
    let mut out = serde_json::to_vec(value)?;
    out.push(b'\n');
    Ok(out)
}
