//! Construction de la scène (`layout`, ADR-0016 D5, lot L4 de J8).
//!
//! Tout est en entiers : abscisses et ordonnées en 1/16 px. Les dates des
//! barres viennent du planning (règle 3) ; aucun calcul de date ici. Les
//! textes autour des barres viennent de la [`BarTextSource`] ; les libellés
//! fixes (titres de colonnes, échelle, légende, ligne de troncature) sont
//! ceux du rendu (H-R5, H-R15).

use std::collections::{BTreeMap, BTreeSet};

use plannercraft_core::{
    CalendarSet, Day, LinkType, Project, Schedule, Task, TaskOutcome, TaskRole, TaskUid, Weekday,
};

use crate::axis::{self, SUB, TierUnit, ymd};
use crate::geometry::{
    Anchor, Axis16, BarGeom, BarTextSource, Class, Clip, FontRef, FontWeight, InterMetrics, Item, LinkPath,
    Paint, Part, PlacedTexts, Prim, Rect16, ResolvedTexts, Scene, SceneRow, TextMeasure, Viewport, bar_geom,
    link_path, place_texts, row_geom_input, row_h16,
};
use crate::request::{Lang, RenderReport};
use crate::style::{BarField, BarRole, BarShape, Pattern, PlanStyle, Rgb, RowBanding, TableColumn};

/// Hauteur d'un niveau de l'en-tête (22 px).
pub(crate) const TIER_H: i64 = 22 * SUB;
/// Hauteur de l'en-tête (deux niveaux).
pub(crate) const HEADER_H: i64 = 2 * TIER_H;
/// Largeur de la colonne ID.
pub(crate) const ID_W: i64 = 40 * SUB;
/// Largeur de la colonne Nom.
const NAME_W: i64 = 240 * SUB;

/// Largeur d'une colonne de la table (H-UI-68).
const fn column_w(c: TableColumn) -> i64 {
    match c {
        TableColumn::Id => ID_W,
        TableColumn::Name => NAME_W,
        TableColumn::Duration | TableColumn::Start | TableColumn::Finish => 64 * SUB,
        TableColumn::Percent => 48 * SUB,
        TableColumn::Resources => 140 * SUB,
    }
}

/// Colonnes de la table d'un style : celles de `page.columns`, sinon N° et
/// Nom (sortie de L7).
pub(crate) fn table_columns(columns: Option<&[TableColumn]>) -> Vec<TableColumn> {
    columns.map_or_else(|| TableColumn::CHART_ONLY.to_vec(), <[_]>::to_vec)
}

/// Largeur de la table (début du Gantt) pour ces colonnes ; 0 sans
/// colonne (« graphique seul », H-UI-64).
pub(crate) fn table_width16(columns: Option<&[TableColumn]>) -> i64 {
    table_columns(columns)
        .into_iter()
        .map(column_w)
        .fold(0, i64::saturating_add)
}

/// Rôles peints à la couleur d'une phase : voir
/// [`crate::geometry::phase_painted`].
const fn phase_painted(role: BarRole) -> bool {
    crate::geometry::phase_painted(role)
}

/// Retrait par niveau hiérarchique (12 px), plafonné à [`MAX_INDENT`].
const INDENT: i64 = 12 * SUB;
/// Retrait maximal d'un nom (120 px).
const MAX_INDENT: i64 = 120 * SUB;
/// Marge intérieure d'un texte (4 px).
const PAD: i64 = 4 * SUB;
/// Segment de sortie d'un lien (6 px).
const STUB: i64 = 6 * SUB;
/// Longueur de la pointe de flèche (4 px).
pub(crate) const ARROW: i64 = 4 * SUB;
/// Demi-largeur de la pointe (0,8 × 4 px ≈ 51/16 px).
pub(crate) const ARROW_HALF: i64 = 51;
/// Lignes de légende au plus (S77 : légende de 3 lignes au plus).
const LEGEND_LINES: usize = 3;
/// Jours au plus examinés pour griser les jours chômés.
const MAX_SHADED_DAYS: i64 = 8_192;

/// Extrémités d'une barre vues par un lien (losange pour un jalon).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Ends {
    /// Début.
    pub start: i64,
    /// Fin.
    pub finish: i64,
    /// Centre vertical.
    pub y: i64,
    /// Demi-hauteur d'entrée d'une flèche par le dessus ou le dessous.
    pub half: i64,
}

/// Direction de la pointe de flèche.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrowDir {
    /// Vers la droite (entrée par le début).
    Right,
    /// Vers la gauche (entrée par la fin).
    Left,
    /// Vers le bas (entrée par le dessus).
    Down,
    /// Vers le haut (entrée par le dessous).
    Up,
}

/// Tracé d'un lien : polyligne orthogonale, flèche au dernier point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RawPath {
    /// Points (2 à 6).
    pub points: Vec<(i64, i64)>,
    /// Direction de la flèche.
    pub dir: ArrowDir,
}

/// Tracé d'un lien `kind` de `p` à `s` (lignes de `row_h`) : même tracé que
/// `link_path` de l'interface (H-R7), en entiers.
pub(crate) fn link_points(kind: LinkType, p: Ends, s: Ends, row_h: i64) -> RawPath {
    let (p0, p1, yp) = (p.start, p.finish.max(p.start), p.y);
    let (s0, s1, ys) = (s.start, s.finish.max(s.start), s.y);
    let below = ys >= yp;
    let half_row = row_h / 2;
    let ymid = if below {
        yp.saturating_add(half_row)
    } else {
        yp.saturating_sub(half_row)
    };
    let path = |points: Vec<(i64, i64)>, dir| RawPath { points, dir };
    match kind {
        LinkType::FinishToStart => {
            let xa = s0.saturating_add(STUB.min(s1.saturating_sub(s0) / 2));
            if xa >= p1.saturating_add(STUB / 2) {
                let (edge, dir) = if below {
                    (ys.saturating_sub(s.half), ArrowDir::Down)
                } else {
                    (ys.saturating_add(s.half), ArrowDir::Up)
                };
                path(vec![(p1, yp), (xa, yp), (xa, edge)], dir)
            } else {
                let xo = p1.saturating_add(STUB);
                let xi = s0.saturating_sub(STUB);
                path(
                    vec![(p1, yp), (xo, yp), (xo, ymid), (xi, ymid), (xi, ys), (s0, ys)],
                    ArrowDir::Right,
                )
            }
        }
        LinkType::StartToStart => {
            let xl = p0.min(s0).saturating_sub(STUB);
            path(vec![(p0, yp), (xl, yp), (xl, ys), (s0, ys)], ArrowDir::Right)
        }
        LinkType::FinishToFinish => {
            let xr = p1.max(s1).saturating_add(STUB);
            path(vec![(p1, yp), (xr, yp), (xr, ys), (s1, ys)], ArrowDir::Left)
        }
        LinkType::StartToFinish => {
            let xl = p0.saturating_sub(STUB);
            if s1.saturating_add(STUB) <= xl {
                path(vec![(p0, yp), (xl, yp), (xl, ys), (s1, ys)], ArrowDir::Left)
            } else {
                let xr = s1.saturating_add(STUB);
                path(
                    vec![(p0, yp), (xl, yp), (xl, ymid), (xr, ymid), (xr, ys), (s1, ys)],
                    ArrowDir::Left,
                )
            }
        }
    }
}

