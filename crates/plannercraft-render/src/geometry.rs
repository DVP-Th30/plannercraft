//! Géométrie partagée du planning (ADR-0016 D5, H-UI-42, P55).
//!
//! Un seul axe entier, les rectangles de barre, le tracé des liens et le
//! placement des textes, en 1/16 px (`i64`). Le SVG et le PDF lisent la
//! [`Scene`] construite par [`layout`] ; l'écran appelle les mêmes fonctions
//! ([`Axis16`], [`bar_geom`], [`link_path`], [`place_texts`]) ligne visible
//! par ligne visible. Aucune date n'est calculée ici (règle 3) : les instants
//! viennent du planning du moteur, les textes de [`BarTextSource`]
//! (implémenté dans `plannercraft-automation`).
//!
//! Hauteurs (H-R14, `docs/features/render.md`) : une barre occupe
//! `hauteur de ligne × height_pct / 100` du style, arrondi au demi-pixel ;
//! avec une ligne de base, barre courante et ligne de base se partagent la
//! ligne comme le Gantt de suivi de l'interface (H-UI-22).

use std::collections::BTreeMap;

use plannercraft_core::{
    BaselineNumber, Day, LinkType, Moment, Project, Schedule, Task, TaskOutcome, TaskRole, TaskUid,
};

use crate::axis::{self, Axis, DaySpan, SUB, Scale};
pub use crate::metrics::{FontRef, FontWeight, InterMetrics, TextMeasure};
use crate::request::{Lang, RangeSpec, RenderLimits, RenderReport, RowFilter, Zoom};
pub use crate::scene::ArrowDir;
use crate::scene::{self, Ends};
use crate::style::{BarField, BarRole, BarShape, FontChoice, PlanStyle, Rgb, Variant};

/// Débord admis d'une barre hors de la plage avant d'être rognée (64 px) :
/// le reste est masqué par la découpe, la sortie reste courte.
const OVERFLOW: i64 = 64 * SUB;

/// Marge entre une barre et son texte (4 px).
pub const TEXT_GAP: i64 = 4 * SUB;

/// Axe temps → abscisse, en 1/16 px : l'axe entier d'`axis.rs`, seul axe du
/// dépôt (l'interface l'adopte en L11).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Axis16 {
    pub(crate) inner: Axis,
    pub(crate) span: DaySpan,
    pub(crate) scale: Scale,
}

impl Axis16 {
    /// Axe d'une vue : plage (H-R3, coupée à `max_days`) et échelle (H-R4)
    /// résolues sur le projet et son planning, début de plage en `left16`.
    #[must_use]
    pub fn new(p: &Project, s: &Schedule, range: RangeSpec, zoom: Zoom, max_days: u32, left16: i64) -> Self {
        let span = axis::resolve_range(range, p, s, max_days);
        let scale = axis::resolve_scale(zoom, span.days());
        Self {
            inner: Axis::new(span, scale, left16),
            span,
            scale,
        }
    }

    /// Axe d'une plage de jours explicite, `from` inclus, `to` exclu (au
    /// moins un jour), à l'échelle `zoom`, début de plage en `left16` :
    /// l'axe de l'écran (J8, lot L11), dont la plage par défaut est
    /// calculée par l'interface sur la même règle (H-R3). `Zoom::Auto` se
    /// résout sur le nombre de jours (H-R4).
    #[must_use]
    pub fn for_days(from: Day, to: Day, zoom: Zoom, left16: i64) -> Self {
        let end = to.max(from.checked_add_days(1).unwrap_or(from));
        let span = DaySpan {
            start: from,
            end,
            truncated: false,
        };
        let scale = axis::resolve_scale(zoom, span.days());
        Self {
            inner: Axis::new(span, scale, left16),
            span,
            scale,
        }
    }

    /// Abscisse d'un instant.
    #[must_use]
    pub fn x16(&self, m: Moment) -> i64 {
        self.inner.x16(m)
    }

    /// Abscisse du minuit d'un jour.
    #[must_use]
    pub fn x16_day(&self, d: Day) -> i64 {
        self.inner.x16_day(d)
    }

    /// Abscisse du début de plage.
    #[must_use]
    pub const fn left16(&self) -> i64 {
        self.inner.left16()
    }

    /// Abscisse de la fin de plage.
    #[must_use]
    pub const fn right16(&self) -> i64 {
        self.inner.right16()
    }

    /// Largeur de la plage.
    #[must_use]
    pub const fn width16(&self) -> i64 {
        self.inner.width16()
    }

    /// Seizièmes de pixel par jour, arrondis à l'entier inférieur
    /// (Trimestre : 19 pour 19,2).
    #[must_use]
    pub const fn px16_per_day(&self) -> i64 {
        self.scale.px16_per_day()
    }

    /// La plage demandée dépassait `max_days`.
    #[must_use]
    pub const fn truncated_days(&self) -> bool {
        self.span.truncated
    }

    /// Abscisse rognée au débord admis autour de la plage.
    fn clamp(&self, x: i64) -> i64 {
        let lo = self.left16().saturating_sub(OVERFLOW);
        let hi = self.right16().saturating_add(OVERFLOW);
        x.clamp(lo, hi)
    }
}

