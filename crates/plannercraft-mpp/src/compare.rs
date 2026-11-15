//! Dates enregistrées par MS Project contre dates recalculées par le moteur
//! (R-291, R-298). Rien n'est corrigé : l'écart est une information.

use std::collections::BTreeMap;

use plannercraft_core::{
    ConstraintKind, Minutes, Project, Schedule, ScheduleAnchor, SchedulingMode, TaskOutcome, TaskUid,
    WorkContour,
};
use serde::Serialize;

use crate::Recorded;

/// Écart au plus d'une tâche « proche », en minutes (R-298) : depuis
/// ADR-0039 (comparaison au dixième), une minute ; avant, deux (arrondi
/// du moteur à la minute).
pub const ROUNDING_MINUTES: i64 = 1;
/// Même seuil en dixièmes de minute.
const ROUNDING_TENTHS: i64 = ROUNDING_MINUTES * 10;
/// Décalage d'une affectation au-delà duquel elle est « décalée » (R-298),
/// en dixièmes de minute : deux minutes, inchangé.
const STAGGER_TENTHS: i64 = 20;

/// Classe de l'écart d'une tâche (R-298).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GapClass {
    /// Début et fin à au plus [`ROUNDING_MINUTES`] minute : écart de calcul
    /// de MS Project que le moteur ne reproduit pas (durées flottantes).
    Rounding,
    /// Écart réel.
    Real,
}

/// Cause connue d'un écart réel que le calcul ne peut pas reproduire à
/// partir des données lues (R-298), constatée sur la tâche elle-même.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GapCause {
    /// Une affectation de la tâche a un profil modifié à la main : la
    /// répartition saisie n'est pas reprise.
    ContouredAssignment,
    /// Une affectation de travail commence, enregistrée, après la tâche
    /// sans retard d'affectation ni de nivellement lu : décalage saisi ou
    /// laissé par un nivellement par affectation.
    StaggeredAssignments,
    /// Tâche automatique sans lien, contrainte, avancement ni
    /// récapitulative manuelle qui place son début : début enregistré
    /// ailleurs qu'au début du projet (planning non recalculé ou déplacé
    /// sans contrainte).
    UnexplainedStart,
}

impl GapCause {
    /// Code stable.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::ContouredAssignment => "mpp.gap.contoured_assignment",
            Self::StaggeredAssignments => "mpp.gap.staggered_assignments",
            Self::UnexplainedStart => "mpp.gap.unexplained_start",
        }
    }

    /// Libellé.
    #[must_use]
    pub const fn label(self, fr: bool) -> &'static str {
        match (self, fr) {
            (Self::ContouredAssignment, true) => "profil d'affectation modifié à la main",
            (Self::ContouredAssignment, false) => "assignment contour edited by hand",
            (Self::StaggeredAssignments, true) => "affectation décalée sans retard enregistré",
            (Self::StaggeredAssignments, false) => "assignment shifted without a saved delay",
            (Self::UnexplainedStart, true) => "début enregistré sans lien ni contrainte qui l'explique",
            (Self::UnexplainedStart, false) => "saved start not explained by any link or constraint",
        }
    }
}

/// Date dont le calcul diffère de l'enregistrement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DateGap {
    /// UID de la tâche.
    pub uid: u32,
    /// ID (position).
    pub id: u32,
    /// `start` ou `finish`.
    pub field: &'static str,
    /// Date enregistrée (`AAAA-MM-JJTHH:MM`).
    pub recorded: String,
    /// Date calculée par le moteur.
    pub computed: String,
    /// Écart (calculé − enregistré), au dixième de minute (ADR-0039) ;
    /// en JSON, des minutes décimales.
    pub minutes: Minutes,
    /// Classe de l'écart de la tâche.
    pub class: GapClass,
    /// Cause connue, non recalculable (écart réel seulement).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<GapCause>,
}

/// Bilan de la comparaison.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Comparison {
    /// Tâches comparées (non vides, planifiées, avec début et fin
    /// enregistrés).
    pub tasks_compared: u32,
    /// Tâches dont le début et la fin calculés égalent, à la minute, ceux
    /// enregistrés.
    pub tasks_identical: u32,
    /// Tâches non identiques dont le début et la fin sont à au plus
    /// [`ROUNDING_MINUTES`] minute.
    pub tasks_rounding: u32,
    /// Tâches en écart réel.
    pub tasks_off: u32,
    /// Débuts égaux.
    pub starts_equal: u32,
    /// Fins égales.
    pub finishes_equal: u32,
    /// Tâches en écart réel par cause connue non recalculable (code).
    pub causes: BTreeMap<&'static str, u32>,
    /// Tous les écarts, dans l'ordre des tâches.
    pub gaps: Vec<DateGap>,
}

impl Comparison {
    /// Tâches dont une date diffère.
    #[must_use]
    pub const fn tasks_different(&self) -> u32 {
        self.tasks_compared.saturating_sub(self.tasks_identical)
    }
}

/// Liens et contraintes qui placent une tâche : les siens et ceux de ses
/// récapitulatives.
struct Placed {
    /// Position de la récapitulative parente, par position de tâche.
    parent: Vec<Option<usize>>,
    /// La tâche a des sous-tâches (récapitulative).
    summary: Vec<bool>,
}

