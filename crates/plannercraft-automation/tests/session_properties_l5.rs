//! P30 étendue aux commandes du lot J4 L5 (liens, contraintes, échéances,
//! projet, exceptions de calendrier), passées en JSON par `Session::apply` :
//! `dry_run` sans effet et même diff que le commit, refus atomique, tout
//! annuler redonne chaque état exactement, tout rétablir aussi.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::many_single_char_names,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use plannercraft_automation::text::Language;
use plannercraft_automation::{ApplyMode, Batch, DocOrigin, HistoryView, Session};
use plannercraft_core::{
    BuiltinCalendarNames, Duration, DurationUnit, ExtraEntry, Link, Minutes, Moment, OutlineLevel, Project,
    ProjectInit, Schedule, ScheduleAnchor, Task, TaskUid, TimeBasis,
};
use proptest::prelude::*;

fn moment(s: &str) -> Moment {
    s.parse().unwrap()
}

const KINDS: [&str; 4] = ["fs", "ss", "ff", "sf"];
const CONSTRAINTS: [&str; 8] = ["asap", "alap", "mso", "mfo", "snet", "snlt", "fnet", "fnlt"];

/// Opération tirée ; les indices de tâche sont pris modulo le nombre de
/// tâches (UID stables : aucune commande L5 n'ajoute ni ne retire de tâche).
#[derive(Clone, Debug)]
enum Op {
    LinkAdd(usize, usize, usize, Option<i64>),
    LinkUpdate(usize, usize, Option<usize>, Option<i64>),
    LinkRemove(usize, usize),
    Constraint(usize, usize, u8),
    Deadline(usize, Option<u8>),
    Info(u8, bool),
    Anchor(bool, u8),
    StatusDate(Option<u8>),
    Exception(u8, u8, bool),
    RemoveException(u8),
    UnknownTask,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (
            any::<usize>(),
            any::<usize>(),
            0usize..4,
            proptest::option::of(-2i64..4)
        )
            .prop_map(|(a, b, k, l)| Op::LinkAdd(a, b, k, l)),
        2 => (
            any::<usize>(),
            any::<usize>(),
            proptest::option::of(0usize..4),
            proptest::option::of(-2i64..4)
        )
            .prop_map(|(a, b, k, l)| Op::LinkUpdate(a, b, k, l)),
        2 => (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Op::LinkRemove(a, b)),
        3 => (any::<usize>(), 0usize..8, 0u8..30).prop_map(|(a, k, d)| Op::Constraint(a, k, d)),
        2 => (any::<usize>(), proptest::option::of(0u8..30)).prop_map(|(a, d)| Op::Deadline(a, d)),
        1 => (any::<u8>(), any::<bool>()).prop_map(|(k, c)| Op::Info(k, c)),
        1 => (any::<bool>(), 0u8..40).prop_map(|(f, d)| Op::Anchor(f, d)),
        1 => proptest::option::of(0u8..30).prop_map(Op::StatusDate),
        3 => (0u8..10, 0u8..4, any::<bool>()).prop_map(|(d, n, w)| Op::Exception(d, n, w)),
        2 => (0u8..10).prop_map(Op::RemoveException),
        1 => Just(Op::UnknownTask),
    ]
}

/// Instant `d` jours après le lun 02/11/2026, à `hh:00`.
fn at(d: u8, hh: u8) -> String {
    let m = moment("2026-11-02T00:00")
        .checked_add(Minutes::new(i64::from(d) * 1_440 + i64::from(hh) * 60))
        .unwrap();
    m.to_string()
}

/// Jour `d` jours après le lun 02/11/2026.
fn day(d: u8) -> String {
    at(d, 0).chars().take(10).collect()
}

