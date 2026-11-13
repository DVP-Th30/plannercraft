//! Lignes de texte (étape 2) : les mots d'une page regroupés par ligne de
//! base, triés de haut en bas puis de gauche à droite. Entiers seulement.

use unicode_normalization::UnicodeNormalization as _;

use crate::extract::{Cpt, Word};

/// Ligne de texte : mots dont les lignes de base sont proches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextLine {
    /// Ligne de base de référence (celle du premier mot, le plus haut).
    pub baseline: Cpt,
    /// Ligne de base la plus basse de la ligne.
    pub bottom: Cpt,
    /// Plus grand corps de la ligne.
    pub size: Cpt,
    /// Mots, de gauche à droite.
    pub words: Vec<Word>,
}

impl TextLine {
    /// Haut de la ligne (ligne de base moins 80 % du corps).
    #[must_use]
    pub fn top(&self) -> Cpt {
        self.baseline.saturating_sub(self.size.saturating_mul(4) / 5)
    }

    /// Mots dont le centre est dans `[x0, x1)`.
    pub fn words_in(&self, x0: Cpt, x1: Cpt) -> impl Iterator<Item = &Word> {
        self.words.iter().filter(move |w| {
            let center = mid(w.x0, w.x1);
            center >= x0 && center < x1
        })
    }
}

/// Milieu de deux coordonnées, sans débordement.
#[must_use]
pub fn mid(a: Cpt, b: Cpt) -> Cpt {
    let sum = i64::from(a).saturating_add(i64::from(b));
    Cpt::try_from(sum / 2).unwrap_or(a)
}

/// Regroupe les mots en lignes : tri par ligne de base, puis un mot rejoint
/// la ligne courante si sa ligne de base est à moins de 45 % du plus grand
/// corps de la première ligne de base de la ligne (une cellule « N° » en
/// petit corps et le nom en corps plus grand restent sur la même ligne ;
/// deux lignes successives d'un nom replié, espacées d'au moins un corps,
/// sont séparées).
#[must_use]
pub fn group_lines(words: &[Word]) -> Vec<TextLine> {
    let mut sorted: Vec<&Word> = words.iter().filter(|w| !w.text.trim().is_empty()).collect();
    sorted.sort_by_key(|w| (w.baseline, w.x0, w.x1));
    let mut lines: Vec<TextLine> = Vec::new();
    for w in sorted {
        let joins = lines.last().is_some_and(|l| {
            let size = l.size.max(w.size).max(1);
            w.baseline.saturating_sub(l.baseline) < size.saturating_mul(45) / 100
        });
        match lines.last_mut() {
            Some(l) if joins => {
                l.words.push(w.clone());
                l.size = l.size.max(w.size);
                l.bottom = l.bottom.max(w.baseline);
            }
            _ => lines.push(TextLine {
                baseline: w.baseline,
                bottom: w.baseline,
                size: w.size,
                words: vec![w.clone()],
            }),
        }
    }
    for l in &mut lines {
        l.words.sort_by_key(|w| (w.x0, w.baseline, w.x1));
    }
    lines
}

/// Texte replié pour la comparaison : minuscules, accents retirés,
/// apostrophes typographiques ramenées à `'`, « œ » → « oe », espaces
/// multiples réduites, espaces de bord retirées.
#[must_use]
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for c in s.nfd() {
        if unicode_normalization::char::is_combining_mark(c) {
            continue;
        }
        let c = match c {
            '\u{2019}' | '\u{2018}' | '\u{02BC}' => '\'',
            '\u{00A0}' | '\u{202F}' | '\u{2009}' => ' ',
            c => c,
        };
        if c.is_whitespace() {
            space = !out.is_empty();
            continue;
        }
        if space {
            out.push(' ');
            space = false;
        }
        match c {
            'œ' | 'Œ' => out.push_str("oe"),
            'æ' | 'Æ' => out.push_str("ae"),
            c => out.extend(c.to_lowercase()),
        }
    }
    out
}

/// Texte de mots joints par une espace.
#[must_use]
pub fn join_words<'a>(words: impl IntoIterator<Item = &'a Word>) -> String {
    let mut out = String::new();
    for w in words {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(w.text.trim());
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::*;

    fn w(text: &str, x0: Cpt, baseline: Cpt, size: Cpt) -> Word {
        Word {
            text: text.to_owned(),
            x0,
            x1: x0.saturating_add(Cpt::try_from(text.len()).unwrap().saturating_mul(400)),
            baseline,
            size,
            bold: false,
            glyphs: Vec::new(),
        }
    }

    #[test]
    fn small_id_and_larger_name_share_a_line() {
        // Observé (corpus EN) : « 16 » en 6,9 pt, ligne de base 1,86 pt
        // au-dessus du nom en 8,7 pt.
        let words = [
            w("Development", 6510, 14238, 868),
            w("16", 4512, 13068, 691),
            w("Preliminary", 6510, 13254, 868),
            w("17", 4512, 15252, 691),
        ];
        let lines = group_lines(&words);
        assert_eq!(lines.len(), 3);
        assert_eq!(join_words(&lines[0].words), "16 Preliminary");
        assert_eq!(join_words(&lines[1].words), "Development");
        assert_eq!(join_words(&lines[2].words), "17");
    }

    #[test]
    fn fold_removes_accents_and_case() {
        assert_eq!(fold("  Nom de la Tâche "), "nom de la tache");
        assert_eq!(fold("Prédécesseurs"), "predecesseurs");
        assert_eq!(fold("% achevé"), "% acheve");
        assert_eq!(fold("Nom\u{00A0}de  la\ttâche"), "nom de la tache");
        assert_eq!(fold("Œuvre l\u{2019}été"), "oeuvre l'ete");
    }
}
