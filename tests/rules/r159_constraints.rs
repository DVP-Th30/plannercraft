//! R-159 — Contraintes par commande : `asap`, `alap` sans date, les six
//! types datés avec ; récapitulative : ASAP, SNET et FNLT seulement ; un
//! conflit avec un lien n'est jamais refusé (avertissement du moteur) ;
//! `task.set_deadline` pose ou retire l'échéance. R-161 : la contrainte
//! changée jette les clés de nivellement de la tâche.

use plannercraft_automation::error::codes;
use plannercraft_automation::view::ViewCtx;
use plannercraft_automation::{ApplyMode, ApplyOutcome, Batch, CommandError, Session, SideEffect};
use plannercraft_core::{Constraint, ExtraEntry, OutlineLevel, Project, Subject, TaskUid};
use plannercraft_tests::moment;
use plannercraft_tests::mspdi::new_project;
use plannercraft_tests::session::{link, push_tasks, session, task};

fn run(s: &mut Session, ops: &str) -> Result<ApplyOutcome, CommandError> {
    let batch = Batch::from_json_str(&format!(r#"{{"format_version":1,"ops":[{ops}]}}"#)).unwrap();
    s.apply(&batch, ApplyMode::Commit)
}

fn constraint(s: &Session, uid: u32) -> Constraint {
    s.project().tasks.get(TaskUid::new(uid)).unwrap().constraint
}

fn one_task() -> Project {
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1], 1);
    p
}

