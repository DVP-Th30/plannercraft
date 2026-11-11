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

    /// Palette claire de l'interface adoucie (lot U1, H-UI-167) : Sobre,
    /// modèle par défaut, tel que l'écran le dessine ([`screen_style`] ;
    /// relue par `sobre_screen_equals_soft_palette`).
    pub const SOFT_LIGHT: Self = Self {
        background: hex32(0x00FA_F9FC),
        text: hex32(0x002B_2931),
        text_strong: hex32(0x002B_2931),
        text_weak: hex32(0x006B_6775),
        grid: hex32(0x00EF_EDF3),
        grid_major: hex32(0x00E4_E1EA),
        header_bg: hex32(0x00F3_F1F6),
        selection: hex32(0x00CC_A9E0),
        bar: hex32(0x004E_7EC7),
        bar_outline: hex32(0x004E_7EC7),
        critical: hex32(0x00CC_5A37),
        milestone: hex32(0x003A_3742),
        summary: hex32(0x003A_3742),
        link: hex32(0x007F_7B89),
        status_date: hex32(0x006B_3FA0),
        splitter: hex32(0x00E4_E1EA),
        non_working: hex32(0x00F2_F0F5),
        summary_critical: hex32(0x00B2_4F30),
        message_error_bg: hex32(0x00FD_ECEA),
        message_warning_bg: hex32(0x00FF_F4D6),
        cursor: hex32(0x0077_19AA),
        baseline: hex32(0x008D_8998),
        progress: hex32(0x0020_3452),
        selection_border: hex32(0x0077_19AA),
        header_selected: hex32(0x00E6_D9EF),
        hover_row: hex32(0x00F0_E9F6),
    };

    /// Palette sombre de l'interface adoucie (même règle).
    pub const SOFT_DARK: Self = Self {
        background: hex32(0x001D_1C21),
        text: hex32(0x00E7_E4EC),
        text_strong: hex32(0x00E7_E4EC),
        text_weak: hex32(0x00A2_9DAE),
        grid: hex32(0x0027_252E),
        grid_major: hex32(0x0030_2D38),
        header_bg: hex32(0x0024_222A),
        selection: hex32(0x0055_416B),
        bar: hex32(0x006F_A3EA),
        bar_outline: hex32(0x006F_A3EA),
        critical: hex32(0x00F0_825C),
        milestone: hex32(0x00CF_CAD8),
        summary: hex32(0x00CF_CAD8),
        link: hex32(0x007D_7887),
        status_date: hex32(0x00B7_94F6),
        splitter: hex32(0x0030_2D38),
        non_working: hex32(0x0022_2027),
        summary_critical: hex32(0x00F5_A88C),
        message_error_bg: hex32(0x003A_1414),
        message_warning_bg: hex32(0x003A_3012),
        cursor: hex32(0x00D4_AEF0),
        baseline: hex32(0x0080_7B8A),
        progress: hex32(0x0039_5377),
        selection_border: hex32(0x00D4_AEF0),
        header_selected: hex32(0x0036_2D41),
        hover_row: hex32(0x002A_2531),
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
    /// Chiffre de la pastille du bouton de l'assistant, sur l'accent (lot
    /// A5, H-UI-166).
    pub on_accent: Color32,
    /// Fond de l'application (interface adoucie, H-UI-167) : barre d'état,
    /// sous la surface des volets.
    pub app: Color32,
    /// Fond des en-têtes (table, échelle de temps, frise).
    pub header: Color32,
    /// Fond des champs de saisie.
    pub field: Color32,
    /// Trait fin entre deux lignes de table.
    pub row_line: Color32,
    /// Liens du Gantt de l'interface adoucie (gris doux, ≥ 3:1).
    pub link: Color32,
    /// Fond d'un bouton d'egui au repos (et des cases à cocher).
    pub control: Color32,
    /// Fond d'un bouton d'egui survolé.
    pub control_hover: Color32,
    /// Fond d'un bouton d'egui appuyé.
    pub control_active: Color32,
    /// Contour d'un bouton, d'un champ et d'une case à cocher d'egui au
    /// repos (cahier : « bordure 1 px », un cran au-dessus du filet pour que
    /// la case vide reste visible sur la surface).
    pub control_border: Color32,
    /// Texte sur l'accent doux (`toggle_bg`) : message de l'utilisateur.
    pub soft_text: Color32,
    /// Succès (point « Planning cohérent », lecture réussie).
    pub ok: Color32,
    /// Voile posé sur l'application sous la palette de commandes (lot U3,
    /// maquette : `rgba(43,41,49,.32)` en clair, `rgba(8,6,12,.55)` en
    /// sombre).
    pub scrim: Color32,
    /// Ombre de la palette de commandes (maquette : `rgba(20,12,30,.28)`).
    pub float_shadow: Color32,
}

