//! R-084 — Récapitulative du projet, ordre et identité des tâches (import
//! MSPDI).

use plannercraft_core::{Minutes, OutlineLevel, TaskId, TaskUid};
use plannercraft_mspdi::{ImportCode, MspdiDocument, ReadOptions, read};
use plannercraft_tests::{moment, tests_path};

fn fixture() -> MspdiDocument {
    let bytes = std::fs::read(tests_path("fixtures/mspdi/null_rows.xml")).unwrap();
    read(&bytes, &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap()
}

fn tasks(body: &str) -> MspdiDocument {
    let xml = format!("<Project><StartDate>2026-10-19T08:00:00</StartDate><Tasks>{body}</Tasks></Project>");
    read(xml.as_bytes(), &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap()
}

#[test]
fn r084_null_rows_keep_ids() {
    // 11 lignes dont 5 vides : ID 1 à 11 dans l'ordre du document, 6 tâches
    // calculées.
    let doc = fixture();
    let p = &doc.project;
    assert_eq!(p.tasks.len(), 11);
    for (i, t) in p.tasks.iter().enumerate() {
        let id = u32::try_from(i + 1).unwrap();
        assert_eq!(p.task_id(t.uid()), Some(TaskId::new(id)));
        assert_eq!(t.uid(), TaskUid::new(100 + id));
    }
    assert_eq!(p.tasks.iter().filter(|t| t.is_null).count(), 5);
    assert_eq!(p.tasks.iter().filter(|t| !t.is_null).count(), 6);
}

#[test]
fn r084_project_summary_is_uid_0() {
    // UID 0, `OutlineLevel 0` → `summary_task`, au premier niveau du modèle.
    let doc = fixture();
    let s = doc.project.summary_task.as_ref().unwrap();
    assert_eq!(s.uid(), TaskUid::PROJECT_SUMMARY);
    assert_eq!(s.outline_level, OutlineLevel::TOP);
    assert!(!doc.project.tasks.contains(TaskUid::PROJECT_SUMMARY));
}

#[test]
fn r084_inconsistencies_reported() {
    let doc = fixture();
    let found = |element: &str, value: &str| {
        doc.diagnostics.iter().any(|d| {
            d.code == ImportCode::Inconsistent
                && d.element == element
                && d.value.as_deref() == Some(value)
                && d.at.is_some()
        })
    };
    // Ligne 9 (UID 109) écrite `ID 13`.
    assert!(found("Tasks/Task/ID", "13"));
    // UID 111 : `Summary 1` alors qu'aucune tâche de niveau supérieur ne la suit.
    assert!(found("Tasks/Task/Summary", "1"));
    // UID 106 : `Summary 1`, conforme à la hiérarchie : pas de diagnostic ;
    // sa durée (calculée) n'est pas dans le modèle.
    let summary_flags = doc
        .diagnostics
        .iter()
        .filter(|d| d.element == "Tasks/Task/Summary")
        .count();
    assert_eq!(summary_flags, 1);
    let t106 = doc.project.tasks.get(TaskUid::new(106)).unwrap();
    assert_eq!(t106.duration.minutes(), Minutes::ZERO);
}

#[test]
fn r084_outline_level_absent_is_top() {
    // UID 103 n'a pas d'`OutlineLevel` : premier niveau.
    let doc = fixture();
    assert_eq!(
        doc.project.tasks.get(TaskUid::new(103)).unwrap().outline_level,
        OutlineLevel::TOP
    );
}

#[test]
fn r084_invalid_identities() {
    let doc = tasks(
        "<Task><UID>1</UID><ID>1</ID><OutlineLevel>0</OutlineLevel></Task>\
         <Task><UID>1</UID><ID>2</ID></Task>\
         <Task><UID>-4</UID></Task>\
         <Task><ID>3</ID></Task>\
         <Task><UID>9</UID><ID>2</ID><OutlineLevel>70000</OutlineLevel></Task>",
    );
    // `OutlineLevel 0` hors UID 0 → niveau 1, signalé ; UID en double : le
    // premier gagne ; UID négatif ou absent : tâche ignorée.
    let p = &doc.project;
    assert_eq!(p.tasks.len(), 2);
    assert_eq!(
        p.tasks.get(TaskUid::new(1)).unwrap().outline_level,
        OutlineLevel::TOP
    );
    assert_eq!(
        p.tasks.get(TaskUid::new(9)).unwrap().outline_level,
        OutlineLevel::TOP
    );
    let has = |code: ImportCode, element: &str| {
        doc.diagnostics
            .iter()
            .any(|d| d.code == code && d.element == element)
    };
    assert!(has(ImportCode::Inconsistent, "Tasks/Task/OutlineLevel"));
    assert!(has(ImportCode::InvalidValue, "Tasks/Task/OutlineLevel"));
    assert!(has(ImportCode::Duplicate, "Tasks/Task/UID"));
    assert!(has(ImportCode::InvalidValue, "Tasks/Task/UID"));
    assert!(has(ImportCode::MissingRequired, "Tasks/Task/UID"));
}