/// Triangle de la pointe : pointe en `tip`, orientée selon `dir`.
pub(crate) fn arrow_points(tip: (i64, i64), dir: ArrowDir) -> [(i64, i64); 3] {
    let (x, y) = tip;
    match dir {
        ArrowDir::Right => [
            tip,
            (x.saturating_sub(ARROW), y.saturating_add(ARROW_HALF)),
            (x.saturating_sub(ARROW), y.saturating_sub(ARROW_HALF)),
        ],
        ArrowDir::Left => [
            tip,
            (x.saturating_add(ARROW), y.saturating_sub(ARROW_HALF)),
            (x.saturating_add(ARROW), y.saturating_add(ARROW_HALF)),
        ],
        ArrowDir::Down => [
            tip,
            (x.saturating_sub(ARROW_HALF), y.saturating_sub(ARROW)),
            (x.saturating_add(ARROW_HALF), y.saturating_sub(ARROW)),
        ],
        ArrowDir::Up => [
            tip,
            (x.saturating_add(ARROW_HALF), y.saturating_add(ARROW)),
            (x.saturating_sub(ARROW_HALF), y.saturating_add(ARROW)),
        ],
    }
}

/// Libellés fixes de la table.
struct Labels {
    id: &'static str,
    name: &'static str,
}

const fn labels(lang: Lang) -> Labels {
    match lang {
        Lang::Fr => Labels {
            id: "N°",
            name: "Nom de la tâche",
        },
        Lang::En => Labels {
            id: "ID",
            name: "Task Name",
        },
    }
}

/// Ligne de troncature : « … N tâches non affichées ».
fn truncation_text(lang: Lang, count: usize) -> String {
    match (lang, count) {
        (Lang::Fr, 1) => "… 1 tâche non affichée".to_owned(),
        (Lang::Fr, c) => format!("… {c} tâches non affichées"),
        (Lang::En, 1) => "… 1 task not shown".to_owned(),
        (Lang::En, c) => format!("… {c} tasks not shown"),
    }
}

/// Libellé d'un rôle dans la légende (H-R15).
pub(crate) const fn role_label(role: BarRole, lang: Lang) -> &'static str {
    match (lang, role) {
        (Lang::Fr, BarRole::Task) => "Tâche",
        (Lang::Fr, BarRole::Critical) => "Tâche critique",
        (Lang::Fr, BarRole::Summary) => "Récapitulative",
        (Lang::Fr, BarRole::SummaryCritical) => "Récapitulative critique",
        (Lang::Fr, BarRole::Milestone) => "Jalon",
        (Lang::Fr, BarRole::MilestoneCritical) => "Jalon critique",
        (Lang::Fr, BarRole::Progress) => "Avancement",
        (Lang::Fr, BarRole::Baseline) => "Ligne de base",
        (Lang::Fr, BarRole::Inactive) => "Tâche inactive",
        (Lang::Fr, BarRole::Manual) => "Manuelle",
        (Lang::Fr, BarRole::Slack) => "Marge",
        (Lang::Fr, BarRole::Deadline) => "Échéance",
        (Lang::En, BarRole::Task) => "Task",
        (Lang::En, BarRole::Critical) => "Critical task",
        (Lang::En, BarRole::Summary) => "Summary",
        (Lang::En, BarRole::SummaryCritical) => "Critical summary",
        (Lang::En, BarRole::Milestone) => "Milestone",
        (Lang::En, BarRole::MilestoneCritical) => "Critical milestone",
        (Lang::En, BarRole::Progress) => "Progress",
        (Lang::En, BarRole::Baseline) => "Baseline",
        (Lang::En, BarRole::Inactive) => "Inactive",
        (Lang::En, BarRole::Manual) => "Manually scheduled",
        (Lang::En, BarRole::Slack) => "Slack",
        (Lang::En, BarRole::Deadline) => "Deadline",
    }
}

/// Étiquette de la date d'état (H-UI-54) : « État 02/11 », « Status 11/02 ».
pub(crate) fn status_label(day: Day, lang: Lang) -> String {
    let (_, m, d) = ymd(day);
    match lang {
        Lang::Fr => format!("État {d:02}/{m:02}"),
        Lang::En => format!("Status {m:02}/{d:02}"),
    }
}

/// Texte tronqué à `max` caractères suivis de « … » (H-R6, table).
fn truncate_chars(s: &str, max: usize) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i >= max {
            out.push('…');
            return out;
        }
        out.push(c);
    }
    out
}

/// Peintures des classes d'un style, pour une variante.
pub(crate) fn paints_of(st: &PlanStyle, v: crate::style::Variant) -> impl Fn(Class) -> Paint + '_ {
    let c = st.tokens(v);
    let phase_fill = move |i: u16| -> Option<Rgb> { st.phases.values().nth(usize::from(i)).map(|p| p.fill) };
    // Texte des tâches (finitions P2, H-UI-126) : rang dans `task_text`.
    let task_look = move |i: u16| {
        st.task_text
            .values()
            .nth(usize::from(i))
            .map(|t| crate::style::task_text_look(st, v, t))
    };
    move |class: Class| {
        let fill = |rgb| Paint {
            fill: Some(rgb),
            stroke: None,
            width16: 0,
            dash16: None,
        };
        let stroke = |rgb| Paint {
            fill: None,
            stroke: Some(rgb),
            width16: SUB,
            dash16: None,
        };
        match class {
            Class::Bg => fill(c.bg),
            Class::HeaderBg => fill(c.header_bg),
            Class::Band(i) => fill(if i == 0 { c.band[0] } else { c.band[1] }),
            Class::NonWorking => fill(c.non_working),
            Class::Grid | Class::RowLine => stroke(c.grid),
            Class::GridMajor | Class::Splitter => stroke(c.grid_major),
            Class::StatusLine => Paint {
                width16: 24,
                dash16: Some((6 * SUB, 4 * SUB)),
                ..stroke(c.status_line)
            },
            Class::StatusLabel => fill(c.status_line),
            Class::Today => Paint {
                dash16: Some((2 * SUB, 3 * SUB)),
                ..stroke(c.today_line)
            },
            Class::Bar(role) => {
                let paint = c.bars.get(&role).copied();
                let shape = st.bars.get(&role).copied();
                let fill_rgb = paint.map_or(c.text, |p| p.fill);
                let outline16 = shape.map_or(0, |b| i64::from(b.outline16));
                let outline_only = shape.is_some_and(|b| b.pattern == Pattern::Outline);
                let outline_rgb = paint.and_then(|p| p.outline).unwrap_or(fill_rgb);
                Paint {
                    fill: (!outline_only).then_some(fill_rgb),
                    stroke: (outline16 > 0 || outline_only).then_some(outline_rgb),
                    width16: if outline_only {
                        outline16.max(SUB)
                    } else {
                        outline16
                    },
                    dash16: None,
                }
            }
            Class::Pattern(role) => stroke(c.bars.get(&role).and_then(|p| p.outline).unwrap_or(c.bg)),
            Class::Link => stroke(c.link),
            Class::LinkCritical => stroke(c.link_critical),
            Class::Arrow => fill(c.link),
            Class::ArrowCritical => fill(c.link_critical),
            Class::Text => fill(c.text),
            Class::TextStrong => fill(c.text_strong),
            Class::TextMuted | Class::TextCut => fill(c.text_muted),
            Class::Phase(i, role) => {
                // Forme et contour du rôle, remplissage de la phase.
                let shape = st.bars.get(&role).copied();
                let outline16 = shape.map_or(0, |b| i64::from(b.outline16));
                let rgb = phase_fill(i).unwrap_or(c.text);
                let outline_rgb = c.bars.get(&role).and_then(|p| p.outline).unwrap_or(c.text_strong);
                Paint {
                    fill: Some(rgb),
                    stroke: (outline16 > 0).then_some(outline_rgb),
                    width16: outline16,
                    dash16: None,
                }
            }
            Class::PhaseRow(i) => {
                fill(phase_fill(i).map_or(c.bg, |rgb| crate::style::phase_row_tint(st, v, rgb)))
            }
            Class::Title => fill(c.cursor),
            Class::TextOnBar(role, phase) => {
                let bar = phase
                    .filter(|_| phase_painted(role))
                    .and_then(phase_fill)
                    .or_else(|| c.bars.get(&role).map(|p| p.fill))
                    .unwrap_or(c.bg);
                fill(crate::style::ink_on(bar, c.text, c.bg))
            }
            Class::TaskText(i) => fill(task_look(i).and_then(|l| l.color).unwrap_or(c.text)),
            Class::TaskFill(i) => fill(task_look(i).and_then(|l| l.fill).unwrap_or(c.bg)),
        }
    }
}

