//! Reprise après plantage (J7, lot L15) : propriété P48 (rejeu d'un
//! journal sur des suites tirées de lots, d'annulations, de rétablissements
//! et d'enregistrements) et contrats du journal sur le disque (orphelin,
//! arrêt sur une entrée en échec, point de reprise).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::needless_pass_by_value,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use plannercraft_automation::recovery::{
    CHECKPOINT_ENTRIES, Entry, Journal, Store, orphans, replay, replay_bytes,
};
use plannercraft_automation::text::Language;
use plannercraft_automation::{ApplyMode, Batch, DocOrigin, EditSource, Session};
use plannercraft_core::{
    BuiltinCalendarNames, Duration, DurationUnit, Link, Minutes, Moment, Project, ProjectInit,
    ScheduleAnchor, Task, TaskUid, TimeBasis,
};
use proptest::prelude::*;
use serde_json::{Value, json};

fn moment(s: &str) -> Moment {
    s.parse().unwrap()
}

fn now() -> Moment {
    moment("2026-10-21T09:00")
}

fn days(n: i64) -> Duration {
    Duration::new(
        Minutes::new(n * 480),
        DurationUnit::Day,
        TimeBasis::Working,
        false,
    )
    .unwrap()
}

fn session(p: Project) -> Session {
    let origin = DocOrigin {
        name: "plan.xml".to_owned(),
        path: Some("/projets/plan.xml".to_owned()),
    };
    Session::from_project(p, origin, moment("2026-10-17T08:00"), Language::Fr)
}

/// Projet de `n` tâches d'un jour enchaînées.
fn chain(n: u32) -> Project {
    let mut p = Project::new(ProjectInit {
        name: "P".to_owned(),
        anchor: ScheduleAnchor::FromStart(moment("2026-11-02T08:00")),
        current_date: moment("2026-10-17T08:00"),
        calendar_names: BuiltinCalendarNames::default(),
    });
    for uid in 1..=n {
        let mut t = Task::new(TaskUid::new(uid), format!("T{uid}"), &p.options);
        t.duration = days(1);
        if uid > 1 {
            t.predecessors.push(Link::finish_to_start(TaskUid::new(uid - 1)));
        }
        p.tasks.push(t).unwrap();
    }
    p
}

fn batch(ops: Value) -> Batch {
    Batch::from_json(json!({ "format_version": 1, "ops": ops })).unwrap()
}

/// Fichiers d'un journal en mémoire, partagés avec le test.
#[derive(Debug, Default)]
struct MemFiles {
    base: Vec<u8>,
    ops: Vec<u8>,
    removed: bool,
}

/// Écrivain en mémoire.
struct MemStore(Arc<Mutex<MemFiles>>);

impl Store for MemStore {
    fn replace_base(&mut self, bytes: &[u8]) -> io::Result<()> {
        bytes.clone_into(&mut self.0.lock().unwrap().base);
        Ok(())
    }

    fn append(&mut self, line: &[u8]) -> io::Result<()> {
        self.0.lock().unwrap().ops.extend_from_slice(line);
        Ok(())
    }

    fn truncate_ops(&mut self) -> io::Result<()> {
        self.0.lock().unwrap().ops.clear();
        Ok(())
    }

    fn remove(self: Box<Self>) -> io::Result<()> {
        self.0.lock().unwrap().removed = true;
        Ok(())
    }
}

fn mem_journal(s: &Session) -> (Journal, Arc<Mutex<MemFiles>>) {
    let files = Arc::new(Mutex::new(MemFiles::default()));
    let journal = Journal::start(Box::new(MemStore(Arc::clone(&files))), s).unwrap();
    (journal, files)
}

fn replay_mem(files: &Arc<Mutex<MemFiles>>) -> Session {
    let files = files.lock().unwrap();
    let (s, report) = replay_bytes(&files.base, &files.ops, Language::Fr).unwrap();
    assert_eq!(report.stopped_at, None, "{report:?}");
    s
}

