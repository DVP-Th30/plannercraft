//! P31 à P33 : équilibrage durée, travail et unités à l'édition
//! (`resource::edit::apply`, R-170, R-189 à R-194), 1 024 cas chacune, valeurs
//! générées dans les bornes de R-002 (unités ≤ 600 000 000 000), du travail
//! (S11) et des durées (domaine des dates).

#![allow(
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use plannercraft_core::resource::edit::effective_effort_driven;
use plannercraft_core::resource::{Edit, Part, Rebalanced, TaskSide, apply};
use plannercraft_core::{AssignmentUid, Duration, Minutes, TaskType, Units, Work, round_div};
use proptest::prelude::*;

fn work(m: i64) -> Work {
    Work::new(Minutes::new(m)).unwrap()
}

fn units(u: i64) -> Units {
    Units::new(u).unwrap()
}

/// Unités : souvent réalistes (≤ 400 %), parfois sur toute la plage R-002.
fn units_value() -> impl Strategy<Value = i64> {
    prop_oneof![3 => 1_i64..=40_000, 1 => 1_i64..=Units::MAX]
}

/// Travail : souvent quelques semaines, parfois toute la plage (S11).
fn work_value() -> impl Strategy<Value = i64> {
    prop_oneof![3 => 0_i64..=100_000, 1 => 0_i64..=Work::MAX_MINUTES]
}

/// Durée : souvent quelques semaines, parfois tout le domaine.
fn duration_value() -> impl Strategy<Value = i64> {
    prop_oneof![3 => 0_i64..=20_000, 1 => 0_i64..=Duration::MAX_MINUTES]
}

fn task_type() -> impl Strategy<Value = TaskType> {
    prop_oneof![
        Just(TaskType::FixedUnits),
        Just(TaskType::FixedDuration),
        Just(TaskType::FixedWork)
    ]
}

/// Édition générée ; les affectations sont désignées par rang (modulo).
#[derive(Clone, Debug)]
enum Step {
    Duration(i64),
    Work(i64),
    Units(usize, i64),
    AssignmentWork(usize, i64),
    Add(i64),
    Remove(usize),
    Type(TaskType),
    EffortDriven(bool),
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        1 => duration_value().prop_map(Step::Duration),
        1 => work_value().prop_map(Step::Work),
        1 => (any::<usize>(), prop_oneof![1 => Just(0_i64), 6 => units_value()]).prop_map(|(i, u)| Step::Units(i, u)),
        1 => (any::<usize>(), work_value()).prop_map(|(i, w)| Step::AssignmentWork(i, w)),
        3 => prop_oneof![1 => Just(0_i64), 6 => units_value()].prop_map(Step::Add),
        1 => any::<usize>().prop_map(Step::Remove),
        1 => task_type().prop_map(Step::Type),
        1 => any::<bool>().prop_map(Step::EffortDriven),
    ]
}

/// État courant d'une tâche et de ses affectations.
struct State {
    side: TaskSide,
    parts: Vec<Part>,
    next_uid: u32,
}

impl State {
    fn pick(&self, i: usize) -> Option<AssignmentUid> {
        if self.parts.is_empty() {
            None
        } else {
            Some(self.parts[i % self.parts.len()].uid)
        }
    }

    /// Traduit `step` en édition (`None` si aucune affectation à désigner).
    fn edit(&mut self, step: &Step) -> Option<Edit> {
        Some(match *step {
            Step::Duration(d) => Edit::Duration(Minutes::new(d)),
            Step::Work(w) => Edit::Work(work(w)),
            Step::Units(i, u) => Edit::Units {
                uid: self.pick(i)?,
                units: units(u),
            },
            Step::AssignmentWork(i, w) => Edit::AssignmentWork {
                uid: self.pick(i)?,
                work: work(w),
            },
            Step::Add(u) => {
                self.next_uid += 1;
                Edit::Add {
                    uid: AssignmentUid::new(self.next_uid),
                    units: units(u),
                }
            }
            Step::Remove(i) => Edit::Remove(self.pick(i)?),
            Step::Type(t) => Edit::Type(t),
            Step::EffortDriven(b) => Edit::EffortDriven(b),
        })
    }

