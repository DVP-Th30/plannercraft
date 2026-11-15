//! Fenêtre « Mise en forme » (lot F1, ADR-0035, H-UI-216 à H-UI-229) : une
//! seule fenêtre à deux onglets, **Tableau** et **Graphique**, avec le
//! planning ouvert en aperçu qui réagit tout de suite. Elle remplace la
//! fenêtre « Mettre en forme les barres » (J8 lot L15, H-UI-59 à H-UI-65,
//! lots R1c et R1d, H-UI-192 à H-UI-197), dont elle garde les
//! comportements.
//!
//! - Entrées : Format → « Mise en forme… » (ruban, menu, palette de
//!   commandes ; type de barre de la tâche active), clic droit sur une barre
//!   (« Mettre en forme les barres de ce type » : onglet Graphique, ce
//!   type). Sans document, rien ne s'ouvre.
//! - En-tête : « Partir d'un modèle » (modèles intégrés les plus utiles,
//!   modèles personnels qui tiennent, le courant marqué ; « Tous les
//!   modèles… » ouvre la galerie). Pied : « Revenir au modèle »,
//!   « Enregistrer comme modèle… », « Mise en page… », phrase d'état,
//!   « Terminé ».
//! - Chaque geste est **une** commande du document (une entrée
//!   d'annulation, [`commands`]) ; curseurs et couleur libre appliqués au
//!   relâcher. Aucun bouton « Annuler » global : Ctrl/Cmd+Z marche fenêtre
//!   ouverte (fenêtre non modale, raccourcis de l'application actifs).
//! - Fenêtre non modale, déplaçable, retenue dans l'écran, ouverte au centre
//!   de la zone de travail (à gauche du rail ou du volet) ; taille bornée à
//!   l'écran. Large : réglages à gauche (ils défilent), aperçu à droite.
//!   Étroite (moins de [`WIDE_MIN`] points) : aperçu au-dessus, réduit,
//!   réglages dessous (ADR-0035 D5).

mod chart;
pub mod commands;
pub mod preview;
mod table;

use egui::{Align, Layout, Rect, Sense, UiBuilder, pos2, vec2};
use plannercraft_automation::Session;
use plannercraft_automation::text::Language;
use plannercraft_core::TaskUid;
use plannercraft_render::style::{BarRole, PlanStyle, Rgb, TemplateId, Variant, contrast_ratio};

use crate::action::{Action, DialogKind, FormatOp};
use crate::column::ColumnState;
use crate::i18n::{Arg, TextId, Texts, template_name};
use crate::row_view::{DisplayRow, VisibleRows};
use crate::state::NamedStyle;

pub use commands::StyleCtx;
pub use preview::{PreviewDrawn, preview_drawn};

/// Rapport minimal d'une barre sur le fond (H-UI-45 : 3:1).
const MIN_BAR: u32 = 3_000;

/// Palette Okabe-Ito (jfly.uni-koeln.de/color) et couleurs des plannings
/// de chantier (gris, rouge, jaune) ; partagée avec la couleur du texte et
/// le surlignage des tâches (finitions P2, H-UI-127).
pub(crate) const SWATCHES: [Rgb; 11] = [
    Rgb(0xE6, 0x9F, 0x00),
    Rgb(0x56, 0xB4, 0xE9),
    Rgb(0x00, 0x9E, 0x73),
    Rgb(0xF0, 0xE4, 0x42),
    Rgb(0x00, 0x72, 0xB2),
    Rgb(0xD5, 0x5E, 0x00),
    Rgb(0xCC, 0x79, 0xA7),
    Rgb(0x00, 0x00, 0x00),
    Rgb(0x7A, 0x7A, 0x7A),
    Rgb(0xC0, 0x00, 0x00),
    Rgb(0xFF, 0xC0, 0x00),
];

/// Largeur du contenu de la fenêtre au plus, en points.
pub const WINDOW_MAX_W: f32 = 980.0;

/// Hauteur du contenu de la fenêtre au plus, en points.
pub const WINDOW_MAX_H: f32 = 660.0;

/// Marge gardée entre la fenêtre et les bords de la zone de travail.
const SCREEN_MARGIN: f32 = 12.0;

/// Barre de titre et cadre de la fenêtre egui, en points (hauteur gardée
/// hors du contenu).
const FRAME_H: f32 = 44.0;

