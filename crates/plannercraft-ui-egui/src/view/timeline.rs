//! Chronologie à l'écran (ADR-0017 D7, H-UI-78 à H-UI-81, H-UI-110,
//! H-UI-111) : frise au-dessus de la table et du Gantt (72 points par
//! défaut, hauteur `UiState::timeline_height` réglée par le séparateur du
//! volet), disposée par
//! `plannercraft_automation::timeline::input` puis `plannercraft_render::timeline::layout`
//! (aucune date calculée ici, règle 3), mise en cache.
//!
//! - **Cache** : la disposition et ses primitives ne sont refaites qu'au
//!   changement de la clé ([`TimelineKey`] : génération du document,
//!   empreinte du numéro d'ouverture et de la liste de la frise du style
//!   effectif, largeur arrondie au pixel, langue). Une image immobile ne
//!   recalcule rien ([`TimelineView::layouts_computed`]).
//! - **Dessin** : primitives de `plannercraft_render::timeline::items`, peintes par
//!   [`super::scene_paint::items_shapes`] (mêmes formes que l'aperçu, le
//!   SVG et le PDF ; date d'état en tirets), échelle 1/16 px → 1 point,
//!   peintures du style effectif (`timeline::paints`), police du planning.
//! - **Partie visible du Gantt (H-UI-81)** : rectangle obtenu par une
//!   projection pixel → pixel : les abscisses du Gantt du début et de la fin
//!   du projet (`TimeAxis::x`, date → pixel, seul sens permis) sont
//!   appariées aux extrémités de l'axe de la frise ; les bords de la partie
//!   visible du Gantt suivent le même rapport. Aucune date n'est calculée.
//! - **Clics** : une zone par barre et par losange (`widget_info` au nom de
//!   la tâche) : clic ⇒ `Timeline(Activate(uid))` (sélection et défilement) ;
//!   clic droit ⇒ menu « Retirer de la chronologie »
//!   (`Timeline(Remove(uid))`) et « Masquer la chronologie ». Clic ailleurs :
//!   rien ; clic droit ailleurs ⇒ « Masquer la chronologie » (H-UI-110).
//! - **Fermer (H-UI-110)** : × de 16 points au coin haut droit, peint au
//!   survol de la frise et au focus clavier (F6) ; menu et × poussent
//!   `Timeline(SetVisible(false))`, comme la case Affichage → Chronologie.
//! - **Hauteur (H-UI-111)** : la disposition du moteur de rendu a la
//!   hauteur fixe de 72 px (trois lignes) ; plus haute, la frise est
//!   centrée dans le volet et son fond étendu ; plus basse (48 points au
//!   moins), elle est réduite à l'échelle (disposition faite pour la
//!   largeur agrandie d'autant, textes compris).

use egui::{Color32, FontFamily, Id, Rect, Sense, Shape, Stroke, pos2};
use plannercraft_core::{Moment, TaskUid};
use plannercraft_render::geometry::{Class, Item, Prim};
use plannercraft_render::timeline::{TimelineLayout, TimelineShape};

use super::FrameCtx;
use super::scene_paint::{Ink, Projection, items_shapes};
use crate::action::{Action, TimelineOp};
use crate::geom::i64_to_f32;
use crate::i18n::{Language, TextId};
use crate::timescale::TimeAxis;

/// Entrées du menu contextuel de la table, du Gantt et de la frise
/// (H-UI-79, H-UI-81) : ajouter, retirer.
pub const CONTEXT_LABELS: (TextId, TextId) = (TextId::TimelineAdd, TextId::TimelineRemove);

/// Hauteur par défaut de la frise et hauteur de sa disposition, en points
/// (72, H-UI-78 ; `plannercraft_render::timeline::TIMELINE_HEIGHT16`).
pub const TIMELINE_H: f32 = 72.0;

/// Côté du bouton × de la frise, en points (H-UI-110).
const CLOSE_SIDE: f32 = 16.0;

/// Identifiant du × de la frise (focus de F6, H-UI-111).
#[must_use]
pub fn close_id() -> Id {
    Id::unique("plannercraft.timeline.close")
}

