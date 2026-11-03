//! R-024 — Instant ouvré suivant : `next_working_moment(t)` = plus petit
//! u >= t dont la minute [u, u+1) est ouvrée ; idempotent. Calendrier
//! Standard.

use plannercraft_core::calendar::DaySetting;
use plannercraft_tests::{compile_one, exception, moment, standard};

fn next(t: &str) -> plannercraft_core::Moment {
    compile_one(standard()).next_working_moment(moment(t)).unwrap()
}

#[test]
fn r024_aligns_on_next_range() {
    // Sam 24/10 10:00 → lun 26/10 08:00.
    assert_eq!(next("2026-10-24T10:00"), moment("2026-10-26T08:00"));
    // Lun 12:00 (début de la pause) → 13:00.
    assert_eq!(next("2026-10-19T12:00"), moment("2026-10-19T13:00"));
    // Lun 17:00 → mar 08:00.
    assert_eq!(next("2026-10-19T17:00"), moment("2026-10-20T08:00"));
    // Lun 07:00 → 08:00 (S9 : début à 7:00 sur un calendrier à 8:00).
    assert_eq!(next("2026-10-19T07:00"), moment("2026-10-19T08:00"));
}

#[test]
fn r024_inside_a_range_is_unchanged_and_idempotent() {
    // Lun 09:30, dans une plage : inchangé.
    assert_eq!(next("2026-10-19T09:30"), moment("2026-10-19T09:30"));
    let c = compile_one(standard());
    let once = c.next_working_moment(moment("2026-10-24T10:00")).unwrap();
    assert_eq!(c.next_working_moment(once), Ok(once));
}

#[test]
fn r024_skips_holiday() {
    let mut cal = standard();
    cal.add_exception(exception("2026-11-11", "2026-11-11", DaySetting::NonWorking))
        .unwrap();
    // Mer 11/11 10:00, férié → jeu 12/11 08:00.
    assert_eq!(
        compile_one(cal).next_working_moment(moment("2026-11-11T10:00")),
        Ok(moment("2026-11-12T08:00"))
    );
}
