//! R-163 — Diff de planning d'un lot (définition, tri, N).

use plannercraft_automation::text::Language;
use plannercraft_automation::{ApplyMode, ApplyOptions, Workspace};
use plannercraft_core::{TaskId, TaskUid};
use plannercraft_tests::moment;
use plannercraft_tests::mspdi::new_project;
use plannercraft_tests::session::{add_task, commit, delete_task, link, push_tasks, session, set_days};
use serde_json::{Value, json};

#[test]
fn r163_diff_example() {
    // Calendrier standard. A (UID 1) 5 j depuis lun 02/11/2026 08:00, fin
    // ven 06/11 17:00, passée à 7 j → fin mar 10/11 17:00 : Δ = 4 jours
    // civils = 5 760 min. B (UID 2, 1 j, FS de A) : fin lun 09/11 17:00 →
    // mer 11/11 17:00, Δ = 2 880 min. C (UID 3) indépendante : non changée.
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1], 5);
    push_tasks(&mut p, &[2, 3], 1);
    link(&mut p, 1, 2);
    let mut s = session(p);
    let o = commit(&mut s, "a", &[set_days(1, 7)]);
    let d = &o.diff;
    assert_eq!(d.changed_total, 2);
    // A (durée) et B (dates) : touchées = changées.
    assert_eq!(d.touched_total, 2);
    assert!(!d.truncated);
    let uids: Vec<u32> = d.changed.iter().map(|c| c.uid.get()).collect();
    assert_eq!(uids, vec![1, 2]);
    let first = &d.changed[0];
    assert_eq!(
        first.finish,
        (Some(moment("2026-11-06T17:00")), Some(moment("2026-11-10T17:00")))
    );
    assert_eq!(first.finish_delta_min, 5_760);
    assert_eq!(
        (first.id_before, first.id_after),
        (Some(TaskId::new(1)), Some(TaskId::new(1)))
    );
    assert_eq!(d.changed[1].finish_delta_min, 2_880);
    assert_eq!(
        d.project_finish,
        (moment("2026-11-09T17:00"), moment("2026-11-11T17:00"))
    );
    assert_eq!(
        d.project_start,
        (moment("2026-11-02T08:00"), moment("2026-11-02T08:00"))
    );
}

#[test]
fn r163_diff_sorted_and_truncated() {
    // Chaîne de 15 tâches de 1 j ; la première passe à 2 j : 15 tâches
    // décalées d'un jour ouvré. Δ de 1 440 min sauf au passage du week-end :
    // les tâches 5, 10 et 15 (fins du vendredi) passent au lundi (Δ 3 j
    // civils = 4 320 min) ; tri par |Δ| décroissant puis par ID. N = 10 par
    // défaut, `changed_total` exact.
    let mut p = new_project("2026-11-02T08:00");
    let uids: Vec<u32> = (1..=15).collect();
    push_tasks(&mut p, &uids, 1);
    for u in 2..=15 {
        link(&mut p, u - 1, u);
    }
    let mut s = session(p);
    let o = s
        .apply_edits(
            None,
            &[set_days(1, 2)],
            ApplyMode::DryRun,
            ApplyOptions::default(),
        )
        .unwrap();
    assert_eq!(o.diff.changed_total, 15);
    assert_eq!(o.diff.touched_total, 15);
    assert_eq!(o.diff.changed.len(), 10);
    assert!(o.diff.truncated);
    let first: Vec<u32> = o.diff.changed.iter().take(4).map(|c| c.uid.get()).collect();
    assert_eq!(first, vec![5, 10, 15, 1]);
    assert_eq!(o.diff.changed[0].finish_delta_min, 4_320);
    assert_eq!(o.diff.changed[3].finish_delta_min, 1_440);
    for w in o.diff.changed.windows(2) {
        let (prev, next) = (&w[0], &w[1]);
        let (dp, dn) = (prev.finish_delta_min.abs(), next.finish_delta_min.abs());
        assert!(
            dp > dn || (dp == dn && prev.id_after < next.id_after),
            "{prev:?} / {next:?}"
        );
    }
    // Détail : jusqu'à `diff_detail_max`.
    let o = s
        .apply_edits(
            None,
            &[set_days(1, 2)],
            ApplyMode::DryRun,
            ApplyOptions {
                detail: true,
                ..ApplyOptions::default()
            },
        )
        .unwrap();
    assert_eq!(o.diff.changed.len(), 15);
    assert!(!o.diff.truncated);
}

