//! Pagination (lot L7 de J8) : H-UI-48, H-UI-50, H-UI-51, P54.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

mod common;

use common::{add, link, project, project_p};
use plannercraft_core::{LinkType, Project, TaskUid};
use plannercraft_render::geometry::{BarTextSource, NoTexts, Scene, Token, Viewport, layout};
use plannercraft_render::paginate::{
    FOOTER_LINES, FULL_SCALE, HEADER_LINES, MAX_PAGES, PageError, Pages, PaginateOptions, SLOT_GAP_PT,
    fit_zoom, layout_pages, paginate, paginate_with,
};
use plannercraft_render::style::{BarField, Fit, PageStyle, Paper, Slot, TemplateId, Variant, template};
use plannercraft_render::{Lang, RenderLimits, Zoom};
use proptest::prelude::*;

/// Source de jetons fixe.
struct Tokens;

impl BarTextSource for Tokens {
    fn bar_text(&self, _uid: TaskUid, _f: BarField) -> Option<String> {
        None
    }

    fn token(&self, t: Token) -> String {
        match t {
            Token::Project => "Chantier\nP".to_owned(),
            Token::StatusDate => "02/11/2026".to_owned(),
            Token::PrintDate => "26/10/2026".to_owned(),
            Token::File => "p.xml".to_owned(),
            Token::ScheduleTitle => "Planning prévisionnel".to_owned(),
            Token::Page | Token::Pages => String::new(),
        }
    }
}

fn page_style(id: TemplateId) -> PageStyle {
    template(id).page
}

fn scene_of(p: &Project, zoom: Zoom, id: TemplateId) -> Scene {
    let s = plannercraft_core::schedule(p);
    let vp = Viewport {
        zoom,
        limits: RenderLimits {
            max_rows: 10_000,
            ..RenderLimits::default()
        },
        ..Viewport::new(Variant::Print, Lang::Fr)
    };
    layout(p, &s, &template(id), &NoTexts, &vp).0
}

/// Capacité en lignes d'une page (corps moins l'échelle), en 1/16 px.
fn capacity16(pages: &Pages) -> i64 {
    let [t, _, b, _] = pages.margins_pt;
    let gaps =
        if pages.header_pt > 0 { SLOT_GAP_PT } else { 0 } + if pages.footer_pt > 0 { SLOT_GAP_PT } else { 0 };
    let body = i64::from(pages.paper_pt.1 - t - b - pages.header_pt - pages.footer_pt - gaps);
    body * 64 * i64::from(FULL_SCALE) / (3 * i64::from(pages.scale)) - pages.scene.header16
}

/// Bandes distinctes des pages (première colonne).
fn bands(pages: &Pages) -> Vec<&plannercraft_render::paginate::Page> {
    pages.pages.iter().filter(|p| p.col == 0).collect()
}

/// Projet : `lead` feuilles, puis une récapitulative et `kids` enfants.
fn lead_then_summary(lead: usize, kids: usize) -> Project {
    let mut p = project("S", "2026-10-19T08:00");
    for i in 0..lead {
        add(&mut p, &format!("A{i}"), 1, 1);
    }
    add(&mut p, "Lot", 0, 1);
    for i in 0..kids {
        add(&mut p, &format!("K{i}"), 1, 2);
    }
    p
}

/// H-UI-48 : aucune ligne coupée ; les bandes se suivent sans trou ni
/// recouvrement, et une bande de plus d'une ligne tient dans la page.
#[test]
fn h_ui_48_no_split_row() {
    let p = lead_then_summary(120, 40);
    let sc = scene_of(&p, Zoom::Week, TemplateId::Sobre);
    let pages = paginate(&sc, &page_style(TemplateId::Sobre), &Tokens).unwrap();
    let cap = capacity16(&pages);
    let mut next = 0;
    for b in bands(&pages) {
        assert_eq!(b.rows.start, next, "bandes contiguës");
        assert!(!b.rows.is_empty());
        let rows_h: i64 = sc.rows[b.rows.clone()].iter().map(|r| r.h16).sum();
        let parent_h = if b.continued_parent.is_some() {
            sc.rows[0].h16
        } else {
            0
        };
        let tail = if b.legend { pages.legend16.1 } else { 0 } + if b.cut { pages.cut16 } else { 0 };
        assert!(
            rows_h + parent_h + tail <= cap,
            "bande {:?} : {} > {cap}",
            b.rows,
            rows_h + parent_h + tail
        );
        next = b.rows.end;
    }
    assert_eq!(next, sc.rows.len());
    assert!(bands(&pages).len() >= 3, "plusieurs bandes");
}

