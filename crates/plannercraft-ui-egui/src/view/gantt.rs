//! Corps du Gantt : bandes, jours chômés, grille, barres, jalons,
//! récapitulatives, chemin critique, date d'état, aujourd'hui, liens et
//! textes autour des barres ; Gantt de suivi (ligne de base 0, bande
//! d'avancement) et infobulle `explain_date` d'une barre (J7, lot L7).
//!
//! Écran stylé (J8, lot L11, ADR-0016 D5) : le style effectif du document
//! (`ScreenStyle`) donne les couleurs, la densité, la forme et la hauteur de
//! chaque rôle de barre ; la géométrie vient des fonctions partagées avec le
//! SVG et le PDF (`plannercraft_render::geometry` : rôle, forme, ordonnées et
//! hauteurs en 1/16 px, choix du tracé des liens, placement des textes,
//! hachures), les abscisses des instants de la projection `f32` de
//! [`TimeAxis`] (même 1/16 px à l'arrondi près, P55). Le Gantt de suivi
//! garde la mise en page en tiers de J7 (H-UI-22).
//!
//! Ordre des couches : fond → bandes → jours chômés → grille → traits de
//! lignes → sélection → date d'état, aujourd'hui → liens → lignes de base →
//! barres → motifs → bandes d'avancement → textes. Aucune date n'est
//! calculée ici : début, fin, ligne de base et `Stop` viennent du moteur,
//! les textes de `plannercraft_automation::render_text` (règle 3).

use std::ops::Range;

use egui::{Color32, FontFamily, FontId, Id, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, pos2};
use plannercraft_automation::command::TaskRef;
use plannercraft_automation::command::params::ExplainParams;
use plannercraft_automation::explain::{DriverView, ExplainView, StepView};
use plannercraft_automation::gesture::GestureKind;
use plannercraft_automation::json::SideJson;
use plannercraft_automation::render_text::SessionTexts;
use plannercraft_core::{LinkType, TaskUid, Weekday};
use plannercraft_render::geometry::{
    self, BarGeom as Geom16, FontRef, FontWeight, InterMetrics, Rect16, ResolvedTexts, RowGeomInput,
};
use plannercraft_render::style::{
    BarRole, BarShape, ColorTokens, Pattern, PlanStyle, Rgb, RowBanding, Variant,
};

use super::{BarDrag, FrameCtx};
use crate::app::OpenDocument;
use crate::column::Column;
use crate::document::project_calendar;
use crate::edit::text_language;
use crate::geom::{
    BarZone, GanttHit, bar_zone, f32_to_bounded_i64, i64_to_f32, index_to_f32, near_polyline, row_at,
};
use crate::i18n::{Arg, TextId, Texts, error_text_id};
use crate::query::{Query, QueryValue};
use crate::row_view::{DisplayRow, VisibleRows};
use crate::rows::{Row, RowKind, RowModel};
use crate::state::GanttStyle;
use crate::theme::{PLAN_FAMILY, PLAN_FAMILY_STRONG};
use crate::timescale::{TierUnit, TimeAxis, TimeScaleZoom, tier_spans};

/// Extrémités écran d'une barre (ou d'un jalon : `start_x == finish_x`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarEnds {
    /// Abscisse du début.
    pub start_x: f32,
    /// Abscisse de la fin.
    pub finish_x: f32,
    /// Ordonnée du centre vertical de la ligne.
    pub y: f32,
}

/// Mesures du tracé des liens, en points logiques.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinkMetrics {
    /// Longueur du segment horizontal qui sort d'une barre.
    pub stub: f32,
    /// Taille de la pointe de flèche.
    pub arrow: f32,
    /// Hauteur d'une ligne.
    pub row_h: f32,
    /// Demi-hauteur d'une barre.
    pub bar_half_h: f32,
}

impl Default for LinkMetrics {
    fn default() -> Self {
        Self {
            stub: 6.0,
            arrow: 4.0,
            row_h: super::ROW_H,
            bar_half_h: 5.0,
        }
    }
}

/// Direction de la pointe de flèche (vers la barre du successeur).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ArrowDir {
    /// Vers la droite (entrée par le début).
    #[default]
    Right,
    /// Vers la gauche (entrée par la fin).
    Left,
    /// Vers le bas (entrée par le dessus).
    Down,
    /// Vers le haut (entrée par le dessous).
    Up,
}

/// Tracé d'un lien : polyligne d'au plus 6 points et pointe de flèche.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinkPath {
    /// Points de la polyligne ; seuls les `len` premiers comptent.
    pub points: [Pos2; 6],
    /// Nombre de points utiles (0 à 6).
    pub len: usize,
    /// Pointe de la flèche.
    pub arrow_tip: Pos2,
    /// Direction de la flèche.
    pub arrow_dir: ArrowDir,
}

impl LinkPath {
    /// Points utiles de la polyligne.
    #[must_use]
    pub fn points(&self) -> &[Pos2] {
        self.points.get(..self.len).unwrap_or(&[])
    }
}

/// Tracé vide (entrée non finie) : aucun point, flèche sans effet.
fn empty_path(tip: Pos2) -> LinkPath {
    LinkPath {
        points: [tip; 6],
        len: 0,
        arrow_tip: tip,
        arrow_dir: ArrowDir::Right,
    }
}

/// Construit un tracé à partir de ses points (au plus 6), la flèche étant au
/// dernier point.
fn path_of(points: &[Pos2], dir: ArrowDir) -> LinkPath {
    let tip = points.last().copied().unwrap_or(Pos2::ZERO);
    let mut out = [tip; 6];
    for (slot, p) in out.iter_mut().zip(points) {
        *slot = *p;
    }
    let len = points.len().min(out.len());
    LinkPath {
        points: out,
        len,
        arrow_tip: tip,
        arrow_dir: dir,
    }
}

/// Tracé d'un lien de type `kind` du prédécesseur `p` au successeur `s`
/// (cas direct et cas replié quand le successeur commence avant).
///
/// Polyligne orthogonale (hypothèse d'affichage H-UI, d'après les captures
/// de l'aide Microsoft, non vérifiée au pixel) :
/// - FS : part de la fin de `p` ; direct (successeur assez à droite) :
///   horizontale puis verticale qui entre par le dessus (ou le dessous si le
///   successeur est au-dessus) près du début de `s` ; replié : sortie `stub`,
///   passage dans l'intercolonne sous `p`, retour à gauche de `s`, entrée par
///   le début ;
/// - SS : du début de `p` vers la gauche, verticale, entrée par le début de
///   `s` ;
/// - FF : de la fin de `p` vers la droite, verticale, entrée par la fin de
///   `s` (flèche vers la gauche) ;
/// - SF : du début de `p` vers la gauche ; direct si `s` finit assez à
///   gauche, sinon replié dans l'intercolonne ; entrée par la fin de `s`.
///
/// Un jalon se passe en donnant les extrémités de son losange. Une entrée
/// non finie donne un tracé vide.
#[must_use]
pub fn link_path(kind: LinkType, p: BarEnds, s: BarEnds, m: &LinkMetrics) -> LinkPath {
    link_path_with(kind, p, s, m, None)
}

/// [`link_path`] dont le choix direct / replié (FS et SF) est imposé par
/// `folded` (choix de la géométrie partagée, J8 lot L11) ; `None` : choix
/// sur les abscisses écran, comme J7.
#[must_use]
pub fn link_path_with(
    kind: LinkType,
    p: BarEnds,
    s: BarEnds,
    m: &LinkMetrics,
    folded: Option<bool>,
) -> LinkPath {
    let values = [
        p.start_x,
        p.finish_x,
        p.y,
        s.start_x,
        s.finish_x,
        s.y,
        m.stub,
        m.row_h,
        m.bar_half_h,
    ];
    if values.iter().any(|v| !v.is_finite()) {
        return empty_path(pos2(0.0, 0.0));
    }
    let (p0, p1, yp) = (p.start_x, p.finish_x.max(p.start_x), p.y);
    let (s0, s1, ys) = (s.start_x, s.finish_x.max(s.start_x), s.y);
    let stub = m.stub.max(0.0);
    let below = ys >= yp;
    let dir = if below { 1.0 } else { -1.0 };
    // Frontière de ligne côté successeur, juste sous (ou sur) le prédécesseur.
    let ymid = yp + dir * m.row_h / 2.0;
    match kind {
        LinkType::FinishToStart => {
            let xa = s0 + stub.min((s1 - s0) / 2.0);
            if folded.map_or(xa >= p1 + stub / 2.0, |f| !f) {
                let edge = ys - dir * m.bar_half_h;
                let arrow = if below { ArrowDir::Down } else { ArrowDir::Up };
                path_of(&[pos2(p1, yp), pos2(xa, yp), pos2(xa, edge)], arrow)
            } else {
                let xo = p1 + stub;
                let xi = s0 - stub;
                path_of(
                    &[
                        pos2(p1, yp),
                        pos2(xo, yp),
                        pos2(xo, ymid),
                        pos2(xi, ymid),
                        pos2(xi, ys),
                        pos2(s0, ys),
                    ],
                    ArrowDir::Right,
                )
            }
        }
        LinkType::StartToStart => {
            let xl = p0.min(s0) - stub;
            path_of(
                &[pos2(p0, yp), pos2(xl, yp), pos2(xl, ys), pos2(s0, ys)],
                ArrowDir::Right,
            )
        }
        LinkType::FinishToFinish => {
            let xr = p1.max(s1) + stub;
            path_of(
                &[pos2(p1, yp), pos2(xr, yp), pos2(xr, ys), pos2(s1, ys)],
                ArrowDir::Left,
            )
        }
        LinkType::StartToFinish => {
            let xl = p0 - stub;
            if folded.map_or(s1 + stub <= xl, |f| !f) {
                path_of(
                    &[pos2(p0, yp), pos2(xl, yp), pos2(xl, ys), pos2(s1, ys)],
                    ArrowDir::Left,
                )
            } else {
                let xr = s1 + stub;
                path_of(
                    &[
                        pos2(p0, yp),
                        pos2(xl, yp),
                        pos2(xl, ymid),
                        pos2(xr, ymid),
                        pos2(xr, ys),
                        pos2(s1, ys),
                    ],
                    ArrowDir::Left,
                )
            }
        }
    }
}

/// Triangle de la pointe de flèche : pointe en `tip`, orientée selon `dir`,
/// de longueur `size` et de demi-largeur `0,8 × size`.
#[must_use]
pub fn arrow_points(tip: Pos2, dir: ArrowDir, size: f32) -> [Pos2; 3] {
    let half = size * 0.8;
    match dir {
        ArrowDir::Right => [
            tip,
            pos2(tip.x - size, tip.y + half),
            pos2(tip.x - size, tip.y - half),
        ],
        ArrowDir::Left => [
            tip,
            pos2(tip.x + size, tip.y - half),
            pos2(tip.x + size, tip.y + half),
        ],
        ArrowDir::Down => [
            tip,
            pos2(tip.x - half, tip.y - size),
            pos2(tip.x + half, tip.y - size),
        ],
        ArrowDir::Up => [
            tip,
            pos2(tip.x + half, tip.y + size),
            pos2(tip.x - half, tip.y + size),
        ],
    }
}

/// Marge horizontale de culling des barres et des liens.
const CULL_MARGIN: f32 = 16.0;

/// Bandes verticales d'une ligne du Gantt de suivi (H-UI-22), en
/// ordonnées écran `(haut, bas)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackingLayout {
    /// Barre courante : au-dessus du tiers inférieur.
    pub current: (f32, f32),
    /// Barre de la ligne de base 0 : dans le tiers inférieur de la ligne.
    pub baseline: (f32, f32),
}

