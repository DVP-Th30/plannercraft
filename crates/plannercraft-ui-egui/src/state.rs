//! État d'interface sérialisable (`UiState`), séparé du modèle.
//!
//! `UiState` regroupe les préférences et la position de lecture, en JSON
//! (`serde_json`), sans aucune donnée du projet ni aucun flottant : le JSON est
//! déterministe et `UiState: Eq`. Lecture tolérante, champ par champ : champ
//! absent → valeur par défaut, champ inconnu ignoré (une version plus récente
//! a pu écrire davantage), champ illisible → sa valeur par défaut sans perdre
//! les autres, entrée de colonne illisible (colonne d'une version future)
//! ignorée.
//!
//! J7 (ADR-0015 D6, contrat L0) : vue principale ([`ViewKind`]), style du
//! Gantt ([`GanttStyle`]) et fichiers récents ([`RecentFile`]), champs
//! ajoutés avec valeur par défaut : la version reste 1 (un état J6 se lit
//! sans avertissement).
//!
//! J8 (ADR-0016, contrat L0) : préférences de la vue des tâches
//! ([`TaskViewPrefs`] : niveau affiché, surbrillance du critique), modèle
//! de mise en forme par défaut, modèles personnels ([`NamedStyle`]) et
//! écriture du style dans le fichier ; même règle, la version reste 1 (un
//! état J7 se lit sans avertissement). Filtre, tri et repli sont des vues
//! non persistées ([`TaskViewState`]).
//!
//! ADR-0017 (contrat L0) : préférences du ruban ([`RibbonPrefs`] : onglet,
//! repli) et visibilité de la chronologie ; même règle, la version reste 1
//! (un état J8 se lit sans avertissement).
//!
//! Seconde passe UX (ADR-0019, contrat L0) : **version 2**. `columns`
//! devient la liste des colonnes **affichées**, dans l'ordre d'affichage
//! (masquer = retirer ; titre et alignements de « Paramètres de champ ») ;
//! un état version 1 est relu avec ses colonnes visibles, dans l'ordre de
//! [`Column::ALL`], largeurs gardées (P63). Champs ajoutés : hauteur de la
//! chronologie ([`UiState::timeline_height`]) et volet de détails
//! ([`DetailsPrefs`]).

use plannercraft_automation::filter::{FilterPreset, TaskFilter};
use plannercraft_render::style::{DocStyle, TemplateId};
use serde::{Deserialize, Serialize};

use crate::column::{Column, ColumnState, MAX_COLUMN_TITLE, MAX_COLUMNS};
use crate::ribbon::RibbonPrefs;
use crate::timescale::TimeScaleZoom;

/// Version du format JSON. Elle s'incrémente quand le sens d'un champ change
/// ou qu'un champ est retiré ou renommé ; ajouter un champ avec valeur par
/// défaut ne l'incrémente pas.
pub const UI_STATE_VERSION: u32 = 2;

/// Clé de stockage eframe (desktop : fichier de `storage_dir`, web :
/// `localStorage`). Contient le nom technique : listée dans `docs/rename.md`.
pub const STATE_KEY: &str = "plannercraft.ui_state";

/// Clé des versions nommées `planrs` (avant le 2026-11-03), relue
/// seulement si [`STATE_KEY`] est absente ([`stored_state`]) ; jamais
/// écrite.
pub const LEGACY_STATE_KEY: &str = "planrs.ui_state";

/// État d'interface stocké : [`STATE_KEY`], à défaut [`LEGACY_STATE_KEY`]
/// (reprise sans perte après le renommage, desktop et web ; la
/// sauvegarde suivante l'écrit sous la nouvelle clé).
#[must_use]
pub fn stored_state(get: impl Fn(&str) -> Option<String>) -> Option<String> {
    get(STATE_KEY).or_else(|| get(LEGACY_STATE_KEY))
}

/// Largeur minimale de la table, en points logiques.
pub const TABLE_WIDTH_MIN: u16 = 120;

/// Largeur maximale de la table, en points logiques.
pub const TABLE_WIDTH_MAX: u16 = 4_000;

/// Largeur par défaut de la table : somme des colonnes visibles par défaut.
pub const TABLE_WIDTH_DEFAULT: u16 = 670;

/// Fichiers récents gardés au plus (H-UI-28).
pub const MAX_RECENT_FILES: usize = 10;

/// Modèles personnels gardés au plus (J8).
pub const MAX_USER_TEMPLATES: usize = 32;

/// Longueur maximale du nom d'un modèle personnel, en caractères (J8).
pub const MAX_USER_TEMPLATE_NAME: usize = 64;

/// Niveau hiérarchique affiché au plus par « Afficher jusqu'au niveau n »
/// (H-UI-34).
pub const MAX_OUTLINE_LEVEL: u8 = 9;

/// Hauteur par défaut de la chronologie, en points (H-UI-78, H-UI-111).
pub const TIMELINE_HEIGHT_DEFAULT: u16 = 72;

/// Hauteur minimale de la chronologie, en points (H-UI-111).
pub const TIMELINE_HEIGHT_MIN: u16 = 48;

/// Hauteur par défaut du volet de détails, en points (H-UI-111) : 360,
/// pour que les grilles du Formulaire tâche soient à l'écran dès
/// l'ouverture en 1 440 × 900 (relecture finale ux2, C5).
pub const DETAILS_HEIGHT_DEFAULT: u16 = 360;

/// Hauteur minimale du volet de détails, en points (H-UI-111).
pub const DETAILS_HEIGHT_MIN: u16 = 120;

/// Hauteur maximale enregistrée d'un volet, en points ; la borne relative à
/// la fenêtre (40 % et 60 %) s'applique à la disposition (`panes::layout`).
pub const PANE_HEIGHT_MAX: u16 = 4_000;

/// Préférences d'interface persistées.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "préférences indépendantes, persistées telles quelles"
)]
pub struct UiState {
    /// Version du format ([`UI_STATE_VERSION`]).
    pub version: u32,
    /// Langue demandée.
    pub language: LanguagePref,
    /// Thème demandé.
    pub theme: ThemePref,
    /// Dernier zoom choisi (écrasé par l'ajustement à chaque ouverture).
    pub zoom: TimeScaleZoom,
    /// Colonnes **affichées**, dans l'ordre d'affichage (version 2) : sans
    /// doublon, ID en premier, Nom présent, [`MAX_COLUMNS`] au plus.
    pub columns: Vec<ColumnState>,
    /// Largeur de la table, en points logiques.
    pub table_width: u16,
    /// Panneau des diagnostics ouvert.
    pub diagnostics_open: bool,
    /// Vue principale (J7).
    pub view: ViewKind,
    /// Style du Gantt (J7).
    pub gantt_style: GanttStyle,
    /// Fichiers récents, le plus récent en tête, [`MAX_RECENT_FILES`] au
    /// plus (J7, H-UI-28).
    pub recent: Vec<RecentFile>,
    /// Préférences de la vue des tâches (J8).
    pub task_view: TaskViewPrefs,
    /// Modèle des documents sans style (J8, H-UI-43 : Classique jusqu'au
    /// lot L15, Sobre ensuite).
    pub default_template: TemplateId,
    /// Modèles personnels, [`MAX_USER_TEMPLATES`] au plus (J8).
    pub user_templates: Vec<NamedStyle>,
    /// Écrire le style choisi dans le fichier (`<?plannercraft-style?>`, J8,
    /// H-R12) ; faux : jamais.
    pub write_style_to_file: bool,
    /// « Graphique seul » à l'écran (J8 lot L15, H-UI-64) : la table est
    /// réduite au N° et au Nom, les colonnes choisies sont gardées.
    pub chart_only: bool,
    /// Ruban : dernier onglet, repli (ADR-0017, H-UI-69, H-UI-72).
    pub ribbon: RibbonPrefs,
    /// Chronologie affichée au-dessus du Gantt (ADR-0017, H-UI-78 : vrai par
    /// défaut).
    pub timeline_visible: bool,
    /// Hauteur de la chronologie, en points (seconde passe UX, H-UI-111 ;
    /// [`TIMELINE_HEIGHT_DEFAULT`] par défaut).
    pub timeline_height: u16,
    /// Volet de détails « Formulaire tâche » (seconde passe UX, H-UI-111,
    /// H-UI-112).
    pub details: DetailsPrefs,
    /// Colonne Indicateurs déjà ajoutée (finitions P1, H-UI-122) : un état
    /// mémorisé sans ce champ (ou à faux) reçoit la colonne une fois, après
    /// l'ID ; la masquer ensuite est respecté.
    pub indicators_added: bool,
    /// Assistant IA (ADR-0024, lot A1) : activation explicite et modèle
    /// choisi. Jamais la clé (envoyée aux agents MCP rattachés sinon).
    pub assistant: AssistantPrefs,
}

/// Longueur au plus de l'identifiant de modèle mémorisé.
pub const MAX_ASSISTANT_MODEL_CHARS: usize = 100;

