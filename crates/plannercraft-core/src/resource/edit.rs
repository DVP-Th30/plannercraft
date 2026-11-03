//! Équilibrage durée, travail et unités à l'édition (ADR-0013 D3 ; règles
//! R-170, R-189 à R-194, lot L2).
//!
//! Fonctions pures, sans calendrier : durées et travail en minutes ouvrées,
//! unités en 1/10 000, calcul en `i128`, arrondi unique R-001. Les commandes
//! (`task.update`, `assignment.*`) les appellent avec l'état d'une tâche et
//! de ses affectations de travail, puis écrivent le résultat ; le recalcul
//! du planning ne rééquilibre jamais (D3 : l'arrondi dériverait et casserait
//! l'aller-retour).
//!
//! Conventions (cahier J5, L2) :
//! - par affectation, `w_a = round_div(d_a × u_a, 10 000)` et
//!   `d_a = round_div(w_a × 10 000, u_a)` (R-170) ; une affectation à
//!   unités nulles n'a pas de `d_a` propre (le moteur lui donne la durée de
//!   la tâche, R-181) ;
//! - quand la durée de la tâche est recalculée (unités fixes ou travail
//!   fixe), elle vaut le plus grand des `d_a` ; quand la règle la garde
//!   (durée fixe, durée saisie, première affectation, ajout ou retrait sans
//!   pilotage par l'effort), elle est rendue telle quelle ;
//! - quand une somme est conservée, chaque part est arrondie (R-001) et le
//!   reste va à la dernière affectation par UID ; si ce reste devenait
//!   négatif (poids minuscule en dernier), les parts sont prises par défaut
//!   (plancher) et le reste, positif, va encore à la dernière ;
//! - le travail rendu est toujours la somme des affectations, sauf pour un
//!   changement de type ou de pilotage, qui ne recalcule rien.

use crate::duration::{Duration, Minutes, Work};
use crate::fixed::round_div;
use crate::ids::AssignmentUid;
use crate::model::TaskType;
use crate::quantity::Units;

/// Côté tâche de l'équilibrage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskSide {
    /// Type de tâche (S41).
    pub task_type: TaskType,
    /// Pilotée par l'effort (S42).
    pub effort_driven: bool,
    /// Durée, minutes ouvrées.
    pub duration: Minutes,
    /// Travail total.
    pub work: Work,
    /// Récapitulative (jamais pilotée, R-193).
    pub summary: bool,
}

/// Affectation d'une ressource de travail (les ressources matérielles et
/// de coût n'entrent pas dans l'équilibrage).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Part {
    /// UID de l'affectation.
    pub uid: AssignmentUid,
    /// Unités (1/10 000).
    pub units: Units,
    /// Travail.
    pub work: Work,
}

/// Modification demandée.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    /// Nouvelle durée de la tâche.
    Duration(Minutes),
    /// Nouveau travail total de la tâche.
    Work(Work),
    /// Nouvelles unités d'une affectation.
    Units {
        /// Affectation.
        uid: AssignmentUid,
        /// Unités.
        units: Units,
    },
    /// Nouveau travail d'une affectation.
    AssignmentWork {
        /// Affectation.
        uid: AssignmentUid,
        /// Travail.
        work: Work,
    },
    /// Ajout d'une affectation (R-190, R-192).
    Add {
        /// UID de l'affectation créée.
        uid: AssignmentUid,
        /// Unités.
        units: Units,
    },
    /// Retrait d'une affectation (R-191, R-192).
    Remove(AssignmentUid),
    /// Changement de type de tâche.
    Type(TaskType),
    /// Changement du pilotage par l'effort.
    EffortDriven(bool),
}

/// Grandeur recalculée par l'équilibrage (matrice S41, R-189).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recalculated {
    /// La durée.
    Duration,
    /// Le travail.
    Work,
    /// Les unités.
    Units,
    /// Rien.
    Nothing,
}

/// État équilibré.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rebalanced {
    /// Durée de la tâche.
    pub duration: Minutes,
    /// Travail total.
    pub work: Work,
    /// Affectations de travail, triées par UID.
    pub parts: Vec<Part>,
    /// Grandeur recalculée.
    pub recalculated: Recalculated,
}