/// Dossier temporaire propre au test, supprimé à la fin.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("pr-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// --- P48 ---------------------------------------------------------------

/// Opération tirée d'un lot ; les index désignent une tâche du projet
/// courant (modulo leur nombre).
#[derive(Clone, Debug)]
enum Op {
    Add(Option<i64>),
    Rename(usize),
    SetDays(usize, i64),
    Delete(usize),
    Indent(usize),
    Outdent(usize),
    Link(usize, usize),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        proptest::option::of(0i64..4).prop_map(Op::Add),
        any::<usize>().prop_map(Op::Rename),
        (any::<usize>(), 0i64..4).prop_map(|(i, d)| Op::SetDays(i, d)),
        any::<usize>().prop_map(Op::Delete),
        any::<usize>().prop_map(Op::Indent),
        any::<usize>().prop_map(Op::Outdent),
        (any::<usize>(), any::<usize>()).prop_map(|(i, j)| Op::Link(i, j)),
    ]
}

/// Pas d'une suite.
#[derive(Clone, Debug)]
enum Step {
    Batch(Vec<Op>),
    Undo,
    Redo,
    Save,
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        4 => proptest::collection::vec(op(), 1..4).prop_map(Step::Batch),
        2 => Just(Step::Undo),
        1 => Just(Step::Redo),
        1 => Just(Step::Save),
    ]
}

fn to_batch(s: &Session, ops: &[Op]) -> Batch {
    let uids: Vec<u32> = s.project().tasks.uids().map(TaskUid::get).collect();
    let t = |i: usize| if uids.is_empty() { 1 } else { uids[i % uids.len()] };
    let ops: Vec<Value> = ops
        .iter()
        .map(|op| match op {
            Op::Add(d) => {
                let mut v = json!({ "op": "task.add", "name": "N", "at": "end" });
                if let Some(d) = d {
                    v["duration"] = json!({ "minutes": d * 480, "unit": "day" });
                }
                v
            }
            Op::Rename(i) => json!({ "op": "task.update", "uid": t(*i), "name": format!("R{i}") }),
            Op::SetDays(i, d) => {
                json!({ "op": "task.update", "uid": t(*i), "duration": { "minutes": d * 480 } })
            }
            Op::Delete(i) => json!({ "op": "task.delete", "uid": t(*i), "cascade": true }),
            Op::Indent(i) => json!({ "op": "task.indent", "uid": t(*i) }),
            Op::Outdent(i) => json!({ "op": "task.outdent", "uid": t(*i) }),
            Op::Link(i, j) => json!({ "op": "link.add", "pred": t(*i), "succ": t(*j) }),
        })
        .collect();
    batch(Value::Array(ops))
}