/// Largeur de contenu sous laquelle l'aperçu passe au-dessus des réglages.
pub const WIDE_MIN: f32 = 760.0;

/// Largeur de la colonne des réglages (fenêtre large), en points.
const CONTROLS_W: f32 = 344.0;

/// Hauteur de l'aperçu au-dessus des réglages (fenêtre étroite).
const NARROW_PREVIEW_H: f32 = 200.0;

/// Hauteur de l'en-tête (« Partir d'un modèle »).
const HEADER_H: f32 = 30.0;

/// Hauteur du pied (boutons, phrase d'état).
const FOOT_H: f32 = 30.0;

/// Modèles intégrés proposés dans l'en-tête (ceux de la maquette validée) ;
/// les autres sont dans la galerie (« Tous les modèles… »).
const HEADER_TEMPLATES: [TemplateId; 4] = [
    TemplateId::Sobre,
    TemplateId::Classique,
    TemplateId::Chantier,
    TemplateId::ImpressionNb,
];

/// Caractères au plus du nom d'un modèle personnel dans l'en-tête.
const CHIP_CHARS: usize = 18;

/// Onglet de la fenêtre.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FormatTab {
    /// Colonnes, lignes, texte des tâches.
    Table,
    /// Barres, textes des barres, échelle, fond.
    #[default]
    Chart,
}

/// État de la fenêtre ouverte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatWindowState {
    /// Onglet affiché.
    pub tab: FormatTab,
    /// Type de barre réglé (onglet Graphique).
    pub role: BarRole,
    /// Tâche de la barre cliquée (couleur de phase d'une récapitulative,
    /// à défaut de la tâche active).
    pub task: Option<TaskUid>,
    /// Colonne choisie dans l'onglet Tableau (rang dans `UiState.columns`).
    pub column: Option<usize>,
    /// Couleur libre en cours de choix (appliquée au relâcher).
    pending: Option<Rgb>,
    /// Épaisseur en cours de glisser (appliquée au relâcher).
    thickness: Option<u8>,
    /// Saisie du nom de « Enregistrer comme modèle… » affichée.
    pub saving: bool,
    /// Nom saisi pour « Enregistrer comme modèle… ».
    pub name: String,
    /// Phrase d'état du pied (sinon la phrase par défaut).
    pub status: Option<String>,
    /// Scène de l'aperçu.
    pub preview: preview::PreviewCache,
}

impl FormatWindowState {
    /// Fenêtre sur l'onglet `tab`, type `role`, ouverte depuis la barre de
    /// `task`.
    #[must_use]
    pub fn new(tab: FormatTab, role: BarRole, task: Option<TaskUid>) -> Self {
        Self {
            tab,
            role,
            task,
            column: None,
            pending: None,
            thickness: None,
            saving: false,
            name: String::new(),
            status: None,
            preview: preview::PreviewCache::default(),
        }
    }

    /// Change de type de barre (réglages en cours oubliés).
    fn set_role(&mut self, role: BarRole) {
        if role != self.role {
            self.role = role;
            self.pending = None;
            self.thickness = None;
        }
    }
}

/// Entrées de la fenêtre à une image.
pub struct FormatInput<'a> {
    /// Session du document.
    pub session: &'a Session,
    /// Clé du document (ouverture, génération).
    pub key: (u64, u64),
    /// Style du document et contexte des commandes.
    pub ctx: StyleCtx<'a>,
    /// Style tel que l'écran le dessine (`theme::screen_style` : interface
    /// adoucie comprise) : couleur courante marquée, cartes, aperçu.
    pub shown: &'a PlanStyle,
    /// Variante affichée (thème).
    pub variant: Variant,
    /// Lignes affichées par la vue (filtre, regroupement, plan replié).
    pub rows: Option<VisibleRows<'a>>,
    /// Surbrillance du chemin critique de la vue (H-UI-38) : compte des
    /// barres critiques.
    pub highlight_critical: bool,
    /// Tâche active.
    pub active: Option<TaskUid>,
    /// Nombre de tâches sélectionnées.
    pub selected: usize,
    /// Colonnes de la table (préférence d'interface).
    pub columns: &'a [ColumnState],
    /// Modèles personnels.
    pub user_templates: &'a [NamedStyle],
    /// Apparence du texte des tâches mises en forme (H-UI-126).
    pub looks: &'a crate::theme::TextLooks,
    /// Raccourci d'annulation tel qu'affiché (« Ctrl+Z », « ⌘Z »).
    pub undo: String,
}

