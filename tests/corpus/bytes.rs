//! Octets identiques des contrats pour un planning en minutes entières
//! (ADR-0039 D5, lot B2).
//!
//! Le moteur compte en dixièmes de minute depuis l'ADR-0039 ; les contrats
//! (modèle sérialisé, journal des commandes, sauvegarde automatique, vues
//! JSON, MSPDI écrit) doivent rester, octet pour octet, ceux d'avant le
//! lot B2 tant que toutes les valeurs sont des minutes entières. Les
//! empreintes de référence (`tests/fixtures/contract_bytes.txt`) ont été
//! produites par ce même scénario sur `main` à `d3b6fda` (avant B2) ;
//! `PLANNERCRAFT_BYTES_PRINT=1` imprime la table au lieu de la comparer.
//!
//! Plannings : l'exemple fourni avec l'application (toujours) et deux
//! fichiers du corpus MSPDI (`practicemap`, `shopmetrics`), lus seulement
//! s'ils sont présents (exigés sous `PLANNERCRAFT_CORPUS=required`, job CI
//! `corpus`).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io;
use std::rc::Rc;

use plannercraft_automation::journal::{Entry, to_line};
use plannercraft_automation::recovery::{Journal, Store};
use plannercraft_automation::text::Language;
use plannercraft_automation::{DocOrigin, Session, Workspace};
use plannercraft_mspdi::write::WriteOptions;
use plannercraft_tests::corpus::manifest::sha256_hex;
use plannercraft_tests::{moment, tests_path};
use serde_json::{Value, json};

/// Écrivain de journal en mémoire (base et lignes).
#[derive(Clone, Default)]
struct MemStore(Rc<RefCell<(Vec<u8>, Vec<u8>)>>);

impl Store for MemStore {
    fn replace_base(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.0.borrow_mut().0 = bytes.to_vec();
        Ok(())
    }
    fn append(&mut self, line: &[u8]) -> io::Result<()> {
        self.0.borrow_mut().1.extend_from_slice(line);
        Ok(())
    }
    fn truncate_ops(&mut self) -> io::Result<()> {
        self.0.borrow_mut().1.clear();
        Ok(())
    }
    fn remove(self: Box<Self>) -> io::Result<()> {
        Ok(())
    }
}

/// Résultat d'un appel, erreur comprise (sérialisée telle quelle).
fn call(ws: &mut Workspace, tool: &str, params: &Value) -> Value {
    match ws.call(tool, params.clone()) {
        Ok(v) => v,
        Err(e) => serde_json::to_value(&e).unwrap(),
    }
}

/// UID des feuilles actives planifiées, dans l'ordre du planning.
fn leaves(ws: &mut Workspace) -> Vec<u64> {
    let list = call(ws, "task.list", &json!({ "summary": false, "limit": 500 }));
    list["tasks"]
        .as_array()
        .map(|tasks| {
            tasks
                .iter()
                .filter(|t| t["milestone"] == json!(false) && t["start"].is_string())
                .filter_map(|t| t["uid"].as_u64())
                .collect()
        })
        .unwrap_or_default()
}

