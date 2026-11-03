//! Types publics du planning calculé (ADR-0009, spécification J2 § 2.2).
//!
//! Les valeurs calculées vivent ici, jamais dans le modèle (ADR-0008) : un
//! `Schedule` est le résultat d'un appel à [`super::schedule`], immuable.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::calendar::Span;
use crate::diagnostic::Diagnostic;
use crate::duration::{Minutes, Work};
use crate::ids::{AssignmentUid, CalendarUid, ResourceUid, TaskUid};
use crate::model::{ConstraintKind, LinkType};
use crate::progress::ProgressState;
use crate::quantity::{Percent, Units};
use crate::time::Moment;

/// Planning calculé d'un projet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Schedule {
    /// Début du projet : depuis le début, l'ancre telle que saisie (brute,
    /// non recalée, R-036) ; à rebours, le début calculé `S*` (R-107 : min
    /// des débuts au plus tard des tâches actives planifiées, l'ancre `F` si
    /// rien n'est planifié).
    pub project_start: Moment,
    /// Fin du projet : depuis le début, R-045 (max des fins au plus tôt des
    /// tâches actives planifiées ; `project_start` si rien n'est planifié) ;
    /// à rebours, l'ancre `F` telle que saisie (brute, R-106).
    pub project_finish: Moment,
    /// R-068 : récapitulative du projet (UID 0), si une tâche est planifiée.
    pub project_summary: Option<TaskSchedule>,
    /// Une entrée par tâche du modèle (lignes vides comprises), UID 0 exclu.
    pub tasks: BTreeMap<TaskUid, TaskOutcome>,
    /// R-035 : ordre topologique stable des tâches planifiées (une
    /// récapitulative à la position de son agrégat).
    pub order: Vec<TaskUid>,
    /// Diagnostics (compilation des calendriers comprise), triés,
    /// dédoublonnés.
    pub diagnostics: Vec<Diagnostic>,
    /// Affectations placées (J5, R-171 à R-183), par UID ; absent du JSON
    /// quand vide.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub assignments: BTreeMap<AssignmentUid, AssignmentSchedule>,
    /// Charge des ressources (J5, R-184 à R-187), par UID ; absent du JSON
    /// quand vide.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub resources: BTreeMap<ResourceUid, ResourceLoad>,
}

impl Schedule {
    /// Résultat de la tâche `uid` (UID 0 : aucun, voir
    /// [`Schedule::scheduled`]).
    #[must_use]
    pub fn get(&self, uid: TaskUid) -> Option<&TaskOutcome> {
        self.tasks.get(&uid)
    }

    /// Résultat d'une tâche planifiée ; l'UID 0 donne la récapitulative du
    /// projet.
    #[must_use]
    pub fn scheduled(&self, uid: TaskUid) -> Option<&TaskSchedule> {
        if uid == TaskUid::PROJECT_SUMMARY {
            return self.project_summary.as_ref();
        }
        match self.tasks.get(&uid) {
            Some(TaskOutcome::Scheduled(s)) => Some(s),
            _ => None,
        }
    }

    /// Tâches critiques, dans l'ordre des UID (récapitulative du projet
    /// exclue).
    pub fn critical(&self) -> impl Iterator<Item = (TaskUid, &TaskSchedule)> {
        self.tasks.iter().filter_map(|(uid, o)| match o {
            TaskOutcome::Scheduled(s) if s.critical => Some((*uid, s)),
            _ => None,
        })
    }
}

/// Résultat d'une tâche : planifiée, ou exclue du calcul avec sa raison.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TaskOutcome {
    /// Tâche planifiée.
    Scheduled(TaskSchedule),
    /// Tâche exclue du calcul.
    Excluded {
        /// Raison.
        reason: Exclusion,
        /// R-062 : placement d'affichage d'une tâche inactive ; `None` sinon.
        display: Option<Span>,
    },
}

