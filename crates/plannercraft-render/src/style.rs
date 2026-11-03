//! Style du planning (`PlanStyle`) et modèles de mise en forme intégrés
//! (ADR-0016 D6, D7 ; `docs/features/render.md` § 5.1).
//!
//! Un style est une donnée : sérialisé en JSON (`deny_unknown_fields`, aucun
//! flottant), borné par [`PlanStyle::validate`], lu par l'écran, le SVG et le
//! PDF. Six modèles intégrés (`templates/*.json`, [`TEMPLATES`]), chacun en
//! variantes claire, sombre et impression. Un document stylé garde un modèle
//! de base et des surcharges ([`DocStyle`], [`StylePatch`]) ; [`resolve`]
//! les combine.
//!
//! Écart au contrat écrit de L0 (consigné dans ADR-0016) : les couleurs des
//! barres sont rangées par variante dans [`ColorTokens::bars`] ([`BarPaint`])
//! et [`BarStyle`] ne garde que la forme. Sans cela, le modèle Classique ne
//! pourrait pas reproduire `Palette::DARK` de J7, dont les barres diffèrent
//! de la variante claire.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::OnceLock;

use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};

/// Version du format de style écrite et acceptée.
pub const STYLE_VERSION: u16 = 1;

/// Taille maximale d'un style de document lu (H-R12), en octets.
pub const MAX_DOC_STYLE_BYTES: usize = 64 * 1024;

/// Longueur maximale du nom d'un style, en caractères.
pub const MAX_NAME_CHARS: usize = 64;

/// Longueur maximale d'une zone d'en-tête ou de pied de page, en caractères.
pub const MAX_SLOT_CHARS: usize = 256;

/// Longueur maximale de la référence d'un logo, en caractères.
pub const MAX_LOGO_REF_CHARS: usize = 1024;

/// Couleur opaque sRGB, écrite `#RRGGBB`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// Écriture `#RRGGBB` (majuscules).
    #[must_use]
    pub fn hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }

    /// Lecture stricte de `#RRGGBB` (casse indifférente).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let digits = s.strip_prefix('#')?;
        if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let channel = |i: usize| {
            digits
                .get(i..i.saturating_add(2))
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        };
        Some(Self(channel(0)?, channel(2)?, channel(4)?))
    }
}

impl Serialize for Rgb {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.hex())
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct RgbVisitor;
        impl Visitor<'_> for RgbVisitor {
            type Value = Rgb;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("une couleur #RRGGBB")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Rgb, E> {
                Rgb::parse(v).ok_or_else(|| E::invalid_value(de::Unexpected::Str(v), &self))
            }
        }
        deserializer.deserialize_str(RgbVisitor)
    }
}

/// Modèle de mise en forme intégré.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateId {
    /// Apparence de J7, reproduite exactement (défaut pendant le
    /// développement de J8).
    Classique,
    /// Défaut visé après validation du propriétaire.
    Sobre,
    /// Texte ≥ 7:1 (WCAG 1.4.6).
    ContrasteEleve,
    /// Présentation client : aérée, bandes de phase.
    Presentation,
    /// Chantier : compact, purement visuel (ADR-0016, écart au brief § 12).
    Chantier,
    /// Impression noir et blanc.
    ImpressionNb,
}

impl TemplateId {
    /// Tous les modèles, dans l'ordre de la galerie.
    pub const ALL: [Self; 6] = [
        Self::Classique,
        Self::Sobre,
        Self::ContrasteEleve,
        Self::Presentation,
        Self::Chantier,
        Self::ImpressionNb,
    ];

    /// Identifiant écrit (`classique`, `contraste_eleve`…).
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Classique => "classique",
            Self::Sobre => "sobre",
            Self::ContrasteEleve => "contraste_eleve",
            Self::Presentation => "presentation",
            Self::Chantier => "chantier",
            Self::ImpressionNb => "impression_nb",
        }
    }

    /// Lecture de l'identifiant écrit.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.key() == key)
    }
}

/// Variante d'un modèle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Variant {
    /// Écran, thème clair.
    Light,
    /// Écran, thème sombre (le sombre est une variante, jamais un modèle).
    Dark,
    /// Impression et PDF.
    Print,
}

/// Valeurs d'un style par variante ; `print` absent ⇒ `light`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Variants<T> {
    /// Thème clair.
    pub light: T,
    /// Thème sombre.
    pub dark: T,
    /// Impression ; `None` ⇒ `light`.
    pub print: Option<T>,
}

impl<T> Variants<T> {
    /// Valeur de la variante `v`.
    #[must_use]
    pub fn get(&self, v: Variant) -> &T {
        match v {
            Variant::Light => &self.light,
            Variant::Dark => &self.dark,
            Variant::Print => self.print.as_ref().unwrap_or(&self.light),
        }
    }
}

/// Rôle d'une barre (catégorie de tâches, S78 `GanttBarEditEx`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarRole {
    /// Tâche (feuille) non critique.
    Task,
    /// Tâche critique.
    Critical,
    /// Récapitulative.
    Summary,
    /// Récapitulative critique.
    SummaryCritical,
    /// Jalon.
    Milestone,
    /// Jalon critique.
    MilestoneCritical,
    /// Bande d'avancement, dessinée dans la barre.
    Progress,
    /// Ligne de base.
    Baseline,
    /// Tâche inactive.
    Inactive,
    /// Tâche manuelle.
    Manual,
    /// Marge totale.
    Slack,
    /// Échéance.
    Deadline,
}

impl BarRole {
    /// Tous les rôles.
    pub const ALL: [Self; 12] = [
        Self::Task,
        Self::Critical,
        Self::Summary,
        Self::SummaryCritical,
        Self::Milestone,
        Self::MilestoneCritical,
        Self::Progress,
        Self::Baseline,
        Self::Inactive,
        Self::Manual,
        Self::Slack,
        Self::Deadline,
    ];
}

/// Couleurs d'une barre pour une variante.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BarPaint {
    /// Remplissage.
    pub fill: Rgb,
    /// Contour, dessiné si [`BarStyle::outline16`] > 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<Rgb>,
}

/// Couleurs d'une variante.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorTokens {
    /// Fond de la table et du Gantt.
    pub bg: Rgb,
    /// Texte courant.
    pub text: Rgb,
    /// Texte appuyé (récapitulatives).
    pub text_strong: Rgb,
    /// Texte atténué.
    pub text_muted: Rgb,
    /// Fond des en-têtes.
    pub header_bg: Rgb,
    /// Grille fine.
    pub grid: Rgb,
    /// Grille marquée (et séparateur table / Gantt).
    pub grid_major: Rgb,
    /// Fonds alternés des lignes (zébrage) ou des phases.
    pub band: [Rgb; 2],
    /// Jours chômés.
    pub non_working: Rgb,
    /// Ligne de la date d'état.
    pub status_line: Rgb,
    /// Marque d'aujourd'hui.
    pub today_line: Rgb,
    /// Lien.
    pub link: Rgb,
    /// Lien critique.
    pub link_critical: Rgb,
    /// Ligne sélectionnée.
    pub selection: Rgb,
    /// Cadre de la cellule courante.
    pub cursor: Rgb,
    /// Fond du bandeau d'erreur.
    pub message_error_bg: Rgb,
    /// Fond du bandeau d'avertissement.
    pub message_warning_bg: Rgb,
    /// Couleurs des barres, une entrée par rôle.
    pub bars: BTreeMap<BarRole, BarPaint>,
}

