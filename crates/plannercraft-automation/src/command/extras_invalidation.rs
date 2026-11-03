//! Invalidation des `Extras` d'une entité modifiée (R-161) : table des clés
//! et jet, partagés par les commandes de tâches, de liens, de contraintes et
//! de calendriers.
//!
//! Les `Extras` gardent tels quels des éléments MSPDI hors modèle. Certains
//! dépendent des dates calculées par MS Project (répartition dans le temps,
//! nivellement, interruption) : ils deviennent faux dès qu'une commande
//! change ce dont ces dates dépendent, et sont alors jetés, chaque jet
//! signalé par `SideEffect::ExtrasDropped`. Table figée par un test contre
//! `plannercraft_mspdi::schema`.
//!
//! R-161 amendée (J5, lot L7) : `LevelingDelay` et `LevelingDelayFormat`
//! sortent de la table. Le retard de nivellement est une entrée du modèle
//! (ADR-0013 D4), gardé à l'édition comme MS Project garde un retard posé ;
//! un `Extras` hérité d'avant J5 l'est aussi (l'écrivain le réécrit tant que
//! le modèle n'a pas de retard, R-198). Une affectation dont les unités, le
//! travail, le délai ou la ressource changent perd sa répartition.

use plannercraft_core::{AssignmentUid, Subject, TaskUid};
use plannercraft_mspdi::extras::ORIGIN;

use super::exec::Ctx;
use crate::outcome::SideEffect;

/// Champ (ou groupe de champs) d'une tâche modifié par une commande.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TaskChange {
    /// Nom.
    Name,
    /// Notes.
    Notes,
    /// Durée.
    Duration,
    /// Contrainte (début ou fin saisis, R-158).
    Constraint,
    /// Drapeau jalon.
    Milestone,
    /// Tâche active.
    Active,
    /// Liens de la tâche (prédécesseurs ajoutés, modifiés ou retirés).
    Links,
    /// Place dans la hiérarchie (niveau, parent, récapitulative ou non).
    Outline,
    /// Type de tâche (la durée fixe place sur la durée de la tâche, R-170).
    Type,
    /// Travail total de la tâche (R-194, R-197).
    Work,
    /// Pilotage par l'effort : sans effet sur les dates.
    EffortDriven,
    /// Avancement (J6, `progress.update`, `progress.reschedule`, R-161
    /// amendée) : interruption de la tâche.
    Progress,
    /// Mode de planification ou valeurs manuelles (J8, R-253, R-254).
    Mode,
}

/// Éléments de tâche MSPDI qui dépendent des dates calculées : répartition
/// (`TimephasedData`), dates d'avant nivellement (`PreLeveledStart`,
/// `PreLeveledFinish`), interruption (`Resume`, `Stop`). `LevelingDelay` et
/// `LevelingDelayFormat` n'en sont plus (R-161 amendée, J5) : entrées du
/// modèle.
pub(crate) const TASK_SCHEDULE_KEYS: &[&str] = &[
    "TimephasedData",
    "PreLeveledStart",
    "PreLeveledFinish",
    "Resume",
    "Stop",
];

/// Éléments de tâche MSPDI jetés quand son avancement change (R-161
/// amendée, J6) : interruption calculée par MS Project (`Stop`, `Resume`,
/// `ResumeValid`) ; la reprise saisie est une entrée du modèle
/// (`Progress::resume`, R-204).
pub(crate) const TASK_PROGRESS_KEYS: &[&str] = &["Resume", "ResumeValid", "Stop"];

/// Types de `TimephasedData` d'une affectation qui portent son travail réel
/// (S37 : 2 et 3) ; jetés quand son travail réel change (R-161 amendée,
/// J6), gardés sinon.
pub(crate) const ACTUAL_TIMEPHASED_TYPES: &[i64] = &[2, 3];

/// Éléments d'affectation MSPDI jetés quand la durée de sa tâche change,
/// qu'un calendrier qui la planifie change (R-165), ou que ses unités, son
/// travail, son délai ou sa ressource changent (R-161 amendée, J5) :
/// répartition.
pub(crate) const ASSIGNMENT_SCHEDULE_KEYS: &[&str] = &["TimephasedData"];

