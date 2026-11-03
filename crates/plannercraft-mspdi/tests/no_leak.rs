//! P19 « pas de fuite » (ADR-0004, ADR-0009 § 7.8) : les valeurs calculées
//! par MS Project ne doivent pas entrer dans le `Project`. Retirer du texte
//! XML les `Start`/`Finish` des tâches automatiques, `Early*`, `Late*`,
//! `*Slack`, `Critical`, `Summary` et la `Duration` des récapitulatives ne
//! change pas le projet lu.
//!
//! L'égalité porte sur le `Project` entier, entrée unique du moteur, et sur
//! `schedule(&doc.project)`. Seuls les enfants directs des tâches sont
//! retirés : ceux des éléments conservés en `Extras` (R-087) restent.
//!
//! Aussi : aucun élément calculé (classe C) ni calculé non recalculé
//! (classe X) n'entre dans les `Extras` (spécification J3, § 5.2).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::arithmetic_side_effects,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use std::path::{Path, PathBuf};

use plannercraft_core::schedule;
use plannercraft_mspdi::{ReadOptions, read};

fn opts() -> ReadOptions {
    ReadOptions::new("2026-01-01T00:00".parse().unwrap())
}

fn repo(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(relative)
}

fn xml_files(dir: &Path) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<(String, String)> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("xml"))
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            (name, std::fs::read_to_string(&p).unwrap())
        })
        .collect();
    out.sort();
    out
}

/// Valeur du premier élément `<name>…</name>` d'un fragment.
fn first(fragment: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let start = fragment.find(&open)? + open.len();
    let end = fragment[start..].find(&close)? + start;
    Some(fragment[start..end].trim().to_owned())
}

/// Retire les enfants directs `<name>…</name>` (et `<name/>`) d'un corps
/// d'élément (le texte qui suit sa balise ouvrante) : les éléments de même
/// nom plus profonds (`Start` d'une `TimephasedData` ou d'une `Baseline`,
/// conservées en `Extras`, R-087) restent.
fn strip(fragment: &str, name: &str) -> String {
    let mut out = String::with_capacity(fragment.len());
    let mut depth: i32 = 0;
    let mut rest = fragment;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let tag = &rest[i..];
        let end = tag.find('>').map_or(tag.len(), |j| j + 1);
        let inner = &tag[1..end.saturating_sub(1)];
        let closing = inner.starts_with('/');
        let self_closing = inner.ends_with('/');
        let tag_name: String = inner
            .trim_start_matches('/')
            .chars()
            .take_while(|c| !c.is_whitespace() && *c != '/')
            .collect();
        if depth == 0 && !closing && tag_name == name {
            if self_closing {
                rest = &tag[end..];
                continue;
            }
            let close = format!("</{name}>");
            let Some(j) = tag.find(&close) else {
                out.push_str(tag);
                return out;
            };
            rest = &tag[j + close.len()..];
            continue;
        }
        if closing {
            depth -= 1;
        } else if !self_closing && !inner.starts_with('?') && !inner.starts_with('!') {
            depth += 1;
        }
        out.push_str(&tag[..end]);
        rest = &tag[end..];
    }
    out.push_str(rest);
    out
}

/// Le document sans les valeurs calculées. Récapitulative : déduite de la
/// hiérarchie (R-052), pas du drapeau `Summary`.
fn without_calculated(xml: &str) -> String {
    let (Some(a), Some(b)) = (xml.find("<Tasks>"), xml.find("</Tasks>")) else {
        return xml.to_owned();
    };
    let (head, body, tail) = (&xml[..a], &xml[a..b], &xml[b..]);
    let parts: Vec<&str> = body.split("<Task>").collect();
    // Niveau des tâches non vides, dans l'ordre (premier morceau : `<Tasks>`).
    let levels: Vec<Option<u32>> = parts[1..]
        .iter()
        .map(|p| {
            let null = first(p, "IsNull").is_some_and(|v| v == "1" || v == "true");
            let level = first(p, "OutlineLevel").and_then(|v| v.parse().ok()).unwrap_or(1);
            (!null).then_some(level)
        })
        .collect();
    let mut summary = vec![false; levels.len()];
    let mut next: Option<u32> = None;
    for (i, level) in levels.iter().enumerate().rev() {
        if let Some(l) = level {
            summary[i] = next.is_some_and(|n| n > *l) || *l == 0;
            next = Some(*l);
        }
    }
    let mut out = String::from(head);
    out.push_str(parts[0]);
    for (i, part) in parts[1..].iter().enumerate() {
        let manual = first(part, "Manual").is_some_and(|v| v == "1" || v == "true");
        let mut p = (*part).to_owned();
        for name in [
            "EarlyStart",
            "EarlyFinish",
            "LateStart",
            "LateFinish",
            "TotalSlack",
            "FreeSlack",
            "StartSlack",
            "FinishSlack",
            "Critical",
            "Summary",
        ] {
            p = strip(&p, name);
        }
        if !manual {
            p = strip(&strip(&p, "Start"), "Finish");
            if summary[i] {
                p = strip(&p, "Duration");
            }
        }
        out.push_str("<Task>");
        out.push_str(&p);
    }
    out.push_str(tail);
    out
}

