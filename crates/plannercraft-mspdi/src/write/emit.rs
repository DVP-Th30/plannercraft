//! Émetteur XML de l'écrivain : pile des éléments ouverts, indentation,
//! fins de ligne, échappement des chaînes (R-131), sortie déterministe.
//!
//! Émetteur propre plutôt que `quick_xml::Writer` : l'échappement exact
//! (`&`, `<`, `>`, `"`, `\r` → `&#13;`), l'élément vide `<Nom/>` sans enfant
//! et les charges brutes des `Extras` (R-123), réécrites telles quelles à
//! leur niveau d'indentation, s'écrivent plus simplement à la main.

use plannercraft_core::{Moment, Subject};

use crate::scalar::civil;

use super::{Indent, Newline, WriteCode, WriteDiagnostic};

/// Élément ouvert : nom, et vrai si sa balise ouvrante est déjà écrite
/// (un premier enfant l'a forcée).
struct Open {
    name: &'static str,
    started: bool,
}

/// Émetteur.
pub(crate) struct Emitter {
    out: Vec<u8>,
    newline: &'static str,
    indent: Indent,
    stack: Vec<Open>,
    /// Chemin courant pour les diagnostics (« Tasks/Task »).
    path: Vec<&'static str>,
    /// Sujet courant pour les diagnostics.
    pub(crate) subject: Option<Subject>,
    pub(crate) diagnostics: Vec<WriteDiagnostic>,
}

impl Emitter {
    pub(crate) fn new(newline: Newline, indent: Indent) -> Self {
        Self {
            out: Vec::with_capacity(64 * 1024),
            newline: match newline {
                Newline::Lf => "\n",
                Newline::Crlf => "\r\n",
            },
            indent,
            stack: Vec::new(),
            path: Vec::new(),
            subject: None,
            diagnostics: Vec::new(),
        }
    }

    /// Octets écrits.
    pub(crate) fn finish(self) -> (Vec<u8>, Vec<WriteDiagnostic>) {
        (self.out, self.diagnostics)
    }

    /// Ajoute du texte brut.
    fn push(&mut self, s: &str) {
        self.out.extend_from_slice(s.as_bytes());
    }

    /// Fin de ligne (rien sans indentation).
    fn line_end(&mut self) {
        if !matches!(self.indent, Indent::None) {
            let nl = self.newline;
            self.push(nl);
        }
    }

    /// Indentation de la profondeur courante (celle d'un enfant de
    /// l'élément ouvert le plus profond).
    fn pad(&mut self) {
        self.pad_at(self.stack.len());
    }

    /// Indentation d'une profondeur donnée.
    fn pad_at(&mut self, depth: usize) {
        match self.indent {
            Indent::Tab => {
                for _ in 0..depth {
                    self.out.push(b'\t');
                }
            }
            Indent::Spaces(n) => {
                let count = depth.saturating_mul(usize::from(n));
                self.out.resize(self.out.len().saturating_add(count), b' ');
            }
            Indent::None => {}
        }
    }

    /// Écrit la balise ouvrante de l'élément ouvert le plus profond si elle
    /// ne l'est pas encore (un enfant arrive). Seul le plus profond peut
    /// être en attente : `open` écrit d'abord la balise de son parent.
    fn start_parent(&mut self) {
        let Some(top) = self.stack.last() else {
            return;
        };
        if top.started {
            return;
        }
        let name = top.name;
        self.pad_at(self.stack.len().saturating_sub(1));
        self.push("<");
        self.push(name);
        self.push(">");
        self.line_end();
        if let Some(top) = self.stack.last_mut() {
            top.started = true;
        }
    }

