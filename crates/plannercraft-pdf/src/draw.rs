//! Tracé d'une page : régions de la scène (échelle répétée, parent
//! « (suite) », lignes de la bande, troncature, légende) placées sur le
//! papier, chacune découpée à sa zone ; chronologie en tête de la première
//! page (H-UI-82) ; en-tête et pied de page. Les
//! primitives sont celles du SVG (`Prim`), lues dans l'ordre de la scène.

use krilla::color::rgb;
use krilla::geom::{Path, PathBuilder, Point, Rect, Transform};
use krilla::num::NormalizedF32;
use krilla::paint::{Fill, FillRule, Stroke, StrokeDash};
use krilla::surface::Surface;
use plannercraft_render::geometry::{
    Anchor, Class, Clip, FontRef, FontWeight, Item, Paint, Part, Prim, Scene, fit_text, pattern_segments,
};
use plannercraft_render::paginate::{Page, Pages, SLOT_GAP_PT, SLOT_LINE_PT, TIMELINE_GAP16, TITLE_LINE_PT};
use plannercraft_render::style::Rgb;
use plannercraft_render::{InterMetrics, Lang, TextMeasure};

use crate::fonts::{Fonts, to_f32};

/// Corps des en-têtes et pieds de page : 12 px (9 pt).
const SLOT_FONT: FontRef = FontRef {
    weight: FontWeight::Regular,
    size16: 192,
};

/// Police du titre du cartouche (H-UI-66) : Inter `SemiBold` 22 px.
const TITLE_FONT: FontRef = FontRef {
    weight: FontWeight::SemiBold,
    size16: 352,
};

/// Police de la première ligne de la zone droite du cartouche.
const STRONG_SLOT_FONT: FontRef = FontRef {
    weight: FontWeight::SemiBold,
    size16: 192,
};

/// Points par seizième de pixel à 100 % (3/64).
const PT_PER_16: f64 = 3.0 / 64.0;

/// Flottant vers `f32` du PDF.
#[allow(
    clippy::cast_possible_truncation,
    reason = "coordonnées du PDF, flottantes par nature"
)]
const fn f(v: f64) -> f32 {
    v as f32
}

/// Entier vers `f64`.
#[allow(
    clippy::cast_precision_loss,
    reason = "coordonnées du PDF, flottantes par nature"
)]
const fn d(v: i64) -> f64 {
    v as f64
}

/// Couleur krilla.
fn color(c: Rgb) -> rgb::Color {
    rgb::Color::new(c.0, c.1, c.2)
}

fn fill_of(c: Rgb) -> Fill {
    Fill {
        paint: color(c).into(),
        opacity: NormalizedF32::ONE,
        rule: FillRule::NonZero,
    }
}

fn stroke_of(p: &Paint, c: Rgb) -> Stroke {
    Stroke {
        paint: color(c).into(),
        width: to_f32(p.width16.max(1)),
        dash: p.dash16.map(|(a, b)| StrokeDash {
            array: vec![to_f32(a), to_f32(b)],
            offset: 0.0,
        }),
        ..Stroke::default()
    }
}

/// Rectangle de scène (`None` : vide).
fn rect(x0: i64, y0: i64, x1: i64, y1: i64) -> Option<Rect> {
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Rect::from_ltrb(to_f32(x0), to_f32(y0), to_f32(x1), to_f32(y1))
}

fn rect_path(r: Rect) -> Option<Path> {
    let mut b = PathBuilder::new();
    b.push_rect(r);
    b.finish()
}

/// Région de la scène placée sur la page.
#[derive(Clone, Copy, Debug)]
struct Region {
    /// Zone de la scène (1/16 px), fins exclues.
    sx0: i64,
    sx1: i64,
    sy0: i64,
    sy1: i64,
    /// Coin haut gauche sur la page (pt).
    ox: f64,
    oy: f64,
    /// Points par unité de scène.
    k: f64,
}

