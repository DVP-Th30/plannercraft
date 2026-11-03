//! Polices du PDF : Inter Regular et `SemiBold` du dépôt, police de repli de
//! l'utilisateur ; glyphe de chaque caractère par la `cmap` (skrifa, la
//! version de krilla), mis en cache. Aucun façonnage (H-R13) : chaque
//! caractère est un glyphe, avancé de sa largeur H-R11.

use std::collections::{BTreeMap, BTreeSet};

use krilla::text::{Font, GlyphId, KrillaGlyph};
use plannercraft_render::{FontRef, FontWeight, InterMetrics};
use skrifa::MetadataProvider;

use crate::{FontSet, PdfError};

/// Caractères absents listés au plus (`pdf.glyph_missing`).
pub const MAX_MISSING_LISTED: usize = 32;

/// Une police chargée et sa table de caractères (cache).
struct Face {
    font: Font,
    data: Vec<u8>,
    cache: BTreeMap<char, Option<u32>>,
}

impl Face {
    fn new(data: Vec<u8>) -> Result<Self, PdfError> {
        // Police illisible : refusée tout de suite plutôt qu'au premier
        // glyphe.
        skrifa::FontRef::new(&data).map_err(|_| PdfError::FontInvalid)?;
        let font = Font::new(data.clone().into(), 0).ok_or(PdfError::FontInvalid)?;
        Ok(Self {
            font,
            data,
            cache: BTreeMap::new(),
        })
    }

    /// Glyphe de `c` (`None` : absent de la police).
    fn glyph(&mut self, c: char) -> Option<u32> {
        if let Some(g) = self.cache.get(&c) {
            return *g;
        }
        let g = skrifa::FontRef::new(&self.data)
            .ok()
            .and_then(|f| f.charmap().map(c))
            .map(skrifa::GlyphId::to_u32)
            .filter(|g| *g != 0);
        self.cache.insert(c, g);
        g
    }
}

/// Polices d'un export.
pub struct Fonts {
    regular: Face,
    semibold: Face,
    fallback: Option<Face>,
    /// Caractères introuvables dans toutes les polices.
    pub missing: BTreeSet<char>,
}

/// Suite de glyphes d'une même police.
pub struct Run {
    /// Police.
    pub font: Font,
    /// Texte de la suite (pour la table `ToUnicode`).
    pub text: String,
    /// Glyphes, avances en fraction du corps.
    pub glyphs: Vec<KrillaGlyph>,
    /// Avance totale en 1/16 px.
    pub width16: i64,
}

impl Fonts {
    /// Charge les polices.
    ///
    /// # Errors
    ///
    /// [`PdfError::FontInvalid`] si une police est illisible.
    pub fn load(set: &FontSet) -> Result<Self, PdfError> {
        Ok(Self {
            regular: Face::new(set.regular.to_vec())?,
            semibold: Face::new(set.semibold.to_vec())?,
            fallback: set.fallback.clone().map(Face::new).transpose()?,
            missing: BTreeSet::new(),
        })
    }

    /// Suites de glyphes de `text` dans la graisse de `font` : un glyphe
    /// par caractère, avance H-R11 (celle de la mesure de la scène, un
    /// cadratin pour un caractère absent d'Inter) ; caractère absent
    /// d'Inter pris dans la police de repli, sinon glyphe 0 et noté
    /// manquant ; contrôles écrits en espace.
    pub fn runs(&mut self, text: &str, font: FontRef) -> Vec<Run> {
        let size = f32::from(font.size16.max(1));
        let mut runs: Vec<Run> = Vec::new();
        // 0 : Inter, 1 : repli.
        let mut current: Option<u8> = None;
        for ch in text.chars() {
            let shown = if ch.is_control() { ' ' } else { ch };
            let advance16 = InterMetrics::advance16(shown, font);
            let primary = match font.weight {
                FontWeight::Regular => &mut self.regular,
                FontWeight::SemiBold => &mut self.semibold,
            };
            let (which, gid) = if let Some(g) = primary.glyph(shown) {
                (0_u8, g)
            } else if let Some(g) = self.fallback.as_mut().and_then(|f| f.glyph(shown)) {
                (1, g)
            } else {
                if !shown.is_whitespace() {
                    self.missing.insert(shown);
                }
                (0, 0)
            };
            if current != Some(which) || runs.is_empty() {
                let face = if which == 1 {
                    self.fallback.as_ref().map_or(&self.regular.font, |f| &f.font)
                } else {
                    match font.weight {
                        FontWeight::Regular => &self.regular.font,
                        FontWeight::SemiBold => &self.semibold.font,
                    }
                };
                runs.push(Run {
                    font: face.clone(),
                    text: String::new(),
                    glyphs: Vec::new(),
                    width16: 0,
                });
                current = Some(which);
            }
            let Some(run) = runs.last_mut() else {
                continue;
            };
            let start = run.text.len();
            run.text.push(ch);
            run.glyphs.push(KrillaGlyph {
                glyph_id: GlyphId::new(gid),
                text_range: start..run.text.len(),
                x_advance: to_f32(advance16) / size,
                x_offset: 0.0,
                y_offset: 0.0,
                y_advance: 0.0,
                location: None,
            });
            run.width16 = run.width16.saturating_add(advance16);
        }
        runs
    }
}

/// Entier vers flottant du PDF (coordonnées bornées par la scène, bien
/// en deçà de 2^24 en pratique ; au-delà, l'arrondi ne change que
/// l'affichage).
#[allow(
    clippy::cast_precision_loss,
    reason = "coordonnées du PDF, flottantes par nature"
)]
pub const fn to_f32(v: i64) -> f32 {
    v as f32
}
