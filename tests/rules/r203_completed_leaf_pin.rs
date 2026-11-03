//! R-203 — Feuille achevée : épingle réelle. Dates planifiées, au plus tôt
//! et au plus tard = [début réel, fin réelle] bruts (non recalés) ; liens
//! entrants, porte, contrainte, retard de nivellement et ALAP ignorés
//! (`progress.link_overridden` pour chaque lien dont la borne est violée,
//! jamais `link.manual_not_honored`) ; `Driver::Actual`,
//! `LateDriver::Actual` ; TS = FS = 0, jamais critique ; successeurs depuis
//! la fin réelle ; aucun plafond vers ses prédécesseurs. Source : S51, S20
//! (« A task stops being critical when it's completed, because it can no
//! longer affect the completion of successor tasks ») ; O37, O38 (A7 UID 2,
//! 3) ; reste : hypothèse.

use plannercraft_core::{
    Constraint, Diagnostic, Driver, LateDriver, LevelingDelay, LinkType, Minutes, ProgressState, Span,
};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

/// A 1 j ; B 2 j FS de A, achevée lun 13:00 – mar 12:00 (D 480) ; C 1 j
/// FS de B (exemple de la fiche).
fn example() -> Net {
    Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 480)
        .task("C", 480)
        .fs("A", "B")
        .fs("B", "C")
        .with_task("B", |t| {
            t.progress.actual_start = Some(moment("2026-10-19T13:00"));
            t.progress.actual_finish = Some(moment("2026-10-20T12:00"));
            t.progress.actual_duration = Minutes::new(480);
        })
}

#[test]
fn r203_completed_pins_actual_dates() {
    let run = example().run();
    let b = run.at("B");
    let actual = Span {
        start: moment("2026-10-19T13:00"),
        finish: moment("2026-10-20T12:00"),
    };
    assert_eq!((b.start, b.finish), (actual.start, actual.finish));
    assert_eq!(b.early, actual);
    assert_eq!(b.late, actual);
    assert_eq!(b.driver, Driver::Actual);
    assert_eq!(b.late_driver, LateDriver::Actual);
    assert_eq!(b.driver_bound, None);
    assert_eq!(b.duration, Minutes::new(480));
    let p = b.progress.unwrap();
    assert_eq!(p.state, ProgressState::Complete);
    assert_eq!(p.actual_start, actual.start);
    assert_eq!(p.actual_finish, Some(actual.finish));
    assert_eq!((p.stop, p.resume), (actual.finish, actual.finish));
    assert_eq!(p.remaining_duration, Minutes::ZERO);
    assert_eq!(p.percent_complete, plannercraft_core::Percent::FULL);
}

#[test]
fn r203_incoming_link_overridden() {
    // Borne de A → B : lun 17:00 > début réel lun 13:00 : le réel l'emporte,
    // `progress.link_overridden` (info), jamais `link.manual_not_honored`.
    let run = example().run();
    let d = &run.schedule.diagnostics;
    assert!(d.contains(&Diagnostic::ProgressLinkOverridden {
        predecessor: run.uid("A"),
        successor: run.uid("B"),
    }));
    assert!(!run.codes().contains(&"link.manual_not_honored"), "{d:?}");
    // Borne tenue (A achevée plus tôt, sans conflit) : aucun diagnostic.
    let run = example()
        .with_task("B", |t| {
            t.progress.actual_start = Some(moment("2026-10-20T08:00"));
        })
        .run();
    assert!(!run.codes().contains(&"progress.link_overridden"));
}

#[test]
fn r203_not_critical_zero_slack() {
    // B achevée : TS = FS = 0, non critique, même avec une longue tâche Z en
    // parallèle qui lui laisserait de la marge.
    let run = example().task("Z", 4_800).run();
    let b = run.at("B");
    assert_eq!(b.total_slack, Minutes::ZERO);
    assert_eq!(b.free_slack, Minutes::ZERO);
    assert!(!b.critical);
    assert!(run.at("Z").critical);
    // Sans Z : C finit le projet, critique.
    let run = example().run();
    assert!(!run.at("B").critical);
    assert!(run.at("C").critical);
}

#[test]
fn r203_no_ceiling_on_predecessor() {
    // A : fin au plus tard = fin du projet (mer 12:00), TS(A) = 720, et non
    // une marge négative tirée du début réel de B.
    let run = example().run();
    let a = run.at("A");
    assert_eq!(a.late.finish, moment("2026-10-21T12:00"));
    assert_eq!(a.total_slack, Minutes::new(720));
    assert_eq!(a.late_driver, LateDriver::ProjectFinish);
    assert!(!a.critical);
    assert_eq!(run.schedule.project_finish, moment("2026-10-21T12:00"));
}

#[test]
fn r203_successor_from_actual_finish() {
    // C part de la fin réelle de B (mar 12:00) : mar 13:00 – mer 12:00.
    let run = example().run();
    let c = run.at("C");
    assert_eq!(c.start, moment("2026-10-20T13:00"));
    assert_eq!(c.finish, moment("2026-10-21T12:00"));
    assert_eq!(
        c.driver,
        Driver::Link {
            predecessor: run.uid("B"),
            link: LinkType::FinishToStart,
        }
    );
    assert_eq!(c.total_slack, Minutes::ZERO);
}

