//! Exécution des exceptions de calendrier (R-165, R-161).
//!
//! Une exception posée par commande couvre chaque jour de `[from, to]`
//! (récurrence « période ») ; elle remplace une exception de même période,
//! et toute autre exception qui partage un jour avec elle est un refus
//! (pas de découpage implicite).
//!
//! J7 (lot L5, ADR-0015 D8) : `calendar.add`, `calendar.update`,
//! `calendar.set_week`, `calendar.delete` (R-247, R-248) ; nom d'exception
//! (R-165 amendée, O44).

use plannercraft_core::calendar::{
    Calendar, CalendarException as ModelException, CalendarKind, DayHours, DayRule, DaySetting, Recurrence,
    TimeRange, builtin,
};
use plannercraft_core::time::{Week, Weekday};
use plannercraft_core::{
    Assignment, AssignmentUid, CalendarModelError, CalendarUid, DayRange, HasUid, Project, ResourceUid,
    Subject, TaskUid, TimeOfDay,
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::exec::Ctx;
use super::extras_invalidation::{ASSIGNMENT_SCHEDULE_KEYS, drop_keys};
use super::{CalendarException, CalendarExceptionKey, CalendarRef, Patch};
use crate::error::{CommandError, CommandErrorKind, MAX_LISTED_CALENDAR_USERS, codes};
use crate::json::{DaySettingJson, RangeJson, WeekSettingJson, WeekdayJson};
use crate::outcome::SideEffect;

/// `param.invalid` sur `field`.
fn invalid(why: String, field: &str) -> CommandError {
    CommandError::new(codes::PARAM_INVALID, CommandErrorKind::InvalidParam(why)).with_field(field)
}

/// UID du calendrier désigné, qui doit exister.
fn resolve_calendar(p: &Project, r: CalendarRef) -> Result<CalendarUid, CommandError> {
    let uid = match r {
        CalendarRef::Project => p.calendar,
        CalendarRef::Uid(u) => u,
    };
    if p.calendars.contains(uid) {
        Ok(uid)
    } else {
        Err(CommandError::new(
            codes::CALENDAR_NOT_FOUND,
            CommandErrorKind::InvalidParam(format!("aucun calendrier d'UID {uid}")),
        )
        .with_field("cal")
        .with_subject(Subject::Calendar(uid)))
    }
}

/// Réglage des jours : chômés sans plage, sinon ouvrés sur les plages
/// (validées par le modèle : 1 à 5, non vides, sans chevauchement).
fn setting(ranges: &[[TimeOfDay; 2]]) -> Result<DaySetting, CommandError> {
    if ranges.is_empty() {
        return Ok(DaySetting::NonWorking);
    }
    let bad = |e: &CalendarModelError| invalid(e.to_string(), "ranges");
    let ranges = ranges
        .iter()
        .map(|[from, to]| TimeRange::new(*from, *to))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| bad(&e))?;
    DayHours::new(ranges)
        .map(DaySetting::Working)
        .map_err(|e| bad(&e))
}

/// `calendar.exception_overlap` : chevauchement avec l'exception `existing`.
fn overlap(cal: CalendarUid, existing: usize) -> CommandError {
    CommandError::new(
        codes::CALENDAR_EXCEPTION_OVERLAP,
        CommandErrorKind::InvalidParam(format!(
            "l'exception chevauche l'exception n° {existing} du calendrier {cal} sans la recouvrir exactement"
        )),
    )
    .with_field("from")
    .with_subject(Subject::Calendar(cal))
}

