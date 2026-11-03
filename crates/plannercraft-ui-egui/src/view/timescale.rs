//! En-tête à deux niveaux de l'échelle de temps (au-dessus du Gantt).
//!
//! Bande haute : unité grossière du zoom ; bande basse : unité fine. Les
//! cellules viennent de [`tier_cells`] (culling à la largeur visible), les
//! étiquettes sont centrées dans la partie visible de leur cellule et
//! tronquées par egui (`LayoutJob`).

use chrono::Datelike as _;
use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{Color32, FontId, Rect, Shape, Stroke, pos2};
use plannercraft_core::Weekday;

use super::FrameCtx;
use crate::format::DateOrder;
use crate::i18n::{Arg, TextId};
use crate::timescale::{TierCell, TimeAxis, tier_cells};

/// Taille de police des étiquettes de l'en-tête.
const HEADER_FONT_PX: f32 = 12.0;

/// Marge horizontale d'une étiquette dans sa cellule.
const LABEL_PAD: f32 = 2.0;

/// Largeur visible minimale pour dessiner une étiquette (sinon une cellule
/// partielle au bord n'afficherait que « … »).
const MIN_LABEL_W: f32 = 24.0;

/// Dessine les deux niveaux de l'en-tête dans `rect`, selon `axis`.
#[allow(
    clippy::too_many_lines,
    reason = "niveaux de l'échelle, étiquettes et date d'état, dans l'ordre du tracé"
)]
pub fn paint_header(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, axis: &TimeAxis, rect: Rect) {
    if !rect.is_finite() || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let painter = ui.painter_at(rect);
    let palette = fc.palette;
    let week_start = fc
        .doc
        .map_or(Weekday::Monday, |doc| doc.project().options.week_start_day);
    let (top_unit, bottom_unit) = fc.state.zoom.tiers();
    let labels = fc.texts.format_labels();
    // Mois relatifs au démarrage (H-UI-67, J8 lot L15) : un niveau de plus
    // en haut, les trois niveaux se partagent l'en-tête.
    let relative = fc.task_view.style.style.scale.relative_months;
    let (rel_band, rect_two) = if relative {
        let cut = rect.top() + rect.height() / 3.0;
        (
            Some(Rect::from_min_max(rect.min, pos2(rect.right(), cut))),
            Rect::from_min_max(pos2(rect.left(), cut), rect.max),
        )
    } else {
        (None, rect)
    };
    let mid_y = rect_two.center().y;
    let top = Rect::from_min_max(rect_two.min, pos2(rect_two.right(), mid_y));
    let bottom = Rect::from_min_max(pos2(rect_two.left(), mid_y), rect_two.max);

    let mut shapes = vec![Shape::rect_filled(rect, 0.0, palette.header_bg)];
    let mut rel_texts = Vec::new();
    if let (Some(band), Some(doc)) = (rel_band, fc.doc) {
        let start = doc.schedule().project_start.day();
        let cells: Vec<TierCell> = crate::timescale::tier_spans(
            axis,
            crate::timescale::TierUnit::Month,
            week_start,
            (band.left(), band.right()),
        )
        .into_iter()
        .map(|(day, x0, x1)| TierCell {
            x0,
            x1,
            label: plannercraft_render::geometry::relative_month_label(
                plannercraft_render::geometry::relative_month(day, start),
            ),
        })
        .collect();
        band_shapes(band, &cells, palette.grid_major, &mut shapes);
        collect_labels(band, cells, &mut rel_texts);
        let y = band.bottom() - 0.5;
        shapes.push(Shape::line_segment(
            [pos2(band.left(), y), pos2(band.right(), y)],
            Stroke::new(1.0, palette.grid_major),
        ));
    }
    let bands = [
        (top, top_unit, palette.grid_major),
        (bottom, bottom_unit, palette.grid),
    ];
    let mut texts = Vec::new();
    // Étiquette de la date d'état (H-UI-54, J8 lot L11) : dans le niveau
    // bas, à droite du trait (à gauche s'il n'y a pas la place) ; les
    // étiquettes qu'elle recouvrirait ne sont pas écrites.
    let tag = status_tag(&painter, fc, axis, bottom);
    for (band, unit, line_color) in bands {
        let cells = tier_cells(axis, unit, week_start, (band.left(), band.right()), labels);
        band_shapes(band, &cells, line_color, &mut shapes);
        let before = texts.len();
        collect_labels(band, cells, &mut texts);
        if band == bottom
            && let Some((x0, x1, _)) = &tag
        {
            let kept: Vec<_> = texts
                .drain(before..)
                .filter(|(_, a, b, _)| *b <= *x0 - 2.0 || *a >= *x1 + 2.0)
                .collect();
            texts.extend(kept);
        }
    }
    // Séparation des deux bandes et bord inférieur de l'en-tête.
    let stroke = Stroke::new(1.0, palette.grid_major);
    shapes.push(Shape::line_segment(
        [pos2(rect.left(), mid_y), pos2(rect.right(), mid_y)],
        stroke,
    ));
    let bottom_y = rect.bottom() - 0.5;
    shapes.push(Shape::line_segment(
        [pos2(rect.left(), bottom_y), pos2(rect.right(), bottom_y)],
        stroke,
    ));

    let text_color = palette.text;
    // Mois relatifs en couleur d'accent (`cursor` du style), comme la scène.
    let accent = {
        let c = fc.task_view.style.style.tokens(fc.task_view.style.variant).cursor;
        Color32::from_rgb(c.0, c.1, c.2)
    };
    let colored = rel_texts
        .into_iter()
        .map(|t| (t, accent))
        .chain(texts.into_iter().map(|t| (t, text_color)));
    for ((band, x0, x1, label), text_color) in colored {
        let width = x1 - x0 - 2.0 * LABEL_PAD;
        if !width.is_finite() || width < MIN_LABEL_W {
            continue;
        }
        let mut job = LayoutJob::single_section(
            label,
            TextFormat::simple(FontId::proportional(HEADER_FONT_PX), Color32::PLACEHOLDER),
        );
        job.wrap = TextWrapping::truncate_at_width(width);
        let galley = painter.layout_job(job);
        let size = galley.size();
        let x = f32::midpoint(x0, x1) - size.x / 2.0;
        let y = band.center().y - size.y / 2.0;
        if x.is_finite() && y.is_finite() && !galley.is_empty() {
            shapes.push(Shape::galley(pos2(x, y), galley, text_color));
        }
    }
    if let Some((x0, _, galley)) = tag {
        let y = bottom.center().y - galley.size().y / 2.0;
        shapes.push(Shape::galley(pos2(x0, y), galley, Color32::PLACEHOLDER));
    }
    fc.stats.shapes = fc.stats.shapes.saturating_add(shapes.len());
    painter.extend(shapes);
}

