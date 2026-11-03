//! R-263 — Modèle des champs personnalisés : Texte1-30, Nombre1-20,
//! Date1-10, Indicateur1-20 ; texte ≤ 255 caractères, nombre en
//! dix-millièmes (point fixe, règle 5) borné à ±10^14 ; `Project.custom_fields`
//! (alias) et `Task.custom` (absent = vide) ; `Project::validate` refuse une
//! valeur du mauvais type. Aucun calcul n'en dépend. Source : S90 (noms des
//! champs), S91 (alias de 50 caractères) ; bornes : hypothèse.

use std::collections::BTreeMap;

use plannercraft_core::model::custom::{
    CUSTOM_NUMBER_SCALE, MAX_CUSTOM_ALIAS_CHARS, MAX_CUSTOM_NUMBER_ABS, MAX_CUSTOM_TEXT_CHARS,
};
use plannercraft_core::{
    CustomFieldDef, CustomFieldId, CustomKind, CustomValue, CustomValueError, Diagnostic, Project, Task,
    TaskUid,
};
use plannercraft_tests::moment;
use plannercraft_tests::mspdi::new_project;

fn id(kind: CustomKind, n: u8) -> CustomFieldId {
    CustomFieldId::new(kind, n).unwrap()
}

fn with_task() -> Project {
    let mut p = new_project("2026-10-19T08:00");
    let o = p.options;
    p.tasks
        .push(Task::new(TaskUid::new(1), "T".to_owned(), &o))
        .unwrap();
    p
}

#[test]
fn r263_value_bounds() {
    // Texte : 255 caractères acceptés (comptés en caractères, pas en
    // octets : « é » vaut 1), 256 refusés.
    assert_eq!(MAX_CUSTOM_TEXT_CHARS, 255);
    assert_eq!(CustomValue::Text("é".repeat(255)).check(), Ok(()));
    assert_eq!(
        CustomValue::Text("é".repeat(256)).check(),
        Err(CustomValueError::TooLong)
    );
    // Nombre : point fixe en dix-millièmes, 12,5 ⇒ 125 000 ; borne 10^14
    // (10^18 dix-millièmes) des deux côtés.
    assert_eq!(CUSTOM_NUMBER_SCALE, 10_000);
    assert_eq!(MAX_CUSTOM_NUMBER_ABS, 100_000_000_000_000 * CUSTOM_NUMBER_SCALE);
    assert_eq!(CustomValue::Number(125_000).check(), Ok(()));
    assert_eq!(CustomValue::Number(MAX_CUSTOM_NUMBER_ABS).check(), Ok(()));
    assert_eq!(CustomValue::Number(-MAX_CUSTOM_NUMBER_ABS).check(), Ok(()));
    assert_eq!(
        CustomValue::Number(MAX_CUSTOM_NUMBER_ABS + 1).check(),
        Err(CustomValueError::OutOfRange)
    );
    assert_eq!(
        CustomValue::Number(i64::MIN).check(),
        Err(CustomValueError::OutOfRange)
    );
    // Date et indicateur : toujours dans leurs bornes.
    assert_eq!(CustomValue::Date(moment("2026-10-19T08:00")).check(), Ok(()));
    assert_eq!(CustomValue::Flag(true).check(), Ok(()));
    // Vides : texte vide et `Flag(false)` ne sont pas stockés.
    assert!(CustomValue::Text(String::new()).is_empty());
    assert!(CustomValue::Flag(false).is_empty());
    assert!(!CustomValue::Flag(true).is_empty());
    assert!(!CustomValue::Number(0).is_empty());
    assert_eq!(MAX_CUSTOM_ALIAS_CHARS, 50);
    // Noms canoniques (S90).
    assert_eq!(id(CustomKind::Text, 1).canonical_name(), "Text1");
    assert_eq!(id(CustomKind::Number, 20).canonical_name(), "Number20");
    assert_eq!(id(CustomKind::Date, 10).canonical_name(), "Date10");
    assert_eq!(id(CustomKind::Flag, 7).canonical_name(), "Flag7");
    assert_eq!(CustomFieldId::TEXT1, id(CustomKind::Text, 1));
}

