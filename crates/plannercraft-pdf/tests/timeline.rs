//! Chronologie imprimée dans le PDF (ADR-0017, lot L1) : H-UI-82 (frise en
//! tête de la première page seulement), P57 avec frise (mêmes octets,
//! textes extractibles).

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

use common::{add, at, link, page_texts, project, scene_of, scene_texts};
use plannercraft_core::{Project, TaskUid};
use plannercraft_pdf::{FontSet, PdfMeta, pdf};
use plannercraft_render::Lang;
use plannercraft_render::geometry::{NoTexts, Prim};
use plannercraft_render::paginate::{Pages, PaginateOptions, TimelineSource, paginate_timeline};
use plannercraft_render::style::{TemplateId, Variant, template};
use plannercraft_render::timeline::{TimelineInput, TimelineItem, TimelineKind};
use proptest::prelude::*;

fn meta() -> PdfMeta {
    PdfMeta {
        title: "P".to_owned(),
        created: at("2026-10-26T09:30"),
        lang: Lang::Fr,
    }
}

/// Chaîne de `n` tâches de deux jours.
fn chain(n: usize) -> Project {
    let mut p = project("Chaîne", "2026-10-19T08:00");
    let mut prev: Option<TaskUid> = None;
    for i in 0..n {
        let u = add(&mut p, &format!("Tâche {i}"), 2, 1);
        if let Some(pr) = prev {
            link(&mut p, pr, u);
        }
        prev = Some(u);
    }
    p
}

/// Frise de `p` (dates du moteur), tâches `uids`, textes du repère.
fn input_of(p: &Project, uids: &[u32]) -> TimelineInput {
    let s = plannercraft_core::schedule(p);
    let items = uids
        .iter()
        .filter_map(|&u| {
            let uid = TaskUid::new(u);
            let Some(plannercraft_core::TaskOutcome::Scheduled(ts)) = s.tasks.get(&uid) else {
                return None;
            };
            Some(TimelineItem {
                uid,
                id: u,
                kind: if ts.start == ts.finish {
                    TimelineKind::Milestone
                } else {
                    TimelineKind::Bar
                },
                start: ts.start,
                finish: ts.finish,
                name: p.tasks.get(uid)?.name.clone(),
                date_label: "06/11".to_owned(),
                inactive: false,
            })
        })
        .collect();
    TimelineInput {
        start: s.project_start,
        finish: s.project_finish,
        status: Some(at("2026-11-06T17:00")),
        start_label: "Début lun 19/10/26".to_owned(),
        finish_label: "Fin ven 11/12/26".to_owned(),
        status_label: Some("État 06/11".to_owned()),
        ticks: vec![
            (at("2026-11-01T00:00"), "nov.".to_owned()),
            (at("2026-12-01T00:00"), "déc.".to_owned()),
        ],
        items,
    }
}

/// Pages de `p` dans le modèle `id`, frise demandée par la mise en page.
fn pages_with_timeline(p: &Project, id: TemplateId, input: &TimelineInput) -> Pages {
    let mut st = template(id);
    st.page.timeline = true;
    let scene = scene_of(p, id);
    let src = TimelineSource {
        input,
        style: &st,
        variant: Variant::Print,
    };
    paginate_timeline(&scene, &st.page, &NoTexts, PaginateOptions::default(), Some(src)).unwrap()
}

/// Textes de la frise disposée.
fn timeline_texts(pages: &Pages) -> Vec<String> {
    pages
        .timeline
        .as_ref()
        .unwrap()
        .items
        .iter()
        .filter_map(|it| match &it.prim {
            Prim::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// H-UI-82 : frise sur la page 1 (« Début », « Fin », noms des tâches de la
/// frise extraits du PDF), absente de la page 2.
#[test]
fn h_ui_82_pdf_first_page_only() {
    let p = chain(120);
    let input = input_of(&p, &[3, 10, 20]);
    let pages = pages_with_timeline(&p, TemplateId::Sobre, &input);
    assert!(pages.pages.len() >= 2, "{} pages", pages.pages.len());
    let (bytes, report) = pdf(&pages, &FontSet::inter(), &meta()).unwrap();
    assert_eq!(report.missing_total, 0);
    let texts = page_texts(&bytes);
    let first: BTreeSet<&str> = texts[0].iter().map(String::as_str).collect();
    let second: BTreeSet<&str> = texts[1].iter().map(String::as_str).collect();
    // « nov. » cède la place à l'étiquette de la date d'état (06/11),
    // « déc. » est dessinée.
    assert!(first.contains("déc."));
    assert!(!first.contains("nov."));
    for t in ["Début lun 19/10/26", "Fin ven 11/12/26", "État 06/11"] {
        assert!(first.contains(t), "« {t} » absent de la page 1 : {first:?}");
        assert!(!second.contains(t), "« {t} » sur la page 2");
    }
    for t in timeline_texts(&pages) {
        assert!(first.contains(t.as_str()), "« {t} » absent de la page 1");
    }
    // Sans la case de la mise en page : aucune frise.
    let st = template(TemplateId::Sobre);
    let scene = scene_of(&p, TemplateId::Sobre);
    let src = TimelineSource {
        input: &input,
        style: &st,
        variant: Variant::Print,
    };
    let plain = paginate_timeline(&scene, &st.page, &NoTexts, PaginateOptions::default(), Some(src)).unwrap();
    assert!(plain.timeline.is_none());
    let (bytes, _) = pdf(&plain, &FontSet::inter(), &meta()).unwrap();
    let all: BTreeSet<String> = page_texts(&bytes).into_iter().flatten().collect();
    assert!(!all.contains("Début lun 19/10/26"));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// P57 avec frise : deux appels ⇒ mêmes octets ; aucun texte de la
    /// scène ni de la frise absent du PDF.
    #[test]
    fn p57_with_timeline(
        n in 1_usize..80,
        picks in proptest::collection::vec(1_u32..80, 0..12),
        id in 0_usize..6,
    ) {
        let p = chain(n);
        let input = input_of(&p, &picks);
        let pages = pages_with_timeline(&p, TemplateId::ALL[id], &input);
        let (a, ra) = pdf(&pages, &FontSet::inter(), &meta()).unwrap();
        let (b, _) = pdf(&pages, &FontSet::inter(), &meta()).unwrap();
        prop_assert_eq!(&a, &b);
        prop_assert_eq!(ra.missing_total, 0);
        let found: BTreeSet<String> = page_texts(&a).into_iter().flatten().collect();
        for t in scene_texts(&pages.scene).into_iter().chain(timeline_texts(&pages)) {
            prop_assert!(found.contains(&t), "« {} » absent", t);
        }
    }
}
