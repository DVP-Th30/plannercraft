//! Rendus de référence (lot L7) : comparaison octet à octet avec
//! `tests/golden/*.svg`, régénérés par `PLANNERCRAFT_BLESS=1`.

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
use std::path::PathBuf;

use common::{add, at, house, link, project, xml_texts};
use plannercraft_core::{Day, LinkType, TaskOutcome};
use plannercraft_render::{GanttRequest, Lang, RangeSpec, RenderLimits, RowFilter, Theme, Zoom, svg};

/// Requête par défaut (plage du projet, semaines, français, clair).
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

/// Compare `actual` au fichier de référence `name.svg` (ou l'écrit si
/// `PLANNERCRAFT_BLESS=1`) ; vérifie aussi qu'il est bien formé.
fn golden(name: &str, actual: &str) {
    xml_texts(actual);
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests",
        "golden",
        &format!("{name}.svg"),
    ]
    .iter()
    .collect();
    if std::env::var_os("PLANNERCRAFT_BLESS").is_some_and(|v| v == "1") {
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{} illisible ({e}) : régénérer par PLANNERCRAFT_BLESS=1",
            path.display()
        )
    });
    if expected != actual {
        let at = expected
            .bytes()
            .zip(actual.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| expected.len().min(actual.len()));
        let from = at.saturating_sub(80);
        panic!(
            "{name} diffère à l'octet {at} :\nattendu : {}\nobtenu  : {}",
            expected.get(from..(at + 80).min(expected.len())).unwrap_or(""),
            actual.get(from..(at + 80).min(actual.len())).unwrap_or("")
        );
    }
}

#[test]
fn render_golden_house_fr_light() {
    let (p, s) = house();
    let (out, report) = svg(&p, &s, &request());
    assert_eq!(report.shown, 49);
    assert_eq!(report.hidden, 0);
    assert!(!report.truncated_bytes && !report.truncated_days);
    assert_eq!(report.bytes, out.len());
    let texts = xml_texts(&out);
    assert!(texts.iter().any(|t| t == "Études et permis"));
    assert!(texts.iter().any(|t| t == "Nom de la tâche"));
    assert!(texts.iter().any(|t| t == "novembre 2026"));
    golden("house_fr_light", &out);
}

#[test]
fn render_golden_house_en_dark() {
    let (p, s) = house();
    let req = GanttRequest {
        zoom: Zoom::Month,
        language: Lang::En,
        theme: Theme::Dark,
        ..request()
    };
    let (out, report) = svg(&p, &s, &req);
    assert_eq!(report.shown, 49);
    let texts = xml_texts(&out);
    assert!(texts.iter().any(|t| t == "Task Name"));
    assert!(texts.iter().any(|t| t == "Q4 2026"));
    assert!(out.contains("#1E1F22"), "fond sombre");
    golden("house_en_dark", &out);
}

#[test]
fn render_golden_link_types() {
    let mut p = project("Liens", "2026-11-02T08:00");
    let kinds = [
        LinkType::FinishToStart,
        LinkType::StartToStart,
        LinkType::FinishToFinish,
        LinkType::StartToFinish,
    ];
    for (i, kind) in kinds.into_iter().enumerate() {
        let a = add(&mut p, &format!("A{i}"), 2, 1);
        let b = add(&mut p, &format!("B{i}"), 3, 1);
        link(&mut p, a, b, kind);
    }
    let s = plannercraft_core::schedule(&p);
    let req = GanttRequest {
        zoom: Zoom::Day,
        ..request()
    };
    let (out, report) = svg(&p, &s, &req);
    assert_eq!(report.shown, 8);
    assert_eq!(out.matches("<polyline").count(), 4, "un tracé par lien");
    golden("link_types", &out);
}

#[test]
fn render_golden_milestone_excluded_status_date() {
    let mut p = project("Jalons", "2026-11-02T08:00");
    add(&mut p, "Phase", 0, 1);
    let a = add(&mut p, "Travaux", 3, 2);
    let m = add(&mut p, "Réception", 0, 2);
    link(&mut p, a, m, LinkType::FinishToStart);
    let off = add(&mut p, "Option écartée", 2, 1);
    p.tasks.get_mut(off).unwrap().active = false;
    let blank = add(&mut p, "", 0, 1);
    p.tasks.get_mut(blank).unwrap().is_null = true;
    p.status_date = Some(at("2026-11-04T12:00"));
    let s = plannercraft_core::schedule(&p);
    assert!(matches!(s.tasks.get(&off), Some(TaskOutcome::Excluded { .. })));
    let (out, report) = svg(&p, &s, &request());
    assert_eq!(report.shown, 5);
    assert_eq!(out.matches("class=\"sd\"").count(), 1, "date d'état");
    assert!(out.contains("<polygon class=\"mc\"") || out.contains("<polygon class=\"m\""));
    assert!(out.contains("class=\"x\""), "tâche exclue en contour");
    golden("milestone_excluded_status_date", &out);
}