#[test]
fn r263_validate_type_mismatch() {
    let mut p = with_task();
    let t = p.tasks.get_mut(TaskUid::new(1)).unwrap();
    t.custom
        .insert(id(CustomKind::Text, 1), CustomValue::Text("Lot A".to_owned()));
    t.custom
        .insert(id(CustomKind::Number, 1), CustomValue::Number(125_000));
    assert_eq!(p.validate().finish(), Vec::new());

    // Nombre rangé sous Texte2, texte trop long sous Texte3, nombre hors
    // bornes sous Nombre2 : une erreur chacun.
    let t = p.tasks.get_mut(TaskUid::new(1)).unwrap();
    t.custom.insert(id(CustomKind::Text, 2), CustomValue::Number(1));
    t.custom
        .insert(id(CustomKind::Text, 3), CustomValue::Text("x".repeat(256)));
    t.custom
        .insert(id(CustomKind::Number, 2), CustomValue::Number(i64::MAX));
    let d = p.validate().finish();
    let bad: Vec<CustomFieldId> = d
        .iter()
        .filter_map(|d| match d {
            Diagnostic::TaskCustomInvalid { task, field } if *task == TaskUid::new(1) => Some(*field),
            _ => None,
        })
        .collect();
    assert_eq!(bad.len(), 3, "{d:?}");
    for f in [
        id(CustomKind::Text, 2),
        id(CustomKind::Text, 3),
        id(CustomKind::Number, 2),
    ] {
        assert!(bad.contains(&f), "{f}");
    }
    let one = d
        .iter()
        .find(|d| matches!(d, Diagnostic::TaskCustomInvalid { .. }))
        .unwrap();
    assert_eq!(one.code(), "task.custom_invalid");
    assert_eq!(one.severity(), plannercraft_core::Severity::Error);
}

#[test]
fn r263_json_absent_means_empty() {
    // Sessions et journaux antérieurs : `custom` et `custom_fields`
    // absents ⇒ vides ; un projet sans champ personnalisé s'écrit comme
    // avant (aucune clé nouvelle).
    let p = with_task();
    let json = serde_json::to_value(&p).unwrap();
    assert!(json.get("custom_fields").is_none());
    assert!(json["tasks"][0].get("custom").is_none());
    assert_eq!(serde_json::from_value::<Project>(json).unwrap(), p);

    let mut p = with_task();
    p.custom_fields.insert(
        id(CustomKind::Text, 1),
        CustomFieldDef {
            alias: Some("Lot".to_owned()),
        },
    );
    let t = p.tasks.get_mut(TaskUid::new(1)).unwrap();
    t.custom
        .insert(id(CustomKind::Number, 1), CustomValue::Number(125_000));
    t.custom.insert(
        id(CustomKind::Date, 1),
        CustomValue::Date(moment("2026-11-02T08:00")),
    );
    t.custom.insert(id(CustomKind::Flag, 1), CustomValue::Flag(true));
    let json = serde_json::to_value(&p).unwrap();
    assert_eq!(json["custom_fields"]["text1"]["alias"], "Lot");
    assert_eq!(json["tasks"][0]["custom"]["number1"]["number"], 125_000);
    assert_eq!(json["tasks"][0]["custom"]["date1"]["date"], "2026-11-02T08:00");
    assert_eq!(json["tasks"][0]["custom"]["flag1"]["flag"], true);
    assert_eq!(serde_json::from_value::<Project>(json.clone()).unwrap(), p);
    // Clé inconnue refusée.
    let mut bad = json;
    bad["tasks"][0]["custom"]["text31"] = serde_json::json!({ "text": "x" });
    assert!(serde_json::from_value::<Project>(bad).is_err());
    let _: BTreeMap<CustomFieldId, CustomValue> = BTreeMap::new();
}