/// Mise à l'échelle de la frise dans un volet de hauteur `height` :
/// `(échelle, décalage vertical)` ; échelle ≤ 1 (volet plus bas que la
/// disposition), décalage ≥ 0 (volet plus haut : disposition centrée).
/// Hauteur non finie ou nulle ⇒ `(1, 0)`.
#[must_use]
pub fn fit(height: f32) -> (f32, f32) {
    if !height.is_finite() || height <= 0.0 {
        return (1.0, 0.0);
    }
    if height < TIMELINE_H {
        (height / TIMELINE_H, 0.0)
    } else {
        (1.0, ((height - TIMELINE_H) * 0.5).floor())
    }
}

/// Seizièmes de pixel par point (échelle de la frise à l'écran).
const SUB: f32 = 16.0;

/// Largeur de travail au plus (pixels) : bien au-delà d'un écran 8K.
const MAX_WIDTH_PX: i64 = 100_000;

/// Clé du cache de la disposition : génération du document, empreinte du
/// style effectif, largeur arrondie au pixel, langue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimelineKey {
    /// Génération du document.
    pub generation: u64,
    /// Empreinte du style effectif : numéro d'ouverture du document et
    /// liste des tâches de la frise (seule partie du style lue par la
    /// disposition).
    pub style: u64,
    /// Largeur de la disposition, en pixels entiers.
    pub width_px: u32,
    /// Hauteur du volet, en pixels entiers (fond étendu).
    pub height_px: u32,
    /// Langue des textes.
    pub language: Language,
}

/// Relevé de la frise à la dernière image (tests, `h_ui_80_*`, `h_ui_81_*`).
#[derive(Clone, Debug, PartialEq)]
pub struct TimelineDrawn {
    /// Rectangle de la frise.
    pub rect: Rect,
    /// Rectangle de la partie visible du Gantt (H-UI-81), s'il tombe sur
    /// la frise.
    pub window: Option<Rect>,
    /// Zones cliquables : barre ou losange de chaque tâche dessinée.
    pub elements: Vec<(TaskUid, Rect)>,
    /// Rectangles peints (fond, barres), dans l'ordre de dessin.
    pub painted: Vec<Rect>,
    /// Bouton × peint (survol de la frise ou focus clavier, H-UI-110).
    pub close: Option<Rect>,
}

/// État transitoire de la frise (jamais persisté).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TimelineView {
    /// Dernière disposition et sa clé.
    pub cache: Option<(TimelineKey, TimelineLayout)>,
    /// Dispositions calculées depuis la création (tests du cache).
    pub layouts_computed: u64,
    /// Primitives de la disposition en cache (`timeline::items`).
    items: Vec<Item>,
    /// Début et fin du projet de la disposition en cache (du moteur).
    bounds: Option<(Moment, Moment)>,
    /// Relevé de la dernière image (`None` : frise non dessinée).
    pub drawn: Option<TimelineDrawn>,
}

/// La frise est-elle montrée en tête de la vue Gantt (préférence cochée,
/// document ouvert) ?
#[must_use]
pub fn wanted(fc: &FrameCtx<'_>) -> bool {
    fc.state.timeline_visible && fc.doc.is_some()
}

/// Largeur de la frise en pixels entiers, bornée à `[0, 100 000]` (0 si
/// non finie) : seule conversion `f32` → entier de la frise (règle 10).
#[must_use]
pub fn width_px(width: f32) -> u32 {
    let px = crate::geom::f32_to_bounded_i64(width.round(), MAX_WIDTH_PX).max(0);
    u32::try_from(px).unwrap_or(0)
}

/// Empreinte FNV-1a 64 bits d'une suite d'entiers (déterministe).
fn fingerprint(values: impl IntoIterator<Item = u64>) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    values.into_iter().fold(OFFSET, |hash, v| {
        v.to_le_bytes()
            .into_iter()
            .fold(hash, |h, b| (h ^ u64::from(b)).wrapping_mul(PRIME))
    })
}

/// Langue des textes du moteur.
const fn engine_language(lang: Language) -> plannercraft_automation::text::Language {
    match lang {
        Language::Fr => plannercraft_automation::text::Language::Fr,
        Language::En => plannercraft_automation::text::Language::En,
    }
}

