//! R-222 — Ligne de base : cumul vers les récapitulatives. `None` ;
//! `ToAllSummaries` : chaque ancêtre d'une tâche capturée (UID 0 compris)
//! reçoit la ligne n recalculée sur les lignes n de ses enfants (début min,
//! fin max, travail Σ, durée = temps ouvré sur son échelle de mesure) ;
//! `FromSubtasks` : seules les récapitulatives de la sélection ; enfants
//! sans ligne n ignorés ; cumul sans sélection ⇒ `RollupNeedsSelection`.
//! Les ancêtres cumulés ne comptent pas pour le refus de R-223. Source :
//! S58 (« Roll up baselines » : « To all summary tasks », « From subtasks
//! into selected summary task(s) ») ; sémantique : hypothèse.

use plannercraft_core::TaskUid;
use plannercraft_core::baseline::{self, BaselineError, Rollup, Scope};
use plannercraft_tests::net::Net;

use crate::r220_baseline_values::{line, number, plan, set, spec, with_root};

/// S {A 2 j lun-mar, B 2 j FS mer-jeu} sous l'UID 0.
fn example() -> Net {
    with_root(
        Net::new("2026-10-19T08:00")
            .task("S", 0)
            .task("A", 960)
            .level("A", 2)
            .task("B", 960)
            .level("B", 2)
            .fs("A", "B"),
    )
}

#[test]
fn r222_to_all_summaries() {
    // Ligne 1 de A seule ⇒ S { lun 08:00, mar 17:00, 960 } (B sans ligne,
    // ignorée), UID 0 de même ; puis ligne 1 de B ⇒ S { lun, jeu 17:00,
    // 1 920 }, sans refus bien que S ait déjà la ligne 1.
    let net = example();
    let (s_uid, a, b) = (net.uid("S"), net.uid("A"), net.uid("B"));
    let (mut p, s) = plan(net);
    let first = set(
        &mut p,
        &s,
        &spec(1, Scope::Tasks(vec![a]), Rollup::ToAllSummaries, false),
    );
    let partial = line("2026-10-19T08:00", "2026-10-20T17:00", 960, 0);
    assert_eq!(first.tasks.get(&s_uid), Some(&partial));
    assert_eq!(first.tasks.get(&TaskUid::PROJECT_SUMMARY), Some(&partial));
    assert_eq!(first.tasks.get(&b), None);
    let second = set(
        &mut p,
        &s,
        &spec(1, Scope::Tasks(vec![b]), Rollup::ToAllSummaries, false),
    );
    let full = line("2026-10-19T08:00", "2026-10-22T17:00", 1_920, 0);
    assert_eq!(second.tasks.get(&s_uid), Some(&full));
    assert_eq!(p.tasks.get(s_uid).unwrap().baselines.get(&number(1)), Some(&full));
    assert_eq!(
        p.summary_task.as_ref().unwrap().baselines.get(&number(1)),
        Some(&full)
    );
    // Sans cumul, S reste sans ligne 2.
    let none = set(&mut p, &s, &spec(2, Scope::Tasks(vec![a]), Rollup::None, false));
    assert_eq!(none.tasks.keys().copied().collect::<Vec<_>>(), vec![a]);
}

#[test]
fn r222_from_subtasks() {
    // A a une ligne 1, B non : S sélectionnée reçoit le cumul de A seule ;
    // A sélectionnée seule ne touche pas S.
    let net = example();
    let (s_uid, a) = (net.uid("S"), net.uid("A"));
    let (mut p, s) = plan(net);
    let only_a = set(
        &mut p,
        &s,
        &spec(1, Scope::Tasks(vec![a]), Rollup::FromSubtasks, false),
    );
    assert_eq!(only_a.tasks.keys().copied().collect::<Vec<_>>(), vec![a]);
    let edit = set(
        &mut p,
        &s,
        &spec(1, Scope::Tasks(vec![s_uid]), Rollup::FromSubtasks, false),
    );
    assert_eq!(
        edit.tasks.get(&s_uid),
        Some(&line("2026-10-19T08:00", "2026-10-20T17:00", 960, 0))
    );
    assert_eq!(edit.tasks.get(&TaskUid::PROJECT_SUMMARY), None);
    // Aucun enfant avec la ligne 3 : la récapitulative garde la capture
    // directe du moteur.
    let edit = baseline::capture(
        &p,
        &s,
        &spec(3, Scope::Tasks(vec![s_uid]), Rollup::FromSubtasks, false),
    )
    .unwrap();
    assert_eq!(
        edit.tasks.get(&s_uid),
        Some(&line("2026-10-19T08:00", "2026-10-22T17:00", 1_920, 0))
    );
}

#[test]
fn r222_needs_selection() {
    let (p, s) = plan(example());
    for rollup in [Rollup::ToAllSummaries, Rollup::FromSubtasks] {
        assert_eq!(
            baseline::capture(&p, &s, &spec(0, Scope::Project, rollup, false)),
            Err(BaselineError::RollupNeedsSelection)
        );
        assert_eq!(
            baseline::capture(&p, &s, &spec(0, Scope::Tasks(Vec::new()), rollup, false)),
            Err(BaselineError::RollupNeedsSelection)
        );
    }
    // Sélection vide sans cumul : capture vide.
    let edit = baseline::capture(&p, &s, &spec(0, Scope::Tasks(Vec::new()), Rollup::None, false)).unwrap();
    assert!(edit.tasks.is_empty() && edit.skipped.is_empty());
}
