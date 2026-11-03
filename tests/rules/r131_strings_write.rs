//! R-131 — Chaînes à l'écriture : échappement, caractères interdits,
//! longueur maximale.

use plannercraft_core::{Task, TaskUid};
use plannercraft_mspdi::write::WriteCode;
use plannercraft_tests::mspdi::{new_project, reread, text, written};

#[test]
fn r131_escaping_and_carriage_returns() {
    let mut proj = new_project("2026-10-19T08:00");
    proj.info.name = "A & B <C> \"D\"".to_owned();
    let opts = proj.options;
    let mut t = Task::new(TaskUid::new(1), "T".to_owned(), &opts);
    t.notes = "l1\r\nl2".to_owned();
    proj.tasks.push(t).unwrap();
    let out = written(&proj);
    let xml = text(&out);
    assert!(xml.contains("<Name>A &amp; B &lt;C&gt; &quot;D&quot;</Name>"));
    assert!(xml.contains("<Notes>l1&#13;\nl2</Notes>"));
    let back = reread(&out.xml).project;
    assert_eq!(back.info.name, proj.info.name);
    assert_eq!(back.tasks.get(TaskUid::new(1)).unwrap().notes, "l1\r\nl2");
    assert_eq!(out.diagnostics, Vec::new());
}

#[test]
fn r131_invalid_characters_and_truncation() {
    let mut proj = new_project("2026-10-19T08:00");
    let opts = proj.options;
    proj.tasks
        .push(Task::new(TaskUid::new(1), "a\u{1}b".to_owned(), &opts))
        .unwrap();
    // `Task/Name` : `maxLength` 512 dans pj15 (table du schéma).
    proj.tasks
        .push(Task::new(TaskUid::new(2), "é".repeat(600), &opts))
        .unwrap();
    let out = written(&proj);
    let codes: Vec<(&str, &str)> = out
        .diagnostics
        .iter()
        .map(|d| (d.code.code(), d.element.as_str()))
        .collect();
    assert_eq!(
        codes,
        [
            ("mspdi.write.invalid_char", "Tasks/Task/Name"),
            ("mspdi.write.truncated", "Tasks/Task/Name")
        ]
    );
    assert_eq!(out.diagnostics[1].code, WriteCode::Truncated { max: 512 });
    let back = reread(&out.xml).project;
    assert_eq!(back.tasks.get(TaskUid::new(1)).unwrap().name, "a\u{FFFD}b");
    assert_eq!(back.tasks.get(TaskUid::new(2)).unwrap().name, "é".repeat(512));
}