/// Rectangle en 1/16 px.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect16 {
    /// Abscisse du bord gauche.
    pub x: i64,
    /// Ordonnée du bord haut.
    pub y: i64,
    /// Largeur (≥ 0).
    pub w: i64,
    /// Hauteur (≥ 0).
    pub h: i64,
}

impl Rect16 {
    /// Bord droit.
    #[must_use]
    pub const fn right(&self) -> i64 {
        self.x.saturating_add(self.w)
    }

    /// Bord bas.
    #[must_use]
    pub const fn bottom(&self) -> i64 {
        self.y.saturating_add(self.h)
    }
}

/// Entrée de la géométrie d'une ligne (valeurs du moteur, règle 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowGeomInput {
    /// Tâche.
    pub uid: TaskUid,
    /// Rôle de la barre.
    pub role: BarRole,
    /// Début planifié.
    pub start: Option<Moment>,
    /// Fin planifiée.
    pub finish: Option<Moment>,
    /// % achevé en centièmes (10 000 = 100 %) : bande d'avancement quand
    /// `stop` manque (proportion de la barre, aucune date).
    pub percent: u16,
    /// Fin de la partie réelle (`Stop`, R-204) : bande d'avancement
    /// [début, `stop`] comme l'interface (H-UI-22).
    pub stop: Option<Moment>,
    /// Ligne de base 0 (début, fin).
    pub baseline: Option<(Moment, Moment)>,
}

/// Rôle de la barre d'une tâche (`None` : ligne vide ou sans barre).
///
/// Ordre (H-UI-36, H-UI-37) : inactive (ou exclue du calcul) ; récapitulative
/// (critique ou non) ; jalon (case cochée ou durée nulle, critique ou non) ;
/// manuelle ; critique ; tâche.
#[must_use]
pub fn bar_role(task: &Task, outcome: Option<&TaskOutcome>) -> Option<BarRole> {
    if task.is_null {
        return None;
    }
    Some(match outcome? {
        TaskOutcome::Excluded { .. } => BarRole::Inactive,
        TaskOutcome::Scheduled(s) => {
            let milestone = task.milestone || task.duration.is_zero();
            match (s.role, milestone, s.manual, s.critical) {
                (TaskRole::Summary, _, _, true) => BarRole::SummaryCritical,
                (TaskRole::Summary, _, _, false) => BarRole::Summary,
                (_, true, _, true) => BarRole::MilestoneCritical,
                (_, true, _, false) => BarRole::Milestone,
                (_, false, true, _) => BarRole::Manual,
                (_, false, false, true) => BarRole::Critical,
                (_, false, false, false) => BarRole::Task,
            }
        }
    })
}

/// Entrée de géométrie d'une tâche, lue sur le modèle et le planning du
/// moteur (`None` : ligne sans barre).
#[must_use]
pub fn row_geom_input(task: &Task, outcome: Option<&TaskOutcome>) -> Option<RowGeomInput> {
    let role = bar_role(task, outcome)?;
    let (start, finish, percent, stop) = match outcome? {
        TaskOutcome::Scheduled(s) => (
            Some(s.start),
            Some(s.finish),
            u16::try_from(s.percent_complete.get().clamp(0, 10_000)).unwrap_or(0),
            s.progress.map(|p| p.stop),
        ),
        TaskOutcome::Excluded { display, .. } => {
            (display.map(|d| d.start), display.map(|d| d.finish), 0, None)
        }
    };
    let baseline = task
        .baselines
        .get(&BaselineNumber::PRIMARY)
        .and_then(|b| b.start.zip(b.finish));
    Some(RowGeomInput {
        uid: task.uid(),
        role,
        start,
        finish,
        percent,
        stop,
        baseline,
    })
}

/// Géométrie d'une barre.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarGeom {
    /// Rôle.
    pub role: BarRole,
    /// Forme dessinée (losange pour une barre exclue de largeur ≤ 1 px).
    pub shape: BarShape,
    /// Rectangle englobant (losange et crochets compris).
    pub rect: Rect16,
    /// Bande d'avancement.
    pub progress: Option<Rect16>,
    /// Ligne de base (largeur nulle : jalon de base, losange).
    pub baseline: Option<Rect16>,
    /// Centre vertical de la barre (point d'attache des liens).
    pub yc16: i64,
    /// Demi-hauteur vue par les liens (celle d'une barre de tâche).
    pub link_half16: i64,
    /// Haut de la ligne.
    pub row_top16: i64,
    /// Hauteur de la ligne.
    pub row_h16: i64,
}

impl BarGeom {
    /// Extrémités vues par un lien (losange pour un jalon).
    fn ends(&self) -> Ends {
        Ends {
            start: self.rect.x,
            finish: self.rect.right(),
            y: self.yc16,
            half: self.link_half16,
        }
    }
}

/// Hauteur d'une ligne d'un style, en 1/16 px.
#[must_use]
pub fn row_h16(st: &PlanStyle) -> i64 {
    i64::from(st.density.row_px()).saturating_mul(SUB)
}

/// `row16 × pct / 100`, arrondi au demi-pixel (8/16), au moins un demi-pixel.
fn snap_half_px(row16: i64, pct: u8) -> i64 {
    let raw = row16.saturating_mul(i64::from(pct));
    let halves = plannercraft_core::round_div(i128::from(raw), 800).unwrap_or(0);
    halves.max(1).saturating_mul(8)
}

