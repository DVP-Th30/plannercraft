//! Entrées hostiles et bornes du rendu (lot L7, règle 10).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

mod common;

use common::{add, link, project, xml_texts};
use plannercraft_core::{Day, LinkType, Project};
use plannercraft_render::{GanttRequest, Lang, RangeSpec, RenderLimits, RowFilter, Theme, Zoom, svg};
use proptest::prelude::*;

fn request() -> GanttRequest {
    GanttRequest {
        range: RangeSpec::Auto,
        zoom: Zoom::Week,
        filter: RowFilter::default(),
        language: Lang::Fr,
        theme: Theme::Light,
        limits: RenderLimits::default(),
    }
}

/// Ce que doit montrer un nom : caractères interdits remplacés, tabulation
/// et fins de ligne en espace, coupe à `max` caractères + « … ».
fn visible(name: &str, max: usize) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if i >= max {
            out.push('…');
            break;
        }
        out.push(match c {
            '\t' | '\n' | '\r' => ' ',
            c if c < ' ' || c == '\u{FFFE}' || c == '\u{FFFF}' => '\u{FFFD}',
            c => c,
        });
    }
    out
}

/// Nom hostile : caractères quelconques, fragments XML, ou très long (1 Mio).
fn hostile_name() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        Just("]]>".to_owned()),
        Just("<!--".to_owned()),
        Just("</text><script>".to_owned()),
        Just("&amp;&#0;&".to_owned()),
        Just("\"'".to_owned()),
        Just("\u{0}\u{1}\u{8}\u{B}\u{C}\u{1F}\u{7F}\u{85}".to_owned()),
        Just("\u{FFFE}\u{FFFF}\u{FFFD}".to_owned()),
        Just("\t\r\n".to_owned()),
        Just("é😀\u{200B}\u{202E}".to_owned()),
        any::<String>(),
    ];
    prop_oneof![
        8 => proptest::collection::vec(piece, 0..12).prop_map(|v| v.concat()),
        1 => "[a-z<&\\]>]{1,8}".prop_map(|s| s.repeat(1 + (1 << 20) / s.len())),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn render_hostile_names_well_formed(
        names in proptest::collection::vec(hostile_name(), 1..4),
        title in hostile_name(),
        dark in any::<bool>(),
        name_chars in 0_u16..60,
    ) {
        let mut p = project(&title, "2026-11-02T08:00");
        let mut prev = None;
        for (i, n) in names.iter().enumerate() {
            let uid = add(&mut p, n, i64::try_from(i).unwrap() + 1, 1);
            if let Some(pr) = prev {
                link(&mut p, pr, uid, LinkType::FinishToStart);
            }
            prev = Some(uid);
        }
        let s = plannercraft_core::schedule(&p);
        let req = GanttRequest {
            theme: if dark { Theme::Dark } else { Theme::Light },
            limits: RenderLimits { name_chars, ..RenderLimits::default() },
            ..request()
        };
        let (out, report) = svg(&p, &s, &req);
        prop_assert_eq!(report.bytes, out.len());
        prop_assert!(out.len() < 200_000, "sortie bornée : {}", out.len());
        prop_assert!(!out.contains("]]>"));
        prop_assert!(!out.contains("<script"));
        let texts = xml_texts(&out);
        for n in names.iter().filter(|n| !n.is_empty()) {
            // Coupé à `name_chars`, puis à la largeur de la colonne Nom
            // (relecture finale de J8, F3) : un préfixe suivi de « … ».
            let want = visible(n, usize::from(name_chars));
            let shown = texts.iter().any(|t| {
                *t == want || (t.ends_with('…') && want.starts_with(t.trim_end_matches('…')))
            });
            prop_assert!(shown, "nom attendu absent : {:?}", want);
        }
    }
}

