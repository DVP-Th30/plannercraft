//! Lecture caractère par caractère, commune aux parseurs du texte : espaces,
//! nombres décimaux exacts, repli de la casse et des accents.

use plannercraft_core::Decimal;

use super::{DecimalMark, Language, MAX_INPUT_CHARS, TextError};

/// Caractères d'une entrée bornée à [`MAX_INPUT_CHARS`].
///
/// # Errors
///
/// [`TextError::TooLong`] au-delà de la borne.
pub(crate) fn bounded_chars(s: &str) -> Result<Vec<char>, TextError> {
    let mut out = Vec::new();
    for c in s.chars() {
        if out.len() >= MAX_INPUT_CHARS {
            return Err(TextError::TooLong);
        }
        out.push(c);
    }
    Ok(out)
}

/// Espace admis entre les éléments et comme séparateur de milliers : espaces
/// ASCII, espace insécable (U+00A0) et espace fine insécable (U+202F).
pub(crate) const fn is_space(c: char) -> bool {
    c.is_ascii_whitespace() || c == '\u{a0}' || c == '\u{202f}'
}

/// Curseur sur les caractères d'une entrée.
pub(crate) struct Cursor<'a> {
    chars: &'a [char],
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub(crate) const fn new(chars: &'a [char]) -> Self {
        Self { chars, pos: 0 }
    }

    /// Position courante, en caractères.
    #[cfg(test)]
    pub(crate) const fn pos(&self) -> usize {
        self.pos
    }

    /// Caractère courant.
    pub(crate) fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    /// Caractère à `n` positions de la position courante.
    pub(crate) fn peek_at(&self, n: usize) -> Option<char> {
        self.chars.get(self.pos.checked_add(n)?).copied()
    }

    /// Avance d'un caractère.
    pub(crate) fn bump(&mut self) {
        if self.pos < self.chars.len() {
            self.pos = self.pos.saturating_add(1);
        }
    }

    /// Vrai si toute l'entrée est lue.
    pub(crate) const fn at_end(&self) -> bool {
        self.pos >= self.chars.len()
    }

    /// Saute les espaces.
    pub(crate) fn skip_spaces(&mut self) {
        while self.peek().is_some_and(is_space) {
            self.bump();
        }
    }

    /// Reste de l'entrée.
    pub(crate) fn rest(&self) -> &'a [char] {
        self.chars.get(self.pos..).unwrap_or(&[])
    }

    /// Lit un signe facultatif (`+` ou `-`) suivi d'espaces ; vrai pour `-`.
    pub(crate) fn sign(&mut self) -> bool {
        let negative = match self.peek() {
            Some('-') => true,
            Some('+') => false,
            _ => return false,
        };
        self.bump();
        self.skip_spaces();
        negative
    }

    /// Entier non signé fait de chiffres ASCII seuls (numéro de tâche).
    ///
    /// # Errors
    ///
    /// [`TextError::BadNumber`] sans chiffre, [`TextError::OutOfRange`] au-delà
    /// de `u32`.
    pub(crate) fn integer(&mut self) -> Result<u32, TextError> {
        let start = self.pos;
        let mut v: u32 = 0;
        while let Some(d) = self.peek().and_then(|c| c.to_digit(10)) {
            v = v
                .checked_mul(10)
                .and_then(|v| v.checked_add(d))
                .ok_or(TextError::OutOfRange)?;
            self.bump();
        }
        if self.pos == start {
            return Err(TextError::BadNumber { at: start });
        }
        Ok(v)
    }

    /// Nombre décimal non signé (R-140) : chiffres, groupes de trois
    /// chiffres précédés d'une espace (séparateur de milliers), puis `,` ou
    /// `.` et 1 à 6 décimales.
    ///
    /// # Errors
    ///
    /// [`TextError::BadNumber`] sans chiffre ou au-delà de 6 décimales,
    /// [`TextError::OutOfRange`] si la mantisse ne tient pas dans un `i64`.
    pub(crate) fn decimal(&mut self) -> Result<Decimal, TextError> {
        self.decimal_in(Language::Fr)
    }

    /// Nombre décimal de [`Cursor::decimal`], lu dans la langue `language`.
    /// En anglais, une virgule suivie d'exactement trois chiffres puis de la
    /// fin du nombre est un séparateur de milliers anglais (« 1,000 hrs »),
    /// pas une marque décimale : refusée (`BadNumber` sur la virgule) plutôt
    /// que lue 1 sans diagnostic (R-140).
    ///
    /// # Errors
    ///
    /// Comme [`Cursor::decimal`].
    pub(crate) fn decimal_in(&mut self, language: Language) -> Result<Decimal, TextError> {
        let start = self.pos;
        let mut mantissa: i64 = 0;
        let mut digits = 0_usize;
        loop {
            match self.peek() {
                Some(c) if c.is_ascii_digit() => {
                    mantissa = push_digit(mantissa, c)?;
                    digits = digits.saturating_add(1);
                    self.bump();
                }
                Some(c) if is_space(c) && digits > 0 && self.thousands_group_follows() => {
                    self.bump();
                }
                _ => break,
            }
        }
        if digits == 0 {
            return Err(TextError::BadNumber { at: start });
        }
        let mut scale: u8 = 0;
        if matches!(self.peek(), Some(',' | '.')) && self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) {
            if language == Language::En
                && self.peek() == Some(',')
                && (1..=3).all(|i| self.peek_at(i).is_some_and(|c| c.is_ascii_digit()))
                && !self.peek_at(4).is_some_and(|c| c.is_ascii_digit())
            {
                return Err(TextError::BadNumber { at: self.pos });
            }
            self.bump();
            while let Some(c) = self.peek().filter(char::is_ascii_digit) {
                if scale >= Decimal::MAX_SCALE {
                    return Err(TextError::BadNumber { at: self.pos });
                }
                mantissa = push_digit(mantissa, c)?;
                scale = scale.saturating_add(1);
                self.bump();
            }
        }
        Decimal::new(mantissa, scale).map_err(|_| TextError::BadNumber { at: start })
    }

    /// Vrai si la position courante (une espace) est suivie d'exactement
    /// trois chiffres puis d'un non-chiffre : séparateur de milliers.
    fn thousands_group_follows(&self) -> bool {
        (1..=3).all(|i| self.peek_at(i).is_some_and(|c| c.is_ascii_digit()))
            && !self.peek_at(4).is_some_and(|c| c.is_ascii_digit())
    }
}

