//! R-188 — Utilisation par période : périodes civiles jour, semaine (début
//! `week_start_day`), mois, de `from` à `to` ; par ressource, travail de la
//! période (unités effectives × minutes ouvrées de l'affectation dans la
//! période) et drapeau « surutilisée » si un intervalle de surutilisation
//! la coupe ; au plus `MAX_PERIODS` périodes et `MAX_RESOURCES` ressources,
//! sinon `usage.limit`. Hypothèse.

use plannercraft_core::resource::usage::{MAX_PERIODS, MAX_RESOURCES};
use plannercraft_core::resource::{Granularity, UsageError, usage};
use plannercraft_core::{ResourceUid, Weekday};
use plannercraft_tests::moment;
use plannercraft_tests::net::{Net, Run};

use crate::r171_assignment_effective_calendar::{assign, resource};
use crate::r184_work_rollups::work;

/// R 100 % sur A lun-mer (24 h) et B mar (8 h) ; R2 sans affectation.
fn example() -> (Net, Run) {
    let build = || {
        let net = Net::new("2026-10-19T08:00")
            .task("P", 480)
            .task("A", 1_440)
            .task("B", 480)
            .fs("P", "B");
        let net = resource(resource(net, 1, None), 2, None);
        assign(assign(net, 1, "A", 1, 1_440), 2, "B", 1, 480)
    };
    (build(), build().run())
}

#[test]
fn r188_week_buckets() {
    let (net, run) = example();
    let p = net.build();
    let r = [ResourceUid::new(1), ResourceUid::new(2)];
    let week = usage(
        &p,
        &run.schedule,
        moment("2026-10-19T00:00"),
        moment("2026-11-02T00:00"),
        Granularity::Week,
        &r,
    )
    .unwrap();
    assert_eq!(
        week.periods,
        vec![moment("2026-10-19T00:00"), moment("2026-10-26T00:00")]
    );
    assert_eq!(week.rows[0].work, vec![work(1_920), work(0)]);
    assert_eq!(week.rows[0].overallocated, vec![true, false]);
    assert_eq!(week.rows[1].work, vec![work(0), work(0)]);
    assert!(!week.truncated);
    // Par jour : 8 h, 16 h (surutilisé), 8 h.
    let day = usage(
        &p,
        &run.schedule,
        moment("2026-10-19T08:00"),
        moment("2026-10-22T00:00"),
        Granularity::Day,
        &r[..1],
    )
    .unwrap();
    assert_eq!(day.periods[0], moment("2026-10-19T00:00"));
    assert_eq!(day.rows[0].work, vec![work(480), work(960), work(480)]);
    assert_eq!(day.rows[0].overallocated, vec![false, true, false]);
    // Semaine commençant le dimanche, mois : tout dans une période.
    let mut sunday = p.clone();
    sunday.options.week_start_day = Weekday::Sunday;
    let s = usage(
        &sunday,
        &run.schedule,
        moment("2026-10-19T08:00"),
        moment("2026-10-24T00:00"),
        Granularity::Week,
        &r[..1],
    )
    .unwrap();
    assert_eq!(s.periods, vec![moment("2026-10-18T00:00")]);
    assert_eq!(s.rows[0].work, vec![work(1_920)]);
    let m = usage(
        &p,
        &run.schedule,
        moment("2026-10-19T08:00"),
        moment("2026-11-30T00:00"),
        Granularity::Month,
        &r[..1],
    )
    .unwrap();
    assert_eq!(
        m.periods,
        vec![moment("2026-10-01T00:00"), moment("2026-11-01T00:00")]
    );
    assert_eq!(m.rows[0].work, vec![work(1_920), work(0)]);
}

#[test]
fn r188_limit() {
    let (net, run) = example();
    let p = net.build();
    let limit = UsageError::Limit {
        periods: MAX_PERIODS,
        resources: MAX_RESOURCES,
    };
    let too_long = usage(
        &p,
        &run.schedule,
        moment("2026-01-01T00:00"),
        moment("2029-12-31T00:00"),
        Granularity::Day,
        &[ResourceUid::new(1)],
    );
    assert_eq!(too_long, Err(limit));
    assert_eq!(limit.code(), "usage.limit");
    let many: Vec<_> = (0..=u32::try_from(MAX_RESOURCES).unwrap())
        .map(ResourceUid::new)
        .collect();
    let too_wide = usage(
        &p,
        &run.schedule,
        moment("2026-10-19T00:00"),
        moment("2026-10-26T00:00"),
        Granularity::Week,
        &many,
    );
    assert_eq!(too_wide, Err(limit));
    // Intervalle vide : aucune période.
    let empty = usage(
        &p,
        &run.schedule,
        moment("2026-10-26T00:00"),
        moment("2026-10-19T00:00"),
        Granularity::Week,
        &[ResourceUid::new(1)],
    )
    .unwrap();
    assert!(empty.periods.is_empty() && empty.rows[0].work.is_empty());
}