/// Modification refusée.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    /// Tâche à travail fixe non pilotée par l'effort (S41 : « fixed-work
    /// tasks … always effort-driven », R-193).
    #[error("une tâche à travail fixe est toujours pilotée par l'effort")]
    FixedWorkEffortDriven,
    /// Unités nulles sur une tâche à travail fixe (R-193).
    #[error("unités nulles refusées sur une tâche à travail fixe")]
    ZeroUnitsFixedWork,
    /// Durée, travail ou unités d'une récapitulative (R-193).
    #[error("une récapitulative ne s'équilibre pas")]
    SummaryNotEditable,
    /// Affectation désignée absente.
    #[error("affectation {0} inconnue")]
    UnknownAssignment(AssignmentUid),
    /// Ajout d'une affectation dont l'UID existe déjà (ajout du lot L2 au
    /// contrat L0 : aucune autre variante ne le décrit).
    #[error("affectation {0} déjà présente")]
    DuplicateAssignment(AssignmentUid),
    /// Résultat hors des bornes (travail, durée, unités).
    #[error("résultat hors des bornes")]
    Overflow,
}

impl EditError {
    /// Code stable.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::FixedWorkEffortDriven => "edit.fixed_work_effort_driven",
            Self::ZeroUnitsFixedWork => "edit.zero_units_fixed_work",
            Self::SummaryNotEditable => "edit.summary_not_editable",
            Self::UnknownAssignment(_) => "edit.unknown_assignment",
            Self::DuplicateAssignment(_) => "edit.duplicate_assignment",
            Self::Overflow => "edit.overflow",
        }
    }
}

/// Pilotage par l'effort effectif : une tâche à travail fixe l'est toujours
/// (R-193), quel que soit l'indicateur enregistré. Les commandes s'en
/// servent pour écrire l'indicateur après `Edit::Type(FixedWork)`.
#[must_use]
pub const fn effective_effort_driven(task_type: TaskType, effort_driven: bool) -> bool {
    effort_driven || matches!(task_type, TaskType::FixedWork)
}

/// Échelle des unités : 10 000 = 100 % (R-002).
const UNIT_SCALE: i128 = 10_000;

/// Affectation en cours de calcul, en entiers bruts.
#[derive(Clone, Copy, Debug)]
struct Raw {
    uid: AssignmentUid,
    units: i64,
    work: i64,
}

/// Règle de durée de la tâche à la sortie.
#[derive(Clone, Copy, Debug)]
enum TaskDuration {
    /// Gardée (ou saisie) : rendue telle quelle.
    Held(i64),
    /// Recalculée : plus grand des `d_a`, cette valeur à défaut
    /// d'affectation à unités non nulles.
    Recalculated(i64),
}

/// `w = round_div(d × u, 10 000)` (R-170).
fn work_from(duration: i64, units: i64) -> Result<i64, EditError> {
    let num = i128::from(duration)
        .checked_mul(i128::from(units))
        .ok_or(EditError::Overflow)?;
    round_div(num, UNIT_SCALE).ok_or(EditError::Overflow)
}

/// `d = round_div(w × 10 000, u)` (R-170) ; `None` pour des unités nulles ;
/// au-delà du domaine des durées : [`EditError::Overflow`].
fn duration_from(work: i64, units: i64) -> Result<Option<i64>, EditError> {
    if units == 0 {
        return Ok(None);
    }
    let num = i128::from(work)
        .checked_mul(UNIT_SCALE)
        .ok_or(EditError::Overflow)?;
    let d = round_div(num, i128::from(units)).ok_or(EditError::Overflow)?;
    if !(0..=Duration::MAX_MINUTES).contains(&d) {
        return Err(EditError::Overflow);
    }
    Ok(Some(d))
}

/// `u = round_div(w × 10 000, d)` ; durée nulle : unités gardées si le
/// travail est nul, sinon [`EditError::Overflow`].
fn units_from(work: i64, duration: i64, current: i64) -> Result<i64, EditError> {
    if duration == 0 {
        return if work == 0 {
            Ok(current)
        } else {
            Err(EditError::Overflow)
        };
    }
    let num = i128::from(work)
        .checked_mul(UNIT_SCALE)
        .ok_or(EditError::Overflow)?;
    round_div(num, i128::from(duration)).ok_or(EditError::Overflow)
}

