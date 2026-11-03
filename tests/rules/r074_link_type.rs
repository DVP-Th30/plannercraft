//! R-074 — Type de lien et liens ignorés (import MSPDI).

use std::fmt::Write as _;

use plannercraft_core::{Link, LinkType, TaskUid};
use plannercraft_mspdi::{ImportCode, LinkIgnoredReason, MspdiDocument, ReadOptions, read};
use plannercraft_tests::{moment, tests_path};

fn fixture() -> MspdiDocument {
    let bytes = std::fs::read(tests_path("fixtures/mspdi/tasks.xml")).unwrap();
    read(&bytes, &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap()
}

fn links(doc: &MspdiDocument, uid: u32) -> Vec<(u32, LinkType)> {
    doc.project
        .tasks
        .get(TaskUid::new(uid))
        .unwrap()
        .predecessors
        .iter()
        .map(|l: &Link| (l.predecessor.get(), l.kind))
        .collect()
}

#[test]
fn r074_type_codes() {
    // 0 FF, 1 FS, 2 SF, 3 SS ; ordre MSPDI conservé.
    let doc = fixture();
    assert_eq!(links(&doc, 3), [(2, LinkType::FinishToStart)]);
    assert_eq!(
        links(&doc, 4),
        [(3, LinkType::FinishToFinish), (2, LinkType::StartToStart)]
    );
    assert_eq!(
        links(&doc, 5),
        [(4, LinkType::StartToFinish), (10, LinkType::FinishToStart)]
    );
}

#[test]
fn r074_ignored_links_are_reported() {
    // UID 5 : 4 en double (le premier, SF, gardé), 99 inconnu, lien sur soi,
    // lien vers un autre projet, type 7 inconnu : tous ignorés et signalés.
    let doc = fixture();
    let link = |code: &ImportCode, element: &str| {
        doc.diagnostics
            .iter()
            .any(|d| &d.code == code && d.element == format!("Tasks/Task/PredecessorLink/{element}"))
    };
    assert!(link(&ImportCode::Duplicate, "PredecessorUID"));
    assert!(link(&ImportCode::DanglingReference, "PredecessorUID"));
    for (reason, element) in [
        (LinkIgnoredReason::SelfLink, "PredecessorUID"),
        (LinkIgnoredReason::CrossProject, "CrossProject"),
        (LinkIgnoredReason::UnknownType, "Type"),
    ] {
        assert!(link(&ImportCode::LinkIgnored { reason }, element), "{reason:?}");
    }
}

#[test]
fn r074_absent_type_is_finish_to_start() {
    let xml = "<Project><StartDate>2026-10-19T08:00:00</StartDate><Tasks>\
               <Task><UID>1</UID><ID>1</ID></Task>\
               <Task><UID>2</UID><ID>2</ID><PredecessorLink><PredecessorUID>1</PredecessorUID></PredecessorLink></Task>\
               </Tasks></Project>";
    let doc = read(xml.as_bytes(), &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap();
    assert_eq!(links(&doc, 2), [(1, LinkType::FinishToStart)]);
}

#[test]
fn r074_cycle_is_accepted() {
    // Cinq tâches en cycle : l'import garde les liens (le moteur les traite,
    // R-065).
    let mut tasks = String::new();
    for uid in 1..=5 {
        let pred = if uid == 1 { 5 } else { uid - 1 };
        let _ = write!(
            tasks,
            "<Task><UID>{uid}</UID><ID>{uid}</ID><PredecessorLink><PredecessorUID>{pred}</PredecessorUID></PredecessorLink></Task>"
        );
    }
    let xml = format!("<Project><StartDate>2026-10-19T08:00:00</StartDate><Tasks>{tasks}</Tasks></Project>");
    let doc = read(xml.as_bytes(), &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap();
    assert_eq!(links(&doc, 1), [(5, LinkType::FinishToStart)]);
    assert_eq!(
        doc.project
            .tasks
            .iter()
            .map(|t| t.predecessors.len())
            .sum::<usize>(),
        5
    );
}
