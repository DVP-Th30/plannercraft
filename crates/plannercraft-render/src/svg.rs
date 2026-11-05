//! Écriture SVG de la scène (ADR-0016 D5, lot L4 de J8).
//!
//! `svg.rs` ne lit que la [`Scene`] construite par `layout` : une classe CSS
//! par [`Class`], peintures tirées du style (fin de la palette recopiée,
//! H-R9 soldée). Document déterministe : ordre des primitives = ordre de la
//! scène, coordonnées entières en 1/16 px écrites avec quatre décimales
//! fixes, aucune police système mesurée (largeurs d'Inter, H-R11), tous les
//! textes échappés. Bornes : `max_rows` lignes, `max_days` jours,
//! `max_bytes` octets (arrêt à la ligne entière précédente). Hypothèses
//! d'affichage H-R (`docs/features/render.md`).

use std::fmt::Write as _;

use plannercraft_core::{Project, Schedule};

use crate::axis::push_fixed16;
use crate::geometry::{
    Anchor, BarTextSource, Class, Clip, FontWeight, Item, NoTexts, Paint, Part, Prim, Scene, Viewport,
    layout, pattern_segments,
};
use crate::paginate::{PrintTimeline, TIMELINE_GAP16};
use crate::request::{GanttRequest, RenderReport, Theme};
use crate::style::{BarRole, FontChoice, PlanStyle, TemplateId, Variant, template};
use crate::timeline::{TIMELINE_HEIGHT16, TimelineInput};

/// Nom CSS d'une classe (vide : texte courant, sans attribut) ; phases
/// numérotées (`ph0b`, `pr0`, H-UI-65).
fn class_name(c: Class) -> std::borrow::Cow<'static, str> {
    match c {
        Class::Phase(i, r) => format!("ph{i}{}", bar_name(r)).into(),
        Class::PhaseRow(i) => format!("pr{i}").into(),
        Class::TaskText(i) => format!("tx{i}").into(),
        Class::TaskFill(i) => format!("tf{i}").into(),
        Class::TextOnBar(r, None) => format!("ti{}", bar_name(r)).into(),
        Class::TextOnBar(r, Some(i)) => format!("ti{i}{}", bar_name(r)).into(),
        other => fixed_class_name(other).into(),
    }
}

/// Nom CSS des classes fixes.
const fn fixed_class_name(c: Class) -> &'static str {
    match c {
        Class::Title => "tt",
        Class::Bg => "bg",
        Class::HeaderBg => "hd",
        Class::Band(0) => "z0",
        Class::Band(_) => "z1",
        Class::NonWorking => "nw",
        Class::Grid => "gr",
        Class::GridMajor => "gm",
        Class::Splitter => "sp",
        Class::RowLine => "rl",
        Class::StatusLine => "sd",
        Class::StatusLabel => "sdt",
        Class::Today => "td",
        Class::Bar(r) => bar_name(r),
        Class::Pattern(r) => pattern_name(r),
        Class::Link => "l",
        Class::LinkCritical => "lc",
        Class::Arrow => "la",
        Class::ArrowCritical => "lac",
        Class::Text
        | Class::Phase(..)
        | Class::PhaseRow(_)
        | Class::TextOnBar(..)
        | Class::TaskText(_)
        | Class::TaskFill(_) => "",
        Class::TextStrong => "ts",
        Class::TextMuted => "tw",
        Class::TextCut => "tc",
    }
}

/// Classe d'une barre (`b`, `bc`, `s`, `sc`, `m`, `mc` comme J7).
const fn bar_name(r: BarRole) -> &'static str {
    match r {
        BarRole::Task => "b",
        BarRole::Critical => "bc",
        BarRole::Summary => "s",
        BarRole::SummaryCritical => "sc",
        BarRole::Milestone => "m",
        BarRole::MilestoneCritical => "mc",
        BarRole::Progress => "pg",
        BarRole::Baseline => "bl",
        BarRole::Inactive => "x",
        BarRole::Manual => "mn",
        BarRole::Slack => "sk",
        BarRole::Deadline => "dl",
    }
}

/// Classe du motif d'un rôle.
const fn pattern_name(r: BarRole) -> &'static str {
    match r {
        BarRole::Task => "pb",
        BarRole::Critical => "pbc",
        BarRole::Summary => "ps",
        BarRole::SummaryCritical => "psc",
        BarRole::Milestone => "pm",
        BarRole::MilestoneCritical => "pmc",
        BarRole::Progress => "ppg",
        BarRole::Baseline => "pbl",
        BarRole::Inactive => "px",
        BarRole::Manual => "pmn",
        BarRole::Slack => "psk",
        BarRole::Deadline => "pdl",
    }
}

