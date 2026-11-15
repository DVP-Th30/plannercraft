//! Texte brut d'une note RTF (R-289).
//!
//! Observé : les notes d'une tâche sont du RTF (`{\rtf1\ansi…`). Le texte
//! est tiré d'après la spécification RTF publiée par Microsoft : groupes
//! de destination ignorés (`\fonttbl`, `\colortbl`, `\stylesheet`,
//! `\info`, `\*…`), `\par` et `\line` = saut de ligne, `\tab`,
//! `\'hh` (Windows-1252), `\uN` (Unicode, suivi de `\ucN` caractères de
//! repli sautés). Un texte qui ne commence pas par `{\rtf` est rendu tel
//! quel. Sortie bornée à `max_chars` caractères ; `true` si tronquée.

/// Destinations dont le contenu n'est pas du texte.
const SKIPPED: &[&str] = &[
    "fonttbl",
    "colortbl",
    "stylesheet",
    "info",
    "pict",
    "object",
    "header",
    "footer",
    "listtable",
    "listoverridetable",
    "rsidtbl",
    "generator",
    "themedata",
    "datastore",
    "latentstyles",
];

/// Windows-1252, octets 0x80 à 0x9F (le reste = Latin-1).
const CP1252_HIGH: [char; 32] = [
    '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž', '\u{8F}', '\u{90}',
    '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9D}', 'ž', 'Ÿ',
];

fn cp1252(b: u8) -> char {
    if (0x80..0xA0).contains(&b) {
        CP1252_HIGH
            .get(usize::from(b.wrapping_sub(0x80)))
            .copied()
            .unwrap_or('\u{FFFD}')
    } else {
        char::from(b)
    }
}

