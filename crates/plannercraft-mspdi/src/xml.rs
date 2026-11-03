//! Lecture événementielle du XML (quick-xml) et arbre borné par sous-arbre
//! (ADR-0009, A19 ; capture des éléments non modélisés : R-087, ADR-0010).
//!
//! Le document n'est jamais chargé en arbre complet : seuls les sous-arbres
//! utiles sont construits (`Calendars/Calendar`, `Tasks/Task`,
//! `Resources/Resource`, `Assignments/Assignment`, les champs scalaires de
//! `Project`).
//! Sous un élément retenu, la table du schéma ([`crate::schema`]) décide du
//! sort de chaque enfant, selon sa classe :
//! - lu (M, M?, D, C, F, et `GUID`) : construit ;
//! - calculé non recalculé (X) : sauté, compté par nom
//!   ([`RawDocument::dropped_computed`]) ;
//! - conservé (P, ou hors schéma) : **capturé** — ses descendants sont
//!   analysés (limites de profondeur, d'éléments, de texte) mais pas
//!   construits, et l'élément garde la tranche exacte de l'entrée qui le
//!   contient ([`Element::raw`]), qui deviendra un `Extras` (R-087). Sous un
//!   jour de calendrier, une exception, une semaine datée, une plage ou un
//!   lien, qui n'ont pas d'`Extras`, un enfant non lu est perdu, compté
//!   ([`RawDocument::not_preserved`]) et signalé (`mspdi.not_preserved`).
//!
//! Un arbre retenu reste peu profond quelle que soit la profondeur de
//! l'entrée (aucune récursion, même à la libération). La construction
//! utilise une pile explicite. Toutes les limites sont vérifiées au fil de la
//! lecture, avant l'allocation qu'elles protègent : nombre d'éléments
//! retenus ([`Limits::max_retained`], une capture compte pour un) et octets
//! capturés ([`Limits::max_extras_bytes`]).

use std::borrow::Cow;
use std::collections::BTreeMap;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::Limits;
use crate::diag::{ImportCode, Sink};
use crate::error::{LineIndex, MspdiError};
use crate::schema::{Class, Parent, sequence};
use crate::style_pi::{self, StyleIgnoredReason};

/// Élément XML retenu : nom local, texte concaténé, enfants retenus,
/// position (octet du début de la balise ouvrante).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Element<'a> {
    /// Nom local (préfixe d'espace de noms retiré), emprunté à l'entrée.
    pub(crate) name: Cow<'a, str>,
    /// Texte, CDATA et références résolues, concaténés, sans rognage ;
    /// emprunté à l'entrée tant qu'il tient en un morceau.
    pub(crate) text: Cow<'a, str>,
    /// Enfants retenus, dans l'ordre du document.
    pub(crate) children: Vec<Element<'a>>,
    /// L'élément a des enfants, retenus ou non : son texte (indentation)
    /// n'est pas lu.
    pub(crate) structured: bool,
    /// Octet du début de la balise ouvrante.
    pub(crate) at: u64,
    /// Élément capturé (classe P ou hors schéma) : tranche exacte de
    /// l'entrée, de `<` à la fin de la balise fermante (R-087).
    pub(crate) raw: Option<&'a str>,
}

/// Sections d'interface : leur présence signe un enregistrement par
/// MS Project (`SourceInfo::ui_sections`).
pub(crate) const UI_SECTIONS: &[&str] = &[
    "Views",
    "Tables",
    "Filters",
    "Groups",
    "Maps",
    "Reports",
    "Drawings",
    "DataLinks",
    "VBAProjects",
];

/// Forme d'un élément retenu : décide du sort de ses enfants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Champ scalaire : aucun enfant retenu.
    Scalar,
    /// Entité dont les enfants non lus sont capturés (`Task`, `Calendar`,
    /// `Resource`, `Assignment`, lignes de base).
    Entity(Parent),
    /// Élément sans `Extras` (`WeekDay`, `Exception`, `WorkWeek`,
    /// `PredecessorLink`) : un enfant non lu est perdu.
    Strict(Parent),
    /// `WeekDays`.
    WeekDays,
    /// `WorkingTimes`.
    WorkingTimes,
    /// `WorkingTime`.
    WorkingTime,
    /// `TimePeriod`.
    TimePeriod,
    /// `Exceptions`.
    Exceptions,
    /// `WorkWeeks`.
    WorkWeeks,
}

/// Sort d'un enfant d'un élément retenu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fate {
    /// Construit, avec cette forme.
    Build(Kind),
    /// Capturé verbatim (R-087).
    Capture,
    /// Calculé non recalculé (classe X) : sauté, compté par nom.
    Drop,
    /// Perdu (aucun `Extras` pour ce parent) : sauté, compté, signalé.
    Lost,
    /// Sauté (contenu d'un champ scalaire, conteneur inconnu…).
    Skip,
}

/// Parents qui portent un `GUID` (élément hors schéma, modélisé).
const fn has_guid(parent: Parent) -> bool {
    matches!(
        parent,
        Parent::Project | Parent::Calendar | Parent::Task | Parent::Resource | Parent::Assignment
    )
}