/// Somme vérifiée.
fn sum(mut values: impl Iterator<Item = i64>) -> Result<i64, EditError> {
    values.try_fold(0_i64, |acc, v| acc.checked_add(v).ok_or(EditError::Overflow))
}

/// Complète `heads` (toutes les parts sauf la dernière) par le reste
/// `total − Σ heads` ; `None` si ce reste est négatif.
fn with_remainder(total: i64, mut heads: Vec<i64>) -> Result<Option<Vec<i64>>, EditError> {
    let rest = total
        .checked_sub(sum(heads.iter().copied())?)
        .ok_or(EditError::Overflow)?;
    if rest < 0 {
        return Ok(None);
    }
    heads.push(rest);
    Ok(Some(heads))
}

/// Répartit `total` (≥ 0) au prorata de `weights` (≥ 0, somme > 0) : parts
/// arrondies (R-001), reste à la dernière ; repli sur des parts par défaut
/// si le reste devenait négatif.
fn prorata(total: i64, weights: &[i64]) -> Result<Vec<i64>, EditError> {
    let Some((_, head)) = weights.split_last() else {
        return Ok(Vec::new());
    };
    let total_w = i128::from(sum(weights.iter().copied())?);
    let share = |w: i64, floor: bool| -> Result<i64, EditError> {
        let num = i128::from(total)
            .checked_mul(i128::from(w))
            .ok_or(EditError::Overflow)?;
        if floor {
            let q = num.checked_div(total_w).ok_or(EditError::Overflow)?;
            i64::try_from(q).map_err(|_| EditError::Overflow)
        } else {
            round_div(num, total_w).ok_or(EditError::Overflow)
        }
    };
    let rounded = head
        .iter()
        .map(|w| share(*w, false))
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(parts) = with_remainder(total, rounded)? {
        return Ok(parts);
    }
    let floored = head
        .iter()
        .map(|w| share(*w, true))
        .collect::<Result<Vec<_>, _>>()?;
    with_remainder(total, floored)?.ok_or(EditError::Overflow)
}

/// Poids de répartition : `primary` si sa somme est positive, sinon
/// `secondary`, sinon parts égales.
fn weights(raw: &[Raw], primary: fn(&Raw) -> i64, secondary: fn(&Raw) -> i64) -> Result<Vec<i64>, EditError> {
    for pick in [primary, secondary] {
        let w: Vec<i64> = raw.iter().map(pick).collect();
        if sum(w.iter().copied())? > 0 {
            return Ok(w);
        }
    }
    Ok(vec![1; raw.len()])
}

/// Répartit `total` sur `raw` (prorata de `weights`), en remplaçant (`add`
/// faux) ou en ajoutant au travail existant.
fn spread(raw: &mut [Raw], total: i64, weights: &[i64], add: bool) -> Result<(), EditError> {
    let shares = prorata(total, weights)?;
    for (r, s) in raw.iter_mut().zip(shares) {
        r.work = if add {
            r.work.checked_add(s).ok_or(EditError::Overflow)?
        } else {
            s
        };
    }
    Ok(())
}

/// Durée fixe : unités recalculées depuis le travail et la durée gardée.
fn units_from_held(raw: &mut [Raw], duration: i64) -> Result<(), EditError> {
    for r in raw.iter_mut() {
        r.units = units_from(r.work, duration, r.units)?;
    }
    Ok(())
}

/// Assemble le résultat : durée de la tâche, contrôles de bornes, travail
/// total = Σ des affectations.
fn finish(
    task_type: TaskType,
    raw: &[Raw],
    duration: TaskDuration,
    recalculated: Recalculated,
) -> Result<Rebalanced, EditError> {
    if matches!(task_type, TaskType::FixedWork) && raw.iter().any(|r| r.units == 0) {
        return Err(EditError::ZeroUnitsFixedWork);
    }
    // Hors durée fixe, chaque d_a doit tenir dans le domaine des durées
    // (R-170), que la durée de la tâche soit gardée ou recalculée.
    let mut max = None;
    if !matches!(task_type, TaskType::FixedDuration) {
        for r in raw {
            if let Some(d) = duration_from(r.work, r.units)? {
                max = Some(max.map_or(d, |m: i64| m.max(d)));
            }
        }
    }
    let duration = match duration {
        TaskDuration::Held(d) => d,
        TaskDuration::Recalculated(fallback) => max.unwrap_or(fallback),
    };
    if !(0..=Duration::MAX_MINUTES).contains(&duration) {
        return Err(EditError::Overflow);
    }
    let parts = to_parts(raw)?;
    let total = sum(raw.iter().map(|r| r.work))?;
    Ok(Rebalanced {
        duration: Minutes::new(duration),
        work: Work::new(Minutes::new(total)).map_err(|_| EditError::Overflow)?,
        parts,
        recalculated,
    })
}