/// Raison d'une exclusion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Exclusion {
    /// Ligne vide (`IsNull`).
    NullRow,
    /// Tâche inactive (R-062).
    Inactive,
    /// Sous une récapitulative inactive (R-062).
    InactiveAncestor,
    /// Tâche manuelle sans début ni fin (R-063).
    ManualUndated,
    /// Récapitulative sans aucun enfant planifié (R-053).
    EmptySummary,
    /// Conversion impossible dans la passe principale : passe avant depuis
    /// le début (R-069), passe arrière à rebours (R-111).
    Unschedulable,
}

/// Valeurs calculées d'une tâche planifiée.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskSchedule {
    /// Feuille ou récapitulative.
    pub role: TaskRole,
    /// Tâche en mode manuel (dates saisies conservées, R-063, R-064).
    pub manual: bool,
    /// Début planifié (`Start` MSPDI) : `early.start` depuis le début,
    /// `late.start` à rebours (R-106) ; une tâche épinglée a `early ==
    /// late` (R-092, R-097, R-108) ; récapitulative et UID 0 : enveloppe des
    /// dates planifiées de leurs enfants (R-112).
    pub start: Moment,
    /// Fin planifiée (`Finish` MSPDI), comme `start`.
    pub finish: Moment,
    /// Dates au plus tôt.
    pub early: Span,
    /// Dates au plus tard.
    pub late: Span,
    /// Feuille : durée de la tâche (manuelle : durée utilisée, R-063) ;
    /// récapitulative : temps ouvré entre son début et sa fin (R-054).
    pub duration: Minutes,
    /// Marge totale, sur l'échelle de marge de la tâche (R-048) ; signée.
    pub total_slack: Minutes,
    /// Marge libre (R-049) ; signée.
    pub free_slack: Minutes,
    /// Critique (R-050).
    pub critical: bool,
    /// Feuille : copie de l'entrée ; récapitulative : R-055.
    pub percent_complete: Percent,
    /// Cause du début au plus tôt (base d'`explain_date`, J6).
    pub driver: Driver,
    /// Cause de la fin au plus tard (R-047 ; base d'`explain_date` à
    /// rebours et pour une tâche « le plus tard possible »).
    pub late_driver: LateDriver,
    /// Dates de repli : une conversion a échoué ici ou en amont (R-069).
    pub degraded: bool,
    /// Travail calculé (J5, R-184) : feuille, somme des affectations de
    /// travail ; récapitulative, somme des enfants et de ses affectations.
    /// Absent du JSON quand nul.
    #[serde(default, skip_serializing_if = "is_zero_work")]
    pub work: Work,
    /// Avancement calculé (J6, R-200, R-204, R-210) ; `None` pour une tâche
    /// non commencée sans agrégat. Absent du JSON quand il n'y en a pas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<ProgressSchedule>,
    /// Borne brute retenue pour le pilote, avant recalage sur le calendrier
    /// (J6, R-230, base d'`explain_date`) : date du lien converti, date de
    /// contrainte, début du projet ou borne de porte ; `None` pour
    /// `Manual`, `Rollup` et `Actual`. Absente du JSON quand elle manque.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver_bound: Option<Moment>,
}

/// Avancement calculé d'une tâche planifiée (J6, ADR-0014 D2, D4).
///
/// `stop` est toujours calculé, jamais stocké ; une reprise saisie est
/// stockée dans le modèle (`Progress::resume`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgressSchedule {
    /// État (R-200).
    pub state: ProgressState,
    /// Début réel, saisi ou implicite (R-200 : début au plus tôt du
    /// réseau) ; récapitulative : min des débuts réels (R-210).
    pub actual_start: Moment,
    /// Fin réelle (tâche achevée ; récapitulative dont toutes les feuilles
    /// le sont).
    pub actual_finish: Option<Moment>,
    /// Fin de la partie réelle (`Stop`, R-204) ; achevée : fin réelle.
    pub stop: Moment,
    /// Reprise du reste (`Resume`, R-204, R-205) ; achevée : fin réelle.
    pub resume: Moment,
    /// Durée réelle effective (R-200, R-210).
    pub actual_duration: Minutes,
    /// Durée restante = D − réel (R-200).
    pub remaining_duration: Minutes,
    /// % achevé dérivé (R-200, R-210), 1/10 000.
    pub percent_complete: Percent,
    /// % travail achevé (R-211, R-210), 1/10 000.
    pub percent_work_complete: Percent,
    /// Travail réel (R-210) : Σ travail réel des affectations de travail du
    /// périmètre de R-184 (feuille : ses propres affectations) ; relecture
    /// finale J6, J6-F2 : écrit `ActualWork` d'une récapitulative.
    #[serde(default, skip_serializing_if = "is_zero_work")]
    pub actual_work: Work,
}

