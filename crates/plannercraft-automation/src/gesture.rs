//! Gestes du Gantt traduits en lots de commandes (J7, ADR-0015 D5 ; N12 de
//! `docs/ui-needs.md` ; règles R-240 à R-243, lot L8).
//!
//! L'interface ne produit qu'un **écart en jours entiers**
//! (`TimeAxis::days_for_dx`), jamais une date (règle 3) : ce module le
//! traduit en lot ordinaire (`task.update`, `progress.update`, `link.add`),
//! appliqué au lâcher comme toute édition (une entrée d'annulation).
//! Pendant le glisser, [`preview`] donne le début et la fin calculés sur le
//! calendrier seul, sans recalcul du planning.
//!
//! - R-240 : déplacer = début planifié + n × 1 440 min (jours calendaires,
//!   heure gardée) ⇒ `task.update { start }` (R-158 pose la contrainte).
//! - R-241 : redimensionner = fin planifiée + n jours ; durée = temps ouvré
//!   [début, cible] sur l'échelle de mesure de la tâche (R-113), écoulée en
//!   minutes civiles ⇒ `task.update { duration }`.
//! - R-242 : avancement = début + max(n, 0) jours ; % = ⌊travail × 100 / D⌋
//!   × 100 ⇒ `progress.update { percent_complete }`.
//! - R-243 : lien FS sans décalage ⇒ `link.add` ; paire déjà liée (dans un
//!   sens ou dans l'autre) ⇒ [`GestureError::AlreadyLinked`], aucun lot
//!   (R-260 c, seconde passe UX).

use plannercraft_core::calendar::{CalendarSet, CompiledCalendar, TimeScale, builtin};
use plannercraft_core::progress::facts;
use plannercraft_core::schedule::{TaskRole, TaskSchedule, measure_calendar_in};
use plannercraft_core::{Arena, Minutes, Moment, Percent, ProgressState, Task, TaskUid, TimeBasis};

use crate::command::{
    Batch, Command, DurationJson, LinkSpec, LinkTypeJson, Patch, PercentJson, ProgressUpdate, TaskRef,
    TaskUpdate,
};
use crate::error::codes;
use crate::session::Session;

/// Minutes d'un jour calendaire.
const MINUTES_PER_DAY: i64 = 1_440;

/// Geste sur une barre.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GestureKind {
    /// Déplacer la barre (R-240).
    Move,
    /// Redimensionner par sa fin (R-241).
    Resize,
    /// Glisser l'avancement (R-242).
    Progress,
}

/// Raison pour laquelle une barre ne se glisse pas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotDraggable {
    /// Récapitulative (seul le lien est permis).
    Summary,
    /// Tâche commencée (arbitrage du 2026-10-20 : le réel ne se déplace pas
    /// d'un geste).
    Started,
    /// Jalon (pas de durée ni d'avancement à glisser).
    Milestone,
    /// Tâche exclue du calcul.
    Excluded,
    /// Ligne vide.
    Blank,
}

/// Geste refusé.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GestureError {
    /// Tâche inconnue.
    #[error("tâche {0} introuvable")]
    UnknownTask(TaskUid),
    /// Barre qui ne se glisse pas ainsi.
    #[error("la tâche {0} ne se glisse pas ainsi ({1:?})")]
    NotDraggable(TaskUid, NotDraggable),
    /// Lien d'une tâche vers elle-même.
    #[error("lien d'une tâche vers elle-même")]
    SameTask,
    /// Écart hors du domaine des dates (ou conversion du calendrier
    /// impossible).
    #[error("écart hors limites")]
    Overflow,
    /// Les deux tâches sont déjà liées, dans un sens ou dans l'autre
    /// (R-260 c) : aucun lot (l'interface en fait une information).
    #[error("ces tâches sont déjà liées")]
    AlreadyLinked,
}

impl GestureError {
    /// Code stable (`task.not_found`, `gesture.not_draggable`,
    /// `link.self`, `limit.exceeded`, `link.duplicate`).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnknownTask(_) => codes::TASK_NOT_FOUND,
            Self::NotDraggable(..) => codes::GESTURE_NOT_DRAGGABLE,
            Self::SameTask => codes::LINK_SELF,
            Self::Overflow => codes::LIMIT_EXCEEDED,
            Self::AlreadyLinked => codes::LINK_DUPLICATE,
        }
    }
}

/// Aperçu d'un geste : dates calculées après application (calendrier seul).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preview {
    /// Début.
    pub start: Moment,
    /// Fin.
    pub finish: Moment,
    /// Durée (redimensionnement).
    pub duration: Option<Minutes>,
    /// % achevé (glisser l'avancement).
    pub percent: Option<Percent>,
}

