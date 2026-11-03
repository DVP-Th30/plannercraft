//! Allocation des UID de tâche (R-151, lot L2) et des UID de ressource,
//! d'affectation et de calendrier (R-195, R-196, lot J5 L7).

use plannercraft_core::{AssignmentUid, CalendarUid, Project, ResourceUid, TaskUid};

use crate::error::{CommandError, CommandErrorKind, codes};

/// Allocateur d'UID de tâche d'une session : jamais réutilisés, l'undo ne
/// fait pas reculer le compteur (R-151).
///
/// `last` est le plus grand UID connu de la session : présent dans le projet
/// ou déjà alloué. Le suivant est `last + 1` par `checked_add` ; 0 (UID de la
/// récapitulative du projet) n'est jamais rendu.
///
/// Ressources, affectations et calendriers ont chacun leur compteur, de même
/// règle (R-195 : comme R-151, compteur propre) : plus grand UID connu de la
/// session, suivant par `checked_add`, jamais réutilisé après un undo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UidAlloc {
    last: u32,
    last_resource: u32,
    last_assignment: u32,
    last_calendar: u32,
}

/// Plus grands UID présents dans `p` : tâche, ressource, affectation,
/// calendrier (0 pour une arène vide).
fn max_uids(p: &Project) -> [u32; 4] {
    [
        max_task_uid(p),
        p.resources.max_uid().map_or(0, ResourceUid::get),
        p.assignments.max_uid().map_or(0, AssignmentUid::get),
        p.calendars.max_uid().map_or(0, CalendarUid::get),
    ]
}

/// `last + 1`, ou `limit.exceeded { what }` au-delà de `u32::MAX` (compteur
/// inchangé).
fn next(last: &mut u32, what: &'static str) -> Result<u32, CommandError> {
    let next = last.checked_add(1).ok_or_else(|| {
        CommandError::new(
            codes::LIMIT_EXCEEDED,
            CommandErrorKind::Limit {
                what,
                max: u64::from(u32::MAX),
            },
        )
    })?;
    *last = next;
    Ok(next)
}

impl UidAlloc {
    /// Allocateur d'un projet ouvert : chaque compteur part du plus grand
    /// UID présent de son arène (0 si elle est vide).
    pub fn from_project(p: &Project) -> Self {
        let [last, last_resource, last_assignment, last_calendar] = max_uids(p);
        Self {
            last,
            last_resource,
            last_assignment,
            last_calendar,
        }
    }

    /// UID suivant (R-151).
    ///
    /// # Errors
    ///
    /// `limit.exceeded { what: "task_uid" }` si `last` vaut `u32::MAX`.
    pub fn alloc(&mut self) -> Result<TaskUid, CommandError> {
        next(&mut self.last, "task_uid").map(TaskUid::new)
    }

    /// UID de ressource suivant (R-195). La ressource UID 0 des fichiers
    /// MS Project (O34) n'est jamais rendue : le premier UID est 1.
    ///
    /// # Errors
    ///
    /// `limit.exceeded { what: "resource_uid" }` au-delà de `u32::MAX`.
    pub fn alloc_resource(&mut self) -> Result<ResourceUid, CommandError> {
        next(&mut self.last_resource, "resource_uid").map(ResourceUid::new)
    }

    /// UID d'affectation suivant (R-196).
    ///
    /// # Errors
    ///
    /// `limit.exceeded { what: "assignment_uid" }` au-delà de `u32::MAX`.
    pub fn alloc_assignment(&mut self) -> Result<AssignmentUid, CommandError> {
        next(&mut self.last_assignment, "assignment_uid").map(AssignmentUid::new)
    }

    /// UID de calendrier suivant (calendrier propre d'une ressource, R-195).
    ///
    /// # Errors
    ///
    /// `limit.exceeded { what: "calendar_uid" }` au-delà de `u32::MAX`.
    pub fn alloc_calendar(&mut self) -> Result<CalendarUid, CommandError> {
        next(&mut self.last_calendar, "calendar_uid").map(CalendarUid::new)
    }

