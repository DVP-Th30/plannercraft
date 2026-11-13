//! `cargo xtask corpus fetch` : télécharge les fichiers du corpus public
//! (ADR-0004, ADR-0009) dans `tests/corpus/files/`, ignoré par git ;
//! `cargo xtask corpus fetch-pdf` : ceux du corpus de l'import PDF
//! (ADR-0032) dans `tests/corpus-pdf/files/`, même mécanisme.
//!
//! Le manifeste `tests/corpus/manifest.toml` est relu avec des types propres
//! à cette tâche (`name`, `url`, `mirrors`, `size`, `sha256` ; les autres clés
//! sont ignorées). Pour chaque entrée :
//! - fichier présent avec la bonne empreinte : rien ;
//! - sinon `url`, puis chaque miroir dans l'ordre, par `curl` vers un fichier
//!   partiel `.part-<nom>` ; taille et SHA-256 contrôlés, puis renommage
//!   atomique ; tout échec donne une ligne « absent : <raison> » et le
//!   fichier partiel est supprimé.
//!
//! Code de sortie 0 même si des fichiers manquent (un absent ne compte ni pour
//! ni contre, le harnais le signale) ; échec seulement si le manifeste est
//! illisible. Aucune crate réseau : `curl` est un outil du système (présent
//! sur les runners GitHub), les tests restent hermétiques.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Manifeste, vu par le téléchargement.
#[derive(Debug, Deserialize)]
struct Manifest {
    /// Entrées du corpus.
    #[serde(default)]
    file: Vec<Entry>,
}

/// Champs d'une entrée utiles au téléchargement.
#[derive(Debug, Deserialize)]
struct Entry {
    /// Nom dans `tests/corpus/files/`.
    name: String,
    /// Adresse principale.
    url: String,
    /// Adresses de repli, dans l'ordre.
    #[serde(default)]
    mirrors: Vec<String>,
    /// Taille attendue en octets.
    size: u64,
    /// Empreinte SHA-256 attendue (64 hexadécimaux minuscules).
    sha256: String,
}

/// Issue du traitement d'une entrée.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// Déjà présent avec la bonne empreinte.
    Present,
    /// Téléchargé depuis cette adresse.
    Downloaded(String),
    /// Absent, avec la raison de chaque tentative.
    Absent(String),
}

pub fn run(args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        Some("fetch") if args.len() == 1 => fetch(&corpus_dir("corpus")),
        // Corpus de l'import PDF (ADR-0032) : même manifeste réduit (les
        // clés propres au test de l'import sont ignorées ici), autre dossier.
        Some("fetch-pdf") if args.len() == 1 => fetch(&corpus_dir("corpus-pdf")),
        _ => {
            eprintln!("usage : cargo xtask corpus fetch | fetch-pdf");
            ExitCode::FAILURE
        }
    }
}

/// Dossier `tests/<nom>/` du dépôt.
fn corpus_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tests")
        .join(name)
}

fn fetch(dir: &Path) -> ExitCode {
    let dir = dir.to_path_buf();
    let manifest_path = dir.join("manifest.toml");
    let manifest = match std::fs::read_to_string(&manifest_path)
        .map_err(|e| e.to_string())
        .and_then(|text| parse(&text))
    {
        Ok(m) => m,
        Err(e) => {
            eprintln!(
                "xtask corpus : manifeste illisible ({}) : {e}",
                manifest_path.display()
            );
            return ExitCode::FAILURE;
        }
    };
    let files = dir.join("files");
    if let Err(e) = std::fs::create_dir_all(&files) {
        eprintln!("xtask corpus : impossible de créer {} : {e}", files.display());
        return ExitCode::FAILURE;
    }
    let mut absent = 0_usize;
    for entry in &manifest.file {
        let outcome = process(entry, &files, &mut download_with_curl);
        match &outcome {
            Outcome::Present => println!("présent : {}", entry.name),
            Outcome::Downloaded(url) => println!("téléchargé : {} ({url})", entry.name),
            Outcome::Absent(reason) => {
                absent = absent.saturating_add(1);
                println!("absent : {} : {reason}", entry.name);
            }
        }
    }
    println!(
        "xtask corpus : {} fichier(s) au manifeste, {absent} absent(s)",
        manifest.file.len()
    );
    ExitCode::SUCCESS
}

