//! Aperçu de la fenêtre « Mise en forme » (lot F1, H-UI-219) : les
//! premières lignes utiles du planning ouvert (autour de la tâche active),
//! dessinées avec le style courant tel que l'écran le montre, mises à jour
//! à l'image qui suit chaque réglage.
//!
//! Partie graphique : la scène de `plannercraft-render` (`layout`, géométrie
//! du SVG et du PDF, H-UI-42), table masquée, plage des seules lignes
//! montrées remplissant la largeur (`Zoom::Fill`). Partie tableau : dessinée ici avec les
//! **colonnes de l'écran** (`UiState.columns` : titres, largeurs,
//! alignements) et les textes du `RowModel` (règle 3 : aucun calcul), sur
//! les fonds de ligne de la scène (zébrage, bandes et couleurs de phase,
//! surlignage des tâches) : ainsi un réglage de l'onglet Tableau se voit
//! aussi. Les colonnes qui ne tiennent pas dans la moitié de l'aperçu sont
//! comptées « + n » dans l'en-tête.
//!
//! Clic : dans le tableau ⇒ onglet Tableau ; sur une barre ⇒ son type et
//! l'onglet Graphique (hit-test sur les primitives `Bar`, `Phase`,
//! `Pattern`, `Progress` de la scène, à 3 points près).
//!
//! Écart connu : la scène colore les tâches critiques même quand la
//! surbrillance du chemin critique est coupée dans la vue (comme l'aperçu
//! d'impression et le PDF).

use std::collections::BTreeSet;

use egui::{Color32, FontFamily, FontId, Pos2, Rect, Sense, Stroke, pos2, vec2};
use plannercraft_automation::Session;
use plannercraft_automation::render_text::SessionTexts;
use plannercraft_automation::text::Language;
use plannercraft_core::TaskUid;
use plannercraft_render::geometry::{Class, Clip, Part, Prim, Rect16, Scene, Viewport, layout};
use plannercraft_render::style::{BarRole, FontChoice, PlanStyle, Rgb, TemplateId, Variant};
use plannercraft_render::{Lang, RangeSpec, RenderLimits, RowFilter, Zoom};

use crate::column::{CellAlign, ColumnState};
use crate::geom::i64_to_f32;
use crate::i18n::{Arg, TextId, Texts};
use crate::row_view::{DisplayRow, VisibleRows};
use crate::rows::RowKind;
use crate::view::scene_paint::{Projection, region_shapes};

/// Lignes au plus dans l'aperçu.
pub const PREVIEW_ROWS: usize = 14;

/// Lignes montrées au-dessus de la tâche active quand c'est possible.
const ROWS_ABOVE_ACTIVE: usize = 3;

/// Part de la largeur de l'aperçu laissée au tableau.
const TABLE_SHARE: f32 = 0.5;

/// Largeur au plus de la colonne du nom dans l'aperçu, en points.
const NAME_MAX_W: f32 = 180.0;

/// Retrait d'un niveau hiérarchique dans la colonne du nom, en points.
const INDENT: f32 = 12.0;

/// Marge intérieure d'une cellule, en points.
const CELL_PAD: f32 = 5.0;

/// Tolérance du clic sur une barre, en points.
const HIT_SLOP: f32 = 3.0;

/// Clé du cache de la scène.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Key {
    /// Document (ouverture, génération).
    doc: (u64, u64),
    /// Variante dessinée.
    variant: Variant,
    /// Langue des textes.
    lang: Language,
    /// Tâches montrées.
    uids: Vec<TaskUid>,
    /// Largeur du Gantt, en points entiers.
    gantt_w: u32,
    /// Modèle des documents sans style.
    default: TemplateId,
}

/// Scène de l'aperçu gardée entre deux images (relue quand le document, sa
/// génération, la langue, le thème, les lignes ou la largeur changent).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PreviewCache {
    /// Clé et scène.
    scene: Option<(Key, Scene)>,
    /// Scènes construites depuis l'ouverture (tests : une par réglage).
    pub builds: u64,
}

/// Entrées de l'aperçu à une image.
pub struct PreviewInput<'a> {
    /// Session du document.
    pub session: &'a Session,
    /// Clé du document (ouverture, génération).
    pub key: (u64, u64),
    /// Style tel que l'écran le dessine (interface adoucie comprise).
    pub style: &'a PlanStyle,
    /// Variante de l'écran.
    pub variant: Variant,
    /// Langue des textes.
    pub lang: Language,
    /// Modèle des documents sans style.
    pub default: TemplateId,
    /// Lignes affichées par la vue (ordre, filtre, plan replié).
    pub rows: Option<VisibleRows<'a>>,
    /// Tâche active.
    pub active: Option<TaskUid>,
    /// Colonnes de la table de l'écran.
    pub columns: &'a [ColumnState],
    /// Apparence du texte des tâches mises en forme (H-UI-126).
    pub looks: &'a crate::theme::TextLooks,
}

