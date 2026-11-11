//! Vues JSON partagées par la CLI (`--json`), le MCP et le canal
//! (ADR-0012 D1, lot L6).
//!
//! Contrats de surface :
//! - **C-MCP-01** : toute date et toute marge viennent du `Schedule` du
//!   moteur (règle 3) ; ce module ne fait que les recopier en texte ISO
//!   local. Seule mesure faite ici, `DiffView::finish_delta` : le temps
//!   ouvré entre les deux fins du projet, compté par le calendrier compilé
//!   du cœur (R-020), jamais estimé ;
//! - **C-MCP-02** : forme JSON figée par les fichiers de référence
//!   `tests/golden/views/*.json` (vues résumées) ;
//! - **C-MCP-03** : sortie bornée : [`MAX_LIST_TASKS`] tâches par page,
//!   [`MAX_DIFF_TASKS`] tâches de diff, [`MAX_VIEW_DIAGNOSTICS`]
//!   diagnostics, messages d'au plus [`MAX_MESSAGE_CHARS`] caractères ;
//!   J5 : [`MAX_LIST_RESOURCES`] ressources par page,
//!   [`MAX_RESOURCE_ASSIGNMENTS`] affectations par ressource,
//!   [`MAX_USAGE_CELLS`] cellules d'utilisation.
//!
//! Ressources (J5, lot L8) : travail, pic, surutilisation, dates et unités
//! effectives des affectations viennent du `Schedule` (R-184 à R-188) ;
//! unités en [`UnitsJson`] (`value` en 1/10 000, `text` « 50% », ou la
//! quantité « 3 » d'une ressource matérielle), travail en heures.
//!
//! Durées : la forme unique `{"minutes","unit","text"}` de [`DurationJson`]
//! (une durée de sortie est une entrée valide) ; une marge négative a des
//! minutes négatives et un texte précédé de « - » (sortie seulement).

use std::collections::BTreeMap;

use plannercraft_core::calendar::{CalendarSet, CompiledCalendar};
use plannercraft_core::resource::usage::MAX_ASSIGNMENT_ROWS;
use plannercraft_core::resource::{AssignmentUsageReport, UsageError, usage_by_assignment, usage_with};
use plannercraft_core::{
    Assignment, Decimal, Diagnostic, Duration, DurationUnit, HasUid, LagValue, Minutes, Moment, Percent,
    ProgressState, Project, Resource, ResourceKind, ResourceUid, Schedule, Severity, Subject, Task,
    TaskBaseline, TaskRole, TaskSchedule, TaskUid, TimeBasis, Units, Work, round_div,
};
use serde::Serialize;

use crate::command::params::{ResourceListParams, TaskListParams, UsageParams};
use crate::diff::{ScheduleDiff, TaskChange};
use crate::error::{CommandError, CommandErrorKind, MAX_MESSAGE_CHARS, codes};
use crate::filter::{FilterCtx, TaskFilter};
use crate::json::{
    ConstraintKindJson, DurationJson, GranularityJson, LagJson, LinkTypeJson, PercentJson, ResourceKindJson,
    TaskTypeJson, UnitsJson,
};
use crate::outcome::{ApplyOutcome, SideEffect};
use crate::session::Session;
use crate::text::TextContext;
use crate::text::lex::format_decimal;

/// Tâches d'une page de `task.list` au plus (C-MCP-03).
pub const MAX_LIST_TASKS: usize = 500;
/// Tâches d'une page de `task.list` par défaut.
pub const DEFAULT_LIST_TASKS: usize = 50;
/// Tâches d'un diff au plus (C-MCP-03).
pub const MAX_DIFF_TASKS: usize = 1_000;
/// Diagnostics d'une vue au plus (C-MCP-03) ; le total reste compté.
pub const MAX_VIEW_DIAGNOSTICS: usize = 100;
/// Ressources d'une page de `resource.list` au plus (C-MCP-03, J5).
pub const MAX_LIST_RESOURCES: usize = 500;
/// Ressources d'une page de `resource.list` par défaut.
pub const DEFAULT_LIST_RESOURCES: usize = 50;
/// Affectations détaillées par ressource au plus (C-MCP-03, J5) ; le total
/// reste compté (`assignments_total`).
pub const MAX_RESOURCE_ASSIGNMENTS: usize = 100;
/// Ressources détaillées par tâche (`TaskView.resources`) au plus (C-MCP-03,
/// J5) ; le total est alors donné (`resources_total`).
pub const MAX_TASK_RESOURCES: usize = 100;
/// Cellules (périodes × ressources) d'un rapport d'utilisation au plus
/// (C-MCP-03, J5) : borne de surface, plus serrée que celle du cœur
/// (1 000 × 1 000, R-188).
pub const MAX_USAGE_CELLS: usize = 20_000;

/// Champs d'une tâche repris par une ligne de `baseline_variance`
/// ([`ViewCtx::task_brief`]), mêmes valeurs que [`TaskView`].
pub(crate) struct TaskBrief {
    /// ID (position).
    pub id: u32,
    /// Nom.
    pub name: String,
    /// Début planifié.
    pub start: Option<String>,
    /// Fin planifiée.
    pub finish: Option<String>,
    /// Durée.
    pub duration: DurationJson,
    /// Travail calculé, absent quand nul.
    pub work: Option<DurationJson>,
}

/// Tâche, telle que le moteur l'a planifiée.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TaskView {
    /// UID (stable).
    pub uid: u32,
    /// ID (position, change avec l'ordre).
    pub id: u32,
    /// Nom.
    pub name: String,
    /// Niveau hiérarchique (1 : premier niveau).
    pub level: u16,
    /// Récapitulative.
    pub summary: bool,
    /// Jalon.
    pub milestone: bool,
    /// Durée : saisie pour une feuille, calculée pour une récapitulative.
    pub duration: DurationJson,
    /// Début planifié (ISO local) ; absent si la tâche n'est pas planifiée.
    pub start: Option<String>,
    /// Fin planifiée (ISO local).
    pub finish: Option<String>,
    /// Marge totale (unité par défaut du projet).
    pub total_slack: Option<DurationJson>,
    /// Marge libre.
    pub free_slack: Option<DurationJson>,
    /// Critique.
    pub critical: bool,
    /// Contrainte, sauf « dès que possible ».
    pub constraint: Option<ConstraintOut>,
    /// Échéance (ISO local).
    pub deadline: Option<String>,
    /// Liens vers les prédécesseurs.
    pub predecessors: Vec<LinkOut>,
    /// Codes des diagnostics de la tâche.
    pub diagnostics: Vec<String>,
    /// Ressources affectées (J5), dans l'ordre des affectations ; absent du
    /// JSON quand vide (affectation fictive −65 535 exclue).
    /// Au plus [`MAX_TASK_RESOURCES`] (C-MCP-03).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<TaskResourceOut>,
    /// Nombre total de ressources affectées quand `resources` est coupée à
    /// [`MAX_TASK_RESOURCES`] (C-MCP-03) ; absent sinon.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources_total: Option<usize>,
    /// Travail calculé par le moteur (R-184), en heures ; absent du JSON
    /// quand nul.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work: Option<DurationJson>,
    /// Avancement calculé par le moteur (J6, R-235) : état et % achevé ;
    /// absent pour une tâche non commencée.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<TaskProgressOut>,
}

/// Avancement résumé d'une tâche, dans [`TaskView`] (J6, R-235).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TaskProgressOut {
    /// État (`started`, `complete`).
    pub state: ProgressState,
    /// % achevé (R-200, R-210).
    pub percent: PercentJson,
}

