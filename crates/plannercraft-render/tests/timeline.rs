//! Chronologie (ADR-0017, lot L1) : géométrie H-R16, disposition H-UI-80,
//! propriété P58, frise imprimée en tête de la première page (H-UI-82,
//! P54) et du SVG.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

mod common;

use common::{add, at, project, project_p, uid, xml_texts};
use plannercraft_core::{Moment, Project, TaskOutcome, TaskUid};
use plannercraft_render::geometry::{BarTextSource, NoTexts, Prim, Token, Viewport, layout as scene_layout};
use plannercraft_render::paginate::{
    MAX_PAGES, PageError, PaginateOptions, TIMELINE_GAP16, TimelineSource, fit_zoom, paginate_timeline,
};
use plannercraft_render::style::{BarField, Fit, Paper, TemplateId, Variant, template};
use plannercraft_render::timeline::{
    AXIS_GAP16, AXIS_Y16, BOTTOM_BASE16, PAD16, TIMELINE_FONT, TIMELINE_FONT_STRONG, TIMELINE_HEIGHT16,
    TOP_BASE16, TimelineInput, TimelineItem, TimelineKind, TimelineLayout, TimelineShape, TimelineText,
    layout,
};
use plannercraft_render::{
    GanttRequest, InterMetrics, Lang, RangeSpec, RenderLimits, RowFilter, TextMeasure, Theme, Zoom,
    svg_styled, svg_styled_timeline,
};
use proptest::prelude::*;

/// Étiquettes du repère P (formes de `plannercraft_automation::timeline`).
const START_LABEL: &str = "Début lun 19/10/26";
const FINISH_LABEL: &str = "Fin mar 17/11/26";

/// Données de la frise de P avec les tâches `uids` (dates du moteur).
fn p_input(uids: &[u32]) -> TimelineInput {
    let p = project_p();
    let s = plannercraft_core::schedule(&p);
    let items = uids
        .iter()
        .map(|&u| {
            let Some(TaskOutcome::Scheduled(ts)) = s.tasks.get(&uid(u)) else {
                panic!("tâche {u} non planifiée");
            };
            let task = p.tasks.get(uid(u)).unwrap();
            TimelineItem {
                uid: uid(u),
                id: u,
                kind: if task.duration.is_zero() {
                    TimelineKind::Milestone
                } else {
                    TimelineKind::Bar
                },
                start: ts.start,
                finish: ts.finish,
                name: task.name.clone(),
                date_label: "06/11".to_owned(),
                inactive: false,
            }
        })
        .collect();
    TimelineInput {
        start: s.project_start,
        finish: s.project_finish,
        status: None,
        start_label: START_LABEL.to_owned(),
        finish_label: FINISH_LABEL.to_owned(),
        status_label: None,
        ticks: vec![(at("2026-11-01T00:00"), "nov.".to_owned())],
        items,
    }
}

/// Entrée minimale : projet de `start` à `finish`, sans texte.
fn bare(start: &str, finish: &str, items: Vec<TimelineItem>) -> TimelineInput {
    TimelineInput {
        start: at(start),
        finish: at(finish),
        status: None,
        start_label: String::new(),
        finish_label: String::new(),
        status_label: None,
        ticks: Vec::new(),
        items,
    }
}

fn bar(u: u32, start: &str, finish: &str, name: &str) -> TimelineItem {
    TimelineItem {
        uid: uid(u),
        id: u,
        kind: TimelineKind::Bar,
        start: at(start),
        finish: at(finish),
        name: name.to_owned(),
        date_label: String::new(),
        inactive: false,
    }
}

fn milestone(u: u32, when: &str, name: &str, date: &str) -> TimelineItem {
    TimelineItem {
        kind: TimelineKind::Milestone,
        date_label: date.to_owned(),
        ..bar(u, when, when, name)
    }
}

/// Marge d'une étiquette d'extrémité (H-R16).
fn margin(label: &str) -> i64 {
    PAD16 + AXIS_GAP16 + InterMetrics.width16(label, TIMELINE_FONT_STRONG)
}

fn axis(l: &TimelineLayout) -> (i64, i64) {
    l.shapes
        .iter()
        .find_map(|s| match s {
            TimelineShape::Axis { x16, .. } => Some(*x16),
            _ => None,
        })
        .unwrap()
}

