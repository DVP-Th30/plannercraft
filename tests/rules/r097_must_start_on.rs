//! R-097 — Doit commencer le (MSO) : option active, la feuille est épinglée
//! sur l'ancrage `A` (R-090) : `early = late = A`, quelles que soient ses
//! bornes, chaque borne violée signalée (R-100) ; option inactive :
//! fenêtre (plancher `s*` en passe avant, plafond `A.finish` en passe
//! arrière, `constraint.not_honored` si le début dépasse `s*`). Sources :
//! S27 (« ES,LS,SS=CD »), S8, S31.

use plannercraft_core::{
    Constraint, ConstraintKind, Diagnostic, Driver, LateDriver, LinkType, Minutes, OverriddenBound,
};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

fn mso(d: &str) -> Constraint {
    Constraint::MustStartOn { date: moment(d) }
}

#[test]
fn r097_pinned_task_is_critical() {
    // A 1 j MSO mer 21/10 08:00, C 4 j seule (fin jeu 22/10 17:00) : A mer
    // 08:00 – 17:00 au plus tôt et au plus tard, TF 0, critique malgré la
    // fin du projet un jour plus tard.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("C", 1_920)
        .constraint("A", mso("2026-10-21T08:00"))
        .run();
    let a = run.at("A");
    assert_eq!(a.early.start, moment("2026-10-21T08:00"));
    assert_eq!(a.early, a.late);
    assert_eq!(a.total_slack, Minutes::ZERO);
    assert!(a.critical);
    let kind = ConstraintKind::MustStartOn;
    assert_eq!(a.driver, Driver::Constraint { kind });
    assert_eq!(a.late_driver, LateDriver::Constraint { kind });
    assert_eq!(run.codes(), [""; 0]);
}

#[test]
fn r097_conflict_with_a_link() {
    // P 3 j FS → A 1 j MSO mar 20/10 08:00 : A mar 08:00 – 17:00, override
    // du lien ; fin du projet mer 17:00 (P) ; P au plus tard jeu 15/10 08:00
    // – lun 19/10 17:00, TF(P) −960, FS(P) 0.
    let run = Net::new("2026-10-19T08:00")
        .task("P", 1_440)
        .task("A", 480)
        .fs("P", "A")
        .constraint("A", mso("2026-10-20T08:00"))
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-20T08:00"));
    assert_eq!(run.at("A").late.start, moment("2026-10-20T08:00"));
    assert_eq!(run.schedule.project_finish, moment("2026-10-21T17:00"));
    let p = run.at("P");
    assert_eq!(p.late.start, moment("2026-10-15T08:00"));
    assert_eq!(p.late.finish, moment("2026-10-19T17:00"));
    assert_eq!(p.total_slack, Minutes::new(-960));
    assert_eq!(p.free_slack, Minutes::ZERO);
    assert_eq!(
        run.schedule.diagnostics,
        [Diagnostic::ConstraintOverride {
            task: run.uid("A"),
            kind: ConstraintKind::MustStartOn,
            bound: OverriddenBound::Predecessor {
                predecessor: run.uid("P"),
                link: LinkType::FinishToStart
            }
        }]
    );
}

#[test]
fn r097_window_when_the_option_is_off() {
    // Même réseau, option inactive : A jeu 22/10 08:00 – 17:00 (le lien),
    // `not_honored` ; au plus tard mar 08:00 – 17:00 (plafond A.finish) :
    // TF(A) = TF(P) = −960.
    let run = Net::new("2026-10-19T08:00")
        .task("P", 1_440)
        .task("A", 480)
        .fs("P", "A")
        .constraint("A", mso("2026-10-20T08:00"))
        .honor(false)
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-22T08:00"));
    assert_eq!(run.at("A").late.start, moment("2026-10-20T08:00"));
    assert_eq!(run.at("A").total_slack, Minutes::new(-960));
    assert_eq!(run.at("P").total_slack, Minutes::new(-960));
    assert_eq!(
        run.schedule.diagnostics,
        [Diagnostic::ConstraintNotHonored {
            task: run.uid("A"),
            kind: ConstraintKind::MustStartOn,
            date: moment("2026-10-20T08:00")
        }]
    );
    // Fenêtre sans conflit : MSO jeu 22/10 08:00, plancher → A jeu ;
    // plafond jeu 17:00 → au plus tard jeu ; TF 0 ; aucun diagnostic.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("C", 2_400)
        .constraint("A", mso("2026-10-22T08:00"))
        .honor(false)
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-22T08:00"));
    assert_eq!(run.at("A").late.start, moment("2026-10-22T08:00"));
    assert_eq!(run.at("A").total_slack, Minutes::ZERO);
    assert_eq!(run.codes(), [""; 0]);
}
