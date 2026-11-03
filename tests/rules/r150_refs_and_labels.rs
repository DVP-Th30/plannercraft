//! R-150 — Désignation des tâches dans un lot : UID (entier JSON) ou
//! étiquette `"$x"` créée par un `task.add` antérieur du même lot ;
//! étiquette inconnue → `batch.unknown_label`, redéfinie →
//! `batch.duplicate_label`, chaîne sans `$` → `param.invalid` ; l'ID n'est
//! jamais accepté.
//!
//! L3 écrit la résolution (`resolve_task`, `define_label`) ; l'exécution de
//! `task.add` (L4) et l'allocation des UID (L2) n'existent pas encore : les
//! tâches « créées par le lot » sont ajoutées à la main, avec les UID que
//! R-151 leur donnera (UID max 3 → 4, puis 5).

use std::collections::BTreeMap;

use plannercraft_automation::Batch;
use plannercraft_automation::command::{Command, TaskRef, define_label, is_valid_label, resolve_task};
use plannercraft_automation::error::{CommandErrorKind, codes};
use plannercraft_core::calendar::BuiltinCalendarNames;
use plannercraft_core::{Project, ProjectInit, ScheduleAnchor, Subject, Task, TaskUid};
use plannercraft_tests::moment;

/// Projet de 3 tâches, UID 1 à 3.
fn three_tasks() -> Project {
    let mut p = Project::new(ProjectInit {
        name: "P".to_owned(),
        anchor: ScheduleAnchor::FromStart(moment("2026-11-02T08:00")),
        current_date: moment("2026-10-19T08:00"),
        calendar_names: BuiltinCalendarNames::default(),
    });
    for (uid, name) in [(1, "A"), (2, "B"), (3, "C")] {
        let t = Task::new(TaskUid::new(uid), name.to_owned(), &p.options);
        p.tasks.push(t).unwrap();
    }
    p
}

/// Ajoute la tâche `uid` comme le ferait `task.add` (fin du projet).
fn add(p: &mut Project, uid: u32, name: &str) -> TaskUid {
    let t = Task::new(TaskUid::new(uid), name.to_owned(), &p.options);
    p.tasks.push(t).unwrap();
    TaskUid::new(uid)
}

fn label(s: &str) -> TaskRef {
    TaskRef::Label(s.to_owned())
}

#[test]
fn r150_label_resolution() {
    // Lot du cahier : [add "A" as $a, add "B" as $b, link $a→$b] sur un
    // projet de 3 tâches → labels {$a:4, $b:5}.
    let json = r#"{"format_version":1,"ops":[
        {"op":"task.add","name":"A","as":"$a"},
        {"op":"task.add","name":"B","as":"$b"},
        {"op":"link.add","pred":"$a","succ":"$b"}
    ]}"#;
    let batch = Batch::from_json_str(json).unwrap();
    let mut p = three_tasks();
    let mut labels = BTreeMap::new();
    for (op, uid) in batch.ops.iter().take(2).zip([4, 5]) {
        let Command::TaskAdd(a) = op else {
            panic!("task.add attendu")
        };
        let created = add(&mut p, uid, &a.name);
        define_label(a.label.as_deref().unwrap(), created, &mut labels).unwrap();
    }
    assert_eq!(
        labels,
        BTreeMap::from([
            ("$a".to_owned(), TaskUid::new(4)),
            ("$b".to_owned(), TaskUid::new(5))
        ])
    );
    let Command::LinkAdd(link) = &batch.ops[2] else {
        panic!("link.add attendu")
    };
    assert_eq!(
        resolve_task(&link.pred, &labels, &p, "pred").unwrap(),
        TaskUid::new(4)
    );
    assert_eq!(
        resolve_task(&link.succ, &labels, &p, "succ").unwrap(),
        TaskUid::new(5)
    );
    // IDs (position + 1, R-052) : 4 et 5, égaux ici aux UID par hasard.
    assert_eq!(p.tasks.position(TaskUid::new(5)), Some(4));
    // Un UID existant se désigne directement, sans étiquette.
    assert_eq!(
        resolve_task(&TaskRef::Uid(TaskUid::new(2)), &labels, &p, "uid").unwrap(),
        TaskUid::new(2)
    );
}

