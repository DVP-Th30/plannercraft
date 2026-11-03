//! R-209 — Affectations d'une tâche commencée. Achevée (lot L1) :
//! affectations de travail placées depuis le début réel sur leur échelle
//! (R-171, R-173), partie réelle `actual` = `span` ; affectation à travail
//! nul : span vide au début réel brut. En cours (lot L3) : partie réelle
//! depuis le début réel, `d_act = round_div(AW × 10 000, U)`, reste depuis
//! `Resume`, `d_rem = round_div((W − AW) × 10 000, U)` (U = 0 : durées de
//! la tâche, R-181), `actual` = partie réelle, `span` = [début réel, fin du
//! reste], `remaining_start` = début du reste ; fin de la tâche = plus
//! grande fin des restes. La feuille commencée (achevée ou en cours) sort de
//! `sync_assigned_durations` et de `placed_by_assignments` (R-173
//! amendée) : sa durée est celle de l'avancement. Source : O39 (A7
//! affectations 38 à 45) ; partie en cours : hypothèse.

use plannercraft_core::schedule::{placed_by_assignments, sync_assigned_durations};
use plannercraft_core::{Minutes, Percent, Span, Units, Work};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

use crate::r171_assignment_effective_calendar::{assign, assign_full, placed, resource};

/// B 1 j achevée, début réel lun 13:00, fin réelle mar 12:00 (D 480).
fn completed_b() -> Net {
    Net::new("2026-10-19T08:00").task("B", 480).with_task("B", |t| {
        t.progress.actual_start = Some(moment("2026-10-19T13:00"));
        t.progress.actual_finish = Some(moment("2026-10-20T12:00"));
    })
}

#[test]
fn r209_completed_assignments_from_actual_start() {
    // R1 100 % 8 h ⇒ lun 13:00 – mar 12:00, partie réelle = span ; dates au
    // plus tard = réelles.
    let run = assign(resource(completed_b(), 1, None), 1, "B", 1, 480).run();
    let a = placed(&run, 1);
    let expected = Span {
        start: moment("2026-10-19T13:00"),
        finish: moment("2026-10-20T12:00"),
    };
    assert_eq!(a.span, expected);
    assert_eq!(a.actual, Some(expected));
    assert_eq!(a.late, Some(expected));
    assert_eq!(a.remaining_start, None);
    // Avec un délai d'un jour, R2 part du début réel décalé (mar 13:00 –
    // mer 12:00), la tâche garde ses dates réelles.
    let net = resource(resource(completed_b(), 1, None), 2, None);
    let run = assign_full(assign(net, 1, "B", 1, 480), 2, "B", Some(2), (480, 10_000, 480)).run();
    assert_eq!(placed(&run, 2).span.start, moment("2026-10-20T13:00"));
    assert_eq!(placed(&run, 2).actual, Some(placed(&run, 2).span));
    assert_eq!(run.at("B").finish, moment("2026-10-20T12:00"));
}

#[test]
fn r209_zero_work_at_actual_start() {
    // Début réel lun 12:30 (pause, chômé) : R1 8 h placée depuis le début
    // réel (lun 13:00 – mar 12:00), R2 à travail nul datée au début réel
    // brut (lun 12:30 – 12:30, O39 : A7 affectations 39 à 41 à ven 08:00).
    let net = Net::new("2026-10-19T08:00").task("B", 480).with_task("B", |t| {
        t.progress.actual_start = Some(moment("2026-10-19T12:30"));
        t.progress.actual_finish = Some(moment("2026-10-20T12:00"));
    });
    let net = resource(resource(net, 1, None), 2, None);
    let run = assign(assign(net, 1, "B", 1, 480), 2, "B", 2, 0).run();
    assert_eq!(placed(&run, 1).span.start, moment("2026-10-19T13:00"));
    assert_eq!(placed(&run, 1).span.finish, moment("2026-10-20T12:00"));
    let zero = Span {
        start: moment("2026-10-19T12:30"),
        finish: moment("2026-10-19T12:30"),
    };
    assert_eq!(placed(&run, 2).span, zero);
    assert_eq!(placed(&run, 2).actual, Some(zero));
}

#[test]
fn r209_excluded_from_sync() {
    // B achevée avec R1 1 j et R2 1 j décalée de 2 j : l'union des
    // affectations (960) ne remplace pas la durée de l'avancement (480).
    let net = resource(resource(completed_b(), 1, None), 2, None);
    let mut project = assign_full(assign(net, 1, "B", 1, 480), 2, "B", Some(2), (480, 10_000, 960)).build();
    let s = plannercraft_core::schedule(&project);
    let b = s.scheduled(plannercraft_core::TaskUid::new(1)).unwrap();
    assert_eq!(b.duration, Minutes::new(480));
    assert_eq!(placed_by_assignments(&project, &s).len(), 0);
    assert_eq!(sync_assigned_durations(&mut project, &s), Vec::new());
    assert_eq!(
        project
            .tasks
            .get(plannercraft_core::TaskUid::new(1))
            .unwrap()
            .duration
            .minutes(),
        Minutes::new(480)
    );
    // Non achevée : la même feuille suit l'union (R-173).
    let net = resource(
        resource(Net::new("2026-10-19T08:00").task("B", 480), 1, None),
        2,
        None,
    );
    let project = assign_full(assign(net, 1, "B", 1, 480), 2, "B", Some(2), (480, 10_000, 960)).build();
    let s = plannercraft_core::schedule(&project);
    assert_eq!(placed_by_assignments(&project, &s).len(), 1);
}