/// Mise en page d'une ligne de suivi de hauteur `row_h` dont le haut est à
/// `row_top` (hypothèse H-UI-22) : barre courante de `row_top + 3` à
/// 1 point au-dessus du tiers inférieur, ligne de base de 1 point sous le
/// début du tiers inférieur à 2 points du bas. Entrée non finie ou ligne
/// trop basse : bandes vides (haut = bas), jamais de NaN.
#[must_use]
pub fn tracking_layout(row_top: f32, row_h: f32) -> TrackingLayout {
    if !row_top.is_finite() || !row_h.is_finite() || row_h <= 0.0 {
        let top = if row_top.is_finite() { row_top } else { 0.0 };
        return TrackingLayout {
            current: (top, top),
            baseline: (top, top),
        };
    }
    let lower = row_top + 2.0 * row_h / 3.0;
    let band = |top: f32, bottom: f32| {
        let top = top.min(row_top + row_h);
        (top, bottom.max(top))
    };
    TrackingLayout {
        current: band(row_top + 3.0, lower - 1.0),
        baseline: band(lower + 1.0, row_top + row_h - 2.0),
    }
}

/// Abscisses `(gauche, droite)` de la bande d'avancement d'une barre
/// `[x0, x1]` dont la partie réelle s'arrête à `stop_x` (H-UI-22 :
/// [début, `Stop`], bornée à la barre) ; `None` si elle est vide ou si une
/// entrée n'est pas finie.
#[must_use]
pub fn progress_span(x0: f32, x1: f32, stop_x: f32) -> Option<(f32, f32)> {
    if !(x0.is_finite() && x1.is_finite() && stop_x.is_finite()) {
        return None;
    }
    let right = stop_x.min(x1);
    (right > x0).then_some((x0, right))
}

/// Débord admis d'une barre hors de la plage avant d'être rognée (64 px,
/// comme la géométrie partagée).
const OVERFLOW_PX: f32 = 64.0;

/// Couleur egui d'une couleur du style.
const fn color(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

/// Contexte de géométrie du Gantt à une image (J8, lot L11) : style
/// effectif, variante, mode suivi, surbrillance du chemin critique.
#[derive(Clone, Copy, Debug)]
pub struct GanttGeo<'a> {
    /// Style effectif du document.
    pub style: &'a PlanStyle,
    /// Variante (thème résolu).
    pub variant: Variant,
    /// Gantt de suivi (H-UI-22).
    pub tracking: bool,
    /// Surbrillance du chemin critique (H-UI-38) : sans elle, barres et
    /// liens critiques prennent le style des non critiques.
    pub highlight_critical: bool,
    /// Interface adoucie (lot U1, H-UI-170) : récapitulative en barre fine
    /// à embouts discrets, avancement dans la teinte de la barre.
    pub soft: bool,
}

impl<'a> GanttGeo<'a> {
    /// Contexte de l'image `fc`, sur le style `style` (copie du style de
    /// `fc`, pour laisser `fc` modifiable pendant le dessin).
    #[must_use]
    pub fn new(fc: &FrameCtx<'_>, style: &'a PlanStyle) -> Self {
        Self {
            style,
            variant: fc.task_view.style.variant,
            tracking: fc.state.gantt_style == GanttStyle::Tracking,
            highlight_critical: fc.task_view.state.highlight_critical,
            soft: fc.task_view.style.soft,
        }
    }

    /// Hauteur d'une ligne en 1/16 px (densité du style).
    fn row_h16(&self) -> i64 {
        geometry::row_h16(self.style)
    }

    /// Hauteur d'une ligne (densité du style : 18, 22 ou 28 px).
    #[must_use]
    pub fn row_h(&self) -> f32 {
        i64_to_f32(self.row_h16()) / 16.0
    }

    /// Couleurs de la variante.
    fn tokens(&self) -> &'a ColorTokens {
        self.style.tokens(self.variant)
    }
}

/// Rôle de la barre d'une ligne (même ordre que
/// `plannercraft_render::geometry::bar_role`) : exclue ⇒ `Inactive` (H-UI-37) ;
/// récapitulative ; jalon ; manuelle ⇒ `Manual` (barre en contour,
/// H-UI-36) ; critique ; tâche. Sans surbrillance du critique (H-UI-38),
/// les rôles critiques deviennent leurs rôles ordinaires. Ligne vide :
/// aucun rôle.
#[must_use]
pub const fn row_role(row: &Row, highlight_critical: bool) -> Option<BarRole> {
    let critical = row.critical && highlight_critical;
    Some(match row.kind {
        RowKind::Null => return None,
        RowKind::Excluded => BarRole::Inactive,
        RowKind::Summary if critical => BarRole::SummaryCritical,
        RowKind::Summary => BarRole::Summary,
        RowKind::Milestone if critical => BarRole::MilestoneCritical,
        RowKind::Milestone => BarRole::Milestone,
        RowKind::Leaf if row.manual => BarRole::Manual,
        RowKind::Leaf if critical => BarRole::Critical,
        RowKind::Leaf => BarRole::Task,
    })
}

/// Entrée de la géométrie partagée pour une ligne : les valeurs du moteur
/// lues par `geometry::row_geom_input`, prises dans le modèle de lignes
/// (P55). Ligne de base et avancement seulement dans le Gantt de suivi (vue
/// standard : barres seules, comme J7).
#[must_use]
pub fn row_input(row: &Row, geo: &GanttGeo<'_>) -> Option<RowGeomInput> {
    let role = row_role(row, geo.highlight_critical)?;
    let tracking = geo.tracking;
    Some(RowGeomInput {
        uid: row.uid,
        role,
        start: row.bar.map(|(start, _)| start),
        finish: row.bar.map(|(_, finish)| finish),
        percent: if tracking {
            u16::try_from(row.percent.get().clamp(0, 10_000)).unwrap_or(0)
        } else {
            0
        },
        stop: if tracking { row.stop } else { None },
        baseline: if tracking { row.baseline } else { None },
    })
}

/// Barre d'une ligne à l'écran : géométrie partagée et sa projection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenBar {
    /// Géométrie partagée (1/16 px ; abscisses depuis le début de la
    /// plage, ordonnées depuis le haut de la ligne affichée 0).
    pub g: Geom16,
    /// Rectangle écran englobant (losange et crochets compris).
    pub rect: Rect,
    /// Centre vertical écran (attache des liens).
    pub yc: f32,
    /// Demi-hauteur de la barre (survol, découpe).
    pub half: f32,
    /// Demi-hauteur d'entrée d'une flèche par le dessus ou le dessous.
    pub link_half: f32,
    /// Nature de la ligne.
    pub kind: RowKind,
}

impl ScreenBar {
    /// Extrémités vues par les liens (losange pour un jalon).
    fn ends(&self) -> BarEnds {
        BarEnds {
            start_x: self.rect.left(),
            finish_x: self.rect.right(),
            y: self.yc,
        }
    }

    /// Hors de la découpe horizontale (marge comprise, comme J7).
    fn culled(&self, clip: Rect) -> bool {
        let d = if self.g.shape == BarShape::Diamond {
            0.0
        } else {
            self.half + 1.0
        };
        self.rect.right() + d < clip.left() - CULL_MARGIN || self.rect.left() - d > clip.right() + CULL_MARGIN
    }

    /// Zone de survol de l'infobulle et du clic : la barre (losange d'un
    /// jalon) et 2 points autour.
    fn hover_rect(&self) -> Rect {
        Rect::from_min_max(
            pos2(self.rect.left() - 2.0, self.yc - self.half - 2.0),
            pos2(self.rect.right() + 2.0, self.yc + self.half + 2.0),
        )
    }
}

/// Barre de la ligne affichée `i` (haut de la ligne 0 en `origin`).
///
/// Rôle, forme, ordonnées et hauteurs viennent de
/// `geometry::bar_geom` sur l'axe entier de `axis` ; les abscisses des
/// instants de la projection `f32` de `axis`, par les mêmes règles (rognage
/// à 64 px hors de la plage, largeur d'au moins 1 px, losange centré sur le
/// début). Gantt de suivi avec ligne de base : barre courante dans le haut
/// de la ligne, en tiers comme J7 (H-UI-22). `None` : ligne vide ou sans
/// dates, valeurs non finies.
#[must_use]
pub fn screen_bar(
    row: &Row,
    axis: &TimeAxis,
    origin: Pos2,
    i: usize,
    geo: &GanttGeo<'_>,
) -> Option<ScreenBar> {
    let input = row_input(row, geo)?;
    let start = input.start?;
    let row_h16 = geo.row_h16();
    let top16 = i64::try_from(i).ok()?.checked_mul(row_h16)?;
    let g = geometry::bar_geom(&input, axis.axis16(), geo.style, top16);
    let lo = axis.left() - OVERFLOW_PX;
    let hi = axis.left() + axis.content_width() + OVERFLOW_PX;
    let clamp_x = |x: f32| x.max(lo).min(hi);
    let to_y = |v: i64| origin.y + i64_to_f32(v) / 16.0;
    let x0 = clamp_x(axis.x(start));
    let x1 = clamp_x(axis.x(input.finish.unwrap_or(start)));
    let link_half = i64_to_f32(g.link_half16) / 16.0;
    let (yc, half) = if geo.tracking && row.baseline.is_some() {
        let row_top = origin.y + index_to_f32(i) * geo.row_h();
        let (a, b) = tracking_layout(row_top, geo.row_h()).current;
        (f32::midpoint(a, b), (b - a) / 2.0)
    } else {
        (to_y(g.yc16), link_half)
    };
    let rect = match (g.shape, geo.tracking && row.baseline.is_some()) {
        (BarShape::Diamond, false) => {
            let d = i64_to_f32(g.rect.w) / 32.0;
            Rect::from_min_max(pos2(x0 - d, yc - d), pos2(x0 + d, yc + d))
        }
        (BarShape::Diamond, true) => {
            let d = half + 1.0;
            Rect::from_min_max(pos2(x0 - d, yc - d), pos2(x0 + d, yc + d))
        }
        (BarShape::Bracket, true) => {
            Rect::from_min_max(pos2(x0, yc - half), pos2(x1.max(x0 + 1.0), yc + 0.6 * half))
        }
        (BarShape::Bar | BarShape::Line, true) => {
            Rect::from_min_max(pos2(x0, yc - half), pos2(x1.max(x0 + 1.0), yc + half))
        }
        (BarShape::Bar | BarShape::Line | BarShape::Bracket, false) => Rect::from_min_max(
            pos2(x0, to_y(g.rect.y)),
            pos2(x1.max(x0 + 1.0), to_y(g.rect.bottom())),
        ),
    };
    let finite = rect.is_finite() && yc.is_finite() && half.is_finite();
    finite.then_some(ScreenBar {
        g,
        rect,
        yc,
        half,
        link_half,
        kind: row.kind,
    })
}

/// Tracé écran d'un lien entre deux barres : choix du tracé (direct ou
/// replié) par la géométrie partagée, coordonnées par la projection de
/// l'écran (J7, H-R7). `None` si la géométrie n'a pas de tracé.
#[must_use]
pub fn screen_link(kind: LinkType, p: &ScreenBar, s: &ScreenBar, row_h: f32) -> Option<LinkPath> {
    let shared = geometry::link_path(&p.g, &s.g, kind);
    if shared.points.is_empty() {
        return None;
    }
    let folded = shared.points.len() >= 6;
    let m = LinkMetrics {
        row_h,
        bar_half_h: s.link_half,
        ..LinkMetrics::default()
    };
    Some(link_path_with(kind, p.ends(), s.ends(), &m, Some(folded)))
}

/// Parties d'une barre à crochets dans `r` : barre pleine des 5/12 de la
/// hauteur (2 px au moins), pointes de 5/8 de la hauteur (bornées à la
/// demi-largeur) jusqu'en bas (H-R14, comme la scène et J7).
fn bracket_parts(r: Rect) -> (Rect, [Pos2; 3], [Pos2; 3]) {
    let h = r.height();
    let thick = (h * 5.0 / 12.0).max(2.0);
    let w = (h * 5.0 / 8.0).min(r.width() / 2.0).max(1.0);
    let (x0, x1, top) = (r.left(), r.right(), r.top());
    let bar = Rect::from_min_max(pos2(x0, top), pos2(x1, top + thick));
    let left = [pos2(x0, top), pos2(x0 + w, top), pos2(x0, top + h)];
    let right = [pos2(x1 - w, top), pos2(x1, top), pos2(x1, top + h)];
    (bar, left, right)
}

