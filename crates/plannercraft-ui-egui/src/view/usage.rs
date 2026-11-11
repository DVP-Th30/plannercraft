//! Utilisation des ressources (J7, lot L11 ; `ViewKind::ResourceUsage`,
//! menu Affichage ; brief § 8 « utilisation des ressources (avec
//! surutilisations en évidence) »).
//!
//! - Volet gauche : grille ([`crate::grid::Sheet`]) Nom, Travail ; une ligne
//!   par ressource (UID 0 cachée exclue), dépliable en ses affectations
//!   (`ViewTransient::expanded`, repliées par défaut ; clic sur « + » ou
//!   touches + et -). Navigation au clavier de L1 (H-UI-13 à H-UI-17), sans
//!   ligne vide (la vue ne crée rien).
//! - Volet droit : une colonne par graduation fine de l'échelle de temps,
//!   sur le même `TimeAxis` et le même défilement horizontal que le Gantt
//!   (H-UI-25) ; valeurs du moteur par blocs de 64 périodes pour les seules
//!   ressources visibles ([`crate::timephased`], R-188, R-245, R-246).
//! - Ressource surutilisée : nom, travail et cellules des périodes
//!   surutilisées en rouge (H-UI-23, R-186). Refus du moteur
//!   (`usage.limit`) : message dans la vue, jamais de panique.
//!
//! Aucun calcul ici (règle 3) : travail total (`ResourceLoad::work`,
//! `AssignmentSchedule::work`), travail par période et surutilisation
//! viennent du moteur ; l'interface ne fait qu'écrire les heures
//! (`ConversionFactors::to_unit_value`, deux décimales, R-009).

use std::collections::{BTreeMap, BTreeSet};

use egui::scroll_area::ScrollBarVisibility;
use egui::{Id, Key, Modifiers, Pos2, Rect, Sense, Stroke, pos2, vec2};
use plannercraft_automation::text::{self, Language as TextLanguage};
use plannercraft_automation::{Batch, Session};
use plannercraft_core::resource::usage::UsageError;
use plannercraft_core::{
    AssignmentUid, ConversionFactors, DurationUnit, Moment, ResourceKind, ResourceUid, TaskUid, TimeBasis,
    Work,
};

use super::main_view::{HBarSpec, hbar, paint_corners, paint_splitter};
use super::table::{NAV_FOCUS_FILTER, focus_elsewhere, table_id};
use super::{FrameCtx, OVERSCAN, Pane, ViewTransient, timescale};
use crate::action::Action;
use crate::app::OpenDocument;
use crate::column::CellAlign;
use crate::geom::{self, index_to_f32};
use crate::grid::{self, CellStyle, HeaderPlace, RowPlace, Sheet, cell_font};
use crate::i18n::{Arg, Language, TextId, Texts};
use crate::nav::{self, CellCursor, GridPos, GridShape, NavMove, RowRef};
use crate::theme::Palette;
use crate::timephased::{self, UsageCache, UsageCacheKey};
use crate::timescale::TimeAxis;

/// Lignes demandées au moteur par fenêtre : la clé du cache ne change qu'au
/// passage d'une fenêtre alignée de 32 lignes à la suivante.
pub const KEY_WINDOW: usize = 32;

/// Ligne de la vue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum UsageKey {
    /// Ressource.
    Resource(ResourceUid),
    /// Affectation d'une ressource dépliée.
    Assignment(AssignmentUid),
}

/// Colonne du volet gauche.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum UsageColumn {
    /// Déplier / replier (« + » / « - »).
    Expand,
    /// Nom de la ressource, ou de la tâche d'une affectation.
    #[default]
    Name,
    /// Travail total (moteur).
    Work,
}

impl UsageColumn {
    /// Titre de la colonne.
    #[must_use]
    pub const fn title(self) -> TextId {
        match self {
            Self::Expand => TextId::UsageColExpand,
            Self::Name => TextId::UsageColName,
            Self::Work => TextId::UsageColWork,
        }
    }

    /// Rang de la colonne dans [`COLUMNS`].
    const fn ordinal(self) -> usize {
        match self {
            Self::Expand => 0,
            Self::Name => 1,
            Self::Work => 2,
        }
    }
}

/// Colonnes du volet gauche et leurs largeurs (hypothèse : fixes en J7).
pub const COLUMNS: [(UsageColumn, u16); 3] = [
    (UsageColumn::Expand, 24),
    (UsageColumn::Name, 200),
    (UsageColumn::Work, 90),
];

/// Marque d'une ressource repliée (cliquer déplie).
pub const EXPAND_MARK: &str = "+";

/// Marque d'une ressource dépliée (cliquer replie).
pub const COLLAPSE_MARK: &str = "-";

/// Ligne du volet gauche.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageLine {
    /// Clé de la ligne.
    pub key: UsageKey,
    /// Ressource de la ligne (la sienne, ou celle de l'affectation).
    pub resource: ResourceUid,
    /// ID affiché de la ressource.
    pub id: u64,
    /// Ressource à affectations comptées (dépliable).
    pub expandable: bool,
    /// Ressource dépliée.
    pub expanded: bool,
    /// Ressource surutilisée (R-186) ; faux pour une affectation.
    pub overallocated: bool,
    /// Étiquette d'unité d'une ressource matérielle (quantités).
    pub material: Option<String>,
}