/// H-UI-48 : une récapitulative n'est jamais la dernière ligne d'une page
/// quand ses enfants sont sur la suivante (elle est reportée), quelle que
/// soit sa position.
#[test]
fn h_ui_48_summary_not_orphaned() {
    let mut reported = false;
    for lead in 0..40 {
        let p = lead_then_summary(lead, 6);
        let sc = scene_of(&p, Zoom::Week, TemplateId::Classique);
        let pages = paginate(&sc, &page_style(TemplateId::Classique), &Tokens).unwrap();
        let cap = capacity16(&pages);
        for b in bands(&pages) {
            let last = b.rows.end - 1;
            if b.rows.end < sc.rows.len() {
                assert!(
                    !(sc.rows[last].summary && sc.rows[b.rows.end].level > sc.rows[last].level),
                    "récapitulative seule en bas de page : lead = {lead}, bande {:?}",
                    b.rows
                );
            }
            // Reportée : la bande aurait pu prendre une ligne de plus.
            let rows_h: i64 = sc.rows[b.rows.clone()].iter().map(|r| r.h16).sum();
            if b.rows.end < sc.rows.len() && rows_h + sc.rows[0].h16 <= cap {
                reported = true;
            }
        }
    }
    assert!(reported, "au moins un report constaté");
}

/// H-UI-48 : une page qui commence au milieu des enfants d'une
/// récapitulative la rappelle « (suite) ».
#[test]
fn h_ui_48_continued_parent() {
    let p = lead_then_summary(2, 80);
    let sc = scene_of(&p, Zoom::Week, TemplateId::Sobre);
    let pages = paginate(&sc, &page_style(TemplateId::Sobre), &Tokens).unwrap();
    let b = bands(&pages);
    assert!(b.len() >= 2);
    assert_eq!(b[0].continued_parent, None);
    for band in &b[1..] {
        assert_eq!(
            band.continued_parent,
            Some(2),
            "« Lot » rappelé en haut de {:?}",
            band.rows
        );
    }
}

/// H-UI-50 : « 2 pages en largeur » choisit l'échelle de temps qui remplit
/// la largeur de deux pages (`Zoom::Fill`, J8 lot L16 : le Gantt imprimé
/// remplit la page), puis réduit au besoin ; jamais plus de deux colonnes
/// de pages.
#[test]
fn h_ui_50_pages_wide_2() {
    let mut style = page_style(TemplateId::Sobre);
    style.fit = Fit::PagesWide(2);
    let Some(Zoom::Fill { width_px }) = fit_zoom(&style) else {
        panic!("zoom ajusté attendu")
    };
    // A4 paysage : 842 − 2 × 28 = 786 pt = 1 048 px ; table 280 px
    // répétée : 2 × 768 px.
    assert_eq!(width_px, 1_536);
    // Projet P : remplit les deux pages en largeur, à 100 %.
    let p = project_p();
    let pages = paginate(
        &scene_of(&p, Zoom::Fill { width_px }, TemplateId::Sobre),
        &style,
        &Tokens,
    )
    .unwrap();
    assert_eq!(pages.scale, FULL_SCALE);
    assert!(pages.pages.iter().all(|pg| pg.col < 2));
    assert!(pages.pages.iter().any(|pg| pg.col == 1), "deux pages remplies");
    // Deux ans de travaux : deux colonnes au plus, échelle réduite.
    let mut long = project("L", "2026-10-19T08:00");
    let a = add(&mut long, "Gros œuvre", 300, 1);
    let b = add(&mut long, "Second œuvre", 220, 1);
    link(&mut long, a, b, LinkType::FinishToStart);
    for zoom in [
        Zoom::Fill { width_px },
        Zoom::Auto { width_px },
        Zoom::Week,
        Zoom::Month,
    ] {
        let pages = paginate(&scene_of(&long, zoom, TemplateId::Sobre), &style, &Tokens).unwrap();
        let cols = pages.pages.iter().map(|pg| pg.col).max().unwrap() + 1;
        assert!(cols <= 2, "{zoom:?} : {cols} colonnes");
        assert!(pages.scale <= FULL_SCALE);
        assert!(pages.pages.iter().all(|pg| pg.table), "table répétée");
    }
    // Sans ajustement, la plage au jour ne tient pas : colonnes multiples.
    style.fit = Fit::Scale(100);
    let pages = paginate(&scene_of(&long, Zoom::Day, TemplateId::Sobre), &style, &Tokens).unwrap();
    assert!(pages.pages.iter().any(|pg| pg.col >= 2));
}

