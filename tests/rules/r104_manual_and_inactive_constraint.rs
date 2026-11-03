//! R-104 — Contrainte d'une tâche manuelle ou inactive. Manuelle (feuille
//! ou récapitulative datée) : contrainte stockée, ignorée par le calcul,
//! `constraint.manual_ignored` (info) si elle diffère du défaut du sens
//! (ASAP depuis le début) ; l'échéance s'applique (plafond de ses dates au
//! plus tard, informatives) et son dépassement est signalé. Inactive :
//! contrainte et échéance ignorées, sans diagnostic. Sources : S8 (colonne
//! « manually scheduled » : « Ignored by Project »), S25 ; inactive :
//! hypothèse.

use plannercraft_core::{Constraint, ConstraintKind, Diagnostic, Minutes, TaskOutcome};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

fn manual() -> Net {
    // M manuelle mer 21/10 08:00 – jeu 22/10 17:00.
    Net::new("2026-10-19T08:00").task("M", 480).manual(
        "M",
        Some("2026-10-21T08:00"),
        Some("2026-10-22T17:00"),
    )
}

#[test]
fn r104_manual_constraint_is_ignored() {
    // MSO lun 19/10 08:00 : dates inchangées, `constraint.manual_ignored`.
    let run = manual()
        .constraint(
            "M",
            Constraint::MustStartOn {
                date: moment("2026-10-19T08:00"),
            },
        )
        .run();
    assert_eq!(run.at("M").early.start, moment("2026-10-21T08:00"));
    assert_eq!(run.at("M").early.finish, moment("2026-10-22T17:00"));
    assert_eq!(
        run.schedule.diagnostics,
        [Diagnostic::ConstraintManualIgnored {
            task: run.uid("M"),
            kind: ConstraintKind::MustStartOn
        }]
    );
    // ASAP (défaut depuis le début) : aucun diagnostic ; ALAP : signalée.
    assert_eq!(manual().run().codes(), [""; 0]);
    let run = manual().constraint("M", Constraint::AsLateAsPossible).run();
    assert_eq!(run.codes(), ["constraint.manual_ignored"]);
}

#[test]
fn r104_manual_deadline_applies() {
    // Échéance mar 20/10 17:00 sur M : au plus tard lun 08:00 – mar 17:00,
    // TF −960, `task.deadline_missed { M, mar 17:00, jeu 17:00 }`.
    let run = manual().deadline("M", "2026-10-20T17:00").run();
    let m = run.at("M");
    assert_eq!(m.late.start, moment("2026-10-19T08:00"));
    assert_eq!(m.late.finish, moment("2026-10-20T17:00"));
    assert_eq!(m.total_slack, Minutes::new(-960));
    assert_eq!(
        run.schedule.diagnostics,
        [Diagnostic::TaskDeadlineMissed {
            task: run.uid("M"),
            deadline: moment("2026-10-20T17:00"),
            finish: moment("2026-10-22T17:00"),
        }]
    );
}

#[test]
fn r104_manual_summary() {
    // S manuelle mar 20/10 08:00 – ven 23/10 17:00 {A 1 j}, FNLT et échéance
    // mer 21/10 17:00 : la contrainte est ignorée (info), l'échéance plafonne
    // les dates au plus tard de S et son dépassement est signalé.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .manual("S", Some("2026-10-20T08:00"), Some("2026-10-23T17:00"))
        .constraint(
            "S",
            Constraint::FinishNoLaterThan {
                date: moment("2026-10-21T17:00"),
            },
        )
        .deadline("S", "2026-10-21T17:00")
        .run();
    assert_eq!(run.at("S").late.finish, moment("2026-10-21T17:00"));
    assert_eq!(run.codes(), ["constraint.manual_ignored", "task.deadline_missed"]);
}

#[test]
fn r104_inactive_task_ignores_constraint_and_deadline() {
    // I inactive SNET jeu 22/10 08:00 et échéance lun 19/10 08:00 :
    // affichage lun 08:00 – 17:00 (contrainte ignorée), aucun diagnostic.
    let run = Net::new("2026-10-19T08:00")
        .task("I", 480)
        .inactive("I")
        .constraint(
            "I",
            Constraint::StartNoEarlierThan {
                date: moment("2026-10-22T08:00"),
            },
        )
        .deadline("I", "2026-10-19T08:00")
        .run();
    let TaskOutcome::Excluded { display: Some(d), .. } = run.outcome("I") else {
        panic!("I doit être exclue avec un affichage");
    };
    assert_eq!(d.start, moment("2026-10-19T08:00"));
    assert_eq!(d.finish, moment("2026-10-19T17:00"));
    assert_eq!(run.codes(), [""; 0]);
}
