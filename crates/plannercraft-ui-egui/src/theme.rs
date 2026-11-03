//! Palette unique de la table et du Gantt, claire et sombre.
//!
//! Choisie sur le thème **résolu** (`ctx.theme()`), jamais sur la préférence.
//! Nuances propres (barres bleues, chemin critique rouge, récapitulatives et
//! jalons sombres, comme l'usage de MS Project), non copiées (hypothèse).

use std::sync::Arc;

use egui::Color32;
use plannercraft_render::style::{
    BarRole, ColorTokens, FontChoice, PlanStyle, Rgb, TemplateId, Variant, blend, contrast_ratio,
    min_text_contrast, template,
};

/// Couleurs de dessin de la vue principale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    /// Fond des corps de table et de Gantt.
    pub background: Color32,
    /// Texte courant.
    pub text: Color32,
    /// Texte appuyé (récapitulatives).
    pub text_strong: Color32,
    /// Texte atténué (lignes vides, tâches exclues).
    pub text_weak: Color32,
    /// Grille fine (niveau bas de l'échelle, séparateurs de colonnes).
    pub grid: Color32,
    /// Grille marquée (niveau haut de l'échelle).
    pub grid_major: Color32,
    /// Fond des en-têtes.
    pub header_bg: Color32,
    /// Teinte du bloc sélectionné et des lignes entières de la table
    /// (H-UI-103 : ≥ 1,5:1 sur le fond, ≥ 1,3:1 sur le zébrage et les
    /// bandes, texte lisible ; dérivée par [`SelectionTokens::derive`]).
    pub selection: Color32,
    /// Barre de tâche.
    pub bar: Color32,
    /// Contour de barre de tâche.
    pub bar_outline: Color32,
    /// Barre et lien du chemin critique.
    pub critical: Color32,
    /// Losange de jalon.
    pub milestone: Color32,
    /// Barre à crochets de récapitulative.
    pub summary: Color32,
    /// Lien entre tâches.
    pub link: Color32,
    /// Ligne de la date d'état.
    pub status_date: Color32,
    /// Séparateur table / Gantt.
    pub splitter: Color32,
    /// Fond des jours chômés du calendrier du projet (week-ends, jours
    /// fériés) dans le Gantt, aux zooms Jour et Semaine (J7, lot L7, N3).
    pub non_working: Color32,
    /// Barre à crochets d'une récapitulative critique.
    pub summary_critical: Color32,
    /// Fond du bandeau d'un message d'erreur (texte en `text_strong`).
    pub message_error_bg: Color32,
    /// Fond du bandeau d'un avertissement (texte en `text_strong`).
    pub message_warning_bg: Color32,
    /// Cadre de la cellule courante de la table (J7, lot L1) : contraste
    /// ≥ 3:1 sur le fond, le fond alterné et la sélection.
    pub cursor: Color32,
    /// Barre de la ligne de base 0 du Gantt de suivi (J7, lot L7,
    /// H-UI-22) : contraste ≥ 3:1 sur le fond et sur les jours chômés.
    pub baseline: Color32,
    /// Bande d'avancement du Gantt de suivi, dessinée dans la barre (J7,
    /// lot L7, H-UI-22) : contraste ≥ 3:1 sur la barre et sur la barre
    /// critique.
    pub progress: Color32,
    /// Bordure de 2 px du bloc sélectionné (ADR-0018 D11, H-UI-103 : ≥ 3:1
    /// sur le fond, les bandes et la teinte).
    pub selection_border: Color32,
    /// Fond des ID des lignes de la sélection, en-tête « appuyé », et des
    /// lignes sélectionnées du Gantt (H-UI-103).
    pub header_selected: Color32,
    /// Survol discret de la ligne sous le pointeur (H-UI-103, jamais
    /// pendant un glisser).
    pub hover_row: Color32,
}

impl Palette {
    /// Palette claire.
    pub const LIGHT: Self = Self {
        background: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        text: Color32::from_rgb(0x1F, 0x1F, 0x1F),
        text_strong: Color32::from_rgb(0x00, 0x00, 0x00),
        text_weak: Color32::from_rgb(0x6B, 0x70, 0x78),
        grid: Color32::from_rgb(0xE3, 0xE5, 0xE8),
        grid_major: Color32::from_rgb(0xC4, 0xC8, 0xCE),
        header_bg: Color32::from_rgb(0xF3, 0xF4, 0xF6),
        selection: Color32::from_rgb(0xA3, 0xBF, 0xE6),
        bar: Color32::from_rgb(0x3F, 0x7C, 0xC4),
        bar_outline: Color32::from_rgb(0x2B, 0x5C, 0x99),
        critical: Color32::from_rgb(0xC9, 0x30, 0x2C),
        milestone: Color32::from_rgb(0x26, 0x26, 0x26),
        summary: Color32::from_rgb(0x26, 0x26, 0x26),
        link: Color32::from_rgb(0x4A, 0x4F, 0x57),
        status_date: Color32::from_rgb(0x7B, 0x3F, 0xB8),
        splitter: Color32::from_rgb(0xC4, 0xC8, 0xCE),
        non_working: Color32::from_rgb(0xED, 0xEF, 0xF2),
        summary_critical: Color32::from_rgb(0x8E, 0x1F, 0x1C),
        message_error_bg: Color32::from_rgb(0xFD, 0xEC, 0xEA),
        message_warning_bg: Color32::from_rgb(0xFF, 0xF4, 0xD6),
        cursor: Color32::from_rgb(0x1F, 0x5F, 0xBF),
        baseline: Color32::from_rgb(0x80, 0x86, 0x8F),
        progress: Color32::from_rgb(0x10, 0x14, 0x1A),
        selection_border: Color32::from_rgb(0x1F, 0x5F, 0xBF),
        header_selected: Color32::from_rgb(0xD3, 0xDF, 0xF0),
        hover_row: Color32::from_rgb(0xE9, 0xEF, 0xF8),
    };

