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
        selection: Color32::from_rgb(0xD3, 0xB3, 0xE5),
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
        cursor: Color32::from_rgb(0x77, 0x19, 0xAA),
        baseline: Color32::from_rgb(0x80, 0x86, 0x8F),
        progress: Color32::from_rgb(0x10, 0x14, 0x1A),
        selection_border: Color32::from_rgb(0x77, 0x19, 0xAA),
        header_selected: Color32::from_rgb(0xE7, 0xDC, 0xF0),
        hover_row: Color32::from_rgb(0xF3, 0xEE, 0xF8),
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
        selection: Color32::from_rgb(0x59, 0x47, 0x6A),
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
        cursor: Color32::from_rgb(0xD4, 0xAE, 0xF0),
        baseline: Color32::from_rgb(0x8E, 0x94, 0x9C),
        progress: Color32::from_rgb(0x0F, 0x23, 0x40),
        selection_border: Color32::from_rgb(0xD4, 0xAE, 0xF0),
        header_selected: Color32::from_rgb(0x37, 0x33, 0x41),
        hover_row: Color32::from_rgb(0x2B, 0x29, 0x32),
    };

    /// Palette tirée d'un style du planning (ADR-0016 D5) pour une variante.
    ///
    /// Le séparateur table / Gantt prend la grille marquée (égaux en J7). Un
    /// rôle de barre absent (style non validé) prend le texte courant.
    #[must_use]
    pub fn from_style(style: &PlanStyle, variant: Variant) -> Self {
        // Finitions P4, seconde passe (H-UI-136) : modèle à la sélection
        // bleue d'origine ⇒ sélection et curseur à l'accent de l'interface,
        // à l'écran seulement (le style du document n'est pas touché).
        let accented = accent_selection_tokens(style, variant);
        let c = accented.as_ref().unwrap_or_else(|| style.tokens(variant));
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

/// Violet de la marque (ADR-0021, `#7719AA`) : bloc du symbole de
/// l'application et accent du ruban en thème clair.
pub const BRAND_VIOLET: Color32 = Color32::from_rgb(0x77, 0x19, 0xAA);

/// Contraste exigé des textes de l'habillage sur leur fond (H-UI-132, WCAG
/// AA, millièmes).
pub const CHROME_TEXT_CONTRAST: u32 = 4500;

/// Contraste exigé des icônes, traits d'accent et du grisé de l'habillage
/// sur leur fond (H-UI-132, WCAG 1.4.11, millièmes).
pub const CHROME_GRAPHIC_CONTRAST: u32 = 3000;

/// Jetons de l'habillage : ruban, barre de menus, chronologie à l'écran
/// (finitions P4, ADR-0022 D6, H-UI-132 à H-UI-134).
///
/// Propres au thème de l'interface, **jamais** au style du document : les
/// couleurs du Gantt, de la table et de l'impression restent celles du
/// modèle de mise en forme. Le violet de la marque n'est qu'un accent
/// (onglet actif, bascules enfoncées, chronologie, focus) ; aucun dégradé,
/// aucune ombre. Contrastes testés par `h_ui_132_chrome_contrast`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChromeTokens {
    /// Fond du ruban et de la barre de menus (fond de la fenêtre, pas de
    /// bloc gris).
    pub surface: Color32,
    /// Filet bas du ruban, séparateurs de groupes et de la barre d'accès
    /// rapide, filet bas de la chronologie.
    pub rule: Color32,
    /// Libellés et icônes.
    pub text: Color32,
    /// Nom des groupes (gris moyen, ≥ 4,5:1).
    pub text_weak: Color32,
    /// Libellé et icône d'un élément grisé (lisible, ≥ 3:1, opaque).
    pub disabled: Color32,
    /// Accent de la marque : onglet actif et son trait, icône d'une bascule
    /// enfoncée, anneau de focus, chronologie.
    pub accent: Color32,
    /// Fond d'un élément ou d'un onglet survolé.
    pub hover: Color32,
    /// Fond d'un élément appuyé ou dont la liste est ouverte.
    pub pressed: Color32,
    /// Fond d'une bascule enfoncée (violet très clair).
    pub toggle_bg: Color32,
    /// Fond de la chronologie (neutre).
    pub timeline_bg: Color32,
    /// Remplissage des barres de la chronologie (teinte légère de
    /// l'accent, contour à l'accent).
    pub timeline_bar: Color32,
    /// Texte dans une barre de la chronologie.
    pub timeline_bar_text: Color32,
    /// Remplissage du cadre de la partie visible du Gantt (translucide).
    pub window_fill: Color32,
    /// Contour du cadre de la partie visible du Gantt (translucide).
    pub window_stroke: Color32,
    /// Fond du texte sélectionné et des éléments choisis d'egui (zones de
    /// saisie, listes).
    pub selection_bg: Color32,
    /// Trait et texte des éléments choisis d'egui, contour de focus des
    /// zones de saisie.
    pub selection_fg: Color32,
    /// Liseré d'un pixel autour du symbole de l'application : la surface en
    /// clair (invisible), un gris en sombre, où le carré noir se perdrait.
    pub logo_ring: Color32,
    /// Cadre de la cellule active et bordure du bloc dans la table et le
    /// Gantt des modèles à sélection bleue d'origine (H-UI-136) : l'accent
    /// en clair, un violet plus clair en sombre (≥ 3:1 sur la teinte).
    pub table_cursor: Color32,
}

