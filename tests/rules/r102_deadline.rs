//! R-102 — Échéance : plafond brut de la fin au plus tard (comme les autres
//! plafonds, avant `backward_span`), propagé aux prédécesseurs ; les dates
//! au plus tôt ne bougent pas ; `task.deadline_missed` si la fin planifiée
//! dépasse l'échéance en temps ouvré de l'échelle de placement. Sources :
//! S8 (« Deadline dates don't usually affect task scheduling »), S31, S32,
//! O16.

use plannercraft_core::{Diagnostic, LateDriver, Minutes, Pass};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

#[test]
fn r102_deadline_caps_the_late_finish() {
    // A 1 j, échéance mar 20/10 17:00, C 4 j seule : A lun 08:00 (au plus
    // tôt inchangé) ; LF(A) mar 17:00, cause l'échéance ; TF(A) 480 (1 440
    // sans échéance) ; aucun diagnostic.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("C", 1_920)
        .deadline("A", "2026-10-20T17:00")
        .run();
    let a = run.at("A");
    assert_eq!(a.early.start, moment("2026-10-19T08:00"));
    assert_eq!(a.late.finish, moment("2026-10-20T17:00"));
    assert_eq!(a.late_driver, LateDriver::Deadline);
    assert_eq!(a.total_slack, Minutes::new(480));
    assert_eq!(run.codes(), [""; 0]);
}

#[test]
fn r102_missed_deadline_and_negative_slack() {
    // P 2 j FS → A 1 j, échéance mar 20/10 17:00, C 4 j : A mer 08:00 –
    // 17:00 ; au plus tard lun – mar ; TF(A) −480, TF(P) −480 (propagée) ;
    // `task.deadline_missed { A, mar 17:00, mer 17:00 }`.
    let run = Net::new("2026-10-19T08:00")
        .task("P", 960)
        .task("A", 480)
        .task("C", 1_920)
        .fs("P", "A")
        .deadline("A", "2026-10-20T17:00")
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-21T08:00"));
    assert_eq!(run.at("A").total_slack, Minutes::new(-480));
    assert_eq!(run.at("P").total_slack, Minutes::new(-480));
    assert!(run.at("A").critical && run.at("P").critical);
    assert_eq!(
        run.schedule.diagnostics,
        [Diagnostic::TaskDeadlineMissed {
            task: run.uid("A"),
            deadline: moment("2026-10-20T17:00"),
            finish: moment("2026-10-21T17:00"),
        }]
    );
}

#[test]
fn r102_deadline_in_non_working_time_is_a_raw_cap() {
    // A 1 j, C 10 j seule (fin ven 30/10 17:00), échéance sam 24/10 10:00 :
    // LF(A) brut sam 10:00 → au plus tard ven 23/10 08:00 – 17:00, TF(A)
    // 1 920 ; une fin ven 17:00 ne dépasse pas sam 10:00 en temps ouvré.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("C", 4_800)
        .deadline("A", "2026-10-24T10:00")
        .run();
    assert_eq!(run.at("A").late.start, moment("2026-10-23T08:00"));
    assert_eq!(run.at("A").late.finish, moment("2026-10-23T17:00"));
    assert_eq!(run.at("A").total_slack, Minutes::new(1_920));
    assert_eq!(run.codes(), [""; 0]);
    // Fin planifiée égale à l'échéance en temps ouvré (ven 17:00 contre sam
    // 10:00) : pas de dépassement.
    let run = Net::new("2026-10-23T08:00")
        .task("A", 480)
        .deadline("A", "2026-10-24T10:00")
        .run();
    assert_eq!(run.codes(), [""; 0]);
}

#[test]
fn r102_deadline_out_of_the_domain_falls_back() {
    // Échéance au tout début du domaine (dim 01/01/1984 00:00) : aucune fin
    // au plus tard ne se place avant elle (R-069, passe arrière) → dates au
    // plus tard de repli (= au plus tôt), cause `Fallback`, dégradée,
    // `task.unschedulable { pass: Backward }`, et l'échéance est dépassée.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .deadline("A", "1984-01-01T00:00")
        .run();
    let a = run.at("A");
    assert_eq!(a.late, a.early);
    assert_eq!(a.late_driver, LateDriver::Fallback);
    assert!(a.degraded);
    assert!(run.schedule.diagnostics.iter().any(|d| matches!(
        d,
        Diagnostic::TaskUnschedulable {
            pass: Pass::Backward,
            ..
        }
    )));
    assert!(run.codes().contains(&"task.deadline_missed"));
}