#[test]
fn r163_critical_entered_left() {
    // A 5 j et B 3 j en parallèle : A critique. B passe à 7 j : B devient
    // critique, A ne l'est plus (listes triées par UID).
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1], 5);
    push_tasks(&mut p, &[2], 3);
    let mut s = session(p);
    let o = commit(&mut s, "b", &[set_days(2, 7)]);
    assert_eq!(o.diff.critical_entered, vec![TaskUid::new(2)]);
    assert_eq!(o.diff.critical_left, vec![TaskUid::new(1)]);
    // A n'a pas bougé : sa marge seule a changé, elle n'est pas « changée ».
    let uids: Vec<u32> = o.diff.changed.iter().map(|c| c.uid.get()).collect();
    assert_eq!(uids, vec![2]);
    assert_eq!(o.diff.touched_total, 1);
}

#[test]
fn r163_added_and_deleted_tasks() {
    // Ajout (avant absent) et suppression (après absent) : changées, Δ 0 ;
    // la supprimée (sans ID d'après) en dernier.
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1, 2], 1);
    let mut s = session(p);
    let o = commit(&mut s, "a", &[delete_task(1), add_task("N", 2)]);
    let d = &o.diff;
    assert_eq!(d.changed_total, 2);
    assert_eq!(d.touched_total, 2);
    let added = &d.changed[0];
    assert_eq!(added.uid, TaskUid::new(3));
    assert_eq!((added.id_before, added.id_after), (None, Some(TaskId::new(2))));
    assert_eq!(added.start.0, None);
    assert_eq!(added.start.1, Some(moment("2026-11-02T08:00")));
    assert_eq!(added.finish_delta_min, 0);
    let deleted = &d.changed[1];
    assert_eq!(deleted.uid, TaskUid::new(1));
    assert_eq!(
        (deleted.id_before, deleted.id_after),
        (Some(TaskId::new(1)), None)
    );
    assert_eq!(deleted.finish.1, None);
    assert_eq!(d.critical_left, vec![TaskUid::new(1), TaskUid::new(2)]);
    assert_eq!(d.critical_entered, vec![TaskUid::new(3)]);
}

// --- R-163 amendée (2026-11-11) : tâches touchées (`touched_total`) ---

/// Espace de travail : projet « P » (lun 02/11/2026 08:00, calendrier
/// standard) et le lot `ops` commité.
fn workspace(ops: &Value) -> Workspace {
    let mut ws = Workspace::new(moment("2026-11-02T08:00"), Language::Fr);
    ws.call("project.new", json!({ "name": "P", "start": "2026-11-02T08:00" }))
        .unwrap();
    ws.call("batch.apply", json!({ "ops": ops })).unwrap();
    ws
}

/// (`touched_total`, `changed_total`) d'un diff JSON.
fn counts(diff: &Value) -> (u64, u64) {
    (
        diff["touched_total"].as_u64().unwrap(),
        diff["changed_total"].as_u64().unwrap(),
    )
}

/// `dry_run`, `what_if` puis commit du lot `ops` sur `ws` : les trois
/// diffs sont identiques (même pipeline) ; rend ses deux compteurs.
fn touched_and_changed(ws: &mut Workspace, ops: &Value) -> (u64, u64) {
    let dry = ws
        .call("batch.apply", json!({ "ops": ops, "dry_run": true }))
        .unwrap();
    let what_if = ws.call("analysis.what_if", json!({ "ops": ops })).unwrap();
    assert_eq!(what_if["accepted"], json!(true));
    assert_eq!(what_if["diff"], dry["diff"]);
    let committed = ws.call("batch.apply", json!({ "ops": ops })).unwrap();
    assert_eq!(committed["diff"], dry["diff"]);
    let (touched, changed) = counts(&dry["diff"]);
    assert!(touched >= changed, "{touched} < {changed}");
    (touched, changed)
}

/// Cinq tâches indépendantes de 2 j et une sixième, laissée à 0 %.
fn six_tasks() -> Workspace {
    let ops: Vec<Value> = (1..=6)
        .map(|i| json!({ "op": "task.add", "name": format!("T{i}"), "duration": { "text": "2j" } }))
        .collect();
    workspace(&Value::Array(ops))
}

#[test]
fn r163_touched_progress_only() {
    // Exemple de la règle : cinq tâches passées à 100 % sans que leurs
    // dates bougent ⇒ `touched_total` 5, `changed_total` 0 (« 5 tâches
    // modifiées · aucune date déplacée »).
    let mut ws = six_tasks();
    let ops: Vec<Value> = (1..=5)
        .map(|uid| json!({ "op": "progress.update", "task": uid, "percent_complete": { "text": "100%" } }))
        .collect();
    assert_eq!(touched_and_changed(&mut ws, &Value::Array(ops)), (5, 0));
}

