//! R-076 — Booléens, éléments absents et valeurs par défaut du schéma
//! (import MSPDI).

use plannercraft_core::{DurationUnit, ScheduleAnchor, TaskType, Weekday};
use plannercraft_mspdi::{ImportCode, MspdiDocument, ReadOptions, read};
use plannercraft_tests::{moment, tests_path, time};

fn fixture(name: &str) -> MspdiDocument {
    let bytes = std::fs::read(tests_path(&format!("fixtures/mspdi/{name}"))).unwrap();
    read(&bytes, &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap()
}

/// Projet minimal autour de `body` (champs de `Project`).
fn project(body: &str) -> MspdiDocument {
    let xml = format!(
        "<Project xmlns=\"http://schemas.microsoft.com/project\">\
         <StartDate>2026-10-19T08:00:00</StartDate><FinishDate>2026-10-30T17:00:00</FinishDate>\
         <CalendarUID>1</CalendarUID>{body}</Project>"
    );
    read(xml.as_bytes(), &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap()
}

fn has(doc: &MspdiDocument, code: &ImportCode, element: &str) -> bool {
    doc.diagnostics
        .iter()
        .any(|d| &d.code == code && d.element == element)
}

#[test]
fn r076_absent_elements_take_schema_defaults() {
    let doc = fixture("project_defaults.xml");
    let o = doc.project.options;
    // `default=` du schéma : planification depuis le début, contraintes
    // respectées, effort et estimation vrais, durée fixe.
    assert_eq!(
        doc.project.anchor,
        ScheduleAnchor::FromStart(moment("2026-10-19T08:00"))
    );
    assert!(o.honor_constraints);
    assert!(o.new_tasks_effort_driven);
    assert!(o.new_tasks_estimated);
    assert_eq!(o.default_task_type, TaskType::FixedDuration);
    assert_eq!(o.critical_slack_limit_days, 0);
    assert!(!o.multiple_critical_paths);
    // Absents sans défaut du schéma : valeurs du modèle (R-032).
    assert_eq!(o.factors.minutes_per_day(), 480);
    assert_eq!(o.factors.minutes_per_week(), 2_400);
    assert_eq!(o.factors.days_per_month(), 20);
    assert_eq!(o.default_duration_unit, DurationUnit::Day);
    assert_eq!(o.week_start_day, Weekday::Monday);
    // `CurrentDate` absent : « maintenant » injecté ; `StatusDate` absent : NA.
    assert_eq!(doc.project.current_date, moment("2026-01-01T00:00"));
    assert_eq!(doc.project.status_date, None);
    assert_eq!(doc.source.save_version, Some(12));
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
}

#[test]
fn r076_explicit_values_override_defaults() {
    let doc = fixture("project_options.xml");
    let p = &doc.project;
    let o = p.options;
    assert!(!o.honor_constraints);
    assert!(!o.new_tasks_effort_driven);
    assert!(!o.new_tasks_estimated);
    assert_eq!(o.default_task_type, TaskType::FixedUnits);
    assert_eq!(o.week_start_day, Weekday::Sunday);
    assert_eq!(o.factors.minutes_per_day(), 450);
    assert_eq!(o.factors.minutes_per_week(), 2_250);
    assert_eq!(o.factors.days_per_month(), 21);
    assert_eq!(o.default_start_time, time("07:30"));
    assert_eq!(o.default_finish_time, time("16:30"));
    // `DurationFormat` 5 = heures ; `WorkFormat` 3 = jours.
    assert_eq!(o.default_duration_unit, DurationUnit::Hour);
    assert_eq!(o.default_work_unit, DurationUnit::Day);
    assert_eq!(p.current_date, moment("2026-10-20T08:00"));
    assert_eq!(p.status_date, Some(moment("2026-10-21T17:00")));
    assert_eq!(p.info.name, "options.xml");
    assert_eq!(p.info.category, "Catégorie");
    assert_eq!(p.info.creation_date, Some(moment("2026-10-01T09:30")));
    assert_eq!(
        p.info.guid.map(|g| g.to_string()).as_deref(),
        Some("0A1B2C3D-4E5F-6071-8293-A4B5C6D7E8F9")
    );
    // Valeurs enregistrées du projet : à part.
    assert_eq!(doc.recorded.project_start, Some(moment("2026-10-19T08:00")));
    assert_eq!(doc.recorded.project_finish, Some(moment("2026-10-23T17:00")));
    assert_eq!(doc.source.ui_sections, ["Views", "Filters"]);
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
}

#[test]
fn r076_invalid_boolean_takes_default_with_diagnostic() {
    // `2` n'est pas un booléen : défaut du schéma (vrai) et diagnostic.
    let doc = project("<HonorConstraints>2</HonorConstraints><NewTasksEstimated> false </NewTasksEstimated>");
    assert!(doc.project.options.honor_constraints);
    assert!(has(&doc, &ImportCode::InvalidValue, "HonorConstraints"));
    // Espaces autour d'un booléen valide : tolérés.
    assert!(!doc.project.options.new_tasks_estimated);
}

#[test]
fn r076_out_of_range_factor_takes_default() {
    // 0 minute par jour : hors bornes (1 à 1 440), défaut 480, diagnostic.
    let doc = project("<MinutesPerDay>0</MinutesPerDay><DaysPerMonth>abc</DaysPerMonth>");
    assert_eq!(doc.project.options.factors.minutes_per_day(), 480);
    assert_eq!(doc.project.options.factors.days_per_month(), 20);
    assert!(has(&doc, &ImportCode::InvalidValue, "MinutesPerDay"));
    assert!(has(&doc, &ImportCode::InvalidValue, "DaysPerMonth"));
}

#[test]
fn r076_schedule_from_finish_default_and_value() {
    // `ScheduleFromStart` absent : vrai (défaut du schéma) ; faux :
    // planification à rebours sur `FinishDate` (R-089), sans diagnostic.
    let doc = project("");
    assert!(matches!(doc.project.anchor, ScheduleAnchor::FromStart(_)));
    let doc = project("<ScheduleFromStart>0</ScheduleFromStart>");
    assert_eq!(
        doc.project.anchor,
        ScheduleAnchor::FromFinish(moment("2026-10-30T17:00"))
    );
    assert!(
        doc.diagnostics
            .iter()
            .all(|d| !d.code.code().starts_with("mspdi.")),
        "{:?}",
        doc.diagnostics
    );
}

#[test]
fn r076_missing_start_date_uses_injected_now() {
    let xml = "<Project><CalendarUID>1</CalendarUID></Project>";
    let doc = read(xml.as_bytes(), &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap();
    assert_eq!(
        doc.project.anchor,
        ScheduleAnchor::FromStart(moment("2026-01-01T00:00"))
    );
    assert!(has(&doc, &ImportCode::MissingRequired, "StartDate"));
    assert_eq!(doc.recorded.project_start, None);
}

#[test]
fn r076_task_booleans() {
    // `SaveVersion` 12, tâche sans `Active`, `Manual`, `IsNull` : active,
    // automatique, non vide.
    let xml = "<Project><SaveVersion>12</SaveVersion><StartDate>2026-10-19T08:00:00</StartDate>\
               <Tasks><Task><UID>1</UID><ID>1</ID><Duration>PT8H0M0S</Duration></Task></Tasks></Project>";
    let doc = read(xml.as_bytes(), &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap();
    let t = doc.project.tasks.get(plannercraft_core::TaskUid::new(1)).unwrap();
    assert!(t.active);
    assert!(!t.is_null);
    assert_eq!(t.mode, plannercraft_core::SchedulingMode::Auto);
    // Fixture `tasks.xml` : UID 6 `<Milestone>2</Milestone>` → faux, signalé ;
    // UID 7 `<Active>0</Active>` → inactive.
    let doc = fixture("tasks.xml");
    let t6 = doc.project.tasks.get(plannercraft_core::TaskUid::new(6)).unwrap();
    assert!(!t6.milestone);
    assert!(has(&doc, &ImportCode::InvalidValue, "Tasks/Task/Milestone"));
    assert!(
        !doc.project
            .tasks
            .get(plannercraft_core::TaskUid::new(7))
            .unwrap()
            .active
    );
    // Type de tâche absent : `DefaultTaskType` du projet (absent → durée fixe).
    let doc = read(xml.as_bytes(), &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap();
    assert_eq!(
        doc.project
            .tasks
            .get(plannercraft_core::TaskUid::new(1))
            .unwrap()
            .task_type,
        TaskType::FixedDuration
    );
}