/// Accumulateur de primitives.
struct Items(Vec<Item>);

impl Items {
    fn push(&mut self, clip: Clip, part: Part, class: Class, prim: Prim) {
        self.0.push(Item {
            clip,
            part,
            class,
            prim,
        });
    }

    fn rect(&mut self, clip: Clip, part: Part, class: Class, r: Rect16) {
        self.push(clip, part, class, Prim::Rect { r, radius16: 0 });
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "texte complet : découpe, partie, classe, position, police, ancrage"
    )]
    fn text(
        &mut self,
        clip: Clip,
        part: Part,
        class: Class,
        x16: i64,
        y16: i64,
        text: String,
        font: FontRef,
        anchor: Anchor,
    ) {
        self.push(
            clip,
            part,
            class,
            Prim::Text {
                x16,
                y16,
                text,
                font,
                anchor,
            },
        );
    }
}

/// Ligne retenue avant placement.
struct Kept<'a> {
    position: usize,
    task: &'a Task,
    critical: bool,
}

/// Primitives d'une barre.
fn bar_items(items: &mut Items, part: Part, bar: &BarGeom, st: &PlanStyle, phase: Option<u16>) {
    let style = st.bars.get(&bar.role).copied();
    let pattern = style.map_or(Pattern::Solid, |b| b.pattern);
    let radius = style.map_or(0, |b| i64::from(b.radius16));
    let r = bar.rect;
    if let Some(b) = bar.baseline {
        let prim = if b.w == 0 {
            let d = (b.h / 2).saturating_add(SUB);
            Prim::Diamond {
                r: Rect16 {
                    x: b.x.saturating_sub(d),
                    y: b.y.saturating_add(b.h / 2).saturating_sub(d),
                    w: d.saturating_mul(2),
                    h: d.saturating_mul(2),
                },
            }
        } else {
            Prim::Rect { r: b, radius16: 0 }
        };
        items.push(Clip::Gantt, part, Class::Bar(BarRole::Baseline), prim);
    }
    let class = match phase {
        Some(i) if phase_painted(bar.role) => Class::Phase(i, bar.role),
        _ => Class::Bar(bar.role),
    };
    let mut pattern_rect = r;
    let mut diamond = false;
    match bar.shape {
        BarShape::Diamond => {
            diamond = true;
            items.push(Clip::Gantt, part, class, Prim::Diamond { r });
        }
        BarShape::Bracket => {
            let thick = (r.h.saturating_mul(5) / 12).max(2 * SUB);
            let tip_w = (r.h.saturating_mul(5) / 8).min(r.w / 2).max(SUB);
            pattern_rect.h = thick;
            items.push(
                Clip::Gantt,
                part,
                class,
                Prim::Bracket {
                    r,
                    thick16: thick,
                    tip_w16: tip_w,
                },
            );
        }
        BarShape::Bar | BarShape::Line => {
            items.push(
                Clip::Gantt,
                part,
                class,
                Prim::Rect {
                    r,
                    radius16: radius.min(r.h / 2),
                },
            );
        }
    }
    if matches!(pattern, Pattern::Hatch | Pattern::Dots) {
        items.push(
            Clip::Gantt,
            part,
            Class::Pattern(bar.role),
            Prim::Hatch {
                r: pattern_rect,
                dots: pattern == Pattern::Dots,
                diamond,
            },
        );
    }
    if let Some(pr) = bar.progress {
        items.push(
            Clip::Gantt,
            part,
            Class::Bar(BarRole::Progress),
            Prim::Rect { r: pr, radius16: 0 },
        );
    }
}

/// Textes autour d'une barre, résolus par la source (nom du modèle à
/// défaut pour `Name`).
fn resolve_texts(
    task: &Task,
    role: BarRole,
    st: &PlanStyle,
    src: &dyn BarTextSource,
    font: FontRef,
) -> ResolvedTexts {
    let Some(spec) = st.texts.get(&role) else {
        return ResolvedTexts {
            font,
            ..ResolvedTexts::default()
        };
    };
    let uid = task.uid();
    let get = |f: Option<BarField>| {
        let f = f?;
        src.bar_text(uid, f)
            .or_else(|| (f == BarField::Name).then(|| task.name.clone()))
            .filter(|s| !s.is_empty())
    };
    ResolvedTexts {
        left: get(spec.left),
        right: get(spec.right),
        inside: get(spec.inside),
        font,
    }
}

/// Jours chômés du calendrier du projet dans la plage, fusionnés en
/// intervalles `[début, fin)` de jours.
fn non_working_runs(p: &Project, axis: &Axis16) -> Vec<(Day, Day)> {
    let (calendars, _) = CalendarSet::compile(p);
    let Ok(cal) = calendars.get(p.calendar) else {
        return Vec::new();
    };
    let mut runs: Vec<(Day, Day)> = Vec::new();
    let mut day = axis.span.start;
    let mut seen = 0_i64;
    while day < axis.span.end && seen < MAX_SHADED_DAYS {
        let off = cal.day_ranges(day).is_ok_and(<[_]>::is_empty);
        let Ok(next) = day.checked_add_days(1) else {
            break;
        };
        if off {
            match runs.last_mut() {
                Some(last) if last.1 == day => last.1 = next,
                _ => runs.push((day, next)),
            }
        }
        day = next;
        seen = seen.saturating_add(1);
    }
    runs
}

/// Jours ajoutés de chaque côté de la plage au plus par [`label_room`]
/// (un an).
const MAX_ROOM_DAYS: i64 = 366;

/// Passes de [`label_room`] au plus (chaque allongement réduit l'échelle :
/// la passe suivante vérifie et complète).
const ROOM_PASSES: usize = 4;

