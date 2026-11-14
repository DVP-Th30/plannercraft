//! Corpus de l'import PDF (ADR-0032, lot P1a).
//!
//! Corpus public : `tests/corpus-pdf/manifest.toml`, fichiers téléchargés
//! par `cargo xtask corpus fetch-pdf` (jamais commités). Chaque fichier
//! présent est importé : refus attendu (`expect` = code), ou import avec
//! exactement `tasks` tâches et au moins `min_dates_equal` dates calculées
//! égales aux dates lues. Un fichier absent ne compte ni pour ni contre ;
//! `PLANNERCRAFT_PDF_CORPUS=required` fait échouer le test si aucun n'est
//! présent.
//!
//! Plannings du propriétaire : `PLANNERCRAFT_PDF_SAMPLES=<dossier>` (hors
//! du dépôt) ; mesurés sous les noms R1, R2… (ordre des noms de fichier),
//! sans aucun texte des fichiers dans la sortie ; chacun doit s'importer.
//!
//! Tableau de mesure (Markdown) sur la sortie (`--nocapture`) et dans
//! `CARGO_TARGET_TMPDIR/pdf-corpus-report.md`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::print_stdout,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use plannercraft_automation::text::Language;
use plannercraft_core::Moment;
use plannercraft_pdfimport::{
    ConvertOptions, Converted, DeduceLimits, Deduction, ImportOptions, ImportedPlan, NonWorkingDays,
    calendar, convert, deduce, import,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Manifest {
    file: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
struct Entry {
    name: String,
    url: String,
    expect: String,
    tasks: usize,
    min_dates_equal: u32,
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// Ligne de mesure d'un fichier importé.
struct Measure {
    tasks: usize,
    equal: u32,
    row: String,
}

fn measure(label: &str, bytes: &[u8]) -> Result<Measure, String> {
    let t0 = Instant::now();
    let plan: ImportedPlan = import(bytes, &ImportOptions::default()).map_err(|e| e.code().to_owned())?;
    let options = |pin| ConvertOptions {
        name: "corpus".to_owned(),
        now: "2026-11-13T08:00".parse::<Moment>().unwrap(),
        lang: Language::Fr,
        pin_read_starts: pin,
        non_working: Vec::new(),
    };
    let strict = convert(&plan, &options(false)).map_err(|e| e.to_string())?;
    let ms = t0.elapsed().as_millis();
    let pinned = convert(&plan, &options(true)).map_err(|e| e.to_string())?;
    let s = &plan.stats;
    let n = plan.tasks.len();
    let row = format!(
        "| {label} | {} | {} | {n} | {}{} | {}/{} | {} ({}) | {}/{n} · {}/{n} | {} | {}/{} | {}/{} | {ms} |",
        plan.source.pages,
        s.max_id,
        s.levels,
        if s.levels_from_wbs { " WBS" } else { "" },
        s.summaries_consistent,
        s.summaries_checked,
        s.links_read,
        s.links_dropped,
        s.starts_read,
        s.finishes_read,
        s.doubtful_rows,
        strict.dates_equal,
        strict.dates_compared,
        pinned.dates_equal,
        pinned.dates_compared,
    );
    Ok(Measure {
        tasks: n,
        equal: strict.dates_equal,
        row,
    })
}

const HEADER: &str = "| Fichier | Pages | N° max | Tâches lues | Niveaux | Récap. cohérentes | Liens (ignorés) | Débuts · fins relus | Douteuses | Dates égales (R-271) | Dates égales (--pin-starts) | ms (debug) |\n|---|---|---|---|---|---|---|---|---|---|---|---|\n";

#[test]
fn pdf_corpus() {
    let dir = repo().join("tests").join("corpus-pdf");
    let manifest: Manifest =
        toml::from_str(&std::fs::read_to_string(dir.join("manifest.toml")).unwrap()).unwrap();
    let mut report = String::from("## Corpus public\n\n");
    report.push_str(HEADER);
    let mut refused = String::from("\n### Refus\n\n| Fichier | Refus attendu | Obtenu |\n|---|---|---|\n");
    let mut present = 0_usize;
    let mut failures = Vec::new();
    for e in &manifest.file {
        let Ok(bytes) = std::fs::read(dir.join("files").join(&e.name)) else {
            continue;
        };
        present += 1;
        let label = format!("[{}]({})", e.name.trim_end_matches(".pdf"), e.url);
        match measure(&label, &bytes) {
            Ok(m) => {
                let _ = writeln!(report, "{}", m.row);
                if e.expect != "import" {
                    failures.push(format!("{} : importé, refus {} attendu", e.name, e.expect));
                } else if e.tasks > 0 && m.tasks != e.tasks {
                    failures.push(format!("{} : {} tâches, {} attendues", e.name, m.tasks, e.tasks));
                } else if m.equal < e.min_dates_equal {
                    failures.push(format!(
                        "{} : {} dates égales < plancher {}",
                        e.name, m.equal, e.min_dates_equal
                    ));
                }
            }
            Err(code) => {
                let _ = writeln!(refused, "| {label} | {} | {code} |", e.expect);
                if code != e.expect {
                    failures.push(format!("{} : {code}, attendu {}", e.name, e.expect));
                }
            }
        }
    }
    report.push_str(&refused);
    if let Ok(samples) = std::env::var("PLANNERCRAFT_PDF_SAMPLES") {
        report.push_str("\n## Plannings du propriétaire (R1 à Rn)\n\n");
        report.push_str(HEADER);
        let mut paths: Vec<PathBuf> = std::fs::read_dir(&samples)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pdf")))
            .collect();
        paths.sort();
        for (k, p) in paths.iter().enumerate() {
            let label = format!("R{}", k + 1);
            match measure(&label, &std::fs::read(p).unwrap()) {
                Ok(m) => {
                    let _ = writeln!(report, "{}", m.row);
                }
                Err(code) => failures.push(format!("{label} : {code}")),
            }
        }
    }
    println!("{report}");
    std::fs::write(
        Path::new(env!("CARGO_TARGET_TMPDIR")).join("pdf-corpus-report.md"),
        &report,
    )
    .unwrap();
    if std::env::var("PLANNERCRAFT_PDF_CORPUS").as_deref() == Ok("required") {
        assert!(
            present > 0,
            "aucun fichier du corpus PDF : cargo xtask corpus fetch-pdf"
        );
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Conversion avec les jours chômés `non_working`.
fn convert_with(plan: &ImportedPlan, non_working: Vec<NonWorkingDays>) -> Converted {
    convert(
        plan,
        &ConvertOptions {
            name: "corpus".to_owned(),
            now: "2026-11-13T08:00".parse::<Moment>().unwrap(),
            lang: Language::Fr,
            pin_read_starts: false,
            non_working,
        },
    )
    .unwrap()
}

/// Périodes en clair, sans jour précis : mois du début (et de la fin s'il
/// diffère) et jours ouvrés.
fn periods_text(d: &Deduction) -> String {
    const MONTHS: [&str; 12] = [
        "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc.",
    ];
    use chrono::Datelike as _;
    let month = |day: plannercraft_core::Day| MONTHS[day.to_naive().month0() as usize];
    d.periods
        .iter()
        .map(|p| {
            let (from, to) = p.placement(p.default_option).unwrap();
            let (a, b) = (month(from), month(to));
            let m = if a == b { a.to_owned() } else { format!("{a}-{b}") };
            let kind = if p.exact() { "" } else { " à placer" };
            format!("{m} {} j{kind} ({} t.)", p.working_days, p.explained)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Ligne de mesure du calendrier (lot P1b) : dates égales dans quatre cas.
fn calendar_row(label: &str, bytes: &[u8]) -> Result<String, String> {
    let plan: ImportedPlan = import(bytes, &ImportOptions::default()).map_err(|e| e.code().to_owned())?;
    let limits = DeduceLimits::default();
    let standard = convert_with(&plan, Vec::new());
    let holidays = calendar::french_holidays(&plan, Language::Fr);
    let with_holidays = convert_with(&plan, holidays.clone());
    let t0 = Instant::now();
    let alone = deduce(&plan, standard.session.project(), &limits);
    let both = deduce(&plan, with_holidays.session.project(), &limits);
    let us = t0.elapsed().as_micros() / 2;
    let periods_alone = convert_with(
        &plan,
        calendar::periods(&calendar::default_placements(&alone.periods), "Congés"),
    );
    let mut all = holidays;
    all.extend(calendar::periods(
        &calendar::default_placements(&both.periods),
        "Congés",
    ));
    let periods_both = convert_with(&plan, all);
    let pct = |c: &Converted| format!("{}/{}", c.dates_equal, c.dates_compared);
    // Réglage par défaut de la fenêtre : fériés cochés si l'en-tête est
    // français ; périodes proposées sur ce calendrier-là.
    let french = calendar::suggests_french_holidays(&plan);
    let shown = if french { &both } else { &alone };
    Ok(format!(
        "| {label} | {} | {} | {} | {} ({}) | {} ({}) | {} | {}/{}/{} | {}/{}/{}/{} | {us} |",
        if french { "fr" } else { "en" },
        pct(&standard),
        pct(&with_holidays),
        pct(&periods_alone),
        alone.periods.len(),
        pct(&periods_both),
        both.periods.len(),
        periods_text(shown),
        shown.usable,
        shown.proven,
        shown.equations,
        shown.explained,
        shown.impossible,
        shown.ambiguous,
        shown.partial_days + shown.contradicted,
    ))
}

const CALENDAR_HEADER: &str = "| Fichier | Langue | Standard | Fériés | Périodes seules (n) | Fériés + périodes (n) | Périodes proposées (réglage par défaut) | Exploitables/preuves/équations | Expliquées/impossibles/ambiguës/écartées | µs déduction |\n|---|---|---|---|---|---|---|---|---|---|\n";

/// Mesure du calendrier (lot P1b, R-274 à R-277) : dates égales au jour
/// avec le calendrier Standard, les fériés français, les périodes déduites
/// seules, les deux. Sortie seulement (`--nocapture`), aucun plancher.
#[test]
fn pdf_corpus_calendar() {
    let dir = repo().join("tests").join("corpus-pdf");
    let manifest: Manifest =
        toml::from_str(&std::fs::read_to_string(dir.join("manifest.toml")).unwrap()).unwrap();
    let mut report = String::from("## Calendrier, corpus public\n\n");
    report.push_str(CALENDAR_HEADER);
    for e in manifest.file.iter().filter(|e| e.expect == "import") {
        let Ok(bytes) = std::fs::read(dir.join("files").join(&e.name)) else {
            continue;
        };
        if let Ok(row) = calendar_row(e.name.trim_end_matches(".pdf"), &bytes) {
            let _ = writeln!(report, "{row}");
        }
    }
    if let Ok(samples) = std::env::var("PLANNERCRAFT_PDF_SAMPLES") {
        report.push_str("\n## Calendrier, plannings du propriétaire\n\n");
        report.push_str(CALENDAR_HEADER);
        let mut paths: Vec<PathBuf> = std::fs::read_dir(&samples)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pdf")))
            .collect();
        paths.sort();
        for (k, p) in paths.iter().enumerate() {
            let label = format!("R{}", k + 1);
            match calendar_row(&label, &std::fs::read(p).unwrap()) {
                Ok(row) => {
                    let _ = writeln!(report, "{row}");
                }
                Err(code) => {
                    let _ = writeln!(report, "| {label} | refus {code} |");
                }
            }
        }
    }
    println!("{report}");
}
