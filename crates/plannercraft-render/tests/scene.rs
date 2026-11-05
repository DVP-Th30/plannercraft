//! Scène, géométrie partagée et modèles (lot L4 de J8) : H-UI-42, H-UI-44,
//! H-UI-46, H-UI-47, H-UI-53, H-UI-54, H-R11, P55 (partie scène).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

mod common;

use common::{add, at, link, project, project_p, uid, xml_texts};
use plannercraft_core::{BaselineNumber, LinkType, Moment, Project, Schedule, TaskBaseline};
use plannercraft_render::geometry::{
    ArrowDir, Axis16, BarGeom, Class, Clip, FontRef, FontWeight, InterMetrics, Item, NoTexts, Part, Prim,
    Rect16, ResolvedTexts, RowGeomInput, Scene, TEXT_GAP, TextMeasure, TextSide, Viewport, arrow_dir,
    bar_geom, layout, link_path, place_texts, row_geom_input,
};
use plannercraft_render::style::{BarRole, PlanStyle, TemplateId, Variant, template};
use plannercraft_render::{GanttRequest, Lang, RangeSpec, RenderLimits, RowFilter, Theme, Zoom, svg_styled};
use proptest::prelude::*;

const BODY: FontRef = FontRef {
    weight: FontWeight::Regular,
    size16: 192,
    italic: false,
};

fn scene(p: &Project, s: &Schedule, id: TemplateId, variant: Variant, lang: Lang) -> Scene {
    layout(p, s, &template(id), &NoTexts, &Viewport::new(variant, lang)).0
}

