//! Reprise après plantage (J7, lot L15 ; ADR-0005 « journal write-ahead »,
//! ADR-0015 « base JSON, pas MSPDI »). Dossier sur le desktop ; sur le web,
//! le même journal (base et lignes, [`replay_bytes`]) est écrit dans un
//! emplacement du navigateur par un autre [`Store`] (lot W1, ADR-0031).
//!
//! Un document ouvert dans l'application tient un **journal** dans son
//! propre dossier, `<recovery>/<pid>-<n>/` :
//! - `base.json` : état de départ (projet en JSON v1 du cœur, plus le nom,
//!   le chemin, la langue, « maintenant », les compteurs d'UID et le drapeau
//!   « enregistré ») et son **époque** ;
//! - `ops.jsonl` : une ligne par changement depuis la base
//!   (`{"epoch":k,"entry":{…}}`), écrite puis `sync_data` ;
//! - `lock` : verrou exclusif (`File::try_lock`) tenu tant que l'application
//!   vit. Verrou libre ⇒ **orphelin** : l'application est morte sans fermer
//!   proprement le document.
//!
//! Point de reprise (`checkpoint`) : la base est réécrite (fichier
//! temporaire puis renommage) avec l'époque suivante, puis `ops.jsonl` est
//! vidé ; une ligne d'une autre époque est ignorée au rejeu, si bien qu'un
//! plantage entre le renommage et la troncature ne rejoue rien deux fois.
//! Il a lieu à chaque état enregistré, tous les [`CHECKPOINT_ENTRIES`]
//! changements ou [`CHECKPOINT_BYTES`] octets de journal, et quand un
//! changement ne s'explique pas par un seul lot, une annulation ou un
//! rétablissement que le rejeu saurait refaire (annulation au-delà de la
//! base, par exemple). L'historique antérieur au point de reprise n'est plus
//! récupérable (dit dans le bandeau).
//!
//! Le journal suit la session par [`Journal::record`] : il compare la
//! position de l'historique ([`Session::history_position`]) à ce qu'il a
//! déjà écrit et en déduit l'entrée (`Apply`, `Undo`, `Redo`). Toute écriture
//! est faite par un [`Store`] : le dossier réel ([`DirStore`]) ou un écrivain
//! simulé (tests du disque plein). Une erreur d'écriture **coupe** le journal
//! ([`Recorder`]) : l'édition n'est jamais bloquée, l'erreur est signalée
//! une fois.

use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::fs::{File, OpenOptions};
use std::io;
#[cfg(not(target_arch = "wasm32"))]
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::path::Path;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicU64, Ordering};

use plannercraft_core::{Moment, Project};
use serde::{Deserialize, Serialize};

use crate::command::Batch;
use crate::history::EditSource;
use crate::session::{ApplyMode, ApplyOptions, DocOrigin, Session};
use crate::text::Language;

/// Version du format de `base.json` et des lignes de `ops.jsonl`.
pub const FORMAT_VERSION: u32 = 1;
/// Changements journalisés avant un point de reprise.
pub const CHECKPOINT_ENTRIES: usize = 500;
/// Octets de `ops.jsonl` avant un point de reprise (8 Mio).
pub const CHECKPOINT_BYTES: u64 = 8 * 1024 * 1024;
/// Taille maximale lue d'un fichier du journal (règle 10 : un fichier
/// démesuré n'est jamais chargé).
pub const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
/// Variable d'environnement : dossier des journaux (absolu), imposé par les
/// tests.
pub const RECOVERY_DIR_ENV: &str = "PLANNERCRAFT_RECOVERY_DIR";

#[cfg(not(target_arch = "wasm32"))]
const BASE_FILE: &str = "base.json";
#[cfg(not(target_arch = "wasm32"))]
const BASE_TMP: &str = "base.json.tmp";
#[cfg(not(target_arch = "wasm32"))]
const OPS_FILE: &str = "ops.jsonl";
#[cfg(not(target_arch = "wasm32"))]
const LOCK_FILE: &str = "lock";

/// Changement journalisé.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Entry {
    /// Lot commité.
    Apply {
        /// Lot, tel qu'appliqué (entrée d'historique).
        batch: Batch,
        /// « Maintenant » au moment du lot (information : le moteur ne lit
        /// pas l'heure pendant un lot).
        now: Moment,
        /// Origine du lot.
        #[serde(default)]
        source: EditSource,
    },
    /// Annulation du dernier lot.
    Undo,
    /// Rétablissement du dernier lot annulé.
    Redo,
}