/// Pourcentage de hauteur d'un rôle (45 si le style ne le connaît pas).
fn height_pct(st: &PlanStyle, role: BarRole) -> u8 {
    st.bars.get(&role).map_or(45, |b| b.height_pct)
}

/// Forme d'un rôle.
fn shape_of(st: &PlanStyle, role: BarRole) -> BarShape {
    st.bars.get(&role).map_or(BarShape::Bar, |b| b.shape)
}

/// Géométrie de la barre d'une ligne dont le haut est en `y16`.
///
/// - barre : de début à fin (au moins 1 px), hauteur de son rôle, centrée ;
/// - losange : centré au début, demi-diagonale = moitié de la hauteur du
///   rôle ;
/// - crochets : hauteur totale du rôle, haut aux 5/8 au-dessus du centre
///   (barre pleine des 5/12 de la hauteur, pointes jusqu'en bas) ;
/// - ligne de base présente : barre courante de `haut + 3 px` à 1 px
///   au-dessus du tiers inférieur, ligne de base de 1 px sous le tiers
///   inférieur à 2 px du bas (H-UI-22) ;
/// - bande d'avancement de début à `stop` (sinon `percent` de la barre),
///   centrée, hauteur du rôle `Progress`.
///
/// Abscisses rognées à 64 px au-delà de la plage. Sans dates : rectangle
/// vide au centre de la ligne.
#[must_use]
#[allow(clippy::too_many_lines, reason = "une branche par forme, lue d'un bloc")]
pub fn bar_geom(row: &RowGeomInput, axis: &Axis16, st: &PlanStyle, y16: i64) -> BarGeom {
    let row_h = row_h16(st);
    let mut shape = shape_of(st, row.role);
    let task_half = snap_half_px(row_h, height_pct(st, BarRole::Task)) / 2;
    let lower = y16.saturating_add(row_h.saturating_mul(2) / 3);
    let (yc, band_half) = if row.baseline.is_some() {
        let top = y16.saturating_add(3 * SUB);
        let bottom = lower.saturating_sub(SUB).max(top);
        (
            top.saturating_add(bottom.saturating_sub(top) / 2),
            bottom.saturating_sub(top) / 2,
        )
    } else {
        (y16.saturating_add(row_h / 2), i64::MAX)
    };
    let link_half = task_half.min(band_half);
    let mut geom = BarGeom {
        role: row.role,
        shape,
        rect: Rect16 {
            x: axis.left16(),
            y: yc,
            w: 0,
            h: 0,
        },
        progress: None,
        baseline: None,
        yc16: yc,
        link_half16: link_half,
        row_top16: y16,
        row_h16: row_h,
    };
    let Some(start) = row.start else {
        return geom;
    };
    let x0 = axis.clamp(axis.x16(start));
    let x1 = row.finish.map_or(x0, |f| axis.clamp(axis.x16(f)).max(x0));
    if row.role == BarRole::Inactive && x1.saturating_sub(x0) <= SUB {
        shape = BarShape::Diamond;
    }
    let full = snap_half_px(row_h, height_pct(st, row.role)).min(band_half.saturating_mul(2));
    let half = full / 2;
    geom.shape = shape;
    geom.rect = match shape {
        BarShape::Diamond => {
            // Jalon : demi-diagonale de son rôle ; jalon exclu : celle d'un
            // jalon (le losange garde sa taille quand la barre disparaît).
            let d = if row.role == BarRole::Inactive {
                snap_half_px(row_h, height_pct(st, BarRole::Milestone)).min(band_half.saturating_mul(2)) / 2
            } else {
                half
            };
            Rect16 {
                x: x0.saturating_sub(d),
                y: yc.saturating_sub(d),
                w: d.saturating_mul(2),
                h: d.saturating_mul(2),
            }
        }
        BarShape::Bracket => {
            let top = yc.saturating_sub(full.saturating_mul(5) / 8);
            Rect16 {
                x: x0,
                y: top,
                w: x1.saturating_sub(x0).max(SUB),
                h: full,
            }
        }
        BarShape::Bar | BarShape::Line => Rect16 {
            x: x0,
            y: yc.saturating_sub(half),
            w: x1.saturating_sub(x0).max(SUB),
            h: half.saturating_mul(2),
        },
    };
    if matches!(row.role, BarRole::Task | BarRole::Critical | BarRole::Manual) && shape == BarShape::Bar {
        let right = if let Some(stop) = row.stop {
            axis.clamp(axis.x16(stop)).min(geom.rect.right())
        } else {
            let w = geom.rect.w.saturating_mul(i64::from(row.percent.min(10_000))) / 10_000;
            geom.rect.x.saturating_add(w)
        };
        if right > geom.rect.x {
            let ph = (snap_half_px(row_h, height_pct(st, BarRole::Progress)) / 2).min(half);
            geom.progress = Some(Rect16 {
                x: geom.rect.x,
                y: yc.saturating_sub(ph),
                w: right.saturating_sub(geom.rect.x),
                h: ph.saturating_mul(2),
            });
        }
    }
    if let Some((bs, bf)) = row.baseline {
        let top = lower.saturating_add(SUB);
        let bottom = y16.saturating_add(row_h).saturating_sub(2 * SUB).max(top);
        let bx0 = axis.clamp(axis.x16(bs));
        let bx1 = axis.clamp(axis.x16(bf)).max(bx0);
        let w = if bf <= bs {
            0
        } else {
            bx1.saturating_sub(bx0).max(SUB)
        };
        geom.baseline = Some(Rect16 {
            x: bx0,
            y: top,
            w,
            h: bottom.saturating_sub(top),
        });
    }
    geom
}

