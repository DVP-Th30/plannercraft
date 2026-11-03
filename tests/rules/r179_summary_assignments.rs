//! R-179 — Affectation sur une récapitulative : départ au début planifié de
//! la récapitulative + délais, `d_a` = W ÷ U sur son calendrier effectif
//! (calendrier de la récapitulative ∩ ressource), jamais tronquée, sans
//! effet sur les dates ni la durée de la récapitulative ; ressource
//! matérielle : span = celui de la récapitulative. Source : O28.

use plannercraft_core::{AssignmentRole, Minutes, ResourceKind, TaskType};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

use crate::r171_assignment_effective_calendar::{assign, assign_full, placed, resource, resource_kind};

/// Récapitulative S {A 10 j} : lun 19/10 – ven 30/10.
fn summary() -> Net {
    Net::new("2026-10-19T08:00")
        .task("S", 0)
        .task("A", 4_800)
        .level("A", 2)
}

#[test]
fn r179_summary_assignment_own_duration() {
    // R 40 h à 100 % sur S ⇒ lun 19/10 – ven 23/10 17:00 ; S inchangée.
    let run = assign(resource(summary(), 1, None), 1, "S", 1, 2_400).run();
    let s = run.at("S");
    assert_eq!(s.start, moment("2026-10-19T08:00"));
    assert_eq!(s.finish, moment("2026-10-30T17:00"));
    assert_eq!(s.duration, Minutes::new(4_800));
    let a = placed(&run, 1);
    assert_eq!(a.role, AssignmentRole::Summary);
    assert_eq!(a.span.start, moment("2026-10-19T08:00"));
    assert_eq!(a.span.finish, moment("2026-10-23T17:00"));
    assert_eq!(a.duration, Minutes::new(2_400));
}

#[test]
fn r179_fixed_duration_summary_assignment_own_duration() {
    // Récapitulative à durée fixe de durée enregistrée nulle (`Type` 1,
    // toutes les récapitulatives du corpus, O32 ; A6 UID 33) : l'affectation
    // dure toujours W ÷ U (40 h ⇒ lun 19/10 – ven 23/10 17:00), et non la
    // durée du modèle (span vide, sans charge).
    let net = summary().with_task("S", |t| {
        t.task_type = TaskType::FixedDuration;
        t.duration =
            plannercraft_core::Duration::new(Minutes::ZERO, t.duration.unit(), t.duration.basis(), false)
                .unwrap();
    });
    let run = assign(resource(net, 1, None), 1, "S", 1, 2_400).run();
    assert_eq!(run.at("S").finish, moment("2026-10-30T17:00"));
    let a = placed(&run, 1);
    assert_eq!(a.role, AssignmentRole::Summary);
    assert_eq!(a.span.start, moment("2026-10-19T08:00"));
    assert_eq!(a.span.finish, moment("2026-10-23T17:00"));
    assert_eq!(a.duration, Minutes::new(2_400));
    // La ressource est chargée à 100 % sur ce span (pic nul sans span).
    assert_eq!(
        run.schedule
            .resources
            .get(&plannercraft_core::ResourceUid::new(1))
            .map(|r| r.peak),
        Some(plannercraft_core::Units::FULL)
    );
}

#[test]
fn r179_summary_assignment_never_truncated() {
    // 120 h à 100 % (15 j) dépassent la récapitulative (10 j) : l'affectation
    // finit ven 06/11, la récapitulative reste au ven 30/10 (O28 : 200 h et
    // 240 h finissent avant ou après sans la déplacer).
    let run = assign(resource(summary(), 1, None), 1, "S", 1, 7_200).run();
    assert_eq!(run.at("S").finish, moment("2026-10-30T17:00"));
    assert_eq!(placed(&run, 1).span.finish, moment("2026-11-06T17:00"));
}

#[test]
fn r179_summary_assignment_delay() {
    // Délai de 1 j : mar 20/10 – lun 26/10 17:00 (40 h).
    let net = assign_full(
        resource(summary(), 1, None),
        1,
        "S",
        Some(1),
        (2_400, 10_000, 480),
    );
    let a = placed(&net.run(), 1);
    assert_eq!(a.span.start, moment("2026-10-20T08:00"));
    assert_eq!(a.span.finish, moment("2026-10-26T17:00"));
}

#[test]
fn r179_material_covers_summary() {
    // Ressource matérielle sur S : span = celui de S (O28, R10).
    let net = resource_kind(
        summary(),
        1,
        None,
        ResourceKind::Material {
            label: "u".to_owned(),
        },
    );
    let run = assign_full(net, 1, "S", Some(1), (60, 10_000, 0)).run();
    let a = placed(&run, 1);
    assert_eq!(a.role, AssignmentRole::Summary);
    assert_eq!(a.span, run.at("S").early);
}
