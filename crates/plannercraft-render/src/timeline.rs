//! Géométrie de la chronologie (H-R16, ADR-0017 D7) : frise au-dessus de la
//! vue Gantt et en tête de la première page imprimée.
//!
//! Entrée ([`TimelineInput`]) : dates et textes fournis par le moteur via
//! `plannercraft_automation::timeline::input` (règle 3, aucune date calculée ici) ;
//! sortie ([`TimelineLayout`]) : formes en entiers de 1/16 px, sans egui,
//! peintes par l'interface, le SVG et le PDF (même principe que H-UI-42).
//! [`items`] traduit la disposition en primitives de la scène (`Item`),
//! lues telles quelles par le SVG, le PDF et l'aperçu ; [`paints`] donne
//! leurs peintures, tirées des jetons existants du style.
//!
//! Gabarit vertical (72 px, en px depuis le haut de la frise) :
//!
//! | Zone | Ordonnées | Contenu |
//! |---|---|---|
//! | bande haute | 0 à 11 (ligne de base 9) | étiquettes « au-dessus » des jalons, étiquette de la date d'état, graduations |
//! | rangées | 11, 27, 43 (14 px, 2 px d'écart) | barres et leurs noms (ligne de base au centre + 1/3 du corps) |
//! | axe | 62 | axe, graduations (± 4 px), losanges (demi-diagonale 5 px), étiquettes d'extrémité (ligne de base 65) dans les marges |
//! | bande basse | 62 à 72 (ligne de base 70) | étiquettes « au-dessous » des jalons, à droite du losange |
//!
//! Textes en Inter 10 px (Regular ; `SemiBold` pour les extrémités),
//! mesurés par la table de H-R11, tronqués avec « … » à leur boîte, jamais
//! rognés ni superposés (P58).

use std::collections::BTreeMap;

use plannercraft_core::{Moment, TaskUid};

use crate::axis::SUB;
use crate::geometry::{Anchor, Class, Clip, Item, Paint, Part, Prim, Rect16, fit_text};
use crate::metrics::{FontRef, FontWeight, InterMetrics, TextMeasure};
use crate::style::{BarRole, MAX_TIMELINE_TASKS, PlanStyle, Variant};

/// Hauteur de la frise, en 1/16 px (72 px).
pub const TIMELINE_HEIGHT16: i64 = 72 * 16;

/// Police des textes de la frise : Inter 10 px.
pub const TIMELINE_FONT: FontRef = FontRef {
    weight: FontWeight::Regular,
    size16: 160,
    italic: false,
};

/// Police des étiquettes d'extrémité : Inter `SemiBold` 10 px.
pub const TIMELINE_FONT_STRONG: FontRef = FontRef {
    weight: FontWeight::SemiBold,
    size16: 160,
    italic: false,
};

/// Nombre de rangées de barres.
pub const TIMELINE_ROWS: usize = 3;

/// Haut des trois rangées de barres.
pub const ROW_TOP16: [i64; TIMELINE_ROWS] = [11 * SUB, 27 * SUB, 43 * SUB];

/// Hauteur d'une rangée de barres (14 px).
pub const ROW_H16: i64 = 14 * SUB;

/// Ordonnée de l'axe.
pub const AXIS_Y16: i64 = 62 * SUB;

/// Demi-diagonale d'un losange de jalon (losange de 10 px).
pub const DIAMOND_HALF16: i64 = 5 * SUB;

/// Demi-hauteur d'une graduation, de part et d'autre de l'axe.
pub const TICK_HALF16: i64 = 4 * SUB;

/// Ligne de base de la bande haute.
pub const TOP_BASE16: i64 = 9 * SUB;

/// Ligne de base de la bande basse.
pub const BOTTOM_BASE16: i64 = 70 * SUB;

/// Ligne de base des étiquettes d'extrémité (axe + 3 px).
pub const BOUND_BASE16: i64 = 65 * SUB;

/// Marge autour des étiquettes d'extrémité.
pub const PAD16: i64 = 4 * SUB;

