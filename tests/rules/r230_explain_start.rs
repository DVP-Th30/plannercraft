//! R-230 — Explication du début (lot L7) : étapes dans l'ordre `Driver`
//! (pilote, borne brute `driver_bound`), `Snap` si la borne brute diffère
//! du début au plus tôt (calendrier de placement cité), `LevelingDelay` si
//! le début planifié diffère du début au plus tôt (R-178), `Actual` puis
//! `Resume` (feuille en cours, cause de la reprise), `ConstraintNotBinding`
//! pour une contrainte datée qui ne pilote pas, `Degraded` pour une tâche
//! de repli. Lecture seule du `Schedule` : aucun recalcul. Source : brief
//! § 6 ; R-044 ; hypothèse.

#![allow(
    clippy::many_single_char_names,
    clippy::too_many_lines,
    reason = "tests : noms des tâches des exemples (A, B, C…)"
)]

use plannercraft_core::schedule::explain::{ResumeCause, Side, Step, explain};
use plannercraft_core::{
    CalendarUid, Constraint, ConstraintKind, Driver, LevelingDelay, LinkType, Minutes, Project, Schedule,
    TaskUid,
};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

use crate::r204_in_progress_leaf::started;

/// Projet de `net` et son planning, avec les UID des tâches `names`.
pub(crate) fn plan<const N: usize>(net: Net, names: [&str; N]) -> (Project, Schedule, [TaskUid; N]) {
    let uids = names.map(|n| net.uid(n));
    let p = net.build();
    let s = plannercraft_core::schedule(&p);
    (p, s, uids)
}

/// A 1 j lun ; B 2 j lun-mar ; C 1 j FS de A et de B (exemple de R-230).
pub(crate) fn abc() -> Net {
    Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 960)
        .task("C", 480)
        .fs("A", "C")
        .fs("B", "C")
}

#[test]
fn r230_link_driver_snap() {
    // C mer 21/10 08:00 : pilote `Link{B, FS}`, brut mar 17:00, recalé mer
    // 08:00 sur Standard (R-024) ; B sans lien : début du projet, pas de
    // recalage.
    let (p, s, [b, c]) = plan(abc(), ["B", "C"]);
    let x = explain(&p, &s, c, Side::Start, 5).unwrap();
    assert_eq!(x.date, moment("2026-10-21T08:00"));
    assert_eq!(
        x.steps,
        vec![
            Step::Driver {
                driver: Driver::Link {
                    predecessor: b,
                    link: LinkType::FinishToStart,
                },
                raw: Some(moment("2026-10-20T17:00")),
            },
            Step::Snap {
                raw: moment("2026-10-20T17:00"),
                snapped: moment("2026-10-21T08:00"),
                calendar: Some(CalendarUid::new(1)),
            },
        ]
    );
    let x = explain(&p, &s, b, Side::Start, 5).unwrap();
    assert_eq!(
        x.steps,
        vec![Step::Driver {
            driver: Driver::ProjectStart,
            raw: Some(moment("2026-10-19T08:00")),
        }]
    );
    // Tâche sur un calendrier propre (24 heures) : la borne lun 17:00 est
    // un instant ouvré, aucun recalage.
    let (p, s, [e]) = plan(
        Net::new("2026-10-19T08:00")
            .task("A", 480)
            .task("E", 480)
            .calendar("E", 2)
            .fs("A", "E"),
        ["E"],
    );
    let x = explain(&p, &s, e, Side::Start, 5).unwrap();
    assert_eq!(x.steps.len(), 1);
    assert_eq!(x.date, moment("2026-10-19T17:00"));
}

#[test]
fn r230_leveling_delay_step() {
    // L 1 j FS de A, retardée de 1 j par nivellement : borne lun 17:00,
    // recalée mar 08:00 (début au plus tôt), retardée à mer 08:00.
    let (p, s, [a, l]) = plan(
        Net::new("2026-10-19T08:00")
            .task("A", 480)
            .task("L", 480)
            .fs("A", "L")
            .with_task("L", |t| {
                t.leveling_delay = Some(LevelingDelay {
                    minutes: Minutes::new(480),
                    unit: plannercraft_core::DurationUnit::Day,
                    elapsed: false,
                });
            }),
        ["A", "L"],
    );
    let x = explain(&p, &s, l, Side::Start, 5).unwrap();
    assert_eq!(x.date, moment("2026-10-21T08:00"));
    let delay = p.tasks.get(l).unwrap().leveling_delay.unwrap();
    assert_eq!(
        x.steps,
        vec![
            Step::Driver {
                driver: Driver::Link {
                    predecessor: a,
                    link: LinkType::FinishToStart,
                },
                raw: Some(moment("2026-10-19T17:00")),
            },
            Step::Snap {
                raw: moment("2026-10-19T17:00"),
                snapped: moment("2026-10-20T08:00"),
                calendar: Some(CalendarUid::new(1)),
            },
            Step::LevelingDelay {
                delay,
                from: moment("2026-10-20T08:00"),
                to: moment("2026-10-21T08:00"),
            },
        ]
    );
}

