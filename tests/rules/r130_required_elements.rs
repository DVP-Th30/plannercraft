//! R-130 — Éléments obligatoires à l'écriture.

use plannercraft_mspdi::ReadOptions;
use plannercraft_tests::mspdi::{compact, new_project, reread, text, written};
use plannercraft_tests::{moment, tests_path};

#[test]
fn r130_new_project_save_version_and_currency() {
    let proj = new_project("2026-10-19T08:00");
    let xml = compact(&text(&written(&proj)));
    assert!(
        xml.contains("<Project xmlns=\"http://schemas.microsoft.com/project\"><SaveVersion>14</SaveVersion>")
    );
    assert!(xml.contains("<CurrencyCode>EUR</CurrencyCode>"));
    // Calendriers non vides ; `Tasks` toujours écrit, vide ici.
    assert!(xml.contains("<Calendars><Calendar>"));
    assert!(xml.contains("<Tasks/>"));
    assert!(!xml.contains("<Resources") && !xml.contains("<Assignments"));
    let back = reread(&written(&proj).xml);
    assert_eq!(back.source.save_version, Some(14));
}

#[test]
fn r130_currency_code_read_back_is_kept() {
    // A6 (`takeout-system-zh.xml`) : `CurrencyCode` CNY relu, réécrit.
    let path = tests_path("corpus/files/takeout-system-zh.xml");
    let Ok(bytes) = std::fs::read(&path) else {
        return;
    };
    let doc = plannercraft_mspdi::read(&bytes, &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap();
    let xml = text(&written(&doc.project));
    assert!(xml.contains("<CurrencyCode>CNY</CurrencyCode>"));
    assert!(!xml.contains("<CurrencyCode>EUR</CurrencyCode>"));
}