/// Récapitulative de l'interface adoucie dans `r` (lot U1, H-UI-170) :
/// barre fine aux bouts arrondis (60 % de la hauteur, 2 px au moins) et deux
/// embouts discrets, triangles de la moitié de la hauteur en largeur qui
/// descendent jusqu'au bas de `r` (au lieu des pointes des 5/8 de la
/// hauteur).
fn soft_bracket_parts(r: Rect) -> (Rect, [[Pos2; 3]; 2]) {
    let h = r.height();
    let thick = (h * 0.6).max(2.0).min(h);
    let w = (h * 0.5).min(r.width() / 2.0).max(1.0);
    let (x0, x1, top, bottom) = (r.left(), r.right(), r.top(), r.bottom());
    let bar = Rect::from_min_max(pos2(x0, top), pos2(x1, top + thick));
    let y = top + thick / 2.0;
    let left = [pos2(x0, y), pos2(x0 + w, y), pos2(x0, bottom)];
    let right = [pos2(x1 - w, y), pos2(x1, y), pos2(x1, bottom)];
    (bar, [left, right])
}

/// Contour d'une barre à crochets : barre pleine et pointes, d'un seul
/// trait.
fn bracket_outline(r: Rect) -> Vec<Pos2> {
    let (bar, left, _) = bracket_parts(r);
    let (x0, x1, top, bottom) = (r.left(), r.right(), r.top(), r.bottom());
    let w = left[1].x - x0;
    let h = (bottom - top).max(f32::EPSILON);
    // Bord intérieur d'une pointe au bas de la barre pleine.
    let inner = w - w * (bar.bottom() - top) / h;
    vec![
        pos2(x0, top),
        pos2(x1, top),
        pos2(x1, bottom),
        pos2(x1 - inner, bar.bottom()),
        pos2(x0 + inner, bar.bottom()),
        pos2(x0, bottom),
    ]
}

/// Formes d'une barre (sans contrôle de découpe) : couleurs et formes du
/// rôle dans le style. Barre pleine : remplissage et trait intérieur de sa
/// couleur de contour (sa couleur de remplissage à défaut), au moins 1 px,
/// comme J7 ; motif « contour seul » : le trait seul (tâche manuelle,
/// inactive) ; losange : rempli, ou contour seul.
fn bar_shapes(b: &ScreenBar, geo: &GanttGeo<'_>, phase: Option<Rgb>, out: &mut Vec<Shape>) {
    let c = geo.tokens();
    let role = b.g.role;
    let st = geo.style.bars.get(&role).copied();
    let paint = c.bars.get(&role).copied();
    // Couleur de phase (H-UI-65) : remplissage seulement, forme et contour
    // du rôle gardés (comme la scène).
    let phase = phase.filter(|_| plannercraft_render::geometry::phase_painted(role));
    let fill_rgb = phase.unwrap_or_else(|| paint.map_or(c.text, |p| p.fill));
    let fill = color(fill_rgb);
    let outline = color(paint.and_then(|p| p.outline).unwrap_or(if phase.is_some() {
        c.text_strong
    } else {
        fill_rgb
    }));
    let pattern = st.map_or(Pattern::Solid, |s| s.pattern);
    let outline16 = st.map_or(0, |s| i64::from(s.outline16));
    let stroke = Stroke::new(i64_to_f32(outline16.max(16)) / 16.0, outline);
    let outline_only = pattern == Pattern::Outline;
    let r = b.rect;
    match b.g.shape {
        BarShape::Bar | BarShape::Line => {
            let radius = (i64_to_f32(st.map_or(0, |s| i64::from(s.radius16))) / 16.0).min(r.height() / 2.0);
            if !outline_only {
                out.push(Shape::rect_filled(r, radius, fill));
            }
            out.push(Shape::rect_stroke(r, radius, stroke, StrokeKind::Inside));
        }
        BarShape::Bracket if geo.soft && !outline_only => {
            let (bar, caps) = soft_bracket_parts(r);
            out.push(Shape::rect_filled(bar, bar.height() / 2.0, fill));
            for cap in caps {
                out.push(Shape::convex_polygon(cap.to_vec(), fill, Stroke::NONE));
            }
        }
        BarShape::Bracket => {
            if outline_only {
                out.push(Shape::closed_line(bracket_outline(r), stroke));
            } else {
                let (bar, left, right) = bracket_parts(r);
                out.push(Shape::rect_filled(bar, 0.0, fill));
                out.push(Shape::convex_polygon(left.to_vec(), fill, Stroke::NONE));
                out.push(Shape::convex_polygon(right.to_vec(), fill, Stroke::NONE));
                if outline16 > 0 {
                    out.push(Shape::closed_line(bracket_outline(r), stroke));
                }
            }
        }
        BarShape::Diamond => {
            let points = diamond(r.center().x, r.center().y, r.width() / 2.0);
            if outline_only {
                out.push(Shape::closed_line(points, stroke));
            } else {
                let edge = if outline16 > 0 { stroke } else { Stroke::NONE };
                out.push(Shape::convex_polygon(points, fill, edge));
            }
        }
    }
}

/// Seizièmes de pixel d'une longueur écran (bornée, jamais de panique).
fn to16(v: f32) -> i64 {
    f32_to_bounded_i64(v * 16.0, i64::from(i32::MAX))
}

/// Hachures ou points d'une barre au motif `Hatch` ou `Dots` (H-UI-44) :
/// segments de `geometry::pattern_segments` découpés dans la barre (dans
/// la barre pleine d'une récapitulative, dans le losange d'un jalon),
/// calculés sur la partie visible seulement (pas des hachures gardé : le
/// motif ne glisse pas au défilement). Trait de 1 px de la couleur de
/// contour du rôle (le fond à défaut), comme la scène.
fn hatch_shapes(b: &ScreenBar, geo: &GanttGeo<'_>, clip: Rect, out: &mut Vec<Shape>) {
    let role = b.g.role;
    let pattern = geo.style.bars.get(&role).map_or(Pattern::Solid, |s| s.pattern);
    if !matches!(pattern, Pattern::Hatch | Pattern::Dots) {
        return;
    }
    let c = geo.tokens();
    let ink = color(c.bars.get(&role).and_then(|p| p.outline).unwrap_or(c.bg));
    let mut area = b.rect;
    if b.g.shape == BarShape::Bracket {
        area = bracket_parts(area).0;
    }
    let diamond = b.g.shape == BarShape::Diamond;
    // Repère local : coin haut-gauche de la barre, en 1/16 px.
    let (ox, oy) = (area.left(), area.top());
    let step = 64_i64;
    let mut x0 = 0_i64;
    let mut x1 = to16(area.width());
    if !diamond {
        // Partie visible, bord gauche recalé sur un multiple du pas.
        let skip = to16(clip.left() - CULL_MARGIN - ox).max(0);
        x0 = skip.saturating_sub(skip.rem_euclid(step));
        x1 = x1.min(to16(clip.right() + CULL_MARGIN - ox));
    }
    if x1 <= x0 {
        return;
    }
    let r = Rect16 {
        x: x0,
        y: 0,
        w: x1.saturating_sub(x0),
        h: to16(area.height()),
    };
    let stroke = Stroke::new(1.0, ink);
    let at = |x: i64, y: i64| pos2(ox + i64_to_f32(x) / 16.0, oy + i64_to_f32(y) / 16.0);
    for (ax, ay, bx, by) in geometry::pattern_segments(r, pattern == Pattern::Dots, diamond) {
        out.push(Shape::line_segment([at(ax, ay), at(bx, by)], stroke));
    }
}

/// Contexte des textes autour des barres à une image.
struct TextCtx<'a> {
    /// Peintre du Gantt (mise en page des textes).
    painter: &'a egui::Painter,
    /// Source des textes (moteur, règle 3).
    src: &'a SessionTexts<'a>,
    /// Projection.
    axis: &'a TimeAxis,
    /// Découpe du Gantt.
    clip: Rect,
}

/// Textes autour d'une barre (champs du style pour son rôle) : valeurs de
/// la source du moteur (règle 3), placés par `geometry::place_texts` dans
/// la partie visible du Gantt (H-UI-46 : bascule, « … », jamais rognés),
/// mesurés en Inter (H-R11), dessinés dans la police du planning, centrés
/// verticalement sur la barre.
fn text_shapes(
    tc: &TextCtx<'_>,
    bar: &ScreenBar,
    uid: TaskUid,
    phase: Option<Rgb>,
    geo: &GanttGeo<'_>,
    out: &mut Vec<Shape>,
) {
    use plannercraft_render::geometry::BarTextSource as _;
    let Some(fields) = geo.style.texts.get(&bar.g.role) else {
        return;
    };
    let strong = matches!(bar.g.role, BarRole::Summary | BarRole::SummaryCritical);
    let font = FontRef {
        weight: if strong {
            FontWeight::SemiBold
        } else {
            FontWeight::Regular
        },
        size16: geo.style.font.size16,
        italic: false,
    };
    let field = |f: Option<plannercraft_render::style::BarField>| f.and_then(|f| tc.src.bar_text(uid, f));
    let texts = ResolvedTexts {
        left: field(fields.left),
        right: field(fields.right),
        inside: field(fields.inside),
        font,
    };
    if texts.left.is_none() && texts.right.is_none() && texts.inside.is_none() {
        return;
    }
    let base = tc.axis.left();
    let clip16 = (to16(tc.clip.left() - base), to16(tc.clip.right() - base));
    let mut drawn = bar.g;
    // Barre telle que dessinée (projection de l'écran) : les textes la
    // longent au même 1/16 px que la scène, à l'arrondi près.
    drawn.rect.x = to16(bar.rect.left() - base);
    drawn.rect.w = to16(bar.rect.width());
    let placed = geometry::place_texts(&drawn, &texts, &InterMetrics, clip16);
    let tokens = geo.tokens();
    let ink_rgb = if strong { tokens.text_strong } else { tokens.text };
    let ink = color(ink_rgb);
    // Dedans : encre la plus contrastée sur le remplissage de la barre
    // (H-UI-61, comme la scène `Class::TextOnBar`).
    let bar_fill = phase
        .filter(|_| plannercraft_render::geometry::phase_painted(bar.g.role))
        .or_else(|| tokens.bars.get(&bar.g.role).map(|p| p.fill))
        .unwrap_or(tokens.bg);
    let inside_ink = color(plannercraft_render::style::ink_on(bar_fill, ink_rgb, tokens.bg));
    let family = match geo.style.font.family {
        plannercraft_render::style::FontChoice::Inter if strong => {
            FontFamily::Name(PLAN_FAMILY_STRONG.into())
        }
        plannercraft_render::style::FontChoice::Inter => FontFamily::Name(PLAN_FAMILY.into()),
        plannercraft_render::style::FontChoice::Interface => FontFamily::Proportional,
    };
    let size = f32::from(font.size16) / 16.0;
    let bg = color(tokens.bg);
    for item in placed.items {
        let halo = item.side != plannercraft_render::geometry::TextSide::Inside;
        let item_ink = if halo { ink } else { inside_ink };
        let galley = tc
            .painter
            .layout_no_wrap(item.text, FontId::new(size, family.clone()), item_ink);
        let left = tc.axis.x_of16(item.x16);
        let top = bar.yc - galley.size().y / 2.0;
        if left.is_finite() && top.is_finite() {
            // H-UI-64 : fond sous l'étiquette, au-dessus des liens et de la
            // grille (même boîte que la scène, `label_halo`).
            if halo {
                let size = galley.size();
                let r = Rect::from_min_size(pos2(left - 1.0, top), egui::vec2(size.x + 2.0, size.y));
                out.push(Shape::rect_filled(r, 0.0, bg));
            }
            out.push(Shape::galley(pos2(left, top), galley, item_ink));
        }
    }
}

