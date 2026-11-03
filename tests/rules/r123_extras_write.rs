//! R-123 — Écriture des `Extras`.

use plannercraft_core::{BaselineNumber, ExtraEntry, Extras, Moment, Task, TaskBaseline, TaskUid};
use plannercraft_mspdi::write::{Computed, OrderProfile, WriteCode, WriteOptions, write};
use plannercraft_tests::mspdi::{compact, new_project, reread, text, written};

fn extra(origin: &str, key: &str, payload: &str) -> ExtraEntry {
    ExtraEntry {
        origin: origin.to_owned(),
        key: key.to_owned(),
        payload: payload.to_owned(),
    }
}

fn project_with(extras: Vec<ExtraEntry>) -> plannercraft_core::Project {
    let mut proj = new_project("2026-10-19T08:00");
    let mut t = Task::new(TaskUid::new(1), "A".to_owned(), &proj.options);
    t.extras = Extras(extras);
    proj.tasks.push(t).unwrap();
    proj
}

#[test]
fn r123_extras_are_written_at_their_place() {
    let proj = project_with(vec![
        extra("mspdi", "^>Zzz", "<Zzz/>"),
        extra(
            "mspdi",
            "DurationFormat>FreeformDurationFormat",
            "<FreeformDurationFormat>7</FreeformDurationFormat>",
        ),
        extra(
            "mspdi",
            "ExtendedAttribute",
            "<ExtendedAttribute><FieldID>1</FieldID><Value>a</Value></ExtendedAttribute>",
        ),
    ]);
    let out = written(&proj);
    let xml = compact(&text(&out));
    assert!(xml.contains("<Task><Zzz/><UID>1</UID>"));
    assert!(xml.contains(
        "<DurationFormat>39</DurationFormat><FreeformDurationFormat>7</FreeformDurationFormat><Work>"
    ));
    assert!(xml.contains("<CommitmentType") || xml.contains("<ExtendedAttribute><FieldID>1</FieldID>"));
    assert_eq!(out.diagnostics, Vec::new());
    // Relu : mêmes `Extras` (ordre canonique).
    let back = reread(&out.xml).project;
    assert_eq!(
        back.tasks.get(TaskUid::new(1)).unwrap().extras,
        proj.tasks.get(TaskUid::new(1)).unwrap().extras
    );
}

#[test]
fn r123_invalid_and_foreign_extras_are_not_written() {
    let proj = project_with(vec![
        // Nom modélisé (M) : aucun emplacement.
        extra("mspdi", "Name", "<Name>x</Name>"),
        // Deux éléments racine.
        extra("mspdi", "Contact", "<Contact>x</Contact><Evil/>"),
        // Clé illisible.
        extra("mspdi", "a b", "<a/>"),
        // Origine autre que MSPDI : gardée dans le modèle, jamais écrite.
        extra("plannercraft", "Hyperlink", "<Hyperlink>h</Hyperlink>"),
    ]);
    let out = written(&proj);
    let xml = text(&out);
    assert!(!xml.contains("<Evil") && !xml.contains("<Hyperlink") && !xml.contains("<Name>x</Name>"));
    let codes: Vec<(&str, &str)> = out
        .diagnostics
        .iter()
        .map(|d| (d.code.code(), d.element.as_str()))
        .collect();
    assert_eq!(
        codes,
        [
            ("mspdi.write.extra_invalid", "Tasks/Task/Name"),
            ("mspdi.write.extra_invalid", "Tasks/Task/Contact"),
            ("mspdi.write.extra_invalid", "Tasks/Task/a b"),
        ]
    );
    assert_eq!(out.diagnostics[0].code, WriteCode::ExtraInvalid);
}

#[test]
fn r123_strict_profile_drops_out_of_schema_names() {
    let proj = project_with(vec![
        extra(
            "mspdi",
            "DurationFormat>FreeformDurationFormat",
            "<FreeformDurationFormat>7</FreeformDurationFormat>",
        ),
        extra(
            "mspdi",
            "ExtendedAttribute",
            "<ExtendedAttribute><FieldID>1</FieldID></ExtendedAttribute>",
        ),
    ]);
    let out = write(
        &proj,
        Computed::Omit,
        &WriteOptions {
            profile: OrderProfile::Strict,
            ..WriteOptions::default()
        },
    );
    let xml = text(&out);
    assert!(!xml.contains("FreeformDurationFormat"));
    assert!(xml.contains("<ExtendedAttribute><FieldID>1</FieldID></ExtendedAttribute>"));
    assert_eq!(out.diagnostics, Vec::new());
}

#[test]
fn r123_baseline_scope_and_root_prefixes() {
    let mut proj = project_with(vec![extra(
        "mspdi",
        "Baseline#2/TimephasedData",
        "<TimephasedData><Type>4</Type></TimephasedData>",
    )]);
    let t = proj.tasks.get_mut(TaskUid::new(1)).unwrap();
    t.baselines.insert(
        BaselineNumber::new(2).unwrap(),
        TaskBaseline {
            start: Some("2026-10-19T08:00".parse::<Moment>().unwrap()),
            ..TaskBaseline::default()
        },
    );
    proj.extras = Extras(vec![
        extra("mspdi", "@xmlns:p", "urn:p"),
        extra("mspdi", "Assignments>Pp", "<p:Pp/>"),
    ]);
    let out = written(&proj);
    let xml = compact(&text(&out));
    // Racine : déclaration réémise ; enfant de la ligne de base 2 dans sa
    // ligne de base (`TimephasedData` en tête dans pj15).
    assert!(xml.contains("<Project xmlns=\"http://schemas.microsoft.com/project\" xmlns:p=\"urn:p\">"));
    assert!(xml.contains("<Baseline><TimephasedData><Type>4</Type></TimephasedData><Number>2</Number>"));
    let back = reread(&out.xml).project;
    assert_eq!(
        back.tasks.get(TaskUid::new(1)).unwrap().extras,
        proj.tasks.get(TaskUid::new(1)).unwrap().extras
    );
    let keys: Vec<&str> = back.extras.0.iter().map(|e| e.key.as_str()).collect();
    // `Assignments` (vide, omis) n'est pas une ancre réécrite : l'élément
    // relu s'ancre sur `Tasks`, toujours écrit ; `CurrencyCode` ajouté
    // (R-130).
    assert_eq!(keys, ["@xmlns:p", "CurrencyCode", "Tasks>Pp"]);
}
