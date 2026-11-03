//! Valeurs calculées écrites (R-122, ADR-0010 D12) : depuis le `Schedule`
//! de PlannerCraft, depuis le planning relu (`RecordedSchedule`, projet non
//! modifié), ou omises (MS Project les calcule).

use std::collections::{BTreeMap, BTreeSet};

use plannercraft_core::progress::ProgressState;

use plannercraft_core::calendar::{CalendarSet, CompiledCalendar, TimeScale, standard};
use plannercraft_core::schedule::measure_calendar_in;
use plannercraft_core::{
    Arena, AssignmentSchedule, AssignmentUid, BaselineNumber, Decimal, Duration, Exclusion, Minutes, Moment,
    Progress, Project, ResourceUid, Schedule, Task, TaskOutcome, TaskRole, TaskSchedule, TaskUid, TimeBasis,
    Work,
};

use super::WriteDiagnostic;
use crate::extras::ORIGIN;
use crate::recorded::{RecordedSchedule, RecordedTask, Tenths};
use crate::scalar::parse_int;

/// Source des valeurs calculées écrites.
#[derive(Clone, Copy, Debug)]
pub enum Computed<'a> {
    /// Valeurs du moteur (`schedule(&project)`).
    Schedule(&'a Schedule),
    /// Valeurs relues, réécrites telles quelles (projet non modifié depuis
    /// la lecture).
    Recorded(&'a RecordedSchedule),
    /// Aucune valeur calculée : MS Project les calcule à l'ouverture.
    Omit,
}

/// Valeur entière d'une charge `<LevelingDelay>n</LevelingDelay>`.
pub(crate) fn leveling_delay(payload: &str) -> Option<i64> {
    let start = payload.find('>')?.checked_add(1)?;
    let end = payload.rfind("</")?;
    parse_int(payload.get(start..end)?)
}

/// R-122 : source des valeurs calculées écrites. Amendement J6 (R-215) :
/// le moteur calcule l'avancement, il n'y a plus de manque ; les valeurs
/// du moteur sont toujours écrites (`recorded` est gardé pour la
/// stabilité de l'API, R-162) et aucun diagnostic n'est rendu.
#[must_use]
pub fn choose_computed<'a>(
    _project: &Project,
    schedule: &'a Schedule,
    _recorded: Option<&'a RecordedSchedule>,
) -> (Computed<'a>, Vec<WriteDiagnostic>) {
    (Computed::Schedule(schedule), Vec::new())
}

/// Valeurs calculées d'une tâche, prêtes à écrire.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TaskValues {
    pub(crate) start: Option<Moment>,
    pub(crate) finish: Option<Moment>,
    pub(crate) early_start: Option<Moment>,
    pub(crate) early_finish: Option<Moment>,
    pub(crate) late_start: Option<Moment>,
    pub(crate) late_finish: Option<Moment>,
    pub(crate) total_slack: Option<i64>,
    pub(crate) free_slack: Option<i64>,
    pub(crate) start_slack: Option<i64>,
    pub(crate) finish_slack: Option<i64>,
    pub(crate) critical: Option<bool>,
    /// Durée calculée d'une récapitulative automatique, en minutes.
    pub(crate) duration: Option<i64>,
    /// R-199 : travail calculé d'une récapitulative et de l'UID 0 (R-184),
    /// avec `Computed::Schedule` seulement.
    pub(crate) work: Option<Work>,
    /// R-199 : durée calculée d'une feuille automatique affectée (R-173),
    /// avec `Computed::Schedule` seulement (voir [`assigned_leaf_duration`]).
    pub(crate) leaf_duration: Option<Duration>,
    /// R-215 : avancement écrit avec `Computed::Schedule` (feuille : % dérivé,
    /// durée réelle effective, début réel implicite compris ; récapitulative
    /// et UID 0 : agrégats de R-210) ; `None` : celui du modèle.
    pub(crate) progress: Option<Progress>,
    /// `Stop` (R-215) ; relu avec `Computed::Recorded`.
    pub(crate) stop: Option<Moment>,
    /// `Resume` (R-214, R-215) ; relu avec `Computed::Recorded`.
    pub(crate) resume: Option<Moment>,
    /// R-226 : `StartVariance`, `FinishVariance` (minutes × 1000) et
    /// `WorkVariance` (minutes × 1000, décimal).
    pub(crate) start_variance: Option<i64>,
    pub(crate) finish_variance: Option<i64>,
    pub(crate) work_variance: Option<Decimal>,
}

