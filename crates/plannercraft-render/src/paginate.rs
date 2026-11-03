//! Pagination de la scène (ADR-0016 D8, H-UI-48 à H-UI-51, P54).
//!
//! Fonction pure : la [`Scene`] de `layout` est découpée en pages sans
//! jamais être redessinée. Bandes horizontales de lignes (aucune ligne
//! coupée, récapitulative jamais seule en bas de page, parent rappelé
//! « (suite) » en haut de page), tranches verticales de la plage de dates
//! (table répétée sur chaque page en largeur si `repeat_table`), en-têtes de
//! colonnes et échelle répétés sur chaque page, légende sur la dernière bande
//! (ou sur toutes), en-tête et pied de page à trois zones dont les jetons
//! sont remplacés (`BarTextSource::token`, `{page}` et `{pages}` ici).
//! Au plus [`MAX_PAGES`] pages, sinon [`PageError::TooManyPages`].
//!
//! Unités : la scène est en 1/16 px (96 px par pouce), le papier en points
//! (72 par pouce) : 1 pt = 64/3 seizièmes de pixel. L'échelle est en
//! dix-millièmes (10 000 = 100 %). Hypothèses H-UI-48 à H-UI-51 :
//! `docs/features/ui.md` § 1.11 ; détail : `docs/features/render.md` § 5.3.

use std::ops::Range;

use crate::axis::SUB;
use crate::geometry::{BarTextSource, Item, Part, Scene, Token};
use crate::request::Zoom;
use crate::scene::table_width16;
use crate::style::{Fit, PageStyle, Paper, PlanStyle, Slot, Variant};
use crate::timeline::{TIMELINE_HEIGHT16, TimelineInput, TimelineLayout};

/// Plafond du nombre de pages d'un export (règle 10 : entrée bornée).
pub const MAX_PAGES: usize = 1_000;

/// Lignes d'en-tête de page au plus (S77).
pub const HEADER_LINES: usize = 5;

/// Lignes de pied de page au plus (S77).
pub const FOOTER_LINES: usize = 3;

/// Interligne des en-têtes et pieds de page, en points.
pub const SLOT_LINE_PT: u32 = 12;

/// Hauteur de la ligne de titre du cartouche (H-UI-66), en points.
pub const TITLE_LINE_PT: u32 = 26;
/// Écart entre l'en-tête (ou le pied) de page et le planning, en points.
pub const SLOT_GAP_PT: u32 = 6;

/// Échelle pleine (100 %), en dix-millièmes.
pub const FULL_SCALE: u32 = 10_000;

/// Échelle minimale admise (10 %, borne de `Fit::Scale`).
const MIN_SCALE: u32 = 1_000;

/// Échelle maximale admise (400 %).
const MAX_SCALE: u32 = 40_000;

/// Écart entre la chronologie imprimée et l'échelle du planning (8 px,
/// H-UI-82).
pub const TIMELINE_GAP16: i64 = 8 * SUB;

/// Largeur de Gantt minimale d'une page à table répétée (32 px) : en
/// dessous, la table n'est pas répétée (elle mangerait la page).
const MIN_SLICE16: i64 = 32 * SUB;

/// Une page : bande de lignes et tranche d'abscisses de la scène.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "drapeaux indépendants d'une page (table, légende, troncature, chronologie)"
)]
pub struct Page {
    /// Rang en largeur (0 = première colonne de pages).
    pub col: u16,
    /// Rang en hauteur.
    pub row: u16,
    /// Lignes de la scène portées par la page.
    pub rows: Range<usize>,
    /// Tranche d'abscisses (1/16 px) de la scène, fin exclue. Avec
    /// [`Page::table`], la tranche du Gantt seulement (la table est
    /// dessinée à gauche) ; sinon la tranche entière, table comprise pour
    /// la première colonne.
    pub x16: (i64, i64),
    /// Ligne rappelée « (suite) » en haut de page.
    pub continued_parent: Option<usize>,
    /// Zones d'en-tête, jetons remplacés (gauche, centre, droite) ; lignes
    /// séparées par `\n`, au plus [`HEADER_LINES`].
    pub header: [String; 3],
    /// Zones de pied de page, jetons remplacés ; au plus
    /// [`FOOTER_LINES`] lignes.
    pub footer: [String; 3],
    /// Table (ID et noms) répétée à gauche de la tranche.
    pub table: bool,
    /// Légende dessinée en bas de la page (H-UI-47).
    pub legend: bool,
    /// Ligne de troncature dessinée après la dernière ligne.
    pub cut: bool,
    /// Chronologie dessinée en tête de la page (première page seulement,
    /// H-UI-82).
    pub timeline: bool,
}

