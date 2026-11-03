//! Paramètres des commandes du registre qui ne sont pas des commandes
//! d'écriture d'un lot : lecture, analyse, historique, fichiers, et
//! `batch.apply` (ADR-0012 D7, lot L3).
//!
//! Ces types fixent la forme JSON et le schéma de chaque outil ; leur
//! exécution est écrite par `Workspace` (L6). Les surfaces ajoutent
//! elles-mêmes `dry_run` (écriture) ou `detail` (diff détaillé) au schéma
//! (L8). Champs inconnus refusés partout.

use plannercraft_core::{Day, Moment};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Batch, Command, Patch, TaskRef};
use crate::json::{DaySchema, GranularityJson, MomentSchema, SideJson};
use crate::text::Language;

/// Paramètres vides (`project.summary`, `analysis.critical_path`,
/// `history.undo`, `history.redo`, `history.list`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoParams {}

/// `project.open` : ouvre un fichier MSPDI.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenProjectParams {
    /// Chemin du fichier MSPDI (`.xml`).
    pub path: String,
}

/// `project.new` : projet vide, planifié depuis `start` ou à rebours depuis
/// `finish` (exactement un des deux).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewProjectParams {
    /// Nom du projet.
    pub name: String,
    /// Date de début (planification depuis le début).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<MomentSchema>")]
    pub start: Option<Moment>,
    /// Date de fin (planification à rebours).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<MomentSchema>")]
    pub finish: Option<Moment>,
    /// Langue des textes du projet (défaut : celle de la surface).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<Language>,
}

/// `task.list` : tâches dans l'ordre des ID, filtrées et paginées.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskListParams {
    /// Tâches critiques seulement.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub critical: bool,
    /// Tâches en retard seulement : fin planifiée après l'échéance ou après
    /// la fin de la ligne de base 0, non achevée à fin passée ou non
    /// commencée à début passé à la date d'état.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub late: bool,
    /// Tâches dont les dates planifiées touchent ce jour ou un jour suivant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<DaySchema>")]
    pub from: Option<Day>,
    /// Tâches dont les dates planifiées touchent ce jour ou un jour précédent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<DaySchema>")]
    pub to: Option<Day>,
    /// Niveau hiérarchique maximal (1 : premier niveau seulement).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub outline_max: Option<u16>,
    /// Récapitulatives seulement (`true`) ou feuilles seulement (`false`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<bool>,
    /// Reprise après la tâche de cet UID (curseur `next_cursor` d'une page
    /// précédente).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub cursor: Option<u32>,
    /// Taille de la page (défaut 50, au plus 500).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 500))]
    pub limit: Option<u32>,
    /// Tâches affectées à cette ressource (UID, J5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<u32>,
    /// Feuilles planifiées non achevées (< 100 %) seulement (`true`), ou
    /// les autres tâches (`false`) (J8, R-251).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incomplete: Option<bool>,
    /// Jalons seulement (`true`) ou hors jalons (`false`) (J8, R-251).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub milestone: Option<bool>,
    /// Tâches actives (`true`) ou inactives (`false`) (J8, R-251).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    /// Nom contenant ce texte, sans casse ni accents (J8, R-251 ; au plus
    /// 256 caractères).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 256))]
    pub name_contains: Option<String>,
}

/// `resource.list` (J5, R-195) : ressources dans l'ordre des UID, filtrées
/// et paginées comme `task.list`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResourceListParams {
    /// Ressources surutilisées seulement (R-186).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub overallocated: bool,
    /// Reprise après la ressource de cet UID (curseur `next_cursor`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<u32>,
    /// Taille de la page (défaut 50, au plus 500).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 500))]
    pub limit: Option<u32>,
}

