//! Agrégats des récapitulatives et assemblage du résultat (spécification J2
//! § 3.10).
//!
//! Récapitulatives : dates (R-053, agrégat de la passe avant), durée
//! (R-054), % achevé (R-055), dates au plus tard agrégées (R-060, dans
//! `backward.rs`), récapitulative manuelle datée (R-064), récapitulative du
//! projet (R-068), dates planifiées (R-112 : au plus tôt depuis le début, au
//! plus tard à rebours, enveloppe de celles des enfants).

use std::collections::BTreeMap;
use std::ops::Range;

use super::constraint::Mode;
use super::network::{NodeIx, NodeKind, PROJECT, Role, Status, TaskInfo, pos};
use super::{
    Calc, Driver, Exclusion, LateDriver, ProgressSchedule, Schedule, TaskOutcome, TaskRole, TaskSchedule,
};
use crate::calendar::Span;
use crate::diagnostic::Diagnostic;
use crate::duration::{Minutes, Work};
use crate::fixed::round_div;
use crate::ids::TaskUid;
use crate::model::{ResourceKind, SchedulingMode, Task};
use crate::progress::{self, ProgressState};
use crate::quantity::Percent;
use crate::time::Moment;

impl Calc<'_, '_> {
    /// Assemble le `Schedule` : une entrée par tâche du modèle, l'ordre
    /// topologique des tâches planifiées, les diagnostics triés.
    pub(super) fn output(mut self, order: &[NodeIx]) -> Schedule {
        self.empty_summary_links();
        // J5 : affectations placées et durée calculée des feuilles à parts
        // (R-173).
        let (assignments, durations) = self.place_assignments();
        let sums = match self.strategy {
            super::Strategy::Fast => self.percent_sums(),
            super::Strategy::Reference => None,
        };
        // Sans aucun avancement saisi, ni travail par tâche ni agrégats
        // (P37 : planning inchangé, coût nul sur les réseaux sans suivi).
        let any_progress = self
            .project
            .tasks
            .iter()
            .any(|t| progress::facts(t).state != ProgressState::NotStarted);
        let work = if any_progress {
            self.leaf_work()
        } else {
            Vec::new()
        };
        let mut tasks: BTreeMap<TaskUid, TaskOutcome> = BTreeMap::new();
        for (p, task) in self.project.tasks.iter().enumerate() {
            let outcome = if let Some(info) = self.net.task(p).cloned() {
                let w = work.get(p).copied().unwrap_or((0, 0));
                self.outcome(p, task, &info, (sums.as_deref(), durations.get(&p).copied(), w))
            } else {
                self.diags.push(Diagnostic::ScheduleInternal {
                    detail: format!("tâche {} non classée", task.uid()),
                });
                TaskOutcome::Excluded {
                    reason: Exclusion::Unschedulable,
                    display: None,
                }
            };
            tasks.insert(task.uid(), outcome);
        }
        let mut sorted = Vec::new();
        for &v in order {
            let Some(node) = self.net.node(v) else { continue };
            if node.kind != NodeKind::Gate && matches!(tasks.get(&node.uid), Some(TaskOutcome::Scheduled(_)))
            {
                sorted.push(node.uid);
            }
        }
        // R-210 : agrégats d'avancement des récapitulatives, puis de l'UID 0.
        let root = if any_progress {
            self.progress_rollup(&mut tasks, &work)
        } else {
            Agg::default()
        };
        let project_summary = self.project_summary(&tasks, sums.as_deref(), &root);
        Schedule {
            project_start: self.start,
            project_finish: self.finish,
            project_summary,
            tasks,
            order: sorted,
            diagnostics: self.diags.finish(),
            assignments,
            resources: BTreeMap::new(),
        }
    }

