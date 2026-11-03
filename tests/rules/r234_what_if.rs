//! R-234 — `what_if` : `Session::simulate` exécute le lot sur une copie
//! (même pipeline que `dry_run`), la session ne change pas ; réponse
//! `{ accepted, errors?, diff, project_finish_variance?, explanations }` ;
//! un lot refusé est rendu comme donnée. Repère : calendrier Standard,
//! début du projet lun 02/11/2026 08:00. Hypothèse.

#![allow(
    clippy::many_single_char_names,
    reason = "tests : noms des tâches des exemples (A, B, C…)"
)]

use plannercraft_automation::Workspace;
use plannercraft_automation::text::Language;
use plannercraft_tests::moment;
use serde_json::{Value, json};

/// Exemple de R-234 : A 5 j (lun 02/11/2026) → B 1 j (FS), ligne 0 posée.
/// Rend l'espace et les UID de A et B.
pub fn ab_with_baseline() -> (Workspace, u64, u64) {
    let mut ws = Workspace::new(moment("2026-11-02T08:00"), Language::Fr);
    ws.call(
        "project.new",
        json!({ "name": "Essai", "start": "2026-11-02T08:00" }),
    )
    .unwrap();
    let out = ws
        .call(
            "batch.apply",
            json!({ "ops": [
                { "op": "task.add", "name": "A", "as": "$a", "duration": { "text": "5j" } },
                { "op": "task.add", "name": "B", "as": "$b", "duration": { "text": "1j" } },
                { "op": "link.add", "pred": "$a", "succ": "$b" },
                { "op": "baseline.set" }
            ] }),
        )
        .unwrap();
    let a = out["labels"]["$a"].as_u64().unwrap();
    let b = out["labels"]["$b"].as_u64().unwrap();
    (ws, a, b)
}

/// Empreinte complète de la session (projet, planning, historique,
/// génération, enregistrement, compteurs d'UID).
fn fingerprint(ws: &Workspace) -> String {
    format!("{:?}", ws.session().unwrap())
}

fn seven_days(a: u64) -> Value {
    json!([{ "op": "task.update", "uid": a, "duration": { "text": "7j" } }])
}

#[test]
fn r234_what_if_session_untouched() {
    let (mut ws, a, b) = ab_with_baseline();
    let before = fingerprint(&ws);
    let summary = ws.call("project.summary", json!({})).unwrap();
    let v = ws
        .call(
            "analysis.what_if",
            json!({ "ops": seven_days(a), "baseline": 0, "explain": [b] }),
        )
        .unwrap();
    assert_eq!(v["accepted"], json!(true));
    assert_eq!(fingerprint(&ws), before);
    assert_eq!(ws.call("project.summary", json!({})).unwrap(), summary);
    // Le diff est celui d'un `dry_run` du même lot.
    let dry = ws
        .call("batch.apply", json!({ "ops": seven_days(a), "dry_run": true }))
        .unwrap();
    assert_eq!(v["diff"], dry["diff"]);
    assert_eq!(fingerprint(&ws), before);
    // Exemple : A finit mar 10/11 17:00 (Δ 5 760 min civiles), B mer 11/11.
    let changed = v["diff"]["changed"].as_array().unwrap();
    let a_change = changed.iter().find(|c| c["uid"] == json!(a)).unwrap();
    assert_eq!(a_change["finish_after"], json!("2026-11-10T17:00"));
    assert_eq!(a_change["finish_delta_min"], json!(5_760));
    let b_change = changed.iter().find(|c| c["uid"] == json!(b)).unwrap();
    assert_eq!(b_change["start_after"], json!("2026-11-11T08:00"));
    assert_eq!(v["diff"]["finish_after"], json!("2026-11-11T17:00"));
}