/// H-UI-50, relecture finale de J8 (F2) : « 1 × 1 page » d'un planning de
/// 85 lignes sur deux ans, A3 paysage : la hauteur impose une réduction,
/// et le Gantt est élargi d'autant (`layout_pages`, seconde passe) ; la
/// scène réduite occupe toute la largeur imprimable au lieu d'environ la
/// moitié.
#[test]
fn h_ui_50_pages_wide_tall_fills_width() {
    let mut p = project("B", "2026-10-19T08:00");
    let mut prev = None;
    for i in 0..85 {
        let t = add(&mut p, &format!("Tâche {i}"), 6, 1);
        if let Some(prev) = prev {
            link(&mut p, prev, t, LinkType::FinishToStart);
        }
        prev = Some(t);
    }
    let s = plannercraft_core::schedule(&p);
    let st = {
        let mut st = template(TemplateId::Sobre);
        st.page.paper = Paper::A3;
        st.page.landscape = true;
        st.page.fit = Fit::PagesWideTall(1, 1);
        st
    };
    let vp = Viewport {
        zoom: fit_zoom(&st.page).unwrap(),
        limits: RenderLimits {
            max_rows: 10_000,
            ..RenderLimits::default()
        },
        ..Viewport::new(Variant::Print, Lang::Fr)
    };
    let (pages, _) = layout_pages(&p, &s, &st, &Tokens, &vp, PaginateOptions::default());
    let pages = pages.unwrap();
    assert_eq!(pages.pages.len(), 1, "une seule feuille");
    assert!(
        pages.scale < FULL_SCALE * 3 / 4,
        "réduite par la hauteur : {}",
        pages.scale
    );
    let [_, right, _, left] = pages.margins_pt;
    let printable16 = i64::from(pages.paper_pt.0 - left - right) * 64 / 3;
    let used16 = pages.scene.width16 * i64::from(pages.scale) / i64::from(FULL_SCALE);
    assert!(
        used16 * 100 >= printable16 * 97 && used16 <= printable16,
        "largeur occupée {used16} / {printable16}"
    );
}