/// Tracé orthogonal d'un lien : au plus 6 points et la pointe.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinkPath {
    /// Points en 1/16 px.
    pub points: Vec<(i64, i64)>,
    /// Pointe de flèche (trois sommets), si le tracé en a une.
    pub arrow: Option<[(i64, i64); 3]>,
}

/// Tracé d'un lien `kind` entre deux barres (H-R7 : FS direct par le dessus
/// ou replié par l'intercolonne, SS, FF, SF direct ou replié ; segment de
/// sortie 6 px, flèche 4 px). Barre vide (sans dates) : aucun point.
#[must_use]
pub fn link_path(from: &BarGeom, to: &BarGeom, kind: LinkType) -> LinkPath {
    if from.rect.w == 0 && from.rect.h == 0 || to.rect.w == 0 && to.rect.h == 0 {
        return LinkPath::default();
    }
    let path = scene::link_points(kind, from.ends(), to.ends(), from.row_h16);
    let arrow = path.points.last().map(|&tip| scene::arrow_points(tip, path.dir));
    LinkPath {
        points: path.points,
        arrow,
    }
}

/// Direction de la pointe d'un tracé (pour les tests de forme).
#[must_use]
pub fn arrow_dir(path: &LinkPath) -> Option<ArrowDir> {
    let arrow = path.arrow?;
    let [(tx, ty), (ax, ay), (bx, _)] = arrow;
    // Pointe horizontale : base verticale (deux sommets de même abscisse).
    Some(if ax == bx {
        if ax < tx { ArrowDir::Right } else { ArrowDir::Left }
    } else if ay < ty {
        ArrowDir::Down
    } else {
        ArrowDir::Up
    })
}

/// Textes d'une barre, déjà résolus par la source (règle 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedTexts {
    /// À gauche.
    pub left: Option<String>,
    /// À droite.
    pub right: Option<String>,
    /// Dedans.
    pub inside: Option<String>,
    /// Police des trois textes.
    pub font: FontRef,
}

impl Default for ResolvedTexts {
    fn default() -> Self {
        Self {
            left: None,
            right: None,
            inside: None,
            font: FontRef {
                weight: FontWeight::Regular,
                size16: 192,
                italic: false,
            },
        }
    }
}

/// Côté d'un texte placé.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TextSide {
    /// À gauche de la barre.
    Left,
    /// Dans la barre.
    Inside,
    /// À droite de la barre.
    Right,
}

/// Texte placé (H-UI-46 : jamais rogné, tronqué avec « … »).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacedText {
    /// Texte final (tronqué au besoin).
    pub text: String,
    /// Abscisse du début du texte.
    pub x16: i64,
    /// Ordonnée de la ligne de base.
    pub y16: i64,
    /// Largeur mesurée (H-R11).
    pub width16: i64,
    /// Police.
    pub font: FontRef,
    /// Côté effectif (après bascule).
    pub side: TextSide,
}

/// Textes placés autour d'une barre.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlacedTexts {
    /// Dans l'ordre gauche, dedans, droite.
    pub items: Vec<PlacedText>,
}

/// Plus long préfixe de `s` qui tient dans `max16` : `s` entier, sinon
/// préfixe suivi de « … » ; `None` si même « … » ne tient pas (H-UI-46).
#[must_use]
pub fn fit_text(s: &str, max16: i64, font: FontRef, m: &dyn TextMeasure) -> Option<(String, i64)> {
    let full = m.width16(s, font);
    if full <= max16 {
        return Some((s.to_owned(), full));
    }
    let ellipsis = m.width16("…", font);
    if ellipsis > max16 {
        return None;
    }
    let mut out = String::new();
    let mut width = 0_i64;
    let mut buf = [0_u8; 4];
    for c in s.chars() {
        let w = m.width16(c.encode_utf8(&mut buf), font);
        if width.saturating_add(w).saturating_add(ellipsis) > max16 {
            break;
        }
        width = width.saturating_add(w);
        out.push(c);
    }
    // Pas d'espace pendante avant « … ».
    while out.ends_with(' ') {
        out.pop();
    }
    out.push('…');
    let w = m.width16(&out, font);
    Some((out, w))
}

