//! P30 étendue aux commandes du lot J5 L7 (`resource.*`, `assignment.*`,
//! `task.update` étendu, défauts du projet), passées en JSON par
//! `Session::apply` : `dry_run` sans effet et même diff que le commit,
//! refus atomique, tout annuler redonne chaque état exactement, tout
//! rétablir aussi (512 cas). P31 après des suites de commandes : pour toute
//! affectation de travail placée hors durée fixe,
//! `|W − d_a × U / 10 000| ≤ max(1, U / 20 000)` min (tolérance du lot L2
//! au-delà de 200 %) ; le travail enregistré d'une feuille vaut la somme de
//! ses affectations de travail.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_wrap,
    clippy::many_single_char_names,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use plannercraft_automation::command::is_summary_at;
use plannercraft_automation::text::Language;
use plannercraft_automation::{ApplyMode, Batch, DocOrigin, HistoryView, Session};
use plannercraft_core::calendar::Calendar;
use plannercraft_core::{
    AssignmentRole, BuiltinCalendarNames, CalendarUid, Duration, DurationUnit, Minutes, Moment, OutlineLevel,
    Project, ProjectInit, Resource, ResourceKind, ResourceUid, Schedule, ScheduleAnchor, Task, TaskType,
    TaskUid, TimeBasis, Units,
};
use proptest::prelude::*;

fn moment(s: &str) -> Moment {
    s.parse().unwrap()
}

const TYPES: [&str; 3] = ["fixed_units", "fixed_duration", "fixed_work"];
const KINDS: [&str; 3] = ["work", "material", "cost"];

/// Opération tirée ; tâches désignées par indice (modulo le nombre de
/// tâches, UID stables), ressources par UID 1 à 6 (1 à 4 présentes au
/// départ, 5 et 6 créées par `resource.add` ou absentes).
#[derive(Clone, Debug)]
enum Op {
    ResourceAdd(usize, Option<u16>, u8),
    ResourceUpdate(u32, Option<u16>, Option<u8>, Option<usize>),
    ResourceDelete(u32, bool),
    AssignmentAdd(usize, u32, Option<u16>, Option<u16>),
    AssignmentUpdate(usize, u32, Option<u16>, Option<u16>, Option<u8>),
    AssignmentRemove(usize, u32),
    TaskType(usize, usize),
    EffortDriven(usize, bool),
    Work(usize, u16),
    Duration(usize, u8),
    Defaults(usize, bool),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        1 => (0usize..3, proptest::option::of(0u16..300), 0u8..3)
            .prop_map(|(k, m, c)| Op::ResourceAdd(k, m, c)),
        1 => (
            1u32..6,
            proptest::option::of(0u16..300),
            proptest::option::of(0u8..4),
            proptest::option::of(0usize..3)
        )
            .prop_map(|(r, m, c, k)| Op::ResourceUpdate(r, m, c, k)),
        1 => (1u32..6, any::<bool>()).prop_map(|(r, c)| Op::ResourceDelete(r, c)),
        5 => (
            any::<usize>(),
            1u32..6,
            proptest::option::of(0u16..300),
            proptest::option::of(0u16..80)
        )
            .prop_map(|(t, r, u, w)| Op::AssignmentAdd(t, r, u, w)),
        3 => (
            any::<usize>(),
            1u32..6,
            proptest::option::of(0u16..300),
            proptest::option::of(0u16..80),
            proptest::option::of(0u8..3)
        )
            .prop_map(|(t, r, u, w, d)| Op::AssignmentUpdate(t, r, u, w, d)),
        2 => (any::<usize>(), 1u32..6).prop_map(|(t, r)| Op::AssignmentRemove(t, r)),
        2 => (any::<usize>(), 0usize..3).prop_map(|(t, k)| Op::TaskType(t, k)),
        2 => (any::<usize>(), any::<bool>()).prop_map(|(t, e)| Op::EffortDriven(t, e)),
        2 => (any::<usize>(), 0u16..80).prop_map(|(t, w)| Op::Work(t, w)),
        2 => (any::<usize>(), 0u8..6).prop_map(|(t, d)| Op::Duration(t, d)),
        1 => (0usize..3, any::<bool>()).prop_map(|(k, e)| Op::Defaults(k, e)),
    ]
}