/// Forme d'un enfant lu (classe M, M?, D, C, F) d'une entité.
fn structured(parent: Parent, name: &str) -> Kind {
    match (parent, name) {
        (Parent::Task, "PredecessorLink") => Kind::Strict(Parent::PredecessorLink),
        (Parent::Task, "Baseline") => Kind::Entity(Parent::TaskBaseline),
        (Parent::Resource, "Baseline") => Kind::Entity(Parent::ResourceBaseline),
        (Parent::Assignment, "Baseline") => Kind::Entity(Parent::AssignmentBaseline),
        (Parent::Calendar, "WeekDays") => Kind::WeekDays,
        (Parent::Calendar, "Exceptions") => Kind::Exceptions,
        (Parent::Calendar, "WorkWeeks") => Kind::WorkWeeks,
        (Parent::WeekDay | Parent::Exception | Parent::WorkWeek, "TimePeriod") => Kind::TimePeriod,
        (Parent::WeekDay | Parent::Exception, "WorkingTimes") => Kind::WorkingTimes,
        // Forme pj15 d'une semaine datée : `WeekDay` direct (`DayType`,
        // `DayWorking`).
        (Parent::WorkWeek, "WeekDay") => Kind::Strict(Parent::WeekDay),
        _ => Kind::Scalar,
    }
}

/// Sort de l'enfant `name` d'un élément de forme `parent`.
fn fate(parent: Kind, name: &str, capture: bool) -> Fate {
    let lost_unless = |known: bool, kind: Kind| if known { Fate::Build(kind) } else { Fate::Lost };
    match parent {
        Kind::Entity(p) => {
            if name == "GUID" && has_guid(p) {
                return Fate::Build(Kind::Scalar);
            }
            match sequence(p).class(name) {
                Some(
                    Class::Modeled | Class::ModeledOpt | Class::Derived | Class::Computed | Class::Format,
                ) => Fate::Build(structured(p, name)),
                Some(Class::Dropped) => Fate::Drop,
                Some(Class::Preserved) | None if capture => Fate::Capture,
                Some(Class::Preserved) | None => Fate::Skip,
            }
        }
        Kind::Strict(p) => {
            // Forme lue depuis J2 d'une semaine datée : `WeekDays/WeekDay`
            // avec plages (R-082), hors du schéma pj15.
            if p == Parent::WorkWeek && name == "WeekDays" {
                return Fate::Build(Kind::WeekDays);
            }
            match sequence(p).class(name) {
                Some(
                    Class::Modeled | Class::ModeledOpt | Class::Derived | Class::Computed | Class::Format,
                ) => Fate::Build(structured(p, name)),
                _ => Fate::Lost,
            }
        }
        Kind::WeekDays => lost_unless(name == "WeekDay", Kind::Strict(Parent::WeekDay)),
        Kind::Exceptions => lost_unless(name == "Exception", Kind::Strict(Parent::Exception)),
        Kind::WorkWeeks => lost_unless(name == "WorkWeek", Kind::Strict(Parent::WorkWeek)),
        Kind::WorkingTimes => lost_unless(name == "WorkingTime", Kind::WorkingTime),
        Kind::WorkingTime => lost_unless(matches!(name, "FromTime" | "ToTime"), Kind::Scalar),
        Kind::TimePeriod => lost_unless(matches!(name, "FromDate" | "ToDate"), Kind::Scalar),
        Kind::Scalar => Fate::Skip,
    }
}

/// Résultat de la lecture XML, avant la correspondance typée.
#[derive(Debug, Default)]
pub(crate) struct RawDocument<'a> {
    /// Espace de noms de la racine, s'il est déclaré.
    pub(crate) namespace: Option<String>,
    /// Déclarations `xmlns:préfixe` de la racine (préfixe, adresse), dans
    /// l'ordre : réémises à l'écriture (R-087).
    pub(crate) prefixes: Vec<(String, String)>,
    /// Octet du début de la racine `Project`.
    pub(crate) root_at: u64,
    /// Enfants de `Project` hors conteneurs : champs lus et éléments
    /// capturés, dans l'ordre du document.
    pub(crate) fields: Vec<Element<'a>>,
    /// Éléments `Calendars/Calendar`.
    pub(crate) calendars: Vec<Element<'a>>,
    /// Éléments `Tasks/Task`.
    pub(crate) tasks: Vec<Element<'a>>,
    /// Éléments `Resources/Resource`.
    pub(crate) resources: Vec<Element<'a>>,
    /// Éléments `Assignments/Assignment`.
    pub(crate) assignments: Vec<Element<'a>>,
    /// Sections d'interface présentes, dans l'ordre du document, sans doublon.
    pub(crate) ui_sections: Vec<String>,
    /// Éléments sautés (hors calculés comptés et perdus).
    pub(crate) skipped_elements: u64,
    /// Éléments calculés non recalculés (classe X), par nom.
    pub(crate) dropped_computed: BTreeMap<String, u64>,
    /// Éléments perdus (enfants non lus d'un parent sans `Extras`).
    pub(crate) not_preserved: u64,
    /// Charge de la première instruction `<?plannercraft-style?>` lue avant la
    /// racine, bornée (H-R12, [`crate::style_pi`]).
    pub(crate) style: Option<String>,
}