/// Contexte de tracé d'une page.
pub struct PageCtx<'a> {
    /// Pages (scène comprise).
    pub pages: &'a Pages,
    /// Polices.
    pub fonts: &'a mut Fonts,
    /// Langue (« (suite) »).
    pub lang: Lang,
}

/// Peinture d'une classe (texte courant à défaut).
fn paint_of(scene: &Scene, class: Class) -> Paint {
    scene.paints.get(&class).copied().unwrap_or(Paint {
        fill: scene.paints.get(&Class::Text).and_then(|p| p.fill),
        stroke: None,
        width16: 16,
        dash16: None,
    })
}

/// Couleur du texte courant.
fn text_color(scene: &Scene) -> Rgb {
    scene
        .paints
        .get(&Class::Text)
        .and_then(|p| p.fill)
        .unwrap_or(Rgb(0, 0, 0))
}

/// Trace une page complète.
#[allow(
    clippy::too_many_lines,
    reason = "régions de la page dans l'ordre vertical, lues d'un bloc"
)]
pub fn draw_page(surface: &mut Surface<'_>, ctx: &mut PageCtx<'_>, page: &Page) {
    let pages = ctx.pages;
    let scene = &pages.scene;
    let (w_pt, h_pt) = (f64::from(pages.paper_pt.0), f64::from(pages.paper_pt.1));
    let [mt, mr, mb, ml] = pages.margins_pt.map(f64::from);
    let k = PT_PER_16 * f64::from(pages.scale) / 10_000.0;
    let content_w = (w_pt - ml - mr).max(1.0);
    // Fond de la page.
    let bg = paint_of(scene, Class::Bg).fill.unwrap_or(Rgb(0xFF, 0xFF, 0xFF));
    if let Some(r) = Rect::from_xywh(0.0, 0.0, f(w_pt), f(h_pt)).and_then(rect_path) {
        surface.set_stroke(None);
        surface.set_fill(Some(fill_of(bg)));
        surface.draw_path(&r);
    }
    let header_pt = f64::from(pages.header_pt);
    let footer_pt = f64::from(pages.footer_pt);
    let gap = f64::from(SLOT_GAP_PT);
    let body_top = mt + if header_pt > 0.0 { header_pt + gap } else { 0.0 };
    let body_bottom = h_pt - mb - if footer_pt > 0.0 { footer_pt + gap } else { 0.0 };
    // Colonnes : table répétée à gauche, puis la tranche de Gantt.
    let (x0, x1) = page.x16;
    let mut columns: Vec<(i64, i64, f64)> = Vec::new();
    if page.table && x0 >= scene.table16 {
        columns.push((0, scene.table16, ml));
        columns.push((x0, x1, ml + d(scene.table16) * k));
    } else {
        columns.push((x0, x1, ml));
    }
    let mut y = body_top;
    // Chronologie en tête de la première page (H-UI-82, H-R16), sur toute
    // la largeur imprimable.
    if page.timeline
        && let Some(tl) = &pages.timeline
    {
        let r = Region {
            sx0: 0,
            sx1: tl.width16,
            sy0: 0,
            sy1: pages.timeline16.1,
            ox: ml,
            oy: y,
            k,
        };
        draw_items(surface, ctx, &r, &tl.items, &|_| true);
        y += d(pages.timeline16.1.saturating_add(TIMELINE_GAP16)) * k;
    }
    let place = |sy0: i64, sy1: i64, oy: f64| {
        columns.iter().map(move |&(sx0, sx1, ox)| Region {
            sx0,
            sx1,
            sy0,
            sy1,
            ox,
            oy,
            k,
        })
    };
    // Échelle et titres de colonnes, répétés (H-UI-49).
    for r in place(0, scene.header16, y) {
        draw_region(surface, ctx, &r, &|it| it.part == Part::Fixed);
    }
    y += d(scene.header16) * k;
    // Parent rappelé « (suite) » (H-UI-48).
    if let Some(pi) = page.continued_parent
        && let Some(parent) = scene.rows.get(pi)
    {
        let part = Part::Row(u32::try_from(pi).unwrap_or(u32::MAX));
        let (sy0, sy1) = (parent.y16, parent.y16.saturating_add(parent.h16));
        for r in place(sy0, sy1, y) {
            draw_region(surface, ctx, &r, &|it| {
                it.part == Part::Fixed
                    || it.part == part
                        && !matches!(
                            it.class,
                            Class::Link | Class::LinkCritical | Class::Arrow | Class::ArrowCritical
                        )
            });
        }
        if let Some(first) = columns.first() {
            continued_suffix(
                surface,
                ctx,
                pi,
                &Region {
                    sx0: first.0,
                    sx1: scene.table16.min(first.1),
                    sy0,
                    sy1,
                    ox: first.2,
                    oy: y,
                    k,
                },
            );
        }
        y += d(parent.h16) * k;
    }
    // Lignes de la bande.
    let rows_y = page
        .rows
        .clone()
        .next()
        .and_then(|i| scene.rows.get(i))
        .map(|r| r.y16);
    let rows_end = page
        .rows
        .end
        .checked_sub(1)
        .and_then(|i| scene.rows.get(i))
        .map(|r| r.y16.saturating_add(r.h16));
    if let (Some(sy0), Some(sy1)) = (rows_y, rows_end) {
        let range = page.rows.clone();
        for r in place(sy0, sy1, y) {
            draw_region(surface, ctx, &r, &|it| match it.part {
                Part::Fixed => true,
                Part::Row(i) => usize::try_from(i).is_ok_and(|i| range.contains(&i)),
                Part::Cut | Part::Legend => false,
            });
        }
        y += d(sy1.saturating_sub(sy0)) * k;
    }
    // Ligne de troncature.
    let (legend_top, legend_h) = pages.legend16;
    if page.cut && pages.cut16 > 0 {
        let sy0 = legend_top.saturating_sub(pages.cut16);
        for r in place(sy0, legend_top, y) {
            draw_region(surface, ctx, &r, &|it| it.part == Part::Cut);
        }
    }
    // Légende, en bas du corps, réduite si elle dépasse la largeur.
    if page.legend && legend_h > 0 {
        let extent = legend_extent(scene).max(1);
        let kl = k.min(content_w / d(extent));
        let oy = (body_bottom - d(legend_h) * kl).max(y);
        let r = Region {
            sx0: 0,
            sx1: extent.saturating_add(16),
            sy0: legend_top,
            sy1: legend_top.saturating_add(legend_h),
            ox: ml,
            oy,
            k: kl,
        };
        draw_region(surface, ctx, &r, &|it| it.part == Part::Legend);
    }
    // En-tête et pied de page.
    let color = text_color(scene);
    let title = pages
        .title_block
        .then(|| paint_of(scene, Class::Title).fill.unwrap_or(color));
    slots(
        surface,
        ctx.fonts,
        &page.header,
        (ml, mt, content_w),
        color,
        title,
    );
    if footer_pt > 0.0 {
        slots(
            surface,
            ctx.fonts,
            &page.footer,
            (ml, h_pt - mb - footer_pt, content_w),
            color,
            None,
        );
    }
}

