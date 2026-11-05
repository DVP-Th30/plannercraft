//! Dessin à l'écran d'une scène de `plannercraft-render` (J8, lot L15) :
//! vignettes de la galerie des modèles (H-UI-52) et aperçu paginé de la
//! mise en page et du mode impression (H-UI-68).
//!
//! La scène est celle du SVG et du PDF (`layout`, même géométrie que
//! l'écran, H-UI-42) : l'aperçu montre ce qui sera exporté. Le dessin est
//! une projection affine des 1/16 px de la scène vers un rectangle de
//! l'écran ; les textes trop petits pour être lus (moins de 3 px) sont
//! omis, les tirets dessinés pleins. Aucune date n'est calculée ici
//! (règle 3).

use std::collections::BTreeMap;

use egui::{Color32, FontFamily, FontId, Painter, Pos2, Rect, Shape, Stroke, pos2};
use plannercraft_render::geometry::{
    Anchor, Class, Clip, FontWeight, Item, Paint, Part, Prim, Scene, pattern_segments,
};
use plannercraft_render::paginate::{Page, Pages, SLOT_GAP_PT, SLOT_LINE_PT, TIMELINE_GAP16, TITLE_LINE_PT};
use plannercraft_render::style::{FontChoice, Rgb};

use crate::geom::i64_to_f32;

/// Taille de texte (px écran) en dessous de laquelle un texte est omis.
const MIN_TEXT_PX: f32 = 3.0;

/// Points par seizième de pixel de la scène à 100 % (96 px par pouce,
/// 72 points par pouce).
const PT_PER_16: f32 = 3.0 / 64.0;

/// Points (entier) en `f32` (papier, marges, interlignes : petits entiers).
fn pt(v: u32) -> f32 {
    i64_to_f32(i64::from(v))
}

/// Couleur egui.
const fn color(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

/// Projection d'une zone de la scène (1/16 px) vers l'écran.
#[derive(Clone, Copy, Debug)]
pub struct Projection {
    /// Coin haut-gauche de la zone dans la scène (1/16 px).
    pub src: (i64, i64),
    /// Coin haut-gauche à l'écran.
    pub dst: Pos2,
    /// Pixels d'écran par seizième de pixel de la scène.
    pub k: f32,
}

impl Projection {
    /// Point de la scène à l'écran.
    fn at(&self, x: i64, y: i64) -> Pos2 {
        pos2(
            self.dst.x + i64_to_f32(x.saturating_sub(self.src.0)) * self.k,
            self.dst.y + i64_to_f32(y.saturating_sub(self.src.1)) * self.k,
        )
    }

    /// Longueur de la scène à l'écran.
    fn len(&self, v: i64) -> f32 {
        i64_to_f32(v) * self.k
    }
}

/// Encres d'un dessin : peintures par classe, famille de police, police
/// Inter installée, tirets dessinés (chronologie à l'écran) ou pleins
/// (aperçu, vignettes : rendu inchangé depuis J8).
#[derive(Clone, Copy, Debug)]
pub struct Ink<'a> {
    /// Peintures des classes.
    pub paints: &'a BTreeMap<Class, Paint>,
    /// Famille de police du style.
    pub family: FontChoice,
    /// Police Inter installée dans egui.
    pub inter: bool,
    /// Segments tiretés dessinés en tirets (sinon pleins).
    pub dashes: bool,
}

impl<'a> Ink<'a> {
    /// Encres de la scène `scene` (tirets pleins, comme le reste de
    /// l'aperçu).
    #[must_use]
    pub const fn of(scene: &'a Scene, inter: bool) -> Self {
        Self {
            paints: &scene.paints,
            family: scene.family,
            inter,
            dashes: false,
        }
    }
}

/// Peinture d'une classe (texte courant à défaut).
fn paint_of(paints: &BTreeMap<Class, Paint>, class: Class) -> Paint {
    paints.get(&class).copied().unwrap_or(Paint {
        fill: paints.get(&Class::Text).and_then(|p| p.fill),
        stroke: None,
        width16: 0,
        dash16: None,
    })
}