/// Forme d'une barre.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarShape {
    /// Rectangle (coins arrondis par `radius16`).
    Bar,
    /// Barre à crochets (récapitulative).
    Bracket,
    /// Losange (jalon).
    Diamond,
    /// Trait.
    Line,
}

/// Motif de remplissage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pattern {
    /// Plein.
    Solid,
    /// Hachures.
    Hatch,
    /// Points.
    Dots,
    /// Contour seul.
    Outline,
}

/// Forme d'une barre (indépendante de la variante).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BarStyle {
    /// Épaisseur du contour en 1/16 px (0 : aucun), ≤ 64.
    pub outline16: u16,
    /// Forme.
    pub shape: BarShape,
    /// Hauteur en % de la ligne, 10 à 100.
    pub height_pct: u8,
    /// Rayon des coins en 1/16 px, ≤ 128.
    pub radius16: u16,
    /// Motif.
    pub pattern: Pattern,
}

/// Champ affiché autour d'une barre (texte venu de `BarTextSource`, règle 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarField {
    /// Nom.
    Name,
    /// Ressources affectées.
    Resources,
    /// Début.
    Start,
    /// Fin.
    Finish,
    /// Durée.
    Duration,
    /// % achevé.
    Percent,
    /// Marge totale.
    TotalSlack,
    /// Début – fin.
    StartFinish,
    /// Nom suivi de la durée courte entre parenthèses, « Arche préfa PH
    /// RDC (13 j) » (J8 L15, H-UI-63 : case « durée après le nom »).
    NameDuration,
    /// Durée courte (« 13 j », « 4 sem. », « 17,4 mois » ; H-UI-63).
    DurationShort,
    /// Début court, `JJ/MM/AA` (H-UI-63, colonnes d'impression).
    StartShort,
    /// Fin courte, `JJ/MM/AA` (date d'un jalon, H-UI-63).
    FinishShort,
}

impl BarField {
    /// Tous les champs, dans l'ordre des listes de choix.
    pub const ALL: [Self; 12] = [
        Self::Name,
        Self::NameDuration,
        Self::Resources,
        Self::Start,
        Self::Finish,
        Self::StartShort,
        Self::FinishShort,
        Self::Duration,
        Self::DurationShort,
        Self::Percent,
        Self::TotalSlack,
        Self::StartFinish,
    ];
}

/// Textes autour d'une barre.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BarTexts {
    /// À gauche.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<BarField>,
    /// À droite.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<BarField>,
    /// Dedans.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inside: Option<BarField>,
}

/// Densité des lignes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Density {
    /// 18 px.
    Compact,
    /// 22 px (J7).
    Normal,
    /// 28 px.
    Airy,
}

impl Density {
    /// Hauteur de ligne en pixels.
    #[must_use]
    pub const fn row_px(self) -> u16 {
        match self {
            Self::Compact => 18,
            Self::Normal => 22,
            Self::Airy => 28,
        }
    }
}

/// Échelle de temps préférée.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TierPreset {
    /// Choisie selon la plage (H-R4).
    Auto,
    /// Semaine / jour.
    Day,
    /// Mois / semaine.
    Week,
    /// Trimestre / mois.
    Month,
    /// Année / trimestre.
    Quarter,
}

/// Échelle de temps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "interrupteurs indépendants, sérialisés tels quels"
)]
pub struct ScaleStyle {
    /// Niveaux de l'échelle.
    pub tiers: TierPreset,
    /// Numéros de semaine.
    pub week_numbers: bool,
    /// Jours chômés grisés.
    pub shade_non_working: bool,
    /// Date d'état étiquetée dans l'échelle (H-UI-54).
    pub status_label: bool,
    /// Aujourd'hui marqué (H-UI-54).
    pub today_mark: bool,
    /// Niveau des mois relatifs au démarrage (M-3 … M1 … Mn) au-dessus des
    /// autres (J8 L15, H-UI-67) ; absent du JSON s'il est faux.
    #[serde(default, skip_serializing_if = "is_false")]
    pub relative_months: bool,
}

/// Faux (champ omis à l'écriture : un style écrit avant L15 garde ses
/// octets).
#[allow(clippy::trivially_copy_pass_by_ref, reason = "signature imposée par serde")]
const fn is_false(b: &bool) -> bool {
    !*b
}

/// Fond des lignes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowBanding {
    /// Aucun.
    None,
    /// Zébrage.
    Zebra,
    /// Bandes de phase (récapitulatives de niveau 1, H-UI-53).
    Phase,
}

/// Grille.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GridStyle {
    /// Fond des lignes.
    pub rows: RowBanding,
    /// Verticales du niveau haut.
    pub major_v: bool,
    /// Verticales du niveau bas.
    pub minor_v: bool,
    /// Traits horizontaux entre lignes.
    pub row_lines: bool,
}

/// Police du planning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FontChoice {
    /// Police de l'interface (egui), Classique.
    Interface,
    /// Inter (ADR-0016 D9).
    Inter,
}

/// Police.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontStyle {
    /// Famille.
    pub family: FontChoice,
    /// Taille en 1/16 px, 160 à 320.
    pub size16: u16,
}

/// Légende (H-UI-47).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegendStyle {
    /// Légende affichée.
    pub show: bool,
    /// Sur chaque page (sinon la dernière).
    pub every_page: bool,
}

/// Format de papier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Paper {
    /// 210 × 297 mm.
    A4,
    /// 297 × 420 mm.
    A3,
    /// 8,5 × 11 pouces.
    Letter,
    /// 11 × 17 pouces.
    Tabloid,
}

/// Ajustement à la page (H-UI-50).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    /// Échelle en %, 10 à 400.
    Scale(u16),
    /// N pages en largeur, 1 à 20.
    PagesWide(u16),
    /// N pages en largeur, M en hauteur, 1 à 20 chacun.
    PagesWideTall(u16, u16),
}

/// Zone d'en-tête ou de pied de page (≤ 256 caractères ; jetons `{projet}`,
/// `{date_etat}`, `{page}`, `{pages}`, `{date_impression}`, `{fichier}`,
/// `{planning}`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Slot(pub String);

/// Référence d'un logo (chemin relatif au document ; lot L7).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogoRef {
    /// Chemin, ≤ 1 024 caractères.
    pub path: String,
}

/// Mise en page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "cases indépendantes de la mise en page, sérialisées telles quelles"
)]
pub struct PageStyle {
    /// Papier.
    pub paper: Paper,
    /// Paysage.
    pub landscape: bool,
    /// Marges haut, droite, bas, gauche en mm, ≤ 50.
    pub margins_mm: [u16; 4],
    /// Ajustement.
    pub fit: Fit,
    /// Table répétée sur chaque page en largeur.
    pub repeat_table: bool,
    /// En-tête : gauche, centre, droite.
    pub header: [Slot; 3],
    /// Pied de page : gauche, centre, droite.
    pub footer: [Slot; 3],
    /// Logo.
    pub logo: Option<LogoRef>,
    /// Colonnes de la table du SVG et du PDF (J8 L15, H-UI-64, H-UI-68) ;
    /// `None` : N° et Nom (sortie de L7) ; liste vide : table masquée
    /// (« graphique seul »). Au plus [`MAX_TABLE_COLUMNS`], sans doublon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<Vec<TableColumn>>,
    /// Cartouche (H-UI-66) : première ligne de la zone centrale de
    /// l'en-tête en titre, première ligne de la zone droite appuyée.
    #[serde(default, skip_serializing_if = "is_false")]
    pub title_block: bool,
    /// Chronologie en tête de la première page (ADR-0017 D8, H-UI-82).
    #[serde(default, skip_serializing_if = "is_false")]
    pub timeline: bool,
}