/// Valeurs calculées d'une affectation, prêtes à écrire (R-198, R-199).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AssignmentValues {
    pub(crate) start: Option<Moment>,
    pub(crate) finish: Option<Moment>,
    /// `ActualStart`, `ActualFinish`, `Stop`, `Resume` (R-215).
    pub(crate) actual_start: Option<Moment>,
    pub(crate) actual_finish: Option<Moment>,
    pub(crate) stop: Option<Moment>,
    pub(crate) resume: Option<Moment>,
    /// R-226 : écarts ; `StartVariance`/`FinishVariance` relus seulement
    /// (unité non documentée par pj15).
    pub(crate) start_variance: Option<i64>,
    pub(crate) finish_variance: Option<i64>,
    pub(crate) work_variance: Option<Decimal>,
    /// R-199 : `TimephasedData` de type 1 à jeter (placement changé).
    pub(crate) drop_timephased: bool,
    /// R-215 : `TimephasedData` de réel (types 2-3) à jeter (travail réel
    /// changé).
    pub(crate) drop_actuals: bool,
}

/// Valeurs calculées d'une ressource, prêtes à écrire (R-198, R-199).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ResourceValues {
    pub(crate) work: Option<Work>,
    pub(crate) overallocated: Option<bool>,
    /// R-226 : `WorkVariance` (minutes × 1000).
    pub(crate) work_variance: Option<Decimal>,
}

/// Valeurs calculées de tout le projet.
#[derive(Debug, Default)]
pub(crate) struct Values {
    pub(crate) tasks: BTreeMap<TaskUid, TaskValues>,
    /// Affectations : relues, ou placées par le moteur (R-199).
    pub(crate) assignments: BTreeMap<AssignmentUid, AssignmentValues>,
    /// Ressources : relues, ou charges du moteur (R-199).
    pub(crate) resources: BTreeMap<ResourceUid, ResourceValues>,
    pub(crate) project_start: Option<Moment>,
    pub(crate) project_finish: Option<Moment>,
}

/// Dixièmes de minute d'une marge (minutes × 10) ; débordement : omise.
fn tenths(m: Minutes) -> Option<i64> {
    m.get().checked_mul(10)
}

/// Échelles de marge (R-048, R-066, R-067, R-113) : calendrier de mesure
/// du moteur (`plannercraft_core::schedule::measure_calendar_in` : calendrier
/// propre utilisable, sinon calendrier du projet — sa base s'il est dérivé —,
/// sinon le Standard intégré) ; temps civil pour une feuille à durée
/// écoulée.
struct Scales<'p> {
    project: &'p Project,
    set: CalendarSet,
    fallback: Option<CompiledCalendar>,
}

impl<'p> Scales<'p> {
    fn new(project: &'p Project) -> Self {
        let (set, _) = CalendarSet::compile(project);
        let usable = set
            .get(project.calendar)
            .is_ok_and(CompiledCalendar::has_working_time);
        let fallback = if usable {
            None
        } else {
            let std = standard(project.calendar, "Standard".to_owned());
            Arena::try_from(vec![std])
                .ok()
                .and_then(|a| CompiledCalendar::compile(&a, project.calendar).ok())
        };
        Self {
            project,
            set,
            fallback,
        }
    }

    /// Échelle de mesure de `task` ; `summary` : récapitulative (sa marge se
    /// mesure sur un calendrier, jamais en temps écoulé, comme le moteur).
    fn of(&self, task: &Task, summary: bool) -> Option<TimeScale<'_>> {
        if !summary && task.duration.basis() == TimeBasis::Elapsed {
            return Some(TimeScale::Elapsed);
        }
        let cal = match measure_calendar_in(self.project, &self.set, task) {
            Some(uid) => self.set.get(uid).ok(),
            None => self.fallback.as_ref(),
        };
        cal.map(TimeScale::Working)
    }
}

