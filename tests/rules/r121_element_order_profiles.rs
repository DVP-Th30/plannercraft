//! R-121 — Écriture : ordre des éléments, profils observé et strict.

use plannercraft_core::{Guid, Task, TaskUid};
use plannercraft_mspdi::ReadOptions;
use plannercraft_mspdi::write::{Computed, OrderProfile, WriteOptions, write};
use plannercraft_tests::mspdi::{entity_children, new_project, order_agrees, text};
use plannercraft_tests::{moment, tests_path};

fn task_children(xml: &str) -> Vec<String> {
    entity_children(xml)
        .into_iter()
        .find(|(path, _)| path == "Project/Tasks/Task")
        .map(|(_, c)| c)
        .unwrap()
}

#[test]
fn r121_observed_and_strict_task_order() {
    let mut proj = new_project("2026-10-19T08:00");
    let opts = proj.options;
    let mut t = Task::new(TaskUid::new(1), "A".to_owned(), &opts);
    t.guid = Some("6C0BB28C-F383-EF11-BD95-6245B5077038".parse::<Guid>().unwrap());
    proj.tasks.push(t).unwrap();
    let observed = text(&write(&proj, Computed::Omit, &WriteOptions::default()));
    let names = task_children(&observed);
    assert_eq!(
        names[..8],
        ["UID", "GUID", "ID", "Name", "Active", "Manual", "Type", "IsNull"]
    );
    let strict = String::from_utf8(
        write(
            &proj,
            Computed::Omit,
            &WriteOptions {
                profile: OrderProfile::Strict,
                ..WriteOptions::default()
            },
        )
        .xml,
    )
    .unwrap();
    let names = task_children(&strict);
    assert_eq!(names[..5], ["UID", "ID", "Name", "Type", "IsNull"]);
    assert_eq!(names[names.len() - 2..], ["Active", "Manual"]);
    assert!(!names.iter().any(|n| n == "GUID"));
    // Projet : `GUID` après `Name` dans le profil observé seulement.
    proj.info.guid = Some("6C0BB28C-F383-EF11-BD95-6245B5077038".parse::<Guid>().unwrap());
    let observed = text(&write(&proj, Computed::Omit, &WriteOptions::default()));
    let root = &entity_children(&observed)[0].1;
    let i = root.iter().position(|n| n == "Name").unwrap();
    assert_eq!(root[i + 1], "GUID");
}

#[test]
fn r121_corpus_order_agrees_with_the_source() {
    // Fichiers du corpus relus puis écrits (profil observé) : pour chaque
    // entité, les éléments réécrits présents dans la source y apparaissent
    // dans le même ordre. Corpus absent : rien à vérifier.
    let dir = tests_path("corpus/files");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    for entry in entries {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("xml") {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        let doc = plannercraft_mspdi::read(source.as_bytes(), &ReadOptions::new(moment("2026-01-01T00:00")))
            .unwrap();
        let out = text(&write(
            &doc.project,
            Computed::Recorded(&doc.recorded),
            &WriteOptions::default(),
        ));
        let before = entity_children(source.trim_start_matches('\u{feff}'));
        let after = entity_children(&out);
        let of = |list: &[(String, Vec<String>)], wanted: &str| -> Vec<Vec<String>> {
            list.iter()
                .filter(|(proj, _)| proj == wanted)
                .map(|(_, c)| c.clone())
                .collect()
        };
        for entity in [
            "Project",
            "Project/Calendars/Calendar",
            "Project/Tasks/Task",
            "Project/Resources/Resource",
            "Project/Assignments/Assignment",
        ] {
            for (written, source) in of(&after, entity).iter().zip(of(&before, entity).iter()) {
                assert!(
                    order_agrees(written, source),
                    "{} : {entity} réordonné\n{written:?}\n{source:?}",
                    path.display()
                );
            }
        }
    }
}
