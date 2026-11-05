//! Décaler la sélection (finitions avant v1.0.0, ADR-0022 D1, H-UI-121) :
//! Tâche → Planification → Décaler ▾ et son jumeau du menu Tâche.
//!
//! Un geste = un lot `task.shift` (R-267), une entrée d'annulation. Le lot
//! est d'abord simulé (`Session::simulate`) : s'il ne change rien (tâches
//! toutes achevées, inactives ou sans date), aucune entrée n'est créée ;
//! une information dit pourquoi si des tâches ont été ignorées. Sinon il
//! est appliqué et un compte rendu en langage d'utilisateur est publié
//! (tâches décalées, ignorées, contraintes remplacées, décalages limités
//! par les prédécesseurs). Aucune date n'est calculée ici : les dates
//! viennent du moteur (règle 3).

use plannercraft_automation::command::{DurationJson, TaskShift};
use plannercraft_automation::text::TextContext;
use plannercraft_automation::{
    ApplyOptions, ApplyOutcome, Batch, Command, CommandError, SideEffect, TaskRef,
};
use plannercraft_core::{Project, Severity, TaskUid, TimeBasis};

use crate::action::ShiftStep;
use crate::app::{App, EditOrigin};
use crate::i18n::{Arg, TextId, Texts};

/// Plafond d'ID cités par une ligne du compte rendu ; au-delà : « … ».
const MAX_CITED: usize = 12;

/// Décalage JSON d'un pas prédéfini : minutes signées de l'unité dans les
/// facteurs du projet (R-006 : 1 j = minutes par jour, 1 sem = minutes par
/// semaine), unité d'affichage gardée.
#[must_use]
pub fn step_amount(project: &Project, step: ShiftStep) -> DurationJson {
    let (count, unit) = step.amount();
    let ctx = TextContext::for_project(project, plannercraft_automation::text::Language::Fr);
    let per = ctx
        .factors
        .minutes_per_unit(unit.into(), TimeBasis::Working)
        .get();
    DurationJson {
        minutes: Some(count.saturating_mul(per)),
        unit: Some(unit),
        text: None,
        elapsed: None,
        estimated: None,
    }
}

/// ID affichés (position + 1) des tâches `uids`, dans l'ordre des ID,
/// séparés par « , » ; au-delà de [`MAX_CITED`] : « … ».
fn cited(project: &Project, uids: &[TaskUid]) -> String {
    let mut ids: Vec<usize> = uids
        .iter()
        .filter_map(|uid| project.tasks.position(*uid))
        .map(|pos| pos.saturating_add(1))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    let mut parts: Vec<String> = ids.iter().take(MAX_CITED).map(ToString::to_string).collect();
    if ids.len() > MAX_CITED {
        parts.push("…".to_owned());
    }
    parts.join(", ")
}

/// Tâches d'un effet du lot.
fn tasks_of(outcome: &ApplyOutcome, pick: impl Fn(&SideEffect) -> Option<TaskUid>) -> Vec<TaskUid> {
    outcome.side_effects.iter().filter_map(pick).collect()
}

/// Compte rendu d'un décalage appliqué (H-UI-121) : gravité (information,
/// avertissement s'il y a une contrainte remplacée ou un décalage limité)
/// et texte ; `by` : le décalage tel que l'utilisateur l'a choisi.
#[must_use]
pub fn report(texts: &Texts, project: &Project, outcome: &ApplyOutcome, by: &str) -> (Severity, String) {
    let shifted = tasks_of(outcome, |e| match e {
        SideEffect::TaskShifted { task } => Some(*task),
        _ => None,
    });
    let skipped = tasks_of(outcome, |e| match e {
        SideEffect::ShiftSkipped { task, .. } => Some(*task),
        _ => None,
    });
    let replaced = tasks_of(outcome, |e| match e {
        SideEffect::ShiftConstraintReplaced { task, .. } => Some(*task),
        _ => None,
    });
    let limited = tasks_of(outcome, |e| match e {
        SideEffect::ShiftLimited { task } => Some(*task),
        _ => None,
    });
    let count = i64::try_from(shifted.len()).unwrap_or(i64::MAX);
    let mut lines = vec![texts.get_args(
        TextId::ShiftReportDone,
        &[
            ("count", Arg::Count(count)),
            ("by", Arg::Str(by.to_owned().into())),
        ],
    )];
    for (uids, id) in [
        (&skipped, TextId::ShiftReportSkipped),
        (&replaced, TextId::ShiftReportReplaced),
        (&limited, TextId::ShiftReportLimited),
    ] {
        if !uids.is_empty() {
            lines.push(texts.get_args(id, &[("tasks", Arg::Str(cited(project, uids).into()))]));
        }
    }
    let severity = if replaced.is_empty() && limited.is_empty() {
        Severity::Info
    } else {
        Severity::Warning
    };
    (severity, lines.join(" "))
}

impl App {
    /// Décale la sélection d'un pas prédéfini (H-UI-121).
    pub(crate) fn shift_step(&mut self, step: ShiftStep) {
        let Some(document) = &self.document else {
            return;
        };
        let by = step_amount(document.project(), step);
        let label = self.texts.get(step.label());
        drop(self.shift_selection(by, &label, EditOrigin::User));
    }

    /// Décale la sélection de `by` (un lot `task.shift`, R-267) ; `shown` :
    /// le décalage tel qu'affiché dans le compte rendu. Rien à décaler ⇒
    /// aucune entrée d'annulation (information si des tâches ont été
    /// ignorées) ; refus du registre : affiché selon `origin` et rendu.
    ///
    /// # Errors
    ///
    /// Refus du registre (`param.invalid` sur `by`, budget de recalcul…).
    pub(crate) fn shift_selection(
        &mut self,
        by: DurationJson,
        shown: &str,
        origin: EditOrigin,
    ) -> Result<(), CommandError> {
        let Some(document) = &self.document else {
            return Ok(());
        };
        let uids: Vec<TaskUid> = self.selected.uids().iter().copied().collect();
        if uids.is_empty() {
            let text = self.texts.get(TextId::TaskCmdNoSelection);
            self.push_message(Severity::Info, text);
            return Ok(());
        }
        let batch = Batch {
            format_version: Batch::FORMAT_VERSION,
            lang: Some(self.text_language()),
            ops: vec![Command::TaskShift(TaskShift {
                tasks: uids.into_iter().map(TaskRef::Uid).collect(),
                by,
            })],
            label: Some(self.texts.get(TextId::RibbonShift)),
        };
        let session = &document.session;
        let simulated = match session.simulate(&batch, ApplyOptions::default()) {
            Ok(simulated) => simulated,
            Err(error) => {
                if origin == EditOrigin::User {
                    self.report_command_error(&error);
                }
                return Err(error);
            }
        };
        if simulated.project == *session.project() {
            // Geste sans effet : aucune entrée ; une information seulement
            // si des tâches ont été écartées (sinon rien, R-260).
            let skipped = simulated
                .outcome
                .side_effects
                .iter()
                .any(|e| matches!(e, SideEffect::ShiftSkipped { .. }));
            if skipped {
                let text = self.texts.get(TextId::ShiftReportNothing);
                self.push_message(Severity::Info, text);
            }
            return Ok(());
        }
        let outcome = self.execute(&batch, origin)?;
        if let Some(document) = &self.document {
            let (severity, text) = report(&self.texts, document.project(), &outcome, shown);
            self.push_message(severity, text);
        }
        Ok(())
    }
}
