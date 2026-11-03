//! P11 : sérialisation (ADR-0008, § 9.3).
//!
//! - Aller-retour JSON sans perte pour les durées, les décalages, les
//!   calendriers et de petits projets générés.
//! - Un JSON muté (un nombre remplacé, un booléen inversé) est soit refusé,
//!   soit lu en une valeur qui se resérialise à l'identique : aucune valeur
//!   hors invariant ne peut être construite, aucune valeur n'est corrigée en
//!   silence.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use std::collections::BTreeMap;

use plannercraft_core::calendar::{
    Calendar, CalendarException, DayHours, DayRule, DaySetting, MonthItem, MonthPosition, Recurrence,
    TimeRange, WorkWeek,
};
use plannercraft_core::{
    Assignment, AssignmentUid, BaselineNumber, BuiltinCalendarNames, CalendarUid, Constraint, Day, DayRange,
    Duration, DurationUnit, Lag, LagValue, Link, LinkType, Minutes, Moment, Month, Percent, Priority,
    Project, ProjectInit, Resource, ResourceKind, ResourceUid, ScheduleAnchor, SchedulingMode, SignedPercent,
    Task, TaskBaseline, TaskType, TaskUid, TimeBasis, TimeOfDay, Units, Week, Weekday, Work,
};
use proptest::prelude::*;
use serde_json::Value;

// ---------------------------------------------------------------------------
// Générateurs
// ---------------------------------------------------------------------------

fn unit() -> impl Strategy<Value = DurationUnit> {
    prop_oneof![
        Just(DurationUnit::Minute),
        Just(DurationUnit::Hour),
        Just(DurationUnit::Day),
        Just(DurationUnit::Week),
        Just(DurationUnit::Month),
    ]
}

fn basis() -> impl Strategy<Value = TimeBasis> {
    prop_oneof![Just(TimeBasis::Working), Just(TimeBasis::Elapsed)]
}

fn duration() -> impl Strategy<Value = Duration> {
    (0..=Duration::MAX_MINUTES, unit(), basis(), any::<bool>())
        .prop_map(|(m, u, b, e)| Duration::new(Minutes::new(m), u, b, e).unwrap())
}

fn lag() -> impl Strategy<Value = Lag> {
    let time = (-Duration::MAX_MINUTES..=Duration::MAX_MINUTES, unit()).prop_map(|(m, u)| LagValue::Time {
        minutes: Minutes::new(m),
        unit: u,
    });
    let percent = (-1_000_000_i64..=1_000_000).prop_map(|p| LagValue::Percent {
        percent: SignedPercent::new(p).unwrap(),
    });
    (basis(), prop_oneof![time, percent]).prop_map(|(b, v)| Lag::new(b, v).unwrap())
}

/// Instant de 2026-2027, ou près des bornes du domaine.
fn moment() -> impl Strategy<Value = Moment> {
    let start_2026 = "2026-01-01T00:00".parse::<Moment>().unwrap().minutes();
    let end_2027 = "2028-01-01T00:00".parse::<Moment>().unwrap().minutes();
    prop_oneof![
        8 => start_2026..end_2027,
        1 => 0_i64..20_000,
        1 => (Moment::MAX.minutes() - 20_000)..=Moment::MAX.minutes(),
    ]
    .prop_map(|m| Moment::from_minutes(m).unwrap())
}

fn day() -> impl Strategy<Value = Day> {
    let first = "2026-01-01".parse::<Day>().unwrap().index();
    (first..first + 730).prop_map(|d| Day::new(d).unwrap())
}

fn day_range() -> impl Strategy<Value = DayRange> {
    (day(), 0_i32..60).prop_map(|(d, len)| {
        let last = d.checked_add_days(len).unwrap();
        DayRange::new(d, last).unwrap()
    })
}

/// 1 à 5 plages disjointes au pas de 1 ou 15 minutes, jusqu'à 24:00.
fn day_hours() -> impl Strategy<Value = DayHours> + Clone {
    (
        prop::collection::btree_set(0_u16..=1_440, 2..=10),
        prop_oneof![Just(1_u16), Just(15)],
    )
        .prop_filter_map("au moins une plage", |(cuts, step)| {
            let cuts: Vec<u16> = cuts.into_iter().map(|c| (c / step * step).min(1_440)).collect();
            let mut ranges = Vec::new();
            for pair in cuts.chunks(2) {
                if let [a, b] = pair
                    && a < b
                    && ranges.len() < 5
                {
                    ranges.push(
                        TimeRange::new(TimeOfDay::new(*a).unwrap(), TimeOfDay::new(*b).unwrap()).unwrap(),
                    );
                }
            }
            // Des coupures arrondies peuvent se recouvrir : on ne garde que
            // des plages disjointes.
            DayHours::new(ranges).ok()
        })
}