/// Vrai pour un travail nul (champ omis du JSON).
#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "signature imposée par `skip_serializing_if`"
)]
fn is_zero_work(w: &Work) -> bool {
    *w == Work::ZERO
}

/// Nombre maximal d'intervalles de surutilisation gardés par ressource
/// ([`ResourceLoad::overloads`], règle 10) ; au-delà, `truncated`.
pub const MAX_OVERLOADS: usize = 1_000;

/// Affectation placée par le moteur (ADR-0013 D1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssignmentSchedule {
    /// Tâche affectée.
    pub task: TaskUid,
    /// Ressource ; `None` pour une affectation sans ressource (fictive
    /// −65 535 relue, O24).
    pub resource: Option<ResourceUid>,
    /// Rôle dans le placement de la tâche (R-176, R-179, R-183).
    pub role: AssignmentRole,
    /// Dates planifiées de l'affectation (`s_a`, `f_a`, R-173) : placement
    /// depuis le début planifié de la tâche, au plus tôt depuis le début, au
    /// plus tard à rebours (R-112).
    pub span: Span,
    /// Dates au plus tard (R-174) ; `None` si la passe arrière ne les donne
    /// pas.
    pub late: Option<Span>,
    /// Durée de l'affectation `d_a`, en minutes du calendrier effectif
    /// (R-170).
    pub duration: Minutes,
    /// Travail de l'affectation.
    pub work: Work,
    /// Unités effectives (débit : travail ÷ temps ouvré de l'intervalle,
    /// R-186 ; 1/10 000).
    pub effective_units: Units,
    /// Calendrier qui place l'affectation (R-171).
    pub calendar: EffectiveCalendar,
    /// Partie réelle de l'affectation (J6, R-209) : `span` pour une tâche
    /// achevée, [début réel, fin du réel] pour une tâche en cours ; `None`
    /// pour une tâche non commencée. Absente du JSON quand elle manque.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual: Option<Span>,
    /// Début du reste d'une affectation de tâche en cours (J6, R-209 :
    /// depuis `Resume`). Absent du JSON quand il manque.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_start: Option<Moment>,
}

/// Rôle d'une affectation dans le calcul.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentRole {
    /// Ressource de travail sur une feuille automatique : place la tâche
    /// (R-173).
    Work,
    /// Sans effet sur les dates : affectation fictive −65 535, ressource
    /// matérielle ou de coût, ressource inconnue (R-176).
    Neutral,
    /// Affectation d'une récapitulative (R-179).
    Summary,
    /// Affectation d'une tâche manuelle (R-183).
    Manual,
}

/// Calendrier effectif d'une affectation (R-171).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EffectiveCalendar {
    /// Calendrier de la tâche seul (la tâche ignore les calendriers de
    /// ressource, S48).
    Task(CalendarUid),
    /// Calendrier de la ressource seul (tâche sans calendrier propre).
    Resource(CalendarUid),
    /// Intersection du calendrier de la tâche et de celui de la ressource
    /// (R-172).
    Intersection {
        /// Calendrier de la tâche.
        task: CalendarUid,
        /// Calendrier de la ressource.
        resource: CalendarUid,
    },
    /// Calendrier du projet (ni la tâche ni la ressource n'en ont).
    Project(CalendarUid),
    /// Temps écoulé (tâche à durée écoulée, R-182).
    Elapsed,
}

