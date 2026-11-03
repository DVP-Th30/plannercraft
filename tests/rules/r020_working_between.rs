//! R-020 — Fonction cumulée et temps ouvré entre deux instants : W(t) =
//! minutes ouvrées dans [1984-01-01 00:00, t) ; `working_between(a, b)` =
//! W(b) − W(a), signé et additif. Calendrier Standard.

use plannercraft_tests::{compile_one, mins, moment, standard};

#[test]
fn r020_full_week() {
    let c = compile_one(standard());
    // Lun 19/10 08:00 → ven 23/10 17:00 : 5 jours × 480 = 2 400.
    assert_eq!(
        c.working_between(moment("2026-10-19T08:00"), moment("2026-10-23T17:00")),
        Ok(mins(2_400))
    );
}

#[test]
fn r020_lunch_break_is_not_working() {
    let c = compile_one(standard());
    // Lun 12:00 → 13:00 : pause, 0.
    assert_eq!(
        c.working_between(moment("2026-10-19T12:00"), moment("2026-10-19T13:00")),
        Ok(mins(0))
    );
}

#[test]
fn r020_signed_across_weekend() {
    let c = compile_one(standard());
    let a = moment("2026-10-23T15:00");
    let b = moment("2026-10-26T10:00");
    // Ven 15:00-17:00 (120) + week-end (0) + lun 08:00-10:00 (120) = 240.
    assert_eq!(c.working_between(a, b), Ok(mins(240)));
    // Dans l'autre sens : −240 (antisymétrie).
    assert_eq!(c.working_between(b, a), Ok(mins(-240)));
    // Ven 17:00 → lun 08:00 : rien d'ouvré.
    assert_eq!(
        c.working_between(moment("2026-10-23T17:00"), moment("2026-10-26T08:00")),
        Ok(mins(0))
    );
}

#[test]
fn r020_additive() {
    let c = compile_one(standard());
    let a = moment("2026-10-19T09:00");
    let b = moment("2026-10-21T15:30");
    let d = moment("2026-10-27T11:00");
    // a → b : lun 09-12 (180) + 13-17 (240) = 420, mar 480, mer 08-12 (240)
    // + 13-15:30 (150) = 390 ; total 1 290.
    assert_eq!(c.working_between(a, b), Ok(mins(1_290)));
    // b → d : mer 15:30-17:00 (90), jeu et ven 960, lun 480, mar 08-11 (180)
    // = 1 710 ; a → d = 1 290 + 1 710 = 3 000.
    assert_eq!(c.working_between(b, d), Ok(mins(1_710)));
    assert_eq!(c.working_between(a, d), Ok(mins(3_000)));
}
