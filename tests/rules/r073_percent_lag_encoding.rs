//! R-073 — Décalage en pourcentage : `LinkLag` = pourcentage × 10 (hypothèse
//! pure, import MSPDI).

use plannercraft_mspdi::scalar::percent_lag_from_link_lag;

#[test]
fn r073_percent_lag_is_link_lag_times_ten() {
    // 500 → 5 000 en 1/10 000 = 50 % ; −250 → −2 500 = −25 %.
    assert_eq!(percent_lag_from_link_lag(500), Some(5_000));
    assert_eq!(percent_lag_from_link_lag(-250), Some(-2_500));
    assert_eq!(percent_lag_from_link_lag(0), Some(0));
    // Débordement : aucune valeur (l'import signale et applique 0).
    assert_eq!(percent_lag_from_link_lag(i64::MAX), None);
}

#[test]
fn r073_percent_lag_import() {
    use plannercraft_core::{LagValue, SignedPercent, TaskUid, TimeBasis};
    let bytes = std::fs::read(plannercraft_tests::tests_path("fixtures/mspdi/tasks.xml")).unwrap();
    let doc = plannercraft_mspdi::read(
        &bytes,
        &plannercraft_mspdi::ReadOptions::new(plannercraft_tests::moment("2026-01-01T00:00")),
    )
    .unwrap();
    let percent = |uid: u32| {
        doc.project
            .tasks
            .get(TaskUid::new(uid))
            .unwrap()
            .predecessors
            .iter()
            .find_map(|l| match l.lag.value {
                LagValue::Percent { percent } => Some((l.lag.basis, percent)),
                LagValue::Time { .. } => None,
            })
            .unwrap()
    };
    // UID 5 : 500 + `LagFormat` 19 → 50 % ouvré ; UID 7 : −250 + 20 → −25 %
    // écoulé.
    assert_eq!(
        percent(5),
        (TimeBasis::Working, SignedPercent::new(5_000).unwrap())
    );
    assert_eq!(
        percent(7),
        (TimeBasis::Elapsed, SignedPercent::new(-2_500).unwrap())
    );
    // Signalé à chaque occurrence (hypothèse pure).
    let unsupported = doc
        .diagnostics
        .iter()
        .filter(|d| {
            d.code
                == plannercraft_mspdi::ImportCode::Unsupported {
                    what: "percent_lag_encoding".to_owned(),
                }
        })
        .count();
    assert_eq!(unsupported, 2);
}
