//! Modèle V1 (ADR-0008, § 5) : projet, tâches, liens, ressources,
//! affectations, lignes de base, données hors modèle.
//!
//! Le modèle suit la **sémantique** de MSPDI, pas son encodage : aucun code
//! numérique MSPDI ici (correspondances dans `plannercraft-mspdi`, J3). Les valeurs
//! calculées restent hors du modèle (`Schedule`, J2).
//!
//! Sérialisation JSON : un miroir à plat (`ProjectData`) avec
//! `format_version` (1) et `deny_unknown_fields`. La lecture refuse une
//! valeur hors des bornes d'un type, un UID en double, l'UID 0 dans `tasks`,
//! une tâche récapitulative d'UID non nul et une version inconnue ; tout le
//! reste (références, liens, calendriers) est rapporté par
//! `Project::validate` : un fichier imparfait s'ouvre et s'explique.

mod assignment;
mod baseline;
pub mod custom;
mod extras;
mod leveling;
mod link;
mod options;
mod resource;
mod task;
mod validate;

use std::collections::BTreeMap;

use serde::de::{Deserializer, Error as _};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

pub use assignment::{Assignment, WorkContour};
pub use baseline::{AssignmentBaseline, ResourceBaseline, TaskBaseline};
pub use custom::{CustomFieldDef, CustomFieldId, CustomKind, CustomValue, CustomValueError};
pub use extras::{ExtraEntry, Extras};
pub use leveling::LevelingDelay;
pub use link::{Link, LinkType};
pub use options::{ProgressOption, ProgressOptions, ProjectOptions};
pub use resource::{Resource, ResourceKind, resource_id};
pub use task::{
    Constraint, ConstraintKind, OutlineLevel, Priority, Progress, SchedulingMode, Task, TaskType,
};

use crate::arena::Arena;
use crate::calendar::builtin::{self, BuiltinCalendarNames};
use crate::calendar::model::Calendar;
use crate::error::ModelError;
use crate::ids::{AssignmentUid, CalendarUid, Guid, ResourceUid, TaskId, TaskUid};
use crate::time::Moment;

/// Version du format JSON du projet.
pub const FORMAT_VERSION: u32 = 1;

/// Sens de planification et date pilote (S2). L'autre borne du projet est une
/// sortie du calcul (J2, J3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleAnchor {
    /// Planification depuis la date de début du projet.
    FromStart(Moment),
    /// Planification à rebours depuis la date de fin du projet.
    FromFinish(Moment),
}

/// Informations descriptives du projet.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectInfo {
    /// GUID conservé tel quel.
    pub guid: Option<Guid>,
    /// Nom.
    pub name: String,
    /// Titre.
    pub title: String,
    /// Objet.
    pub subject: String,
    /// Auteur.
    pub author: String,
    /// Responsable.
    pub manager: String,
    /// Société.
    pub company: String,
    /// Catégorie.
    pub category: String,
    /// Date de création.
    pub creation_date: Option<Moment>,
}

/// Paramètres d'un projet neuf.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectInit {
    /// Nom.
    pub name: String,
    /// Sens de planification et date pilote.
    pub anchor: ScheduleAnchor,
    /// « Maintenant », injecté (ADR-0005).
    pub current_date: Moment,
    /// Noms des calendriers fournis (français par défaut).
    pub calendar_names: BuiltinCalendarNames,
}

/// Projet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    /// Informations descriptives.
    pub info: ProjectInfo,
    /// Sens de planification et date pilote.
    pub anchor: ScheduleAnchor,
    /// Date d'état ; `None` = NA (R-034).
    pub status_date: Option<Moment>,
    /// « Maintenant », injecté (ADR-0005) ; `CurrentDate` MSPDI.
    pub current_date: Moment,
    /// Calendrier du projet.
    pub calendar: CalendarUid,
    /// Options.
    pub options: ProjectOptions,
    /// Calendriers.
    pub calendars: Arena<CalendarUid, Calendar>,
    /// Tâche UID 0 / ID 0 de MSPDI, rangée à part.
    pub summary_task: Option<Task>,
    /// Tâches dans l'ordre des ID (ID = position + 1), lignes vides comprises.
    pub tasks: Arena<TaskUid, Task>,
    /// Ressources.
    pub resources: Arena<ResourceUid, Resource>,
    /// Affectations.
    pub assignments: Arena<AssignmentUid, Assignment>,
    /// Définitions des champs personnalisés (R-263) : alias des champs
    /// renommés ; un champ sans entrée garde son nom canonique.
    pub custom_fields: BTreeMap<CustomFieldId, CustomFieldDef>,
    /// Données hors modèle.
    pub extras: Extras,
}