/// R-161 : clés d'`Extras` de la tâche jetées quand `change` est modifié.
pub(crate) const fn task_keys(change: TaskChange) -> &'static [&'static str] {
    match change {
        TaskChange::Name | TaskChange::Notes | TaskChange::EffortDriven => &[],
        TaskChange::Duration
        | TaskChange::Type
        | TaskChange::Work
        | TaskChange::Constraint
        | TaskChange::Milestone
        | TaskChange::Active
        | TaskChange::Links
        | TaskChange::Outline
        | TaskChange::Mode => TASK_SCHEDULE_KEYS,
        TaskChange::Progress => TASK_PROGRESS_KEYS,
    }
}

/// Type (`<Type>n</Type>`, enfant direct, préfixe d'espace de noms admis)
/// d'une ligne `TimephasedData` gardée en `Extras` ; `None` si absent ou
/// illisible. Lecture sans analyseur XML : les enfants d'une ligne sont
/// plats (S5 : `Type`, `UID`, `Start`, `Finish`, `Unit`, `Value`).
fn timephased_type(payload: &str) -> Option<i64> {
    let mut rest = payload;
    while let Some(at) = rest.find("Type>") {
        let (head, tail) = rest.split_at(at);
        rest = tail.get("Type>".len()..).unwrap_or("");
        // Balise ouvrante `<Type>` ou `<p:Type>` (jamais `</Type>`).
        let open = head.rfind('<').and_then(|lt| head.get(lt.saturating_add(1)..));
        let Some(name) = open else {
            continue;
        };
        if name.starts_with('/') || !(name.is_empty() || name.ends_with(':')) {
            continue;
        }
        let text = rest.split('<').next().unwrap_or("");
        return text.trim().parse().ok();
    }
    None
}

/// Retire de `extras` les entrées MSPDI dont la clé est dans `keys` ; rend
/// les clés retirées, triées, sans doublon. Seule implémentation du jet
/// (tâches, affectations, calendriers).
pub(super) fn drop_keys(extras: &mut plannercraft_core::Extras, keys: &[&str]) -> Vec<String> {
    let mut dropped = Vec::new();
    extras.0.retain(|e| {
        let hit = e.origin == ORIGIN && keys.contains(&e.key.as_str());
        if hit {
            dropped.push(e.key.clone());
        }
        !hit
    });
    dropped.sort();
    dropped.dedup();
    dropped
}

