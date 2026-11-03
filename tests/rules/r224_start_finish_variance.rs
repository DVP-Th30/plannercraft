//! R-224 — Écarts de début et de fin. Écart = valeur − ligne de base, en
//! minutes ouvrées signées sur l'échelle de mesure de la tâche
//! (`measure_calendar`, R-113 ; minutes civiles pour une feuille à durée
//! écoulée) ; sans ligne de base, ou sans date de base : 0. Source : S59
//! (« Start Variance = Start - Baseline Start », « Finish Variance =
//! Finish - Baseline Finish », « contains "0 days" until… ») ; échelle :
//! hypothèse.

use plannercraft_core::baseline::{self, Rollup, Scope};
use plannercraft_core::{CalendarSet, Constraint, Minutes, TaskBaseline, TaskUid};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

use crate::r220_baseline_values::{number, plan, set, spec};

/// Écarts (début, fin) de la tâche `uid` à la ligne 0.
fn start_finish(p: &plannercraft_core::Project, s: &plannercraft_core::Schedule, uid: TaskUid) -> (i64, i64) {
    let (cals, _) = CalendarSet::compile(p);
    let v = baseline::variance(p, s, &cals, number(0));
    let t = v.tasks.iter().find(|t| t.uid == uid).unwrap();
    (t.variance.start.get(), t.variance.finish.get())
}

#[test]
fn r224_start_variance_working_minutes() {
    // T 1 j, ligne 0 lun 19/10 08:00 – 17:00 ; puis SNET mar 20/10 13:00 ⇒
    // T mar 13:00 – mer 12:00 : écart de début = W(lun 08:00 → mar 13:00) =
    // 480 + 240 = +720 ; de fin = W(lun 17:00 → mer 12:00) = 480 + 240 =
    // +720. Avant décalage : 0.
    let net = Net::new("2026-10-19T08:00").task("T", 480);
    let t = net.uid("T");
    let (mut p, s) = plan(net);
    let _ = set(&mut p, &s, &spec(0, Scope::Project, Rollup::None, false));
    assert_eq!(start_finish(&p, &s, t), (0, 0));
    p.tasks.get_mut(t).unwrap().constraint = Constraint::StartNoEarlierThan {
        date: moment("2026-10-20T13:00"),
    };
    let s = plannercraft_core::schedule(&p);
    assert_eq!(s.scheduled(t).unwrap().start, moment("2026-10-20T13:00"));
    assert_eq!(start_finish(&p, &s, t), (720, 720));
}

#[test]
fn r224_negative_variance() {
    // Projet au ven 16/10 08:00, T 1 j ven 08:00 – 17:00 ; ligne 0 relue au
    // lun 19/10 08:00 – 17:00 ⇒ écarts −480 (le week-end ne compte pas).
    let net = Net::new("2026-10-16T08:00").task("T", 480);
    let t = net.uid("T");
    let (mut p, s) = plan(net);
    p.tasks.get_mut(t).unwrap().baselines.insert(
        number(0),
        TaskBaseline {
            start: Some(moment("2026-10-19T08:00")),
            finish: Some(moment("2026-10-19T17:00")),
            ..TaskBaseline::default()
        },
    );
    assert_eq!(start_finish(&p, &s, t), (-480, -480));
}

#[test]
fn r224_elapsed_leaf_civil_minutes() {
    // Feuille à durée écoulée E (1 440 min civiles) lun 08:00 ; ligne 0 au
    // dim 18/10 08:00 ⇒ +1 440 minutes civiles (0 en minutes ouvrées).
    let net = Net::new("2026-10-19T08:00").elapsed("E", 1_440);
    let e = net.uid("E");
    let (mut p, s) = plan(net);
    p.tasks.get_mut(e).unwrap().baselines.insert(
        number(0),
        TaskBaseline {
            start: Some(moment("2026-10-18T08:00")),
            ..TaskBaseline::default()
        },
    );
    // Fin de base absente : écart de fin 0.
    assert_eq!(start_finish(&p, &s, e), (1_440, 0));
}

#[test]
fn r224_no_baseline_zero() {
    // Sans ligne de base : début et fin 0 (O43), ligne absente.
    let net = Net::new("2026-10-19T08:00").task("T", 480);
    let t = net.uid("T");
    let (p, s) = plan(net);
    let (cals, _) = CalendarSet::compile(&p);
    let v = baseline::variance(&p, &s, &cals, number(0));
    let tv = v.tasks.iter().find(|x| x.uid == t).unwrap();
    assert_eq!(tv.baseline, None);
    assert_eq!(
        (tv.variance.start, tv.variance.finish),
        (Minutes::ZERO, Minutes::ZERO)
    );
    assert_eq!(v.project.uid, TaskUid::PROJECT_SUMMARY);
}
