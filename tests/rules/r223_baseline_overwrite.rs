//! R-223 — Ligne de base : remplacement. Une entité de la portée déjà dotée
//! de la ligne n ⇒ `Exists { number, count }` sauf `overwrite` ; remplacée :
//! coût et coût fixe de l'ancienne ligne recopiés tels quels (PlannerCraft ne
//! calcule pas les coûts : `cost_copied`, d'où `baseline.cost_not_computed` ;
//! arbitrage du 2026-10-20 de l'ADR-0014, qui remplace l'effacement du
//! cahier), `Extras` MSPDI de portée `Baseline#n/` de l'entité jetés
//! (extension de R-161). Source : S58 (« When prompted to overwrite the
//! existing saved data, click Yes. ») ; reste : hypothèse.

use plannercraft_core::baseline::{self, BaselineError, Rollup, Scope};
use plannercraft_core::{
    AssignmentUid, ExtraEntry, Money, ResourceBaseline, ResourceUid, Subject, TaskBaseline,
};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

use crate::r171_assignment_effective_calendar::{assign, resource};
use crate::r220_baseline_values::{number, plan, set, spec};

/// T 1 j affectée à R1 (affectation 1).
fn example() -> Net {
    assign(
        resource(Net::new("2026-10-19T08:00").task("T", 480), 1, None),
        1,
        "T",
        1,
        480,
    )
}

fn mspdi(key: &str) -> ExtraEntry {
    ExtraEntry {
        origin: "mspdi".to_owned(),
        key: key.to_owned(),
        payload: format!("<{key}/>"),
    }
}

#[test]
fn r223_exists_refused() {
    // Ligne 0 posée sur T, l'affectation 1 et R1 ⇒ nouvelle capture refusée
    // (3 entités), acceptée sur une autre ligne.
    let (mut p, s) = plan(example());
    let _ = set(&mut p, &s, &spec(0, Scope::Project, Rollup::None, false));
    assert_eq!(
        baseline::capture(&p, &s, &spec(0, Scope::Project, Rollup::None, false)),
        Err(BaselineError::Exists {
            number: number(0),
            count: 3
        })
    );
    assert!(baseline::capture(&p, &s, &spec(1, Scope::Project, Rollup::None, false)).is_ok());
    assert!(baseline::capture(&p, &s, &spec(0, Scope::Project, Rollup::None, true)).is_ok());
}

#[test]
fn r223_overwrite_copies_cost() {
    // Ligne 0 relue d'un fichier : T au 12/10, coût 8 000,00, coût fixe
    // 50,00 ; R1 coût 1 000,00. Remplacée : dates du moteur, coûts recopiés.
    let net = example();
    let t = net.uid("T");
    let (mut p, s) = plan(net);
    let old = TaskBaseline {
        start: Some(moment("2026-10-12T08:00")),
        finish: Some(moment("2026-10-12T17:00")),
        duration: None,
        work: None,
        cost: Some(Money::new(800_000).unwrap()),
        fixed_cost: Some(Money::new(5_000).unwrap()),
    };
    p.tasks.get_mut(t).unwrap().baselines.insert(number(0), old);
    p.resources
        .get_mut(ResourceUid::new(1))
        .unwrap()
        .baselines
        .insert(
            number(0),
            ResourceBaseline {
                work: None,
                cost: Some(Money::new(100_000).unwrap()),
            },
        );
    let edit = set(&mut p, &s, &spec(0, Scope::Project, Rollup::None, true));
    assert!(edit.cost_copied);
    let line = p
        .tasks
        .get(t)
        .unwrap()
        .baselines
        .get(&number(0))
        .copied()
        .unwrap();
    assert_eq!(line.start, Some(moment("2026-10-19T08:00")));
    assert_eq!(line.finish, Some(moment("2026-10-19T17:00")));
    assert_eq!(line.cost, old.cost);
    assert_eq!(line.fixed_cost, old.fixed_cost);
    let res = p
        .resources
        .get(ResourceUid::new(1))
        .unwrap()
        .baselines
        .get(&number(0))
        .copied()
        .unwrap();
    assert_eq!(res.cost, Some(Money::new(100_000).unwrap()));
    // L'affectation n'avait pas de ligne : pas de coût.
    let asg = p
        .assignments
        .get(AssignmentUid::new(1))
        .unwrap()
        .baselines
        .get(&number(0))
        .copied()
        .unwrap();
    assert_eq!(asg.cost, None);
    // Nouvelle ligne sans ancien coût : rien à recopier.
    let fresh = baseline::capture(&p, &s, &spec(4, Scope::Project, Rollup::None, false)).unwrap();
    assert!(!fresh.cost_copied);
}

#[test]
fn r223_overwrite_drops_scoped_extras() {
    // T a une ligne 0 et des Extras `Baseline#0/…`, `Baseline#10/…`,
    // `Baseline#1/…` et une clé hors ligne : remplacer la ligne 0 ne jette
    // que les clés `Baseline#0/`, triées.
    let net = example();
    let t = net.uid("T");
    let (mut p, s) = plan(net);
    {
        let task = p.tasks.get_mut(t).unwrap();
        task.baselines.insert(number(0), TaskBaseline::default());
        task.extras.0.extend([
            mspdi("Baseline#0/Number>Zeta"),
            mspdi("Baseline#10/Interim"),
            mspdi("Baseline#0/Alpha"),
            mspdi("Baseline#1/Alpha"),
            mspdi("Hyperlink"),
        ]);
    }
    let edit = baseline::capture(&p, &s, &spec(0, Scope::Tasks(vec![t]), Rollup::None, true)).unwrap();
    let dropped = baseline::apply(&mut p, number(0), edit);
    assert_eq!(
        dropped,
        vec![(
            Subject::Task(t),
            vec!["Baseline#0/Alpha".to_owned(), "Baseline#0/Number>Zeta".to_owned()]
        )]
    );
    let keys: Vec<&str> = p
        .tasks
        .get(t)
        .unwrap()
        .extras
        .0
        .iter()
        .map(|e| e.key.as_str())
        .collect();
    assert_eq!(keys, vec!["Baseline#10/Interim", "Baseline#1/Alpha", "Hyperlink"]);
    // Une ligne neuve ne jette rien.
    let edit = baseline::capture(&p, &s, &spec(1, Scope::Tasks(vec![t]), Rollup::None, false)).unwrap();
    assert_eq!(baseline::apply(&mut p, number(1), edit), Vec::new());
}
