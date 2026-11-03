//! Champs personnalisés en MSPDI (R-265 lecture, R-266 écriture ; ADR-0019
//! D8).
//!
//! Le lecteur XML capture les `ExtendedAttribute` (classe P) en `Extras`
//! verbatim (R-087) : la définition des champs dans le conteneur
//! `Project/ExtendedAttributes`, les valeurs dans chaque `Task`. Ce module
//! **remonte** dans le modèle ce que PlannerCraft sait représenter, et laisse le
//! reste en `Extras` à l'identique :
//!
//! - table `FieldID` ⇄ [`CustomFieldId`] figée depuis S90 (`PjField`) :
//!   Texte1-30, Nombre1-20, Date1-10, Indicateur1-20 ;
//! - définition **simple** : enfants parmi `FieldID`, `FieldName`, `Alias`
//!   (≤ 50 caractères), `Guid`, `SecondaryPID`, `SecondaryGuid`, chacun au
//!   plus une fois, sans attribut ni sous-élément ⇒ `CustomFieldDef { alias }`
//!   (`FieldName` et les identifiants internes ne sont pas gardés : le
//!   champ est réécrit avec son nom canonique, hypothèse) ;
//! - champ **étranger** : une de ses définitions n'est pas simple
//!   (`Formula`, table de choix `Ltuid`/`ValueList`/`ValuelistUID`, autre
//!   élément), ou il est défini deux fois, ou le conteneur n'est pas un
//!   `<ExtendedAttributes>` sans attribut ni préfixe : sa définition et ses
//!   valeurs restent en `Extras`, octet pour octet ;
//! - valeur d'une tâche (`FieldID` et `Value` seuls) d'un champ modélisé non
//!   étranger : lue selon le type (texte tel quel, ≤ 255 caractères ;
//!   nombre décimal au point arrondi au dix-millième, demi éloigné de zéro,
//!   R-001 ; date ISO, R-075 ; indicateur `0`/`1`) ; vide (texte vide,
//!   indicateur 0) ⇒ ni gardée ni stockée ; illisible ⇒ gardée en `Extras`
//!   et diagnostic `mspdi.custom_unreadable` (info) ; seconde valeur du même
//!   champ dans une tâche ⇒ gardée en `Extras`.
//!
//! Conteneur après la remontée : rien n'est remonté ⇒ inchangé (octet pour
//! octet) ; sinon forme canonique `<ExtendedAttributes>` + définitions
//! restantes concaténées (tranches exactes) + `</ExtendedAttributes>`,
//! retiré s'il ne reste rien. À l'écriture, si le modèle a des définitions
//! à écrire, le conteneur est écrit par le modèle (définitions dans l'ordre
//! des `FieldID`), suivi des définitions restantes des `Extras` ; sinon le
//! conteneur des `Extras` est réécrit tel quel. Les mêmes fonctions servent
//! à la lecture et à [`crate::normalize`] : `read(write(p)).project ==
//! normalize(p)` (P62).

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use plannercraft_core::model::custom::{
    CUSTOM_NUMBER_SCALE, MAX_CUSTOM_ALIAS_CHARS, MAX_CUSTOM_NUMBER_ABS, MAX_CUSTOM_TEXT_CHARS,
};
use plannercraft_core::{
    CustomFieldDef, CustomFieldId, CustomKind, CustomValue, ExtraEntry, Extras, Project,
};
use quick_xml::Reader;
use quick_xml::events::Event;

use crate::extras::{Anchor, ExtraKey, ORIGIN, valid_payload};
use crate::scalar::{parse_bool, parse_datetime, parse_int};

/// Nom du conteneur des définitions (enfant de `Project`).
pub const CONTAINER: &str = "ExtendedAttributes";
/// Nom d'une définition ou d'une valeur.
pub const ATTRIBUTE: &str = "ExtendedAttribute";

/// Enfants admis d'une définition simple.
const DEF_CHILDREN: [&str; 6] = [
    "FieldID",
    "FieldName",
    "Alias",
    "Guid",
    "SecondaryPID",
    "SecondaryGuid",
];

/// Longueur maximale d'un texte de champ lu (règle 10 : un nombre ou une
/// date plus longs sont illisibles sans être analysés).
const MAX_SCALAR_CHARS: usize = 64;