/// Écrit `v` (1/16 px) à quatre décimales fixes.
fn n(out: &mut String, v: i64) {
    push_fixed16(out, v);
}

/// Écrit `v` (1/16 px) sans zéros finaux (« 1.5 », « 2 ») : épaisseurs et
/// tirets de la feuille de style.
fn short(out: &mut String, v: i64) {
    let mut s = String::new();
    push_fixed16(&mut s, v);
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    out.push_str(trimmed);
}

/// Écrit une liste de points `x,y x,y …`.
fn points(out: &mut String, pts: &[(i64, i64)]) {
    for (i, (x, y)) in pts.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        n(out, *x);
        out.push(',');
        n(out, *y);
    }
}

/// Caractère échappé pour un texte XML : `&<>"'` en entités, tabulation et
/// fins de ligne en espace, caractères interdits par XML 1.0 en U+FFFD.
fn push_escaped_char(out: &mut String, c: char) {
    match c {
        '&' => out.push_str("&amp;"),
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        '"' => out.push_str("&quot;"),
        '\'' => out.push_str("&apos;"),
        '\t' | '\n' | '\r' => out.push(' '),
        c if c < ' ' || c == '\u{FFFE}' || c == '\u{FFFF}' => out.push('\u{FFFD}'),
        c => out.push(c),
    }
}

/// Texte échappé, tronqué à `max_chars` caractères suivis de « … ».
fn push_text(out: &mut String, s: &str, max_chars: usize) {
    for (i, c) in s.chars().enumerate() {
        if i >= max_chars {
            out.push('…');
            return;
        }
        push_escaped_char(out, c);
    }
}

/// Règle CSS d'une peinture.
fn css_paint(out: &mut String, p: &Paint) {
    out.push_str("fill:");
    match p.fill {
        Some(c) => out.push_str(&c.hex()),
        None => out.push_str("none"),
    }
    if let Some(c) = p.stroke {
        out.push_str(";stroke:");
        out.push_str(&c.hex());
        if p.width16 != 16 {
            out.push_str(";stroke-width:");
            short(out, p.width16);
        }
        if let Some((a, b)) = p.dash16 {
            out.push_str(";stroke-dasharray:");
            short(out, a);
            out.push(' ');
            short(out, b);
        }
    }
}

/// Feuille de style : police, puis une règle par classe utilisée.
fn style(out: &mut String, scene: &Scene) {
    let family = match scene.family {
        FontChoice::Interface => "sans-serif",
        FontChoice::Inter => "Inter,sans-serif",
    };
    let bold = match scene.family {
        FontChoice::Interface => "bold",
        FontChoice::Inter => "600",
    };
    out.push_str("<style>");
    let text = scene
        .paints
        .get(&Class::Text)
        .and_then(|p| p.fill)
        .map_or_else(|| "#000000".to_owned(), crate::style::Rgb::hex);
    let _ = write!(out, "text{{font-family:{family};font-size:");
    short(out, i64::from(scene.font.size16));
    let _ = write!(out, "px;fill:{text}}}");
    for (class, paint) in &scene.paints {
        let name = class_name(*class);
        if name.is_empty() {
            continue;
        }
        let _ = write!(out, ".{name}{{");
        match class {
            Class::TextStrong
            | Class::TextMuted
            | Class::TextCut
            | Class::StatusLabel
            | Class::Title
            | Class::TaskText(_) => {
                out.push_str("fill:");
                out.push_str(&paint.fill.map_or_else(|| text.clone(), crate::style::Rgb::hex));
                if *class == Class::TextStrong {
                    let _ = write!(out, ";font-weight:{bold}");
                }
                if *class == Class::TextCut {
                    out.push_str(";font-style:italic");
                }
            }
            _ => css_paint(out, paint),
        }
        out.push('}');
    }
    out.push_str("</style>");
}

/// Attribut de classe (rien pour le texte courant).
fn class_attr(out: &mut String, c: Class) {
    let name = class_name(c);
    if !name.is_empty() {
        let _ = write!(out, " class=\"{name}\"");
    }
}

