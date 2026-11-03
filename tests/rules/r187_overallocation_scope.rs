//! R-187 — Périmètre de la surutilisation : ressources de travail actives,
//! génériques comprises ; inactives, affectation fictive, UID 0 et
//! ressources inconnues exclues ; affectations des tâches inactives
//! exclues ; capacité = `max_units`, constante : `AvailabilityPeriods`
//! relues en `Extras` ignorées, `resource.availability_ignored` (info).
//! Oracles : A4 et A6 sans surutilisation (aucun faux positif). Source :
//! S47, S49 ; O31.

use plannercraft_core::{ExtraEntry, ResourceKind, ResourceUid, Severity, Units, schedule};
use plannercraft_mspdi::ReadOptions;
use plannercraft_tests::net::Net;
use plannercraft_tests::{moment, tests_path};

use crate::r171_assignment_effective_calendar::{assign, resource};
use crate::r184_work_rollups::work;

#[test]
fn r187_inactive_excluded() {
    // R1 inactive sur A et B en parallèle ; R2 sur A et sur C inactive en
    // parallèle ; R3 générique sur A et B : seule R3 est surutilisée.
    let net = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 480)
        .task("C", 480)
        .inactive("C");
    let net = resource(resource(resource(net, 1, None), 2, None), 3, None).project(|p| {
        p.resources.get_mut(ResourceUid::new(1)).unwrap().is_inactive = true;
        p.resources.get_mut(ResourceUid::new(3)).unwrap().is_generic = true;
    });
    let net = assign(assign(net, 1, "A", 1, 480), 2, "B", 1, 480);
    let net = assign(assign(net, 3, "A", 2, 480), 4, "C", 2, 480);
    let run = assign(assign(net, 5, "A", 3, 480), 6, "B", 3, 480).run();
    let load = |r: u32| &run.schedule.resources[&ResourceUid::new(r)];
    assert!(!load(1).overallocated && load(1).peak == Units::ZERO);
    assert_eq!(load(1).work, work(960));
    assert!(!load(2).overallocated);
    assert_eq!(load(2).work, work(480));
    assert!(load(3).overallocated);
    let flagged: Vec<_> = run
        .schedule
        .diagnostics
        .iter()
        .filter(|d| d.code() == "resource.overallocated")
        .collect();
    assert_eq!(flagged.len(), 1, "{flagged:?}");
}

#[test]
fn r187_availability_ignored_diag() {
    // Disponibilités datées relues : ignorées (capacité constante), une
    // information par ressource.
    let net = resource(Net::new("2026-10-19T08:00").task("A", 480), 1, None).project(|p| {
        p.resources
            .get_mut(ResourceUid::new(1))
            .unwrap()
            .extras
            .0
            .push(ExtraEntry {
                origin: "mspdi".to_owned(),
                key: "AvailabilityPeriods".to_owned(),
                payload: "<AvailabilityPeriods><AvailabilityPeriod><AvailableUnits>0.5</AvailableUnits></AvailabilityPeriod></AvailabilityPeriods>".to_owned(),
            });
    });
    let run = assign(net, 1, "A", 1, 480).run();
    let d: Vec<_> = run
        .schedule
        .diagnostics
        .iter()
        .filter(|d| d.code() == "resource.availability_ignored")
        .collect();
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].severity(), Severity::Info);
    assert!(!run.schedule.resources[&ResourceUid::new(1)].overallocated);
}

#[test]
fn r187_corpus_no_false_positive() {
    // A4 et A6 : `OverAllocated` 0 sur toutes leurs ressources (O31) ;
    // corpus absent : rien à vérifier.
    for name in ["shopmetrics.xml", "takeout-system-zh.xml"] {
        let Ok(bytes) = std::fs::read(tests_path(&format!("corpus/files/{name}"))) else {
            continue;
        };
        let doc = plannercraft_mspdi::read(&bytes, &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap();
        let s = schedule(&doc.project);
        let over: Vec<_> = s
            .resources
            .iter()
            .filter(|(_, l)| l.overallocated)
            .map(|(r, l)| (*r, l.overloads.first().copied()))
            .collect();
        assert!(over.is_empty(), "{name} : faux positifs {over:?}");
        let work_resources = doc
            .project
            .resources
            .iter()
            .filter(|r| r.kind == ResourceKind::Work && r.uid() != ResourceUid::new(0))
            .count();
        assert!(work_resources > 0, "{name}");
    }
}
