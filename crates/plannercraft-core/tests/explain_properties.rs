//! Propriété P41 (J6, R-230 à R-232) : `explain` sur des réseaux tirés.
//!
//! Générateur (graine tirée par proptest, xorshift) : 1 à 30 tâches dont
//! des récapitulatives (plan valide), durées 0 à 2 400 (10 % de jalons,
//! 10 % écoulées), 15 % sur le calendrier 24 heures ; 0 à 3 liens vers des
//! tâches antérieures, des quatre types, décalages ouvrés ou écoulés ; 10 %
//! de SNET ou FNET ; 5 % de retards de nivellement, 5 % de manuelles, 5 %
//! d'achevées, 10 % de commencées ; option de fractionnement tirée. Cas :
//! `PROPTEST_CASES` plafonnée à 1 024.
//!
//! - **P41** : pour toute tâche planifiée (UID 0 compris), des deux côtés,
//!   à une profondeur tirée de 1 à 50 : `explain` réussit ; la date est
//!   celle du `Schedule` ; le premier pas est `Driver` (pilote et borne du
//!   `TaskSchedule`) ; `Snap` présent si et seulement si la borne brute
//!   diffère du début au plus tôt, et recaler la borne sur l'échelle de la
//!   tâche redonne `early.start` (feuille automatique non commencée, sans
//!   retard, hors repli) ; la chaîne est finie (au plus `depth` maillons,
//!   `truncated` seulement à cette longueur), sans répétition, sans la
//!   tâche elle-même, chaque maillon égal au `Schedule` ; deux appels
//!   donnent la même explication.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::missing_panics_doc,
    clippy::arithmetic_side_effects,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names,
    reason = "tests : un échec doit s'arrêter net ; notations courtes du générateur"
)]

use std::collections::BTreeSet;

use plannercraft_core::calendar::BuiltinCalendarNames;
use plannercraft_core::schedule::explain::{MAX_DEPTH, Side, Step, explain};
use plannercraft_core::{
    CalendarSet, CalendarUid, Constraint, Duration, DurationUnit, Lag, LagValue, LevelingDelay, Link,
    LinkType, Minutes, Moment, OutlineLevel, Percent, Project, ProjectInit, ScheduleAnchor, SchedulingMode,
    Task, TaskOutcome, TaskRole, TaskUid, TimeBasis, TimeScale,
};
use proptest::prelude::*;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Entier dans `[lo, hi]`.
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        let span = u64::try_from(hi - lo + 1).unwrap();
        lo + i64::try_from(self.next() % span).unwrap()
    }

    /// Vrai avec une probabilité de `pct` %.
    fn chance(&mut self, pct: u64) -> bool {
        self.next() % 100 < pct
    }
}

fn start() -> Moment {
    "2026-10-19T08:00".parse().unwrap()
}

fn at(minutes_after_start: i64) -> Moment {
    Moment::from_minutes(start().minutes() + minutes_after_start).unwrap()
}

fn generate(seed: u64) -> Project {
    let mut r = Rng(seed | 1);
    let mut p = Project::new(ProjectInit {
        name: "P41".to_owned(),
        anchor: ScheduleAnchor::FromStart(start()),
        current_date: start(),
        calendar_names: BuiltinCalendarNames::default(),
    });
    p.options.progress.split_in_progress = r.chance(70);
    let n = r.range(1, 30);
    let mut level: u16 = 1;
    for i in 1..=n {
        let uid = u32::try_from(i).unwrap();
        let mut t = Task::new(TaskUid::new(uid), format!("T{uid}"), &p.options);
        // Plan valide : au plus un niveau de plus que la tâche précédente.
        level = if i == 1 {
            1
        } else {
            u16::try_from(r.range(1, i64::from(level) + 1).min(3)).unwrap()
        };
        t.outline_level = OutlineLevel::new(level).unwrap();
        let minutes = if r.chance(10) { 0 } else { r.range(60, 2_400) };
        let basis = if r.chance(10) {
            TimeBasis::Elapsed
        } else {
            TimeBasis::Working
        };
        t.duration = Duration::new(Minutes::new(minutes), DurationUnit::Day, basis, false).unwrap();
        if r.chance(15) {
            t.calendar = Some(CalendarUid::new(2));
        }
        for _ in 0..r.range(0, 3) {
            if i == 1 {
                break;
            }
            let pred = u32::try_from(r.range(1, i - 1)).unwrap();
            let kind = match r.range(0, 9) {
                0 => LinkType::StartToStart,
                1 => LinkType::FinishToFinish,
                2 => LinkType::StartToFinish,
                _ => LinkType::FinishToStart,
            };
            let lag = match r.range(0, 5) {
                0 => Lag {
                    basis: TimeBasis::Working,
                    value: LagValue::Time {
                        minutes: Minutes::new(r.range(-480, 960)),
                        unit: DurationUnit::Hour,
                    },
                },
                1 => Lag {
                    basis: TimeBasis::Elapsed,
                    value: LagValue::Time {
                        minutes: Minutes::new(r.range(0, 1_440)),
                        unit: DurationUnit::Hour,
                    },
                },
                _ => Lag::ZERO,
            };
            if !t.predecessors.iter().any(|l| l.predecessor == TaskUid::new(pred)) {
                t.predecessors.push(Link {
                    predecessor: TaskUid::new(pred),
                    kind,
                    lag,
                });
            }
        }
        if r.chance(10) {
            let date = at(r.range(0, 14_400));
            t.constraint = if r.chance(50) {
                Constraint::StartNoEarlierThan { date }
            } else {
                Constraint::FinishNoEarlierThan { date }
            };
        }
        if r.chance(5) {
            t.leveling_delay = Some(LevelingDelay {
                minutes: Minutes::new(r.range(60, 960)),
                unit: DurationUnit::Hour,
                elapsed: r.chance(30),
            });
        }
        match r.range(0, 19) {
            0 => {
                t.mode = SchedulingMode::Manual {
                    start: Some(at(r.range(0, 4_800))),
                    finish: None,
                    duration: None,
                };
            }
            1 => {
                let s = r.range(0, 4_800);
                t.progress.actual_start = Some(at(s));
                t.progress.actual_finish = Some(at(s + r.range(0, 2_000)));
            }
            2 | 3 => {
                t.progress.actual_start = Some(at(r.range(0, 4_800)));
                t.progress.percent_complete = Percent::new(r.range(0, 9_900)).unwrap();
            }
            _ => {}
        }
        p.tasks.push(t).unwrap();
    }
    p
}