/// Abscisse droite des primitives de la légende.
fn legend_extent(scene: &Scene) -> i64 {
    scene
        .items
        .iter()
        .filter(|it| it.part == Part::Legend)
        .map(|it| match &it.prim {
            Prim::Text { x16, text, font, .. } => x16.saturating_add(InterMetrics.width16(text, *font)),
            Prim::Rect { r, .. } | Prim::Diamond { r } | Prim::Bracket { r, .. } | Prim::Hatch { r, .. } => {
                r.right()
            }
            _ => 0,
        })
        .max()
        .unwrap_or(0)
}

/// « (suite) » après le nom du parent rappelé.
fn continued_suffix(surface: &mut Surface<'_>, ctx: &mut PageCtx<'_>, pi: usize, r: &Region) {
    let pages = ctx.pages;
    let scene = &pages.scene;
    let part = Part::Row(u32::try_from(pi).unwrap_or(u32::MAX));
    let Some((x, y, font)) = scene.items.iter().find_map(|it| match &it.prim {
        Prim::Text {
            x16,
            y16,
            text,
            font,
            anchor: Anchor::Start,
        } if it.part == part && it.clip == Clip::Table => {
            Some((x16.saturating_add(InterMetrics.width16(text, *font)), *y16, *font))
        }
        _ => None,
    }) else {
        return;
    };
    let suffix = match ctx.lang {
        Lang::Fr => " (suite)",
        Lang::En => " (continued)",
    };
    let muted = paint_of(scene, Class::TextMuted)
        .fill
        .unwrap_or_else(|| text_color(scene));
    with_region(surface, r, r.sx0, r.sx1, |s| {
        text(s, ctx.fonts, suffix, x, y, font, Anchor::Start, muted);
    });
}

