//! Conformité au schéma MSPDI `mspdi_pj15.xsd` (ADR-0010, D18).
//!
//! Le schéma est un fichier public de Microsoft, jamais commité : `cargo
//! xtask schema fetch` l'extrait du SDK de Project 2013 dans
//! `tests/schema/mspdi_pj15.xsd` (ignoré par git, sha256 vérifié). Absent :
//! les tests passent avec un message, sauf sous `PLANNERCRAFT_SCHEMA=required`
//! (job CI `schema`), où ils échouent.
//!
//! - P27 : la table `plannercraft_mspdi::schema` égale le XSD analysé (noms et
//!   ordre des séquences, `minOccurs`, `maxOccurs`, `maxLength`) ;
//! - P26 : les sorties du profil strict (projets générés de P24, fichiers
//!   du corpus relus) sont valides contre pj15 (validateur `uppsala`,
//!   espace de noms ramené à `/2007`, celui du XSD) ; une sortie du profil
//!   observé, débarrassée de `GUID` et des éléments hors schéma puis
//!   remise dans l'ordre pj15, égale la sortie stricte.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::arithmetic_side_effects,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

mod doc;
#[path = "../../crates/plannercraft-mspdi/tests/support/generate.rs"]
mod generate;
mod xsd;

use plannercraft_mspdi::schema::{Parent, Profile, sequence};

/// Empreinte SHA-256 du XSD (révision 2012-07-18).
const XSD_SHA256: &str = "1c208a9732d6ba16f677a9f2fb9d5a629f0c200346b1851998a44765c502eb85";

/// Texte du XSD, empreinte vérifiée ; `None` s'il est absent et que
/// `PLANNERCRAFT_SCHEMA` ne vaut pas `required`.
fn schema_text() -> Option<String> {
    let path = plannercraft_tests::tests_path("schema/mspdi_pj15.xsd");
    let required = std::env::var("PLANNERCRAFT_SCHEMA").is_ok_and(|v| v == "required");
    let Ok(bytes) = std::fs::read(&path) else {
        assert!(
            !required,
            "PLANNERCRAFT_SCHEMA=required et {} absent : lancer `cargo xtask schema fetch`",
            path.display()
        );
        eprintln!(
            "schéma absent ({}) : test sauté ; `cargo xtask schema fetch` pour le lancer",
            path.display()
        );
        return None;
    };
    assert_eq!(
        plannercraft_tests::corpus::manifest::sha256_hex(&bytes),
        XSD_SHA256,
        "{} : empreinte inattendue",
        path.display()
    );
    let text = String::from_utf8(bytes).expect("XSD en UTF-8");
    Some(text.trim_start_matches('\u{feff}').to_owned())
}

/// P27 : la table du schéma égale le XSD analysé.
#[test]
fn p27_schema_table_matches_the_xsd() {
    let Some(text) = schema_text() else {
        return;
    };
    let tree = xsd::parse(&text);
    let project = xsd::project_element(&tree).expect("élément Project");
    for parent in Parent::ALL {
        let element =
            xsd::descend(project, parent.path()).unwrap_or_else(|| panic!("{parent:?} absent du XSD"));
        let expected = xsd::sequence(element);
        let table = sequence(parent).strict;
        let got: Vec<xsd::Item> = table
            .iter()
            .map(|s| xsd::Item {
                name: s.name.to_owned(),
                min: s.min,
                many: s.many,
                max_len: s.max_len,
            })
            .collect();
        assert_eq!(got, expected, "{parent:?} : table différente du XSD");
    }
}

/// P27 : le profil observé est une permutation de pj15 plus `GUID`
/// (`WorkWeek` excepté, voir `Parent::WorkWeek`).
#[test]
fn p27_observed_profile_is_a_permutation() {
    for parent in Parent::ALL {
        if parent == Parent::WorkWeek {
            continue;
        }
        let seq = sequence(parent);
        let mut strict: Vec<&str> = seq.names(Profile::Strict).collect();
        let mut observed: Vec<&str> = seq.names(Profile::Observed).filter(|n| *n != "GUID").collect();
        strict.sort_unstable();
        observed.sort_unstable();
        assert_eq!(strict, observed, "{parent:?}");
    }
}