/// Impression ajustée (`Zoom::Fill`) sur la plage automatique (H-R3) :
/// les 7 jours de marge avant le début et les 21 jours après la fin du
/// projet ne laissent qu'une dizaine ou une trentaine de pixels à
/// l'échelle d'un projet de deux ans, et les étiquettes des barres du
/// bord (nom d'une récapitulative à gauche, « 17,4 mois » ou nom d'un
/// jalon de fin à droite) basculaient de côté ou étaient tronquées
/// (relecture finale de J8, F9). Rend l'axe dont la plage est allongée du
/// plus petit nombre de jours qui fait tenir ces étiquettes, mesurées en
/// Inter (H-R11) : une barre à `r` px du bord d'une plage de `D` jours sur
/// `W` px, étiquette de `L` px (écart compris), demande
/// `E ≥ D × (L − r) / (W − L)` jours de ce côté ; quelques passes, chaque
/// allongement réduisant l'échelle. `None` : rien à allonger (autre zoom,
/// plage donnée, coupe à `max_days`, aucune étiquette en défaut).
#[allow(
    clippy::too_many_arguments,
    reason = "mêmes entrées que la scène, lues sans les recopier"
)]
fn label_room(
    p: &Project,
    s: &Schedule,
    st: &PlanStyle,
    src: &dyn BarTextSource,
    vp: &Viewport,
    kept: &[Kept<'_>],
    body: FontRef,
    metrics: &dyn TextMeasure,
    table_w: i64,
) -> Option<Axis16> {
    if !matches!(vp.zoom, crate::request::Zoom::Fill { .. })
        || !matches!(vp.range, crate::request::RangeSpec::Auto)
    {
        return None;
    }
    let mut axis = Axis16::new(p, s, vp.range, vp.zoom, vp.limits.max_days, table_w);
    if axis.span.truncated {
        return None;
    }
    // Étiquettes à gauche et à droite de chaque barre, mesurées une fois.
    let labels: Vec<(crate::geometry::RowGeomInput, i64, i64)> = kept
        .iter()
        .filter_map(|k| {
            let input = row_geom_input(k.task, s.tasks.get(&k.task.uid()))?;
            let texts = resolve_texts(k.task, input.role, st, src, body);
            let need = |t: Option<String>| {
                t.map_or(0, |t| {
                    metrics
                        .width16(&t, body)
                        .saturating_add(crate::geometry::TEXT_GAP)
                })
            };
            Some((input, need(texts.left), need(texts.right)))
        })
        .collect();
    let (mut before, mut after) = (0_i64, 0_i64);
    let mut changed = false;
    for _ in 0..ROOM_PASSES {
        let w = axis.width16();
        let d = axis.span.days();
        let days = |need: i64, room: i64| {
            if need <= room || need >= w {
                return 0;
            }
            d.saturating_mul(need.saturating_sub(room))
                .checked_div(w.saturating_sub(need))
                .unwrap_or(0)
                .saturating_add(1)
        };
        let (mut add_before, mut add_after) = (0_i64, 0_i64);
        for (input, left, right) in &labels {
            let bar = bar_geom(input, &axis, st, 0);
            add_before = add_before.max(days(*left, bar.rect.x.saturating_sub(axis.left16())));
            add_after = add_after.max(days(*right, axis.right16().saturating_sub(bar.rect.right())));
        }
        if add_before == 0 && add_after == 0 {
            break;
        }
        before = before.saturating_add(add_before).min(MAX_ROOM_DAYS);
        after = after.saturating_add(add_after).min(MAX_ROOM_DAYS);
        let first = Axis16::new(p, s, vp.range, vp.zoom, vp.limits.max_days, table_w);
        let last = first.span.end.checked_add_days(-1).ok()?;
        let from = first
            .span
            .start
            .checked_add_days(i32::try_from(before.saturating_neg()).ok()?)
            .ok()?;
        let to = last.checked_add_days(i32::try_from(after).ok()?).ok()?;
        let wider = Axis16::new(
            p,
            s,
            crate::request::RangeSpec::Span { from, to },
            vp.zoom,
            vp.limits.max_days,
            table_w,
        );
        if wider.span.truncated {
            break;
        }
        axis = wider;
        changed = true;
    }
    changed.then_some(axis)
}

/// Construit la scène.
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    reason = "assemblage linéaire des couches, dans l'ordre du tracé ; noms du contrat (p, s, st)"
)]
pub(crate) fn build(
    p: &Project,
    s: &Schedule,
    st: &PlanStyle,
    src: &dyn BarTextSource,
    vp: &Viewport,
) -> (Scene, RenderReport) {
    let metrics = InterMetrics;
    let paint = paints_of(st, vp.variant);
    let row_h = row_h16(st);
    let body = FontRef {
        weight: FontWeight::Regular,
        size16: st.font.size16,
        italic: false,
    };
    let strong = FontRef {
        weight: FontWeight::SemiBold,
        ..body
    };
    let columns = table_columns(st.page.columns.as_deref());
    let table_w = table_width16(Some(&columns));
    let axis = Axis16::new(p, s, vp.range, vp.zoom, vp.limits.max_days, table_w);
    // H-UI-67 : un niveau de plus en haut de l'en-tête pour les mois
    // relatifs ; les deux niveaux de l'échelle descendent d'autant.
    let tier0 = if st.scale.relative_months { TIER_H } else { 0 };
    let header_h = HEADER_H.saturating_add(tier0);
    let phase_by_pos = crate::geometry::task_phases(p, &st.phases);
    // Mise en forme du texte des tâches (finitions P2, H-UI-126) : rang de
    // classe et mise en forme, par UID.
    let text_ranks: BTreeMap<u32, (u16, crate::style::TaskText)> = st
        .task_text
        .iter()
        .enumerate()
        .map(|(rank, (uid, t))| (*uid, (u16::try_from(rank).unwrap_or(u16::MAX), *t)))
        .collect();
    let name_chars = usize::from(vp.limits.name_chars);
    let baseline_dy = (row_h / 2).saturating_add(i64::from(body.size16) / 3);

    // Lignes du filtre, dans l'ordre des ID.
    let filter = &vp.filter;
    let mut kept: Vec<Kept<'_>> = Vec::new();
    for (position, task) in p.tasks.iter().enumerate() {
        let uid = task.uid();
        let critical =
            !task.is_null && matches!(s.tasks.get(&uid), Some(TaskOutcome::Scheduled(t)) if t.critical);
        if filter.critical_only && !critical {
            continue;
        }
        if filter.uids.as_ref().is_some_and(|set| !set.contains(&uid)) {
            continue;
        }
        if filter
            .max_outline
            .is_some_and(|max| task.outline_level.get() > max)
        {
            continue;
        }
        kept.push(Kept {
            position,
            task,
            critical,
        });
    }
    let max_rows = usize::try_from(vp.limits.max_rows).unwrap_or(usize::MAX);
    let shown = kept.len().min(max_rows);
    let cut = kept.len().saturating_sub(shown);
    let kept = kept.get(..shown).unwrap_or(&[]);
    // H-R3 (relecture finale de J8, F9) : impression ajustée, plage
    // automatique : la plage s'allonge pour que l'étiquette de droite des
    // barres de fin de projet tienne à droite.
    let axis = label_room(p, s, st, src, vp, kept, body, &metrics, table_w).unwrap_or(axis);
    let width = table_w.saturating_add(axis.width16());
    let n_rows = i64::try_from(shown.saturating_add(usize::from(cut > 0))).unwrap_or(i64::MAX);
    let rows_bottom = header_h.saturating_add(n_rows.saturating_mul(row_h));
    let row_top =
        |i: usize| header_h.saturating_add(i64::try_from(i).unwrap_or(i64::MAX).saturating_mul(row_h));
    let part_of = |i: usize| Part::Row(u32::try_from(i).unwrap_or(u32::MAX));

    // Lignes de la scène : barres et textes.
    let mut rows: Vec<SceneRow> = Vec::with_capacity(shown);
    let mut index: BTreeMap<TaskUid, usize> = BTreeMap::new();
    let clip_x = (axis.left16(), axis.right16());
    for (i, k) in kept.iter().enumerate() {
        let uid = k.task.uid();
        let top = row_top(i);
        let outcome = s.tasks.get(&uid);
        let bar = row_geom_input(k.task, outcome).map(|input| bar_geom(&input, &axis, st, top));
        let texts = bar.map_or_else(PlacedTexts::default, |b| {
            let resolved = resolve_texts(k.task, b.role, st, src, body);
            place_texts(&b, &resolved, &metrics, clip_x)
        });
        let summary = matches!(outcome, Some(TaskOutcome::Scheduled(t)) if t.role == TaskRole::Summary);
        index.insert(uid, i);
        rows.push(SceneRow {
            uid: (!k.task.is_null).then_some(uid),
            id: u32::try_from(k.position)
                .ok()
                .and_then(|v| v.checked_add(1))
                .unwrap_or(u32::MAX),
            y16: top,
            h16: row_h,
            level: k.task.outline_level.get(),
            summary,
            bar,
            texts,
        });
    }

    // Liens entre lignes montrées, rangés sur leur ligne la plus basse.
    let mut links: Vec<(usize, bool, LinkPath)> = Vec::new();
    for (to, k) in kept.iter().enumerate() {
        for link in &k.task.predecessors {
            let Some(&from) = index.get(&link.predecessor) else {
                continue;
            };
            if from == to {
                continue;
            }
            let geom = |i: usize| rows.get(i).and_then(|r| r.bar);
            let (Some(a), Some(b)) = (geom(from), geom(to)) else {
                continue;
            };
            let path = link_path(&a, &b, link.kind);
            if path.points.is_empty() {
                continue;
            }
            let critical = kept.get(from).is_some_and(|x| x.critical) && k.critical;
            links.push((from.max(to), critical, path));
        }
    }
    // Tri stable : l'ordre d'apparition départage.
    links.sort_by_key(|(last, _, _)| *last);

    let mut items = Items(Vec::new());

    // Légende : rôles présents (H-UI-47).
    let mut present: BTreeSet<BarRole> = BTreeSet::new();
    for r in &rows {
        if let Some(b) = r.bar {
            present.insert(b.role);
            if b.progress.is_some() {
                present.insert(BarRole::Progress);
            }
            if b.baseline.is_some() {
                present.insert(BarRole::Baseline);
            }
        }
    }
    let legend: Vec<BarRole> = if st.legend.show {
        BarRole::ALL.into_iter().filter(|r| present.contains(r)).collect()
    } else {
        Vec::new()
    };
    let legend_layout = legend_layout(&legend, vp.lang, body, width, &metrics);
    let legend_h = if legend_layout.is_empty() {
        0
    } else {
        let line_count = legend_layout
            .iter()
            .map(|e| e.line)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        i64::from(line_count)
            .saturating_mul(row_h)
            .saturating_add(2 * PAD)
    };
    let height = rows_bottom.saturating_add(legend_h);

    // Fond, bandes, jours chômés.
    items.rect(
        Clip::None,
        Part::Fixed,
        Class::Bg,
        Rect16 {
            x: 0,
            y: 0,
            w: width,
            h: height,
        },
    );
    let bg = st.tokens(vp.variant).bg;
    let mut phase: Option<usize> = None;
    for (i, r) in rows.iter().enumerate() {
        let band = match st.grid.rows {
            RowBanding::None => None,
            RowBanding::Zebra => Some(i % 2),
            RowBanding::Phase => {
                if r.level == 1 {
                    phase = Some(phase.map_or(0, |v| v.saturating_add(1)));
                }
                Some(phase.unwrap_or(0) % 2)
            }
        };
        // H-UI-62 : bandes dans la table seulement, jamais dans le Gantt.
        let own_phase = kept
            .get(i)
            .and_then(|k| phase_by_pos.get(k.position).copied().flatten())
            .filter(|(_, own)| *own);
        // Surlignage d'une tâche (finitions P2, H-UI-126) : prime sur la
        // couleur de phase et les bandes ; table seulement.
        if let Some(rank) = kept
            .get(i)
            .filter(|k| !k.task.is_null)
            .and_then(|k| text_ranks.get(&k.task.uid().get()))
            .filter(|(_, t)| t.fill.is_some())
            .map(|(rank, _)| *rank)
        {
            items.rect(
                Clip::Table,
                part_of(i),
                Class::TaskFill(rank),
                Rect16 {
                    x: 0,
                    y: r.y16,
                    w: table_w,
                    h: row_h,
                },
            );
            continue;
        }
        if let Some((pi, _)) = own_phase {
            items.rect(
                Clip::Table,
                part_of(i),
                Class::PhaseRow(pi),
                Rect16 {
                    x: 0,
                    y: r.y16,
                    w: table_w,
                    h: row_h,
                },
            );
            continue;
        }
        let Some(band) = band else {
            continue;
        };
        let class = Class::Band(u8::from(band == 1));
        if paint(class).fill == Some(bg) {
            continue;
        }
        items.rect(
            Clip::Table,
            part_of(i),
            class,
            Rect16 {
                x: 0,
                y: r.y16,
                w: table_w,
                h: row_h,
            },
        );
    }
    // Jours chômés : tous à l'échelle Semaine et plus fine ; aux échelles
    // larges, les périodes d'au moins `LONG_OFF_DAYS` jours (H-UI-62).
    if st.scale.shade_non_working {
        let fine = axis.px16_per_day() >= 192;
        for (a, b) in non_working_runs(p, &axis) {
            if !fine && i64::from(b.index().saturating_sub(a.index())) < crate::geometry::LONG_OFF_DAYS {
                continue;
            }
            let x0 = axis.inner.x16_day(a);
            let x1 = axis.inner.x16_day(b);
            items.rect(
                Clip::Gantt,
                Part::Fixed,
                Class::NonWorking,
                Rect16 {
                    x: x0,
                    y: header_h,
                    w: x1.saturating_sub(x0),
                    h: rows_bottom.saturating_sub(header_h),
                },
            );
        }
    }

    // En-tête : fond, titres de colonnes, échelle et grille.
    items.rect(
        Clip::None,
        Part::Fixed,
        Class::HeaderBg,
        Rect16 {
            x: 0,
            y: 0,
            w: width,
            h: header_h,
        },
    );
    let l = labels(vp.lang);
    // Milieu de l'en-tête, ligne de base 5/12 du corps plus bas (27 px en
    // 12 px, comme J7).
    let header_y = (header_h / 2).saturating_add(i64::from(strong.size16).saturating_mul(5) / 12);
    let mut col_x = 0_i64;
    for &c in &columns {
        let w = column_w(c);
        let (x, anchor) = column_anchor(c, col_x, w);
        let title = column_title(c, &l, vp.lang);
        // Titre plus large que sa colonne : coupé avec « … » (H-UI-46).
        let room = w.saturating_sub(2 * PAD);
        let shown = fit_chars(title, room, strong, &metrics);
        if !shown.is_empty() {
            items.text(
                Clip::None,
                Part::Fixed,
                Class::TextStrong,
                x,
                header_y,
                shown,
                strong,
                anchor,
            );
        }
        col_x = col_x.saturating_add(w);
    }
    let label_font = FontRef {
        weight: FontWeight::Regular,
        size16: 192,
        italic: false,
    };
    // Étiquette de la date d'état (H-UI-54) : à droite du trait, à gauche
    // s'il n'y a pas la place ; les étiquettes du niveau bas qu'elle
    // recouvrirait ne sont pas écrites.
    let status = p.effective_status_date();
    let sx = axis.x16(status);
    let status_in = sx >= axis.left16() && sx <= axis.right16();
    let status_tag: Option<(i64, i64, String)> = (status_in && st.scale.status_label).then(|| {
        let text = status_label(status.day(), vp.lang);
        let w = metrics.width16(&text, label_font);
        let room_right = axis.right16().saturating_sub(sx.saturating_add(3 * SUB));
        let x = if w.saturating_add(2 * SUB) <= room_right {
            sx.saturating_add(3 * SUB)
        } else {
            sx.saturating_sub(3 * SUB).saturating_sub(w)
        };
        (x, x.saturating_add(w), text)
    });
    let (major, minor) = axis.scale.tiers();
    let week_start = p.options.week_start_day;
    if st.scale.relative_months {
        // H-UI-67 : mois relatifs au démarrage, niveau du haut ; frontières
        // tracées dans l'en-tête seulement.
        let start = s.project_start.day();
        let cells = axis::relative_month_cells(&axis.inner, axis.span, start, label_font);
        let segs: Vec<(i64, i64, i64, i64)> = cells
            .iter()
            .filter(|c| c.boundary && c.x0 > axis.left16())
            .map(|c| (c.x0, 0, c.x0, TIER_H))
            .collect();
        if !segs.is_empty() {
            items.push(Clip::None, Part::Fixed, Class::GridMajor, Prim::Segments { segs });
        }
        for c in cells.iter().filter(|c| !c.label.is_empty()) {
            items.text(
                Clip::Gantt,
                Part::Fixed,
                Class::Title,
                c.x0.saturating_add(3 * SUB),
                15 * SUB,
                c.label.clone(),
                label_font,
                Anchor::Start,
            );
        }
        items.push(
            Clip::None,
            Part::Fixed,
            Class::GridMajor,
            Prim::Segments {
                segs: vec![(table_w, TIER_H, width, TIER_H)],
            },
        );
    }
    for (unit, class, y0, show) in [
        (major, Class::GridMajor, tier0, st.grid.major_v),
        (minor, Class::Grid, tier0.saturating_add(TIER_H), st.grid.minor_v),
    ] {
        let numbers = st.scale.week_numbers && unit == TierUnit::Week;
        let cells = axis::tier_cells(
            &axis.inner,
            axis.span,
            unit,
            week_start,
            vp.lang,
            label_font,
            numbers,
        );
        let segs: Vec<(i64, i64, i64, i64)> = cells
            .iter()
            .filter(|c| c.boundary && c.x0 > axis.left16())
            .map(|c| (c.x0, y0, c.x0, rows_bottom))
            .collect();
        if show && !segs.is_empty() {
            items.push(Clip::None, Part::Fixed, class, Prim::Segments { segs });
        }
        // Une étiquette qui ne tient pas dans sa cellule rognée (bord de
        // plage) n'est pas écrite.
        for c in cells.iter().filter(|c| {
            let w = axis::label_width16(&c.label, label_font);
            let x = c.x0.saturating_add(3 * SUB);
            let hidden = y0 == tier0.saturating_add(TIER_H)
                && status_tag.as_ref().is_some_and(|(a, b, _)| {
                    x < b.saturating_add(4 * SUB) && x.saturating_add(w) > a.saturating_sub(4 * SUB)
                });
            !c.label.is_empty() && !hidden && w <= c.x1.saturating_sub(c.x0)
        }) {
            items.text(
                Clip::Gantt,
                Part::Fixed,
                Class::Text,
                c.x0.saturating_add(3 * SUB),
                y0.saturating_add(15 * SUB),
                c.label.clone(),
                label_font,
                Anchor::Start,
            );
        }
    }
    // H-UI-62 : traits entre lignes dans la table seulement.
    if st.grid.row_lines {
        for (i, r) in rows.iter().enumerate() {
            let y = r.y16.saturating_add(row_h);
            items.push(
                Clip::Table,
                part_of(i),
                Class::RowLine,
                Prim::Segments {
                    segs: vec![(0, y, table_w, y)],
                },
            );
        }
    }

    // Date d'état (H-R8, H-UI-54) et aujourd'hui.
    if status_in {
        items.push(
            Clip::None,
            Part::Fixed,
            Class::StatusLine,
            Prim::Segments {
                segs: vec![(sx, header_h, sx, rows_bottom)],
            },
        );
        if let Some((x, _, text)) = status_tag.clone() {
            items.text(
                Clip::Gantt,
                Part::Fixed,
                Class::StatusLabel,
                x,
                tier0.saturating_add(TIER_H).saturating_add(15 * SUB),
                text,
                label_font,
                Anchor::Start,
            );
        }
    }
    if st.scale.today_mark {
        let tx = axis.x16(p.current_date);
        if tx >= axis.left16() && tx <= axis.right16() && tx != sx {
            items.push(
                Clip::None,
                Part::Fixed,
                Class::Today,
                Prim::Segments {
                    segs: vec![(tx, header_h, tx, rows_bottom)],
                },
            );
        }
    }

    // Séparateurs de la table et du bas de l'en-tête.
    items.push(
        Clip::None,
        Part::Fixed,
        Class::Splitter,
        Prim::Segments {
            segs: splitters(&columns, rows_bottom, width, header_h),
        },
    );

    // Liens, sous les barres.
    for (last, critical, path) in &links {
        let (line, head) = if *critical {
            (Class::LinkCritical, Class::ArrowCritical)
        } else {
            (Class::Link, Class::Arrow)
        };
        items.push(
            Clip::Gantt,
            part_of(*last),
            line,
            Prim::Polyline {
                points: path.points.clone(),
            },
        );
        if let Some(arrow) = path.arrow {
            items.push(
                Clip::Gantt,
                part_of(*last),
                head,
                Prim::Polygon {
                    points: arrow.to_vec(),
                },
            );
        }
    }

    // Barres et leurs textes.
    for (i, (r, k)) in rows.iter().zip(kept).enumerate() {
        if let Some(b) = &r.bar {
            let phase = phase_by_pos.get(k.position).copied().flatten().map(|(pi, _)| pi);
            bar_items(&mut items, part_of(i), b, st, phase);
        }
    }
    for (i, (r, k)) in rows.iter().zip(kept).enumerate() {
        let phase = phase_by_pos.get(k.position).copied().flatten().map(|(pi, _)| pi);
        for t in &r.texts.items {
            // H-UI-64 : fond sous une étiquette hors de la barre, au-dessus
            // des liens et de la grille (lisible sans la table) ; dedans,
            // encre la plus contrastée sur la barre (H-UI-61).
            let class = if t.side == crate::geometry::TextSide::Inside {
                r.bar
                    .as_ref()
                    .map_or(Class::Text, |b| Class::TextOnBar(b.role, phase))
            } else {
                items.rect(Clip::Gantt, part_of(i), Class::Bg, label_halo(t));
                Class::Text
            };
            items.text(
                Clip::Gantt,
                part_of(i),
                class,
                t.x16,
                t.y16,
                t.text.clone(),
                t.font,
                Anchor::Start,
            );
        }
    }

    // Table : colonnes du style (N° et Nom par défaut, H-UI-68).
    for (i, (r, k)) in rows.iter().zip(kept).enumerate() {
        let outcome = s.tasks.get(&k.task.uid());
        let (class, font) = if r.summary {
            (Class::TextStrong, strong)
        } else if k.task.is_null || matches!(outcome, Some(TaskOutcome::Excluded { .. }) | None) {
            (Class::TextMuted, body)
        } else {
            (Class::Text, body)
        };
        // Texte d'une tâche mis en forme (finitions P2, H-UI-126) : toutes
        // les colonnes sauf N° ; gras = Inter SemiBold, italique simulé.
        let styled = (!k.task.is_null)
            .then(|| text_ranks.get(&k.task.uid().get()))
            .flatten()
            .map(|(rank, t)| {
                let class = if t.color.is_some() {
                    Class::TaskText(*rank)
                } else {
                    class
                };
                let weight = if t.bold { FontWeight::SemiBold } else { font.weight };
                (
                    class,
                    FontRef {
                        weight,
                        italic: t.italic,
                        ..font
                    },
                )
            });
        let y = r.y16.saturating_add(baseline_dy);
        let mut col_x = 0_i64;
        for &c in &columns {
            let w = column_w(c);
            let x_here = col_x;
            col_x = col_x.saturating_add(w);
            let text = match c {
                TableColumn::Id => Some(r.id.to_string()),
                TableColumn::Name => {
                    (!k.task.name.is_empty()).then(|| truncate_chars(&k.task.name, name_chars))
                }
                _ if k.task.is_null => None,
                other => column_field(other).and_then(|f| src.bar_text(k.task.uid(), f)),
            };
            let Some(text) = text.filter(|t| !t.is_empty()) else {
                continue;
            };
            let (mut x, anchor) = column_anchor(c, x_here, w);
            let mut room = w.saturating_sub(2 * PAD);
            if c == TableColumn::Name {
                let level = i64::from(r.level.saturating_sub(1));
                let indent = level.saturating_mul(INDENT).min(MAX_INDENT);
                x = x.saturating_add(indent);
                room = room.saturating_sub(indent);
            }
            // Nom compris (relecture finale de J8, F3) : coupé à la largeur
            // de sa colonne, retrait déduit, au lieu de déborder sur la
            // colonne suivante (le clip est la table entière).
            let (class, font) = match styled {
                Some(look) if c != TableColumn::Id => look,
                _ => (class, font),
            };
            let text = if c == TableColumn::Id {
                text
            } else {
                fit_chars(&text, room, font, &metrics)
            };
            items.text(Clip::Table, part_of(i), class, x, y, text, font, anchor);
        }
    }

    // Légende.
    let legend_top = rows_bottom.saturating_add(PAD);
    for e in &legend_layout {
        let top = legend_top.saturating_add(i64::from(e.line).saturating_mul(row_h));
        let yc = top.saturating_add(row_h / 2);
        legend_swatch(&mut items, e.role, e.x, yc, st);
        items.text(
            Clip::None,
            Part::Legend,
            Class::Text,
            e.x.saturating_add(SWATCH_W).saturating_add(PAD),
            top.saturating_add(baseline_dy),
            role_label(e.role, vp.lang).to_owned(),
            body,
            Anchor::Start,
        );
    }

    // Ligne de troncature.
    if cut > 0 {
        let italic = body;
        items.text(
            Clip::None,
            Part::Cut,
            Class::TextCut,
            PAD,
            row_top(shown).saturating_add(baseline_dy),
            truncation_text(vp.lang, cut),
            italic,
            Anchor::Start,
        );
    }

    let used: BTreeSet<Class> = items.0.iter().map(|i| i.class).collect();
    let title = st.page.title_block.then_some(Class::Title);
    let paints: BTreeMap<Class, Paint> = used
        .into_iter()
        .chain([Class::Text])
        .chain(title)
        .map(|c| (c, paint(c)))
        .collect();
    let scene = Scene {
        width16: width,
        height16: height,
        header16: header_h,
        table16: table_w,
        title: p.info.name.clone(),
        family: st.font.family,
        font: body,
        links: links.into_iter().map(|(_, _, path)| path).collect(),
        rows,
        legend,
        items: items.0,
        paints,
    };
    let report = RenderReport {
        shown: u32::try_from(shown).unwrap_or(u32::MAX),
        hidden: u32::try_from(p.tasks.len().saturating_sub(shown)).unwrap_or(u32::MAX),
        truncated_days: axis.truncated_days(),
        truncated_bytes: false,
        bytes: 0,
    };
    (scene, report)
}