/// `analysis.resource_usage` (J5, R-188) : travail par période et par
/// ressource, calculé par le moteur à la demande.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UsageParams {
    /// Début de la plage (défaut : début du projet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<MomentSchema>")]
    pub from: Option<Moment>,
    /// Fin de la plage, exclue (défaut : fin du projet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<MomentSchema>")]
    pub to: Option<Moment>,
    /// Granularité des périodes (défaut : semaine).
    #[serde(default)]
    pub by: GranularityJson,
    /// Ressources (UID) ; vide : toutes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<u32>,
    /// Ajoute une ligne par affectation et le total de chaque tâche
    /// (J7, R-245 ; défaut : non).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub assignments: bool,
}

/// `analysis.explain_date` (J6, R-230 à R-233) : pourquoi une tâche
/// commence (ou finit) à sa date ; calculé par le moteur à la demande.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExplainParams {
    /// Tâche (UID).
    pub task: TaskRef,
    /// Date expliquée (défaut : début).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<SideJson>,
    /// Profondeur de la chaîne des pilotes (défaut 5, de 1 à 50).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 50))]
    pub depth: Option<u8>,
}

/// `analysis.what_if` (J6, R-234) : simule un lot sur une copie, sans
/// changer la session ; un lot refusé est rendu comme donnée.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WhatIfParams {
    /// Opérations simulées, dans l'ordre (même forme que `apply_batch`).
    pub ops: Vec<Command>,
    /// Ligne de base de référence de l'écart de fin du projet (0 à 10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(max = 10))]
    pub baseline: Option<u8>,
    /// Tâches expliquées sur l'état simulé (au plus 5).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(length(max = 5))]
    pub explain: Vec<TaskRef>,
    /// Diff détaillé.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub detail: bool,
}

/// `analysis.baseline_variance` (J6, R-228) : écarts à une ligne de base,
/// ligne du projet puis tâches, paginés.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VarianceParams {
    /// Ligne de base (0 à 10 ; défaut 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(max = 10))]
    pub baseline: Option<u8>,
    /// Tâches à écart non nul seulement.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub only_varied: bool,
    /// Reprise d'une page précédente (son `next_cursor` ; sens fixé par
    /// le lot L8, R-228).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<u32>,
    /// Taille de la page (défaut 50, au plus 500).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 500))]
    pub limit: Option<u32>,
}

/// `task.get` : une tâche.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskGetParams {
    /// UID de la tâche.
    #[schemars(range(min = 1))]
    pub uid: u32,
}

/// Échelle de temps d'un rendu.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ZoomJson {
    /// Choisie selon l'étendue du projet.
    #[default]
    Auto,
    /// Jours.
    Day,
    /// Semaines.
    Week,
    /// Mois.
    Month,
    /// Trimestres.
    Quarter,
}

/// `render.gantt` : diagramme de Gantt en SVG, borné.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderGanttParams {
    /// Premier jour affiché (défaut : début du projet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<DaySchema>")]
    pub from: Option<Day>,
    /// Dernier jour affiché (défaut : fin du projet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<DaySchema>")]
    pub to: Option<Day>,
    /// Échelle de temps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoom: Option<ZoomJson>,
    /// Tâches critiques seulement.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub critical_only: bool,
    /// Lignes affichées au plus (au plus 200).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 200))]
    pub max_rows: Option<u32>,
    /// Modèle de mise en forme (défaut : style du document, sinon
    /// Classique, apparence J7).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<super::style::TemplateJson>,
}

/// `project.save` : enregistre en MSPDI, sur le fichier ouvert ou sous
/// `path`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SaveProjectParams {
    /// Chemin (défaut : le fichier ouvert).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// `project.export_mspdi` : écrit une copie MSPDI sans changer le fichier
/// de la session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportMspdiParams {
    /// Chemin du fichier écrit.
    pub path: String,
}

/// `project.import_csv` : ouvre un projet construit depuis un CSV de tâches
/// (R-144 à R-146).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportCsvParams {
    /// Chemin du fichier CSV.
    pub path: String,
    /// Langue des en-têtes et des valeurs (défaut : celle de la surface).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<Language>,
}