/// R-161 : jette la répartition (`TimephasedData`) des affectations
/// planifiées par le calendrier `cal` ou un calendrier qui en dérive : celles
/// des tâches qui l'utilisent (calendrier propre, ou celui du projet à
/// défaut) et celles des ressources qui l'utilisent. Un seul effet résumé,
/// sujet le calendrier.
fn drop_assignment_extras(ctx: &mut Ctx<'_>, cal: CalendarUid) {
    let p = &mut *ctx.project;
    let affected: Vec<CalendarUid> = p
        .calendars
        .iter()
        .filter(|c| c.uid() == cal || matches!(c.kind(), CalendarKind::Derived { base, .. } if *base == cal))
        .map(Calendar::uid)
        .collect();
    let project_calendar = p.calendar;
    let uses = |c: Option<CalendarUid>| affected.contains(&c.unwrap_or(project_calendar));
    let targets: Vec<AssignmentUid> = p
        .assignments
        .iter()
        .filter(|a| {
            let by_task = p.tasks.get(a.task).is_some_and(|t| uses(t.calendar));
            let by_resource = a
                .resource
                .and_then(|r| p.resources.get(r))
                .is_some_and(|r| r.calendar.is_some_and(|c| affected.contains(&c)));
            (by_task || by_resource) && !a.extras.is_empty()
        })
        .map(Assignment::uid)
        .collect();
    let mut dropped = Vec::new();
    for uid in targets {
        if let Some(a) = p.assignments.get_mut(uid) {
            dropped.extend(drop_keys(&mut a.extras, ASSIGNMENT_SCHEDULE_KEYS));
        }
    }
    dropped.sort();
    dropped.dedup();
    if !dropped.is_empty() {
        ctx.effects.push(SideEffect::ExtrasDropped {
            subject: Subject::Calendar(cal),
            keys: dropped,
        });
    }
}

impl Ctx<'_> {
    /// R-161 : jette la répartition des affectations planifiées par `cal`
    /// ou un calendrier qui en dérive (calendrier du projet changé, R-249).
    pub(crate) fn drop_calendar_assignment_extras(&mut self, cal: CalendarUid) {
        drop_assignment_extras(self, cal);
    }
}

/// `calendar.set_exception` (R-165).
pub(super) fn set_exception(op: &CalendarException, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    // Nom d'exception (R-165 amendée, O44) : au plus 512 caractères.
    if let Some(n) = &op.name {
        check_length(n, "name")?;
    }
    let cal = resolve_calendar(ctx.project, op.cal)?;
    let period = DayRange::new(op.from, op.to).map_err(|e| invalid(e.to_string(), "to"))?;
    let setting = setting(&op.ranges)?;
    ctx.touched.insert(Subject::Calendar(cal), ctx.op_index);
    let calendar = ctx.project.calendars.get_mut(cal).ok_or_else(|| {
        CommandError::new(
            codes::CALENDAR_NOT_FOUND,
            CommandErrorKind::InvalidParam(String::new()),
        )
        .with_field("cal")
    })?;
    // Remplacement : l'exception de même période est retirée (son nom est
    // gardé sauf `name` donné), puis la nouvelle ajoutée ; le modèle refuse
    // tout autre chevauchement (jours dépliés, R-017, R-018).
    let mut name = String::new();
    if let Some(i) = calendar.exceptions().iter().position(|e| e.period == period) {
        let old = calendar
            .remove_exception(i)
            .map_err(|e| invalid(e.to_string(), "from"))?;
        name = old.name;
    }
    if let Some(n) = &op.name {
        name.clone_from(n);
    }
    let exception = ModelException {
        name,
        period,
        setting,
        recurrence: Recurrence::Period,
        occurrences: None,
    };
    match calendar.add_exception(exception) {
        Ok(_) => {}
        Err(CalendarModelError::OverlappingException { existing }) => return Err(overlap(cal, existing)),
        Err(e) => return Err(invalid(e.to_string(), "from")),
    }
    drop_assignment_extras(ctx, cal);
    Ok(())
}

/// `calendar.remove_exception` (R-165) : retire l'exception qui commence le
/// jour `from` (la première dans l'ordre de stockage).
pub(super) fn remove_exception(op: &CalendarExceptionKey, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let cal = resolve_calendar(ctx.project, op.cal)?;
    ctx.touched.insert(Subject::Calendar(cal), ctx.op_index);
    let calendar = ctx.project.calendars.get_mut(cal).ok_or_else(|| {
        CommandError::new(
            codes::CALENDAR_NOT_FOUND,
            CommandErrorKind::InvalidParam(String::new()),
        )
        .with_field("cal")
    })?;
    let i = calendar
        .exceptions()
        .iter()
        .position(|e| e.period.first() == op.from)
        .ok_or_else(|| {
            CommandError::new(
                codes::CALENDAR_EXCEPTION_NOT_FOUND,
                CommandErrorKind::InvalidParam(format!(
                    "aucune exception du calendrier {cal} ne commence le {}",
                    op.from
                )),
            )
            .with_field("from")
            .with_subject(Subject::Calendar(cal))
        })?;
    calendar
        .remove_exception(i)
        .map_err(|e| invalid(e.to_string(), "from"))?;
    drop_assignment_extras(ctx, cal);
    Ok(())
}

