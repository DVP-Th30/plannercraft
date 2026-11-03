//! Import des tâches, des liens et des valeurs enregistrées (R-071, R-074,
//! R-076, R-083 à R-085) ; retards de nivellement (R-198) ; travail fixe
//! toujours piloté par l'effort (R-193).

use std::collections::BTreeSet;

use plannercraft_core::progress::{ProgressState, facts};
use plannercraft_core::{
    CalendarUid, Constraint, Decimal, Diagnostic, Duration, DurationUnit, Lag, LagValue, LevelingDelay, Link,
    LinkType, Minutes, Moment, OutlineLevel, Percent, Priority, Project, ProjectOptions, SchedulingMode,
    SignedPercent, Subject, Task, TaskType, TaskUid, TimeBasis, Work,
};

use crate::baseline;
use crate::codes::{self, ConstraintKind, DurationCode};
use crate::custom;
use crate::diag::{ImportCode, LinkIgnoredReason, Sink};
use crate::extras;
use crate::fields::Fields;
use crate::project::{code, guid};
use crate::recorded::{RecordedSchedule, RecordedTask, Tenths};
use crate::scalar::{percent_lag_from_link_lag, tenths_to_minutes};
use crate::schema::Parent;
use crate::xml::Element;

/// Chemin d'une tâche.
const TASK: &str = "Tasks/Task";
/// Chemin d'un lien.
const LINK: &str = "Tasks/Task/PredecessorLink";

/// Tâche lue, avant la résolution des liens.
struct Read<'e> {
    task: Task,
    element: &'e Element<'e>,
    /// Durée telle qu'écrite (minutes ≥ 0, unité, base, estimation).
    duration: Duration,
}

/// R-214 : restes relus remplacés par la dérivation (R-200, R-211),
/// comptés pour un seul `mspdi.progress_inconsistent` par fichier.
#[derive(Debug, Default)]
pub(crate) struct Replaced {
    /// Valeurs relues remplacées.
    count: u32,
    /// Premier remplacement : position et chemin de l'élément.
    first: Option<(u64, String)>,
}

impl Replaced {
    /// Note un reste relu `read` remplacé par `derived` s'ils diffèrent
    /// (élément `path` présent à la position `at`).
    pub(crate) fn note<T: PartialEq>(&mut self, read: Option<T>, derived: &T, at: u64, path: &str) {
        if read.is_some_and(|r| r != *derived) {
            self.count = self.count.saturating_add(1);
            if self.first.is_none() {
                self.first = Some((at, path.to_owned()));
            }
        }
    }

    /// Émet `mspdi.progress_inconsistent { count }` (info, sujet : le
    /// projet) s'il y a eu au moins un remplacement.
    pub(crate) fn report(self, sink: &mut Sink<'_>) {
        if let Some((at, path)) = self.first {
            sink.push(
                ImportCode::ProgressInconsistent { count: self.count },
                Some(Subject::Project),
                at,
                &path,
                None,
            );
        }
    }
}

/// R-214 : la reprise saisie (`Progress::resume`) n'a de sens que pour une
/// feuille commencée non achevée ; ailleurs, `Resume` n'est qu'une valeur
/// calculée (gardée dans `RecordedSchedule`).
pub(crate) fn keeps_resume(task: &Task, summary: bool) -> bool {
    !summary && facts(task).state == ProgressState::Started
}

