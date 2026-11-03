//! R-170 — Équation par affectation : `W_a = d_a × U_a / 10 000` (R-001) ;
//! inverse `d_a = W_a × 10 000 / U_a` en `i128`, refus `Overflow` au-delà du
//! domaine des durées. Les aides de ce fichier servent aussi aux tests de
//! R-189 à R-194 (`resource::edit`, fonctions pures en minutes ouvrées).

use plannercraft_core::resource::{Edit, EditError, Part, Rebalanced, Recalculated, TaskSide, apply};
use plannercraft_core::{AssignmentUid, Minutes, TaskType, Units, Work};

/// Un jour ouvré du calendrier Standard : 480 min.
pub(crate) const DAY: i64 = 480;
/// Une heure : 60 min.
pub(crate) const HOUR: i64 = 60;

pub(crate) fn work(minutes: i64) -> Work {
    Work::new(Minutes::new(minutes)).unwrap()
}

pub(crate) fn units(v: i64) -> Units {
    Units::new(v).unwrap()
}

pub(crate) fn uid(v: u32) -> AssignmentUid {
    AssignmentUid::new(v)
}

/// Affectation `uid` à `u` (1/10 000) avec `w` minutes de travail.
pub(crate) fn part(id: u32, u: i64, w: i64) -> Part {
    Part {
        uid: uid(id),
        units: units(u),
        work: work(w),
    }
}

/// Tâche feuille de type `t`, durée `d`, travail = Σ des affectations.
pub(crate) fn task(t: TaskType, effort_driven: bool, d: i64, parts: &[Part]) -> TaskSide {
    let total = parts.iter().map(|p| p.work.minutes().get()).sum();
    TaskSide {
        task_type: t,
        effort_driven,
        duration: Minutes::new(d),
        work: work(total),
        summary: false,
    }
}

/// Applique `edit` et contrôle durée, travail total et grandeur recalculée.
pub(crate) fn check(
    side: &TaskSide,
    parts: &[Part],
    edit: Edit,
    duration: i64,
    total: i64,
    recalculated: Recalculated,
) -> Rebalanced {
    let out = apply(side, parts, &edit).unwrap();
    assert_eq!(out.duration, Minutes::new(duration), "durée");
    assert_eq!(out.work, work(total), "travail");
    assert_eq!(out.recalculated, recalculated, "grandeur recalculée");
    out
}

#[test]
fn r170_work_equals_duration_times_units() {
    // Durée fixe, 5 j = 2 400 min, sans affectation (la première est
    // exemptée du pilotage) : W = 2 400 × 10 000 / 10 000 = 2 400 (40 h).
    let side = task(TaskType::FixedDuration, false, 5 * DAY, &[]);
    let add = |u| Edit::Add {
        uid: uid(1),
        units: units(u),
    };
    let full = check(&side, &[], add(10_000), 5 * DAY, 40 * HOUR, Recalculated::Work);
    assert_eq!(full.parts, [part(1, 10_000, 2_400)]);
    // À 50 % : 2 400 × 5 000 / 10 000 = 1 200 (20 h).
    let half = check(&side, &[], add(5_000), 5 * DAY, 20 * HOUR, Recalculated::Work);
    assert_eq!(half.parts, [part(1, 5_000, 1_200)]);
    // Arrondi R-001 : 7 min à 50 % = 3,5 → 4.
    let odd = task(TaskType::FixedUnits, false, 7, &[]);
    let out = check(&odd, &[], add(5_000), 7, 4, Recalculated::Work);
    assert_eq!(out.parts, [part(1, 5_000, 4)]);
}

#[test]
fn r170_inverse_from_work() {
    // Unités fixes, 20 h saisies sur une affectation à 5 000 :
    // d = 1 200 × 10 000 / 5 000 = 2 400 (5 j).
    let parts = [part(1, 5_000, 600)];
    let side = task(TaskType::FixedUnits, false, 1_200, &parts);
    let edit = Edit::AssignmentWork {
        uid: uid(1),
        work: work(20 * HOUR),
    };
    check(&side, &parts, edit, 5 * DAY, 20 * HOUR, Recalculated::Duration);
}

#[test]
fn r170_inverse_overflow_refused() {
    // 1 000 000 min à 0,01 % (U = 1) : d = 10¹⁰ min > 87 308 640 (domaine
    // des durées) → refus, jamais de panique.
    let parts = [part(1, 1, 0)];
    let side = task(TaskType::FixedUnits, false, 0, &parts);
    let edit = Edit::AssignmentWork {
        uid: uid(1),
        work: work(1_000_000),
    };
    assert_eq!(apply(&side, &parts, &edit), Err(EditError::Overflow));
    // Travail maximal (S11 : 1 666 666 666 h) à U = 1 : même refus.
    let edit = Edit::AssignmentWork {
        uid: uid(1),
        work: work(Work::MAX_MINUTES),
    };
    assert_eq!(apply(&side, &parts, &edit), Err(EditError::Overflow));
}