    /// R-068 : récapitulative du projet (UID 0), agrégat des tâches actives
    /// planifiées de premier niveau (R-053, R-054, R-055, R-060) sur le
    /// calendrier du projet ; sans lien, sa marge libre égale sa marge
    /// totale ; dates planifiées et durée sur l'enveloppe des dates
    /// planifiées de ces tâches (R-112). `None` si aucune tâche n'est
    /// planifiée.
    fn project_summary(
        &self,
        tasks: &BTreeMap<crate::ids::TaskUid, TaskOutcome>,
        sums: Option<&[LeafSums]>,
        root: &Agg,
    ) -> Option<TaskSchedule> {
        let (mut early, mut late, mut planned, mut degraded): (
            Option<Span>,
            Option<Span>,
            Option<Span>,
            bool,
        ) = (None, None, None, false);
        let hull = |a: Option<Span>, b: Span| {
            Some(a.map_or(b, |a| Span {
                start: a.start.min(b.start),
                finish: a.finish.max(b.finish),
            }))
        };
        for info in &self.net.tasks {
            if info.parent.is_some() || info.role == Role::Null || info.inactive() {
                continue;
            }
            let Some(TaskOutcome::Scheduled(ts)) = tasks.get(&info.uid) else {
                continue;
            };
            early = hull(early, ts.early);
            late = hull(late, ts.late);
            planned = hull(
                planned,
                Span {
                    start: ts.start,
                    finish: ts.finish,
                },
            );
            degraded |= ts.degraded;
        }
        let (mut early, mut late, mut planned) = (early?, late?, planned?);
        // R-210 : une récapitulative commencée commence au plus tard à son
        // début réel agrégé (O40 : A7 UID 0, `Start` = `EarlyStart` =
        // `LateStart` = début réel, sous une récapitulative manuelle qui
        // garde ses dates saisies).
        if let Some(a) = root.actual_start.filter(|&a| a < planned.start) {
            planned.start = a;
            early.start = early.start.min(a);
            late.start = late.start.min(a);
        }
        let scale = self.net.scale(PROJECT);
        let between = |a, b| scale.and_then(|s| s.between(a, b).ok());
        let duration = between(planned.start, planned.finish);
        let tf = match (
            between(early.start, late.start),
            between(early.finish, late.finish),
        ) {
            (Some(a), Some(b)) => Some(a.min(b)),
            _ => None,
        };
        degraded |= duration.is_none() || tf.is_none();
        let tf = tf.unwrap_or(Minutes::ZERO);
        let mut ts = TaskSchedule {
            role: TaskRole::Summary,
            manual: false,
            start: planned.start,
            finish: planned.finish,
            early,
            late,
            duration: duration.unwrap_or(Minutes::ZERO),
            total_slack: tf,
            // R-101 : sans lien, marge libre = marge totale, bornée à 0.
            free_slack: tf.max(Minutes::ZERO),
            critical: tf <= self.critical_limit(),
            percent_complete: self.percent_of(sums, 0..self.net.tasks.len()),
            driver: Driver::Rollup,
            late_driver: LateDriver::Rollup,
            degraded,
            work: Work::ZERO,
            progress: None,
            driver_bound: None,
        };
        ts.progress = summary_progress(&ts, root, None);
        Some(ts)
    }

    /// (Σ W, Σ travail réel) des affectations de travail (ressource de
    /// travail connue) de chaque tâche, par position (R-210, R-211).
    fn leaf_work(&self) -> Vec<(i128, i128)> {
        let mut out = vec![(0i128, 0i128); self.project.tasks.len()];
        for a in &self.project.assignments {
            let work_resource = a
                .resource
                .and_then(|r| self.project.resources.get(r))
                .is_some_and(|r| r.kind == ResourceKind::Work);
            if !work_resource {
                continue;
            }
            let Some(slot) = self.project.tasks.position(a.task).and_then(|p| out.get_mut(p)) else {
                continue;
            };
            slot.0 = slot.0.saturating_add(i128::from(a.work.minutes().get()));
            slot.1 = slot.1.saturating_add(i128::from(a.actual_work.minutes().get()));
        }
        out
    }

