//! R-163 — Diff de planning d'un lot (définition, tri, N).

use plannercraft_automation::{ApplyMode, ApplyOptions};
use plannercraft_core::{TaskId, TaskUid};
use plannercraft_tests::moment;
use plannercraft_tests::mspdi::new_project;
use plannercraft_tests::session::{add_task, commit, delete_task, link, push_tasks, session, set_days};

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
