//! Essai bloquant du lot L0 (ADR-0016 D8) : une page, un rectangle et trois
//! glyphes d'Inter, placés un par un. Garde la preuve que krilla compile et
//! écrit sur toutes les cibles, wasm32 compris.

use krilla::Document;
use krilla::color::rgb;
use krilla::geom::{PathBuilder, Point, Rect};
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::Fill;
use krilla::text::{Font, GlyphId, KrillaGlyph};
use skrifa::MetadataProvider;
use skrifa::instance::{LocationRef, Size};
use skrifa::raw::TableProvider;

use crate::PdfError;

/// Texte de l'essai.
const SMOKE_TEXT: &str = "Abc";

/// Écrit un PDF d'une page A4 portrait (595 × 842 pt) : un rectangle et le
/// texte « Abc » dans la police `font` (TTF), glyphes placés aux avances de
/// la police, sans façonnage.
///
/// # Errors
///
/// [`PdfError::FontInvalid`] si la police est illisible ou n'a pas les
/// caractères ; [`PdfError::Geometry`] ou [`PdfError::Write`] si krilla
/// refuse la page.
pub fn smoke_page(font: &'static [u8]) -> Result<Vec<u8>, PdfError> {
    let glyphs = glyphs_for(font, SMOKE_TEXT)?;
    let pdf_font = Font::new(font.into(), 0).ok_or(PdfError::FontInvalid)?;
    let mut document = Document::new();
    let settings = PageSettings::from_wh(595.0, 842.0).ok_or(PdfError::Geometry)?;
    let mut page = document.start_page_with(settings);
    let mut surface = page.surface();
    let rect = {
        let mut builder = PathBuilder::new();
        builder.push_rect(Rect::from_xywh(72.0, 72.0, 200.0, 40.0).ok_or(PdfError::Geometry)?);
        builder.finish().ok_or(PdfError::Geometry)?
    };
    surface.set_fill(Some(Fill {
        paint: rgb::Color::new(0x1F, 0x29, 0x33).into(),
        opacity: NormalizedF32::ONE,
        rule: krilla::paint::FillRule::default(),
    }));
    surface.draw_path(&rect);
    surface.draw_glyphs(
        Point::from_xy(72.0, 160.0),
        &glyphs,
        pdf_font,
        SMOKE_TEXT,
        12.0,
        false,
    );
    surface.finish();
    page.finish();
    document.finish().map_err(|e| PdfError::Write(format!("{e:?}")))
}

/// Glyphes de `text` dans `font`, un par caractère, avance normalisée par
/// le cadratin (convention de krilla).
fn glyphs_for(font: &[u8], text: &str) -> Result<Vec<KrillaGlyph>, PdfError> {
    let face = skrifa::FontRef::new(font).map_err(|_| PdfError::FontInvalid)?;
    let charmap = face.charmap();
    let metrics = face.glyph_metrics(Size::unscaled(), LocationRef::default());
    let upem = f32::from(face.head().map_err(|_| PdfError::FontInvalid)?.units_per_em());
    let mut out = Vec::new();
    for (start, ch) in text.char_indices() {
        let gid = charmap.map(ch).ok_or(PdfError::FontInvalid)?;
        let advance = metrics.advance_width(gid).unwrap_or(0.0);
        out.push(KrillaGlyph {
            glyph_id: GlyphId::new(gid.to_u32()),
            text_range: start..start.saturating_add(ch.len_utf8()),
            x_advance: advance / upem,
            x_offset: 0.0,
            y_offset: 0.0,
            y_advance: 0.0,
            location: None,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests")]

    use super::*;

    use crate::INTER_REGULAR as INTER;

    #[test]
    fn pdf_smoke_one_page() {
        let bytes = smoke_page(INTER).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        let text = String::from_utf8_lossy(&bytes);
        // krilla écrit les dictionnaires sans espaces : `/Type/Page/…`.
        assert_eq!(text.matches("/Type/Page/").count(), 1, "une seule page");
        assert!(text.contains("Inter-Regular"), "police incorporée");
        // Déterminisme (H-R13) : deux appels, mêmes octets.
        assert_eq!(bytes, smoke_page(INTER).unwrap());
    }
}