/// Importe les tâches dans `project` et leurs valeurs enregistrées dans
/// `recorded` ; restes relus incohérents comptés dans `replaced` (R-214).
pub(crate) fn import(
    elements: &[Element<'_>],
    project: &mut Project,
    recorded: &mut RecordedSchedule,
    replaced: &mut Replaced,
    foreign: &BTreeSet<plannercraft_core::CustomFieldId>,
    sink: &mut Sink<'_>,
) {
    let options = project.options;
    let mut seen: BTreeSet<TaskUid> = BTreeSet::new();
    let mut rows: Vec<Read<'_>> = Vec::new();
    for element in elements {
        let mut f = Fields::new(&element.children, TASK, None, element.at);
        let Some(uid) = task_uid(&f, sink) else {
            continue;
        };
        if !seen.insert(uid) {
            f.report(
                ImportCode::Duplicate,
                "UID",
                element.at,
                Some(&uid.get().to_string()),
                sink,
            );
            continue;
        }
        f.set_subject(Some(Subject::Task(uid)));
        let rec = recorded_task(&f, sink);
        let position = rows.len();
        let (mut task, duration) = task(uid, &f, &rec, &options, position, sink);
        let baselines = baseline::task(&element.children, &options, f.subject(), sink);
        task.baselines = baselines.map;
        let mut kept = extras::collect(&element.children, Parent::Task, None);
        kept.extend(baselines.extras);
        // R-265 : valeurs des champs personnalisés modélisés.
        let subject = f.subject();
        custom::lift_task(&mut kept, &mut task.custom, foreign, |payload| {
            sink.push(
                ImportCode::CustomUnreadable,
                subject,
                element.at,
                "Tasks/Task/ExtendedAttribute",
                Some(payload),
            );
        });
        task.extras = plannercraft_core::Extras(kept);
        extras::canonical_order(&mut task.extras, Parent::Task);
        recorded.tasks.insert(uid, rec);
        if uid == TaskUid::PROJECT_SUMMARY {
            project.summary_task = Some(task);
        } else {
            rows.push(Read {
                task,
                element,
                duration,
            });
        }
    }

    // Hiérarchie (R-052) : récapitulative ⇔ la tâche non vide suivante a un
    // niveau supérieur. Le drapeau `Summary` n'est lu que pour la comparer.
    let summaries = summary_flags(&rows);
    let known: BTreeSet<TaskUid> = rows.iter().map(|r| r.task.uid()).collect();
    for (row, is_summary) in rows.iter_mut().zip(summaries) {
        let uid = row.task.uid();
        let f = Fields::new(
            &row.element.children,
            TASK,
            Some(Subject::Task(uid)),
            row.element.at,
        );
        if let Some(rec) = recorded.tasks.get(&uid)
            && let Some(flag) = rec.summary
            && flag != is_summary
            && !row.task.is_null
        {
            f.report(
                ImportCode::Inconsistent,
                "Summary",
                row.element.at,
                Some(if flag { "1" } else { "0" }),
                sink,
            );
        }
        if is_summary && matches!(row.task.mode, SchedulingMode::Auto) {
            // Durée d'une récapitulative automatique : valeur calculée, gardée
            // dans `recorded` seulement (ADR-0004, effacement structurel).
            row.task.duration = computed_zero(row.duration);
        } else if !row.task.is_null && !f.has("Duration") {
            f.missing("Duration", sink);
        }
        row.task.predecessors = links(uid, &row.element.children, &known, &options, sink);
        if !is_summary {
            force_effort_driven(&mut row.task, &f, sink);
            derive_progress(&mut row.task, &f, recorded.tasks.get(&uid), replaced);
        }
    }
    if let Some(summary) = &mut project.summary_task {
        summary.duration = computed_zero(summary.duration);
    }

    for row in rows {
        // UID uniques (filtrés plus haut) : l'ajout ne peut pas échouer.
        let _ = project.tasks.push(row.task);
    }
}

/// R-214 : progression relue d'une feuille. Restes (`RemainingDuration`,
/// `RemainingWork`) remplacés par la dérivation (R-200, R-211 : D − réel,
/// W − réel), comptés s'ils diffèrent ; `PercentComplete` relu reste
/// stocké, mais la durée réelle l'emporte (R-200). `Resume` d'une feuille
/// commencée non achevée → `Progress::resume`.
fn derive_progress(task: &mut Task, f: &Fields<'_>, rec: Option<&RecordedTask>, replaced: &mut Replaced) {
    let (duration, work) = crate::write::remaining(task, false);
    if !task.is_null {
        let read_d = f
            .has("RemainingDuration")
            .then_some(task.progress.remaining_duration);
        replaced.note(read_d, &duration, f.at(), &f.path_of("RemainingDuration"));
        let read_w = f.has("RemainingWork").then_some(task.progress.remaining_work);
        replaced.note(read_w, &work, f.at(), &f.path_of("RemainingWork"));
    }
    task.progress.remaining_duration = duration;
    task.progress.remaining_work = work;
    if keeps_resume(task, false) {
        task.progress.resume = rec.and_then(|r| r.resume);
    }
}

/// R-071 : durée d'une récapitulative automatique ou de l'UID 0 (valeur
/// calculée, gardée dans `RecordedSchedule`) : minutes à 0, unité, base et
/// estimation gardées (amendement J3, R-133 : l'aller-retour d'une
/// récapitulative du projet manuelle garde l'estimation de sa durée saisie).
pub(crate) fn computed_zero(d: Duration) -> Duration {
    Duration::new(Minutes::ZERO, d.unit(), d.basis(), d.is_estimated()).unwrap_or(Duration::zero(d.unit()))
}

/// UID d'une tâche ; absent, négatif ou illisible : tâche ignorée.
fn task_uid(f: &Fields<'_>, sink: &mut Sink<'_>) -> Option<TaskUid> {
    let Some((v, at)) = f.int("UID", sink) else {
        if f.scalar("UID", sink).is_none() {
            f.missing("UID", sink);
        }
        return None;
    };
    if let Ok(u) = u32::try_from(v) {
        Some(TaskUid::new(u))
    } else {
        f.invalid("UID", at, &v.to_string(), sink);
        None
    }
}

/// R-083 : valeurs calculées enregistrées.
fn recorded_task(f: &Fields<'_>, sink: &mut Sink<'_>) -> RecordedTask {
    let tenths = |name: &str, sink: &mut Sink<'_>| f.int(name, sink).map(|(v, _)| Tenths(v));
    RecordedTask {
        start: f.datetime("Start", sink),
        finish: f.datetime("Finish", sink),
        early_start: f.datetime("EarlyStart", sink),
        early_finish: f.datetime("EarlyFinish", sink),
        late_start: f.datetime("LateStart", sink),
        late_finish: f.datetime("LateFinish", sink),
        total_slack: tenths("TotalSlack", sink),
        free_slack: tenths("FreeSlack", sink),
        start_slack: tenths("StartSlack", sink),
        finish_slack: tenths("FinishSlack", sink),
        critical: f.bool("Critical", sink),
        summary: f.bool("Summary", sink),
        duration: f.duration("Duration", sink).map(|(d, _)| Minutes::new(d.minutes)),
        assignments: 0,
        stop: f.datetime("Stop", sink),
        resume: f.datetime("Resume", sink),
        start_variance: f.int("StartVariance", sink).map(|(v, _)| v),
        finish_variance: f.int("FinishVariance", sink).map(|(v, _)| v),
        work_variance: variance_decimal(f, "WorkVariance", sink),
    }
}

/// R-226 : `WorkVariance` relu (minutes × 1000, `xsd:float` sans
/// exposant), décimal normalisé ; illisible → absent, signalé.
pub(crate) fn variance_decimal(f: &Fields<'_>, name: &str, sink: &mut Sink<'_>) -> Option<Decimal> {
    let (t, at) = f.scalar(name, sink)?;
    let parsed = crate::scalar::parse_decimal(t);
    if parsed.is_none() {
        f.invalid(name, at, t, sink);
    }
    parsed.map(|d| d.value.normalized())
}

/// Construit une tâche (sans ses liens). Rend aussi sa durée telle qu'écrite.
fn task(
    uid: TaskUid,
    f: &Fields<'_>,
    rec: &RecordedTask,
    options: &ProjectOptions,
    position: usize,
    sink: &mut Sink<'_>,
) -> (Task, Duration) {
    let name = f.raw("Name", sink).unwrap_or_default().to_owned();
    let mut t = Task::new(uid, name, options);
    t.guid = guid(f, "GUID", sink);
    t.is_null = f.bool("IsNull", sink).unwrap_or(false);
    identity(uid, f, position, sink);
    t.outline_level = outline_level(uid, f, sink);
    t.wbs = f.raw("WBS", sink).map(str::to_owned);
    t.task_type = code(f, "Type", codes::task_type, sink).unwrap_or(options.default_task_type);
    t.effort_driven = f
        .bool("EffortDriven", sink)
        .unwrap_or(options.new_tasks_effort_driven);
    let duration = task_duration(f, rec.duration, options, sink);
    t.duration = duration;
    t.work = baseline::work(f, "Work", sink).unwrap_or(Work::ZERO);
    t.milestone = f.bool("Milestone", sink).unwrap_or(false);
    t.constraint = constraint(f, sink);
    t.deadline = f.datetime("Deadline", sink);
    t.calendar = task_calendar(f, sink);
    t.ignore_resource_calendar = f.bool("IgnoreResourceCalendar", sink).unwrap_or(false);
    t.leveling_delay = leveling_delay(f, TASK_DELAY_ELAPSED, sink);
    t.priority = priority(f, sink);
    t.progress.percent_complete = percent(f, "PercentComplete", sink);
    t.progress.percent_work_complete = percent(f, "PercentWorkComplete", sink);
    t.progress.physical_percent_complete = percent(f, "PhysicalPercentComplete", sink);
    t.progress.actual_start = f.datetime("ActualStart", sink);
    t.progress.actual_finish = f.datetime("ActualFinish", sink);
    t.progress.actual_duration = non_negative_minutes(f, "ActualDuration", sink);
    t.progress.remaining_duration = non_negative_minutes(f, "RemainingDuration", sink);
    t.progress.actual_work = baseline::work(f, "ActualWork", sink).unwrap_or(Work::ZERO);
    t.progress.remaining_work = baseline::work(f, "RemainingWork", sink).unwrap_or(Work::ZERO);
    f.raw("Notes", sink).unwrap_or_default().clone_into(&mut t.notes);
    t.mode = mode(f, rec, duration, sink);
    t.active = f.bool("Active", sink).unwrap_or(true);
    t.recurring = f.bool("Recurring", sink).unwrap_or(false);
    t.created = f.datetime("CreateDate", sink);
    (t, duration)
}

/// Format par défaut du retard de nivellement d'une tâche, si
/// `LevelingDelayFormat` manque ou ne convient pas : jours écoulés (8),
/// format de toutes les tâches du corpus (hypothèse, R-198).
pub(crate) const TASK_DELAY_ELAPSED: bool = true;

/// R-198 : retard de nivellement (`LevelingDelay` en dixièmes de minute,
/// `LevelingDelayFormat` en codes S3) d'une tâche ou d'une affectation ;
/// absent ou 0 → `None` ; reste arrondi (R-001) → `mspdi.rounded` ;
/// négatif ou hors du domaine des durées (R-002) → `mspdi.invalid_value`,
/// `None`. Format absent, inconnu (`mspdi.unknown_code`), en pourcentage
/// ou nul (`mspdi.invalid_value`) : jours, écoulés selon
/// `default_elapsed`.
pub(crate) fn leveling_delay(
    f: &Fields<'_>,
    default_elapsed: bool,
    sink: &mut Sink<'_>,
) -> Option<LevelingDelay> {
    let (tenths, at) = f.int("LevelingDelay", sink)?;
    if tenths == 0 {
        return None;
    }
    let format = f.int("LevelingDelayFormat", sink);
    if let Some((code, code_at)) = format
        && codes::leveling_delay_format(code).is_none()
    {
        if codes::duration_format(code).is_none() {
            f.report(
                ImportCode::UnknownCode { code },
                "LevelingDelayFormat",
                code_at,
                None,
                sink,
            );
        } else {
            f.invalid("LevelingDelayFormat", code_at, &code.to_string(), sink);
        }
    }
    let (minutes, rounded) = tenths_to_minutes(tenths);
    if minutes.is_negative() || minutes.get() > Duration::MAX_MINUTES {
        f.invalid("LevelingDelay", at, &tenths.to_string(), sink);
        return None;
    }
    if rounded {
        f.report(
            ImportCode::Rounded,
            "LevelingDelay",
            at,
            Some(&tenths.to_string()),
            sink,
        );
    }
    delay_value(tenths, format.map(|(c, _)| c), default_elapsed)
}

/// R-198, sans diagnostic : retard de `tenths` dixièmes de minute au
/// format `format` (relecture d'un `Extras` hérité, `normalize`) ; nul
/// après l'arrondi → `None` ; négatif ou hors du domaine des durées →
/// `None`.
pub(crate) fn delay_value(tenths: i64, format: Option<i64>, default_elapsed: bool) -> Option<LevelingDelay> {
    let (minutes, _) = tenths_to_minutes(tenths);
    if minutes.get() == 0 {
        return None;
    }
    let (unit, elapsed) = format
        .and_then(codes::leveling_delay_format)
        .unwrap_or((DurationUnit::Day, default_elapsed));
    let basis = if elapsed {
        TimeBasis::Elapsed
    } else {
        TimeBasis::Working
    };
    Duration::new(minutes, unit, basis, false)
        .ok()
        .and_then(|_| LevelingDelay::new(minutes, unit, elapsed))
}

/// R-193 (import) : une feuille à travail fixe est toujours pilotée par
/// l'effort (S41) ; lue non pilotée (`EffortDriven` 0, ou défaut du projet),
/// elle le devient, avec `task.fixed_work_effort_driven`. Une récapitulative
/// n'est jamais pilotée : elle n'est pas touchée.
fn force_effort_driven(task: &mut Task, f: &Fields<'_>, sink: &mut Sink<'_>) {
    if task.task_type != TaskType::FixedWork || task.effort_driven || task.is_null {
        return;
    }
    task.effort_driven = true;
    let (at, value) = f
        .scalar("EffortDriven", sink)
        .map_or((f.at(), None), |(t, at)| (at, Some(t)));
    f.report(
        ImportCode::Core(Diagnostic::TaskFixedWorkEffortDriven { task: task.uid() }),
        "EffortDriven",
        at,
        value,
        sink,
    );
}

/// R-084 : `ID` comparé à la position (UID 0 : ID 0).
fn identity(uid: TaskUid, f: &Fields<'_>, position: usize, sink: &mut Sink<'_>) {
    let Some((id, at)) = f.int("ID", sink) else {
        return;
    };
    let expected = if uid == TaskUid::PROJECT_SUMMARY {
        0
    } else {
        i64::try_from(position).unwrap_or(i64::MAX).saturating_add(1)
    };
    if id != expected {
        f.report(ImportCode::Inconsistent, "ID", at, Some(&id.to_string()), sink);
    }
}

/// R-084 : `OutlineLevel` ; absent → 1 ; 0 réservé à l'UID 0 ; la
/// récapitulative du projet reçoit le premier niveau.
fn outline_level(uid: TaskUid, f: &Fields<'_>, sink: &mut Sink<'_>) -> OutlineLevel {
    let Some((v, at)) = f.int("OutlineLevel", sink) else {
        return OutlineLevel::TOP;
    };
    if uid == TaskUid::PROJECT_SUMMARY {
        if v != 0 {
            f.report(
                ImportCode::Inconsistent,
                "OutlineLevel",
                at,
                Some(&v.to_string()),
                sink,
            );
        }
        return OutlineLevel::TOP;
    }
    if let Some(l) = u16::try_from(v).ok().and_then(|l| OutlineLevel::new(l).ok()) {
        return l;
    }
    let code = if v == 0 {
        ImportCode::Inconsistent
    } else {
        ImportCode::InvalidValue
    };
    f.report(code, "OutlineLevel", at, Some(&v.to_string()), sink);
    OutlineLevel::TOP
}

/// R-071 : unité, base et estimation d'une durée de tâche.
fn duration_format(
    f: &Fields<'_>,
    options: &ProjectOptions,
    sink: &mut Sink<'_>,
) -> (DurationUnit, TimeBasis, bool) {
    let format = code(f, "DurationFormat", codes::duration_format, sink);
    let (unit, basis, estimated) = match format {
        Some(DurationCode::Time {
            unit,
            basis,
            estimated,
        }) => (unit, basis, estimated),
        Some(DurationCode::Null { estimated }) => {
            (options.default_duration_unit, TimeBasis::Working, estimated)
        }
        Some(DurationCode::Percent { .. }) => {
            if let Some((t, at)) = f.scalar("DurationFormat", sink) {
                f.invalid("DurationFormat", at, t, sink);
            }
            (DurationUnit::Day, TimeBasis::Working, false)
        }
        None => (options.default_duration_unit, TimeBasis::Working, false),
    };
    // `Estimated` présent l'emporte sur les codes « ? ».
    (unit, basis, f.bool("Estimated", sink).unwrap_or(estimated))
}

/// Durée d'une tâche (R-070, R-071) : négative ou hors bornes → 0, signalé.
fn task_duration(
    f: &Fields<'_>,
    minutes: Option<Minutes>,
    options: &ProjectOptions,
    sink: &mut Sink<'_>,
) -> Duration {
    let (unit, basis, estimated) = duration_format(f, options, sink);
    let zero = Duration::new(Minutes::ZERO, unit, basis, estimated).unwrap_or(Duration::zero(unit));
    let Some(m) = minutes else {
        return zero;
    };
    Duration::new(m, unit, basis, estimated).unwrap_or_else(|_| {
        if let Some((t, at)) = f.scalar("Duration", sink) {
            f.invalid("Duration", at, t, sink);
        }
        zero
    })
}

/// Contrainte (`ConstraintType`, `ConstraintDate`) ; date manquante d'une
/// contrainte datée → « le plus tôt possible », signalé.
fn constraint(f: &Fields<'_>, sink: &mut Sink<'_>) -> Constraint {
    let kind =
        code(f, "ConstraintType", codes::constraint_type, sink).unwrap_or(ConstraintKind::AsSoonAsPossible);
    let dated = |make: fn(Moment) -> Constraint, sink: &mut Sink<'_>| {
        if let Some(date) = f.datetime("ConstraintDate", sink) {
            return make(date);
        }
        if f.scalar("ConstraintDate", sink).is_none() {
            f.missing("ConstraintDate", sink);
        }
        Constraint::AsSoonAsPossible
    };
    match kind {
        ConstraintKind::AsSoonAsPossible => Constraint::AsSoonAsPossible,
        ConstraintKind::AsLateAsPossible => Constraint::AsLateAsPossible,
        ConstraintKind::MustStartOn => dated(|date| Constraint::MustStartOn { date }, sink),
        ConstraintKind::MustFinishOn => dated(|date| Constraint::MustFinishOn { date }, sink),
        ConstraintKind::StartNoEarlierThan => dated(|date| Constraint::StartNoEarlierThan { date }, sink),
        ConstraintKind::StartNoLaterThan => dated(|date| Constraint::StartNoLaterThan { date }, sink),
        ConstraintKind::FinishNoEarlierThan => dated(|date| Constraint::FinishNoEarlierThan { date }, sink),
        ConstraintKind::FinishNoLaterThan => dated(|date| Constraint::FinishNoLaterThan { date }, sink),
    }
}