/// Famille de police (Inter si le style la demande et que la police est
/// installée, sinon la police de l'interface).
fn family(choice: FontChoice, weight: FontWeight, inter: bool) -> FontFamily {
    match (choice, inter, weight) {
        (FontChoice::Inter, true, FontWeight::SemiBold) => {
            FontFamily::Name(crate::theme::PLAN_FAMILY_STRONG.into())
        }
        (FontChoice::Inter, true, FontWeight::Regular) => FontFamily::Name(crate::theme::PLAN_FAMILY.into()),
        _ => FontFamily::Proportional,
    }
}

/// Formes d'une primitive.
#[allow(clippy::too_many_lines, reason = "une branche par primitive")]
fn item_shapes(painter: &Painter, ink: &Ink<'_>, it: &Item, pr: &Projection, out: &mut Vec<Shape>) {
    let paint = paint_of(ink.paints, it.class);
    let fill = paint.fill.map(color);
    let stroke = paint.stroke.map_or(Stroke::NONE, |c| {
        Stroke::new((pr.len(paint.width16.max(16))).clamp(0.5, 4.0), color(c))
    });
    match &it.prim {
        Prim::Rect { r, radius16 } => {
            let rect = Rect::from_min_max(pr.at(r.x, r.y), pr.at(r.right(), r.bottom()));
            if !rect.is_finite() {
                return;
            }
            let radius = pr.len(*radius16).min(rect.height() / 2.0).max(0.0);
            if let Some(f) = fill {
                out.push(Shape::rect_filled(rect, radius, f));
            }
            if paint.stroke.is_some() {
                out.push(Shape::rect_stroke(rect, radius, stroke, egui::StrokeKind::Inside));
            }
        }
        Prim::Diamond { r } => {
            let c = pr.at(r.x.saturating_add(r.w / 2), r.y.saturating_add(r.h / 2));
            let (dx, dy) = (pr.len(r.w) / 2.0, pr.len(r.h) / 2.0);
            let pts = vec![
                pos2(c.x, c.y - dy),
                pos2(c.x + dx, c.y),
                pos2(c.x, c.y + dy),
                pos2(c.x - dx, c.y),
            ];
            if pts.iter().all(|p| p.is_finite()) {
                out.push(Shape::convex_polygon(
                    pts,
                    fill.unwrap_or(Color32::TRANSPARENT),
                    stroke,
                ));
            }
        }
        Prim::Bracket { r, thick16, tip_w16 } => {
            let ink = fill.unwrap_or(Color32::TRANSPARENT);
            let top = pr.at(r.x, r.y);
            let bar = Rect::from_min_max(top, pr.at(r.right(), r.y.saturating_add(*thick16)));
            if !bar.is_finite() {
                return;
            }
            out.push(Shape::rect_filled(bar, 0.0, ink));
            let bottom = pr.at(r.x, r.bottom()).y;
            let tip = pr.len(*tip_w16);
            for (x, dir) in [(bar.left(), 1.0), (bar.right(), -1.0)] {
                let pts = vec![
                    pos2(x, bar.bottom()),
                    pos2(x + dir * tip, bar.bottom()),
                    pos2(x, bottom),
                ];
                if pts.iter().all(|p| p.is_finite()) {
                    out.push(Shape::convex_polygon(pts, ink, Stroke::NONE));
                }
            }
        }
        Prim::Polyline { points } => {
            let pts: Vec<Pos2> = points.iter().map(|&(x, y)| pr.at(x, y)).collect();
            if pts.len() >= 2 && pts.iter().all(|p| p.is_finite()) {
                out.push(Shape::line(pts, stroke));
            }
        }
        Prim::Polygon { points } => {
            let pts: Vec<Pos2> = points.iter().map(|&(x, y)| pr.at(x, y)).collect();
            if pts.len() >= 3 && pts.iter().all(|p| p.is_finite()) {
                out.push(Shape::convex_polygon(
                    pts,
                    fill.unwrap_or(Color32::TRANSPARENT),
                    Stroke::NONE,
                ));
            }
        }
        Prim::Segments { segs } => {
            let dash = paint
                .dash16
                .filter(|_| ink.dashes)
                .map(|(on, off)| (pr.len(on).max(1.0), pr.len(off).max(1.0)))
                .filter(|(on, off)| on.is_finite() && off.is_finite());
            for &(x0, y0, x1, y1) in segs {
                let (a, b) = (pr.at(x0, y0), pr.at(x1, y1));
                if a.is_finite() && b.is_finite() {
                    match dash {
                        Some((on, off)) => out.extend(Shape::dashed_line(&[a, b], stroke, on, off)),
                        None => out.push(Shape::line_segment([a, b], stroke)),
                    }
                }
            }
        }
        Prim::Hatch { r, dots, diamond } => {
            let ink = Stroke::new(1.0, paint.stroke.map_or(Color32::GRAY, color));
            for (ax, ay, bx, by) in pattern_segments(*r, *dots, *diamond) {
                let (a, b) = (pr.at(ax, ay), pr.at(bx, by));
                if a.is_finite() && b.is_finite() {
                    out.push(Shape::line_segment([a, b], ink));
                }
            }
        }
        Prim::Text {
            x16,
            y16,
            text,
            font,
            anchor,
        } => {
            let size = pr.len(i64::from(font.size16));
            if !size.is_finite() || size < MIN_TEXT_PX || text.is_empty() {
                return;
            }
            let text_color = fill.unwrap_or(Color32::BLACK);
            let font_id = FontId::new(size, family(ink.family, font.weight, ink.inter));
            // Italique d'une tâche (finitions P2, H-UI-126) : oblique d'egui.
            let galley = if font.italic {
                let mut format = egui::TextFormat::simple(font_id, text_color);
                format.italics = true;
                painter.layout_job(egui::text::LayoutJob::single_section(text.clone(), format))
            } else {
                painter.layout_no_wrap(text.clone(), font_id, text_color)
            };
            let base = pr.at(*x16, *y16);
            let w = galley.size().x;
            let x = match anchor {
                Anchor::Start => base.x,
                Anchor::End => base.x - w,
            };
            let y = base.y - size * 0.95;
            if x.is_finite() && y.is_finite() {
                out.push(Shape::galley(pos2(x, y), galley, text_color));
            }
        }
    }
}

