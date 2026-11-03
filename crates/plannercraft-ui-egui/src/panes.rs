//! Volets de la vue principale (seconde passe UX, ADR-0019 D5, H-UI-111) :
//! chronologie en haut, vue (table et Gantt) au milieu, volet de détails
//! (« Formulaire tâche ») en bas, séparés par des séparateurs tirés.
//!
//! Fonctions pures (aucun egui) : disposition des hauteurs ([`layout`]),
//! glisser d'un séparateur ([`drag_split`]) et passage d'un volet à
//! l'autre au clavier ([`next`], F6). Toute hauteur `f32` est testée
//! finie et bornée avant usage (règle 10). `App::pane_op` écrit hauteurs,
//! visibilité et forme dans `UiState` ; `view::main_view` dessine les
//! volets et les séparateurs (lot L4) et sert F6.

use crate::action::PaneOp;
use crate::state::{
    DETAILS_HEIGHT_DEFAULT, DETAILS_HEIGHT_MIN, PANE_HEIGHT_MAX, TIMELINE_HEIGHT_DEFAULT, TIMELINE_HEIGHT_MIN,
};

/// Part maximale de la zone donnée à la chronologie (H-UI-111).
pub const TIMELINE_MAX_SHARE: f32 = 0.4;

/// Part maximale de la zone donnée au volet de détails (H-UI-111).
pub const DETAILS_MAX_SHARE: f32 = 0.6;

/// Hauteur minimale gardée à la vue (table et Gantt), en points.
pub const VIEW_MIN: f32 = 120.0;

/// Demi-épaisseur de la zone de saisie d'un séparateur, en points.
pub const SPLITTER_GRAB: f32 = 4.0;

/// Hauteurs des trois volets, en points (0 : volet absent).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PaneHeights {
    /// Chronologie.
    pub timeline: f32,
    /// Vue (table et Gantt).
    pub view: f32,
    /// Volet de détails.
    pub details: f32,
}

/// Hauteur demandée `wanted` (non finie ⇒ `default`), bornée à `[min,
/// share × total]` (le plafond ne descend pas sous `min`).
fn bounded(wanted: f32, default: f32, min: f32, share: f32, total: f32) -> f32 {
    let wanted = if wanted.is_finite() { wanted } else { default };
    let max = (share * total).max(min);
    wanted.clamp(min, max)
}

/// Disposition de la zone de hauteur `total` : chronologie de hauteur
/// `timeline` si `Some` (affichée), volet de détails de hauteur `details`
/// si `Some`. Chaque volet est borné (chronologie de 48 points à 40 % de
/// la zone, détails de 120 points à 60 %, valeur non finie ⇒ hauteur par
/// défaut) ; la vue garde au moins [`VIEW_MIN`] : faute de place, le volet
/// de détails puis la chronologie rétrécissent jusqu'à leur minimum, puis
/// disparaissent (hauteur 0). `total` non fini ou négatif ⇒ tout à 0.
#[must_use]
pub fn layout(total: f32, timeline: Option<f32>, details: Option<f32>) -> PaneHeights {
    if !total.is_finite() || total <= 0.0 {
        return PaneHeights::default();
    }
    let t_min = f32::from(TIMELINE_HEIGHT_MIN);
    let d_min = f32::from(DETAILS_HEIGHT_MIN);
    let mut t = timeline.map_or(0.0, |h| {
        bounded(
            h,
            f32::from(TIMELINE_HEIGHT_DEFAULT),
            t_min,
            TIMELINE_MAX_SHARE,
            total,
        )
    });
    let mut d = details.map_or(0.0, |h| {
        bounded(
            h,
            f32::from(DETAILS_HEIGHT_DEFAULT),
            d_min,
            DETAILS_MAX_SHARE,
            total,
        )
    });
    // Place manquante pour la vue : détails, puis chronologie, rétrécis
    // jusqu'à leur minimum, puis retirés (détails d'abord).
    let mut lack = VIEW_MIN - (total - t - d);
    for (pane, min) in [(&mut d, d_min), (&mut t, t_min)] {
        if lack > 0.0 && *pane > 0.0 {
            let cut = lack.min(*pane - min).max(0.0);
            *pane -= cut;
            lack -= cut;
        }
    }
    if total - t - d < VIEW_MIN {
        d = 0.0;
    }
    if total - t - d < VIEW_MIN {
        t = 0.0;
    }
    PaneHeights {
        timeline: t,
        view: (total - t - d).max(0.0),
        details: d,
    }
}