/// Destination d'un sous-arbre retenu, une fois fermé.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dest {
    Field,
    Calendar,
    Task,
    Resource,
    Assignment,
}

/// Conteneur de premier niveau.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Container {
    Calendars,
    Tasks,
    Resources,
    Assignments,
}

/// Rôle d'un élément ouvert.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    /// `Project`.
    Root,
    /// Conteneur parcouru (`Calendars`, `Tasks`, `Resources`, `Assignments`).
    Container(Container),
    /// Élément retenu ; `top` pour la racine du sous-arbre ; `kind` décide
    /// des enfants retenus (les autres sont sautés).
    Build { top: Option<Dest>, kind: Kind },
    /// Élément capturé ; `top` pour un enfant direct de `Project`.
    Capture { top: Option<Dest> },
    /// Descendant d'un élément capturé : analysé, non construit.
    InCapture,
    /// Élément sauté (lui et tout son contenu).
    Skip,
}

/// Élément ouvert.
struct Frame<'a> {
    name: Cow<'a, str>,
    role: Role,
}

/// État de la lecture.
struct State<'a, 's, 'l> {
    /// Entrée.
    input: &'a str,
    limits: Limits,
    /// Capturer les éléments non modélisés (`ReadOptions::keep_extras`).
    capture: bool,
    open: Vec<Frame<'a>>,
    /// Pile des éléments retenus en construction (parallèle aux cadres
    /// `Build` et `Capture`).
    building: Vec<Element<'a>>,
    doc: RawDocument<'a>,
    elements: u64,
    /// Éléments retenus (construits ou capturés), bornés par
    /// [`Limits::max_retained`].
    retained: u64,
    /// Octets capturés, bornés par [`Limits::max_extras_bytes`].
    captured_bytes: u64,
    root_seen: bool,
    sink: &'s mut Sink<'l>,
}

/// Lit le document : contrôles structurels (erreurs) et anomalies tolérées
/// (diagnostics dans `sink`). `capture` : garder les éléments non
/// modélisés (R-087).
pub(crate) fn read_document<'a>(
    text: &'a str,
    limits: Limits,
    capture: bool,
    lines: &LineIndex<'_>,
    sink: &mut Sink<'_>,
) -> Result<RawDocument<'a>, MspdiError> {
    let mut reader = Reader::from_str(text);
    {
        let config = reader.config_mut();
        config.expand_empty_elements = true;
        config.check_end_names = true;
        config.trim_text(false);
    }
    let mut state = State {
        input: text,
        limits,
        capture,
        open: Vec::new(),
        building: Vec::new(),
        doc: RawDocument::default(),
        elements: 0,
        retained: 0,
        captured_bytes: 0,
        root_seen: false,
        sink,
    };
    loop {
        let at = reader.buffer_position();
        let event = match reader.read_event() {
            Ok(e) => e,
            Err(e) => {
                return Err(MspdiError::Xml {
                    at: lines.locate(reader.error_position()),
                    message: e.to_string(),
                });
            }
        };
        let after = reader.buffer_position();
        match event {
            Event::Start(e) => state.start(&e, at, lines)?,
            Event::End(_) => state.end(after, lines)?,
            Event::Empty(e) => {
                // `expand_empty_elements` les déplie : branche gardée par sûreté.
                state.start(&e, at, lines)?;
                state.end(after, lines)?;
            }
            Event::Text(t) => state.text(t.xml10_content(), at, lines)?,
            Event::CData(t) => state.text(t.xml10_content(), at, lines)?,
            Event::GeneralRef(r) => {
                let resolved = if r.is_char_ref() {
                    r.resolve_char_ref().ok().flatten().map(String::from)
                } else {
                    quick_xml::escape::resolve_xml_entity(&r).map(str::to_owned)
                };
                let piece = resolved.unwrap_or_else(|| {
                    let raw = format!("&{};", &*r);
                    state
                        .sink
                        .push(ImportCode::Inconsistent, None, at, &state.path(), Some(&raw));
                    raw
                });
                state.text(Cow::Owned(piece), at, lines)?;
            }
            Event::Decl(d) => {
                if let Some(Ok(enc)) = d.encoding() {
                    let enc = enc.trim().to_ascii_lowercase();
                    if enc != "utf-8" && enc != "utf8" {
                        state
                            .sink
                            .push(ImportCode::Inconsistent, None, at, "?xml/encoding", Some(&enc));
                    }
                }
            }
            Event::DocType(_) => {
                return Err(MspdiError::DoctypeNotAllowed { at: lines.locate(at) });
            }
            Event::PI(pi) if pi.target() == style_pi::TARGET || pi.target() == style_pi::LEGACY_TARGET => {
                state.style_pi(pi.content(), at);
            }
            Event::Comment(_) | Event::PI(_) => {}
            Event::Eof => {
                if !state.open.is_empty() || !state.root_seen {
                    return Err(MspdiError::UnexpectedEof {
                        at: lines.locate(at),
                        open: state.open.iter().map(|f| f.name.to_string()).collect(),
                    });
                }
                return Ok(state.doc);
            }
        }
    }
}