/// R-265 : `FieldID` d'un champ (S90, `PjField`, champs de tâche) :
/// Texte1 188743731, Texte2 à Texte5 de 3 en 3, Texte6 à Texte10
/// 188743746 à 188743750, Texte11 à Texte30 188743997 à 188744016 ;
/// Nombre1 à Nombre5 188743767 à 188743771, Nombre6 à Nombre20 188743982 à
/// 188743996 ; Date1 à Date10 188743945 à 188743954 ; Indicateur1 à
/// Indicateur10 188743752 à 188743761, Indicateur11 à Indicateur20
/// 188743972 à 188743981.
#[must_use]
pub fn field_id(id: CustomFieldId) -> i64 {
    let n = i64::from(id.index());
    let (base, offset): (i64, i64) = match (id.kind(), id.index()) {
        (CustomKind::Text, 1..=5) => (188_743_731, n.saturating_sub(1).saturating_mul(3)),
        (CustomKind::Text, 6..=10) => (188_743_746, n.saturating_sub(6)),
        (CustomKind::Text, _) => (188_743_997, n.saturating_sub(11)),
        (CustomKind::Number, 1..=5) => (188_743_767, n.saturating_sub(1)),
        (CustomKind::Number, _) => (188_743_982, n.saturating_sub(6)),
        (CustomKind::Date, _) => (188_743_945, n.saturating_sub(1)),
        (CustomKind::Flag, 1..=10) => (188_743_752, n.saturating_sub(1)),
        (CustomKind::Flag, _) => (188_743_972, n.saturating_sub(11)),
    };
    base.saturating_add(offset)
}

/// Champ d'un `FieldID` ; `None` pour tout autre champ (coût, durée,
/// ressource, code hiérarchique…).
#[must_use]
pub fn from_field_id(value: i64) -> Option<CustomFieldId> {
    let at = |kind: CustomKind, first: i64, base: u8| {
        let n = value.checked_sub(first).and_then(|d| u8::try_from(d).ok())?;
        CustomFieldId::new(kind, n.checked_add(base)?)
    };
    match value {
        188_743_731..=188_743_743 => {
            let d = value.checked_sub(188_743_731)?;
            if d.checked_rem(3)? != 0 {
                return None;
            }
            let k = u8::try_from(d.checked_div(3)?).ok()?;
            CustomFieldId::new(CustomKind::Text, k.checked_add(1)?)
        }
        188_743_746..=188_743_750 => at(CustomKind::Text, 188_743_746, 6),
        188_743_997..=188_744_016 => at(CustomKind::Text, 188_743_997, 11),
        188_743_767..=188_743_771 => at(CustomKind::Number, 188_743_767, 1),
        188_743_982..=188_743_996 => at(CustomKind::Number, 188_743_982, 6),
        188_743_945..=188_743_954 => at(CustomKind::Date, 188_743_945, 1),
        188_743_752..=188_743_761 => at(CustomKind::Flag, 188_743_752, 1),
        188_743_972..=188_743_981 => at(CustomKind::Flag, 188_743_972, 11),
        _ => None,
    }
}

/// Nom canonique écrit dans `FieldName` (S90 : « Text1 », « Number1 »…).
#[must_use]
pub fn field_name(id: CustomFieldId) -> String {
    id.canonical_name()
}

/// Enfant direct d'un élément analysé à plat.
#[derive(Debug)]
struct FlatChild {
    /// Nom local.
    name: String,
    /// Texte (entités résolues).
    text: String,
    /// Tranche exacte dans la charge.
    range: Range<usize>,
    /// Sans attribut ni sous-élément.
    simple: bool,
}

/// Élément analysé à plat : sa racine et ses enfants directs.
#[derive(Debug)]
struct Flat {
    /// Nom qualifié de la racine (préfixe compris).
    root: String,
    /// La racine porte des attributs.
    root_attrs: bool,
    /// Texte non blanc directement sous la racine.
    mixed: bool,
    /// Enfants directs.
    children: Vec<FlatChild>,
}

/// Position courante du lecteur, en octets.
fn position(reader: &Reader<&[u8]>) -> Option<usize> {
    usize::try_from(reader.buffer_position()).ok()
}

