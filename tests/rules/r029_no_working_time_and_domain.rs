//! R-029 — Calendrier sans temps ouvré, sortie du domaine : jamais de
//! boucle, jamais de panique ; `NoWorkingTime` ou `OutOfDomain` avec sa
//! direction.

use plannercraft_core::calendar::{Calendar, DaySetting};
use plannercraft_core::{CalendarError, CalendarUid, Direction, Week};
use plannercraft_tests::{compile_one, exception, mins, moment, standard, working};

fn uid(v: u32) -> CalendarUid {
    CalendarUid::new(v)
}

fn idle(v: u32) -> Calendar {
    Calendar::new_base(
        uid(v),
        "Rien".to_owned(),
        Week::from_fn(|_| DaySetting::NonWorking),
    )
}

#[test]
fn r029_no_working_time() {
    let c = compile_one(idle(9));
    let t = moment("2026-10-19T08:00");
    let none = Err(CalendarError::NoWorkingTime { calendar: uid(9) });
    assert!(!c.has_working_time());
    // Lun 08:00 + 60 : impossible.
    assert_eq!(c.add_working(t, mins(60)), none);
    assert_eq!(c.add_working(t, mins(-60)), none);
    assert_eq!(c.next_working_moment(t), none);
    assert_eq!(c.previous_working_moment(t), none);
    // Aucun temps entre deux instants ; durée nulle : identité.
    assert_eq!(c.working_between(t, moment("2026-10-30T08:00")), Ok(mins(0)));
    assert_eq!(c.add_working(t, mins(0)), Ok(t));
}

#[test]
fn r029_end_of_domain() {
    let c = compile_one(standard());
    // Mer 2149-12-31 16:00 + 60 → 17:00, dernière minute ouvrée du domaine.
    assert_eq!(
        c.add_working(moment("2149-12-31T16:00"), mins(60)),
        Ok(moment("2149-12-31T17:00"))
    );
    // + 61 : au-delà de 2149.
    assert_eq!(
        c.add_working(moment("2149-12-31T16:00"), mins(61)),
        Err(CalendarError::OutOfDomain {
            calendar: uid(1),
            direction: Direction::After
        })
    );
    assert_eq!(
        c.next_working_moment(moment("2149-12-31T17:00")),
        Err(CalendarError::OutOfDomain {
            calendar: uid(1),
            direction: Direction::After
        })
    );
}

#[test]
fn r029_start_of_domain() {
    let c = compile_one(standard());
    // Lun 1984-01-02 09:00 − 60 → 08:00, première minute ouvrée du domaine.
    assert_eq!(
        c.add_working(moment("1984-01-02T09:00"), mins(-60)),
        Ok(moment("1984-01-02T08:00"))
    );
    // − 61 : avant 1984.
    assert_eq!(
        c.add_working(moment("1984-01-02T09:00"), mins(-61)),
        Err(CalendarError::OutOfDomain {
            calendar: uid(1),
            direction: Direction::Before
        })
    );
}

#[test]
fn r029_single_working_day() {
    // Semaine chômée ; seul le mar 01/12/2026 est ouvré [08:00,12:00) (240
    // minutes sur tout le domaine).
    let mut cal = idle(9);
    cal.add_exception(exception(
        "2026-12-01",
        "2026-12-01",
        working(&[("08:00", "12:00")]),
    ))
    .unwrap();
    let c = compile_one(cal);
    assert_eq!(
        c.add_working(moment("2026-12-01T08:00"), mins(240)),
        Ok(moment("2026-12-01T12:00"))
    );
    // 01/12 08:00 + 600 : il n'y a que 240 minutes → au-delà du domaine.
    assert_eq!(
        c.add_working(moment("2026-12-01T08:00"), mins(600)),
        Err(CalendarError::OutOfDomain {
            calendar: uid(9),
            direction: Direction::After
        })
    );
    // Depuis 1984 : la première minute ouvrée est le 01/12/2026 08:00.
    assert_eq!(
        c.next_working_moment(moment("1984-01-01T00:00")),
        Ok(moment("2026-12-01T08:00"))
    );
}