/// Affectations typées, bornes de R-002 et du travail contrôlées.
fn to_parts(raw: &[Raw]) -> Result<Vec<Part>, EditError> {
    raw.iter()
        .map(|r| {
            Ok(Part {
                uid: r.uid,
                units: Units::new(r.units).map_err(|_| EditError::Overflow)?,
                work: Work::new(Minutes::new(r.work)).map_err(|_| EditError::Overflow)?,
            })
        })
        .collect()
}

/// Position de `uid` dans `raw`.
fn position(raw: &[Raw], uid: AssignmentUid) -> Result<usize, EditError> {
    raw.iter()
        .position(|r| r.uid == uid)
        .ok_or(EditError::UnknownAssignment(uid))
}

/// Applique `edit` à la tâche `task` et à ses affectations de travail
/// `parts` (ordre quelconque, UID uniques) : nouvel état, trié par UID.
///
/// Matrice (R-189), pour la grandeur modifiée → grandeur recalculée :
/// unités fixes : unités → durée, durée → travail, travail → durée ; durée
/// fixe : unités → travail, durée → travail, travail → unités ; travail
/// fixe : unités → durée, durée → unités, travail → durée. Ajout et retrait :
/// R-190 à R-192 ; travail de la tâche : R-194.
///
/// # Errors
///
/// [`EditError::SummaryNotEditable`] pour toute édition d'une
/// récapitulative ; [`EditError::UnknownAssignment`],
/// [`EditError::DuplicateAssignment`] ; [`EditError::FixedWorkEffortDriven`]
/// et [`EditError::ZeroUnitsFixedWork`] (R-193) ; [`EditError::Overflow`]
/// pour une durée saisie hors de `[0, Duration::MAX_MINUTES]`, un `d_a` hors
/// de ce domaine (R-170) ou un résultat hors des bornes de R-002 et du
/// travail (S11).
pub fn apply(task: &TaskSide, parts: &[Part], edit: &Edit) -> Result<Rebalanced, EditError> {
    if task.summary {
        return Err(EditError::SummaryNotEditable);
    }
    let mut raw: Vec<Raw> = parts
        .iter()
        .map(|p| Raw {
            uid: p.uid,
            units: p.units.get(),
            work: p.work.minutes().get(),
        })
        .collect();
    raw.sort_by_key(|r| r.uid);
    let kind = task.task_type;
    let effort = effective_effort_driven(kind, task.effort_driven);
    let d0 = task.duration.get();
    match *edit {
        Edit::Duration(d) => {
            let d = d.get();
            if !(0..=Duration::MAX_MINUTES).contains(&d) {
                return Err(EditError::Overflow);
            }
            let recalculated = if raw.is_empty() {
                Recalculated::Nothing
            } else if matches!(kind, TaskType::FixedWork) {
                units_from_held(&mut raw, d)?;
                Recalculated::Units
            } else {
                for r in &mut raw {
                    r.work = work_from(d, r.units)?;
                }
                Recalculated::Work
            };
            finish(kind, &raw, TaskDuration::Held(d), recalculated)
        }
        Edit::Work(w) => {
            if raw.is_empty() {
                // Sans affectation de travail, le travail reste nul (R-194).
                return finish(kind, &raw, TaskDuration::Held(d0), Recalculated::Nothing);
            }
            let weights = weights(&raw, |r| r.work, |r| r.units)?;
            spread(&mut raw, w.minutes().get(), &weights, false)?;
            rebalance_after_work(kind, &mut raw, d0)
        }
        Edit::Units { uid, units } => {
            let i = position(&raw, uid)?;
            let units = units.get();
            if matches!(kind, TaskType::FixedWork) && units == 0 {
                return Err(EditError::ZeroUnitsFixedWork);
            }
            let Some(r) = raw.get_mut(i) else {
                return Err(EditError::UnknownAssignment(uid));
            };
            r.units = units;
            if matches!(kind, TaskType::FixedDuration) {
                r.work = work_from(d0, units)?;
                finish(kind, &raw, TaskDuration::Held(d0), Recalculated::Work)
            } else {
                finish(kind, &raw, TaskDuration::Recalculated(d0), Recalculated::Duration)
            }
        }
        Edit::AssignmentWork { uid, work } => {
            let i = position(&raw, uid)?;
            let Some(r) = raw.get_mut(i) else {
                return Err(EditError::UnknownAssignment(uid));
            };
            r.work = work.minutes().get();
            rebalance_after_work(kind, &mut raw, d0)
        }
        Edit::Add { uid, units } => {
            if raw.iter().any(|r| r.uid == uid) {
                return Err(EditError::DuplicateAssignment(uid));
            }
            let units = units.get();
            if matches!(kind, TaskType::FixedWork) && units == 0 {
                return Err(EditError::ZeroUnitsFixedWork);
            }
            add(kind, effort, raw, uid, units, d0)
        }
        Edit::Remove(uid) => {
            let i = position(&raw, uid)?;
            let removed = raw.remove(i);
            remove(kind, effort, raw, removed.work, d0)
        }
        Edit::Type(t) => {
            if matches!(t, TaskType::FixedWork) && raw.iter().any(|r| r.units == 0) {
                return Err(EditError::ZeroUnitsFixedWork);
            }
            unchanged(task, &raw)
        }
        Edit::EffortDriven(flag) => {
            if !flag && matches!(kind, TaskType::FixedWork) {
                return Err(EditError::FixedWorkEffortDriven);
            }
            unchanged(task, &raw)
        }
    }
}

