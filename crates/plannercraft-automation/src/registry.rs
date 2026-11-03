//! Registre des commandes (ADR-0012 D7) : table triée par id, source unique
//! des outils MCP, des commandes de la CLI, du canal et des raccourcis de
//! l'interface.
//!
//! Entrées disponibles : J4, les huit entrées J5 (lot L8 de J5), les six
//! entrées J6 (contrat du lot L0, activées par les lots L7 et L8 de J6) et
//! les cinq écritures J7 (`calendar.add`, `calendar.update`,
//! `calendar.set_week`, `calendar.delete`, `baseline.clear`, contrat J7 L0,
//! activées par le lot L5), puis les entrées J8 : `project.export_pdf`,
//! réservée au jalon 8 par le contrat L0, activée par le lot L7, et les six
//! entrées `style.*` du lot L6 (`style.apply`, `style.get`, `style.list`,
//! posées par L0, plus `style.set_bar_role`, `style.reset_to_template`,
//! `style.save_as_template`), et `custom_field.rename` (seconde passe UX,
//! lot L8, R-264). Une entrée réservée à un jalon futur
//! (`Availability::Milestone`) n'est pas listée et est refusée
//! `command.not_yet_available`. Les trois entrées `app.*` (genre [`Kind::Ui`]) ne sont servies que
//! par une application rattachée (`Availability::Attached`, ADR-0015 D11) :
//! non listées par une surface « headless », refusées
//! `control.not_attached`. Le schéma de chaque
//! entrée est celui de ses paramètres (JSON Schema 2020-12, sous-schémas en
//! ligne, sans `$ref`) ; les surfaces y ajoutent `dry_run` ou `detail`.
//! Schémas de référence : `tests/golden/schemas/<id>.json`.

use schemars::generate::SchemaSettings;
use schemars::{JsonSchema, Schema};

use crate::command::params::{
    ApplyBatchParams, ExplainParams, ExportMspdiParams, ExportPdfParams, ImportCsvParams, NewProjectParams,
    NoParams, OpenProjectParams, RenderGanttParams, ResourceListParams, SaveProjectParams, ScreenshotParams,
    TaskGetParams, TaskListParams, UiActionParams, UsageParams, VarianceParams, WhatIfParams,
};
use crate::command::style::{StyleApply, StyleResetToTemplate, StyleSaveAsTemplate, StyleSetBarRole};
use crate::command::{
    AnchorSpec, AssignmentAdd, AssignmentKey, AssignmentUpdate, BaselineClear, BaselineSet, CalendarAdd,
    CalendarDelete, CalendarException, CalendarExceptionKey, CalendarSetWeek, CalendarUpdate,
    CustomFieldRename, LinkKey, LinkSpec, ProgressReschedule, ProgressUpdate, ProjectInfoPatch, ResourceAdd,
    ResourceDelete, ResourceUpdate, SetConstraint, SetDeadline, SetStatusDate, TaskAdd, TaskDelete, TaskMove,
    TaskOne, TaskUpdate,
};
use crate::text::Language;

/// Genre d'une commande.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Lecture.
    Read,
    /// Écriture (lot, annulable).
    Write,
    /// Analyse (lecture calculée).
    Analysis,
    /// Historique (annuler, rétablir, lister).
    History,
    /// Fichiers (ouvrir, enregistrer, importer).
    File,
    /// Interface de l'application rattachée (J7 : `app.*`), sans
    /// historique ni effet sur le projet.
    Ui,
}

/// Disponibilité d'une commande.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    /// Disponible et listée.
    Now,
    /// Réservée au jalon donné, non listée.
    Milestone(u8),
    /// Servie seulement par une application rattachée (canal de contrôle,
    /// J7) : non listée par une surface « headless », refusée
    /// `control.not_attached`.
    Attached,
}

