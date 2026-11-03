//! Commande `baseline.set` (J6, ADR-0014 D6 ; R-227) sur
//! `plannercraft_core::baseline` : capture des valeurs du planning courant du lot
//! (R-220 à R-222), refus d'une ligne existante sans `overwrite` (R-223),
//! écriture dans le clone du projet ; annulable comme toute commande
//! (images d'entités, R-164).

use std::collections::BTreeSet;

use plannercraft_core::baseline::{self, BaselineError, BaselineSpec, Scope};
use plannercraft_core::{
    AssignmentUid, BaselineNumber, Diagnostic, Extras, HasUid, ResourceUid, Subject, TaskUid,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::TaskRef;
use super::exec::Ctx;
use crate::error::{CommandError, CommandErrorKind, codes};
use crate::json::RollupJson;
use crate::outcome::SideEffect;

/// `baseline.set` (R-227) : enregistre la ligne de base `number` (0 à 10,
/// défaut 0) sur le projet entier ou sur `tasks`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BaselineSet {
    /// Numéro de la ligne (0 à 10 ; défaut 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(max = 10))]
    pub number: Option<u8>,
    /// Tâches sélectionnées (défaut : projet entier).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tasks: Option<Vec<TaskRef>>,
    /// Cumul vers les récapitulatives (défaut : aucun ; demande une
    /// sélection).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollup: Option<RollupJson>,
    /// Remplacer une ligne existante (sinon `baseline.exists`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub overwrite: bool,
}

/// `baseline.clear` (J7, R-250) : efface la ligne de base `number` (0 à
/// 10, défaut 0) du projet entier ou de `tasks`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BaselineClear {
    /// Numéro de la ligne (0 à 10 ; défaut 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(max = 10))]
    pub number: Option<u8>,
    /// Tâches sélectionnées (défaut : projet entier, ressources comprises).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tasks: Option<Vec<TaskRef>>,
}

/// Jette les `Extras` relus de portée `Baseline#n/` (origine `mspdi`) ;
/// rend leurs clés triées, sans doublon (R-250, comme R-223).
fn drop_scoped(extras: &mut Extras, n: BaselineNumber) -> Vec<String> {
    let prefix = format!("Baseline#{}/", n.get());
    let mut keys = Vec::new();
    extras.0.retain(|e| {
        let scoped = e.origin == plannercraft_mspdi::extras::ORIGIN && e.key.starts_with(&prefix);
        if scoped {
            keys.push(e.key.clone());
        }
        !scoped
    });
    keys.sort();
    keys.dedup();
    keys
}

/// Retire la ligne `n` d'une entité et ses `Extras` de portée ; rend
/// (ligne retirée, clés jetées).
fn clear_one<B>(
    baselines: &mut std::collections::BTreeMap<BaselineNumber, B>,
    extras: &mut Extras,
    n: BaselineNumber,
) -> (bool, Vec<String>) {
    let removed = baselines.remove(&n).is_some();
    (removed, drop_scoped(extras, n))
}

/// Effacement en cours : compteurs, effets et sujets touchés.
#[derive(Default)]
struct Cleared {
    tasks: usize,
    assignments: usize,
    resources: usize,
    dropped: Vec<(Subject, Vec<String>)>,
    touched: Vec<Subject>,
}

impl Cleared {
    /// Compte le résultat de [`clear_one`] pour `subject`.
    fn record(&mut self, subject: Subject, (removed, keys): (bool, Vec<String>)) {
        if removed {
            match subject {
                Subject::Task(_) => self.tasks = self.tasks.saturating_add(1),
                Subject::Assignment(_) => self.assignments = self.assignments.saturating_add(1),
                Subject::Resource(_) => self.resources = self.resources.saturating_add(1),
                _ => {}
            }
        }
        if removed || !keys.is_empty() {
            self.touched.push(subject);
        }
        if !keys.is_empty() {
            self.dropped.push((subject, keys));
        }
    }
}