/// Étiquette « État 02/11 » de la date d'état si le style la demande
/// (`scale.status_label`) et que la date est dans la bande : abscisses
/// gauche et droite et texte mis en page (couleur de la date d'état).
fn status_tag(
    painter: &egui::Painter,
    fc: &FrameCtx<'_>,
    axis: &TimeAxis,
    band: Rect,
) -> Option<(f32, f32, std::sync::Arc<egui::Galley>)> {
    let screen = &fc.task_view.style;
    if !screen.style.scale.status_label {
        return None;
    }
    let doc = fc.doc?;
    let status = doc.project().effective_status_date();
    let x = axis.x(status).round() + 0.5;
    if !x.is_finite() || x < band.left() || x > band.right() {
        return None;
    }
    let date = status.day().to_naive();
    let date = match fc.texts.format_labels().date_order {
        DateOrder::MonthDayYear => format!("{:02}/{:02}", date.month(), date.day()),
        DateOrder::DayMonthYear => format!("{:02}/{:02}", date.day(), date.month()),
    };
    let text = fc
        .texts
        .get_args(TextId::GanttStatusLabel, &[("date", Arg::Str(date.into()))]);
    let ink = fc.palette.status_date;
    let galley = painter.layout_no_wrap(text, FontId::proportional(HEADER_FONT_PX), ink);
    let w = galley.size().x;
    let x0 = if x + 3.0 + w <= band.right() {
        x + 3.0
    } else {
        x - 3.0 - w
    };
    Some((x0, x0 + w, galley))
}

/// Séparateurs verticaux au bord gauche de chaque cellule visible.
fn band_shapes(band: Rect, cells: &[TierCell], color: Color32, shapes: &mut Vec<Shape>) {
    let stroke = Stroke::new(1.0, color);
    for cell in cells {
        let x = cell.x0.round() + 0.5;
        if x.is_finite() && x > band.left() && x < band.right() {
            shapes.push(Shape::line_segment(
                [pos2(x, band.top()), pos2(x, band.bottom())],
                stroke,
            ));
        }
    }
}

/// Étiquettes à dessiner : partie visible de chaque cellule (étiquette
/// « collante » d'une cellule partiellement visible).
fn collect_labels(band: Rect, cells: Vec<TierCell>, out: &mut Vec<(Rect, f32, f32, String)>) {
    for cell in cells {
        if cell.label.is_empty() {
            continue;
        }
        let x0 = cell.x0.max(band.left());
        let x1 = cell.x1.min(band.right());
        if x0.is_finite() && x1.is_finite() && x1 > x0 {
            out.push((band, x0, x1, cell.label));
        }
    }
}