/// Place les textes d'une barre dans la zone `clip` (abscisses) — H-UI-46 :
///
/// - à droite : `TEXT_GAP` après la barre ; s'il sortirait de la zone, placé
///   à gauche s'il y tient entier, sinon tronqué avec « … » du côté le plus
///   large ;
/// - à gauche : symétrique (bascule à droite) ;
/// - dedans : centré s'il tient, sinon tronqué à la barre moins deux marges,
///   omis si même « … » n'y tient pas.
///
/// Ligne de base au centre de la barre plus un tiers de la taille du corps.
/// Un texte n'est jamais rogné : au pire il est omis.
#[must_use]
pub fn place_texts(
    bar: &BarGeom,
    texts: &ResolvedTexts,
    m: &dyn TextMeasure,
    clip: (i64, i64),
) -> PlacedTexts {
    let font = texts.font;
    let y = bar.yc16.saturating_add(i64::from(font.size16) / 3);
    let (lo, hi) = (clip.0.min(clip.1), clip.0.max(clip.1));
    let bar_l = bar.rect.x;
    let bar_r = bar.rect.right();
    // Place de chaque côté, dans la zone ; un côté dont l'ancre est hors
    // de la zone n'a aucune place.
    let right_anchor = bar_r.saturating_add(TEXT_GAP);
    let left_anchor = bar_l.saturating_sub(TEXT_GAP);
    let right_room = if right_anchor < lo {
        -1
    } else {
        hi.saturating_sub(right_anchor)
    };
    let left_room = if left_anchor > hi {
        -1
    } else {
        left_anchor.saturating_sub(lo)
    };
    let mut items = Vec::new();
    // Côtés déjà occupés : un texte basculé du côté de l'autre se range
    // après lui (J8 L15 : jamais deux étiquettes superposées).
    let mut sides = Sides {
        right_anchor,
        left_anchor,
        right_room,
        left_room,
    };
    if let Some(t) = texts
        .left
        .as_deref()
        .and_then(|s| sides.place(s, TextSide::Left, font, m))
    {
        items.push(t);
    }
    if let Some(s) = texts.inside.as_deref().filter(|s| !s.is_empty()) {
        let room = bar.rect.w.saturating_sub(TEXT_GAP.saturating_mul(2));
        // Partie visible de la barre seulement.
        let vis_l = bar_l.max(lo);
        let vis_r = bar_r.min(hi);
        let room = room.min(
            vis_r
                .saturating_sub(vis_l)
                .saturating_sub(TEXT_GAP.saturating_mul(2)),
        );
        if let Some((text, width)) = fit_text(s, room, font, m) {
            let x = vis_l.saturating_add(vis_r.saturating_sub(vis_l).saturating_sub(width) / 2);
            items.push(PlacedText {
                text,
                x16: x,
                y16: y,
                width16: width,
                font,
                side: TextSide::Inside,
            });
        }
    }
    if let Some(t) = texts
        .right
        .as_deref()
        .and_then(|s| sides.place(s, TextSide::Right, font, m))
    {
        items.push(t);
    }
    for t in &mut items {
        t.y16 = y;
    }
    PlacedTexts { items }
}

/// Place restante de chaque côté d'une barre pendant `place_texts`.
struct Sides {
    /// Début du prochain texte à droite.
    right_anchor: i64,
    /// Fin du prochain texte à gauche.
    left_anchor: i64,
    /// Place à droite (−1 : aucune).
    right_room: i64,
    /// Place à gauche (−1 : aucune).
    left_room: i64,
}

impl Sides {
    /// Place `s` du côté préféré, sinon de l'autre, sinon tronqué du côté le
    /// plus large (H-UI-46) ; le côté choisi est consommé.
    fn place(&mut self, s: &str, prefer: TextSide, font: FontRef, m: &dyn TextMeasure) -> Option<PlacedText> {
        if s.is_empty() {
            return None;
        }
        let full = m.width16(s, font);
        let (first, first_room, second, second_room) = match prefer {
            TextSide::Left => (TextSide::Left, self.left_room, TextSide::Right, self.right_room),
            _ => (TextSide::Right, self.right_room, TextSide::Left, self.left_room),
        };
        let (side, text, width) = if full <= first_room {
            (first, s.to_owned(), full)
        } else if full <= second_room {
            (second, s.to_owned(), full)
        } else {
            let (side, room) = if first_room >= second_room {
                (first, first_room)
            } else {
                (second, second_room)
            };
            let (t, w) = fit_text(s, room, font, m)?;
            (side, t, w)
        };
        let used = width.saturating_add(TEXT_GAP);
        let x = if side == TextSide::Left {
            let x = self.left_anchor.saturating_sub(width);
            self.left_anchor = self.left_anchor.saturating_sub(used);
            self.left_room = self.left_room.saturating_sub(used);
            x
        } else {
            let x = self.right_anchor;
            self.right_anchor = self.right_anchor.saturating_add(used);
            self.right_room = self.right_room.saturating_sub(used);
            x
        };
        Some(PlacedText {
            text,
            x16: x,
            y16: 0,
            width16: width,
            font,
            side,
        })
    }
}

/// Jeton d'en-tête ou de pied de page (H-UI-51).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Token {
    /// `{projet}`.
    Project,
    /// `{date_etat}`.
    StatusDate,
    /// `{page}`.
    Page,
    /// `{pages}`.
    Pages,
    /// `{date_impression}`.
    PrintDate,
    /// `{fichier}`.
    File,
    /// `{planning}` : titre de planning neutre dans la langue du rendu
    /// (« Planning prévisionnel », « Provisional schedule »), cartouche de
    /// « Planning travaux » (relecture finale de J8).
    ScheduleTitle,
}

/// Source des textes affichés autour des barres et dans l'en-tête (règle
/// 3 : formatés par `plannercraft-automation`, jamais calculés ici).
pub trait BarTextSource {
    /// Texte du champ `f` de la tâche `uid` (`None` : rien à afficher).
    fn bar_text(&self, uid: TaskUid, f: BarField) -> Option<String>;
    /// Valeur d'un jeton d'en-tête ou de pied de page.
    fn token(&self, t: Token) -> String;
}

