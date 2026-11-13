//! Sauvegarde automatique dans le navigateur (version web, lot W1,
//! ADR-0031, H-UI-198 à H-UI-201).
//!
//! Le planning ouvert est rouvert au prochain chargement de la page, dans
//! le même navigateur, modifications non enregistrées comprises. Aucun
//! octet ne quitte le navigateur.
//!
//! **Même journal que le desktop.** Le [`Recorder`] de la reprise après
//! plantage (`plannercraft_automation::recovery`) suit le document ; seul
//! l'écrivain change : [`SlotStore`] range la base (projet en JSON v1 du
//! cœur, nom, « enregistré », compteurs d'UID) et une ligne par changement
//! dans un **emplacement** clé → octets, que la version web confie à
//! `IndexedDB` (`apps/plannercraft-web`). La base JSON se relit sans fichier
//! d'origine : c'est ce qui manque au web, où un fichier téléchargé ne se
//! relit pas sans geste de l'utilisateur.
//!
//! Clés de l'emplacement :
//! - [`BASE_KEY`] : en-tête `plannercraft-autosave 1 <n>` (numéro de la
//!   première ligne valable), saut de ligne, puis la base du journal ;
//! - `ops/<n sur 20 chiffres>` : une ligne du journal (ordre des clés =
//!   ordre des lignes).
//!
//! Chaque écriture de base efface toutes les lignes **dans la même
//! transaction** ([`KvOp`] groupées) : une base et des lignes d'un autre
//! journal ne se mélangent jamais. Les écritures du navigateur sont
//! asynchrones : un échec (quota plein, stockage bloqué) arrive après
//! coup. Le premier échec arrête toute écriture ([`SlotWriter::failure`]),
//! la relecture ne garde que les lignes contiguës à partir du numéro de la
//! base ([`read_slot`]) : un trou laissé par une écriture perdue arrête le
//! rejeu, il ne saute jamais une modification.
//!
//! **Deux onglets.** Un seul onglet écrit : celui qui obtient le verrou du
//! navigateur ([`LOCK_NAME`], Web Locks, tenu tant que l'onglet vit). Un
//! autre onglet ouvert pendant ce temps démarre vide, sans sauvegarde
//! automatique, et le dit ([`StartPlan::OtherTab`]) : il ne lit ni
//! n'écrase la sauvegarde de l'autre. Sans verrou disponible (navigateur
//! ancien, page servie hors contexte sécurisé), pas de sauvegarde
//! automatique non plus ([`StartPlan::Unavailable`]).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io;
use std::rc::Rc;

use plannercraft_automation::Session;
use plannercraft_automation::recovery::{Recorder, RecoveryLog, Store};
use plannercraft_core::Moment;

/// Base de données `IndexedDB` de la sauvegarde automatique.
pub const DB_NAME: &str = "plannercraft";
/// Magasin d'objets de l'emplacement.
pub const STORE_NAME: &str = "autosave";
/// Nom du verrou du navigateur (Web Locks) tenu par l'onglet qui écrit.
pub const LOCK_NAME: &str = "plannercraft-autosave";
/// Clé de la base.
pub const BASE_KEY: &str = "base";
/// Début (inclus) des clés des lignes.
pub const OPS_START: &str = "ops/";
/// Fin (exclue) des clés des lignes : `'0'` suit `'/'`.
pub const OPS_END: &str = "ops0";
/// Mot magique de l'en-tête de la base.
const HEADER_MAGIC: &str = "plannercraft-autosave";
/// Version du format de l'emplacement (en-tête et clés).
pub const SLOT_VERSION: u32 = 1;

/// Écriture élémentaire dans l'emplacement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KvOp {
    /// Pose `value` sous `key`.
    Put {
        /// Clé.
        key: String,
        /// Octets.
        value: Vec<u8>,
    },
    /// Retire `key`.
    Delete(String),
    /// Retire les clés de `start` (inclus) à `end` (exclu).
    DeleteRange {
        /// Début inclus.
        start: String,
        /// Fin exclue.
        end: String,
    },
}