/// Chronologie imprimée (H-UI-82, H-R16) : disposition sur la largeur
/// imprimable, et ses primitives (`timeline::items`), origine en haut à
/// gauche de la frise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrintTimeline {
    /// Largeur de la frise (largeur imprimable à l'échelle des pages).
    pub width16: i64,
    /// Disposition (H-R16).
    pub layout: TimelineLayout,
    /// Primitives, peintures dans `Pages::scene.paints`.
    pub items: Vec<Item>,
}

/// Chronologie à imprimer : données du moteur et style de ses peintures.
#[derive(Clone, Copy, Debug)]
pub struct TimelineSource<'a> {
    /// Données de la frise (`plannercraft_automation::timeline::input`).
    pub input: &'a TimelineInput,
    /// Style (peintures des jetons existants).
    pub style: &'a PlanStyle,
    /// Variante du style.
    pub variant: Variant,
}

/// Pages d'un export, dans l'ordre de lecture : bande par bande, de gauche
/// à droite dans une bande.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pages {
    /// Pages.
    pub pages: Vec<Page>,
    /// Largeur et hauteur du papier en points (1/72 pouce), orientation
    /// appliquée.
    pub paper_pt: (u32, u32),
    /// Marges haut, droite, bas, gauche, en points.
    pub margins_pt: [u32; 4],
    /// Hauteur de la zone d'en-tête de page (0 : aucun texte), en points.
    pub header_pt: u32,
    /// Hauteur de la zone de pied de page (0 : aucun texte), en points.
    pub footer_pt: u32,
    /// Échelle du planning, en dix-millièmes (10 000 = 100 %).
    pub scale: u32,
    /// Hauteur de la ligne de troncature dans la scène (0 : aucune).
    pub cut16: i64,
    /// Haut de la légende dans la scène et sa hauteur (0 : aucune).
    pub legend16: (i64, i64),
    /// Scène paginée (le PDF la lit page par page).
    pub scene: Scene,
    /// Cartouche (H-UI-66) : première ligne de la zone centrale de
    /// l'en-tête en titre sur [`TITLE_LINE_PT`], première ligne de la zone
    /// droite appuyée.
    pub title_block: bool,
    /// Chronologie en tête de la première page : haut (0, sous l'en-tête
    /// et le cartouche) et hauteur ([`TIMELINE_HEIGHT16`]) dans l'espace de
    /// la scène ; `(0, 0)` sans chronologie. Le planning de la première
    /// page commence [`TIMELINE_GAP16`] plus bas.
    pub timeline16: (i64, i64),
    /// Chronologie imprimée (`page.timeline` et données fournies).
    pub timeline: Option<PrintTimeline>,
}

impl Pages {
    /// Hauteur réservée en tête de la page `index` (chronologie et son
    /// écart), en 1/16 px de scène : non nulle sur la première page
    /// seulement.
    #[must_use]
    pub fn lead16(&self, index: usize) -> i64 {
        let first = self.pages.get(index).is_some_and(|p| p.timeline);
        if first && self.timeline16.1 > 0 {
            self.timeline16.1.saturating_add(TIMELINE_GAP16)
        } else {
            0
        }
    }
}

/// Erreur de pagination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PageError {
    /// Plus de [`MAX_PAGES`] pages nécessaires.
    TooManyPages {
        /// Pages nécessaires (au moins `MAX_PAGES + 1`).
        needed: usize,
    },
    /// Pagination livrée par le lot L7 (variante du contrat, plus rendue).
    NotYetAvailable,
}

impl std::fmt::Display for PageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooManyPages { needed } => {
                write!(f, "{needed} pages nécessaires, {MAX_PAGES} au plus")
            }
            Self::NotYetAvailable => f.write_str("pagination pas encore disponible"),
        }
    }
}

impl std::error::Error for PageError {}

/// Options de la pagination tirées du style, hors de [`PageStyle`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PaginateOptions {
    /// Légende sur chaque page (`LegendStyle::every_page`), sinon sur la
    /// dernière bande seulement.
    pub legend_every_page: bool,
}