/// Avancement détaillé d'une tâche, dans [`TaskDetailView`] (J6, R-235) :
/// valeurs du moteur (`ProgressSchedule`), % physique saisi.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProgressDetailOut {
    /// État.
    pub state: ProgressState,
    /// % achevé (durée).
    pub percent_complete: PercentJson,
    /// % travail achevé.
    pub percent_work_complete: PercentJson,
    /// % physique achevé (saisi, sans effet sur le calcul, R-212).
    pub physical_percent_complete: PercentJson,
    /// Début réel (saisi ou implicite, R-200).
    pub actual_start: String,
    /// Fin réelle ; absente si la tâche n'est pas achevée.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_finish: Option<String>,
    /// Durée réelle (unité de la tâche).
    pub actual_duration: DurationJson,
    /// Durée restante = D − réel.
    pub remaining_duration: DurationJson,
    /// Travail réel (heures) d'une feuille ; absent d'une récapitulative.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_work: Option<DurationJson>,
    /// Travail restant = W − réel (heures) d'une feuille.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_work: Option<DurationJson>,
    /// Fin de la partie réelle (`Stop`, R-204).
    pub stop: String,
    /// Reprise du reste (`Resume`, R-204).
    pub resume: String,
}

/// Valeurs d'une ligne de base (J6, R-228, R-235).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BaselineOut {
    /// Début de base.
    pub start: Option<String>,
    /// Fin de base.
    pub finish: Option<String>,
    /// Durée de base ; absente si la ligne n'en a pas.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<DurationJson>,
    /// Travail de base (heures) ; absent si la ligne n'en a pas.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work: Option<DurationJson>,
}

/// Ligne de base numérotée d'une tâche, dans [`TaskDetailView`] (J6).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NumberedBaselineOut {
    /// Numéro (0 à 10).
    pub number: u8,
    /// Valeurs.
    #[serde(flatten)]
    pub values: BaselineOut,
}

/// Valeurs d'une ligne de base en sortie.
#[must_use]
pub fn baseline_out(b: &TaskBaseline, text: &TextContext) -> BaselineOut {
    BaselineOut {
        start: b.start.map(iso),
        finish: b.finish.map(iso),
        duration: b.duration.map(|d| DurationJson::from_duration(&d, text)),
        work: b.work.map(|w| work_out(w, text)),
    }
}

/// % de sortie (J6) : `value` en 1/100 de % (10 000 = 100 %) et `text`
/// (« 40% », « 12,5% »), relisible par [`PercentJson::to_percent`].
#[must_use]
pub fn percent_out(p: Percent, ctx: &TextContext) -> PercentJson {
    PercentJson {
        value: Some(p.get()),
        text: Decimal::new(p.get(), 2)
            .ok()
            .map(|d| format!("{}%", format_decimal(d, ctx.decimal))),
    }
}

/// Ressource affectée à une tâche, dans [`TaskView`] (J5).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TaskResourceOut {
    /// UID de la ressource.
    pub uid: u32,
    /// Nom.
    pub name: String,
    /// Unités de l'affectation (quantité pour une ressource matérielle).
    pub units: UnitsJson,
}

/// Détail d'une tâche (`task.get`) : la [`TaskView`], plus le type et le
/// pilotage par l'effort quand ils diffèrent des défauts du projet (J5).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TaskDetailView {
    /// Vue de la tâche (champs à plat).
    #[serde(flatten)]
    pub task: TaskView,
    /// Type et pilotage par l'effort ; absent quand ils valent
    /// `default_task_type` et `new_tasks_effort_driven` du projet, et pour
    /// une récapitulative (jamais pilotée, R-193).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduling: Option<SchedulingOut>,
    /// Avancement détaillé (J6, R-235) ; absent pour une tâche non
    /// commencée.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<ProgressDetailOut>,
    /// Lignes de base de la tâche (0 à 10, au plus 11), par numéro ;
    /// absentes du JSON quand il n'y en a pas (J6, R-235).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub baselines: Vec<NumberedBaselineOut>,
}

/// Type de tâche et pilotage par l'effort (S41, R-189, R-193).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SchedulingOut {
    /// Type de tâche.
    #[serde(rename = "type")]
    pub task_type: TaskTypeJson,
    /// Pilotée par l'effort.
    pub effort_driven: bool,
}

/// Contrainte de date d'une tâche.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ConstraintOut {
    /// Type.
    pub kind: ConstraintKindJson,
    /// Date (absente pour « dès/le plus tard possible »).
    pub date: Option<String>,
}

/// Lien vers un prédécesseur.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LinkOut {
    /// UID du prédécesseur.
    pub pred: u32,
    /// Type.
    pub kind: LinkTypeJson,
    /// Décalage, s'il n'est pas nul.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lag: Option<LagJson>,
}

/// Résumé du projet (`project.summary`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SummaryView {
    /// Nom du projet.
    pub name: String,
    /// Début planifié (ISO local).
    pub start: String,
    /// Fin planifiée (ISO local).
    pub finish: String,
    /// Durée du projet.
    pub duration: DurationJson,
    /// Tâches (lignes vides exclues).
    pub tasks: usize,
    /// Récapitulatives.
    pub summaries: usize,
    /// Jalons.
    pub milestones: usize,
    /// Tâches critiques.
    pub critical: usize,
    /// Liens.
    pub links: usize,
    /// Diagnostics de gravité erreur.
    pub errors: usize,
    /// Avertissements.
    pub warnings: usize,
    /// Document modifié depuis l'enregistrement.
    pub dirty: bool,
    /// Entrées d'historique.
    pub history_len: usize,
    /// Chemin du fichier.
    pub path: Option<String>,
    /// Date d'état effective (J6, R-219, R-235) : saisie, sinon la date du
    /// jour du projet (ISO local).
    pub status_date: String,
    /// % achevé du projet (UID 0, R-210) ; absent sans avancement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent_complete: Option<PercentJson>,
    /// Numéros des lignes de base présentes (tâches, projet, affectations,
    /// ressources), croissants ; absent quand il n'y en a pas.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub baselines: Vec<u8>,
}

/// Page de `task.list`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ListView {
    /// Tâches de la page, dans l'ordre des ID.
    pub tasks: Vec<TaskView>,
    /// UID à passer en `cursor` pour la page suivante ; absent à la fin.
    pub next_cursor: Option<u32>,
    /// Tâches retenues par le filtre, toutes pages comprises.
    pub total: usize,
}

/// Diagnostic, lisible tel quel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DiagView {
    /// Code stable.
    pub code: String,
    /// Gravité.
    pub severity: Severity,
    /// Élément concerné.
    pub subject: Option<Subject>,
    /// Message (borné).
    pub message: String,
}

/// Résultat d'un lot, d'une annulation ou d'un rétablissement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OutcomeView {
    /// Vrai si la session a changé (faux en `dry_run`).
    pub committed: bool,
    /// Étiquettes `$x` → UID.
    pub labels: BTreeMap<String, u32>,
    /// UID des tâches créées, dans l'ordre (aussi en `dry_run`).
    pub created_uids: Vec<u32>,
    /// Tâches créées, telles que planifiées (vide en `dry_run` : elles
    /// n'existent que dans la copie jetée ; leurs dates sont dans `diff`).
    pub created: Vec<TaskView>,
    /// Diff de planning.
    pub diff: DiffView,
    /// Avertissements nouveaux.
    pub warnings: Vec<DiagView>,
    /// Effets de bord.
    pub side_effects: Vec<SideEffect>,
    /// Entrées d'historique après l'opération.
    pub history_len: usize,
    /// Étiquettes `$x` des ressources créées → UID (J5) ; absent du JSON
    /// quand vide.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub resource_labels: BTreeMap<String, u32>,
    /// UID des ressources créées, dans l'ordre (J5) ; absent du JSON quand
    /// vide.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub created_resources: Vec<u32>,
}

