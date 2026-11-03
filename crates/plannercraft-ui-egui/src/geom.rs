//! Géométrie pure de la vue principale : conversions numériques, plage de
//! lignes visibles, mise en page.
//!
//! Toutes les conversions entre entiers et `f32` sont regroupées ici, pour
//! que les lints pedantic `cast_*` ne soient levées qu'à un seul endroit, avec
//! leur raison. Aucune conversion ne produit une date à partir d'un pixel.

use std::ops::Range;

use egui::{Pos2, Rect, pos2, vec2};
use plannercraft_core::TaskUid;

use crate::rows::RowKind;
use crate::state::TABLE_WIDTH_MIN;
use crate::view::Metrics;

/// Indice de ligne → pixels.
#[expect(
    clippy::cast_precision_loss,
    reason = "indice de ligne → pixels ; exact sous 2^24 lignes"
)]
#[must_use]
pub fn index_to_f32(i: usize) -> f32 {
    i as f32
}

/// Pixels → indice de ligne : 0 pour NaN ou une valeur négative, saturé
/// au-delà de `usize::MAX`.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "`as` sature et donne 0 pour NaN ; négatifs filtrés"
)]
#[must_use]
pub fn f32_to_index(v: f32) -> usize {
    if v.is_nan() || v <= 0.0 { 0 } else { v as usize }
}

/// Minutes (écart déjà borné par l'appelant) → `f32`, pour le dessin.
#[expect(
    clippy::cast_precision_loss,
    reason = "dessin : écart borné à 20 ans ≈ 10,5 M min < 2^24, exact en f32"
)]
#[must_use]
pub fn i64_to_f32(v: i64) -> f32 {
    v as f32
}

/// `f32` → entier borné à `[-bound, bound]` (`bound` ≥ 0) : 0 pour NaN,
/// saturé aux bornes ; partie fractionnaire tronquée (l'appelant arrondit).
#[expect(
    clippy::cast_possible_truncation,
    reason = "valeur bornée à ±bound avant la conversion ; `as` sature et donne 0 pour NaN"
)]
#[must_use]
pub fn f32_to_bounded_i64(v: f32, bound: i64) -> i64 {
    let bound = bound.max(0);
    if v.is_nan() {
        return 0;
    }
    (v as i64).clamp(bound.saturating_neg(), bound)
}

/// Largeur minimale laissée au Gantt par le séparateur, en points logiques.
pub const GANTT_MIN_WIDTH: f32 = 200.0;

/// Lignes qui coupent `[top, bottom)` (coordonnées de contenu), élargies de
/// `overscan`. Jamais de panique : résultat inclus dans `0..len`, avec
/// `start ≤ end` ; plage vide pour NaN, infinis ou hauteur nulle.
#[must_use]
pub fn visible_rows(top: f32, bottom: f32, row_h: f32, len: usize, overscan: usize) -> Range<usize> {
    if len == 0
        || !row_h.is_finite()
        || row_h <= 0.0
        || !top.is_finite()
        || !bottom.is_finite()
        || bottom <= top
    {
        return 0..0;
    }
    let first = f32_to_index((top.max(0.0) / row_h).floor()).min(len);
    let end = f32_to_index((bottom / row_h).ceil()).min(len);
    let start = first.saturating_sub(overscan);
    let end = end.saturating_add(overscan).min(len);
    start.min(end)..end
}

/// Ligne sous l'ordonnée `y` (coordonnées de contenu : 0 en haut de la
/// ligne 0), si elle existe. Indice de ligne seulement : jamais de date.
#[must_use]
pub fn row_at(y: f32, row_h: f32, len: usize) -> Option<usize> {
    if !y.is_finite() || !row_h.is_finite() || row_h <= 0.0 || y < 0.0 {
        return None;
    }
    let i = f32_to_index((y / row_h).floor());
    (i < len).then_some(i)
}

/// Cible d'un pointeur dans le Gantt (J7, lot L8, H-UI-20, H-UI-21) :
/// UID et zones seulement, jamais une date.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GanttHit {
    /// Intérieur d'une barre (déplacer, ou lier si lâché sur une autre
    /// ligne ; récapitulative : lier seulement).
    Bar(TaskUid),
    /// Bord gauche d'une barre : glisser l'avancement.
    BarStart(TaskUid),
    /// Bord droit d'une barre : redimensionner.
    BarEnd(TaskUid),
    /// Tracé d'un lien (prédécesseur, successeur).
    Link(TaskUid, TaskUid),
    /// Rien : le glisser fait défiler.
    Empty,
}

