//! R-072 — `LinkLag` en dixièmes de minute → minutes (import MSPDI).

use plannercraft_core::Minutes;
use plannercraft_mspdi::scalar::tenths_to_minutes;

#[test]
fn r072_tenths_divided_by_ten() {
    // 4 800 dixièmes = 480 min (1 j de 480) ; −4 800 → −480 (avance).
    assert_eq!(tenths_to_minutes(4_800), (Minutes::new(480), false));
    assert_eq!(tenths_to_minutes(-4_800), (Minutes::new(-480), false));
    // 28 800 dixièmes = 2 880 min (2 ej).
    assert_eq!(tenths_to_minutes(28_800), (Minutes::new(2_880), false));
}

#[test]
fn r072_remainder_rounded_by_r001() {
    // 25 dixièmes = 2,5 min → 3 (moitié loin de zéro), arrondi signalé ;
    // −25 → −3 ; 24 → 2.
    assert_eq!(tenths_to_minutes(25), (Minutes::new(3), true));
    assert_eq!(tenths_to_minutes(-25), (Minutes::new(-3), true));
    assert_eq!(tenths_to_minutes(24), (Minutes::new(2), true));
}

/// Liens de la tâche `uid` de la fixture `tasks.xml` : (base, minutes, unité).
fn lags(
    uid: u32,
) -> (
    Vec<(plannercraft_core::TimeBasis, i64, plannercraft_core::DurationUnit)>,
    plannercraft_mspdi::MspdiDocument,
) {
    let bytes = std::fs::read(plannercraft_tests::tests_path("fixtures/mspdi/tasks.xml")).unwrap();
    let doc = plannercraft_mspdi::read(
        &bytes,
        &plannercraft_mspdi::ReadOptions::new(plannercraft_tests::moment("2026-01-01T00:00")),
    )
    .unwrap();
    let out = doc
        .project
        .tasks
        .get(plannercraft_core::TaskUid::new(uid))
        .unwrap()
        .predecessors
        .iter()
        .filter_map(|l| match l.lag.value {
            plannercraft_core::LagValue::Time { minutes, unit } => Some((l.lag.basis, minutes.get(), unit)),
            plannercraft_core::LagValue::Percent { .. } => None,
        })
        .collect();
    (out, doc)
}

#[test]
fn r072_lag_format_gives_basis_and_unit() {
    use plannercraft_core::DurationUnit::Day;
    use plannercraft_core::TimeBasis::{Elapsed, Working};
    // UID 3 : 4 800 + `LagFormat` 7 → +480 ouvré (1 j).
    assert_eq!(lags(3).0, [(Working, 480, Day)]);
    // UID 4 : FF −4 800 + 7 → −480 ; SS 28 800 + 8 → +2 880 écoulé (2 ej).
    assert_eq!(lags(4).0, [(Working, -480, Day), (Elapsed, 2_880, Day)]);
    // UID 5 : SF 25 dixièmes → 3 min, arrondi signalé.
    let (l, doc) = lags(5);
    assert_eq!(l, [(Working, 3, Day)]);
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.code == plannercraft_mspdi::ImportCode::Rounded
                && d.element == "Tasks/Task/PredecessorLink/LinkLag"
                && d.value.as_deref() == Some("25"))
    );
}
