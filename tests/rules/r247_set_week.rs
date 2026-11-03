//! R-247 — Semaine type par commande (`calendar.set_week`) : jours cités
//! réglés (hérité, chômé, ouvré sur des plages), jours non cités inchangés ;
//! `inherit` sur un calendrier de base refusé ; plages triées, disjointes,
//! de durée > 0, 1 à 5 par jour ; semaine sans minute ouvrée acceptée avec
//! l'avertissement `calendar.no_working_time` (R-160 amendée), sans boucle.
//! Repère : calendrier Standard (UID 1, calendrier du projet), début lun
//! 19/10/2026 08:00.

use plannercraft_automation::error::codes;
use plannercraft_automation::{ApplyMode, Session};
use plannercraft_core::calendar::{CalendarKind, DayRule, DaySetting};
use plannercraft_core::{CalendarUid, Diagnostic, Moment, TaskUid, Weekday};
use plannercraft_tests::moment;
use plannercraft_tests::resource_edit::with_tasks;
use plannercraft_tests::task_edit::{apply, batch, try_apply};
use serde_json::{Value, json};

/// T (2 j) affectée à Maçon (calendrier propre UID 4, dérivé de S),
/// début saisi ven 23/10 08:00.
fn mason_from_friday() -> Session {
    let mut s = with_tasks(&[("T", "2j")]);
    apply(
        &mut s,
        json!([
            { "op": "resource.add", "name": "Maçon" },
            { "op": "assignment.add", "task": 1, "resource": 1 },
            { "op": "task.update", "uid": 1, "start": "2026-10-23T08:00" }
        ]),
    );
    s
}

fn dates(s: &Session) -> (Moment, Moment) {
    let t = s.schedule().scheduled(TaskUid::new(1)).unwrap();
    (t.start, t.finish)
}

fn set_week(calendar: &Value, days: &Value) -> Value {
    json!([{ "op": "calendar.set_week", "calendar": calendar, "days": days }])
}

#[test]
fn r247_saturday_ranges_example() {
    // Cahier : samedi 08:00-12:00 sur le calendrier de Maçon ⇒ ven 8 h +
    // sam 4 h + lun 4 h, fin lun 26/10 12:00 (au lieu du lun 17:00).
    let mut s = mason_from_friday();
    assert_eq!(
        dates(&s),
        (moment("2026-10-23T08:00"), moment("2026-10-26T17:00"))
    );
    apply(
        &mut s,
        set_week(
            &json!(4),
            &json!([{ "day": "saturday", "setting": { "ranges": [{ "from": "08:00", "to": "12:00" }] } }]),
        ),
    );
    assert_eq!(
        dates(&s),
        (moment("2026-10-23T08:00"), moment("2026-10-26T12:00"))
    );
    let p = s.project();
    let CalendarKind::Derived { base, week } = p.calendars.get(CalendarUid::new(4)).unwrap().kind() else {
        panic!("calendrier dérivé attendu");
    };
    assert_eq!(*base, CalendarUid::new(1));
    assert!(matches!(week.get(Weekday::Saturday), DayRule::Working(h) if h.total().get() == 240));
    // Jours non cités : hérités, inchangés.
    for d in [Weekday::Sunday, Weekday::Monday, Weekday::Friday] {
        assert_eq!(week.get(d), &DayRule::Inherit, "{d:?}");
    }
    // Retour à l'héritage : la fin redevient lun 17:00.
    apply(
        &mut s,
        set_week(&json!(4), &json!([{ "day": "saturday", "setting": "inherit" }])),
    );
    assert_eq!(dates(&s).1, moment("2026-10-26T17:00"));
    // Calendrier du projet par `"project"` : vendredi chômé ⇒ la tâche
    // garde son calendrier de ressource, inchangée ; le Standard change.
    apply(
        &mut s,
        set_week(
            &json!("project"),
            &json!([{ "day": "friday", "setting": "non_working" }]),
        ),
    );
    let p = s.project();
    let CalendarKind::Base { week } = p.calendars.get(p.calendar).unwrap().kind() else {
        panic!("calendrier de base attendu");
    };
    assert_eq!(week.get(Weekday::Friday), &DaySetting::NonWorking);
    assert!(matches!(week.get(Weekday::Thursday), DaySetting::Working(h) if h.total().get() == 480));
}

#[test]
fn r247_inherit_on_base_refused() {
    let mut s = mason_from_friday();
    let before = s.project().clone();
    let err = try_apply(
        &mut s,
        set_week(
            &json!("project"),
            &json!([
                { "day": "monday", "setting": "non_working" },
                { "day": "sunday", "setting": "inherit" }
            ]),
        ),
    )
    .unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref(), err.op_index),
        (codes::PARAM_INVALID, Some("days[1].setting"), Some(0))
    );
    assert_eq!(s.project(), &before);
    // Calendrier inconnu.
    let err = try_apply(&mut s, set_week(&json!(99), &json!([]))).unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref()),
        (codes::CALENDAR_NOT_FOUND, Some("calendar"))
    );
}