    /// R-210 : agrégats d'avancement de chaque récapitulative active
    /// planifiée sur ses feuilles descendantes actives planifiées (les mêmes
    /// que R-055, manuelles et jalons compris, à travers les récapitulatives
    /// manuelles), en un seul passage des positions décroissantes (chaque
    /// récapitulative reçoit l'agrégat de ses enfants directs) ; rend
    /// l'agrégat de toutes les tâches (UID 0). Une récapitulative
    /// automatique commencée commence au plus tard à son début réel agrégé,
    /// marges et durée remesurées (même lecture que l'UID 0, O40).
    fn progress_rollup(&self, tasks: &mut BTreeMap<TaskUid, TaskOutcome>, work: &[(i128, i128)]) -> Agg {
        let n = self.net.tasks.len();
        let mut acc = vec![Agg::default(); n];
        let mut root = Agg::default();
        for q in (0..n).rev() {
            let Some(info) = self.net.task(q) else { continue };
            let own = match info.role {
                Role::Leaf => {
                    let Some((a, d, _)) = self.leaf_terms(q) else {
                        continue;
                    };
                    let ps = match tasks.get(&info.uid) {
                        Some(TaskOutcome::Scheduled(ts)) => ts.progress,
                        _ => continue,
                    };
                    let w = work.get(q).copied().unwrap_or((0, 0));
                    Agg::leaf((a, d), w, ps)
                }
                Role::Summary if !info.inactive() => {
                    // % travail : même périmètre que le travail de R-184,
                    // affectations propres de la récapitulative comprises
                    // (O40 : les 378 h de A7 comptent celles des UID 6, 13).
                    let mut agg = acc.get(q).copied().unwrap_or_default();
                    let (w, aw) = work.get(q).copied().unwrap_or((0, 0));
                    agg.work = agg.work.saturating_add(w);
                    agg.actual_work = agg.actual_work.saturating_add(aw);
                    let fixed = info.exit.is_some_and(|r| self.net.is_fixed_rollup(r));
                    let measure = info
                        .exit
                        .and_then(|r| self.net.node(r))
                        .and_then(|node| self.net.scale(node.measure))
                        .copied();
                    // R-208 : récapitulative manuelle, durée saisie (O41 : A7
                    // UID 1, `ActualDuration` = `ManualDuration` à 100 %).
                    let entered = self.project.tasks.at(q).map(|t| match t.mode {
                        SchedulingMode::Manual {
                            duration: Some(d), ..
                        } => d.minutes(),
                        _ => t.duration.minutes(),
                    });
                    if let Some(TaskOutcome::Scheduled(ts)) = tasks.get_mut(&info.uid) {
                        if !fixed && let Some(m) = measure {
                            self.lower_to_actual_start(ts, &agg, m);
                        }
                        ts.progress = summary_progress(ts, &agg, entered.filter(|_| fixed));
                    }
                    agg
                }
                Role::Summary | Role::Null => continue,
            };
            match info.parent.and_then(|par| acc.get_mut(par)) {
                Some(parent) => parent.merge(&own),
                None => root.merge(&own),
            }
        }
        root
    }

    /// R-210 : récapitulative automatique commencée dont le début réel
    /// agrégé précède son début planifié (sous-tâche d'une récapitulative
    /// manuelle, R-064) : début planifié, au plus tôt et au plus tard ramenés
    /// au début réel ; durée (R-054), marges (R-048, R-049) et criticité
    /// remesurées sur son échelle de mesure `m`.
    fn lower_to_actual_start(&self, ts: &mut TaskSchedule, agg: &Agg, m: crate::calendar::TimeScale<'_>) {
        let Some(a) = agg.actual_start.filter(|&a| a < ts.start) else {
            return;
        };
        ts.start = a;
        ts.early.start = ts.early.start.min(a);
        ts.late.start = ts.late.start.min(a);
        if let Ok(d) = m.between(ts.start, ts.finish) {
            ts.duration = d;
        }
        if let (Ok(s), Ok(f)) = (
            m.between(ts.early.start, ts.late.start),
            m.between(ts.early.finish, ts.late.finish),
        ) {
            let tf = s.min(f);
            ts.total_slack = tf;
            ts.free_slack = ts.free_slack.min(tf).max(Minutes::ZERO);
            ts.critical = tf <= self.critical_limit();
        }
    }

