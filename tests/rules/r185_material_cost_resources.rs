//! R-185 — Ressources matérielles et de coût : la quantité d'une
//! ressource matérielle est son travail en heures (PT3H = 3, O30), sans
//! effet sur les dates ni sur le travail de la tâche, hors surutilisation ;
//! une ressource de coût n'a ni travail ni unités. Source : S5 ; O30.

use plannercraft_core::{ResourceKind, ResourceUid, Units, Work};
use plannercraft_tests::net::Net;

use crate::r171_assignment_effective_calendar::{assign, assign_full, resource, resource_kind};
use crate::r184_work_rollups::work;

#[test]
fn r185_material_quantity_no_effect() {
    let build = || {
        let net = Net::new("2026-10-19T08:00").task("A", 960).task("B", 960);
        let net = resource_kind(
            resource(net, 1, None),
            7,
            None,
            ResourceKind::Material {
                label: "t".to_owned(),
            },
        );
        resource_kind(net, 8, None, ResourceKind::Cost)
    };
    let alone = assign(build(), 1, "A", 1, 960).run();
    // Quantité 3 (180 min) sur A, 10 000 sur B en parallèle, coût sur A.
    let net = assign(build(), 1, "A", 1, 960);
    let net = assign_full(net, 2, "A", Some(7), (180, 10_000, 0));
    let net = assign_full(net, 3, "B", Some(7), (600_000, 10_000, 0));
    let run = assign_full(net, 4, "A", Some(8), (0, 10_000, 0)).run();
    assert_eq!(run.at("A").start, alone.at("A").start);
    assert_eq!(run.at("A").finish, alone.at("A").finish);
    assert_eq!(run.at("A").work, work(960));
    assert_eq!(run.at("B").work, Work::ZERO);
    let m = &run.schedule.resources[&ResourceUid::new(7)];
    assert_eq!(m.work, work(600_180));
    assert!(!m.overallocated && m.overloads.is_empty());
    assert_eq!(m.peak, Units::ZERO);
    let c = &run.schedule.resources[&ResourceUid::new(8)];
    assert_eq!(
        (c.work, c.peak, c.overallocated),
        (Work::ZERO, Units::ZERO, false)
    );
    assert!(!run.codes().contains(&"resource.overallocated"));
}
