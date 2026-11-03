//! Colonnes de la table des tâches et leurs métadonnées.
//!
//! [`Column::ALL`] liste les 14 colonnes d'avant la seconde passe UX, dans
//! leur ordre historique (textes pré-formatés du `RowModel`, cases du menu
//! Colonnes, ordre de migration de l'état v1). Seconde passe UX (ADR-0019
//! D7, contrat L0) : les champs de H-UI-117 ([`Column::FIELDS`], titres
//! compris) et les champs personnalisés ([`Column::Custom`]) ; l'ordre
//! d'affichage est celui de `UiState.columns` (version 2), libre. Les
//! valeurs des nouveaux champs sont servies par le lot L7 (vides et en
//! lecture seule jusque-là), les champs personnalisés par L9.

use plannercraft_core::model::custom::{CustomFieldId, CustomKind};
use serde::{Deserialize, Serialize};

use crate::i18n::TextId;
use crate::state::SortKey;

/// Colonne de la table des tâches.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Column {
    /// Identifiant de ligne (ID MS Project, position dans la liste).
    Id,
    /// Nom de la tâche, indenté selon le niveau hiérarchique (colonne du
    /// curseur par défaut).
    #[default]
    Name,
    /// Durée au format d'affichage MS Project (« 5 jours »).
    Duration,
    /// Début calculé.
    Start,
    /// Fin calculée.
    Finish,
    /// Prédécesseurs (« 3;5DD+2 jours »).
    Predecessors,
    /// Marge totale, en jours.
    TotalSlack,
    /// Tâche critique (oui / non).
    Critical,
    /// % achevé (J7, éditable au lot L6).
    PercentComplete,
    /// Noms des ressources affectées (J7, lecture seule ; N11).
    ResourceNames,
    /// Début de la ligne de base 0 (J7).
    BaselineStart,
    /// Fin de la ligne de base 0 (J7).
    BaselineFinish,
    /// Variation de fin par rapport à la ligne de base 0, en jours ouvrés
    /// signés (J7).
    FinishVariance,
    /// Mode de planification, « Manuelle » / « Automatique » (J8, lot L11,
    /// H-UI-36 ; lecture seule, masquée par défaut).
    Mode,
    // --- Seconde passe UX (ADR-0019 D7, contrat L0, H-UI-117) : valeurs
    // servies par le lot L7.
    /// Identifiant unique (UID) de la tâche, lecture seule.
    UniqueId,
    /// Code WBS, lecture seule.
    Wbs,
    /// Niveau hiérarchique, lecture seule.
    OutlineLevel,
    /// Tâche récapitulative (oui / non), lecture seule.
    Summary,
    /// Jalon (oui / non).
    Milestone,
    /// Notes, sur une ligne.
    Notes,
    /// Date de création, lecture seule.
    Created,
    /// Priorité (0 à 1 000).
    Priority,
    /// Tâche active (oui / non).
    Active,
    /// Successeurs (« 7;9DD »).
    Successors,
    /// Type de contrainte.
    ConstraintType,
    /// Date de contrainte.
    ConstraintDate,
    /// Échéance.
    Deadline,
    /// Type de tâche (unités, durée ou travail fixe).
    TaskType,
    /// Pilotée par l'effort (oui / non).
    EffortDriven,
    /// Travail.
    Work,
    /// Début au plus tôt (moteur), lecture seule.
    EarlyStart,
    /// Fin au plus tôt (moteur), lecture seule.
    EarlyFinish,
    /// Début au plus tard (moteur), lecture seule.
    LateStart,
    /// Fin au plus tard (moteur), lecture seule.
    LateFinish,
    /// Marge libre (moteur), lecture seule.
    FreeSlack,
    /// % travail achevé.
    PercentWorkComplete,
    /// Début réel.
    ActualStart,
    /// Fin réelle.
    ActualFinish,
    /// Durée réelle.
    ActualDuration,
    /// Durée restante.
    RemainingDuration,
    /// Travail réel, lecture seule.
    ActualWork,
    /// Travail restant, lecture seule.
    RemainingWork,
    /// Durée de la ligne de base 0, lecture seule.
    BaselineDuration,
    /// Travail de la ligne de base 0, lecture seule.
    BaselineWork,
    /// Début de la ligne de base 1, lecture seule.
    Baseline1Start,
    /// Fin de la ligne de base 1, lecture seule.
    Baseline1Finish,
    /// Durée de la ligne de base 1, lecture seule.
    Baseline1Duration,
    /// Travail de la ligne de base 1, lecture seule.
    Baseline1Work,
    /// Début de la ligne de base 2, lecture seule.
    Baseline2Start,
    /// Fin de la ligne de base 2, lecture seule.
    Baseline2Finish,
    /// Durée de la ligne de base 2, lecture seule.
    Baseline2Duration,
    /// Travail de la ligne de base 2, lecture seule.
    Baseline2Work,
    /// Début de la ligne de base 3, lecture seule.
    Baseline3Start,
    /// Fin de la ligne de base 3, lecture seule.
    Baseline3Finish,
    /// Durée de la ligne de base 3, lecture seule.
    Baseline3Duration,
    /// Travail de la ligne de base 3, lecture seule.
    Baseline3Work,
    /// Début de la ligne de base 4, lecture seule.
    Baseline4Start,
    /// Fin de la ligne de base 4, lecture seule.
    Baseline4Finish,
    /// Durée de la ligne de base 4, lecture seule.
    Baseline4Duration,
    /// Travail de la ligne de base 4, lecture seule.
    Baseline4Work,
    /// Début de la ligne de base 5, lecture seule.
    Baseline5Start,
    /// Fin de la ligne de base 5, lecture seule.
    Baseline5Finish,
    /// Durée de la ligne de base 5, lecture seule.
    Baseline5Duration,
    /// Travail de la ligne de base 5, lecture seule.
    Baseline5Work,
    /// Début de la ligne de base 6, lecture seule.
    Baseline6Start,
    /// Fin de la ligne de base 6, lecture seule.
    Baseline6Finish,
    /// Durée de la ligne de base 6, lecture seule.
    Baseline6Duration,
    /// Travail de la ligne de base 6, lecture seule.
    Baseline6Work,
    /// Début de la ligne de base 7, lecture seule.
    Baseline7Start,
    /// Fin de la ligne de base 7, lecture seule.
    Baseline7Finish,
    /// Durée de la ligne de base 7, lecture seule.
    Baseline7Duration,
    /// Travail de la ligne de base 7, lecture seule.
    Baseline7Work,
    /// Début de la ligne de base 8, lecture seule.
    Baseline8Start,
    /// Fin de la ligne de base 8, lecture seule.
    Baseline8Finish,
    /// Durée de la ligne de base 8, lecture seule.
    Baseline8Duration,
    /// Travail de la ligne de base 8, lecture seule.
    Baseline8Work,
    /// Début de la ligne de base 9, lecture seule.
    Baseline9Start,
    /// Fin de la ligne de base 9, lecture seule.
    Baseline9Finish,
    /// Durée de la ligne de base 9, lecture seule.
    Baseline9Duration,
    /// Travail de la ligne de base 9, lecture seule.
    Baseline9Work,
    /// Début de la ligne de base 10, lecture seule.
    Baseline10Start,
    /// Fin de la ligne de base 10, lecture seule.
    Baseline10Finish,
    /// Durée de la ligne de base 10, lecture seule.
    Baseline10Duration,
    /// Travail de la ligne de base 10, lecture seule.
    Baseline10Work,
    /// Variation de début (ligne de base 0), lecture seule.
    StartVariance,
    /// Variation de durée (ligne de base 0), lecture seule.
    DurationVariance,
    /// Variation de travail (ligne de base 0), lecture seule.
    WorkVariance,
    /// Coût (non calculé : grisé).
    Cost,
    /// Coût fixe (non calculé : grisé).
    FixedCost,
    /// Coût réel (non calculé : grisé).
    ActualCost,
    /// Coût restant (non calculé : grisé).
    RemainingCost,
    /// Coût de la ligne de base 0 (non calculé : grisé).
    BaselineCost,
    /// Variation de coût (non calculé : grisé).
    CostVariance,
    /// Champ personnalisé (Texte1, Nombre1… ; H-UI-118, servi par L9).
    Custom(CustomFieldId),
}

