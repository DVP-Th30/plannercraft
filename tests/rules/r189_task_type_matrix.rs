//! R-189 — Matrice des types de tâche (S41). Une affectation à 100 %,
//! 2 j (960 min), 16 h (960 min) ; chaque case donne la grandeur modifiée,
//! la grandeur recalculée et le résultat attendu.

use crate::r170_work_equals_duration_times_units::{DAY, HOUR, check, part, task, uid, units, work};
use plannercraft_core::TaskType::{FixedDuration, FixedUnits, FixedWork};
use plannercraft_core::resource::{Edit, Recalculated};
use plannercraft_core::{Minutes, TaskType};

fn one(
    t: TaskType,
) -> (
    plannercraft_core::resource::TaskSide,
    [plannercraft_core::resource::Part; 1],
) {
    let parts = [part(1, 10_000, 16 * HOUR)];
    (task(t, false, 2 * DAY, &parts), parts)
}

fn set_units(u: i64) -> Edit {
    Edit::Units {
        uid: uid(1),
        units: units(u),
    }
}

#[test]
fn r189_fixed_units_units_changes_duration() {
    // 960 min de travail gardées, 50 % : d = 960 × 10 000 / 5 000 = 1 920 (4 j).
    let (side, parts) = one(FixedUnits);
    let out = check(
        &side,
        &parts,
        set_units(5_000),
        4 * DAY,
        16 * HOUR,
        Recalculated::Duration,
    );
    assert_eq!(out.parts, [part(1, 5_000, 960)]);
}

#[test]
fn r189_fixed_units_duration_changes_work() {
    // 3 j = 1 440 à 100 % : W = 1 440 (24 h).
    let (side, parts) = one(FixedUnits);
    check(
        &side,
        &parts,
        Edit::Duration(Minutes::new(3 * DAY)),
        3 * DAY,
        24 * HOUR,
        Recalculated::Work,
    );
}

#[test]
fn r189_fixed_units_work_changes_duration() {
    // 24 h à 100 % : d = 1 440 (3 j).
    let (side, parts) = one(FixedUnits);
    check(
        &side,
        &parts,
        Edit::Work(work(24 * HOUR)),
        3 * DAY,
        24 * HOUR,
        Recalculated::Duration,
    );
}

#[test]
fn r189_fixed_duration_units_changes_work() {
    // 2 j gardés, 50 % : W = 960 × 5 000 / 10 000 = 480 (8 h).
    let (side, parts) = one(FixedDuration);
    let out = check(
        &side,
        &parts,
        set_units(5_000),
        2 * DAY,
        8 * HOUR,
        Recalculated::Work,
    );
    assert_eq!(out.parts, [part(1, 5_000, 480)]);
}

#[test]
fn r189_fixed_duration_duration_changes_work() {
    // 3 j à 100 % : W = 1 440 (24 h).
    let (side, parts) = one(FixedDuration);
    check(
        &side,
        &parts,
        Edit::Duration(Minutes::new(3 * DAY)),
        3 * DAY,
        24 * HOUR,
        Recalculated::Work,
    );
}

#[test]
fn r189_fixed_duration_work_changes_units() {
    // 24 h sur 2 j gardés : U = 1 440 × 10 000 / 960 = 15 000 (150 %).
    let (side, parts) = one(FixedDuration);
    let out = check(
        &side,
        &parts,
        Edit::Work(work(24 * HOUR)),
        2 * DAY,
        24 * HOUR,
        Recalculated::Units,
    );
    assert_eq!(out.parts, [part(1, 15_000, 1_440)]);
}

#[test]
fn r189_fixed_work_units_changes_duration() {
    // 16 h gardées, 200 % : d = 960 × 10 000 / 20 000 = 480 (1 j).
    let (side, parts) = one(FixedWork);
    check(
        &side,
        &parts,
        set_units(20_000),
        DAY,
        16 * HOUR,
        Recalculated::Duration,
    );
}

#[test]
fn r189_fixed_work_duration_changes_units() {
    // 16 h gardées sur 4 j : U = 960 × 10 000 / 1 920 = 5 000 (50 %).
    let (side, parts) = one(FixedWork);
    let out = check(
        &side,
        &parts,
        Edit::Duration(Minutes::new(4 * DAY)),
        4 * DAY,
        16 * HOUR,
        Recalculated::Units,
    );
    assert_eq!(out.parts, [part(1, 5_000, 960)]);
}

#[test]
fn r189_fixed_work_work_changes_duration() {
    // 24 h à 100 % : d = 1 440 (3 j).
    let (side, parts) = one(FixedWork);
    check(
        &side,
        &parts,
        Edit::Work(work(24 * HOUR)),
        3 * DAY,
        24 * HOUR,
        Recalculated::Duration,
    );
}

#[test]
fn r189_multi_assignment() {
    // Durée fixe, R1 100 % 16 h et R2 50 % 8 h sur 2 j ; durée → 3 j :
    // chaque w_a = 1 440 × u_a : 1 440 et 720, total 2 160 (36 h).
    let parts = [part(1, 10_000, 960), part(2, 5_000, 480)];
    let side = task(FixedDuration, false, 2 * DAY, &parts);
    let out = check(
        &side,
        &parts,
        Edit::Duration(Minutes::new(3 * DAY)),
        3 * DAY,
        36 * HOUR,
        Recalculated::Work,
    );
    assert_eq!(out.parts, [part(1, 10_000, 1_440), part(2, 5_000, 720)]);
    // Unités fixes, mêmes affectations, unités de R2 → 100 % : travail de R2
    // gardé (480), d_2 = 480 ; la durée reste le plus grand d_a (960, R1).
    let side = task(FixedUnits, false, 2 * DAY, &parts);
    let edit = Edit::Units {
        uid: uid(2),
        units: units(10_000),
    };
    check(&side, &parts, edit, 2 * DAY, 24 * HOUR, Recalculated::Duration);
}

#[test]
fn r189_type_change_recalculates_nothing() {
    // Unités fixes → durée fixe → travail fixe : état rendu tel quel, même
    // incohérent (durée 3 j, 16 h à 100 %).
    let parts = [part(2, 10_000, 960), part(1, 5_000, 300)];
    let side = task(FixedUnits, false, 3 * DAY, &parts);
    for t in [FixedDuration, FixedWork, FixedUnits] {
        let out = check(
            &side,
            &parts,
            Edit::Type(t),
            3 * DAY,
            1_260,
            Recalculated::Nothing,
        );
        assert_eq!(out.parts, [part(1, 5_000, 300), part(2, 10_000, 960)]);
    }
}
