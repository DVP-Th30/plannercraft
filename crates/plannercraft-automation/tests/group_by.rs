//! Regroupement des tâches (finitions P3, ADR-0022 D4, H-UI-129) :
//! `style.set_group_by`, une entrée d'annulation par appel, stockage dans le
//! style du document (fichier sans style écrit octet pour octet comme
//! avant), `style.get`, `render.gantt` et `project.export_pdf` qui le
//! suivent, modèles qui le gardent.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

use plannercraft_automation::command::group_by::{StyleSetGroupBy, group_by_of, would_change};
use plannercraft_automation::session::DocOrigin;
use plannercraft_automation::text::Language;
use plannercraft_automation::{AnchorSpec, ApplyMode, Batch, CallEnv, NewProject, Session, call_with};
use plannercraft_core::{CustomFieldId, CustomValue};
use plannercraft_mspdi::write::WriteOptions;
use plannercraft_render::style::GroupBy;
use serde_json::{Value, json};

fn now() -> plannercraft_core::Moment {
    "2026-10-19T08:00".parse().unwrap()
}

fn env() -> CallEnv {
    CallEnv::new(now(), Language::Fr)
}

/// Projet de trois tâches (UID 1 à 3).
fn session() -> Session {
    let init = NewProject {
        name: "P".to_owned(),
        anchor: AnchorSpec::Start(now()),
        lang: None,
    };
    let mut s = Session::new_project(init, now()).unwrap();
    let batch = Batch::from_json(json!({
        "format_version": 1,
        "ops": [
            { "op": "task.add", "name": "Fondations", "duration": { "text": "5j" } },
            { "op": "task.add", "name": "Murs", "duration": { "text": "10j" } },
            { "op": "task.add", "name": "Toiture", "duration": { "text": "3j" } }
        ]
    }))
    .unwrap();
    s.apply(&batch, ApplyMode::Commit).unwrap();
    s
}

fn call(s: &mut Session, id: &str, params: Value) -> Value {
    call_with(s, &env(), id, params).unwrap_or_else(|e| panic!("{id} : {e:?}"))
}

fn xml(s: &Session) -> String {
    String::from_utf8(s.save_bytes(&WriteOptions::default()).xml).unwrap()
}

fn reopen(bytes: &[u8]) -> Session {
    let origin = DocOrigin {
        name: "p.xml".to_owned(),
        path: None,
    };
    Session::open_bytes(bytes, origin, now(), Language::Fr).unwrap().0
}

fn text1(s: &mut Session) {
    call(
        s,
        "task.update",
        json!({ "uid": 1, "custom": { "text1": "Lot B" } }),
    );
    call(
        s,
        "task.update",
        json!({ "uid": 2, "custom": { "text1": "Lot A" } }),
    );
}

#[test]
fn h_ui_129_set_group_by_one_undo_and_roundtrip() {
    let mut s = session();
    let plain = xml(&s);
    // Fichier sans style : relu et écrit octet pour octet.
    assert_eq!(xml(&reopen(plain.as_bytes())), plain);
    // « Aucun groupe » sans style : rien à écrire.
    let none = StyleSetGroupBy::default();
    assert!(!would_change(s.project(), &none));
    call(&mut s, "style.set_group_by", json!({ "group_by": null }));
    assert_eq!(xml(&s), plain);
    let entries = s.history().entries.len();
    call(
        &mut s,
        "style.set_group_by",
        json!({ "group_by": { "custom": "text1" } }),
    );
    assert_eq!(s.history().entries.len(), entries + 1);
    assert_eq!(
        group_by_of(s.project()),
        Some(GroupBy::Custom(CustomFieldId::TEXT1))
    );
    let grouped = xml(&s);
    assert!(grouped.contains("plannercraft-style"));
    let opened = reopen(grouped.as_bytes());
    assert_eq!(group_by_of(opened.project()), group_by_of(s.project()));
    assert_eq!(xml(&opened), grouped);
    // Rien dans le MSPDI hors de l'instruction de style.
    let without = String::from_utf8(
        opened
            .save_bytes(&WriteOptions {
                style: false,
                ..WriteOptions::default()
            })
            .xml,
    )
    .unwrap();
    assert_eq!(without, plain);
    // Même regroupement : geste sans effet.
    let same: StyleSetGroupBy = serde_json::from_value(json!({ "group_by": { "custom": "text1" } })).unwrap();
    assert!(!would_change(s.project(), &same));
    // Retiré, puis annulé : octets d'avant.
    call(&mut s, "style.set_group_by", json!({}));
    assert_eq!(group_by_of(s.project()), None);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(xml(&s), grouped);
}