/// Où l'aperçu a été cliqué.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewHit {
    /// Dans la partie tableau.
    Table,
    /// Dans la partie graphique, sur une barre du type donné (`None` : à
    /// côté des barres).
    Chart(Option<BarRole>),
}

/// Relevé de l'aperçu dessiné à la dernière image (tests).
#[derive(Clone, Debug, PartialEq)]
pub struct PreviewDrawn {
    /// Feuille de l'aperçu.
    pub sheet: Rect,
    /// Bord droit de la partie tableau.
    pub table_right: f32,
    /// Barres dessinées : type et rectangle à l'écran.
    pub bars: Vec<(BarRole, Rect)>,
    /// Colonnes montrées (titres), puis « + n » s'il en manque.
    pub headers: Vec<String>,
    /// Scènes construites depuis l'ouverture de la fenêtre.
    pub builds: u64,
}

/// Identifiant du relevé.
fn drawn_id() -> egui::Id {
    egui::Id::unique("plannercraft.format-window.preview-drawn")
}

/// Aperçu dessiné à la dernière image (`None` : fenêtre fermée ou sans
/// aperçu).
#[must_use]
pub fn preview_drawn(ctx: &egui::Context) -> Option<PreviewDrawn> {
    ctx.data(|d| d.get_temp::<Option<PreviewDrawn>>(drawn_id()))
        .flatten()
}

/// Efface le relevé (fenêtre fermée).
pub fn clear_drawn(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp::<Option<PreviewDrawn>>(drawn_id(), None));
}

/// Langue du rendu.
const fn render_lang(lang: Language) -> Lang {
    match lang {
        Language::Fr => Lang::Fr,
        Language::En => Lang::En,
    }
}

