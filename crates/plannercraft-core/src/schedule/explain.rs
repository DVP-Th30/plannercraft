//! Explication d'une date (J6, ADR-0014 D8 ; règles R-230 à R-233).
//! Contrat J6 (lot L0) ; règles remplies par le lot L7.
//!
//! [`explain`] lit un `Schedule` déjà calculé : aucun recalcul, aucune
//! trace gardée par le moteur. La passe avant ne pose qu'une valeur de plus,
//! `TaskSchedule::driver_bound` (borne brute du pilote, R-230). Les étapes
//! citent le pilote, la borne brute, le recalage sur le calendrier, le
//! retard de nivellement, l'avancement (réel, reprise) et la durée ; la
//! chaîne remonte les pilotes, bornée en profondeur (règle 10). Aucune date
//! n'est rédigée hors du moteur (règle 3) : les résumés FR/EN sont des
//! gabarits remplis par ces champs (R-233).
//!
//! Lot L7 : étapes de R-230 et R-231, chaîne de R-232 ; les résumés
//! (R-233) sont rédigés par `plannercraft-automation` à partir de ces seuls
//! champs. Seul calcul fait ici : la borne d'un lien entrant d'une feuille
//! en cours, pour nommer la cause de sa reprise (le calendrier des
//! décalages ouvrés est compilé à la demande, jamais sur le chemin du
//! moteur).

use std::cmp::Reverse;
use std::collections::BTreeSet;

use serde::Serialize;

use super::types::{
    AssignmentRole, AssignmentSchedule, Driver, EffectiveCalendar, LateDriver, ProgressSchedule, Schedule,
    TaskRole, TaskSchedule,
};
use crate::calendar::{Calendar, CalendarKind, CompiledCalendar, Span, TimeScale};
use crate::diagnostic::{Diagnostic, Severity, Subject};
use crate::duration::{LagValue, Minutes, TimeBasis, percent_of};
use crate::ids::{AssignmentUid, CalendarUid, TaskUid};
use crate::model::{
    ConstraintKind, LevelingDelay, Link, LinkType, Project, ScheduleAnchor, SchedulingMode, Task,
};
use crate::progress::ProgressState;
use crate::time::Moment;

/// Profondeur de chaîne par défaut (R-232).
pub const DEFAULT_DEPTH: u8 = 5;
/// Profondeur de chaîne maximale (R-232, règle 10).
pub const MAX_DEPTH: u8 = 50;

/// Date expliquée.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// Début planifié (R-230).
    #[default]
    Start,
    /// Fin planifiée (R-231).
    Finish,
}

/// Étape d'une explication, dans l'ordre de R-230 et R-231.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum Step {
    /// Pilote du début au plus tôt et sa borne brute
    /// (`TaskSchedule::driver_bound`).
    Driver {
        /// Pilote (R-044).
        driver: Driver,
        /// Borne brute ; `None` pour `Manual`, `Rollup`, `Actual`.
        raw: Option<Moment>,
    },
    /// Recalage de la borne brute sur le calendrier de placement (R-024,
    /// R-030).
    Snap {
        /// Borne brute.
        raw: Moment,
        /// Instant recalé.
        snapped: Moment,
        /// Calendrier de placement ; `None` en temps écoulé.
        calendar: Option<CalendarUid>,
    },
    /// Retard de nivellement enregistré (R-178).
    LevelingDelay {
        /// Retard.
        delay: LevelingDelay,
        /// Début au plus tôt non retardé.
        from: Moment,
        /// Début retardé.
        to: Moment,
    },
    /// Dates réelles (R-203, R-204).
    Actual {
        /// Début réel.
        start: Moment,
        /// Fin réelle (tâche achevée).
        finish: Option<Moment>,
    },
    /// Reprise du reste d'une tâche en cours (R-204, R-205).
    Resume {
        /// Fin de la partie réelle.
        stop: Moment,
        /// Reprise.
        resume: Moment,
        /// Cause de la reprise.
        cause: ResumeCause,
    },
    /// Durée placée (R-231).
    Duration {
        /// Minutes de la durée (ou du reste d'une tâche en cours).
        minutes: Minutes,
        /// Calendrier de placement ; `None` en temps écoulé.
        calendar: Option<CalendarUid>,
        /// Feuille à parts : affectation la plus longue (R-173).
        longest: Option<AssignmentUid>,
    },
    /// Contrainte de date qui ne lie pas (R-230).
    ConstraintNotBinding {
        /// Type de la contrainte.
        kind: ConstraintKind,
        /// Date de la contrainte.
        date: Moment,
    },
    /// Dates de repli (R-069) : code stable de la cause.
    Degraded {
        /// Code stable.
        code: &'static str,
    },
}