/// Primitives de `scene` retenues par `keep`, projetées par `pr` ; rend
/// le nombre de formes.
pub fn region_shapes(
    painter: &Painter,
    scene: &Scene,
    pr: &Projection,
    inter: bool,
    keep: &dyn Fn(&Item) -> bool,
    out: &mut Vec<Shape>,
) -> usize {
    let before = out.len();
    let ink = Ink::of(scene, inter);
    for it in scene.items.iter().filter(|it| keep(it)) {
        item_shapes(painter, &ink, it, pr, out);
    }
    out.len().saturating_sub(before)
}

/// Primitives `items` (hors scène : chronologie, H-R16) projetées par
/// `pr` avec les encres `ink` ; rend le nombre de formes.
pub fn items_shapes(
    painter: &Painter,
    items: &[Item],
    ink: &Ink<'_>,
    pr: &Projection,
    out: &mut Vec<Shape>,
) -> usize {
    let before = out.len();
    for it in items {
        item_shapes(painter, ink, it, pr, out);
    }
    out.len().saturating_sub(before)
}

/// Vignette : la scène entière réduite à la largeur de `dst` (haut gardé,
/// bas rogné), fond peint ; `painter` doit être découpé à `dst`.
pub fn thumbnail(painter: &Painter, scene: &Scene, dst: Rect, inter: bool) {
    if !dst.is_finite() || dst.width() <= 0.0 || scene.width16 <= 0 {
        return;
    }
    let k = dst.width() / i64_to_f32(scene.width16);
    let pr = Projection {
        src: (0, 0),
        dst: dst.min,
        k,
    };
    let mut shapes = vec![Shape::rect_filled(
        dst,
        0.0,
        paint_of(&scene.paints, Class::Bg)
            .fill
            .map_or(Color32::WHITE, color),
    )];
    region_shapes(painter, scene, &pr, inter, &|_| true, &mut shapes);
    painter.extend(shapes);
}

/// Taille d'une page à l'écran dans `room`, proportions du papier gardées.
#[must_use]
pub fn page_rect(pages: &Pages, room: Rect) -> Rect {
    let (w, h) = (pt(pages.paper_pt.0.max(1)), pt(pages.paper_pt.1.max(1)));
    let s = (room.width() / w).min(room.height() / h);
    let size = egui::vec2(w * s, h * s);
    Rect::from_center_size(room.center(), size)
}