/// Segments `M x,y V y` / `H x` / `L x,y`.
fn segments(out: &mut String, segs: &[(i64, i64, i64, i64)]) {
    for &(x0, y0, x1, y1) in segs {
        out.push('M');
        points(out, &[(x0, y0)]);
        if x0 == x1 {
            out.push('V');
            n(out, y1);
        } else if y0 == y1 {
            out.push('H');
            n(out, x1);
        } else {
            out.push('L');
            points(out, &[(x1, y1)]);
        }
    }
}

/// Écrit une primitive.
#[allow(clippy::too_many_lines, reason = "une branche par primitive")]
fn item(out: &mut String, it: &Item, base_size: u16) {
    match &it.prim {
        Prim::Rect { r, radius16 } => {
            out.push_str("<rect");
            class_attr(out, it.class);
            out.push_str(" x=\"");
            n(out, r.x);
            out.push_str("\" y=\"");
            n(out, r.y);
            out.push_str("\" width=\"");
            n(out, r.w);
            out.push_str("\" height=\"");
            n(out, r.h);
            out.push('"');
            if *radius16 > 0 {
                out.push_str(" rx=\"");
                short(out, *radius16);
                out.push('"');
            }
            out.push_str("/>");
        }
        Prim::Diamond { r } => {
            let (cx, cy) = (r.x.saturating_add(r.w / 2), r.y.saturating_add(r.h / 2));
            out.push_str("<polygon");
            class_attr(out, it.class);
            out.push_str(" points=\"");
            points(out, &[(cx, r.y), (r.right(), cy), (cx, r.bottom()), (r.x, cy)]);
            out.push_str("\"/>");
        }
        Prim::Bracket { r, thick16, tip_w16 } => {
            let (x0, x1, top) = (r.x, r.right(), r.y);
            let bottom = top.saturating_add(*thick16);
            let tip = r.bottom();
            out.push_str("<path");
            class_attr(out, it.class);
            out.push_str(" d=\"M");
            points(out, &[(x0, top), (x1, top), (x1, bottom), (x0, bottom)]);
            out.push_str("Z M");
            points(out, &[(x0, top), (x0.saturating_add(*tip_w16), top), (x0, tip)]);
            out.push_str("Z M");
            points(out, &[(x1.saturating_sub(*tip_w16), top), (x1, top), (x1, tip)]);
            out.push_str("Z\"/>");
        }
        Prim::Polyline { points: pts } => {
            out.push_str("<polyline");
            class_attr(out, it.class);
            out.push_str(" points=\"");
            points(out, pts);
            out.push_str("\"/>");
        }
        Prim::Polygon { points: pts } => {
            out.push_str("<polygon");
            class_attr(out, it.class);
            out.push_str(" points=\"");
            points(out, pts);
            out.push_str("\"/>");
        }
        Prim::Segments { segs } => {
            if segs.is_empty() {
                return;
            }
            out.push_str("<path");
            class_attr(out, it.class);
            out.push_str(" d=\"");
            segments(out, segs);
            out.push_str("\"/>");
        }
        Prim::Hatch { r, dots, diamond } => {
            let segs = pattern_segments(*r, *dots, *diamond);
            if segs.is_empty() {
                return;
            }
            out.push_str("<path");
            class_attr(out, it.class);
            out.push_str(" d=\"");
            segments(out, &segs);
            out.push_str("\"/>");
        }
        Prim::Text {
            x16,
            y16,
            text,
            font,
            anchor,
        } => {
            out.push_str("<text");
            // Un texte en gras hors des classes appuyées garde son poids.
            if font.weight == FontWeight::SemiBold && it.class != Class::TextStrong {
                out.push_str(" font-weight=\"bold\"");
            }
            // Italique d'une tâche (finitions P2, H-UI-126).
            if font.italic {
                out.push_str(" font-style=\"italic\"");
            }
            class_attr(out, it.class);
            out.push_str(" x=\"");
            n(out, *x16);
            out.push_str("\" y=\"");
            n(out, *y16);
            out.push('"');
            if *anchor == Anchor::End {
                out.push_str(" text-anchor=\"end\"");
            }
            if font.size16 != base_size {
                out.push_str(" font-size=\"");
                short(out, i64::from(font.size16));
                out.push('"');
            }
            out.push('>');
            push_text(out, text, usize::MAX);
            out.push_str("</text>");
        }
    }
}

