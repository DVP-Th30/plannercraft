//! R-300 — Reprise enregistrée à la minute (ADR-0039, hypothèse) : quand
//! l'arrêt d'une feuille en cours tombe entre deux minutes et que la
//! reprise saisie, au plus à la minute supérieure, tombe hors du temps
//! ouvré, le reste part de l'arrêt. Observé : `maryam24-git…` UID 2 (durée
//! 1 440 min, réel 1 439,9, `Stop` et `Resume` enregistrés à 17:00, fin
//! 17:00) ; une reprise ouvrée à la minute supérieure est suivie
//! (`SafiaAbdulRab…`, réel 3 623,9 min, reprise un dixième après l'arrêt).

use plannercraft_core::Minutes;
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

/// T 3 j (1 440), début réel lun 19/10 08:00, réel `tenths`, reprise `resume`.
fn started(tenths: i64, resume: &'static str) -> Net {
    Net::new("2026-10-19T08:00")
        .task("T", 1_440)
        .with_task("T", move |t| {
            t.progress.actual_start = Some(moment("2026-10-19T08:00"));
            t.progress.actual_duration = Minutes::from_tenths(tenths);
            t.progress.resume = Some(moment(resume));
        })
}

#[test]
fn r300_resume_at_the_next_minute_does_not_interrupt() {
    // Réel 1 439,9 : arrêt mer 16:59:54 ; reprise saisie mer 17:00 (minute
    // supérieure, hors temps ouvré) : le reste (0,1 min) part de l'arrêt,
    // fin mer 17:00, comme MS Project.
    let run = started(14_399, "2026-10-21T17:00").run();
    let p = run.at("T").progress.unwrap();
    assert_eq!(p.stop, moment("2026-10-21T16:59:54"));
    assert_eq!(p.resume, moment("2026-10-21T16:59:54"));
    assert_eq!(run.at("T").finish, moment("2026-10-21T17:00"));
}

#[test]
fn r300_other_resumes_unchanged() {
    // Reprise plus tardive (jeu 08:00) : elle est gardée, fin jeu 08:00:06.
    let run = started(14_399, "2026-10-22T08:00").run();
    assert_eq!(run.at("T").finish, moment("2026-10-22T08:00:06"));
    // Reprise ouvrée à la minute supérieure (réel 1 438,9, arrêt 16:58:54,
    // reprise 16:59, ouvrée) : suivie.
    let run = started(14_389, "2026-10-21T16:59").run();
    assert_eq!(run.at("T").progress.unwrap().resume, moment("2026-10-21T16:59"));
    // Arrêt à la minute entière (réel 1 439, arrêt 16:59) : la reprise du
    // jeudi est gardée, fin jeu 08:01 (R-204, inchangé).
    let run = started(14_390, "2026-10-22T08:00").run();
    assert_eq!(run.at("T").progress.unwrap().stop, moment("2026-10-21T16:59"));
    assert_eq!(run.at("T").finish, moment("2026-10-22T08:01"));
}
