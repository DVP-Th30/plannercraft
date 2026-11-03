//! Éléments non modélisés conservés en `Extras` (R-087 lecture, R-123
//! écriture ; ADR-0010, D14).
//!
//! Un enfant de classe P (ou hors schéma) d'une entité est capturé verbatim
//! (tranche exacte de l'entrée) dans un [`ExtraEntry`] d'origine
//! [`ORIGIN`], sous une clé texte qui dit où le réécrire :
//!
//! ```text
//! [ "Baseline#" n "/" ] [ ancre ">" ] nom
//! ```
//!
//! - un nom du schéma n'a pas d'ancre : sa place vient de la table
//!   ([`crate::schema`]) ;
//! - un nom hors schéma est ancré sur le plus proche frère précédent **du
//!   schéma et toujours réécrit** (classes M, D, F, ou P capturé), `^` s'il
//!   n'y en a aucun ; les classes C, X et M? ne servent jamais d'ancre (leur
//!   présence dépend des valeurs : la clé changerait à la relecture) ;
//! - `Baseline#n/` : enfant non modélisé de la ligne de base n ;
//! - `@xmlns:p` (projet seulement) : déclaration d'espace de noms de la
//!   racine, réémise sur la racine.
//!
//! Ordre canonique : les `Extras` d'une entité sont triés (tri stable) par
//! emplacement d'écriture dans le profil observé — nom du schéma : son rang ;
//! ancré : rang de l'ancre + ½ ; `^` : −½ ; portée `Baseline#n` : sous la
//! ligne de base n. L'écriture suit le même ordre : lecture → écriture →
//! lecture donne des `Extras` égaux.

use std::fmt;

use plannercraft_core::{ExtraEntry, Extras};
use quick_xml::Reader;
use quick_xml::events::Event;

use crate::schema::{Class, Parent, Profile, sequence};
use crate::xml::Element;

/// Origine des `Extras` écrits et relus par ce module.
pub const ORIGIN: &str = "mspdi";

/// Ancre d'un élément hors schéma.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Anchor {
    /// Aucune : nom du schéma, placé par la table.
    None,
    /// `^` : avant le premier élément de l'entité.
    Start,
    /// Juste après cet élément du schéma (et ses `Extras` de même nom).
    After(String),
}

/// Clé d'un `Extras` MSPDI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtraKey {
    /// Ligne de base n (`Baseline#n/`), pour un enfant non modélisé d'une
    /// ligne de base.
    pub scope: Option<u8>,
    /// Ancre.
    pub anchor: Anchor,
    /// Nom local de l'élément ; `@xmlns:p` pour une déclaration d'espace de
    /// noms de la racine.
    pub name: String,
}

impl ExtraKey {
    /// Lit une clé ; `None` si elle est mal formée (nom vide, portée
    /// illisible).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let (scope, rest) = match text.strip_prefix("Baseline#") {
            Some(after) => {
                let (n, rest) = after.split_once('/')?;
                (Some(n.parse::<u8>().ok()?), rest)
            }
            None => (None, text),
        };
        let (anchor, name) = match rest.split_once('>') {
            Some(("^", name)) => (Anchor::Start, name),
            Some((a, name)) if is_name(a) => (Anchor::After(a.to_owned()), name),
            Some(_) => return None,
            None => (Anchor::None, rest),
        };
        let attribute = name.starts_with("@xmlns:") && scope.is_none() && anchor == Anchor::None;
        if !(is_name(name) || attribute) {
            return None;
        }
        Some(Self {
            scope,
            anchor,
            name: name.to_owned(),
        })
    }

    /// Vrai pour une déclaration d'espace de noms de la racine.
    #[must_use]
    pub fn is_attribute(&self) -> bool {
        self.name.starts_with('@')
    }
}

impl fmt::Display for ExtraKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(n) = self.scope {
            write!(f, "Baseline#{n}/")?;
        }
        match &self.anchor {
            Anchor::None => {}
            Anchor::Start => f.write_str("^>")?,
            Anchor::After(a) => write!(f, "{a}>")?,
        }
        f.write_str(&self.name)
    }
}

