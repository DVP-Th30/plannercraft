//! R-032 — Valeurs par défaut d'un projet neuf et d'une nouvelle tâche.

use plannercraft_core::calendar::CalendarKind;
use plannercraft_core::{
    BuiltinCalendarNames, CalendarUid, Constraint, DurationUnit, Minutes, Project, ProjectInit,
    ProjectOptions, ScheduleAnchor, SchedulingMode, Task, TaskType, TaskUid, TimeBasis, TimeOfDay, Weekday,
};
use plannercraft_tests::moment;

fn new_project() -> Project {
    Project::new(ProjectInit {
        name: "Chantier".to_owned(),
        anchor: ScheduleAnchor::FromStart(moment("2026-10-19T08:00")),
        current_date: moment("2026-10-17T08:00"),
        calendar_names: BuiltinCalendarNames::default(),
    })
}

#[test]
fn r032_default_options() {
    let o = ProjectOptions::default();
    // 8 h = 480 min par jour, 40 h = 2 400 min par semaine, 20 jours par mois (S8).
    assert_eq!(o.factors.minutes_per_day(), 480);
    assert_eq!(o.factors.minutes_per_week(), 2_400);
    assert_eq!(o.factors.days_per_month(), 20);
    // Début 08:00, fin 17:00 (S8, S9).
    assert_eq!(o.default_start_time, TimeOfDay::hm(8, 0).unwrap());
    assert_eq!(o.default_finish_time, TimeOfDay::hm(17, 0).unwrap());
    // Durées en jours (S10), travail en heures.
    assert_eq!(o.default_duration_unit, DurationUnit::Day);
    assert_eq!(o.default_work_unit, DurationUnit::Hour);
    // Unités fixes, sans pilotage par l'effort (hypothèse), estimées (S14).
    assert_eq!(o.default_task_type, TaskType::FixedUnits);
    assert!(!o.new_tasks_effort_driven);
    assert!(o.new_tasks_estimated);
    // Semaine commençant le lundi (hypothèse), contraintes respectées (S2).
    assert_eq!(o.week_start_day, Weekday::Monday);
    assert!(o.honor_constraints);
    // Seuil de criticité 0 jour (S19) et un seul chemin critique (S20).
    assert_eq!(o.critical_slack_limit_days, 0);
    assert!(!o.multiple_critical_paths);
}

#[test]
fn r032_new_project() {
    let p = new_project();
    assert_eq!(p.options, ProjectOptions::default());
    // Planification depuis le début (S6), sans date d'état.
    assert_eq!(p.anchor, ScheduleAnchor::FromStart(moment("2026-10-19T08:00")));
    assert_eq!(p.status_date, None);
    // Calendriers Standard (UID 1, calendrier du projet), 24 heures (UID 2),
    // Équipe de nuit (UID 3), tous de base.
    assert_eq!(p.calendar, CalendarUid::new(1));
    let names: Vec<(u32, &str)> = p
        .calendars
        .iter()
        .map(|c| (c.uid().get(), c.name.as_str()))
        .collect();
    assert_eq!(names, [(1, "Standard"), (2, "24 heures"), (3, "Équipe de nuit")]);
    assert!(
        p.calendars
            .iter()
            .all(|c| matches!(c.kind(), CalendarKind::Base { .. }))
    );
    assert!(p.tasks.is_empty() && p.resources.is_empty() && p.assignments.is_empty());
    assert!(p.summary_task.is_none());
    assert_eq!(p.info.name, "Chantier");
    assert_eq!(p.info.creation_date, Some(moment("2026-10-17T08:00")));
}

#[test]
fn r032_new_task_is_one_estimated_day() {
    let p = new_project();
    let t = Task::new(TaskUid::new(1), "Fondations".to_owned(), &p.options);
    // « 1j? » : 480 min, unité jour, ouvrée, estimée (S14).
    assert_eq!(t.duration.minutes(), Minutes::new(480));
    assert_eq!(t.duration.unit(), DurationUnit::Day);
    assert_eq!(t.duration.basis(), TimeBasis::Working);
    assert!(t.duration.is_estimated());
    assert_eq!(
        serde_json::to_string(&t.duration).unwrap(),
        r#"{"minutes":480,"unit":"day","basis":"working","estimated":true}"#
    );
    assert_eq!(t.task_type, TaskType::FixedUnits);
    assert!(!t.effort_driven);
    assert_eq!(t.constraint, Constraint::AsSoonAsPossible);
    assert_eq!(t.priority.get(), 500);
    assert_eq!(t.mode, SchedulingMode::Auto);
    assert!(t.active && !t.milestone && !t.is_null);
    assert_eq!(t.outline_level.get(), 1);
}
