//! R-124 — Ressources MSPDI : type, capacité, taux par heure et unité
//! d'affichage, coût par utilisation (lecture ; écriture : commit
//! d'écriture des ressources).

use plannercraft_core::{Money, Rate, RateUnit, ResourceUid};
use plannercraft_mspdi::{ReadOptions, read};
use plannercraft_tests::moment;

use crate::r088_resources_assignments_baselines_import as r088;

fn rate(amount: i64, per: RateUnit) -> Rate {
    Rate {
        amount: Money::new(amount).unwrap(),
        per,
    }
}

#[test]
fn r124_rates_are_hourly_in_the_file() {
    let doc = r088::fixture();
    let r = |uid: u32| doc.project.resources.get(ResourceUid::new(uid)).unwrap();
    // 4,15875 par heure affiché par jour, 480 min/j → 4,15875 × 8 × 100 =
    // 3 327 centièmes par jour (O18).
    assert_eq!(r(1).standard_rate, rate(3_327, RateUnit::Day));
    // 10 par heure, format heure → 1 000 centièmes par heure.
    assert_eq!(r(1).overtime_rate, rate(1_000, RateUnit::Hour));
    // Coût par utilisation 12,5 → 1 250 centièmes.
    assert_eq!(r(1).cost_per_use, Money::new(1_250).unwrap());
    // Matériel : 25 par unité.
    assert_eq!(r(2).standard_rate, rate(2_500, RateUnit::MaterialUnit));
    // An = 12 mois de 20 jours de 480 min : 120 000/h × 1 920 h × 100.
    assert_eq!(r(4).standard_rate, rate(23_040_000_000, RateUnit::Year));
    // Taux absent : 0 par heure.
    assert_eq!(r(3).standard_rate, Rate::default());
}

#[test]
fn r124_rate_with_another_day_length() {
    // 133,333333 par heure affiché par jour, 450 min/j → 99 999,99975
    // centièmes → 100 000 (1 000,00 par jour, R-001).
    let xml = "<Project xmlns=\"http://schemas.microsoft.com/project\"><StartDate>2026-10-19T08:00:00</StartDate>\
               <MinutesPerDay>450</MinutesPerDay><Resources><Resource><UID>1</UID><Name>R</Name>\
               <StandardRate>133.333333</StandardRate><StandardRateFormat>3</StandardRateFormat>\
               <OvertimeRate>1.5</OvertimeRate><OvertimeRateFormat>1</OvertimeRateFormat></Resource></Resources></Project>";
    let doc = read(xml.as_bytes(), &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap();
    let r = doc.project.resources.get(ResourceUid::new(1)).unwrap();
    assert_eq!(r.standard_rate, rate(100_000, RateUnit::Day));
    // 1,5 par heure affiché par minute : 1,5 × 100 / 60 = 2,5 → 3 (R-001).
    assert_eq!(r.overtime_rate, rate(3, RateUnit::Minute));
}

#[test]
fn r124_rates_are_written_per_hour() {
    use plannercraft_core::{Resource, ResourceKind, Units};
    use plannercraft_tests::mspdi::{compact, new_project, reread, text, written};
    let mut proj = new_project("2026-10-19T08:00");
    let mut a = Resource::new(ResourceUid::new(1), "A".to_owned());
    // A6 UID 8 (O18) : 3 327 c/j, 480 min/j → 4,15875 par heure.
    a.standard_rate = rate(3_327, RateUnit::Day);
    a.cost_per_use = Money::new(1_250).unwrap();
    proj.resources.push(a).unwrap();
    let mut m = Resource::new(ResourceUid::new(2), "M".to_owned());
    m.kind = ResourceKind::Material {
        label: "m³".to_owned(),
    };
    m.max_units = Units::new(5_000).unwrap();
    proj.resources.push(m).unwrap();
    let mut c = Resource::new(ResourceUid::new(3), "C".to_owned());
    c.kind = ResourceKind::Cost;
    proj.resources.push(c).unwrap();
    let out = written(&proj);
    let xml = compact(&text(&out));
    assert!(xml.contains("<StandardRate>4.15875</StandardRate><StandardRateFormat>3</StandardRateFormat>"));
    assert!(xml.contains("<CostPerUse>12.5</CostPerUse>"));
    // `MaxUnits` 10 000 → « 1 » ; matérielle : `Type 0`, étiquette ;
    // coût : `Type 1`, `IsCostResource 1`.
    assert!(xml.contains("<MaxUnits>1</MaxUnits>"));
    assert!(xml.contains("<Name>M</Name><Type>0</Type>"));
    assert!(xml.contains("<MaterialLabel>m³</MaterialLabel>") && xml.contains("<MaxUnits>0.5</MaxUnits>"));
    assert!(xml.contains("<IsCostResource>1</IsCostResource>"));
    assert_eq!(reread(&out.xml).project.resources, proj.resources);
}

#[test]
fn r124_rate_written_with_six_decimals_reads_back_exactly() {
    use plannercraft_core::Resource;
    use plannercraft_tests::mspdi::{compact, new_project, reread, text, written};
    // 1 000,00 par jour de 450 min → 133.333333 par heure → relu 100 000.
    let mut proj = new_project("2026-10-19T08:00");
    proj.options.factors = plannercraft_core::ConversionFactors::new(450, 2_250, 20).unwrap();
    let mut r = Resource::new(ResourceUid::new(1), "R".to_owned());
    r.standard_rate = rate(100_000, RateUnit::Day);
    r.overtime_rate = rate(7, RateUnit::Minute);
    proj.resources.push(r).unwrap();
    let out = written(&proj);
    let xml = compact(&text(&out));
    assert!(xml.contains("<StandardRate>133.333333</StandardRate>"), "{xml}");
    let back = reread(&out.xml).project;
    assert_eq!(back.resources, proj.resources);
}
