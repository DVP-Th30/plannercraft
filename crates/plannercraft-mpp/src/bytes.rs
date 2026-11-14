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
