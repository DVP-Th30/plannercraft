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

/// Forme du ruban (interface adoucie, lot U2, H-UI-174).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RibbonMode {
    /// Une ligne de 52 points (défaut) : commandes prioritaires, le reste
    /// dans le menu « … » de chaque groupe.
    #[default]
    Compact,
    /// Ruban complet d'ADR-0017 (grands et petits boutons, noms des
    /// groupes).
    Full,
    /// Replié : seule la barre du haut reste (H-UI-72).
    Collapsed,
}

/// Préférences du ruban persistées (`UiState.ribbon`, H-UI-69, H-UI-72,
/// H-UI-174) : écrites `{"tab", "collapsed", "mode"}` ; `collapsed` garde
/// son sens (ruban replié) pour une version d'avant le lot U2, qui ignore
/// `mode` ; un état d'avant le lot U2 (`{"tab", "collapsed"}`) est relu
/// sans avertissement : `collapsed` vrai ⇒ [`RibbonMode::Collapsed`],
/// sinon [`RibbonMode::Compact`] (le nouveau défaut). Aucun champ retiré
/// ni renommé : la version d'`UiState` ne change pas.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "RibbonPrefsRead", into = "RibbonPrefsWrite")]
pub struct RibbonPrefs {
    /// Dernier onglet choisi.
    pub tab: RibbonTab,
    /// Forme du ruban.
    pub mode: RibbonMode,
}

impl RibbonPrefs {
    /// Ruban replié (seule la barre du haut reste).
    #[must_use]
    pub fn collapsed(self) -> bool {
        self.mode == RibbonMode::Collapsed
    }
}

/// Lecture tolérante des préférences du ruban : champ `mode` (lot U2) ou
/// ancien booléen `collapsed` (ADR-0017), champs absents à leur défaut.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(default)]
struct RibbonPrefsRead {
    tab: RibbonTab,
    mode: Option<RibbonMode>,
    collapsed: Option<bool>,
}

/// Écriture des préférences du ruban : `collapsed` (sens d'ADR-0017,
/// dérivé du mode) puis `mode`.
#[derive(Clone, Copy, Debug, Serialize)]
struct RibbonPrefsWrite {
    tab: RibbonTab,
    collapsed: bool,
    mode: RibbonMode,
}

impl From<RibbonPrefs> for RibbonPrefsWrite {
    fn from(prefs: RibbonPrefs) -> Self {
        Self {
            tab: prefs.tab,
            collapsed: prefs.collapsed(),
            mode: prefs.mode,
        }
    }
}

impl From<RibbonPrefsRead> for RibbonPrefs {
    fn from(read: RibbonPrefsRead) -> Self {
        let mode = read.mode.unwrap_or(if read.collapsed == Some(true) {
            RibbonMode::Collapsed
        } else {
            RibbonMode::Compact
        });
        Self { tab: read.tab, mode }
    }
}

/// Opération sur l'état du ruban (`Action::Ribbon`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RibbonOp {
    /// Choisit l'onglet (ruban déplié).
    Select(RibbonTab),
    /// Replie ou déplie (double-clic sur un onglet, Ctrl/Cmd+F1, Affichage
    /// → Réduire le ruban ; H-UI-72) ; referme la surimpression. Déplier
    /// rend le ruban compact (H-UI-174).
    ToggleCollapsed,
    /// Chevron du ruban (H-UI-174) : compact ⇄ complet ; un ruban replié
    /// devient complet ; referme la surimpression.
    ToggleFull,
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
            prefs.mode = if prefs.collapsed() {
                RibbonMode::Compact
            } else {
                RibbonMode::Collapsed
            };
            *overlay = None;
        }
        RibbonOp::ToggleFull => {
            prefs.mode = match prefs.mode {
                RibbonMode::Full => RibbonMode::Compact,
                RibbonMode::Compact | RibbonMode::Collapsed => RibbonMode::Full,
            };
            *overlay = None;
        }
        RibbonOp::OpenOverlay(tab) => {
            prefs.tab = tab;
            *overlay = prefs.collapsed().then_some(tab);
        }
        RibbonOp::CloseOverlay => *overlay = None,
    }
}

