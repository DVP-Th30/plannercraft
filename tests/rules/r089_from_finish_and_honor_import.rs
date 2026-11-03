//! R-089 — Lecture MSPDI : planification à rebours (`ScheduleFromStart` 0 →
//! ancre sur `FinishDate`, `StartDate` gardée dans le planning enregistré)
//! et option « les tâches respectent toujours leurs dates de contrainte »
//! (`HonorConstraints` lu littéralement ; à 0 avec une tâche automatique
//! active de contrainte datée : `mspdi.honor_constraints_off`, info).
//! Source : schéma MSPDI, élément Project (S34) ; O15 (hypothèse
//! concurrente : codage inversé).

use plannercraft_core::{ScheduleAnchor, Severity};
use plannercraft_mspdi::{ImportCode, MspdiDocument, ReadOptions, read};
use plannercraft_tests::moment;

/// Projet minimal : champs `head` de `Project`, puis `tasks` (contenu de
/// `Tasks`).
fn project(head: &str, tasks: &str) -> MspdiDocument {
    let xml = format!(
        "<Project xmlns=\"http://schemas.microsoft.com/project\">{head}\
         <CalendarUID>1</CalendarUID><Tasks>{tasks}</Tasks></Project>"
    );
    read(xml.as_bytes(), &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap()
}

const FNET_TASK: &str = "<Task><UID>1</UID><ID>1</ID><Name>A</Name><Duration>PT8H0M0S</Duration>\
    <ConstraintType>6</ConstraintType><ConstraintDate>2026-10-22T17:00:00</ConstraintDate></Task>";

/// Codes des diagnostics propres à l'import (`mspdi.*`) ; ceux du cœur
/// (`core.*`, ici le calendrier 1 absent du document) sont écartés.
fn codes(doc: &MspdiDocument) -> Vec<String> {
    doc.diagnostics
        .iter()
        .map(|d| d.code.code())
        .filter(|c| c.starts_with("mspdi."))
        .collect()
}

#[test]
fn r089_schedule_from_finish_anchor() {
    // `ScheduleFromStart 0`, `FinishDate` ven 23/10 17:00 → à rebours depuis
    // ven 17:00 ; `StartDate` lun 08:00 gardée dans le planning enregistré ;
    // aucun `mspdi.unsupported`.
    let doc = project(
        "<StartDate>2026-10-19T08:00:00</StartDate><FinishDate>2026-10-23T17:00:00</FinishDate>\
         <ScheduleFromStart>0</ScheduleFromStart>",
        "",
    );
    assert_eq!(
        doc.project.anchor,
        ScheduleAnchor::FromFinish(moment("2026-10-23T17:00"))
    );
    assert_eq!(doc.recorded.project_start, Some(moment("2026-10-19T08:00")));
    assert!(codes(&doc).is_empty(), "{:?}", doc.diagnostics);
}

#[test]
fn r089_missing_finish_date() {
    // À rebours sans `FinishDate` : ancre sur `StartDate`, signalée ;
    // `StartDate` n'est pas exigée à rebours.
    let doc = project(
        "<StartDate>2026-10-19T08:00:00</StartDate><ScheduleFromStart>0</ScheduleFromStart>",
        "",
    );
    assert_eq!(
        doc.project.anchor,
        ScheduleAnchor::FromFinish(moment("2026-10-19T08:00"))
    );
    assert_eq!(codes(&doc), ["mspdi.missing_required"]);
    assert!(doc.diagnostics.iter().any(|d| d.element == "FinishDate"));
    let doc = project(
        "<FinishDate>2026-10-23T17:00:00</FinishDate><ScheduleFromStart>0</ScheduleFromStart>",
        "",
    );
    assert!(codes(&doc).is_empty(), "{:?}", doc.diagnostics);
    assert_eq!(doc.recorded.project_start, None);
}

#[test]
fn r089_honor_constraints_read_literally() {
    // 0 avec une tâche FNET → option fausse, `mspdi.honor_constraints_off`
    // (info, une fois).
    let head = "<StartDate>2026-10-19T08:00:00</StartDate><HonorConstraints>0</HonorConstraints>";
    let doc = project(head, FNET_TASK);
    assert!(!doc.project.options.honor_constraints);
    let off: Vec<_> = doc
        .diagnostics
        .iter()
        .filter(|d| d.code == ImportCode::HonorConstraintsOff)
        .collect();
    assert_eq!(off.len(), 1, "{:?}", doc.diagnostics);
    assert_eq!(off[0].code.code(), "mspdi.honor_constraints_off");
    assert_eq!(off[0].severity, Severity::Info);
    assert_eq!(off[0].element, "HonorConstraints");
    // 0 sans contrainte datée : rien à signaler.
    let doc = project(
        head,
        "<Task><UID>1</UID><ID>1</ID><Name>A</Name><Duration>PT8H0M0S</Duration></Task>",
    );
    assert!(!doc.project.options.honor_constraints);
    assert!(!codes(&doc).contains(&"mspdi.honor_constraints_off".to_owned()));
    // Absent → vrai (défaut du schéma), aucun diagnostic.
    let doc = project("<StartDate>2026-10-19T08:00:00</StartDate>", FNET_TASK);
    assert!(doc.project.options.honor_constraints);
    assert!(!codes(&doc).contains(&"mspdi.honor_constraints_off".to_owned()));
}

#[test]
fn r089_from_finish_file_is_scheduled() {
    // Bout à bout : un fichier à rebours se planifie (R-106) : A 1 j, F ven
    // 23/10 17:00 → A ven 08:00 – 17:00 (ALAP par défaut, R-108 ; ici la
    // contrainte lue est ALAP).
    let doc = project(
        "<StartDate>2026-10-19T08:00:00</StartDate><FinishDate>2026-10-23T17:00:00</FinishDate>\
         <ScheduleFromStart>0</ScheduleFromStart>",
        "<Task><UID>1</UID><ID>1</ID><Name>A</Name><Duration>PT8H0M0S</Duration>\
         <ConstraintType>1</ConstraintType></Task>",
    );
    let s = plannercraft_core::schedule(&doc.project);
    let a = s.scheduled(plannercraft_core::TaskUid::new(1)).unwrap();
    assert_eq!(a.start, moment("2026-10-23T08:00"));
    assert_eq!(s.project_finish, moment("2026-10-23T17:00"));
}