/// `StartSlack`, `FinishSlack` (dixièmes) sur l'échelle de marge, gardés
/// seulement si leur minimum redonne la marge totale du moteur (sinon
/// l'échelle supposée n'est pas la sienne : omis).
fn start_finish_slack(scale: Option<TimeScale<'_>>, s: &TaskSchedule) -> (Option<i64>, Option<i64>) {
    let Some(scale) = scale else {
        return (None, None);
    };
    let ss = scale.between(s.early.start, s.late.start).ok();
    let fs = scale.between(s.early.finish, s.late.finish).ok();
    match (ss, fs) {
        (Some(a), Some(b)) if a.min(b) == s.total_slack => (tenths(a), tenths(b)),
        _ => (None, None),
    }
}

fn from_task_schedule(s: &TaskSchedule, scale: Option<TimeScale<'_>>, summary: bool) -> TaskValues {
    let (start_slack, finish_slack) = start_finish_slack(scale, s);
    TaskValues {
        start: Some(s.start),
        finish: Some(s.finish),
        early_start: Some(s.early.start),
        early_finish: Some(s.early.finish),
        late_start: Some(s.late.start),
        late_finish: Some(s.late.finish),
        total_slack: tenths(s.total_slack),
        free_slack: tenths(s.free_slack),
        start_slack,
        finish_slack,
        critical: Some(s.critical),
        duration: (summary && !s.manual).then_some(s.duration.get()),
        work: summary.then_some(s.work),
        ..TaskValues::default()
    }
}

/// R-215 : avancement écrit d'une tâche planifiée par le moteur, `Stop` et
/// `Resume` ; rien sans avancement calculé pour une feuille (tâche non
/// commencée) : le modèle est alors écrit. Feuille : % achevé dérivé
/// (R-200), durée réelle effective, début réel (implicite compris, R-200) et
/// fin réelle du moteur ; feuille en cours : reprise saisie seulement (la
/// reprise relue est une reprise saisie, R-214 ; J6-F1) ; les restes en sont dérivés à
/// l'écriture (R-127). Récapitulative et UID 0 : %, % travail, début et fin
/// réels, durée réelle et reste de R-210, travail réel (Σ du périmètre de
/// R-184) et travail restant = travail calculé − travail réel (relecture
/// finale J6, J6-F2) ; sans feuille commencée : % de R-055, aucun réel
/// (dates, durée, travail, % travail), reste de durée = durée calculée,
/// travail restant = travail calculé.
pub(crate) fn progress_values(
    t: &Task,
    s: &TaskSchedule,
) -> (Option<Progress>, Option<Moment>, Option<Moment>) {
    let summary = s.role == TaskRole::Summary;
    let Some(ps) = s.progress else {
        if !summary {
            return (None, None, None);
        }
        let mut p = t.progress;
        p.percent_complete = s.percent_complete;
        p.percent_work_complete = plannercraft_core::Percent::ZERO;
        p.actual_start = None;
        p.actual_finish = None;
        p.actual_duration = Minutes::ZERO;
        p.actual_work = Work::ZERO;
        p.remaining_duration = s.duration.max(Minutes::ZERO);
        p.remaining_work = s.work;
        p.resume = None;
        return (Some(p), None, None);
    };
    let mut p = t.progress;
    p.percent_complete = ps.percent_complete;
    p.actual_start = Some(ps.actual_start);
    p.actual_finish = ps.actual_finish;
    p.actual_duration = ps.actual_duration;
    if summary {
        p.percent_work_complete = ps.percent_work_complete;
        p.remaining_duration = ps.remaining_duration;
        p.actual_work = ps.actual_work;
        let rest = s
            .work
            .minutes()
            .get()
            .saturating_sub(ps.actual_work.minutes().get())
            .max(0);
        p.remaining_work = Work::new(Minutes::new(rest)).unwrap_or(Work::ZERO);
        p.resume = None;
    } else if ps.state != ProgressState::Started {
        p.resume = None;
    }
    // Relecture finale J6 (J6-F1) : `Resume` d'une feuille en cours est relu
    // comme reprise saisie (R-214), plancher du moteur ; la reprise calculée
    // (tirée d'un lien, d'une porte…) n'est donc jamais écrite : seule la
    // reprise saisie l'est, telle quelle, et l'aller-retour garde le modèle.
    let resume = if !summary && ps.state == ProgressState::Started {
        p.resume
    } else {
        Some(ps.resume)
    };
    (Some(p), Some(ps.stop), resume)
}

