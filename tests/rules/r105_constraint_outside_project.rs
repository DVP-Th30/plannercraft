//! R-105 — Contrainte dure hors du projet : option active, un serrage (ou
//! une épingle, R-097) peut placer une tâche avant le début du projet :
//! `constraint.override { ProjectStart }` ; l'UID 0 s'étend jusqu'à elle ;
//! `project_start` reste l'ancre. Option inactive : le plancher R-036
//! l'emporte, `constraint.not_honored`. Source : S24 ; hypothèse.

use plannercraft_core::{Constraint, ConstraintKind, Diagnostic, Minutes, OverriddenBound};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

fn net(honor: bool) -> Net {
    // A 1 j SNLT ven 16/10 08:00, début du projet lun 19/10 08:00.
    Net::new("2026-10-19T08:00")
        .task("A", 480)
        .constraint(
            "A",
            Constraint::StartNoLaterThan {
                date: moment("2026-10-16T08:00"),
            },
        )
        .honor(honor)
}

#[test]
fn r105_clamp_before_the_project_start() {
    // Active : A ven 16/10 08:00 – 17:00, override du début du projet ; UID
    // 0 ven 16/10 08:00 – 17:00 ; `project_start` lun 08:00 (l'ancre) ;
    // `project_finish` ven 17:00 (R-045 : max des fins au plus tôt).
    let run = net(true).run();
    assert_eq!(run.at("A").early.start, moment("2026-10-16T08:00"));
    assert_eq!(run.at("A").early.finish, moment("2026-10-16T17:00"));
    assert_eq!(run.at("A").total_slack, Minutes::ZERO);
    assert_eq!(
        run.schedule.diagnostics,
        [Diagnostic::ConstraintOverride {
            task: run.uid("A"),
            kind: ConstraintKind::StartNoLaterThan,
            bound: OverriddenBound::ProjectStart,
        }]
    );
    let ps = run.schedule.project_summary.unwrap();
    assert_eq!(ps.start, moment("2026-10-16T08:00"));
    assert_eq!(run.schedule.project_start, moment("2026-10-19T08:00"));
    assert_eq!(run.schedule.project_finish, moment("2026-10-16T17:00"));
}

#[test]
fn r105_project_start_wins_when_the_option_is_off() {
    // Inactive : A lun 19/10 08:00 – 17:00 (R-036), au plus tard ven 16/10,
    // TF −480, `not_honored`.
    let run = net(false).run();
    assert_eq!(run.at("A").early.start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("A").late.start, moment("2026-10-16T08:00"));
    assert_eq!(run.at("A").total_slack, Minutes::new(-480));
    assert_eq!(
        run.schedule.diagnostics,
        [Diagnostic::ConstraintNotHonored {
            task: run.uid("A"),
            kind: ConstraintKind::StartNoLaterThan,
            date: moment("2026-10-16T08:00"),
        }]
    );
}

#[test]
fn r105_pin_before_the_project_start() {
    // A 1 j MSO ven 16/10 08:00 → ven 16/10 08:00 – 17:00, override du début
    // du projet, UID 0 depuis ven 16/10 08:00 ; option inactive : A lun
    // 19/10, TF −480, `not_honored`.
    let mso = Constraint::MustStartOn {
        date: moment("2026-10-16T08:00"),
    };
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .constraint("A", mso)
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-16T08:00"));
    assert_eq!(
        run.schedule.diagnostics,
        [Diagnostic::ConstraintOverride {
            task: run.uid("A"),
            kind: ConstraintKind::MustStartOn,
            bound: OverriddenBound::ProjectStart,
        }]
    );
    assert_eq!(
        run.schedule.project_summary.unwrap().start,
        moment("2026-10-16T08:00")
    );
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .constraint("A", mso)
        .honor(false)
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("A").total_slack, Minutes::new(-480));
    assert_eq!(run.codes(), ["constraint.not_honored"]);
}
