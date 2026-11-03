//! R-242 — Glisser l'avancement : point = début planifié + max(n, 0)
//! jours calendaires ; travail = temps ouvré [début, min(point, fin)] sur
//! l'échelle de mesure ; % = ⌊travail × 100 / D⌋ × 100 (entier, `i128`),
//! borné [0, 10 000] ; point ≥ fin ⇒ 10 000 ; ⇒ `progress.update {
//! percent_complete }` (R-201). Feuille de durée non nulle (jalon : aucune
//! zone). Repère : Standard, début lun 19/10/2026 08:00.

use plannercraft_automation::gesture::{self, GestureError, GestureKind, NotDraggable};
use plannercraft_automation::{ApplyMode, Session};
use plannercraft_core::{CalendarSet, Percent, TaskUid};
use plannercraft_tests::resource_edit::with_tasks;
use plannercraft_tests::task_edit::get;

const T: TaskUid = TaskUid::new(1);

/// Glisse l'avancement de `days` jours ; rend le % envoyé (celui de
/// l'aperçu). Le % stocké est celui que R-201 dérive de la durée réelle.
fn drag(s: &mut Session, days: i64) -> i64 {
    let set = CalendarSet::compile(s.project()).0;
    let p = gesture::preview(s, &set, T, GestureKind::Progress, days).unwrap();
    let percent = p.percent.unwrap();
    match gesture::progress_drag(s, &set, T, days).unwrap() {
        Some(b) => {
            let json = serde_json::to_value(&b).unwrap();
            assert_eq!(json["ops"][0]["op"], "progress.update");
            assert_eq!(
                json["ops"][0]["percent_complete"]["value"],
                percent.get(),
                "aperçu"
            );
            s.apply(&b, ApplyMode::Commit).unwrap();
        }
        None => assert_eq!(get(s, 1).progress.percent_complete, percent),
    }
    percent.get()
}

#[test]
fn r242_progress_floor_percent() {
    // T 4 j (1 920) lun 19/10 – jeu 22/10 ; n = 2 ⇒ 960 ⇒ 5 000 ; n = 1 ⇒
    // 2 500 ; T 3 j, n = 1 ⇒ 480 / 1 440 ⇒ 33 % ⇒ 3 300 (jamais 3 333).
    let mut s = with_tasks(&[("T", "4j")]);
    assert_eq!(drag(&mut s, 2), 5_000);
    assert_eq!(drag(&mut s, 1), 2_500);
    // Même % : aucun lot.
    let set = CalendarSet::compile(s.project()).0;
    assert_eq!(gesture::progress_drag(&s, &set, T, 1).unwrap(), None);
    // n < 0 vaut 0 : 0 %.
    assert_eq!(drag(&mut s, -3), 0);
    let mut s = with_tasks(&[("T", "3j")]);
    assert_eq!(drag(&mut s, 1), 3_300);
    // R-201 : réel = 33 % de 1 440 = 475 min, % dérivé 3 299.
    assert_eq!(get(&s, 1).progress.percent_complete.get(), 3_299);
}

#[test]
fn r242_milestone_no_zone() {
    let s = with_tasks(&[("J", "0j"), ("S", "1j"), ("A", "1j")]);
    let set = CalendarSet::compile(s.project()).0;
    assert_eq!(
        gesture::progress_drag(&s, &set, T, 1).unwrap_err(),
        GestureError::NotDraggable(T, NotDraggable::Milestone)
    );
    let mut s = s;
    plannercraft_tests::task_edit::apply(&mut s, serde_json::json!([{ "op": "task.indent", "uid": 3 }]));
    let set = CalendarSet::compile(s.project()).0;
    assert_eq!(
        gesture::progress_drag(&s, &set, TaskUid::new(2), 1).unwrap_err(),
        GestureError::NotDraggable(TaskUid::new(2), NotDraggable::Summary)
    );
}

#[test]
fn r242_beyond_finish_complete() {
    // Point ≥ fin ⇒ 100 % ; une tâche commencée reste glissable.
    let mut s = with_tasks(&[("T", "2j")]);
    assert_eq!(drag(&mut s, 1), 5_000);
    assert_eq!(drag(&mut s, 30), Percent::FULL.get());
    assert!(s.schedule().scheduled(T).unwrap().progress.is_some());
}

/// Relecture finale J7 : T 3 j, glisser d'un jour envoie 3 300 et R-201
/// stocke 3 299 ; refaire le même geste ne change rien : aucun lot, aucune
/// entrée d'annulation.
#[test]
fn r242_same_point_twice_no_batch() {
    let mut s = with_tasks(&[("T", "3j")]);
    assert_eq!(drag(&mut s, 1), 3_300);
    assert_eq!(get(&s, 1).progress.percent_complete.get(), 3_299);
    let entries = s.history().entries.len();
    let set = CalendarSet::compile(s.project()).0;
    assert_eq!(gesture::progress_drag(&s, &set, T, 1).unwrap(), None);
    assert_eq!(s.history().entries.len(), entries);
    // Un autre point fait toujours un lot.
    assert!(gesture::progress_drag(&s, &set, T, 2).unwrap().is_some());
}
