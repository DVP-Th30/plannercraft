//! Correspondance typée de l'élément `Project` (§ 7.6) : informations,
//! ancre, options, calendrier du projet ; puis calendriers, tâches,
//! ressources et affectations.
//!
//! Valeurs par défaut : celles du schéma MSPDI pour un élément absent
//! (R-076), sinon celles du modèle (R-032).

use std::collections::{BTreeMap, BTreeSet};

use plannercraft_core::{
    Arena, BuiltinCalendarNames, CalendarUid, ConversionFactors, Diagnostic, ExtraEntry, Extras, Guid,
    ProgressOptions, Project, ProjectInfo, ProjectInit, ProjectOptions, ScheduleAnchor, SchedulingMode,
    Subject, TaskType, TaskUid,
};

use crate::ReadOptions;
use crate::assignment;
use crate::calendar;
use crate::codes::{self, DurationCode};
use crate::custom;
use crate::diag::{ImportCode, Sink};
use crate::extras::{self, ORIGIN};
use crate::fields::Fields;
use crate::recorded::RecordedSchedule;
use crate::resource;
use crate::schema::Parent;
use crate::task;
use crate::xml::RawDocument;

/// Résultat de la correspondance.
pub(crate) struct Built {
    pub(crate) project: Project,
    pub(crate) recorded: RecordedSchedule,
    pub(crate) save_version: Option<i64>,
    /// Calculés écartés après la lecture XML (répartitions de type 1 des
    /// affectations, R-087), à ajouter à ceux de la lecture.
    pub(crate) dropped_computed: BTreeMap<String, u64>,
}

/// UID du calendrier du projet quand `CalendarUID` manque et qu'aucun
/// calendrier n'est lu (hypothèse : celui du Standard d'un projet neuf).
const FALLBACK_CALENDAR: CalendarUid = CalendarUid::new(1);

/// Construit le projet et le planning enregistré.
pub(crate) fn build(raw: &RawDocument<'_>, options: &ReadOptions, sink: &mut Sink<'_>) -> Built {
    let f = Fields::new(raw.fields.iter(), "", Some(Subject::Project), raw.root_at);
    let save_version = f.int("SaveVersion", sink).map(|(v, _)| v);

    // R-089 : sens de planification (`ScheduleFromStart`, défaut vrai du
    // schéma, R-076). Depuis le début, l'ancre est `StartDate` ; à rebours,
    // `FinishDate` (« Required if ScheduleFromStart is false », S34), sinon
    // `StartDate`, sinon « maintenant », avec `mspdi.missing_required`.
    // `StartDate` est toujours gardée dans le planning enregistré (R-083).
    let from_start = f.bool("ScheduleFromStart", sink).unwrap_or(true);
    let recorded_start = f.datetime("StartDate", sink);
    let recorded_finish = f.datetime("FinishDate", sink);
    let start = recorded_start.unwrap_or(options.current_date);
    let current_date = f.datetime("CurrentDate", sink).unwrap_or(options.current_date);
    let anchor = if from_start {
        if !f.has("StartDate") {
            f.missing("StartDate", sink);
        }
        ScheduleAnchor::FromStart(start)
    } else {
        if !f.has("FinishDate") {
            f.missing("FinishDate", sink);
        }
        ScheduleAnchor::FromFinish(recorded_finish.unwrap_or(start))
    };

    let mut project = Project::new(ProjectInit {
        name: String::new(),
        anchor,
        current_date,
        calendar_names: BuiltinCalendarNames::default(),
    });
    // Les calendriers sont ceux du fichier, jamais ceux d'un projet neuf.
    project.calendars = Arena::new();
    project.info = info(&f, sink);
    project.status_date = f.datetime("StatusDate", sink);
    project.options = project_options(&f, sink);
    project.calendar = project_calendar(&f, sink);
    project.calendars = calendar::import(&raw.calendars, project.calendar, sink);

    let mut recorded = RecordedSchedule {
        project_start: recorded_start,
        project_finish: recorded_finish,
        ..RecordedSchedule::default()
    };

    // R-265 : champs personnalisés étrangers (définis par une formule, une
    // table de choix…) d'après le conteneur des définitions : leurs valeurs
    // restent en `Extras`.
    let containers: Vec<&str> = raw
        .fields
        .iter()
        .filter(|e| e.name == custom::CONTAINER)
        .filter_map(|e| e.raw)
        .collect();
    let foreign = custom::foreign_in_payloads(&containers);
    let mut replaced = task::Replaced::default();
    task::import(
        &raw.tasks,
        &mut project,
        &mut recorded,
        &mut replaced,
        &foreign,
        sink,
    );
    honor_constraints_off(&f, &project, sink);
    project.resources = resource::import(
        &raw.resources,
        &project.options.factors,
        &mut recorded.resources,
        sink,
    );
    let tasks: BTreeSet<TaskUid> = recorded.tasks.keys().copied().collect();
    let (assignments, dropped_timephased) = assignment::import(
        &raw.assignments,
        &tasks,
        &mut recorded.assignments,
        &mut replaced,
        sink,
    );
    replaced.report(sink);
    for (uid, n) in assignment::count_by_task(&assignments) {
        if let Some(rec) = recorded.tasks.get_mut(&uid) {
            rec.assignments = n;
        }
    }
    project.assignments = assignments;
    project.extras = project_extras(raw, &project);
    custom::lift_container(&mut project.extras, &mut project.custom_fields, &BTreeSet::new());
    let mut dropped_computed = BTreeMap::new();
    if dropped_timephased > 0 {
        dropped_computed.insert("TimephasedData".to_owned(), dropped_timephased);
    }

    Built {
        project,
        recorded,
        save_version,
        dropped_computed,
    }
}