/// Texte d'une note ; `(texte, tronqué)`.
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "automate de lecture RTF d'un seul tenant, plus lisible ainsi"
)]
pub fn to_text(raw: &[u8], max_chars: usize) -> (String, bool) {
    if !raw.starts_with(b"{\\rtf") {
        let text: String = String::from_utf8_lossy(raw).chars().take(max_chars).collect();
        let truncated = String::from_utf8_lossy(raw).chars().count() > max_chars;
        return (text.trim_end_matches('\0').to_owned(), truncated);
    }
    let mut out = String::new();
    let mut count = 0_usize;
    let mut truncated = false;
    // Pile des groupes : (groupe ignoré, caractères de repli de \u).
    let mut stack: Vec<(bool, u32)> = Vec::new();
    let mut skip = false;
    let mut uc = 1_u32;
    let mut pending_fallback = 0_u32;
    let mut i = 0_usize;
    let mut push = |c: char, out: &mut String, skip: bool| {
        if skip {
            return;
        }
        if count >= max_chars {
            truncated = true;
            return;
        }
        out.push(c);
        count = count.saturating_add(1);
    };
    while let Some(&b) = raw.get(i) {
        i = i.saturating_add(1);
        match b {
            b'{' => {
                if stack.len() < 256 {
                    stack.push((skip, uc));
                }
                // `{\*` : destination facultative, ignorée.
                if raw.get(i) == Some(&b'\\') && raw.get(i.saturating_add(1)) == Some(&b'*') {
                    skip = true;
                }
            }
            b'}' => {
                if let Some((s, u)) = stack.pop() {
                    skip = s;
                    uc = u;
                }
            }
            b'\\' => {
                let Some(&next) = raw.get(i) else { break };
                if next.is_ascii_alphabetic() {
                    let start = i;
                    while raw.get(i).is_some_and(u8::is_ascii_alphabetic) {
                        i = i.saturating_add(1);
                    }
                    let word = std::str::from_utf8(raw.get(start..i).unwrap_or_default()).unwrap_or("");
                    let num_start = i;
                    if raw.get(i) == Some(&b'-') {
                        i = i.saturating_add(1);
                    }
                    while raw.get(i).is_some_and(u8::is_ascii_digit) && i.saturating_sub(num_start) < 12 {
                        i = i.saturating_add(1);
                    }
                    let param: Option<i64> = std::str::from_utf8(raw.get(num_start..i).unwrap_or_default())
                        .ok()
                        .and_then(|s| s.parse().ok());
                    if raw.get(i) == Some(&b' ') {
                        i = i.saturating_add(1);
                    }
                    if pending_fallback > 0 {
                        pending_fallback = pending_fallback.saturating_sub(1);
                        continue;
                    }
                    match word {
                        w if SKIPPED.contains(&w) => skip = true,
                        "par" | "line" => push('\n', &mut out, skip),
                        "tab" => push('\t', &mut out, skip),
                        "endash" => push('\u{2013}', &mut out, skip),
                        "emdash" => push('\u{2014}', &mut out, skip),
                        "lquote" => push('\u{2018}', &mut out, skip),
                        "rquote" => push('\u{2019}', &mut out, skip),
                        "ldblquote" => push('\u{201C}', &mut out, skip),
                        "rdblquote" => push('\u{201D}', &mut out, skip),
                        "bullet" => push('\u{2022}', &mut out, skip),
                        "uc" => uc = param.and_then(|p| u32::try_from(p).ok()).unwrap_or(1).min(8),
                        "u" => {
                            let v = param.unwrap_or(0);
                            // `\u` signé sur 16 bits : −4 064 = U+F020.
                            let v = if v < 0 { v.saturating_add(65_536) } else { v };
                            let c = u32::try_from(v)
                                .ok()
                                .and_then(char::from_u32)
                                .unwrap_or('\u{FFFD}');
                            push(c, &mut out, skip);
                            pending_fallback = uc;
                        }
                        _ => {}
                    }
                } else if next == b'\'' {
                    let hex = raw
                        .get(i.saturating_add(1)..i.saturating_add(3))
                        .unwrap_or_default();
                    i = i.saturating_add(3);
                    if pending_fallback > 0 {
                        pending_fallback = pending_fallback.saturating_sub(1);
                        continue;
                    }
                    if let Some(v) = std::str::from_utf8(hex)
                        .ok()
                        .and_then(|h| u8::from_str_radix(h, 16).ok())
                    {
                        push(cp1252(v), &mut out, skip);
                    }
                } else {
                    i = i.saturating_add(1);
                    if pending_fallback > 0 {
                        pending_fallback = pending_fallback.saturating_sub(1);
                        continue;
                    }
                    match next {
                        b'\\' | b'{' | b'}' => push(char::from(next), &mut out, skip),
                        b'~' => push('\u{A0}', &mut out, skip),
                        _ => {}
                    }
                }
            }
            b'\r' | b'\n' | 0 => {}
            _ => {
                if pending_fallback > 0 {
                    pending_fallback = pending_fallback.saturating_sub(1);
                    continue;
                }
                push(cp1252(b), &mut out, skip);
            }
        }
    }
    let text = out.trim_end_matches(['\n', ' ']).to_owned();
    (text, truncated)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests")]
    use super::to_text;

    #[test]
    fn plain_rtf_note() {
        let raw = br"{\rtf1\ansi\ansicpg1252\deff0{\fonttbl{\f0\fswiss Arial;}}{\colortbl ;\red0\green0\blue0;}\viewkind4\uc1\pard\f0\fs20 Notes Example\par}";
        assert_eq!(to_text(raw, 100), ("Notes Example".to_owned(), false));
    }

    #[test]
    fn escapes_and_unicode() {
        let raw = b"{\\rtf1 caf\\'e9 \\u8364? \\{x\\}\\par fin}";
        assert_eq!(to_text(raw, 100).0, "caf\u{e9} \u{20ac} {x}\nfin");
    }

    #[test]
    fn bounded_and_unbalanced() {
        let raw = br"{\rtf1 abcdef{{{{";
        assert_eq!(to_text(raw, 3), ("abc".to_owned(), true));
    }
}