/// `calendar.add` (J7, R-248) : nouveau calendrier de base (aucun de
/// `base`, `copy_of`), dérivé de `base`, ou copie de `copy_of` (les deux :
/// `param.invalid`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CalendarAdd {
    /// Nom (non vide, unique sans tenir compte de la casse).
    pub name: String,
    /// Calendrier de base dont le nouveau dérive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<u32>,
    /// Calendrier copié (semaine, exceptions et base).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copy_of: Option<u32>,
}

/// `calendar.update` (J7, R-248) : nom et base ; absent gardé, `null` sur
/// `base` : le calendrier devient un calendrier de base.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CalendarUpdate {
    /// Calendrier.
    pub uid: u32,
    /// Nom (`null` refusé).
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub name: Patch<String>,
    /// Base.
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub base: Patch<u32>,
}

/// `calendar.set_week` (J7, R-247) : réglage de jours de la semaine type ;
/// jours non cités inchangés.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CalendarSetWeek {
    /// Calendrier (UID ou `"project"`).
    pub calendar: CalendarRef,
    /// Jours réglés.
    pub days: Vec<DaySettingJson>,
}

/// `calendar.delete` (J7, R-248) : refusé `calendar.in_use` si le
/// calendrier sert (projet, tâche, ressource, base).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CalendarDelete {
    /// Calendrier.
    pub uid: u32,
}

/// Longueur maximale d'un nom de calendrier ou d'exception, en caractères
/// (comme les noms de tâche et de ressource).
const MAX_NAME_CHARS: usize = super::task::MAX_NAME_CHARS;

/// Texte de plus de [`MAX_NAME_CHARS`] caractères : `param.invalid`.
fn check_length(s: &str, field: &str) -> Result<(), CommandError> {
    if s.chars().count() > MAX_NAME_CHARS {
        return Err(invalid(
            format!("texte de plus de {MAX_NAME_CHARS} caractères"),
            field,
        ));
    }
    Ok(())
}

/// `calendar.not_found` pour `uid`, sur `field`.
fn not_found(uid: CalendarUid, field: &str) -> CommandError {
    CommandError::new(
        codes::CALENDAR_NOT_FOUND,
        CommandErrorKind::InvalidParam(format!("aucun calendrier d'UID {uid}")),
    )
    .with_field(field)
    .with_subject(Subject::Calendar(uid))
}

/// R-248 : nom non vide (espaces exclus), au plus 512 caractères, unique
/// sans tenir compte de la casse parmi les calendriers autres que `except`.
fn check_name(p: &Project, name: &str, except: Option<CalendarUid>, field: &str) -> Result<(), CommandError> {
    if name.trim().is_empty() {
        return Err(invalid("nom de calendrier vide".to_owned(), field));
    }
    check_length(name, field)?;
    let key = name.to_lowercase();
    if let Some(other) = p
        .calendars
        .iter()
        .find(|c| Some(c.uid()) != except && c.name.to_lowercase() == key)
    {
        return Err(CommandError::new(
            codes::CALENDAR_NAME_TAKEN,
            CommandErrorKind::CalendarNameTaken {
                name: name.to_owned(),
                existing: other.uid(),
            },
        )
        .with_field(field)
        .with_subject(Subject::Calendar(other.uid())));
    }
    Ok(())
}

/// R-248 : `base` existe et est un calendrier de base (héritage à un
/// niveau, J1).
fn check_base(p: &Project, base: CalendarUid, field: &str) -> Result<(), CommandError> {
    let Some(c) = p.calendars.get(base) else {
        return Err(not_found(base, field));
    };
    if !c.is_base() {
        return Err(CommandError::new(
            codes::CALENDAR_BASE_NOT_BASE,
            CommandErrorKind::CalendarBaseNotBase { base },
        )
        .with_field(field)
        .with_subject(Subject::Calendar(base)));
    }
    Ok(())
}

/// Calendriers dérivés de `base`, par UID croissant.
fn derived_of(p: &Project, base: CalendarUid) -> Vec<CalendarUid> {
    let mut v: Vec<CalendarUid> = p
        .calendars
        .iter()
        .filter(|c| matches!(c.kind(), CalendarKind::Derived { base: b, .. } if *b == base))
        .map(Calendar::uid)
        .collect();
    v.sort();
    v
}

