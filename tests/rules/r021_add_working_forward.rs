//! R-021 — Fin d'une durée ouvrée positive : `add_working(t, d > 0)` = plus
//! petit instant u tel que d minutes ouvrées séparent t de u ; la fin tombe
//! en fin de plage, jamais au début de la plage suivante. Calendrier Standard.

use plannercraft_tests::{compile_one, mins, moment, standard};

fn add(from: &str, d: i64) -> plannercraft_core::Moment {
    compile_one(standard())
        .add_working(moment(from), mins(d))
        .unwrap()
}

#[test]
fn r021_one_day_ends_at_17_00() {
    // Lun 19/10 08:00 + 480 : 08-12 (240) + 13-17 (240) → lun 17:00, et non
    // mar 08:00 (S4 : Start 08:00 + PT16H → Finish le lendemain 17:00).
    assert_eq!(add("2026-10-19T08:00", 480), moment("2026-10-19T17:00"));
    // Exemple S4 : 16 h → le lendemain 17:00.
    assert_eq!(add("2026-10-19T08:00", 960), moment("2026-10-20T17:00"));
}

#[test]
fn r021_half_day_ends_at_12_00() {
    // Lun 08:00 + 240 → 12:00, et non 13:00.
    assert_eq!(add("2026-10-19T08:00", 240), moment("2026-10-19T12:00"));
    // + 300 : 240 le matin, 60 l'après-midi → 14:00.
    assert_eq!(add("2026-10-19T08:00", 300), moment("2026-10-19T14:00"));
    // + 720 : lundi 480, mardi matin 240 → mar 12:00.
    assert_eq!(add("2026-10-19T08:00", 720), moment("2026-10-20T12:00"));
}

#[test]
fn r021_crosses_nights_and_weekends() {
    // Lun 16:00 + 120 : lun 16-17 (60), mar 08-09 (60) → mar 09:00.
    assert_eq!(add("2026-10-19T16:00", 120), moment("2026-10-20T09:00"));
    // Ven 23/10 13:00 + 960 : ven 13-17 (240), lun 26 (480), mar 27 08-12
    // (240) → mar 27/10 12:00.
    assert_eq!(add("2026-10-23T13:00", 960), moment("2026-10-27T12:00"));
    // Sam 24/10 10:00 + 480 : rien le week-end, lun 26 complet → 17:00.
    assert_eq!(add("2026-10-24T10:00", 480), moment("2026-10-26T17:00"));
}