/// Libellé d'un type de barre (cartes, listes, titre des sections).
#[must_use]
pub const fn role_label(role: BarRole) -> TextId {
    match role {
        BarRole::Task => TextId::FpRoleTask,
        BarRole::Critical => TextId::FpRoleCritical,
        BarRole::Summary => TextId::FpRoleSummary,
        BarRole::SummaryCritical => TextId::FpRoleSummaryCritical,
        BarRole::Milestone => TextId::FpRoleMilestone,
        BarRole::MilestoneCritical => TextId::FpRoleMilestoneCritical,
        BarRole::Progress => TextId::FpRoleProgress,
        BarRole::Baseline => TextId::FpRoleBaseline,
        BarRole::Inactive => TextId::FpRoleInactive,
        BarRole::Manual => TextId::FpRoleManual,
        BarRole::Slack => TextId::FpRoleSlack,
        BarRole::Deadline => TextId::FpRoleDeadline,
    }
}

/// Le type `role` est-il celui d'une ligne du Gantt (lot R1d, H-UI-197) ?
/// Avancement, ligne de base, marge et échéance se dessinent par-dessus ou
/// à côté d'une barre : ce ne sont pas des types de ligne.
#[must_use]
pub const fn is_row_role(role: BarRole) -> bool {
    match role {
        BarRole::Task
        | BarRole::Critical
        | BarRole::Summary
        | BarRole::SummaryCritical
        | BarRole::Milestone
        | BarRole::MilestoneCritical
        | BarRole::Inactive
        | BarRole::Manual => true,
        BarRole::Progress | BarRole::Baseline | BarRole::Slack | BarRole::Deadline => false,
    }
}

/// Nombre de barres du type `role` que le Gantt dessine (lot R1d,
/// H-UI-197), avec sa règle (`gantt::row_role`) : un simple parcours des
/// lignes affichées, sans allocation. Un en-tête de groupe compte pour une
/// récapitulative quand il a une barre, comme le Gantt le dessine
/// (H-UI-129).
#[must_use]
pub fn role_count(rows: &VisibleRows<'_>, role: BarRole, highlight_critical: bool) -> usize {
    (0..rows.len())
        .filter(|&i| {
            let d = DisplayRow::at(i);
            let drawn = match rows.header_bar_row(d) {
                Some(group) => super::gantt::row_role(&group, highlight_critical),
                None => rows
                    .row(d)
                    .and_then(|row| super::gantt::row_role(row, highlight_critical)),
            };
            drawn == Some(role)
        })
        .count()
}

/// Nuancier des barres (lot R1c, H-UI-193) : trois rangées dont les deux
/// premières ne dépendent pas des couleurs choisies (un clic ne déplace
/// jamais une pastille sous le pointeur) :
/// 1. couleurs du modèle de base `model` (telles que l'écran les dessine),
///    triées ;
/// 2. la palette [`SWATCHES`], moins les couleurs de la rangée 1 ;
/// 3. couleurs du document `shown` absentes des deux premières (couleurs
///    libres déjà utilisées), dans l'ordre de [`BarRole::ALL`].
///
/// Une couleur n'apparaît qu'une fois (nom accessible unique) ; rangée 3
/// vide si aucune couleur libre.
#[must_use]
pub fn swatch_rows(model: &PlanStyle, shown: &PlanStyle, v: Variant) -> [Vec<Rgb>; 3] {
    let mut first: Vec<Rgb> = BarRole::ALL
        .into_iter()
        .filter_map(|r| commands::shown_color(model, r, v))
        .collect();
    first.sort_unstable();
    first.dedup();
    let second: Vec<Rgb> = SWATCHES.iter().copied().filter(|c| !first.contains(c)).collect();
    let mut third: Vec<Rgb> = Vec::new();
    for c in BarRole::ALL
        .into_iter()
        .filter_map(|r| commands::shown_color(shown, r, v))
    {
        if !first.contains(&c) && !second.contains(&c) && !third.contains(&c) {
            third.push(c);
        }
    }
    [first, second, third]
}

