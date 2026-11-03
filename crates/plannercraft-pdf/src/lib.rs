//! Export PDF du planning (ADR-0016 D8).
//!
//! Couche du rendu : ne dépend ni d'egui ni de `plannercraft-automation` (règle 9).
//! Écrit avec krilla 0.8.2 sans façonnage : chaque glyphe est placé à
//! l'avance de la table de largeurs d'Inter (H-R11), pour que coupures et
//! points de suspension soient identiques au SVG. Sortie déterministe (H-R13) :
//! date de création et langue injectées, identifiant du document tiré d'un
//! hachage stable des pages, aucune horloge ni aléa.
//!
//! [`pdf`] lit les [`Pages`] de `plannercraft_render::paginate` : sur chaque page,
//! l'échelle et les titres de colonnes (H-UI-49), le parent rappelé
//! « (suite) » (H-UI-48), les lignes de la bande, la ligne de troncature, la
//! légende (H-UI-47), l'en-tête et le pied de page (H-UI-51). Les motifs
//! (hachures, points) sont des chemins. Logo : non dessiné (première coupe
//! du lot L7, `render.md` § 5.4).
#![forbid(unsafe_code)]

mod draw;
mod fonts;
mod smoke;

use krilla::Document;
use krilla::metadata::{DateTime, Metadata};
use krilla::page::PageSettings;
use plannercraft_core::Moment;
use plannercraft_render::Lang;
use plannercraft_render::paginate::{MAX_PAGES, Pages};

pub use fonts::MAX_MISSING_LISTED;

pub use smoke::smoke_page;

/// Inter Regular embarquée (`assets/fonts`, OFL-1.1, `NOTICE`).
pub static INTER_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
/// Inter `SemiBold` embarquée.
pub static INTER_SEMIBOLD: &[u8] = include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf");

/// Métadonnées du document (H-R13 : date et identifiant injectés, jamais
/// d'horloge).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PdfMeta {
    /// Titre.
    pub title: String,
    /// Date de création, fournie par l'appelant.
    pub created: Moment,
    /// Langue du document.
    pub lang: Lang,
}

/// Polices du PDF ; `fallback` : police de l'utilisateur pour les
/// caractères absents d'Inter (CJK, desktop seulement), sinon
/// `pdf.glyph_missing`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontSet {
    /// Texte courant.
    pub regular: &'static [u8],
    /// Texte appuyé.
    pub semibold: &'static [u8],
    /// Police de repli.
    pub fallback: Option<Vec<u8>>,
}

impl FontSet {
    /// Inter seule (Regular et `SemiBold` du dépôt).
    #[must_use]
    pub fn inter() -> Self {
        Self {
            regular: INTER_REGULAR,
            semibold: INTER_SEMIBOLD,
            fallback: None,
        }
    }
}

/// Rapport d'un export PDF.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PdfReport {
    /// Pages écrites.
    pub pages: usize,
    /// Lignes de la scène exportées.
    pub rows: usize,
    /// Taille du document.
    pub bytes: usize,
    /// Caractères absents des polices (`pdf.glyph_missing`), dans l'ordre
    /// des points de code, au plus [`MAX_MISSING_LISTED`] ; dessinés par le
    /// glyphe 0 (boîte vide), texte extractible gardé.
    pub missing_glyphs: Vec<char>,
    /// Caractères absents en tout.
    pub missing_total: usize,
}

/// Écrit le PDF des pages (H-R13, H-UI-47 à H-UI-51).
///
/// # Errors
///
/// [`PdfError::TooManyPages`] au-delà de [`MAX_PAGES`] pages ;
/// [`PdfError::Geometry`] sans page ou papier nul ; [`PdfError::FontInvalid`]
/// pour une police illisible ; [`PdfError::Write`] si krilla refuse le
/// document.
pub fn pdf(pages: &Pages, fonts: &FontSet, meta: &PdfMeta) -> Result<(Vec<u8>, PdfReport), PdfError> {
    if pages.pages.len() > MAX_PAGES {
        return Err(PdfError::TooManyPages(pages.pages.len()));
    }
    if pages.pages.is_empty() {
        return Err(PdfError::Geometry);
    }
    let mut loaded = fonts::Fonts::load(fonts)?;
    let mut document = Document::new();
    document.set_metadata(metadata(pages, meta));
    let (w, h) = pages.paper_pt;
    let settings = PageSettings::from_wh(fonts::to_f32(i64::from(w)), fonts::to_f32(i64::from(h)))
        .ok_or(PdfError::Geometry)?;
    let mut ctx = draw::PageCtx {
        pages,
        fonts: &mut loaded,
        lang: meta.lang,
    };
    for page in &pages.pages {
        let mut p = document.start_page_with(settings.clone());
        let mut surface = p.surface();
        draw::draw_page(&mut surface, &mut ctx, page);
        surface.finish();
        p.finish();
    }
    let bytes = document.finish().map_err(|e| PdfError::Write(format!("{e:?}")))?;
    let missing_total = loaded.missing.len();
    let report = PdfReport {
        pages: pages.pages.len(),
        rows: pages.scene.rows.len(),
        bytes: bytes.len(),
        missing_glyphs: loaded.missing.into_iter().take(MAX_MISSING_LISTED).collect(),
        missing_total,
    };
    Ok((bytes, report))
}

