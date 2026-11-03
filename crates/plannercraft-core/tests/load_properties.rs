//! P36 (R-186, R-187) : la surutilisation calculée par le moteur égale un
//! oracle naïf, minute par minute, sur un horizon d'environ 30 jours, et
//! retirer une affectation n'en crée jamais ; 512 cas chacune.
//!
//! Réseaux générés : 1 à 3 ressources de travail (sans calendrier, sur le
//! Standard, l'Équipe de nuit ou un calendrier aléatoire aux plages qui
//! touchent minuit), capacité 50 à 200 % ; 1 à 8 feuilles automatiques sans
//! lien (unités fixes ou durée fixe, calendrier propre une fois sur quatre),
//! début contraint dans les 5 premiers jours ; 1 à 3 affectations par
//! feuille (25 à 200 %, délai une fois sur cinq). Sans lien, retirer une
//! affectation ne déplace aucune autre affectation : la charge ne peut que
//! baisser.

#![allow(
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::many_single_char_names,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use std::collections::{BTreeMap, BTreeSet};

use plannercraft_core::calendar::{BuiltinCalendarNames, Calendar, DayHours, DaySetting, TimeRange};
use plannercraft_core::{
    Assignment, AssignmentUid, CalendarSet, CalendarUid, CompiledCalendar, Constraint, Duration,
    DurationUnit, EffectiveCalendar, Minutes, Moment, Project, ProjectInit, Resource, ResourceUid, Schedule,
    ScheduleAnchor, Task, TaskType, TaskUid, TimeBasis, TimeOfDay, Units, Week, Weekday, Work, schedule,
};
use proptest::prelude::*;

/// Générateur pseudo-aléatoire déterministe (xorshift64*).
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

    /// Entier uniforme dans `lo..=hi`.
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % ((hi - lo + 1) as u64)) as i64
    }

    fn chance(&mut self, pct: i64) -> bool {
        self.range(0, 99) < pct
    }
}

const RANDOM: u32 = 4;

/// Calendrier de base aléatoire (UID 4) : chaque jour chômé une fois sur
/// quatre (le lundi toujours ouvré), sinon 1 à 2 plages au quart d'heure,
/// dont certaines touchent 00:00 ou 24:00.
fn random_calendar(r: &mut Rng) -> Calendar {
    let mut day = |force: bool| {
        if !force && r.chance(25) {
            return DaySetting::NonWorking;
        }
        let mut points: Vec<i64> = (0..r.range(2, 4)).map(|_| r.range(0, 96) * 15).collect();
        if r.chance(30) {
            points.push(0);
        }
        if r.chance(30) {
            points.push(1_440);
        }
        points.sort_unstable();
        points.dedup();
        let t = |m: i64| TimeOfDay::new(m as u16).unwrap();
        let ranges: Vec<TimeRange> = points
            .chunks(2)
            .filter(|c| c.len() == 2 && c[0] < c[1])
            .map(|c| TimeRange::new(t(c[0]), t(c[1])).unwrap())
            .collect();
        if ranges.is_empty() {
            DaySetting::Working(DayHours::new(vec![TimeRange::new(t(480), t(720)).unwrap()]).unwrap())
        } else {
            DaySetting::Working(DayHours::new(ranges).unwrap())
        }
    };
    let week = Week::from_fn(|wd| day(wd == Weekday::Monday));
    Calendar::new_base(CalendarUid::new(RANDOM), "Aléatoire".to_owned(), week)
}

fn start() -> Moment {
    "2026-10-19T08:00".parse().unwrap()
}

/// Réseau de P36 (voir l'en-tête).
fn network(seed: u64) -> Project {
    let mut r = Rng(seed | 1);
    let mut p = Project::new(ProjectInit {
        name: "P36".to_owned(),
        anchor: ScheduleAnchor::FromStart(start()),
        current_date: start(),
        calendar_names: BuiltinCalendarNames::default(),
    });
    p.calendars.push(random_calendar(&mut r)).unwrap();
    let resources = r.range(1, 3) as u32;
    for k in 1..=resources {
        let mut res = Resource::new(ResourceUid::new(k), format!("R{k}"));
        res.calendar = match r.range(0, 3) {
            0 => None,
            1 => Some(CalendarUid::new(1)),
            2 => Some(CalendarUid::new(3)),
            _ => Some(CalendarUid::new(RANDOM)),
        };
        res.max_units = Units::new([5_000, 10_000, 15_000, 20_000][r.range(0, 3) as usize]).unwrap();
        p.resources.push(res).unwrap();
    }
    let options = p.options;
    let mut next = 1u32;
    for i in 1..=(r.range(1, 8) as u32) {
        let mut t = Task::new(TaskUid::new(i), format!("T{i}"), &options);
        let d = r.range(1, 16) * 60;
        t.duration = Duration::new(Minutes::new(d), DurationUnit::Hour, TimeBasis::Working, false).unwrap();
        if r.chance(25) {
            t.task_type = TaskType::FixedDuration;
        }
        if r.chance(25) {
            t.calendar = Some(CalendarUid::new([1, 2, RANDOM][r.range(0, 2) as usize]));
        }
        let at = start()
            .checked_add(Minutes::new(r.range(0, 5 * 24 * 4) * 15))
            .unwrap();
        t.constraint = Constraint::StartNoEarlierThan { date: at };
        p.tasks.push(t).unwrap();
        let mut used = BTreeSet::new();
        for _ in 0..r.range(1, 3) {
            let res = r.range(1, i64::from(resources)) as u32;
            if !used.insert(res) {
                continue;
            }
            let mut a = Assignment::new(
                AssignmentUid::new(next),
                TaskUid::new(i),
                Some(ResourceUid::new(res)),
            );
            a.units = Units::new(r.range(1, 8) * 2_500).unwrap();
            a.work = Work::new(Minutes::new(r.range(0, 16) * 60)).unwrap();
            if r.chance(20) {
                a.delay = Minutes::new(r.range(1, 8) * 60);
            }
            p.assignments.push(a).unwrap();
            next += 1;
        }
    }
    p
}