/// Lignes et textes du volet gauche, construits une fois par état du
/// document, langue et ensemble déplié.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UsageRows {
    /// Clé de construction.
    key: Option<((u64, u64), Language, Vec<ResourceUid>)>,
    /// Lignes, dans l'ordre d'affichage.
    lines: Vec<UsageLine>,
    /// Textes, [`COLUMNS`] par ligne.
    texts: Vec<String>,
    /// Indice de chaque ligne.
    index: BTreeMap<UsageKey, usize>,
    /// Suffixe des heures (« h »), séparateur décimal, texte d'une valeur
    /// non écrite.
    labels: WorkLabels,
}

/// Mots de l'écriture du travail.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkLabels {
    /// Suffixe des heures (FR « \u{a0}h », EN « h »).
    pub hours: String,
    /// Séparateur décimal.
    pub decimal_sep: char,
    /// Valeur qui ne peut pas être écrite.
    pub unknown: String,
}

impl WorkLabels {
    /// Mots de la langue de `texts`.
    #[must_use]
    pub fn new(texts: &Texts) -> Self {
        let labels = texts.format_labels();
        Self {
            hours: texts.get(TextId::UsageHoursSuffix),
            decimal_sep: labels.decimal_sep,
            unknown: labels.unknown.clone(),
        }
    }
}

/// Travail écrit en heures (« 12 h », « 7,5 h », deux décimales au plus,
/// R-009) ; quantité d'une ressource matérielle suivie de son étiquette
/// (R-185 : la quantité est le travail en heures) ; vide pour 0.
#[must_use]
pub fn work_text(work: Work, material: Option<&str>, f: &ConversionFactors, l: &WorkLabels) -> String {
    let mut out = String::new();
    if work == Work::ZERO {
        return out;
    }
    match f.to_unit_value(work.minutes(), DurationUnit::Hour, TimeBasis::Working) {
        Ok(value) => {
            text::write_decimal(&mut out, value, l.decimal_sep);
            match material {
                Some(label) if !label.is_empty() => {
                    out.push(' ');
                    out.push_str(label);
                }
                Some(_) => {}
                None => out.push_str(&l.hours),
            }
        }
        Err(_) => out.push_str(&l.unknown),
    }
    out
}

impl UsageRows {
    /// Construit les lignes d'un document ; les ressources de `expanded`
    /// montrent leurs affectations, triées par (tâche, UID) comme
    /// `usage_by_assignment` (R-245).
    #[must_use]
    pub fn build(doc: &OpenDocument, texts: &Texts, expanded: &BTreeSet<ResourceUid>) -> Self {
        let project = doc.project();
        let schedule = doc.schedule();
        let factors = &project.options.factors;
        let labels = WorkLabels::new(texts);
        let hidden = ResourceUid::new(0);
        // Affectations comptées par ressource (celles que `usage` compte :
        // ressource connue, jamais fictive), triées par (tâche, UID).
        let mut by_resource: BTreeMap<ResourceUid, Vec<(TaskUid, AssignmentUid, Work)>> = BTreeMap::new();
        for (&uid, a) in &schedule.assignments {
            if let Some(r) = a.resource {
                by_resource.entry(r).or_default().push((a.task, uid, a.work));
            }
        }
        for list in by_resource.values_mut() {
            list.sort_unstable_by_key(|&(task, uid, _)| (task, uid));
        }
        let mut out = Self {
            key: Some((
                doc.cache_key(),
                texts.language(),
                expanded.iter().copied().collect(),
            )),
            labels,
            ..Self::default()
        };
        for (position, resource) in project.resources.iter().enumerate() {
            let uid = resource.uid();
            if uid == hidden {
                continue;
            }
            let id = plannercraft_core::resource_id(&project.resources, position);
            let load = schedule.resources.get(&uid);
            let cost = resource.kind == ResourceKind::Cost;
            let material = match &resource.kind {
                ResourceKind::Material { label } => Some(label.clone()),
                _ => None,
            };
            let assignments = by_resource.get(&uid).filter(|_| !cost);
            let expandable = assignments.is_some_and(|list| !list.is_empty());
            let is_open = expandable && expanded.contains(&uid);
            let work = if cost {
                String::new()
            } else {
                work_text(
                    load.map_or(Work::ZERO, |l| l.work),
                    material.as_deref(),
                    factors,
                    &out.labels,
                )
            };
            let mark = match (expandable, is_open) {
                (false, _) => String::new(),
                (true, false) => EXPAND_MARK.to_owned(),
                (true, true) => COLLAPSE_MARK.to_owned(),
            };
            out.push(
                UsageLine {
                    key: UsageKey::Resource(uid),
                    resource: uid,
                    id,
                    expandable,
                    expanded: is_open,
                    overallocated: load.is_some_and(|l| l.overallocated),
                    material: material.clone(),
                },
                [mark, resource.name.clone(), work],
            );
            if !is_open {
                continue;
            }
            for &(task, a, w) in assignments.into_iter().flatten() {
                let name = project
                    .tasks
                    .get(task)
                    .map(|t| t.name.clone())
                    .unwrap_or_default();
                let work = work_text(w, material.as_deref(), factors, &out.labels);
                out.push(
                    UsageLine {
                        key: UsageKey::Assignment(a),
                        resource: uid,
                        id,
                        expandable: false,
                        expanded: false,
                        overallocated: false,
                        material: material.clone(),
                    },
                    [String::new(), name, work],
                );
            }
        }
        out
    }

    /// Ajoute une ligne et ses textes.
    fn push(&mut self, line: UsageLine, texts: [String; 3]) {
        self.index.insert(line.key, self.lines.len());
        self.lines.push(line);
        self.texts.extend(texts);
    }

