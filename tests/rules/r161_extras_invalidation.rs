//! R-161 — Invalidation des `Extras` d'une tâche modifiée (partie tâches) :
//! les éléments MSPDI qui dépendent des dates calculées sont jetés quand ce
//! dont elles dépendent change ; le nom et les notes n'en jettent aucun.
//! Amendée en J5 (lot L7) : `LevelingDelay` et `LevelingDelayFormat` sortent
//! de la table (entrées du modèle, gardées à l'édition) ; une affectation
//! dont les unités, le travail, le délai ou la ressource changent perd sa
//! répartition. Amendée en J6 (lot L6) : l'avancement d'une tâche jette son
//! interruption (`Stop`, `Resume`, `ResumeValid`), le travail réel d'une
//! affectation ses lignes de répartition réelles (types 2 et 3).

use plannercraft_automation::SideEffect;
use plannercraft_core::{
    Assignment, AssignmentUid, DurationUnit, ExtraEntry, LevelingDelay, Minutes, Project, Resource,
    ResourceUid, Subject, TaskUid,
};
use plannercraft_mspdi::write::WriteOptions;
use plannercraft_tests::mspdi::new_project;
use plannercraft_tests::session::{link, push_tasks, session};
use plannercraft_tests::task_edit::{apply, get};
use serde_json::json;

fn entry(key: &str, payload: &str) -> ExtraEntry {
    ExtraEntry {
        origin: "mspdi".to_owned(),
        key: key.to_owned(),
        payload: payload.to_owned(),
    }
}

/// Tâche 1 relue avec `Hyperlink`, `PreLeveledStart` et `PreLeveledFinish`
/// en Extras, une affectation avec sa répartition.
fn project() -> Project {
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1, 2], 2);
    let t = p.tasks.get_mut(TaskUid::new(1)).unwrap();
    t.extras.0.push(entry("Hyperlink", "<Hyperlink>plan</Hyperlink>"));
    t.extras.0.push(entry(
        "PreLeveledStart",
        "<PreLeveledStart>2026-11-02T08:00:00</PreLeveledStart>",
    ));
    t.extras.0.push(entry(
        "PreLeveledFinish",
        "<PreLeveledFinish>2026-11-03T17:00:00</PreLeveledFinish>",
    ));
    let mut a = Assignment::new(AssignmentUid::new(1), TaskUid::new(1), None);
    a.extras.0.push(entry(
        "TimephasedData",
        "<TimephasedData><Type>2</Type><UID>1</UID></TimephasedData>",
    ));
    p.assignments.push(a).unwrap();
    p
}

fn mspdi(s: &plannercraft_automation::Session) -> String {
    String::from_utf8(s.save_bytes(&WriteOptions::default()).xml).unwrap()
}

#[test]
fn r161_duration_drops_leveling_extras() {
    let mut s = session(project());
    assert!(mspdi(&s).contains("<PreLeveledStart>"));
    let o = apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 1, "duration": { "text": "3j" } }]),
    );
    assert!(o.side_effects.contains(&SideEffect::ExtrasDropped {
        subject: Subject::Task(TaskUid::new(1)),
        keys: vec!["PreLeveledFinish".to_owned(), "PreLeveledStart".to_owned()],
    }));
    assert!(o.side_effects.contains(&SideEffect::ExtrasDropped {
        subject: Subject::Assignment(AssignmentUid::new(1)),
        keys: vec!["TimephasedData".to_owned()],
    }));
    let keys: Vec<_> = get(&s, 1).extras.0.iter().map(|e| e.key.as_str()).collect();
    assert_eq!(keys, ["Hyperlink"]);
    let xml = mspdi(&s);
    assert!(!xml.contains("PreLeveledStart"), "{xml}");
    assert!(xml.contains("<Hyperlink>plan</Hyperlink>"));
    // Même valeur : rien à jeter, aucun effet.
    let o = apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 1, "duration": { "text": "3j" } }]),
    );
    assert!(
        o.side_effects
            .iter()
            .all(|e| !matches!(e, SideEffect::ExtrasDropped { .. }))
    );
}

#[test]
fn r161_rename_keeps_extras() {
    let mut s = session(project());
    let o = apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 1, "name": "Autre", "notes": "note" }]),
    );
    assert!(
        o.side_effects
            .iter()
            .all(|e| !matches!(e, SideEffect::ExtrasDropped { .. }))
    );
    assert_eq!(get(&s, 1).extras.0.len(), 3);
    assert_eq!(s.project().assignments.iter().next().unwrap().extras.0.len(), 1);
}