impl Ctx<'_> {
    /// R-161 : jette les `Extras` de la tâche `uid` que `change` invalide
    /// et signale le jet ; sans effet si rien n'est jeté.
    pub(crate) fn drop_task_extras(&mut self, uid: TaskUid, change: TaskChange) {
        let keys = task_keys(change);
        if keys.is_empty() {
            return;
        }
        let Some(task) = self.project.tasks.get_mut(uid) else {
            return;
        };
        let dropped = drop_keys(&mut task.extras, keys);
        if !dropped.is_empty() {
            self.effects.push(SideEffect::ExtrasDropped {
                subject: Subject::Task(uid),
                keys: dropped,
            });
        }
    }

    /// R-161 : la durée de la tâche `uid` a changé ; jette la répartition
    /// de chacune de ses affectations (un effet par affectation touchée).
    pub(crate) fn drop_assignment_extras(&mut self, uid: TaskUid) {
        let touched: Vec<_> = self
            .project
            .assignments
            .iter()
            .filter(|a| a.task == uid && !a.extras.is_empty())
            .map(plannercraft_core::HasUid::uid)
            .collect();
        for a in touched {
            self.drop_one_assignment_extras(a);
        }
    }

    /// R-161 (amendée, J6) : le travail réel de l'affectation `uid` a
    /// changé ; jette ses lignes `TimephasedData` de travail réel (types 2
    /// et 3, [`ACTUAL_TIMEPHASED_TYPES`]) et signale le jet ; les autres
    /// lignes (type 1, lignes de base `Baseline#n/`) restent.
    pub(crate) fn drop_actual_timephased(&mut self, uid: AssignmentUid) {
        let Some(assignment) = self.project.assignments.get_mut(uid) else {
            return;
        };
        let mut dropped = false;
        assignment.extras.0.retain(|e| {
            let hit = e.origin == ORIGIN
                && e.key == "TimephasedData"
                && timephased_type(&e.payload).is_some_and(|t| ACTUAL_TIMEPHASED_TYPES.contains(&t));
            dropped |= hit;
            !hit
        });
        if dropped {
            self.effects.push(SideEffect::ExtrasDropped {
                subject: Subject::Assignment(uid),
                keys: vec!["TimephasedData".to_owned()],
            });
        }
    }

    /// R-161 (amendée, J5) : les unités, le travail, le délai ou la
    /// ressource de l'affectation `uid` ont changé ; jette sa répartition et
    /// signale le jet ; sans effet si rien n'est jeté.
    pub(crate) fn drop_one_assignment_extras(&mut self, uid: AssignmentUid) {
        let Some(assignment) = self.project.assignments.get_mut(uid) else {
            return;
        };
        let dropped = drop_keys(&mut assignment.extras, ASSIGNMENT_SCHEDULE_KEYS);
        if !dropped.is_empty() {
            self.effects.push(SideEffect::ExtrasDropped {
                subject: Subject::Assignment(uid),
                keys: dropped,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use plannercraft_mspdi::schema::{Parent, sequence};

    use super::{
        ASSIGNMENT_SCHEDULE_KEYS, TASK_PROGRESS_KEYS, TASK_SCHEDULE_KEYS, TaskChange, task_keys,
        timephased_type,
    };

    #[test]
    fn r161_table_matches_mspdi_schema() {
        // Chaque clé jetée est un élément de la séquence MSPDI de son
        // entité : une faute de frappe ne jetterait jamais rien.
        for key in TASK_SCHEDULE_KEYS {
            assert!(sequence(Parent::Task).slot(key).is_some(), "Task/{key}");
        }
        for key in TASK_PROGRESS_KEYS {
            assert!(sequence(Parent::Task).slot(key).is_some(), "Task/{key}");
        }
        for key in ASSIGNMENT_SCHEDULE_KEYS {
            assert!(
                sequence(Parent::Assignment).slot(key).is_some(),
                "Assignment/{key}"
            );
        }
        // Table figée.
        assert_eq!(task_keys(TaskChange::Name), [] as [&str; 0]);
        assert_eq!(task_keys(TaskChange::Notes), [] as [&str; 0]);
        assert_eq!(task_keys(TaskChange::EffortDriven), [] as [&str; 0]);
        for c in [
            TaskChange::Type,
            TaskChange::Work,
            TaskChange::Duration,
            TaskChange::Constraint,
            TaskChange::Milestone,
            TaskChange::Active,
            TaskChange::Links,
            TaskChange::Outline,
        ] {
            assert_eq!(task_keys(c), TASK_SCHEDULE_KEYS, "{c:?}");
        }
        assert_eq!(
            TASK_SCHEDULE_KEYS,
            [
                "TimephasedData",
                "PreLeveledStart",
                "PreLeveledFinish",
                "Resume",
                "Stop"
            ]
        );
        assert_eq!(ASSIGNMENT_SCHEDULE_KEYS, ["TimephasedData"]);
        assert_eq!(task_keys(TaskChange::Progress), ["Resume", "ResumeValid", "Stop"]);
    }

    #[test]
    fn r161_timephased_type_read() {
        let row = |t: &str| {
            format!("<TimephasedData><Type>{t}</Type><UID>1</UID><Start>x</Start></TimephasedData>")
        };
        assert_eq!(timephased_type(&row("2")), Some(2));
        assert_eq!(timephased_type(&row(" 3 ")), Some(3));
        assert_eq!(timephased_type(&row("x")), None);
        assert_eq!(
            timephased_type("<p:TimephasedData><p:Type>1</p:Type></p:TimephasedData>"),
            Some(1)
        );
        assert_eq!(
            timephased_type("<TimephasedData><UID>2</UID></TimephasedData>"),
            None
        );
        assert_eq!(
            timephased_type("<TimephasedData><SubType>2</SubType></TimephasedData>"),
            None
        );
        assert_eq!(timephased_type(""), None);
    }
}