/// Alignement horizontal du texte d'une cellule (ou d'un titre, H-UI-115).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CellAlign {
    /// Aligné à gauche.
    Left,
    /// Aligné à droite (nombres).
    Right,
    /// Centré.
    Center,
}

impl Column {
    /// Les 14 colonnes d'avant la seconde passe UX, dans leur ordre
    /// historique.
    pub const ALL: [Self; 14] = [
        Self::Id,
        Self::Name,
        Self::Duration,
        Self::Start,
        Self::Finish,
        Self::Predecessors,
        Self::TotalSlack,
        Self::Critical,
        Self::PercentComplete,
        Self::ResourceNames,
        Self::BaselineStart,
        Self::BaselineFinish,
        Self::FinishVariance,
        Self::Mode,
    ];

    /// Nombre de colonnes historiques (longueur de [`Column::ALL`]).
    pub const COUNT: usize = Self::ALL.len();

    /// Champs ajoutés par la seconde passe UX (H-UI-117), dans l'ordre de
    /// l'énumération (champs personnalisés exclus).
    pub const FIELDS: [Self; 79] = [
        Self::UniqueId,
        Self::Wbs,
        Self::OutlineLevel,
        Self::Summary,
        Self::Milestone,
        Self::Notes,
        Self::Created,
        Self::Priority,
        Self::Active,
        Self::Successors,
        Self::ConstraintType,
        Self::ConstraintDate,
        Self::Deadline,
        Self::TaskType,
        Self::EffortDriven,
        Self::Work,
        Self::EarlyStart,
        Self::EarlyFinish,
        Self::LateStart,
        Self::LateFinish,
        Self::FreeSlack,
        Self::PercentWorkComplete,
        Self::ActualStart,
        Self::ActualFinish,
        Self::ActualDuration,
        Self::RemainingDuration,
        Self::ActualWork,
        Self::RemainingWork,
        Self::BaselineDuration,
        Self::BaselineWork,
        Self::Baseline1Start,
        Self::Baseline1Finish,
        Self::Baseline1Duration,
        Self::Baseline1Work,
        Self::Baseline2Start,
        Self::Baseline2Finish,
        Self::Baseline2Duration,
        Self::Baseline2Work,
        Self::Baseline3Start,
        Self::Baseline3Finish,
        Self::Baseline3Duration,
        Self::Baseline3Work,
        Self::Baseline4Start,
        Self::Baseline4Finish,
        Self::Baseline4Duration,
        Self::Baseline4Work,
        Self::Baseline5Start,
        Self::Baseline5Finish,
        Self::Baseline5Duration,
        Self::Baseline5Work,
        Self::Baseline6Start,
        Self::Baseline6Finish,
        Self::Baseline6Duration,
        Self::Baseline6Work,
        Self::Baseline7Start,
        Self::Baseline7Finish,
        Self::Baseline7Duration,
        Self::Baseline7Work,
        Self::Baseline8Start,
        Self::Baseline8Finish,
        Self::Baseline8Duration,
        Self::Baseline8Work,
        Self::Baseline9Start,
        Self::Baseline9Finish,
        Self::Baseline9Duration,
        Self::Baseline9Work,
        Self::Baseline10Start,
        Self::Baseline10Finish,
        Self::Baseline10Duration,
        Self::Baseline10Work,
        Self::StartVariance,
        Self::DurationVariance,
        Self::WorkVariance,
        Self::Cost,
        Self::FixedCost,
        Self::ActualCost,
        Self::RemainingCost,
        Self::BaselineCost,
        Self::CostVariance,
    ];