/// Couleur egui.
const fn color(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

/// Tâches montrées : jusqu'à [`PREVIEW_ROWS`] lignes de tâche affichées
/// (ordre de la vue, lignes vides et en-têtes de groupe écartés), à partir
/// de [`ROWS_ABOVE_ACTIVE`] lignes au-dessus de la tâche active si elle est
/// plus bas, fenêtre ramenée dans la liste.
fn pick_rows(rows: &VisibleRows<'_>, active: Option<TaskUid>) -> Vec<TaskUid> {
    let all: Vec<TaskUid> = (0..rows.len())
        .filter_map(|i| rows.row(DisplayRow::at(i)))
        .filter(|r| r.kind != RowKind::Null)
        .map(|r| r.uid)
        .collect();
    let at = active
        .and_then(|uid| all.iter().position(|u| *u == uid))
        .unwrap_or(0);
    let last_start = all.len().saturating_sub(PREVIEW_ROWS);
    let start = at.saturating_sub(ROWS_ABOVE_ACTIVE).min(last_start);
    all.into_iter().skip(start).take(PREVIEW_ROWS).collect()
}

/// Plage de dates des tâches montrées : du premier début (2 jours avant) à
/// la dernière fin, plus deux tiers de la durée (au moins 10 jours) pour
/// les textes à droite des barres.
fn range_of(rows: &VisibleRows<'_>, uids: &[TaskUid]) -> RangeSpec {
    let model = rows.model();
    let mut span: Option<(plannercraft_core::Day, plannercraft_core::Day)> = None;
    for uid in uids {
        let Some((start, finish)) = model
            .index_of(*uid)
            .and_then(|i| model.row(i))
            .and_then(|r| r.bar)
        else {
            continue;
        };
        let (a, b) = (start.day(), finish.day());
        span = Some(span.map_or((a, b), |(lo, hi)| (lo.min(a), hi.max(b))));
    }
    let Some((from, to)) = span else {
        return RangeSpec::Auto;
    };
    let days = to.index().saturating_sub(from.index());
    let after = days.saturating_mul(2).saturating_div(3).max(10);
    match (from.checked_add_days(-2), to.checked_add_days(after)) {
        (Ok(from), Ok(to)) => RangeSpec::Span { from, to },
        _ => RangeSpec::Auto,
    }
}

impl PreviewCache {
    /// Scène des tâches `uids` (relue si la clé a changé).
    fn scene(&mut self, input: &PreviewInput<'_>, uids: Vec<TaskUid>, gantt_w: u32) -> Option<&Scene> {
        let rows = input.rows.as_ref()?;
        let key = Key {
            doc: input.key,
            variant: input.variant,
            lang: input.lang,
            uids,
            gantt_w,
            default: input.default,
        };
        let fresh = self.scene.as_ref().is_some_and(|(k, _)| *k == key);
        if !fresh {
            let mut st = input.style.clone();
            st.page.columns = Some(Vec::new());
            st.legend.show = false;
            let vp = Viewport {
                range: range_of(rows, &key.uids),
                zoom: Zoom::Fill { width_px: gantt_w },
                filter: RowFilter {
                    uids: Some(key.uids.iter().copied().collect::<BTreeSet<_>>()),
                    ..RowFilter::default()
                },
                limits: RenderLimits {
                    max_rows: u32::try_from(PREVIEW_ROWS).unwrap_or(u32::MAX),
                    ..RenderLimits::default()
                },
                ..Viewport::new(input.variant, render_lang(input.lang))
            };
            let texts = SessionTexts {
                session: input.session,
                lang: input.lang,
            };
            let scene = layout(
                input.session.project(),
                input.session.schedule(),
                &st,
                &texts,
                &vp,
            )
            .0;
            self.builds = self.builds.saturating_add(1);
            self.scene = Some((key, scene));
        }
        self.scene.as_ref().map(|(_, s)| s)
    }
}

/// Seizièmes de pixel de la scène en points (1 px de scène = 1 point).
fn pts(v: i64) -> f32 {
    i64_to_f32(v) / 16.0
}

/// Rectangle d'une primitive de barre, à l'écran.
fn prim_rect(prim: &Prim, origin: Pos2) -> Option<Rect> {
    let r: Rect16 = match prim {
        Prim::Rect { r, .. } | Prim::Diamond { r } | Prim::Bracket { r, .. } | Prim::Hatch { r, .. } => *r,
        _ => return None,
    };
    let rect = Rect::from_min_size(
        pos2(origin.x + pts(r.x), origin.y + pts(r.y)),
        vec2(pts(r.w), pts(r.h)),
    );
    rect.is_finite().then_some(rect)
}

/// Type de barre d'une classe de la scène.
const fn bar_role(class: Class) -> Option<BarRole> {
    match class {
        Class::Bar(role) | Class::Phase(_, role) | Class::Pattern(role) | Class::Progress(role) => Some(role),
        _ => None,
    }
}

/// Peinture de remplissage d'une classe de la scène.
fn fill_of(scene: &Scene, class: Class) -> Option<Color32> {
    scene.paints.get(&class).and_then(|p| p.fill).map(color)
}

/// Colonnes de l'aperçu : rang dans `columns` et largeur, tant qu'elles
/// tiennent dans `room` (le N° et le nom toujours ; les indicateurs,
/// icônes de l'écran, jamais) ; puis le nombre de colonnes laissées.
fn fit_columns(columns: &[ColumnState], room: f32) -> (Vec<(usize, f32)>, usize) {
    let mut used = 0.0;
    let mut shown = Vec::new();
    // Indicateurs : des icônes de l'écran, sans texte dans l'aperçu.
    let mut skipped = 0_usize;
    for (i, c) in columns.iter().enumerate() {
        if c.column == crate::column::Column::Indicators {
            skipped = skipped.saturating_add(1);
            continue;
        }
        let w = f32::from(c.width).min(if c.column == crate::column::Column::Name {
            NAME_MAX_W
        } else {
            f32::from(c.width)
        });
        let essential = !c.column.hideable();
        if !essential && used + w > room {
            break;
        }
        used += w;
        shown.push((i, w));
    }
    let left = columns.len().saturating_sub(shown.len()).saturating_sub(skipped);
    (shown, left)
}

/// Dessine l'aperçu dans `rect` ; rend où il a été cliqué.
#[allow(clippy::too_many_lines, reason = "tableau, graphique, clic : un bloc chacun")]
pub fn show(
    ui: &mut egui::Ui,
    texts: &Texts,
    cache: &mut PreviewCache,
    input: &PreviewInput<'_>,
    rect: Rect,
) -> Option<PreviewHit> {
    let response = ui.interact(
        rect,
        egui::Id::unique("plannercraft.format-window.preview"),
        Sense::click(),
    );
    let name = texts.get(TextId::FwPreviewName);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, name.clone()));
    let Some(rows) = input.rows.as_ref() else {
        empty(ui, texts, rect);
        return None;
    };
    let uids = pick_rows(rows, input.active);
    if uids.is_empty() {
        empty(ui, texts, rect);
        return None;
    }
    let (shown, left) = fit_columns(input.columns, rect.width() * TABLE_SHARE);
    let table_w: f32 = shown.iter().map(|(_, w)| *w).sum();
    let gantt_w = (rect.width() - table_w).max(40.0);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "largeur positive bornée par l'écran"
    )]
    let gantt_px = gantt_w.round().clamp(40.0, 4_000.0) as u32;
    if cache.scene(input, uids, gantt_px).is_none() {
        empty(ui, texts, rect);
        return None;
    }
    let builds = cache.builds;
    let scene = cache.scene.as_ref().map(|(_, s)| s)?;
    let painter = ui.painter_at(rect);
    let bg = fill_of(scene, Class::Bg).unwrap_or(Color32::WHITE);
    painter.rect_filled(rect, 6.0, bg);
    let header_h = pts(scene.header16);
    let table_right = rect.left() + table_w;
    // Partie graphique : la scène, table masquée.
    let gantt_rect = Rect::from_min_max(pos2(table_right, rect.top()), rect.max);
    let gantt_painter = painter.with_clip_rect(gantt_rect.intersect(painter.clip_rect()));
    let inter = ui.ctx().fonts(|f| {
        f.families()
            .contains(&FontFamily::Name(crate::theme::PLAN_FAMILY.into()))
    });
    let pr = Projection {
        src: (0, 0),
        dst: pos2(table_right, rect.top()),
        k: 1.0 / 16.0,
    };
    let mut shapes = Vec::new();
    region_shapes(
        &gantt_painter,
        scene,
        &pr,
        inter,
        &|it| it.clip != Clip::Table,
        &mut shapes,
    );
    gantt_painter.extend(shapes);
    // Partie tableau : fonds de ligne de la scène, textes du modèle.
    let table_rect = Rect::from_min_max(rect.min, pos2(table_right, rect.bottom()));
    let tp = painter.with_clip_rect(table_rect.intersect(painter.clip_rect()));
    let header_bg = fill_of(scene, Class::HeaderBg).unwrap_or(bg);
    tp.rect_filled(
        Rect::from_min_size(rect.min, vec2(table_w, header_h)),
        0.0,
        header_bg,
    );
    let family = match (scene.family, inter) {
        (FontChoice::Inter, true) => FontFamily::Name(crate::theme::PLAN_FAMILY.into()),
        _ => FontFamily::Proportional,
    };
    let strong_family = match (scene.family, inter) {
        (FontChoice::Inter, true) => FontFamily::Name(crate::theme::PLAN_FAMILY_STRONG.into()),
        _ => FontFamily::Proportional,
    };
    let size = (f32::from(scene.font.size16) / 16.0).clamp(9.0, 16.0);
    let text = fill_of(scene, Class::Text).unwrap_or(Color32::BLACK);
    let strong = fill_of(scene, Class::TextStrong).unwrap_or(text);
    let muted = fill_of(scene, Class::TextMuted).unwrap_or(text);
    let line = scene
        .paints
        .get(&Class::GridMajor)
        .and_then(|p| p.stroke.or(p.fill))
        .map_or(Color32::GRAY, color);
    let mut headers = Vec::new();
    let mut x = rect.left();
    for (i, w) in &shown {
        let Some(c) = input.columns.get(*i) else {
            continue;
        };
        let title = c.display_title(texts);
        cell_text(
            &tp,
            Rect::from_min_size(pos2(x, rect.top()), vec2(*w, header_h)),
            &title,
            FontId::new(size, strong_family.clone()),
            strong,
            c.header_align.unwrap_or(CellAlign::Left),
            0.0,
        );
        headers.push(title);
        x += w;
    }
    if left > 0 {
        let more = texts.get_args(
            TextId::FwPreviewMoreColumns,
            &[("count", Arg::Count(i64::try_from(left).unwrap_or(i64::MAX)))],
        );
        // Dans la dernière colonne, à droite de l'en-tête.
        let galley = tp.layout_no_wrap(more.clone(), FontId::new(size - 1.0, family.clone()), muted);
        let at = pos2(
            table_right - galley.size().x - CELL_PAD,
            rect.top() + (header_h - galley.size().y) / 2.0,
        );
        tp.rect_filled(
            Rect::from_min_size(
                pos2(at.x - 3.0, at.y - 1.0),
                vec2(galley.size().x + 6.0, galley.size().y + 2.0),
            ),
            3.0,
            header_bg,
        );
        tp.galley(at, galley, muted);
        headers.push(more);
    }
    let model = rows.model();
    for (r, row) in scene.rows.iter().enumerate() {
        let top = rect.top() + pts(row.y16);
        let h = pts(row.h16);
        let row_rect = Rect::from_min_size(pos2(rect.left(), top), vec2(table_w, h));
        let part = Part::Row(u32::try_from(r).unwrap_or(u32::MAX));
        if let Some(fill) = scene
            .items
            .iter()
            .filter(|it| it.clip == Clip::Table && it.part == part)
            .find_map(|it| match it.class {
                Class::Band(_) | Class::PhaseRow(_) | Class::TaskFill(_) | Class::HeaderBg => {
                    fill_of(scene, it.class)
                }
                _ => None,
            })
        {
            tp.rect_filled(row_rect, 0.0, fill);
        }
        let Some(uid) = row.uid else {
            continue;
        };
        let Some(index) = model.index_of(uid) else {
            continue;
        };
        let model_row = model.row(index);
        let look = input.looks.get(&uid.get()).copied().unwrap_or_default();
        let summary = row.summary;
        let inactive = model_row.is_some_and(|m| !m.active);
        let plain = if inactive {
            muted
        } else if summary {
            strong
        } else {
            text
        };
        let ink = look.color.map_or(plain, color);
        let fam = if summary || look.bold {
            strong_family.clone()
        } else {
            family.clone()
        };
        let mut x = rect.left();
        for (i, w) in &shown {
            let Some(c) = input.columns.get(*i) else {
                continue;
            };
            let value = model.text(index, c.column);
            let indent = if c.column == crate::column::Column::Name {
                INDENT * f32::from(row.level.saturating_sub(1))
            } else {
                0.0
            };
            cell_text(
                &tp,
                Rect::from_min_size(pos2(x, top), vec2(*w, h)),
                value,
                FontId::new(size, fam.clone()),
                ink,
                c.data_align(),
                indent,
            );
            x += w;
        }
    }
    // Séparateur tableau / graphique.
    painter.line_segment(
        [pos2(table_right, rect.top()), pos2(table_right, rect.bottom())],
        Stroke::new(1.0, line),
    );
    painter.rect_stroke(
        rect,
        6.0,
        Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        egui::StrokeKind::Inside,
    );
    // Barres (clic et relevé).
    let origin = pos2(table_right, rect.top());
    let bars: Vec<(BarRole, Rect)> = scene
        .items
        .iter()
        .filter_map(|it| Some((bar_role(it.class)?, prim_rect(&it.prim, origin)?)))
        .filter(|(_, r)| r.intersects(gantt_rect))
        .collect();
    let hit = response
        .clicked()
        .then(|| response.interact_pointer_pos())
        .flatten()
        .map(|p| {
            if p.x < table_right {
                PreviewHit::Table
            } else {
                PreviewHit::Chart(
                    bars.iter()
                        .rev()
                        .find(|(_, r)| r.expand(HIT_SLOP).contains(p))
                        .map(|(role, _)| *role),
                )
            }
        });
    let drawn = PreviewDrawn {
        sheet: rect,
        table_right,
        bars,
        headers,
        builds,
    };
    ui.ctx()
        .data_mut(|d| d.insert_temp::<Option<PreviewDrawn>>(drawn_id(), Some(drawn)));
    hit
}