/// Ligne de `ops.jsonl`.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Line {
    epoch: u64,
    entry: Entry,
}

/// Époque d'une ligne, sans lire l'entrée.
#[derive(Deserialize)]
struct LineEpoch {
    epoch: u64,
}

/// Empreinte du fichier du document au moment de la base : taille et date
/// de modification (relecture finale J7, interface F3). À la restauration,
/// un fichier qui ne lui correspond plus a été écrit entre-temps (autre
/// instance, `plannercraft mcp --headless`…) : le document restauré ne doit pas
/// l'écraser par un simple Enregistrer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileStamp {
    /// Taille en octets.
    pub len: u64,
    /// Date de modification, en nanosecondes depuis l'époque Unix (0 si
    /// le système ne la donne pas).
    pub modified_ns: u64,
}

impl FileStamp {
    /// Empreinte du fichier `path` ; `None` s'il n'existe pas ou ne se lit
    /// pas.
    #[must_use]
    pub fn of(path: &Path) -> Option<Self> {
        let meta = fs::metadata(path).ok()?;
        if !meta.is_file() {
            return None;
        }
        let modified_ns = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX));
        Some(Self {
            len: meta.len(),
            modified_ns,
        })
    }
}

/// `base.json` (écriture, sans copie du projet).
#[derive(Serialize)]
struct BaseRef<'a> {
    v: u32,
    epoch: u64,
    name: &'a str,
    path: Option<&'a str>,
    lang: Language,
    now: Moment,
    saved: bool,
    checkpoints: u32,
    uids: [u32; 4],
    /// Empreinte du fichier `path` à la base (absente : pas de chemin, ou
    /// fichier introuvable).
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<FileStamp>,
    project: &'a Project,
}

/// `base.json` (lecture complète).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BaseFile {
    v: u32,
    epoch: u64,
    name: String,
    path: Option<String>,
    lang: Language,
    now: Moment,
    saved: bool,
    checkpoints: u32,
    uids: [u32; 4],
    #[serde(default)]
    #[allow(dead_code, reason = "lu par `BaseMeta` ; gardé pour `deny_unknown_fields`")]
    file: Option<FileStamp>,
    project: Project,
}

/// En-tête de `base.json` (le projet est sauté).
#[derive(Deserialize)]
#[cfg_attr(
    target_arch = "wasm32",
    allow(
        dead_code,
        reason = "web : seule la version est lue (`replay_bytes`), le reste sert aux orphelins du desktop"
    )
)]
struct BaseMeta {
    v: u32,
    epoch: u64,
    name: String,
    path: Option<String>,
    saved: bool,
    checkpoints: u32,
    #[serde(default)]
    file: Option<FileStamp>,
}

/// Version seule d'un `base.json` (autre format reconnu à son en-tête).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Deserialize)]
struct BaseVersion {
    v: u32,
}

/// Erreur de lecture ou de rejeu d'un journal.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RecoveryError {
    /// Lecture impossible.
    #[error("journal de reprise illisible : {0}")]
    Io(String),
    /// `base.json` invalide.
    #[error("base du journal de reprise invalide : {0}")]
    BadBase(String),
    /// Version de format inconnue.
    #[error("journal de reprise de version {0} non prise en charge")]
    Version(u32),
    /// Fichier plus grand que [`MAX_FILE_BYTES`].
    #[error("fichier du journal de reprise trop grand ({0} octets)")]
    TooLarge(u64),
}

/// Écritures d'un journal : le dossier réel ([`DirStore`]), l'emplacement
/// du navigateur (version web, lot W1 : `plannercraft_ui_egui::autosave`,
/// écritures remises à `IndexedDB`, d'où l'absence de borne `Send`) ou un
/// écrivain simulé.
pub trait Store {
    /// Remplace `base.json` de façon atomique.
    ///
    /// # Errors
    ///
    /// Toute erreur d'écriture (disque plein…).
    fn replace_base(&mut self, bytes: &[u8]) -> io::Result<()>;
    /// Ajoute une ligne (fin de ligne comprise) à `ops.jsonl`, puis
    /// `sync_data`.
    ///
    /// # Errors
    ///
    /// Toute erreur d'écriture.
    fn append(&mut self, line: &[u8]) -> io::Result<()>;
    /// Vide `ops.jsonl`.
    ///
    /// # Errors
    ///
    /// Toute erreur d'écriture.
    fn truncate_ops(&mut self) -> io::Result<()>;
    /// Supprime le journal (fermeture propre du document).
    ///
    /// # Errors
    ///
    /// Toute erreur de suppression.
    fn remove(self: Box<Self>) -> io::Result<()>;
}

