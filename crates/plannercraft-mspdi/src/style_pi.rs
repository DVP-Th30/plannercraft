//! Style du document dans le fichier : instruction de traitement
//! `<?plannercraft-style {…}?>` (H-R12, ADR-0016 D7).
//!
//! Le style choisi par `style.apply` vit dans `Project.extras`, entrée
//! d'origine [`ORIGIN`] et de clé [`KEY`], charge = JSON canonique du style
//! (`DocStyle::to_json` de `plannercraft-render`). Ce module ne lit pas le JSON
//! (`plannercraft-mspdi` ne dépend pas du rendu) : il borne et place
//! l'instruction ; le contenu est validé à l'ouverture par
//! `plannercraft-automation` (version, champs, bornes).
//!
//! - **Lecture** : première instruction de cible [`TARGET`] (ou
//!   [`LEGACY_TARGET`], fichier d'une version nommée `planrs`) avant l'élément
//!   racine, au plus [`MAX_BYTES`] octets ; sinon ignorée avec le
//!   diagnostic `style.ignored` ([`crate::ImportCode::StyleIgnored`]).
//! - **Écriture** : juste après la déclaration XML, si l'entrée existe, que
//!   sa charge est écrivable ([`writable`]) et que `WriteOptions::style`
//!   est vrai ; une entrée d'origine `plannercraft` n'est jamais écrite comme
//!   élément XML (R-123 : seules les entrées d'origine `mspdi` le sont).
//!   Un projet sans entrée s'écrit octet pour octet comme en J7.

use plannercraft_core::{ExtraEntry, Extras};

/// Origine de l'entrée de style dans `Project.extras`.
pub const ORIGIN: &str = "plannercraft";

/// Origine de l'entrée de style des versions nommées `planrs` (avant le
/// 2026-11-03), présente dans un projet JSON qu'elles ont écrit (journal de
/// reprise) : reconnue par [`entry`] et remplacée par [`set`], jamais
/// écrite.
pub const LEGACY_ORIGIN: &str = "planrs";

fn is_style(e: &ExtraEntry) -> bool {
    (e.origin == ORIGIN || e.origin == LEGACY_ORIGIN) && e.key == KEY
}

/// Clé de l'entrée de style.
pub const KEY: &str = "style";

/// Cible de l'instruction de traitement.
pub const TARGET: &str = "plannercraft-style";

/// Cible écrite par les versions nommées `planrs` (avant le 2026-11-03) :
/// lue comme [`TARGET`] (même charge, mêmes règles : la première des deux
/// l'emporte), jamais écrite. Un fichier enregistré par une ancienne
/// version garde ainsi son style ; il est réécrit sous [`TARGET`].
pub const LEGACY_TARGET: &str = "planrs-style";

/// Taille maximale de la charge (64 Kio, comme `parse_doc_style`).
pub const MAX_BYTES: usize = 64 * 1024;

/// Raison pour laquelle une instruction `plannercraft-style` est ignorée.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleIgnoredReason {
    /// Charge de plus de [`MAX_BYTES`] octets.
    TooLarge,
    /// Seconde instruction (la première l'emporte).
    Duplicate,
    /// Instruction après l'ouverture de la racine.
    AfterRoot,
    /// Contenu illisible ou hors bornes (JSON, version, valeurs), constaté
    /// à l'ouverture par `plannercraft-automation`.
    Invalid,
}

impl StyleIgnoredReason {
    /// Code stable de la raison (valeur du diagnostic).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TooLarge => "too_large",
            Self::Duplicate => "duplicate",
            Self::AfterRoot => "after_root",
            Self::Invalid => "invalid",
        }
    }
}

/// Entrée de style du projet (la première, s'il y en a plusieurs).
#[must_use]
pub fn entry(extras: &Extras) -> Option<&ExtraEntry> {
    extras.0.iter().find(|e| is_style(e))
}

/// Remplace l'entrée de style (`None` : la retire). L'entrée nouvelle va en
/// fin de liste, comme à la lecture (les entrées d'origine autre que
/// `mspdi` suivent les autres, ordre canonique de [`crate::extras`]).
pub fn set(extras: &mut Extras, payload: Option<String>) {
    extras.0.retain(|e| !is_style(e));
    if let Some(payload) = payload {
        extras.0.push(ExtraEntry {
            origin: ORIGIN.to_owned(),
            key: KEY.to_owned(),
            payload,
        });
    }
}

/// Vrai si `payload` peut s'écrire tel quel dans l'instruction et se relire
/// à l'identique : non vide, au plus [`MAX_BYTES`] octets, sans espace de
/// bord (la lecture les retire), sans `?>` (fin de l'instruction), sans
/// caractère interdit par XML 1.0.
#[must_use]
pub fn writable(payload: &str) -> bool {
    !payload.is_empty()
        && payload.len() <= MAX_BYTES
        && payload.trim_matches(is_xml_space) == payload
        && !payload.contains("?>")
        && payload.chars().all(crate::write::is_xml_char)
}

/// Espace XML (`S` de XML 1.0).
const fn is_xml_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

/// Charge lue dans le contenu d'une instruction (espaces de bord retirés).
pub(crate) fn payload_of(content: &str) -> &str {
    content.trim_matches(is_xml_space)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_replaces_and_removes() {
        let mut ex = Extras(vec![ExtraEntry {
            origin: "mspdi".to_owned(),
            key: "CurrencyCode".to_owned(),
            payload: "<CurrencyCode>EUR</CurrencyCode>".to_owned(),
        }]);
        set(&mut ex, Some("{\"v\":1}".to_owned()));
        set(&mut ex, Some("{\"v\":2}".to_owned()));
        assert_eq!(ex.0.len(), 2);
        assert_eq!(entry(&ex).map(|e| e.payload.as_str()), Some("{\"v\":2}"));
        set(&mut ex, None);
        assert!(entry(&ex).is_none());
        assert_eq!(ex.0.len(), 1);
    }

    #[test]
    fn writable_bounds() {
        assert!(writable("{\"v\":1}"));
        assert!(!writable(""));
        assert!(!writable(" {}"));
        assert!(!writable("{\"a\":\"?>\"}"));
        assert!(!writable("{\"a\":\"\u{1}\"}"));
        assert!(!writable(&"x".repeat(MAX_BYTES + 1)));
        assert!(writable(&"x".repeat(MAX_BYTES)));
        assert_eq!(payload_of(" {}\n"), "{}");
        assert_eq!(StyleIgnoredReason::AfterRoot.as_str(), "after_root");
    }
}