    /// Largeur minimale d'une colonne, en points logiques.
    pub const MIN_WIDTH: u16 = 24;

    /// Largeur maximale d'une colonne, en points logiques.
    pub const MAX_WIDTH: u16 = 2_000;

    /// Position dans [`Column::ALL`] (indice des textes pré-formatés du
    /// `RowModel`) ; `None` pour un champ de la seconde passe.
    #[must_use]
    pub const fn ordinal(self) -> Option<usize> {
        Some(match self {
            Self::Id => 0,
            Self::Name => 1,
            Self::Duration => 2,
            Self::Start => 3,
            Self::Finish => 4,
            Self::Predecessors => 5,
            Self::TotalSlack => 6,
            Self::Critical => 7,
            Self::PercentComplete => 8,
            Self::ResourceNames => 9,
            Self::BaselineStart => 10,
            Self::BaselineFinish => 11,
            Self::FinishVariance => 12,
            Self::Mode => 13,
            _ => return None,
        })
    }

    /// Largeur par défaut, en points logiques (colonnes visibles : 642 au
    /// total).
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "une ligne par champ de la liste H-UI-117")]
    pub const fn default_width(self) -> u16 {
        match self {
            Self::Id => 40,
            Self::Name => 220,
            Self::Duration
            | Self::TotalSlack
            | Self::FinishVariance
            | Self::Wbs
            | Self::EffortDriven
            | Self::Work
            | Self::FreeSlack
            | Self::ActualDuration
            | Self::RemainingDuration
            | Self::ActualWork
            | Self::RemainingWork
            | Self::BaselineDuration
            | Self::BaselineWork
            | Self::Baseline1Duration
            | Self::Baseline1Work
            | Self::Baseline2Duration
            | Self::Baseline2Work
            | Self::Baseline3Duration
            | Self::Baseline3Work
            | Self::Baseline4Duration
            | Self::Baseline4Work
            | Self::Baseline5Duration
            | Self::Baseline5Work
            | Self::Baseline6Duration
            | Self::Baseline6Work
            | Self::Baseline7Duration
            | Self::Baseline7Work
            | Self::Baseline8Duration
            | Self::Baseline8Work
            | Self::Baseline9Duration
            | Self::Baseline9Work
            | Self::Baseline10Duration
            | Self::Baseline10Work
            | Self::StartVariance
            | Self::DurationVariance
            | Self::WorkVariance
            | Self::Cost
            | Self::FixedCost
            | Self::ActualCost
            | Self::RemainingCost
            | Self::BaselineCost
            | Self::CostVariance => 80,
            Self::Start
            | Self::Finish
            | Self::BaselineStart
            | Self::BaselineFinish
            | Self::Created
            | Self::ConstraintDate
            | Self::Deadline
            | Self::EarlyStart
            | Self::EarlyFinish
            | Self::LateStart
            | Self::LateFinish
            | Self::ActualStart
            | Self::ActualFinish
            | Self::Baseline1Start
            | Self::Baseline1Finish
            | Self::Baseline2Start
            | Self::Baseline2Finish
            | Self::Baseline3Start
            | Self::Baseline3Finish
            | Self::Baseline4Start
            | Self::Baseline4Finish
            | Self::Baseline5Start
            | Self::Baseline5Finish
            | Self::Baseline6Start
            | Self::Baseline6Finish
            | Self::Baseline7Start
            | Self::Baseline7Finish
            | Self::Baseline8Start
            | Self::Baseline8Finish
            | Self::Baseline9Start
            | Self::Baseline9Finish
            | Self::Baseline10Start
            | Self::Baseline10Finish => 96,
            Self::Predecessors | Self::Mode | Self::Successors | Self::TaskType => 110,
            Self::Critical
            | Self::PercentComplete
            | Self::Milestone
            | Self::Active
            | Self::PercentWorkComplete => 60,
            Self::ResourceNames | Self::ConstraintType => 140,
            Self::UniqueId | Self::OutlineLevel | Self::Summary | Self::Priority => 70,
            Self::Notes => 160,
            Self::Custom(id) => match id.kind() {
                CustomKind::Text => 120,
                CustomKind::Date => 96,
                CustomKind::Number => 80,
                CustomKind::Flag => 60,
            },
        }
    }

    /// Visible par défaut : ID, Nom, Durée, Début, Fin, Prédécesseurs.
    #[must_use]
    pub const fn default_visible(self) -> bool {
        matches!(
            self,
            Self::Id | Self::Name | Self::Duration | Self::Start | Self::Finish | Self::Predecessors
        )
    }

    /// La colonne peut-elle être masquée ? L'ID et le Nom restent toujours
    /// visibles (hypothèse d'affichage H-UI-12).
    #[must_use]
    pub const fn hideable(self) -> bool {
        !matches!(self, Self::Id | Self::Name)
    }

    /// Alignement du texte des cellules : nombres à droite (ID, durée,
    /// marge), indicateurs centrés, le reste à gauche.
    #[must_use]
    pub const fn align(self) -> CellAlign {
        match self {
            Self::Id
            | Self::Duration
            | Self::TotalSlack
            | Self::PercentComplete
            | Self::FinishVariance
            | Self::UniqueId
            | Self::OutlineLevel
            | Self::Priority
            | Self::Work
            | Self::FreeSlack
            | Self::PercentWorkComplete
            | Self::ActualDuration
            | Self::RemainingDuration
            | Self::ActualWork
            | Self::RemainingWork
            | Self::BaselineDuration
            | Self::BaselineWork
            | Self::Baseline1Duration
            | Self::Baseline1Work
            | Self::Baseline2Duration
            | Self::Baseline2Work
            | Self::Baseline3Duration
            | Self::Baseline3Work
            | Self::Baseline4Duration
            | Self::Baseline4Work
            | Self::Baseline5Duration
            | Self::Baseline5Work
            | Self::Baseline6Duration
            | Self::Baseline6Work
            | Self::Baseline7Duration
            | Self::Baseline7Work
            | Self::Baseline8Duration
            | Self::Baseline8Work
            | Self::Baseline9Duration
            | Self::Baseline9Work
            | Self::Baseline10Duration
            | Self::Baseline10Work
            | Self::StartVariance
            | Self::DurationVariance
            | Self::WorkVariance
            | Self::Cost
            | Self::FixedCost
            | Self::ActualCost
            | Self::RemainingCost
            | Self::BaselineCost
            | Self::CostVariance => CellAlign::Right,
            Self::Critical | Self::Summary | Self::Milestone | Self::Active | Self::EffortDriven => {
                CellAlign::Center
            }
            Self::Name
            | Self::Start
            | Self::Finish
            | Self::Predecessors
            | Self::ResourceNames
            | Self::BaselineStart
            | Self::BaselineFinish
            | Self::Mode
            | Self::Wbs
            | Self::Notes
            | Self::Created
            | Self::Successors
            | Self::ConstraintType
            | Self::ConstraintDate
            | Self::Deadline
            | Self::TaskType
            | Self::EarlyStart
            | Self::EarlyFinish
            | Self::LateStart
            | Self::LateFinish
            | Self::ActualStart
            | Self::ActualFinish
            | Self::Baseline1Start
            | Self::Baseline1Finish
            | Self::Baseline2Start
            | Self::Baseline2Finish
            | Self::Baseline3Start
            | Self::Baseline3Finish
            | Self::Baseline4Start
            | Self::Baseline4Finish
            | Self::Baseline5Start
            | Self::Baseline5Finish
            | Self::Baseline6Start
            | Self::Baseline6Finish
            | Self::Baseline7Start
            | Self::Baseline7Finish
            | Self::Baseline8Start
            | Self::Baseline8Finish
            | Self::Baseline9Start
            | Self::Baseline9Finish
            | Self::Baseline10Start
            | Self::Baseline10Finish => CellAlign::Left,
            Self::Custom(id) => match id.kind() {
                CustomKind::Number => CellAlign::Right,
                CustomKind::Flag => CellAlign::Center,
                CustomKind::Text | CustomKind::Date => CellAlign::Left,
            },
        }
    }

    /// Identifiant du titre traduit de la colonne ; champ personnalisé : le
    /// titre de son type, à compléter du numéro (`$n`) par
    /// [`Column::label`].
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "une ligne par champ de la liste H-UI-117")]
    pub const fn title(self) -> TextId {
        match self {
            Self::Id => TextId::ColId,
            Self::Name => TextId::ColName,
            Self::Duration => TextId::ColDuration,
            Self::Start => TextId::ColStart,
            Self::Finish => TextId::ColFinish,
            Self::Predecessors => TextId::ColPredecessors,
            Self::TotalSlack => TextId::ColTotalSlack,
            Self::Critical => TextId::ColCritical,
            Self::PercentComplete => TextId::ColPercentComplete,
            Self::ResourceNames => TextId::ColResourceNames,
            Self::BaselineStart => TextId::ColBaselineStart,
            Self::BaselineFinish => TextId::ColBaselineFinish,
            Self::FinishVariance => TextId::ColFinishVariance,
            Self::Mode => TextId::ColMode,
            Self::UniqueId => TextId::ColUniqueId,
            Self::Wbs => TextId::ColWbs,
            Self::OutlineLevel => TextId::ColOutlineLevel,
            Self::Summary => TextId::ColSummary,
            Self::Milestone => TextId::ColMilestone,
            Self::Notes => TextId::ColNotes,
            Self::Created => TextId::ColCreated,
            Self::Priority => TextId::ColPriority,
            Self::Active => TextId::ColActive,
            Self::Successors => TextId::ColSuccessors,
            Self::ConstraintType => TextId::ColConstraintType,
            Self::ConstraintDate => TextId::ColConstraintDate,
            Self::Deadline => TextId::ColDeadline,
            Self::TaskType => TextId::ColTaskType,
            Self::EffortDriven => TextId::ColEffortDriven,
            Self::Work => TextId::ColWork,
            Self::EarlyStart => TextId::ColEarlyStart,
            Self::EarlyFinish => TextId::ColEarlyFinish,
            Self::LateStart => TextId::ColLateStart,
            Self::LateFinish => TextId::ColLateFinish,
            Self::FreeSlack => TextId::ColFreeSlack,
            Self::PercentWorkComplete => TextId::ColPercentWorkComplete,
            Self::ActualStart => TextId::ColActualStart,
            Self::ActualFinish => TextId::ColActualFinish,
            Self::ActualDuration => TextId::ColActualDuration,
            Self::RemainingDuration => TextId::ColRemainingDuration,
            Self::ActualWork => TextId::ColActualWork,
            Self::RemainingWork => TextId::ColRemainingWork,
            Self::BaselineDuration => TextId::ColBaselineDuration,
            Self::BaselineWork => TextId::ColBaselineWork,
            Self::Baseline1Start => TextId::ColBaseline1Start,
            Self::Baseline1Finish => TextId::ColBaseline1Finish,
            Self::Baseline1Duration => TextId::ColBaseline1Duration,
            Self::Baseline1Work => TextId::ColBaseline1Work,
            Self::Baseline2Start => TextId::ColBaseline2Start,
            Self::Baseline2Finish => TextId::ColBaseline2Finish,
            Self::Baseline2Duration => TextId::ColBaseline2Duration,
            Self::Baseline2Work => TextId::ColBaseline2Work,
            Self::Baseline3Start => TextId::ColBaseline3Start,
            Self::Baseline3Finish => TextId::ColBaseline3Finish,
            Self::Baseline3Duration => TextId::ColBaseline3Duration,
            Self::Baseline3Work => TextId::ColBaseline3Work,
            Self::Baseline4Start => TextId::ColBaseline4Start,
            Self::Baseline4Finish => TextId::ColBaseline4Finish,
            Self::Baseline4Duration => TextId::ColBaseline4Duration,
            Self::Baseline4Work => TextId::ColBaseline4Work,
            Self::Baseline5Start => TextId::ColBaseline5Start,
            Self::Baseline5Finish => TextId::ColBaseline5Finish,
            Self::Baseline5Duration => TextId::ColBaseline5Duration,
            Self::Baseline5Work => TextId::ColBaseline5Work,
            Self::Baseline6Start => TextId::ColBaseline6Start,
            Self::Baseline6Finish => TextId::ColBaseline6Finish,
            Self::Baseline6Duration => TextId::ColBaseline6Duration,
            Self::Baseline6Work => TextId::ColBaseline6Work,
            Self::Baseline7Start => TextId::ColBaseline7Start,
            Self::Baseline7Finish => TextId::ColBaseline7Finish,
            Self::Baseline7Duration => TextId::ColBaseline7Duration,
            Self::Baseline7Work => TextId::ColBaseline7Work,
            Self::Baseline8Start => TextId::ColBaseline8Start,
            Self::Baseline8Finish => TextId::ColBaseline8Finish,
            Self::Baseline8Duration => TextId::ColBaseline8Duration,
            Self::Baseline8Work => TextId::ColBaseline8Work,
            Self::Baseline9Start => TextId::ColBaseline9Start,
            Self::Baseline9Finish => TextId::ColBaseline9Finish,
            Self::Baseline9Duration => TextId::ColBaseline9Duration,
            Self::Baseline9Work => TextId::ColBaseline9Work,
            Self::Baseline10Start => TextId::ColBaseline10Start,
            Self::Baseline10Finish => TextId::ColBaseline10Finish,
            Self::Baseline10Duration => TextId::ColBaseline10Duration,
            Self::Baseline10Work => TextId::ColBaseline10Work,
            Self::StartVariance => TextId::ColStartVariance,
            Self::DurationVariance => TextId::ColDurationVariance,
            Self::WorkVariance => TextId::ColWorkVariance,
            Self::Cost => TextId::ColCost,
            Self::FixedCost => TextId::ColFixedCost,
            Self::ActualCost => TextId::ColActualCost,
            Self::RemainingCost => TextId::ColRemainingCost,
            Self::BaselineCost => TextId::ColBaselineCost,
            Self::CostVariance => TextId::ColCostVariance,
            Self::Custom(id) => match id.kind() {
                CustomKind::Text => TextId::ColCustomText,
                CustomKind::Number => TextId::ColCustomNumber,
                CustomKind::Date => TextId::ColCustomDate,
                CustomKind::Flag => TextId::ColCustomFlag,
            },
        }
    }

    /// Titre affiché par défaut (« Durée », « Texte1 ») ; avec l'alias
    /// d'un champ personnalisé (« Lot (Texte1) », H-UI-118) :
    /// `field::column_label`.
    #[must_use]
    pub fn label(self, texts: &crate::i18n::Texts) -> String {
        match self {
            Self::Custom(id) => texts.get_args(
                self.title(),
                &[("n", crate::i18n::Arg::Count(i64::from(id.index())))],
            ),
            _ => texts.get(self.title()),
        }
    }
}