/// Changement de type ou de pilotage : rien n'est recalculé (R-189).
fn unchanged(task: &TaskSide, raw: &[Raw]) -> Result<Rebalanced, EditError> {
    Ok(Rebalanced {
        duration: task.duration,
        work: task.work,
        parts: to_parts(raw)?,
        recalculated: Recalculated::Nothing,
    })
}

/// Après une saisie de travail (tâche ou affectation) : durée fixe → unités ;
/// unités fixes et travail fixe → durée (R-189, R-194).
fn rebalance_after_work(kind: TaskType, raw: &mut [Raw], d0: i64) -> Result<Rebalanced, EditError> {
    if matches!(kind, TaskType::FixedDuration) {
        units_from_held(raw, d0)?;
        finish(kind, raw, TaskDuration::Held(d0), Recalculated::Units)
    } else {
        finish(kind, raw, TaskDuration::Recalculated(d0), Recalculated::Duration)
    }
}

/// Ajout d'une affectation (R-190, R-192).
fn add(
    kind: TaskType,
    effort: bool,
    mut raw: Vec<Raw>,
    uid: AssignmentUid,
    units: i64,
    d0: i64,
) -> Result<Rebalanced, EditError> {
    let first = raw.is_empty();
    let total = sum(raw.iter().map(|r| r.work))?;
    raw.push(Raw { uid, units, work: 0 });
    raw.sort_by_key(|r| r.uid);
    if first || !effort {
        // Première affectation (exemptée, S42) ou tâche non pilotée : durée
        // gardée, travail de la nouvelle = D × u.
        let i = position(&raw, uid)?;
        if let Some(r) = raw.get_mut(i) {
            r.work = work_from(d0, units)?;
        }
        return finish(kind, &raw, TaskDuration::Held(d0), Recalculated::Work);
    }
    if matches!(kind, TaskType::FixedDuration) {
        // Durée gardée ; travail réparti au prorata des unités demandées,
        // unités recalculées (S43 : « each resource will be assigned less of
        // the total work »).
        let weights = weights(&raw, |r| r.units, |r| r.work)?;
        spread(&mut raw, total, &weights, false)?;
        units_from_held(&mut raw, d0)?;
        return finish(kind, &raw, TaskDuration::Held(d0), Recalculated::Units);
    }
    // Unités fixes et travail fixe : D = W × 10 000 / Σ U et w_a = D × u_a,
    // calculé sans arrondir D d'abord : w_a = W × u_a / Σ U (R-001), reste à
    // la dernière ; la durée rendue est le plus grand des d_a. Arrondir D
    // avant de multiplier amplifierait l'erreur d'un facteur u_a / 10 000.
    let total_units = sum(raw.iter().map(|r| r.units))?;
    if total_units == 0 {
        // Unités toutes nulles : rien à répartir, durée gardée.
        return finish(kind, &raw, TaskDuration::Held(d0), Recalculated::Nothing);
    }
    let weights: Vec<i64> = raw.iter().map(|r| r.units).collect();
    spread(&mut raw, total, &weights, false)?;
    finish(kind, &raw, TaskDuration::Recalculated(d0), Recalculated::Duration)
}