/// Avertissement de contraste d'une couleur de barre ou de phase (H-UI-61 ;
/// lot R1c : la même règle pour les deux) : la couleur est mesurée sur le
/// fond de chaque variante de `bgs` (les trois pour une couleur choisie, qui
/// vaut partout ; la variante affichée seule pour une couleur du modèle) ;
/// sous 3:1 sur l'un, le pire rapport est dit **avec le fond en cause**
/// (une couleur lisible à l'écran peut ne pas l'être dans l'autre thème) et
/// la couleur conforme la plus proche sur tous les fonds mesurés est
/// proposée (aucune si elle n'existe pas). Rend la proposition cliquée.
fn contrast_warning(
    ui: &mut egui::Ui,
    texts: &Texts,
    lang: Language,
    rgb: Rgb,
    bgs: &[(Variant, Rgb)],
) -> Option<Rgb> {
    let (ratio, variant, worst) = bgs
        .iter()
        .map(|(v, bg)| (contrast_ratio(rgb, *bg), *v, *bg))
        .min_by_key(|(r, _, _)| *r)?;
    if ratio >= MIN_BAR {
        return None;
    }
    let shown = format!("{},{}", ratio / 1000, (ratio % 1000) / 100);
    let shown = if matches!(lang, Language::En) {
        shown.replace(',', ".")
    } else {
        shown
    };
    // Clé du sélecteur de `fp-contrast-low` (fond clair par défaut).
    let bg = match variant {
        Variant::Light => "light",
        Variant::Dark => "dark",
        Variant::Print => "print",
    };
    ui.add(
        egui::Label::new(
            egui::RichText::new(texts.get_args(
                TextId::FpContrastLow,
                &[("ratio", Arg::Str(shown.into())), ("bg", Arg::Str(bg.into()))],
            ))
            .color(ui.visuals().warn_fg_color),
        )
        .wrap(),
    );
    let better = plannercraft_automation::command::style::nearest_compliant(rgb, worst, MIN_BAR)
        .filter(|b| bgs.iter().all(|(_, bg)| contrast_ratio(*b, *bg) >= MIN_BAR))?;
    let label = texts.get_args(
        TextId::FpUseSuggestion,
        &[("color", Arg::Str(better.hex().into()))],
    );
    ui.button(label).clicked().then_some(better)
}

/// Bouton d'une couleur du nuancier (nom accessible « Couleur #RRGGBB ») :
/// 18 points, coins de 2 (menus de couleur du texte et du surlignage,
/// inchangés).
pub(crate) fn swatch(ui: &mut egui::Ui, texts: &Texts, label: TextId, rgb: Rgb, selected: bool) -> bool {
    swatch_sized(ui, texts, label, rgb, selected, 18.0, 2.0)
}

/// Pastille du nuancier de la fenêtre « Mise en forme » : 20 points, coins
/// de 4 (cible plus grande, maquette validée).
fn big_swatch(ui: &mut egui::Ui, texts: &Texts, label: TextId, rgb: Rgb, selected: bool) -> bool {
    swatch_sized(ui, texts, label, rgb, selected, 20.0, 4.0)
}

/// Pastille de côté `edge` et de coins `radius`.
fn swatch_sized(
    ui: &mut egui::Ui,
    texts: &Texts,
    label: TextId,
    rgb: Rgb,
    selected: bool,
    edge: f32,
    radius: f32,
) -> bool {
    let size = egui::vec2(edge, edge);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let name = texts.get_args(label, &[("color", Arg::Str(rgb.hex().into()))]);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name.clone()));
    let stroke = if selected {
        egui::Stroke::new(2.0, ui.visuals().strong_text_color())
    } else {
        egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color)
    };
    ui.painter()
        .rect_filled(rect, radius, egui::Color32::from_rgb(rgb.0, rgb.1, rgb.2));
    ui.painter()
        .rect_stroke(rect, radius, stroke, egui::StrokeKind::Inside);
    response.on_hover_text(rgb.hex()).clicked()
}

/// Identifiant de la fenêtre.
#[must_use]
pub fn window_id() -> egui::Id {
    egui::Id::unique("plannercraft.format-window")
}

/// Zone de travail : l'écran moins le rail ou le volet latéral (lot R1c).
pub(crate) fn work_area(ctx: &egui::Context) -> Rect {
    let screen = ctx.content_rect();
    let right = super::side_panel::work_right(ctx)
        .filter(|r| r.is_finite() && *r > screen.left())
        .unwrap_or(screen.right());
    Rect::from_min_max(screen.min, pos2(right.min(screen.right()), screen.bottom()))
}