fn bar_of(l: &TimelineLayout, u: u32) -> Option<(u8, [i64; 4])> {
    l.shapes.iter().find_map(|s| match s {
        TimelineShape::Bar {
            uid: id, row, rect16, ..
        } if *id == uid(u) => Some((*row, *rect16)),
        _ => None,
    })
}

/// Textes : (x, ligne de base, texte, rôle).
fn texts(l: &TimelineLayout) -> Vec<(i64, i64, String, TimelineText)> {
    l.shapes
        .iter()
        .filter_map(|s| match s {
            TimelineShape::Text {
                x16, y16, text, role, ..
            } => Some((*x16, *y16, text.clone(), *role)),
            _ => None,
        })
        .collect()
}

/// H-R16 : début ⇒ marge gauche, fin ⇒ bord droit moins marge ; une barre
/// du début à la fin couvre l'axe.
#[test]
fn h_r16_axis_maps_bounds() {
    let mut input = bare(
        "2026-10-19T08:00",
        "2026-11-17T17:00",
        vec![bar(1, "2026-10-19T08:00", "2026-11-17T17:00", "Tout")],
    );
    input.start_label = START_LABEL.to_owned();
    input.finish_label = FINISH_LABEL.to_owned();
    let w = 1_200 * 16;
    let l = layout(&input, w);
    let (x0, x1) = axis(&l);
    assert_eq!(x0, margin(START_LABEL));
    assert_eq!(x1, w - margin(FINISH_LABEL));
    let (row, r) = bar_of(&l, 1).unwrap();
    assert_eq!((row, r[0], r[0] + r[2]), (0, x0, x1));
    // Étiquettes d'extrémité aux bords, sur la ligne de l'axe.
    let t = texts(&l);
    assert!(t.contains(&(PAD16, 65 * 16, START_LABEL.to_owned(), TimelineText::Bound)));
    let right = w - PAD16 - InterMetrics.width16(FINISH_LABEL, TIMELINE_FONT_STRONG);
    assert!(t.contains(&(right, 65 * 16, FINISH_LABEL.to_owned(), TimelineText::Bound)));
    assert_eq!(l.height16, TIMELINE_HEIGHT16);
}

/// Exemple de H-R16 : P sur 1 000 px utiles (16 000 seizièmes, 42 300
/// minutes de projet), « Murs » du 26/10 08:00 (10 080 min) au 06/11 17:00
/// (26 460 min) ⇒ barre de 3 812 à 10 008 seizièmes de l'origine de l'axe
/// (238,25 à 625,5 px), rangée 1 ; marges : « Début lun 19/10/26 » et
/// « Fin mar 17/11/26 » en Inter `SemiBold` 10 px plus deux fois 4 px.
#[test]
fn h_r16_example_p_wall_bar() {
    let input = p_input(&[2, 3, 4, 6]);
    let (m0, m1) = (margin(START_LABEL), margin(FINISH_LABEL));
    assert_eq!((m0, m1), (1_630, 1_416));
    let l = layout(&input, 16_000 + m0 + m1);
    let (x0, x1) = axis(&l);
    assert_eq!((x0, x1 - x0), (m0, 16_000));
    let (row, r) = bar_of(&l, 3).unwrap();
    assert_eq!(row, 0);
    assert_eq!((r[0] - x0, r[0] + r[2] - x0), (3_812, 10_008));
    // H-UI-80 : 2, 3 et 6 sur la première rangée, 4 en losange sur l'axe.
    assert_eq!(bar_of(&l, 2).unwrap().0, 0);
    assert_eq!(bar_of(&l, 6).unwrap().0, 0);
    let diamond = l.shapes.iter().find_map(|s| match s {
        TimelineShape::Diamond { uid: u, center16, .. } if *u == uid(4) => Some(*center16),
        _ => None,
    });
    assert_eq!(diamond, Some((x0 + 10_008, AXIS_Y16)));
    assert_eq!(l.hidden, 0);
    // Graduation de novembre.
    let nov = x0 + (13 * 1_440 - 480) * 16_000 / 42_300;
    assert!(l.shapes.contains(&TimelineShape::Tick { x16: nov }));
    assert!(texts(&l).iter().any(|t| t.2 == "nov." && t.0 == nov + 32));
}

