//! Horodatages et durées des enregistrements (R-283, R-285).
//!
//! Observé (`docs/formats/mpp.md` § 6) : un horodatage tient sur 4 octets,
//! `u32` petit-boutiste dont les 16 bits hauts sont le numéro du jour (1 =
//! 1984-01-01) et les 16 bits bas l'heure en dixièmes de minute ;
//! `FF FF FF FF` = aucune date (NA) ; lu exactement, au dixième de minute
//! (ADR-0039). Une durée ou un décalage est un `i32` en dixièmes de
//! minute, lu exactement.

use plannercraft_core::{Day, Minutes, Moment};

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
    // Au dixième de minute, exact (ADR-0039).
    let at = i64::from(day.saturating_sub(1))
        .checked_mul(i64::from(TENTHS_PER_DAY))
        .and_then(|t| t.checked_add(i64::from(tenths)));
    match at.map(Moment::from_tenths) {
        Some(Ok(m)) => Stamp::At(m),
        _ => Stamp::Invalid,
    }
}

/// Jour depuis son numéro (1 = 1984-01-01), `None` hors du domaine.
#[must_use]
pub fn day(n: u16) -> Option<Day> {
    Day::new(i32::from(n).checked_sub(1)?).ok()
}

/// Durée depuis des dixièmes de minute, exacte (ADR-0039 ; avant :
/// arrondie à la minute par R-001).
#[must_use]
pub const fn minutes_from_tenths(tenths: i64) -> Option<Minutes> {
    Some(Minutes::from_tenths(tenths))
}