impl Placed {
    fn new(project: &Project) -> Self {
        let mut parent = Vec::with_capacity(project.tasks.len());
        let mut stack: Vec<(u16, usize)> = Vec::new();
        for (pos, t) in project.tasks.iter().enumerate() {
            if t.is_null {
                parent.push(None);
                continue;
            }
            let level = t.outline_level.get();
            while stack.last().is_some_and(|(l, _)| *l >= level) {
                stack.pop();
            }
            parent.push(stack.last().map(|(_, p)| *p));
            stack.push((level, pos));
        }
        let mut summary = vec![false; parent.len()];
        for p in parent.iter().flatten() {
            if let Some(s) = summary.get_mut(*p) {
                *s = true;
            }
        }
        Self { parent, summary }
    }

    /// Une tâche ou une de ses récapitulatives a un lien, une contrainte,
    /// un mode manuel ou de l'avancement.
    fn anchored(&self, project: &Project, pos: usize) -> bool {
        let mut cur = Some(pos);
        let mut budget = self.parent.len();
        while let Some(p) = cur {
            let Some(t) = project.tasks.at(p) else {
                return true;
            };
            if !t.predecessors.is_empty()
                || t.constraint.kind() != ConstraintKind::AsSoonAsPossible
                || !matches!(t.mode, SchedulingMode::Auto)
                || t.progress.actual_start.is_some()
                || t.leveling_delay.is_some()
            {
                return true;
            }
            budget = budget.saturating_sub(1);
            if budget == 0 {
                return true;
            }
            cur = self.parent.get(p).copied().flatten();
        }
        false
    }
}

/// Cause connue de l'écart réel de la tâche de position `pos`.
fn cause(
    project: &Project,
    placed: &Placed,
    recorded: &Recorded,
    pos: usize,
    uid: TaskUid,
    recorded_start: plannercraft_core::Moment,
) -> Option<GapCause> {
    let task = project.tasks.at(pos)?;
    let assignments: Vec<_> = project.assignments.iter().filter(|a| a.task == uid).collect();
    if assignments.iter().any(|a| a.contour == WorkContour::Contoured) {
        return Some(GapCause::ContouredAssignment);
    }
    let work = |a: &plannercraft_core::Assignment| {
        a.resource
            .and_then(|r| project.resources.get(r))
            .is_some_and(|r| r.kind == plannercraft_core::ResourceKind::Work)
    };
    let shifted = assignments.iter().any(|a| {
        work(a)
            && a.delay == Minutes::ZERO
            && a.leveling_delay.is_none()
            && recorded
                .assignments
                .get(&a.uid())
                .and_then(|r| r.start)
                .is_some_and(|s| s.tenths().saturating_sub(recorded_start.tenths()) > STAGGER_TENTHS)
    });
    if shifted {
        return Some(GapCause::StaggeredAssignments);
    }
    if let ScheduleAnchor::FromStart(start) = project.anchor
        && !placed.summary.get(pos).copied().unwrap_or(true)
        && !placed.anchored(project, pos)
        && task.progress.actual_start.is_none()
        && recorded_start != start
    {
        return Some(GapCause::UnexplainedStart);
    }
    None
}

/// Compare les dates enregistrées au planning calculé par le moteur.
#[must_use]
pub fn compare(project: &Project, schedule: &Schedule, recorded: &Recorded) -> Comparison {
    let mut out = Comparison::default();
    let placed = Placed::new(project);
    for (pos, task) in project.tasks.iter().enumerate() {
        if task.is_null {
            continue;
        }
        let uid = task.uid();
        let (Some(rec), Some(TaskOutcome::Scheduled(s))) =
            (recorded.tasks.get(&uid), schedule.tasks.get(&uid))
        else {
            continue;
        };
        let (Some(rs), Some(rf)) = (rec.start, rec.finish) else {
            continue;
        };
        out.tasks_compared = out.tasks_compared.saturating_add(1);
        // Écarts au dixième de minute (ADR-0039), la précision de MS Project.
        let ds = s.start.tenths().saturating_sub(rs.tenths());
        let df = s.finish.tenths().saturating_sub(rf.tenths());
        if ds == 0 {
            out.starts_equal = out.starts_equal.saturating_add(1);
        }
        if df == 0 {
            out.finishes_equal = out.finishes_equal.saturating_add(1);
        }
        if ds == 0 && df == 0 {
            out.tasks_identical = out.tasks_identical.saturating_add(1);
            continue;
        }
        let rounding = ds.abs() <= ROUNDING_TENTHS && df.abs() <= ROUNDING_TENTHS;
        let (class, why) = if rounding {
            out.tasks_rounding = out.tasks_rounding.saturating_add(1);
            (GapClass::Rounding, None)
        } else {
            out.tasks_off = out.tasks_off.saturating_add(1);
            let why = cause(project, &placed, recorded, pos, uid, rs);
            if let Some(c) = why {
                let n = out.causes.entry(c.code()).or_insert(0);
                *n = n.saturating_add(1);
            }
            (GapClass::Real, why)
        };
        let id = u32::try_from(pos).unwrap_or(u32::MAX).saturating_add(1);
        for (field, r, c, d) in [("start", rs, s.start, ds), ("finish", rf, s.finish, df)] {
            if d != 0 {
                out.gaps.push(DateGap {
                    uid: uid.get(),
                    id,
                    field,
                    recorded: r.to_string(),
                    computed: c.to_string(),
                    minutes: Minutes::from_tenths(d),
                    class,
                    cause: why,
                });
            }
        }
    }
    out
}