    /// Palette sombre.
    pub const DARK: Self = Self {
        background: Color32::from_rgb(0x1E, 0x1F, 0x22),
        text: Color32::from_rgb(0xE6, 0xE6, 0xE6),
        text_strong: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        text_weak: Color32::from_rgb(0x9A, 0xA0, 0xA8),
        grid: Color32::from_rgb(0x33, 0x36, 0x3C),
        grid_major: Color32::from_rgb(0x4A, 0x4E, 0x56),
        header_bg: Color32::from_rgb(0x26, 0x28, 0x2C),
        selection: Color32::from_rgb(0x36, 0x4B, 0x67),
        bar: Color32::from_rgb(0x6F, 0xA8, 0xE8),
        bar_outline: Color32::from_rgb(0x4A, 0x86, 0xC8),
        critical: Color32::from_rgb(0xFF, 0x6B, 0x66),
        milestone: Color32::from_rgb(0xE6, 0xE6, 0xE6),
        summary: Color32::from_rgb(0xE6, 0xE6, 0xE6),
        link: Color32::from_rgb(0xB5, 0xBA, 0xC2),
        status_date: Color32::from_rgb(0xC4, 0x9B, 0xFF),
        splitter: Color32::from_rgb(0x4A, 0x4E, 0x56),
        non_working: Color32::from_rgb(0x2A, 0x2C, 0x31),
        summary_critical: Color32::from_rgb(0xFF, 0x8A, 0x85),
        message_error_bg: Color32::from_rgb(0x3A, 0x14, 0x14),
        message_warning_bg: Color32::from_rgb(0x3A, 0x30, 0x12),
        cursor: Color32::from_rgb(0x8A, 0xB4, 0xF8),
        baseline: Color32::from_rgb(0x8E, 0x94, 0x9C),
        progress: Color32::from_rgb(0x0F, 0x23, 0x40),
        selection_border: Color32::from_rgb(0x8A, 0xB4, 0xF8),
        header_selected: Color32::from_rgb(0x2C, 0x36, 0x44),
        hover_row: Color32::from_rgb(0x25, 0x2B, 0x33),
    };

    /// Palette tirée d'un style du planning (ADR-0016 D5) pour une variante.
    ///
    /// Le séparateur table / Gantt prend la grille marquée (égaux en J7). Un
    /// rôle de barre absent (style non validé) prend le texte courant.
    #[must_use]
    pub fn from_style(style: &PlanStyle, variant: Variant) -> Self {
        let c = style.tokens(variant);
        let sel = SelectionTokens::derive(c, min_text_contrast(style));
        let rgb = |v: Rgb| Color32::from_rgb(v.0, v.1, v.2);
        let fill = |role: BarRole| rgb(c.bars.get(&role).map_or(c.text, |p| p.fill));
        let outline = |role: BarRole| rgb(c.bars.get(&role).map_or(c.text, |p| p.outline.unwrap_or(p.fill)));
        Self {
            background: rgb(c.bg),
            text: rgb(c.text),
            text_strong: rgb(c.text_strong),
            text_weak: rgb(c.text_muted),
            grid: rgb(c.grid),
            grid_major: rgb(c.grid_major),
            header_bg: rgb(c.header_bg),
            selection: rgb(sel.selection),
            bar: fill(BarRole::Task),
            bar_outline: outline(BarRole::Task),
            critical: fill(BarRole::Critical),
            milestone: fill(BarRole::Milestone),
            summary: fill(BarRole::Summary),
            link: rgb(c.link),
            status_date: rgb(c.status_line),
            splitter: rgb(c.grid_major),
            non_working: rgb(c.non_working),
            summary_critical: fill(BarRole::SummaryCritical),
            message_error_bg: rgb(c.message_error_bg),
            message_warning_bg: rgb(c.message_warning_bg),
            cursor: rgb(c.cursor),
            baseline: fill(BarRole::Baseline),
            progress: fill(BarRole::Progress),
            selection_border: rgb(sel.border),
            header_selected: rgb(sel.header_selected),
            hover_row: rgb(sel.hover_row),
        }
    }

    /// Palette du thème résolu.
    #[must_use]
    pub const fn for_theme(theme: egui::Theme) -> Self {
        match theme {
            egui::Theme::Light => Self::LIGHT,
            egui::Theme::Dark => Self::DARK,
        }
    }
}

/// Contraste exigé du texte d'une notification sur son fond (H-UI-104,
/// WCAG AA, millièmes).
pub const TOAST_TEXT_CONTRAST: u32 = 4500;