/// T 3 j (1 440) commencée lun 08:00 à 3 333 (réel 480).
fn running_t() -> Net {
    Net::new("2026-10-19T08:00").task("T", 1_440).with_task("T", |t| {
        t.progress.actual_start = Some(moment("2026-10-19T08:00"));
        t.progress.percent_complete = Percent::new(3_333).unwrap();
    })
}

/// Travail réel `aw` (minutes) de l'affectation `uid`.
fn actual_work(net: Net, uid: u32, aw: i64) -> Net {
    net.project(|p| {
        let a = p
            .assignments
            .get_mut(plannercraft_core::AssignmentUid::new(uid))
            .unwrap();
        a.actual_work = Work::new(Minutes::new(aw)).unwrap();
    })
}

#[test]
fn r209_two_part_assignments() {
    // R1 100 % 1 440, travail réel 480 : partie réelle lun 08:00 – 17:00,
    // reste 960 mar – mer, span lun 08:00 – mer 17:00.
    let net = actual_work(assign(resource(running_t(), 1, None), 1, "T", 1, 1_440), 1, 480);
    let run = net.run();
    let a = placed(&run, 1);
    assert_eq!(
        a.actual,
        Some(Span {
            start: moment("2026-10-19T08:00"),
            finish: moment("2026-10-19T17:00"),
        })
    );
    assert_eq!(a.remaining_start, Some(moment("2026-10-20T08:00")));
    assert_eq!(
        a.span,
        Span {
            start: moment("2026-10-19T08:00"),
            finish: moment("2026-10-21T17:00"),
        }
    );
    assert_eq!(run.at("T").finish, moment("2026-10-21T17:00"));
    assert_eq!(run.at("T").duration, Minutes::new(1_440));
    // Hors séquence (P 2 j FS → T) : le reste part de la reprise mer 08:00.
    let net = actual_work(
        assign(
            resource(running_t().task("P", 960).fs("P", "T"), 1, None),
            1,
            "T",
            1,
            1_440,
        ),
        1,
        480,
    );
    let run = net.run();
    let a = placed(&run, 1);
    assert_eq!(a.remaining_start, Some(moment("2026-10-21T08:00")));
    assert_eq!(a.span.finish, moment("2026-10-22T17:00"));
    assert_eq!(a.actual.unwrap().finish, moment("2026-10-19T17:00"));
    // Deux affectations : R2 960 sans travail réel, partie réelle vide au
    // début réel, reste mar – mer ; la tâche finit à la plus grande fin des
    // restes (mer 17:00) ; travail nul : instant du début réel.
    let three = || {
        let net = resource(resource(resource(running_t(), 1, None), 2, None), 3, None);
        actual_work(
            assign(
                assign(assign(net, 1, "T", 1, 1_440), 2, "T", 2, 960),
                3,
                "T",
                3,
                0,
            ),
            1,
            480,
        )
    };
    let run = three().run();
    let r2 = placed(&run, 2);
    let point = Span {
        start: moment("2026-10-19T08:00"),
        finish: moment("2026-10-19T08:00"),
    };
    assert_eq!(r2.actual, Some(point));
    assert_eq!(r2.remaining_start, Some(moment("2026-10-20T08:00")));
    assert_eq!(r2.span.finish, moment("2026-10-21T17:00"));
    assert_eq!(placed(&run, 3).span, point);
    assert_eq!(placed(&run, 3).remaining_start, None);
    assert_eq!(run.at("T").finish, moment("2026-10-21T17:00"));
    // Exclue de la synchronisation des durées (R-173 amendée).
    let mut project = three().build();
    let s = plannercraft_core::schedule(&project);
    assert_eq!(placed_by_assignments(&project, &s).len(), 0);
    assert_eq!(sync_assigned_durations(&mut project, &s), Vec::new());
}

#[test]
fn r209_zero_units_in_progress() {
    // Unités nulles (R-181) : durées de la tâche, partie réelle 480, reste
    // 960 : mêmes dates que la tâche.
    let net = assign(resource(running_t(), 1, None), 1, "T", 1, 1_440).project(|p| {
        let a = p
            .assignments
            .get_mut(plannercraft_core::AssignmentUid::new(1))
            .unwrap();
        a.units = Units::ZERO;
    });
    let run = net.run();
    let a = placed(&run, 1);
    assert_eq!(a.actual.unwrap().finish, moment("2026-10-19T17:00"));
    assert_eq!(a.remaining_start, Some(moment("2026-10-20T08:00")));
    assert_eq!(a.span.finish, moment("2026-10-21T17:00"));
    assert_eq!(run.at("T").finish, moment("2026-10-21T17:00"));
}