/// Tâche du modèle et son planning, après les contrôles communs : tâche
/// connue, pas une ligne vide, planifiée.
fn scheduled(s: &Session, uid: TaskUid) -> Result<(&Task, &TaskSchedule), GestureError> {
    let task = s.project().tasks.get(uid).ok_or(GestureError::UnknownTask(uid))?;
    if task.is_null {
        return Err(GestureError::NotDraggable(uid, NotDraggable::Blank));
    }
    let t = s
        .schedule()
        .scheduled(uid)
        .ok_or(GestureError::NotDraggable(uid, NotDraggable::Excluded))?;
    Ok((task, t))
}

/// Feuille planifiée non commencée (R-240, R-241).
fn unstarted_leaf(s: &Session, uid: TaskUid) -> Result<(&Task, &TaskSchedule), GestureError> {
    let (task, t) = scheduled(s, uid)?;
    if t.role == TaskRole::Summary {
        return Err(GestureError::NotDraggable(uid, NotDraggable::Summary));
    }
    if facts(task).state != ProgressState::NotStarted {
        return Err(GestureError::NotDraggable(uid, NotDraggable::Started));
    }
    Ok((task, t))
}

/// `n` jours calendaires (1 440 min chacun) ajoutés à `t`.
fn plus_days(t: Moment, days: i64) -> Result<Moment, GestureError> {
    let minutes = days.checked_mul(MINUTES_PER_DAY).ok_or(GestureError::Overflow)?;
    t.checked_add(Minutes::new(minutes))
        .map_err(|_| GestureError::Overflow)
}

/// Lot d'une seule opération, sans libellé.
fn one(op: Command) -> Batch {
    Batch {
        format_version: Batch::FORMAT_VERSION,
        lang: None,
        ops: vec![op],
        label: None,
    }
}

/// Calendrier de mesure (R-113), emprunté ou compilé (repli Standard
/// intégré de R-066 quand le calendrier du projet est inutilisable).
enum Measure<'a> {
    /// Calendrier compilé de l'ensemble.
    Set(&'a CompiledCalendar),
    /// Standard intégré compilé seul.
    Builtin(CompiledCalendar),
}

impl Measure<'_> {
    /// Échelle de mesure de `task` : temps civil pour une durée écoulée.
    fn scale(&self, task: &Task) -> TimeScale<'_> {
        if task.duration.basis() == TimeBasis::Elapsed {
            return TimeScale::Elapsed;
        }
        match self {
            Self::Set(c) => TimeScale::Working(c),
            Self::Builtin(c) => TimeScale::Working(c),
        }
    }
}

/// Calendrier qui mesure `task` (N13, R-113).
fn measure<'a>(s: &Session, set: &'a CalendarSet, task: &Task) -> Result<Measure<'a>, GestureError> {
    let project = s.project();
    if let Some(c) = measure_calendar_in(project, set, task).and_then(|uid| set.get(uid).ok()) {
        return Ok(Measure::Set(c));
    }
    let standard = builtin::standard(project.calendar, "Standard".to_owned());
    let arena = Arena::try_from(vec![standard]).map_err(|_| GestureError::Overflow)?;
    CompiledCalendar::compile(&arena, project.calendar)
        .map(Measure::Builtin)
        .map_err(|_| GestureError::Overflow)
}

/// Déplace la barre de `uid` de `days` jours calendaires (R-240) ;
/// `days` = 0 : aucun lot.
///
/// # Errors
///
/// [`GestureError`] : tâche inconnue, ligne vide, exclue, récapitulative,
/// commencée ; écart hors du domaine des dates.
pub fn bar_move(s: &Session, uid: TaskUid, days: i64) -> Result<Option<Batch>, GestureError> {
    let (_, t) = unstarted_leaf(s, uid)?;
    if days == 0 {
        return Ok(None);
    }
    let start = plus_days(t.start, days)?;
    Ok(Some(one(Command::TaskUpdate(TaskUpdate {
        start: Patch::Set(start),
        ..task_update(uid)
    }))))
}

/// `task.update` sans champ.
fn task_update(uid: TaskUid) -> TaskUpdate {
    TaskUpdate {
        uid: TaskRef::Uid(uid),
        name: Patch::Keep,
        duration: Patch::Keep,
        milestone: Patch::Keep,
        notes: Patch::Keep,
        start: Patch::Keep,
        finish: Patch::Keep,
        active: Patch::Keep,
        task_type: Patch::Keep,
        effort_driven: Patch::Keep,
        work: Patch::Keep,
        mode: Patch::Keep,
        custom: std::collections::BTreeMap::new(),
    }
}