/// Barre de la ligne de base 0 d'une ligne dont le haut est à `top`
/// (H-UI-22) : rectangle plein dans le tiers inférieur, losange si début et
/// fin de base coïncident ; rien pour une ligne vide ou exclue, sans ligne
/// de base, hors de la découpe ou non finie.
fn baseline_shape(
    row: &Row,
    axis: &TimeAxis,
    top: f32,
    row_h: f32,
    clip: Rect,
    color: Color32,
) -> Option<Shape> {
    if matches!(row.kind, RowKind::Null | RowKind::Excluded) {
        return None;
    }
    let (start, finish) = row.baseline?;
    let (y0, y1) = tracking_layout(top, row_h).baseline;
    let x0 = axis.x(start);
    let point = finish <= start;
    let x1 = if point { x0 } else { axis.x(finish).max(x0 + 1.0) };
    let d = (y1 - y0) / 2.0 + 1.0;
    let finite = [x0, x1, y0, y1, d].iter().all(|v| v.is_finite());
    if !finite || x1 + d < clip.left() - CULL_MARGIN || x0 - d > clip.right() + CULL_MARGIN {
        return None;
    }
    Some(if point {
        Shape::convex_polygon(diamond(x0, f32::midpoint(y0, y1), d), color, Stroke::NONE)
    } else {
        Shape::rect_filled(Rect::from_min_max(pos2(x0, y0), pos2(x1, y1)), 0.0, color)
    })
}

/// Bande d'avancement d'une barre de tâche feuille (H-UI-22, Gantt de
/// suivi) : sur [début, `Stop`] (fin de la partie réelle, R-204), dans la
/// moitié centrale de la barre.
fn progress_shape(row: &Row, bar: &ScreenBar, axis: &TimeAxis, color: Color32) -> Option<Shape> {
    if bar.kind != RowKind::Leaf || bar.g.shape != BarShape::Bar {
        return None;
    }
    let (a, b) = progress_span(bar.rect.left(), bar.rect.right(), axis.x(row.stop?))?;
    let h = (bar.half / 2.0).max(1.0);
    let rect = Rect::from_min_max(pos2(a, bar.yc - h), pos2(b, bar.yc + h));
    rect.is_finite().then(|| Shape::rect_filled(rect, 0.0, color))
}

/// Sommets d'un losange centré en `(x, y)`, sens horaire.
fn diamond(x: f32, y: f32, d: f32) -> Vec<Pos2> {
    vec![pos2(x, y - d), pos2(x + d, y), pos2(x, y + d), pos2(x - d, y)]
}

/// Boîte englobante d'un tracé (pointe comprise) qui coupe la découpe.
fn path_visible(path: &LinkPath, clip: Rect) -> bool {
    let mut bbox = Rect::NOTHING;
    for p in path.points() {
        bbox.extend_with(*p);
    }
    bbox.is_finite() && bbox.expand(CULL_MARGIN).intersects(clip)
}

/// Indice de bande (0 ou 1) des lignes affichées `range` (H-UI-53) :
/// zébrage par rang affiché ; bandes de phase par récapitulative de niveau
/// 1 (une phase commence à chaque ligne de niveau 1 du modèle, comme la
/// scène) ; `None` sans bandes.
pub(crate) fn band_of(banding: RowBanding, rows: VisibleRows<'_>, i: usize, phases: &[u8]) -> Option<u8> {
    match banding {
        RowBanding::None => None,
        RowBanding::Zebra => Some(u8::from(i % 2 == 1)),
        RowBanding::Phase => phases.get(rows.model_row(DisplayRow::at(i))?.index()).copied(),
    }
}

/// Phase (0 ou 1) de chaque ligne du modèle : bascule à chaque ligne de
/// niveau 1 ; les lignes avant la première phase sont en phase 0.
#[must_use]
pub fn phases(model: &RowModel) -> Vec<u8> {
    let mut out = Vec::with_capacity(model.len());
    let mut phase: Option<u8> = None;
    for i in 0..model.len() {
        if model.row(i).is_some_and(|r| r.level == 1) {
            phase = Some(phase.map_or(0, |p| u8::from(p == 0)));
        }
        out.push(phase.unwrap_or(0));
    }
    out
}

/// Jours chômés du calendrier du projet grisés (N3), aux zooms Jour et
/// Semaine seulement : jours visibles consécutifs fusionnés, donc au plus un
/// rectangle par jour visible. Calendrier compilé une fois par génération
/// (`OpenDocument::calendars`) ; calendrier du projet en échec ou hors
/// budget : rien (le diagnostic du moteur est au panneau). Rend le nombre
/// de rectangles. À l'écran, le grisage suit J7 quel que soit
/// `scale.shade_non_working` (réglage du SVG et du PDF).
fn non_working_shapes(
    fc: &FrameCtx<'_>,
    axis: &TimeAxis,
    week_start: Weekday,
    clip: Rect,
    out: &mut Vec<Shape>,
) -> usize {
    let fine = matches!(fc.state.zoom, TimeScaleZoom::Day | TimeScaleZoom::Week);
    // Échelles larges (H-UI-62, J8 lot L15) : périodes chômées d'au moins
    // `LONG_OFF_DAYS` jours, si le style grise les jours chômés.
    let long_only = !fine && fc.task_view.style.style.scale.shade_non_working;
    if !fine && !long_only {
        return 0;
    }
    let Some(doc) = fc.doc else {
        return 0;
    };
    let set = doc.calendars();
    let Some(calendar) = project_calendar(&set, doc.project()) else {
        return 0;
    };
    let mut count = 0_usize;
    let mut push = |(x0, x1): (f32, f32), out: &mut Vec<Shape>| {
        let rect = Rect::from_min_max(
            pos2(x0.max(clip.left()), clip.top()),
            pos2(x1.min(clip.right()), clip.bottom()),
        );
        if rect.is_finite() && rect.width() > 0.0 {
            out.push(Shape::rect_filled(rect, 0.0, fc.palette.non_working));
            count = count.saturating_add(1);
        }
    };
    // Période en cours : abscisses et nombre de jours.
    let mut run: Option<(f32, f32, i64)> = None;
    let min_days = if long_only {
        plannercraft_render::geometry::LONG_OFF_DAYS
    } else {
        1
    };
    // Aux échelles larges, une période commencée avant la zone visible
    // compte ses jours depuis le début : la zone est élargie d'autant.
    let margin = if long_only { axis.px_per_day() * 40.0 } else { 0.0 };
    for (day, x0, x1) in tier_spans(
        axis,
        TierUnit::Day,
        week_start,
        (clip.left() - margin, clip.right() + margin),
    ) {
        let off = calendar.day_ranges(day).is_ok_and(<[_]>::is_empty);
        if off {
            run = Some(run.map_or((x0, x1, 1), |(a, _, n)| (a, x1, n.saturating_add(1))));
        } else if let Some((a, b, n)) = run.take()
            && n >= min_days
        {
            push((a, b), out);
        }
    }
    if let Some((a, b, n)) = run
        && n >= min_days
    {
        push((a, b), out);
    }
    count
}

/// Fond, bandes (H-UI-53), jours chômés, grille verticale aux frontières
/// des deux niveaux de l'échelle (selon `grid.major_v`, `grid.minor_v`),
/// traits de lignes (`grid.row_lines`), bande de sélection, ligne de la date
/// d'état et trait d'aujourd'hui (H-UI-54, `scale.today_mark`).
fn background_shapes(
    fc: &FrameCtx<'_>,
    geo: &GanttGeo<'_>,
    axis: &TimeAxis,
    origin: Pos2,
    range: &Range<usize>,
    clip: Rect,
    out: &mut Vec<Shape>,
) {
    let palette = fc.palette;
    let c = geo.tokens();
    let row_h = fc.metrics.row_h;
    out.push(Shape::rect_filled(clip, 0.0, palette.background));
    let row_rect = |i: usize| {
        let top = origin.y + index_to_f32(i) * row_h;
        Rect::from_min_max(pos2(clip.left(), top), pos2(clip.right(), top + row_h))
    };
    // H-UI-62 (J8 lot L15) : ni bandes ni traits de lignes dans la zone
    // graphique (table seulement) ; quadrillage vertical seul.
    let _ = c;
    let week_start = fc
        .doc
        .map_or(Weekday::Monday, |doc| doc.project().options.week_start_day);
    non_working_shapes(fc, axis, week_start, clip, out);
    let (major, minor) = fc.state.zoom.tiers();
    let grid = geo.style.grid;
    for (unit, color, show) in [
        (minor, palette.grid, grid.minor_v),
        (major, palette.grid_major, grid.major_v),
    ] {
        if !show {
            continue;
        }
        let stroke = Stroke::new(1.0, color);
        for (_, x0, _) in tier_spans(axis, unit, week_start, (clip.left(), clip.right())) {
            let x = x0.round() + 0.5;
            if x.is_finite() && x > clip.left() && x < clip.right() {
                out.push(Shape::line_segment(
                    [pos2(x, clip.top()), pos2(x, clip.bottom())],
                    stroke,
                ));
            }
        }
    }
    if let Some(rows) = fc.visible().filter(|_| !fc.selected.is_empty()) {
        for i in range.clone() {
            if rows
                .row(DisplayRow::at(i))
                .is_some_and(|r| fc.selected.contains(r.uid))
            {
                // Lignes entières comme lignes d'un bloc : la teinte
                // discrète de l'ID appuyé (H-UI-103), qui garde les barres
                // lisibles (≥ 3:1, `h_ui_103_selection_contrast`).
                let rect = row_rect(i);
                if rect.is_finite() {
                    out.push(Shape::rect_filled(rect, 0.0, palette.header_selected));
                }
            }
        }
    }
    if let Some(doc) = fc.doc {
        let status = doc.project().effective_status_date();
        let sx = axis.x(status).round() + 0.5;
        if sx.is_finite() && sx >= clip.left() && sx <= clip.right() {
            let line = [pos2(sx, clip.top()), pos2(sx, clip.bottom())];
            out.extend(Shape::dashed_line(
                &line,
                Stroke::new(1.5, palette.status_date),
                6.0,
                4.0,
            ));
        }
        // Aujourd'hui (H-UI-54) : trait pointillé distinct, sauf le même
        // jour que la date d'état ou à moins de 4 points de son trait
        // (H-UI-185, règle partagée avec l'impression).
        if geo.style.scale.today_mark {
            let today = doc.project().current_date;
            let tx = axis.x(today).round() + 0.5;
            let status_drawn = sx.is_finite() && sx >= clip.left() && sx <= clip.right();
            let gap16 = crate::geom::f32_to_bounded_i64(((tx - sx) * 16.0).round(), i64::from(i32::MAX));
            if plannercraft_render::geometry::today_line_shown(status, today, status_drawn, gap16)
                && tx.is_finite()
                && tx >= clip.left()
                && tx <= clip.right()
            {
                let line = [pos2(tx, clip.top()), pos2(tx, clip.bottom())];
                out.extend(Shape::dashed_line(
                    &line,
                    Stroke::new(1.0, color(c.today_line)),
                    2.0,
                    3.0,
                ));
            }
        }
    }
}

/// Repère du corps : projection, coin de la ligne 0, découpe, style.
#[derive(Clone, Copy)]
struct Body<'a> {
    /// Projection temps → pixels.
    axis: &'a TimeAxis,
    /// Coin haut-gauche écran de la ligne 0.
    origin: Pos2,
    /// Découpe du corps.
    clip: Rect,
    /// Style et mode.
    geo: GanttGeo<'a>,
}

impl Body<'_> {
    /// Barre de la ligne affichée `i`.
    fn bar(&self, rows: VisibleRows<'_>, i: usize) -> Option<ScreenBar> {
        rows.row(DisplayRow::at(i))
            .and_then(|r| screen_bar(r, self.axis, self.origin, i, &self.geo))
    }
}