impl ChromeTokens {
    /// Thème clair (interface adoucie, lot U1 : plus de blanc pur ni de noir
    /// pur).
    pub const LIGHT: Self = Self {
        surface: hex32(0x00FA_F9FC),
        rule: hex32(0x00E4_E1EA),
        text: hex32(0x002B_2931),
        text_weak: hex32(0x006B_6775),
        disabled: Color32::from_rgb(0x7A, 0x79, 0x82),
        accent: BRAND_VIOLET,
        hover: hex32(0x00EF_EDF3),
        pressed: hex32(0x00E4_E1EA),
        toggle_bg: hex32(0x00F0_E6F7),
        timeline_bg: hex32(0x00F3_F1F6),
        timeline_bar: Color32::from_rgb(0xEB, 0xDD, 0xF5),
        timeline_bar_text: hex32(0x002B_2931),
        window_fill: Color32::from_rgba_unmultiplied_const(0x77, 0x19, 0xAA, 0x06),
        window_stroke: Color32::from_rgba_unmultiplied_const(0x77, 0x19, 0xAA, 0x99),
        selection_bg: Color32::from_rgb(0xE6, 0xD5, 0xF3),
        selection_fg: BRAND_VIOLET,
        logo_ring: hex32(0x00FA_F9FC),
        table_cursor: BRAND_VIOLET,
        on_accent: Color32::WHITE,
        app: hex32(0x00EE_ECF2),
        header: hex32(0x00F3_F1F6),
        field: Color32::WHITE,
        row_line: hex32(0x00EF_EDF3),
        link: hex32(0x007F_7B89),
        control: hex32(0x00EC_EAF0),
        control_hover: hex32(0x00E4_E1EA),
        control_active: hex32(0x00D9_D5E0),
        control_border: hex32(0x00D6_D2DD),
        soft_text: hex32(0x005E_1389),
        ok: hex32(0x002E_8B57),
        scrim: Color32::from_rgba_unmultiplied_const(0x2B, 0x29, 0x31, 82),
        float_shadow: Color32::from_rgba_unmultiplied_const(0x14, 0x0C, 0x1E, 71),
    };

    /// Thème sombre (interface adoucie, lot U1).
    pub const DARK: Self = Self {
        surface: hex32(0x001D_1C21),
        rule: hex32(0x0030_2D38),
        text: hex32(0x00E7_E4EC),
        text_weak: hex32(0x00A2_9DAE),
        disabled: Color32::from_rgb(0x7E, 0x7E, 0x87),
        accent: Color32::from_rgb(0xB5, 0x7B, 0xE0),
        hover: hex32(0x0027_252E),
        pressed: hex32(0x0030_2D38),
        toggle_bg: hex32(0x0035_264A),
        timeline_bg: hex32(0x0024_222A),
        timeline_bar: Color32::from_rgb(0x3F, 0x2D, 0x54),
        timeline_bar_text: hex32(0x00E7_E4EC),
        window_fill: Color32::from_rgba_unmultiplied_const(0xB5, 0x7B, 0xE0, 0x0A),
        window_stroke: Color32::from_rgba_unmultiplied_const(0xB5, 0x7B, 0xE0, 0x99),
        selection_bg: Color32::from_rgb(0x4A, 0x30, 0x63),
        selection_fg: Color32::from_rgb(0xD4, 0xAE, 0xF0),
        logo_ring: Color32::from_rgb(0x4C, 0x4D, 0x55),
        table_cursor: Color32::from_rgb(0xD4, 0xAE, 0xF0),
        // Accent clair du sombre : chiffre foncé (≈ 6:1), le blanc n'y
        // atteint pas 3:1.
        on_accent: hex32(0x001D_1C21),
        app: hex32(0x0014_1317),
        header: hex32(0x0024_222A),
        field: hex32(0x0024_222A),
        row_line: hex32(0x0027_252E),
        link: hex32(0x007D_7887),
        control: hex32(0x002A_2830),
        control_hover: hex32(0x0034_313C),
        control_active: hex32(0x003F_3B48),
        control_border: hex32(0x003F_3B48),
        soft_text: hex32(0x00D9_B8F2),
        ok: hex32(0x005F_C08A),
        scrim: Color32::from_rgba_unmultiplied_const(0x08, 0x06, 0x0C, 140),
        float_shadow: Color32::from_rgba_unmultiplied_const(0x14, 0x0C, 0x1E, 71),
    };

    /// Jetons du thème résolu.
    #[must_use]
    pub const fn for_theme(theme: egui::Theme) -> Self {
        match theme {
            egui::Theme::Light => Self::LIGHT,
            egui::Theme::Dark => Self::DARK,
        }
    }

    /// Pose les visuels d'egui des deux thèmes de `ctx` sur les jetons de
    /// l'habillage ([`soft_visuals`]) : sélection à l'accent (H-UI-132),
    /// surfaces, filets, champs, boutons et textes de l'interface adoucie
    /// (lot U1, H-UI-167).
    pub fn install(ctx: &egui::Context) {
        for theme in [egui::Theme::Light, egui::Theme::Dark] {
            ctx.style_mut_of(theme, |style| style.visuals = soft_visuals(theme));
        }
    }
}