/// Rectangle d'écran d'un rectangle de la frise (1/16 px, origine `o`,
/// échelle `scale` points par pixel de la disposition).
fn screen_rect(origin: egui::Pos2, scale: f32, rect16: [i64; 4]) -> Rect {
    let px = |v: i64| i64_to_f32(v) / SUB * scale;
    let [left, top, width, height] = rect16.map(px);
    Rect::from_min_size(pos2(origin.x + left, origin.y + top), egui::vec2(width, height))
}

/// Zones cliquables de la disposition : barre ou losange de chaque tâche.
fn elements(layout: &TimelineLayout, o: egui::Pos2, scale: f32) -> Vec<(TaskUid, Rect)> {
    layout
        .shapes
        .iter()
        .filter_map(|s| match s {
            TimelineShape::Bar { uid, rect16, .. } => Some((*uid, screen_rect(o, scale, *rect16))),
            TimelineShape::Diamond {
                uid,
                center16,
                half16,
                ..
            } => {
                let side = half16.saturating_mul(2);
                Some((
                    *uid,
                    screen_rect(
                        o,
                        scale,
                        [
                            center16.0.saturating_sub(*half16),
                            center16.1.saturating_sub(*half16),
                            side,
                            side,
                        ],
                    ),
                ))
            }
            _ => None,
        })
        .filter(|(_, r)| r.is_finite())
        .collect()
}

/// Abscisses (1/16 px) des extrémités de l'axe de la frise.
fn axis_x16(layout: &TimelineLayout) -> Option<(i64, i64)> {
    layout.shapes.iter().find_map(|s| match s {
        TimelineShape::Axis { x16, .. } => Some(*x16),
        _ => None,
    })
}

/// Partie visible du Gantt sur la frise (H-UI-81) : projection pixel →
/// pixel. `gantt` : axe de l'écran et rectangle visible du Gantt ;
/// `bounds` : début et fin du projet (du moteur) ; `axis` : abscisses
/// d'écran des extrémités de l'axe de la frise. `None` si le projet est de
/// durée nulle à l'écran ou si la partie visible tombe hors de la frise.
#[must_use]
pub fn visible_window(
    rect: Rect,
    axis: (f32, f32),
    gantt: (&TimeAxis, Rect),
    bounds: (Moment, Moment),
) -> Option<Rect> {
    let (time_axis, clip) = gantt;
    let (gs, gf) = (time_axis.x(bounds.0), time_axis.x(bounds.1));
    let span = gf - gs;
    let (fx0, fx1) = axis;
    if !span.is_finite()
        || span < 1.0
        || !rect.is_finite()
        || !clip.is_finite()
        || !fx0.is_finite()
        || !fx1.is_finite()
    {
        return None;
    }
    let k = (fx1 - fx0) / span;
    let project = |x: f32| (fx0 + (x - gs) * k).clamp(rect.left(), rect.right());
    let (left, right) = (project(clip.left()), project(clip.right()));
    (left.is_finite() && right.is_finite() && right - left >= 1.0)
        .then(|| Rect::from_min_max(pos2(left, rect.top() + 1.0), pos2(right, rect.bottom() - 2.0)))
}

impl TimelineView {
    /// Disposition à jour pour `key` (recalculée seulement si la clé a
    /// changé) ; `None` sans document.
    fn refresh(&mut self, fc: &FrameCtx<'_>, key: TimelineKey) {
        let Some(doc) = fc.doc else {
            return;
        };
        if self.cache.as_ref().map(|(k, _)| *k) != Some(key) {
            let spec = &fc.task_view.style.style.timeline;
            let input =
                plannercraft_automation::timeline::input(&doc.session, spec, engine_language(key.language));
            let width16 = i64::from(key.width_px).saturating_mul(16);
            let out = plannercraft_render::timeline::layout(&input, width16);
            self.items = plannercraft_render::timeline::items(&out);
            extend_band(&mut self.items, key.height_px);
            self.bounds = Some((input.start, input.finish));
            self.cache = Some((key, out));
            self.layouts_computed = self.layouts_computed.saturating_add(1);
        }
    }
}

