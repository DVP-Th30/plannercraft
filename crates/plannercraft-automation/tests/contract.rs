//! Contrat J4 (lot L0, ADR-0012) : les squelettes compilent, se construisent
//! et répondent sans paniquer ; le registre est trié et sans doublon ; le
//! format des lots se lit et se relit.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

use std::collections::BTreeSet;

use plannercraft_automation::command::{AnchorSpec, Command, Patch, TaskRef};
use plannercraft_automation::error::codes;
use plannercraft_automation::journal::{Entry, parse_line, to_line};
use plannercraft_automation::registry::{Availability, by_mcp_name, lookup};
use plannercraft_automation::text::Language;
use plannercraft_automation::{ApplyMode, Batch, HistoryError, NewProject, REGISTRY, Session, Workspace};
use plannercraft_core::{Moment, TaskUid};
use plannercraft_mspdi::write::WriteOptions;

fn now() -> Moment {
    "2026-10-19T08:00".parse().unwrap()
}

fn new_session() -> Session {
    let init = NewProject {
        name: "Projet1".to_owned(),
        anchor: AnchorSpec::Start("2026-11-02T08:00".parse().unwrap()),
        lang: None,
    };
    Session::new_project(init, now()).unwrap()
}

#[test]
fn contract_compiles() {
    let mut s = new_session();
    assert_eq!(s.project().tasks.len(), 0);
    assert_eq!(s.generation(), 0);
    assert!(!s.is_dirty());
    assert_eq!(s.origin().name, "Projet1");

    // Lot vide : sans effet (pipeline L2) ; version inconnue : refusée sans
    // rien toucher.
    let mut batch = Batch {
        format_version: 1,
        lang: None,
        ops: Vec::new(),
        label: None,
    };
    let before = s.project().clone();
    for mode in [ApplyMode::Commit, ApplyMode::DryRun] {
        let outcome = s.apply(&batch, mode).unwrap();
        assert!(!outcome.committed);
    }
    batch.format_version = 2;
    let err = s.apply(&batch, ApplyMode::Commit).unwrap_err();
    assert_eq!(err.code, codes::BATCH_UNSUPPORTED_VERSION);
    assert_ne!(err.message(Language::Fr), "");
    assert_eq!(s.project(), &before);
    assert_eq!(s.undo().unwrap_err(), HistoryError::Empty);
    assert_eq!(s.redo().unwrap_err().code(), codes::HISTORY_EMPTY);
    assert_eq!(s.history().entries.len(), 0);
    s.mark_saved();
    assert!(!s.is_dirty());

    // Écriture puis relecture du projet neuf.
    let written = s.save_bytes(&WriteOptions::default());
    assert_ne!(written.xml.len(), 0);
    let origin = s.origin().clone();
    let (reopened, _diagnostics) = Session::open_bytes(&written.xml, origin, now(), Language::En).unwrap();
    assert_eq!(reopened.project().tasks.len(), 0);
    assert_eq!(reopened.text_context().language, Language::En);
    assert!(Session::open_bytes(b"pas du xml", s.origin().clone(), now(), Language::Fr).is_err());

    // Espace de travail, journal, rendu.
    let mut ws = Workspace::new(now(), Language::En);
    assert!(ws.session().is_none());
    let err = ws.call("project.summary", serde_json::json!({})).unwrap_err();
    assert_eq!(err.code, codes::SESSION_NONE);
    let entry = Entry {
        v: 1,
        seq: 7,
        tool: "add_task".to_owned(),
        params: serde_json::json!({ "name": "A" }),
        result: None,
    };
    assert_eq!(parse_line(&to_line(&entry)).unwrap(), entry);
    assert!(parse_line("{").is_err());
    let (svg, report) = plannercraft_render::svg(s.project(), s.schedule(), &gantt_request());
    assert!(svg.starts_with("<svg"));
    assert_eq!(report.bytes, svg.len());
}

fn gantt_request() -> plannercraft_render::GanttRequest {
    plannercraft_render::GanttRequest {
        range: plannercraft_render::RangeSpec::Auto,
        zoom: plannercraft_render::Zoom::Week,
        filter: plannercraft_render::RowFilter::default(),
        language: plannercraft_render::Lang::Fr,
        theme: plannercraft_render::Theme::Light,
        limits: plannercraft_render::RenderLimits::default(),
    }
}