/// Source sans texte : seuls les noms du modèle sont affichés (champ
/// `Name`), rien d'autre (rendu sans `plannercraft-automation`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NoTexts;

impl BarTextSource for NoTexts {
    fn bar_text(&self, _uid: TaskUid, _f: BarField) -> Option<String> {
        None
    }

    fn token(&self, _t: Token) -> String {
        String::new()
    }
}

/// Couleur de phase de chaque tâche du projet, par position (H-UI-65) :
/// `(rang de la phase, la tâche porte elle-même la couleur)`. Une tâche
/// sans couleur propre prend celle de l'ancêtre le plus proche qui en a une
/// avec `children`.
#[must_use]
pub fn task_phases(
    p: &plannercraft_core::Project,
    phases: &std::collections::BTreeMap<u32, crate::style::PhaseColor>,
) -> Vec<Option<(u16, bool)>> {
    if phases.is_empty() {
        return vec![None; p.tasks.len()];
    }
    let rank = |uid: u32| {
        phases
            .keys()
            .position(|k| *k == uid)
            .and_then(|i| u16::try_from(i).ok())
    };
    // Pile des ancêtres : (niveau, phase transmise aux descendants).
    let mut stack: Vec<(u16, Option<u16>)> = Vec::new();
    let mut out = Vec::with_capacity(p.tasks.len());
    for task in &p.tasks {
        let level = task.outline_level.get();
        while stack.last().is_some_and(|(l, _)| *l >= level) {
            stack.pop();
        }
        let inherited = stack.iter().rev().find_map(|(_, ph)| *ph);
        let own = rank(task.uid().get()).filter(|_| !task.is_null);
        let passed = own.filter(|_| phases.get(&task.uid().get()).is_some_and(|c| c.children));
        out.push(match own {
            Some(i) => Some((i, true)),
            None => inherited.map(|i| (i, false)),
        });
        stack.push((level, passed.or(inherited)));
    }
    out
}

/// Rôles peints à la couleur d'une phase (H-UI-65) : barres de tâches et
/// de récapitulatives ; jalons, inactives et repères gardent la leur.
#[must_use]
pub const fn phase_painted(role: crate::style::BarRole) -> bool {
    use crate::style::BarRole;
    matches!(
        role,
        BarRole::Task | BarRole::Critical | BarRole::Summary | BarRole::SummaryCritical | BarRole::Manual
    )
}

/// Jours chômés consécutifs à partir desquels une période est grisée aux
/// échelles larges (H-UI-62 : fériés accolés, fermetures, congés) ; une
/// fin de semaine seule (2 jours) ne l'est pas.
pub const LONG_OFF_DAYS: i64 = 3;

/// Rang du mois de `day` relatif au mois de `start` (H-UI-67) : 1 pour le
/// mois du démarrage, 2 le suivant… ; −1, −2… avant (aucun mois 0).
#[must_use]
pub fn relative_month(day: plannercraft_core::Day, start: plannercraft_core::Day) -> i32 {
    let month_index = |d: plannercraft_core::Day| {
        let (y, m, _) = crate::axis::ymd(d);
        y.saturating_mul(12).saturating_add(i32::try_from(m).unwrap_or(1))
    };
    let diff = month_index(day).saturating_sub(month_index(start));
    if diff >= 0 { diff.saturating_add(1) } else { diff }
}

/// Étiquette d'un mois relatif : « M1 », « M-3 » (H-UI-67).
#[must_use]
pub fn relative_month_label(k: i32) -> String {
    format!("M{k}")
}

/// Fenêtre rendue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Viewport {
    /// Plage de dates.
    pub range: RangeSpec,
    /// Échelle.
    pub zoom: Zoom,
    /// Variante du style.
    pub variant: Variant,
    /// Langue des étiquettes.
    pub lang: Lang,
    /// Lignes retenues.
    pub filter: RowFilter,
    /// Bornes (lignes, jours, caractères d'un nom).
    pub limits: RenderLimits,
}

impl Viewport {
    /// Fenêtre par défaut : plage du projet, semaines, variante `variant`,
    /// toutes les lignes, bornes par défaut.
    #[must_use]
    pub fn new(variant: Variant, lang: Lang) -> Self {
        Self {
            range: RangeSpec::Auto,
            zoom: Zoom::Week,
            variant,
            lang,
            filter: RowFilter::default(),
            limits: RenderLimits::default(),
        }
    }
}

/// Ligne de la scène.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneRow {
    /// Tâche (`None` : ligne vide).
    pub uid: Option<TaskUid>,
    /// ID affiché.
    pub id: u32,
    /// Ordonnée du haut de la ligne.
    pub y16: i64,
    /// Hauteur.
    pub h16: i64,
    /// Niveau hiérarchique.
    pub level: u16,
    /// Récapitulative (repère de la pagination, H-UI-48).
    pub summary: bool,
    /// Barre.
    pub bar: Option<BarGeom>,
    /// Textes placés.
    pub texts: PlacedTexts,
}