/// UID des calendriers fournis dans un projet neuf (hypothèse).
const STANDARD_UID: CalendarUid = CalendarUid::new(1);
const TWENTY_FOUR_HOURS_UID: CalendarUid = CalendarUid::new(2);
const NIGHT_SHIFT_UID: CalendarUid = CalendarUid::new(3);

impl Project {
    /// Projet neuf (R-032) : options par défaut, calendriers Standard (UID 1,
    /// calendrier du projet), 24 heures (UID 2) et Équipe de nuit (UID 3)
    /// (hypothèse : MS Project les propose tous les trois), aucune tâche.
    /// La date de création est la date courante injectée.
    #[must_use]
    pub fn new(init: ProjectInit) -> Self {
        let ProjectInit {
            name,
            anchor,
            current_date,
            calendar_names,
        } = init;
        let mut calendars = Arena::new();
        for calendar in [
            builtin::standard(STANDARD_UID, calendar_names.standard),
            builtin::twenty_four_hours(TWENTY_FOUR_HOURS_UID, calendar_names.twenty_four_hours),
            builtin::night_shift(NIGHT_SHIFT_UID, calendar_names.night_shift),
        ] {
            // UID distincts : l'ajout ne peut pas échouer (vérifié par R-032).
            let _ = calendars.push(calendar);
        }
        Self {
            info: ProjectInfo {
                name,
                creation_date: Some(current_date),
                ..ProjectInfo::default()
            },
            anchor,
            status_date: None,
            current_date,
            calendar: STANDARD_UID,
            options: ProjectOptions::default(),
            calendars,
            summary_task: None,
            tasks: Arena::new(),
            resources: Arena::new(),
            assignments: Arena::new(),
            custom_fields: BTreeMap::new(),
            extras: Extras::new(),
        }
    }

    /// R-034 : date d'état effective ; sans date d'état (NA), la date
    /// courante injectée.
    #[must_use]
    pub fn effective_status_date(&self) -> Moment {
        self.status_date.unwrap_or(self.current_date)
    }

    /// ID (position + 1) de la tâche d'UID `uid`.
    #[must_use]
    pub fn task_id(&self, uid: TaskUid) -> Option<TaskId> {
        let position = self.tasks.position(uid)?;
        let id = u32::try_from(position).ok()?.checked_add(1)?;
        Some(TaskId::new(id))
    }

    /// Tâche d'ID `id` (ID 0 : aucune, la récapitulative du projet est à part).
    #[must_use]
    pub fn task_by_id(&self, id: TaskId) -> Option<&Task> {
        let position = usize::try_from(id.get().checked_sub(1)?).ok()?;
        self.tasks.at(position)
    }
}

/// Miroir sérialisé du projet (lecture).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectData {
    format_version: u32,
    info: ProjectInfo,
    anchor: ScheduleAnchor,
    status_date: Option<Moment>,
    current_date: Moment,
    calendar: CalendarUid,
    options: ProjectOptions,
    calendars: Arena<CalendarUid, Calendar>,
    summary_task: Option<Task>,
    tasks: Arena<TaskUid, Task>,
    resources: Arena<ResourceUid, Resource>,
    assignments: Arena<AssignmentUid, Assignment>,
    /// Absent d'un projet enregistré avant L8 (R-263) : aucun.
    #[serde(default)]
    custom_fields: BTreeMap<CustomFieldId, CustomFieldDef>,
    extras: Extras,
}

/// Miroir sérialisé du projet (écriture, sans copie). Mêmes champs, même
/// ordre que [`ProjectData`].
#[derive(Serialize)]
struct ProjectDataRef<'a> {
    format_version: u32,
    info: &'a ProjectInfo,
    anchor: &'a ScheduleAnchor,
    status_date: &'a Option<Moment>,
    current_date: &'a Moment,
    calendar: &'a CalendarUid,
    options: &'a ProjectOptions,
    calendars: &'a Arena<CalendarUid, Calendar>,
    summary_task: &'a Option<Task>,
    tasks: &'a Arena<TaskUid, Task>,
    resources: &'a Arena<ResourceUid, Resource>,
    assignments: &'a Arena<AssignmentUid, Assignment>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    custom_fields: &'a BTreeMap<CustomFieldId, CustomFieldDef>,
    extras: &'a Extras,
}