    /// Vrai si les lignes ont été construites pour cet état du document,
    /// cette langue et cet ensemble déplié.
    #[must_use]
    pub fn is_current(
        &self,
        doc: &OpenDocument,
        language: Language,
        expanded: &BTreeSet<ResourceUid>,
    ) -> bool {
        self.key.as_ref().is_some_and(|(key, lang, open)| {
            *key == doc.cache_key() && *lang == language && open.iter().copied().eq(expanded.iter().copied())
        })
    }

    /// Nombre de lignes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Aucune ligne.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Ligne d'indice `i`.
    #[must_use]
    pub fn line(&self, i: usize) -> Option<&UsageLine> {
        self.lines.get(i)
    }

    /// Indice d'une ligne.
    #[must_use]
    pub fn index_of(&self, key: UsageKey) -> Option<usize> {
        self.index.get(&key).copied()
    }

    /// Texte d'une cellule ; vide hors bornes.
    #[must_use]
    pub fn text(&self, i: usize, c: UsageColumn) -> &str {
        i.checked_mul(COLUMNS.len())
            .and_then(|base| base.checked_add(c.ordinal()))
            .and_then(|k| self.texts.get(k))
            .map_or("", String::as_str)
    }

    /// Mots de l'écriture du travail.
    #[must_use]
    pub const fn labels(&self) -> &WorkLabels {
        &self.labels
    }

    /// Ressources des lignes `range`, sans doublon, dans l'ordre des lignes ;
    /// et, parmi elles, les dépliées : ligne de la ressource dépliée, **ou**
    /// ligne d'une de ses affectations (une fenêtre qui commence au milieu
    /// des affectations ne voit pas la ligne de la ressource : relecture
    /// finale J7, F2 de la relecture des performances, cellules vides).
    #[must_use]
    pub fn resources_in(&self, range: std::ops::Range<usize>) -> (Vec<ResourceUid>, Vec<ResourceUid>) {
        let mut seen = BTreeSet::new();
        let mut open = BTreeSet::new();
        let mut resources = Vec::new();
        let mut expanded = Vec::new();
        for line in self.lines.get(range).unwrap_or_default() {
            if seen.insert(line.resource) {
                resources.push(line.resource);
            }
            let unfolded = line.expanded || matches!(line.key, UsageKey::Assignment(_));
            if unfolded && open.insert(line.resource) {
                expanded.push(line.resource);
            }
        }
        (resources, expanded)
    }
}

/// Le volet gauche vu comme une grille ([`Sheet`]) : lecture seule.
pub struct UsageSheet<'a> {
    /// Lignes et textes.
    rows: &'a UsageRows,
    /// Message d'une saisie refusée.
    read_only: String,
}

impl<'a> UsageSheet<'a> {
    /// Grille des lignes `rows`.
    #[must_use]
    pub fn new(rows: &'a UsageRows, texts: &Texts) -> Self {
        Self {
            rows,
            read_only: texts.get(TextId::EditErrorNotEditable),
        }
    }
}

impl Sheet for UsageSheet<'_> {
    type Key = UsageKey;
    type Col = UsageColumn;

    fn len(&self) -> usize {
        self.rows.len()
    }

    fn key(&self, row: usize) -> Option<UsageKey> {
        self.rows.line(row).map(|line| line.key)
    }

    fn index_of(&self, key: UsageKey) -> Option<usize> {
        self.rows.index_of(key)
    }

    fn columns(&self) -> &[(UsageColumn, u16)] {
        &COLUMNS
    }

    fn title(&self, col: UsageColumn) -> TextId {
        col.title()
    }

    fn align(&self, col: UsageColumn) -> CellAlign {
        match col {
            UsageColumn::Expand => CellAlign::Center,
            UsageColumn::Name => CellAlign::Left,
            UsageColumn::Work => CellAlign::Right,
        }
    }

    /// Ressource surutilisée : nom et travail en rouge (H-UI-23) ;
    /// affectation en retrait.
    fn style(&self, row: usize, col: UsageColumn, palette: &Palette) -> CellStyle {
        let Some(line) = self.rows.line(row) else {
            return CellStyle::plain(palette.text);
        };
        let red = line.overallocated && col != UsageColumn::Expand;
        CellStyle {
            color: if red { palette.critical } else { palette.text },
            faux_bold: false,
            strike: false,
            italic: false,
            level: match (line.key, col) {
                (UsageKey::Assignment(_), UsageColumn::Name) => 2,
                _ => 0,
            },
        }
    }

    fn text(&self, row: usize, col: UsageColumn) -> &str {
        self.rows.text(row, col)
    }

    fn editable(&self, _: UsageColumn) -> bool {
        false
    }

    fn commit(
        &self,
        _: RowRef<UsageKey>,
        _: UsageColumn,
        _: &str,
        _: &Session,
        _: TextLanguage,
    ) -> Result<Batch, String> {
        Err(self.read_only.clone())
    }
}

/// Cellule du volet droit dessinée à la dernière image (tests, mesure).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaintedCell {
    /// Ligne.
    pub line: usize,
    /// Clé de la ligne.
    pub key: UsageKey,
    /// Bornes de la période (moteur).
    pub start: Moment,
    /// Fin exclue de la période.
    pub finish: Moment,
    /// Travail de la période (moteur).
    pub work: Work,
    /// Période surutilisée (ressource seulement).
    pub over: bool,
}