/// Volet plus haut que la disposition (H-UI-111) : fond de la frise
/// (première bande, `Class::HeaderBg`) étendu à toute la hauteur du volet,
/// la disposition étant centrée (décalage de [`fit`]).
fn extend_band(items: &mut [Item], height_px: u32) {
    let (scale, offset) = fit(f32::from(u16::try_from(height_px).unwrap_or(u16::MAX)));
    if scale < 1.0 || offset <= 0.0 {
        return;
    }
    let off16 = crate::geom::f32_to_bounded_i64(offset * SUB, MAX_WIDTH_PX.saturating_mul(16));
    let h16 = i64::from(height_px).saturating_mul(16);
    if let Some(Prim::Rect { r, .. }) = items
        .iter_mut()
        .find(|it| it.class == Class::HeaderBg && matches!(it.prim, Prim::Rect { .. }))
        .map(|it| &mut it.prim)
    {
        r.y = r.y.saturating_sub(off16);
        r.h = h16.saturating_add(2 * 16);
    }
}

/// Frise sans tâche dessinée : comment y ajouter des tâches (texte
/// atténué au centre, découverte).
fn empty_hint(painter: &egui::Painter, fc: &FrameCtx<'_>, rect: Rect) {
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        fc.texts.get(TextId::TimelineEmptyHint),
        egui::FontId::proportional(12.0),
        fc.palette.text_weak,
    );
}

/// Peintures de la frise **à l'écran** (finitions P4, H-UI-134) : fond
/// neutre, barres en teinte légère de l'accent avec contour à l'accent,
/// losanges, date d'état et son étiquette à l'accent, texte des barres
/// lisible (jetons de [`crate::theme::ChromeTokens`]). Les tâches inactives,
/// l'axe et les autres textes gardent les peintures du style ; l'aperçu, le
/// SVG et le PDF gardent toutes celles du style du document (la frise
/// imprimée appartient au document, l'accent à l'interface).
#[must_use]
pub fn screen_paints(
    mut paints: std::collections::BTreeMap<Class, plannercraft_render::geometry::Paint>,
    t: &crate::theme::ChromeTokens,
) -> std::collections::BTreeMap<Class, plannercraft_render::geometry::Paint> {
    use plannercraft_render::geometry::Paint;
    use plannercraft_render::style::{BarRole, Rgb};
    let rgb = |c: Color32| Rgb(c.r(), c.g(), c.b());
    let solid = |fill: Color32| Paint {
        fill: Some(rgb(fill)),
        stroke: None,
        width16: 0,
        dash16: None,
    };
    paints.insert(Class::HeaderBg, solid(t.timeline_bg));
    paints.insert(
        Class::Bar(BarRole::Task),
        Paint {
            fill: Some(rgb(t.timeline_bar)),
            stroke: Some(rgb(t.accent)),
            width16: 16,
            dash16: None,
        },
    );
    paints.insert(Class::Bar(BarRole::Milestone), solid(t.accent));
    paints.insert(Class::TextOnBar(BarRole::Task, None), solid(t.timeline_bar_text));
    paints.insert(Class::StatusLabel, solid(t.accent));
    let status = paints.get(&Class::StatusLine).copied();
    paints.insert(
        Class::StatusLine,
        Paint {
            stroke: Some(rgb(t.accent)),
            ..status.unwrap_or(Paint {
                fill: None,
                stroke: None,
                width16: 16,
                dash16: None,
            })
        },
    );
    paints
}

/// Entrée « Masquer la chronologie » d'un menu contextuel (H-UI-110).
fn hide_entry(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    if ui.button(fc.texts.get(TextId::TimelineHide)).clicked() {
        fc.actions.push(Action::Timeline(TimelineOp::SetVisible(false)));
        ui.close();
    }
}