/// Colonnes au plus dans la table du SVG et du PDF.
pub const MAX_TABLE_COLUMNS: usize = 8;

/// Couleurs de phase au plus dans un style (H-UI-65).
pub const MAX_PHASES: usize = 512;

/// Tâches au plus sur la chronologie (ADR-0017 D7, H-UI-79).
pub const MAX_TIMELINE_TASKS: usize = 200;

/// Tâches ajoutées à la chronologie (ADR-0017 D7) : UID triés, sans
/// doublon, au plus [`MAX_TIMELINE_TASKS`] ([`PlanStyle::validate`]). Un
/// UID absent du projet est ignoré à l'affichage.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineSpec {
    /// UID des tâches, triés et sans doublon.
    #[serde(default)]
    pub tasks: Vec<u32>,
}

impl TimelineSpec {
    /// Vrai si aucune tâche n'est sur la chronologie.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// Liste bornée : UID triés, dédoublonnés, coupés à
    /// [`MAX_TIMELINE_TASKS`] (les plus petits gardés).
    #[must_use]
    pub fn bounded(mut tasks: Vec<u32>) -> Self {
        tasks.sort_unstable();
        tasks.dedup();
        tasks.truncate(MAX_TIMELINE_TASKS);
        Self { tasks }
    }

    /// Vrai si la liste est triée strictement et dans la borne.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.tasks.len() <= MAX_TIMELINE_TASKS && self.tasks.windows(2).all(|w| matches!(w, [a, b] if a < b))
    }
}

/// Colonne de la table du SVG et du PDF (H-UI-68) ; textes venus de
/// `BarTextSource` (règle 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableColumn {
    /// N° (ID).
    Id,
    /// Nom de la tâche.
    Name,
    /// Durée courte.
    Duration,
    /// Début court.
    Start,
    /// Fin courte.
    Finish,
    /// % achevé.
    Percent,
    /// Ressources.
    Resources,
}

impl TableColumn {
    /// Toutes les colonnes, dans l'ordre des listes de choix.
    pub const ALL: [Self; 7] = [
        Self::Id,
        Self::Name,
        Self::Duration,
        Self::Start,
        Self::Finish,
        Self::Percent,
        Self::Resources,
    ];

    /// Colonnes d'impression par défaut (H-UI-68) : N°, Nom, Durée, Début,
    /// Fin.
    pub const PRINT_DEFAULT: [Self; 5] = [Self::Id, Self::Name, Self::Duration, Self::Start, Self::Finish];

    /// « Graphique seul », table réduite (H-UI-64) : N° et Nom.
    pub const CHART_ONLY: [Self; 2] = [Self::Id, Self::Name];
}

/// Couleur d'une phase (récapitulative, H-UI-65).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhaseColor {
    /// Couleur de la barre et du fond de la ligne dans la table.
    pub fill: Rgb,
    /// Reprise par les barres des sous-tâches.
    #[serde(default, skip_serializing_if = "is_false")]
    pub children: bool,
}

/// Style complet du planning.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanStyle {
    /// Version du format ([`STYLE_VERSION`]).
    pub v: u16,
    /// Nom affiché.
    pub name: String,
    /// Modèle d'origine.
    pub base: Option<TemplateId>,
    /// Couleurs par variante.
    pub colors: Variants<ColorTokens>,
    /// Formes des barres, une entrée par rôle.
    pub bars: BTreeMap<BarRole, BarStyle>,
    /// Textes autour des barres (rôles absents : aucun texte).
    pub texts: BTreeMap<BarRole, BarTexts>,
    /// Échelle.
    pub scale: ScaleStyle,
    /// Grille.
    pub grid: GridStyle,
    /// Densité.
    pub density: Density,
    /// Police.
    pub font: FontStyle,
    /// Légende.
    pub legend: LegendStyle,
    /// Mise en page.
    pub page: PageStyle,
    /// Couleurs de phase par UID de récapitulative (H-UI-65), au plus
    /// [`MAX_PHASES`].
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub phases: BTreeMap<u32, PhaseColor>,
    /// Tâches de la chronologie (ADR-0017 D7, H-UI-79).
    #[serde(default, skip_serializing_if = "TimelineSpec::is_empty")]
    pub timeline: TimelineSpec,
}

/// Surcharge partielle des couleurs d'une variante.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
#[allow(missing_docs, reason = "mêmes champs que ColorTokens, tous facultatifs")]
pub struct ColorTokensPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg: Option<Rgb>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<Rgb>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_strong: Option<Rgb>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_muted: Option<Rgb>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header_bg: Option<Rgb>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid: Option<Rgb>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid_major: Option<Rgb>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub band: Option<[Rgb; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub non_working: Option<Rgb>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_line: Option<Rgb>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub today_line: Option<Rgb>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<Rgb>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_critical: Option<Rgb>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection: Option<Rgb>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<Rgb>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_error_bg: Option<Rgb>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_warning_bg: Option<Rgb>,
    /// Couleurs de barres remplacées, par rôle.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub bars: BTreeMap<BarRole, BarPaint>,
}

impl ColorTokensPatch {
    /// Applique la surcharge.
    fn apply(&self, c: &mut ColorTokens) {
        let set = |dst: &mut Rgb, src: Option<Rgb>| {
            if let Some(v) = src {
                *dst = v;
            }
        };
        set(&mut c.bg, self.bg);
        set(&mut c.text, self.text);
        set(&mut c.text_strong, self.text_strong);
        set(&mut c.text_muted, self.text_muted);
        set(&mut c.header_bg, self.header_bg);
        set(&mut c.grid, self.grid);
        set(&mut c.grid_major, self.grid_major);
        if let Some(band) = self.band {
            c.band = band;
        }
        set(&mut c.non_working, self.non_working);
        set(&mut c.status_line, self.status_line);
        set(&mut c.today_line, self.today_line);
        set(&mut c.link, self.link);
        set(&mut c.link_critical, self.link_critical);
        set(&mut c.selection, self.selection);
        set(&mut c.cursor, self.cursor);
        set(&mut c.message_error_bg, self.message_error_bg);
        set(&mut c.message_warning_bg, self.message_warning_bg);
        for (role, paint) in &self.bars {
            c.bars.insert(*role, *paint);
        }
    }
}

/// Surcharge partielle des couleurs, par variante.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct VariantsPatch {
    /// Thème clair.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub light: Option<ColorTokensPatch>,
    /// Thème sombre.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dark: Option<ColorTokensPatch>,
    /// Impression (part de la variante claire si le modèle n'en a pas).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub print: Option<ColorTokensPatch>,
}

/// Surcharges d'un style : mêmes sections que [`PlanStyle`], facultatives ;
/// `bars` et `texts` remplacent les rôles nommés seulement.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct StylePatch {
    /// Nom affiché.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Couleurs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colors: Option<VariantsPatch>,
    /// Formes des barres remplacées, par rôle.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub bars: BTreeMap<BarRole, BarStyle>,
    /// Textes remplacés, par rôle.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub texts: BTreeMap<BarRole, BarTexts>,
    /// Échelle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<ScaleStyle>,
    /// Grille.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid: Option<GridStyle>,
    /// Densité.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub density: Option<Density>,
    /// Police.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font: Option<FontStyle>,
    /// Légende.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legend: Option<LegendStyle>,
    /// Mise en page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<PageStyle>,
    /// Couleurs de phase (H-UI-65), remplacées en entier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phases: Option<BTreeMap<u32, PhaseColor>>,
    /// Tâches de la chronologie (ADR-0017 D7), remplacées en entier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeline: Option<TimelineSpec>,
}