    /// Écrit le résultat comme le ferait une commande.
    fn commit(&mut self, edit: &Edit, out: Rebalanced) {
        self.side.duration = out.duration;
        self.side.work = out.work;
        self.parts = out.parts;
        match *edit {
            Edit::Type(t) => {
                self.side.task_type = t;
                self.side.effort_driven = effective_effort_driven(t, self.side.effort_driven);
            }
            Edit::EffortDriven(b) => self.side.effort_driven = b,
            _ => {}
        }
    }
}

fn total_work(parts: &[Part]) -> i64 {
    parts.iter().map(|p| p.work.minutes().get()).sum()
}

/// P31 pour une affectation hors durée fixe : `d_a = W × 10 000 / U`
/// (R-170) dans le domaine des durées et `|W − d_a × U / 10 000| ≤ 1` min.
/// Au-delà de 200 %, la minute de `d_a` ne permet pas mieux que
/// `U / 20 000` min (`|d_a − W × 10 000 / U| ≤ ½`) : la tolérance est
/// alors celle-là.
fn p31_holds(p: &Part) -> bool {
    let u = i128::from(p.units.get());
    if u == 0 {
        return true;
    }
    let w = i128::from(p.work.minutes().get());
    let Some(d) = round_div(w * 10_000, u) else {
        return false;
    };
    if !(0..=Duration::MAX_MINUTES).contains(&d) {
        return false;
    }
    let gap = (w * 10_000 - i128::from(d) * u).abs();
    gap <= 10_000.max((u + 1) / 2)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    /// P31 : après toute suite d'éditions acceptées, chaque affectation
    /// d'une tâche hors durée fixe vérifie R-170 ; le travail rendu est la
    /// somme des affectations.
    #[test]
    fn p31_work_matches_duration_times_units(
        kind in task_type(),
        effort in any::<bool>(),
        d0 in duration_value(),
        steps in prop::collection::vec(step(), 1..12),
    ) {
        let mut s = State {
            side: TaskSide {
                task_type: kind,
                effort_driven: effective_effort_driven(kind, effort),
                duration: Minutes::new(d0),
                work: Work::ZERO,
                summary: false,
            },
            parts: Vec::new(),
            next_uid: 0,
        };
        for st in &steps {
            let Some(edit) = s.edit(st) else { continue };
            let Ok(out) = apply(&s.side, &s.parts, &edit) else { continue };
            let neutral = matches!(edit, Edit::Type(_) | Edit::EffortDriven(_));
            s.commit(&edit, out);
            prop_assert!(s.parts.windows(2).all(|w| w[0].uid < w[1].uid), "parts triées par UID");
            if neutral {
                continue;
            }
            prop_assert_eq!(s.side.work.minutes().get(), total_work(&s.parts));
            prop_assert!((0..=Duration::MAX_MINUTES).contains(&s.side.duration.get()));
            if s.side.task_type != TaskType::FixedDuration {
                for p in &s.parts {
                    prop_assert!(p31_holds(p), "P31 : {:?} après {:?}", p, edit);
                }
            }
        }
    }

    /// P32 : conservation selon le type (travail fixe garde W, unités fixes
    /// gardent U, durée fixe garde D) ; pilotée par l'effort, Σ W gardé à
    /// l'ajout et au retrait (hors première et dernière affectation).
    #[test]
    fn p32_conservation_by_type(
        kind in task_type(),
        effort in any::<bool>(),
        d0 in duration_value(),
        initial in prop::collection::vec((units_value(), work_value()), 1..5),
        st in step(),
    ) {
        let parts: Vec<Part> = initial
            .iter()
            .enumerate()
            .map(|(i, &(u, w))| Part { uid: AssignmentUid::new(u32::try_from(i).unwrap() + 1), units: units(u), work: work(w) })
            .collect();
        let Some(total) = parts.iter().try_fold(0_i64, |a, p| a.checked_add(p.work.minutes().get())) else {
            return Ok(());
        };
        let Ok(total_w) = Work::new(Minutes::new(total)) else { return Ok(()) };
        let mut s = State {
            side: TaskSide {
                task_type: kind,
                effort_driven: effective_effort_driven(kind, effort),
                duration: Minutes::new(d0),
                work: total_w,
                summary: false,
            },
            parts,
            next_uid: 10,
        };
        let Some(edit) = s.edit(&st) else { return Ok(()) };
        let Ok(out) = apply(&s.side, &s.parts, &edit) else { return Ok(()) };
        let before = &s.parts;
        let removed_last = matches!(edit, Edit::Remove(_)) && before.len() == 1;
        match kind {
            TaskType::FixedWork => {
                if matches!(edit, Edit::Duration(_) | Edit::Units { .. } | Edit::Add { .. } | Edit::Type(_) | Edit::EffortDriven(_))
                    || (matches!(edit, Edit::Remove(_)) && !removed_last)
                {
                    prop_assert_eq!(out.work, s.side.work, "travail fixe : W gardé");
                }
                if matches!(edit, Edit::Duration(_) | Edit::Units { .. }) {
                    for (a, b) in before.iter().zip(&out.parts) {
                        prop_assert_eq!(a.work, b.work, "travail fixe : w_a gardé");
                    }
                }
            }
            TaskType::FixedUnits => {
                let edited = match edit { Edit::Units { uid, .. } => Some(uid), _ => None };
                for p in &out.parts {
                    if Some(p.uid) == edited { continue; }
                    if let Some(a) = before.iter().find(|a| a.uid == p.uid) {
                        prop_assert_eq!(a.units, p.units, "unités fixes : U gardées");
                    }
                }
            }
            TaskType::FixedDuration => {
                if !matches!(edit, Edit::Duration(_)) {
                    prop_assert_eq!(out.duration, s.side.duration, "durée fixe : D gardée");
                }
            }
        }
        if s.side.effort_driven {
            let add_not_first = matches!(edit, Edit::Add { .. });
            let remove_not_last = matches!(edit, Edit::Remove(_)) && !removed_last;
            if add_not_first || remove_not_last {
                prop_assert_eq!(out.work, s.side.work, "pilotée : Σ W gardé");
            }
        }
    }

    /// P33 : une seule affectation initiale, à l'état normalisé de R-170
    /// (unités fixes, travail fixe : D = W × 10 000 / U ; durée fixe :
    /// U = W × 10 000 / D) : ajouter puis retirer une ressource restaure
    /// exactement l'état, pilotée par l'effort ou non.
    #[test]
    fn p33_add_then_remove_restores(
        kind in task_type(),
        effort in any::<bool>(),
        u1 in units_value(),
        w1 in work_value(),
        d_fd in 1_i64..=Duration::MAX_MINUTES,
        u2 in units_value(),
    ) {
        let (d, u1) = if kind == TaskType::FixedDuration {
            let Some(u) = round_div(i128::from(w1) * 10_000, i128::from(d_fd)) else { return Ok(()) };
            let Ok(u) = Units::new(u) else { return Ok(()) };
            (d_fd, u)
        } else {
            let Some(d) = round_div(i128::from(w1) * 10_000, i128::from(u1)) else { return Ok(()) };
            if d > Duration::MAX_MINUTES { return Ok(()); }
            (d, units(u1))
        };
        let side = TaskSide {
            task_type: kind,
            effort_driven: effective_effort_driven(kind, effort),
            duration: Minutes::new(d),
            work: work(w1),
            summary: false,
        };
        let start = vec![Part { uid: AssignmentUid::new(1), units: u1, work: work(w1) }];
        let r2 = AssignmentUid::new(2);
        let Ok(added) = apply(&side, &start, &Edit::Add { uid: r2, units: units(u2) }) else { return Ok(()) };
        let mid = TaskSide { duration: added.duration, work: added.work, ..side };
        let back = apply(&mid, &added.parts, &Edit::Remove(r2)).unwrap();
        prop_assert_eq!(back.duration, side.duration);
        prop_assert_eq!(back.work, side.work);
        prop_assert_eq!(back.parts, start);
    }
}