/// Préférences de l'assistant IA (ADR-0024, lot A1, H-UI-138).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AssistantPrefs {
    /// Activé par l'utilisateur après la mention « vous échangez avec une
    /// IA » (AI Act art. 50) ; faux par défaut.
    pub enabled: bool,
    /// Modèle OpenRouter de la liste fermée ; un identifiant inconnu est
    /// remplacé par le défaut à l'usage.
    pub model: String,
}

impl Default for AssistantPrefs {
    fn default() -> Self {
        Self {
            enabled: false,
            model: plannercraft_assistant::openrouter::DEFAULT_MODEL.to_owned(),
        }
    }
}

/// Préférences du volet de détails (seconde passe UX, ADR-0019 D5, D6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DetailsPrefs {
    /// Volet affiché (faux par défaut ; Affichage → Détails).
    pub visible: bool,
    /// Hauteur, en points ([`DETAILS_HEIGHT_DEFAULT`] par défaut).
    pub height: u16,
    /// Forme des deux grilles du bas (clic droit dans le volet).
    pub form: DetailsForm,
}

impl Default for DetailsPrefs {
    fn default() -> Self {
        Self {
            visible: false,
            height: DETAILS_HEIGHT_DEFAULT,
            form: DetailsForm::default(),
        }
    }
}

/// Forme du volet de détails, comme le Formulaire tâche de MS Project
/// (H-UI-112).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetailsForm {
    /// Ressources et prédécesseurs (défaut).
    #[default]
    ResourcesPredecessors,
    /// Ressources et successeurs.
    ResourcesSuccessors,
    /// Prédécesseurs et successeurs.
    PredecessorsSuccessors,
}

/// Préférences persistées de la vue des tâches (J8, ADR-0016 D2) : seul
/// « Afficher jusqu'au niveau n » et la surbrillance du critique sont
/// gardés ; filtre, tri et repli ne le sont pas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskViewPrefs {
    /// Niveau affiché (1 à 9 ; `None` : tous).
    pub outline_level: Option<u8>,
    /// Surbrillance du chemin critique (H-UI-38, vrai par défaut).
    pub highlight_critical: bool,
}

impl Default for TaskViewPrefs {
    fn default() -> Self {
        Self {
            outline_level: None,
            highlight_critical: true,
        }
    }
}

/// Modèle personnel : nom et style (modèle de base et surcharges).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedStyle {
    /// Nom affiché (≤ [`MAX_USER_TEMPLATE_NAME`] caractères).
    pub name: String,
    /// Style.
    pub style: DocStyle,
}

/// Choix de filtre de la vue des tâches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FilterChoice {
    /// Filtre prédéfini.
    Preset(FilterPreset),
    /// Filtre personnalisé.
    Custom(TaskFilter),
}

impl Default for FilterChoice {
    fn default() -> Self {
        Self::Preset(FilterPreset::All)
    }
}

/// Clé de tri (H-UI-31).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortKey {
    /// Ordre des ID (Maj+F3).
    #[default]
    Id,
    /// Nom.
    Name,
    /// Début.
    Start,
    /// Fin.
    Finish,
    /// Durée.
    Duration,
    /// % achevé.
    PercentComplete,
    /// Marge totale.
    TotalSlack,
}

impl SortKey {
    /// Clés proposées par la fenêtre « Trier » (J8, lot L9), dans l'ordre.
    pub const ALL: [Self; 7] = [
        Self::Id,
        Self::Name,
        Self::Start,
        Self::Finish,
        Self::Duration,
        Self::PercentComplete,
        Self::TotalSlack,
    ];

    /// Colonne dont le titre nomme la clé.
    #[must_use]
    pub const fn column(self) -> Column {
        match self {
            Self::Id => Column::Id,
            Self::Name => Column::Name,
            Self::Start => Column::Start,
            Self::Finish => Column::Finish,
            Self::Duration => Column::Duration,
            Self::PercentComplete => Column::PercentComplete,
            Self::TotalSlack => Column::TotalSlack,
        }
    }
}

/// Tri de la vue des tâches (sans renumérotation).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SortSpec {
    /// Clé.
    pub key: SortKey,
    /// Décroissant.
    pub descending: bool,
    /// « Conserver la structure » (tri des frères, coché par défaut).
    pub keep_outline: bool,
}

/// État de la vue des tâches (J8, ADR-0016 D2) : une vue, rien d'écrit
/// (P50). Contrat posé par le lot L0, lu par `RowView` en L5.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskViewState {
    /// Filtre.
    pub filter: FilterChoice,
    /// Tri (`None` : ordre des ID).
    pub sort: Option<SortSpec>,
    /// Niveau affiché (`None` : tous).
    pub outline_level: Option<u8>,
    /// Surbrillance du chemin critique.
    pub highlight_critical: bool,
}

impl TaskViewState {
    /// Filtre effectif (préréglage déplié).
    #[must_use]
    pub fn task_filter(&self) -> TaskFilter {
        match &self.filter {
            FilterChoice::Preset(preset) => TaskFilter::from_preset(preset),
            FilterChoice::Custom(filter) => filter.clone(),
        }
    }

    /// Un filtre est posé (autre que « Toutes les tâches »).
    #[must_use]
    pub fn is_filtered(&self) -> bool {
        match &self.filter {
            FilterChoice::Preset(FilterPreset::All) => false,
            FilterChoice::Preset(_) => true,
            FilterChoice::Custom(filter) => !filter.is_empty(),
        }
    }

    /// Un tri est posé (autre que l'ordre des ID croissant).
    #[must_use]
    pub fn is_sorted(&self) -> bool {
        self.sort
            .is_some_and(|spec| spec.key != SortKey::Id || spec.descending)
    }

    /// Sous tri ou filtre, les opérations de structure qui déplacent des
    /// lignes sont grisées (H-UI-32, J8 lot L9).
    #[must_use]
    pub fn reorder_locked(&self) -> bool {
        self.is_filtered() || self.is_sorted()
    }
}

impl Default for TaskViewState {
    fn default() -> Self {
        let prefs = TaskViewPrefs::default();
        Self {
            filter: FilterChoice::default(),
            sort: None,
            outline_level: prefs.outline_level,
            highlight_critical: prefs.highlight_critical,
        }
    }
}

/// Vue principale (J7, ADR-0015 D6).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewKind {
    /// Diagramme de Gantt (table des tâches et barres).
    #[default]
    Gantt,
    /// Feuille des ressources (lot L10).
    ResourceSheet,
    /// Utilisation des ressources (lot L11).
    ResourceUsage,
}

/// Style du Gantt (J7, H-UI-22).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GanttStyle {
    /// Barres planifiées seules.
    #[default]
    Standard,
    /// Gantt de suivi : ligne de base 0, bande d'avancement (lot L7).
    Tracking,
}

/// Fichier récent (J7, H-UI-28) : chemin et nom affiché.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecentFile {
    /// Chemin du fichier.
    pub path: String,
    /// Nom affiché.
    pub name: String,
}

/// Langue demandée par l'utilisateur.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LanguagePref {
    /// Langue du système (repli : français).
    #[default]
    System,
    /// Français.
    Fr,
    /// Anglais.
    En,
}

/// Thème demandé par l'utilisateur.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemePref {
    /// Thème du système.
    #[default]
    System,
    /// Clair.
    Light,
    /// Sombre.
    Dark,
}

/// Avertissement de lecture de l'état : affiché une fois, jamais bloquant.
///
/// Le texte (`Display`) est inséré tel quel dans le message traduit
/// `state-load-warning` : il ne contient donc aucun mot d'une langue, seulement
/// des repères techniques (« JSON 1:5 », « v2 > v1 », noms de champs).
#[derive(Debug, thiserror::Error)]
pub enum StateLoadError {
    /// JSON illisible : l'état par défaut est utilisé. Texte : position de
    /// l'erreur (« JSON 1:5 »).
    #[error("JSON {}:{}", .0.line(), .0.column())]
    Json(#[from] serde_json::Error),
    /// JSON valide mais pas un objet : l'état par défaut est utilisé.
    #[error("JSON")]
    NotObject,
    /// État écrit par une version plus récente : état par défaut (le sens des
    /// champs a pu changer).
    #[error("v{found} > v{supported}")]
    TooNew {
        /// Version trouvée.
        found: u32,
        /// Version la plus récente comprise.
        supported: u32,
    },
    /// Champs illisibles remplacés par leur valeur par défaut, les autres
    /// étant conservés. Texte : noms JSON des champs (« theme, columns »).
    #[error("{}", .0.join(", "))]
    Fields(Vec<&'static str>),
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            version: UI_STATE_VERSION,
            language: LanguagePref::default(),
            theme: ThemePref::default(),
            zoom: TimeScaleZoom::default(),
            columns: default_columns(),
            table_width: TABLE_WIDTH_DEFAULT,
            diagnostics_open: false,
            view: ViewKind::default(),
            gantt_style: GanttStyle::default(),
            recent: Vec::new(),
            task_view: TaskViewPrefs::default(),
            // Sobre retouché (H-UI-62 à H-UI-64) après les retours du
            // propriétaire du 2026-10-26 (J8 lot L15, H-UI-43).
            default_template: TemplateId::Sobre,
            user_templates: Vec::new(),
            write_style_to_file: true,
            chart_only: false,
            ribbon: RibbonPrefs::default(),
            timeline_visible: true,
            timeline_height: TIMELINE_HEIGHT_DEFAULT,
            details: DetailsPrefs::default(),
            indicators_added: true,
            assistant: AssistantPrefs::default(),
        }
    }
}

