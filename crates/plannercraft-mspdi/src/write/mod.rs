//! Écriture MSPDI (R-120 à R-132 ; ADR-0010, D11).
//!
//! [`write`] est infaillible (règle 10) : toute anomalie (valeur arrondie,
//! chaîne tronquée, caractère interdit) devient un [`WriteDiagnostic`].
//! L'ordre des éléments vient de la table du schéma ([`crate::schema`]) :
//! profil [`OrderProfile::Observed`] (défaut, ordre des fichiers
//! enregistrés par MS Project, O19) ou [`OrderProfile::Strict`] (ordre pj15,
//! sans `GUID` ni élément hors schéma). Tout champ modélisé non optionnel
//! est écrit explicitement, même égal au défaut du schéma : la relecture ne
//! dépend jamais des défauts (ADR-0008, R-076). Sortie déterministe octet
//! pour octet (aucune `HashMap`, ordre des arènes).
//!
//! Aucune E/S : la sortie est en octets (UTF-8 sans BOM).

mod assignment;
mod baseline;
mod calendar;
mod computed;
mod emit;
mod leveling;
mod project;
mod resource;
mod task;
mod timephased;

use std::collections::BTreeMap;

use plannercraft_core::{Extras, Percent, Project, Subject, round_div};

use crate::extras::{self, Anchor, ExtraKey, ORIGIN};
pub use crate::schema::Profile as OrderProfile;
use crate::schema::{Class, Parent, sequence};
use crate::style_pi;
pub(crate) use assignment::remaining_work as assignment_remaining_work;
pub(crate) use computed::leveling_delay as leveling_payload_int;
pub use computed::{Computed, choose_computed, schedule_image};
use emit::Emitter;
pub(crate) use emit::is_xml_char;
pub(crate) use resource::hourly_rate_text;
pub(crate) use task::remaining;

/// Texte échappé tel que l'écrivain l'écrit (R-131).
pub(crate) fn escape_text(s: &str) -> String {
    emit::escape(s)
}

/// Texte sans caractère interdit par XML 1.0 (U+FFFD, R-131).
pub(crate) fn sanitize_text(s: &str) -> String {
    emit::sanitize(s).0
}

/// Fin de ligne.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Newline {
    /// `\n` (défaut).
    #[default]
    Lf,
    /// `\r\n`.
    Crlf,
}

/// Indentation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Indent {
    /// Une tabulation par niveau (défaut, comme MS Project).
    #[default]
    Tab,
    /// N espaces par niveau.
    Spaces(u8),
    /// Aucune indentation ni fin de ligne entre les éléments.
    None,
}

/// Options d'écriture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteOptions {
    /// Profil d'ordre (R-121).
    pub profile: OrderProfile,
    /// `SaveVersion` (R-130) : 14, version écrite par MS Project 2010 et
    /// suivants.
    pub save_version: i64,
    /// `CurrencyCode` écrit si le projet n'en a pas relu un (R-130).
    pub currency_code: String,
    /// Fin de ligne.
    pub newline: Newline,
    /// Indentation.
    pub indent: Indent,
    /// Écrire le style du document (`<?plannercraft-style?>`, H-R12) s'il en a un
    /// (vrai par défaut ; faux : préférence « ne jamais écrire la mise en
    /// forme dans le fichier »). Sans style, rien n'est écrit dans les deux
    /// cas.
    pub style: bool,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            profile: OrderProfile::Observed,
            save_version: 14,
            currency_code: "EUR".to_owned(),
            newline: Newline::Lf,
            indent: Indent::Tab,
            style: true,
        }
    }
}

/// Document écrit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Written {
    /// Octets UTF-8, sans BOM.
    pub xml: Vec<u8>,
    /// Diagnostics d'écriture, dans l'ordre d'écriture.
    pub diagnostics: Vec<WriteDiagnostic>,
}

/// Diagnostic d'écriture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteDiagnostic {
    /// Nature.
    pub code: WriteCode,
    /// Élément du projet concerné.
    pub subject: Option<Subject>,
    /// Chemin de l'élément XML (« Tasks/Task/Name »).
    pub element: String,
    /// Valeur en cause (tronquée à 200 caractères).
    pub value: Option<String>,
}

