//! R-218 — Options de la date d'état (S54, O36) : lues et réécrites telles
//! quelles ; une option que le calcul ne suit pas est signalée une fois
//! (`progress.option_unsupported`), le calcul garde les valeurs par défaut.

use plannercraft_core::{Diagnostic, ProgressOption, Severity, schedule};
use plannercraft_mspdi::write::{Computed, WriteOptions, write};
use plannercraft_mspdi::{ImportCode, MspdiDocument, ReadOptions, read};
use plannercraft_tests::moment;
use plannercraft_tests::mspdi::{compact, text};

/// Document à une tâche de 2 j avec les options `options`.
fn doc(options: &str) -> MspdiDocument {
    let xml = format!(
        "<Project><StartDate>2026-10-19T08:00:00</StartDate><CalendarUID>1</CalendarUID>{options}\
         <Calendars><Calendar><UID>1</UID><Name>Standard</Name><IsBaseCalendar>1</IsBaseCalendar></Calendar></Calendars>\
         <Tasks><Task><UID>1</UID><ID>1</ID><Name>T</Name><Duration>PT16H0M0S</Duration></Task></Tasks></Project>"
    );
    read(xml.as_bytes(), &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap()
}

fn unsupported(d: &MspdiDocument) -> Vec<(ProgressOption, String)> {
    d.diagnostics
        .iter()
        .filter_map(|x| match &x.code {
            ImportCode::Core(Diagnostic::ProgressOptionUnsupported { option }) => {
                Some((*option, x.element.clone()))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn r218_option_unsupported_diag() {
    // `TaskUpdatesResource` 0 et `MoveRemainingStartsBack` 1 : deux infos,
    // une par option, nommant l'élément ; les autres options à leur défaut
    // ne disent rien.
    let d = doc(
        "<TaskUpdatesResource>0</TaskUpdatesResource><MoveCompletedEndsBack>0</MoveCompletedEndsBack>\
         <MoveRemainingStartsBack>1</MoveRemainingStartsBack>",
    );
    assert_eq!(
        unsupported(&d),
        [
            (
                ProgressOption::TaskUpdatesResource,
                "TaskUpdatesResource".to_owned()
            ),
            (
                ProgressOption::MoveRemainingStartsBack,
                "MoveRemainingStartsBack".to_owned()
            ),
        ]
    );
    let x = d
        .diagnostics
        .iter()
        .find(|x| {
            matches!(
                x.code,
                ImportCode::Core(Diagnostic::ProgressOptionUnsupported { .. })
            )
        })
        .unwrap();
    assert_eq!(x.code.code(), "core.progress.option_unsupported");
    assert_eq!(x.severity, Severity::Info);
    // Valeurs gardées dans le modèle et réécrites telles quelles.
    let o = d.project.options.progress;
    assert!(!o.task_updates_resource && o.move_remaining_starts_back);
    let xml = compact(&text(&write(
        &d.project,
        Computed::Omit,
        &WriteOptions::default(),
    )));
    assert!(xml.contains("<TaskUpdatesResource>0</TaskUpdatesResource>"));
    assert!(xml.contains("<MoveRemainingStartsBack>1</MoveRemainingStartsBack>"));
    // Les cinq fichiers ont les valeurs par défaut (O36) : rien à signaler.
    assert_eq!(unsupported(&doc("")), Vec::new());
}

#[test]
fn r218_options_do_not_change_the_schedule() {
    // Calcul aux valeurs par défaut : options `Move*` à 1 ⇒ même planning.
    let base = doc("");
    let moved = doc(
        "<MoveCompletedEndsBack>1</MoveCompletedEndsBack><MoveRemainingStartsBack>1</MoveRemainingStartsBack>\
         <MoveRemainingStartsForward>1</MoveRemainingStartsForward><MoveCompletedEndsForward>1</MoveCompletedEndsForward>",
    );
    assert_eq!(unsupported(&moved).len(), 4);
    assert_eq!(schedule(&moved.project).tasks, schedule(&base.project).tasks);
}