impl ChromeTokens {
    /// Thème clair.
    pub const LIGHT: Self = Self {
        surface: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        rule: Color32::from_rgb(0xE2, 0xE1, 0xE6),
        text: Color32::from_rgb(0x1F, 0x1F, 0x23),
        text_weak: Color32::from_rgb(0x6C, 0x6B, 0x74),
        disabled: Color32::from_rgb(0x7A, 0x79, 0x82),
        accent: BRAND_VIOLET,
        hover: Color32::from_rgb(0xF2, 0xF0, 0xF5),
        pressed: Color32::from_rgb(0xE7, 0xE2, 0xED),
        toggle_bg: Color32::from_rgb(0xF1, 0xE6, 0xF8),
        timeline_bg: Color32::from_rgb(0xF8, 0xF8, 0xFA),
        timeline_bar: Color32::from_rgb(0xEB, 0xDD, 0xF5),
        timeline_bar_text: Color32::from_rgb(0x1F, 0x1F, 0x23),
        window_fill: Color32::from_rgba_unmultiplied_const(0x77, 0x19, 0xAA, 0x06),
        window_stroke: Color32::from_rgba_unmultiplied_const(0x77, 0x19, 0xAA, 0x99),
        selection_bg: Color32::from_rgb(0xE6, 0xD5, 0xF3),
        selection_fg: BRAND_VIOLET,
        logo_ring: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        table_cursor: BRAND_VIOLET,
    };

    /// Thème sombre.
    pub const DARK: Self = Self {
        surface: Color32::from_rgb(0x1E, 0x1F, 0x22),
        rule: Color32::from_rgb(0x36, 0x37, 0x3D),
        text: Color32::from_rgb(0xE6, 0xE6, 0xEA),
        text_weak: Color32::from_rgb(0x9E, 0x9D, 0xA7),
        disabled: Color32::from_rgb(0x7E, 0x7E, 0x87),
        accent: Color32::from_rgb(0xB5, 0x7B, 0xE0),
        hover: Color32::from_rgb(0x2A, 0x2B, 0x30),
        pressed: Color32::from_rgb(0x34, 0x35, 0x3B),
        toggle_bg: Color32::from_rgb(0x3A, 0x2D, 0x48),
        timeline_bg: Color32::from_rgb(0x23, 0x24, 0x28),
        timeline_bar: Color32::from_rgb(0x3F, 0x2D, 0x54),
        timeline_bar_text: Color32::from_rgb(0xE6, 0xE6, 0xEA),
        window_fill: Color32::from_rgba_unmultiplied_const(0xB5, 0x7B, 0xE0, 0x0A),
        window_stroke: Color32::from_rgba_unmultiplied_const(0xB5, 0x7B, 0xE0, 0x99),
        selection_bg: Color32::from_rgb(0x4A, 0x30, 0x63),
        selection_fg: Color32::from_rgb(0xD4, 0xAE, 0xF0),
        logo_ring: Color32::from_rgb(0x4C, 0x4D, 0x55),
        table_cursor: Color32::from_rgb(0xD4, 0xAE, 0xF0),
    };