/// Octets de chaque contrat après le scénario, par nom.
fn artifacts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let now = moment("2026-10-17T08:00");
    let origin = DocOrigin {
        name: "P".to_owned(),
        path: None,
    };
    let (session, _) = Session::open_bytes(bytes, origin, now, Language::Fr).unwrap();
    let store = MemStore::default();
    let mut recovery = Journal::start(Box::new(store.clone()), &session).unwrap();
    let mut ws = Workspace::new(now, Language::Fr);
    ws.set_session(session);
    let mut out = BTreeMap::new();

    let leaves = leaves(&mut ws);
    assert!(leaves.len() >= 4, "au moins quatre feuilles planifiées");
    let (a, b, c, d) = (leaves[0], leaves[1], leaves[2], leaves[3]);
    let start = call(&mut ws, "task.get", &json!({ "uid": a }))["start"].clone();
    // Scénario : durées, décalages, travail, dates, avancement, ligne de
    // base, décalage de tâches, annulation et rétablissement.
    let script: Vec<(&str, Value)> = vec![
        ("task.update", json!({ "uid": a, "duration": { "text": "3j" } })),
        (
            "task.update",
            json!({ "uid": b, "duration": { "minutes": 1500, "unit": "hour" } }),
        ),
        (
            "link.add",
            json!({ "pred": a, "succ": d, "kind": "ss", "lag": { "text": "+2j" } }),
        ),
        (
            "link.add",
            json!({ "pred": b, "succ": d, "lag": { "minutes": -240, "unit": "hour" } }),
        ),
        (
            "task.set_constraint",
            json!({ "uid": c, "kind": "snet", "date": start }),
        ),
        ("resource.add", json!({ "name": "Équipe B2" })),
        (
            "assignment.add",
            json!({ "task": c, "resource": 0, "work": { "text": "20h" } }),
        ),
        (
            "progress.update",
            json!({ "task": a, "percent_complete": { "text": "40%" } }),
        ),
        ("baseline.set", json!({})),
        ("task.shift", json!({ "tasks": [b], "by": { "text": "1j" } })),
        ("history.undo", json!({})),
        ("history.redo", json!({})),
    ];
    let mut journal = String::new();
    let mut resource = json!(0);
    for (seq, (tool, params)) in script.iter().enumerate() {
        // La ressource affectée est celle que `resource.add` vient de créer.
        let mut params = params.clone();
        if *tool == "assignment.add" {
            params["resource"] = resource.clone();
        }
        let result = call(&mut ws, tool, &params);
        if *tool == "resource.add" {
            resource = result["created_resources"][0].clone();
        }
        if std::env::var("PLANNERCRAFT_BYTES_DEBUG").is_ok() {
            eprintln!(
                "{tool} -> {}",
                result.to_string().chars().take(300).collect::<String>()
            );
        }
        let entry = Entry {
            v: 1,
            seq: u64::try_from(seq).unwrap(),
            tool: (*tool).to_owned(),
            params,
            result: Some(result),
        };
        journal.push_str(&to_line(&entry));
        journal.push('\n');
        recovery.record(ws.session().unwrap(), now).unwrap();
    }
    out.insert("journal".to_owned(), journal.into_bytes());
    let (base, ops) = store.0.borrow().clone();
    out.insert("autosave.base".to_owned(), base);
    out.insert("autosave.ops".to_owned(), ops);

    views(&mut ws, a, c, d, &mut out);

    let session = ws.session_mut().unwrap();
    out.insert("model".to_owned(), serde_json::to_vec(session.project()).unwrap());
    out.insert(
        "mspdi".to_owned(),
        session.save_bytes(&WriteOptions::default()).xml,
    );
    out
}

/// Vues JSON après le scénario, ajoutées à `out`.
fn views(ws: &mut Workspace, a: u64, c: u64, d: u64, out: &mut BTreeMap<String, Vec<u8>>) {
    let views: Vec<(&str, &str, Value)> = vec![
        ("view.task_list", "task.list", json!({ "limit": 500 })),
        ("view.task_get_a", "task.get", json!({ "uid": a })),
        ("view.task_get_c", "task.get", json!({ "uid": c })),
        ("view.task_get_d", "task.get", json!({ "uid": d })),
        ("view.summary", "project.summary", json!({})),
        ("view.critical_path", "analysis.critical_path", json!({})),
        ("view.resource_list", "resource.list", json!({})),
        ("view.usage", "analysis.resource_usage", json!({ "by": "day" })),
        (
            "view.variance",
            "analysis.baseline_variance",
            json!({ "limit": 500 }),
        ),
        ("view.explain", "analysis.explain_date", json!({ "task": d })),
    ];
    for (name, tool, params) in views {
        out.insert(
            name.to_owned(),
            serde_json::to_vec(&call(ws, tool, &params)).unwrap(),
        );
    }
}

/// Plannings du test : nom et octets ; un fichier du corpus absent est
/// sauté, sauf sous `PLANNERCRAFT_CORPUS=required`.
fn plans() -> Vec<(String, Vec<u8>)> {
    let mut plans = vec![(
        "demo".to_owned(),
        std::fs::read(tests_path("../crates/plannercraft-ui-egui/assets/demo/house.xml")).unwrap(),
    )];
    let required = std::env::var("PLANNERCRAFT_CORPUS").as_deref() == Ok("required");
    for name in ["practicemap", "shopmetrics"] {
        match std::fs::read(tests_path(&format!("corpus/files/{name}.xml"))) {
            Ok(bytes) => plans.push((name.to_owned(), bytes)),
            Err(e) if required => panic!("corpus exigé, {name}.xml absent : {e}"),
            Err(_) => eprintln!("contract_bytes : {name}.xml absent, sauté"),
        }
    }
    plans
}