/// Taille du contenu de la fenêtre pour une zone de travail `area` (H-UI-217) :
/// au plus [`WINDOW_MAX_W`] × [`WINDOW_MAX_H`], bornée à la zone moins les
/// marges et le cadre, jamais négative.
#[must_use]
pub fn content_size(area: egui::Vec2) -> egui::Vec2 {
    let w = (area.x - 2.0 * SCREEN_MARGIN).clamp(240.0, WINDOW_MAX_W);
    let h = (area.y - 2.0 * SCREEN_MARGIN - FRAME_H).clamp(200.0, WINDOW_MAX_H);
    vec2(w, h)
}

/// Phrase d'état par défaut du pied.
fn default_status(texts: &Texts, undo: &str) -> String {
    texts.get_args(TextId::FwStatus, &[("undo", Arg::Str(undo.to_owned().into()))])
}

/// Dessine la fenêtre (une image) ; rend ses actions.
pub fn show(
    ctx: &egui::Context,
    texts: &Texts,
    state: &mut FormatWindowState,
    input: &FormatInput<'_>,
) -> Vec<Action> {
    let mut actions = Vec::new();
    let area = work_area(ctx);
    let size = content_size(area.size());
    let mut open = true;
    egui::Window::new(texts.get(TextId::FwTitle))
        .id(window_id())
        .open(&mut open)
        // Au centre de la zone de travail à la première ouverture, puis
        // déplaçable par la barre de titre, retenue dans l'écran ; position
        // gardée par egui tant que l'application tourne (H-UI-217).
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(area.center())
        .constrain(true)
        .movable(true)
        .resizable(false)
        .collapsible(false)
        .fixed_size(size)
        .show(ctx, |ui| body(ui, texts, state, input, size, &mut actions));
    if !open {
        actions.push(Action::Format(FormatOp::Close));
    }
    actions
}

/// Contenu de la fenêtre : en-tête, réglages et aperçu, pied.
fn body(
    ui: &mut egui::Ui,
    texts: &Texts,
    state: &mut FormatWindowState,
    input: &FormatInput<'_>,
    size: egui::Vec2,
    actions: &mut Vec<Action>,
) {
    let full = Rect::from_min_size(ui.cursor().min, size);
    let gap = 8.0;
    let header = Rect::from_min_size(full.min, vec2(size.x, HEADER_H));
    let foot = Rect::from_min_max(pos2(full.left(), full.bottom() - FOOT_H), full.max);
    let middle = Rect::from_min_max(
        pos2(full.left(), header.bottom() + gap),
        pos2(full.right(), foot.top() - gap),
    );
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(header)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| header_row(ui, texts, state, input, actions),
    );
    let rule = ui.visuals().widgets.noninteractive.bg_stroke;
    ui.painter()
        .hline(full.x_range(), header.bottom() + gap / 2.0, rule);
    ui.painter().hline(full.x_range(), foot.top() - gap / 2.0, rule);
    if size.x >= WIDE_MIN {
        let controls = Rect::from_min_size(middle.min, vec2(CONTROLS_W, middle.height()));
        let preview = Rect::from_min_max(pos2(controls.right() + 2.0 * gap, middle.top()), middle.max);
        ui.painter().vline(controls.right() + gap, middle.y_range(), rule);
        controls_column(ui, texts, state, input, controls, actions);
        preview_column(ui, texts, state, input, preview);
    } else {
        let preview = Rect::from_min_size(middle.min, vec2(middle.width(), NARROW_PREVIEW_H));
        let controls = Rect::from_min_max(pos2(middle.left(), preview.bottom() + gap), middle.max);
        preview_column(ui, texts, state, input, preview);
        controls_column(ui, texts, state, input, controls, actions);
    }
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(foot)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| foot_row(ui, texts, state, input, actions),
    );
    ui.advance_cursor_after_rect(full);
}