/// Applique [`progress_values`] aux valeurs d'une tâche.
fn with_progress(values: &mut TaskValues, t: &Task, s: &TaskSchedule) {
    (values.progress, values.stop, values.resume) = progress_values(t, s);
}

/// R-199 : durée écrite d'une feuille automatique placée par ses
/// affectations de travail : la durée calculée du moteur (mesure de l'union
/// des intervalles travaillés, R-173), la même que celle que la session
/// reporte dans le modèle (`plannercraft_core::schedule::assigned_leaf_duration`,
/// règle et exclusions : durée fixe, S41 ; unités nulles, R-181). `None` :
/// la durée du modèle est écrite.
pub(crate) fn assigned_leaf_duration(
    task: &Task,
    s: &TaskSchedule,
    placed: &BTreeSet<TaskUid>,
) -> Option<Duration> {
    plannercraft_core::schedule::assigned_leaf_duration(task, s, placed)
}

/// Valeurs depuis le `Schedule` (R-122).
pub(crate) fn from_schedule(project: &Project, schedule: &Schedule) -> Values {
    let scales = Scales::new(project);
    let placed = plannercraft_core::schedule::placed_by_assignments(project, schedule);
    let mut tasks = BTreeMap::new();
    for t in &project.tasks {
        let values = match schedule.get(t.uid()) {
            Some(TaskOutcome::Scheduled(s)) => {
                let summary = s.role == TaskRole::Summary;
                let mut values = TaskValues {
                    leaf_duration: assigned_leaf_duration(t, s, &placed),
                    ..from_task_schedule(s, scales.of(t, summary), summary)
                };
                with_progress(&mut values, t, s);
                values
            }
            Some(TaskOutcome::Excluded {
                reason: Exclusion::Inactive,
                display: Some(span),
            }) => TaskValues {
                start: Some(span.start),
                finish: Some(span.finish),
                ..TaskValues::default()
            },
            _ => TaskValues::default(),
        };
        tasks.insert(t.uid(), values);
    }
    if let (Some(summary), Some(s)) = (&project.summary_task, &schedule.project_summary) {
        let mut values = from_task_schedule(s, scales.of(summary, true), true);
        with_progress(&mut values, summary, s);
        tasks.insert(TaskUid::PROJECT_SUMMARY, values);
    }
    // R-199 : `Start`/`Finish` de chaque affectation placée ; répartition
    // gardée seulement si son placement est celui du moteur.
    let assignments = project
        .assignments
        .iter()
        .map(|a| {
            let placed = schedule.assignments.get(&a.uid());
            let values = AssignmentValues {
                start: placed.map(|p| p.span.start),
                finish: placed.map(|p| p.span.finish),
                drop_timephased: super::timephased::stale(a, placed),
                drop_actuals: super::timephased::actuals_stale(a),
                ..assignment_actuals(schedule, placed)
            };
            (a.uid(), values)
        })
        .collect();
    // R-199 : travail et surutilisation de chaque ressource chargée
    // (`PeakUnits` jamais écrit, O31).
    let resources = schedule
        .resources
        .iter()
        .map(|(uid, load)| {
            (
                *uid,
                ResourceValues {
                    work: Some(load.work),
                    overallocated: Some(load.overallocated),
                    work_variance: None,
                },
            )
        })
        .collect();
    let mut values = Values {
        tasks,
        assignments,
        resources,
        project_start: Some(schedule.project_start),
        project_finish: Some(schedule.project_finish),
    };
    variances(&mut values, project, schedule, &scales.set);
    values
}

/// R-226 : écart × 1000 (minutes × 1000 du schéma) ; débordement : omis.
fn thousand(m: Minutes) -> Option<i64> {
    m.get().checked_mul(1_000)
}