/// Nom local d'une balise (préfixe retiré), emprunté à l'entrée : la balise
/// commence à l'octet `at` par `<` suivi du nom qualifié. Repli sur une copie
/// si l'entrée ne correspond pas (jamais attendu).
fn local_name<'a>(input: &'a str, at: u64, e: &BytesStart<'_>) -> Cow<'a, str> {
    let qname = e.name();
    let qname = qname.as_ref();
    let borrowed = usize::try_from(at)
        .ok()
        .and_then(|a| a.checked_add(1))
        .and_then(|start| input.get(start..start.checked_add(qname.len())?))
        .filter(|s| *s == qname)
        .and_then(|s| s.rsplit(':').next());
    borrowed.map_or_else(|| Cow::Owned(e.local_name().as_ref().to_owned()), Cow::Borrowed)
}

/// Espace de noms déclaré par la racine pour son propre préfixe.
fn root_namespace(e: &BytesStart<'_>) -> Option<String> {
    let qname = e.name();
    let wanted = match qname.prefix() {
        Some(p) => format!("xmlns:{}", p.as_ref()),
        None => "xmlns".to_owned(),
    };
    e.attributes()
        .flatten()
        .find_map(|a| (a.key.as_ref() == wanted).then(|| a.value.clone().into_owned()))
}

/// Déclarations `xmlns:préfixe` de la racine.
fn root_prefixes(e: &BytesStart<'_>) -> Vec<(String, String)> {
    e.attributes()
        .flatten()
        .filter_map(|a| {
            let key = a.key.as_ref();
            let prefix = key.strip_prefix("xmlns:")?;
            Some((prefix.to_owned(), a.value.clone().into_owned()))
        })
        .collect()
}

impl<'a> State<'a, '_, '_> {
    /// Instruction `<?plannercraft-style?>` (H-R12) : la première lue avant la
    /// racine et d'au plus 64 Kio est gardée ; les autres sont ignorées,
    /// diagnostic `style.ignored`.
    fn style_pi(&mut self, content: &str, at: u64) {
        let payload = style_pi::payload_of(content);
        let reason = if self.root_seen {
            Some(StyleIgnoredReason::AfterRoot)
        } else if self.doc.style.is_some() {
            Some(StyleIgnoredReason::Duplicate)
        } else if payload.len() > style_pi::MAX_BYTES {
            Some(StyleIgnoredReason::TooLarge)
        } else {
            None
        };
        match reason {
            None => self.doc.style = Some(payload.to_owned()),
            Some(reason) => {
                let shown =
                    (reason == StyleIgnoredReason::TooLarge).then(|| format!("{} octets", payload.len()));
                self.sink.push(
                    ImportCode::StyleIgnored { reason },
                    None,
                    at,
                    "?plannercraft-style",
                    shown.as_deref(),
                );
            }
        }
    }

    /// Chemin des éléments ouverts sous la racine (« Tasks/Task/UID »).
    fn path(&self) -> String {
        let names: Vec<&str> = self.open.iter().skip(1).map(|f| f.name.as_ref()).collect();
        names.join("/")
    }

    /// Rôle d'un enfant de `Project`.
    fn root_child(&mut self, name: &str, at: u64) -> Role {
        let container = match name {
            "Calendars" => Some(Container::Calendars),
            "Tasks" => Some(Container::Tasks),
            "Resources" => Some(Container::Resources),
            "Assignments" => Some(Container::Assignments),
            _ => None,
        };
        if let Some(c) = container {
            // Repère dans la suite des enfants de `Project` : un élément
            // hors schéma qui suit `Tasks` s'y ancre (R-087).
            self.doc.fields.push(Element {
                name: Cow::Owned(name.to_owned()),
                at,
                ..Element::default()
            });
            return Role::Container(c);
        }
        if UI_SECTIONS.contains(&name) && !self.doc.ui_sections.iter().any(|s| s == name) {
            self.doc.ui_sections.push(name.to_owned());
        }
        let field = Role::Build {
            top: Some(Dest::Field),
            kind: Kind::Scalar,
        };
        if name == "GUID" {
            return field;
        }
        match sequence(Parent::Project).class(name) {
            Some(Class::Modeled | Class::ModeledOpt | Class::Derived | Class::Computed | Class::Format) => {
                field
            }
            Some(Class::Dropped) => {
                self.drop_computed(name);
                Role::Skip
            }
            Some(Class::Preserved) | None if self.capture => Role::Capture {
                top: Some(Dest::Field),
            },
            Some(Class::Preserved) | None => Role::Skip,
        }
    }

