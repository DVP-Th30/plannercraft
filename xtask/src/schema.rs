//! `cargo xtask schema fetch` : extrait le schéma MSPDI `mspdi_pj15.xsd` du
//! SDK de Project 2013 (ADR-0010, D18) dans `tests/schema/mspdi_pj15.xsd`,
//! ignoré par git.
//!
//! Le schéma est un fichier public de Microsoft, distribué dans le MSI du
//! SDK (64 Mo). La tâche :
//! - ne fait rien si le XSD est présent avec la bonne empreinte ;
//! - sinon télécharge le MSI par `curl` (taille bornée, https seulement)
//!   dans `target/xtask-schema/`, vérifie sa taille et son SHA-256, lit le
//!   flux `PJ15SDK.cab` (crate `msi`), en extrait le fichier
//!   `Projec_1_File_xsd_mspdi_pj15` (crate `cab`, Rust pur), vérifie son
//!   SHA-256, l'écrit (renommage atomique) et supprime le MSI.
//!
//! Le test `plannercraft-tests --test schema` revérifie l'empreinte du XSD. En CI
//! (job `schema`), le XSD est mis en cache sous la clé de son empreinte et
//! revérifié par cette tâche après restauration.

use std::fmt::Write as _;
use std::io::{Cursor, Read as _};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use sha2::{Digest, Sha256};

/// Adresse du MSI du SDK de Project 2013 (page « Project 2013 SDK »,
/// microsoft.com/en-us/download/details.aspx?id=30435).
const MSI_URL: &str =
    "https://download.microsoft.com/download/2/1/2/2125fe61-0e1e-49f4-adc1-cce92ff613db/Project2013SDK.msi";
/// Taille du MSI, en octets.
const MSI_SIZE: u64 = 64_038_400;
/// Empreinte SHA-256 du MSI.
const MSI_SHA256: &str = "75891caee6e10b87ab2281d33cdb9c19d16e70b48cc1cf7d7433c583564988b1";
/// Flux du MSI qui contient l'archive CAB.
const CAB_STREAM: &str = "PJ15SDK.cab";
/// Nom du fichier XSD dans l'archive CAB (clé de la table `File` du MSI).
const CAB_FILE: &str = "Projec_1_File_xsd_mspdi_pj15";
/// Empreinte SHA-256 du XSD (révision 2012-07-18, citée dans
/// `docs/scheduling-rules.md`).
pub const XSD_SHA256: &str = "1c208a9732d6ba16f677a9f2fb9d5a629f0c200346b1851998a44765c502eb85";
/// Taille maximale du XSD extrait.
const XSD_MAX: u64 = 1_048_576;

pub fn run(args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        Some("fetch") if args.len() == 1 => fetch(),
        _ => {
            eprintln!("usage : cargo xtask schema fetch");
            ExitCode::FAILURE
        }
    }
}

/// Racine du dépôt.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn fetch() -> ExitCode {
    let target = root().join("tests").join("schema").join("mspdi_pj15.xsd");
    if let Ok(bytes) = std::fs::read(&target)
        && sha256_hex(&bytes) == XSD_SHA256
    {
        println!("présent : {}", target.display());
        return ExitCode::SUCCESS;
    }
    let work = root().join("target").join("xtask-schema");
    match fetch_into(&target, &work, &mut download_with_curl) {
        Ok(()) => {
            println!("extrait : {}", target.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("xtask schema : {e}");
            ExitCode::FAILURE
        }
    }
}