    /// Jetons du thème résolu.
    #[must_use]
    pub const fn for_theme(theme: egui::Theme) -> Self {
        match theme {
            egui::Theme::Light => Self::LIGHT,
            egui::Theme::Dark => Self::DARK,
        }
    }

    /// Pose les couleurs de sélection d'egui (texte sélectionné, éléments
    /// choisis, focus des zones de saisie) des deux thèmes de `ctx` sur
    /// l'accent (H-UI-132) ; le reste des visuels d'egui est inchangé.
    pub fn install(ctx: &egui::Context) {
        for theme in [egui::Theme::Light, egui::Theme::Dark] {
            let t = Self::for_theme(theme);
            ctx.style_mut_of(theme, |style| {
                style.visuals.selection.bg_fill = t.selection_bg;
                style.visuals.selection.stroke = egui::Stroke::new(1.0, t.selection_fg);
            });
        }
    }
}

/// Jetons de la conversation de l'assistant (lot A1b d'ADR-0024, cahier
/// `docs/plans/assistant-chat.md` § 1, H-UI-143) : propres au thème de
/// l'interface, jamais au style du document. Textes ≥ 4,5:1 sur leur fond,
/// icônes d'état ≥ 3:1 (`h_ui_143_chat_contrast`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChatTokens {
    /// Fond du volet (celui d'egui : `Visuals::panel_fill`).
    pub panel: Color32,
    /// Texte des messages.
    pub text: Color32,
    /// Texte appuyé (gras du Markdown : egui l'affiche par la couleur).
    pub strong: Color32,
    /// Méta : pied de message, lignes d'outils, barre d'état, réflexion.
    pub meta: Color32,
    /// Fond du message de l'utilisateur (accent à ~9 % en clair, ~16 % en
    /// sombre, rendu opaque sur le volet).
    pub user_bg: Color32,
    /// Filet gauche du bloc réflexion déplié.
    pub rule: Color32,
    /// Lecture réussie (✓).
    pub ok: Color32,
    /// Lecture en échec (⚠).
    pub warn: Color32,
    /// Fond de l'encadré d'erreur.
    pub error_bg: Color32,
    /// Contour de l'encadré d'erreur.
    pub error_border: Color32,
    /// Texte de l'encadré d'erreur.
    pub error_text: Color32,
    /// Fond de la pastille « ↓ Suite de la réponse », des suggestions et de
    /// la saisie.
    pub surface: Color32,
    /// Contour de la pastille, des suggestions et de la saisie.
    pub border: Color32,
    /// Bouton Envoyer / Arrêter (accent).
    pub send_bg: Color32,
    /// Icône du bouton Envoyer / Arrêter.
    pub send_fg: Color32,
    /// Bouton Envoyer grisé.
    pub send_disabled_bg: Color32,
    /// Icône du bouton Envoyer grisé.
    pub send_disabled_fg: Color32,
    /// Accent (icône de l'en-tête, focus).
    pub accent: Color32,
}

impl ChatTokens {
    /// Thème clair.
    pub const LIGHT: Self = Self {
        panel: Color32::from_rgb(0xF8, 0xF8, 0xF8),
        text: Color32::from_rgb(0x30, 0x30, 0x36),
        strong: Color32::from_rgb(0x00, 0x00, 0x00),
        meta: Color32::from_rgb(0x63, 0x62, 0x6B),
        user_bg: Color32::from_rgb(0xEC, 0xE4, 0xF1),
        rule: Color32::from_rgb(0xD6, 0xD0, 0xDE),
        ok: Color32::from_rgb(0x2E, 0x7D, 0x32),
        warn: Color32::from_rgb(0xA8, 0x4B, 0x00),
        error_bg: Color32::from_rgb(0xFC, 0xEE, 0xED),
        error_border: Color32::from_rgb(0xE3, 0xA5, 0xA0),
        error_text: Color32::from_rgb(0x8A, 0x1C, 0x1C),
        surface: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        border: Color32::from_rgb(0xDD, 0xDB, 0xE2),
        send_bg: BRAND_VIOLET,
        send_fg: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        send_disabled_bg: Color32::from_rgb(0xE4, 0xE2, 0xE8),
        send_disabled_fg: Color32::from_rgb(0x8E, 0x8C, 0x96),
        accent: BRAND_VIOLET,
    };