/// Dossier par défaut des journaux : [`RECOVERY_DIR_ENV`] s'il est absolu,
/// sinon `recovery` à côté du dossier du canal de contrôle
/// (`state_dir()/../recovery`).
#[must_use]
#[cfg(not(target_arch = "wasm32"))]
pub fn recovery_dir() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os(RECOVERY_DIR_ENV) {
        let path = PathBuf::from(value);
        if path.is_absolute() {
            return Some(path);
        }
    }
    let control = crate::control::state_dir()?;
    Some(control.parent()?.join("recovery"))
}

/// Crée `dir` et ses parents, en 0700 sur Unix (le journal contient le
/// projet).
#[cfg(not(target_arch = "wasm32"))]
fn create_private_dir_all(dir: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        builder.mode(0o700);
    }
    builder.create(dir)
}

/// Crée `dir` (qui ne doit pas exister), en 0700 sur Unix.
#[cfg(not(target_arch = "wasm32"))]
fn create_private_dir(dir: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        builder.mode(0o700);
    }
    builder.create(dir)
}

/// Journal dans un dossier `<root>/<pid>-<n>/`, verrou tenu.
#[derive(Debug)]
#[cfg(not(target_arch = "wasm32"))]
pub struct DirStore {
    dir: PathBuf,
    ops: File,
    lock: File,
}

#[cfg(not(target_arch = "wasm32"))]
impl DirStore {
    /// Crée un dossier de journal neuf sous `root` (créé au besoin) et prend
    /// son verrou.
    ///
    /// # Errors
    ///
    /// Dossier impossible à créer, verrou indisponible.
    pub fn create(root: &Path) -> io::Result<Self> {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        create_private_dir_all(root)?;
        let pid = std::process::id();
        // Un dossier de même nom peut rester d'un processus mort de même
        // pid : on passe au numéro suivant.
        for _ in 0..1_000 {
            let n = SEQ.fetch_add(1, Ordering::Relaxed);
            let dir = root.join(format!("{pid}-{n}"));
            match create_private_dir(&dir) {
                Ok(()) => return Self::open_new(dir),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "aucun nom de journal libre",
        ))
    }

    /// Verrou d'abord (un dossier sans base est vu sans valeur), puis
    /// `ops.jsonl`.
    fn open_new(dir: PathBuf) -> io::Result<Self> {
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(dir.join(LOCK_FILE))?;
        lock.try_lock().map_err(io::Error::from)?;
        let ops = OpenOptions::new()
            .append(true)
            .create_new(true)
            .open(dir.join(OPS_FILE))?;
        Ok(Self { dir, ops, lock })
    }

    /// Dossier du journal.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Store for DirStore {
    fn replace_base(&mut self, bytes: &[u8]) -> io::Result<()> {
        let tmp = self.dir.join(BASE_TMP);
        let mut file = File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_data()?;
        drop(file);
        fs::rename(&tmp, self.dir.join(BASE_FILE))?;
        // Renommage durable (Unix) ; un dossier ne s'ouvre pas partout
        // (Windows) : sans effet alors.
        if let Ok(dir) = File::open(&self.dir) {
            let _ = dir.sync_all();
        }
        Ok(())
    }

    fn append(&mut self, line: &[u8]) -> io::Result<()> {
        self.ops.write_all(line)?;
        self.ops.sync_data()
    }

    fn truncate_ops(&mut self) -> io::Result<()> {
        self.ops.set_len(0)?;
        self.ops.sync_data()
    }

    fn remove(self: Box<Self>) -> io::Result<()> {
        let Self { dir, ops, lock } = *self;
        drop(ops);
        for name in [OPS_FILE, BASE_FILE, BASE_TMP] {
            match fs::remove_file(dir.join(name)) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        drop(lock);
        fs::remove_dir_all(&dir)
    }
}

/// Suivi de l'historique déjà journalisé : ce que le rejeu saurait annuler
/// et rétablir depuis la base.
#[derive(Clone, Debug, Default)]
struct Tracker {
    /// Position de l'historique de la session à la base.
    base: u64,
    /// Positions après chaque lot journalisé encore annulable par le rejeu.
    undo: Vec<u64>,
    /// Positions rétablissables par le rejeu (dernier = prochain).
    redo: Vec<u64>,
    /// Dernier identifiant attribué par un commit de la session
    /// ([`Session::history_last_id`]) déjà vu.
    last_id: u64,
    /// Génération de la session au dernier enregistrement.
    generation: u64,
}

impl Tracker {
    fn at(session: &Session) -> Self {
        let base = session.history_position();
        Self {
            base,
            undo: Vec::new(),
            redo: Vec::new(),
            last_id: session.history_last_id(),
            generation: session.generation(),
        }
    }

