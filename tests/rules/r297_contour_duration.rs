//! R-297 — Affectation de travail d'une feuille à profil non uniforme
//! (`WorkContour` 1 à 8) : sa part dure la durée de la tâche (comme une
//! tâche à durée fixe), au lieu de travail ÷ unités ; elle reste placée
//! comme une affectation uniforme (R-180). Hypothèse fondée sur les
//! fichiers `.mpp` de test publics (lot M2) : avec les affectations lues,
//! `mpp14timephasedsegments.mpp` recalcule 10 tâches sur 20 comme MS
//! Project par travail ÷ unités, 19 avec la durée de la tâche.

use plannercraft_core::{AssignmentUid, Minutes, WorkContour};
use plannercraft_tests::moment;
use plannercraft_tests::net::{Net, Run};

use crate::r171_assignment_effective_calendar::{assign, placed, resource};

/// Tâche 3 j, R1 16 h de profil `contour`.
fn run(contour: WorkContour) -> Run {
    let net = assign(
        resource(Net::new("2026-10-19T08:00").task("A", 1_440), 1, None),
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
fn r297_contoured_part_lasts_the_task_duration() {
    // Uniforme : 16 h ÷ 100 % = 2 j, lun 08:00 – mar 17:00.
    let flat = run(WorkContour::Flat);
    assert_eq!(flat.at("A").early.finish, moment("2026-10-20T17:00"));
    // En cloche et profil modifié à la main : la durée de la tâche, 3 j.
    for contour in [WorkContour::Bell, WorkContour::Contoured] {
        let r = run(contour);
        assert_eq!(r.at("A").early.finish, moment("2026-10-21T17:00"));
        assert_eq!(placed(&r, 1).span.finish, moment("2026-10-21T17:00"));
        assert_eq!(r.at("A").duration, Minutes::new(1_440));
    }
}