    /// Thème sombre.
    pub const DARK: Self = Self {
        panel: Color32::from_rgb(0x1B, 0x1B, 0x1B),
        text: Color32::from_rgb(0xD4, 0xD4, 0xDA),
        strong: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        meta: Color32::from_rgb(0xA3, 0xA2, 0xAC),
        user_bg: Color32::from_rgb(0x34, 0x2A, 0x3B),
        rule: Color32::from_rgb(0x45, 0x40, 0x4D),
        ok: Color32::from_rgb(0x6F, 0xCF, 0x7F),
        warn: Color32::from_rgb(0xF0, 0xB3, 0x5A),
        error_bg: Color32::from_rgb(0x38, 0x20, 0x20),
        error_border: Color32::from_rgb(0x8C, 0x45, 0x45),
        error_text: Color32::from_rgb(0xF6, 0xBE, 0xBE),
        surface: Color32::from_rgb(0x24, 0x25, 0x29),
        border: Color32::from_rgb(0x3D, 0x3E, 0x45),
        send_bg: Color32::from_rgb(0xB5, 0x7B, 0xE0),
        send_fg: Color32::from_rgb(0x1B, 0x1B, 0x1B),
        send_disabled_bg: Color32::from_rgb(0x34, 0x35, 0x3B),
        send_disabled_fg: Color32::from_rgb(0x7E, 0x7E, 0x87),
        accent: Color32::from_rgb(0xB5, 0x7B, 0xE0),
    };

    /// Jetons du thème résolu.
    #[must_use]
    pub const fn for_theme(theme: egui::Theme) -> Self {
        match theme {
            egui::Theme::Light => Self::LIGHT,
            egui::Theme::Dark => Self::DARK,
        }
    }
}

/// Symbole de l'application dessiné dans la barre d'accès rapide
/// (H-UI-132) : même composition que l'icône (ADR-0021, `xtask/src/icon.rs`)
/// sur une grille de 16 cellules, en formes egui (aucune image) : carré
/// arrondi noir (rayon 185/824 du côté), carré blanc aux cellules 2..5 ×
/// 11..14, bloc violet aux cellules 6..14 × 2..14. Formes vides si `rect`
/// n'est pas fini ou est vide.
#[must_use]
pub fn logo_shapes(rect: egui::Rect) -> Vec<egui::Shape> {
    let side = rect.width().min(rect.height());
    if !rect.is_finite() || !side.is_finite() || side <= 0.0 {
        return Vec::new();
    }
    let left = rect.center().x - side / 2.0;
    let top = rect.center().y - side / 2.0;
    let cell = side / 16.0;
    let cells = |x0: f32, y0: f32, x1: f32, y1: f32| {
        egui::Rect::from_min_max(
            egui::pos2(left + x0 * cell, top + y0 * cell),
            egui::pos2(left + x1 * cell, top + y1 * cell),
        )
    };
    vec![
        egui::Shape::rect_filled(cells(0.0, 0.0, 16.0, 16.0), side * 185.0 / 824.0, Color32::BLACK),
        egui::Shape::rect_filled(cells(2.0, 11.0, 5.0, 14.0), 0.0, Color32::WHITE),
        egui::Shape::rect_filled(cells(6.0, 2.0, 14.0, 14.0), 0.0, BRAND_VIOLET),
    ]
}

/// Modèles dont la sélection d'origine est le bleu par défaut (H-UI-136) :
/// leur sélection et leur curseur passent à l'accent de l'interface.
pub const BLUE_SELECTION_TEMPLATES: [TemplateId; 2] = [TemplateId::Classique, TemplateId::Sobre];

