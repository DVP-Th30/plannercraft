//! R-210 — Agrégats d'avancement d'une récapitulative et de l'UID 0
//! (amende R-055). Sur les feuilles actives planifiées descendantes : % de
//! R-055 (réel effectif de R-200) ; réel = `round_div(D × Σ réels, Σ D)`
//! (Σ D = 0 : `percent_of(D, %)`) ; reste = D − réel ; début réel = min des
//! débuts réels ; fin réelle = max des fins réelles si toutes sont
//! achevées ; `Stop` = min des `Stop` des feuilles commencées, `Resume` =
//! `Stop` ; % travail = `round_div(Σ travail réel × 10 000, Σ W)`, Σ W = 0 :
//! 0. Une récapitulative automatique commencée commence au plus tard à son
//! début réel agrégé (O40). Source : Q4, S26 ; O40 (A7 UID 0).

use plannercraft_core::{Diagnostic, Minutes, Percent, ProgressState, TaskUid, Work};
use plannercraft_mspdi::{ReadOptions, read};
use plannercraft_tests::net::Net;
use plannercraft_tests::{moment, tests_path};

use crate::r171_assignment_effective_calendar::{assign, resource};

fn pct(v: i64) -> Percent {
    Percent::new(v).unwrap()
}

/// S {A 960 achevée lun 08:00 – mar 17:00, B 480 non commencée, C 1 440 à
/// 33,33 % sans début réel}, toutes parallèles.
fn example() -> Net {
    Net::new("2026-10-19T08:00")
        .task("S", 0)
        .task("A", 960)
        .level("A", 2)
        .task("B", 480)
        .level("B", 2)
        .task("C", 1_440)
        .level("C", 2)
        .with_task("A", |t| {
            t.progress.actual_start = Some(moment("2026-10-19T08:00"));
            t.progress.actual_finish = Some(moment("2026-10-20T17:00"));
        })
        .with_task("C", |t| t.progress.percent_complete = pct(3_333))
}

#[test]
fn r210_summary_rollup() {
    // Σ réels = 960 + 0 + 480 = 1 440, Σ D = 2 880 ⇒ % 5 000 ; D(S) = 1 440
    // (lun 08:00 – mer 17:00) ⇒ réel 720, reste 720.
    let run = example().run();
    let s = run.at("S");
    assert_eq!(s.percent_complete, pct(5_000));
    let p = s.progress.unwrap();
    assert_eq!(p.state, ProgressState::Started);
    assert_eq!(p.actual_duration, Minutes::new(720));
    assert_eq!(p.remaining_duration, Minutes::new(720));
    assert_eq!(p.percent_complete, pct(5_000));
    assert_eq!(p.actual_start, moment("2026-10-19T08:00"));
    assert_eq!(p.actual_finish, None);
    // C commencée sans début réel : implicite = début au plus tôt du réseau.
    let c = run.at("C").progress.unwrap();
    assert_eq!(c.actual_start, moment("2026-10-19T08:00"));
    assert_eq!(c.actual_duration, Minutes::new(480));
    assert!(
        run.schedule
            .diagnostics
            .contains(&Diagnostic::ProgressActualStartMissing { task: run.uid("C") })
    );
    // B non commencée : pas d'avancement.
    assert_eq!(run.at("B").progress, None);
    // UID 0 : mêmes feuilles, même durée.
    let root = run.schedule.project_summary.unwrap().progress.unwrap();
    assert_eq!(root.actual_duration, Minutes::new(720));
    assert_eq!(root.percent_work_complete, Percent::ZERO);
}

#[test]
fn r210_zero_duration_children() {
    // S {M1 jalon achevé lun 10:00, M2 jalon non commencé} : Σ D = 0 ⇒
    // % = moyenne (10 000 + 0) / 2 = 5 000 ; réel = percent_of(D(S), %), D(S)
    // = 120 (lun 08:00 – 10:00) ⇒ 60.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .task("M1", 0)
        .level("M1", 2)
        .task("M2", 0)
        .level("M2", 2)
        .with_task("M1", |t| {
            t.progress.actual_start = Some(moment("2026-10-19T10:00"));
            t.progress.actual_finish = Some(moment("2026-10-19T10:00"));
        })
        .run();
    let s = run.at("S");
    assert_eq!(s.duration, Minutes::new(120));
    assert_eq!(s.percent_complete, pct(5_000));
    let p = s.progress.unwrap();
    assert_eq!(p.actual_duration, Minutes::new(60));
    assert_eq!(p.remaining_duration, Minutes::new(60));
    assert_eq!(p.actual_start, moment("2026-10-19T10:00"));
}