#[cfg(test)]
mod tests {
    use super::{RibbonMode, RibbonOp, RibbonPrefs, RibbonTab, reduce};

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
        assert!(prefs.collapsed());
        reduce(&mut prefs, &mut overlay, RibbonOp::OpenOverlay(RibbonTab::View));
        assert_eq!(overlay, Some(RibbonTab::View));
        reduce(&mut prefs, &mut overlay, RibbonOp::Select(RibbonTab::Task));
        assert_eq!(overlay, Some(RibbonTab::Task));
        reduce(&mut prefs, &mut overlay, RibbonOp::CloseOverlay);
        assert_eq!(overlay, None);
        reduce(&mut prefs, &mut overlay, RibbonOp::OpenOverlay(RibbonTab::View));
        reduce(&mut prefs, &mut overlay, RibbonOp::ToggleCollapsed);
        assert!(!prefs.collapsed());
        assert_eq!(prefs.mode, RibbonMode::Compact);
        assert_eq!(overlay, None);
    }

    /// H-UI-174 : compact par défaut ; le chevron passe de compact à
    /// complet et retour ; un ruban replié passe au complet ; Ctrl/Cmd+F1
    /// replie un ruban complet et le déplie compact ; la surimpression est
    /// refermée.
    #[test]
    fn h_ui_174_reduce_modes() {
        let mut prefs = RibbonPrefs::default();
        let mut overlay = None;
        assert_eq!(prefs.mode, RibbonMode::Compact);
        reduce(&mut prefs, &mut overlay, RibbonOp::ToggleFull);
        assert_eq!(prefs.mode, RibbonMode::Full);
        reduce(&mut prefs, &mut overlay, RibbonOp::ToggleFull);
        assert_eq!(prefs.mode, RibbonMode::Compact);
        reduce(&mut prefs, &mut overlay, RibbonOp::ToggleFull);
        reduce(&mut prefs, &mut overlay, RibbonOp::ToggleCollapsed);
        assert_eq!(prefs.mode, RibbonMode::Collapsed);
        reduce(&mut prefs, &mut overlay, RibbonOp::OpenOverlay(RibbonTab::View));
        assert_eq!(overlay, Some(RibbonTab::View));
        reduce(&mut prefs, &mut overlay, RibbonOp::ToggleFull);
        assert_eq!((prefs.mode, overlay), (RibbonMode::Full, None));
        reduce(&mut prefs, &mut overlay, RibbonOp::ToggleCollapsed);
        reduce(&mut prefs, &mut overlay, RibbonOp::ToggleCollapsed);
        assert_eq!(prefs.mode, RibbonMode::Compact);
    }

    /// Préférences : JSON stable (`snake_case`), relues.
    #[test]
    fn ribbon_prefs_json() {
        let prefs = RibbonPrefs {
            tab: RibbonTab::Resource,
            mode: RibbonMode::Collapsed,
        };
        let json = serde_json::to_string(&prefs).unwrap_or_default();
        assert_eq!(json, r#"{"tab":"resource","collapsed":true,"mode":"collapsed"}"#);
        assert_eq!(serde_json::from_str::<RibbonPrefs>(&json).ok(), Some(prefs));
        assert_eq!(
            serde_json::from_str::<RibbonPrefs>("{}").ok(),
            Some(RibbonPrefs::default())
        );
        for mode in [RibbonMode::Compact, RibbonMode::Full, RibbonMode::Collapsed] {
            let prefs = RibbonPrefs {
                tab: RibbonTab::View,
                mode,
            };
            let json = serde_json::to_string(&prefs).unwrap_or_default();
            assert_eq!(serde_json::from_str::<RibbonPrefs>(&json).ok(), Some(prefs));
        }
    }

    /// H-UI-174 : un état d'avant le lot U2 (`collapsed`) est relu sans
    /// erreur ; replié ⇒ mode replié, déplié ⇒ compact (nouveau défaut) ;
    /// `mode` l'emporte sur `collapsed` s'il y a les deux.
    #[test]
    fn h_ui_174_prefs_migration() {
        let read = |json: &str| serde_json::from_str::<RibbonPrefs>(json).ok();
        assert_eq!(
            read(r#"{"tab":"format","collapsed":true}"#),
            Some(RibbonPrefs {
                tab: RibbonTab::Format,
                mode: RibbonMode::Collapsed,
            })
        );
        assert_eq!(
            read(r#"{"tab":"project","collapsed":false}"#),
            Some(RibbonPrefs {
                tab: RibbonTab::Project,
                mode: RibbonMode::Compact,
            })
        );
        assert_eq!(
            read(r#"{"tab":"task","mode":"full","collapsed":true}"#),
            Some(RibbonPrefs {
                tab: RibbonTab::Task,
                mode: RibbonMode::Full,
            })
        );
        assert_eq!(read(r#"{"mode":"plié"}"#), None);
    }
}