impl StylePatch {
    /// Vrai si la surcharge ne change rien.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Style d'un document (H-R12) : `{"v":1,"base":"sobre","over":{…}}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocStyle {
    /// Version du format.
    pub v: u16,
    /// Modèle de base.
    pub base: TemplateId,
    /// Surcharges.
    #[serde(default, skip_serializing_if = "StylePatch::is_empty")]
    pub over: StylePatch,
}

impl DocStyle {
    /// Style résolu.
    #[must_use]
    pub fn resolve(&self) -> PlanStyle {
        resolve(self.base, &self.over)
    }

    /// JSON compact canonique (H-R12) : clés triées à tous les niveaux,
    /// `>` écrit `\u003e` (une instruction de traitement ne peut pas
    /// contenir `?>`). Mêmes octets pour le même style.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        if let Ok(value) = serde_json::to_value(self) {
            write_canonical(&value, &mut out);
        }
        out
    }
}

/// Écrit `value` en JSON compact, clés d'objet triées (indépendant de la
/// fonction `preserve_order` de `serde_json`), `>` échappé.
fn write_canonical(value: &serde_json::Value, out: &mut String) {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, key) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_json_string(key, out);
                out.push(':');
                if let Some(v) = map.get(key) {
                    write_canonical(v, out);
                }
            }
            out.push('}');
        }
        serde_json::Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        serde_json::Value::String(s) => push_json_string(s, out),
        other => out.push_str(&other.to_string()),
    }
}

fn push_json_string(s: &str, out: &mut String) {
    let quoted = serde_json::Value::String(s.to_owned()).to_string();
    out.push_str(&quoted.replace('>', "\\u003e"));
}

/// Erreur de lecture ou de validation d'un style.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StyleError {
    /// Plus de [`MAX_DOC_STYLE_BYTES`] octets.
    TooLarge {
        /// Taille lue.
        bytes: usize,
    },
    /// JSON invalide ou champ inconnu.
    Syntax(String),
    /// Version inconnue.
    UnknownVersion(u16),
    /// Valeur hors bornes (champ nommé).
    OutOfBounds(&'static str),
    /// Rôle de barre absent (style incomplet).
    MissingRole(BarRole),
}

impl fmt::Display for StyleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { bytes } => write!(f, "style trop grand ({bytes} octets, 65 536 au plus)"),
            Self::Syntax(e) => write!(f, "style illisible : {e}"),
            Self::UnknownVersion(v) => write!(f, "version de style inconnue : {v}"),
            Self::OutOfBounds(field) => write!(f, "valeur hors bornes : {field}"),
            Self::MissingRole(role) => write!(f, "rôle de barre absent : {role:?}"),
        }
    }
}

impl std::error::Error for StyleError {}

fn check(ok: bool, field: &'static str) -> Result<(), StyleError> {
    if ok {
        Ok(())
    } else {
        Err(StyleError::OutOfBounds(field))
    }
}

impl PlanStyle {
    /// Vérifie les bornes et la complétude (un rôle de chaque sorte).
    ///
    /// # Errors
    ///
    /// La première valeur hors bornes ou le premier rôle absent.
    pub fn validate(&self) -> Result<(), StyleError> {
        if self.v != STYLE_VERSION {
            return Err(StyleError::UnknownVersion(self.v));
        }
        check(self.name.chars().count() <= MAX_NAME_CHARS, "name")?;
        for role in BarRole::ALL {
            let shape = self.bars.get(&role).ok_or(StyleError::MissingRole(role))?;
            check(shape.outline16 <= 64, "bars.outline16")?;
            check((10..=100).contains(&shape.height_pct), "bars.height_pct")?;
            check(shape.radius16 <= 128, "bars.radius16")?;
            for variant in [Variant::Light, Variant::Dark, Variant::Print] {
                if !self.colors.get(variant).bars.contains_key(&role) {
                    return Err(StyleError::MissingRole(role));
                }
            }
        }
        check((160..=320).contains(&self.font.size16), "font.size16")?;
        let page = &self.page;
        check(page.margins_mm.iter().all(|m| *m <= 50), "page.margins_mm")?;
        let fit_ok = match page.fit {
            Fit::Scale(s) => (10..=400).contains(&s),
            Fit::PagesWide(w) => (1..=20).contains(&w),
            Fit::PagesWideTall(w, h) => (1..=20).contains(&w) && (1..=20).contains(&h),
        };
        check(fit_ok, "page.fit")?;
        let slots_ok = page
            .header
            .iter()
            .chain(page.footer.iter())
            .all(|s| s.0.chars().count() <= MAX_SLOT_CHARS);
        check(slots_ok, "page.header/footer")?;
        if let Some(logo) = &page.logo {
            check(logo.path.chars().count() <= MAX_LOGO_REF_CHARS, "page.logo")?;
        }
        if let Some(columns) = &page.columns {
            let distinct = columns.iter().collect::<std::collections::BTreeSet<_>>().len();
            check(
                columns.len() <= MAX_TABLE_COLUMNS && distinct == columns.len(),
                "page.columns",
            )?;
        }
        check(self.phases.len() <= MAX_PHASES, "phases")?;
        check(self.timeline.is_valid(), "timeline")?;
        Ok(())
    }

    /// Couleurs de la variante `v`.
    #[must_use]
    pub fn tokens(&self, v: Variant) -> &ColorTokens {
        self.colors.get(v)
    }

    /// Style de secours, sans fichier : texte noir sur blanc, barres grises.
    /// Servi seulement si un modèle intégré était illisible (impossible tant
    /// que `templates_parse_and_validate` passe).
    #[must_use]
    pub fn fallback() -> Self {
        let black = Rgb(0, 0, 0);
        let white = Rgb(0xFF, 0xFF, 0xFF);
        let gray = Rgb(0x59, 0x59, 0x59);
        let light_gray = Rgb(0xE6, 0xE6, 0xE6);
        let bars: BTreeMap<BarRole, BarPaint> = BarRole::ALL
            .into_iter()
            .map(|r| {
                (
                    r,
                    BarPaint {
                        fill: gray,
                        outline: None,
                    },
                )
            })
            .collect();
        let tokens = |bg: Rgb, fg: Rgb| ColorTokens {
            bg,
            text: fg,
            text_strong: fg,
            text_muted: fg,
            header_bg: bg,
            grid: light_gray,
            grid_major: gray,
            band: [bg, bg],
            non_working: bg,
            status_line: fg,
            today_line: fg,
            link: fg,
            link_critical: fg,
            selection: light_gray,
            cursor: fg,
            message_error_bg: bg,
            message_warning_bg: bg,
            bars: bars.clone(),
        };
        let shape = BarStyle {
            outline16: 0,
            shape: BarShape::Bar,
            height_pct: 45,
            radius16: 0,
            pattern: Pattern::Solid,
        };
        Self {
            v: STYLE_VERSION,
            name: String::new(),
            base: None,
            colors: Variants {
                light: tokens(white, black),
                dark: tokens(black, white),
                print: None,
            },
            bars: BarRole::ALL.into_iter().map(|r| (r, shape)).collect(),
            texts: BTreeMap::new(),
            scale: ScaleStyle {
                tiers: TierPreset::Auto,
                week_numbers: false,
                shade_non_working: false,
                status_label: false,
                today_mark: false,
                relative_months: false,
            },
            grid: GridStyle {
                rows: RowBanding::None,
                major_v: true,
                minor_v: false,
                row_lines: false,
            },
            density: Density::Normal,
            font: FontStyle {
                family: FontChoice::Interface,
                size16: 192,
            },
            legend: LegendStyle {
                show: false,
                every_page: false,
            },
            page: PageStyle {
                paper: Paper::A4,
                landscape: true,
                margins_mm: [10; 4],
                fit: Fit::PagesWide(1),
                repeat_table: true,
                header: Default::default(),
                footer: Default::default(),
                logo: None,
                columns: None,
                title_block: false,
                timeline: false,
            },
            phases: BTreeMap::new(),
            timeline: TimelineSpec::default(),
        }
    }