/// Entrée du registre.
#[derive(Clone, Copy, Debug)]
pub struct CommandSpec {
    /// Id à point (`task.add`).
    pub id: &'static str,
    /// Nom de l'outil MCP (`add_task`, motif `^[a-zA-Z0-9_-]{1,64}$`).
    pub mcp_name: &'static str,
    /// Genre.
    pub kind: Kind,
    /// Accepte `dry_run`.
    pub dry_run: bool,
    /// Crée une entrée d'historique.
    pub undoable: bool,
    /// Disponibilité.
    pub available: Availability,
    /// Première version du format qui la connaît.
    pub since_format: u32,
    /// Schéma JSON des paramètres.
    pub schema: fn() -> Schema,
    /// Résumé en français.
    pub summary_fr: &'static str,
    /// Résumé en anglais.
    pub summary_en: &'static str,
}

impl CommandSpec {
    /// Résumé dans la langue `lang`.
    #[must_use]
    pub const fn summary(&self, lang: Language) -> &'static str {
        match lang {
            Language::Fr => self.summary_fr,
            Language::En => self.summary_en,
        }
    }

    /// Vrai si la commande est disponible (listée par les surfaces).
    #[must_use]
    pub const fn is_listed(&self) -> bool {
        matches!(self.available, Availability::Now)
    }
}

/// Schéma JSON (2020-12) des paramètres `T`, tel que lu (contrat de
/// désérialisation), sous-schémas en ligne : un client qui ignore `$ref`
/// le lit tel quel.
#[must_use]
pub fn schema_of<T: JsonSchema>() -> Schema {
    SchemaSettings::draft2020_12()
        .with(|s| s.inline_subschemas = true)
        .for_deserialize()
        .into_generator()
        .into_root_schema_for::<T>()
}

/// Entrée disponible.
const fn now(
    id: &'static str,
    mcp_name: &'static str,
    kind: Kind,
    schema: fn() -> Schema,
    summary_fr: &'static str,
    summary_en: &'static str,
) -> CommandSpec {
    let write = matches!(kind, Kind::Write);
    CommandSpec {
        id,
        mcp_name,
        kind,
        dry_run: write,
        undoable: write,
        available: Availability::Now,
        since_format: 1,
        schema,
        summary_fr,
        summary_en,
    }
}

/// Entrée réservée au jalon `milestone` (non listée, refusée
/// `command.not_yet_available`). Aucune depuis le lot L7 de J8 ; gardée
/// pour réserver une entrée d'un jalon futur.
#[allow(
    dead_code,
    reason = "aucune entrée réservée aujourd'hui ; forme gardée pour les jalons futurs"
)]
const fn reserved(
    milestone: u8,
    id: &'static str,
    mcp_name: &'static str,
    kind: Kind,
    schema: fn() -> Schema,
    summary_fr: &'static str,
    summary_en: &'static str,
) -> CommandSpec {
    let mut spec = now(id, mcp_name, kind, schema, summary_fr, summary_en);
    spec.available = Availability::Milestone(milestone);
    spec
}

/// Entrée d'interface servie par une application rattachée (genre
/// [`Kind::Ui`], sans historique ni `dry_run`).
const fn attached(
    id: &'static str,
    mcp_name: &'static str,
    schema: fn() -> Schema,
    summary_fr: &'static str,
    summary_en: &'static str,
) -> CommandSpec {
    let mut spec = now(id, mcp_name, Kind::Ui, schema, summary_fr, summary_en);
    spec.available = Availability::Attached;
    spec
}