/// Écart entre une étiquette d'extrémité et le bout de l'axe : un losange
/// posé au bout de l'axe (demi-diagonale 5 px) ne la touche pas.
pub const AXIS_GAP16: i64 = 8 * SUB;

/// Écart minimal entre deux barres d'une rangée.
pub const BAR_GAP16: i64 = 4 * SUB;

/// Écart minimal entre deux textes d'une même bande.
pub const TEXT_GAP16: i64 = 4 * SUB;

/// Retrait du nom dans sa barre, et écart d'un nom à droite de sa barre.
pub const NAME_PAD16: i64 = 3 * SUB;

/// Largeur sous laquelle le nom passe à droite de la barre.
pub const NARROW_BAR16: i64 = 24 * SUB;

/// Largeur minimale de l'axe gardée quand les étiquettes d'extrémité sont
/// tronquées.
pub const MIN_AXIS16: i64 = 16 * SUB;

/// Largeur au plus d'une étiquette de jalon.
pub const MILESTONE_LABEL16: i64 = 200 * SUB;

/// Rayon des coins des barres (2 px).
pub const BAR_RADIUS16: i64 = 2 * SUB;

/// Largeur de travail au plus (bien au-delà de 100 000 px) : les produits
/// de l'axe restent dans `i128` sans saturer.
const MAX_WIDTH16: i64 = 1 << 40;

/// Durée minimale de l'axe : un jour (projet de durée nulle).
const MIN_SPAN_MINUTES: i64 = 1_440;

/// Données de la frise, toutes venues du moteur (règle 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimelineInput {
    /// Début du projet (bord gauche de l'axe).
    pub start: Moment,
    /// Fin du projet (bord droit de l'axe).
    pub finish: Moment,
    /// Date d'état effective, si le projet en a une.
    pub status: Option<Moment>,
    /// Étiquette du début (« Début lun 19/10/26 »).
    pub start_label: String,
    /// Étiquette de la fin (« Fin mar 17/11/26 »).
    pub finish_label: String,
    /// Étiquette de la date d'état (« État 06/11 »).
    pub status_label: Option<String>,
    /// Graduations : instant et texte (« oct. »).
    pub ticks: Vec<(Moment, String)>,
    /// Tâches de la frise présentes dans le projet.
    pub items: Vec<TimelineItem>,
}

/// Tâche de la frise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimelineItem {
    /// UID de la tâche.
    pub uid: TaskUid,
    /// ID (ordre de la table), départage des rangées.
    pub id: u32,
    /// Barre ou jalon (dit par le moteur).
    pub kind: TimelineKind,
    /// Début planifié.
    pub start: Moment,
    /// Fin planifiée.
    pub finish: Moment,
    /// Nom de la tâche.
    pub name: String,
    /// Date courte (jalons : « 06/11 »).
    pub date_label: String,
    /// Tâche inactive (R-062) : peinte dans le style `Inactive`.
    pub inactive: bool,
}

/// Sorte d'élément de la frise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimelineKind {
    /// Barre, du début à la fin.
    Bar,
    /// Losange sur l'axe.
    Milestone,
}

/// Frise disposée : formes dans l'ordre de dessin (H-UI-80).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimelineLayout {
    /// Hauteur, en 1/16 px ([`TIMELINE_HEIGHT16`]).
    pub height16: i64,
    /// Formes, dans l'ordre de dessin.
    pub shapes: Vec<TimelineShape>,
    /// Éléments valides non dessinés (au-delà de trois rangées), comptés
    /// « +n ».
    pub hidden: u32,
}