/// Cause de la reprise d'une tâche en cours (R-204, R-205).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResumeCause {
    /// Premier instant ouvré après la fin de la partie réelle.
    NextWorking,
    /// Début du projet (relecture finale J6, J6-EXPLAIN-1).
    ProjectStart,
    /// Borne de la récapitulative parente (sa porte : liens FS/SS
    /// entrants, SNET, porte de ses ancêtres), relecture finale J6.
    SummaryGate {
        /// Récapitulative parente.
        summary: TaskUid,
    },
    /// Lien depuis un prédécesseur (propre, ou FF/SF vers une
    /// récapitulative ancêtre déplié sur la feuille, R-058).
    Link {
        /// Prédécesseur.
        predecessor: TaskUid,
        /// Type du lien.
        link: LinkType,
    },
    /// Contrainte de début (SNET, MSO).
    Constraint(ConstraintKind),
    /// Reprise saisie (`Progress::resume`).
    Entered,
}

/// Côté au plus tard d'une tâche (R-231).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct LateState {
    /// Dates au plus tard.
    pub late: Span,
    /// Cause de la fin au plus tard (R-047).
    pub driver: LateDriver,
    /// Marge totale (R-048).
    pub total_slack: Minutes,
    /// Marge libre (R-049).
    pub free_slack: Minutes,
    /// Critique (R-050).
    pub critical: bool,
}

/// Maillon de la chaîne des pilotes (R-232).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ChainLink {
    /// Tâche.
    pub uid: TaskUid,
    /// Début planifié.
    pub start: Moment,
    /// Fin planifiée.
    pub finish: Moment,
    /// Pilote de son début.
    pub driver: Driver,
    /// Borne brute du pilote.
    pub raw: Option<Moment>,
}

/// Explication d'une date.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Explanation {
    /// Tâche expliquée.
    pub task: TaskUid,
    /// Date expliquée.
    pub side: Side,
    /// Valeur de la date (celle du `Schedule`).
    pub date: Moment,
    /// Étapes, dans l'ordre.
    pub steps: Vec<Step>,
    /// Côté au plus tard.
    pub late: LateState,
    /// Chaîne des pilotes, de la tâche vers l'amont (R-232).
    pub chain: Vec<ChainLink>,
    /// Chaîne coupée à la profondeur demandée.
    pub truncated: bool,
    /// L'explication repose sur une hypothèse non vérifiée (R-206 : marge
    /// d'une tâche en cours).
    pub hypothesis: bool,
}

/// Explication impossible.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ExplainError {
    /// Tâche inexistante (`task.not_found`).
    #[error("aucune tâche d'UID {0}")]
    UnknownTask(TaskUid),
    /// Tâche exclue du calcul : aucune date à expliquer
    /// (`explain.not_scheduled`).
    #[error("la tâche {0} n'est pas planifiée")]
    NotScheduled(TaskUid),
    /// Profondeur hors de `1..=50` (`param.invalid`, R-232).
    #[error("profondeur {0} hors de 1 à 50")]
    DepthOutOfRange(u8),
}

