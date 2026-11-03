//! P30 étendue aux commandes de tâches (lot J4 L4) : sur des projets tirés,
//! des lots de `task.add`, `task.update`, `task.delete`, `task.move`,
//! `task.indent`, `task.outdent` réels ; `dry_run` sans effet et même
//! résultat que le commit, refus atomique, tout annuler redonne chaque état
//! exactement, tout rétablir aussi.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::many_single_char_names,
    clippy::needless_pass_by_value,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use plannercraft_automation::text::Language;
use plannercraft_automation::{ApplyMode, Batch, DocOrigin, Session};
use plannercraft_core::{
    BuiltinCalendarNames, Duration, DurationUnit, ExtraEntry, Link, Minutes, Moment, OutlineLevel, Project,
    ProjectInit, Schedule, ScheduleAnchor, Task, TaskUid, TimeBasis,
};
use proptest::prelude::*;
use serde_json::{Value, json};

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

fn session(p: Project) -> Session {
    let origin = DocOrigin {
        name: "P".to_owned(),
        path: None,
    };
    Session::from_project(p, origin, moment("2026-10-17T08:00"), Language::Fr)
}

/// Projet tiré : 0 à 12 tâches (UID épars, niveaux valides ou non, Extras
/// de nivellement sur certaines), liens fin à début quelconques ; depuis le
/// début ou à rebours.
fn project() -> impl Strategy<Value = Project> {
    let task = (
        1u32..4,
        0i64..4,
        proptest::collection::vec(any::<usize>(), 0..2),
        1u16..4,
        any::<bool>(),
    );
    (proptest::collection::vec(task, 0..12), any::<bool>()).prop_map(|(specs, backward)| {
        let anchor = if backward {
            ScheduleAnchor::FromFinish(moment("2026-12-31T17:00"))
        } else {
            ScheduleAnchor::FromStart(moment("2026-11-02T08:00"))
        };
        let mut p = Project::new(ProjectInit {
            name: "P".to_owned(),
            anchor,
            current_date: moment("2026-10-17T08:00"),
            calendar_names: BuiltinCalendarNames::default(),
        });
        let mut uid = 0u32;
        let n = specs.len();
        let mut links = Vec::new();
        for (gap, d, preds, level, extras) in specs {
            uid += gap;
            let mut t = Task::new(TaskUid::new(uid), format!("T{uid}"), &p.options);
            t.duration = days(d);
            t.outline_level = OutlineLevel::new(level).unwrap();
            if extras {
                t.extras.0.push(ExtraEntry {
                    origin: "mspdi".to_owned(),
                    key: "LevelingDelay".to_owned(),
                    payload: "<LevelingDelay>4800</LevelingDelay>".to_owned(),
                });
            }
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

/// Opération tirée ; les index désignent une tâche du projet courant
/// (modulo leur nombre), ou une tâche créée plus tôt dans le lot.
#[derive(Clone, Debug)]
enum Op {
    Add(u8, usize, Option<i64>),
    Rename(usize),
    SetDays(usize, i64),
    Start(usize, Option<u8>),
    Milestone(usize, bool),
    Delete(usize, bool),
    Move(usize, u8, usize),
    Indent(usize),
    Outdent(usize),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0u8..5, any::<usize>(), proptest::option::of(0i64..4)).prop_map(|(k, i, d)| Op::Add(k, i, d)),
        any::<usize>().prop_map(Op::Rename),
        (any::<usize>(), 0i64..4).prop_map(|(i, d)| Op::SetDays(i, d)),
        (any::<usize>(), proptest::option::of(0u8..10)).prop_map(|(i, d)| Op::Start(i, d)),
        (any::<usize>(), any::<bool>()).prop_map(|(i, b)| Op::Milestone(i, b)),
        (any::<usize>(), any::<bool>()).prop_map(|(i, c)| Op::Delete(i, c)),
        (any::<usize>(), 0u8..5, any::<usize>()).prop_map(|(i, k, j)| Op::Move(i, k, j)),
        any::<usize>().prop_map(Op::Indent),
        any::<usize>().prop_map(Op::Outdent),
    ]
}

/// Désignation JSON de la `i`-ème cible : une tâche du projet ou une
/// étiquette du lot.
fn target(uids: &[TaskUid], labels: usize, i: usize) -> Value {
    let n = uids.len() + labels;
    if n == 0 {
        return json!(1);
    }
    let k = i % n;
    if k < uids.len() {
        json!(uids[k].get())
    } else {
        json!(format!("$l{}", k - uids.len()))
    }
}

fn placement(kind: u8, t: Value) -> Value {
    match kind {
        0 => json!("end"),
        1 => json!({ "after": t }),
        2 => json!({ "before": t }),
        3 => json!({ "first_child_of": t }),
        _ => json!({ "last_child_of": t }),
    }
}

/// Lot JSON des opérations, sur l'état courant de la session.
fn to_batch(s: &Session, ops: &[Op]) -> Batch {
    let uids: Vec<TaskUid> = s.project().tasks.uids().collect();
    let mut labels = 0usize;
    let mut json_ops = Vec::new();
    for op in ops {
        let t = |i: usize| target(&uids, labels, i);
        let v = match op {
            Op::Add(k, i, d) => {
                let mut v = json!({ "op": "task.add", "name": "N", "at": placement(*k, t(*i)), "as": format!("$l{labels}") });
                if let Some(d) = d {
                    v["duration"] = json!({ "minutes": d * 480, "unit": "day" });
                }
                labels += 1;
                v
            }
            Op::Rename(i) => json!({ "op": "task.update", "uid": t(*i), "name": "R" }),
            Op::SetDays(i, d) => {
                json!({ "op": "task.update", "uid": t(*i), "duration": { "minutes": d * 480 } })
            }
            Op::Start(i, d) => {
                let date = d.map(|d| format!("2026-11-{:02}T08:00", 2 + u32::from(d)));
                json!({ "op": "task.update", "uid": t(*i), "start": date })
            }
            Op::Milestone(i, b) => json!({ "op": "task.update", "uid": t(*i), "milestone": b }),
            Op::Delete(i, c) => json!({ "op": "task.delete", "uid": t(*i), "cascade": c }),
            Op::Move(i, k, j) => {
                json!({ "op": "task.move", "uid": t(*i), "to": placement(*k, t(*j)) })
            }
            Op::Indent(i) => json!({ "op": "task.indent", "uid": t(*i) }),
            Op::Outdent(i) => json!({ "op": "task.outdent", "uid": t(*i) }),
        };
        json_ops.push(v);
    }
    Batch::from_json(json!({ "format_version": 1, "ops": json_ops })).unwrap()
}

#[derive(Debug, PartialEq)]
struct Snap {
    project: Project,
    schedule: Schedule,
    generation: u64,
    history_len: usize,
}

fn snap(s: &Session) -> Snap {
    Snap {
        project: s.project().clone(),
        schedule: s.schedule().clone(),
        generation: s.generation(),
        history_len: s.history().entries.len(),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn p30_task_commands_undo_restores_exactly(
        p in project(),
        batches in proptest::collection::vec(proptest::collection::vec(op(), 1..5), 1..4),
    ) {
        let mut s = session(p);
        let mut states = vec![snap(&s)];
        for ops in batches {
            let batch = to_batch(&s, &ops);
            let before = snap(&s);
            let dry = s.apply(&batch, ApplyMode::DryRun);
            prop_assert_eq!(&snap(&s), &before);
            match s.apply(&batch, ApplyMode::Commit) {
                Ok(o) => {
                    prop_assert!(o.committed);
                    let dry = dry.unwrap();
                    prop_assert_eq!(&dry.diff, &o.diff);
                    prop_assert_eq!(&dry.warnings, &o.warnings);
                    prop_assert_eq!(&dry.labels, &o.labels);
                    prop_assert_eq!(&dry.created, &o.created);
                    prop_assert_eq!(&dry.side_effects, &o.side_effects);
                    prop_assert_eq!(s.schedule(), &plannercraft_core::schedule(s.project()));
                    states.push(snap(&s));
                }
                Err(e) => {
                    prop_assert_eq!(dry.unwrap_err(), e);
                    prop_assert_eq!(&snap(&s), &before);
                }
            }
        }
        for i in (0..states.len() - 1).rev() {
            s.undo().unwrap();
            let now = snap(&s);
            prop_assert_eq!(&now.project, &states[i].project);
            prop_assert_eq!(&now.schedule, &states[i].schedule);
        }
        prop_assert!(s.undo().is_err());
        for state in states.iter().skip(1) {
            s.redo().unwrap();
            let now = snap(&s);
            prop_assert_eq!(&now.project, &state.project);
            prop_assert_eq!(&now.schedule, &state.schedule);
        }
        prop_assert!(s.redo().is_err());
    }
}