/// R-226 : écarts du moteur à la ligne de base 0 (R-224, R-225), au
/// format du schéma (minutes × 1000) : tâches planifiées et UID 0
/// (`StartVariance`, `FinishVariance` entiers, `WorkVariance` décimal),
/// ressources (UID 0 exclu) et affectations placées (`WorkVariance`
/// seulement : l'unité des écarts de dates d'affectation n'est pas
/// documentée, ADR-0014 D7). Sans ligne de base : début et fin 0, travail =
/// travail (O43).
fn variances(values: &mut Values, project: &Project, schedule: &Schedule, set: &CalendarSet) {
    let Ok(number) = BaselineNumber::new(0) else {
        return;
    };
    let v = plannercraft_core::baseline::variance(project, schedule, set, number);
    let work = |m: Minutes| thousand(m).map(Decimal::integer);
    let root = schedule.project_summary.is_some().then_some(&v.project);
    for tv in v.tasks.iter().chain(root) {
        if let Some(t) = values.tasks.get_mut(&tv.uid) {
            t.start_variance = thousand(tv.variance.start);
            t.finish_variance = thousand(tv.variance.finish);
            t.work_variance = work(tv.variance.work);
        }
    }
    for (uid, m) in &v.resources {
        values.resources.entry(*uid).or_default().work_variance = work(*m);
    }
    for (uid, m) in &v.assignments {
        if let Some(a) = values.assignments.get_mut(uid) {
            a.work_variance = work(*m);
        }
    }
}

/// R-226 : `WorkVariance` écrit avec au moins deux décimales, comme
/// MS Project (`10800000.00`).
pub(crate) fn variance_text(d: Decimal) -> String {
    let n = d.normalized();
    match 2_u8.checked_sub(n.scale()).filter(|k| *k > 0) {
        Some(k) => 10_i64
            .checked_pow(u32::from(k))
            .and_then(|f| n.mantissa().checked_mul(f))
            .and_then(|m| Decimal::new(m, 2).ok())
            .map_or_else(|| n.to_string(), |x| x.to_string()),
        None => n.to_string(),
    }
}

/// R-215 : réels et coupure d'une affectation placée d'une tâche commencée
/// (partie réelle calculée, R-209) : `ActualStart` = début de son
/// placement, `ActualFinish` = fin si la tâche est achevée, `Stop` = fin de
/// sa partie réelle, `Resume` = début de son reste (achevée : `Stop`) ;
/// rien sinon.
fn assignment_actuals(schedule: &Schedule, placed: Option<&AssignmentSchedule>) -> AssignmentValues {
    let Some((p, actual)) = placed.and_then(|p| Some((p, p.actual?))) else {
        return AssignmentValues::default();
    };
    let complete = schedule
        .scheduled(p.task)
        .and_then(|t| t.progress)
        .is_some_and(|ps| ps.state == ProgressState::Complete);
    let stop = actual.finish;
    AssignmentValues {
        actual_start: Some(p.span.start),
        actual_finish: complete.then_some(p.span.finish),
        stop: Some(stop),
        resume: Some(if complete {
            stop
        } else {
            p.remaining_start.unwrap_or(stop)
        }),
        ..AssignmentValues::default()
    }
}

