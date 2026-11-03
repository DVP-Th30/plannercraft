//! R-055 — % achevé d'une récapitulative : `round_div(Σ réel × 10 000,
//! Σ durée)` sur les feuilles actives planifiées descendantes (récapitulatives
//! intermédiaires exclues), réel = `percent_of(durée, % achevé)` (S26) ;
//! Σ durée nulle : moyenne arrondie des % des feuilles ; aucune feuille : 0.
//! Source : Q4, S26 ; cas limites et arrondi : hypothèse.

use plannercraft_core::{LinkType, Percent};
use plannercraft_tests::net::{Net, lag_w};

fn pct(v: i64) -> Percent {
    Percent::new(v).unwrap()
}

#[test]
fn r055_weighted_by_leaf_durations() {
    // S {A 960 à 50 %, B 480 à 0 %, C 240 à 100 %} :
    // (480 + 0 + 240) / 1 680 = 0,428 57 → 4 286 (centièmes de %).
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .task("A", 960)
        .level("A", 2)
        .with_task("A", |t| t.progress.percent_complete = pct(5_000))
        .task("B", 480)
        .level("B", 2)
        .task("C", 240)
        .level("C", 2)
        .with_task("C", |t| t.progress.percent_complete = Percent::FULL)
        .run();
    assert_eq!(run.at("S").percent_complete, pct(4_286));
    // Une feuille garde son % saisi.
    assert_eq!(run.at("A").percent_complete, pct(5_000));
}

#[test]
fn r055_intermediate_summaries_not_weighted() {
    // S1 {S2 {A 960 à 50 % ; B 480 FS+2 j de A}, C 240 à 100 %} : toujours
    // 720 / 1 680 → 4 286. S2 : 480 / 1 440 → 3 333. Une pondération par la
    // durée de S2 (2 400) donnerait (2 400 × 33,33 % + 240) / 2 640 → 3 939.
    let run = Net::new("2026-10-19T08:00")
        .task("S1", 0)
        .task("S2", 0)
        .level("S2", 2)
        .task("A", 960)
        .level("A", 3)
        .with_task("A", |t| t.progress.percent_complete = pct(5_000))
        .task("B", 480)
        .level("B", 3)
        .link("A", "B", LinkType::FinishToStart, lag_w(960))
        .task("C", 240)
        .level("C", 2)
        .with_task("C", |t| t.progress.percent_complete = Percent::FULL)
        .run();
    assert_eq!(run.at("S2").duration.get(), 2_400);
    assert_eq!(run.at("S2").percent_complete, pct(3_333));
    assert_eq!(run.at("S1").percent_complete, pct(4_286));
}

#[test]
fn r055_zero_durations_average() {
    // S {M1 jalon à 100 %, M2 jalon à 0 %} : Σ durée = 0 → moyenne 5 000.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .task("M1", 0)
        .level("M1", 2)
        .with_task("M1", |t| t.progress.percent_complete = Percent::FULL)
        .task("M2", 0)
        .level("M2", 2)
        .run();
    assert_eq!(run.at("S").percent_complete, pct(5_000));
}

#[test]
fn r055_uses_effective_actual() {
    // Amendement J6 (R-210) : le réel d'une feuille est son réel effectif
    // (R-200). A 960 à 50 % stocké mais durée réelle saisie 720 ⇒ 720 (le
    // réel gagne) ; B 480 achevée (fin réelle) sans durée réelle ni % ⇒ 480 ;
    // S = (720 + 480) / 1 440 ⇒ 8 333.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .task("A", 960)
        .level("A", 2)
        .with_task("A", |t| {
            t.progress.percent_complete = pct(5_000);
            t.progress.actual_duration = plannercraft_core::Minutes::new(720);
        })
        .task("B", 480)
        .level("B", 2)
        .with_task("B", |t| {
            t.progress.actual_start = Some(plannercraft_tests::moment("2026-10-19T08:00"));
            t.progress.actual_finish = Some(plannercraft_tests::moment("2026-10-19T17:00"));
        })
        .run();
    assert_eq!(run.at("S").percent_complete, pct(8_333));
    // Une feuille garde son % saisi.
    assert_eq!(run.at("A").percent_complete, pct(5_000));
    assert_eq!(run.at("B").percent_complete, Percent::ZERO);
    // Une durée réelle saisie au-delà de la durée (achevée) borne le % à 100.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .with_task("A", |t| {
            t.progress.actual_start = Some(plannercraft_tests::moment("2026-10-19T08:00"));
            t.progress.actual_finish = Some(plannercraft_tests::moment("2026-10-19T17:00"));
            t.progress.actual_duration = plannercraft_core::Minutes::new(960);
        })
        .run();
    assert_eq!(run.at("S").percent_complete, Percent::FULL);
}
