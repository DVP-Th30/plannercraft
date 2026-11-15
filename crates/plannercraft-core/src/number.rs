//! Nombres décimaux des contrats sérialisés (ADR-0039 D5, lot B2) : une
//! valeur entière en point fixe (dixièmes ou millièmes de minute) écrite en
//! minutes, sans perte et sans calcul flottant.
//!
//! - **Écriture** : un entier de minutes s'écrit comme un entier (jamais
//!   `480.0`) ; une valeur fractionnaire s'écrit avec ses seules décimales
//!   utiles (`112.5`, `0.125`). Le nombre est formé en texte puis lu en
//!   `f64` (lecture correctement arrondie, sans arithmétique) : le
//!   sérialiseur JSON écrit le `f64` le plus court qui se relit, soit le
//!   même texte, car ces valeurs ont au plus 15 chiffres significatifs.
//! - **Lecture** : un entier garde son sens ; un nombre décimal est relu
//!   par son écriture la plus courte (`Display` de `f64`, sans exposant) et
//!   accepté seulement s'il a au plus `digits` décimales (il tombe sur
//!   l'échelle), sinon refusé. Aucune tolérance : `112.15` n'est pas un
//!   dixième de minute.
//!
//! Le `f64` ne sert qu'au passage par serde ; aucun calcul n'est fait avec
//! (règle 5).

#![allow(
    clippy::disallowed_types,
    reason = "frontière serde : un nombre JSON décimal passe par f64, formé et relu en texte, jamais calculé (règle 5, ADR-0039)"
)]

use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserializer, Serializer};

/// `10^digits` (digits ≤ 3 ici), `None` au-delà de `i64`.
const fn pow10(digits: u32) -> Option<i64> {
    10_i64.checked_pow(digits)
}

/// Texte décimal de `value / 10^digits` : entier sans point, sinon les
/// seules décimales utiles (`1125`, 1 → `"112.5"` ; `-5`, 1 → `"-0.5"`).
#[must_use]
pub fn to_text(value: i64, digits: u32) -> String {
    let Some(scale) = pow10(digits) else {
        return value.to_string();
    };
    let (Some(whole), Some(rest)) = (value.checked_div(scale), value.checked_rem(scale)) else {
        return value.to_string();
    };
    if rest == 0 {
        return whole.to_string();
    }
    let sign = if value < 0 { "-" } else { "" };
    let mut frac = format!(
        "{:0width$}",
        rest.unsigned_abs(),
        width = usize::try_from(digits).unwrap_or(0)
    );
    while frac.ends_with('0') {
        frac.pop();
    }
    format!("{sign}{}.{frac}", whole.unsigned_abs())
}

/// Lit un décimal écrit `[-]chiffres[.chiffres]` en entier à l'échelle
/// `10^digits` ; `None` s'il a plus de `digits` décimales ou déborde.
#[must_use]
pub fn from_text(s: &str, digits: u32) -> Option<i64> {
    let scale = pow10(digits)?;
    let (negative, body) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s),
    };
    let (int_part, frac_part) = body.split_once('.').unwrap_or((body, ""));
    if int_part.is_empty() || !int_part.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if !frac_part.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let frac_digits = frac_part.trim_end_matches('0');
    let used = u32::try_from(frac_digits.len()).ok()?;
    if used > digits {
        return None;
    }
    // En i128 : `i64::MIN` s'écrit avec une magnitude hors de `i64`.
    let whole: i128 = int_part.parse().ok()?;
    let frac: i128 = if frac_digits.is_empty() {
        0
    } else {
        let pad = pow10(digits.checked_sub(used)?)?;
        frac_digits.parse::<i128>().ok()?.checked_mul(i128::from(pad))?
    };
    let magnitude = whole.checked_mul(i128::from(scale))?.checked_add(frac)?;
    let signed = if negative {
        magnitude.checked_neg()?
    } else {
        magnitude
    };
    i64::try_from(signed).ok()
}

/// Écrit `value / 10^digits` : entier JSON si la valeur est entière, sinon
/// nombre décimal (voir le module).
///
/// # Errors
///
/// Erreur du sérialiseur.
pub fn serialize<S: Serializer>(value: i64, digits: u32, s: S) -> Result<S::Ok, S::Error> {
    let text = to_text(value, digits);
    if !text.contains('.')
        && let Ok(v) = text.parse::<i64>()
    {
        return s.serialize_i64(v);
    }
    match text.parse::<f64>() {
        Ok(v) => s.serialize_f64(v),
        Err(e) => Err(serde::ser::Error::custom(e)),
    }
}

/// Lit un nombre de minutes à l'échelle `10^digits` (voir le module).
///
/// # Errors
///
/// Nombre hors des bornes de `i64` à l'échelle, ou qui ne tombe pas sur
/// l'échelle (plus de `digits` décimales), ou qui n'est pas un nombre.
pub fn deserialize<'de, D: Deserializer<'de>>(d: D, digits: u32) -> Result<i64, D::Error> {
    d.deserialize_any(ScaledVisitor { digits })
}

/// Visiteur d'un nombre à l'échelle `10^digits`.
struct ScaledVisitor {
    digits: u32,
}

impl ScaledVisitor {
    /// Message d'un décimal qui ne tombe pas sur l'échelle.
    fn scale_name(&self) -> &'static str {
        match self.digits {
            0 => "une minute entière",
            1 => "un dixième de minute",
            2 => "un centième de minute",
            _ => "un millième de minute",
        }
    }
}

impl Visitor<'_> for ScaledVisitor {
    type Value = i64;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "un nombre de minutes tombant sur {}", self.scale_name())
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<i64, E> {
        pow10(self.digits)
            .and_then(|scale| v.checked_mul(scale))
            .ok_or_else(|| E::custom("minutes hors des bornes"))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<i64, E> {
        let v = i64::try_from(v).map_err(|_| E::custom("minutes hors des bornes"))?;
        self.visit_i64(v)
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<i64, E> {
        // Écriture la plus courte qui se relit (sans exposant) : celle que
        // le client a écrite, pour un décimal d'au plus 15 chiffres.
        let text = v.to_string();
        from_text(&text, self.digits).ok_or_else(|| {
            E::custom(format!(
                "{text} minutes : la valeur doit tomber sur {} (et rester dans les bornes)",
                self.scale_name()
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{from_text, to_text};

    #[test]
    fn text_round_trip() {
        for (v, d, s) in [
            (4_800, 1, "480"),
            (1_125, 1, "112.5"),
            (-5, 1, "-0.5"),
            (0, 1, "0"),
            (125, 3, "0.125"),
            (112_500, 3, "112.5"),
            (-1_001, 3, "-1.001"),
            (-4_800, 1, "-480"),
        ] {
            assert_eq!(to_text(v, d), s, "{v}");
            assert_eq!(from_text(s, d), Some(v), "{s}");
        }
        assert_eq!(from_text("112.15", 1), None);
        assert_eq!(from_text("112.50", 1), Some(1_125));
        assert_eq!(from_text("1e3", 1), None);
        assert_eq!(from_text("", 1), None);
        assert_eq!(from_text(".5", 1), None);
        assert_eq!(from_text("-", 1), None);
        assert_eq!(from_text("99999999999999999999", 1), None);
        assert_eq!(to_text(i64::MIN, 1), "-922337203685477580.8");
        assert_eq!(from_text(&to_text(i64::MIN, 1), 1), Some(i64::MIN));
        assert_eq!(from_text(&to_text(i64::MAX, 3), 3), Some(i64::MAX));
    }
}
