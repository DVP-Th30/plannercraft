//! Élément `Project` : informations, ancre, options, calendrier, puis
//! calendriers, tâches, ressources, affectations et `Extras` du projet
//! (R-120, R-123, R-130, R-132).

use std::collections::BTreeSet;

use plannercraft_core::{ProgressOptions, Project, ScheduleAnchor};

use super::computed::Values;
use super::emit::Emitter;
use super::{
    Planned, WriteCode, WriteOptions, assignment, calendar, children, max_len, plan, resource, task, unscoped,
};
use crate::codes;
use crate::custom::{self, WritePlan};
use crate::extras::Anchor;
use crate::scalar::{format_bool, format_datetime, format_time};
use crate::schema::Parent;

/// Écrit les enfants de `Project`.
#[allow(
    clippy::too_many_lines,
    reason = "un bras par élément de `Project`, rien à factoriser"
)]
pub(crate) fn write(em: &mut Emitter, p: &Project, values: Option<&Values>, options: &WriteOptions) {
    // Les enfants de `Project` s'écrivent sous la racine déjà ouverte.
    let profile = options.profile;
    let o = &p.options;
    let text = |em: &mut Emitter, name: &'static str, value: &str| {
        em.text(name, value, max_len(Parent::Project, name));
    };
    let planned = plan(em, "", Parent::Project, &p.extras, &[], profile);
    // R-130 : `CurrencyCode` relu (conservé, R-087), sinon celui des options.
    let has_currency = planned
        .iter()
        .any(|x| x.key.name == "CurrencyCode" && x.key.anchor == Anchor::None);
    let custom_plan = custom_plan(em, p, &planned);
    let mut top = unscoped(&planned);
    if custom_plan.owns_container() {
        top.retain(|x| !is_container(x));
    }
    children(em, Parent::Project, profile, &top, |em, name| {
        if let Some(v) = progress_option(o.progress, name) {
            return em.leaf(name, format_bool(v));
        }
        match name {
            "SaveVersion" => em.leaf(name, &options.save_version.to_string()),
            "GUID" => em.opt_leaf(name, p.info.guid.map(|g| g.to_string())),
            "Name" => text(em, name, &p.info.name),
            "Title" => text(em, name, &p.info.title),
            "Subject" => text(em, name, &p.info.subject),
            "Category" => text(em, name, &p.info.category),
            "Company" => text(em, name, &p.info.company),
            "Manager" => text(em, name, &p.info.manager),
            "Author" => text(em, name, &p.info.author),
            "CreationDate" => em.opt_leaf(name, p.info.creation_date.map(format_datetime)),
            "ScheduleFromStart" => em.leaf(
                name,
                format_bool(matches!(p.anchor, ScheduleAnchor::FromStart(_))),
            ),
            // R-132 : l'ancre est une entrée ; l'autre borne est calculée
            // (R-122).
            "StartDate" => em.opt_leaf(
                name,
                match p.anchor {
                    ScheduleAnchor::FromStart(s) => Some(s),
                    ScheduleAnchor::FromFinish(_) => values.and_then(|v| v.project_start),
                }
                .map(format_datetime),
            ),
            "FinishDate" => em.opt_leaf(
                name,
                match p.anchor {
                    ScheduleAnchor::FromFinish(f) => Some(f),
                    ScheduleAnchor::FromStart(_) => values.and_then(|v| v.project_finish),
                }
                .map(format_datetime),
            ),
            "CriticalSlackLimit" => em.leaf(name, &o.critical_slack_limit_days.to_string()),
            "CurrencyCode" => {
                if !has_currency {
                    text(em, name, &options.currency_code);
                }
            }
            "CalendarUID" => em.leaf(name, &p.calendar.get().to_string()),
            "DefaultStartTime" => em.leaf(name, &format_time(o.default_start_time, false)),
            "DefaultFinishTime" => em.leaf(name, &format_time(o.default_finish_time, false)),
            "MinutesPerDay" => em.leaf(name, &o.factors.minutes_per_day().to_string()),
            "MinutesPerWeek" => em.leaf(name, &o.factors.minutes_per_week().to_string()),
            "DaysPerMonth" => em.leaf(name, &o.factors.days_per_month().to_string()),
            "DefaultTaskType" => em.leaf(name, &codes::task_type_code(o.default_task_type).to_string()),
            "DurationFormat" => em.leaf(
                name,
                &codes::duration_format_code(
                    o.default_duration_unit,
                    plannercraft_core::TimeBasis::Working,
                    false,
                )
                .to_string(),
            ),
            "WorkFormat" => em.leaf(name, &codes::work_format_code(o.default_work_unit).to_string()),
            "HonorConstraints" => em.leaf(name, format_bool(o.honor_constraints)),
            "MultipleCriticalPaths" => em.leaf(name, format_bool(o.multiple_critical_paths)),
            "NewTasksEffortDriven" => em.leaf(name, format_bool(o.new_tasks_effort_driven)),
            "NewTasksEstimated" => em.leaf(name, format_bool(o.new_tasks_estimated)),
            "WeekStartDay" => em.leaf(name, &codes::week_start_code(o.week_start_day).to_string()),
            "StatusDate" => em.opt_leaf(name, p.status_date.map(format_datetime)),
            "CurrentDate" => em.leaf(name, &format_datetime(p.current_date)),
            "Calendars" => {
                if !p.calendars.is_empty() {
                    em.open(name);
                    for c in &p.calendars {
                        calendar::write(em, c, profile);
                    }
                    em.close();
                }
            }
            "ExtendedAttributes" if custom_plan.owns_container() => {
                write_definitions(em, p, &custom_plan);
            }
            "Tasks" => {
                em.open(name);
                task::write_all(em, p, values, profile, &custom_plan.foreign);
                em.close();
            }
            "Resources" if !p.resources.is_empty() => {
                em.open(name);
                resource::write_all(em, p, values, profile);
                em.close();
            }
            "Assignments" if !p.assignments.is_empty() => {
                em.open(name);
                for a in &p.assignments {
                    assignment::write(em, a, values, profile);
                }
                em.close();
            }
            _ => {}
        }
    });
}

