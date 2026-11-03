//! Session d'édition (lot J4 L2) : pipeline d'application, atomicité,
//! `dry_run`, lot vide, coût du clone, et la propriété P30 (« undo restaure
//! exactement »), sur des opérations de test passées par
//! `Session::apply_edits` (les commandes réelles l'étendront en L4, L5).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::print_stderr,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use std::time::Instant;

use plannercraft_automation::error::{CommandError, codes};
use plannercraft_automation::session::{Edit, EditCtx};
use plannercraft_automation::text::Language;
use plannercraft_automation::{
    ApplyMode, ApplyOptions, Batch, CommandErrorKind, DocOrigin, Session, SessionLimits,
};
use plannercraft_core::{
    Arena, BuiltinCalendarNames, Duration, DurationUnit, Link, Minutes, Moment, OutlineLevel, Project,
    ProjectInit, Resource, ResourceUid, Schedule, ScheduleAnchor, Subject, Task, TaskUid, TimeBasis,
};
use proptest::prelude::*;

fn moment(s: &str) -> Moment {
    s.parse().unwrap()
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

fn empty_project() -> Project {
    Project::new(ProjectInit {
        name: "P".to_owned(),
        anchor: ScheduleAnchor::FromStart(moment("2026-11-02T08:00")),
        current_date: moment("2026-10-17T08:00"),
        calendar_names: BuiltinCalendarNames::default(),
    })
}

fn session(p: Project) -> Session {
    let origin = DocOrigin {
        name: "P".to_owned(),
        path: None,
    };
    Session::from_project(p, origin, moment("2026-10-17T08:00"), Language::Fr)
}

/// Chaîne de `n` tâches d'un jour, sous des récapitulatives de 10 tâches.
fn chain(n: u32) -> Project {
    let mut p = empty_project();
    let mut uid = 0u32;
    let mut prev_leaf: Option<TaskUid> = None;
    for i in 0..n {
        uid += 1;
        let mut t = Task::new(TaskUid::new(uid), format!("T{uid}"), &p.options);
        if i % 10 == 0 {
            t.outline_level = OutlineLevel::new(1).unwrap();
            p.tasks.push(t).unwrap();
            continue;
        }
        t.outline_level = OutlineLevel::new(2).unwrap();
        t.duration = days(1);
        if let Some(prev) = prev_leaf {
            t.predecessors.push(Link::finish_to_start(prev));
        }
        prev_leaf = Some(TaskUid::new(uid));
        p.tasks.push(t).unwrap();
    }
    p
}

fn edit(f: impl Fn(&mut EditCtx<'_, '_>) -> Result<(), CommandError> + 'static) -> Edit {
    Box::new(f)
}

fn rename(uid: u32, name: &'static str) -> Edit {
    edit(move |c| {
        name.clone_into(&mut c.project().tasks.get_mut(TaskUid::new(uid)).unwrap().name);
        c.touch(Subject::Task(TaskUid::new(uid)));
        Ok(())
    })
}

fn fail() -> Edit {
    edit(|_| {
        Err(CommandError::new(
            codes::PARAM_INVALID,
            CommandErrorKind::InvalidParam("test".to_owned()),
        ))
    })
}

/// Instantané observable d'une session.
#[derive(Debug, PartialEq)]
struct Snap {
    project: Project,
    schedule: Schedule,
    history: plannercraft_automation::HistoryView,
    generation: u64,
    dirty: bool,
}

fn snap(s: &Session) -> Snap {
    Snap {
        project: s.project().clone(),
        schedule: s.schedule().clone(),
        history: s.history(),
        generation: s.generation(),
        dirty: s.is_dirty(),
    }
}

#[test]
fn session_dry_run_untouched() {
    let mut s = session(chain(12));
    let before = snap(&s);
    let o = s
        .apply_edits(
            None,
            &[rename(2, "X")],
            ApplyMode::DryRun,
            ApplyOptions::default(),
        )
        .unwrap();
    assert!(!o.committed);
    assert_eq!(snap(&s), before);
}

#[test]
fn session_atomic_on_error() {
    // Deux opérations réussies puis une refusée : rien n'est appliqué,
    // l'erreur porte l'index de l'opération fautive.
    let mut s = session(chain(12));
    let before = snap(&s);
    let err = s
        .apply_edits(
            None,
            &[rename(2, "X"), rename(3, "Y"), fail()],
            ApplyMode::Commit,
            ApplyOptions::default(),
        )
        .unwrap_err();
    assert_eq!(err.code, codes::PARAM_INVALID);
    assert_eq!(err.op_index, Some(2));
    assert_eq!(snap(&s), before);
}

#[test]
fn session_empty_batch_noop() {
    let mut s = session(chain(5));
    let before = snap(&s);
    let batch = Batch {
        format_version: 1,
        lang: None,
        ops: Vec::new(),
        label: Some("vide".to_owned()),
    };
    for mode in [ApplyMode::Commit, ApplyMode::DryRun] {
        let o = s.apply(&batch, mode).unwrap();
        assert!(!o.committed);
        assert_eq!(o.history_len, 0);
        assert_eq!(o.diff.changed_total, 0);
    }
    assert_eq!(snap(&s), before);
}

#[test]
fn session_batch_version_and_limit_refused() {
    let mut s = session(chain(3));
    let before = snap(&s);
    let batch = Batch {
        format_version: 2,
        lang: None,
        ops: Vec::new(),
        label: None,
    };
    assert_eq!(
        s.apply(&batch, ApplyMode::Commit).unwrap_err().code,
        codes::BATCH_UNSUPPORTED_VERSION
    );
    s.set_limits(SessionLimits {
        max_ops_per_batch: 1,
        ..SessionLimits::default()
    });
    let err = s
        .apply_edits(
            None,
            &[rename(1, "A"), rename(2, "B")],
            ApplyMode::Commit,
            ApplyOptions::default(),
        )
        .unwrap_err();
    assert_eq!(err.code, codes::LIMIT_EXCEEDED);
    assert_eq!(err.kind, CommandErrorKind::Limit { what: "ops", max: 1 });
    assert_eq!(snap(&s), before);
}

#[test]
fn session_clone_cost_5000() {
    // Mesure informative (ADR-0012 D3) : taille et temps du clone d'un
    // projet de 5 000 tâches ; le budget de 10 ms n'est vérifié que par
    // `session_clone_cost_5000_budget` (ignoré hors mesure, en release).
    let p = chain(5_000);
    let (median, bytes) = clone_cost(&p);
    eprintln!("session_clone_cost_5000 : médiane {median:?}, ~{bytes} octets (JSON)");
}

#[test]
#[ignore = "mesure en release : cargo test -p plannercraft-automation --release --test session_properties -- --ignored"]
fn session_clone_cost_5000_budget() {
    let p = chain(5_000);
    let (median, _) = clone_cost(&p);
    assert!(median.as_millis() <= 10, "clone de 5 000 tâches : {median:?}");
}

fn clone_cost(p: &Project) -> (std::time::Duration, usize) {
    let mut times = Vec::new();
    for _ in 0..9 {
        let t0 = Instant::now();
        let c = std::hint::black_box(p.clone());
        times.push(t0.elapsed());
        drop(c);
    }
    times.sort();
    let bytes = serde_json::to_vec(p).map_or(0, |v| v.len());
    (times[times.len() / 2], bytes)
}

#[test]
fn session_rename_delta_small_at_5000() {
    // Critère de fin L2 : le delta d'un renommage sur 5 000 tâches pèse
    // moins de 4 Kio.
    let mut s = session(chain(5_000));
    s.apply_edits(
        None,
        &[rename(2_500, "Renommée")],
        ApplyMode::Commit,
        ApplyOptions::default(),
    )
    .unwrap();
    let bytes = s.history().entries[0].bytes;
    assert!(bytes < 4 * 1024, "{bytes} octets");
}

// ---------------------------------------------------------------------------
// P30 : undo restaure exactement ; undo + redo = identique ; dry_run sans
// effet.

/// Opération de test tirée ; les indices sont pris modulo le nombre de
/// tâches au moment de l'exécution.
#[derive(Clone, Debug)]
enum Op {
    Rename(usize),
    SetDays(usize, i64),
    Add(usize, u16),
    Delete(usize),
    Swap(usize, usize),
    AddLink(usize, usize),
    ClearLinks(usize),
    SetLevel(usize, u16),
    ProjectName(u8),
    StatusDate(Option<u8>),
    AddResource,
    Fail,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        any::<usize>().prop_map(Op::Rename),
        (any::<usize>(), 0i64..6).prop_map(|(i, d)| Op::SetDays(i, d)),
        (any::<usize>(), 1u16..4).prop_map(|(i, l)| Op::Add(i, l)),
        any::<usize>().prop_map(Op::Delete),
        (any::<usize>(), any::<usize>()).prop_map(|(i, j)| Op::Swap(i, j)),
        (any::<usize>(), any::<usize>()).prop_map(|(i, j)| Op::AddLink(i, j)),
        any::<usize>().prop_map(Op::ClearLinks),
        (any::<usize>(), 1u16..4).prop_map(|(i, l)| Op::SetLevel(i, l)),
        any::<u8>().prop_map(Op::ProjectName),
        proptest::option::of(0u8..30).prop_map(Op::StatusDate),
        Just(Op::AddResource),
        Just(Op::Fail),
    ]
}