/// Analyse une charge (un seul élément bien formé) à plat ; `None` si elle
/// est illisible (entité inconnue, mal formée).
#[allow(
    clippy::too_many_lines,
    reason = "un bras par événement XML, lisible d'un tenant"
)]
fn flat(payload: &str) -> Option<Flat> {
    let mut reader = Reader::from_str(payload);
    reader.config_mut().check_end_names = true;
    let mut depth: u32 = 0;
    let mut out: Option<Flat> = None;
    let mut closed = false;
    let mut current: Option<FlatChild> = None;
    loop {
        let start = position(&reader)?;
        let event = reader.read_event().ok()?;
        let end = position(&reader)?;
        match event {
            Event::Start(e) | Event::Empty(e) if depth == 0 => {
                if out.is_some() || closed {
                    return None;
                }
                out = Some(Flat {
                    root: e.name().as_ref().to_owned(),
                    root_attrs: e.attributes().next().is_some(),
                    mixed: false,
                    children: Vec::new(),
                });
                // Élément vide : racine déjà fermée.
                if end > start && payload.get(start..end)?.ends_with("/>") {
                    closed = true;
                } else {
                    depth = 1;
                }
            }
            Event::Start(e) => {
                if depth == 1 {
                    current = Some(FlatChild {
                        name: e.local_name().as_ref().to_owned(),
                        text: String::new(),
                        range: start..start,
                        simple: e.attributes().next().is_none(),
                    });
                } else if let Some(c) = current.as_mut() {
                    c.simple = false;
                }
                depth = depth.checked_add(1)?;
            }
            Event::Empty(e) => {
                if depth == 1 {
                    out.as_mut()?.children.push(FlatChild {
                        name: e.local_name().as_ref().to_owned(),
                        text: String::new(),
                        range: start..end,
                        simple: e.attributes().next().is_none(),
                    });
                } else if let Some(c) = current.as_mut() {
                    c.simple = false;
                }
            }
            Event::End(_) => {
                depth = depth.checked_sub(1)?;
                if depth == 1 {
                    let mut c = current.take()?;
                    c.range = c.range.start..end;
                    out.as_mut()?.children.push(c);
                } else if depth == 0 {
                    closed = true;
                }
            }
            Event::Text(t) => {
                let text = t.xml10_content();
                match depth {
                    1 if !text.trim().is_empty() => out.as_mut()?.mixed = true,
                    2 => current.as_mut()?.text.push_str(&text),
                    _ => {}
                }
            }
            Event::CData(t) => match depth {
                1 => out.as_mut()?.mixed = true,
                2 => current.as_mut()?.text.push_str(&t.xml10_content()),
                _ => {}
            },
            Event::GeneralRef(r) => {
                let resolved = if r.is_char_ref() {
                    r.resolve_char_ref().ok().flatten().map(String::from)
                } else {
                    quick_xml::escape::resolve_xml_entity(&r).map(str::to_owned)
                }?;
                match depth {
                    1 => out.as_mut()?.mixed = true,
                    2 => current.as_mut()?.text.push_str(&resolved),
                    _ => {}
                }
            }
            Event::Decl(_) | Event::DocType(_) => return None,
            Event::Comment(_) | Event::PI(_) => {}
            Event::Eof => return if depth == 0 && closed { out } else { None },
        }
    }
}

/// Nature d'une définition du conteneur.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Def {
    /// Définition simple d'un champ modélisé.
    Simple {
        /// Champ.
        id: CustomFieldId,
        /// Alias (non vide).
        alias: Option<String>,
    },
    /// Définition d'un champ modélisé que PlannerCraft ne représente pas
    /// (formule, table de choix…).
    Foreign(CustomFieldId),
    /// Tout autre enfant (autre champ, code hiérarchique, illisible).
    Other,
}

/// Classe une définition (charge d'un enfant du conteneur).
fn classify_def(slice: &str) -> Def {
    let Some(f) = flat(slice) else {
        return Def::Other;
    };
    if f.root != ATTRIBUTE && !f.root.ends_with(":ExtendedAttribute") {
        return Def::Other;
    }
    let mut ids = f.children.iter().filter(|c| c.name == "FieldID");
    let Some(id) = ids
        .next()
        .and_then(|c| parse_int(&c.text))
        .and_then(from_field_id)
    else {
        return Def::Other;
    };
    let once = |name: &str| f.children.iter().filter(|c| c.name == name).count() <= 1;
    let alias = f
        .children
        .iter()
        .find(|c| c.name == "Alias")
        .map(|c| c.text.clone());
    let simple = f.root == ATTRIBUTE
        && !f.root_attrs
        && !f.mixed
        && ids.next().is_none()
        && f.children
            .iter()
            .all(|c| c.simple && DEF_CHILDREN.contains(&c.name.as_str()))
        && DEF_CHILDREN.iter().all(|n| once(n))
        && alias
            .as_ref()
            .is_none_or(|a| a.chars().count() <= MAX_CUSTOM_ALIAS_CHARS);
    if simple {
        Def::Simple {
            id,
            alias: alias.filter(|a| !a.is_empty()),
        }
    } else {
        Def::Foreign(id)
    }
}

