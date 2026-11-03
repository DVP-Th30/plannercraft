//! R-109 — Contraintes à rebours (table de dérivation, colonnes « à
//! rebours », R-099) : SNLT, FNLT plafonds ; SNET, FNET serrés dans la passe
//! arrière principale si l'option est active (`constraint.override` par
//! plafond violé : successeur, plafond hérité, ancre `F`), planchers de la
//! passe avant secondaire sinon (`constraint.not_honored` si la date
//! planifiée précède l'ancrage) ; MSO, MFO épinglées (option active) ou
//! fenêtres. Source : miroir de S27, Q13 ; hypothèse.

use plannercraft_core::{
    Constraint, ConstraintKind, Diagnostic, LateDriver, LinkType, Minutes, OverriddenBound, TaskUid,
};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

#[test]
fn r109_finish_no_later_than_caps() {
    // F ven 23/10 17:00 ; A 1 j FS → B 1 j FNLT jeu 22/10 17:00 → B jeu, A
    // mer ; UID 0 mer 08:00 – jeu 17:00.
    let run = Net::new("2026-10-19T08:00")
        .from_finish("2026-10-23T17:00")
        .task("A", 480)
        .task("B", 480)
        .fs("A", "B")
        .constraint(
            "B",
            Constraint::FinishNoLaterThan {
                date: moment("2026-10-22T17:00"),
            },
        )
        .run();
    assert_eq!(run.at("B").start, moment("2026-10-22T08:00"));
    assert_eq!(
        run.at("B").late_driver,
        LateDriver::Constraint {
            kind: ConstraintKind::FinishNoLaterThan
        }
    );
    assert_eq!(run.at("A").start, moment("2026-10-21T08:00"));
    let p = run.schedule.scheduled(TaskUid::PROJECT_SUMMARY).unwrap();
    assert_eq!(
        (p.start, p.finish),
        (moment("2026-10-21T08:00"), moment("2026-10-22T17:00"))
    );
    assert_eq!(run.codes(), [""; 0]);
}

#[test]
fn r109_start_no_later_than_caps() {
    // A 1 j SNLT mar 20/10 08:00, F ven → plafond `A.finish` mar 17:00 → A
    // mar 08:00 – 17:00.
    let run = Net::new("2026-10-19T08:00")
        .from_finish("2026-10-23T17:00")
        .task("A", 480)
        .constraint(
            "A",
            Constraint::StartNoLaterThan {
                date: moment("2026-10-20T08:00"),
            },
        )
        .run();
    assert_eq!(run.at("A").start, moment("2026-10-20T08:00"));
    assert_eq!(run.codes(), [""; 0]);
}

/// A 1 j `c` FS → B 2 j ; F ven 23/10 17:00.
fn clamped(c: Constraint, honor: bool) -> plannercraft_tests::net::Run {
    Net::new("2026-10-19T08:00")
        .from_finish("2026-10-23T17:00")
        .honor(honor)
        .task("A", 480)
        .task("B", 960)
        .fs("A", "B")
        .constraint("A", c)
        .run()
}

#[test]
fn r109_start_no_earlier_than_clamped() {
    // Option active : A 1 j SNET jeu 22/10 08:00 → A au plus tard jeu 08:00
    // – 17:00 (serrage), `override { A, SNET, Successor { B, FS } }` ; S*
    // jeu 08:00 ; B au plus tôt ven 08:00 – lun 26/10 17:00, TF(B) −480.
    let snet = Constraint::StartNoEarlierThan {
        date: moment("2026-10-22T08:00"),
    };
    let run = clamped(snet, true);
    let a = run.at("A");
    assert_eq!(a.late.start, moment("2026-10-22T08:00"));
    assert_eq!(a.early, a.late);
    assert_eq!(
        a.late_driver,
        LateDriver::Constraint {
            kind: ConstraintKind::StartNoEarlierThan
        }
    );
    assert_eq!(
        run.schedule.diagnostics,
        [Diagnostic::ConstraintOverride {
            task: run.uid("A"),
            kind: ConstraintKind::StartNoEarlierThan,
            bound: OverriddenBound::Successor {
                successor: run.uid("B"),
                link: LinkType::FinishToStart
            }
        }]
    );
    assert_eq!(run.schedule.project_start, moment("2026-10-22T08:00"));
    let b = run.at("B");
    assert_eq!(b.early.start, moment("2026-10-23T08:00"));
    assert_eq!(b.early.finish, moment("2026-10-26T17:00"));
    assert_eq!(b.total_slack, Minutes::new(-480));
    assert_eq!(a.total_slack, Minutes::ZERO);
    // FNET jeu 22/10 17:00 : même placement.
    let fnet = Constraint::FinishNoEarlierThan {
        date: moment("2026-10-22T17:00"),
    };
    let run = clamped(fnet, true);
    assert_eq!(run.at("A").late.start, moment("2026-10-22T08:00"));
    assert_eq!(run.codes(), ["constraint.override"]);
}

