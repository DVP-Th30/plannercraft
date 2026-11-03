//! R-177 — Délai d'affectation (`Delay`) : compté en temps ouvré de
//! l'échelle effective de l'affectation depuis le début au plus tôt ; il ne
//! décale pas le début de la tâche (la tâche garde sa durée, S45). Source :
//! S45 ; O20 (lecture en dixièmes de minute).

use plannercraft_core::Minutes;
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

use crate::r171_assignment_effective_calendar::{assign, assign_full, placed, resource};

#[test]
fn r177_delay_does_not_move_task() {
    // Tâche 5 j, R1 5 j, R2 1 j avec délai 4 j ⇒ R2 ven 23/10 ; tâche lun
    // 19/10 08:00 – ven 23/10 17:00, durée 2 400.
    let net = Net::new("2026-10-19T08:00").task("A", 2_400);
    let net = resource(resource(net, 1, None), 2, None);
    let net = assign(net, 1, "A", 1, 2_400);
    let run = assign_full(net, 2, "A", Some(2), (480, 10_000, 1_920)).run();
    let a = run.at("A");
    assert_eq!(a.start, moment("2026-10-19T08:00"));
    assert_eq!(a.finish, moment("2026-10-23T17:00"));
    assert_eq!(a.duration, Minutes::new(2_400));
    assert_eq!(placed(&run, 2).span.start, moment("2026-10-23T08:00"));
}

#[test]
fn r177_single_delayed_assignment() {
    // Une seule affectation, 1 j décalé de 1 j : la tâche commence lun
    // 08:00 (S45) et finit mar 17:00 ; durée calculée = 480 (temps
    // travaillé), affectation mar 08:00 – 17:00.
    let net = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 480)
        .fs("A", "B");
    let run = assign_full(resource(net, 1, None), 1, "A", Some(1), (480, 10_000, 480)).run();
    assert_eq!(run.at("A").start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("A").finish, moment("2026-10-20T17:00"));
    assert_eq!(run.at("A").duration, Minutes::new(480));
    assert_eq!(placed(&run, 1).span.start, moment("2026-10-20T08:00"));
    assert_eq!(run.at("B").start, moment("2026-10-21T08:00"));
}