/// État transitoire de la vue (jamais sérialisé).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UsageState {
    /// Cellule courante du volet gauche (`Blank` : aucune ligne).
    pub cursor: CellCursor<UsageKey, UsageColumn>,
    /// Lignes et textes.
    pub rows: UsageRows,
    /// Blocs de périodes du moteur.
    pub cache: UsageCache,
    /// Défilement vertical à imposer une fois.
    pub pending_scroll_y: Option<f32>,
    /// Défilement vertical à l'image précédente.
    pub last_scroll_y: f32,
    /// Hauteur visible du corps à l'image précédente.
    pub last_body_height: f32,
    /// Cellules du volet droit de la dernière image.
    pub last_cells: Vec<PaintedCell>,
    /// Abscisses (gauche, droite) des colonnes de périodes de la dernière
    /// image, dans l'ordre.
    pub last_columns: Vec<(f32, f32)>,
    /// Refus affiché à la dernière image.
    pub last_message: Option<String>,
}

impl UsageState {
    /// Reconstruit les lignes si nécessaire ; un curseur sur une ligne
    /// disparue revient sur la ligne de sa ressource (ou la première).
    pub fn ensure_rows(&mut self, doc: &OpenDocument, texts: &Texts, expanded: &BTreeSet<ResourceUid>) {
        if !self.rows.is_current(doc, texts.language(), expanded) {
            let resource = match self.cursor.row {
                RowRef::Item(key) => self
                    .rows
                    .index_of(key)
                    .and_then(|i| self.rows.line(i))
                    .map(|l| l.resource),
                RowRef::Blank => None,
            };
            self.rows = UsageRows::build(doc, texts, expanded);
            if let RowRef::Item(key) = self.cursor.row
                && self.rows.index_of(key).is_none()
            {
                self.cursor.row = resource
                    .map(UsageKey::Resource)
                    .filter(|k| self.rows.index_of(*k).is_some())
                    .map_or(RowRef::Blank, RowRef::Item);
            }
        }
        if self.cursor.row == RowRef::Blank
            && let Some(line) = self.rows.line(0)
        {
            self.cursor.row = RowRef::Item(line.key);
        }
    }

    /// Indice de la ligne du curseur.
    #[must_use]
    pub fn cursor_index(&self) -> Option<usize> {
        self.cursor.row.item().and_then(|key| self.rows.index_of(key))
    }

    /// Déplacement au clavier (H-UI-13 à H-UI-15, sans ligne vide) ; le
    /// défilement suit le curseur avec une ligne de marge (H-UI-17).
    pub fn navigate(
        &mut self,
        doc: &OpenDocument,
        texts: &Texts,
        expanded: &BTreeSet<ResourceUid>,
        mv: NavMove,
        row_h: f32,
    ) {
        self.ensure_rows(doc, texts, expanded);
        let len = self.rows.len();
        if len == 0 {
            return;
        }
        let row = self.cursor_index().unwrap_or(0);
        let col = COLUMNS
            .iter()
            .position(|(c, _)| *c == self.cursor.column)
            .unwrap_or(0);
        let shape = GridShape {
            rows: len,
            cols: COLUMNS.len(),
            page: nav::page_rows(self.last_body_height, row_h),
        };
        let target = nav::step(GridPos { row, col }, mv, shape);
        if let Some(line) = self.rows.line(target.row) {
            self.cursor.row = RowRef::Item(line.key);
        }
        if let Some((column, _)) = COLUMNS.get(target.col) {
            self.cursor.column = *column;
        }
        self.cursor.anchor = None;
        if let Some(y) =
            nav::ensure_visible(self.last_scroll_y, target.row, row_h, self.last_body_height, len)
        {
            self.pending_scroll_y = Some(y);
            self.last_scroll_y = y;
        }
    }
}

/// Déplie ou replie la ressource de la ligne `i` (`open` : `None` bascule).
fn set_expanded(rows: &UsageRows, expanded: &mut BTreeSet<ResourceUid>, i: usize, open: Option<bool>) {
    let Some(line) = rows.line(i) else {
        return;
    };
    let uid = line.resource;
    let resource_line = rows.index_of(UsageKey::Resource(uid)).and_then(|j| rows.line(j));
    if !resource_line.is_some_and(|l| l.expandable) {
        return;
    }
    let open = open.unwrap_or(!expanded.contains(&uid));
    if open {
        expanded.insert(uid);
    } else {
        expanded.remove(&uid);
    }
}

/// Défilement horizontal borné à `[0, max]` (0 si non fini).
fn clamp_scroll(scroll: f32, max: f32) -> f32 {
    if scroll.is_finite() {
        scroll.clamp(0.0, max)
    } else {
        0.0
    }
}