/// Écrivain de l'emplacement : `IndexedDB` sur le web, mémoire en test.
pub trait SlotWriter {
    /// Applique `ops` dans l'ordre, ensemble (une transaction) : soit toutes,
    /// soit aucune. Peut finir plus tard ; l'ordre des appels est l'ordre
    /// d'application. Après un échec, n'écrit plus rien.
    fn submit(&mut self, ops: Vec<KvOp>);
    /// Premier échec constaté (détail du navigateur), `None` tant que tout
    /// va bien.
    fn failure(&self) -> Option<String>;
}

/// Clé de la ligne `seq`.
#[must_use]
pub fn op_key(seq: u64) -> String {
    format!("{OPS_START}{seq:020}")
}

/// En-tête de la base : version et numéro de la première ligne.
fn header(first_seq: u64) -> String {
    format!("{HEADER_MAGIC} {SLOT_VERSION} {first_seq}\n")
}

/// Contenu relu d'un emplacement : octets de la base du journal et lignes
/// valables, prêts pour `recovery::replay_bytes`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotContent {
    /// Base du journal (JSON).
    pub base: Vec<u8>,
    /// Lignes contiguës à partir du numéro de la base, bout à bout.
    pub ops: Vec<u8>,
    /// Lignes présentes mais écartées (après un trou).
    pub dropped: usize,
}

/// Relit l'emplacement `entries` (toutes ses clés) : `Ok(None)` sans base ;
/// `Err` si l'en-tête est illisible ou d'une autre version. Les lignes sont
/// prises à partir du numéro de l'en-tête et s'arrêtent au premier
/// manquant.
///
/// # Errors
///
/// En-tête absent, illisible ou de version inconnue (détail en clair).
pub fn read_slot(entries: &BTreeMap<String, Vec<u8>>) -> Result<Option<SlotContent>, String> {
    let Some(value) = entries.get(BASE_KEY) else {
        return Ok(None);
    };
    let Some(newline) = value.iter().position(|&b| b == b'\n') else {
        return Err("en-tête de la sauvegarde absent".to_owned());
    };
    let (head, rest) = value.split_at(newline);
    let base = rest.get(1..).unwrap_or_default().to_vec();
    let head = std::str::from_utf8(head).map_err(|e| e.to_string())?;
    let mut parts = head.split(' ');
    if parts.next() != Some(HEADER_MAGIC) {
        return Err("en-tête de la sauvegarde inconnu".to_owned());
    }
    let version: u32 = parts
        .next()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| "version de la sauvegarde illisible".to_owned())?;
    if version != SLOT_VERSION {
        return Err(format!("sauvegarde de version {version} non prise en charge"));
    }
    let first: u64 = parts
        .next()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| "numéro de ligne de la sauvegarde illisible".to_owned())?;
    let mut ops = Vec::new();
    let mut seq = first;
    let mut taken = 0_usize;
    while let Some(line) = entries.get(&op_key(seq)) {
        ops.extend_from_slice(line);
        taken = taken.saturating_add(1);
        let Some(next) = seq.checked_add(1) else {
            break;
        };
        seq = next;
    }
    let present = entries.range(OPS_START.to_owned()..OPS_END.to_owned()).count();
    Ok(Some(SlotContent {
        base,
        ops,
        dropped: present.saturating_sub(taken),
    }))
}

/// État partagé des écrivains d'un onglet : l'écrivain et le numéro de la
/// prochaine ligne (croissant pendant toute la vie de l'onglet).
struct Shared {
    writer: Box<dyn SlotWriter>,
    next_seq: u64,
}

/// [`Store`] du journal sur l'emplacement du navigateur.
///
/// - `replace_base` : lignes effacées et base posée, une seule transaction ;
/// - `append` : une ligne, numéro suivant ;
/// - `truncate_ops` : rien (déjà fait par `replace_base`, qui le précède
///   toujours dans `Journal::checkpoint`) ;
/// - `remove` : emplacement vidé (document fermé).
///
/// Un échec déjà constaté rend une erreur (le [`Recorder`] coupe alors le
/// journal, l'édition continue).
pub struct SlotStore {
    shared: Rc<RefCell<Shared>>,
}