/// `n` tâches feuilles par chaînes FS de 10, durées de 1 à 5 jours.
fn many(n: usize) -> Project {
    let mut p = project("Grand", "2026-01-05T08:00");
    let mut prev = None;
    for i in 0..n {
        let uid = add(
            &mut p,
            &format!("Tâche {i}"),
            i64::try_from(i % 5).unwrap() + 1,
            1,
        );
        if i % 10 != 0
            && let Some(pr) = prev
        {
            link(&mut p, pr, uid, LinkType::FinishToStart);
        }
        prev = Some(uid);
    }
    p
}

/// Barres de feuilles (critiques ou non) d'un document.
fn bars(svg: &str) -> usize {
    svg.matches("<rect class=\"b\"").count() + svg.matches("<rect class=\"bc\"").count()
}

#[test]
fn render_5000_tasks_bounded() {
    let p = many(5_000);
    let s = plannercraft_core::schedule(&p);
    let req = request();
    let (out, report) = svg(&p, &s, &req);
    assert!(out.len() <= req.limits.max_bytes);
    assert_eq!(report.shown, 400);
    assert_eq!(report.hidden, 4_600);
    assert!(!report.truncated_bytes);
    let texts = xml_texts(&out);
    assert_eq!(
        texts.last().map(String::as_str),
        Some("… 4600 tâches non affichées")
    );
    assert_eq!(bars(&out), 400);

    // Coupe à l'octet : arrêt à une ligne entière, rapport fidèle.
    let max_bytes = 30_000;
    let req = GanttRequest {
        limits: RenderLimits {
            max_bytes,
            ..RenderLimits::default()
        },
        language: Lang::En,
        ..request()
    };
    let (out, report) = svg(&p, &s, &req);
    assert!(out.len() <= max_bytes, "{} > {max_bytes}", out.len());
    assert!(report.truncated_bytes);
    assert!(report.shown > 0 && report.shown < 400);
    assert_eq!(report.hidden, 5_000 - report.shown);
    let texts = xml_texts(&out);
    assert_eq!(
        texts.last().cloned(),
        Some(format!("… {} tasks not shown", 5_000 - report.shown))
    );
    assert_eq!(bars(&out), usize::try_from(report.shown).unwrap());

    // Borne absurde : document vide, toujours bien formé.
    let req = GanttRequest {
        limits: RenderLimits {
            max_bytes: 100,
            ..RenderLimits::default()
        },
        ..request()
    };
    let (out, report) = svg(&p, &s, &req);
    xml_texts(&out);
    assert!(out.len() <= 100);
    assert!(report.truncated_bytes);
    assert_eq!(report.shown, 0);
}

#[test]
fn render_range_bounded_by_max_days() {
    let p = many(3);
    let s = plannercraft_core::schedule(&p);
    let req = GanttRequest {
        range: RangeSpec::Span {
            from: Day::MIN,
            to: Day::MAX,
        },
        zoom: Zoom::Day,
        ..request()
    };
    let (out, report) = svg(&p, &s, &req);
    assert!(report.truncated_days);
    assert!(out.len() <= req.limits.max_bytes);
    xml_texts(&out);
    // Toutes les échelles et bornes extrêmes : jamais de panique.
    for zoom in [
        Zoom::Quarter,
        Zoom::Auto { width_px: 0 },
        Zoom::Auto { width_px: u32::MAX },
        Zoom::Fill { width_px: 0 },
        Zoom::Fill { width_px: 1 },
        Zoom::Fill { width_px: u32::MAX },
    ] {
        for limits in [
            RenderLimits {
                max_rows: 0,
                max_days: 0,
                max_bytes: 0,
                name_chars: 0,
            },
            RenderLimits {
                max_rows: u32::MAX,
                max_days: u32::MAX,
                max_bytes: usize::MAX,
                name_chars: u16::MAX,
            },
        ] {
            let req = GanttRequest {
                zoom,
                limits,
                range: RangeSpec::Span {
                    from: Day::MAX,
                    to: Day::MAX,
                },
                ..request()
            };
            let (out, report) = svg(&p, &s, &req);
            assert_eq!(report.bytes, out.len());
            xml_texts(&out);
        }
    }
}
