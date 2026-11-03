//! P30 étendue aux commandes J6 du lot L6 (`progress.update`,
//! `progress.reschedule`, `baseline.set`), mêlées à des éditions de
//! durée, de liens, de date d'état et d'affectations, passées en JSON par
//! `Session::apply` : `dry_run` sans effet et même diff, mêmes
//! avertissements et mêmes effets que le commit, refus atomique, tout
//! annuler redonne chaque état exactement, tout rétablir aussi (512 cas).
//! Après chaque commit : avancement cohérent (R-200 : 100 % ⇔ fin réelle,
//! fin réelle ≥ début réel, début réel posé) et planning égal au recalcul
//! du projet.

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
    BuiltinCalendarNames, Duration, DurationUnit, Minutes, Moment, OutlineLevel, Percent, Project,
    ProjectInit, Resource, ResourceUid, Schedule, ScheduleAnchor, Task, TaskUid, TimeBasis,
};
use proptest::prelude::*;

fn moment(s: &str) -> Moment {
    s.parse().unwrap()
}

/// Jours ouvrés du lun 19/10/2026 au ven 13/11/2026 (dates tirées).
const DAYS: [&str; 20] = [
    "2026-10-19",
    "2026-10-20",
    "2026-10-21",
    "2026-10-22",
    "2026-10-23",
    "2026-10-26",
    "2026-10-27",
    "2026-10-28",
    "2026-10-29",
    "2026-10-30",
    "2026-11-02",
    "2026-11-03",
    "2026-11-04",
    "2026-11-05",
    "2026-11-06",
    "2026-11-09",
    "2026-11-10",
    "2026-11-11",
    "2026-11-12",
    "2026-11-13",
];
const HOURS: [&str; 4] = ["08:00", "10:00", "13:00", "17:00"];

/// Opération tirée ; tâches désignées par indice (modulo le nombre de
/// tâches, UID stables).
#[derive(Clone, Debug)]
enum Op {
    Percent(usize, u16),
    Start(usize, usize, usize),
    Finish(usize, usize, usize),
    ActualDuration(usize, u8),
    Remaining(usize, u8),
    ActualWork(usize, u16),
    PercentWork(usize, u16),
    Reschedule(Option<(usize, usize)>, Option<usize>),
    Baseline(u8, Option<usize>, bool, u8),
    Duration(usize, u8),
    Link(usize, usize),
    StatusDate(Option<(usize, usize)>),
    Assign(usize, u32),
}

fn op() -> impl Strategy<Value = Op> {
    let date = (0usize..DAYS.len(), 0usize..HOURS.len());
    prop_oneof![
        4 => (any::<usize>(), 0u16..=100).prop_map(|(t, p)| Op::Percent(t, p)),
        2 => (any::<usize>(), date.clone()).prop_map(|(t, (d, h))| Op::Start(t, d, h)),
        2 => (any::<usize>(), date.clone()).prop_map(|(t, (d, h))| Op::Finish(t, d, h)),
        1 => (any::<usize>(), 0u8..8).prop_map(|(t, d)| Op::ActualDuration(t, d)),
        1 => (any::<usize>(), 0u8..8).prop_map(|(t, d)| Op::Remaining(t, d)),
        1 => (any::<usize>(), 0u16..40).prop_map(|(t, h)| Op::ActualWork(t, h)),
        1 => (any::<usize>(), 0u16..=100).prop_map(|(t, p)| Op::PercentWork(t, p)),
        2 => (proptest::option::of(date.clone()), proptest::option::of(any::<usize>()))
            .prop_map(|(d, t)| Op::Reschedule(d, t)),
        2 => (0u8..3, proptest::option::of(any::<usize>()), any::<bool>(), 0u8..3)
            .prop_map(|(n, t, o, r)| Op::Baseline(n, t, o, r)),
        2 => (any::<usize>(), 0u8..6).prop_map(|(t, d)| Op::Duration(t, d)),
        1 => (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Op::Link(a, b)),
        1 => proptest::option::of(date).prop_map(Op::StatusDate),
        1 => (any::<usize>(), 1u32..3).prop_map(|(t, r)| Op::Assign(t, r)),
    ]
}

