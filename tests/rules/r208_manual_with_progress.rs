//! R-208 — Tâche manuelle avec avancement (amende R-063, R-064). Feuille
//! manuelle : le début réel remplace le début saisi, la fin réelle la fin
//! saisie ; achevée, c'est l'épingle réelle de R-203 (dates au plus tard =
//! réelles). Récapitulative manuelle : dates saisies gardées (R-064),
//! agrégats de R-210 calculés, durée réelle = `percent_of(durée utilisée,
//! %)`. Source : O41 (A7 UID 1 et 4) ; hypothèse (UID 4 ne discrimine pas).

use plannercraft_core::{Diagnostic, Driver, LateDriver, Minutes, Percent, ProgressState};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

#[test]
fn r208_manual_leaf_actual_dates() {
    // M manuelle saisie mer 21/10 08:00 – jeu 17:00, début réel mar 13:00,
    // 50 % ⇒ M mar 13:00 – jeu 17:00, durée utilisée 1 200.
    let run = Net::new("2026-10-19T08:00")
        .task("M", 960)
        .manual("M", Some("2026-10-21T08:00"), Some("2026-10-22T17:00"))
        .with_task("M", |t| {
            t.progress.actual_start = Some(moment("2026-10-20T13:00"));
            t.progress.percent_complete = Percent::new(5_000).unwrap();
        })
        .run();
    let m = run.at("M");
    assert!(m.manual);
    assert_eq!(m.driver, Driver::Manual);
    assert_eq!(m.start, moment("2026-10-20T13:00"));
    assert_eq!(m.finish, moment("2026-10-22T17:00"));
    assert_eq!(m.duration, Minutes::new(1_200));
    let p = m.progress.unwrap();
    assert_eq!(p.state, ProgressState::Started);
    assert_eq!(p.actual_start, moment("2026-10-20T13:00"));

    // Achevée (début réel lun 08:00, fin réelle mar 12:00) : épingle réelle,
    // dates au plus tard = réelles, TS 0 ; le lien violé donne
    // `progress.link_overridden`, jamais `link.manual_not_honored`.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("M", 960)
        .manual("M", Some("2026-10-21T08:00"), Some("2026-10-22T17:00"))
        .fs("A", "M")
        .task("Z", 2_400)
        .with_task("M", |t| {
            t.progress.actual_start = Some(moment("2026-10-19T08:00"));
            t.progress.actual_finish = Some(moment("2026-10-20T12:00"));
        })
        .run();
    let m = run.at("M");
    assert!(m.manual);
    assert_eq!(m.driver, Driver::Actual);
    assert_eq!(m.late_driver, LateDriver::Actual);
    assert_eq!(
        (m.start, m.finish),
        (moment("2026-10-19T08:00"), moment("2026-10-20T12:00"))
    );
    assert_eq!(m.late, m.early);
    assert_eq!(m.total_slack, Minutes::ZERO);
    assert!(!m.critical);
    assert!(
        run.schedule
            .diagnostics
            .contains(&Diagnostic::ProgressLinkOverridden {
                predecessor: run.uid("A"),
                successor: run.uid("M"),
            })
    );
    assert!(!run.codes().contains(&"link.manual_not_honored"));
}

#[test]
fn r208_manual_summary_keeps_entered() {
    // S manuelle saisie lun 09:00 – mer 17:00 {A 1 j achevée lun 08:00 –
    // 17:00 ; B 4 h achevée mar 08:00 – 12:00} : S garde lun 09:00 (O41 :
    // A7 UID 1, `Start` 09:00 ≠ `ActualStart` 08:00) ; début réel agrégé lun
    // 08:00 ; 100 % ; réel = percent_of(durée saisie de S, 1 380, 100 %) ;
    // fin réelle mar 12:00 ; `Stop` = plus petit `Stop` des feuilles = lun
    // 17:00.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 1_380)
        .manual("S", Some("2026-10-19T09:00"), Some("2026-10-21T17:00"))
        .task("A", 480)
        .level("A", 2)
        .task("B", 240)
        .level("B", 2)
        .with_task("A", |t| {
            t.progress.actual_start = Some(moment("2026-10-19T08:00"));
            t.progress.actual_finish = Some(moment("2026-10-19T17:00"));
        })
        .with_task("B", |t| {
            t.progress.actual_start = Some(moment("2026-10-20T08:00"));
            t.progress.actual_finish = Some(moment("2026-10-20T12:00"));
        })
        .run();
    let s = run.at("S");
    assert!(s.manual);
    assert_eq!(s.start, moment("2026-10-19T09:00"));
    assert_eq!(s.duration, Minutes::new(1_380));
    assert_eq!(s.percent_complete, Percent::FULL);
    let p = s.progress.unwrap();
    assert_eq!(p.state, ProgressState::Complete);
    assert_eq!(p.actual_start, moment("2026-10-19T08:00"));
    assert_eq!(p.actual_finish, Some(moment("2026-10-20T12:00")));
    assert_eq!(p.actual_duration, Minutes::new(1_380));
    assert_eq!(p.remaining_duration, Minutes::ZERO);
    assert_eq!(
        (p.stop, p.resume),
        (moment("2026-10-19T17:00"), moment("2026-10-19T17:00"))
    );
    // L'UID 0 (automatique) commence au début réel agrégé (O40).
    let ps = run.schedule.project_summary.unwrap();
    assert_eq!(ps.start, moment("2026-10-19T08:00"));
    assert_eq!(ps.early.start, moment("2026-10-19T08:00"));
    assert_eq!(ps.late.start, moment("2026-10-19T08:00"));
}