/// Explique la date `side` de la tâche `uid` (UID 0 : récapitulative du
/// projet) dans le planning `s` de `p`, chaîne bornée à `depth` maillons
/// (1 à [`MAX_DEPTH`]).
///
/// Étapes (R-230, R-231) : pilote et borne brute ; recalage si la borne
/// brute diffère du début au plus tôt ; retard de nivellement si le début
/// planifié diffère du début au plus tôt ; dates réelles, puis reprise d'une
/// feuille en cours ; contrainte de date qui ne lie pas ; repli ; côté fin,
/// la durée placée (le reste d'une feuille en cours ; rien de plus pour une
/// feuille achevée, dont la fin est réelle). Chaîne (R-232) : pilotes
/// remontés depuis la tâche, sans répétition.
///
/// # Errors
///
/// [`ExplainError`].
pub fn explain(
    p: &Project,
    s: &Schedule,
    uid: TaskUid,
    side: Side,
    depth: u8,
) -> Result<Explanation, ExplainError> {
    let task = if uid == TaskUid::PROJECT_SUMMARY {
        p.summary_task.as_ref()
    } else {
        Some(p.tasks.get(uid).ok_or(ExplainError::UnknownTask(uid))?)
    };
    if !(1..=MAX_DEPTH).contains(&depth) {
        return Err(ExplainError::DepthOutOfRange(depth));
    }
    let ts = *s.scheduled(uid).ok_or(ExplainError::NotScheduled(uid))?;
    let ctx = Ctx { p, s, uid, task, ts };
    let mut steps = ctx.start_steps();
    if side == Side::Finish && !ctx.completed() {
        steps.push(ctx.duration_step());
    }
    let (chain, truncated) = ctx.chain(depth);
    Ok(Explanation {
        task: uid,
        side,
        date: match side {
            Side::Start => ts.start,
            Side::Finish => ts.finish,
        },
        steps,
        late: LateState {
            late: ts.late,
            driver: ts.late_driver,
            total_slack: ts.total_slack,
            free_slack: ts.free_slack,
            critical: ts.critical,
        },
        chain,
        truncated,
        hypothesis: ctx.hypothesis(),
    })
}

/// Contexte d'une explication : lectures seules du modèle et du planning.
struct Ctx<'a> {
    p: &'a Project,
    s: &'a Schedule,
    uid: TaskUid,
    /// Tâche du modèle ; `None` pour l'UID 0 sans tâche relue.
    task: Option<&'a Task>,
    ts: TaskSchedule,
}

