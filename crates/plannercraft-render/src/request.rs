//! Requête de rendu, bornes et rapport (lot L7).

use std::collections::BTreeSet;

use plannercraft_core::{Day, TaskUid};

/// Requête de rendu d'un Gantt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GanttRequest {
    /// Plage de jours.
    pub range: RangeSpec,
    /// Échelle.
    pub zoom: Zoom,
    /// Lignes retenues.
    pub filter: RowFilter,
    /// Langue des libellés.
    pub language: Lang,
    /// Thème.
    pub theme: Theme,
    /// Bornes.
    pub limits: RenderLimits,
}

/// Plage de jours affichée.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RangeSpec {
    /// Plage du projet.
    #[default]
    Auto,
    /// Jours donnés, inclus.
    Span {
        /// Premier jour.
        from: Day,
        /// Dernier jour.
        to: Day,
    },
}

/// Échelle de temps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Zoom {
    /// Jours (36 px par jour).
    Day,
    /// Semaines (12 px par jour).
    #[default]
    Week,
    /// Mois (3,5 px par jour).
    Month,
    /// Trimestres (1,2 px par jour).
    Quarter,
    /// Ajustée à une largeur.
    Auto {
        /// Largeur visée en pixels.
        width_px: u32,
    },
    /// Remplie (impression ajustée « N pages en largeur », J8 lot L16) :
    /// échelle continue qui occupe exactement `width_px` sur la plage, au
    /// plus 36 px par jour ; niveaux de l'en-tête d'un cran plus fins que
    /// l'échelle discrète atteinte (H-R4). Le Gantt imprimé remplit la page
    /// au lieu de s'arrêter à l'échelle discrète inférieure.
    Fill {
        /// Largeur à remplir en pixels.
        width_px: u32,
    },
}

/// Filtre des lignes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RowFilter {
    /// Tâches critiques seules.
    pub critical_only: bool,
    /// Tâches retenues (toutes si `None`).
    pub uids: Option<BTreeSet<TaskUid>>,
    /// Niveau hiérarchique maximal.
    pub max_outline: Option<u16>,
}

/// Langue des libellés (le rendu ne dépend pas d'`automation`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lang {
    /// Français.
    #[default]
    Fr,
    /// Anglais.
    En,
}

/// Thème.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    /// Clair.
    #[default]
    Light,
    /// Sombre.
    Dark,
}

/// Bornes du rendu (règle 10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderLimits {
    /// Lignes au plus (400).
    pub max_rows: u32,
    /// Jours au plus (7 305, vingt ans).
    pub max_days: u32,
    /// Octets au plus (4 Mio).
    pub max_bytes: usize,
    /// Caractères d'un nom avant troncature (40).
    pub name_chars: u16,
}

impl Default for RenderLimits {
    fn default() -> Self {
        Self {
            max_rows: 400,
            max_days: 7_305,
            max_bytes: 4 * 1024 * 1024,
            name_chars: 40,
        }
    }
}

/// Rapport du rendu : ce qui a été montré ou coupé.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderReport {
    /// Lignes affichées.
    pub shown: u32,
    /// Lignes masquées (filtre ou borne).
    pub hidden: u32,
    /// Plage coupée à `max_days`.
    pub truncated_days: bool,
    /// Sortie coupée à `max_bytes`.
    pub truncated_bytes: bool,
    /// Taille de la sortie.
    pub bytes: usize,
}