/// Jetons de couleur de `style` pour `variant` avec la sélection et le
/// curseur de l'interface (H-UI-136), si le style garde ceux, bleus, d'un
/// modèle de [`BLUE_SELECTION_TEMPLATES`] (même `selection` et même
/// `cursor` que ce modèle dans la variante) ; `None` sinon : modèle à
/// sélection propre (Contraste élevé, Présentation, Planning travaux,
/// Impression N&B), couleur choisie par l'utilisateur, ou variante
/// d'impression (rien ne change à l'impression).
///
/// Graine de la teinte : la bascule de l'habillage (violet très clair),
/// renforcée par [`SelectionTokens::derive`] vers le curseur jusqu'aux
/// seuils de H-UI-103 ; curseur et bordure du bloc :
/// [`ChromeTokens::table_cursor`].
#[must_use]
pub fn accent_selection_tokens(style: &PlanStyle, variant: Variant) -> Option<ColorTokens> {
    let chrome = match variant {
        Variant::Light => ChromeTokens::LIGHT,
        Variant::Dark => ChromeTokens::DARK,
        Variant::Print => return None,
    };
    // Les deux variantes doivent garder la sélection du modèle bleu :
    // Planning travaux partage la sélection sombre de Classique mais pas la
    // claire, il garde donc la sienne partout.
    let same = |t: &ColorTokens, c: &ColorTokens| t.selection == c.selection && t.cursor == c.cursor;
    let blue = BLUE_SELECTION_TEMPLATES.iter().any(|id| {
        let t = template(*id);
        [Variant::Light, Variant::Dark]
            .into_iter()
            .all(|v| same(t.tokens(v), style.tokens(v)))
    });
    let c = style.tokens(variant);
    if !blue {
        return None;
    }
    let rgb = |v: Color32| Rgb(v.r(), v.g(), v.b());
    let mut out = c.clone();
    out.selection = rgb(chrome.toggle_bg);
    out.cursor = rgb(chrome.table_cursor);
    Some(out)
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
    /// - ID appuyé : l'en-tête mêlé vers la teinte par pas de 1 % jusqu'à
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
        // Pas de 1 % (finitions P4, seconde passe) : le premier mélange qui
        // tient le seuil, sans le dépasser de tout un pas de 5 % (les barres
        // claires d'un modèle doivent rester ≥ 3:1 sur la ligne
        // sélectionnée du Gantt).
        let header_selected = (0..=100_u8)
            .map(|k| blend(c.header_bg, selection, k))
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
    /// Mise en forme du texte des tâches résolue pour la variante
    /// (finitions P2, H-UI-126), par UID ; vide sans mise en forme.
    /// Calculée une fois par style (cache), jamais pendant une image.
    pub text_looks: Arc<TextLooks>,
}

/// Apparence du texte des tâches mises en forme (H-UI-126) : UID → aspect
/// dans la variante de l'écran.
pub type TextLooks = std::collections::BTreeMap<u32, plannercraft_render::style::TextLook>;