/// Dessine la frise dans `rect` (en tête de la vue Gantt : habillage
/// ruban, frise visible, vue Gantt ou Gantt de suivi, voir [`wanted`]) ;
/// `gantt` : axe et rectangle visible du Gantt de cette image, pour le
/// rectangle de la partie visible (H-UI-81).
#[allow(
    clippy::too_many_lines,
    reason = "dessin, zones cliquables et × de la frise, dans l'ordre"
)]
pub fn show(
    ui: &mut egui::Ui,
    fc: &mut FrameCtx<'_>,
    view: &mut TimelineView,
    rect: Rect,
    gantt: Option<(&TimeAxis, Rect)>,
) {
    view.drawn = None;
    let Some(doc) = fc.doc else {
        return;
    };
    if !rect.is_finite() || rect.width() < 1.0 || rect.height() < 1.0 {
        return;
    }
    // Volet plus bas que la disposition : réduite à l'échelle (faite pour
    // une largeur agrandie d'autant) ; plus haut : centrée.
    let (scale, offset) = fit(rect.height());
    let screen = fc.task_view.style.clone();
    let tasks = screen.style.timeline.tasks.iter().map(|uid| u64::from(*uid));
    let key = TimelineKey {
        generation: doc.generation(),
        style: fingerprint(std::iter::once(doc.opening).chain(tasks)),
        width_px: width_px(rect.width() / scale),
        height_px: width_px(rect.height()),
        language: fc.texts.language(),
    };
    view.refresh(fc, key);
    let Some((_, layout)) = &view.cache else {
        return;
    };
    let origin = pos2(rect.left(), rect.top() + offset);
    let found = elements(layout, origin, scale);
    let window = gantt
        .zip(axis_x16(layout))
        .zip(view.bounds)
        .and_then(|((g, (a0, a1)), bounds)| {
            let x = |v: i64| origin.x + i64_to_f32(v) / SUB * scale;
            visible_window(rect, (x(a0), x(a1)), g, bounds)
        });

    // Dessin : primitives de la scène, puis la partie visible et le
    // séparateur bas.
    let painter = ui.painter_at(rect);
    let tokens = crate::theme::ChromeTokens::for_theme(ui.ctx().theme());
    let paints = screen_paints(
        plannercraft_render::timeline::paints(&screen.style, screen.variant),
        &tokens,
    );
    let inter = ui.ctx().fonts(|f| {
        f.families()
            .contains(&FontFamily::Name(crate::theme::PLAN_FAMILY.into()))
    });
    let ink = Ink {
        paints: &paints,
        family: screen.style.font.family,
        inter,
        dashes: true,
    };
    let pr = Projection {
        src: (0, 0),
        dst: origin,
        k: scale / SUB,
    };
    let mut shapes = Vec::with_capacity(view.items.len().saturating_add(3));
    items_shapes(&painter, &view.items, &ink, &pr, &mut shapes);
    let filled: Vec<Rect> = shapes
        .iter()
        .filter_map(|s| match s {
            Shape::Rect(r) if r.fill != Color32::TRANSPARENT => Some(r.rect),
            _ => None,
        })
        .collect();
    if let Some(w) = window {
        // Finitions P4 (H-UI-134) : cadre en violet transparent.
        shapes.push(Shape::rect_filled(w, 4.0, tokens.window_fill));
        shapes.push(Shape::rect_stroke(
            w,
            4.0,
            Stroke::new(1.5, tokens.window_stroke),
            egui::StrokeKind::Inside,
        ));
    }
    let bottom = rect.bottom() - 0.5;
    shapes.push(Shape::line_segment(
        [pos2(rect.left(), bottom), pos2(rect.right(), bottom)],
        Stroke::new(1.0, tokens.rule),
    ));
    fc.stats.shapes = fc.stats.shapes.saturating_add(shapes.len());
    painter.extend(shapes);
    if found.is_empty() {
        empty_hint(&painter, fc, rect);
    }

    // Fond de la frise : clic droit n'importe où ⇒ « Masquer la
    // chronologie » (H-UI-110) ; les éléments, interrogés après, passent
    // devant.
    let hovered = ui.rect_contains_pointer(rect);
    let background = ui.interact(
        rect,
        Id::unique("plannercraft.timeline.background"),
        Sense::click(),
    );
    background.context_menu(|ui| hide_entry(ui, fc));

    // Zones cliquables : une par élément, au nom de la tâche.
    let project = doc.project();
    let texts = fc.texts;
    for (uid, zone) in &found {
        let id = Id::unique("plannercraft.timeline").with(uid.get());
        let response = ui.interact(zone.intersect(rect), id, Sense::click());
        // Nom lu seulement quand l'accessibilité le demande.
        response.widget_info(|| {
            let name = project.tasks.get(*uid).map_or("", |t| t.name.as_str());
            egui::WidgetInfo::labeled(egui::Role::Button, true, name)
        });
        if response.clicked() {
            fc.actions.push(Action::Timeline(TimelineOp::Activate(*uid)));
        }
        response.context_menu(|ui| {
            if ui.button(texts.get(CONTEXT_LABELS.1)).clicked() {
                fc.actions.push(Action::Timeline(TimelineOp::Remove(*uid)));
                ui.close();
            }
            hide_entry(ui, fc);
        });
    }

    // × au coin haut droit, peint au survol de la frise ou au focus
    // clavier (H-UI-110).
    let close_rect = Rect::from_min_size(
        pos2(rect.right() - CLOSE_SIDE - 4.0, rect.top() + 4.0),
        egui::vec2(CLOSE_SIDE, CLOSE_SIDE),
    );
    let (clicked, close_shown) =
        super::main_view::close_button(ui, fc, close_rect, close_id(), TextId::TimelineCloseTip, hovered);
    if clicked {
        fc.actions.push(Action::Timeline(TimelineOp::SetVisible(false)));
    }
    view.drawn = Some(TimelineDrawn {
        rect,
        window,
        elements: found,
        painted: filled,
        close: close_shown.then_some(close_rect),
    });
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::float_cmp,
    reason = "tests : un échec doit s'arrêter net et lisiblement ; valeurs exactes"
)]
mod tests {
    use egui::{Rect, pos2};