/// Hauteur après un glisser de `delta` points depuis `current`, bornée à
/// `[min, max]` ; `delta` ou `current` non fini, ou bornes invalides ⇒
/// `current` (inchangée).
#[must_use]
pub fn drag_split(current: f32, delta: f32, min: f32, max: f32) -> f32 {
    if !current.is_finite() || !delta.is_finite() || !min.is_finite() || !max.is_finite() || min > max {
        return current;
    }
    (current + delta).clamp(min, max)
}

/// Volet qui reçoit le clavier (F6, Maj+F6 ; H-UI-111).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PaneFocus {
    /// Table et Gantt.
    #[default]
    Table,
    /// Chronologie.
    Timeline,
    /// Volet de détails.
    Details,
}

/// Volet suivant (`back` : précédent) dans l'ordre de l'écran (chronologie,
/// table, détails), en sautant les volets masqués ; `visible` :
/// (chronologie, détails) affichés. La table est toujours là.
#[must_use]
pub fn next(f: PaneFocus, visible: (bool, bool), back: bool) -> PaneFocus {
    let order = if back {
        [PaneFocus::Details, PaneFocus::Table, PaneFocus::Timeline]
    } else {
        [PaneFocus::Timeline, PaneFocus::Table, PaneFocus::Details]
    };
    let shown = |p: PaneFocus| match p {
        PaneFocus::Table => true,
        PaneFocus::Timeline => visible.0,
        PaneFocus::Details => visible.1,
    };
    let at = order.iter().position(|p| *p == f).unwrap_or(1);
    order
        .iter()
        .cycle()
        .skip(at.saturating_add(1))
        .take(order.len())
        .copied()
        .find(|p| shown(*p))
        .unwrap_or(PaneFocus::Table)
}

/// Glisser d'un séparateur en cours (H-UI-111).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplitDrag {
    /// Séparateur tiré.
    pub splitter: Splitter,
    /// Hauteur du volet au début du glisser, en points.
    pub start_height: f32,
    /// Ordonnée du pointeur au début du glisser.
    pub start_y: f32,
}

/// Séparateur entre deux volets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Splitter {
    /// Sous la chronologie.
    Timeline,
    /// Au-dessus du volet de détails.
    Details,
}

impl crate::App {
    /// Opération sur les volets (`Action::Pane`) : hauteurs bornées
    /// ([`TIMELINE_HEIGHT_MIN`], [`DETAILS_HEIGHT_MIN`],
    /// [`PANE_HEIGHT_MAX`]), visibilité et forme du volet de détails
    /// écrites dans `UiState` (préférences, hors de l'historique
    /// d'annulation, enregistrées avec les autres préférences). F6 / Maj+F6
    /// : demande servie par la vue Gantt à l'image suivante (volet courant
    /// lu au focus d'egui, `view::main_view`) ; hors de la vue Gantt,
    /// rien (un seul volet).
    pub(crate) fn pane_op(&mut self, op: PaneOp) {
        match op {
            PaneOp::SetTimelineHeight(h) => {
                self.state.timeline_height = h.clamp(TIMELINE_HEIGHT_MIN, PANE_HEIGHT_MAX);
            }
            PaneOp::ShowDetails(on) => {
                // Masquer le volet valide d'abord son brouillon (OK
                // implicite, H-UI-113) : refus ⇒ le volet reste affiché
                // avec son refus, rien n'est perdu sans le dire.
                if !on && !self.details_op(crate::action::DetailsOp::Ok) {
                    return;
                }
                self.state.details.visible = on;
            }
            PaneOp::SetDetailsHeight(h) => {
                self.state.details.height = h.clamp(DETAILS_HEIGHT_MIN, PANE_HEIGHT_MAX);
            }
            PaneOp::SetDetailsForm(form) => self.state.details.form = form,
            PaneOp::FocusNext { back } => {
                if self.state.view == crate::state::ViewKind::Gantt && self.document.is_some() {
                    self.view.panes.focus_request = Some(back);
                }
            }
        }
    }
}

/// Textes posés par le lot L0 pour les volets, servis par le lot L4 :
/// « Masquer la chronologie », bouton × et séparateurs.
pub const L4_TEXTS: [crate::i18n::TextId; 4] = [
    crate::i18n::TextId::TimelineHide,
    crate::i18n::TextId::TimelineCloseTip,
    crate::i18n::TextId::SplitterTip,
    crate::i18n::TextId::DetailsCloseTip,
];

#[cfg(test)]
#[allow(clippy::float_cmp, reason = "tests : hauteurs exactes")]
mod tests {
    use super::{PaneFocus, PaneHeights, VIEW_MIN, drag_split, layout, next};

