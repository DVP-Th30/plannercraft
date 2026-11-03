//! P56 (partie style, lot L4 de J8) : toute chaîne (≤ 64 Kio) donne un style
//! borné ou une erreur, jamais de panique ; sérialisation puis lecture =
//! identité.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

use plannercraft_render::style::{
    BarPaint, BarRole, ColorTokensPatch, Density, DocStyle, FontChoice, FontStyle, MAX_DOC_STYLE_BYTES, Rgb,
    StyleError, StylePatch, TemplateId, VariantsPatch, parse_doc_style, resolve, template,
};
use proptest::prelude::*;

/// Document valide de référence (modèle Sobre, quelques surcharges).
const VALID: &str = r##"{"base":"sobre","over":{"colors":{"light":{"bars":{"critical":{"fill":"#AA0000","outline":"#550000"}},"text":"#101010"}},"density":"compact","font":{"family":"inter","size16":200},"name":"Mon style"},"v":1}"##;

/// Chaîne JSON plausible : fragments de style mêlés au hasard.
fn jsonish() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        Just("{".to_owned()),
        Just("}".to_owned()),
        Just("\"v\":1,".to_owned()),
        Just("\"base\":\"sobre\"".to_owned()),
        Just("\"over\":{".to_owned()),
        Just("\"font\":{\"family\":\"inter\",\"size16\":".to_owned()),
        Just("\"density\":\"airy\"".to_owned()),
        Just("\"bars\":{\"task\":{\"outline16\":99999,\"shape\":\"bar\",\"height_pct\":0,\"radius16\":1,\"pattern\":\"solid\"}}".to_owned()),
        Just("\"#GG0000\"".to_owned()),
        Just("1e999".to_owned()),
        Just("-1".to_owned()),
        Just("18446744073709551616".to_owned()),
        Just("[[[[[[[[".to_owned()),
        Just(",".to_owned()),
        any::<String>(),
    ];
    proptest::collection::vec(piece, 0..40).prop_map(|v| v.concat())
}

/// Chaîne valide avec un octet remplacé, inséré ou retiré.
fn mutated() -> impl Strategy<Value = String> {
    (0..VALID.len(), any::<char>(), 0_u8..3).prop_map(|(at, c, op)| {
        let mut chars: Vec<char> = VALID.chars().collect();
        let at = at.min(chars.len().saturating_sub(1));
        match op {
            0 => chars[at] = c,
            1 => chars.insert(at, c),
            _ => {
                chars.remove(at);
            }
        }
        chars.into_iter().collect()
    })
}

/// Résultat accepté d'une lecture : style résolu borné, aller-retour exact.
fn check(input: &str) -> Result<(), TestCaseError> {
    match parse_doc_style(input) {
        Ok(doc) => {
            let style = doc.resolve();
            prop_assert!(style.validate().is_ok());
            let json = doc.to_json();
            prop_assert!(!json.contains("?>"));
            prop_assert_eq!(parse_doc_style(&json), Ok(doc));
        }
        Err(StyleError::TooLarge { bytes }) => prop_assert!(bytes > MAX_DOC_STYLE_BYTES),
        Err(_) => {}
    }
    Ok(())
}

fn rgb() -> impl Strategy<Value = Rgb> {
    (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(r, g, b)| Rgb(r, g, b))
}