/// En-tête : « Partir d'un modèle », modèles intégrés et personnels qui
/// tiennent, « Tous les modèles… » (H-UI-226).
fn header_row(
    ui: &mut egui::Ui,
    texts: &Texts,
    state: &mut FormatWindowState,
    input: &FormatInput<'_>,
    actions: &mut Vec<Action>,
) {
    ui.label(egui::RichText::new(texts.get(TextId::FwFromTemplate)).weak());
    let current = input.ctx.doc.map_or_else(
        || {
            (
                input.ctx.default,
                plannercraft_render::style::StylePatch::default(),
            )
        },
        |d| (d.base, d.over.template_part()),
    );
    let user_match = input
        .user_templates
        .iter()
        .position(|t| t.style.base == current.0 && t.style.over.template_part() == current.1);
    let all = texts.get(TextId::FwAllTemplates);
    let font = egui::TextStyle::Button.resolve(ui.style());
    let width_of = |ui: &egui::Ui, s: &str| {
        ui.painter()
            .layout_no_wrap(s.to_owned(), font.clone(), egui::Color32::WHITE)
            .size()
            .x
            + 2.0 * ui.spacing().button_padding.x
            + ui.spacing().item_spacing.x
    };
    let reserve = width_of(ui, &all) + 4.0;
    for id in HEADER_TEMPLATES {
        let name = texts.get(template_name(id));
        // Au-delà de ce qui tient : dans la galerie (« Tous les modèles… »).
        if ui.available_width() < width_of(ui, &name) + reserve {
            break;
        }
        let on = user_match.is_none() && current.0 == id;
        if ui.selectable_label(on, name.as_str()).clicked() {
            actions.push(Action::Exec(crate::dialog::gallery::template_batch(
                id,
                None,
                input.ctx.lang,
            )));
            state.status = Some(texts.get_args(
                TextId::FwTemplateApplied,
                &[("name", Arg::Str(name.clone().into()))],
            ));
        }
    }
    for (i, t) in input.user_templates.iter().enumerate() {
        let mut name: String = t.name.chars().take(CHIP_CHARS).collect();
        if t.name.chars().count() > CHIP_CHARS {
            name.push('…');
        }
        if ui.available_width() < width_of(ui, &name) + reserve {
            break;
        }
        let chip = ui
            .selectable_label(user_match == Some(i), name.as_str())
            .on_hover_text(t.name.as_str());
        if chip.clicked() {
            actions.push(Action::Exec(crate::dialog::gallery::template_batch(
                t.style.base,
                Some(&t.style.over),
                input.ctx.lang,
            )));
            state.status = Some(texts.get_args(
                TextId::FwTemplateApplied,
                &[("name", Arg::Str(t.name.clone().into()))],
            ));
        }
    }
    if ui.link(all).clicked() {
        actions.push(Action::ShowDialog(DialogKind::Gallery));
    }
}

/// Colonne des réglages : onglets, puis réglages de l'onglet (défilants).
fn controls_column(
    ui: &mut egui::Ui,
    texts: &Texts,
    state: &mut FormatWindowState,
    input: &FormatInput<'_>,
    rect: Rect,
    actions: &mut Vec<Action>,
) {
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min)),
        |ui| {
            tabs(ui, texts, state);
            ui.add_space(6.0);
            egui::ScrollArea::vertical()
                .id_salt((
                    "plannercraft.format-window.controls",
                    state.tab == FormatTab::Chart,
                ))
                .auto_shrink([false, false])
                .max_height(ui.available_height())
                .show(ui, |ui| {
                    ui.set_width(ui.available_width() - 10.0);
                    match state.tab {
                        FormatTab::Chart => chart::show(ui, texts, state, input, actions),
                        FormatTab::Table => table::show(ui, texts, state, input, actions),
                    }
                });
        },
    );
}

/// Onglets « Tableau » et « Graphique », en contrôle segmenté.
fn tabs(ui: &mut egui::Ui, texts: &Texts, state: &mut FormatWindowState) {
    let width = ui.available_width();
    let height = 28.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let back = ui.visuals().widgets.inactive.weak_bg_fill;
    ui.painter().rect_filled(rect, 8.0, back);
    let half = (width - 6.0) / 2.0;
    for (n, (tab, id)) in [
        (FormatTab::Table, TextId::FwTabTable),
        (FormatTab::Chart, TextId::FwTabChart),
    ]
    .into_iter()
    .enumerate()
    {
        #[allow(clippy::cast_precision_loss, reason = "rang d'onglet, 0 ou 1")]
        let left = rect.left() + 3.0 + n as f32 * half;
        let r = Rect::from_min_size(pos2(left, rect.top() + 3.0), vec2(half, height - 6.0));
        let response = ui.interact(
            r,
            egui::Id::unique("plannercraft.format-window.tab").with(n),
            Sense::click(),
        );
        let on = state.tab == tab;
        let name = texts.get(id);
        response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, name.clone()));
        if on {
            ui.painter().rect_filled(r, 6.0, ui.visuals().panel_fill);
            ui.painter().rect_stroke(
                r,
                6.0,
                ui.visuals().widgets.noninteractive.bg_stroke,
                egui::StrokeKind::Inside,
            );
        } else if response.hovered() {
            ui.painter()
                .rect_filled(r, 6.0, ui.visuals().widgets.hovered.weak_bg_fill);
        }
        let color = if on {
            ui.visuals().strong_text_color()
        } else {
            ui.visuals().weak_text_color()
        };
        ui.painter().text(
            r.center(),
            egui::Align2::CENTER_CENTER,
            name,
            crate::theme::strong_font(ui.ctx(), 13.0),
            color,
        );
        if response.clicked() {
            state.tab = tab;
        }
    }
}