/// Fond d'une étiquette de barre (H-UI-64) : boîte du texte, 1 px de
/// marge à gauche et à droite, hauteur du corps de la police.
pub(crate) fn label_halo(t: &crate::geometry::PlacedText) -> Rect16 {
    let size = i64::from(t.font.size16);
    // Ascendantes ≈ 0,8 du corps au-dessus de la ligne de base, descendantes
    // ≈ 0,25 en dessous.
    let top = t.y16.saturating_sub(size.saturating_mul(4) / 5);
    Rect16 {
        x: t.x16.saturating_sub(SUB),
        y: top,
        w: t.width16.saturating_add(2 * SUB),
        h: size.saturating_mul(21) / 20,
    }
}

/// Abscisse d'ancrage et ancrage d'un texte de colonne : nombres et
/// durées à droite, le reste à gauche.
const fn column_anchor(c: TableColumn, x0: i64, w: i64) -> (i64, Anchor) {
    match c {
        TableColumn::Id | TableColumn::Duration | TableColumn::Percent => {
            (x0.saturating_add(w).saturating_sub(PAD), Anchor::End)
        }
        _ => (x0.saturating_add(PAD), Anchor::Start),
    }
}

/// Champ de la source affiché par une colonne (règle 3).
const fn column_field(c: TableColumn) -> Option<BarField> {
    match c {
        TableColumn::Id | TableColumn::Name => None,
        TableColumn::Duration => Some(BarField::DurationShort),
        TableColumn::Start => Some(BarField::StartShort),
        TableColumn::Finish => Some(BarField::FinishShort),
        TableColumn::Percent => Some(BarField::Percent),
        TableColumn::Resources => Some(BarField::Resources),
    }
}

