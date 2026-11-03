//! R-193 — Travail fixe toujours piloté par l'effort, récapitulative jamais
//! (S41 : « always effort-driven » ; S42 : « Summary tasks … can't be set
//! to effort-driven », « You can't remove effort-driven scheduling from
//! fixed-work tasks »). Unités nulles refusées en travail fixe.

use crate::r170_work_equals_duration_times_units::{DAY, part, task, uid, units, work};
use plannercraft_core::Minutes;
use plannercraft_core::TaskType::{FixedUnits, FixedWork};
use plannercraft_core::resource::{Edit, EditError, Recalculated, apply, edit::effective_effort_driven};

#[test]
fn r193_fw_effort_driven_refused() {
    let parts = [part(1, 10_000, 960)];
    let side = task(FixedWork, true, 2 * DAY, &parts);
    assert_eq!(
        apply(&side, &parts, &Edit::EffortDriven(false)),
        Err(EditError::FixedWorkEffortDriven)
    );
    assert_eq!(
        EditError::FixedWorkEffortDriven.code(),
        "edit.fixed_work_effort_driven"
    );
    // `true` est accepté et ne recalcule rien.
    let out = apply(&side, &parts, &Edit::EffortDriven(true)).unwrap();
    assert_eq!(out.recalculated, Recalculated::Nothing);
}

#[test]
fn r193_type_fixed_work_makes_effort_driven() {
    // Passage en travail fixe d'une tâche non pilotée : accepté, rien de
    // recalculé ; le pilotage effectif devient vrai (la commande l'écrit).
    let parts = [part(1, 10_000, 960)];
    let side = task(FixedUnits, false, 2 * DAY, &parts);
    let out = apply(&side, &parts, &Edit::Type(FixedWork)).unwrap();
    assert_eq!(out.recalculated, Recalculated::Nothing);
    assert!(effective_effort_driven(FixedWork, false));
    assert!(!effective_effort_driven(FixedUnits, false));
}

#[test]
fn r193_summary_refused() {
    let mut side = task(FixedUnits, false, 2 * DAY, &[]);
    side.summary = true;
    for edit in [
        Edit::Duration(Minutes::new(DAY)),
        Edit::Work(work(60)),
        Edit::Add {
            uid: uid(1),
            units: units(10_000),
        },
        Edit::Type(FixedWork),
        Edit::EffortDriven(true),
    ] {
        assert_eq!(apply(&side, &[], &edit), Err(EditError::SummaryNotEditable));
    }
}

#[test]
fn r193_zero_units_fw_refused() {
    let parts = [part(1, 10_000, 960)];
    let side = task(FixedWork, true, 2 * DAY, &parts);
    let zero = Edit::Units {
        uid: uid(1),
        units: units(0),
    };
    assert_eq!(apply(&side, &parts, &zero), Err(EditError::ZeroUnitsFixedWork));
    let add = Edit::Add {
        uid: uid(2),
        units: units(0),
    };
    assert_eq!(apply(&side, &parts, &add), Err(EditError::ZeroUnitsFixedWork));
    // Passage en travail fixe avec une affectation à unités nulles : refusé.
    let parts = [part(1, 0, 0)];
    let side = task(FixedUnits, false, 2 * DAY, &parts);
    assert_eq!(
        apply(&side, &parts, &Edit::Type(FixedWork)),
        Err(EditError::ZeroUnitsFixedWork)
    );
}

#[test]
fn r193_import_fixed_work_forced_effort_driven() {
    // Import (lot L3) : feuille à travail fixe (`Type` 2) relue avec
    // `EffortDriven` 0 → pilotée, `task.fixed_work_effort_driven` ; une
    // récapitulative à travail fixe n'est jamais pilotée (gardée telle
    // quelle) ; une feuille à unités fixes non pilotée reste non pilotée.
    use plannercraft_core::{Diagnostic, TaskUid};
    use plannercraft_mspdi::{ImportCode, ReadOptions, read};
    let xml = "<Project><StartDate>2026-10-19T08:00:00</StartDate><Tasks>\
        <Task><UID>1</UID><ID>1</ID><OutlineLevel>1</OutlineLevel><Type>2</Type><EffortDriven>0</EffortDriven><Duration>PT16H0M0S</Duration></Task>\
        <Task><UID>2</UID><ID>2</ID><OutlineLevel>2</OutlineLevel><Type>2</Type><EffortDriven>0</EffortDriven><Duration>PT16H0M0S</Duration></Task>\
        <Task><UID>3</UID><ID>3</ID><OutlineLevel>1</OutlineLevel><Type>0</Type><EffortDriven>0</EffortDriven><Duration>PT16H0M0S</Duration></Task>\
        </Tasks></Project>";
    let doc = read(
        xml.as_bytes(),
        &ReadOptions::new(plannercraft_tests::moment("2026-01-01T00:00")),
    )
    .unwrap();
    let driven = |uid: u32| doc.project.tasks.get(TaskUid::new(uid)).unwrap().effort_driven;
    assert!(!driven(1), "récapitulative");
    assert!(driven(2), "feuille à travail fixe");
    assert!(!driven(3), "unités fixes");
    let forced: Vec<_> = doc
        .diagnostics
        .iter()
        .filter_map(|d| match &d.code {
            ImportCode::Core(Diagnostic::TaskFixedWorkEffortDriven { task }) => {
                Some((*task, d.element.clone()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(forced, [(TaskUid::new(2), "Tasks/Task/EffortDriven".to_owned())]);
    assert_eq!(
        doc.diagnostics
            .iter()
            .find(|d| matches!(d.code, ImportCode::Core(_)))
            .unwrap()
            .code
            .code(),
        "core.task.fixed_work_effort_driven"
    );
}