fn day_setting() -> impl Strategy<Value = DaySetting> + Clone {
    prop_oneof![1 => Just(DaySetting::NonWorking), 3 => day_hours().prop_map(DaySetting::Working)]
}

fn day_rule() -> impl Strategy<Value = DayRule> + Clone {
    prop_oneof![
        2 => Just(DayRule::Inherit),
        1 => Just(DayRule::NonWorking),
        2 => day_hours().prop_map(DayRule::Working),
    ]
}

fn week<T: std::fmt::Debug + Clone>(s: impl Strategy<Value = T> + Clone) -> impl Strategy<Value = Week<T>> {
    (
        s.clone(),
        s.clone(),
        s.clone(),
        s.clone(),
        s.clone(),
        s.clone(),
        s,
    )
        .prop_map(
            |(sunday, monday, tuesday, wednesday, thursday, friday, saturday)| Week {
                sunday,
                monday,
                tuesday,
                wednesday,
                thursday,
                friday,
                saturday,
            },
        )
}

fn weekday() -> impl Strategy<Value = Weekday> {
    prop::sample::select(Weekday::ALL.to_vec())
}

fn month() -> impl Strategy<Value = Month> {
    prop::sample::select(Month::ALL.to_vec())
}

fn position() -> impl Strategy<Value = MonthPosition> {
    prop::sample::select(vec![
        MonthPosition::First,
        MonthPosition::Second,
        MonthPosition::Third,
        MonthPosition::Fourth,
        MonthPosition::Last,
    ])
}

fn item() -> impl Strategy<Value = MonthItem> {
    prop_oneof![
        Just(MonthItem::Day),
        Just(MonthItem::Weekday),
        Just(MonthItem::WeekendDay),
        weekday().prop_map(MonthItem::On),
    ]
}

fn recurrence() -> impl Strategy<Value = Recurrence> {
    prop_oneof![
        Just(Recurrence::Period),
        (1_u16..10).prop_map(|every| Recurrence::Daily { every }),
        (1_u16..4, week(any::<bool>())).prop_map(|(every, days)| Recurrence::Weekly { every, days }),
        (1_u16..4, 1_u8..=31).prop_map(|(every, day)| Recurrence::MonthlyByDay { every, day }),
        (1_u16..4, position(), item()).prop_map(|(every, position, item)| Recurrence::MonthlyByPosition {
            every,
            position,
            item
        }),
        (month(), 1_u8..=31).prop_map(|(month, day)| Recurrence::YearlyByDay { month, day }),
        (month(), position(), item()).prop_map(|(month, position, item)| Recurrence::YearlyByPosition {
            month,
            position,
            item
        }),
        (1_u16..4).prop_map(|every| Recurrence::ByDayCount { every }),
        (1_u16..4).prop_map(|every| Recurrence::ByWeekdayCount { every }),
    ]
}

fn exception() -> impl Strategy<Value = CalendarException> {
    (
        day_range(),
        day_setting(),
        recurrence(),
        prop::option::of(0_u32..20),
    )
        .prop_map(|(period, setting, recurrence, occurrences)| CalendarException {
            name: "Exception".to_owned(),
            period,
            setting,
            recurrence,
            occurrences,
        })
}

fn work_week() -> impl Strategy<Value = WorkWeek> {
    (day_range(), week(day_rule())).prop_map(|(period, days)| WorkWeek {
        name: "Semaine".to_owned(),
        period,
        days,
    })
}

/// Calendrier de base ou dérivé, avec exceptions et semaines datées
/// (éventuellement chevauchantes : la lecture les accepte).
fn calendar(uid: u32) -> impl Strategy<Value = Calendar> {
    let kind = prop_oneof![
        week(day_setting()).prop_map(move |w| Calendar::new_base(
            CalendarUid::new(uid),
            "Base".to_owned(),
            w
        )),
        week(day_rule()).prop_map(move |w| {
            let mut c =
                Calendar::new_derived(CalendarUid::new(uid), "Dérivé".to_owned(), CalendarUid::new(1));
            for d in Weekday::ALL {
                c.set_day(d, w.get(d).clone()).unwrap();
            }
            c
        }),
    ];
    (
        kind,
        prop::collection::vec(exception(), 0..6),
        prop::collection::vec(work_week(), 0..3),
    )
        .prop_map(|(c, exceptions, work_weeks)| {
            // Les exceptions et semaines datées se posent par la forme
            // sérialisée, qui accepte les chevauchements (fichier importé).
            let mut v = serde_json::to_value(&c).unwrap();
            v["exceptions"] = serde_json::to_value(exceptions).unwrap();
            v["work_weeks"] = serde_json::to_value(work_weeks).unwrap();
            serde_json::from_value(v).unwrap()
        })
}