fn p41(seed: u64) {
    let mut r = Rng(seed.rotate_left(17) | 1);
    let p = generate(seed);
    let s = plannercraft_core::schedule(&p);
    let (set, _) = CalendarSet::compile(&p);
    let uids: Vec<TaskUid> = std::iter::once(TaskUid::PROJECT_SUMMARY)
        .chain(p.tasks.iter().map(plannercraft_core::HasUid::uid))
        .collect();
    for uid in uids {
        let Some(ts) = s.scheduled(uid) else {
            assert!(explain(&p, &s, uid, Side::Start, 5).is_err());
            continue;
        };
        for side in [Side::Start, Side::Finish] {
            let depth = u8::try_from(r.range(1, i64::from(MAX_DEPTH))).unwrap();
            let x = explain(&p, &s, uid, side, depth).unwrap();
            assert_eq!(x, explain(&p, &s, uid, side, depth).unwrap(), "déterminisme");
            assert_eq!(x.task, uid);
            let date = match side {
                Side::Start => ts.start,
                Side::Finish => ts.finish,
            };
            assert_eq!(x.date, date);
            assert_eq!(
                x.steps.first(),
                Some(&Step::Driver {
                    driver: ts.driver,
                    raw: ts.driver_bound,
                }),
                "{uid}"
            );
            let snap = x.steps.iter().find_map(|st| match st {
                Step::Snap { raw, snapped, .. } => Some((*raw, *snapped)),
                _ => None,
            });
            match ts.driver_bound {
                Some(raw) if raw != ts.early.start => assert_eq!(snap, Some((raw, ts.early.start))),
                _ => assert_eq!(snap, None),
            }
            check_snap(&p, &set, uid, ts);
            assert!(x.chain.len() <= usize::from(depth));
            assert!(!x.truncated || x.chain.len() == usize::from(depth));
            let mut seen = BTreeSet::from([uid]);
            for l in &x.chain {
                assert!(seen.insert(l.uid), "répétition {}", l.uid);
                let cs = s.scheduled(l.uid).unwrap();
                assert_eq!(
                    (l.start, l.finish, l.driver, l.raw),
                    (cs.start, cs.finish, cs.driver, cs.driver_bound)
                );
            }
            assert_eq!(x.late.late, ts.late);
            assert_eq!(x.late.critical, ts.critical);
        }
    }
}

/// Recaler la borne brute sur l'échelle de la tâche redonne son début au
/// plus tôt : feuille automatique non commencée, sans retard de
/// nivellement, hors repli (P41).
fn check_snap(p: &Project, set: &CalendarSet, uid: TaskUid, ts: &plannercraft_core::TaskSchedule) {
    let Some(task) = p.tasks.get(uid) else {
        return;
    };
    let Some(raw) = ts.driver_bound else {
        return;
    };
    let eligible = ts.role == TaskRole::Leaf
        && !ts.manual
        && !ts.degraded
        && ts
            .progress
            .is_none_or(|g| g.state == plannercraft_core::ProgressState::NotStarted)
        && task.leveling_delay.is_none();
    if !eligible {
        return;
    }
    let cal = task.calendar.unwrap_or(p.calendar);
    let compiled = set.get(cal).unwrap();
    let scale = if task.duration.basis() == TimeBasis::Elapsed {
        TimeScale::Elapsed
    } else {
        TimeScale::Working(compiled)
    };
    let snapped = scale.forward_span(raw, ts.duration).unwrap().start;
    assert_eq!(snapped, ts.early.start, "tâche {uid}");
}

fn capped(cap: u32) -> ProptestConfig {
    ProptestConfig::with_cases(ProptestConfig::default().cases.min(cap))
}

proptest! {
    #![proptest_config(capped(1_024))]

    #[test]
    fn p41_explain(seed in any::<u64>()) {
        p41(seed);
    }
}

#[test]
fn p41_fixed_seeds() {
    for seed in [1, 2, 3, 42, 4_242, 0xDEAD_BEEF, u64::MAX] {
        p41(seed);
    }
    // Sanité du générateur : des tâches exclues ou non, l'UID 0 planifié.
    let p = generate(42);
    let s = plannercraft_core::schedule(&p);
    assert!(s.project_summary.is_some());
    assert!(s.tasks.values().any(|o| matches!(o, TaskOutcome::Scheduled(_))));
}
