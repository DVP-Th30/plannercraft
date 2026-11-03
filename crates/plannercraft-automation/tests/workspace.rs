//! Lot L6 (ADR-0012) : fichiers (lecture bornée, écriture atomique,
//! confinement), journal, vues JSON et `Workspace`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use plannercraft_automation::error::codes;
use plannercraft_automation::files::{FileError, confine, read_path, write_atomic};
use plannercraft_automation::journal::{Entry, parse_line, to_line};
use serde_json::json;

/// Dossier temporaire propre à un test (retiré à la fin du test).
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("plannercraft-ws-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(unix)]
#[test]
fn files_write_atomic_follows_symlink() {
    // plan.xml -> sync/plan.xml : la cible est remplacée, le lien reste un
    // lien ; aucun temporaire laissé ; lien sans cible refusé.
    let dir = TempDir::new("symlink");
    let sync = dir.path().join("sync");
    std::fs::create_dir_all(&sync).unwrap();
    let target = sync.join("plan.xml");
    std::fs::write(&target, b"ancien").unwrap();
    let link = dir.path().join("plan.xml");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    write_atomic(&link, b"nouveau").unwrap();
    assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read(&target).unwrap(), b"nouveau");
    let names = |d: &std::path::Path| {
        let mut v: Vec<_> = std::fs::read_dir(d)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        v.sort();
        v
    };
    assert_eq!(names(&sync), vec![std::ffi::OsString::from("plan.xml")]);
    assert_eq!(
        names(dir.path()),
        vec![
            std::ffi::OsString::from("plan.xml"),
            std::ffi::OsString::from("sync")
        ]
    );
    let dangling = dir.path().join("orphelin.xml");
    std::os::unix::fs::symlink(sync.join("absent.xml"), &dangling).unwrap();
    let err = write_atomic(&dangling, b"x").unwrap_err();
    assert_eq!(err.code(), codes::FILE_WRITE);
    assert!(
        std::fs::symlink_metadata(&dangling)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn files_write_atomic_replaces() {
    let dir = TempDir::new("atomic");
    let p = dir.path().join("plan.xml");
    write_atomic(&p, b"ancien").unwrap();
    write_atomic(&p, b"nouveau contenu").unwrap();
    assert_eq!(std::fs::read(&p).unwrap(), b"nouveau contenu");
    // Aucun temporaire laissé dans le dossier.
    let names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, vec![std::ffi::OsString::from("plan.xml")]);
    // Dossier absent : erreur propre, rien de créé.
    let missing = dir.path().join("absent").join("plan.xml");
    let err = write_atomic(&missing, b"x").unwrap_err();
    assert_eq!(err.code(), codes::FILE_WRITE);
    assert!(!missing.exists());
    // Lecture bornée.
    assert_eq!(read_path(&p, 1_000).unwrap(), b"nouveau contenu");
    let err = read_path(&p, 3).unwrap_err();
    assert!(matches!(err, FileError::TooLarge { max: 3, .. }));
    assert_eq!(err.code(), codes::FILE_TOO_LARGE);
    assert_eq!(read_path(&missing, 10).unwrap_err().code(), codes::FILE_READ);
    assert_eq!(read_path(dir.path(), 10).unwrap_err().code(), codes::FILE_READ);
}

#[test]
fn files_confine_outside_root_refused() {
    let base = TempDir::new("confine");
    let root = base.path().join("root");
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::write(root.join("sub/a.xml"), b"a").unwrap();
    std::fs::write(base.path().join("secret.xml"), b"s").unwrap();
    let canon_root = std::fs::canonicalize(&root).unwrap();

    // Relatif, absolu, fichier à créer : dans la racine.
    assert_eq!(
        confine(&root, std::path::Path::new("sub/a.xml")).unwrap(),
        canon_root.join("sub/a.xml")
    );
    assert_eq!(
        confine(&root, &root.join("sub/a.xml")).unwrap(),
        canon_root.join("sub/a.xml")
    );
    assert_eq!(
        confine(&root, std::path::Path::new("new.xml")).unwrap(),
        canon_root.join("new.xml")
    );
    // Hors de la racine : « .. », absolu ailleurs, dossier absent, nom « .. ».
    for p in [
        PathBuf::from("../secret.xml"),
        PathBuf::from("sub/../../secret.xml"),
        base.path().join("secret.xml"),
        PathBuf::from("/etc/passwd"),
        PathBuf::from("absent/new.xml"),
        PathBuf::from(".."),
    ] {
        let err = confine(&root, &p).unwrap_err();
        assert_eq!(err.code(), codes::FILE_OUTSIDE_ROOT, "{}", p.display());
    }
    // Lien symbolique qui sort de la racine.
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(base.path().join("secret.xml"), root.join("lien.xml")).unwrap();
        assert_eq!(
            confine(&root, std::path::Path::new("lien.xml"))
                .unwrap_err()
                .code(),
            codes::FILE_OUTSIDE_ROOT
        );
    }
}