    /// Entrée qui mène du dernier état journalisé à celui de `session`, si
    /// un seul changement rejouable l'explique ; `None` sinon (point de
    /// reprise).
    fn step(&self, session: &Session, now: Moment) -> Option<Entry> {
        if session.generation() != self.generation.checked_add(1)? {
            return None;
        }
        let position = session.history_position();
        if self.redo.last() == Some(&position) {
            return Some(Entry::Redo);
        }
        let previous = match self.undo.len() {
            0 => None,
            1 => Some(self.base),
            n => self.undo.get(n.checked_sub(2)?).copied(),
        };
        if previous == Some(position) {
            return Some(Entry::Undo);
        }
        // Nouveau lot : un identifiant neuf, en haut de la pile (un lot
        // rétabli au-delà de la base n'a pas d'identifiant neuf).
        let (id, batch, source) = session.last_applied()?;
        let fresh = session.history_last_id() > self.last_id;
        (fresh && id == position && id == session.history_last_id()).then(|| Entry::Apply {
            batch: batch.clone(),
            now,
            source,
        })
    }

    /// Suit une entrée écrite.
    fn follow(&mut self, entry: &Entry, session: &Session) {
        let position = session.history_position();
        match entry {
            Entry::Apply { .. } => {
                self.undo.push(position);
                self.redo.clear();
            }
            Entry::Undo => {
                if let Some(top) = self.undo.pop() {
                    self.redo.push(top);
                }
            }
            Entry::Redo => {
                if let Some(top) = self.redo.pop() {
                    self.undo.push(top);
                }
            }
        }
        self.last_id = session.history_last_id();
        self.generation = session.generation();
    }
}

/// Journal de reprise d'un document ouvert.
pub struct Journal {
    store: Box<dyn Store>,
    epoch: u64,
    /// Entrées écrites depuis la base.
    entries: usize,
    /// Octets écrits depuis la base.
    bytes: u64,
    /// Points de reprise faits (0 : base = état d'ouverture).
    checkpoints: u32,
    /// La base est l'état enregistré du document.
    base_saved: bool,
    tracker: Tracker,
}

impl std::fmt::Debug for Journal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Journal")
            .field("epoch", &self.epoch)
            .field("entries", &self.entries)
            .field("bytes", &self.bytes)
            .field("checkpoints", &self.checkpoints)
            .finish_non_exhaustive()
    }
}

/// Octets de `base.json` pour l'état de `session`.
fn base_bytes(session: &Session, epoch: u64, checkpoints: u32) -> io::Result<Vec<u8>> {
    let origin = session.origin();
    let base = BaseRef {
        v: FORMAT_VERSION,
        epoch,
        name: &origin.name,
        path: origin.path.as_deref(),
        lang: session.lang(),
        now: session.now(),
        saved: !session.is_dirty(),
        checkpoints,
        uids: session.uid_counters(),
        file: origin.path.as_deref().and_then(|p| FileStamp::of(Path::new(p))),
        project: session.project(),
    };
    serde_json::to_vec(&base).map_err(io::Error::other)
}