/// Points (arrondis) d'une longueur en millimètres : `mm × 72 / 25,4`.
const fn mm_to_pt(mm: u32) -> u32 {
    // 72 / 25,4 = 720 / 254 ; arrondi au plus proche.
    (mm.saturating_mul(720).saturating_add(127)) / 254
}

/// Papier en points, portrait (largeur, hauteur).
#[must_use]
pub const fn paper_portrait_pt(p: Paper) -> (u32, u32) {
    match p {
        // 210 × 297 mm.
        Paper::A4 => (595, 842),
        // 297 × 420 mm.
        Paper::A3 => (842, 1_191),
        // 8,5 × 11 pouces.
        Paper::Letter => (612, 792),
        // 11 × 17 pouces.
        Paper::Tabloid => (792, 1_224),
    }
}

/// Papier en points, orientation de `page` appliquée.
#[must_use]
pub const fn paper_pt(page: &PageStyle) -> (u32, u32) {
    let (w, h) = paper_portrait_pt(page.paper);
    if page.landscape { (h, w) } else { (w, h) }
}

/// Seizièmes de pixel d'une longueur en points à l'échelle `scale` :
/// `pt × 64 / 3 × 10 000 / scale`, arrondi inférieur.
fn pt_to16(pt: u32, scale: u32) -> i64 {
    let num = i128::from(pt)
        .saturating_mul(64)
        .saturating_mul(i128::from(FULL_SCALE));
    let den = 3_i128.saturating_mul(i128::from(scale.max(1)));
    i64::try_from(num.checked_div(den).unwrap_or(0)).unwrap_or(i64::MAX)
}

/// Marges en points (haut, droite, bas, gauche).
fn margins_pt(page: &PageStyle) -> [u32; 4] {
    page.margins_mm.map(|m| mm_to_pt(u32::from(m.min(50))))
}

/// Zone imprimable (largeur, hauteur) en points.
fn printable_pt(page: &PageStyle) -> (u32, u32) {
    let (width, height) = paper_pt(page);
    let [top, right, bottom, left] = margins_pt(page);
    (
        width.saturating_sub(left).saturating_sub(right),
        height.saturating_sub(top).saturating_sub(bottom),
    )
}

/// Échelle de temps qui fait tenir le Gantt dans `PagesWide(n)` (ou
/// `PagesWideTall(n, _)`) à 100 % : `Zoom::Fill` sur la largeur de `n`
/// pages, table répétée déduite (H-UI-50, étiquettes par H-R4 et H-R5) ;
/// le Gantt remplit la largeur (J8 lot L16 : `Zoom::Auto` s'arrêtait à
/// l'échelle discrète inférieure et laissait jusqu'à deux tiers de la page
/// vides).
/// `None` pour une échelle en % (le zoom de l'appelant est gardé).
#[must_use]
pub fn fit_zoom(page: &PageStyle) -> Option<Zoom> {
    fit_zoom_at(page, FULL_SCALE)
}

/// Comme [`fit_zoom`], pour une page imprimée à l'échelle `scale`
/// (dix-millièmes, bornée de 10 % à 100 %) : la largeur de `n` pages
/// vaut `1 / scale` fois plus de scène, le Gantt est élargi d'autant
/// (relecture finale de J8, F2 : « N × M pages » réduit par la hauteur).
#[must_use]
pub fn fit_zoom_at(page: &PageStyle, scale: u32) -> Option<Zoom> {
    let n = match page.fit {
        Fit::Scale(_) => return None,
        Fit::PagesWide(n) | Fit::PagesWideTall(n, _) => i64::from(n.clamp(1, 20)),
    };
    let (w_pt, _) = printable_pt(page);
    let content16 = pt_to16(w_pt, scale.clamp(MIN_SCALE, FULL_SCALE));
    let table16 = table_width16(page.columns.as_deref());
    let gantt16 = if page.repeat_table && content16 >= table16.saturating_add(MIN_SLICE16) {
        content16.saturating_sub(table16).saturating_mul(n)
    } else {
        content16.saturating_mul(n).saturating_sub(table16)
    };
    let width_px = u32::try_from((gantt16 / SUB).max(1)).unwrap_or(u32::MAX);
    Some(Zoom::Fill { width_px })
}

/// Écart d'échelle toléré par la seconde passe de [`layout_pages`] (1 %).
const REFIT_SLACK: u32 = 100;

