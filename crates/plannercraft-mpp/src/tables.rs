//! Tables d'une entité (`TBkndTask`, `TBkndCons`, `TBkndCal`…) et jeu de
//! propriétés `Props`, telles qu'observées sur le corpus
//! (`docs/formats/mpp.md` § 3 à § 5). Toute longueur et tout décalage lus
//! sont bornés par les octets du flux.

use std::collections::BTreeMap;

use crate::bytes::{le_u16, le_u32};

/// En-tête de `VarMeta` (octets).
const VAR_META_HEADER: usize = 24;
/// Entrée de `VarMeta`.
const VAR_META_ENTRY: usize = 12;
/// En-tête de `FixedMeta`.
const FIXED_META_HEADER: usize = 16;
/// En-tête de `Props`.
const PROPS_HEADER: usize = 16;

/// Valeurs variables d'une entité : UID → (champ → octets). Les octets sont
/// empruntés à `Var2Data` : plusieurs entrées qui désignent la même valeur
/// ne la recopient pas (aucune amplification d'allocation, règle 10).
pub type VarValues<'a> = BTreeMap<u32, BTreeMap<u32, &'a [u8]>>;

/// Lit `VarMeta` et `Var2Data` : chaque entrée (UID, décalage, champ)
/// désigne dans `Var2Data` une valeur préfixée de sa longueur (`u32`).
/// Une entrée hors des octets est ignorée ; `max_entries` borne le nombre
/// d'entrées lues.
#[must_use]
pub fn var_values<'a>(meta: &[u8], data: &'a [u8], max_entries: usize) -> VarValues<'a> {
    let mut out: VarValues = BTreeMap::new();
    let count = le_u32(meta, 8).and_then(|n| usize::try_from(n).ok()).unwrap_or(0);
    let room = meta.len().saturating_sub(VAR_META_HEADER) / VAR_META_ENTRY;
    for k in 0..count.min(room).min(max_entries) {
        let base = VAR_META_HEADER.saturating_add(k.saturating_mul(VAR_META_ENTRY));
        let (Some(uid), Some(off), Some(field)) = (
            le_u32(meta, base),
            le_u32(meta, base.saturating_add(4)),
            le_u32(meta, base.saturating_add(8)),
        ) else {
            continue;
        };
        let Ok(off) = usize::try_from(off) else { continue };
        let Some(len) = le_u32(data, off).and_then(|l| usize::try_from(l).ok()) else {
            continue;
        };
        let Some(value) = off
            .checked_add(4)
            .and_then(|s| Some((s, s.checked_add(len)?)))
            .and_then(|(s, e)| data.get(s..e))
        else {
            continue;
        };
        out.entry(uid).or_default().insert(field, value);
    }
    out
}

/// Élément de `FixedMeta` et l'enregistrement qu'il désigne.
#[derive(Clone, Copy, Debug)]
pub struct FixedItem<'a> {
    /// Octets de l'élément de méta (indicateurs à l'octet 0, décalage à
    /// l'octet 4).
    pub meta: &'a [u8],
    /// Indicateurs (`u16` à l'octet 0) : bit 1 = enregistrement supprimé,
    /// bit 2 = enregistrement court (16 octets : ligne vide ou réservé).
    pub flags: u16,
    /// Octets de l'enregistrement, de son décalage à la fin de `FixedData`
    /// (l'appelant en prend la longueur de sa disposition).
    pub data: &'a [u8],
    /// Décalage de l'enregistrement dans `FixedData`.
    pub offset: usize,
    /// Un emplacement plus loin désigne le même enregistrement : celui-ci
    /// est une copie périmée (le dernier emplacement fait foi, R-284).
    pub superseded: bool,
}

impl FixedItem<'_> {
    /// Élément à ignorer : enregistrement supprimé (bit 1 des indicateurs)
    /// ou copie périmée (`superseded`).
    #[must_use]
    pub const fn deleted(&self) -> bool {
        self.flags & 0x2 != 0 || self.superseded
    }

    /// Enregistrement court (bit 2 des indicateurs).
    #[must_use]
    pub const fn short(&self) -> bool {
        self.flags & 0x4 != 0
    }
}

/// Taille d'un élément de `FixedMeta`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemLen {
    /// Taille constante observée pour cette table (`TBkndTask` : 47 ;
    /// `TBkndCons`, `TBkndCal` : 10 ; `TBkndRsc` : 37 ; `TBkndAssn` : 34).
    Known(usize),
    /// Table seconde (`Fixed2Meta`) : autant d'emplacements que la table
    /// fixe de même entité, dont on donne le nombre.
    Slots(usize),
}