/// Titre d'une colonne.
const fn column_title(c: TableColumn, l: &Labels, lang: Lang) -> &'static str {
    match (c, lang) {
        (TableColumn::Id, _) => l.id,
        (TableColumn::Name, _) => l.name,
        (TableColumn::Duration, Lang::Fr) => "Durée",
        (TableColumn::Duration, Lang::En) => "Duration",
        (TableColumn::Start, Lang::Fr) => "Début",
        (TableColumn::Start, Lang::En) => "Start",
        (TableColumn::Finish, Lang::Fr) => "Fin",
        (TableColumn::Finish, Lang::En) => "Finish",
        (TableColumn::Percent, Lang::Fr) => "% achevé",
        (TableColumn::Percent, Lang::En) => "% Complete",
        (TableColumn::Resources, Lang::Fr) => "Ressources",
        (TableColumn::Resources, Lang::En) => "Resource Names",
    }
}

/// Texte coupé avec « … » pour tenir dans `room` (1/16 px), mesuré en
/// Inter (H-R11) ; vide si même « … » ne tient pas. Plus long préfixe
/// qui tient, par dichotomie sur les frontières de caractères (la largeur
/// d'un préfixe croît avec sa longueur) : O(n log n) pour un nom long.
fn fit_chars(text: &str, room: i64, font: FontRef, m: &dyn TextMeasure) -> String {
    if m.width16(text, font) <= room {
        return text.to_owned();
    }
    let ends: Vec<usize> = text.char_indices().map(|(i, _)| i).skip(1).collect();
    let fits = |k: usize| {
        let end = if k == 0 {
            0
        } else {
            ends.get(k.saturating_sub(1)).copied().unwrap_or(text.len())
        };
        let mut candidate = text.get(..end).unwrap_or_default().to_owned();
        candidate.push('…');
        (m.width16(&candidate, font) <= room).then_some(candidate)
    };
    // Plus grand k (caractères gardés) dont « préfixe… » tient.
    let (mut lo, mut hi) = (0_usize, text.chars().count());
    let Some(mut best) = fits(0) else {
        return String::new();
    };
    while lo < hi {
        let mid = lo.saturating_add(hi.saturating_sub(lo).saturating_add(1) / 2);
        match fits(mid) {
            Some(candidate) => {
                best = candidate;
                lo = mid;
            }
            None => hi = mid.saturating_sub(1),
        }
    }
    best
}

