//! Volets en cartes (interface adoucie, lot U2, H-UI-172) : chaque volet
//! (ruban, vue, chronologie, Formulaire tâche, volet latéral, diagnostics)
//! est une carte posée sur le fond de l'application : marge extérieure de
//! [`OUTER`] points (4 en plein écran et sous [`NARROW_WINDOW`] points),
//! gouttières de [`GUTTER`] points, coins de [`RADIUS`] points, bordure d'un
//! point au filet des jetons ([`ChromeTokens::rule`]).
//!
//! Les séparateurs tirables d'ADR-0019 vivent dans les gouttières : la
//! gouttière fait exactement la zone de saisie d'un séparateur (± 4 points
//! autour de son milieu, `panes::SPLITTER_GRAB`).
//!
//! egui ne découpe pas à un rectangle arrondi : le contenu d'une carte est
//! dessiné dans son rectangle intérieur ([`inner`]), puis [`finish`] repeint
//! le fond de l'application autour des coins arrondis (trait extérieur de
//! [`MASK_W`] points, plus large que l'écart entre un coin carré et son arc)
//! et la bordure par-dessus. Aucun `f32` non fini n'est dessiné (règle 10).

use egui::{Color32, CornerRadius, Rect, Stroke, StrokeKind};

use crate::theme::ChromeTokens;

/// Marge extérieure entre les cartes et le bord de la fenêtre, en points.
pub const OUTER: f32 = 8.0;

/// Marge extérieure en plein écran et dans une fenêtre étroite.
pub const OUTER_TIGHT: f32 = 4.0;

/// Gouttière entre deux cartes, en points (zone de saisie d'un séparateur).
pub const GUTTER: f32 = 8.0;

/// Rayon des coins d'une carte, en points.
pub const RADIUS: f32 = 10.0;

/// Largeur de fenêtre sous laquelle la marge extérieure passe à
/// [`OUTER_TIGHT`].
pub const NARROW_WINDOW: f32 = 900.0;

/// Largeur du trait qui repeint le fond autour des coins : plus que
/// l'écart entre le coin carré et l'arc (`RADIUS × (√2 − 1)` ≈ 4,1 points),
/// moins que la gouttière (la carte voisine n'est jamais touchée).
const MASK_W: f32 = 5.0;

/// Clé du retrait de l'`Ui` racine dans la mémoire d'egui.
fn inset_id() -> egui::Id {
    egui::Id::unique("plannercraft.cards.inset")
}

/// Retient le retrait de l'`Ui` racine par rapport à la fenêtre (`App::show`,
/// à chaque image) : nul dans l'application ; le harnais de kittest pose
/// une marge de 8 points autour de l'`Ui` racine, déduite de la marge des
/// cartes pour que les captures montrent les marges de l'application.
pub fn set_inset(ctx: &egui::Context, inset: f32) {
    let inset = if inset.is_finite() { inset.max(0.0) } else { 0.0 };
    ctx.data_mut(|d| d.insert_temp(inset_id(), inset));
}

/// Marge extérieure de l'image, comptée depuis le bord de la fenêtre :
/// [`OUTER_TIGHT`] en plein écran ou sous [`NARROW_WINDOW`] points de
/// large, [`OUTER`] sinon (moins le retrait de l'`Ui` racine).
#[must_use]
pub fn outer_margin(ctx: &egui::Context) -> f32 {
    let width = ctx.content_rect().width();
    let fullscreen = ctx.input(|i| i.viewport().fullscreen).unwrap_or(false);
    let inset = ctx.data(|d| d.get_temp::<f32>(inset_id())).unwrap_or(0.0);
    (outer_for(width, fullscreen) - inset).max(0.0)
}

/// Marge extérieure pour une fenêtre de `width` points (fonction pure).
#[must_use]
pub fn outer_for(width: f32, fullscreen: bool) -> f32 {
    if fullscreen || (width.is_finite() && width < NARROW_WINDOW) {
        OUTER_TIGHT
    } else {
        OUTER
    }
}

