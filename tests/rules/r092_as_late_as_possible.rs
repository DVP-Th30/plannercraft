//! R-092 — Le plus tard possible (ALAP) depuis le début : après la passe
//! arrière, une feuille ALAP automatique active, non dégradée, de marge
//! totale ≥ 0 est épinglée sur ses dates au plus tard (ES := LS, KB 74978),
//! puis la passe avant est rejouée une fois (seulement s'il y a une
//! épingle) : ses successeurs sont replacés, ses prédécesseurs gardent leur
//! marge, la fin du projet ne bouge pas. Pilote `Constraint {
//! AsLateAsPossible }`. Une tâche à marge négative n'est pas épinglée.
//! Sources : S27 (« ES=(Calculated)LS »), Q12 (« suck all of the float out
//! of the task and all of its successors »), S8 (ALAP avec échéance).
//! Hypothèse concurrente M1 : dates au plus tôt pures, TF > 0.

use plannercraft_core::{Constraint, ConstraintKind, Diagnostic, Driver, LinkType, Minutes};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

#[test]
fn r092_alap_takes_its_late_dates_and_moves_its_successors() {
    // A 1 j ALAP → B 1 j (FS) ; C 3 j seule. Avant l'épingle : A lun, B
    // mar ; au plus tard B mer, A mar (TF 480). Après : A mar 08:00 – 17:00,
    // B mer 08:00 – 17:00 ; TF A = B = C = 0, tous critiques ; FS(A) 0 ;
    // fin du projet mer 17:00 inchangée.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 480)
        .task("C", 1_440)
        .fs("A", "B")
        .constraint("A", Constraint::AsLateAsPossible)
        .run();
    let a = run.at("A");
    assert_eq!(a.early.start, moment("2026-10-20T08:00"));
    assert_eq!(a.early, a.late);
    assert_eq!(a.start, moment("2026-10-20T08:00"));
    assert_eq!(
        a.driver,
        Driver::Constraint {
            kind: ConstraintKind::AsLateAsPossible
        }
    );
    let b = run.at("B");
    assert_eq!(b.early.start, moment("2026-10-21T08:00"));
    assert_eq!(
        b.driver,
        Driver::Link {
            predecessor: run.uid("A"),
            link: LinkType::FinishToStart
        }
    );
    for name in ["A", "B", "C"] {
        assert_eq!(run.at(name).total_slack, Minutes::ZERO, "{name}");
        assert!(run.at(name).critical, "{name}");
    }
    assert_eq!(a.free_slack, Minutes::ZERO);
    assert_eq!(run.schedule.project_finish, moment("2026-10-21T17:00"));
    assert_eq!(run.codes(), [""; 0]);
}

#[test]
fn r092_predecessors_keep_their_slack() {
    // P 1 j → A 1 j ALAP, C 3 j : A au plus tard mer ; P au plus tard mar,
    // TF(P) 480 (inchangée) ; A épinglée mer.
    let run = Net::new("2026-10-19T08:00")
        .task("P", 480)
        .task("A", 480)
        .task("C", 1_440)
        .fs("P", "A")
        .constraint("A", Constraint::AsLateAsPossible)
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-21T08:00"));
    assert_eq!(run.at("P").early.start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("P").total_slack, Minutes::new(480));
}

#[test]
fn r092_alap_with_a_deadline() {
    // A 1 j ALAP, échéance mar 20/10 17:00 ; C 4 j seule → A épinglée mar
    // 08:00 – 17:00, TF 0.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("C", 1_920)
        .constraint("A", Constraint::AsLateAsPossible)
        .deadline("A", "2026-10-20T17:00")
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-20T08:00"));
    assert_eq!(run.at("A").early.finish, moment("2026-10-20T17:00"));
    assert_eq!(run.at("A").total_slack, Minutes::ZERO);
    assert_eq!(run.codes(), [""; 0]);
}

#[test]
fn r092_negative_slack_is_not_pinned() {
    // P 2 j FS → A 1 j ALAP, échéance mar 20/10 17:00 ; C 4 j → A non
    // épinglée, mer 08:00 – 17:00, TF −480, échéance dépassée.
    let run = Net::new("2026-10-19T08:00")
        .task("P", 960)
        .task("A", 480)
        .task("C", 1_920)
        .fs("P", "A")
        .constraint("A", Constraint::AsLateAsPossible)
        .deadline("A", "2026-10-20T17:00")
        .run();
    let a = run.at("A");
    assert_eq!(a.early.start, moment("2026-10-21T08:00"));
    assert_eq!(a.total_slack, Minutes::new(-480));
    assert_eq!(
        a.driver,
        Driver::Link {
            predecessor: run.uid("P"),
            link: LinkType::FinishToStart
        }
    );
    assert_eq!(
        run.schedule.diagnostics,
        [Diagnostic::TaskDeadlineMissed {
            task: run.uid("A"),
            deadline: moment("2026-10-20T17:00"),
            finish: moment("2026-10-21T17:00"),
        }]
    );
}

#[test]
fn r092_critical_alap_does_not_move() {
    // A 1 j ALAP seule : dates au plus tard = au plus tôt, aucune épingle,
    // pilote inchangé.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .constraint("A", Constraint::AsLateAsPossible)
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("A").driver, Driver::ProjectStart);
}

#[test]
fn r092_chain_of_alap_tasks() {
    // A 1 j ALAP → B 1 j ALAP ; C 4 j seule (fin jeu 17:00) : au plus tard
    // B jeu, A mer ; les deux épinglées (une seule repasse suffit).
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 480)
        .task("C", 1_920)
        .fs("A", "B")
        .constraint("A", Constraint::AsLateAsPossible)
        .constraint("B", Constraint::AsLateAsPossible)
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-21T08:00"));
    assert_eq!(run.at("B").early.start, moment("2026-10-22T08:00"));
    assert_eq!(run.codes(), [""; 0]);
}

#[test]
fn r092_replay_keeps_hard_constraints() {
    // La repasse rejoue la passe principale, contraintes comprises : A 1 j
    // ALAP, C 3 j (A épinglée mar) ; P 2 j FS → X 1 j SNLT mar 20/10 08:00
    // (serrage, option active) : X reste mar 08:00 après la repasse, et son
    // override n'est signalé qu'une fois.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("C", 1_440)
        .task("P", 960)
        .task("X", 480)
        .fs("P", "X")
        .constraint("A", Constraint::AsLateAsPossible)
        .constraint(
            "X",
            Constraint::StartNoLaterThan {
                date: moment("2026-10-20T08:00"),
            },
        )
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-21T08:00"));
    assert_eq!(run.at("X").early.start, moment("2026-10-20T08:00"));
    assert_eq!(run.codes(), ["constraint.override"]);
}