impl TryFrom<ProjectData> for Project {
    type Error = ModelError;
    fn try_from(d: ProjectData) -> Result<Self, Self::Error> {
        if d.format_version != FORMAT_VERSION {
            return Err(ModelError::UnsupportedFormatVersion {
                version: d.format_version,
            });
        }
        if d.tasks.contains(TaskUid::PROJECT_SUMMARY) {
            return Err(ModelError::ReservedUid {
                uid: TaskUid::PROJECT_SUMMARY.get(),
            });
        }
        if let Some(summary) = &d.summary_task
            && summary.uid() != TaskUid::PROJECT_SUMMARY
        {
            return Err(ModelError::SummaryTaskUid {
                uid: summary.uid().get(),
            });
        }
        Ok(Self {
            info: d.info,
            anchor: d.anchor,
            status_date: d.status_date,
            current_date: d.current_date,
            calendar: d.calendar,
            options: d.options,
            calendars: d.calendars,
            summary_task: d.summary_task,
            tasks: d.tasks,
            resources: d.resources,
            assignments: d.assignments,
            custom_fields: d.custom_fields,
            extras: d.extras,
        })
    }
}

impl Serialize for Project {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ProjectDataRef {
            format_version: FORMAT_VERSION,
            info: &self.info,
            anchor: &self.anchor,
            status_date: &self.status_date,
            current_date: &self.current_date,
            calendar: &self.calendar,
            options: &self.options,
            calendars: &self.calendars,
            summary_task: &self.summary_task,
            tasks: &self.tasks,
            resources: &self.resources,
            assignments: &self.assignments,
            custom_fields: &self.custom_fields,
            extras: &self.extras,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Project {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let data = ProjectData::deserialize(deserializer)?;
        Self::try_from(data).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{Project, ProjectInit, ScheduleAnchor, Task};
    use crate::calendar::builtin::BuiltinCalendarNames;
    use crate::ids::{TaskId, TaskUid};
    use crate::time::Moment;

    fn m(s: &str) -> Moment {
        s.parse().unwrap()
    }

    fn project() -> Project {
        Project::new(ProjectInit {
            name: "P".to_owned(),
            anchor: ScheduleAnchor::FromStart(m("2026-10-19T08:00")),
            current_date: m("2026-10-17T08:00"),
            calendar_names: BuiltinCalendarNames::default(),
        })
    }

    #[test]
    fn task_ids_follow_positions() {
        let mut p = project();
        let o = p.options;
        p.tasks
            .push(Task::new(TaskUid::new(10), "a".to_owned(), &o))
            .unwrap();
        p.tasks
            .push(Task::new(TaskUid::new(4), "b".to_owned(), &o))
            .unwrap();
        assert_eq!(p.task_id(TaskUid::new(10)), Some(TaskId::new(1)));
        assert_eq!(p.task_id(TaskUid::new(4)), Some(TaskId::new(2)));
        assert_eq!(p.task_id(TaskUid::new(5)), None);
        assert_eq!(p.task_by_id(TaskId::new(2)).unwrap().name, "b");
        assert!(p.task_by_id(TaskId::new(0)).is_none());
        assert!(p.task_by_id(TaskId::new(3)).is_none());
    }

    #[test]
    fn deserialization_refusals() {
        let mut p = project();
        let o = p.options;
        p.tasks
            .push(Task::new(TaskUid::new(1), "a".to_owned(), &o))
            .unwrap();
        let good = serde_json::to_value(&p).unwrap();
        assert_eq!(serde_json::from_value::<Project>(good.clone()).unwrap(), p);

        // UID 0 dans `tasks`.
        let mut bad = good.clone();
        bad["tasks"][0]["uid"] = 0.into();
        let err = serde_json::from_value::<Project>(bad).unwrap_err().to_string();
        assert!(err.contains("réservé"), "{err}");

        // UID en double.
        let mut bad = good.clone();
        let task = bad["tasks"][0].clone();
        bad["tasks"].as_array_mut().unwrap().push(task);
        let err = serde_json::from_value::<Project>(bad).unwrap_err().to_string();
        assert!(err.contains("en double"), "{err}");

        // Version de format inconnue.
        let mut bad = good.clone();
        bad["format_version"] = 2.into();
        let err = serde_json::from_value::<Project>(bad).unwrap_err().to_string();
        assert!(err.contains("version de format 2"), "{err}");

        // Champ inconnu.
        let mut bad = good.clone();
        bad["finish_date"] = "2026-12-31T17:00".into();
        assert!(serde_json::from_value::<Project>(bad).is_err());

        // Tâche récapitulative d'UID non nul.
        let mut bad = good.clone();
        bad["summary_task"] = bad["tasks"][0].clone();
        let err = serde_json::from_value::<Project>(bad).unwrap_err().to_string();
        assert!(err.contains("UID 0"), "{err}");

        // Valeur hors bornes d'un type (priorité 1 001).
        let mut bad = good;
        bad["tasks"][0]["priority"] = 1_001.into();
        assert!(serde_json::from_value::<Project>(bad).is_err());
    }
}
