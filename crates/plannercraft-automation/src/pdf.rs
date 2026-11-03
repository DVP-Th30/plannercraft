//! `project.export_pdf` (J8, lot L7, ADR-0016 D8) : le planning de la
//! session en PDF, par le même chemin que le SVG (`layout`), paginé
//! (`paginate`, H-UI-48 à H-UI-51) puis écrit (`plannercraft_pdf::pdf`, H-R13).
//!
//! Style : modèle demandé, sinon celui du document, sinon Classique
//! (`HEADLESS_DEFAULT`) ; mise en page du style, papier, orientation et
//! « N pages en largeur » remplaçables par la requête ; variante
//! d'impression toujours. Textes autour des barres, en-tête et pied de page
//! formatés par `SessionTexts` (règle 3). Chronologie en tête de la
//! première page si le style la demande (`page.timeline`, H-UI-82,
//! `timeline::print_input`). Les octets sont rendus à
//! l'appelant (le routeur les écrit) : aucune entrée/sortie ici, wasm
//! compris.

use plannercraft_pdf::{FontSet, PdfError, PdfMeta};
use plannercraft_render::geometry::Viewport;
use plannercraft_render::paginate::{PageError, PaginateOptions, fit_zoom, layout_pages_timeline};
use plannercraft_render::style::{Fit, PageStyle, Paper, Variant, template};
use plannercraft_render::{Lang, RangeSpec, RenderLimits, RowFilter, Zoom};
use serde::Serialize;

use crate::command::params::{ExportPdfParams, PaperJson};
use crate::command::style;
use crate::error::codes;
use crate::render_text::SessionTexts;
use crate::session::Session;
use crate::text::Language;
use crate::workspace::ToolError;

/// Lignes exportées au plus (au-delà : ligne « … N tâches non affichées »).
pub const MAX_PDF_ROWS: u32 = 20_000;

/// PDF produit et son rapport.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PdfExport {
    /// Document.
    pub bytes: Vec<u8>,
    /// Rapport (sérialisé dans la réponse, chemin ajouté par le routeur).
    pub report: PdfExportReport,
}

/// Rapport d'un export PDF.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct PdfExportReport {
    /// Pages écrites.
    pub pages: usize,
    /// Tâches exportées.
    pub shown: u32,
    /// Tâches non exportées (filtre ou borne).
    pub hidden: u32,
    /// Plage de dates coupée (20 ans au plus).
    pub truncated_days: bool,
    /// Échelle du planning sur le papier, en % (arrondi inférieur).
    pub scale_pct: u32,
    /// Caractères absents des polices (`pdf.glyph_missing`), 32 au plus.
    pub missing_glyphs: String,
    /// Codes des avertissements (`pdf.glyph_missing`).
    pub diagnostics: Vec<String>,
}

/// Texte selon la langue.
const fn tr(lang: Language, fr: &'static str, en: &'static str) -> &'static str {
    match lang {
        Language::Fr => fr,
        Language::En => en,
    }
}

fn invalid(lang: Language, field: &str, fr: &'static str, en: &'static str) -> ToolError {
    ToolError::simple(codes::PARAM_INVALID, tr(lang, fr, en).to_owned()).with_field(field)
}

fn too_many(lang: Language, needed: usize) -> ToolError {
    ToolError::simple(
        codes::PDF_TOO_MANY_PAGES,
        match lang {
            Language::Fr => format!(
                "{needed} pages nécessaires, 1 000 au plus : réduire la plage, filtrer les lignes ou choisir moins de pages en largeur"
            ),
            Language::En => format!(
                "{needed} pages needed, 1,000 at most: narrow the range, filter rows or choose fewer pages wide"
            ),
        },
    )
}

/// PDF du planning de `s` selon `p` (chemin non lu).
///
/// # Errors
///
/// `param.invalid` (plage inversée, pages en largeur hors de 1 à 20,
/// niveau 0) ; `pdf.too_many_pages` au-delà de 1 000 pages ; `file.write`
/// si l'écriture du document échoue.
pub fn export(s: &Session, p: &ExportPdfParams, lang: Language) -> Result<PdfExport, ToolError> {
    export_rows(s, p, None, lang)
}

