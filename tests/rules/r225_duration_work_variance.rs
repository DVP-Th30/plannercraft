//! R-225 — Écarts de durée et de travail ; projet, ressources,
//! affectations. Durée : D calculée − D de base ; travail : W − W de base,
//! une valeur de base absente comptant 0 (donc W sans ligne de base, O43) ;
//! projet = UID 0 ; ressource : W − W de base ; affectation : W − W de
//! base. Source : S59 (« Work Variance = Work - Baseline Work », « Duration
//! Variance = Duration - Baseline Duration ») ; O43 (A4, A6).

use plannercraft_core::baseline::{self, Rollup, Scope};
use plannercraft_core::{AssignmentUid, CalendarSet, Minutes, ResourceUid, TaskBaseline, TaskUid};
use plannercraft_mspdi::{ReadOptions, read};
use plannercraft_tests::net::Net;
use plannercraft_tests::{moment, tests_path};

use crate::r171_assignment_effective_calendar::{assign, assign_full, resource};
use crate::r220_baseline_values::{number, plan, set, spec, with_root, work};

/// T 5 j affectée à R1 pour 2 400 min (affectation 1), sous l'UID 0.
fn example(w: i64) -> Net {
    let net = with_root(Net::new("2026-10-19T08:00").task("T", 2_400));
    assign(resource(net, 1, None), 1, "T", 1, w)
}

#[test]
fn r225_work_variance_without_baseline() {
    // Sans ligne : travail = W, durée = D (base absente = 0), dates 0.
    let net = example(2_400);
    let t = net.uid("T");
    let (p, s) = plan(net);
    let (cals, _) = CalendarSet::compile(&p);
    let v = baseline::variance(&p, &s, &cals, number(0));
    let tv = v.tasks.iter().find(|x| x.uid == t).unwrap();
    assert_eq!(tv.variance.work, Minutes::new(2_400));
    assert_eq!(tv.variance.duration, Minutes::new(2_400));
    assert_eq!(v.project.variance.work, Minutes::new(2_400));
    assert_eq!(v.resources, vec![(ResourceUid::new(1), Minutes::new(2_400))]);
    assert_eq!(v.assignments, vec![(AssignmentUid::new(1), Minutes::new(2_400))]);
}

#[test]
fn r225_work_and_duration_variance() {
    // Ligne 0 posée à 40 h (2 400) ; travail porté à 48 h (2 880) sur 5 j ⇒
    // écart de travail +480 (tâche, projet, ressource, affectation) ; durée
    // de base 2 400, durée 6 j (2 880 à 100 %) ⇒ +480.
    let net = example(2_400);
    let t = net.uid("T");
    let (mut p, s) = plan(net);
    let _ = set(&mut p, &s, &spec(0, Scope::Project, Rollup::None, false));
    p.assignments.get_mut(AssignmentUid::new(1)).unwrap().work = work(2_880);
    let s = plannercraft_core::schedule(&p);
    let (cals, _) = CalendarSet::compile(&p);
    let v = baseline::variance(&p, &s, &cals, number(0));
    let tv = v.tasks.iter().find(|x| x.uid == t).unwrap();
    assert_eq!(s.scheduled(t).unwrap().work, work(2_880));
    assert_eq!(tv.variance.work, Minutes::new(480));
    assert_eq!(tv.variance.duration, Minutes::new(480));
    assert_eq!(tv.variance.start, Minutes::ZERO);
    assert_eq!(tv.variance.finish, Minutes::new(480));
    assert_eq!(v.project.variance.work, Minutes::new(480));
    assert_eq!(v.resources, vec![(ResourceUid::new(1), Minutes::new(480))]);
    assert_eq!(v.assignments, vec![(AssignmentUid::new(1), Minutes::new(480))]);
    // Ligne de base de travail seul : durée de base absente = 0.
    p.tasks.get_mut(t).unwrap().baselines.insert(
        number(2),
        TaskBaseline {
            work: Some(work(3_000)),
            start: Some(moment("2026-10-19T08:00")),
            ..TaskBaseline::default()
        },
    );
    let v = baseline::variance(&p, &s, &cals, number(2));
    let tv = v.tasks.iter().find(|x| x.uid == t).unwrap();
    assert_eq!(tv.variance.work, Minutes::new(-120));
    assert_eq!(tv.variance.duration, Minutes::new(2_880));
    assert_eq!(v.project.baseline, None);
    assert_eq!(v.project.uid, TaskUid::PROJECT_SUMMARY);
}

