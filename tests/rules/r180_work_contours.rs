//! R-180 — Profils de travail non uniformes : placés comme uniformes,
//! `assignment.contour_unsupported` (info, une fois par affectation) ; le
//! profil est gardé pour l'aller-retour. Hypothèse.

use plannercraft_core::{AssignmentUid, WorkContour};
use plannercraft_tests::net::{Net, Run};

use crate::r171_assignment_effective_calendar::{assign, placed, resource};

/// Tâche 2 j, R1 16 h de profil `contour`.
fn run(contour: WorkContour) -> Run {
    let net = assign(
        resource(Net::new("2026-10-19T08:00").task("A", 960), 1, None),
        1,
        "A",
        1,
        960,
    );
    net.project(|p| {
        if let Some(a) = p.assignments.get_mut(AssignmentUid::new(1)) {
            a.contour = contour;
        }
    })
    .run()
}

#[test]
fn r180_bell_scheduled_flat() {
    // En cloche : mêmes dates qu'uniforme, un seul diagnostic, profil gardé.
    let (flat, bell) = (run(WorkContour::Flat), run(WorkContour::Bell));
    assert_eq!(placed(&bell, 1).span, placed(&flat, 1).span);
    assert_eq!(bell.at("A"), flat.at("A"));
    let codes = bell.codes();
    assert_eq!(
        codes
            .iter()
            .filter(|c| **c == "assignment.contour_unsupported")
            .count(),
        1,
        "{codes:?}"
    );
    assert!(!flat.codes().contains(&"assignment.contour_unsupported"));
}