/// Colonnes affichées d'un état neuf : ID, Indicateurs (finitions P1,
/// H-UI-122), puis les colonnes visibles par défaut de [`Column::ALL`].
fn default_columns() -> Vec<ColumnState> {
    let mut columns: Vec<ColumnState> = Column::ALL
        .into_iter()
        .filter(|c| c.default_visible())
        .map(ColumnState::default_for)
        .collect();
    columns.insert(1.min(columns.len()), ColumnState::default_for(Column::Indicators));
    columns
}

/// Migration unique de la colonne Indicateurs (H-UI-122) : ajoutée après
/// l'ID si elle n'a jamais été ajoutée (`state.indicators_added` faux),
/// qu'elle n'est pas déjà affichée et qu'il reste de la place ; la table
/// s'élargit d'autant (les autres colonnes gardent leur largeur visible).
/// Le drapeau passe à vrai.
fn add_indicators_once(state: &mut UiState) {
    let columns = &mut state.columns;
    if !state.indicators_added
        && !columns.iter().any(|c| c.column == Column::Indicators)
        && columns.len() < MAX_COLUMNS
    {
        let at = columns
            .iter()
            .position(|c| c.column == Column::Id)
            .map_or(0, |i| i.saturating_add(1));
        let added = ColumnState::default_for(Column::Indicators);
        state.table_width = state
            .table_width
            .saturating_add(added.width)
            .clamp(TABLE_WIDTH_MIN, TABLE_WIDTH_MAX);
        columns.insert(at, added);
    }
    state.indicators_added = true;
}

impl UiState {
    /// Lecture tolérante : jamais d'échec. Rend l'état normalisé et, s'il y a
    /// lieu, l'avertissement à afficher une fois.
    ///
    /// - JSON illisible : état par défaut et [`StateLoadError::Json`] ;
    ///   JSON valide qui n'est pas un objet : [`StateLoadError::NotObject`] ;
    /// - `version` supérieure à [`UI_STATE_VERSION`] : état par défaut et
    ///   [`StateLoadError::TooNew`] (le sens des champs a pu changer) ;
    /// - sinon, chaque champ est lu indépendamment : absent → défaut, inconnu
    ///   → ignoré, illisible → défaut et nom du champ dans
    ///   [`StateLoadError::Fields`] ; dans `columns`, une entrée illisible
    ///   est écartée (et signalée), les autres sont gardées.
    #[must_use]
    pub fn from_json_lenient(json: &str) -> (Self, Option<StateLoadError>) {
        let value = match serde_json::from_str::<serde_json::Value>(json) {
            Ok(value) => value,
            Err(error) => return (Self::default(), Some(StateLoadError::Json(error))),
        };
        let Some(object) = value.as_object() else {
            return (Self::default(), Some(StateLoadError::NotObject));
        };
        // La version est lue avant le reste : un état plus récent peut avoir
        // changé le sens d'un champ connu.
        if let Some(found) = object.get("version").and_then(serde_json::Value::as_u64)
            && found > u64::from(UI_STATE_VERSION)
        {
            let found = u32::try_from(found).unwrap_or(u32::MAX);
            return (
                Self::default(),
                Some(StateLoadError::TooNew {
                    found,
                    supported: UI_STATE_VERSION,
                }),
            );
        }
        let mut reader = FieldReader {
            object,
            bad: Vec::new(),
        };
        let defaults = Self::default();
        // Version 1 (ou absente) : colonnes au format v1 (une entrée par
        // colonne, `visible`), migrées vers la liste des colonnes affichées.
        let v1 = object
            .get("version")
            .and_then(serde_json::Value::as_u64)
            .is_none_or(|v| v < 2);
        let state = Self {
            version: reader.field("version", defaults.version),
            language: reader.field("language", defaults.language),
            theme: reader.field("theme", defaults.theme),
            zoom: reader.field("zoom", defaults.zoom),
            columns: if v1 {
                reader.columns_v1(defaults.columns)
            } else {
                reader.list("columns", defaults.columns)
            },
            table_width: reader.field("table_width", defaults.table_width),
            diagnostics_open: reader.field("diagnostics_open", defaults.diagnostics_open),
            view: reader.field("view", defaults.view),
            gantt_style: reader.field("gantt_style", defaults.gantt_style),
            recent: reader.field("recent", defaults.recent),
            task_view: reader.field("task_view", defaults.task_view),
            default_template: reader.field("default_template", defaults.default_template),
            user_templates: reader.list("user_templates", defaults.user_templates),
            write_style_to_file: reader.field("write_style_to_file", defaults.write_style_to_file),
            chart_only: reader.field("chart_only", defaults.chart_only),
            ribbon: reader.field("ribbon", defaults.ribbon),
            timeline_visible: reader.field("timeline_visible", defaults.timeline_visible),
            timeline_height: reader.field("timeline_height", defaults.timeline_height),
            details: reader.field("details", defaults.details),
            indicators_added: reader.field("indicators_added", false),
            assistant: reader.field("assistant", defaults.assistant),
        };
        let mut state = state.normalized();
        add_indicators_once(&mut state);
        let warning = (!reader.bad.is_empty()).then_some(StateLoadError::Fields(reader.bad));
        (state.normalized(), warning)
    }

    /// JSON compact déterministe (ordre des champs fixe, aucune map).
    ///
    /// # Errors
    ///
    /// Erreur de sérialisation de `serde_json` (non attendue pour ce type).
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Inscrit `path` en tête des fichiers récents (H-UI-28) : une entrée
    /// déjà présente remonte (nom mis à jour), le plus ancien sort au-delà
    /// de [`MAX_RECENT_FILES`].
    pub fn remember_recent(&mut self, path: &str, name: &str) {
        self.recent.retain(|recent| recent.path != path);
        self.recent.insert(
            0,
            RecentFile {
                path: path.to_owned(),
                name: name.to_owned(),
            },
        );
        self.recent.truncate(MAX_RECENT_FILES);
        self.user_templates.truncate(MAX_USER_TEMPLATES);
        for t in &mut self.user_templates {
            if t.name.chars().count() > MAX_USER_TEMPLATE_NAME {
                t.name = t.name.chars().take(MAX_USER_TEMPLATE_NAME).collect();
            }
        }
        self.task_view.outline_level = self
            .task_view
            .outline_level
            .map(|n| n.clamp(1, MAX_OUTLINE_LEVEL));
    }

    /// Retire `path` des fichiers récents (fichier introuvable, H-UI-28).
    pub fn forget_recent(&mut self, path: &str) {
        self.recent.retain(|recent| recent.path != path);
    }