#[test]
fn r230_actual_and_resume() {
    // B achevée de R-203 (début réel lun 13:00, fin réelle mar 12:00) :
    // `Actual`, sans recalage ni reprise.
    let (p, s, [b]) = plan(
        Net::new("2026-10-19T08:00")
            .task("A", 480)
            .task("B", 960)
            .fs("A", "B")
            .with_task("B", |t| {
                t.progress.actual_start = Some(moment("2026-10-19T13:00"));
                t.progress.actual_finish = Some(moment("2026-10-20T12:00"));
            }),
        ["B"],
    );
    let x = explain(&p, &s, b, Side::Start, 5).unwrap();
    assert_eq!(
        x.steps,
        vec![
            Step::Driver {
                driver: Driver::Actual,
                raw: None,
            },
            Step::Actual {
                start: moment("2026-10-19T13:00"),
                finish: Some(moment("2026-10-20T12:00")),
            },
        ]
    );
    // T de R-205 (P 2 j FS → T commencée lun 08:00 à 3 333) : reprise mer
    // 08:00, causée par le lien depuis P.
    let (p, s, [pp, t]) = plan(started(3_333).task("P", 960).fs("P", "T"), ["P", "T"]);
    let x = explain(&p, &s, t, Side::Start, 5).unwrap();
    assert_eq!(x.date, moment("2026-10-19T08:00"));
    assert_eq!(
        x.steps,
        vec![
            Step::Driver {
                driver: Driver::Actual,
                raw: None,
            },
            Step::Actual {
                start: moment("2026-10-19T08:00"),
                finish: None,
            },
            Step::Resume {
                stop: moment("2026-10-19T17:00"),
                resume: moment("2026-10-21T08:00"),
                cause: ResumeCause::Link {
                    predecessor: pp,
                    link: LinkType::FinishToStart,
                },
            },
        ]
    );
    let resume_cause = |x: &plannercraft_core::schedule::explain::Explanation| {
        x.steps.iter().find_map(|s| match s {
            Step::Resume { cause, .. } => Some(*cause),
            _ => None,
        })
    };
    // Sans lien : instant ouvré suivant `Stop`.
    let (p, s, [t]) = plan(started(3_333), ["T"]);
    let x = explain(&p, &s, t, Side::Start, 5).unwrap();
    assert_eq!(resume_cause(&x), Some(ResumeCause::NextWorking));
    // Reprise saisie jeu 08:00 (R-204).
    let (p, s, [t]) = plan(
        started(3_333).with_task("T", |t| t.progress.resume = Some(moment("2026-10-22T08:00"))),
        ["T"],
    );
    let x = explain(&p, &s, t, Side::Start, 5).unwrap();
    assert_eq!(resume_cause(&x), Some(ResumeCause::Entered));
    // SNET mer 08:00 : plancher de la reprise (R-205).
    let (p, s, [t]) = plan(
        started(3_333).constraint(
            "T",
            Constraint::StartNoEarlierThan {
                date: moment("2026-10-21T08:00"),
            },
        ),
        ["T"],
    );
    let x = explain(&p, &s, t, Side::Start, 5).unwrap();
    assert_eq!(
        resume_cause(&x),
        Some(ResumeCause::Constraint(ConstraintKind::StartNoEarlierThan))
    );
    assert!(
        !x.steps
            .iter()
            .any(|s| matches!(s, Step::ConstraintNotBinding { .. }))
    );
    // Fractionnement non permis : le lien ne joue pas (R-205).
    let (p, s, [t]) = plan(
        started(3_333)
            .task("P", 960)
            .fs("P", "T")
            .options(|o| o.progress.split_in_progress = false),
        ["T"],
    );
    let x = explain(&p, &s, t, Side::Start, 5).unwrap();
    assert_eq!(resume_cause(&x), Some(ResumeCause::NextWorking));
    // FF : P 5 j FF → T, le reste finit avec P : reprise jeu 08:00.
    let (p, s, [pp, t]) = plan(
        started(3_333).task("P", 2_400).link(
            "P",
            "T",
            LinkType::FinishToFinish,
            plannercraft_core::Lag::ZERO,
        ),
        ["P", "T"],
    );
    let x = explain(&p, &s, t, Side::Start, 5).unwrap();
    assert_eq!(
        resume_cause(&x),
        Some(ResumeCause::Link {
            predecessor: pp,
            link: LinkType::FinishToFinish,
        })
    );
    // Relecture finale J6 (J6-EXPLAIN-1) : toutes les bornes du réseau
    // (R-205). P 2 j lun-mar FS → récapitulative S, T dans S commencée lun
    // 08:00 à 3 333 : la porte de S (fin de P, mar 17:00) cale la reprise
    // mer 08:00.
    let in_summary = || {
        Net::new("2026-10-19T08:00")
            .task("P", 960)
            .task("S", 0)
            .task("T", 1_440)
            .level("T", 2)
            .fs("P", "S")
            .with_task("T", |t| {
                t.progress.actual_start = Some(moment("2026-10-19T08:00"));
                t.progress.percent_complete = plannercraft_core::Percent::new(3_333).unwrap();
            })
    };
    let (p, s, [sum, t]) = plan(in_summary(), ["S", "T"]);
    let x = explain(&p, &s, t, Side::Start, 5).unwrap();
    assert_eq!(
        x.steps.last(),
        Some(&Step::Resume {
            stop: moment("2026-10-19T17:00"),
            resume: moment("2026-10-21T08:00"),
            cause: ResumeCause::SummaryGate { summary: sum },
        })
    );
    // Un lien propre de T à égalité avec la porte : la porte, premier terme
    // du moteur, est gardée.
    let (p, s, [sum, t]) = plan(in_summary().task("Q", 960).fs("Q", "T"), ["S", "T"]);
    let x = explain(&p, &s, t, Side::Start, 5).unwrap();
    assert_eq!(resume_cause(&x), Some(ResumeCause::SummaryGate { summary: sum }));
    // Porte d'un ancêtre : S0 ⊃ S ⊃ T, lien FS P → S0 ; la cause cite la
    // récapitulative parente S, par laquelle passe la borne.
    let (p, s, [sum, t]) = plan(
        Net::new("2026-10-19T08:00")
            .task("P", 960)
            .task("S0", 0)
            .task("S", 0)
            .level("S", 2)
            .task("T", 1_440)
            .level("T", 3)
            .fs("P", "S0")
            .with_task("T", |t| {
                t.progress.actual_start = Some(moment("2026-10-19T08:00"));
                t.progress.percent_complete = plannercraft_core::Percent::new(3_333).unwrap();
            }),
        ["S", "T"],
    );
    let x = explain(&p, &s, t, Side::Start, 5).unwrap();
    assert_eq!(resume_cause(&x), Some(ResumeCause::SummaryGate { summary: sum }));
    assert_eq!(
        s.scheduled(t).and_then(|g| g.progress).map(|g| g.resume),
        Some(moment("2026-10-21T08:00"))
    );
    // FF P 5 j → récapitulative S, déplié sur T (R-058) : reprise jeu
    // 08:00, causée par le lien.
    let (p, s, [pp, t]) = plan(
        Net::new("2026-10-19T08:00")
            .task("P", 2_400)
            .task("S", 0)
            .task("T", 1_440)
            .level("T", 2)
            .link("P", "S", LinkType::FinishToFinish, plannercraft_core::Lag::ZERO)
            .with_task("T", |t| {
                t.progress.actual_start = Some(moment("2026-10-19T08:00"));
                t.progress.percent_complete = plannercraft_core::Percent::new(3_333).unwrap();
            }),
        ["P", "T"],
    );
    let x = explain(&p, &s, t, Side::Start, 5).unwrap();
    assert_eq!(
        resume_cause(&x),
        Some(ResumeCause::Link {
            predecessor: pp,
            link: LinkType::FinishToFinish,
        })
    );
    // Début du projet mar 13:00, après `Stop` (lun 17:00) : il cale la
    // reprise.
    let (p, s, [t]) = plan(
        started(3_333).project(|p| {
            p.anchor = plannercraft_core::ScheduleAnchor::FromStart(moment("2026-10-20T13:00"));
        }),
        ["T"],
    );
    let x = explain(&p, &s, t, Side::Start, 5).unwrap();
    assert_eq!(
        x.steps.last(),
        Some(&Step::Resume {
            stop: moment("2026-10-19T17:00"),
            resume: moment("2026-10-20T13:00"),
            cause: ResumeCause::ProjectStart,
        })
    );
}