/// Registre, trié par id (vérifié par test).
pub static REGISTRY: &[CommandSpec] = &[
    now(
        "analysis.baseline_variance",
        "baseline_variance",
        Kind::Analysis,
        schema_of::<VarianceParams>,
        "Écarts à une ligne de base (début, fin, durée, travail), calculés par le moteur, paginés.",
        "Variance from a baseline (start, finish, duration, work), computed by the engine, paginated.",
    ),
    now(
        "analysis.critical_path",
        "critical_path",
        Kind::Read,
        schema_of::<NoParams>,
        "Chemin critique : tâches critiques dans l'ordre, fin du projet.",
        "Critical path: critical tasks in order, project finish.",
    ),
    now(
        "analysis.explain_date",
        "explain_date",
        Kind::Analysis,
        schema_of::<ExplainParams>,
        "Explique la date d'une tâche : pilote, contrainte, calendrier, avancement, chaîne des causes.",
        "Explain a task date: driver, constraint, calendar, progress, chain of causes.",
    ),
    now(
        "analysis.resource_usage",
        "resource_usage",
        Kind::Analysis,
        schema_of::<UsageParams>,
        "Utilisation des ressources : travail par période et surutilisation, calculés par le moteur.",
        "Resource usage: work per period and overallocation, computed by the engine.",
    ),
    now(
        "analysis.what_if",
        "what_if",
        Kind::Analysis,
        schema_of::<WhatIfParams>,
        "Simule un lot sans changer le projet : diff, écart de fin à une ligne de base, explications.",
        "Simulate a batch without changing the project: diff, finish variance from a baseline, explanations.",
    ),
    attached(
        "app.screenshot",
        "screenshot",
        schema_of::<ScreenshotParams>,
        "Capture PNG de la fenêtre de l'application rattachée (vérification visuelle).",
        "PNG screenshot of the attached application window (visual check).",
    ),
    attached(
        "app.ui",
        "ui_action",
        schema_of::<UiActionParams>,
        "Action d'interface dans l'application rattachée : sélection, défilement, zoom, vue, thème, langue.",
        "Interface action in the attached application: selection, scrolling, zoom, view, theme, language.",
    ),
    attached(
        "app.ui_state",
        "ui_state",
        schema_of::<NoParams>,
        "État de l'interface de l'application rattachée : vue, sélection, lignes visibles, document modifié.",
        "Interface state of the attached application: view, selection, visible rows, modified document.",
    ),
    now(
        "assignment.add",
        "assign_resource",
        Kind::Write,
        schema_of::<AssignmentAdd>,
        "Affecte une ressource à une tâche (unités, travail ou quantité, retard).",
        "Assign a resource to a task (units, work or quantity, delay).",
    ),
    now(
        "assignment.remove",
        "unassign_resource",
        Kind::Write,
        schema_of::<AssignmentKey>,
        "Retire une ressource d'une tâche.",
        "Remove a resource from a task.",
    ),
    now(
        "assignment.update",
        "update_assignment",
        Kind::Write,
        schema_of::<AssignmentUpdate>,
        "Modifie une affectation (unités, travail, quantité, retard).",
        "Update an assignment (units, work, quantity, delay).",
    ),
    now(
        "baseline.clear",
        "clear_baseline",
        Kind::Write,
        schema_of::<BaselineClear>,
        "Efface une ligne de base (0 à 10) du projet ou d'une sélection de tâches.",
        "Clear a baseline (0 to 10) from the project or a selection of tasks.",
    ),
    now(
        "baseline.set",
        "set_baseline",
        Kind::Write,
        schema_of::<BaselineSet>,
        "Enregistre une ligne de base (0 à 10) sur le projet ou une sélection, avec cumul facultatif.",
        "Set a baseline (0 to 10) on the project or a selection, with optional roll-up.",
    ),
    now(
        "batch.apply",
        "apply_batch",
        Kind::Write,
        schema_of::<ApplyBatchParams>,
        "Applique un lot de commandes en une seule transaction (étiquettes \"$x\" entre commandes).",
        "Apply a batch of commands as one transaction (\"$x\" labels across commands).",
    ),
    now(
        "calendar.add",
        "add_calendar",
        Kind::Write,
        schema_of::<CalendarAdd>,
        "Crée un calendrier : de base, dérivé d'une base, ou copie d'un calendrier existant.",
        "Create a calendar: base, derived from a base, or copy of an existing calendar.",
    ),
    now(
        "calendar.delete",
        "delete_calendar",
        Kind::Write,
        schema_of::<CalendarDelete>,
        "Supprime un calendrier inutilisé (ni projet, ni tâche, ni ressource, ni base).",
        "Delete an unused calendar (not the project's, a task's, a resource's or a base).",
    ),
    now(
        "calendar.remove_exception",
        "remove_calendar_exception",
        Kind::Write,
        schema_of::<CalendarExceptionKey>,
        "Retire une exception de calendrier.",
        "Remove a calendar exception.",
    ),
    now(
        "calendar.set_exception",
        "edit_calendar",
        Kind::Write,
        schema_of::<CalendarException>,
        "Crée ou remplace une exception de calendrier (jours chômés ou horaires).",
        "Create or replace a calendar exception (nonworking days or working times).",
    ),
    now(
        "calendar.set_week",
        "set_work_week",
        Kind::Write,
        schema_of::<CalendarSetWeek>,
        "Règle la semaine type d'un calendrier : jours hérités, chômés ou ouvrés sur des plages.",
        "Set a calendar's work week: days inherited, nonworking or working on time ranges.",
    ),
    now(
        "calendar.update",
        "update_calendar",
        Kind::Write,
        schema_of::<CalendarUpdate>,
        "Renomme un calendrier ou change sa base.",
        "Rename a calendar or change its base.",
    ),
    now(
        "custom_field.rename",
        "rename_custom_field",
        Kind::Write,
        schema_of::<CustomFieldRename>,
        "Renomme un champ personnalisé de tâche (alias de Texte1-30, Nombre1-20, Date1-10, Indicateur1-20) ou lui rend son nom ; ses valeurs se posent par task.update { custom }.",
        "Rename a task custom field (alias of Text1-30, Number1-20, Date1-10, Flag1-20) or restore its name; values are set with task.update { custom }.",
    ),
    now(
        "history.list",
        "history",
        Kind::History,
        schema_of::<NoParams>,
        "Historique des modifications annulables.",
        "History of undoable changes.",
    ),
    now(
        "history.redo",
        "redo",
        Kind::History,
        schema_of::<NoParams>,
        "Rétablit la dernière modification annulée.",
        "Redo the last undone change.",
    ),
    now(
        "history.undo",
        "undo",
        Kind::History,
        schema_of::<NoParams>,
        "Annule la dernière modification.",
        "Undo the last change.",
    ),
    now(
        "link.add",
        "add_link",
        Kind::Write,
        schema_of::<LinkSpec>,
        "Ajoute un lien entre deux tâches (FS par défaut, décalage facultatif).",
        "Add a link between two tasks (FS by default, optional lag).",
    ),
    now(
        "link.remove",
        "remove_link",
        Kind::Write,
        schema_of::<LinkKey>,
        "Retire un lien.",
        "Remove a link.",
    ),
    now(
        "link.update",
        "update_link",
        Kind::Write,
        schema_of::<LinkSpec>,
        "Modifie le type ou le décalage d'un lien.",
        "Change the type or lag of a link.",
    ),
    now(
        "progress.reschedule",
        "reschedule_remaining",
        Kind::Write,
        schema_of::<ProgressReschedule>,
        "Replanifie le travail restant après une date (défaut : date d'état) ; dates réelles inchangées.",
        "Reschedule remaining work after a date (default: status date); actual dates unchanged.",
    ),
    now(
        "progress.update",
        "update_progress",
        Kind::Write,
        schema_of::<ProgressUpdate>,
        "Met à jour l'avancement d'une tâche (% achevé, dates réelles, durées réelle et restante, travail réel).",
        "Update a task's progress (% complete, actual dates, actual and remaining duration, actual work).",
    ),
    now(
        "project.export_mspdi",
        "export_mspdi",
        Kind::File,
        schema_of::<ExportMspdiParams>,
        "Écrit une copie du projet au format MSPDI (XML de MS Project).",
        "Write a copy of the project as MSPDI (MS Project XML).",
    ),
    now(
        "project.export_pdf",
        "export_pdf",
        Kind::File,
        schema_of::<ExportPdfParams>,
        "Écrit le planning en PDF (style, mise en page, plage et lignes facultatifs).",
        "Write the schedule as PDF (optional style, page setup, range and rows).",
    ),
    now(
        "project.import_csv",
        "import_csv",
        Kind::File,
        schema_of::<ImportCsvParams>,
        "Ouvre un projet construit depuis un fichier CSV de tâches.",
        "Open a project built from a CSV file of tasks.",
    ),
    now(
        "project.new",
        "new_project",
        Kind::File,
        schema_of::<NewProjectParams>,
        "Crée un projet vide, planifié depuis une date de début ou de fin.",
        "Create an empty project, scheduled from a start or finish date.",
    ),
    now(
        "project.open",
        "open_project",
        Kind::File,
        schema_of::<OpenProjectParams>,
        "Ouvre un fichier MSPDI (XML de MS Project).",
        "Open an MSPDI file (MS Project XML).",
    ),
    now(
        "project.save",
        "save_project",
        Kind::File,
        schema_of::<SaveProjectParams>,
        "Enregistre le projet (MSPDI), sur son fichier ou sous un autre chemin.",
        "Save the project (MSPDI), to its file or another path.",
    ),
    now(
        "project.set_anchor",
        "set_project_start",
        Kind::Write,
        schema_of::<AnchorSpec>,
        "Fixe la date de début du projet, ou sa date de fin (planification à rebours).",
        "Set the project start date, or its finish date (backward scheduling).",
    ),
    now(
        "project.set_info",
        "set_project_info",
        Kind::Write,
        schema_of::<ProjectInfoPatch>,
        "Modifie les informations du projet (nom, titre, auteur…).",
        "Update the project information (name, title, author…).",
    ),
    now(
        "project.set_status_date",
        "set_status_date",
        Kind::Write,
        schema_of::<SetStatusDate>,
        "Pose ou retire la date d'état.",
        "Set or clear the status date.",
    ),
    now(
        "project.summary",
        "project_summary",
        Kind::Read,
        schema_of::<NoParams>,
        "Résumé du projet : dates, nombre de tâches, tâches critiques, erreurs.",
        "Project summary: dates, task counts, critical tasks, errors.",
    ),
    now(
        "render.gantt",
        "render_gantt",
        Kind::Read,
        schema_of::<RenderGanttParams>,
        "Diagramme de Gantt en SVG, avec plage, échelle et filtre.",
        "Gantt chart as SVG, with range, zoom and filter.",
    ),
    now(
        "resource.add",
        "add_resource",
        Kind::Write,
        schema_of::<ResourceAdd>,
        "Ajoute une ressource (travail, matérielle ou coût ; calendrier propre par défaut).",
        "Add a resource (work, material or cost; own calendar by default).",
    ),
    now(
        "resource.delete",
        "delete_resource",
        Kind::Write,
        schema_of::<ResourceDelete>,
        "Supprime une ressource (cascade obligatoire si elle est affectée).",
        "Delete a resource (cascade required if it is assigned).",
    ),
    now(
        "resource.list",
        "list_resources",
        Kind::Read,
        schema_of::<ResourceListParams>,
        "Liste des ressources, avec travail et surutilisation, paginée.",
        "List resources, with work and overallocation, paginated.",
    ),
    now(
        "resource.update",
        "update_resource",
        Kind::Write,
        schema_of::<ResourceUpdate>,
        "Modifie une ressource (nom, capacité, calendrier, initiales, groupe…).",
        "Update a resource (name, capacity, calendar, initials, group…).",
    ),
    now(
        "style.apply",
        "apply_style",
        Kind::Write,
        schema_of::<StyleApply>,
        "Applique un modèle de mise en forme ou une surcharge au document, ou revient au modèle par défaut. Un modèle garde les tâches de la chronologie et les couleurs de phase du document (sauf si `patch` les donne) ; `reset` les retire.",
        "Apply a formatting template or an override to the document, or reset to the default template. A template keeps the document's timeline tasks and phase colours (unless `patch` sets them); `reset` removes them.",
    ),
    now(
        "style.get",
        "get_style",
        Kind::Read,
        schema_of::<NoParams>,
        "Style du document : modèle de base, surcharges, style résolu et contrastes insuffisants.",
        "Document style: base template, overrides, resolved style and insufficient contrasts.",
    ),
    now(
        "style.list",
        "list_styles",
        Kind::Read,
        schema_of::<NoParams>,
        "Modèles de mise en forme intégrés et style du document.",
        "Built-in formatting templates and the document style.",
    ),
    now(
        "style.reset_to_template",
        "reset_style",
        Kind::Write,
        schema_of::<StyleResetToTemplate>,
        "Revient au modèle : retire les surcharges du style du document, ou celles d'un type de barre.",
        "Revert to the template: remove the document style overrides, or those of one bar type.",
    ),
    now(
        "style.save_as_template",
        "save_style_as_template",
        Kind::Read,
        schema_of::<StyleSaveAsTemplate>,
        "Style du document nommé, à garder comme modèle personnel (rien n'est modifié).",
        "The document style under a name, to keep as a personal template (nothing is changed).",
    ),
    now(
        "style.set_bar_role",
        "set_bar_style",
        Kind::Write,
        schema_of::<StyleSetBarRole>,
        "Met en forme un type de barre : couleur, épaisseur, extrémités, textes à gauche, à droite, dedans.",
        "Format a bar type: color, thickness, ends, texts on the left, right, inside.",
    ),
    now(
        "task.add",
        "add_task",
        Kind::Write,
        schema_of::<TaskAdd>,
        "Ajoute une tâche (placement, durée, jalon, notes ; étiquette \"as\" pour un lot).",
        "Add a task (placement, duration, milestone, notes; \"as\" label for a batch).",
    ),
    now(
        "task.delete",
        "delete_task",
        Kind::Write,
        schema_of::<TaskDelete>,
        "Supprime une tâche, avec ses liens (cascade obligatoire pour une récapitulative).",
        "Delete a task and its links (cascade required for a summary task).",
    ),
    now(
        "task.get",
        "get_task",
        Kind::Read,
        schema_of::<TaskGetParams>,
        "Détail d'une tâche : dates du moteur, marges, liens, contrainte, ressources, travail, type.",
        "Task details: engine dates, slack, links, constraint, resources, work, task type.",
    ),
    now(
        "task.indent",
        "indent_task",
        Kind::Write,
        schema_of::<TaskOne>,
        "Indente une tâche et ses sous-tâches.",
        "Indent a task and its subtasks.",
    ),
    now(
        "task.list",
        "list_tasks",
        Kind::Read,
        schema_of::<TaskListParams>,
        "Liste des tâches, filtrée (critiques, en retard, plage de dates, ressource) et paginée.",
        "List tasks, filtered (critical, late, date range, resource) and paginated.",
    ),
    now(
        "task.move",
        "move_task",
        Kind::Write,
        schema_of::<TaskMove>,
        "Déplace une tâche et ses sous-tâches (ordre et hiérarchie).",
        "Move a task and its subtasks (order and hierarchy).",
    ),
    now(
        "task.outdent",
        "outdent_task",
        Kind::Write,
        schema_of::<TaskOne>,
        "Désindente une tâche et ses sous-tâches.",
        "Outdent a task and its subtasks.",
    ),
    now(
        "task.set_constraint",
        "set_constraint",
        Kind::Write,
        schema_of::<SetConstraint>,
        "Pose une contrainte de date sur une tâche.",
        "Set a date constraint on a task.",
    ),
    now(
        "task.set_deadline",
        "set_deadline",
        Kind::Write,
        schema_of::<SetDeadline>,
        "Pose ou retire l'échéance d'une tâche.",
        "Set or clear a task deadline.",
    ),
    now(
        "task.update",
        "update_task",
        Kind::Write,
        schema_of::<TaskUpdate>,
        "Modifie une tâche (nom, durée, jalon, notes, début ou fin saisis, active, type, pilotage par l'effort, travail).",
        "Update a task (name, duration, milestone, notes, entered start or finish, active, type, effort-driven, work).",
    ),
];

/// Entrées disponibles, dans l'ordre des ids.
pub fn listed() -> impl Iterator<Item = &'static CommandSpec> {
    REGISTRY.iter().filter(|s| s.is_listed())
}

/// Entrée d'id `id` (recherche dichotomique).
#[must_use]
pub fn lookup(id: &str) -> Option<&'static CommandSpec> {
    REGISTRY
        .binary_search_by(|s| s.id.cmp(id))
        .ok()
        .and_then(|i| REGISTRY.get(i))
}

/// Entrée de nom MCP `n`.
#[must_use]
pub fn by_mcp_name(n: &str) -> Option<&'static CommandSpec> {
    REGISTRY.iter().find(|s| s.mcp_name == n)
}