/// Copie de `c` sous l'UID `uid`, de nature `kind` : nom, semaines datées et
/// exceptions recopiées ; GUID et `Extras` (propres au fichier relu) non
/// recopiés pour une copie (`keep_identity` faux), gardés sinon.
fn rebuild(
    c: &Calendar,
    uid: CalendarUid,
    kind: CalendarKind,
    keep_identity: bool,
) -> Result<Calendar, CommandError> {
    let name = c.name.clone();
    let mut out = match kind {
        CalendarKind::Base { week } => Calendar::new_base(uid, name, week),
        CalendarKind::Derived { base, week } => {
            let mut d = Calendar::new_derived(uid, name, base);
            for day in Weekday::ALL {
                d.set_day(day, week.get(day).clone())
                    .map_err(|e| invalid(e.to_string(), "base"))?;
            }
            d
        }
    };
    for w in c.work_weeks() {
        out.add_work_week(w.clone())
            .map_err(|e| invalid(e.to_string(), "uid"))?;
    }
    for e in c.exceptions() {
        out.add_exception(e.clone())
            .map_err(|e| invalid(e.to_string(), "uid"))?;
    }
    if keep_identity {
        out.guid = c.guid;
        out.extras = c.extras.clone();
    }
    Ok(out)
}

/// Semaine complète d'un calendrier : la sienne s'il est de base, sinon
/// chaque jour hérité pris à sa base (jour chômé si la base manque).
fn resolved_week(p: &Project, c: &Calendar) -> Week<DaySetting> {
    match c.kind() {
        CalendarKind::Base { week } => week.clone(),
        CalendarKind::Derived { base, week } => {
            let base_week = p.calendars.get(*base).and_then(|b| match b.kind() {
                CalendarKind::Base { week } => Some(week),
                CalendarKind::Derived { .. } => None,
            });
            Week::from_fn(|d| match week.get(d) {
                DayRule::NonWorking => DaySetting::NonWorking,
                DayRule::Working(h) => DaySetting::Working(h.clone()),
                DayRule::Inherit => base_week.map_or(DaySetting::NonWorking, |w| w.get(d).clone()),
            })
        }
    }
}

/// `calendar.add` (R-248) : nom contrôlé, puis calendrier de base à la
/// semaine du Standard intégré, dérivé de `base`, ou copie de `copy_of` ;
/// UID suivant (compteur de R-195) ; `SideEffect::CalendarCreated`.
pub(super) fn add(op: &CalendarAdd, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    if op.base.is_some() && op.copy_of.is_some() {
        return Err(invalid(
            "« base » et « copy_of » s'excluent".to_owned(),
            "copy_of",
        ));
    }
    check_name(ctx.project, &op.name, None, "name")?;
    let source = match op.copy_of.map(CalendarUid::new) {
        Some(c) => Some(
            ctx.project
                .calendars
                .get(c)
                .ok_or_else(|| not_found(c, "copy_of"))?
                .clone(),
        ),
        None => None,
    };
    if let Some(b) = op.base.map(CalendarUid::new) {
        check_base(ctx.project, b, "base")?;
    }
    let uid = ctx.uids.alloc_calendar()?;
    let calendar = match (source, op.base.map(CalendarUid::new)) {
        (Some(src), _) => {
            let mut c = rebuild(&src, uid, src.kind().clone(), false)?;
            c.name.clone_from(&op.name);
            c
        }
        (None, Some(b)) => Calendar::new_derived(uid, op.name.clone(), b),
        (None, None) => builtin::standard(uid, op.name.clone()),
    };
    ctx.project
        .calendars
        .push(calendar)
        .map_err(|e| invalid(e.to_string(), "name"))?;
    ctx.effects.push(SideEffect::CalendarCreated(uid));
    ctx.touched.insert(Subject::Calendar(uid), ctx.op_index);
    Ok(())
}