/// Visuels d'egui de l'interface adoucie pour `theme` (lot U1, H-UI-167) :
/// ceux d'egui, dont les couleurs passent aux jetons de [`ChromeTokens`].
/// Volets, fenêtres et menus sur la surface (plus de blanc pur ni de gris
/// d'egui), champs sur `field`, boutons sur `control`, filets et contours
/// sur `rule`, textes sur `text` (plus de noir pur, y compris le texte
/// « fort » d'egui), texte atténué sur `text_weak`, sélection à l'accent
/// (H-UI-132), liens hypertexte à l'accent. Formes, marges et rayons
/// d'egui inchangés.
#[must_use]
pub fn soft_visuals(theme: egui::Theme) -> egui::Visuals {
    use egui::Stroke;
    let t = ChromeTokens::for_theme(theme);
    let mut v = match theme {
        egui::Theme::Light => egui::Visuals::light(),
        egui::Theme::Dark => egui::Visuals::dark(),
    };
    v.panel_fill = t.surface;
    v.window_fill = t.surface;
    v.window_stroke = Stroke::new(1.0, t.rule);
    v.extreme_bg_color = t.field;
    v.text_edit_bg_color = Some(t.field);
    v.faint_bg_color = t.header;
    v.code_bg_color = t.header;
    v.weak_text_color = Some(t.text_weak);
    v.hyperlink_color = t.accent;
    v.selection.bg_fill = t.selection_bg;
    v.selection.stroke = Stroke::new(1.0, t.selection_fg);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = t.surface;
    w.noninteractive.weak_bg_fill = t.surface;
    w.noninteractive.bg_stroke = Stroke::new(1.0, t.rule);
    w.noninteractive.fg_stroke = Stroke::new(1.0, t.text);
    w.inactive.bg_fill = t.control;
    w.inactive.weak_bg_fill = t.control;
    w.inactive.fg_stroke = Stroke::new(1.0, t.text);
    w.inactive.bg_stroke = Stroke::new(1.0, t.control_border);
    w.hovered.bg_fill = t.control_hover;
    w.hovered.weak_bg_fill = t.control_hover;
    w.hovered.bg_stroke = Stroke::new(1.0, t.text_weak);
    w.hovered.fg_stroke = Stroke::new(1.5, t.text);
    w.active.bg_fill = t.control_active;
    w.active.weak_bg_fill = t.control_active;
    w.active.bg_stroke = Stroke::new(1.0, t.text);
    w.active.fg_stroke = Stroke::new(2.0, t.text);
    w.open.bg_fill = t.surface;
    w.open.weak_bg_fill = t.control_hover;
    w.open.bg_stroke = Stroke::new(1.0, t.rule);
    w.open.fg_stroke = Stroke::new(1.0, t.text);
    v
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
    /// Texte appuyé (gras du Markdown par la couleur : repli tant que les
    /// familles Inter ne sont pas chargées, lot A2).
    pub strong: Color32,
    /// Méta : pied de message, lignes d'outils, barre d'état, réflexion.
    pub meta: Color32,
    /// Fond du message de l'utilisateur : accent doux de l'interface
    /// adoucie (lot U1), opaque sur le volet.
    pub user_bg: Color32,
    /// Texte du message de l'utilisateur (lot U1 : texte de l'accent doux).
    pub user_text: Color32,
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
    /// Thème clair (interface adoucie, lot U1).
    pub const LIGHT: Self = Self {
        panel: ChromeTokens::LIGHT.surface,
        text: ChromeTokens::LIGHT.text,
        // Gras par la couleur : un cran plus foncé que le texte, sans noir
        // pur.
        strong: hex32(0x0017_161B),
        meta: ChromeTokens::LIGHT.text_weak,
        user_bg: ChromeTokens::LIGHT.toggle_bg,
        user_text: ChromeTokens::LIGHT.soft_text,
        rule: hex32(0x00D6_D0DE),
        ok: ChromeTokens::LIGHT.ok,
        warn: Color32::from_rgb(0xA8, 0x4B, 0x00),
        error_bg: Color32::from_rgb(0xFC, 0xEE, 0xED),
        error_border: Color32::from_rgb(0xE3, 0xA5, 0xA0),
        error_text: Color32::from_rgb(0x8A, 0x1C, 0x1C),
        surface: ChromeTokens::LIGHT.field,
        border: ChromeTokens::LIGHT.rule,
        send_bg: BRAND_VIOLET,
        send_fg: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        send_disabled_bg: ChromeTokens::LIGHT.control,
        send_disabled_fg: Color32::from_rgb(0x8E, 0x8C, 0x96),
        accent: BRAND_VIOLET,
    };

    /// Thème sombre (interface adoucie, lot U1).
    pub const DARK: Self = Self {
        panel: ChromeTokens::DARK.surface,
        text: ChromeTokens::DARK.text,
        strong: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        meta: ChromeTokens::DARK.text_weak,
        user_bg: ChromeTokens::DARK.toggle_bg,
        user_text: ChromeTokens::DARK.soft_text,
        rule: Color32::from_rgb(0x45, 0x40, 0x4D),
        ok: ChromeTokens::DARK.ok,
        warn: Color32::from_rgb(0xF0, 0xB3, 0x5A),
        error_bg: Color32::from_rgb(0x38, 0x20, 0x20),
        error_border: Color32::from_rgb(0x8C, 0x45, 0x45),
        error_text: Color32::from_rgb(0xF6, 0xBE, 0xBE),
        surface: ChromeTokens::DARK.field,
        border: ChromeTokens::DARK.rule,
        send_bg: Color32::from_rgb(0xB5, 0x7B, 0xE0),
        send_fg: ChromeTokens::DARK.surface,
        send_disabled_bg: ChromeTokens::DARK.control,
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

/// Graine de la teinte de sélection à l'accent (H-UI-136), claire et
/// sombre : la bascule de l'habillage d'avant le lot U1, gardée telle
/// quelle pour que l'écran de Classique ne bouge pas quand l'accent doux de
/// l'interface adoucie (`ChromeTokens::toggle_bg`) change.
const ACCENT_SELECTION_SEED: [Rgb; 2] = [Rgb(0xF1, 0xE6, 0xF8), Rgb(0x3A, 0x2D, 0x48)];

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
/// Graine de la teinte : [`ACCENT_SELECTION_SEED`] (violet très clair),
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
    out.selection = if variant == Variant::Dark {
        ACCENT_SELECTION_SEED[1]
    } else {
        ACCENT_SELECTION_SEED[0]
    };
    out.cursor = rgb(chrome.table_cursor);
    Some(out)
}

/// Modèles affichés à l'écran dans l'**interface adoucie** (lot U1,
/// ADR-0026, H-UI-167) : Sobre, modèle par défaut des préférences (le
/// « thème d'interface par défaut »). Classique garde l'écran de J7
/// (`Palette::LIGHT` / `DARK`) et les autres modèles le leur : choisir un
/// modèle dans la galerie change toujours l'écran.
pub const SOFT_TEMPLATES: [TemplateId; 1] = [TemplateId::Sobre];

/// Couleur `0xRRGGBB` en [`Rgb`].
const fn hex(v: u32) -> Rgb {
    let [_, r, g, b] = v.to_be_bytes();
    Rgb(r, g, b)
}

/// Couleur `0xRRGGBB` en [`Color32`].
const fn hex32(v: u32) -> Color32 {
    let [_, r, g, b] = v.to_be_bytes();
    Color32::from_rgb(r, g, b)
}

/// Encre d'avancement d'une barre de remplissage `fill` (interface adoucie,
/// H-UI-170) : la même teinte assombrie, par pas de 1 % vers le noir,
/// jusqu'à 3:1 sur la barre (H-UI-22) ; le noir si rien n'y parvient.
#[must_use]
pub fn progress_ink(fill: Rgb) -> Rgb {
    (0..=100_u8)
        .map(|k| blend(fill, Rgb(0, 0, 0), k))
        .find(|c| contrast_ratio(*c, fill) >= 3000)
        .unwrap_or(Rgb(0, 0, 0))
}

/// Jetons de couleur de l'interface adoucie pour la variante d'écran
/// `variant` (H-UI-167) ; `None` pour l'impression, qui garde toujours les
/// couleurs du modèle. Valeurs du cahier « Interface adoucie » (maquette
/// validée le 2026-11-10) ; les liens sont assombris jusqu'à 3:1 sur le
/// fond, les jours chômés et la ligne sélectionnée (le gris doux du cahier,
/// `#A9A4B5`, n'atteint que 2,3:1 sur la surface) ; avancement : teinte de
/// la barre assombrie ([`progress_ink`]). Sélection et curseur : ceux de
/// l'habillage (H-UI-136).
#[must_use]
pub fn soft_tokens(variant: Variant) -> Option<ColorTokens> {
    use plannercraft_render::style::BarPaint;
    let light = match variant {
        Variant::Light => true,
        Variant::Dark => false,
        Variant::Print => return None,
    };
    let chrome = if light {
        ChromeTokens::LIGHT
    } else {
        ChromeTokens::DARK
    };
    let pick = |l: u32, d: u32| hex(if light { l } else { d });
    let bg = pick(0x00FA_F9FC, 0x001D_1C21);
    let task = pick(0x004E_7EC7, 0x006F_A3EA);
    let critical = pick(0x00CC_5A37, 0x00F0_825C);
    let summary = pick(0x003A_3742, 0x00CF_CAD8);
    let gray = pick(0x008D_8998, 0x0080_7B8A);
    let solid = BarPaint::solid;
    let outlined = |outline: Rgb| BarPaint::outlined(bg, outline);
    let bars = [
        (BarRole::Task, solid(task)),
        (BarRole::Critical, solid(critical)),
        (BarRole::Summary, solid(summary)),
        (BarRole::SummaryCritical, solid(pick(0x00B2_4F30, 0x00F5_A88C))),
        (BarRole::Milestone, solid(summary)),
        (BarRole::MilestoneCritical, solid(critical)),
        // Jeton unique (palette, Gantt de suivi hors écran adouci) : bleu
        // assombri lisible aussi sur la barre critique ; à l'écran,
        // chaque barre prend sa propre teinte ([`progress_ink`]).
        (
            BarRole::Progress,
            solid(
                (0..=100_u8)
                    .map(|k| blend(task, Rgb(0, 0, 0), k))
                    .find(|c| contrast_ratio(*c, task) >= 3000 && contrast_ratio(*c, critical) >= 3000)
                    .unwrap_or(Rgb(0, 0, 0)),
            ),
        ),
        (BarRole::Baseline, solid(gray)),
        (BarRole::Inactive, outlined(gray)),
        (BarRole::Manual, outlined(task)),
        (BarRole::Slack, solid(gray)),
        (BarRole::Deadline, solid(pick(0x00B5_4700, 0x00FF_8C42))),
    ]
    .into_iter()
    .collect();
    let rgb = |c: Color32| Rgb(c.r(), c.g(), c.b());
    Some(ColorTokens {
        bg,
        text: rgb(chrome.text),
        // Plus de noir pur (cahier) : le texte appuyé ne se distingue que
        // par la graisse.
        text_strong: rgb(chrome.text),
        text_muted: rgb(chrome.text_weak),
        header_bg: rgb(chrome.header),
        grid: rgb(chrome.row_line),
        grid_major: rgb(chrome.rule),
        band: [bg, rgb(chrome.header)],
        non_working: pick(0x00F2_F0F5, 0x0022_2027),
        status_line: pick(0x006B_3FA0, 0x00B7_94F6),
        today_line: pick(0x000B_7A55, 0x004F_D1A5),
        link: rgb(chrome.link),
        link_critical: critical,
        selection: rgb(chrome.toggle_bg),
        cursor: rgb(chrome.table_cursor),
        message_error_bg: pick(0x00FD_ECEA, 0x003A_1414),
        message_warning_bg: pick(0x00FF_F4D6, 0x003A_3012),
        bars,
    })
}

/// Style du planning tel que l'écran le dessine (lot U1, H-UI-167) : un
/// style dont les couleurs claires et sombres sont celles d'un modèle de
/// [`SOFT_TEMPLATES`] prend les couleurs de l'interface adoucie (claire et
/// sombre ; l'impression garde celles du modèle, recopiées dans
/// `colors.print`) ; les barres pleines dont la forme est celle du modèle
/// prennent des coins de 4 px. Rend vrai si le style est adouci. Un autre
/// modèle, ou une surcharge d'une couleur autre que celles des barres, est
/// rendu tel quel : l'aperçu, le SVG et le PDF lisent toujours le style du
/// document, jamais celui-ci.
///
/// Lot R1c (H-UI-195, ADR-0030) : une couleur de barre posée par
/// l'utilisateur ([`plannercraft_render::style::BarPaint::is_user`]) ne fait plus perdre l'interface
/// adoucie au reste de l'écran (avant : tout l'écran repassait aux
/// couleurs et aux formes de Sobre) ; seul son rôle la porte, contour
/// dérivé comme le modèle adouci du rôle (aucun s'il n'en a pas), le
/// remplissage d'un rôle « contour seul » restant le fond adouci. De même,
/// une forme changée ne retire les coins adoucis qu'à son rôle.
#[must_use]
pub fn screen_style(mut style: PlanStyle) -> (PlanStyle, bool) {
    let model = SOFT_TEMPLATES.into_iter().map(template).find(|t| {
        same_frame(&t.colors.light, &style.colors.light) && same_frame(&t.colors.dark, &style.colors.dark)
    });
    let (Some(model), Some(light), Some(dark)) =
        (model, soft_tokens(Variant::Light), soft_tokens(Variant::Dark))
    else {
        return (style, false);
    };
    let print = style.tokens(Variant::Print).clone();
    style.colors.print = Some(print);
    let light = soft_with_user_bars(light, &style, Variant::Light);
    let dark = soft_with_user_bars(dark, &style, Variant::Dark);
    style.colors.light = light;
    style.colors.dark = dark;
    for role in [
        BarRole::Task,
        BarRole::Critical,
        BarRole::Inactive,
        BarRole::Manual,
    ] {
        let same = model.bars.get(&role) == style.bars.get(&role);
        if same && let Some(bar) = style.bars.get_mut(&role) {
            bar.radius16 = SOFT_BAR_RADIUS16;
        }
    }
    (style, true)
}

/// `style` a les couleurs du modèle `model`, barres choisies par
/// l'utilisateur mises à part (lot R1c) : mêmes couleurs hors barres (fond,
/// texte, grille…) et mêmes barres pour les rôles non choisis.
fn same_frame(model: &ColorTokens, style: &ColorTokens) -> bool {
    let frame = |c: &ColorTokens| ColorTokens {
        bars: std::collections::BTreeMap::new(),
        ..c.clone()
    };
    frame(model) == frame(style)
        && model.bars.len() == style.bars.len()
        && style
            .bars
            .iter()
            .all(|(role, paint)| paint.is_user() || model.bars.get(role) == Some(paint))
}

/// Jetons adoucis `soft`, couleurs de barres posées par l'utilisateur dans
/// `style` (variante `v`) gardées (lot R1c, H-UI-195).
fn soft_with_user_bars(mut soft: ColorTokens, style: &PlanStyle, v: Variant) -> ColorTokens {
    let bg = soft.bg;
    for (role, paint) in &style.tokens(v).bars {
        if !paint.is_user() {
            continue;
        }
        let outline_only = style
            .bars
            .get(role)
            .is_some_and(|b| b.pattern == plannercraft_render::style::Pattern::Outline);
        let quiet = soft.bars.get(role).copied();
        let mut own = *paint;
        if outline_only {
            own.fill = quiet.map_or(bg, |q| q.fill);
            if let Some(key) = own.outline {
                own.progress = Some(plannercraft_render::style::user_progress(key, own.fill));
            }
        } else {
            own.outline = quiet
                .and_then(|q| q.outline)
                .map(|_| plannercraft_render::style::user_outline(own.fill, bg));
        }
        soft.bars.insert(*role, own);
    }
    soft
}

/// Rayon des barres pleines de l'interface adoucie, en 1/16 px (4 px à
/// l'échelle 100 %, cahier U1 § 4).
pub const SOFT_BAR_RADIUS16: u16 = 64;

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
/// elles y sont déjà ; egui ne relit que les noms), Inter Regular aussi en
/// tête de la famille proportionnelle de l'interface (lot U1). Les polices
/// d'egui suivent en repli pour les caractères absents d'Inter.
pub fn install_plan_fonts(ctx: &egui::Context) {
    use egui::epaint::text::{FontData, FontInsert, FontPriority, InsertFontFamily};
    for (name, bytes) in [(PLAN_FAMILY, INTER_REGULAR), (PLAN_FAMILY_STRONG, INTER_SEMIBOLD)] {
        let mut families = vec![InsertFontFamily {
            family: egui::FontFamily::Name(name.into()),
            priority: FontPriority::Highest,
        }];
        // Interface adoucie (lot U1, H-UI-169) : Inter Regular devient la
        // police proportionnelle de toute l'interface, en tête de la
        // famille (Ubuntu d'egui en repli pour les glyphes absents).
        if name == PLAN_FAMILY {
            families.push(InsertFontFamily {
                family: egui::FontFamily::Proportional,
                priority: FontPriority::Highest,
            });
        }
        ctx.add_font(FontInsert::new(name, FontData::from_static(bytes), families));
    }
}

/// Vrai si les familles Inter sont chargées dans `ctx` ; sinon les
/// installe (prises en compte à l'image suivante, ou aussitôt si aucun
/// texte n'a encore été mis en page) et rend faux : l'appelant garde la
/// police d'egui pour cette image (une famille inconnue ne doit jamais
/// être demandée à egui).
#[must_use]
pub fn plan_fonts_ready(ctx: &egui::Context) -> bool {
    let ready = ctx.fonts(|f| {
        let families = &f.definitions().families;
        families.contains_key(&egui::FontFamily::Name(PLAN_FAMILY.into()))
            && families.contains_key(&egui::FontFamily::Name(PLAN_FAMILY_STRONG.into()))
    });
    if !ready {
        install_plan_fonts(ctx);
    }
    ready
}

/// Inter `SemiBold` de `size` points si les familles Inter sont chargées,
/// sinon la police proportionnelle d'egui (onglet actif de la barre du
/// haut, en-tête du Formulaire tâche ; lot U2).
#[must_use]
pub fn strong_font(ctx: &egui::Context, size: f32) -> egui::FontId {
    if plan_fonts_ready(ctx) {
        egui::FontId::new(size, egui::FontFamily::Name(PLAN_FAMILY_STRONG.into()))
    } else {
        egui::FontId::proportional(size)
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
    /// Style adouci par [`screen_style`] (lot U1, H-UI-167) : table sans
    /// zébrure ni filets verticaux, barres et avancement de l'interface
    /// adoucie.
    pub soft: bool,
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
    /// Classique clair (J7), non adouci (lot U1 : Sobre seul l'est).
    fn default() -> Self {
        let (style, soft) = screen_style(template(TemplateId::Classique));
        Self {
            style: Arc::new(style),
            variant: Variant::Light,
            phases: Arc::new(Vec::new()),
            phase_colors: Arc::new(PhaseColors::new()),
            text_looks: Arc::new(TextLooks::new()),
            soft,
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
    /// Style gardé (tel que dessiné à l'écran, [`screen_style`]).
    style: Arc<PlanStyle>,
    /// Le style gardé est adouci (lot U1).
    soft: bool,
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
            soft: ScreenStyle::default().soft,
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
            // Interface adoucie (lot U1) : l'écran dessine le style adouci ;
            // l'aperçu, le SVG et le PDF relisent le style du document.
            let (style, soft) = screen_style(resolve());
            self.soft = soft;
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
            soft: self.soft,
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

    /// Palettes écrites en dur : Classique (J7) et interface adoucie (lot
    /// U1), claires et sombres.
    fn palettes() -> [(&'static str, Palette); 4] {
        [
            ("clair", Palette::LIGHT),
            ("sombre", Palette::DARK),
            ("clair adouci", Palette::SOFT_LIGHT),
            ("sombre adouci", Palette::SOFT_DARK),
        ]
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
    /// les deux thèmes (l'interface adoucie du lot U1 ne le touche pas), sauf la sélection et le curseur passés à l'accent
    /// de l'interface (H-UI-136) ; la variante d'impression garde le bleu
    /// du modèle.
    #[test]
    fn classique_equals_j7_palette() {
        let style = template(TemplateId::Classique);
        assert_eq!(super::screen_style(style.clone()), (style.clone(), false));
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

    /// Sobre (modèle par défaut) à l'écran = palettes de l'interface
    /// adoucie (lot U1, H-UI-167), dans les deux thèmes ; sa variante
    /// d'impression reste celle du modèle.
    #[test]
    fn sobre_screen_equals_soft_palette() {
        let style = template(TemplateId::Sobre);
        let (screen, soft) = super::screen_style(style.clone());
        assert!(soft);
        assert_eq!(Palette::from_style(&screen, Variant::Light), Palette::SOFT_LIGHT);
        assert_eq!(Palette::from_style(&screen, Variant::Dark), Palette::SOFT_DARK);
        assert_eq!(
            Palette::from_style(&screen, Variant::Print),
            Palette::from_style(&style, Variant::Print)
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
        // Sobre est adouci à l'écran (lot U1).
        let soft = super::screen_style(template(TemplateId::Sobre)).0;
        assert_eq!(light, Palette::from_style(&soft, Variant::Light));
        assert_eq!(dark, Palette::from_style(&soft, Variant::Dark));
        assert_eq!(light, Palette::SOFT_LIGHT);
        assert!(a.soft);
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
        // Modèles tels quels et tels que l'écran les dessine (interface
        // adoucie, lot U1 : Classique et Sobre).
        let styles = TemplateId::ALL
            .into_iter()
            .flat_map(|id| [(id, template(id)), (id, super::screen_style(template(id)).0)]);
        for (id, style) in styles {
            let min = plannercraft_render::style::min_text_contrast(&style);
            for variant in [Variant::Light, Variant::Dark] {
                let c = style.tokens(variant);
                let p = Palette::from_style(&style, variant);
                let rgb = |v: Rgb| Color32::from_rgb(v.0, v.1, v.2);
                let zebra = p.background.lerp_to_gamma(p.header_bg, 0.6);
                let name = format!("{id:?} {variant:?} {}", style.colors.print.is_some());
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
            for (screen, style) in [(false, template(id)), (true, super::screen_style(template(id)).0)] {
                for variant in [Variant::Light, Variant::Dark] {
                    cases.push((
                        format!("{id:?} {variant:?} écran {screen}"),
                        Palette::from_style(&style, variant),
                    ));
                }
            }
        }
        assert_eq!(cases.len(), 4 + 4 * TemplateId::ALL.len());
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

    /// H-UI-167 (lot U1) : jetons de l'interface adoucie, clair et sombre.
    /// Textes sur le fond d'application, les en-têtes, les champs et les
    /// boutons d'egui ≥ 4,5:1 ; texte atténué ≥ 4,5:1 ; point « Planning
    /// cohérent » et liens ≥ 3:1 ; filets, lignes et en-têtes distincts de
    /// la surface ; jetons opaques.
    #[test]
    fn h_ui_167_soft_chrome_tokens() {
        use super::{CHROME_GRAPHIC_CONTRAST as GRAPHIC, CHROME_TEXT_CONTRAST as TEXT, ChromeTokens};
        for (name, t) in [("clair", ChromeTokens::LIGHT), ("sombre", ChromeTokens::DARK)] {
            // Interface adoucie (lot U1, H-UI-167) : fond d'application,
            // en-têtes, champs et boutons d'egui ; texte sur l'accent doux ;
            // point « Planning cohérent » et liens du Gantt visibles.
            for (label, back) in [
                ("fond d'application", t.app),
                ("en-tête", t.header),
                ("champ", t.field),
                ("bouton", t.control),
                ("bouton survolé", t.control_hover),
                ("bouton appuyé", t.control_active),
            ] {
                let ratio = contrast(t.text, back);
                assert!(ratio >= TEXT, "{name} : texte sur {label} = {ratio}‰");
            }
            for (label, back) in [
                ("fond d'application", t.app),
                ("en-tête", t.header),
                ("surface", t.surface),
            ] {
                let ratio = contrast(t.text_weak, back);
                assert!(ratio >= TEXT, "{name} : texte atténué sur {label} = {ratio}‰");
                let ratio = contrast(t.ok, back);
                assert!(ratio >= GRAPHIC, "{name} : succès sur {label} = {ratio}‰");
            }
            let ratio = contrast(t.soft_text, t.toggle_bg);
            assert!(ratio >= TEXT, "{name} : texte sur l'accent doux = {ratio}‰");
            let ratio = contrast(t.link, t.surface);
            assert!(ratio >= GRAPHIC, "{name} : lien sur la surface = {ratio}‰");
            for (label, back) in [("filet", t.rule), ("ligne", t.row_line), ("en-tête", t.header)] {
                assert_ne!(back, t.surface, "{name} : {label} = surface");
            }
            for c in [
                t.app,
                t.header,
                t.field,
                t.row_line,
                t.link,
                t.control,
                t.control_hover,
                t.control_active,
                t.control_border,
                t.soft_text,
                t.ok,
            ] {
                assert_eq!(c.a(), 255, "{name} : {c:?}");
            }
        }
    }

    /// H-UI-143 (lot A1b) : jetons de la conversation, clair et sombre.
    /// Textes (messages, méta, erreur, message de l'utilisateur) ≥ 4,5:1 sur
    /// leur fond ; icônes d'état, accent et bouton Envoyer ≥ 3:1 ; fond du
    /// volet = celui d'egui ; message de l'utilisateur distinct du volet.
    #[test]
    fn h_ui_143_chat_contrast() {
        use super::{CHROME_GRAPHIC_CONTRAST as GRAPHIC, CHROME_TEXT_CONTRAST as TEXT, ChatTokens};
        for (name, t, visuals) in [
            (
                "clair",
                ChatTokens::LIGHT,
                super::soft_visuals(egui::Theme::Light),
            ),
            ("sombre", ChatTokens::DARK, super::soft_visuals(egui::Theme::Dark)),
        ] {
            assert_eq!(t.panel, visuals.panel_fill, "{name} : fond du volet");
            for (what, ink, back) in [
                ("texte", t.text, t.panel),
                ("texte appuyé", t.strong, t.panel),
                ("méta", t.meta, t.panel),
                ("texte sur message utilisateur", t.text, t.user_bg),
                ("message de l'utilisateur", t.user_text, t.user_bg),
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

    /// H-UI-167 (lot U1) : l'écran adoucit Classique et Sobre tels quels
    /// (couleurs claires et sombres de l'interface, coins de 4 px), garde
    /// leur variante d'impression, et rend tout autre style tel quel :
    /// autres modèles, couleur choisie par l'utilisateur. Formes
    /// personnalisées : couleurs adoucies, formes gardées. Plus de blanc
    /// pur ni de noir pur dans le thème clair adouci.
    #[test]
    fn h_ui_167_soft_screen_style() {
        use plannercraft_render::style::BarRole;
        for id in TemplateId::ALL {
            let style = template(id);
            let (screen, soft) = super::screen_style(style.clone());
            assert_eq!(soft, super::SOFT_TEMPLATES.contains(&id), "{id:?}");
            assert_eq!(
                screen.tokens(Variant::Print),
                style.tokens(Variant::Print),
                "{id:?} : impression"
            );
            if !soft {
                assert_eq!(screen, style, "{id:?} : rendu tel quel");
                continue;
            }
            for v in [Variant::Light, Variant::Dark] {
                assert_eq!(
                    Some(screen.tokens(v).clone()),
                    super::soft_tokens(v),
                    "{id:?} {v:?}"
                );
            }
            for role in [
                BarRole::Task,
                BarRole::Critical,
                BarRole::Inactive,
                BarRole::Manual,
            ] {
                assert_eq!(
                    screen.bars.get(&role).map(|b| b.radius16),
                    Some(super::SOFT_BAR_RADIUS16),
                    "{id:?} {role:?}"
                );
            }
            assert_eq!(screen.texts, style.texts, "{id:?} : textes des barres inchangés");
        }
        assert!(super::soft_tokens(Variant::Print).is_none());
        // Couleur autre que celles des barres : style rendu tel quel (une
        // couleur de barre choisie garde l'écran adouci depuis le lot R1c,
        // `h_ui_195_soft_screen_keeps_with_user_color`).
        let mut custom = template(TemplateId::Sobre);
        custom.colors.light.bg = Rgb(0xFF, 0xFF, 0xF0);
        assert!(!super::screen_style(custom).1);
        // Formes personnalisées : couleurs adoucies, formes gardées.
        let mut shaped = template(TemplateId::Sobre);
        if let Some(bar) = shaped.bars.get_mut(&BarRole::Task) {
            bar.radius16 = 0;
        }
        let (screen, soft) = super::screen_style(shaped);
        assert!(soft);
        assert_eq!(screen.bars.get(&BarRole::Task).map(|b| b.radius16), Some(0));
        let light = super::soft_tokens(Variant::Light).unwrap();
        for c in [light.bg, light.text, light.text_strong, light.header_bg] {
            assert!(c != Rgb(0, 0, 0) && c != Rgb(0xFF, 0xFF, 0xFF), "{c:?}");
        }
    }

    /// H-UI-195 (lot R1c, I1) : une couleur de barre choisie sur Sobre ne
    /// fait plus perdre l'interface adoucie au reste de l'écran (constat de
    /// l'essai du 2026-11-11 : table, autres barres, récapitulatives et
    /// coins repassaient à Sobre) ; seul son rôle change, avec un avancement
    /// lisible et sans contour si le rôle adouci n'en a pas ; un rôle
    /// « contour seul » garde le fond adouci et prend la couleur au contour.
    #[test]
    fn h_ui_195_soft_screen_keeps_with_user_color() {
        use plannercraft_render::style::{
            BarPaint, BarRole, ColorTokensPatch, StylePatch, VariantsPatch, contrast_ratio, resolve,
        };
        let soft_plain = super::screen_style(template(TemplateId::Sobre)).0;
        for (role, fill) in [
            (BarRole::Task, Rgb(0xE6, 0x9F, 0x00)),
            (BarRole::Critical, Rgb(0, 0, 0)),
            (BarRole::Summary, Rgb(0xF0, 0xE4, 0x42)),
            (BarRole::Milestone, Rgb(0xC0, 0, 0)),
            (BarRole::Manual, Rgb(0xC0, 0, 0)),
        ] {
            let mut tokens = ColorTokensPatch::default();
            tokens.bars.insert(role, BarPaint::solid(fill));
            let over = StylePatch {
                colors: Some(VariantsPatch {
                    light: Some(tokens.clone()),
                    dark: Some(tokens),
                    print: None,
                }),
                ..StylePatch::default()
            };
            let (screen, soft) = super::screen_style(resolve(TemplateId::Sobre, &over));
            assert!(soft, "{role:?} : écran adouci gardé");
            assert_eq!(screen.bars, soft_plain.bars, "{role:?} : coins adoucis gardés");
            for v in [Variant::Light, Variant::Dark] {
                let mut a = screen.tokens(v).clone();
                let mut b = soft_plain.tokens(v).clone();
                let own = a.bars.remove(&role).unwrap();
                let quiet = b.bars.remove(&role).unwrap();
                assert_eq!(a, b, "{role:?} {v:?} : rien d'autre ne bouge");
                let progress = own.progress.unwrap();
                assert!(contrast_ratio(progress, own.fill) >= 3_000, "{role:?} {v:?}");
                if role == BarRole::Manual {
                    assert_eq!((own.fill, own.outline), (quiet.fill, Some(fill)));
                } else {
                    assert_eq!(own.fill, fill);
                    assert_eq!(own.outline.is_some(), quiet.outline.is_some(), "{role:?} {v:?}");
                }
            }
            // L'impression garde le style du document (couleur posée).
            let print = screen.tokens(Variant::Print).bars[&role];
            assert!(print.is_user());
        }
    }

    /// H-UI-169 (lot U1) : visuels d'egui adoucis. Volets, fenêtres et
    /// menus sur la surface ; champs sur `field` ; textes (courant, fort,
    /// boutons) au texte de l'habillage, jamais noir ni blanc purs ;
    /// sélection à l'accent (H-UI-132).
    #[test]
    fn h_ui_169_soft_visuals() {
        use super::{ChromeTokens, soft_visuals};
        for theme in [egui::Theme::Light, egui::Theme::Dark] {
            let t = ChromeTokens::for_theme(theme);
            let v = soft_visuals(theme);
            assert_eq!(v.dark_mode, theme == egui::Theme::Dark);
            assert_eq!(v.panel_fill, t.surface, "{theme:?}");
            assert_eq!(v.window_fill, t.surface, "{theme:?}");
            assert_eq!(v.text_edit_bg_color(), t.field, "{theme:?}");
            assert_eq!(v.text_color(), t.text, "{theme:?}");
            assert_eq!(v.strong_text_color(), t.text, "{theme:?}");
            assert_eq!(v.weak_text_color(), t.text_weak, "{theme:?}");
            assert_eq!(v.widgets.inactive.text_color(), t.text, "{theme:?}");
            assert_eq!(v.widgets.hovered.text_color(), t.text, "{theme:?}");
            assert_eq!(v.selection.bg_fill, t.selection_bg, "{theme:?}");
            assert_eq!(v.selection.stroke.color, t.selection_fg, "{theme:?}");
            for c in [v.text_color(), v.strong_text_color(), v.panel_fill, v.window_fill] {
                assert!(c != Color32::BLACK && c != Color32::WHITE, "{theme:?} {c:?}");
            }
        }
    }

    /// H-UI-170 (lot U1) : avancement de l'interface adoucie dans la teinte
    /// de la barre, assombrie (aucune composante plus claire), à 3:1 au
    /// moins sur la barre, pour les barres et critiques des deux thèmes.
    #[test]
    fn h_ui_170_progress_same_hue() {
        use plannercraft_render::style::BarRole;
        for v in [Variant::Light, Variant::Dark] {
            let c = super::soft_tokens(v).unwrap();
            for role in [BarRole::Task, BarRole::Critical] {
                let fill = c.bars.get(&role).unwrap().fill;
                let ink = super::progress_ink(fill);
                let rgb = |x: Rgb| Color32::from_rgb(x.0, x.1, x.2);
                let ratio = contrast(rgb(ink), rgb(fill));
                assert!(ratio >= 3000, "{v:?} {role:?} : {ratio}‰");
                assert!(
                    ink.0 <= fill.0 && ink.1 <= fill.1 && ink.2 <= fill.2,
                    "{v:?} {role:?}"
                );
                // Même teinte : composante dominante gardée.
                let dominant = |x: Rgb| if x.2 >= x.0 { 2 } else { 0 };
                assert_eq!(dominant(ink), dominant(fill), "{v:?} {role:?}");
            }
            // Jeton de la palette : bleu assombri, lisible sur les deux.
            let token = c.bars.get(&BarRole::Progress).unwrap().fill;
            let task = c.bars.get(&BarRole::Task).unwrap().fill;
            assert!(
                token.0 <= task.0 && token.1 <= task.1 && token.2 <= task.2,
                "{v:?}"
            );
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