/// Marge d'egui (`i8`) d'une marge de cartes : [`OUTER`] ou [`GUTTER`] ⇒ 8,
/// [`OUTER_TIGHT`] ⇒ 4, autre ⇒ 0 (aucune conversion de flottant).
#[must_use]
pub fn px(margin: f32) -> i8 {
    if margin >= OUTER {
        8
    } else if margin >= OUTER_TIGHT {
        4
    } else {
        0
    }
}

/// Rectangle du contenu d'une carte : la carte moins sa bordure.
#[must_use]
pub fn inner(card: Rect) -> Rect {
    card.shrink(1.0)
}

/// Fond d'une carte (surface des jetons), à peindre avant son contenu.
pub fn paint_fill(painter: &egui::Painter, card: Rect) {
    if !card.is_finite() || !card.is_positive() {
        return;
    }
    let tokens = ChromeTokens::for_theme(painter.ctx().theme());
    painter.rect_filled(card, CornerRadius::same(radius_u8()), tokens.surface);
}

/// Fin d'une carte, après son contenu : fond de l'application repeint
/// autour des coins arrondis, puis bordure d'un point.
pub fn finish(painter: &egui::Painter, card: Rect) {
    if !card.is_finite() || !card.is_positive() {
        return;
    }
    let tokens = ChromeTokens::for_theme(painter.ctx().theme());
    finish_with(painter, card, tokens.app, tokens.rule);
}

/// [`finish`] aux couleurs données (fond de l'application, bordure).
pub fn finish_with(painter: &egui::Painter, card: Rect, app: Color32, border: Color32) {
    if !card.is_finite() || !card.is_positive() {
        return;
    }
    let corner = CornerRadius::same(radius_u8());
    painter.rect_stroke(card, corner, Stroke::new(MASK_W, app), StrokeKind::Outside);
    painter.rect_stroke(card, corner, Stroke::new(1.0, border), StrokeKind::Inside);
}

/// Rayon en entier pour `CornerRadius` (10 points).
const fn radius_u8() -> u8 {
    // RADIUS = 10.0, constante : conversion exacte.
    10
}

/// Fond de l'application (jetons du thème de `ctx`) peint sur toute la
/// fenêtre, au plan du fond.
pub fn paint_app_background(ctx: &egui::Context) {
    let rect = ctx.content_rect();
    if !rect.is_finite() {
        return;
    }
    let tokens = ChromeTokens::for_theme(ctx.theme());
    ctx.layer_painter(egui::LayerId::background())
        .rect_filled(rect, 0.0, tokens.app);
}

#[cfg(test)]
mod tests {
    use super::{GUTTER, MASK_W, OUTER, OUTER_TIGHT, RADIUS, outer_for, radius_u8};

    /// H-UI-172 : marges de 8 points, 4 en plein écran et sous 900 points ;
    /// largeur non finie ⇒ marge normale.
    #[test]
    fn h_ui_172_outer_margin() {
        assert!((outer_for(1280.0, false) - OUTER).abs() < f32::EPSILON);
        assert!((outer_for(1280.0, true) - OUTER_TIGHT).abs() < f32::EPSILON);
        assert!((outer_for(899.0, false) - OUTER_TIGHT).abs() < f32::EPSILON);
        assert!((outer_for(900.0, false) - OUTER).abs() < f32::EPSILON);
        assert!((outer_for(f32::NAN, false) - OUTER).abs() < f32::EPSILON);
    }

    /// H-UI-172 : le masque des coins couvre l'écart coin carré ↔ arc sans
    /// atteindre la carte voisine ; rayon entier égal à `RADIUS`.
    #[test]
    fn h_ui_172_mask_width() {
        let gap = RADIUS * (std::f32::consts::SQRT_2 - 1.0);
        assert!(MASK_W > gap && MASK_W < GUTTER);
        assert!((f32::from(radius_u8()) - RADIUS).abs() < f32::EPSILON);
    }
}