/// Lignes d'une zone d'en-tête ou de pied de page.
#[allow(
    clippy::too_many_arguments,
    reason = "zone, place, échelle, encres : un appel par zone"
)]
fn slot_shapes(
    painter: &Painter,
    zones: &[String; 3],
    origin: Pos2,
    width: f32,
    s: f32,
    title: Option<Color32>,
    ink: Color32,
    inter: bool,
    out: &mut Vec<Shape>,
) {
    let line = pt(SLOT_LINE_PT) * s;
    let shift = if title.is_some() && !zones[1].trim().is_empty() {
        pt(TITLE_LINE_PT.saturating_sub(SLOT_LINE_PT)) * s
    } else {
        0.0
    };
    for (z, zone) in zones.iter().enumerate() {
        for (n, text) in zone.lines().enumerate() {
            let (size, color, strong) = match (title, z, n) {
                (Some(t), 1, 0) => (16.5 * s, t, true),
                (Some(_), 2, 0) => (9.0 * s, ink, true),
                _ => (9.0 * s, ink, false),
            };
            if size < MIN_TEXT_PX || text.is_empty() {
                continue;
            }
            // Inter si elle est installée (style qui la demande), sinon la
            // police de l'interface : un style sans Inter n'installe pas la
            // famille, son nom seul ferait paniquer epaint (règle 10).
            let fam = match (inter, strong) {
                (true, true) => FontFamily::Name(crate::theme::PLAN_FAMILY_STRONG.into()),
                (true, false) => FontFamily::Name(crate::theme::PLAN_FAMILY.into()),
                (false, _) => FontFamily::Proportional,
            };
            let galley = painter.layout_no_wrap(text.to_owned(), FontId::new(size, fam), color);
            let w = galley.size().x.min(width / 3.0);
            let x = match z {
                0 => origin.x,
                1 => origin.x + (width - w) / 2.0,
                _ => origin.x + width - w,
            };
            #[allow(clippy::cast_precision_loss, reason = "rang de ligne, petit entier")]
            let y = if title.is_some() && z == 1 && n == 0 && shift > 0.0 {
                origin.y
            } else {
                origin.y + line * n as f32 + shift
            };
            out.push(Shape::galley(pos2(x, y), galley, color));
        }
    }
}