/// Erreurs de validation d'un document contre pj15.
fn errors(validator: &uppsala::XsdValidator, xml: &str) -> Vec<String> {
    let xml = xml.replace(
        "xmlns=\"http://schemas.microsoft.com/project\"",
        "xmlns=\"http://schemas.microsoft.com/project/2007\"",
    );
    let document = uppsala::parse(&xml).expect("document bien formé");
    validator
        .validate(&document)
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn validator(text: &str) -> uppsala::XsdValidator {
    let schema = uppsala::parse(text).expect("XSD bien formé");
    uppsala::XsdValidator::from_schema(&schema).expect("XSD compilable")
}

/// Document écrit pour P26.
struct Written {
    name: String,
    strict: String,
    observed: String,
    /// Charges verbatim des `Extras` relus d'un fichier du corpus (données
    /// de MS Project, recopiées telles quelles).
    payloads: Vec<String>,
}

/// Charges des `Extras` d'un projet.
fn payloads(p: &plannercraft_core::Project) -> Vec<String> {
    let mut out: Vec<String> = p.extras.0.iter().map(|e| e.payload.clone()).collect();
    for t in p.summary_task.iter().chain(p.tasks.iter()) {
        out.extend(t.extras.0.iter().map(|e| e.payload.clone()));
    }
    for c in &p.calendars {
        out.extend(c.extras.0.iter().map(|e| e.payload.clone()));
    }
    for r in &p.resources {
        out.extend(r.extras.0.iter().map(|e| e.payload.clone()));
    }
    for a in &p.assignments {
        out.extend(a.extras.0.iter().map(|e| e.payload.clone()));
    }
    out
}

/// Lignes (depuis 1) occupées par les charges dans un texte (charges
/// distinctes, recherche des débuts de ligne par dichotomie).
fn payload_lines(text: &str, payloads: &[String]) -> Vec<(usize, usize)> {
    let starts: Vec<usize> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let line_of = |at: usize| starts.partition_point(|s| *s <= at);
    let unique: std::collections::BTreeSet<&str> = payloads
        .iter()
        .map(String::as_str)
        .filter(|p| p.contains('<'))
        .collect();
    let mut out = Vec::new();
    for p in unique {
        for (at, _) in text.match_indices(p) {
            out.push((line_of(at), line_of(at + p.len())));
        }
    }
    out
}

/// Documents écrits pour P26 : projets générés, puis fichiers du corpus.
fn documents() -> Vec<Written> {
    use plannercraft_core::{Severity, schedule};
    use plannercraft_mspdi::write::{Computed, OrderProfile, WriteOptions, write};
    let strict = WriteOptions {
        profile: OrderProfile::Strict,
        ..WriteOptions::default()
    };
    let text = |b: Vec<u8>| String::from_utf8(b).expect("UTF-8");
    let mut out = Vec::new();
    for seed in 0..64 {
        let p = generate::project(seed);
        if p.validate()
            .finish()
            .iter()
            .any(|d| d.severity() == Severity::Error)
        {
            continue;
        }
        let s = schedule(&p);
        out.push(Written {
            name: format!("graine {seed}"),
            strict: text(write(&p, Computed::Schedule(&s), &strict).xml),
            observed: text(write(&p, Computed::Schedule(&s), &WriteOptions::default()).xml),
            // Extras hérités verbatim (R-198 : `LevelingDelayFormat` hors
            // pj15 d'un projet enregistré avant J5) : admis comme au corpus.
            payloads: payloads(&p),
        });
    }
    let dir = plannercraft_tests::tests_path("corpus/files");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .map(|d| d.filter_map(Result::ok).map(|e| e.path()).collect())
        .unwrap_or_default();
    files.sort();
    for path in files
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "xml"))
    {
        let bytes = std::fs::read(&path).unwrap();
        let read = plannercraft_mspdi::read(
            &bytes,
            &plannercraft_mspdi::ReadOptions::new(plannercraft_tests::moment("2026-01-01T00:00")),
        )
        .unwrap();
        let rec = Computed::Recorded(&read.recorded);
        out.push(Written {
            name: path.display().to_string(),
            strict: text(write(&read.project, rec, &strict).xml),
            observed: text(write(&read.project, rec, &WriteOptions::default()).xml),
            payloads: payloads(&read.project),
        });
        // J6 (R-215, R-226) : avancement, `Stop`/`Resume` et écarts écrits
        // par le moteur.
        let s = schedule(&read.project);
        let engine = Computed::Schedule(&s);
        out.push(Written {
            name: format!("{} (moteur)", path.display()),
            strict: text(write(&read.project, engine, &strict).xml),
            observed: text(write(&read.project, engine, &WriteOptions::default()).xml),
            payloads: payloads(&read.project),
        });
    }
    out
}