    /// Rôle d'un enfant d'un élément retenu de forme `kind`.
    fn child_role(&mut self, kind: Kind, name: &str, at: u64) -> Role {
        // Le texte d'un élément qui a des enfants n'est jamais lu
        // (indentation) : il n'est pas gardé, même si l'enfant est sauté.
        if let Some(parent) = self.building.last_mut() {
            parent.structured = true;
            parent.text = Cow::Borrowed("");
        }
        match fate(kind, name, self.capture) {
            Fate::Build(kind) => Role::Build { top: None, kind },
            Fate::Capture => Role::Capture { top: None },
            Fate::Drop => {
                self.drop_computed(name);
                Role::Skip
            }
            Fate::Lost => {
                self.doc.not_preserved = self.doc.not_preserved.saturating_add(1);
                let path = format!("{}/{name}", self.path());
                self.sink.push(ImportCode::NotPreserved, None, at, &path, None);
                Role::Skip
            }
            Fate::Skip => Role::Skip,
        }
    }

    fn drop_computed(&mut self, name: &str) {
        if let Some(n) = self.doc.dropped_computed.get_mut(name) {
            *n = n.saturating_add(1);
        } else {
            self.doc.dropped_computed.insert(name.to_owned(), 1);
        }
    }

    fn start(&mut self, e: &BytesStart<'_>, at: u64, lines: &LineIndex<'_>) -> Result<(), MspdiError> {
        self.elements = self.elements.saturating_add(1);
        if self.elements > self.limits.max_elements {
            return Err(MspdiError::TooManyElements {
                max: self.limits.max_elements,
            });
        }
        let max_depth = self.limits.max_depth.min(crate::MAX_DEPTH);
        if self.open.len() >= usize::from(max_depth) {
            return Err(MspdiError::TooDeep {
                at: lines.locate(at),
                max: max_depth,
            });
        }
        let name = local_name(self.input, at, e);
        let role = match self.open.last().map(|f| f.role) {
            None => {
                if self.root_seen {
                    return Err(MspdiError::Xml {
                        at: lines.locate(at),
                        message: "plusieurs éléments racine".to_owned(),
                    });
                }
                if name != "Project" {
                    return Err(MspdiError::NotMspdi {
                        root: name.into_owned(),
                    });
                }
                self.root_seen = true;
                self.doc.namespace = root_namespace(e);
                self.doc.prefixes = root_prefixes(e);
                self.doc.root_at = at;
                Role::Root
            }
            Some(Role::Root) => self.root_child(&name, at),
            Some(Role::Container(c)) => match (c, name.as_ref()) {
                (Container::Calendars, "Calendar") => Role::Build {
                    top: Some(Dest::Calendar),
                    kind: Kind::Entity(Parent::Calendar),
                },
                (Container::Tasks, "Task") => Role::Build {
                    top: Some(Dest::Task),
                    kind: Kind::Entity(Parent::Task),
                },
                (Container::Resources, "Resource") => Role::Build {
                    top: Some(Dest::Resource),
                    kind: Kind::Entity(Parent::Resource),
                },
                (Container::Assignments, "Assignment") => Role::Build {
                    top: Some(Dest::Assignment),
                    kind: Kind::Entity(Parent::Assignment),
                },
                _ => Role::Skip,
            },
            Some(Role::Build { kind, .. }) => self.child_role(kind, &name, at),
            Some(Role::Capture { .. } | Role::InCapture) => {
                if let Some(parent) = self.building.last_mut() {
                    parent.structured = true;
                    parent.text = Cow::Borrowed("");
                }
                Role::InCapture
            }
            Some(Role::Skip) => Role::Skip,
        };
        match role {
            Role::Skip => self.doc.skipped_elements = self.doc.skipped_elements.saturating_add(1),
            Role::Build { .. } | Role::Capture { .. } => {
                self.retained = self.retained.saturating_add(1);
                if self.retained > self.limits.max_retained {
                    return Err(MspdiError::TooManyRetained {
                        at: lines.locate(at),
                        max: self.limits.max_retained,
                    });
                }
                self.building.push(Element {
                    name: name.clone(),
                    text: Cow::Borrowed(""),
                    children: Vec::new(),
                    structured: false,
                    at,
                    raw: None,
                });
            }
            Role::Root | Role::Container(_) | Role::InCapture => {}
        }
        self.open.push(Frame { name, role });
        Ok(())
    }

    /// Fin de l'élément ouvert ; `after` : octet qui suit sa balise
    /// fermante.
    fn end(&mut self, after: u64, lines: &LineIndex<'_>) -> Result<(), MspdiError> {
        let Some(frame) = self.open.pop() else {
            return Ok(());
        };
        let (Role::Build { top, .. } | Role::Capture { top }) = frame.role else {
            return Ok(());
        };
        let Some(mut done) = self.building.pop() else {
            return Ok(());
        };
        if matches!(frame.role, Role::Capture { .. }) {
            let size = after.saturating_sub(done.at);
            self.captured_bytes = self.captured_bytes.saturating_add(size);
            if self.captured_bytes > self.limits.max_extras_bytes {
                return Err(MspdiError::TooManyRetained {
                    at: lines.locate(done.at),
                    max: self.limits.max_extras_bytes,
                });
            }
            let range = usize::try_from(done.at).ok().zip(usize::try_from(after).ok());
            done.raw = range.and_then(|(a, b)| self.input.get(a..b));
        }
        match top {
            None => {
                if let Some(parent) = self.building.last_mut() {
                    parent.children.push(done);
                }
            }
            Some(Dest::Field) => self.doc.fields.push(done),
            Some(Dest::Calendar) => self.doc.calendars.push(done),
            Some(Dest::Task) => self.doc.tasks.push(done),
            Some(Dest::Resource) => self.doc.resources.push(done),
            Some(Dest::Assignment) => self.doc.assignments.push(done),
        }
        Ok(())
    }

