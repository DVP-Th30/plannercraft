//! JSON de référence d'un petit projet complet, écrit à la main
//! (`tests/golden/project_v1.json`) : la lecture donne les valeurs attendues
//! et l'écriture redonne le fichier à l'identique (ADR-0008, § 9.2).

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use plannercraft_core::calendar::{CalendarKind, DayRule};
use plannercraft_core::{
    CalendarUid, Constraint, DurationUnit, Lag, LagValue, LinkType, Minutes, Moment, Project, ResourceKind,
    ResourceUid, SchedulingMode, TaskId, TaskUid, TimeBasis,
};

const GOLDEN: &str = include_str!("golden/project_v1.json");

fn m(s: &str) -> Moment {
    s.parse().unwrap()
}

#[test]
fn golden_project_reads_expected_values() {
    let p: Project = serde_json::from_str(GOLDEN).unwrap();
    assert_eq!(p.info.name, "Chantier");
    assert_eq!(p.calendar, CalendarUid::new(1));
    assert_eq!(p.calendars.len(), 2);
    let paul = p.calendars.get(CalendarUid::new(4)).unwrap();
    match paul.kind() {
        CalendarKind::Derived { base, week } => {
            assert_eq!(*base, CalendarUid::new(1));
            assert_eq!(week.monday, DayRule::Inherit);
            assert!(matches!(week.friday, DayRule::Working(_)));
        }
        CalendarKind::Base { .. } => panic!("Paul est un calendrier dérivé"),
    }
    assert_eq!(paul.work_weeks().len(), 1);
    // Ordre des ID : Fondations (1), ligne vide (2), Murs (3).
    assert_eq!(p.task_id(TaskUid::new(2)), Some(TaskId::new(3)));
    assert!(p.task_by_id(TaskId::new(2)).unwrap().is_null);
    let murs = p.tasks.get(TaskUid::new(2)).unwrap();
    assert_eq!(murs.duration.minutes(), Minutes::new(4_800));
    assert_eq!(murs.duration.unit(), DurationUnit::Week);
    assert!(murs.duration.is_estimated());
    assert_eq!(
        murs.constraint,
        Constraint::StartNoEarlierThan {
            date: m("2026-10-26T08:00")
        }
    );
    assert_eq!(murs.predecessors.len(), 1);
    assert_eq!(murs.predecessors[0].kind, LinkType::FinishToStart);
    assert_eq!(
        murs.predecessors[0].lag,
        Lag::new(
            TimeBasis::Working,
            LagValue::Time {
                minutes: Minutes::new(-480),
                unit: DurationUnit::Day
            }
        )
        .unwrap()
    );
    assert!(matches!(murs.mode, SchedulingMode::Manual { start: Some(_), .. }));
    assert_eq!(murs.extras.0.len(), 1);
    let fondations = p.tasks.get(TaskUid::new(1)).unwrap();
    assert_eq!(fondations.progress.percent_complete.get(), 5_000);
    assert_eq!(fondations.baselines.len(), 1);
    let beton = p.resources.get(ResourceUid::new(2)).unwrap();
    assert_eq!(
        beton.kind,
        ResourceKind::Material {
            label: "m³".to_owned()
        }
    );
    assert_eq!(p.assignments.len(), 1);
    assert_eq!(p.effective_status_date(), m("2026-10-17T08:00"));
}

#[test]
fn golden_project_writes_back_identically() {
    let p: Project = serde_json::from_str(GOLDEN).unwrap();
    let written = serde_json::to_string_pretty(&p).unwrap();
    assert_eq!(written, GOLDEN.trim_end());
}

#[test]
fn golden_project_is_valid() {
    let p: Project = serde_json::from_str(GOLDEN).unwrap();
    assert_eq!(p.validate().finish(), []);
}

/// Contrat J5 (lot L0) : les champs nouveaux (`leveling_delay` des tâches et
/// des affectations) sont absents du JSON quand ils sont vides ; la
/// référence V1 se relit et se réécrit à l'identique, et un retard posé
/// fait l'aller-retour.
#[test]
fn project_v1_golden_unchanged() {
    assert!(!GOLDEN.contains("leveling_delay"));
    let mut p: Project = serde_json::from_str(GOLDEN).unwrap();
    assert!(p.tasks.iter().all(|t| t.leveling_delay.is_none()));
    assert!(p.assignments.iter().all(|a| a.leveling_delay.is_none()));
    assert_eq!(serde_json::to_string_pretty(&p).unwrap(), GOLDEN.trim_end());

    let delay = plannercraft_core::LevelingDelay::new(Minutes::new(4_320), DurationUnit::Day, true).unwrap();
    let task = p.tasks.iter().next().unwrap().uid();
    p.tasks.get_mut(task).unwrap().leveling_delay = Some(delay);
    let assignment = p.assignments.iter().next().unwrap().uid();
    p.assignments.get_mut(assignment).unwrap().leveling_delay = Some(delay);
    let json = serde_json::to_string(&p).unwrap();
    assert_eq!(
        json.matches(r#""leveling_delay":{"minutes":4320,"unit":"day","elapsed":true}"#)
            .count(),
        2
    );
    let back: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(back, p);

    // Contrat J6 (lot L0) : reprise saisie (`Progress::resume`) et options
    // de l'avancement absentes du JSON à leurs défauts ; posées, elles font
    // l'aller-retour.
    assert!(!GOLDEN.contains("resume") && !GOLDEN.contains("split_in_progress"));
    let mut p: Project = serde_json::from_str(GOLDEN).unwrap();
    assert!(p.tasks.iter().all(|t| t.progress.resume.is_none()));
    assert!(p.options.progress.is_default());
    p.tasks.get_mut(task).unwrap().progress.resume = Some(m("2026-10-20T08:00"));
    p.options.progress.split_in_progress = false;
    let json = serde_json::to_string(&p).unwrap();
    assert!(json.contains(r#""resume":"2026-10-20T08:00""#), "{json}");
    assert!(json.contains(r#""split_in_progress":false"#), "{json}");
    let back: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(back, p);
}