fn texts_of(scene: &Scene, class: Class) -> Vec<&str> {
    scene
        .items
        .iter()
        .filter(|i| i.class == class)
        .filter_map(|i| match &i.prim {
            Prim::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// Exemple chiffré : barre de 3 « Murs » de P au zoom Semaine (Classique).
/// Plage : lun 12/10/2026 (début − 7 j) ; début lun 26/10 08:00 =
/// 14 j × 1 440 + 480 = 20 640 min → x16 = 280 × 16 + 20 640 × 192 / 1 440
/// = 4 480 + 2 752 = 7 232 (452 px) ; fin ven 06/11 17:00 = 37 020 min →
/// 4 936, largeur 2 184 (136,5 px). Ligne 3 (indice 2) : haut 44 + 2 × 22
/// = 88 px, centre 99 px, demi-hauteur 5 px.
#[test]
fn geometry_bar_rect_examples() {
    let p = project_p();
    let s = plannercraft_core::schedule(&p);
    let sc = scene(&p, &s, TemplateId::Classique, Variant::Light, Lang::Fr);
    let murs = sc.rows[2].bar.unwrap();
    assert_eq!(murs.role, BarRole::Critical);
    assert_eq!(
        murs.rect,
        Rect16 {
            x: 7_232,
            y: 99 * 16 - 80,
            w: 2_184,
            h: 160
        }
    );
    // Fondations : lun 19/10 08:00 → 7 × 1 440 + 480 = 10 560 min → 1 408.
    let fondations = sc.rows[1].bar.unwrap();
    assert_eq!(fondations.rect.x, 4_480 + 1_408);
    // Jalon 4 : losange de demi-diagonale 6 px centré à la fin de 3.
    let livraison = sc.rows[3].bar.unwrap();
    assert_eq!(livraison.rect.x + livraison.rect.w / 2, murs.rect.right());
    assert_eq!(livraison.rect.w, 192);
    // Récapitulative 1 : de début de 2 à fin de 4, crochets de 8 px.
    let go = sc.rows[0].bar.unwrap();
    assert_eq!(go.rect.x, fondations.rect.x);
    assert_eq!(go.rect.h, 128);
    // H-UI-42 : la scène et l'appel direct donnent le même rectangle.
    let axis = Axis16::new(&p, &s, RangeSpec::Auto, Zoom::Week, 7_305, 280 * 16);
    let input = row_geom_input(p.tasks.get(uid(3)).unwrap(), s.tasks.get(&uid(3))).unwrap();
    assert_eq!(
        bar_geom(&input, &axis, &template(TemplateId::Classique), 88 * 16),
        murs
    );
}

/// H-UI-42 (J8, lot L16) : le SVG et les pages du PDF lisent la même
/// scène ; la barre de 3 « Murs » (x = 7 232/16 = 452 px au zoom Semaine,
/// voir `geometry_bar_rect_examples`) est au même endroit dans le SVG et
/// dans la scène paginée.
#[test]
fn h_ui_42_svg_and_pdf_pages_share_scene_geometry() {
    let p = project_p();
    let s = plannercraft_core::schedule(&p);
    let st = template(TemplateId::Classique);
    let vp = Viewport {
        zoom: Zoom::Week,
        ..Viewport::new(Variant::Light, Lang::Fr)
    };
    let (sc, _) = layout(&p, &s, &st, &NoTexts, &vp);
    let murs = sc.rows[2].bar.unwrap();
    assert_eq!(murs.rect.x, 7_232);
    let (svg, _) = svg_styled(&p, &s, &request(Theme::Light, Lang::Fr), &st, &NoTexts);
    assert!(
        svg.contains("x=\"452.0000\""),
        "barre de 3 absente du SVG à x = 452"
    );
    let mut page = st.page.clone();
    page.fit = plannercraft_render::style::Fit::Scale(100);
    let pages = plannercraft_render::paginate::paginate(&sc, &page, &NoTexts).unwrap();
    assert_eq!(pages.scene.rows[2].bar.unwrap().rect, murs.rect);
}

fn geom(start: &str, finish: &str, row: i64) -> BarGeom {
    let p = project("L", "2026-11-02T08:00");
    let s = plannercraft_core::schedule(&p);
    let axis = Axis16::new(
        &p,
        &s,
        RangeSpec::Span {
            from: "2026-11-02".parse().unwrap(),
            to: "2026-11-30".parse().unwrap(),
        },
        Zoom::Day,
        7_305,
        0,
    );
    let input = RowGeomInput {
        uid: uid(1),
        role: BarRole::Task,
        start: Some(at(start)),
        finish: Some(at(finish)),
        percent: 0,
        stop: None,
        baseline: None,
    };
    bar_geom(&input, &axis, &template(TemplateId::Classique), row * 22 * 16)
}

/// H-R7 : FS direct par le dessus, SS et FF par le côté, SF replié ; au plus
/// 6 points, flèche sur le dernier.
#[test]
fn link_path_fs_ss_ff_sf() {
    let a = geom("2026-11-02T08:00", "2026-11-03T17:00", 0);
    let b = geom("2026-11-05T08:00", "2026-11-06T17:00", 1);
    let fs = link_path(&a, &b, LinkType::FinishToStart);
    assert_eq!(fs.points.len(), 3);
    assert_eq!(fs.points[0], (a.rect.right(), a.yc16));
    assert_eq!(fs.points[2], (b.rect.x + 6 * 16, b.yc16 - 80));
    assert_eq!(arrow_dir(&fs), Some(ArrowDir::Down));
    let ss = link_path(&a, &b, LinkType::StartToStart);
    assert_eq!(ss.points.last(), Some(&(b.rect.x, b.yc16)));
    assert_eq!(arrow_dir(&ss), Some(ArrowDir::Right));
    let ff = link_path(&a, &b, LinkType::FinishToFinish);
    assert_eq!(ff.points.last(), Some(&(b.rect.right(), b.yc16)));
    assert_eq!(arrow_dir(&ff), Some(ArrowDir::Left));
    let sf = link_path(&a, &b, LinkType::StartToFinish);
    assert_eq!(sf.points.len(), 6);
    assert_eq!(sf.points.last(), Some(&(b.rect.right(), b.yc16)));
    assert_eq!(arrow_dir(&sf), Some(ArrowDir::Left));
    // FS replié : successeur qui commence avant la fin du prédécesseur.
    let c = geom("2026-11-03T08:00", "2026-11-04T17:00", 2);
    let folded = link_path(&a, &c, LinkType::FinishToStart);
    assert_eq!(folded.points.len(), 6);
    assert_eq!(arrow_dir(&folded), Some(ArrowDir::Right));
    // Lien vers le haut.
    let up = link_path(
        &c,
        &geom("2026-11-10T08:00", "2026-11-11T17:00", 0),
        LinkType::FinishToStart,
    );
    assert_eq!(arrow_dir(&up), Some(ArrowDir::Up));
    for path in [fs, ss, ff, sf, folded, up] {
        assert!(path.points.len() <= 6);
        assert_eq!(path.arrow.map(|a| a[0]), path.points.last().copied());
    }
}

fn texts(right: Option<&str>, left: Option<&str>, inside: Option<&str>) -> ResolvedTexts {
    ResolvedTexts {
        left: left.map(str::to_owned),
        right: right.map(str::to_owned),
        inside: inside.map(str::to_owned),
        font: BODY,
    }
}

fn bar_at(x: i64, w: i64) -> BarGeom {
    let mut b = geom("2026-11-02T08:00", "2026-11-03T17:00", 0);
    b.rect.x = x;
    b.rect.w = w;
    b
}

/// H-UI-46 : barre finissant à 8 px du bord ⇒ nom à gauche, entier.
#[test]
fn h_ui_46_text_flips_left() {
    let clip = (0, 1_000 * 16);
    let bar = bar_at(800 * 16, 192 * 16);
    let placed = place_texts(&bar, &texts(Some("Peinture"), None, None), &InterMetrics, clip);
    let t = &placed.items[0];
    assert_eq!(t.side, TextSide::Left);
    assert_eq!(t.text, "Peinture");
    assert_eq!(t.x16 + t.width16 + TEXT_GAP, bar.rect.x);
    // Place à droite : reste à droite.
    let bar = bar_at(100 * 16, 50 * 16);
    let placed = place_texts(&bar, &texts(Some("Peinture"), None, None), &InterMetrics, clip);
    assert_eq!(placed.items[0].side, TextSide::Right);
    assert_eq!(placed.items[0].x16, bar.rect.right() + TEXT_GAP);
    // Texte à gauche sans place : basculé à droite.
    let bar = bar_at(10 * 16, 50 * 16);
    let placed = place_texts(&bar, &texts(None, Some("Fondations"), None), &InterMetrics, clip);
    assert_eq!(placed.items[0].side, TextSide::Right);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// H-UI-64 (J8 L15) : deux étiquettes d'une même barre (gauche et
    /// droite, l'une basculée du côté de l'autre) ne se recouvrent jamais.
    #[test]
    fn h_ui_64_labels_never_overlap(
        x in -200_i64..1_200,
        w in 0_i64..600,
        left in "[A-Za-z ]{1,30}",
        right in "[A-Za-z ]{1,30}",
        clip_w in 0_i64..1_000,
    ) {
        let clip = (0, clip_w * 16);
        let bar = bar_at(x * 16, w * 16);
        let placed = place_texts(&bar, &texts(Some(&right), Some(&left), None), &InterMetrics, clip).items;
        for (i, a) in placed.iter().enumerate() {
            for b in placed.iter().skip(i + 1) {
                let disjoint = a.x16 + a.width16 <= b.x16 || b.x16 + b.width16 <= a.x16;
                prop_assert!(disjoint, "{a:?} recouvre {b:?}");
            }
        }
    }

    /// H-UI-46 : un texte placé tient toujours dans la zone (jamais rogné) ;
    /// il est entier ou tronqué avec « … », et sa largeur est mesurée.
    #[test]
    fn h_ui_46_ellipsis_never_clipped(
        x in -200_i64..1_200,
        w in 0_i64..600,
        name in "[A-Za-zéœ ]{0,60}",
        side in 0_u8..3,
        clip_w in 0_i64..1_000,
    ) {
        let clip = (0, clip_w * 16);
        let bar = bar_at(x * 16, w * 16);
        let t = match side {
            0 => texts(Some(&name), None, None),
            1 => texts(None, Some(&name), None),
            _ => texts(None, None, Some(&name)),
        };
        for item in place_texts(&bar, &t, &InterMetrics, clip).items {
            prop_assert!(item.x16 >= clip.0, "{item:?}");
            prop_assert!(item.x16 + item.width16 <= clip.1, "{item:?}");
            prop_assert_eq!(item.width16, InterMetrics.width16(&item.text, BODY));
            prop_assert!(item.text == name || item.text.ends_with('…'));
            if item.side == TextSide::Inside {
                prop_assert!(item.x16 >= bar.rect.x && item.x16 + item.width16 <= bar.rect.right());
            }
        }
    }
}

/// H-UI-44 : dans chaque modèle (sauf Classique, J7 consigné), la barre et
/// le jalon critiques portent un indice non coloré : motif, ou contour
/// ≥ 2 px plus épais que la tâche.
#[test]
fn h_ui_44_critical_has_non_color_cue() {
    // P plus un jalon final critique « Réception » (7→8).
    let mut p = project_p();
    let t8 = add(&mut p, "Réception", 0, 1);
    link(&mut p, uid(7), t8, LinkType::FinishToStart);
    let s = plannercraft_core::schedule(&p);
    for id in TemplateId::ALL {
        let st = template(id);
        for variant in [Variant::Light, Variant::Dark, Variant::Print] {
            let sc = scene(&p, &s, id, variant, Lang::Fr);
            for (crit, plain) in [
                (BarRole::Critical, BarRole::Task),
                (BarRole::MilestoneCritical, BarRole::Milestone),
            ] {
                assert!(
                    sc.items.iter().any(|i| i.class == Class::Bar(crit)),
                    "{id:?} {crit:?}"
                );
                let hatched = sc.items.iter().any(|i| i.class == Class::Pattern(crit));
                let width = sc.paints[&Class::Bar(crit)]
                    .stroke
                    .map_or(0, |_| sc.paints[&Class::Bar(crit)].width16);
                let thicker = width >= 32 && width > i64::from(st.bars[&plain].outline16);
                assert_eq!(
                    hatched || thicker,
                    id != TemplateId::Classique,
                    "{id:?} {variant:?} {crit:?}"
                );
            }
        }
    }
}

/// H-UI-47 : la légende ne montre que les rôles présents ; un projet sans
/// ligne de base n'a pas d'entrée « Ligne de base ».
#[test]
fn h_ui_47_legend_only_present_roles() {
    let mut p = project_p();
    let s = plannercraft_core::schedule(&p);
    let sc = scene(&p, &s, TemplateId::Sobre, Variant::Light, Lang::Fr);
    assert_eq!(
        sc.legend,
        [BarRole::Critical, BarRole::SummaryCritical, BarRole::Milestone]
    );
    let legend: Vec<&str> = sc
        .items
        .iter()
        .filter(|i| i.part == Part::Legend)
        .filter_map(|i| match &i.prim {
            Prim::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(legend, ["Tâche critique", "Récapitulative critique", "Jalon"]);
    assert!(sc.height16 > sc.rows.last().unwrap().y16 + sc.rows.last().unwrap().h16);
    // Ligne de base sur 6 : l'entrée apparaît ; avancement sur 2 aussi.
    p.tasks.get_mut(uid(6)).unwrap().baselines.insert(
        BaselineNumber::PRIMARY,
        TaskBaseline {
            start: Some(at("2026-11-09T08:00")),
            finish: Some(at("2026-11-12T17:00")),
            ..TaskBaseline::default()
        },
    );
    let s = plannercraft_core::schedule(&p);
    let sc = scene(&p, &s, TemplateId::Sobre, Variant::Light, Lang::En);
    assert!(sc.legend.contains(&BarRole::Baseline));
    assert!(texts_of(&sc, Class::Text).contains(&"Baseline"));
    // Classique (J7) : pas de légende.
    assert_eq!(
        scene(&p, &s, TemplateId::Classique, Variant::Light, Lang::Fr).legend,
        []
    );
}

fn band_rows(sc: &Scene, class: Class) -> Vec<u32> {
    sc.items
        .iter()
        .filter(|i| i.class == class)
        .filter_map(|i| match i.part {
            Part::Row(r) => Some(r),
            _ => None,
        })
        .collect()
}

/// H-UI-53 : Présentation, bandes de phase (1 et ses enfants sur la bande
/// A, 5 et les siens sur la bande B), dans la table seulement (H-UI-62) ;
/// Sobre retouché en L15 : plus de zébrage.
#[test]
fn h_ui_53_phase_bands() {
    let p = project_p();
    let s = plannercraft_core::schedule(&p);
    let pres = scene(&p, &s, TemplateId::Presentation, Variant::Light, Lang::Fr);
    assert_eq!(band_rows(&pres, Class::Band(1)), [4, 5, 6]);
    for it in pres.items.iter().filter(|i| i.class == Class::Band(1)) {
        assert_eq!(it.clip, Clip::Table);
        let Prim::Rect { r, .. } = &it.prim else {
            panic!("bande non rectangulaire");
        };
        assert_eq!((r.x, r.right()), (0, pres.table16));
    }
    // Bande A = fond : rien à peindre.
    assert_eq!(band_rows(&pres, Class::Band(0)), Vec::<u32>::new());
    let sobre = scene(&p, &s, TemplateId::Sobre, Variant::Light, Lang::Fr);
    assert_eq!(band_rows(&sobre, Class::Band(1)), Vec::<u32>::new());
    let classique = scene(&p, &s, TemplateId::Classique, Variant::Light, Lang::Fr);
    assert_eq!(band_rows(&classique, Class::Band(1)), Vec::<u32>::new());
}

/// Segments horizontaux d'une primitive (`y0 == y1`), avec leur ordonnée.
fn horizontal_segments(it: &Item) -> Vec<(i64, i64, i64)> {
    match &it.prim {
        Prim::Segments { segs } => segs
            .iter()
            .filter(|(_, y0, _, y1)| y0 == y1)
            .map(|&(x0, y, x1, _)| (x0, x1, y))
            .collect(),
        _ => Vec::new(),
    }
}

/// H-UI-62 (scène, tous les modèles) : aucun fond de ligne ni trait
/// horizontal dans la zone graphique sous l'en-tête ; seules les verticales
/// de l'échelle et les jours chômés y sont peints.
#[test]
fn h_ui_62_scene_vertical_grid_only() {
    let p = project_p();
    let s = plannercraft_core::schedule(&p);
    for id in TemplateId::ALL {
        let sc = scene(&p, &s, id, Variant::Light, Lang::Fr);
        for it in &sc.items {
            if matches!(it.class, Class::Band(_) | Class::RowLine | Class::PhaseRow(_)) {
                assert_ne!(it.clip, Clip::Gantt, "{id:?} {it:?}");
                if let Prim::Rect { r, .. } = &it.prim {
                    assert!(r.right() <= sc.table16, "{id:?} {it:?}");
                }
            }
            for (x0, x1, y) in horizontal_segments(it) {
                let in_body = y > sc.header16;
                let in_gantt = x1 > sc.table16;
                assert!(
                    !(in_body && in_gantt),
                    "{id:?} : trait horizontal {x0}..{x1} à {y} dans le Gantt"
                );
            }
        }
    }
}

/// H-UI-62 : aux échelles larges, une fermeture (au moins 3 jours chômés de
/// suite) est grisée, une fin de semaine seule non.
#[test]
fn h_ui_62_long_closures_shaded_at_month_zoom() {
    use plannercraft_core::calendar::{CalendarException, DaySetting, Recurrence};
    use plannercraft_core::{Day, DayRange};
    let month = |p: &Project| {
        let s = plannercraft_core::schedule(p);
        let vp = Viewport {
            zoom: Zoom::Month,
            ..Viewport::new(Variant::Light, Lang::Fr)
        };
        let sc = layout(p, &s, &template(TemplateId::Sobre), &NoTexts, &vp).0;
        sc.items.iter().filter(|i| i.class == Class::NonWorking).count()
    };
    let mut p = project_p();
    assert_eq!(month(&p), 0, "fins de semaine seules : rien à l'échelle Mois");
    let day = |s: &str| s.parse::<Day>().unwrap();
    let closure = CalendarException {
        name: "Fermeture".to_owned(),
        period: DayRange::new(day("2026-11-02"), day("2026-11-06")).unwrap(),
        setting: DaySetting::NonWorking,
        recurrence: Recurrence::Period,
        occurrences: None,
    };
    let cal = p.calendar;
    p.calendars.get_mut(cal).unwrap().add_exception(closure).unwrap();
    // Sam 31/10 au dim 08/11 : une seule période grisée.
    assert_eq!(month(&p), 1);
}

/// H-UI-64 : les liens sont dessinés avant les étiquettes des barres
/// (étiquettes au-dessus des traits).
#[test]
fn h_ui_64_scene_labels_after_links() {
    let p = project_p();
    let s = plannercraft_core::schedule(&p);
    let sc = layout(
        &p,
        &s,
        &template(TemplateId::Sobre),
        &NoTexts,
        &Viewport::new(Variant::Light, Lang::Fr),
    )
    .0;
    let last_link = sc
        .items
        .iter()
        .rposition(|i| {
            matches!(
                i.class,
                Class::Link | Class::LinkCritical | Class::Arrow | Class::ArrowCritical
            )
        })
        .unwrap();
    let first_label = sc
        .items
        .iter()
        .position(|i| {
            i.clip == Clip::Gantt && matches!(i.part, Part::Row(_)) && matches!(i.prim, Prim::Text { .. })
        })
        .unwrap();
    assert!(last_link < first_label, "{last_link} ≥ {first_label}");
}

/// H-UI-65 : une récapitulative colorée peint sa barre, celles de ses
/// sous-tâches (option) et le fond de sa ligne dans la table, en teinte
/// lisible.
#[test]
fn h_ui_65_scene_phase_row_and_bars() {
    use plannercraft_render::style::{PhaseColor, Rgb, contrast_ratio};
    let p = project_p();
    let s = plannercraft_core::schedule(&p);
    let mut st = template(TemplateId::Sobre);
    st.phases.insert(
        5,
        PhaseColor {
            fill: Rgb(0xE0, 0x10, 0x10),
            children: true,
        },
    );
    let sc = layout(&p, &s, &st, &NoTexts, &Viewport::new(Variant::Light, Lang::Fr)).0;
    // Ligne 5 (indice 4) : fond de phase dans la table.
    assert_eq!(band_rows(&sc, Class::PhaseRow(0)), [4]);
    // Barres : 5 (récapitulative), 6 et 7 (sous-tâches) à la couleur.
    let phased: Vec<u32> = sc
        .items
        .iter()
        .filter(|i| matches!(i.class, Class::Phase(0, _)))
        .filter_map(|i| match i.part {
            Part::Row(r) => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(phased, [4, 5, 6]);
    let row_fill = sc.paints[&Class::PhaseRow(0)].fill.unwrap();
    let text = st.tokens(Variant::Light).text_strong;
    assert!(contrast_ratio(text, row_fill) >= 4_500);
    // Sans `children` : la récapitulative seule.
    st.phases.insert(
        5,
        PhaseColor {
            fill: Rgb(0xE0, 0x10, 0x10),
            children: false,
        },
    );
    let sc = layout(&p, &s, &st, &NoTexts, &Viewport::new(Variant::Light, Lang::Fr)).0;
    assert_eq!(
        sc.items
            .iter()
            .filter(|i| matches!(i.class, Class::Phase(0, _)))
            .count(),
        1
    );
}

/// H-UI-67 : niveau des mois relatifs (M1 = mois du démarrage) au-dessus
/// des deux niveaux de l'échelle ; en-tête plus haut d'un niveau.
#[test]
fn h_ui_67_scene_relative_months() {
    let p = project_p();
    let s = plannercraft_core::schedule(&p);
    let mut st = template(TemplateId::Sobre);
    let plain = layout(&p, &s, &st, &NoTexts, &Viewport::new(Variant::Light, Lang::Fr)).0;
    st.scale.relative_months = true;
    let sc = layout(&p, &s, &st, &NoTexts, &Viewport::new(Variant::Light, Lang::Fr)).0;
    assert_eq!(sc.header16, plain.header16 + 22 * 16);
    let labels = texts_of(&sc, Class::Title);
    // Plage du 12/10/2026 à fin novembre : octobre = M1, novembre = M2.
    assert_eq!(labels.first().copied(), Some("M1"), "{labels:?}");
    assert!(labels.contains(&"M2"), "{labels:?}");
    assert_eq!(plannercraft_render::geometry::relative_month_label(-3), "M-3");
    let d = |s: &str| s.parse::<plannercraft_core::Day>().unwrap();
    assert_eq!(
        plannercraft_render::geometry::relative_month(d("2026-01-15"), d("2026-01-05")),
        1
    );
    assert_eq!(
        plannercraft_render::geometry::relative_month(d("2025-10-01"), d("2026-01-05")),
        -3
    );
    assert_eq!(
        plannercraft_render::geometry::relative_month(d("2027-11-30"), d("2026-01-05")),
        23
    );
}

/// H-UI-68 et H-UI-64 : colonnes de la table tirées du style (N°, Nom,
/// Durée, Début, Fin), table masquée pour « graphique seul ».
#[test]
fn h_ui_68_scene_print_columns() {
    use plannercraft_render::style::TableColumn;
    let p = project_p();
    let s = plannercraft_core::schedule(&p);
    let mut st = template(TemplateId::Sobre);
    let plain = layout(&p, &s, &st, &NoTexts, &Viewport::new(Variant::Light, Lang::Fr)).0;
    assert_eq!(plain.table16, 280 * 16);
    st.page.columns = Some(TableColumn::PRINT_DEFAULT.to_vec());
    let sc = layout(&p, &s, &st, &NoTexts, &Viewport::new(Variant::Light, Lang::Fr)).0;
    assert_eq!(sc.table16, (280 + 64 * 3) * 16);
    let titles = texts_of(&sc, Class::TextStrong);
    for t in ["N°", "Nom de la tâche", "Durée", "Début", "Fin"] {
        assert!(titles.contains(&t), "{t} absent : {titles:?}");
    }
    st.page.columns = Some(Vec::new());
    let chart = layout(&p, &s, &st, &NoTexts, &Viewport::new(Variant::Light, Lang::Fr)).0;
    assert_eq!(chart.table16, 0);
    assert!(
        !chart
            .items
            .iter()
            .any(|i| i.clip == Clip::Table && matches!(i.prim, Prim::Text { .. })),
        "table masquée"
    );
}

/// H-UI-54 : date d'état 02/11 ⇒ « État 02/11 » dans l'échelle, aujourd'hui
/// en pointillé distinct.
#[test]
fn h_ui_54_status_label() {
    let mut p = project_p();
    p.status_date = Some(at("2026-11-02T17:00"));
    p.current_date = at("2026-11-05T10:00");
    let s = plannercraft_core::schedule(&p);
    let sc = scene(&p, &s, TemplateId::Sobre, Variant::Light, Lang::Fr);
    assert_eq!(texts_of(&sc, Class::StatusLabel), ["État 02/11"]);
    let lines = |sc: &Scene, class| sc.items.iter().filter(|i: &&Item| i.class == class).count();
    assert_eq!(lines(&sc, Class::StatusLine), 1);
    assert_eq!(lines(&sc, Class::Today), 1);
    assert_ne!(
        sc.paints[&Class::Today].dash16,
        sc.paints[&Class::StatusLine].dash16
    );
    let en = scene(&p, &s, TemplateId::Sobre, Variant::Light, Lang::En);
    assert_eq!(texts_of(&en, Class::StatusLabel), ["Status 11/02"]);
    // Classique (J7) : ligne seule, ni étiquette ni aujourd'hui.
    let cl = scene(&p, &s, TemplateId::Classique, Variant::Light, Lang::Fr);
    assert_eq!(texts_of(&cl, Class::StatusLabel), Vec::<&str>::new());
    assert_eq!(lines(&cl, Class::StatusLine), 1);
    assert_eq!(lines(&cl, Class::Today), 0);
}

/// Chantier : numéros de semaine (« S44 · 26 » : lun 26/10/2026) et jours
/// chômés grisés.
#[test]
fn chantier_week_numbers_and_non_working() {
    let p = project_p();
    let s = plannercraft_core::schedule(&p);
    let sc = scene(&p, &s, TemplateId::Chantier, Variant::Light, Lang::Fr);
    assert!(
        texts_of(&sc, Class::Text)
            .iter()
            .any(|t| t.starts_with("S44 · 26")),
        "{:?}",
        texts_of(&sc, Class::Text)
    );
    assert!(sc.items.iter().any(|i| i.class == Class::NonWorking));
    assert!(
        !scene(&p, &s, TemplateId::Classique, Variant::Light, Lang::Fr)
            .items
            .iter()
            .any(|i| i.class == Class::NonWorking)
    );
}

fn request(theme: Theme, lang: Lang) -> GanttRequest {
    GanttRequest {
        range: RangeSpec::Auto,
        zoom: Zoom::Week,
        filter: RowFilter::default(),
        language: lang,
        theme,
        limits: RenderLimits::default(),
    }
}

/// Mêmes entrées, mêmes octets, pour chaque modèle et variante ; documents
/// bien formés, noms présents.
#[test]
fn svg_deterministic() {
    let (p, s) = common::house();
    for id in TemplateId::ALL {
        let st = template(id);
        for theme in [Theme::Light, Theme::Dark] {
            for lang in [Lang::Fr, Lang::En] {
                let req = request(theme, lang);
                let (a, ra) = svg_styled(&p, &s, &req, &st, &NoTexts);
                let (b, rb) = svg_styled(&p.clone(), &plannercraft_core::schedule(&p), &req, &st, &NoTexts);
                assert_eq!(a, b, "{id:?}");
                assert_eq!(ra, rb);
                assert_eq!(ra.bytes, a.len());
                let texts = xml_texts(&a);
                assert!(texts.iter().any(|t| t == "Études et permis"), "{id:?}");
            }
        }
    }
}

/// Projet généré : `n` feuilles en chaînes, quelques récapitulatives,
/// jalons, liens de tous types.
fn generated(spec: &[(u8, u8, u8)]) -> Project {
    let mut p = project("G", "2026-10-19T08:00");
    let mut prev = None;
    for (i, &(days, level, kind)) in spec.iter().enumerate() {
        let level = if i == 0 { 1 } else { u16::from(level % 3) + 1 };
        let u = add(&mut p, &format!("T{i}"), i64::from(days % 6), level);
        if let Some(pr) = prev
            && kind % 3 != 0
        {
            let lk = match kind % 4 {
                0 => LinkType::FinishToStart,
                1 => LinkType::StartToStart,
                2 => LinkType::FinishToFinish,
                _ => LinkType::StartToFinish,
            };
            link(&mut p, pr, u, lk);
        }
        prev = Some(u);
    }
    p
}

fn bar_of(st: &PlanStyle, p: &Project, s: &Schedule, axis: &Axis16, i: usize, top: i64) -> Option<BarGeom> {
    let task = p.tasks.iter().nth(i)?;
    let input = row_geom_input(task, s.tasks.get(&task.uid()))?;
    Some(bar_geom(&input, axis, st, top))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// P55 (partie scène) : pour tout projet généré et tout zoom, les
    /// rectangles de barre, les points des liens et l'abscisse de la date
    /// d'état de la scène égalent ceux des fonctions de géométrie appelées
    /// ligne par ligne (chemin de l'écran, L11).
    #[test]
    fn p55_scene_matches_geometry(
        spec in proptest::collection::vec((0_u8..8, 0_u8..3, 0_u8..6), 1..25),
        zoom in 0_u8..4,
        id in 0_usize..6,
        status in 0_i64..60,
    ) {
        let mut p = generated(&spec);
        p.status_date = Some(Moment::from_minutes(at("2026-10-12T00:00").minutes() + status * 1_440).unwrap());
        let s = plannercraft_core::schedule(&p);
        let zoom = [Zoom::Day, Zoom::Week, Zoom::Month, Zoom::Quarter][usize::from(zoom)];
        let st = template(TemplateId::ALL[id]);
        let vp = Viewport { zoom, ..Viewport::new(Variant::Light, Lang::Fr) };
        let (sc, _) = layout(&p, &s, &st, &NoTexts, &vp);
        let axis = Axis16::new(&p, &s, RangeSpec::Auto, zoom, vp.limits.max_days, sc.table16);
        let row_h = i64::from(st.density.row_px()) * 16;
        let mut bars = Vec::new();
        for (i, row) in sc.rows.iter().enumerate() {
            let top = sc.header16 + i64::try_from(i).unwrap() * row_h;
            prop_assert_eq!(row.y16, top);
            let screen = bar_of(&st, &p, &s, &axis, i, top);
            prop_assert_eq!(row.bar, screen);
            bars.push(screen);
        }
        let mut links = Vec::new();
        for (to, task) in p.tasks.iter().enumerate() {
            for l in &task.predecessors {
                let from = p.tasks.iter().position(|t| t.uid() == l.predecessor).unwrap();
                if let (Some(Some(a)), Some(Some(b))) = (bars.get(from), bars.get(to)) {
                    let path = link_path(a, b, l.kind);
                    if !path.points.is_empty() {
                        links.push((from.max(to), path));
                    }
                }
            }
        }
        links.sort_by_key(|(last, _)| *last);
        let links: Vec<_> = links.into_iter().map(|(_, path)| path).collect();
        prop_assert_eq!(&sc.links, &links);
        let x = axis.x16(p.effective_status_date());
        let drawn: Vec<i64> = sc.items.iter().filter(|i| i.class == Class::StatusLine).filter_map(|i| match &i.prim {
            Prim::Segments { segs } => segs.first().map(|s| s.0),
            _ => None,
        }).collect();
        if x >= axis.left16() && x <= axis.right16() {
            prop_assert_eq!(drawn, vec![x]);
        } else {
            prop_assert!(drawn.is_empty());
        }
    }
}

/// Relecture finale de J8 (F3) : dans la table imprimée (colonnes N°, Nom,
/// Durée, Début, Fin), aucun texte ne déborde de sa colonne, nom long en
/// retrait ou en gras (récapitulative) compris : le nom est coupé à la
/// largeur de sa colonne, retrait déduit.
fn assert_table_texts_in_columns(names: &[(String, u16)], id: TemplateId) {
    use plannercraft_render::style::TableColumn;
    let mut p = project("T", "2026-10-19T08:00");
    for (name, level) in names {
        add(&mut p, name, 3, *level);
    }
    let s = plannercraft_core::schedule(&p);
    let mut st = template(id);
    st.page.columns = Some(TableColumn::PRINT_DEFAULT.to_vec());
    let sc = layout(&p, &s, &st, &NoTexts, &Viewport::new(Variant::Print, Lang::Fr)).0;
    // Bords des colonnes (N° 40 px, Nom 240 px, dates 64 px).
    let mut edges = Vec::new();
    let mut x = 0;
    for w in [40, 240, 64, 64, 64] {
        edges.push((x * 16, (x + w) * 16));
        x += w;
    }
    let mut seen_name = false;
    for item in sc.items.iter().filter(|i| i.clip == Clip::Table) {
        let Prim::Text {
            x16,
            text,
            font,
            anchor,
            ..
        } = &item.prim
        else {
            continue;
        };
        if !matches!(item.part, Part::Row(_)) {
            continue;
        }
        let w = InterMetrics.width16(text, *font);
        let (left, right) = match anchor {
            plannercraft_render::geometry::Anchor::End => (x16 - w, *x16),
            plannercraft_render::geometry::Anchor::Start => (*x16, x16 + w),
        };
        let (c0, c1) = *edges
            .iter()
            .find(|(a, b)| *x16 >= *a && *x16 <= *b)
            .expect("texte hors de la table");
        if c0 == 40 * 16 {
            seen_name = true;
        }
        assert!(
            left >= c0 && right <= c1,
            "« {text} » ({left}..{right}) déborde de la colonne {c0}..{c1}"
        );
    }
    assert!(seen_name || names.iter().all(|(n, _)| n.is_empty()));
}

#[test]
fn table_texts_fit_columns_vaulx_example() {
    let names = vec![
        ("TRAVAUX".to_owned(), 1),
        ("Bâtiment A".to_owned(), 2),
        (
            "Travaux en cycle (48 logements, 1 commencé tous les 2 jours)".to_owned(),
            3,
        ),
        ("Cloisons".to_owned(), 4),
    ];
    for id in TemplateId::ALL {
        assert_table_texts_in_columns(&names, id);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn table_texts_fit_columns(
        names in proptest::collection::vec(("[A-Za-zéÉW (),0-9]{0,90}", 1_u16..8), 1..12)
    ) {
        // Niveaux plausibles : chaque ligne au plus un cran sous la précédente.
        let mut level = 0_u16;
        let names: Vec<(String, u16)> = names
            .into_iter()
            .map(|(n, l)| {
                level = l.min(level + 1);
                (n, level)
            })
            .collect();
        assert_table_texts_in_columns(&names, TemplateId::Sobre);
    }
}

/// Source qui donne un texte à chaque champ.
struct AllTexts;

impl plannercraft_render::geometry::BarTextSource for AllTexts {
    fn bar_text(
        &self,
        _uid: plannercraft_core::TaskUid,
        f: plannercraft_render::style::BarField,
    ) -> Option<String> {
        Some(format!("{f:?}"))
    }

    fn token(&self, _t: plannercraft_render::geometry::Token) -> String {
        String::new()
    }
}

/// H-UI-63 et H-UI-64, valables pour tous les modèles (relecture finale de
/// J8, F4) : sous chaque modèle stylé (Classique garde le rendu nu de J7),
/// chaque barre porte au moins une étiquette, tâche manuelle et inactive
/// comprises ; récapitulative : nom à gauche, durée à droite ; jalon : date
/// à gauche, nom à droite.
#[test]
fn h_ui_64_every_bar_labelled_in_every_template() {
    use plannercraft_core::SchedulingMode;
    use plannercraft_render::geometry::TextSide;
    let mut p = project_p();
    let manual = add(&mut p, "Manuelle", 2, 2);
    p.tasks.get_mut(manual).unwrap().mode = SchedulingMode::Manual {
        start: Some(at("2026-10-21T08:00")),
        finish: None,
        duration: None,
    };
    let inactive = add(&mut p, "Inactive", 2, 2);
    p.tasks.get_mut(inactive).unwrap().active = false;
    let s = plannercraft_core::schedule(&p);
    for id in TemplateId::ALL {
        if id == TemplateId::Classique {
            continue;
        }
        let sc = layout(
            &p,
            &s,
            &template(id),
            &AllTexts,
            &Viewport::new(Variant::Print, Lang::Fr),
        )
        .0;
        let mut roles = std::collections::BTreeSet::new();
        for row in &sc.rows {
            let Some(bar) = &row.bar else { continue };
            roles.insert(format!("{:?}", bar.role));
            assert!(
                !row.texts.items.is_empty(),
                "{id:?} : barre {:?} de la ligne {} sans étiquette",
                bar.role,
                row.id
            );
            let side = |s: TextSide| {
                row.texts
                    .items
                    .iter()
                    .find(|t| t.side == s)
                    .map(|t| t.text.as_str())
            };
            match bar.role {
                BarRole::Summary | BarRole::SummaryCritical => {
                    assert_eq!(side(TextSide::Right), Some("DurationShort"), "{id:?}");
                }
                BarRole::Milestone | BarRole::MilestoneCritical => {
                    assert_eq!(side(TextSide::Left), Some("FinishShort"), "{id:?}");
                    assert_eq!(side(TextSide::Right), Some("Name"), "{id:?}");
                }
                _ => {}
            }
        }
        for role in ["Manual", "Inactive", "Summary", "Critical"] {
            assert!(
                roles.iter().any(|r| r.starts_with(role)),
                "{id:?} : rôle {role} absent : {roles:?}"
            );
        }
        assert!(roles.iter().any(|r| r.starts_with("Milestone")), "{roles:?}");
    }
}

/// H-R3, relecture finale de J8 (F9) : impression ajustée d'un projet de
/// deux ans sur une page A3 : la plage automatique s'allonge pour que les
/// étiquettes des barres du bord (nom de la récapitulative à gauche, sa
/// durée et le nom du jalon final à droite) tiennent de leur côté,
/// entières.
#[test]
fn h_r3_fill_keeps_end_labels_on_the_right() {
    use plannercraft_render::geometry::TextSide;
    use plannercraft_render::paginate::fit_zoom;
    use plannercraft_render::style::Paper;
    let mut p = project("Deux ans", "2026-01-05T08:00");
    add(&mut p, "TRAVAUX", 0, 1);
    let structure = add(&mut p, "Gros œuvre", 300, 2);
    let finishes = add(&mut p, "Second œuvre", 220, 2);
    let handover = add(&mut p, "Réception des travaux du lot principal", 0, 2);
    link(&mut p, structure, finishes, LinkType::FinishToStart);
    link(&mut p, finishes, handover, LinkType::FinishToStart);
    let s = plannercraft_core::schedule(&p);
    let mut st = template(TemplateId::Sobre);
    st.page.paper = Paper::A3;
    st.page.landscape = true;
    let vp = Viewport {
        zoom: fit_zoom(&st.page).unwrap(),
        ..Viewport::new(Variant::Print, Lang::Fr)
    };
    let sc = layout(&p, &s, &st, &AllTexts, &vp).0;
    // Récapitulative : nom à gauche, durée à droite ; jalon de fin : date
    // à gauche, nom à droite ; rien de tronqué.
    for (i, left, right) in [(0, "Name", "DurationShort"), (3, "FinishShort", "Name")] {
        let items = &sc.rows[i].texts.items;
        let side = |want: &str| items.iter().find(|t| t.text == want).map(|t| t.side);
        assert_eq!(side(left), Some(TextSide::Left), "ligne {i} : {items:?}");
        assert_eq!(side(right), Some(TextSide::Right), "ligne {i} : {items:?}");
    }
}

/// H-UI-61, relecture finale de J8 : un texte dans la barre prend l'encre
/// (texte ou fond) la plus contrastée sur le remplissage : en Impression
/// N&B, plus de texte noir sur la barre critique noire.
#[test]
fn h_ui_61_inside_text_ink_on_bar() {
    use plannercraft_render::style::{BarField, BarTexts, contrast_ratio, ink_on};
    let p = project_p();
    let s = plannercraft_core::schedule(&p);
    for id in TemplateId::ALL {
        let mut st = template(id);
        for role in [BarRole::Task, BarRole::Critical] {
            st.texts.insert(
                role,
                BarTexts {
                    inside: Some(BarField::Name),
                    ..BarTexts::default()
                },
            );
        }
        for v in [Variant::Light, Variant::Dark, Variant::Print] {
            let sc = layout(&p, &s, &st, &NoTexts, &Viewport::new(v, Lang::Fr)).0;
            let c = st.tokens(v);
            let mut seen = 0;
            for item in &sc.items {
                let Class::TextOnBar(role, None) = item.class else {
                    continue;
                };
                seen += 1;
                let fill = c.bars[&role].fill;
                let ink = sc.paints[&item.class].fill.unwrap();
                assert_eq!(ink, ink_on(fill, c.text, c.bg), "{id:?} {v:?} {role:?}");
                assert!(
                    contrast_ratio(ink, fill) >= contrast_ratio(c.text, fill),
                    "{id:?} {v:?} {role:?}"
                );
            }
            assert!(seen > 0, "{id:?} {v:?} : aucun texte dans une barre");
        }
    }
    // Impression N&B : barre critique noire, texte noir ⇒ encre du fond.
    let st = template(TemplateId::ImpressionNb);
    let c = st.tokens(Variant::Print);
    let fill = c.bars[&BarRole::Critical].fill;
    assert!(contrast_ratio(ink_on(fill, c.text, c.bg), fill) >= 4_500);
}

/// H-UI-126 (finitions P2) : texte d'une tâche mis en forme dans la table
/// (gras = Inter `SemiBold`, italique, couleur), surlignage qui prime sur la
/// couleur de phase, N° jamais mis en forme, Gantt intact ; SVG en italique.
#[test]
fn h_ui_126_scene_task_text_and_fill() {
    use plannercraft_render::style::{PhaseColor, Rgb, TaskText};
    let p = project_p();
    let s = plannercraft_core::schedule(&p);
    let mut st = template(TemplateId::Classique);
    let murs = sc_uid(&p, 3);
    let summary = sc_uid(&p, 5);
    let red = Rgb(0xC0, 0, 0);
    let yellow = Rgb(0xFF, 0xF2, 0xCC);
    st.task_text.insert(
        murs,
        TaskText {
            bold: true,
            italic: true,
            color: Some(red),
            fill: None,
        },
    );
    st.task_text.insert(
        summary,
        TaskText {
            fill: Some(yellow),
            ..TaskText::default()
        },
    );
    st.phases.insert(
        summary,
        PhaseColor {
            fill: Rgb(0xE0, 0x10, 0x10),
            children: false,
        },
    );
    let plain = layout(
        &p,
        &s,
        &template(TemplateId::Classique),
        &NoTexts,
        &Viewport::new(Variant::Light, Lang::Fr),
    )
    .0;
    let sc = layout(&p, &s, &st, &NoTexts, &Viewport::new(Variant::Light, Lang::Fr)).0;
    // Rangs : `murs` avant `summary` dans la table des UID.
    let (rank_murs, rank_summary) = if murs < summary { (0, 1) } else { (1, 0) };
    // Ligne 3 (indice 2) : texte rouge, gras, italique, sauf le N°.
    let styled: Vec<(&str, FontRef)> = sc
        .items
        .iter()
        .filter(|i| i.class == Class::TaskText(rank_murs))
        .filter_map(|i| match &i.prim {
            Prim::Text { text, font, .. } => Some((text.as_str(), *font)),
            _ => None,
        })
        .collect();
    assert_eq!(styled.len(), 1, "{styled:?}");
    assert_eq!(styled[0].0, "Murs");
    assert_eq!(styled[0].1.weight, FontWeight::SemiBold);
    assert!(styled[0].1.italic);
    assert_eq!(sc.paints[&Class::TaskText(rank_murs)].fill, Some(red));
    assert!(texts_of(&sc, Class::Text).contains(&"3"), "N° non mis en forme");
    // Ligne 5 (indice 4) : surlignage au lieu de la couleur de phase.
    assert_eq!(band_rows(&sc, Class::TaskFill(rank_summary)), [4]);
    assert_eq!(band_rows(&sc, Class::PhaseRow(0)), [] as [u32; 0]);
    assert_eq!(sc.paints[&Class::TaskFill(rank_summary)].fill, Some(yellow));
    // Gantt : mêmes primitives que sans mise en forme (barres de phase
    // comprises, la phase colore toujours la barre).
    let gantt = |sc: &Scene| -> usize { sc.items.iter().filter(|i| i.clip == Clip::Gantt).count() };
    assert!(gantt(&sc) >= gantt(&plain));
    // SVG : italique et couleur.
    let (svg, _) = svg_styled(&p, &s, &request(Theme::Light, Lang::Fr), &st, &NoTexts);
    assert!(svg.contains("font-style=\"italic\""), "{svg}");
    assert!(svg.contains(&format!(".tx{rank_murs}{{fill:#C00000")), "{svg}");
    assert!(svg.contains(&format!(".tf{rank_summary}{{")), "{svg}");
}

/// UID de la tâche d'ID `id` du projet P.
fn sc_uid(p: &Project, id: usize) -> u32 {
    p.tasks.iter().nth(id - 1).unwrap().uid().get()
}