fn uid_at(p: &Project, i: usize) -> Option<TaskUid> {
    let n = p.tasks.len();
    if n == 0 {
        return None;
    }
    p.tasks.at(i % n).map(plannercraft_core::HasUid::uid)
}

fn to_edit(op: Op) -> Edit {
    edit(move |c| {
        match &op {
            Op::Rename(i) => {
                if let Some(u) = uid_at(c.project(), *i) {
                    let t = c.project().tasks.get_mut(u).unwrap();
                    t.name.push('*');
                    c.touch(Subject::Task(u));
                }
            }
            Op::SetDays(i, d) => {
                if let Some(u) = uid_at(c.project(), *i) {
                    c.project().tasks.get_mut(u).unwrap().duration = days(*d);
                    c.touch(Subject::Task(u));
                }
            }
            Op::Add(i, level) => {
                let uid = c.alloc_uid()?;
                let mut t = Task::new(uid, format!("N{}", uid.get()), &c.project().options);
                t.outline_level = OutlineLevel::new(*level).unwrap();
                let n = c.project().tasks.len();
                c.project().tasks.insert(i % (n + 1), t).unwrap();
                c.touch(Subject::Task(uid));
                c.created(uid, None);
            }
            Op::Delete(i) => {
                if let Some(u) = uid_at(c.project(), *i) {
                    c.project().tasks.remove(u);
                    c.touch(Subject::Task(u));
                }
            }
            Op::Swap(i, j) => {
                let n = c.project().tasks.len();
                if n > 1 {
                    let mut items: Vec<Task> = std::mem::take(&mut c.project().tasks).into();
                    items.swap(i % n, j % n);
                    c.project().tasks = Arena::try_from(items).unwrap();
                }
            }
            Op::AddLink(i, j) => {
                if let (Some(a), Some(b)) = (uid_at(c.project(), *i), uid_at(c.project(), *j)) {
                    c.project()
                        .tasks
                        .get_mut(b)
                        .unwrap()
                        .predecessors
                        .push(Link::finish_to_start(a));
                    c.touch(Subject::Link {
                        predecessor: a,
                        successor: b,
                    });
                }
            }
            Op::ClearLinks(i) => {
                if let Some(u) = uid_at(c.project(), *i) {
                    c.project().tasks.get_mut(u).unwrap().predecessors.clear();
                    c.touch(Subject::Task(u));
                }
            }
            Op::SetLevel(i, level) => {
                if let Some(u) = uid_at(c.project(), *i) {
                    c.project().tasks.get_mut(u).unwrap().outline_level = OutlineLevel::new(*level).unwrap();
                    c.touch(Subject::Task(u));
                }
            }
            Op::ProjectName(k) => c.project().info.name = format!("P{k}"),
            Op::StatusDate(d) => {
                c.project().status_date = d.map(|d| {
                    moment("2026-11-02T08:00")
                        .checked_add(Minutes::new(i64::from(d) * 1_440))
                        .unwrap()
                });
            }
            Op::AddResource => {
                let next = c.project().resources.max_uid().map_or(1, |r| r.get() + 1);
                let r = Resource::new(ResourceUid::new(next), format!("R{next}"));
                c.project().resources.push(r).unwrap();
            }
            Op::Fail => {
                return Err(CommandError::new(
                    codes::PARAM_INVALID,
                    CommandErrorKind::InvalidParam("test".to_owned()),
                ));
            }
        }
        Ok(())
    })
}

