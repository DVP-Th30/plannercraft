//! R-027 — Week-ends et jours fériés sautés : le temps chômé ne consomme pas
//! de durée (conséquence de R-020 et R-021, testée à part).

use plannercraft_core::calendar::DaySetting;
use plannercraft_tests::{compile_one, exception, mins, moment, standard};

#[test]
fn r027_holiday_wednesday() {
    // Férié le mer 11/11/2026.
    let mut cal = standard();
    cal.add_exception(exception("2026-11-11", "2026-11-11", DaySetting::NonWorking))
        .unwrap();
    let c = compile_one(cal);
    // Lun 09/11 08:00 + 5 j (2 400) : lun, mar, jeu, ven (4 × 480 = 1 920),
    // le week-end, puis lun 16/11 (480) → lun 16/11 17:00.
    let start = moment("2026-11-09T08:00");
    let end = moment("2026-11-16T17:00");
    assert_eq!(c.add_working(start, mins(2_400)), Ok(end));
    assert_eq!(c.working_between(start, end), Ok(mins(2_400)));
    // Lun 09/11 08:00 → ven 13/11 17:00 : 4 jours ouvrés = 1 920.
    assert_eq!(
        c.working_between(start, moment("2026-11-13T17:00")),
        Ok(mins(1_920))
    );
    // Ven 06/11 13:00 + 3 j (1 440) : ven 240, lun 480, mar 480, (mer
    // férié), jeu 08-12 240 → jeu 12/11 12:00.
    assert_eq!(
        c.add_working(moment("2026-11-06T13:00"), mins(1_440)),
        Ok(moment("2026-11-12T12:00"))
    );
}

#[test]
fn r027_holiday_friday() {
    // Férié le ven 25/12/2026 : jeu 24/12 08:00 + 2 j (960) : jeudi 480,
    // (vendredi férié, week-end), lun 28/12 480 → lun 28/12 17:00.
    let mut cal = standard();
    cal.add_exception(exception("2026-12-25", "2026-12-25", DaySetting::NonWorking))
        .unwrap();
    assert_eq!(
        compile_one(cal).add_working(moment("2026-12-24T08:00"), mins(960)),
        Ok(moment("2026-12-28T17:00"))
    );
}
