//! R-086 — Options du chemin critique (`CriticalSlackLimit`,
//! `MultipleCriticalPaths`) à l'import MSPDI.

use plannercraft_mspdi::{ImportCode, MspdiDocument, ReadOptions, read};
use plannercraft_tests::moment;

fn project(body: &str) -> MspdiDocument {
    let xml = format!(
        "<Project><StartDate>2026-10-19T08:00:00</StartDate><CalendarUID>1</CalendarUID>{body}</Project>"
    );
    read(xml.as_bytes(), &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap()
}

#[test]
fn r086_critical_slack_limit_in_days() {
    // 10 jours ; avec 480 min/jour, le moteur en fait un seuil de 4 800 min.
    let doc = project(
        "<CriticalSlackLimit>10</CriticalSlackLimit><MultipleCriticalPaths>1</MultipleCriticalPaths>",
    );
    let o = doc.project.options;
    assert_eq!(o.critical_slack_limit_days, 10);
    assert_eq!(
        i64::from(o.critical_slack_limit_days) * i64::from(o.factors.minutes_per_day()),
        4_800
    );
    assert!(o.multiple_critical_paths);
    assert!(doc.diagnostics.iter().all(|d| d.element != "CriticalSlackLimit"));
}

#[test]
fn r086_absent_means_zero_and_single_path() {
    let doc = project("");
    assert_eq!(doc.project.options.critical_slack_limit_days, 0);
    assert!(!doc.project.options.multiple_critical_paths);
}

#[test]
fn r086_negative_or_too_large_is_invalid() {
    for value in ["-1", "4294967296", "1.5"] {
        let doc = project(&format!("<CriticalSlackLimit>{value}</CriticalSlackLimit>"));
        assert_eq!(doc.project.options.critical_slack_limit_days, 0, "{value}");
        assert!(
            doc.diagnostics
                .iter()
                .any(|d| d.code == ImportCode::InvalidValue && d.element == "CriticalSlackLimit"),
            "{value}"
        );
    }
}