/// Nature d'un diagnostic d'écriture (codes stables `mspdi.write.*`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WriteCode {
    /// Valeur arrondie à la précision du format (pourcentage entier…).
    Rounded,
    /// Chaîne tronquée au `maxLength` du schéma (en caractères).
    Truncated {
        /// Longueur maximale.
        max: u32,
    },
    /// Caractère interdit par XML 1.0 remplacé par U+FFFD.
    InvalidChar,
    /// `Extras` non réécrit : clé illisible ou sans emplacement, charge qui
    /// n'est pas un seul élément bien formé du nom de la clé (R-123).
    ExtraInvalid,
    /// Le `Schedule` ne correspond pas aux tâches du projet (autre projet,
    /// projet modifié) : aucune valeur calculée écrite.
    ScheduleMismatch,
    /// R-199, R-215 : `TimephasedData` d'une affectation (sujet du
    /// diagnostic) jetées : son placement par le moteur n'est plus celui que
    /// décrivent ses lignes de type 1, ou son travail réel n'est plus la
    /// somme de ses lignes de type 2.
    TimephasedDropped {
        /// Nombre d'éléments `TimephasedData` jetés.
        count: u32,
    },
}

impl WriteCode {
    /// Code stable.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Rounded => "mspdi.write.rounded",
            Self::Truncated { .. } => "mspdi.write.truncated",
            Self::InvalidChar => "mspdi.write.invalid_char",
            Self::ExtraInvalid => "mspdi.write.extra_invalid",
            Self::ScheduleMismatch => "mspdi.write.schedule_mismatch",
            Self::TimephasedDropped { .. } => "mspdi.write.timephased_dropped",
        }
    }
}

/// Écrit un projet en MSPDI, avec les valeurs calculées de `computed`
/// (R-122 ; [`choose_computed`] choisit la source). Infaillible : toute
/// anomalie devient un diagnostic.
#[must_use]
pub fn write(project: &Project, computed: Computed<'_>, options: &WriteOptions) -> Written {
    let mut em = Emitter::new(options.newline, options.indent);
    let values = match computed {
        Computed::Schedule(s) if computed::matches(project, s) => Some(computed::from_schedule(project, s)),
        Computed::Schedule(_) => {
            em.report(WriteCode::ScheduleMismatch, "Tasks", None);
            None
        }
        Computed::Recorded(r) => Some(computed::from_recorded(r)),
        Computed::Omit => None,
    };
    let namespaces: Vec<(String, String)> = project
        .extras
        .0
        .iter()
        .filter(|e| e.origin == ORIGIN)
        .filter_map(|e| Some((e.key.strip_prefix("@xmlns:")?.to_owned(), e.payload.clone())))
        .filter(|(prefix, _)| ExtraKey::parse(&format!("@xmlns:{prefix}")).is_some())
        .collect();
    // H-R12 : style du document juste après la déclaration, s'il est
    // écrivable ; sinon rien, et un diagnostic.
    let style = style_pi::entry(&project.extras)
        .filter(|_| options.style)
        .map(|e| e.payload.as_str());
    let style = match style {
        Some(payload) if style_pi::writable(payload) => Some(payload),
        Some(payload) => {
            let shown: String = payload.chars().take(200).collect();
            em.report(WriteCode::ExtraInvalid, "?plannercraft-style", Some(shown));
            None
        }
        None => None,
    };
    em.root(&namespaces, style);
    project::write(&mut em, project, values.as_ref(), options);
    em.end_root();
    let (xml, diagnostics) = em.finish();
    Written { xml, diagnostics }
}

/// Un `Extras` à réécrire (R-123) : clé lue, charge validée.
pub(crate) struct Planned<'e> {
    pub(crate) key: ExtraKey,
    payload: &'e str,
}

