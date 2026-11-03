//! R-023 — Durée nulle : `add_working(t, 0) = t`, sans recalage, quel que soit
//! t (le placement d'un jalon relève de R-031). Calendrier Standard.

use plannercraft_core::calendar::{Calendar, DaySetting};
use plannercraft_core::{CalendarUid, Week};
use plannercraft_tests::{compile_one, mins, moment, standard};

#[test]
fn r023_zero_is_identity() {
    let c = compile_one(standard());
    // Lun 17:00 (fin de plage) : inchangé.
    let t = moment("2026-10-19T17:00");
    assert_eq!(c.add_working(t, mins(0)), Ok(t));
    // Sam 10:00 (chômé) : inchangé, pas de recalage au lundi.
    let t = moment("2026-10-24T10:00");
    assert_eq!(c.add_working(t, mins(0)), Ok(t));
    // Pause de midi : inchangé.
    let t = moment("2026-10-19T12:30");
    assert_eq!(c.add_working(t, mins(0)), Ok(t));
}

#[test]
fn r023_zero_on_calendar_without_working_time() {
    // Même sans aucun temps ouvré, une durée nulle ne consomme rien.
    let c = compile_one(Calendar::new_base(
        CalendarUid::new(9),
        "Rien".to_owned(),
        Week::from_fn(|_| DaySetting::NonWorking),
    ));
    let t = moment("2026-10-19T08:00");
    assert_eq!(c.add_working(t, mins(0)), Ok(t));
}
