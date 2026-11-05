//! Export PDF (lot L7 de J8) : H-UI-48, H-UI-49, H-UI-51, H-R13, P57,
//! `pdf.too_many_pages`, `pdf.glyph_missing`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

mod common;

use std::collections::BTreeSet;

use common::{add, at, link, page_texts, pages_of, project, project_p, scene_texts};
use plannercraft_core::{Project, TaskUid};
use plannercraft_pdf::{FontSet, PdfError, PdfMeta, pdf};
use plannercraft_render::Lang;
use plannercraft_render::geometry::{Part, Prim};
use plannercraft_render::paginate::{MAX_PAGES, Pages};
use plannercraft_render::style::TemplateId;
use proptest::prelude::*;

fn meta() -> PdfMeta {
    PdfMeta {
        title: "P".to_owned(),
        created: at("2026-10-26T09:30"),
        lang: Lang::Fr,
    }
}

fn write(pages: &Pages) -> (Vec<u8>, plannercraft_pdf::PdfReport) {
    pdf(pages, &FontSet::inter(), &meta()).unwrap()
}

/// Projet : `lead` feuilles, une récapitulative « Lot » et `kids` enfants.
fn long(lead: usize, kids: usize) -> Project {
    let mut p = project("Long", "2026-10-19T08:00");
    let mut prev: Option<TaskUid> = None;
    for i in 0..lead {
        let u = add(&mut p, &format!("Tâche {i}"), 2, 1);
        if let Some(pr) = prev {
            link(&mut p, pr, u);
        }
        prev = Some(u);
    }
    add(&mut p, "Lot", 0, 1);
    for i in 0..kids {
        add(&mut p, &format!("Enfant {i}"), 1, 2);
    }
    p
}

/// Extraction : chaque texte de la scène se retrouve dans le PDF (table
/// `ToUnicode`, un bloc de texte par texte placé).
#[test]
fn pdf_text_extractable() {
    for id in TemplateId::ALL {
        let pages = pages_of(&project_p(), id);
        let (bytes, report) = write(&pages);
        assert!(bytes.starts_with(b"%PDF-"));
        assert_eq!(report.pages, pages.pages.len());
        assert!(report.missing_glyphs.is_empty(), "{:?}", report.missing_glyphs);
        let found: BTreeSet<String> = page_texts(&bytes).into_iter().flatten().collect();
        for t in scene_texts(&pages.scene) {
            assert!(found.contains(&t), "{id:?} : « {t} » absent du PDF ({found:?})");
        }
        for name in ["Gros œuvre", "Fondations", "Plâtrerie", "Nom de la tâche"] {
            assert!(found.contains(name), "{id:?} : {name}");
        }
    }
}

