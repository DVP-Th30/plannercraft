//! « Garder les dates de MS Project » (R-299, lot M3, ADR-0038) : le lot
//! de `helpers::keep_dates` passe les tâches en planification manuelle
//! avec le début et la fin donnés, en une entrée d'annulation.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

use plannercraft_automation::helpers::{self, HelperError};
use plannercraft_automation::{AnchorSpec, ApplyMode, Batch, NewProject, Session};
use plannercraft_core::{Moment, SchedulingMode, TaskUid};
use serde_json::json;

fn at(s: &str) -> Moment {
    s.parse().unwrap()
}

/// S { A 1 j ; B 2 j FD de A } ; C 1 j ; depuis le lun 19/10/2026 08:00.
fn session() -> Session {
    let init = NewProject {
        name: "P".to_owned(),
        anchor: AnchorSpec::Start(at("2026-10-19T08:00")),
        lang: None,
    };
    let mut s = Session::new_project(init, at("2026-10-19T08:00")).unwrap();
    let batch = Batch::from_json(json!({
        "format_version": 1,
        "ops": [
            { "op": "task.add", "name": "S" },
            { "op": "task.add", "name": "A", "as": "$a", "duration": { "text": "1j" } },
            { "op": "task.add", "name": "B", "as": "$b", "duration": { "text": "2j" } },
            { "op": "task.add", "name": "C", "duration": { "text": "1j" } },
            { "op": "task.indent", "uid": "$a" },
            { "op": "task.indent", "uid": "$b" },
            { "op": "link.add", "pred": "$a", "succ": "$b" }
        ]
    }))
    .unwrap();
    s.apply(&batch, ApplyMode::Commit).unwrap();
    s
}

#[test]
fn r299_keep_dates_one_batch_manual_with_both_dates() {
    let mut s = session();
    let (sum, a, b, c) = (TaskUid::new(1), TaskUid::new(2), TaskUid::new(3), TaskUid::new(4));
    assert_eq!(s.schedule().scheduled(a).unwrap().start, at("2026-10-19T08:00"));
    let before = s.project().clone();
    let generation = s.generation();
    let dates = [
        // Ordre quelconque, doublon : fusionnés, ordre des ID.
        (b, at("2026-10-22T08:00"), at("2026-10-26T17:00")),
        (a, at("2026-10-21T08:00"), at("2026-10-21T17:00")),
        (a, at("2026-10-21T08:00"), at("2026-10-21T17:00")),
        // Récapitulative : ignorée (ses dates suivent ses sous-tâches).
        (sum, at("2026-10-21T08:00"), at("2026-10-26T17:00")),
        // Fin avant le début : ignorée.
        (c, at("2026-10-20T17:00"), at("2026-10-20T08:00")),
    ];
    let batch = helpers::keep_dates(&s, &dates).unwrap().unwrap();
    assert_eq!(batch.ops.len(), 4);
    s.apply(&batch, ApplyMode::Commit).unwrap();
    assert_eq!(s.generation(), generation + 1);
    let sched = s.schedule();
    let span = |uid| {
        let t = sched.scheduled(uid).unwrap();
        (t.start, t.finish)
    };
    assert_eq!(span(a), (at("2026-10-21T08:00"), at("2026-10-21T17:00")));
    // Trois jours ouvrés (jeu, ven, lun) : durée mesurée, dates exactes.
    assert_eq!(span(b), (at("2026-10-22T08:00"), at("2026-10-26T17:00")));
    assert_eq!(span(sum), (at("2026-10-21T08:00"), at("2026-10-26T17:00")));
    assert_eq!(span(c), (at("2026-10-19T08:00"), at("2026-10-19T17:00")));
    let mode = |uid| s.project().tasks.get(uid).unwrap().mode;
    assert!(matches!(
        mode(b),
        SchedulingMode::Manual { start: Some(st), finish: Some(fi), .. }
            if st == at("2026-10-22T08:00") && fi == at("2026-10-26T17:00")
    ));
    assert_eq!(mode(sum), SchedulingMode::Auto);
    assert_eq!(mode(c), SchedulingMode::Auto);
    // Déjà gardées à ces dates : rien à faire.
    assert_eq!(helpers::keep_dates(&s, &dates[..2]).unwrap(), None);
    // Une seule entrée d'annulation.
    s.undo().unwrap();
    assert_eq!(s.project(), &before);
    // UID inconnu : refus, aucun lot.
    assert_eq!(
        helpers::keep_dates(
            &s,
            &[(TaskUid::new(99), at("2026-10-21T08:00"), at("2026-10-21T17:00"))]
        ),
        Err(HelperError::UnknownTask(TaskUid::new(99)))
    );
    assert_eq!(helpers::keep_dates(&s, &[]).unwrap(), None);
}
