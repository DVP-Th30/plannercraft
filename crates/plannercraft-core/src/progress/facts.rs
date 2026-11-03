//! Faits d'avancement d'une tâche (R-200, partie faits ; contrat J6, lot
//! L0) et restes dérivés (R-200, R-211 ; ADR-0014 D4).
//!
//! La vérité est le réel : durée et durée réelle pour une tâche, travail et
//! travail réel pour une affectation. Les restes et le % de durée en
//! découlent ; les champs `remaining_duration`, `remaining_work` et
//! `percent_complete` stockés ne sont pas relus ici (un reste relu
//! incohérent est signalé et remplacé par le lecteur MSPDI, R-214).

use serde::{Deserialize, Serialize};

use crate::duration::{Minutes, Work, percent_of};
use crate::fixed::round_div;
use crate::model::{Assignment, Task};
use crate::quantity::{Percent, SignedPercent};
use crate::time::Moment;

/// État d'avancement d'une tâche (R-200).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressState {
    /// Non commencée : ni début réel, ni durée réelle, ni %.
    NotStarted,
    /// Commencée, non achevée.
    Started,
    /// Achevée : fin réelle saisie.
    Complete,
}

/// Faits d'avancement d'une tâche, lus sur sa saisie (R-200).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProgressFacts {
    /// État.
    pub state: ProgressState,
    /// Début réel saisi ; `None` pour une tâche commencée sans début réel
    /// (`implicit_start`) : le moteur prend son début au plus tôt.
    pub actual_start: Option<Moment>,
    /// Fin réelle saisie.
    pub actual_finish: Option<Moment>,
    /// Durée réelle effective, ≥ 0 ; au plus la durée sauf pour une tâche
    /// achevée.
    pub actual_duration: Minutes,
    /// Commencée sans début réel : début réel implicite
    /// (`progress.actual_start_missing`).
    pub implicit_start: bool,
}

/// R-200 : faits d'avancement de `task`.
///
/// - achevée ⇔ fin réelle saisie ; commencée ⇔ début réel, durée réelle
///   > 0 ou % > 0 ; non commencée sinon ;
/// - durée réelle effective : la durée réelle saisie si elle est > 0, sinon
///   `percent_of(D, %)` (R-010) ; tâche achevée sans durée réelle saisie :
///   D (hypothèse : achevée ⇒ 100 %) ; non achevée : bornée à `[0, D]` ;
/// - début réel implicite : commencée sans début réel.
///
/// Exemples : D 1 440, 3 333 sans durée réelle ⇒ réel 480 ; D 2 400, réel
/// 960 ⇒ commencée.
#[must_use]
pub fn facts(task: &Task) -> ProgressFacts {
    let p = &task.progress;
    let d = task.duration.minutes().max(Minutes::ZERO);
    let entered = p.actual_duration.max(Minutes::ZERO);
    let state = if p.actual_finish.is_some() {
        ProgressState::Complete
    } else if p.actual_start.is_some() || entered > Minutes::ZERO || p.percent_complete > Percent::ZERO {
        ProgressState::Started
    } else {
        ProgressState::NotStarted
    };
    let actual_duration = match state {
        ProgressState::NotStarted => Minutes::ZERO,
        ProgressState::Complete if entered > Minutes::ZERO => entered,
        ProgressState::Complete => d,
        ProgressState::Started if entered > Minutes::ZERO => entered.min(d),
        // `percent_of` ne déborde pas : % ≤ 10 000, le résultat est ≤ D.
        ProgressState::Started => percent_of_minutes(d, p.percent_complete).min(d),
    };
    ProgressFacts {
        state,
        actual_start: p.actual_start,
        actual_finish: p.actual_finish,
        actual_duration,
        implicit_start: state != ProgressState::NotStarted && p.actual_start.is_none(),
    }
}

/// `percent_of(d, pct)` (R-010) ; débordement impossible (`pct` ≤ 10 000),
/// replié sur `d` par prudence (règle 10).
fn percent_of_minutes(d: Minutes, pct: Percent) -> Minutes {
    SignedPercent::new(pct.get())
        .ok()
        .and_then(|s| percent_of(d, s).ok())
        .unwrap_or(d)
}