fn parse(text: &str) -> Result<Manifest, String> {
    let manifest: Manifest = toml::from_str(text).map_err(|e| e.to_string())?;
    for entry in &manifest.file {
        if !is_safe_name(&entry.name) {
            return Err(format!("nom de fichier refusé : {:?}", entry.name));
        }
    }
    Ok(manifest)
}

/// Nom de fichier simple : ni séparateur, ni `..`, ni nom caché.
fn is_safe_name(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('.') && !name.contains(['/', '\\']) && name != ".."
}

/// Empreinte SHA-256 en hexadécimal minuscule.
fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in &digest {
        // L'écriture dans une `String` n'échoue pas.
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Contenu conforme à l'entrée (taille, puis empreinte) ; sinon la raison.
fn check(entry: &Entry, bytes: &[u8]) -> Result<(), String> {
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if size != entry.size {
        return Err(format!("taille {size} octets, attendu {}", entry.size));
    }
    let hash = sha256_hex(bytes);
    if hash != entry.sha256 {
        return Err(format!("sha256 {hash}, attendu {}", entry.sha256));
    }
    Ok(())
}

/// Traite une entrée : rien si le fichier est déjà bon, sinon essaie chaque
/// adresse avec `download(url, destination, taille attendue)`. Un fichier
/// téléchargé plus gros que prévu est refusé avant d'être lu en mémoire.
fn process(
    entry: &Entry,
    files: &Path,
    download: &mut dyn FnMut(&str, &Path, u64) -> Result<(), String>,
) -> Outcome {
    let target = files.join(&entry.name);
    if let Ok(bytes) = std::fs::read(&target)
        && check(entry, &bytes).is_ok()
    {
        return Outcome::Present;
    }
    let part = files.join(format!(".part-{}", entry.name));
    let mut reasons = Vec::new();
    for url in std::iter::once(&entry.url).chain(entry.mirrors.iter()) {
        let _ = std::fs::remove_file(&part);
        let attempt = if url.starts_with("https://") {
            download(url, &part, entry.size)
                .and_then(|()| not_larger(&part, entry.size))
                .and_then(|()| std::fs::read(&part).map_err(|e| e.to_string()))
                .and_then(|bytes| check(entry, &bytes))
                .and_then(|()| std::fs::rename(&part, &target).map_err(|e| e.to_string()))
        } else {
            Err("adresse non https refusée".to_owned())
        };
        match attempt {
            Ok(()) => return Outcome::Downloaded(url.clone()),
            Err(e) => reasons.push(format!("{url} : {e}")),
        }
    }
    let _ = std::fs::remove_file(&part);
    Outcome::Absent(reasons.join(" ; "))
}

/// Taille du fichier `path` au plus `max` octets, vérifiée sur ses
/// métadonnées (sans le lire).
fn not_larger(path: &Path, max: u64) -> Result<(), String> {
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > max {
        return Err(format!("taille {size} octets, attendu {max}"));
    }
    Ok(())
}

/// Téléchargement par `curl` (échec HTTP = erreur, redirections suivies,
/// 60 s au plus, deux nouvelles tentatives, `--max-filesize` à la taille du
/// manifeste : un serveur ou un miroir compromis ne peut pas servir un
/// fichier énorme).
fn download_with_curl(url: &str, destination: &Path, max_size: u64) -> Result<(), String> {
    let output = Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--max-time",
            "60",
            "--retry",
            "2",
            "--proto",
            "=https",
            "--max-filesize",
        ])
        .arg(max_size.to_string())
        .arg("--output")
        .arg(destination)
        .arg(url)
        .output()
        .map_err(|e| format!("curl introuvable : {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "curl ({}) : {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::panic,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::*;

    const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    fn entry(name: &str, mirrors: &[&str]) -> Entry {
        Entry {
            name: name.to_owned(),
            url: "https://example.invalid/a.xml".to_owned(),
            mirrors: mirrors.iter().map(|m| (*m).to_owned()).collect(),
            size: 3,
            sha256: ABC_SHA256.to_owned(),
        }
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("plannercraft-xtask-corpus-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn sha256_of_abc() {
        assert_eq!(sha256_hex(b"abc"), ABC_SHA256);
    }

    #[test]
    fn manifest_is_read_with_unknown_keys_ignored() {
        let m = parse(
            r#"format = 2
[[file]]
name = "a.xml"
url = "https://example.invalid/a.xml"
mirrors = []
size = 3
sha256 = "00"
license = "none"
"#,
        )
        .unwrap();
        assert_eq!(m.file.len(), 1);
        assert!(parse("[[file]]\nname = \"../x\"\nurl = \"\"\nsize = 1\nsha256 = \"\"").is_err());
        assert!(parse("pas du toml = =").is_err());
    }

    #[test]
    fn safe_names() {
        assert!(is_safe_name("a.xml"));
        assert!(!is_safe_name("../a.xml"));
        assert!(!is_safe_name(".part-a.xml"));
        assert!(!is_safe_name("a\\b"));
        assert!(!is_safe_name(""));
    }

    #[test]
    fn present_file_is_not_downloaded() {
        let dir = temp_dir("present");
        std::fs::write(dir.join("a.xml"), b"abc").unwrap();
        let mut calls = 0;
        let outcome = process(&entry("a.xml", &[]), &dir, &mut |_, _, _| {
            calls += 1;
            Ok(())
        });
        assert_eq!(outcome, Outcome::Present);
        assert_eq!(calls, 0);
    }

    #[test]
    fn mirror_is_used_when_the_main_url_fails_and_bad_content_is_refused() {
        let dir = temp_dir("mirror");
        // Contenu faux sur l'adresse principale, bon sur le second miroir.
        let e = entry(
            "a.xml",
            &["https://example.invalid/m1.xml", "https://example.invalid/m2.xml"],
        );
        let outcome = process(&e, &dir, &mut |url, dest, max| {
            assert_eq!(max, 3);
            if url.ends_with("m1.xml") {
                return Err("404".to_owned());
            }
            let body: &[u8] = if url.ends_with("m2.xml") { b"abc" } else { b"abd" };
            std::fs::write(dest, body).map_err(|e| e.to_string())
        });
        assert_eq!(
            outcome,
            Outcome::Downloaded("https://example.invalid/m2.xml".to_owned())
        );
        assert_eq!(std::fs::read(dir.join("a.xml")).unwrap(), b"abc");
        assert!(!dir.join(".part-a.xml").exists());
    }

    #[test]
    fn every_failure_leaves_the_file_absent_and_no_partial_file() {
        let dir = temp_dir("absent");
        let mut e = entry("a.xml", &["http://example.invalid/clair.xml"]);
        e.size = 4;
        let outcome = process(&e, &dir, &mut |_, dest, _| {
            std::fs::write(dest, b"abc").map_err(|e| e.to_string())
        });
        let Outcome::Absent(reason) = outcome else {
            panic!("absent attendu : {outcome:?}");
        };
        assert!(reason.contains("taille 3 octets"), "{reason}");
        assert!(reason.contains("non https"), "{reason}");
        assert!(!dir.join("a.xml").exists());
        assert!(!dir.join(".part-a.xml").exists());
    }

    #[test]
    fn oversized_download_is_refused_before_reading() {
        let dir = temp_dir("oversized");
        let outcome = process(&entry("a.xml", &[]), &dir, &mut |_, dest, _| {
            std::fs::write(dest, vec![b'x'; 4_096]).map_err(|e| e.to_string())
        });
        let Outcome::Absent(reason) = outcome else {
            panic!("absent attendu : {outcome:?}");
        };
        assert!(reason.contains("taille 4096 octets, attendu 3"), "{reason}");
        assert!(!dir.join(".part-a.xml").exists());
    }
}
