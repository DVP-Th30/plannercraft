//! Tracé des icônes du ruban (H-UI-77, ADR-0017 D6).
//!
//! Les icônes Lucide sont des polylignes précalculées au build
//! ([`super::icon_data`], `cargo xtask icons`) : egui les trace comme les
//! liens du Gantt, à toute échelle (1×, 1,5×, 2×, 3× : retessellées par
//! epaint, jamais une texture), à la couleur demandée (texte du thème,
//! atténuée si l'élément est grisé). Trait de 2/24 du côté, disques aux
//! extrémités des polylignes ouvertes et aux points isolés (bouts ronds de
//! Lucide).

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke};

use super::icon_data::{Icon, IconData};

/// Côté de la grille Lucide.
const GRID: f32 = 24.0;

/// Sous-unités par unité de grille dans [`super::icon_data`].
const SUBUNITS: f32 = 64.0;

/// Épaisseur du trait Lucide, en unités de grille.
const STROKE: f32 = 2.0;

/// Formes epaint de `icon` dans `rect` (carré centré de côté
/// `min(largeur, hauteur)`) : vide si le rectangle n'est pas fini ou est
/// vide.
#[must_use]
pub fn icon_shapes(rect: Rect, icon: Icon, color: Color32) -> Vec<Shape> {
    let side = rect.width().min(rect.height());
    if !rect.is_finite() || !side.is_finite() || side <= 0.0 {
        return Vec::new();
    }
    let scale = side / GRID;
    let center = rect.center();
    let origin = Pos2::new(center.x - side / 2.0, center.y - side / 2.0);
    let at = |(x, y): (i16, i16)| {
        Pos2::new(
            origin.x + f32::from(x) / SUBUNITS * scale,
            origin.y + f32::from(y) / SUBUNITS * scale,
        )
    };
    let width = STROKE * scale;
    let stroke = Stroke::new(width, color);
    let radius = width / 2.0;
    let data: &IconData = icon.data();
    let mut shapes = Vec::with_capacity(data.lines.len().saturating_mul(3).saturating_add(data.dots.len()));
    for line in data.lines {
        let points: Vec<Pos2> = line.points.iter().copied().map(at).collect();
        if line.closed {
            shapes.push(Shape::closed_line(points, stroke));
        } else {
            let ends = [points.first().copied(), points.last().copied()];
            shapes.push(Shape::line(points, stroke));
            for end in ends.into_iter().flatten() {
                shapes.push(Shape::circle_filled(end, radius, color));
            }
        }
    }
    for dot in data.dots {
        shapes.push(Shape::circle_filled(at(*dot), radius, color));
    }
    shapes
}

/// Peint `icon` dans `rect` à la couleur `color` ; rectangle non fini ou
/// vide : rien.
pub fn paint_icon(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    for shape in icon_shapes(rect, icon, color) {
        painter.add(shape);
    }
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::panic,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use egui::{Color32, Pos2, Rect, vec2};

    use super::{Icon, icon_shapes};

    /// H-UI-77 : chaque icône a des formes, chaque polyligne au moins deux
    /// points.
    #[test]
    fn h_ui_77_every_icon_has_shapes() {
        assert!(Icon::ALL.len() >= 40);
        for icon in Icon::ALL {
            let data = icon.data();
            assert!(!data.lines.is_empty() || !data.dots.is_empty(), "{}", icon.name());
            assert!(data.lines.iter().all(|l| l.points.len() >= 2), "{}", icon.name());
            let shapes = icon_shapes(
                Rect::from_min_size(Pos2::ZERO, vec2(24.0, 24.0)),
                icon,
                Color32::BLACK,
            );
            assert!(!shapes.is_empty(), "{}", icon.name());
        }
    }

    /// H-UI-77 : toute forme (trait compris) reste dans le rectangle, à 16,
    /// 24 et 32 px, à l'origine ou décalé.
    #[test]
    fn h_ui_77_icon_inside_rect() {
        for side in [16.0_f32, 24.0, 32.0] {
            for origin in [Pos2::ZERO, Pos2::new(103.5, 41.25)] {
                let rect = Rect::from_min_size(origin, vec2(side, side));
                for icon in Icon::ALL {
                    for shape in icon_shapes(rect, icon, Color32::WHITE) {
                        let bounds = shape.visual_bounding_rect();
                        assert!(
                            rect.expand(0.01).contains_rect(bounds),
                            "{} à {side} px : {bounds:?} hors de {rect:?}",
                            icon.name()
                        );
                    }
                }
            }
        }
    }

    /// H-UI-77 : rectangle non fini, vide ou retourné ⇒ rien.
    #[test]
    fn h_ui_77_non_finite_rect_draws_nothing() {
        for rect in [
            Rect::NOTHING,
            Rect::EVERYTHING,
            Rect::from_min_size(Pos2::ZERO, vec2(0.0, 24.0)),
            Rect::from_min_size(Pos2::ZERO, vec2(f32::NAN, 24.0)),
            Rect::from_min_size(Pos2::new(f32::INFINITY, 0.0), vec2(24.0, 24.0)),
            Rect::from_min_max(Pos2::new(10.0, 10.0), Pos2::ZERO),
        ] {
            assert!(
                icon_shapes(rect, Icon::Save, Color32::WHITE).is_empty(),
                "{rect:?}"
            );
        }
    }

    /// Couleur demandée sur toutes les formes (thème, grisé : H-UI-77).
    #[test]
    fn h_ui_77_icon_takes_color() {
        let gray = Color32::from_gray(140);
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(16.0, 16.0));
        for shape in icon_shapes(rect, Icon::Link, gray) {
            match shape {
                egui::Shape::Path(p) => assert_eq!(p.stroke.color, egui::epaint::ColorMode::Solid(gray)),
                egui::Shape::Circle(c) => assert_eq!(c.fill, gray),
                other => panic!("forme inattendue : {other:?}"),
            }
        }
    }
}