    /// Bornes appliquées : colonnes par [`normalize_columns`], largeur de
    /// table bornée à `[TABLE_WIDTH_MIN, TABLE_WIDTH_MAX]`, hauteurs des
    /// volets bornées, version ramenée à [`UI_STATE_VERSION`], fichiers
    /// récents coupés à [`MAX_RECENT_FILES`].
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.version = UI_STATE_VERSION;
        self.recent.truncate(MAX_RECENT_FILES);
        self.user_templates.truncate(MAX_USER_TEMPLATES);
        for t in &mut self.user_templates {
            if t.name.chars().count() > MAX_USER_TEMPLATE_NAME {
                t.name = t.name.chars().take(MAX_USER_TEMPLATE_NAME).collect();
            }
        }
        self.task_view.outline_level = self
            .task_view
            .outline_level
            .map(|n| n.clamp(1, MAX_OUTLINE_LEVEL));
        self.table_width = self.table_width.clamp(TABLE_WIDTH_MIN, TABLE_WIDTH_MAX);
        self.columns = normalize_columns(std::mem::take(&mut self.columns));
        self.timeline_height = self.timeline_height.clamp(TIMELINE_HEIGHT_MIN, PANE_HEIGHT_MAX);
        self.details.height = self.details.height.clamp(DETAILS_HEIGHT_MIN, PANE_HEIGHT_MAX);
        if self.assistant.model.chars().count() > MAX_ASSISTANT_MODEL_CHARS {
            self.assistant.model = AssistantPrefs::default().model;
        }
        self
    }

    /// État d'une colonne affichée (`None` : masquée).
    #[must_use]
    pub fn column(&self, column: Column) -> Option<&ColumnState> {
        self.columns.iter().find(|state| state.column == column)
    }

    /// La colonne est-elle affichée (hors « graphique seul ») ?
    #[must_use]
    pub fn shown(&self, column: Column) -> bool {
        self.column(column).is_some()
    }

    /// Colonnes visibles à l'écran, dans l'ordre d'affichage (« graphique
    /// seul » : ID et Nom).
    pub fn visible_columns(&self) -> impl Iterator<Item = &ColumnState> + '_ {
        let chart_only = self.chart_only;
        self.columns
            .iter()
            .filter(move |state| !chart_only || !state.column.hideable())
    }

    /// Largeur de la table à l'écran : celle choisie, ou la somme des
    /// colonnes N° et Nom en « graphique seul » (H-UI-64), séparateurs
    /// compris.
    #[must_use]
    pub fn effective_table_width(&self) -> u16 {
        if self.chart_only {
            let sum: u32 = self.visible_columns().map(|c| u32::from(c.width)).sum();
            u16::try_from(sum.saturating_add(2))
                .unwrap_or(u16::MAX)
                .clamp(TABLE_WIDTH_MIN, TABLE_WIDTH_MAX)
        } else {
            self.table_width
        }
    }

    /// Affiche ou masque `column` (J7, lot L6 ; cases du menu Colonnes) ;
    /// l'ID et le Nom restent visibles (H-UI-12). Affichée, elle prend la
    /// place que lui donne l'ordre de [`Column::ALL`] (avant la première
    /// colonne affichée qui la suit dans cet ordre, sinon en fin), largeur
    /// par défaut. Rend vrai si l'affichage a changé.
    pub fn toggle_column(&mut self, column: Column) -> bool {
        if !column.hideable() {
            return false;
        }
        if let Some(at) = self.columns.iter().position(|state| state.column == column) {
            return self.hide_column(at);
        }
        let rank = |c: Column| c.ordinal().unwrap_or(usize::MAX);
        let at = self
            .columns
            .iter()
            .position(|state| rank(state.column) > rank(column))
            .unwrap_or(self.columns.len());
        self.insert_column(at, column)
    }

    /// Insère `column` au rang `at` (borné : jamais avant l'ID ; au-delà de
    /// la fin : en fin), largeur par défaut ; déjà affichée : déplacée à ce
    /// rang avec ses réglages (H-UI-114). Refusée (faux) si
    /// [`MAX_COLUMNS`] colonnes sont déjà affichées ou si c'est l'ID.
    pub fn insert_column(&mut self, at: usize, column: Column) -> bool {
        if column == Column::Id {
            return false;
        }
        if let Some(from) = self.columns.iter().position(|state| state.column == column) {
            // `at` compté avant le retrait : le rang d'arrivée tient compte
            // de la place libérée.
            let to = if at > from { at.saturating_sub(1) } else { at };
            return self.move_column(from, to);
        }
        if self.columns.len() >= MAX_COLUMNS {
            return false;
        }
        let at = at.max(1).min(self.columns.len());
        self.columns.insert(at, ColumnState::default_for(column));
        true
    }

    /// Masque la colonne de rang `at` (la retire) ; ID et Nom refusés.
    /// Rend vrai si elle a été retirée.
    pub fn hide_column(&mut self, at: usize) -> bool {
        if self.columns.get(at).is_none_or(|state| !state.column.hideable()) {
            return false;
        }
        self.columns.remove(at);
        true
    }

    /// Déplace la colonne de rang `from` au rang `to` (borné à la fin) ;
    /// l'ID reste en premier (déplacer l'ID ou devant lui : refusé, H-UI-116).
    /// Rend vrai si l'ordre a changé.
    pub fn move_column(&mut self, from: usize, to: usize) -> bool {
        let last = self.columns.len().saturating_sub(1);
        let to = to.min(last);
        if from == 0 || to == 0 || from == to || from > last {
            return false;
        }
        let state = self.columns.remove(from);
        self.columns.insert(to, state);
        true
    }

    /// Réglages de « Paramètres de champ » (H-UI-115) pour la colonne de
    /// rang `at` : champ (un champ déjà affiché ailleurs est refusé), titre
    /// (vide ⇒ défaut, [`MAX_COLUMN_TITLE`] caractères), alignements,
    /// largeur bornée. L'ID reste l'ID. Rend vrai si quelque chose a changé.
    pub fn set_column_settings(&mut self, at: usize, settings: ColumnState) -> bool {
        let Some(current) = self.columns.get(at) else {
            return false;
        };
        let column_ok = settings.column == current.column
            || (current.column.hideable()
                && settings.column != Column::Id
                && !self.columns.iter().any(|state| state.column == settings.column));
        if !column_ok {
            return false;
        }
        let settings = clean_column(settings);
        match self.columns.get_mut(at) {
            Some(state) if *state != settings => {
                *state = settings;
                true
            }
            _ => false,
        }
    }

    /// Largeur de `column`, bornée à `[Column::MIN_WIDTH, Column::MAX_WIDTH]`
    /// comme par [`UiState::normalized`] (J7, lot L6). Rend vrai si elle a
    /// changé.
    pub fn set_column_width(&mut self, column: Column, width: u16) -> bool {
        let width = width.clamp(Column::MIN_WIDTH, Column::MAX_WIDTH);
        match self.columns.iter_mut().find(|state| state.column == column) {
            Some(state) if state.width != width => {
                state.width = width;
                true
            }
            _ => false,
        }
    }
}

/// Réglages d'une colonne bornés : largeur dans `[Column::MIN_WIDTH,
/// Column::MAX_WIDTH]`, titre sans blancs aux bords, vide ⇒ `None`, coupé à
/// [`MAX_COLUMN_TITLE`] caractères.
fn clean_column(mut state: ColumnState) -> ColumnState {
    state.width = state.width.clamp(Column::MIN_WIDTH, Column::MAX_WIDTH);
    state.title = state
        .title
        .map(|t| {
            let cut: String = t.trim().chars().take(MAX_COLUMN_TITLE).collect();
            cut.trim_end().to_owned()
        })
        .filter(|t| !t.is_empty());
    state
}

/// Colonnes affichées normalisées (version 2, P63) : première occurrence
/// d'une colonne gardée, réglages bornés ([`clean_column`]), ID en premier
/// (ajouté s'il manque), Nom présent (ajouté après l'ID s'il manque),
/// [`MAX_COLUMNS`] au plus (les dernières, hors ID et Nom, partent).
#[must_use]
pub fn normalize_columns(columns: Vec<ColumnState>) -> Vec<ColumnState> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out: Vec<ColumnState> = columns
        .into_iter()
        .filter(|state| seen.insert(state.column))
        .map(clean_column)
        .collect();
    match out.iter().position(|state| state.column == Column::Id) {
        Some(0) => {}
        Some(at) => {
            let id = out.remove(at);
            out.insert(0, id);
        }
        None => out.insert(0, ColumnState::default_for(Column::Id)),
    }
    if !out.iter().any(|state| state.column == Column::Name) {
        out.insert(1, ColumnState::default_for(Column::Name));
    }
    while out.len() > MAX_COLUMNS {
        match out.iter().rposition(|state| state.column.hideable()) {
            Some(at) => drop(out.remove(at)),
            None => break,
        }
    }
    out
}

/// Colonne au format v1 (une entrée par colonne, affichée ou non), relue
/// pour la migration vers la version 2.
#[derive(Clone, Copy, Debug, Deserialize)]
struct ColumnStateV1 {
    /// Colonne.
    column: Column,
    /// Affichée.
    visible: bool,
    /// Largeur.
    width: u16,
}

/// Lecteur champ par champ d'un objet JSON d'état : chaque champ illisible
/// prend sa valeur par défaut et son nom est relevé.
struct FieldReader<'a> {
    /// Objet JSON lu.
    object: &'a serde_json::Map<String, serde_json::Value>,
    /// Noms des champs illisibles, dans l'ordre de lecture.
    bad: Vec<&'static str>,
}