/// Dessine la vue dans la place restante de `ui`.
pub fn show(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, view: &mut ViewTransient) {
    let avail = ui.available_rect_before_wrap();
    let Some(doc) = fc.doc else {
        ui.advance_cursor_after_rect(avail);
        return;
    };
    let metrics = *fc.metrics;
    let layout = geom::main_layout(avail, f32::from(fc.state.table_width), &metrics);
    let gantt_w = layout.gantt_clip.width();
    view.last_gantt_width = gantt_w;
    view.last_gantt_left = layout.gantt_clip.left();
    let content_w = TimeAxis::new(doc.range, fc.state.zoom, 0.0).content_width();
    let max_scroll = (content_w - gantt_w).max(0.0);
    let max_scroll = if max_scroll.is_finite() { max_scroll } else { 0.0 };
    let scroll_x = clamp_scroll(view.gantt_scroll_x, max_scroll);
    view.gantt_scroll_x = scroll_x;
    let axis = TimeAxis::new(doc.range, fc.state.zoom, layout.gantt_clip.left() - scroll_x);

    let ViewTransient { usage, expanded, .. } = view;
    usage.ensure_rows(doc, fc.texts, expanded);
    expand_from_keys(ui, usage, expanded);
    usage.ensure_rows(doc, fc.texts, expanded);

    paint_left_header(ui, fc, layout.table_header);
    timescale::paint_header(ui, fc, &axis, layout.gantt_header);
    paint_corners(ui, fc, &layout, avail);
    paint_body(ui, fc, doc, usage, expanded, &layout, &axis);

    // Molette horizontale (ou Maj + molette) au-dessus du volet droit.
    let mut delta_x = 0.0;
    if ui.rect_contains_pointer(layout.gantt_clip.union(layout.gantt_header)) {
        let wheel_x = ui.input(|i| i.smooth_scroll_delta.x);
        if wheel_x.is_finite() && wheel_x.abs() > f32::EPSILON {
            delta_x -= wheel_x;
            ui.input_mut(|i| i.smooth_scroll_delta.x = 0.0);
        }
    }
    paint_splitter(ui, fc, &layout, avail);
    delta_x += hbar(
        ui,
        fc,
        layout.gantt_hbar,
        &HBarSpec {
            id: Id::unique("plannercraft.usage-hbar"),
            label: TextId::GanttScrollLabel,
            scroll: scroll_x,
            content_w,
            visible_w: gantt_w,
        },
    );
    let target = clamp_scroll(scroll_x + delta_x, max_scroll);
    let dx = target - scroll_x;
    if dx.abs() > f32::EPSILON {
        fc.actions.push(Action::ScrollGanttX(dx));
    }
    show_message(ui, fc, usage, layout.gantt_clip);
    ui.advance_cursor_after_rect(avail);
}

/// Touches + (ou =) et - sur la ligne courante, la vue ayant le focus et
/// aucune fenêtre ni menu ouverts : déplie, replie (hypothèse : MS Project
/// déplie par l'indicateur du plan).
fn expand_from_keys(ui: &egui::Ui, usage: &UsageState, expanded: &mut BTreeSet<ResourceUid>) {
    let ctx = ui.ctx();
    if focus_elsewhere(ctx) || ctx.any_popup_open() || !ctx.memory(|m| m.has_focus(table_id())) {
        return;
    }
    let Some(i) = usage.cursor_index() else {
        return;
    };
    let (open, close) = ctx.input_mut(|input| {
        let open =
            input.consume_key(Modifiers::NONE, Key::Plus) || input.consume_key(Modifiers::NONE, Key::Equals);
        let close = input.consume_key(Modifiers::NONE, Key::Minus);
        (open, close)
    });
    if open {
        set_expanded(&usage.rows, expanded, i, Some(true));
    } else if close {
        set_expanded(&usage.rows, expanded, i, Some(false));
    }
}

/// En-tête du volet gauche : fond, titres, séparateurs, trait bas.
fn paint_left_header(ui: &egui::Ui, fc: &mut FrameCtx<'_>, rect: Rect) {
    if !rect.is_finite() || !rect.is_positive() {
        return;
    }
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    let palette = fc.palette;
    let texts = fc.texts;
    painter.rect_filled(rect, 0.0, palette.header_bg);
    let (_, titles) = grid::paint_titles(
        &painter,
        &COLUMNS,
        |column: UsageColumn| texts.get(column.title()),
        palette,
        HeaderPlace {
            rect,
            left: rect.left(),
            pad: fc.metrics.pad_x,
        },
        &grid::title_font(ui.style()),
    );
    painter.hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0, palette.grid_major),
    );
    fc.stats.shapes = fc.stats.shapes.saturating_add(titles.saturating_add(2));
}

/// Corps : `ScrollArea` verticale unique pour les deux volets, lignes
/// visibles seulement.
fn paint_body(
    ui: &mut egui::Ui,
    fc: &mut FrameCtx<'_>,
    doc: &OpenDocument,
    usage: &mut UsageState,
    expanded: &mut BTreeSet<ResourceUid>,
    layout: &geom::MainLayout,
    axis: &TimeAxis,
) {
    let row_h = fc.metrics.row_h;
    let len = usage.rows.len();
    let content_h = index_to_f32(len) * row_h;
    let content_h = if content_h.is_finite() {
        content_h.max(0.0)
    } else {
        0.0
    };
    let mut area = egui::ScrollArea::vertical()
        .id_salt("plannercraft.usage")
        .auto_shrink([false, false])
        .content_margin(egui::Margin::ZERO)
        .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible);
    if let Some(y) = usage.pending_scroll_y.take().filter(|y| y.is_finite()) {
        let max = (content_h - layout.body.height()).max(0.0);
        area = area.vertical_scroll_offset(y.clamp(0.0, if max.is_finite() { max } else { 0.0 }));
    }
    let mut body_ui = ui.new_child(egui::UiBuilder::new().max_rect(layout.body));
    body_ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
    let (left, right) = (layout.table_clip, layout.gantt_clip);
    let (scroll_y, view_h) = area
        .show_viewport(&mut body_ui, |ui, viewport| {
            let origin = ui.max_rect().min;
            ui.set_min_size(vec2(ui.available_width().max(0.0), content_h));
            let range = geom::visible_rows(viewport.top(), viewport.bottom(), row_h, len, OVERSCAN);
            focus_widget(ui, fc, left.union(right));
            paint_rows(ui, fc, usage, expanded, origin, range.clone(), left, right);
            paint_periods(ui, fc, doc, usage, origin, range, axis, right);
            (viewport.top(), viewport.height())
        })
        .inner;
    usage.last_scroll_y = scroll_y;
    usage.last_body_height = view_h;
}

