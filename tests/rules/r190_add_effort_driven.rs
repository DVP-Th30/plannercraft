//! R-190 — Ajout d'une ressource, tâche pilotée par l'effort (S42, S43) :
//! première affectation exemptée (W = D × U, D gardée) ; ensuite W total
//! gardé ; unités fixes et travail fixe : D = W × 10 000 / Σ U ; durée
//! fixe : D gardée, travail au prorata des unités, unités recalculées.

use crate::r170_work_equals_duration_times_units::{DAY, HOUR, check, part, task, uid, units};
use plannercraft_core::TaskType::{FixedDuration, FixedUnits, FixedWork};
use plannercraft_core::resource::{Edit, Recalculated};

fn add(id: u32, u: i64) -> Edit {
    Edit::Add {
        uid: uid(id),
        units: units(u),
    }
}

#[test]
fn r190_first_assignment_exempt() {
    // Unités fixes pilotée, 4 j sans affectation ; + R1 50 % : D gardée
    // (1 920), W = 1 920 × 5 000 / 10 000 = 960 (16 h), pas de répartition.
    let side = task(FixedUnits, true, 4 * DAY, &[]);
    let out = check(&side, &[], add(1, 5_000), 4 * DAY, 16 * HOUR, Recalculated::Work);
    assert_eq!(out.parts, [part(1, 5_000, 960)]);
    // Travail fixe : même exemption.
    let side = task(FixedWork, true, 4 * DAY, &[]);
    check(&side, &[], add(1, 10_000), 4 * DAY, 32 * HOUR, Recalculated::Work);
}

#[test]
fn r190_fu_add_halves_duration() {
    // 4 j, R1 100 % 32 h ; + R2 100 % : D = 1 920 × 10 000 / 20 000 = 960
    // (2 j), 16 h chacune.
    let parts = [part(1, 10_000, 1_920)];
    let side = task(FixedUnits, true, 4 * DAY, &parts);
    let out = check(
        &side,
        &parts,
        add(2, 10_000),
        2 * DAY,
        32 * HOUR,
        Recalculated::Duration,
    );
    assert_eq!(out.parts, [part(1, 10_000, 960), part(2, 10_000, 960)]);
}

#[test]
fn r190_unequal_units() {
    // 3 j, R1 100 % 24 h ; + R2 50 % : D = 1 440 × 10 000 / 15 000 = 960
    // (2 j) ; w1 = 960 (16 h), w2 = 1 440 − 960 = 480 (8 h, reste).
    let parts = [part(1, 10_000, 1_440)];
    let side = task(FixedUnits, true, 3 * DAY, &parts);
    let out = check(
        &side,
        &parts,
        add(2, 5_000),
        2 * DAY,
        24 * HOUR,
        Recalculated::Duration,
    );
    assert_eq!(out.parts, [part(1, 10_000, 960), part(2, 5_000, 480)]);
}

#[test]
fn r190_fd_lowers_units() {
    // Durée fixe 4 j, R1 100 % 32 h ; + R2 100 % : D gardée ; 1 920 réparties
    // au prorata des unités demandées (960, 960) ; U = 960 × 10 000 / 1 920
    // = 5 000 chacune.
    let parts = [part(1, 10_000, 1_920)];
    let side = task(FixedDuration, true, 4 * DAY, &parts);
    let out = check(
        &side,
        &parts,
        add(2, 10_000),
        4 * DAY,
        32 * HOUR,
        Recalculated::Units,
    );
    assert_eq!(out.parts, [part(1, 5_000, 960), part(2, 5_000, 960)]);
}

#[test]
fn r190_s43_two_more_resources() {
    // S43 : 4 j avec 2 ressources (W = 3 840) ; + R3 : D = 3 840 / 3 = 1 280 ;
    // + R4 : D = 3 840 / 4 = 960 (2 j).
    let parts = [part(1, 10_000, 1_920), part(2, 10_000, 1_920)];
    let side = task(FixedUnits, true, 4 * DAY, &parts);
    let three = check(
        &side,
        &parts,
        add(3, 10_000),
        1_280,
        64 * HOUR,
        Recalculated::Duration,
    );
    let side = task(FixedUnits, true, 1_280, &three.parts);
    let four = check(
        &side,
        &three.parts,
        add(4, 10_000),
        2 * DAY,
        64 * HOUR,
        Recalculated::Duration,
    );
    assert!(four.parts.iter().all(|p| p.work.minutes().get() == 960));
}

#[test]
fn r190_sequential_equals_simultaneous() {
    // Ajouts successifs de R3 (100 %) puis R4 (100 %) = un ajout de 200 %
    // d'un coup (profil plat) : W 3 840 et Σ U 40 000 identiques, D 960.
    let parts = [part(1, 10_000, 1_920), part(2, 10_000, 1_920)];
    let side = task(FixedUnits, true, 4 * DAY, &parts);
    let three = check(
        &side,
        &parts,
        add(3, 10_000),
        1_280,
        3_840,
        Recalculated::Duration,
    );
    let side3 = task(FixedUnits, true, 1_280, &three.parts);
    let seq = check(
        &side3,
        &three.parts,
        add(4, 10_000),
        960,
        3_840,
        Recalculated::Duration,
    );
    let once = check(&side, &parts, add(3, 20_000), 960, 3_840, Recalculated::Duration);
    let sum_units = |ps: &[plannercraft_core::resource::Part]| ps.iter().map(|p| p.units.get()).sum::<i64>();
    assert_eq!(sum_units(&seq.parts), sum_units(&once.parts));
}

#[test]
fn r190_fixed_work_recorded_not_effort_driven_still_driven() {
    // Travail fixe relu avec `EffortDriven` 0 : traité comme piloté (R-193).
    let parts = [part(1, 10_000, 1_920)];
    let side = task(FixedWork, false, 4 * DAY, &parts);
    check(
        &side,
        &parts,
        add(2, 10_000),
        2 * DAY,
        32 * HOUR,
        Recalculated::Duration,
    );
}