/// R-123 : `Extras` d'une entité de `parent` à réécrire. Origine autre que
/// MSPDI : gardés dans le modèle, jamais écrits. Clé illisible, sans
/// emplacement (nom modélisé, ancre hors du schéma…), portée d'une ligne de
/// base absente, ou charge invalide : ignorés, `mspdi.write.extra_invalid`.
/// Profil strict : noms hors schéma non écrits. Déclarations d'espace de
/// noms (`@xmlns:p`) : écrites sur la racine, pas ici.
pub(crate) fn plan<'e>(
    em: &mut Emitter,
    element: &str,
    parent: Parent,
    extras: &'e Extras,
    baselines: &[u8],
    profile: OrderProfile,
) -> Vec<Planned<'e>> {
    let mut out = Vec::new();
    let path = |key: &str| {
        if element.is_empty() {
            key.to_owned()
        } else {
            format!("{element}/{key}")
        }
    };
    for e in extras.0.iter().filter(|e| e.origin == ORIGIN) {
        let Some(key) = ExtraKey::parse(&e.key) else {
            em.report(WriteCode::ExtraInvalid, &path(&e.key), Some(e.key.clone()));
            continue;
        };
        if key.is_attribute() && parent == Parent::Project {
            continue;
        }
        let in_scope = key.scope.is_none_or(|n| baselines.contains(&n));
        let ok = in_scope
            && extras::placement(parent, &e.key).is_some()
            && extras::valid_payload(&key.name, &e.payload);
        if !ok {
            em.report(WriteCode::ExtraInvalid, &path(&e.key), Some(e.key.clone()));
            continue;
        }
        let inner = match key.scope {
            Some(_) => extras::baseline_parent(parent).unwrap_or(parent),
            None => parent,
        };
        if profile == OrderProfile::Strict && sequence(inner).class(&key.name).is_none() {
            continue;
        }
        out.push(Planned {
            key,
            payload: &e.payload,
        });
    }
    out
}

/// Aucun `Extras` (éléments qui n'en ont pas : lien, jour, exception…).
pub(crate) const NO_EXTRAS: &[&Planned<'static>] = &[];

/// `Extras` d'une ligne de base n, parmi ceux d'une entité.
pub(crate) fn scoped<'p, 'e>(planned: &'p [Planned<'e>], n: u8) -> Vec<&'p Planned<'e>> {
    planned.iter().filter(|p| p.key.scope == Some(n)).collect()
}

/// Écrit les enfants d'une entité de `parent` : pour chaque nom du profil,
/// `field` écrit la valeur modélisée, déduite ou calculée, puis les `Extras`
/// de ce nom, puis ceux ancrés sur lui ; ceux ancrés en tête (`^`) d'abord
/// (R-123). `extras` : ceux de la portée écrite (entité ou ligne de base).
pub(crate) fn children(
    em: &mut Emitter,
    parent: Parent,
    profile: OrderProfile,
    extras: &[&Planned<'_>],
    mut field: impl FnMut(&mut Emitter, &'static str),
) {
    let mut named: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut anchored: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for p in extras {
        match &p.key.anchor {
            Anchor::None => named.entry(p.key.name.as_str()).or_default().push(p.payload),
            Anchor::Start => em.raw(p.payload),
            Anchor::After(a) => anchored.entry(a.as_str()).or_default().push(p.payload),
        }
    }
    for (name, class) in sequence(parent).classed(profile) {
        // Calculés non recalculés (X) : rien à écrire. Les conservés (P)
        // passent par `field` (`CurrencyCode` a une valeur par défaut, R-130).
        if *class != Some(Class::Dropped) {
            field(em, name);
        }
        if extras.is_empty() {
            continue;
        }
        for payload in named
            .get(name)
            .into_iter()
            .flatten()
            .chain(anchored.get(name).into_iter().flatten())
        {
            em.raw(payload);
        }
    }
}

/// Écrit une entité de `parent` sous l'élément `element` (voir
/// [`children`]).
fn entity(
    em: &mut Emitter,
    element: &'static str,
    parent: Parent,
    profile: OrderProfile,
    extras: &[&Planned<'_>],
    field: impl FnMut(&mut Emitter, &'static str),
) {
    em.open(element);
    children(em, parent, profile, extras, field);
    em.close();
}

/// `Extras` d'une entité hors portée de ligne de base.
pub(crate) fn unscoped<'p, 'e>(planned: &'p [Planned<'e>]) -> Vec<&'p Planned<'e>> {
    planned.iter().filter(|p| p.key.scope.is_none()).collect()
}

/// Longueur maximale d'un élément de `parent` (table du schéma).
fn max_len(parent: Parent, name: &str) -> Option<u16> {
    sequence(parent).slot(name).and_then(|s| s.max_len)
}

/// R-127 : pourcentage entier `round_div(p, 100)` (R-001), arrondi signalé.
fn percent(em: &mut Emitter, name: &'static str, p: Percent) {
    let v = p.get();
    let whole = round_div(i128::from(v), 100).unwrap_or(0);
    if v.checked_rem(100).is_some_and(|r| r != 0) {
        em.report(WriteCode::Rounded, name, Some(v.to_string()));
    }
    em.leaf(name, &whole.to_string());
}
