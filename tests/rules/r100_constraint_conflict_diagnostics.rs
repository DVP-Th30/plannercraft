//! R-100 — Conflit contrainte/lien : signalement. Un `constraint.override`
//! (avertissement, sujet la tâche contrainte) par borne violée
//! (`OverriddenBound`), en passe principale ; jamais d'échec ni de
//! panique ; le lien violé reste dans le réseau et transmet la marge
//! négative. Sources : S30 (« Project warns us of the scheduling conflict,
//! and then overlaps the tasks »), S8 (« Inflexible constraints usually
//! override any task dependencies »).

use plannercraft_core::{
    Constraint, ConstraintKind, Diagnostic, LinkType, Minutes, OverriddenBound, Severity, Subject,
};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

fn snlt(d: &str) -> Constraint {
    Constraint::StartNoLaterThan { date: moment(d) }
}

#[test]
fn r100_one_override_per_violated_link() {
    // P 2 j FS → A ; Q 3 j FS → A ; A 1 j SNLT mar 20/10 08:00 : deux
    // liens violés, deux diagnostics, triés (P avant Q) ; A mar ; Q au plus
    // tard jeu 15/10 08:00 – lun 17:00, TF(Q) −960 ; TF(P) −480.
    let run = Net::new("2026-10-19T08:00")
        .task("P", 960)
        .task("Q", 1_440)
        .task("A", 480)
        .fs("P", "A")
        .fs("Q", "A")
        .constraint("A", snlt("2026-10-20T08:00"))
        .run();
    let (p, q, a) = (run.uid("P"), run.uid("Q"), run.uid("A"));
    let over = |predecessor| Diagnostic::ConstraintOverride {
        task: a,
        kind: ConstraintKind::StartNoLaterThan,
        bound: OverriddenBound::Predecessor {
            predecessor,
            link: LinkType::FinishToStart,
        },
    };
    assert_eq!(run.schedule.diagnostics, [over(p), over(q)]);
    let d = &run.schedule.diagnostics[0];
    assert_eq!(d.code(), "constraint.override");
    assert_eq!(d.severity(), Severity::Warning);
    assert_eq!(d.subject(), Subject::Task(a));
    assert_eq!(run.at("A").early.start, moment("2026-10-20T08:00"));
    assert_eq!(run.at("Q").late.start, moment("2026-10-15T08:00"));
    assert_eq!(run.at("Q").late.finish, moment("2026-10-19T17:00"));
    assert_eq!(run.at("Q").total_slack, Minutes::new(-960));
    assert_eq!(run.at("P").total_slack, Minutes::new(-480));
}

#[test]
fn r100_summary_gate_override() {
    // S {A 1 j SNLT lun 19/10 08:00} ; X 2 j FS → S : la porte (mar 17:00)
    // voudrait A mer ; A reste lun 08:00 – 17:00, override de la porte de S ;
    // X au plus tard jeu 15/10 08:00 – ven 16/10 17:00, TF(X) −960.
    let run = Net::new("2026-10-19T08:00")
        .task("X", 960)
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .fs("X", "S")
        .constraint("A", snlt("2026-10-19T08:00"))
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-19T08:00"));
    assert_eq!(
        run.schedule.diagnostics,
        [Diagnostic::ConstraintOverride {
            task: run.uid("A"),
            kind: ConstraintKind::StartNoLaterThan,
            bound: OverriddenBound::SummaryGate {
                summary: run.uid("S")
            },
        }]
    );
    assert_eq!(run.at("X").late.start, moment("2026-10-15T08:00"));
    assert_eq!(run.at("X").late.finish, moment("2026-10-16T17:00"));
    assert_eq!(run.at("X").total_slack, Minutes::new(-960));
}

#[test]
fn r100_finish_link_override() {
    // P 3 j FF → A 1 j SNLT mar 20/10 08:00 : borne de fin mer 17:00, A
    // placée mer 08:00 > s* → A mar 08:00 – 17:00 ; P.between(A.finish,
    // borne) > 0 → override du lien FF.
    let run = Net::new("2026-10-19T08:00")
        .task("P", 1_440)
        .task("A", 480)
        .link("P", "A", LinkType::FinishToFinish, plannercraft_core::Lag::ZERO)
        .constraint("A", snlt("2026-10-20T08:00"))
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-20T08:00"));
    assert_eq!(
        run.schedule.diagnostics,
        [Diagnostic::ConstraintOverride {
            task: run.uid("A"),
            kind: ConstraintKind::StartNoLaterThan,
            bound: OverriddenBound::Predecessor {
                predecessor: run.uid("P"),
                link: LinkType::FinishToFinish
            },
        }]
    );
}

#[test]
fn r100_summary_gate_override_with_mso() {
    // Cas de la spécification : S {A 1 j MSO lun 19/10 08:00} ; X 2 j FS →
    // S → A lun 08:00 – 17:00, override de la porte de S ; X au plus tard
    // jeu 15/10 08:00 – ven 16/10 17:00, TF(X) −960.
    let run = Net::new("2026-10-19T08:00")
        .task("X", 960)
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .fs("X", "S")
        .constraint(
            "A",
            Constraint::MustStartOn {
                date: moment("2026-10-19T08:00"),
            },
        )
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-19T08:00"));
    assert_eq!(
        run.schedule.diagnostics,
        [Diagnostic::ConstraintOverride {
            task: run.uid("A"),
            kind: ConstraintKind::MustStartOn,
            bound: OverriddenBound::SummaryGate {
                summary: run.uid("S")
            },
        }]
    );
    assert_eq!(run.at("X").late.start, moment("2026-10-15T08:00"));
    assert_eq!(run.at("X").total_slack, Minutes::new(-960));
}
