//! R-186 — Surutilisation : charge de r à l'instant t = Σ des unités
//! effectives des affectations de r dont l'échelle effective est ouvrée à
//! t et dont le span contient t ; unités effectives = W ÷ temps ouvré du
//! span (= U hors durée fixe). Surutilisée ⇔ charge > `max_units` sur un
//! intervalle non vide ; intervalles maximaux fusionnés avec leur pic ;
//! diagnostic `resource.overallocated`. Source : S47 ; O31 ; débit
//! effectif : hypothèse.

use plannercraft_core::{Constraint, Diagnostic, Overload, ResourceUid, TaskType, Units};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

use crate::r171_assignment_effective_calendar::{assign, assign_full, placed, resource};

/// Intervalle `[from, to)` au pic `peak` (1/10 000).
pub(crate) fn over(from: &str, to: &str, peak: i64) -> Overload {
    Overload {
        from: moment(from),
        to: moment(to),
        peak: Units::new(peak).unwrap(),
    }
}

#[test]
fn r186_parallel_tasks_overallocated() {
    // R 100 % sur A lun-mer et sur B mar ⇒ mar [08:00, 12:00) et [13:00,
    // 17:00), pic 200 %.
    let net = Net::new("2026-10-19T08:00")
        .task("P", 480)
        .task("A", 1_440)
        .task("B", 480)
        .fs("P", "B");
    let net = assign(assign(resource(net, 1, None), 1, "A", 1, 1_440), 2, "B", 1, 480);
    let run = net.run();
    let r = &run.schedule.resources[&ResourceUid::new(1)];
    assert!(r.overallocated && !r.truncated);
    assert_eq!(
        r.overloads,
        vec![
            over("2026-10-20T08:00", "2026-10-20T12:00", 20_000),
            over("2026-10-20T13:00", "2026-10-20T17:00", 20_000),
        ]
    );
    assert_eq!(r.peak, Units::new(20_000).unwrap());
    assert!(
        run.schedule
            .diagnostics
            .contains(&Diagnostic::ResourceOverallocated {
                resource: ResourceUid::new(1),
                from: moment("2026-10-20T08:00"),
                to: moment("2026-10-20T12:00"),
                peak: Units::new(20_000).unwrap(),
            })
    );
}

#[test]
fn r186_single_over_capacity() {
    // A seule à 150 % (24 h, d_a = 16 h) ⇒ toutes les plages ouvrées de son
    // span, pic 150 %.
    let net = resource(Net::new("2026-10-19T08:00").task("A", 960), 1, None);
    let run = assign_full(net, 1, "A", Some(1), (1_440, 15_000, 0)).run();
    let r = &run.schedule.resources[&ResourceUid::new(1)];
    assert_eq!(
        r.overloads,
        vec![
            over("2026-10-19T08:00", "2026-10-19T12:00", 15_000),
            over("2026-10-19T13:00", "2026-10-19T17:00", 15_000),
            over("2026-10-20T08:00", "2026-10-20T12:00", 15_000),
            over("2026-10-20T13:00", "2026-10-20T17:00", 15_000),
        ]
    );
    assert_eq!(r.peak, Units::new(15_000).unwrap());
    // Capacité 200 % : rien.
    let net = resource(Net::new("2026-10-19T08:00").task("A", 960), 1, None).project(|p| {
        p.resources.get_mut(ResourceUid::new(1)).unwrap().max_units = Units::new(20_000).unwrap();
    });
    let run = assign_full(net, 1, "A", Some(1), (1_440, 15_000, 0)).run();
    let r = &run.schedule.resources[&ResourceUid::new(1)];
    assert!(!r.overallocated && r.overloads.is_empty());
    assert_eq!(r.peak, Units::new(15_000).unwrap());
}

#[test]
fn r186_intraday_overlap() {
    // Deux heures de chevauchement dans la journée (comme A7 R3, ven
    // 18:00-20:00) : A lun 08:00-17:00, B 2 h à partir de lun 15:00 ⇒
    // [15:00, 17:00), pic 200 % ; un seau par jour (16 h ≤ 8 h × 2) ne le
    // verrait pas.
    let net = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 120)
        .constraint(
            "B",
            Constraint::StartNoEarlierThan {
                date: moment("2026-10-19T15:00"),
            },
        );
    let run = assign(assign(resource(net, 1, None), 1, "A", 1, 480), 2, "B", 1, 120).run();
    let r = &run.schedule.resources[&ResourceUid::new(1)];
    assert_eq!(
        r.overloads,
        vec![over("2026-10-19T15:00", "2026-10-19T17:00", 20_000)]
    );
}

#[test]
fn r186_fixed_duration_effective_units() {
    // A à durée fixe 2 j, R 100 % saisi pour 8 h ⇒ d_a = 2 j, débit 50 % ;
    // B 2 j à 50 % en parallèle ⇒ charge 100 %, pas de surutilisation (les
    // unités saisies donneraient 150 %).
    let net = Net::new("2026-10-19T08:00")
        .task("A", 960)
        .task("B", 960)
        .with_task("A", |t| t.task_type = TaskType::FixedDuration);
    let net = assign(resource(net, 1, None), 1, "A", 1, 480);
    let run = assign_full(net, 2, "B", Some(1), (480, 5_000, 0)).run();
    assert_eq!(placed(&run, 1).effective_units, Units::new(5_000).unwrap());
    assert_eq!(placed(&run, 2).effective_units, Units::new(5_000).unwrap());
    let r = &run.schedule.resources[&ResourceUid::new(1)];
    assert!(!r.overallocated);
    assert_eq!(r.peak, Units::FULL);
}