/// Taille d'un élément et nombre d'emplacements du flux de méta.
///
/// Observé (`docs/formats/mpp.md` § 4) : le nombre lu à l'octet 8 de
/// l'en-tête est le nombre d'éléments en service ; le flux peut porter des
/// emplacements de plus (copies d'éléments plus anciens). Taille : la valeur
/// connue si elle divise le corps du flux, sinon (corps / nombre) quand il
/// tombe juste ; pour une table seconde, (corps / emplacements de la table
/// fixe) d'abord.
#[must_use]
pub fn item_geometry(meta: &[u8], len: ItemLen) -> Option<(usize, usize)> {
    let count = le_u32(meta, 8).and_then(|n| usize::try_from(n).ok()).unwrap_or(0);
    let body = meta.len().saturating_sub(FIXED_META_HEADER);
    let exact = |size: usize| (size >= 8 && body.checked_rem(size) == Some(0)).then_some(size);
    let by_count = body.checked_div(count).and_then(exact);
    let size = match len {
        ItemLen::Known(k) => exact(k).or(by_count),
        ItemLen::Slots(n) => body.checked_div(n).and_then(exact).or(by_count),
    }?;
    Some((size, body.checked_div(size).unwrap_or(0)))
}

/// Lit `FixedMeta` : en-tête de 16 octets (nombre d'éléments en service à
/// l'octet 8), puis des éléments de taille constante ([`item_geometry`]) ;
/// seuls les `nombre` premiers emplacements sont rendus (les suivants sont
/// des restes d'enregistrements antérieurs, observé `docs/formats/mpp.md`
/// § 4), chacun marqué `superseded` si un emplacement plus loin désigne le
/// même enregistrement.
/// `max_items` borne le nombre d'éléments rendus.
#[must_use]
pub fn fixed_items<'a>(meta: &'a [u8], data: &'a [u8], max_items: usize, len: ItemLen) -> Vec<FixedItem<'a>> {
    let mut out: Vec<FixedItem<'a>> = Vec::new();
    let count = le_u32(meta, 8).and_then(|n| usize::try_from(n).ok()).unwrap_or(0);
    let Some((item_len, slots)) = item_geometry(meta, len) else {
        return out;
    };
    for k in 0..count.min(slots).min(max_items) {
        let start = FIXED_META_HEADER.saturating_add(k.saturating_mul(item_len));
        let Some(item) = start.checked_add(item_len).and_then(|end| meta.get(start..end)) else {
            break;
        };
        let (Some(flags), Some(off)) = (le_u16(item, 0), le_u32(item, 4)) else {
            continue;
        };
        let Some((offset, record)) = usize::try_from(off).ok().and_then(|o| Some((o, data.get(o..)?))) else {
            continue;
        };
        out.push(FixedItem {
            meta: item,
            flags,
            data: record,
            offset,
            superseded: false,
        });
    }
    // Le dernier emplacement de chaque enregistrement fait foi.
    let mut last: BTreeMap<usize, usize> = BTreeMap::new();
    for (k, item) in out.iter().enumerate() {
        last.insert(item.offset, k);
    }
    for (k, item) in out.iter_mut().enumerate() {
        item.superseded = last.get(&item.offset) != Some(&k);
    }
    out
}

/// Lit `Props` : en-tête de 16 octets, puis des entrées
/// `[u32 taille][u32 clé][u32 ?][taille octets]`, alignées sur 2 octets. La
/// première clé gagne.
#[must_use]
pub fn props(data: &[u8], max_entries: usize) -> BTreeMap<u32, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut off = PROPS_HEADER;
    for _ in 0..max_entries {
        let (Some(size), Some(key)) = (le_u32(data, off), le_u32(data, off.saturating_add(4))) else {
            break;
        };
        let Ok(size) = usize::try_from(size) else { break };
        let Some(start) = off.checked_add(12) else { break };
        let Some(end) = start.checked_add(size) else { break };
        let Some(value) = data.get(start..end) else { break };
        out.entry(key).or_insert_with(|| value.to_vec());
        // Une valeur de longueur impaire est suivie d'un octet de bourrage
        // (observé : `docs/formats/mpp.md` § 5).
        off = end.saturating_add(size & 1);
    }
    out
}