/// Rôle visuel d'une primitive, lié à une peinture de la scène
/// ([`Scene::paints`]) : le SVG en fait une classe CSS, le PDF lit la
/// peinture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Class {
    /// Fond.
    Bg,
    /// Fond de l'en-tête.
    HeaderBg,
    /// Bande de fond (zébrage, phase), indice 0 ou 1.
    Band(u8),
    /// Jours chômés.
    NonWorking,
    /// Grille fine.
    Grid,
    /// Grille marquée.
    GridMajor,
    /// Séparateurs de la table.
    Splitter,
    /// Traits entre lignes.
    RowLine,
    /// Date d'état.
    StatusLine,
    /// Étiquette de la date d'état.
    StatusLabel,
    /// Aujourd'hui.
    Today,
    /// Barre d'un rôle.
    Bar(BarRole),
    /// Motif (hachures, points) d'un rôle.
    Pattern(BarRole),
    /// Lien.
    Link,
    /// Lien critique.
    LinkCritical,
    /// Pointe de flèche.
    Arrow,
    /// Pointe critique.
    ArrowCritical,
    /// Texte courant.
    Text,
    /// Texte appuyé (récapitulatives, titres).
    TextStrong,
    /// Texte atténué (lignes vides, exclues).
    TextMuted,
    /// Ligne de troncature.
    TextCut,
    /// Barre d'un rôle peinte à la couleur de la phase d'indice donné
    /// (H-UI-65, rang dans `PlanStyle::phases`).
    Phase(u16, BarRole),
    /// Fond d'une ligne de phase dans la table (teinte lisible, H-UI-65).
    PhaseRow(u16),
    /// Titre du cartouche (H-UI-66, couleur d'accent `cursor`).
    Title,
    /// Texte dans une barre d'un rôle, peinte à la couleur de la phase
    /// d'indice donné s'il y en a une : encre du texte ou du fond, la plus
    /// contrastée sur le remplissage (`style::ink_on`, relecture finale de
    /// J8).
    TextOnBar(BarRole, Option<u16>),
    /// Texte d'une tâche de couleur choisie dans la table (finitions P2,
    /// H-UI-126), rang de la tâche dans `PlanStyle::task_text`.
    TaskText(u16),
    /// Surlignage d'une tâche dans la table (H-UI-126), même rang.
    TaskFill(u16),
}

/// Peinture d'une classe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Paint {
    /// Remplissage (`None` : aucun).
    pub fill: Option<Rgb>,
    /// Trait (`None` : aucun).
    pub stroke: Option<Rgb>,
    /// Épaisseur du trait en 1/16 px.
    pub width16: i64,
    /// Tirets (trait, espace) en 1/16 px.
    pub dash16: Option<(i64, i64)>,
}

/// Zone de découpe d'une primitive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Clip {
    /// Document entier.
    None,
    /// Table (ID et noms).
    Table,
    /// Gantt (à droite de la table).
    Gantt,
}

/// Partie du document portant une primitive (coupe des lignes, pagination).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Part {
    /// Fond, en-tête, grille : indépendant des lignes.
    Fixed,
    /// Ligne `i` de la scène (liens rangés sur leur ligne la plus basse).
    Row(u32),
    /// Ligne de troncature.
    Cut,
    /// Légende (H-UI-47).
    Legend,
}

/// Ancrage horizontal d'un texte.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Anchor {
    /// `x` au début du texte.
    Start,
    /// `x` à la fin du texte.
    End,
}

/// Primitive de la scène, en 1/16 px.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Prim {
    /// Rectangle, coins arrondis.
    Rect {
        /// Rectangle.
        r: Rect16,
        /// Rayon des coins.
        radius16: i64,
    },
    /// Losange inscrit dans `r`.
    Diamond {
        /// Rectangle englobant.
        r: Rect16,
    },
    /// Barre à crochets : barre pleine de `thick16` en haut, pointes de
    /// `tip_w16` de large jusqu'en bas de `r`.
    Bracket {
        /// Rectangle englobant.
        r: Rect16,
        /// Épaisseur de la barre.
        thick16: i64,
        /// Largeur des pointes.
        tip_w16: i64,
    },
    /// Ligne brisée ouverte.
    Polyline {
        /// Points.
        points: Vec<(i64, i64)>,
    },
    /// Polygone plein (pointes de flèche, losanges de légende).
    Polygon {
        /// Sommets.
        points: Vec<(i64, i64)>,
    },
    /// Segments indépendants (grille, séparateurs).
    Segments {
        /// Segments `(x0, y0, x1, y1)`.
        segs: Vec<(i64, i64, i64, i64)>,
    },
    /// Texte sur une ligne, déjà tronqué, non échappé.
    Text {
        /// Abscisse d'ancrage.
        x16: i64,
        /// Ligne de base.
        y16: i64,
        /// Texte.
        text: String,
        /// Police.
        font: FontRef,
        /// Ancrage.
        anchor: Anchor,
    },
    /// Hachures à 45° (ou points) dans `r` : segments par
    /// [`pattern_segments`].
    Hatch {
        /// Zone.
        r: Rect16,
        /// Points au lieu de traits.
        dots: bool,
        /// Limité au losange inscrit dans `r` (jalons).
        diamond: bool,
    },
}

/// Primitive placée : découpe, partie, classe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// Découpe.
    pub clip: Clip,
    /// Partie.
    pub part: Part,
    /// Classe (peinture).
    pub class: Class,
    /// Primitive.
    pub prim: Prim,
}