#[test]
fn r225_corpus_a4_without_baseline() {
    // A4 (fichier du corpus, absent hors du job `corpus`), O43 : aucune
    // ligne de base ; `WorkVariance` = `Work` × 1000 : UID 1, 180 h ⇒
    // 10 800 ; ressource R1, 524 h ⇒ 31 440 ; écarts de dates nuls.
    let Ok(bytes) = std::fs::read(tests_path("corpus/files/shopmetrics.xml")) else {
        return;
    };
    let d = read(&bytes, &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap();
    let s = plannercraft_core::schedule(&d.project);
    let (cals, _) = CalendarSet::compile(&d.project);
    let v = baseline::variance(&d.project, &s, &cals, number(0));
    let u1 = v.tasks.iter().find(|x| x.uid == TaskUid::new(1)).unwrap();
    assert_eq!(u1.variance.work, Minutes::new(10_800));
    assert!(v.tasks.iter().all(|t| t.baseline.is_none()
        && t.variance.start == Minutes::ZERO
        && t.variance.finish == Minutes::ZERO));
    assert!(v.resources.contains(&(ResourceUid::new(1), Minutes::new(31_440))));
}

#[test]
fn r225_corpus_a7_recorded_baseline_matches() {
    // A7 (fichier du corpus), O42 : ligne de base 0 égale aux valeurs
    // courantes ⇒ écarts de début, de fin et de travail nuls sur toutes les
    // tâches, l'UID 0, les ressources et les affectations.
    let Ok(bytes) = std::fs::read(tests_path("corpus/files/projinteg.xml")) else {
        return;
    };
    let d = read(&bytes, &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap();
    let s = plannercraft_core::schedule(&d.project);
    let (cals, _) = CalendarSet::compile(&d.project);
    let v = baseline::variance(&d.project, &s, &cals, number(0));
    for t in v.tasks.iter().chain([&v.project]) {
        assert!(t.baseline.is_some(), "UID {} sans ligne", t.uid);
        assert_eq!(
            (t.variance.start, t.variance.finish, t.variance.work),
            (Minutes::ZERO, Minutes::ZERO, Minutes::ZERO),
            "UID {}",
            t.uid
        );
    }
    assert!(
        v.resources.iter().all(|(_, w)| *w == Minutes::ZERO),
        "{:?}",
        v.resources
    );
    assert!(
        v.assignments.iter().all(|(_, w)| *w == Minutes::ZERO),
        "{:?}",
        v.assignments
    );
}

#[test]
fn r225_fictive_assignment_zero() {
    // Affectation fictive (sans ressource, −65 535 relue) de 480 min : écart
    // de travail 0 et travail de base absent, comme les 75 affectations
    // fictives de travail non nul du corpus (`WorkVariance` 0, A7 sans
    // `Work` de base).
    let net = assign_full(
        Net::new("2026-10-19T08:00").task("T", 480),
        1,
        "T",
        None,
        (480, 10_000, 0),
    );
    let (mut p, s) = plan(net);
    let (cals, _) = CalendarSet::compile(&p);
    let v = baseline::variance(&p, &s, &cals, number(0));
    assert_eq!(v.assignments, vec![(AssignmentUid::new(1), Minutes::ZERO)]);
    let edit = set(&mut p, &s, &spec(0, Scope::Project, Rollup::None, false));
    assert_eq!(edit.assignments.get(&AssignmentUid::new(1)).unwrap().work, None);
}