/// Zone d'une barre sous le pointeur (H-UI-20).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarZone {
    /// Bord gauche : avancement.
    Progress,
    /// Bord droit : durée.
    Resize,
    /// Intérieur : déplacer ou lier.
    Inside,
}

/// Largeur des bords actifs d'une barre, en points.
pub const BAR_EDGE: f32 = 4.0;

/// Largeur en dessous de laquelle une barre n'a que son intérieur.
pub const BAR_ZONES_MIN: f32 = 12.0;

/// Zone de l'abscisse `x` sur une barre qui s'étend de `left` à `right`
/// (H-UI-20) : feuille d'au moins [`BAR_ZONES_MIN`] points, `[left, left +
/// 4[` avancement et `]right − 4, right]` durée, le reste intérieur ;
/// feuille plus étroite, jalon et récapitulative : intérieur seul ; ligne
/// vide ou exclue, hors de la barre ou valeurs non finies : `None`.
#[must_use]
pub fn bar_zone(x: f32, left: f32, right: f32, kind: RowKind) -> Option<BarZone> {
    if !(x.is_finite() && left.is_finite() && right.is_finite()) || x < left || x > right {
        return None;
    }
    match kind {
        RowKind::Null | RowKind::Excluded => None,
        RowKind::Leaf if right - left >= BAR_ZONES_MIN && x - left < BAR_EDGE => Some(BarZone::Progress),
        RowKind::Leaf if right - left >= BAR_ZONES_MIN && right - x < BAR_EDGE => Some(BarZone::Resize),
        RowKind::Leaf | RowKind::Milestone | RowKind::Summary => Some(BarZone::Inside),
    }
}

/// Vrai si `p` est à au plus `tol` points d'un segment de la polyligne
/// `points` (tracé d'un lien) ; faux pour une polyligne de moins de deux
/// points ou des valeurs non finies.
#[must_use]
pub fn near_polyline(p: Pos2, points: &[Pos2], tol: f32) -> bool {
    if !p.is_finite() || !tol.is_finite() {
        return false;
    }
    points.windows(2).any(|w| match w {
        [a, b] => segment_distance(p, *a, *b) <= tol,
        _ => false,
    })
}