/// Conteneur des définitions, analysé.
struct Container {
    /// `<ExtendedAttributes>` sans préfixe, attribut ni texte mêlé.
    plain: bool,
    /// Enfants : tranche et nature.
    children: Vec<(Range<usize>, Def)>,
}

impl Container {
    /// Analyse la charge du conteneur.
    fn parse(payload: &str) -> Self {
        let Some(f) = flat(payload) else {
            return Self {
                plain: false,
                children: Vec::new(),
            };
        };
        let plain = f.root == CONTAINER && !f.root_attrs && !f.mixed;
        let children = f
            .children
            .into_iter()
            .map(|c| {
                let def = if c.name == ATTRIBUTE {
                    payload.get(c.range.clone()).map_or(Def::Other, classify_def)
                } else {
                    Def::Other
                };
                (c.range, def)
            })
            .collect();
        Self { plain, children }
    }

    /// Champs étrangers : définis par une définition non simple, ou deux
    /// fois ; tous si le conteneur n'est pas simple.
    fn foreign(&self) -> BTreeSet<CustomFieldId> {
        if !self.plain {
            return CustomFieldId::all().collect();
        }
        let mut count: BTreeMap<CustomFieldId, u32> = BTreeMap::new();
        let mut out = BTreeSet::new();
        for (_, def) in &self.children {
            match def {
                Def::Simple { id, .. } => {
                    let n = count.entry(*id).or_insert(0);
                    *n = n.saturating_add(1);
                    if *n > 1 {
                        out.insert(*id);
                    }
                }
                Def::Foreign(id) => {
                    out.insert(*id);
                }
                Def::Other => {}
            }
        }
        out
    }
}

/// Vrai pour un `Extras` MSPDI de clé `name` (sans ancre ni portée).
fn is_key(e: &ExtraEntry, name: &str) -> bool {
    e.origin == ORIGIN
        && (e.key == name
            || ExtraKey::parse(&e.key)
                .is_some_and(|k| k.name == name && k.anchor == Anchor::None && k.scope.is_none()))
}

/// Comme [`is_key`], avec une charge réécrivable (R-123).
fn is_entry(e: &ExtraEntry, name: &str) -> bool {
    is_key(e, name) && valid_payload(name, &e.payload)
}

/// Index du conteneur des définitions dans les `Extras` du projet : `Ok`
/// avec `None` s'il n'y en a pas, `Err(())` s'il y en a plusieurs (tous les
/// champs sont alors étrangers).
fn container_index(extras: &Extras) -> Result<Option<usize>, ()> {
    let mut found = None;
    for (i, e) in extras.0.iter().enumerate() {
        if is_entry(e, CONTAINER) {
            if found.is_some() {
                return Err(());
            }
            found = Some(i);
        }
    }
    Ok(found)
}

/// Champs étrangers d'un projet (R-265) : définis dans le fichier par une
/// formule, une table de choix ou une définition que PlannerCraft ne représente
/// pas ; leurs valeurs ne se modifient pas (R-264).
#[must_use]
pub fn foreign_fields(project: &Project) -> BTreeSet<CustomFieldId> {
    foreign_in(&project.extras)
}

/// Champs étrangers d'après les `Extras` du projet.
pub(crate) fn foreign_in(extras: &Extras) -> BTreeSet<CustomFieldId> {
    match container_index(extras) {
        Err(()) => CustomFieldId::all().collect(),
        Ok(None) => BTreeSet::new(),
        Ok(Some(i)) => extras
            .0
            .get(i)
            .map(|e| Container::parse(&e.payload).foreign())
            .unwrap_or_default(),
    }
}

/// Champs étrangers d'après les charges brutes des conteneurs lus
/// (lecture) : plusieurs conteneurs ⇒ tous.
pub(crate) fn foreign_in_payloads(payloads: &[&str]) -> BTreeSet<CustomFieldId> {
    let valid: Vec<&str> = payloads
        .iter()
        .copied()
        .filter(|p| valid_payload(CONTAINER, p))
        .collect();
    match valid.as_slice() {
        [] => BTreeSet::new(),
        [one] => Container::parse(one).foreign(),
        _ => CustomFieldId::all().collect(),
    }
}