/// Plafond de colonnes affichées (H-UI-114, P63).
pub const MAX_COLUMNS: usize = 64;

/// Longueur maximale d'un titre de colonne, en caractères (H-UI-115).
pub const MAX_COLUMN_TITLE: usize = 64;

/// État persistant d'une colonne **affichée** (`UiState` version 2) :
/// largeur en points entiers (aucun flottant dans l'état sérialisé),
/// titre et alignements de « Paramètres de champ » (H-UI-115, `None` : par
/// défaut). Masquer une colonne la retire de la liste.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnState {
    /// Colonne concernée.
    pub column: Column,
    /// Largeur en points logiques.
    pub width: u16,
    /// Titre choisi (≤ [`MAX_COLUMN_TITLE`] caractères, jamais vide).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Alignement des données choisi.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<CellAlign>,
    /// Alignement du titre choisi.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header_align: Option<CellAlign>,
}

impl ColumnState {
    /// Titre affiché de la colonne : celui de « Paramètres de champ »
    /// (H-UI-115), sinon le titre par défaut du champ ([`Column::label`]).
    #[must_use]
    pub fn display_title(&self, texts: &crate::i18n::Texts) -> String {
        self.title.clone().unwrap_or_else(|| self.column.label(texts))
    }

    /// Alignement des données (H-UI-115) : celui choisi, sinon celui du
    /// champ.
    #[must_use]
    pub fn data_align(&self) -> CellAlign {
        self.align.unwrap_or(self.column.align())
    }