/// Charge calculée d'une ressource (R-184, R-186, R-187).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceLoad {
    /// Travail total de ses affectations.
    pub work: Work,
    /// Pic d'unités effectives sur les intervalles travaillés (1/10 000).
    pub peak: Units,
    /// Surutilisée : un intervalle dépasse sa capacité (`max_units`).
    pub overallocated: bool,
    /// Intervalles de surutilisation, dans l'ordre (au plus
    /// [`MAX_OVERLOADS`]).
    pub overloads: Vec<Overload>,
    /// Vrai si `overloads` est coupé.
    pub truncated: bool,
}

/// Intervalle de surutilisation d'une ressource.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Overload {
    /// Début.
    pub from: Moment,
    /// Fin.
    pub to: Moment,
    /// Pic d'unités effectives sur l'intervalle (1/10 000).
    pub peak: Units,
}

/// Rôle d'une tâche dans la hiérarchie (R-052).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskRole {
    /// Tâche sans sous-tâche.
    Leaf,
    /// Tâche récapitulative.
    Summary,
}

/// Cause du début au plus tôt d'une tâche (R-044).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Driver {
    /// Début du projet (R-036).
    ProjectStart,
    /// Borne posée sur une récapitulative ancêtre (R-056).
    SummaryGate {
        /// Récapitulative dont la borne s'applique.
        summary: TaskUid,
    },
    /// Lien depuis un prédécesseur.
    Link {
        /// Prédécesseur (d'origine, éventuellement récapitulatif).
        predecessor: TaskUid,
        /// Type du lien.
        link: LinkType,
    },
    /// Dates saisies (tâche manuelle, R-063).
    Manual,
    /// Agrégat des sous-tâches (récapitulative, R-053).
    Rollup,
    /// Contrainte de date qui déplace le début placé (R-044, R-093, R-094).
    Constraint {
        /// Type de la contrainte (`constraint` en JSON : le champ `kind`
        /// porte déjà la variante).
        #[serde(rename = "constraint")]
        kind: ConstraintKind,
    },
    /// Début réel d'une tâche commencée (J6, R-203, R-204) : liens,
    /// porte et contrainte ne déplacent plus son début.
    Actual,
}

/// Cause de la fin au plus tard d'une tâche (R-047) : le premier plafond
/// strictement le plus petit, dans l'ordre fin du projet (ou ancre `F` à
/// rebours), plusieurs chemins critiques, plafond hérité d'une
/// récapitulative, liens sortants, contrainte, échéance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LateDriver {
    /// Fin du projet (R-046) ou ancre `F` de la planification à rebours
    /// (R-106).
    ProjectFinish,
    /// Plafond d'un lien sortant (R-047).
    Successor {
        /// Successeur (d'origine, éventuellement récapitulatif).
        successor: TaskUid,
        /// Type du lien.
        link: LinkType,
    },
    /// Plafond hérité d'une récapitulative ancêtre : `C(S)` ou `K(S)`
    /// (R-057).
    SummaryCap {
        /// Récapitulative dont le plafond s'applique.
        summary: TaskUid,
    },
    /// Plusieurs chemins critiques : fin au plus tard = fin au plus tôt
    /// (R-051).
    MultipleCriticalPaths,
    /// Contrainte de date : plafond (SNLT, FNLT, R-095, R-096), fenêtre ou
    /// épingle (R-097, R-098), serrage à rebours (SNET, FNET, R-109), épingle
    /// d'une tâche ASAP à rebours (R-108).
    Constraint {
        /// Type de la contrainte (`constraint` en JSON : le champ `kind`
        /// porte déjà la variante).
        #[serde(rename = "constraint")]
        kind: ConstraintKind,
    },
    /// Échéance : plafond brut de la fin au plus tard (R-102).
    Deadline,
    /// Dates saisies d'une tâche ou d'une récapitulative manuelle, prises
    /// comme dates au plus tard à rebours (R-063, R-064, R-106).
    Manual,
    /// Récapitulative : max des fins au plus tard de ses enfants (R-060).
    Rollup,
    /// Repli : dates au plus tard = dates au plus tôt (R-069).
    Fallback,
    /// Dates au plus tard = dates réelles d'une tâche achevée (J6, R-203),
    /// ou au plus tôt d'une tâche en cours à rebours (R-207).
    Actual,
}
