//! Décodage des octets et détection du séparateur (R-144).

use encoding_rs::{UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1252};

use super::{CsvEncoding, CsvError};

/// Texte décodé : contenu, encodage retenu, présence de séquences
/// invalides remplacées par U+FFFD.
pub(crate) struct Decoded {
    pub(crate) text: String,
    pub(crate) encoding: CsvEncoding,
    pub(crate) malformed: bool,
}

/// Encodage détecté : BOM UTF-8 (retiré) ou UTF-16 (FF FE, FE FF) ; sinon
/// UTF-8 s'il est valide ; sinon Windows-1252.
fn detect(bytes: &[u8]) -> CsvEncoding {
    match bytes {
        [0xEF, 0xBB, 0xBF, ..] => CsvEncoding::Utf8,
        [0xFF, 0xFE, ..] => CsvEncoding::Utf16Le,
        [0xFE, 0xFF, ..] => CsvEncoding::Utf16Be,
        _ if std::str::from_utf8(bytes).is_ok() => CsvEncoding::Utf8,
        _ => CsvEncoding::Windows1252,
    }
}

/// R-144 : décode les octets avec l'encodage imposé ou détecté (BOM retiré
/// s'il correspond). Une séquence invalide est remplacée par U+FFFD.
///
/// # Errors
///
/// [`CsvError::Encoding`] si l'encodage est imposé et que les octets n'y
/// sont pas valides (mauvais choix de l'appelant : refus plutôt qu'un texte
/// faux).
pub(crate) fn decode(bytes: &[u8], forced: Option<CsvEncoding>) -> Result<Decoded, CsvError> {
    let encoding = forced.unwrap_or_else(|| detect(bytes));
    let codec = match encoding {
        CsvEncoding::Utf8 => UTF_8,
        CsvEncoding::Utf16Le => UTF_16LE,
        CsvEncoding::Utf16Be => UTF_16BE,
        CsvEncoding::Windows1252 => WINDOWS_1252,
    };
    let (text, malformed) = match encoding {
        CsvEncoding::Windows1252 => codec.decode_without_bom_handling(bytes),
        _ => codec.decode_with_bom_removal(bytes),
    };
    if malformed && forced.is_some() {
        return Err(CsvError::Encoding);
    }
    Ok(Decoded {
        text: text.into_owned(),
        encoding,
        malformed,
    })
}

/// Début utile du texte (R-144) : une ligne Excel `sep=X` en tête (après
/// d'éventuelles lignes vides) fixe le séparateur et n'est pas lue comme un
/// enregistrement.
pub(crate) struct Preamble {
    /// Séparateur fixé par la ligne `sep=X`.
    pub(crate) separator: Option<u8>,
    /// Octet où commence la lecture (après la ligne `sep=X`, sinon 0).
    pub(crate) body_start: usize,
    /// Ligne physique (1 = première) de `body_start`.
    pub(crate) body_line: u64,
}

/// Vrai si la ligne n'a que des espaces (ou rien).
fn blank_line(line: &str) -> bool {
    line.chars().all(char::is_whitespace)
}

/// Lignes physiques du texte (fins de ligne `\n`, `\r\n`, `\r` seul) avec
/// l'octet qui suit chacune (début de la ligne suivante).
fn physical_lines(text: &str) -> impl Iterator<Item = (&str, usize)> {
    let bytes = text.as_bytes();
    let mut start = 0_usize;
    std::iter::from_fn(move || {
        if start >= bytes.len() {
            return None;
        }
        let rest = bytes.get(start..)?;
        let end = rest
            .iter()
            .position(|b| matches!(b, b'\n' | b'\r'))
            .map_or(bytes.len(), |p| start.saturating_add(p));
        let next = match (bytes.get(end), bytes.get(end.saturating_add(1))) {
            (Some(b'\r'), Some(b'\n')) => end.saturating_add(2),
            (Some(_), _) => end.saturating_add(1),
            (None, _) => end,
        };
        let line = text.get(start..end).unwrap_or("");
        start = next;
        Some((line, next))
    })
}

/// R-144 : lignes vides de tête sautées ; si la première ligne non vide est
/// `sep=X` (casse ignorée, espaces de bord tolérées, `X` un caractère ASCII
/// autre que le guillemet), elle fixe le séparateur et la lecture commence
/// à la ligne suivante.
pub(crate) fn preamble(text: &str) -> Preamble {
    let none = Preamble {
        separator: None,
        body_start: 0,
        body_line: 1,
    };
    let mut line_no: u64 = 0;
    for (line, next) in physical_lines(text) {
        line_no = line_no.saturating_add(1);
        if blank_line(line) {
            continue;
        }
        // Espaces seules retirées : `sep=\t` garde sa tabulation.
        let trimmed = line.trim_matches(' ');
        if !trimmed.get(..4).is_some_and(|h| h.eq_ignore_ascii_case("sep=")) {
            return none;
        }
        let sep = match trimmed.get(4..).map(str::as_bytes) {
            Some([b]) if b.is_ascii() && !matches!(b, b'"' | b'\n' | b'\r') => *b,
            _ => return none,
        };
        return Preamble {
            separator: Some(sep),
            body_start: next,
            body_line: line_no.saturating_add(1),
        };
    }
    none
}

/// R-144 : séparateur de la première ligne non vide (l'en-tête), hors
/// guillemets : le plus fréquent de `;`, tabulation, `,` ; à égalité `;`
/// puis tabulation puis `,` ; aucun : `;`.
pub(crate) fn detect_separator(text: &str) -> u8 {
    let start = physical_lines(text)
        .scan(0_usize, |begin, (line, next)| {
            let here = *begin;
            *begin = next;
            Some((here, blank_line(line)))
        })
        .find(|(_, blank)| !blank)
        .map_or(0, |(here, _)| here);
    let text = text.get(start..).unwrap_or("");
    let mut counts = [(b';', 0_u32), (b'\t', 0), (b',', 0)];
    let mut quoted = false;
    for c in text.chars() {
        match c {
            '"' => quoted = !quoted,
            '\n' | '\r' if !quoted => break,
            _ if !quoted => {
                for (sep, n) in &mut counts {
                    if u32::from(*sep) == u32::from(c) {
                        *n = n.saturating_add(1);
                    }
                }
            }
            _ => {}
        }
    }
    let mut best = (b';', 0_u32);
    for (sep, n) in counts {
        if n > best.1 {
            best = (sep, n);
        }
    }
    best.0
}

#[cfg(test)]
mod tests {
    use super::{detect_separator, preamble};

    /// R-144 : lignes vides de tête et ligne Excel `sep=X`.
    #[test]
    fn r144_leading_blank_lines_and_sep_line() {
        assert_eq!(detect_separator("\n\r\n  \nName,Duration\n"), b',');
        assert_eq!(detect_separator("a;b\nx,y,z\n"), b';');
        let p = preamble("\r\nSEP=,\r\nN°;Nom\n");
        assert_eq!((p.separator, p.body_start, p.body_line), (Some(b','), 9, 3));
        let p = preamble("sep=\t\nN°\tNom");
        assert_eq!(p.separator, Some(b'\t'));
        for text in ["N°;Nom\n", "sep=;;\n", "sep=\n", "x\nsep=,\n", ""] {
            let p = preamble(text);
            assert_eq!((p.separator, p.body_start, p.body_line), (None, 0, 1), "{text:?}");
        }
    }
}