/// R-200 : durée restante = D − durée réelle effective, ≥ 0 ; 0 pour une
/// tâche achevée.
///
/// Exemple : D 2 400, réel 960 ⇒ 1 440.
#[must_use]
pub fn remaining_duration(task: &Task) -> Minutes {
    let f = facts(task);
    if f.state == ProgressState::Complete {
        return Minutes::ZERO;
    }
    let d = task.duration.minutes().max(Minutes::ZERO);
    Minutes::new(d.get().saturating_sub(f.actual_duration.get()).max(0))
}

/// R-211 : travail restant d'une affectation = W − travail réel, ≥ 0.
///
/// Exemple : W 960, travail réel 480 ⇒ 480.
#[must_use]
pub fn remaining_work(a: &Assignment) -> Work {
    let rest = a
        .work
        .minutes()
        .get()
        .saturating_sub(a.actual_work.minutes().get())
        .max(0);
    // Toujours valide : 0 ≤ reste ≤ W ≤ maximum du travail.
    Work::new(Minutes::new(rest)).unwrap_or(Work::ZERO)
}

/// R-200 : % achevé dérivé = `round_div(réel × 10 000, D)`, borné à
/// `[0, 9 999]` pour une tâche non achevée (100 % ⇔ fin réelle, P39 : un
/// réel de D − 1 sur D > 20 000 s'arrondirait à 10 000) ; achevée : 10 000 ;
/// D = 0 : 10 000 si achevée, sinon 0.
///
/// Exemple : D 2 400, réel 960 ⇒ 4 000.
#[must_use]
pub fn percent_of_duration(task: &Task) -> Percent {
    let f = facts(task);
    if f.state == ProgressState::Complete {
        return Percent::FULL;
    }
    let d = task.duration.minutes().get();
    if d <= 0 {
        return Percent::ZERO;
    }
    let num = i128::from(f.actual_duration.get()).saturating_mul(10_000);
    round_div(num, i128::from(d))
        .map(|v| v.clamp(0, Percent::FULL.get().saturating_sub(1)))
        .and_then(|v| Percent::new(v).ok())
        .unwrap_or(Percent::ZERO)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{ProgressState, facts, percent_of_duration, remaining_duration, remaining_work};
    use crate::duration::{Duration, DurationUnit, Minutes, TimeBasis, Work};
    use crate::ids::{AssignmentUid, TaskUid};
    use crate::model::{Assignment, ProjectOptions, Task};
    use crate::quantity::Percent;

    fn task(d: i64) -> Task {
        let mut t = Task::new(TaskUid::new(1), "T".to_owned(), &ProjectOptions::default());
        t.duration = Duration::new(Minutes::new(d), DurationUnit::Day, TimeBasis::Working, false).unwrap();
        t
    }

    #[test]
    fn bounds_and_zero_duration() {
        // Durée réelle saisie au-delà de D sur une tâche en cours : bornée.
        let mut t = task(480);
        t.progress.actual_duration = Minutes::new(600);
        assert_eq!(facts(&t).actual_duration, Minutes::new(480));
        assert_eq!(remaining_duration(&t), Minutes::ZERO);
        // Non achevée : jamais 100 % sans fin réelle (P39).
        assert_eq!(percent_of_duration(&t), Percent::new(9_999).unwrap());
        let mut big = task(30_000);
        big.progress.actual_duration = Minutes::new(29_999);
        assert_eq!(percent_of_duration(&big), Percent::new(9_999).unwrap());
        // Jalon commencé, non achevé : 0 % ; achevé : 100 %.
        let mut m = task(0);
        m.progress.actual_start = Some("2026-10-19T08:00".parse().unwrap());
        assert_eq!(facts(&m).state, ProgressState::Started);
        assert_eq!(percent_of_duration(&m), Percent::ZERO);
        m.progress.actual_finish = m.progress.actual_start;
        assert_eq!(percent_of_duration(&m), Percent::FULL);
        assert_eq!(remaining_duration(&m), Minutes::ZERO);
    }

    #[test]
    fn remaining_work_derived() {
        let mut a = Assignment::new(AssignmentUid::new(1), TaskUid::new(1), None);
        a.work = Work::new(Minutes::new(960)).unwrap();
        a.actual_work = Work::new(Minutes::new(480)).unwrap();
        assert_eq!(remaining_work(&a), Work::new(Minutes::new(480)).unwrap());
        a.actual_work = Work::new(Minutes::new(1_200)).unwrap();
        assert_eq!(remaining_work(&a), Work::ZERO);
    }
}
