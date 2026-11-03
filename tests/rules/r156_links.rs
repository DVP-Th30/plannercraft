//! R-156 — Liens par commande : `link.add` (FS et décalage nul par défaut ;
//! auto-lien, doublon, récapitulative ↔ descendante refusés ; cycle refusé
//! par R-160), `link.update` (champs absents gardés), `link.remove` ; lien
//! absent → `link.not_found`. R-161 (liens) : les clés de nivellement du
//! successeur sont jetées.

use plannercraft_automation::error::codes;
use plannercraft_automation::{
    ApplyMode, ApplyOutcome, Batch, CommandError, CommandErrorKind, Session, SideEffect,
};
use plannercraft_core::{ExtraEntry, LinkType, OutlineLevel, Project, Subject, TaskUid};
use plannercraft_tests::moment;
use plannercraft_tests::mspdi::new_project;
use plannercraft_tests::session::{push_tasks, session};

fn run(s: &mut Session, ops: &str) -> Result<ApplyOutcome, CommandError> {
    let batch = Batch::from_json_str(&format!(r#"{{"format_version":1,"ops":[{ops}]}}"#)).unwrap();
    s.apply(&batch, ApplyMode::Commit)
}

/// Tâches 1 et 2 de `days` jours, depuis le lun 02/11/2026 08:00.
fn two(days: i64) -> Project {
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1, 2], days);
    p
}

fn start(s: &Session, uid: u32) -> plannercraft_core::Moment {
    s.schedule().scheduled(TaskUid::new(uid)).unwrap().start
}

#[test]
fn r156_self_link_refused() {
    let mut s = session(two(1));
    let before = s.project().clone();
    let err = run(&mut s, r#"{"op":"link.add","pred":1,"succ":1}"#).unwrap_err();
    assert_eq!(err.code, codes::LINK_SELF);
    assert_eq!(err.kind, CommandErrorKind::SelfLink);
    assert_eq!(err.op_index, Some(0));
    assert_eq!(s.project(), &before);
}

#[test]
fn r156_duplicate_refused() {
    // Cahier : add 1→2 FS +2j, puis add 1→2 SS → `link.duplicate`.
    let mut s = session(two(1));
    run(
        &mut s,
        r#"{"op":"link.add","pred":1,"succ":2,"lag":{"text":"2j"}}"#,
    )
    .unwrap();
    assert_eq!(start(&s, 2), moment("2026-11-05T08:00"));
    let err = run(&mut s, r#"{"op":"link.add","pred":1,"succ":2,"kind":"ss"}"#).unwrap_err();
    assert_eq!(err.code, codes::LINK_DUPLICATE);
    assert_eq!(
        err.subject,
        Some(Subject::Link {
            predecessor: TaskUid::new(1),
            successor: TaskUid::new(2)
        })
    );
    // Même refus au sein d'un seul lot, rien appliqué.
    let mut s = session(two(1));
    let err = run(
        &mut s,
        r#"{"op":"link.add","pred":1,"succ":2},{"op":"link.add","pred":1,"succ":2}"#,
    )
    .unwrap_err();
    assert_eq!((err.code, err.op_index), (codes::LINK_DUPLICATE, Some(1)));
    assert_eq!(s.project().tasks.get(TaskUid::new(2)).unwrap().predecessors, []);
}

#[test]
fn r156_summary_descendant_refused() {
    // 1 S (n1), 2 A (n2), 3 B (n1) : S ↔ A refusé dans les deux sens ;
    // S → B et A → B acceptés.
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1, 2, 3], 1);
    p.tasks.get_mut(TaskUid::new(2)).unwrap().outline_level = OutlineLevel::new(2).unwrap();
    let mut s = session(p);
    for ops in [
        r#"{"op":"link.add","pred":1,"succ":2}"#,
        r#"{"op":"link.add","pred":2,"succ":1}"#,
    ] {
        let err = run(&mut s, ops).unwrap_err();
        assert_eq!(err.code, codes::LINK_SUMMARY_DESCENDANT, "{ops}");
        assert_eq!(err.kind, CommandErrorKind::SummaryDescendantLink);
    }
    run(
        &mut s,
        r#"{"op":"link.add","pred":2,"succ":3},{"op":"link.add","pred":1,"succ":3,"kind":"ss"}"#,
    )
    .unwrap();
}

#[test]
fn r156_cycle_refused_by_delta() {
    // 1 → 2 → 3 ; ajouter 3 → 1 crée un cycle : refus R-160, op_index du
    // lien.
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1, 2, 3], 1);
    let mut s = session(p);
    run(
        &mut s,
        r#"{"op":"link.add","pred":1,"succ":2},{"op":"link.add","pred":2,"succ":3}"#,
    )
    .unwrap();
    let before = s.project().clone();
    let err = run(
        &mut s,
        r#"{"op":"task.set_deadline","uid":2,"deadline":"2026-12-01T17:00"},{"op":"link.add","pred":3,"succ":1}"#,
    )
    .unwrap_err();
    assert_eq!(err.code, codes::BATCH_WOULD_CREATE_ERRORS);
    assert_eq!(err.op_index, Some(1));
    let CommandErrorKind::WouldCreateErrors(errors) = &err.kind else {
        panic!("{err:?}");
    };
    assert!(errors.iter().any(|d| d.code() == "link.cycle"), "{errors:?}");
    assert_eq!(s.project(), &before);
}

