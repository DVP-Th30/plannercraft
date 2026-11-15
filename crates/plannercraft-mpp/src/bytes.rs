//! Lecture bornée d'entiers petit-boutistes : `None` hors des octets, jamais
//! de panique (règle 10).

/// `u8` à `off`.
#[must_use]
pub fn le_u8(b: &[u8], off: usize) -> Option<u8> {
    b.get(off).copied()
}

/// `u16` petit-boutiste à `off`.
#[must_use]
pub fn le_u16(b: &[u8], off: usize) -> Option<u16> {
    let s = b.get(off..off.checked_add(2)?)?;
    Some(u16::from_le_bytes(s.try_into().ok()?))
}

/// `u32` petit-boutiste à `off`.
#[must_use]
pub fn le_u32(b: &[u8], off: usize) -> Option<u32> {
    let s = b.get(off..off.checked_add(4)?)?;
    Some(u32::from_le_bytes(s.try_into().ok()?))
}

/// `i32` petit-boutiste à `off`.
#[must_use]
pub fn le_i32(b: &[u8], off: usize) -> Option<i32> {
    let s = b.get(off..off.checked_add(4)?)?;
    Some(i32::from_le_bytes(s.try_into().ok()?))
}

/// `u64` petit-boutiste à `off`.
#[must_use]
pub fn le_u64(b: &[u8], off: usize) -> Option<u64> {
    let s = b.get(off..off.checked_add(8)?)?;
    Some(u64::from_le_bytes(s.try_into().ok()?))
}

/// Texte UTF-16 petit-boutiste, arrêté au premier caractère nul, borné à
/// `max_chars` caractères ; paires de substitution invalides remplacées.
#[must_use]
pub fn utf16_z(b: &[u8], max_chars: usize) -> String {
    let units: Vec<u16> = b
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|u| *u != 0)
        .take(max_chars)
        .collect();
    String::from_utf16_lossy(&units)
}

/// Tout le texte UTF-16 petit-boutiste de `b`, un caractère nul devenant un
/// saut de ligne, borné à `max_chars` caractères.
#[must_use]
pub fn utf16_z_all(b: &[u8], max_chars: usize) -> String {
    let units: Vec<u16> = b
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take(max_chars)
        .map(|u| if u == 0 { 0x0A } else { u })
        .collect();
    String::from_utf16_lossy(&units)
}

/// Nombre IEEE 754 double (`f64` petit-boutiste) à `off`, divisé par
/// `divisor` et arrondi à l'entier, moitié loin de zéro (R-001). Décodé
/// exactement à partir des bits (mantisse et exposant), sans calcul
/// flottant : même résultat sur toutes les plateformes (règle 5). `None`
/// hors des octets, pour un infini ou un non-nombre, ou hors de `i64`.
#[must_use]
pub fn le_f64_round(b: &[u8], off: usize, divisor: u64) -> Option<i64> {
    let bits = le_u64(b, off)?;
    let negative = bits >> 63 == 1;
    let exponent = u16::try_from((bits >> 52) & 0x7FF).ok()?;
    let fraction = bits & ((1_u64 << 52) - 1);
    if exponent == 0x7FF || divisor == 0 {
        return None;
    }
    // Valeur = mantisse × 2^shift.
    let (mantissa, shift) = if exponent == 0 {
        (fraction, -1074_i32)
    } else {
        (fraction | (1_u64 << 52), i32::from(exponent).checked_sub(1075)?)
    };
    if mantissa == 0 {
        return Some(0);
    }
    let (num, den) = if shift >= 0 {
        // Au-delà de 2^63, la valeur sort de toute façon de `i64`.
        if shift > 11 {
            return None;
        }
        (
            u128::from(mantissa).checked_shl(shift.unsigned_abs())?,
            u128::from(divisor),
        )
    } else {
        let s = shift.unsigned_abs();
        // Valeur sous 2^-50 (mantisse sur 53 bits) : arrondie à 0.
        if s > 100 {
            return Some(0);
        }
        (u128::from(mantissa), u128::from(divisor).checked_shl(s)?)
    };
    let q = num.checked_div(den)?;
    let r = num.checked_rem(den)?;
    let rounded = if r.checked_mul(2)? >= den {
        q.checked_add(1)?
    } else {
        q
    };
    let v = i64::try_from(rounded).ok()?;
    Some(if negative { v.checked_neg()? } else { v })
}
