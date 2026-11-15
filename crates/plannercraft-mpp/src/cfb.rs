//! Lecture seule d'un fichier composé (Compound File Binary), écrite depuis
//! la spécification publique de Microsoft `[MS-CFB]` (ADR-0036 D2).
//!
//! Un `.mpp` est un fichier composé : un petit système de fichiers dans un
//! fichier (secteurs, table d'allocation FAT, répertoire en arbre, mini
//! flux pour les petits flux). Ce module ne fait que lire : en-tête
//! (`[MS-CFB]` § 2.2), DIFAT et FAT (§ 2.3, § 2.4), répertoire (§ 2.6),
//! mini FAT et mini flux (§ 2.5).
//!
//! Entrée hostile (règle 10) : toute valeur lue est bornée avant usage ;
//! une chaîne de secteurs qui boucle, sort du fichier ou annonce une taille
//! plus grande que ce qu'elle porte est une erreur, jamais une panique ni
//! une allocation démesurée (toute allocation est bornée par la taille de
//! l'entrée).

use std::collections::BTreeSet;

use crate::bytes::{le_u16, le_u32, le_u64};

/// Signature de l'en-tête (`[MS-CFB]` § 2.2).
const SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
/// Fin de chaîne.
const END_OF_CHAIN: u32 = 0xFFFF_FFFE;
/// Plus grand numéro de secteur ordinaire.
const MAX_REG_SECT: u32 = 0xFFFF_FFFA;
/// Pas de flux (frère ou enfant absent).
const NO_STREAM: u32 = 0xFFFF_FFFF;
/// Taille d'une entrée de répertoire.
const DIR_ENTRY_LEN: usize = 128;
/// Entrées DIFAT de l'en-tête.
const HEADER_DIFAT: usize = 109;

/// Erreur de lecture du conteneur.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CfbError {
    /// Pas la signature d'un fichier composé.
    NotCompound,
    /// En-tête incohérent (version, taille de secteur, ordre des octets).
    BadHeader,
    /// Chaîne de secteurs invalide (boucle, secteur hors du fichier, FAT
    /// trop courte).
    BadChain,
    /// Répertoire illisible.
    BadDirectory,
    /// Flux plus court que sa taille annoncée.
    Truncated,
}

/// Nature d'une entrée du répertoire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    /// Dossier (« storage »).
    Storage,
    /// Flux.
    Stream,
    /// Racine.
    Root,
}

/// Entrée du répertoire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Nom, tel qu'écrit (les dossiers d'un `.mpp` commencent par des
    /// espaces : « `   114` »).
    pub name: String,
    /// Nature.
    pub kind: EntryKind,
    left: u32,
    right: u32,
    child: u32,
    start: u32,
    size: u64,
}

/// Fichier composé lu.
#[derive(Debug)]
pub struct Cfb<'a> {
    data: &'a [u8],
    sector_size: usize,
    mini_cutoff: u64,
    fat: Vec<u32>,
    mini_fat: Vec<u32>,
    entries: Vec<Option<Entry>>,
    mini_stream: Vec<u8>,
}