/// Scène et pages du planning : `layout` puis [`paginate_with`], chemin
/// commun de l'export PDF et de l'aperçu du mode impression.
///
/// « N pages en largeur et M en hauteur » (`Fit::PagesWideTall`) avec un
/// zoom ajusté (`Zoom::Fill`) : quand la hauteur impose une réduction
/// (échelle `s` < 100 %), la scène est recalculée avec le Gantt élargi de
/// `1 / s` ([`fit_zoom_at`]) puis repaginée, pour que la feuille reste
/// pleine en largeur (relecture finale de J8, F2 ; le défaut corrigé par
/// L16 pour `PagesWide`). La seconde passe n'est gardée que si son échelle
/// ne perd pas plus de 1 % (réduction imposée par la largeur : première
/// passe gardée).
///
/// # Errors
///
/// Ceux de [`paginate_with`] (première passe).
pub fn layout_pages(
    project: &plannercraft_core::Project,
    schedule: &plannercraft_core::Schedule,
    st: &crate::style::PlanStyle,
    src: &dyn BarTextSource,
    vp: &crate::geometry::Viewport,
    opts: PaginateOptions,
) -> (Result<Pages, PageError>, crate::request::RenderReport) {
    layout_pages_timeline(project, schedule, st, src, vp, opts, None)
}

/// Comme [`layout_pages`], avec la chronologie `timeline` en tête de la
/// première page si la mise en page la demande (`page.timeline`, H-UI-82 ;
/// `None` : aucune). Chemin commun de l'export PDF et de l'aperçu.
///
/// # Errors
///
/// Ceux de [`paginate_timeline`] (première passe).
pub fn layout_pages_timeline(
    project: &plannercraft_core::Project,
    schedule: &plannercraft_core::Schedule,
    st: &crate::style::PlanStyle,
    src: &dyn BarTextSource,
    vp: &crate::geometry::Viewport,
    opts: PaginateOptions,
    timeline: Option<&TimelineInput>,
) -> (Result<Pages, PageError>, crate::request::RenderReport) {
    let tl = timeline.map(|input| TimelineSource {
        input,
        style: st,
        variant: vp.variant,
    });
    let (scene, report) = crate::geometry::layout(project, schedule, st, src, vp);
    let first = paginate_timeline(&scene, &st.page, src, opts, tl);
    drop(scene);
    let refit = match (&first, st.page.fit, vp.zoom) {
        (Ok(pages), Fit::PagesWideTall(..), Zoom::Fill { .. }) if pages.scale < FULL_SCALE => {
            Some(pages.scale)
        }
        _ => None,
    };
    let Some(scale) = refit else {
        return (first, report);
    };
    let Some(zoom) = fit_zoom_at(&st.page, scale) else {
        return (first, report);
    };
    let wide = crate::geometry::Viewport { zoom, ..vp.clone() };
    let (scene, report2) = crate::geometry::layout(project, schedule, st, src, &wide);
    match paginate_timeline(&scene, &st.page, src, opts, tl) {
        Ok(pages) if pages.scale.saturating_add(REFIT_SLACK) >= scale => (Ok(pages), report2),
        _ => (first, report),
    }
}

/// Hauteur et rangs utiles de la scène.
struct Shape {
    /// Hauteur de la ligne de troncature (0 : aucune).
    cut16: i64,
    /// Haut de la légende.
    legend_top16: i64,
    /// Hauteur de la légende (0 : aucune).
    legend16: i64,
}

fn shape_of(scene: &Scene) -> Shape {
    let rows_end = scene
        .rows
        .last()
        .map_or(scene.header16, |r| r.y16.saturating_add(r.h16));
    let has_cut = scene.items.iter().any(|i| i.part == Part::Cut);
    let row_h = scene.rows.first().map_or(22 * SUB, |r| r.h16);
    let cut16 = if has_cut { row_h } else { 0 };
    let legend_top16 = rows_end.saturating_add(cut16);
    let has_legend = scene.items.iter().any(|i| i.part == Part::Legend);
    let legend16 = if has_legend {
        scene.height16.saturating_sub(legend_top16).max(0)
    } else {
        0
    };
    Shape {
        cut16,
        legend_top16,
        legend16,
    }
}

/// Géométrie horizontale à une échelle : table répétée, largeur d'une
/// tranche, nombre de colonnes de pages.
#[derive(Clone, Copy)]
struct Columns {
    table: bool,
    start16: i64,
    slice16: i64,
    count: usize,
}

