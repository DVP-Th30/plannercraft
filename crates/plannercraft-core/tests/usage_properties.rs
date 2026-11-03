//! P46 (R-245, R-246, R-188) : pour tout projet généré avec affectations,
//! Σ des lignes d'affectation d'une ressource = sa ligne d'`usage`, période
//! par période ; total d'une tâche = Σ de ses lignes de travail ; découpage
//! en blocs de 64 périodes = calcul d'un seul tenant ; `usage_with` sur un
//! `CalendarSet` déjà compilé = `usage` ; 1 024 cas.
//!
//! Projets générés : 1 à 5 ressources (travail le plus souvent, parfois
//! matérielle ou de coût ; sans calendrier, Standard ou Équipe de nuit),
//! 1 à 12 feuilles automatiques sans lien (1 à 10 j, unités fixes ou durée
//! fixe, début contraint dans les 200 jours après le 01/09/2026, pour
//! franchir des bornes de mois et de trimestre), 0 à 3 affectations par
//! feuille (25 à 200 %, délai une fois sur cinq), une affectation fictive
//! (sans ressource) une fois sur dix ; plage `[from, to)` et granularité
//! aléatoires, liste de ressources aléatoire (doublons et UID inconnu
//! compris).

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

use std::collections::BTreeMap;

use plannercraft_core::calendar::BuiltinCalendarNames;
use plannercraft_core::resource::{Granularity, usage, usage_by_assignment, usage_with};
use plannercraft_core::{
    Assignment, AssignmentUid, CalendarSet, CalendarUid, Constraint, Duration, DurationUnit, Minutes, Moment,
    Project, ProjectInit, Resource, ResourceKind, ResourceUid, ScheduleAnchor, Task, TaskType, TaskUid,
    TimeBasis, Units, Work, schedule,
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

fn origin() -> Moment {
    "2026-09-01T08:00".parse().unwrap()
}

fn days(n: i64) -> Minutes {
    Minutes::new(n * 1_440)
}

/// Projet de P46 (voir l'en-tête).
fn project(r: &mut Rng) -> Project {
    let mut p = Project::new(ProjectInit {
        name: "P46".to_owned(),
        anchor: ScheduleAnchor::FromStart(origin()),
        current_date: origin(),
        calendar_names: BuiltinCalendarNames::default(),
    });
    let resources = r.range(1, 5) as u32;
    for k in 1..=resources {
        let mut res = Resource::new(ResourceUid::new(k), format!("R{k}"));
        res.calendar = match r.range(0, 2) {
            0 => None,
            1 => Some(CalendarUid::new(1)),
            _ => Some(CalendarUid::new(3)),
        };
        res.kind = match r.range(0, 9) {
            0 => ResourceKind::Material {
                label: "u".to_owned(),
            },
            1 => ResourceKind::Cost,
            _ => ResourceKind::Work,
        };
        p.resources.push(res).unwrap();
    }
    let options = p.options;
    let mut next = 1u32;
    for i in 1..=(r.range(1, 12) as u32) {
        let mut t = Task::new(TaskUid::new(i), format!("T{i}"), &options);
        let d = r.range(1, 10) * 480;
        t.duration = Duration::new(Minutes::new(d), DurationUnit::Day, TimeBasis::Working, false).unwrap();
        if r.chance(25) {
            t.task_type = TaskType::FixedDuration;
        }
        let at = origin().checked_add(days(r.range(0, 200))).unwrap();
        t.constraint = Constraint::StartNoEarlierThan { date: at };
        p.tasks.push(t).unwrap();
        for _ in 0..r.range(0, 3) {
            let resource = if r.chance(10) {
                None
            } else {
                Some(ResourceUid::new(r.range(1, i64::from(resources)) as u32))
            };
            let mut a = Assignment::new(AssignmentUid::new(next), TaskUid::new(i), resource);
            a.units = Units::new(r.range(1, 8) * 2_500).unwrap();
            a.work = Work::new(Minutes::new(r.range(0, 80) * 60)).unwrap();
            if r.chance(20) {
                a.delay = Minutes::new(r.range(1, 8) * 60);
            }
            p.assignments.push(a).unwrap();
            next += 1;
        }
    }
    p
}

fn granularity(r: &mut Rng) -> Granularity {
    [
        Granularity::Day,
        Granularity::Week,
        Granularity::Month,
        Granularity::Quarter,
    ][r.range(0, 3) as usize]
}

fn minutes(w: &[Work]) -> Vec<i64> {
    w.iter().map(|w| w.minutes().get()).collect()
}

/// Ressources demandées : toutes, dans un ordre mélangé, avec un doublon et
/// un UID inconnu une fois sur deux ; sinon un sous-ensemble.
fn requested(r: &mut Rng, p: &Project) -> Vec<ResourceUid> {
    let n = p.resources.len() as i64;
    let mut out: Vec<ResourceUid> = (0..r.range(1, n + 2))
        .map(|_| ResourceUid::new(r.range(1, n) as u32))
        .collect();
    if r.chance(50) {
        out.extend((1..=n).map(|k| ResourceUid::new(k as u32)));
        out.push(ResourceUid::new(99));
    }
    out
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1_024))]

    /// P46 : Σ des lignes = ligne de la ressource ; totaux par tâche ;
    /// `usage_with` = `usage`.
    #[test]
    fn p46_assignment_rows_sum_to_resource(seed in any::<u64>()) {
        let mut r = Rng(seed | 1);
        let p = project(&mut r);
        let s = schedule(&p);
        let (set, _) = CalendarSet::compile(&p);
        let by = granularity(&mut r);
        let from = origin().checked_add(days(r.range(-20, 150))).unwrap();
        let to = from.checked_add(Minutes::new(r.range(0, 120 * 24) * 60)).unwrap();
        let wanted = requested(&mut r, &p);
        let line = usage(&p, &s, from, to, by, &wanted).unwrap();
        let with = usage_with(&p, &s, &set, from, to, by, &wanted).unwrap();
        prop_assert_eq!(&line, &with);
        let rows = usage_by_assignment(&p, &s, &set, from, to, by, &wanted).unwrap();
        let n = line.periods.len();
        prop_assert_eq!(rows.periods.len(), n);
        prop_assert_eq!(rows.periods.first().map(|x| x.start), (n > 0).then_some(from));
        prop_assert_eq!(rows.periods.last().map(|x| x.finish), (n > 0).then_some(to));
        // Lignes triées, sans doublon.
        let keys: Vec<_> = rows.rows.iter().map(|a| (a.resource, a.task, a.assignment)).collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        sorted.dedup();
        prop_assert_eq!(&keys, &sorted);
        for row in &line.rows {
            let mut sum = vec![0_i64; n];
            for a in rows.rows.iter().filter(|a| a.resource == row.resource) {
                prop_assert_eq!(a.work.len(), n);
                for (x, w) in sum.iter_mut().zip(&a.work) {
                    *x += w.minutes().get();
                }
            }
            prop_assert_eq!(&sum, &minutes(&row.work), "ressource {:?}", row.resource);
        }
        // Totaux par tâche : Σ des lignes à une ressource de travail.
        let mut totals: BTreeMap<TaskUid, Vec<i64>> = BTreeMap::new();
        for a in &rows.rows {
            if p.resources.get(a.resource).is_some_and(|x| x.kind == ResourceKind::Work) {
                let t = totals.entry(a.task).or_insert_with(|| vec![0; n]);
                for (x, w) in t.iter_mut().zip(&a.work) {
                    *x += w.minutes().get();
                }
            }
        }
        let got: BTreeMap<TaskUid, Vec<i64>> =
            rows.task_totals.iter().map(|(t, w)| (*t, minutes(w))).collect();
        prop_assert_eq!(got.len(), rows.task_totals.len());
        prop_assert_eq!(got, totals);
    }

    /// P46 : le calcul par blocs de 64 périodes, mis bout à bout, égale le
    /// calcul d'un seul tenant (lignes et totaux).
    #[test]
    fn p46_blocks_equal_whole(seed in any::<u64>()) {
        let mut r = Rng(seed | 1);
        let p = project(&mut r);
        let s = schedule(&p);
        let (set, _) = CalendarSet::compile(&p);
        let by = [Granularity::Day, Granularity::Week][r.range(0, 1) as usize];
        let from = origin().checked_add(Minutes::new(r.range(-10 * 24, 10 * 24) * 60)).unwrap();
        let to = from.checked_add(days(r.range(1, 300))).unwrap();
        let wanted = requested(&mut r, &p);
        let whole = usage_by_assignment(&p, &s, &set, from, to, by, &wanted).unwrap();
        let line = usage_with(&p, &s, &set, from, to, by, &wanted).unwrap();
        let n = whole.periods.len();
        let mut rows: Vec<Vec<i64>> = vec![Vec::new(); whole.rows.len()];
        let mut totals: Vec<Vec<i64>> = vec![Vec::new(); whole.task_totals.len()];
        let mut lines: Vec<Vec<i64>> = vec![Vec::new(); line.rows.len()];
        let mut k = 0;
        while k < n {
            let end = (k + 64).min(n);
            let (a, b) = (whole.periods[k].start, whole.periods[end - 1].finish);
            let block = usage_by_assignment(&p, &s, &set, a, b, by, &wanted).unwrap();
            prop_assert_eq!(&block.periods[..], &whole.periods[k..end]);
            prop_assert_eq!(block.rows.len(), rows.len());
            for (acc, row) in rows.iter_mut().zip(&block.rows) {
                acc.extend(minutes(&row.work));
            }
            // Une tâche sans travail dans le bloc garde son total (nul).
            prop_assert_eq!(block.task_totals.len(), totals.len());
            for (acc, (_, w)) in totals.iter_mut().zip(&block.task_totals) {
                acc.extend(minutes(w));
            }
            let l = usage_with(&p, &s, &set, a, b, by, &wanted).unwrap();
            for (acc, row) in lines.iter_mut().zip(&l.rows) {
                acc.extend(minutes(&row.work));
                }
            k = end;
        }
        for (acc, row) in rows.iter().zip(&whole.rows) {
            prop_assert_eq!(acc, &minutes(&row.work));
        }
        for (acc, (_, w)) in totals.iter().zip(&whole.task_totals) {
            prop_assert_eq!(acc, &minutes(w));
        }
        for (acc, row) in lines.iter().zip(&line.rows) {
            prop_assert_eq!(acc, &minutes(&row.work));
        }
    }
}