    /// Déclaration XML, instruction `<?plannercraft-style?>` si `style` est
    /// donné (H-R12, charge déjà vérifiée par `style_pi::writable`), et
    /// balise ouvrante de la racine, avec ses déclarations d'espaces de noms.
    pub(crate) fn root(&mut self, namespaces: &[(String, String)], style: Option<&str>) {
        self.push(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#);
        let nl = self.newline;
        self.push(nl);
        if let Some(payload) = style {
            self.push("<?");
            self.push(crate::style_pi::TARGET);
            self.push(" ");
            self.push(payload);
            self.push("?>");
            self.push(nl);
        }
        self.push(r#"<Project xmlns="http://schemas.microsoft.com/project""#);
        for (prefix, uri) in namespaces {
            self.push(" xmlns:");
            self.push(prefix);
            self.push("=\"");
            let escaped = escape(uri);
            self.push(&escaped);
            self.push("\"");
        }
        self.push(">");
        self.line_end();
        self.stack.push(Open {
            name: "Project",
            started: true,
        });
    }

    /// Ferme la racine.
    pub(crate) fn end_root(&mut self) {
        let _ = self.stack.pop();
        self.push("</Project>");
        let nl = self.newline;
        self.push(nl);
    }

    /// Ouvre un élément structuré ; sa balise n'est écrite qu'au premier
    /// enfant (sinon `<Nom/>` à la fermeture).
    pub(crate) fn open(&mut self, name: &'static str) {
        self.start_parent();
        self.stack.push(Open { name, started: false });
        self.path.push(name);
    }

    /// Ferme l'élément ouvert le plus profond.
    pub(crate) fn close(&mut self) {
        let Some(open) = self.stack.pop() else {
            return;
        };
        let _ = self.path.pop();
        self.pad();
        if open.started {
            self.push("</");
            self.push(open.name);
            self.push(">");
        } else {
            self.push("<");
            self.push(open.name);
            self.push("/>");
        }
        self.line_end();
    }

    /// Chemin de l'élément `name` sous l'élément courant.
    fn path_of(&self, name: &str) -> String {
        let mut p: Vec<&str> = self.path.clone();
        p.push(name);
        p.join("/")
    }

    /// Diagnostic d'écriture sur l'élément `name`.
    pub(crate) fn report(&mut self, code: WriteCode, name: &str, value: Option<String>) {
        let element = self.path_of(name);
        self.diagnostics.push(WriteDiagnostic {
            code,
            subject: self.subject,
            element,
            value,
        });
    }

    /// Élément feuille au texte déjà sûr (nombre, date, code) ; vide :
    /// `<Nom/>`.
    pub(crate) fn leaf(&mut self, name: &'static str, value: &str) {
        self.start_parent();
        self.pad();
        self.push("<");
        self.push(name);
        if value.is_empty() {
            self.push("/>");
        } else {
            self.push(">");
            self.push(value);
            self.push("</");
            self.push(name);
            self.push(">");
        }
        self.line_end();
    }

    /// Ouvre une feuille : indentation et balise ouvrante.
    fn leaf_open(&mut self, name: &'static str) {
        self.start_parent();
        self.pad();
        self.out.push(b'<');
        self.push(name);
        self.out.push(b'>');
    }

    /// Ferme une feuille ouverte par `leaf_open`.
    fn leaf_close(&mut self, name: &'static str) {
        self.push("</");
        self.push(name);
        self.out.push(b'>');
        self.line_end();
    }

    /// Feuille entière, sans allocation.
    pub(crate) fn leaf_int(&mut self, name: &'static str, v: i64) {
        self.leaf_open(name);
        push_int(&mut self.out, v, 1);
        self.leaf_close(name);
    }

    /// Feuille entière facultative.
    pub(crate) fn opt_int(&mut self, name: &'static str, v: Option<i64>) {
        if let Some(v) = v {
            self.leaf_int(name, v);
        }
    }

    /// Feuille date `AAAA-MM-JJThh:mm:00` (R-120), sans allocation.
    pub(crate) fn leaf_date(&mut self, name: &'static str, m: Moment) {
        let Some((y, mo, d, h, mi)) = civil(m.minutes()) else {
            self.leaf(name, &crate::scalar::format_datetime(m));
            return;
        };
        self.leaf_open(name);
        push_int(&mut self.out, y, 4);
        self.out.push(b'-');
        push_int(&mut self.out, mo, 2);
        self.out.push(b'-');
        push_int(&mut self.out, d, 2);
        self.out.push(b'T');
        push_int(&mut self.out, h, 2);
        self.out.push(b':');
        push_int(&mut self.out, mi, 2);
        self.push(":00");
        self.leaf_close(name);
    }

    /// Feuille date facultative.
    pub(crate) fn opt_date(&mut self, name: &'static str, m: Option<Moment>) {
        if let Some(m) = m {
            self.leaf_date(name, m);
        }
    }

    /// Feuille durée ISO `PT{h}H{m}M0S` (R-120), sans allocation.
    pub(crate) fn leaf_iso(&mut self, name: &'static str, minutes: i64) {
        self.leaf_open(name);
        if minutes < 0 {
            self.out.push(b'-');
        }
        let abs = minutes.unsigned_abs();
        self.push("PT");
        push_int(&mut self.out, i64::try_from(abs / 60).unwrap_or(i64::MAX), 1);
        self.out.push(b'H');
        push_int(&mut self.out, i64::try_from(abs % 60).unwrap_or(0), 1);
        self.push("M0S");
        self.leaf_close(name);
    }

    /// Élément feuille facultatif : écrit seulement s'il a une valeur.
    pub(crate) fn opt_leaf(&mut self, name: &'static str, value: Option<String>) {
        if let Some(v) = value {
            self.leaf(name, &v);
        }
    }

    /// Élément feuille de texte libre (R-131) : caractères interdits par
    /// XML 1.0 remplacés par U+FFFD (`mspdi.write.invalid_char`), troncature
    /// à `max_len` caractères (`mspdi.write.truncated`), échappement.
    pub(crate) fn text(&mut self, name: &'static str, value: &str, max_len: Option<u16>) {
        let (clean, replaced) = sanitize(value);
        if replaced {
            self.report(
                WriteCode::InvalidChar,
                name,
                Some(value.chars().take(200).collect()),
            );
        }
        let clean = match max_len {
            Some(max) if clean.chars().count() > usize::from(max) => {
                self.report(
                    WriteCode::Truncated { max: u32::from(max) },
                    name,
                    Some(clean.chars().take(200).collect()),
                );
                clean.chars().take(usize::from(max)).collect()
            }
            _ => clean,
        };
        let escaped = escape(&clean);
        self.leaf(name, &escaped);
    }

    /// Charge brute d'un `Extras` (déjà validée), à la profondeur courante.
    pub(crate) fn raw(&mut self, payload: &str) {
        self.start_parent();
        self.pad();
        self.push(payload);
        self.line_end();
    }
}

/// Écrit un entier en décimal, complété de zéros à gauche jusqu'à `width`
/// chiffres.
fn push_int(out: &mut Vec<u8>, v: i64, width: usize) {
    let mut digits = [0_u8; 20];
    let mut n = v.unsigned_abs();
    let mut len = 0_usize;
    loop {
        if let Some(slot) = digits.get_mut(len) {
            *slot = b'0'.saturating_add(u8::try_from(n % 10).unwrap_or(0));
        }
        len = len.saturating_add(1);
        n /= 10;
        if n == 0 || len >= digits.len() {
            break;
        }
    }
    if v < 0 {
        out.push(b'-');
    }
    for _ in len..width {
        out.push(b'0');
    }
    for i in (0..len).rev() {
        if let Some(d) = digits.get(i) {
            out.push(*d);
        }
    }
}

/// Caractère admis par XML 1.0 (`Char`).
pub(crate) const fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
}

/// Remplace les caractères interdits par XML 1.0 par U+FFFD ; vrai si un
/// remplacement a eu lieu.
pub(crate) fn sanitize(s: &str) -> (String, bool) {
    if s.chars().all(is_xml_char) {
        return (s.to_owned(), false);
    }
    (
        s.chars()
            .map(|c| if is_xml_char(c) { c } else { '\u{FFFD}' })
            .collect(),
        true,
    )
}

/// R-131 : échappe `&`, `<`, `>`, `"` et `\r` (`&#13;`, sinon normalisé en
/// LF à la relecture).
pub(crate) fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\r' => out.push_str("&#13;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{Emitter, escape, sanitize};
    use crate::write::{Indent, Newline};

    #[test]
    fn r131_escape_and_sanitize() {
        assert_eq!(
            escape("A & B <x> \"q\" l1\r\nl2"),
            "A &amp; B &lt;x&gt; &quot;q&quot; l1&#13;\nl2"
        );
        assert_eq!(sanitize("a\u{1}b"), ("a\u{FFFD}b".to_owned(), true));
        assert_eq!(sanitize("a\tb\n"), ("a\tb\n".to_owned(), false));
    }

    #[test]
    fn r120_empty_elements_and_indentation() {
        let mut e = Emitter::new(Newline::Lf, Indent::Tab);
        e.root(&[], None);
        e.open("Tasks");
        e.close();
        e.open("Calendars");
        e.leaf("UID", "1");
        e.close();
        e.text("Name", "", None);
        e.end_root();
        let (out, _) = e.finish();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
             <Project xmlns=\"http://schemas.microsoft.com/project\">\n\
             \t<Tasks/>\n\t<Calendars>\n\t\t<UID>1</UID>\n\t</Calendars>\n\t<Name/>\n</Project>\n"
        );
    }
}