fn run_step(s: &mut Session, step: &Step) {
    match step {
        Step::Batch(ops) => {
            let b = to_batch(s, ops);
            let _ = s.apply(&b, ApplyMode::Commit);
        }
        Step::Undo => drop(s.undo()),
        Step::Redo => drop(s.redo()),
        Step::Save => s.mark_saved(),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// P48 : `replay(base JSON, ops)` redonne le projet, le planning, l'état
    /// modifié, les compteurs d'UID et l'historique (annuler, rétablir) ;
    /// sans point de reprise, l'historique est identique entrée par entrée.
    #[test]
    fn p48_replay_restores_session(
        n in 0u32..6,
        steps in proptest::collection::vec(step(), 0..14),
    ) {
        let mut live = session(chain(n));
        let (mut journal, files) = mem_journal(&live);
        for step in &steps {
            run_step(&mut live, step);
            journal.record(&live, now()).unwrap();
        }
        let mut back = replay_mem(&files);
        prop_assert_eq!(back.project(), live.project());
        prop_assert_eq!(back.schedule(), live.schedule());
        prop_assert_eq!(back.is_dirty(), live.is_dirty());
        prop_assert_eq!(back.uid_counters(), live.uid_counters());
        prop_assert_eq!(back.origin(), live.origin());
        let history = back.history();
        if journal.epoch() == 0 {
            prop_assert_eq!(&history, &live.history());
        } else {
            prop_assert!(history.entries.len() <= live.history().entries.len());
        }
        // Annuler tout ce que le rejeu sait annuler, puis le rétablir : mêmes
        // états des deux côtés.
        let k = history.entries.len();
        for _ in 0..k {
            back.undo().unwrap();
            live.undo().unwrap();
            prop_assert_eq!(back.project(), live.project());
            prop_assert_eq!(back.is_dirty(), live.is_dirty());
        }
        for _ in 0..back.history().redo.len() {
            back.redo().unwrap();
            live.redo().unwrap();
            prop_assert_eq!(back.project(), live.project());
            prop_assert_eq!(back.is_dirty(), live.is_dirty());
        }
        // Mêmes UID alloués ensuite.
        let add = batch(json!([{ "op": "task.add", "name": "Après", "at": "end" }]));
        let a = back.apply(&add, ApplyMode::Commit).map(|o| o.created);
        let b = live.apply(&add, ApplyMode::Commit).map(|o| o.created);
        prop_assert_eq!(a, b);
        prop_assert_eq!(back.project(), live.project());
    }
}

/// Une entrée sérialisée se relit à l'identique (format des lignes).
#[test]
fn recovery_entry_roundtrip() {
    let entries = [
        Entry::Apply {
            batch: batch(json!([{ "op": "task.add", "name": "A", "at": "end", "as": "$a" }])),
            now: now(),
            source: EditSource::Agent,
        },
        Entry::Undo,
        Entry::Redo,
    ];
    for e in entries {
        let text = serde_json::to_string(&e).unwrap();
        let back: Entry = serde_json::from_str(&text).unwrap();
        assert_eq!(back, e, "{text}");
    }
}

/// Un lot accepté d'un agent garde son origine au rejeu.
#[test]
fn recovery_replay_keeps_agent_source() {
    let mut live = session(chain(2));
    let (mut journal, files) = mem_journal(&live);
    let b = batch(json!([{ "op": "task.update", "uid": 1, "name": "Agent" }]));
    live.apply_with(
        &b,
        ApplyMode::Commit,
        plannercraft_automation::ApplyOptions {
            detail: false,
            source: EditSource::Agent,
        },
    )
    .unwrap();
    journal.record(&live, now()).unwrap();
    let back = replay_mem(&files);
    assert_eq!(back.history(), live.history());
    assert_eq!(back.history().entries[0].source, EditSource::Agent);
}

// --- Disque --------------------------------------------------------------

/// Verrou tenu : pas d'orphelin ; verrou relâché (journal abandonné sans
/// fermeture, comme après un plantage) : un orphelin, avec le nom et le
/// nombre de modifications ; fermeture propre : plus rien.
#[test]
fn recovery_orphan_detected_after_lock_released() {
    let tmp = TempDir::new("orphan");
    let mut live = session(chain(3));
    let mut journal = Journal::create(tmp.path(), &live).unwrap();
    for name in ["A", "B"] {
        let b = batch(json!([{ "op": "task.update", "uid": 1, "name": name }]));
        live.apply(&b, ApplyMode::Commit).unwrap();
        journal.record(&live, now()).unwrap();
    }
    assert_eq!(orphans(tmp.path()), Vec::new(), "journal vivant vu orphelin");
    drop(journal);
    let found = orphans(tmp.path());
    assert_eq!(found.len(), 1, "{found:?}");
    let o = &found[0];
    assert_eq!(o.name, "plan.xml");
    assert_eq!(o.path.as_deref(), Some("/projets/plan.xml"));
    assert_eq!(o.entries, 2);
    assert!(o.saved && !o.checkpointed && !o.unreadable);
    assert!(o.worth_restoring());
    let (back, report) = replay(o, Language::Fr).unwrap();
    assert_eq!(report.applied, 2);
    assert_eq!(back.project(), live.project());
    assert_eq!(back.history(), live.history());
    assert!(back.is_dirty());
    o.discard().unwrap();
    assert_eq!(orphans(tmp.path()), Vec::new());

    // Fermeture propre : le dossier disparaît.
    let journal = Journal::create(tmp.path(), &live).unwrap();
    journal.close().unwrap();
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 0);
}

