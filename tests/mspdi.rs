//! Aides des tests MSPDI (écriture, relecture, ordre des éléments).

use plannercraft_core::{BuiltinCalendarNames, Project, ProjectInit, ScheduleAnchor};
use plannercraft_mspdi::write::{Computed, WriteOptions, Written, write};
use plannercraft_mspdi::{MspdiDocument, ReadOptions, read};
use quick_xml::Reader;
use quick_xml::events::Event;

use crate::moment;

/// Projet neuf commencé à `start` (« maintenant » : 17/10/2026 08:00).
#[must_use]
pub fn new_project(start: &str) -> Project {
    Project::new(ProjectInit {
        name: "P".to_owned(),
        anchor: ScheduleAnchor::FromStart(moment(start)),
        current_date: moment("2026-10-17T08:00"),
        calendar_names: BuiltinCalendarNames::default(),
    })
}

/// Écrit avec les options par défaut.
#[must_use]
pub fn written(p: &Project) -> Written {
    write(p, Computed::Omit, &WriteOptions::default())
}

/// Texte écrit (UTF-8).
///
/// # Panics
///
/// Si la sortie n'est pas de l'UTF-8 (jamais attendu).
#[must_use]
pub fn text(w: &Written) -> String {
    match String::from_utf8(w.xml.clone()) {
        Ok(s) => s,
        Err(e) => panic!("sortie non UTF-8 : {e}"),
    }
}

/// Texte sans fins de ligne ni tabulations (comparaison de fragments).
#[must_use]
pub fn compact(xml: &str) -> String {
    xml.replace(['\n', '\t'], "")
}

/// Relit des octets MSPDI.
///
/// # Panics
///
/// Si le document est structurellement illisible.
#[must_use]
pub fn reread(xml: &[u8]) -> MspdiDocument {
    match read(xml, &ReadOptions::new(moment("2026-01-01T00:00"))) {
        Ok(d) => d,
        Err(e) => panic!("relecture impossible : {e}"),
    }
}

/// Noms des enfants de chaque entité, dans l'ordre du document : racine
/// `Project`, puis chaque `Calendar`, `Task`, `Resource`, `Assignment`
/// (chemin, noms locaux).
///
/// # Panics
///
/// Si le document est mal formé.
#[must_use]
pub fn entity_children(xml: &str) -> Vec<(String, Vec<String>)> {
    let mut reader = Reader::from_str(xml);
    let mut stack: Vec<String> = Vec::new();
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    // Indice, dans `out`, de l'entité ouverte à chaque profondeur.
    let mut open: Vec<Option<usize>> = Vec::new();
    loop {
        let event = match reader.read_event() {
            Ok(e) => e,
            Err(e) => panic!("XML mal formé : {e}"),
        };
        let (start, name) = match &event {
            Event::Start(e) => (true, e.local_name().as_ref().to_owned()),
            Event::Empty(e) => (false, e.local_name().as_ref().to_owned()),
            Event::End(_) => {
                let _ = stack.pop();
                let _ = open.pop();
                continue;
            }
            Event::Eof => break,
            _ => continue,
        };
        if let Some(Some(i)) = open.last()
            && let Some((_, children)) = out.get_mut(*i)
        {
            children.push(name.clone());
        }
        let path = stack
            .iter()
            .chain(std::iter::once(&name))
            .cloned()
            .collect::<Vec<_>>()
            .join("/");
        let entity = matches!(
            path.as_str(),
            "Project"
                | "Project/Calendars/Calendar"
                | "Project/Tasks/Task"
                | "Project/Resources/Resource"
                | "Project/Assignments/Assignment"
        );
        if start {
            stack.push(name);
            if entity {
                out.push((path, Vec::new()));
                open.push(Some(out.len().saturating_sub(1)));
            } else {
                open.push(None);
            }
        } else if entity {
            out.push((path, Vec::new()));
        }
    }
    out
}

/// Vrai si les noms de `written` présents dans `source` y apparaissent dans
/// le même ordre (ordre réécrit cohérent avec l'ordre source, R-121).
#[must_use]
pub fn order_agrees(written: &[String], source: &[String]) -> bool {
    let mut rest = source.iter();
    written
        .iter()
        .filter(|n| source.contains(n))
        .all(|n| rest.by_ref().any(|s| s == n))
}
