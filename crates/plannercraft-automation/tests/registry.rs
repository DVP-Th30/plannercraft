//! Registre des commandes (ADR-0012 D7, lot L3) : table triée, noms MCP
//! conformes, outils du brief présents, réservés non listés, schémas de
//! référence (`tests/golden/schemas/<id>.json`, régénérés par
//! `PLANNERCRAFT_BLESS=1`).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

use std::collections::BTreeSet;
use std::path::PathBuf;

use plannercraft_automation::registry::{Availability, Kind, by_mcp_name, listed, lookup};
use plannercraft_automation::text::Language;
use plannercraft_automation::{Batch, Command, REGISTRY};
use serde_json::{Map, Value};

/// Motif des noms d'outils de l'API Claude : `^[a-zA-Z0-9_-]{1,64}$`.
fn is_claude_tool_name(n: &str) -> bool {
    (1..=64).contains(&n.len())
        && n.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

#[test]
fn registry_sorted_unique_ids_and_mcp_names() {
    let ids: Vec<&str> = REGISTRY.iter().map(|s| s.id).collect();
    for pair in ids.windows(2) {
        assert!(pair[0] < pair[1], "REGISTRY non trié ou en double : {pair:?}");
    }
    let names: BTreeSet<&str> = REGISTRY.iter().map(|s| s.mcp_name).collect();
    assert_eq!(names.len(), REGISTRY.len(), "noms MCP en double");
    for spec in REGISTRY {
        assert_eq!(lookup(spec.id).unwrap().mcp_name, spec.mcp_name);
        assert_eq!(by_mcp_name(spec.mcp_name).unwrap().id, spec.id);
        // Id à point : `famille.action`, minuscules ASCII et `_`.
        let (family, action) = spec.id.split_once('.').unwrap();
        for part in [family, action] {
            assert!(
                !part.is_empty() && part.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
                "id mal formé : {}",
                spec.id
            );
        }
        assert_eq!(spec.since_format, 1);
        assert!(!spec.summary_fr.is_empty() && !spec.summary_en.is_empty());
        assert_eq!(spec.summary(Language::Fr), spec.summary_fr);
        assert_eq!(spec.summary(Language::En), spec.summary_en);
        // Seules les écritures acceptent `dry_run` et s'annulent (`app.*`,
        // genre `Ui`, sans historique).
        let write = spec.kind == Kind::Write;
        assert_eq!(spec.dry_run, write, "{}", spec.id);
        assert_eq!(spec.undoable, write, "{}", spec.id);
    }
    assert!(lookup("task.nope").is_none());
    assert!(by_mcp_name("nope").is_none());
}

#[test]
fn registry_mcp_names_match_claude_pattern() {
    for spec in REGISTRY {
        assert!(
            is_claude_tool_name(spec.mcp_name),
            "nom MCP invalide : {}",
            spec.mcp_name
        );
    }
    assert!(!is_claude_tool_name("add.task"));
    assert!(!is_claude_tool_name(""));
}

/// Outils V1 du brief (§ 6), listés ou réservés.
const BRIEF_TOOLS: &[&str] = &[
    "open_project",
    "project_summary",
    "list_tasks",
    "get_task",
    "critical_path",
    "resource_usage",
    "baseline_variance",
    "render_gantt",
    "add_task",
    "update_task",
    "delete_task",
    "move_task",
    "add_link",
    "remove_link",
    "set_constraint",
    "assign_resource",
    "update_assignment",
    "add_resource",
    "update_progress",
    "set_baseline",
    "edit_calendar",
    "what_if",
    "explain_date",
    "undo",
    "redo",
    "history",
    "save_project",
    "export_mspdi",
];

#[test]
fn registry_brief_tools_present() {
    for name in BRIEF_TOOLS {
        assert!(by_mcp_name(name).is_some(), "outil du brief absent : {name}");
    }
    // Écarts au brief assumés (ADR-0012 D8) : outils J4 en plus.
    for name in ["new_project", "apply_batch", "import_csv"] {
        assert!(by_mcp_name(name).unwrap().is_listed());
    }
}

#[test]
fn registry_reserved_not_listed() {
    // Contrat J6 : plus aucune entrée J6 réservée. Contrat J7 : les cinq
    // écritures réservées au jalon 7 par le lot L0 sont activées par le lot
    // L5 ; restent hors liste les trois entrées `app.*` servies par une
    // application rattachée.
    let reserved: BTreeSet<&str> = REGISTRY
        .iter()
        .filter(|s| matches!(s.available, Availability::Milestone(_)))
        .map(|s| s.id)
        .collect();
    // Contrat J8 (lot L0, ADR-0016) : quatre entrées réservées au jalon 8 ;
    // les trois `style.*` activées par le lot L6, le PDF par le lot L7.
    assert!(reserved.is_empty(), "{reserved:?}");
    let spec = lookup("project.export_pdf").unwrap();
    assert_eq!(spec.available, Availability::Now);
    assert_eq!(spec.mcp_name, "export_pdf");
    assert_eq!(spec.kind, Kind::File);
    assert!(spec.is_listed());
    for id in [
        "baseline.clear",
        "calendar.add",
        "calendar.delete",
        "calendar.set_week",
        "calendar.update",
    ] {
        let spec = lookup(id).unwrap();
        assert_eq!(spec.available, Availability::Now, "{id}");
        assert!(spec.is_listed(), "{id}");
        assert_eq!(spec.kind, Kind::Write, "{id}");
    }
    assert!(listed().all(|s| s.available == Availability::Now));
    // Trois entrées `app.*` hors liste, aucune réservée depuis le lot L7.
    assert_eq!(listed().count(), REGISTRY.len() - 3);
}

/// Contrat J7 (lot L0, ADR-0015 D11) : `app.ui_state`, `app.ui`,
/// `app.screenshot`, genre `Ui`, sans historique ni `dry_run`, jamais
/// listées par une surface « headless ».
#[test]
fn registry_attached_not_listed_headless() {
    let attached: Vec<&str> = REGISTRY
        .iter()
        .filter(|s| s.available == Availability::Attached)
        .map(|s| s.id)
        .collect();
    assert_eq!(attached, ["app.screenshot", "app.ui", "app.ui_state"]);
    for (id, name) in [
        ("app.screenshot", "screenshot"),
        ("app.ui", "ui_action"),
        ("app.ui_state", "ui_state"),
    ] {
        let spec = lookup(id).unwrap();
        assert_eq!(spec.mcp_name, name);
        assert_eq!(spec.kind, Kind::Ui, "{id}");
        assert!(!spec.dry_run && !spec.undoable, "{id}");
        assert!(!spec.is_listed(), "{id}");
        assert!(listed().all(|s| s.id != id));
    }
}

/// Lot L8 J6 : les six outils J6 sont listés (44 outils en tout) ; les
/// trois écritures acceptent `dry_run` et sont annulables, aucune n'est
/// destructive ; les trois analyses sont des lectures calculées.
#[test]
fn registry_j6_tools_listed() {
    for (name, kind) in [
        ("update_progress", Kind::Write),
        ("reschedule_remaining", Kind::Write),
        ("set_baseline", Kind::Write),
        ("explain_date", Kind::Analysis),
        ("what_if", Kind::Analysis),
        ("baseline_variance", Kind::Analysis),
    ] {
        let spec = by_mcp_name(name).unwrap();
        assert!(spec.is_listed(), "{name}");
        assert_eq!(spec.available, Availability::Now, "{name}");
        assert_eq!(spec.kind, kind, "{name}");
        assert_eq!(spec.dry_run, kind == Kind::Write, "{name}");
        assert_eq!(spec.undoable, kind == Kind::Write, "{name}");
    }
    // 44 en J6, 49 depuis le lot L5 de J7, 55 depuis le lot L6 de J8
    // (`registry_j8_style_tools_listed`), 56 depuis le lot L7.
    assert_eq!(listed().count(), LISTED);
}

/// Lot L5 J7 (ADR-0015 D8) : les cinq écritures J7 sont listées (49 outils
/// en tout), acceptent `dry_run` et sont annulables.
#[test]
fn registry_j7_tools_listed() {
    for name in [
        "add_calendar",
        "update_calendar",
        "set_work_week",
        "delete_calendar",
        "clear_baseline",
    ] {
        let spec = by_mcp_name(name).unwrap();
        assert!(spec.is_listed(), "{name}");
        assert_eq!(spec.kind, Kind::Write, "{name}");
        assert!(spec.dry_run && spec.undoable, "{name}");
    }
    assert_eq!(listed().count(), LISTED);
}

/// Outils listés : 49 en J7, plus les six `style.*` du lot L6 de J8,
/// `export_pdf` (lot L7), `rename_custom_field` (seconde passe UX, lot
/// L8, R-264), `shift_tasks` (finitions P1, R-267) et
/// `set_task_text_style` (finitions P2, H-UI-126) et `set_group_by`
/// (finitions P3, H-UI-129).
const LISTED: usize = 60;

/// Lot L6 J8 (ADR-0016 D7, H-UI-59 à H-UI-61) : les six entrées `style.*`
/// sont listées ; trois écritures annulables avec `dry_run`, trois lectures
/// (`save_style_as_template` ne modifie rien : l'interface garde le modèle
/// dans ses préférences).
#[test]
fn registry_j8_style_tools_listed() {
    for (id, name, kind) in [
        ("style.apply", "apply_style", Kind::Write),
        ("style.get", "get_style", Kind::Read),
        ("style.list", "list_styles", Kind::Read),
        ("style.reset_to_template", "reset_style", Kind::Write),
        ("style.save_as_template", "save_style_as_template", Kind::Read),
        ("style.set_bar_role", "set_bar_style", Kind::Write),
    ] {
        let spec = lookup(id).unwrap();
        assert!(spec.is_listed(), "{id}");
        assert_eq!(spec.mcp_name, name, "{id}");
        assert_eq!(spec.kind, kind, "{id}");
        assert_eq!(spec.dry_run && spec.undoable, kind == Kind::Write, "{id}");
    }
    assert_eq!(listed().count(), LISTED);
}

/// Lot L8 (ADR-0013 D9) : les huit outils J5 sont listés, 38 outils en
/// tout ; `delete_resource` est la seule écriture J5 destructive.
#[test]
fn registry_j5_tools_listed() {
    for name in [
        "add_resource",
        "update_resource",
        "delete_resource",
        "list_resources",
        "assign_resource",
        "update_assignment",
        "unassign_resource",
        "resource_usage",
    ] {
        let spec = by_mcp_name(name).unwrap();
        assert!(spec.is_listed(), "{name}");
        assert_eq!(spec.available, Availability::Now, "{name}");
    }
    // 38 outils en J5, 44 en J6, 49 en J7, 55 en J8 (lot L6), 56 (lot L7).
    assert_eq!(listed().count(), LISTED);
    assert_eq!(by_mcp_name("list_resources").unwrap().kind, Kind::Read);
    assert_eq!(by_mcp_name("resource_usage").unwrap().kind, Kind::Analysis);
}

/// Chaque commande d'écriture d'un lot a son entrée, de même id, listée et
/// annulable.
#[test]
fn registry_write_commands_match_batch_ops() {
    let json = r##"{"format_version":1,"ops":[
        {"op":"task.add","name":"A"},
        {"op":"task.update","uid":1},
        {"op":"task.delete","uid":1},
        {"op":"task.move","uid":1,"to":"end"},
        {"op":"task.indent","uid":1},
        {"op":"task.outdent","uid":1},
        {"op":"link.add","pred":1,"succ":2},
        {"op":"link.update","pred":1,"succ":2},
        {"op":"link.remove","pred":1,"succ":2},
        {"op":"task.set_constraint","uid":1,"kind":"asap"},
        {"op":"task.set_deadline","uid":1,"deadline":null},
        {"op":"project.set_info"},
        {"op":"project.set_anchor","finish":"2026-12-18T17:00"},
        {"op":"project.set_status_date"},
        {"op":"calendar.set_exception","cal":1,"from":"2026-11-11","to":"2026-11-11"},
        {"op":"calendar.remove_exception","cal":"project","from":"2026-11-11"},
        {"op":"resource.add","name":"R","as":"$r"},
        {"op":"resource.update","uid":"$r","name":"S"},
        {"op":"resource.delete","uid":1},
        {"op":"assignment.add","task":1,"resource":"$r"},
        {"op":"assignment.update","task":1,"resource":1},
        {"op":"assignment.remove","task":1,"resource":1},
        {"op":"calendar.add","name":"B"},
        {"op":"calendar.update","uid":4,"name":"C"},
        {"op":"calendar.set_week","calendar":"project","days":[]},
        {"op":"calendar.delete","uid":4},
        {"op":"baseline.clear"},
        {"op":"style.apply","template":"sobre"},
        {"op":"style.set_bar_role","role":"critical","color":"#AA0000"},
        {"op":"style.reset_to_template"}
    ]}"##;
    let batch = Batch::from_json_str(json).unwrap();
    assert_eq!(batch.ops.len(), 30);
    for op in &batch.ops {
        let spec = lookup(op.id()).unwrap();
        assert!(spec.is_listed() && spec.undoable, "{}", op.id());
        let v = serde_json::to_value(op).unwrap();
        assert_eq!(v["op"], op.id());
    }
}

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/schemas")
}