fn check(name: &str, xml: &str) {
    let full = read(xml.as_bytes(), &opts()).unwrap_or_else(|e| panic!("{name} : {e}"));
    let stripped_xml = without_calculated(xml);
    assert_ne!(stripped_xml.len(), xml.len(), "{name} : rien n'a été retiré");
    let stripped = read(stripped_xml.as_bytes(), &opts()).unwrap_or_else(|e| panic!("{name} : {e}"));
    assert_eq!(
        full.project, stripped.project,
        "{name} : une valeur calculée a fuité"
    );
    assert_eq!(
        schedule(&full.project),
        schedule(&stripped.project),
        "{name} : le planning dépend d'une valeur calculée"
    );
}

#[test]
fn p19_fixtures_no_leak() {
    for (name, xml) in xml_files(&repo("tests/fixtures/mspdi")) {
        if xml.contains("<Task>") {
            check(&name, &xml);
        }
    }
}

#[test]
fn p19_corpus_no_leak() {
    for (name, xml) in xml_files(&repo("tests/corpus/files")) {
        check(&name, &xml);
    }
}

/// Aucune clé d'`Extras` ne nomme un élément de classe C ou X.
fn check_extras(name: &str, xml: &str) {
    use plannercraft_mspdi::extras::ExtraKey;
    use plannercraft_mspdi::schema::{Class, Parent, sequence};
    let doc = read(xml.as_bytes(), &opts()).unwrap_or_else(|e| panic!("{name} : {e}"));
    let p = &doc.project;
    let entities = std::iter::once((Parent::Project, &p.extras))
        .chain(p.calendars.iter().map(|c| (Parent::Calendar, &c.extras)))
        .chain(
            p.summary_task
                .iter()
                .chain(p.tasks.iter())
                .map(|t| (Parent::Task, &t.extras)),
        )
        .chain(p.resources.iter().map(|r| (Parent::Resource, &r.extras)))
        .chain(p.assignments.iter().map(|a| (Parent::Assignment, &a.extras)));
    for (parent, extras) in entities {
        for e in &extras.0 {
            let key = ExtraKey::parse(&e.key).unwrap_or_else(|| panic!("{name} : clé {}", e.key));
            let class = sequence(parent).class(&key.name);
            assert!(
                !matches!(class, Some(Class::Computed | Class::Dropped)),
                "{name} : {} ({class:?}) dans les Extras",
                e.key
            );
        }
    }
}

#[test]
fn p19_no_computed_value_in_extras() {
    for (name, xml) in xml_files(&repo("tests/fixtures/mspdi"))
        .into_iter()
        .chain(xml_files(&repo("tests/corpus/files")))
    {
        check_extras(&name, &xml);
    }
}

#[test]
fn stripping_helpers() {
    assert_eq!(strip("<B>1</B><C>2</C><B>3</B>", "B"), "<C>2</C>");
    assert_eq!(strip("<B/><a><B>1</B></a>", "B"), "<a><B>1</B></a>");
    assert_eq!(first("<x><B> 7 </B></x>", "B").as_deref(), Some("7"));
    let xml = "<Project><Tasks><Task><UID>1</UID><OutlineLevel>1</OutlineLevel><Duration>D</Duration>\
               <Start>s</Start></Task><Task><UID>2</UID><OutlineLevel>2</OutlineLevel><Duration>E</Duration>\
               </Task></Tasks></Project>";
    let out = without_calculated(xml);
    assert!(!out.contains("<Duration>D</Duration>") && out.contains("<Duration>E</Duration>"));
    assert!(!out.contains("<Start>"));
}
