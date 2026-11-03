//! R-022 — Décalage négatif : `add_working(t, −d)` = plus grand instant s
//! tel que d minutes ouvrées séparent s de t ; le résultat tombe au début
//! d'une minute ouvrée (convention de début). Calendrier Standard.

use plannercraft_tests::{compile_one, mins, moment, standard};

fn sub(from: &str, d: i64) -> plannercraft_core::Moment {
    compile_one(standard())
        .add_working(moment(from), mins(-d))
        .unwrap()
}

#[test]
fn r022_one_day_back() {
    // Mer 21/10 17:00 − 480 → mer 08:00 (la journée entière).
    assert_eq!(sub("2026-10-21T17:00", 480), moment("2026-10-21T08:00"));
    // Mer 21/10 10:00 − 480 : mer 08-10 (120), mar 13-17 (240), mar 10-12
    // (120) → mar 20/10 10:00.
    assert_eq!(sub("2026-10-21T10:00", 480), moment("2026-10-20T10:00"));
}

#[test]
fn r022_lands_on_start_of_working_minute() {
    // Lun 26/10 08:00 − 60 : ven 16:00-17:00 → ven 23/10 16:00.
    assert_eq!(sub("2026-10-26T08:00", 60), moment("2026-10-23T16:00"));
    // Lun 26/10 09:00 − 60 : lun 08:00-09:00 → lun 08:00, et non ven 17:00.
    assert_eq!(sub("2026-10-26T09:00", 60), moment("2026-10-26T08:00"));
    // Lun 09:00 − 120 : lun 08-09 (60) + ven 16-17 (60) → ven 16:00.
    assert_eq!(sub("2026-10-26T09:00", 120), moment("2026-10-23T16:00"));
    // Mar 20/10 12:00 − 240 → mar 08:00, et non lun 17:00.
    assert_eq!(sub("2026-10-20T12:00", 240), moment("2026-10-20T08:00"));
    // Lun 13:00 − 240 : la pause ne compte pas → lun 08:00.
    assert_eq!(sub("2026-10-19T13:00", 240), moment("2026-10-19T08:00"));
}
