//! R-095 — Ne pas commencer après le (SNLT) : plafond `A.finish` (R-090)
//! en passe arrière ; option « respecter les contraintes » active : serrage
//! en passe avant (le début ne dépasse pas `s*`), chaque borne violée
//! signalée (R-100). Sources : S27 (« LS=CD »), S8, S31, Q13.

use plannercraft_core::{
    Constraint, ConstraintKind, Diagnostic, Driver, LateDriver, LinkType, Minutes, OverriddenBound,
};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

fn net(honor: bool) -> Net {
    // P 2 j FS → A 1 j SNLT mar 20/10 08:00.
    Net::new("2026-10-19T08:00")
        .task("P", 960)
        .task("A", 480)
        .fs("P", "A")
        .constraint(
            "A",
            Constraint::StartNoLaterThan {
                date: moment("2026-10-20T08:00"),
            },
        )
        .honor(honor)
}

#[test]
fn r095_honored_constraint_overrides_the_link() {
    // Option active : A mar 08:00 – 17:00 (le lien voudrait mer), pilote la
    // contrainte, un override pour le lien P → A ; fin du projet mar 17:00 ;
    // TF(A) 0 ; P au plus tard ven 16/10 08:00 – lun 19/10 17:00, TF(P)
    // −480, FS(P) 0 ; P et A critiques.
    let run = net(true).run();
    let a = run.at("A");
    assert_eq!(a.early.start, moment("2026-10-20T08:00"));
    assert_eq!(a.early.finish, moment("2026-10-20T17:00"));
    assert_eq!(
        a.driver,
        Driver::Constraint {
            kind: ConstraintKind::StartNoLaterThan
        }
    );
    assert_eq!(a.total_slack, Minutes::ZERO);
    assert_eq!(run.schedule.project_finish, moment("2026-10-20T17:00"));
    let p = run.at("P");
    assert_eq!(p.late.start, moment("2026-10-16T08:00"));
    assert_eq!(p.late.finish, moment("2026-10-19T17:00"));
    assert_eq!(p.total_slack, Minutes::new(-480));
    assert_eq!(p.free_slack, Minutes::ZERO);
    assert!(p.critical && a.critical);
    assert_eq!(
        run.schedule.diagnostics,
        [Diagnostic::ConstraintOverride {
            task: run.uid("A"),
            kind: ConstraintKind::StartNoLaterThan,
            bound: OverriddenBound::Predecessor {
                predecessor: run.uid("P"),
                link: LinkType::FinishToStart
            }
        }]
    );
}

#[test]
fn r095_not_honored_when_the_option_is_off() {
    // Option inactive : le lien l'emporte, A mer 08:00 – 17:00,
    // `constraint.not_honored` ; fin mer 17:00 ; A au plus tard mar 08:00 –
    // 17:00 (plafond A.finish), TF(A) −480, cause la contrainte ; TF(P)
    // −480.
    let run = net(false).run();
    let a = run.at("A");
    assert_eq!(a.early.start, moment("2026-10-21T08:00"));
    assert_eq!(
        a.driver,
        Driver::Link {
            predecessor: run.uid("P"),
            link: LinkType::FinishToStart
        }
    );
    assert_eq!(a.late.start, moment("2026-10-20T08:00"));
    assert_eq!(a.late.finish, moment("2026-10-20T17:00"));
    assert_eq!(
        a.late_driver,
        LateDriver::Constraint {
            kind: ConstraintKind::StartNoLaterThan
        }
    );
    assert_eq!(a.total_slack, Minutes::new(-480));
    assert_eq!(run.schedule.project_finish, moment("2026-10-21T17:00"));
    assert_eq!(run.at("P").total_slack, Minutes::new(-480));
    assert_eq!(
        run.schedule.diagnostics,
        [Diagnostic::ConstraintNotHonored {
            task: run.uid("A"),
            kind: ConstraintKind::StartNoLaterThan,
            date: moment("2026-10-20T08:00")
        }]
    );
}

#[test]
fn r095_cap_without_conflict() {
    // A 1 j SNLT mer 21/10 08:00, C 4 j seule : A lun ; au plus tard mer
    // 08:00 – 17:00 (et non jeu), TF(A) 960 ; aucun diagnostic.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("C", 1_920)
        .constraint(
            "A",
            Constraint::StartNoLaterThan {
                date: moment("2026-10-21T08:00"),
            },
        )
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("A").late.start, moment("2026-10-21T08:00"));
    assert_eq!(run.at("A").total_slack, Minutes::new(960));
    assert_eq!(run.codes(), [""; 0]);
}