/// R-089 : `HonorConstraints` est lu littéralement (S34 : `default="true"`,
/// « Whether tasks honour their constraint dates ») ; à 0 alors qu'une tâche
/// automatique active a une contrainte datée, l'option change le calcul de
/// cette tâche : `mspdi.honor_constraints_off` (info, une fois). Les cinq
/// fichiers du corpus ont 0 (O15) : un codage inversé reste l'hypothèse
/// concurrente prioritaire.
fn honor_constraints_off(f: &Fields<'_>, project: &Project, sink: &mut Sink<'_>) {
    if project.options.honor_constraints {
        return;
    }
    let dated = project
        .tasks
        .iter()
        .any(|t| !t.is_null && t.active && t.mode == SchedulingMode::Auto && t.constraint.kind().is_dated());
    if dated {
        let value = f.raw("HonorConstraints", sink);
        f.report(
            ImportCode::HonorConstraintsOff,
            "HonorConstraints",
            f.at(),
            value,
            sink,
        );
    }
}

/// `Extras` du projet (R-087) : déclarations `xmlns:préfixe` de la racine,
/// puis enfants capturés de `Project`, dans l'ordre canonique, puis le
/// style du document s'il a été lu (H-R12).
fn project_extras(raw: &RawDocument<'_>, project: &Project) -> Extras {
    let mut out: Vec<ExtraEntry> = raw
        .prefixes
        .iter()
        .map(|(prefix, uri)| ExtraEntry {
            origin: ORIGIN.to_owned(),
            key: format!("@xmlns:{prefix}"),
            payload: uri.clone(),
        })
        .collect();
    // Conteneurs non vides : réécrits, donc ancres (R-087).
    let written = |name: &str| match name {
        "Calendars" => !project.calendars.is_empty(),
        "Resources" => !project.resources.is_empty(),
        "Assignments" => !project.assignments.is_empty(),
        _ => false,
    };
    out.extend(extras::collect_with(&raw.fields, Parent::Project, None, written));
    let mut extras = Extras(out);
    extras::canonical_order(&mut extras, Parent::Project);
    // Style du document (H-R12) : entrée d'origine `plannercraft`, en fin de liste.
    crate::style_pi::set(&mut extras, raw.style.clone());
    extras
}

/// Informations descriptives.
fn info(f: &Fields<'_>, sink: &mut Sink<'_>) -> ProjectInfo {
    let text = |name: &str, sink: &mut Sink<'_>| f.raw(name, sink).unwrap_or_default().to_owned();
    ProjectInfo {
        guid: guid(f, "GUID", sink),
        name: text("Name", sink),
        title: text("Title", sink),
        subject: text("Subject", sink),
        author: text("Author", sink),
        manager: text("Manager", sink),
        company: text("Company", sink),
        category: text("Category", sink),
        creation_date: f.datetime("CreationDate", sink),
    }
}

/// GUID `8-4-4-4-12` ; illisible → diagnostic, aucun.
pub(crate) fn guid(f: &Fields<'_>, name: &str, sink: &mut Sink<'_>) -> Option<Guid> {
    let (t, at) = f.scalar(name, sink)?;
    let g = t.parse().ok();
    if g.is_none() {
        f.invalid(name, at, t, sink);
    }
    g
}

/// Entier borné ; absent → `default` ; hors bornes ou illisible →
/// diagnostic et `default`.
fn bounded_u32(
    f: &Fields<'_>,
    name: &str,
    range: std::ops::RangeInclusive<u32>,
    default: u32,
    sink: &mut Sink<'_>,
) -> u32 {
    match f.int(name, sink) {
        None => default,
        Some((v, at)) => match u32::try_from(v) {
            Ok(v) if range.contains(&v) => v,
            _ => {
                f.invalid(name, at, &v.to_string(), sink);
                default
            }
        },
    }
}

/// Code numérique traduit par `table` ; absent → `None` ; inconnu →
/// diagnostic `mspdi.unknown_code`, `None`.
pub(crate) fn code<T>(
    f: &Fields<'_>,
    name: &str,
    table: impl Fn(i64) -> Option<T>,
    sink: &mut Sink<'_>,
) -> Option<T> {
    let (v, at) = f.int(name, sink)?;
    let out = table(v);
    if out.is_none() {
        f.report(ImportCode::UnknownCode { code: v }, name, at, None, sink);
    }
    out
}