#[test]
fn render_same_input_same_bytes() {
    let (p, s) = house();
    for zoom in [
        Zoom::Day,
        Zoom::Week,
        Zoom::Month,
        Zoom::Quarter,
        Zoom::Auto { width_px: 900 },
    ] {
        let req = GanttRequest { zoom, ..request() };
        let (a, ra) = svg(&p, &s, &req);
        let (b, rb) = svg(&p.clone(), &plannercraft_core::schedule(&p), &req);
        assert_eq!(a, b);
        assert_eq!(ra, rb);
        xml_texts(&a);
    }
}

#[test]
fn render_critical_only_filter() {
    let (p, s) = house();
    let critical: BTreeSet<_> = s
        .tasks
        .iter()
        .filter(|(_, o)| matches!(o, TaskOutcome::Scheduled(t) if t.critical))
        .map(|(uid, _)| *uid)
        .collect();
    assert!(!critical.is_empty() && critical.len() < p.tasks.len());
    let req = GanttRequest {
        filter: RowFilter {
            critical_only: true,
            ..RowFilter::default()
        },
        ..request()
    };
    let (out, report) = svg(&p, &s, &req);
    assert_eq!(usize::try_from(report.shown).unwrap(), critical.len());
    assert_eq!(
        usize::try_from(report.hidden).unwrap(),
        p.tasks.len() - critical.len()
    );
    // Aucune barre, récapitulative ni jalon non critique ; aucune ligne de
    // troncature (le filtre n'est pas une borne).
    for class in ["\"b\"", "\"s\"", "\"m\"", "\"l\""] {
        assert!(
            !out.contains(&format!("class={class}")),
            "classe {class} présente"
        );
    }
    assert!(!out.contains("non affich"));
    let texts = xml_texts(&out);
    for task in &p.tasks {
        if !critical.contains(&task.uid()) && !task.name.is_empty() {
            assert!(!texts.contains(&task.name), "{} affichée", task.name);
        }
    }
    // Filtre par UID et par niveau.
    let first: BTreeSet<_> = p.tasks.iter().take(3).map(plannercraft_core::Task::uid).collect();
    let req = GanttRequest {
        filter: RowFilter {
            uids: Some(first),
            ..RowFilter::default()
        },
        ..request()
    };
    assert_eq!(svg(&p, &s, &req).1.shown, 3);
    let req = GanttRequest {
        filter: RowFilter {
            max_outline: Some(1),
            ..RowFilter::default()
        },
        ..request()
    };
    let top = p.tasks.iter().filter(|t| t.outline_level.get() == 1).count();
    assert_eq!(usize::try_from(svg(&p, &s, &req).1.shown).unwrap(), top);
}

#[test]
fn render_x16_integer_axis() {
    // Origine lun 02/11/2026 00:00, Semaine (192/16 px par jour) ; tâche
    // débutant mer 04/11 08:00 : 2 × 1 440 + 480 = 3 360 min →
    // x16 = round_div(3 360 × 192, 1 440) = 448 → « 28.0000 » depuis le
    // début du Gantt (280 px), soit 308.0000.
    let mut p = project("Axe", "2026-11-04T08:00");
    add(&mut p, "T", 1, 1);
    let s = plannercraft_core::schedule(&p);
    let from: Day = "2026-11-02".parse().unwrap();
    let to: Day = "2026-11-15".parse().unwrap();
    let req = GanttRequest {
        range: RangeSpec::Span { from, to },
        ..request()
    };
    let (out, _) = svg(&p, &s, &req);
    assert!(out.contains("<rect class=\"bc\" x=\"308.0000\""), "{out}");
    // Fin : mer 17:00 = 2 × 1 440 + 1 020 = 3 900 min → 520/16 = 32.5 px ;
    // largeur 72/16 = 4.5 px.
    assert!(out.contains("width=\"4.5000\""), "{out}");
    // Largeur du document : 280 + 14 jours × 12 px = 448 px.
    assert!(out.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"448.0000\""));
    // Plage inversée : remise dans l'ordre.
    let req = GanttRequest {
        range: RangeSpec::Span { from: to, to: from },
        ..request()
    };
    assert_eq!(svg(&p, &s, &req).0, out);
}