fn constraint() -> impl Strategy<Value = Constraint> {
    prop_oneof![
        Just(Constraint::AsSoonAsPossible),
        Just(Constraint::AsLateAsPossible),
        moment().prop_map(|date| Constraint::MustStartOn { date }),
        moment().prop_map(|date| Constraint::MustFinishOn { date }),
        moment().prop_map(|date| Constraint::StartNoEarlierThan { date }),
        moment().prop_map(|date| Constraint::StartNoLaterThan { date }),
        moment().prop_map(|date| Constraint::FinishNoEarlierThan { date }),
        moment().prop_map(|date| Constraint::FinishNoLaterThan { date }),
    ]
}

fn link() -> impl Strategy<Value = Link> {
    (
        1_u32..8,
        prop::sample::select(vec![
            LinkType::FinishToStart,
            LinkType::StartToStart,
            LinkType::FinishToFinish,
            LinkType::StartToFinish,
        ]),
        lag(),
    )
        .prop_map(|(p, kind, lag)| Link {
            predecessor: TaskUid::new(p),
            kind,
            lag,
        })
}

/// Champs variables d'une tâche.
#[derive(Clone, Debug)]
struct TaskFields {
    is_null: bool,
    duration: Duration,
    work: i64,
    task_type: TaskType,
    constraint: Constraint,
    deadline: Option<Moment>,
    priority: u16,
    percent: i64,
    links: Vec<Link>,
    manual: Option<(Option<Moment>, Option<Duration>)>,
    baseline: Option<(u8, Option<Moment>)>,
}

fn task_fields() -> impl Strategy<Value = TaskFields> {
    (
        any::<bool>(),
        duration(),
        0_i64..=Work::MAX_MINUTES,
        prop::sample::select(vec![
            TaskType::FixedUnits,
            TaskType::FixedDuration,
            TaskType::FixedWork,
        ]),
        constraint(),
        prop::option::of(moment()),
        0_u16..=1_000,
        0_i64..=10_000,
        prop::collection::vec(link(), 0..3),
        prop::option::of((prop::option::of(moment()), prop::option::of(duration()))),
        prop::option::of((0_u8..=10, prop::option::of(moment()))),
    )
        .prop_map(
            |(
                is_null,
                duration,
                work,
                task_type,
                constraint,
                deadline,
                priority,
                percent,
                links,
                manual,
                baseline,
            )| {
                TaskFields {
                    is_null,
                    duration,
                    work,
                    task_type,
                    constraint,
                    deadline,
                    priority,
                    percent,
                    links,
                    manual,
                    baseline,
                }
            },
        )
}

fn project() -> impl Strategy<Value = Project> {
    (
        moment(),
        moment(),
        prop::option::of(moment()),
        prop::collection::vec(task_fields(), 0..6),
        prop::collection::vec((0_i64..=60_000, any::<bool>()), 0..3),
        prop::collection::vec(calendar(4), 0..2),
    )
        .prop_map(|(start, now, status, tasks, resources, extra_calendars)| {
            let mut p = Project::new(ProjectInit {
                name: "Projet".to_owned(),
                anchor: ScheduleAnchor::FromStart(start),
                current_date: now,
                calendar_names: BuiltinCalendarNames::default(),
            });
            p.status_date = status;
            for c in extra_calendars {
                p.calendars.push(c).unwrap();
            }
            let options = p.options;
            for (i, f) in tasks.into_iter().enumerate() {
                let uid = u32::try_from(i).unwrap() + 1;
                let mut t = Task::new(TaskUid::new(uid), format!("Tâche {uid}"), &options);
                t.is_null = f.is_null;
                t.duration = f.duration;
                t.work = Work::new(Minutes::new(f.work)).unwrap();
                t.task_type = f.task_type;
                t.constraint = f.constraint;
                t.deadline = f.deadline;
                t.priority = Priority::new(f.priority).unwrap();
                t.progress.percent_complete = Percent::new(f.percent).unwrap();
                t.predecessors = f.links;
                if let Some((start, duration)) = f.manual {
                    t.mode = SchedulingMode::Manual {
                        start,
                        finish: None,
                        duration,
                    };
                }
                if let Some((n, start)) = f.baseline {
                    t.baselines = BTreeMap::from([(
                        BaselineNumber::new(n).unwrap(),
                        TaskBaseline {
                            start,
                            ..TaskBaseline::default()
                        },
                    )]);
                }
                p.tasks.push(t).unwrap();
            }
            for (i, (units, material)) in resources.into_iter().enumerate() {
                let uid = u32::try_from(i).unwrap() + 1;
                let mut r = Resource::new(ResourceUid::new(uid), format!("R{uid}"));
                if material {
                    r.kind = ResourceKind::Material {
                        label: "t".to_owned(),
                    };
                }
                r.max_units = Units::new(units).unwrap();
                p.resources.push(r).unwrap();
                let mut a = Assignment::new(
                    AssignmentUid::new(uid),
                    TaskUid::new(1),
                    Some(ResourceUid::new(uid)),
                );
                a.units = Units::new(units).unwrap();
                p.assignments.push(a).unwrap();
            }
            p
        })
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

/// Pointeurs JSON de toutes les feuilles nombre ou booléen.
fn scalar_leaves(v: &Value, path: &str, out: &mut Vec<String>) {
    match v {
        Value::Number(_) | Value::Bool(_) => out.push(path.to_owned()),
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                scalar_leaves(item, &format!("{path}/{i}"), out);
            }
        }
        Value::Object(map) => {
            for (k, item) in map {
                scalar_leaves(
                    item,
                    &format!("{path}/{}", k.replace('~', "~0").replace('/', "~1")),
                    out,
                );
            }
        }
        Value::Null | Value::String(_) => {}
    }
}