/// Diff de planning (R-163), dates en ISO local.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DiffView {
    /// Début du projet avant.
    pub start_before: String,
    /// Début du projet après.
    pub start_after: String,
    /// Fin du projet avant.
    pub finish_before: String,
    /// Fin du projet après.
    pub finish_after: String,
    /// Décalage de la fin du projet : temps ouvré du calendrier du projet,
    /// en jours (minutes exactes dans `minutes`).
    pub finish_delta: DurationJson,
    /// Tâches devenues critiques.
    pub critical_entered: Vec<u32>,
    /// Tâches qui ne sont plus critiques.
    pub critical_left: Vec<u32>,
    /// Tâches aux dates déplacées (début ou fin), apparues ou disparues,
    /// en tout (R-163).
    pub changed_total: usize,
    /// Tâches touchées par le lot, en tout (R-163 amendée) : donnée
    /// enregistrée changée, ajoutées, supprimées, déplacées dans la liste
    /// (2026-11-11), ou dates déplacées ; toujours `>= changed_total`.
    pub touched_total: usize,
    /// Tâches changées, les plus décalées d'abord.
    pub changed: Vec<TaskChangeView>,
    /// Vrai si `changed` est coupé.
    pub truncated: bool,
    /// Ressources surutilisées apparues ou résolues (J5, ADR-0013 D9,
    /// signalées dès un `dry_run`) ; absent du JSON quand vide.
    #[serde(skip_serializing_if = "OverallocationView::is_empty")]
    pub overallocated: OverallocationView,
}

/// Surutilisations changées par un lot (J5).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct OverallocationView {
    /// Ressources (UID) devenues surutilisées.
    pub new: Vec<u32>,
    /// Ressources (UID) qui ne le sont plus.
    pub resolved: Vec<u32>,
}

impl OverallocationView {
    /// Vrai si rien n'a changé.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.new.is_empty() && self.resolved.is_empty()
    }
}

/// Ressource, telle que planifiée (J5, `resource.list`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ResourceView {
    /// UID.
    pub uid: u32,
    /// ID (position + 1, comme l'écrivain MSPDI ; change avec l'ordre).
    pub id: u32,
    /// Nom.
    pub name: String,
    /// Type.
    pub kind: ResourceKindJson,
    /// Étiquette d'unité (ressource matérielle) ; absente sinon.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material_label: Option<String>,
    /// Capacité maximale.
    pub max_units: UnitsJson,
    /// Calendrier propre.
    pub calendar: Option<u32>,
    /// Initiales.
    pub initials: String,
    /// Groupe.
    pub group: String,
    /// Travail total calculé par le moteur (R-184), en heures (quantité
    /// × 60 min pour une ressource matérielle, R-185).
    pub work: DurationJson,
    /// Pic d'unités effectives (R-186).
    pub peak: UnitsJson,
    /// Surutilisée (R-186).
    pub overallocated: bool,
    /// Inactive.
    pub inactive: bool,
    /// Affectations, dans l'ordre du modèle, au plus
    /// [`MAX_RESOURCE_ASSIGNMENTS`].
    pub assignments: Vec<AssignmentView>,
    /// Affectations en tout.
    pub assignments_total: usize,
}

/// Affectation, telle que planifiée (J5).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AssignmentView {
    /// UID.
    pub uid: u32,
    /// UID de la tâche.
    pub task: u32,
    /// UID de la ressource ; absent pour l'affectation fictive −65 535.
    pub resource: Option<u32>,
    /// Unités saisies (quantité pour une ressource matérielle).
    pub units: UnitsJson,
    /// Travail, en heures.
    pub work: DurationJson,
    /// Retard d'affectation (R-177), dans l'unité par défaut du projet ;
    /// absent quand nul.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay: Option<DurationJson>,
    /// Début placé par le moteur (ISO local) ; absent si la tâche n'est pas
    /// planifiée.
    pub start: Option<String>,
    /// Fin placée par le moteur.
    pub finish: Option<String>,
    /// Unités effectives (débit, R-186).
    pub effective_units: Option<UnitsJson>,
}

/// Page de `resource.list` (J5).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ResourceListView {
    /// Ressources de la page, dans l'ordre des UID.
    pub resources: Vec<ResourceView>,
    /// UID à passer en `cursor` pour la page suivante ; absent à la fin.
    pub next_cursor: Option<u32>,
    /// Ressources retenues par le filtre, toutes pages comprises.
    pub total: usize,
}

/// Utilisation par période (J5, `analysis.resource_usage`, R-188).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UsageView {
    /// Granularité.
    pub by: GranularityJson,
    /// Début de chaque période (ISO local).
    pub periods: Vec<String>,
    /// Une ligne par ressource.
    pub rows: Vec<UsageRowView>,
    /// Vrai si le rapport est coupé.
    pub truncated: bool,
    /// Une ligne par affectation (J7, R-245), triées par (ressource, tâche,
    /// UID) ; seulement si `assignments` est demandé, absent si vide.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub assignments: Vec<AssignmentUsageView>,
    /// Total de chaque tâche affectée à une ressource de travail (R-245),
    /// dans l'ordre des UID ; seulement si `assignments` est demandé,
    /// absent si vide.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub task_totals: Vec<TaskUsageView>,
}

/// Ligne d'une affectation dans [`UsageView`] (R-245).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AssignmentUsageView {
    /// UID de l'affectation.
    pub uid: u32,
    /// UID de la tâche.
    pub task: u32,
    /// UID de la ressource.
    pub resource: u32,
    /// Travail de chaque période.
    pub work: Vec<DurationJson>,
}

/// Total d'une tâche dans [`UsageView`] (R-245).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TaskUsageView {
    /// UID de la tâche.
    pub uid: u32,
    /// Nom.
    pub name: String,
    /// Travail de chaque période (Σ de ses affectations de travail).
    pub work: Vec<DurationJson>,
}

/// Ligne d'une ressource dans [`UsageView`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UsageRowView {
    /// UID de la ressource.
    pub uid: u32,
    /// Nom.
    pub name: String,
    /// Travail de chaque période.
    pub work: Vec<DurationJson>,
    /// Surutilisation de chaque période.
    pub overallocated: Vec<bool>,
}

/// Changement d'une tâche.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TaskChangeView {
    /// UID.
    pub uid: u32,
    /// ID avant (absent : tâche ajoutée).
    pub id_before: Option<u32>,
    /// ID après (absent : tâche supprimée).
    pub id_after: Option<u32>,
    /// Début avant.
    pub start_before: Option<String>,
    /// Début après.
    pub start_after: Option<String>,
    /// Fin avant.
    pub finish_before: Option<String>,
    /// Fin après.
    pub finish_after: Option<String>,
    /// Marge totale avant.
    pub total_slack_before: Option<DurationJson>,
    /// Marge totale après.
    pub total_slack_after: Option<DurationJson>,
    /// Critique avant.
    pub critical_before: bool,
    /// Critique après.
    pub critical_after: bool,
    /// Minutes écoulées entre la fin avant et la fin après.
    pub finish_delta_min: i64,
}

/// Chemin critique (`analysis.critical_path`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CriticalPathView {
    /// Fin du projet.
    pub project_finish: String,
    /// Tâches critiques (feuilles), dans l'ordre du calcul (R-035).
    pub tasks: Vec<TaskView>,
    /// Tâches critiques en tout.
    pub total: usize,
    /// Vrai si `tasks` est coupé à [`MAX_LIST_TASKS`].
    pub truncated: bool,
}