/// Durée de R-241 : temps de `start` à `target` sur l'échelle de mesure,
/// nulle si `target <= start`.
fn resized(scale: TimeScale<'_>, start: Moment, target: Moment) -> Result<Minutes, GestureError> {
    if target <= start {
        return Ok(Minutes::ZERO);
    }
    scale.between(start, target).map_err(|_| GestureError::Overflow)
}

/// Redimensionne la barre de `uid` par sa fin, de `days` jours
/// calendaires (R-241) ; `days` = 0 : aucun lot.
///
/// # Errors
///
/// [`GestureError`] : comme [`bar_move`], plus jalon.
pub fn bar_resize(
    s: &Session,
    set: &CalendarSet,
    uid: TaskUid,
    days: i64,
) -> Result<Option<Batch>, GestureError> {
    let (task, t) = unstarted_leaf(s, uid)?;
    if task.milestone {
        return Err(GestureError::NotDraggable(uid, NotDraggable::Milestone));
    }
    if days == 0 {
        return Ok(None);
    }
    let target = plus_days(t.finish, days)?;
    let cal = measure(s, set, task)?;
    let minutes = resized(cal.scale(task), t.start, target)?;
    let duration = DurationJson {
        minutes: Some(minutes.get()),
        unit: Some(task.duration.unit().into()),
        text: None,
        elapsed: Some(task.duration.basis() == TimeBasis::Elapsed),
        estimated: None,
    };
    Ok(Some(one(Command::TaskUpdate(TaskUpdate {
        duration: Patch::Set(duration),
        ..task_update(uid)
    }))))
}

/// % de R-242 en 1/10 000 : ⌊travail × 100 / D⌋ × 100, borné ; le point
/// au-delà de la fin vaut 100 %.
fn dragged_percent(
    scale: TimeScale<'_>,
    t: &TaskSchedule,
    days_from_start: i64,
) -> Result<Percent, GestureError> {
    let point = plus_days(t.start, days_from_start.max(0))?;
    if point >= t.finish {
        return Ok(Percent::FULL);
    }
    let work = scale
        .between(t.start, point)
        .map_err(|_| GestureError::Overflow)?;
    let d = i128::from(t.duration.get());
    if d <= 0 {
        return Ok(Percent::ZERO);
    }
    let whole = i128::from(work.get())
        .saturating_mul(100)
        .checked_div(d)
        .unwrap_or(0);
    let value = whole.saturating_mul(100).clamp(0, 10_000);
    let value = i64::try_from(value).map_err(|_| GestureError::Overflow)?;
    Percent::new(value).map_err(|_| GestureError::Overflow)
}

/// Feuille planifiée de durée non nulle (R-242).
fn progress_leaf(s: &Session, uid: TaskUid) -> Result<(&Task, &TaskSchedule), GestureError> {
    let (task, t) = scheduled(s, uid)?;
    if t.role == TaskRole::Summary {
        return Err(GestureError::NotDraggable(uid, NotDraggable::Summary));
    }
    if task.milestone || t.duration.get() <= 0 {
        return Err(GestureError::NotDraggable(uid, NotDraggable::Milestone));
    }
    Ok((task, t))
}

/// % que R-201 stockerait après la saisie de `p` sur `task` (durée D) :
/// réel = `percent_of(D, p)` borné à D − 1, puis % dérivé du réel arrondi
/// (R-200) ; 0 ⇒ 0 ; D = 0 ou 100 % ⇒ 100 %. Un geste dont ce % égale le %
/// courant ne change rien : aucun lot (relecture finale J7 : T 3 j, 33 %
/// envoyé, 32,99 % stocké ; refaire le même geste ajoutait une entrée
/// d'annulation sans effet).
pub(crate) fn stored_percent(task: &Task, p: Percent) -> Percent {
    if p == Percent::ZERO {
        return p;
    }
    let d = task.duration.minutes().get();
    if d <= 0 || p == Percent::FULL {
        return Percent::FULL;
    }
    let share = i128::from(d).saturating_mul(i128::from(p.get()));
    let actual = plannercraft_core::round_div(share, 10_000)
        .unwrap_or(0)
        .min(d.saturating_sub(1))
        .max(0);
    plannercraft_core::round_div(i128::from(actual).saturating_mul(10_000), i128::from(d))
        .map(|v| v.clamp(0, Percent::FULL.get().saturating_sub(1)))
        .and_then(|v| Percent::new(v).ok())
        .unwrap_or(p)
}