fn to_json(op: &Op, uids: &[u32]) -> String {
    let count = uids.len().max(1);
    let u = |i: usize| uids.get(i % count).copied().unwrap_or(1);
    let lag = |l: Option<i64>| {
        l.map_or(String::new(), |days| {
            format!(r#","lag":{{"minutes":{}}}"#, days * 480)
        })
    };
    match op {
        Op::LinkAdd(a, b, k, l) => format!(
            r#"{{"op":"link.add","pred":{},"succ":{},"kind":"{}"{}}}"#,
            u(*a),
            u(*b),
            KINDS[*k],
            lag(*l)
        ),
        Op::LinkUpdate(a, b, k, l) => format!(
            r#"{{"op":"link.update","pred":{},"succ":{}{}{}}}"#,
            u(*a),
            u(*b),
            k.map_or(String::new(), |k| format!(r#","kind":"{}""#, KINDS[k])),
            lag(*l)
        ),
        Op::LinkRemove(a, b) => format!(r#"{{"op":"link.remove","pred":{},"succ":{}}}"#, u(*a), u(*b)),
        Op::Constraint(a, k, d) => {
            let date = if *k < 2 {
                String::new()
            } else {
                format!(r#","date":"{}""#, at(*d, 8))
            };
            format!(
                r#"{{"op":"task.set_constraint","uid":{},"kind":"{}"{date}}}"#,
                u(*a),
                CONSTRAINTS[*k]
            )
        }
        Op::Deadline(a, d) => format!(
            r#"{{"op":"task.set_deadline","uid":{},"deadline":{}}}"#,
            u(*a),
            d.map_or("null".to_owned(), |d| format!(r#""{}""#, at(d, 17)))
        ),
        Op::Info(k, clear) => {
            if *clear {
                r#"{"op":"project.set_info","author":null}"#.to_owned()
            } else {
                format!(r#"{{"op":"project.set_info","name":"P{k}","author":"A{k}"}}"#)
            }
        }
        Op::Anchor(finish, d) => {
            if *finish {
                format!(r#"{{"op":"project.set_anchor","finish":"{}"}}"#, at(*d + 10, 17))
            } else {
                format!(r#"{{"op":"project.set_anchor","start":"{}"}}"#, at(*d, 8))
            }
        }
        Op::StatusDate(d) => format!(
            r#"{{"op":"project.set_status_date","date":{}}}"#,
            d.map_or("null".to_owned(), |d| format!(r#""{}""#, at(d, 17)))
        ),
        Op::Exception(d, len, working) => format!(
            r#"{{"op":"calendar.set_exception","cal":"project","from":"{}","to":"{}"{}}}"#,
            day(*d),
            day(*d + *len),
            if *working {
                r#","ranges":[["09:00","12:00"]]"#
            } else {
                ""
            }
        ),
        Op::RemoveException(d) => format!(
            r#"{{"op":"calendar.remove_exception","cal":"project","from":"{}"}}"#,
            day(*d)
        ),
        Op::UnknownTask => r#"{"op":"task.set_deadline","uid":4000000000,"deadline":null}"#.to_owned(),
    }
}

/// Projet tiré : 1 à 10 tâches (UID épars, niveaux 1 à 3, durées 0 à 4 j),
/// liens fin à début quelconques (erreurs héritées possibles), `Extras` de
/// nivellement sur certaines tâches (R-161).
fn project() -> impl Strategy<Value = Project> {
    let task = (
        1u32..4,
        0i64..5,
        proptest::collection::vec(any::<usize>(), 0..2),
        1u16..4,
        any::<bool>(),
    );
    proptest::collection::vec(task, 1..10).prop_map(|specs| {
        let mut p = Project::new(ProjectInit {
            name: "P".to_owned(),
            anchor: ScheduleAnchor::FromStart(moment("2026-11-02T08:00")),
            current_date: moment("2026-10-17T08:00"),
            calendar_names: BuiltinCalendarNames::default(),
        });
        let mut uid = 0u32;
        let n = specs.len();
        let mut links = Vec::new();
        for (gap, d, preds, level, extras) in specs {
            uid += gap;
            let mut t = Task::new(TaskUid::new(uid), format!("T{uid}"), &p.options);
            t.duration = Duration::new(
                Minutes::new(d * 480),
                DurationUnit::Day,
                TimeBasis::Working,
                false,
            )
            .unwrap();
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

#[derive(Debug, PartialEq)]
struct Snap {
    project: Project,
    schedule: Schedule,
    history: HistoryView,
    generation: u64,
}

fn snap(s: &Session) -> Snap {
    Snap {
        project: s.project().clone(),
        schedule: s.schedule().clone(),
        history: s.history(),
        generation: s.generation(),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn p30_l5_commands_undo_restores_exactly(
        p in project(),
        batches in proptest::collection::vec(proptest::collection::vec(op(), 1..4), 1..5),
    ) {
        let uids: Vec<u32> = p.tasks.iter().map(|t| t.uid().get()).collect();
        let origin = DocOrigin { name: "P".to_owned(), path: None };
        let mut s = Session::from_project(p, origin, moment("2026-10-17T08:00"), Language::Fr);
        let mut states = vec![snap(&s)];
        for ops in batches {
            let body: Vec<String> = ops.iter().map(|o| to_json(o, &uids)).collect();
            let json = format!(r#"{{"format_version":1,"ops":[{}]}}"#, body.join(","));
            let batch = Batch::from_json_str(&json).unwrap();
            let before = snap(&s);
            let dry = s.apply(&batch, ApplyMode::DryRun);
            prop_assert_eq!(&snap(&s), &before);
            match s.apply(&batch, ApplyMode::Commit) {
                Ok(o) => {
                    prop_assert!(o.committed);
                    let dry = dry.unwrap();
                    prop_assert_eq!(&dry.diff, &o.diff);
                    prop_assert_eq!(&dry.warnings, &o.warnings);
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
