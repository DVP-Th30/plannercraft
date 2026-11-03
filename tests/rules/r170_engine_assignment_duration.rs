//! R-170 (moteur) — Durée d'une affectation : `d_a = round_div(W × 10 000,
//! U)` pour une tâche à unités fixes ou à travail fixe ; durée de la tâche
//! pour une tâche à durée fixe et pour des unités nulles (R-181) ; un `d_a`
//! hors du domaine des durées rend la tâche non planifiable (R-069),
//! `assignment.duration_out_of_domain`. Le recalcul ne rééquilibre jamais
//! (ADR-0013 D3) : la durée enregistrée de la tâche ne place plus une
//! feuille affectée.

use plannercraft_core::{AssignmentRole, Exclusion, Minutes, TaskOutcome, TaskType};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

use crate::r171_assignment_effective_calendar::{assign_full, placed, resource};

/// Tâche « A » de 2 j enregistrés (960 min), de type `ty`, affectée à R1
/// pour `work` minutes à `units`.
fn run(ty: TaskType, work: i64, units: i64) -> plannercraft_tests::net::Run {
    let net = Net::new("2026-10-19T08:00")
        .task("A", 960)
        .with_task("A", |t| t.task_type = ty);
    assign_full(resource(net, 1, None), 1, "A", Some(1), (work, units, 0)).run()
}

#[test]
fn r170_engine_d_a_from_work() {
    // Unités fixes et travail fixe : 16 h à 50 % ⇒ d_a = 1 920 (4 j) ⇒
    // lun 08:00 – jeu 17:00, malgré la durée enregistrée de 2 j.
    for ty in [TaskType::FixedUnits, TaskType::FixedWork] {
        let r = run(ty, 960, 5_000);
        assert_eq!(r.at("A").finish, moment("2026-10-22T17:00"), "{ty:?}");
        assert_eq!(r.at("A").duration, Minutes::new(1_920), "{ty:?}");
        let a = placed(&r, 1);
        assert_eq!(a.duration, Minutes::new(1_920));
        assert_eq!(a.role, AssignmentRole::Work);
    }
    // Arrondi R-001 : 7 min à 30 % ⇒ 23,3 → 23 min.
    let r = run(TaskType::FixedUnits, 7, 3_000);
    assert_eq!(placed(&r, 1).duration, Minutes::new(23));
}

#[test]
fn r170_engine_fixed_duration_keeps_task_duration() {
    // Durée fixe : d_a = durée de la tâche (2 j), quel que soit le travail.
    let r = run(TaskType::FixedDuration, 960, 5_000);
    assert_eq!(r.at("A").finish, moment("2026-10-20T17:00"));
    assert_eq!(placed(&r, 1).duration, Minutes::new(960));
}

#[test]
fn r170_engine_zero_units_takes_task_duration() {
    // Unités nulles : pas de d_a propre, durée de la tâche (R-181).
    let r = run(TaskType::FixedUnits, 960, 0);
    assert_eq!(r.at("A").finish, moment("2026-10-20T17:00"));
    assert_eq!(placed(&r, 1).duration, Minutes::new(960));
}

#[test]
fn r170_engine_out_of_domain_unschedulable() {
    // 1 666 666 666 h à U = 1 (1/10 000) : d_a = 10¹⁵ min, hors du domaine
    // ⇒ tâche non planifiable, jamais de panique.
    let r = run(TaskType::FixedUnits, 99_999_999_960, 1);
    assert!(matches!(
        r.outcome("A"),
        TaskOutcome::Excluded {
            reason: Exclusion::Unschedulable,
            ..
        }
    ));
    let codes = r.codes();
    assert!(codes.contains(&"assignment.duration_out_of_domain"), "{codes:?}");
    assert!(codes.contains(&"task.unschedulable"), "{codes:?}");
    assert!(r.schedule.assignments.is_empty());
}
