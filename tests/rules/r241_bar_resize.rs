//! R-241 — Redimensionner une barre par sa fin : cible = fin planifiée + n
//! jours calendaires ; durée = temps ouvré [début, cible] sur l'échelle de
//! mesure de la tâche (R-113), nulle si cible ≤ début ; durée écoulée :
//! minutes civiles ; ⇒ `task.update { duration: { minutes, unit } }`
//! (unité courante gardée). Feuille non jalon, non commencée. Repère :
//! Standard, début lun 19/10/2026 08:00.

use plannercraft_automation::gesture::{self, GestureError, GestureKind, NotDraggable};
use plannercraft_automation::{ApplyMode, Session};
use plannercraft_core::{CalendarSet, DurationUnit, Minutes, TaskUid, TimeBasis};
use plannercraft_tests::moment;
use plannercraft_tests::resource_edit::with_tasks;
use plannercraft_tests::task_edit::get;

const T: TaskUid = TaskUid::new(1);

fn resize(s: &mut Session, days: i64) {
    let set = CalendarSet::compile(s.project()).0;
    let p = gesture::preview(s, &set, T, GestureKind::Resize, days).unwrap();
    let b = gesture::bar_resize(s, &set, T, days).unwrap().unwrap();
    s.apply(&b, ApplyMode::Commit).unwrap();
    let t = s.schedule().scheduled(T).unwrap();
    assert_eq!((p.start, p.finish), (t.start, t.finish), "aperçu");
    assert_eq!(p.duration, Some(get(s, 1).duration.minutes()));
}

#[test]
fn r241_resize_working_time() {
    // T 2 j lun 08:00 – mar 17:00 ; n = 3 ⇒ cible ven 23/10 17:00 ⇒ 2 400
    // min « 5 jours » ; n = −1 (depuis 2 j) ⇒ 480 « 1 jour ».
    let mut s = with_tasks(&[("T", "2j")]);
    resize(&mut s, 3);
    let d = get(&s, 1).duration;
    assert_eq!((d.minutes(), d.unit()), (Minutes::new(2_400), DurationUnit::Day));
    assert_eq!(
        s.schedule().scheduled(T).unwrap().finish,
        moment("2026-10-23T17:00")
    );
    let mut s = with_tasks(&[("T", "2j")]);
    resize(&mut s, -1);
    assert_eq!(get(&s, 1).duration.minutes(), Minutes::new(480));
    let set = CalendarSet::compile(s.project()).0;
    assert_eq!(gesture::bar_resize(&s, &set, T, 0).unwrap(), None);
    // Unité courante gardée : 16 h ⇒ heures.
    let mut s = with_tasks(&[("T", "16h")]);
    resize(&mut s, 1);
    let d = get(&s, 1).duration;
    assert_eq!((d.minutes(), d.unit()), (Minutes::new(1_440), DurationUnit::Hour));
}

#[test]
fn r241_resize_to_zero_milestone() {
    // n = −2 ⇒ cible dim 18/10 17:00 ≤ début ⇒ 0, jalon (R-157).
    let mut s = with_tasks(&[("T", "2j")]);
    resize(&mut s, -2);
    assert_eq!(get(&s, 1).duration.minutes(), Minutes::ZERO);
    assert!(get(&s, 1).milestone);
    // Un jalon ne se redimensionne pas.
    let set = CalendarSet::compile(s.project()).0;
    assert_eq!(
        gesture::bar_resize(&s, &set, T, 1).unwrap_err(),
        GestureError::NotDraggable(T, NotDraggable::Milestone)
    );
}

#[test]
fn r241_elapsed_duration() {
    // T 2 jours écoulés lun 19/10 08:00 – mer 21/10 08:00 ; n = 1 ⇒ cible
    // jeu 22/10 08:00 ⇒ 4 320 min civiles, unité et base gardées.
    let mut s = with_tasks(&[("T", "2ej")]);
    assert_eq!(get(&s, 1).duration.basis(), TimeBasis::Elapsed);
    resize(&mut s, 1);
    let d = get(&s, 1).duration;
    assert_eq!(
        (d.minutes(), d.basis()),
        (Minutes::new(4_320), TimeBasis::Elapsed)
    );
    assert_eq!(
        s.schedule().scheduled(T).unwrap().finish,
        moment("2026-10-22T08:00")
    );
}

#[test]
fn r241_started_not_draggable() {
    let mut s = with_tasks(&[("T", "2j")]);
    plannercraft_tests::task_edit::apply(
        &mut s,
        serde_json::json!([{ "op": "progress.update", "task": 1, "percent_complete": { "value": 1000 } }]),
    );
    let set = CalendarSet::compile(s.project()).0;
    assert_eq!(
        gesture::bar_resize(&s, &set, T, 1).unwrap_err(),
        GestureError::NotDraggable(T, NotDraggable::Started)
    );
}