impl<'a> Cfb<'a> {
    /// Lit l'en-tête, la FAT, le répertoire, la mini FAT et le mini flux.
    ///
    /// # Errors
    ///
    /// [`CfbError`] si la structure est inexploitable.
    pub fn parse(data: &'a [u8]) -> Result<Self, CfbError> {
        if data.get(..8) != Some(&SIGNATURE[..]) {
            return Err(CfbError::NotCompound);
        }
        let header = data.get(..512).ok_or(CfbError::BadHeader)?;
        let major = le_u16(header, 0x1A).ok_or(CfbError::BadHeader)?;
        let byte_order = le_u16(header, 0x1C).ok_or(CfbError::BadHeader)?;
        let shift = le_u16(header, 0x1E).ok_or(CfbError::BadHeader)?;
        let mini_shift = le_u16(header, 0x20).ok_or(CfbError::BadHeader)?;
        let sector_size = match (major, shift) {
            (3, 9) => 512_usize,
            (4, 12) => 4096_usize,
            _ => return Err(CfbError::BadHeader),
        };
        if byte_order != 0xFFFE || mini_shift != 6 {
            return Err(CfbError::BadHeader);
        }
        let n_fat = le_u32(header, 0x2C).ok_or(CfbError::BadHeader)?;
        let first_dir = le_u32(header, 0x30).ok_or(CfbError::BadHeader)?;
        let mini_cutoff = u64::from(le_u32(header, 0x38).ok_or(CfbError::BadHeader)?);
        let first_mini_fat = le_u32(header, 0x3C).ok_or(CfbError::BadHeader)?;
        let first_difat = le_u32(header, 0x44).ok_or(CfbError::BadHeader)?;
        let mut cfb = Self {
            data,
            sector_size,
            mini_cutoff,
            fat: Vec::new(),
            mini_fat: Vec::new(),
            entries: Vec::new(),
            mini_stream: Vec::new(),
        };
        let total = cfb.sector_count();
        // Secteurs de la FAT : DIFAT de l'en-tête, puis secteurs DIFAT
        // chaînés (chacun finit par le numéro du suivant). Au plus un
        // secteur de FAT par secteur du fichier.
        let n_fat = usize::try_from(n_fat).map_err(|_| CfbError::BadHeader)?;
        if n_fat > total {
            return Err(CfbError::BadHeader);
        }
        let mut fat_sectors: Vec<u32> = Vec::with_capacity(n_fat);
        for i in 0..HEADER_DIFAT {
            if fat_sectors.len() >= n_fat {
                break;
            }
            let off = i
                .checked_mul(4)
                .and_then(|o| o.checked_add(0x4C))
                .ok_or(CfbError::BadHeader)?;
            fat_sectors.push(le_u32(header, off).ok_or(CfbError::BadHeader)?);
        }
        let per_difat = sector_size.checked_div(4).unwrap_or(0);
        let mut next = first_difat;
        let mut visited = BTreeSet::new();
        while fat_sectors.len() < n_fat {
            if next > MAX_REG_SECT || !visited.insert(next) {
                return Err(CfbError::BadChain);
            }
            let sector = cfb.sector(next)?;
            for i in 0..per_difat.saturating_sub(1) {
                if fat_sectors.len() >= n_fat {
                    break;
                }
                fat_sectors.push(le_u32(sector, i.saturating_mul(4)).ok_or(CfbError::BadChain)?);
            }
            next = le_u32(sector, per_difat.saturating_sub(1).saturating_mul(4)).ok_or(CfbError::BadChain)?;
        }
        let mut fat = Vec::with_capacity(n_fat.saturating_mul(per_difat));
        for s in &fat_sectors {
            let sector = cfb.sector(*s)?;
            for i in 0..per_difat {
                fat.push(le_u32(sector, i.saturating_mul(4)).ok_or(CfbError::BadChain)?);
            }
        }
        cfb.fat = fat;
        // Répertoire.
        let dir = cfb.read_chain(first_dir, None)?;
        let mut entries = Vec::with_capacity(dir.len().checked_div(DIR_ENTRY_LEN).unwrap_or(0));
        for raw in dir.as_chunks::<DIR_ENTRY_LEN>().0 {
            entries.push(parse_entry(raw.as_slice(), major));
        }
        let root = entries
            .first()
            .and_then(Option::as_ref)
            .ok_or(CfbError::BadDirectory)?;
        if root.kind != EntryKind::Root {
            return Err(CfbError::BadDirectory);
        }
        let (root_start, root_size) = (root.start, root.size);
        cfb.entries = entries;
        // Mini FAT et mini flux (portés par la racine).
        if root_size > 0 {
            let fat_bytes = cfb.read_chain(first_mini_fat, None)?;
            cfb.mini_fat = fat_bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| u32::from_le_bytes(*c))
                .collect();
            cfb.mini_stream = cfb.read_chain(root_start, Some(root_size))?;
        }
        Ok(cfb)
    }

    /// Nombre de secteurs entiers après l'en-tête.
    fn sector_count(&self) -> usize {
        self.data
            .len()
            .checked_div(self.sector_size)
            .unwrap_or(0)
            .saturating_sub(1)
    }

    /// Octets du secteur `n`.
    fn sector(&self, n: u32) -> Result<&'a [u8], CfbError> {
        let n = usize::try_from(n).map_err(|_| CfbError::BadChain)?;
        let start = n
            .checked_add(1)
            .and_then(|k| k.checked_mul(self.sector_size))
            .ok_or(CfbError::BadChain)?;
        let end = start.checked_add(self.sector_size).ok_or(CfbError::BadChain)?;
        self.data.get(start..end).ok_or(CfbError::BadChain)
    }

    /// Suit une chaîne de la FAT depuis `start` et concatène ses secteurs ;
    /// `size` (taille annoncée) tronque le résultat et doit être couverte.
    fn read_chain(&self, start: u32, size: Option<u64>) -> Result<Vec<u8>, CfbError> {
        let cap = match size {
            Some(s) => usize::try_from(s).map_err(|_| CfbError::Truncated)?,
            None => self.data.len(),
        };
        if cap > self.data.len() {
            return Err(CfbError::Truncated);
        }
        let mut out = Vec::with_capacity(cap.min(self.data.len()));
        let mut next = start;
        // Un secteur déjà visité : chaîne bouclée (ensemble borné par la
        // longueur de la chaîne, elle-même bornée par la taille de l'entrée).
        let mut visited = BTreeSet::new();
        while next != END_OF_CHAIN && out.len() < cap {
            if next > MAX_REG_SECT || !visited.insert(next) {
                return Err(CfbError::BadChain);
            }
            let idx = usize::try_from(next).map_err(|_| CfbError::BadChain)?;
            out.extend_from_slice(self.sector(next)?);
            next = *self.fat.get(idx).ok_or(CfbError::BadChain)?;
        }
        if size.is_some() && out.len() < cap {
            return Err(CfbError::Truncated);
        }
        out.truncate(cap);
        Ok(out)
    }

    /// Suit une chaîne de la mini FAT dans le mini flux.
    fn read_mini_chain(&self, start: u32, size: u64) -> Result<Vec<u8>, CfbError> {
        let cap = usize::try_from(size).map_err(|_| CfbError::Truncated)?;
        if cap > self.mini_stream.len() {
            return Err(CfbError::Truncated);
        }
        let mut out = Vec::with_capacity(cap);
        let mut next = start;
        let mut visited = BTreeSet::new();
        while next != END_OF_CHAIN && out.len() < cap {
            if !visited.insert(next) {
                return Err(CfbError::BadChain);
            }
            let idx = usize::try_from(next).map_err(|_| CfbError::BadChain)?;
            let from = idx.checked_mul(64).ok_or(CfbError::BadChain)?;
            let to = from.checked_add(64).ok_or(CfbError::BadChain)?;
            out.extend_from_slice(self.mini_stream.get(from..to).ok_or(CfbError::BadChain)?);
            next = *self.mini_fat.get(idx).ok_or(CfbError::BadChain)?;
        }
        if out.len() < cap {
            return Err(CfbError::Truncated);
        }
        out.truncate(cap);
        Ok(out)
    }

    /// Entrée d'index `i`.
    #[must_use]
    pub fn entry(&self, i: usize) -> Option<&Entry> {
        self.entries.get(i).and_then(Option::as_ref)
    }

    /// Index des enfants directs de l'entrée `parent` (arbre rouge-noir des
    /// frères, `[MS-CFB]` § 2.6.4), parcouru sans récursion ; une entrée
    /// déjà vue (arbre bouclé) n'est pas revisitée.
    #[must_use]
    pub fn children(&self, parent: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let Some(p) = self.entry(parent) else {
            return out;
        };
        let mut seen = BTreeSet::new();
        let mut stack = vec![p.child];
        while let Some(id) = stack.pop() {
            if id == NO_STREAM {
                continue;
            }
            let Ok(i) = usize::try_from(id) else { continue };
            if !seen.insert(i) {
                continue;
            }
            if let Some(e) = self.entry(i) {
                out.push(i);
                stack.push(e.left);
                stack.push(e.right);
            }
        }
        out.sort_unstable();
        out
    }

    /// Enfant de `parent` nommé `name` (comparaison exacte).
    #[must_use]
    pub fn child(&self, parent: usize, name: &str) -> Option<usize> {
        self.children(parent)
            .into_iter()
            .find(|i| self.entry(*i).is_some_and(|e| e.name == name))
    }

    /// Entrée au chemin `path` depuis la racine.
    #[must_use]
    pub fn find(&self, path: &[&str]) -> Option<usize> {
        let mut at = 0_usize;
        for name in path {
            at = self.child(at, name)?;
        }
        Some(at)
    }

    /// Contenu du flux d'index `i`, borné à `max` octets.
    ///
    /// # Errors
    ///
    /// [`CfbError`] si la chaîne est invalide ou plus courte que la taille
    /// annoncée ; [`CfbError::Truncated`] si la taille dépasse `max`.
    pub fn read_stream(&self, i: usize, max: usize) -> Result<Vec<u8>, CfbError> {
        let e = self.entry(i).ok_or(CfbError::BadDirectory)?;
        if e.kind != EntryKind::Stream {
            return Err(CfbError::BadDirectory);
        }
        let max = u64::try_from(max).unwrap_or(u64::MAX);
        if e.size > max {
            return Err(CfbError::Truncated);
        }
        if e.size == 0 {
            return Ok(Vec::new());
        }
        if e.size < self.mini_cutoff {
            self.read_mini_chain(e.start, e.size)
        } else {
            self.read_chain(e.start, Some(e.size))
        }
    }
}

/// Entrée de répertoire (`[MS-CFB]` § 2.6.1) ; `None` si elle est libre ou
/// illisible.
fn parse_entry(raw: &[u8], major: u16) -> Option<Entry> {
    let kind = match raw.get(0x42)? {
        1 => EntryKind::Storage,
        2 => EntryKind::Stream,
        5 => EntryKind::Root,
        _ => return None,
    };
    let name_len = usize::from(le_u16(raw, 0x40)?).min(64);
    let units: Vec<u16> = raw
        .get(..name_len)?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|u| *u != 0)
        .collect();
    let name = String::from_utf16_lossy(&units);
    let size = if major == 3 {
        // Version 3 : seuls les 32 bits de poids faible comptent (§ 2.6.3).
        u64::from(le_u32(raw, 0x78)?)
    } else {
        le_u64(raw, 0x78)?
    };
    Some(Entry {
        name,
        kind,
        left: le_u32(raw, 0x44)?,
        right: le_u32(raw, 0x48)?,
        child: le_u32(raw, 0x4C)?,
        start: le_u32(raw, 0x74)?,
        size,
    })
}