impl SlotStore {
    /// Échec constaté, en erreur d'entrée-sortie.
    fn check(&self) -> io::Result<()> {
        match self.shared.borrow().writer.failure() {
            Some(detail) => Err(io::Error::other(detail)),
            None => Ok(()),
        }
    }
}

impl Store for SlotStore {
    fn replace_base(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.check()?;
        let mut shared = self.shared.borrow_mut();
        let mut value = header(shared.next_seq).into_bytes();
        value.extend_from_slice(bytes);
        shared.writer.submit(vec![
            KvOp::DeleteRange {
                start: OPS_START.to_owned(),
                end: OPS_END.to_owned(),
            },
            KvOp::Put {
                key: BASE_KEY.to_owned(),
                value,
            },
        ]);
        Ok(())
    }

    fn append(&mut self, line: &[u8]) -> io::Result<()> {
        self.check()?;
        let mut shared = self.shared.borrow_mut();
        let seq = shared.next_seq;
        shared.next_seq = seq.saturating_add(1);
        shared.writer.submit(vec![KvOp::Put {
            key: op_key(seq),
            value: line.to_vec(),
        }]);
        Ok(())
    }

    fn truncate_ops(&mut self) -> io::Result<()> {
        self.check()
    }

    fn remove(self: Box<Self>) -> io::Result<()> {
        self.check()?;
        self.shared.borrow_mut().writer.submit(vec![
            KvOp::Delete(BASE_KEY.to_owned()),
            KvOp::DeleteRange {
                start: OPS_START.to_owned(),
                end: OPS_END.to_owned(),
            },
        ]);
        Ok(())
    }
}

/// [`RecoveryLog`] de la version web : le [`Recorder`] du desktop sur
/// [`SlotStore`]. Une erreur de stockage n'est signalée qu'une fois pour
/// toute la vie de l'onglet (un échec du navigateur est durable : quota
/// plein, stockage bloqué).
pub struct BrowserAutosave {
    recorder: Recorder,
    reported: bool,
}

impl std::fmt::Debug for BrowserAutosave {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrowserAutosave")
            .field("recorder", &self.recorder)
            .field("reported", &self.reported)
            .finish()
    }
}

impl BrowserAutosave {
    /// Sauvegarde automatique écrite par `writer`.
    #[must_use]
    pub fn new(writer: Box<dyn SlotWriter>) -> Self {
        let shared = Rc::new(RefCell::new(Shared { writer, next_seq: 0 }));
        let recorder = Recorder::with_factory(Box::new(move || {
            Ok(Box::new(SlotStore {
                shared: Rc::clone(&shared),
            }) as Box<dyn Store>)
        }));
        Self {
            recorder,
            reported: false,
        }
    }
}

impl RecoveryLog for BrowserAutosave {
    fn record(&mut self, document: Option<(u64, &Session)>, now: Moment) {
        self.recorder.record(document, now);
    }

    fn closed(&mut self) {
        self.recorder.closed();
    }

    fn take_issue(&mut self) -> Option<String> {
        let issue = self.recorder.take_issue()?;
        if self.reported {
            return None;
        }
        self.reported = true;
        Some(issue)
    }
}

/// Verrou du navigateur demandé au démarrage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockState {
    /// Obtenu : cet onglet est le seul à écrire.
    Acquired,
    /// Tenu par un autre onglet de ce navigateur.
    HeldElsewhere,
    /// API absente (navigateur ancien, page hors contexte sécurisé).
    Unsupported,
}

/// Démarrage de la sauvegarde automatique.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartPlan {
    /// Rouvrir le dernier planning de ce navigateur, puis le suivre.
    Restore(SlotContent),
    /// Rien à rouvrir : démarrer vide, puis suivre le prochain document.
    Empty,
    /// Sauvegarde illisible (autre version, en-tête abîmé) : démarrer vide,
    /// le dire ; elle sera remplacée par le prochain document.
    Unreadable(String),
    /// Un autre onglet tient la sauvegarde : démarrer vide, sans
    /// sauvegarde automatique, le dire.
    OtherTab,
    /// Pas de sauvegarde automatique (stockage ou verrou indisponible) :
    /// démarrer vide, le dire avec le détail.
    Unavailable(String),
}

