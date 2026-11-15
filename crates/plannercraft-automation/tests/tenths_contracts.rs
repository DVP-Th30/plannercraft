//! Contrats JSON au dixième de minute (ADR-0039 D5, lots B2-B3).
//!
//! En sortie, une valeur qui n'est pas une minute entière s'écrit sans
//! perte (minutes décimales, secondes d'un instant) ; une valeur entière
//! reste un entier JSON. En saisie, les commandes refusent encore une
//! fraction de minute (le calcul ne la prend en charge qu'au lot B4), et
//! le disent, au lieu de l'arrondir plus loin en silence.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

use plannercraft_automation::error::codes;
use plannercraft_automation::json::{DurationJson, MinutesNumber};
use plannercraft_automation::text::{Language, TextContext};
use plannercraft_automation::view::work_out;
use plannercraft_automation::{DocOrigin, Session, Workspace};
use plannercraft_core::{Duration, DurationUnit, Minutes, Moment, ProjectOptions, TaskUid, TimeBasis, Work};
use serde_json::{Value, json};

fn now() -> Moment {
    "2026-11-02T08:00".parse().unwrap()
}

/// Projet neuf avec deux tâches de 2 jours liées (UID 1 → 2).
fn two_tasks() -> Workspace {
    let mut ws = Workspace::new(now(), Language::Fr);
    ws.call("project.new", json!({ "name": "P", "start": "2026-11-02T08:00" }))
        .unwrap();
    ws.call(
        "batch.apply",
        json!({ "ops": [
            { "op": "task.add", "name": "A", "as": "$a", "duration": { "text": "2j" } },
            { "op": "task.add", "name": "B", "as": "$b", "duration": { "text": "2j" } },
            { "op": "link.add", "pred": "$a", "succ": "$b" }
        ] }),
    )
    .unwrap();
    ws
}

#[test]
fn whole_values_stay_json_integers() {
    let ctx = TextContext::for_options(&ProjectOptions::default(), Language::Fr);
    let d = Duration::new(Minutes::new(960), DurationUnit::Day, TimeBasis::Working, false).unwrap();
    let out = serde_json::to_value(DurationJson::from_duration(&d, &ctx)).unwrap();
    assert_eq!(out["minutes"], json!(960));
    assert!(out["minutes"].is_i64(), "entier JSON, jamais 960.0");
    assert_eq!(serde_json::to_string(&MinutesNumber::whole(480)).unwrap(), "480");
}

#[test]
fn fractions_are_written_exactly() {
    let ctx = TextContext::for_options(&ProjectOptions::default(), Language::Fr);
    let d = Duration::new(
        Minutes::from_tenths(1_125),
        DurationUnit::Minute,
        TimeBasis::Working,
        false,
    )
    .unwrap();
    let out = serde_json::to_value(DurationJson::from_duration(&d, &ctx)).unwrap();
    assert_eq!(out["minutes"], json!(112.5));
    // Travail au millième : minutes exactes, pas de texte (il ne saurait
    // pas redonner le millième).
    let w = work_out(Work::from_milli(112_125).unwrap(), &ctx);
    assert_eq!(serde_json::to_value(w.minutes).unwrap(), json!(112.125));
    assert_eq!(w.text, None);
    let w = work_out(Work::from_milli(480_000).unwrap(), &ctx);
    assert_eq!(serde_json::to_value(w.minutes).unwrap(), json!(480));
    assert_eq!(w.text.as_deref(), Some("8h"));
    // Lecture du nombre : sur l'échelle ou refusé.
    let read = |s: &str| serde_json::from_str::<MinutesNumber>(s).map(MinutesNumber::milli);
    assert_eq!(read("112.5").unwrap(), 112_500);
    assert_eq!(read("0.125").unwrap(), 125);
    assert_eq!(read("480.0").unwrap(), 480_000);
    assert!(read("0.0005").is_err());
}