    /// Liens vers une récapitulative sans enfant planifié (R-053) :
    /// `link.excluded_endpoint` (les liens qui en partent sont signalés en
    /// passe avant).
    fn empty_summary_links(&mut self) {
        for arc in &self.net.arcs {
            let Some(l) = arc.link else { continue };
            let Some(target) = self.net.node(arc.to) else {
                continue;
            };
            if target.kind != NodeKind::Gate || target.inactive {
                continue;
            }
            let empty = target
                .twin
                .and_then(pos)
                .and_then(|r| self.st.get(r))
                .is_none_or(|s| s.early.is_none());
            if empty {
                self.diags.push(Diagnostic::LinkExcludedEndpoint {
                    predecessor: l.pred,
                    successor: l.succ,
                });
            }
        }
    }

    /// Résultat de la tâche en position `p`.
    /// `measured` : durée calculée d'une feuille à parts (R-173).
    #[allow(
        clippy::too_many_lines,
        reason = "construction d'un `TaskSchedule` champ par champ (champs J6 compris)"
    )]
    fn outcome(
        &mut self,
        p: usize,
        task: &Task,
        info: &TaskInfo,
        (sums, measured, work): (Option<&[LeafSums]>, Option<Minutes>, (i128, i128)),
    ) -> TaskOutcome {
        let excluded = |reason| TaskOutcome::Excluded {
            reason,
            display: None,
        };
        if info.role == Role::Null {
            return excluded(Exclusion::NullRow);
        }
        if info.manual_undated {
            return excluded(Exclusion::ManualUndated);
        }
        let exit = info.exit.and_then(|v| self.state(v).copied());
        if info.inactive() {
            // R-062 : placement d'affichage.
            let reason = if info.status == Status::Inactive {
                Exclusion::Inactive
            } else {
                Exclusion::InactiveAncestor
            };
            return TaskOutcome::Excluded {
                reason,
                display: exit.and_then(|s| s.early),
            };
        }
        let Some(state) = exit else {
            return excluded(Exclusion::Unschedulable);
        };
        let node = info.exit.and_then(|v| self.net.node(v));
        let fixed = node.is_some_and(|n| n.fixed.is_some());
        let Some(early) = state.early else {
            // Une récapitulative manuelle datée dont les dates ne se
            // calculent pas est non planifiable, pas vide (R-064, R-069).
            return excluded(if info.role == Role::Summary && !fixed {
                Exclusion::EmptySummary
            } else {
                Exclusion::Unschedulable
            });
        };
        let mut degraded = state.degraded || state.late_degraded;
        let late = state.late.unwrap_or_else(|| {
            degraded = true;
            early
        });
        if state.late.is_none() {
            self.diags.push(Diagnostic::ScheduleInternal {
                detail: format!("tâche {} sans dates au plus tard", task.uid()),
            });
        }
        // Cause de la fin au plus tard : agrégat des enfants pour une
        // récapitulative automatique (R-060), repli sans dates au plus tard
        // (R-069).
        let late_driver = if state.late.is_none() {
            LateDriver::Fallback
        } else if info.role == Role::Summary && !fixed {
            LateDriver::Rollup
        } else {
            state.late_driver
        };
        // R-112 : dates planifiées, au plus tôt depuis le début, au plus tard
        // à rebours (enveloppe de celles des enfants pour une récapitulative
        // automatique, par R-053 et R-060).
        let planned = match self.mode {
            Mode::FromStart => early,
            Mode::FromFinish => late,
        };
        // R-178 : le retard de nivellement d'une feuille décale son début
        // planifié, pas son début au plus tôt (O27).
        let early = Span {
            start: state.undelayed.unwrap_or(early.start),
            ..early
        };
        // R-203 : feuille achevée, TS = FS = 0, jamais critique.
        let actual = node.and_then(super::network::Node::actual).is_some();
        let (role, manual, duration, percent_complete) = if info.role == Role::Summary {
            // R-054, mesurée sur les dates planifiées (R-112) ; récapitulative
            // manuelle datée : durée utilisée (R-064).
            let duration = if fixed {
                node.map(|n| n.duration)
            } else {
                // R-113 : mesurée sur le calendrier du projet (de base) pour
                // une récapitulative sans calendrier propre.
                node.and_then(|n| self.net.scale(n.measure))
                    .and_then(|s| s.between(planned.start, planned.finish).ok())
            };
            if duration.is_none() {
                degraded = true;
            }
            (
                TaskRole::Summary,
                fixed,
                duration.unwrap_or(Minutes::ZERO),
                self.percent_of(sums, p.saturating_add(1)..info.end),
            )
        } else {
            // R-209 : la durée d'une feuille achevée est celle de
            // l'avancement, jamais l'union de ses affectations.
            let node_duration = node.map_or(task.duration.minutes(), |n| n.duration);
            (
                TaskRole::Leaf,
                fixed || !matches!(task.mode, SchedulingMode::Auto),
                if actual {
                    node_duration
                } else {
                    measured.unwrap_or(node_duration)
                },
                task.progress.percent_complete,
            )
        };
        let mut ts = TaskSchedule {
            role,
            manual,
            start: planned.start,
            finish: planned.finish,
            early,
            late,
            duration,
            total_slack: state.total_slack,
            free_slack: state.free_slack,
            critical: !actual && state.total_slack <= self.critical_limit(),
            percent_complete,
            driver: state.driver,
            late_driver,
            degraded,
            work: Work::ZERO,
            progress: None,
            driver_bound: if info.role == Role::Leaf {
                state.driver_bound
            } else {
                None
            },
        };
        if info.role == Role::Leaf
            && let Some(n) = node
        {
            ts.progress = self.leaf_progress(task, n, (&ts, state.run), work);
        }
        TaskOutcome::Scheduled(ts)
    }

    /// Avancement calculé d'une feuille planifiée (R-200, R-203, R-204 pour
    /// `Stop` et `Resume`, R-211 pour le % travail) ; `None` si elle n'est
    /// pas commencée. Achevée : dates réelles = dates planifiées (épingle,
    /// R-203), `Stop` = `Resume` = fin réelle. Commencée : début réel saisi,
    /// sinon implicite = début au plus tôt du réseau
    /// (`progress.actual_start_missing`) ; `Stop` = fin de la partie réelle
    /// placée depuis le début réel sur l'échelle de placement (début réel si
    /// la durée réelle est nulle) ; `Resume` = instant ouvré suivant `Stop`,
    /// ou la reprise saisie si elle est plus tardive. Feuille automatique
    /// en cours (R-204, R-205) : les dates du moteur (`run`), reprise
    /// comprise. `work` : (Σ W, Σ travail réel) de ses affectations de
    /// travail.
    fn leaf_progress(
        &mut self,
        task: &Task,
        node: &super::network::Node,
        (ts, run): (&TaskSchedule, Option<super::Run>),
        work: (i128, i128),
    ) -> Option<ProgressSchedule> {
        let f = progress::facts(task);
        let percent_work_complete = ratio(work.1, work.0);
        let actual_work = work_of(work.1);
        match f.state {
            ProgressState::NotStarted => None,
            ProgressState::Complete => Some(ProgressSchedule {
                state: f.state,
                actual_start: ts.start,
                actual_finish: Some(ts.finish),
                stop: ts.finish,
                resume: ts.finish,
                actual_duration: f.actual_duration,
                remaining_duration: Minutes::ZERO,
                percent_complete: Percent::FULL,
                percent_work_complete,
                actual_work,
            }),
            ProgressState::Started => {
                if let Some(run) = run.filter(|_| node.in_progress().is_some()) {
                    return Some(ProgressSchedule {
                        state: f.state,
                        actual_start: run.actual_start,
                        actual_finish: None,
                        stop: run.stop,
                        resume: run.resume,
                        actual_duration: f.actual_duration,
                        remaining_duration: progress::remaining_duration(task),
                        percent_complete: progress::percent_of_duration(task),
                        percent_work_complete,
                        actual_work,
                    });
                }
                let actual_start = f.actual_start.unwrap_or(ts.early.start);
                if f.actual_start.is_none() {
                    self.diags
                        .push(Diagnostic::ProgressActualStartMissing { task: task.uid() });
                }
                let scale = self.net.scale(node.place).copied();
                let stop = scale
                    .filter(|_| f.actual_duration > Minutes::ZERO)
                    .and_then(|s| s.forward_span(actual_start, f.actual_duration).ok())
                    .map_or(actual_start, |span| span.finish);
                let next = scale
                    .and_then(|s| s.forward_span(stop, Minutes::new(1)).ok())
                    .map_or(stop, |span| span.start);
                let resume = task.progress.resume.map_or(next, |r| r.max(next));
                Some(ProgressSchedule {
                    state: f.state,
                    actual_start,
                    actual_finish: None,
                    stop,
                    resume,
                    actual_duration: f.actual_duration,
                    remaining_duration: progress::remaining_duration(task),
                    percent_complete: progress::percent_of_duration(task),
                    percent_work_complete,
                    actual_work,
                })
            }
        }
    }

    /// R-055 : feuille retenue en position `q` (active, planifiée) :
    /// `(réel, durée, %)`, réel = `round_div(durée × %, 10 000)` (R-010) ;
    /// `None` pour une récapitulative, une tâche inactive ou non planifiée.
    ///
    /// Amendement J6 (R-055, R-210) : la durée est celle de la tâche (D de
    /// R-200) et le réel est le réel effectif de R-200 — la durée réelle
    /// saisie si elle est > 0 (bornée à la durée pour une tâche non
    /// achevée), sinon `durée × %` ; une feuille achevée sans durée réelle
    /// saisie compte sa durée, et 100 % pour la moyenne des jalons. Sans
    /// avancement saisi, le % reste 0.
    fn leaf_terms(&self, q: usize) -> Option<(i128, i128, i128)> {
        let (leaf, task) = (self.net.task(q)?, self.project.tasks.at(q)?);
        if leaf.role != Role::Leaf || leaf.inactive() {
            return None;
        }
        let v = leaf.exit?;
        self.state(v)?.early?;
        // D de la tâche (R-200), et non la durée utilisée d'une manuelle ni
        // la plus longue part d'une feuille affectée (O40 : A7, Σ D = 9 840
        // sur les durées des tâches).
        let d = i128::from(task.duration.minutes().get().max(0));
        let state = progress::facts(task).state;
        let entered = i128::from(task.progress.actual_duration.get().max(0));
        let pct = match state {
            ProgressState::Complete => 10_000,
            _ => i128::from(task.progress.percent_complete.get()),
        };
        let a = match state {
            ProgressState::Complete if entered > 0 => entered,
            ProgressState::Complete => d,
            ProgressState::Started if entered > 0 => entered.min(d),
            ProgressState::Started | ProgressState::NotStarted => d
                .checked_mul(pct)
                .and_then(|x| round_div(x, 10_000))
                .map_or(0, i128::from),
        };
        Some((a, d, pct))
    }

    /// Sommes préfixes de [`Self::leaf_terms`] sur les positions (lot L15) :
    /// `sums[q]` porte les sommes des positions `0..q`. Le sous-arbre d'une
    /// récapitulative est une plage contiguë de positions : son % achevé se
    /// lit en O(1). `None` en cas de débordement (jamais attendu : 2³²
    /// feuilles de durée `i64::MAX` tiennent en `i128`) ; l'appelant passe
    /// alors par la référence.
    fn percent_sums(&self) -> Option<Vec<LeafSums>> {
        let n = self.net.tasks.len();
        let mut sums = Vec::with_capacity(n.checked_add(1)?);
        let mut acc = LeafSums::default();
        sums.push(acc);
        for q in 0..n {
            if let Some((a, d, pct)) = self.leaf_terms(q) {
                acc = LeafSums {
                    actual: acc.actual.checked_add(a)?,
                    total: acc.total.checked_add(d)?,
                    pct_sum: acc.pct_sum.checked_add(pct)?,
                    count: acc.count.checked_add(1)?,
                };
            }
            sums.push(acc);
        }
        Some(sums)
    }

    /// R-055 sur les positions `range` : par les sommes préfixes si elles
    /// existent, sinon par la référence.
    fn percent_of(&self, sums: Option<&[LeafSums]>, range: Range<usize>) -> Percent {
        let Some(sums) = sums else {
            return self.percent_over(range);
        };
        let n = sums.len().saturating_sub(1);
        let end = range.end.min(n);
        let start = range.start.min(end);
        let (Some(hi), Some(lo)) = (sums.get(end), sums.get(start)) else {
            return self.percent_over(range);
        };
        let diff = |a: i128, b: i128| a.checked_sub(b);
        match (
            diff(hi.actual, lo.actual),
            diff(hi.total, lo.total),
            diff(hi.pct_sum, lo.pct_sum),
            diff(hi.count, lo.count),
        ) {
            (Some(actual), Some(total), Some(pct_sum), Some(count)) => {
                percent_value(actual, total, pct_sum, count)
            }
            _ => self.percent_over(range),
        }
    }

    /// R-055 : % achevé d'une récapitulative,
    /// `round_div(Σ réel × 10 000, Σ durée)` sur ses feuilles descendantes
    /// actives planifiées (récapitulatives intermédiaires exclues), réel =
    /// durée × % (R-010) ; Σ durée nulle : moyenne arrondie des % ; aucune
    /// feuille : 0.
    /// Feuilles des positions `range` (le sous-arbre d'une récapitulative,
    /// ou tout le projet pour la récapitulative du projet, R-068).
    /// Référence (J3, sous-arbre parcouru) : oracle des sommes préfixes et
    /// leur repli.
    fn percent_over(&self, range: Range<usize>) -> Percent {
        let (mut actual, mut total, mut pct_sum, mut count) = (0i128, 0i128, 0i128, 0i128);
        for q in range {
            let Some((a, d, pct)) = self.leaf_terms(q) else {
                continue;
            };
            actual = actual.saturating_add(a);
            total = total.saturating_add(d);
            pct_sum = pct_sum.saturating_add(pct);
            count = count.saturating_add(1);
        }
        percent_value(actual, total, pct_sum, count)
    }
}