#[test]
fn r109_start_no_earlier_than_not_honored() {
    // Option inactive : A au plus tard mer, `not_honored { A, SNET, jeu
    // 08:00 }` ; S* mer ; A au plus tôt jeu (plancher) ; TF(A) = TF(B) =
    // −480.
    let date = moment("2026-10-22T08:00");
    let run = clamped(Constraint::StartNoEarlierThan { date }, false);
    assert_eq!(run.at("A").start, moment("2026-10-21T08:00"));
    assert_eq!(run.at("A").early.start, moment("2026-10-22T08:00"));
    assert_eq!(run.schedule.project_start, moment("2026-10-21T08:00"));
    assert_eq!(
        run.schedule.diagnostics,
        [Diagnostic::ConstraintNotHonored {
            task: run.uid("A"),
            kind: ConstraintKind::StartNoEarlierThan,
            date
        }]
    );
    assert_eq!(run.at("A").total_slack, Minutes::new(-480));
    assert_eq!(run.at("B").total_slack, Minutes::new(-480));
}

#[test]
fn r109_must_start_on_pinned() {
    // MSO jeu 22/10 08:00, option active : A jeu 08:00 – 17:00 au plus tôt
    // et au plus tard, override du lien vers B ; TF(B) −480.
    let mso = Constraint::MustStartOn {
        date: moment("2026-10-22T08:00"),
    };
    let run = clamped(mso, true);
    let a = run.at("A");
    assert_eq!(a.early, a.late);
    assert_eq!(a.start, moment("2026-10-22T08:00"));
    assert_eq!(run.codes(), ["constraint.override"]);
    assert_eq!(run.at("B").total_slack, Minutes::new(-480));
}

#[test]
fn r109_must_finish_on_after_the_anchor() {
    // A 1 j MFO lun 26/10 17:00, F ven 23/10 17:00. Option active : A lun
    // 26/10 08:00 – 17:00, `override { A, MFO, ProjectFinish }`. Option
    // inactive (fenêtre) : au plus tard ven, au plus tôt lun (plancher),
    // TF −480, `not_honored`.
    let mfo = Constraint::MustFinishOn {
        date: moment("2026-10-26T17:00"),
    };
    let run = Net::new("2026-10-19T08:00")
        .from_finish("2026-10-23T17:00")
        .task("A", 480)
        .constraint("A", mfo)
        .run();
    assert_eq!(run.at("A").start, moment("2026-10-26T08:00"));
    assert_eq!(
        run.schedule.diagnostics,
        [Diagnostic::ConstraintOverride {
            task: run.uid("A"),
            kind: ConstraintKind::MustFinishOn,
            bound: OverriddenBound::ProjectFinish
        }]
    );
    let run = Net::new("2026-10-19T08:00")
        .from_finish("2026-10-23T17:00")
        .honor(false)
        .task("A", 480)
        .constraint("A", mfo)
        .run();
    let a = run.at("A");
    assert_eq!(a.start, moment("2026-10-23T08:00"));
    assert_eq!(a.early.start, moment("2026-10-26T08:00"));
    assert_eq!(a.total_slack, Minutes::new(-480));
    assert_eq!(run.codes(), ["constraint.not_honored"]);
}
