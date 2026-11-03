//! Identifiants (ADR-0005, ADR-0008) : ceux de MSPDI, sans quatrième.
//!
//! - `UID` : stable, porté par l'entité ([`TaskUid`], [`ResourceUid`],
//!   [`AssignmentUid`], [`CalendarUid`]) ;
//! - `ID` : numéro d'ordre d'une tâche = position + 1, calculé, jamais stocké
//!   ([`TaskId`]) ;
//! - `GUID` : [`Guid`], conservé tel quel ; le cœur n'en génère jamais (pas
//!   d'aléa, règle 5) : un nouveau GUID est fourni par l'appelant (J4).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{EntityKind, ModelError};

/// UID d'une entité rangée dans une [`Arena`](crate::arena::Arena) : sait
/// nommer son type d'entité et donner sa valeur brute (messages d'erreur).
pub trait EntityUid: Ord + Copy {
    /// Type d'entité identifié.
    const ENTITY: EntityKind;
    /// Valeur brute de l'UID.
    fn raw(self) -> u32;
}

macro_rules! uid_type {
    ($(#[$doc:meta])* $name:ident, $entity:expr) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(u32);

        impl $name {
            /// UID depuis sa valeur brute.
            #[must_use]
            pub const fn new(v: u32) -> Self {
                Self(v)
            }

            /// Valeur brute.
            #[must_use]
            pub const fn get(self) -> u32 {
                self.0
            }
        }

        impl EntityUid for $name {
            const ENTITY: EntityKind = $entity;
            fn raw(self) -> u32 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

uid_type!(
    /// UID de tâche. 0 est réservé à la tâche récapitulative du projet.
    TaskUid,
    EntityKind::Task
);
uid_type!(
    /// UID de ressource.
    ResourceUid,
    EntityKind::Resource
);
uid_type!(
    /// UID d'affectation.
    AssignmentUid,
    EntityKind::Assignment
);
uid_type!(
    /// UID de calendrier.
    CalendarUid,
    EntityKind::Calendar
);

impl TaskUid {
    /// Tâche récapitulative du projet (UID 0 / ID 0 de MSPDI).
    pub const PROJECT_SUMMARY: Self = Self(0);
}

/// Numéro d'ordre d'une tâche (`ID` MSPDI) : position dans la liste + 1.
/// Calculé à la demande, jamais stocké.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskId(u32);

impl TaskId {
    /// ID depuis sa valeur brute.
    #[must_use]
    pub const fn new(v: u32) -> Self {
        Self(v)
    }

    /// Valeur brute.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// GUID de 16 octets, écrit `8-4-4-4-12` en hexadécimal : lu en majuscules ou
/// minuscules, écrit en majuscules.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Guid([u8; 16]);

impl Guid {
    /// GUID depuis ses octets, dans l'ordre d'écriture.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Octets, dans l'ordre d'écriture.
    #[must_use]
    pub const fn bytes(&self) -> [u8; 16] {
        self.0
    }
}

/// Valeur d'un chiffre hexadécimal ASCII.
fn hex(b: u8) -> Option<u8> {
    char::from(b).to_digit(16).and_then(|v| u8::try_from(v).ok())
}

impl FromStr for Guid {
    type Err = ModelError;

    /// `XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX`, chiffres hexadécimaux.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || ModelError::InvalidGuid { input: s.to_owned() };
        let bytes = s.as_bytes();
        if bytes.len() != 36 {
            return Err(invalid());
        }
        let mut nibbles = Vec::with_capacity(32);
        for (i, b) in bytes.iter().enumerate() {
            if matches!(i, 8 | 13 | 18 | 23) {
                if *b != b'-' {
                    return Err(invalid());
                }
            } else {
                nibbles.push(hex(*b).ok_or_else(invalid)?);
            }
        }
        let mut out = [0_u8; 16];
        for (slot, [high, low]) in out.iter_mut().zip(nibbles.as_chunks::<2>().0) {
            *slot = high
                .checked_mul(16)
                .and_then(|h| h.checked_add(*low))
                .ok_or_else(invalid)?;
        }
        Ok(Self(out))
    }
}

impl fmt::Display for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, b) in self.0.iter().enumerate() {
            if matches!(i, 4 | 6 | 8 | 10) {
                f.write_str("-")?;
            }
            write!(f, "{b:02X}")?;
        }
        Ok(())
    }
}

impl TryFrom<String> for Guid {
    type Error = ModelError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<Guid> for String {
    fn from(g: Guid) -> Self {
        g.to_string()
    }
}

/// Numéro de ligne de base, de 0 à 10 (11 lignes de base, S11).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct BaselineNumber(u8);

impl BaselineNumber {
    /// Ligne de base n° 0 (« Baseline » sans numéro), seule calculée en V1.
    pub const PRIMARY: Self = Self(0);
    /// Plus grand numéro.
    pub const MAX: u8 = 10;

    /// Numéro de ligne de base.
    ///
    /// # Errors
    ///
    /// [`ModelError::InvalidBaselineNumber`] au-delà de 10.
    pub const fn new(v: u8) -> Result<Self, ModelError> {
        if v > Self::MAX {
            return Err(ModelError::InvalidBaselineNumber { number: v });
        }
        Ok(Self(v))
    }

    /// Valeur brute.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for BaselineNumber {
    type Error = ModelError;
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        Self::new(v)
    }
}

impl From<BaselineNumber> for u8 {
    fn from(b: BaselineNumber) -> Self {
        b.0
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use std::collections::BTreeMap;

    use super::{BaselineNumber, CalendarUid, EntityUid, Guid, TaskId, TaskUid};
    use crate::error::{EntityKind, ModelError};

    #[test]
    fn uids() {
        assert_eq!(TaskUid::new(4).get(), 4);
        assert_eq!(TaskUid::PROJECT_SUMMARY.get(), 0);
        assert_eq!(CalendarUid::ENTITY, EntityKind::Calendar);
        assert_eq!(CalendarUid::new(7).raw(), 7);
        assert!(TaskUid::new(1) < TaskUid::new(2));
        assert_eq!(serde_json::to_string(&TaskUid::new(4)).unwrap(), "4");
        assert_eq!(serde_json::from_str::<TaskUid>("4").unwrap(), TaskUid::new(4));
        assert!(serde_json::from_str::<TaskUid>("-1").is_err());
        assert_eq!(TaskId::new(3).get(), 3);
        assert_eq!(TaskUid::new(9).to_string(), "9");
    }

    #[test]
    fn guid_text() {
        let g: Guid = "8f0c1a2b-3c4d-5e6f-7a8b-9c0d1e2f3a4b".parse().unwrap();
        assert_eq!(g.to_string(), "8F0C1A2B-3C4D-5E6F-7A8B-9C0D1E2F3A4B");
        assert_eq!(g.bytes()[0], 0x8F);
        assert_eq!(g.bytes()[15], 0x4B);
        assert_eq!("8F0C1A2B-3C4D-5E6F-7A8B-9C0D1E2F3A4B".parse::<Guid>().unwrap(), g);
        assert_eq!(Guid::from_bytes(g.bytes()), g);
        for bad in [
            "",
            "8f0c1a2b-3c4d-5e6f-7a8b-9c0d1e2f3a4",
            "8f0c1a2b-3c4d-5e6f-7a8b-9c0d1e2f3a4bb",
            "8f0c1a2b3c4d-5e6f-7a8b-9c0d1e2f3a4b0",
            "{8f0c1a2b-3c4d-5e6f-7a8b-9c0d1e2f3a}",
            "8f0c1a2b-3c4d-5e6f-7a8b-9c0d1e2f3a4g",
            "8f0c1a2b-3c4d-5e6f-7a8b-9c0d1e2f3a4é",
        ] {
            assert_eq!(
                bad.parse::<Guid>(),
                Err(ModelError::InvalidGuid {
                    input: bad.to_owned()
                }),
                "{bad}"
            );
        }
    }

    #[test]
    fn guid_golden_json() {
        let g = Guid::from_bytes([0xAB; 16]);
        let json = serde_json::to_string(&g).unwrap();
        assert_eq!(json, r#""ABABABAB-ABAB-ABAB-ABAB-ABABABABABAB""#);
        assert_eq!(serde_json::from_str::<Guid>(&json.to_lowercase()).unwrap(), g);
        assert!(serde_json::from_str::<Guid>(r#""nope""#).is_err());
    }

    #[test]
    fn baseline_numbers() {
        assert_eq!(BaselineNumber::new(10).unwrap().get(), 10);
        assert_eq!(
            BaselineNumber::new(11),
            Err(ModelError::InvalidBaselineNumber { number: 11 })
        );
        assert_eq!(BaselineNumber::PRIMARY.get(), 0);
        // Clé de map JSON : écrite en texte, relue et validée.
        let map = BTreeMap::from([(BaselineNumber::new(3).unwrap(), 1_u8)]);
        let json = serde_json::to_string(&map).unwrap();
        assert_eq!(json, r#"{"3":1}"#);
        assert_eq!(
            serde_json::from_str::<BTreeMap<BaselineNumber, u8>>(&json).unwrap(),
            map
        );
        assert!(serde_json::from_str::<BTreeMap<BaselineNumber, u8>>(r#"{"11":1}"#).is_err());
    }
}