/// Valeurs de remplacement : petites, aux bornes des types, quelconques.
fn replacement() -> impl Strategy<Value = i64> {
    prop_oneof![
        -3_i64..=3,
        prop::sample::select(vec![
            i64::MIN,
            i64::MAX,
            -1_000_001,
            1_000_001,
            10_001,
            1_001,
            1_441,
            10_081,
            65_536,
            87_308_641,
            99_999_999_961,
            600_000_000_001,
            100_000_000_000_000,
            i64::from(u32::MAX) + 1,
        ]),
        any::<i64>(),
    ]
}

/// Applique la mutation n° `index` (modulo le nombre de feuilles) à `v`.
fn mutate(v: &mut Value, index: usize, by: i64) {
    let mut leaves = Vec::new();
    scalar_leaves(v, "", &mut leaves);
    if leaves.is_empty() {
        return;
    }
    let pointer = &leaves[index % leaves.len()];
    let leaf = v.pointer_mut(pointer).unwrap();
    *leaf = match leaf {
        Value::Bool(b) => Value::Bool(!*b),
        _ => Value::from(by),
    };
}

/// Vérifie « refusé, ou relu et resérialisé à l'identique ».
fn check_mutation<T>(value: &Value) -> Result<(), TestCaseError>
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    if let Ok(read) = serde_json::from_value::<T>(value.clone()) {
        let written = serde_json::to_value(&read).unwrap();
        prop_assert_eq!(&written, value);
        let reread: T = serde_json::from_value(written).unwrap();
        prop_assert_eq!(reread, read);
    }
    Ok(())
}

fn round_trip<T>(x: &T) -> Result<(), TestCaseError>
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let json = serde_json::to_string(x).unwrap();
    let back: T = serde_json::from_str(&json).unwrap();
    prop_assert_eq!(&back, x);
    prop_assert_eq!(serde_json::to_string(&back).unwrap(), json);
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    #[test]
    fn p11_duration_round_trip(d in duration()) {
        round_trip(&d)?;
    }

    #[test]
    fn p11_lag_round_trip(l in lag()) {
        round_trip(&l)?;
    }

    #[test]
    fn p11_calendar_round_trip(c in calendar(7)) {
        round_trip(&c)?;
    }

    #[test]
    fn p11_project_round_trip(p in project()) {
        round_trip(&p)?;
    }

    #[test]
    fn p11_mutated_duration_and_lag(d in duration(), l in lag(), index in any::<usize>(), by in replacement()) {
        let mut v = serde_json::to_value(d).unwrap();
        mutate(&mut v, index, by);
        check_mutation::<Duration>(&v)?;
        let mut v = serde_json::to_value(l).unwrap();
        mutate(&mut v, index, by);
        check_mutation::<Lag>(&v)?;
    }

    #[test]
    fn p11_mutated_calendar(c in calendar(7), index in any::<usize>(), by in replacement()) {
        let mut v = serde_json::to_value(&c).unwrap();
        mutate(&mut v, index, by);
        check_mutation::<Calendar>(&v)?;
    }

    #[test]
    fn p11_mutated_project(p in project(), index in any::<usize>(), by in replacement()) {
        let mut v = serde_json::to_value(&p).unwrap();
        mutate(&mut v, index, by);
        check_mutation::<Project>(&v)?;
    }
}
