//! Exécution des commandes du projet (informations, ancre, date d'état).
//!
//! Aucune n'est une règle de calcul : elles posent des entrées du moteur
//! (sens et date pilote, date d'état), que le recalcul du lot exploite.

use plannercraft_core::{CalendarUid, ScheduleAnchor, Subject, TaskType};

use super::exec::Ctx;
use super::{AnchorSpec, Patch, ProjectInfoPatch, SetStatusDate};
use crate::error::{CommandError, CommandErrorKind, codes};

/// Applique un `Patch<String>` : `Clear` vide le champ.
fn patch_string(field: &mut String, patch: &Patch<String>) {
    match patch {
        Patch::Keep => {}
        Patch::Set(v) => field.clone_from(v),
        Patch::Clear => field.clear(),
    }
}

/// `param.invalid` sur `field`.
fn invalid(field: &str, why: &str) -> CommandError {
    CommandError::new(
        codes::PARAM_INVALID,
        CommandErrorKind::InvalidParam(why.to_owned()),
    )
    .with_field(field)
}

/// `project.set_info` : champs descriptifs ; absent gardé, `null` vidé.
/// Type et pilotage par défaut des nouvelles tâches (R-197, S2
/// `DefaultTaskType`, `NewTasksEffortDriven`) : seules les tâches créées
/// ensuite les prennent (R-152) ; `null` refusé.
pub(super) fn set_info(op: &ProjectInfoPatch, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    // Calendrier du projet (R-249) : calendrier existant, de base ou
    // dérivé ; `null` refusé. Le recalcul du lot fait le reste (R-113).
    let calendar = match op.calendar {
        Patch::Keep => None,
        Patch::Clear => {
            return Err(invalid(
                "calendar",
                "UID de calendrier attendu (il ne s'efface pas)",
            ));
        }
        Patch::Set(c) => {
            let c = CalendarUid::new(c);
            if !ctx.project.calendars.contains(c) {
                return Err(CommandError::new(
                    codes::CALENDAR_NOT_FOUND,
                    CommandErrorKind::InvalidParam(format!("aucun calendrier d'UID {c}")),
                )
                .with_field("calendar")
                .with_subject(Subject::Calendar(c)));
            }
            Some(c)
        }
    };
    let default_task_type = match op.default_task_type {
        Patch::Keep => None,
        Patch::Set(t) => Some(TaskType::from(t)),
        Patch::Clear => return Err(invalid("default_task_type", "type attendu (il ne s'efface pas)")),
    };
    let effort = match op.new_tasks_effort_driven {
        Patch::Keep => None,
        Patch::Set(e) => Some(e),
        Patch::Clear => return Err(invalid("new_tasks_effort_driven", "booléen attendu")),
    };
    if let Some(t) = default_task_type {
        ctx.project.options.default_task_type = t;
    }
    if let Some(e) = effort {
        ctx.project.options.new_tasks_effort_driven = e;
    }
    if let Some(c) = calendar
        && c != ctx.project.calendar
    {
        let previous = ctx.project.calendar;
        ctx.project.calendar = c;
        // R-161 : les affectations planifiées par l'ancien ou le nouveau
        // calendrier du projet perdent leur répartition.
        ctx.drop_calendar_assignment_extras(previous);
        ctx.drop_calendar_assignment_extras(c);
    }
    let info = &mut ctx.project.info;
    patch_string(&mut info.name, &op.name);
    patch_string(&mut info.title, &op.title);
    patch_string(&mut info.subject, &op.subject);
    patch_string(&mut info.author, &op.author);
    patch_string(&mut info.manager, &op.manager);
    patch_string(&mut info.company, &op.company);
    patch_string(&mut info.category, &op.category);
    ctx.touched.insert(Subject::Project, ctx.op_index);
    Ok(())
}

/// `project.set_anchor` : sens de planification et date pilote. Les
/// contraintes des tâches existantes ne changent pas (ADR-0012, lot L5) :
/// seules les tâches ajoutées ensuite prennent la contrainte par défaut du
/// nouveau sens (R-108, R-152).
#[allow(
    clippy::unnecessary_wraps,
    reason = "signature commune des commandes, appelée par la répartition `Exec` (exec.rs)"
)]
pub(super) fn set_anchor(op: &AnchorSpec, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    ctx.project.anchor = match *op {
        AnchorSpec::Start(m) => ScheduleAnchor::FromStart(m),
        AnchorSpec::Finish(m) => ScheduleAnchor::FromFinish(m),
    };
    ctx.touched.insert(Subject::Project, ctx.op_index);
    Ok(())
}

/// `project.set_status_date` : pose (`Set`), retire (`Clear` : « NA », la
/// date du jour s'applique, R-034) ou garde la date d'état.
#[allow(
    clippy::unnecessary_wraps,
    reason = "signature commune des commandes, appelée par la répartition `Exec` (exec.rs)"
)]
pub(super) fn set_status_date(op: &SetStatusDate, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    match op.date {
        Patch::Keep => {}
        Patch::Set(d) => ctx.project.status_date = Some(d),
        Patch::Clear => ctx.project.status_date = None,
    }
    ctx.touched.insert(Subject::Project, ctx.op_index);
    Ok(())
}