/// R-265 : remonte les définitions simples du conteneur des `Extras` du
/// projet dans `defs` (alias), sauf celles des champs de `written` (le
/// modèle les écrit, elles sont remplacées) ; le conteneur devient sa forme
/// canonique (définitions restantes) s'il y a eu une remontée ou si
/// `written` n'est pas vide, retiré s'il ne reste rien.
pub(crate) fn lift_container(
    extras: &mut Extras,
    defs: &mut BTreeMap<CustomFieldId, CustomFieldDef>,
    written: &BTreeSet<CustomFieldId>,
) {
    let Ok(Some(index)) = container_index(extras) else {
        return;
    };
    let Some(entry) = extras.0.get(index) else {
        return;
    };
    let container = Container::parse(&entry.payload);
    if !container.plain {
        return;
    }
    let foreign = container.foreign();
    let mut lifted = !written.is_empty();
    let mut residue = String::new();
    let mut kept = 0_usize;
    for (range, def) in &container.children {
        match def {
            Def::Simple { id, alias } if !foreign.contains(id) => {
                lifted = true;
                if !written.contains(id)
                    && let Some(a) = alias
                {
                    defs.insert(
                        *id,
                        CustomFieldDef {
                            alias: Some(a.clone()),
                        },
                    );
                }
            }
            _ => {
                residue.push_str(entry.payload.get(range.clone()).unwrap_or_default());
                kept = kept.saturating_add(1);
            }
        }
    }
    if !lifted {
        return;
    }
    if kept == 0 {
        extras.0.remove(index);
    } else if let Some(e) = extras.0.get_mut(index) {
        e.payload = format!("<{CONTAINER}>{residue}</{CONTAINER}>");
    }
}

/// Valeur d'une tâche, lue.
enum ValueRead {
    /// Champ non modélisé, étranger, ou valeur d'une autre forme
    /// (`ValueGUID`…) : laissée en `Extras`, sans diagnostic.
    Leave,
    /// Valeur vide : ni gardée ni stockée.
    Empty,
    /// Valeur lue.
    Value(CustomFieldId, CustomValue),
    /// Valeur illisible d'un champ modélisé : gardée, diagnostic.
    Unreadable,
}

/// R-265 : nombre décimal au point (`[+-]chiffres[.chiffres]`) en
/// dix-millièmes, arrondi demi éloigné de zéro (R-001) ; exposant,
/// virgule, valeur vide ou au-delà de ±10^14 : `None`.
#[must_use]
pub fn parse_number(s: &str) -> Option<i64> {
    let t = s.trim();
    if t.chars().count() > MAX_SCALAR_CHARS {
        return None;
    }
    let (negative, body) = match t.as_bytes().first() {
        Some(b'-') => (true, t.get(1..)?),
        Some(b'+') => (false, t.get(1..)?),
        _ => (false, t),
    };
    let (int, frac) = body.split_once('.').unwrap_or((body, ""));
    if (int.is_empty() && frac.is_empty()) || !int.bytes().chain(frac.bytes()).all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut value: i128 = 0;
    for b in int.bytes() {
        value = value
            .checked_mul(10)?
            .checked_add(i128::from(b.checked_sub(b'0')?))?;
    }
    let mut digits = frac.bytes();
    for _ in 0..4 {
        let d = digits.next().map_or(Some(0), |b| b.checked_sub(b'0'))?;
        value = value.checked_mul(10)?.checked_add(i128::from(d))?;
    }
    // R-001 : la cinquième décimale décide (≥ 5 ⇒ éloigné de zéro).
    if digits.next().is_some_and(|b| b >= b'5') {
        value = value.checked_add(1)?;
    }
    if negative {
        value = value.checked_neg()?;
    }
    let v = i64::try_from(value).ok()?;
    (v.unsigned_abs() <= MAX_CUSTOM_NUMBER_ABS.unsigned_abs()).then_some(v)
}

/// R-266 : nombre en dix-millièmes écrit au point, sans zéro final
/// (`125 000` ⇒ « 12.5 », `30 000` ⇒ « 3 »).
#[must_use]
pub fn format_number(v: i64) -> String {
    let scale = CUSTOM_NUMBER_SCALE.unsigned_abs();
    let abs = v.unsigned_abs();
    let (int, frac) = (
        abs.checked_div(scale).unwrap_or(0),
        abs.checked_rem(scale).unwrap_or(0),
    );
    let sign = if v < 0 { "-" } else { "" };
    if frac == 0 {
        return format!("{sign}{int}");
    }
    let digits = format!("{frac:04}");
    format!("{sign}{int}.{}", digits.trim_end_matches('0'))
}