#[test]
fn r156_update_keeps_absent_fields() {
    // Cahier : 1 → 2 FS +2j ; update kind SS → 2 démarre au début de 1 +
    // 2 j, le décalage est gardé.
    let mut s = session(two(5));
    run(
        &mut s,
        r#"{"op":"link.add","pred":1,"succ":2,"lag":{"text":"2j"}}"#,
    )
    .unwrap();
    run(&mut s, r#"{"op":"link.update","pred":1,"succ":2,"kind":"ss"}"#).unwrap();
    let link = s.project().tasks.get(TaskUid::new(2)).unwrap().predecessors[0];
    assert_eq!(link.kind, LinkType::StartToStart);
    assert_eq!(
        link.lag.value,
        plannercraft_core::LagValue::Time {
            minutes: plannercraft_core::Minutes::new(960),
            unit: plannercraft_core::DurationUnit::Day
        }
    );
    assert_eq!(start(&s, 2), moment("2026-11-04T08:00"));
    // Décalage seul : le type SS est gardé.
    run(
        &mut s,
        r#"{"op":"link.update","pred":1,"succ":2,"lag":{"minutes":0}}"#,
    )
    .unwrap();
    let link = s.project().tasks.get(TaskUid::new(2)).unwrap().predecessors[0];
    assert_eq!(link.kind, LinkType::StartToStart);
    assert_eq!(start(&s, 2), moment("2026-11-02T08:00"));
    // Paire absente → `link.not_found` (update et remove).
    for ops in [
        r#"{"op":"link.update","pred":2,"succ":1,"kind":"ff"}"#,
        r#"{"op":"link.remove","pred":2,"succ":1}"#,
    ] {
        assert_eq!(run(&mut s, ops).unwrap_err().code, codes::LINK_NOT_FOUND, "{ops}");
    }
    // Retrait : 2 revient au début du projet.
    run(&mut s, r#"{"op":"link.remove","pred":1,"succ":2}"#).unwrap();
    assert_eq!(s.project().tasks.get(TaskUid::new(2)).unwrap().predecessors, []);
}

#[test]
fn r156_percent_lag() {
    // A 4 j → B FS +50 % : 50 % de la durée de A = 2 j ; A finit le jeu
    // 05/11 17:00, B commence le mar 10/11 08:00.
    let mut s = session(two(4));
    run(
        &mut s,
        r#"{"op":"link.add","pred":1,"succ":2,"lag":{"percent":5000}}"#,
    )
    .unwrap();
    assert_eq!(start(&s, 2), moment("2026-11-10T08:00"));
    // Même décalage par le texte.
    let mut t = session(two(4));
    run(
        &mut t,
        r#"{"op":"link.add","pred":1,"succ":2,"lag":{"text":"50%"}}"#,
    )
    .unwrap();
    assert_eq!(t.project(), s.project());
}

#[test]
fn r156_unknown_tasks_refused() {
    // UID inconnu → `task.not_found`, étiquette inconnue →
    // `batch.unknown_label`, sur le champ fautif.
    let mut s = session(two(1));
    let err = run(&mut s, r#"{"op":"link.add","pred":1,"succ":9}"#).unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref()),
        (codes::TASK_NOT_FOUND, Some("succ"))
    );
    let err = run(&mut s, r#"{"op":"link.remove","pred":"$x","succ":2}"#).unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref()),
        (codes::BATCH_UNKNOWN_LABEL, Some("pred"))
    );
}

#[test]
fn r161_link_change_drops_leveling_extras() {
    // R-161 (liens) : `PreLeveledStart` du successeur jeté, effet signalé ;
    // une clé sans rapport (`Hyperlink`) est gardée, `LevelingDelay` aussi
    // (R-161 amendée en J5 : entrée du modèle).
    let mut p = two(1);
    let extra = |key: &str| ExtraEntry {
        origin: "mspdi".to_owned(),
        key: key.to_owned(),
        payload: format!("<{key}>4800</{key}>"),
    };
    p.tasks.get_mut(TaskUid::new(2)).unwrap().extras.0 = vec![
        extra("Hyperlink"),
        extra("LevelingDelay"),
        extra("PreLeveledStart"),
    ];
    let mut s = session(p);
    let o = run(&mut s, r#"{"op":"link.add","pred":1,"succ":2}"#).unwrap();
    assert!(o.side_effects.contains(&SideEffect::ExtrasDropped {
        subject: Subject::Task(TaskUid::new(2)),
        keys: vec!["PreLeveledStart".to_owned()],
    }));
    let keys: Vec<&str> = s
        .project()
        .tasks
        .get(TaskUid::new(2))
        .unwrap()
        .extras
        .0
        .iter()
        .map(|e| e.key.as_str())
        .collect();
    assert_eq!(keys, ["Hyperlink", "LevelingDelay"]);
    // Plus rien à jeter : aucun effet.
    let o = run(&mut s, r#"{"op":"link.update","pred":1,"succ":2,"kind":"ss"}"#).unwrap();
    assert!(
        !o.side_effects
            .iter()
            .any(|e| matches!(e, SideEffect::ExtrasDropped { .. }))
    );
}