#[test]
fn r150_unknown_label() {
    let p = three_tasks();
    let labels = BTreeMap::new();
    let err = resolve_task(&label("$nope"), &labels, &p, "succ").unwrap_err();
    assert_eq!(err.code, codes::BATCH_UNKNOWN_LABEL);
    assert_eq!(err.kind, CommandErrorKind::UnknownLabel("$nope".to_owned()));
    assert_eq!(err.field.as_deref(), Some("succ"));
    // UID absent : task.not_found, sujet la tâche ; l'UID 0 (récapitulative
    // du projet) n'est pas une tâche du lot.
    for uid in [0, 9] {
        let err = resolve_task(&TaskRef::Uid(TaskUid::new(uid)), &labels, &p, "uid").unwrap_err();
        assert_eq!(err.code, codes::TASK_NOT_FOUND);
        assert_eq!(err.subject, Some(Subject::Task(TaskUid::new(uid))));
    }
    // Étiquette d'une tâche créée puis supprimée dans le lot : task.not_found.
    let mut q = three_tasks();
    let mut labels = BTreeMap::new();
    let t = add(&mut q, 4, "X");
    define_label("$x", t, &mut labels).unwrap();
    q.tasks.remove(t);
    assert_eq!(
        resolve_task(&label("$x"), &labels, &q, "uid").unwrap_err().code,
        codes::TASK_NOT_FOUND
    );
}

#[test]
fn r150_duplicate_label() {
    let mut p = three_tasks();
    let mut labels = BTreeMap::new();
    let a = add(&mut p, 4, "A");
    define_label("$a", a, &mut labels).unwrap();
    let b = add(&mut p, 5, "B");
    let err = define_label("$a", b, &mut labels).unwrap_err();
    assert_eq!(err.code, codes::BATCH_DUPLICATE_LABEL);
    assert_eq!(err.kind, CommandErrorKind::DuplicateLabel("$a".to_owned()));
    assert_eq!(err.field.as_deref(), Some("as"));
    // Table inchangée : $a désigne toujours la première tâche.
    assert_eq!(labels.get("$a"), Some(&a));
    assert_eq!(labels.len(), 1);
    // Étiquette mal formée dans `as` : param.invalid.
    for bad in ["a", "$", "$a b", "$é"] {
        assert_eq!(
            define_label(bad, b, &mut labels).unwrap_err().code,
            codes::PARAM_INVALID,
            "{bad}"
        );
    }
}

#[test]
fn r150_string_without_dollar_refused() {
    // À la lecture JSON : toute chaîne qui n'est pas une étiquette est
    // refusée, y compris un ID écrit en texte ("2").
    for r in ["\"A\"", "\"2\"", "\"$\"", "\"$a.b\"", "-1", "1.5", "4294967296"] {
        let json = format!(r#"{{"format_version":1,"ops":[{{"op":"task.indent","uid":{r}}}]}}"#);
        let err = Batch::from_json_str(&json).unwrap_err();
        assert_eq!(err.code, codes::PARAM_INVALID, "{r}");
    }
    let long = format!("${}", "a".repeat(65));
    assert!(!is_valid_label(&long));
    assert!(is_valid_label(&format!("${}", "a".repeat(64))));
    assert!(is_valid_label("$fond_de-fouille2"));
    // Construite en Rust, la valeur est refusée à la résolution.
    let p = three_tasks();
    let err = resolve_task(&label("A"), &BTreeMap::new(), &p, "pred").unwrap_err();
    assert_eq!(err.code, codes::PARAM_INVALID);
    assert_eq!(err.field.as_deref(), Some("pred"));
}
