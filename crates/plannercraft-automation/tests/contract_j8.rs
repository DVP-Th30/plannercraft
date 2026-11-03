//! Contrat J8 (lot L0, ADR-0016) : types, squelettes et registre.
//! `project.export_pdf` est exécutée depuis L7 (`tests/export_pdf.rs`), les
//! `style.*` depuis L6 ;
//! `render.gantt.style`
//! est exécuté depuis L4 ; les critères J8 de `task.list` et
//! `task.update.mode` sont exécutés depuis L2 (R-251, R-253), le
//! presse-papiers et l'aperçu CSV depuis L3 (R-244, R-256) ; les
//! squelettes répondent sans paniquer.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

use plannercraft_automation::clip::{self, ClipError, PasteOptions, RowClip};
use plannercraft_automation::error::codes;
use plannercraft_automation::filter::{FilterCtx, FilterPreset, TaskFilter};
use plannercraft_automation::helpers::{self, HelperError, PercentStep};
use plannercraft_automation::text::Language;
use plannercraft_automation::{
    AnchorSpec, ApplyMode, Batch, CallEnv, NewProject, Placement, Session, call_with,
};
use plannercraft_core::TaskUid;
use serde_json::json;

fn now() -> plannercraft_core::Moment {
    "2026-10-19T08:00".parse().unwrap()
}

fn session() -> Session {
    let init = NewProject {
        name: "P".to_owned(),
        anchor: AnchorSpec::Start(now()),
        lang: None,
    };
    let mut s = Session::new_project(init, now()).unwrap();
    let batch = Batch::from_json(json!({
        "format_version": 1,
        "ops": [
            { "op": "task.add", "name": "Fondations", "duration": { "text": "5j" } },
            { "op": "task.add", "name": "Murs", "duration": { "text": "10j" } }
        ]
    }))
    .unwrap();
    s.apply(&batch, ApplyMode::Commit).unwrap();
    s
}

#[test]
fn contract_j8_reserved_and_new_fields_refused() {
    let mut s = session();
    let before = s.project().clone();
    let generation = s.generation();
    let env = CallEnv::new(now(), Language::Fr);
    // `style.*` exécutées depuis L6 (`tests/style_j8.rs`), le PDF depuis L7
    // (`tests/export_pdf.rs`) : plage inversée refusée sans rien écrire.
    let err = call_with(
        &mut s,
        &env,
        "project.export_pdf",
        json!({ "path": "p.pdf", "range": { "from": "2026-11-02", "to": "2026-10-01" } }),
    )
    .unwrap_err();
    assert_eq!(err.code, codes::PARAM_INVALID);
    assert!(!std::path::Path::new("p.pdf").exists());
    // Paramètres mal formés : refusés comme avant.
    let err = call_with(
        &mut s,
        &env,
        "task.update",
        json!({ "uid": 1, "mode": "hybride" }),
    )
    .unwrap_err();
    assert_eq!(err.code, codes::PARAM_INVALID);
    // Lectures J7 inchangées.
    assert!(call_with(&mut s, &env, "task.list", json!({ "critical": true })).is_ok());
    assert!(call_with(&mut s, &env, "render.gantt", json!({})).is_ok());
    // Style du rendu (L4) : modèle nommé, textes formatés par la session.
    let sobre = call_with(&mut s, &env, "render.gantt", json!({ "style": "sobre" })).unwrap();
    let svg = sobre["svg"].as_str().unwrap();
    assert!(svg.contains("Inter,sans-serif"), "police d'Inter");
    // Table et texte à droite de la barre (Sobre) ; table seule (Classique).
    assert_eq!(svg.matches(">Murs</text>").count(), 2);
    let classique = call_with(&mut s, &env, "render.gantt", json!({})).unwrap();
    let classique = classique["svg"].as_str().unwrap();
    assert!(classique.contains("font-family:sans-serif"));
    assert_eq!(classique.matches(">Murs</text>").count(), 1);
    assert_eq!(s.project(), &before);
    assert_eq!(s.generation(), generation);
}

#[test]
fn contract_j8_skeletons_answer_without_panic() {
    let s = session();
    let uids = [TaskUid::new(1), TaskUid::new(2)];
    // Raccourcis (L2, R-252, R-253, R-255) : des lots ordinaires.
    for b in [
        helpers::mark_percent(&s, &uids, PercentStep::P50),
        helpers::toggle_mode(&s, &uids),
        helpers::toggle_active(&s, &uids),
    ] {
        assert_eq!(b.unwrap().map(|b| b.ops.len()), Some(2));
    }
    assert_eq!(
        helpers::toggle_active(&s, &[TaskUid::new(9)]),
        Err(HelperError::UnknownTask(TaskUid::new(9)))
    );
    assert_eq!(PercentStep::P75.hundredths(), 7_500);
    // Presse-papiers (L3, R-244, R-256).
    assert_eq!(RowClip::capture(&s, &uids).map(|c| c.tasks.len()), Ok(2));
    let empty = RowClip::default();
    assert_eq!(
        clip::paste_batch(&s, &empty, &Placement::End, &PasteOptions::default()).map(|_| ()),
        Err(ClipError::Unrecognized)
    );
    let pasted = clip::paste_text(&s, "A\nB", &Placement::End, Language::Fr).unwrap();
    assert_eq!((pasted.0.ops.len(), pasted.1.tasks), (2, 2));
    assert!(PasteOptions::default().links);

    // Filtre (L2, R-251) : « Toutes » montre tout ; critique = Murs seule.
    let ctx = FilterCtx {
        project: s.project(),
        schedule: s.schedule(),
        status: now(),
    };
    let hits = TaskFilter::from_preset(&FilterPreset::All).hits(&ctx);
    assert_eq!(hits.shown, vec![true, true]);
    let hits = TaskFilter::from_preset(&FilterPreset::Critical).hits(&ctx);
    assert_eq!(hits.shown, vec![false, true]);
    assert_eq!(TaskFilter::from_preset(&FilterPreset::Late).late, Some(true));
    assert!(TaskFilter::from_preset(&FilterPreset::All).show_related_summaries);
    assert_eq!(plannercraft_automation::filter::fold_text("Écrêté"), "ecrete");

    let opts = plannercraft_automation::csv::CsvImportOptions::new(
        plannercraft_core::ScheduleAnchor::FromStart(now()),
        now(),
    );
    // Aperçu CSV (L3).
    let p = plannercraft_automation::csv::preview(b"Nom;Duree\nA;1j\n", &opts).unwrap();
    assert_eq!(p.rows, vec![vec!["A".to_owned(), "1j".to_owned()]]);
}
