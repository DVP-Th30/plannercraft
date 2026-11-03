//! Harnais de l'oracle MS Project (coquille, ADR-0004).
//!
//! Format attendu (voir `tests/oracle/README.md`) : pour chaque cas, un
//! fichier MSPDI d'entrée `tests/oracle/input/<cas>.xml` et le même projet
//! recalculé et exporté par MS Project `tests/oracle/msproject/<cas>.xml`,
//! produit par `tools/oracle/generate.bas`. Le harnais recalculera l'entrée
//! avec le moteur et comparera les dates champ par champ, avec un rapport par
//! cas.
//!
//! État J1 : ni lecture MSPDI ni moteur. Le harnais découvre les cas et
//! échoue s'il en trouve, pour qu'un cas déposé ne passe jamais en silence
//! avant que la comparaison existe.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use std::path::{Path, PathBuf};

/// Cas d'oracle : une paire entrée / sortie de MS Project.
#[derive(Debug)]
struct OracleCase {
    name: String,
    input: PathBuf,
    expected: PathBuf,
}

/// Liste les cas de `oracle_dir`, triés par nom (déterminisme). Une entrée
/// sans sortie MS Project correspondante est une erreur.
fn discover_cases(oracle_dir: &Path) -> Result<Vec<OracleCase>, String> {
    let input_dir = oracle_dir.join("input");
    if !input_dir.exists() {
        return Ok(Vec::new());
    }
    let entries =
        std::fs::read_dir(&input_dir).map_err(|error| format!("{}: {error}", input_dir.display()))?;
    let mut cases = Vec::new();
    for entry in entries {
        let input = entry.map_err(|error| error.to_string())?.path();
        if input.extension().and_then(|ext| ext.to_str()) != Some("xml") {
            continue;
        }
        let Some(file_name) = input.file_name() else {
            continue;
        };
        let expected = oracle_dir.join("msproject").join(file_name);
        let name = input
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_owned();
        if !expected.is_file() {
            return Err(format!(
                "cas `{name}` : sortie MS Project manquante ({})",
                expected.display()
            ));
        }
        cases.push(OracleCase {
            name,
            input,
            expected,
        });
    }
    cases.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(cases)
}

#[test]
fn oracle_cases_are_compared() {
    let cases = discover_cases(&plannercraft_tests::tests_path("oracle")).expect("découverte des cas");
    if cases.is_empty() {
        println!("oracle : aucun cas (pas de licence MS Project pour l'instant)");
        return;
    }
    let listing: Vec<String> = cases
        .iter()
        .map(|case| {
            format!(
                "{} ({} / {})",
                case.name,
                case.input.display(),
                case.expected.display()
            )
        })
        .collect();
    panic!(
        "oracle : {} cas présents mais la comparaison n'est pas encore implémentée (MSPDI en J3) : {}",
        cases.len(),
        listing.join(", ")
    );
}