/// Les vues d'un modèle qui porte des fractions (relu d'un fichier du
/// modèle) les écrivent sans perte : durée en minutes décimales, dates
/// avec leurs secondes.
#[test]
fn views_write_fractions_of_the_model() {
    let mut ws = two_tasks();
    let mut project = ws.session().unwrap().project().clone();
    let a = project.tasks.get_mut(TaskUid::new(1)).unwrap();
    a.duration = Duration::new(
        Minutes::from_tenths(9_605),
        DurationUnit::Minute,
        TimeBasis::Working,
        false,
    )
    .unwrap();
    let origin = DocOrigin {
        name: "P".to_owned(),
        path: None,
    };
    ws.set_session(Session::from_project(project, origin, now(), Language::Fr));
    let a = ws.call("task.get", json!({ "uid": 1 })).unwrap();
    assert_eq!(a["duration"]["minutes"], json!(960.5));
    // 960 min = deux jours (lun.-mar.) ; la demi-minute tombe mercredi.
    assert_eq!(a["finish"], json!("2026-11-04T08:00:30"));
    let b = ws.call("task.get", json!({ "uid": 2 })).unwrap();
    assert_eq!(b["start"], json!("2026-11-04T08:00:30"));
}

/// Erreur d'un appel refusé.
fn refused(ws: &mut Workspace, tool: &str, params: Value) -> (String, Option<String>) {
    let e = ws.call(tool, params).unwrap_err();
    (e.code.to_owned(), e.field)
}

#[test]
fn entries_refuse_a_fraction_of_minute_until_b4() {
    let mut ws = two_tasks();
    let before = serde_json::to_vec(ws.session().unwrap().project()).unwrap();
    let invalid = codes::PARAM_INVALID.to_owned();
    let (code, _) = refused(
        &mut ws,
        "task.update",
        json!({ "uid": 1, "duration": { "minutes": 112.5, "unit": "minute" } }),
    );
    assert_eq!(code, invalid);
    let (code, _) = refused(
        &mut ws,
        "link.update",
        json!({ "pred": 1, "succ": 2, "lag": { "minutes": 0.5, "unit": "minute" } }),
    );
    assert_eq!(code, invalid);
    // Hors de l'échelle : refusé dès la lecture du nombre.
    let (code, _) = refused(
        &mut ws,
        "task.update",
        json!({ "uid": 1, "duration": { "minutes": 112.15 } }),
    );
    assert_eq!(code, invalid);
    for (tool, params, field) in [
        (
            "task.update",
            json!({ "uid": 1, "start": "2026-11-02T08:00:30" }),
            "start",
        ),
        (
            "task.set_constraint",
            json!({ "uid": 2, "kind": "snet", "date": "2026-11-04T08:00:06" }),
            "date",
        ),
        (
            "task.set_deadline",
            json!({ "uid": 2, "deadline": "2026-11-09T17:00:30" }),
            "deadline",
        ),
        (
            "project.set_status_date",
            json!({ "date": "2026-11-03T12:00:30" }),
            "date",
        ),
        (
            "project.set_anchor",
            json!({ "start": "2026-11-02T08:00:30" }),
            "start",
        ),
        (
            "progress.update",
            json!({ "task": 1, "actual_start": "2026-11-02T08:00:30" }),
            "actual_start",
        ),
    ] {
        let (code, f) = refused(&mut ws, tool, params);
        assert_eq!(
            (code.as_str(), f.as_deref()),
            (invalid.as_str(), Some(field)),
            "{tool}"
        );
    }
    // Un instant hors de l'échelle (7 s) est refusé dès la lecture.
    let (code, _) = refused(
        &mut ws,
        "task.update",
        json!({ "uid": 1, "start": "2026-11-02T08:00:07" }),
    );
    assert_eq!(code, invalid);
    // Rien n'a changé.
    assert_eq!(
        serde_json::to_vec(ws.session().unwrap().project()).unwrap(),
        before
    );
    // Un décimal entier reste une minute entière : accepté.
    ws.call(
        "task.update",
        json!({ "uid": 1, "duration": { "minutes": 1440.0, "unit": "day" } }),
    )
    .unwrap();
    let a = ws.call("task.get", json!({ "uid": 1 })).unwrap();
    assert_eq!(a["duration"]["minutes"], json!(1440));
    // `project.new` aussi.
    let mut fresh = Workspace::new(now(), Language::Fr);
    let e = fresh
        .call(
            "project.new",
            json!({ "name": "P", "start": "2026-11-02T08:00:30" }),
        )
        .unwrap_err();
    assert_eq!(e.code, codes::PARAM_INVALID);
}