/// `batch.apply` : lot atomique de commandes (`format_version` facultatif,
/// 1 par défaut).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApplyBatchParams {
    /// Version du format (1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format_version: Option<u32>,
    /// Langue des textes (durées).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<Language>,
    /// Opérations, dans l'ordre ; `{"op":"task.add",…}`.
    pub ops: Vec<Command>,
    /// Libellé de l'entrée d'historique.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl ApplyBatchParams {
    /// Lot correspondant (`format_version` 1 par défaut ; une autre version
    /// est refusée par `Session::apply`).
    #[must_use]
    pub fn into_batch(self) -> Batch {
        Batch {
            format_version: self.format_version.unwrap_or(1),
            lang: self.lang,
            ops: self.ops,
            label: self.label,
        }
    }
}

/// Vue principale demandée par `app.ui` (J7, ADR-0015 D6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ViewJson {
    /// Diagramme de Gantt.
    Gantt,
    /// Feuille des ressources.
    ResourceSheet,
    /// Utilisation des ressources.
    ResourceUsage,
}

/// Thème demandé par `app.ui` (J7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ThemeJson {
    /// Thème du système.
    System,
    /// Clair.
    Light,
    /// Sombre.
    Dark,
}

/// Langue d'interface demandée par `app.ui` (J7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UiLanguageJson {
    /// Langue du système.
    System,
    /// Français.
    Fr,
    /// Anglais.
    En,
}

/// `app.ui` (J7, ADR-0015 D11, disponibilité `Attached`) : actions
/// d'interface sans historique, appliquées dans cet ordre ; au moins un
/// champ (contrôle : lot L3). Aucune ne modifie le projet.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UiActionParams {
    /// Sélectionne la tâche d'UID donné (`null` : aucune sélection).
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub select: Patch<u32>,
    /// Fait défiler jusqu'à la tâche d'UID donné.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scroll_to: Option<u32>,
    /// Zoom de l'échelle de temps (`auto` : ajusté au projet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoom: Option<ZoomJson>,
    /// Vue principale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<ViewJson>,
    /// Thème.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeJson>,
    /// Langue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<UiLanguageJson>,
}

/// `app.screenshot` (J7, ADR-0015 D11, disponibilité `Attached`) : PNG de
/// la fenêtre, réduit de moitié `scale_down` fois (C-CTL-08).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScreenshotParams {
    /// Réductions de moitié demandées (0 à 2 ; défaut 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(max = 2))]
    pub scale_down: Option<u8>,
}

/// `project.export_pdf` (J8, contrat L0 ; exécution en L7) : écrit le
/// planning en PDF.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportPdfParams {
    /// Chemin du fichier écrit.
    pub path: String,
    /// Modèle de mise en forme (défaut : style du document).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<super::style::TemplateJson>,
    /// Mise en page (défaut : celle du style).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<PdfPageJson>,
    /// Plage de jours (défaut : tout le projet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<PdfRangeJson>,
    /// Lignes exportées (défaut : toutes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<PdfRowsJson>,
}

/// Papier d'un export PDF.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PaperJson {
    /// A4.
    A4,
    /// A3.
    A3,
    /// Lettre US.
    Letter,
    /// Tabloïd US.
    Tabloid,
}

/// Mise en page d'un export PDF.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PdfPageJson {
    /// Papier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paper: Option<PaperJson>,
    /// Paysage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub landscape: Option<bool>,
    /// Pages en largeur (1 à 20).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 20))]
    pub pages_wide: Option<u16>,
}

/// Plage de jours d'un export PDF.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PdfRangeJson {
    /// Premier jour.
    #[schemars(with = "DaySchema")]
    pub from: Day,
    /// Dernier jour.
    #[schemars(with = "DaySchema")]
    pub to: Day,
}

/// Lignes d'un export PDF.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PdfRowsJson {
    /// Tâches critiques seulement.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub critical_only: bool,
    /// Niveau hiérarchique maximal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub outline_max: Option<u16>,
}
