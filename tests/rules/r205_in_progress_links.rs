//! R-205 — Liens et contraintes d'une tâche en cours ; option de
//! fractionnement. Fractionnement permis (`split_in_progress`, défaut) :
//! une borne FS/SS au-delà de `Stop`, ou une contrainte de début (SNET,
//! MSO) au-delà, repousse `Resume` ; une borne FF/SF porte sur la fin du
//! reste. Non permis (A7) : reste collé à `Stop`, bornes sans effet,
//! `progress.link_ignored` (info, par lien). Contraintes de fin (et toute
//! autre contrainte datée) : ignorées, `constraint.ignored_in_progress` ;
//! échéance gardée (R-102). Source : S53 (« If you allow tasks in progress
//! to be split, the resume date could be scheduled for a later date ») ;
//! O36 ; reste : hypothèse.

use plannercraft_core::{Constraint, ConstraintKind, Diagnostic, Driver, LinkType, Minutes};
use plannercraft_tests::moment;
use plannercraft_tests::net::{Net, lag_w};

use crate::r204_in_progress_leaf::started;

/// P 2 j lun-mar non commencée, FS → T commencée lun 08:00 à 3 333.
fn out_of_sequence() -> Net {
    started(3_333).task("P", 960).fs("P", "T")
}

#[test]
fn r205_out_of_sequence_split() {
    // Borne de P (mar 17:00) au-delà de `Stop` (lun 17:00) : reprise mer
    // 08:00, fin jeu 17:00 ; le début reste le début réel.
    let run = out_of_sequence().run();
    let t = run.at("T");
    assert_eq!(t.start, moment("2026-10-19T08:00"));
    assert_eq!(t.finish, moment("2026-10-22T17:00"));
    assert_eq!(t.driver, Driver::Actual);
    let p = t.progress.unwrap();
    assert_eq!(p.stop, moment("2026-10-19T17:00"));
    assert_eq!(p.resume, moment("2026-10-21T08:00"));
    assert!(!run.codes().contains(&"progress.link_ignored"));
    // SS avec 1 j de décalage : borne mar 08:00, sans effet (reprise mar
    // 08:00 de toute façon).
    let run = started(3_333)
        .task("P", 960)
        .link("P", "T", LinkType::StartToStart, lag_w(480))
        .run();
    assert_eq!(run.at("T").progress.unwrap().resume, moment("2026-10-20T08:00"));
    // FF : P 5 j (fin ven 17:00) FF → T : le reste finit au plus tôt ven
    // 17:00, reprise jeu 08:00.
    let run = started(3_333)
        .task("P", 2_400)
        .link("P", "T", LinkType::FinishToFinish, plannercraft_core::Lag::ZERO)
        .run();
    assert_eq!(run.at("T").progress.unwrap().resume, moment("2026-10-22T08:00"));
    assert_eq!(run.at("T").finish, moment("2026-10-23T17:00"));
    // SNET mer 21/10 08:00 : plancher de la reprise, pas du début.
    let run = started(3_333)
        .constraint(
            "T",
            Constraint::StartNoEarlierThan {
                date: moment("2026-10-21T08:00"),
            },
        )
        .run();
    assert_eq!(run.at("T").start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("T").progress.unwrap().resume, moment("2026-10-21T08:00"));
    assert_eq!(run.at("T").finish, moment("2026-10-22T17:00"));
    assert!(!run.codes().contains(&"constraint.ignored_in_progress"));
}

#[test]
fn r205_no_split_link_ignored() {
    // Fractionnement non permis : reste collé à `Stop`, fin mer 17:00,
    // `progress.link_ignored` pour P → T ; la contrainte SNET est ignorée.
    let run = out_of_sequence()
        .options(|o| o.progress.split_in_progress = false)
        .run();
    let t = run.at("T");
    assert_eq!(t.finish, moment("2026-10-21T17:00"));
    assert_eq!(t.progress.unwrap().resume, moment("2026-10-20T08:00"));
    assert!(
        run.schedule
            .diagnostics
            .contains(&Diagnostic::ProgressLinkIgnored {
                predecessor: run.uid("P"),
                successor: run.uid("T"),
            })
    );
    let run = started(3_333)
        .options(|o| o.progress.split_in_progress = false)
        .constraint(
            "T",
            Constraint::StartNoEarlierThan {
                date: moment("2026-10-21T08:00"),
            },
        )
        .run();
    assert_eq!(run.at("T").finish, moment("2026-10-21T17:00"));
    assert!(
        run.schedule
            .diagnostics
            .contains(&Diagnostic::ConstraintIgnoredInProgress {
                task: run.uid("T"),
                kind: ConstraintKind::StartNoEarlierThan,
            })
    );
    // Lien dont la borne est tenue : aucun diagnostic.
    let run = started(3_333)
        .task("P", 0)
        .fs("P", "T")
        .options(|o| o.progress.split_in_progress = false)
        .run();
    assert!(!run.codes().contains(&"progress.link_ignored"));
}

#[test]
fn r205_finish_constraint_ignored() {
    // FNLT mar 20/10 17:00, MFO, FNET : ignorées pour les dates (fin mer
    // 17:00, aucune marge négative tirée de la contrainte), info.
    for (c, kind) in [
        (
            Constraint::FinishNoLaterThan {
                date: moment("2026-10-20T17:00"),
            },
            ConstraintKind::FinishNoLaterThan,
        ),
        (
            Constraint::MustFinishOn {
                date: moment("2026-10-26T17:00"),
            },
            ConstraintKind::MustFinishOn,
        ),
        (
            Constraint::FinishNoEarlierThan {
                date: moment("2026-10-26T17:00"),
            },
            ConstraintKind::FinishNoEarlierThan,
        ),
    ] {
        let run = started(3_333).constraint("T", c).run();
        let t = run.at("T");
        assert_eq!(t.finish, moment("2026-10-21T17:00"), "{kind:?}");
        assert_eq!(t.total_slack, Minutes::ZERO, "{kind:?}");
        assert!(
            run.schedule
                .diagnostics
                .contains(&Diagnostic::ConstraintIgnoredInProgress {
                    task: run.uid("T"),
                    kind,
                }),
            "{kind:?}"
        );
        assert!(!run.codes().contains(&"constraint.not_honored"), "{kind:?}");
    }
    // Échéance gardée (R-102) : mar 17:00 dépassée.
    let run = started(3_333).deadline("T", "2026-10-20T17:00").run();
    assert!(run.codes().contains(&"task.deadline_missed"));
    assert!(run.at("T").total_slack < Minutes::ZERO);
}
