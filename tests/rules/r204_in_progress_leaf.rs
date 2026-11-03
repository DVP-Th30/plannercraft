//! R-204 — Feuille en cours : partie réelle, `Stop`, `Resume`, reste. Début
//! planifié = début réel brut ; `Stop` = `forward_span(début réel,
//! réel).finish` (réel nul : `Stop` = début réel) ; `Resume` = max(instant
//! ouvré suivant `Stop`, reprise saisie, bornes de R-205) ; fin =
//! `forward_span(Resume, D − réel).finish` ; `Driver::Actual`. Source : S26,
//! S51, S53 ; reste : hypothèse.

use plannercraft_core::{Diagnostic, Driver, Minutes, Percent, ProgressState};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

/// T 3 j (1 440), début réel lun 19/10 08:00, `pct` (1/100 de %).
pub(crate) fn started(pct: i64) -> Net {
    Net::new("2026-10-19T08:00").task("T", 1_440).with_task("T", |t| {
        t.progress.actual_start = Some(moment("2026-10-19T08:00"));
        t.progress.percent_complete = Percent::new(pct).unwrap();
    })
}

#[test]
fn r204_in_progress_split_point() {
    // 3 333 ⇒ réel 480 : `Stop` lun 17:00, `Resume` mar 08:00, fin mer 17:00.
    let run = started(3_333).run();
    let t = run.at("T");
    assert_eq!(t.start, moment("2026-10-19T08:00"));
    assert_eq!(t.finish, moment("2026-10-21T17:00"));
    assert_eq!(t.early.start, moment("2026-10-19T08:00"));
    assert_eq!(t.driver, Driver::Actual);
    assert_eq!(t.driver_bound, None);
    assert_eq!(t.duration, Minutes::new(1_440));
    let p = t.progress.unwrap();
    assert_eq!(p.state, ProgressState::Started);
    assert_eq!(p.actual_start, moment("2026-10-19T08:00"));
    assert_eq!(p.actual_finish, None);
    assert_eq!(p.actual_duration, Minutes::new(480));
    assert_eq!(p.remaining_duration, Minutes::new(960));
    assert_eq!(p.stop, moment("2026-10-19T17:00"));
    assert_eq!(p.resume, moment("2026-10-20T08:00"));
    // 3 300 ⇒ réel 475 : `Stop` lun 16:55, reprise aussitôt, fin mer 17:00.
    let run = started(3_300).run();
    let p = run.at("T").progress.unwrap();
    assert_eq!(p.actual_duration, Minutes::new(475));
    assert_eq!(p.stop, moment("2026-10-19T16:55"));
    assert_eq!(p.resume, moment("2026-10-19T16:55"));
    assert_eq!(run.at("T").finish, moment("2026-10-21T17:00"));
    // Début réel absent : implicite = début au plus tôt du réseau,
    // `progress.actual_start_missing` une seule fois.
    let run = started(3_333)
        .with_task("T", |t| t.progress.actual_start = None)
        .run();
    assert_eq!(run.at("T").start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("T").finish, moment("2026-10-21T17:00"));
    let missing = Diagnostic::ProgressActualStartMissing { task: run.uid("T") };
    assert_eq!(
        run.schedule.diagnostics.iter().filter(|d| **d == missing).count(),
        1
    );
}

#[test]
fn r204_zero_actual() {
    // Début réel seul, lun 12:30 (pause, chômé, gardé brut) : réel 0,
    // `Stop` = début réel, reprise lun 13:00, fin jeu 22/10 12:00.
    let run = Net::new("2026-10-19T08:00")
        .task("T", 1_440)
        .with_task("T", |t| {
            t.progress.actual_start = Some(moment("2026-10-19T12:30"));
        })
        .run();
    let t = run.at("T");
    assert_eq!(t.start, moment("2026-10-19T12:30"));
    assert_eq!(t.finish, moment("2026-10-22T12:00"));
    let p = t.progress.unwrap();
    assert_eq!(p.actual_duration, Minutes::ZERO);
    assert_eq!(p.stop, moment("2026-10-19T12:30"));
    assert_eq!(p.resume, moment("2026-10-19T13:00"));
    assert_eq!(p.percent_complete, Percent::ZERO);
    // Réel = D sans fin réelle : reste nul, la tâche finit à `Stop`.
    let run = started(9_999)
        .with_task("T", |t| t.progress.actual_duration = Minutes::new(1_440))
        .run();
    let p = run.at("T").progress.unwrap();
    assert_eq!(p.remaining_duration, Minutes::ZERO);
    assert_eq!(p.stop, moment("2026-10-21T17:00"));
    assert_eq!(p.resume, moment("2026-10-21T17:00"));
    assert_eq!(run.at("T").finish, moment("2026-10-21T17:00"));
}

#[test]
fn r204_entered_resume_later() {
    // Reprise saisie jeu 22/10 08:00 : le reste (960) en part, fin ven
    // 23/10 17:00 ; une reprise saisie avant `Stop` est sans effet.
    let run = started(3_333)
        .with_task("T", |t| t.progress.resume = Some(moment("2026-10-22T08:00")))
        .run();
    let p = run.at("T").progress.unwrap();
    assert_eq!(p.stop, moment("2026-10-19T17:00"));
    assert_eq!(p.resume, moment("2026-10-22T08:00"));
    assert_eq!(run.at("T").finish, moment("2026-10-23T17:00"));
    let run = started(3_333)
        .with_task("T", |t| t.progress.resume = Some(moment("2026-10-19T10:00")))
        .run();
    assert_eq!(run.at("T").progress.unwrap().resume, moment("2026-10-20T08:00"));
    assert_eq!(run.at("T").finish, moment("2026-10-21T17:00"));
}