/// Sommes de R-055 sur une plage de positions.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct LeafSums {
    /// Σ réel.
    actual: i128,
    /// Σ durée.
    total: i128,
    /// Σ %.
    pct_sum: i128,
    /// Nombre de feuilles retenues.
    count: i128,
}

/// R-055 : valeur du % achevé depuis ses sommes.
fn percent_value(actual: i128, total: i128, pct_sum: i128, count: i128) -> Percent {
    let value = if total > 0 {
        actual.checked_mul(10_000).and_then(|x| round_div(x, total))
    } else if count > 0 {
        round_div(pct_sum, count)
    } else {
        Some(0)
    };
    // Réels saisis au-delà des durées (feuilles achevées) : borné à 100 %.
    value
        .map(|v| v.clamp(0, Percent::FULL.get()))
        .and_then(|v| Percent::new(v).ok())
        .unwrap_or(Percent::ZERO)
}

/// `round_div(num × 10 000, den)` borné à `[0, 10 000]` ; `den` ≤ 0 : 0
/// (R-210, R-211 : % travail achevé).
fn ratio(num: i128, den: i128) -> Percent {
    if den <= 0 {
        return Percent::ZERO;
    }
    num.checked_mul(10_000)
        .and_then(|x| round_div(x, den))
        .map(|v| v.clamp(0, Percent::FULL.get()))
        .and_then(|v| Percent::new(v).ok())
        .unwrap_or(Percent::ZERO)
}