/// Forme de la frise ; rectangles `[x, y, largeur, hauteur]` en 1/16 px,
/// origine en haut à gauche de la frise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TimelineShape {
    /// Fond de la frise.
    Band {
        /// Rectangle.
        rect16: [i64; 4],
    },
    /// Barre d'une tâche.
    Bar {
        /// Tâche.
        uid: TaskUid,
        /// Rangée (0 à 2).
        row: u8,
        /// Rectangle.
        rect16: [i64; 4],
        /// Tâche inactive.
        inactive: bool,
    },
    /// Losange d'un jalon.
    Diamond {
        /// Tâche.
        uid: TaskUid,
        /// Centre.
        center16: (i64, i64),
        /// Demi-diagonale.
        half16: i64,
        /// Tâche inactive.
        inactive: bool,
    },
    /// Axe horizontal.
    Axis {
        /// Ordonnée.
        y16: i64,
        /// Abscisses de début et de fin.
        x16: (i64, i64),
    },
    /// Graduation.
    Tick {
        /// Abscisse.
        x16: i64,
    },
    /// Date d'état (trait tireté).
    StatusLine {
        /// Abscisse.
        x16: i64,
    },
    /// Texte placé (ligne de base à `y16`).
    Text {
        /// Abscisse du bord gauche.
        x16: i64,
        /// Ordonnée de la ligne de base.
        y16: i64,
        /// Texte, déjà tronqué (« … », H-R11).
        text: String,
        /// Rôle (couleur, graisse).
        role: TimelineText,
        /// Tâche nommée, s'il y en a une.
        uid: Option<TaskUid>,
    },
}

/// Rôle d'un texte de la frise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimelineText {
    /// Étiquette d'extrémité (début, fin).
    Bound,
    /// Graduation.
    Tick,
    /// Date d'état.
    Status,
    /// Nom dans (ou à droite de) une barre.
    BarName,
    /// Nom d'un jalon.
    MilestoneName,
    /// Date d'un jalon.
    MilestoneDate,
    /// Compteur « +n ».
    Overflow,
}

impl TimelineText {
    /// Police du rôle (`SemiBold` pour les extrémités).
    #[must_use]
    pub const fn font(self) -> FontRef {
        match self {
            Self::Bound => TIMELINE_FONT_STRONG,
            _ => TIMELINE_FONT,
        }
    }
}

/// Axe linéaire : abscisse d'un instant, arrondi inférieur de
/// `(t − début) × longueur / (fin − début)` en `i128`, borné à l'axe.
#[derive(Clone, Copy, Debug)]
struct Line {
    start: i64,
    span: i64,
    x0: i64,
    len: i64,
}

impl Line {
    fn x(self, t: Moment) -> i64 {
        let dt = t.minutes().saturating_sub(self.start).clamp(0, self.span);
        let num = i128::from(dt).saturating_mul(i128::from(self.len));
        let off = num.checked_div(i128::from(self.span.max(1))).unwrap_or(0);
        let off = i64::try_from(off).unwrap_or(self.len).clamp(0, self.len);
        self.x0.saturating_add(off)
    }

    fn contains(self, t: Moment) -> bool {
        let dt = t.minutes().saturating_sub(self.start);
        (0..=self.span).contains(&dt)
    }
}

/// Texte mesuré à placer.
fn width_of(s: &str, role: TimelineText) -> i64 {
    InterMetrics.width16(s, role.font())
}

/// Intervalles occupés d'une bande de textes.
#[derive(Default)]
struct Occupied(Vec<(i64, i64)>);

impl Occupied {
    fn free(&self, a: i64, b: i64) -> bool {
        self.0
            .iter()
            .all(|&(x0, x1)| b.saturating_add(TEXT_GAP16) <= x0 || a >= x1.saturating_add(TEXT_GAP16))
    }

    fn take(&mut self, a: i64, b: i64) {
        self.0.push((a, b));
    }
}

/// Barre placée.
struct Placed {
    index: usize,
    row: usize,
    x0: i64,
    x1: i64,
}