#[test]
fn r247_ranges_validated() {
    let mut s = mason_from_friday();
    let before = s.project().clone();
    let r = |a: &str, b: &str| json!({ "from": a, "to": b });
    let cases = [
        // Inversée, vide, chevauchante, non triée, six plages, aucune.
        (json!([r("12:00", "08:00")]), "days[0].setting"),
        (json!([r("08:00", "08:00")]), "days[0].setting"),
        (
            json!([r("08:00", "12:00"), r("11:00", "13:00")]),
            "days[0].setting",
        ),
        (
            json!([r("13:00", "17:00"), r("08:00", "12:00")]),
            "days[0].setting",
        ),
        (
            json!([
                r("00:00", "01:00"),
                r("02:00", "03:00"),
                r("04:00", "05:00"),
                r("06:00", "07:00"),
                r("08:00", "09:00"),
                r("10:00", "11:00")
            ]),
            "days[0].setting",
        ),
        (json!([]), "days[0].setting"),
    ];
    for (ranges, field) in cases {
        let ops = set_week(
            &json!(4),
            &json!([{ "day": "saturday", "setting": { "ranges": ranges } }]),
        );
        let err = try_apply(&mut s, ops).unwrap_err();
        assert_eq!(
            (err.code, err.field.as_deref()),
            (codes::PARAM_INVALID, Some(field)),
            "{ranges}"
        );
    }
    // Jour cité deux fois.
    let err = try_apply(
        &mut s,
        set_week(
            &json!(4),
            &json!([
                { "day": "saturday", "setting": "non_working" },
                { "day": "saturday", "setting": "inherit" }
            ]),
        ),
    )
    .unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref()),
        (codes::PARAM_INVALID, Some("days[1].day"))
    );
    // « 24:00 » admis, plages contiguës admises, cinq plages au plus.
    apply(
        &mut s,
        set_week(
            &json!(4),
            &json!([{ "day": "sunday", "setting": { "ranges": [
                r("00:00", "04:00"), r("04:00", "08:00"), r("10:00", "12:00"), r("13:00", "17:00"), r("20:00", "24:00")
            ] } }]),
        ),
    );
    assert_ne!(s.project(), &before);
}

#[test]
fn r247_no_working_time_warns_no_loop() {
    // Calendrier de Maçon entièrement chômé : accepté, avertissement
    // `calendar.no_working_time` ; la tâche repasse sur le calendrier du
    // projet (R-066), sans boucle.
    let mut s = mason_from_friday();
    let all = |setting: &str| -> Value {
        let days: Vec<Value> = [
            "monday",
            "tuesday",
            "wednesday",
            "thursday",
            "friday",
            "saturday",
            "sunday",
        ]
        .iter()
        .map(|d| json!({ "day": d, "setting": setting }))
        .collect();
        Value::Array(days)
    };
    let ops = set_week(&json!(4), &all("non_working"));
    let dry = s.apply(&batch(ops.clone()), ApplyMode::DryRun).unwrap();
    let o = apply(&mut s, ops);
    assert_eq!(o.warnings, dry.warnings);
    assert!(
        o.warnings.contains(&Diagnostic::CalendarNoWorkingTime {
            calendar: CalendarUid::new(4)
        }),
        "{:?}",
        o.warnings
    );
    assert!(o.warnings.iter().any(|d| d.code() == "task.calendar_fallback"));
    assert_eq!(
        dates(&s),
        (moment("2026-10-23T08:00"), moment("2026-10-26T17:00"))
    );
    // Calendrier du projet entièrement chômé : accepté aussi, repli sur le
    // Standard intégré (`project.calendar_fallback`), dates calculées.
    let o = apply(&mut s, set_week(&json!("project"), &all("non_working")));
    let codes: Vec<&str> = o.warnings.iter().map(Diagnostic::code).collect();
    assert!(codes.contains(&"calendar.no_working_time"), "{codes:?}");
    assert!(codes.contains(&"project.calendar_fallback"), "{codes:?}");
    assert_eq!(
        dates(&s),
        (moment("2026-10-23T08:00"), moment("2026-10-26T17:00"))
    );
    // Une autre erreur nouvelle reste refusée (R-160) dans le même état.
    let err = try_apply(
        &mut s,
        json!([
            { "op": "task.add", "name": "U", "duration": { "text": "1j" } },
            { "op": "link.add", "pred": 1, "succ": 2 },
            { "op": "link.add", "pred": 2, "succ": 1 }
        ]),
    )
    .unwrap_err();
    assert_eq!(err.code, codes::BATCH_WOULD_CREATE_ERRORS);
    // Annuler rend la semaine d'origine, sans avertissement nouveau.
    s.undo().unwrap();
    s.undo().unwrap();
    assert_eq!(
        dates(&s),
        (moment("2026-10-23T08:00"), moment("2026-10-26T17:00"))
    );
    let p = s.project();
    assert!(p.calendars.get(p.calendar).unwrap().is_base());
}
