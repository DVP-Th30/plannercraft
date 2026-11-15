//! Assemblage du projet : propriétés (`   114/Props`, R-288), calendriers,
//! tâches, liens ; comptes de ce qui n'est pas repris.

use std::collections::{BTreeMap, BTreeSet};

use plannercraft_core::calendar::model::Calendar;
use plannercraft_core::{
    Arena, ConversionFactors, DurationUnit, Project, ProjectInit, ScheduleAnchor, TaskUid, TimeOfDay,
};
use plannercraft_mspdi::codes::{self, DurationCode};

use crate::bytes::{le_u16, le_u32, utf16_z};
use crate::cfb::Cfb;
use crate::detect::Found;
use crate::diag::{MppCode, Sink};
use crate::layout::project_prop;
use crate::tables::{self, FixedItem, ItemLen};
use crate::time::{self, Stamp};

use crate::{MppError, NotImported, ReadOptions, Recorded, calendar, task};

/// Projet assemblé.
#[derive(Debug)]
pub struct Built {
    /// Projet.
    pub project: Project,
    /// Dates enregistrées.
    pub recorded: Recorded,
    /// Non repris.
    pub not_imported: NotImported,
    /// Enregistrements de tâche en service (R-292).
    pub task_records: u32,
    /// Dont incohérents (R-292).
    pub inconsistent_records: u32,
}

/// Flux d'une entité : (méta fixe, données fixes, méta variable, données
/// variables) ; un flux absent est vide.
struct Entity {
    fixed_meta: Vec<u8>,
    fixed_data: Vec<u8>,
    var_meta: Vec<u8>,
    var_data: Vec<u8>,
    fixed2_meta: Vec<u8>,
    fixed2_data: Vec<u8>,
}

fn entity(cfb: &Cfb<'_>, dir: usize, name: &str, max: usize) -> Entity {
    let read = |stream: &str| -> Vec<u8> {
        cfb.child(dir, name)
            .and_then(|e| cfb.child(e, stream))
            .and_then(|s| cfb.read_stream(s, max).ok())
            .unwrap_or_default()
    };
    Entity {
        fixed_meta: read("FixedMeta"),
        fixed_data: read("FixedData"),
        var_meta: read("VarMeta"),
        var_data: read("Var2Data"),
        fixed2_meta: read("Fixed2Meta"),
        fixed2_data: read("Fixed2Data"),
    }
}