/// Options du projet (R-076 pour les absents).
fn project_options(f: &Fields<'_>, sink: &mut Sink<'_>) -> ProjectOptions {
    let defaults = ProjectOptions::default();
    let base = ConversionFactors::default();
    let per_day = bounded_u32(f, "MinutesPerDay", 1..=1_440, base.minutes_per_day(), sink);
    let per_week = bounded_u32(f, "MinutesPerWeek", 1..=10_080, base.minutes_per_week(), sink);
    let per_month = bounded_u32(f, "DaysPerMonth", 1..=31, base.days_per_month(), sink);
    let factors = ConversionFactors::new(per_day, per_week, per_month).unwrap_or(base);

    let duration_unit = match code(f, "DurationFormat", codes::duration_format, sink) {
        Some(DurationCode::Time { unit, .. }) => unit,
        Some(DurationCode::Null { .. }) | None => defaults.default_duration_unit,
        Some(DurationCode::Percent { .. }) => {
            if let Some((t, at)) = f.scalar("DurationFormat", sink) {
                f.invalid("DurationFormat", at, t, sink);
            }
            defaults.default_duration_unit
        }
    };
    ProjectOptions {
        factors,
        default_start_time: f
            .time("DefaultStartTime", sink)
            .unwrap_or(defaults.default_start_time),
        default_finish_time: f
            .time("DefaultFinishTime", sink)
            .unwrap_or(defaults.default_finish_time),
        default_duration_unit: duration_unit,
        default_work_unit: code(f, "WorkFormat", codes::work_format, sink)
            .unwrap_or(defaults.default_work_unit),
        // R-076 : valeurs `default=` du schéma pour un élément absent.
        default_task_type: code(f, "DefaultTaskType", codes::task_type, sink)
            .unwrap_or(TaskType::FixedDuration),
        new_tasks_effort_driven: f.bool("NewTasksEffortDriven", sink).unwrap_or(true),
        new_tasks_estimated: f.bool("NewTasksEstimated", sink).unwrap_or(true),
        week_start_day: code(f, "WeekStartDay", codes::week_start_day, sink)
            .unwrap_or(defaults.week_start_day),
        honor_constraints: f.bool("HonorConstraints", sink).unwrap_or(true),
        // R-086 : seuil de criticité en jours, entier >= 0 ; un seul chemin
        // critique par défaut.
        critical_slack_limit_days: bounded_u32(f, "CriticalSlackLimit", 0..=u32::MAX, 0, sink),
        multiple_critical_paths: f.bool("MultipleCriticalPaths", sink).unwrap_or(false),
        progress: progress_options(f, sink),
    }
}

/// R-214, R-218 : options de l'avancement (S54), lues telles quelles et
/// réécrites ; absentes : défauts d'un projet neuf (O36). Une option que le
/// calcul ne suit pas (`TaskUpdatesResource` à 0, une option `Move*` à 1)
/// donne `progress.option_unsupported { option }` (info, une fois par
/// option) : le calcul garde les valeurs par défaut.
fn progress_options(f: &Fields<'_>, sink: &mut Sink<'_>) -> ProgressOptions {
    let d = ProgressOptions::default();
    let read = |name: &str, default: bool, sink: &mut Sink<'_>| f.bool(name, sink).unwrap_or(default);
    let options = ProgressOptions {
        split_in_progress: read("SplitsInProgressTasks", d.split_in_progress, sink),
        task_updates_resource: read("TaskUpdatesResource", d.task_updates_resource, sink),
        move_completed_ends_back: read("MoveCompletedEndsBack", d.move_completed_ends_back, sink),
        move_remaining_starts_back: read("MoveRemainingStartsBack", d.move_remaining_starts_back, sink),
        move_remaining_starts_forward: read(
            "MoveRemainingStartsForward",
            d.move_remaining_starts_forward,
            sink,
        ),
        move_completed_ends_forward: read("MoveCompletedEndsForward", d.move_completed_ends_forward, sink),
    };
    for option in options.unsupported() {
        let name = option.mspdi_name();
        let (at, value) = f
            .scalar(name, sink)
            .map_or((f.at(), None), |(t, at)| (at, Some(t)));
        f.report(
            ImportCode::Core(Diagnostic::ProgressOptionUnsupported { option }),
            name,
            at,
            value,
            sink,
        );
    }
    options
}

/// Calendrier du projet (`CalendarUID`).
fn project_calendar(f: &Fields<'_>, sink: &mut Sink<'_>) -> CalendarUid {
    match f.int("CalendarUID", sink) {
        None => {
            if !f.has("CalendarUID") {
                f.missing("CalendarUID", sink);
            }
            FALLBACK_CALENDAR
        }
        Some((v, at)) => u32::try_from(v).map_or_else(
            |_| {
                f.invalid("CalendarUID", at, &v.to_string(), sink);
                FALLBACK_CALENDAR
            },
            CalendarUid::new,
        ),
    }
}