/// Pose la transformation et la découpe de `r` restreinte à `[cx0, cx1)`,
/// trace, puis retire les deux.
fn with_region(
    surface: &mut Surface<'_>,
    r: &Region,
    cx0: i64,
    cx1: i64,
    draw: impl FnOnce(&mut Surface<'_>),
) {
    let (lo, hi) = (cx0.max(r.sx0), cx1.min(r.sx1));
    let Some(clip) = rect(lo, r.sy0, hi, r.sy1).and_then(rect_path) else {
        return;
    };
    let tx = r.ox - d(r.sx0) * r.k;
    let ty = r.oy - d(r.sy0) * r.k;
    surface.push_transform(&Transform::from_row(f(r.k), 0.0, 0.0, f(r.k), f(tx), f(ty)));
    surface.push_clip_path(&clip, &FillRule::NonZero);
    draw(surface);
    surface.pop();
    surface.pop();
}

/// Trace les primitives retenues de la région, groupées par découpe
/// (table, Gantt, aucune), dans l'ordre de la scène.
fn draw_region(surface: &mut Surface<'_>, ctx: &mut PageCtx<'_>, r: &Region, keep: &dyn Fn(&Item) -> bool) {
    let pages = ctx.pages;
    draw_items(surface, ctx, r, &pages.scene.items, keep);
}

/// Trace les primitives retenues de `items` dans la région (peintures de
/// la scène), groupées par découpe, dans l'ordre.
fn draw_items(
    surface: &mut Surface<'_>,
    ctx: &mut PageCtx<'_>,
    r: &Region,
    items: &[Item],
    keep: &dyn Fn(&Item) -> bool,
) {
    let pages = ctx.pages;
    let scene = &pages.scene;
    let fonts = &mut *ctx.fonts;
    let mut start = 0_usize;
    while start < items.len() {
        let Some(clip) = items.get(start).map(|it| it.clip) else {
            break;
        };
        let end = items
            .iter()
            .skip(start)
            .position(|it| it.clip != clip)
            .map_or(items.len(), |n| start.saturating_add(n));
        let group = items.get(start..end).unwrap_or(&[]);
        let (cx0, cx1) = match clip {
            Clip::None => (i64::MIN, i64::MAX),
            Clip::Table => (0, scene.table16),
            Clip::Gantt => (scene.table16, i64::MAX),
        };
        // Primitives hors de la zone visible omises (taille du fichier,
        // pages en largeur).
        let (lo, hi) = (cx0.max(r.sx0), cx1.min(r.sx1));
        let shown = |it: &&Item| {
            keep(it)
                && bbox(it).is_none_or(|(x0, y0, x1, y1)| x1 >= lo && x0 <= hi && y1 >= r.sy0 && y0 <= r.sy1)
        };
        if lo < hi && group.iter().any(|it| shown(&it)) {
            with_region(surface, r, cx0, cx1, |s| {
                for it in group.iter().filter(shown) {
                    draw_item(s, fonts, scene, it);
                }
            });
        }
        start = end;
    }
}

/// Boîte englobante approchée d'une primitive (`None` : inconnue, toujours
/// tracée), marge d'un trait comprise.
fn bbox(it: &Item) -> Option<(i64, i64, i64, i64)> {
    const M: i64 = 64;
    let pts = |p: &[(i64, i64)]| {
        let xs = p.iter().map(|q| q.0);
        let ys = p.iter().map(|q| q.1);
        Some((
            xs.clone().min()?.saturating_sub(M),
            ys.clone().min()?.saturating_sub(M),
            xs.max()?.saturating_add(M),
            ys.max()?.saturating_add(M),
        ))
    };
    match &it.prim {
        Prim::Rect { r, .. } | Prim::Diamond { r } | Prim::Bracket { r, .. } | Prim::Hatch { r, .. } => {
            Some((
                r.x.saturating_sub(M),
                r.y.saturating_sub(M),
                r.right().saturating_add(M),
                r.bottom().saturating_add(M),
            ))
        }
        Prim::Polyline { points } | Prim::Polygon { points } => pts(points),
        Prim::Segments { segs } => {
            let all: Vec<(i64, i64)> = segs.iter().flat_map(|&(a, b, c, d)| [(a, b), (c, d)]).collect();
            pts(&all)
        }
        Prim::Text {
            x16,
            y16,
            text,
            font,
            anchor,
        } => {
            let w = InterMetrics.width16(text, *font);
            let size = i64::from(font.size16);
            let x0 = match anchor {
                Anchor::Start => *x16,
                Anchor::End => x16.saturating_sub(w),
            };
            Some((
                x0.saturating_sub(M),
                y16.saturating_sub(size).saturating_sub(M),
                x0.saturating_add(w).saturating_add(M),
                y16.saturating_add(size / 2).saturating_add(M),
            ))
        }
    }
}

/// Pose remplissage et trait d'une peinture.
fn set_paint(surface: &mut Surface<'_>, p: &Paint, fill: bool, stroke: bool) -> bool {
    let fill = if fill { p.fill.map(fill_of) } else { None };
    let stroke = if stroke {
        p.stroke.map(|c| stroke_of(p, c))
    } else {
        None
    };
    let any = fill.is_some() || stroke.is_some();
    surface.set_fill(fill);
    surface.set_stroke(stroke);
    any
}

fn poly_path(points: &[(i64, i64)], close: bool) -> Option<Path> {
    let mut b = PathBuilder::new();
    for (i, &(x, y)) in points.iter().enumerate() {
        if i == 0 {
            b.move_to(to_f32(x), to_f32(y));
        } else {
            b.line_to(to_f32(x), to_f32(y));
        }
    }
    if close {
        b.close();
    }
    b.finish()
}

fn segments_path(segs: &[(i64, i64, i64, i64)]) -> Option<Path> {
    let mut b = PathBuilder::new();
    for &(x0, y0, x1, y1) in segs {
        b.move_to(to_f32(x0), to_f32(y0));
        b.line_to(to_f32(x1), to_f32(y1));
    }
    b.finish()
}

/// Rectangle à coins arrondis (quarts de cercle approchés par des
/// courbes de Bézier cubiques).
#[allow(
    clippy::many_single_char_names,
    reason = "géométrie d'un rectangle : x, y, w, h, rayon"
)]
fn rounded_path(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Option<Path> {
    const KAPPA: f32 = 0.552_284_8;
    let r = radius.min(w / 2.0).min(h / 2.0).max(0.0);
    let c = r * KAPPA;
    let (x1, y1) = (x + w, y + h);
    let mut b = PathBuilder::new();
    b.move_to(x + r, y);
    b.line_to(x1 - r, y);
    b.cubic_to(x1 - r + c, y, x1, y + r - c, x1, y + r);
    b.line_to(x1, y1 - r);
    b.cubic_to(x1, y1 - r + c, x1 - r + c, y1, x1 - r, y1);
    b.line_to(x + r, y1);
    b.cubic_to(x + r - c, y1, x, y1 - r + c, x, y1 - r);
    b.line_to(x, y + r);
    b.cubic_to(x, y + r - c, x + r - c, y, x + r, y);
    b.close();
    b.finish()
}

/// Trace une primitive.
fn draw_item(surface: &mut Surface<'_>, fonts: &mut Fonts, scene: &Scene, it: &Item) {
    let p = paint_of(scene, it.class);
    match &it.prim {
        Prim::Rect { r, radius16 } => {
            let path = if *radius16 > 0 {
                rounded_path(
                    to_f32(r.x),
                    to_f32(r.y),
                    to_f32(r.w),
                    to_f32(r.h),
                    to_f32(*radius16),
                )
            } else {
                rect(r.x, r.y, r.right(), r.bottom()).and_then(rect_path)
            };
            if let Some(path) = path
                && set_paint(surface, &p, true, true)
            {
                surface.draw_path(&path);
            }
        }
        Prim::Diamond { r } => {
            let (cx, cy) = (r.x.saturating_add(r.w / 2), r.y.saturating_add(r.h / 2));
            let pts = [(cx, r.y), (r.right(), cy), (cx, r.bottom()), (r.x, cy)];
            if let Some(path) = poly_path(&pts, true)
                && set_paint(surface, &p, true, true)
            {
                surface.draw_path(&path);
            }
        }
        Prim::Bracket { r, thick16, tip_w16 } => {
            let (x0, x1, top) = (r.x, r.right(), r.y);
            let bottom = top.saturating_add(*thick16);
            let tip = r.bottom();
            let mut b = PathBuilder::new();
            for pts in [
                [(x0, top), (x1, top), (x1, bottom), (x0, bottom)].as_slice(),
                [(x0, top), (x0.saturating_add(*tip_w16), top), (x0, tip)].as_slice(),
                [(x1.saturating_sub(*tip_w16), top), (x1, top), (x1, tip)].as_slice(),
            ] {
                for (i, &(x, y)) in pts.iter().enumerate() {
                    if i == 0 {
                        b.move_to(to_f32(x), to_f32(y));
                    } else {
                        b.line_to(to_f32(x), to_f32(y));
                    }
                }
                b.close();
            }
            if let Some(path) = b.finish()
                && set_paint(surface, &p, true, true)
            {
                surface.draw_path(&path);
            }
        }
        Prim::Polyline { points } => {
            if let Some(path) = poly_path(points, false)
                && set_paint(surface, &p, false, true)
            {
                surface.draw_path(&path);
            }
        }
        Prim::Polygon { points } => {
            if let Some(path) = poly_path(points, true)
                && set_paint(surface, &p, true, true)
            {
                surface.draw_path(&path);
            }
        }
        Prim::Segments { segs } => {
            if let Some(path) = segments_path(segs)
                && set_paint(surface, &p, false, true)
            {
                surface.draw_path(&path);
            }
        }
        Prim::Hatch { r, dots, diamond } => {
            let segs = pattern_segments(*r, *dots, *diamond);
            if let Some(path) = segments_path(&segs)
                && set_paint(surface, &p, false, true)
            {
                surface.draw_path(&path);
            }
        }
        Prim::Text {
            x16,
            y16,
            text: s,
            font,
            anchor,
        } => {
            let c = p.fill.unwrap_or_else(|| text_color(scene));
            text(surface, fonts, s, *x16, *y16, *font, *anchor, c);
        }
    }
}

/// Texte posé glyphe par glyphe (H-R13), ancré au début ou à la fin.
#[allow(
    clippy::too_many_arguments,
    reason = "texte complet : surface, polices, chaîne, position, police, ancrage, couleur"
)]
fn text(
    surface: &mut Surface<'_>,
    fonts: &mut Fonts,
    s: &str,
    x16: i64,
    y16: i64,
    font: FontRef,
    anchor: Anchor,
    c: Rgb,
) {
    if s.is_empty() {
        return;
    }
    let runs = fonts.runs(s, font);
    let total: i64 = runs.iter().map(|r| r.width16).fold(0, i64::saturating_add);
    let mut x = match anchor {
        Anchor::Start => x16,
        Anchor::End => x16.saturating_sub(total),
    };
    surface.set_stroke(None);
    surface.set_fill(Some(fill_of(c)));
    let size = f32::from(font.size16.max(1));
    for run in runs {
        surface.draw_glyphs(
            Point::from_xy(to_f32(x), to_f32(y16)),
            &run.glyphs,
            run.font,
            &run.text,
            size,
            false,
        );
        x = x.saturating_add(run.width16);
    }
}