/// `calendar.update` (R-248) : nom (`null` refusé) ; base posée (le
/// calendrier devient ou reste dérivé : jours réglés gardés, un calendrier
/// de base garde chacun de ses jours comme réglage propre ; refusé s'il sert
/// de base) ou retirée (`null` : il devient de base, jours hérités résolus
/// sur l'ancienne base). Exceptions et semaines datées gardées.
pub(super) fn update(op: &CalendarUpdate, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let uid = CalendarUid::new(op.uid);
    let Some(current) = ctx.project.calendars.get(uid) else {
        return Err(not_found(uid, "uid"));
    };
    let name = match &op.name {
        Patch::Keep => None,
        Patch::Clear => return Err(invalid("le nom ne s'efface pas".to_owned(), "name")),
        Patch::Set(n) => {
            check_name(ctx.project, n, Some(uid), "name")?;
            Some(n.clone())
        }
    };
    let kind = match op.base {
        Patch::Keep => None,
        Patch::Clear => match current.kind() {
            CalendarKind::Base { .. } => None,
            CalendarKind::Derived { .. } => Some(CalendarKind::Base {
                week: resolved_week(ctx.project, current),
            }),
        },
        Patch::Set(b) => {
            let b = CalendarUid::new(b);
            if b == uid {
                return Err(CommandError::new(
                    codes::CALENDAR_BASE_NOT_BASE,
                    CommandErrorKind::CalendarBaseNotBase { base: b },
                )
                .with_field("base")
                .with_subject(Subject::Calendar(b)));
            }
            check_base(ctx.project, b, "base")?;
            let dependents = derived_of(ctx.project, uid);
            if !dependents.is_empty() {
                return Err(CommandError::new(
                    codes::CALENDAR_BASE_NOT_BASE,
                    CommandErrorKind::CalendarServesAsBase {
                        calendar: uid,
                        count: dependents.len(),
                    },
                )
                .with_field("base")
                .with_subject(Subject::Calendar(uid)));
            }
            let week = match current.kind() {
                CalendarKind::Base { week } => Week::from_fn(|d| DayRule::from(week.get(d).clone())),
                CalendarKind::Derived { week, .. } => week.clone(),
            };
            Some(CalendarKind::Derived { base: b, week })
        }
    };
    let mut changed = false;
    let mut next = current.clone();
    if let Some(k) = kind
        && &k != current.kind()
    {
        next = rebuild(current, uid, k, true)?;
        changed = true;
    }
    if let Some(n) = name {
        next.name = n;
    }
    if let Some(slot) = ctx.project.calendars.get_mut(uid) {
        *slot = next;
    }
    ctx.touched.insert(Subject::Calendar(uid), ctx.op_index);
    if changed {
        // R-161 : la base change le placement des affectations planifiées.
        drop_assignment_extras(ctx, uid);
    }
    Ok(())
}

/// Jour du modèle d'un jour JSON.
const fn weekday(d: WeekdayJson) -> Weekday {
    match d {
        WeekdayJson::Monday => Weekday::Monday,
        WeekdayJson::Tuesday => Weekday::Tuesday,
        WeekdayJson::Wednesday => Weekday::Wednesday,
        WeekdayJson::Thursday => Weekday::Thursday,
        WeekdayJson::Friday => Weekday::Friday,
        WeekdayJson::Saturday => Weekday::Saturday,
        WeekdayJson::Sunday => Weekday::Sunday,
    }
}

/// R-247 : plages d'un jour, triées par début, disjointes (contiguës
/// admises), de durée > 0, 1 à 5 (`DayHours::MAX_RANGES`, borne de
/// `WorkingTimes` du schéma pj15).
fn week_ranges(ranges: &[RangeJson], field: &str) -> Result<DayHours, CommandError> {
    if ranges.is_empty() {
        return Err(invalid(
            "au moins une plage (sinon « non_working »)".to_owned(),
            field,
        ));
    }
    if ranges.len() > DayHours::MAX_RANGES {
        return Err(invalid(
            format!("{} plages : au plus {}", ranges.len(), DayHours::MAX_RANGES),
            field,
        ));
    }
    let mut out = Vec::with_capacity(ranges.len());
    let mut last_end: Option<TimeOfDay> = None;
    for r in ranges {
        let range = TimeRange::new(r.from, r.to).map_err(|e| invalid(e.to_string(), field))?;
        if last_end.is_some_and(|end| r.from < end) {
            return Err(invalid(
                "plages non triées ou qui se chevauchent".to_owned(),
                field,
            ));
        }
        last_end = Some(r.to);
        out.push(range);
    }
    DayHours::new(out).map_err(|e| invalid(e.to_string(), field))
}

