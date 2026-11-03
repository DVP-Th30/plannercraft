//! R-191 — Retrait d'une ressource, tâche pilotée par l'effort : le travail
//! retiré va aux restantes au prorata de leurs unités ; unités fixes et
//! travail fixe : durée recalculée ; durée fixe : unités recalculées.
//! Retrait de la dernière : D gardée, W = 0.

use crate::r170_work_equals_duration_times_units::{DAY, HOUR, check, part, task, uid};
use plannercraft_core::TaskType::{FixedDuration, FixedUnits, FixedWork};
use plannercraft_core::resource::{Edit, Recalculated};

#[test]
fn r191_fu_remove() {
    // 2 j, R1 et R2 100 % à 16 h ; retrait de R2 : R1 32 h, d = 1 920 (4 j).
    let parts = [part(1, 10_000, 960), part(2, 10_000, 960)];
    let side = task(FixedUnits, true, 2 * DAY, &parts);
    let out = check(
        &side,
        &parts,
        Edit::Remove(uid(2)),
        4 * DAY,
        32 * HOUR,
        Recalculated::Duration,
    );
    assert_eq!(out.parts, [part(1, 10_000, 1_920)]);
}

#[test]
fn r191_fd_remove() {
    // Durée fixe, même cas : R1 32 h sur 2 j gardés, U = 1 920 × 10 000 / 960
    // = 20 000 (200 %).
    let parts = [part(1, 10_000, 960), part(2, 10_000, 960)];
    let side = task(FixedDuration, true, 2 * DAY, &parts);
    let out = check(
        &side,
        &parts,
        Edit::Remove(uid(2)),
        2 * DAY,
        32 * HOUR,
        Recalculated::Units,
    );
    assert_eq!(out.parts, [part(1, 20_000, 1_920)]);
}

#[test]
fn r191_prorata_of_units() {
    // Travail fixe, R1 100 % 8 h, R2 50 % 4 h, R3 50 % 4 h (1 j) ; retrait
    // de R1 : 480 réparties 240 / 240 ; R2 et R3 à 480 sur 50 % → 2 j.
    let parts = [part(1, 10_000, 480), part(2, 5_000, 240), part(3, 5_000, 240)];
    let side = task(FixedWork, true, DAY, &parts);
    let out = check(
        &side,
        &parts,
        Edit::Remove(uid(1)),
        2 * DAY,
        16 * HOUR,
        Recalculated::Duration,
    );
    assert_eq!(out.parts, [part(2, 5_000, 480), part(3, 5_000, 480)]);
}

#[test]
fn r191_last_removed() {
    // Seule affectation retirée : tâche sans affectation, D gardée, W = 0.
    let parts = [part(1, 10_000, 960)];
    let side = task(FixedUnits, true, 2 * DAY, &parts);
    let out = check(
        &side,
        &parts,
        Edit::Remove(uid(1)),
        2 * DAY,
        0,
        Recalculated::Work,
    );
    assert_eq!(out.parts, []);
}