fn columns(scene: &Scene, page: &PageStyle, scale: u32) -> Columns {
    let (w_pt, _) = printable_pt(page);
    let content16 = pt_to16(w_pt, scale).max(SUB);
    let table = page.repeat_table && content16 >= scene.table16.saturating_add(MIN_SLICE16);
    let (start16, slice16) = if table {
        (scene.table16, content16.saturating_sub(scene.table16))
    } else {
        (0, content16)
    };
    let len = scene.width16.saturating_sub(start16).max(0);
    let count = if len == 0 {
        1
    } else {
        usize::try_from(
            len.saturating_add(slice16.saturating_sub(1))
                .checked_div(slice16.max(1))
                .unwrap_or(0),
        )
        .unwrap_or(usize::MAX)
        .max(1)
    };
    Columns {
        table,
        start16,
        slice16,
        count,
    }
}

/// Lignes d'un emplacement (`\n`), au plus `max`, jetons remplacés.
fn slot_lines(slot: &Slot, max: usize, tokens: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    slot.0
        .split('\n')
        .take(max)
        .map(|line| replace_tokens(line, tokens))
        .collect()
}

/// Remplace `{jeton}` par sa valeur ; jeton inconnu gardé tel quel.
fn replace_tokens(line: &str, tokens: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(open) = rest.find('{') {
        let (before, after) = rest.split_at(open);
        out.push_str(before);
        let Some(close) = after.find('}') else {
            out.push_str(after);
            return out;
        };
        let name = after.get(1..close).unwrap_or("");
        match tokens(name) {
            Some(v) => out.push_str(&v),
            None => out.push_str(after.get(..=close).unwrap_or("")),
        }
        rest = after.get(close.saturating_add(1)..).unwrap_or("");
    }
    out.push_str(rest);
    out
}

