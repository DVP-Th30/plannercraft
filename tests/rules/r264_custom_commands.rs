//! R-264 — Commandes des champs personnalisés : `task.update { custom }`
//! (clé `text1` … `flag20`, `null` efface ; nombre lu dans la langue du lot,
//! date ISO comme `start`) et `custom_field.rename { field, alias }` ;
//! refus `custom.unknown_field`, `custom.value_type`, `custom.too_long`,
//! `custom.read_only`, `param.invalid` ; une entrée d'annulation par lot ;
//! MCP : mêmes opérations, aucune date calculée. Source : S94 (« Choose
//! Rename to permanently rename the field in this project. ») ; forme des
//! commandes : hypothèse.

use plannercraft_automation::error::codes;
use plannercraft_automation::text::Language;
use plannercraft_automation::{ApplyMode, Session, Workspace};
use plannercraft_core::{CustomFieldId, CustomKind, CustomValue, TaskUid};
use plannercraft_tests::daily::plan_p;
use plannercraft_tests::moment;
use plannercraft_tests::session::session;
use plannercraft_tests::task_edit::{apply, batch, get, try_apply};
use serde_json::json;

fn id(kind: CustomKind, n: u8) -> CustomFieldId {
    CustomFieldId::new(kind, n).unwrap()
}

#[test]
fn r264_update_and_clear() {
    let mut s = plan_p();
    let before = s.schedule().clone();
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 2, "custom": {
            "text1": "Lot A", "number1": "12,5", "number2": 3, "date1": "2026-11-02T08:00",
            "flag1": true, "flag2": false, "text2": ""
        } }]),
    );
    let t = get(&s, 2);
    assert_eq!(
        t.custom.get(&CustomFieldId::TEXT1),
        Some(&CustomValue::Text("Lot A".to_owned()))
    );
    // Exemple : Nombre1 « 12,5 » ⇒ 125 000 dix-millièmes.
    assert_eq!(
        t.custom.get(&id(CustomKind::Number, 1)),
        Some(&CustomValue::Number(125_000))
    );
    assert_eq!(
        t.custom.get(&id(CustomKind::Number, 2)),
        Some(&CustomValue::Number(30_000))
    );
    assert_eq!(
        t.custom.get(&id(CustomKind::Date, 1)),
        Some(&CustomValue::Date(moment("2026-11-02T08:00")))
    );
    assert_eq!(
        t.custom.get(&id(CustomKind::Flag, 1)),
        Some(&CustomValue::Flag(true))
    );
    // `false` et texte vide : rien de stocké.
    assert_eq!(t.custom.len(), 5);
    // Aucun calcul n'en dépend.
    assert_eq!(s.schedule(), &before);
    // Anglais : point décimal ; `null` efface.
    let mut b = batch(json!([{ "op": "task.update", "uid": 2, "custom": {
        "number1": "-0.25", "text1": null, "flag1": false
    } }]));
    b.lang = Some(Language::En);
    s.apply(&b, ApplyMode::Commit).unwrap();
    let t = get(&s, 2);
    assert_eq!(
        t.custom.get(&id(CustomKind::Number, 1)),
        Some(&CustomValue::Number(-2_500))
    );
    assert!(!t.custom.contains_key(&CustomFieldId::TEXT1));
    assert!(!t.custom.contains_key(&id(CustomKind::Flag, 1)));
}

#[test]
fn r264_rename_alias() {
    let mut s = plan_p();
    apply(
        &mut s,
        json!([{ "op": "custom_field.rename", "field": "text1", "alias": "Lot" }]),
    );
    assert_eq!(
        s.project()
            .custom_fields
            .get(&CustomFieldId::TEXT1)
            .and_then(|d| d.alias.as_deref()),
        Some("Lot")
    );
    // `null` (ou vide) : le champ reprend son nom.
    apply(
        &mut s,
        json!([{ "op": "custom_field.rename", "field": "text1", "alias": null }]),
    );
    assert!(s.project().custom_fields.is_empty());
    apply(
        &mut s,
        json!([{ "op": "custom_field.rename", "field": "flag20", "alias": "Validé" }]),
    );
    apply(
        &mut s,
        json!([{ "op": "custom_field.rename", "field": "flag20", "alias": "" }]),
    );
    assert!(s.project().custom_fields.is_empty());
}