/// H-UI-51 : jetons remplacés (`{page}`, `{pages}`, ceux de la source),
/// jeton inconnu gardé, fins de ligne d'un jeton aplaties, en-tête coupé à
/// 5 lignes et pied à 3.
#[test]
fn h_ui_51_tokens_and_line_limits() {
    let p = lead_then_summary(150, 0);
    let sc = scene_of(&p, Zoom::Week, TemplateId::Sobre);
    let mut style = page_style(TemplateId::Sobre);
    style.header = [
        Slot("{projet}\n2\n3\n4\n5\n6\n7".to_owned()),
        Slot("{inconnu}".to_owned()),
        Slot("État {date_etat}".to_owned()),
    ];
    style.footer = [
        Slot("{fichier}\nb\nc\nd".to_owned()),
        Slot("Imprimé le {date_impression}".to_owned()),
        Slot("Page {page}/{pages}".to_owned()),
    ];
    let pages = paginate(&sc, &style, &Tokens).unwrap();
    let n = pages.pages.len();
    assert!(n >= 3);
    let second = &pages.pages[1];
    assert_eq!(second.header[0], "Chantier P\n2\n3\n4\n5");
    assert_eq!(second.header[0].lines().count(), HEADER_LINES);
    assert_eq!(second.header[1], "{inconnu}");
    assert_eq!(second.header[2], "État 02/11/2026");
    assert_eq!(second.footer[0].lines().count(), FOOTER_LINES);
    assert_eq!(second.footer[1], "Imprimé le 26/10/2026");
    assert_eq!(second.footer[2], format!("Page 2/{n}"));
    assert_eq!(pages.header_pt, 5 * 12);
    assert_eq!(pages.footer_pt, 3 * 12);
    // En-tête et pied vides : aucune place réservée.
    style.header = Default::default();
    style.footer = Default::default();
    let bare = paginate(&sc, &style, &Tokens).unwrap();
    assert_eq!((bare.header_pt, bare.footer_pt), (0, 0));
    assert!(bare.pages.len() <= n);
}

/// Légende sur la dernière page (H-UI-47), ou sur toutes.
#[test]
fn h_ui_47_legend_last_or_every_page() {
    let p = lead_then_summary(150, 3);
    let sc = scene_of(&p, Zoom::Week, TemplateId::Sobre);
    let style = page_style(TemplateId::Sobre);
    let pages = paginate(&sc, &style, &Tokens).unwrap();
    assert!(pages.legend16.1 > 0);
    let with: Vec<bool> = pages.pages.iter().map(|pg| pg.legend).collect();
    assert_eq!(with.iter().filter(|b| **b).count(), 1);
    assert!(with.last().copied().unwrap());
    let every = paginate_with(
        &sc,
        &style,
        &Tokens,
        PaginateOptions {
            legend_every_page: true,
        },
    )
    .unwrap();
    assert!(every.pages.iter().all(|pg| pg.legend));
}

/// `pdf.too_many_pages` côté pagination : plafond vérifié avant de
/// construire les pages.
#[test]
fn pdf_too_many_pages_paginate() {
    let mut p = project("G", "2026-10-19T08:00");
    let mut prev = None;
    for i in 0..1_500 {
        let u = add(&mut p, &format!("T{i}"), 5, 1);
        if let Some(pr) = prev {
            link(&mut p, pr, u, LinkType::FinishToStart);
        }
        prev = Some(u);
    }
    let sc = scene_of(&p, Zoom::Day, TemplateId::Sobre);
    let mut style = page_style(TemplateId::Sobre);
    style.fit = Fit::Scale(400);
    match paginate(&sc, &style, &Tokens) {
        Err(PageError::TooManyPages { needed }) => assert!(needed > MAX_PAGES),
        other => panic!("refus attendu : {:?}", other.map(|p| p.pages.len())),
    }
}