    /// Alignement du titre (H-UI-115) : celui choisi, sinon centré.
    #[must_use]
    pub fn title_align(&self) -> CellAlign {
        self.header_align.unwrap_or(CellAlign::Center)
    }

    /// État par défaut d'une colonne.
    #[must_use]
    pub const fn default_for(column: Column) -> Self {
        Self {
            column,
            width: column.default_width(),
            title: None,
            align: None,
            header_align: None,
        }
    }
}

/// Glisser d'un en-tête de colonne en cours (H-UI-116, servi par le lot
/// L6) : colonne déplacée et place du trait d'insertion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnDrag {
    /// Rang de la colonne glissée dans `UiState.columns`.
    pub from: usize,
    /// Rang devant lequel elle irait (`columns.len()` : en fin).
    pub to: usize,
    /// Le lâcher à cet endroit est refusé (avant l'ID).
    pub refused: bool,
}

/// Entrées ajoutées au menu de l'en-tête de colonne (H-UI-99, ADR-0018 D7 :
/// Ajuster la largeur, Trier par ordre croissant, décroissant) et
/// info-bulle de la poignée de bordure ; posées en L0, placées par le lot
/// L3 (`view::table::column_menu`, `view::table::resize_grip`).
pub const HEADER_MENU_TEXTS: [TextId; 4] = [
    TextId::MenuFitColumn,
    TextId::MenuSortAscending,
    TextId::MenuSortDescending,
    TextId::TipColumnResize,
];

