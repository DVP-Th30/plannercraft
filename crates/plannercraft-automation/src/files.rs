//! Fichiers : lecture bornée, écriture atomique, confinement (hors wasm ;
//! lot L6).
//!
//! Seul module d'`automation` qui touche au système de fichiers (avec
//! `control`) ; `Workspace` passe par lui pour ouvrir, enregistrer et
//! importer.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::codes;
use crate::text::Language;

/// Erreur de fichier.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FileError {
    /// Lecture impossible.
    #[error("lecture impossible : {path} ({message})")]
    Read {
        /// Chemin.
        path: String,
        /// Cause.
        message: String,
    },
    /// Écriture impossible.
    #[error("écriture impossible : {path} ({message})")]
    Write {
        /// Chemin.
        path: String,
        /// Cause.
        message: String,
    },
    /// Fichier trop gros.
    #[error("fichier trop gros : {path} (au plus {max} octets)")]
    TooLarge {
        /// Chemin.
        path: String,
        /// Taille maximale.
        max: u64,
    },
    /// Chemin hors de la racine autorisée.
    #[error("chemin hors de la racine autorisée : {path}")]
    OutsideRoot {
        /// Chemin.
        path: String,
    },
}

impl FileError {
    /// Code stable.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Read { .. } => codes::FILE_READ,
            Self::Write { .. } => codes::FILE_WRITE,
            Self::TooLarge { .. } => codes::FILE_TOO_LARGE,
            Self::OutsideRoot { .. } => codes::FILE_OUTSIDE_ROOT,
        }
    }

    /// Message pour l'utilisateur, en français ou en anglais (la cause
    /// système reste celle du système d'exploitation).
    #[must_use]
    pub fn message(&self, lang: Language) -> String {
        match lang {
            Language::Fr => self.to_string(),
            Language::En => match self {
                Self::Read { path, message } => format!("cannot read {path} ({message})"),
                Self::Write { path, message } => format!("cannot write {path} ({message})"),
                Self::TooLarge { path, max } => format!("file too large: {path} (at most {max} bytes)"),
                Self::OutsideRoot { path } => format!("path outside the allowed root: {path}"),
            },
        }
    }

    fn read(p: &Path, e: &io::Error) -> Self {
        Self::Read {
            path: p.display().to_string(),
            message: e.to_string(),
        }
    }

    fn write(p: &Path, e: &io::Error) -> Self {
        Self::Write {
            path: p.display().to_string(),
            message: e.to_string(),
        }
    }
}

/// Lit au plus `max` octets : au-delà, [`FileError::TooLarge`] sans lire la
/// suite (la taille annoncée n'est pas crue : la lecture s'arrête à
/// `max + 1` octets).
///
/// # Errors
///
/// [`FileError::Read`] (fichier absent, illisible, dossier) ou
/// [`FileError::TooLarge`].
pub fn read_path(p: &Path, max: u64) -> Result<Vec<u8>, FileError> {
    let file = File::open(p).map_err(|e| FileError::read(p, &e))?;
    let meta = file.metadata().map_err(|e| FileError::read(p, &e))?;
    if meta.is_dir() {
        return Err(FileError::read(p, &io::Error::other("c'est un dossier")));
    }
    let too_large = || FileError::TooLarge {
        path: p.display().to_string(),
        max,
    };
    if meta.len() > max {
        return Err(too_large());
    }
    let mut bytes = Vec::new();
    file.take(max.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| FileError::read(p, &e))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max {
        return Err(too_large());
    }
    Ok(bytes)
}