/// Liens qui touchent les lignes `range` (intersection d'intervalles) ;
/// rend le nombre de liens dessinés. Lien critique en couleur critique
/// seulement avec la surbrillance (H-UI-38).
fn link_shapes(rows: VisibleRows<'_>, body: &Body<'_>, range: &Range<usize>, out: &mut Vec<Shape>) -> usize {
    let c = body.geo.tokens();
    let row_h = body.geo.row_h();
    let arrow = LinkMetrics::default().arrow;
    let mut painted = 0_usize;
    for (shown, link) in rows.links_touching(range.clone()) {
        let (from, to) = (shown.from.index(), shown.to.index());
        let (Some(p), Some(s)) = (body.bar(rows, from), body.bar(rows, to)) else {
            continue;
        };
        let Some(path) = screen_link(link.kind, &p, &s, row_h) else {
            continue;
        };
        if path.len < 2 || !path_visible(&path, body.clip) {
            continue;
        }
        let ink = color(if link.critical && body.geo.highlight_critical {
            c.link_critical
        } else {
            c.link
        });
        out.push(Shape::line(path.points().to_vec(), Stroke::new(1.0, ink)));
        let tri = arrow_points(path.arrow_tip, path.arrow_dir, arrow);
        out.push(Shape::convex_polygon(tri.to_vec(), ink, Stroke::NONE));
        painted = painted.saturating_add(1);
    }
    painted
}

/// Clé du cache de l'infobulle : (ouverture, génération, UID, côté).
type TipKey = (u64, u64, TaskUid, SideJson);

/// Cache d'une entrée de l'infobulle `explain_date` (H-UI-26), gardé dans
/// la mémoire temporaire d'egui : la lecture n'est refaite que si le
/// document, sa génération, la tâche ou le côté changent.
#[derive(Clone, Debug, Default)]
pub struct ExplainTipCache<T = BarTip> {
    /// Clé du contenu gardé.
    key: Option<TipKey>,
    /// Contenu gardé (infobulle construite, H-UI-186).
    text: T,
    /// Lectures faites (tests).
    runs: u64,
}

impl<T: Clone> ExplainTipCache<T> {
    /// Contenu de `key`, lu par `run` seulement si la clé change.
    fn text_for(&mut self, key: TipKey, run: impl FnOnce() -> T) -> T {
        if self.key != Some(key) {
            self.text = run();
            self.key = Some(key);
            self.runs = self.runs.saturating_add(1);
        }
        self.text.clone()
    }
}

/// Identifiant du cache de l'infobulle dans la mémoire d'egui.
fn tip_cache_id() -> Id {
    Id::unique("plannercraft.gantt.explain_tip")
}

/// Largeur maximale de l'infobulle d'une barre, en points (H-UI-186).
pub const BAR_TIP_MAX_WIDTH: f32 = 360.0;

/// Infobulle d'une barre (H-UI-26, H-UI-186) : textes déjà mis en forme,
/// rien n'est calculé à l'affichage.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BarTip {
    /// Nom de la tâche (cellule Nom).
    pub name: String,
    /// Lignes libellé / valeur (cellules Début, Fin, Durée, % achevé ;
    /// une seule date pour un jalon).
    pub fields: Vec<(String, String)>,
    /// « Pourquoi cette date », réduit au pilote direct : zéro, une ou deux
    /// lignes (ou le refus du moteur traduit).
    pub why: Vec<String>,
}

/// Texte d'une cellule de la tâche `uid` (vide si elle n'est pas dans le
/// modèle).
fn cell(rows: &RowModel, uid: TaskUid, column: Column) -> &str {
    rows.index_of(uid).map_or("", |i| rows.text(i, column))
}

/// Ligne « pourquoi » du pilote direct `driver` (H-UI-186), construite sur
/// les champs structurés de la vue d'`explain_date` et les cellules du
/// tableau ; `None` quand il n'y a rien d'utile à dire (agrégat d'une
/// récapitulative).
fn driver_line(
    driver: &DriverView,
    doc: &OpenDocument,
    rows: &RowModel,
    texts: &Texts,
    uid: TaskUid,
) -> Option<String> {
    use plannercraft_automation::text::DisplayLabels as _;

    let line = match driver {
        DriverView::ProjectStart => texts.get(TextId::BarTipProjectStart),
        DriverView::SummaryGate { summary } => texts.get_args(
            TextId::BarTipSummaryGate,
            &[(
                "name",
                Arg::Str(cell(rows, TaskUid::new(*summary), Column::Name).into()),
            )],
        ),
        DriverView::Link { predecessor, link } => {
            let pred = TaskUid::new(*predecessor);
            let kind = LinkType::from(*link);
            // FD et FF : la fin du prédécesseur ; DD et DF : son début.
            let (id, column) = match kind {
                LinkType::FinishToStart | LinkType::FinishToFinish => {
                    (TextId::BarTipAfterFinish, Column::Finish)
                }
                LinkType::StartToStart | LinkType::StartToFinish => (TextId::BarTipAfterStart, Column::Start),
            };
            texts.get_args(
                id,
                &[
                    ("name", Arg::Str(cell(rows, pred, Column::Name).into())),
                    ("link", Arg::Str(texts.format_labels().link_code(kind).into())),
                    ("date", Arg::Str(cell(rows, pred, column).into())),
                ],
            )
        }
        DriverView::Manual => texts.get(TextId::BarTipManual),
        DriverView::Rollup => return None,
        DriverView::Constraint { .. } => {
            // Cellules Type et Date de contrainte, mises en forme comme dans
            // le tableau même si ces colonnes ne sont pas affichées.
            let fields = crate::field::field_labels(texts);
            let text = |column| rows.cell_text(doc, texts.format_labels(), &fields, uid, column);
            let (kind, date) = (text(Column::ConstraintType), text(Column::ConstraintDate));
            let what = if date.is_empty() {
                kind
            } else {
                format!("{kind} {date}")
            };
            texts.get_args(TextId::BarTipConstraint, &[("what", Arg::Str(what.into()))])
        }
        DriverView::Actual => texts.get(TextId::BarTipActual),
    };
    Some(line)
}

/// Lignes « pourquoi » d'une explication (H-UI-186) : le pilote direct,
/// puis le calendrier de la tâche s'il a déplacé la date (calendrier autre
/// que celui du projet). Jamais la chaîne des prédécesseurs.
fn why_lines(
    view: &ExplainView,
    doc: &OpenDocument,
    rows: &RowModel,
    texts: &Texts,
    uid: TaskUid,
) -> Vec<String> {
    let mut lines = Vec::new();
    let driver = view.steps.iter().find_map(|step| match step {
        StepView::Driver { driver, .. } => Some(driver),
        _ => None,
    });
    if let Some(line) = driver.and_then(|d| driver_line(d, doc, rows, texts, uid)) {
        lines.push(line);
    }
    if let Some(name) = moved_by_calendar(&view.steps, doc.project().calendar.get()) {
        lines.push(texts.get_args(TextId::BarTipCalendar, &[("name", Arg::Str(name.into()))]));
    }
    lines
}

/// Calendrier propre à la tâche qui a déplacé sa date (H-UI-186) : étape
/// de recalage dont l'instant a bougé, sur un calendrier nommé autre que
/// celui du projet (`project_calendar`, UID), et pilote qui se recale
/// (ni agrégat, ni dates saisies ou réelles). Le recalage ordinaire sur le
/// calendrier du projet (fin à 17:00, reprise le lendemain à 08:00) n'est
/// pas une explication utile.
fn moved_by_calendar(steps: &[StepView], project_calendar: u32) -> Option<&str> {
    let snaps = steps.iter().any(|step| {
        matches!(
            step,
            StepView::Driver {
                driver: DriverView::ProjectStart
                    | DriverView::SummaryGate { .. }
                    | DriverView::Link { .. }
                    | DriverView::Constraint { .. },
                ..
            }
        )
    });
    if !snaps {
        return None;
    }
    steps.iter().find_map(|step| match step {
        StepView::Snap {
            raw,
            snapped,
            calendar: Some(calendar),
        } if raw != snapped && calendar.uid != project_calendar && !calendar.name.is_empty() => {
            Some(calendar.name.as_str())
        }
        _ => None,
    })
}

/// Infobulle de la barre de la tâche `uid` (H-UI-26, H-UI-186) : nom,
/// grille des cellules du tableau (Début, Fin, Durée, % achevé ; Date et
/// % achevé pour un jalon), puis le pilote direct du début lu par la
/// lecture typée `Query::Explain` (même fonction qu'`analysis.explain_date`,
/// profondeur 1). Refus du moteur traduit ; aucune date ni durée calculée
/// ou rédigée ici (règle 3).
#[must_use]
pub fn bar_tip(doc: &OpenDocument, rows: &RowModel, texts: &Texts, uid: TaskUid) -> BarTip {
    let milestone = rows
        .index_of(uid)
        .and_then(|i| rows.row(i))
        .is_some_and(|r| r.kind == RowKind::Milestone);
    let field = |label: TextId, column: Column| (texts.get(label), cell(rows, uid, column).to_owned());
    let fields = if milestone {
        vec![
            field(TextId::BarTipDate, Column::Start),
            field(TextId::ColPercentComplete, Column::PercentComplete),
        ]
    } else {
        vec![
            field(TextId::ColStart, Column::Start),
            field(TextId::ColFinish, Column::Finish),
            field(TextId::ColDuration, Column::Duration),
            field(TextId::ColPercentComplete, Column::PercentComplete),
        ]
    };
    let query = Query::Explain(ExplainParams {
        task: TaskRef::Uid(uid),
        side: Some(SideJson::Start),
        depth: Some(1),
    });
    let why = match query.run(&doc.session) {
        Ok(QueryValue::Explain(view)) => why_lines(&view, doc, rows, texts, uid),
        Ok(QueryValue::Usage(_) | QueryValue::Variance(_)) => Vec::new(),
        Err(error) => {
            let detail = error.message(text_language(texts.language()));
            vec![texts.get_args(error_text_id(error.code), &[("detail", Arg::Str(detail.into()))])]
        }
    };
    BarTip {
        name: cell(rows, uid, Column::Name).to_owned(),
        fields,
        why,
    }
}

/// Dessine l'infobulle `tip` (H-UI-186) : nom en gras, grille libellé
/// (texte atténué) / valeur, filet, « Pourquoi cette date » et ses lignes.
fn show_bar_tip(ui: &mut egui::Ui, texts: &Texts, tip: &BarTip) {
    ui.set_max_width(BAR_TIP_MAX_WIDTH);
    ui.label(egui::RichText::new(&tip.name).strong());
    egui::Grid::new("plannercraft.gantt.bar_tip.grid")
        .num_columns(2)
        .spacing(egui::vec2(12.0, 2.0))
        .show(ui, |ui| {
            for (label, value) in &tip.fields {
                ui.label(egui::RichText::new(label).weak());
                ui.label(value);
                ui.end_row();
            }
        });
    if !tip.why.is_empty() {
        ui.separator();
        ui.label(egui::RichText::new(texts.get(TextId::BarTipWhy)).weak());
        for line in &tip.why {
            ui.label(line);
        }
    }
}

/// Infobulle de la barre sous le pointeur (H-UI-26), après le délai
/// d'egui : une zone de survol (sans clic ni glisser, le fond garde
/// sélection et défilement) posée seulement sur la barre survolée.
fn bar_tooltip(
    ui: &mut egui::Ui,
    fc: &FrameCtx<'_>,
    rows: VisibleRows<'_>,
    body: &Body<'_>,
    range: &Range<usize>,
) {
    let (Some(doc), Some(model)) = (fc.doc, fc.rows) else {
        return;
    };
    let Some(p) = ui.ctx().pointer_hover_pos() else {
        return;
    };
    if !ui.rect_contains_pointer(body.clip) {
        return;
    }
    let Some(i) = row_at(p.y - body.origin.y, body.geo.row_h(), rows.len()).filter(|i| range.contains(i))
    else {
        return;
    };
    let (Some(row), Some(bar)) = (rows.row(DisplayRow::at(i)), body.bar(rows, i)) else {
        return;
    };
    let hover = bar.hover_rect();
    if !hover.contains(p) {
        return;
    }
    let uid = row.uid;
    let response = ui.interact(
        hover.intersect(body.clip),
        ui.make_persistent_id(("plannercraft.gantt.bar_tip", uid)),
        Sense::hover(),
    );
    response.on_hover_ui(|ui| {
        let (opening, generation) = doc.cache_key();
        let key = (opening, generation, uid, SideJson::Start);
        let mut cache = ui
            .ctx()
            .data(|d| d.get_temp::<ExplainTipCache>(tip_cache_id()))
            .unwrap_or_default();
        let tip = cache.text_for(key, || bar_tip(doc, model, fc.texts, uid));
        ui.ctx().data_mut(|d| d.insert_temp(tip_cache_id(), cache));
        show_bar_tip(ui, fc.texts, &tip);
    });
}

