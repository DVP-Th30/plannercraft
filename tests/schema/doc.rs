//! Arbre d'un document MSPDI écrit (noms locaux, texte) et forme canonique
//! pj15 d'une sortie du profil observé (P26).

use plannercraft_mspdi::schema::{Parent, Profile, sequence};
use quick_xml::Reader;
use quick_xml::events::Event;

/// Élément : nom local, texte (feuille), enfants.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Elem {
    pub name: String,
    pub text: String,
    pub children: Vec<Elem>,
}

/// Arbre d'un document (racine `Project`).
pub fn parse(xml: &str) -> Elem {
    let mut reader = Reader::from_str(xml);
    let mut stack: Vec<Elem> = vec![Elem::default()];
    loop {
        match reader.read_event().expect("XML bien formé") {
            Event::Start(e) => stack.push(Elem {
                name: e.local_name().as_ref().to_owned(),
                ..Elem::default()
            }),
            Event::Empty(e) => {
                let el = Elem {
                    name: e.local_name().as_ref().to_owned(),
                    ..Elem::default()
                };
                stack.last_mut().unwrap().children.push(el);
            }
            Event::Text(t) => {
                let s = t.xml10_content();
                stack.last_mut().unwrap().text.push_str(&s);
            }
            Event::GeneralRef(r) => {
                let s = r.resolve_char_ref().ok().flatten().map_or_else(
                    || {
                        quick_xml::escape::resolve_xml_entity(&r)
                            .unwrap_or_default()
                            .to_owned()
                    },
                    String::from,
                );
                stack.last_mut().unwrap().text.push_str(&s);
            }
            Event::End(_) => {
                let mut done = stack.pop().unwrap();
                if !done.children.is_empty() {
                    done.text.clear();
                }
                stack.last_mut().unwrap().children.push(done);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    stack.pop().unwrap().children.pop().expect("racine")
}

/// Parent du schéma d'un élément d'entité, selon son nom et celui de son
/// parent.
fn parent_of(parent: Option<Parent>, name: &str) -> Option<Parent> {
    match (parent, name) {
        (None, "Project") => Some(Parent::Project),
        (_, "Calendar") => Some(Parent::Calendar),
        (_, "Task") => Some(Parent::Task),
        (_, "Resource") => Some(Parent::Resource),
        (_, "Assignment") => Some(Parent::Assignment),
        (Some(Parent::Task), "PredecessorLink") => Some(Parent::PredecessorLink),
        (Some(Parent::Task), "Baseline") => Some(Parent::TaskBaseline),
        (Some(Parent::Resource), "Baseline") => Some(Parent::ResourceBaseline),
        (Some(Parent::Assignment), "Baseline") => Some(Parent::AssignmentBaseline),
        (_, "Exception") => Some(Parent::Exception),
        (_, "WorkWeek") => Some(Parent::WorkWeek),
        _ => None,
    }
}

/// Forme pj15 d'une sortie du profil observé : `GUID` et éléments hors
/// schéma retirés, enfants des entités remis dans l'ordre pj15 (tri
/// stable), semaine datée ramenée à la forme pj15 (`WeekDay` direct sans
/// plages).
pub fn canonical(e: &mut Elem, parent: Option<Parent>) {
    let me = parent_of(parent, &e.name);
    let context = me.or(parent);
    if let Some(p) = me {
        let seq = sequence(p);
        if p == Parent::WorkWeek {
            let mut days = Vec::new();
            for c in std::mem::take(&mut e.children) {
                if c.name == "WeekDays" {
                    for mut d in c.children {
                        d.children.retain(|x| x.name != "WorkingTimes");
                        days.push(d);
                    }
                } else {
                    e.children.push(c);
                }
            }
            e.children.extend(days);
        }
        e.children.retain(|c| seq.class(&c.name).is_some());
        e.children
            .sort_by_key(|c| seq.position(Profile::Strict, &c.name).unwrap_or(usize::MAX));
    }
    for c in &mut e.children {
        canonical(c, context);
    }
}

/// Éléments d'entité dont le texte dépasse le `maxLength` (en caractères)
/// de la table du schéma.
pub fn over_max_len(e: &Elem, parent: Option<Parent>) -> Vec<String> {
    let me = parent_of(parent, &e.name);
    let mut out = Vec::new();
    for c in &e.children {
        if let Some(max) = me.and_then(|p| sequence(p).slot(&c.name)).and_then(|s| s.max_len)
            && c.text.chars().count() > usize::from(max)
        {
            out.push(c.name.clone());
        }
        out.extend(over_max_len(c, me.or(parent)));
    }
    out
}
