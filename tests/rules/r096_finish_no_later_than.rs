//! R-096 — Ne pas finir après le (FNLT) : plafond `f*` (R-090) en passe
//! arrière ; option active : serrage en passe avant (la fin ne dépasse pas
//! `f*`), chaque borne violée signalée (R-100). Sources : S27 (« LF=CD »),
//! S30 (exemple « Prepare lot »), Q13.

use plannercraft_core::{
    Constraint, ConstraintKind, Diagnostic, LateDriver, LinkType, Minutes, OverriddenBound,
};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

fn fnlt(d: &str) -> Constraint {
    Constraint::FinishNoLaterThan { date: moment(d) }
}

fn net(honor: bool) -> Net {
    // P 1 j FS → A 2 j FNLT mar 20/10 17:00.
    Net::new("2026-10-19T08:00")
        .task("P", 480)
        .task("A", 960)
        .fs("P", "A")
        .constraint("A", fnlt("2026-10-20T17:00"))
        .honor(honor)
}

#[test]
fn r096_honored_constraint_overrides_the_link() {
    // Option active : A lun 08:00 – mar 17:00 (fin = f*), override du lien ;
    // P au plus tard ven 16/10 08:00 – 17:00, TF(P) −480 ; TF(A) 0.
    let run = net(true).run();
    let a = run.at("A");
    assert_eq!(a.early.start, moment("2026-10-19T08:00"));
    assert_eq!(a.early.finish, moment("2026-10-20T17:00"));
    assert_eq!(a.total_slack, Minutes::ZERO);
    let p = run.at("P");
    assert_eq!(p.late.start, moment("2026-10-16T08:00"));
    assert_eq!(p.late.finish, moment("2026-10-16T17:00"));
    assert_eq!(p.total_slack, Minutes::new(-480));
    assert_eq!(
        run.schedule.diagnostics,
        [Diagnostic::ConstraintOverride {
            task: run.uid("A"),
            kind: ConstraintKind::FinishNoLaterThan,
            bound: OverriddenBound::Predecessor {
                predecessor: run.uid("P"),
                link: LinkType::FinishToStart
            }
        }]
    );
}

#[test]
fn r096_not_honored_when_the_option_is_off() {
    // Option inactive : A mar 08:00 – mer 17:00, `not_honored` ; au plus tard
    // lun 08:00 – mar 17:00 : TF(A) −480 ; TF(P) −480.
    let run = net(false).run();
    let a = run.at("A");
    assert_eq!(a.early.start, moment("2026-10-20T08:00"));
    assert_eq!(a.early.finish, moment("2026-10-21T17:00"));
    assert_eq!(a.late.start, moment("2026-10-19T08:00"));
    assert_eq!(a.total_slack, Minutes::new(-480));
    assert_eq!(run.at("P").total_slack, Minutes::new(-480));
    assert_eq!(
        run.schedule.diagnostics,
        [Diagnostic::ConstraintNotHonored {
            task: run.uid("A"),
            kind: ConstraintKind::FinishNoLaterThan,
            date: moment("2026-10-20T17:00")
        }]
    );
}

#[test]
fn r096_cap_without_conflict() {
    // A 2 j FNLT mer 21/10 17:00, C 4 j : A au plus tard mar 08:00 – mer
    // 17:00, TF(A) 480 (1 440 sans la contrainte), cause la contrainte.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 960)
        .task("C", 1_920)
        .constraint("A", fnlt("2026-10-21T17:00"))
        .run();
    let a = run.at("A");
    assert_eq!(a.late.start, moment("2026-10-20T08:00"));
    assert_eq!(a.late.finish, moment("2026-10-21T17:00"));
    assert_eq!(a.total_slack, Minutes::new(480));
    assert_eq!(
        a.late_driver,
        LateDriver::Constraint {
            kind: ConstraintKind::FinishNoLaterThan
        }
    );
    assert_eq!(run.codes(), [""; 0]);
}