/// Unités en pourcentage entier → JSON.
fn units(p: u16) -> String {
    format!(r#"{{"value":{}}}"#, i64::from(p) * 100)
}

/// Heures → durée JSON (travail).
fn hours(h: u16) -> String {
    format!(r#"{{"minutes":{},"unit":"hour"}}"#, i64::from(h) * 60)
}

fn to_json(op: &Op, uids: &[u32]) -> String {
    let count = uids.len().max(1);
    let t = |i: usize| uids.get(i % count).copied().unwrap_or(1);
    let opt = |name: &str, v: Option<String>| v.map_or(String::new(), |v| format!(r#","{name}":{v}"#));
    match op {
        Op::ResourceAdd(k, m, c) => {
            let kind = KINDS[*k];
            let extra = if kind == "work" {
                let cal = match c {
                    0 => r#","calendar":"own""#.to_owned(),
                    1 => r#","calendar":"project""#.to_owned(),
                    _ => r#","calendar":1"#.to_owned(),
                };
                format!("{}{cal}", opt("max_units", m.map(units)))
            } else {
                String::new()
            };
            format!(r#"{{"op":"resource.add","name":"R","kind":"{kind}"{extra}}}"#)
        }
        Op::ResourceUpdate(r, m, c, k) => format!(
            r#"{{"op":"resource.update","uid":{r}{}{}{}}}"#,
            opt("max_units", m.map(units)),
            opt(
                "calendar",
                c.map(|c| if c == 0 { "null".to_owned() } else { c.to_string() })
            ),
            opt("kind", k.map(|k| format!(r#""{}""#, KINDS[k]))),
        ),
        Op::ResourceDelete(r, c) => format!(r#"{{"op":"resource.delete","uid":{r},"cascade":{c}}}"#),
        // R3 (matérielle) : quantité à la place des unités ; R4 (coût) :
        // ni l'un ni l'autre (les refus de type sont testés par R-196).
        Op::AssignmentAdd(i, r, u, w) => format!(
            r#"{{"op":"assignment.add","task":{},"resource":{r}{}{}}}"#,
            t(*i),
            match r {
                3 => opt("quantity", u.map(units)),
                4 => String::new(),
                _ => opt("units", u.map(units)),
            },
            opt("work", w.filter(|_| *r != 3 && *r != 4).map(hours)),
        ),
        Op::AssignmentUpdate(i, r, u, w, d) => format!(
            r#"{{"op":"assignment.update","task":{},"resource":{r}{}{}{}}}"#,
            t(*i),
            match r {
                3 => opt("quantity", u.map(units)),
                4 => String::new(),
                _ => opt("units", u.map(units)),
            },
            opt("work", w.filter(|_| *r != 3 && *r != 4).map(hours)),
            opt(
                "delay",
                d.map(|d| format!(r#"{{"minutes":{}}}"#, i64::from(d) * 480))
            ),
        ),
        Op::AssignmentRemove(i, r) => {
            format!(r#"{{"op":"assignment.remove","task":{},"resource":{r}}}"#, t(*i))
        }
        Op::TaskType(i, k) => format!(r#"{{"op":"task.update","uid":{},"type":"{}"}}"#, t(*i), TYPES[*k]),
        Op::EffortDriven(i, e) => format!(r#"{{"op":"task.update","uid":{},"effort_driven":{e}}}"#, t(*i)),
        Op::Work(i, w) => format!(r#"{{"op":"task.update","uid":{},"work":{}}}"#, t(*i), hours(*w)),
        Op::Duration(i, d) => format!(
            r#"{{"op":"task.update","uid":{},"duration":{{"minutes":{}}}}}"#,
            t(*i),
            i64::from(*d) * 480
        ),
        Op::Defaults(k, e) => format!(
            r#"{{"op":"project.set_info","default_task_type":"{}","new_tasks_effort_driven":{e}}}"#,
            TYPES[*k]
        ),
    }
}

/// Projet tiré : 1 à 8 tâches (UID épars, niveaux 1 et 2, durées 0 à 4 j)
/// et quatre ressources : R1 (travail), R2 (travail, 50 %, calendrier
/// propre 4 dérivé de Standard), R3 (matérielle), R4 (coût).
fn project() -> impl Strategy<Value = Project> {
    let task = (1u32..3, 0i64..5, 1u16..3);
    proptest::collection::vec(task, 1..8).prop_map(|specs| {
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
        p.calendars
            .push(Calendar::new_derived(
                CalendarUid::new(4),
                "R2".to_owned(),
                CalendarUid::new(1),
            ))
            .unwrap();
        for (uid, kind) in [
            (1, ResourceKind::Work),
            (2, ResourceKind::Work),
            (
                3,
                ResourceKind::Material {
                    label: "m³".to_owned(),
                },
            ),
            (4, ResourceKind::Cost),
        ] {
            let mut r = Resource::new(ResourceUid::new(uid), format!("R{uid}"));
            r.kind = kind;
            if uid == 2 {
                r.max_units = Units::new(5_000).unwrap();
                r.calendar = Some(CalendarUid::new(4));
            }
            p.resources.push(r).unwrap();
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

/// P31 et cumul du travail enregistré sur l'état courant ; message
/// d'échec, ou `None`.
fn p31_violation(s: &Session) -> Option<String> {
    let p = s.project();
    for (uid, a) in &s.schedule().assignments {
        let Some(model) = p.assignments.get(*uid) else {
            continue;
        };
        let Some(task) = p.tasks.get(a.task) else {
            continue;
        };
        if a.role != AssignmentRole::Work
            || task.task_type == TaskType::FixedDuration
            || model.units.get() == 0
        {
            continue;
        }
        let u = i128::from(model.units.get());
        let w = i128::from(model.work.minutes().get());
        let d = i128::from(a.duration.get());
        // |W × 10 000 − d × U| ≤ max(1, U / 20 000) × 10 000.
        let gap = (w * 10_000 - d * u).abs();
        let tolerance = (u / 2).max(10_000);
        if gap > tolerance {
            return Some(format!("P31 : affectation {uid} W {w} d {d} U {u}"));
        }
    }
    for (pos, t) in p.tasks.iter().enumerate() {
        if is_summary_at(&p.tasks, pos) {
            continue;
        }
        let sum: i64 = p
            .assignments
            .iter()
            .filter(|a| a.task == t.uid())
            .filter(|a| {
                a.resource
                    .and_then(|r| p.resources.get(r))
                    .is_some_and(|r| r.kind == ResourceKind::Work)
            })
            .map(|a| a.work.minutes().get())
            .sum();
        let has_parts = p.assignments.iter().any(|a| a.task == t.uid());
        if has_parts && t.work.minutes().get() != sum {
            return Some(format!(
                "travail de la tâche {} : {} ≠ Σ {sum}",
                t.uid(),
                t.work.minutes().get()
            ));
        }
    }
    None
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn p30_l7_commands_undo_restores_exactly(
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
                    prop_assert_eq!(&dry.created_resources, &o.created_resources);
                    prop_assert_eq!(s.schedule(), &plannercraft_core::schedule(s.project()));
                    if let Some(v) = p31_violation(&s) {
                        prop_assert!(false, "{} après {}", v, json);
                    }
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