impl StartPlan {
    /// Cet onglet tient la sauvegarde et la met à jour.
    #[must_use]
    pub const fn writes(&self) -> bool {
        matches!(self, Self::Restore(_) | Self::Empty | Self::Unreadable(_))
    }
}

/// Décide le démarrage d'après le verrou et la lecture de l'emplacement
/// (faite seulement si le verrou est obtenu ; `read` vaut alors le contenu
/// ou l'échec d'`IndexedDB`).
#[must_use]
pub fn plan_start(lock: LockState, read: Option<Result<BTreeMap<String, Vec<u8>>, String>>) -> StartPlan {
    match lock {
        LockState::HeldElsewhere => StartPlan::OtherTab,
        LockState::Unsupported => StartPlan::Unavailable("Web Locks".to_owned()),
        LockState::Acquired => match read {
            None => StartPlan::Unavailable("IndexedDB".to_owned()),
            Some(Err(detail)) => StartPlan::Unavailable(detail),
            Some(Ok(entries)) => match read_slot(&entries) {
                Ok(Some(content)) => StartPlan::Restore(content),
                Ok(None) => StartPlan::Empty,
                Err(detail) => StartPlan::Unreadable(detail),
            },
        },
    }
}

/// Écrivain en mémoire (tests, et repli documenté) : applique chaque lot
/// aussitôt ; une panne simulée ([`MemorySlot::fail`]) fait échouer les
/// lots suivants, sans rien écrire.
#[derive(Clone, Debug, Default)]
pub struct MemorySlot {
    entries: Rc<RefCell<BTreeMap<String, Vec<u8>>>>,
    failure: Rc<RefCell<Option<String>>>,
}

impl MemorySlot {
    /// Emplacement vide.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Contenu courant (copie).
    #[must_use]
    pub fn entries(&self) -> BTreeMap<String, Vec<u8>> {
        self.entries.borrow().clone()
    }

    /// Simule une panne durable du stockage (quota plein…).
    pub fn fail(&self, detail: &str) {
        *self.failure.borrow_mut() = Some(detail.to_owned());
    }

    /// Retire `key` (trou simulé).
    pub fn remove_key(&self, key: &str) {
        self.entries.borrow_mut().remove(key);
    }
}

impl SlotWriter for MemorySlot {
    fn submit(&mut self, ops: Vec<KvOp>) {
        if self.failure.borrow().is_some() {
            return;
        }
        let mut entries = self.entries.borrow_mut();
        for op in ops {
            match op {
                KvOp::Put { key, value } => {
                    entries.insert(key, value);
                }
                KvOp::Delete(key) => {
                    entries.remove(&key);
                }
                KvOp::DeleteRange { start, end } => {
                    let doomed: Vec<String> = entries.range(start..end).map(|(k, _)| k.clone()).collect();
                    for key in doomed {
                        entries.remove(&key);
                    }
                }
            }
        }
    }