/// Projet tiré : 0 à 15 tâches (UID épars, niveaux valides ou non), durées
/// de 0 à 5 jours, liens fin à début quelconques (cycles et liens
/// récapitulative ↔ descendante possibles : erreurs héritées).
fn project() -> impl Strategy<Value = Project> {
    let task = (
        1u32..4,
        0i64..6,
        proptest::collection::vec(any::<usize>(), 0..3),
        1u16..4,
    );
    proptest::collection::vec(task, 0..15).prop_map(|specs| {
        let mut p = empty_project();
        let mut uid = 0u32;
        let n = specs.len();
        let mut links = Vec::new();
        for (gap, d, preds, level) in specs {
            uid += gap;
            let mut t = Task::new(TaskUid::new(uid), format!("T{uid}"), &p.options);
            t.duration = days(d);
            t.outline_level = OutlineLevel::new(level).unwrap();
            links.push((TaskUid::new(uid), preds));
            p.tasks.push(t).unwrap();
        }
        for (succ, preds) in links {
            for i in preds {
                let pred = p.tasks.at(i % n).unwrap().uid();
                if pred != succ {
                    p.tasks
                        .get_mut(succ)
                        .unwrap()
                        .predecessors
                        .push(Link::finish_to_start(pred));
                }
            }
        }
        p
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn p30_undo_restores_exactly(
        p in project(),
        batches in proptest::collection::vec(proptest::collection::vec(op(), 1..5), 1..4),
    ) {
        let mut s = session(p);
        let mut states = vec![snap(&s)];
        for ops in batches {
            let edits: Vec<Edit> = ops.iter().cloned().map(to_edit).collect();
            let before = snap(&s);
            // dry_run : ni projet, ni planning, ni historique, ni génération.
            let dry = s.apply_edits(None, &edits, ApplyMode::DryRun, ApplyOptions::default());
            prop_assert_eq!(&snap(&s), &before);
            match s.apply_edits(Some("lot".to_owned()), &edits, ApplyMode::Commit, ApplyOptions::default()) {
                Ok(o) => {
                    prop_assert!(o.committed);
                    let dry = dry.unwrap();
                    prop_assert_eq!(&dry.diff, &o.diff);
                    prop_assert_eq!(&dry.warnings, &o.warnings);
                    prop_assert_eq!(s.schedule(), &plannercraft_core::schedule(s.project()));
                    states.push(snap(&s));
                }
                Err(e) => {
                    // Atomicité : refus sans effet ; dry_run refusé pareil.
                    prop_assert_eq!(dry.unwrap_err(), e);
                    prop_assert_eq!(&snap(&s), &before);
                }
            }
        }
        // Tout annuler : chaque état d'avant, exactement.
        for i in (0..states.len() - 1).rev() {
            s.undo().unwrap();
            let now = snap(&s);
            prop_assert_eq!(&now.project, &states[i].project);
            prop_assert_eq!(&now.schedule, &states[i].schedule);
        }
        prop_assert!(s.undo().is_err());
        prop_assert!(!s.is_dirty());
        // Tout rétablir : chaque état d'après, exactement.
        for state in states.iter().skip(1) {
            s.redo().unwrap();
            let now = snap(&s);
            prop_assert_eq!(&now.project, &state.project);
            prop_assert_eq!(&now.schedule, &state.schedule);
        }
        prop_assert!(s.redo().is_err());
    }
}