#[test]
fn r161_structure_and_dates_drop_leveling_extras() {
    // Début saisi, jalon, activité, hiérarchie : mêmes clés jetées ; les
    // affectations ne perdent leur répartition que sur un changement de
    // durée.
    for op in [
        json!({ "op": "task.update", "uid": 1, "start": "2026-11-04T08:00" }),
        json!({ "op": "task.update", "uid": 1, "milestone": true }),
        json!({ "op": "task.update", "uid": 1, "active": false }),
        json!({ "op": "task.move", "uid": 1, "to": "end" }),
    ] {
        let mut s = session(project());
        let o = apply(&mut s, json!([op.clone()]));
        let keys: Vec<_> = get(&s, 1).extras.0.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(keys, ["Hyperlink"], "{op}");
        assert!(
            o.side_effects.iter().any(|e| matches!(
                e,
                SideEffect::ExtrasDropped { subject: Subject::Task(u), .. } if u.get() == 1
            )),
            "{op}"
        );
        assert_eq!(
            s.project().assignments.iter().next().unwrap().extras.0.len(),
            1,
            "{op}"
        );
    }
    // Une feuille qui devient récapitulative (ajout d'un enfant) perd aussi
    // ses Extras de planning.
    let mut s = session(project());
    apply(
        &mut s,
        json!([{ "op": "task.add", "name": "Enfant", "at": { "first_child_of": 1 } }]),
    );
    let keys: Vec<_> = get(&s, 1).extras.0.iter().map(|e| e.key.as_str()).collect();
    assert_eq!(keys, ["Hyperlink"]);
}

#[test]
fn r161_unchanged_link_or_place_keeps_extras() {
    // Lien 2 → 1 ; `link.update` sans champ ou avec les mêmes valeurs, et
    // `task.move` qui remet la tâche à sa place : rien ne change, aucune
    // clé jetée, aucun effet, MSPDI réécrit avec `PreLeveledStart`.
    for op in [
        json!({ "op": "link.update", "pred": 2, "succ": 1 }),
        json!({ "op": "link.update", "pred": 2, "succ": 1, "kind": "fs" }),
        json!({ "op": "link.update", "pred": 2, "succ": 1, "kind": "fs", "lag": { "minutes": 0 } }),
        json!({ "op": "task.move", "uid": 1, "to": { "before": 2 } }),
    ] {
        let mut p = project();
        link(&mut p, 2, 1);
        let mut s = session(p);
        let o = apply(&mut s, json!([op.clone()]));
        assert_eq!(get(&s, 1).extras.0.len(), 3, "{op}");
        assert!(
            !o.side_effects
                .iter()
                .any(|e| matches!(e, SideEffect::ExtrasDropped { .. })),
            "{op}"
        );
        assert!(mspdi(&s).contains("<PreLeveledStart>"), "{op}");
    }
    // Le même lien vraiment changé jette les clés (contrôle).
    let mut p = project();
    link(&mut p, 2, 1);
    let mut s = session(p);
    apply(
        &mut s,
        json!([{ "op": "link.update", "pred": 2, "succ": 1, "kind": "ss" }]),
    );
    assert_eq!(get(&s, 1).extras.0.len(), 1);
}

/// Tâche 1 avec un retard de nivellement dans le modèle (1 j écoulé), une
/// tâche 2 avec un `LevelingDelay` hérité en Extras (projet enregistré
/// avant J5), et une ressource affectée à la tâche 1 avec sa répartition.
fn delayed() -> Project {
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1, 2], 2);
    p.tasks.get_mut(TaskUid::new(1)).unwrap().leveling_delay =
        LevelingDelay::new(Minutes::new(1_440), DurationUnit::Day, true);
    let t2 = p.tasks.get_mut(TaskUid::new(2)).unwrap();
    t2.extras
        .0
        .push(entry("LevelingDelay", "<LevelingDelay>4800</LevelingDelay>"));
    t2.extras.0.push(entry(
        "LevelingDelayFormat",
        "<LevelingDelayFormat>7</LevelingDelayFormat>",
    ));
    p.resources
        .push(Resource::new(ResourceUid::new(1), "R".to_owned()))
        .unwrap();
    let mut a = Assignment::new(AssignmentUid::new(1), TaskUid::new(1), Some(ResourceUid::new(1)));
    a.work = plannercraft_core::Work::new(Minutes::new(960)).unwrap();
    a.extras.0.push(entry(
        "TimephasedData",
        "<TimephasedData><Type>1</Type><UID>1</UID></TimephasedData>",
    ));
    p.assignments.push(a).unwrap();
    p
}