/// Forme courante d'une valeur, lue sans analyseur XML (chemin rapide) :
/// `<ExtendedAttribute><FieldID>n</FieldID><Value>texte</Value></ExtendedAttribute>`,
/// espaces admis entre les balises, texte sans `<` ni `&` ; `<Value/>`
/// admis. `None` : autre forme, l'analyse complète décide.
fn quick_value(payload: &str) -> Option<(i64, &str)> {
    let rest = payload.strip_prefix("<ExtendedAttribute>")?.trim_start();
    let rest = rest.strip_prefix("<FieldID>")?;
    let (digits, rest) = rest.split_once("</FieldID>")?;
    let id = parse_int(digits)?;
    let rest = rest.trim_start();
    let (text, rest) = if let Some(r) = rest.strip_prefix("<Value/>") {
        ("", r)
    } else {
        let r = rest.strip_prefix("<Value>")?;
        r.split_once("</Value>")?
    };
    if text.contains(['<', '&', '\r']) {
        return None;
    }
    (rest.trim() == "</ExtendedAttribute>").then_some((id, text))
}

/// Lit la valeur d'une tâche (charge d'un `ExtendedAttribute`).
fn read_value(payload: &str, foreign: &BTreeSet<CustomFieldId>) -> ValueRead {
    if let Some((fid, text)) = quick_value(payload) {
        return match from_field_id(fid) {
            Some(id) if !foreign.contains(&id) => typed_value(id, text),
            _ => ValueRead::Leave,
        };
    }
    let Some(f) = flat(payload) else {
        return ValueRead::Leave;
    };
    let Some(id) = f
        .children
        .iter()
        .find(|c| c.name == "FieldID")
        .and_then(|c| parse_int(&c.text))
        .and_then(from_field_id)
    else {
        return ValueRead::Leave;
    };
    if foreign.contains(&id) {
        return ValueRead::Leave;
    }
    let shape = f.root == ATTRIBUTE
        && !f.root_attrs
        && !f.mixed
        && f.children
            .iter()
            .all(|c| c.simple && matches!(c.name.as_str(), "FieldID" | "Value"))
        && f.children.iter().filter(|c| c.name == "FieldID").count() == 1
        && f.children.iter().filter(|c| c.name == "Value").count() <= 1;
    if !shape {
        return ValueRead::Leave;
    }
    let Some(text) = f
        .children
        .iter()
        .find(|c| c.name == "Value")
        .map(|c| c.text.as_str())
    else {
        return ValueRead::Unreadable;
    };
    typed_value(id, text)
}

/// Valeur `text` lue selon le type du champ `id`.
fn typed_value(id: CustomFieldId, text: &str) -> ValueRead {
    let value = match id.kind() {
        CustomKind::Text => {
            (text.chars().count() <= MAX_CUSTOM_TEXT_CHARS).then(|| CustomValue::Text(text.to_owned()))
        }
        CustomKind::Number => parse_number(text).map(CustomValue::Number),
        CustomKind::Date => (text.chars().count() <= MAX_SCALAR_CHARS)
            .then(|| parse_datetime(text).ok())
            .flatten()
            .map(|p| CustomValue::Date(p.moment)),
        CustomKind::Flag => parse_bool(text).map(CustomValue::Flag),
    };
    match value {
        None => ValueRead::Unreadable,
        Some(v) if v.is_empty() => ValueRead::Empty,
        Some(v) => ValueRead::Value(id, v),
    }
}

/// R-265 : remonte les valeurs d'une tâche gardées en `Extras` dans
/// `custom` (voir le module) ; `unreadable` reçoit la charge de chaque
/// valeur illisible d'un champ modélisé (gardée).
pub(crate) fn lift_task(
    extras: &mut Vec<ExtraEntry>,
    custom: &mut BTreeMap<CustomFieldId, CustomValue>,
    foreign: &BTreeSet<CustomFieldId>,
    mut unreadable: impl FnMut(&str),
) {
    let mut seen: BTreeSet<CustomFieldId> = custom.keys().copied().collect();
    extras.retain(|e| {
        // Charge non réécrivable : relue telle quelle par `flat` (entité
        // inconnue ⇒ laissée) ; une valeur gardée qui ne le serait pas est
        // jetée par l'écriture (R-123) comme tout `Extras`.
        if !is_key(e, ATTRIBUTE) {
            return true;
        }
        match read_value(&e.payload, foreign) {
            ValueRead::Leave => true,
            ValueRead::Unreadable => {
                unreadable(&e.payload);
                true
            }
            ValueRead::Empty => false,
            ValueRead::Value(id, v) => {
                if seen.insert(id) {
                    custom.insert(id, v);
                    false
                } else {
                    true
                }
            }
        }
    });
}