impl Journal {
    /// Crée un journal neuf sous `root` (dossier `<pid>-<n>/`), base = état
    /// courant de `session`.
    ///
    /// # Errors
    ///
    /// Dossier, verrou ou base impossibles à écrire.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn create(root: &Path, session: &Session) -> io::Result<Self> {
        Self::start(Box::new(DirStore::create(root)?), session)
    }

    /// Journal sur `store`, base = état courant de `session`.
    ///
    /// # Errors
    ///
    /// Base impossible à écrire.
    pub fn start(mut store: Box<dyn Store>, session: &Session) -> io::Result<Self> {
        store.replace_base(&base_bytes(session, 0, 0)?)?;
        Ok(Self {
            store,
            epoch: 0,
            entries: 0,
            bytes: 0,
            checkpoints: 0,
            base_saved: !session.is_dirty(),
            tracker: Tracker::at(session),
        })
    }

    /// Ajoute une entrée (écrite puis `sync_data`). N'avance pas le suivi de
    /// l'historique : réservé aux appelants qui savent ce qu'ils écrivent
    /// (tests) ; l'application passe par [`Journal::record`].
    ///
    /// # Errors
    ///
    /// Toute erreur d'écriture.
    pub fn append(&mut self, entry: &Entry) -> io::Result<()> {
        let line = Line {
            epoch: self.epoch,
            entry: entry.clone(),
        };
        let mut bytes = serde_json::to_vec(&line).map_err(io::Error::other)?;
        bytes.push(b'\n');
        self.store.append(&bytes)?;
        self.entries = self.entries.saturating_add(1);
        let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        self.bytes = self.bytes.saturating_add(len);
        Ok(())
    }

    /// Point de reprise : base = état courant de `session` (époque
    /// suivante), journal vidé.
    ///
    /// # Errors
    ///
    /// Toute erreur d'écriture (la base précédente et ses lignes restent
    /// valables tant que le renommage n'a pas eu lieu).
    pub fn checkpoint(&mut self, session: &Session) -> io::Result<()> {
        let epoch = self.epoch.saturating_add(1);
        let checkpoints = self.checkpoints.saturating_add(1);
        self.store
            .replace_base(&base_bytes(session, epoch, checkpoints)?)?;
        self.epoch = epoch;
        self.checkpoints = checkpoints;
        self.entries = 0;
        self.bytes = 0;
        self.base_saved = !session.is_dirty();
        self.tracker = Tracker::at(session);
        self.store.truncate_ops()
    }

    /// Journalise l'état de `session` après un changement éventuel : rien
    /// si la génération n'a pas bougé et que l'état enregistré est déjà la
    /// base ; un point de reprise si l'état est l'état enregistré, si le
    /// changement ne s'explique pas par une seule entrée, ou au-delà de
    /// [`CHECKPOINT_ENTRIES`] et [`CHECKPOINT_BYTES`] ; sinon une entrée.
    ///
    /// # Errors
    ///
    /// Toute erreur d'écriture.
    pub fn record(&mut self, session: &Session, now: Moment) -> io::Result<()> {
        let clean = !session.is_dirty();
        if session.generation() == self.tracker.generation {
            if clean && (self.entries > 0 || !self.base_saved) {
                return self.checkpoint(session);
            }
            return Ok(());
        }
        if clean {
            return self.checkpoint(session);
        }
        let Some(entry) = self.tracker.step(session, now) else {
            return self.checkpoint(session);
        };
        self.append(&entry)?;
        self.tracker.follow(&entry, session);
        if self.entries >= CHECKPOINT_ENTRIES || self.bytes >= CHECKPOINT_BYTES {
            return self.checkpoint(session);
        }
        Ok(())
    }

    /// Entrées écrites depuis la base.
    #[must_use]
    pub const fn entries(&self) -> usize {
        self.entries
    }

    /// Époque de la base (points de reprise faits).
    #[must_use]
    pub const fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Supprime le journal (fermeture propre du document).
    ///
    /// # Errors
    ///
    /// Toute erreur de suppression.
    pub fn close(self) -> io::Result<()> {
        self.store.remove()
    }
}

