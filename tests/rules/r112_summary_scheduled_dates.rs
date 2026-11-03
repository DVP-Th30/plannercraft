//! R-112 — Dates planifiées d'une récapitulative et de l'UID 0 : enveloppe
//! des dates planifiées de leurs enfants (`start` = min, `finish` = max) ;
//! durée R-054 mesurée sur elles. Depuis le début, elles égalent
//! l'enveloppe des dates au plus tôt, épingles comprises. Sources : S23, S8.

use plannercraft_core::{Constraint, Minutes};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

#[test]
fn r112_summary_envelope_includes_pinned_alap() {
    // S {A 1 j ALAP ; B 3 j} : A épinglée mer 08:00 – 17:00 ; S lun 08:00 –
    // mer 17:00 (planifiées = au plus tôt), durée 1 440.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .task("A", 480)
        .task("B", 1_440)
        .level("A", 2)
        .level("B", 2)
        .constraint("A", Constraint::AsLateAsPossible)
        .run();
    assert_eq!(run.at("A").start, moment("2026-10-21T08:00"));
    let s = run.at("S");
    assert_eq!(s.start, moment("2026-10-19T08:00"));
    assert_eq!(s.finish, moment("2026-10-21T17:00"));
    assert_eq!(s.early.start, s.start);
    assert_eq!(s.duration, Minutes::new(1_440));
    let ps = run.schedule.project_summary.unwrap();
    assert_eq!((ps.start, ps.finish), (s.start, s.finish));
}

#[test]
fn r112_summary_follows_a_pinned_child() {
    // S {A 1 j ALAP}, C 3 j seule : A épinglée mer → S mer 08:00 – 17:00
    // (au plus tôt de S = enveloppe après la repasse).
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .task("A", 480)
        .task("C", 1_440)
        .level("A", 2)
        .constraint("A", Constraint::AsLateAsPossible)
        .run();
    assert_eq!(run.at("S").start, moment("2026-10-21T08:00"));
    assert_eq!(run.at("S").finish, moment("2026-10-21T17:00"));
    assert_eq!(run.at("S").duration, Minutes::new(480));
}

#[test]
fn r112_from_finish_summary_on_late_dates() {
    // À rebours (R-106) : F ven 23/10 17:00, S {C 1 j}, D 3 j seule → C au
    // plus tard ven, au plus tôt mer (S* = mer, LS de D) ; S planifiée ven
    // 08:00 – 17:00 (enveloppe au plus tard), durée 480, au plus tôt mer,
    // TF(S) 960 ; UID 0 mer 08:00 – ven 17:00.
    let run = Net::new("2026-10-19T08:00")
        .from_finish("2026-10-23T17:00")
        .task("S", 0)
        .task("C", 480)
        .task("D", 1_440)
        .level("C", 2)
        .run();
    let c = run.at("C");
    assert_eq!(c.start, moment("2026-10-23T08:00"));
    assert_eq!(c.early.start, moment("2026-10-21T08:00"));
    let s = run.at("S");
    assert_eq!(
        (s.start, s.finish),
        (moment("2026-10-23T08:00"), moment("2026-10-23T17:00"))
    );
    assert_eq!(s.duration, Minutes::new(480));
    assert_eq!(s.early.start, moment("2026-10-21T08:00"));
    assert_eq!(s.total_slack, Minutes::new(960));
    let ps = run.schedule.project_summary.unwrap();
    assert_eq!(
        (ps.start, ps.finish),
        (moment("2026-10-21T08:00"), moment("2026-10-23T17:00"))
    );
    assert_eq!(ps.duration, Minutes::new(1_440));
}
