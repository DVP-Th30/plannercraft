//! Corpus de l'import `.mpp` (ADR-0036, lot M1).
//!
//! Corpus public : `tests/corpus-mpp/manifest.toml` (sélection de fichiers de
//! test publics, adresses figées sur un commit), fichiers téléchargés par
//! `cargo xtask corpus fetch-mpp` (jamais commités). Chaque fichier présent
//! est lu : refus attendu (`expect` = code), ou lecture avec exactement les
//! comptes attendus (tâches, lignes vides, liens, calendriers, exceptions),
//! le début du projet enregistré, et au moins `min_identical` tâches dont
//! les dates recalculées par le moteur égalent les dates enregistrées. Un
//! fichier absent ne compte ni pour ni contre ;
//! `PLANNERCRAFT_MPP_CORPUS=required` fait échouer le test si aucun n'est
//! présent. Tableau de mesure sur la sortie (`--nocapture`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::print_stdout,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use std::path::{Path, PathBuf};

use plannercraft_core::{Moment, schedule};
use plannercraft_mpp::{ReadOptions, compare, read, report};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Manifest {
    file: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
struct Entry {
    name: String,
    expect: String,
    #[serde(default)]
    tasks: usize,
    #[serde(default)]
    null_tasks: usize,
    #[serde(default)]
    links: usize,
    #[serde(default)]
    calendars: usize,
    #[serde(default)]
    calendar_exceptions: usize,
    #[serde(default)]
    project_start: String,
    #[serde(default)]
    min_identical: u32,
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

#[test]
fn mpp_corpus() {
    let dir = repo().join("tests").join("corpus-mpp");
    let manifest: Manifest =
        toml::from_str(&std::fs::read_to_string(dir.join("manifest.toml")).unwrap()).unwrap();
    let now: Moment = "2026-11-14T08:00".parse().unwrap();
    let mut present = 0;
    let mut failures = Vec::new();
    println!("| Fichier | Attendu | Obtenu | Tâches | Liens | Calendriers | Dates identiques |");
    println!("|---|---|---|---|---|---|---|");
    for e in &manifest.file {
        let Ok(bytes) = std::fs::read(dir.join("files").join(&e.name)) else {
            continue;
        };
        present += 1;
        let got = read(&bytes, &ReadOptions::new("corpus".to_owned(), now));
        match (&got, e.expect.as_str()) {
            (Err(err), expected) => {
                println!("| {} | {expected} | {} | | | | |", e.name, err.code());
                if err.code() != expected {
                    failures.push(format!("{} : refus {} au lieu de {expected}", e.name, err.code()));
                }
            }
            (Ok(doc), "import") => {
                let s = schedule(&doc.project);
                let c = compare(&doc.project, &s, &doc.recorded);
                let v = report::view(doc, Some(&c));
                println!(
                    "| {} | import | import | {} ({} vides) | {} | {} ({} exceptions) | {}/{} |",
                    e.name,
                    v.tasks,
                    v.null_tasks,
                    v.links,
                    v.calendars,
                    v.calendar_exceptions,
                    c.tasks_identical,
                    c.tasks_compared
                );
                let found = (
                    v.tasks,
                    v.null_tasks,
                    v.links,
                    v.calendars,
                    v.calendar_exceptions,
                    v.project_start.clone().unwrap_or_default(),
                );
                let wanted = (
                    e.tasks,
                    e.null_tasks,
                    e.links,
                    e.calendars,
                    e.calendar_exceptions,
                    e.project_start.clone(),
                );
                if found != wanted {
                    failures.push(format!("{} : lu {found:?}, attendu {wanted:?}", e.name));
                }
                if c.tasks_identical < e.min_identical {
                    failures.push(format!(
                        "{} : {} tâches identiques, plancher {}",
                        e.name, c.tasks_identical, e.min_identical
                    ));
                }
            }
            (Ok(_), expected) => failures.push(format!("{} : lu, refus {expected} attendu", e.name)),
        }
    }
    println!("\n{present} fichier(s) présent(s) sur {}", manifest.file.len());
    if std::env::var("PLANNERCRAFT_MPP_CORPUS").as_deref() == Ok("required") {
        assert!(
            present > 0,
            "aucun fichier du corpus : lancer `cargo xtask corpus fetch-mpp`"
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
