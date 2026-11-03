//! Ruban à onglets (ADR-0017, H-UI-69 à H-UI-77).
//!
//! Le ruban est une **table de données** ([`spec::RIBBON`] : onglet →
//! groupes → éléments) dessinée par un seul moteur (`view::ribbon`). Chaque
//! élément pousse l'[`Action`](crate::Action) de son jumeau de la barre de
//! menus, avec le même prédicat d'activation ([`spec::RibbonCmd::action`],
//! [`spec::RibbonCmd::enabled`]) : aucune commande n'est atteignable par le
//! ruban seul.
//!
//! Lot L0 (contrat) : types, réducteur de l'état du ruban ([`reduce`]),
//! icônes ([`icon_data`], généré par `cargo xtask icons` ; [`icons`],
//! tracé) et table. Lot L2 : moteur de dessin (`view::ribbon`) et menus
//! jumeaux (`view::chrome`). Lot L5 : le ruban devient le seul habillage
//! (l'ancienne barre d'outils de J7 et J8 est retirée).

#[rustfmt::skip]
pub mod icon_data;
pub mod icons;
pub mod spec;

use serde::{Deserialize, Serialize};

pub use icon_data::Icon;
pub use icons::paint_icon;

/// Onglet du ruban (H-UI-69 ; S80 : Task, Resource, Project, View, Format).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RibbonTab {
    /// Tâche (onglet du premier lancement).
    #[default]
    Task,
    /// Ressource.
    Resource,
    /// Projet.
    Project,
    /// Affichage.
    View,
    /// Format.
    Format,
}

impl RibbonTab {
    /// Onglets, dans l'ordre d'affichage.
    pub const ALL: [Self; 5] = [
        Self::Task,
        Self::Resource,
        Self::Project,
        Self::View,
        Self::Format,
    ];
}

/// Préférences du ruban persistées (`UiState.ribbon`, H-UI-69, H-UI-72).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RibbonPrefs {
    /// Dernier onglet choisi.
    pub tab: RibbonTab,
    /// Ruban replié (seule la bande d'onglets reste).
    pub collapsed: bool,
}

/// Opération sur l'état du ruban (`Action::Ribbon`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RibbonOp {
    /// Choisit l'onglet (ruban déplié).
    Select(RibbonTab),
    /// Replie ou déplie (double-clic sur un onglet, Ctrl/Cmd+F1, Affichage
    /// → Réduire le ruban ; H-UI-72) ; referme la surimpression.
    ToggleCollapsed,
    /// Ruban replié : ouvre l'onglet en surimpression au-dessus de la vue
    /// (H-UI-72) ; déplié : choisit simplement l'onglet.
    OpenOverlay(RibbonTab),
    /// Referme la surimpression (action du ruban, Échap, clic ailleurs).
    CloseOverlay,
}

/// Activation d'un élément du ruban ou de son jumeau de menu (H-UI-70,
/// H-UI-73) : actif, sinon la raison affichée dans l'info-bulle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Enabled {
    /// Actif.
    pub yes: bool,
    /// Raison du grisé (`None` si actif).
    pub reason: Option<crate::i18n::TextId>,
}

impl Enabled {
    /// Actif.
    pub const YES: Self = Self {
        yes: true,
        reason: None,
    };

    /// Grisé pour `reason`.
    #[must_use]
    pub const fn no(reason: crate::i18n::TextId) -> Self {
        Self {
            yes: false,
            reason: Some(reason),
        }
    }
}

/// Applique `op` aux préférences du ruban et à la surimpression (onglet
/// ouvert au-dessus de la vue quand le ruban est replié).
pub fn reduce(prefs: &mut RibbonPrefs, overlay: &mut Option<RibbonTab>, op: RibbonOp) {
    match op {
        RibbonOp::Select(tab) => {
            prefs.tab = tab;
            if overlay.is_some() {
                *overlay = Some(tab);
            }
        }
        RibbonOp::ToggleCollapsed => {
            prefs.collapsed = !prefs.collapsed;
            *overlay = None;
        }
        RibbonOp::OpenOverlay(tab) => {
            prefs.tab = tab;
            *overlay = prefs.collapsed.then_some(tab);
        }
        RibbonOp::CloseOverlay => *overlay = None,
    }
}

#[cfg(test)]
mod tests {
    use super::{RibbonOp, RibbonPrefs, RibbonTab, reduce};

    /// H-UI-69, H-UI-72 : onglet retenu ; repli ; surimpression seulement
    /// replié, refermée par le repli et par `CloseOverlay`.
    #[test]
    fn ribbon_reduce_tab_collapse_overlay() {
        let mut prefs = RibbonPrefs::default();
        let mut overlay = None;
        assert_eq!(prefs.tab, RibbonTab::Task);
        reduce(&mut prefs, &mut overlay, RibbonOp::Select(RibbonTab::Format));
        assert_eq!((prefs.tab, overlay), (RibbonTab::Format, None));
        // Déplié : « ouvrir en surimpression » choisit seulement l'onglet.
        reduce(
            &mut prefs,
            &mut overlay,
            RibbonOp::OpenOverlay(RibbonTab::Project),
        );
        assert_eq!((prefs.tab, overlay), (RibbonTab::Project, None));
        reduce(&mut prefs, &mut overlay, RibbonOp::ToggleCollapsed);
        assert!(prefs.collapsed);
        reduce(&mut prefs, &mut overlay, RibbonOp::OpenOverlay(RibbonTab::View));
        assert_eq!(overlay, Some(RibbonTab::View));
        reduce(&mut prefs, &mut overlay, RibbonOp::Select(RibbonTab::Task));
        assert_eq!(overlay, Some(RibbonTab::Task));
        reduce(&mut prefs, &mut overlay, RibbonOp::CloseOverlay);
        assert_eq!(overlay, None);
        reduce(&mut prefs, &mut overlay, RibbonOp::OpenOverlay(RibbonTab::View));
        reduce(&mut prefs, &mut overlay, RibbonOp::ToggleCollapsed);
        assert!(!prefs.collapsed);
        assert_eq!(overlay, None);
    }

    /// Préférences : JSON stable (`snake_case`), relues.
    #[test]
    fn ribbon_prefs_json() {
        let prefs = RibbonPrefs {
            tab: RibbonTab::Resource,
            collapsed: true,
        };
        let json = serde_json::to_string(&prefs).unwrap_or_default();
        assert_eq!(json, r#"{"tab":"resource","collapsed":true}"#);
        assert_eq!(serde_json::from_str::<RibbonPrefs>(&json).ok(), Some(prefs));
        assert_eq!(
            serde_json::from_str::<RibbonPrefs>("{}").ok(),
            Some(RibbonPrefs::default())
        );
    }
}