/// `baseline.clear` (R-250) : sans `tasks`, toutes les tâches (UID 0
/// compris), leurs affectations et les ressources ; avec `tasks`, ces
/// tâches et leurs affectations (ressources inchangées, hypothèse) ;
/// récapitulatives non recalculées ; `Extras` `Baseline#n/` jetés ; effet
/// `BaselineCleared`. Rien à effacer : information
/// `baseline.nothing_to_clear`, aucun changement.
pub(super) fn clear(op: &BaselineClear, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let number = BaselineNumber::new(op.number.unwrap_or(0)).map_err(|_| {
        CommandError::new(
            codes::PARAM_INVALID,
            CommandErrorKind::InvalidParam("numéro de ligne de base attendu de 0 à 10".to_owned()),
        )
        .with_field("number")
    })?;
    let selection = match &op.tasks {
        None => None,
        Some(refs) => {
            let mut uids = BTreeSet::new();
            for r in refs {
                uids.insert(ctx.task(r, "tasks")?);
            }
            Some(uids)
        }
    };
    let p = &mut *ctx.project;
    let mut out = Cleared::default();
    if selection.is_none()
        && let Some(root) = p.summary_task.as_mut()
    {
        let r = clear_one(&mut root.baselines, &mut root.extras, number);
        out.record(Subject::Task(TaskUid::PROJECT_SUMMARY), r);
    }
    let in_scope = |t: TaskUid| selection.as_ref().is_none_or(|s| s.contains(&t));
    let mut tasks: Vec<TaskUid> = p.tasks.iter().map(HasUid::uid).filter(|t| in_scope(*t)).collect();
    tasks.sort();
    for uid in tasks {
        if let Some(t) = p.tasks.get_mut(uid) {
            let r = clear_one(&mut t.baselines, &mut t.extras, number);
            out.record(Subject::Task(uid), r);
        }
    }
    let mut assignments: Vec<AssignmentUid> = p
        .assignments
        .iter()
        .filter(|a| in_scope(a.task))
        .map(HasUid::uid)
        .collect();
    assignments.sort();
    for uid in assignments {
        if let Some(a) = p.assignments.get_mut(uid) {
            let r = clear_one(&mut a.baselines, &mut a.extras, number);
            out.record(Subject::Assignment(uid), r);
        }
    }
    if selection.is_none() {
        let mut resources: Vec<ResourceUid> = p.resources.iter().map(HasUid::uid).collect();
        resources.sort();
        for uid in resources {
            if let Some(r) = p.resources.get_mut(uid) {
                let res = clear_one(&mut r.baselines, &mut r.extras, number);
                out.record(Subject::Resource(uid), res);
            }
        }
    }
    if out.touched.is_empty() {
        ctx.notes.push(Diagnostic::BaselineNothingToClear { number });
        return Ok(());
    }
    ctx.effects.push(SideEffect::BaselineCleared {
        number: number.get(),
        tasks: count(out.tasks),
        assignments: count(out.assignments),
        resources: count(out.resources),
    });
    for (subject, keys) in out.dropped {
        ctx.effects.push(SideEffect::ExtrasDropped { subject, keys });
    }
    for subject in out.touched {
        ctx.touched.insert(subject, ctx.op_index);
    }
    Ok(())
}

/// Refus de la capture → erreur de commande (R-227).
fn capture_error(e: BaselineError) -> CommandError {
    match e {
        BaselineError::Exists { number, count } => CommandError::new(
            codes::BASELINE_EXISTS,
            CommandErrorKind::BaselineExists {
                number: number.get(),
                count,
            },
        )
        .with_field("overwrite"),
        BaselineError::UnknownTask(uid) => {
            CommandError::new(codes::TASK_NOT_FOUND, CommandErrorKind::UnknownTask(uid))
                .with_field("tasks")
                .with_subject(Subject::Task(uid))
        }
        BaselineError::RollupNeedsSelection => CommandError::new(
            codes::BASELINE_ROLLUP_NEEDS_SELECTION,
            CommandErrorKind::BaselineRollupNeedsSelection,
        )
        .with_field("rollup"),
    }
}

/// Nombre d'entrées, borné à `u32::MAX` (règle 10).
fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// `baseline.set` (R-227) : numéro 0 à 10 (sinon `param.invalid`), portée
/// projet ou `tasks`, cumul (`rollup`) et remplacement (`overwrite`) par
/// `baseline::capture` sur le planning courant du lot ; effets :
/// `BaselineSet { … }`, `ExtrasDropped` des `Baseline#n/` d'une ligne
/// remplacée (R-223) ; remarques `baseline.task_skipped` (tâches sautées,
/// R-221) et `baseline.cost_not_computed` (coût recopié, R-223). Aucune
/// date ne change : en `dry_run`, le diff est vide et l'effet rendu.
pub(super) fn set(op: &BaselineSet, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let number = BaselineNumber::new(op.number.unwrap_or(0)).map_err(|_| {
        CommandError::new(
            codes::PARAM_INVALID,
            CommandErrorKind::InvalidParam("numéro de ligne de base attendu de 0 à 10".to_owned()),
        )
        .with_field("number")
    })?;
    let scope = match &op.tasks {
        None => Scope::Project,
        Some(refs) => {
            let mut uids = BTreeSet::new();
            for r in refs {
                uids.insert(ctx.task(r, "tasks")?);
            }
            Scope::Tasks(uids.into_iter().collect())
        }
    };
    let spec = BaselineSpec {
        number,
        scope,
        rollup: op.rollup.unwrap_or_default().into(),
        overwrite: op.overwrite,
    };
    ctx.ensure_schedule()?;
    let edit = baseline::capture(ctx.project, ctx.schedule(), &spec).map_err(capture_error)?;
    let effect = SideEffect::BaselineSet {
        number: number.get(),
        tasks: count(edit.tasks.len()),
        assignments: count(edit.assignments.len()),
        resources: count(edit.resources.len()),
    };
    let mut subjects: Vec<Subject> = edit.tasks.keys().map(|u| Subject::Task(*u)).collect();
    subjects.extend(edit.assignments.keys().map(|u| Subject::Assignment(*u)));
    subjects.extend(edit.resources.keys().map(|u| Subject::Resource(*u)));
    if !edit.skipped.is_empty() {
        ctx.notes.push(Diagnostic::BaselineTaskSkipped {
            count: count(edit.skipped.len()),
        });
    }
    if edit.cost_copied {
        ctx.notes.push(Diagnostic::BaselineCostNotComputed { number });
    }
    let dropped = baseline::apply(ctx.project, number, edit);
    ctx.effects.push(effect);
    for (subject, keys) in dropped {
        ctx.effects.push(SideEffect::ExtrasDropped { subject, keys });
    }
    for subject in subjects {
        ctx.touched.insert(subject, ctx.op_index);
    }
    Ok(())
}