#[test]
fn r161_leveling_delay_kept_on_edit() {
    // Durée, contrainte, jalon, activité, liens, hiérarchie : le retard du
    // modèle et le `LevelingDelay` hérité restent ; aucune tâche ne perd
    // d'Extras (aucune autre clé de planning ici).
    for op in [
        json!({ "op": "task.update", "uid": 2, "duration": { "text": "3j" } }),
        json!({ "op": "task.update", "uid": 2, "start": "2026-11-04T08:00" }),
        json!({ "op": "task.update", "uid": 2, "active": false }),
        json!({ "op": "link.add", "pred": 1, "succ": 2 }),
        json!({ "op": "task.indent", "uid": 2 }),
    ] {
        let mut s = session(delayed());
        let o = apply(&mut s, json!([op.clone()]));
        assert!(
            !o.side_effects.iter().any(|e| matches!(
                e,
                SideEffect::ExtrasDropped {
                    subject: Subject::Task(_),
                    ..
                }
            )),
            "{op}"
        );
        assert_eq!(get(&s, 2).extras.0.len(), 2, "{op}");
        assert_eq!(
            get(&s, 1).leveling_delay.map(|d| d.minutes),
            Some(Minutes::new(1_440)),
            "{op}"
        );
        assert!(mspdi(&s).contains("<LevelingDelay>4800</LevelingDelay>"), "{op}");
    }
}

#[test]
fn r161_assignment_change_drops_its_timephased_data() {
    // Unités, travail ou délai de l'affectation changés : sa répartition
    // est jetée, un effet ; le retard de nivellement de la tâche reste.
    for op in [
        json!({ "op": "assignment.update", "task": 1, "resource": 1, "units": { "text": "50%" } }),
        json!({ "op": "assignment.update", "task": 1, "resource": 1, "work": { "text": "8h" } }),
        json!({ "op": "assignment.update", "task": 1, "resource": 1, "delay": { "text": "1j" } }),
    ] {
        let mut s = session(delayed());
        let o = apply(&mut s, json!([op.clone()]));
        assert!(
            o.side_effects.contains(&SideEffect::ExtrasDropped {
                subject: Subject::Assignment(AssignmentUid::new(1)),
                keys: vec!["TimephasedData".to_owned()],
            }),
            "{op}"
        );
        let a = s.project().assignments.get(AssignmentUid::new(1)).unwrap();
        assert!(a.extras.is_empty(), "{op}");
        assert!(get(&s, 1).leveling_delay.is_some(), "{op}");
    }
    // Même délai reposé : rien ne change, rien n'est jeté.
    let mut s = session(delayed());
    let o = apply(
        &mut s,
        json!([{ "op": "assignment.update", "task": 1, "resource": 1, "delay": { "minutes": 0 } }]),
    );
    assert!(
        !o.side_effects
            .iter()
            .any(|e| matches!(e, SideEffect::ExtrasDropped { .. }))
    );
}

#[test]
fn r161_progress_drops_actual_timephased() {
    // R-161 amendée (J6) : le travail réel d'une affectation change
    // (`progress.update`) ⇒ ses lignes `TimephasedData` de travail réel
    // (types 2 et 3) sont jetées, un effet ; la ligne de type 1 (planifiée,
    // R-199 décide à l'écriture) et celle d'une ligne de base restent. Un
    // avancement inchangé ne jette rien.
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1], 2);
    p.resources
        .push(Resource::new(ResourceUid::new(1), "R".to_owned()))
        .unwrap();
    let mut a = Assignment::new(AssignmentUid::new(1), TaskUid::new(1), Some(ResourceUid::new(1)));
    a.work = plannercraft_core::Work::new(Minutes::new(960)).unwrap();
    let row = |t: u8| format!("<TimephasedData><Type>{t}</Type><UID>1</UID></TimephasedData>");
    for t in [1, 2, 3] {
        a.extras.0.push(entry("TimephasedData", &row(t)));
    }
    a.extras.0.push(entry("Baseline#0/TimephasedData", &row(4)));
    p.assignments.push(a).unwrap();
    let mut s = session(p);
    let update = json!([{ "op": "progress.update", "task": 1, "percent_complete": { "text": "50%" } }]);
    let o = apply(&mut s, update.clone());
    assert!(o.side_effects.contains(&SideEffect::ExtrasDropped {
        subject: Subject::Assignment(AssignmentUid::new(1)),
        keys: vec!["TimephasedData".to_owned()],
    }));
    let a = s.project().assignments.get(AssignmentUid::new(1)).unwrap();
    let kept: Vec<&str> = a.extras.0.iter().map(|e| e.payload.as_str()).collect();
    assert_eq!(kept, [row(1).as_str(), row(4).as_str()]);
    assert_eq!(a.actual_work.minutes(), Minutes::new(480));
    let o = apply(&mut s, update);
    assert!(
        !o.side_effects
            .iter()
            .any(|e| matches!(e, SideEffect::ExtrasDropped { .. }))
    );
}