impl Ctx<'_> {
    /// Feuille commencée : son avancement calculé.
    fn leaf_progress(&self) -> Option<ProgressSchedule> {
        self.ts
            .progress
            .filter(|g| self.ts.role == TaskRole::Leaf && g.state != ProgressState::NotStarted)
    }

    /// Feuille achevée (R-203, R-208).
    fn completed(&self) -> bool {
        self.leaf_progress()
            .is_some_and(|g| g.state == ProgressState::Complete)
    }

    /// Feuille automatique en cours (R-204).
    fn running(&self) -> Option<ProgressSchedule> {
        self.leaf_progress()
            .filter(|g| g.state == ProgressState::Started && !self.ts.manual)
    }

    /// R-230 : étapes du début, dans l'ordre.
    fn start_steps(&self) -> Vec<Step> {
        let ts = &self.ts;
        let mut steps = vec![Step::Driver {
            driver: ts.driver,
            raw: ts.driver_bound,
        }];
        if let Some(raw) = ts.driver_bound
            && raw != ts.early.start
        {
            steps.push(Step::Snap {
                raw,
                snapped: ts.early.start,
                calendar: self.placement_calendar(Placement::Start),
            });
        }
        if let Some(delay) = self.leveling_delay() {
            steps.push(Step::LevelingDelay {
                delay,
                from: ts.early.start,
                to: ts.start,
            });
        }
        let mut cause = None;
        if let Some(g) = self.leaf_progress() {
            steps.push(Step::Actual {
                start: g.actual_start,
                finish: g.actual_finish,
            });
        }
        if let Some(g) = self.running() {
            let c = self.resume_cause(g);
            cause = Some(c);
            steps.push(Step::Resume {
                stop: g.stop,
                resume: g.resume,
                cause: c,
            });
        }
        if let Some((kind, date)) = self.constraint_not_binding(cause) {
            steps.push(Step::ConstraintNotBinding { kind, date });
        }
        if ts.degraded {
            steps.push(Step::Degraded {
                code: self.degraded_code(),
            });
        }
        steps
    }

    /// R-231 : durée placée ; reste d'une feuille en cours ; feuille à
    /// parts : affectation de travail qui finit la dernière.
    fn duration_step(&self) -> Step {
        let minutes = self.running().map_or(self.ts.duration, |g| g.remaining_duration);
        Step::Duration {
            minutes,
            calendar: self.placement_calendar(Placement::Finish),
            longest: self.last_part().map(|(u, _)| u),
        }
    }

    /// R-178 : retard de nivellement d'une feuille automatique qui décale
    /// son début planifié (depuis le début seulement).
    fn leveling_delay(&self) -> Option<LevelingDelay> {
        let ts = &self.ts;
        if ts.role != TaskRole::Leaf
            || ts.manual
            || ts.start == ts.early.start
            || !matches!(self.p.anchor, ScheduleAnchor::FromStart(_))
        {
            return None;
        }
        self.task?.leveling_delay.filter(|l| l.minutes > Minutes::ZERO)
    }

    /// R-230 : contrainte datée d'une feuille qui ne pilote ni son début ni
    /// sa reprise.
    fn constraint_not_binding(&self, cause: Option<ResumeCause>) -> Option<(ConstraintKind, Moment)> {
        if self.ts.role != TaskRole::Leaf {
            return None;
        }
        let c = self.task?.constraint;
        let date = c.date()?;
        let kind = c.kind();
        let drives = self.ts.driver == Driver::Constraint { kind }
            || self.ts.late_driver == LateDriver::Constraint { kind }
            || cause == Some(ResumeCause::Constraint(kind));
        (!drives).then_some((kind, date))
    }

    /// Code stable d'un repli (R-069) : premier diagnostic d'erreur ou
    /// d'avertissement de la tâche, sinon `task.unschedulable` (conversion
    /// impossible en amont).
    fn degraded_code(&self) -> &'static str {
        self.s
            .diagnostics
            .iter()
            .find(|d| d.subject() == Subject::Task(self.uid) && d.severity() <= Severity::Warning)
            .map_or("task.unschedulable", Diagnostic::code)
    }

    /// Affectations de travail de la tâche (R-173), dans l'ordre des UID.
    fn parts(&self) -> impl Iterator<Item = (AssignmentUid, &AssignmentSchedule)> + '_ {
        let uid = self.uid;
        self.s
            .assignments
            .iter()
            .filter(move |(_, a)| a.task == uid && a.role == AssignmentRole::Work)
            .map(|(u, a)| (*u, a))
    }

    /// Affectation de travail qui finit la dernière (la plus longue à
    /// égalité, puis le plus petit UID) : elle fixe la fin (R-173).
    fn last_part(&self) -> Option<(AssignmentUid, &AssignmentSchedule)> {
        self.parts()
            .max_by_key(|(u, a)| (a.span.finish, a.duration, Reverse(*u)))
    }

    /// Calendrier de placement cité (R-030, R-171, R-173) ; `None` en temps
    /// écoulé ou sur le calendrier Standard intégré de repli (R-066). Feuille
    /// à parts : calendrier effectif de l'affectation qui commence la
    /// première (`Start`) ou finit la dernière (`Finish`) ; une intersection
    /// cite le calendrier de la tâche.
    fn placement_calendar(&self, at: Placement) -> Option<CalendarUid> {
        if self.ts.role == TaskRole::Leaf {
            if self
                .task
                .is_some_and(|t| t.duration.basis() == TimeBasis::Elapsed)
            {
                return None;
            }
            let part = match at {
                Placement::Start => self.parts().min_by_key(|(u, a)| (a.span.start, *u)),
                Placement::Finish => self.last_part(),
            };
            if let Some((_, a)) = part {
                return match a.calendar {
                    EffectiveCalendar::Task(c)
                    | EffectiveCalendar::Resource(c)
                    | EffectiveCalendar::Project(c)
                    | EffectiveCalendar::Intersection { task: c, .. } => Some(c),
                    EffectiveCalendar::Elapsed => None,
                };
            }
        }
        self.measure_calendar()
    }

    /// Calendrier propre de la tâche s'il est utilisable (aucun
    /// `task.calendar_fallback`), sinon celui du projet (R-041, R-113).
    fn measure_calendar(&self) -> Option<CalendarUid> {
        self.measure_calendar_of(self.uid, self.task)
    }

    /// [`Ctx::measure_calendar`] de la tâche `uid`.
    fn measure_calendar_of(&self, uid: TaskUid, task: Option<&Task>) -> Option<CalendarUid> {
        let own = task
            .filter(|t| uid != TaskUid::PROJECT_SUMMARY && !t.is_null)
            .and_then(|t| t.calendar)
            .filter(|&c| {
                !self.s.diagnostics.iter().any(|d| {
                    matches!(d, Diagnostic::TaskCalendarFallback { task, calendar }
                        if *task == uid && *calendar == c)
                })
            });
        own.or_else(|| project_calendar(self.p, self.s))
    }

    /// R-204, R-205 : cause de la reprise d'une feuille en cours. La
    /// reprise est le premier instant de placement du reste depuis
    /// `max(Stop, reprise saisie, bornes, date SNET/MSO)` (bornes et date
    /// seulement si le fractionnement est permis) ; les bornes sont toutes
    /// celles du réseau (R-205) : début du projet, porte de la récapitulative
    /// parente, liens FF/SF vers une récapitulative ancêtre dépliés sur la
    /// feuille (R-058), liens propres. La cause est le terme qui atteint ce
    /// maximum, dans l'ordre du moteur (reprise saisie, début du projet,
    /// porte, liens dépliés de l'ancêtre le plus haut au plus bas, liens
    /// propres dans l'ordre de stockage, contrainte), une égalité gardant le
    /// premier ; un terme après la reprise n'est jamais retenu. Une borne FF
    /// ou SF qui atteint la fin du reste vaut la reprise. Aucun terme :
    /// `NextWorking`. Hypothèse (bornes relues sur les dates planifiées).
    fn resume_cause(&self, g: ProgressSchedule) -> ResumeCause {
        let (stop, resume) = (g.stop, g.resume);
        let mut best = (stop, ResumeCause::NextWorking);
        let mut take = |x: Moment, cause: ResumeCause| {
            if x > best.0 && x <= resume {
                best = (x, cause);
            }
        };
        let Some(task) = self.task else {
            return best.1;
        };
        if let Some(r) = task.progress.resume {
            take(r, ResumeCause::Entered);
        }
        let ScheduleAnchor::FromStart(project_start) = self.p.anchor else {
            return best.1;
        };
        if !self.p.options.progress.split_in_progress {
            return best.1;
        }
        take(project_start, ResumeCause::ProjectStart);
        let ancestors = self.ancestors();
        if let Some(&parent) = ancestors.first()
            && let Some(gate) = self.gate_value(&ancestors, project_start)
        {
            take(gate, ResumeCause::SummaryGate { summary: parent });
        }
        let finish = self.ts.finish;
        let finish_link = |link: &Link, x: Moment, take: &mut dyn FnMut(Moment, ResumeCause)| {
            if x >= finish {
                take(
                    resume,
                    ResumeCause::Link {
                        predecessor: link.predecessor,
                        link: link.kind,
                    },
                );
            }
        };
        // R-058 : liens FF/SF vers une récapitulative ancêtre, dépliés sur
        // la feuille (ordre de stockage : l'ancêtre le plus haut d'abord).
        for &a in ancestors.iter().rev() {
            let Some(at) = self.p.tasks.get(a) else { continue };
            let mut scale = LagScale::Unresolved;
            let cal = self.measure_calendar_of(a, Some(at));
            for link in at
                .predecessors
                .iter()
                .filter(|l| matches!(l.kind, LinkType::FinishToFinish | LinkType::StartToFinish))
            {
                if let Some(x) = self
                    .s
                    .scheduled(link.predecessor)
                    .and_then(|ps| self.link_bound(link, ps, cal, &mut scale))
                {
                    finish_link(link, x, &mut take);
                }
            }
        }
        let mut lag_scale = LagScale::Unresolved;
        let cal = self.measure_calendar();
        for link in &task.predecessors {
            let Some(ps) = self.s.scheduled(link.predecessor) else {
                continue;
            };
            let Some(x) = self.link_bound(link, ps, cal, &mut lag_scale) else {
                continue;
            };
            match link.kind {
                LinkType::FinishToStart | LinkType::StartToStart => take(
                    x,
                    ResumeCause::Link {
                        predecessor: link.predecessor,
                        link: link.kind,
                    },
                ),
                LinkType::FinishToFinish | LinkType::StartToFinish => finish_link(link, x, &mut take),
            }
        }
        let c = task.constraint;
        if let (Some(date), kind @ (ConstraintKind::StartNoEarlierThan | ConstraintKind::MustStartOn)) =
            (c.date(), c.kind())
        {
            take(date, ResumeCause::Constraint(kind));
        }
        best.1
    }

    /// Récapitulatives ancêtres de la tâche, de la parente à la plus haute
    /// (hors UID 0) ; vide pour une tâche de premier niveau.
    fn ancestors(&self) -> Vec<TaskUid> {
        let tasks = &self.p.tasks;
        let Some(pos) = tasks.position(self.uid) else {
            return Vec::new();
        };
        let Some(mut level) = tasks.at(pos).map(|t| t.outline_level) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for t in tasks.iter().take(pos).rev() {
            if t.is_null || t.outline_level >= level {
                continue;
            }
            out.push(t.uid());
            level = t.outline_level;
        }
        out
    }

    /// Porte de la récapitulative `ancestors[0]` (R-056, R-103) relue sur
    /// les dates planifiées : max du début du projet et, pour chaque
    /// ancêtre, de ses bornes FS/SS entrantes (lecture de
    /// [`Ctx::gate_cause`]) et de sa date SNET (automatique). `None` sans
    /// ancêtre.
    fn gate_value(&self, ancestors: &[TaskUid], project_start: Moment) -> Option<Moment> {
        ancestors.first()?;
        let mut gate = project_start;
        for &a in ancestors {
            let Some(at) = self.p.tasks.get(a) else { continue };
            if at.is_null {
                continue;
            }
            let mut scale = LagScale::Unresolved;
            let cal = self.measure_calendar_of(a, Some(at));
            for link in at
                .predecessors
                .iter()
                .filter(|l| matches!(l.kind, LinkType::FinishToStart | LinkType::StartToStart))
            {
                if let Some(x) = self
                    .s
                    .scheduled(link.predecessor)
                    .and_then(|ps| self.link_bound(link, ps, cal, &mut scale))
                {
                    gate = gate.max(x);
                }
            }
            let dated_manual = matches!(
                at.mode,
                SchedulingMode::Manual { start, finish, .. } if start.is_some() || finish.is_some()
            );
            if !dated_manual
                && at.constraint.kind() == ConstraintKind::StartNoEarlierThan
                && let Some(date) = at.constraint.date()
            {
                gate = gate.max(date);
            }
        }
        Some(gate)
    }

    /// Borne `g(ancre)` d'un lien entrant (R-037 à R-043) relue sur les
    /// dates planifiées du prédécesseur ; décalage ouvré sur le calendrier
    /// du successeur `cal` (R-041), compilé une fois dans `lag_scale`.
    /// `None` si elle ne se calcule pas.
    fn link_bound(
        &self,
        link: &Link,
        ps: &TaskSchedule,
        cal: Option<CalendarUid>,
        lag_scale: &mut LagScale,
    ) -> Option<Moment> {
        let anchor = match link.kind {
            LinkType::FinishToStart | LinkType::FinishToFinish => ps.finish,
            LinkType::StartToStart | LinkType::StartToFinish => ps.start,
        };
        let lag = match link.lag.value {
            LagValue::Time { minutes, .. } => minutes,
            LagValue::Percent { percent } => percent_of(ps.duration, percent).ok()?,
        };
        if lag == Minutes::ZERO {
            return Some(anchor);
        }
        match link.lag.basis {
            TimeBasis::Elapsed => TimeScale::Elapsed.add(anchor, lag).ok(),
            TimeBasis::Working => {
                if matches!(lag_scale, LagScale::Unresolved) {
                    *lag_scale = cal
                        .and_then(|c| CompiledCalendar::compile(&self.p.calendars, c).ok())
                        .map_or(LagScale::Missing, LagScale::Compiled);
                }
                match lag_scale {
                    LagScale::Compiled(c) => TimeScale::Working(c).add(anchor, lag).ok(),
                    LagScale::Unresolved | LagScale::Missing => None,
                }
            }
        }
    }

    /// R-232 : chaîne des pilotes depuis la tâche, au plus `depth`
    /// maillons, sans répétition (règle 10) ; `true` si elle est coupée.
    fn chain(&self, depth: u8) -> (Vec<ChainLink>, bool) {
        let mut seen = BTreeSet::from([self.uid]);
        let mut out = Vec::new();
        let (mut uid, mut driver, mut raw) = (self.uid, self.ts.driver, self.ts.driver_bound);
        // Borne de porte par laquelle on arrive sur une récapitulative.
        let mut gate: Option<Moment> = None;
        loop {
            let next = gate
                .and_then(|r| self.gate_cause(uid, r))
                .or_else(|| self.upstream(uid, driver));
            let Some(next) = next else { break };
            if !seen.insert(next) {
                break;
            }
            if out.len() >= usize::from(depth) {
                return (out, true);
            }
            let Some(ns) = self.s.scheduled(next) else {
                break;
            };
            out.push(ChainLink {
                uid: next,
                start: ns.start,
                finish: ns.finish,
                driver: ns.driver,
                raw: ns.driver_bound,
            });
            gate = match driver {
                Driver::SummaryGate { .. } => raw,
                _ => None,
            };
            (uid, driver, raw) = (next, ns.driver, ns.driver_bound);
        }
        (out, false)
    }

    /// Maillon amont d'un pilote : prédécesseur d'un lien, récapitulative
    /// d'une porte, sous-tâche au début minimal d'un agrégat (la première
    /// dans l'ordre des ID) ; aucun pour le début du projet, une contrainte,
    /// des dates saisies ou réelles.
    fn upstream(&self, uid: TaskUid, driver: Driver) -> Option<TaskUid> {
        match driver {
            Driver::Link { predecessor, .. } => Some(predecessor),
            Driver::SummaryGate { summary } => Some(summary),
            Driver::Rollup => self.earliest_child(uid),
            Driver::ProjectStart | Driver::Manual | Driver::Constraint { .. } | Driver::Actual => None,
        }
    }

    /// R-056 : cause de la porte de la récapitulative `summary` dont la
    /// borne brute vaut `raw` : son premier lien FS ou SS entrant dont la
    /// borne vaut `raw`, sinon aucune (début du projet, SNET, porte d'un
    /// ancêtre : la chaîne reprend par l'agrégat).
    fn gate_cause(&self, summary: TaskUid, raw: Moment) -> Option<TaskUid> {
        let task = self.p.tasks.get(summary)?;
        let mut scale = LagScale::Unresolved;
        let cal = self.measure_calendar_of(summary, Some(task));
        task.predecessors
            .iter()
            .filter(|l| matches!(l.kind, LinkType::FinishToStart | LinkType::StartToStart))
            .find(|l| {
                self.s
                    .scheduled(l.predecessor)
                    .and_then(|ps| self.link_bound(l, ps, cal, &mut scale))
                    == Some(raw)
            })
            .map(|l| l.predecessor)
    }

    /// Sous-tâche planifiée de début minimal de la récapitulative `uid`
    /// (UID 0 : toutes les tâches), la première dans l'ordre des ID.
    fn earliest_child(&self, uid: TaskUid) -> Option<TaskUid> {
        let tasks = &self.p.tasks;
        let (from, level) = if uid == TaskUid::PROJECT_SUMMARY {
            (0, None)
        } else {
            let pos = tasks.position(uid)?;
            (pos.checked_add(1)?, Some(tasks.at(pos)?.outline_level))
        };
        let mut best: Option<(Moment, TaskUid)> = None;
        for t in tasks.iter().skip(from) {
            if t.is_null {
                continue;
            }
            if level.is_some_and(|l| t.outline_level <= l) {
                break;
            }
            if let Some(cs) = self.s.scheduled(t.uid())
                && best.is_none_or(|(b, _)| cs.start < b)
            {
                best = Some((cs.start, t.uid()));
            }
        }
        best.map(|(_, u)| u)
    }

    /// R-231 : l'explication repose sur R-206 (marge d'une tâche en cours,
    /// ou plafond tiré de la partie restante d'un successeur en cours).
    fn hypothesis(&self) -> bool {
        let running = |uid: TaskUid| {
            self.s.scheduled(uid).is_some_and(|t| {
                t.role == TaskRole::Leaf
                    && !t.manual
                    && t.progress.is_some_and(|g| g.state == ProgressState::Started)
            })
        };
        running(self.uid)
            || matches!(self.ts.late_driver, LateDriver::Successor { successor, .. } if running(successor))
    }
}

/// Extrémité de la tâche dont on cite le calendrier.
#[derive(Clone, Copy)]
enum Placement {
    Start,
    Finish,
}

/// Échelle des décalages ouvrés d'une tâche, compilée au besoin.
enum LagScale {
    Unresolved,
    Missing,
    Compiled(CompiledCalendar),
}

/// Calendrier du projet tel que le moteur l'utilise (R-113 : la base d'un
/// calendrier du projet dérivé) ; `None` sous `project.calendar_fallback`
/// (Standard intégré, R-066).
fn project_calendar(p: &Project, s: &Schedule) -> Option<CalendarUid> {
    if s.diagnostics
        .iter()
        .any(|d| matches!(d, Diagnostic::ProjectCalendarFallback { .. }))
    {
        return None;
    }
    match p.calendars.get(p.calendar).map(Calendar::kind) {
        Some(CalendarKind::Derived { base, .. }) => Some(*base),
        _ => Some(p.calendar),
    }
}
