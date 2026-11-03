//! R-192 — Sans pilotage par l'effort : ajout ou retrait gardent D ; le
//! travail varie de D × u (S42).

use crate::r170_work_equals_duration_times_units::{DAY, HOUR, check, part, task, uid, units};
use plannercraft_core::TaskType::{FixedDuration, FixedUnits};
use plannercraft_core::resource::{Edit, Recalculated};

#[test]
fn r192_add_keeps_duration() {
    // Unités fixes 4 j, R1 100 % 32 h ; + R2 100 % : 4 j, W 64 h.
    let parts = [part(1, 10_000, 1_920)];
    let side = task(FixedUnits, false, 4 * DAY, &parts);
    let add = Edit::Add {
        uid: uid(2),
        units: units(10_000),
    };
    let out = check(&side, &parts, add, 4 * DAY, 64 * HOUR, Recalculated::Work);
    assert_eq!(out.parts, [part(1, 10_000, 1_920), part(2, 10_000, 1_920)]);
    // Retrait de R2 : retour à 32 h, 4 j.
    let side = task(FixedUnits, false, 4 * DAY, &out.parts);
    let back = check(
        &side,
        &out.parts,
        Edit::Remove(uid(2)),
        4 * DAY,
        32 * HOUR,
        Recalculated::Work,
    );
    assert_eq!(back.parts, parts);
}

#[test]
fn r192_fixed_duration_add() {
    // Durée fixe 2 j non pilotée, R1 100 % ; + R2 50 % : D gardée, w2 = 480.
    let parts = [part(1, 10_000, 960)];
    let side = task(FixedDuration, false, 2 * DAY, &parts);
    let add = Edit::Add {
        uid: uid(2),
        units: units(5_000),
    };
    check(&side, &parts, add, 2 * DAY, 24 * HOUR, Recalculated::Work);
}
