//! R-016 — Précédence de résolution d'un jour, jour de semaine par jour de
//! semaine (`Inherit` passe au niveau suivant) : exception du dérivé >
//! exception de la base > semaine datée du dérivé > semaine par défaut du
//! dérivé > semaine datée de la base > semaine par défaut de la base.

use plannercraft_core::calendar::{Calendar, DayRule, DaySetting, DaySource, WorkWeek, resolve_day};
use plannercraft_core::{CalendarUid, Week, Weekday};
use plannercraft_tests::{calendars, day, day_minutes, days, exception, hours, standard, working};

fn uid(v: u32) -> CalendarUid {
    CalendarUid::new(v)
}

/// Standard avec Noël 2026 chômé.
fn base_with_christmas() -> Calendar {
    let mut c = standard();
    c.add_exception(exception("2026-12-25", "2026-12-25", DaySetting::NonWorking))
        .unwrap();
    c
}

/// « Paul » (UID 4), dérivé de la base : vendredi [08:00,12:00).
fn paul() -> Calendar {
    let mut c = Calendar::new_derived(uid(4), "Paul".to_owned(), uid(1));
    c.set_day(Weekday::Friday, DayRule::Working(hours(&[("08:00", "12:00")])))
        .unwrap();
    c
}

#[test]
fn r016_base_exception_beats_derived_day() {
    let a = calendars(vec![base_with_christmas(), paul()]);
    // Ven 18/12 : vendredi de Paul, 240.
    assert_eq!(day_minutes(&a, 4, "2026-12-18"), 240);
    // Ven 25/12 : l'exception de la base l'emporte sur le vendredi de Paul : 0.
    assert_eq!(day_minutes(&a, 4, "2026-12-25"), 0);
    let r = resolve_day(&a, uid(4), day("2026-12-25")).unwrap();
    assert_eq!(
        r.source,
        DaySource::Exception {
            calendar: uid(1),
            index: 0
        }
    );
    // Lun 21/12 : hérité de Standard, 480.
    assert_eq!(day_minutes(&a, 4, "2026-12-21"), 480);
}

#[test]
fn r016_derived_exception_beats_base_exception() {
    // Paul a sa propre exception le 25/12, ouvrée [09:00,11:00) : 120.
    let mut p = paul();
    p.add_exception(exception(
        "2026-12-25",
        "2026-12-25",
        working(&[("09:00", "11:00")]),
    ))
    .unwrap();
    let a = calendars(vec![base_with_christmas(), p]);
    assert_eq!(day_minutes(&a, 4, "2026-12-25"), 120);
}

#[test]
fn r016_derived_default_week_beats_base_work_week() {
    // Semaine datée de la base du 01/07/2027 au 31/08/2027 : lun-ven
    // [08:00,15:00) = 420 ; week-end hérité (chômé).
    let mut base = standard();
    base.add_work_week(WorkWeek {
        name: "Été".to_owned(),
        period: days("2027-07-01", "2027-08-31"),
        days: Week::from_fn(|d| match d {
            Weekday::Saturday | Weekday::Sunday => DayRule::Inherit,
            _ => DayRule::Working(hours(&[("08:00", "15:00")])),
        }),
    })
    .unwrap();
    let a = calendars(vec![base, paul()]);
    // Ven 02/07/2027 : le vendredi par défaut de Paul (240) l'emporte sur la
    // semaine datée de la base (420).
    assert_eq!(day_minutes(&a, 4, "2027-07-02"), 240);
    // Lun 05/07/2027 : Paul hérite → semaine datée de la base, 420.
    assert_eq!(day_minutes(&a, 4, "2027-07-05"), 420);
    // La base elle-même : 420 le vendredi.
    assert_eq!(day_minutes(&a, 1, "2027-07-02"), 420);
}

#[test]
fn r016_derived_conversion_skips_base_holiday() {
    use plannercraft_tests::{compiled, mins, moment};
    let a = calendars(vec![base_with_christmas(), paul()]);
    let c = compiled(&a, 4);
    // Jeu 24/12 08:00 + 720 sur Paul : jeudi 480, ven 25/12 férié (exception
    // de la base), lun 28/12 08-12 (240) → lun 28/12 12:00.
    assert_eq!(
        c.add_working(moment("2026-12-24T08:00"), mins(720)),
        Ok(moment("2026-12-28T12:00"))
    );
}