/// Valeur d'un jeton sur une ligne : fins de ligne et contrôles en espace.
fn one_line(s: &str) -> String {
    s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

/// Nombre de lignes d'un trio d'emplacements (le plus haut des trois).
fn lines_of(slots: &[Slot; 3], max: usize) -> u32 {
    let n = slots
        .iter()
        .filter(|s| !s.0.trim().is_empty())
        .map(|s| s.0.split('\n').take(max).count())
        .max()
        .unwrap_or(0);
    u32::try_from(n).unwrap_or(0)
}

/// Bandes de lignes : `(lignes, parent rappelé)`.
type Band = (Range<usize>, Option<usize>);

/// Parent rappelé en haut d'une bande commençant à `start` : la plus proche
/// récapitulative précédente de niveau inférieur (H-UI-48).
fn continued_parent(scene: &Scene, start: usize) -> Option<usize> {
    let first = scene.rows.get(start)?;
    if start == 0 || first.level <= 1 {
        return None;
    }
    let before = scene.rows.get(..start)?;
    before.iter().rposition(|r| r.summary && r.level < first.level)
}

/// Une récapitulative en dernière ligne de `rows` suivie d'une ligne plus
/// profonde (son premier enfant) serait seule en bas de page.
fn orphan_summary(scene: &Scene, end: usize) -> bool {
    let (Some(last), Some(next)) = (
        end.checked_sub(1).and_then(|i| scene.rows.get(i)),
        scene.rows.get(end),
    ) else {
        return false;
    };
    last.summary && next.level > last.level
}

/// Découpe les lignes en bandes de hauteur `cap16` au plus (au moins une
/// ligne par bande), la dernière portant `tail16` en plus (troncature et
/// légende).
/// La première bande porte en plus `first16` en tête (chronologie,
/// H-UI-82).
fn bands(scene: &Scene, cap16: i64, row_extra16: i64, tail16: i64, first16: i64) -> Vec<Band> {
    let n = scene.rows.len();
    let h = |i: usize| scene.rows.get(i).map_or(0, |r| r.h16);
    let row_h = h(0);
    let mut out: Vec<Band> = Vec::new();
    let mut i = 0_usize;
    while i < n {
        let start = i;
        let parent = continued_parent(scene, start);
        let avail = cap16
            .saturating_sub(row_extra16)
            .saturating_sub(if parent.is_some() { row_h } else { 0 })
            .saturating_sub(if start == 0 { first16 } else { 0 });
        let mut used = 0_i64;
        while i < n && (i == start || used.saturating_add(h(i)) <= avail) {
            used = used.saturating_add(h(i));
            i = i.saturating_add(1);
        }
        // H-UI-48 : une récapitulative (ou une suite de récapitulatives
        // emboîtées) ne finit pas une page dont la suite porte ses enfants.
        while i < n && i.saturating_sub(start) > 1 && orphan_summary(scene, i) {
            i = i.saturating_sub(1);
        }
        out.push((start..i, parent));
    }
    if out.is_empty() {
        out.push((0..0, None));
        return out;
    }
    // La dernière bande porte la troncature et la légende : si elles ne
    // tiennent pas, ses dernières lignes passent sur une bande nouvelle.
    let fits = |band: &Band, tail: i64| {
        let band_h: i64 = band.0.clone().map(h).fold(0, i64::saturating_add);
        let parent_h = if band.1.is_some() { row_h } else { 0 };
        let lead = if band.0.start == 0 { first16 } else { 0 };
        band_h
            .saturating_add(parent_h)
            .saturating_add(lead)
            .saturating_add(row_extra16)
            .saturating_add(tail)
            <= cap16
    };
    if let Some(last) = out.last().cloned()
        && !fits(&last, tail16)
        && last.0.len() > 1
    {
        let end = last.0.end;
        let mut split = end.saturating_sub(1);
        // Le reste ne finit pas sur une récapitulative orpheline, tant que
        // la bande nouvelle tient.
        loop {
            let candidate: Band = (split..end, continued_parent(scene, split));
            let can_grow = split.saturating_sub(last.0.start) > 1
                && orphan_summary(scene, split)
                && fits(
                    &(
                        split.saturating_sub(1)..end,
                        continued_parent(scene, split.saturating_sub(1)),
                    ),
                    tail16,
                );
            if !can_grow {
                if let Some(slot) = out.last_mut() {
                    slot.0 = last.0.start..split;
                }
                out.push(candidate);
                break;
            }
            split = split.saturating_sub(1);
        }
    }
    out
}

/// Échelle de `fit` (dix-millièmes), bornée de 10 % à 400 %.
fn scale_for(
    scene: &Scene,
    page: &PageStyle,
    cap: &dyn Fn(u32) -> i64,
    tail16: i64,
    extra16: i64,
    first16: i64,
) -> u32 {
    match page.fit {
        Fit::Scale(pct) => u32::from(pct).saturating_mul(100).clamp(MIN_SCALE, MAX_SCALE),
        Fit::PagesWide(n) => wide_scale(scene, page, usize::from(n.clamp(1, 20))),
        Fit::PagesWideTall(n, m) => {
            let w = wide_scale(scene, page, usize::from(n.clamp(1, 20)));
            let t = tall_scale(scene, cap, usize::from(m.clamp(1, 20)), tail16, extra16, first16);
            w.min(t)
        }
    }
}

/// Plus grande échelle ≤ 100 % qui tient en `n` colonnes de pages (10 % au
/// moins : au-delà, plus de colonnes).
fn wide_scale(scene: &Scene, page: &PageStyle, n: usize) -> u32 {
    if columns(scene, page, FULL_SCALE).count <= n {
        return FULL_SCALE;
    }
    // Recherche dichotomique : le nombre de colonnes décroît avec l'échelle.
    let (mut lo, mut hi) = (MIN_SCALE, FULL_SCALE);
    if columns(scene, page, lo).count > n {
        return MIN_SCALE;
    }
    while hi.saturating_sub(lo) > 1 {
        let mid = lo.saturating_add(hi.saturating_sub(lo) / 2);
        if columns(scene, page, mid).count <= n {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

/// Plus grande échelle ≤ 100 % dont les lignes tiennent en `m` bandes.
fn tall_scale(
    scene: &Scene,
    cap: &dyn Fn(u32) -> i64,
    m: usize,
    tail16: i64,
    extra16: i64,
    first16: i64,
) -> u32 {
    let count = |s: u32| bands(scene, cap(s), extra16, tail16, first16).len();
    if count(FULL_SCALE) <= m {
        return FULL_SCALE;
    }
    let (mut lo, mut hi) = (MIN_SCALE, FULL_SCALE);
    if count(lo) > m {
        return MIN_SCALE;
    }
    while hi.saturating_sub(lo) > 1 {
        let mid = lo.saturating_add(hi.saturating_sub(lo) / 2);
        if count(mid) <= m {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

/// Découpe la scène en pages, légende sur la dernière bande seulement. Voir
/// [`paginate_with`].
///
/// # Errors
///
/// [`PageError::TooManyPages`] au-delà de [`MAX_PAGES`] pages.
pub fn paginate(scene: &Scene, page: &PageStyle, src: &dyn BarTextSource) -> Result<Pages, PageError> {
    paginate_with(scene, page, src, PaginateOptions::default())
}

/// Découpe la scène en pages (H-UI-48 à H-UI-51).
///
/// - échelle : `Fit::Scale(p)` = p % ; `PagesWide(n)` = 100 % ou moins pour
///   tenir en `n` pages en largeur (la plage de dates a déjà été mise à
///   l'échelle par [`fit_zoom`]) ; `PagesWideTall(n, m)` : la plus petite
///   des deux échelles ; jamais sous 10 % ni au-dessus de 400 % ;
/// - colonnes : tranches de la plage de dates, la table (ID, noms)
///   répétée à gauche de chaque page si `repeat_table` et si la page garde
///   au moins 32 px de Gantt ;
/// - bandes : aucune ligne coupée, au moins une ligne par page ; une
///   récapitulative suivie de ses enfants ne finit pas une page ; une page
///   dont la première ligne a un parent hors de la page le rappelle
///   « (suite) » sur une ligne de plus ;
/// - l'échelle (deux niveaux) et les titres de colonnes sont répétés en
///   haut de chaque page ; la ligne de troncature suit la dernière ligne ;
///   la légende est en bas de la dernière bande (de chaque colonne), ou de
///   chaque page si `legend_every_page` ;
/// - en-tête (5 lignes au plus) et pied de page (3 lignes au plus) :
///   jetons `{projet}`, `{date_etat}`, `{date_impression}`, `{fichier}`,
///   `{planning}` (par `src`), `{page}` et `{pages}` (numéros de 1 à N) ; jeton inconnu
///   gardé tel quel.
///
/// # Errors
///
/// [`PageError::TooManyPages`] au-delà de [`MAX_PAGES`] pages (calculé
/// avant de construire les pages).
pub fn paginate_with(
    scene: &Scene,
    page: &PageStyle,
    src: &dyn BarTextSource,
    opts: PaginateOptions,
) -> Result<Pages, PageError> {
    paginate_timeline(scene, page, src, opts, None)
}

/// Comme [`paginate_with`], avec la chronologie `timeline` en tête de la
/// **première page seulement** si `page.timeline` (H-UI-82, ADR-0017 D8) :
/// sous l'en-tête et le cartouche, sur toute la largeur imprimable à
/// l'échelle des pages, hauteur [`TIMELINE_HEIGHT16`] suivie de
/// [`TIMELINE_GAP16`] ; les lignes de la première bande diminuent d'autant
/// (P54 tient : au moins une ligne par bande). Disposition par
/// `timeline::layout` (H-R16), primitives par `timeline::items`, peintures
/// ajoutées à celles de la scène. Sans `page.timeline` ou sans données :
/// identique à [`paginate_with`].
///
/// # Errors
///
/// [`PageError::TooManyPages`] au-delà de [`MAX_PAGES`] pages.
#[allow(
    clippy::too_many_lines,
    reason = "pagination d'un bloc : échelle, colonnes, bandes, pages, chronologie"
)]
pub fn paginate_timeline(
    scene: &Scene,
    page: &PageStyle,
    src: &dyn BarTextSource,
    opts: PaginateOptions,
    timeline: Option<TimelineSource<'_>>,
) -> Result<Pages, PageError> {
    let timeline = timeline.filter(|_| page.timeline);
    let first16 = if timeline.is_some() {
        TIMELINE_HEIGHT16.saturating_add(TIMELINE_GAP16)
    } else {
        0
    };
    let paper = paper_pt(page);
    let margins = margins_pt(page);
    let (_, h_pt) = printable_pt(page);
    let header_lines = lines_of(&page.header, HEADER_LINES);
    let footer_lines = lines_of(&page.footer, FOOTER_LINES);
    let title = page.title_block && !page.header[1].0.trim().is_empty();
    let header_pt = header_lines
        .saturating_mul(SLOT_LINE_PT)
        .saturating_add(if title {
            TITLE_LINE_PT.saturating_sub(SLOT_LINE_PT)
        } else {
            0
        });
    let footer_pt = footer_lines.saturating_mul(SLOT_LINE_PT);
    let gaps = if header_pt > 0 { SLOT_GAP_PT } else { 0 }.saturating_add(if footer_pt > 0 {
        SLOT_GAP_PT
    } else {
        0
    });
    let body_pt = h_pt
        .saturating_sub(header_pt)
        .saturating_sub(footer_pt)
        .saturating_sub(gaps);
    let shape = shape_of(scene);
    // Capacité en lignes d'une page à l'échelle `s` : corps moins l'échelle
    // répétée.
    let header16 = scene.header16;
    let cap = move |s: u32| pt_to16(body_pt, s).saturating_sub(header16).max(0);
    let (extra16, tail16) = if opts.legend_every_page {
        (shape.legend16, shape.cut16)
    } else {
        (0, shape.cut16.saturating_add(shape.legend16))
    };
    let scale = scale_for(scene, page, &cap, tail16, extra16, first16);
    let cols = columns(scene, page, scale);
    let row_bands = bands(scene, cap(scale), extra16, tail16, first16);
    let needed = row_bands.len().saturating_mul(cols.count);
    if needed > MAX_PAGES {
        return Err(PageError::TooManyPages { needed });
    }
    let total = needed.to_string();
    let fixed = |name: &str| -> Option<String> {
        let t = match name {
            "projet" => Token::Project,
            "date_etat" => Token::StatusDate,
            "date_impression" => Token::PrintDate,
            "fichier" => Token::File,
            "planning" => Token::ScheduleTitle,
            _ => return None,
        };
        Some(one_line(&src.token(t)))
    };
    let mut pages = Vec::with_capacity(needed);
    let last_band = row_bands.len().saturating_sub(1);
    for (b, (rows, parent)) in row_bands.into_iter().enumerate() {
        let is_last = b == last_band;
        for c in 0..cols.count {
            let number = pages.len().saturating_add(1).to_string();
            let tokens = |name: &str| -> Option<String> {
                match name {
                    "page" => Some(number.clone()),
                    "pages" => Some(total.clone()),
                    other => fixed(other),
                }
            };
            let slots = |s: &[Slot; 3], max: usize| -> [String; 3] {
                [0, 1, 2].map(|k| {
                    s.get(k)
                        .map_or_else(String::new, |slot| slot_lines(slot, max, &tokens).join("\n"))
                })
            };
            let off = i64::try_from(c).unwrap_or(i64::MAX).saturating_mul(cols.slice16);
            let x0 = cols.start16.saturating_add(off);
            let x1 = x0.saturating_add(cols.slice16).min(scene.width16.max(x0));
            pages.push(Page {
                col: u16::try_from(c).unwrap_or(u16::MAX),
                row: u16::try_from(b).unwrap_or(u16::MAX),
                rows: rows.clone(),
                x16: (x0, x1),
                continued_parent: parent,
                header: slots(&page.header, HEADER_LINES),
                footer: slots(&page.footer, FOOTER_LINES),
                table: cols.table,
                legend: shape.legend16 > 0 && (opts.legend_every_page || is_last),
                cut: shape.cut16 > 0 && is_last,
                timeline: timeline.is_some() && pages.is_empty(),
            });
        }
    }
    let mut scene = scene.clone();
    let print = timeline.map(|tl| {
        let (w_pt, _) = printable_pt(page);
        let width16 = pt_to16(w_pt, scale).max(SUB);
        let layout = crate::timeline::layout(tl.input, width16);
        let items = crate::timeline::items(&layout);
        for (class, paint) in crate::timeline::paints(tl.style, tl.variant) {
            scene.paints.entry(class).or_insert(paint);
        }
        PrintTimeline {
            width16,
            layout,
            items,
        }
    });
    Ok(Pages {
        pages,
        paper_pt: paper,
        margins_pt: margins,
        header_pt,
        footer_pt,
        scale,
        cut16: shape.cut16,
        legend16: (shape.legend_top16, shape.legend16),
        scene,
        title_block: title,
        timeline16: if print.is_some() {
            (0, TIMELINE_HEIGHT16)
        } else {
            (0, 0)
        },
        timeline: print,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests")]
mod tests {
    use super::*;

    #[test]
    fn tokens_replaced_unknown_kept() {
        let t = |n: &str| (n == "page").then(|| "2".to_owned());
        assert_eq!(replace_tokens("Page {page}/{x} {", &t), "Page 2/{x} {");
        assert_eq!(replace_tokens("{page}{page}", &t), "22");
    }

    #[test]
    fn paper_sizes_and_margins() {
        assert_eq!(mm_to_pt(10), 28);
        assert_eq!(mm_to_pt(210), 595);
        assert_eq!(pt_to16(3, FULL_SCALE), 64);
        assert_eq!(pt_to16(3, 20_000), 32);
    }
}