/// Calendrier propre (`CalendarUID`, −1 = aucun).
fn task_calendar(f: &Fields<'_>, sink: &mut Sink<'_>) -> Option<CalendarUid> {
    let (v, at) = f.int("CalendarUID", sink)?;
    if v == -1 {
        return None;
    }
    let uid = u32::try_from(v).ok().map(CalendarUid::new);
    if uid.is_none() {
        f.invalid("CalendarUID", at, &v.to_string(), sink);
    }
    uid
}

/// Priorité, 0 à 1 000 ; sinon 500, signalé.
fn priority(f: &Fields<'_>, sink: &mut Sink<'_>) -> Priority {
    let Some((v, at)) = f.int("Priority", sink) else {
        return Priority::DEFAULT;
    };
    u16::try_from(v)
        .ok()
        .and_then(|p| Priority::new(p).ok())
        .unwrap_or_else(|| {
            f.invalid("Priority", at, &v.to_string(), sink);
            Priority::DEFAULT
        })
}

/// Pourcentage entier 0 à 100 → 1/10 000 ; sinon 0, signalé.
fn percent(f: &Fields<'_>, name: &str, sink: &mut Sink<'_>) -> Percent {
    let Some((v, at)) = f.int(name, sink) else {
        return Percent::ZERO;
    };
    Percent::from_percent(Decimal::integer(v)).unwrap_or_else(|_| {
        f.invalid(name, at, &v.to_string(), sink);
        Percent::ZERO
    })
}