/// Texte ISO local d'un instant (R-075).
fn iso(m: Moment) -> String {
    m.to_string()
}

/// Durée de sortie de `minutes` (signées) dans l'unité `unit`, temps ouvré.
/// Une valeur négative (marge) s'écrit « -2j » ; hors des bornes du modèle,
/// seules les minutes et l'unité sont données.
#[must_use]
pub fn signed_minutes(minutes: Minutes, unit: DurationUnit, ctx: &TextContext) -> DurationJson {
    let (negative, abs) = if minutes.is_negative() {
        (true, minutes.checked_neg())
    } else {
        (false, Some(minutes))
    };
    let parsed = abs.and_then(|a| Duration::new(a, unit, TimeBasis::Working, false).ok());
    let Some(d) = parsed else {
        return DurationJson {
            minutes: Some(minutes.get()),
            unit: Some(unit.into()),
            ..DurationJson::default()
        };
    };
    let mut out = DurationJson::from_duration(&d, ctx);
    out.minutes = Some(minutes.get());
    out.elapsed = None;
    out.estimated = None;
    if negative {
        out.text = out.text.map(|t| format!("-{t}"));
    }
    out
}

/// Unités de sortie (J5) : `value` en 1/10 000 et `text` en pourcentage
/// (« 50% », « 12,5% »), relisible par [`UnitsJson::to_units`].
#[must_use]
pub fn units_out(u: Units, ctx: &TextContext) -> UnitsJson {
    UnitsJson {
        value: Some(u.get()),
        text: Decimal::new(u.get(), 2)
            .ok()
            .map(|d| format!("{}%", format_decimal(d, ctx.decimal))),
    }
}

/// Quantité de sortie d'une ressource matérielle (J5) : `value` en
/// 1/10 000 et `text` en nombre (« 3 », « 1,5 »), relisible par
/// [`UnitsJson::to_quantity`].
#[must_use]
pub fn quantity_out(u: Units, ctx: &TextContext) -> UnitsJson {
    UnitsJson {
        value: Some(u.get()),
        text: Decimal::new(u.get(), 4)
            .ok()
            .map(|d| format_decimal(d, ctx.decimal)),
    }
}

/// Quantité d'une affectation matérielle : son travail en heures (PT3H =
/// 3, O30, R-185), en 1/10 000 arrondi (R-001) ; l'unité de l'affectation
/// reste 100 % (R-196). Saturée au plafond des unités.
#[must_use]
pub fn material_quantity(w: Work) -> Units {
    let q = i128::from(w.minutes().get())
        .checked_mul(10_000)
        .and_then(|n| round_div(n, 60))
        .unwrap_or(i64::MAX)
        .clamp(Units::MIN, Units::MAX);
    Units::new(q).unwrap_or(Units::ZERO)
}

/// Travail de sortie, en heures (J5).
#[must_use]
pub fn work_out(w: Work, ctx: &TextContext) -> DurationJson {
    signed_minutes(w.minutes(), DurationUnit::Hour, ctx)
}

/// Type de ressource en JSON.
const fn kind_json(k: &ResourceKind) -> ResourceKindJson {
    match k {
        ResourceKind::Work => ResourceKindJson::Work,
        ResourceKind::Material { .. } => ResourceKindJson::Material,
        ResourceKind::Cost => ResourceKindJson::Cost,
    }
}

/// Coupe `s` à [`MAX_MESSAGE_CHARS`] caractères (C-MCP-03).
fn bounded(s: String) -> String {
    if s.chars().count() <= MAX_MESSAGE_CHARS {
        return s;
    }
    let mut out: String = s.chars().take(MAX_MESSAGE_CHARS.saturating_sub(1)).collect();
    out.push('…');
    out
}

impl DiagView {
    /// Vue d'un diagnostic du cœur.
    #[must_use]
    pub fn from_diagnostic(d: &Diagnostic) -> Self {
        Self {
            code: d.code().to_owned(),
            severity: d.severity(),
            subject: Some(d.subject()),
            message: bounded(d.to_string()),
        }
    }

    /// Vue d'un diagnostic d'import MSPDI.
    #[must_use]
    pub fn from_import(d: &plannercraft_mspdi::ImportDiagnostic) -> Self {
        Self {
            code: d.code.code(),
            severity: d.severity,
            subject: d.subject,
            message: bounded(d.to_string()),
        }
    }

    /// Vues d'une liste de diagnostics, au plus [`MAX_VIEW_DIAGNOSTICS`].
    pub fn list<'a>(ds: impl IntoIterator<Item = &'a Diagnostic>) -> Vec<Self> {
        ds.into_iter()
            .take(MAX_VIEW_DIAGNOSTICS)
            .map(Self::from_diagnostic)
            .collect()
    }
}

/// Contexte de construction des vues sur un état (projet et planning).
pub struct ViewCtx<'a> {
    project: &'a Project,
    schedule: &'a Schedule,
    text: TextContext,
    /// Codes des diagnostics par tâche (dédoublonnés, triés).
    task_diags: BTreeMap<TaskUid, Vec<String>>,
    /// Affectations par tâche, dans l'ordre du modèle (J5).
    by_task: BTreeMap<TaskUid, Vec<&'a Assignment>>,
}

impl<'a> ViewCtx<'a> {
    /// Contexte de l'état courant d'une session.
    #[must_use]
    pub fn new(session: &'a Session) -> Self {
        let mut task_diags: BTreeMap<TaskUid, Vec<String>> = BTreeMap::new();
        for d in session.diagnostics() {
            if let Subject::Task(uid) = d.subject() {
                let codes = task_diags.entry(uid).or_default();
                let code = d.code();
                if !codes.iter().any(|c| c == code) {
                    codes.push(code.to_owned());
                }
            }
        }
        for codes in task_diags.values_mut() {
            codes.sort_unstable();
        }
        let project = session.project();
        let mut by_task: BTreeMap<TaskUid, Vec<&'a Assignment>> = BTreeMap::new();
        for a in &project.assignments {
            by_task.entry(a.task).or_default().push(a);
        }
        Self {
            project,
            schedule: session.schedule(),
            text: session.text_context(),
            task_diags,
            by_task,
        }
    }

    /// Vrai si une affectation de la tâche `task` désigne `resource`.
    fn task_has_resource(&self, task: TaskUid, resource: ResourceUid) -> bool {
        self.by_task
            .get(&task)
            .is_some_and(|v| v.iter().any(|a| a.resource == Some(resource)))
    }

    /// Vrai si l'affectation désigne une ressource matérielle.
    fn is_material(&self, a: &Assignment) -> bool {
        a.resource
            .and_then(|r| self.project.resources.get(r))
            .is_some_and(|r| matches!(r.kind, ResourceKind::Material { .. }))
    }

    /// Unités d'une affectation : quantité pour une ressource matérielle
    /// (portée par le travail, O30), pourcentage sinon.
    fn assignment_units(&self, a: &Assignment) -> UnitsJson {
        if self.is_material(a) {
            quantity_out(material_quantity(a.work), &self.text)
        } else {
            units_out(a.units, &self.text)
        }
    }

    /// Ressources affectées à une tâche (affectation fictive exclue), au
    /// plus [`MAX_TASK_RESOURCES`], et leur nombre total s'il est dépassé.
    fn task_resources(&self, task: TaskUid) -> (Vec<TaskResourceOut>, Option<usize>) {
        let Some(list) = self.by_task.get(&task) else {
            return (Vec::new(), None);
        };
        let mut out = Vec::new();
        let mut total = 0usize;
        for a in list {
            let Some(r) = a.resource else {
                continue;
            };
            total = total.saturating_add(1);
            if out.len() < MAX_TASK_RESOURCES {
                out.push(TaskResourceOut {
                    uid: r.get(),
                    name: self
                        .project
                        .resources
                        .get(r)
                        .map(|r| r.name.clone())
                        .unwrap_or_default(),
                    units: self.assignment_units(a),
                });
            }
        }
        let truncated = total > out.len();
        (out, truncated.then_some(total))
    }

