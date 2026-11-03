//! R-194 — Travail saisi sur plusieurs affectations : réparti au prorata du
//! travail existant, à défaut des unités ; reste à la dernière par UID ;
//! puis la grandeur du type est recalculée.

use crate::r170_work_equals_duration_times_units::{DAY, HOUR, check, part, task, work};
use plannercraft_core::TaskType::{FixedDuration, FixedUnits};
use plannercraft_core::resource::{Edit, Recalculated};

#[test]
fn r194_prorata_and_remainder() {
    // Unités fixes 2 j, R1 100 % 16 h, R2 50 % 8 h ; travail → 48 h (2 880) :
    // prorata 960 : 480 → 1 920 et 960 ; d1 = 1 920, d2 = 960 × 10 000 /
    // 5 000 = 1 920 → D 4 j.
    let parts = [part(1, 10_000, 960), part(2, 5_000, 480)];
    let side = task(FixedUnits, false, 2 * DAY, &parts);
    let out = check(
        &side,
        &parts,
        Edit::Work(work(48 * HOUR)),
        4 * DAY,
        48 * HOUR,
        Recalculated::Duration,
    );
    assert_eq!(out.parts, [part(1, 10_000, 1_920), part(2, 5_000, 960)]);
    // 7 h (420) sur trois affectations égales : 140 chacune.
    let three = [part(1, 10_000, 60), part(2, 10_000, 60), part(3, 10_000, 60)];
    let side = task(FixedUnits, false, 60, &three);
    let out = check(
        &side,
        &three,
        Edit::Work(work(7 * HOUR)),
        140,
        7 * HOUR,
        Recalculated::Duration,
    );
    assert_eq!(
        out.parts,
        [part(1, 10_000, 140), part(2, 10_000, 140), part(3, 10_000, 140)]
    );
    // 10 min sur trois égales : 3,33 → 3, 3, reste 4 à la dernière (UID 3) ;
    // durée = plus grand d_a = 4.
    let out = check(&side, &three, Edit::Work(work(10)), 4, 10, Recalculated::Duration);
    assert_eq!(
        out.parts,
        [part(1, 10_000, 3), part(2, 10_000, 3), part(3, 10_000, 4)]
    );
}

#[test]
fn r194_units_when_no_work() {
    // Durée fixe 1 j, travail existant nul : prorata des unités. R1 100 %,
    // R2 50 % ; travail → 12 h (720) : 480 et 240 ; U = w × 10 000 / 480
    // → 10 000 et 5 000.
    let parts = [part(1, 10_000, 0), part(2, 5_000, 0)];
    let side = task(FixedDuration, false, DAY, &parts);
    let out = check(
        &side,
        &parts,
        Edit::Work(work(12 * HOUR)),
        DAY,
        12 * HOUR,
        Recalculated::Units,
    );
    assert_eq!(out.parts, [part(1, 10_000, 480), part(2, 5_000, 240)]);
}
