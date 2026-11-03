//! R-165 — Exceptions de calendrier par commande : `calendar.set_exception`
//! crée ou remplace l'exception exactement sur `[from, to]` (chômée sans
//! plage, ouvrée sur les plages données) ; tout autre chevauchement →
//! `calendar.exception_overlap` ; `calendar.remove_exception` retire celle
//! qui commence le jour donné. R-161 (calendriers) : la répartition des
//! affectations planifiées par ce calendrier est jetée.

use plannercraft_automation::error::codes;
use plannercraft_automation::{ApplyMode, ApplyOutcome, Batch, CommandError, Session, SideEffect};
use plannercraft_core::calendar::DaySetting;
use plannercraft_core::{
    Assignment, AssignmentUid, Constraint, ExtraEntry, Moment, Project, Subject, TaskUid,
};
use plannercraft_tests::moment;
use plannercraft_tests::mspdi::new_project;
use plannercraft_tests::session::{push_tasks, session};

fn run(s: &mut Session, ops: &str) -> Result<ApplyOutcome, CommandError> {
    let batch = Batch::from_json_str(&format!(r#"{{"format_version":1,"ops":[{ops}]}}"#)).unwrap();
    s.apply(&batch, ApplyMode::Commit)
}

/// Tâche 1 de 3 j, SNET mar 10/11/2026 08:00, calendrier du projet
/// (Standard).
fn three_days_from_tuesday() -> Project {
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1], 3);
    p.tasks.get_mut(TaskUid::new(1)).unwrap().constraint = Constraint::StartNoEarlierThan {
        date: moment("2026-11-10T08:00"),
    };
    p
}

fn finish(s: &Session) -> Moment {
    s.schedule().scheduled(TaskUid::new(1)).unwrap().finish
}

fn exceptions(s: &Session) -> usize {
    let p = s.project();
    p.calendars.get(p.calendar).unwrap().exceptions().len()
}

#[test]
fn r165_nonworking_day_shifts_finish() {
    // Cahier : mer 11/11 chômé → la tâche finit le ven 13/11 17:00 au lieu
    // du jeu 12/11.
    let mut s = session(three_days_from_tuesday());
    assert_eq!(finish(&s), moment("2026-11-12T17:00"));
    run(
        &mut s,
        r#"{"op":"calendar.set_exception","cal":"project","from":"2026-11-11","to":"2026-11-11"}"#,
    )
    .unwrap();
    assert_eq!(finish(&s), moment("2026-11-13T17:00"));
    // Même période, ouvrée le matin seulement : remplacement exact (une
    // seule exception), 4 h le mercredi → fin ven 12:00.
    run(
        &mut s,
        r#"{"op":"calendar.set_exception","cal":"project","from":"2026-11-11","to":"2026-11-11","ranges":[["08:00","12:00"]]}"#,
    )
    .unwrap();
    assert_eq!(exceptions(&s), 1);
    let p = s.project();
    let e = &p.calendars.get(p.calendar).unwrap().exceptions()[0];
    assert!(matches!(&e.setting, DaySetting::Working(h) if h.total().get() == 240));
    assert_eq!(finish(&s), moment("2026-11-13T12:00"));
    // Plages invalides (inversées, chevauchantes) → `param.invalid`.
    for ranges in [
        r#"[["12:00","08:00"]]"#,
        r#"[["08:00","12:00"],["11:00","13:00"]]"#,
    ] {
        let ops = format!(
            r#"{{"op":"calendar.set_exception","cal":"project","from":"2026-11-18","to":"2026-11-18","ranges":{ranges}}}"#
        );
        let err = run(&mut s, &ops).unwrap_err();
        assert_eq!(
            (err.code, err.field.as_deref()),
            (codes::PARAM_INVALID, Some("ranges")),
            "{ranges}"
        );
    }
    // `from` après `to` → `param.invalid`.
    let err = run(
        &mut s,
        r#"{"op":"calendar.set_exception","cal":"project","from":"2026-11-20","to":"2026-11-19"}"#,
    )
    .unwrap_err();
    assert_eq!(err.code, codes::PARAM_INVALID);
}