/// Comme [`export`], tâches `uids` seules (`None` : toutes) en plus du
/// filtre de `p.rows` : l'interface imprime les lignes de sa vue affichée
/// (filtre, niveau, repli ; relecture finale de J8, F1).
///
/// # Errors
///
/// Comme [`export`].
pub fn export_rows(
    s: &Session,
    p: &ExportPdfParams,
    uids: Option<&std::collections::BTreeSet<plannercraft_core::TaskUid>>,
    lang: Language,
) -> Result<PdfExport, ToolError> {
    let project = s.project();
    let sched = s.schedule();
    let mut st = p.style.map_or_else(
        || style::effective_style(project, style::HEADLESS_DEFAULT),
        |t| template(t.into()),
    );
    page_overrides(&mut st.page, p, lang)?;
    let mut vp = viewport(&st.page, p, lang)?;
    vp.filter.uids = uids.cloned();
    let texts = SessionTexts { session: s, lang };
    let options = PaginateOptions {
        legend_every_page: st.legend.every_page,
    };
    // Chronologie en tête de la première page si la mise en page la
    // demande (H-UI-82) ; dates et textes du moteur (règle 3).
    let timeline = crate::timeline::print_input(s, &st, lang);
    let (pages, render) = layout_pages_timeline(project, sched, &st, &texts, &vp, options, timeline.as_ref());
    let pages = pages.map_err(|e| match e {
        PageError::TooManyPages { needed } => too_many(lang, needed),
        PageError::NotYetAvailable => ToolError::simple(codes::FILE_WRITE, e.to_string()),
    })?;
    let title = if project.info.name.is_empty() {
        s.origin().name.clone()
    } else {
        project.info.name.clone()
    };
    let meta = PdfMeta {
        title,
        created: s.now(),
        lang: vp.lang,
    };
    let (bytes, report) = plannercraft_pdf::pdf(&pages, &FontSet::inter(), &meta).map_err(|e| match e {
        PdfError::TooManyPages(n) => too_many(lang, n),
        other => ToolError::simple(
            codes::FILE_WRITE,
            format!(
                "{} : {other}",
                tr(lang, "écriture du PDF impossible", "cannot write the PDF")
            ),
        ),
    })?;
    let diagnostics = if report.missing_total > 0 {
        vec![codes::PDF_GLYPH_MISSING.to_owned()]
    } else {
        Vec::new()
    };
    Ok(PdfExport {
        bytes,
        report: PdfExportReport {
            pages: report.pages,
            shown: render.shown,
            hidden: render.hidden,
            truncated_days: render.truncated_days,
            scale_pct: pages.scale / 100,
            missing_glyphs: report.missing_glyphs.into_iter().collect(),
            diagnostics,
        },
    })
}

/// PDF de pages déjà paginées (aperçu du mode impression de l'interface,
/// J8 lot L15, H-UI-68) : mêmes métadonnées que [`export`] (titre du
/// projet, « maintenant » de la session, H-R13), polices Inter.
///
/// # Errors
///
/// Texte du refus de `plannercraft-pdf` (plus de 1 000 pages, écriture).
pub fn pages_pdf(
    s: &Session,
    pages: &plannercraft_render::paginate::Pages,
    lang: Language,
) -> Result<Vec<u8>, String> {
    let project = s.project();
    let title = if project.info.name.is_empty() {
        s.origin().name.clone()
    } else {
        project.info.name.clone()
    };
    let meta = PdfMeta {
        title,
        created: s.now(),
        lang: match lang {
            Language::Fr => Lang::Fr,
            Language::En => Lang::En,
        },
    };
    plannercraft_pdf::pdf(pages, &FontSet::inter(), &meta)
        .map(|(bytes, _)| bytes)
        .map_err(|e| e.to_string())
}

/// Papier, orientation et « N pages en largeur » de la requête.
fn page_overrides(st: &mut PageStyle, p: &ExportPdfParams, lang: Language) -> Result<(), ToolError> {
    if let Some(page) = &p.page {
        if let Some(paper) = page.paper {
            st.paper = match paper {
                PaperJson::A4 => Paper::A4,
                PaperJson::A3 => Paper::A3,
                PaperJson::Letter => Paper::Letter,
                PaperJson::Tabloid => Paper::Tabloid,
            };
        }
        if let Some(landscape) = page.landscape {
            st.landscape = landscape;
        }
        if let Some(n) = page.pages_wide {
            if !(1..=20).contains(&n) {
                return Err(invalid(
                    lang,
                    "page.pages_wide",
                    "« pages_wide » doit valoir de 1 à 20",
                    "\"pages_wide\" must be from 1 to 20",
                ));
            }
            st.fit = Fit::PagesWide(n);
        }
    }
    Ok(())
}

/// Fenêtre du rendu : plage, échelle ajustée à la page, lignes, variante
/// d'impression.
fn viewport(page: &PageStyle, p: &ExportPdfParams, lang: Language) -> Result<Viewport, ToolError> {
    let range = match p.range {
        None => RangeSpec::Auto,
        Some(r) if r.from > r.to => {
            return Err(invalid(
                lang,
                "range.from",
                "« from » est après « to »",
                "\"from\" is after \"to\"",
            ));
        }
        Some(r) => RangeSpec::Span {
            from: r.from,
            to: r.to,
        },
    };
    let rows = p.rows.unwrap_or_default();
    if rows.outline_max == Some(0) {
        return Err(invalid(
            lang,
            "rows.outline_max",
            "« outline_max » doit valoir au moins 1",
            "\"outline_max\" must be at least 1",
        ));
    }
    let render_lang = match lang {
        Language::Fr => Lang::Fr,
        Language::En => Lang::En,
    };
    let vp = Viewport {
        range,
        zoom: fit_zoom(page).unwrap_or(Zoom::Week),
        variant: Variant::Print,
        lang: render_lang,
        filter: RowFilter {
            critical_only: rows.critical_only,
            uids: None,
            max_outline: rows.outline_max,
        },
        limits: RenderLimits {
            max_rows: MAX_PDF_ROWS,
            ..RenderLimits::default()
        },
    };
    Ok(vp)
}