/// Document complet de la scène ; `sizes` reçoit, si demandé, la taille
/// écrite de chaque ligne (`Part::Row`). Avec une chronologie (H-UI-82),
/// elle est écrite en tête sur toute la largeur et la scène est décalée de
/// sa hauteur et de [`TIMELINE_GAP16`] (`<g transform>`).
fn write(
    scene: &Scene,
    name_chars: usize,
    mut sizes: Option<&mut Vec<usize>>,
    timeline: Option<&PrintTimeline>,
) -> String {
    let mut out = String::new();
    let lead = timeline.map_or(0, |_| TIMELINE_HEIGHT16.saturating_add(TIMELINE_GAP16));
    let (width, height) = (scene.width16, scene.height16.saturating_add(lead));
    out.push_str("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"");
    n(&mut out, width);
    out.push_str("\" height=\"");
    n(&mut out, height);
    out.push_str("\" viewBox=\"0 0 ");
    n(&mut out, width);
    out.push(' ');
    n(&mut out, height);
    out.push_str("\"><title>");
    push_text(&mut out, &scene.title, name_chars);
    out.push_str("</title>");
    style(&mut out, scene);
    // Découpes : textes dans la table, Gantt à droite de la table.
    out.push_str("<defs><clipPath id=\"n\"><rect x=\"0.0000\" y=\"0.0000\" width=\"");
    n(&mut out, scene.table16);
    out.push_str("\" height=\"");
    n(&mut out, height);
    out.push_str("\"/></clipPath><clipPath id=\"g\"><rect x=\"");
    n(&mut out, scene.table16);
    out.push_str("\" y=\"0.0000\" width=\"");
    n(&mut out, width.saturating_sub(scene.table16));
    out.push_str("\" height=\"");
    n(&mut out, height);
    out.push_str("\"/></clipPath></defs>");
    if let Some(tl) = timeline {
        for it in &tl.items {
            item(&mut out, it, scene.font.size16);
        }
        out.push_str("<g transform=\"translate(0 ");
        n(&mut out, lead);
        out.push_str(")\">");
    }
    let mut clip = Clip::None;
    for it in &scene.items {
        if it.clip != clip {
            if clip != Clip::None {
                out.push_str("</g>");
            }
            match it.clip {
                Clip::None => {}
                Clip::Table => out.push_str("<g clip-path=\"url(#n)\">"),
                Clip::Gantt => out.push_str("<g clip-path=\"url(#g)\">"),
            }
            clip = it.clip;
        }
        let before = out.len();
        item(&mut out, it, scene.font.size16);
        if let (Some(sizes), Part::Row(i)) = (sizes.as_deref_mut(), it.part) {
            let i = usize::try_from(i).unwrap_or(usize::MAX);
            if i >= sizes.len() && i < scene.rows.len() {
                sizes.resize(i.saturating_add(1), 0);
            }
            if let Some(slot) = sizes.get_mut(i) {
                *slot = slot.saturating_add(out.len().saturating_sub(before));
            }
        }
    }
    if clip != Clip::None {
        out.push_str("</g>");
    }
    if timeline.is_some() {
        out.push_str("</g>");
    }
    out.push_str("</svg>");
    out
}

/// Chronologie de la largeur de la scène et peintures ajoutées à la scène.
fn with_timeline(
    mut scene: Scene,
    input: Option<&TimelineInput>,
    st: &PlanStyle,
    variant: Variant,
) -> (Scene, Option<PrintTimeline>) {
    let Some(input) = input else {
        return (scene, None);
    };
    let width16 = scene.width16.max(0);
    let layout = crate::timeline::layout(input, width16);
    let items = crate::timeline::items(&layout);
    for (class, paint) in crate::timeline::paints(st, variant) {
        scene.paints.entry(class).or_insert(paint);
    }
    (
        scene,
        Some(PrintTimeline {
            width16,
            layout,
            items,
        }),
    )
}

/// Plus petit document : rendu quand même l'en-tête dépasse `max_bytes`.
const MINIMAL: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"0\" height=\"0\"/>";

/// SVG du Gantt dans le modèle Classique (apparence J7), sans textes autour
/// des barres. Infaillible : toute borne atteinte est signalée dans le
/// rapport. Voir [`svg_styled`].
#[must_use]
pub fn svg(project: &Project, schedule: &Schedule, req: &GanttRequest) -> (String, RenderReport) {
    svg_styled(project, schedule, req, &template(TemplateId::Classique), &NoTexts)
}

/// SVG du Gantt dans le style `st`, textes autour des barres fournis par
/// `src` (règle 3), et rapport. Infaillible.
///
/// Lignes : celles du filtre, dans l'ordre des ID, au plus `max_rows` ; au
/// delà, une dernière ligne « … N tâches non affichées ». Si le document
/// dépasse `max_bytes`, il s'arrête à la dernière ligne entière qui tient,
/// suivie de la ligne de troncature ; si même l'en-tête ne tient pas, un
/// document vide (`width="0"`) est rendu, `truncated_bytes` vrai.
#[must_use]
pub fn svg_styled(
    project: &Project,
    schedule: &Schedule,
    req: &GanttRequest,
    st: &PlanStyle,
    src: &dyn BarTextSource,
) -> (String, RenderReport) {
    svg_styled_timeline(project, schedule, req, st, src, None)
}

/// Comme [`svg_styled`], avec la chronologie `timeline` en tête du document
/// (H-UI-82, géométrie H-R16 sur toute la largeur, sans rectangle de la
/// partie visible) ; `None` : document identique à [`svg_styled`] octet
/// pour octet. Les données viennent du moteur
/// (`plannercraft_automation::timeline::input`, règle 3).
#[must_use]
pub fn svg_styled_timeline(
    project: &Project,
    schedule: &Schedule,
    req: &GanttRequest,
    st: &PlanStyle,
    src: &dyn BarTextSource,
    timeline: Option<&TimelineInput>,
) -> (String, RenderReport) {
    let variant = match req.theme {
        Theme::Light => Variant::Light,
        Theme::Dark => Variant::Dark,
    };
    let mut vp = Viewport {
        range: req.range,
        zoom: req.zoom,
        variant,
        lang: req.language,
        filter: req.filter.clone(),
        limits: req.limits,
    };
    let name_chars = usize::from(req.limits.name_chars);
    let max_bytes = req.limits.max_bytes;
    let (scene, mut report) = layout(project, schedule, st, src, &vp);
    let (scene, tl) = with_timeline(scene, timeline, st, variant);
    let mut sizes = Vec::new();
    let full = write(&scene, name_chars, Some(&mut sizes), tl.as_ref());
    if full.len() <= max_bytes {
        report.bytes = full.len();
        return (full, report);
    }

    // Coupe à l'octet : plus grand préfixe de lignes qui tient avec la
    // partie fixe, puis réduction tant que le document dépasse.
    let rows_total: usize = sizes.iter().copied().fold(0, usize::saturating_add);
    let fixed = full.len().saturating_sub(rows_total);
    let mut k = 0_usize;
    let mut used = fixed;
    for s in &sizes {
        let next = used.saturating_add(*s);
        if next > max_bytes {
            break;
        }
        used = next;
        k = k.saturating_add(1);
    }
    let avg = rows_total.checked_div(sizes.len()).unwrap_or(1).max(1);
    loop {
        vp.limits.max_rows = u32::try_from(k).unwrap_or(u32::MAX);
        let (scene, mut report) = layout(project, schedule, st, src, &vp);
        let (scene, tl) = with_timeline(scene, timeline, st, variant);
        let out = write(&scene, name_chars, None, tl.as_ref());
        if out.len() <= max_bytes {
            report.truncated_bytes = true;
            report.bytes = out.len();
            return (out, report);
        }
        if k == 0 {
            let out = MINIMAL.to_owned();
            report.shown = 0;
            report.hidden = u32::try_from(project.tasks.len()).unwrap_or(u32::MAX);
            report.truncated_bytes = true;
            report.bytes = out.len();
            return (out, report);
        }
        let over = out.len().saturating_sub(max_bytes);
        k = k.saturating_sub(over.checked_div(avg).unwrap_or(0).saturating_add(1));
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use super::*;

    #[test]
    fn escape_and_truncate() {
        let mut s = String::new();
        push_text(&mut s, "a<b>&\"'\u{1}\u{FFFF}\n", 100);
        assert_eq!(s, "a&lt;b&gt;&amp;&quot;&apos;\u{FFFD}\u{FFFD} ");
        let mut s = String::new();
        push_text(&mut s, "abcdef", 3);
        assert_eq!(s, "abc…");
        let mut s = String::new();
        push_text(&mut s, "abc", 3);
        assert_eq!(s, "abc");
    }

    #[test]
    fn short_numbers() {
        let f = |v| {
            let mut s = String::new();
            short(&mut s, v);
            s
        };
        assert_eq!(f(24), "1.5");
        assert_eq!(f(32), "2");
        assert_eq!(f(0), "0");
        assert_eq!(f(1), "0.0625");
    }
}