/// Retire des `Extras` d'une tâche les valeurs du champ `id` (une valeur
/// posée ou effacée par commande remplace celle du fichier, R-264).
pub fn remove_task_values(extras: &mut Extras, id: CustomFieldId) {
    let wanted = field_id(id);
    extras.0.retain(|e| {
        if !is_entry(e, ATTRIBUTE) {
            return true;
        }
        let fid = flat(&e.payload).and_then(|f| {
            f.children
                .iter()
                .find(|c| c.name == "FieldID")
                .and_then(|c| parse_int(&c.text))
        });
        fid != Some(wanted)
    });
}

/// Plan d'écriture des champs personnalisés (R-266).
pub(crate) struct WritePlan {
    /// Champs étrangers (conteneur des `Extras`) : rien n'en est écrit par
    /// le modèle.
    pub(crate) foreign: BTreeSet<CustomFieldId>,
    /// Champs dont le modèle écrit la définition : un alias ou au moins une
    /// valeur, hors étrangers, dans l'ordre des `FieldID`.
    pub(crate) written: Vec<CustomFieldId>,
    /// Définitions restantes du conteneur des `Extras`, réécrites après
    /// celles du modèle (vide si le modèle n'écrit rien : le conteneur est
    /// alors réécrit tel quel).
    pub(crate) residue: Vec<String>,
}

/// Champs du modèle à écrire : alias non vide ou valeur non vide, hors
/// `foreign`.
pub(crate) fn written_fields(
    project: &Project,
    foreign: &BTreeSet<CustomFieldId>,
) -> BTreeSet<CustomFieldId> {
    let mut out: BTreeSet<CustomFieldId> = project
        .custom_fields
        .iter()
        .filter(|(_, d)| d.alias.as_deref().is_some_and(|a| !a.is_empty()))
        .map(|(id, _)| *id)
        .collect();
    for t in project.summary_task.iter().chain(project.tasks.iter()) {
        out.extend(t.custom.iter().filter(|(_, v)| !v.is_empty()).map(|(id, _)| *id));
    }
    out.retain(|id| !foreign.contains(id));
    out
}

impl WritePlan {
    /// Plan d'un projet ; `container` : charge du conteneur des `Extras`
    /// qui sera écrit (déjà validée), s'il y en a un seul.
    pub(crate) fn new(project: &Project, container: Option<&str>, several: bool) -> Self {
        let parsed = container.map(Container::parse);
        let foreign = if several {
            CustomFieldId::all().collect()
        } else {
            parsed.as_ref().map(Container::foreign).unwrap_or_default()
        };
        let set = written_fields(project, &foreign);
        let mut written: Vec<CustomFieldId> = set.iter().copied().collect();
        written.sort_by_key(|id| field_id(*id));
        let mut residue = Vec::new();
        if !written.is_empty()
            && let (Some(c), Some(payload)) = (&parsed, container)
            && c.plain
        {
            for (range, def) in &c.children {
                let superseded = matches!(def, Def::Simple { id, .. } if set.contains(id));
                if !superseded && let Some(s) = payload.get(range.clone()) {
                    residue.push(s.to_owned());
                }
            }
        }
        Self {
            foreign,
            written,
            residue,
        }
    }

    /// Le modèle écrit le conteneur (et remplace celui des `Extras`).
    pub(crate) fn owns_container(&self) -> bool {
        !self.written.is_empty()
    }
}

/// Valeurs d'une tâche à écrire, dans l'ordre des `FieldID`.
pub(crate) fn task_values<'t>(
    custom: &'t BTreeMap<CustomFieldId, CustomValue>,
    foreign: &BTreeSet<CustomFieldId>,
) -> Vec<(CustomFieldId, &'t CustomValue)> {
    let mut out: Vec<(CustomFieldId, &CustomValue)> = custom
        .iter()
        .filter(|(id, v)| !foreign.contains(id) && !v.is_empty())
        .map(|(id, v)| (*id, v))
        .collect();
    out.sort_by_key(|(id, _)| field_id(*id));
    out
}

