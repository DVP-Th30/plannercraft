//! Largeurs de texte en 1/16 px, tirées de la table d'Inter (H-R11).
//!
//! Remplace l'estimation de H-R1 pour le SVG, le PDF et l'écran stylé :
//! chaque caractère prend l'avance de sa graisse, convertie en 1/16 px par
//! arrondi inférieur (`avance × taille16 / cadratin`, entiers `i64`), sans
//! crénage ni façonnage ; un caractère absent de la table vaut un cadratin.
//! Les glyphes du PDF sont placés aux mêmes positions (H-R13).

use crate::metrics_inter::{REGULAR, SEMIBOLD, UNITS_PER_EM};

/// Graisse d'Inter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FontWeight {
    /// Inter Regular (texte courant).
    Regular,
    /// Inter `SemiBold` (récapitulatives, en-têtes).
    SemiBold,
}

/// Police d'un texte : graisse et taille en 1/16 px.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FontRef {
    /// Graisse.
    pub weight: FontWeight,
    /// Taille du corps en 1/16 px (12 px = 192).
    pub size16: u16,
}

/// Mesure d'un texte (H-R11). Implémentée par [`InterMetrics`] ; un test
/// peut fournir la sienne.
pub trait TextMeasure {
    /// Largeur de `s` dans la police `f`, en 1/16 px.
    fn width16(&self, s: &str, f: FontRef) -> i64;
}

/// Mesure par la table d'Inter générée (`cargo xtask fonts`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InterMetrics;

impl InterMetrics {
    /// Avance d'un caractère en unités de police ; un cadratin s'il est
    /// absent de la table.
    #[must_use]
    pub fn advance_units(ch: char, weight: FontWeight) -> u16 {
        let table = match weight {
            FontWeight::Regular => REGULAR,
            FontWeight::SemiBold => SEMIBOLD,
        };
        table
            .binary_search_by_key(&ch, |&(c, _)| c)
            .ok()
            .and_then(|i| table.get(i))
            .map_or(UNITS_PER_EM, |&(_, advance)| advance)
    }

    /// Avance d'un caractère en 1/16 px, arrondie vers le bas.
    #[must_use]
    pub fn advance16(ch: char, f: FontRef) -> i64 {
        let units = i64::from(Self::advance_units(ch, f.weight));
        // Au plus 65 535 × 65 535 : aucun débordement en i64.
        units
            .saturating_mul(i64::from(f.size16))
            .checked_div(i64::from(UNITS_PER_EM))
            .unwrap_or(0)
    }

    /// Cadratin des polices, en unités de police.
    #[must_use]
    pub const fn units_per_em() -> u16 {
        UNITS_PER_EM
    }
}

impl TextMeasure for InterMetrics {
    fn width16(&self, s: &str, f: FontRef) -> i64 {
        s.chars()
            .fold(0_i64, |acc, ch| acc.saturating_add(Self::advance16(ch, f)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: FontRef = FontRef {
        weight: FontWeight::Regular,
        size16: 12 * 16,
    };

    /// Exemple de H-R11 : « Murs » en 12 px = 173 + 113 + 72 + 101 = 459
    /// (28,69 px).
    #[test]
    fn h_r11_murs_width() {
        assert_eq!(InterMetrics.width16("Murs", BODY), 459);
        assert_eq!(InterMetrics::advance16('M', BODY), 173);
    }

    #[test]
    fn h_r11_missing_char_is_one_em() {
        // Idéogramme absent du sous-ensemble : un cadratin, 12 px.
        assert_eq!(InterMetrics.width16("中", BODY), 12 * 16);
        assert_eq!(InterMetrics.width16("", BODY), 0);
    }

    #[test]
    fn h_r11_semibold_wider_than_regular() {
        let bold = FontRef {
            weight: FontWeight::SemiBold,
            ..BODY
        };
        assert!(InterMetrics.width16("Gros œuvre", bold) > InterMetrics.width16("Gros œuvre", BODY));
    }

    #[test]
    fn h_r11_huge_inputs_saturate() {
        let huge = FontRef {
            weight: FontWeight::Regular,
            size16: u16::MAX,
        };
        assert!(InterMetrics.width16(&"W".repeat(10_000), huge) > 0);
    }
}