    fn failure(&self) -> Option<String> {
        self.failure.borrow().clone()
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : un échec doit s'arrêter net et lisiblement"
    )]

    use std::collections::BTreeMap;

    use plannercraft_automation::recovery::{RecoveryLog, replay_bytes};
    use plannercraft_automation::text::Language;
    use plannercraft_automation::{Batch, DocOrigin, Session};
    use plannercraft_core::Moment;

    use super::{BASE_KEY, BrowserAutosave, LockState, MemorySlot, StartPlan, op_key, plan_start, read_slot};

    fn now() -> Moment {
        "2027-01-15T08:00".parse().unwrap()
    }

    fn session() -> Session {
        let bytes = crate::DEMO_MSPDI;
        let loaded = crate::document::load(bytes, now()).unwrap();
        let mut session = loaded.session;
        session.set_origin(DocOrigin {
            name: "maison.xml".to_owned(),
            path: None,
        });
        session
    }

    fn rename(session: &mut Session, name: &str) {
        let batch = Batch::from_json_str(&format!(
            r#"{{"format_version":1,"ops":[{{"op":"task.update","uid":3,"name":"{name}"}}]}}"#
        ))
        .unwrap();
        session
            .apply(&batch, plannercraft_automation::ApplyMode::Commit)
            .unwrap();
    }

    fn task3(session: &Session) -> String {
        session
            .project()
            .tasks
            .iter()
            .find(|t| t.uid() == plannercraft_core::TaskUid::new(3))
            .map(|t| t.name.clone())
            .unwrap()
    }

    /// H-UI-198 : le journal du desktop s'écrit dans l'emplacement (base,
    /// puis une ligne par modification) et se rejoue tel quel ; un état
    /// enregistré réécrit la base et efface les lignes ; fermer le document
    /// vide l'emplacement.
    #[test]
    fn h_ui_198_slot_holds_the_journal() {
        let slot = MemorySlot::new();
        let mut log = BrowserAutosave::new(Box::new(slot.clone()));
        let mut s = session();
        log.record(Some((1, &s)), now());
        let entries = slot.entries();
        assert!(entries.contains_key(BASE_KEY));
        assert_eq!(entries.len(), 1, "base seule à l'ouverture");
        rename(&mut s, "Fondations A");
        log.record(Some((1, &s)), now());
        rename(&mut s, "Fondations B");
        log.record(Some((1, &s)), now());
        let entries = slot.entries();
        assert!(entries.contains_key(&op_key(0)) && entries.contains_key(&op_key(1)));
        let content = read_slot(&entries).unwrap().unwrap();
        assert_eq!(content.dropped, 0);
        let (replayed, report) = replay_bytes(&content.base, &content.ops, Language::Fr).unwrap();
        assert_eq!(report.applied, 2);
        assert_eq!(task3(&replayed), "Fondations B");
        assert!(replayed.is_dirty());
        assert_eq!(replayed.origin().name, "maison.xml");
        // Enregistré : nouvelle base, lignes effacées, base « enregistrée ».
        s.mark_saved();
        log.record(Some((1, &s)), now());
        let entries = slot.entries();
        assert_eq!(entries.len(), 1, "lignes effacées au point de reprise");
        let content = read_slot(&entries).unwrap().unwrap();
        let (replayed, _) = replay_bytes(&content.base, &content.ops, Language::Fr).unwrap();
        assert!(!replayed.is_dirty());
        assert_eq!(task3(&replayed), "Fondations B");
        // Une ligne après le point de reprise porte le numéro suivant.
        rename(&mut s, "Fondations C");
        log.record(Some((1, &s)), now());
        let content = read_slot(&slot.entries()).unwrap().unwrap();
        let (replayed, report) = replay_bytes(&content.base, &content.ops, Language::Fr).unwrap();
        assert_eq!((report.applied, task3(&replayed).as_str()), (1, "Fondations C"));
        // Document fermé proprement : emplacement vide (page vide au retour).
        log.record(None, now());
        assert!(slot.entries().is_empty());
        assert_eq!(
            plan_start(LockState::Acquired, Some(Ok(slot.entries()))),
            StartPlan::Empty
        );
    }

    /// H-UI-198 : un nouveau document remplace la sauvegarde du précédent
    /// (base neuve, anciennes lignes effacées dans la même transaction).
    #[test]
    fn h_ui_198_new_document_replaces_slot() {
        let slot = MemorySlot::new();
        let mut log = BrowserAutosave::new(Box::new(slot.clone()));
        let mut a = session();
        log.record(Some((1, &a)), now());
        rename(&mut a, "A1");
        log.record(Some((1, &a)), now());
        let mut b = session();
        b.set_origin(DocOrigin {
            name: "b.xml".to_owned(),
            path: None,
        });
        log.record(Some((2, &b)), now());
        let entries = slot.entries();
        assert_eq!(entries.len(), 1, "anciennes lignes effacées");
        let content = read_slot(&entries).unwrap().unwrap();
        let (replayed, report) = replay_bytes(&content.base, &content.ops, Language::Fr).unwrap();
        assert_eq!(report.applied, 0);
        assert_eq!(replayed.origin().name, "b.xml");
    }

    /// H-UI-200 : un seul onglet écrit. Verrou tenu ailleurs ⇒ démarrage
    /// vide sans écriture ni lecture ; verrou absent ⇒ pas de sauvegarde ;
    /// lecture impossible ⇒ pas de sauvegarde ; sauvegarde illisible ⇒
    /// démarrage vide, écrasée au prochain document.
    #[test]
    fn h_ui_200_lock_decides_who_writes() {
        let full: BTreeMap<String, Vec<u8>> = [(BASE_KEY.to_owned(), b"x".to_vec())].into();
        assert_eq!(plan_start(LockState::HeldElsewhere, None), StartPlan::OtherTab);
        assert_eq!(
            plan_start(LockState::HeldElsewhere, Some(Ok(full.clone()))),
            StartPlan::OtherTab
        );
        assert!(!StartPlan::OtherTab.writes());
        assert!(matches!(
            plan_start(LockState::Unsupported, None),
            StartPlan::Unavailable(_)
        ));
        assert!(matches!(
            plan_start(LockState::Acquired, Some(Err("SecurityError".to_owned()))),
            StartPlan::Unavailable(d) if d == "SecurityError"
        ));
        assert!(matches!(
            plan_start(LockState::Acquired, None),
            StartPlan::Unavailable(_)
        ));
        let unreadable = plan_start(LockState::Acquired, Some(Ok(full)));
        assert!(matches!(unreadable, StartPlan::Unreadable(_)), "{unreadable:?}");
        assert!(unreadable.writes());
        let other_version: BTreeMap<String, Vec<u8>> =
            [(BASE_KEY.to_owned(), b"plannercraft-autosave 9 0\n{}".to_vec())].into();
        assert!(matches!(
            plan_start(LockState::Acquired, Some(Ok(other_version))),
            StartPlan::Unreadable(d) if d.contains('9')
        ));
        assert_eq!(
            plan_start(LockState::Acquired, Some(Ok(BTreeMap::new()))),
            StartPlan::Empty
        );
        assert!(StartPlan::Empty.writes());
    }

    /// H-UI-201 : panne du stockage. L'écriture s'arrête, l'erreur est dite
    /// une fois pour la vie de l'onglet (même après un enregistrement qui
    /// relance le journal) ; une ligne perdue arrête le rejeu sur la
    /// dernière modification sûre, jamais au-delà d'un trou.
    #[test]
    fn h_ui_201_failure_reported_once_and_gap_stops_replay() {
        let slot = MemorySlot::new();
        let mut log = BrowserAutosave::new(Box::new(slot.clone()));
        let mut s = session();
        log.record(Some((1, &s)), now());
        for name in ["N1", "N2", "N3"] {
            rename(&mut s, name);
            log.record(Some((1, &s)), now());
        }
        assert!(log.take_issue().is_none());
        // Trou : la ligne 1 est perdue ; seule la ligne 0 est rejouée.
        let mut holed = slot.entries();
        holed.remove(&op_key(1));
        let content = read_slot(&holed).unwrap().unwrap();
        assert_eq!(content.dropped, 1);
        let (replayed, report) = replay_bytes(&content.base, &content.ops, Language::Fr).unwrap();
        assert_eq!((report.applied, task3(&replayed).as_str()), (1, "N1"));
        // Panne : signalée une fois, plus rien n'est écrit.
        slot.fail("QuotaExceededError");
        let before = slot.entries();
        rename(&mut s, "N4");
        log.record(Some((1, &s)), now());
        let issue = log.take_issue().unwrap();
        assert!(issue.contains("QuotaExceededError"), "{issue}");
        rename(&mut s, "N5");
        log.record(Some((1, &s)), now());
        s.mark_saved();
        log.record(Some((1, &s)), now());
        rename(&mut s, "N6");
        log.record(Some((1, &s)), now());
        assert_eq!(log.take_issue(), None, "une seule fois");
        assert_eq!(slot.entries(), before, "rien d'écrit après la panne");
        // L'état gardé reste cohérent : N3, dernière modification écrite.
        let content = read_slot(&slot.entries()).unwrap().unwrap();
        let (replayed, _) = replay_bytes(&content.base, &content.ops, Language::Fr).unwrap();
        assert_eq!(task3(&replayed), "N3");
    }
}