impl FieldReader<'_> {
    /// Champ `name` : absent → `default` ; illisible → `default`, relevé.
    fn field<T: serde::de::DeserializeOwned>(&mut self, name: &'static str, default: T) -> T {
        match self.object.get(name) {
            None => default,
            Some(raw) => T::deserialize(raw).unwrap_or_else(|error| {
                log::warn!("état d'interface : champ « {name} » ignoré : {error}");
                self.bad.push(name);
                default
            }),
        }
    }

    /// Champ liste `name` (J8, modèles personnels) : chaque entrée est lue
    /// seule ; une entrée illisible est écartée et le champ relevé une
    /// fois. Pas un tableau → `default`, relevé.
    fn list<T: serde::de::DeserializeOwned>(&mut self, name: &'static str, default: Vec<T>) -> Vec<T> {
        let Some(raw) = self.object.get(name) else {
            return default;
        };
        let Some(entries) = raw.as_array() else {
            log::warn!("état d'interface : champ « {name} » ignoré : tableau attendu");
            self.bad.push(name);
            return default;
        };
        let mut out = Vec::new();
        let mut skipped = false;
        for entry in entries {
            match T::deserialize(entry) {
                Ok(v) => out.push(v),
                Err(error) => {
                    log::warn!("état d'interface : entrée de « {name} » ignorée : {error}");
                    skipped = true;
                }
            }
        }
        if skipped {
            self.bad.push(name);
        }
        out
    }

    /// Champ `columns` au format v1 : un tableau dont chaque entrée est lue
    /// seule ; une entrée illisible (colonne inconnue d'une version future,
    /// largeur hors type…) est écartée et le champ relevé une fois. Migré
    /// comme la normalisation v1 l'affichait : pour chaque colonne de
    /// [`Column::ALL`], sa première entrée (affichée ou non ; ID et Nom
    /// toujours), à défaut son affichage par défaut ; largeurs gardées. Pas
    /// un tableau → `default`, relevé.
    fn columns_v1(&mut self, default: Vec<ColumnState>) -> Vec<ColumnState> {
        const NAME: &str = "columns";
        let Some(raw) = self.object.get(NAME) else {
            return default;
        };
        let Some(entries) = raw.as_array() else {
            log::warn!("état d'interface : champ « {NAME} » ignoré : tableau attendu");
            self.bad.push(NAME);
            return default;
        };
        let mut read = Vec::with_capacity(entries.len().min(Column::COUNT));
        let mut skipped = false;
        for entry in entries {
            match ColumnStateV1::deserialize(entry) {
                Ok(state) => read.push(state),
                Err(error) => {
                    log::warn!("état d'interface : colonne ignorée : {error}");
                    skipped = true;
                }
            }
        }
        if skipped {
            self.bad.push(NAME);
        }
        Column::ALL
            .into_iter()
            .filter_map(|column| match read.iter().find(|state| state.column == column) {
                Some(state) if state.visible || !column.hideable() => Some(ColumnState {
                    width: state.width,
                    ..ColumnState::default_for(column)
                }),
                Some(_) => None,
                None => column.default_visible().then(|| ColumnState::default_for(column)),
            })
            .collect()
    }
}

/// Refus d'enregistrement d'un modèle personnel (J8, H-UI-61).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum UserTemplateError {
    /// Nom vide ou de plus de [`MAX_USER_TEMPLATE_NAME`] caractères.
    #[error("nom de modèle de 1 à {MAX_USER_TEMPLATE_NAME} caractères attendu")]
    BadName,
    /// [`MAX_USER_TEMPLATES`] modèles déjà enregistrés (un nom existant
    /// reste remplaçable).
    #[error("{MAX_USER_TEMPLATES} modèles personnels au plus")]
    Full,
}

/// Style du document et modèles (J8, lot L6 ; ADR-0016 D7).
impl UiState {
    /// Style effectif d'un document (H-UI-43) : son style s'il en a un
    /// lisible, sinon le modèle par défaut des préférences.
    #[must_use]
    pub fn effective_style(
        &self,
        project: &plannercraft_core::Project,
    ) -> plannercraft_render::style::PlanStyle {
        plannercraft_automation::command::style::effective_style(project, self.default_template)
    }

    /// Options d'écriture MSPDI selon les préférences : style du document
    /// écrit (`<?plannercraft-style?>`) seulement si `write_style_to_file`.
    #[must_use]
    pub fn write_options(&self) -> plannercraft_mspdi::write::WriteOptions {
        plannercraft_mspdi::write::WriteOptions {
            style: self.write_style_to_file,
            ..plannercraft_mspdi::write::WriteOptions::default()
        }
    }

