//! R-245 — Utilisation par affectation : une ligne par affectation que la
//! ligne de sa ressource compte (R-188), travail par période calculé de la
//! même façon ; Σ des lignes d'une ressource = sa ligne d'`usage` ; total
//! d'une tâche = Σ de ses lignes de travail ; au-delà de
//! `MAX_ASSIGNMENT_ROWS` lignes, `usage.limit`. Hypothèse (S60 pour la
//! surutilisation affichée).

use plannercraft_core::calendar::{CalendarSet, Span};
use plannercraft_core::resource::usage::{MAX_ASSIGNMENT_ROWS, MAX_PERIODS, MAX_RESOURCES};
use plannercraft_core::resource::{Granularity, UsageError, usage, usage_by_assignment};
use plannercraft_core::{AssignmentUid, ResourceKind, ResourceUid};
use plannercraft_tests::moment;
use plannercraft_tests::net::{Net, Run};

use crate::r171_assignment_effective_calendar::{assign, assign_full, resource, resource_kind};
use crate::r184_work_rollups::work;

/// Maçon (UID 1) 100 % sur T1 (lun 19-mar 20/10, 16 h) et 50 % sur T2
/// (lun 19-ven 23/10, 20 h) ; Électricien (UID 2) 100 % sur T2 (8 h, lun) ;
/// Ciment (UID 3, matérielle) 5 sur T1.
fn example() -> (Net, Run) {
    let build = || {
        let net = Net::new("2026-10-19T08:00").task("T1", 960).task("T2", 2_400);
        let net = resource(resource(net, 1, None), 2, None);
        let net = resource_kind(
            net,
            3,
            None,
            ResourceKind::Material {
                label: "sac".to_owned(),
            },
        );
        let net = assign(net, 1, "T1", 1, 960);
        let net = assign_full(net, 2, "T2", Some(1), (1_200, 5_000, 0));
        let net = assign(net, 3, "T2", 2, 480);
        assign_full(net, 4, "T1", Some(3), (300, 10_000, 0))
    };
    (build(), build().run())
}

#[test]
fn r245_assignment_rows_sum_to_resource() {
    let (net, run) = example();
    let p = net.build();
    let (set, _) = CalendarSet::compile(&p);
    let all = [ResourceUid::new(1), ResourceUid::new(2), ResourceUid::new(3)];
    let (from, to) = (moment("2026-10-19T00:00"), moment("2026-10-24T00:00"));
    let r = usage_by_assignment(&p, &run.schedule, &set, from, to, Granularity::Day, &all).unwrap();
    // Périodes bornées à [from, to) : lun à ven.
    assert_eq!(r.periods.len(), 5);
    assert_eq!(
        r.periods[0],
        Span {
            start: from,
            finish: moment("2026-10-20T00:00")
        }
    );
    assert_eq!(r.periods[4].finish, to);
    // Lignes triées (ressource, tâche, UID) : Maçon T1, Maçon T2, Électricien T2, Ciment T1.
    let keys: Vec<_> = r
        .rows
        .iter()
        .map(|x| (x.resource.get(), x.task, x.assignment.get()))
        .collect();
    let (t1, t2) = (run.uid("T1"), run.uid("T2"));
    assert_eq!(keys, vec![(1, t1, 1), (1, t2, 2), (2, t2, 3), (3, t1, 4)]);
    // Lundi : T1 8 h, T2 4 h ⇒ Maçon 12 h.
    assert_eq!(
        r.rows[0].work,
        vec![work(480), work(480), work(0), work(0), work(0)]
    );
    assert_eq!(r.rows[1].work, vec![work(240); 5]);
    let line = usage(&p, &run.schedule, from, to, Granularity::Day, &all).unwrap();
    assert_eq!(line.rows[0].work[0], work(720));
    assert!(line.rows[0].overallocated[0]);
    // Σ des lignes de chaque ressource = sa ligne, période par période.
    for row in &line.rows {
        let mut sum = vec![0_i64; 5];
        for a in r.rows.iter().filter(|a| a.resource == row.resource) {
            for (s, w) in sum.iter_mut().zip(&a.work) {
                *s += w.minutes().get();
            }
        }
        let expected: Vec<i64> = row.work.iter().map(|w| w.minutes().get()).collect();
        assert_eq!(sum, expected, "ressource {:?}", row.resource);
    }
    // Semaine du 19/10 : 16 h + 20 h = 36 h pour le Maçon.
    let week = usage_by_assignment(
        &p,
        &run.schedule,
        &set,
        from,
        to,
        Granularity::Week,
        &[ResourceUid::new(1)],
    )
    .unwrap();
    assert_eq!(week.rows.len(), 2);
    assert_eq!(week.rows[0].work, vec![work(960)]);
    assert_eq!(week.rows[1].work, vec![work(1_200)]);
}

