//! R-162 — Valeurs relues du fichier jetées au premier commit.

use plannercraft_automation::text::Language;
use plannercraft_automation::{DocOrigin, Session, SideEffect};
use plannercraft_core::{Percent, TaskUid, schedule};
use plannercraft_mspdi::write::{Computed, WriteOptions, choose_computed, write};
use plannercraft_tests::moment;
use plannercraft_tests::mspdi::new_project;
use plannercraft_tests::session::{commit, push_tasks, rename};

/// Document MSPDI d'un projet avec avancement (manque `progress` de R-122
/// jusqu'en J5 ; depuis J6, R-215, le moteur le calcule).
fn document_with_progress() -> Vec<u8> {
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1, 2], 2);
    p.tasks
        .get_mut(TaskUid::new(1))
        .unwrap()
        .progress
        .percent_complete = Percent::new(5_000).unwrap();
    let s = schedule(&p);
    write(&p, Computed::Schedule(&s), &WriteOptions::default()).xml
}

fn open(bytes: &[u8]) -> Session {
    let origin = DocOrigin {
        name: "p.xml".to_owned(),
        path: None,
    };
    Session::open_bytes(bytes, origin, moment("2026-10-17T08:00"), Language::Fr)
        .unwrap()
        .0
}

#[test]
fn r162_unmodified_save_uses_engine_values() {
    // Amendement J6 (R-215, R-122) : plus aucun manque, le planning de
    // PlannerCraft est écrit dès l'ouverture, valeurs relues ou non.
    let bytes = document_with_progress();
    let s = open(&bytes);
    let expected = write(
        s.project(),
        Computed::Schedule(s.schedule()),
        &WriteOptions::default(),
    );
    let out = s.save_bytes(&WriteOptions::default());
    assert_eq!(out.xml, expected.xml);
    assert_eq!(out.diagnostics.len(), 0, "{:?}", out.diagnostics);
}

#[test]
fn r162_first_commit_discards_recorded() {
    // Un `rename` puis undo : les valeurs relues ne reviennent pas ;
    // l'écriture suit `choose_computed` (valeurs du moteur, J6).
    let bytes = document_with_progress();
    let mut s = open(&bytes);
    let o = commit(&mut s, "r", &[rename(2, "B")]);
    assert_eq!(o.side_effects, vec![SideEffect::RecordedDiscarded]);
    // Le second commit ne le signale plus.
    let o = commit(&mut s, "r2", &[rename(2, "C")]);
    assert_eq!(o.side_effects.len(), 0);
    s.undo().unwrap();
    s.undo().unwrap();
    let (computed, diags) = choose_computed(s.project(), s.schedule(), None);
    assert!(matches!(computed, Computed::Schedule(_)));
    assert_eq!(diags, Vec::new());
    let expected = write(s.project(), computed, &WriteOptions::default());
    let out = s.save_bytes(&WriteOptions::default());
    assert_eq!(out.xml, expected.xml);
}
