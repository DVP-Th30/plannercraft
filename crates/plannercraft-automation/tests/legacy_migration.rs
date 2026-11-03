//! Reprise sans perte de l'état laissé par une version antérieure nommée
//! `planrs` (renommage du 2026-11-03, `docs/rename.md`) : répertoire d'état
//! (rendez-vous et journaux de reprise) et fusion générique d'un dossier.

#![cfg(not(target_arch = "wasm32"))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use plannercraft_automation::control::adopt_legacy_state_dir_in;
use plannercraft_automation::files::adopt_legacy_dir;

/// Dossier temporaire propre au test, supprimé à la fin.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("pc-legacy-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[test]
fn legacy_state_dir_adopted_when_new_absent() {
    let base = TempDir::new("state");
    let old = base.path().join("planrs");
    write(&old.join("recovery").join("1-0").join("base.json"), "{\"v\":2}");
    write(&old.join("control").join("1-0.json"), "{}");

    let moved = adopt_legacy_state_dir_in(base.path()).unwrap();

    assert_eq!(moved, 2);
    let new = base.path().join("plannercraft");
    assert_eq!(
        std::fs::read_to_string(new.join("recovery").join("1-0").join("base.json")).unwrap(),
        "{\"v\":2}"
    );
    assert!(new.join("control").join("1-0.json").is_file());
    assert!(!old.exists(), "ancien dossier vidé puis supprimé");
}

#[test]
fn legacy_state_dir_never_overwrites_new_entries() {
    let base = TempDir::new("keep");
    let old = base.path().join("planrs");
    let new = base.path().join("plannercraft");
    write(&old.join("recovery").join("a.json"), "ancien");
    write(&old.join("app.ron"), "ancien app");
    write(&new.join("recovery").join("b.json"), "nouveau");

    let moved = adopt_legacy_state_dir_in(base.path()).unwrap();

    // `recovery` existe déjà : gardé tel quel, l'ancien reste en place.
    assert_eq!(moved, 1);
    assert_eq!(
        std::fs::read_to_string(new.join("app.ron")).unwrap(),
        "ancien app"
    );
    assert!(new.join("recovery").join("b.json").is_file());
    assert!(!new.join("recovery").join("a.json").exists());
    assert!(old.join("recovery").join("a.json").is_file(), "rien n'est perdu");
}

#[test]
fn legacy_dir_absent_is_a_no_op() {
    let base = TempDir::new("none");
    assert_eq!(adopt_legacy_state_dir_in(base.path()).unwrap(), 0);
    assert!(!base.path().join("plannercraft").exists(), "rien de créé");
    let same = base.path().join("x");
    write(&same.join("f"), "f");
    assert_eq!(adopt_legacy_dir(&same, &same).unwrap(), 0);
    assert!(same.join("f").is_file());
}