/// H-R16 : projet de durée nulle ⇒ axe d'un jour ; barre de durée nulle
/// large d'un pixel ; largeurs dégénérées sans panique.
#[test]
fn h_r16_zero_length_project() {
    let input = bare(
        "2026-10-19T08:00",
        "2026-10-19T08:00",
        vec![
            bar(1, "2026-10-19T08:00", "2026-10-19T08:00", "Rien"),
            milestone(2, "2026-10-19T20:00", "Mi-jour", "19/10"),
        ],
    );
    let l = layout(&input, 1_000 * 16);
    let (x0, x1) = axis(&l);
    let (_, r) = bar_of(&l, 1).unwrap();
    assert_eq!((r[0], r[2]), (x0, 16));
    // 12 h sur un jour : milieu de l'axe.
    let c = l.shapes.iter().find_map(|s| match s {
        TimelineShape::Diamond { center16, .. } => Some(center16.0),
        _ => None,
    });
    assert_eq!(c, Some(x0 + (x1 - x0) / 2));
    for w in [0, 1, 15, 16, 100, 160] {
        let l = layout(&input, w);
        assert_eq!(l.height16, TIMELINE_HEIGHT16);
        assert!(check_props(&input, &l, w).is_ok(), "{w}");
    }
}

/// H-UI-80 : premier ajustement par (début, ID), 4 px au moins entre deux
/// barres d'une rangée.
#[test]
fn h_ui_80_rows_first_fit() {
    let input = bare(
        "2026-10-01T00:00",
        "2026-10-31T00:00",
        vec![
            bar(10, "2026-10-05T00:00", "2026-10-15T00:00", "B"),
            bar(11, "2026-10-01T00:00", "2026-10-10T00:00", "A"),
            bar(12, "2026-10-05T00:00", "2026-10-09T00:00", "C"),
            // Commence au même instant que A se termine : moins de 4 px,
            // rangée suivante libre.
            bar(13, "2026-10-10T00:00", "2026-10-20T00:00", "D"),
            bar(14, "2026-10-21T00:00", "2026-10-25T00:00", "E"),
        ],
    );
    let l = layout(&input, 3_000 * 16);
    let rows: Vec<u8> = [11, 10, 12, 13, 14]
        .iter()
        .map(|&u| bar_of(&l, u).unwrap().0)
        .collect();
    // A (1/10) rangée 1 ; B et C (5/10, B avant C par l'ID) rangées 2 et 3 ;
    // D (10/10) : A finit à son début (écart nul), B occupe la rangée 2
    // jusqu'au 15/10, C finit le 9/10 ⇒ rangée 3 ; E (21/10) : rangée 1.
    assert_eq!(rows, [0, 1, 2, 2, 0]);
    // Le 10/10 pile tombe à moins de 4 px de la fin de A : pas en rangée 1.
    assert_ne!(bar_of(&l, 13).unwrap().0, 0);
    assert_eq!(l.hidden, 0);
}

/// H-UI-80 : au-delà de trois rangées, le reste est compté « +n » au bord
/// droit.
#[test]
fn h_ui_80_overflow_counter() {
    let items = (1..=5)
        .map(|u| bar(u, "2026-10-05T00:00", "2026-10-15T00:00", &format!("T{u}")))
        .collect();
    let l = layout(&bare("2026-10-01T00:00", "2026-10-31T00:00", items), 800 * 16);
    assert_eq!(l.hidden, 2);
    let bars = l
        .shapes
        .iter()
        .filter(|s| matches!(s, TimelineShape::Bar { .. }))
        .count();
    assert_eq!(bars, 3);
    let over = texts(&l)
        .into_iter()
        .find(|t| t.3 == TimelineText::Overflow)
        .unwrap();
    assert_eq!(over.2, "+2");
    assert_eq!(
        over.0 + InterMetrics.width16("+2", TIMELINE_FONT),
        800 * 16 - PAD16
    );
}

