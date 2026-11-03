//! R-184 — Cumuls du travail : feuille = Σ W de ses affectations de
//! ressources de travail ; récapitulative = Σ de ses enfants + ses propres
//! affectations de travail ; UID 0 = Σ des tâches de premier niveau ;
//! ressource = Σ W de ses affectations des tâches actives. L'affectation
//! fictive (sans ressource), les ressources matérielles et de coût ne font
//! pas le travail d'une tâche. Source : O25 (A4 UID 4 : 40 h + 20 h), O28
//! (A6 UID 33 : 352 + 200 + 240 h, matériel R10 exclu).

use plannercraft_core::{ResourceKind, ResourceUid, Work};
use plannercraft_tests::net::Net;

use crate::r171_assignment_effective_calendar::{assign, assign_full, resource, resource_kind};

/// Travail de `m` minutes.
pub(crate) fn work(m: i64) -> Work {
    Work::new(plannercraft_core::Minutes::new(m)).unwrap()
}

#[test]
fn r184_leaf_sum() {
    // A4 UID 4 : tâche de 5 j, R1 40 h à 100 %, R2 20 h à 50 % ⇒ 60 h.
    let net = Net::new("2026-10-19T08:00").task("A", 2_400);
    let net = resource(resource(net, 1, None), 2, None);
    let net = assign(net, 1, "A", 1, 2_400);
    let run = assign_full(net, 2, "A", Some(2), (1_200, 5_000, 0)).run();
    assert_eq!(run.at("A").work, work(3_600));
    assert_eq!(run.schedule.project_summary.unwrap().work, work(3_600));
    let r1 = &run.schedule.resources[&ResourceUid::new(1)];
    let r2 = &run.schedule.resources[&ResourceUid::new(2)];
    assert_eq!((r1.work, r2.work), (work(2_400), work(1_200)));
}

#[test]
fn r184_summary_includes_own_assignments() {
    // Comme A6 UID 33 : enfants 8 h + 8 h, la récapitulative porte R3 20 h
    // et une ressource matérielle (quantité 3) ⇒ 36 h, matériel exclu.
    let net = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .task("B", 480)
        .level("B", 2);
    let net = resource(resource(resource(net, 1, None), 2, None), 3, None);
    let net = resource_kind(
        net,
        10,
        None,
        ResourceKind::Material {
            label: "m3".to_owned(),
        },
    );
    let net = assign(assign(net, 1, "A", 1, 480), 2, "B", 2, 480);
    let net = assign(net, 3, "S", 3, 1_200);
    let run = assign(net, 4, "S", 10, 180).run();
    assert_eq!(run.at("A").work, work(480));
    assert_eq!(run.at("S").work, work(2_160));
    assert_eq!(run.schedule.project_summary.unwrap().work, work(2_160));
    assert_eq!(run.schedule.resources[&ResourceUid::new(3)].work, work(1_200));
}

#[test]
fn r184_placeholder_excluded() {
    // Affectation fictive (−65 535, O24) : travail = durée, travail de la
    // tâche nul ; la ressource UID 0 n'a pas de charge.
    let net = Net::new("2026-10-19T08:00").task("A", 480);
    let net = resource(net, 0, None);
    let run = assign_full(net, 1, "A", None, (480, 10_000, 0)).run();
    assert_eq!(run.at("A").work, Work::ZERO);
    assert!(run.schedule.resources.is_empty());
}
