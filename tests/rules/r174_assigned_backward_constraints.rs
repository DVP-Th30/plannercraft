//! R-174 — Passe arrière, contraintes et échéances d'une feuille affectée :
//! LS = min sur ses affectations du début qui finit chacune à la fin au plus
//! tard, moins son délai, sur son échelle ; contrainte de fin (FNET, FNLT,
//! MFO, échéance) : début = ce même minimum sous la date ; contrainte de
//! début (SNET, SNLT, MSO) : sur le début au plus tôt ; ALAP épinglée comme
//! R-092. Étend R-090 à R-111. Hypothèse.

use plannercraft_core::resource::{Granularity, usage};
use plannercraft_core::{Constraint, Minutes, ResourceUid};
use plannercraft_tests::moment;
use plannercraft_tests::net::{Net, Run};

use crate::r171_assignment_effective_calendar::{assign, assign_full, placed, resource};
use crate::r184_work_rollups::work;
use crate::r186_overallocation::over;

/// Feuille « A » de R-173 : R1 1 j, R2 1 j décalée de 2 j ; `extra`
/// complète le réseau.
fn network(extra: impl FnOnce(Net) -> Net) -> Run {
    let net = extra(Net::new("2026-10-19T08:00").task("A", 480));
    let net = resource(resource(net, 1, None), 2, None);
    let net = assign(net, 1, "A", 1, 480);
    assign_full(net, 2, "A", Some(2), (480, 10_000, 960)).run()
}

#[test]
fn r174_backward_min() {
    // LF = mer 17:00 (fin du projet) : R1 → mer 08:00, R2 → mer 08:00 −
    // 2 j = lun 08:00 ⇒ LS lun 08:00, TF 0, critique.
    let run = network(|n| n);
    let a = run.at("A");
    assert_eq!(a.late.start, moment("2026-10-19T08:00"));
    assert_eq!(a.late.finish, moment("2026-10-21T17:00"));
    assert_eq!(a.total_slack, Minutes::ZERO);
    assert!(a.critical);
    assert_eq!(placed(&run, 2).late.unwrap().start, moment("2026-10-21T08:00"));
}

#[test]
fn r174_slack_with_a_longer_task() {
    // Avec Z 5 j en parallèle (fin ven 17:00) : LS = min(ven 08:00, ven
    // 08:00 − 2 j = mer 08:00) = mer 08:00 ⇒ TF = 960.
    let run = network(|n| n.task("Z", 2_400));
    assert_eq!(run.at("A").late.start, moment("2026-10-21T08:00"));
    assert_eq!(run.at("A").total_slack, Minutes::new(960));
}

#[test]
fn r174_fnet_on_envelope() {
    // FNET ven 23/10 17:00 ⇒ début = min(ven 08:00, mer 08:00) = mer 08:00 ;
    // R2 finit ven 17:00, la date de la contrainte.
    let run = network(|n| {
        n.constraint(
            "A",
            Constraint::FinishNoEarlierThan {
                date: moment("2026-10-23T17:00"),
            },
        )
    });
    let a = run.at("A");
    assert_eq!(a.early.start, moment("2026-10-21T08:00"));
    assert_eq!(a.early.finish, moment("2026-10-23T17:00"));
    assert_eq!(placed(&run, 1).span.start, moment("2026-10-21T08:00"));
    assert_eq!(placed(&run, 2).span.finish, moment("2026-10-23T17:00"));
}

#[test]
fn r174_deadline_caps_late_finish() {
    // Échéance mar 20/10 17:00 avec Z 5 j : LF de A = mar 17:00 ⇒ R1 mar,
    // R2 mar − 2 j = ven 16/10 08:00 ⇒ LS ven 16/10, marge négative.
    let run = network(|n| {
        n.task("Z", 2_400)
            .with_task("A", |t| t.deadline = Some(moment("2026-10-20T17:00")))
    });
    assert_eq!(run.at("A").late.start, moment("2026-10-16T08:00"));
    assert_eq!(run.at("A").total_slack, Minutes::new(-480));
}

#[test]
fn r174_alap_assigned() {
    // ALAP avec Z 5 j : épinglée sur ses dates au plus tard, LS mer 08:00 ⇒
    // R1 mer, R2 ven, tâche mer 08:00 – ven 17:00.
    let run = network(|n| n.task("Z", 2_400).constraint("A", Constraint::AsLateAsPossible));
    let a = run.at("A");
    assert_eq!(a.start, moment("2026-10-21T08:00"));
    assert_eq!(a.finish, moment("2026-10-23T17:00"));
    assert_eq!(a.early, a.late);
    assert_eq!(placed(&run, 2).span.start, moment("2026-10-23T08:00"));
    assert!(run.schedule.diagnostics.is_empty(), "{:?}", run.codes());
}

#[test]
fn r174_from_finish_assignments_on_planned_dates() {
    // À rebours depuis ven 23/10/2026 17:00 (S) : C 5 j avec R1 40 h, A 1 j
    // avec R1 8 h, toutes deux ALAP (R-108). A a 4 j de marge : planifiée
    // sur ses dates au plus tard, ven 08:00 – 17:00 (R-112), au plus tôt
    // lun 19/10. Son affectation est rangée sur les dates planifiées, et R1
    // est surutilisée le vendredi (C + A = 200 %), pas le lundi.
    let build = || {
        let net = Net::new("2026-10-19T08:00")
            .from_finish("2026-10-23T17:00")
            .task("C", 2_400)
            .task("A", 480);
        assign(assign(resource(net, 1, None), 1, "C", 1, 2_400), 2, "A", 1, 480)
    };
    let run = build().run();
    let a = run.at("A");
    assert_eq!(a.start, moment("2026-10-23T08:00"));
    assert_eq!(a.early.start, moment("2026-10-19T08:00"));
    let pa = placed(&run, 2);
    assert_eq!(pa.span.start, a.start);
    assert_eq!(pa.span.finish, a.finish);
    assert_eq!(pa.late.unwrap().start, a.late.start);
    let r = &run.schedule.resources[&ResourceUid::new(1)];
    assert_eq!(
        r.overloads,
        vec![
            over("2026-10-23T08:00", "2026-10-23T12:00", 20_000),
            over("2026-10-23T13:00", "2026-10-23T17:00", 20_000),
        ]
    );
    // Utilisation par jour (R-188) : 8 h du lundi au jeudi, 16 h le
    // vendredi, seul jour surutilisé.
    let p = build().build();
    let days = usage(
        &p,
        &run.schedule,
        moment("2026-10-19T00:00"),
        moment("2026-10-24T00:00"),
        Granularity::Day,
        &[ResourceUid::new(1)],
    )
    .unwrap();
    assert_eq!(
        days.rows[0].work,
        vec![work(480), work(480), work(480), work(480), work(960)]
    );
    assert_eq!(days.rows[0].overallocated, vec![false, false, false, false, true]);
}