/// Dispose la frise sur `width16` (1/16 px, borné à `[0, 2^40]`) selon
/// H-R16 et H-UI-80 : axe du début à la fin du projet entre les marges des
/// étiquettes d'extrémité ; barres en trois rangées au plus (premier
/// ajustement par (début, ID), 4 px au moins entre deux barres d'une
/// rangée), le reste compté « +n » ; jalons en losanges sur l'axe, date
/// courte et nom au-dessus, au-dessous si l'étiquette chevaucherait la
/// précédente ; date d'état tiretée ; graduations. Un UID en double n'est
/// dessiné qu'une fois ; au-delà de [`MAX_TIMELINE_TASKS`] éléments, le reste
/// est compté dans `hidden`. Jamais de panique, même résultat à chaque
/// appel.
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "frise disposée d'un bloc, dans l'ordre de dessin de H-UI-80"
)]
pub fn layout(input: &TimelineInput, width16: i64) -> TimelineLayout {
    let w = width16.clamp(0, MAX_WIDTH16);
    let mut shapes = vec![TimelineShape::Band {
        rect16: [0, 0, w, TIMELINE_HEIGHT16],
    }];

    // Étiquettes d'extrémité et marges de l'axe.
    let room = w
        .saturating_sub(PAD16.saturating_add(AXIS_GAP16).saturating_mul(2))
        .saturating_sub(MIN_AXIS16)
        .max(0)
        / 2;
    let bound = |s: &str| {
        if s.is_empty() {
            None
        } else {
            fit_text(s, room, TIMELINE_FONT_STRONG, &InterMetrics)
        }
    };
    let start_label = bound(&input.start_label);
    let finish_label = bound(&input.finish_label);
    let margin = |l: &Option<(String, i64)>| {
        l.as_ref().map_or(PAD16, |(_, lw)| {
            PAD16.saturating_add(AXIS_GAP16).saturating_add(*lw)
        })
    };
    let x0 = margin(&start_label).min(w);
    let x1 = w.saturating_sub(margin(&finish_label)).max(x0);
    let start = input.start.minutes();
    let span = input.finish.minutes().saturating_sub(start);
    let line = Line {
        start,
        span: if span <= 0 { MIN_SPAN_MINUTES } else { span },
        x0,
        len: x1.saturating_sub(x0),
    };

    // Éléments : UID distincts, dans la borne ; tri par (début, ID, UID).
    let mut seen = std::collections::BTreeSet::new();
    let mut valid: Vec<usize> = Vec::new();
    let mut hidden: u32 = 0;
    for (i, item) in input.items.iter().enumerate() {
        if !seen.insert(item.uid) {
            continue;
        }
        if valid.len() >= MAX_TIMELINE_TASKS {
            hidden = hidden.saturating_add(1);
            continue;
        }
        valid.push(i);
    }
    let item_at = |i: usize| input.items.get(i);
    valid.sort_by_key(|&i| item_at(i).map(|it| (it.start, it.id, it.uid)));

    // Barres : premier ajustement sur trois rangées.
    let mut rows_end: [Option<i64>; TIMELINE_ROWS] = [None; TIMELINE_ROWS];
    let mut placed: Vec<Placed> = Vec::new();
    let mut milestones: Vec<usize> = Vec::new();
    for &i in &valid {
        let Some(it) = item_at(i) else { continue };
        if it.kind == TimelineKind::Milestone {
            milestones.push(i);
            continue;
        }
        let mut bx0 = line.x(it.start);
        let mut bx1 = line.x(it.finish).max(bx0.saturating_add(SUB));
        if bx1 > x1 {
            bx1 = x1;
            bx0 = bx0.min(x1.saturating_sub(SUB).max(x0));
        }
        let row = rows_end
            .iter()
            .position(|end| end.is_none_or(|e| bx0 >= e.saturating_add(BAR_GAP16)));
        match row.and_then(|r| rows_end.get_mut(r).map(|slot| (r, slot))) {
            Some((r, slot)) => {
                *slot = Some(bx1);
                placed.push(Placed {
                    index: i,
                    row: r,
                    x0: bx0,
                    x1: bx1,
                });
            }
            None => hidden = hidden.saturating_add(1),
        }
    }
    for p in &placed {
        let Some(it) = item_at(p.index) else { continue };
        shapes.push(TimelineShape::Bar {
            uid: it.uid,
            row: u8::try_from(p.row).unwrap_or(0),
            rect16: [
                p.x0,
                ROW_TOP16.get(p.row).copied().unwrap_or(0),
                p.x1.saturating_sub(p.x0),
                ROW_H16,
            ],
            inactive: it.inactive,
        });
    }

    // Axe, graduations, date d'état.
    shapes.push(TimelineShape::Axis {
        y16: AXIS_Y16,
        x16: (x0, x1),
    });
    let ticks: Vec<(i64, &str)> = input
        .ticks
        .iter()
        .filter(|(t, _)| line.contains(*t))
        .map(|(t, s)| (line.x(*t), s.as_str()))
        .collect();
    for &(x, _) in &ticks {
        shapes.push(TimelineShape::Tick { x16: x });
    }
    let status_x = input.status.filter(|t| line.contains(*t)).map(|t| line.x(t));
    if let Some(x) = status_x {
        shapes.push(TimelineShape::StatusLine { x16: x });
    }

    // Jalons : losanges sur l'axe.
    let half = DIAMOND_HALF16.min(w / 2);
    let mut diamonds: Vec<(usize, i64)> = Vec::new();
    for &i in &milestones {
        let Some(it) = item_at(i) else { continue };
        let cx = line.x(it.start).clamp(half, w.saturating_sub(half).max(half));
        diamonds.push((i, cx));
        shapes.push(TimelineShape::Diamond {
            uid: it.uid,
            center16: (cx, AXIS_Y16),
            half16: half,
            inactive: it.inactive,
        });
    }

    // Textes.
    let mut texts: Vec<TimelineShape> = Vec::new();
    let mut push_text = |x16: i64, y16: i64, text: String, role: TimelineText, uid: Option<TaskUid>| {
        if !text.is_empty() {
            texts.push(TimelineShape::Text {
                x16,
                y16,
                text,
                role,
                uid,
            });
        }
    };
    if let Some((s, _)) = start_label {
        push_text(PAD16.min(w), BOUND_BASE16, s, TimelineText::Bound, None);
    }
    if let Some((s, lw)) = finish_label {
        let x = w.saturating_sub(PAD16).saturating_sub(lw).max(0);
        push_text(x, BOUND_BASE16, s, TimelineText::Bound, None);
    }

    // Compteur « +n » au bord droit de la première rangée.
    let row0_base = ROW_TOP16[0].saturating_add(name_base_offset());
    let mut row0_limit = x1;
    if hidden > 0 {
        let s = format!("+{hidden}");
        let ow = width_of(&s, TimelineText::Overflow);
        let ox = w.saturating_sub(PAD16).saturating_sub(ow);
        if ox >= 0 {
            row0_limit = row0_limit.min(ox.saturating_sub(TEXT_GAP16));
            push_text(ox, row0_base, s, TimelineText::Overflow, None);
        }
    }

    // Noms des barres : dedans, ou à droite d'une barre étroite s'il y a la
    // place avant la barre suivante de la rangée.
    for p in &placed {
        let Some(it) = item_at(p.index) else { continue };
        let top = ROW_TOP16.get(p.row).copied().unwrap_or(0);
        let base = top.saturating_add(name_base_offset());
        let limit = if p.row == 0 { row0_limit } else { x1 };
        let next = placed
            .iter()
            .filter(|q| q.row == p.row && q.x0 > p.x1)
            .map(|q| q.x0)
            .min()
            .map_or(limit, |n| n.saturating_sub(TEXT_GAP16).min(limit));
        let bar_w = p.x1.saturating_sub(p.x0);
        let (x, room) = if bar_w >= NARROW_BAR16 {
            let x = p.x0.saturating_add(NAME_PAD16);
            (x, p.x1.saturating_sub(NAME_PAD16).min(limit).saturating_sub(x))
        } else {
            let x = p.x1.saturating_add(NAME_PAD16);
            (x, next.saturating_sub(x))
        };
        if room <= 0 {
            continue;
        }
        if let Some((s, _)) = fit_text(&it.name, room, TIMELINE_FONT, &InterMetrics) {
            push_text(x, base, s, TimelineText::BarName, Some(it.uid));
        }
    }

    // Étiquettes des jalons : au-dessus, au-dessous si elle chevaucherait la
    // précédente du dessus.
    let mut top = Occupied::default();
    let mut above_end: Option<i64> = None;
    let mut below_end: Option<i64> = None;
    let date_gap = width_of(" ", TimelineText::MilestoneName);
    let ellipsis16 = width_of("…", TimelineText::MilestoneName);
    for (k, &(i, cx)) in diamonds.iter().enumerate() {
        let Some(it) = item_at(i) else { continue };
        let date_w = width_of(&it.date_label, TimelineText::MilestoneDate);
        let name_w = width_of(&it.name, TimelineText::MilestoneName);
        let full = if it.name.is_empty() {
            date_w
        } else if it.date_label.is_empty() {
            name_w
        } else {
            date_w.saturating_add(date_gap).saturating_add(name_w)
        }
        .min(MILESTONE_LABEL16);
        let next_cx = diamonds.get(k.saturating_add(1)).map(|&(_, c)| c);
        // Bande haute : de la marge à la marge ; bande basse : sur l'axe,
        // avant le losange suivant.
        let slots = [
            (
                cx.saturating_sub(half),
                PAD16.min(w),
                w.saturating_sub(PAD16),
                above_end,
                TOP_BASE16,
            ),
            (
                cx.saturating_add(half).saturating_add(2 * SUB),
                x0,
                next_cx.map_or(x1, |n| n.saturating_sub(half).saturating_sub(2 * SUB).min(x1)),
                below_end,
                BOTTOM_BASE16,
            ),
        ];
        let position = |(want, left, right, end, _): (i64, i64, i64, Option<i64>, i64)| {
            let floor = end.map_or(left, |e| e.saturating_add(TEXT_GAP16).max(left));
            let mut x = want.max(floor);
            if x.saturating_add(full) > right {
                x = right.saturating_sub(full).max(floor);
            }
            (
                x,
                right.saturating_sub(x),
                x >= floor && x.saturating_add(full) <= right,
            )
        };
        // Au-dessus si l'étiquette, à sa place, ne chevauche pas la
        // précédente du dessus ; sinon au-dessous (tronquée au besoin) ;
        // au pire au-dessus, décalée et tronquée.
        let fits_above = slots.first().is_some_and(|s| {
            s.0.max(s.1) >= s.3.map_or(s.1, |e| e.saturating_add(TEXT_GAP16)) && position(*s).2
        });
        let below_room = slots.get(1).map_or(0, |s| position(*s).1);
        let choice = usize::from(!fits_above && below_room >= ellipsis16);
        let Some(&slot) = slots.get(choice) else { continue };
        let (x, room, _) = position(slot);
        let room = room.min(MILESTONE_LABEL16);
        if room <= 0 {
            continue;
        }
        let base = slot.4;
        let mut cursor = x;
        let mut end = x;
        if !it.date_label.is_empty()
            && let Some((s, sw)) = fit_text(&it.date_label, room, TIMELINE_FONT, &InterMetrics)
        {
            end = cursor.saturating_add(sw);
            cursor = end.saturating_add(date_gap);
            push_text(x, base, s, TimelineText::MilestoneDate, Some(it.uid));
        }
        let left = x.saturating_add(room).saturating_sub(cursor);
        if !it.name.is_empty()
            && left > 0
            && let Some((s, sw)) = fit_text(&it.name, left, TIMELINE_FONT, &InterMetrics)
        {
            push_text(cursor, base, s, TimelineText::MilestoneName, Some(it.uid));
            end = cursor.saturating_add(sw);
        }
        if end > x {
            if choice == 0 {
                above_end = Some(end);
                top.take(x, end);
            } else {
                below_end = Some(end);
            }
        }
    }

    // Date d'état, dans la bande haute, à droite du trait (à gauche s'il
    // n'y a pas la place).
    if let (Some(sx), Some(label)) = (status_x, input.status_label.as_deref())
        && !label.is_empty()
    {
        let sw = width_of(label, TimelineText::Status);
        let right = sx.saturating_add(2 * SUB);
        let x = if right.saturating_add(sw) <= w.saturating_sub(PAD16) {
            right
        } else {
            sx.saturating_sub(2 * SUB).saturating_sub(sw)
        };
        let end = x.saturating_add(sw);
        if x >= PAD16.min(w) && end <= w && top.free(x, end) {
            top.take(x, end);
            push_text(x, TOP_BASE16, label.to_owned(), TimelineText::Status, None);
        }
    }

    // Graduations : texte après le trait, avant la graduation suivante.
    for (k, &(tx, label)) in ticks.iter().enumerate() {
        if label.is_empty() {
            continue;
        }
        let x = tx.saturating_add(2 * SUB);
        let next = ticks
            .get(k.saturating_add(1))
            .map_or(w.saturating_sub(PAD16), |&(n, _)| n.min(w.saturating_sub(PAD16)));
        let tw = width_of(label, TimelineText::Tick);
        let end = x.saturating_add(tw);
        if end <= next && top.free(x, end) {
            top.take(x, end);
            push_text(x, TOP_BASE16, label.to_owned(), TimelineText::Tick, None);
        }
    }

    shapes.extend(texts);
    TimelineLayout {
        height16: TIMELINE_HEIGHT16,
        shapes,
        hidden,
    }
}