/// Trois zones d'en-tête ou de pied de page (gauche, centre, droite), une
/// ligne tous les 12 pt, textes tronqués à leur tiers avec « … ». Avec un
/// cartouche (`title` : couleur du titre, H-UI-66), la première ligne de
/// la zone centrale est un titre sur 26 pt (Inter `SemiBold` 22 px) et la
/// première ligne de la zone droite est appuyée.
fn slots(
    surface: &mut Surface<'_>,
    fonts: &mut Fonts,
    zones: &[String; 3],
    (ox, oy, width): (f64, f64, f64),
    ink: Rgb,
    title: Option<Rgb>,
) {
    // Espace de tracé : 1/16 px à 100 %.
    let k = PT_PER_16;
    let width16 = (width / k).max(1.0);
    let third = width16 / 3.0;
    let line16 = f64::from(SLOT_LINE_PT) / k;
    let title16 = f64::from(TITLE_LINE_PT) / k;
    let pad = 8.0_f64;
    let room = round16(third - 2.0 * pad);
    // Le titre, s'il y en a un, décale les autres lignes de toutes les zones.
    let shift = if title.is_some() && !zones[1].trim().is_empty() {
        title16 - line16
    } else {
        0.0
    };
    surface.push_transform(&Transform::from_row(f(k), 0.0, 0.0, f(k), f(ox), f(oy)));
    for (z, zone) in zones.iter().enumerate() {
        for (n, line) in zone.lines().enumerate() {
            let (font, color) = match (title, z, n) {
                (Some(t), 1, 0) => (TITLE_FONT, t),
                (Some(_), 2, 0) => (STRONG_SLOT_FONT, ink),
                _ => (SLOT_FONT, ink),
            };
            let Some((shown, w16)) = fit_text(line, room, font, &InterMetrics) else {
                continue;
            };
            if shown.is_empty() {
                continue;
            }
            #[allow(clippy::cast_precision_loss, reason = "rang de ligne, petit entier")]
            let baseline = if title.is_some() && z == 1 && n == 0 && shift > 0.0 {
                title16 - 6.0 / k
            } else {
                line16 * (n as f64 + 1.0) - 3.0 / k + shift
            };
            let left = match z {
                0 => 0.0,
                1 => (width16 - d(w16)) / 2.0,
                _ => width16 - d(w16),
            };
            text(
                surface,
                fonts,
                &shown,
                round16(left),
                round16(baseline),
                font,
                Anchor::Start,
                color,
            );
        }
    }
    surface.pop();
}

/// Arrondi d'une coordonnée en 1/16 px (bornée par la page).
#[allow(
    clippy::cast_possible_truncation,
    reason = "valeur finie bornée par la taille du papier"
)]
fn round16(v: f64) -> i64 {
    if v.is_finite() {
        v.round().clamp(-1.0e9, 1.0e9) as i64
    } else {
        0
    }
}