#[test]
fn r264_errors() {
    let mut s = plan_p();
    let project = s.project().clone();
    let refused = |s: &mut Session, ops: serde_json::Value| try_apply(s, ops).unwrap_err();
    // Champ inconnu.
    for key in ["text31", "cost1", "Text1", "number0"] {
        let e = refused(
            &mut s,
            json!([{ "op": "task.update", "uid": 2, "custom": { key: "x" } }]),
        );
        assert_eq!(e.code, codes::CUSTOM_UNKNOWN_FIELD, "{key}");
        assert_eq!(e.field.as_deref(), Some(format!("custom.{key}").as_str()));
        assert_eq!(e.subject, Some(plannercraft_core::Subject::Task(TaskUid::new(2))));
    }
    let e = refused(
        &mut s,
        json!([{ "op": "custom_field.rename", "field": "date11", "alias": "x" }]),
    );
    assert_eq!(e.code, codes::CUSTOM_UNKNOWN_FIELD);
    // Type : texte pour un nombre, nombre illisible, date illisible,
    // indicateur en texte, nombre JSON pour un texte, au-delà de 10^14.
    for (key, value) in [
        ("number1", json!("douze")),
        ("number1", json!("1e5")),
        ("number1", json!(1e20)),
        ("number1", json!("100000000000001")),
        ("date1", json!("demain")),
        ("flag1", json!("oui")),
        ("text1", json!(12)),
        ("date1", json!(true)),
    ] {
        let e = refused(
            &mut s,
            json!([{ "op": "task.update", "uid": 2, "custom": { key: value } }]),
        );
        assert_eq!(e.code, codes::CUSTOM_VALUE_TYPE, "{key} {value}");
    }
    // Trop long : texte de 256 caractères, alias de 51.
    let e = refused(
        &mut s,
        json!([{ "op": "task.update", "uid": 2, "custom": { "text1": "é".repeat(256) } }]),
    );
    assert_eq!(e.code, codes::CUSTOM_TOO_LONG);
    let e = refused(
        &mut s,
        json!([{ "op": "custom_field.rename", "field": "text1", "alias": "a".repeat(51) }]),
    );
    assert_eq!(e.code, codes::CUSTOM_TOO_LONG);
    assert_eq!(e.field.as_deref(), Some("alias"));
    // Forme : `custom` qui n'est pas un objet, champ inconnu de la commande.
    for ops in [
        json!([{ "op": "task.update", "uid": 2, "custom": "text1" }]),
        json!([{ "op": "custom_field.rename", "field": "text1", "name": "x" }]),
    ] {
        let mut v = serde_json::Map::new();
        v.insert("format_version".to_owned(), json!(1));
        v.insert("ops".to_owned(), ops);
        let e = plannercraft_automation::Batch::from_json(serde_json::Value::Object(v)).unwrap_err();
        assert_eq!(e.code, codes::PARAM_INVALID);
    }
    // Atomicité : rien n'a changé.
    assert_eq!(s.project(), &project);
}

#[test]
fn r264_read_only_foreign_field() {
    // Champ défini par le fichier avec une formule (R-265) : ni valeur ni
    // alias modifiables.
    let mut s = plan_p();
    let mut p = s.project().clone();
    p.extras.0.push(plannercraft_core::ExtraEntry {
        origin: "mspdi".to_owned(),
        key: "ExtendedAttributes".to_owned(),
        payload: "<ExtendedAttributes><ExtendedAttribute><FieldID>188743734</FieldID>\
                  <Formula>[Name]</Formula></ExtendedAttribute></ExtendedAttributes>"
            .to_owned(),
    });
    s = session(p);
    for ops in [
        json!([{ "op": "task.update", "uid": 2, "custom": { "text2": "x" } }]),
        json!([{ "op": "custom_field.rename", "field": "text2", "alias": "x" }]),
    ] {
        let e = try_apply(&mut s, ops).unwrap_err();
        assert_eq!(e.code, codes::CUSTOM_READ_ONLY);
    }
    // Les autres champs restent modifiables.
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 2, "custom": { "text1": "x" } }]),
    );
}