/// Agrégat d'avancement de R-210 sur des feuilles.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Agg {
    /// Σ réels effectifs (termes de R-055).
    actual: i128,
    /// Σ durées (termes de R-055).
    total: i128,
    /// Σ travail réel des affectations de travail.
    actual_work: i128,
    /// Σ travail des affectations de travail.
    work: i128,
    /// Feuilles retenues.
    leaves: u64,
    /// Feuilles achevées.
    complete: u64,
    /// Feuilles commencées (achevées comprises).
    started: u64,
    /// Plus petit début réel.
    actual_start: Option<Moment>,
    /// Plus grande fin réelle.
    actual_finish: Option<Moment>,
    /// Plus petit `Stop` des feuilles commencées.
    stop: Option<Moment>,
}

impl Agg {
    /// Feuille : termes de R-055 `(réel, durée)`, travail `(W, réel)`,
    /// avancement calculé.
    fn leaf(
        (actual, total): (i128, i128),
        (work, actual_work): (i128, i128),
        p: Option<ProgressSchedule>,
    ) -> Self {
        let mut agg = Self {
            actual,
            total,
            actual_work,
            work,
            leaves: 1,
            ..Self::default()
        };
        if let Some(p) = p {
            agg.started = 1;
            agg.actual_start = Some(p.actual_start);
            agg.stop = Some(p.stop);
            if p.state == ProgressState::Complete {
                agg.complete = 1;
                agg.actual_finish = p.actual_finish;
            }
        }
        agg
    }