fn at(d: usize, h: usize) -> String {
    format!(r#""{}T{}""#, DAYS[d], HOURS[h])
}

fn to_json(op: &Op, uids: &[u32]) -> String {
    let count = uids.len().max(1);
    let t = |i: usize| uids.get(i % count).copied().unwrap_or(1);
    let update = |i: usize, field: &str| format!(r#"{{"op":"progress.update","task":{},{field}}}"#, t(i));
    match op {
        Op::Percent(i, p) => update(*i, &format!(r#""percent_complete":{{"text":"{p}%"}}"#)),
        Op::Start(i, d, h) => update(*i, &format!(r#""actual_start":{}"#, at(*d, *h))),
        Op::Finish(i, d, h) => update(*i, &format!(r#""actual_finish":{}"#, at(*d, *h))),
        Op::ActualDuration(i, d) => update(
            *i,
            &format!(r#""actual_duration":{{"minutes":{}}}"#, i64::from(*d) * 240),
        ),
        Op::Remaining(i, d) => update(
            *i,
            &format!(r#""remaining_duration":{{"minutes":{}}}"#, i64::from(*d) * 240),
        ),
        Op::ActualWork(i, h) => update(
            *i,
            &format!(
                r#""actual_work":{{"minutes":{},"unit":"hour"}}"#,
                i64::from(*h) * 60
            ),
        ),
        Op::PercentWork(i, p) => update(*i, &format!(r#""percent_work_complete":{{"value":{}}}"#, p * 100)),
        Op::Reschedule(d, i) => {
            let after = d.map_or(String::new(), |(d, h)| format!(r#","after":{}"#, at(d, h)));
            let tasks = i.map_or(String::new(), |i| format!(r#","tasks":[{}]"#, t(i)));
            format!(r#"{{"op":"progress.reschedule"{after}{tasks}}}"#)
        }
        Op::Baseline(n, i, o, r) => {
            let tasks = i.map_or(String::new(), |i| format!(r#","tasks":[{}]"#, t(i)));
            let rollup = ["none", "to_all_summaries", "from_subtasks"][usize::from(*r)];
            format!(r#"{{"op":"baseline.set","number":{n}{tasks},"rollup":"{rollup}","overwrite":{o}}}"#)
        }
        Op::Duration(i, d) => format!(
            r#"{{"op":"task.update","uid":{},"duration":{{"minutes":{}}}}}"#,
            t(*i),
            i64::from(*d) * 480
        ),
        Op::Link(a, b) => format!(r#"{{"op":"link.add","pred":{},"succ":{}}}"#, t(*a), t(*b)),
        Op::StatusDate(d) => format!(
            r#"{{"op":"project.set_status_date","date":{}}}"#,
            d.map_or("null".to_owned(), |(d, h)| at(d, h))
        ),
        Op::Assign(i, r) => format!(r#"{{"op":"assignment.add","task":{},"resource":{r}}}"#, t(*i)),
    }
}

/// Projet tiré : 1 à 7 tâches (UID épars, niveaux 1 et 2, durées 0 à 4 j)
/// et deux ressources de travail.
fn project() -> impl Strategy<Value = Project> {
    let task = (1u32..3, 0i64..5, 1u16..3);
    proptest::collection::vec(task, 1..7).prop_map(|specs| {
        let mut p = Project::new(ProjectInit {
            name: "P".to_owned(),
            anchor: ScheduleAnchor::FromStart(moment("2026-10-19T08:00")),
            current_date: moment("2026-10-17T08:00"),
            calendar_names: BuiltinCalendarNames::default(),
        });
        let mut uid = 0u32;
        for (gap, d, level) in specs {
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
            p.tasks.push(t).unwrap();
        }
        for uid in [1, 2] {
            p.resources
                .push(Resource::new(ResourceUid::new(uid), format!("R{uid}")))
                .unwrap();
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

/// R-200 sur l'état courant ; message d'échec, ou `None`. Seuls les
/// invariants qu'aucune commande antérieure à J6 ne peut défaire sont
/// vérifiés : 100 % ⇔ fin réelle, fin réelle ≥ début réel, début réel posé
/// dès que la tâche est commencée. (Une durée réduite par `task.update` sous
/// le réel reste lisible : `progress::facts` borne le réel à D.)
fn progress_violation(s: &Session) -> Option<String> {
    for t in &s.project().tasks {
        let g = &t.progress;
        if g.actual_finish.is_some() != (g.percent_complete == Percent::FULL) {
            return Some(format!("tâche {} : 100 % ⇎ fin réelle ({g:?})", t.uid()));
        }
        if let (Some(a), Some(f)) = (g.actual_start, g.actual_finish)
            && f < a
        {
            return Some(format!("tâche {} : fin réelle avant le début réel", t.uid()));
        }
        let started = g.actual_finish.is_some()
            || g.actual_duration > Minutes::ZERO
            || g.percent_complete > Percent::ZERO;
        if started && g.actual_start.is_none() {
            return Some(format!("tâche {} : commencée sans début réel", t.uid()));
        }
    }
    None
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn p30_l6_progress_and_baseline_undo_restores_exactly(
        p in project(),
        batches in proptest::collection::vec(proptest::collection::vec(op(), 1..4), 1..8),
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
                    if let Some(v) = progress_violation(&s) {
                        prop_assert!(false, "{} après {}", v, json);
                    }
                    states.push(snap(&s));
                }
                Err(e) => {
                    prop_assert_ne!(e.code, "command.not_yet_available");
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