/// Colonne de l'aperçu : titre, aperçu, phrase sur le clic.
fn preview_column(
    ui: &mut egui::Ui,
    texts: &Texts,
    state: &mut FormatWindowState,
    input: &FormatInput<'_>,
    rect: Rect,
) {
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min)),
        |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(texts.get(TextId::FwPreviewTitle)).weak());
                let what = match state.tab {
                    FormatTab::Table => texts.get(TextId::FwTabTable),
                    FormatTab::Chart => texts.get(role_label(state.role)),
                };
                ui.label(
                    egui::RichText::new(
                        texts.get_args(TextId::FwPreviewSetting, &[("name", Arg::Str(what.into()))]),
                    )
                    .strong(),
                );
            });
            let hint_h = 34.0;
            let sheet_h = (ui.available_height() - hint_h).max(60.0);
            let (sheet, _) = ui.allocate_exact_size(vec2(ui.available_width(), sheet_h), Sense::hover());
            let pin = preview::PreviewInput {
                session: input.session,
                key: input.key,
                style: input.shown,
                variant: input.variant,
                lang: input.ctx.lang,
                default: input.ctx.default,
                rows: input.rows,
                active: input.active,
                columns: input.columns,
                looks: input.looks,
            };
            match preview::show(ui, texts, &mut state.preview, &pin, sheet) {
                Some(preview::PreviewHit::Table) => state.tab = FormatTab::Table,
                Some(preview::PreviewHit::Chart(role)) => {
                    state.tab = FormatTab::Chart;
                    if let Some(role) = role {
                        state.set_role(role);
                    }
                }
                None => {}
            }
            ui.add(
                egui::Label::new(
                    egui::RichText::new(texts.get(TextId::FwPreviewHint))
                        .weak()
                        .small(),
                )
                .wrap(),
            );
        },
    );
}

/// Pied : « Revenir au modèle », « Enregistrer comme modèle… »,
/// « Mise en page… », phrase d'état, « Terminé ».
fn foot_row(
    ui: &mut egui::Ui,
    texts: &Texts,
    state: &mut FormatWindowState,
    input: &FormatInput<'_>,
    actions: &mut Vec<Action>,
) {
    if state.saving {
        let name = texts.get(TextId::GallerySaveName);
        let field = crate::dialog::text_field(ui, &name, &mut state.name, true, 200.0);
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui.button(texts.get(TextId::FwSave)).clicked() || enter {
            actions.push(Action::Format(FormatOp::SaveUserTemplate(state.name.clone())));
        }
        if ui.button(texts.get(TextId::FwSaveCancel)).clicked() {
            state.saving = false;
        }
    } else {
        if ui.button(texts.get(TextId::FpRevert)).clicked() {
            actions.push(Action::Exec(commands::reset_all(input.ctx.lang)));
            let name = texts.get(template_name(input.ctx.base()));
            state.status = Some(texts.get_args(TextId::FwReverted, &[("name", Arg::Str(name.into()))]));
        }
        if ui.button(texts.get(TextId::FpSaveTemplate)).clicked() {
            state.saving = true;
        }
        if ui.link(texts.get(TextId::MenuPageSetup)).clicked() {
            actions.push(Action::ShowDialog(DialogKind::PageSetup { print_mode: false }));
        }
    }
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let done = egui::Button::new(egui::RichText::new(texts.get(TextId::FwDone)).strong())
            .fill(ui.visuals().selection.bg_fill);
        if ui.add(done).clicked() {
            actions.push(Action::Format(FormatOp::Close));
        }
        let status = state
            .status
            .clone()
            .unwrap_or_else(|| default_status(texts, &input.undo));
        ui.add(egui::Label::new(egui::RichText::new(status).weak().small()).truncate());
    });
}

