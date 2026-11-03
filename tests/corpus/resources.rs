//! Comparaison des valeurs de ressources du corpus (J5, lot L9), à compteur
//! séparé des cas de tâches ([`super::compare`]) : plancher
//! `plannercraft_core::parity::CORPUS_RESOURCE_FLOOR`, hors `FLOOR`.
//!
//! - **Cas de tâche** : une tâche automatique, active, planifiée (feuille ou
//!   récapitulative, UID 0 exclu) qui porte du travail enregistré ou une
//!   affectation à une vraie ressource. Champs : `Work` de la tâche (lu dans
//!   le modèle, `Task::work`, contre `TaskSchedule::work`, R-184) ; `Start`
//!   et `Finish` de chacune de ses affectations hors affectation fictive
//!   −65 535 (`RecordedAssignment` contre `AssignmentSchedule::span`, R-173
//!   pour une feuille, R-179 pour une récapitulative, depuis le
//!   2026-10-19).
//! - **Cas de ressource** : une ressource du fichier (UID 0 exclu, O34) dont
//!   le fichier enregistre `Work` ou `OverAllocated`. Champs : `Work`
//!   (R-184) et `OverAllocated` (R-186, R-187).
//! - **Identique** : tous les champs comparables égaux, sans tolérance.
//!
//! `OverAllocated` de A7 (`projinteg.xml`) est indicatif (O31 : il vaut 1
//! alors que `PeakUnits` vaut 1.00 ; seuil de MS Project non établi, § 3
//! point 5 du cahier J5) : compté, mais jamais une vérification de R-186.

use std::collections::BTreeMap;

use plannercraft_core::{Assignment, Schedule, SchedulingMode, TaskOutcome, TaskRole, TaskUid};
use plannercraft_mspdi::MspdiDocument;
use serde::Serialize;

/// Champ comparé.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceField {
    /// `Work` d'une tâche.
    TaskWork,
    /// `Start` d'une affectation.
    AssignmentStart,
    /// `Finish` d'une affectation.
    AssignmentFinish,
    /// `Work` d'une ressource.
    ResourceWork,
    /// `OverAllocated` d'une ressource.
    Overallocated,
}

impl ResourceField {
    /// Nom stable.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TaskWork => "task_work",
            Self::AssignmentStart => "assignment_start",
            Self::AssignmentFinish => "assignment_finish",
            Self::ResourceWork => "resource_work",
            Self::Overallocated => "overallocated",
        }
    }
}

/// Nature d'un cas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceCaseKind {
    /// Feuille.
    Leaf,
    /// Récapitulative.
    Summary,
    /// Ressource.
    Resource,
}

/// Divergence sur un champ (UID de l'affectation pour ses dates).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ResourceDivergence {
    /// Champ.
    pub field: ResourceField,
    /// Affectation concernée (dates d'affectation seulement).
    pub assignment: Option<u32>,
    /// Valeur enregistrée par MS Project.
    pub recorded: String,
    /// Valeur de PlannerCraft.
    pub computed: String,
}

/// Un cas comparé (tâche ou ressource).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ResourceCase {
    /// Nature.
    pub kind: ResourceCaseKind,
    /// UID de la tâche ou de la ressource.
    pub uid: u32,
    /// Nombre de valeurs comparées.
    pub compared: u32,
    /// Divergences (vide si identique).
    pub divergences: Vec<ResourceDivergence>,
}

impl ResourceCase {
    /// Tous les champs comparables égaux.
    #[must_use]
    pub fn identical(&self) -> bool {
        self.divergences.is_empty()
    }
}

/// Comparaison des ressources d'un fichier.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ResourceComparison {
    /// Cas de tâche (ordre des UID), puis cas de ressource (ordre des UID).
    pub cases: Vec<ResourceCase>,
    /// Affectations fictives −65 535 du fichier (jamais comparées).
    pub placeholders: u32,
    /// Tâches écartées, par raison (`manual`, `inactive`, `not_scheduled`).
    pub excluded: BTreeMap<&'static str, u32>,
}

impl ResourceComparison {
    /// Nombre de cas.
    #[must_use]
    pub fn case_count(&self) -> u32 {
        count(self.cases.len())
    }