#[test]
fn r264_undo_one_entry() {
    // Exemple : `custom_field.rename text1 "Lot"` puis `task.update 2 {
    // text1: "GO" }` ⇒ deux entrées ; chaque annulation défait la sienne.
    let mut s = plan_p();
    let start = s.project().clone();
    let len = s.history().entries.len();
    apply(
        &mut s,
        json!([{ "op": "custom_field.rename", "field": "text1", "alias": "Lot" }]),
    );
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 2, "custom": { "text1": "GO", "number3": "4" } }]),
    );
    assert_eq!(s.history().entries.len(), len + 2);
    let renamed = {
        let mut p = start.clone();
        p.custom_fields = s.project().custom_fields.clone();
        p
    };
    s.undo().unwrap();
    assert_eq!(s.project(), &renamed);
    s.undo().unwrap();
    assert_eq!(s.project(), &start);
    s.redo().unwrap();
    s.redo().unwrap();
    assert_eq!(get(&s, 2).custom.len(), 2);
    // `dry_run` : rien n'est écrit.
    let b = batch(json!([{ "op": "task.update", "uid": 3, "custom": { "flag1": true } }]));
    s.apply(&b, ApplyMode::DryRun).unwrap();
    assert!(get(&s, 3).custom.is_empty());
}

#[test]
fn r264_mcp_same_ops() {
    // Surface MCP (`Workspace::call`) : `update_task` et `rename_custom_field`
    // (entrées `task.update` et `custom_field.rename`), `apply_batch` aussi ;
    // `dry_run` ; une entrée d'historique par appel.
    let mut ws = Workspace::new(moment("2026-11-02T08:00"), Language::Fr);
    ws.call(
        "project.new",
        json!({ "name": "Essai", "start": "2026-11-02T08:00" }),
    )
    .unwrap();
    let out = ws
        .call(
            "batch.apply",
            json!({ "ops": [{ "op": "task.add", "name": "A", "as": "$a" }] }),
        )
        .unwrap();
    let a = out["labels"]["$a"].as_u64().unwrap();
    let spec = plannercraft_automation::registry::lookup("custom_field.rename").unwrap();
    assert_eq!(spec.mcp_name, "rename_custom_field");
    assert!(spec.is_listed() && spec.dry_run && spec.undoable);
    ws.call(
        "custom_field.rename",
        json!({ "field": "number1", "alias": "Quantité", "dry_run": true }),
    )
    .unwrap();
    assert!(ws.session().unwrap().project().custom_fields.is_empty());
    ws.call(
        "custom_field.rename",
        json!({ "field": "number1", "alias": "Quantité" }),
    )
    .unwrap();
    ws.call(
        "task.update",
        json!({ "uid": a, "custom": { "number1": "12,5" } }),
    )
    .unwrap();
    let session = ws.session().unwrap();
    assert_eq!(session.history().entries.len(), 3);
    let t = session
        .project()
        .tasks
        .get(TaskUid::new(u32::try_from(a).unwrap()))
        .unwrap();
    assert_eq!(
        t.custom.get(&id(CustomKind::Number, 1)),
        Some(&CustomValue::Number(125_000))
    );
    let e = ws
        .call("task.update", json!({ "uid": a, "custom": { "text99": "x" } }))
        .unwrap_err();
    assert_eq!(e.code, codes::CUSTOM_UNKNOWN_FIELD);
    assert_eq!(e.field.as_deref(), Some("custom.text99"));
}

#[test]
fn r264_value_replaces_unreadable_file_value() {
    // Valeur illisible du fichier gardée en `Extras` (R-265) : la valeur
    // posée (ou effacée) par commande la remplace ; les autres champs
    // gardés restent.
    let mut p = plan_p().project().clone();
    let t = p.tasks.get_mut(TaskUid::new(2)).unwrap();
    for payload in [
        "<ExtendedAttribute><FieldID>188743767</FieldID><Value>abc</Value></ExtendedAttribute>",
        "<ExtendedAttribute><FieldID>188743786</FieldID><Value>12</Value></ExtendedAttribute>",
    ] {
        t.extras.0.push(plannercraft_core::ExtraEntry {
            origin: "mspdi".to_owned(),
            key: "ExtendedAttribute".to_owned(),
            payload: payload.to_owned(),
        });
    }
    let mut s = session(p);
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 2, "custom": { "number1": "7" } }]),
    );
    let t = get(&s, 2);
    assert_eq!(t.extras.0.len(), 1);
    assert!(t.extras.0[0].payload.contains("188743786"));
    assert_eq!(
        t.custom.get(&id(CustomKind::Number, 1)),
        Some(&CustomValue::Number(70_000))
    );
}