#[test]
fn h_ui_129_get_style_exposes_group_by_and_refusals() {
    let mut s = session();
    call(&mut s, "style.set_group_by", json!({ "group_by": "critical" }));
    let view = call(&mut s, "style.get", json!({}));
    assert_eq!(view["resolved"]["group_by"], "critical");
    let before = s.project().clone();
    for bad in [
        json!({ "custom": "number1" }),
        json!({ "custom": "text99" }),
        json!("lot"),
    ] {
        assert!(call_with(&mut s, &env(), "style.set_group_by", json!({ "group_by": bad })).is_err());
    }
    assert_eq!(s.project(), &before);
}

#[test]
fn h_ui_129_templates_keep_group_by_and_personal_template_drops_it() {
    let mut s = session();
    call(&mut s, "style.set_group_by", json!({ "group_by": "milestone" }));
    call(&mut s, "style.apply", json!({ "template": "sobre" }));
    assert_eq!(group_by_of(s.project()), Some(GroupBy::Milestone));
    call(&mut s, "style.reset_to_template", json!({}));
    assert_eq!(group_by_of(s.project()), Some(GroupBy::Milestone));
    let named = call(&mut s, "style.save_as_template", json!({ "name": "Mien" }));
    assert!(named["style"]["over"].get("group_by").is_none(), "{named}");
    // `reset` retire le style, regroupement compris.
    call(&mut s, "style.apply", json!({ "reset": true }));
    assert_eq!(group_by_of(s.project()), None);
}

#[test]
fn h_ui_129_render_gantt_and_pdf_follow_document_grouping() {
    let mut s = session();
    text1(&mut s);
    let plain = call(&mut s, "render.gantt", json!({}));
    assert!(!plain["svg"].as_str().unwrap().contains("Lot A (1 tâche)"));
    call(
        &mut s,
        "style.set_group_by",
        json!({ "group_by": { "custom": "text1" } }),
    );
    for params in [json!({}), json!({ "style": "sobre" })] {
        let view = call(&mut s, "render.gantt", params.clone());
        let svg = view["svg"].as_str().unwrap();
        let a = svg
            .find("Lot A (1 tâche)")
            .unwrap_or_else(|| panic!("{params} : {svg}"));
        let b = svg.find("Lot B (1 tâche)").unwrap();
        let none = svg.find("(sans valeur) (1 tâche)").unwrap();
        assert!(a < b && b < none, "ordre des groupes");
    }
    assert_eq!(
        s.project()
            .tasks
            .get(plannercraft_core::TaskUid::new(2))
            .unwrap()
            .custom
            .get(&CustomFieldId::TEXT1),
        Some(&CustomValue::Text("Lot A".to_owned()))
    );
    // PDF : trois tâches et trois en-têtes de groupe (Lot A, Lot B, sans
    // valeur), contre trois lignes sans regroupement.
    let params: plannercraft_automation::command::params::ExportPdfParams =
        serde_json::from_value(json!({ "path": "p.pdf" })).unwrap();
    let grouped = plannercraft_automation::pdf::export(&s, &params, Language::Fr).unwrap();
    assert_eq!(grouped.report.shown, 6);
    call(&mut s, "style.set_group_by", json!({ "group_by": null }));
    let plain = plannercraft_automation::pdf::export(&s, &params, Language::Fr).unwrap();
    assert_eq!(plain.report.shown, 3);
}