#[test]
fn r159_asap_with_date_refused() {
    let mut s = session(one_task());
    for kind in ["asap", "alap"] {
        let ops =
            format!(r#"{{"op":"task.set_constraint","uid":1,"kind":"{kind}","date":"2026-11-04T08:00"}}"#);
        let err = run(&mut s, &ops).unwrap_err();
        assert_eq!(
            (err.code, err.field.as_deref()),
            (codes::PARAM_INVALID, Some("date")),
            "{kind}"
        );
    }
    run(&mut s, r#"{"op":"task.set_constraint","uid":1,"kind":"alap"}"#).unwrap();
    assert_eq!(constraint(&s, 1), Constraint::AsLateAsPossible);
    run(&mut s, r#"{"op":"task.set_constraint","uid":1,"kind":"asap"}"#).unwrap();
    assert_eq!(constraint(&s, 1), Constraint::AsSoonAsPossible);
}

#[test]
fn r159_dated_without_date_refused() {
    let mut s = session(one_task());
    for kind in ["mso", "mfo", "snet", "snlt", "fnet", "fnlt"] {
        let ops = format!(r#"{{"op":"task.set_constraint","uid":1,"kind":"{kind}"}}"#);
        let err = run(&mut s, &ops).unwrap_err();
        assert_eq!(
            (err.code, err.field.as_deref()),
            (codes::PARAM_INVALID, Some("date")),
            "{kind}"
        );
        let ops =
            format!(r#"{{"op":"task.set_constraint","uid":1,"kind":"{kind}","date":"2026-11-04T08:00"}}"#);
        run(&mut s, &ops).unwrap();
        assert_eq!(
            constraint(&s, 1).date(),
            Some(moment("2026-11-04T08:00")),
            "{kind}"
        );
    }
    // SNET mer 04/11 08:00 : la tâche commence ce jour-là.
    run(
        &mut s,
        r#"{"op":"task.set_constraint","uid":1,"kind":"snet","date":"2026-11-04T08:00"}"#,
    )
    .unwrap();
    assert_eq!(
        s.schedule().scheduled(TaskUid::new(1)).unwrap().start,
        moment("2026-11-04T08:00")
    );
}

#[test]
fn r159_summary_kinds() {
    // 1 S (n1) { 2 A (n2) } : ASAP, SNET, FNLT acceptés ; ALAP, MSO, MFO,
    // SNLT, FNET refusés (`param.invalid`, champ `kind`). Une feuille
    // accepte tout.
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1, 2], 1);
    p.tasks.get_mut(TaskUid::new(2)).unwrap().outline_level = OutlineLevel::new(2).unwrap();
    let mut s = session(p);
    for (kind, ok) in [
        ("asap", true),
        ("snet", true),
        ("fnlt", true),
        ("alap", false),
        ("mso", false),
        ("mfo", false),
        ("snlt", false),
        ("fnet", false),
    ] {
        let date = if matches!(kind, "asap" | "alap") {
            String::new()
        } else {
            r#","date":"2026-11-20T17:00""#.to_owned()
        };
        for uid in [1, 2] {
            let ops = format!(r#"{{"op":"task.set_constraint","uid":{uid},"kind":"{kind}"{date}}}"#);
            let r = run(&mut s, &ops);
            if ok || uid == 2 {
                r.unwrap();
            } else {
                let err = r.unwrap_err();
                assert_eq!(
                    (err.code, err.field.as_deref()),
                    (codes::PARAM_INVALID, Some("kind")),
                    "{kind}"
                );
                assert_eq!(err.subject, Some(Subject::Task(TaskUid::new(1))));
            }
        }
    }
}

#[test]
fn r159_summary_kinds_null_row_ignored() {
    // 1 S (n1) [2 vide, n1] 3 A (n2) : la ligne vide ne compte pas (R-052),
    // S reste récapitulative. MSO refusé (`kind`), lien S → A refusé en
    // `link.summary_descendant`, vue `summary: true`, filtre cohérent.
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1, 2, 3], 1);
    p.tasks.get_mut(TaskUid::new(2)).unwrap().is_null = true;
    p.tasks.get_mut(TaskUid::new(3)).unwrap().outline_level = OutlineLevel::new(2).unwrap();
    let mut s = session(p);
    let err = run(
        &mut s,
        r#"{"op":"task.set_constraint","uid":1,"kind":"mso","date":"2026-11-20T17:00"}"#,
    )
    .unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref()),
        (codes::PARAM_INVALID, Some("kind"))
    );
    let err = run(&mut s, r#"{"op":"link.add","pred":1,"succ":3}"#).unwrap_err();
    assert_eq!(err.code, codes::LINK_SUMMARY_DESCENDANT);
    let ctx = ViewCtx::new(&s);
    assert!(ctx.task(TaskUid::new(1)).unwrap().summary);
    assert!(!ctx.task(TaskUid::new(3)).unwrap().summary);
}

#[test]
fn r159_override_warning_not_refusal() {
    // Cahier : 3 (9 j) finit le jeu 12/11 17:00, 3 → 4 ; MSO mar
    // 10/11 08:00 sur 4 → 4 commence le 10/11 08:00, `constraint.override`
    // en avertissement, lot accepté.
    let mut p = new_project("2026-11-02T08:00");
    push_tasks(&mut p, &[1, 2], 1);
    let mut t3 = task(&p, 3, 9);
    t3.name = "Longue".to_owned();
    p.tasks.push(t3).unwrap();
    push_tasks(&mut p, &[4], 1);
    link(&mut p, 3, 4);
    let mut s = session(p);
    assert_eq!(
        s.schedule().scheduled(TaskUid::new(3)).unwrap().finish,
        moment("2026-11-12T17:00")
    );
    let o = run(
        &mut s,
        r#"{"op":"task.set_constraint","uid":4,"kind":"mso","date":"2026-11-10T08:00"}"#,
    )
    .unwrap();
    assert!(o.committed);
    assert_eq!(
        s.schedule().scheduled(TaskUid::new(4)).unwrap().start,
        moment("2026-11-10T08:00")
    );
    assert!(
        o.warnings.iter().any(|d| d.code() == "constraint.override"),
        "{:?}",
        o.warnings
    );
}

#[test]
fn r159_deadline_patch() {
    let mut s = session(one_task());
    let deadline = |s: &Session| s.project().tasks.get(TaskUid::new(1)).unwrap().deadline;
    let o = run(
        &mut s,
        r#"{"op":"task.set_deadline","uid":1,"deadline":"2026-11-01T17:00"}"#,
    )
    .unwrap();
    assert_eq!(deadline(&s), Some(moment("2026-11-01T17:00")));
    assert!(o.warnings.iter().any(|d| d.code() == "task.deadline_missed"));
    run(&mut s, r#"{"op":"task.set_deadline","uid":1}"#).unwrap();
    assert_eq!(deadline(&s), Some(moment("2026-11-01T17:00")));
    run(&mut s, r#"{"op":"task.set_deadline","uid":1,"deadline":null}"#).unwrap();
    assert_eq!(deadline(&s), None);
}

#[test]
fn r161_constraint_drops_leveling_extras() {
    // R-161 : contrainte changée → `PreLeveledStart` jeté ; même contrainte
    // reposée → rien.
    let mut p = one_task();
    p.tasks.get_mut(TaskUid::new(1)).unwrap().extras.0 = vec![ExtraEntry {
        origin: "mspdi".to_owned(),
        key: "PreLeveledStart".to_owned(),
        payload: "<PreLeveledStart>2026-11-02T08:00:00</PreLeveledStart>".to_owned(),
    }];
    let mut s = session(p);
    let ops = r#"{"op":"task.set_constraint","uid":1,"kind":"snet","date":"2026-11-03T08:00"}"#;
    let o = run(&mut s, ops).unwrap();
    assert_eq!(
        o.side_effects,
        vec![SideEffect::ExtrasDropped {
            subject: Subject::Task(TaskUid::new(1)),
            keys: vec!["PreLeveledStart".to_owned()],
        }]
    );
    assert!(s.project().tasks.get(TaskUid::new(1)).unwrap().extras.is_empty());
}