    /// Applique une surcharge (sans valider).
    fn apply(&mut self, over: &StylePatch) {
        if let Some(name) = &over.name {
            self.name.clone_from(name);
        }
        if let Some(colors) = &over.colors {
            if let Some(p) = &colors.light {
                p.apply(&mut self.colors.light);
            }
            if let Some(p) = &colors.dark {
                p.apply(&mut self.colors.dark);
            }
            if let Some(p) = &colors.print {
                let mut print = self
                    .colors
                    .print
                    .take()
                    .unwrap_or_else(|| self.colors.light.clone());
                p.apply(&mut print);
                self.colors.print = Some(print);
            }
        }
        for (role, shape) in &over.bars {
            self.bars.insert(*role, *shape);
        }
        for (role, texts) in &over.texts {
            self.texts.insert(*role, *texts);
        }
        if let Some(v) = over.scale {
            self.scale = v;
        }
        if let Some(v) = over.grid {
            self.grid = v;
        }
        if let Some(v) = over.density {
            self.density = v;
        }
        if let Some(v) = over.font {
            self.font = v;
        }
        if let Some(v) = over.legend {
            self.legend = v;
        }
        if let Some(v) = &over.page {
            self.page = v.clone();
        }
        if let Some(v) = &over.phases {
            self.phases.clone_from(v);
        }
        if let Some(v) = &over.timeline {
            self.timeline.clone_from(v);
        }
    }
}

/// Modèles intégrés : identifiant et JSON (`templates/*.json`).
pub const TEMPLATES: [(TemplateId, &str); 6] = [
    (TemplateId::Classique, include_str!("../templates/classique.json")),
    (TemplateId::Sobre, include_str!("../templates/sobre.json")),
    (
        TemplateId::ContrasteEleve,
        include_str!("../templates/contraste_eleve.json"),
    ),
    (
        TemplateId::Presentation,
        include_str!("../templates/presentation.json"),
    ),
    (TemplateId::Chantier, include_str!("../templates/chantier.json")),
    (
        TemplateId::ImpressionNb,
        include_str!("../templates/impression_nb.json"),
    ),
];

/// Lecture d'un modèle intégré, sans cache (tests).
fn parse_template(json: &str) -> Result<PlanStyle, StyleError> {
    let style: PlanStyle = serde_json::from_str(json).map_err(|e| StyleError::Syntax(e.to_string()))?;
    style.validate()?;
    Ok(style)
}

/// Modèle intégré `id` (lu une fois, puis gardé).
#[must_use]
pub fn template(id: TemplateId) -> PlanStyle {
    static CACHE: OnceLock<Vec<(TemplateId, PlanStyle)>> = OnceLock::new();
    let all = CACHE.get_or_init(|| {
        TEMPLATES
            .iter()
            .map(|(tid, json)| {
                (
                    *tid,
                    parse_template(json).unwrap_or_else(|_| PlanStyle::fallback()),
                )
            })
            .collect()
    });
    all.iter()
        .find(|(tid, _)| *tid == id)
        .map_or_else(PlanStyle::fallback, |(_, s)| s.clone())
}

/// Modèle `base` et surcharges `over`. Une surcharge qui sortirait des bornes
/// est ignorée en entier (le modèle seul est rendu) : `parse_doc_style` la
/// refuse en amont avec un diagnostic.
#[must_use]
pub fn resolve(base: TemplateId, over: &StylePatch) -> PlanStyle {
    let plain = template(base);
    let mut style = plain.clone();
    style.apply(over);
    style.base = Some(base);
    if style.validate().is_ok() { style } else { plain }
}

/// Lit le style d'un document (H-R12) : au plus 64 Kio, champs inconnus
/// refusés, version 1, style résolu dans les bornes.
///
/// # Errors
///
/// [`StyleError`] : trop grand, illisible, version inconnue ou hors bornes.
pub fn parse_doc_style(json: &str) -> Result<DocStyle, StyleError> {
    if json.len() > MAX_DOC_STYLE_BYTES {
        return Err(StyleError::TooLarge { bytes: json.len() });
    }
    let doc: DocStyle = serde_json::from_str(json).map_err(|e| StyleError::Syntax(e.to_string()))?;
    if doc.v != STYLE_VERSION {
        return Err(StyleError::UnknownVersion(doc.v));
    }
    let mut style = template(doc.base);
    style.apply(&doc.over);
    style.validate()?;
    Ok(doc)
}

/// Luminance linéaire d'un canal sRGB 8 bits, en millionièmes (WCAG 2.2 :
/// `c/12,92` si `c ≤ 0,04045`, sinon `((c + 0,055)/1,055)^2,4`), arrondie.
/// Table testée contre la formule (`srgb_table_matches_formula`).
#[rustfmt::skip]
#[allow(clippy::unreadable_literal, reason = "table générée, une valeur par canal")]
const SRGB_LINEAR_PPM: [u32; 256] = [
    0, 304, 607, 911, 1214, 1518, 1821, 2125, 2428, 2732, 3035, 3347,
    3677, 4025, 4391, 4777, 5182, 5605, 6049, 6512, 6995, 7499, 8023, 8568,
    9134, 9721, 10330, 10960, 11612, 12286, 12983, 13702, 14444, 15209, 15996, 16807,
    17642, 18500, 19382, 20289, 21219, 22174, 23153, 24158, 25187, 26241, 27321, 28426,
    29557, 30713, 31896, 33105, 34340, 35601, 36889, 38204, 39546, 40915, 42311, 43735,
    45186, 46665, 48172, 49707, 51269, 52861, 54480, 56128, 57805, 59511, 61246, 63010,
    64803, 66626, 68478, 70360, 72272, 74214, 76185, 78187, 80220, 82283, 84376, 86500,
    88656, 90842, 93059, 95307, 97587, 99899, 102242, 104616, 107023, 109462, 111932, 114435,
    116971, 119538, 122139, 124772, 127438, 130136, 132868, 135633, 138432, 141263, 144128, 147027,
    149960, 152926, 155926, 158961, 162029, 165132, 168269, 171441, 174647, 177888, 181164, 184475,
    187821, 191202, 194618, 198069, 201556, 205079, 208637, 212231, 215861, 219526, 223228, 226966,
    230740, 234551, 238398, 242281, 246201, 250158, 254152, 258183, 262251, 266356, 270498, 274677,
    278894, 283149, 287441, 291771, 296138, 300544, 304987, 309469, 313989, 318547, 323143, 327778,
    332452, 337164, 341914, 346704, 351533, 356400, 361307, 366253, 371238, 376262, 381326, 386429,
    391572, 396755, 401978, 407240, 412543, 417885, 423268, 428690, 434154, 439657, 445201, 450786,
    456411, 462077, 467784, 473531, 479320, 485150, 491021, 496933, 502886, 508881, 514918, 520996,
    527115, 533276, 539479, 545724, 552011, 558340, 564712, 571125, 577580, 584078, 590619, 597202,
    603827, 610496, 617207, 623960, 630757, 637597, 644480, 651406, 658375, 665387, 672443, 679542,
    686685, 693872, 701102, 708376, 715694, 723055, 730461, 737911, 745404, 752942, 760525, 768151,
    775822, 783538, 791298, 799103, 806952, 814847, 822786, 830770, 838799, 846873, 854993, 863157,
    871367, 879622, 887923, 896269, 904661, 913099, 921582, 930111, 938686, 947307, 955973, 964686,
    973445, 982251, 991102, 1000000,
];

