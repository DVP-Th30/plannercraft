//! Données hors modèle conservées pour l'aller-retour (ADR-0008, § 5.6).

use serde::{Deserialize, Serialize};

/// Données hors modèle conservées pour l'aller-retour (champs personnalisés,
/// hyperliens…). Opaques pour le cœur, qui ne les lit, ne les valide ni ne
/// les interprète jamais.
///
/// Elles vivent dans le modèle (et non dans un sac propre à `plannercraft-mspdi`)
/// pour suivre les commandes, l'undo par instantanés et la suppression d'une
/// entité. La liste blanche des éléments conservés et le contenu de `payload`
/// sont décidés en J3 ; les éléments calculés n'y vont pas.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Extras(pub Vec<ExtraEntry>);

impl Extras {
    /// Aucune donnée hors modèle.
    #[must_use]
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    /// Vrai si aucune entrée.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Une donnée hors modèle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtraEntry {
    /// Origine (ex. `"mspdi"`).
    pub origin: String,
    /// Clé dans l'origine (ex. nom d'élément).
    pub key: String,
    /// Contenu brut, opaque.
    pub payload: String,
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{ExtraEntry, Extras};

    #[test]
    fn extras_json() {
        assert!(Extras::new().is_empty());
        let e = Extras(vec![ExtraEntry {
            origin: "mspdi".to_owned(),
            key: "Hyperlink".to_owned(),
            payload: "<Hyperlink>x</Hyperlink>".to_owned(),
        }]);
        let json = serde_json::to_string(&e).unwrap();
        assert_eq!(
            json,
            r#"[{"origin":"mspdi","key":"Hyperlink","payload":"<Hyperlink>x</Hyperlink>"}]"#
        );
        assert_eq!(serde_json::from_str::<Extras>(&json).unwrap(), e);
    }
}
