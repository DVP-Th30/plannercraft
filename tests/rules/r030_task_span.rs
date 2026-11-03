//! R-030 — Placement d'une durée > 0. Passe avant : début = instant ouvré
//! suivant la date au plus tôt, fin = début + durée (R-021). Passe arrière :
//! fin = instant ouvré précédant la date au plus tard, début = fin − durée
//! (R-022). Calendrier Standard.

use plannercraft_core::calendar::CompiledCalendar;
use plannercraft_core::{Moment, TimeScale};
use plannercraft_tests::{compile_one, mins, moment, standard};

fn cal() -> CompiledCalendar {
    compile_one(standard())
}

fn forward(c: &CompiledCalendar, from: &str, d: i64) -> (Moment, Moment) {
    let s = TimeScale::Working(c).forward_span(moment(from), mins(d)).unwrap();
    (s.start, s.finish)
}

fn backward(c: &CompiledCalendar, to: &str, d: i64) -> (Moment, Moment) {
    let s = TimeScale::Working(c).backward_span(moment(to), mins(d)).unwrap();
    (s.start, s.finish)
}

#[test]
fn r030_forward_span_aligns_start() {
    let c = cal();
    // Lun 12:00 (pause), 240 : début 13:00, fin 17:00.
    assert_eq!(
        forward(&c, "2026-10-19T12:00", 240),
        (moment("2026-10-19T13:00"), moment("2026-10-19T17:00"))
    );
    // Ven 23/10 17:00, 480 : début lun 26/10 08:00, fin 17:00.
    assert_eq!(
        forward(&c, "2026-10-23T17:00", 480),
        (moment("2026-10-26T08:00"), moment("2026-10-26T17:00"))
    );
    // Sam 24/10 10:00, 480 : début lun 08:00, fin 17:00.
    assert_eq!(
        forward(&c, "2026-10-24T10:00", 480),
        (moment("2026-10-26T08:00"), moment("2026-10-26T17:00"))
    );
}

#[test]
fn r030_backward_span_aligns_finish() {
    let c = cal();
    // Au plus tard lun 26/10 08:00, 480 : fin ven 23/10 17:00, début ven 08:00.
    assert_eq!(
        backward(&c, "2026-10-26T08:00", 480),
        (moment("2026-10-23T08:00"), moment("2026-10-23T17:00"))
    );
    // Au plus tard lun 13:00, 240 : fin lun 12:00, début lun 08:00.
    assert_eq!(
        backward(&c, "2026-10-19T13:00", 240),
        (moment("2026-10-19T08:00"), moment("2026-10-19T12:00"))
    );
}

#[test]
fn r030_elapsed_span_out_of_domain() {
    use plannercraft_core::{CalendarError, Direction};
    // Temps écoulé : fin au-delà de 2150-01-01 00:00 (passe avant) ou début
    // avant 1984-01-01 00:00 (passe arrière) → `ElapsedOutOfDomain`.
    let e = TimeScale::Elapsed;
    assert_eq!(
        e.forward_span(moment("2149-12-31T23:00"), mins(61)),
        Err(CalendarError::ElapsedOutOfDomain {
            direction: Direction::After
        })
    );
    assert_eq!(
        e.backward_span(moment("1984-01-01T01:00"), mins(61)),
        Err(CalendarError::ElapsedOutOfDomain {
            direction: Direction::Before
        })
    );
    let s = e.backward_span(moment("1984-01-01T01:00"), mins(60)).unwrap();
    assert_eq!(
        (s.start, s.finish),
        (moment("1984-01-01T00:00"), moment("1984-01-01T01:00"))
    );
}