/// Distance de `point` au segment `[from, to]` (infinie si non finie).
fn segment_distance(point: Pos2, from: Pos2, to: Pos2) -> f32 {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let len2 = dx * dx + dy * dy;
    let along = if len2 > 0.0 {
        (((point.x - from.x) * dx + (point.y - from.y) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let distance = pos2(from.x + dx * along, from.y + dy * along).distance(point);
    if distance.is_finite() {
        distance
    } else {
        f32::INFINITY
    }
}

/// Rectangles de la vue principale (coordonnées écran).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MainLayout {
    /// En-tête de la table (titres de colonnes).
    pub table_header: Rect,
    /// En-tête du Gantt (échelle de temps à deux niveaux).
    pub gantt_header: Rect,
    /// Séparateur déplaçable, pleine hauteur.
    pub splitter: Rect,
    /// Rectangle donné à la `ScrollArea` verticale unique.
    pub body: Rect,
    /// Découpe du corps de la table.
    pub table_clip: Rect,
    /// Découpe du corps du Gantt.
    pub gantt_clip: Rect,
    /// Barre de défilement horizontale du Gantt.
    pub gantt_hbar: Rect,
}

/// Largeur effective de la table dans une zone de largeur `avail_w` :
/// bornée à `[TABLE_WIDTH_MIN, avail_w − GANTT_MIN_WIDTH]`, puis à la place
/// disponible (séparateur compris) si la zone est trop étroite.
#[must_use]
pub fn clamp_table_width(table_width: f32, avail_w: f32, splitter_w: f32) -> f32 {
    let avail_w = if avail_w.is_finite() {
        avail_w.max(0.0)
    } else {
        0.0
    };
    let splitter_w = if splitter_w.is_finite() {
        splitter_w.max(0.0)
    } else {
        0.0
    };
    let low = f32::from(TABLE_WIDTH_MIN);
    let high = (avail_w - GANTT_MIN_WIDTH).max(low);
    let wanted = if table_width.is_finite() { table_width } else { low };
    let room = (avail_w - splitter_w).max(0.0);
    wanted.max(low).min(high).min(room)
}

/// Mise en page de la vue principale dans `avail` : en-tête de la table, en-tête
/// du Gantt et séparateur sur `header_h`, puis le corps commun, puis la barre
/// horizontale du Gantt sur `hbar_h`. Le corps occupe toute la largeur ; le
/// Gantt (en-tête, découpe, barre horizontale) s'arrête `vbar_w` avant le
/// bord droit, place de la barre verticale pleine. La table, de largeur `table_width`, est
/// bornée par [`clamp_table_width`]. Un `avail` non fini donne des rectangles
/// vides à l'origine (jamais de NaN dans un `Rect`).
#[must_use]
pub fn main_layout(avail: Rect, table_width: f32, m: &Metrics) -> MainLayout {
    let avail = if avail.is_finite() {
        Rect::from_min_size(avail.min, vec2(avail.width().max(0.0), avail.height().max(0.0)))
    } else {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(0.0, 0.0))
    };
    let finite = |v: f32| if v.is_finite() { v.max(0.0) } else { 0.0 };
    let splitter_w = finite(m.splitter_w).min(avail.width());
    let table_w = clamp_table_width(table_width, avail.width(), splitter_w);
    let split_left = avail.left() + table_w;
    let split_right = (split_left + splitter_w).min(avail.right());
    let header_bottom = (avail.top() + finite(m.header_h)).min(avail.bottom());
    let hbar_top = (avail.bottom() - finite(m.hbar_h)).max(header_bottom);
    // Le Gantt s'arrête avant la barre verticale pleine du corps.
    let gantt_right = (avail.right() - finite(m.vbar_w)).max(split_right);
    let table_header = Rect::from_min_max(avail.min, pos2(split_left, header_bottom));
    let gantt_header = Rect::from_min_max(pos2(split_right, avail.top()), pos2(gantt_right, header_bottom));
    let splitter = Rect::from_min_max(pos2(split_left, avail.top()), pos2(split_right, avail.bottom()));
    let body = Rect::from_min_max(pos2(avail.left(), header_bottom), pos2(avail.right(), hbar_top));
    let table_clip = Rect::from_min_max(body.min, pos2(split_left, body.bottom()));
    let gantt_clip = Rect::from_min_max(pos2(split_right, body.top()), pos2(gantt_right, body.bottom()));
    let gantt_hbar = Rect::from_min_max(pos2(split_right, hbar_top), pos2(gantt_right, avail.bottom()));
    MainLayout {
        table_header,
        gantt_header,
        splitter,
        body,
        table_clip,
        gantt_clip,
        gantt_hbar,
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    reason = "tests : un échec doit s'arrêter net ; valeurs exactes attendues"
)]
mod tests {
    use egui::{Rect, pos2, vec2};

    use super::{
        BarZone, GANTT_MIN_WIDTH, bar_zone, f32_to_bounded_i64, f32_to_index, index_to_f32, main_layout,
        near_polyline, row_at, visible_rows,
    };
    use crate::rows::RowKind;
    use crate::state::TABLE_WIDTH_MIN;
    use crate::view::Metrics;

    #[test]
    fn conversions_saturate() {
        assert_eq!(f32_to_index(f32::NAN), 0);
        assert_eq!(f32_to_index(-3.0), 0);
        assert_eq!(f32_to_index(3.9), 3);
        assert_eq!(f32_to_index(f32::INFINITY), usize::MAX);
        assert_eq!(index_to_f32(42), 42.0);
    }

    #[test]
    fn visible_rows_top() {
        // Fenêtre de 220 px en haut : lignes 0 à 9 visibles, + 2 de marge.
        assert_eq!(visible_rows(0.0, 220.0, 22.0, 100, 2), 0..12);
    }

    #[test]
    fn visible_rows_middle() {
        // 440 / 22 = 20 ; 660 / 22 = 30.
        assert_eq!(visible_rows(440.0, 660.0, 22.0, 100, 2), 18..32);
        // Ligne coupée en haut : 450 / 22 = 20,4 → la ligne 20 est visible.
        assert_eq!(visible_rows(450.0, 660.0, 22.0, 100, 0), 20..30);
    }

    #[test]
    fn visible_rows_last_partial() {
        // 10 lignes (220 px), fenêtre 150..230 : la dernière ligne (9) est
        // partiellement visible et la fin est bornée à `len`.
        let r = visible_rows(150.0, 230.0, 22.0, 10, 2);
        assert_eq!(r, 4..10);
        assert_eq!(visible_rows(200.0, 210.0, 22.0, 10, 0), 9..10);
    }

    #[test]
    fn visible_rows_past_end() {
        let r = visible_rows(5_000.0, 5_200.0, 22.0, 10, 2);
        assert!(r.start <= r.end && r.end <= 10, "{r:?}");
        assert!(r.len() <= 2, "{r:?}");
        let r = visible_rows(5_000.0, 5_200.0, 22.0, 10, 0);
        assert!(r.is_empty(), "{r:?}");
    }

    #[test]
    fn visible_rows_empty() {
        assert_eq!(visible_rows(0.0, 500.0, 22.0, 0, 2), 0..0);
        assert_eq!(visible_rows(100.0, 100.0, 22.0, 10, 2), 0..0);
        assert_eq!(visible_rows(200.0, 100.0, 22.0, 10, 2), 0..0);
        assert_eq!(visible_rows(0.0, 100.0, 0.0, 10, 2), 0..0);
        assert_eq!(visible_rows(0.0, 100.0, -22.0, 10, 2), 0..0);
    }

    #[test]
    fn visible_rows_nan() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(visible_rows(bad, 100.0, 22.0, 10, 2), 0..0);
            assert_eq!(visible_rows(0.0, bad, 22.0, 10, 2), 0..0);
            assert_eq!(visible_rows(0.0, 100.0, bad, 10, 2), 0..0);
        }
        // Valeurs extrêmes mais finies : bornées à `len`.
        let r = visible_rows(-1e30, 1e30, 22.0, 10, usize::MAX);
        assert_eq!(r, 0..10);
    }

    #[test]
    fn bounded_i64_conversion() {
        assert_eq!(f32_to_bounded_i64(f32::NAN, 10), 0);
        assert_eq!(f32_to_bounded_i64(f32::INFINITY, 10), 10);
        assert_eq!(f32_to_bounded_i64(f32::NEG_INFINITY, 10), -10);
        assert_eq!(f32_to_bounded_i64(3.7, 10), 3);
        assert_eq!(f32_to_bounded_i64(-3.0, 10), -3);
        assert_eq!(f32_to_bounded_i64(5.0, -1), 0);
    }

    #[test]
    fn h_ui_20_bar_zones() {
        // Barre de 60 px : x ∈ [0, 4[ avancement, ]56, 60] durée.
        let zone = |x| bar_zone(x, 0.0, 60.0, RowKind::Leaf);
        assert_eq!(zone(0.0), Some(BarZone::Progress));
        assert_eq!(zone(3.9), Some(BarZone::Progress));
        assert_eq!(zone(4.0), Some(BarZone::Inside));
        assert_eq!(zone(56.0), Some(BarZone::Inside));
        assert_eq!(zone(56.1), Some(BarZone::Resize));
        assert_eq!(zone(60.0), Some(BarZone::Resize));
        assert_eq!(zone(60.1), None);
        assert_eq!(zone(-0.1), None);
        assert_eq!(zone(f32::NAN), None);
        // Barre < 12 px : intérieur seul ; jalon, récapitulative : intérieur.
        assert_eq!(bar_zone(1.0, 0.0, 11.9, RowKind::Leaf), Some(BarZone::Inside));
        assert_eq!(bar_zone(0.0, 0.0, 60.0, RowKind::Summary), Some(BarZone::Inside));
        assert_eq!(
            bar_zone(59.0, 0.0, 60.0, RowKind::Milestone),
            Some(BarZone::Inside)
        );
        assert_eq!(bar_zone(10.0, 0.0, 60.0, RowKind::Excluded), None);
        assert_eq!(bar_zone(10.0, 0.0, 60.0, RowKind::Null), None);
    }

    #[test]
    fn near_polyline_segments() {
        let pts = [pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(10.0, 20.0)];
        assert!(near_polyline(pos2(5.0, 2.0), &pts, 3.0));
        assert!(near_polyline(pos2(12.0, 15.0), &pts, 3.0));
        assert!(!near_polyline(pos2(5.0, 5.0), &pts, 3.0));
        assert!(!near_polyline(pos2(f32::NAN, 0.0), &pts, 3.0));
        assert!(!near_polyline(pos2(0.0, 0.0), &pts[..1], 3.0));
        // Segment dégénéré : distance au point.
        assert!(near_polyline(
            pos2(1.0, 1.0),
            &[pos2(0.0, 0.0), pos2(0.0, 0.0)],
            2.0
        ));
    }

    #[test]
    fn row_at_bounds() {
        assert_eq!(row_at(0.0, 22.0, 3), Some(0));
        assert_eq!(row_at(43.9, 22.0, 3), Some(1));
        assert_eq!(row_at(66.0, 22.0, 3), None);
        assert_eq!(row_at(-1.0, 22.0, 3), None);
        assert_eq!(row_at(f32::NAN, 22.0, 3), None);
        assert_eq!(row_at(10.0, 0.0, 3), None);
    }

    #[test]
    fn main_layout_table_width_clamped() {
        let m = Metrics::default();
        let avail = Rect::from_min_size(pos2(10.0, 20.0), vec2(1000.0, 600.0));
        let min = f32::from(TABLE_WIDTH_MIN);

        // Largeur normale : respectée.
        let l = main_layout(avail, 642.0, &m);
        assert_eq!(l.table_header.width(), 642.0);
        assert_eq!(l.table_clip.width(), 642.0);
        assert_eq!(l.splitter.left(), 652.0);
        assert_eq!(l.splitter.width(), m.splitter_w);
        assert_eq!(l.gantt_clip.left(), l.splitter.right());
        assert_eq!(l.gantt_header.left(), l.gantt_clip.left());
        // Le Gantt s'arrête avant la barre verticale pleine ; le corps non.
        assert_eq!(l.gantt_header.right(), avail.right() - m.vbar_w);
        assert_eq!(l.gantt_clip.right(), avail.right() - m.vbar_w);
        assert_eq!(l.gantt_hbar.right(), avail.right() - m.vbar_w);
        assert_eq!(l.body.right(), avail.right());
        assert!(m.vbar_w > 0.0);
        // En-têtes sur `header_h`, corps entre l'en-tête et la barre.
        assert_eq!(l.table_header.height(), m.header_h);
        assert_eq!(l.gantt_header.height(), m.header_h);
        assert_eq!(l.body.top(), avail.top() + m.header_h);
        assert_eq!(l.body.bottom(), avail.bottom() - m.hbar_h);
        assert_eq!(l.gantt_hbar.height(), m.hbar_h);
        assert_eq!(l.gantt_hbar.left(), l.gantt_clip.left());
        assert_eq!(l.table_clip.top(), l.gantt_clip.top());
        assert_eq!(l.table_clip.bottom(), l.gantt_clip.bottom());

        // Trop large : le Gantt garde GANTT_MIN_WIDTH.
        let l = main_layout(avail, 5_000.0, &m);
        assert_eq!(l.table_header.width(), 1000.0 - GANTT_MIN_WIDTH);

        // Trop étroite, négative, NaN : minimum.
        for w in [0.0, -50.0, f32::NAN, f32::NEG_INFINITY] {
            let l = main_layout(avail, w, &m);
            assert_eq!(l.table_header.width(), min, "{w}");
        }

        // Zone plus étroite que le minimum : la table prend la place
        // disponible, aucun rectangle n'est inversé.
        let narrow = Rect::from_min_size(pos2(0.0, 0.0), vec2(80.0, 30.0));
        let l = main_layout(narrow, 642.0, &m);
        for r in [
            l.table_header,
            l.gantt_header,
            l.splitter,
            l.body,
            l.table_clip,
            l.gantt_clip,
            l.gantt_hbar,
        ] {
            assert!(r.min.x <= r.max.x && r.min.y <= r.max.y, "{r:?}");
            assert!(r.is_finite(), "{r:?}");
        }
        assert_eq!(l.splitter.right(), 80.0);

        // Zone non finie : rectangles finis.
        let l = main_layout(Rect::EVERYTHING, 642.0, &m);
        assert!(l.body.is_finite());
    }
}
