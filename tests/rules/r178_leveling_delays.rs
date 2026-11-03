//! R-178 — Retards de nivellement enregistrés. Tâche : `ES` = plus grande
//! borne de début (début du projet, porte, FS, SS) ; `ES'` = `ES` + retard
//! (écoulé si `elapsed`, sinon ouvré sur l'échelle de placement), recalé sur
//! l'instant ouvré suivant ; une borne de fin (FF, SF) porte sur `ES'` ;
//! `early.start` = `ES` (non retardé), `start` = `ES'`, `early.finish` =
//! `finish` ; les successeurs partent de la fin retardée ; dates au plus
//! tard sans le retard, et les prédécesseurs bornés par le placement non
//! retardé. Affectation : retard ajouté au `Delay`, sur son échelle.
//! Source : S46 ; O10, O26, O27.

use plannercraft_core::{DurationUnit, LevelingDelay, LinkType, Minutes};
use plannercraft_tests::moment;
use plannercraft_tests::net::{Net, lag_w};

use crate::r171_assignment_effective_calendar::{assign, placed, resource};

/// Retard de nivellement de `minutes`, écoulé ou ouvré.
fn delay(minutes: i64, elapsed: bool) -> Option<LevelingDelay> {
    LevelingDelay::new(Minutes::new(minutes), DurationUnit::Day, elapsed)
}

#[test]
fn r178_task_delay_es_not_delayed() {
    // 1 j, ES jeu 22/10 08:00, retard 3 ej ⇒ dim 25/10 08:00 recalé lun
    // 26/10 08:00, fin lun 17:00 ; `early.start` jeu 22/10 08:00 (A6 UID 20,
    // O27) ; successeur FS mar 27/10.
    let run = Net::new("2026-10-22T08:00")
        .task("A", 480)
        .task("B", 480)
        .fs("A", "B")
        .with_task("A", |t| t.leveling_delay = delay(4_320, true))
        .run();
    let a = run.at("A");
    assert_eq!(a.start, moment("2026-10-26T08:00"));
    assert_eq!(a.finish, moment("2026-10-26T17:00"));
    assert_eq!(a.early.start, moment("2026-10-22T08:00"));
    assert_eq!(a.early.finish, a.finish);
    assert_eq!(a.duration, Minutes::new(480));
    assert_eq!(run.at("B").start, moment("2026-10-27T08:00"));
}

#[test]
fn r178_task_delay_working() {
    // Retard de 1 j ouvré depuis ven 23/10 08:00 (Standard) ⇒ lun 26/10.
    let run = Net::new("2026-10-23T08:00")
        .task("A", 480)
        .with_task("A", |t| t.leveling_delay = delay(480, false))
        .run();
    assert_eq!(run.at("A").start, moment("2026-10-26T08:00"));
    assert_eq!(run.at("A").early.start, moment("2026-10-23T08:00"));
}

#[test]
fn r178_finish_bound_on_delayed_start() {
    // P 3 j (lun – mer) FF → A 1 j, retard 1 ej : ES' = mar 08:00 donnerait
    // une fin avant celle de P ; la borne FF porte sur ES' ⇒ A mer 08:00 –
    // 17:00 ; `early.start` reste la borne de début seule (lun 08:00, comme
    // A6 UID 20).
    let run = Net::new("2026-10-19T08:00")
        .task("P", 1_440)
        .task("A", 480)
        .link("P", "A", LinkType::FinishToFinish, lag_w(0))
        .with_task("A", |t| t.leveling_delay = delay(1_440, true))
        .run();
    let a = run.at("A");
    assert_eq!(a.start, moment("2026-10-21T08:00"));
    assert_eq!(a.finish, moment("2026-10-21T17:00"));
    assert_eq!(a.early.start, moment("2026-10-19T08:00"));
}

#[test]
fn r178_late_dates_without_delay_and_predecessor_held() {
    // P 1 j FF → A 1 j retardé de 2 ej (A mer) ; L 5 j fixe la fin du
    // projet ven 23/10 17:00. A : dates au plus tard sans le retard (ven) ;
    // P : borné par le placement non retardé de A (lun 17:00), marge 0
    // (A6 UID 21 → 36).
    let run = Net::new("2026-10-19T08:00")
        .task("P", 480)
        .task("A", 480)
        .task("L", 2_400)
        .link("P", "A", LinkType::FinishToFinish, lag_w(0))
        .with_task("A", |t| t.leveling_delay = delay(2_880, true))
        .run();
    let a = run.at("A");
    assert_eq!(a.start, moment("2026-10-21T08:00"));
    assert_eq!(a.late.start, moment("2026-10-23T08:00"));
    assert_eq!(a.late.finish, moment("2026-10-23T17:00"));
    assert_eq!(a.total_slack, Minutes::new(960));
    let p = run.at("P");
    assert_eq!(p.late.finish, moment("2026-10-19T17:00"));
    assert_eq!(p.total_slack, Minutes::ZERO);
}

#[test]
fn r178_assignment_delay_working() {
    // Tâche 2 j, R1 8 h et R2 8 h, R1 retardée de 1 j ouvré ⇒ R1 mar, R2
    // lun ; tâche lun 08:00 – mar 17:00, durée 960 (A6 UID 7, O26).
    let net = Net::new("2026-10-19T08:00").task("A", 960);
    let net = resource(resource(net, 1, None), 2, None);
    let net = assign(assign(net, 1, "A", 1, 480), 2, "A", 2, 480).project(|p| {
        let a = p
            .assignments
            .get_mut(plannercraft_core::AssignmentUid::new(1))
            .unwrap();
        a.leveling_delay = delay(480, false);
    });
    let run = net.run();
    assert_eq!(placed(&run, 1).span.start, moment("2026-10-20T08:00"));
    assert_eq!(placed(&run, 2).span.start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("A").start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("A").finish, moment("2026-10-20T17:00"));
    assert_eq!(run.at("A").duration, Minutes::new(960));
}

#[test]
fn r178_assignment_delay_elapsed_adds_to_delay() {
    // Une affectation de 1 j, `Delay` 1 j ouvré puis retard 2 ej : lun
    // 08:00 + 1 j = lun 17:00, + 48 h = mer 17:00, recalé jeu 08:00.
    let net = Net::new("2026-10-19T08:00").task("A", 480);
    let net = crate::r171_assignment_effective_calendar::assign_full(
        resource(net, 1, None),
        1,
        "A",
        Some(1),
        (480, 10_000, 480),
    )
    .project(|p| {
        let a = p
            .assignments
            .get_mut(plannercraft_core::AssignmentUid::new(1))
            .unwrap();
        a.leveling_delay = delay(2_880, true);
    });
    let run = net.run();
    assert_eq!(placed(&run, 1).span.start, moment("2026-10-22T08:00"));
    assert_eq!(run.at("A").start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("A").finish, moment("2026-10-22T17:00"));
}
