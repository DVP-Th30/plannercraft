//! Lecteur SSE (`text/event-stream`) tolérant, écrit à la main.
//!
//! Suit la spécification WHATWG : lignes terminées par `\n`, `\r\n` ou `\r`
//! (y compris coupées entre deux morceaux), lignes de commentaire `:`
//! ignorées (OpenRouter envoie `: OPENROUTER PROCESSING` pour garder la
//! connexion ouverte), champ `data` (un espace facultatif après `:`),
//! plusieurs lignes `data` d'un même évènement jointes par `\n`, évènement
//! émis à la ligne vide. Les autres champs (`event`, `id`, `retry`) sont
//! ignorés : l'API Chat Completions n'en a pas l'usage.
//!
//! Règle 10 : une ligne de plus de [`MAX_LINE_BYTES`] octets est une
//! erreur, pas une allocation sans fin ; un texte non UTF-8 est remplacé
//! (U+FFFD) puis refusé par le lecteur JSON en aval.

/// Longueur au plus d'une ligne (et d'un évènement).
pub const MAX_LINE_BYTES: usize = 1024 * 1024;

/// Évènement SSE : contenu de ses lignes `data`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SseEvent {
    /// Lignes `data` jointes par `\n`.
    pub data: String,
}

/// Erreur du lecteur.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SseError {
    /// Ligne ou évènement trop long.
    TooLong,
}

/// Lecteur incrémental.
#[derive(Debug, Default)]
pub struct SseParser {
    /// Octets reçus, pas encore découpés en lignes.
    pending: Vec<u8>,
    /// Lignes `data` de l'évènement en cours.
    data: Option<String>,
}

impl SseParser {
    /// Lecteur vide.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Lit `bytes` et rend les évènements complets.
    ///
    /// # Errors
    ///
    /// [`SseError::TooLong`] si une ligne ou un évènement dépasse
    /// [`MAX_LINE_BYTES`] ; le lecteur est alors vidé.
    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<SseEvent>, SseError> {
        self.pending.extend_from_slice(bytes);
        let mut events = Vec::new();
        while let Some((line_end, next)) = line_break(&self.pending) {
            let line: Vec<u8> = self.pending.drain(..next).take(line_end).collect();
            if let Some(event) = self.line(&line)? {
                events.push(event);
            }
        }
        if self.pending.len() > MAX_LINE_BYTES {
            self.reset();
            return Err(SseError::TooLong);
        }
        Ok(events)
    }

    /// Fin du flux : la dernière ligne sans fin de ligne est lue, et un
    /// évènement non terminé par une ligne vide est rendu (tolérance).
    ///
    /// # Errors
    ///
    /// [`SseError::TooLong`] comme [`SseParser::feed`].
    pub fn finish(&mut self) -> Result<Option<SseEvent>, SseError> {
        let rest = std::mem::take(&mut self.pending);
        // Un `\r` final attendait peut-être son `\n` : c'est une fin de ligne.
        let rest = rest.strip_suffix(b"\r").unwrap_or(&rest).to_vec();
        if !rest.is_empty()
            && let Some(event) = self.line(&rest)?
        {
            return Ok(Some(event));
        }
        Ok(self.data.take().map(|data| SseEvent { data }))
    }

    fn reset(&mut self) {
        self.pending.clear();
        self.data = None;
    }

    /// Une ligne complète (sans sa fin de ligne).
    fn line(&mut self, line: &[u8]) -> Result<Option<SseEvent>, SseError> {
        if line.is_empty() {
            return Ok(self.data.take().map(|data| SseEvent { data }));
        }
        if line.first() == Some(&b':') {
            return Ok(None);
        }
        let (field, value) = match line.iter().position(|b| *b == b':') {
            Some(colon) => {
                let (field, rest) = line.split_at(colon);
                let value = rest.get(1..).unwrap_or_default();
                (field, value.strip_prefix(b" ").unwrap_or(value))
            }
            None => (line, &[][..]),
        };
        if field != b"data" {
            return Ok(None);
        }
        let value = String::from_utf8_lossy(value);
        let data = self.data.get_or_insert_with(String::new);
        let grown = data.len().saturating_add(value.len()).saturating_add(1);
        if grown > MAX_LINE_BYTES {
            self.reset();
            return Err(SseError::TooLong);
        }
        if !data.is_empty() {
            data.push('\n');
        }
        data.push_str(&value);
        Ok(None)
    }
}

/// Première fin de ligne de `buf` : (longueur de la ligne, début de la
/// suivante). Un `\r` final reste en attente (son `\n` peut suivre).
fn line_break(buf: &[u8]) -> Option<(usize, usize)> {
    let pos = buf.iter().position(|b| *b == b'\n' || *b == b'\r')?;
    match buf.get(pos) {
        Some(b'\n') => Some((pos, pos.checked_add(1)?)),
        _ => match buf.get(pos.checked_add(1)?) {
            Some(b'\n') => Some((pos, pos.checked_add(2)?)),
            Some(_) => Some((pos, pos.checked_add(1)?)),
            None => None,
        },
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

    fn all(parser: &mut SseParser, chunks: &[&[u8]]) -> Vec<String> {
        let mut out = Vec::new();
        for chunk in chunks {
            out.extend(parser.feed(chunk).unwrap().into_iter().map(|e| e.data));
        }
        out.extend(parser.finish().unwrap().map(|e| e.data));
        out
    }

    #[test]
    fn comments_ignored_and_done_kept() {
        let mut p = SseParser::new();
        let got = all(
            &mut p,
            &[
                b": OPENROUTER PROCESSING\n\n",
                b"data: {\"a\":1}\n\n",
                b"data: [DONE]\n\n",
            ],
        );
        assert_eq!(got, ["{\"a\":1}", "[DONE]"]);
    }

    #[test]
    fn line_split_across_chunks_and_crlf() {
        let mut p = SseParser::new();
        let got = all(&mut p, &[b"da", b"ta: x", b"y\r", b"\n\r", b"\ndata:z\r\r"]);
        assert_eq!(got, ["xy", "z"]);
    }

    #[test]
    fn multi_line_data_joined() {
        let mut p = SseParser::new();
        let got = all(&mut p, &[b"data: a\ndata: b\nevent: x\nid: 3\n\n"]);
        assert_eq!(got, ["a\nb"]);
    }

    #[test]
    fn unterminated_event_flushed_at_finish() {
        let mut p = SseParser::new();
        let got = all(&mut p, &[b"data: last"]);
        assert_eq!(got, ["last"]);
    }

    #[test]
    fn too_long_line_is_error_not_growth() {
        let mut p = SseParser::new();
        let big = vec![b'a'; MAX_LINE_BYTES + 1];
        assert_eq!(p.feed(&big), Err(SseError::TooLong));
        // Le lecteur repart proprement.
        assert_eq!(all(&mut p, &[b"data: ok\n\n"]), ["ok"]);
    }

    #[test]
    fn invalid_utf8_replaced() {
        let mut p = SseParser::new();
        let got = all(&mut p, &[b"data: \xff\n\n"]);
        assert_eq!(got, ["\u{fffd}"]);
    }

    #[test]
    fn byte_by_byte_equals_whole() {
        let whole: &[u8] = b": c\r\ndata: {\"x\":\"\xc3\xa9\"}\r\n\r\ndata: [DONE]\n\n";
        let mut p = SseParser::new();
        let expected = all(&mut p, &[whole]);
        let mut p = SseParser::new();
        let chunks: Vec<&[u8]> = whole.chunks(1).collect();
        assert_eq!(all(&mut p, &chunks), expected);
    }
}