    /// Enregistre un modèle personnel (« Enregistrer comme modèle »,
    /// H-UI-61) : nom rogné, un modèle de même nom est remplacé sur place,
    /// sinon ajouté en fin de liste.
    ///
    /// # Errors
    ///
    /// [`UserTemplateError::BadName`], [`UserTemplateError::Full`].
    pub fn save_user_template(&mut self, name: &str, style: DocStyle) -> Result<(), UserTemplateError> {
        let name = name.trim();
        let n = name.chars().count();
        if n == 0 || n > MAX_USER_TEMPLATE_NAME {
            return Err(UserTemplateError::BadName);
        }
        if let Some(t) = self.user_templates.iter_mut().find(|t| t.name == name) {
            t.style = style;
            return Ok(());
        }
        if self.user_templates.len() >= MAX_USER_TEMPLATES {
            return Err(UserTemplateError::Full);
        }
        self.user_templates.push(NamedStyle {
            name: name.to_owned(),
            style,
        });
        Ok(())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use plannercraft_automation::filter::FilterPreset;
    use plannercraft_render::style::TemplateId;

    use super::{
        FilterChoice, GanttStyle, LanguagePref, MAX_OUTLINE_LEVEL, MAX_RECENT_FILES, MAX_USER_TEMPLATE_NAME,
        MAX_USER_TEMPLATES, NamedStyle, RecentFile, StateLoadError, TABLE_WIDTH_DEFAULT, TABLE_WIDTH_MAX,
        TABLE_WIDTH_MIN, TaskViewPrefs, TaskViewState, ThemePref, UI_STATE_VERSION, UiState, ViewKind,
    };
    use crate::column::{Column, ColumnState};
    use crate::timescale::TimeScaleZoom;

    /// JSON de référence de `UiState::default()` (format v2).
    const REFERENCE_V2: &str = include_str!("../tests/fixtures/ui_state_v2.json");

    /// État v1 par défaut écrit par `main` avant la seconde passe UX.
    const REFERENCE_V1: &str = include_str!("../tests/fixtures/ui_state_v1.json");

    /// Renommage du 2026-11-03 : l'état enregistré sous la clé de `planrs`
    /// est repris (desktop et web) tant que la nouvelle clé est absente ;
    /// la nouvelle l'emporte dès qu'elle existe.
    #[test]
    fn ui_state_legacy_key_adopted() {
        use std::collections::BTreeMap;
        let mut store = BTreeMap::new();
        let get = |store: &BTreeMap<&str, String>, key: &str| store.get(key).cloned();
        assert_eq!(super::stored_state(|k| get(&store, k)), None);
        store.insert(super::LEGACY_STATE_KEY, REFERENCE_V1.to_owned());
        assert_eq!(super::LEGACY_STATE_KEY, "planrs.ui_state");
        let adopted = super::stored_state(|k| get(&store, k)).unwrap();
        let (state, warning) = UiState::from_json_lenient(&adopted);
        assert!(warning.is_none(), "{warning:?}");
        assert_eq!(state.version, UI_STATE_VERSION);
        store.insert(super::STATE_KEY, "nouveau".to_owned());
        assert_eq!(
            super::stored_state(|k| get(&store, k)).as_deref(),
            Some("nouveau")
        );
    }

    #[test]
    fn ui_state_default_json_reference() {
        let json = UiState::default().to_json().unwrap();
        assert_eq!(
            json,
            REFERENCE_V2.trim_end(),
            "le format JSON de UiState a changé : décider s'il faut incrémenter UI_STATE_VERSION"
        );
        let (state, warning) = UiState::from_json_lenient(REFERENCE_V2);
        assert!(warning.is_none(), "{warning:?}");
        assert_eq!(state, UiState::default());
    }

    /// État J6 (sans `view`, `gantt_style` ni `recent`, 8 colonnes) : lu
    /// sans avertissement, champs J7 à leur défaut, colonnes J7 ajoutées
    /// masquées (contrat J7 L0 : la version reste 1).
    #[test]
    fn ui_state_v1_without_new_fields_reads() {
        const J6: &str = include_str!("../tests/fixtures/ui_state_v1_j6.json");
        assert!(!J6.contains("gantt_style") && !J6.contains("percent_complete"));
        let (state, warning) = UiState::from_json_lenient(J6);
        assert!(warning.is_none(), "{warning:?}");
        assert_eq!(state, UiState::default());
        assert_eq!(state.view, ViewKind::Gantt);
        assert_eq!(state.gantt_style, GanttStyle::Standard);
        assert_eq!(state.recent.len(), 0);
        // Colonnes J7 et colonne « Mode » (J8, lot L11) masquées : absentes
        // de la liste v2 ; Indicateurs ajoutée une fois (finitions P1).
        assert_eq!(state.columns.len(), 7);
        assert!(!state.shown(Column::Mode) && !state.shown(Column::TotalSlack));
    }

    /// Champs J7 relus ; fichiers récents coupés à 10 (H-UI-28).
    #[test]
    fn ui_state_j7_fields_round_trip() {
        let recent: Vec<RecentFile> = (0..12)
            .map(|i| RecentFile {
                path: format!("/p/{i}.xml"),
                name: format!("{i}.xml"),
            })
            .collect();
        let state = UiState {
            view: ViewKind::ResourceUsage,
            gantt_style: GanttStyle::Tracking,
            recent,
            ..UiState::default()
        };
        let (read, warning) = UiState::from_json_lenient(&state.to_json().unwrap());
        assert!(warning.is_none(), "{warning:?}");
        assert_eq!(read.view, ViewKind::ResourceUsage);
        assert_eq!(read.gantt_style, GanttStyle::Tracking);
        assert_eq!(read.recent.len(), MAX_RECENT_FILES);
        assert_eq!(read.recent[0].name, "0.xml");
        let (read, warning) = UiState::from_json_lenient(r#"{"view":"calendar"}"#);
        assert_eq!(read.view, ViewKind::Gantt);
        assert!(matches!(warning, Some(StateLoadError::Fields(f)) if f == ["view"]));
    }

    /// État J7 (sans les champs J8) : lu sans avertissement, champs J8 à
    /// leur défaut (contrat J8 L0 : la version reste 1).
    #[test]
    fn ui_state_v1_j7_reads_without_new_fields() {
        const J7: &str = include_str!("../tests/fixtures/ui_state_v1_j7.json");
        assert!(!J7.contains("task_view") && !J7.contains("default_template"));
        let (state, warning) = UiState::from_json_lenient(J7);
        assert!(warning.is_none(), "{warning:?}");
        assert_eq!(state, UiState::default());
        // J8 lot L15 : Sobre devient le modèle par défaut (H-UI-43).
        assert_eq!(state.default_template, TemplateId::Sobre);
        assert!(state.task_view.highlight_critical && state.write_style_to_file);
        assert!(!state.chart_only);
    }

    /// ADR-0017 (L0) : un état J8 (sans `ribbon` ni `timeline_visible`) se
    /// lit sans avertissement, champs nouveaux à leur défaut ; champs
    /// relus à l'identique ; champ illisible ⇒ son défaut seul.
    #[test]
    fn ui_state_v1_fixture() {
        use crate::ribbon::{RibbonPrefs, RibbonTab};

        const J8: &str = include_str!("../tests/fixtures/ui_state_v1_j8.json");
        assert!(!J8.contains("ribbon") && !J8.contains("timeline_visible"));
        let (state, warning) = UiState::from_json_lenient(J8);
        assert!(warning.is_none(), "{warning:?}");
        assert_eq!(state, UiState::default());
        assert_eq!(state.ribbon, RibbonPrefs::default());
        assert_eq!(state.ribbon.tab, RibbonTab::Task);
        assert!(state.timeline_visible && !state.ribbon.collapsed);
        let state = UiState {
            ribbon: RibbonPrefs {
                tab: RibbonTab::Format,
                collapsed: true,
            },
            timeline_visible: false,
            ..UiState::default()
        };
        let (read, warning) = UiState::from_json_lenient(&state.to_json().unwrap());
        assert!(warning.is_none(), "{warning:?}");
        assert_eq!(read, state);
        let (read, warning) =
            UiState::from_json_lenient(r#"{"ribbon":{"tab":"calendrier"},"timeline_visible":false}"#);
        assert_eq!(read.ribbon, RibbonPrefs::default());
        assert!(!read.timeline_visible);
        assert!(matches!(warning, Some(StateLoadError::Fields(f)) if f == ["ribbon"]));
    }

    /// Champs J8 relus à l'identique ; bornes appliquées ; une entrée de
    /// modèle personnel illisible est écartée seule.
    #[test]
    fn ui_state_v1_roundtrip() {
        let sobre = plannercraft_render::style::parse_doc_style(r#"{"base":"sobre","v":1}"#).unwrap();
        let state = UiState {
            task_view: TaskViewPrefs {
                outline_level: Some(2),
                highlight_critical: false,
            },
            default_template: TemplateId::Sobre,
            user_templates: vec![NamedStyle {
                name: "Agence".to_owned(),
                style: sobre.clone(),
            }],
            write_style_to_file: false,
            ..UiState::default()
        };
        let (read, warning) = UiState::from_json_lenient(&state.to_json().unwrap());
        assert!(warning.is_none(), "{warning:?}");
        assert_eq!(read, state);

        let json = r#"{"task_view":{"outline_level":40,"highlight_critical":true},
            "user_templates":[{"name":"A","style":{"v":1,"base":"sobre"}},{"name":"B","style":{"v":1,"base":"inconnu"}}]}"#;
        let (read, warning) = UiState::from_json_lenient(json);
        assert_eq!(read.task_view.outline_level, Some(MAX_OUTLINE_LEVEL));
        assert_eq!(read.user_templates.len(), 1);
        assert_eq!(read.user_templates[0].style, sobre);
        assert!(matches!(warning, Some(StateLoadError::Fields(f)) if f == ["user_templates"]));
        let many: Vec<NamedStyle> = (0..40)
            .map(|i| NamedStyle {
                name: format!("{i}{}", "x".repeat(100)),
                style: sobre.clone(),
            })
            .collect();
        let state = UiState {
            user_templates: many,
            ..UiState::default()
        }
        .normalized();
        assert_eq!(state.user_templates.len(), MAX_USER_TEMPLATES);
        assert!(
            state
                .user_templates
                .iter()
                .all(|t| t.name.chars().count() == MAX_USER_TEMPLATE_NAME)
        );
        assert_eq!(
            TaskViewState::default().filter,
            FilterChoice::Preset(FilterPreset::All)
        );
    }

    /// H-UI-64 : « graphique seul » réduit la table au N° et au Nom sans
    /// toucher aux colonnes choisies ; relu à l'identique.
    #[test]
    fn h_ui_64_chart_only_keeps_columns() {
        let mut state = UiState::default();
        let before: Vec<ColumnState> = state.columns.clone();
        state.chart_only = true;
        let shown: Vec<Column> = state.visible_columns().map(|c| c.column).collect();
        assert_eq!(shown, [Column::Id, Column::Name]);
        assert_eq!(state.effective_table_width(), 40 + 220 + 2);
        assert_eq!(state.columns, before);
        let (read, warning) = UiState::from_json_lenient(&state.to_json().unwrap());
        assert!(warning.is_none(), "{warning:?}");
        assert!(read.chart_only);
        state.chart_only = false;
        assert_eq!(state.visible_columns().count(), 7);
        assert_eq!(state.effective_table_width(), TABLE_WIDTH_DEFAULT);
    }

    #[test]
    fn ui_state_default_values() {
        let state = UiState::default();
        assert_eq!(state.version, UI_STATE_VERSION);
        assert_eq!(state.language, LanguagePref::System);
        assert_eq!(state.theme, ThemePref::System);
        assert_eq!(state.zoom, TimeScaleZoom::Week);
        assert!(!state.diagnostics_open);
        let visible_width: u16 = state.visible_columns().map(|c| c.width).sum();
        assert_eq!(visible_width, TABLE_WIDTH_DEFAULT);
        assert_eq!(state.table_width, TABLE_WIDTH_DEFAULT);
        assert_eq!(state.clone().normalized(), state);
    }

    #[test]
    fn ui_state_garbage_is_default() {
        for garbage in ["", "{", "not json"] {
            let (state, warning) = UiState::from_json_lenient(garbage);
            assert_eq!(state, UiState::default(), "{garbage:?}");
            assert!(
                matches!(warning, Some(StateLoadError::Json(_))),
                "{garbage:?} : {warning:?}"
            );
        }
        for not_object in ["null", "42", "[]", "\"x\""] {
            let (state, warning) = UiState::from_json_lenient(not_object);
            assert_eq!(state, UiState::default(), "{not_object:?}");
            assert!(
                matches!(warning, Some(StateLoadError::NotObject)),
                "{not_object:?} : {warning:?}"
            );
        }
        let (state, warning) = UiState::from_json_lenient("{\"table_width\": -5}");
        assert_eq!(state, UiState::default());
        assert!(
            matches!(&warning, Some(StateLoadError::Fields(fields)) if fields == &["table_width"]),
            "{warning:?}"
        );
    }

    /// D-fix-11 : un champ illisible prend sa valeur par défaut, les autres
    /// sont conservés ; une colonne inconnue (version future) est écartée
    /// sans perdre les autres colonnes.
    #[test]
    fn ui_state_lenient_field_by_field() {
        // Langue inconnue : seul ce champ revient au défaut, le thème reste.
        let (state, warning) = UiState::from_json_lenient(r#"{"language":"de","theme":"dark"}"#);
        assert_eq!(state.theme, ThemePref::Dark);
        assert_eq!(state.language, LanguagePref::System);
        assert!(
            matches!(&warning, Some(StateLoadError::Fields(f)) if f == &["language"]),
            "{warning:?}"
        );
        // Plusieurs champs illisibles, d'autres valides.
        let json = r#"{"version":"9","language":"en","theme":42,"zoom":"century",
            "table_width":"wide","diagnostics_open":true}"#;
        let (state, warning) = UiState::from_json_lenient(json);
        assert_eq!(state.language, LanguagePref::En);
        assert_eq!(state.theme, ThemePref::System);
        assert_eq!(state.zoom, TimeScaleZoom::Week);
        assert_eq!(state.table_width, TABLE_WIDTH_DEFAULT);
        assert!(state.diagnostics_open);
        assert_eq!(state.version, UI_STATE_VERSION);
        match &warning {
            Some(StateLoadError::Fields(fields)) => {
                assert_eq!(fields, &["version", "theme", "zoom", "table_width"]);
            }
            other => panic!("attendu Fields, obtenu {other:?}"),
        }
        // Texte neutre (inséré dans le message traduit `state-load-warning`).
        assert_eq!(
            warning.map(|w| w.to_string()).as_deref(),
            Some("version, theme, zoom, table_width")
        );
        // Colonne d'une version future : écartée, les autres colonnes et le
        // thème sont gardés.
        // Seconde passe UX : état v1 (sans version) migré ; Durée masquée
        // retirée, Marge totale illisible ⇒ son affichage par défaut.
        let json = r#"{"theme":"light","columns":[
            {"column":"future_col","visible":true,"width":50},
            {"column":"duration","visible":false,"width":70},
            {"column":"total_slack","visible":true,"width":"x"}]}"#;
        let (state, warning) = UiState::from_json_lenient(json);
        assert_eq!(state.theme, ThemePref::Light);
        assert_eq!(state.column(Column::Duration), None);
        assert_eq!(state.column(Column::TotalSlack), None);
        let order: Vec<Column> = state.columns.iter().map(|c| c.column).collect();
        assert_eq!(
            order,
            [
                Column::Id,
                Column::Indicators,
                Column::Name,
                Column::Start,
                Column::Finish,
                Column::Predecessors
            ]
        );
        assert!(
            matches!(&warning, Some(StateLoadError::Fields(f)) if f == &["columns"]),
            "{warning:?}"
        );
        // Même chose en v2 : entrée illisible écartée, les autres gardées.
        let json = r#"{"version":2,"columns":[
            {"column":"future_col","width":50},
            {"column":"duration","width":70},
            {"column":"free_slack","width":"x"}]}"#;
        let (state, warning) = UiState::from_json_lenient(json);
        let order: Vec<Column> = state.columns.iter().map(|c| c.column).collect();
        assert_eq!(
            order,
            [Column::Id, Column::Indicators, Column::Name, Column::Duration]
        );
        assert_eq!(state.column(Column::Duration).unwrap().width, 70);
        assert!(matches!(&warning, Some(StateLoadError::Fields(f)) if f == &["columns"]));
        // `columns` qui n'est pas un tableau : colonnes par défaut.
        let (state, warning) = UiState::from_json_lenient(r#"{"columns":{"id":1},"zoom":"day"}"#);
        assert_eq!(state.columns, UiState::default().columns);
        assert_eq!(state.zoom, TimeScaleZoom::Day);
        assert!(matches!(warning, Some(StateLoadError::Fields(_))));
    }