/// Vrai si `v` contient une clé `key` à un niveau quelconque.
fn contains_key(v: &Value, key: &str) -> bool {
    match v {
        Value::Object(m) => m.contains_key(key) || m.values().any(|x| contains_key(x, key)),
        Value::Array(a) => a.iter().any(|x| contains_key(x, key)),
        _ => false,
    }
}

/// Un fichier par entrée du registre, listée ou réservée (les entrées
/// réservées J6 portent le schéma de leur contrat, lot L0). Chaque schéma : JSON Schema 2020-12,
/// objet au sommet sans combinateur (exigence des outils de l'API Claude),
/// sans `$ref` (sous-schémas en ligne), convertible en `JsonObject` de rmcp
/// (`serde_json::Map<String, Value>`, `rmcp::model::JsonObject`).
#[test]
fn schemas_golden() {
    let dir = golden_dir();
    let bless = std::env::var_os("PLANNERCRAFT_BLESS").is_some();
    if bless {
        std::fs::create_dir_all(&dir).unwrap();
    }
    let mut expected_files = BTreeSet::new();
    let mut stale = Vec::new();
    for spec in REGISTRY {
        let schema = serde_json::to_value((spec.schema)()).unwrap();
        let object: Map<String, Value> = serde_json::from_value(schema.clone())
            .unwrap_or_else(|_| panic!("{} : schéma non objet", spec.id));
        assert_eq!(
            object.get("$schema").and_then(Value::as_str),
            Some("https://json-schema.org/draft/2020-12/schema"),
            "{}",
            spec.id
        );
        assert_eq!(
            object.get("type").and_then(Value::as_str),
            Some("object"),
            "{}",
            spec.id
        );
        for combinator in ["oneOf", "anyOf", "allOf"] {
            assert!(
                !object.contains_key(combinator),
                "{} : {combinator} au sommet",
                spec.id
            );
        }
        assert!(!contains_key(&schema, "$ref"), "{} : $ref restant", spec.id);

        let file = format!("{}.json", spec.id);
        let path = dir.join(&file);
        expected_files.insert(file);
        let mut text = serde_json::to_string_pretty(&schema).unwrap();
        text.push('\n');
        if bless {
            std::fs::write(&path, &text).unwrap();
        } else if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
            stale.push(spec.id);
        }
    }
    assert!(
        stale.is_empty(),
        "schémas différents de la référence (PLANNERCRAFT_BLESS=1 pour régénérer) : {stale:?}"
    );
    let present: BTreeSet<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        present, expected_files,
        "fichiers de référence en trop ou manquants"
    );
}

/// Le schéma d'un lot décrit chaque commande par sa constante `op`.
#[test]
fn schemas_batch_lists_every_op() {
    let schema = serde_json::to_value((lookup("batch.apply").unwrap().schema)()).unwrap();
    let text = schema.to_string();
    let json = r#"{"format_version":1,"ops":[{"op":"task.add","name":"A"}]}"#;
    let op: Command = Batch::from_json_str(json).unwrap().ops.remove(0);
    assert!(text.contains(&format!("\"{}\"", op.id())));
    for spec in listed().filter(|s| s.kind == Kind::Write && s.id != "batch.apply") {
        assert!(
            text.contains(&format!("\"const\":\"{}\"", spec.id)),
            "{} absent du lot",
            spec.id
        );
    }
}