/// Marge ajoutée à la largeur ajustée, en points : la troncature par « … »
/// (`grid::truncated_galley`) ne doit jamais jouer sur le texte le plus
/// large après un arrondi.
const FIT_SLACK: f32 = 1.0;

/// Nombre de textes mis en page exactement par l'ajustement : les plus
/// larges d'après la somme des avances de leurs glyphes. Hypothèse : le
/// crénage, seul écart entre cette somme et la mise en page, ne fait pas
/// passer un texte hors des 64 premiers devant le plus large ; au pire la
/// largeur manque de quelques dixièmes de point, absorbés par
/// [`FIT_SLACK`].
const FIT_EXACT: usize = 64;

/// Largeur ajustée d'une colonne (H-UI-99, S88 : double-clic sur la bordure
/// d'en-tête) : la plus grande des largeurs de texte `texts` (retrait de
/// niveau de chaque ligne déjà compris), augmentée de `indent` (retrait
/// commun à toutes les lignes), et du titre `title`, plus les marges `pad`
/// de chaque côté, arrondie au point supérieur et bornée à
/// `[Column::MIN_WIDTH, Column::MAX_WIDTH]`. Les valeurs non finies ou
/// négatives sont ignorées ; `None` si aucune valeur n'est utilisable ou si
/// `pad` ou `indent` ne sont pas finis.
#[must_use]
pub fn fit_width(texts: impl Iterator<Item = f32>, title: f32, pad: f32, indent: f32) -> Option<u16> {
    if !pad.is_finite() || !indent.is_finite() {
        return None;
    }
    let usable = |w: &f32| w.is_finite() && *w >= 0.0;
    let widest = texts
        .filter(usable)
        .map(|w| w + indent.max(0.0))
        .chain(std::iter::once(title).filter(usable))
        .reduce(f32::max)?;
    let w = (widest + 2.0 * pad.max(0.0) + FIT_SLACK).ceil();
    if !w.is_finite() {
        return None;
    }
    let w = w.clamp(f32::from(Column::MIN_WIDTH), f32::from(Column::MAX_WIDTH));
    // Borné à [24, 2 000] et fini : la conversion est exacte.
    u16::try_from(crate::geom::f32_to_index(w)).ok()
}

/// Clé de tri de la vue des tâches nommée par la colonne (menu d'en-tête,
/// H-UI-99) : seules les clés de [`SortKey`] ; `None` pour une colonne qui
/// ne se trie pas (entrées « Trier » grisées).
#[must_use]
pub fn sort_key(column: Column) -> Option<SortKey> {
    SortKey::ALL.into_iter().find(|key| key.column() == column)
}

