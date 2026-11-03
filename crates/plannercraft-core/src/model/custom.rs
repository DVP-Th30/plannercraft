//! Champs personnalisés simples (ADR-0019 D8, R-263) : types seuls.
//!
//! Texte1 à Texte30, Nombre1 à Nombre20, Date1 à Date10, Indicateur1 à
//! Indicateur20, sans formule ni table de choix. Types posés par le lot L0
//! de la seconde passe UX ; bornes des valeurs et champs
//! `Project.custom_fields` et `Task.custom` ajoutés par le lot L8 (R-263) ;
//! commandes dans `plannercraft-automation` (R-264), MSPDI dans `plannercraft-mspdi`
//! (R-265, R-266). Aucun calcul n'en dépend.
//!
//! Identifiant sérialisé en minuscules, sans séparateur (`"text1"`,
//! `"number20"`, `"date10"`, `"flag1"`) : clé des commandes (`task.update {
//! custom }`, R-264) et des maps JSON. Aucun nombre à virgule flottante :
//! un nombre est un entier en dix-millièmes (règle 5).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::time::Moment;

/// Échelle du point fixe des nombres personnalisés : `Number(125_000)`
/// vaut 12,5 (R-263).
pub const CUSTOM_NUMBER_SCALE: i64 = 10_000;

/// Longueur maximale d'un texte personnalisé, en caractères (R-263).
pub const MAX_CUSTOM_TEXT_CHARS: usize = 255;

/// Longueur maximale d'un alias, en caractères (R-263 ; schéma MSPDI :
/// `Alias` de 50 caractères au plus, S91).
pub const MAX_CUSTOM_ALIAS_CHARS: usize = 50;

/// Plus grande valeur absolue d'un nombre personnalisé, en dix-millièmes :
/// |x| ≤ 10^14 (R-263), soit 10^18 dix-millièmes (tient dans un `i64`).
pub const MAX_CUSTOM_NUMBER_ABS: i64 = 1_000_000_000_000_000_000;

/// Valeur personnalisée hors de ses bornes (R-263).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CustomValueError {
    /// Texte de plus de [`MAX_CUSTOM_TEXT_CHARS`] caractères.
    #[error("texte de plus de 255 caractères")]
    TooLong,
    /// Nombre au-delà de ±10^14.
    #[error("nombre hors des bornes (±10^14)")]
    OutOfRange,
}

/// Type d'un champ personnalisé.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomKind {
    /// Texte (Texte1 à Texte30).
    Text,
    /// Nombre (Nombre1 à Nombre20), point fixe en dix-millièmes.
    Number,
    /// Date (Date1 à Date10).
    Date,
    /// Indicateur oui / non (Indicateur1 à Indicateur20).
    Flag,
}

impl CustomKind {
    /// Tous les types, dans l'ordre de [`CustomFieldId::all`].
    pub const ALL: [Self; 4] = [Self::Text, Self::Number, Self::Date, Self::Flag];

    /// Préfixe de l'identifiant sérialisé (`"text"` pour `"text1"`).
    #[must_use]
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Number => "number",
            Self::Date => "date",
            Self::Flag => "flag",
        }
    }
}

/// Identifiant d'un champ personnalisé de tâche : type et numéro, numéro
/// borné par [`CustomFieldId::max`] (hors bornes : aucune valeur).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CustomFieldId {
    /// Type.
    kind: CustomKind,
    /// Numéro, de 1 à `max(kind)`.
    index: u8,
}

impl CustomFieldId {
    /// Texte1, premier champ.
    pub const TEXT1: Self = Self {
        kind: CustomKind::Text,
        index: 1,
    };

    /// Champ `kind` de numéro `index` ; `None` hors de `1..=max(kind)`.
    #[must_use]
    pub const fn new(kind: CustomKind, index: u8) -> Option<Self> {
        if index >= 1 && index <= Self::max(kind) {
            Some(Self { kind, index })
        } else {
            None
        }
    }

    /// Numéro le plus grand d'un type : Texte 30, Nombre 20, Date 10,
    /// Indicateur 20.
    #[must_use]
    pub const fn max(kind: CustomKind) -> u8 {
        match kind {
            CustomKind::Text => 30,
            CustomKind::Number | CustomKind::Flag => 20,
            CustomKind::Date => 10,
        }
    }

    /// Tous les champs, par type puis par numéro (80).
    pub fn all() -> impl Iterator<Item = Self> {
        CustomKind::ALL
            .into_iter()
            .flat_map(|kind| (1..=Self::max(kind)).map(move |index| Self { kind, index }))
    }

    /// Type du champ.
    #[must_use]
    pub const fn kind(self) -> CustomKind {
        self.kind
    }

    /// Numéro du champ (1 à `max(kind)`).
    #[must_use]
    pub const fn index(self) -> u8 {
        self.index
    }

    /// Nom canonique anglais du champ (S90 : « Represents the Text1 (Task)
    /// field. ») : `Text1`, `Number20`, `Date10`, `Flag1` ; c'est le
    /// `FieldName` écrit dans le MSPDI (R-266).
    #[must_use]
    pub fn canonical_name(self) -> String {
        let word = match self.kind {
            CustomKind::Text => "Text",
            CustomKind::Number => "Number",
            CustomKind::Date => "Date",
            CustomKind::Flag => "Flag",
        };
        format!("{word}{}", self.index)
    }
}

impl fmt::Display for CustomFieldId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.kind.prefix(), self.index)
    }
}

/// Identifiant illisible : préfixe inconnu ou numéro hors bornes.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("champ personnalisé inconnu : « {0} »")]
pub struct UnknownCustomField(pub String);