/// `mantissa × 10 + chiffre`, `OutOfRange` en cas de débordement.
fn push_digit(mantissa: i64, c: char) -> Result<i64, TextError> {
    let d = i64::from(c.to_digit(10).unwrap_or(0));
    mantissa
        .checked_mul(10)
        .and_then(|m| m.checked_add(d))
        .ok_or(TextError::OutOfRange)
}

/// Repli de la casse et des accents latins usuels (« Éj » → « ej »), pour
/// comparer une unité, un en-tête ou un mot-clé.
pub(crate) fn fold(s: impl IntoIterator<Item = char>) -> String {
    let mut out = String::new();
    for c in s {
        for l in c.to_lowercase() {
            out.push(match l {
                'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => 'a',
                'é' | 'è' | 'ê' | 'ë' => 'e',
                'î' | 'ï' | 'í' | 'ì' => 'i',
                'ô' | 'ö' | 'ó' | 'ò' | 'õ' => 'o',
                'ù' | 'û' | 'ü' | 'ú' => 'u',
                'ç' => 'c',
                'ñ' => 'n',
                'ÿ' => 'y',
                other => other,
            });
        }
    }
    out
}

/// Décimal en texte avec la marque choisie, sans zéro final (`1,5`, `0,01458`,
/// `-2`).
pub(crate) fn format_decimal(value: Decimal, mark: DecimalMark) -> String {
    let value = value.normalized();
    let mantissa = i128::from(value.mantissa());
    let negative = mantissa < 0;
    // Écriture des chiffres de |mantisse| (au moins scale + 1 chiffres), puis
    // insertion de la marque : aucun calcul qui puisse échouer.
    let mut digits: Vec<char> = mantissa.unsigned_abs().to_string().chars().collect();
    let scale = usize::from(value.scale());
    while digits.len() <= scale {
        digits.insert(0, '0');
    }
    let int_len = digits.len().saturating_sub(scale);
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    for (i, c) in digits.iter().enumerate() {
        if i == int_len && scale > 0 {
            out.push(match mark {
                DecimalMark::Dot => '.',
                DecimalMark::Comma => ',',
            });
        }
        out.push(*c);
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use plannercraft_core::Decimal;

    use super::{Cursor, bounded_chars, fold, format_decimal};
    use crate::text::{DecimalMark, Language, MAX_INPUT_CHARS, TextError};

    fn dec(s: &str) -> Result<(Decimal, usize), TextError> {
        let chars = bounded_chars(s).unwrap();
        let mut c = Cursor::new(&chars);
        let d = c.decimal()?;
        Ok((d, c.pos()))
    }

    #[test]
    fn decimals_and_thousands() {
        assert_eq!(dec("12").unwrap(), (Decimal::new(12, 0).unwrap(), 2));
        assert_eq!(dec("1,5j").unwrap(), (Decimal::new(15, 1).unwrap(), 3));
        assert_eq!(dec("1.25").unwrap(), (Decimal::new(125, 2).unwrap(), 4));
        // Séparateurs de milliers : espace, U+00A0, U+202F, groupes de 3.
        assert_eq!(dec("1 000 h").unwrap().0, Decimal::new(1_000, 0).unwrap());
        assert_eq!(
            dec("1\u{a0}000\u{202f}000").unwrap().0,
            Decimal::new(1_000_000, 0).unwrap()
        );
        // Un groupe qui n'a pas trois chiffres n'est pas un séparateur.
        assert_eq!(dec("1 00").unwrap(), (Decimal::new(1, 0).unwrap(), 1));
        assert_eq!(dec("5 2j").unwrap(), (Decimal::new(5, 0).unwrap(), 1));
        // Marque décimale sans chiffre après : non lue.
        assert_eq!(dec("5,j").unwrap(), (Decimal::new(5, 0).unwrap(), 1));
        assert_eq!(dec("0,123456").unwrap().0, Decimal::new(123_456, 6).unwrap());
        assert_eq!(dec("0,1234567"), Err(TextError::BadNumber { at: 8 }));
        assert_eq!(dec("j"), Err(TextError::BadNumber { at: 0 }));
        assert_eq!(dec("99999999999999999999"), Err(TextError::OutOfRange));
    }

    /// R-140 : en anglais, « 1,000 » est un séparateur de milliers, refusé ;
    /// une virgule suivie d'un autre nombre de chiffres reste décimale.
    #[test]
    fn r140_english_comma_thousands_refused() {
        let en = |s: &str| {
            let chars = bounded_chars(s).unwrap();
            Cursor::new(&chars).decimal_in(Language::En)
        };
        assert_eq!(en("1,000 hrs"), Err(TextError::BadNumber { at: 1 }));
        assert_eq!(en("12,345"), Err(TextError::BadNumber { at: 2 }));
        assert_eq!(en("1,5"), Ok(Decimal::new(15, 1).unwrap()));
        assert_eq!(en("1,0005"), Ok(Decimal::new(10_005, 4).unwrap()));
        assert_eq!(en("1.000"), Ok(Decimal::new(1_000, 3).unwrap()));
        assert_eq!(dec("1,000").unwrap().0, Decimal::new(1_000, 3).unwrap());
    }

    #[test]
    fn bounded_input() {
        assert!(bounded_chars(&"1".repeat(MAX_INPUT_CHARS)).is_ok());
        assert_eq!(
            bounded_chars(&"é".repeat(MAX_INPUT_CHARS + 1)),
            Err(TextError::TooLong)
        );
    }

    #[test]
    fn folding_and_formatting() {
        assert_eq!(fold("Éj".chars()), "ej");
        assert_eq!(fold("Durée".chars()), "duree");
        assert_eq!(
            format_decimal(Decimal::new(150, 2).unwrap(), DecimalMark::Comma),
            "1,5"
        );
        assert_eq!(
            format_decimal(Decimal::new(-2, 0).unwrap(), DecimalMark::Dot),
            "-2"
        );
        assert_eq!(
            format_decimal(Decimal::new(1_458, 5).unwrap(), DecimalMark::Comma),
            "0,01458"
        );
        assert_eq!(
            format_decimal(Decimal::new(-5, 1).unwrap(), DecimalMark::Dot),
            "-0.5"
        );
        assert_eq!(
            format_decimal(Decimal::new(i64::MIN, 0).unwrap(), DecimalMark::Dot),
            i64::MIN.to_string()
        );
    }
}