/// Journal d'un processus mort (verrou libre).
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "constats indépendants lus sur le disque, pas un état à plusieurs valeurs"
)]
#[cfg(not(target_arch = "wasm32"))]
pub struct Orphan {
    /// Identifiant : nom du dossier (`<pid>-<n>`).
    pub id: String,
    /// Dossier du journal.
    pub dir: PathBuf,
    /// Nom du document.
    pub name: String,
    /// Chemin du document, s'il en avait un.
    pub path: Option<String>,
    /// Entrées de l'époque courante (modifications depuis la base).
    pub entries: usize,
    /// La base est l'état enregistré.
    pub saved: bool,
    /// Un point de reprise a eu lieu : l'historique antérieur est perdu.
    pub checkpointed: bool,
    /// Base illisible : rien n'est récupérable par cette version.
    pub unreadable: bool,
    /// Journal d'une **autre version** de format, ou base plus grande que
    /// [`MAX_FILE_BYTES`] : illisible ici mais peut-être pas ailleurs, il
    /// n'est **jamais supprimé automatiquement** (proposé, Ignorer le
    /// supprime ; relecture finale J7, relais F5).
    pub kept: bool,
    /// Le fichier du document a changé depuis la base (empreinte
    /// différente) : la restauration ne doit pas reprendre son chemin
    /// (relecture finale J7, interface F3).
    pub file_changed: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl Orphan {
    /// Le journal porte des modifications non enregistrées.
    #[must_use]
    pub const fn worth_restoring(&self) -> bool {
        !self.unreadable && (self.entries > 0 || !self.saved)
    }

    /// Le journal est à proposer au démarrage : modifications à restaurer,
    /// ou journal gardé ([`Orphan::kept`]) ; les autres sont supprimés.
    #[must_use]
    pub const fn worth_offering(&self) -> bool {
        self.worth_restoring() || self.kept
    }

    /// Supprime le journal.
    ///
    /// # Errors
    ///
    /// Toute erreur de suppression.
    pub fn discard(&self) -> io::Result<()> {
        match fs::remove_dir_all(&self.dir) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

/// Contenu d'un fichier du journal, borné par [`MAX_FILE_BYTES`].
#[cfg(not(target_arch = "wasm32"))]
fn read_bounded(path: &Path) -> Result<Vec<u8>, RecoveryError> {
    let file = File::open(path).map_err(|e| RecoveryError::Io(e.to_string()))?;
    let len = file
        .metadata()
        .map_err(|e| RecoveryError::Io(e.to_string()))?
        .len();
    if len > MAX_FILE_BYTES {
        return Err(RecoveryError::TooLarge(len));
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|e| RecoveryError::Io(e.to_string()))?;
    Ok(bytes)
}

/// Le verrou de `dir` est libre (ou absent : dossier inachevé).
#[cfg(not(target_arch = "wasm32"))]
fn lock_free(dir: &Path) -> bool {
    match OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.join(LOCK_FILE))
    {
        Ok(file) => file.try_lock().is_ok(),
        Err(e) => e.kind() == io::ErrorKind::NotFound,
    }
}

/// Entrées de l'époque `epoch` dans `ops.jsonl` (les lignes illisibles
/// comptent : le rejeu s'y arrêtera, avec un message).
#[cfg(not(target_arch = "wasm32"))]
fn count_entries(dir: &Path, epoch: u64) -> usize {
    let Ok(file) = File::open(dir.join(OPS_FILE)) else {
        return 0;
    };
    BufReader::new(file.take(MAX_FILE_BYTES))
        .split(b'\n')
        .map_while(Result::ok)
        .filter(|line| !line.is_empty())
        .filter(|line| serde_json::from_slice::<LineEpoch>(line).map_or(true, |l| l.epoch == epoch))
        .count()
}

/// Journaux orphelins de `root`, triés par identifiant : dossiers dont le
/// verrou est libre. Un dossier illisible est rendu `unreadable` (sans
/// valeur, à supprimer).
#[must_use]
#[cfg(not(target_arch = "wasm32"))]
pub fn orphans(root: &Path) -> Vec<Orphan> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut found: Vec<Orphan> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|entry| {
            let dir = entry.path();
            let id = entry.file_name().into_string().ok()?;
            lock_free(&dir).then(|| orphan_at(id, dir))
        })
        .collect();
    found.sort_by(|a, b| a.id.cmp(&b.id));
    found
}

/// Description de l'orphelin du dossier `dir`. Base d'une autre version
/// ou trop grande : gardée (`kept`), jamais supprimée sans l'accord de
/// l'utilisateur ; base absente ou JSON invalide : illisible.
#[cfg(not(target_arch = "wasm32"))]
fn orphan_at(id: String, dir: PathBuf) -> Orphan {
    let unreadable = |id: String, dir: PathBuf, name: Option<String>, kept: bool| Orphan {
        name: name.unwrap_or_else(|| id.clone()),
        id,
        dir,
        path: None,
        entries: 0,
        saved: true,
        checkpointed: false,
        unreadable: true,
        kept,
        file_changed: false,
    };
    let bytes = match read_bounded(&dir.join(BASE_FILE)) {
        Ok(bytes) => bytes,
        Err(RecoveryError::TooLarge(_)) => return unreadable(id, dir, None, true),
        Err(_) => return unreadable(id, dir, None, false),
    };
    let meta = match serde_json::from_slice::<BaseMeta>(&bytes) {
        Ok(meta) if meta.v == FORMAT_VERSION => meta,
        Ok(meta) => return unreadable(id, dir, Some(meta.name), true),
        Err(_) => {
            let foreign = serde_json::from_slice::<BaseVersion>(&bytes).is_ok_and(|b| b.v != FORMAT_VERSION);
            return unreadable(id, dir, None, foreign);
        }
    };
    let file_changed = meta
        .path
        .as_deref()
        .and_then(|p| FileStamp::of(Path::new(p)))
        .is_some_and(|now| Some(now) != meta.file);
    Orphan {
        entries: count_entries(&dir, meta.epoch),
        id,
        dir,
        name: meta.name,
        path: meta.path,
        saved: meta.saved,
        checkpointed: meta.checkpoints > 0,
        unreadable: false,
        kept: false,
        file_changed,
    }
}