#[test]
fn r234_refusal_as_data() {
    let (mut ws, a, b) = ab_with_baseline();
    let before = fingerprint(&ws);
    // Cycle : refus R-160, rendu comme donnée, jamais une erreur d'outil.
    let v = ws
        .call(
            "analysis.what_if",
            json!({ "ops": [{ "op": "link.add", "pred": b, "succ": a }], "explain": [a] }),
        )
        .unwrap();
    assert_eq!(v["accepted"], json!(false));
    assert_eq!(v["errors"][0]["code"], json!("batch.would_create_errors"));
    assert_eq!(v["errors"][0]["op_index"], json!(0));
    assert!(v.get("diff").is_none());
    assert_eq!(v["explanations"], json!([]));
    // Commande refusée : même forme, avec son index.
    let v = ws
        .call(
            "analysis.what_if",
            json!({ "ops": [
                { "op": "task.update", "uid": a, "name": "A2" },
                { "op": "task.delete", "uid": 999 }
            ] }),
        )
        .unwrap();
    assert_eq!(v["accepted"], json!(false));
    assert_eq!(v["errors"][0]["code"], json!("task.not_found"));
    assert_eq!(v["errors"][0]["op_index"], json!(1));
    assert_eq!(fingerprint(&ws), before);
    // Paramètres invalides : erreurs d'outil.
    let e = ws
        .call(
            "analysis.what_if",
            json!({ "ops": [], "explain": [a, a, a, a, a, a] }),
        )
        .unwrap_err();
    assert_eq!((e.code, e.field.as_deref()), ("limit.exceeded", Some("explain")));
    let e = ws
        .call("analysis.what_if", json!({ "ops": [], "baseline": 11 }))
        .unwrap_err();
    assert_eq!((e.code, e.field.as_deref()), ("param.invalid", Some("baseline")));
    // Lot vide : accepté, rien ne change.
    let v = ws.call("analysis.what_if", json!({ "ops": [] })).unwrap();
    assert_eq!(v["accepted"], json!(true));
    assert_eq!(v["diff"]["changed_total"], json!(0));
    assert_eq!(fingerprint(&ws), before);
}

#[test]
fn r234_explain_after() {
    let (mut ws, a, b) = ab_with_baseline();
    let v = ws
        .call(
            "analysis.what_if",
            json!({ "ops": seven_days(a), "explain": [b] }),
        )
        .unwrap();
    // B expliquée sur l'état simulé : pilote `Link{A, FS}`, borne brute
    // mar 10/11 17:00, recalée mer 11/11 08:00.
    let x = &v["explanations"][0];
    assert_eq!(x["task"]["uid"], json!(b));
    assert_eq!(x["date"], json!("2026-11-11T08:00"));
    assert_eq!(x["steps"][0]["driver"]["kind"], json!("link"));
    assert_eq!(x["steps"][0]["driver"]["predecessor"], json!(a));
    assert_eq!(x["steps"][0]["raw"], json!("2026-11-10T17:00"));
    // L'état courant, lui, n'a pas bougé.
    let now = ws.call("analysis.explain_date", json!({ "task": b })).unwrap();
    assert_eq!(now["date"], json!("2026-11-09T08:00"));
    // Une tâche créée par le lot simulé s'explique par son étiquette.
    let v = ws
        .call(
            "analysis.what_if",
            json!({
                "ops": [
                    { "op": "task.add", "name": "C", "as": "$c", "duration": { "text": "2j" } },
                    { "op": "link.add", "pred": b, "succ": "$c" }
                ],
                "explain": ["$c"]
            }),
        )
        .unwrap();
    let c = v["labels"]["$c"].clone();
    assert_eq!(v["explanations"][0]["task"]["uid"], c);
    assert_eq!(v["explanations"][0]["date"], json!("2026-11-10T08:00"));
    // Étiquette inconnue : erreur d'outil sur `explain`.
    let e = ws
        .call(
            "analysis.what_if",
            json!({ "ops": seven_days(a), "explain": ["$z"] }),
        )
        .unwrap_err();
    assert_eq!(
        (e.code, e.field.as_deref()),
        ("batch.unknown_label", Some("explain"))
    );
}

#[test]
fn r234_finish_variance() {
    let (mut ws, a, _) = ab_with_baseline();
    // Exemple : fin du projet +960 (2 j) contre la ligne 0 (fin de base
    // lun 09/11 17:00), ligne déduite des tâches (projet créé par PlannerCraft).
    let v = ws
        .call("analysis.what_if", json!({ "ops": seven_days(a), "baseline": 0 }))
        .unwrap();
    assert_eq!(v["project_finish_variance"]["minutes"], json!(960));
    assert_eq!(v["project_finish_variance"]["text"], json!("2j"));
    assert_eq!(v["baseline_finish"], json!("2026-11-09T17:00"));
    assert_eq!(v["baseline_derived"], json!(true));
    // Sans `baseline` ou sans ligne n : absent.
    let v = ws
        .call("analysis.what_if", json!({ "ops": seven_days(a) }))
        .unwrap();
    assert!(v.get("project_finish_variance").is_none());
    let v = ws
        .call("analysis.what_if", json!({ "ops": seven_days(a), "baseline": 3 }))
        .unwrap();
    assert!(v.get("project_finish_variance").is_none());
    // Lot qui avance la fin : écart négatif.
    let v = ws
        .call(
            "analysis.what_if",
            json!({ "ops": [{ "op": "task.update", "uid": a, "duration": { "text": "4j" } }], "baseline": 0 }),
        )
        .unwrap();
    assert_eq!(v["project_finish_variance"]["minutes"], json!(-480));
}
