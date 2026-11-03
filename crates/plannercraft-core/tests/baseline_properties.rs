//! P42 : `baseline.set` puis écarts (R-220 à R-225), 1 024 réseaux générés
//! sur le calendrier Standard (1 à 14 tâches, plan hiérarchique valide,
//! liens FS entre tâches sans lien de parenté, durées 0 à 2 400, une
//! ressource affectée à une feuille sur trois, récapitulative du projet
//! stockée une fois sur deux) :
//! - capture du projet entier puis écriture ⇒ tous les écarts sont nuls
//!   (tâches, UID 0, ressources, affectations) ;
//! - décaler une feuille non commencée de k minutes ouvrées (SNET au début
//!   planifié + k) ⇒ écart de début k ;
//! - la capture est déterministe et `apply` idempotente.

#![allow(
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use plannercraft_core::baseline::{self, BaselineSpec, Rollup, Scope};
use plannercraft_core::calendar::BuiltinCalendarNames;
use plannercraft_core::{
    Assignment, AssignmentUid, BaselineNumber, CalendarSet, Constraint, Duration, DurationUnit, Link,
    Minutes, OutlineLevel, Project, ProjectInit, Resource, ResourceUid, ScheduleAnchor, Task, TaskRole,
    TaskUid, TimeBasis, Work,
};
use proptest::prelude::*;

/// Tâche tirée : niveau relatif (0 : même niveau, 1 : un de plus, n > 1 :
/// remonte), durée, affectée ou non, prédécesseurs (positions relatives).
type Spec = (u16, i64, bool, Vec<usize>);

fn spec() -> impl Strategy<Value = Spec> {
    (
        0_u16..4,
        0_i64..=2_400,
        prop::bool::weighted(0.33),
        prop::collection::vec(1_usize..6, 0..3),
    )
}

/// Parents (positions) d'un plan hiérarchique.
fn parents(levels: &[u16]) -> Vec<Option<usize>> {
    let mut stack: Vec<(u16, usize)> = Vec::new();
    let mut out = Vec::new();
    for (i, &l) in levels.iter().enumerate() {
        while stack.last().is_some_and(|&(sl, _)| sl >= l) {
            stack.pop();
        }
        out.push(stack.last().map(|&(_, p)| p));
        stack.push((l, i));
    }
    out
}

fn related(par: &[Option<usize>], a: usize, b: usize) -> bool {
    let up = |mut x: usize, y: usize| {
        while let Some(p) = par[x] {
            if p == y {
                return true;
            }
            x = p;
        }
        false
    };
    up(a, b) || up(b, a)
}

fn build(specs: &[Spec], root: bool) -> Project {
    let start = "2026-10-19T08:00".parse().unwrap();
    let mut p = Project::new(ProjectInit {
        name: "P42".to_owned(),
        anchor: ScheduleAnchor::FromStart(start),
        current_date: start,
        calendar_names: BuiltinCalendarNames::default(),
    });
    let options = p.options;
    if root {
        let mut t = Task::new(TaskUid::PROJECT_SUMMARY, "Projet".to_owned(), &options);
        t.duration = Duration::zero(DurationUnit::Day);
        p.summary_task = Some(t);
    }
    p.resources
        .push(Resource::new(ResourceUid::new(1), "R1".to_owned()))
        .unwrap();
    let mut levels: Vec<u16> = Vec::new();
    for (rel, ..) in specs {
        let prev = levels.last().copied().unwrap_or(1);
        let level = match rel {
            0 => prev,
            1 if !levels.is_empty() => prev + 1,
            1 => 1,
            n => prev.saturating_sub(*n - 1).max(1),
        };
        levels.push(level);
    }
    let par = parents(&levels);
    let mut next_assignment = 1;
    for (i, (_, minutes, assigned, preds)) in specs.iter().enumerate() {
        let uid = TaskUid::new(u32::try_from(i + 1).unwrap());
        let mut t = Task::new(uid, format!("T{}", i + 1), &options);
        t.duration = Duration::new(
            Minutes::new(*minutes),
            DurationUnit::Day,
            TimeBasis::Working,
            false,
        )
        .unwrap();
        t.outline_level = OutlineLevel::new(levels[i]).unwrap();
        for &back in preds {
            if back > i || related(&par, i, i - back) {
                continue;
            }
            let pred = TaskUid::new(u32::try_from(i - back + 1).unwrap());
            if t.predecessors.iter().all(|l| l.predecessor != pred) {
                t.predecessors.push(Link::finish_to_start(pred));
            }
        }
        let leaf = !par.contains(&Some(i));
        if *assigned && leaf && *minutes > 0 {
            let mut a = Assignment::new(
                AssignmentUid::new(next_assignment),
                uid,
                Some(ResourceUid::new(1)),
            );
            a.work = Work::new(Minutes::new(*minutes)).unwrap();
            p.assignments.push(a).unwrap();
            next_assignment += 1;
        }
        p.tasks.push(t).unwrap();
    }
    p
}

fn primary() -> BaselineSpec {
    BaselineSpec {
        number: BaselineNumber::PRIMARY,
        scope: Scope::Project,
        rollup: Rollup::None,
        overwrite: false,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    #[test]
    fn p42_baseline_then_variance(
        specs in prop::collection::vec(spec(), 1..15),
        root in any::<bool>(),
        pick in any::<prop::sample::Index>(),
        k in 0_i64..=3_000,
    ) {
        let mut p = build(&specs, root);
        let s = plannercraft_core::schedule(&p);
        let edit = baseline::capture(&p, &s, &primary()).unwrap();
        prop_assert_eq!(&baseline::capture(&p, &s, &primary()).unwrap(), &edit);
        let _ = baseline::apply(&mut p, BaselineNumber::PRIMARY, edit.clone());
        let once = p.clone();
        let _ = baseline::apply(&mut p, BaselineNumber::PRIMARY, edit);
        prop_assert_eq!(&p, &once);

        // Tous les écarts nuls juste après la capture.
        let (set, _) = CalendarSet::compile(&p);
        let v = baseline::variance(&p, &s, &set, BaselineNumber::PRIMARY);
        for t in v.tasks.iter().chain([&v.project]) {
            if t.uid == TaskUid::PROJECT_SUMMARY && !root {
                continue;
            }
            prop_assert!(t.baseline.is_some(), "UID {} sans ligne", t.uid);
            prop_assert_eq!(t.variance, baseline::Variance::default(), "UID {}", t.uid);
        }
        prop_assert!(v.resources.iter().all(|(_, w)| *w == Minutes::ZERO));
        prop_assert!(v.assignments.iter().all(|(_, w)| *w == Minutes::ZERO));

        // Décaler une feuille de k minutes ouvrées.
        let leaves: Vec<TaskUid> = s
            .tasks
            .keys()
            .copied()
            .filter(|u| s.scheduled(*u).is_some_and(|t| t.role == TaskRole::Leaf))
            .collect();
        prop_assume!(!leaves.is_empty());
        let target = leaves[pick.index(leaves.len())];
        let before = s.scheduled(target).unwrap().start;
        let standard = set.get(p.calendar).unwrap();
        let date = standard.add_working(before, Minutes::new(k)).unwrap();
        p.tasks.get_mut(target).unwrap().constraint = Constraint::StartNoEarlierThan { date };
        let s2 = plannercraft_core::schedule(&p);
        let v2 = baseline::variance(&p, &s2, &set, BaselineNumber::PRIMARY);
        let tv = v2.tasks.iter().find(|t| t.uid == target).unwrap();
        prop_assert_eq!(tv.variance.start, Minutes::new(k), "UID {} décalée de {}", target, k);
    }
}
