//! Robustesse du moteur face à la taille (spécification J2 § 5.2) : chaîne
//! FS de 50 000 tâches, plan de 2 000 niveaux, 10 000 liens vers une seule
//! tâche. Aucune panique, aucun débordement de pile (en debug).

#![allow(
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use plannercraft_core::calendar::BuiltinCalendarNames;
use plannercraft_core::{
    Link, Minutes, Moment, OutlineLevel, Project, ProjectInit, ScheduleAnchor, Task, TaskRole, TaskUid,
    schedule,
};

/// Chaîne FS de `n` tâches d'un jour.
fn chain(n: u32, start: &str) -> Project {
    let start: Moment = start.parse().unwrap();
    let mut p = Project::new(ProjectInit {
        name: "Chaîne".to_owned(),
        anchor: ScheduleAnchor::FromStart(start),
        current_date: start,
        calendar_names: BuiltinCalendarNames::default(),
    });
    let options = p.options;
    for i in 1..=n {
        let mut t = Task::new(TaskUid::new(i), String::new(), &options);
        if i > 1 {
            t.predecessors.push(Link::finish_to_start(TaskUid::new(i - 1)));
        }
        p.tasks.push(t).unwrap();
    }
    p
}

#[test]
fn deep_chain_of_50_000_tasks() {
    // 50 000 jours ouvrés dépassent 2149 : la fin de chaîne est non
    // planifiable et dégradée, sans panique ni récursion.
    let s = schedule(&chain(50_000, "1984-01-02T08:00"));
    assert_eq!(s.tasks.len(), 50_000);
    assert!(s.diagnostics.iter().any(|d| d.code() == "task.unschedulable"));
    let first = s.scheduled(TaskUid::new(1)).unwrap();
    assert_eq!(first.total_slack, Minutes::ZERO);
}

#[test]
fn deep_outline_of_2_000_levels() {
    let mut p = chain(2_000, "2026-10-19T08:00");
    let uids: Vec<TaskUid> = p.tasks.iter().map(Task::uid).collect();
    for (i, uid) in uids.iter().enumerate() {
        let t = p.tasks.get_mut(*uid).unwrap();
        t.predecessors.clear();
        t.outline_level = OutlineLevel::new(u16::try_from(i + 1).unwrap()).unwrap();
    }
    // Lien de la dernière (feuille) vers une tâche de premier niveau.
    let options = p.options;
    let mut last = Task::new(TaskUid::new(5_000), String::new(), &options);
    last.predecessors.push(Link::finish_to_start(TaskUid::new(1)));
    p.tasks.push(last).unwrap();
    let s = schedule(&p);
    let top = s.scheduled(TaskUid::new(1)).unwrap();
    assert_eq!(top.role, TaskRole::Summary);
    assert_eq!(top.early, s.scheduled(TaskUid::new(2_000)).unwrap().early);
    assert!(s.project_summary.is_some());
}

#[test]
fn ten_thousand_links_into_one_task() {
    let mut p = chain(10_001, "2026-10-19T08:00");
    let uids: Vec<TaskUid> = p.tasks.iter().map(Task::uid).collect();
    for uid in &uids {
        p.tasks.get_mut(*uid).unwrap().predecessors.clear();
    }
    let target = p.tasks.get_mut(TaskUid::new(10_001)).unwrap();
    for i in (1..=10_000).rev() {
        target.predecessors.push(Link::finish_to_start(TaskUid::new(i)));
    }
    let s = schedule(&p);
    let t = s.scheduled(TaskUid::new(10_001)).unwrap();
    assert_eq!(t.early.start, "2026-10-20T08:00".parse::<Moment>().unwrap());
}