/// Lectures faites par le cache de l'infobulle (tests de H-UI-26).
#[must_use]
pub fn explain_tip_runs(ctx: &egui::Context) -> u64 {
    ctx.data(|d| d.get_temp::<ExplainTipCache>(tip_cache_id()))
        .map_or(0, |cache| cache.runs)
}

/// Corps du Gantt : lignes `range`, `origin` étant le coin haut-gauche écran
/// de la ligne 0, découpe `clip`, projection `axis`.
///
/// Couches : fond → bandes → jours chômés → grille → traits de lignes →
/// sélection → date d'état, aujourd'hui → liens → lignes de base (suivi) →
/// barres → motifs → bandes d'avancement (suivi) → textes. Seules les lignes
/// de `range` sont parcourues (liens : ceux dont l'intervalle de lignes
/// coupe `range`) ; une barre hors de la découpe horizontale n'est pas
/// dessinée. Infobulle `explain_date` sur la barre survolée.
pub fn paint_body(
    ui: &mut egui::Ui,
    fc: &mut FrameCtx<'_>,
    axis: &TimeAxis,
    origin: Pos2,
    range: Range<usize>,
    clip: Rect,
) {
    if !clip.is_finite() || !origin.is_finite() || clip.width() <= 0.0 || clip.height() <= 0.0 {
        return;
    }
    let painter = ui.painter_at(clip);
    let screen = fc.task_view.style.clone();
    let geo = GanttGeo::new(fc, &screen.style);
    let c = geo.tokens();
    let mut shapes = Vec::new();
    background_shapes(fc, &geo, axis, origin, &range, clip, &mut shapes);
    let mut links = 0;
    if let Some(rows) = fc.visible() {
        let body = Body {
            axis,
            origin,
            clip,
            geo,
        };
        links = link_shapes(rows, &body, &range, &mut shapes);
        let texts = fc.doc.map(|doc| SessionTexts {
            session: &doc.session,
            lang: text_language(fc.texts.language()),
        });
        let fill = |role: BarRole| color(c.bars.get(&role).map_or(c.text, |p| p.fill));
        let mut labels = Vec::new();
        for i in range.clone() {
            // En-tête de groupe (finitions P3, H-UI-129) : barre
            // récapitulative du groupe, sans texte ni infobulle.
            if let Some(group) = rows.header_bar_row(DisplayRow::at(i)) {
                if let Some(bar) = screen_bar(&group, axis, origin, i, &geo)
                    && !bar.culled(clip)
                {
                    bar_shapes(&bar, &geo, None, &mut shapes);
                }
                continue;
            }
            let Some(row) = rows.row(DisplayRow::at(i)) else {
                continue;
            };
            if geo.tracking
                && let Some(shape) = baseline_shape(
                    row,
                    axis,
                    origin.y + index_to_f32(i) * geo.row_h(),
                    geo.row_h(),
                    clip,
                    fill(BarRole::Baseline),
                )
            {
                shapes.push(shape);
            }
            let Some(bar) = body.bar(rows, i) else {
                continue;
            };
            if bar.culled(clip) {
                continue;
            }
            let phase = screen.phase_colors.get(&row.uid).map(|(rgb, _)| *rgb);
            bar_shapes(&bar, &geo, phase, &mut shapes);
            hatch_shapes(&bar, &geo, clip, &mut shapes);
            // Interface adoucie (lot U1, H-UI-170) : avancement dans la
            // teinte de la barre, assombrie jusqu'à 3:1.
            let progress = if geo.soft {
                let role_fill = c.bars.get(&bar.g.role).map_or(c.text, |p| p.fill);
                let phase_fill = phase.filter(|_| plannercraft_render::geometry::phase_painted(bar.g.role));
                color(crate::theme::progress_ink(phase_fill.unwrap_or(role_fill)))
            } else {
                fill(BarRole::Progress)
            };
            if geo.tracking
                && let Some(shape) = progress_shape(row, &bar, axis, progress)
            {
                shapes.push(shape);
            }
            if let Some(src) = &texts {
                let tc = TextCtx {
                    painter: &painter,
                    src,
                    axis,
                    clip,
                };
                text_shapes(&tc, &bar, row.uid, phase, &geo, &mut labels);
            }
        }
        shapes.extend(labels);
        bar_tooltip(ui, fc, rows, &body, &range);
    }
    fc.stats.gantt_rows = range;
    fc.stats.links_painted = links;
    fc.stats.shapes = fc.stats.shapes.saturating_add(shapes.len());
    painter.extend(shapes);
}

/// Tolérance du pointeur autour du tracé d'un lien, en points.
const LINK_HIT_TOL: f32 = 3.0;

/// Cible du pointeur `p` (écran) dans le corps du Gantt (H-UI-20,
/// H-UI-21) : zone de la barre de sa ligne (bande verticale de la barre et
/// 2 points autour ; losange d'un jalon), sinon tracé d'un lien qui
/// traverse sa ligne, sinon rien. `origin` : coin haut-gauche écran de la
/// ligne 0 ; même géométrie que le dessin (`geo`). Indices (lignes
/// affichées) et UID seulement : aucune date.
#[must_use]
pub fn hit_test(
    rows: VisibleRows<'_>,
    axis: &TimeAxis,
    origin: Pos2,
    geo: &GanttGeo<'_>,
    p: Pos2,
) -> GanttHit {
    let Some(i) = row_at(p.y - origin.y, geo.row_h(), rows.len()) else {
        return GanttHit::Empty;
    };
    let body = Body {
        axis,
        origin,
        clip: Rect::EVERYTHING,
        geo: *geo,
    };
    if let (Some(row), Some(bar)) = (rows.row(DisplayRow::at(i)), body.bar(rows, i)) {
        let band = bar.hover_rect();
        let (left, right) = if bar.kind == RowKind::Milestone {
            (band.left(), band.right())
        } else {
            (bar.rect.left(), bar.rect.right())
        };
        if band.y_range().contains(p.y) {
            match bar_zone(p.x, left, right, row.kind) {
                Some(BarZone::Progress) => return GanttHit::BarStart(row.uid),
                Some(BarZone::Resize) => return GanttHit::BarEnd(row.uid),
                Some(BarZone::Inside) => return GanttHit::Bar(row.uid),
                None => {}
            }
        }
    }
    for (shown, link) in rows.links_touching(i..i.saturating_add(1)) {
        let (from, to) = (shown.from.index(), shown.to.index());
        let (Some(pred), Some(succ)) = (rows.row(shown.from), rows.row(shown.to)) else {
            continue;
        };
        let (Some(pb), Some(sb)) = (body.bar(rows, from), body.bar(rows, to)) else {
            continue;
        };
        let Some(path) = screen_link(link.kind, &pb, &sb, geo.row_h()) else {
            continue;
        };
        if near_polyline(p, path.points(), LINK_HIT_TOL) || path.arrow_tip.distance(p) <= LINK_HIT_TOL {
            return GanttHit::Link(pred.uid, succ.uid);
        }
    }
    GanttHit::Empty
}