/// Minutes ouvrées d'un calendrier effectif sur `[from, from + n)` (oracle :
/// `is_working` minute par minute).
fn bitmap(p: &Project, set: &CalendarSet, cal: EffectiveCalendar, from: Moment, n: usize) -> Vec<bool> {
    let compiled: Option<CompiledCalendar> = match cal {
        EffectiveCalendar::Task(c) | EffectiveCalendar::Resource(c) | EffectiveCalendar::Project(c) => {
            set.get(c).ok().cloned()
        }
        EffectiveCalendar::Intersection { task, resource } => Some(
            set.get(task)
                .unwrap()
                .intersect(set.get(resource).unwrap())
                .unwrap(),
        ),
        EffectiveCalendar::Elapsed => None,
    };
    let _ = p;
    (0..n)
        .map(|i| {
            let t = from.checked_add(Minutes::new(i as i64)).unwrap();
            compiled.as_ref().is_none_or(|c| c.is_working(t).unwrap())
        })
        .collect()
}

/// Oracle : par ressource, minutes surchargées et pic, sur `[start, end)`.
fn oracle(p: &Project, s: &Schedule) -> BTreeMap<ResourceUid, (BTreeSet<i64>, i64)> {
    let (set, _) = CalendarSet::compile(p);
    let from = Moment::at(start().day(), TimeOfDay::MIDNIGHT).unwrap();
    let end = s
        .assignments
        .values()
        .map(|a| a.span.finish)
        .max()
        .unwrap_or(from);
    let n = usize::try_from(from.minutes_until(end).get()).unwrap();
    let mut maps: BTreeMap<EffectiveCalendar, Vec<bool>> = BTreeMap::new();
    let mut out = BTreeMap::new();
    for res in &p.resources {
        let mut load = vec![0i64; n];
        for a in s.assignments.values().filter(|a| a.resource == Some(res.uid())) {
            let map = maps
                .entry(a.calendar)
                .or_insert_with(|| bitmap(p, &set, a.calendar, from, n));
            let lo = from.minutes_until(a.span.start).get() as usize;
            let hi = from.minutes_until(a.span.finish).get() as usize;
            for m in lo..hi {
                if map[m] {
                    load[m] += a.effective_units.get();
                }
            }
        }
        let cap = res.max_units.get();
        let over: BTreeSet<i64> = (0..n)
            .filter(|&m| load[m] > cap)
            .map(|m| from.minutes() + m as i64)
            .collect();
        out.insert(res.uid(), (over, load.iter().copied().max().unwrap_or(0)));
    }
    out
}

/// Minutes couvertes par les intervalles de surutilisation de `r`.
fn covered(s: &Schedule, r: ResourceUid) -> BTreeSet<i64> {
    s.resources[&r]
        .overloads
        .iter()
        .flat_map(|o| o.from.minutes()..o.to.minutes())
        .collect()
}

fn p36_matches(seed: u64) {
    let p = network(seed);
    let s = schedule(&p);
    for (r, (over, peak)) in oracle(&p, &s) {
        let load = &s.resources[&r];
        assert!(!load.truncated, "graine {seed}");
        assert_eq!(covered(&s, r), over, "graine {seed}, ressource {r:?}");
        assert_eq!(load.overallocated, !over.is_empty(), "graine {seed}");
        assert_eq!(load.peak.get(), peak, "graine {seed}, ressource {r:?}");
        // Intervalles maximaux, triés, disjoints, non contigus.
        for w in load.overloads.windows(2) {
            assert!(w[0].to < w[1].from, "graine {seed} : {w:?}");
        }
        assert_eq!(
            s.diagnostics
                .iter()
                .filter(|d| d.code() == "resource.overallocated"
                    && d.subject() == plannercraft_core::Subject::Resource(r))
                .count(),
            usize::from(load.overallocated),
            "graine {seed}"
        );
    }
}

fn p36_removal(seed: u64, pick: usize) {
    let p = network(seed);
    let before = schedule(&p);
    let uids: Vec<AssignmentUid> = p.assignments.uids().collect();
    let gone = uids[pick % uids.len()];
    let mut q = p.clone();
    q.assignments.remove(gone).unwrap();
    let after = schedule(&q);
    for r in p.resources.uids() {
        let (old, new) = (covered(&before, r), covered(&after, r));
        assert!(
            new.is_subset(&old),
            "graine {seed}, retrait {gone:?}, ressource {r:?}"
        );
        assert!(
            after.resources[&r].peak <= before.resources[&r].peak,
            "graine {seed}"
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn r186_p36_matches_minute_oracle(seed in any::<u64>()) {
        p36_matches(seed);
    }

    #[test]
    fn r186_p36_removal_never_creates(seed in any::<u64>(), pick in 0usize..64) {
        p36_removal(seed, pick);
    }
}

#[test]
fn r186_p36_fixed_seeds() {
    for seed in 0..64 {
        p36_matches(seed);
        p36_removal(seed, seed as usize);
    }
}
