//! R-088 — Lecture : ressources, affectations, lignes de base, travail des
//! tâches (fixture `resources.xml`).

use plannercraft_core::{
    AssignmentUid, BaselineNumber, Minutes, ResourceKind, ResourceUid, TaskUid, Units, WorkContour,
};
use plannercraft_mspdi::{ImportCode, MspdiDocument, ReadOptions, read};
use plannercraft_tests::{moment, tests_path};

pub(crate) fn fixture() -> MspdiDocument {
    let bytes = std::fs::read(tests_path("fixtures/mspdi/resources.xml")).unwrap();
    read(&bytes, &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap()
}

fn has(doc: &MspdiDocument, code: &ImportCode, element: &str) -> bool {
    doc.diagnostics
        .iter()
        .any(|d| &d.code == code && d.element == element)
}

#[test]
fn r088_resources_are_read() {
    let doc = fixture();
    let r = |uid: u32| doc.project.resources.get(ResourceUid::new(uid)).unwrap();
    assert_eq!(doc.project.resources.len(), 4);
    let ana = r(1);
    assert_eq!(
        (
            ana.name.as_str(),
            ana.initials.as_str(),
            ana.group.as_str(),
            ana.notes.as_str()
        ),
        ("Ana", "A", "Études", "Note")
    );
    assert_eq!(ana.kind, ResourceKind::Work);
    // `MaxUnits` « 1.00 » → 10 000 (1/10 000, R-001).
    assert_eq!(ana.max_units, Units::FULL);
    assert_eq!(ana.calendar, Some(plannercraft_core::CalendarUid::new(2)));
    assert!(!ana.is_generic && !ana.is_inactive);
    assert_eq!(
        r(2).kind,
        ResourceKind::Material {
            label: "m³".to_owned()
        }
    );
    // `IsCostResource 1` l'emporte sur `Type 1`.
    assert_eq!(r(3).kind, ResourceKind::Cost);
    // `MaxUnits` « 0.5 » → 5 000 ; `CalendarUID -1` → aucun.
    assert_eq!(r(4).max_units, Units::new(5_000).unwrap());
    assert_eq!(r(4).calendar, None);
}

#[test]
fn r088_assignments_are_read() {
    let doc = fixture();
    let a = |uid: u32| doc.project.assignments.get(AssignmentUid::new(uid)).unwrap();
    // UID 3 : `TaskUID 99` inconnu → ignorée, `mspdi.dangling_reference`.
    assert_eq!(doc.project.assignments.len(), 3);
    assert!(doc.project.assignments.get(AssignmentUid::new(3)).is_none());
    assert!(has(
        &doc,
        &ImportCode::DanglingReference,
        "Assignments/Assignment/TaskUID"
    ));
    let one = a(1);
    assert_eq!(
        (one.task, one.resource),
        (TaskUid::new(1), Some(ResourceUid::new(1)))
    );
    // `Units` « 0.5 » → 5 000 ; travail ISO.
    assert_eq!(one.units, Units::new(5_000).unwrap());
    assert_eq!(one.work.minutes(), Minutes::new(960));
    assert_eq!(one.actual_work.minutes(), Minutes::new(240));
    assert_eq!(one.remaining_work.minutes(), Minutes::new(720));
    // `Delay` 4 200 dixièmes → 420 min (O20).
    assert_eq!(one.delay, Minutes::new(420));
    assert_eq!(one.contour, WorkContour::Flat);
    // UID 2 : `ResourceUID -65535` → aucune ressource ; `Delay` 25 → 3,
    // `mspdi.rounded`.
    assert_eq!(a(2).resource, None);
    assert_eq!(a(2).delay, Minutes::new(3));
    assert!(has(&doc, &ImportCode::Rounded, "Assignments/Assignment/Delay"));
    assert_eq!(a(2).contour, WorkContour::Contoured);
    // UID 4 : ressource 9 inconnue → gardée, signalée par `Project::validate`.
    assert_eq!(a(4).resource, Some(ResourceUid::new(9)));
    assert_eq!(a(4).units, Units::new(25_000).unwrap());
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.code.code() == "core.assignment.unknown_resource")
    );
    // Affectations comptées par tâche (R-083).
    assert_eq!(doc.recorded.tasks[&TaskUid::new(1)].assignments, 1);
    assert_eq!(doc.recorded.tasks[&TaskUid::new(2)].assignments, 2);
}

#[test]
fn r088_task_work_and_baselines_are_read() {
    let doc = fixture();
    let t1 = doc.project.tasks.get(TaskUid::new(1)).unwrap();
    assert_eq!(t1.work.minutes(), Minutes::new(960));
    assert_eq!(t1.progress.actual_work.minutes(), Minutes::new(240));
    assert_eq!(t1.progress.remaining_work.minutes(), Minutes::new(720));
    // Travail négatif → 0, `mspdi.invalid_value`.
    let t2 = doc.project.tasks.get(TaskUid::new(2)).unwrap();
    assert_eq!(t2.work.minutes(), Minutes::ZERO);
    assert!(has(&doc, &ImportCode::InvalidValue, "Tasks/Task/Work"));
    // Lignes de base 0 et 3 ; la n° 11 est ignorée, la seconde n° 3 aussi.
    let numbers: Vec<u8> = t1.baselines.keys().map(|n| n.get()).collect();
    assert_eq!(numbers, [0, 3]);
    assert!(has(&doc, &ImportCode::InvalidValue, "Tasks/Task/Baseline/Number"));
    assert!(has(&doc, &ImportCode::Duplicate, "Tasks/Task/Baseline/Number"));
    let b3 = t1.baselines[&BaselineNumber::new(3).unwrap()];
    assert_eq!(b3.start, Some(moment("2026-10-20T08:00")));
    assert_eq!(b3.finish, None);
    assert_eq!(b3.duration.unwrap().minutes(), Minutes::new(480));
}

#[test]
fn r088_computed_values_are_dropped_and_counted() {
    // X : `PeakUnits`, `Cost` de la ressource 1 ; `Cost` de l'affectation
    // 1 ; `BCWS` des lignes de base ; répartitions de type 1 de
    // l'affectation 1 (profil uniforme). Amendement J5 (R-198) : `Work` de
    // la ressource et `Start` de l'affectation sont relus (classe C), plus
    // écartés.
    let doc = fixture();
    let d = |name: &str| doc.source.dropped_computed.get(name).copied();
    assert_eq!(d("PeakUnits"), Some(1));
    assert_eq!(d("Work"), None);
    assert_eq!(d("Cost"), Some(2));
    assert_eq!(d("Start"), None);
    let r1 = doc.recorded.resources[&plannercraft_core::ResourceUid::new(1)];
    assert_eq!(r1.work.unwrap().minutes(), Minutes::new(12_000));
    let a1 = doc.recorded.assignments[&plannercraft_core::AssignmentUid::new(1)];
    assert_eq!(a1.start, Some(moment("2026-10-19T15:00")));
    assert_eq!(d("BCWS"), Some(2));
    assert_eq!(d("TimephasedData"), Some(2));
}