    /// Vue d'une affectation : dates et unités effectives du moteur.
    #[must_use]
    pub fn assignment(&self, a: &Assignment) -> AssignmentView {
        let placed = self.schedule.assignments.get(&a.uid());
        let delay = (a.delay != Minutes::ZERO)
            .then(|| signed_minutes(a.delay, self.project.options.default_duration_unit, &self.text));
        let material = self.is_material(a);
        AssignmentView {
            uid: a.uid().get(),
            task: a.task.get(),
            resource: a.resource.map(ResourceUid::get),
            units: self.assignment_units(a),
            work: work_out(placed.map_or(a.work, |p| p.work), &self.text),
            delay,
            start: placed.map(|p| iso(p.span.start)),
            finish: placed.map(|p| iso(p.span.finish)),
            effective_units: placed.map(|p| {
                if material {
                    quantity_out(material_quantity(p.work), &self.text)
                } else {
                    units_out(p.effective_units, &self.text)
                }
            }),
        }
    }

    /// Vue d'une ressource à la position `pos` : travail, pic et
    /// surutilisation du moteur (R-184 à R-186), affectations bornées.
    #[must_use]
    pub fn resource_at(&self, pos: usize, r: &Resource) -> ResourceView {
        let uid = r.uid();
        let load = self.schedule.resources.get(&uid);
        // ID de MS Project : la ressource UID 0 cachée n'est pas comptée.
        let id =
            u32::try_from(plannercraft_core::resource_id(&self.project.resources, pos)).unwrap_or(u32::MAX);
        let mut assignments = Vec::new();
        let mut total = 0usize;
        for a in self
            .project
            .assignments
            .iter()
            .filter(|a| a.resource == Some(uid))
        {
            total = total.saturating_add(1);
            if assignments.len() < MAX_RESOURCE_ASSIGNMENTS {
                assignments.push(self.assignment(a));
            }
        }
        ResourceView {
            uid: uid.get(),
            id,
            name: r.name.clone(),
            kind: kind_json(&r.kind),
            material_label: match &r.kind {
                ResourceKind::Material { label } => Some(label.clone()),
                _ => None,
            },
            max_units: units_out(r.max_units, &self.text),
            calendar: r.calendar.map(plannercraft_core::CalendarUid::get),
            initials: r.initials.clone(),
            group: r.group.clone(),
            work: work_out(load.map_or(Work::ZERO, |l| l.work), &self.text),
            peak: units_out(load.map_or(Units::ZERO, |l| l.peak), &self.text),
            overallocated: load.is_some_and(|l| l.overallocated),
            inactive: r.is_inactive,
            assignments,
            assignments_total: total,
        }
    }

    /// Détail de la tâche `uid` (`task.get`), lignes vides exclues.
    #[must_use]
    pub fn task_detail(&self, uid: TaskUid) -> Option<TaskDetailView> {
        let pos = self.project.tasks.position(uid)?;
        let task = self.project.tasks.at(pos).filter(|t| !t.is_null)?;
        let view = self.task_at(pos, task);
        let options = &self.project.options;
        let defaults = task.task_type == options.default_task_type
            && task.effort_driven == options.new_tasks_effort_driven;
        let scheduling = (!view.summary && !defaults).then(|| SchedulingOut {
            task_type: task.task_type.into(),
            effort_driven: task.effort_driven,
        });
        let progress = self.progress_detail(task);
        let baselines = task
            .baselines
            .iter()
            .map(|(n, b)| NumberedBaselineOut {
                number: n.get(),
                values: baseline_out(b, &self.text),
            })
            .collect();
        Some(TaskDetailView {
            task: view,
            scheduling,
            progress,
            baselines,
        })
    }

    /// Avancement détaillé d'une tâche commencée (R-235) : valeurs du
    /// moteur ; % physique et travail réel saisis (feuille seulement).
    fn progress_detail(&self, task: &Task) -> Option<ProgressDetailOut> {
        let ts = self.schedule.scheduled(task.uid())?;
        let g = ts.progress.filter(|g| g.state != ProgressState::NotStarted)?;
        let unit = task.duration.unit();
        let leaf = ts.role != TaskRole::Summary;
        let actual_work = task.progress.actual_work;
        let remaining_work = Minutes::new(
            ts.work
                .minutes()
                .get()
                .saturating_sub(actual_work.minutes().get())
                .max(0),
        );
        Some(ProgressDetailOut {
            state: g.state,
            percent_complete: percent_out(g.percent_complete, &self.text),
            percent_work_complete: percent_out(g.percent_work_complete, &self.text),
            physical_percent_complete: percent_out(task.progress.physical_percent_complete, &self.text),
            actual_start: iso(g.actual_start),
            actual_finish: g.actual_finish.map(iso),
            actual_duration: signed_minutes(g.actual_duration, unit, &self.text),
            remaining_duration: signed_minutes(g.remaining_duration, unit, &self.text),
            actual_work: leaf.then(|| work_out(actual_work, &self.text)),
            remaining_work: leaf.then(|| signed_minutes(remaining_work, DurationUnit::Hour, &self.text)),
            stop: iso(g.stop),
            resume: iso(g.resume),
        })
    }

    /// Contexte du texte (langue de la session).
    #[must_use]
    pub const fn text(&self) -> &TextContext {
        &self.text
    }

    /// Vrai si la tâche à la position `pos` a une sous-tâche : la tâche non
    /// vide suivante est plus profonde, lignes vides ignorées (R-052).
    #[must_use]
    pub fn is_summary_at(&self, pos: usize) -> bool {
        crate::command::outline::is_summary_at(&self.project.tasks, pos)
    }

    /// Vue de la tâche `uid`, si elle existe (lignes vides comprises).
    #[must_use]
    pub fn task(&self, uid: TaskUid) -> Option<TaskView> {
        let pos = self.project.tasks.position(uid)?;
        let task = self.project.tasks.at(pos)?;
        Some(self.task_at(pos, task))
    }

    /// Vue de `task`, à la position `pos`.
    #[must_use]
    pub fn task_at(&self, pos: usize, task: &Task) -> TaskView {
        let uid = task.uid();
        let scheduled = self.schedule.scheduled(uid);
        let id = pos
            .checked_add(1)
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(u32::MAX);
        let slack_unit = self.project.options.default_duration_unit;
        let (resources, resources_total) = self.task_resources(uid);
        TaskView {
            uid: uid.get(),
            id,
            name: task.name.clone(),
            level: task.outline_level.get(),
            summary: self.is_summary_at(pos),
            milestone: task.milestone,
            duration: self.duration(task, scheduled),
            start: scheduled.map(|s| iso(s.start)),
            finish: scheduled.map(|s| iso(s.finish)),
            total_slack: scheduled.map(|s| signed_minutes(s.total_slack, slack_unit, &self.text)),
            free_slack: scheduled.map(|s| signed_minutes(s.free_slack, slack_unit, &self.text)),
            critical: scheduled.is_some_and(|s| s.critical),
            constraint: constraint_out(task),
            deadline: task.deadline.map(iso),
            predecessors: task
                .predecessors
                .iter()
                .map(|l| LinkOut {
                    pred: l.predecessor.get(),
                    kind: l.kind.into(),
                    lag: lag_out(&l.lag, &self.text),
                })
                .collect(),
            diagnostics: self.task_diags.get(&uid).cloned().unwrap_or_default(),
            resources,
            resources_total,
            work: scheduled
                .map(|s| s.work)
                .filter(|w| *w != Work::ZERO)
                .map(|w| work_out(w, &self.text)),
            progress: scheduled
                .and_then(|s| s.progress)
                .filter(|g| g.state != ProgressState::NotStarted)
                .map(|g| TaskProgressOut {
                    state: g.state,
                    percent: percent_out(g.percent_complete, &self.text),
                }),
        }
    }