impl crate::App {
    /// Opération sur les colonnes affichées (`Action::Column`, H-UI-114 à
    /// H-UI-116, lot L6) : une préférence d'affichage, hors de l'historique
    /// d'annulation. Insérer (à gauche du rang donné ; champ déjà affiché :
    /// déplacé ; 64 colonnes au plus, refus nommé), masquer, déplacer (l'ID
    /// reste en premier, refus nommé), régler (« Paramètres de champ ») ;
    /// ouvrir le sélecteur ou « Paramètres de champ » ([`ACTIVE_COLUMN`] :
    /// colonne de la cellule active). Défilement horizontal borné et
    /// curseur ramené au Nom si sa colonne disparaît.
    ///
    /// [`ACTIVE_COLUMN`]: crate::action::ACTIVE_COLUMN
    #[allow(
        clippy::needless_pass_by_value,
        reason = "action consommée par le réducteur (bras `Action::Column(op)`)"
    )]
    pub(crate) fn column_op(&mut self, op: crate::action::ColumnOp) {
        use crate::action::ColumnOp;
        let changed = match op {
            ColumnOp::Insert { at, column } => {
                let full = !self.state.shown(column) && self.state.columns.len() >= MAX_COLUMNS;
                if full {
                    let text = self.texts.get(TextId::ColumnsMaxReached);
                    self.push_message(plannercraft_core::Severity::Info, text);
                }
                self.state.insert_column(at, column)
            }
            ColumnOp::Hide(at) => self.state.hide_column(at),
            ColumnOp::Move { from, to } => {
                if (from == 0 || to == 0) && from != to && from < self.state.columns.len() {
                    let text = self.texts.get(TextId::ColumnIdStaysFirst);
                    self.push_message(plannercraft_core::Severity::Info, text);
                }
                self.state.move_column(from, to)
            }
            ColumnOp::Settings { at, settings } => self.state.set_column_settings(at, settings),
            ColumnOp::OpenPicker { at } => {
                if self.document.is_some() {
                    let at = self.column_rank(at);
                    self.view.picker = Some(crate::field::FieldPicker::insert_at(at));
                }
                false
            }
            ColumnOp::OpenSettings(at) => {
                let at = self.column_rank(at);
                if self.document.is_some()
                    && let Some(state) = self.state.columns.get(at)
                {
                    let draft = crate::dialog::field_settings::FieldSettingsDraft::of(state);
                    self.view.picker = Some(crate::field::FieldPicker {
                        at,
                        focus_pending: true,
                        settings: Some(Box::new(draft)),
                        ..crate::field::FieldPicker::default()
                    });
                }
                false
            }
        };
        if changed {
            self.columns_changed();
        }
    }

    /// Rang dans `UiState.columns` : `at` lui-même, ou, pour
    /// [`crate::action::ACTIVE_COLUMN`], celui de la colonne de la cellule
    /// active (le Nom si elle n'est pas affichée).
    fn column_rank(&self, at: usize) -> usize {
        if at != crate::action::ACTIVE_COLUMN {
            return at.min(self.state.columns.len());
        }
        let rank = |column: Column| self.state.columns.iter().position(|c| c.column == column);
        rank(self.view.cursor.column)
            .or_else(|| rank(Column::Name))
            .unwrap_or(1)
    }

    /// Ajuste la largeur de `column` au contenu des lignes affichées et à
    /// son titre (`Action::FitColumn`, double-clic sur la bordure ou menu
    /// d'en-tête, H-UI-99) : galley de chaque texte du `RowModel` des lignes
    /// affichées dans la police de la table (Inter si le style la demande),
    /// retrait de niveau du Nom (un cran de plus avec les triangles de
    /// repli) et gras simulé des récapitulatives compris ; titre dans la
    /// police des titres ; marges de `Metrics`. La largeur passe par le
    /// chemin de `SetColumnWidth` (`UiState::set_column_width`, bornée) :
    /// une préférence d'affichage, hors de l'historique d'annulation.
    pub(crate) fn fit_column(&mut self, column: Column, ctx: &egui::Context) {
        let (screen, _) = self.screen_style(ctx.theme());
        let metrics = crate::view::Metrics::default();
        let title_font = egui::TextStyle::Body.resolve(&ctx.global_style());
        let mut font = title_font.clone();
        if screen.inter() {
            font.family = egui::FontFamily::Name(crate::theme::PLAN_FAMILY.into());
        }
        let project = self.document.as_ref().map(crate::app::OpenDocument::project);
        let title = self.state.column(column).map_or_else(
            || crate::field::column_label(column, &self.texts, project),
            |state| crate::field::display_title(state, &self.texts, project),
        );
        let outline = self.outline_triangles;
        let Some(rows) = self.visible_rows() else {
            return;
        };
        let width = ctx.fonts_mut(|fonts| {
            // Estimation rapide d'abord (somme des avances des glyphes, un
            // cache par caractère), puis mise en page exacte des seuls
            // textes les plus larges (`FIT_EXACT`) : 5 000 galleys coûtent
            // plus que le budget de 50 ms (`best_fit_5000`).
            let mut advance: std::collections::BTreeMap<char, f32> = std::collections::BTreeMap::new();
            let mut candidates: Vec<(f32, f32, &str)> = (0..rows.len())
                .filter_map(|i| {
                    let d = crate::row_view::DisplayRow::at(i);
                    let row = rows.row(d)?;
                    let text = rows.text(d, column);
                    if text.is_empty() || row.kind == crate::rows::RowKind::Null {
                        return None;
                    }
                    let level = match column {
                        Column::Name if outline => row.level.saturating_add(1),
                        Column::Name => row.level,
                        _ => 0,
                    };
                    let indent = metrics.indent * f32::from(level.saturating_sub(1));
                    let bold = if row.kind == crate::rows::RowKind::Summary {
                        crate::grid::FAUX_BOLD_DX
                    } else {
                        0.0
                    };
                    let extra = indent + bold;
                    let estimate: f32 = text
                        .chars()
                        .map(|c| *advance.entry(c).or_insert_with(|| fonts.glyph_width(&font, c)))
                        .sum();
                    Some((estimate + extra, extra, text))
                })
                .collect();
            candidates.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
            let mut measure = |text: &str, font: &egui::FontId| {
                fonts
                    .layout_no_wrap(text.to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
                    .size()
                    .x
            };
            let texts: Vec<f32> = candidates
                .iter()
                .take(FIT_EXACT)
                .map(|(_, extra, text)| measure(text, &font) + extra)
                .collect();
            let title_w = measure(&title, &title_font);
            fit_width(texts.into_iter(), title_w, metrics.pad_x, 0.0)
        });
        if let Some(width) = width {
            self.state.set_column_width(column, width);
        }
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
    use std::collections::BTreeSet;

    use super::{CellAlign, Column, ColumnState};
    use crate::i18n::TextId;

    /// H-UI-99 : plus large des textes (retrait commun ajouté) et du titre,
    /// marges des deux côtés, un point de jeu, arrondi au point supérieur,
    /// bornes ; valeurs non finies ou négatives ignorées.
    #[test]
    fn column_fit_width_bounds() {
        use super::fit_width;
        assert_eq!(
            fit_width([50.2, 80.0, 12.0].into_iter(), 30.0, 4.0, 0.0),
            Some(89)
        );
        assert_eq!(fit_width([50.0].into_iter(), 30.0, 4.0, 14.0), Some(73));
        // Titre plus large que le contenu.
        assert_eq!(fit_width([10.0].into_iter(), 60.0, 4.0, 0.0), Some(69));
        assert_eq!(fit_width(std::iter::empty(), 60.0, 4.0, 0.0), Some(69));
        // Bornes.
        assert_eq!(
            fit_width([1.0].into_iter(), 1.0, 0.0, 0.0),
            Some(Column::MIN_WIDTH)
        );
        assert_eq!(
            fit_width([1e9].into_iter(), 0.0, 4.0, 0.0),
            Some(Column::MAX_WIDTH)
        );
        // Valeurs non finies ou négatives ignorées.
        assert_eq!(
            fit_width(
                [f32::NAN, f32::INFINITY, -5.0, 40.0].into_iter(),
                f32::NAN,
                4.0,
                0.0
            ),
            Some(49)
        );
        assert_eq!(fit_width([f32::NAN].into_iter(), f32::NAN, 4.0, 0.0), None);
        assert_eq!(fit_width([40.0].into_iter(), 10.0, f32::NAN, 0.0), None);
        assert_eq!(fit_width([40.0].into_iter(), 10.0, 4.0, f32::INFINITY), None);
        assert_eq!(fit_width([f32::MAX].into_iter(), 0.0, f32::MAX, 0.0), None);
    }

    /// H-UI-99 : les colonnes qui nomment une clé de `SortKey` se trient,
    /// les autres non.
    #[test]
    fn column_sort_key() {
        use super::sort_key;
        use crate::state::SortKey;
        for key in SortKey::ALL {
            assert_eq!(sort_key(key.column()), Some(key));
        }
        for column in [
            Column::Predecessors,
            Column::Critical,
            Column::ResourceNames,
            Column::BaselineStart,
            Column::BaselineFinish,
            Column::FinishVariance,
            Column::Mode,
        ] {
            assert_eq!(sort_key(column), None, "{column:?}");
        }
    }

    #[test]
    fn column_ordinal_matches_all() {
        assert_eq!(Column::COUNT, 14);
        for (index, column) in Column::ALL.iter().enumerate() {
            assert_eq!(column.ordinal(), Some(index), "{column:?}");
        }
        for column in Column::FIELDS {
            assert_eq!(column.ordinal(), None, "{column:?}");
            assert!(!column.default_visible() && column.hideable());
            let width = column.default_width();
            assert!((Column::MIN_WIDTH..=Column::MAX_WIDTH).contains(&width));
        }
    }

    #[test]
    fn column_default_widths() {
        let widths: Vec<u16> = Column::ALL.iter().map(|c| c.default_width()).collect();
        assert_eq!(
            widths,
            [40, 220, 80, 96, 96, 110, 80, 60, 60, 140, 96, 96, 80, 110]
        );
        for column in Column::ALL {
            let width = column.default_width();
            assert!((Column::MIN_WIDTH..=Column::MAX_WIDTH).contains(&width));
        }
    }

    #[test]
    fn column_default_visibility_and_hideable() {
        let hidden: Vec<Column> = Column::ALL.into_iter().filter(|c| !c.default_visible()).collect();
        assert_eq!(
            hidden,
            [
                Column::TotalSlack,
                Column::Critical,
                Column::PercentComplete,
                Column::ResourceNames,
                Column::BaselineStart,
                Column::BaselineFinish,
                Column::FinishVariance,
                Column::Mode,
            ]
        );
        assert_eq!(Column::default(), Column::Name);
        assert!(!Column::Id.hideable());
        assert!(!Column::Name.hideable());
        assert!(Column::Duration.hideable());
        assert!(Column::Critical.hideable());
    }

    #[test]
    fn column_align() {
        assert_eq!(Column::Id.align(), CellAlign::Right);
        assert_eq!(Column::Duration.align(), CellAlign::Right);
        assert_eq!(Column::TotalSlack.align(), CellAlign::Right);
        assert_eq!(Column::Critical.align(), CellAlign::Center);
        for column in [Column::Name, Column::Start, Column::Finish, Column::Predecessors] {
            assert_eq!(column.align(), CellAlign::Left, "{column:?}");
        }
    }

    #[test]
    fn column_titles_distinct() {
        let titles: BTreeSet<TextId> = Column::ALL.iter().map(|c| c.title()).collect();
        assert_eq!(titles.len(), Column::COUNT);
        assert_eq!(Column::Name.title(), TextId::ColName);
    }

    #[test]
    fn column_state_default_for() {
        let state = ColumnState::default_for(Column::Critical);
        assert_eq!(
            state,
            ColumnState {
                column: Column::Critical,
                width: 60,
                title: None,
                align: None,
                header_align: None,
            }
        );
        let state = ColumnState::default_for(Column::Name);
        assert_eq!(state.width, 220);
        // État v2 : réglages absents non écrits.
        assert_eq!(
            serde_json::to_string(&state).unwrap(),
            r#"{"column":"name","width":220}"#
        );
    }
}