/// Séparateurs de la table (un par bord de colonne) et bas de l'en-tête.
fn splitters(
    columns: &[TableColumn],
    rows_bottom: i64,
    width: i64,
    header_h: i64,
) -> Vec<(i64, i64, i64, i64)> {
    let mut segs = Vec::new();
    let mut x = 0_i64;
    for &c in columns {
        x = x.saturating_add(column_w(c));
        segs.push((x, 0, x, rows_bottom));
    }
    segs.push((0, header_h, width, header_h));
    segs
}

/// Largeur d'un échantillon de légende (16 px).
const SWATCH_W: i64 = 16 * SUB;
/// Écart entre deux entrées de légende (16 px).
const LEGEND_GAP: i64 = 16 * SUB;

/// Entrée de légende placée.
struct LegendEntry {
    role: BarRole,
    x: i64,
    line: u8,
}

/// Entrées de légende de gauche à droite, à la ligne quand la largeur est
/// atteinte, au plus [`LEGEND_LINES`] lignes (le reste est omis).
fn legend_layout(
    roles: &[BarRole],
    lang: Lang,
    font: FontRef,
    width: i64,
    m: &dyn TextMeasure,
) -> Vec<LegendEntry> {
    let mut out = Vec::new();
    let mut x = 2 * PAD;
    let mut line = 0_u8;
    for &role in roles {
        let w = SWATCH_W
            .saturating_add(PAD)
            .saturating_add(m.width16(role_label(role, lang), font));
        if x > 2 * PAD && x.saturating_add(w) > width.saturating_sub(PAD) {
            line = line.saturating_add(1);
            x = 2 * PAD;
        }
        if usize::from(line) >= LEGEND_LINES {
            break;
        }
        out.push(LegendEntry { role, x, line });
        x = x.saturating_add(w).saturating_add(LEGEND_GAP);
    }
    out
}

