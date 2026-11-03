//! R-249 — Calendrier du projet par commande (`project.set_info
//! { calendar }`) : calendrier existant, de base ou dérivé ; `null`
//! refusé ; recalcul complet, R-113 comprise. Repère : début lun
//! 19/10/2026 08:00, calendriers fournis 1 (Standard), 2 (24 heures).

use plannercraft_automation::Session;
use plannercraft_automation::error::codes;
use plannercraft_core::{CalendarUid, Moment, TaskUid};
use plannercraft_tests::moment;
use plannercraft_tests::resource_edit::with_tasks;
use plannercraft_tests::task_edit::{apply, try_apply};
use serde_json::json;

fn dates(s: &Session, uid: u32) -> (Moment, Moment) {
    let t = s.schedule().scheduled(TaskUid::new(uid)).unwrap();
    (t.start, t.finish)
}

#[test]
fn r249_project_calendar_recalculates() {
    // Cahier : S → « 24 heures » ⇒ T 1 j (480) lun 19/10 08:00 – 16:00 ;
    // T2 1 j FS ⇒ lun 16:00 – mar 00:00.
    let mut s = with_tasks(&[("T", "1j"), ("T2", "1j")]);
    apply(&mut s, json!([{ "op": "link.add", "pred": 1, "succ": 2 }]));
    assert_eq!(
        dates(&s, 2),
        (moment("2026-10-20T08:00"), moment("2026-10-20T17:00"))
    );
    let o = apply(&mut s, json!([{ "op": "project.set_info", "calendar": 2 }]));
    assert_eq!(s.project().calendar, CalendarUid::new(2));
    assert_eq!(
        dates(&s, 1),
        (moment("2026-10-19T08:00"), moment("2026-10-19T16:00"))
    );
    assert_eq!(
        dates(&s, 2),
        (moment("2026-10-19T16:00"), moment("2026-10-20T00:00"))
    );
    assert!(o.diff.changed_total > 0);
    // Calendrier dérivé (comme A4) : accepté ; les marges se mesurent sur
    // sa base (R-113).
    apply(
        &mut s,
        json!([
            { "op": "calendar.add", "name": "Chantier", "base": 1 },
            { "op": "project.set_info", "calendar": 4 }
        ]),
    );
    assert_eq!(s.project().calendar, CalendarUid::new(4));
    assert_eq!(
        dates(&s, 2),
        (moment("2026-10-20T08:00"), moment("2026-10-20T17:00"))
    );
    // Inconnu, `null` : refusés, rien ne change.
    let before = s.project().clone();
    let err = try_apply(&mut s, json!([{ "op": "project.set_info", "calendar": 99 }])).unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref()),
        (codes::CALENDAR_NOT_FOUND, Some("calendar"))
    );
    let err = try_apply(&mut s, json!([{ "op": "project.set_info", "calendar": null }])).unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref()),
        (codes::PARAM_INVALID, Some("calendar"))
    );
    assert_eq!(s.project(), &before);
    // Annuler rend le calendrier précédent et ses dates.
    s.undo().unwrap();
    assert_eq!(s.project().calendar, CalendarUid::new(2));
    assert_eq!(
        dates(&s, 2),
        (moment("2026-10-19T16:00"), moment("2026-10-20T00:00"))
    );
}