/// Contraste exigé du liseré et de l'icône d'une notification sur son fond
/// (H-UI-104, élément graphique, WCAG 1.4.11, millièmes).
pub const TOAST_ACCENT_CONTRAST: u32 = 3000;

/// Ambres candidats du liseré d'un avertissement : le plus contrasté sur le
/// fond de la notification est retenu (foncé sur fond clair, clair sur fond
/// sombre).
const TOAST_WARNING_CANDIDATES: [Color32; 2] = [
    Color32::from_rgb(0x9A, 0x5B, 0x00),
    Color32::from_rgb(0xF2, 0xB8, 0x4B),
];

/// Jetons de dessin des notifications (seconde passe UX, lot L10,
/// H-UI-104) : carte au fond du planning, liseré gauche et icône colorés
/// selon la sorte, ombre portée. Dérivés de la [`Palette`] de l'image
/// (modèle et variante du document), jamais écrits en dur par modèle :
/// texte ≥ 4,5:1 et liserés ≥ 3:1 sur le fond, testés pour chaque modèle
/// et chaque variante (`h_ui_104_toast_contrast`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToastTokens {
    /// Fond de la carte.
    pub bg: Color32,
    /// Texte (titre, phrase, détails).
    pub text: Color32,
    /// Contour fin de la carte.
    pub border: Color32,
    /// Liseré et icône d'une information.
    pub info: Color32,
    /// Liseré et icône d'un avertissement.
    pub warning: Color32,
    /// Liseré et icône d'un refus.
    pub error: Color32,
    /// Ombre portée (translucide).
    pub shadow: Color32,
}

impl ToastTokens {
    /// Jetons des notifications pour la palette `p`.
    #[must_use]
    pub fn of(p: &Palette) -> Self {
        let rgb = |c: Color32| Rgb(c.r(), c.g(), c.b());
        let on_bg = |c: Color32| contrast_ratio(rgb(c), rgb(p.background));
        // Le plus contrasté de `candidates` sur le fond (premier à égalité).
        let best = |candidates: &[Color32]| {
            candidates
                .iter()
                .copied()
                .fold(None::<(Color32, u32)>, |acc, c| {
                    let r = on_bg(c);
                    match acc {
                        Some((_, best)) if best >= r => acc,
                        _ => Some((c, r)),
                    }
                })
                .map_or(p.text_strong, |(c, _)| c)
        };
        // Fond clair : ombre légère ; fond sombre : ombre plus dense.
        let light = on_bg(Color32::BLACK) >= on_bg(Color32::WHITE);
        let shadow = if light {
            Color32::from_black_alpha(48)
        } else {
            Color32::from_black_alpha(140)
        };
        Self {
            bg: p.background,
            text: best(&[p.text_strong, p.text]),
            border: p.grid_major,
            // Liserés : couleur du thème si elle se voit, sinon le texte.
            info: best(&[p.cursor, p.text_strong]),
            warning: best(&TOAST_WARNING_CANDIDATES),
            error: best(&[p.critical, p.text_strong]),
            shadow,
        }
    }
}

/// Teinte du bloc sur le fond (H-UI-103, millièmes) : seuil exigé.
pub const SELECTION_ON_BG: u32 = 1500;

/// Teinte visée sur le fond (millièmes) : au-dessus du seuil, pour que la
/// sélection se voie d'un coup d'œil (SEL-08 : 1,2:1 en clair, 1,5:1 en
/// sombre avant la passe UX).
const SELECTION_GOAL: u32 = 1800;

/// Teinte du bloc sur le zébrage et les bandes (H-UI-103, millièmes).
pub const SELECTION_ON_BAND: u32 = 1300;

/// Bordure du bloc sur le fond, les bandes et la teinte (WCAG 1.4.11).
pub const BORDER_CONTRAST: u32 = 3000;

/// ID appuyé (`header_selected`) sur l'en-tête et sur le fond (millièmes) :
/// distinct sans être criard ; c'est aussi le fond des lignes
/// sélectionnées du Gantt, où les barres doivent rester lisibles.
const PRESSED_ON_HEADER: u32 = 1200;

/// Zébrage de la table sans bandes de style (`lerp_to_gamma(header_bg,
/// 0.6)` au dessin), en entiers.
fn zebra_of(c: &ColorTokens) -> Rgb {
    blend(c.bg, c.header_bg, 60)
}

/// Jetons de sélection dérivés des jetons d'un style (H-UI-103, lot L5 de
/// la passe UX) : un style personnalisé garde ses couleurs, renforcées
/// juste assez pour tenir les seuils, pour toutes les variantes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectionTokens {
    /// Teinte des cellules du bloc et des lignes entières.
    pub selection: Rgb,
    /// Bordure de 2 px du bloc.
    pub border: Rgb,
    /// ID appuyé des lignes de la sélection ; fond des lignes
    /// sélectionnées du Gantt.
    pub header_selected: Rgb,
    /// Survol discret d'une ligne.
    pub hover_row: Rgb,
}

