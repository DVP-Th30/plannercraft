//! Échelle de mesure des lignes de base (R-222, R-224, R-225) : minutes
//! ouvrées signées sur le calendrier qui mesure la tâche dans le moteur
//! (`measure_calendar_in`, R-113), minutes civiles pour une feuille à durée
//! écoulée (R-067).

use crate::arena::Arena;
use crate::calendar::{CalendarSet, CompiledCalendar, standard};
use crate::duration::{Minutes, TimeBasis};
use crate::ids::{CalendarUid, TaskUid};
use crate::model::{Project, Task};
use crate::schedule::{TaskRole, measure_calendar_in};
use crate::time::Moment;

/// Échelles de mesure d'un projet, calendriers déjà compilés.
pub(crate) struct Measures<'a> {
    /// Projet.
    project: &'a Project,
    /// Calendriers compilés du projet.
    set: &'a CalendarSet,
    /// Calendrier de mesure de l'UID 0 (calendrier du projet ou sa base).
    project_measure: Option<CalendarUid>,
    /// Calendrier Standard intégré, compilé seul : repli du moteur quand le
    /// calendrier du projet est inutilisable (R-066).
    fallback: Option<CompiledCalendar>,
}

impl<'a> Measures<'a> {
    /// Échelles de `project` sur ses calendriers compilés `set`.
    pub(crate) fn new(project: &'a Project, set: &'a CalendarSet) -> Self {
        // L'UID 0 ne lit pas son calendrier propre (R-068) : une tâche neuve
        // d'UID 0 donne la même mesure que la récapitulative du projet.
        let root = Task::new(TaskUid::PROJECT_SUMMARY, String::new(), &project.options);
        let project_measure = measure_calendar_in(project, set, &root);
        let fallback = if project_measure.is_none() {
            Arena::try_from(vec![standard(project.calendar, "Standard".to_owned())])
                .ok()
                .and_then(|arena| CompiledCalendar::compile(&arena, project.calendar).ok())
        } else {
            None
        };
        Self {
            project,
            set,
            project_measure,
            fallback,
        }
    }

    /// Calendrier compilé de mesure d'une tâche (`None` pour l'UID 0 ou une
    /// tâche absente du modèle : celui de la récapitulative du projet).
    fn calendar(&self, task: Option<&Task>) -> Option<&CompiledCalendar> {
        let uid = match task {
            Some(t) if t.uid() != TaskUid::PROJECT_SUMMARY => measure_calendar_in(self.project, self.set, t),
            _ => self.project_measure,
        };
        match uid {
            Some(uid) => self.set.get(uid).ok(),
            None => self.fallback.as_ref(),
        }
    }

    /// Minutes signées de `a` à `b` sur l'échelle de mesure de `task` (rôle
    /// `role` dans le planning) : civiles pour une feuille à durée écoulée,
    /// ouvrées sinon (R-020, antisymétrique). Calendrier introuvable ou
    /// conversion impossible : minutes civiles (jamais de panique).
    pub(crate) fn between(&self, task: Option<&Task>, role: TaskRole, a: Moment, b: Moment) -> Minutes {
        let elapsed =
            role == TaskRole::Leaf && task.is_some_and(|t| t.duration.basis() == TimeBasis::Elapsed);
        if !elapsed
            && let Some(cal) = self.calendar(task)
            && let Ok(m) = cal.working_between(a, b)
        {
            return m;
        }
        a.minutes_until(b)
    }
}