/// Luminance relative d'une couleur, en millionièmes (0 à 1 000 000).
#[must_use]
pub fn luminance_ppm(c: Rgb) -> u32 {
    let lin = |v: u8| u64::from(SRGB_LINEAR_PPM.get(usize::from(v)).copied().unwrap_or(0));
    // 0,2126 R + 0,7152 G + 0,0722 B, coefficients en dix-millièmes :
    // au plus 10 000 × 1 000 000, aucun débordement en u64.
    let weighted = lin(c.0)
        .saturating_mul(2126)
        .saturating_add(lin(c.1).saturating_mul(7152))
        .saturating_add(lin(c.2).saturating_mul(722));
    let rounded = weighted.saturating_add(5000) / 10_000;
    u32::try_from(rounded).unwrap_or(u32::MAX)
}

/// Rapport de contraste WCAG 2.2 entre deux couleurs, en millièmes
/// (4 500 = 4,5:1 ; 1 000 à 21 000), arrondi vers le bas.
#[must_use]
pub fn contrast_ratio(a: Rgb, b: Rgb) -> u32 {
    let (la, lb) = (u64::from(luminance_ppm(a)), u64::from(luminance_ppm(b)));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    let ratio = hi
        .saturating_add(50_000)
        .saturating_mul(1000)
        .checked_div(lo.saturating_add(50_000))
        .unwrap_or(1000);
    u32::try_from(ratio).unwrap_or(u32::MAX)
}

/// Mélange de `a` vers `b` à `t` % (0 : `a`, 100 : `b`), entier.
#[must_use]
pub fn blend(a: Rgb, b: Rgb, t: u8) -> Rgb {
    let t = u16::from(t.min(100));
    let mix = |x: u8, y: u8| {
        let v = u16::from(x)
            .saturating_mul(100_u16.saturating_sub(t))
            .saturating_add(u16::from(y).saturating_mul(t))
            .saturating_add(50)
            / 100;
        u8::try_from(v).unwrap_or(u8::MAX)
    };
    Rgb(mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2))
}

/// Fond lisible d'une ligne de phase dans la table (H-UI-65, H-UI-45) :
/// la couleur de la phase, éclaircie vers `bg` par pas de 5 % jusqu'à ce
/// que chaque encre de `inks` atteigne `min` (millièmes : 4 500 = 4,5:1)
/// dessus ; `bg` au pire.
#[must_use]
pub fn readable_tint(fill: Rgb, bg: Rgb, inks: &[Rgb], min: u32) -> Rgb {
    (0..=20_u8)
        .map(|step| blend(fill, bg, step.saturating_mul(5)))
        .find(|c| inks.iter().all(|ink| contrast_ratio(*ink, *c) >= min))
        .unwrap_or(bg)
}

/// Encre d'un texte dessiné **dans** une barre de remplissage `fill`
/// (H-UI-46 « dedans », H-UI-61) : l'encre du texte ou le fond, la plus
/// contrastée sur le remplissage (relecture finale de J8 : texte noir sur
/// barre noire en Impression N&B, 2,8:1 sur la barre bleue de Sobre).
#[must_use]
pub fn ink_on(fill: Rgb, text: Rgb, bg: Rgb) -> Rgb {
    if contrast_ratio(text, fill) >= contrast_ratio(bg, fill) {
        text
    } else {
        bg
    }
}

/// Contraste minimal du texte d'un style (H-UI-45) : 7:1 pour Contraste
/// élevé, 4,5:1 sinon (millièmes).
#[must_use]
pub fn min_text_contrast(st: &PlanStyle) -> u32 {
    if st.base == Some(TemplateId::ContrasteEleve) {
        7_000
    } else {
        4_500
    }
}

/// Fond d'une ligne de phase de couleur `fill` dans la table, variante
/// `v` (H-UI-65) : teinte lisible pour les trois encres de la table
/// (texte, appuyé, atténué d'une ligne exclue ou inactive) au seuil du
/// style (relecture finale de J8 : le texte atténué et le 7:1 de
/// Contraste élevé n'étaient pas pris en compte).
#[must_use]
pub fn phase_row_tint(st: &PlanStyle, v: Variant, fill: Rgb) -> Rgb {
    let c = st.tokens(v);
    readable_tint(
        fill,
        c.bg,
        &[c.text, c.text_strong, c.text_muted],
        min_text_contrast(st),
    )
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::float_arithmetic,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "tests : formule flottante de référence, échecs lisibles"
)]
mod tests {
    use super::*;