#[test]
fn r203_raw_non_working_start_kept() {
    // Début réel sam 24/10 10:00 (chômé), fin réelle lun 26/10 12:00 :
    // gardés bruts (O37 : A7 UID 2 commence à un instant chômé). Contrainte
    // FNET, retard de nivellement et ALAP sont sans effet.
    let completed = || {
        Net::new("2026-10-19T08:00").task("T", 240).with_task("T", |t| {
            t.progress.actual_start = Some(moment("2026-10-24T10:00"));
            t.progress.actual_finish = Some(moment("2026-10-26T12:00"));
        })
    };
    let expected = Span {
        start: moment("2026-10-24T10:00"),
        finish: moment("2026-10-26T12:00"),
    };
    let variants: [&dyn Fn(Net) -> Net; 4] = [
        &|n| n,
        &|n| {
            n.constraint(
                "T",
                Constraint::FinishNoEarlierThan {
                    date: moment("2026-11-06T17:00"),
                },
            )
        },
        &|n| {
            n.with_task("T", |t| {
                t.leveling_delay = Some(LevelingDelay {
                    minutes: Minutes::new(2_400),
                    elapsed: false,
                    unit: plannercraft_core::DurationUnit::Day,
                });
            })
        },
        &|n| n.constraint("T", Constraint::AsLateAsPossible).task("Z", 4_800),
    ];
    for (k, f) in variants.iter().enumerate() {
        let run = f(completed()).run();
        let t = run.at("T");
        assert_eq!(t.early, expected, "variante {k}");
        assert_eq!(t.late, expected, "variante {k}");
        assert_eq!(
            (t.start, t.finish),
            (expected.start, expected.finish),
            "variante {k}"
        );
        assert!(!run.codes().contains(&"constraint.not_honored"), "variante {k}");
    }
    // Début réel absent : implicite = début au plus tôt du réseau (lun
    // 08:00), `progress.actual_start_missing`.
    let run = completed()
        .with_task("T", |t| t.progress.actual_start = None)
        .run();
    assert_eq!(run.at("T").start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("T").finish, moment("2026-10-26T12:00"));
    assert!(
        run.schedule
            .diagnostics
            .contains(&Diagnostic::ProgressActualStartMissing { task: run.uid("T") })
    );
}

#[test]
fn r203_from_finish_unchanged() {
    // R-207 : à rebours (ancre ven 23/10 17:00, tâches ALAP), l'épingle
    // réelle est la même ; elle ne plafonne pas son prédécesseur A, placé
    // sous la seule ancre (ven 08:00 – 17:00).
    let run = Net::new("2026-10-19T08:00")
        .from_finish("2026-10-23T17:00")
        .task("A", 480)
        .task("B", 480)
        .fs("A", "B")
        .with_task("B", |t| {
            t.progress.actual_start = Some(moment("2026-10-19T13:00"));
            t.progress.actual_finish = Some(moment("2026-10-20T12:00"));
        })
        .run();
    let b = run.at("B");
    assert_eq!(b.start, moment("2026-10-19T13:00"));
    assert_eq!(b.early.finish, moment("2026-10-20T12:00"));
    assert_eq!(b.late.finish, moment("2026-10-20T12:00"));
    assert_eq!(b.late_driver, LateDriver::Actual);
    assert_eq!(b.total_slack, Minutes::ZERO);
    assert_eq!(run.at("A").start, moment("2026-10-23T08:00"));
    assert_eq!(run.at("A").late_driver, LateDriver::ProjectFinish);
}

#[test]
fn r203_corpus_a7_completed_leaves() {
    // A7 (fichier du corpus, absent hors du job `corpus`), O37, O38 : UID 2
    // achevée ven 04/10/2024 08:00 (chômé) – sam 13:00, FNET sans effet ;
    // UID 3 FS de UID 2, réelle ven 09:00 – 20:00 : le réel l'emporte sur le
    // lien (`progress.link_overridden`) ; TS 0, non critiques.
    let Ok(bytes) = std::fs::read(plannercraft_tests::tests_path("corpus/files/projinteg.xml")) else {
        return;
    };
    let options = plannercraft_mspdi::ReadOptions::new(moment("2026-01-01T00:00"));
    let d = plannercraft_mspdi::read(&bytes, &options).unwrap();
    let s = plannercraft_core::schedule(&d.project);
    let uid = plannercraft_core::TaskUid::new;
    for (u, start, finish) in [
        (2, "2024-10-04T08:00", "2024-10-05T13:00"),
        (3, "2024-10-04T09:00", "2024-10-04T20:00"),
    ] {
        let t = s.scheduled(uid(u)).unwrap();
        let span = Span {
            start: moment(start),
            finish: moment(finish),
        };
        assert_eq!((t.start, t.finish), (span.start, span.finish), "UID {u}");
        assert_eq!((t.early, t.late), (span, span), "UID {u}");
        assert_eq!(t.total_slack, Minutes::ZERO, "UID {u}");
        assert!(!t.critical, "UID {u}");
    }
    assert!(s.diagnostics.contains(&Diagnostic::ProgressLinkOverridden {
        predecessor: uid(2),
        successor: uid(3),
    }));
}
