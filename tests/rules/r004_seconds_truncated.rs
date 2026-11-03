//! R-004 — Secondes tronquées à l'import, refusées à la saisie stricte.

use chrono::NaiveDateTime;
use plannercraft_core::{Moment, TimeError};
use plannercraft_tests::moment;

fn naive(s: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").unwrap()
}

#[test]
fn r004_floor_truncates_and_reports() {
    // 08:00:30 et 08:00:59 → 08:00 (plancher), troncature signalée.
    assert_eq!(
        Moment::from_naive_floor(naive("2026-10-19T08:00:30")),
        Ok((moment("2026-10-19T08:00"), true))
    );
    assert_eq!(
        Moment::from_naive_floor(naive("2026-10-19T08:00:59")),
        Ok((moment("2026-10-19T08:00"), true))
    );
    // 08:00:00 → 08:00 sans signal.
    assert_eq!(
        Moment::from_naive_floor(naive("2026-10-19T08:00:00")),
        Ok((moment("2026-10-19T08:00"), false))
    );
}

#[test]
fn r004_strict_input_refuses_seconds() {
    assert_eq!(
        Moment::from_naive(naive("2026-10-19T08:00:30")),
        Err(TimeError::NonZeroSeconds)
    );
    assert_eq!(
        Moment::from_naive(naive("2026-10-19T08:00:00")),
        Ok(moment("2026-10-19T08:00"))
    );
}