    /// Prend en compte les UID présents dans `p` (`last = max(last, max
    /// présent)`, pour chaque compteur) : un UID posé hors de l'allocateur
    /// n'est jamais rendu.
    pub fn observe(&mut self, p: &Project) {
        let [task, resource, assignment, calendar] = max_uids(p);
        self.last = self.last.max(task);
        self.last_resource = self.last_resource.max(resource);
        self.last_assignment = self.last_assignment.max(assignment);
        self.last_calendar = self.last_calendar.max(calendar);
    }
}

impl UidAlloc {
    /// Compteurs (tâche, ressource, affectation, calendrier) : journal de
    /// reprise (J7, lot L15), pour qu'un rejeu alloue les mêmes UID.
    pub const fn counters(&self) -> [u32; 4] {
        [
            self.last,
            self.last_resource,
            self.last_assignment,
            self.last_calendar,
        ]
    }

    /// Relève chaque compteur à au moins `c` (jamais de recul, R-151).
    pub fn raise(&mut self, c: [u32; 4]) {
        let [task, resource, assignment, calendar] = c;
        self.last = self.last.max(task);
        self.last_resource = self.last_resource.max(resource);
        self.last_assignment = self.last_assignment.max(assignment);
        self.last_calendar = self.last_calendar.max(calendar);
    }
}

/// Plus grand UID de tâche présent, 0 si aucune tâche.
fn max_task_uid(p: &Project) -> u32 {
    p.tasks.max_uid().map_or(0, TaskUid::get)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use plannercraft_core::{Moment, Project, ProjectInit, ScheduleAnchor, Task, TaskUid};

    use super::UidAlloc;
    use crate::error::codes;

    fn project(uids: &[u32]) -> Project {
        let start: Moment = "2026-11-02T08:00".parse().unwrap();
        let mut p = Project::new(ProjectInit {
            name: "P".to_owned(),
            anchor: ScheduleAnchor::FromStart(start),
            current_date: start,
            calendar_names: plannercraft_core::BuiltinCalendarNames::default(),
        });
        for &u in uids {
            let t = Task::new(TaskUid::new(u), format!("T{u}"), &p.options);
            p.tasks.push(t).unwrap();
        }
        p
    }

    #[test]
    fn r151_alloc_after_max_present() {
        let mut a = UidAlloc::from_project(&project(&[1, 2, 7]));
        assert_eq!(a.alloc().unwrap(), TaskUid::new(8));
        assert_eq!(a.alloc().unwrap(), TaskUid::new(9));
    }

    #[test]
    fn r151_empty_project_starts_at_one() {
        let mut a = UidAlloc::from_project(&project(&[]));
        assert_eq!(a.alloc().unwrap(), TaskUid::new(1));
    }

    #[test]
    fn r151_observe_never_goes_back() {
        let mut a = UidAlloc::from_project(&project(&[3]));
        assert_eq!(a.alloc().unwrap(), TaskUid::new(4));
        a.observe(&project(&[1]));
        assert_eq!(a.alloc().unwrap(), TaskUid::new(5));
        a.observe(&project(&[40]));
        assert_eq!(a.alloc().unwrap(), TaskUid::new(41));
    }

    #[test]
    fn r195_resource_assignment_calendar_counters() {
        // Projet neuf : calendriers 1 à 3 fournis (R-032), aucune ressource
        // ni affectation.
        let mut a = UidAlloc::from_project(&project(&[1]));
        assert_eq!(a.alloc_resource().unwrap().get(), 1);
        assert_eq!(a.alloc_resource().unwrap().get(), 2);
        assert_eq!(a.alloc_assignment().unwrap().get(), 1);
        assert_eq!(a.alloc_calendar().unwrap().get(), 4);
        // Les compteurs sont indépendants de celui des tâches.
        assert_eq!(a.alloc().unwrap(), TaskUid::new(2));
        a.last_resource = u32::MAX;
        assert_eq!(a.alloc_resource().unwrap_err().code, codes::LIMIT_EXCEEDED);
        assert_eq!(a.last_resource, u32::MAX);
    }

    #[test]
    fn r151_overflow() {
        let mut a = UidAlloc::from_project(&project(&[u32::MAX]));
        assert_eq!(a.alloc().unwrap_err().code, codes::LIMIT_EXCEEDED);
        // Refus sans effet : le compteur ne bouge pas.
        assert_eq!(a.last, u32::MAX);
    }
}