/// H-UI-80 : étiquette d'un jalon au-dessus, au-dessous si elle
/// chevaucherait la précédente du dessus.
#[test]
fn h_ui_80_milestone_label_alternates() {
    let items = vec![
        milestone(1, "2026-10-06T08:00", "Réception", "06/10"),
        milestone(2, "2026-10-08T08:00", "Levée des réserves", "08/10"),
        milestone(3, "2026-10-20T08:00", "Visite", "20/10"),
        milestone(4, "2026-10-27T08:00", "Livraison", "27/10"),
    ];
    let input = bare("2026-10-01T00:00", "2026-10-31T00:00", items);
    let l = layout(&input, 1_000 * 16);
    let line_of = |name: &str| {
        texts(&l)
            .into_iter()
            .find(|t| t.3 == TimelineText::MilestoneName && t.2.starts_with(name))
            .map(|t| t.1)
    };
    assert_eq!(line_of("Réception"), Some(TOP_BASE16));
    assert_eq!(line_of("Levée"), Some(BOTTOM_BASE16));
    assert_eq!(line_of("Visite"), Some(TOP_BASE16));
    assert_eq!(line_of("Livraison"), Some(TOP_BASE16));
    // Dates courtes devant les noms, sur la même ligne.
    let date = texts(&l)
        .into_iter()
        .find(|t| t.3 == TimelineText::MilestoneDate && t.2 == "06/10")
        .unwrap();
    assert_eq!(date.1, TOP_BASE16);
    check_props(&input, &l, 1_000 * 16).unwrap();
    // Jalon tout au bord droit : étiquette ramenée à gauche, entière.
    let input = bare(
        "2026-10-01T00:00",
        "2026-10-31T00:00",
        vec![milestone(5, "2026-10-31T00:00", "Livraison", "31/10")],
    );
    let l = layout(&input, 1_000 * 16);
    assert!(texts(&l).iter().any(|t| t.2 == "Livraison" && t.1 == TOP_BASE16));
    check_props(&input, &l, 1_000 * 16).unwrap();
}

/// H-UI-80, H-R11 : un nom trop long est tronqué avec « … » dans sa barre,
/// jamais rogné ; une barre étroite porte son nom à droite s'il y a la
/// place.
#[test]
fn h_ui_80_text_truncated_not_clipped() {
    let long = "Terrassement général et fondations spéciales du bâtiment";
    let items = vec![
        bar(1, "2026-10-01T00:00", "2026-10-06T00:00", long),
        bar(2, "2026-10-20T00:00", "2026-10-20T12:00", "Court"),
    ];
    let l = layout(&bare("2026-10-01T00:00", "2026-10-31T00:00", items), 600 * 16);
    let t = texts(&l);
    let (_, r) = bar_of(&l, 1).unwrap();
    let name = t.iter().find(|t| t.2.starts_with("Terrass")).unwrap();
    assert!(name.2.ends_with('…'), "{}", name.2);
    let width = InterMetrics.width16(&name.2, TIMELINE_FONT);
    assert!(name.0 >= r[0] && name.0 + width <= r[0] + r[2], "{name:?} {r:?}");
    let (_, r2) = bar_of(&l, 2).unwrap();
    assert!(r2[2] < 24 * 16);
    let short = t.iter().find(|t| t.2 == "Court").unwrap();
    assert_eq!(short.0, r2[0] + r2[2] + 3 * 16);
    // Primitives : le nom dans la barre est peint en encre de barre.
    let prims = plannercraft_render::timeline::items(&l);
    assert!(prims.iter().any(|it| matches!(
        &it.prim,
        Prim::Text { text, .. } if text.starts_with("Terrass")
    ) && matches!(
        it.class,
        plannercraft_render::geometry::Class::TextOnBar(..)
    )));
}

/// Boîte verticale d'un texte de 10 px : de 8 px au-dessus de la ligne de
/// base à 2 px dessous.
fn text_box(x: i64, y: i64, s: &str, role: TimelineText) -> (i64, i64, i64, i64) {
    let w = InterMetrics.width16(s, role.font());
    (x, y - 8 * 16, x + w, y + 2 * 16)
}

