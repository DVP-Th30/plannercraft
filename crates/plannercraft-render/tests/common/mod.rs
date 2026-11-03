//! Outils partagés des tests du rendu : projets construits à la main,
//! maison de démonstration, relecture XML.

#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    reason = "tests : un échec doit s'arrêter net et lisiblement ; chaque fichier n'utilise qu'une partie des outils"
)]

use plannercraft_core::calendar::BuiltinCalendarNames;
use plannercraft_core::{
    Duration, DurationUnit, Lag, Link, LinkType, Minutes, Moment, OutlineLevel, Project, ProjectInit,
    Schedule, ScheduleAnchor, Task, TaskUid, TimeBasis,
};
use quick_xml::events::Event;

/// Instant ISO.
pub fn at(s: &str) -> Moment {
    s.parse().unwrap()
}

/// Projet vide démarrant à `start`, « maintenant » = `start`.
pub fn project(name: &str, start: &str) -> Project {
    Project::new(ProjectInit {
        name: name.to_owned(),
        anchor: ScheduleAnchor::FromStart(at(start)),
        current_date: at(start),
        calendar_names: BuiltinCalendarNames::default(),
    })
}

/// Ajoute une tâche de `days` jours ouvrés au niveau `level` ; rend son UID.
pub fn add(p: &mut Project, name: &str, days: i64, level: u16) -> TaskUid {
    let uid = TaskUid::new(u32::try_from(p.tasks.len() + 1).unwrap());
    let options = p.options;
    let mut t = Task::new(uid, name.to_owned(), &options);
    t.outline_level = OutlineLevel::new(level).unwrap();
    t.duration = Duration::new(
        Minutes::new(days * 480),
        DurationUnit::Day,
        TimeBasis::Working,
        false,
    )
    .unwrap();
    p.tasks.push(t).unwrap();
    uid
}

/// Lien `pred` → `succ` sans décalage.
pub fn link(p: &mut Project, pred: TaskUid, succ: TaskUid, kind: LinkType) {
    p.tasks.get_mut(succ).unwrap().predecessors.push(Link {
        predecessor: pred,
        kind,
        lag: Lag::ZERO,
    });
}

/// Maison de démonstration de l'interface (49 tâches), « maintenant » fixé.
pub fn house() -> (Project, Schedule) {
    let bytes = include_bytes!("../../../plannercraft-ui-egui/assets/demo/house.xml");
    let doc = plannercraft_mspdi::read(
        bytes,
        &plannercraft_mspdi::ReadOptions::new(at("2026-10-17T08:00")),
    )
    .unwrap();
    let schedule = plannercraft_core::schedule(&doc.project);
    (doc.project, schedule)
}

/// Relit `svg` avec quick-xml (bien formé, attributs valides, seules les
/// cinq entités prédéfinies) et rend le texte de chaque élément `text`,
/// entités résolues.
pub fn xml_texts(svg: &str) -> Vec<String> {
    for c in svg.chars() {
        let legal = matches!(c, '\u{9}' | '\u{A}' | '\u{D}' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}')
            || c >= '\u{10000}';
        assert!(legal, "caractère interdit U+{:04X}", u32::from(c));
    }
    let mut reader = quick_xml::Reader::from_str(svg);
    let mut texts = Vec::new();
    let mut current: Option<String> = None;
    let mut depth = 0_usize;
    let mut roots = 0_usize;
    loop {
        match reader
            .read_event()
            .unwrap_or_else(|e| panic!("XML mal formé : {e}"))
        {
            Event::Start(e) => {
                if depth == 0 {
                    roots += 1;
                }
                depth += 1;
                for a in e.attributes() {
                    a.unwrap();
                }
                if e.name().as_ref() == "text" {
                    current = Some(String::new());
                }
            }
            Event::Empty(e) => {
                if depth == 0 {
                    roots += 1;
                }
                for a in e.attributes() {
                    a.unwrap();
                }
            }
            Event::End(e) => {
                depth -= 1;
                if e.name().as_ref() == "text" {
                    texts.push(current.take().unwrap());
                }
            }
            Event::Text(t) => {
                if let Some(s) = current.as_mut() {
                    s.push_str(t.as_ref());
                }
            }
            Event::GeneralRef(r) => {
                assert!(!r.is_char_ref(), "référence numérique inattendue");
                let c = match r.as_ref() {
                    "amp" => '&',
                    "lt" => '<',
                    "gt" => '>',
                    "quot" => '"',
                    "apos" => '\'',
                    other => panic!("entité inattendue : {other}"),
                };
                if let Some(s) = current.as_mut() {
                    s.push(c);
                }
            }
            Event::CData(_) | Event::PI(_) | Event::DocType(_) => panic!("construction XML inattendue"),
            Event::Eof => break,
            Event::Decl(_) | Event::Comment(_) => {}
        }
    }
    assert_eq!(depth, 0);
    assert_eq!(roots, 1, "un seul élément racine");
    texts
}

/// Projet « P » du cahier J8 (repère des exemples) : 1 « Gros œuvre » ⊃
/// 2 « Fondations » 5 j, 3 « Murs » 10 j (2→3 FS), 4 « Livraison » jalon
/// (3→4 FS) ; 5 « Second œuvre » ⊃ 6 « Plâtrerie » 4 j (3→6 FS),
/// 7 « Peinture » 3 j (6→7 FS) ; début lun 19/10/2026 08:00.
pub fn project_p() -> Project {
    let mut p = project("P", "2026-10-19T08:00");
    add(&mut p, "Gros œuvre", 0, 1);
    let t2 = add(&mut p, "Fondations", 5, 2);
    let t3 = add(&mut p, "Murs", 10, 2);
    let t4 = add(&mut p, "Livraison", 0, 2);
    add(&mut p, "Second œuvre", 0, 1);
    let t6 = add(&mut p, "Plâtrerie", 4, 2);
    let t7 = add(&mut p, "Peinture", 3, 2);
    link(&mut p, t2, t3, LinkType::FinishToStart);
    link(&mut p, t3, t4, LinkType::FinishToStart);
    link(&mut p, t3, t6, LinkType::FinishToStart);
    link(&mut p, t6, t7, LinkType::FinishToStart);
    p
}

/// UID `n`.
pub fn uid(n: u32) -> TaskUid {
    TaskUid::new(n)
}