/// Style de document généré dans les bornes.
fn doc() -> impl Strategy<Value = DocStyle> {
    (
        0_usize..6,
        proptest::option::of("[ -~éà>?<]{0,64}"),
        proptest::option::of(0_u8..3),
        proptest::option::of(160_u16..=320),
        proptest::option::of((rgb(), proptest::option::of(rgb()))),
        proptest::option::of(rgb()),
    )
        .prop_map(|(base, name, density, size, critical, text)| {
            let mut over = StylePatch {
                name,
                density: density.map(|d| [Density::Compact, Density::Normal, Density::Airy][usize::from(d)]),
                font: size.map(|size16| FontStyle {
                    family: FontChoice::Inter,
                    size16,
                }),
                ..StylePatch::default()
            };
            if critical.is_some() || text.is_some() {
                let mut light = ColorTokensPatch {
                    text,
                    ..ColorTokensPatch::default()
                };
                if let Some((fill, outline)) = critical {
                    light.bars.insert(BarRole::Critical, BarPaint { fill, outline });
                }
                over.colors = Some(VariantsPatch {
                    light: Some(light),
                    ..VariantsPatch::default()
                });
            }
            DocStyle {
                v: 1,
                base: TemplateId::ALL[base],
                over,
            }
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// P56 : chaîne quelconque ⇒ style borné ou erreur, jamais de panique.
    #[test]
    fn p56_any_string_bounded_or_error(input in prop_oneof![any::<String>(), jsonish(), mutated()]) {
        check(&input)?;
    }

    /// P56 : sérialisation puis lecture = identité ; le style résolu
    /// porte les surcharges.
    #[test]
    fn p56_roundtrip_identity(doc in doc()) {
        let json = doc.to_json();
        prop_assert!(json.len() <= MAX_DOC_STYLE_BYTES);
        let again = parse_doc_style(&json).unwrap();
        prop_assert_eq!(&again, &doc);
        prop_assert_eq!(again.to_json(), json);
        let style = resolve(doc.base, &doc.over);
        prop_assert!(style.validate().is_ok());
        if let Some(size) = doc.over.font.map(|f| f.size16) {
            prop_assert_eq!(style.font.size16, size);
        }
    }
}

/// P56 : au-delà de 64 Kio, refus sans lecture ; juste en dessous, lu.
#[test]
fn p56_size_bound() {
    let pad = |n: usize| {
        format!(
            r#"{{"v":1,"base":"sobre","over":{{"name":"{}"}}}}"#,
            "a".repeat(n)
        )
    };
    let base_len = pad(0).len();
    let over = pad(MAX_DOC_STYLE_BYTES + 1 - base_len);
    assert!(matches!(parse_doc_style(&over), Err(StyleError::TooLarge { .. })));
    // Nom trop long (> 64 caractères) mais taille admise : hors bornes.
    let long = pad(MAX_DOC_STYLE_BYTES - base_len);
    assert_eq!(long.len(), MAX_DOC_STYLE_BYTES);
    assert_eq!(parse_doc_style(&long), Err(StyleError::OutOfBounds("name")));
    assert_eq!(
        parse_doc_style(VALID).unwrap().resolve().density,
        Density::Compact
    );
    assert_ne!(
        template(TemplateId::Sobre),
        parse_doc_style(VALID).unwrap().resolve()
    );
}

/// H-UI-45 et H-UI-65, relecture finale de J8 : le fond d'une ligne de
/// phase garde lisibles les trois encres de la table (texte, appuyé,
/// atténué d'une ligne exclue ou inactive), au seuil du modèle (7:1 en
/// Contraste élevé), pour chaque modèle, variante et couleur du nuancier.
#[test]
fn h_ui_45_phase_row_tint_all_inks() {
    use plannercraft_render::style::{Rgb, Variant, contrast_ratio, min_text_contrast, phase_row_tint};
    let swatches = [
        Rgb(0xE6, 0x9F, 0x00),
        Rgb(0x56, 0xB4, 0xE9),
        Rgb(0x00, 0x9E, 0x73),
        Rgb(0xF0, 0xE4, 0x42),
        Rgb(0x00, 0x72, 0xB2),
        Rgb(0xD5, 0x5E, 0x00),
        Rgb(0xCC, 0x79, 0xA7),
        Rgb(0xE0, 0x30, 0x20),
        Rgb(0x00, 0x00, 0x00),
        Rgb(0xFF, 0xFF, 0xFF),
    ];
    for id in TemplateId::ALL {
        let st = template(id);
        let min = min_text_contrast(&st);
        for v in [Variant::Light, Variant::Dark, Variant::Print] {
            let c = st.tokens(v);
            for fill in swatches {
                let tint = phase_row_tint(&st, v, fill);
                for ink in [c.text, c.text_strong, c.text_muted] {
                    let ratio = contrast_ratio(ink, tint);
                    assert!(
                        ratio >= min,
                        "{id:?} {v:?} phase {} : encre {} sur {} = {ratio}",
                        fill.hex(),
                        ink.hex(),
                        tint.hex()
                    );
                }
            }
        }
    }
}