    /// Nombre de cas identiques.
    #[must_use]
    pub fn identical_count(&self) -> u32 {
        count(self.cases.iter().filter(|c| c.identical()).count())
    }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn bump(map: &mut BTreeMap<&'static str, u32>, key: &'static str) {
    let n = map.entry(key).or_insert(0);
    *n = n.saturating_add(1);
}

/// Compare les valeurs de ressources d'un document lu au planning recalculé.
#[must_use]
pub fn compare(doc: &MspdiDocument, schedule: &Schedule) -> ResourceComparison {
    let project = &doc.project;
    let mut out = ResourceComparison {
        placeholders: count(
            project
                .assignments
                .iter()
                .filter(|a| a.resource.is_none())
                .count(),
        ),
        ..ResourceComparison::default()
    };
    task_cases(doc, schedule, &mut out);
    resource_cases(doc, schedule, &mut out);
    out
}

/// Cas de tâche (ordre du modèle).
fn task_cases(doc: &MspdiDocument, schedule: &Schedule, out: &mut ResourceComparison) {
    let project = &doc.project;
    for task in &project.tasks {
        let uid = task.uid();
        if task.is_null || uid == TaskUid::PROJECT_SUMMARY {
            continue;
        }
        let assignments: Vec<_> = project.assignments.iter().filter(|a| a.task == uid).collect();
        let real = assignments.iter().filter(|a| a.resource.is_some()).count();
        if task.work.minutes().get() == 0 && real == 0 {
            continue;
        }
        if matches!(task.mode, SchedulingMode::Manual { .. }) {
            bump(&mut out.excluded, "manual");
            continue;
        }
        if !task.active {
            bump(&mut out.excluded, "inactive");
            continue;
        }
        let Some(TaskOutcome::Scheduled(ts)) = schedule.get(uid) else {
            bump(&mut out.excluded, "not_scheduled");
            continue;
        };
        let kind = match ts.role {
            TaskRole::Leaf => ResourceCaseKind::Leaf,
            TaskRole::Summary => ResourceCaseKind::Summary,
        };
        let mut case = ResourceCase {
            kind,
            uid: uid.get(),
            compared: 1,
            divergences: Vec::new(),
        };
        if task.work != ts.work {
            case.divergences.push(ResourceDivergence {
                field: ResourceField::TaskWork,
                assignment: None,
                recorded: format!("{} min", task.work.minutes().get()),
                computed: format!("{} min", ts.work.minutes().get()),
            });
        }
        // Feuilles (R-173) et récapitulatives (R-179) : dates de chaque
        // affectation.
        assignment_dates(doc, schedule, &assignments, &mut case);
        out.cases.push(case);
    }
}

/// Dates des affectations d'une tâche (feuille ou récapitulative), hors
/// fictive −65 535.
fn assignment_dates(
    doc: &MspdiDocument,
    schedule: &Schedule,
    assignments: &[&Assignment],
    case: &mut ResourceCase,
) {
    for a in assignments {
        if a.resource.is_none() {
            continue;
        }
        let Some(rec) = doc.recorded.assignments.get(&a.uid()) else {
            continue;
        };
        let placed = schedule.assignments.get(&a.uid()).map(|s| s.span);
        let dates = [
            (ResourceField::AssignmentStart, rec.start, placed.map(|s| s.start)),
            (
                ResourceField::AssignmentFinish,
                rec.finish,
                placed.map(|s| s.finish),
            ),
        ];
        for (field, recorded, computed) in dates {
            let Some(recorded) = recorded else { continue };
            case.compared = case.compared.saturating_add(1);
            if computed != Some(recorded) {
                case.divergences.push(ResourceDivergence {
                    field,
                    assignment: Some(a.uid().get()),
                    recorded: recorded.to_string(),
                    computed: computed.map_or_else(|| "absent".to_owned(), |m| m.to_string()),
                });
            }
        }
    }
}

/// Cas de ressource (ordre des UID).
fn resource_cases(doc: &MspdiDocument, schedule: &Schedule, out: &mut ResourceComparison) {
    for (&uid, rec) in &doc.recorded.resources {
        if uid.get() == 0 || (rec.work.is_none() && rec.overallocated.is_none()) {
            continue;
        }
        let load = schedule.resources.get(&uid);
        let mut case = ResourceCase {
            kind: ResourceCaseKind::Resource,
            uid: uid.get(),
            compared: 0,
            divergences: Vec::new(),
        };
        if let Some(recorded) = rec.work {
            case.compared = case.compared.saturating_add(1);
            let computed = load.map(|l| l.work);
            if computed != Some(recorded) {
                case.divergences.push(ResourceDivergence {
                    field: ResourceField::ResourceWork,
                    assignment: None,
                    recorded: format!("{} min", recorded.minutes().get()),
                    computed: computed
                        .map_or_else(|| "absent".to_owned(), |w| format!("{} min", w.minutes().get())),
                });
            }
        }
        if let Some(recorded) = rec.overallocated {
            case.compared = case.compared.saturating_add(1);
            let computed = load.map(|l| l.overallocated);
            if computed != Some(recorded) {
                case.divergences.push(ResourceDivergence {
                    field: ResourceField::Overallocated,
                    assignment: None,
                    recorded: recorded.to_string(),
                    computed: computed.map_or_else(|| "absent".to_owned(), |b| b.to_string()),
                });
            }
        }
        out.cases.push(case);
    }
}

/// Planchers par fichier : nombre minimal de cas identiques (tâches et
/// ressources) ; relevés au mesuré, jamais baissés sans ADR. Leur somme
/// vaut `plannercraft_core::parity::CORPUS_RESOURCE_FLOOR` (testé).
pub const FLOORS: [(&str, u32); 5] = [
    // A1 et A2 : aucune ressource, seulement des affectations fictives (O24).
    ("cogito-advanced-sample-msp2010.xml", 0),
    ("practicemap.xml", 0),
    // A4 et A6 : tous les cas identiques (mesuré 44/44 et 35/35).
    ("shopmetrics.xml", 44),
    ("takeout-system-zh.xml", 35),
    // A7 : 7/7 depuis J6 lot L1 (affectations des feuilles achevées UID 2
    // et 3 placées depuis le début réel, R-209) ; `OverAllocated` indicatif
    // (O31).
    ("projinteg.xml", 7),
];