/// Apparences du texte des tâches de `style` dans la variante `v`.
#[must_use]
pub fn text_looks(style: &PlanStyle, v: Variant) -> TextLooks {
    style
        .task_text
        .iter()
        .map(|(uid, t)| (*uid, plannercraft_render::style::task_text_look(style, v, t)))
        .collect()
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
            text_looks: Arc::new(TextLooks::new()),
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
    /// Apparences du texte des tâches, claire et sombre (finitions P2).
    looks: (Arc<TextLooks>, Arc<TextLooks>),
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
            looks: (Arc::new(TextLooks::new()), Arc::new(TextLooks::new())),
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
            self.looks = (
                Arc::new(text_looks(&style, Variant::Light)),
                Arc::new(text_looks(&style, Variant::Dark)),
            );
            self.style = Arc::new(style);
            self.key = Some(key);
            self.builds = self.builds.saturating_add(1);
        }
        let (light, dark) = self.palettes.unwrap_or((Palette::LIGHT, Palette::DARK));
        let palette = match theme {
            egui::Theme::Light => light,
            egui::Theme::Dark => dark,
        };
        let variant = variant_for(theme);
        let looks = if variant == Variant::Dark {
            &self.looks.1
        } else {
            &self.looks.0
        };
        let screen = ScreenStyle {
            style: Arc::clone(&self.style),
            variant,
            phases: Arc::new(Vec::new()),
            phase_colors: Arc::new(PhaseColors::new()),
            text_looks: Arc::clone(looks),
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
    /// les deux thèmes, sauf la sélection et le curseur passés à l'accent
    /// de l'interface (H-UI-136) ; la variante d'impression garde le bleu
    /// du modèle.
    #[test]
    fn classique_equals_j7_palette() {
        let style = template(TemplateId::Classique);
        assert_eq!(Palette::from_style(&style, Variant::Light), Palette::LIGHT);
        assert_eq!(Palette::from_style(&style, Variant::Dark), Palette::DARK);
        let print = Palette::from_style(&style, Variant::Print);
        assert_eq!(print.cursor, Color32::from_rgb(0x1F, 0x5F, 0xBF));
        assert_eq!(
            Palette {
                selection: Palette::LIGHT.selection,
                cursor: Palette::LIGHT.cursor,
                selection_border: Palette::LIGHT.selection_border,
                header_selected: Palette::LIGHT.header_selected,
                hover_row: Palette::LIGHT.hover_row,
                ..print
            },
            Palette::LIGHT
        );
    }

    /// H-UI-136 (finitions P4, seconde passe) : Classique et Sobre (sélection
    /// bleue d'origine) prennent la sélection et le curseur de l'interface
    /// à l'écran, clair et sombre ; Contraste élevé, Présentation, Planning
    /// travaux et Impression N&B gardent les leurs ; une sélection choisie
    /// dans le style la garde ; texte et texte appuyé ≥ 4,5:1 sur la
    /// teinte, l'ID appuyé et le survol ; style du document inchangé.
    #[test]
    fn h_ui_136_accent_selection_for_blue_templates() {
        use super::{BLUE_SELECTION_TEMPLATES, ChromeTokens, accent_selection_tokens};
        for id in TemplateId::ALL {
            let style = template(id);
            for (variant, chrome) in [
                (Variant::Light, ChromeTokens::LIGHT),
                (Variant::Dark, ChromeTokens::DARK),
            ] {
                let p = Palette::from_style(&style, variant);
                let c = style.tokens(variant);
                let name = format!("{id:?} {variant:?}");
                let accented = BLUE_SELECTION_TEMPLATES.contains(&id);
                assert_eq!(
                    accent_selection_tokens(&style, variant).is_some(),
                    accented,
                    "{name}"
                );
                if accented {
                    assert_eq!(p.cursor, chrome.table_cursor, "{name}");
                    assert_eq!(p.selection_border, chrome.table_cursor, "{name}");
                    // Teinte violette : bleu au-dessus du vert, rouge au moins autant.
                    for tint in [p.selection, p.header_selected, p.hover_row] {
                        assert!(tint.r() >= tint.g() && tint.b() > tint.g(), "{name} {tint:?}");
                    }
                    for back in [p.selection, p.header_selected, p.hover_row] {
                        for ink in [p.text, p.text_strong] {
                            let ratio = contrast(ink, back);
                            assert!(ratio >= 4500, "{name} : texte sur {back:?} = {ratio}‰");
                        }
                    }
                } else {
                    let rgb = |v: Rgb| Color32::from_rgb(v.0, v.1, v.2);
                    assert_eq!(p.cursor, rgb(c.cursor), "{name}");
                }
            }
            assert!(accent_selection_tokens(&style, Variant::Print).is_none());
        }
        // Sélection choisie par l'utilisateur dans le style : gardée.
        let mut custom = template(TemplateId::Classique);
        let mut light = custom.tokens(Variant::Light).clone();
        light.selection = Rgb(0xCC, 0xE8, 0xCC);
        custom.colors.light = light;
        assert!(accent_selection_tokens(&custom, Variant::Light).is_none());
        assert!(accent_selection_tokens(&custom, Variant::Dark).is_none());
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

    /// H-UI-132 (finitions P4) : jetons de l'habillage, clair et sombre.
    /// Textes (libellés, noms de groupes, onglet actif, texte des barres de
    /// la chronologie) ≥ 4,5:1 sur leurs fonds ; icônes, accent, grisé,
    /// filets d'accent ≥ 3:1 ; survol, appui et bascule distincts de la
    /// surface ; jetons opaques sauf le cadre de la partie visible ; accent
    /// clair = violet de la marque, accent sombre violet clair.
    #[test]
    fn h_ui_132_chrome_contrast() {
        use super::{CHROME_GRAPHIC_CONTRAST as GRAPHIC, CHROME_TEXT_CONTRAST as TEXT, ChromeTokens};
        assert_eq!(ChromeTokens::LIGHT.accent, Color32::from_rgb(0x77, 0x19, 0xAA));
        assert_eq!(ChromeTokens::DARK.accent, Color32::from_rgb(0xB5, 0x7B, 0xE0));
        for (name, t) in [("clair", ChromeTokens::LIGHT), ("sombre", ChromeTokens::DARK)] {
            let backs = [
                ("surface", t.surface),
                ("survol", t.hover),
                ("appui", t.pressed),
                ("bascule", t.toggle_bg),
            ];
            for (label, back) in backs {
                let ratio = contrast(t.text, back);
                assert!(ratio >= TEXT, "{name} : texte sur {label} = {ratio}‰");
                for (what, ink) in [("accent", t.accent), ("grisé", t.disabled)] {
                    let ratio = contrast(ink, back);
                    assert!(ratio >= GRAPHIC, "{name} : {what} sur {label} = {ratio}‰");
                }
                assert_ne!(back, t.disabled, "{name} {label}");
            }
            // Onglet actif (texte à l'accent) et nom des groupes.
            for (what, ink) in [("onglet actif", t.accent), ("nom de groupe", t.text_weak)] {
                for (label, back) in [("surface", t.surface), ("survol", t.hover)] {
                    let ratio = contrast(ink, back);
                    assert!(ratio >= TEXT, "{name} : {what} sur {label} = {ratio}‰");
                }
            }
            for (label, back) in [
                ("survol", t.hover),
                ("appui", t.pressed),
                ("bascule", t.toggle_bg),
            ] {
                assert_ne!(back, t.surface, "{name} : {label} = surface");
            }
            // Chronologie : texte des barres, contour (accent) des barres et
            // losanges, date d'état sur le fond.
            let ratio = contrast(t.timeline_bar_text, t.timeline_bar);
            assert!(ratio >= TEXT, "{name} : texte dans une barre = {ratio}‰");
            for (label, back) in [("fond", t.timeline_bg), ("barre", t.timeline_bar)] {
                let ratio = contrast(t.accent, back);
                assert!(
                    ratio >= GRAPHIC,
                    "{name} : accent sur {label} de la frise = {ratio}‰"
                );
            }
            for (label, ink) in [("texte", t.text), ("nom de groupe", t.text_weak)] {
                let ratio = contrast(ink, t.timeline_bg);
                assert!(ratio >= TEXT, "{name} : {label} sur la frise = {ratio}‰");
            }
            // Sélection d'egui : texte lisible dessus, trait visible.
            let ratio = contrast(t.text, t.selection_bg);
            assert!(ratio >= TEXT, "{name} : texte sélectionné = {ratio}‰");
            let ratio = contrast(t.selection_fg, t.selection_bg);
            assert!(ratio >= GRAPHIC, "{name} : choix sur sélection = {ratio}‰");
            // Filet visible sans être un trait fort.
            assert_ne!(t.rule, t.surface, "{name} : filet");
            for c in [
                t.surface,
                t.rule,
                t.text,
                t.text_weak,
                t.disabled,
                t.accent,
                t.hover,
                t.pressed,
                t.toggle_bg,
                t.timeline_bg,
                t.timeline_bar,
                t.timeline_bar_text,
                t.selection_bg,
                t.selection_fg,
                t.logo_ring,
            ] {
                assert_eq!(c.a(), 255, "{name} : {c:?}");
            }
            assert!(t.window_fill.a() > 0 && t.window_fill.a() < 32, "{name}");
            assert!(t.window_stroke.a() > 64 && t.window_stroke.a() < 255, "{name}");
        }
        assert_eq!(ChromeTokens::for_theme(egui::Theme::Dark), ChromeTokens::DARK);
    }

    /// H-UI-143 (lot A1b) : jetons de la conversation, clair et sombre.
    /// Textes (messages, méta, erreur, message de l'utilisateur) ≥ 4,5:1 sur
    /// leur fond ; icônes d'état, accent et bouton Envoyer ≥ 3:1 ; fond du
    /// volet = celui d'egui ; message de l'utilisateur distinct du volet.
    #[test]
    fn h_ui_143_chat_contrast() {
        use super::{CHROME_GRAPHIC_CONTRAST as GRAPHIC, CHROME_TEXT_CONTRAST as TEXT, ChatTokens};
        for (name, t, visuals) in [
            ("clair", ChatTokens::LIGHT, egui::Visuals::light()),
            ("sombre", ChatTokens::DARK, egui::Visuals::dark()),
        ] {
            assert_eq!(t.panel, visuals.panel_fill, "{name} : fond du volet");
            for (what, ink, back) in [
                ("texte", t.text, t.panel),
                ("texte appuyé", t.strong, t.panel),
                ("méta", t.meta, t.panel),
                ("texte sur message utilisateur", t.text, t.user_bg),
                ("méta sur message utilisateur", t.meta, t.user_bg),
                ("texte d'erreur", t.error_text, t.error_bg),
                ("texte sur surface", t.text, t.surface),
                ("méta sur surface", t.meta, t.surface),
            ] {
                let ratio = contrast(ink, back);
                assert!(ratio >= TEXT, "{name} : {what} = {ratio}‰");
            }
            for (what, ink, back) in [
                ("lecture réussie", t.ok, t.panel),
                ("lecture en échec", t.warn, t.panel),
                ("accent", t.accent, t.panel),
                ("bouton Envoyer", t.send_bg, t.surface),
                ("icône Envoyer", t.send_fg, t.send_bg),
            ] {
                let ratio = contrast(ink, back);
                assert!(ratio >= GRAPHIC, "{name} : {what} = {ratio}‰");
            }
            assert_ne!(t.user_bg, t.panel, "{name}");
            // Le gras se distingue du texte (egui l'affiche par la couleur).
            assert!(
                contrast(t.strong, t.panel) > contrast(t.text, t.panel),
                "{name} : gras"
            );
            assert_ne!(t.send_disabled_bg, t.send_bg, "{name}");
        }
        assert_eq!(ChatTokens::for_theme(egui::Theme::Dark), ChatTokens::DARK);
    }

    /// H-UI-132 : symbole de l'application, trois formes de la grille de 16
    /// cellules (fond noir arrondi, carré blanc, bloc violet), dans le
    /// rectangle ; rectangle vide ou non fini ⇒ rien.
    #[test]
    fn h_ui_132_logo_shapes() {
        use egui::{Pos2, Rect, Shape, vec2};
        let rect = Rect::from_min_size(Pos2::new(10.0, 4.0), vec2(16.0, 16.0));
        let shapes = super::logo_shapes(rect);
        assert_eq!(shapes.len(), 3);
        let fills: Vec<(Rect, Color32)> = shapes
            .iter()
            .map(|s| match s {
                Shape::Rect(r) => (r.rect, r.fill),
                other => panic!("forme inattendue : {other:?}"),
            })
            .collect();
        assert_eq!(fills[0], (rect, Color32::BLACK));
        assert_eq!(fills[1].1, Color32::WHITE);
        assert_eq!(
            fills[1].0,
            Rect::from_min_max(Pos2::new(12.0, 15.0), Pos2::new(15.0, 18.0))
        );
        assert_eq!(fills[2].1, super::BRAND_VIOLET);
        assert_eq!(
            fills[2].0,
            Rect::from_min_max(Pos2::new(16.0, 6.0), Pos2::new(24.0, 18.0))
        );
        for (r, _) in &fills {
            assert!(rect.contains_rect(*r));
        }
        assert_eq!(super::logo_shapes(Rect::NOTHING).len(), 0);
        assert_eq!(
            super::logo_shapes(Rect::from_min_size(Pos2::ZERO, vec2(0.0, 16.0))).len(),
            0
        );
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