#[test]
fn r163_touched_rename_only() {
    let mut ws = six_tasks();
    let ops = json!([{ "op": "task.update", "uid": 3, "name": "Autre nom", "notes": "n" }]);
    assert_eq!(touched_and_changed(&mut ws, &ops), (1, 0));
}

#[test]
fn r163_touched_union_with_moved_dates() {
    // A 5 j → B 1 j → C 1 j, D indépendante. A passe à 7 j (A, B, C
    // décalées) et D est renommée : touchées = {A, B, C} ∪ {D} = 4, dates
    // déplacées = 3. B et C sont touchées par leurs dates seules.
    let mut ws = workspace(&json!([
        { "op": "task.add", "name": "A", "as": "$a", "duration": { "text": "5j" } },
        { "op": "task.add", "name": "B", "as": "$b", "duration": { "text": "1j" } },
        { "op": "task.add", "name": "C", "as": "$c", "duration": { "text": "1j" } },
        { "op": "task.add", "name": "D", "duration": { "text": "1j" } },
        { "op": "link.add", "pred": "$a", "succ": "$b" },
        { "op": "link.add", "pred": "$b", "succ": "$c" }
    ]));
    let ops = json!([
        { "op": "task.update", "uid": 1, "duration": { "text": "7j" } },
        { "op": "task.update", "uid": 4, "name": "D bis" }
    ]);
    assert_eq!(touched_and_changed(&mut ws, &ops), (4, 3));
}

#[test]
fn r163_touched_added_and_deleted() {
    let mut ws = six_tasks();
    let add = json!([{ "op": "task.add", "name": "N", "duration": { "text": "1j" } }]);
    assert_eq!(touched_and_changed(&mut ws, &add), (1, 1));
    let delete = json!([{ "op": "task.delete", "uid": 2 }]);
    assert_eq!(touched_and_changed(&mut ws, &delete), (1, 1));
}

#[test]
fn r163_touched_assignment_counts_its_task() {
    // Une ressource ajoutée ne touche aucune tâche ; son affectation touche
    // la tâche affectée (donnée de la tâche : ses affectations).
    let mut ws = six_tasks();
    let resource = json!([{ "op": "resource.add", "name": "Maçon" }]);
    assert_eq!(touched_and_changed(&mut ws, &resource), (0, 0));
    let assign = json!([{ "op": "assignment.add", "task": 2, "resource": 1 }]);
    let (touched, _) = touched_and_changed(&mut ws, &assign);
    assert_eq!(touched, 1);
}

#[test]
fn r163_touched_empty_batch() {
    let mut ws = six_tasks();
    assert_eq!(touched_and_changed(&mut ws, &json!([])), (0, 0));
}

#[test]
fn r163_touched_undo_redo() {
    // Annuler et rétablir rendent le diff du delta de l'entrée : mêmes
    // tâches touchées que le lot.
    let mut ws = six_tasks();
    let ops: Vec<Value> = (1..=5)
        .map(|uid| json!({ "op": "progress.update", "task": uid, "percent_complete": { "text": "100%" } }))
        .collect();
    ws.call("batch.apply", json!({ "ops": ops })).unwrap();
    let undo = ws.call("history.undo", json!({})).unwrap();
    assert_eq!(counts(&undo["diff"]), (5, 0));
    let redo = ws.call("history.redo", json!({})).unwrap();
    assert_eq!(counts(&redo["diff"]), (5, 0));
}

#[test]
fn r163_touched_summary_and_order() {
    // Deux sous-tâches passées à 100 % : la récapitulative, dont
    // l'avancement est calculé et non enregistré, n'est pas touchée.
    // Déplacer une tâche dans la liste sans changer de niveau ne change
    // que l'ordre (et les ID) : aucune tâche touchée (règle consignée).
    let mut ws = workspace(&json!([
        { "op": "task.add", "name": "S", "duration": { "text": "1j" } },
        { "op": "task.add", "name": "a", "duration": { "text": "2j" } },
        { "op": "task.add", "name": "b", "duration": { "text": "2j" } },
        { "op": "task.add", "name": "c", "duration": { "text": "2j" } }
    ]));
    ws.call(
        "batch.apply",
        json!({ "ops": [{ "op": "task.indent", "uid": 2 }, { "op": "task.indent", "uid": 3 }] }),
    )
    .unwrap();
    let ops = json!([
        { "op": "progress.update", "task": 2, "percent_complete": { "text": "100%" } },
        { "op": "progress.update", "task": 3, "percent_complete": { "text": "100%" } }
    ]);
    assert_eq!(touched_and_changed(&mut ws, &ops), (2, 0));
    let order_only = json!([{ "op": "task.move", "uid": 4, "to": { "before": 1 } }]);
    assert_eq!(touched_and_changed(&mut ws, &order_only), (0, 0));
}