/// Propriétés P58 d'une disposition.
#[allow(
    clippy::many_single_char_names,
    reason = "géométrie : largeur, hauteur, boîtes"
)]
fn check_props(input: &TimelineInput, l: &TimelineLayout, width16: i64) -> Result<(), String> {
    let w = width16.clamp(0, 1 << 40);
    let h = TIMELINE_HEIGHT16;
    let inside = |x0: i64, y0: i64, x1: i64, y1: i64| x0 >= 0 && y0 >= 0 && x1 <= w && y1 <= h && x0 <= x1;
    let mut bars: Vec<(u8, i64, i64)> = Vec::new();
    let mut boxes: Vec<(i64, i64, i64, i64)> = Vec::new();
    let mut drawn = std::collections::BTreeSet::new();
    for s in &l.shapes {
        match s {
            TimelineShape::Band { rect16 } => {
                if *rect16 != [0, 0, w, h] {
                    return Err(format!("bande {rect16:?}"));
                }
            }
            TimelineShape::Bar { uid, row, rect16, .. } => {
                let [x, y, bw, bh] = *rect16;
                if !inside(x, y, x + bw, y + bh) || *row > 2 {
                    return Err(format!("barre hors bande {rect16:?}"));
                }
                bars.push((*row, x, x + bw));
                drawn.insert(*uid);
            }
            TimelineShape::Diamond {
                uid,
                center16,
                half16,
                ..
            } => {
                let (cx, cy) = *center16;
                if !inside(cx - half16, cy - half16, cx + half16, cy + half16) {
                    return Err(format!("losange hors bande {center16:?}"));
                }
                drawn.insert(*uid);
            }
            TimelineShape::Axis { y16, x16 } => {
                if !inside(x16.0, *y16, x16.1, *y16) {
                    return Err(format!("axe {x16:?}"));
                }
            }
            TimelineShape::Tick { x16 } | TimelineShape::StatusLine { x16 } => {
                if !inside(*x16, 0, *x16, h) {
                    return Err(format!("trait {x16}"));
                }
            }
            TimelineShape::Text {
                x16, y16, text, role, ..
            } => {
                let b = text_box(*x16, *y16, text, *role);
                if !(b.0 >= 0 && b.2 <= w && *y16 >= 0 && *y16 <= h) {
                    return Err(format!("texte hors bande {text:?} {b:?}"));
                }
                if let Some(o) = boxes
                    .iter()
                    .find(|o| b.0 < o.2 && o.0 < b.2 && b.1 < o.3 && o.1 < b.3)
                {
                    return Err(format!("textes superposés {text:?} {b:?} {o:?}"));
                }
                boxes.push(b);
            }
        }
    }
    for (i, a) in bars.iter().enumerate() {
        for b in bars.iter().skip(i + 1) {
            if a.0 == b.0 && a.1 < b.2 && b.1 < a.2 {
                return Err(format!("barres superposées {a:?} {b:?}"));
            }
        }
    }
    let valid: std::collections::BTreeSet<TaskUid> = input.items.iter().map(|it| it.uid).collect();
    let total = u32::try_from(drawn.len()).unwrap() + l.hidden;
    if total != u32::try_from(valid.len()).unwrap() {
        return Err(format!(
            "{} dessinés + {} cachés ≠ {} valides",
            drawn.len(),
            l.hidden,
            valid.len()
        ));
    }
    Ok(())
}