/// `Extras` du conteneur des définitions des champs personnalisés.
fn is_container(x: &Planned<'_>) -> bool {
    x.key.name == custom::CONTAINER && x.key.anchor == Anchor::None && x.key.scope.is_none()
}

/// R-266 : plan d'écriture des champs personnalisés ; une valeur ou un
/// alias du modèle sur un champ défini par le fichier (formule, table de
/// choix) n'est pas écrit, `mspdi.write.extra_invalid`.
fn custom_plan(em: &mut Emitter, p: &Project, planned: &[Planned<'_>]) -> WritePlan {
    let containers: Vec<&str> = planned
        .iter()
        .filter(|x| is_container(x))
        .map(|x| x.payload)
        .collect();
    let single = match containers.as_slice() {
        [one] => Some(*one),
        _ => None,
    };
    let plan = WritePlan::new(p, single, containers.len() > 1);
    for id in custom::written_fields(p, &BTreeSet::new()) {
        if plan.foreign.contains(&id) {
            em.report(
                WriteCode::ExtraInvalid,
                custom::CONTAINER,
                Some(format!(
                    "{id} (champ défini par le fichier : formule ou table de choix)"
                )),
            );
        }
    }
    plan
}

/// R-266 : conteneur des définitions écrit par le modèle : une définition
/// par champ qui a un alias ou une valeur (`FieldID`, `FieldName` canonique,
/// `Alias` s'il existe), dans l'ordre des `FieldID`, puis les définitions
/// gardées en `Extras` (formules, tables de choix, autres champs) telles
/// quelles.
fn write_definitions(em: &mut Emitter, p: &Project, plan: &WritePlan) {
    em.open(custom::CONTAINER);
    for id in &plan.written {
        em.open(custom::ATTRIBUTE);
        em.leaf_int("FieldID", custom::field_id(*id));
        em.leaf("FieldName", &custom::field_name(*id));
        if let Some(alias) = p
            .custom_fields
            .get(id)
            .and_then(|d| d.alias.as_deref())
            .filter(|a| !a.is_empty())
        {
            em.text("Alias", alias, Some(50));
        }
        em.close();
    }
    for payload in &plan.residue {
        em.raw(payload);
    }
    em.close();
}

/// R-218 : valeur de l'option d'avancement `name` (réécrite telle quelle),
/// `None` pour un autre élément.
const fn progress_option(o: ProgressOptions, name: &str) -> Option<bool> {
    Some(match name.as_bytes() {
        b"SplitsInProgressTasks" => o.split_in_progress,
        b"TaskUpdatesResource" => o.task_updates_resource,
        b"MoveCompletedEndsBack" => o.move_completed_ends_back,
        b"MoveRemainingStartsBack" => o.move_remaining_starts_back,
        b"MoveRemainingStartsForward" => o.move_remaining_starts_forward,
        b"MoveCompletedEndsForward" => o.move_completed_ends_forward,
        _ => return None,
    })
}