    /// Réunit deux agrégats.
    fn merge(&mut self, o: &Self) {
        let min = |a: Option<Moment>, b: Option<Moment>| match (a, b) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let max = |a: Option<Moment>, b: Option<Moment>| match (a, b) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        self.actual = self.actual.saturating_add(o.actual);
        self.total = self.total.saturating_add(o.total);
        self.actual_work = self.actual_work.saturating_add(o.actual_work);
        self.work = self.work.saturating_add(o.work);
        self.leaves = self.leaves.saturating_add(o.leaves);
        self.complete = self.complete.saturating_add(o.complete);
        self.started = self.started.saturating_add(o.started);
        self.actual_start = min(self.actual_start, o.actual_start);
        self.actual_finish = max(self.actual_finish, o.actual_finish);
        self.stop = min(self.stop, o.stop);
    }
}

/// R-210 (R-208 pour une récapitulative manuelle) : avancement d'une
/// récapitulative ou de l'UID 0 depuis l'agrégat de ses feuilles ; `None`
/// si aucune n'est commencée.
/// - % : celui de R-055 (`ts.percent_complete`) ;
/// - réel : `round_div(D × Σ réels, Σ D)` en `i128` (Σ D = 0 :
///   `percent_of(D, %)`), D = durée de la récapitulative (R-054) ;
///   récapitulative manuelle (`manual` : sa durée saisie, sinon celle de la
///   tâche) : D = cette durée, réel = `percent_of(D, %)` ; borné à `[0, D]` ;
/// - reste = D − réel ; début réel = min des débuts réels ; fin réelle =
///   max des fins réelles si toutes ses feuilles sont achevées ;
/// - `Stop` = min des `Stop` des feuilles commencées, `Resume` = `Stop` ;
/// - % travail = `round_div(Σ travail réel × 10 000, Σ W)` sur les
///   affectations de travail des feuilles et des récapitulatives du
///   sous-arbre (périmètre du travail de R-184), Σ W = 0 : 0.
fn summary_progress(ts: &TaskSchedule, agg: &Agg, manual: Option<Minutes>) -> Option<ProgressSchedule> {
    let (Some(actual_start), Some(stop)) = (agg.actual_start, agg.stop) else {
        return None;
    };
    if agg.started == 0 {
        return None;
    }
    let dm = manual.unwrap_or(ts.duration).max(Minutes::ZERO);
    let d = i128::from(dm.get());
    let by_percent = || {
        d.checked_mul(i128::from(ts.percent_complete.get()))
            .and_then(|x| round_div(x, 10_000))
    };
    let actual = if manual.is_some() || agg.total <= 0 {
        by_percent()
    } else {
        d.checked_mul(agg.actual).and_then(|x| round_div(x, agg.total))
    }
    .map_or(0, |a| a.clamp(0, d.try_into().unwrap_or(i64::MAX)));
    let complete = agg.leaves > 0 && agg.complete == agg.leaves;
    Some(ProgressSchedule {
        state: if complete {
            ProgressState::Complete
        } else {
            ProgressState::Started
        },
        actual_start,
        actual_finish: agg.actual_finish.filter(|_| complete),
        stop,
        resume: stop,
        actual_duration: Minutes::new(actual),
        remaining_duration: Minutes::new(dm.get().saturating_sub(actual).max(0)),
        percent_complete: ts.percent_complete,
        percent_work_complete: ratio(agg.actual_work, agg.work),
        actual_work: work_of(agg.actual_work),
    })
}

/// Travail en minutes `i128` (somme saturée) en [`Work`], borné au domaine
/// (règle 10 : jamais de panique, un travail hors domaine est ramené à 0).
fn work_of(minutes: i128) -> Work {
    i64::try_from(minutes.max(0))
        .ok()
        .and_then(|m| Work::new(Minutes::new(m)).ok())
        .unwrap_or(Work::ZERO)
}