/// Échantillon d'un rôle dans la légende, centré verticalement en `yc`.
fn legend_swatch(items: &mut Items, role: BarRole, x: i64, yc: i64, st: &PlanStyle) {
    let style = st.bars.get(&role).copied();
    let shape = style.map_or(BarShape::Bar, |b| b.shape);
    let pattern = style.map_or(Pattern::Solid, |b| b.pattern);
    let class = Class::Bar(role);
    let (r, diamond) = match shape {
        BarShape::Diamond => {
            let d = 6 * SUB;
            let cx = x.saturating_add(SWATCH_W / 2);
            (
                Rect16 {
                    x: cx.saturating_sub(d),
                    y: yc.saturating_sub(d),
                    w: 2 * d,
                    h: 2 * d,
                },
                true,
            )
        }
        BarShape::Bar | BarShape::Bracket | BarShape::Line => (
            Rect16 {
                x,
                y: yc.saturating_sub(5 * SUB),
                w: SWATCH_W,
                h: 10 * SUB,
            },
            false,
        ),
    };
    let prim = match shape {
        BarShape::Diamond => Prim::Diamond { r },
        BarShape::Bracket => Prim::Bracket {
            r,
            thick16: r.h.saturating_mul(5) / 12,
            tip_w16: 5 * SUB,
        },
        BarShape::Bar | BarShape::Line => Prim::Rect { r, radius16: 0 },
    };
    items.push(Clip::None, Part::Legend, class, prim);
    if matches!(pattern, Pattern::Hatch | Pattern::Dots) {
        let mut pr = r;
        if shape == BarShape::Bracket {
            pr.h = r.h.saturating_mul(5) / 12;
        }
        items.push(
            Clip::None,
            Part::Legend,
            Class::Pattern(role),
            Prim::Hatch {
                r: pr,
                dots: pattern == Pattern::Dots,
                diamond,
            },
        );
    }
}

/// Numéro de semaine ISO 8601 d'un jour (semaine du jeudi).
pub(crate) fn iso_week(d: Day) -> u32 {
    let pos = Weekday::ALL.iter().position(|w| *w == d.weekday()).unwrap_or(1);
    // `Weekday::ALL` commence au dimanche : lundi = 1 … dimanche = 7.
    let iso = if pos == 0 {
        7
    } else {
        i32::try_from(pos).unwrap_or(1)
    };
    let thursday = d.checked_add_days(4_i32.saturating_sub(iso)).unwrap_or(d);
    let (year, _, _) = ymd(thursday);
    let jan1: Option<Day> = format!("{year:04}-01-01").parse().ok();
    jan1.map_or(1, |j| {
        let days = thursday.index().saturating_sub(j.index());
        u32::try_from(days / 7).unwrap_or(0).saturating_add(1)
    })
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use super::*;

    const fn ends(start: i64, finish: i64, row: i64) -> Ends {
        Ends {
            start: start * SUB,
            finish: finish * SUB,
            y: (11 + 22 * row) * SUB,
            half: 5 * SUB,
        }
    }

    const ROW: i64 = 22 * SUB;

    #[test]
    fn fs_direct_enters_from_above() {
        let path = link_points(LinkType::FinishToStart, ends(0, 50, 0), ends(80, 120, 1), ROW);
        assert_eq!(path.dir, ArrowDir::Down);
        assert_eq!(path.points.len(), 3);
        assert_eq!(path.points[0], (50 * SUB, 11 * SUB));
        assert_eq!(path.points[2], (86 * SUB, 28 * SUB));
    }

    #[test]
    fn fs_folded_when_successor_starts_before() {
        let path = link_points(LinkType::FinishToStart, ends(0, 50, 0), ends(20, 60, 1), ROW);
        assert_eq!(path.dir, ArrowDir::Right);
        assert_eq!(path.points.len(), 6);
        assert_eq!(path.points[5], (20 * SUB, 33 * SUB));
    }

    #[test]
    fn other_kinds_end_on_expected_side() {
        let p = ends(10, 50, 0);
        let s = ends(30, 70, 2);
        assert_eq!(
            link_points(LinkType::StartToStart, p, s, ROW).points.last(),
            Some(&(30 * SUB, 55 * SUB))
        );
        let ff = link_points(LinkType::FinishToFinish, p, s, ROW);
        assert_eq!(ff.points.last(), Some(&(70 * SUB, 55 * SUB)));
        assert_eq!(ff.dir, ArrowDir::Left);
        let sf = link_points(LinkType::StartToFinish, p, s, ROW);
        assert_eq!(sf.points.len(), 6);
        assert_eq!(sf.points.last(), Some(&(70 * SUB, 55 * SUB)));
        let up = link_points(LinkType::FinishToStart, ends(0, 10, 3), ends(40, 60, 1), ROW);
        assert_eq!(up.dir, ArrowDir::Up);
    }

    #[test]
    fn iso_weeks() {
        let w = |s: &str| iso_week(s.parse().unwrap());
        assert_eq!(w("2026-10-19"), 43);
        assert_eq!(w("2026-10-25"), 43);
        assert_eq!(w("2026-01-01"), 1);
        assert_eq!(w("2027-01-01"), 53);
        assert_eq!(w("2021-01-03"), 53);
        assert_eq!(w("2024-12-30"), 1);
        assert_eq!(
            status_label("2026-11-02".parse().unwrap(), Lang::Fr),
            "État 02/11"
        );
        assert_eq!(
            status_label("2026-11-02".parse().unwrap(), Lang::En),
            "Status 11/02"
        );
    }
}