/// Ligne de base d'un nom dans sa rangée : centre de la rangée plus un
/// tiers du corps (comme H-R1).
const fn name_base_offset() -> i64 {
    // 160 / 3 : tiers du corps de 10 px (TIMELINE_FONT), en 1/16 px.
    ROW_H16 / 2 + 160 / 3
}

/// Classe d'un texte de la frise (`inside` : nom dans sa barre).
const fn text_class(role: TimelineText, inside: Option<bool>) -> Class {
    match role {
        TimelineText::Bound => Class::TextStrong,
        TimelineText::Tick | TimelineText::MilestoneDate | TimelineText::Overflow => Class::TextMuted,
        TimelineText::Status => Class::StatusLabel,
        TimelineText::BarName => match inside {
            Some(true) => Class::TextOnBar(BarRole::Task, None),
            Some(false) => Class::TextOnBar(BarRole::Inactive, None),
            None => Class::Text,
        },
        TimelineText::MilestoneName => Class::Text,
    }
}

/// Primitives de la scène de la frise, dans l'ordre de dessin, origine en
/// haut à gauche de la frise (découpe aucune, partie fixe) : fond
/// (`HeaderBg`), barres (`Bar(Task)`, `Bar(Inactive)`), axe et graduations
/// (`GridMajor`), date d'état (`StatusLine`), losanges (`Bar(Milestone)`,
/// `Bar(Inactive)`), textes (`TextStrong`, `TextMuted`, `StatusLabel`,
/// `Text`, `TextOnBar`). Lues par le SVG, le PDF et l'aperçu.
#[must_use]
#[allow(clippy::too_many_lines, reason = "une branche par forme de la frise")]
pub fn items(layout: &TimelineLayout) -> Vec<Item> {
    let item = |class: Class, prim: Prim| Item {
        clip: Clip::None,
        part: Part::Fixed,
        class,
        prim,
    };
    let rect = |r: [i64; 4]| Rect16 {
        x: r[0],
        y: r[1],
        w: r[2],
        h: r[3],
    };
    // Barre de chaque tâche (nom dedans ou dehors, inactive ou non).
    let bars: BTreeMap<TaskUid, ([i64; 4], bool)> = layout
        .shapes
        .iter()
        .filter_map(|s| match s {
            TimelineShape::Bar {
                uid,
                rect16,
                inactive,
                ..
            } => Some((*uid, (*rect16, *inactive))),
            _ => None,
        })
        .collect();
    layout
        .shapes
        .iter()
        .map(|s| match s {
            TimelineShape::Band { rect16 } => item(
                Class::HeaderBg,
                Prim::Rect {
                    r: rect(*rect16),
                    radius16: 0,
                },
            ),
            TimelineShape::Bar { rect16, inactive, .. } => item(
                Class::Bar(if *inactive {
                    BarRole::Inactive
                } else {
                    BarRole::Task
                }),
                Prim::Rect {
                    r: rect(*rect16),
                    radius16: BAR_RADIUS16,
                },
            ),
            TimelineShape::Axis { y16, x16 } => item(
                Class::GridMajor,
                Prim::Segments {
                    segs: vec![(x16.0, *y16, x16.1, *y16)],
                },
            ),
            TimelineShape::Tick { x16 } => item(
                Class::GridMajor,
                Prim::Segments {
                    segs: vec![(
                        *x16,
                        AXIS_Y16.saturating_sub(TICK_HALF16),
                        *x16,
                        AXIS_Y16.saturating_add(TICK_HALF16),
                    )],
                },
            ),
            TimelineShape::StatusLine { x16 } => item(
                Class::StatusLine,
                Prim::Segments {
                    segs: vec![(*x16, ROW_TOP16[0], *x16, AXIS_Y16.saturating_add(TICK_HALF16))],
                },
            ),
            TimelineShape::Diamond {
                center16,
                half16,
                inactive,
                ..
            } => item(
                Class::Bar(if *inactive {
                    BarRole::Inactive
                } else {
                    BarRole::Milestone
                }),
                Prim::Diamond {
                    r: Rect16 {
                        x: center16.0.saturating_sub(*half16),
                        y: center16.1.saturating_sub(*half16),
                        w: half16.saturating_mul(2),
                        h: half16.saturating_mul(2),
                    },
                },
            ),
            TimelineShape::Text {
                x16,
                y16,
                text,
                role,
                uid,
            } => {
                let inside = match (role, uid.and_then(|u| bars.get(&u))) {
                    (TimelineText::BarName, Some((r, inactive))) => {
                        let within = *x16 >= r[0] && *x16 < r[0].saturating_add(r[2]);
                        within.then_some(!*inactive)
                    }
                    _ => None,
                };
                item(
                    text_class(*role, inside),
                    Prim::Text {
                        x16: *x16,
                        y16: *y16,
                        text: text.clone(),
                        font: role.font(),
                        anchor: Anchor::Start,
                    },
                )
            }
        })
        .collect()
}

