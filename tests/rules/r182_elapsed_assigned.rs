//! R-182 — Tâche à durée écoulée affectée : calendriers ignorés, `d_a` en
//! minutes civiles (W ÷ U). Hypothèse.

use plannercraft_core::{EffectiveCalendar, Minutes};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

use crate::r171_assignment_effective_calendar::{STANDARD, assign, assign_full, placed, resource};

#[test]
fn r182_elapsed_ignores_calendars() {
    // 3 ej, R1 100 % (72 h), ressource sur le Standard ⇒ lun 08:00 → jeu
    // 08:00, affectation sur le temps civil.
    let net = Net::new("2026-10-19T08:00").elapsed("A", 4_320);
    let run = assign(resource(net, 1, Some(STANDARD)), 1, "A", 1, 4_320).run();
    assert_eq!(run.at("A").start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("A").finish, moment("2026-10-22T08:00"));
    let a = placed(&run, 1);
    assert_eq!(a.calendar, EffectiveCalendar::Elapsed);
    assert_eq!(a.span.finish, moment("2026-10-22T08:00"));
    assert_eq!(a.duration, Minutes::new(4_320));
}

#[test]
fn r182_elapsed_two_rates() {
    // R1 100 % 72 h (3 ej) et R2 50 % 72 h (6 ej) ⇒ fin dim 25/10 08:00
    // (temps civil), durée 8 640.
    let net = Net::new("2026-10-19T08:00").elapsed("A", 4_320);
    let net = resource(resource(net, 1, Some(STANDARD)), 2, Some(STANDARD));
    let net = assign(net, 1, "A", 1, 4_320);
    let run = assign_full(net, 2, "A", Some(2), (4_320, 5_000, 0)).run();
    assert_eq!(run.at("A").finish, moment("2026-10-25T08:00"));
    assert_eq!(placed(&run, 1).span.finish, moment("2026-10-22T08:00"));
    assert_eq!(placed(&run, 2).duration, Minutes::new(8_640));
}