    use super::{fingerprint, visible_window, width_px};
    use crate::timescale::{TimeAxis, TimeScaleZoom, ViewSpan};

    /// Règle 10 : largeur non finie, négative ou démesurée bornée.
    #[test]
    fn timeline_width_px_bounded() {
        assert_eq!(width_px(f32::NAN), 0);
        assert_eq!(width_px(f32::INFINITY), 100_000);
        assert_eq!(width_px(-5.0), 0);
        assert_eq!(width_px(1_439.6), 1_440);
        assert_eq!(width_px(1.0e12), 100_000);
    }

    /// Empreinte stable, sensible à l'ordre et au contenu.
    #[test]
    fn timeline_fingerprint_stable() {
        assert_eq!(fingerprint([1, 3, 4]), fingerprint([1, 3, 4]));
        assert_ne!(fingerprint([1, 3, 4]), fingerprint([1, 4, 3]));
        assert_ne!(fingerprint([1]), fingerprint([2]));
    }

    /// H-UI-81 : projection pixel → pixel ; bornée à la frise ; projet de
    /// durée nulle ⇒ aucun rectangle.
    #[test]
    fn h_ui_81_window_projection() {
        let start = "2026-10-19T08:00".parse().unwrap();
        let finish = "2026-11-17T17:00".parse().unwrap();
        let day = |m: u32, d: u32| {
            plannercraft_core::Day::from_naive(chrono::NaiveDate::from_ymd_opt(2026, m, d).unwrap()).unwrap()
        };
        let range = ViewSpan {
            start: day(10, 12),
            end: day(11, 30),
        };
        let axis = TimeAxis::new(range, TimeScaleZoom::Day, 0.0);
        let rect = Rect::from_min_max(pos2(0.0, 100.0), pos2(1_000.0, 172.0));
        let (gs, gf) = (axis.x(start), axis.x(finish));
        // Partie visible = projet entier ⇒ l'axe de la frise.
        let clip = Rect::from_min_max(pos2(gs, 0.0), pos2(gf, 10.0));
        let w = visible_window(rect, (100.0, 950.0), (&axis, clip), (start, finish)).unwrap();
        assert!(
            (w.left() - 100.0).abs() < 0.01 && (w.right() - 950.0).abs() < 0.01,
            "{w:?}"
        );
        // Décalage de 100 px ⇒ décalage de 100 × rapport.
        let shifted = clip.translate(egui::vec2(100.0, 0.0));
        let s = visible_window(rect, (100.0, 950.0), (&axis, shifted), (start, finish)).unwrap();
        let ratio = 850.0 / (gf - gs);
        assert!((s.left() - w.left() - 100.0 * ratio).abs() < 0.01, "{s:?}");
        assert_eq!(s.right(), rect.right());
        // Hors de la frise, ou projet de durée nulle : rien.
        let far = clip.translate(egui::vec2(1.0e6, 0.0));
        assert!(visible_window(rect, (100.0, 950.0), (&axis, far), (start, finish)).is_none());
        assert!(visible_window(rect, (100.0, 950.0), (&axis, clip), (start, start)).is_none());
    }
}