/// Nom XML simple (lettres, chiffres, `_`, `-`, `.`, ne commençant pas par
/// un chiffre ni `-` ou `.`), sans préfixe.
fn is_name(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// Parent des lignes de base d'une entité.
#[must_use]
pub const fn baseline_parent(parent: Parent) -> Option<Parent> {
    match parent {
        Parent::Task => Some(Parent::TaskBaseline),
        Parent::Resource => Some(Parent::ResourceBaseline),
        Parent::Assignment => Some(Parent::AssignmentBaseline),
        _ => None,
    }
}

/// Le nom est-il un élément du schéma de `parent` qu'un `Extras` peut
/// porter : classe P, `PredecessorLink` (lien vers un autre projet,
/// conservé entier, R-087), ou `LevelingDelay`/`LevelingDelayFormat` d'une
/// tâche ou d'une affectation (modélisés depuis J5, R-198 ; un `Extras`
/// hérité d'un projet enregistré avant J5 reste réécrit tant que le modèle
/// n'a pas de retard) ?
#[must_use]
pub fn is_preserved_name(parent: Parent, name: &str) -> bool {
    match sequence(parent).class(name) {
        Some(Class::Preserved) => true,
        Some(_) => {
            (parent == Parent::Task && name == "PredecessorLink")
                || (matches!(parent, Parent::Task | Parent::Assignment)
                    && matches!(name, "LevelingDelay" | "LevelingDelayFormat"))
        }
        None => false,
    }
}

/// Le nom peut-il servir d'ancre dans `parent` : élément du schéma toujours
/// réécrit (M, D, F) ?
#[must_use]
pub fn is_static_anchor(parent: Parent, name: &str) -> bool {
    sequence(parent)
        .slot(name)
        .is_some_and(crate::schema::Slot::always_written)
}

/// `Extras` des enfants capturés d'un élément, avec leurs clés (R-087) :
/// parcours unique, l'ancre courante est le dernier enfant du schéma
/// toujours réécrit ou capturé. `scope` : ligne de base n.
pub(crate) fn collect(children: &[Element<'_>], parent: Parent, scope: Option<u8>) -> Vec<ExtraEntry> {
    collect_with(children, parent, scope, |_| false)
}

/// Comme [`collect`], avec des éléments M? que l'on sait réécrits
/// (`written`) : ils servent aussi d'ancre. Projet : `Calendars`,
/// `Resources`, `Assignments` quand le modèle en a (ils sont alors
/// toujours réécrits, R-130).
pub(crate) fn collect_with(
    children: &[Element<'_>],
    parent: Parent,
    scope: Option<u8>,
    written: impl Fn(&str) -> bool,
) -> Vec<ExtraEntry> {
    let mut out = Vec::new();
    let mut anchor: Option<&str> = None;
    for child in children {
        let name = child.name.as_ref();
        let in_schema = sequence(parent).class(name).is_some();
        match child.raw {
            Some(raw) => {
                let key = ExtraKey {
                    scope,
                    anchor: if in_schema {
                        Anchor::None
                    } else {
                        anchor.map_or(Anchor::Start, |a| Anchor::After(a.to_owned()))
                    },
                    name: name.to_owned(),
                };
                out.push(ExtraEntry {
                    origin: ORIGIN.to_owned(),
                    key: key.to_string(),
                    payload: raw.to_owned(),
                });
                if in_schema {
                    anchor = Some(name);
                }
            }
            None => {
                if is_static_anchor(parent, name) || written(name) {
                    anchor = Some(name);
                }
            }
        }
    }
    out
}

/// Rang d'écriture d'une clé dans `parent` (profil observé), en demi-rangs :
/// nom du schéma `2 × rang`, ancré `2 × rang(ancre) + 1`, `^` −1,
/// déclaration d'espace de noms −2 ; `None` si la clé ne désigne aucun
/// emplacement (nom hors schéma sans ancre, ancre hors schéma, nom du schéma
/// non conservable).
fn slot_rank(parent: Parent, key: &ExtraKey) -> Option<i64> {
    let seq = sequence(parent);
    let rank = |name: &str| {
        seq.position(Profile::Observed, name)
            .and_then(|p| i64::try_from(p).ok())
            .and_then(|p| p.checked_mul(2))
    };
    if key.is_attribute() {
        return (parent == Parent::Project).then_some(-2);
    }
    match &key.anchor {
        Anchor::None => {
            if is_preserved_name(parent, &key.name) {
                rank(&key.name)
            } else {
                None
            }
        }
        Anchor::Start => (seq.class(&key.name).is_none()).then_some(-1),
        Anchor::After(a) => {
            if seq.class(&key.name).is_some() || seq.class(a).is_none() {
                return None;
            }
            rank(a).and_then(|r| r.checked_add(1))
        }
    }
}

/// Emplacement d'écriture d'un `Extras` d'une entité de `parent` : rang
/// dans l'entité, puis (portée de ligne de base) numéro et rang dans la
/// ligne de base. `None` : clé illisible ou sans emplacement (non écrite).
#[must_use]
pub fn placement(parent: Parent, key: &str) -> Option<(i64, u8, i64)> {
    let key = ExtraKey::parse(key)?;
    match key.scope {
        None => Some((slot_rank(parent, &key)?, 0, 0)),
        Some(n) => {
            let inner_parent = baseline_parent(parent)?;
            let inner = slot_rank(inner_parent, &key)?;
            let base = sequence(parent)
                .position(Profile::Observed, "Baseline")
                .and_then(|p| i64::try_from(p).ok())
                .and_then(|p| p.checked_mul(2))?;
            Some((base, n, inner))
        }
    }
}

/// Ordre canonique des `Extras` d'une entité (tri stable) : ceux d'origine
/// MSPDI par emplacement d'écriture, les autres à la fin dans leur ordre.
pub fn canonical_order(extras: &mut Extras, parent: Parent) {
    extras.0.sort_by_cached_key(|e| {
        if e.origin == ORIGIN {
            placement(parent, &e.key).map_or((1, 0, 0, 0), |(a, n, b)| (0, a, n, b))
        } else {
            (2, 0, 0, 0)
        }
    });
}

/// Profondeur maximale d'une charge réécrite.
const MAX_PAYLOAD_DEPTH: u32 = 64;

/// R-123 : une charge se réécrit telle quelle si c'est du XML bien formé fait
/// d'un seul élément racine de nom local `name` (préfixe admis), sans rien
/// autour (ni espace, ni commentaire, ni déclaration, ni DOCTYPE), sans
/// caractère interdit par XML 1.0, de profondeur au plus 64 : elle est alors
/// relue à l'identique (la capture va de `<` à la fin de la balise
/// fermante).
#[must_use]
pub fn valid_payload(name: &str, payload: &str) -> bool {
    if !payload.chars().all(crate::write::is_xml_char) || payload.trim() != payload {
        return false;
    }
    let mut reader = Reader::from_str(payload);
    reader.config_mut().check_end_names = true;
    let mut depth: u32 = 0;
    let mut roots: u32 = 0;
    loop {
        let Ok(event) = reader.read_event() else {
            return false;
        };
        let root = |e: &quick_xml::events::BytesStart<'_>, roots: &mut u32| {
            *roots = roots.saturating_add(1);
            *roots == 1 && e.local_name().as_ref() == name
        };
        match event {
            Event::Start(e) => {
                if (depth == 0 && !root(&e, &mut roots)) || !attributes_well_formed(&e) {
                    return false;
                }
                depth = depth.saturating_add(1);
                if depth > MAX_PAYLOAD_DEPTH {
                    return false;
                }
            }
            Event::Empty(e) => {
                if (depth == 0 && !root(&e, &mut roots)) || !attributes_well_formed(&e) {
                    return false;
                }
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            Event::Text(t) => {
                if depth == 0 && !t.xml10_content().trim().is_empty() {
                    return false;
                }
            }
            Event::CData(_) | Event::GeneralRef(_) | Event::Comment(_) if depth == 0 => return false,
            // R-087 : seules les entités prédéfinies et les références de
            // caractère valides sont du XML bien formé sans DTD ; une entité
            // inconnue (`&nbsp;`, tolérée à la lecture) rendrait le document
            // illisible par MS Project.
            Event::GeneralRef(r) => {
                let known = if r.is_char_ref() {
                    r.resolve_char_ref()
                        .ok()
                        .flatten()
                        .is_some_and(crate::write::is_xml_char)
                } else {
                    matches!(&*r, "amp" | "lt" | "gt" | "quot" | "apos")
                };
                if !known {
                    return false;
                }
            }
            Event::Decl(_) | Event::DocType(_) | Event::PI(_) => return false,
            Event::Eof => return depth == 0 && roots == 1,
            _ => {}
        }
    }
}

/// R-087 : attributs lisibles, sans entité inconnue ni caractère interdit.
fn attributes_well_formed(e: &quick_xml::events::BytesStart<'_>) -> bool {
    e.attributes().all(|a| {
        a.ok()
            .and_then(|a| a.normalized_value(quick_xml::XmlVersion::Implicit1_0).ok())
            .is_some_and(|v| v.chars().all(crate::write::is_xml_char))
    })
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{Anchor, ExtraKey, placement};
    use crate::schema::Parent;

    #[test]
    fn r087_keys_parse_and_format() {
        for text in [
            "ExtendedAttribute",
            "DurationFormat>FreeformDurationFormat",
            "^>Zzz",
            "Baseline#3/TimephasedData",
            "Baseline#0/Number>Foo",
            "@xmlns:p",
        ] {
            let key = ExtraKey::parse(text).unwrap();
            assert_eq!(key.to_string(), text);
        }
        let k = ExtraKey::parse("Recurring>Zzz").unwrap();
        assert_eq!(k.anchor, Anchor::After("Recurring".to_owned()));
        assert_eq!(k.name, "Zzz");
        for bad in [
            "",
            ">x",
            "a>",
            "Baseline#x/Foo",
            "Baseline#300/Foo",
            "a b",
            "1a",
            "x>y>z",
            "@xmlns",
        ] {
            assert!(ExtraKey::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn r087_placements() {
        let task = |k: &str| placement(Parent::Task, k);
        // ExtendedAttribute (P) après CommitmentType dans le profil observé.
        let ea = task("ExtendedAttribute").unwrap();
        let ct = task("CommitmentType").unwrap();
        assert!(ea.0 > ct.0);
        // Ancré : juste après son ancre.
        let df = task("DurationFormat>FreeformDurationFormat").unwrap();
        assert_eq!(df.0 % 2, 1);
        assert!(task("^>Zzz").unwrap().0 < 0);
        // Sans emplacement : nom M, ancre hors schéma, nom du schéma ancré.
        assert_eq!(task("Name"), None);
        assert_eq!(task("Zzz"), None);
        assert_eq!(task("Yyy>Zzz"), None);
        assert_eq!(task("Name>Contact"), None);
        // Lien conservé entier (autre projet).
        assert!(task("PredecessorLink").is_some());
        // Portée d'une ligne de base.
        let b = task("Baseline#2/TimephasedData").unwrap();
        assert_eq!(b.1, 2);
        assert_eq!(placement(Parent::Calendar, "Baseline#2/TimephasedData"), None);
        assert_eq!(placement(Parent::Project, "@xmlns:p"), Some((-2, 0, 0)));
        assert_eq!(task("@xmlns:p"), None);
    }

    #[test]
    fn r123_payload_validation() {
        use super::valid_payload;
        assert!(valid_payload("Name", "<Name>x</Name>"));
        assert!(valid_payload("X", "<X/>"));
        assert!(valid_payload(
            "ExtendedAttribute",
            "<p:ExtendedAttribute><p:FieldID>1</p:FieldID></p:ExtendedAttribute>"
        ));
        assert!(valid_payload("Notes", "<Notes>a &amp; b<!-- c --></Notes>"));
        for (name, bad) in [
            ("Name", "<Name>x</Name><Evil/>"),
            ("Name", "<Other>x</Other>"),
            ("Name", "<Name>x</Nom>"),
            ("Name", "<Name>x"),
            ("Name", "texte"),
            ("Name", "x<Name/>"),
            ("Name", ""),
            ("Name", "<?xml version=\"1.0\"?><Name/>"),
            ("Name", "<!DOCTYPE a><Name/>"),
            ("Name", " <Name/>"),
            ("Name", "<!-- c --><Name/>"),
            ("Name", "<Name>\u{1}</Name>"),
        ] {
            assert!(!valid_payload(name, bad), "{bad}");
        }
        let deep = format!("<A>{}{}</A>", "<b>".repeat(70), "</b>".repeat(70));
        assert!(!valid_payload("A", &deep));
    }

    /// R-087 : entités d'une charge — prédéfinies et références de caractère
    /// valides admises, toute autre refusée (XML mal formé sans DTD).
    #[test]
    fn r087_payload_entities() {
        use super::valid_payload;
        for good in [
            "<H>a &amp; &lt;b&gt; &quot;c&quot; &apos;</H>",
            "<H>&#233;&#xE9;&#x1F600;</H>",
            "<H><i a=\"&amp;\">&#10;</i></H>",
        ] {
            assert!(valid_payload("H", good), "{good}");
        }
        for bad in [
            "<H>a&nbsp;b</H>",
            "<H><i>&eacute;</i></H>",
            "<H>&#1;</H>",
            "<H>&#xD800;</H>",
            "<H>&#99999999;</H>",
            "<H><i a=\"&nbsp;\"/></H>",
            "<H a=\"&#1;\"/>",
        ] {
            assert!(!valid_payload("H", bad), "{bad}");
        }
    }
}