#[test]
fn r165_partial_overlap_refused() {
    let mut s = session(three_days_from_tuesday());
    run(
        &mut s,
        r#"{"op":"calendar.set_exception","cal":"project","from":"2026-11-11","to":"2026-11-13"}"#,
    )
    .unwrap();
    let before = s.project().clone();
    for (from, to) in [
        ("2026-11-12", "2026-11-16"),
        ("2026-11-09", "2026-11-11"),
        ("2026-11-12", "2026-11-12"),
    ] {
        let ops = format!(r#"{{"op":"calendar.set_exception","cal":"project","from":"{from}","to":"{to}"}}"#);
        let err = run(&mut s, &ops).unwrap_err();
        assert_eq!(err.code, codes::CALENDAR_EXCEPTION_OVERLAP, "{from}..{to}");
        let p = s.project();
        assert_eq!(err.subject, Some(Subject::Calendar(p.calendar)));
        assert_eq!(s.project(), &before);
    }
    // Contigu sans chevauchement : accepté.
    run(
        &mut s,
        r#"{"op":"calendar.set_exception","cal":"project","from":"2026-11-14","to":"2026-11-16"}"#,
    )
    .unwrap();
    assert_eq!(exceptions(&s), 2);
}

#[test]
fn r165_remove_exception() {
    let mut s = session(three_days_from_tuesday());
    run(
        &mut s,
        r#"{"op":"calendar.set_exception","cal":"project","from":"2026-11-11","to":"2026-11-11"}"#,
    )
    .unwrap();
    assert_eq!(finish(&s), moment("2026-11-13T17:00"));
    // Désignée par son premier jour ; un autre jour → introuvable.
    let err = run(
        &mut s,
        r#"{"op":"calendar.remove_exception","cal":"project","from":"2026-11-12"}"#,
    )
    .unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref()),
        (codes::CALENDAR_EXCEPTION_NOT_FOUND, Some("from"))
    );
    let cal = s.project().calendar.get();
    run(
        &mut s,
        &format!(r#"{{"op":"calendar.remove_exception","cal":{cal},"from":"2026-11-11"}}"#),
    )
    .unwrap();
    assert_eq!(exceptions(&s), 0);
    assert_eq!(finish(&s), moment("2026-11-12T17:00"));
    // Calendrier inconnu.
    let err = run(
        &mut s,
        r#"{"op":"calendar.remove_exception","cal":999,"from":"2026-11-11"}"#,
    )
    .unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref()),
        (codes::CALENDAR_NOT_FOUND, Some("cal"))
    );
    // Annuler rétablit l'exception.
    s.undo().unwrap();
    assert_eq!(exceptions(&s), 1);
    assert_eq!(finish(&s), moment("2026-11-13T17:00"));
}

#[test]
fn r161_exception_drops_assignment_timephased() {
    // R-161 (calendriers) : l'affectation de la tâche 1 (calendrier du
    // projet) perd sa `TimephasedData` ; un seul effet, sujet le calendrier.
    let mut p = three_days_from_tuesday();
    let mut a = Assignment::new(AssignmentUid::new(1), TaskUid::new(1), None);
    a.extras.0.push(ExtraEntry {
        origin: "mspdi".to_owned(),
        key: "TimephasedData".to_owned(),
        payload: "<TimephasedData><Type>2</Type></TimephasedData>".to_owned(),
    });
    p.assignments.push(a).unwrap();
    let cal = p.calendar;
    let mut s = session(p);
    let o = run(
        &mut s,
        r#"{"op":"calendar.set_exception","cal":"project","from":"2026-11-11","to":"2026-11-11"}"#,
    )
    .unwrap();
    assert!(o.side_effects.contains(&SideEffect::ExtrasDropped {
        subject: Subject::Calendar(cal),
        keys: vec!["TimephasedData".to_owned()],
    }));
    assert!(
        s.project()
            .assignments
            .get(AssignmentUid::new(1))
            .unwrap()
            .extras
            .is_empty()
    );
}

#[test]
fn r165_exception_name_round_trip() {
    // R-165 amendée (J7, O44) : nom facultatif (≤ 512 caractères), gardé à
    // un remplacement sans `name`, relu et écrit (`<Name>`).
    let mut s = session(three_days_from_tuesday());
    run(
        &mut s,
        r#"{"op":"calendar.set_exception","cal":"project","from":"2026-11-11","to":"2026-11-11","name":"Armistice"}"#,
    )
    .unwrap();
    let name = |s: &Session| {
        let p = s.project();
        p.calendars.get(p.calendar).unwrap().exceptions()[0].name.clone()
    };
    assert_eq!(name(&s), "Armistice");
    // Remplacement sans nom : gardé ; avec un nom : remplacé.
    run(
        &mut s,
        r#"{"op":"calendar.set_exception","cal":"project","from":"2026-11-11","to":"2026-11-11","ranges":[["08:00","12:00"]]}"#,
    )
    .unwrap();
    assert_eq!((exceptions(&s), name(&s).as_str()), (1, "Armistice"));
    run(
        &mut s,
        r#"{"op":"calendar.set_exception","cal":"project","from":"2026-11-11","to":"2026-11-11","name":"11 novembre"}"#,
    )
    .unwrap();
    assert_eq!(name(&s), "11 novembre");
    // Écrit puis relu : même nom, même période.
    let w = s.save_bytes(&plannercraft_mspdi::write::WriteOptions::default());
    assert!(String::from_utf8_lossy(&w.xml).contains("<Name>11 novembre</Name>"));
    let doc = plannercraft_tests::mspdi::reread(&w.xml);
    let p = &doc.project;
    let e = &p.calendars.get(p.calendar).unwrap().exceptions()[0];
    assert_eq!(
        (e.name.as_str(), e.period.first()),
        ("11 novembre", plannercraft_tests::day("2026-11-11"))
    );
    // Trop long : refusé.
    let long = "x".repeat(513);
    let ops = format!(
        r#"{{"op":"calendar.set_exception","cal":"project","from":"2026-11-18","to":"2026-11-18","name":"{long}"}}"#
    );
    let err = run(&mut s, &ops).unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref()),
        (codes::PARAM_INVALID, Some("name"))
    );
}
