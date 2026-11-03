//! R-075 — Dates MSPDI sans fuseau, secondes tronquées (import MSPDI).

use plannercraft_mspdi::scalar::{DateTimeError, parse_datetime};
use plannercraft_tests::moment;

#[test]
fn r075_local_datetime() {
    // Lundi 09/11/2009 08:00, sans fuseau ni troncature.
    let p = parse_datetime("2009-11-09T08:00:00").unwrap();
    assert_eq!(p.moment, moment("2009-11-09T08:00"));
    assert!(!p.rounded);
    assert!(!p.zone_ignored);
}

#[test]
fn r075_seconds_truncated() {
    // 18:18:20 → 18:18 (R-004), troncature signalée (`mspdi.rounded`).
    let p = parse_datetime("2026-08-07T18:18:20").unwrap();
    assert_eq!(p.moment, moment("2026-08-07T18:18"));
    assert!(p.rounded);
}

#[test]
fn r075_out_of_domain() {
    // Avant 1984 : aucune date (l'import applique la valeur par défaut).
    assert_eq!(
        parse_datetime("1983-12-31T08:00:00"),
        Err(DateTimeError::OutOfDomain)
    );
}

#[test]
fn r075_timezone_suffix_ignored() {
    // `Z` ou `±hh:mm` : ignoré, l'heure locale écrite est gardée, signalé
    // (`mspdi.inconsistent`).
    let p = parse_datetime("2026-10-19T08:00:00+02:00").unwrap();
    assert_eq!(p.moment, moment("2026-10-19T08:00"));
    assert!(p.zone_ignored);
}

#[test]
fn r075_end_of_day() {
    // 24:00:00 = lendemain 00:00 (XSD).
    assert_eq!(
        parse_datetime("2026-10-19T24:00:00").unwrap().moment,
        moment("2026-10-20T00:00")
    );
}
