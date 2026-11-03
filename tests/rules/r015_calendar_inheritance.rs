//! R-015 — Héritage à un niveau : un calendrier dérivé a une base, qui doit
//! exister et être un calendrier de base ; ses jours `Inherit` prennent la
//! valeur de la base, et une modification de la base se répercute.

use plannercraft_core::calendar::{Calendar, DayRule, chain, resolve_day};
use plannercraft_core::{CalendarError, CalendarUid, Weekday};
use plannercraft_tests::{calendars, day, day_minutes, hours, standard};

fn uid(v: u32) -> CalendarUid {
    CalendarUid::new(v)
}

/// Dérivé de Standard (UID 4) dont seul le vendredi vaut [08:00,12:00).
fn paul() -> Calendar {
    let mut c = Calendar::new_derived(uid(4), "Paul".to_owned(), uid(1));
    c.set_day(Weekday::Friday, DayRule::Working(hours(&[("08:00", "12:00")])))
        .unwrap();
    c
}

#[test]
fn r015_derived_inherits_undefined_days() {
    let a = calendars(vec![standard(), paul()]);
    // Ven 23/10/2026 : jour propre du dérivé, 08:00-12:00 = 240.
    assert_eq!(day_minutes(&a, 4, "2026-10-23"), 240);
    // Lun 19/10/2026 : hérité de Standard, 240 + 240 = 480.
    assert_eq!(day_minutes(&a, 4, "2026-10-19"), 480);
    // Sam 24/10/2026 : hérité, chômé.
    assert_eq!(day_minutes(&a, 4, "2026-10-24"), 0);
}

#[test]
fn r015_base_change_propagates() {
    // La base passe au lundi 09:00-12:00 et 13:00-17:00 : 180 + 240 = 420,
    // que le dérivé hérite sans être modifié.
    let mut base = standard();
    base.set_day(
        Weekday::Monday,
        DayRule::Working(hours(&[("09:00", "12:00"), ("13:00", "17:00")])),
    )
    .unwrap();
    let a = calendars(vec![base, paul()]);
    assert_eq!(day_minutes(&a, 4, "2026-10-19"), 420);
    assert_eq!(day_minutes(&a, 4, "2026-10-23"), 240);
}

#[test]
fn r015_inheritance_errors() {
    let derived = |v: u32, base: u32| Calendar::new_derived(uid(v), format!("D{v}"), uid(base));
    let a = calendars(vec![
        standard(),
        paul(),
        derived(5, 6),
        derived(6, 5),
        derived(7, 4),
        derived(8, 99),
    ]);
    // Dérivé fondé sur un dérivé (7 → 4 → 1).
    assert_eq!(
        chain(&a, uid(7)).unwrap_err(),
        CalendarError::BaseNotBase {
            calendar: uid(7),
            base: uid(4)
        }
    );
    // Boucle 5 → 6 → 5.
    assert_eq!(
        chain(&a, uid(5)).unwrap_err(),
        CalendarError::InheritanceCycle {
            chain: vec![uid(5), uid(6), uid(5)]
        }
    );
    // Base 99 inexistante.
    assert_eq!(
        resolve_day(&a, uid(8), day("2026-10-19")).unwrap_err(),
        CalendarError::BaseMissing {
            calendar: uid(8),
            base: uid(99)
        }
    );
}

#[test]
fn r015_derived_conversion() {
    use plannercraft_tests::{compiled, mins, moment};
    let a = calendars(vec![standard(), paul()]);
    let c = compiled(&a, 4);
    // Ven 23/10 08:00 + 480 sur Paul : ven 08-12 (240), lun 26 08-12 (240)
    // → lun 26/10 12:00.
    assert_eq!(
        c.add_working(moment("2026-10-23T08:00"), mins(480)),
        Ok(moment("2026-10-26T12:00"))
    );
}