/// Widget focalisable de la vue (identifiant de la table : la navigation
/// au clavier du contexte `Table` s'y applique).
fn focus_widget(ui: &egui::Ui, fc: &FrameCtx<'_>, zone: Rect) {
    let clip = zone.intersect(ui.clip_rect());
    if !clip.is_finite() || !clip.is_positive() {
        return;
    }
    let response = ui.interact(clip, table_id(), Sense::focusable_noninteractive());
    let texts = fc.texts;
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Table, true, texts.get(TextId::UsageSheetLabel))
    });
    if response.has_focus() {
        ui.memory_mut(|memory| memory.set_focus_lock_filter(table_id(), NAV_FOCUS_FILTER));
    }
}

/// Lignes `range` : fonds, textes du volet gauche, cadre du curseur,
/// libellés d'accessibilité et clics.
#[allow(clippy::too_many_arguments, reason = "emprunts disjoints de l'état de la vue")]
fn paint_rows(
    ui: &mut egui::Ui,
    fc: &mut FrameCtx<'_>,
    usage: &mut UsageState,
    expanded: &mut BTreeSet<ResourceUid>,
    origin: Pos2,
    range: std::ops::Range<usize>,
    left: Rect,
    right: Rect,
) {
    let clip = ui.clip_rect();
    if !clip.is_finite() || !clip.is_positive() || !origin.x.is_finite() || !origin.y.is_finite() {
        return;
    }
    let left_clip = left.intersect(clip);
    let right_clip = right.intersect(clip);
    let palette = fc.palette;
    let metrics = *fc.metrics;
    let row_h = metrics.row_h;
    let font = cell_font(ui);
    let painter = ui.painter().with_clip_rect(clip);
    let left_painter = ui.painter().with_clip_rect(left_clip);
    painter.rect_filled(left_clip, 0.0, palette.background);
    painter.rect_filled(right_clip, 0.0, palette.background);
    let mut shapes = 2_usize;
    let zebra = palette.background.lerp_to_gamma(palette.header_bg, 0.6);
    let current = usage.cursor_index();
    let texts = fc.texts;
    let mut clicked: Option<(usize, Option<UsageColumn>)> = None;
    {
        let sheet = UsageSheet::new(&usage.rows, texts);
        for i in range.clone() {
            let top = origin.y + index_to_f32(i) * row_h;
            let row_rect = Rect::from_min_max(pos2(left.left(), top), pos2(right.right(), top + row_h));
            if !row_rect.is_finite() || i >= sheet.len() {
                continue;
            }
            let selected = current == Some(i);
            if selected {
                painter.rect_filled(row_rect, 0.0, palette.selection);
                shapes = shapes.saturating_add(1);
            } else if !i.is_multiple_of(2) {
                painter.rect_filled(row_rect, 0.0, zebra);
                shapes = shapes.saturating_add(1);
            }
            painter.hline(
                row_rect.x_range(),
                row_rect.bottom() - 0.5,
                Stroke::new(1.0, palette.grid),
            );
            shapes = shapes.saturating_add(1);
            let at = RowPlace {
                rect: Rect::from_min_max(pos2(left.left(), top), pos2(left.right(), top + row_h)),
                cols_left: left.left(),
                pad: metrics.pad_x,
                indent: metrics.indent,
            };
            shapes = shapes.saturating_add(grid::paint_cells(&left_painter, &sheet, palette, i, at, &font));
            let hit = row_rect.intersect(clip);
            if hit.is_positive() {
                let response = ui.interact(
                    hit,
                    ui.make_persistent_id(("plannercraft-usage-row", i)),
                    Sense::click(),
                );
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::Role::Row,
                        true,
                        selected,
                        row_label(&usage.rows, texts, i),
                    )
                });
                if response.clicked() {
                    let column = response
                        .interact_pointer_pos()
                        .and_then(|p| grid::column_at(&COLUMNS, left.left(), p.x));
                    clicked = Some((i, column));
                }
            }
            if let Some(line) = usage.rows.line(i).filter(|l| l.expandable) {
                expand_button(ui, fc, &usage.rows, line, i, top, left.left());
            }
        }
        shapes = shapes.saturating_add(grid::paint_column_lines(
            &left_painter,
            &COLUMNS,
            left.left(),
            left_clip,
            palette.grid,
        ));
        if let Some(index) = current.filter(|i| range.contains(i)) {
            let top = origin.y + index_to_f32(index) * row_h;
            if paint_cursor(
                &left_painter,
                palette,
                usage.cursor.column,
                left.left(),
                top,
                row_h,
            ) {
                shapes = shapes.saturating_add(1);
            }
        }
    }
    fc.stats.shapes = fc.stats.shapes.saturating_add(shapes);
    if let Some((i, column)) = clicked {
        apply_click(fc, usage, expanded, i, column);
    }
}

/// Cadre de la cellule courante (colonne `column`, ligne en `top`).
fn paint_cursor(
    painter: &egui::Painter,
    palette: &Palette,
    column: UsageColumn,
    left: f32,
    top: f32,
    row_h: f32,
) -> bool {
    grid::column_span(&COLUMNS, left, column).is_some_and(|(x, w)| {
        grid::paint_cursor_frame(
            painter,
            Rect::from_min_size(pos2(x, top), vec2(w, row_h)),
            palette,
        )
    })
}