/// Scène du planning, lue par le SVG et le PDF.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scene {
    /// Largeur totale.
    pub width16: i64,
    /// Hauteur totale (lignes, troncature et légende comprises).
    pub height16: i64,
    /// Hauteur de l'en-tête (échelle).
    pub header16: i64,
    /// Largeur de la table (début du Gantt).
    pub table16: i64,
    /// Titre (nom du projet, non tronqué).
    pub title: String,
    /// Famille de police (`Interface` : police générique).
    pub family: FontChoice,
    /// Police du corps.
    pub font: FontRef,
    /// Lignes dans l'ordre d'affichage.
    pub rows: Vec<SceneRow>,
    /// Liens dessinés.
    pub links: Vec<LinkPath>,
    /// Rôles présents, dans l'ordre de la légende (H-UI-47).
    pub legend: Vec<BarRole>,
    /// Primitives dans l'ordre du tracé.
    pub items: Vec<Item>,
    /// Peintures des classes utilisées.
    pub paints: BTreeMap<Class, Paint>,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            width16: 0,
            height16: 0,
            header16: 0,
            table16: 0,
            title: String::new(),
            family: FontChoice::Interface,
            font: FontRef {
                weight: FontWeight::Regular,
                size16: 192,
                italic: false,
            },
            rows: Vec::new(),
            links: Vec::new(),
            legend: Vec::new(),
            items: Vec::new(),
            paints: BTreeMap::new(),
        }
    }
}

/// Segments d'un motif dans `r` : hachures montantes à 45° tous les 4 px
/// (ou points de 1 px tous les 4 px), limités au losange inscrit si
/// `diamond`, au plus ~4 096 segments (pas élargi au-delà), entiers exacts.
#[must_use]
pub fn pattern_segments(r: Rect16, dots: bool, diamond: bool) -> Vec<(i64, i64, i64, i64)> {
    const MAX: i64 = 4_096;
    let (x0, y0, x1, y1) = (r.x, r.y, r.right(), r.bottom());
    let mut out = Vec::new();
    if r.w <= 0 || r.h <= 0 {
        return out;
    }
    // Losange : centre et demi-diagonales.
    let (cx, cy) = (x0.saturating_add(r.w / 2), y0.saturating_add(r.h / 2));
    let (dx, dy) = ((r.w / 2).max(1), (r.h / 2).max(1));
    let inside = |x: i64, y: i64| {
        // |x − cx| / dx + |y − cy| / dy ≤ 1, en entiers (i128).
        let u = i128::from(x.saturating_sub(cx).unsigned_abs()).saturating_mul(i128::from(dy));
        let v = i128::from(y.saturating_sub(cy).unsigned_abs()).saturating_mul(i128::from(dx));
        u.saturating_add(v) <= i128::from(dx).saturating_mul(i128::from(dy))
    };
    let mut step = 4 * SUB;
    if dots {
        let per = |len: i64| len.checked_div(step).unwrap_or(0).saturating_add(1);
        let cells = per(r.w).saturating_mul(per(r.h));
        if cells > MAX {
            step = step.saturating_mul((cells / MAX).saturating_add(1));
        }
        let mut y = y0.saturating_add(step / 2);
        let mut row = 0_i64;
        while y < y1 {
            let offset = if row % 2 == 0 { step / 2 } else { 0 };
            let mut x = x0.saturating_add(offset);
            while x < x1 {
                if !diamond || inside(x, y) {
                    out.push((x, y, x.saturating_add(SUB).min(x1), y));
                }
                x = x.saturating_add(step);
            }
            y = y.saturating_add(step);
            row = row.saturating_add(1);
        }
        return out;
    }
    let span = r.w.saturating_add(r.h);
    if span.checked_div(step).unwrap_or(0) > MAX {
        step = (span / MAX).saturating_add(1);
    }
    // Droites x + y = c, de x0 + y0 + pas à x1 + y1.
    let mut c = x0.saturating_add(y0).saturating_add(step);
    let end = x1.saturating_add(y1);
    while c < end {
        let mut xa = x0.max(c.saturating_sub(y1));
        let mut xb = x1.min(c.saturating_sub(y0));
        if diamond {
            // Sur la droite, le losange couvre un intervalle : bornes
            // resserrées pas à pas (au plus la taille du losange en 1/16).
            while xa < xb && !inside(xa, c.saturating_sub(xa)) {
                xa = xa.saturating_add(1);
            }
            while xb > xa && !inside(xb, c.saturating_sub(xb)) {
                xb = xb.saturating_sub(1);
            }
        }
        if xa < xb {
            out.push((xa, c.saturating_sub(xa), xb, c.saturating_sub(xb)));
        }
        c = c.saturating_add(step);
    }
    out
}

/// Scène complète d'un projet : lignes du filtre dans l'ordre des ID (au
/// plus `max_rows`, puis ligne de troncature), bandes, jours chômés, grille,
/// en-tête, date d'état et aujourd'hui, liens, barres, textes, table,
/// légende. Rapport : lignes montrées et masquées, plage coupée.
#[must_use]
pub fn layout(
    p: &Project,
    s: &Schedule,
    st: &PlanStyle,
    src: &dyn BarTextSource,
    vp: &Viewport,
) -> (Scene, RenderReport) {
    scene::build(p, s, st, src, vp)
}