    fn srgb_linear(c: u8) -> f64 {
        let c = f64::from(c) / 255.0;
        if c <= 0.040_45 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    #[test]
    fn srgb_table_matches_formula() {
        for v in 0..=255_u8 {
            let expected = (srgb_linear(v) * 1_000_000.0).round() as i64;
            let got = i64::from(SRGB_LINEAR_PPM[usize::from(v)]);
            assert!((expected - got).abs() <= 1, "{v} : {got} ≠ {expected}");
        }
    }

    /// Exemples WCAG : #777 sur #FFF ≈ 4,48:1 ; noir sur blanc 21:1.
    #[test]
    fn contrast_ratio_matches_wcag_examples() {
        let white = Rgb(0xFF, 0xFF, 0xFF);
        let ratio = contrast_ratio(Rgb(0x77, 0x77, 0x77), white);
        assert!((4477..=4479).contains(&ratio), "{ratio}");
        assert_eq!(contrast_ratio(Rgb(0, 0, 0), white), 21_000);
        assert_eq!(contrast_ratio(white, Rgb(0, 0, 0)), 21_000);
        assert_eq!(contrast_ratio(white, white), 1000);
        // Exemple de H-UI-45 : texte de Sobre sur blanc ≥ 4,5:1.
        assert!(contrast_ratio(Rgb(0x1F, 0x29, 0x33), white) >= 4500);
    }

    #[test]
    fn contrast_ratio_matches_float_formula() {
        let colors = [
            Rgb(0x3F, 0x7C, 0xC4),
            Rgb(0xC9, 0x30, 0x2C),
            Rgb(0x1E, 0x1F, 0x22),
            Rgb(0xE6, 0xE6, 0xE6),
            Rgb(0x00, 0x72, 0xB2),
            Rgb(0x80, 0x80, 0x80),
        ];
        for a in colors {
            for b in colors {
                let l = |c: Rgb| {
                    0.2126 * srgb_linear(c.0) + 0.7152 * srgb_linear(c.1) + 0.0722 * srgb_linear(c.2)
                };
                let (x, y) = (l(a), l(b));
                let float = (x.max(y) + 0.05) / (x.min(y) + 0.05) * 1000.0;
                let got = f64::from(contrast_ratio(a, b));
                assert!((float - got).abs() <= 2.0, "{a:?} {b:?} : {got} ≠ {float}");
            }
        }
    }

    #[test]
    fn rgb_parse_and_hex() {
        assert_eq!(Rgb::parse("#1f2933"), Some(Rgb(0x1F, 0x29, 0x33)));
        assert_eq!(Rgb(0x1F, 0x29, 0x33).hex(), "#1F2933");
        for bad in [
            "", "#", "1F2933", "#1F293", "#1F29334", "#GG0000", "#é00000", "#+12345",
        ] {
            assert_eq!(Rgb::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn templates_parse_and_validate() {
        assert_eq!(TEMPLATES.len(), TemplateId::ALL.len());
        for (id, json) in TEMPLATES {
            let style = parse_template(json).unwrap_or_else(|e| panic!("{id:?} : {e}"));
            assert_eq!(style.base, Some(id), "{id:?}");
            assert_eq!(template(id), style, "{id:?}");
            assert_eq!(TemplateId::from_key(id.key()), Some(id));
            // Aller-retour JSON.
            let again: PlanStyle = serde_json::from_str(&serde_json::to_string(&style).unwrap()).unwrap();
            assert_eq!(again, style, "{id:?}");
        }
        assert!(PlanStyle::fallback().validate().is_ok());
    }

    /// Couleur visible d'une barre : contour si le motif est « contour seul »,
    /// sinon le meilleur du remplissage et du contour (≥ 1 px).
    fn bar_visible_ratio(style: &PlanStyle, tokens: &ColorTokens, role: BarRole, bg: Rgb) -> u32 {
        let shape = style.bars[&role];
        let paint = tokens.bars[&role];
        let outline = paint.outline.filter(|_| shape.outline16 >= 16);
        if shape.pattern == Pattern::Outline {
            return contrast_ratio(outline.unwrap_or(paint.fill), bg);
        }
        let fill = contrast_ratio(paint.fill, bg);
        outline.map_or(fill, |o| fill.max(contrast_ratio(o, bg)))
    }

    /// Manques de contraste d'un modèle (libellés triés).
    fn contrast_failures(id: TemplateId) -> Vec<String> {
        let style = template(id);
        let text_min = if id == TemplateId::ContrasteEleve {
            7000
        } else {
            4500
        };
        let mut out = Vec::new();
        for (vname, variant) in [
            ("light", Variant::Light),
            ("dark", Variant::Dark),
            ("print", Variant::Print),
        ] {
            let c = style.tokens(variant);
            for (bname, bg) in [
                ("bg", c.bg),
                ("header_bg", c.header_bg),
                ("selection", c.selection),
            ] {
                for (tname, text) in [
                    ("text", c.text),
                    ("text_strong", c.text_strong),
                    ("text_muted", c.text_muted),
                ] {
                    if contrast_ratio(text, bg) < text_min {
                        out.push(format!("{vname}:{tname}/{bname}"));
                    }
                }
            }
            for (bname, bg) in [("bg", c.bg), ("band1", c.band[1]), ("non_working", c.non_working)] {
                for (gname, g) in [
                    ("link", c.link),
                    ("link_critical", c.link_critical),
                    ("status_line", c.status_line),
                    ("today_line", c.today_line),
                    ("cursor", c.cursor),
                ] {
                    if contrast_ratio(g, bg) < 3000 {
                        out.push(format!("{vname}:{gname}/{bname}"));
                    }
                }
                for role in BarRole::ALL {
                    if role != BarRole::Progress && bar_visible_ratio(&style, c, role, bg) < 3000 {
                        out.push(format!("{vname}:{role:?}/{bname}"));
                    }
                }
            }
            let progress = c.bars[&BarRole::Progress].fill;
            for role in [BarRole::Task, BarRole::Critical] {
                if contrast_ratio(progress, c.bars[&role].fill) < 3000 {
                    out.push(format!("{vname}:Progress/{role:?}"));
                }
            }
        }
        out
    }

    /// H-UI-45 : pour chaque modèle et variante, texte ≥ 4,5:1 (Contraste
    /// élevé ≥ 7:1) sur le fond, l'en-tête et la sélection ; barres, jalons,
    /// liens, date d'état, aujourd'hui et curseur ≥ 3:1 sur le fond, le fond
    /// alterné et les jours chômés ; avancement ≥ 3:1 dans la barre.
    /// Classique reproduit J7 tel quel : ses manques sont consignés ici (et
    /// dans ADR-0016), pas corrigés.
    #[test]
    fn h_ui_45_contrast_all_templates() {
        for id in TemplateId::ALL {
            let failures = contrast_failures(id);
            let expected: &[&str] = if id == TemplateId::Classique {
                &[
                    "light:text_muted/selection",
                    "dark:text_muted/selection",
                    "print:text_muted/selection",
                ]
            } else {
                &[]
            };
            assert_eq!(failures, expected, "{id:?}");
        }
    }

    /// H-UI-44 : le critique se distingue autrement que par la couleur
    /// (contour ≥ 2 px ou motif), sauf Classique (J7, consigné).
    #[test]
    fn h_ui_44_critical_not_color_only() {
        for id in TemplateId::ALL {
            let style = template(id);
            for (crit, plain) in [
                (BarRole::Critical, BarRole::Task),
                (BarRole::MilestoneCritical, BarRole::Milestone),
            ] {
                let (c, p) = (style.bars[&crit], style.bars[&plain]);
                let distinct = c.pattern != p.pattern || (c.outline16 >= 32 && c.outline16 > p.outline16);
                assert_eq!(distinct, id != TemplateId::Classique, "{id:?} {crit:?}");
            }
        }
    }

    #[test]
    fn doc_style_roundtrip_and_bounds() {
        let json = r##"{"base":"sobre","over":{"colors":{"light":{"bars":{"critical":{"fill":"#AA0000"}}}},"density":"compact","name":"a>b"},"v":1}"##;
        let doc = parse_doc_style(json).unwrap();
        assert_eq!(doc.base, TemplateId::Sobre);
        assert_eq!(doc.to_json(), json.replace('>', "\\u003e"));
        assert_eq!(parse_doc_style(&doc.to_json()).unwrap(), doc);
        let style = doc.resolve();
        assert_eq!(style.density, Density::Compact);
        assert_eq!(
            style.tokens(Variant::Light).bars[&BarRole::Critical].fill,
            Rgb(0xAA, 0, 0)
        );
        assert_eq!(
            style.tokens(Variant::Dark),
            template(TemplateId::Sobre).tokens(Variant::Dark)
        );

        assert_eq!(
            parse_doc_style(r#"{"v":2,"base":"sobre"}"#),
            Err(StyleError::UnknownVersion(2))
        );
        assert!(matches!(
            parse_doc_style(r#"{"v":1,"base":"sobre","x":1}"#),
            Err(StyleError::Syntax(_))
        ));
        assert!(matches!(
            parse_doc_style(r#"{"v":1,"base":"inconnu"}"#),
            Err(StyleError::Syntax(_))
        ));
        let big = format!(
            r#"{{"v":1,"base":"sobre","over":{{"name":"{}"}}}}"#,
            "a".repeat(MAX_DOC_STYLE_BYTES)
        );
        assert!(matches!(parse_doc_style(&big), Err(StyleError::TooLarge { .. })));
        assert_eq!(
            parse_doc_style(r#"{"v":1,"base":"sobre","over":{"font":{"family":"inter","size16":9}}}"#),
            Err(StyleError::OutOfBounds("font.size16"))
        );
        // Surcharge hors bornes : resolve rend le modèle seul.
        let bad = StylePatch {
            density: Some(Density::Airy),
            font: Some(FontStyle {
                family: FontChoice::Inter,
                size16: 1,
            }),
            ..StylePatch::default()
        };
        assert_eq!(resolve(TemplateId::Sobre, &bad), template(TemplateId::Sobre));
    }

    /// Champs de L15 : absents du JSON quand ils valent leur défaut (un
    /// style écrit avant L15 garde ses octets), relus, bornés.
    #[test]
    fn l15_fields_roundtrip_and_bounds() {
        let plain = r#"{"base":"sobre","over":{"density":"compact"},"v":1}"#;
        assert_eq!(parse_doc_style(plain).unwrap().to_json(), plain);
        let mut page = template(TemplateId::Sobre).page;
        page.columns = Some(TableColumn::PRINT_DEFAULT.to_vec());
        page.title_block = true;
        let mut phases = BTreeMap::new();
        phases.insert(
            7,
            PhaseColor {
                fill: Rgb(0xE0, 0x30, 0x20),
                children: true,
            },
        );
        let doc = DocStyle {
            v: STYLE_VERSION,
            base: TemplateId::Sobre,
            over: StylePatch {
                page: Some(page),
                phases: Some(phases.clone()),
                ..StylePatch::default()
            },
        };
        let json = doc.to_json();
        assert!(
            json.contains(r#""columns":["id","name","duration","start","finish"]"#),
            "{json}"
        );
        assert!(
            json.contains(r##""phases":{"7":{"children":true,"fill":"#E03020"}}"##),
            "{json}"
        );
        let back = parse_doc_style(&json).unwrap();
        assert_eq!(back, doc);
        assert_eq!(back.resolve().phases, phases);
        // Doublon de colonne : hors bornes.
        let mut bad = template(TemplateId::Sobre);
        bad.page.columns = Some(vec![TableColumn::Id, TableColumn::Id]);
        assert_eq!(bad.validate(), Err(StyleError::OutOfBounds("page.columns")));
        let mut many = template(TemplateId::Sobre);
        many.phases = (0..=u32::try_from(MAX_PHASES).unwrap())
            .map(|i| {
                (
                    i,
                    PhaseColor {
                        fill: Rgb(0, 0, 0),
                        children: false,
                    },
                )
            })
            .collect();
        assert_eq!(many.validate(), Err(StyleError::OutOfBounds("phases")));
    }

    /// H-UI-65 : le fond d'une ligne de phase garde le texte lisible.
    #[test]
    fn h_ui_65_readable_tint() {
        let white = Rgb(0xFF, 0xFF, 0xFF);
        let text = Rgb(0x1F, 0x29, 0x33);
        let red = Rgb(0xFF, 0, 0);
        let tint = readable_tint(red, white, &[text], 4_500);
        assert!(contrast_ratio(text, tint) >= 4_500);
        assert_ne!(tint, white, "le rouge reste visible");
        // Couleur déjà lisible : gardée telle quelle.
        let pale = Rgb(0xFF, 0xF0, 0xC0);
        assert_eq!(readable_tint(pale, white, &[text], 4_500), pale);
        assert_eq!(blend(red, white, 0), red);
        assert_eq!(blend(red, white, 100), white);
    }

    #[test]
    fn print_patch_starts_from_light() {
        let patch = StylePatch {
            colors: Some(VariantsPatch {
                print: Some(ColorTokensPatch {
                    bg: Some(Rgb(0xFA, 0xFA, 0xFA)),
                    ..ColorTokensPatch::default()
                }),
                ..VariantsPatch::default()
            }),
            ..StylePatch::default()
        };
        let style = resolve(TemplateId::Sobre, &patch);
        let print = style.tokens(Variant::Print);
        assert_eq!(print.bg, Rgb(0xFA, 0xFA, 0xFA));
        assert_eq!(print.text, style.tokens(Variant::Light).text);
    }

    /// ADR-0017 (lot L0) : `timeline` et `page.timeline` absents du JSON
    /// à leur défaut (un style écrit avant gardé à l'octet), relus à
    /// l'identique, `timeline` remplacée en entier par une surcharge.
    #[test]
    fn style_timeline_roundtrip() {
        let plain = r#"{"base":"sobre","over":{"density":"compact"},"v":1}"#;
        let doc = parse_doc_style(plain).unwrap();
        assert_eq!(doc.to_json(), plain);
        assert!(doc.resolve().timeline.is_empty() && !doc.resolve().page.timeline);
        let mut page = template(TemplateId::Sobre).page;
        page.timeline = true;
        let doc = DocStyle {
            v: STYLE_VERSION,
            base: TemplateId::Sobre,
            over: StylePatch {
                page: Some(page),
                timeline: Some(TimelineSpec { tasks: vec![3, 4] }),
                ..StylePatch::default()
            },
        };
        let json = doc.to_json();
        assert!(json.contains(r#""timeline":{"tasks":[3,4]}"#), "{json}");
        assert!(json.contains(r#""timeline":true"#), "{json}");
        let back = parse_doc_style(&json).unwrap();
        assert_eq!(back, doc);
        let resolved = back.resolve();
        assert_eq!(resolved.timeline.tasks, vec![3, 4]);
        assert!(resolved.page.timeline);
        // Style résolu sérialisé puis relu : identité.
        let text = serde_json::to_string(&resolved).unwrap();
        assert_eq!(serde_json::from_str::<PlanStyle>(&text).unwrap(), resolved);
        // Surcharge vide : la liste est vidée (remplacement en entier).
        let mut style = resolved;
        style.apply(&StylePatch {
            timeline: Some(TimelineSpec::default()),
            ..StylePatch::default()
        });
        assert!(style.timeline.is_empty());
    }

    /// ADR-0017 (lot L0) : 201 UID, doublons ou désordre ⇒ hors bornes
    /// (le style du document est refusé avec diagnostic), jamais de
    /// panique ; `bounded` rend une liste valide.
    #[test]
    fn style_timeline_bounded() {
        let mut style = template(TemplateId::Sobre);
        style.timeline.tasks = (1..=u32::try_from(MAX_TIMELINE_TASKS).unwrap()).collect();
        assert_eq!(style.validate(), Ok(()));
        for bad in [
            (1..=u32::try_from(MAX_TIMELINE_TASKS + 1).unwrap()).collect::<Vec<_>>(),
            vec![3, 3],
            vec![4, 3],
        ] {
            style.timeline.tasks.clone_from(&bad);
            assert_eq!(
                style.validate(),
                Err(StyleError::OutOfBounds("timeline")),
                "{bad:?}"
            );
            let json = format!(
                r#"{{"base":"sobre","over":{{"timeline":{{"tasks":{}}}}},"v":1}}"#,
                serde_json::to_string(&bad).unwrap()
            );
            assert_eq!(parse_doc_style(&json), Err(StyleError::OutOfBounds("timeline")));
            assert!(TimelineSpec::bounded(bad).is_valid());
        }
        let bounded = TimelineSpec::bounded((0..500).rev().chain([7, 7]).collect());
        assert_eq!(bounded.tasks.len(), MAX_TIMELINE_TASKS);
        assert_eq!(bounded.tasks.first(), Some(&0));
        // Champ inconnu refusé (deny_unknown_fields).
        let unknown = r#"{"base":"sobre","over":{"timeline":{"tasks":[],"x":1}},"v":1}"#;
        assert!(matches!(parse_doc_style(unknown), Err(StyleError::Syntax(_))));
    }
}