impl crate::app::App {
    /// Applique une opération de mise en forme (J8 lot L15 ; lot F1).
    pub(crate) fn format_op(&mut self, op: FormatOp) {
        match op {
            FormatOp::Open { tab, role, task } => {
                if self.document.is_some() {
                    match &mut self.view.format_window {
                        // Déjà ouverte : onglet et type demandés.
                        Some(w) => {
                            w.tab = tab;
                            w.set_role(role);
                            w.task = task.or(w.task);
                        }
                        None => self.view.format_window = Some(FormatWindowState::new(tab, role, task)),
                    }
                }
            }
            FormatOp::Close => self.view.format_window = None,
            FormatOp::SetTab(tab) => {
                if let Some(w) = &mut self.view.format_window {
                    w.tab = tab;
                }
            }
            FormatOp::SetRole(role) => {
                if let Some(w) = &mut self.view.format_window {
                    w.set_role(role);
                }
            }
            FormatOp::SaveUserTemplate(name) => {
                let (text, ok) = self.save_user_template_named(&name);
                match &mut self.view.format_window {
                    Some(w) => {
                        w.status = Some(text);
                        if ok {
                            w.saving = false;
                            w.name.clear();
                        }
                    }
                    None => self.push_message(plannercraft_core::Severity::Info, text),
                }
            }
            FormatOp::ApplyUserTemplate(i) => {
                let lang = self.text_language();
                if let Some(t) = self.state.user_templates.get(i) {
                    let batch =
                        crate::dialog::gallery::template_batch(t.style.base, Some(&t.style.over), lang);
                    drop(self.execute(&batch, crate::app::EditOrigin::User));
                }
            }
            FormatOp::SetDefaultTemplate(id) => self.state.default_template = id,
            FormatOp::SetRelativeMonths(on) => self.set_relative_months(on),
        }
    }

    /// H-UI-67 : mois relatifs de l'échelle, en surcharge du style du
    /// document (`style.apply { patch: { scale } }`, échelle résolue avec
    /// le seul drapeau changé ; document sans style : modèle par défaut).
    fn set_relative_months(&mut self, on: bool) {
        let lang = self.text_language();
        let Some(document) = &self.document else {
            return;
        };
        let style = self.state.effective_style(document.project());
        let doc = plannercraft_automation::command::style::doc_style(document.project());
        let ctx = StyleCtx {
            style: &style,
            doc: doc.as_ref(),
            default: self.state.default_template,
            lang,
        };
        if let Some(batch) = commands::scale(&ctx, |s| s.relative_months = on) {
            drop(self.execute(&batch, crate::app::EditOrigin::User));
        }
    }

    /// Dessine la fenêtre « Mise en forme » si elle est ouverte ; ses lots
    /// passent par la file des actions (une entrée d'annulation chacun).
    pub(crate) fn show_format_window(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        if self.document.is_none() {
            self.view.format_window = None;
        }
        let Some(mut state) = self.view.format_window.take() else {
            preview::clear_drawn(ctx);
            return;
        };
        let lang = self.text_language();
        // Style tel que dessiné (interface adoucie comprise, lot R1c).
        let screen = self.screen_style(ctx.theme()).0;
        let Some(document) = &self.document else {
            return;
        };
        let style = self.state.effective_style(document.project());
        let doc = plannercraft_automation::command::style::doc_style(document.project());
        let undo = ctx.format_shortcut(&egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND,
            egui::Key::Z,
        ));
        let input = FormatInput {
            session: &document.session,
            key: document.cache_key(),
            ctx: StyleCtx {
                style: &style,
                doc: doc.as_ref(),
                default: self.state.default_template,
                lang,
            },
            shown: &screen.style,
            variant: screen.variant,
            rows: self
                .rows
                .as_ref()
                .map(|model| VisibleRows::new(model, &self.row_view)),
            highlight_critical: self.view.task_view.highlight_critical,
            active: self.selection,
            selected: self.selected.len(),
            columns: &self.state.columns,
            user_templates: &self.state.user_templates,
            looks: &screen.text_looks,
            undo,
        };
        actions.extend(show(ctx, &self.texts, &mut state, &input));
        self.view.format_window = Some(state);
    }
}