/// R-199 : image du projet par une écriture avec `Computed::Schedule` (à
/// composer avec [`crate::normalize`] pour l'aller-retour) : les champs du
/// modèle que l'écrivain remplace par des valeurs du moteur. Travail d'une
/// récapitulative planifiée et de l'UID 0 ← R-184 ; durée d'une feuille
/// automatique affectée ← durée calculée (R-173, voir
/// [`assigned_leaf_duration`]) ; `TimephasedData` d'une affectation dont le
/// placement a changé retirées. Un `Schedule` d'un autre projet ne change
/// rien (`mspdi.write.schedule_mismatch`, aucune valeur écrite).
#[must_use]
pub fn schedule_image(project: &Project, schedule: &Schedule) -> Project {
    let mut p = project.clone();
    if !matches(project, schedule) {
        return p;
    }
    let placed = plannercraft_core::schedule::placed_by_assignments(project, schedule);
    let uids: Vec<TaskUid> = p.tasks.iter().map(Task::uid).collect();
    for uid in uids {
        let (Some(t), Some(TaskOutcome::Scheduled(s))) = (p.tasks.get_mut(uid), schedule.get(uid)) else {
            continue;
        };
        if s.role == TaskRole::Summary {
            t.work = s.work;
        } else if let Some(d) = assigned_leaf_duration(t, s, &placed) {
            t.duration = d;
        }
        if let (Some(progress), _, _) = progress_values(t, s) {
            t.progress = progress;
        }
    }
    if let (Some(t), Some(s)) = (&mut p.summary_task, &schedule.project_summary) {
        t.work = s.work;
        if let (Some(progress), _, _) = progress_values(t, s) {
            t.progress = progress;
        }
    }
    let auids: Vec<AssignmentUid> = p
        .assignments
        .iter()
        .map(plannercraft_core::Assignment::uid)
        .collect();
    for uid in auids {
        let Some(a) = p.assignments.get_mut(uid) else {
            continue;
        };
        if super::timephased::stale(a, schedule.assignments.get(&uid)) {
            a.extras.0.retain(|e| {
                !(e.origin == ORIGIN && super::timephased::is_scheduled_work(&e.key, &e.payload))
            });
        }
        if super::timephased::actuals_stale(a) {
            a.extras
                .0
                .retain(|e| !(e.origin == ORIGIN && super::timephased::is_actual(&e.key, &e.payload)));
        }
    }
    p
}

/// Valeurs relues, telles quelles (R-122).
pub(crate) fn from_recorded(recorded: &RecordedSchedule) -> Values {
    let tasks = recorded
        .tasks
        .iter()
        .map(|(uid, r)| (*uid, recorded_values(r)))
        .collect();
    let assignments = recorded
        .assignments
        .iter()
        .map(|(uid, r)| {
            (
                *uid,
                AssignmentValues {
                    start: r.start,
                    finish: r.finish,
                    actual_start: r.actual_start,
                    actual_finish: r.actual_finish,
                    stop: r.stop,
                    resume: r.resume,
                    start_variance: r.start_variance,
                    finish_variance: r.finish_variance,
                    work_variance: r.work_variance,
                    drop_timephased: false,
                    drop_actuals: false,
                },
            )
        })
        .collect();
    let resources = recorded
        .resources
        .iter()
        .map(|(uid, r)| {
            (
                *uid,
                ResourceValues {
                    work: r.work,
                    overallocated: r.overallocated,
                    work_variance: r.work_variance,
                },
            )
        })
        .collect();
    Values {
        tasks,
        assignments,
        resources,
        project_start: recorded.project_start,
        project_finish: recorded.project_finish,
    }
}

fn recorded_values(r: &RecordedTask) -> TaskValues {
    let t = |v: Option<Tenths>| v.map(|Tenths(x)| x);
    TaskValues {
        start: r.start,
        finish: r.finish,
        early_start: r.early_start,
        early_finish: r.early_finish,
        late_start: r.late_start,
        late_finish: r.late_finish,
        total_slack: t(r.total_slack),
        free_slack: t(r.free_slack),
        start_slack: t(r.start_slack),
        finish_slack: t(r.finish_slack),
        critical: r.critical,
        duration: r.duration.map(Minutes::get),
        stop: r.stop,
        resume: r.resume,
        start_variance: r.start_variance,
        finish_variance: r.finish_variance,
        work_variance: r.work_variance,
        ..TaskValues::default()
    }
}

/// R-214 : reprise saisie écrite sans valeurs calculées : celle d'une
/// feuille commencée non achevée seulement (la seule relue dans le modèle).
pub(crate) fn model_resume(t: &Task, summary: bool) -> Option<Moment> {
    t.progress
        .resume
        .filter(|_| crate::task::keeps_resume(t, summary))
}

/// Le `Schedule` couvre exactement les tâches du projet (UID 0 exclu).
pub(crate) fn matches(project: &Project, schedule: &Schedule) -> bool {
    schedule.tasks.len() == project.tasks.len()
        && project
            .tasks
            .iter()
            .all(|t| schedule.tasks.contains_key(&t.uid()))
}