/// Durée ≥ 0 (durée réelle ou restante) ; négative → 0, signalé.
fn non_negative_minutes(f: &Fields<'_>, name: &str, sink: &mut Sink<'_>) -> Minutes {
    match f.duration(name, sink) {
        Some((d, _)) if d.minutes >= 0 => Minutes::new(d.minutes),
        Some((d, at)) => {
            f.invalid(name, at, &d.minutes.to_string(), sink);
            Minutes::ZERO
        }
        None => Minutes::ZERO,
    }
}

/// R-085 : mode manuel, depuis `ManualStart`, `ManualFinish`,
/// `ManualDuration` ; si aucun des trois n'est présent (même vide), à
/// défaut `Start`, `Finish`, `Duration` (amendement R-129 : un élément
/// présent et vide vaut « non saisi », sans repli).
fn mode(f: &Fields<'_>, rec: &RecordedTask, duration: Duration, sink: &mut Sink<'_>) -> SchedulingMode {
    if f.bool("Manual", sink) != Some(true) {
        return SchedulingMode::Auto;
    }
    let explicit = f.has("ManualStart") || f.has("ManualFinish") || f.has("ManualDuration");
    let start = f
        .datetime("ManualStart", sink)
        .or(if explicit { None } else { rec.start });
    let finish = f
        .datetime("ManualFinish", sink)
        .or(if explicit { None } else { rec.finish });
    let manual_duration = match f.duration("ManualDuration", sink) {
        Some((d, at)) => {
            let made = Duration::new(
                Minutes::new(d.minutes),
                duration.unit(),
                duration.basis(),
                duration.is_estimated(),
            );
            if made.is_err() {
                f.invalid("ManualDuration", at, &d.minutes.to_string(), sink);
            }
            made.ok()
        }
        None => (!explicit && f.has("Duration")).then_some(duration),
    };
    SchedulingMode::Manual {
        start,
        finish,
        duration: manual_duration,
    }
}