#[test]
fn r245_task_totals() {
    let (net, run) = example();
    let p = net.build();
    let (set, _) = CalendarSet::compile(&p);
    let all = [ResourceUid::new(1), ResourceUid::new(2), ResourceUid::new(3)];
    let (from, to) = (moment("2026-10-19T00:00"), moment("2026-10-24T00:00"));
    let r = usage_by_assignment(&p, &run.schedule, &set, from, to, Granularity::Day, &all).unwrap();
    let (t1, t2) = (run.uid("T1"), run.uid("T2"));
    // T1 : Maçon seul (la quantité de Ciment ne s'ajoute pas aux heures).
    // T2 : Maçon 4 h/j + Électricien 8 h le lundi.
    assert_eq!(
        r.task_totals,
        vec![
            (t1, vec![work(480), work(480), work(0), work(0), work(0)]),
            (t2, vec![work(720), work(240), work(240), work(240), work(240)]),
        ]
    );
    // Ressources demandées seulement : sans le Maçon, T1 n'a plus de total.
    let r = usage_by_assignment(
        &p,
        &run.schedule,
        &set,
        from,
        to,
        Granularity::Day,
        &[ResourceUid::new(2), ResourceUid::new(2), ResourceUid::new(99)],
    )
    .unwrap();
    assert_eq!(r.rows.len(), 1, "doublon et ressource inconnue ignorés");
    assert_eq!(
        r.task_totals,
        vec![(t2, vec![work(480), work(0), work(0), work(0), work(0)])]
    );
    // Intervalle vide : aucune période, lignes vides.
    let empty = usage_by_assignment(&p, &run.schedule, &set, to, from, Granularity::Day, &all).unwrap();
    assert_eq!(empty.periods, vec![]);
    assert!(empty.rows.iter().all(|a| a.work.is_empty()));
}

#[test]
fn r245_limit_refused() {
    let limit = UsageError::Limit {
        periods: MAX_PERIODS,
        resources: MAX_RESOURCES,
    };
    assert_eq!(limit.code(), "usage.limit");
    // MAX_ASSIGNMENT_ROWS + 1 affectations d'une même ressource (copies de
    // l'affectation planifiée, plus rapide qu'un recalcul) : refus.
    let n = u32::try_from(MAX_ASSIGNMENT_ROWS).unwrap() + 1;
    let build = || {
        assign(
            resource(Net::new("2026-10-19T08:00").task("T", 480), 1, None),
            1,
            "T",
            1,
            480,
        )
    };
    let p = build().build();
    let mut s = build().run().schedule;
    let proto = s.assignments[&AssignmentUid::new(1)];
    for uid in 2..=n {
        s.assignments.insert(AssignmentUid::new(uid), proto);
    }
    let (set, _) = CalendarSet::compile(&p);
    let (from, to) = (moment("2026-10-19T00:00"), moment("2026-10-20T00:00"));
    let one = [ResourceUid::new(1)];
    assert_eq!(
        usage_by_assignment(&p, &s, &set, from, to, Granularity::Day, &one),
        Err(limit)
    );
    // Exactement MAX_ASSIGNMENT_ROWS : servi.
    s.assignments.remove(&AssignmentUid::new(n));
    let ok = usage_by_assignment(&p, &s, &set, from, to, Granularity::Day, &one).unwrap();
    assert_eq!(ok.rows.len(), MAX_ASSIGNMENT_ROWS);
    // Bornes communes avec `usage` : périodes et ressources.
    let many = vec![ResourceUid::new(1); MAX_RESOURCES + 1];
    assert_eq!(
        usage_by_assignment(&p, &s, &set, from, to, Granularity::Day, &many),
        Err(limit)
    );
    let long = usage_by_assignment(
        &p,
        &s,
        &set,
        moment("2026-01-01T00:00"),
        moment("2029-12-31T00:00"),
        Granularity::Day,
        &one,
    );
    assert_eq!(long, Err(limit));
}