/// Clic sur la ligne `i` : curseur (ligne, colonne sous le pointeur), focus
/// de la vue ; sur « + » / « - » : déplie ou replie.
fn apply_click(
    fc: &mut FrameCtx<'_>,
    usage: &mut UsageState,
    expanded: &mut BTreeSet<ResourceUid>,
    i: usize,
    column: Option<UsageColumn>,
) {
    if let Some(line) = usage.rows.line(i) {
        usage.cursor.row = RowRef::Item(line.key);
    }
    if let Some(column) = column {
        usage.cursor.column = column;
        if column == UsageColumn::Expand {
            set_expanded(&usage.rows, expanded, i, None);
        }
    }
    fc.actions.push(Action::FocusPane(Pane::Table));
}

/// Bouton d'accessibilité du « + » / « - » d'une ressource dépliable (le
/// texte est dessiné par la grille ; le clic passe par la ligne).
fn expand_button(
    ui: &egui::Ui,
    fc: &FrameCtx<'_>,
    rows: &UsageRows,
    line: &UsageLine,
    i: usize,
    top: f32,
    left: f32,
) {
    let Some((x, w)) = grid::column_span(&COLUMNS, left, UsageColumn::Expand) else {
        return;
    };
    let rect = Rect::from_min_size(pos2(x, top), vec2(w, fc.metrics.row_h)).intersect(ui.clip_rect());
    if !rect.is_finite() || !rect.is_positive() {
        return;
    }
    let response = ui.interact(
        rect,
        ui.make_persistent_id(("plannercraft-usage-expand", i)),
        Sense::hover(),
    );
    let id = if line.expanded {
        TextId::UsageCollapse
    } else {
        TextId::UsageExpand
    };
    let name = rows.text(i, UsageColumn::Name).to_owned();
    let texts = fc.texts;
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            true,
            texts.get_args(id, &[("name", Arg::Str(name.clone().into()))]),
        )
    });
}

/// Libellé d'accessibilité de la ligne `i`.
fn row_label(rows: &UsageRows, texts: &Texts, i: usize) -> String {
    let Some(line) = rows.line(i) else {
        return String::new();
    };
    let name = rows.text(i, UsageColumn::Name).to_owned();
    match line.key {
        UsageKey::Resource(_) => texts.get_args(
            TextId::ResRowLabel,
            &[
                ("id", Arg::Count(i64::try_from(line.id).unwrap_or(i64::MAX))),
                ("name", Arg::Str(name.into())),
            ],
        ),
        UsageKey::Assignment(_) => {
            let resource = rows
                .index_of(UsageKey::Resource(line.resource))
                .map(|j| rows.text(j, UsageColumn::Name).to_owned())
                .unwrap_or_default();
            texts.get_args(
                TextId::UsageAssignmentLabel,
                // `name` : la tâche ; `detail` : la ressource.
                &[
                    ("name", Arg::Str(name.into())),
                    ("detail", Arg::Str(resource.into())),
                ],
            )
        }
    }
}

/// Clé du cache des lignes `range` : fenêtre alignée de [`KEY_WINDOW`]
/// lignes qui les contient.
fn cache_key(
    doc: &OpenDocument,
    usage: &UsageState,
    range: &std::ops::Range<usize>,
    fc: &FrameCtx<'_>,
) -> UsageCacheKey {
    let start = range
        .start
        .checked_div(KEY_WINDOW)
        .unwrap_or(0)
        .saturating_mul(KEY_WINDOW);
    let end = range
        .end
        .div_ceil(KEY_WINDOW)
        .saturating_mul(KEY_WINDOW)
        .min(usage.rows.len());
    let (resources, expanded) = usage.rows.resources_in(start..end.max(start));
    UsageCacheKey {
        doc: doc.cache_key(),
        by: timephased::granularity(fc.state.zoom),
        resources,
        expanded,
    }
}

