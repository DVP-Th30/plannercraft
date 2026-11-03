//! R-232 — Chaîne des causes : depuis la tâche, `Link` → prédécesseur,
//! `SummaryGate` → récapitulative (puis le lien entrant de la
//! récapitulative dont la borne est celle de la porte), `Rollup` → sous-tâche
//! au début minimal (la première dans l'ordre des ID) ; arrêt sur
//! `ProjectStart`, `Constraint`, `Manual`, `Actual` ; au plus `depth`
//! maillons (5 par défaut, 1 à 50, sinon `DepthOutOfRange` →
//! `param.invalid`), `truncated` si la chaîne est coupée ; garde de
//! répétition (règle 10). Chaque maillon recopie le `Schedule`. Hypothèse.

#![allow(
    clippy::many_single_char_names,
    clippy::too_many_lines,
    reason = "tests : noms des tâches des exemples (A, B, C…)"
)]

use plannercraft_core::schedule::explain::{ExplainError, Side, explain};
use plannercraft_core::{Driver, LinkType};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

use crate::r230_explain_start::{abc, plan};

#[test]
fn r232_chain_depth_and_truncated() {
    // C ← B ← début du projet (lun 08:00).
    let (p, s, [b, c]) = plan(abc(), ["B", "C"]);
    let x = explain(&p, &s, c, Side::Start, 5).unwrap();
    assert_eq!(x.chain.len(), 1);
    let link = x.chain[0];
    assert_eq!(link.uid, b);
    assert_eq!(link.start, moment("2026-10-19T08:00"));
    assert_eq!(link.finish, moment("2026-10-20T17:00"));
    assert_eq!(link.driver, Driver::ProjectStart);
    assert_eq!(link.raw, Some(moment("2026-10-19T08:00")));
    assert!(!x.truncated);
    // A → B → C → D (FS) : D ← C ← B ← A ; coupée à 2.
    let (p, s, [a, b, c, d]) = plan(
        Net::new("2026-10-19T08:00")
            .task("A", 480)
            .task("B", 480)
            .task("C", 480)
            .task("D", 480)
            .fs("A", "B")
            .fs("B", "C")
            .fs("C", "D"),
        ["A", "B", "C", "D"],
    );
    let x = explain(&p, &s, d, Side::Start, 5).unwrap();
    let uids: Vec<_> = x.chain.iter().map(|l| l.uid).collect();
    assert_eq!(uids, vec![c, b, a]);
    assert!(!x.truncated);
    for l in &x.chain {
        let ts = s.scheduled(l.uid).unwrap();
        assert_eq!(
            (l.start, l.finish, l.driver, l.raw),
            (ts.start, ts.finish, ts.driver, ts.driver_bound)
        );
    }
    let x = explain(&p, &s, d, Side::Start, 2).unwrap();
    assert_eq!(x.chain.iter().map(|l| l.uid).collect::<Vec<_>>(), vec![c, b]);
    assert!(x.truncated);
    // Exactement la longueur de la chaîne : pas coupée.
    assert!(!explain(&p, &s, d, Side::Start, 3).unwrap().truncated);
    // Porte : F sous S, S FS de A ⇒ F ← S ← A.
    let (p, s, [a, sm, f]) = plan(
        Net::new("2026-10-19T08:00")
            .task("A", 480)
            .task("S", 0)
            .task("F", 480)
            .level("F", 2)
            .fs("A", "S"),
        ["A", "S", "F"],
    );
    let x = explain(&p, &s, f, Side::Start, 5).unwrap();
    assert_eq!(x.chain.iter().map(|l| l.uid).collect::<Vec<_>>(), vec![sm, a]);
    assert_eq!(x.chain[0].driver, Driver::Rollup);
    // Agrégat : S ← F (sous-tâche au début minimal) ; F renvoie à S, déjà
    // vue : arrêt.
    let x = explain(&p, &s, sm, Side::Start, 5).unwrap();
    assert_eq!(x.chain.iter().map(|l| l.uid).collect::<Vec<_>>(), vec![f]);
    assert!(!x.truncated);
    // Lien depuis une récapitulative : B FS de S ⇒ B ← S ← F.
    let (p, s, [sm, f, b]) = plan(
        Net::new("2026-10-19T08:00")
            .task("S", 0)
            .task("F", 480)
            .level("F", 2)
            .task("B", 480)
            .link("S", "B", LinkType::FinishToStart, plannercraft_core::Lag::ZERO),
        ["S", "F", "B"],
    );
    let x = explain(&p, &s, b, Side::Start, 5).unwrap();
    assert_eq!(x.chain.iter().map(|l| l.uid).collect::<Vec<_>>(), vec![sm, f]);
}

#[test]
fn r232_depth_out_of_range() {
    let (p, s, [c]) = plan(abc(), ["C"]);
    assert_eq!(
        explain(&p, &s, c, Side::Start, 0),
        Err(ExplainError::DepthOutOfRange(0))
    );
    assert_eq!(
        explain(&p, &s, c, Side::Start, 51),
        Err(ExplainError::DepthOutOfRange(51))
    );
    assert!(explain(&p, &s, c, Side::Start, 1).is_ok());
    assert!(explain(&p, &s, c, Side::Start, 50).is_ok());
}