/// Drapeau récapitulatif déduit de la hiérarchie, par ligne.
fn summary_flags(rows: &[Read<'_>]) -> Vec<bool> {
    let levels: Vec<Option<u16>> = rows
        .iter()
        .map(|r| (!r.task.is_null).then_some(r.task.outline_level.get()))
        .collect();
    let mut out = vec![false; rows.len()];
    let mut next_level: Option<u16> = None;
    for (slot, level) in out.iter_mut().zip(&levels).rev() {
        if let Some(l) = level {
            *slot = next_level.is_some_and(|n| n > *l);
            next_level = Some(*l);
        }
    }
    out
}

/// R-072 à R-074 : liens d'une tâche, dans l'ordre du document.
fn links(
    successor: TaskUid,
    children: &[Element<'_>],
    known: &BTreeSet<TaskUid>,
    options: &ProjectOptions,
    sink: &mut Sink<'_>,
) -> Vec<Link> {
    let mut out: Vec<Link> = Vec::new();
    // Prédécesseurs déjà gardés : détection des doublons en O(log n), coût
    // linéaire (§ 7.5) même pour une tâche à des centaines de milliers de
    // prédécesseurs.
    let mut kept: BTreeSet<TaskUid> = BTreeSet::new();
    for e in children.iter().filter(|c| c.name == "PredecessorLink") {
        let mut f = Fields::new(&e.children, LINK, Some(Subject::Task(successor)), e.at);
        let Some((p, p_at)) = f.int("PredecessorUID", sink) else {
            if f.scalar("PredecessorUID", sink).is_none() {
                f.missing("PredecessorUID", sink);
            }
            continue;
        };
        let Ok(p) = u32::try_from(p).map(TaskUid::new) else {
            f.invalid("PredecessorUID", p_at, &p.to_string(), sink);
            continue;
        };
        f.set_subject(Some(Subject::Link {
            predecessor: p,
            successor,
        }));
        let ignored = |reason, name: &str, f: &Fields<'_>, sink: &mut Sink<'_>| {
            f.report(ImportCode::LinkIgnored { reason }, name, e.at, None, sink);
        };
        if f.bool("CrossProject", sink) == Some(true) {
            ignored(LinkIgnoredReason::CrossProject, "CrossProject", &f, sink);
            continue;
        }
        let kind = match f.int("Type", sink) {
            None => LinkType::FinishToStart,
            Some((v, _)) => {
                if let Some(k) = codes::link_type(v) {
                    k
                } else {
                    ignored(LinkIgnoredReason::UnknownType, "Type", &f, sink);
                    continue;
                }
            }
        };
        if p == successor {
            ignored(LinkIgnoredReason::SelfLink, "PredecessorUID", &f, sink);
            continue;
        }
        if !known.contains(&p) {
            f.report(
                ImportCode::DanglingReference,
                "PredecessorUID",
                p_at,
                Some(&p.get().to_string()),
                sink,
            );
            continue;
        }
        if !kept.insert(p) {
            f.report(
                ImportCode::Duplicate,
                "PredecessorUID",
                p_at,
                Some(&p.get().to_string()),
                sink,
            );
            continue;
        }
        out.push(Link {
            predecessor: p,
            kind,
            lag: lag(&f, options, sink),
        });
    }
    out
}

/// R-072, R-073 : décalage (`LinkLag` en dixièmes, `LagFormat`).
fn lag(f: &Fields<'_>, options: &ProjectOptions, sink: &mut Sink<'_>) -> Lag {
    let (tenths, at) = f.int("LinkLag", sink).unwrap_or((0, f.at()));
    let format = code(f, "LagFormat", codes::duration_format, sink);
    let (basis, value) = match format {
        Some(DurationCode::Percent { basis, .. }) => {
            f.report(
                ImportCode::Unsupported {
                    what: "percent_lag_encoding".to_owned(),
                },
                "LagFormat",
                at,
                Some(&tenths.to_string()),
                sink,
            );
            let Some(percent) = percent_lag_from_link_lag(tenths).and_then(|v| SignedPercent::new(v).ok())
            else {
                f.invalid("LinkLag", at, &tenths.to_string(), sink);
                return Lag::ZERO;
            };
            (basis, LagValue::Percent { percent })
        }
        other => {
            let (unit, basis) = match other {
                Some(DurationCode::Time { unit, basis, .. }) => (unit, basis),
                _ => (
                    if matches!(other, Some(DurationCode::Null { .. })) {
                        options.default_duration_unit
                    } else {
                        DurationUnit::Day
                    },
                    TimeBasis::Working,
                ),
            };
            let (minutes, rounded) = tenths_to_minutes(tenths);
            if rounded {
                f.report(
                    ImportCode::Rounded,
                    "LinkLag",
                    at,
                    Some(&tenths.to_string()),
                    sink,
                );
            }
            (basis, LagValue::Time { minutes, unit })
        }
    };
    Lag::new(basis, value).unwrap_or_else(|_| {
        f.invalid("LinkLag", at, &tenths.to_string(), sink);
        Lag::ZERO
    })
}
