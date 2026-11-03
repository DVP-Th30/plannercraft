//! R-176 — Affectations neutres pour les dates : ressource absente
//! (affectation fictive −65 535), matérielle, de coût, ou inconnue
//! (`assignment.unknown_resource` à la validation) : la tâche garde son
//! placement sans affectation ; `AssignmentRole::Neutral`, dates de la tâche ;
//! travail exclu des cumuls (R-184). Source : O24 (A6 UID 24 : affectation
//! −65 535 de 8 h, `Work` de la tâche 0).

use plannercraft_core::{AssignmentRole, CalendarUid, EffectiveCalendar, ResourceKind, Work};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

use crate::r171_assignment_effective_calendar::{assign_full, placed, resource_kind};

#[test]
fn r176_placeholder_neutral() {
    // Tâche 2 j, affectation fictive de 8 h à 100 % : dates inchangées (lun
    // – mar), rôle neutre, travail de la tâche nul.
    let net = Net::new("2026-10-19T08:00").task("A", 960);
    let run = assign_full(net, 1, "A", None, (480, 10_000, 0)).run();
    assert_eq!(run.at("A").finish, moment("2026-10-20T17:00"));
    assert_eq!(run.at("A").work, Work::ZERO);
    let a = placed(&run, 1);
    assert_eq!(a.role, AssignmentRole::Neutral);
    assert_eq!(a.span, run.at("A").early);
    assert_eq!(a.resource, None);
    assert_eq!(a.calendar, EffectiveCalendar::Project(CalendarUid::new(1)));
}

#[test]
fn r176_material_cost_and_unknown_are_neutral() {
    // Matérielle (16 h = 16 unités), de coût, inconnue (UID 99) : aucune
    // ne place la tâche de 1 j.
    let net = Net::new("2026-10-19T08:00").task("A", 480);
    let net = resource_kind(
        net,
        1,
        None,
        ResourceKind::Material {
            label: "m³".to_owned(),
        },
    );
    let net = resource_kind(net, 2, None, ResourceKind::Cost);
    let net = assign_full(net, 1, "A", Some(1), (960, 10_000, 0));
    let net = assign_full(net, 2, "A", Some(2), (0, 10_000, 0));
    let run = assign_full(net, 3, "A", Some(99), (2_400, 10_000, 0)).run();
    assert_eq!(run.at("A").finish, moment("2026-10-19T17:00"));
    for uid in 1..=3 {
        assert_eq!(placed(&run, uid).role, AssignmentRole::Neutral, "{uid}");
        assert_eq!(placed(&run, uid).span, run.at("A").early, "{uid}");
    }
}
