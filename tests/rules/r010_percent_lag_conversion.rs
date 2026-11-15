//! R-010 — Décalage en pourcentage : minutes = `round_div(durée du
//! prédécesseur × pct, 10 000)`. L'application aux liens arrive en J2.

use plannercraft_core::{Minutes, SignedPercent, percent_of};

fn pct(v: i64) -> SignedPercent {
    SignedPercent::new(v).unwrap()
}

#[test]
fn r010_percent_of_predecessor_duration() {
    // 50 % de 1 440 = 720.
    assert_eq!(percent_of(Minutes::new(1_440), pct(5_000)), Ok(Minutes::new(720)));
    // 33 % de 480 = 158,4, au dixième (ADR-0039 ; avant : 158).
    assert_eq!(
        percent_of(Minutes::new(480), pct(3_300)),
        Ok(Minutes::from_tenths(1_584))
    );
    // −25 % de 480 = −120.
    assert_eq!(percent_of(Minutes::new(480), pct(-2_500)), Ok(Minutes::new(-120)));
    // 0,01 % de 480 = 0,048 → 0 (R-001 au dixième).
    assert_eq!(percent_of(Minutes::new(480), pct(1)), Ok(Minutes::new(0)));
    // 0,1 % de 480 = 0,48 → 0,5.
    assert_eq!(
        percent_of(Minutes::new(480), pct(10)),
        Ok(Minutes::from_tenths(5))
    );
}