/// Peintures des classes de la frise ([`items`]) pour le style `st` et la
/// variante `v` : jetons existants (fond d'en-tête, barre de tâche, jalon,
/// inactive, grille, date d'état, textes), aucun jeton nouveau.
#[must_use]
pub fn paints(st: &PlanStyle, v: Variant) -> BTreeMap<Class, Paint> {
    let paint = crate::scene::paints_of(st, v);
    [
        Class::HeaderBg,
        Class::Bar(BarRole::Task),
        Class::Bar(BarRole::Inactive),
        Class::Bar(BarRole::Milestone),
        Class::GridMajor,
        Class::StatusLine,
        Class::StatusLabel,
        Class::Text,
        Class::TextStrong,
        Class::TextMuted,
        Class::TextOnBar(BarRole::Task, None),
        Class::TextOnBar(BarRole::Inactive, None),
    ]
    .into_iter()
    .map(|c| (c, paint(c)))
    .collect()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use plannercraft_core::Moment;

    use super::{TIMELINE_HEIGHT16, TimelineInput, TimelineShape, layout};

    fn input() -> TimelineInput {
        TimelineInput {
            start: Moment::from_minutes(0).unwrap(),
            finish: Moment::from_minutes(60).unwrap(),
            status: None,
            start_label: String::new(),
            finish_label: String::new(),
            status_label: None,
            ticks: Vec::new(),
            items: Vec::new(),
        }
    }

    /// Frise vide : bande et axe seuls, largeur bornée, jamais de panique.
    #[test]
    fn timeline_empty_band_and_axis() {
        for width in [i64::MIN, -1, 0, 16_000, i64::MAX] {
            let out = layout(&input(), width);
            assert_eq!(out.height16, TIMELINE_HEIGHT16);
            assert_eq!(out.hidden, 0);
            assert!(matches!(
                out.shapes.as_slice(),
                [TimelineShape::Band { rect16: [0, 0, w, TIMELINE_HEIGHT16] }, TimelineShape::Axis { .. }] if *w >= 0
            ));
        }
    }
}