/// Texte d'une cellule, coupé à sa largeur, aligné.
fn cell_text(
    painter: &egui::Painter,
    cell: Rect,
    text: &str,
    font: FontId,
    ink: Color32,
    align: CellAlign,
    indent: f32,
) {
    if text.is_empty() || cell.width() <= 2.0 * CELL_PAD {
        return;
    }
    let galley = painter.layout_no_wrap(text.to_owned(), font, ink);
    let room = cell.shrink2(vec2(CELL_PAD, 0.0));
    let w = galley.size().x;
    let x = match align {
        CellAlign::Left => room.left() + indent,
        CellAlign::Right => (room.right() - w).max(room.left()),
        CellAlign::Center => (room.center().x - w / 2.0).max(room.left()),
    };
    let y = cell.center().y - galley.size().y / 2.0;
    painter
        .with_clip_rect(room.intersect(painter.clip_rect()))
        .galley(pos2(x, y), galley, ink);
}

/// Aperçu sans ligne à montrer : phrase centrée.
fn empty(ui: &egui::Ui, texts: &Texts, rect: Rect) {
    let painter = ui.painter_at(rect);
    painter.rect_stroke(
        rect,
        6.0,
        Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        texts.get(TextId::FwPreviewEmpty),
        FontId::proportional(13.0),
        ui.visuals().weak_text_color(),
    );
    ui.ctx()
        .data_mut(|d| d.insert_temp::<Option<PreviewDrawn>>(drawn_id(), None));
}
