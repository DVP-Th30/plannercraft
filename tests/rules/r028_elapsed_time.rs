//! R-028 — Temps écoulé : temps civil pur, 24 h sur 24 et 7 j sur 7, sans
//! calendrier, sans exception, sans recalage du début.

use plannercraft_core::{ConversionFactors, Decimal, DurationUnit, Minutes, Moment, TimeBasis};
use plannercraft_tests::moment;

fn elapsed(n: i64, unit: DurationUnit) -> Minutes {
    ConversionFactors::default()
        .to_minutes(Decimal::integer(n), unit, TimeBasis::Elapsed)
        .unwrap()
}

fn plus(from: &str, d: Minutes) -> Moment {
    moment(from).checked_add(d).unwrap()
}

#[test]
fn r028_elapsed_days_and_weeks() {
    // Ven 23/10 17:00 + 2 ej (2 × 1 440) → dim 25/10 17:00, week-end compris.
    assert_eq!(
        plus("2026-10-23T17:00", elapsed(2, DurationUnit::Day)),
        moment("2026-10-25T17:00")
    );
    // + 1 esem (10 080) → ven 30/10 17:00.
    assert_eq!(
        plus("2026-10-23T17:00", elapsed(1, DurationUnit::Week)),
        moment("2026-10-30T17:00")
    );
    // + 3 ej → lun 26/10 17:00.
    assert_eq!(
        plus("2026-10-23T17:00", elapsed(3, DurationUnit::Day)),
        moment("2026-10-26T17:00")
    );
}

#[test]
fn r028_elapsed_month_is_30_days() {
    // 2026-02-01 00:00 + 1 emois (43 200 = 30 j) : février 2026 a 28 jours →
    // 2026-03-03 00:00.
    assert_eq!(
        plus("2026-02-01T00:00", elapsed(1, DurationUnit::Month)),
        moment("2026-03-03T00:00")
    );
}

#[test]
fn r028_elapsed_span_not_aligned() {
    use plannercraft_core::TimeScale;
    // Sam 24/10 10:00, 1 ej : pas de recalage sur un jour ouvré, sam 10:00 →
    // dim 10:00.
    let span = TimeScale::Elapsed
        .forward_span(moment("2026-10-24T10:00"), elapsed(1, DurationUnit::Day))
        .unwrap();
    assert_eq!(
        (span.start, span.finish),
        (moment("2026-10-24T10:00"), moment("2026-10-25T10:00"))
    );
}

#[test]
fn r028_elapsed_out_of_domain() {
    use plannercraft_core::{CalendarError, Direction, TimeScale};
    // Sans calendrier : l'erreur de domaine n'a pas d'UID et porte son sens.
    let e = TimeScale::Elapsed;
    // Mer 2149-12-31 23:00 + 60 = 2150-01-01 00:00 (dernier instant) ; + 61
    // en sort.
    assert_eq!(
        e.add(moment("2149-12-31T23:00"), Minutes::new(60)),
        Ok(moment("2150-01-01T00:00"))
    );
    assert_eq!(
        e.add(moment("2149-12-31T23:00"), Minutes::new(61)),
        Err(CalendarError::ElapsedOutOfDomain {
            direction: Direction::After
        })
    );
    // Dim 1984-01-01 01:00 − 61 sort du domaine par le bas.
    assert_eq!(
        e.add(moment("1984-01-01T01:00"), Minutes::new(-61)),
        Err(CalendarError::ElapsedOutOfDomain {
            direction: Direction::Before
        })
    );
}