#[test]
fn journal_line_roundtrip() {
    let entries = [
        Entry {
            v: 1,
            seq: 0,
            tool: "add_task".to_owned(),
            params: json!({ "name": "Fondations\n« é »", "duration": { "text": "5j" } }),
            result: None,
        },
        Entry {
            v: 1,
            seq: 42,
            tool: "task.add".to_owned(),
            params: json!({}),
            result: Some(json!({ "committed": true, "created": [7] })),
        },
    ];
    for e in &entries {
        let line = to_line(e);
        assert!(!line.contains('\n'), "une entrée par ligne : {line}");
        assert_eq!(parse_line(&line).unwrap(), *e);
    }
    // `result` absent : non écrit.
    assert!(!to_line(&entries[0]).contains("result"));
    assert!(parse_line("{").is_err());
    assert!(parse_line(r#"{"v":1,"seq":1,"tool":"x","params":{},"extra":1}"#).is_err());
}

// --- Vues et espace de travail ---------------------------------------------

use plannercraft_automation::Workspace;
use plannercraft_automation::text::Language;
use plannercraft_core::{Moment, TaskUid};
use serde_json::Value;

fn now() -> Moment {
    "2026-10-19T08:00".parse().unwrap()
}

fn house_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../plannercraft-ui-egui/assets/demo/house.xml")
}

/// Espace avec `house.xml` ouvert.
fn house(lang: Language) -> Workspace {
    let mut ws = Workspace::new(now(), lang);
    let path = house_path();
    ws.call("project.open", json!({ "path": path.to_str().unwrap() }))
        .unwrap();
    ws
}

/// Espace avec un projet neuf.
fn fresh(lang: Language) -> Workspace {
    let mut ws = Workspace::new(now(), lang);
    ws.call(
        "project.new",
        json!({ "name": "Rénovation", "start": "2026-11-02T08:00" }),
    )
    .unwrap();
    ws
}

/// Toutes les pages de `task.list` (taille `limit`).
fn all_pages(ws: &mut Workspace, limit: u32, filter: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    let mut cursor: Option<u64> = None;
    for _ in 0..1_000 {
        let mut params = filter.clone();
        params["limit"] = json!(limit);
        if let Some(c) = cursor {
            params["cursor"] = json!(c);
        }
        let page = ws.call("task.list", params).unwrap();
        let tasks = page["tasks"].as_array().unwrap();
        assert!(tasks.len() <= limit as usize);
        out.extend(tasks.iter().cloned());
        match page["next_cursor"].as_u64() {
            Some(c) => cursor = Some(c),
            None => return out,
        }
    }
    panic!("pagination sans fin");
}

/// Référence d'une vue : `tests/golden/views/<name>.json`, régénérée par
/// `PLANNERCRAFT_BLESS=1`.
fn golden(name: &str, value: &Value) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/views")
        .join(format!("{name}.json"));
    let mut text = serde_json::to_string_pretty(value).unwrap();
    text.push('\n');
    if std::env::var_os("PLANNERCRAFT_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &text).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("référence absente : {} (PLANNERCRAFT_BLESS=1)", path.display()));
    assert_eq!(
        text, expected,
        "vue {name} différente de la référence (PLANNERCRAFT_BLESS=1 pour régénérer)"
    );
}

#[test]
fn workspace_no_session_error() {
    let mut ws = Workspace::new(now(), Language::En);
    for (id, params) in [
        ("project.summary", json!({})),
        ("task.list", json!({})),
        ("task.get", json!({ "uid": 1 })),
        ("analysis.critical_path", json!({})),
        ("render.gantt", json!({})),
        ("history.undo", json!({})),
        ("history.list", json!({})),
        ("task.add", json!({ "name": "A" })),
        ("batch.apply", json!({ "ops": [] })),
        ("project.save", json!({})),
        ("analysis.what_if", json!({ "ops": [] })),
        ("analysis.baseline_variance", json!({})),
        ("progress.update", json!({ "task": 1 })),
        ("baseline.set", json!({})),
    ] {
        let err = ws.call(id, params).unwrap_err();
        assert_eq!(err.code, codes::SESSION_NONE, "{id}");
        assert!(err.message.contains("open_project"), "{id} : {}", err.message);
    }
    // Id inconnu, paramètres mal formés (plus aucune commande réservée
    // depuis le lot L8 de J6).
    assert_eq!(
        ws.call("task.nope", json!({})).unwrap_err().code,
        codes::PARAM_INVALID
    );
    assert_eq!(
        ws.call("project.summary", json!({ "x": 1 })).unwrap_err().code,
        codes::PARAM_INVALID
    );
    assert!(ws.session().is_none());
}

/// C-MCP-01 : chaque date d'une `TaskView` est celle du moteur.
#[test]
fn view_task_dates_from_engine() {
    let mut ws = house(Language::Fr);
    let tasks = all_pages(&mut ws, 500, &json!({}));
    let session = ws.session().unwrap();
    let schedule = session.schedule().clone();
    let visible = session.project().tasks.iter().filter(|t| !t.is_null).count();
    assert_eq!(tasks.len(), visible);
    assert!(tasks.len() >= 20, "house.xml : {} tâches", tasks.len());
    for t in &tasks {
        let uid = TaskUid::new(u32::try_from(t["uid"].as_u64().unwrap()).unwrap());
        let s = schedule.scheduled(uid);
        assert_eq!(
            t["start"].as_str().map(str::to_owned),
            s.map(|s| s.start.to_string()),
            "début {uid}"
        );
        assert_eq!(
            t["finish"].as_str().map(str::to_owned),
            s.map(|s| s.finish.to_string()),
            "fin {uid}"
        );
        assert_eq!(t["critical"].as_bool().unwrap(), s.is_some_and(|s| s.critical));
        assert_eq!(
            t["total_slack"]["minutes"].as_i64(),
            s.map(|s| s.total_slack.get())
        );
        assert_eq!(t["free_slack"]["minutes"].as_i64(), s.map(|s| s.free_slack.get()));
        // `task.get` donne la même vue que la liste.
        let one = ws.call("task.get", json!({ "uid": uid.get() })).unwrap();
        assert_eq!(&one, t);
    }
    // Dates du résumé et du chemin critique : celles du moteur aussi.
    let session = ws.session().unwrap();
    let (start, finish) = (
        session.schedule().project_start.to_string(),
        session.schedule().project_finish.to_string(),
    );
    let summary = ws.call("project.summary", json!({})).unwrap();
    assert_eq!(summary["start"], json!(start));
    assert_eq!(summary["finish"], json!(finish));
    let cp = ws.call("analysis.critical_path", json!({})).unwrap();
    assert_eq!(cp["project_finish"], json!(finish));
    assert!(cp["total"].as_u64().unwrap() > 0);
    for t in cp["tasks"].as_array().unwrap() {
        assert_eq!(t["critical"], json!(true));
        assert_eq!(t["summary"], json!(false));
    }
    let err = ws.call("task.get", json!({ "uid": 999_999 })).unwrap_err();
    assert_eq!(err.code, codes::TASK_NOT_FOUND);
    assert_eq!(err.field.as_deref(), Some("uid"));
}

/// Pagination : les pages mises bout à bout redonnent la liste entière,
/// pour plusieurs tailles et filtres ; `total` est constant.
#[test]
fn list_pagination_cursor_stable() {
    let mut ws = house(Language::En);
    for filter in [
        json!({}),
        json!({ "critical": true }),
        json!({ "summary": false }),
        json!({ "summary": true }),
        json!({ "outline_max": 1 }),
        json!({ "from": "2026-11-16", "to": "2026-11-27" }),
    ] {
        let full = ws.call("task.list", {
            let mut f = filter.clone();
            f["limit"] = json!(500);
            f
        });
        let full = full.unwrap();
        let expected = full["tasks"].as_array().unwrap().clone();
        assert_eq!(
            usize::try_from(full["total"].as_u64().unwrap()).unwrap(),
            expected.len()
        );
        assert!(full["next_cursor"].is_null());
        for limit in [1, 3, 7, 50] {
            assert_eq!(all_pages(&mut ws, limit, &filter), expected, "{filter} / {limit}");
        }
        // Filtres : chaque tâche retenue y répond.
        for t in &expected {
            if filter.get("critical").is_some() {
                assert_eq!(t["critical"], json!(true));
            }
            if let Some(b) = filter.get("summary") {
                assert_eq!(&t["summary"], b);
            }
            if filter.get("outline_max").is_some() {
                assert_eq!(t["level"], json!(1));
            }
            if filter.get("from").is_some() {
                assert!(t["finish"].as_str().unwrap() >= "2026-11-16");
                assert!(t["start"].as_str().unwrap() < "2026-11-28");
            }
        }
    }
    // Défaut : 50 tâches par page.
    let page = ws.call("task.list", json!({})).unwrap();
    let total = usize::try_from(page["total"].as_u64().unwrap()).unwrap();
    assert_eq!(page["tasks"].as_array().unwrap().len(), total.min(50));
    // Curseur inconnu : refusé.
    let err = ws.call("task.list", json!({ "cursor": 999_999 })).unwrap_err();
    assert_eq!(err.code, codes::TASK_NOT_FOUND);
    assert_eq!(err.field.as_deref(), Some("cursor"));
    // Ressource inconnue : refusée (J5, lot L8).
    let err = ws.call("task.list", json!({ "resource": 999_999 })).unwrap_err();
    assert_eq!(err.code, codes::RESOURCE_NOT_FOUND);
    assert_eq!(err.field.as_deref(), Some("resource"));
}

#[test]
fn list_limit_over_500_refused() {
    let mut ws = house(Language::Fr);
    assert!(ws.call("task.list", json!({ "limit": 500 })).is_ok());
    let err = ws.call("task.list", json!({ "limit": 501 })).unwrap_err();
    assert_eq!(err.code, codes::LIMIT_EXCEEDED);
    assert_eq!(err.field.as_deref(), Some("limit"));
    assert!(err.message.contains("500"), "{}", err.message);
    for bad in [
        json!({ "limit": 0 }),
        json!({ "outline_max": 0 }),
        json!({ "from": "2026-12-01", "to": "2026-11-01" }),
    ] {
        assert_eq!(ws.call("task.list", bad).unwrap_err().code, codes::PARAM_INVALID);
    }
    // Rendu : 200 lignes au plus.
    let err = ws.call("render.gantt", json!({ "max_rows": 201 })).unwrap_err();
    assert_eq!(err.code, codes::LIMIT_EXCEEDED);
}

#[test]
fn view_golden_summary_house() {
    let mut ws = house(Language::Fr);
    let mut summary = ws.call("project.summary", json!({})).unwrap();
    assert_eq!(summary["dirty"], json!(false));
    assert_eq!(summary["history_len"], json!(0));
    // Chemin absolu de la machine : retiré de la référence.
    assert!(summary["path"].as_str().unwrap().ends_with("house.xml"));
    summary["path"] = Value::Null;
    golden("summary_house", &summary);
    let mut ws = house(Language::En);
    let mut summary_en = ws.call("project.summary", json!({})).unwrap();
    summary_en["path"] = Value::Null;
    golden("summary_house_en", &summary_en);
}

#[test]
fn view_golden_outcome_add_task() {
    let mut ws = fresh(Language::Fr);
    let out = ws
        .call(
            "task.add",
            json!({ "name": "Fondations", "duration": { "text": "5j" } }),
        )
        .unwrap();
    assert_eq!(out["committed"], json!(true));
    assert_eq!(out["history_len"], json!(1));
    golden("outcome_add_task", &out);
    // Lot de deux tâches liées (étiquettes), puis le même en `dry_run`.
    let batch = json!({
        "ops": [
            { "op": "task.add", "name": "Murs", "as": "$murs", "duration": { "text": "10j" } },
            { "op": "link.add", "pred": 1, "succ": "$murs" }
        ]
    });
    let mut dry = batch.clone();
    dry["dry_run"] = json!(true);
    let simulated = ws.call("batch.apply", dry).unwrap();
    assert_eq!(simulated["committed"], json!(false));
    assert_eq!(simulated["created"], json!([]));
    assert_eq!(simulated["history_len"], json!(1));
    assert_eq!(ws.session().unwrap().history().entries.len(), 1);
    let applied = ws.call("batch.apply", batch).unwrap();
    assert_eq!(applied["committed"], json!(true));
    assert_eq!(applied["labels"], simulated["labels"]);
    assert_eq!(applied["diff"], simulated["diff"]);
    assert_eq!(applied["created_uids"], simulated["created_uids"]);
    golden("outcome_batch_link", &applied);
}

/// Écritures unitaires : `dry_run` ne crée pas d'entrée (C-MCP-04), un
/// refus garde l'état et rend une erreur structurée ; annuler, rétablir.
#[test]
fn workspace_write_dry_run_undo_redo() {
    let mut ws = fresh(Language::En);
    let out = ws
        .call(
            "task.add",
            json!({ "name": "A", "dry_run": true, "detail": true }),
        )
        .unwrap();
    assert_eq!(out["committed"], json!(false));
    assert_eq!(out["created_uids"], json!([1]));
    assert_eq!(ws.session().unwrap().project().tasks.len(), 0);
    ws.call("task.add", json!({ "name": "A" })).unwrap();
    ws.call("task.add", json!({ "name": "B" })).unwrap();
    ws.call("link.add", json!({ "pred": 1, "succ": 2 })).unwrap();
    assert_eq!(ws.session().unwrap().history().entries.len(), 3);
    // Cycle : refusé par R-160, état intact, diagnostics joints.
    let before = ws.session().unwrap().project().clone();
    let err = ws.call("link.add", json!({ "pred": 2, "succ": 1 })).unwrap_err();
    assert_eq!(err.code, codes::BATCH_WOULD_CREATE_ERRORS);
    assert_eq!(err.op_index, Some(0));
    assert_ne!(err.diagnostics.len(), 0);
    assert!(err.message.chars().count() <= 2_000);
    assert_eq!(ws.session().unwrap().project(), &before);
    // Paramètres faux.
    for bad in [
        json!({ "name": "A", "dry_run": "oui" }),
        json!({ "name": "A", "op": "task.delete" }),
        json!({ "name": "A", "x": 1 }),
        json!([1]),
    ] {
        assert_eq!(ws.call("task.add", bad).unwrap_err().code, codes::PARAM_INVALID);
    }
    let undone = ws.call("history.undo", json!({})).unwrap();
    assert_eq!(undone["history_len"], json!(2));
    let redone = ws.call("history.redo", json!({})).unwrap();
    assert_eq!(redone["history_len"], json!(3));
    assert_eq!(ws.session().unwrap().project(), &before);
    assert_eq!(
        ws.call("history.redo", json!({})).unwrap_err().code,
        codes::HISTORY_EMPTY
    );
    let history = ws.call("history.list", json!({})).unwrap();
    assert_eq!(history["entries"].as_array().unwrap().len(), 3);
    // Projet neuf : une date exactement.
    for bad in [
        json!({ "name": "X" }),
        json!({ "name": "X", "start": "2026-11-02T08:00", "finish": "2026-12-02T17:00" }),
    ] {
        assert_eq!(
            ws.call("project.new", bad).unwrap_err().code,
            codes::PARAM_INVALID
        );
    }
}

#[test]
fn workspace_render_gantt_bounded() {
    let mut ws = house(Language::Fr);
    let out = ws.call("render.gantt", json!({ "max_rows": 5 })).unwrap();
    let svg = out["svg"].as_str().unwrap();
    assert!(svg.starts_with("<svg"));
    assert_eq!(
        usize::try_from(out["bytes"].as_u64().unwrap()).unwrap(),
        svg.len()
    );
    assert!(out["shown"].as_u64().unwrap() <= 5);
    assert!(out["hidden"].as_u64().unwrap() > 0);
    let again = ws.call("render.gantt", json!({ "max_rows": 5 })).unwrap();
    assert_eq!(again, out, "rendu déterministe");
    let ranged = ws
        .call(
            "render.gantt",
            json!({ "from": "2026-11-02", "zoom": "day", "critical_only": true }),
        )
        .unwrap();
    assert!(ranged["svg"].as_str().unwrap().starts_with("<svg"));
    let err = ws
        .call(
            "render.gantt",
            json!({ "from": "2027-01-10", "to": "2026-11-02" }),
        )
        .unwrap_err();
    assert_eq!(err.code, codes::PARAM_INVALID);
}

/// Critère de fin L6 : `house.xml` ouvert, modifié, enregistré, relu :
/// projets égaux par `normalize` (R-133) de l'image du moteur (R-215).
#[test]
fn workspace_open_save_reopen_equal() {
    let dir = TempDir::new("save");
    let copy = dir.path().join("maison.xml");
    std::fs::copy(house_path(), &copy).unwrap();
    let mut ws = Workspace::new(now(), Language::Fr);
    let opened = ws
        .call("project.open", json!({ "path": copy.to_str().unwrap() }))
        .unwrap();
    assert_eq!(opened["summary"]["dirty"], json!(false));
    ws.call(
        "batch.apply",
        json!({
            "lang": "fr",
            "ops": [
                { "op": "task.add", "name": "Réception", "as": "$r", "milestone": true, "duration": { "minutes": 0 } },
                { "op": "link.add", "pred": 2, "succ": "$r", "lag": { "text": "+2j" } },
                { "op": "task.update", "uid": 2, "notes": "relu" }
            ]
        }),
    )
    .unwrap();
    assert_eq!(
        ws.call("project.summary", json!({})).unwrap()["dirty"],
        json!(true)
    );
    let saved = ws.call("project.save", json!({})).unwrap();
    assert_eq!(saved["dirty"], json!(false));
    assert!(saved["bytes"].as_u64().unwrap() > 0);
    let session = ws.session().unwrap();
    let options = plannercraft_mspdi::write::WriteOptions::default();
    // Image écrite par le moteur (R-215 : restes et travail réel des
    // récapitulatives calculés, relecture finale J6, J6-F2).
    let expected = plannercraft_mspdi::normalize(
        &plannercraft_mspdi::write::schedule_image(session.project(), session.schedule()),
        &options,
    );

    let mut again = Workspace::new(now(), Language::Fr);
    again
        .call("project.open", json!({ "path": copy.to_str().unwrap() }))
        .unwrap();
    let reopened = again.session().unwrap().project();
    assert_eq!(plannercraft_mspdi::normalize(reopened, &options), expected);
    // Même planning relu.
    assert_eq!(
        again.call("task.list", json!({ "limit": 500 })).unwrap()["tasks"],
        ws.call("task.list", json!({ "limit": 500 })).unwrap()["tasks"]
    );

    // Enregistrer sous : suit le nouveau chemin ; exporter : copie seule.
    let other = dir.path().join("autre.xml");
    ws.call("task.add", json!({ "name": "Z" })).unwrap();
    ws.call(
        "project.export_mspdi",
        json!({ "path": dir.path().join("copie.xml").to_str().unwrap() }),
    )
    .unwrap();
    assert_eq!(
        ws.call("project.summary", json!({})).unwrap()["dirty"],
        json!(true)
    );
    ws.call("project.save", json!({ "path": other.to_str().unwrap() }))
        .unwrap();
    let summary = ws.call("project.summary", json!({})).unwrap();
    assert_eq!(summary["dirty"], json!(false));
    assert_eq!(summary["path"], json!(other.to_str().unwrap()));
    // Projet neuf sans fichier : chemin exigé.
    let mut neuf = fresh(Language::Fr);
    let err = neuf.call("project.save", json!({})).unwrap_err();
    assert_eq!(err.code, codes::PARAM_INVALID);
    assert_eq!(err.field.as_deref(), Some("path"));
    // Fichiers absents ou illisibles.
    let err = neuf
        .call(
            "project.open",
            json!({ "path": dir.path().join("absent.xml").to_str().unwrap() }),
        )
        .unwrap_err();
    assert_eq!(err.code, codes::FILE_READ);
    std::fs::write(dir.path().join("faux.xml"), b"pas du xml").unwrap();
    let err = neuf
        .call(
            "project.open",
            json!({ "path": dir.path().join("faux.xml").to_str().unwrap() }),
        )
        .unwrap_err();
    assert_eq!(err.code, codes::FILE_INVALID);
    // La session précédente reste ouverte après un échec.
    assert!(neuf.session().is_some());
}

/// Racine (MCP HTTP) : chemins confinés, `..` refusé.
#[test]
fn workspace_root_confines_paths() {
    let dir = TempDir::new("root");
    std::fs::copy(house_path(), dir.path().join("maison.xml")).unwrap();
    let mut ws = Workspace::new(now(), Language::En).with_root(dir.path().to_path_buf());
    ws.call("project.open", json!({ "path": "maison.xml" })).unwrap();
    ws.call("project.save", json!({ "path": "copie.xml" })).unwrap();
    assert!(dir.path().join("copie.xml").exists());
    let outside = house_path();
    for p in ["../x.xml", outside.to_str().unwrap()] {
        let err = ws.call("project.open", json!({ "path": p })).unwrap_err();
        assert_eq!(err.code, codes::FILE_OUTSIDE_ROOT, "{p}");
        let err = ws.call("project.export_mspdi", json!({ "path": p })).unwrap_err();
        assert_eq!(err.code, codes::FILE_OUTSIDE_ROOT, "{p}");
    }
}

#[test]
fn workspace_import_csv_roundtrip() {
    let dir = TempDir::new("csv");
    let csv = dir.path().join("taches.csv");
    std::fs::write(
        &csv,
        "Nom;Durée;Prédécesseurs\nFondations;5j;\nMurs;10j;1\nToit;3j;2\n",
    )
    .unwrap();
    let mut ws = Workspace::new(now(), Language::Fr);
    let out = ws
        .call(
            "project.import_csv",
            json!({ "path": csv.to_str().unwrap(), "lang": "fr" }),
        )
        .unwrap();
    assert_eq!(out["summary"]["tasks"], json!(3));
    assert_eq!(out["summary"]["links"], json!(2));
    assert_eq!(out["summary"]["path"], Value::Null);
    let list = ws.call("task.list", json!({})).unwrap();
    assert_eq!(
        list["tasks"][2]["predecessors"][0]["pred"],
        list["tasks"][1]["uid"]
    );
    let saved = dir.path().join("taches.xml");
    ws.call("project.save", json!({ "path": saved.to_str().unwrap() }))
        .unwrap();
    let mut again = Workspace::new(now(), Language::Fr);
    again
        .call("project.open", json!({ "path": saved.to_str().unwrap() }))
        .unwrap();
    assert_eq!(
        again.call("task.list", json!({})).unwrap()["tasks"],
        list["tasks"]
    );
}

#[test]
fn workspace_batch_version_checked_before_ops() {
    // ADR-0012 L3 (4) : un lot d'un format futur est refusé comme tel, même
    // si ses opérations sont mal formées pour cette version
    // (`progress.update` est connue depuis le contrat J6, mais sans champ
    // `uid`).
    let mut ws = fresh(Language::Fr);
    let err = ws
        .call(
            "batch.apply",
            json!({ "format_version": 2, "ops": [{ "op": "progress.update", "uid": 1 }] }),
        )
        .unwrap_err();
    assert_eq!(err.code, codes::BATCH_UNSUPPORTED_VERSION);
    assert_eq!(err.field.as_deref(), Some("format_version"));
    // Version 1 ou absente : l'opération inconnue est un défaut de forme.
    for batch in [
        json!({ "format_version": 1, "ops": [{ "op": "progress.update", "uid": 1 }] }),
        json!({ "ops": [{ "op": "progress.update", "uid": 1 }] }),
    ] {
        let err = ws.call("batch.apply", batch).unwrap_err();
        assert_eq!(err.code, codes::PARAM_INVALID);
    }
    // Version non entière : `param.invalid`.
    let err = ws
        .call("batch.apply", json!({ "format_version": "2", "ops": [] }))
        .unwrap_err();
    assert_eq!(err.code, codes::PARAM_INVALID);
}
