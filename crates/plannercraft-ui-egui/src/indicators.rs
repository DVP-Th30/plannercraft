//! Colonne Indicateurs (finitions avant v1.0.0, ADR-0022 D2, H-UI-122) :
//! icônes d'une ligne, dans l'ordre note, contrainte, échéance dépassée,
//! tâche manuelle, inactive, achevée ; infobulle d'une ligne par
//! indicateur. Lecture seule du modèle et du `Schedule` (règle 3) :
//! l'échéance dépassée est celle que le moteur signale
//! (`task.deadline_missed`, R-102), le % achevé celui du moteur.

use std::collections::BTreeMap;

use plannercraft_core::{ConstraintKind, Diagnostic, Moment, Percent, ScheduleAnchor, Task, TaskUid};

use crate::format::{FieldLabels, FormatLabels, format_date};
use crate::i18n::{Arg, TextId, Texts};
use crate::ribbon::Icon;
use crate::rows::Row;

/// Indicateur d'une ligne.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Indicator {
    /// La tâche a une note (clic : ouvre les notes).
    Note,
    /// Contrainte autre que « Dès que possible » (planification depuis le
    /// début) ou « Au plus tard possible » (depuis la fin).
    Constraint {
        /// Type.
        kind: ConstraintKind,
        /// Date, pour un type daté.
        date: Option<Moment>,
    },
    /// Échéance dépassée (fin planifiée après l'échéance, d'après le
    /// moteur).
    DeadlineMissed {
        /// Échéance.
        deadline: Moment,
    },
    /// Tâche planifiée manuellement.
    Manual,
    /// Tâche inactive.
    Inactive,
    /// Tâche achevée (100 %).
    Complete,
}

impl Indicator {
    /// Icône Lucide de l'indicateur.
    #[must_use]
    pub const fn icon(self) -> Icon {
        match self {
            Self::Note => Icon::StickyNote,
            Self::Constraint { .. } => Icon::CalendarClock,
            Self::DeadlineMissed { .. } => Icon::Flag,
            Self::Manual => Icon::Pin,
            Self::Inactive => Icon::CircleOff,
            Self::Complete => Icon::CircleCheck,
        }
    }

    /// Ligne de l'infobulle, en langage d'utilisateur.
    #[must_use]
    pub fn line(self, texts: &Texts, fields: &FieldLabels, labels: &FormatLabels) -> String {
        match self {
            Self::Note => texts.get(TextId::IndicatorNote),
            Self::Constraint { kind, date } => {
                let kind = fields.constraint(kind).to_owned();
                match date {
                    Some(date) => texts.get_args(
                        TextId::IndicatorConstraint,
                        &[
                            ("kind", Arg::Str(kind.into())),
                            ("date", Arg::Str(format_date(date, labels).into())),
                        ],
                    ),
                    None => texts.get_args(
                        TextId::IndicatorConstraintUndated,
                        &[("kind", Arg::Str(kind.into()))],
                    ),
                }
            }
            Self::DeadlineMissed { deadline } => texts.get_args(
                TextId::IndicatorDeadline,
                &[("date", Arg::Str(format_date(deadline, labels).into()))],
            ),
            Self::Manual => texts.get(TextId::IndicatorManual),
            Self::Inactive => texts.get(TextId::IndicatorInactive),
            Self::Complete => texts.get(TextId::IndicatorComplete),
        }
    }
}

/// Échéances dépassées signalées par le moteur (`task.deadline_missed`),
/// par tâche.
#[must_use]
pub fn missed_deadlines(diagnostics: &[Diagnostic]) -> BTreeMap<TaskUid, Moment> {
    diagnostics
        .iter()
        .filter_map(|d| match d {
            Diagnostic::TaskDeadlineMissed { task, deadline, .. } => Some((*task, *deadline)),
            _ => None,
        })
        .collect()
}

/// Contrainte par défaut du sens de planification (aucun indicateur).
const fn default_constraint(anchor: ScheduleAnchor) -> ConstraintKind {
    match anchor {
        ScheduleAnchor::FromStart(_) => ConstraintKind::AsSoonAsPossible,
        ScheduleAnchor::FromFinish(_) => ConstraintKind::AsLateAsPossible,
    }
}

/// Indicateurs de la tâche `task` affichée par la ligne `row`, dans
/// l'ordre de la colonne.
#[must_use]
pub fn of(
    task: &Task,
    row: &Row,
    anchor: ScheduleAnchor,
    missed: &BTreeMap<TaskUid, Moment>,
) -> Vec<Indicator> {
    let mut out = Vec::new();
    if !task.notes.trim().is_empty() {
        out.push(Indicator::Note);
    }
    let kind = task.constraint.kind();
    if kind != default_constraint(anchor) {
        out.push(Indicator::Constraint {
            kind,
            date: task.constraint.date(),
        });
    }
    if let Some(deadline) = missed.get(&row.uid) {
        out.push(Indicator::DeadlineMissed { deadline: *deadline });
    }
    if row.manual {
        out.push(Indicator::Manual);
    }
    if !row.active {
        out.push(Indicator::Inactive);
    }
    if row.percent >= Percent::FULL {
        out.push(Indicator::Complete);
    }
    out
}

/// Infobulle : une ligne par indicateur.
#[must_use]
pub fn tooltip(
    indicators: &[Indicator],
    texts: &Texts,
    fields: &FieldLabels,
    labels: &FormatLabels,
) -> String {
    indicators
        .iter()
        .map(|i| i.line(texts, fields, labels))
        .collect::<Vec<_>>()
        .join("\n")
}