/// Télécharge le MSI dans `work`, en extrait le XSD vers `target`.
fn fetch_into(
    target: &Path,
    work: &Path,
    download: &mut dyn FnMut(&str, &Path, u64) -> Result<(), String>,
) -> Result<(), String> {
    std::fs::create_dir_all(work).map_err(|e| format!("{} : {e}", work.display()))?;
    let msi_path = work.join("Project2013SDK.msi");
    let have = std::fs::read(&msi_path)
        .ok()
        .filter(|b| u64::try_from(b.len()).ok() == Some(MSI_SIZE) && sha256_hex(b) == MSI_SHA256);
    let msi_bytes = if let Some(b) = have {
        b
    } else {
        let _ = std::fs::remove_file(&msi_path);
        download(MSI_URL, &msi_path, MSI_SIZE)?;
        let size = std::fs::metadata(&msi_path).map_err(|e| e.to_string())?.len();
        if size != MSI_SIZE {
            let _ = std::fs::remove_file(&msi_path);
            return Err(format!("MSI de {size} octets, attendu {MSI_SIZE}"));
        }
        let bytes = std::fs::read(&msi_path).map_err(|e| e.to_string())?;
        let hash = sha256_hex(&bytes);
        if hash != MSI_SHA256 {
            let _ = std::fs::remove_file(&msi_path);
            return Err(format!("MSI sha256 {hash}, attendu {MSI_SHA256}"));
        }
        bytes
    };
    let xsd = extract(msi_bytes)?;
    let hash = sha256_hex(&xsd);
    if hash != XSD_SHA256 {
        return Err(format!("XSD sha256 {hash}, attendu {XSD_SHA256}"));
    }
    write_atomically(target, &xsd)?;
    let _ = std::fs::remove_file(&msi_path);
    Ok(())
}

/// Extrait le XSD du MSI (flux CAB, puis fichier de l'archive).
fn extract(msi_bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    let mut package =
        msi::Package::open(Cursor::new(msi_bytes)).map_err(|e| format!("MSI illisible : {e}"))?;
    let mut cab_bytes = Vec::new();
    package
        .read_stream(CAB_STREAM)
        .map_err(|e| format!("flux {CAB_STREAM} absent : {e}"))?
        .read_to_end(&mut cab_bytes)
        .map_err(|e| format!("flux {CAB_STREAM} illisible : {e}"))?;
    let mut cabinet =
        cab::Cabinet::new(Cursor::new(cab_bytes)).map_err(|e| format!("archive CAB illisible : {e}"))?;
    let mut xsd = Vec::new();
    cabinet
        .read_file(CAB_FILE)
        .map_err(|e| format!("fichier {CAB_FILE} absent de l'archive : {e}"))?
        .take(XSD_MAX.saturating_add(1))
        .read_to_end(&mut xsd)
        .map_err(|e| format!("fichier {CAB_FILE} illisible : {e}"))?;
    if u64::try_from(xsd.len()).unwrap_or(u64::MAX) > XSD_MAX {
        return Err(format!("XSD de plus de {XSD_MAX} octets"));
    }
    Ok(xsd)
}

/// Écrit `bytes` dans `target` par un fichier partiel renommé.
fn write_atomically(target: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = target
        .parent()
        .ok_or_else(|| format!("{} : pas de dossier parent", target.display()))?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{} : {e}", dir.display()))?;
    let part = dir.join(".part-mspdi_pj15.xsd");
    std::fs::write(&part, bytes).map_err(|e| format!("{} : {e}", part.display()))?;
    std::fs::rename(&part, target).map_err(|e| {
        let _ = std::fs::remove_file(&part);
        format!("{} : {e}", target.display())
    })
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

/// Téléchargement par `curl` : https seulement, redirections suivies, taille
/// bornée par `--max-filesize`, 10 minutes au plus, deux nouvelles
/// tentatives.
fn download_with_curl(url: &str, destination: &Path, max_size: u64) -> Result<(), String> {
    let output = Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--max-time",
            "600",
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
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("plannercraft-xtask-schema-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn wrong_msi_is_refused_and_removed() {
        let dir = temp_dir("wrong");
        let target = dir.join("out").join("mspdi_pj15.xsd");
        let err = fetch_into(&target, &dir, &mut |url, dest, max| {
            assert_eq!(url, MSI_URL);
            assert_eq!(max, MSI_SIZE);
            std::fs::write(dest, b"pas un msi").map_err(|e| e.to_string())
        })
        .unwrap_err();
        assert!(err.contains("octets, attendu"), "{err}");
        assert!(!dir.join("Project2013SDK.msi").exists());
        assert!(!target.exists());
    }

    #[test]
    fn download_failure_is_reported() {
        let dir = temp_dir("fail");
        let target = dir.join("mspdi_pj15.xsd");
        let err = fetch_into(&target, &dir, &mut |_, _, _| Err("404".to_owned())).unwrap_err();
        assert_eq!(err, "404");
    }

    #[test]
    fn garbage_is_not_an_msi() {
        assert!(extract(b"garbage".to_vec()).is_err());
    }
}