impl SelectionTokens {
    /// Dérive les jetons de `c`, texte lisible à `min_text` (millièmes).
    ///
    /// - Teinte : la `selection` du style, mêlée par pas de 5 % vers le
    ///   cadre du curseur (bleu du style) jusqu'à
    ///   [`SELECTION_GOAL`] sur le fond et [`SELECTION_ON_BAND`] sur le
    ///   zébrage et les deux bandes, texte et texte appuyé ≥ `min_text` ;
    ///   sinon le premier pas qui tient [`SELECTION_ON_BG`] ; sinon le pas
    ///   le plus marqué qui garde le texte lisible.
    /// - Bordure : le cadre du curseur s'il tient [`BORDER_CONTRAST`] sur
    ///   le fond, le zébrage, les bandes et la teinte ; sinon le texte
    ///   appuyé.
    /// - ID appuyé : l'en-tête mêlé vers la teinte jusqu'à
    ///   `PRESSED_ON_HEADER` sur l'en-tête et le fond, texte lisible.
    /// - Survol : à mi-chemin du fond et de l'ID appuyé.
    #[must_use]
    pub fn derive(c: &ColorTokens, min_text: u32) -> Self {
        let inks = [c.text, c.text_strong];
        let readable = |s: Rgb| inks.iter().all(|ink| contrast_ratio(*ink, s) >= min_text);
        let backs = [zebra_of(c), c.band[0], c.band[1]];
        let tinted = |s: Rgb, on_bg: u32| {
            contrast_ratio(s, c.bg) >= on_bg
                && backs.iter().all(|b| contrast_ratio(s, *b) >= SELECTION_ON_BAND)
                && readable(s)
        };
        let steps = || (0..=20_u8).map(|k| blend(c.selection, c.cursor, k.saturating_mul(5)));
        let selection = steps()
            .find(|s| tinted(*s, SELECTION_GOAL))
            .or_else(|| steps().find(|s| tinted(*s, SELECTION_ON_BG)))
            .or_else(|| steps().take_while(|s| readable(*s)).last())
            .unwrap_or(c.selection);
        let border_ok = |b: Rgb| {
            [c.bg, selection]
                .iter()
                .chain(backs.iter())
                .all(|on| contrast_ratio(b, *on) >= BORDER_CONTRAST)
        };
        let border = if border_ok(c.cursor) {
            c.cursor
        } else {
            c.text_strong
        };
        let header_selected = (0..=20_u8)
            .map(|k| blend(c.header_bg, selection, k.saturating_mul(5)))
            .find(|h| {
                contrast_ratio(*h, c.header_bg) >= PRESSED_ON_HEADER
                    && contrast_ratio(*h, c.bg) >= PRESSED_ON_HEADER
                    && readable(*h)
            })
            .unwrap_or(selection);
        Self {
            selection,
            border,
            header_selected,
            hover_row: blend(c.bg, header_selected, 50),
        }
    }
}

/// Variante du style pour un thème résolu de l'interface.
#[must_use]
pub const fn variant_for(theme: egui::Theme) -> Variant {
    match theme {
        egui::Theme::Light => Variant::Light,
        egui::Theme::Dark => Variant::Dark,
    }
}

/// Famille de police du planning (Inter Regular), chargée seulement quand
/// le style effectif la demande (ADR-0016 D9, J8 lot L11).
pub const PLAN_FAMILY: &str = "plannercraft-plan";

/// Famille Inter `SemiBold` (textes appuyés : récapitulatives).
pub const PLAN_FAMILY_STRONG: &str = "plannercraft-plan-semibold";

/// Inter Regular (OFL-1.1, `assets/fonts`, mention dans `NOTICE`).
static INTER_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");

/// Inter `SemiBold`.
static INTER_SEMIBOLD: &[u8] = include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf");

/// Installe les deux familles Inter du planning dans `ctx` (sans effet si
/// elles y sont déjà ; egui ne relit que les noms). Les polices d'egui
/// suivent en repli pour les caractères absents d'Inter.
pub fn install_plan_fonts(ctx: &egui::Context) {
    use egui::epaint::text::{FontData, FontInsert, FontPriority, InsertFontFamily};
    for (name, bytes) in [(PLAN_FAMILY, INTER_REGULAR), (PLAN_FAMILY_STRONG, INTER_SEMIBOLD)] {
        ctx.add_font(FontInsert::new(
            name,
            FontData::from_static(bytes),
            vec![InsertFontFamily {
                family: egui::FontFamily::Name(name.into()),
                priority: FontPriority::Highest,
            }],
        ));
    }
}

/// Style du planning lu par l'écran à une image (J8, lot L11, ADR-0016
/// D5) : style effectif du document (modèle par défaut des préférences pour
/// un document sans style, H-UI-43) et variante du thème résolu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScreenStyle {
    /// Style résolu.
    pub style: Arc<PlanStyle>,
    /// Variante (clair ou sombre).
    pub variant: Variant,
    /// Phase (0 ou 1) de chaque ligne du modèle pour les bandes de phase
    /// (H-UI-53) ; vide sans bandes de phase. Calculée une fois par
    /// génération du document, jamais pendant une image.
    pub phases: Arc<Vec<u8>>,
    /// Couleur de phase par tâche (H-UI-65, J8 lot L15) : `(couleur, la
    /// tâche porte elle-même la couleur)` ; vide sans couleur de phase.
    /// Calculée une fois par génération du document.
    pub phase_colors: Arc<PhaseColors>,
}