/// Métadonnées : titre, langue, date de création injectée, producteur,
/// identifiant du document (16 octets d'un hachage stable des pages, en
/// hexadécimal).
fn metadata(pages: &Pages, meta: &PdfMeta) -> Metadata {
    let mut m = Metadata::new()
        .title(meta.title.clone())
        .language(
            match meta.lang {
                Lang::Fr => "fr",
                Lang::En => "en",
            }
            .to_owned(),
        )
        .producer(format!("PlannerCraft {}", plannercraft_core::PRODUCT_VERSION))
        .document_id(document_id(pages));
    if let Some(dt) = datetime(meta.created) {
        m = m.creation_date(dt);
    }
    m
}

/// Date krilla d'un instant (`AAAA-MM-JJTHH:MM`, heure locale du projet).
fn datetime(m: Moment) -> Option<DateTime> {
    let s = m.to_string();
    let num = |a: usize, b: usize| s.get(a..b).and_then(|v| v.parse::<u16>().ok());
    let year = num(0, 4)?;
    let byte = |a: usize, b: usize| num(a, b).and_then(|v| u8::try_from(v).ok());
    Some(
        DateTime::new(year)
            .month(byte(5, 7)?)
            .day(byte(8, 10)?)
            .hour(byte(11, 13)?)
            .minute(byte(14, 16)?),
    )
}

/// Hachage FNV-1a 64 bits alimenté par l'écriture `Debug` des pages, sans
/// chaîne intermédiaire.
struct Fnv(u64);

impl std::fmt::Write for Fnv {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        for b in s.bytes() {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01B3);
        }
        Ok(())
    }
}

/// Identifiant du document : deux FNV-1a 64 bits (bases différentes) sur
/// le contenu des pages, soit 16 octets en hexadécimal (H-R13).
fn document_id(pages: &Pages) -> String {
    use std::fmt::Write as _;
    let mut a = Fnv(0xCBF2_9CE4_8422_2325);
    let mut b = Fnv(0x6C62_272E_07BB_0142);
    let _ = write!(a, "{pages:?}");
    let _ = write!(b, "{pages:?}");
    format!("{:016x}{:016x}", a.0, b.0)
}

/// Erreur de l'export PDF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdfError {
    /// Police illisible (fichier tronqué ou non OpenType).
    FontInvalid,
    /// Dimension de page ou de forme refusée par krilla (non finie, nulle).
    Geometry,
    /// Échec de l'écriture du document par krilla.
    Write(String),
    /// Plus de pages que `MAX_PAGES` (`pdf.too_many_pages`).
    TooManyPages(usize),
    /// Caractère absent des polices (`pdf.glyph_missing`).
    GlyphMissing(char),
    /// Logo illisible ou hors bornes (`pdf.logo_invalid`).
    LogoInvalid,
    /// Fonction posée par le contrat (L0), livrée par un lot ultérieur.
    NotYetAvailable,
}

impl std::fmt::Display for PdfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FontInvalid => f.write_str("police illisible"),
            Self::Geometry => f.write_str("géométrie refusée"),
            Self::Write(e) => write!(f, "écriture du PDF : {e}"),
            Self::TooManyPages(n) => write!(f, "{n} pages, 1 000 au plus"),
            Self::GlyphMissing(c) => write!(f, "caractère absent des polices : U+{:04X}", u32::from(*c)),
            Self::LogoInvalid => f.write_str("logo illisible"),
            Self::NotYetAvailable => f.write_str("export PDF pas encore disponible"),
        }
    }
}

impl std::error::Error for PdfError {}