/// P26 : sorties strictes valides contre pj15. Fichiers du corpus : les
/// seules erreurs admises sont dans les charges verbatim de leurs `Extras`
/// (données de MS Project hors pj15, comme `ExtendedAttribute/Guid` de A7 :
/// les fichiers sources ont déjà 76 à 80 erreurs, O19).
#[test]
fn p26_strict_output_is_valid_against_pj15() {
    let Some(text) = schema_text() else {
        return;
    };
    let validator = validator(&text);
    for w in documents() {
        // `uppsala` compte `maxLength` en octets UTF-8, le schéma XML en
        // caractères : ces erreurs sont écartées ici et la longueur en
        // caractères est vérifiée à part (`doc::over_max_len`).
        let errs: Vec<String> = errors(&validator, &w.strict)
            .into_iter()
            .filter(|e| !e.contains("exceeds maxLength"))
            .collect();
        // Charges localisées seulement s'il y a des erreurs (coûteux).
        let spans = if errs.is_empty() {
            Vec::new()
        } else {
            payload_lines(&w.strict, &w.payloads)
        };
        let errs: Vec<String> = errs
            .into_iter()
            .filter(|e| {
                let line: usize = e.split(':').next().and_then(|l| l.parse().ok()).unwrap_or(0);
                !spans.iter().any(|(a, b)| (*a..=*b).contains(&line))
            })
            .collect();
        assert!(
            errs.is_empty(),
            "{} : {} erreurs pj15\n{:#?}",
            w.name,
            errs.len(),
            errs.iter().take(10).collect::<Vec<_>>()
        );
        let tree = doc::parse(&w.strict);
        let long = doc::over_max_len(&tree, None);
        assert!(long.is_empty(), "{} : maxLength dépassé : {long:?}", w.name);
    }
}

/// P26 : sortie observée remise dans l'ordre pj15 = sortie stricte.
#[test]
fn p26_observed_reordered_equals_strict() {
    for w in documents() {
        let mut o = doc::parse(&w.observed);
        doc::canonical(&mut o, None);
        let s = doc::parse(&w.strict);
        assert!(
            o == s,
            "{} : sortie observée réordonnée différente de la sortie stricte",
            w.name
        );
    }
}

/// H-R12 (J8, lot L6) : l'instruction `<?plannercraft-style?>` ne change rien à
/// la conformité pj15 de la sortie stricte.
#[test]
fn style_xsd_strict_with_pi() {
    use plannercraft_mspdi::write::{Computed, OrderProfile, WriteOptions, write};
    let Some(text) = schema_text() else {
        return;
    };
    let validator = validator(&text);
    let strict = WriteOptions {
        profile: OrderProfile::Strict,
        ..WriteOptions::default()
    };
    let mut p = generate::project(3);
    plannercraft_mspdi::style_pi::set(
        &mut p.extras,
        Some(r#"{"base":"presentation","over":{"density":"airy"},"v":1}"#.to_owned()),
    );
    let s = plannercraft_core::schedule(&p);
    let xml = String::from_utf8(write(&p, Computed::Schedule(&s), &strict).xml).expect("UTF-8");
    assert!(xml.contains("?>\n<?plannercraft-style {"), "instruction écrite");
    let errs: Vec<String> = errors(&validator, &xml)
        .into_iter()
        .filter(|e| !e.contains("exceeds maxLength"))
        .collect();
    assert!(errs.is_empty(), "{errs:#?}");
}