/// Couleurs de phase des tâches (H-UI-65) : UID → (couleur, couleur
/// propre).
pub type PhaseColors = std::collections::BTreeMap<plannercraft_core::TaskUid, (Rgb, bool)>;

/// Couleurs de phase de chaque tâche d'un projet (même règle que la scène :
/// `plannercraft_render::geometry::task_phases`).
#[must_use]
pub fn phase_colors(p: &plannercraft_core::Project, style: &PlanStyle) -> PhaseColors {
    let fills: Vec<Rgb> = style.phases.values().map(|c| c.fill).collect();
    plannercraft_render::geometry::task_phases(p, &style.phases)
        .into_iter()
        .zip(p.tasks.iter())
        .filter_map(|(phase, task)| {
            let (rank, own) = phase?;
            let fill = fills.get(usize::from(rank)).copied()?;
            Some((task.uid(), (fill, own)))
        })
        .collect()
}

impl Default for ScreenStyle {
    /// Classique clair (J7).
    fn default() -> Self {
        Self {
            style: Arc::new(template(TemplateId::Classique)),
            variant: Variant::Light,
            phases: Arc::new(Vec::new()),
            phase_colors: Arc::new(PhaseColors::new()),
        }
    }
}

impl ScreenStyle {
    /// Hauteur d'une ligne (densité du style : 18, 22 ou 28 px).
    #[must_use]
    pub fn row_h(&self) -> f32 {
        f32::from(self.style.density.row_px())
    }

    /// Le style demande la police Inter (table et Gantt).
    #[must_use]
    pub fn inter(&self) -> bool {
        self.style.font.family == FontChoice::Inter
    }
}

/// Cache du style effectif et de ses palettes (J8, lot L11) : relu
/// seulement quand le document (ouverture, génération) ou le modèle par
/// défaut changent ; la palette du thème courant est servie sans calcul.
#[derive(Clone, Debug)]
pub struct StyleCache {
    /// Clé du style gardé : (ouverture, génération) du document, modèle
    /// par défaut.
    key: Option<(Option<(u64, u64)>, TemplateId)>,
    /// Style gardé.
    style: Arc<PlanStyle>,
    /// Palettes claire et sombre du style gardé.
    palettes: Option<(Palette, Palette)>,
    /// Phases gardées et leur clé (document).
    phases: (Option<(u64, u64)>, Arc<Vec<u8>>),
    /// Couleurs de phase gardées et leur clé (document, H-UI-65).
    phase_colors: (Option<(u64, u64)>, Arc<PhaseColors>),
    /// Lectures faites (tests).
    builds: u64,
}

impl Default for StyleCache {
    fn default() -> Self {
        Self {
            key: None,
            style: ScreenStyle::default().style,
            palettes: None,
            phases: (None, Arc::new(Vec::new())),
            phase_colors: (None, Arc::new(PhaseColors::new())),
            builds: 0,
        }
    }
}

impl StyleCache {
    /// Style et palette pour la clé `(document, défaut)` et le thème
    /// `theme` ; `resolve` n'est appelé que si la clé change.
    pub fn screen(
        &mut self,
        doc_key: Option<(u64, u64)>,
        default: TemplateId,
        theme: egui::Theme,
        resolve: impl FnOnce() -> PlanStyle,
    ) -> (ScreenStyle, Palette) {
        let key = (doc_key, default);
        if self.key != Some(key) || self.palettes.is_none() {
            let style = resolve();
            self.palettes = Some((
                Palette::from_style(&style, Variant::Light),
                Palette::from_style(&style, Variant::Dark),
            ));
            self.style = Arc::new(style);
            self.key = Some(key);
            self.builds = self.builds.saturating_add(1);
        }
        let (light, dark) = self.palettes.unwrap_or((Palette::LIGHT, Palette::DARK));
        let palette = match theme {
            egui::Theme::Light => light,
            egui::Theme::Dark => dark,
        };
        let screen = ScreenStyle {
            style: Arc::clone(&self.style),
            variant: variant_for(theme),
            phases: Arc::new(Vec::new()),
            phase_colors: Arc::new(PhaseColors::new()),
        };
        (screen, palette)
    }

    /// Couleurs de phase du document `doc_key` (H-UI-65), calculées par
    /// `compute` seulement si le document a changé.
    pub fn phase_colors(
        &mut self,
        doc_key: Option<(u64, u64)>,
        compute: impl FnOnce() -> PhaseColors,
    ) -> Arc<PhaseColors> {
        if self.phase_colors.0 != doc_key || doc_key.is_none() {
            self.phase_colors = (doc_key, Arc::new(compute()));
        }
        Arc::clone(&self.phase_colors.1)
    }

    /// Phases des lignes du document `doc_key`, calculées par `compute`
    /// seulement si le document a changé.
    pub fn phases(&mut self, doc_key: Option<(u64, u64)>, compute: impl FnOnce() -> Vec<u8>) -> Arc<Vec<u8>> {
        if self.phases.0 != doc_key || doc_key.is_none() {
            self.phases = (doc_key, Arc::new(compute()));
        }
        Arc::clone(&self.phases.1)
    }

    /// Résolutions faites depuis la création (tests du cache).
    #[must_use]
    pub const fn builds(&self) -> u64 {
        self.builds
    }

