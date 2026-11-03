//! R-230 — Explication du début, partie moteur (lot L1) : la passe avant
//! pose `TaskSchedule::driver_bound`, la borne brute retenue pour le pilote
//! avant recalage (R-044) : date du lien converti (FF, SF : converti en
//! début), date d'une contrainte de début (type fin : début de son
//! ancrage), début du projet, borne de la porte ; avant le retard de
//! nivellement (R-178) ; `None` pour `Manual`, `Rollup` et `Actual`.
//! Recaler la borne sur l'échelle de la tâche redonne son début au plus tôt
//! (hors retard, réel, manuelle ; P41, lot L7). Hypothèse.

use plannercraft_core::{Constraint, Driver, LevelingDelay, LinkType, TimeScale};
use plannercraft_tests::net::{Net, lag_w};
use plannercraft_tests::{compile_one, moment, standard};

#[test]
fn r230_driver_bound_link_and_constraint() {
    // A 1 j lun ; B FS de A : borne brute lun 17:00, recalée mar 08:00.
    // C SNET mer 10:30 (après A) : borne = date de la contrainte. D FNET
    // jeu 12:00, 4 h : borne = début de l'ancrage (jeu 08:00). E sans lien :
    // début du projet. S {F} FS de A : F pilotée par la porte, borne lun
    // 17:00. G FF+0 de A, 2 h : borne de fin convertie (lun 15:00).
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 480)
        .fs("A", "B")
        .task("C", 480)
        .fs("A", "C")
        .constraint(
            "C",
            Constraint::StartNoEarlierThan {
                date: moment("2026-10-21T10:30"),
            },
        )
        .task("D", 240)
        .constraint(
            "D",
            Constraint::FinishNoEarlierThan {
                date: moment("2026-10-22T12:00"),
            },
        )
        .task("E", 480)
        .task("S", 0)
        .task("F", 480)
        .level("F", 2)
        .fs("A", "S")
        .task("G", 120)
        .link("A", "G", LinkType::FinishToFinish, lag_w(0))
        .run();
    let tb = run.at("B");
    assert_eq!(
        tb.driver,
        Driver::Link {
            predecessor: run.uid("A"),
            link: LinkType::FinishToStart,
        }
    );
    assert_eq!(tb.driver_bound, Some(moment("2026-10-19T17:00")));
    assert_eq!(tb.early.start, moment("2026-10-20T08:00"));
    let tc = run.at("C");
    assert!(matches!(tc.driver, Driver::Constraint { .. }));
    assert_eq!(tc.driver_bound, Some(moment("2026-10-21T10:30")));
    let td = run.at("D");
    assert!(matches!(td.driver, Driver::Constraint { .. }));
    assert_eq!(td.driver_bound, Some(moment("2026-10-22T08:00")));
    assert_eq!(run.at("E").driver, Driver::ProjectStart);
    assert_eq!(run.at("E").driver_bound, Some(moment("2026-10-19T08:00")));
    let tf = run.at("F");
    assert_eq!(
        tf.driver,
        Driver::SummaryGate {
            summary: run.uid("S")
        }
    );
    assert_eq!(tf.driver_bound, Some(moment("2026-10-19T17:00")));
    let tg = run.at("G");
    assert_eq!(tg.driver_bound, Some(moment("2026-10-19T15:00")));
    // Recaler la borne sur l'échelle de la tâche redonne le début au plus
    // tôt (P41).
    let cal = compile_one(standard());
    let scale = TimeScale::Working(&cal);
    for name in ["B", "C", "D", "E", "F", "G"] {
        let t = run.at(name);
        let snapped = scale
            .forward_span(t.driver_bound.unwrap(), t.duration)
            .unwrap()
            .start;
        assert_eq!(snapped, t.early.start, "{name}");
    }
    // Récapitulative : `Rollup`, aucune borne.
    assert_eq!(run.at("S").driver_bound, None);
}

#[test]
fn r230_no_bound_for_manual_actual_and_before_leveling_delay() {
    // M manuelle, T achevée : aucune borne. L 1 j FS de A retardée de 1 j
    // par nivellement : borne = celle du lien (lun 17:00), avant le retard.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("M", 480)
        .manual("M", Some("2026-10-21T08:00"), None)
        .task("T", 480)
        .with_task("T", |t| {
            t.progress.actual_start = Some(moment("2026-10-19T08:00"));
            t.progress.actual_finish = Some(moment("2026-10-19T17:00"));
        })
        .task("L", 480)
        .fs("A", "L")
        .with_task("L", |t| {
            t.leveling_delay = Some(LevelingDelay {
                minutes: plannercraft_core::Minutes::new(480),
                unit: plannercraft_core::DurationUnit::Day,
                elapsed: false,
            });
        })
        .run();
    assert_eq!(run.at("M").driver, Driver::Manual);
    assert_eq!(run.at("M").driver_bound, None);
    assert_eq!(run.at("T").driver, Driver::Actual);
    assert_eq!(run.at("T").driver_bound, None);
    assert_eq!(run.at("L").start, moment("2026-10-21T08:00"));
    assert_eq!(run.at("L").driver_bound, Some(moment("2026-10-19T17:00")));
}