    /// Champs de [`Self::task_at`] lus par `baseline_variance` (ID, nom,
    /// dates, durée, travail), sans les liens, ressources ni diagnostics
    /// (relecture finale J6, J6-PERF-3).
    pub(crate) fn task_brief(&self, pos: usize, task: &Task) -> TaskBrief {
        let scheduled = self.schedule.scheduled(task.uid());
        TaskBrief {
            id: pos
                .checked_add(1)
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(u32::MAX),
            name: task.name.clone(),
            start: scheduled.map(|s| iso(s.start)),
            finish: scheduled.map(|s| iso(s.finish)),
            duration: self.duration(task, scheduled),
            work: scheduled
                .map(|s| s.work)
                .filter(|w| *w != Work::ZERO)
                .map(|w| work_out(w, &self.text)),
        }
    }

    /// Durée d'une tâche : saisie (feuille, tâche non planifiée), ou temps
    /// ouvré calculé par le moteur (récapitulative, R-054), dans l'unité de
    /// la tâche.
    fn duration(&self, task: &Task, scheduled: Option<&TaskSchedule>) -> DurationJson {
        match scheduled {
            Some(s) if s.role == TaskRole::Summary => {
                let mut d = signed_minutes(s.duration, task.duration.unit(), &self.text);
                d.elapsed = Some(false);
                d.estimated = Some(false);
                d
            }
            _ => DurationJson::from_duration(&task.duration, &self.text),
        }
    }