/// Volet droit : colonnes des périodes des blocs visibles, valeurs du
/// moteur des lignes `range`.
#[allow(clippy::too_many_arguments, reason = "contexte de dessin d'une image")]
fn paint_periods(
    ui: &egui::Ui,
    fc: &mut FrameCtx<'_>,
    doc: &OpenDocument,
    usage: &mut UsageState,
    origin: Pos2,
    range: std::ops::Range<usize>,
    axis: &TimeAxis,
    right: Rect,
) {
    usage.last_cells.clear();
    usage.last_columns.clear();
    usage.last_message = None;
    // Rien au-delà de la fin de la plage (comme l'en-tête et le Gantt).
    let content_right = axis.left() + axis.content_width();
    let clip = right.intersect(ui.clip_rect());
    let clip = Rect::from_min_max(clip.min, pos2(clip.right().min(content_right), clip.bottom()));
    if !clip.is_finite() || !clip.is_positive() || !origin.y.is_finite() {
        return;
    }
    let key = cache_key(doc, usage, &range, fc);
    let week_start = doc.project().options.week_start_day;
    let chunks = timephased::visible_chunks(axis, doc.range, key.by, week_start, (clip.left(), clip.right()));
    let position: BTreeMap<ResourceUid, usize> =
        key.resources.iter().enumerate().map(|(i, r)| (*r, i)).collect();
    let painter = ui.painter().with_clip_rect(clip);
    let palette = fc.palette;
    let metrics = *fc.metrics;
    let font = cell_font(ui);
    let factors = &doc.project().options.factors;
    let mut shapes = 0_usize;
    let UsageState {
        rows,
        cache,
        last_cells,
        last_columns,
        last_message,
        ..
    } = usage;
    for (k, from, to) in chunks {
        let result = cache.get_or_fetch(&key, k, || {
            let set = doc.calendars();
            timephased::fetch(doc.project(), doc.schedule(), &set, &key, (from, to))
        });
        let chunk = match result {
            Ok(chunk) => chunk,
            Err(error) => {
                *last_message = Some(limit_message(fc.texts, *error));
                continue;
            }
        };
        for (p, span) in chunk.periods.iter().enumerate() {
            let (x0, x1) = (axis.x(span.start), axis.x(span.finish));
            if !x0.is_finite() || !x1.is_finite() || x1 <= clip.left() || x0 >= clip.right() {
                continue;
            }
            last_columns.push((x0, x1));
            let x = x1.round() - 0.5;
            if x > clip.left() && x < clip.right() {
                painter.vline(x, clip.y_range(), Stroke::new(1.0, palette.grid));
                shapes = shapes.saturating_add(1);
            }
            for i in range.clone() {
                let Some(line) = rows.line(i) else {
                    continue;
                };
                let (work, over) = match line.key {
                    UsageKey::Resource(uid) => position
                        .get(&uid)
                        .and_then(|&r| chunk.resource_cell(r, p))
                        .unwrap_or((Work::ZERO, false)),
                    UsageKey::Assignment(uid) => (chunk.assignment_cell(uid, p).unwrap_or(Work::ZERO), false),
                };
                last_cells.push(PaintedCell {
                    line: i,
                    key: line.key,
                    start: span.start,
                    finish: span.finish,
                    work,
                    over,
                });
                let text = work_text(work, line.material.as_deref(), factors, rows.labels());
                if text.is_empty() {
                    continue;
                }
                let max_w = (x1 - x0) - 2.0 * metrics.pad_x;
                if !max_w.is_finite() || max_w < grid::MIN_TEXT_WIDTH {
                    continue;
                }
                let top = origin.y + index_to_f32(i) * metrics.row_h;
                let galley = grid::truncated_galley(&painter, &text, font.clone(), max_w);
                let size = galley.size();
                let tx = grid::aligned_x(CellAlign::Right, x0, x1 - x0, metrics.pad_x, size.x);
                let ty = top + (metrics.row_h - size.y) * 0.5;
                if tx.is_finite() && ty.is_finite() {
                    let color = cell_color(over, palette);
                    painter.galley(pos2(tx, ty), galley, color);
                    shapes = shapes.saturating_add(1);
                }
            }
        }
    }
    let texts = fc.texts;
    let response = ui.interact(clip, Id::unique("plannercraft.usage-periods"), Sense::hover());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Grid, true, texts.get(TextId::UsagePeriodsLabel))
    });
    fc.stats.shapes = fc.stats.shapes.saturating_add(shapes);
}

/// Couleur du texte d'une cellule de période : rouge si la période est
/// surutilisée (H-UI-23).
#[must_use]
pub const fn cell_color(over: bool, palette: &Palette) -> egui::Color32 {
    if over { palette.critical } else { palette.text }
}

/// Message d'un refus du moteur, avec son code stable (`usage.limit`).
#[must_use]
pub fn limit_message(texts: &Texts, error: UsageError) -> String {
    texts.get_args(TextId::UsageLimit, &[("detail", Arg::Str(error.code().into()))])
}

/// Refus du moteur affiché en haut du volet droit.
fn show_message(ui: &mut egui::Ui, fc: &FrameCtx<'_>, usage: &UsageState, right: Rect) {
    let Some(message) = usage.last_message.clone() else {
        return;
    };
    let rect = Rect::from_min_size(right.min, vec2(right.width(), fc.metrics.row_h * 2.0)).intersect(right);
    if !rect.is_finite() || !rect.is_positive() {
        return;
    }
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    egui::Frame::popup(child.style()).show(&mut child, |ui| {
        ui.label(egui::RichText::new(message).color(fc.palette.critical));
    });
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable as _;
    use plannercraft_core::resource::usage::UsageError;

    use crate::{Action, App, DocOrigin, Env, LanguagePref, ThemePref, UiState, ViewKind};

    /// Fichier de ressources commité.
    const RESOURCES: &[u8] = include_bytes!("../../../../tests/mcp/expected/resources_j5.xml");

    fn harness() -> Harness<'static, App> {
        let state = UiState {
            language: LanguagePref::Fr,
            theme: ThemePref::Light,
            ..UiState::default()
        };
        let app = App::new(state, Env::fixed("2027-01-15T08:00".parse().unwrap()));
        Harness::builder()
            .with_size(egui::vec2(1280.0, 720.0))
            .with_pixels_per_point(1.0)
            .with_max_steps(8)
            .build_ui_state(crate::ui, app)
    }

    /// Refus du moteur (`usage.limit`) : message dans la vue, aucune
    /// panique ; le refus est gardé en cache (aucune nouvelle requête).
    #[test]
    fn usage_limit_message() {
        let mut harness = harness();
        harness.run();
        harness.state_mut().open_bytes(RESOURCES, DocOrigin::default());
        harness
            .state_mut()
            .dispatch(Action::SetView(ViewKind::ResourceUsage));
        harness.run();
        let limit = UsageError::Limit {
            periods: 1_000,
            resources: 1_000,
        };
        assert!(!harness.state().view.usage.cache.is_empty());
        harness.state_mut().view.usage.cache.poison(limit);
        let queries = harness.state().view.usage.cache.queries();
        harness.run();
        let app = harness.state();
        let expected = super::limit_message(app.texts(), limit);
        assert!(expected.contains("usage.limit"), "{expected}");
        assert_eq!(app.view.usage.last_message.as_deref(), Some(expected.as_str()));
        assert_eq!(app.view.usage.last_cells.len(), 0);
        assert_eq!(app.view.usage.cache.queries(), queries);
        assert!(harness.query_by_label(&expected).is_some(), "message non affiché");
    }
}
