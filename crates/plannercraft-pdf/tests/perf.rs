//! Budget `pdf_5000` (lot L7 de J8, `docs/plans/j8.md` § 2.8) : réseau
//! généré de 5 000 tâches et ~10 000 liens, modèle Sobre, A3 paysage,
//! 1 page en largeur : `layout` + `paginate` + `pdf` en moins de 10 s, au
//! plus `MAX_PAGES` pages. Ignoré par défaut, à lancer en release :
//! `cargo test -p plannercraft-pdf --release --test perf -- --ignored --nocapture`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::print_stdout,
    reason = "tests : un échec doit s'arrêter net et lisiblement ; mesure publiée"
)]

mod common;

use std::time::Instant;

use common::{add, at, project};
use plannercraft_core::{Lag, Link, LinkType, TaskUid};
use plannercraft_pdf::{FontSet, PdfMeta, pdf};
use plannercraft_render::geometry::{NoTexts, Viewport, layout};
use plannercraft_render::paginate::{MAX_PAGES, PaginateOptions, fit_zoom, paginate_with};
use plannercraft_render::style::{Fit, Paper, TemplateId, Variant, template};
use plannercraft_render::{Lang, RenderLimits};

#[test]
#[ignore = "mesure de performance, à lancer en release"]
fn pdf_5000() {
    // 50 récapitulatives de 99 feuilles : chaîne FS dans chaque phase, lien
    // SS vers la feuille de même rang de la phase précédente.
    let mut p = project("Réseau 5000", "2026-10-19T08:00");
    let mut prev_phase: Vec<TaskUid> = Vec::new();
    for phase in 0..50 {
        add(&mut p, &format!("Phase {phase}"), 0, 1);
        let mut current = Vec::new();
        for k in 0..99_usize {
            let u = add(
                &mut p,
                &format!("Tâche {phase}.{k}"),
                i64::try_from(1 + k % 4).unwrap(),
                2,
            );
            let t = p.tasks.get_mut(u).unwrap();
            if let Some(&prev) = current.last() {
                t.predecessors.push(Link {
                    predecessor: prev,
                    kind: LinkType::FinishToStart,
                    lag: Lag::ZERO,
                });
            }
            if let Some(&above) = prev_phase.get(k) {
                t.predecessors.push(Link {
                    predecessor: above,
                    kind: LinkType::StartToStart,
                    lag: Lag::ZERO,
                });
            }
            current.push(u);
        }
        prev_phase = current;
    }
    assert_eq!(p.tasks.len(), 5_000);
    let s = plannercraft_core::schedule(&p);
    let mut st = template(TemplateId::Sobre);
    st.page.paper = Paper::A3;
    st.page.landscape = true;
    st.page.fit = Fit::PagesWide(1);
    let start = Instant::now();
    let vp = Viewport {
        zoom: fit_zoom(&st.page).unwrap(),
        limits: RenderLimits {
            max_rows: 20_000,
            ..RenderLimits::default()
        },
        ..Viewport::new(Variant::Print, Lang::Fr)
    };
    let (scene, _) = layout(&p, &s, &st, &NoTexts, &vp);
    let t_layout = start.elapsed();
    let pages = paginate_with(&scene, &st.page, &NoTexts, PaginateOptions::default()).unwrap();
    let t_paginate = start.elapsed();
    let meta = PdfMeta {
        title: "Réseau 5000".to_owned(),
        created: at("2026-10-26T09:00"),
        lang: Lang::Fr,
    };
    let (bytes, report) = pdf(&pages, &FontSet::inter(), &meta).unwrap();
    let total = start.elapsed();
    println!(
        "pdf_5000 : layout {:.0} ms, paginate {:.0} ms, total {:.0} ms ; {} pages, {} Kio, échelle {} %",
        t_layout.as_secs_f64() * 1e3,
        t_paginate.saturating_sub(t_layout).as_secs_f64() * 1e3,
        total.as_secs_f64() * 1e3,
        report.pages,
        bytes.len() / 1024,
        pages.scale / 100,
    );
    assert!(report.pages <= MAX_PAGES);
    assert_eq!(report.rows, 5_000);
    assert!(total.as_secs_f64() < 10.0, "budget de 10 s dépassé : {total:?}");
}