#[test]
fn r210_stop_min_of_started() {
    // S {A 1 j achevée lun 08:00 – 17:00 (R1, 8 h réalisées), B 4 h achevée
    // mar 08:00 – 12:00, C 1 j non commencée (R1, 8 h)} : `Stop` = min des
    // `Stop` = lun 17:00 = `Resume` ; % travail = 480 / 960 = 5 000 ; fin
    // réelle absente (C non achevée).
    let net = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .task("B", 240)
        .level("B", 2)
        .task("C", 480)
        .level("C", 2)
        .with_task("A", |t| {
            t.progress.actual_start = Some(moment("2026-10-19T08:00"));
            t.progress.actual_finish = Some(moment("2026-10-19T17:00"));
        })
        .with_task("B", |t| {
            t.progress.actual_start = Some(moment("2026-10-20T08:00"));
            t.progress.actual_finish = Some(moment("2026-10-20T12:00"));
        });
    let net = assign(assign(resource(net, 1, None), 1, "A", 1, 480), 2, "C", 1, 480).project(|p| {
        let a = p
            .assignments
            .get_mut(plannercraft_core::AssignmentUid::new(1))
            .unwrap();
        a.actual_work = Work::new(Minutes::new(480)).unwrap();
    });
    let run = net.run();
    let p = run.at("S").progress.unwrap();
    assert_eq!(
        (p.stop, p.resume),
        (moment("2026-10-19T17:00"), moment("2026-10-19T17:00"))
    );
    assert_eq!(p.percent_work_complete, pct(5_000));
    assert_eq!(p.actual_finish, None);
    assert_eq!(run.at("A").progress.unwrap().percent_work_complete, Percent::FULL);
    // Toutes achevées : fin réelle = la plus tardive (mar 12:00).
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
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
    let p = run.at("S").progress.unwrap();
    assert_eq!(p.state, ProgressState::Complete);
    assert_eq!(p.actual_finish, Some(moment("2026-10-20T12:00")));
    assert_eq!(p.stop, moment("2026-10-19T17:00"));
}

#[test]
fn r210_started_summary_starts_at_actual_start() {
    // P (automatique) {M manuelle saisie lun 09:00 – mer 17:00 {A 1 j
    // achevée lun 08:00 – 17:00}} : P et l'UID 0 commencent au début réel
    // agrégé (lun 08:00), au plus tôt et au plus tard aussi (O40).
    let run = Net::new("2026-10-19T08:00")
        .task("P", 0)
        .task("M", 0)
        .level("M", 2)
        .manual("M", Some("2026-10-19T09:00"), Some("2026-10-21T17:00"))
        .task("A", 480)
        .level("A", 3)
        .with_task("A", |t| {
            t.progress.actual_start = Some(moment("2026-10-19T08:00"));
            t.progress.actual_finish = Some(moment("2026-10-19T17:00"));
        })
        .run();
    assert_eq!(run.at("M").start, moment("2026-10-19T09:00"));
    for s in [*run.at("P"), run.schedule.project_summary.unwrap()] {
        assert_eq!(s.start, moment("2026-10-19T08:00"));
        assert_eq!(s.early.start, moment("2026-10-19T08:00"));
        assert_eq!(s.late.start, moment("2026-10-19T08:00"));
        assert_eq!(s.duration, Minutes::new(1_440));
        assert_eq!(s.total_slack, Minutes::ZERO);
    }
    // Sans avancement : P garde l'enveloppe (lun 09:00).
    let run = Net::new("2026-10-19T08:00")
        .task("P", 0)
        .task("M", 0)
        .level("M", 2)
        .manual("M", Some("2026-10-19T09:00"), Some("2026-10-21T17:00"))
        .task("A", 480)
        .level("A", 3)
        .run();
    assert_eq!(run.at("P").start, moment("2026-10-19T09:00"));
    assert_eq!(run.at("P").progress, None);
}

#[test]
fn r210_corpus_a7_project_summary() {
    // A7 (fichier du corpus, absent hors du job `corpus`), O40 : UID 0 à
    // 960 / 9 840 ⇒ 976 (écrit 10 %) ; réel round_div(7 920 × 960, 9 840) =
    // 773 (relu 772,7) ; % travail 22 h / 378 h ⇒ 582 (écrit 6 %) ; début
    // réel ven 04/10/2024 08:00 ; `Stop` = `Resume` = ven 20:00 ; pas de fin
    // réelle. UID 1 (manuelle) : réel 24 h, `Stop` ven 20:00.
    let Ok(bytes) = std::fs::read(tests_path("corpus/files/projinteg.xml")) else {
        return;
    };
    let d = read(&bytes, &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap();
    let s = plannercraft_core::schedule(&d.project);
    let root = s.project_summary.unwrap();
    assert_eq!(root.percent_complete, pct(976));
    assert_eq!(root.start, moment("2024-10-04T08:00"));
    let p = root.progress.unwrap();
    assert_eq!(p.actual_duration, Minutes::new(773));
    assert_eq!(p.percent_work_complete, pct(582));
    assert_eq!(p.actual_start, moment("2024-10-04T08:00"));
    assert_eq!(
        (p.stop, p.resume),
        (moment("2024-10-04T20:00"), moment("2024-10-04T20:00"))
    );
    assert_eq!(p.actual_finish, None);
    let u1 = s.scheduled(TaskUid::new(1)).unwrap().progress.unwrap();
    assert_eq!(u1.actual_duration, Minutes::new(1_440));
    assert_eq!(u1.stop, moment("2024-10-04T20:00"));
    assert_eq!(u1.actual_finish, Some(moment("2024-10-05T17:00")));
}