#[test]
fn r230_constraint_not_binding() {
    // C SNET lun 19/10 10:00, FS de A (fin lun 17:00) : le lien pilote, la
    // contrainte ne lie pas. D SNET mer 10:30 : la contrainte pilote.
    let (p, s, [c, d]) = plan(
        Net::new("2026-10-19T08:00")
            .task("A", 480)
            .task("C", 480)
            .fs("A", "C")
            .constraint(
                "C",
                Constraint::StartNoEarlierThan {
                    date: moment("2026-10-19T10:00"),
                },
            )
            .task("D", 480)
            .constraint(
                "D",
                Constraint::StartNoEarlierThan {
                    date: moment("2026-10-21T10:30"),
                },
            ),
        ["C", "D"],
    );
    let x = explain(&p, &s, c, Side::Start, 5).unwrap();
    assert_eq!(
        x.steps.last(),
        Some(&Step::ConstraintNotBinding {
            kind: ConstraintKind::StartNoEarlierThan,
            date: moment("2026-10-19T10:00"),
        })
    );
    let x = explain(&p, &s, d, Side::Start, 5).unwrap();
    assert_eq!(
        x.steps,
        vec![Step::Driver {
            driver: Driver::Constraint {
                kind: ConstraintKind::StartNoEarlierThan,
            },
            raw: Some(moment("2026-10-21T10:30")),
        }]
    );
}
