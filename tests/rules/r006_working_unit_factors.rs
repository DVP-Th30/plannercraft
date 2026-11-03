//! R-006 — Facteurs des unités ouvrées : m = 1 ; h = 60 ; j = `MinutesPerDay` ;
//! sem = `MinutesPerWeek` ; mois = `DaysPerMonth` × `MinutesPerDay`.

use plannercraft_core::{ConversionFactors, Decimal, DurationUnit, Minutes, TimeBasis};

const W: TimeBasis = TimeBasis::Working;

fn minutes(f: &ConversionFactors, value: Decimal, unit: DurationUnit) -> i64 {
    f.to_minutes(value, unit, W).unwrap().get()
}

#[test]
fn r006_default_factors() {
    let f = ConversionFactors::default();
    // 5 j × 480 = 2 400 ; 2 sem × 2 400 = 4 800 ; 1 mois = 20 × 480 = 9 600.
    assert_eq!(minutes(&f, Decimal::integer(5), DurationUnit::Day), 2_400);
    assert_eq!(minutes(&f, Decimal::integer(2), DurationUnit::Week), 4_800);
    assert_eq!(minutes(&f, Decimal::integer(1), DurationUnit::Month), 9_600);
    // 3 h × 60 = 180 ; 1,5 h × 60 = 90.
    assert_eq!(minutes(&f, Decimal::integer(3), DurationUnit::Hour), 180);
    assert_eq!(minutes(&f, Decimal::new(15, 1).unwrap(), DurationUnit::Hour), 90);
    assert_eq!(minutes(&f, Decimal::integer(7), DurationUnit::Minute), 7);
}

#[test]
fn r006_week_is_not_five_days() {
    // MinutesPerDay = 420, MinutesPerWeek inchangé (2 400) : la semaine vaut
    // 2 400 et non 5 × 420 = 2 100 ; le mois vaut 20 × 420 = 8 400.
    let f = ConversionFactors::new(420, 2_400, 20).unwrap();
    assert_eq!(minutes(&f, Decimal::integer(1), DurationUnit::Day), 420);
    assert_eq!(minutes(&f, Decimal::integer(1), DurationUnit::Week), 2_400);
    assert_eq!(minutes(&f, Decimal::integer(1), DurationUnit::Month), 8_400);
    assert_eq!(f.minutes_per_unit(DurationUnit::Week, W), Minutes::new(2_400));
}

#[test]
fn r006_other_minutes_per_day() {
    // 2 j × 450 = 900.
    let f = ConversionFactors::new(450, 2_250, 20).unwrap();
    assert_eq!(minutes(&f, Decimal::integer(2), DurationUnit::Day), 900);
}