    /// H-UI-111 : bornes (48 à 40 %, 120 à 60 %), non fini ⇒ défauts, vue ≥
    /// 120 points, volets retirés faute de place.
    #[test]
    fn panes_layout_bounds() {
        // Défauts dans une grande zone.
        let h = layout(800.0, Some(72.0), Some(220.0));
        assert_eq!(
            h,
            PaneHeights {
                timeline: 72.0,
                view: 508.0,
                details: 220.0
            }
        );
        // Non fini ⇒ défaut ; absents ⇒ 0.
        let h = layout(800.0, Some(f32::NAN), Some(f32::INFINITY));
        assert_eq!((h.timeline, h.details), (72.0, 360.0));
        assert_eq!(layout(800.0, None, None).view, 800.0);
        // Bornes basses et hautes (relatives à la zone).
        let h = layout(1000.0, Some(1.0), Some(1.0));
        assert_eq!((h.timeline, h.details), (48.0, 120.0));
        let h = layout(1000.0, Some(900.0), None);
        assert_eq!(h.timeline, 400.0);
        let h = layout(1000.0, None, Some(900.0));
        assert_eq!(h.details, 600.0);
        // Les deux au plus : la vue garde 120 points.
        let h = layout(1000.0, Some(900.0), Some(900.0));
        assert!(h.view >= VIEW_MIN, "{h:?}");
        assert_eq!(h.timeline + h.view + h.details, 1000.0);
        // Les deux au plus dans une zone moyenne : détails rétrécis, pas
        // retirés (relecture L4).
        let h = layout(600.0, Some(4000.0), Some(4000.0));
        assert_eq!((h.timeline, h.view, h.details), (240.0, 120.0, 240.0));
        // Peu de place : détails puis chronologie rétrécis puis retirés.
        let h = layout(300.0, Some(72.0), Some(220.0));
        assert!(h.view >= VIEW_MIN, "{h:?}");
        assert_eq!(h.timeline + h.view + h.details, 300.0);
        let h = layout(200.0, Some(72.0), Some(220.0));
        assert_eq!(h.details, 0.0);
        assert!(h.view >= VIEW_MIN, "{h:?}");
        let h = layout(130.0, Some(72.0), Some(220.0));
        assert_eq!((h.timeline, h.details, h.view), (0.0, 0.0, 130.0));
        // Zone absurde.
        for total in [f32::NAN, f32::INFINITY, -5.0, 0.0] {
            assert_eq!(layout(total, Some(72.0), Some(220.0)), PaneHeights::default());
        }
        for total in [50.0_f32, 168.0, 170.0, 290.0, 410.0, 4000.0] {
            let h = layout(total, Some(72.0), Some(220.0));
            assert!(h.timeline.is_finite() && h.view.is_finite() && h.details.is_finite());
            assert!(
                (h.timeline + h.view + h.details - total).abs() < 1e-3,
                "{total} {h:?}"
            );
        }
        // Glisser borné, non fini ⇒ inchangé.
        assert_eq!(drag_split(72.0, 80.0, 48.0, 400.0), 152.0);
        assert_eq!(drag_split(72.0, -80.0, 48.0, 400.0), 48.0);
        assert_eq!(drag_split(72.0, 1e9, 48.0, 400.0), 400.0);
        assert_eq!(drag_split(72.0, f32::NAN, 48.0, 400.0), 72.0);
        assert_eq!(drag_split(72.0, 5.0, 48.0, f32::NAN), 72.0);
        assert_eq!(drag_split(72.0, 5.0, 400.0, 48.0), 72.0);
    }

    /// H-UI-111 : F6 dans l'ordre de l'écran, volets masqués sautés ; Maj+F6
    /// à rebours.
    #[test]
    fn panes_focus_cycle() {
        use PaneFocus::{Details, Table, Timeline};
        let all = (true, true);
        assert_eq!(next(Table, all, false), Details);
        assert_eq!(next(Details, all, false), Timeline);
        assert_eq!(next(Timeline, all, false), Table);
        assert_eq!(next(Table, all, true), Timeline);
        assert_eq!(next(Timeline, all, true), Details);
        assert_eq!(next(Table, (false, true), false), Details);
        assert_eq!(next(Details, (false, true), false), Table);
        assert_eq!(next(Table, (true, false), false), Timeline);
        assert_eq!(next(Table, (false, false), false), Table);
        assert_eq!(next(Table, (false, false), true), Table);
        // Volet devenu masqué : on repart vers un volet affiché.
        assert_eq!(next(Details, (false, false), false), Table);
    }
}