#[test]
fn registry_sorted_and_unique() {
    let ids: Vec<&str> = REGISTRY.iter().map(|s| s.id).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(ids, sorted, "REGISTRY doit être trié par id, sans doublon");
    let names: BTreeSet<&str> = REGISTRY.iter().map(|s| s.mcp_name).collect();
    assert_eq!(names.len(), REGISTRY.len(), "noms MCP en double");
    for spec in REGISTRY {
        assert_eq!(lookup(spec.id).map(|s| s.id), Some(spec.id));
        assert_eq!(by_mcp_name(spec.mcp_name).map(|s| s.id), Some(spec.id));
        assert!(
            spec.mcp_name.len() <= 64
                && spec
                    .mcp_name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
            "nom MCP invalide : {}",
            spec.mcp_name
        );
        let _ = (spec.schema)();
    }
    // Entrées réservées (J6) présentes et non disponibles ; `resource.add`
    // activée en J5 (lot L8).
    // `analysis.explain_date` activée en J6 (lot L7).
    assert_eq!(
        lookup("analysis.explain_date").unwrap().available,
        Availability::Now
    );
    assert_eq!(lookup("analysis.what_if").unwrap().available, Availability::Now);
    assert_eq!(
        lookup("analysis.baseline_variance").unwrap().available,
        Availability::Now
    );
    // Toutes les entrées J6 et J7 activées ; seules les quatre entrées J8
    // sont réservées (contrat J8 L0, `contract_j8.rs`).
    assert_eq!(lookup("baseline.set").unwrap().available, Availability::Now);
    assert!(
        REGISTRY
            .iter()
            .all(|s| !matches!(s.available, Availability::Milestone(m) if m != 8))
    );
    assert_eq!(lookup("resource.add").unwrap().available, Availability::Now);
    assert!(lookup("task.nope").is_none());
}

#[test]
fn contract_batch_json_shape() {
    let json = r#"{
        "format_version": 1,
        "lang": "fr",
        "ops": [
            {"op": "task.add", "name": "A", "as": "$a", "duration": {"text": "5j"}},
            {"op": "task.add", "name": "B", "as": "$b", "at": {"after": "$a"}},
            {"op": "link.add", "pred": "$a", "succ": "$b", "kind": "ss", "lag": {"text": "+2j"}},
            {"op": "task.update", "uid": 3, "notes": null, "name": "C"},
            {"op": "project.set_anchor", "start": "2026-11-02T08:00"},
            {"op": "project.set_status_date", "date": null},
            {"op": "calendar.set_exception", "cal": "project", "from": "2026-11-11", "to": "2026-11-11", "ranges": []}
        ]
    }"#;
    let batch: Batch = serde_json::from_str(json).unwrap();
    assert_eq!(batch.ops.len(), 7);
    let Command::TaskAdd(add) = &batch.ops[0] else {
        panic!("task.add attendu");
    };
    assert_eq!(add.label.as_deref(), Some("$a"));
    let Command::TaskUpdate(update) = &batch.ops[3] else {
        panic!("task.update attendu");
    };
    assert_eq!(update.uid, TaskRef::Uid(TaskUid::new(3)));
    assert_eq!(update.notes, Patch::Clear);
    assert_eq!(update.name, Patch::Set("C".to_owned()));
    assert_eq!(update.duration, Patch::Keep);
    // Aller-retour à l'identique.
    let again: Batch = serde_json::from_value(serde_json::to_value(&batch).unwrap()).unwrap();
    assert_eq!(again, batch);
    // Champ inconnu refusé, au niveau du lot comme d'une commande.
    assert!(serde_json::from_str::<Batch>(r#"{"format_version":1,"ops":[],"x":1}"#).is_err());
    assert!(
        serde_json::from_str::<Batch>(r#"{"format_version":1,"ops":[{"op":"task.add","name":"A","x":1}]}"#)
            .is_err()
    );
    assert!(
        serde_json::from_str::<Batch>(r#"{"format_version":1,"ops":[{"op":"project.set_anchor"}]}"#).is_err()
    );
    // Schéma du lot généré sans panique.
    let schema = serde_json::to_value(schemars::schema_for!(Batch)).unwrap();
    assert!(schema.get("properties").is_some());
}