/// H-UI-49 : l'échelle (deux niveaux) et les titres de colonnes sont sur
/// chaque page.
#[test]
fn h_ui_49_header_repeated() {
    let pages = pages_of(&long(120, 0), TemplateId::Sobre);
    assert!(pages.pages.len() >= 3);
    let header: Vec<String> = pages
        .scene
        .items
        .iter()
        .filter(|it| it.part == Part::Fixed)
        .filter_map(|it| match &it.prim {
            Prim::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert!(header.len() > 3, "titres et étiquettes d'échelle");
    let (bytes, _) = write(&pages);
    let texts = page_texts(&bytes);
    assert_eq!(texts.len(), pages.pages.len());
    for (n, page) in texts.iter().enumerate() {
        for h in &header {
            assert!(page.contains(h), "page {} sans « {h} »", n + 1);
        }
    }
}

/// H-UI-48 : la page qui commence dans les enfants de « Lot » le rappelle
/// « Lot (suite) » ; H-UI-51 : « Page 2/N » au pied.
#[test]
fn h_ui_48_continued_parent_in_pdf() {
    let pages = pages_of(&long(5, 90), TemplateId::Sobre);
    let n = pages.pages.len();
    assert!(n >= 2);
    let (bytes, _) = write(&pages);
    let texts = page_texts(&bytes);
    assert!(!texts[0].iter().any(|t| t == " (suite)"));
    for page in &texts[1..] {
        assert!(page.iter().any(|t| t == " (suite)"), "{page:?}");
        assert!(page.iter().any(|t| t == "Lot"));
    }
    assert!(
        texts[1].iter().any(|t| *t == format!("Page 2/{n}")),
        "{:?}",
        texts[1]
    );
}

/// `pdf.too_many_pages` : plus de 1 000 pages refusées sans rien écrire.
#[test]
fn pdf_too_many_pages() {
    let mut pages = pages_of(&project_p(), TemplateId::Sobre);
    let one = pages.pages[0].clone();
    pages.pages = vec![one; MAX_PAGES + 1];
    assert_eq!(
        pdf(&pages, &FontSet::inter(), &meta()).unwrap_err(),
        PdfError::TooManyPages(MAX_PAGES + 1)
    );
    pages.pages.clear();
    assert_eq!(
        pdf(&pages, &FontSet::inter(), &meta()).unwrap_err(),
        PdfError::Geometry
    );
}

/// `pdf.glyph_missing` : noms en caractères absents d'Inter (synthétiques,
/// comme ceux d'A6) : PDF écrit quand même, caractères listés une fois
/// chacun, dans l'ordre des points de code, 32 au plus.
#[test]
fn pdf_glyph_missing_cjk() {
    let mut p = project("工事", "2026-10-19T08:00");
    add(&mut p, "基礎工事", 3, 1);
    add(&mut p, "東京タワー", 2, 1);
    add(&mut p, "Murs", 2, 1);
    let (_, few) = write(&pages_of(&p, TemplateId::Sobre));
    let expected: BTreeSet<char> = "工事基礎東京タワー".chars().collect();
    assert_eq!(few.missing_glyphs, expected.into_iter().collect::<Vec<_>>());
    let many: String = ('\u{4E00}'..='\u{4E40}').collect();
    add(&mut p, &many, 1, 1);
    let pages = pages_of(&p, TemplateId::Sobre);
    let (bytes, report) = write(&pages);
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(report.missing_glyphs.contains(&'\u{4E00}'));
    assert!(report.missing_glyphs.len() <= plannercraft_pdf::MAX_MISSING_LISTED);
    assert!(
        report.missing_total > plannercraft_pdf::MAX_MISSING_LISTED,
        "{report:?}"
    );
    let sorted = {
        let mut v = report.missing_glyphs.clone();
        v.sort_unstable();
        v
    };
    assert_eq!(sorted, report.missing_glyphs);
    assert!(!report.missing_glyphs.contains(&'M'));
    let found: BTreeSet<String> = page_texts(&bytes).into_iter().flatten().collect();
    assert!(found.contains("Murs"));
}

/// H-R13 : métadonnées injectées (titre, langue, date), aucune horloge.
#[test]
fn h_r13_metadata_injected() {
    let (bytes, _) = write(&pages_of(&project_p(), TemplateId::Sobre));
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("/Title(P)"));
    assert!(text.contains("/Lang(fr)"));
    assert!(text.contains("D:20261026093000"), "date de création injectée");
}

fn generated(spec: &[(u8, u8)]) -> Project {
    let mut p = project("G", "2026-10-19T08:00");
    let mut level = 1_u16;
    let mut prev: Option<TaskUid> = None;
    for (i, &(days, step)) in spec.iter().enumerate() {
        level = match step % 3 {
            0 => level,
            1 => (level + 1).min(3),
            _ => level.saturating_sub(1).max(1),
        };
        if i == 0 {
            level = 1;
        }
        let u = add(&mut p, &format!("T{i} é"), i64::from(days % 7), level);
        if let Some(pr) = prev
            && step % 2 == 0
        {
            link(&mut p, pr, u);
        }
        prev = Some(u);
    }
    p
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// P57 : deux appels sur les mêmes pages ⇒ mêmes octets ; aucun texte
    /// de la scène absent du PDF.
    #[test]
    fn p57_same_bytes_no_text_missing(
        spec in proptest::collection::vec((0_u8..7, 0_u8..6), 1..60),
        id in 0_usize..6,
    ) {
        let pages = pages_of(&generated(&spec), TemplateId::ALL[id]);
        let (a, _) = write(&pages);
        let (b, _) = write(&pages);
        prop_assert_eq!(&a, &b);
        let found: BTreeSet<String> = page_texts(&a).into_iter().flatten().collect();
        for t in scene_texts(&pages.scene) {
            prop_assert!(found.contains(&t), "« {} » absent", t);
        }
    }
}

/// H-UI-66 : cartouche du modèle « Planning travaux » (titre par le jeton
/// `{planning}`, projet à droite, date d'état et page au pied), en-tête
/// agrandi de la ligne de titre ; les autres modèles n'en ont pas. Relecture
/// finale de J8 : aucun texte en dur ni hypothèse contractuelle (« Aléas
/// 10 % ») dans le modèle, le titre suit la langue du rendu.
#[test]
fn h_ui_66_title_block_pdf() {
    use plannercraft_render::geometry::{BarTextSource, Token};
    use plannercraft_render::paginate::{SLOT_LINE_PT, TITLE_LINE_PT, paginate};
    use plannercraft_render::style::{BarField, template};
    /// Jetons d'une session française.
    struct Fr;
    impl BarTextSource for Fr {
        fn bar_text(&self, _uid: TaskUid, _f: BarField) -> Option<String> {
            None
        }
        fn token(&self, t: Token) -> String {
            match t {
                Token::ScheduleTitle => "Planning prévisionnel".to_owned(),
                Token::Project => "Résidence Beaujeu".to_owned(),
                _ => String::new(),
            }
        }
    }
    let p = project_p();
    let pages = paginate(
        &common::scene_of(&p, TemplateId::Chantier),
        &template(TemplateId::Chantier).page,
        &Fr,
    )
    .unwrap();
    assert!(pages.title_block);
    assert_eq!(pages.header_pt, SLOT_LINE_PT + TITLE_LINE_PT - SLOT_LINE_PT);
    let (bytes, report) = write(&pages);
    assert!(report.missing_glyphs.is_empty(), "{:?}", report.missing_glyphs);
    let texts = page_texts(&bytes);
    let first = &texts[0];
    for t in ["Planning prévisionnel", "Résidence Beaujeu"] {
        assert!(first.iter().any(|x| x == t), "« {t} » absent : {first:?}");
    }
    assert!(first.iter().any(|x| x.starts_with("Page 1/")), "{first:?}");
    assert!(!first.iter().any(|x| x.contains("Aléas")), "{first:?}");
    let sobre = pages_of(&project_p(), TemplateId::Sobre);
    assert!(!sobre.title_block);
    assert_eq!(sobre.header_pt, SLOT_LINE_PT);
}

/// H-UI-126 (finitions P2) : une tâche en gras, italique, couleur et
/// surlignage s'exporte (oblique simulé par une transformation) ; ses
/// textes restent extractibles et le PDF diffère de celui sans mise en
/// forme.
#[test]
fn h_ui_126_pdf_task_text_style() {
    use plannercraft_render::geometry::{NoTexts, Viewport, layout};
    use plannercraft_render::paginate::paginate;
    use plannercraft_render::style::{Rgb, TaskText, Variant, template};
    let p = project_p();
    let s = plannercraft_core::schedule(&p);
    let mut st = template(TemplateId::Classique);
    let uid = p.tasks.iter().nth(2).unwrap().uid().get();
    st.task_text.insert(
        uid,
        TaskText {
            bold: true,
            italic: true,
            color: Some(Rgb(0xC0, 0, 0)),
            fill: Some(Rgb(0xFF, 0xF2, 0xCC)),
        },
    );
    let vp = Viewport::new(Variant::Print, Lang::Fr);
    let styled = paginate(&layout(&p, &s, &st, &NoTexts, &vp).0, &st.page, &NoTexts).unwrap();
    let (bytes, report) = write(&styled);
    assert!(report.missing_glyphs.is_empty(), "{:?}", report.missing_glyphs);
    let found: BTreeSet<String> = page_texts(&bytes).into_iter().flatten().collect();
    assert!(found.contains("Murs"), "{found:?}");
    let plain_st = template(TemplateId::Classique);
    let plain = paginate(
        &layout(&p, &s, &plain_st, &NoTexts, &vp).0,
        &plain_st.page,
        &NoTexts,
    )
    .unwrap();
    assert_ne!(write(&plain).0, bytes);
}