/// Enregistrements vivants d'une table (ni réservés, ni supprimés, ni
/// courts) : comptes de ce qui n'est pas repris.
fn live(items: &[FixedItem<'_>]) -> u32 {
    let n = items
        .iter()
        .filter(|i| !i.deleted() && !i.short())
        .filter(|i| le_u32(i.data, 0).is_some_and(|u| u < 0xFFFF_0000))
        .count();
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn stamp_prop(p: &BTreeMap<u32, Vec<u8>>, key: u32) -> Option<plannercraft_core::Moment> {
    match time::stamp(le_u32(p.get(&key)?, 0)?) {
        Stamp::At(m) => Some(m),
        Stamp::Na | Stamp::Invalid => None,
    }
}

/// Titre, facteurs de conversion, heures et unité par défaut (R-288).
fn apply_props(project: &mut Project, props: &BTreeMap<u32, Vec<u8>>, sink: &mut Sink) {
    project.info.title = props
        .get(&project_prop::TITLE)
        .map(|b| utf16_z(b, 1_024))
        .unwrap_or_default();
    project.info.creation_date = None;
    let mpd = props
        .get(&project_prop::MINUTES_PER_DAY)
        .and_then(|v| le_u32(v, 0));
    let mpw = props
        .get(&project_prop::MINUTES_PER_WEEK)
        .and_then(|v| le_u32(v, 0));
    let dpm = props
        .get(&project_prop::DAYS_PER_MONTH)
        .and_then(|v| le_u16(v, 0))
        .map(u32::from);
    match (mpd, mpw, dpm) {
        (Some(d), Some(w), Some(m)) => match ConversionFactors::new(d, w, m) {
            Ok(f) => project.options.factors = f,
            Err(_) => sink.push(
                MppCode::PropertyMissing,
                None,
                Some(format!("facteurs {d}/{w}/{m}")),
            ),
        },
        _ => sink.push(
            MppCode::PropertyMissing,
            None,
            Some("minutes par jour et par semaine".to_owned()),
        ),
    }
    let time_prop = |key: u32| {
        props
            .get(&key)
            .and_then(|v| le_u16(v, 0))
            .and_then(|t| TimeOfDay::new(t / 10).ok())
    };
    project.status_date = stamp_prop(props, project_prop::STATUS_DATE);
    if let Some(d) = props
        .get(&project_prop::WEEK_START)
        .and_then(|v| le_u16(v, 0))
        .and_then(|c| codes::week_start_day(i64::from(c)))
    {
        project.options.week_start_day = d;
    }
    // Options de calcul (`u16`, 0 = non) ; absentes : celles d'un projet neuf.
    let flag = |key: u32| props.get(&key).and_then(|v| le_u16(v, 0)).map(|v| v != 0);
    if let Some(b) = flag(project_prop::TASK_UPDATES_RESOURCE) {
        project.options.progress.task_updates_resource = b;
    }
    if let Some(b) = flag(project_prop::SPLIT_IN_PROGRESS) {
        project.options.progress.split_in_progress = b;
    }
    if project_prop::HONOR_CONSTRAINTS
        .iter()
        .all(|k| flag(*k) == Some(false))
    {
        project.options.honor_constraints = false;
    }
    if let Some(t) = time_prop(project_prop::DEFAULT_START) {
        project.options.default_start_time = t;
    }
    if let Some(t) = time_prop(project_prop::DEFAULT_FINISH) {
        project.options.default_finish_time = t;
    }
    if let Some(code) = props
        .get(&project_prop::DURATION_FORMAT)
        .and_then(|v| le_u16(v, 0))
        && let Some(DurationCode::Time { unit, .. }) = codes::duration_format(i64::from(code))
    {
        project.options.default_duration_unit = unit;
    }
}

/// Calendriers de base au projet ; calendrier du projet par son nom
/// (`0x0240000E`), sinon le premier (R-287, R-288). Sans calendrier lu, les
/// calendriers d'un projet neuf restent.
fn install_calendars(
    project: &mut Project,
    base: Vec<Calendar>,
    props: &BTreeMap<u32, Vec<u8>>,
    sink: &mut Sink,
) {
    let calendar_name = props
        .get(&project_prop::CALENDAR_NAME)
        .map(|b| utf16_z(b, 255))
        .unwrap_or_default();
    let shown = if calendar_name.is_empty() {
        "nom du calendrier du projet absent".to_owned()
    } else {
        calendar_name.clone()
    };
    if base.is_empty() {
        sink.push(MppCode::ProjectCalendarFallback, None, Some(shown));
    } else {
        let project_calendar = base
            .iter()
            .find(|c| c.name == calendar_name)
            .or_else(|| {
                sink.push(MppCode::ProjectCalendarFallback, None, Some(shown.clone()));
                base.first()
            })
            .map(Calendar::uid);
        let mut arena = Arena::new();
        for c in base {
            // UID uniques (filtrés à la lecture) : l'ajout ne peut pas échouer.
            let _ = arena.push(c);
        }
        project.calendars = arena;
        if let Some(uid) = project_calendar {
            project.calendar = uid;
        }
    }
}

/// R-292 : refus d'une table de tâches incohérente : des enregistrements en
/// service sans aucune tâche lue, ou un quart au moins d'enregistrements
/// incohérents (au moins deux). Seuils : `docs/plans/mpp-import.md` § 9.
#[must_use]
pub const fn refuse_inconsistent(records: u32, inconsistent: u32, any_task: bool) -> bool {
    if records > 0 && !any_task {
        return true;
    }
    inconsistent >= 2 && inconsistent.saturating_mul(4) >= records
}

/// Ce que ce lot ne reprend pas : enregistrements vivants des ressources et
/// des affectations (comptes approchés), calendriers dérivés.
fn not_imported(cfb: &Cfb<'_>, dir: usize, max: usize, records: usize, derived: u32) -> NotImported {
    let count = |name: &str, len: usize| {
        let e = entity(cfb, dir, name, max);
        live(&tables::fixed_items(
            &e.fixed_meta,
            &e.fixed_data,
            records,
            ItemLen::Known(len),
        ))
    };
    NotImported {
        resources: count("TBkndRsc", 37),
        assignments: count("TBkndAssn", 34),
        resource_calendars: derived,
    }
}

/// Assemble le projet.
///
/// # Errors
///
/// [`MppError::MissingStream`] sans flux de tâches.
pub fn build(
    cfb: &Cfb<'_>,
    found: &Found,
    options: &ReadOptions,
    sink: &mut Sink,
) -> Result<Built, MppError> {
    let max = options.limits.max_stream_bytes;
    let records = options.limits.max_records;
    let dir = found.data_dir;
    let props_raw = cfb
        .child(dir, "Props")
        .and_then(|s| cfb.read_stream(s, max).ok())
        .unwrap_or_default();
    let props = tables::props(&props_raw, records);

    // Ancre et facteurs.
    let start = stamp_prop(&props, project_prop::START);
    let finish = stamp_prop(&props, project_prop::FINISH);
    let from_finish = props.get(&project_prop::FROM_START).and_then(|v| le_u16(v, 0)) == Some(0);
    let anchor = match (from_finish, start, finish) {
        (true, _, Some(f)) => ScheduleAnchor::FromFinish(f),
        (_, Some(s), _) => ScheduleAnchor::FromStart(s),
        _ => {
            sink.push(MppCode::PropertyMissing, None, Some("début du projet".to_owned()));
            ScheduleAnchor::FromStart(options.current_date)
        }
    };
    let mut project = Project::new(ProjectInit {
        name: options.name.clone(),
        anchor,
        current_date: options.current_date,
        calendar_names: options.calendar_names.clone(),
    });
    apply_props(&mut project, &props, sink);
    let default_unit: DurationUnit = project.options.default_duration_unit;

    // Calendriers.
    let cal = entity(cfb, dir, "TBkndCal", max);
    let cal_items = tables::fixed_items(&cal.fixed_meta, &cal.fixed_data, records, ItemLen::Known(10));
    let cal_var = tables::var_values(&cal.var_meta, &cal.var_data, records);
    let defaults = calendar::default_week(props.get(&project_prop::DEFAULT_WEEK).map(Vec::as_slice), sink);
    let calendars = calendar::read(&cal_items, &cal_var, found.layout, &defaults, sink);
    install_calendars(&mut project, calendars.base, &props, sink);
    let known_calendars: BTreeSet<u32> = project.calendars.iter().map(|c| c.uid().get()).collect();

    // Tâches et liens.
    let tasks = entity(cfb, dir, "TBkndTask", max);
    if tasks.fixed_meta.is_empty() {
        return Err(MppError::MissingStream("TBkndTask/FixedMeta"));
    }
    let task_items = tables::fixed_items(&tasks.fixed_meta, &tasks.fixed_data, records, ItemLen::Known(47));
    let task_slots = tables::item_geometry(&tasks.fixed_meta, ItemLen::Known(47)).map_or(0, |(_, n)| n);
    let task_var = tables::var_values(&tasks.var_meta, &tasks.var_data, records);
    let task_items2 = tables::fixed_items(
        &tasks.fixed2_meta,
        &tasks.fixed2_data,
        records,
        ItemLen::Slots(task_slots),
    );
    let mut read = task::read(
        &task_items,
        &task_items2,
        &task_var,
        found.layout,
        &project.options,
        &known_calendars,
        sink,
    );
    let cons = entity(cfb, dir, "TBkndCons", max);
    let cons_items = tables::fixed_items(&cons.fixed_meta, &cons.fixed_data, records, ItemLen::Known(10));
    // R-292 : lire juste ou ne pas lire.
    if refuse_inconsistent(
        read.records,
        read.inconsistent,
        read.summary.is_some() || !read.tasks.is_empty(),
    ) {
        return Err(MppError::LayoutUnrecognized {
            saved_by: found.saved_by.clone(),
            records: read.records,
            inconsistent: read.inconsistent,
        });
    }
    let (task_records, inconsistent_records) = (read.records, read.inconsistent);
    task::attach_links(&cons_items, found.layout, &mut read, default_unit, sink);
    project.summary_task = read.summary;
    let mut arena = Arena::new();
    for t in read.tasks {
        let uid: TaskUid = t.uid();
        if arena.push(t).is_err() {
            sink.push(MppCode::DuplicateUid, Some(uid.get()), None);
        }
    }
    project.tasks = arena;

    let not_imported = not_imported(cfb, dir, max, records, calendars.derived);
    Ok(Built {
        project,
        recorded: Recorded {
            project_start: start,
            project_finish: finish,
            tasks: read.recorded,
        },
        not_imported,
        task_records,
        inconsistent_records,
    })
}