/// Projet généré : niveaux cohérents (au plus un de plus que la ligne
/// précédente), durées et liens variés.
fn generated(spec: &[(u8, u8, u8)]) -> Project {
    let mut p = project("G", "2026-10-19T08:00");
    let mut prev: Option<TaskUid> = None;
    let mut level = 1_u16;
    for (i, &(days, step, kind)) in spec.iter().enumerate() {
        level = match step % 3 {
            0 => level,
            1 => (level + 1).min(4),
            _ => level.saturating_sub(1).max(1),
        };
        if i == 0 {
            level = 1;
        }
        let u = add(&mut p, &format!("T{i}"), i64::from(days % 9), level);
        if let Some(pr) = prev
            && kind % 3 == 1
        {
            link(&mut p, pr, u, LinkType::FinishToStart);
        }
        prev = Some(u);
    }
    p
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1_024))]

    /// P54 : toute ligne de la scène est sur exactement une bande de pages,
    /// dans l'ordre ; chaque bande a toutes ses colonnes ; aucune page vide ;
    /// nombre de pages ≤ `MAX_PAGES` ou refus.
    #[test]
    fn p54_every_row_on_exactly_one_band(
        spec in proptest::collection::vec((0_u8..9, 0_u8..3, 0_u8..3), 0..90),
        paper in 0_usize..4,
        landscape in any::<bool>(),
        fit in 0_u8..3,
        n in 1_u16..4,
        pct in 10_u16..=400,
        margin in 0_u16..=50,
        repeat in any::<bool>(),
        every in any::<bool>(),
        id in 0_usize..6,
        zoom in 0_usize..4,
    ) {
        let p = generated(&spec);
        let mut style = page_style(TemplateId::ALL[id]);
        style.paper = [Paper::A4, Paper::A3, Paper::Letter, Paper::Tabloid][paper];
        style.landscape = landscape;
        style.margins_mm = [margin; 4];
        style.repeat_table = repeat;
        style.fit = match fit {
            0 => Fit::Scale(pct),
            1 => Fit::PagesWide(n),
            _ => Fit::PagesWideTall(n, n),
        };
        let z = fit_zoom(&style).unwrap_or([Zoom::Day, Zoom::Week, Zoom::Month, Zoom::Quarter][zoom]);
        let sc = scene_of(&p, z, TemplateId::ALL[id]);
        match paginate_with(&sc, &style, &Tokens, PaginateOptions { legend_every_page: every }) {
            Err(PageError::TooManyPages { needed }) => prop_assert!(needed > MAX_PAGES),
            Err(e) => prop_assert!(false, "erreur inattendue {e}"),
            Ok(pages) => {
                prop_assert!(!pages.pages.is_empty());
                prop_assert!(pages.pages.len() <= MAX_PAGES);
                let cols = usize::from(pages.pages.iter().map(|pg| pg.col).max().unwrap()) + 1;
                prop_assert_eq!(pages.pages.len() % cols, 0);
                let mut seen = vec![0_u32; sc.rows.len()];
                for pg in pages.pages.iter().filter(|pg| pg.col == 0) {
                    for i in pg.rows.clone() {
                        seen[i] += 1;
                    }
                }
                prop_assert!(seen.iter().all(|c| *c == 1), "{seen:?}");
                let mut next = 0;
                for (k, pg) in pages.pages.iter().enumerate() {
                    prop_assert_eq!(usize::from(pg.col), k % cols);
                    prop_assert_eq!(usize::from(pg.row), k / cols);
                    if sc.rows.is_empty() {
                        prop_assert!(pg.rows.is_empty());
                    } else {
                        prop_assert!(!pg.rows.is_empty(), "page vide {k}");
                    }
                    if pg.col == 0 {
                        prop_assert_eq!(pg.rows.start, next);
                        next = pg.rows.end;
                    }
                    prop_assert!(pg.x16.0 < pg.x16.1 || sc.width16 <= pg.x16.0);
                }
                prop_assert_eq!(next, sc.rows.len());
                // Les tranches couvrent tout le Gantt.
                let last = pages.pages.iter().filter(|pg| pg.row == 0).map(|pg| pg.x16.1).max().unwrap();
                prop_assert!(last >= sc.width16 || sc.width16 <= sc.table16);
                if let plannercraft_render::style::Fit::PagesWide(n) = style.fit {
                    prop_assert!(cols <= usize::from(n) || pages.scale == 1_000);
                }
            }
        }
    }
}

/// H-UI-66, relecture finale de J8 : les en-têtes et pieds des modèles
/// intégrés sont neutres (jetons seulement, et « Page », mot des deux
/// langues) : ni texte français en dur dans une interface anglaise, ni
/// hypothèse contractuelle (« Aléas 10 % ») que le projet ne porte pas.
#[test]
fn h_ui_66_template_slots_language_neutral() {
    for id in TemplateId::ALL {
        let page = template(id).page;
        for Slot(text) in page.header.iter().chain(page.footer.iter()) {
            let mut rest = String::new();
            let mut depth = 0;
            for c in text.chars() {
                match c {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ if depth == 0 => rest.push(c),
                    _ => {}
                }
            }
            let rest = rest.replace("Page", "");
            assert!(
                !rest.chars().any(char::is_alphabetic),
                "{id:?} : texte en dur « {text} »"
            );
        }
    }
}
