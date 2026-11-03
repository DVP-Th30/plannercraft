//! R-231 — Explication de la fin et du côté au plus tard : les étapes du
//! début (R-230), puis `Duration { minutes, calendar, longest }` (reste
//! d'une feuille en cours ; feuille à parts : l'affectation de travail qui
//! finit la dernière) ; feuille achevée : `Actual` seul, sans durée ;
//! `LateState` recopie les dates au plus tard, `LateDriver`, TS, FS et la
//! criticité du `Schedule` ; `hypothesis = true` quand R-206 intervient
//! (tâche en cours, ou plafond tiré d'un successeur en cours). Hypothèse.

#![allow(
    clippy::many_single_char_names,
    clippy::too_many_lines,
    reason = "tests : noms des tâches des exemples (A, B, C…)"
)]

use plannercraft_core::schedule::explain::{Side, Step, explain};
use plannercraft_core::{AssignmentUid, CalendarUid, LateDriver, LinkType, Minutes};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

use crate::r171_assignment_effective_calendar::{assign, resource};
use crate::r204_in_progress_leaf::started;
use crate::r230_explain_start::{abc, plan};

#[test]
fn r231_finish_duration_and_late_side() {
    // C ⇒ mer 17:00 = mer 08:00 + 480 sur Standard ; TS 0 ; critique.
    let (p, s, [c]) = plan(abc(), ["C"]);
    let x = explain(&p, &s, c, Side::Finish, 5).unwrap();
    assert_eq!(x.date, moment("2026-10-21T17:00"));
    assert_eq!(
        x.steps.last(),
        Some(&Step::Duration {
            minutes: Minutes::new(480),
            calendar: Some(CalendarUid::new(1)),
            longest: None,
        })
    );
    assert!(matches!(x.steps.first(), Some(Step::Driver { .. })));
    let ts = s.scheduled(c).unwrap();
    assert_eq!(x.late.late, ts.late);
    assert_eq!(x.late.driver, LateDriver::ProjectFinish);
    assert_eq!(x.late.total_slack, Minutes::ZERO);
    assert_eq!(x.late.free_slack, Minutes::ZERO);
    assert!(x.late.critical);
    // Feuille à parts : R1 8 h, R2 16 h ⇒ R2 finit la dernière.
    let net = Net::new("2026-10-19T08:00").task("A", 480);
    let net = assign(
        assign(resource(resource(net, 1, None), 2, None), 1, "A", 1, 480),
        2,
        "A",
        2,
        960,
    );
    let (p, s, [a]) = plan(net, ["A"]);
    let x = explain(&p, &s, a, Side::Finish, 5).unwrap();
    assert_eq!(x.date, moment("2026-10-20T17:00"));
    assert_eq!(
        x.steps.last(),
        Some(&Step::Duration {
            minutes: Minutes::new(960),
            calendar: Some(CalendarUid::new(1)),
            longest: Some(AssignmentUid::new(2)),
        })
    );
    // En cours : le reste (960) ; achevée : la fin réelle, sans durée.
    let (p, s, [t]) = plan(started(3_333), ["T"]);
    let x = explain(&p, &s, t, Side::Finish, 5).unwrap();
    assert_eq!(x.date, moment("2026-10-21T17:00"));
    assert!(matches!(
        x.steps.last(),
        Some(Step::Duration { minutes, .. }) if *minutes == Minutes::new(960)
    ));
    let (p, s, [d]) = plan(
        Net::new("2026-10-19T08:00").task("D", 480).with_task("D", |t| {
            t.progress.actual_start = Some(moment("2026-10-19T08:00"));
            t.progress.actual_finish = Some(moment("2026-10-19T15:00"));
        }),
        ["D"],
    );
    let x = explain(&p, &s, d, Side::Finish, 5).unwrap();
    assert_eq!(x.date, moment("2026-10-19T15:00"));
    assert_eq!(
        x.steps.last(),
        Some(&Step::Actual {
            start: moment("2026-10-19T08:00"),
            finish: Some(moment("2026-10-19T15:00")),
        })
    );
    assert_eq!(x.late.driver, LateDriver::Actual);
    assert!(!x.late.critical);
}

#[test]
fn r231_hypothesis_flag() {
    // T en cours : R-206 ; P FS → T : plafond de P tiré du reste de T.
    let (p, s, [pp, t, u]) = plan(
        started(3_333).task("P", 480).fs("P", "T").task("U", 480),
        ["P", "T", "U"],
    );
    assert!(explain(&p, &s, t, Side::Start, 5).unwrap().hypothesis);
    let x = explain(&p, &s, pp, Side::Finish, 5).unwrap();
    assert_eq!(
        x.late.driver,
        LateDriver::Successor {
            successor: t,
            link: LinkType::FinishToStart,
        }
    );
    assert!(x.hypothesis);
    assert!(!explain(&p, &s, u, Side::Start, 5).unwrap().hypothesis);
    let (p, s, [c]) = plan(abc(), ["C"]);
    assert!(!explain(&p, &s, c, Side::Finish, 5).unwrap().hypothesis);
}