    fn text(&mut self, piece: Cow<'a, str>, at: u64, lines: &LineIndex<'_>) -> Result<(), MspdiError> {
        let max = self.limits.max_text_bytes;
        let too_long = || MspdiError::TextTooLong {
            at: lines.locate(at),
            max,
        };
        if piece.len() > max {
            return Err(too_long());
        }
        if !matches!(
            self.open.last().map(|f| f.role),
            Some(Role::Build { .. } | Role::Capture { .. })
        ) {
            return Ok(());
        }
        if let Some(current) = self.building.last_mut() {
            if current.structured {
                return Ok(());
            }
            if current.text.len().saturating_add(piece.len()) > max {
                return Err(too_long());
            }
            if current.text.is_empty() {
                current.text = piece;
            } else {
                current.text.to_mut().push_str(&piece);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::string_slice,
        clippy::arithmetic_side_effects,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use proptest::prelude::*;

    use super::{RawDocument, read_document};
    use crate::Limits;
    use crate::diag::{ImportCode, ImportDiagnostic, Sink};
    use crate::error::{LineIndex, MspdiError};

    fn read_with(text: &str, limits: Limits) -> (Result<RawDocument<'_>, MspdiError>, Vec<ImportDiagnostic>) {
        let lines = LineIndex::new(text);
        let mut sink = Sink::new(&lines);
        let r = read_document(text, limits, true, &lines, &mut sink);
        (r, sink.out)
    }

    fn read(text: &str) -> (Result<RawDocument<'_>, MspdiError>, Vec<ImportDiagnostic>) {
        read_with(text, Limits::default())
    }

    const SMALL: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Project xmlns="http://schemas.microsoft.com/project">
  <SaveVersion>14</SaveVersion>
  <Name>A &amp; B &#233;&#x41;</Name>
  <Title><![CDATA[x < y]]></Title>
  <Views><View><Name>Gantt</Name></View></Views>
  <Maps/>
  <Resources><Resource><UID>1</UID></Resource></Resources>
  <Calendars>
    <Calendar><UID>1</UID><WeekDays><WeekDay><DayType>1</DayType></WeekDay></WeekDays></Calendar>
  </Calendars>
  <Tasks>
    <Task><UID>0</UID><Name/></Task>
    <Task><UID>1</UID><PredecessorLink><PredecessorUID>0</PredecessorUID></PredecessorLink></Task>
  </Tasks>
  <Assignments>
    <Assignment><UID>5</UID><TaskUID>1</TaskUID><TimephasedData><Type>1</Type></TimephasedData></Assignment>
  </Assignments>
</Project>
"#;

    #[test]
    fn subtrees_and_fields() {
        let (doc, diags) = read(SMALL);
        let doc = doc.unwrap();
        assert!(diags.is_empty(), "{diags:?}");
        assert_eq!(
            doc.namespace.as_deref(),
            Some("http://schemas.microsoft.com/project")
        );
        // Champs lus, éléments capturés (sections d'interface, hors schéma)
        // et repères des conteneurs, dans l'ordre du document.
        let names: Vec<&str> = doc.fields.iter().map(|f| f.name.as_ref()).collect();
        assert_eq!(
            names,
            [
                "SaveVersion",
                "Name",
                "Title",
                "Views",
                "Maps",
                "Resources",
                "Calendars",
                "Tasks",
                "Assignments"
            ]
        );
        assert_eq!(doc.fields[1].text, "A & B éA");
        assert_eq!(doc.fields[2].text, "x < y");
        assert_eq!(
            doc.fields[3].raw,
            Some("<Views><View><Name>Gantt</Name></View></Views>")
        );
        assert_eq!(doc.fields[4].raw, Some("<Maps/>"));
        assert_eq!(doc.fields[1].raw, None);
        assert_eq!(doc.ui_sections, ["Views", "Maps"]);
        // Rien n'est sauté : la répartition de l'affectation est capturée
        // (le filtre des répartitions calculées vient après, R-087).
        assert_eq!(doc.skipped_elements, 0);
        assert_eq!(doc.resources.len(), 1);
        assert_eq!(doc.calendars.len(), 1);
        let cal = &doc.calendars[0];
        assert_eq!(cal.children[1].children[0].children[0].text, "1");
        assert_eq!(doc.tasks.len(), 2);
        assert_eq!(doc.tasks[0].children[1].name, "Name");
        assert_eq!(doc.tasks[1].children[1].children[0].text, "0");
        assert_eq!(doc.assignments.len(), 1);
        let names: Vec<&str> = doc.assignments[0]
            .children
            .iter()
            .map(|c| c.name.as_ref())
            .collect();
        assert_eq!(names, ["UID", "TaskUID", "TimephasedData"]);
        assert_eq!(doc.assignments[0].children[1].text, "1");
        assert!(doc.assignments[0].children[2].raw.is_some());
        // Position : début de la balise.
        assert_eq!(&SMALL[usize::try_from(doc.tasks[1].at).unwrap()..][..6], "<Task>");
    }

    #[test]
    fn prefixed_root() {
        let (doc, _) = read(r#"<p:Project xmlns:p="urn:x"><p:Name>n</p:Name></p:Project>"#);
        let doc = doc.unwrap();
        assert_eq!(doc.namespace.as_deref(), Some("urn:x"));
        assert_eq!(doc.fields[0].text, "n");
    }

    #[test]
    fn tolerated_anomalies() {
        let (doc, diags) =
            read(r#"<?xml version="1.0" encoding="ISO-8859-1"?><Project><Name>&nbsp;x</Name></Project>"#);
        assert_eq!(doc.unwrap().fields[0].text, "&nbsp;x");
        let codes: Vec<&ImportCode> = diags.iter().map(|d| &d.code).collect();
        assert_eq!(codes, [&ImportCode::Inconsistent, &ImportCode::Inconsistent]);
        assert_eq!(diags[1].element, "Name");
        assert_eq!(diags[1].value.as_deref(), Some("&nbsp;"));
    }

    #[test]
    fn structural_errors() {
        assert!(matches!(read("<Projet/>").0, Err(MspdiError::NotMspdi { root }) if root == "Projet"));
        assert!(matches!(
            read("<!DOCTYPE p [<!ENTITY a \"b\">]><Project/>").0,
            Err(MspdiError::DoctypeNotAllowed { .. })
        ));
        match read("<Project><Tasks><Task>").0 {
            Err(MspdiError::UnexpectedEof { open, .. }) => assert_eq!(open, ["Project", "Tasks", "Task"]),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            read(r#"<?xml version="1.0"?>"#).0,
            Err(MspdiError::UnexpectedEof { open, .. }) if open.is_empty()
        ));
        assert!(matches!(read("<Project></Tasks>").0, Err(MspdiError::Xml { .. })));
        assert!(matches!(
            read("<Project/><Project/>").0,
            Err(MspdiError::Xml { .. })
        ));
        assert!(matches!(
            read("<Project><Name>a &b</Name></Project>").0,
            Err(MspdiError::Xml { .. })
        ));
        match read("<Project>\n  <Tasks></Task></Tasks></Project>").0 {
            Err(MspdiError::Xml { at, .. }) => assert_eq!(at.line, 2),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn limits() {
        let deep = format!("{}{}", "<Project>", "<a>".repeat(100_000));
        assert!(matches!(read(&deep).0, Err(MspdiError::TooDeep { max: 64, .. })));
        let many = format!("<Project>{}</Project>", "<a/>".repeat(20));
        let limits = Limits {
            max_elements: 10,
            ..Limits::default()
        };
        assert!(matches!(
            read_with(&many, limits).0,
            Err(MspdiError::TooManyElements { max: 10 })
        ));
        let long = format!("<Project><Name>{}</Name></Project>", "x".repeat(20 * 1024 * 1024));
        assert!(matches!(read(&long).0, Err(MspdiError::TextTooLong { .. })));
        // Texte retenu fait de morceaux : la somme est bornée aussi.
        let limits = Limits {
            max_text_bytes: 8,
            ..Limits::default()
        };
        let pieces = "<Project><Name>abcd&amp;efgh</Name></Project>";
        assert!(matches!(
            read_with(pieces, limits).0,
            Err(MspdiError::TextTooLong { .. })
        ));
    }

    #[test]
    fn r087_children_by_class() {
        // Sous `Task` et `Calendar`, la classe décide : lus (M, M?, D, C, F)
        // construits ; calculés non recalculés (X) sautés et comptés par
        // nom ; conservés (P) et hors schéma capturés verbatim ; enfants non
        // lus d'un lien ou d'un jour de calendrier perdus, comptés et
        // signalés. Le texte d'un champ qui a des enfants n'est pas lu.
        let text = "<Project><Tasks><Task><UID>1</UID><Baseline><Number>0</Number></Baseline>\
                    <ExtendedAttribute><FieldID>1</FieldID></ExtendedAttribute><X/><Name>a<b/>c</Name>\
                    <Cost>5</Cost><PredecessorLink><PredecessorUID>2</PredecessorUID><Junk/></PredecessorLink>\
                    </Task></Tasks><Calendars><Calendar><UID>1</UID><Foo/><WeekDays><WeekDay><DayType>1</DayType>\
                    <Bar/></WeekDay></WeekDays></Calendar></Calendars></Project>";
        let (doc, diags) = read(text);
        let doc = doc.unwrap();
        let lost: Vec<&str> = diags.iter().map(|d| d.element.as_str()).collect();
        assert_eq!(
            lost,
            [
                "Tasks/Task/PredecessorLink/Junk",
                "Calendars/Calendar/WeekDays/WeekDay/Bar"
            ]
        );
        assert!(diags.iter().all(|d| d.code == ImportCode::NotPreserved));
        assert_eq!(doc.not_preserved, 2);
        let names =
            |e: &super::Element<'_>| e.children.iter().map(|c| c.name.to_string()).collect::<Vec<_>>();
        let task = &doc.tasks[0];
        assert_eq!(
            names(task),
            [
                "UID",
                "Baseline",
                "ExtendedAttribute",
                "X",
                "Name",
                "PredecessorLink"
            ]
        );
        assert_eq!(
            task.children[2].raw,
            Some("<ExtendedAttribute><FieldID>1</FieldID></ExtendedAttribute>")
        );
        assert_eq!(task.children[3].raw, Some("<X/>"));
        assert_eq!(task.children[4].text, "");
        assert_eq!(names(&task.children[5]), ["PredecessorUID"]);
        assert_eq!(doc.dropped_computed.get("Cost"), Some(&1));
        let cal = &doc.calendars[0];
        assert_eq!(names(cal), ["UID", "Foo", "WeekDays"]);
        assert_eq!(names(&cal.children[2].children[0]), ["DayType"]);
        // b (sous Name), Cost, Junk, Bar.
        assert_eq!(doc.skipped_elements, 4);
        // Sans capture : P et hors schéma sautés.
        let lines = LineIndex::new(text);
        let mut sink = Sink::new(&lines);
        let plain = read_document(text, Limits::default(), false, &lines, &mut sink).unwrap();
        assert_eq!(
            names(&plain.tasks[0]),
            ["UID", "Baseline", "Name", "PredecessorLink"]
        );
        assert_eq!(plain.skipped_elements, 8);
    }

    #[test]
    fn r087_capture_limits() {
        // Chaque capture compte pour un élément retenu ; le total des octets
        // capturés est borné.
        let text = "<Project><Tasks><Task><UID>1</UID><Contact>a</Contact><Contact>b</Contact></Task></Tasks></Project>";
        let limits = Limits {
            max_retained: 3,
            ..Limits::default()
        };
        assert!(matches!(
            read_with(text, limits).0,
            Err(MspdiError::TooManyRetained { max: 3, .. })
        ));
        let limits = Limits {
            max_extras_bytes: 39,
            ..Limits::default()
        };
        // Deux captures de 20 octets.
        assert!(matches!(
            read_with(text, limits).0,
            Err(MspdiError::TooManyRetained { max: 39, .. })
        ));
        let limits = Limits {
            max_extras_bytes: 40,
            ..Limits::default()
        };
        assert!(read_with(text, limits).0.is_ok());
    }

    #[test]
    fn retained_budget() {
        // Name, Task, UID, Name : le quatrième élément retenu dépasse 3.
        let limits = Limits {
            max_retained: 3,
            ..Limits::default()
        };
        let text = "<Project><Name>a</Name><Tasks><Task><UID>1</UID><Name>x</Name></Task></Tasks></Project>";
        assert!(matches!(
            read_with(text, limits).0,
            Err(MspdiError::TooManyRetained { max: 3, .. })
        ));
        // Les éléments sautés (ici, calculés non recalculés) ne comptent pas.
        let skipped = format!(
            "<Project><Tasks><Task><UID>1</UID>{}</Task></Tasks></Project>",
            "<Cost>0</Cost>".repeat(1_000)
        );
        let doc = read_with(&skipped, limits).0.unwrap();
        assert_eq!(doc.skipped_elements, 1_000);
        assert_eq!(doc.dropped_computed.get("Cost"), Some(&1_000));
    }

    #[test]
    fn depth_is_capped_whatever_the_limit() {
        let limits = Limits {
            max_depth: u16::MAX,
            ..Limits::default()
        };
        let deep = format!("<Project>{}", "<a>".repeat(1_000));
        assert!(matches!(
            read_with(&deep, limits).0,
            Err(MspdiError::TooDeep {
                max: crate::MAX_DEPTH,
                ..
            })
        ));
    }

    #[test]
    fn every_prefix_is_ok_or_err() {
        for end in 0..=SMALL.len() {
            if let Some(prefix) = SMALL.get(..end) {
                let _ = read(prefix);
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]
        /// Octets modifiés, insérés ou supprimés : jamais de panique.
        #[test]
        fn mutations_never_panic(
            edits in proptest::collection::vec((any::<u16>(), any::<u8>(), 0_u8..3), 1..8)
        ) {
            let mut bytes = SMALL.as_bytes().to_vec();
            for (pos, byte, op) in edits {
                let len = bytes.len().max(1);
                let p = usize::from(pos) % len;
                match op {
                    0 if p < bytes.len() => bytes[p] = byte,
                    1 => bytes.insert(p.min(bytes.len()), byte),
                    _ if p < bytes.len() => { bytes.remove(p); }
                    _ => {}
                }
            }
            if let Ok(text) = std::str::from_utf8(&bytes) {
                let _ = read(text);
            }
        }
    }
}