    /// Tâches visibles (lignes vides exclues), avec leur position.
    pub fn rows(&self) -> impl Iterator<Item = (usize, &'a Task)> + 'a {
        self.project.tasks.iter().enumerate().filter(|(_, t)| !t.is_null)
    }

    /// Vue de l'ID et du UID d'un changement.
    fn change(&self, c: &TaskChange) -> TaskChangeView {
        let unit = self.project.options.default_duration_unit;
        let slack = |m: Option<Minutes>| m.map(|m| signed_minutes(m, unit, &self.text));
        TaskChangeView {
            uid: c.uid.get(),
            id_before: c.id_before.map(plannercraft_core::TaskId::get),
            id_after: c.id_after.map(plannercraft_core::TaskId::get),
            start_before: c.start.0.map(iso),
            start_after: c.start.1.map(iso),
            finish_before: c.finish.0.map(iso),
            finish_after: c.finish.1.map(iso),
            total_slack_before: slack(c.total_slack.0),
            total_slack_after: slack(c.total_slack.1),
            critical_before: c.critical.0,
            critical_after: c.critical.1,
            finish_delta_min: c.finish_delta_min,
        }
    }

    /// Vue d'un diff ; `finish_delta` est mesuré sur le calendrier du
    /// projet de cet état (R-020), à défaut en minutes écoulées.
    #[must_use]
    pub fn diff(&self, d: &ScheduleDiff) -> DiffView {
        let (before, after) = d.project_finish;
        let working = CompiledCalendar::compile(&self.project.calendars, self.project.calendar)
            .ok()
            .and_then(|c| c.working_between(before, after).ok());
        let finish_delta = working.map_or_else(
            || {
                let mut out = signed_minutes(before.minutes_until(after), DurationUnit::Day, &self.text);
                out.elapsed = Some(true);
                out.text = None;
                out
            },
            |m| signed_minutes(m, DurationUnit::Day, &self.text),
        );
        let changed: Vec<TaskChangeView> = d
            .changed
            .iter()
            .take(MAX_DIFF_TASKS)
            .map(|c| self.change(c))
            .collect();
        DiffView {
            start_before: iso(d.project_start.0),
            start_after: iso(d.project_start.1),
            finish_before: iso(before),
            finish_after: iso(after),
            finish_delta,
            critical_entered: d.critical_entered.iter().map(|u| u.get()).collect(),
            critical_left: d.critical_left.iter().map(|u| u.get()).collect(),
            changed_total: d.changed_total,
            touched_total: d.touched_total,
            truncated: d.truncated || changed.len() < d.changed.len(),
            changed,
            overallocated: OverallocationView {
                new: d.overallocated_new.iter().map(|u| u.get()).collect(),
                resolved: d.overallocated_resolved.iter().map(|u| u.get()).collect(),
            },
        }
    }

    /// Vue d'un résultat appliqué à cet état (après un commit, un undo ou
    /// un redo, ou l'état intact d'un `dry_run`).
    #[must_use]
    pub fn outcome(&self, o: &ApplyOutcome) -> OutcomeView {
        let created = if o.committed {
            o.created.iter().filter_map(|u| self.task(*u)).collect()
        } else {
            Vec::new()
        };
        OutcomeView {
            committed: o.committed,
            labels: o.labels.iter().map(|(k, v)| (k.clone(), v.get())).collect(),
            created_uids: o.created.iter().map(|u| u.get()).collect(),
            created,
            diff: self.diff(&o.diff),
            warnings: DiagView::list(&o.warnings),
            side_effects: o.side_effects.clone(),
            history_len: o.history_len,
            resource_labels: o
                .resource_labels
                .iter()
                .map(|(k, v)| (k.clone(), v.get()))
                .collect(),
            created_resources: o.created_resources.iter().map(|u| u.get()).collect(),
        }
    }

    /// Chemin critique : feuilles critiques dans l'ordre topologique du
    /// calcul (R-035), au plus [`MAX_LIST_TASKS`].
    #[must_use]
    pub fn critical_path(&self) -> CriticalPathView {
        let mut tasks = Vec::new();
        let mut total = 0usize;
        for uid in &self.schedule.order {
            let Some(s) = self.schedule.scheduled(*uid) else {
                continue;
            };
            if !s.critical || s.role == TaskRole::Summary {
                continue;
            }
            total = total.saturating_add(1);
            if tasks.len() < MAX_LIST_TASKS
                && let Some(v) = self.task(*uid)
            {
                tasks.push(v);
            }
        }
        CriticalPathView {
            project_finish: iso(self.schedule.project_finish),
            truncated: tasks.len() < total,
            tasks,
            total,
        }
    }
}

/// Contrainte d'une tâche, sauf « dès que possible ».
fn constraint_out(task: &Task) -> Option<ConstraintOut> {
    let kind = task.constraint.kind();
    if kind == plannercraft_core::ConstraintKind::AsSoonAsPossible {
        return None;
    }
    Some(ConstraintOut {
        kind: kind.into(),
        date: task.constraint.date().map(iso),
    })
}

/// Décalage d'un lien, sauf s'il est nul.
fn lag_out(l: &plannercraft_core::Lag, ctx: &TextContext) -> Option<LagJson> {
    let zero = match l.value {
        LagValue::Time { minutes, .. } => minutes.get() == 0,
        LagValue::Percent { percent } => percent.get() == 0,
    };
    (!zero).then(|| LagJson::from_lag(l, ctx))
}

/// Résumé de l'état courant d'une session.
#[must_use]
pub fn summary(session: &Session) -> SummaryView {
    let project = session.project();
    let schedule = session.schedule();
    let ctx = ViewCtx::new(session);
    let (mut tasks, mut summaries, mut milestones, mut critical, mut links) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    for (pos, t) in ctx.rows() {
        tasks = tasks.saturating_add(1);
        if ctx.is_summary_at(pos) {
            summaries = summaries.saturating_add(1);
        }
        if t.milestone {
            milestones = milestones.saturating_add(1);
        }
        if schedule.scheduled(t.uid()).is_some_and(|s| s.critical) {
            critical = critical.saturating_add(1);
        }
        links = links.saturating_add(t.predecessors.len());
    }
    let count = |sev: Severity| {
        session
            .diagnostics()
            .iter()
            .filter(|d| d.severity() == sev)
            .count()
    };
    let duration_min = schedule.project_summary.as_ref().map_or_else(
        || {
            CompiledCalendar::compile(&project.calendars, project.calendar)
                .ok()
                .and_then(|c| {
                    c.working_between(schedule.project_start, schedule.project_finish)
                        .ok()
                })
                .unwrap_or(Minutes::ZERO)
        },
        |s| s.duration,
    );
    let name = if project.info.name.is_empty() {
        session.origin().name.clone()
    } else {
        project.info.name.clone()
    };
    SummaryView {
        name,
        start: iso(schedule.project_start),
        finish: iso(schedule.project_finish),
        duration: signed_minutes(duration_min, project.options.default_duration_unit, ctx.text()),
        tasks,
        summaries,
        milestones,
        critical,
        links,
        errors: count(Severity::Error),
        warnings: count(Severity::Warning),
        dirty: session.is_dirty(),
        history_len: session.history().entries.len(),
        path: session.origin().path.clone(),
        status_date: iso(project.effective_status_date()),
        percent_complete: schedule
            .project_summary
            .as_ref()
            .and_then(|s| s.progress)
            .filter(|g| g.state != ProgressState::NotStarted)
            .map(|g| percent_out(g.percent_complete, ctx.text())),
        baselines: baseline_numbers(project),
    }
}

/// Numéros des lignes de base présentes dans le projet (tâches, UID 0,
/// affectations, ressources), croissants, sans doublon (R-235).
fn baseline_numbers(p: &Project) -> Vec<u8> {
    let mut seen = std::collections::BTreeSet::new();
    for t in p.tasks.iter().chain(p.summary_task.iter()) {
        seen.extend(t.baselines.keys().map(|n| n.get()));
    }
    for a in &p.assignments {
        seen.extend(a.baselines.keys().map(|n| n.get()));
    }
    for r in &p.resources {
        seen.extend(r.baselines.keys().map(|n| n.get()));
    }
    seen.into_iter().collect()
}

/// Erreur `param.invalid` sur le champ `field`.
fn invalid(field: &str, why: &str) -> CommandError {
    CommandError::new(
        codes::PARAM_INVALID,
        CommandErrorKind::InvalidParam(why.to_owned()),
    )
    .with_field(field)
}

/// Plafond de `name_contains` de `task.list`, en caractères (requête de
/// recherche, `docs/plans/j8.md` règle 10).
pub const MAX_NAME_CONTAINS_CHARS: usize = 256;

/// Page de `task.list` : tâches dans l'ordre des ID (lignes vides
/// exclues), filtrées par [`TaskFilter::hits`] (R-251, sans récapitulatives
/// liées) puis coupées après le curseur.
///
/// Filtres : `critical` (critique selon le moteur), `late` (R-236 :
/// échéance ou fin de la ligne 0 dépassée, ou retard à la date d'état),
/// `from`/`to` (dates planifiées qui touchent
/// la plage de jours, bornes comprises : début au plus tard le jour `to`,
/// fin au plus tôt le jour `from`), `outline_max`, `summary` ; J8 (R-251) :
/// `incomplete` (feuille planifiée à moins de 100 %), `milestone`, `active`,
/// `name_contains` (sans casse ni accents). Une tâche non planifiée n'est
/// retenue par aucun filtre de dates ni par `critical`, `late` ou
/// `incomplete`. `resource` (UID, J5) reste comparé par UID, hors de
/// [`TaskFilter`] (qui nomme la ressource) : deux ressources homonymes ne
/// se confondent pas.
///
/// # Errors
///
/// `limit.exceeded` au-delà de [`MAX_LIST_TASKS`] ou de
/// [`MAX_NAME_CONTAINS_CHARS`] ; `param.invalid` pour `limit` 0,
/// `outline_max` 0, `from` après `to` ; `task.not_found` (champ `cursor`)
/// si le curseur ne désigne aucune tâche ; `resource.not_found` (champ
/// `resource`) si `resource` ne désigne aucune ressource.
pub fn list(session: &Session, p: &TaskListParams) -> Result<ListView, CommandError> {
    let resource = match p.resource {
        None => None,
        Some(r) => Some(known_resource(session.project(), r, "resource")?),
    };
    let limit = match p.limit {
        None => DEFAULT_LIST_TASKS,
        Some(0) => return Err(invalid("limit", "« limit » doit valoir au moins 1")),
        Some(n) => {
            let n = usize::try_from(n).unwrap_or(usize::MAX);
            if n > MAX_LIST_TASKS {
                return Err(self::limit("limit", MAX_LIST_TASKS, "limit"));
            }
            n
        }
    };
    if p.outline_max == Some(0) {
        return Err(invalid("outline_max", "« outline_max » doit valoir au moins 1"));
    }
    if let (Some(from), Some(to)) = (p.from, p.to)
        && from > to
    {
        return Err(invalid("from", "« from » est après « to »"));
    }
    if p.name_contains
        .as_ref()
        .is_some_and(|n| n.chars().count() > MAX_NAME_CONTAINS_CHARS)
    {
        return Err(self::limit(
            "name_contains",
            MAX_NAME_CONTAINS_CHARS,
            "name_contains",
        ));
    }
    let ctx = ViewCtx::new(session);
    let project = session.project();
    let start_pos = match p.cursor {
        None => 0,
        Some(c) => {
            let uid = TaskUid::new(c);
            let pos = project.tasks.position(uid).ok_or_else(|| {
                CommandError::new(codes::TASK_NOT_FOUND, CommandErrorKind::UnknownTask(uid))
                    .with_field("cursor")
            })?;
            pos.saturating_add(1)
        }
    };
    let filter = TaskFilter {
        critical: p.critical.then_some(true),
        late: p.late.then_some(true),
        incomplete: p.incomplete,
        milestone: p.milestone,
        summary: p.summary,
        active: p.active,
        resource: None,
        from: p.from,
        to: p.to,
        outline_max: p.outline_max,
        name_contains: p.name_contains.clone(),
        show_related_summaries: false,
    };
    let hits = filter.hits(&FilterCtx {
        project,
        schedule: session.schedule(),
        status: project.effective_status_date(),
    });
    let keep = |pos: usize, t: &Task| -> bool {
        hits.shown.get(pos).copied().unwrap_or(false)
            && resource.is_none_or(|r| ctx.task_has_resource(t.uid(), r))
    };
    let mut total = 0usize;
    let mut tasks = Vec::new();
    let mut next_cursor = None;
    for (pos, t) in ctx.rows() {
        if !keep(pos, t) {
            continue;
        }
        total = total.saturating_add(1);
        if pos < start_pos {
            continue;
        }
        if tasks.len() < limit {
            tasks.push(ctx.task_at(pos, t));
        } else if next_cursor.is_none() {
            next_cursor = tasks.last().map(|v: &TaskView| v.uid);
        }
    }
    Ok(ListView {
        tasks,
        next_cursor,
        total,
    })
}

/// Ressource `uid` du projet, sinon `resource.not_found` sur `field`.
fn known_resource(project: &Project, uid: u32, field: &str) -> Result<ResourceUid, CommandError> {
    let uid = ResourceUid::new(uid);
    if project.resources.contains(uid) {
        Ok(uid)
    } else {
        Err(
            CommandError::new(codes::RESOURCE_NOT_FOUND, CommandErrorKind::UnknownResource(uid))
                .with_field(field),
        )
    }
}

/// Erreur `limit.exceeded` sur le champ `field`.
fn limit(what: &'static str, max: usize, field: &str) -> CommandError {
    CommandError::new(
        codes::LIMIT_EXCEEDED,
        CommandErrorKind::Limit {
            what,
            max: u64::try_from(max).unwrap_or(u64::MAX),
        },
    )
    .with_field(field)
}

/// Vrai pour la ressource UID 0 des fichiers MS Project (O34), cachée
/// comme dans MS Project : jamais listée ni comptée par défaut.
fn is_hidden_resource(r: &Resource) -> bool {
    r.uid().get() == 0
}

/// Page de `resource.list` (J5) : ressources dans l'ordre des UID
/// (ressource UID 0 exclue, O34), filtrées puis coupées après le curseur.
///
/// # Errors
///
/// `limit.exceeded` au-delà de [`MAX_LIST_RESOURCES`] ; `param.invalid`
/// pour `limit` 0 ; `resource.not_found` (champ `cursor`) si le curseur ne
/// désigne aucune ressource.
pub fn list_resources(session: &Session, p: &ResourceListParams) -> Result<ResourceListView, CommandError> {
    let limit_n = match p.limit {
        None => DEFAULT_LIST_RESOURCES,
        Some(0) => return Err(invalid("limit", "« limit » doit valoir au moins 1")),
        Some(n) => {
            let n = usize::try_from(n).unwrap_or(usize::MAX);
            if n > MAX_LIST_RESOURCES {
                return Err(limit("limit", MAX_LIST_RESOURCES, "limit"));
            }
            n
        }
    };
    let project = session.project();
    let after = match p.cursor {
        None => None,
        Some(c) => Some(known_resource(project, c, "cursor")?),
    };
    let ctx = ViewCtx::new(session);
    let schedule = session.schedule();
    let mut rows: Vec<(usize, &Resource)> = project
        .resources
        .iter()
        .enumerate()
        .filter(|(_, r)| !is_hidden_resource(r))
        .filter(|(_, r)| {
            !p.overallocated || schedule.resources.get(&r.uid()).is_some_and(|l| l.overallocated)
        })
        .collect();
    rows.sort_by_key(|(_, r)| r.uid());
    let total = rows.len();
    let mut resources = Vec::new();
    let mut next_cursor = None;
    for (pos, r) in rows {
        if after.is_some_and(|a| r.uid() <= a) {
            continue;
        }
        if resources.len() < limit_n {
            resources.push(ctx.resource_at(pos, r));
        } else {
            next_cursor = resources.last().map(|v: &ResourceView| v.uid);
            break;
        }
    }
    Ok(ResourceListView {
        resources,
        next_cursor,
        total,
    })
}

/// Utilisation des ressources par période (`analysis.resource_usage`,
/// R-188, trimestre R-246) : calculée par le cœur à la demande sur `[from,
/// to)` (défaut : dates du projet), lignes dans l'ordre demandé (doublons
/// retirés) ou, sans liste, toutes les ressources dans l'ordre des UID
/// (UID 0 exclue). `assignments` : lignes par affectation et totaux par
/// tâche en plus (R-245), sur les mêmes calendriers compilés.
///
/// # Errors
///
/// `param.invalid` si `from` est après `to` ; `resource.not_found` (champ
/// `resources`) ; `limit.exceeded` au-delà des bornes du cœur ou de
/// [`MAX_USAGE_CELLS`] cellules (lignes d'affectation et totaux compris).
pub fn resource_usage(session: &Session, p: &UsageParams) -> Result<UsageView, CommandError> {
    let project = session.project();
    let schedule = session.schedule();
    let from = p.from.unwrap_or(schedule.project_start);
    let to = p.to.unwrap_or(schedule.project_finish);
    if from > to {
        return Err(invalid("from", "« from » est après « to »"));
    }
    let resources: Vec<ResourceUid> = if p.resources.is_empty() {
        let mut all: Vec<ResourceUid> = project
            .resources
            .iter()
            .filter(|r| !is_hidden_resource(r))
            .map(HasUid::uid)
            .collect();
        all.sort_unstable();
        all
    } else {
        let mut seen = std::collections::BTreeSet::new();
        let mut out = Vec::with_capacity(p.resources.len());
        for r in &p.resources {
            let uid = known_resource(project, *r, "resources")?;
            if seen.insert(uid) {
                out.push(uid);
            }
        }
        out
    };
    let refused = |e: UsageError| match e {
        UsageError::Limit {
            periods,
            resources: max_resources,
        } => {
            if resources.len() > max_resources {
                limit("resources", max_resources, "resources")
            } else {
                limit("periods", periods, "by")
            }
        }
    };
    let (set, _) = CalendarSet::compile(project);
    let report = usage_with(project, schedule, &set, from, to, p.by.into(), &resources).map_err(refused)?;
    let mut lines = report.rows.len();
    let by_assignment = if p.assignments {
        // Périodes et ressources déjà admises par `usage_with` : seul le
        // nombre de lignes peut encore refuser.
        let r = usage_by_assignment(project, schedule, &set, from, to, p.by.into(), &resources)
            .map_err(|_| limit("assignments", MAX_ASSIGNMENT_ROWS, "assignments"))?;
        lines = lines
            .saturating_add(r.rows.len())
            .saturating_add(r.task_totals.len());
        Some(r)
    } else {
        None
    };
    let cells = report.periods.len().saturating_mul(lines);
    if cells > MAX_USAGE_CELLS {
        return Err(limit("periods × resources", MAX_USAGE_CELLS, "by"));
    }
    let text = session.text_context();
    let (assignments, task_totals) = by_assignment
        .map(|r| assignment_views(project, &r, &text))
        .unwrap_or_default();
    let rows = report
        .rows
        .iter()
        .map(|row| UsageRowView {
            uid: row.resource.get(),
            name: project
                .resources
                .get(row.resource)
                .map(|r| r.name.clone())
                .unwrap_or_default(),
            work: row.work.iter().map(|w| work_out(*w, &text)).collect(),
            overallocated: row.overallocated.clone(),
        })
        .collect();
    Ok(UsageView {
        by: p.by,
        periods: report.periods.iter().map(|m| iso(*m)).collect(),
        rows,
        truncated: report.truncated,
        assignments,
        task_totals,
    })
}

/// Lignes par affectation et totaux par tâche de [`UsageView`] (R-245).
fn assignment_views(
    project: &Project,
    r: &AssignmentUsageReport,
    text: &TextContext,
) -> (Vec<AssignmentUsageView>, Vec<TaskUsageView>) {
    let works = |w: &[Work]| -> Vec<DurationJson> { w.iter().map(|w| work_out(*w, text)).collect() };
    let rows = r
        .rows
        .iter()
        .map(|a| AssignmentUsageView {
            uid: a.assignment.get(),
            task: a.task.get(),
            resource: a.resource.get(),
            work: works(&a.work),
        })
        .collect();
    let totals = r
        .task_totals
        .iter()
        .map(|(t, w)| TaskUsageView {
            uid: t.get(),
            name: project.tasks.get(*t).map(|t| t.name.clone()).unwrap_or_default(),
            work: works(w),
        })
        .collect();
    (rows, totals)
}