/// `calendar.set_week` (R-247) : jours cités réglés (hérité, chômé, ou
/// ouvré sur des plages), jours non cités inchangés ; un jour cité deux fois
/// ou `inherit` sur un calendrier de base : `param.invalid` sur
/// `days[i].day` ou `days[i].setting`. Une semaine résultante sans minute
/// ouvrée est acceptée : `calendar.no_working_time` en avertissement
/// (R-160 amendée, ADR-0015 D8), les tâches concernées repassent par R-066.
pub(super) fn set_week(op: &CalendarSetWeek, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let cal = resolve_calendar(ctx.project, op.calendar).map_err(|e| CommandError {
        field: Some("calendar".to_owned()),
        ..e
    })?;
    let is_base = ctx.project.calendars.get(cal).is_some_and(Calendar::is_base);
    let mut rules: Vec<(Weekday, DayRule)> = Vec::with_capacity(op.days.len());
    for (i, d) in op.days.iter().enumerate() {
        let day = weekday(d.day);
        if rules.iter().any(|(seen, _)| *seen == day) {
            return Err(invalid(
                "jour cité deux fois".to_owned(),
                &format!("days[{i}].day"),
            ));
        }
        let field = format!("days[{i}].setting");
        let rule = match &d.setting {
            WeekSettingJson::Inherit if is_base => {
                return Err(invalid(
                    "un calendrier de base n'hérite d'aucun calendrier".to_owned(),
                    &field,
                ));
            }
            WeekSettingJson::Inherit => DayRule::Inherit,
            WeekSettingJson::NonWorking => DayRule::NonWorking,
            WeekSettingJson::Ranges(r) => DayRule::Working(week_ranges(r, &field)?),
        };
        rules.push((day, rule));
    }
    let calendar = ctx
        .project
        .calendars
        .get_mut(cal)
        .ok_or_else(|| not_found(cal, "calendar"))?;
    for (day, rule) in rules {
        calendar
            .set_day(day, rule)
            .map_err(|e| invalid(e.to_string(), "days"))?;
    }
    ctx.touched.insert(Subject::Calendar(cal), ctx.op_index);
    drop_assignment_extras(ctx, cal);
    Ok(())
}

/// R-248 : utilisateurs d'un calendrier, dans l'ordre projet, tâches,
/// ressources, calendriers dérivés (UID croissants).
fn users_of(p: &Project, cal: CalendarUid) -> Vec<Subject> {
    let mut users = Vec::new();
    if p.calendar == cal {
        users.push(Subject::Project);
    }
    let mut tasks: Vec<TaskUid> = p
        .tasks
        .iter()
        .filter(|t| t.calendar == Some(cal))
        .map(HasUid::uid)
        .collect();
    tasks.sort();
    if p.summary_task.as_ref().is_some_and(|t| t.calendar == Some(cal)) {
        users.push(Subject::Task(TaskUid::PROJECT_SUMMARY));
    }
    users.extend(tasks.into_iter().map(Subject::Task));
    let mut resources: Vec<ResourceUid> = p
        .resources
        .iter()
        .filter(|r| r.calendar == Some(cal))
        .map(HasUid::uid)
        .collect();
    resources.sort();
    users.extend(resources.into_iter().map(Subject::Resource));
    users.extend(derived_of(p, cal).into_iter().map(Subject::Calendar));
    users
}

/// `calendar.delete` (R-248) : refusé `calendar.in_use` si le calendrier
/// sert (projet, tâche, ressource, base), utilisateurs listés (20 au plus).
pub(super) fn delete(op: &CalendarDelete, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let uid = CalendarUid::new(op.uid);
    if !ctx.project.calendars.contains(uid) {
        return Err(not_found(uid, "uid"));
    }
    let users = users_of(ctx.project, uid);
    if !users.is_empty() {
        let total = users.len();
        let listed = users.into_iter().take(MAX_LISTED_CALENDAR_USERS).collect();
        return Err(CommandError::new(
            codes::CALENDAR_IN_USE,
            CommandErrorKind::CalendarInUse {
                calendar: uid,
                users: listed,
                total,
            },
        )
        .with_field("uid")
        .with_subject(Subject::Calendar(uid)));
    }
    ctx.project.calendars.remove(uid);
    ctx.effects.push(SideEffect::CalendarDeleted(uid));
    ctx.touched.insert(Subject::Calendar(uid), ctx.op_index);
    Ok(())
}