    /// Les textes d'avertissement ne contiennent aucun mot d'une langue
    /// (insérés tels quels dans un message traduit).
    #[test]
    fn ui_state_warning_text_is_language_neutral() {
        let (_, json) = UiState::from_json_lenient("{");
        assert!(json.unwrap().to_string().starts_with("JSON "));
        let (_, too_new) = UiState::from_json_lenient(r#"{"version":3}"#);
        assert_eq!(too_new.unwrap().to_string(), "v3 > v2");
        assert_eq!(StateLoadError::NotObject.to_string(), "JSON");
    }

    #[test]
    fn ui_state_unknown_field_ignored() {
        let json = r#"{"version":1,"theme":"dark","future_field":{"a":[1,2]},"zoom":"month"}"#;
        let (state, warning) = UiState::from_json_lenient(json);
        assert!(warning.is_none(), "{warning:?}");
        assert_eq!(state.theme, ThemePref::Dark);
        assert_eq!(state.zoom, TimeScaleZoom::Month);
        // Champs absents : valeurs par défaut.
        assert_eq!(state.language, LanguagePref::System);
        assert_eq!(state.columns, UiState::default().columns);
        // Objet vide : état par défaut, sans avertissement.
        let (state, warning) = UiState::from_json_lenient("{}");
        assert!(warning.is_none());
        assert_eq!(state, UiState::default());
    }

    #[test]
    fn ui_state_too_new_warns() {
        let json = r#"{"version":3,"theme":"dark","zoom":{"px_per_day":3}}"#;
        let (state, warning) = UiState::from_json_lenient(json);
        assert_eq!(state, UiState::default());
        match warning {
            Some(StateLoadError::TooNew { found, supported }) => {
                assert_eq!(found, 3);
                assert_eq!(supported, UI_STATE_VERSION);
            }
            other => panic!("attendu TooNew, obtenu {other:?}"),
        }
        let (_, warning) = UiState::from_json_lenient(r#"{"version":99999999999}"#);
        assert!(matches!(
            warning,
            Some(StateLoadError::TooNew { found: u32::MAX, .. })
        ));
    }

    /// Version 2 : première occurrence gardée, largeurs bornées, ID en
    /// premier, Nom ajouté s'il manque, titres bornés, hauteurs bornées.
    #[test]
    fn ui_state_normalized_bounds() {
        let state = UiState {
            version: 0,
            table_width: 1,
            timeline_height: 0,
            details: super::DetailsPrefs {
                visible: true,
                height: u16::MAX,
                form: super::DetailsForm::PredecessorsSuccessors,
            },
            columns: vec![
                ColumnState {
                    width: 0,
                    title: Some("   ".to_owned()),
                    ..ColumnState::default_for(Column::Critical)
                },
                ColumnState {
                    width: 9_999,
                    ..ColumnState::default_for(Column::Id)
                },
                ColumnState {
                    width: 300,
                    ..ColumnState::default_for(Column::Critical)
                },
                ColumnState {
                    title: Some("x".repeat(100)),
                    ..ColumnState::default_for(Column::Duration)
                },
            ],
            ..UiState::default()
        }
        .normalized();
        assert_eq!(state.version, UI_STATE_VERSION);
        assert_eq!(state.table_width, TABLE_WIDTH_MIN);
        assert_eq!(state.timeline_height, super::TIMELINE_HEIGHT_MIN);
        assert_eq!(state.details.height, super::PANE_HEIGHT_MAX);
        let order: Vec<Column> = state.columns.iter().map(|c| c.column).collect();
        assert_eq!(
            order,
            [Column::Id, Column::Name, Column::Critical, Column::Duration]
        );
        assert_eq!(state.column(Column::Id).unwrap().width, Column::MAX_WIDTH);
        assert_eq!(
            state.column(Column::Critical),
            Some(&ColumnState {
                width: Column::MIN_WIDTH,
                ..ColumnState::default_for(Column::Critical)
            })
        );
        assert_eq!(
            state
                .column(Column::Duration)
                .unwrap()
                .title
                .as_ref()
                .map(|t| t.chars().count()),
            Some(crate::column::MAX_COLUMN_TITLE)
        );
        assert_eq!(state.clone().normalized(), state);

        let wide = UiState {
            table_width: u16::MAX,
            ..UiState::default()
        }
        .normalized();
        assert_eq!(wide.table_width, TABLE_WIDTH_MAX);

        // La lecture tolérante normalise aussi ; liste vide : ID et Nom.
        let (read, warning) = UiState::from_json_lenient(r#"{"version":2,"table_width":0,"columns":[]}"#);
        assert!(warning.is_none());
        // Bornée, puis élargie de la colonne Indicateurs ajoutée une fois.
        assert_eq!(read.table_width, TABLE_WIDTH_MIN + 28);
        let order: Vec<Column> = read.columns.iter().map(|c| c.column).collect();
        assert_eq!(order, [Column::Id, Column::Indicators, Column::Name]);
    }

    /// P63 (état v1 relu) : l'état v1 par défaut de `main` et un état v1
    /// réglé (Marge totale et % achevé affichés, Durée masquée, largeurs
    /// changées) sont relus avec les mêmes colonnes visibles, dans le même
    /// ordre, largeurs gardées, sans avertissement.
    #[test]
    fn ui_state_v1_columns_migrate() {
        let (state, warning) = UiState::from_json_lenient(REFERENCE_V1);
        assert!(warning.is_none(), "{warning:?}");
        assert_eq!(state, UiState::default());
        let tuned = REFERENCE_V1
            .replace(
                r#"{"column":"duration","visible":true,"width":80}"#,
                r#"{"column":"duration","visible":false,"width":80}"#,
            )
            .replace(
                r#"{"column":"total_slack","visible":false,"width":80}"#,
                r#"{"column":"total_slack","visible":true,"width":91}"#,
            )
            .replace(
                r#"{"column":"percent_complete","visible":false,"width":60}"#,
                r#"{"column":"percent_complete","visible":true,"width":66}"#,
            )
            .replace(
                r#"{"column":"name","visible":true,"width":220}"#,
                r#"{"column":"name","visible":false,"width":300}"#,
            );
        assert_ne!(tuned, REFERENCE_V1);
        let (state, warning) = UiState::from_json_lenient(&tuned);
        assert!(warning.is_none(), "{warning:?}");
        let shown: Vec<(Column, u16)> = state.columns.iter().map(|c| (c.column, c.width)).collect();
        assert_eq!(
            shown,
            [
                (Column::Id, 40),
                (Column::Indicators, 28),
                (Column::Name, 300),
                (Column::Start, 96),
                (Column::Finish, 96),
                (Column::Predecessors, 110),
                (Column::TotalSlack, 91),
                (Column::PercentComplete, 66),
            ]
        );
        assert_eq!(state.version, UI_STATE_VERSION);
        // Réécrit en v2, relu à l'identique.
        let (again, warning) = UiState::from_json_lenient(&state.to_json().unwrap());
        assert!(warning.is_none(), "{warning:?}");
        assert_eq!(again, state);
    }

    #[test]
    fn ui_state_roundtrip() {
        let mut state = UiState {
            language: LanguagePref::En,
            theme: ThemePref::Light,
            zoom: TimeScaleZoom::Quarter,
            table_width: 800,
            diagnostics_open: true,
            ..UiState::default()
        };
        assert!(state.insert_column(2, Column::TotalSlack));
        assert!(state.set_column_width(Column::TotalSlack, 123));
        assert!(
            state.insert_column(
                3,
                Column::Custom(
                    plannercraft_core::model::CustomFieldId::new(
                        plannercraft_core::model::CustomKind::Text,
                        7
                    )
                    .unwrap()
                )
            )
        );
        let name_at = state
            .columns
            .iter()
            .position(|c| c.column == Column::Name)
            .unwrap();
        assert!(state.set_column_settings(
            name_at,
            ColumnState {
                title: Some("Tâche".to_owned()),
                align: Some(crate::column::CellAlign::Center),
                ..ColumnState::default_for(Column::Name)
            }
        ));
        state.timeline_height = 150;
        state.details.visible = true;
        state.details.form = super::DetailsForm::ResourcesSuccessors;
        let json = state.to_json().unwrap();
        let (read, warning) = UiState::from_json_lenient(&json);
        assert!(warning.is_none(), "{warning:?}");
        assert_eq!(read, state);
        assert_eq!(read.to_json().unwrap(), json);
    }

    fn project() -> plannercraft_core::Project {
        let now: plannercraft_core::Moment = "2026-10-19T08:00".parse().unwrap();
        plannercraft_core::Project::new(plannercraft_core::ProjectInit {
            name: "P".to_owned(),
            anchor: plannercraft_core::ScheduleAnchor::FromStart(now),
            current_date: now,
            calendar_names: plannercraft_core::BuiltinCalendarNames::default(),
        })
    }

    /// H-UI-43 : document sans style ⇒ modèle par défaut des préférences
    /// (Sobre depuis le lot L15) ; document stylé ⇒ son style.
    #[test]
    fn h_ui_43_default_template_applied() {
        use plannercraft_render::style::template;
        let mut state = UiState::default();
        let mut p = project();
        assert_eq!(state.effective_style(&p), template(TemplateId::Sobre));
        state.default_template = TemplateId::Classique;
        assert_eq!(state.effective_style(&p), template(TemplateId::Classique));
        state.default_template = TemplateId::Sobre;
        assert_eq!(state.effective_style(&p), template(TemplateId::Sobre));
        plannercraft_mspdi::style_pi::set(&mut p.extras, Some(r#"{"base":"chantier","v":1}"#.to_owned()));
        assert_eq!(state.effective_style(&p), template(TemplateId::Chantier));
        // Style illisible : modèle par défaut.
        plannercraft_mspdi::style_pi::set(&mut p.extras, Some("{".to_owned()));
        assert_eq!(state.effective_style(&p), template(TemplateId::Sobre));
    }

    #[test]
    fn write_options_follow_preference() {
        let mut state = UiState::default();
        assert!(state.write_options().style);
        state.write_style_to_file = false;
        assert!(!state.write_options().style);
    }

    #[test]
    fn save_user_template_bounds() {
        use super::UserTemplateError;
        use plannercraft_render::style::DocStyle;
        let doc = |base| DocStyle {
            v: 1,
            base,
            over: plannercraft_render::style::StylePatch::default(),
        };
        let mut state = UiState::default();
        state
            .save_user_template("  Client  ", doc(TemplateId::Sobre))
            .unwrap();
        state
            .save_user_template("Client", doc(TemplateId::Chantier))
            .unwrap();
        assert_eq!(state.user_templates.len(), 1);
        assert_eq!(state.user_templates[0].name, "Client");
        assert_eq!(state.user_templates[0].style.base, TemplateId::Chantier);
        assert_eq!(
            state.save_user_template(" ", doc(TemplateId::Sobre)),
            Err(UserTemplateError::BadName)
        );
        let long = "x".repeat(MAX_USER_TEMPLATE_NAME + 1);
        assert_eq!(
            state.save_user_template(&long, doc(TemplateId::Sobre)),
            Err(UserTemplateError::BadName)
        );
        for i in 1..MAX_USER_TEMPLATES {
            state
                .save_user_template(&format!("M{i}"), doc(TemplateId::Sobre))
                .unwrap();
        }
        assert_eq!(
            state.save_user_template("Un de plus", doc(TemplateId::Sobre)),
            Err(UserTemplateError::Full)
        );
        // Un nom existant reste remplaçable.
        state
            .save_user_template("M1", doc(TemplateId::Presentation))
            .unwrap();
        assert_eq!(state.user_templates.len(), MAX_USER_TEMPLATES);
    }

    /// Opération d'état des colonnes (P63).
    #[derive(Clone, Debug)]
    enum ColOp {
        Insert(usize, Column),
        Hide(usize),
        Move(usize, usize),
        Settings(usize, Column, u16, Option<String>),
        Width(Column, u16),
        Toggle(Column),
    }

    fn any_column() -> impl proptest::strategy::Strategy<Value = Column> {
        use plannercraft_core::model::{CustomFieldId, CustomKind};
        use proptest::prelude::*;
        prop_oneof![
            proptest::sample::select(Column::ALL.to_vec()),
            proptest::sample::select(Column::FIELDS.to_vec()),
            (0u8..4, 1u8..=30).prop_map(|(k, i)| {
                let kind = CustomKind::ALL[usize::from(k)];
                let i = i.min(CustomFieldId::max(kind));
                Column::Custom(CustomFieldId::new(kind, i).unwrap())
            }),
        ]
    }

    fn any_op() -> impl proptest::strategy::Strategy<Value = ColOp> {
        use proptest::prelude::*;
        prop_oneof![
            4 => (0usize..80, any_column()).prop_map(|(at, c)| ColOp::Insert(at, c)),
            2 => (0usize..80).prop_map(ColOp::Hide),
            2 => (0usize..80, 0usize..80).prop_map(|(a, b)| ColOp::Move(a, b)),
            1 => (0usize..80, any_column(), any::<u16>(), proptest::option::of("[ a-zé]{0,80}"))
                .prop_map(|(at, c, w, t)| ColOp::Settings(at, c, w, t)),
            1 => (any_column(), any::<u16>()).prop_map(|(c, w)| ColOp::Width(c, w)),
            1 => any_column().prop_map(ColOp::Toggle),
        ]
    }

    fn check_columns(state: &UiState) {
        let columns: Vec<Column> = state.columns.iter().map(|c| c.column).collect();
        let unique: std::collections::BTreeSet<Column> = columns.iter().copied().collect();
        assert_eq!(unique.len(), columns.len(), "doublon : {columns:?}");
        assert_eq!(columns.first(), Some(&Column::Id));
        assert!(columns.contains(&Column::Name));
        assert!(columns.len() <= crate::column::MAX_COLUMNS);
        for c in &state.columns {
            assert!((Column::MIN_WIDTH..=Column::MAX_WIDTH).contains(&c.width));
            assert!(
                c.title.as_ref().is_none_or(|t| {
                    !t.is_empty() && t.chars().count() <= crate::column::MAX_COLUMN_TITLE
                })
            );
        }
        assert_eq!(state.clone().normalized(), *state, "hors forme normale");
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(256))]

        /// P63 (contrat L0, opérations d'état seulement) : pour toute suite
        /// (≤ 64) d'insertions, retraits, déplacements, paramètres et
        /// largeurs de colonnes : sans doublon, ID en premier, Nom présent,
        /// ≤ 64 colonnes, largeurs et titres bornés, forme normale ;
        /// sérialisé puis relu ⇒ égal.
        #[test]
        fn p63_column_state_invariants(ops in proptest::collection::vec(any_op(), 0..64)) {
            let mut state = UiState::default();
            for op in ops {
                match op {
                    ColOp::Insert(at, c) => drop(state.insert_column(at, c)),
                    ColOp::Hide(at) => drop(state.hide_column(at)),
                    ColOp::Move(a, b) => drop(state.move_column(a, b)),
                    ColOp::Settings(at, c, w, t) => drop(state.set_column_settings(
                        at,
                        ColumnState { width: w, title: t, ..ColumnState::default_for(c) },
                    )),
                    ColOp::Width(c, w) => drop(state.set_column_width(c, w)),
                    ColOp::Toggle(c) => drop(state.toggle_column(c)),
                }
                check_columns(&state);
            }
            let (read, warning) = UiState::from_json_lenient(&state.to_json().unwrap());
            proptest::prop_assert!(warning.is_none(), "{warning:?}");
            proptest::prop_assert_eq!(read, state);
        }
    }
}
