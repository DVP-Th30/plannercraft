//! R-108 — À rebours, « le plus tard possible » est la contrainte par
//! défaut (`Constraint::default_for`) ; une tâche « le plus tôt possible »
//! est épinglée sur ses dates au plus tôt (miroir de R-092) si sa marge
//! totale est ≥ 0, puis la passe arrière est rejouée une fois : ses
//! prédécesseurs sont replacés (ils perdent leur marge), cause de sa fin au
//! plus tard `Constraint { AsSoonAsPossible }`. Sources : S28, S29, S33 ;
//! miroir : hypothèse.

use plannercraft_core::{Constraint, ConstraintKind, LateDriver, Minutes, Moment, ScheduleAnchor};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

#[test]
fn r108_default_constraint_depends_on_the_anchor() {
    let at = Moment::MIN;
    assert_eq!(
        Constraint::default_for(&ScheduleAnchor::FromStart(at)),
        Constraint::AsSoonAsPossible
    );
    assert_eq!(
        Constraint::default_for(&ScheduleAnchor::FromFinish(at)),
        Constraint::AsLateAsPossible
    );
}

#[test]
fn r108_asap_task_takes_its_early_dates() {
    // F ven 23/10 17:00 ; X 1 j FS → A 1 j ASAP ; C 4 j. Passe principale :
    // A ven, X jeu, C mar – ven ; S* mar 08:00 ; secondaire : X mar, A mer ;
    // TF(A) 960 → A épinglée mer ; repasse : X au plus tard mar ; TF X = A =
    // C = 0.
    let run = Net::new("2026-10-19T08:00")
        .from_finish("2026-10-23T17:00")
        .task("X", 480)
        .task("A", 480)
        .task("C", 1_920)
        .fs("X", "A")
        .constraint("A", Constraint::AsSoonAsPossible)
        .run();
    let a = run.at("A");
    assert_eq!(a.start, moment("2026-10-21T08:00"));
    assert_eq!(a.early, a.late);
    assert_eq!(
        a.late_driver,
        LateDriver::Constraint {
            kind: ConstraintKind::AsSoonAsPossible
        }
    );
    assert_eq!(run.at("X").late.start, moment("2026-10-20T08:00"));
    for name in ["X", "A", "C"] {
        assert_eq!(run.at(name).total_slack, Minutes::ZERO, "{name}");
        assert!(run.at(name).critical, "{name}");
    }
    assert_eq!(run.schedule.project_start, moment("2026-10-20T08:00"));
    assert_eq!(run.codes(), [""; 0]);
}

#[test]
fn r108_asap_variant_keeps_the_computed_start() {
    // R-106 avec C ASAP : C planifiée mer (au plus tôt), TF 0 ; S* inchangé.
    let run = Net::new("2026-10-19T08:00")
        .from_finish("2026-10-23T17:00")
        .task("A", 480)
        .task("B", 960)
        .task("C", 480)
        .fs("A", "B")
        .constraint("C", Constraint::AsSoonAsPossible)
        .run();
    assert_eq!(run.at("C").start, moment("2026-10-21T08:00"));
    assert_eq!(run.at("C").total_slack, Minutes::ZERO);
    assert_eq!(run.schedule.project_start, moment("2026-10-21T08:00"));
}

#[test]
fn r108_asap_with_negative_slack_is_not_pinned() {
    // Option inactive ; X 1 j SNET ven 23/10 08:00 FS → A 1 j ASAP ; F ven
    // 23/10 17:00. A au plus tard ven, X jeu (SNET non tenu) ; S* jeu ;
    // passe avant : X ven (plancher), A lun 26/10 → TF(A) −480 : pas
    // d'épingle, A planifiée ven (au plus tard).
    let run = Net::new("2026-10-19T08:00")
        .from_finish("2026-10-23T17:00")
        .honor(false)
        .task("X", 480)
        .task("A", 480)
        .fs("X", "A")
        .constraint(
            "X",
            Constraint::StartNoEarlierThan {
                date: moment("2026-10-23T08:00"),
            },
        )
        .constraint("A", Constraint::AsSoonAsPossible)
        .run();
    let a = run.at("A");
    assert_eq!(a.total_slack, Minutes::new(-480));
    assert_eq!(a.start, moment("2026-10-23T08:00"));
    assert_eq!(a.early.start, moment("2026-10-26T08:00"));
    assert_eq!(run.codes(), ["constraint.not_honored"]);
}
