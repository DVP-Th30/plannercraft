//! R-246 — Trimestre : périodes civiles alignées sur les 1er janvier,
//! avril, juillet et octobre à 00:00 ; exercice fiscal ignoré ; plafond
//! `MAX_PERIODS` inchangé. Source S62 (graduations) ; hypothèse
//! (exercice fiscal).

use plannercraft_core::calendar::{CalendarSet, Span};
use plannercraft_core::resource::usage::{MAX_PERIODS, MAX_RESOURCES};
use plannercraft_core::resource::{Granularity, UsageError, usage, usage_by_assignment};
use plannercraft_core::{ResourceUid, Weekday};
use plannercraft_tests::moment;
use plannercraft_tests::net::{Net, Run};

use crate::r171_assignment_effective_calendar::{assign, resource};
use crate::r184_work_rollups::work;

/// T de 4 j du lun 30/03/2026 au jeu 02/04/2026 sur S, R 100 % (32 h).
fn example() -> (Net, Run) {
    let build = || {
        assign(
            resource(Net::new("2026-03-30T08:00").task("T", 1_920), 1, None),
            1,
            "T",
            1,
            1_920,
        )
    };
    (build(), build().run())
}

#[test]
fn r246_quarter_boundaries() {
    let (net, run) = example();
    let p = net.build();
    let r = [ResourceUid::new(1)];
    let q = |from: &str, to: &str| {
        usage(
            &p,
            &run.schedule,
            moment(from),
            moment(to),
            Granularity::Quarter,
            &r,
        )
        .unwrap()
        .periods
    };
    // Du 15/02 au 20/05/2026 : T1 et T2 civils.
    assert_eq!(
        q("2026-02-15T00:00", "2026-05-20T00:00"),
        vec![moment("2026-01-01T00:00"), moment("2026-04-01T00:00")]
    );
    // Un trimestre par bord : 1er juillet exclu de [.., 01/07[, puis passage d'année.
    assert_eq!(
        q("2026-04-01T00:00", "2026-07-01T00:00"),
        vec![moment("2026-04-01T00:00")]
    );
    assert_eq!(
        q("2026-08-31T12:00", "2027-01-01T00:01"),
        vec![
            moment("2026-07-01T00:00"),
            moment("2026-10-01T00:00"),
            moment("2027-01-01T00:00")
        ]
    );
    assert_eq!(
        q("2026-12-31T23:59", "2027-01-01T00:00"),
        vec![moment("2026-10-01T00:00")]
    );
    // Semaine du projet sans effet sur le trimestre ; intervalle vide.
    let mut sunday = p.clone();
    sunday.options.week_start_day = Weekday::Sunday;
    let s = usage(
        &sunday,
        &run.schedule,
        moment("2026-11-15T00:00"),
        moment("2026-11-16T00:00"),
        Granularity::Quarter,
        &r,
    )
    .unwrap();
    assert_eq!(s.periods, vec![moment("2026-10-01T00:00")]);
    assert_eq!(q("2026-05-01T00:00", "2026-05-01T00:00"), vec![]);
    // Plafond inchangé : tout le domaine (1984-2149) tient en 664 trimestres,
    // jamais au-delà de MAX_PERIODS ; le même domaine en mois est refusé.
    let (set, _) = CalendarSet::compile(&p);
    let (start, end) = (moment("1984-01-01T00:00"), moment("2149-12-31T23:59"));
    let all = usage_by_assignment(&p, &run.schedule, &set, start, end, Granularity::Quarter, &r).unwrap();
    assert_eq!(all.periods.len(), 664);
    assert!(all.periods.len() <= MAX_PERIODS);
    assert_eq!(all.periods[663].start, moment("2149-10-01T00:00"));
    assert_eq!(
        all.rows[0].work.iter().map(|w| w.minutes().get()).sum::<i64>(),
        1_920
    );
    let limit = UsageError::Limit {
        periods: MAX_PERIODS,
        resources: MAX_RESOURCES,
    };
    assert_eq!(
        usage(&p, &run.schedule, start, end, Granularity::Month, &r),
        Err(limit)
    );
}

#[test]
fn r246_quarter_split_work() {
    let (net, run) = example();
    let p = net.build();
    let (set, _) = CalendarSet::compile(&p);
    let r = [ResourceUid::new(1)];
    let (from, to) = (moment("2026-02-15T00:00"), moment("2026-05-20T00:00"));
    // 16 h au T1 (lun 30, mar 31/03), 16 h au T2 (mer 01, jeu 02/04).
    let u = usage(&p, &run.schedule, from, to, Granularity::Quarter, &r).unwrap();
    assert_eq!(u.rows[0].work, vec![work(960), work(960)]);
    let a = usage_by_assignment(&p, &run.schedule, &set, from, to, Granularity::Quarter, &r).unwrap();
    assert_eq!(
        a.periods,
        vec![
            Span {
                start: from,
                finish: moment("2026-04-01T00:00")
            },
            Span {
                start: moment("2026-04-01T00:00"),
                finish: to
            },
        ]
    );
    assert_eq!(a.rows[0].work, vec![work(960), work(960)]);
    assert_eq!(a.task_totals, vec![(run.uid("T"), vec![work(960), work(960)])]);
}