/// Bilan d'un rejeu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayReport {
    /// Entrées rejouées.
    pub applied: usize,
    /// Index (depuis 0) de la première entrée en échec : le rejeu s'est
    /// arrêté juste avant.
    pub stopped_at: Option<usize>,
    /// Raison de l'arrêt, dans la langue demandée.
    pub error: Option<String>,
    /// La base est un point de reprise : l'historique antérieur est perdu.
    pub checkpointed: bool,
}

/// Rejoue un journal orphelin : session sur la base, puis chaque entrée de
/// son époque. Une entrée illisible ou refusée arrête le rejeu à l'entrée
/// précédente ([`ReplayReport::stopped_at`]).
///
/// # Errors
///
/// Base illisible ou de version inconnue.
#[cfg(not(target_arch = "wasm32"))]
pub fn replay(o: &Orphan, lang: Language) -> Result<(Session, ReplayReport), RecoveryError> {
    let base = read_bounded(&o.dir.join(BASE_FILE))?;
    let ops = match read_bounded(&o.dir.join(OPS_FILE)) {
        Ok(ops) => ops,
        Err(RecoveryError::Io(_)) => Vec::new(),
        Err(e) => return Err(e),
    };
    replay_bytes(&base, &ops, lang)
}

/// Rejeu à partir des octets de `base.json` et de `ops.jsonl` (voir
/// [`replay`]) : session non modifiée si la base était l'état enregistré
/// et qu'aucune entrée ne l'a changé, modifiée sinon.
///
/// # Errors
///
/// Base illisible ou de version inconnue.
pub fn replay_bytes(
    base: &[u8],
    ops: &[u8],
    lang: Language,
) -> Result<(Session, ReplayReport), RecoveryError> {
    let base: BaseFile = match serde_json::from_slice(base) {
        Ok(base) => base,
        Err(e) => {
            // Une autre version se reconnaît à son en-tête seul.
            return Err(match serde_json::from_slice::<BaseMeta>(base) {
                Ok(head) if head.v != FORMAT_VERSION => RecoveryError::Version(head.v),
                _ => RecoveryError::BadBase(e.to_string()),
            });
        }
    };
    if base.v != FORMAT_VERSION {
        return Err(RecoveryError::Version(base.v));
    }
    let origin = DocOrigin {
        name: base.name,
        path: base.path,
    };
    let mut session = Session::from_project(base.project, origin, base.now, base.lang);
    session.raise_uid_counters(base.uids);
    if !base.saved {
        session.mark_unsaved();
    }
    let mut report = ReplayReport {
        applied: 0,
        stopped_at: None,
        error: None,
        checkpointed: base.checkpoints > 0,
    };
    for raw in ops.split(|&b| b == b'\n').filter(|line| !line.is_empty()) {
        let result = match serde_json::from_slice::<Line>(raw) {
            Ok(line) if line.epoch != base.epoch => continue,
            Ok(line) => run(&mut session, line.entry, lang),
            Err(e) => match serde_json::from_slice::<LineEpoch>(raw) {
                Ok(l) if l.epoch != base.epoch => continue,
                _ => Err(e.to_string()),
            },
        };
        if let Err(error) = result {
            report.stopped_at = Some(report.applied);
            report.error = Some(error);
            break;
        }
        report.applied = report.applied.saturating_add(1);
    }
    Ok((session, report))
}

/// Rejoue une entrée.
fn run(session: &mut Session, entry: Entry, lang: Language) -> Result<(), String> {
    match entry {
        Entry::Apply { batch, source, .. } => {
            let options = ApplyOptions {
                detail: false,
                source,
            };
            session
                .apply_with(&batch, ApplyMode::Commit, options)
                .map(drop)
                .map_err(|e| e.message(lang))
        }
        Entry::Undo => session.undo().map(drop).map_err(|e| e.to_string()),
        Entry::Redo => session.redo().map(drop).map_err(|e| e.to_string()),
    }
}