/// Compteur des fichiers temporaires du processus (noms uniques).
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Écrit atomiquement : fichier temporaire du même dossier (créé, jamais
/// écrasé), écriture complète, `fsync`, renommage sur `p`, puis `fsync` du
/// dossier (au mieux). Un lecteur voit l'ancien contenu ou le nouveau,
/// jamais un mélange ; en cas d'échec, `p` est intact et le temporaire
/// retiré. Les droits d'un fichier existant sont repris (au mieux). Si `p`
/// est un lien symbolique, c'est sa cible qui est remplacée (temporaire
/// dans le dossier de la cible) : le lien reste un lien.
///
/// # Errors
///
/// [`FileError::Write`] (dossier absent, droits, disque plein, lien
/// symbolique sans cible…).
pub fn write_atomic(p: &Path, bytes: &[u8]) -> Result<(), FileError> {
    let target = link_target(p).map_err(|e| FileError::write(p, &e))?;
    write_atomic_at(target.as_deref().unwrap_or(p), bytes)
}

/// Cible d'un lien symbolique (`None` si `p` n'en est pas un, ou n'existe
/// pas encore). Un lien sans cible est une erreur : le remplacer par un
/// fichier ordinaire laisserait croire que la cible est à jour.
fn link_target(p: &Path) -> io::Result<Option<PathBuf>> {
    match fs::symlink_metadata(p) {
        Ok(meta) if meta.file_type().is_symlink() => fs::canonicalize(p).map(Some),
        _ => Ok(None),
    }
}

fn write_atomic_at(p: &Path, bytes: &[u8]) -> Result<(), FileError> {
    let Some(name) = p.file_name() else {
        return Err(FileError::write(p, &io::Error::other("nom de fichier attendu")));
    };
    let dir = match p.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let mut tmp_name = std::ffi::OsString::from(".");
    tmp_name.push(name);
    tmp_name.push(format!(".plannercraft-tmp-{}-{seq}", std::process::id()));
    let tmp = dir.join(tmp_name);
    let result = write_then_rename(&tmp, p, bytes);
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map_err(|e| FileError::write(p, &e))?;
    // Le renommage est durable une fois le dossier synchronisé ; un échec
    // ici (système de fichiers qui refuse d'ouvrir un dossier) n'annule pas
    // l'écriture faite.
    if let Ok(d) = File::open(&dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

fn write_then_rename(tmp: &Path, p: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(tmp)?;
    if let Ok(meta) = fs::metadata(p) {
        let _ = fs::set_permissions(tmp, meta.permissions());
    }
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(tmp, p)
}

/// Chemin canonique de `p`, refusé hors de `root` (MCP HTTP). Un chemin
/// relatif se lit depuis `root`. Un fichier qui n'existe pas encore
/// (enregistrement) est accepté si son dossier existe dans la racine et que
/// son nom est un nom simple. Les liens symboliques sont résolus : un lien
/// qui sort de la racine est refusé.
///
/// # Errors
///
/// [`FileError::OutsideRoot`] hors de la racine (ou chemin qui ne s'y
/// résout pas) ; [`FileError::Read`] si la racine elle-même est
/// introuvable.
pub fn confine(root: &Path, p: &Path) -> Result<PathBuf, FileError> {
    let root = fs::canonicalize(root).map_err(|e| FileError::read(root, &e))?;
    let outside = || FileError::OutsideRoot {
        path: p.display().to_string(),
    };
    let joined = if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(p)
    };
    let resolved = if let Ok(c) = fs::canonicalize(&joined) {
        c
    } else {
        // Fichier à créer : dossier existant, nom simple.
        let (Some(parent), Some(name)) = (joined.parent(), joined.file_name()) else {
            return Err(outside());
        };
        if !matches!(Path::new(name).components().next(), Some(Component::Normal(_))) {
            return Err(outside());
        }
        fs::canonicalize(parent).map_err(|_| outside())?.join(name)
    };
    if resolved.starts_with(&root) {
        Ok(resolved)
    } else {
        Err(outside())
    }
}

/// Reprend le contenu d'un dossier laissé par une version antérieure
/// (renommage `planrs` → PlannerCraft du 2026-11-03, `docs/rename.md`) :
/// chaque entrée de `old` absente de `new` y est déplacée (`fs::rename`,
/// même volume) ; une entrée déjà présente dans `new` n'est jamais écrasée
/// et reste dans `old`. `old` est supprimé s'il finit vide. Rend le nombre
/// d'entrées déplacées ; `old` absent (cas courant) : `Ok(0)`, sans rien
/// créer.
///
/// Fusion entrée par entrée, et non renommage du dossier entier : sur un
/// système de fichiers insensible à la casse (macOS par défaut), le dossier
/// de préférences d'eframe (`PlannerCraft`) et le dossier d'état
/// (`plannercraft`) sont le même dossier, si bien que `new` peut déjà
/// exister quand la seconde reprise s'exécute.
///
/// # Errors
///
/// Toute erreur de lecture de `old`, de création de `new` ou de
/// déplacement ; les entrées déplacées avant l'erreur le restent.
pub fn adopt_legacy_dir(old: &Path, new: &Path) -> io::Result<usize> {
    if !old.is_dir() || old == new {
        return Ok(0);
    }
    fs::create_dir_all(new)?;
    let mut moved: usize = 0;
    for entry in fs::read_dir(old)? {
        let entry = entry?;
        let target = new.join(entry.file_name());
        if fs::symlink_metadata(&target).is_ok() {
            continue;
        }
        fs::rename(entry.path(), &target)?;
        moved = moved.saturating_add(1);
    }
    // Échec ignoré : `old` n'est pas vide (entrée gardée) ou a disparu.
    let _ = fs::remove_dir(old);
    Ok(moved)
}