impl FromStr for CustomFieldId {
    type Err = UnknownCustomField;

    /// Lit `"text1"` … `"flag20"` (minuscules, sans zéro en tête).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let unknown = || UnknownCustomField(s.to_owned());
        let kind = CustomKind::ALL
            .into_iter()
            .find(|kind| s.starts_with(kind.prefix()))
            .ok_or_else(unknown)?;
        let digits = s.get(kind.prefix().len()..).ok_or_else(unknown)?;
        if digits.is_empty() || digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(unknown());
        }
        let index: u8 = digits.parse().map_err(|_| unknown())?;
        Self::new(kind, index).ok_or_else(unknown)
    }
}

impl TryFrom<String> for CustomFieldId {
    type Error = UnknownCustomField;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<CustomFieldId> for String {
    fn from(id: CustomFieldId) -> Self {
        id.to_string()
    }
}

/// Valeur d'un champ personnalisé (R-263).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomValue {
    /// Texte (≤ [`MAX_CUSTOM_TEXT_CHARS`] caractères, [`CustomValue::check`]).
    Text(String),
    /// Nombre en dix-millièmes ([`CUSTOM_NUMBER_SCALE`]).
    Number(i64),
    /// Date.
    Date(Moment),
    /// Indicateur.
    Flag(bool),
}

impl CustomValue {
    /// R-263 : valeur dans ses bornes — texte d'au plus
    /// [`MAX_CUSTOM_TEXT_CHARS`] caractères, nombre d'au plus
    /// [`MAX_CUSTOM_NUMBER_ABS`] dix-millièmes en valeur absolue.
    ///
    /// # Errors
    ///
    /// [`CustomValueError`] hors des bornes.
    pub fn check(&self) -> Result<(), CustomValueError> {
        match self {
            Self::Text(t) if t.chars().nth(MAX_CUSTOM_TEXT_CHARS).is_some() => Err(CustomValueError::TooLong),
            Self::Number(n) if n.unsigned_abs() > MAX_CUSTOM_NUMBER_ABS.unsigned_abs() => {
                Err(CustomValueError::OutOfRange)
            }
            _ => Ok(()),
        }
    }

    /// Valeur « vide » : texte vide ou indicateur à `false`, jamais stockée
    /// (R-263 : absent = vide).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Text(t) if t.is_empty()) || *self == Self::Flag(false)
    }

    /// Type de la valeur.
    #[must_use]
    pub const fn kind(&self) -> CustomKind {
        match self {
            Self::Text(_) => CustomKind::Text,
            Self::Number(_) => CustomKind::Number,
            Self::Date(_) => CustomKind::Date,
            Self::Flag(_) => CustomKind::Flag,
        }
    }
}

/// Définition d'un champ personnalisé dans le projet : son alias (nom
/// affiché, « Lot » pour Texte1), ≤ [`MAX_CUSTOM_ALIAS_CHARS`] caractères.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomFieldDef {
    /// Alias, s'il y en a un.
    pub alias: Option<String>,
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use super::{CustomFieldId, CustomKind, CustomValue};

    /// R-263 (types, L0) : Texte 1-30, Nombre 1-20, Date 1-10, Indicateur
    /// 1-20 ; 0 et max+1 ⇒ `None` ; 80 champs, uniques, triés.
    #[test]
    fn custom_field_id_bounds() {
        for (kind, max) in [
            (CustomKind::Text, 30),
            (CustomKind::Number, 20),
            (CustomKind::Date, 10),
            (CustomKind::Flag, 20),
        ] {
            assert_eq!(CustomFieldId::max(kind), max);
            assert!(CustomFieldId::new(kind, 0).is_none());
            assert!(CustomFieldId::new(kind, max + 1).is_none());
            assert!(CustomFieldId::new(kind, u8::MAX).is_none());
            let first = CustomFieldId::new(kind, 1).unwrap();
            let last = CustomFieldId::new(kind, max).unwrap();
            assert_eq!((first.kind(), first.index()), (kind, 1));
            assert_eq!(last.index(), max);
        }
        let all: Vec<CustomFieldId> = CustomFieldId::all().collect();
        assert_eq!(all.len(), 80);
        assert!(all.windows(2).all(|w| w[0] < w[1]));
    }

    /// Identifiant sérialisé en minuscules (`"text1"`), relu à l'identique ;
    /// préfixe inconnu, zéro en tête ou numéro hors bornes refusés.
    #[test]
    fn custom_field_id_text_roundtrip() {
        for id in CustomFieldId::all() {
            let text = id.to_string();
            assert_eq!(text.parse::<CustomFieldId>().unwrap(), id);
            let json = serde_json::to_string(&id).unwrap();
            assert_eq!(json, format!("\"{text}\""));
            assert_eq!(serde_json::from_str::<CustomFieldId>(&json).unwrap(), id);
        }
        assert_eq!(
            CustomFieldId::new(CustomKind::Number, 20).unwrap().to_string(),
            "number20"
        );
        for bad in [
            "", "text", "text0", "text01", "text31", "Text1", "cost1", "flag-1", "date 1", "text1x",
        ] {
            assert!(bad.parse::<CustomFieldId>().is_err(), "{bad}");
        }
        assert!(serde_json::from_str::<CustomFieldId>("\"date11\"").is_err());
        assert_eq!(CustomValue::Number(125_000).kind(), CustomKind::Number);
        assert_eq!(CustomValue::Flag(true).kind(), CustomKind::Flag);
    }
}