/// Élément généré : instants dans une fenêtre de ±400 jours autour du
/// projet (hors plage compris), durées nulles, jalons, noms variés.
fn item_strategy() -> impl Strategy<Value = TimelineItem> {
    (
        1_u32..60,
        0_u32..400,
        -400_i64..800,
        0_i64..200,
        any::<bool>(),
        any::<bool>(),
        "[a-zA-Zéœ…中 ]{0,40}",
    )
        .prop_map(|(u, id, day, len, ms, inactive, name)| {
            let base = at("2026-01-01T00:00").minutes() + day * 1_440;
            let start = Moment::from_minutes(base.max(0)).unwrap();
            let finish = Moment::from_minutes((base + len * 720).max(0)).unwrap();
            TimelineItem {
                uid: uid(u),
                id,
                kind: if ms {
                    TimelineKind::Milestone
                } else {
                    TimelineKind::Bar
                },
                start,
                finish,
                name,
                date_label: "06/11".to_owned(),
                inactive,
            }
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// P58 : pour toute liste ≤ 200 éléments (doublons, jalons, durées
    /// nulles, hors plage) et toute largeur de 0 à 100 000 px, `layout` ne
    /// panique pas, toute forme est dans la bande, deux barres d'une rangée
    /// ne se chevauchent pas, deux textes non plus, dessinés + « +n » =
    /// valides, deux appels ⇒ même résultat.
    #[test]
    fn p58_timeline_layout_props(
        items in proptest::collection::vec(item_strategy(), 0..200),
        width_px in 0_i64..=100_000,
        span_days in 0_i64..4_000,
        status_day in -10_i64..4_010,
        labels in any::<bool>(),
    ) {
        let start = at("2026-01-01T08:00");
        let finish = Moment::from_minutes(start.minutes() + span_days * 1_440).unwrap();
        let status = Moment::from_minutes((start.minutes() + status_day * 1_440).max(0)).unwrap();
        let ticks = (1..=(span_days / 30).min(150))
            .map(|k| (Moment::from_minutes(start.minutes() + k * 30 * 1_440).unwrap(), format!("m{k}")))
            .collect();
        let input = TimelineInput {
            start,
            finish,
            status: Some(status),
            start_label: if labels { START_LABEL.to_owned() } else { String::new() },
            finish_label: if labels { FINISH_LABEL.to_owned() } else { String::new() },
            status_label: Some("État 06/11".to_owned()),
            ticks,
            items,
        };
        let w = width_px * 16;
        let a = layout(&input, w);
        let b = layout(&input, w);
        prop_assert_eq!(&a, &b);
        if let Err(e) = check_props(&input, &a, w) {
            prop_assert!(false, "{}", e);
        }
    }
}

/// Source de jetons fixe.
struct Tokens;

impl BarTextSource for Tokens {
    fn bar_text(&self, _uid: TaskUid, _f: BarField) -> Option<String> {
        None
    }

    fn token(&self, _t: Token) -> String {
        "P".to_owned()
    }
}

/// H-UI-82 : SVG avec frise en tête (textes d'extrémité présents, scène
/// décalée) ; sans frise : octets identiques à `svg_styled`.
#[test]
fn h_ui_82_svg_has_timeline() {
    let p = project_p();
    let s = plannercraft_core::schedule(&p);
    let req = GanttRequest {
        range: RangeSpec::Auto,
        zoom: Zoom::Week,
        filter: RowFilter::default(),
        language: Lang::Fr,
        theme: Theme::Light,
        limits: RenderLimits::default(),
    };
    let st = template(TemplateId::Sobre);
    let (plain, _) = svg_styled(&p, &s, &req, &st, &NoTexts);
    let (same, _) = svg_styled_timeline(&p, &s, &req, &st, &NoTexts, None);
    assert_eq!(plain, same);
    let input = p_input(&[2, 3, 4, 6]);
    let (svg, report) = svg_styled_timeline(&p, &s, &req, &st, &NoTexts, Some(&input));
    assert!(!report.truncated_bytes);
    let t = xml_texts(&svg);
    assert!(t.contains(&START_LABEL.to_owned()), "{t:?}");
    assert!(t.contains(&FINISH_LABEL.to_owned()));
    assert!(t.contains(&"Murs".to_owned()));
    assert!(svg.contains("<g transform=\"translate(0 80.0000)\">"), "{svg}");
    // Déterministe.
    assert_eq!(
        svg,
        svg_styled_timeline(&p, &s, &req, &st, &NoTexts, Some(&input)).0
    );
}

/// Projet généré pour P54 (même forme que `tests/paginate.rs`).
fn generated(n: usize) -> Project {
    let mut p = project("G", "2026-10-19T08:00");
    for i in 0..n {
        add(
            &mut p,
            &format!("T{i}"),
            i64::try_from(i % 9).unwrap(),
            1 + u16::try_from(i % 3).unwrap(),
        );
    }
    p
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// P54 avec chronologie : toute ligne sur exactement une bande, aucune
    /// page vide, frise sur la première page seulement, hauteur fixe ;
    /// sans `page.timeline`, pages identiques à la pagination sans frise.
    #[test]
    fn p54_with_timeline(
        n in 0_usize..120,
        paper in 0_usize..4,
        landscape in any::<bool>(),
        fit in 0_u8..3,
        k in 1_u16..4,
        pct in 10_u16..=400,
        id in 0_usize..6,
    ) {
        let p = generated(n);
        let s = plannercraft_core::schedule(&p);
        let tid = TemplateId::ALL[id];
        let mut st = template(tid);
        st.page.paper = [Paper::A4, Paper::A3, Paper::Letter, Paper::Tabloid][paper];
        st.page.landscape = landscape;
        st.page.fit = match fit {
            0 => Fit::Scale(pct),
            1 => Fit::PagesWide(k),
            _ => Fit::PagesWideTall(k, k),
        };
        let vp = Viewport {
            zoom: fit_zoom(&st.page).unwrap_or(Zoom::Week),
            limits: RenderLimits { max_rows: 10_000, ..RenderLimits::default() },
            ..Viewport::new(Variant::Print, Lang::Fr)
        };
        let scene = scene_layout(&p, &s, &st, &Tokens, &vp).0;
        let input = p_input(&[2, 3]);
        let src = TimelineSource { input: &input, style: &st, variant: Variant::Print };
        let opts = PaginateOptions::default();
        let off = paginate_timeline(&scene, &st.page, &Tokens, opts, Some(src));
        let plain = plannercraft_render::paginate::paginate_with(&scene, &st.page, &Tokens, opts);
        prop_assert_eq!(&off, &plain);
        st.page.timeline = true;
        let src = TimelineSource { input: &input, style: &st, variant: Variant::Print };
        match paginate_timeline(&scene, &st.page, &Tokens, opts, Some(src)) {
            Err(PageError::TooManyPages { needed }) => prop_assert!(needed > MAX_PAGES),
            Err(e) => prop_assert!(false, "erreur inattendue {}", e),
            Ok(pages) => {
                prop_assert_eq!(pages.timeline16, (0, TIMELINE_HEIGHT16));
                let tl = pages.timeline.as_ref().unwrap();
                prop_assert!(tl.width16 > 0);
                prop_assert_eq!(&tl.layout, &layout(&input, tl.width16));
                let mut seen = vec![0_u32; scene.rows.len()];
                for (i, pg) in pages.pages.iter().enumerate() {
                    prop_assert_eq!(pg.timeline, i == 0);
                    prop_assert_eq!(pages.lead16(i), if i == 0 { TIMELINE_HEIGHT16 + TIMELINE_GAP16 } else { 0 });
                    if !scene.rows.is_empty() {
                        prop_assert!(!pg.rows.is_empty());
                    }
                    if pg.col == 0 {
                        for r in pg.rows.clone() {
                            seen[r] += 1;
                        }
                    }
                }
                prop_assert!(seen.iter().all(|c| *c == 1), "{:?}", seen);
            }
        }
    }
}

/// H-UI-82 : la frise prend la place de lignes sur la première page
/// seulement.
#[test]
fn h_ui_82_first_band_shorter() {
    let p = generated(200);
    let s = plannercraft_core::schedule(&p);
    let mut st = template(TemplateId::Sobre);
    st.page.fit = Fit::Scale(100);
    let vp = Viewport {
        zoom: Zoom::Week,
        limits: RenderLimits {
            max_rows: 10_000,
            ..RenderLimits::default()
        },
        ..Viewport::new(Variant::Print, Lang::Fr)
    };
    let scene = scene_layout(&p, &s, &st, &Tokens, &vp).0;
    let input = p_input(&[2, 3, 4]);
    let opts = PaginateOptions::default();
    let plain = plannercraft_render::paginate::paginate_with(&scene, &st.page, &Tokens, opts).unwrap();
    st.page.timeline = true;
    let src = TimelineSource {
        input: &input,
        style: &st,
        variant: Variant::Print,
    };
    let with = paginate_timeline(&scene, &st.page, &Tokens, opts, Some(src)).unwrap();
    let first = |pages: &plannercraft_render::paginate::Pages| pages.pages[0].rows.len();
    let row_h = scene.rows[0].h16;
    let lost = (TIMELINE_HEIGHT16 + TIMELINE_GAP16 + row_h - 1) / row_h;
    assert!(first(&with) + usize::try_from(lost).unwrap() + 1 >= first(&plain));
    assert!(first(&with) < first(&plain));
    assert!(with.pages[0].timeline);
    assert!(with.pages.iter().skip(1).all(|pg| !pg.timeline));
    // Primitives de la frise et leurs peintures dans la scène paginée.
    let tl = with.timeline.unwrap();
    assert!(
        tl.items
            .iter()
            .all(|it| with.scene.paints.contains_key(&it.class))
    );
}
