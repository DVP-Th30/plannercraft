//! R-181 — Unités nulles, bornes de `d_a` : ressource de travail à U = 0 :
//! `d_a` = durée de la tâche, W gardé tel quel, `assignment.zero_units` ;
//! `d_a` hors du domaine des minutes : `assignment.duration_out_of_domain`,
//! la tâche suit R-069 (conversion impossible, jamais de panique).
//! Hypothèse.

use plannercraft_core::{Exclusion, Minutes, TaskOutcome, Work};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

use crate::r171_assignment_effective_calendar::{assign, assign_full, placed, resource};

#[test]
fn r181_zero_units() {
    // Tâche 2 j, R1 100 % 16 h, R2 0 % avec 3 h : R2 couvre la durée de la
    // tâche (lun – mar), garde ses 3 h ; la tâche reste lun – mar.
    let net = Net::new("2026-10-19T08:00").task("A", 960);
    let net = resource(resource(net, 1, None), 2, None);
    let net = assign(net, 1, "A", 1, 960);
    let run = assign_full(net, 2, "A", Some(2), (180, 0, 0)).run();
    assert_eq!(run.at("A").finish, moment("2026-10-20T17:00"));
    let r2 = placed(&run, 2);
    assert_eq!(r2.duration, Minutes::new(960));
    assert_eq!(r2.span.finish, moment("2026-10-20T17:00"));
    assert_eq!(r2.work, Work::new(Minutes::new(180)).unwrap());
    let codes = run.codes();
    assert_eq!(
        codes.iter().filter(|c| **c == "assignment.zero_units").count(),
        1,
        "{codes:?}"
    );
}

#[test]
fn r181_out_of_domain_no_panic() {
    // W = 1 666 666 666 h à 100 % : `d_a` hors du domaine des durées ⇒
    // diagnostic, tâche non planifiable (R-069), successeur dégradé.
    let net = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 480)
        .fs("A", "B");
    let run = assign_full(
        resource(net, 1, None),
        1,
        "A",
        Some(1),
        (99_999_999_960, 10_000, 0),
    )
    .run();
    assert!(run.codes().contains(&"assignment.duration_out_of_domain"));
    assert!(matches!(
        run.outcome("A"),
        TaskOutcome::Excluded {
            reason: Exclusion::Unschedulable,
            ..
        }
    ));
    assert!(run.at("B").degraded);
}
