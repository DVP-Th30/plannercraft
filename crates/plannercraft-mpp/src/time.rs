//! Horodatages et durées des enregistrements (R-283, R-285).
//!
//! Observé (`docs/formats/mpp.md` § 6) : un horodatage tient sur 4 octets,
//! `u32` petit-boutiste dont les 16 bits hauts sont le numéro du jour (1 =
//! 1984-01-01) et les 16 bits bas l'heure en dixièmes de minute ;
//! `FF FF FF FF` = aucune date (NA). Une durée ou un décalage est un `i32`
//! en dixièmes de minute.

use plannercraft_core::{Day, Minutes, Moment, round_div};

/// Aucune date.
pub const NA: u32 = 0xFFFF_FFFF;
/// Dixièmes de minute dans une journée.
const TENTHS_PER_DAY: u32 = 14_400;

/// Lecture d'un horodatage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stamp {
    /// `FF FF FF FF` : aucune date.
    Na,
    /// Date lue.
    At(Moment),
    /// Jour 0, heure au-delà de 24:00 ou hors du domaine du moteur
    /// (1984-01-01 à 2149-12-31).
    Invalid,
}

/// Horodatage depuis sa valeur brute.
#[must_use]
pub fn stamp(raw: u32) -> Stamp {
    if raw == NA {
        return Stamp::Na;
    }
    let day = raw >> 16;
    let tenths = raw & 0xFFFF;
    if day == 0 || tenths > TENTHS_PER_DAY {
        return Stamp::Invalid;
    }
    let minutes = i64::from(day.saturating_sub(1))
        .checked_mul(1_440)
        .and_then(|m| m.checked_add(round_div(i128::from(tenths), 10)?));
    match minutes.map(Moment::from_minutes) {
        Some(Ok(m)) => Stamp::At(m),
        _ => Stamp::Invalid,
    }
}

/// Jour depuis son numéro (1 = 1984-01-01), `None` hors du domaine.
#[must_use]
pub fn day(n: u16) -> Option<Day> {
    Day::new(i32::from(n).checked_sub(1)?).ok()
}

/// Minutes depuis des dixièmes de minute, arrondies par R-001.
#[must_use]
pub fn minutes_from_tenths(tenths: i64) -> Option<Minutes> {
    round_div(i128::from(tenths), 10).map(Minutes::new)
}