/// Fantôme du glisser en cours (J7, lot L8) : contour de la barre aux
/// dates de l'aperçu (`gesture::preview`, abscisses par `TimeAxis::x`,
/// sens date → pixel), bande d'avancement de l'aperçu, ou trait du lien
/// tracé vers la ligne survolée. Aucune date n'est calculée ici.
pub fn paint_drag(
    ui: &egui::Ui,
    fc: &FrameCtx<'_>,
    axis: &TimeAxis,
    origin: Pos2,
    clip: Rect,
    drag: &BarDrag,
    hover: Option<Pos2>,
) {
    let Some(rows) = fc.visible() else {
        return;
    };
    let Some(d) = rows.display_of(drag.uid) else {
        return;
    };
    let i = d.index();
    let row_h = fc.metrics.row_h;
    let top = origin.y + index_to_f32(i) * row_h;
    let yc = top + row_h / 2.0;
    let half = LinkMetrics::default().bar_half_h + 1.0;
    let painter = ui.painter_at(clip);
    let stroke = Stroke::new(1.5, fc.palette.text_strong);
    let linking = drag.kind == GestureKind::Move && drag.over_row.is_some_and(|r| r != d);
    if linking {
        if let Some(p) = hover.filter(|p| p.is_finite()) {
            let from = pos2(clip.left() + drag.origin_x, yc);
            painter.line_segment([from, p], stroke);
        }
        return;
    }
    let Some(preview) = drag.preview else {
        return;
    };
    let x0 = axis.x(preview.start);
    let x1 = axis.x(preview.finish).max(x0 + 1.0);
    if !(x0.is_finite() && x1.is_finite() && yc.is_finite()) {
        return;
    }
    let rect = Rect::from_min_max(pos2(x0, yc - half), pos2(x1, yc + half));
    painter.rect_stroke(rect, 2.0, stroke, StrokeKind::Outside);
    if let Some(percent) = preview.percent {
        // Bande proportionnelle au % de l'aperçu (dessin seulement).
        let share = index_to_f32(usize::try_from(percent.get().clamp(0, 10_000)).unwrap_or(0)) / 10_000.0;
        let band = Rect::from_min_max(pos2(x0, yc - 1.5), pos2(x0 + (x1 - x0) * share, yc + 1.5));
        painter.rect_filled(band, 0.0, fc.palette.text_strong);
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::similar_names,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use super::*;
    use crate::theme::Palette;

    const M: LinkMetrics = LinkMetrics {
        stub: 6.0,
        arrow: 4.0,
        row_h: 22.0,
        bar_half_h: 5.0,
    };

    fn bar(start_x: f32, finish_x: f32, row: u8) -> BarEnds {
        BarEnds {
            start_x,
            finish_x,
            y: 11.0 + 22.0 * f32::from(row),
        }
    }

    /// Vérifications communes : points finis, segments orthogonaux, premier
    /// point sur l'extrémité attendue du prédécesseur, dernier sur
    /// l'extrémité attendue du successeur, flèche au dernier point.
    fn check(path: &LinkPath, first: Pos2, last: Pos2, dir: ArrowDir) {
        let pts = path.points();
        assert!(pts.len() >= 2 && pts.len() <= 6, "{pts:?}");
        assert!(pts.iter().all(|p| p.x.is_finite() && p.y.is_finite()));
        for w in pts.windows(2) {
            assert!(w[0].x == w[1].x || w[0].y == w[1].y, "segment oblique {w:?}");
        }
        assert_eq!(pts[0], first);
        assert_eq!(*pts.last().unwrap(), last);
        assert_eq!(path.arrow_tip, last);
        assert_eq!(path.arrow_dir, dir);
        // Le dernier segment va dans le sens de la flèche.
        let (a, b) = (pts[pts.len() - 2], pts[pts.len() - 1]);
        let ok = match dir {
            ArrowDir::Right => b.x > a.x,
            ArrowDir::Left => b.x < a.x,
            ArrowDir::Down => b.y > a.y,
            ArrowDir::Up => b.y < a.y,
        };
        assert!(ok, "flèche {dir:?} contre le segment {a:?} → {b:?}");
    }

    #[test]
    fn link_path_fs_direct() {
        let p = bar(10.0, 50.0, 0);
        let s = bar(80.0, 140.0, 1);
        let path = link_path(LinkType::FinishToStart, p, s, &M);
        // Entre par le dessus du successeur, à `stub` de son début.
        check(&path, pos2(50.0, 11.0), pos2(86.0, 33.0 - 5.0), ArrowDir::Down);
        assert_eq!(path.len, 3);
    }

    #[test]
    fn link_path_fs_folded() {
        let p = bar(10.0, 50.0, 0);
        let s = bar(30.0, 90.0, 2);
        let path = link_path(LinkType::FinishToStart, p, s, &M);
        check(&path, pos2(50.0, 11.0), pos2(30.0, 55.0), ArrowDir::Right);
        assert_eq!(path.len, 6);
        // Passage dans l'intercolonne, sous la ligne du prédécesseur.
        assert_eq!(path.points[2].y, 22.0);
        assert_eq!(path.points[3], pos2(24.0, 22.0));
    }

    #[test]
    fn link_path_ss() {
        let p = bar(40.0, 90.0, 0);
        let s = bar(60.0, 120.0, 1);
        let path = link_path(LinkType::StartToStart, p, s, &M);
        check(&path, pos2(40.0, 11.0), pos2(60.0, 33.0), ArrowDir::Right);
        assert_eq!(path.points[1].x, 34.0);
    }

    #[test]
    fn link_path_ff() {
        let p = bar(40.0, 90.0, 0);
        let s = bar(20.0, 70.0, 1);
        let path = link_path(LinkType::FinishToFinish, p, s, &M);
        check(&path, pos2(90.0, 11.0), pos2(70.0, 33.0), ArrowDir::Left);
        assert_eq!(path.points[1].x, 96.0);
    }

    #[test]
    fn link_path_sf() {
        // Direct : le successeur finit bien avant le début du prédécesseur.
        let p = bar(100.0, 150.0, 0);
        let s = bar(20.0, 60.0, 1);
        let path = link_path(LinkType::StartToFinish, p, s, &M);
        check(&path, pos2(100.0, 11.0), pos2(60.0, 33.0), ArrowDir::Left);
        assert_eq!(path.len, 4);
        // Replié : le successeur finit après le début du prédécesseur.
        let s = bar(80.0, 130.0, 1);
        let path = link_path(LinkType::StartToFinish, p, s, &M);
        check(&path, pos2(100.0, 11.0), pos2(130.0, 33.0), ArrowDir::Left);
        assert_eq!(path.len, 6);
    }

    #[test]
    fn link_path_successor_above() {
        let p = bar(10.0, 50.0, 3);
        let s = bar(80.0, 140.0, 1);
        let path = link_path(LinkType::FinishToStart, p, s, &M);
        // Entre par le dessous du successeur, flèche vers le haut.
        check(&path, pos2(50.0, 77.0), pos2(86.0, 33.0 + 5.0), ArrowDir::Up);
        // Replié au-dessus : l'intercolonne est la frontière au-dessus de P.
        let s = bar(20.0, 60.0, 1);
        let path = link_path(LinkType::FinishToStart, p, s, &M);
        check(&path, pos2(50.0, 77.0), pos2(20.0, 33.0), ArrowDir::Right);
        assert_eq!(path.points[2].y, 66.0);
    }

    #[test]
    fn link_path_milestone() {
        // Jalon prédécesseur (losange centré en 50, demi-diagonale 6).
        let p = bar(44.0, 56.0, 0);
        let s = bar(90.0, 140.0, 1);
        let path = link_path(LinkType::FinishToStart, p, s, &M);
        check(&path, pos2(56.0, 11.0), pos2(96.0, 28.0), ArrowDir::Down);
        // Jalon successeur : la flèche tombe au centre du losange.
        let p = bar(10.0, 40.0, 0);
        let s = bar(74.0, 86.0, 1);
        let path = link_path(LinkType::FinishToStart, p, s, &M);
        check(&path, pos2(40.0, 11.0), pos2(80.0, 28.0), ArrowDir::Down);
        // Entrée non finie : tracé vide, sans panique.
        let bad = bar(f32::NAN, 10.0, 0);
        assert_eq!(link_path(LinkType::FinishToStart, bad, s, &M).len, 0);
    }

    #[test]
    fn arrow_points_oriented() {
        let tip = pos2(10.0, 10.0);
        let right = arrow_points(tip, ArrowDir::Right, 4.0);
        assert!(right[1].x < tip.x && right[2].x < tip.x);
        let left = arrow_points(tip, ArrowDir::Left, 4.0);
        assert!(left[1].x > tip.x && left[2].x > tip.x);
        let down = arrow_points(tip, ArrowDir::Down, 4.0);
        assert!(down[1].y < tip.y && down[2].y < tip.y);
        let up = arrow_points(tip, ArrowDir::Up, 4.0);
        assert!(up[1].y > tip.y && up[2].y > tip.y);
    }
    /// H-UI-22 : ligne de base dans le tiers inférieur, barre courante
    /// au-dessus, sans chevauchement ; entrée non finie sans NaN.
    #[test]
    fn h_ui_22_tracking_layout_lower_third() {
        let l = tracking_layout(100.0, 22.0);
        let lower = 100.0 + 22.0 * 2.0 / 3.0;
        assert!(l.baseline.0 >= lower && l.baseline.1 <= 122.0, "{l:?}");
        assert!(l.baseline.1 > l.baseline.0, "{l:?}");
        assert!(l.current.0 >= 100.0 && l.current.1 < lower, "{l:?}");
        assert!(l.current.1 > l.current.0 && l.current.1 < l.baseline.0, "{l:?}");
        for (top, h) in [(f32::NAN, 22.0), (0.0, f32::INFINITY), (0.0, 0.0), (0.0, -3.0)] {
            let l = tracking_layout(top, h);
            for v in [l.current.0, l.current.1, l.baseline.0, l.baseline.1] {
                assert!(v.is_finite(), "{top} {h} : {l:?}");
            }
            assert!(l.current.1 >= l.current.0 && l.baseline.1 >= l.baseline.0);
        }
        // Ligne très basse : bandes bornées à la ligne, jamais inversées.
        let l = tracking_layout(0.0, 3.0);
        assert!(
            l.current.1 >= l.current.0 && l.baseline.1 >= l.baseline.0,
            "{l:?}"
        );
        assert!(l.current.0 <= 3.0 && l.baseline.0 <= 3.0, "{l:?}");
    }

    /// H-UI-22 : bande sur [début, `Stop`], bornée à la barre ; vide avant
    /// le début ; non finie ⇒ rien. Exemple du cahier : barre de 4 jours à
    /// 12 px par jour, `Stop` au soir du 2e jour ⇒ 24 px.
    #[test]
    fn h_ui_22_progress_span_bounded() {
        assert_eq!(progress_span(100.0, 148.0, 124.0), Some((100.0, 124.0)));
        assert_eq!(progress_span(100.0, 148.0, 500.0), Some((100.0, 148.0)));
        assert_eq!(progress_span(100.0, 148.0, 100.0), None);
        assert_eq!(progress_span(100.0, 148.0, 50.0), None);
        assert_eq!(progress_span(f32::NAN, 148.0, 120.0), None);
        assert_eq!(progress_span(100.0, 148.0, f32::INFINITY), None);
    }

    /// H-UI-186 : ligne « Calendrier » seulement pour un calendrier propre
    /// à la tâche qui a déplacé la date d'un pilote qui se recale.
    #[test]
    fn h_ui_186_calendar_line_only_when_it_moved() {
        use plannercraft_automation::explain::{CalendarRefView, DriverView, StepView};
        use plannercraft_automation::json::LinkTypeJson;

        let link = StepView::Driver {
            driver: DriverView::Link {
                predecessor: 2,
                link: LinkTypeJson::Fs,
            },
            raw: Some("2026-12-23T17:00".to_owned()),
        };
        let snap = |raw: &str, uid: u32, name: &str| StepView::Snap {
            raw: "2026-12-23T17:00".to_owned(),
            snapped: raw.to_owned(),
            calendar: Some(CalendarRefView {
                uid,
                name: name.to_owned(),
            }),
        };
        let own = snap("2027-01-04T08:00", 2, "Entreprise de terrassement");
        assert_eq!(
            super::moved_by_calendar(&[link.clone(), own.clone()], 1),
            Some("Entreprise de terrassement")
        );
        // Calendrier du projet, instant inchangé, sans nom : rien.
        assert_eq!(
            super::moved_by_calendar(&[link.clone(), snap("2027-01-04T08:00", 1, "Standard")], 1),
            None
        );
        assert_eq!(
            super::moved_by_calendar(&[link.clone(), snap("2026-12-23T17:00", 2, "X")], 1),
            None
        );
        assert_eq!(
            super::moved_by_calendar(&[link, snap("2027-01-04T08:00", 2, "")], 1),
            None
        );
        // Dates saisies : pas de recalage à expliquer.
        let manual = StepView::Driver {
            driver: DriverView::Manual,
            raw: None,
        };
        assert_eq!(super::moved_by_calendar(&[manual, own], 1), None);
    }

    /// H-UI-26 : une entrée, relue seulement quand la clé change.
    #[test]
    fn h_ui_26_tip_cache_one_entry() {
        let mut cache = ExplainTipCache::<String>::default();
        let a = (1, 0, TaskUid::new(3), SideJson::Start);
        let b = (1, 1, TaskUid::new(3), SideJson::Start);
        assert_eq!(cache.text_for(a, || "x".to_owned()), "x");
        assert_eq!(cache.text_for(a, || "y".to_owned()), "x");
        assert_eq!(cache.runs, 1);
        assert_eq!(cache.text_for(b, || "z".to_owned()), "z");
        assert_eq!(cache.text_for(a, || "w".to_owned()), "w");
        assert_eq!(cache.runs, 3);
    }

    /// Document de test ouvert sur un projet généré.
    fn generated_doc(summaries: usize, leaves: usize, links: usize, inactive_every: usize) -> OpenDocument {
        use plannercraft_core::Moment;

        use crate::app::DocOrigin;

        let mut p = plannercraft_tests::generate::ui_network(summaries, leaves, links);
        let uids: Vec<TaskUid> = p.tasks.uids().collect();
        for (k, uid) in uids.into_iter().enumerate() {
            if inactive_every > 0
                && k % inactive_every == 2
                && let Some(task) = p.tasks.get_mut(uid)
            {
                task.active = false;
            }
        }
        let now: Moment = "2026-01-05T08:00".parse().unwrap();
        let origin = DocOrigin {
            name: String::new(),
            path: None,
            demo: false,
        };
        OpenDocument::new(crate::document::load_project(p, now), origin, 1)
    }

    /// Écart toléré entre une abscisse écran (`f32`, × 16) et celle de la
    /// scène (1/16 px entier) : l'arrondi au 1/16 px, plus l'erreur du
    /// `f32` sur de grandes abscisses.
    fn close16(screen: f32, scene: i64) -> bool {
        let x = screen * 16.0;
        (x - crate::geom::i64_to_f32(scene)).abs() <= 0.5 + x.abs() * 1e-6
    }

    /// P55 sur un projet, un zoom et un modèle : barres (rôle, forme,
    /// rectangle), liens et date d'état de l'écran égaux à ceux de la scène
    /// au 1/16 px (ordonnées exactes, abscisses à l'arrondi près).
    #[allow(
        clippy::too_many_lines,
        reason = "barres, liens et date d'état, lus d'un bloc"
    )]
    fn check_p55(doc: &OpenDocument, zoom: TimeScaleZoom, id: plannercraft_render::style::TemplateId) {
        use plannercraft_render::geometry::{Class, NoTexts, Prim, Viewport, layout};
        use plannercraft_render::{Lang, RangeSpec};

        use crate::i18n::{Language, Texts};
        use crate::row_view::RowView;
        use crate::timescale::render_zoom;

        let style = plannercraft_render::style::template(id);
        let texts = Texts::new(Language::Fr);
        let model = RowModel::build(doc, texts.format_labels());
        let view = RowView::unfiltered(&model);
        let rows = VisibleRows::new(&model, &view);
        let axis = TimeAxis::new(doc.range, zoom, 0.0);
        let geo = GanttGeo {
            style: &style,
            variant: Variant::Light,
            tracking: false,
            highlight_critical: true,
            soft: false,
        };
        let last = doc.range.end.checked_add_days(-1).unwrap();
        let vp = Viewport {
            range: RangeSpec::Span {
                from: doc.range.start,
                to: last.max(doc.range.start),
            },
            zoom: render_zoom(zoom),
            ..Viewport::new(Variant::Light, Lang::Fr)
        };
        let (scene, _) = layout(doc.project(), doc.schedule(), &style, &NoTexts, &vp);
        assert_eq!(scene.rows.len(), rows.len(), "{zoom:?} {id:?}");
        let (dx, dy) = (scene.table16, scene.header16);
        let mut screen_bars = Vec::new();
        for (i, srow) in scene.rows.iter().enumerate() {
            let row = rows.row(DisplayRow::at(i)).unwrap();
            assert_eq!(srow.uid, Some(row.uid));
            assert_eq!(
                srow.y16,
                dy + i64::try_from(i).unwrap() * geometry::row_h16(&style)
            );
            let screen = screen_bar(row, &axis, pos2(0.0, 0.0), i, &geo);
            let drawn = srow.bar.filter(|b| b.rect.w != 0 || b.rect.h != 0);
            match (screen, drawn) {
                (None, None) => {}
                (Some(s), Some(b)) => {
                    let ctx = format!("{zoom:?} {id:?} ligne {i} : {s:?} / {b:?}");
                    assert_eq!(s.g.role, b.role, "{ctx}");
                    assert_eq!(s.g.shape, b.shape, "{ctx}");
                    assert_eq!(
                        s.rect.top() * 16.0,
                        crate::geom::i64_to_f32(b.rect.y - dy),
                        "{ctx}"
                    );
                    assert_eq!(
                        s.rect.bottom() * 16.0,
                        crate::geom::i64_to_f32(b.rect.bottom() - dy),
                        "{ctx}"
                    );
                    assert_eq!(s.yc * 16.0, crate::geom::i64_to_f32(b.yc16 - dy), "{ctx}");
                    assert!(close16(s.rect.left(), b.rect.x - dx), "{ctx}");
                    assert!(close16(s.rect.right(), b.rect.right() - dx), "{ctx}");
                }
                (s, b) => panic!("{zoom:?} {id:?} ligne {i} : écran {s:?}, scène {b:?}"),
            }
            screen_bars.push((screen, drawn));
        }
        // Liens : même tracé (nombre de points, ordonnées exactes,
        // abscisses à l'arrondi près) sur les barres de l'écran et de la
        // scène.
        for (to, task) in doc.project().tasks.iter().enumerate() {
            for link in &task.predecessors {
                let from = doc
                    .project()
                    .tasks
                    .iter()
                    .position(|t| t.uid() == link.predecessor)
                    .unwrap();
                let (Some((Some(sp), Some(bp))), Some((Some(ss), Some(bs)))) =
                    (screen_bars.get(from), screen_bars.get(to))
                else {
                    continue;
                };
                let shared = geometry::link_path(bp, bs, link.kind);
                let screen = screen_link(link.kind, sp, ss, geo.row_h());
                let Some(screen) = screen else {
                    assert_eq!(shared.points.len(), 0);
                    continue;
                };
                assert_eq!(
                    screen.points().len(),
                    shared.points.len(),
                    "{zoom:?} {id:?} {from}→{to}"
                );
                for (p, &(x16, y16)) in screen.points().iter().zip(&shared.points) {
                    assert_eq!(
                        p.y * 16.0,
                        crate::geom::i64_to_f32(y16 - dy),
                        "{zoom:?} {id:?} {from}→{to}"
                    );
                    assert!(
                        close16(p.x, x16 - dx),
                        "{zoom:?} {id:?} {from}→{to} : {p:?} / {x16}"
                    );
                }
            }
        }
        // Date d'état.
        let status = doc.project().effective_status_date();
        let drawn: Vec<i64> = scene
            .items
            .iter()
            .filter(|item| item.class == Class::StatusLine)
            .filter_map(|item| match &item.prim {
                Prim::Segments { segs } => segs.first().map(|seg| seg.0),
                _ => None,
            })
            .collect();
        if let Some(&x) = drawn.first() {
            assert!(close16(axis.x(status), x - dx), "{zoom:?} date d'état");
        }
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(512))]

        /// P55 (écran, J8 lot L11) : pour tout projet généré, tout zoom et
        /// tout modèle, rectangles de barre, points des liens et abscisse
        /// de la date d'état calculés pour l'écran égalent ceux de la scène
        /// (1/16 px ; abscisses à l'arrondi du 1/16 px près, l'écran
        /// projetant les instants en `f32` comme J7).
        #[test]
        fn p55_screen_matches_scene(
            summaries in 1_usize..5,
            leaves in 1_usize..9,
            links in 0_usize..40,
            inactive in 0_usize..7,
            zoom in 0_usize..4,
            id in 0_usize..6,
        ) {
            let doc = generated_doc(summaries, leaves, links, inactive);
            check_p55(&doc, TimeScaleZoom::ALL[zoom], plannercraft_render::style::TemplateId::ALL[id]);
        }
    }

    /// H-UI-36 à H-UI-38 : rôle d'une ligne (inactive, manuelle avant
    /// critique, critique seulement avec la surbrillance).
    #[test]
    fn h_ui_38_highlight_off_maps_roles() {
        let mut row = Row {
            uid: TaskUid::new(1),
            id: 1,
            level: 1,
            kind: RowKind::Leaf,
            critical: true,
            bar: None,
            percent: plannercraft_core::Percent::ZERO,
            baseline: None,
            finish_variance: None,
            stop: None,
            manual: false,
            active: true,
        };
        assert_eq!(row_role(&row, true), Some(BarRole::Critical));
        assert_eq!(row_role(&row, false), Some(BarRole::Task));
        row.manual = true;
        assert_eq!(row_role(&row, true), Some(BarRole::Manual));
        row.kind = RowKind::Summary;
        assert_eq!(row_role(&row, true), Some(BarRole::SummaryCritical));
        assert_eq!(row_role(&row, false), Some(BarRole::Summary));
        row.kind = RowKind::Milestone;
        assert_eq!(row_role(&row, true), Some(BarRole::MilestoneCritical));
        assert_eq!(row_role(&row, false), Some(BarRole::Milestone));
        row.kind = RowKind::Excluded;
        assert_eq!(row_role(&row, true), Some(BarRole::Inactive));
        row.kind = RowKind::Null;
        assert_eq!(row_role(&row, true), None);
    }

    /// Hachures (H-UI-44) : segments dans la barre, bornés à sa partie
    /// visible, recalés sur le pas (le motif ne glisse pas).
    #[test]
    fn styled_hatch_segments_inside_bar() {
        let style = plannercraft_render::style::template(plannercraft_render::style::TemplateId::Chantier);
        let geo = GanttGeo {
            style: &style,
            variant: Variant::Light,
            tracking: false,
            highlight_critical: true,
            soft: false,
        };
        let g = Geom16 {
            role: BarRole::Critical,
            shape: BarShape::Bar,
            rect: Rect16::default(),
            progress: None,
            baseline: None,
            yc16: 0,
            link_half16: 80,
            row_top16: 0,
            row_h16: 288,
        };
        let bar = ScreenBar {
            g,
            rect: Rect::from_min_max(pos2(100.0, 10.0), pos2(900.0, 20.0)),
            yc: 15.0,
            half: 5.0,
            link_half: 5.0,
            kind: RowKind::Leaf,
        };
        let clip = Rect::from_min_max(pos2(300.0, 0.0), pos2(500.0, 100.0));
        let mut out = Vec::new();
        hatch_shapes(&bar, &geo, clip, &mut out);
        assert_ne!(out.len(), 0);
        let mut firsts = Vec::new();
        for shape in &out {
            let Shape::LineSegment { points, .. } = shape else {
                panic!("{shape:?}");
            };
            for p in points {
                assert!(bar.rect.expand(0.01).contains(*p), "{p:?}");
                assert!(p.x >= clip.left() - CULL_MARGIN - 4.01 && p.x <= clip.right() + CULL_MARGIN + 0.01);
            }
            firsts.push(points[0].x + points[0].y);
        }
        // Droites x + y = c, c ≡ x0 + y0 (mod 4 px) : même famille quel que
        // soit le défilement.
        for c in firsts {
            let k = (c - 110.0) / 4.0;
            assert!((k - k.round()).abs() < 1e-3, "{c}");
        }
    }

    /// Fumée : en-tête et corps dessinés sur l'exemple, à tous les zooms, y
    /// compris avec une découpe dégénérée, sans panique ni `Rect` non fini
    /// (egui vérifie par `debug_assert!`).
    #[test]
    fn gantt_paint_smoke_demo() {
        use plannercraft_core::{Day, Moment, TimeOfDay};

        use crate::app::{DocOrigin, OpenDocument};
        use crate::i18n::{Language, Texts};
        use crate::row_view::RowView;
        use crate::rows::RowModel;
        use crate::state::UiState;
        use crate::timescale::TimeScaleZoom;
        use crate::view::{FrameStats, Metrics};

        let day = Day::from_naive(chrono::NaiveDate::from_ymd_opt(2027, 1, 15).unwrap()).unwrap();
        let now = Moment::at(day, TimeOfDay::hm(8, 0).unwrap()).unwrap();
        let loaded = crate::document::load(crate::DEMO_MSPDI, now).unwrap();
        let origin = DocOrigin {
            name: String::new(),
            path: None,
            demo: true,
        };
        let doc = OpenDocument::new(loaded, origin, 1);
        let texts = Texts::new(Language::Fr);
        let rows = RowModel::build(&doc, texts.format_labels());
        let row_view = RowView::unfiltered(&rows);
        let palette = Palette::LIGHT;
        let metrics = Metrics::default();
        let selection = rows.row(2).map(|r| r.uid);
        let selected = crate::selection::Selection::single(selection);
        let ctx = egui::Context::default();
        let styles = [
            crate::state::GanttStyle::Standard,
            crate::state::GanttStyle::Tracking,
        ];
        for (zoom, gantt_style) in TimeScaleZoom::ALL
            .into_iter()
            .flat_map(|z| styles.map(|s| (z, s)))
        {
            let state = UiState {
                zoom,
                gantt_style,
                ..UiState::default()
            };
            let mut actions = Vec::new();
            let mut stats = FrameStats::default();
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                let mut fc = FrameCtx {
                    state: &state,
                    doc: Some(&doc),
                    rows: Some(&rows),
                    row_view: Some(&row_view),
                    texts: &texts,
                    palette: &palette,
                    metrics: &metrics,
                    diagnostics: &crate::view::DiagnosticsView::default(),
                    selection,
                    selected: &selected,
                    actions: &mut actions,
                    stats: &mut stats,
                    window_open: false,
                    task_view: crate::view::task_view::TaskViewFrame::default(),
                };
                let header = Rect::from_min_max(pos2(600.0, 0.0), pos2(1400.0, 44.0));
                let clip = Rect::from_min_max(pos2(600.0, 44.0), pos2(1400.0, 700.0));
                for scroll in [0.0, 500.0, 5_000.0] {
                    let axis = TimeAxis::new(doc.range, zoom, 600.0 - scroll);
                    crate::view::timescale::paint_header(ui, &mut fc, &axis, header);
                    let len = rows.len();
                    paint_body(ui, &mut fc, &axis, pos2(600.0, 44.0), 0..len, clip);
                }
                let axis = TimeAxis::new(doc.range, zoom, 600.0);
                let nan = Rect::from_min_max(pos2(f32::NAN, 0.0), pos2(10.0, 10.0));
                paint_body(ui, &mut fc, &axis, pos2(600.0, 44.0), 0..3, nan);
                crate::view::timescale::paint_header(ui, &mut fc, &axis, nan);
            });
            out.textures_delta.clear();
            assert!(stats.shapes > 0, "{zoom:?}");
            assert!(stats.links_painted <= 46);
        }
    }
}