/// Dessine la page `page` de `pages` dans `dst` (rectangle aux proportions
/// du papier, voir [`page_rect`]) : fond, en-tête et pied de page, échelle
/// et titres de colonnes, lignes de la bande, légende ; comme le PDF
/// (`plannercraft-pdf`, `draw_page`), sans la ligne « (suite) ».
#[allow(
    clippy::too_many_lines,
    reason = "suite des zones de la page, dans l'ordre du PDF"
)]
pub fn paint_page(painter: &Painter, pages: &Pages, page: &Page, dst: Rect, inter: bool) {
    if !dst.is_finite() || dst.width() <= 0.0 {
        return;
    }
    let scene = &pages.scene;
    let s = dst.width() / pt(pages.paper_pt.0.max(1));
    let [mt, mr, mb, ml] = pages.margins_pt.map(|m| pt(m) * s);
    let k = PT_PER_16 * pt(pages.scale) / 10_000.0 * s;
    let bg = paint_of(&scene.paints, Class::Bg)
        .fill
        .map_or(Color32::WHITE, color);
    let ink = paint_of(&scene.paints, Class::Text)
        .fill
        .map_or(Color32::BLACK, color);
    let mut shapes = vec![
        Shape::rect_filled(dst, 0.0, bg),
        Shape::rect_stroke(
            dst,
            0.0,
            Stroke::new(1.0, Color32::GRAY),
            egui::StrokeKind::Outside,
        ),
    ];
    let content_w = dst.width() - ml - mr;
    let header = pt(pages.header_pt) * s;
    let footer = pt(pages.footer_pt) * s;
    let gap = pt(SLOT_GAP_PT) * s;
    let title = pages
        .title_block
        .then(|| paint_of(&scene.paints, Class::Title).fill.map_or(ink, color));
    slot_shapes(
        painter,
        &page.header,
        pos2(dst.left() + ml, dst.top() + mt),
        content_w,
        s,
        title,
        ink,
        inter,
        &mut shapes,
    );
    if footer > 0.0 {
        slot_shapes(
            painter,
            &page.footer,
            pos2(dst.left() + ml, dst.bottom() - mb - footer),
            content_w,
            s,
            None,
            ink,
            inter,
            &mut shapes,
        );
    }
    painter.extend(shapes);
    let body_top = dst.top() + mt + if header > 0.0 { header + gap } else { 0.0 };
    // Colonnes : table répétée à gauche, puis la tranche du Gantt.
    let (x0, x1) = page.x16;
    let mut columns: Vec<(i64, i64, f32)> = Vec::new();
    if page.table && x0 >= scene.table16 {
        columns.push((0, scene.table16, dst.left() + ml));
        columns.push((x0, x1, dst.left() + ml + i64_to_f32(scene.table16) * k));
    } else {
        columns.push((x0, x1, dst.left() + ml));
    }
    let mut y = body_top;
    // Chronologie en tête de la première page (H-UI-82, H-R16), sur toute
    // la largeur imprimable, comme le PDF (`draw_page`).
    if page.timeline
        && let Some(tl) = &pages.timeline
    {
        let origin = pos2(dst.left() + ml, y);
        let h = i64_to_f32(pages.timeline16.1) * k;
        let clip = Rect::from_min_size(origin, egui::vec2(content_w.max(0.0), h.max(0.0)));
        let sub = painter.with_clip_rect(clip.intersect(painter.clip_rect()));
        let pr = Projection {
            src: (0, 0),
            dst: origin,
            k,
        };
        let mut local = Vec::new();
        items_shapes(&sub, &tl.items, &Ink::of(scene, inter), &pr, &mut local);
        sub.extend(local);
        y += i64_to_f32(pages.timeline16.1.saturating_add(TIMELINE_GAP16)) * k;
    }
    let band = |sy0: i64, sy1: i64, y: f32, filter: &dyn Fn(&Item) -> bool| {
        for &(sx0, sx1, ox) in &columns {
            let clip = Rect::from_min_max(
                pos2(ox, y),
                pos2(
                    ox + i64_to_f32(sx1.saturating_sub(sx0)) * k,
                    y + i64_to_f32(sy1.saturating_sub(sy0)) * k,
                ),
            );
            let sub = painter.with_clip_rect(clip.intersect(painter.clip_rect()));
            let pr = Projection {
                src: (sx0, sy0),
                dst: pos2(ox, y),
                k,
            };
            let table_only = sx1 <= scene.table16 && sx0 == 0;
            let keep = |it: &Item| {
                filter(it)
                    && match it.clip {
                        Clip::Table => sx0 < scene.table16,
                        Clip::Gantt => !table_only,
                        Clip::None => true,
                    }
            };
            let mut local = Vec::new();
            region_shapes(&sub, scene, &pr, inter, &keep, &mut local);
            sub.extend(local);
        }
    };
    band(0, scene.header16, y, &|it| it.part == Part::Fixed);
    y += i64_to_f32(scene.header16) * k;
    let first = scene.rows.get(page.rows.start);
    let last = page.rows.end.checked_sub(1).and_then(|i| scene.rows.get(i));
    if let (Some(first), Some(last)) = (first, last) {
        let (sy0, sy1) = (first.y16, last.y16.saturating_add(last.h16));
        let range = page.rows.clone();
        band(sy0, sy1, y, &|it| match it.part {
            Part::Fixed => true,
            Part::Row(r) => usize::try_from(r).is_ok_and(|r| range.contains(&r)),
            Part::Cut | Part::Legend => false,
        });
        y += i64_to_f32(sy1.saturating_sub(sy0)) * k;
    }
    if page.legend && pages.legend16.1 > 0 {
        let (top, h) = pages.legend16;
        let pr = Projection {
            src: (0, top),
            dst: pos2(dst.left() + ml, y),
            k,
        };
        let clip = Rect::from_min_size(pos2(dst.left() + ml, y), egui::vec2(content_w, i64_to_f32(h) * k));
        let sub = painter.with_clip_rect(clip.intersect(painter.clip_rect()));
        let mut local = Vec::new();
        region_shapes(&sub, scene, &pr, inter, &|it| it.part == Part::Legend, &mut local);
        sub.extend(local);
    }
}