/// Relecture finale J7, interface F3 : la base garde l'empreinte du fichier
/// du document ; un fichier réécrit depuis (autre instance, MCP headless)
/// marque l'orphelin `file_changed`.
#[test]
fn recovery_file_changed_since_base_detected() {
    let tmp = TempDir::new("stamp");
    let root = tmp.path().join("journaux");
    let file = tmp.path().join("plan.xml");
    std::fs::write(&file, b"<Project/>").unwrap();
    let mut live = session(chain(2));
    live.set_origin(DocOrigin {
        name: "plan.xml".to_owned(),
        path: Some(file.to_str().unwrap().to_owned()),
    });
    let mut journal = Journal::create(&root, &live).unwrap();
    let b = batch(json!([{ "op": "task.update", "uid": 1, "name": "A" }]));
    live.apply(&b, ApplyMode::Commit).unwrap();
    journal.record(&live, now()).unwrap();
    drop(journal);
    let found = orphans(&root);
    assert_eq!(found.len(), 1);
    assert!(!found[0].file_changed, "fichier inchangé");
    // Le fichier est réécrit pendant que l'application est fermée.
    std::fs::write(&file, b"<Project><Name>plus recent</Name></Project>").unwrap();
    let found = orphans(&root);
    assert!(found[0].file_changed, "fichier réécrit non vu");
    assert!(found[0].worth_restoring());
}