    /// Style gardé (hauteur de ligne de la navigation entre deux images).
    #[must_use]
    pub fn style(&self) -> &PlanStyle {
        &self.style
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use egui::Color32;

    use plannercraft_render::style::{TemplateId, Variant, template};

    use super::{Palette, Rgb};

    /// Rapport de contraste WCAG en millièmes, par la fonction entière
    /// partagée avec le rendu (`plannercraft_render::style::contrast_ratio`).
    fn contrast(a: Color32, b: Color32) -> u32 {
        plannercraft_render::style::contrast_ratio(Rgb(a.r(), a.g(), a.b()), Rgb(b.r(), b.g(), b.b()))
    }

    fn luminance(c: Color32) -> u32 {
        plannercraft_render::style::luminance_ppm(Rgb(c.r(), c.g(), c.b()))
    }

    fn palettes() -> [(&'static str, Palette); 2] {
        [("clair", Palette::LIGHT), ("sombre", Palette::DARK)]
    }

    #[test]
    fn palette_contrast_helper_sane() {
        assert_eq!(contrast(Color32::BLACK, Color32::WHITE), 21_000);
        assert_eq!(contrast(Color32::WHITE, Color32::WHITE), 1000);
    }

    #[test]
    fn palette_text_contrast() {
        for (name, p) in palettes() {
            for (label, bg) in [
                ("fond", p.background),
                ("en-tête", p.header_bg),
                ("sélection", p.selection),
            ] {
                for (text_label, text) in [("texte", p.text), ("texte fort", p.text_strong)] {
                    let ratio = contrast(text, bg);
                    assert!(ratio >= 4500, "{name} : {text_label} sur {label} = {ratio}‰");
                }
            }
            let weak = contrast(p.text_weak, p.background);
            assert!(weak >= 3000, "{name} : texte atténué = {weak}‰");
        }
    }

    /// Bandeau de messages (D-fix-9) : texte fort et texte courant lisibles
    /// (WCAG AA, 4,5:1) sur les fonds teintés d'erreur et d'avertissement,
    /// dans les deux thèmes ; le liseré critique se distingue du fond.
    #[test]
    fn palette_message_banner_contrast() {
        for (name, p) in palettes() {
            for (label, bg) in [
                ("erreur", p.message_error_bg),
                ("avertissement", p.message_warning_bg),
            ] {
                for (text_label, text) in [("texte fort", p.text_strong), ("texte", p.text)] {
                    let ratio = contrast(text, bg);
                    assert!(
                        ratio >= 4500,
                        "{name} : {text_label} sur bandeau {label} = {ratio}‰"
                    );
                }
                assert_ne!(bg, p.background, "{name} : bandeau {label} non teinté");
            }
            let accent = contrast(p.critical, p.message_error_bg);
            assert!(accent >= 3000, "{name} : liseré critique sur bandeau = {accent}‰");
            // Erreur rougeâtre, avertissement jaunâtre.
            assert!(p.message_error_bg.r() > p.message_error_bg.b(), "{name}");
            assert!(p.message_warning_bg.g() > p.message_warning_bg.b(), "{name}");
        }
    }

    #[test]
    fn palette_graphics_contrast() {
        for (name, p) in palettes() {
            for (label, color) in [
                ("barre", p.bar),
                ("critique", p.critical),
                ("jalon", p.milestone),
                ("récapitulative", p.summary),
                ("récapitulative critique", p.summary_critical),
                ("lien", p.link),
                ("date d'état", p.status_date),
            ] {
                let ratio = contrast(color, p.background);
                assert!(ratio >= 3000, "{name} : {label} sur fond = {ratio}‰");
            }
        }
    }

    /// J7 L1 : cadre du curseur lisible sur le fond, le fond alterné de la
    /// table et la ligne sélectionnée (≥ 3:1, WCAG 1.4.11).
    #[test]
    fn palette_cursor_contrast() {
        for (name, p) in palettes() {
            let zebra = p.background.lerp_to_gamma(p.header_bg, 0.6);
            for (label, bg) in [
                ("fond", p.background),
                ("fond alterné", zebra),
                ("sélection", p.selection),
            ] {
                let ratio = contrast(p.cursor, bg);
                assert!(ratio >= 3000, "{name} : curseur sur {label} = {ratio}‰");
            }
        }
    }

    /// J7 L7 (H-UI-22, N3) : ligne de base lisible sur le fond et sur les
    /// jours chômés ; bande d'avancement lisible dans la barre (courante et
    /// critique) ; barres, jalons et liens lisibles sur un jour chômé ;
    /// jours chômés distincts du fond ; ligne de base distincte de la barre.
    #[test]
    fn palette_tracking_and_non_working_contrast() {
        for (name, p) in palettes() {
            assert_ne!(p.non_working, p.background, "{name}");
            for (label, bg) in [("fond", p.background), ("jour chômé", p.non_working)] {
                let ratio = contrast(p.baseline, bg);
                assert!(ratio >= 3000, "{name} : ligne de base sur {label} = {ratio}‰");
            }
            for (label, bar) in [("barre", p.bar), ("barre critique", p.critical)] {
                let ratio = contrast(p.progress, bar);
                assert!(ratio >= 3000, "{name} : avancement dans la {label} = {ratio}‰");
            }
            for (label, color) in [
                ("barre", p.bar),
                ("critique", p.critical),
                ("jalon", p.milestone),
                ("récapitulative", p.summary),
                ("lien", p.link),
                ("date d'état", p.status_date),
            ] {
                let ratio = contrast(color, p.non_working);
                assert!(ratio >= 3000, "{name} : {label} sur jour chômé = {ratio}‰");
            }
            assert_ne!(p.baseline, p.bar, "{name}");
            // Ligne de base grise : aucune composante dominante nette.
            let spread = p.baseline.r().abs_diff(p.baseline.b());
            assert!(spread < 0x20, "{name} : ligne de base teintée");
        }
    }

    #[test]
    fn palette_critical_distinct_from_bar() {
        for (name, p) in palettes() {
            assert_ne!(p.critical, p.bar, "{name}");
            // Rouge dominant pour le critique, bleu dominant pour la barre.
            assert!(p.critical.r() > p.critical.b(), "{name}");
            assert!(p.bar.b() > p.bar.r(), "{name}");
            assert_ne!(p.summary_critical, p.summary, "{name}");
        }
    }

    /// Le modèle Classique reproduit J7 champ par champ (ADR-0016 D6), dans
    /// les deux thèmes ; l'impression part du clair.
    #[test]
    fn classique_equals_j7_palette() {
        let style = template(TemplateId::Classique);
        assert_eq!(Palette::from_style(&style, Variant::Light), Palette::LIGHT);
        assert_eq!(Palette::from_style(&style, Variant::Dark), Palette::DARK);
        assert_eq!(Palette::from_style(&style, Variant::Print), Palette::LIGHT);
    }

    /// Le style n'est relu que si le document ou le modèle par défaut
    /// changent ; le thème ne fait que choisir la palette.
    #[test]
    fn style_cache_rebuilds_on_key_only() {
        let mut cache = super::StyleCache::default();
        let sobre = || template(TemplateId::Sobre);
        let (a, light) = cache.screen(Some((1, 0)), TemplateId::Sobre, egui::Theme::Light, sobre);
        let (b, dark) = cache.screen(Some((1, 0)), TemplateId::Sobre, egui::Theme::Dark, || {
            panic!("relu")
        });
        assert_eq!(cache.builds(), 1);
        assert_eq!(a.style, b.style);
        assert_eq!(
            light,
            Palette::from_style(&template(TemplateId::Sobre), Variant::Light)
        );
        assert_eq!(
            dark,
            Palette::from_style(&template(TemplateId::Sobre), Variant::Dark)
        );
        assert_eq!(b.variant, Variant::Dark);
        cache.screen(Some((1, 1)), TemplateId::Sobre, egui::Theme::Dark, sobre);
        cache.screen(Some((1, 1)), TemplateId::Classique, egui::Theme::Dark, sobre);
        assert_eq!(cache.builds(), 3);
        let (classique, light) =
            super::StyleCache::default().screen(None, TemplateId::Classique, egui::Theme::Light, || {
                template(TemplateId::Classique)
            });
        assert_eq!(light, Palette::LIGHT);
        assert_eq!(classique, super::ScreenStyle::default());
        assert!((classique.row_h() - 22.0).abs() < f32::EPSILON);
        assert!(!classique.inter());
    }

    #[test]
    fn palette_light_dark_differ() {
        assert_ne!(Palette::LIGHT, Palette::DARK);
        assert!(luminance(Palette::LIGHT.background) > luminance(Palette::DARK.background));
        assert_eq!(Palette::for_theme(egui::Theme::Light), Palette::LIGHT);
        assert_eq!(Palette::for_theme(egui::Theme::Dark), Palette::DARK);
    }

    /// H-UI-103 (U09) : pour chaque modèle intégré et chaque thème, teinte
    /// du bloc ≥ 1,5:1 sur le fond et ≥ 1,3:1 sur le zébrage et les deux
    /// bandes ; bordure ≥ 3:1 sur le fond, les bandes et la teinte ; texte
    /// et texte appuyé lisibles (seuil du modèle) sur la teinte, l'ID
    /// appuyé et le survol ; ID appuyé distinct de l'en-tête et du fond ;
    /// barres, jalons et liens ≥ 3:1 sur les lignes sélectionnées du Gantt ;
    /// cadre de la cellule active ≥ 3:1 sur la teinte.
    #[test]
    fn h_ui_103_selection_contrast() {
        for id in TemplateId::ALL {
            let style = template(id);
            let min = plannercraft_render::style::min_text_contrast(&style);
            for variant in [Variant::Light, Variant::Dark] {
                let c = style.tokens(variant);
                let p = Palette::from_style(&style, variant);
                let rgb = |v: Rgb| Color32::from_rgb(v.0, v.1, v.2);
                let zebra = p.background.lerp_to_gamma(p.header_bg, 0.6);
                let name = format!("{id:?} {variant:?}");
                let on_bg = contrast(p.selection, p.background);
                assert!(
                    on_bg >= super::SELECTION_ON_BG,
                    "{name} : teinte sur fond = {on_bg}‰"
                );
                for (label, back) in [
                    ("zébrage", zebra),
                    ("bande 0", rgb(c.band[0])),
                    ("bande 1", rgb(c.band[1])),
                ] {
                    let ratio = contrast(p.selection, back);
                    assert!(
                        ratio >= super::SELECTION_ON_BAND,
                        "{name} : teinte sur {label} = {ratio}‰"
                    );
                    let border = contrast(p.selection_border, back);
                    assert!(
                        border >= super::BORDER_CONTRAST,
                        "{name} : bordure sur {label} = {border}‰"
                    );
                }
                for (label, back) in [("fond", p.background), ("teinte", p.selection)] {
                    let border = contrast(p.selection_border, back);
                    assert!(
                        border >= super::BORDER_CONTRAST,
                        "{name} : bordure sur {label} = {border}‰"
                    );
                    let frame = contrast(p.cursor, back);
                    assert!(
                        frame >= super::BORDER_CONTRAST,
                        "{name} : cadre actif sur {label} = {frame}‰"
                    );
                }
                for (label, back) in [
                    ("teinte", p.selection),
                    ("ID appuyé", p.header_selected),
                    ("survol", p.hover_row),
                ] {
                    for ink in [p.text, p.text_strong] {
                        let ratio = contrast(ink, back);
                        assert!(ratio >= min, "{name} : texte sur {label} = {ratio}‰");
                    }
                }
                assert!(
                    contrast(p.header_selected, p.header_bg) >= 1200,
                    "{name} : ID appuyé"
                );
                assert!(
                    contrast(p.header_selected, p.background) >= 1200,
                    "{name} : Gantt"
                );
                assert_ne!(p.hover_row, p.background, "{name} : survol");
                for (label, color) in [
                    ("barre", p.bar),
                    ("critique", p.critical),
                    ("jalon", p.milestone),
                    ("récapitulative", p.summary),
                    ("lien", p.link),
                ] {
                    let ratio = contrast(color, p.header_selected);
                    assert!(
                        ratio >= 3000,
                        "{name} : {label} sur ligne sélectionnée du Gantt = {ratio}‰"
                    );
                }
            }
        }
    }

    /// Les jetons de sélection de Classique sont ceux des palettes J7
    /// écrites en dur (relues par `classique_equals_j7_palette`), et un
    /// style à la sélection déjà marquée la garde telle quelle.
    #[test]
    fn selection_tokens_keep_strong_style_tint() {
        let mut c = template(TemplateId::Classique).tokens(Variant::Light).clone();
        c.selection = Rgb(0x9C, 0xBA, 0xE3);
        let derived = super::SelectionTokens::derive(&c, 4500);
        assert_eq!(derived.selection, c.selection);
        assert_eq!(derived.border, c.cursor);
    }

    /// H-UI-104 (lot L10) : notifications lisibles dans chaque modèle et
    /// chaque variante (et les palettes sans document) : texte ≥ 4,5:1 sur
    /// le fond de la carte, liserés d'information, d'avertissement et de
    /// refus ≥ 3:1 et distincts deux à deux ; fond et texte opaques ; ombre
    /// translucide.
    #[test]
    fn h_ui_104_toast_contrast() {
        let mut cases: Vec<(String, Palette)> = palettes()
            .into_iter()
            .map(|(name, p)| (name.to_owned(), p))
            .collect();
        for id in TemplateId::ALL {
            let style = template(id);
            for variant in [Variant::Light, Variant::Dark] {
                cases.push((
                    format!("{id:?} {variant:?}"),
                    Palette::from_style(&style, variant),
                ));
            }
        }
        assert_eq!(cases.len(), 2 + 2 * TemplateId::ALL.len());
        for (name, p) in cases {
            let t = super::ToastTokens::of(&p);
            assert_eq!((t.bg.a(), t.text.a(), t.border.a()), (255, 255, 255), "{name}");
            let text = contrast(t.text, t.bg);
            assert!(
                text >= super::TOAST_TEXT_CONTRAST,
                "{name} : texte sur notification = {text}‰"
            );
            for (label, accent) in [("info", t.info), ("avertissement", t.warning), ("refus", t.error)] {
                assert_eq!(accent.a(), 255, "{name} {label}");
                let ratio = contrast(accent, t.bg);
                assert!(
                    ratio >= super::TOAST_ACCENT_CONTRAST,
                    "{name} : liseré {label} = {ratio}‰"
                );
            }
            assert_ne!(t.info, t.warning, "{name}");
            assert_ne!(t.warning, t.error, "{name}");
            assert!(t.shadow.a() > 0 && t.shadow.a() < 255, "{name}");
        }
    }

    #[test]
    fn palette_opaque() {
        for (name, p) in palettes() {
            for color in [
                p.background,
                p.text,
                p.text_strong,
                p.text_weak,
                p.grid,
                p.grid_major,
                p.header_bg,
                p.selection,
                p.bar,
                p.bar_outline,
                p.critical,
                p.milestone,
                p.summary,
                p.link,
                p.status_date,
                p.splitter,
                p.non_working,
                p.summary_critical,
                p.message_error_bg,
                p.message_warning_bg,
                p.cursor,
                p.baseline,
                p.progress,
                p.selection_border,
                p.header_selected,
                p.hover_row,
            ] {
                assert_eq!(color.a(), 255, "{name} : {color:?}");
            }
        }
    }
}