/// P62 : image des champs personnalisés par l'aller-retour (appelée par
/// [`crate::normalize`] après la normalisation des `Extras`) — valeurs et
/// définitions des champs étrangers retirées, textes et alias nettoyés et
/// tronqués (255, 50), vides retirés, définitions simples du conteneur
/// remontées, valeurs lisibles des `Extras` remontées.
pub(crate) fn normalize(p: &mut Project, clean: impl Fn(&str, usize) -> String) {
    let foreign = foreign_in(&p.extras);
    p.custom_fields.retain(|id, d| {
        d.alias = d
            .alias
            .as_deref()
            .map(|a| clean(a, MAX_CUSTOM_ALIAS_CHARS))
            .filter(|a| !a.is_empty());
        d.alias.is_some() && !foreign.contains(id)
    });
    let tidy = |custom: &mut BTreeMap<CustomFieldId, CustomValue>| {
        custom.retain(|id, v| {
            if let CustomValue::Text(t) = v {
                *t = clean(t, MAX_CUSTOM_TEXT_CHARS);
            }
            !foreign.contains(id) && !v.is_empty()
        });
    };
    if let Some(t) = p.summary_task.as_mut() {
        tidy(&mut t.custom);
    }
    let uids: Vec<_> = p.tasks.iter().map(plannercraft_core::Task::uid).collect();
    for uid in &uids {
        if let Some(t) = p.tasks.get_mut(*uid) {
            tidy(&mut t.custom);
        }
    }
    let written = written_fields(p, &foreign);
    lift_container(&mut p.extras, &mut p.custom_fields, &written);
    if let Some(t) = p.summary_task.as_mut() {
        lift_task(&mut t.extras.0, &mut t.custom, &foreign, |_| {});
    }
    for uid in uids {
        if let Some(t) = p.tasks.get_mut(uid) {
            lift_task(&mut t.extras.0, &mut t.custom, &foreign, |_| {});
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use plannercraft_core::{CustomFieldId, CustomKind};

    use super::{Def, classify_def, field_id, format_number, from_field_id, parse_number};

    #[test]
    fn field_ids_unique_and_inverse() {
        let mut seen = std::collections::BTreeSet::new();
        for id in CustomFieldId::all() {
            assert!(seen.insert(field_id(id)), "{id}");
            assert_eq!(from_field_id(field_id(id)), Some(id));
        }
        assert_eq!(from_field_id(188_743_786), None);
        assert_eq!(
            field_id(CustomFieldId::new(CustomKind::Text, 30).unwrap()),
            188_744_016
        );
    }

    #[test]
    fn numbers_fixed_point() {
        for (text, v) in [
            ("12.5", Some(125_000)),
            ("-0.25", Some(-2_500)),
            ("3", Some(30_000)),
            ("0.00005", Some(1)),
            ("0.000049", Some(0)),
            ("-0.00005", Some(-1)),
            (".5", Some(5_000)),
            ("1e3", None),
            ("1,5", None),
            ("", None),
            ("-", None),
            ("100000000000001", None),
            ("100000000000000", Some(1_000_000_000_000_000_000)),
        ] {
            assert_eq!(parse_number(text), v, "{text}");
        }
        assert_eq!(format_number(125_000), "12.5");
        assert_eq!(format_number(-2_500), "-0.25");
        assert_eq!(format_number(30_000), "3");
        assert_eq!(format_number(1), "0.0001");
        assert_eq!(format_number(i64::MIN), "-922337203685477.5808");
    }

    #[test]
    fn definitions_classified() {
        let text1 = CustomFieldId::TEXT1;
        assert_eq!(
            classify_def(
                "<ExtendedAttribute><FieldID>188743731</FieldID><FieldName>Text1</FieldName>\
                 <Alias>L &amp; M</Alias><Guid>x</Guid></ExtendedAttribute>"
            ),
            Def::Simple {
                id: text1,
                alias: Some("L & M".to_owned())
            }
        );
        assert_eq!(
            classify_def(
                "<ExtendedAttribute><FieldID>188743731</FieldID><Formula>[Name]</Formula></ExtendedAttribute>"
            ),
            Def::Foreign(text1)
        );
        assert_eq!(
            classify_def("<ExtendedAttribute><FieldID>188743786</FieldID></ExtendedAttribute>"),
            Def::Other
        );
        let long = "x".repeat(51);
        assert_eq!(
            classify_def(&format!(
                "<ExtendedAttribute><FieldID>188743731</FieldID><Alias>{long}</Alias></ExtendedAttribute>"
            )),
            Def::Foreign(text1)
        );
    }
}