/// `usage_with` sur un `CalendarSet` déjà compilé = `usage`, pour les quatre
/// granularités (contrat J7, ADR-0015 D7) ; exemple fixe.
#[test]
fn usage_with_equals_usage() {
    let mut r = Rng(2026);
    let p = project(&mut r);
    let s = schedule(&p);
    let (set, _) = CalendarSet::compile(&p);
    let all: Vec<ResourceUid> = (1..=p.resources.len() as u32).map(ResourceUid::new).collect();
    let (from, to) = (origin(), origin().checked_add(days(400)).unwrap());
    for by in [
        Granularity::Day,
        Granularity::Week,
        Granularity::Month,
        Granularity::Quarter,
    ] {
        let a = usage(&p, &s, from, to, by, &all).unwrap();
        let b = usage_with(&p, &s, &set, from, to, by, &all).unwrap();
        assert_eq!(a, b, "{by:?}");
        assert!(!a.periods.is_empty(), "{by:?}");
    }
}

/// Le générateur de P46 n'est pas trivial : sur 200 graines, la plupart des
/// projets ont du travail dans au moins une ligne d'affectation, et
/// certains une ligne matérielle ou une ressource de coût demandée.
#[test]
fn p46_generator_is_not_trivial() {
    let (mut busy, mut material, mut cost) = (0, 0, 0);
    for seed in 0..200_u64 {
        let mut r = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let p = project(&mut r);
        let s = schedule(&p);
        let (set, _) = CalendarSet::compile(&p);
        let all: Vec<ResourceUid> = (1..=p.resources.len() as u32).map(ResourceUid::new).collect();
        let (from, to) = (origin(), origin().checked_add(days(260)).unwrap());
        let rows = usage_by_assignment(&p, &s, &set, from, to, Granularity::Week, &all).unwrap();
        if rows
            .rows
            .iter()
            .any(|a| a.work.iter().any(|w| w.minutes().get() > 0))
        {
            busy += 1;
        }
        let kind = |k: fn(&ResourceKind) -> bool| p.resources.iter().any(|x| k(&x.kind));
        material += i32::from(kind(|k| matches!(k, ResourceKind::Material { .. })));
        cost += i32::from(kind(|k| matches!(k, ResourceKind::Cost)));
    }
    assert!(busy > 150, "{busy} projets chargés");
    assert!(
        material > 20 && cost > 20,
        "{material} matérielles, {cost} de coût"
    );
}
