//! R-164 — Historique : une entrée par lot, budget, rétablissement, état
//! modifié.

use plannercraft_automation::SessionLimits;
use plannercraft_automation::error::HistoryError;
use plannercraft_core::TaskUid;
use plannercraft_tests::mspdi::new_project;
use plannercraft_tests::session::{commit, push_tasks, rename, session};

fn name(s: &plannercraft_automation::Session) -> String {
    s.project().tasks.get(TaskUid::new(1)).unwrap().name.clone()
}

#[test]
fn r164_budget_drops_oldest() {
    // Budget 2 entrées ; lots A, B, C → historique [B, C], `dropped = 1` ;
    // deux undo possibles, le troisième `history.empty`.
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1], 1);
    let mut s = session(p);
    s.set_limits(SessionLimits {
        history_entries: 2,
        ..SessionLimits::default()
    });
    for l in ["A", "B", "C"] {
        commit(&mut s, l, &[rename(1, l)]);
    }
    let h = s.history();
    let labels: Vec<_> = h.entries.iter().map(|e| e.label.clone().unwrap()).collect();
    assert_eq!(labels, vec!["B", "C"]);
    assert_eq!(h.dropped, 1);
    s.undo().unwrap();
    assert_eq!(name(&s), "B");
    s.undo().unwrap();
    assert_eq!(name(&s), "A");
    assert_eq!(s.undo().unwrap_err(), HistoryError::Empty);
}

#[test]
fn r164_byte_budget_keeps_last_entry() {
    // Budget de 1 octet : chaque entrée le dépasse, la dernière est gardée
    // (au moins une annulation possible).
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1], 1);
    let mut s = session(p);
    s.set_limits(SessionLimits {
        history_bytes: 1,
        ..SessionLimits::default()
    });
    commit(&mut s, "A", &[rename(1, "A")]);
    commit(&mut s, "B", &[rename(1, "B")]);
    let h = s.history();
    assert_eq!(h.entries.len(), 1);
    assert_eq!(h.dropped, 1);
    assert!(h.entries[0].bytes > 1);
    s.undo().unwrap();
    assert_eq!(name(&s), "A");
}

#[test]
fn r164_redo_cleared_by_commit() {
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1], 1);
    let mut s = session(p);
    commit(&mut s, "A", &[rename(1, "A")]);
    let g = s.generation();
    let o = s.undo().unwrap();
    assert!(o.committed);
    assert_eq!(o.history_len, 0);
    assert_eq!(s.generation(), g + 1);
    assert_eq!(name(&s), "T1");
    assert_eq!(s.history().redo.len(), 1);
    s.redo().unwrap();
    assert_eq!(name(&s), "A");
    s.undo().unwrap();
    commit(&mut s, "B", &[rename(1, "B")]);
    assert_eq!(s.history().redo.len(), 0);
    assert_eq!(s.redo().unwrap_err(), HistoryError::Empty);
    assert_eq!(s.redo().unwrap_err().code(), "history.empty");
}

#[test]
fn r164_dirty_tracking() {
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1], 1);
    let mut s = session(p);
    assert!(!s.is_dirty());
    commit(&mut s, "A", &[rename(1, "A")]);
    assert!(s.is_dirty());
    s.undo().unwrap();
    assert!(!s.is_dirty());
    s.redo().unwrap();
    assert!(s.is_dirty());
    s.mark_saved();
    assert!(!s.is_dirty());
    // Annuler puis un autre lot : même profondeur, autre état → modifié.
    s.undo().unwrap();
    assert!(s.is_dirty());
    commit(&mut s, "B", &[rename(1, "B")]);
    assert!(s.is_dirty());
    s.undo().unwrap();
    assert!(s.is_dirty());
}

#[test]
fn r164_dirty_when_saved_state_dropped() {
    // Budget 1 entrée : l'état d'ouverture (enregistré) est abandonné, le
    // document reste modifié même après toutes les annulations possibles.
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1], 1);
    let mut s = session(p);
    s.set_limits(SessionLimits {
        history_entries: 1,
        ..SessionLimits::default()
    });
    commit(&mut s, "A", &[rename(1, "A")]);
    commit(&mut s, "B", &[rename(1, "B")]);
    s.undo().unwrap();
    assert_eq!(name(&s), "A");
    assert!(s.is_dirty());
}