/// Pose l'avancement de `uid` au point `days_from_start` jours après son
/// début (R-242) ; % que R-201 stockerait égal au % courant : aucun lot.
///
/// # Errors
///
/// [`GestureError`] : tâche inconnue, ligne vide, exclue, récapitulative,
/// jalon (durée nulle) ; écart hors du domaine des dates.
pub fn progress_drag(
    s: &Session,
    set: &CalendarSet,
    uid: TaskUid,
    days_from_start: i64,
) -> Result<Option<Batch>, GestureError> {
    let (task, t) = progress_leaf(s, uid)?;
    let cal = measure(s, set, task)?;
    let percent = dragged_percent(cal.scale(task), t, days_from_start)?;
    if percent == task.progress.percent_complete
        || stored_percent(task, percent) == task.progress.percent_complete
    {
        return Ok(None);
    }
    Ok(Some(one(Command::ProgressUpdate(ProgressUpdate {
        task: TaskRef::Uid(uid),
        percent_complete: Some(PercentJson {
            value: Some(percent.get()),
            text: None,
        }),
        actual_start: None,
        actual_finish: None,
        actual_duration: None,
        remaining_duration: None,
        percent_work_complete: None,
        actual_work: None,
        physical_percent_complete: None,
    }))))
}

/// Lien FS sans décalage de `predecessor` vers `successor` (R-243). Une
/// paire déjà liée, dans un sens ou dans l'autre, ne produit aucun lot
/// (R-260 c) ; les autres refus du registre (cycle, R-059) viennent à
/// l'application du lot.
///
/// # Errors
///
/// [`GestureError`] : [`GestureError::SameTask`], tâche inconnue, ligne
/// vide, tâche exclue, [`GestureError::AlreadyLinked`].
pub fn link_drag(s: &Session, predecessor: TaskUid, successor: TaskUid) -> Result<Batch, GestureError> {
    if predecessor == successor {
        return Err(GestureError::SameTask);
    }
    let (pred_task, _) = scheduled(s, predecessor)?;
    let (succ_task, _) = scheduled(s, successor)?;
    let linked = succ_task
        .predecessors
        .iter()
        .any(|l| l.predecessor == predecessor)
        || pred_task.predecessors.iter().any(|l| l.predecessor == successor);
    if linked {
        return Err(GestureError::AlreadyLinked);
    }
    Ok(one(Command::LinkAdd(LinkSpec {
        pred: TaskRef::Uid(predecessor),
        succ: TaskRef::Uid(successor),
        kind: Some(LinkTypeJson::Fs),
        lag: None,
    })))
}

/// Aperçu du geste `kind` de `days` jours sur `uid`, sans recalcul : sur
/// l'échelle de mesure de la tâche, début recalé comme une contrainte de
/// début (R-090, passe avant) pour un déplacement, fin replacée pour un
/// redimensionnement, dates gardées pour l'avancement. Exact pour une
/// feuille automatique sans lien, contrainte ni ressource (P49) ; sinon
/// indicatif (le moteur décide au lâcher).
///
/// # Errors
///
/// [`GestureError`] : les refus du geste correspondant.
pub fn preview(
    s: &Session,
    set: &CalendarSet,
    uid: TaskUid,
    kind: GestureKind,
    days: i64,
) -> Result<Preview, GestureError> {
    match kind {
        GestureKind::Move => {
            let (task, t) = unstarted_leaf(s, uid)?;
            let raw = plus_days(t.start, days)?;
            let cal = measure(s, set, task)?;
            let d = t.duration.max(Minutes::ZERO);
            let span = if matches!(task.mode, plannercraft_core::SchedulingMode::Manual { .. }) {
                let finish = cal.scale(task).add(raw, d).map_err(|_| GestureError::Overflow)?;
                plannercraft_core::Span { start: raw, finish }
            } else {
                cal.scale(task)
                    .forward_span(raw, d)
                    .map_err(|_| GestureError::Overflow)?
            };
            Ok(Preview {
                start: span.start,
                finish: span.finish,
                duration: None,
                percent: None,
            })
        }
        GestureKind::Resize => {
            let (task, t) = unstarted_leaf(s, uid)?;
            if task.milestone {
                return Err(GestureError::NotDraggable(uid, NotDraggable::Milestone));
            }
            let target = plus_days(t.finish, days)?;
            let cal = measure(s, set, task)?;
            let scale = cal.scale(task);
            let d = resized(scale, t.start, target)?;
            let span = scale
                .forward_span(t.start, d)
                .map_err(|_| GestureError::Overflow)?;
            Ok(Preview {
                start: span.start,
                finish: span.finish,
                duration: Some(d),
                percent: None,
            })
        }
        GestureKind::Progress => {
            let (task, t) = progress_leaf(s, uid)?;
            let cal = measure(s, set, task)?;
            let percent = dragged_percent(cal.scale(task), t, days)?;
            Ok(Preview {
                start: t.start,
                finish: t.finish,
                duration: None,
                percent: Some(percent),
            })
        }
    }
}