#[test]
fn contract_bytes_unchanged_for_whole_minutes() {
    let mut table = String::new();
    for (plan, bytes) in plans() {
        for (name, out) in artifacts(&bytes) {
            let _ = writeln!(table, "{plan}/{name} {}", sha256_hex(&out));
        }
    }
    // Fichiers de compatibilité (journal et sauvegarde de l'exemple),
    // écrits une fois depuis `main` dans `tests/fixtures/compat/`.
    if let Ok(dir) = std::env::var("PLANNERCRAFT_BYTES_DUMP") {
        let demo = artifacts(&plans()[0].1);
        for (name, file) in [
            ("journal", "demo-journal.jsonl"),
            ("autosave.base", "demo-autosave-base.json"),
            ("autosave.ops", "demo-autosave-ops.jsonl"),
        ] {
            std::fs::write(std::path::Path::new(&dir).join(file), &demo[name]).unwrap();
        }
    }
    if std::env::var("PLANNERCRAFT_BYTES_PRINT").as_deref() == Ok("1") {
        print!("{table}");
        return;
    }
    let reference = std::fs::read_to_string(tests_path("fixtures/contract_bytes.txt")).unwrap();
    let expected: BTreeMap<&str, &str> = reference
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .filter_map(|l| l.split_once(' '))
        .collect();
    let mut checked = 0;
    for line in table.lines() {
        let (key, hash) = line.split_once(' ').unwrap();
        let want = expected
            .get(key)
            .unwrap_or_else(|| panic!("{key} : empreinte de référence absente"));
        assert_eq!(
            hash, *want,
            "{key} : octets différents de main (planning en minutes entières)"
        );
        checked += 1;
    }
    assert!(checked >= 15, "{checked} contrats comparés");
}

/// Empreinte de référence (`main`, avant B2) d'un contrat.
fn reference(key: &str) -> String {
    let reference = std::fs::read_to_string(tests_path("fixtures/contract_bytes.txt")).unwrap();
    reference
        .lines()
        .find_map(|l| l.strip_prefix(key).and_then(|r| r.strip_prefix(' ')))
        .unwrap_or_else(|| panic!("{key} : empreinte absente"))
        .to_owned()
}

/// Compatibilité ascendante : la sauvegarde automatique écrite par `main`
/// (base et lignes) se relit avec le même sens : le projet relu est, octet
/// pour octet, le modèle de `main` après le scénario.
#[test]
fn autosave_written_by_main_reads_back_identically() {
    let base = std::fs::read(tests_path("fixtures/compat/demo-autosave-base.json")).unwrap();
    let ops = std::fs::read(tests_path("fixtures/compat/demo-autosave-ops.jsonl")).unwrap();
    assert_eq!(sha256_hex(&base), reference("demo/autosave.base"));
    let (session, report) =
        plannercraft_automation::recovery::replay_bytes(&base, &ops, Language::Fr).unwrap();
    assert_eq!(report.stopped_at, None, "rejeu complet : {:?}", report.error);
    let model = serde_json::to_vec(session.project()).unwrap();
    assert_eq!(sha256_hex(&model), reference("demo/model"));
}

/// Compatibilité ascendante : le journal de commandes écrit par `main` se
/// rejoue sur l'exemple et redonne, octet pour octet, le modèle et les
/// résultats de `main`.
#[test]
fn command_journal_written_by_main_replays_identically() {
    let journal = std::fs::read_to_string(tests_path("fixtures/compat/demo-journal.jsonl")).unwrap();
    assert_eq!(sha256_hex(journal.as_bytes()), reference("demo/journal"));
    let bytes = std::fs::read(tests_path("../crates/plannercraft-ui-egui/assets/demo/house.xml")).unwrap();
    let now = moment("2026-10-17T08:00");
    let origin = DocOrigin {
        name: "P".to_owned(),
        path: None,
    };
    let (session, _) = Session::open_bytes(&bytes, origin, now, Language::Fr).unwrap();
    let mut ws = Workspace::new(now, Language::Fr);
    ws.set_session(session);
    for line in journal.lines() {
        let entry = plannercraft_automation::journal::parse_line(line).unwrap();
        let result = call(&mut ws, &entry.tool, &entry.params);
        assert_eq!(
            Some(&result),
            entry.result.as_ref(),
            "{} : résultat différent",
            entry.tool
        );
    }
    let model = serde_json::to_vec(ws.session().unwrap().project()).unwrap();
    assert_eq!(sha256_hex(&model), reference("demo/model"));
}