/// Journal de reprise vu de l'application : [`Recorder`] dans un dossier
/// sur le desktop ; sur le web (lot W1, ADR-0031), le même [`Recorder`]
/// écrit dans un emplacement du navigateur (`IndexedDB`), relu au démarrage
/// suivant pour rouvrir le planning ; aucun sous test (`App` sans journal).
pub trait RecoveryLog {
    /// Le document courant a peut-être changé : `(numéro d'ouverture,
    /// session)`, ou `None` (aucun document : le précédent a été fermé
    /// proprement, son journal est supprimé). Un nouveau numéro d'ouverture
    /// remplace le journal. Jamais bloquant.
    fn record(&mut self, document: Option<(u64, &Session)>, now: Moment);
    /// Fermeture propre de l'application : journal supprimé.
    fn closed(&mut self);
    /// Erreur d'écriture à afficher (journal coupé), une fois.
    fn take_issue(&mut self) -> Option<String>;
}

/// Fabrique des écrivains d'un nouveau journal.
pub type StoreFactory = Box<dyn FnMut() -> io::Result<Box<dyn Store>>>;

/// [`RecoveryLog`] de l'application : un journal par document ouvert, coupé
/// à la première erreur d'écriture (l'édition continue), relancé quand le
/// document revient à son état enregistré.
pub struct Recorder {
    factory: StoreFactory,
    journal: Option<Journal>,
    /// Numéro d'ouverture du document suivi.
    key: Option<u64>,
    /// Journal coupé (ou jamais créé) pour ce document.
    cut: bool,
    issue: Option<String>,
}

impl std::fmt::Debug for Recorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recorder")
            .field("journal", &self.journal)
            .field("key", &self.key)
            .field("cut", &self.cut)
            .finish_non_exhaustive()
    }
}

impl Recorder {
    /// Journaux dans `root` (dossiers [`DirStore`]).
    #[must_use]
    #[cfg(not(target_arch = "wasm32"))]
    pub fn in_dir(root: PathBuf) -> Self {
        Self::with_factory(Box::new(move || {
            DirStore::create(&root).map(|s| Box::new(s) as Box<dyn Store>)
        }))
    }

    /// Journaux écrits par les écrivains de `factory` (tests).
    #[must_use]
    pub fn with_factory(factory: StoreFactory) -> Self {
        Self {
            factory,
            journal: None,
            key: None,
            cut: false,
            issue: None,
        }
    }

    /// Journal du document suivi (tests).
    #[must_use]
    pub const fn journal(&self) -> Option<&Journal> {
        self.journal.as_ref()
    }

    /// Le journal du document suivi est coupé.
    #[must_use]
    pub const fn is_cut(&self) -> bool {
        self.cut
    }

    /// Supprime le journal courant (erreur signalée).
    fn drop_journal(&mut self) {
        if let Some(journal) = self.journal.take()
            && let Err(e) = journal.close()
        {
            self.issue = Some(e.to_string());
        }
    }

    /// Nouveau journal pour `session`.
    fn start(&mut self, session: &Session) {
        match (self.factory)().and_then(|store| Journal::start(store, session)) {
            Ok(journal) => {
                self.journal = Some(journal);
                self.cut = false;
            }
            Err(e) => {
                self.cut = true;
                self.issue = Some(e.to_string());
            }
        }
    }
}

impl RecoveryLog for Recorder {
    fn record(&mut self, document: Option<(u64, &Session)>, now: Moment) {
        let Some((key, session)) = document else {
            self.drop_journal();
            self.key = None;
            self.cut = false;
            return;
        };
        if self.key != Some(key) {
            self.drop_journal();
            self.key = Some(key);
            self.start(session);
            return;
        }
        if self.cut {
            // Journal coupé : rien n'est écrit ; à l'état enregistré, le
            // journal partiel ne sert plus, un nouveau est tenté.
            if !session.is_dirty() {
                self.drop_journal();
                self.start(session);
            }
            return;
        }
        let Some(journal) = &mut self.journal else {
            return;
        };
        if let Err(e) = journal.record(session, now) {
            // Journal gardé sur le disque (préfixe récupérable après un
            // plantage), plus rien n'y est écrit.
            self.cut = true;
            self.issue = Some(e.to_string());
        }
    }

    fn closed(&mut self) {
        self.drop_journal();
        self.key = None;
        self.cut = false;
    }

    fn take_issue(&mut self) -> Option<String> {
        self.issue.take()
    }
}