/// Retrait d'une affectation (R-191, R-192) ; `raw` est déjà privé de
/// l'affectation retirée, dont le travail est `removed`.
fn remove(
    kind: TaskType,
    effort: bool,
    mut raw: Vec<Raw>,
    removed: i64,
    d0: i64,
) -> Result<Rebalanced, EditError> {
    if raw.is_empty() || !effort {
        // Dernière retirée (tâche sans affectation, W = 0) ou tâche non
        // pilotée : durée gardée.
        return finish(kind, &raw, TaskDuration::Held(d0), Recalculated::Work);
    }
    let weights = weights(&raw, |r| r.units, |r| r.work)?;
    spread(&mut raw, removed, &weights, true)?;
    rebalance_after_work(kind, &mut raw, d0)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{Edit, EditError, Part, Recalculated, TaskSide, apply, prorata};
    use crate::duration::{Minutes, Work};
    use crate::ids::AssignmentUid;
    use crate::model::TaskType;
    use crate::quantity::Units;

    fn part(uid: u32, work: i64) -> Part {
        Part {
            uid: AssignmentUid::new(uid),
            units: Units::FULL,
            work: Work::new(Minutes::new(work)).unwrap(),
        }
    }

    #[test]
    fn contract_apply_skeleton() {
        let task = TaskSide {
            task_type: TaskType::FixedUnits,
            effort_driven: false,
            duration: Minutes::new(480),
            work: Work::new(Minutes::new(960)).unwrap(),
            summary: false,
        };
        let parts = [part(5, 480), part(2, 480)];
        let out = apply(&task, &parts, &Edit::EffortDriven(true)).unwrap();
        assert_eq!(out.parts, [part(2, 480), part(5, 480)]);
        assert_eq!(out.recalculated, Recalculated::Nothing);
        let missing = AssignmentUid::new(9);
        assert_eq!(
            apply(&task, &parts, &Edit::Remove(missing)),
            Err(EditError::UnknownAssignment(missing))
        );
        assert_eq!(EditError::Overflow.code(), "edit.overflow");
    }

    #[test]
    fn prorata_remainder_goes_last_and_never_negative() {
        // 10 sur trois parts égales : 3, 3, reste 4.
        assert_eq!(prorata(10, &[1, 1, 1]).unwrap(), [3, 3, 4]);
        // 1 sur (1, 1, 0) : arrondi 1, 1 → reste −1 ; repli plancher 0, 0, 1.
        assert_eq!(prorata(1, &[1, 1, 0]).unwrap(), [0, 0, 1]);
        assert_eq!(prorata(5, &[]).unwrap(), Vec::<i64>::new());
    }

    #[test]
    fn duplicate_add_refused() {
        let task = TaskSide {
            task_type: TaskType::FixedUnits,
            effort_driven: true,
            duration: Minutes::new(480),
            work: Work::new(Minutes::new(480)).unwrap(),
            summary: false,
        };
        let uid = AssignmentUid::new(2);
        assert_eq!(
            apply(
                &task,
                &[part(2, 480)],
                &Edit::Add {
                    uid,
                    units: Units::FULL
                }
            ),
            Err(EditError::DuplicateAssignment(uid))
        );
        assert_eq!(
            EditError::DuplicateAssignment(uid).code(),
            "edit.duplicate_assignment"
        );
    }

    #[test]
    fn negative_duration_refused() {
        let task = TaskSide {
            task_type: TaskType::FixedDuration,
            effort_driven: false,
            duration: Minutes::new(480),
            work: Work::ZERO,
            summary: false,
        };
        assert_eq!(
            apply(&task, &[], &Edit::Duration(Minutes::new(-1))),
            Err(EditError::Overflow)
        );
    }
}