/// Relecture finale J7, relais F5 : base d'une autre version de format ou
/// plus grande que `MAX_FILE_BYTES` ⇒ orphelin gardé (`kept`, proposé,
/// jamais supprimé d'office) ; JSON invalide ⇒ illisible, sans valeur.
#[test]
fn recovery_foreign_or_large_base_kept() {
    use plannercraft_automation::recovery::MAX_FILE_BYTES;

    let tmp = TempDir::new("foreign");
    let base = |name: &str| {
        let dir = tmp.path().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("base.json")
    };
    std::fs::write(base("1-0"), br#"{"v":2,"format":"futur"}"#).unwrap();
    let large = std::fs::File::create(base("1-1")).unwrap();
    large.set_len(MAX_FILE_BYTES + 1).unwrap();
    drop(large);
    std::fs::write(base("1-2"), b"{ pas du json").unwrap();
    let found = orphans(tmp.path());
    let kept: Vec<(&str, bool, bool)> = found
        .iter()
        .map(|o| (o.id.as_str(), o.kept, o.worth_offering()))
        .collect();
    assert_eq!(
        kept,
        [("1-0", true, true), ("1-1", true, true), ("1-2", false, false)]
    );
    assert!(found.iter().all(|o| o.unreadable && !o.worth_restoring()));
}

/// Orphelin sans modification ni base modifiée : sans valeur.
#[test]
fn recovery_clean_orphan_not_worth_restoring() {
    let tmp = TempDir::new("clean");
    let live = session(chain(1));
    drop(Journal::create(tmp.path(), &live).unwrap());
    let found = orphans(tmp.path());
    assert_eq!(found.len(), 1);
    assert!(!found[0].worth_restoring());
}

/// Une entrée refusée (tâche inconnue) arrête le rejeu juste avant elle ;
/// une ligne tronquée (écriture interrompue) aussi.
#[test]
fn recovery_replay_stops_on_bad_entry() {
    let tmp = TempDir::new("bad");
    let mut live = session(chain(3));
    let mut journal = Journal::create(tmp.path(), &live).unwrap();
    for name in ["A", "B", "C"] {
        let b = batch(json!([{ "op": "task.update", "uid": 2, "name": name }]));
        live.apply(&b, ApplyMode::Commit).unwrap();
        journal.record(&live, now()).unwrap();
    }
    let expected = live.project().clone();
    let bad = Entry::Apply {
        batch: batch(json!([{ "op": "task.update", "uid": 999, "name": "X" }])),
        now: now(),
        source: EditSource::User,
    };
    journal.append(&bad).unwrap();
    journal.append(&Entry::Undo).unwrap();
    drop(journal);
    let o = orphans(tmp.path()).remove(0);
    assert_eq!(o.entries, 5);
    let (back, report) = replay(&o, Language::Fr).unwrap();
    assert_eq!(report.applied, 3);
    assert_eq!(report.stopped_at, Some(3));
    assert!(
        report.error.as_deref().is_some_and(|e| e.contains("999")),
        "{report:?}"
    );
    assert_eq!(back.project(), &expected);

    // Ligne tronquée en fin de journal.
    let ops = o.dir.join("ops.jsonl");
    let text = std::fs::read_to_string(&ops).unwrap();
    let mut lines: Vec<&str> = text.lines().take(2).collect();
    let torn = "{\"epoch\":0,\"entry\":{\"kind\":\"app";
    lines.push(torn);
    std::fs::write(&ops, lines.join("\n")).unwrap();
    let (back, report) = replay(&o, Language::Fr).unwrap();
    assert_eq!((report.applied, report.stopped_at), (2, Some(2)));
    assert_eq!(back.project().tasks.get(TaskUid::new(2)).unwrap().name, "B");
}

/// Point de reprise : tous les 500 changements, et à l'état enregistré ;
/// la base est réécrite (époque suivante), le journal vidé ; le rejeu ignore
/// une ligne d'une époque précédente (plantage entre le renommage et la
/// troncature).
#[test]
fn recovery_checkpoint_truncates() {
    let mut live = session(chain(2));
    let (mut journal, files) = mem_journal(&live);
    for i in 0..CHECKPOINT_ENTRIES {
        let b = batch(json!([{ "op": "task.update", "uid": 1, "name": format!("N{i}") }]));
        live.apply(&b, ApplyMode::Commit).unwrap();
        journal.record(&live, now()).unwrap();
        if i + 1 < CHECKPOINT_ENTRIES {
            assert_eq!(journal.entries(), i + 1);
        }
    }
    assert_eq!(journal.epoch(), 1);
    assert_eq!(journal.entries(), 0);
    assert_eq!(files.lock().unwrap().ops.len(), 0);
    let back = replay_mem(&files);
    assert_eq!(back.project(), live.project());
    assert!(back.is_dirty(), "point de reprise d'un état non enregistré");
    assert_eq!(back.history().entries.len(), 0);

    // Une modification puis l'enregistrement : nouveau point de reprise,
    // base enregistrée.
    let b = batch(json!([{ "op": "task.update", "uid": 2, "name": "Z" }]));
    live.apply(&b, ApplyMode::Commit).unwrap();
    journal.record(&live, now()).unwrap();
    assert_eq!(journal.entries(), 1);
    let stale = files.lock().unwrap().ops.clone();
    live.mark_saved();
    journal.record(&live, now()).unwrap();
    assert_eq!((journal.epoch(), journal.entries()), (2, 0));
    let base = files.lock().unwrap().base.clone();
    let (back, report) = replay_bytes(&base, &stale, Language::Fr).unwrap();
    assert_eq!(report.applied, 0, "ligne d'une époque précédente rejouée");
    assert_eq!(back.project(), live.project());
    assert!(!back.is_dirty());

    // Annuler au-delà de la base : point de reprise (le rejeu ne saurait
    // pas annuler ce lot).
    live.undo().unwrap();
    journal.record(&live, now()).unwrap();
    assert_eq!(journal.epoch(), 3);
    let back = replay_mem(&files);
    assert_eq!(back.project(), live.project());
    assert!(back.is_dirty());
    journal.close().unwrap();
    assert!(files.lock().unwrap().removed);
}
